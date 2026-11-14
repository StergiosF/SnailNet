//! Offline simulator for the SnailNet scheduler (PLAN.md §5, §19.1).
//!
//! Where the core's own tests feed the scheduler open-loop traffic, this
//! crate closes the loop: senders react to the drops, ECN marks and delay
//! the scheduler causes, the way real TCP and QUIC do. That's what download
//! shaping depends on, since the scheduler only holds packets that already
//! crossed the internet.
//!
//! The modelled path, for the download direction:
//!
//! ```text
//! sender ──rtt/2──▶ ISP link (line rate, FIFO buffer) ──▶ scheduler (shaped
//!   ▲                                                     rate) ──▶ app
//!   └──────────────────────── ack / loss signal, rtt/2 ◀──────────────┘
//! ```
//!
//! Shaping below the line rate keeps the ISP's bloated buffer empty; the
//! scheduler's CoDel queues take its place. Everything runs on a
//! [`snailnet_core::clock::SimClock`], so runs are exact and repeatable.
//!
//! Not modelled: the upload direction (TCP ACKs, which the express lane
//! carries in the real service), packet reordering, delayed ACKs, and
//! retransmission timeouts (every loss is signalled to its sender).

mod report;
mod rng;
mod sim;
mod traffic;

pub use report::{FlowReport, Report, percentile};
pub use sim::{SimConfig, Simulation, shaped_config};
pub use traffic::{FlowSpec, MSS, Model};

pub use snailnet_core::clock::{NANOS_PER_MILLI, NANOS_PER_SEC, Nanos};
pub use snailnet_core::scheduler::{ClassConfig, Priority, SchedulerConfig, shares};

/// Milliseconds to [`Nanos`].
pub const fn ms(n: u64) -> Nanos {
    n * NANOS_PER_MILLI
}

/// Megabits per second to bytes per second.
pub const fn mbps(n: u64) -> u64 {
    n * 1_000_000 / 8
}
