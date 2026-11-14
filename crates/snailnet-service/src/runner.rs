//! The supervisor (PLAN.md §6.4, §18.1): load the config, shape, and keep
//! the network working whatever happens.
//!
//! ```text
//! start ─► load config ─► crash guard ─► Safe mode? ──yes──► monitor only
//!                                           │ no
//!                    no line speed? ◄───────┤
//!                    monitor only           ▼
//!                               ┌──► start shaping ──fails──┐
//!                               │        │ running          │
//!                               │        ▼                  │
//!                               │   fail-open ──────────────┤ count it
//!                               │                           ▼
//!                               └──── wait 10 s ◄── 3 in 10 min? ──yes──► Safe mode
//! ```
//!
//! "Monitor only" means no WinDivert handles at all (Phase 1 has nothing
//! else to monitor yet; IPC and stats arrive in the next step). The loop
//! exits on a stop request from any state; every exit path stops shaping
//! first, so the network is back to normal before the service reports
//! Stopped.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use snailnet_core::rules::Plan;
use tracing::{error, info, warn};

use crate::crashguard::{self, CrashGuard};
use crate::paths::DataDir;
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
    /// Stop and close everything. Returns the fail-open reason, if any.
    fn stop(self) -> Option<String>;
}

/// A stop request that any thread can make and the runner can wait on.
#[derive(Clone, Default)]
pub struct StopSignal(Arc<(Mutex<bool>, Condvar)>);

impl StopSignal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn request(&self) {
        let (lock, cv) = &*self.0;
        *lock.lock().unwrap_or_else(|e| e.into_inner()) = true;
        cv.notify_all();
    }

    pub fn is_requested(&self) -> bool {
        *self.0.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Wait up to `d`. True if a stop was requested.
    pub fn wait(&self, d: Duration) -> bool {
        let (lock, cv) = &*self.0;
        let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let (guard, _) = cv
            .wait_timeout_while(guard, d, |stop| !*stop)
            .unwrap_or_else(|e| e.into_inner());
        *guard
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// How often to check for a fail-open.
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

/// Run until `stop` is requested. `now` gives Unix seconds.
pub fn run<E: Engine>(
    dir: &DataDir,
    engine: &mut E,
    stop: &StopSignal,
    timing: Timing,
    now: &dyn Fn() -> u64,
) -> Result<Ended> {
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
    let (guard, crashed) = CrashGuard::start(dir, now()).context("reading the crash log")?;
    if crashed {
        warn!("the previous run ended without a clean stop (crash, kill or power loss)");
    }
    let plan = loaded.config.plan();
    let ended = supervise(
        &plan,
        &loaded.config.active().name,
        guard,
        engine,
        stop,
        timing,
        now,
    );
    info!("SnailNet service stopped");
    ended
}

fn supervise<E: Engine>(
    plan: &Plan,
    profile: &str,
    mut guard: CrashGuard,
    engine: &mut E,
    stop: &StopSignal,
    timing: Timing,
    now: &dyn Fn() -> u64,
) -> Result<Ended> {
    let ended = loop {
        if guard.safe_mode(now()) {
            error!(
                "Safe mode: {} crashes or fail-opens in the last {} minutes. Not shaping; \
                 the network runs normally. Restart the service to try again once the \
                 cause is fixed (or wait {} minutes).",
                guard.recent(now()),
                crashguard::WINDOW_SECS / 60,
                crashguard::WINDOW_SECS / 60,
            );
            idle(stop);
            break Ended::StoppedInSafeMode;
        }
        if !plan.shapes_anything() {
            info!("no line speed set ([line] down_mbps / up_mbps in config.toml); not shaping");
            idle(stop);
            break Ended::StoppedUnshaped;
        }
        match engine.start(plan) {
            Ok(mut running) => {
                log_plan(plan, profile);
                let mut since_report = Duration::ZERO;
                let failure = loop {
                    if stop.wait(timing.poll) {
                        break None;
                    }
                    if let Some(f) = running.failure() {
                        break Some(f);
                    }
                    since_report += timing.poll;
                    if since_report >= timing.report_every {
                        running.report();
                        since_report = Duration::ZERO;
                    }
                };
                let failure = running.stop().or(failure);
                if stop.is_requested() {
                    if let Some(f) = failure {
                        warn!("shaping had failed open while stopping: {f}");
                    }
                    break Ended::Stopped;
                }
                let f = failure.unwrap_or_else(|| "shaping stopped by itself".into());
                error!("shaping failed open (the network kept working, unshaped): {f}");
            }
            Err(e) => error!("could not start shaping: {e:#}"),
        }
        guard
            .record_failure(now())
            .context("writing the crash log")?;
        if guard.safe_mode(now()) {
            continue;
        }
        info!("retrying in {} s", timing.retry_after.as_secs());
        if stop.wait(timing.retry_after) {
            break Ended::Stopped;
        }
    };
    guard.clean_exit().context("clearing the running marker")?;
    Ok(ended)
}

fn idle(stop: &StopSignal) {
    while !stop.wait(Duration::from_secs(3600)) {}
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

    use snailnet_core::rules::Config;

    use super::*;

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
        stop_after_starts: Option<(usize, StopSignal)>,
    }

    struct FakeRunning {
        polls: Cell<usize>,
        fail_after: Option<usize>,
        live: Arc<AtomicUsize>,
    }

    impl Engine for Fake {
        type Running = FakeRunning;

        fn start(&mut self, _plan: &Plan) -> Result<FakeRunning> {
            let n = self.starts.fetch_add(1, Ordering::SeqCst);
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
            stop_after_starts: None,
        }
    }

    /// Run on another thread; ask it to stop after `after`.
    fn run_for(d: &DataDir, engine: Fake, after: Duration, now: u64) -> (Result<Ended>, Fake) {
        let stop = StopSignal::new();
        let (d, s) = (d.clone(), stop.clone());
        let h = std::thread::spawn(move || {
            let mut engine = engine;
            let r = run(&d, &mut engine, &s, fast(), &move || now);
            (r, engine)
        });
        std::thread::sleep(after);
        stop.request();
        h.join().unwrap()
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
        let (r, f) = run_for(&d, fake(&script), Duration::from_millis(80), 1_000);
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
        let stop = StopSignal::new();
        let mut f = fake(&[Script::Runs]);
        f.stop_after_starts = Some((1, stop.clone()));
        let r = run(&d, &mut f, &stop, fast(), &|| 1_000).unwrap();
        assert_eq!(r, Ended::Stopped);
        assert_eq!(f.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn stop_signal() {
        let s = StopSignal::new();
        assert!(!s.wait(Duration::from_millis(1)));
        let s2 = s.clone();
        let h = std::thread::spawn(move || s2.wait(Duration::from_secs(10)));
        std::thread::sleep(Duration::from_millis(10));
        s.request();
        assert!(h.join().unwrap());
        assert!(s.is_requested() && s.wait(Duration::ZERO));
    }
}
