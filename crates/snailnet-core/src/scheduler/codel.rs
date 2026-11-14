//! CoDel active queue management (RFC 8289), one instance per flow queue
//! (PLAN.md §9.4).
//!
//! Download shaping holds packets that already crossed the internet, so the
//! sender only slows down if it sees delay or loss. CoDel watches each
//! packet's sojourn time; once it has stayed above [`TARGET`] for a whole
//! [`INTERVAL`], it signals: ECN-marks the packet if the flow negotiated ECN,
//! drops it otherwise, and signals faster (interval / √count) while the delay
//! persists. The control loop mirrors Linux's `codel_dequeue`.

use std::collections::VecDeque;

use super::Packet;
use crate::clock::{NANOS_PER_MILLI, Nanos};

pub const TARGET: Nanos = 5 * NANOS_PER_MILLI;
pub const INTERVAL: Nanos = 100 * NANOS_PER_MILLI;
/// A queue holding no more than one full packet is never "too long".
const MAX_PACKET: u64 = 1514;

/// A queued packet and when it was queued.
#[derive(Debug)]
pub(crate) struct Entry<P> {
    pub packet: P,
    pub at: Nanos,
}

/// What CoDel did to the packets it removed on one dequeue.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Verdicts {
    pub dropped_packets: u64,
    pub dropped_bytes: u64,
    pub marked: u64,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct Codel {
    /// When the sojourn time first went above target in the current episode,
    /// plus one interval. `None` while below target.
    first_above: Option<Nanos>,
    drop_next: Nanos,
    count: u32,
    last_count: u32,
    dropping: bool,
}

impl Codel {
    /// Dequeue the next packet to deliver from `queue`, dropping (or
    /// ECN-marking) as CoDel decides. `bytes` is the queue's byte count and is
    /// kept in step. Dropped packets are freed here and tallied in `v`.
    pub fn dequeue<P: Packet>(
        &mut self,
        queue: &mut VecDeque<Entry<P>>,
        bytes: &mut u64,
        now: Nanos,
        v: &mut Verdicts,
    ) -> Option<P> {
        let (mut packet, mut ok_to_drop) = self.pop(queue, bytes, now)?;
        if self.dropping {
            if !ok_to_drop {
                self.dropping = false;
            }
            while self.dropping && now >= self.drop_next {
                self.count = self.count.saturating_add(1);
                self.drop_next = control_law(self.drop_next, self.count);
                if packet.mark_ce() {
                    v.marked += 1;
                    return Some(packet);
                }
                drop_packet(packet, v);
                (packet, ok_to_drop) = self.pop(queue, bytes, now)?;
                if !ok_to_drop {
                    self.dropping = false;
                }
            }
        } else if ok_to_drop {
            let marked = packet.mark_ce();
            if marked {
                v.marked += 1;
            }
            self.dropping = true;
            // If we were dropping recently, resume near the old rate instead
            // of starting over (RFC 8289 §5.5).
            let delta = self.count.wrapping_sub(self.last_count);
            self.count = if delta > 1 && now.saturating_sub(self.drop_next) < 16 * INTERVAL {
                delta
            } else {
                1
            };
            self.last_count = self.count;
            self.drop_next = control_law(now, self.count);
            if !marked {
                drop_packet(packet, v);
                (packet, _) = self.pop(queue, bytes, now)?;
            }
        }
        Some(packet)
    }

    /// Pop the head and decide whether it may be dropped. An empty queue
    /// ends the episode.
    fn pop<P: Packet>(
        &mut self,
        queue: &mut VecDeque<Entry<P>>,
        bytes: &mut u64,
        now: Nanos,
    ) -> Option<(P, bool)> {
        let Some(e) = queue.pop_front() else {
            self.first_above = None;
            self.dropping = false;
            return None;
        };
        *bytes -= e.packet.wire_len() as u64;
        let sojourn = now.saturating_sub(e.at);
        let ok = if sojourn < TARGET || *bytes <= MAX_PACKET {
            self.first_above = None;
            false
        } else {
            match self.first_above {
                None => {
                    self.first_above = Some(now + INTERVAL);
                    false
                }
                Some(t) => now >= t,
            }
        };
        Some((e.packet, ok))
    }
}

fn control_law(t: Nanos, count: u32) -> Nanos {
    t + (INTERVAL as f64 / (count.max(1) as f64).sqrt()) as Nanos
}

fn drop_packet<P: Packet>(packet: P, v: &mut Verdicts) {
    v.dropped_packets += 1;
    v.dropped_bytes += packet.wire_len() as u64;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct P {
        len: usize,
        ect: bool,
        ce: bool,
    }
    impl Packet for P {
        fn wire_len(&self) -> usize {
            self.len
        }
        fn mark_ce(&mut self) -> bool {
            self.ce = self.ect;
            self.ect
        }
    }

    /// Queue arriving at `in_per_ms` packets/ms, served at `out_per_ms`, for
    /// `ms` milliseconds. Returns what CoDel did.
    fn run(ect: bool, in_per_ms: u32, out_per_ms: u32, ms: u64) -> Verdicts {
        let mut c = Codel::default();
        let mut q = VecDeque::new();
        let mut bytes = 0u64;
        let mut v = Verdicts::default();
        for t in 0..ms {
            let now = t * NANOS_PER_MILLI;
            for _ in 0..in_per_ms {
                q.push_back(Entry {
                    packet: P {
                        len: 1500,
                        ect,
                        ce: false,
                    },
                    at: now,
                });
                bytes += 1500;
            }
            for _ in 0..out_per_ms {
                if let Some(p) = c.dequeue(&mut q, &mut bytes, now, &mut v) {
                    assert!(ect || !p.ce, "a non-ECN packet was marked");
                }
            }
        }
        assert_eq!(bytes, q.len() as u64 * 1500);
        v
    }

    #[test]
    fn short_queue_is_left_alone() {
        assert_eq!(run(false, 1, 1, 2_000), Verdicts::default());
    }

    #[test]
    fn standing_queue_gets_drops() {
        // Arrivals at 2× the service rate with a sender that never backs off:
        // CoDel must keep dropping.
        let v = run(false, 2, 1, 2_000);
        assert!(v.dropped_packets > 10, "{v:?}");
        assert_eq!(v.marked, 0);
    }

    #[test]
    fn ecn_flows_are_marked_not_dropped() {
        let v = run(true, 2, 1, 2_000);
        assert!(v.marked > 10, "{v:?}");
        assert_eq!(v.dropped_packets, 0);
    }

    #[test]
    fn nothing_happens_before_one_interval() {
        // Delay is above target, but not for a full interval yet.
        assert_eq!(run(false, 2, 1, 90), Verdicts::default());
    }

    #[test]
    fn control_law_speeds_up() {
        assert_eq!(control_law(0, 1), INTERVAL);
        assert_eq!(control_law(0, 4), INTERVAL / 2);
        assert!(control_law(0, 100) < control_law(0, 99));
    }
}
