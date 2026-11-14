//! Phase 0 scheduler: a root token bucket (the shaped line rate) in front of
//! weighted Deficit Round Robin across N classes (PLAN.md §9.2, minus
//! priority, ceilings and CoDel, which arrive in Phase 1).
//!
//! Properties:
//! - **Shares.** Saturated classes get bandwidth in proportion to their weights.
//! - **Work-conserving.** Idle classes accrue no credit, so their share goes to
//!   backlogged classes in proportion to the latter's weights.
//! - **Byte-fair.** Credit is in bytes, so small-packet flows aren't penalised.

mod token_bucket;

pub use token_bucket::TokenBucket;

use crate::clock::Nanos;
use std::collections::VecDeque;

/// Anything the scheduler can queue: it only needs the on-wire size.
pub trait Packet {
    fn wire_len(&self) -> usize;
}

pub type ClassId = usize;

/// Bytes of DRR credit handed out per round, split across classes by weight.
/// Eight full-size Ethernet frames keeps interleaving fine-grained.
const ROUND_QUANTUM_BYTES: u64 = 8 * 1514;

#[derive(Debug, Clone)]
pub struct ClassConfig {
    pub name: String,
    /// Relative weight (e.g. basis points). Must be > 0.
    pub weight: u32,
    /// Tail-drop limit for this class's queue.
    pub queue_limit_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Shaped rate of the whole direction.
    pub rate_bytes_per_sec: u64,
    pub burst_bytes: u64,
    pub classes: Vec<ClassConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    NoClasses,
    ZeroWeight(String),
    ZeroRate,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoClasses => write!(f, "at least one class is required"),
            Self::ZeroWeight(n) => write!(f, "class {n:?} has weight 0"),
            Self::ZeroRate => write!(f, "rate must be greater than zero"),
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClassStats {
    pub sent_packets: u64,
    pub sent_bytes: u64,
    pub dropped_packets: u64,
    pub dropped_bytes: u64,
    pub queued_bytes: u64,
}

#[derive(Debug)]
pub enum Dequeue<P> {
    /// Send this packet now.
    Packet { class: ClassId, packet: P },
    /// Packets are waiting, but the line rate says not before this time.
    WaitUntil(Nanos),
    /// Nothing queued.
    Empty,
}

struct Class<P> {
    name: String,
    quantum: u64,
    deficit: u64,
    queue: VecDeque<P>,
    limit_bytes: u64,
    stats: ClassStats,
}

pub struct Scheduler<P> {
    root: TokenBucket,
    classes: Vec<Class<P>>,
    /// Round-robin position: the class served last.
    cursor: usize,
    backlog_bytes: u64,
}

impl<P: Packet> Scheduler<P> {
    pub fn new(config: SchedulerConfig, now: Nanos) -> Result<Self, ConfigError> {
        if config.classes.is_empty() {
            return Err(ConfigError::NoClasses);
        }
        if config.rate_bytes_per_sec == 0 {
            return Err(ConfigError::ZeroRate);
        }
        if let Some(c) = config.classes.iter().find(|c| c.weight == 0) {
            return Err(ConfigError::ZeroWeight(c.name.clone()));
        }
        let total: u64 = config.classes.iter().map(|c| c.weight as u64).sum();
        let classes = config
            .classes
            .into_iter()
            .map(|c| Class {
                quantum: (ROUND_QUANTUM_BYTES * c.weight as u64 / total).max(1),
                name: c.name,
                deficit: 0,
                queue: VecDeque::new(),
                limit_bytes: c.queue_limit_bytes,
                stats: ClassStats::default(),
            })
            .collect();
        Ok(Self {
            root: TokenBucket::new(config.rate_bytes_per_sec, config.burst_bytes, now),
            classes,
            cursor: 0,
            backlog_bytes: 0,
        })
    }

    pub fn class_count(&self) -> usize {
        self.classes.len()
    }

    pub fn class_name(&self, class: ClassId) -> &str {
        &self.classes[class].name
    }

    pub fn stats(&self, class: ClassId) -> ClassStats {
        self.classes[class].stats
    }

    pub fn rate_bytes_per_sec(&self) -> u64 {
        self.root.rate_bytes_per_sec()
    }

    pub fn is_empty(&self) -> bool {
        self.backlog_bytes == 0
    }

    /// Queue a packet. Returns it back if the class queue is full (tail drop).
    ///
    /// # Panics
    /// If `class` is out of range.
    pub fn enqueue(&mut self, class: ClassId, packet: P) -> Result<(), P> {
        let len = packet.wire_len() as u64;
        let c = &mut self.classes[class];
        if c.stats.queued_bytes + len > c.limit_bytes && !c.queue.is_empty() {
            c.stats.dropped_packets += 1;
            c.stats.dropped_bytes += len;
            return Err(packet);
        }
        c.queue.push_back(packet);
        c.stats.queued_bytes += len;
        self.backlog_bytes += len;
        Ok(())
    }

    pub fn dequeue(&mut self, now: Nanos) -> Dequeue<P> {
        self.root.refill(now);
        let Some(i) = self.select() else {
            return Dequeue::Empty;
        };
        if !self.root.ready() {
            let wait = self.root.time_until_ready().unwrap_or(Nanos::MAX);
            return Dequeue::WaitUntil(now.saturating_add(wait));
        }
        let c = &mut self.classes[i];
        let packet = c.queue.pop_front().expect("selected class has a packet");
        let len = packet.wire_len() as u64;
        c.deficit -= len;
        c.stats.queued_bytes -= len;
        c.stats.sent_packets += 1;
        c.stats.sent_bytes += len;
        if c.queue.is_empty() {
            // Idle classes keep no credit: this is what makes DRR work-conserving.
            c.deficit = 0;
        }
        self.backlog_bytes -= len;
        self.root.consume(len);
        Dequeue::Packet { class: i, packet }
    }

    /// Remove everything that's queued, ignoring the rate. Used on shutdown so
    /// no packet is lost.
    pub fn drain_all(&mut self) -> Vec<(ClassId, P)> {
        let mut out = Vec::new();
        for (i, c) in self.classes.iter_mut().enumerate() {
            out.extend(c.queue.drain(..).map(|p| (i, p)));
            c.stats.queued_bytes = 0;
            c.deficit = 0;
        }
        self.backlog_bytes = 0;
        out
    }

    /// Pick the next class to serve, topping up DRR credit if nobody can
    /// afford their head packet. Doesn't remove anything.
    fn select(&mut self) -> Option<ClassId> {
        if self.backlog_bytes == 0 {
            return None;
        }
        let n = self.classes.len();
        for _ in 0..2 {
            for k in 0..n {
                let i = (self.cursor + k) % n;
                let c = &self.classes[i];
                if let Some(head) = c.queue.front()
                    && c.deficit >= head.wire_len() as u64
                {
                    self.cursor = i;
                    return Some(i);
                }
            }
            // Nobody can afford their head packet. Give every backlogged class
            // the smallest number of rounds that lets at least one of them.
            let rounds = self
                .classes
                .iter()
                .filter_map(|c| {
                    let head = c.queue.front()?.wire_len() as u64;
                    Some((head - c.deficit).div_ceil(c.quantum))
                })
                .min()?;
            for c in self.classes.iter_mut().filter(|c| !c.queue.is_empty()) {
                c.deficit += rounds * c.quantum;
            }
            // Credit was handed out together, so restart the scan at the next
            // class to keep service order round-robin.
            self.cursor = (self.cursor + 1) % n;
        }
        unreachable!("after a top-up at least one class can send");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{NANOS_PER_MILLI, NANOS_PER_SEC};

    #[derive(Debug)]
    struct P(usize);
    impl Packet for P {
        fn wire_len(&self) -> usize {
            self.0
        }
    }

    const MB: u64 = 1_000_000;

    fn sched(rate: u64, weights: &[u32]) -> Scheduler<P> {
        Scheduler::new(
            SchedulerConfig {
                rate_bytes_per_sec: rate,
                burst_bytes: 3_028,
                classes: weights
                    .iter()
                    .enumerate()
                    .map(|(i, &w)| ClassConfig {
                        name: format!("c{i}"),
                        weight: w,
                        queue_limit_bytes: 256 * 1024,
                    })
                    .collect(),
            },
            0,
        )
        .unwrap()
    }

    /// Run the scheduler on simulated time. `active[i]` says whether class i
    /// is kept backlogged with packets of `sizes[i]` bytes. Returns bytes sent
    /// per class in the window [warmup, warmup + measure).
    fn run(
        s: &mut Scheduler<P>,
        active: &[bool],
        sizes: &[usize],
        warmup: Nanos,
        measure: Nanos,
    ) -> Vec<u64> {
        let mut now: Nanos = 0;
        let mut sent = vec![0u64; s.class_count()];
        while now < warmup + measure {
            for (i, &on) in active.iter().enumerate() {
                // Keep a modest backlog so drops don't happen.
                while on && s.stats(i).queued_bytes < 64 * 1024 {
                    s.enqueue(i, P(sizes[i])).unwrap();
                }
            }
            match s.dequeue(now) {
                Dequeue::Packet { class, packet } => {
                    if now >= warmup {
                        sent[class] += packet.0 as u64;
                    }
                }
                Dequeue::WaitUntil(t) => now = t,
                Dequeue::Empty => now += NANOS_PER_MILLI,
            }
        }
        sent
    }

    fn assert_share(sent: &[u64], expected: &[f64], tol: f64) {
        let total: u64 = sent.iter().sum();
        for (i, &e) in expected.iter().enumerate() {
            let got = sent[i] as f64 / total as f64;
            assert!(
                (got - e).abs() <= tol,
                "class {i}: got {got:.4}, expected {e:.4} (sent {sent:?})"
            );
        }
    }

    #[test]
    fn rejects_bad_config() {
        let cfg = |classes| SchedulerConfig {
            rate_bytes_per_sec: 1,
            burst_bytes: 0,
            classes,
        };
        assert_eq!(
            Scheduler::<P>::new(cfg(vec![]), 0).err(),
            Some(ConfigError::NoClasses)
        );
        let zero = vec![ClassConfig {
            name: "x".into(),
            weight: 0,
            queue_limit_bytes: 1,
        }];
        assert_eq!(
            Scheduler::<P>::new(cfg(zero), 0).err(),
            Some(ConfigError::ZeroWeight("x".into()))
        );
    }

    #[test]
    fn saturated_40_60_split_within_3_percent() {
        let mut s = sched(10 * MB, &[40, 60]);
        let sent = run(
            &mut s,
            &[true, true],
            &[1500, 1500],
            NANOS_PER_SEC / 2,
            2 * NANOS_PER_SEC,
        );
        assert_share(&sent, &[0.40, 0.60], 0.03);
    }

    #[test]
    fn total_rate_matches_root() {
        let mut s = sched(10 * MB, &[40, 60]);
        let sent = run(
            &mut s,
            &[true, true],
            &[1500, 1500],
            NANOS_PER_SEC / 2,
            2 * NANOS_PER_SEC,
        );
        let total: u64 = sent.iter().sum();
        let expected = 20 * MB;
        assert!(total.abs_diff(expected) <= expected / 100, "total {total}");
    }

    #[test]
    fn lone_class_gets_the_whole_line() {
        let mut s = sched(10 * MB, &[40, 60]);
        let sent = run(
            &mut s,
            &[true, false],
            &[1500, 1500],
            NANOS_PER_SEC / 2,
            2 * NANOS_PER_SEC,
        );
        assert_eq!(sent[1], 0);
        assert!(sent[0].abs_diff(20 * MB) <= 20 * MB / 100, "sent {sent:?}");
    }

    #[test]
    fn idle_share_is_redistributed_by_weight() {
        // Steam 40, YouTube 45 (idle), Other 15 → Steam:Other = 40:15.
        let mut s = sched(10 * MB, &[40, 45, 15]);
        let sent = run(
            &mut s,
            &[true, false, true],
            &[1500, 1500, 1500],
            NANOS_PER_SEC / 2,
            2 * NANOS_PER_SEC,
        );
        assert_eq!(sent[1], 0);
        let a = sent[0] as f64 / (sent[0] + sent[2]) as f64;
        assert!((a - 40.0 / 55.0).abs() <= 0.03, "steam share {a:.4}");
    }

    #[test]
    fn fairness_is_in_bytes_not_packets() {
        let mut s = sched(10 * MB, &[50, 50]);
        let sent = run(
            &mut s,
            &[true, true],
            &[1500, 100],
            NANOS_PER_SEC / 2,
            2 * NANOS_PER_SEC,
        );
        assert_share(&sent, &[0.5, 0.5], 0.03);
    }

    #[test]
    fn tiny_weight_still_progresses_and_stays_small() {
        let mut s = sched(10 * MB, &[9_900, 100]);
        let sent = run(
            &mut s,
            &[true, true],
            &[1500, 1500],
            NANOS_PER_SEC / 2,
            2 * NANOS_PER_SEC,
        );
        assert!(sent[1] > 0, "low-weight class starved");
        assert_share(&sent, &[0.99, 0.01], 0.01);
    }

    #[test]
    fn oversized_packets_pass() {
        // A coalesced 64 KB receive is bigger than burst and quantum.
        let mut s = sched(10 * MB, &[1, 1]);
        let sent = run(&mut s, &[true, true], &[65_000, 1500], 0, NANOS_PER_SEC);
        assert!(sent[0] > 0 && sent[1] > 0, "sent {sent:?}");
    }

    #[test]
    fn tail_drop_when_queue_full() {
        let mut s = sched(MB, &[1]);
        let limit = 256 * 1024;
        let mut accepted = 0;
        while s.enqueue(0, P(1500)).is_ok() {
            accepted += 1500;
        }
        assert!(accepted <= limit + 1500);
        assert_eq!(s.stats(0).dropped_packets, 1);
    }

    #[test]
    fn empty_and_wait_states() {
        let mut s = sched(1_000, &[1]);
        assert!(matches!(s.dequeue(0), Dequeue::Empty));
        s.enqueue(0, P(5_000)).unwrap();
        s.enqueue(0, P(1_000)).unwrap();
        assert!(matches!(s.dequeue(0), Dequeue::Packet { .. }));
        // 5000 B against a 3028 B burst at 1000 B/s → wait ~1.97 s.
        match s.dequeue(0) {
            Dequeue::WaitUntil(t) => assert_eq!(t, 1_972_000_000),
            other => panic!("expected wait, got {other:?}"),
        }
    }

    #[test]
    fn drain_all_returns_everything() {
        let mut s = sched(1, &[1, 1]);
        s.enqueue(0, P(10)).unwrap();
        s.enqueue(1, P(20)).unwrap();
        s.enqueue(1, P(30)).unwrap();
        let drained = s.drain_all();
        assert_eq!(drained.len(), 3);
        assert!(s.is_empty());
        assert!(matches!(s.dequeue(0), Dequeue::Empty));
    }
}
