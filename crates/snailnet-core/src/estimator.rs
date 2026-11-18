//! The line-speed estimator (PLAN.md §10.2, §10.4), one per direction:
//! pure logic, fed once a second with what the line moved and how much the
//! ping rose over its idle baseline. The service owns the probe and applies
//! [`Direction::rate_bps`] to the shaper.
//!
//! While SnailNet shapes, traffic can't go faster than the rate it set, so
//! throughput alone can't reveal spare speed. Like cake-autorate it reads
//! the ping instead:
//!
//! - **Busy and the ping stays flat** (< [`CALM_MS`]) while traffic presses
//!   on the rate: the line has room. Capacity creeps up, at most
//!   [`RAISE_PER_MIN`] a minute.
//! - **Busy and the ping jumps** (> [`SLOW_MS`]): the line got slower
//!   (Wi-Fi, peak hours). Capacity drops [`CUT`] a second, down to
//!   [`FLOOR`] of the reference speed.
//! - **Low-latency mode** tunes its factor in between: a rise above
//!   [`TUNE_DOWN_MS`] held for [`TUNE_HOLD_S`] seconds takes 1% off (floor
//!   [`FACTOR_MIN`]); [`TUNE_UP_S`] calm busy seconds put 1% back (ceiling
//!   [`FACTOR_MAX`]).
//!
//! It also keeps the **peak sustained throughput** (5 s average, a max that
//! halves over [`PEAK_HALF_LIFE_S`]): with no speed known yet, that is the
//! first estimate.

use std::collections::VecDeque;

/// Ping rise (ms over the idle baseline) under which the line has room.
pub const CALM_MS: f64 = 5.0;
/// Ping rise that tunes Low-latency mode down.
pub const TUNE_DOWN_MS: f64 = 15.0;
/// Ping rise that means the line itself got slower.
pub const SLOW_MS: f64 = 30.0;
/// Seconds a rise above [`TUNE_DOWN_MS`] must last before the factor drops.
pub const TUNE_HOLD_S: u32 = 5;
/// Calm busy seconds before the factor goes back up (5 minutes).
pub const TUNE_UP_S: u32 = 300;
pub const FACTOR_MIN: f64 = 0.80;
pub const FACTOR_MAX: f64 = 0.97;
/// Fastest capacity rise: 10% a minute.
pub const RAISE_PER_MIN: f64 = 0.10;
/// Capacity cut per second while the line is slow.
pub const CUT: f64 = 0.05;
/// Never below this share of the reference speed.
pub const FLOOR: f64 = 0.30;
/// Never above this many times the reference speed (a misreading probe
/// can't run the rate away).
pub const CEILING: f64 = 3.0;
/// Traffic at this share of the rate or more is "busy".
pub const BUSY: f64 = 0.80;
/// Traffic at this share of the rate or more presses on it.
pub const PRESSING: f64 = 0.95;
/// The peak halves over a week.
pub const PEAK_HALF_LIFE_S: f64 = 7.0 * 24.0 * 3600.0;
/// Seconds averaged into one sustained sample.
const SUSTAIN_S: usize = 5;
/// Sustained traffic that counts as real enough to learn from, bps.
const LEARN_MIN_BPS: f64 = 500_000.0;

/// Where the reference speed came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Typed by the user and pinned: never adapted.
    Fixed,
    /// Typed by the user, adapted from there.
    Typed,
    /// From the speed test.
    Measured,
    /// Learned from traffic alone.
    Passive,
}

/// One second of one direction.
#[derive(Debug, Clone, Copy)]
pub struct Tick {
    /// Bits per second moved in this second.
    pub bps: f64,
    /// Ping rise over the idle baseline, ms; `None` without a sample.
    pub rise_ms: Option<f64>,
}

/// One direction's estimate.
#[derive(Debug, Clone)]
pub struct Direction {
    /// The speed the user typed or the test measured (bps); bounds the floor.
    reference: Option<f64>,
    source: Source,
    /// Current capacity estimate, bps.
    capacity: Option<f64>,
    /// Low-latency factor; `None` = Low-latency mode off.
    factor: Option<f64>,
    recent: VecDeque<f64>,
    peak: f64,
    /// Consecutive busy seconds with the ping above [`TUNE_DOWN_MS`].
    raised_for: u32,
    /// Consecutive busy seconds with the ping calm.
    calm_for: u32,
}

impl Direction {
    /// A direction starting from `reference` bps (none known: `None`), with
    /// Low-latency mode's starting factor, or `None` with it off.
    pub fn new(reference: Option<f64>, source: Source, factor: Option<f64>) -> Self {
        Self {
            reference,
            source,
            capacity: reference,
            factor: factor.map(|f| f.clamp(FACTOR_MIN, FACTOR_MAX)),
            recent: VecDeque::with_capacity(SUSTAIN_S),
            peak: 0.0,
            raised_for: 0,
            calm_for: 0,
        }
    }

    /// The rate to shape at, bps: capacity × factor. `None` until a speed is known.
    pub fn rate_bps(&self) -> Option<f64> {
        self.capacity.map(|c| c * self.factor.unwrap_or(1.0))
    }

    pub fn capacity_bps(&self) -> Option<f64> {
        self.capacity
    }

    pub fn factor(&self) -> Option<f64> {
        self.factor
    }

    pub fn source(&self) -> Source {
        self.source
    }

    /// The highest sustained throughput seen, decayed, bps.
    pub fn peak_bps(&self) -> f64 {
        self.peak
    }

    /// Feed one second.
    pub fn tick(&mut self, t: Tick) {
        let bps = t.bps.max(0.0);
        if self.recent.len() == SUSTAIN_S {
            self.recent.pop_front();
        }
        self.recent.push_back(bps);
        let sustained = self.recent.iter().sum::<f64>() / self.recent.len() as f64;
        if self.recent.len() == SUSTAIN_S {
            self.peak = (self.peak * 0.5f64.powf(1.0 / PEAK_HALF_LIFE_S)).max(sustained);
        }

        let Some(cap) = self.capacity else {
            // Nothing known: what the line has carried is the first estimate,
            // once it has carried something real.
            if self.peak >= LEARN_MIN_BPS {
                self.capacity = Some(self.peak);
                self.source = Source::Passive;
            }
            return;
        };
        if self.source == Source::Passive {
            // Learning from traffic: capacity follows the peak up.
            self.capacity = Some(cap.max(self.peak));
        }
        if self.source == Source::Fixed {
            return;
        }
        let rate = cap * self.factor.unwrap_or(1.0);
        let busy = sustained >= BUSY * rate;
        let Some(rise) = t.rise_ms.filter(|_| busy) else {
            self.raised_for = 0;
            self.calm_for = 0;
            return;
        };

        if rise > SLOW_MS {
            let floor = FLOOR * self.reference.unwrap_or(self.peak.max(cap));
            self.capacity = Some((cap * (1.0 - CUT)).max(floor));
        } else if rise < CALM_MS && sustained >= PRESSING * rate {
            let ceiling = self.reference.map_or(f64::INFINITY, |r| CEILING * r);
            self.capacity = Some((cap * (1.0 + RAISE_PER_MIN / 60.0)).min(ceiling.max(cap)));
        }

        if rise > TUNE_DOWN_MS {
            self.calm_for = 0;
            self.raised_for += 1;
            if self.raised_for >= TUNE_HOLD_S {
                self.raised_for = 0;
                self.factor = self.factor.map(|f| (f - 0.01).max(FACTOR_MIN));
            }
        } else if rise < CALM_MS {
            self.raised_for = 0;
            self.calm_for += 1;
            if self.calm_for >= TUNE_UP_S {
                self.calm_for = 0;
                self.factor = self.factor.map(|f| (f + 0.01).min(FACTOR_MAX));
            }
        } else {
            self.raised_for = 0;
            self.calm_for = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MBPS: f64 = 1e6;

    fn run(d: &mut Direction, secs: u32, bps: f64, rise: Option<f64>) {
        for _ in 0..secs {
            d.tick(Tick { bps, rise_ms: rise });
        }
    }

    /// `secs` seconds of traffic at the current rate (it moves as it adapts).
    fn press(d: &mut Direction, secs: u32, share: f64, rise: f64) {
        for _ in 0..secs {
            let bps = share * d.rate_bps().unwrap();
            d.tick(Tick {
                bps,
                rise_ms: Some(rise),
            });
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() / b < 1e-6
    }

    #[test]
    fn rate_is_capacity_times_factor() {
        let d = Direction::new(Some(25.0 * MBPS), Source::Typed, Some(0.92));
        assert!(close(d.rate_bps().unwrap(), 23.0 * MBPS));
        let off = Direction::new(Some(25.0 * MBPS), Source::Typed, None);
        assert!(close(off.rate_bps().unwrap(), 25.0 * MBPS));
    }

    #[test]
    fn learns_a_first_estimate_from_traffic() {
        let mut d = Direction::new(None, Source::Passive, None);
        run(&mut d, 4, 20.0 * MBPS, None);
        assert_eq!(d.capacity_bps(), None, "needs 5 s of traffic");
        run(&mut d, 1, 20.0 * MBPS, None);
        assert!(close(d.capacity_bps().unwrap(), 20.0 * MBPS));
        assert_eq!(d.source(), Source::Passive);
        run(&mut d, 10, 30.0 * MBPS, None);
        assert!(
            close(d.capacity_bps().unwrap(), 30.0 * MBPS),
            "follows the peak up"
        );
    }

    #[test]
    fn a_trickle_teaches_nothing() {
        let mut d = Direction::new(None, Source::Passive, None);
        run(&mut d, 60, 100_000.0, None);
        assert_eq!(d.capacity_bps(), None);
    }

    #[test]
    fn calm_ping_while_pressing_raises_at_most_ten_percent_a_minute() {
        let mut d = Direction::new(Some(25.0 * MBPS), Source::Measured, None);
        press(&mut d, 60, 1.0, 2.0);
        let cap = d.capacity_bps().unwrap();
        assert!(cap > 25.0 * MBPS);
        assert!(cap <= 25.0 * MBPS * 1.106, "bounded: {cap}");
    }

    #[test]
    fn capacity_stops_at_three_times_the_reference() {
        let mut d = Direction::new(Some(10.0 * MBPS), Source::Typed, None);
        press(&mut d, 3600, 1.0, 1.0);
        assert!(close(d.capacity_bps().unwrap(), 30.0 * MBPS));
    }

    #[test]
    fn idle_or_unprobed_never_moves_capacity() {
        let mut d = Direction::new(Some(25.0 * MBPS), Source::Typed, Some(0.92));
        run(&mut d, 600, 1.0 * MBPS, Some(50.0));
        run(&mut d, 600, 23.0 * MBPS, None);
        assert!(close(d.capacity_bps().unwrap(), 25.0 * MBPS));
        assert!(close(d.factor().unwrap(), 0.92));
    }

    #[test]
    fn a_ping_jump_cuts_fast_down_to_the_floor() {
        let mut d = Direction::new(Some(25.0 * MBPS), Source::Typed, None);
        press(&mut d, 5, 1.0, 80.0);
        let cap = d.capacity_bps().unwrap();
        assert!(cap < 25.0 * MBPS * 0.8, "5 s cut: {cap}");
        press(&mut d, 600, 1.0, 80.0);
        assert!(close(d.capacity_bps().unwrap(), 25.0 * MBPS * FLOOR));
    }

    #[test]
    fn a_pinned_speed_never_adapts() {
        let mut d = Direction::new(Some(25.0 * MBPS), Source::Fixed, Some(0.92));
        press(&mut d, 400, 1.0, 2.0);
        press(&mut d, 400, 1.0, 80.0);
        assert!(close(d.capacity_bps().unwrap(), 25.0 * MBPS));
        assert!(close(d.factor().unwrap(), 0.92));
    }

    #[test]
    fn low_latency_factor_tunes_down_on_a_held_rise_and_back_up_when_calm() {
        let mut d = Direction::new(Some(25.0 * MBPS), Source::Typed, Some(0.92));
        press(&mut d, 4, 1.0, 20.0);
        assert!(close(d.factor().unwrap(), 0.92), "not held long enough");
        press(&mut d, 1, 1.0, 20.0);
        assert!(close(d.factor().unwrap(), 0.91));
        press(&mut d, 1000, 1.0, 20.0);
        assert!(close(d.factor().unwrap(), FACTOR_MIN), "floor");
        // A moderate rise doesn't touch capacity.
        assert!(close(d.capacity_bps().unwrap(), 25.0 * MBPS));
        // Calm but not pressing on the rate (80–95%): factor back up, capacity
        // kept. First let the 5 s average settle with neutral seconds.
        press(&mut d, 5, 0.85, 10.0);
        press(&mut d, TUNE_UP_S, 0.85, 1.0);
        assert!(close(d.factor().unwrap(), FACTOR_MIN + 0.01));
        assert!(close(d.capacity_bps().unwrap(), 25.0 * MBPS));
    }

    #[test]
    fn the_peak_halves_over_a_week() {
        let mut d = Direction::new(Some(25.0 * MBPS), Source::Typed, None);
        run(&mut d, 5, 20.0 * MBPS, None);
        assert!(close(d.peak_bps(), 20.0 * MBPS));
        run(&mut d, PEAK_HALF_LIFE_S as u32, 0.0, None);
        assert!(
            (d.peak_bps() / (10.0 * MBPS) - 1.0).abs() < 1e-3,
            "{}",
            d.peak_bps()
        );
    }
}
