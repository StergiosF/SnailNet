//! Packet interception (PLAN.md §6).
//!
//! [`PacketEngine`] is the seam the scheduler talks to. [`windivert`] is the
//! only implementation today; a WFP driver or a test mock can slot in later.
//! All WinDivert `unsafe` is confined to this crate.

pub mod address;
pub mod batch;
#[cfg(windows)]
pub mod windivert;

pub use address::{Address, FlowData};
pub use batch::{RecvBatch, SendBatch};

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("WinDivert.dll not found. Looked in: {0}")]
    LibraryNotFound(String),
    #[error("could not load WinDivert.dll from {path}: {source}")]
    LibraryLoad {
        path: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("WinDivertOpen failed (error {code}): {hint}")]
    Open { code: i32, hint: &'static str },
    #[error("the handle was shut down")]
    Shutdown,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Receive packets, send them back later. Closing the engine (or the process
/// dying) must restore normal networking: that is the fail-open contract.
pub trait PacketEngine: Send + Sync {
    /// Block until at least one packet arrives. Returns the packet count.
    fn recv_batch(&self, batch: &mut RecvBatch) -> Result<usize, EngineError>;
    /// Reinject every packet in `batch`, then clear it.
    fn send_batch(&self, batch: &mut SendBatch) -> Result<(), EngineError>;
    /// Unblock pending and future receives. Sends keep working so queued
    /// packets can be flushed.
    fn shutdown(&self);
}
