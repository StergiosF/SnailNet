//! The live line speed (PLAN.md §10.2, §10.4): the estimator from
//! `snailnet-core::estimator`, fed each second while shaping with what each
//! direction moved and the ping rise, and the rate it wants.
//!
//! The ping comes from a probe thread ([`Probe`]): an ICMP echo to the
//! router and to 1.1.1.1 (the user's choice, 2026-11-18) every second while
//! the line is busy, and every 30 s while it's idle, so each host has an
//! idle baseline to rise from: the lowest round trip of the last 5 minutes.
//! The rise is the larger of the two hosts'. Pings skip SnailNet's own
//! queues (the engine captures TCP and UDP only).

use std::collections::VecDeque;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use snailnet_core::estimator::{BUSY, Direction, Source, Tick};
use snailnet_core::rules::{Config, LOW_LATENCY_FACTOR_DOWN, LOW_LATENCY_FACTOR_UP};
use tracing::{info, warn};

/// The public host the probe pings besides the router.
pub const PUBLIC_HOST: Ipv4Addr = Ipv4Addr::new(1, 1, 1, 1);
/// A rate change smaller than this isn't worth applying.
const APPLY_STEP: f64 = 0.02;
/// How long a baseline remembers its lowest round trip.
const BASELINE_WINDOW: Duration = Duration::from_secs(300);
/// A ping older than this says nothing about now.
const FRESH: Duration = Duration::from_secs(3);

/// The lowest round trip of the last [`BASELINE_WINDOW`], and the latest.
#[derive(Debug, Default)]
pub struct Baseline {
    samples: VecDeque<(Instant, Duration)>,
}

impl Baseline {
    pub fn add(&mut self, at: Instant, rtt: Duration) {
        while self
            .samples
            .front()
            .is_some_and(|(t, _)| at.duration_since(*t) > BASELINE_WINDOW)
        {
            self.samples.pop_front();
        }
        self.samples.push_back((at, rtt));
    }

    /// How far the latest ping (if fresh at `now`) sits above the lowest, ms.
    pub fn rise_ms(&self, now: Instant) -> Option<f64> {
        let &(at, last) = self.samples.back()?;
        if now.duration_since(at) > FRESH {
            return None;
        }
        let low = self.samples.iter().map(|(_, r)| *r).min()?;
        Some((last - low).as_secs_f64() * 1e3)
    }
}

/// Each pinged host and its baseline.
type Hosts = Vec<(Ipv4Addr, Baseline)>;

/// The ping thread; stops when dropped.
pub struct Probe {
    busy: Arc<AtomicBool>,
    stop: Arc<(Mutex<bool>, Condvar)>,
    hosts: Arc<Mutex<Hosts>>,
    thread: Option<JoinHandle<()>>,
}

impl Probe {
    pub fn start() -> Option<Self> {
        let mut pinger = match snailnet_win::ping::Pinger::new() {
            Ok(p) => p,
            Err(e) => {
                warn!("line speed: no ping probe ({e}); keeping the typed speed");
                return None;
            }
        };
        let busy = Arc::new(AtomicBool::new(false));
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let hosts: Arc<Mutex<Hosts>> = Arc::default();
        let (b, s, h) = (busy.clone(), stop.clone(), hosts.clone());
        let thread = std::thread::Builder::new()
            .name("snailnet-probe".into())
            .spawn(move || {
                loop {
                    // The router can change (another network): ask each round.
                    let mut targets = vec![PUBLIC_HOST];
                    if let Some(gw) = snailnet_win::ping::gateway_towards(PUBLIC_HOST) {
                        targets.insert(0, gw);
                    }
                    for host in targets {
                        let rtt = pinger.ping(host, Duration::from_millis(800));
                        let mut g = h.lock().unwrap_or_else(PoisonError::into_inner);
                        let i = match g.iter().position(|(x, _)| *x == host) {
                            Some(i) => i,
                            None => {
                                g.push((host, Baseline::default()));
                                g.len() - 1
                            }
                        };
                        if let Some(rtt) = rtt {
                            g[i].1.add(Instant::now(), rtt);
                        }
                    }
                    // 1 s while busy; 30 s while idle, cut short when the
                    // line gets busy (set_busy wakes this wait).
                    let idle = !b.load(Ordering::Relaxed);
                    let every = Duration::from_secs(if idle { 30 } else { 1 });
                    let (lock, cv) = &*s;
                    let guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
                    let (stopped, _) = cv
                        .wait_timeout_while(guard, every, |stopped| {
                            !*stopped && !(idle && b.load(Ordering::Relaxed))
                        })
                        .unwrap_or_else(PoisonError::into_inner);
                    if *stopped {
                        return;
                    }
                }
            })
            .ok()?;
        Some(Self {
            busy,
            stop,
            hosts,
            thread: Some(thread),
        })
    }

    fn set_busy(&self, busy: bool) {
        let was = self.busy.swap(busy, Ordering::Relaxed);
        if busy && !was {
            // Start the 1 Hz pings now rather than after the idle wait.
            self.stop.1.notify_all();
        }
    }

    /// The larger ping rise of the hosts, ms.
    fn rise_ms(&self) -> Option<f64> {
        let now = Instant::now();
        let g = self.hosts.lock().unwrap_or_else(PoisonError::into_inner);
        g.iter()
            .filter_map(|(_, b)| b.rise_ms(now))
            .reduce(f64::max)
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        let (lock, cv) = &*self.stop;
        *lock.lock().unwrap_or_else(PoisonError::into_inner) = true;
        cv.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// What the estimators start from; a change rebuilds them.
#[derive(Debug, Clone, PartialEq)]
struct Basis {
    mbps: [Option<f64>; 2],
    factors: [Option<f64>; 2],
    pinned: bool,
}

impl Basis {
    fn of(c: &Config) -> Self {
        let factor = |default: f64| c.line.factor.or(c.settings.low_latency.then_some(default));
        Self {
            mbps: [c.line.down_mbps, c.line.up_mbps],
            factors: [
                factor(LOW_LATENCY_FACTOR_DOWN),
                factor(LOW_LATENCY_FACTOR_UP),
            ],
            pinned: c.line.is_pinned(),
        }
    }
}

/// The estimators for one shaping session, and the rates last applied.
pub struct Autorate {
    basis: Option<Basis>,
    dirs: [Option<Direction>; 2],
    /// The rate the plan runs at now, per direction (bps).
    running: [Option<f64>; 2],
    /// Whether `running` came from the estimator (else it's the typed rate).
    live: [bool; 2],
    probe: Option<Probe>,
}

impl Autorate {
    /// `probe`: whether to ping (off in tests: nothing then adapts).
    pub fn new(probe: bool) -> Self {
        Self {
            basis: None,
            dirs: [None, None],
            running: [None, None],
            live: [false, false],
            probe: if probe { Probe::start() } else { None },
        }
    }

    /// Follow the config: a new typed speed, factor or pin starts over from it.
    pub fn follow(&mut self, c: &Config) {
        let basis = Basis::of(c);
        if self.basis.as_ref() == Some(&basis) {
            return;
        }
        let source = if basis.pinned {
            Source::Fixed
        } else {
            Source::Typed
        };
        for i in 0..2 {
            self.dirs[i] =
                basis.mbps[i].map(|m| Direction::new(Some(m * 1e6), source, basis.factors[i]));
            self.running[i] = self.dirs[i].as_ref().and_then(Direction::rate_bps);
            self.live[i] = false;
        }
        self.basis = Some(basis);
    }

    /// The live rates to plan with (`[down, up]`, bps); `None` where the
    /// typed speed stands.
    pub fn live(&self) -> [Option<f64>; 2] {
        [0, 1].map(|i| self.running[i].filter(|_| self.live[i]))
    }

    /// One second: what each direction moved (bps). Returns the new live
    /// rates when one moved enough to apply.
    pub fn tick(&mut self, moved: [Option<f64>; 2]) -> Option<[Option<f64>; 2]> {
        let rise = self.probe.as_ref().and_then(Probe::rise_ms);
        self.step(moved, rise)
    }

    /// [`Autorate::tick`] with the ping rise given.
    fn step(&mut self, moved: [Option<f64>; 2], rise: Option<f64>) -> Option<[Option<f64>; 2]> {
        let mut busy = false;
        let mut changed = false;
        for (i, moved) in moved.into_iter().enumerate() {
            let Some(d) = &mut self.dirs[i] else { continue };
            let bps = moved.unwrap_or(0.0);
            busy |= self.running[i].is_some_and(|r| bps >= BUSY * r);
            d.tick(Tick { bps, rise_ms: rise });
            let (Some(want), Some(now)) = (d.rate_bps(), self.running[i]) else {
                continue;
            };
            if (want / now - 1.0).abs() >= APPLY_STEP {
                self.running[i] = Some(want);
                self.live[i] = true;
                changed = true;
            }
        }
        if let Some(p) = &self.probe {
            p.set_busy(busy);
        }
        changed.then(|| self.live())
    }
}

/// Log a rate change in words.
pub fn log_live(live: [Option<f64>; 2]) {
    let fmt = |r: Option<f64>| r.map_or("as typed".to_string(), |b| format!("{:.1} Mbps", b / 1e6));
    info!(
        "line speed: shaping at down {} · up {}",
        fmt(live[0]),
        fmt(live[1])
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_rise_is_latest_over_lowest_while_fresh() {
        let t0 = Instant::now();
        let mut b = Baseline::default();
        b.add(t0, Duration::from_millis(12));
        b.add(t0 + Duration::from_secs(1), Duration::from_millis(40));
        let r = b.rise_ms(t0 + Duration::from_secs(1)).unwrap();
        assert!((r - 28.0).abs() < 1e-9);
        assert_eq!(b.rise_ms(t0 + Duration::from_secs(10)), None, "stale");
    }

    #[test]
    fn baseline_forgets_after_five_minutes() {
        let t0 = Instant::now();
        let mut b = Baseline::default();
        b.add(t0, Duration::from_millis(5));
        let later = t0 + BASELINE_WINDOW + Duration::from_secs(1);
        b.add(later, Duration::from_millis(30));
        assert_eq!(b.rise_ms(later), Some(0.0));
    }

    fn config(down: Option<f64>, pinned: bool) -> Config {
        let mut c = Config::default();
        c.line.down_mbps = down;
        c.line.pinned = pinned.then_some(true);
        c
    }

    #[test]
    fn without_a_probe_the_typed_speed_stands() {
        let mut a = Autorate::new(false);
        a.follow(&config(Some(25.0), false));
        for _ in 0..600 {
            assert_eq!(a.tick([Some(23e6), None]), None);
        }
        assert_eq!(a.live(), [None, None]);
    }

    /// Run `secs` seconds with traffic at `share` of the current rate.
    fn drive(a: &mut Autorate, secs: u32, share: f64, rise: f64) -> Vec<[Option<f64>; 2]> {
        (0..secs)
            .filter_map(|_| {
                let rate = a.running[0].unwrap();
                a.step([Some(share * rate), None], Some(rise))
            })
            .collect()
    }

    #[test]
    fn calm_pings_under_load_raise_the_rate_in_steps() {
        let mut a = Autorate::new(false);
        a.follow(&config(Some(25.0), false));
        let typed = a.running[0].unwrap();
        let changes = drive(&mut a, 120, 1.0, 1.0);
        assert!(!changes.is_empty());
        let last = changes.last().unwrap()[0].unwrap();
        assert!(last > typed * 1.1 && last < typed * 1.25, "{last}");
        // Each applied step is at least 2%.
        for w in changes.windows(2) {
            assert!(w[1][0].unwrap() / w[0][0].unwrap() >= 1.0 + APPLY_STEP - 1e-9);
        }
    }

    #[test]
    fn a_ping_jump_under_load_lowers_the_rate() {
        let mut a = Autorate::new(false);
        a.follow(&config(Some(25.0), false));
        let typed = a.running[0].unwrap();
        drive(&mut a, 10, 1.0, 80.0);
        assert!(a.live()[0].unwrap() < typed * 0.7);
    }

    #[test]
    fn a_pinned_speed_stays_put() {
        let mut a = Autorate::new(false);
        a.follow(&config(Some(25.0), true));
        assert!(drive(&mut a, 120, 1.0, 80.0).is_empty());
        assert_eq!(a.live(), [None, None]);
    }

    #[test]
    fn a_new_typed_speed_starts_over() {
        let mut a = Autorate::new(false);
        a.follow(&config(Some(25.0), false));
        a.running[0] = Some(20e6);
        a.live[0] = true;
        a.follow(&config(Some(25.0), false));
        assert_eq!(a.live(), [Some(20e6), None], "same basis: kept");
        a.follow(&config(Some(50.0), false));
        assert_eq!(a.live(), [None, None]);
    }
}
