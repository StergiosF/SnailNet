//! WinDivert + classification + the fast path, wired together.
//!
//! - **Event threads** (PLAN.md §4: `flow-sniffer`, `socket-sniffer`,
//!   sniff-only for now; SOCKET blocking arrives with Block per app in
//!   Phase 5) keep the shared [`WinAttribution`] current:
//!   - `flow-sniffer`: FLOW established/deleted (exact 5-tuple → PID),
//!   - `socket-sniffer`: SOCKET connect/accept/bind/listen/close, which
//!     usually arrive *before* the flow's first packet,
//!   - `port-table`: the IP Helper snapshot once a second, for connections
//!     that were already open when SnailNet started.
//! - **[`AppClassifier`]**: packet → app → class, for the rx threads.
//! - **[`Shaper`]**: opens everything in the right order and closes it again;
//!   used by the service ([`WinEngine`]) and by `snailnet spike`.

use std::collections::HashMap;
use std::hash::{BuildHasher, RandomState};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use snailnet_core::classify::{
    App, AppId, Attribution, Event, FlowKey, Miss, Pack, Pending, Resolver, Source,
};
use snailnet_core::clock::{Clock, MonotonicClock};
use snailnet_core::parse::ip::PacketInfo;
use snailnet_core::rules::Plan;
use snailnet_core::scheduler::{ClassId, PacketMeta, Priority, SchedulerConfig};
use snailnet_engine::batch::DEFAULT_BATCH;
use snailnet_engine::fastpath::{
    Classifier, Counters, DirectionStats, FastPath, FastPathConfig, NO_TAG, Nap, Snapshot, Tag,
};
use snailnet_engine::windivert::{Layer, WinDivert, WinDivertLib, event, flags};
use snailnet_engine::{Address, RecvBatch};
use snailnet_win::process::{WinProcesses, windows_dir};
use snailnet_win::timer::HighResTimer;

use crate::runner;

pub type WinAttribution = Attribution<WinProcesses>;

pub fn new_attribution() -> WinAttribution {
    let resolver = Resolver::new(Pack::builtin(), &windows_dir());
    Attribution::new(resolver, WinProcesses::new())
}

/// A FLOW or SOCKET address → classify event. `None` for anything else.
pub fn decode(a: &Address) -> Option<Event> {
    let f = a.flow();
    let key = FlowKey::new(
        f.protocol,
        (f.local_ip(), f.local_port),
        (f.remote_ip(), f.remote_port),
    );
    let (pid, endpoint) = (f.process_id, f.endpoint_id);
    Some(match a.event() {
        event::FLOW_ESTABLISHED => Event::FlowEstablished { key, pid },
        event::FLOW_DELETED => Event::FlowDeleted { key },
        event::SOCKET_CONNECT | event::SOCKET_ACCEPT => Event::Connected { key, pid, endpoint },
        event::SOCKET_BIND | event::SOCKET_LISTEN => Event::Bound { key, pid, endpoint },
        event::SOCKET_CLOSE => Event::Closed { key, endpoint },
        _ => return None,
    })
}

const EVENT_FILTER: &str = "!loopback and (tcp or udp)";
const PORT_TABLE_EVERY: Duration = Duration::from_secs(1);

/// The running event threads. Open them *before* the NETWORK handle so no
/// new flow is missed.
pub struct Sniffers {
    flow: Arc<WinDivert>,
    socket: Arc<WinDivert>,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl Sniffers {
    pub fn start(lib: &Arc<WinDivertLib>, attribution: Arc<WinAttribution>) -> Result<Self> {
        let sniff = flags::SNIFF | flags::RECV_ONLY;
        let flow = Arc::new(
            WinDivert::open(lib, EVENT_FILTER, Layer::Flow, 0, sniff)
                .context("opening the FLOW layer (PID mapping)")?,
        );
        let socket = Arc::new(
            WinDivert::open(lib, EVENT_FILTER, Layer::Socket, 0, sniff)
                .context("opening the SOCKET layer (early PID mapping)")?,
        );
        // The snapshot covers what was open before the handles above.
        attribution.set_ports(&snailnet_win::ports::snapshot());
        let stop = Arc::new(AtomicBool::new(false));
        let mut sniffers = Self {
            flow,
            socket,
            stop,
            threads: Vec::new(),
        };
        // On a failed spawn, dropping `sniffers` stops what did start.
        for name in ["flow-sniffer", "socket-sniffer"] {
            let handle = if name == "flow-sniffer" {
                sniffers.flow.clone()
            } else {
                sniffers.socket.clone()
            };
            let attribution = attribution.clone();
            sniffers.threads.push(
                std::thread::Builder::new()
                    .name(name.into())
                    .spawn(move || event_loop(&handle, &attribution))?,
            );
        }
        let stop = sniffers.stop.clone();
        sniffers.threads.push(
            std::thread::Builder::new()
                .name("port-table".into())
                .spawn(move || {
                    let mut next = Instant::now() + PORT_TABLE_EVERY;
                    while !stop.load(Relaxed) {
                        std::thread::sleep(Duration::from_millis(100));
                        if Instant::now() >= next {
                            attribution.set_ports(&snailnet_win::ports::snapshot());
                            next += PORT_TABLE_EVERY;
                        }
                    }
                })?,
        );
        Ok(sniffers)
    }

    /// Ask every thread to stop. Safe from a Ctrl+C handler.
    pub fn stopper(&self) -> impl Fn() + Send + 'static {
        let (flow, socket, stop) = (self.flow.clone(), self.socket.clone(), self.stop.clone());
        move || {
            stop.store(true, Relaxed);
            flow.shutdown_recv();
            socket.shutdown_recv();
        }
    }

    pub fn stop(self) {
        // Drop does the work.
    }
}

impl Drop for Sniffers {
    fn drop(&mut self) {
        (self.stopper())();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

fn event_loop(handle: &WinDivert, attribution: &WinAttribution) {
    let mut batch = RecvBatch::new(DEFAULT_BATCH);
    while handle.recv(&mut batch).is_ok() {
        for a in batch.addresses() {
            if let Some(ev) = decode(a) {
                attribution.apply(ev);
            }
        }
    }
}

/// Which class an app's traffic goes to.
pub trait Rules: Send + Sync + 'static {
    fn class_of(&self, app: &App) -> ClassId;
    fn prio_of(&self, class: ClassId) -> Priority;
    /// Where unattributed traffic goes: Everything else.
    fn other_class(&self) -> ClassId;
}

impl Rules for Plan {
    fn class_of(&self, app: &App) -> ClassId {
        Plan::class_of(self, app)
    }

    fn prio_of(&self, class: ClassId) -> Priority {
        Plan::prio_of(self, class)
    }

    fn other_class(&self) -> ClassId {
        Plan::other_class(self)
    }
}

/// How packets were attributed to an app (packet counts), plus flows whose
/// first packets beat their FLOW/SOCKET event. Readers `swap` them to 0.
#[derive(Default)]
pub struct WhoCounters {
    pub by_flow: AtomicU64,
    pub by_socket: AtomicU64,
    pub by_port: AtomicU64,
    pub pending: AtomicU64,
    pub unknown: AtomicU64,
    pub late_flows: AtomicU64,
}

/// Packet → app → class. Each rx thread has its own clone (caches and
/// pending list); the attribution behind it is shared.
pub struct AppClassifier<R> {
    attribution: Arc<WinAttribution>,
    who: Arc<WhoCounters>,
    rules: Arc<R>,
    /// App ids are never reused (a reused PID is a new process key and is
    /// resolved afresh), so this needs no expiry. One entry per app.
    app_class: HashMap<AppId, ClassId>,
    pending: Pending,
    clock: Arc<MonotonicClock>,
    /// Keyed per run, so nobody on the internet can aim flows at one bucket.
    /// Clones share the keys, so every rx thread hashes a flow the same.
    flow_hasher: RandomState,
}

impl<R> Clone for AppClassifier<R> {
    fn clone(&self) -> Self {
        Self {
            attribution: self.attribution.clone(),
            who: self.who.clone(),
            rules: self.rules.clone(),
            app_class: self.app_class.clone(),
            pending: self.pending.clone(),
            clock: self.clock.clone(),
            flow_hasher: self.flow_hasher.clone(),
        }
    }
}

impl<R: Rules> AppClassifier<R> {
    fn class_of(&mut self, app: AppId) -> ClassId {
        let (rules, attribution) = (&self.rules, &self.attribution);
        *self
            .app_class
            .entry(app)
            .or_insert_with(|| rules.class_of(&attribution.app(app)))
    }
}

impl<R: Rules> Classifier for AppClassifier<R> {
    /// The tag is the packet's [`AppId`] ([`NO_TAG`] when unknown), so
    /// per-tag byte counts are per-app.
    fn classify(&mut self, info: &PacketInfo, outbound: bool) -> (PacketMeta, Tag) {
        let key = FlowKey::from_packet(info, outbound);
        let who = self.who.clone();
        let (class, tag) = match self.attribution.lookup(&key) {
            Some((owner, source)) => {
                if self.pending.hit(&key) {
                    who.late_flows.fetch_add(1, Relaxed);
                }
                match source {
                    Source::Flow => who.by_flow.fetch_add(1, Relaxed),
                    Source::Socket => who.by_socket.fetch_add(1, Relaxed),
                    Source::Port => who.by_port.fetch_add(1, Relaxed),
                };
                (self.class_of(owner.app), owner.app)
            }
            None => {
                match self.pending.miss(key, self.clock.now()) {
                    Miss::Pending => who.pending.fetch_add(1, Relaxed),
                    Miss::Unknown => who.unknown.fetch_add(1, Relaxed),
                };
                (self.rules.other_class(), NO_TAG)
            }
        };
        let meta = PacketMeta {
            prio: self.rules.prio_of(class),
            ..PacketMeta::new(class, self.flow_hasher.hash_one(key))
        };
        (meta, tag)
    }
}

struct TimerNap(HighResTimer);

impl Nap for TimerNap {
    fn nap(&mut self, d: Duration) {
        self.0.sleep(d);
    }
}

/// What to shape and how.
pub struct Spec<R> {
    /// `None`: that direction passes straight through.
    pub down: Option<SchedulerConfig>,
    pub up: Option<SchedulerConfig>,
    pub rules: Arc<R>,
    pub shape_lan: bool,
    /// 1 or 2. 2 can reorder packets within a flow.
    pub rx_threads: usize,
}

/// Running shaping. Dropping it (or [`Shaper::stop`]) closes every handle,
/// after flushing what it holds, and traffic flows normally again.
pub struct Shaper {
    fast_path: Option<FastPath<WinDivert>>,
    sniffers: Option<Sniffers>,
    who: Arc<WhoCounters>,
    attribution: Arc<WinAttribution>,
}

impl Shaper {
    pub fn start<R: Rules>(spec: Spec<R>) -> Result<Self> {
        let lib = WinDivertLib::load()?;
        let mut filter = String::from(
            "!loopback and !impostor and (tcp or udp) and remotePort != 53 and localPort != 53",
        );
        match (spec.down.is_some(), spec.up.is_some()) {
            (true, false) => filter.push_str(" and inbound"),
            (false, true) => filter.push_str(" and outbound"),
            _ => {}
        }
        let timer = HighResTimer::new().context("creating a high-resolution timer")?;

        // Sniffers first, so no new flow is missed. If anything below fails,
        // dropping them closes their handles.
        let attribution = Arc::new(new_attribution());
        let sniffers = Sniffers::start(&lib, attribution.clone())?;
        let net = Arc::new(
            WinDivert::open(&lib, &filter, Layer::Network, 0, 0)
                .context("opening the NETWORK layer")?,
        );
        let who = Arc::new(WhoCounters::default());
        let classifier = AppClassifier {
            attribution: attribution.clone(),
            who: who.clone(),
            rules: spec.rules,
            app_class: HashMap::new(),
            pending: Pending::default(),
            clock: Arc::new(MonotonicClock::new()),
            flow_hasher: RandomState::new(),
        };
        let fast_path = FastPath::start(
            net,
            spec.down,
            spec.up,
            classifier,
            Box::new(TimerNap(timer)),
            FastPathConfig {
                rx_threads: spec.rx_threads,
                shape_lan: spec.shape_lan,
                ..FastPathConfig::default()
            },
        )
        .context("starting the fast path")?;
        Ok(Self {
            fast_path: Some(fast_path),
            sniffers: Some(sniffers),
            who,
            attribution,
        })
    }

    fn fp(&self) -> &FastPath<WinDivert> {
        self.fast_path.as_ref().expect("present until stop")
    }

    /// Why shaping failed open, if it did (traffic is flowing unshaped).
    pub fn failure(&self) -> Option<String> {
        self.fp().failure()
    }

    pub fn snapshot(&self) -> Snapshot {
        self.fp().snapshot()
    }

    pub fn counters(&self) -> Counters {
        self.fp().counters()
    }

    pub fn pool_available(&self) -> (usize, usize) {
        self.fp().pool_available()
    }

    pub fn who(&self) -> &WhoCounters {
        &self.who
    }

    pub fn attribution(&self) -> &WinAttribution {
        &self.attribution
    }

    /// Stop: flush queued packets, close every handle, join every thread.
    /// Returns the fail-open reason, if there was one.
    pub fn stop(mut self) -> Option<String> {
        self.halt()
    }

    fn halt(&mut self) -> Option<String> {
        let failure = self.fast_path.take().and_then(FastPath::stop);
        drop(self.sniffers.take());
        failure
    }
}

impl Drop for Shaper {
    fn drop(&mut self) {
        self.halt();
    }
}

/// The service's engine: [`Shaper`] over the real driver.
pub struct WinEngine {
    pub rx_threads: usize,
}

impl Default for WinEngine {
    fn default() -> Self {
        Self { rx_threads: 1 }
    }
}

impl runner::Engine for WinEngine {
    type Running = Running;

    fn start(&mut self, plan: &Plan) -> Result<Running> {
        let shaper = Shaper::start(Spec {
            down: plan.down.clone(),
            up: plan.up.clone(),
            rules: Arc::new(plan.clone()),
            shape_lan: plan.shape_lan,
            rx_threads: self.rx_threads,
        })?;
        let report = Report::new(&shaper);
        Ok(Running { shaper, report })
    }
}

pub struct Running {
    shaper: Shaper,
    report: Report,
}

impl runner::Running for Running {
    fn failure(&self) -> Option<String> {
        self.shaper.failure()
    }

    fn report(&mut self) {
        self.report.log(&self.shaper);
    }

    fn sample(&self) -> Option<Snapshot> {
        Some(self.shaper.snapshot())
    }

    fn apps(&self) -> Vec<snailnet_ipc::AppInfo> {
        self.shaper
            .attribution()
            .apps()
            .iter()
            .map(|a| snailnet_ipc::AppInfo {
                family: a.family.clone(),
                name: a.name.clone(),
                exe: a.exe.clone(),
            })
            .collect()
    }

    fn stop(self) -> Option<String> {
        self.shaper.stop()
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

/// Periodic log lines: per-class rates since the last line, attribution
/// mix, and the sizes a soak test watches (flow table, apps, pool).
struct Report {
    at: Instant,
    down: Totals,
    up: Totals,
    counters: Counters,
}

impl Report {
    fn new(s: &Shaper) -> Self {
        let snap = s.snapshot();
        Self {
            at: Instant::now(),
            down: totals(snap.down.as_ref()),
            up: totals(snap.up.as_ref()),
            counters: s.counters(),
        }
    }

    fn log(&mut self, s: &Shaper) {
        let snap = s.snapshot();
        let secs = self.at.elapsed().as_secs_f64().max(1e-3);
        for (arrow, d, last) in [
            ("down", snap.down.as_ref(), &mut self.down),
            ("up", snap.up.as_ref(), &mut self.up),
        ] {
            let Some(d) = d else { continue };
            let now = totals(Some(d));
            last.resize(now.len(), (0, 0));
            let mut line = String::new();
            let mut drops = 0;
            for (((name, _), n), l) in d.classes.iter().zip(&now).zip(last.iter()) {
                let mbps = n.0.saturating_sub(l.0) as f64 * 8.0 / secs / 1e6;
                drops += n.1.saturating_sub(l.1);
                line += &format!(" | {name} {mbps:.2}");
            }
            tracing::info!(
                "{arrow} Mbps (avg over {secs:.0} s, shaped at {:.1}){line} | drops {drops}",
                d.rate_bytes_per_sec as f64 * 8.0 / 1e6
            );
            *last = now;
        }
        let who = s.who();
        let take = |c: &AtomicU64| c.swap(0, Relaxed);
        let (flow, socket, port, pending, unknown, late) = (
            take(&who.by_flow),
            take(&who.by_socket),
            take(&who.by_port),
            take(&who.pending),
            take(&who.unknown),
            take(&who.late_flows),
        );
        let (flows, sockets, ports, full) = s.attribution().sizes();
        let (small, large) = s.pool_available();
        let (c, l) = (s.counters(), self.counters);
        tracing::info!(
            "packets by flow {flow}, socket {socket}, port {port}, pending {pending}, unknown {unknown}; \
             late flows {late}; tracking {flows} flows, {sockets} sockets, {ports} ports, {} apps \
             (table full skips {full}); pool free {small}+{large}; passthrough {}, overflow {}, send errors {}",
            s.attribution().app_count(),
            c.bypassed - l.bypassed,
            (c.pool_exhausted - l.pool_exhausted) + (c.channel_full - l.channel_full),
            c.send_errors - l.send_errors,
        );
        self.counters = c;
        self.at = Instant::now();
    }
}
