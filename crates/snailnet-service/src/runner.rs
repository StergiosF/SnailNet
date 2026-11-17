//! The supervisor (PLAN.md §6.4, §18.1): load the config, shape, and keep
//! the network working whatever happens.
//!
//! ```text
//! start ─► load config ─► crash guard ─► Safe mode? ──yes──► monitor only
//!                                           │ no
//!   config changed ◄───── no line speed? ◄──┤
//!   (back to top)         monitor only      ▼
//!                               ┌──► start shaping ──fails──┐
//!                               │        │ running          │
//!                               │        ├─ config changed: apply live, or if the
//!                               │        │  directions changed, restart (not a failure)
//!                               │        ▼                  │
//!                               │   fail-open ──────────────┤ count it
//!                               │                           ▼
//!                               └──── wait 10 s ◄── 3 in 10 min? ──yes──► Safe mode
//! ```
//!
//! "Monitor only" means no WinDivert handles at all. The config lives in
//! the [`Hub`], where IPC clients change it; a change wakes the loop
//! ([`StopSignal::request_reload`]) and the new plan is applied to the
//! running shaper (PLAN.md §9.6: queues and attribution kept). When it can't
//! be (a direction started or stopped being shaped, or LAN shaping
//! changed), shaping restarts instead; stopping flushes every queued packet,
//! so nothing is lost either way. The loop
//! exits on a stop request from any state; every exit path stops shaping
//! first, so the network is back to normal before the service reports
//! Stopped.

use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use snailnet_core::rules::Plan;
use snailnet_engine::fastpath::Snapshot;
use snailnet_ipc::ServiceState;
use tracing::{error, info, warn};

use crate::crashguard::{self, CrashGuard};
use crate::hub::{Hub, Seen};
use crate::store::{self, Source};

/// Starts shaping for a plan. The real one is [`crate::shaper::WinEngine`].
pub trait Engine {
    type Running: Running;
    fn start(&mut self, plan: &Plan) -> Result<Self::Running>;
}

pub trait Running {
    /// `Some(reason)` once shaping has failed open.
    fn failure(&self) -> Option<String>;
    /// Write a periodic stats line to the log.
    fn report(&mut self) {}
    /// The scheduler's latest per-class numbers, for live stats.
    fn sample(&self) -> Option<Snapshot> {
        None
    }
    /// Apps and sites seen so far, and how flows got their names.
    fn seen(&self) -> Seen {
        Seen::default()
    }
    /// Switch to a new plan without stopping. `Err` means it can't be done
    /// live (the runner restarts shaping instead); nothing was changed.
    fn apply(&mut self, plan: &Plan) -> Result<()> {
        let _ = plan;
        anyhow::bail!("live changes not supported")
    }
    /// Stop and close everything. Returns the fail-open reason, if any.
    fn stop(self) -> Option<String>;
}

#[derive(Default)]
struct Flags {
    stop: bool,
    reload: bool,
}

/// Stop and reload requests that any thread can make and the runner can
/// wait on. A stop is permanent; a reload is consumed by [`Self::take_reload`].
#[derive(Clone, Default)]
pub struct StopSignal(Arc<(Mutex<Flags>, Condvar)>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wake {
    Stop,
    Reload,
    Timeout,
}

impl StopSignal {
    pub fn new() -> Self {
        Self::default()
    }

    fn flags(&self) -> MutexGuard<'_, Flags> {
        self.0.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn request(&self) {
        self.flags().stop = true;
        self.0.1.notify_all();
    }

    /// The config changed: apply it.
    pub fn request_reload(&self) {
        self.flags().reload = true;
        self.0.1.notify_all();
    }

    pub fn is_requested(&self) -> bool {
        self.flags().stop
    }

    /// True (once) if a reload was requested since the last call.
    pub fn take_reload(&self) -> bool {
        std::mem::take(&mut self.flags().reload)
    }

    /// Wait up to `d`. True if a stop was requested.
    pub fn wait(&self, d: Duration) -> bool {
        let guard = self.flags();
        let (guard, _) = self
            .0
            .1
            .wait_timeout_while(guard, d, |f| !f.stop)
            .unwrap_or_else(|e| e.into_inner());
        guard.stop
    }

    /// Wait up to `d` for a stop or a reload (which it consumes).
    fn wait_any(&self, d: Duration) -> Wake {
        let guard = self.flags();
        let (mut guard, _) = self
            .0
            .1
            .wait_timeout_while(guard, d, |f| !f.stop && !f.reload)
            .unwrap_or_else(|e| e.into_inner());
        if guard.stop {
            Wake::Stop
        } else if std::mem::take(&mut guard.reload) {
            Wake::Reload
        } else {
            Wake::Timeout
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// How often to check for a fail-open and publish stats.
    pub poll: Duration,
    /// How often to log a stats line while shaping.
    pub report_every: Duration,
    /// Wait this long after a fail-open before trying again.
    pub retry_after: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            poll: Duration::from_millis(250),
            report_every: Duration::from_secs(60),
            retry_after: Duration::from_secs(10),
        }
    }
}

/// Wall-clock seconds, for the crash log.
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// How a run ended, for tests and the exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// Stopped while shaping (or between retries).
    Stopped,
    /// Stopped while monitoring only: no line speed configured.
    StoppedUnshaped,
    /// Stopped while in Safe mode.
    StoppedInSafeMode,
}

/// Run until the hub's signal requests a stop. `now` gives Unix seconds.
pub fn run<E: Engine>(
    hub: &Hub,
    engine: &mut E,
    timing: Timing,
    now: &dyn Fn() -> u64,
) -> Result<Ended> {
    let dir = hub.dir();
    info!(
        "SnailNet service {} starting; data in {}",
        env!("CARGO_PKG_VERSION"),
        dir.root().display()
    );
    let loaded = store::load(dir).context("loading config.toml")?;
    for p in &loaded.problems {
        warn!("config: {p}");
    }
    match loaded.source {
        Source::File => info!("config: loaded {}", dir.config().display()),
        Source::Backup => warn!("config: using the backup"),
        Source::Default => info!("config: using defaults"),
    }
    let mut config = loaded.config;
    if config.add_missing_modes() {
        info!("config: added the built-in modes");
        if let Err(e) = store::save(dir, &config) {
            warn!("config: could not save the added modes: {e}");
        }
    }
    hub.set_loaded(config);
    let (guard, crashed) = CrashGuard::start(dir, now()).context("reading the crash log")?;
    if crashed {
        warn!("the previous run ended without a clean stop (crash, kill or power loss)");
    }
    let ended = supervise(hub, guard, engine, timing, now);
    info!("SnailNet service stopped");
    ended
}

/// How one shaping session ended.
enum Outcome {
    Stop,
    Reload,
    Failed(String),
}

fn supervise<E: Engine>(
    hub: &Hub,
    mut guard: CrashGuard,
    engine: &mut E,
    timing: Timing,
    now: &dyn Fn() -> u64,
) -> Result<Ended> {
    let stop = hub.signal();
    let ended = loop {
        if stop.is_requested() {
            break Ended::Stopped;
        }
        if guard.safe_mode(now()) {
            let failures = guard.recent(now());
            hub.set_state(ServiceState::SafeMode {
                failures: failures as u32,
            });
            error!(
                "Safe mode: {failures} crashes or fail-opens in the last {} minutes. Not shaping; \
                 the network runs normally. Restart the service to try again once the \
                 cause is fixed (or wait {} minutes).",
                crashguard::WINDOW_SECS / 60,
                crashguard::WINDOW_SECS / 60,
            );
            // Config changes are saved but not applied until a restart.
            while stop.wait_any(Duration::from_secs(3600)) != Wake::Stop {}
            break Ended::StoppedInSafeMode;
        }
        hub.end_boost_if_over(now());
        let (plan, profile) = hub.plan(now());
        if !plan.shapes_anything() {
            hub.set_state(ServiceState::Unshaped);
            info!("no line speed set ([line] down_mbps / up_mbps in config.toml); not shaping");
            match wait_for_change(stop) {
                Wake::Stop => break Ended::StoppedUnshaped,
                _ => continue,
            }
        }
        let reason = match engine.start(&plan) {
            Ok(mut running) => {
                hub.set_state(ServiceState::Shaping);
                log_plan(&plan, &profile);
                let mut since_report = Duration::ZERO;
                let outcome = loop {
                    match stop.wait_any(timing.poll) {
                        Wake::Stop => break Outcome::Stop,
                        Wake::Reload => {
                            let (plan, profile) = hub.plan(now());
                            if !plan.shapes_anything() {
                                break Outcome::Reload;
                            }
                            match running.apply(&plan) {
                                Ok(()) => {
                                    info!("settings changed; applied to the running shaper");
                                    log_plan(&plan, &profile);
                                }
                                Err(e) => {
                                    info!("settings changed; restarting shaping ({e:#})");
                                    break Outcome::Reload;
                                }
                            }
                        }
                        // Ending a boost asks for a reload, applied next turn.
                        Wake::Timeout => {
                            hub.end_boost_if_over(now());
                        }
                    }
                    if let Some(f) = running.failure() {
                        break Outcome::Failed(f);
                    }
                    if let Some(s) = running.sample() {
                        hub.publish(&s, Some(running.seen()));
                    }
                    since_report += timing.poll;
                    if since_report >= timing.report_every {
                        running.report();
                        since_report = Duration::ZERO;
                    }
                };
                let failure = running.stop();
                hub.clear_stats();
                match (outcome, failure) {
                    (Outcome::Stop, f) => {
                        if let Some(f) = f {
                            warn!("shaping had failed open while stopping: {f}");
                        }
                        break Ended::Stopped;
                    }
                    (Outcome::Reload, None) => continue,
                    (Outcome::Reload, Some(f)) | (Outcome::Failed(_), Some(f)) => f,
                    (Outcome::Failed(f), None) => f,
                }
            }
            Err(e) => format!("could not start shaping: {e:#}"),
        };
        error!("shaping failed open (the network kept working, unshaped): {reason}");
        guard
            .record_failure(now())
            .context("writing the crash log")?;
        if guard.safe_mode(now()) {
            continue;
        }
        hub.set_state(ServiceState::Retrying {
            reason: reason.clone(),
        });
        info!("retrying in {} s", timing.retry_after.as_secs());
        // A config change retries at once.
        if stop.wait_any(timing.retry_after) == Wake::Stop {
            break Ended::Stopped;
        }
    };
    guard.clean_exit().context("clearing the running marker")?;
    Ok(ended)
}

/// Wait for a stop or a config change.
fn wait_for_change(stop: &StopSignal) -> Wake {
    loop {
        match stop.wait_any(Duration::from_secs(3600)) {
            Wake::Timeout => {}
            w => return w,
        }
    }
}

fn log_plan(plan: &Plan, profile: &str) {
    for (dir, cfg) in [("down", &plan.down), ("up", &plan.up)] {
        if let Some(c) = cfg {
            let classes: Vec<String> = c
                .classes
                .iter()
                .map(|c| {
                    let mut s =
                        format!("{} {:.1}% {:?}", c.name, c.share_bp as f64 / 100.0, c.prio);
                    if let Some(b) = c.ceil_bytes_per_sec {
                        s += &format!(" max {:.1} Mbps", b as f64 * 8.0 / 1e6);
                    }
                    s
                })
                .collect();
            info!(
                "shaping {dir} at {:.1} Mbps, profile {profile:?}: {}",
                c.rate_bytes_per_sec as f64 * 8.0 / 1e6,
                classes.join(" | ")
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use snailnet_core::rules::{Config, Line};

    use super::*;
    use crate::paths::DataDir;

    /// A scripted engine: each start either fails, fails open after a few
    /// polls, or runs until stopped.
    #[derive(Clone, Copy, Debug)]
    enum Script {
        StartFails,
        FailsOpenAfter(usize),
        Runs,
    }

    struct Fake {
        script: Vec<Script>,
        starts: Arc<AtomicUsize>,
        live: Arc<AtomicUsize>,
        /// Plans it was started with, in order.
        plans: Arc<Mutex<Vec<Plan>>>,
        /// Plans applied live, in order.
        applied: Arc<Mutex<Vec<Plan>>>,
        stop_after_starts: Option<(usize, StopSignal)>,
    }

    struct FakeRunning {
        polls: Cell<usize>,
        fail_after: Option<usize>,
        live: Arc<AtomicUsize>,
        applied: Arc<Mutex<Vec<Plan>>>,
        /// Shaped directions, fixed at start like the real fast path.
        dirs: (bool, bool),
    }

    impl Engine for Fake {
        type Running = FakeRunning;

        fn start(&mut self, plan: &Plan) -> Result<FakeRunning> {
            let n = self.starts.fetch_add(1, Ordering::SeqCst);
            self.plans.lock().unwrap().push(plan.clone());
            if let Some((k, stop)) = &self.stop_after_starts
                && n + 1 >= *k
            {
                stop.request();
            }
            let s = self.script.get(n).copied().unwrap_or(Script::Runs);
            let fail_after = match s {
                Script::StartFails => anyhow::bail!("driver blocked"),
                Script::FailsOpenAfter(k) => Some(k),
                Script::Runs => None,
            };
            self.live.fetch_add(1, Ordering::SeqCst);
            Ok(FakeRunning {
                polls: Cell::new(0),
                fail_after,
                live: self.live.clone(),
                applied: self.applied.clone(),
                dirs: (plan.down.is_some(), plan.up.is_some()),
            })
        }
    }

    impl Running for FakeRunning {
        fn failure(&self) -> Option<String> {
            self.polls.set(self.polls.get() + 1);
            match self.fail_after {
                Some(k) if self.polls.get() > k => Some("scheduler stalled".into()),
                _ => None,
            }
        }

        fn sample(&self) -> Option<Snapshot> {
            Some(Snapshot::default())
        }

        fn apply(&mut self, plan: &Plan) -> Result<()> {
            if (plan.down.is_some(), plan.up.is_some()) != self.dirs {
                anyhow::bail!("directions changed");
            }
            self.applied.lock().unwrap().push(plan.clone());
            Ok(())
        }

        fn stop(self) -> Option<String> {
            self.live.fetch_sub(1, Ordering::SeqCst);
            None
        }
    }

    fn fast() -> Timing {
        Timing {
            poll: Duration::from_millis(1),
            report_every: Duration::from_millis(5),
            retry_after: Duration::from_millis(1),
        }
    }

    fn setup(line: bool) -> (tempfile::TempDir, DataDir) {
        let t = tempfile::tempdir().unwrap();
        let d = DataDir::new(t.path());
        d.create().unwrap();
        let mut c = Config::default();
        if line {
            c.line.down_mbps = Some(25.0);
        }
        store::save(&d, &c).unwrap();
        (t, d)
    }

    fn fake(script: &[Script]) -> Fake {
        Fake {
            script: script.to_vec(),
            starts: Default::default(),
            live: Default::default(),
            plans: Default::default(),
            applied: Default::default(),
            stop_after_starts: None,
        }
    }

    /// Run on another thread; `during` runs while it's going, then it is
    /// asked to stop.
    fn run_with(
        d: &DataDir,
        engine: Fake,
        now: u64,
        during: impl FnOnce(&Hub),
    ) -> (Result<Ended>, Fake) {
        let hub = Hub::new(d.clone(), StopSignal::new());
        let h = {
            let hub = hub.clone();
            std::thread::spawn(move || {
                let mut engine = engine;
                let r = run(&hub, &mut engine, fast(), &move || now);
                (r, engine)
            })
        };
        during(&hub);
        hub.signal().request();
        h.join().unwrap()
    }

    fn run_for(d: &DataDir, engine: Fake, after: Duration, now: u64) -> (Result<Ended>, Fake) {
        run_with(d, engine, now, |_| std::thread::sleep(after))
    }

    /// Poll `f` for up to 2 s.
    fn eventually(mut f: impl FnMut() -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !f() {
            assert!(std::time::Instant::now() < deadline, "timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn shapes_until_stopped_and_exits_cleanly() {
        let (_t, d) = setup(true);
        let (r, f) = run_for(&d, fake(&[Script::Runs]), Duration::from_millis(50), 1_000);
        assert_eq!(r.unwrap(), Ended::Stopped);
        assert_eq!(f.starts.load(Ordering::SeqCst), 1);
        assert_eq!(f.live.load(Ordering::SeqCst), 0, "shaping stopped on exit");
        assert!(!d.running_marker().exists(), "clean exit clears the marker");
    }

    #[test]
    fn no_line_speed_means_no_shaping() {
        let (_t, d) = setup(false);
        let (r, f) = run_for(&d, fake(&[]), Duration::from_millis(20), 1_000);
        assert_eq!(r.unwrap(), Ended::StoppedUnshaped);
        assert_eq!(f.starts.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn setting_a_line_speed_starts_shaping() {
        let (_t, d) = setup(false);
        let (r, f) = run_with(&d, fake(&[]), 1_000, |hub| {
            eventually(|| hub.state() == ServiceState::Unshaped);
            let line = Line {
                down_mbps: Some(25.0),
                ..Line::default()
            };
            hub.set_line(line, hub.rev()).unwrap();
            eventually(|| hub.state() == ServiceState::Shaping);
        });
        assert_eq!(r.unwrap(), Ended::Stopped);
        assert_eq!(f.starts.load(Ordering::SeqCst), 1);
        assert_eq!(f.live.load(Ordering::SeqCst), 0);
    }

    fn rate(p: &Plan) -> u64 {
        p.down.as_ref().unwrap().rate_bytes_per_sec
    }

    #[test]
    fn a_change_to_the_shaped_directions_is_applied_live() {
        let (_t, d) = setup(true);
        let engine = fake(&[]);
        let applied = engine.applied.clone();
        let (r, f) = run_with(&d, engine, 1_000, |hub| {
            eventually(|| hub.state() == ServiceState::Shaping);
            let line = Line {
                down_mbps: Some(50.0),
                ..Line::default()
            };
            hub.set_line(line, hub.rev()).unwrap();
            eventually(|| !applied.lock().unwrap().is_empty());
        });
        assert_eq!(r.unwrap(), Ended::Stopped);
        assert_eq!(f.plans.lock().unwrap().len(), 1, "not restarted");
        let applied = f.applied.lock().unwrap();
        assert_eq!(applied.len(), 1);
        assert!(rate(&applied[0]) > rate(&f.plans.lock().unwrap()[0]));
        assert_eq!(f.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn shaping_a_new_direction_restarts_with_the_new_plan() {
        let (_t, d) = setup(true);
        let (r, f) = run_with(&d, fake(&[]), 1_000, |hub| {
            eventually(|| hub.state() == ServiceState::Shaping);
            let line = Line {
                down_mbps: Some(50.0),
                up_mbps: Some(10.0),
                ..Line::default()
            };
            hub.set_line(line, hub.rev()).unwrap();
            eventually(|| hub.state() == ServiceState::Shaping && hub.rev() == 2);
            std::thread::sleep(Duration::from_millis(20));
        });
        assert_eq!(r.unwrap(), Ended::Stopped);
        let plans = f.plans.lock().unwrap();
        assert_eq!(plans.len(), 2, "restarted once");
        assert!(plans[1].up.is_some());
        assert!(rate(&plans[1]) > rate(&plans[0]));
        assert!(f.applied.lock().unwrap().is_empty());
        assert_eq!(f.live.load(Ordering::SeqCst), 0);
        // A reload is not a failure.
        assert_eq!(std::fs::read_to_string(d.crash_log()).unwrap(), "");
    }

    #[test]
    fn a_fail_open_is_retried() {
        let (_t, d) = setup(true);
        let (r, f) = run_for(
            &d,
            fake(&[Script::FailsOpenAfter(2), Script::Runs]),
            Duration::from_millis(80),
            1_000,
        );
        assert_eq!(r.unwrap(), Ended::Stopped);
        assert_eq!(f.starts.load(Ordering::SeqCst), 2);
        assert_eq!(f.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn three_failures_mean_safe_mode() {
        let (_t, d) = setup(true);
        let script = [
            Script::StartFails,
            Script::FailsOpenAfter(1),
            Script::StartFails,
        ];
        let (r, f) = run_with(&d, fake(&script), 1_000, |hub| {
            eventually(|| matches!(hub.state(), ServiceState::SafeMode { failures: 3 }));
        });
        assert_eq!(r.unwrap(), Ended::StoppedInSafeMode);
        assert_eq!(f.starts.load(Ordering::SeqCst), 3, "no 4th attempt");
        assert_eq!(f.live.load(Ordering::SeqCst), 0);
        // The next start (still within 10 minutes) stays in Safe mode…
        let (r, f) = run_for(&d, fake(&[]), Duration::from_millis(20), 1_100);
        assert_eq!(r.unwrap(), Ended::StoppedInSafeMode);
        assert_eq!(f.starts.load(Ordering::SeqCst), 0);
        // …and after the window it shapes again.
        let later = 1_000 + crashguard::WINDOW_SECS;
        let (r, f) = run_for(&d, fake(&[]), Duration::from_millis(20), later);
        assert_eq!(r.unwrap(), Ended::Stopped);
        assert_eq!(f.starts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn earlier_crashes_count() {
        let (_t, d) = setup(true);
        // Two runs that died without a clean exit…
        for now in [100, 200] {
            let _ = CrashGuard::start(&d, now).unwrap();
        }
        // …so one fail-open now is the third strike.
        let (r, f) = run_for(
            &d,
            fake(&[Script::FailsOpenAfter(1)]),
            Duration::from_millis(50),
            300,
        );
        assert_eq!(r.unwrap(), Ended::StoppedInSafeMode);
        assert_eq!(f.starts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn stop_during_start_stops_shaping() {
        let (_t, d) = setup(true);
        let hub = Hub::new(d.clone(), StopSignal::new());
        let mut f = fake(&[Script::Runs]);
        f.stop_after_starts = Some((1, hub.signal().clone()));
        let r = run(&hub, &mut f, fast(), &|| 1_000).unwrap();
        assert_eq!(r, Ended::Stopped);
        assert_eq!(f.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn stop_signal() {
        let s = StopSignal::new();
        assert!(!s.wait(Duration::from_millis(1)));
        assert_eq!(s.wait_any(Duration::from_millis(1)), Wake::Timeout);
        s.request_reload();
        assert_eq!(s.wait_any(Duration::from_secs(10)), Wake::Reload);
        assert!(!s.take_reload(), "consumed");
        let s2 = s.clone();
        let h = std::thread::spawn(move || s2.wait(Duration::from_secs(10)));
        std::thread::sleep(Duration::from_millis(10));
        s.request();
        assert!(h.join().unwrap());
        assert!(s.is_requested() && s.wait(Duration::ZERO));
        assert_eq!(s.wait_any(Duration::ZERO), Wake::Stop);
    }
}
