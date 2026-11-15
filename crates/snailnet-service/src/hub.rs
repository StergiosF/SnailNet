//! What the runner and the IPC server share (PLAN.md §12): the config and
//! its revision, the service state, the latest stats, and the apps seen.
//!
//! Changes from clients go through [`Hub::change`]: check `rev`, apply,
//! validate, save `config.toml` (atomically, old file → `.bak`), bump
//! `rev`, then ask the runner to reload and tell every client.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use snailnet_core::classify::family::UNKNOWN_ID;
use snailnet_core::rules::{Config, Line, Plan, Rule};
use snailnet_engine::fastpath::{DirectionStats, NO_TAG, Snapshot as EngineSnapshot, Tag};
use snailnet_ipc::{
    AppInfo, ClassTick, DirStats, ErrorCode, Event, FamilyTick, ServiceState, Snapshot, StatsTick,
};
use tokio::sync::broadcast;

use crate::paths::DataDir;
use crate::runner::StopSignal;
use crate::store;

/// A refused change: the code and a message for the user.
pub type Refusal = (ErrorCode, String);

pub struct Hub {
    dir: DataDir,
    signal: StopSignal,
    started: Instant,
    events: broadcast::Sender<Event>,
    inner: Mutex<Inner>,
}

struct Inner {
    config: Config,
    /// 0 until the runner has loaded the config; then 1, 2, … per change.
    rev: u64,
    state: ServiceState,
    last: Option<Sample>,
    tick: Option<StatsTick>,
    apps: Vec<AppInfo>,
}

struct Sample {
    at: Instant,
    down: Option<DirectionStats>,
    up: Option<DirectionStats>,
}

impl Hub {
    pub fn new(dir: DataDir, signal: StopSignal) -> Arc<Self> {
        Arc::new(Self {
            dir,
            signal,
            started: Instant::now(),
            events: broadcast::channel(64).0,
            inner: Mutex::new(Inner {
                config: Config::default(),
                rev: 0,
                state: ServiceState::Starting,
                last: None,
                tick: None,
                apps: Vec::new(),
            }),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn dir(&self) -> &DataDir {
        &self.dir
    }

    pub fn signal(&self) -> &StopSignal {
        &self.signal
    }

    /// Stats and state changes, for one client.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    /// The runner loaded the config from disk.
    pub(crate) fn set_loaded(&self, config: Config) {
        let rev = {
            let mut i = self.lock();
            i.config = config;
            i.rev += 1;
            i.rev
        };
        let _ = self.events.send(Event::ConfigChanged { rev });
    }

    /// The active plan and the active profile's name.
    pub fn plan(&self) -> (Plan, String) {
        let i = self.lock();
        (i.config.plan(), i.config.active().name.clone())
    }

    pub fn rev(&self) -> u64 {
        self.lock().rev
    }

    pub fn state(&self) -> ServiceState {
        self.lock().state.clone()
    }

    pub(crate) fn set_state(&self, state: ServiceState) {
        let changed = {
            let mut i = self.lock();
            let changed = i.state != state;
            i.state = state.clone();
            changed
        };
        if changed {
            let _ = self.events.send(Event::StateChanged(state));
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let i = self.lock();
        Snapshot {
            rev: i.rev,
            state: i.state.clone(),
            config: i.config.clone(),
            apps: i.apps.clone(),
        }
    }

    /// The newest stats, or an empty tick when not shaping.
    pub fn latest_tick(&self) -> StatsTick {
        let i = self.lock();
        match &i.tick {
            Some(t) => StatsTick {
                state: i.state.clone(),
                ..t.clone()
            },
            None => StatsTick {
                t_ms: self.started.elapsed().as_millis() as u64,
                state: i.state.clone(),
                down: None,
                up: None,
            },
        }
    }

    /// The runner's latest numbers. Rates are between the times the engine
    /// took the snapshots; a snapshot already seen changes nothing.
    pub(crate) fn publish(&self, snap: &EngineSnapshot, apps: Option<Vec<AppInfo>>) {
        let now = Instant::now();
        let t_ms = now.duration_since(self.started).as_millis() as u64;
        let at = snap.taken.unwrap_or(now);
        let mut i = self.lock();
        if let Some(apps) = apps {
            i.apps = apps;
        }
        if i.last.as_ref().is_some_and(|l| at <= l.at) {
            return;
        }
        let tick = i.last.as_ref().map(|last| {
            let secs = at.duration_since(last.at).as_secs_f64().max(1e-3);
            StatsTick {
                t_ms,
                state: i.state.clone(),
                down: dir_stats(snap.down.as_ref(), last.down.as_ref(), secs, &i.apps),
                up: dir_stats(snap.up.as_ref(), last.up.as_ref(), secs, &i.apps),
            }
        });
        if tick.is_some() {
            i.tick = tick;
        }
        i.last = Some(Sample {
            at,
            down: snap.down.clone(),
            up: snap.up.clone(),
        });
    }

    /// Shaping stopped: its counters are gone, and so are its rates.
    pub(crate) fn clear_stats(&self) {
        let mut i = self.lock();
        i.last = None;
        i.tick = None;
    }

    pub fn set_rules(&self, profile_id: &str, rules: Vec<Rule>, rev: u64) -> Result<u64, Refusal> {
        self.change(rev, |c| {
            let p = c
                .profiles
                .iter_mut()
                .find(|p| p.id == profile_id)
                .ok_or_else(|| format!("there is no profile {profile_id:?}"))?;
            p.rules = rules;
            Ok(())
        })
    }

    pub fn switch_profile(&self, id: &str, rev: u64) -> Result<u64, Refusal> {
        self.change(rev, |c| {
            if !c.profiles.iter().any(|p| p.id == id) {
                return Err(format!("there is no profile {id:?}"));
            }
            c.active_profile = id.into();
            Ok(())
        })
    }

    pub fn set_line(&self, line: Line, rev: u64) -> Result<u64, Refusal> {
        self.change(rev, |c| {
            c.line = line;
            Ok(())
        })
    }

    /// Apply `f` to a copy of the config at `rev`; validate, save, publish.
    pub fn change(
        &self,
        rev: u64,
        f: impl FnOnce(&mut Config) -> Result<(), String>,
    ) -> Result<u64, Refusal> {
        let new_rev = {
            let mut i = self.lock();
            if i.rev == 0 {
                return Err((
                    ErrorCode::Internal,
                    "the service is still starting; try again".into(),
                ));
            }
            if rev != i.rev {
                return Err((
                    ErrorCode::Conflict,
                    format!(
                        "the settings changed (now revision {}, you had {rev}); fetch them and try again",
                        i.rev
                    ),
                ));
            }
            let mut c = i.config.clone();
            f(&mut c).map_err(|m| (ErrorCode::Invalid, m))?;
            c.validate()
                .map_err(|e| (ErrorCode::Invalid, e.to_string()))?;
            if c == i.config {
                return Ok(i.rev);
            }
            store::save(&self.dir, &c).map_err(|e| {
                (
                    ErrorCode::Internal,
                    format!("could not save config.toml: {e}"),
                )
            })?;
            i.config = c;
            i.rev += 1;
            i.rev
        };
        tracing::info!("config changed by a client; now revision {new_rev}");
        self.signal.request_reload();
        let _ = self.events.send(Event::ConfigChanged { rev: new_rev });
        Ok(new_rev)
    }
}

/// `apps` is indexed by the engine's tags (app ids, in the order first
/// seen); it is read after the snapshot, so it covers every tag in it.
fn dir_stats(
    now: Option<&DirectionStats>,
    last: Option<&DirectionStats>,
    secs: f64,
    apps: &[AppInfo],
) -> Option<DirStats> {
    let now = now?;
    let classes = now
        .classes
        .iter()
        .enumerate()
        .map(|(k, (name, s))| {
            // Same class only if the name matches: a reload can reorder them.
            let prev = last
                .and_then(|l| l.classes.get(k))
                .filter(|(n, _)| n == name)
                .map(|(_, p)| (p.sent_bytes, p.dropped_packets))
                .unwrap_or((s.sent_bytes, s.dropped_packets));
            ClassTick {
                name: name.clone(),
                bps: (s.sent_bytes.saturating_sub(prev.0) as f64 * 8.0 / secs) as u64,
                drops: s.dropped_packets.saturating_sub(prev.1),
                queued_bytes: s.queued_bytes,
            }
        })
        .collect();
    Some(DirStats {
        shaped_bps: now.rate_bytes_per_sec * 8,
        classes,
        families: family_ticks(&now.tags, last.map_or(&[][..], |l| &l.tags), secs, apps),
    })
}

/// Bytes per tag since `last` (both sorted by tag), summed per family.
fn family_ticks(
    now: &[(Tag, u64)],
    last: &[(Tag, u64)],
    secs: f64,
    apps: &[AppInfo],
) -> Vec<FamilyTick> {
    let mut by_family: HashMap<(&str, &str), u64> = HashMap::new();
    for &(tag, bytes) in now {
        let before = last
            .binary_search_by_key(&tag, |&(t, _)| t)
            .map_or(0, |k| last[k].1);
        let delta = bytes.saturating_sub(before);
        if delta == 0 {
            continue;
        }
        let key = match apps.get(tag as usize) {
            Some(a) if tag != NO_TAG => (a.family.as_str(), a.name.as_str()),
            _ => (UNKNOWN_ID, "Unknown"),
        };
        *by_family.entry(key).or_default() += delta;
    }
    let mut out: Vec<FamilyTick> = by_family
        .into_iter()
        .map(|((family, name), bytes)| FamilyTick {
            family: family.to_owned(),
            name: name.to_owned(),
            bps: (bytes as f64 * 8.0 / secs) as u64,
        })
        .collect();
    out.sort_by(|a, b| b.bps.cmp(&a.bps).then_with(|| a.family.cmp(&b.family)));
    out
}

#[cfg(test)]
mod tests {
    use snailnet_core::rules::Target;
    use snailnet_core::scheduler::ClassStats;

    use super::*;

    fn hub() -> (tempfile::TempDir, Arc<Hub>) {
        let t = tempfile::tempdir().unwrap();
        let d = DataDir::new(t.path());
        d.create().unwrap();
        let hub = Hub::new(d, StopSignal::new());
        hub.set_loaded(Config::default());
        (t, hub)
    }

    fn rule(id: &str, share: u32) -> Rule {
        Rule {
            target: Target::family(id),
            share,
            priority: Default::default(),
            ceiling: None,
            enabled: true,
        }
    }

    #[test]
    fn changes_need_the_current_rev_and_are_saved() {
        let (_t, hub) = hub();
        let mut events = hub.subscribe();
        assert_eq!(hub.rev(), 1);
        let rev = hub
            .set_rules("balanced", vec![rule("steam", 4000)], 1)
            .unwrap();
        assert_eq!(rev, 2);
        assert!(hub.signal().take_reload());
        assert_eq!(events.try_recv().unwrap(), Event::ConfigChanged { rev: 2 });
        let saved = store::load(hub.dir()).unwrap().config;
        assert_eq!(saved.profiles[0].rules, vec![rule("steam", 4000)]);

        // A client still on rev 1 is refused, and nothing changes.
        let (code, _) = hub.set_line(Line::default(), 1).unwrap_err();
        assert_eq!(code, ErrorCode::Conflict);
        assert_eq!(hub.rev(), 2);
        assert!(!hub.signal().take_reload());
    }

    #[test]
    fn invalid_changes_are_refused_and_not_saved() {
        let (_t, hub) = hub();
        let r = hub.set_rules("balanced", vec![rule("a", 6000), rule("b", 6000)], 1);
        assert_eq!(r.unwrap_err().0, ErrorCode::Invalid);
        assert_eq!(
            hub.switch_profile("nope", 1).unwrap_err().0,
            ErrorCode::Invalid
        );
        let line = Line {
            down_mbps: Some(0.0),
            ..Line::default()
        };
        assert_eq!(hub.set_line(line, 1).unwrap_err().0, ErrorCode::Invalid);
        assert_eq!(hub.rev(), 1);
        assert!(!hub.dir().config().exists());
    }

    #[test]
    fn a_change_that_changes_nothing_keeps_the_rev() {
        let (_t, hub) = hub();
        assert_eq!(hub.set_line(Line::default(), 1).unwrap(), 1);
        assert!(!hub.signal().take_reload());
    }

    #[test]
    fn nothing_changes_before_the_config_is_loaded() {
        let t = tempfile::tempdir().unwrap();
        let hub = Hub::new(DataDir::new(t.path()), StopSignal::new());
        assert_eq!(
            hub.set_line(Line::default(), 0).unwrap_err().0,
            ErrorCode::Internal
        );
    }

    #[test]
    fn stats_are_rates_between_samples() {
        let (_t, hub) = hub();
        hub.set_state(ServiceState::Shaping);
        let base = Instant::now();
        let snap = |ms: u64, sent: u64, drops: u64| EngineSnapshot {
            taken: Some(base + std::time::Duration::from_millis(ms)),
            down: Some(DirectionStats {
                rate_bytes_per_sec: 1_000_000,
                classes: vec![(
                    "everything else".into(),
                    ClassStats {
                        sent_bytes: sent,
                        dropped_packets: drops,
                        queued_bytes: 77,
                        ..ClassStats::default()
                    },
                )],
                tags: vec![],
            }),
            up: None,
        };
        hub.publish(&snap(0, 1_000, 1), None);
        assert!(hub.latest_tick().down.is_none(), "one sample has no rate");
        // 100 kB over 100 ms of the engine's clock = 8 Mbps exactly, however
        // late the runner gets round to reading it.
        std::thread::sleep(std::time::Duration::from_millis(30));
        hub.publish(&snap(100, 101_000, 4), Some(vec![]));
        let t = hub.latest_tick();
        assert_eq!(t.state, ServiceState::Shaping);
        let d = t.down.unwrap();
        assert_eq!(d.shaped_bps, 8_000_000);
        let c = &d.classes[0];
        assert_eq!((c.drops, c.queued_bytes, c.bps), (3, 77, 8_000_000));
        // The same snapshot read again (the engine hasn't published since)
        // keeps the last rate instead of computing 0 over a short gap.
        hub.publish(&snap(100, 101_000, 4), None);
        assert_eq!(hub.latest_tick().down.unwrap().classes[0].bps, 8_000_000);
        hub.clear_stats();
        assert!(hub.latest_tick().down.is_none());
    }

    #[test]
    fn per_family_rates_merge_a_familys_exes_and_skip_idle_ones() {
        let (_t, hub) = hub();
        let app = |family: &str, name: &str, exe: &str| AppInfo {
            family: family.into(),
            name: name.into(),
            exe: exe.into(),
        };
        // Tags are indexes into this list.
        let apps = vec![
            app("steam", "Steam", "steam.exe"),
            app("steam", "Steam", "steamwebhelper.exe"),
            app("brave", "Brave", "brave.exe"),
            app("discord", "Discord", "discord.exe"),
        ];
        let base = Instant::now();
        let snap = |ms: u64, tags: Vec<(Tag, u64)>| EngineSnapshot {
            taken: Some(base + std::time::Duration::from_millis(ms)),
            down: Some(DirectionStats {
                rate_bytes_per_sec: 1_000_000,
                classes: vec![],
                tags,
            }),
            up: None,
        };
        hub.publish(&snap(0, vec![(0, 10_000), (3, 500)]), Some(apps.clone()));
        // Over 100 ms: steam 50 + 25 kB, brave (new) 100 kB, discord idle,
        // unknown 1 kB.
        hub.publish(
            &snap(
                100,
                vec![
                    (0, 60_000),
                    (1, 25_000),
                    (2, 100_000),
                    (3, 500),
                    (NO_TAG, 1_000),
                ],
            ),
            Some(apps),
        );
        let fam: Vec<(String, String, u64)> = hub
            .latest_tick()
            .down
            .unwrap()
            .families
            .into_iter()
            .map(|f| (f.family, f.name, f.bps))
            .collect();
        let row = |f: &str, n: &str, bps: u64| (f.to_owned(), n.to_owned(), bps);
        assert_eq!(
            fam,
            vec![
                row("brave", "Brave", 8_000_000),
                row("steam", "Steam", 6_000_000),
                row(UNKNOWN_ID, "Unknown", 80_000),
            ]
        );
    }
}
