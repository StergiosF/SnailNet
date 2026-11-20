//! The scheduler: percentages + priorities (PLAN.md §9).
//!
//! One [`Scheduler`] shapes one direction. A root token bucket gates total
//! output at the shaped line rate; under it, classes (one per rule, then
//! "Everything else") share the line by weighted Deficit Round Robin, served
//! in priority order:
//!
//! - **Shares.** Credit (`deficit`) is handed out in rounds in proportion to
//!   each class's share. A class may send only while it has credit, so over
//!   time every backlogged class gets its share, whatever its priority.
//! - **Priority = who goes first.** Among classes with credit, the highest
//!   priority sends first; ties go round-robin.
//! - **Work-conserving.** Rounds happen only when no backlogged class has
//!   credit left, so an idle class's share flows to the busy ones in
//!   proportion to their weights.
//! - **Burst cap.** Idle classes keep earning credit, but only up to
//!   `burst_c` (~30 ms at their share). That's what lets a light game flow
//!   send the moment its packet arrives, and it bounds how far any class can
//!   run ahead of its share.
//! - **Ceilings.** An optional per-class token bucket; a class over its
//!   ceiling simply isn't eligible.
//! - **Inside a class**, flows are fair-queued with CoDel + ECN ([`fq`],
//!   [`codel`]).
//! - **Express lane** (§9.5): DNS, pure TCP control packets and small packets
//!   of High-priority flows skip the queues. They still pay root tokens, and
//!   class credit as they enter the lane, so they can't be used to dodge a
//!   share: a class more than one burst in debt queues normally.
//! - **Live changes** (§9.6): [`Scheduler::reconfigure`] swaps shares, rates
//!   and rules without dropping a packet.
//! - **Pressure per app**: each flow sub-queue remembers its first packet's
//!   [`Tag`] and counts what CoDel did to it, so [`Scheduler::pressure`] can
//!   say which app is queued and being slowed down, even inside one class.

mod codel;
mod config;
mod fq;
pub mod shares;
mod token_bucket;

#[cfg(test)]
mod tests;

pub use config::{ClassConfig, ConfigError, LiveConfig, Priority, SchedulerConfig};
pub use token_bucket::TokenBucket;

use std::collections::VecDeque;
use std::sync::Arc;

use crate::clock::{NANOS_PER_MILLI, NANOS_PER_SEC, Nanos};
use codel::Verdicts;
use fq::FlowQueues;

/// Who a packet belongs to: an opaque id from the classifier (the service
/// packs the app and site into it). The scheduler only groups by it, in
/// [`Scheduler::pressure`].
pub type Tag = u32;
/// No owner known.
pub const NO_TAG: Tag = Tag::MAX;

/// Anything the scheduler can queue.
pub trait Packet {
    /// On-wire size in bytes.
    fn wire_len(&self) -> usize;
    /// The owner's tag. A flow sub-queue takes the tag of the packet that
    /// starts it.
    fn tag(&self) -> Tag {
        NO_TAG
    }
    /// Set ECN Congestion Experienced if the packet is ECN-capable and return
    /// true; return false if it isn't, and CoDel will drop it instead.
    fn mark_ce(&mut self) -> bool {
        false
    }
}

pub type ClassId = usize;

/// What the classifier knows about a packet, handed over with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketMeta {
    /// Class to queue in. Out of range (a rule removed since the packet was
    /// classified) means Everything else.
    pub class: ClassId,
    /// Hash of the flow's 5-tuple: picks the fair-queue bucket in the class.
    pub flow: u64,
    /// The flow's per-app priority: flow weight inside the class, and
    /// express eligibility for small packets.
    pub prio: Priority,
    /// DNS, or a pure TCP control packet (SYN, FIN, RST, ACK-only ≤ 80 B).
    /// Always takes the express lane.
    pub control: bool,
}

impl PacketMeta {
    /// A Normal-priority, non-control packet of flow `flow` in `class`.
    pub fn new(class: ClassId, flow: u64) -> Self {
        Self {
            class,
            flow,
            prio: Priority::Normal,
            control: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClassStats {
    pub sent_packets: u64,
    pub sent_bytes: u64,
    /// Dropped by CoDel or the hard queue limit.
    pub dropped_packets: u64,
    pub dropped_bytes: u64,
    pub ecn_marked: u64,
    /// Sent through the express lane (included in `sent_*`).
    pub express_packets: u64,
    /// Bytes waiting in this class's queues (express lane not included).
    pub queued_bytes: u64,
}

/// How hard one tag's flows are pushing against their share
/// ([`Scheduler::pressure`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pressure {
    /// Bytes of its flows waiting in class queues now.
    pub queued_bytes: u64,
    /// Packets of its flows CoDel dropped or ECN-marked, plus hard queue
    /// limit drops, since the scheduler started. Cumulative: callers take
    /// differences between samples. It only goes backwards in a corner case
    /// (more than `RETIRED_SLOTS` tags lose counts to recycled flow queues
    /// in one class), so take differences with `saturating_sub`.
    pub acted: u64,
}

impl Pressure {
    fn add(&mut self, other: Pressure) {
        self.queued_bytes += other.queued_bytes;
        self.acted += other.acted;
    }
}

#[derive(Debug)]
pub enum Dequeue<P> {
    /// Send this packet now.
    Packet { class: ClassId, packet: P },
    /// Packets are waiting, but the line rate (or every waiting class's
    /// ceiling) says not before this time.
    WaitUntil(Nanos),
    /// Nothing queued.
    Empty,
}

/// Largest packet that can take the express lane for being High priority.
pub const EXPRESS_MAX_LEN: usize = 300;
/// Express lane capacity; beyond this, packets queue normally.
const EXPRESS_LIMIT_BYTES: u64 = 64 * 1024;
/// Credit handed out per DRR round, split across classes by weight.
const ROUND_QUANTUM_BYTES: u64 = 8 * 1514;
const MTU: u64 = 1514;
/// How far ahead of its share a class may run: its burst cap is this much
/// time at its guaranteed rate (PLAN.md §9.2: 20–50 ms).
const BURST_WINDOW: Nanos = 30 * NANOS_PER_MILLI;
const CEIL_BURST_WINDOW: Nanos = 10 * NANOS_PER_MILLI;
/// Hard queue limit: 150 ms at the rate the class can reach, within bounds.
const QUEUE_WINDOW: Nanos = 150 * NANOS_PER_MILLI;
const QUEUE_MIN_BYTES: u64 = 64 * 1024;
const QUEUE_MAX_BYTES: u64 = 2 * 1024 * 1024;

/// Per-class numbers derived from the config and the line rate.
#[derive(Debug, Clone, Copy)]
struct Params {
    prio: Priority,
    quantum: i64,
    burst: i64,
    limit_bytes: u64,
    ceil: Option<(u64, u64)>,
}

fn bytes_in(rate: u64, window: Nanos) -> u64 {
    (rate as u128 * window as u128 / NANOS_PER_SEC as u128) as u64
}

fn derive(config: &SchedulerConfig) -> Vec<Params> {
    // A 0% class still needs a non-zero weight to make progress.
    let weight = |c: &ClassConfig| c.share_bp.max(1) as u64;
    let total: u64 = config.classes.iter().map(weight).sum();
    let rate = config.rate_bytes_per_sec;
    config
        .classes
        .iter()
        .map(|c| {
            let w = weight(c);
            let quantum = (ROUND_QUANTUM_BYTES * w / total).max(1);
            let guaranteed = (rate as u128 * w as u128 / total as u128) as u64;
            let burst = bytes_in(guaranteed, BURST_WINDOW).max(2 * MTU).max(quantum);
            let reach = c.ceil_bytes_per_sec.map_or(rate, |ceil| ceil.min(rate));
            Params {
                prio: if c.share_bp == 0 {
                    Priority::Low
                } else {
                    c.prio
                },
                quantum: quantum as i64,
                burst: burst as i64,
                limit_bytes: bytes_in(reach, QUEUE_WINDOW).clamp(QUEUE_MIN_BYTES, QUEUE_MAX_BYTES),
                ceil: c
                    .ceil_bytes_per_sec
                    .map(|ceil| (ceil, bytes_in(ceil, CEIL_BURST_WINDOW).max(2 * MTU))),
            }
        })
        .collect()
}

struct Class<P> {
    name: String,
    p: Params,
    /// DRR credit in bytes. Between `-(burst + largest packet)` and `burst`.
    deficit: i64,
    ceil: Option<TokenBucket>,
    fq: FlowQueues<P>,
    stats: ClassStats,
}

impl<P: Packet> Class<P> {
    fn new(name: String, p: Params, now: Nanos) -> Self {
        Self {
            name,
            p,
            deficit: 0,
            ceil: p.ceil.map(|(r, b)| TokenBucket::new(r, b, now)),
            fq: FlowQueues::new(),
            stats: ClassStats::default(),
        }
    }

    fn update(&mut self, p: Params, now: Nanos) {
        // Keep the same fraction of a round's credit.
        self.deficit = (self.deficit as i128 * p.quantum as i128 / self.p.quantum as i128) as i64;
        self.deficit = self.deficit.clamp(-p.burst, p.burst);
        self.ceil = match (self.ceil.take(), p.ceil) {
            (Some(mut b), Some((r, burst))) => {
                b.set_rate(r, burst, now);
                Some(b)
            }
            (None, Some((r, burst))) => Some(TokenBucket::new(r, burst, now)),
            (_, None) => None,
        };
        self.p = p;
    }

    fn under_ceiling(&mut self, now: Nanos) -> Result<(), Nanos> {
        match &mut self.ceil {
            None => Ok(()),
            Some(b) => {
                b.refill(now);
                if b.ready() {
                    Ok(())
                } else {
                    Err(b.time_until_ready().unwrap_or(Nanos::MAX))
                }
            }
        }
    }

    fn charge(&mut self, len: u64) {
        if let Some(b) = &mut self.ceil {
            b.consume(len);
        }
        self.stats.sent_packets += 1;
        self.stats.sent_bytes += len;
    }

    fn tally(&mut self, v: Verdicts) {
        self.stats.dropped_packets += v.dropped_packets;
        self.stats.dropped_bytes += v.dropped_bytes;
        self.stats.ecn_marked += v.marked;
    }
}

pub struct Scheduler<P> {
    root: TokenBucket,
    classes: Vec<Class<P>>,
    express: VecDeque<(ClassId, P)>,
    express_bytes: u64,
    /// Round-robin position: the class served last.
    cursor: usize,
    /// Bytes in class queues (express lane not included).
    backlog_bytes: u64,
    config: Arc<SchedulerConfig>,
}

impl<P: Packet> Scheduler<P> {
    pub fn new(config: SchedulerConfig, now: Nanos) -> Result<Self, ConfigError> {
        Self::with_config(Arc::new(config), now)
    }

    /// Start from the current config of a [`LiveConfig`].
    pub fn from_live(live: &LiveConfig, now: Nanos) -> Self {
        Self::with_config(live.load(), now).expect("LiveConfig only holds valid configs")
    }

    fn with_config(config: Arc<SchedulerConfig>, now: Nanos) -> Result<Self, ConfigError> {
        config.validate()?;
        let classes = config
            .classes
            .iter()
            .zip(derive(&config))
            .map(|(c, p)| Class::new(c.name.clone(), p, now))
            .collect();
        Ok(Self {
            root: TokenBucket::new(config.rate_bytes_per_sec, config.burst_bytes, now),
            classes,
            express: VecDeque::new(),
            express_bytes: 0,
            cursor: 0,
            backlog_bytes: 0,
            config,
        })
    }

    pub fn class_count(&self) -> usize {
        self.classes.len()
    }

    /// Everything else: always the last class.
    pub fn other_class(&self) -> ClassId {
        self.classes.len() - 1
    }

    pub fn class_name(&self, class: ClassId) -> &str {
        &self.classes[class].name
    }

    pub fn stats(&self, class: ClassId) -> ClassStats {
        self.classes[class].stats
    }

    pub fn config(&self) -> &SchedulerConfig {
        &self.config
    }

    pub fn rate_bytes_per_sec(&self) -> u64 {
        self.root.rate_bytes_per_sec()
    }

    pub fn is_empty(&self) -> bool {
        self.backlog_bytes == 0 && self.express.is_empty()
    }

    /// Pressure per tag, over every class: `out` is cleared, then gets one
    /// entry per tag with queued bytes or CoDel actions, sorted by tag. Walks
    /// every flow sub-queue, so call it at the stats rate, not per packet.
    /// Doesn't allocate once `out` has room for the tags seen.
    pub fn pressure(&self, out: &mut Vec<(Tag, Pressure)>) {
        out.clear();
        for c in &self.classes {
            c.fq.pressure(out);
        }
        out.sort_unstable_by_key(|&(tag, _)| tag);
        out.dedup_by(|later, kept| {
            let same = later.0 == kept.0;
            if same {
                kept.1.add(later.1);
            }
            same
        });
    }

    /// Queue a packet that arrived at `now`. Never fails: if the class is
    /// over its hard limit, the head of its fattest flow is dropped instead.
    pub fn enqueue(&mut self, now: Nanos, meta: PacketMeta, packet: P) {
        if self.is_empty() {
            // Nobody was waiting, so nobody was wronged: forgive debts left
            // over from the last busy period.
            for c in &mut self.classes {
                c.deficit = c.deficit.max(0);
            }
        }
        let class = meta.class.min(self.other_class());
        let len = packet.wire_len() as u64;
        let c = &mut self.classes[class];
        let urgent = packet.wire_len() <= EXPRESS_MAX_LEN
            && (c.p.prio == Priority::High || meta.prio == Priority::High);
        if (meta.control || urgent)
            // Express traffic is paid for up front, as it enters the lane. A
            // class that keeps it up past its share runs out of credit and
            // falls back to its queue, where its share holds it.
            && c.deficit > -c.p.burst
            && self.express_bytes + len <= EXPRESS_LIMIT_BYTES
        {
            c.deficit -= len as i64;
            self.express.push_back((class, packet));
            self.express_bytes += len;
            return;
        }
        c.fq.push(meta.flow, meta.prio.flow_weight(), packet, now);
        c.stats.queued_bytes += len;
        self.backlog_bytes += len;
        while c.fq.bytes() > c.p.limit_bytes && c.fq.packets() > 1 {
            let Some(dropped) = c.fq.drop_fattest() else {
                break;
            };
            c.stats.dropped_packets += 1;
            c.stats.dropped_bytes += dropped;
            c.stats.queued_bytes -= dropped;
            self.backlog_bytes -= dropped;
        }
    }

    pub fn dequeue(&mut self, now: Nanos) -> Dequeue<P> {
        if self.is_empty() {
            return Dequeue::Empty;
        }
        self.root.refill(now);
        if !self.root.ready() {
            let wait = self.root.time_until_ready().unwrap_or(Nanos::MAX);
            return Dequeue::WaitUntil(now.saturating_add(wait));
        }

        if let Some((class, packet)) = self.express.pop_front() {
            let len = packet.wire_len() as u64;
            self.express_bytes -= len;
            let c = &mut self.classes[class];
            // Credit was taken at enqueue.
            c.charge(len);
            c.stats.express_packets += 1;
            self.root.consume(len);
            return Dequeue::Packet { class, packet };
        }

        loop {
            if self.backlog_bytes == 0 {
                return Dequeue::Empty;
            }
            let n = self.classes.len();
            let mut best: Option<(usize, Priority)> = None;
            let mut any_eligible = false;
            let mut ceil_wait = Nanos::MAX;
            for k in 1..=n {
                let i = (self.cursor + k) % n;
                let c = &mut self.classes[i];
                if c.fq.is_empty() {
                    continue;
                }
                if let Err(wait) = c.under_ceiling(now) {
                    ceil_wait = ceil_wait.min(wait);
                    continue;
                }
                any_eligible = true;
                if c.deficit > 0 && best.is_none_or(|(_, p)| c.p.prio > p) {
                    best = Some((i, c.p.prio));
                }
            }
            if !any_eligible {
                return Dequeue::WaitUntil(now.saturating_add(ceil_wait));
            }
            let Some((i, _)) = best else {
                self.replenish(now);
                continue;
            };

            let c = &mut self.classes[i];
            let before = c.fq.bytes();
            let mut v = Verdicts::default();
            let got = c.fq.pop(now, &mut v);
            let removed = before - c.fq.bytes();
            c.stats.queued_bytes -= removed;
            self.backlog_bytes -= removed;
            c.tally(v);
            // CoDel may have dropped everything this class had; pick again.
            let Some(packet) = got else { continue };
            let len = packet.wire_len() as u64;
            c.deficit -= len as i64;
            c.charge(len);
            self.root.consume(len);
            self.cursor = i;
            return Dequeue::Packet { class: i, packet };
        }
    }

    /// One DRR top-up: the fewest rounds that give some eligible class
    /// credit. Every class earns those rounds, capped at its burst, so idle
    /// classes bank a little credit and nobody banks a lot.
    fn replenish(&mut self, now: Nanos) {
        let rounds = self
            .classes
            .iter_mut()
            .filter(|c| !c.fq.is_empty())
            .filter_map(|c| {
                c.under_ceiling(now).ok()?;
                // Eligible classes have no credit here (deficit ≤ 0), so the
                // shortfall to reach 1 byte of credit is positive.
                let short = (1 - c.deficit) as u64;
                Some(short.div_ceil(c.p.quantum as u64) as i64)
            })
            .min()
            .expect("called only when some class is eligible");
        for c in &mut self.classes {
            c.deficit = c
                .deficit
                .saturating_add(rounds.saturating_mul(c.p.quantum))
                .min(c.p.burst);
        }
    }

    /// Apply the latest [`LiveConfig`] if it changed since the last call.
    /// Cheap enough to call on every scheduler loop iteration.
    pub fn sync(&mut self, live: &LiveConfig, now: Nanos) {
        let latest = live.load();
        if !Arc::ptr_eq(&latest, &self.config) {
            self.reconfigure(latest, now)
                .expect("LiveConfig only holds valid configs");
        }
    }

    /// Switch to a new config without losing a packet (PLAN.md §9.6).
    /// Classes are matched by name: kept classes keep their queues, with
    /// credit rescaled to the new share; new classes start empty; packets of
    /// removed classes move to Everything else. Class ids may change, so
    /// callers must classify against the new config from here on.
    pub fn reconfigure(
        &mut self,
        config: Arc<SchedulerConfig>,
        now: Nanos,
    ) -> Result<(), ConfigError> {
        config.validate()?;
        self.root
            .set_rate(config.rate_bytes_per_sec, config.burst_bytes, now);
        let params = derive(&config);
        let mut old: Vec<Option<Class<P>>> = std::mem::take(&mut self.classes)
            .into_iter()
            .map(Some)
            .collect();
        let mut moved_to = vec![usize::MAX; old.len()];
        for (j, (cc, p)) in config.classes.iter().zip(params).enumerate() {
            let kept = old
                .iter()
                .position(|o| o.as_ref().is_some_and(|c| c.name == cc.name));
            let class = match kept {
                Some(k) => {
                    moved_to[k] = j;
                    let mut c = old[k].take().expect("matched above");
                    c.update(p, now);
                    c
                }
                None => Class::new(cc.name.clone(), p, now),
            };
            self.classes.push(class);
        }
        let other = self.other_class();
        for (k, removed) in old.into_iter().enumerate() {
            if let Some(c) = removed {
                moved_to[k] = other;
                let o = &mut self.classes[other];
                o.stats.queued_bytes += c.fq.bytes();
                o.fq.absorb(c.fq);
            }
        }
        for (class, _) in &mut self.express {
            *class = moved_to[*class];
        }
        self.cursor = self.cursor.min(other);
        self.config = config;
        Ok(())
    }

    /// Remove everything that's queued, ignoring the rate. Used on shutdown
    /// so no packet is lost.
    pub fn drain_all(&mut self) -> Vec<(ClassId, P)> {
        let mut out: Vec<(ClassId, P)> = self.express.drain(..).collect();
        self.express_bytes = 0;
        for (i, c) in self.classes.iter_mut().enumerate() {
            c.fq.drain_into(&mut out, |p| (i, p));
            c.stats.queued_bytes = 0;
            c.deficit = 0;
        }
        self.backlog_bytes = 0;
        out
    }

    #[cfg(test)]
    fn check_invariants(&self, max_packet: i64) {
        let mut backlog = 0;
        let mut pressure = Vec::new();
        self.pressure(&mut pressure);
        let by_tag: u64 = pressure.iter().map(|(_, p)| p.queued_bytes).sum();
        let queued: u64 = self.classes.iter().map(|c| c.stats.queued_bytes).sum();
        assert_eq!(by_tag, queued, "queued bytes per tag don't add up");
        assert!(
            pressure.windows(2).all(|w| w[0].0 < w[1].0),
            "tags not sorted and unique"
        );
        for c in &self.classes {
            c.fq.check_invariants();
            assert_eq!(c.stats.queued_bytes, c.fq.bytes(), "{}", c.name);
            backlog += c.fq.bytes();
            assert!(
                c.deficit <= c.p.burst && c.deficit >= -(c.p.burst + max_packet),
                "{}: deficit {} outside ±{} (+{max_packet})",
                c.name,
                c.deficit,
                c.p.burst
            );
        }
        assert_eq!(backlog, self.backlog_bytes);
        let express: u64 = self.express.iter().map(|(_, p)| p.wire_len() as u64).sum();
        assert_eq!(express, self.express_bytes);
        assert!(self.express.iter().all(|&(c, _)| c < self.classes.len()));
    }
}
