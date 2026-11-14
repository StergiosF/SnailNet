//! `snailnet spike`: the Phase 0 experiment (PLAN.md §21).
//!
//! Divert traffic with WinDivert, attribute each packet to an app, queue it
//! in that app's class, and release it at the shaped line rate using DRR.
//! Ctrl+C (or a crash) closes the handles and traffic flows normally again.

use std::collections::HashMap;
use std::hash::{BuildHasher, RandomState};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, bounded};
use snailnet_core::clock::{Clock, MonotonicClock, NANOS_PER_MILLI, Nanos};
use snailnet_core::parse::ip::{self, PROTO_TCP, is_local_network};
use snailnet_core::scheduler::{
    ClassId, Dequeue, Packet, PacketMeta, Priority, Scheduler, SchedulerConfig,
};
use snailnet_engine::batch::DEFAULT_BATCH;
use snailnet_engine::windivert::{Layer, WinDivert, WinDivertLib, event, flags};
use snailnet_engine::{Address, EngineError, RecvBatch, SendBatch};
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
    /// Seconds between stats lines.
    #[arg(long, default_value_t = 1)]
    interval: u64,
    /// Stop after this many seconds (0 = until Ctrl+C).
    #[arg(long, default_value_t = 0)]
    duration: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dir {
    Down,
    Up,
}

struct Queued {
    data: Vec<u8>,
    addr: Address,
}

impl Packet for Queued {
    fn wire_len(&self) -> usize {
        self.data.len()
    }

    fn mark_ce(&mut self) -> bool {
        ip::set_ce(&mut self.data)
    }
}

struct Item {
    dir: Dir,
    meta: PacketMeta,
    packet: Queued,
}

#[derive(Default)]
struct Counters {
    by_flow: AtomicU64,
    by_port: AtomicU64,
    unattributed: AtomicU64,
    bypassed: AtomicU64,
}

/// Build one direction's scheduler. Queue management (CoDel + ECN and the
/// hard limit) is the scheduler's own (PLAN.md §9.4).
fn direction(
    rules: &[AppRule],
    line_mbps: f64,
    factor: f64,
    now: Nanos,
) -> Result<Scheduler<Queued>> {
    let rate = (line_mbps * factor * 1e6 / 8.0) as u64;
    let classes = rules::classes(rules).map_err(anyhow::Error::msg)?;
    Ok(Scheduler::new(
        SchedulerConfig {
            rate_bytes_per_sec: rate,
            // ~2 ms of line rate keeps the release smooth.
            burst_bytes: (rate / 500).max(2 * 1514),
            classes,
        },
        now,
    )?)
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

    let clock = MonotonicClock::new();
    let mut down = args
        .down
        .map(|m| direction(&args.apps, m, args.factor, clock.now()))
        .transpose()?;
    let mut up = args
        .up
        .map(|m| direction(&args.apps, m, args.factor, clock.now()))
        .transpose()?;
    let other_class = args.apps.len();

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

    let counters = Arc::new(Counters::default());
    let (tx, rx) = bounded::<Item>(16_384);

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
    let rx_thread = {
        let ctx = RxContext {
            net: net.clone(),
            attribution: attribution.clone(),
            counters: counters.clone(),
            rules: args.apps.clone(),
            other_class,
            shape_lan: args.shape_lan,
            shape_down: down.is_some(),
            shape_up: up.is_some(),
        };
        std::thread::Builder::new()
            .name("rx".into())
            .spawn(move || rx_loop(ctx, tx))?
    };

    let result = schedule_loop(
        &clock,
        &net,
        &rx,
        &mut down,
        &mut up,
        &counters,
        &attribution,
        &args,
        &stop,
    );

    // Shut down in an order that never strands a packet: stop receiving,
    // flush everything still queued, then close the handles.
    stop.store(true, Relaxed);
    net.shutdown_recv();
    flow.shutdown_recv();
    let _ = rx_thread.join();
    flush_all(&net, &rx, &mut down, &mut up);
    let _ = flow_thread.join();
    let _ = port_thread.join();
    println!("\nStopped. Handles closed; traffic flows normally again.");
    result
}

fn print_header(args: &SpikeArgs) {
    let classes = rules::classes(&args.apps).expect("validated when the schedulers were built");
    println!("SnailNet Phase 0 spike. Ctrl+C to stop (traffic returns to normal).");
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

struct RxContext {
    net: Arc<WinDivert>,
    attribution: Arc<Attribution>,
    counters: Arc<Counters>,
    rules: Vec<AppRule>,
    other_class: ClassId,
    shape_lan: bool,
    shape_down: bool,
    shape_up: bool,
}

fn rx_loop(ctx: RxContext, tx: Sender<Item>) {
    let mut batch = RecvBatch::new(DEFAULT_BATCH);
    let mut bypass = SendBatch::new(DEFAULT_BATCH);
    // PID → class, refreshed every few seconds to survive PID reuse.
    let mut pid_class: HashMap<u32, ClassId> = HashMap::new();
    let mut cache_born = Instant::now();
    // Keyed per run, so nobody on the internet can aim flows at one bucket.
    let flow_hasher = RandomState::new();

    loop {
        match ctx.net.recv(&mut batch) {
            Ok(_) => {}
            Err(EngineError::Shutdown) => break,
            Err(e) => {
                eprintln!("recv error: {e}");
                break;
            }
        }
        if cache_born.elapsed() > Duration::from_secs(5) {
            pid_class.clear();
            cache_born = Instant::now();
        }
        for (data, addr) in batch.iter() {
            let outbound = addr.outbound();
            let dir = if outbound { Dir::Up } else { Dir::Down };
            let shaped_dir = match dir {
                Dir::Down => ctx.shape_down,
                Dir::Up => ctx.shape_up,
            };
            let info = ip::parse(data);
            // Pass straight through: unparseable, unshaped direction, LAN,
            // and pure TCP control packets (SYN/ACK/FIN/RST; delaying upload
            // ACKs would slow downloads, PLAN.md §9.5).
            let bypass_it = match &info {
                None => true,
                Some(p) => {
                    !shaped_dir
                        || (!ctx.shape_lan
                            && is_local_network(if outbound { p.dst } else { p.src }))
                        || (p.protocol == PROTO_TCP && p.tcp_payload_len == Some(0))
                }
            };
            if bypass_it {
                ctx.counters.bypassed.fetch_add(1, Relaxed);
                bypass.push(data, addr);
                if bypass.is_full() {
                    send_or_log(&ctx.net, &mut bypass);
                }
                continue;
            }
            let info = info.expect("checked above");
            let key = FlowKey::from_packet(&info, outbound);
            let class = match ctx.attribution.lookup(&key) {
                Some((pid, source)) => {
                    match source {
                        Source::Flow => ctx.counters.by_flow.fetch_add(1, Relaxed),
                        Source::Port => ctx.counters.by_port.fetch_add(1, Relaxed),
                    };
                    *pid_class.entry(pid).or_insert_with(|| {
                        snailnet_win::process::exe_name(pid)
                            .and_then(|exe| ctx.rules.iter().position(|r| r.matches(&exe)))
                            .unwrap_or(ctx.other_class)
                    })
                }
                None => {
                    ctx.counters.unattributed.fetch_add(1, Relaxed);
                    ctx.other_class
                }
            };
            let meta = PacketMeta {
                prio: ctx.rules.get(class).map_or(Priority::Normal, |r| r.prio),
                ..PacketMeta::new(class, flow_hasher.hash_one(key))
            };
            let item = Item {
                dir,
                meta,
                packet: Queued {
                    data: data.to_vec(),
                    addr: *addr,
                },
            };
            if tx.send(item).is_err() {
                return;
            }
        }
        send_or_log(&ctx.net, &mut bypass);
    }
    send_or_log(&ctx.net, &mut bypass);
}

fn send_or_log(net: &WinDivert, batch: &mut SendBatch) {
    if let Err(e) = net.send(batch) {
        eprintln!("send error: {e}");
    }
}

#[allow(clippy::too_many_arguments)]
fn schedule_loop(
    clock: &MonotonicClock,
    net: &WinDivert,
    rx: &Receiver<Item>,
    down: &mut Option<Scheduler<Queued>>,
    up: &mut Option<Scheduler<Queued>>,
    counters: &Counters,
    attribution: &Attribution,
    args: &SpikeArgs,
    stop: &AtomicBool,
) -> Result<()> {
    let timer = HighResTimer::new().context("creating a high-resolution timer")?;
    let mut out = SendBatch::new(DEFAULT_BATCH);
    let started = Instant::now();
    let mut stats = Stats::new(down.as_ref(), up.as_ref());
    let mut next_stats = Instant::now() + Duration::from_secs(args.interval.max(1));

    let enqueue =
        |item: Item, down: &mut Option<Scheduler<Queued>>, up: &mut Option<Scheduler<Queued>>| {
            let s = match item.dir {
                Dir::Down => down.as_mut(),
                Dir::Up => up.as_mut(),
            };
            if let Some(s) = s {
                s.enqueue(clock.now(), item.meta, item.packet);
            }
        };

    while !stop.load(Relaxed) {
        if args.duration > 0 && started.elapsed() >= Duration::from_secs(args.duration) {
            break;
        }
        let idle =
            down.as_ref().is_none_or(|s| s.is_empty()) && up.as_ref().is_none_or(|s| s.is_empty());
        if idle {
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(item) => enqueue(item, down, up),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        for item in rx.try_iter().take(4096) {
            enqueue(item, down, up);
        }

        let now = clock.now();
        let mut wake: Option<Nanos> = None;
        for s in [down.as_mut(), up.as_mut()].into_iter().flatten() {
            loop {
                match s.dequeue(now) {
                    Dequeue::Packet { packet, .. } => {
                        out.push(&packet.data, &packet.addr);
                        if out.is_full() {
                            send_or_log(net, &mut out);
                        }
                    }
                    Dequeue::WaitUntil(t) => {
                        wake = Some(wake.map_or(t, |w| w.min(t)));
                        break;
                    }
                    Dequeue::Empty => break,
                }
            }
        }
        send_or_log(net, &mut out);

        if let Some(t) = wake {
            // Cap the nap so newly arrived packets are queued promptly.
            let nap = t.saturating_sub(clock.now()).min(NANOS_PER_MILLI);
            if nap > 0 {
                timer.sleep(Duration::from_nanos(nap));
            }
        }

        if Instant::now() >= next_stats {
            stats.print(
                started.elapsed(),
                down.as_ref(),
                up.as_ref(),
                counters,
                attribution,
            );
            next_stats += Duration::from_secs(args.interval.max(1));
        }
    }
    Ok(())
}

/// Send everything still queued, ignoring the rate, so Ctrl+C never drops.
fn flush_all(
    net: &WinDivert,
    rx: &Receiver<Item>,
    down: &mut Option<Scheduler<Queued>>,
    up: &mut Option<Scheduler<Queued>>,
) {
    let mut out = SendBatch::new(DEFAULT_BATCH);
    let mut push = |p: &Queued| {
        out.push(&p.data, &p.addr);
        if out.is_full() {
            send_or_log(net, &mut out);
        }
    };
    for s in [down.as_mut(), up.as_mut()].into_iter().flatten() {
        for (_, p) in s.drain_all() {
            push(&p);
        }
    }
    for item in rx.try_iter() {
        push(&item.packet);
    }
    send_or_log(net, &mut out);
}

struct Stats {
    last_at: Duration,
    last_down: Vec<(u64, u64)>,
    last_up: Vec<(u64, u64)>,
}

fn snapshot(s: Option<&Scheduler<Queued>>) -> Vec<(u64, u64)> {
    s.map(|s| {
        (0..s.class_count())
            .map(|i| (s.stats(i).sent_bytes, s.stats(i).dropped_packets))
            .collect()
    })
    .unwrap_or_default()
}

impl Stats {
    fn new(down: Option<&Scheduler<Queued>>, up: Option<&Scheduler<Queued>>) -> Self {
        Self {
            last_at: Duration::ZERO,
            last_down: snapshot(down),
            last_up: snapshot(up),
        }
    }

    fn print(
        &mut self,
        at: Duration,
        down: Option<&Scheduler<Queued>>,
        up: Option<&Scheduler<Queued>>,
        c: &Counters,
        attribution: &Attribution,
    ) {
        let secs = (at - self.last_at).as_secs_f64().max(1e-3);
        for (arrow, s, last) in [
            ("↓", down, &mut self.last_down),
            ("↑", up, &mut self.last_up),
        ] {
            let Some(s) = s else { continue };
            let now = snapshot(Some(s));
            let mbps: Vec<f64> = now
                .iter()
                .zip(last.iter())
                .map(|(n, l)| (n.0 - l.0) as f64 * 8.0 / secs / 1e6)
                .collect();
            let total: f64 = mbps.iter().sum();
            let drops: u64 = now.iter().zip(last.iter()).map(|(n, l)| n.1 - l.1).sum();
            let mut line = format!(
                "[{:>5.0}s] {arrow} {:>6.1}/{:.1} Mbps |",
                at.as_secs_f64(),
                total,
                s.rate_bytes_per_sec() as f64 * 8.0 / 1e6
            );
            for (i, m) in mbps.iter().enumerate() {
                let share = if total > 0.0 { m / total * 100.0 } else { 0.0 };
                line += &format!(" {} {:.1} ({:.0}%) |", s.class_name(i), m, share);
            }
            line += &format!(" drops {drops}");
            println!("{line}");
            *last = now;
        }
        let (f, p, u) = (
            c.by_flow.swap(0, Relaxed),
            c.by_port.swap(0, Relaxed),
            c.unattributed.swap(0, Relaxed),
        );
        let b = c.bypassed.swap(0, Relaxed);
        let n = (f + p + u).max(1) as f64;
        println!(
            "         who: flow {:.0}% · port {:.0}% · unknown {:.0}% · passthrough {b} pkts · {} flows tracked",
            f as f64 * 100.0 / n,
            p as f64 * 100.0 / n,
            u as f64 * 100.0 / n,
            attribution.flow_count()
        );
        self.last_at = at;
    }
}
