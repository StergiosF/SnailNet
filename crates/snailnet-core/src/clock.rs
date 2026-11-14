//! Time source for the scheduler. Real code uses [`MonotonicClock`]; tests use
//! [`SimClock`] so that results are exact and repeatable.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Nanoseconds since an arbitrary, clock-specific origin.
pub type Nanos = u64;

pub const NANOS_PER_SEC: Nanos = 1_000_000_000;
pub const NANOS_PER_MILLI: Nanos = 1_000_000;

pub trait Clock: Send + Sync {
    fn now(&self) -> Nanos;
}

/// Monotonic wall time, starting at zero when the clock is created.
#[derive(Debug)]
pub struct MonotonicClock {
    origin: Instant,
}

impl MonotonicClock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for MonotonicClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for MonotonicClock {
    fn now(&self) -> Nanos {
        // u64 nanoseconds overflow after ~584 years of uptime.
        self.origin.elapsed().as_nanos() as Nanos
    }
}

/// Manually advanced clock for deterministic tests and simulations.
#[derive(Debug, Default)]
pub struct SimClock {
    now: AtomicU64,
}

impl SimClock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, t: Nanos) {
        self.now.store(t, Ordering::Relaxed);
    }

    pub fn advance(&self, dt: Nanos) {
        self.now.fetch_add(dt, Ordering::Relaxed);
    }
}

impl Clock for SimClock {
    fn now(&self) -> Nanos {
        self.now.load(Ordering::Relaxed)
    }
}
