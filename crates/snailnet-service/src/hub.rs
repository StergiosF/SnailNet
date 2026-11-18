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
use snailnet_core::modes;
use snailnet_core::networks::Switch;
use snailnet_core::rules::{
    Boost, Config, Line, Measured, Network, Pause, Plan, Rule, Settings, Target,
};
use snailnet_core::sites::SitePack;
use snailnet_engine::fastpath::{DirectionStats, Snapshot as EngineSnapshot, Tag};
use snailnet_ipc::{
    AppInfo, ClassTick, DirStats, ErrorCode, Event, FamilyTick, NameTick, ServiceState, SiteInfo,
    SiteTick, Snapshot, StatsTick,
};
use tokio::sync::broadcast;

use crate::focus::{Focus, Proc};
use crate::paths::DataDir;
use crate::runner::StopSignal;
use crate::store;
use crate::tags::{self, Site};

/// A refused change: the code and a message for the user.
pub type Refusal = (ErrorCode, String);

/// What the running shaper knows besides the engine's counters.
#[derive(Debug, Clone, Default)]
pub struct Seen {
    /// Indexed by app id (the app part of a tag, [`tags`]).
    pub apps: Vec<AppInfo>,
    /// (id, name) indexed by site slot ([`tags::Site::Slot`]).
    pub sites: Vec<(String, String)>,
    /// Naming counters so far (they only grow while shaping runs).
    pub names: Option<NameTick>,
}

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
    sites: Vec<(String, String)>,
    /// A speed test was asked for and hasn't started yet.
    speedtest: bool,
    /// What goes first now (§11.5).
    focus: Focus,
}

struct Sample {
    at: Instant,
    down: Option<DirectionStats>,
    up: Option<DirectionStats>,
    names: Option<NameTick>,
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
                sites: Vec::new(),
                speedtest: false,
                focus: Focus::default(),
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

    /// The plan at `now` (Unix seconds) and the active profile's name.
    pub fn plan(&self, now: u64) -> (Plan, String) {
        self.plan_live(now, [None, None])
    }

    /// [`Hub::plan`] at the estimator's live rates (`[down, up]`, bps;
    /// `None` keeps the typed speed).
    pub fn plan_live(&self, now: u64, live: [Option<f64>; 2]) -> (Plan, String) {
        let i = self.lock();
        let mut plan = i.config.plan_live(now, live);
        if i.config.settings.focus_boost && plan.shapes_anything() {
            plan.focus = i.focus.pids();
        }
        (plan, i.config.active().name.clone())
    }

    /// The config as it is now.
    pub fn config(&self) -> Config {
        self.lock().config.clone()
    }

    /// What each direction moved in the latest stats tick, bps (`[down, up]`;
    /// `None` where that direction isn't shaped or there's no tick yet).
    pub fn moved_bps(&self) -> [Option<f64>; 2] {
        let i = self.lock();
        let sum = |d: Option<&snailnet_ipc::DirStats>| {
            d.map(|d| d.classes.iter().map(|c| c.bps as f64).sum::<f64>())
        };
        match &i.tick {
            Some(t) => [sum(t.down.as_ref()), sum(t.up.as_ref())],
            None => [None, None],
        }
    }

    /// The pause, if it's on at `now`.
    pub fn pause_at(&self, now: u64) -> Option<Pause> {
        self.lock().config.pause_at(now)
    }

    /// End a boost or a pause whose time is up, so the file and every
    /// client see it gone. Returns whether one ended.
    pub fn end_expired(&self, now: u64) -> bool {
        let (boost, pause) = {
            let i = self.lock();
            if i.rev == 0 {
                return false;
            }
            (
                i.config.boost.as_ref().is_some_and(|b| !b.is_on(now)),
                i.config.pause.is_some_and(|p| !p.is_on(now)),
            )
        };
        if !boost && !pause {
            return false;
        }
        let rev = self.rev();
        match self.change(rev, |c| {
            if boost {
                c.boost = None;
            }
            if pause {
                c.pause = None;
            }
            Ok(())
        }) {
            Ok(_) => {
                if boost {
                    tracing::info!("the boost's time is up");
                }
                if pause {
                    tracing::info!("the pause is over");
                }
                true
            }
            // A client changed something at the same moment: next time.
            Err(_) => false,
        }
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
            sites: known_sites(&i.config),
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
                names: None,
            },
        }
    }

    /// The runner's latest numbers. Rates are between the times the engine
    /// took the snapshots; a snapshot already seen changes nothing.
    pub(crate) fn publish(&self, snap: &EngineSnapshot, seen: Option<Seen>) {
        let now = Instant::now();
        let t_ms = now.duration_since(self.started).as_millis() as u64;
        let at = snap.taken.unwrap_or(now);
        let mut i = self.lock();
        let mut names = None;
        if let Some(seen) = seen {
            i.apps = seen.apps;
            i.sites = seen.sites;
            names = seen.names;
        }
        if i.last.as_ref().is_some_and(|l| at <= l.at) {
            return;
        }
        let tick = i.last.as_ref().map(|last| {
            let secs = at.duration_since(last.at).as_secs_f64().max(1e-3);
            let known = Known {
                apps: &i.apps,
                sites: &i.sites,
            };
            StatsTick {
                t_ms,
                state: i.state.clone(),
                down: dir_stats(snap.down.as_ref(), last.down.as_ref(), secs, known),
                up: dir_stats(snap.up.as_ref(), last.up.as_ref(), secs, known),
                names: names.zip(last.names).map(|(n, l)| n.since(&l)),
            }
        });
        if tick.is_some() {
            i.tick = tick;
        }
        i.last = Some(Sample {
            at,
            down: snap.down.clone(),
            up: snap.up.clone(),
            names,
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

    pub fn reset_profile(&self, id: &str, rev: u64) -> Result<u64, Refusal> {
        self.change(rev, |c| {
            let p = c
                .profiles
                .iter_mut()
                .find(|p| p.id == id)
                .ok_or_else(|| format!("there is no profile {id:?}"))?;
            p.rules = modes::template(id).unwrap_or_default();
            Ok(())
        })
    }

    pub fn reset_all(&self, rev: u64) -> Result<u64, Refusal> {
        self.change(rev, |c| {
            *c = Config::default();
            Ok(())
        })
    }

    /// Boost `target` for `minutes` from `now` (Unix seconds).
    pub fn boost(
        &self,
        target: Target,
        minutes: Option<u32>,
        now: u64,
        rev: u64,
    ) -> Result<u64, Refusal> {
        if minutes == Some(0) {
            return Err((ErrorCode::Invalid, "a boost needs at least a minute".into()));
        }
        self.change(rev, |c| {
            c.boost = Some(Boost {
                target,
                until: minutes.map(|m| now + u64::from(m) * 60),
            });
            Ok(())
        })
    }

    /// Pause for `minutes` from `now` (Unix seconds), or until resumed.
    pub fn pause(&self, minutes: Option<u32>, now: u64, rev: u64) -> Result<u64, Refusal> {
        if minutes == Some(0) {
            return Err((ErrorCode::Invalid, "a pause needs at least a minute".into()));
        }
        self.change(rev, |c| {
            c.pause = Some(Pause {
                until: minutes.map(|m| now + u64::from(m) * 60),
            });
            Ok(())
        })
    }

    pub fn resume(&self, rev: u64) -> Result<u64, Refusal> {
        self.change(rev, |c| {
            c.pause = None;
            Ok(())
        })
    }

    /// Block or unblock an app (a family id) in every mode.
    pub fn set_blocked(&self, family: &str, blocked: bool, rev: u64) -> Result<u64, Refusal> {
        let id = family.trim().to_lowercase();
        self.change(rev, |c| {
            c.blocked.retain(|b| !b.eq_ignore_ascii_case(&id));
            if blocked {
                c.blocked.push(id);
                c.blocked.sort();
            }
            Ok(())
        })
    }

    pub fn stop_boost(&self, rev: u64) -> Result<u64, Refusal> {
        self.change(rev, |c| {
            c.boost = None;
            Ok(())
        })
    }

    /// Ask for a speed test (§10.1); the runner runs it with shaping off.
    /// Refused in Safe mode (nothing runs until a restart) and while one
    /// is already running.
    pub fn request_speedtest(&self) -> Result<u64, Refusal> {
        let rev = {
            let mut i = self.lock();
            match i.state {
                ServiceState::SafeMode { .. } => {
                    return Err((
                        ErrorCode::Invalid,
                        "SnailNet is in Safe mode; restart the service first".into(),
                    ));
                }
                ServiceState::Measuring => {
                    return Err((ErrorCode::Invalid, "a speed test is already running".into()));
                }
                _ => {}
            }
            i.speedtest = true;
            i.rev
        };
        self.signal.request_reload();
        Ok(rev)
    }

    /// Whether a speed test is waiting to run.
    pub fn speedtest_pending(&self) -> bool {
        self.lock().speedtest
    }

    /// Take the waiting speed test, if any.
    pub fn take_speedtest(&self) -> bool {
        std::mem::take(&mut self.lock().speedtest)
    }

    /// Save a speed test's result: its speeds become the typed ones (a pin
    /// stays as it was; the user asked to measure), with the record.
    pub fn record_measured(&self, m: Measured) -> Result<u64, Refusal> {
        let rev = self.rev();
        self.change(rev, |c| {
            c.line.down_mbps = Some(m.down_mbps);
            c.line.up_mbps = Some(m.up_mbps);
            c.line.measured = Some(m);
            Ok(())
        })
    }

    /// The window in front is `pid`'s (§11.5); a game stays first while it
    /// runs. Wakes the runner when what goes first changed.
    pub fn focus_changed(&self, pid: u32, is_game: bool) {
        let created = snailnet_win::process::created(pid);
        let changed = self.lock().focus.changed(pid, is_game, created);
        if changed {
            self.signal.request_reload();
        }
    }

    /// Forget games that have exited; wakes the runner if any did.
    pub fn prune_focus(&self) {
        if !self.lock().focus.has_games() {
            return;
        }
        let alive = |p: Proc| snailnet_win::process::created(p.pid) == Some(p.created);
        if self.lock().focus.prune(alive) {
            self.signal.request_reload();
        }
    }

    /// The PC is on `net` now (§10.3): the line speed becomes that
    /// network's (unknown: none yet), and the one left is kept.
    pub fn network_seen(&self, net: Network, now: u64) -> Result<Switch, Refusal> {
        let mut switch = Switch::Same;
        self.change(self.rev(), |c| {
            switch = c.switch_network(net, now);
            Ok(())
        })?;
        Ok(switch)
    }

    pub fn set_settings(&self, settings: Settings, rev: u64) -> Result<u64, Refusal> {
        self.change(rev, |c| {
            c.settings = settings;
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
        tracing::info!("config changed; now revision {new_rev}");
        self.signal.request_reload();
        let _ = self.events.send(Event::ConfigChanged { rev: new_rev });
        Ok(new_rev)
    }
}

/// What a tag's parts index: apps by app id (in the order first seen) and
/// sites by slot. Both are read after the snapshot, so they cover every tag
/// in it.
#[derive(Clone, Copy)]
struct Known<'a> {
    apps: &'a [AppInfo],
    sites: &'a [(String, String)],
}

fn dir_stats(
    now: Option<&DirectionStats>,
    last: Option<&DirectionStats>,
    secs: f64,
    known: Known<'_>,
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
    let (families, sites) = tag_ticks(&now.tags, last.map_or(&[][..], |l| &l.tags), secs, known);
    Some(DirStats {
        shaped_bps: now.rate_bytes_per_sec * 8,
        classes,
        families,
        sites,
    })
}

/// Bytes per tag since `last` (both sorted by tag), summed per family (with
/// the unidentified part) and per (site, family).
fn tag_ticks(
    now: &[(Tag, u64)],
    last: &[(Tag, u64)],
    secs: f64,
    known: Known<'_>,
) -> (Vec<FamilyTick>, Vec<SiteTick>) {
    let bps = |bytes: u64| (bytes as f64 * 8.0 / secs) as u64;
    // (family, name) → (bytes, unidentified bytes)
    let mut by_family: HashMap<(&str, &str), (u64, u64)> = HashMap::new();
    // (site slot, family) → bytes
    let mut by_site: HashMap<(usize, &str), u64> = HashMap::new();
    for &(tag, bytes) in now {
        let before = last
            .binary_search_by_key(&tag, |&(t, _)| t)
            .map_or(0, |k| last[k].1);
        let delta = bytes.saturating_sub(before);
        if delta == 0 {
            continue;
        }
        let (app, site) = tags::unpack(tag);
        let key = match app.and_then(|a| known.apps.get(a as usize)) {
            Some(a) => (a.family.as_str(), a.name.as_str()),
            None => (UNKNOWN_ID, "Unknown"),
        };
        let f = by_family.entry(key).or_default();
        f.0 += delta;
        match site {
            Site::Unidentified => f.1 += delta,
            Site::Slot(k) if usize::from(k) < known.sites.len() => {
                *by_site.entry((usize::from(k), key.0)).or_default() += delta;
            }
            Site::Slot(_) | Site::None => {}
        }
    }
    let mut families: Vec<FamilyTick> = by_family
        .into_iter()
        .map(|((family, name), (bytes, hidden))| FamilyTick {
            family: family.to_owned(),
            name: name.to_owned(),
            bps: bps(bytes),
            unidentified_bps: bps(hidden),
        })
        .collect();
    families.sort_by(|a, b| b.bps.cmp(&a.bps).then_with(|| a.family.cmp(&b.family)));
    let mut sites: Vec<SiteTick> = by_site
        .into_iter()
        .map(|((k, family), bytes)| SiteTick {
            site: known.sites[k].0.clone(),
            name: known.sites[k].1.clone(),
            family: family.to_owned(),
            bps: bps(bytes),
        })
        .collect();
    sites.sort_by(|a, b| {
        b.bps
            .cmp(&a.bps)
            .then_with(|| a.site.cmp(&b.site))
            .then_with(|| a.family.cmp(&b.family))
    });
    (families, sites)
}

/// Every site a rule may name (custom sites replace shipped ones with the
/// same id), sorted by display name.
fn known_sites(config: &Config) -> Vec<SiteInfo> {
    let pack = SitePack::builtin();
    let mut out: Vec<SiteInfo> = pack
        .sites
        .iter()
        .filter(|s| !config.sites.iter().any(|c| c.id == s.id))
        .chain(&config.sites)
        .map(|s| SiteInfo {
            id: s.id.clone(),
            name: s.name.clone(),
        })
        .collect();
    out.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    out
}

#[cfg(test)]
mod tests {
    use snailnet_core::rules::Target;
    use snailnet_core::scheduler::ClassStats;
    use snailnet_engine::fastpath::NO_TAG;

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
    fn the_snapshot_lists_known_sites_with_custom_ones_replacing_shipped() {
        let mut config = Config::default();
        config.sites.push(snailnet_core::sites::SiteDef {
            id: "youtube".into(),
            name: "My Tube".into(),
            domains: vec!["mytube.example".into()],
        });
        let sites = known_sites(&config);
        let tubes: Vec<_> = sites.iter().filter(|s| s.id == "youtube").collect();
        assert_eq!(tubes.len(), 1);
        assert_eq!(tubes[0].name, "My Tube");
        assert!(sites.len() >= 5, "the shipped pack is listed too");
        let names: Vec<String> = sites.iter().map(|s| s.name.to_lowercase()).collect();
        assert!(names.is_sorted());

        let (_t, hub) = hub();
        assert_eq!(hub.snapshot().sites, known_sites(&Config::default()));
    }

    #[test]
    fn the_app_in_front_goes_into_the_plan_while_shaping() {
        let (_t, hub) = hub();
        let me = std::process::id();
        hub.focus_changed(me, true);
        assert!(hub.signal().take_reload());
        assert!(
            hub.plan(0).0.focus.is_empty(),
            "nothing shaped, nothing first"
        );
        let line = Line {
            down_mbps: Some(25.0),
            ..Line::default()
        };
        hub.set_line(line, hub.rev()).unwrap();
        assert_eq!(hub.plan(0).0.focus, vec![me]);
        hub.prune_focus();
        assert_eq!(hub.plan(0).0.focus, vec![me], "this test is still running");
        hub.change(hub.rev(), |c| {
            c.settings.focus_boost = false;
            Ok(())
        })
        .unwrap();
        assert!(hub.plan(0).0.focus.is_empty(), "switched off");
    }

    #[test]
    fn each_network_keeps_its_line_speed_on_disk() {
        let (_t, hub) = hub();
        let net = |id: &str| Network {
            id: id.into(),
            name: id.to_uppercase(),
        };
        let line = Line {
            down_mbps: Some(25.0),
            ..Line::default()
        };
        hub.set_line(line, hub.rev()).unwrap();
        assert_eq!(hub.network_seen(net("home"), 10), Ok(Switch::Adopted));
        assert_eq!(hub.network_seen(net("home"), 11), Ok(Switch::Same));
        assert_eq!(hub.network_seen(net("phone"), 12), Ok(Switch::New));
        assert_eq!(hub.config().line.down_mbps, None, "not shaping yet");
        assert!(hub.signal().take_reload(), "the runner follows");
        assert_eq!(hub.network_seen(net("home"), 13), Ok(Switch::Known));
        let saved = store::load(hub.dir()).unwrap().config;
        assert_eq!(saved.line.down_mbps, Some(25.0));
        assert_eq!(saved.network, Some(net("home")));
        assert_eq!(saved.networks[0].id, "phone");
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
        hub.publish(&snap(100, 101_000, 4), Some(Seen::default()));
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
        let seen = |apps: &Vec<AppInfo>| {
            Some(Seen {
                apps: apps.clone(),
                ..Seen::default()
            })
        };
        hub.publish(&snap(0, vec![(0, 10_000), (3, 500)]), seen(&apps));
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
            seen(&apps),
        );
        let d = hub.latest_tick().down.unwrap();
        assert!(d.sites.is_empty());
        let fam: Vec<(String, String, u64)> = d
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

    #[test]
    fn per_site_rows_and_unidentified_bytes_come_from_the_tags() {
        let (_t, hub) = hub();
        let app = |family: &str, name: &str| AppInfo {
            family: family.into(),
            name: name.into(),
            exe: format!("{family}.exe"),
        };
        let seen = |names: NameTick| {
            Some(Seen {
                apps: vec![app("brave", "Brave"), app("discord", "Discord")],
                sites: vec![
                    ("youtube".into(), "YouTube".into()),
                    ("twitch".into(), "Twitch".into()),
                ],
                names: Some(names),
            })
        };
        let (brave, discord) = (Some(0), Some(1));
        let yt = tags::pack(brave, Site::Slot(0));
        let yt_discord = tags::pack(discord, Site::Slot(0));
        let twitch = tags::pack(brave, Site::Slot(1));
        let hidden = tags::pack(brave, Site::Unidentified);
        let plain = tags::pack(brave, Site::None);
        let hidden_unknown = tags::pack(None, Site::Unidentified);
        let base = Instant::now();
        let snap = |ms: u64, mut tags: Vec<(Tag, u64)>| {
            tags.sort_unstable();
            EngineSnapshot {
                taken: Some(base + std::time::Duration::from_millis(ms)),
                down: Some(DirectionStats {
                    rate_bytes_per_sec: 1_000_000,
                    classes: vec![],
                    tags,
                }),
                up: None,
            }
        };
        let n0 = NameTick {
            sni: 100,
            dns: 10,
            ech: 1,
            none: 2,
            naming: 3,
        };
        hub.publish(&snap(0, vec![(yt, 1_000), (twitch, 7)]), seen(n0));
        let n1 = NameTick {
            sni: 150,
            dns: 10,
            ech: 4,
            none: 2,
            naming: 5,
        };
        // Over 100 ms: Brave 50 kB YouTube + 10 kB unidentified + 5 kB
        // other; Discord 25 kB YouTube; Twitch idle; 1 kB unidentified from
        // an unknown app.
        hub.publish(
            &snap(
                100,
                vec![
                    (yt, 51_000),
                    (yt_discord, 25_000),
                    (twitch, 7),
                    (hidden, 10_000),
                    (plain, 5_000),
                    (hidden_unknown, 1_000),
                ],
            ),
            seen(n1),
        );
        let t = hub.latest_tick();
        let names = t.names.unwrap();
        assert_eq!(names, n1.since(&n0));
        assert_eq!(names.unidentified(), 3);
        let d = t.down.unwrap();
        let sites: Vec<(&str, &str, &str, u64)> = d
            .sites
            .iter()
            .map(|s| (s.site.as_str(), s.name.as_str(), s.family.as_str(), s.bps))
            .collect();
        assert_eq!(
            sites,
            vec![
                ("youtube", "YouTube", "brave", 4_000_000),
                ("youtube", "YouTube", "discord", 2_000_000),
            ]
        );
        let fam: Vec<(&str, u64, u64)> = d
            .families
            .iter()
            .map(|f| (f.family.as_str(), f.bps, f.unidentified_bps))
            .collect();
        assert_eq!(
            fam,
            vec![
                ("brave", 5_200_000, 800_000),
                ("discord", 2_000_000, 0),
                (UNKNOWN_ID, 80_000, 80_000),
            ]
        );
    }

    #[test]
    fn a_tag_naming_a_site_the_list_lacks_still_counts_for_its_app() {
        let (_t, hub) = hub();
        let tag = tags::pack(Some(0), Site::Slot(5));
        let base = Instant::now();
        let snap = |ms: u64, bytes: u64| EngineSnapshot {
            taken: Some(base + std::time::Duration::from_millis(ms)),
            down: Some(DirectionStats {
                rate_bytes_per_sec: 1_000_000,
                classes: vec![],
                tags: vec![(tag, bytes)],
            }),
            up: None,
        };
        let seen = Seen {
            apps: vec![AppInfo {
                family: "brave".into(),
                name: "Brave".into(),
                exe: "brave.exe".into(),
            }],
            ..Seen::default()
        };
        hub.publish(&snap(0, 0), Some(seen.clone()));
        hub.publish(&snap(100, 1_000), Some(seen));
        let t = hub.latest_tick();
        assert_eq!(t.names, None, "no name counters reported");
        let d = t.down.unwrap();
        assert!(d.sites.is_empty());
        assert_eq!(d.families[0].bps, 80_000);
    }
}
