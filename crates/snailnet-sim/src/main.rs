//! `snailnet-sim`: run the Phase 0 scenario (Steam + YouTube + browsing on
//! a 25 Mbps line, PLAN.md §21) at several shaping factors and print what
//! the split and a ping would look like.
//!
//! Usage: `snailnet-sim [line Mbps] [seconds]` (defaults 25 and 30).

use snailnet_sim::{
    ClassConfig, FlowSpec, NANOS_PER_SEC, Priority, Report, SimConfig, Simulation, mbps, ms,
    percentile, shaped_config, shares,
};

const FACTORS: [f64; 5] = [1.0, 0.95, 0.92, 0.85, 0.80];

fn main() {
    let mut args = std::env::args().skip(1);
    let line_mbps: u64 = args.next().map_or(25, |a| a.parse().expect("line Mbps"));
    let secs: u64 = args.next().map_or(30, |a| a.parse().expect("seconds"));
    let line = mbps(line_mbps);
    let (from, to) = (5 * NANOS_PER_SEC, secs * NANOS_PER_SEC);
    assert!(to > from, "run for more than 5 s");

    println!(
        "{line_mbps} Mbps line, 150 ms ISP buffer; Steam 40% Low (4 TCP), Brave 60% \
         (YouTube QUIC + web); ping in Everything else. Measured {}–{secs} s.\n",
        from / NANOS_PER_SEC
    );
    println!(
        "{:>6} {:>8} {:>7} {:>7} {:>7} {:>10} {:>10} {:>12} {:>8}",
        "factor", "Mbps", "Steam", "Brave", "other", "ping p50", "ping p99", "in shaper", "lost"
    );
    for factor in FACTORS {
        let r = run(line, factor, to);
        let shares = r.class_fractions(from, to);
        let pct = |k: &str| shares.get(k).copied().unwrap_or(0.0) * 100.0;
        let ping = &r.flow("ping").queue_delay;
        let p = |q| percentile(ping, from, to, q).map_or(f64::NAN, |d| d as f64 / 1e6);
        let in_shaper = percentile(&r.flow("ping").shaper_delay, from, to, 0.99)
            .map_or(f64::NAN, |d| d as f64 / 1e6);
        println!(
            "{factor:>6.2} {:>8.2} {:>6.1}% {:>6.1}% {:>6.1}% {:>8.1}ms {:>8.1}ms {:>10.1}ms {:>8}",
            r.throughput(from, to) * 8.0 / 1e6,
            pct("Steam"),
            pct("Brave"),
            pct("Everything else"),
            p(0.5),
            p(0.99),
            in_shaper,
            r.total_lost(),
        );
    }
    println!(
        "\nping p50/p99: delay added to an idle ping (ISP buffer + scheduler); \
         in shaper: the scheduler's part, p99."
    );
}

fn run(line: u64, factor: f64, until: u64) -> Report {
    let bp = shares::normalize(&[4000, 6000]);
    let classes = vec![
        ClassConfig {
            prio: Priority::Low,
            ..ClassConfig::new("Steam", bp[0])
        },
        ClassConfig::new("Brave", bp[1]),
        ClassConfig::new("Everything else", bp[2]),
    ];
    let rate = (line as f64 * factor) as u64;
    let mut sim =
        Simulation::new(SimConfig::new(line), shaped_config(rate, classes)).expect("valid config");
    for (i, rtt) in [18, 22, 30, 41].into_iter().enumerate() {
        sim.add(FlowSpec::tcp(format!("steam{i}"), "Steam").rtt(ms(rtt)));
    }
    sim.add(FlowSpec::quic("youtube", "Brave").rtt(ms(15)));
    sim.add(FlowSpec::web("browsing", "Brave", 1_500_000, ms(4000)).rtt(ms(35)));
    sim.add(FlowSpec::cbr("ping", "Everything else", 74, ms(100)).rtt(ms(11)));
    sim.run(until)
}
