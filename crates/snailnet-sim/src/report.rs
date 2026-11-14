//! What a run measured.

use std::collections::BTreeMap;

use snailnet_core::clock::Nanos;
use snailnet_core::scheduler::ClassStats;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlowReport {
    pub name: String,
    /// The class name the flow asked for.
    pub class: String,
    pub sent_packets: u64,
    pub delivered_packets: u64,
    pub delivered_bytes: u64,
    /// Dropped anywhere: the ISP buffer or the scheduler.
    pub lost_packets: u64,
    /// When each of those drops happened.
    pub loss_times: Vec<Nanos>,
    /// Delivered with ECN Congestion Experienced set.
    pub marked_packets: u64,
    /// Web transfers finished.
    pub transfers: u64,
    /// Still on the way when the run ended (path, ISP buffer, scheduler).
    /// `sent = delivered + lost + in_network`, checked at the end of a run.
    pub in_network: u64,
    /// Bytes delivered to the app in each [`Report::bin`].
    pub bins: Vec<u64>,
    /// `(delivered at, time spent in the scheduler)` per delivered packet.
    pub shaper_delay: Vec<(Nanos, Nanos)>,
    /// `(delivered at, queueing delay)` per delivered packet: one-way time
    /// from the sender minus the base path delay, so ISP buffer, line
    /// serialisation and scheduler together. What a ping would see added.
    pub queue_delay: Vec<(Nanos, Nanos)>,
}

impl FlowReport {
    /// Bytes delivered in `[from, to)`. Both must be multiples of `bin`.
    pub fn bytes(&self, bin: Nanos, from: Nanos, to: Nanos) -> u64 {
        sum_bins(&self.bins, bin, from, to)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// Width of the throughput bins.
    pub bin: Nanos,
    pub end: Nanos,
    pub flows: Vec<FlowReport>,
    /// Bytes delivered per scheduler class (by the class's name at the time),
    /// per bin.
    pub classes: BTreeMap<String, Vec<u64>>,
    /// Scheduler counters per class of the final config.
    pub shaper: Vec<(String, ClassStats)>,
    /// Packets the ISP buffer dropped.
    pub isp_drops: u64,
}

impl Report {
    pub fn flow(&self, name: &str) -> &FlowReport {
        self.flows
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("no flow named {name:?}"))
    }

    pub fn flow_bytes(&self, name: &str, from: Nanos, to: Nanos) -> u64 {
        self.flow(name).bytes(self.bin, from, to)
    }

    /// Bytes the scheduler sent for class `name` in `[from, to)`.
    pub fn class_bytes(&self, name: &str, from: Nanos, to: Nanos) -> u64 {
        self.classes
            .get(name)
            .map_or(0, |b| sum_bins(b, self.bin, from, to))
    }

    /// Each class's fraction of all bytes sent in `[from, to)`.
    pub fn class_fractions(&self, from: Nanos, to: Nanos) -> BTreeMap<String, f64> {
        let bytes: BTreeMap<_, _> = self
            .classes
            .keys()
            .map(|k| (k.clone(), self.class_bytes(k, from, to)))
            .collect();
        let total: u64 = bytes.values().sum();
        bytes
            .into_iter()
            .map(|(k, b)| (k, b as f64 / total.max(1) as f64))
            .collect()
    }

    /// Total delivered rate in `[from, to)`, bytes per second.
    pub fn throughput(&self, from: Nanos, to: Nanos) -> f64 {
        let bytes: u64 = self
            .classes
            .keys()
            .map(|k| self.class_bytes(k, from, to))
            .sum();
        bytes as f64 * 1e9 / (to - from) as f64
    }

    pub fn total_lost(&self) -> u64 {
        self.flows.iter().map(|f| f.lost_packets).sum()
    }

    /// Packets of any flow dropped in `[from, to)`.
    pub fn lost_between(&self, from: Nanos, to: Nanos) -> usize {
        self.flows
            .iter()
            .flat_map(|f| &f.loss_times)
            .filter(|t| (from..to).contains(t))
            .count()
    }
}

fn sum_bins(bins: &[u64], bin: Nanos, from: Nanos, to: Nanos) -> u64 {
    assert!(
        from.is_multiple_of(bin) && to.is_multiple_of(bin) && from <= to,
        "[{from}, {to}) is not aligned to {bin} ns bins"
    );
    let (a, b) = ((from / bin) as usize, (to / bin) as usize);
    bins.iter().take(b).skip(a).sum()
}

/// The `p` quantile (0..=1) of the delays of `samples` delivered in
/// `[from, to)`, or `None` if there are none.
pub fn percentile(samples: &[(Nanos, Nanos)], from: Nanos, to: Nanos, p: f64) -> Option<Nanos> {
    let mut v: Vec<Nanos> = samples
        .iter()
        .filter(|(t, _)| (from..to).contains(t))
        .map(|&(_, d)| d)
        .collect();
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    let rank = ((v.len() as f64 * p).ceil() as usize).clamp(1, v.len());
    Some(v[rank - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_is_nearest_rank() {
        let s: Vec<_> = (1..=100).map(|d| (d, d)).collect();
        assert_eq!(percentile(&s, 0, 1000, 0.99), Some(99));
        assert_eq!(percentile(&s, 0, 1000, 1.0), Some(100));
        assert_eq!(percentile(&s, 0, 1000, 0.0), Some(1));
        assert_eq!(percentile(&s, 51, 61, 0.5), Some(55));
        assert_eq!(percentile(&s, 500, 600, 0.5), None);
    }

    #[test]
    fn bins_sum_in_range() {
        assert_eq!(sum_bins(&[1, 2, 3, 4], 10, 10, 30), 5);
        assert_eq!(sum_bins(&[1, 2], 10, 0, 100), 3);
    }
}
