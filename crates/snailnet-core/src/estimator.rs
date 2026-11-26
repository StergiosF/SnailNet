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
//! - **Busy and the ping jumps** (> [`SLOW_MS`], the median of the last
//!   [`RISE_WINDOW`] seconds, held [`SLOW_HOLD_S`] seconds): the line got
//!   slower (Wi-Fi, peak hours). Capacity drops [`CUT`] a second, down to
//!   [`FLOOR`] of the reference speed.
//! - **Not busy:** a cut is undone. Capacity below the reference drifts
//!   back to it (half-life [`RECOVER_HALF_LIFE_S`]), and a lowered factor
//!   steps back 1% every [`FACTOR_RECOVER_S`] seconds.
//!
//! SnailNet must never make the line slower than it is without it (the
//! user, 2026-11-20). So one noisy ping can't cut anything, a cut never
//! goes deeper than [`FLOOR`], and nothing lowered stays lowered once the
//! line is quiet: before 2026-11-20 a single spike a second cut 5% down to
//! 30% of the speed, and only a fully loaded, calm line could win it back.
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
/// Never below this share of the reference speed. A line truly slower
/// than this keeps some of its own lag (as it would without SnailNet)
/// rather than SnailNet shaping it below what it can carry.
pub const FLOOR: f64 = 0.85;
/// Busy seconds whose ping rises are reduced to one (their median), so a
/// single spike says nothing.
pub const RISE_WINDOW: usize = 5;
/// Seconds the median rise must stay above [`SLOW_MS`] before a cut.
pub const SLOW_HOLD_S: u32 = 3;
/// A cut capacity halves its distance back to the reference in this many
/// seconds while the line isn't busy.
pub const RECOVER_HALF_LIFE_S: f64 = 60.0;
/// Quiet seconds per 1% of a lowered factor given back.
pub const FACTOR_RECOVER_S: u32 = 30;
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
    /// The factor it started from: quiet seconds give cuts back up to it.
    factor_start: Option<f64>,
    /// The last busy seconds' ping rises, ms.
    rises: VecDeque<f64>,
    /// Consecutive busy seconds with the median rise above [`SLOW_MS`].
    slow_for: u32,
    /// Consecutive seconds that weren't busy (or had no ping).
    idle_for: u32,
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
            factor_start: factor.map(|f| f.clamp(FACTOR_MIN, FACTOR_MAX)),
            rises: VecDeque::with_capacity(RISE_WINDOW),
            slow_for: 0,
            idle_for: 0,
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

        let Some(mut cap) = self.capacity else {
            // Nothing known: what the line has carried is the first estimate,
            // once it has carried something real.
            if self.peak >= LEARN_MIN_BPS {
                self.capacity = Some(self.peak);
                self.source = Source::Passive;
            }
            return;
        };
        if self.source == Source::Passive && self.recent.len() == SUSTAIN_S && sustained > cap {
            // Learning from live traffic: only raise when the line just proved
            // it can carry more right now (not snapping back to an old peak
            // after a congestion cut).
            cap = sustained;
            self.capacity = Some(cap);
        }
        if self.source == Source::Fixed {
            return;
        }
        let rate = cap * self.factor.unwrap_or(1.0);
        let busy = sustained >= BUSY * rate;
        let Some(raw) = t.rise_ms.filter(|_| busy) else {
            self.raised_for = 0;
            self.calm_for = 0;
            self.slow_for = 0;
            self.rises.clear();
            self.recover();
            return;
        };
        self.idle_for = 0;
        if self.rises.len() == RISE_WINDOW {
            self.rises.pop_front();
        }
        self.rises.push_back(raw);
        let rise = median(&self.rises);

        if rise > SLOW_MS {
            self.slow_for += 1;
            if self.slow_for >= SLOW_HOLD_S {
                let floor = FLOOR * self.reference.unwrap_or(self.peak.max(cap));
                self.capacity = Some((cap * (1.0 - CUT)).max(floor).min(cap));
            }
        } else {
            self.slow_for = 0;
            if rise < CALM_MS && sustained >= PRESSING * rate {
                let ceiling = self.reference.map_or(f64::INFINITY, |r| CEILING * r);
                self.capacity = Some((cap * (1.0 + RAISE_PER_MIN / 60.0)).min(ceiling.max(cap)));
            }
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

    /// A second that wasn't busy: what was cut drifts back.
    fn recover(&mut self) {
        self.idle_for = self.idle_for.saturating_add(1);
        let target = self.reference.or_else(|| {
            (self.source == Source::Passive && self.peak >= LEARN_MIN_BPS).then_some(self.peak)
        });
        if let (Some(cap), Some(r)) = (self.capacity, target)
            && cap < r
        {
            let next = r - (r - cap) * 0.5f64.powf(1.0 / RECOVER_HALF_LIFE_S);
            self.capacity = Some(if r - next < r * 1e-3 { r } else { next });
        }
        if self.idle_for.is_multiple_of(FACTOR_RECOVER_S)
            && let (Some(f), Some(start)) = (self.factor, self.factor_start)
            && f < start
        {
            self.factor = Some((f + 0.01).min(start));
        }
    }
}

/// The middle value (the mean of the middle two for an even count).
fn median(xs: &VecDeque<f64>) -> f64 {
    let mut v: Vec<f64> = xs.iter().copied().collect();
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n == 0 {
        0.0
    } else if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
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
    fn passive_congestion_cuts_stick_and_recover_when_quiet() {
        let mut d = Direction::new(None, Source::Passive, None);
        run(&mut d, 5, 20.0 * MBPS, None);
        assert!(close(d.capacity_bps().unwrap(), 20.0 * MBPS));
        // Sustained ping jump cuts passive capacity down to the floor and keeps it there.
        press(&mut d, SLOW_HOLD_S + 10, 1.0, 80.0);
        assert!(
            close(d.capacity_bps().unwrap(), d.peak_bps() * FLOOR),
            "passive cut must stick: {:?} vs floor {}",
            d.capacity_bps(),
            d.peak_bps() * FLOOR
        );
        // Once traffic goes quiet, capacity drifts back toward the peak.
        run(&mut d, 900, 0.0, None);
        assert!(
            d.capacity_bps().unwrap() > 19.8 * MBPS,
            "passive cut recovers when quiet: {:?}",
            d.capacity_bps()
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
    fn a_held_ping_jump_cuts_down_to_the_floor_and_no_further() {
        let mut d = Direction::new(Some(25.0 * MBPS), Source::Typed, None);
        press(&mut d, SLOW_HOLD_S - 1, 1.0, 80.0);
        assert!(
            close(d.capacity_bps().unwrap(), 25.0 * MBPS),
            "not held yet"
        );
        press(&mut d, 3, 1.0, 80.0);
        let cap = d.capacity_bps().unwrap();
        assert!(cap < 25.0 * MBPS, "held: cut {cap}");
        press(&mut d, 600, 1.0, 80.0);
        assert!(close(d.capacity_bps().unwrap(), 25.0 * MBPS * FLOOR));
    }

    #[test]
    fn single_spikes_never_cut() {
        // A jittery line: one second in three pings 200 ms high.
        let mut d = Direction::new(Some(25.0 * MBPS), Source::Measured, Some(0.92));
        for i in 0..3600 {
            let rise = if i % 3 == 0 { 200.0 } else { 2.0 };
            press(&mut d, 1, 1.0, rise);
        }
        assert!(
            d.capacity_bps().unwrap() >= 25.0 * MBPS,
            "{:?}",
            d.capacity_bps()
        );
    }

    #[test]
    fn a_cut_comes_back_once_the_line_is_quiet() {
        let mut d = Direction::new(Some(25.0 * MBPS), Source::Measured, Some(0.92));
        press(&mut d, 600, 1.0, 80.0);
        assert!(close(d.capacity_bps().unwrap(), 25.0 * MBPS * FLOOR));
        assert!(close(d.factor().unwrap(), FACTOR_MIN));
        // Browsing: well under the rate, whatever the ping does.
        run(&mut d, 60, 1.0 * MBPS, Some(40.0));
        let half = 25.0 * MBPS * (1.0 - (1.0 - FLOOR) / 2.0);
        assert!(
            (d.capacity_bps().unwrap() / half - 1.0).abs() < 1e-3,
            "half way back in a minute: {:?}",
            d.capacity_bps()
        );
        run(&mut d, 900, 0.0, None);
        assert!(
            close(d.capacity_bps().unwrap(), 25.0 * MBPS),
            "all the way back"
        );
        // The factor too, 1% per 30 quiet seconds, up to where it started.
        assert!(close(d.factor().unwrap(), 0.92), "{:?}", d.factor());
    }

    #[test]
    fn the_rate_never_falls_below_the_floor_of_the_line() {
        let mut d = Direction::new(Some(25.0 * MBPS), Source::Measured, Some(0.92));
        let mut seed = 7u64;
        for _ in 0..20_000 {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let rise = (seed >> 33) as f64 % 120.0;
            let share = 0.5 + ((seed >> 20) % 60) as f64 / 100.0;
            press(&mut d, 1, share, rise);
            let rate = d.rate_bps().unwrap();
            assert!(rate >= 25.0 * MBPS * FLOOR * FACTOR_MIN * 0.999, "{rate}");
        }
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
        // The median needs a few calm seconds before it reads calm.
        press(&mut d, TUNE_UP_S + RISE_WINDOW as u32 / 2, 0.85, 1.0);
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
