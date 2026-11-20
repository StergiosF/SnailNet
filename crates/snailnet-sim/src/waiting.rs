//! "Waiting": which app keeps asking for more than it gets (the one-tap
//! fix's signal, docs/HANDOFF-wow.md, Feature 2). Proven here against
//! closed-loop traffic (`tests/waiting.rs`) before the app reads the same
//! numbers from the service's stats tick.
//!
//! An app is **pressed** in a tick when the scheduler acted on it (CoDel
//! dropped or ECN-marked one of its packets, or the queue limit dropped
//! one) or any of its bytes are queued at the tick. A flow whose bottleneck
//! is SnailNet is backlogged nearly all the time; one that gets what it
//! asks for is served the moment it arrives (new flows go first), so its
//! queue is empty at almost every tick. CoDel actions alone aren't enough:
//! a long-RTT flow's sawtooth is slow, so at 100 ms CoDel acts in fewer than
//! half the ticks, while its queue is non-empty in 97% of them.
//!
//! Every greedy app is pressed, the hog included. What sets the waiting app
//! apart is that it is also **losing**: over the window it got less than
//! [`WaitRule::losing`] of an equal split of what the line delivered, split
//! among the apps that were pressed enough to count.
//!
//! An app is waiting when, over the last [`WaitRule::window`] ticks, it was
//! pressed in at least [`WaitRule::min_pressed`] of them and it was losing.

use std::collections::VecDeque;

use crate::report::AppSample;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaitRule {
    /// Ticks looked back over.
    pub window: usize,
    /// Pressed in at least this many of them.
    pub min_pressed: usize,
    /// Losing: delivered less than this fraction of an equal split.
    pub losing: f64,
}

/// The service's stats tick.
pub const TICK_MS: u64 = 250;

/// The rule the simulator settled on: pressed in 14 of the last 20 ticks
/// (70% of 5 s), and under half an equal split.
pub const RULE: WaitRule = WaitRule {
    window: 20,
    min_pressed: 14,
    losing: 0.5,
};

/// Whether `s` shows the scheduler holding the app back in that tick.
pub fn pressed(s: &AppSample) -> bool {
    s.acted > 0 || s.queued_bytes > 0
}

/// Feed it one tick at a time; it says who is waiting.
#[derive(Debug, Clone)]
pub struct WaitWatch {
    rule: WaitRule,
    ticks: VecDeque<Vec<AppSample>>,
}

impl WaitWatch {
    pub fn new(rule: WaitRule) -> Self {
        Self {
            rule,
            ticks: VecDeque::with_capacity(rule.window + 1),
        }
    }

    /// Add a tick (one [`AppSample`] per app, same order every tick) and
    /// return, per app, whether it is waiting now.
    pub fn feed(&mut self, apps: &[AppSample]) -> Vec<bool> {
        self.ticks.push_back(apps.to_vec());
        if self.ticks.len() > self.rule.window {
            self.ticks.pop_front();
        }
        let n = apps.len();
        let mut hits = vec![0usize; n];
        let mut sent = vec![0u64; n];
        for tick in &self.ticks {
            for (a, s) in tick.iter().enumerate().take(n) {
                hits[a] += usize::from(pressed(s));
                sent[a] += s.sent_bytes;
            }
        }
        let busy: Vec<bool> = hits.iter().map(|&h| h >= self.rule.min_pressed).collect();
        let contenders = busy.iter().filter(|&&b| b).count().max(1);
        let fair = sent.iter().sum::<u64>() as f64 / contenders as f64;
        (0..n)
            .map(|a| busy[a] && (sent[a] as f64) < self.rule.losing * fair)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(sent_bytes: u64, queued_bytes: u64, acted: u64) -> AppSample {
        AppSample {
            sent_bytes,
            queued_bytes,
            acted,
        }
    }

    #[test]
    fn the_pressed_loser_waits_and_the_pressed_winner_does_not() {
        let mut w = WaitWatch::new(RULE);
        let mut last = Vec::new();
        for _ in 0..RULE.min_pressed {
            last = w.feed(&[s(700_000, 50_000, 5), s(40_000, 0, 1), s(1_000, 0, 0)]);
        }
        assert_eq!(last, [false, true, false]);
    }

    #[test]
    fn a_queue_without_codel_action_counts_as_pressed() {
        let mut w = WaitWatch::new(RULE);
        let mut last = Vec::new();
        for _ in 0..RULE.min_pressed {
            last = w.feed(&[s(700_000, 50_000, 5), s(40_000, 1500, 0)]);
        }
        assert_eq!(last, [false, true]);
    }

    #[test]
    fn needs_enough_pressed_ticks_in_the_window() {
        let mut w = WaitWatch::new(RULE);
        for _ in 1..RULE.min_pressed {
            assert_eq!(
                w.feed(&[s(700_000, 50_000, 5), s(40_000, 0, 1)]),
                [false, false]
            );
        }
        // Quiet ticks push the pressed ones out of the window.
        for _ in 0..RULE.window {
            assert_eq!(
                w.feed(&[s(700_000, 50_000, 5), s(40_000, 0, 0)]),
                [false, false]
            );
        }
    }

    #[test]
    fn equal_contenders_are_not_losing() {
        let mut w = WaitWatch::new(RULE);
        for _ in 0..RULE.window {
            let v = w.feed(&[
                s(300_000, 9_000, 2),
                s(250_000, 9_000, 2),
                s(220_000, 9_000, 1),
            ]);
            assert_eq!(v, [false, false, false]);
        }
    }
}
