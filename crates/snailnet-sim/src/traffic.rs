//! Traffic models (PLAN.md §19.1) and the congestion control behind them.

use snailnet_core::clock::Nanos;
use snailnet_core::scheduler::Priority;

use crate::ms;

/// Wire size of a full data packet.
pub const MSS: usize = 1500;
/// Initial congestion window, in packets (RFC 6928).
const INITIAL_WINDOW: f64 = 10.0;
/// Receive window cap, in packets (~4 MB).
const MAX_WINDOW: f64 = 2800.0;
const MIN_WINDOW: f64 = 2.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Model {
    /// Reno-style AIMD TCP: slow start, then +1 packet per RTT; halves its
    /// window on loss, or on an ECN mark if it negotiated ECN. `bytes: None`
    /// sends forever (a big download).
    Tcp { ecn: bool, bytes: Option<u64> },
    /// QUIC-like: paced at 1.25 × window / RTT instead of in bursts, CUBIC's
    /// 0.7 backoff, no ECN (most QUIC stacks don't use it yet).
    Quic { bytes: Option<u64> },
    /// Constant bit rate UDP with no congestion control: game ticks, VoIP,
    /// or a ping.
    Cbr { size: usize, period: Nanos },
    /// Web browsing: transfers of random size (exponential, this mean), each
    /// on a fresh TCP connection, separated by random think times.
    Web { mean_bytes: u64, mean_think: Nanos },
}

impl Model {
    /// Whether the sender reacts to losses and marks.
    pub(crate) fn congestion_controlled(&self) -> bool {
        !matches!(self, Self::Cbr { .. })
    }
}

/// One traffic source. Build with [`FlowSpec::tcp`] and friends, then adjust
/// with the chained setters.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowSpec {
    /// Unique name, used to look the flow up in the [`crate::Report`].
    pub name: String,
    /// Name of the class (rule) its packets are classified into. A name the
    /// current config doesn't have means Everything else.
    pub class: String,
    /// The app's priority: flow weight inside the class, and express lane
    /// eligibility for small packets.
    pub prio: Priority,
    pub model: Model,
    /// Base round-trip time between the sender and the app, queues excluded.
    pub rtt: Nanos,
    pub start: Nanos,
    pub stop: Option<Nanos>,
}

impl FlowSpec {
    fn new(name: impl Into<String>, class: impl Into<String>, model: Model) -> Self {
        Self {
            name: name.into(),
            class: class.into(),
            prio: Priority::Normal,
            model,
            rtt: ms(30),
            start: 0,
            stop: None,
        }
    }

    /// A never-ending AIMD TCP download without ECN.
    pub fn tcp(name: impl Into<String>, class: impl Into<String>) -> Self {
        Self::new(
            name,
            class,
            Model::Tcp {
                ecn: false,
                bytes: None,
            },
        )
    }

    /// A never-ending QUIC-like download.
    pub fn quic(name: impl Into<String>, class: impl Into<String>) -> Self {
        Self::new(name, class, Model::Quic { bytes: None })
    }

    /// `size`-byte packets every `period`.
    pub fn cbr(
        name: impl Into<String>,
        class: impl Into<String>,
        size: usize,
        period: Nanos,
    ) -> Self {
        Self::new(name, class, Model::Cbr { size, period })
    }

    pub fn web(
        name: impl Into<String>,
        class: impl Into<String>,
        mean_bytes: u64,
        mean_think: Nanos,
    ) -> Self {
        Self::new(
            name,
            class,
            Model::Web {
                mean_bytes,
                mean_think,
            },
        )
    }

    /// Negotiate ECN (TCP only; other models ignore it).
    pub fn ecn(mut self) -> Self {
        if let Model::Tcp { ecn, .. } = &mut self.model {
            *ecn = true;
        }
        self
    }

    /// Send this many bytes, then stop (TCP and QUIC only).
    pub fn bytes(mut self, n: u64) -> Self {
        if let Model::Tcp { bytes, .. } | Model::Quic { bytes } = &mut self.model {
            *bytes = Some(n);
        }
        self
    }

    pub fn prio(mut self, prio: Priority) -> Self {
        self.prio = prio;
        self
    }

    pub fn rtt(mut self, rtt: Nanos) -> Self {
        self.rtt = rtt;
        self
    }

    pub fn start(mut self, at: Nanos) -> Self {
        self.start = at;
        self
    }

    pub fn stop(mut self, at: Nanos) -> Self {
        self.stop = Some(at);
        self
    }

    pub(crate) fn ecn_capable(&self) -> bool {
        matches!(self.model, Model::Tcp { ecn: true, .. })
    }

    /// Multiplicative decrease on congestion.
    pub(crate) fn beta(&self) -> f64 {
        match self.model {
            Model::Quic { .. } => 0.7,
            _ => 0.5,
        }
    }
}

/// Window-based congestion control, counted in packets.
#[derive(Debug, Clone)]
pub(crate) struct Cc {
    pub cwnd: f64,
    pub ssthresh: f64,
    /// Packets sent and not yet acked or reported lost.
    pub inflight: u32,
    /// Smoothed RTT; the base RTT until the first sample.
    pub srtt: Nanos,
    last_backoff: Option<Nanos>,
    beta: f64,
}

impl Cc {
    pub fn new(base_rtt: Nanos, beta: f64) -> Self {
        Self {
            cwnd: INITIAL_WINDOW,
            ssthresh: f64::INFINITY,
            inflight: 0,
            srtt: base_rtt,
            last_backoff: None,
            beta,
        }
    }

    /// Fresh connection state, as for a new transfer.
    pub fn reset(&mut self) {
        *self = Self::new(self.srtt, self.beta);
    }

    pub fn can_send(&self) -> bool {
        (self.inflight as f64) < self.cwnd.floor().max(1.0)
    }

    pub fn on_ack(&mut self, rtt_sample: Nanos, congested: bool, now: Nanos) {
        self.inflight -= 1;
        self.srtt = (7 * self.srtt + rtt_sample) / 8;
        if congested {
            self.back_off(now);
        } else if self.cwnd < self.ssthresh {
            self.cwnd += 1.0;
        } else {
            self.cwnd += 1.0 / self.cwnd;
        }
        self.cwnd = self.cwnd.min(MAX_WINDOW);
    }

    pub fn on_loss(&mut self, now: Nanos) {
        self.inflight -= 1;
        self.back_off(now);
    }

    /// Cut the window, at most once per RTT (one cut per window of data).
    fn back_off(&mut self, now: Nanos) {
        if self
            .last_backoff
            .is_some_and(|t| now.saturating_sub(t) < self.srtt)
        {
            return;
        }
        self.ssthresh = (self.cwnd * self.beta).max(MIN_WINDOW);
        self.cwnd = self.ssthresh;
        self.last_backoff = Some(now);
    }

    /// Gap between paced packets: 1.25 × the window's rate.
    pub fn pacing_gap(&self, len: usize) -> Nanos {
        let per_packet = self.srtt as f64 / (1.25 * self.cwnd.max(1.0));
        (per_packet * len as f64 / MSS as f64) as Nanos
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_start_then_linear_growth_and_one_cut_per_rtt() {
        let mut cc = Cc::new(ms(20), 0.5);
        cc.inflight = 10;
        for _ in 0..10 {
            cc.on_ack(ms(20), false, 0);
        }
        assert_eq!(cc.cwnd, 20.0);
        cc.inflight = 3;
        cc.on_loss(ms(1));
        cc.on_loss(ms(2));
        assert_eq!(cc.cwnd, 10.0, "two losses in one RTT cut once");
        cc.on_loss(ms(30));
        assert_eq!(cc.cwnd, 5.0);
        cc.inflight = 1;
        cc.on_ack(ms(20), false, ms(31));
        assert_eq!(cc.cwnd, 5.2, "congestion avoidance adds 1/cwnd");
    }

    #[test]
    fn builders() {
        let f = FlowSpec::tcp("a", "c").ecn().bytes(10).prio(Priority::High);
        assert_eq!(
            f.model,
            Model::Tcp {
                ecn: true,
                bytes: Some(10)
            }
        );
        assert!(f.ecn_capable());
        assert!(!FlowSpec::quic("q", "c").ecn().ecn_capable());
        assert!(
            !FlowSpec::cbr("g", "c", 100, ms(16))
                .model
                .congestion_controlled()
        );
    }
}
