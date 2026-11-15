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
use std::sync::atomic::{
    AtomicBool, AtomicU64, Ordering::Acquire, Ordering::Relaxed, Ordering::Release,
};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use snailnet_core::classify::{
    App, AppId, Attribution, Event, FlowKey, Miss, Pack, Pending, Resolver, Source,
};
use snailnet_core::clock::{Clock, MonotonicClock};
use snailnet_core::parse::dns;
use snailnet_core::parse::ip::{self, PROTO_TCP, PROTO_UDP, PacketInfo};
use snailnet_core::rules::Plan;
use snailnet_core::scheduler::{ClassId, PacketMeta, Priority, SchedulerConfig};
use snailnet_core::sites::{DomainCache, Names, SiteMatcher, SitePack};
use snailnet_core::sniff::{FlowNames, Named, Via};
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

/// DNS answers arriving at this PC (H4, PLAN.md §6.2): copies only, so it
/// never delays them. Secure DNS (DoH) is invisible here; SNI covers that.
const DNS_FILTER: &str = "inbound and !loopback and (udp.SrcPort == 53 or tcp.SrcPort == 53)";

/// The `dns-sniffer` thread, feeding the IP → domain cache.
struct DnsSniffer {
    handle: Arc<WinDivert>,
    thread: Option<JoinHandle<()>>,
}

impl DnsSniffer {
    fn start(lib: &Arc<WinDivertLib>, sites: Arc<SiteState>) -> Result<Self> {
        let handle = Arc::new(
            WinDivert::open(
                lib,
                DNS_FILTER,
                Layer::Network,
                0,
                flags::SNIFF | flags::RECV_ONLY,
            )
            .context("opening the DNS sniffer")?,
        );
        let h = handle.clone();
        let thread = std::thread::Builder::new()
            .name("dns-sniffer".into())
            .spawn(move || {
                let mut batch = RecvBatch::new(DEFAULT_BATCH);
                while h.recv(&mut batch).is_ok() {
                    for (pkt, _) in batch.iter() {
                        if let Some(info) = ip::parse(pkt) {
                            sites.learn(info.payload(pkt), info.protocol == PROTO_TCP);
                        }
                    }
                }
            })?;
        Ok(Self {
            handle,
            thread: Some(thread),
        })
    }
}

impl Drop for DnsSniffer {
    fn drop(&mut self) {
        self.handle.shutdown_recv();
        if let Some(t) = self.thread.take() {
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
    /// Whether any rule targets a site, so flows need naming.
    fn has_site_rules(&self) -> bool {
        false
    }
    /// The class of a flow opened for `names` (SNI, or a DNS CNAME chain),
    /// if a site rule claims it. Beats the app's class (PLAN.md §8.3).
    fn site_class_of(&self, _names: &[String]) -> Option<ClassId> {
        None
    }
}

/// A [`Plan`] plus the domain → site matcher for its custom sites.
pub struct PlanRules {
    plan: Plan,
    sites: SiteMatcher,
}

impl PlanRules {
    pub fn new(plan: Plan) -> Self {
        let pack = SitePack::builtin();
        // Custom sites were validated with the config; if they somehow
        // aren't valid, the shipped sites still work.
        let sites = SiteMatcher::new(&pack, &plan.custom_sites)
            .or_else(|_| SiteMatcher::new(&pack, &[]))
            .unwrap_or_default();
        Self { plan, sites }
    }
}

impl Rules for PlanRules {
    fn class_of(&self, app: &App) -> ClassId {
        self.plan.class_of(app)
    }

    fn prio_of(&self, class: ClassId) -> Priority {
        self.plan.prio_of(class)
    }

    fn other_class(&self) -> ClassId {
        self.plan.other_class()
    }

    fn has_site_rules(&self) -> bool {
        self.plan.has_site_rules()
    }

    fn site_class_of(&self, names: &[String]) -> Option<ClassId> {
        let site = self.sites.site_of_any(names.iter().map(String::as_str))?;
        self.plan.site_class(site)
    }
}

/// Flow names and the IP → domain cache, shared by the rx threads and the
/// DNS sniffer. Lock order: `cache` (read) before `names`.
pub struct SiteState {
    names: Mutex<FlowNames>,
    cache: std::sync::RwLock<DomainCache>,
    clock: Arc<MonotonicClock>,
}

const FLOW_NAMES_CAP: usize = 16_384;
const DNS_CACHE_CAP: usize = 16_384;

impl SiteState {
    fn new(clock: Arc<MonotonicClock>) -> Self {
        Self {
            names: Mutex::new(FlowNames::new(FLOW_NAMES_CAP)),
            cache: std::sync::RwLock::new(DomainCache::new(DNS_CACHE_CAP)),
            clock,
        }
    }

    /// Record a DNS response (the UDP payload, or a TCP segment holding a
    /// whole length-prefixed message).
    fn learn(&self, msg: &[u8], tcp: bool) {
        let msg = if tcp {
            match msg {
                [hi, lo, rest @ ..]
                    if usize::from(u16::from_be_bytes([*hi, *lo])) == rest.len() =>
                {
                    rest
                }
                _ => return,
            }
        } else {
            msg
        };
        let Some(res) = dns::answers(msg) else { return };
        let names: Names = Arc::from(res.names);
        let now = self.clock.now();
        let mut cache = self.cache.write().unwrap_or_else(|e| e.into_inner());
        for (addr, ttl) in res.addrs {
            cache.insert(ip::canonical(addr), names.clone(), ttl, now);
        }
    }

    /// (flows being named, cached addresses)
    fn sizes(&self) -> (usize, usize) {
        let cache = self.cache.read().unwrap_or_else(|e| e.into_inner()).len();
        let names = self.names.lock().unwrap_or_else(|e| e.into_inner()).len();
        (names, cache)
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
    /// Packets of flows named by SNI / by the DNS cache / hidden by ECH /
    /// unnamed / still being named. Counted only while site rules exist.
    pub named_sni: AtomicU64,
    pub named_dns: AtomicU64,
    pub named_ech: AtomicU64,
    pub unnamed: AtomicU64,
    pub naming: AtomicU64,
}

/// The rules every rx thread classifies by, swappable while shaping. The
/// generation tells an rx thread to reload them and drop its cache; the
/// lock is only taken then, never per packet.
struct LiveRules {
    current: Mutex<Arc<dyn Rules>>,
    generation: AtomicU64,
}

impl LiveRules {
    fn new(rules: Arc<dyn Rules>) -> Self {
        Self {
            current: Mutex::new(rules),
            generation: AtomicU64::new(0),
        }
    }

    fn load(&self) -> Arc<dyn Rules> {
        self.current
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn publish(&self, rules: Arc<dyn Rules>) {
        let mut current = self.current.lock().unwrap_or_else(|e| e.into_inner());
        *current = rules;
        self.generation.fetch_add(1, Release);
    }
}

/// Packet → app → class. Each rx thread has its own clone (caches and
/// pending list); the attribution behind it is shared.
pub struct AppClassifier {
    attribution: Arc<WinAttribution>,
    who: Arc<WhoCounters>,
    live: Arc<LiveRules>,
    /// The rules this thread uses, as of generation `seen`.
    rules: Arc<dyn Rules>,
    seen: u64,
    /// App ids are never reused (a reused PID is a new process key and is
    /// resolved afresh), so this needs no expiry. One entry per app; cleared
    /// when the rules change.
    app_class: HashMap<AppId, ClassId>,
    pending: Pending,
    sites: Arc<SiteState>,
    clock: Arc<MonotonicClock>,
    /// Keyed per run, so nobody on the internet can aim flows at one bucket.
    /// Clones share the keys, so every rx thread hashes a flow the same.
    flow_hasher: RandomState,
}

impl Clone for AppClassifier {
    fn clone(&self) -> Self {
        Self {
            attribution: self.attribution.clone(),
            who: self.who.clone(),
            live: self.live.clone(),
            rules: self.rules.clone(),
            seen: self.seen,
            app_class: self.app_class.clone(),
            pending: self.pending.clone(),
            sites: self.sites.clone(),
            clock: self.clock.clone(),
            flow_hasher: self.flow_hasher.clone(),
        }
    }
}

impl AppClassifier {
    /// Pick up new rules if they changed since this thread last looked.
    fn refresh(&mut self) {
        let g = self.live.generation.load(Acquire);
        if g != self.seen {
            self.rules = self.live.load();
            self.app_class.clear();
            self.seen = g;
        }
    }

    fn class_of(&mut self, app: AppId) -> ClassId {
        let (rules, attribution) = (&self.rules, &self.attribution);
        *self
            .app_class
            .entry(app)
            .or_insert_with(|| rules.class_of(&attribution.app(app)))
    }

    /// The site rule's class for this flow, if it has a name a site rule
    /// claims. Only called while site rules exist.
    fn site_class(
        &self,
        key: FlowKey,
        pkt: &[u8],
        info: &PacketInfo,
        outbound: bool,
    ) -> Option<ClassId> {
        if !matches!(info.protocol, PROTO_TCP | PROTO_UDP) {
            return None;
        }
        let named = {
            let cache = self.sites.cache.read().unwrap_or_else(|e| e.into_inner());
            let mut names = self.sites.names.lock().unwrap_or_else(|e| e.into_inner());
            names.observe(key, pkt, info, outbound, &cache, self.clock.now())
        };
        let who = &self.who;
        match named {
            Named::Pending => {
                who.naming.fetch_add(1, Relaxed);
                None
            }
            Named::Known { names, via } => {
                match via {
                    Via::Sni => &who.named_sni,
                    Via::Dns => &who.named_dns,
                    Via::Ech => &who.named_ech,
                    Via::None => &who.unnamed,
                }
                .fetch_add(1, Relaxed);
                self.rules.site_class_of(&names?)
            }
        }
    }
}

impl Classifier for AppClassifier {
    /// The tag is the packet's [`AppId`] ([`NO_TAG`] when unknown), so
    /// per-tag byte counts are per-app.
    fn classify(&mut self, pkt: &[u8], info: &PacketInfo, outbound: bool) -> (PacketMeta, Tag) {
        self.refresh();
        let key = FlowKey::from_packet(info, outbound);
        let who = self.who.clone();
        let (app_class, tag) = match self.attribution.lookup(&key) {
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
        // A site rule beats the app's rule, in any app (PLAN.md §8.3).
        let class = if self.rules.has_site_rules() {
            self.site_class(key, pkt, info, outbound)
                .unwrap_or(app_class)
        } else {
            app_class
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
pub struct Spec {
    /// `None`: that direction passes straight through.
    pub down: Option<SchedulerConfig>,
    pub up: Option<SchedulerConfig>,
    pub rules: Arc<dyn Rules>,
    pub shape_lan: bool,
    /// 1 or 2. 2 can reorder packets within a flow.
    pub rx_threads: usize,
}

/// Running shaping. Dropping it (or [`Shaper::stop`]) closes every handle,
/// after flushing what it holds, and traffic flows normally again.
pub struct Shaper {
    fast_path: Option<FastPath<WinDivert>>,
    sniffers: Option<Sniffers>,
    /// `None` if its handle wouldn't open: sites are then named by SNI only.
    dns: Option<DnsSniffer>,
    sites: Arc<SiteState>,
    who: Arc<WhoCounters>,
    attribution: Arc<WinAttribution>,
    rules: Arc<LiveRules>,
}

impl Shaper {
    pub fn start(spec: Spec) -> Result<Self> {
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
        let clock = Arc::new(MonotonicClock::new());
        let sites = Arc::new(SiteState::new(clock.clone()));
        let dns = DnsSniffer::start(&lib, sites.clone())
            .inspect_err(|e| tracing::warn!("no DNS-based site names: {e:#}"))
            .ok();
        let net = Arc::new(
            WinDivert::open(&lib, &filter, Layer::Network, 0, 0)
                .context("opening the NETWORK layer")?,
        );
        let who = Arc::new(WhoCounters::default());
        let rules = Arc::new(LiveRules::new(spec.rules));
        let classifier = AppClassifier {
            attribution: attribution.clone(),
            who: who.clone(),
            rules: rules.load(),
            live: rules.clone(),
            seen: 0,
            app_class: HashMap::new(),
            pending: Pending::default(),
            sites: sites.clone(),
            clock,
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
            dns,
            sites,
            who,
            attribution,
            rules,
        })
    }

    /// New rules and scheduler configs, applied without stopping (PLAN.md
    /// §9.6): nothing queued is dropped and attribution is kept. Fails if the
    /// shaped directions differ; restart then.
    pub fn reconfigure(
        &self,
        down: Option<SchedulerConfig>,
        up: Option<SchedulerConfig>,
        rules: Arc<dyn Rules>,
    ) -> Result<()> {
        // Scheduler first: from here on the rx threads should classify
        // against the new class list, and the gap is a few packets at most.
        self.fp().reconfigure(down, up)?;
        self.rules.publish(rules);
        Ok(())
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
        drop(self.dns.take());
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
            rules: Arc::new(PlanRules::new(plan.clone())),
            shape_lan: plan.shape_lan,
            rx_threads: self.rx_threads,
        })?;
        let report = Report::new(&shaper);
        Ok(Running {
            shaper,
            report,
            shape_lan: plan.shape_lan,
        })
    }
}

pub struct Running {
    shaper: Shaper,
    report: Report,
    /// Fixed for the fast path's lifetime.
    shape_lan: bool,
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

    fn apply(&mut self, plan: &Plan) -> Result<()> {
        if plan.shape_lan != self.shape_lan {
            anyhow::bail!("LAN shaping changed");
        }
        self.shaper.reconfigure(
            plan.down.clone(),
            plan.up.clone(),
            Arc::new(PlanRules::new(plan.clone())),
        )
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
        let (sni, by_dns, ech, none, naming) = (
            take(&who.named_sni),
            take(&who.named_dns),
            take(&who.named_ech),
            take(&who.unnamed),
            take(&who.naming),
        );
        let (named_flows, cached) = s.sites.sizes();
        tracing::info!(
            "site names (packets): sni {sni}, dns {by_dns}, ech {ech}, none {none}, naming {naming}; \
             tracking {named_flows} named flows, {cached} DNS addresses{}",
            if s.dns.is_some() {
                ""
            } else {
                " (no DNS sniffer)"
            }
        );
        self.counters = c;
        self.at = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snailnet_core::rules::Config;

    fn plan(extra: &str) -> Plan {
        Config::parse(&format!(
            "version = 1\nactive_profile = \"p\"\n[[profiles]]\nid = \"p\"\nname = \"P\"\n\
             [[profiles.rules]]\ntarget = {{ kind = \"family\", id = \"brave\" }}\nshare = 3000\n\
             [[profiles.rules]]\ntarget = {{ kind = \"site\", id = \"youtube\" }}\nshare = 4000\n{extra}"
        ))
        .unwrap()
        .plan()
    }

    #[test]
    fn plan_rules_map_names_to_site_classes() {
        let r = PlanRules::new(plan(""));
        assert!(r.has_site_rules());
        let chain = [
            "youtube-ui.l.google.com".to_string(),
            "www.youtube.com".to_string(),
        ];
        assert_eq!(r.site_class_of(&chain), Some(1));
        assert_eq!(r.site_class_of(&["twitch.tv".to_string()]), None, "no rule");
        assert_eq!(r.site_class_of(&["example.com".to_string()]), None);
    }

    #[test]
    fn custom_sites_reach_the_matcher() {
        let extra = "[[profiles.rules]]\ntarget = { kind = \"site\", id = \"work\" }\nshare = 1000\n\
                     [[sites]]\nid = \"work\"\nname = \"Work\"\ndomains = [\"*.corp.example\"]\n";
        let r = PlanRules::new(plan(extra));
        assert_eq!(r.site_class_of(&["git.corp.example".to_string()]), Some(2));
    }

    /// A response: www.youtube.com A 142.250.1.1, TTL 60.
    fn dns_response() -> Vec<u8> {
        let mut m = vec![0x12, 0x34, 0x81, 0x80, 0, 1, 0, 1, 0, 0, 0, 0];
        for label in ["www", "youtube", "com"] {
            m.push(label.len() as u8);
            m.extend_from_slice(label.as_bytes());
        }
        m.extend_from_slice(&[0, 0, 1, 0, 1]); // end, type A, class IN
        m.extend_from_slice(&[0xc0, 12, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 142, 250, 1, 1]);
        m
    }

    #[test]
    fn learning_dns_fills_the_cache_over_udp_and_tcp() {
        let ip: std::net::IpAddr = "142.250.1.1".parse().unwrap();
        let s = SiteState::new(Arc::new(MonotonicClock::new()));
        s.learn(&dns_response(), false);
        let now = s.clock.now();
        let got = s.cache.read().unwrap().get(&ip, now).cloned().unwrap();
        assert_eq!(&got[..], ["www.youtube.com"]);

        let s = SiteState::new(Arc::new(MonotonicClock::new()));
        let msg = dns_response();
        s.learn(&msg, true);
        assert_eq!(s.sizes().1, 0, "TCP needs the length prefix");
        let mut framed = (msg.len() as u16).to_be_bytes().to_vec();
        framed.extend_from_slice(&msg);
        s.learn(&framed, true);
        assert_eq!(s.sizes().1, 1);
        s.learn(&framed[..framed.len() - 1], true); // split segment: ignored
        assert_eq!(s.sizes().1, 1);
    }
}
