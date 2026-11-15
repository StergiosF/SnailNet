//! The packet fast path (PLAN.md §4 threading model, §6.3, §6.4).
//!
//! ```text
//!  engine ──recv──► rx-N ──(pooled packet, bounded channel)──► scheduler ──send──► engine
//!                    │ bypass (unparseable, LAN, unshaped direction,      ▲
//!                    └──────── pool or channel full, failing open) ───────┘
//!                                     watchdog: heartbeat older than the
//!                                     stall limit → engine.shutdown()
//! ```
//!
//! - **rx threads** (1–2) receive batches, parse, ask the [`Classifier`] for
//!   a class, copy the packet into a [`PacketPool`] buffer and hand it over.
//!   Nothing is allocated per packet. Pure TCP control packets are marked
//!   `control` so the scheduler sends them through its express lane.
//! - **The scheduler thread** owns one [`Scheduler`] per shaped direction,
//!   releases packets when tokens allow, and naps between releases.
//! - **The watchdog** fails open if the scheduler thread stops beating for
//!   longer than the stall limit (default 500 ms): it shuts the engine's
//!   receive side, so new traffic bypasses SnailNet entirely, and the rx
//!   threads drain what the driver already queued straight back out.
//!
//! Fail-open never drops what we hold: once failing open (or stopping), the
//! scheduler flushes its queues at full speed and forwards everything still
//! in the channel.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TrySendError, bounded};
use snailnet_core::clock::{Clock, MonotonicClock, NANOS_PER_MILLI, Nanos};
use snailnet_core::parse::ip::{self, PROTO_TCP, PacketInfo, is_local_network};
use snailnet_core::scheduler::{
    ClassStats, ConfigError, Dequeue, LiveConfig, Packet, PacketMeta, Scheduler, SchedulerConfig,
};

use crate::batch::DEFAULT_BATCH;
use crate::pool::{PacketBuf, PacketPool};
use crate::{Address, EngineError, PacketEngine, RecvBatch, SendBatch};

/// The classifier's label for a packet's owner (the service uses its app
/// id). The fast path counts sent bytes per tag; it never interprets them.
pub type Tag = u32;
/// No owner known.
pub const NO_TAG: Tag = Tag::MAX;

/// Decides which class a shaped packet belongs to. Each rx thread gets its
/// own clone, so per-thread caches need no locking.
pub trait Classifier: Clone + Send + 'static {
    /// Class, flow hash and priority for one packet, and its owner's tag.
    /// `control` is filled in by the fast path. Runs on the rx thread: keep
    /// it quick.
    fn classify(&mut self, info: &PacketInfo, outbound: bool) -> (PacketMeta, Tag);
}

/// How the scheduler thread waits for its next release time.
pub trait Nap: Send + 'static {
    fn nap(&mut self, d: Duration);
}

/// Plain `std::thread::sleep`. On Windows it rounds up to the system tick,
/// so real shaping wants a high-resolution timer instead.
pub struct ThreadSleep;

impl Nap for ThreadSleep {
    fn nap(&mut self, d: Duration) {
        std::thread::sleep(d);
    }
}

#[derive(Debug, Clone)]
pub struct FastPathConfig {
    /// Receive threads. 1 keeps per-flow packet order; 2 doubles receive
    /// throughput but two batches can overtake each other.
    pub rx_threads: usize,
    /// Pooled buffers: ordinary and jumbo (offloaded) packets.
    pub small_buffers: usize,
    pub large_buffers: usize,
    /// rx → scheduler channel depth, in packets.
    pub channel_depth: usize,
    /// Fail open if the scheduler thread is silent this long (PLAN.md §6.4).
    pub stall_limit: Duration,
    /// Shape LAN traffic too (printers, NAS, casting). Off by default.
    pub shape_lan: bool,
}

impl Default for FastPathConfig {
    fn default() -> Self {
        Self {
            rx_threads: 1,
            // 16 MB: more than every class's hard queue limit at 25 Mbps.
            small_buffers: 8192,
            large_buffers: 64,
            channel_depth: 16_384,
            stall_limit: Duration::from_millis(500),
            shape_lan: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error("invalid scheduler config: {0}")]
    Config(#[from] ConfigError),
    #[error("could not start a fast-path thread: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("nothing to shape: give a download and/or upload config")]
    NothingToShape,
}

#[derive(Debug, thiserror::Error)]
pub enum ReconfigureError {
    #[error("invalid scheduler config: {0}")]
    Config(#[from] ConfigError),
    /// Which directions are shaped is fixed when the fast path starts.
    #[error("the shaped directions changed; restart the fast path instead")]
    DirectionsChanged,
}

/// A packet sitting in a scheduler queue.
pub struct Queued {
    buf: PacketBuf,
    addr: Address,
    tag: Tag,
}

impl Packet for Queued {
    fn wire_len(&self) -> usize {
        self.buf.len()
    }

    fn mark_ce(&mut self) -> bool {
        ip::set_ce(&mut self.buf)
    }
}

struct Item {
    up: bool,
    meta: PacketMeta,
    packet: Queued,
}

/// Running totals. All packet counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counters {
    /// Handed to a scheduler.
    pub queued: u64,
    /// Passed straight through by design (LAN, unshaped direction,
    /// unparseable, or while failing open).
    pub bypassed: u64,
    /// Passed straight through because every pool buffer was in use.
    pub pool_exhausted: u64,
    /// Passed straight through because the scheduler's inbox was full.
    pub channel_full: u64,
    /// Sends the engine refused (the packets in that batch are lost).
    pub send_errors: u64,
}

#[derive(Default)]
struct AtomicCounters {
    queued: AtomicU64,
    bypassed: AtomicU64,
    pool_exhausted: AtomicU64,
    channel_full: AtomicU64,
    send_errors: AtomicU64,
}

impl AtomicCounters {
    fn load(&self) -> Counters {
        Counters {
            queued: self.queued.load(Relaxed),
            bypassed: self.bypassed.load(Relaxed),
            pool_exhausted: self.pool_exhausted.load(Relaxed),
            channel_full: self.channel_full.load(Relaxed),
            send_errors: self.send_errors.load(Relaxed),
        }
    }
}

/// One direction's per-class numbers, published by the scheduler thread.
#[derive(Debug, Clone, Default)]
pub struct DirectionStats {
    pub rate_bytes_per_sec: u64,
    /// (class name, stats), in class order.
    pub classes: Vec<(String, ClassStats)>,
    /// (tag, bytes sent) since the fast path started, by tag. Packets that
    /// bypass the scheduler (LAN, overflow) aren't counted.
    pub tags: Vec<(Tag, u64)>,
}

/// What the scheduler thread published last (every ~100 ms).
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    /// When the scheduler thread took it (`None`: never published). Rates
    /// must be computed between these, not between the reader's own polls:
    /// a reader polling every 250 ms sees snapshots 200–300 ms apart.
    pub taken: Option<Instant>,
    pub down: Option<DirectionStats>,
    pub up: Option<DirectionStats>,
}

struct Shared {
    clock: MonotonicClock,
    stop: AtomicBool,
    failing_open: AtomicBool,
    failure: Mutex<Option<String>>,
    heartbeat: AtomicU64,
    counters: AtomicCounters,
    snapshot: Mutex<Snapshot>,
    /// The scheduler thread picks up a new config on its next pass.
    live: [Option<LiveConfig>; 2],
}

impl Shared {
    /// Stop diverting: new traffic bypasses us; queued traffic is flushed.
    fn fail_open(&self, engine: &dyn PacketEngine, reason: String) {
        if !self.failing_open.swap(true, Relaxed) {
            *self.failure.lock().unwrap_or_else(|e| e.into_inner()) = Some(reason);
            engine.shutdown();
        }
    }

    fn send(&self, engine: &dyn PacketEngine, batch: &mut SendBatch) {
        if batch.is_empty() {
            return;
        }
        let n = batch.len() as u64;
        if engine.send_batch(batch).is_err() {
            self.counters.send_errors.fetch_add(n, Relaxed);
        }
        batch.clear();
    }
}

/// The running fast path. Dropping it stops it (see [`FastPath::stop`]).
pub struct FastPath<E: PacketEngine + 'static> {
    engine: Arc<E>,
    shared: Arc<Shared>,
    pool: PacketPool,
    threads: Vec<JoinHandle<()>>,
}

impl<E: PacketEngine + 'static> FastPath<E> {
    /// Start shaping `engine`'s traffic: `down` shapes inbound packets, `up`
    /// outbound; a direction without a config passes straight through.
    pub fn start<C: Classifier>(
        engine: Arc<E>,
        down: Option<SchedulerConfig>,
        up: Option<SchedulerConfig>,
        classifier: C,
        nap: Box<dyn Nap>,
        config: FastPathConfig,
    ) -> Result<Self, StartError> {
        if down.is_none() && up.is_none() {
            return Err(StartError::NothingToShape);
        }
        let clock = MonotonicClock::new();
        let now = clock.now();
        let live = [
            down.map(LiveConfig::new).transpose()?,
            up.map(LiveConfig::new).transpose()?,
        ];
        let [down, up] = [0, 1].map(|d| live[d].as_ref().map(|l| Scheduler::from_live(l, now)));
        let shared = Arc::new(Shared {
            clock,
            stop: AtomicBool::new(false),
            failing_open: AtomicBool::new(false),
            failure: Mutex::new(None),
            heartbeat: AtomicU64::new(now),
            counters: AtomicCounters::default(),
            snapshot: Mutex::new(Snapshot::default()),
            live,
        });
        let pool = PacketPool::new(config.small_buffers, config.large_buffers);
        let (tx, rx) = bounded::<Item>(config.channel_depth.max(1));

        let mut fp = Self {
            engine,
            shared,
            pool,
            threads: Vec::new(),
        };
        let shaped = Shaped {
            down: down.is_some(),
            up: up.is_some(),
            lan: config.shape_lan,
        };
        let result = (|| {
            {
                let (engine, shared) = (fp.engine.clone(), fp.shared.clone());
                fp.threads.push(
                    std::thread::Builder::new()
                        .name("scheduler".into())
                        .spawn(move || schedule_loop(&*engine, &shared, rx, down, up, nap))?,
                );
            }
            for n in 0..config.rx_threads.clamp(1, 2) {
                let (engine, shared, pool) =
                    (fp.engine.clone(), fp.shared.clone(), fp.pool.clone());
                let (tx, classifier) = (tx.clone(), classifier.clone());
                fp.threads
                    .push(std::thread::Builder::new().name(format!("rx-{n}")).spawn(
                        move || {
                            rx_loop(&*engine, &shared, &pool, classifier, shaped, tx);
                        },
                    )?);
            }
            {
                let (engine, shared) = (fp.engine.clone(), fp.shared.clone());
                let limit = config.stall_limit;
                fp.threads.push(
                    std::thread::Builder::new()
                        .name("watchdog".into())
                        .spawn(move || watchdog(&*engine, &shared, limit))?,
                );
            }
            Ok::<(), std::io::Error>(())
        })();
        drop(tx);
        // A failed spawn drops `fp`, which stops whatever did start.
        result?;
        Ok(fp)
    }

    /// The scheduler's latest per-class numbers.
    pub fn snapshot(&self) -> Snapshot {
        self.shared
            .snapshot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn counters(&self) -> Counters {
        self.shared.counters.load()
    }

    /// Switch the shaped directions to new configs without stopping
    /// (PLAN.md §9.6): queued packets stay, classes are matched by name
    /// (see [`Scheduler::reconfigure`]). Both configs are checked before
    /// either is applied. Packets the rx threads classified against the old
    /// class list may land in the wrong class for a moment; none are lost.
    pub fn reconfigure(
        &self,
        down: Option<SchedulerConfig>,
        up: Option<SchedulerConfig>,
    ) -> Result<(), ReconfigureError> {
        let new = [down, up];
        if new
            .iter()
            .zip(&self.shared.live)
            .any(|(n, l)| n.is_some() != l.is_some())
        {
            return Err(ReconfigureError::DirectionsChanged);
        }
        for c in new.iter().flatten() {
            c.validate()?;
        }
        for (c, live) in new.into_iter().zip(&self.shared.live) {
            if let (Some(c), Some(live)) = (c, live) {
                live.publish(c)?;
            }
        }
        Ok(())
    }

    /// Why the fast path failed open, if it did. Traffic is flowing
    /// unshaped from that moment; the caller should stop and report.
    pub fn failure(&self) -> Option<String> {
        self.shared
            .failure
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Free pool buffers: (small, large).
    pub fn pool_available(&self) -> (usize, usize) {
        self.pool.available()
    }

    /// Stop receiving, flush every queued packet, join the threads. Returns
    /// the fail-open reason, if there was one.
    pub fn stop(mut self) -> Option<String> {
        self.halt();
        self.failure()
    }

    fn halt(&mut self) {
        self.shared.stop.store(true, Relaxed);
        self.engine.shutdown();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

impl<E: PacketEngine + 'static> Drop for FastPath<E> {
    fn drop(&mut self) {
        self.halt();
    }
}

#[derive(Clone, Copy)]
struct Shaped {
    down: bool,
    up: bool,
    lan: bool,
}

fn rx_loop<C: Classifier>(
    engine: &dyn PacketEngine,
    shared: &Shared,
    pool: &PacketPool,
    mut classifier: C,
    shaped: Shaped,
    tx: Sender<Item>,
) {
    let mut batch = RecvBatch::new(DEFAULT_BATCH);
    let mut bypass = SendBatch::new(DEFAULT_BATCH);
    let c = &shared.counters;
    loop {
        match engine.recv_batch(&mut batch) {
            Ok(_) => {}
            Err(EngineError::Shutdown) => break,
            Err(e) => {
                // Can't receive: the driver's queue would fill and stall the
                // network. Get out of the way.
                shared.fail_open(engine, format!("receive failed: {e}"));
                break;
            }
        }
        let passthrough = shared.failing_open.load(Relaxed);
        for (data, addr) in batch.iter() {
            let up = addr.outbound();
            let meta = if passthrough {
                None
            } else {
                admit(data, up, shaped).map(|info| {
                    let (meta, tag) = classifier.classify(&info, up);
                    let control = info.protocol == PROTO_TCP && info.tcp_payload_len == Some(0);
                    (PacketMeta { control, ..meta }, tag)
                })
            };
            let Some((meta, tag)) = meta else {
                c.bypassed.fetch_add(1, Relaxed);
                push(engine, shared, &mut bypass, data, addr);
                continue;
            };
            let Some(buf) = pool.alloc(data) else {
                c.pool_exhausted.fetch_add(1, Relaxed);
                push(engine, shared, &mut bypass, data, addr);
                continue;
            };
            let item = Item {
                up,
                meta,
                packet: Queued {
                    buf,
                    addr: *addr,
                    tag,
                },
            };
            match tx.try_send(item) {
                Ok(()) => {
                    c.queued.fetch_add(1, Relaxed);
                }
                Err(TrySendError::Full(item) | TrySendError::Disconnected(item)) => {
                    c.channel_full.fetch_add(1, Relaxed);
                    push(engine, shared, &mut bypass, &item.packet.buf, addr);
                }
            }
        }
        shared.send(engine, &mut bypass);
    }
    shared.send(engine, &mut bypass);
}

/// Parse a packet and decide whether it is shaped at all.
fn admit(data: &[u8], up: bool, shaped: Shaped) -> Option<PacketInfo> {
    if !(if up { shaped.up } else { shaped.down }) {
        return None;
    }
    let info = ip::parse(data)?;
    let remote = if up { info.dst } else { info.src };
    if !shaped.lan && is_local_network(remote) {
        return None;
    }
    Some(info)
}

fn push(
    engine: &dyn PacketEngine,
    shared: &Shared,
    batch: &mut SendBatch,
    data: &[u8],
    addr: &Address,
) {
    batch.push(data, addr);
    if batch.is_full() {
        shared.send(engine, batch);
    }
}

/// How often the scheduler thread publishes its stats.
const SNAPSHOT_EVERY: Nanos = 100 * NANOS_PER_MILLI;
/// Longest nap, so newly arrived packets are queued promptly.
const MAX_NAP: Nanos = NANOS_PER_MILLI;
/// When idle, wait this long for a packet before beating again.
const IDLE_WAIT: Duration = Duration::from_millis(20);

fn schedule_loop(
    engine: &dyn PacketEngine,
    shared: &Shared,
    rx: Receiver<Item>,
    mut down: Option<Scheduler<Queued>>,
    mut up: Option<Scheduler<Queued>>,
    mut nap: Box<dyn Nap>,
) {
    let clock = &shared.clock;
    let mut out = SendBatch::new(DEFAULT_BATCH);
    let mut next_snapshot = 0;
    // Bytes sent per tag: [down, up].
    let mut sent: [SentByTag; 2] = Default::default();

    let enqueue = |item: Item, down: &mut Option<Scheduler<Queued>>, up: &mut Option<_>| {
        let s: Option<&mut Scheduler<Queued>> = if item.up { up.as_mut() } else { down.as_mut() };
        match s {
            Some(s) => {
                s.enqueue(clock.now(), item.meta, item.packet);
                None
            }
            // rx only queues shaped directions; be safe anyway.
            None => Some(item.packet),
        }
    };

    'run: loop {
        let now = clock.now();
        shared.heartbeat.store(now, Relaxed);
        for (s, live) in [down.as_mut(), up.as_mut()].into_iter().zip(&shared.live) {
            if let (Some(s), Some(live)) = (s, live) {
                s.sync(live, now);
            }
        }
        if shared.stop.load(Relaxed) || shared.failing_open.load(Relaxed) {
            break;
        }
        if now >= next_snapshot {
            publish(shared, down.as_ref(), up.as_ref(), &sent);
            next_snapshot = now + SNAPSHOT_EVERY;
        }

        let idle = down.as_ref().is_none_or(Scheduler::is_empty)
            && up.as_ref().is_none_or(Scheduler::is_empty);
        if idle {
            match rx.recv_timeout(IDLE_WAIT) {
                Ok(item) => {
                    if let Some(p) = enqueue(item, &mut down, &mut up) {
                        push(engine, shared, &mut out, &p.buf, &p.addr);
                    }
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break 'run,
            }
        }
        for item in rx.try_iter().take(4096) {
            if let Some(p) = enqueue(item, &mut down, &mut up) {
                push(engine, shared, &mut out, &p.buf, &p.addr);
            }
        }

        let now = clock.now();
        let mut wake: Option<Nanos> = None;
        for (s, sent) in [down.as_mut(), up.as_mut()].into_iter().zip(&mut sent) {
            let Some(s) = s else { continue };
            loop {
                match s.dequeue(now) {
                    Dequeue::Packet { packet, .. } => {
                        *sent.entry(packet.tag).or_default() += packet.buf.len() as u64;
                        push(engine, shared, &mut out, &packet.buf, &packet.addr);
                    }
                    Dequeue::WaitUntil(t) => {
                        wake = Some(wake.map_or(t, |w| w.min(t)));
                        break;
                    }
                    Dequeue::Empty => break,
                }
            }
        }
        shared.send(engine, &mut out);

        if let Some(t) = wake {
            let d = t.saturating_sub(clock.now()).min(MAX_NAP);
            if d > 0 {
                nap.nap(Duration::from_nanos(d));
            }
        }
    }

    // Stopping or failing open: release everything we hold, ignoring the
    // rate, then forward whatever the rx threads still hand over until
    // they have all exited.
    for (s, sent) in [down.as_mut(), up.as_mut()].into_iter().zip(&mut sent) {
        let Some(s) = s else { continue };
        for (_, p) in s.drain_all() {
            *sent.entry(p.tag).or_default() += p.buf.len() as u64;
            push(engine, shared, &mut out, &p.buf, &p.addr);
        }
    }
    shared.send(engine, &mut out);
    publish(shared, down.as_ref(), up.as_ref(), &sent);
    loop {
        shared.heartbeat.store(clock.now(), Relaxed);
        match rx.recv_timeout(IDLE_WAIT) {
            Ok(item) => {
                push(
                    engine,
                    shared,
                    &mut out,
                    &item.packet.buf,
                    &item.packet.addr,
                );
                if rx.is_empty() {
                    shared.send(engine, &mut out);
                }
            }
            Err(RecvTimeoutError::Timeout) => shared.send(engine, &mut out),
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    shared.send(engine, &mut out);
}

type SentByTag = HashMap<Tag, u64>;

fn publish(
    shared: &Shared,
    down: Option<&Scheduler<Queued>>,
    up: Option<&Scheduler<Queued>>,
    sent: &[SentByTag; 2],
) {
    let stats = |s: &Scheduler<Queued>, sent: &SentByTag| {
        let mut tags: Vec<(Tag, u64)> = sent.iter().map(|(&t, &b)| (t, b)).collect();
        tags.sort_unstable();
        DirectionStats {
            rate_bytes_per_sec: s.rate_bytes_per_sec(),
            classes: (0..s.class_count())
                .map(|i| (s.class_name(i).to_owned(), s.stats(i)))
                .collect(),
            tags,
        }
    };
    // Never block the scheduler on a reader.
    if let Ok(mut snap) = shared.snapshot.try_lock() {
        *snap = Snapshot {
            taken: Some(Instant::now()),
            down: down.map(|s| stats(s, &sent[0])),
            up: up.map(|s| stats(s, &sent[1])),
        };
    }
}

fn watchdog(engine: &dyn PacketEngine, shared: &Shared, limit: Duration) {
    let tick = (limit / 10).clamp(Duration::from_millis(1), Duration::from_millis(50));
    let mut meter = StallMeter::new(
        limit.as_nanos() as Nanos,
        2 * tick.as_nanos() as Nanos,
        shared.clock.now(),
        shared.heartbeat.load(Relaxed),
    );
    while !shared.stop.load(Relaxed) && !shared.failing_open.load(Relaxed) {
        std::thread::sleep(tick);
        let beat = shared.heartbeat.load(Relaxed);
        if let Some(silent) = meter.observe(shared.clock.now(), beat) {
            shared.fail_open(
                engine,
                format!(
                    "the scheduler thread stalled for {} ms; traffic now bypasses SnailNet",
                    silent / NANOS_PER_MILLI
                ),
            );
        }
    }
}

/// How long the scheduler has been silent, counting only time the watchdog
/// itself was awake to see.
///
/// The clock keeps running while the machine sleeps, but thread waits do
/// not, so after a resume the watchdog may look before the scheduler has
/// had a chance to beat. Comparing the heartbeat with `now` would then see
/// hours of "stall" and fail open for nothing. Instead each observation
/// adds at most `max_step` of silence, and any new beat resets the count.
struct StallMeter {
    limit: Nanos,
    max_step: Nanos,
    last_check: Nanos,
    last_beat: Nanos,
    silent: Nanos,
}

impl StallMeter {
    fn new(limit: Nanos, max_step: Nanos, now: Nanos, beat: Nanos) -> Self {
        Self {
            limit,
            max_step,
            last_check: now,
            last_beat: beat,
            silent: 0,
        }
    }

    /// Records one look at the heartbeat; returns the silence once it
    /// exceeds the limit.
    fn observe(&mut self, now: Nanos, beat: Nanos) -> Option<Nanos> {
        let step = now.saturating_sub(self.last_check).min(self.max_step);
        self.last_check = now;
        if beat != self.last_beat {
            self.last_beat = beat;
            self.silent = 0;
            return None;
        }
        self.silent += step;
        (self.silent > self.limit).then_some(self.silent)
    }
}

#[cfg(test)]
mod stall_tests {
    use super::*;

    const MS: Nanos = NANOS_PER_MILLI;

    #[test]
    fn a_silent_scheduler_trips_after_the_limit() {
        let mut m = StallMeter::new(500 * MS, 100 * MS, 0, 0);
        let mut tripped = None;
        for i in 1..=20 {
            if let Some(s) = m.observe(i * 50 * MS, 0) {
                tripped = Some((i, s));
                break;
            }
        }
        assert_eq!(tripped, Some((11, 550 * MS)));
    }

    #[test]
    fn a_beat_resets_the_count() {
        let mut m = StallMeter::new(500 * MS, 100 * MS, 0, 0);
        for i in 1..=9 {
            assert_eq!(m.observe(i * 50 * MS, 0), None);
        }
        assert_eq!(m.observe(500 * MS, 480 * MS), None);
        for i in 11..=20 {
            assert_eq!(m.observe(i * 50 * MS, 480 * MS), None);
        }
        assert!(m.observe(1050 * MS, 480 * MS).is_some());
    }

    #[test]
    fn a_clock_jump_from_sleep_is_not_a_stall() {
        let mut m = StallMeter::new(500 * MS, 100 * MS, 0, 0);
        assert_eq!(m.observe(50 * MS, 40 * MS), None);
        // The machine sleeps for 11 h; the watchdog looks first on resume.
        let resumed = 11 * 3600 * 1000 * MS;
        assert_eq!(m.observe(resumed, 40 * MS), None);
        // The scheduler beats right after, and all is well.
        assert_eq!(m.observe(resumed + 50 * MS, resumed + 10 * MS), None);
    }
}
