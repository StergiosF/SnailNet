//! `snailnet spike`: the Phase 0 experiment (PLAN.md §21), now running on
//! the Phase 1 fast path (`snailnet_engine::fastpath`).
//!
//! Divert traffic with WinDivert, attribute each packet to an app, queue it
//! in that app's class, and release it at the shaped line rate. Ctrl+C (or a
//! crash, or a stalled scheduler) closes the handles and traffic flows
//! normally again.

use std::collections::HashMap;
use std::hash::{BuildHasher, RandomState};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use snailnet_core::parse::ip::PacketInfo;
use snailnet_core::scheduler::{ClassId, PacketMeta, Priority, SchedulerConfig};
use snailnet_engine::RecvBatch;
use snailnet_engine::batch::DEFAULT_BATCH;
use snailnet_engine::fastpath::{
    Classifier, Counters, DirectionStats, FastPath, FastPathConfig, Nap, Snapshot,
};
use snailnet_engine::windivert::{Layer, WinDivert, WinDivertLib, event, flags};
use snailnet_win::timer::HighResTimer;

use crate::attrib::{Attribution, FlowKey, Source};
use crate::rules::{self, AppRule};

#[derive(clap::Args, Debug)]
pub struct SpikeArgs {
    /// App rule EXE[,EXE…]=PERCENT[:high|normal|low], repeatable. Example:
    /// --app steam.exe,steamwebhelper.exe=40:low --app brave.exe=60
    #[arg(long = "app", value_name = "EXE=PERCENT", value_parser = rules::parse_rule, required = true)]
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

/// How packets were attributed to an app.
#[derive(Default)]
struct WhoCounters {
    by_flow: AtomicU64,
    by_port: AtomicU64,
    unattributed: AtomicU64,
}

/// Packet → app → rule. Each rx thread has its own clone (and PID cache).
#[derive(Clone)]
struct AppClassifier {
    attribution: Arc<Attribution>,
    who: Arc<WhoCounters>,
    rules: Arc<[AppRule]>,
    other_class: ClassId,
    /// PID → class, refreshed every few seconds to survive PID reuse.
    pid_class: HashMap<u32, ClassId>,
    cache_born: Instant,
    /// Keyed per run, so nobody on the internet can aim flows at one bucket.
    /// Clones share the keys, so every rx thread hashes a flow the same.
    flow_hasher: RandomState,
}

impl Classifier for AppClassifier {
    fn classify(&mut self, info: &PacketInfo, outbound: bool) -> PacketMeta {
        if self.cache_born.elapsed() > Duration::from_secs(5) {
            self.pid_class.clear();
            self.cache_born = Instant::now();
        }
        let key = FlowKey::from_packet(info, outbound);
        let class = match self.attribution.lookup(&key) {
            Some((pid, source)) => {
                match source {
                    Source::Flow => self.who.by_flow.fetch_add(1, Relaxed),
                    Source::Port => self.who.by_port.fetch_add(1, Relaxed),
                };
                let (rules, other) = (&self.rules, self.other_class);
                *self.pid_class.entry(pid).or_insert_with(|| {
                    snailnet_win::process::exe_name(pid)
                        .and_then(|exe| rules.iter().position(|r| r.matches(&exe)))
                        .unwrap_or(other)
                })
            }
            None => {
                self.who.unattributed.fetch_add(1, Relaxed);
                self.other_class
            }
        };
        PacketMeta {
            prio: self.rules.get(class).map_or(Priority::Normal, |r| r.prio),
            ..PacketMeta::new(class, self.flow_hasher.hash_one(key))
        }
    }
}

struct TimerNap(HighResTimer);

impl Nap for TimerNap {
    fn nap(&mut self, d: Duration) {
        self.0.sleep(d);
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

    let lib = WinDivertLib::load()?;
    let mut filter = String::from(
        "!loopback and !impostor and (tcp or udp) and remotePort != 53 and localPort != 53",
    );
    match (down.is_some(), up.is_some()) {
        (true, false) => filter.push_str(" and inbound"),
        (false, true) => filter.push_str(" and outbound"),
        _ => {}
    }

    // Open the sniff-only FLOW handle first so no new flow is missed.
    let flow = Arc::new(
        WinDivert::open(
            &lib,
            "!loopback and (tcp or udp)",
            Layer::Flow,
            0,
            flags::SNIFF | flags::RECV_ONLY,
        )
        .context("opening the FLOW layer (PID mapping)")?,
    );
    let attribution = Arc::new(Attribution::default());
    attribution.set_ports(snailnet_win::ports::snapshot());
    let net = Arc::new(
        WinDivert::open(&lib, &filter, Layer::Network, 0, 0)
            .context("opening the NETWORK layer")?,
    );

    print_header(&args);

    let stop = Arc::new(AtomicBool::new(false));
    {
        let (stop, net, flow) = (stop.clone(), net.clone(), flow.clone());
        ctrlc::set_handler(move || {
            stop.store(true, Relaxed);
            net.shutdown_recv();
            flow.shutdown_recv();
        })
        .context("installing the Ctrl+C handler")?;
    }

    let flow_thread = {
        let (flow, attribution) = (flow.clone(), attribution.clone());
        std::thread::Builder::new()
            .name("flow-sniffer".into())
            .spawn(move || flow_loop(&flow, &attribution))?
    };
    let port_thread = {
        let (attribution, stop) = (attribution.clone(), stop.clone());
        std::thread::Builder::new()
            .name("port-table".into())
            .spawn(move || {
                while !stop.load(Relaxed) {
                    std::thread::sleep(Duration::from_secs(1));
                    attribution.set_ports(snailnet_win::ports::snapshot());
                }
            })?
    };

    let who = Arc::new(WhoCounters::default());
    let classifier = AppClassifier {
        attribution: attribution.clone(),
        who: who.clone(),
        rules: args.apps.clone().into(),
        other_class: args.apps.len(),
        pid_class: HashMap::new(),
        cache_born: Instant::now(),
        flow_hasher: RandomState::new(),
    };
    let timer = HighResTimer::new().context("creating a high-resolution timer")?;
    let fast_path = FastPath::start(
        net.clone(),
        down,
        up,
        classifier,
        Box::new(TimerNap(timer)),
        FastPathConfig {
            rx_threads: args.rx_threads.into(),
            shape_lan: args.shape_lan,
            ..FastPathConfig::default()
        },
    );

    let failure = match fast_path {
        Ok(fp) => {
            report_until_stopped(&fp, &args, &stop, &who, &attribution);
            // Stops receiving, flushes every queued packet, joins threads.
            fp.stop()
        }
        Err(e) => Some(format!("could not start shaping: {e}")),
    };

    stop.store(true, Relaxed);
    flow.shutdown_recv();
    let _ = flow_thread.join();
    let _ = port_thread.join();
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

fn flow_loop(flow: &WinDivert, attribution: &Attribution) {
    let mut batch = RecvBatch::new(DEFAULT_BATCH);
    while flow.recv(&mut batch).is_ok() {
        for a in batch.addresses() {
            let f = a.flow();
            let key = FlowKey::new(
                f.protocol,
                (f.local_ip(), f.local_port),
                (f.remote_ip(), f.remote_port),
            );
            match a.event() {
                event::FLOW_ESTABLISHED => attribution.flow_established(key, f.process_id),
                event::FLOW_DELETED => attribution.flow_deleted(&key),
                _ => {}
            }
        }
    }
}

/// Print a stats line every `--interval` until Ctrl+C, `--duration`, or the
/// fast path fails open.
fn report_until_stopped(
    fp: &FastPath<WinDivert>,
    args: &SpikeArgs,
    stop: &AtomicBool,
    who: &WhoCounters,
    attribution: &Attribution,
) {
    let started = Instant::now();
    let every = Duration::from_secs(args.interval.max(1));
    let mut next = started + every;
    let mut stats = Stats::new(&fp.snapshot(), fp.counters());
    while !stop.load(Relaxed) && fp.failure().is_none() {
        if args.duration > 0 && started.elapsed() >= Duration::from_secs(args.duration) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
        if Instant::now() >= next {
            stats.print(
                started.elapsed(),
                &fp.snapshot(),
                fp.counters(),
                who,
                attribution,
            );
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
    last_at: Duration,
    last_down: Totals,
    last_up: Totals,
    last_counters: Counters,
}

impl Stats {
    fn new(snap: &Snapshot, counters: Counters) -> Self {
        Self {
            last_at: Duration::ZERO,
            last_down: totals(snap.down.as_ref()),
            last_up: totals(snap.up.as_ref()),
            last_counters: counters,
        }
    }

    fn print(
        &mut self,
        at: Duration,
        snap: &Snapshot,
        counters: Counters,
        who: &WhoCounters,
        attribution: &Attribution,
    ) {
        let secs = (at - self.last_at).as_secs_f64().max(1e-3);
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
        let (f, p, u) = (
            who.by_flow.swap(0, Relaxed),
            who.by_port.swap(0, Relaxed),
            who.unattributed.swap(0, Relaxed),
        );
        let n = (f + p + u).max(1) as f64;
        let c = counters;
        let l = self.last_counters;
        let mut line = format!(
            "         who: flow {:.0}% · port {:.0}% · unknown {:.0}% · passthrough {} pkts · {} flows tracked",
            f as f64 * 100.0 / n,
            p as f64 * 100.0 / n,
            u as f64 * 100.0 / n,
            c.bypassed - l.bypassed,
            attribution.flow_count()
        );
        let overflow = (c.pool_exhausted - l.pool_exhausted) + (c.channel_full - l.channel_full);
        if overflow > 0 {
            line += &format!(" · overflow passthrough {overflow} pkts");
        }
        if c.send_errors > l.send_errors {
            line += &format!(" · SEND ERRORS {}", c.send_errors - l.send_errors);
        }
        println!("{line}");
        self.last_counters = c;
        self.last_at = at;
    }
}
