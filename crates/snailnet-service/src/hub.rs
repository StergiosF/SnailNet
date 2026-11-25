//! What the runner and the IPC server share (PLAN.md §12): the config and
//! its revision, the service state, the latest stats, and the apps seen.
//!
//! Changes from clients go through [`Hub::change`]: check `rev`, apply,
//! validate, save `config.toml` (atomically, old file → `.bak`), bump
//! `rev`, then ask the runner to reload and tell every client.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use snailnet_core::classify::Resolver;
use snailnet_core::classify::family::UNKNOWN_ID;
use snailnet_core::families::Families;
use snailnet_core::modes;
use snailnet_core::networks::Switch;
use snailnet_core::rules::{
    Boost, Config, Line, Measured, Network, NewApps, Pause, Plan, Rule, Settings, Target,
};
use snailnet_engine::fastpath::{DirectionStats, Snapshot as EngineSnapshot, Tag};
use snailnet_ipc::{
    AppInfo, ClassTick, DirStats, ErrorCode, Event, FamilyTick, GameTick, GameWhy, NameTick,
    PingTick, PromptAction, ServiceState, SiteInfo, SiteTick, Snapshot, StatsTick,
};
use snailnet_win::process::{WinProcesses, windows_dir};
use tokio::sync::broadcast;

use crate::autorate;
use crate::focus::{Focus, Proc};
use crate::newapps::Newcomers;
use crate::paths::DataDir;
use crate::runner::{StopSignal, unix_now};
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
    /// Families in `apps` that are parts of Windows (never "new", §11.4).
    pub windows: HashSet<String>,
    /// App ids (indexes into `apps`) in a live call now (§11.9).
    pub calls: Vec<u32>,
}

/// A process's app family (`None`: unknown), grouped as the user wants,
/// for the focus overrides.
type FamilyOf = Box<dyn FnMut(u32, &Families) -> Option<String> + Send>;

pub struct Hub {
    dir: DataDir,
    signal: StopSignal,
    started: Instant,
    events: broadcast::Sender<Event>,
    inner: Mutex<Inner>,
    /// Only asked when the window in front changes (debounced, rare).
    family_of: Mutex<FamilyOf>,
    /// The route to the internet changed (woke from sleep, Wi-Fi came
    /// back, another adapter): the runner restarts shaping from scratch.
    route_changed: AtomicBool,
}

/// Families as the shaper resolves them (same pack, same rules), with a
/// resolver of its own built on first use and again when the packs change.
fn family_of_process() -> FamilyOf {
    let mut found: Option<(u64, Resolver, WinProcesses)> = None;
    Box::new(move |pid, families: &Families| {
        let packs = snailnet_core::packs::current();
        if found.as_ref().is_some_and(|f| f.0 != packs.serial) {
            found = None;
        }
        let (_, resolver, procs) = found.get_or_insert_with(|| {
            (
                packs.serial,
                Resolver::new(packs.families.clone(), &windows_dir()),
                WinProcesses::new(),
            )
        });
        let family = families.apply(resolver.resolve(pid, procs).app).family;
        (family != UNKNOWN_ID).then_some(family)
    })
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
    /// The "with SnailNet" check (§11.10) waits for shaping: the
    /// `measured.at` it belongs to.
    check_pending: Option<u64>,
    /// The line's ping now, while shaping (§11.10).
    ping: Option<PingTick>,
    /// What goes first now (§11.5).
    focus: Focus,
    /// Families in a live call, as the shaper last saw them (§11.9).
    calls: Vec<String>,
    /// The apps met so far and the open new-app questions (§11.4).
    newcomers: Newcomers,
}

impl Inner {
    /// A game runs and "Gaming mode while you play" is on: the plan uses
    /// the Gaming mode (§11.5).
    /// The games running now, for the app (§11.5).
    fn games(&self) -> Vec<GameTick> {
        self.focus.games(&self.config.settings)
    }

    /// The app in front that goes first now (§11.5), for `tick`: none
    /// while the switch is off or nothing is shaped (then nothing goes
    /// first, as in `plan`).
    fn front(&self, tick: &StatsTick) -> Option<String> {
        let shaping = tick.down.is_some() || tick.up.is_some();
        (self.config.first().focus && shaping)
            .then(|| self.focus.front(&self.config.settings))
            .flatten()
    }

    /// The apps in a live call that go first now (§11.9): none while the
    /// switch is off, never a "never first" app, each family once, sorted.
    fn calls(&self) -> Vec<String> {
        let s = &self.config.settings;
        if !self.config.first().calls {
            return Vec::new();
        }
        let mut out: Vec<String> = self
            .calls
            .iter()
            .filter(|f| !s.never_first(f))
            .cloned()
            .collect();
        out.sort();
        out.dedup();
        out
    }

    fn playing(&self) -> bool {
        self.config.first().gaming
            && self.focus.has_games(&self.config.settings)
            && self.config.profiles.iter().any(|p| p.id == modes::GAMING)
    }
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
            dir: dir.clone(),
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
                check_pending: None,
                ping: None,
                focus: Focus::default(),
                calls: Vec::new(),
                newcomers: Newcomers::load(&dir.known_apps()).unwrap_or_else(|e| {
                    tracing::warn!("could not read the list of known apps: {e}");
                    Newcomers::default()
                }),
            }),
            family_of: Mutex::new(family_of_process()),
            route_changed: AtomicBool::new(false),
        })
    }

    /// The internet route changed: restart shaping, so nothing diverted
    /// on the old one (or across sleep) is kept.
    pub fn route_changed(&self) {
        self.route_changed.store(true, Ordering::SeqCst);
        self.signal.request_reload();
    }

    /// True (once) if the route changed since the last call.
    pub fn take_route_change(&self) -> bool {
        self.route_changed.swap(false, Ordering::SeqCst)
    }

    /// Resolve families with `f` instead (tests).
    #[cfg(test)]
    pub fn set_family_of(&self, f: impl FnMut(u32) -> Option<String> + Send + 'static) {
        let mut f = f;
        *self.family_of.lock().unwrap() = Box::new(move |pid, _: &Families| f(pid));
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
        let mut plan;
        let name;
        if i.playing() {
            // The mode the user picked stays saved; only the plan changes.
            let mut c = i.config.clone();
            c.active_profile = modes::GAMING.into();
            plan = c.plan_live(now, live);
            name = c.active().name.clone();
        } else {
            plan = i.config.plan_live(now, live);
            name = i.config.active().name.clone();
        }
        // While Gaming takes over, its behavior applies, not the saved mode's.
        let first = if i.playing() {
            modes::first_for(modes::GAMING)
        } else {
            i.config.first()
        };
        if first.focus && plan.shapes_anything() {
            plan.focus = i.focus.pids(&i.config.settings);
        }
        if plan.strict {
            // New apps waiting for an answer (§11.4).
            for f in i.newcomers.held(now) {
                if !plan.blocked.contains(&f) {
                    plan.blocked.push(f);
                }
            }
        }
        (plan, name)
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
        if self.lock().newcomers.release_expired(now) {
            tracing::info!("no answer in time: the held app goes online");
            self.signal.request_reload();
        }
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
            prompts: i.newcomers.pending().to_vec(),
        }
    }

    /// The newest stats, or an empty tick when not shaping.
    pub fn latest_tick(&self) -> StatsTick {
        let i = self.lock();
        match &i.tick {
            Some(t) => StatsTick {
                state: i.state.clone(),
                playing: i.playing(),
                games: i.games(),
                calls: i.calls(),
                front: i.front(t),
                ping: i.ping,
                ..t.clone()
            },
            None => StatsTick {
                t_ms: self.started.elapsed().as_millis() as u64,
                state: i.state.clone(),
                down: None,
                up: None,
                names: None,
                playing: i.playing(),
                games: i.games(),
                calls: Vec::new(),
                front: None,
                ping: None,
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
            i.calls = seen
                .calls
                .iter()
                .filter_map(|&a| seen.apps.get(a as usize))
                .map(|a| a.family.clone())
                .collect();
            i.apps = seen.apps;
            i.sites = seen.sites;
            names = seen.names;
            self.meet_apps(&mut i, &seen.windows);
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
                playing: i.playing(),
                games: i.games(),
                calls: i.calls(),
                front: None,
                ping: i.ping,
            }
        });
        let tick = tick.map(|mut t| {
            t.front = i.front(&t);
            t
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

    /// Check the apps seen for newcomers (§11.4): remember them, and ask
    /// about them if the user wants that.
    fn meet_apps(&self, i: &mut Inner, windows: &HashSet<String>) {
        let mode = if i.rev > 0 {
            i.config.settings.new_apps
        } else {
            NewApps::Off
        };
        let Inner {
            newcomers,
            apps,
            config,
            ..
        } = i;
        let (prompts, changed) = newcomers.observe(apps, windows, config, mode, unix_now());
        if changed && let Err(e) = newcomers.save(&self.dir.known_apps()) {
            tracing::warn!("could not save the list of known apps: {e}");
        }
        if prompts.iter().any(|p| p.held) {
            // Strict mode: the plan holds them from now on.
            self.signal.request_reload();
        }
        for p in prompts {
            if p.held {
                tracing::info!("new app held until answered: {} ({})", p.name, p.family);
            } else {
                tracing::info!("new app online: {} ({})", p.name, p.family);
            }
            let _ = self.events.send(Event::NewApp(p));
        }
    }

    /// The user answered new-app question `prompt_id` (§11.4). `block`
    /// blocks the app; `fine` and `slice` close the question (the app opens
    /// the split for a slice) and let a held app go online.
    pub fn answer_prompt(&self, prompt_id: u64, action: &str) -> Result<u64, Refusal> {
        let action = PromptAction::parse(action)
            .ok_or_else(|| (ErrorCode::Invalid, format!("{action:?} isn't an answer")))?;
        let prompt = self.lock().newcomers.answer(prompt_id).ok_or_else(|| {
            (
                ErrorCode::Invalid,
                "that question was answered already".to_owned(),
            )
        })?;
        tracing::info!("new app {}: {}", prompt.family, action.name());
        match action {
            PromptAction::Block => self.set_blocked(&prompt.family, true, self.rev()),
            PromptAction::Fine | PromptAction::Slice => {
                if prompt.held {
                    self.signal.request_reload();
                }
                Ok(self.rev())
            }
        }
    }

    /// Shaping stopped: its counters are gone, and so are its rates.
    pub(crate) fn clear_stats(&self) {
        let mut i = self.lock();
        i.last = None;
        i.tick = None;
        i.calls.clear();
        i.ping = None;
    }

    /// The line's ping now (§11.10), from the runner each second.
    pub(crate) fn set_ping(&self, ping: Option<autorate::PingNow>) {
        self.lock().ping = ping.map(|p| PingTick {
            now_ms: p.now_ms,
            low_ms: p.low_ms,
        });
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
            p.rules = modes::template(id)
                .ok_or("a mode of your own has no starting split to go back to")?;
            Ok(())
        })
    }

    /// A copy of mode `from` named `name`, made the active mode (§11.2).
    pub fn copy_profile(&self, from: &str, name: &str, rev: u64) -> Result<u64, Refusal> {
        self.change(rev, |c| {
            c.active_profile = c.copy_profile(from, name)?;
            Ok(())
        })
    }

    pub fn rename_profile(&self, id: &str, name: &str, rev: u64) -> Result<u64, Refusal> {
        self.change(rev, |c| c.rename_profile(id, name))
    }

    pub fn delete_profile(&self, id: &str, rev: u64) -> Result<u64, Refusal> {
        self.change(rev, |c| c.delete_profile(id))
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
    /// A "with SnailNet" check (§11.10) is set up for it, run once shaping
    /// at the new speed has settled.
    pub fn record_measured(&self, m: Measured) -> Result<u64, Refusal> {
        let rev = self.rev();
        let at = m.at;
        let r = self.change(rev, |c| {
            c.line.down_mbps = Some(m.down_mbps);
            c.line.up_mbps = Some(m.up_mbps);
            c.line.measured = Some(m);
            Ok(())
        });
        if r.is_ok() {
            self.lock().check_pending = Some(at);
        }
        r
    }

    /// Take the waiting "with SnailNet" check: the `measured.at` it
    /// belongs to.
    pub fn take_check(&self) -> Option<u64> {
        self.lock().check_pending.take()
    }

    /// Put a check that was cut short (shaping restarted) back, unless a
    /// newer one is waiting.
    pub(crate) fn retry_check(&self, at: u64) {
        self.lock().check_pending.get_or_insert(at);
    }

    /// The `measured.at` of the current network's last speed test.
    pub fn measured_at(&self) -> Option<u64> {
        self.lock().config.line.measured.as_ref().map(|m| m.at)
    }

    /// Save the "with SnailNet" ping of the check for the speed test taken
    /// at `at`; only if that is still the last one (a newer test, a typed
    /// speed or another network wins). True if it was saved.
    pub fn record_check(&self, at: u64, with_ms: f64) -> Result<bool, Refusal> {
        let mut saved = false;
        self.change(self.rev(), |c| {
            if let Some(m) = c.line.measured.as_mut().filter(|m| m.at == at) {
                m.with_ms = Some(with_ms);
                saved = true;
            }
            Ok(())
        })?;
        Ok(saved)
    }

    /// The window in front is `pid`'s (§11.5); a game stays first while it
    /// runs. Wakes the runner when what goes first changed.
    pub fn focus_changed(&self, pid: u32, detected: Option<GameWhy>) {
        // The app's say-so first; else the service's own look at the exe
        // (same packs), so a game is never missed for an older app.
        let detected = detected.or_else(|| {
            snailnet_win::process::image_path(pid)
                .is_some_and(|p| snailnet_core::games::in_game_library(&p.to_string_lossy()))
                .then_some(GameWhy::Library)
        });
        let created = snailnet_win::process::created(pid);
        let families = self.lock().config.families.clone();
        let family = (self.family_of.lock().unwrap_or_else(|e| e.into_inner()))(pid, &families);
        let changed = {
            let mut i = self.lock();
            let Inner { focus, config, .. } = &mut *i;
            focus.changed(pid, detected, created, family, &config.settings)
        };
        if changed {
            self.signal.request_reload();
        }
    }

    /// Forget games that have exited; wakes the runner if any did.
    pub fn prune_focus(&self) {
        if !self.lock().focus.tracks_games() {
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

    /// Newer packs are in use (§17.2): the runner restarts shaping, and
    /// clients fetch a snapshot with the new site list.
    pub fn packs_changed(&self) {
        let rev = self.lock().rev;
        self.signal.request_reload();
        let _ = self.events.send(Event::ConfigChanged { rev });
    }

    pub fn set_settings(&self, settings: Settings, rev: u64) -> Result<u64, Refusal> {
        let mode = settings.new_apps;
        let r = self.change(rev, |c| {
            c.settings = settings;
            Ok(())
        });
        if r.is_ok() && mode != NewApps::Strict {
            let released = {
                let mut i = self.lock();
                let held = i.newcomers.release_all();
                if mode == NewApps::Off {
                    i.newcomers.clear_pending();
                }
                held
            };
            // The reload `change` asked for may have planned the holds
            // already: once more without them.
            if released {
                self.signal.request_reload();
            }
        }
        r
    }

    /// How apps are grouped (§11.6). The new groups show from the next
    /// stats tick; rules and blocks name families as grouped.
    pub fn set_families(&self, families: Families, rev: u64) -> Result<u64, Refusal> {
        self.change(rev, |c| {
            c.families = families;
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
                .and_then(|()| c.check_sites())
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
    let (families, sites) = tag_ticks(now, last, secs, known);
    Some(DirStats {
        shaped_bps: now.rate_bytes_per_sec * 8,
        classes,
        families,
        sites,
    })
}

/// One family's sums for a tick.
#[derive(Default)]
struct FamilySum {
    bytes: u64,
    unidentified: u64,
    queued: u64,
    acted: u64,
}

/// Bytes per tag since `last` (both sorted by tag), summed per family (with
/// the unidentified part) and per (site, family); and each family's
/// pressure (PLAN.md §11.11): bytes queued now, CoDel actions since `last`.
fn tag_ticks(
    now: &DirectionStats,
    last: Option<&DirectionStats>,
    secs: f64,
    known: Known<'_>,
) -> (Vec<FamilyTick>, Vec<SiteTick>) {
    let bps = |bytes: u64| (bytes as f64 * 8.0 / secs) as u64;
    let family_of = |tag: Tag| {
        let (app, site) = tags::unpack(tag);
        let key = match app.and_then(|a| known.apps.get(a as usize)) {
            Some(a) => (a.family.as_str(), a.name.as_str()),
            None => (UNKNOWN_ID, "Unknown"),
        };
        (key, site)
    };
    let mut by_family: HashMap<(&str, &str), FamilySum> = HashMap::new();
    // (site slot, family) → bytes
    let mut by_site: HashMap<(usize, &str), u64> = HashMap::new();
    let last_tags = last.map_or(&[][..], |l| &l.tags);
    for &(tag, bytes) in &now.tags {
        let before = last_tags
            .binary_search_by_key(&tag, |&(t, _)| t)
            .map_or(0, |k| last_tags[k].1);
        let delta = bytes.saturating_sub(before);
        if delta == 0 {
            continue;
        }
        let (key, site) = family_of(tag);
        let f = by_family.entry(key).or_default();
        f.bytes += delta;
        match site {
            Site::Unidentified => f.unidentified += delta,
            Site::Slot(k) if usize::from(k) < known.sites.len() => {
                *by_site.entry((usize::from(k), key.0)).or_default() += delta;
            }
            Site::Slot(_) | Site::None => {}
        }
    }
    let last_pressure = last.map_or(&[][..], |l| &l.pressure);
    for &(tag, p) in &now.pressure {
        let before = last_pressure
            .binary_search_by_key(&tag, |&(t, _)| t)
            .map_or(0, |k| last_pressure[k].1.acted);
        let acted = p.acted.saturating_sub(before);
        if acted == 0 && p.queued_bytes == 0 {
            continue;
        }
        // An app with bytes queued and nothing delivered this tick is
        // listed too: it is the one waiting hardest.
        let f = by_family.entry(family_of(tag).0).or_default();
        f.queued += p.queued_bytes;
        f.acted += acted;
    }
    let mut families: Vec<FamilyTick> = by_family
        .into_iter()
        .map(|((family, name), f)| FamilyTick {
            family: family.to_owned(),
            name: name.to_owned(),
            bps: bps(f.bytes),
            unidentified_bps: bps(f.unidentified),
            queued_bytes: f.queued,
            acted: f.acted,
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
    let packs = snailnet_core::packs::current();
    let pack = &packs.sites;
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
    use snailnet_core::rules::{STRICT_HOLD_SECS, Target};
    use snailnet_core::scheduler::{ClassStats, Pressure};
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

    #[test]
    fn a_new_app_is_asked_about_and_can_be_blocked() {
        let t = tempfile::tempdir().unwrap();
        let d = DataDir::new(t.path());
        d.create().unwrap();
        // SnailNet has been looking for a while: no quiet learning.
        std::fs::write(d.known_apps(), "since 0\nbrave\n").unwrap();
        let hub = Hub::new(d.clone(), StopSignal::new());
        hub.set_loaded(Config::default());
        let mut events = hub.subscribe();
        let app = |family: &str| AppInfo {
            family: family.into(),
            name: family.to_uppercase(),
            exe: format!("{family}.exe"),
        };
        let seen = Seen {
            apps: vec![app("brave"), app("hades"), app(r"c:\windows\x.exe")],
            windows: [r"c:\windows\x.exe".to_owned()].into(),
            ..Seen::default()
        };
        hub.publish(&EngineSnapshot::default(), Some(seen.clone()));
        let Ok(Event::NewApp(p)) = events.try_recv() else {
            panic!("no prompt");
        };
        assert_eq!(p.family, "hades");
        assert!(events.try_recv().is_err(), "brave is known, x is Windows");
        assert_eq!(hub.snapshot().prompts, vec![p.clone()]);
        let known = std::fs::read_to_string(d.known_apps()).unwrap();
        assert!(known.contains("hades"));
        // Seen again: no second question.
        hub.publish(&EngineSnapshot::default(), Some(seen));
        assert!(events.try_recv().is_err());

        assert!(hub.answer_prompt(p.prompt_id, "maybe").is_err());
        let rev = hub.answer_prompt(p.prompt_id, "block").unwrap();
        assert_eq!(rev, 2);
        assert_eq!(hub.config().blocked, ["hades"]);
        assert!(hub.snapshot().prompts.is_empty());
        assert!(hub.answer_prompt(p.prompt_id, "fine").is_err(), "answered");
    }

    #[test]
    fn families_are_saved_and_reach_the_plan() {
        let (_t, hub) = hub();
        let f = Families {
            split: vec!["steamwebhelper.exe".into()],
            ..Families::default()
        };
        let rev = hub.set_families(f.clone(), hub.rev()).unwrap();
        assert_eq!(hub.config().families, f);
        assert_eq!(hub.plan(0).0.families, f);
        let bad = Families {
            split: vec![r"c:\x.exe".into()],
            ..Families::default()
        };
        assert!(hub.set_families(bad, rev).is_err());
        assert_eq!(hub.config().families, f, "unchanged");
    }

    #[test]
    fn strict_mode_holds_a_new_app_in_the_plan() {
        let t = tempfile::tempdir().unwrap();
        let d = DataDir::new(t.path());
        d.create().unwrap();
        std::fs::write(d.known_apps(), "since 0\n").unwrap();
        let hub = Hub::new(d, StopSignal::new());
        let mut c = Config::default();
        c.line.down_mbps = Some(100.0);
        c.settings.new_apps = NewApps::Strict;
        hub.set_loaded(c);
        let seen = |family: &str| Seen {
            apps: vec![AppInfo {
                family: family.into(),
                name: family.into(),
                exe: format!("{family}.exe"),
            }],
            ..Seen::default()
        };
        let now = unix_now();
        let (plan, _) = hub.plan(now);
        assert!(plan.strict && plan.blocking() && plan.blocked.is_empty());

        hub.publish(&EngineSnapshot::default(), Some(seen("hades")));
        let p = hub.snapshot().prompts[0].clone();
        assert!(p.held);
        assert_eq!(hub.plan(now).0.blocked, ["hades"]);
        // Past the hold the plan lets it go, and the runner's check ends it.
        let end = now + STRICT_HOLD_SECS + 1;
        assert!(hub.plan(end).0.blocked.is_empty());
        hub.end_expired(end);
        assert!(!hub.snapshot().prompts[0].held, "still asked, not held");
        hub.answer_prompt(p.prompt_id, "fine").unwrap();

        // Allowed by the answer before the time is up.
        let mut more = seen("celeste");
        more.apps.insert(0, seen("hades").apps[0].clone());
        hub.publish(&EngineSnapshot::default(), Some(more));
        let p = hub.snapshot().prompts[0].clone();
        assert_eq!(hub.plan(now).0.blocked, ["celeste"]);
        hub.answer_prompt(p.prompt_id, "fine").unwrap();
        assert!(hub.plan(now).0.blocked.is_empty());

        // Notify mode never holds, and its plan doesn't divert both ways.
        let mut s = hub.config().settings;
        s.new_apps = NewApps::Notify;
        hub.set_settings(s, hub.rev()).unwrap();
        assert!(!hub.plan(now).0.blocking());
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
        hub.focus_changed(me, Some(GameWhy::Fullscreen));
        assert!(hub.signal().take_reload());
        assert!(
            hub.plan(0).0.focus.is_empty(),
            "nothing shaped, nothing first"
        );
        hub.change(hub.rev(), |c| {
            c.active_profile = "focus".into();
            Ok(())
        })
        .unwrap();
        let line = Line {
            down_mbps: Some(25.0),
            ..Line::default()
        };
        hub.set_line(line, hub.rev()).unwrap();
        assert_eq!(hub.plan(0).0.focus, vec![me]);
        hub.prune_focus();
        assert_eq!(hub.plan(0).0.focus, vec![me], "this test is still running");
        assert!(!hub.latest_tick().playing, "Focus doesn't auto-switch");
        // A copy of Gaming keeps the automation: while a game runs, the
        // plan is Gaming's but the pick stays saved.
        hub.copy_profile("gaming", "Game night", hub.rev()).unwrap();
        assert_eq!(hub.plan(0).1, "Gaming", "a game runs: Gaming mode");
        assert_eq!(
            hub.config().active_profile,
            "game-night",
            "the pick is kept"
        );
        assert!(hub.latest_tick().playing, "clients are told");
        hub.change(hub.rev(), |c| {
            c.active_profile = "balanced".into();
            Ok(())
        })
        .unwrap();
        assert!(
            hub.plan(0).0.focus.is_empty(),
            "Balanced puts nothing first"
        );
    }

    #[test]
    fn the_app_in_front_reaches_the_tick_while_shaping_and_switched_on() {
        let (_t, hub) = hub();
        hub.set_state(ServiceState::Shaping);
        hub.set_family_of(|_| Some("firefox".into()));
        hub.change(hub.rev(), |c| {
            c.active_profile = "focus".into();
            Ok(())
        })
        .unwrap();
        hub.focus_changed(std::process::id(), None);
        assert_eq!(hub.latest_tick().front, None, "no stats yet");
        let base = Instant::now();
        let snap = |ms: u64, shaped: bool| EngineSnapshot {
            taken: Some(base + std::time::Duration::from_millis(ms)),
            down: shaped.then(|| DirectionStats {
                rate_bytes_per_sec: 1_000_000,
                classes: vec![],
                tags: vec![],
                pressure: vec![],
            }),
            up: None,
        };
        hub.publish(&snap(0, true), None);
        hub.publish(&snap(100, true), None);
        assert_eq!(hub.latest_tick().front.as_deref(), Some("firefox"));

        hub.change(hub.rev(), |c| {
            c.active_profile = "balanced".into();
            Ok(())
        })
        .unwrap();
        assert_eq!(hub.latest_tick().front, None, "Balanced: nothing first");
        hub.change(hub.rev(), |c| {
            c.active_profile = "focus".into();
            Ok(())
        })
        .unwrap();

        hub.publish(&snap(200, false), None);
        assert_eq!(
            hub.latest_tick().front,
            None,
            "only blocking, nothing first"
        );
    }

    #[test]
    fn a_copied_mode_becomes_the_active_one() {
        let (_t, hub) = hub();
        hub.copy_profile("gaming", "Late night", hub.rev()).unwrap();
        let c = hub.config();
        assert_eq!(c.active_profile, "late-night");
        assert_eq!(hub.plan(0).1, "Late night");
        assert!(
            hub.reset_profile("late-night", hub.rev()).is_err(),
            "no template to go back to"
        );
        hub.rename_profile("late-night", "Night", hub.rev())
            .unwrap();
        hub.delete_profile("late-night", hub.rev()).unwrap();
        assert_eq!(hub.config().active_profile, "balanced");
        assert!(hub.delete_profile("gaming", hub.rev()).is_err());
    }

    #[test]
    fn the_users_overrides_decide_what_goes_first() {
        let (_t, hub) = hub();
        hub.set_family_of(|_| Some("hades".into()));
        let me = std::process::id();
        hub.change(hub.rev(), |c| {
            c.line.down_mbps = Some(25.0);
            c.active_profile = "gaming".into();
            c.settings.games = vec!["Hades".into()];
            Ok(())
        })
        .unwrap();
        hub.focus_changed(me, None);
        assert_eq!(hub.plan(0).0.focus, vec![me]);
        assert_eq!(hub.plan(0).1, "Gaming", "a game by the user's word");

        hub.change(hub.rev(), |c| {
            c.settings.games.clear();
            c.settings.never_first = vec!["hades".into()];
            Ok(())
        })
        .unwrap();
        assert!(hub.plan(0).0.focus.is_empty(), "never first");
        assert_eq!(
            hub.plan(0).1,
            "Gaming",
            "the mode stays; the game isn't first"
        );
        assert!(!hub.latest_tick().playing);
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

    fn measured(at: u64) -> Measured {
        Measured {
            at,
            down_mbps: 24.6,
            up_mbps: 4.8,
            idle_ms: Some(12.0),
            loaded_ms: Some(142.0),
            loaded_up_ms: Some(300.0),
            with_ms: None,
        }
    }

    #[test]
    fn a_check_is_saved_only_for_the_test_it_belongs_to() {
        let (_t, hub) = hub();
        assert_eq!(hub.take_check(), None);
        hub.record_measured(measured(100)).unwrap();
        assert_eq!(hub.take_check(), Some(100));
        assert_eq!(hub.take_check(), None, "taken");
        let mut events = hub.subscribe();
        assert_eq!(hub.record_check(100, 21.0), Ok(true));
        assert!(matches!(events.try_recv(), Ok(Event::ConfigChanged { .. })));
        let saved = store::load(hub.dir()).unwrap().config;
        assert_eq!(saved.line.measured.unwrap().with_ms, Some(21.0));

        // A newer test wins over the older check.
        hub.record_measured(measured(200)).unwrap();
        let rev = hub.rev();
        assert_eq!(hub.record_check(100, 19.0), Ok(false));
        assert_eq!(hub.rev(), rev, "nothing changed");
        assert_eq!(hub.config().line.measured.unwrap().with_ms, None);

        // A typed speed drops the record: nothing to attach to.
        let line = Line {
            down_mbps: Some(30.0),
            ..Line::default()
        };
        hub.set_line(line, hub.rev()).unwrap();
        assert_eq!(hub.record_check(200, 19.0), Ok(false));
    }

    #[test]
    fn a_cut_short_check_waits_again_unless_a_newer_one_does() {
        let (_t, hub) = hub();
        hub.retry_check(100);
        assert_eq!(hub.take_check(), Some(100));
        hub.record_measured(measured(200)).unwrap();
        hub.retry_check(100);
        assert_eq!(hub.take_check(), Some(200));
    }

    #[test]
    fn the_ping_reaches_the_tick_until_stats_are_cleared() {
        let (_t, hub) = hub();
        let ping = autorate::PingNow {
            now_ms: 19.0,
            low_ms: 12.0,
        };
        hub.set_ping(Some(ping));
        let want = Some(PingTick {
            now_ms: 19.0,
            low_ms: 12.0,
        });
        assert_eq!(hub.latest_tick().ping, None, "not shaping: no tick yet");
        let t0 = Instant::now();
        for k in 0..2 {
            let snap = EngineSnapshot {
                taken: Some(t0 + std::time::Duration::from_millis(k * 250)),
                ..EngineSnapshot::default()
            };
            hub.publish(&snap, None);
        }
        assert_eq!(hub.latest_tick().ping, want);
        hub.set_ping(None);
        assert_eq!(hub.latest_tick().ping, None);
        hub.set_ping(Some(ping));
        hub.clear_stats();
        assert_eq!(hub.latest_tick().ping, None);
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
                pressure: vec![],
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
    fn calls_reach_the_tick_unless_switched_off_or_never_first() {
        let (_t, hub) = hub();
        hub.set_state(ServiceState::Shaping);
        let app = |family: &str| AppInfo {
            family: family.into(),
            name: family.into(),
            exe: format!("{family}.exe"),
        };
        let base = Instant::now();
        let snap = |ms: u64| EngineSnapshot {
            taken: Some(base + std::time::Duration::from_millis(ms)),
            down: None,
            up: None,
        };
        let seen = || {
            Some(Seen {
                // Two Discord processes, Zoom, and an id past the list.
                apps: vec![app("discord"), app("zoom"), app("discord"), app("obs")],
                calls: vec![2, 1, 0, 3, 99],
                ..Seen::default()
            })
        };
        let mut c = Config {
            active_profile: "calls".into(),
            ..Config::default()
        };
        c.settings.never_first = vec!["obs".into()];
        hub.set_loaded(c.clone());
        hub.publish(&snap(0), seen());
        hub.publish(&snap(100), seen());
        assert_eq!(hub.latest_tick().calls, ["discord", "zoom"]);

        c.active_profile = "balanced".into();
        hub.set_loaded(c);
        assert!(hub.latest_tick().calls.is_empty());

        hub.clear_stats();
        assert!(hub.latest_tick().calls.is_empty());
    }

    #[test]
    fn pressure_is_summed_per_family_and_acted_counts_since_the_last_tick() {
        let (_t, hub) = hub();
        let app = |family: &str, exe: &str| AppInfo {
            family: family.into(),
            name: family.to_uppercase(),
            exe: exe.into(),
        };
        let apps = vec![
            app("steam", "steam.exe"),
            app("steam", "steamwebhelper.exe"),
            app("brave", "brave.exe"),
        ];
        let p = |queued_bytes: u64, acted: u64| Pressure {
            queued_bytes,
            acted,
        };
        let base = Instant::now();
        let snap =
            |ms: u64, tags: Vec<(Tag, u64)>, pressure: Vec<(Tag, Pressure)>| EngineSnapshot {
                taken: Some(base + std::time::Duration::from_millis(ms)),
                down: Some(DirectionStats {
                    rate_bytes_per_sec: 1_000_000,
                    classes: vec![],
                    tags,
                    pressure,
                }),
                up: None,
            };
        let seen = || {
            Some(Seen {
                apps: apps.clone(),
                ..Seen::default()
            })
        };
        hub.publish(
            &snap(0, vec![(0, 0), (2, 0)], vec![(0, p(0, 10)), (2, p(0, 4))]),
            seen(),
        );
        // Steam's two exes: 1000 + 500 bytes queued, 2 + 3 actions since.
        // Brave: nothing delivered, 900 queued, 5 actions: still listed.
        hub.publish(
            &snap(
                100,
                vec![(0, 50_000), (1, 10_000)],
                vec![(0, p(1_000, 12)), (1, p(500, 3)), (2, p(900, 9))],
            ),
            seen(),
        );
        let d = hub.latest_tick().down.unwrap();
        let got: Vec<(String, u64, u64, u64)> = d
            .families
            .into_iter()
            .map(|f| (f.family, f.bps, f.queued_bytes, f.acted))
            .collect();
        assert_eq!(
            got,
            vec![
                ("steam".to_owned(), 4_800_000, 1_500, 5),
                ("brave".to_owned(), 0, 900, 5),
            ]
        );
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
                pressure: vec![],
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
                ..Seen::default()
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
                    pressure: vec![],
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
                pressure: vec![],
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
