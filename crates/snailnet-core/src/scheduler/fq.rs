//! Per-flow fair queuing inside one class (PLAN.md §9.3), the fq_codel
//! design: flows hash into a fixed set of buckets, each with its own CoDel;
//! DRR serves the buckets, with flow weights from per-app priority.
//!
//! A flow that just became active sits on the "new" list, which is served
//! first, so a sparse flow (a game, a DNS-sized request, a fresh page load)
//! gets through ahead of the bulk flows sharing its class. Deficits may go
//! negative, so CoDel can drop whatever is at the head without the DRR
//! needing to know its size up front.
//!
//! Each flow also remembers the [`Tag`] of the packet that started it and
//! counts what CoDel did to it, for [`FlowQueues::pressure`]. When an idle
//! bucket is taken over by another tag, its count moves to a small table of
//! retired counts, so a tag's total never loses what was done to it before.

use std::collections::VecDeque;

use super::codel::{Codel, Entry, Verdicts};
use super::{NO_TAG, Packet, Pressure, Tag};
use crate::clock::Nanos;

/// Buckets per class. Flows that collide share a queue; with a home PC's
/// handful of busy flows that's rare and harmless.
pub(crate) const BUCKETS: usize = 1024;
/// DRR quantum for a weight-1 flow: one full-size frame.
const FLOW_QUANTUM: i64 = 1514;
/// Retired counts kept per class, reserved up front so retiring never
/// allocates. Beyond this many tags, further retired counts are forgotten
/// (their tag's total goes back by that much).
pub(crate) const RETIRED_SLOTS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum List {
    None,
    New,
    Old,
}

struct Flow<P> {
    queue: VecDeque<Entry<P>>,
    bytes: u64,
    deficit: i64,
    weight: i64,
    codel: Codel,
    list: List,
    /// Tag of the packet that started this flow.
    tag: Tag,
    /// Packets CoDel dropped or marked, and hard limit drops, since `tag`
    /// took this bucket.
    acted: u64,
}

impl<P> Flow<P> {
    fn empty() -> Self {
        Self {
            queue: VecDeque::new(),
            bytes: 0,
            deficit: 0,
            weight: 1,
            codel: Codel::default(),
            list: List::None,
            tag: NO_TAG,
            acted: 0,
        }
    }
}

pub(crate) struct FlowQueues<P> {
    /// Allocated on first use, so idle classes cost nothing.
    flows: Vec<Flow<P>>,
    new: VecDeque<u16>,
    old: VecDeque<u16>,
    bytes: u64,
    packets: usize,
    /// `(tag, acted)` of flows whose bucket another tag took over.
    retired: Vec<(Tag, u64)>,
}

/// Add `acted` to `tag`'s retired count, if there's room.
fn retire(retired: &mut Vec<(Tag, u64)>, tag: Tag, acted: u64) {
    if acted == 0 {
        return;
    }
    if let Some(e) = retired.iter_mut().find(|e| e.0 == tag) {
        e.1 += acted;
    } else if retired.len() < RETIRED_SLOTS {
        retired.push((tag, acted));
    }
}

impl<P: Packet> FlowQueues<P> {
    pub fn new() -> Self {
        Self {
            flows: Vec::new(),
            new: VecDeque::new(),
            old: VecDeque::new(),
            bytes: 0,
            packets: 0,
            retired: Vec::new(),
        }
    }

    /// Allocate the buckets and the retired table on first use.
    fn allocate(&mut self) {
        if self.flows.is_empty() {
            self.flows.resize_with(BUCKETS, Flow::empty);
            self.retired.reserve_exact(RETIRED_SLOTS);
        }
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn packets(&self) -> usize {
        self.packets
    }

    pub fn is_empty(&self) -> bool {
        self.packets == 0
    }

    /// Queue `packet` on the bucket for `hash`, queued at `at`. `weight` is
    /// the flow's priority weight (latest packet wins).
    pub fn push(&mut self, hash: u64, weight: i64, packet: P, at: Nanos) {
        self.allocate();
        let i = (hash % BUCKETS as u64) as usize;
        let len = packet.wire_len() as u64;
        let tag = packet.tag();
        let f = &mut self.flows[i];
        if f.list == List::None && f.tag != tag {
            // A new flow in this bucket, of another owner.
            retire(&mut self.retired, f.tag, f.acted);
            f.tag = tag;
            f.acted = 0;
        }
        f.queue.push_back(Entry { packet, at });
        f.bytes += len;
        f.weight = weight.max(1);
        if f.list == List::None {
            f.list = List::New;
            f.deficit = FLOW_QUANTUM * f.weight;
            self.new.push_back(i as u16);
        }
        self.bytes += len;
        self.packets += 1;
    }

    /// Next packet to send, after CoDel has had its say. CoDel's drops and
    /// marks are added to `v`.
    pub fn pop(&mut self, now: Nanos, v: &mut Verdicts) -> Option<P> {
        loop {
            let (from_new, i) = match self.new.front() {
                Some(&i) => (true, i),
                None => (false, *self.old.front()?),
            };
            let f = &mut self.flows[i as usize];
            if f.deficit <= 0 {
                // Used up its turn: top up and go to the back of the old list.
                f.deficit += FLOW_QUANTUM * f.weight;
                self.unlink_front(from_new);
                self.flows[i as usize].list = List::Old;
                self.old.push_back(i);
                continue;
            }
            let (bytes_before, packets_before) = (f.bytes, f.queue.len());
            let acted_before = v.dropped_packets + v.marked;
            let got = f.codel.dequeue(&mut f.queue, &mut f.bytes, now, v);
            f.acted += v.dropped_packets + v.marked - acted_before;
            self.bytes -= bytes_before - f.bytes;
            self.packets -= packets_before - f.queue.len();
            match got {
                Some(p) => {
                    f.deficit -= p.wire_len() as i64;
                    return Some(p);
                }
                None => {
                    // Empty. A new flow that empties goes to the old list
                    // once, so it can't regain "new" priority by pausing.
                    self.unlink_front(from_new);
                    if from_new && !self.old.is_empty() {
                        self.flows[i as usize].list = List::Old;
                        self.old.push_back(i);
                    } else {
                        self.flows[i as usize].list = List::None;
                    }
                }
            }
        }
    }

    /// Drop the head packet of the flow with the most bytes queued (the hard
    /// queue limit, PLAN.md §9.4). Hitting the bulk flow rather than the
    /// newest arrival protects light flows. Returns the bytes dropped.
    pub fn drop_fattest(&mut self) -> Option<u64> {
        let i = self
            .new
            .iter()
            .chain(self.old.iter())
            .copied()
            .max_by_key(|&i| self.flows[i as usize].bytes)?;
        let f = &mut self.flows[i as usize];
        let e = f.queue.pop_front()?;
        let len = e.packet.wire_len() as u64;
        f.bytes -= len;
        f.acted += 1;
        self.bytes -= len;
        self.packets -= 1;
        Some(len)
    }

    /// Remove every packet, resetting all flow state (the counts behind
    /// [`Self::pressure`] are kept).
    pub fn drain_into<T>(&mut self, out: &mut Vec<T>, wrap: impl Fn(P) -> T) {
        for f in &mut self.flows {
            out.extend(f.queue.drain(..).map(|e| wrap(e.packet)));
            retire(&mut self.retired, f.tag, f.acted);
            *f = Flow::empty();
        }
        self.new.clear();
        self.old.clear();
        self.bytes = 0;
        self.packets = 0;
    }

    /// Move every packet from `other` into the same buckets here, keeping
    /// their queue times (live reconfiguration: a removed rule's packets go
    /// to Everything else, none are lost).
    /// The counts behind [`Self::pressure`] come along too.
    pub fn absorb(&mut self, mut other: FlowQueues<P>) {
        self.allocate();
        for (tag, acted) in other.retired.drain(..) {
            retire(&mut self.retired, tag, acted);
        }
        for (i, f) in other.flows.iter_mut().enumerate() {
            retire(&mut self.retired, f.tag, f.acted);
            let weight = f.weight;
            for e in f.queue.drain(..) {
                self.push(i as u64, weight, e.packet, e.at);
            }
        }
    }

    /// Append `(tag, pressure)` for every flow with bytes queued or actions
    /// counted, and every retired count. Tags may repeat; the caller merges.
    pub fn pressure(&self, out: &mut Vec<(Tag, Pressure)>) {
        for f in &self.flows {
            if f.bytes > 0 || f.acted > 0 {
                let p = Pressure {
                    queued_bytes: f.bytes,
                    acted: f.acted,
                };
                out.push((f.tag, p));
            }
        }
        out.extend(self.retired.iter().map(|&(tag, acted)| {
            let p = Pressure {
                queued_bytes: 0,
                acted,
            };
            (tag, p)
        }));
    }

    #[cfg(test)]
    pub fn check_invariants(&self) {
        let bytes: u64 = self.flows.iter().map(|f| f.bytes).sum();
        let packets: usize = self.flows.iter().map(|f| f.queue.len()).sum();
        assert_eq!(bytes, self.bytes);
        assert_eq!(packets, self.packets);
        for f in &self.flows {
            if !f.queue.is_empty() {
                assert_ne!(f.list, List::None, "a backlogged flow is not scheduled");
            }
        }
    }

    fn unlink_front(&mut self, from_new: bool) {
        if from_new {
            self.new.pop_front();
        } else {
            self.old.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct P(usize, u32);
    impl Packet for P {
        fn wire_len(&self) -> usize {
            self.0
        }
        fn tag(&self) -> Tag {
            self.1
        }
    }

    fn pressure(q: &FlowQueues<P>) -> Vec<(Tag, Pressure)> {
        let mut out = Vec::new();
        q.pressure(&mut out);
        out.sort_unstable_by_key(|e| e.0);
        out
    }

    fn pr(queued_bytes: u64, acted: u64) -> Pressure {
        Pressure {
            queued_bytes,
            acted,
        }
    }

    /// Pop `n` packets at time 0 (no CoDel action) and count bytes per tag.
    fn pop_n(q: &mut FlowQueues<P>, n: usize) -> [u64; 4] {
        let mut out = [0u64; 4];
        let mut v = Verdicts::default();
        for _ in 0..n {
            let p = q.pop(0, &mut v).expect("backlogged");
            out[p.1 as usize] += p.0 as u64;
        }
        assert_eq!(v, Verdicts::default());
        q.check_invariants();
        out
    }

    #[test]
    fn equal_flows_share_equally() {
        let mut q = FlowQueues::new();
        for _ in 0..1000 {
            q.push(1, 2, P(1500, 1), 0);
            q.push(2, 2, P(1500, 2), 0);
        }
        let got = pop_n(&mut q, 1000);
        // Stopping mid-turn leaves one flow ahead by at most one turn
        // (quantum × weight) plus the packet that overdrew it.
        let turn = (FLOW_QUANTUM * 2) as u64 + 1500;
        assert!(got[1].abs_diff(got[2]) <= turn, "{got:?}");
    }

    #[test]
    fn weights_4_to_1() {
        let mut q = FlowQueues::new();
        for _ in 0..2000 {
            q.push(1, 4, P(1500, 1), 0);
            q.push(2, 1, P(1500, 2), 0);
        }
        let got = pop_n(&mut q, 1000);
        let share = got[1] as f64 / (got[1] + got[2]) as f64;
        assert!((share - 0.8).abs() < 0.01, "high share {share}");
    }

    #[test]
    fn sparse_flow_jumps_the_bulk_queue() {
        let mut q = FlowQueues::new();
        for _ in 0..100 {
            q.push(1, 2, P(1500, 1), 0);
        }
        pop_n(&mut q, 5);
        q.push(2, 2, P(100, 2), 0);
        let mut v = Verdicts::default();
        assert_eq!(q.pop(0, &mut v), Some(P(100, 2)));
    }

    #[test]
    fn drop_fattest_hits_the_bulk_flow() {
        let mut q = FlowQueues::new();
        for _ in 0..10 {
            q.push(1, 2, P(1500, 1), 0);
        }
        q.push(2, 2, P(100, 2), 0);
        assert_eq!(q.drop_fattest(), Some(1500));
        q.check_invariants();
        assert_eq!(q.packets(), 10);
    }

    #[test]
    fn absorb_and_drain_keep_everything() {
        let mut a = FlowQueues::new();
        let mut b = FlowQueues::new();
        a.push(1, 2, P(10, 1), 0);
        b.push(1, 2, P(20, 2), 5);
        b.push(7, 4, P(30, 3), 6);
        a.absorb(b);
        a.check_invariants();
        assert_eq!((a.packets(), a.bytes()), (3, 60));
        let mut out = Vec::new();
        a.drain_into(&mut out, |p| p);
        assert_eq!(out.len(), 3);
        assert!(a.is_empty());
        a.check_invariants();
    }

    #[test]
    fn a_flow_keeps_its_first_packets_tag() {
        let mut q = FlowQueues::new();
        q.push(1, 2, P(1000, 5), 0);
        // Another owner hashing to the same busy bucket counts as the first.
        q.push(1, 2, P(500, 6), 0);
        q.push(2, 2, P(300, 6), 0);
        assert_eq!(pressure(&q), [(5, pr(1500, 0)), (6, pr(300, 0))]);
    }

    #[test]
    fn a_recycled_bucket_retires_the_old_owners_count() {
        let mut q = FlowQueues::new();
        for _ in 0..3 {
            q.push(1, 2, P(1500, 5), 0);
        }
        assert_eq!(q.drop_fattest(), Some(1500));
        let mut v = Verdicts::default();
        assert!(q.pop(0, &mut v).is_some() && q.pop(0, &mut v).is_some());
        assert_eq!(q.pop(0, &mut v), None, "bucket 1 is idle now");
        q.push(1, 2, P(700, 9), 0);
        assert_eq!(pressure(&q), [(5, pr(0, 1)), (9, pr(700, 0))]);
        q.check_invariants();
    }

    #[test]
    fn retired_counts_are_capped_without_growing() {
        let mut q = FlowQueues::new();
        let mut v = Verdicts::default();
        for tag in 0..(RETIRED_SLOTS as u32 + 10) {
            q.push(1, 2, P(1500, tag), 0);
            q.push(1, 2, P(1500, tag), 0);
            q.drop_fattest();
            while q.pop(0, &mut v).is_some() {}
        }
        assert_eq!(q.retired.len(), RETIRED_SLOTS);
        assert_eq!(q.retired.capacity(), RETIRED_SLOTS);
    }
}
