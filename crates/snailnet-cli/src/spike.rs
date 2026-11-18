//! `snailnet spike`: the Phase 0 experiment (PLAN.md §21), now running on
//! the service's shaper (`snailnet_service::shaper`).
//!
//! Divert traffic with WinDivert, attribute each packet to an app, queue it
//! in that app's class, and release it at the shaped line rate. Ctrl+C (or a
//! crash, or a stalled scheduler) closes the handles and traffic flows
//! normally again.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use snailnet_core::classify::App;
use snailnet_core::scheduler::{ClassId, Priority, SchedulerConfig};
use snailnet_engine::fastpath::{Counters, DirectionStats, Snapshot};
use snailnet_service::shaper::{Rules, Shaper, Spec};

use crate::rules::{self, AppRule};

#[derive(clap::Args, Debug)]
pub struct SpikeArgs {
    /// App rule APP[,APP…]=PERCENT[:high|normal|low], repeatable. APP is a
    /// family (steam, brave, windows-update: see assets/families.toml) or an
    /// exe (game.exe). Example: --app steam=40:low --app brave=60
    #[arg(long = "app", value_name = "APP=PERCENT", value_parser = rules::parse_rule, required = true)]
    apps: Vec<AppRule>,
    /// Your line's download speed in Mbps. Turns on download shaping.
    #[arg(long, value_name = "MBPS")]
    down: Option<f64>,
    /// Your line's upload speed in Mbps. Turns on upload shaping.
    #[arg(long, value_name = "MBPS")]
    up: Option<f64>,
    /// Shape at this fraction of the line so the router's buffer stays empty
    /// (Low-latency mode, PLAN.md §10.4). Use 1.0 to shape at full speed.
    #[arg(long, default_value_t = 0.92)]
    factor: f64,
    /// Also shape traffic to your local network (printers, NAS, casting).
    #[arg(long)]
    shape_lan: bool,
    /// Receive threads (1 or 2). 2 can reorder packets within a flow.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=2))]
    rx_threads: u8,
    /// Seconds between stats lines.
    #[arg(long, default_value_t = 1)]
    interval: u64,
    /// Stop after this many seconds (0 = until Ctrl+C).
    #[arg(long, default_value_t = 0)]
    duration: u64,
}

/// One direction's config. Queue management (CoDel + ECN and the hard
/// limit) is the scheduler's own (PLAN.md §9.4).
fn direction(rules: &[AppRule], line_mbps: f64, factor: f64) -> Result<SchedulerConfig> {
    let rate = (line_mbps * factor * 1e6 / 8.0) as u64;
    Ok(SchedulerConfig {
        rate_bytes_per_sec: rate,
        // ~2 ms of line rate keeps the release smooth.
        burst_bytes: (rate / 500).max(2 * 1514),
        classes: rules::classes(rules).map_err(anyhow::Error::msg)?,
    })
}

/// `--app` rules in order, then Everything else.
struct AppRules(Vec<AppRule>);

impl Rules for AppRules {
    fn class_of(&self, app: &App) -> ClassId {
        self.0
            .iter()
            .position(|r| r.matches(app))
            .unwrap_or(self.0.len())
    }

    fn prio_of(&self, class: ClassId) -> Priority {
        self.0.get(class).map_or(Priority::Normal, |r| r.prio)
    }

    fn other_class(&self) -> ClassId {
        self.0.len()
    }
}

pub fn run(args: SpikeArgs) -> Result<()> {
    if args.down.is_none() && args.up.is_none() {
        bail!("give --down and/or --up (your line speed in Mbps) so there is something to shape");
    }
    if !(0.5..=1.0).contains(&args.factor) {
        bail!("--factor must be between 0.5 and 1.0");
    }
    for v in [args.down, args.up].into_iter().flatten() {
        if !(0.1..=10_000.0).contains(&v) {
            bail!("line speed {v} Mbps is out of range");
        }
    }
    let down = args
        .down
        .map(|m| direction(&args.apps, m, args.factor))
        .transpose()?;
    let up = args
        .up
        .map(|m| direction(&args.apps, m, args.factor))
        .transpose()?;

    let shaper = Shaper::start(Spec {
        down,
        up,
        rules: Arc::new(AppRules(args.apps.clone())),
        shape_lan: args.shape_lan,
        blocking: false,
        rx_threads: args.rx_threads.into(),
        families: Default::default(),
    })?;

    print_header(&args);

    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = stop.clone();
        ctrlc::set_handler(move || stop.store(true, Relaxed))
            .context("installing the Ctrl+C handler")?;
    }

    report_until_stopped(&shaper, &args, &stop);
    // Stops receiving, flushes every queued packet, closes every handle.
    let failure = shaper.stop();
    println!("\nStopped. Handles closed; traffic flows normally again.");
    match failure {
        Some(reason) => bail!("failed open: {reason}"),
        None => Ok(()),
    }
}

fn print_header(args: &SpikeArgs) {
    let classes = rules::classes(&args.apps).expect("validated when the configs were built");
    println!("SnailNet spike. Ctrl+C to stop (traffic returns to normal).");
    for (dir, line) in [("down", args.down), ("up", args.up)] {
        if let Some(m) = line {
            println!(
                "  {dir:>4}: line {m:.1} Mbps → shaped at {:.1} Mbps (factor {:.2})",
                m * args.factor,
                args.factor
            );
        }
    }
    for c in &classes {
        println!(
            "  slice {:>5.1}%  {} ({:?})",
            c.share_bp as f64 / 100.0,
            c.name,
            c.prio
        );
    }
    println!();
}

/// Print a stats line every `--interval` until Ctrl+C, `--duration`, or the
/// fast path fails open.
fn report_until_stopped(shaper: &Shaper, args: &SpikeArgs, stop: &AtomicBool) {
    let started = Instant::now();
    let every = Duration::from_secs(args.interval.max(1));
    let mut next = started + every;
    let mut stats = Stats::new(&shaper.snapshot(), shaper.counters());
    while !stop.load(Relaxed) && shaper.failure().is_none() {
        if args.duration > 0 && started.elapsed() >= Duration::from_secs(args.duration) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
        if Instant::now() >= next {
            stats.print(started.elapsed(), shaper);
            next += every;
        }
    }
}

/// (sent bytes, dropped packets) per class.
type Totals = Vec<(u64, u64)>;

fn totals(d: Option<&DirectionStats>) -> Totals {
    d.map(|d| {
        d.classes
            .iter()
            .map(|(_, s)| (s.sent_bytes, s.dropped_packets))
            .collect()
    })
    .unwrap_or_default()
}

struct Stats {
    /// When the engine took the last snapshot we read (rates are between
    /// these, not between our own 1 s prints).
    last_taken: Option<Instant>,
    last_down: Totals,
    last_up: Totals,
    last_counters: Counters,
}

impl Stats {
    fn new(snap: &Snapshot, counters: Counters) -> Self {
        Self {
            last_taken: snap.taken,
            last_down: totals(snap.down.as_ref()),
            last_up: totals(snap.up.as_ref()),
            last_counters: counters,
        }
    }

    fn print(&mut self, at: Duration, shaper: &Shaper) {
        let snap = shaper.snapshot();
        let taken = snap.taken.unwrap_or_else(Instant::now);
        let secs = match self.last_taken {
            Some(last) => taken.saturating_duration_since(last).as_secs_f64(),
            // Nothing published before we started: count from our start.
            None => at.as_secs_f64(),
        }
        .max(1e-3);
        for (arrow, d, last) in [
            ("↓", snap.down.as_ref(), &mut self.last_down),
            ("↑", snap.up.as_ref(), &mut self.last_up),
        ] {
            let Some(d) = d else { continue };
            let now = totals(Some(d));
            // The first snapshot may land after we started counting.
            last.resize(now.len(), (0, 0));
            let mbps: Vec<f64> = now
                .iter()
                .zip(last.iter())
                .map(|(n, l)| n.0.saturating_sub(l.0) as f64 * 8.0 / secs / 1e6)
                .collect();
            let total: f64 = mbps.iter().sum();
            let drops: u64 = now
                .iter()
                .zip(last.iter())
                .map(|(n, l)| n.1.saturating_sub(l.1))
                .sum();
            let mut line = format!(
                "[{:>5.0}s] {arrow} {:>6.1}/{:.1} Mbps |",
                at.as_secs_f64(),
                total,
                d.rate_bytes_per_sec as f64 * 8.0 / 1e6
            );
            for ((name, _), m) in d.classes.iter().zip(&mbps) {
                let share = if total > 0.0 { m / total * 100.0 } else { 0.0 };
                line += &format!(" {name} {m:.1} ({share:.0}%) |");
            }
            line += &format!(" drops {drops}");
            println!("{line}");
            *last = now;
        }
        let who = shaper.who();
        let take = |c: &AtomicU64| c.swap(0, Relaxed);
        let counts = [
            ("flow", take(&who.by_flow)),
            ("socket", take(&who.by_socket)),
            ("port", take(&who.by_port)),
            ("pending", take(&who.pending)),
            ("unknown", take(&who.unknown)),
        ];
        let n = counts.iter().map(|(_, v)| v).sum::<u64>().max(1) as f64;
        let c = shaper.counters();
        let l = self.last_counters;
        let mut line = String::from("         who:");
        for (name, v) in counts {
            line += &format!(" {name} {:.0}% ·", v as f64 * 100.0 / n);
        }
        let attribution = shaper.attribution();
        let (flows, sockets, ports, full) = attribution.sizes();
        line += &format!(
            " late flows {} · passthrough {} pkts · tracking {flows} flows, {sockets} sockets, {ports} ports, {} apps",
            take(&who.late_flows),
            c.bypassed - l.bypassed,
            attribution.app_count()
        );
        if full > 0 {
            line += &format!(" · TABLE FULL, skipped {full}");
        }
        let overflow = (c.pool_exhausted - l.pool_exhausted) + (c.channel_full - l.channel_full);
        if overflow > 0 {
            line += &format!(" · overflow passthrough {overflow} pkts");
        }
        if c.send_errors > l.send_errors {
            line += &format!(" · SEND ERRORS {}", c.send_errors - l.send_errors);
        }
        println!("{line}");
        self.last_counters = c;
        self.last_taken = Some(taken);
    }
}
