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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use snailnet_core::rules::Plan;
use snailnet_engine::fastpath::Snapshot;
use snailnet_ipc::ServiceState;
use tracing::{error, info, warn};

use crate::autorate::{self, Autorate};
use crate::crashguard::{self, CrashGuard};
use crate::hub::{Hub, Seen};
use crate::speedtest;
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

/// The "with SnailNet" check (§11.10): download while shaping and return
/// the median ping meanwhile. [`speedtest::check`] in the service.
pub type CheckFn = fn(&AtomicBool) -> Result<Option<f64>>;

#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// How often to check for a fail-open and publish stats.
    pub poll: Duration,
    /// How often to log a stats line while shaping.
    pub report_every: Duration,
    /// Wait this long after a fail-open before trying again.
    pub retry_after: Duration,
    /// Adapt the line speed live (ping probe + estimator, §10.2). Off in
    /// tests, which must not ping anything.
    pub autorate: bool,
    /// Watch which network the PC is on and keep a line speed per network
    /// (§10.3). Off in tests, which must not change with the real network.
    pub watch_network: bool,
    /// Fetch newer signed packs once a day (§17.2). Off in tests, which
    /// must not reach the internet.
    pub fetch_packs: bool,
    /// The "with SnailNet" check after a speed test (§11.10). `None` in
    /// tests unless one injects a fake.
    pub check: Option<CheckFn>,
    /// How long shaping runs before the check starts, so the plan has
    /// settled at the new speed.
    pub check_after: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            poll: Duration::from_millis(250),
            report_every: Duration::from_secs(60),
            retry_after: Duration::from_secs(10),
            autorate: true,
            watch_network: true,
            fetch_packs: true,
            check: Some(speedtest::check),
            check_after: CHECK_AFTER,
        }
    }
}

/// How often the live line speed takes a step (the estimator's 1 s tick).
const RATE_EVERY: Duration = Duration::from_secs(1);
/// Shaping runs this long before the "with SnailNet" check starts.
const CHECK_AFTER: Duration = Duration::from_secs(2);

/// A running "with SnailNet" check (§11.10).
struct Check {
    /// The `measured.at` it belongs to.
    at: u64,
    /// The network it started on (§10.3).
    network: Option<String>,
    cancel: Arc<AtomicBool>,
    thread: JoinHandle<Result<Option<f64>>>,
}

impl Check {
    fn start(f: CheckFn, at: u64, network: Option<String>) -> Self {
        info!("speed test: checking ping with SnailNet (~10 s)");
        let cancel = Arc::new(AtomicBool::new(false));
        let thread = {
            let cancel = cancel.clone();
            std::thread::spawn(move || f(&cancel))
        };
        Self {
            at,
            network,
            cancel,
            thread,
        }
    }

    /// Ask it to stop; the thread ends by itself (a download can take until
    /// its phase ends), and nothing it finds is recorded.
    fn cancel(self, why: &str) -> JoinHandle<Result<Option<f64>>> {
        info!("speed test: ping check with SnailNet cancelled ({why})");
        self.cancel.store(true, Ordering::Relaxed);
        self.thread
    }

    /// Record a finished check's ping, if it's still for the last test on
    /// this network.
    fn finish(self, hub: &Hub) {
        let network = hub.config().network.map(|n| n.id);
        match self.thread.join() {
            Ok(Ok(Some(ms))) if network == self.network => match hub.record_check(self.at, ms) {
                Ok(true) => info!("speed test: ping {ms} ms loaded with SnailNet"),
                Ok(false) => {
                    info!("speed test: ping check finished after a newer speed; not saved")
                }
                Err((_, e)) => warn!("speed test: could not save the ping check: {e}"),
            },
            Ok(Ok(Some(_))) => info!("speed test: the network changed during the ping check"),
            Ok(Ok(None)) => info!("speed test: no ping came back during the check with SnailNet"),
            Ok(Err(e)) => warn!("speed test: ping check with SnailNet failed: {e:#}"),
            Err(_) => warn!("speed test: ping check with SnailNet crashed"),
        }
    }
}

/// The stats polls of one second, averaged per direction.
#[derive(Default)]
struct Moved {
    sum: [f64; 2],
    n: [u32; 2],
}

impl Moved {
    fn add(&mut self, bps: [Option<f64>; 2]) {
        for (i, b) in bps.into_iter().enumerate() {
            if let Some(b) = b {
                self.sum[i] += b;
                self.n[i] += 1;
            }
        }
    }

    fn take(&mut self) -> [Option<f64>; 2] {
        let out = [0, 1].map(|i| (self.n[i] > 0).then(|| self.sum[i] / f64::from(self.n[i])));
        *self = Self::default();
        out
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
    // Before the config: its site rules are checked against the packs.
    crate::packs::load_saved(dir);
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
    let ended = std::thread::scope(|s| {
        if timing.watch_network {
            let _ = std::thread::Builder::new()
                .name("snailnet-netwatch".into())
                .spawn_scoped(s, || crate::netwatch::watch(hub));
        }
        if timing.fetch_packs {
            let _ = std::thread::Builder::new()
                .name("snailnet-packs".into())
                .spawn_scoped(s, || crate::packs::watch(hub));
        }
        supervise(hub, guard, engine, timing, now)
    });
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
    // A cancelled check still finishing: the next waits for it, so two
    // downloads never overlap.
    let mut draining: Option<JoinHandle<Result<Option<f64>>>> = None;
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
        if hub.take_speedtest() {
            if measure(hub, stop, now) == Wake::Stop {
                break Ended::Stopped;
            }
            continue;
        }
        hub.end_expired(now());
        let (plan, profile) = hub.plan(now());
        if !plan.needs_engine() {
            let wake = if let Some(pause) = hub.pause_at(now()) {
                hub.set_state(ServiceState::Paused { until: pause.until });
                info!("paused: not shaping or blocking");
                match pause.until {
                    // Wake when it ends (at most hourly, to re-check).
                    Some(u) => {
                        stop.wait_any(Duration::from_secs(u.saturating_sub(now()).clamp(1, 3600)))
                    }
                    None => wait_for_change(stop),
                }
            } else {
                hub.set_state(ServiceState::Unshaped);
                info!("no line speed set ([line] down_mbps / up_mbps in config.toml); not shaping");
                wait_for_change(stop)
            };
            match wake {
                Wake::Stop => break Ended::StoppedUnshaped,
                _ => continue,
            }
        }
        // At boot and on waking, the service runs before Windows has a
        // route to the internet (adapter, Wi-Fi, DHCP still coming up).
        // Diverting then gains nothing and gets in the way of the PC
        // getting online, so wait for the route first.
        let (plan, profile) = match timing.watch_network.then(|| wait_for_route(hub, stop)) {
            Some(Wake::Stop) => break Ended::Stopped,
            // It waited (up to a minute): plan again, so a pause or new
            // settings from meanwhile apply before anything is diverted.
            Some(Wake::Reload) => {
                hub.end_expired(now());
                let (plan, profile) = hub.plan(now());
                if !plan.needs_engine() {
                    continue;
                }
                (plan, profile)
            }
            _ => (plan, profile),
        };
        // A route change seen before this start is already covered by it.
        hub.take_route_change();
        let reason = match engine.start(&plan) {
            Ok(mut running) => {
                // Blocking alone, without a line speed, still says so.
                hub.set_state(if plan.shapes_anything() {
                    ServiceState::Shaping
                } else {
                    ServiceState::Unshaped
                });
                log_plan(&plan, &profile);
                let mut since_report = Duration::ZERO;
                // The live line speed for this session (§10.2).
                let mut autorate = Autorate::new(timing.autorate);
                autorate.follow(&hub.config());
                let mut since_rate = Duration::ZERO;
                let mut moved = Moved::default();
                // The "with SnailNet" check (§11.10) measures the download.
                let started = Instant::now();
                let down_shaped = plan.down.is_some();
                let mut check: Option<Check> = None;
                let outcome = loop {
                    match stop.wait_any(timing.poll) {
                        Wake::Stop => break Outcome::Stop,
                        // A speed test runs with shaping stopped.
                        Wake::Reload if hub.speedtest_pending() => break Outcome::Reload,
                        // Woke from sleep or on another route: start over.
                        Wake::Reload if hub.take_route_change() => break Outcome::Reload,
                        // New packs: rebuild the classifier and site matcher.
                        Wake::Reload if crate::packs::take_restart() => {
                            info!("new site, app and game lists; restarting shaping");
                            break Outcome::Reload;
                        }
                        Wake::Reload => {
                            let config = hub.config();
                            if let Some(c) = check.take() {
                                let network = config.network.as_ref().map(|n| n.id.clone());
                                let at = config.line.measured.as_ref().map(|m| m.at);
                                check = if network != c.network {
                                    draining = Some(c.cancel("the network changed"));
                                    None
                                } else if at != Some(c.at) {
                                    draining = Some(c.cancel("a newer speed"));
                                    None
                                } else {
                                    Some(c)
                                };
                            }
                            autorate.follow(&config);
                            let (plan, profile) = hub.plan_live(now(), autorate.live());
                            if !plan.needs_engine() {
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
                            hub.end_expired(now());
                        }
                    }
                    if let Some(f) = running.failure() {
                        break Outcome::Failed(f);
                    }
                    if check.as_ref().is_some_and(|c| c.thread.is_finished()) {
                        check.take().unwrap().finish(hub);
                    }
                    if check.is_none()
                        && down_shaped
                        && started.elapsed() >= timing.check_after
                        && draining.as_ref().is_none_or(JoinHandle::is_finished)
                        && let Some(f) = timing.check
                        && let Some(at) = hub.take_check()
                    {
                        draining = None;
                        let network = hub.config().network.map(|n| n.id);
                        check = Some(Check::start(f, at, network));
                    }
                    if let Some(s) = running.sample() {
                        hub.publish(&s, Some(running.seen()));
                        moved.add(hub.moved_bps());
                    }
                    since_rate += timing.poll;
                    if since_rate >= RATE_EVERY {
                        since_rate = Duration::ZERO;
                        hub.prune_focus();
                        if let Some(live) = autorate.tick(moved.take()) {
                            let (plan, _) = hub.plan_live(now(), live);
                            if plan.needs_engine() && running.apply(&plan).is_ok() {
                                autorate::log_live(live);
                            }
                        }
                        hub.set_ping(autorate.ping());
                    }
                    since_report += timing.poll;
                    if since_report >= timing.report_every {
                        running.report();
                        since_report = Duration::ZERO;
                    }
                };
                if let Some(c) = check.take() {
                    let at = c.at;
                    let why = match outcome {
                        Outcome::Stop => "stopping",
                        Outcome::Reload => "shaping restarts",
                        Outcome::Failed(_) => "shaping failed",
                    };
                    draining = Some(c.cancel(why));
                    // Once shaping is back (same test, same network), again.
                    if matches!(outcome, Outcome::Reload) && hub.measured_at() == Some(at) {
                        hub.retry_check(at);
                    }
                }
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

/// Run the speed test (§10.1) with nothing shaped or blocked, and save
/// its result as the line speed. A stop request cancels it.
fn measure(hub: &Hub, stop: &StopSignal, now: &dyn Fn() -> u64) -> Wake {
    hub.set_state(ServiceState::Measuring);
    info!("speed test: measuring against Cloudflare with shaping off (~20 s), then with SnailNet");
    let cancel = Arc::new(AtomicBool::new(false));
    let at = now();
    let test = {
        let cancel = cancel.clone();
        std::thread::spawn(move || speedtest::run(&cancel, at))
    };
    while !test.is_finished() {
        if stop.is_requested() {
            cancel.store(true, Ordering::Relaxed);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if stop.is_requested() {
        let _ = test.join();
        return Wake::Stop;
    }
    match test.join() {
        Ok(Ok(m)) => {
            info!(
                "speed test: down {} Mbps, up {} Mbps; ping {} ms idle, {} ms loaded without \
                 SnailNet ({} ms uploading)",
                m.down_mbps,
                m.up_mbps,
                ms(m.idle_ms),
                ms(m.loaded_ms),
                ms(m.loaded_up_ms),
            );
            if let Err((_, e)) = hub.record_measured(m) {
                warn!("speed test: could not save the result: {e}");
            }
        }
        Ok(Err(e)) => warn!("speed test failed: {e:#}"),
        Err(_) => warn!("speed test failed: it crashed"),
    }
    Wake::Reload
}

fn ms(v: Option<f64>) -> String {
    v.map_or("?".into(), |v| v.to_string())
}

/// Longest wait for a route to the internet before shaping starts anyway.
const ROUTE_WAIT: Duration = Duration::from_secs(60);
/// Once a missing route appears, let DHCP, DNS and Windows' own
/// connectivity check finish before diverting.
const ROUTE_SETTLE: Duration = Duration::from_secs(5);

/// Wait (up to [`ROUTE_WAIT`]) until Windows has a route to the internet.
/// `Wake::Timeout` at once when it already has one, `Wake::Reload` after
/// waiting (the caller plans again), `Wake::Stop` on a stop.
fn wait_for_route(hub: &Hub, stop: &StopSignal) -> Wake {
    if snailnet_win::network::route().is_some() {
        return Wake::Timeout;
    }
    hub.set_state(ServiceState::Starting);
    info!("no route to the internet yet; waiting for the network before shaping");
    let deadline = Instant::now() + ROUTE_WAIT;
    while Instant::now() < deadline {
        if stop.wait(Duration::from_secs(1)) {
            return Wake::Stop;
        }
        if snailnet_win::network::route().is_some() {
            info!("network is up; shaping in {} s", ROUTE_SETTLE.as_secs());
            return if stop.wait(ROUTE_SETTLE) {
                Wake::Stop
            } else {
                Wake::Reload
            };
        }
    }
    info!("still no route to the internet; shaping anyway");
    Wake::Reload
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

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
            autorate: false,
            watch_network: false,
            fetch_packs: false,
            check: None,
            check_after: Duration::ZERO,
        }
    }

    /// [`fast`] with a fake "with SnailNet" check.
    fn checking(f: CheckFn) -> Timing {
        Timing {
            check: Some(f),
            check_after: Duration::from_millis(5),
            ..fast()
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
        run_with_clock(d, engine, move || now, during)
    }

    /// [`run_with`] on a clock the test moves.
    fn run_with_clock(
        d: &DataDir,
        engine: Fake,
        now: impl Fn() -> u64 + Send + 'static,
        during: impl FnOnce(&Hub),
    ) -> (Result<Ended>, Fake) {
        run_timed(d, engine, fast(), now, during)
    }

    /// [`run_with_clock`] with its own timing.
    fn run_timed(
        d: &DataDir,
        engine: Fake,
        timing: Timing,
        now: impl Fn() -> u64 + Send + 'static,
        during: impl FnOnce(&Hub),
    ) -> (Result<Ended>, Fake) {
        let hub = Hub::new(d.clone(), StopSignal::new());
        let h = {
            let hub = hub.clone();
            std::thread::spawn(move || {
                let mut engine = engine;
                let r = run(&hub, &mut engine, timing, &now);
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

    #[test]
    fn a_pause_stops_shaping_until_resumed() {
        let (_t, d) = setup(true);
        let (r, f) = run_with(&d, fake(&[]), 1_000, |hub| {
            eventually(|| hub.state() == ServiceState::Shaping);
            hub.pause(None, 1_000, hub.rev()).unwrap();
            eventually(|| hub.state() == ServiceState::Paused { until: None });
            hub.resume(hub.rev()).unwrap();
            eventually(|| hub.state() == ServiceState::Shaping && hub.rev() == 3);
        });
        assert_eq!(r.unwrap(), Ended::Stopped);
        assert_eq!(
            f.starts.load(Ordering::SeqCst),
            2,
            "stopped, then started again"
        );
        assert_eq!(f.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_timed_pause_ends_by_itself() {
        let (_t, d) = setup(true);
        let clock = Arc::new(AtomicU64::new(1_000));
        let c2 = clock.clone();
        let engine = fake(&[]);
        let (r, f) = run_with_clock(
            &d,
            engine,
            move || c2.load(Ordering::SeqCst),
            |hub| {
                eventually(|| hub.state() == ServiceState::Shaping);
                hub.pause(Some(1), 1_000, hub.rev()).unwrap();
                eventually(|| hub.state() == ServiceState::Paused { until: Some(1_060) });
                // The runner sleeps until the pause ends; time jumps there, and
                // any wake-up (here a reload) finds it over.
                clock.store(1_060, Ordering::SeqCst);
                hub.signal().request_reload();
                eventually(|| hub.state() == ServiceState::Shaping);
                assert_eq!(hub.snapshot().config.pause, None, "cleared in the file too");
            },
        );
        assert_eq!(r.unwrap(), Ended::Stopped);
        assert_eq!(f.starts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn blocking_runs_the_engine_without_a_line_speed() {
        let (_t, d) = setup(false);
        let (r, f) = run_with(&d, fake(&[]), 1_000, |hub| {
            eventually(|| hub.state() == ServiceState::Unshaped);
            hub.set_blocked("Steam", true, hub.rev()).unwrap();
            eventually(|| hub.snapshot().config.blocked == ["steam"]);
            std::thread::sleep(Duration::from_millis(30));
        });
        assert_eq!(r.unwrap(), Ended::Stopped);
        let plans = f.plans.lock().unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].blocked, ["steam"]);
        assert!(!plans[0].shapes_anything());
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
        let engine = fake(&[Script::FailsOpenAfter(2), Script::Runs]);
        let starts = engine.starts.clone();
        // Stop once the retry has started, not after a fixed time.
        let (r, f) = run_with(&d, engine, 1_000, |_| {
            eventually(|| starts.load(Ordering::SeqCst) == 2);
        });
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
        // …so one fail-open now is the third strike. Wait for it rather than
        // a fixed time: on a slow machine the stop could come first.
        let (r, f) = run_with(&d, fake(&[Script::FailsOpenAfter(1)]), 300, |hub| {
            eventually(|| matches!(hub.state(), ServiceState::SafeMode { failures: 3 }));
        });
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

    fn measured(at: u64) -> snailnet_core::rules::Measured {
        snailnet_core::rules::Measured {
            at,
            down_mbps: 24.6,
            up_mbps: 4.8,
            idle_ms: Some(12.0),
            loaded_ms: Some(142.0),
            loaded_up_ms: None,
            with_ms: None,
        }
    }

    fn saved_with_ms(d: &DataDir) -> Option<f64> {
        store::load(d).unwrap().config.line.measured?.with_ms
    }

    /// Wait (up to 5 s) for the cancel flag or `release`; a check that was
    /// let go finds 21 ms.
    fn blocking_check(cancel: &AtomicBool, release: &AtomicBool) -> Result<Option<f64>> {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if cancel.load(Ordering::SeqCst) {
                anyhow::bail!("cancelled");
            }
            if release.load(Ordering::SeqCst) {
                return Ok(Some(21.0));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        anyhow::bail!("never released")
    }

    #[test]
    fn the_check_records_the_ping_with_snailnet() {
        fn check(_: &AtomicBool) -> Result<Option<f64>> {
            Ok(Some(21.0))
        }
        let (_t, d) = setup(true);
        let (r, _) = run_timed(
            &d,
            fake(&[]),
            checking(check),
            || 1_000,
            |hub| {
                eventually(|| hub.state() == ServiceState::Shaping);
                hub.record_measured(measured(1_000)).unwrap();
                eventually(|| hub.config().line.measured.and_then(|m| m.with_ms) == Some(21.0));
            },
        );
        assert_eq!(r.unwrap(), Ended::Stopped);
        assert_eq!(saved_with_ms(&d), Some(21.0));
    }

    #[test]
    fn a_stop_cancels_the_check_with_nothing_recorded() {
        static STARTED: AtomicBool = AtomicBool::new(false);
        static CANCELLED: AtomicBool = AtomicBool::new(false);
        static NEVER: AtomicBool = AtomicBool::new(false);
        fn check(cancel: &AtomicBool) -> Result<Option<f64>> {
            STARTED.store(true, Ordering::SeqCst);
            let r = blocking_check(cancel, &NEVER);
            CANCELLED.store(cancel.load(Ordering::SeqCst), Ordering::SeqCst);
            r
        }
        let (_t, d) = setup(true);
        let (r, f) = run_timed(
            &d,
            fake(&[]),
            checking(check),
            || 1_000,
            |hub| {
                eventually(|| hub.state() == ServiceState::Shaping);
                hub.record_measured(measured(1_000)).unwrap();
                eventually(|| STARTED.load(Ordering::SeqCst));
            },
        );
        assert_eq!(r.unwrap(), Ended::Stopped);
        assert_eq!(f.live.load(Ordering::SeqCst), 0);
        eventually(|| CANCELLED.load(Ordering::SeqCst));
        assert_eq!(saved_with_ms(&d), None);
    }

    #[test]
    fn a_newer_speed_wins_over_a_running_check() {
        static STARTS: AtomicUsize = AtomicUsize::new(0);
        static RELEASE: AtomicBool = AtomicBool::new(false);
        fn check(cancel: &AtomicBool) -> Result<Option<f64>> {
            STARTS.fetch_add(1, Ordering::SeqCst);
            blocking_check(cancel, &RELEASE)
        }
        let (_t, d) = setup(true);
        let (r, _) = run_timed(
            &d,
            fake(&[]),
            checking(check),
            || 1_000,
            |hub| {
                eventually(|| hub.state() == ServiceState::Shaping);
                hub.record_measured(measured(100)).unwrap();
                eventually(|| STARTS.load(Ordering::SeqCst) == 1);
                // A newer test while the first check runs: that one is cancelled,
                // and the check runs again for the newer test.
                hub.record_measured(measured(200)).unwrap();
                eventually(|| STARTS.load(Ordering::SeqCst) == 2);
                RELEASE.store(true, Ordering::SeqCst);
                eventually(|| hub.config().line.measured.and_then(|m| m.with_ms).is_some());
            },
        );
        assert_eq!(r.unwrap(), Ended::Stopped);
        let m = store::load(&d).unwrap().config.line.measured.unwrap();
        assert_eq!((m.at, m.with_ms), (200, Some(21.0)));
    }

    #[test]
    fn no_check_while_the_download_isnt_shaped() {
        static STARTS: AtomicUsize = AtomicUsize::new(0);
        fn check(_: &AtomicBool) -> Result<Option<f64>> {
            STARTS.fetch_add(1, Ordering::SeqCst);
            Ok(Some(21.0))
        }
        let (_t, d) = setup(false);
        let mut c = store::load(&d).unwrap().config;
        c.line.up_mbps = Some(10.0);
        store::save(&d, &c).unwrap();
        let (r, _) = run_timed(
            &d,
            fake(&[]),
            checking(check),
            || 1_000,
            |hub| {
                eventually(|| hub.state() == ServiceState::Shaping);
                hub.retry_check(1_000);
                std::thread::sleep(Duration::from_millis(50));
                assert_eq!(hub.take_check(), Some(1_000), "still waiting");
            },
        );
        assert_eq!(r.unwrap(), Ended::Stopped);
        assert_eq!(STARTS.load(Ordering::SeqCst), 0);
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
