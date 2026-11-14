//! Per-flow fair queuing inside one class (PLAN.md §9.3), the fq_codel
//! design: flows hash into a fixed set of buckets, each with its own CoDel;
//! DRR serves the buckets, with flow weights from per-app priority.
//!
//! A flow that just became active sits on the "new" list, which is served
//! first, so a sparse flow (a game, a DNS-sized request, a fresh page load)
//! gets through ahead of the bulk flows sharing its class. Deficits may go
//! negative, so CoDel can drop whatever is at the head without the DRR
//! needing to know its size up front.

use std::collections::VecDeque;

use super::Packet;
use super::codel::{Codel, Entry, Verdicts};
use crate::clock::Nanos;

/// Buckets per class. Flows that collide share a queue; with a home PC's
/// handful of busy flows that's rare and harmless.
pub(crate) const BUCKETS: usize = 1024;
/// DRR quantum for a weight-1 flow: one full-size frame.
const FLOW_QUANTUM: i64 = 1514;

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
}

impl<P: Packet> FlowQueues<P> {
    pub fn new() -> Self {
        Self {
            flows: Vec::new(),
            new: VecDeque::new(),
            old: VecDeque::new(),
            bytes: 0,
            packets: 0,
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
        if self.flows.is_empty() {
            self.flows.resize_with(BUCKETS, Flow::empty);
        }
        let i = (hash % BUCKETS as u64) as usize;
        let len = packet.wire_len() as u64;
        let f = &mut self.flows[i];
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
            let got = f.codel.dequeue(&mut f.queue, &mut f.bytes, now, v);
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
        self.bytes -= len;
        self.packets -= 1;
        Some(len)
    }

    /// Remove every packet, resetting all flow state.
    pub fn drain_into<T>(&mut self, out: &mut Vec<T>, wrap: impl Fn(P) -> T) {
        for f in &mut self.flows {
            out.extend(f.queue.drain(..).map(|e| wrap(e.packet)));
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
    pub fn absorb(&mut self, mut other: FlowQueues<P>) {
        for (i, f) in other.flows.iter_mut().enumerate() {
            let weight = f.weight;
            for e in f.queue.drain(..) {
                self.push(i as u64, weight, e.packet, e.at);
            }
        }
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
}
