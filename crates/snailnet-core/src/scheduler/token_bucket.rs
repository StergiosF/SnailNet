//! Byte token bucket with exact integer arithmetic.
//!
//! Tokens are stored in byte·nanoseconds so refills never lose precision.
//! The bucket may go into debt by one packet: a send is allowed whenever the
//! balance is non-negative, and the packet's full size is then subtracted.
//! That way a packet larger than the burst (e.g. a coalesced 64 KB receive)
//! can still pass, and the long-run rate stays exact.

use crate::clock::{NANOS_PER_SEC, Nanos};

const NS: i128 = NANOS_PER_SEC as i128;

#[derive(Debug, Clone)]
pub struct TokenBucket {
    rate_bytes_per_sec: u64,
    burst_bytes: u64,
    /// Balance in byte·ns. Can be negative (debt) after a large packet.
    tokens: i128,
    last: Nanos,
}

impl TokenBucket {
    /// A bucket that starts full.
    pub fn new(rate_bytes_per_sec: u64, burst_bytes: u64, now: Nanos) -> Self {
        Self {
            rate_bytes_per_sec,
            burst_bytes,
            tokens: burst_bytes as i128 * NS,
            last: now,
        }
    }

    pub fn rate_bytes_per_sec(&self) -> u64 {
        self.rate_bytes_per_sec
    }

    /// Change rate and depth from `now` on. Credit earned so far at the old
    /// rate is kept (capped at the new depth), and so is any debt.
    pub fn set_rate(&mut self, rate_bytes_per_sec: u64, burst_bytes: u64, now: Nanos) {
        self.refill(now);
        self.rate_bytes_per_sec = rate_bytes_per_sec;
        self.burst_bytes = burst_bytes;
        self.tokens = self.tokens.min(burst_bytes as i128 * NS);
    }

    pub fn refill(&mut self, now: Nanos) {
        if now <= self.last {
            return;
        }
        let elapsed = (now - self.last) as i128;
        let cap = self.burst_bytes as i128 * NS;
        self.tokens = (self.tokens + elapsed * self.rate_bytes_per_sec as i128).min(cap);
        self.last = now;
    }

    /// True when a packet may be sent now (balance not in debt).
    pub fn ready(&self) -> bool {
        self.tokens >= 0
    }

    pub fn consume(&mut self, bytes: u64) {
        self.tokens -= bytes as i128 * NS;
    }

    /// Nanoseconds until [`ready`](Self::ready) becomes true, assuming no
    /// other consumption. `None` if the rate is zero and it never will.
    pub fn time_until_ready(&self) -> Option<Nanos> {
        if self.tokens >= 0 {
            return Some(0);
        }
        if self.rate_bytes_per_sec == 0 {
            return None;
        }
        let rate = self.rate_bytes_per_sec as i128;
        let debt = -self.tokens;
        Some(((debt + rate - 1) / rate) as Nanos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_full_and_allows_debt_of_one_packet() {
        let mut b = TokenBucket::new(1_000, 100, 0);
        assert!(b.ready());
        b.consume(1_500); // larger than burst: allowed, goes into debt
        assert!(!b.ready());
        // Debt is 1400 bytes at 1000 B/s → 1.4 s.
        assert_eq!(b.time_until_ready(), Some(1_400_000_000));
        b.refill(1_400_000_000);
        assert!(b.ready());
    }

    #[test]
    fn refill_is_capped_at_burst() {
        let mut b = TokenBucket::new(1_000_000, 3_000, 0);
        b.consume(3_000);
        b.refill(10 * NANOS_PER_SEC);
        b.consume(3_000);
        assert!(b.ready());
        b.consume(1);
        assert!(!b.ready());
    }

    #[test]
    fn time_never_goes_backwards() {
        let mut b = TokenBucket::new(1_000, 0, 5);
        b.consume(10);
        b.refill(1); // earlier than `last`: ignored
        assert!(!b.ready());
    }

    #[test]
    fn set_rate_keeps_debt_and_caps_credit() {
        let mut b = TokenBucket::new(1_000, 10_000, 0);
        b.set_rate(1_000, 500, 0);
        b.consume(500);
        assert!(b.ready());
        b.consume(1_000);
        b.set_rate(2_000, 500, 0);
        // 1000 B of debt at the new 2000 B/s → 0.5 s.
        assert_eq!(b.time_until_ready(), Some(500_000_000));
    }

    #[test]
    fn zero_rate_never_recovers() {
        let mut b = TokenBucket::new(0, 0, 0);
        b.consume(1);
        assert_eq!(b.time_until_ready(), None);
    }
}
