//! The scheduler properties PLAN.md §19.1 asks the simulator to prove,
//! against closed-loop traffic: senders that back off on drops and ECN
//! marks, on a line with a real ISP buffer in front of the scheduler.

use snailnet_sim::{
    ClassConfig, FlowSpec, NANOS_PER_SEC, Nanos, Priority, Report, SchedulerConfig, SimConfig,
    Simulation, mbps, ms, percentile, shaped_config, shares,
};

const SEC: Nanos = NANOS_PER_SEC;
const LINE: u64 = mbps(25);
/// The default shaped rate: 92% of the line.
const RATE: u64 = LINE * 92 / 100;
const MTU: u64 = 1514;

fn class(name: &str, bp: u32, prio: Priority) -> ClassConfig {
    ClassConfig {
        prio,
        ..ClassConfig::new(name, bp)
    }
}

/// Rules `a` and `b` asking for these percentages, normalised as the UI
/// will (Everything else keeps its 5% floor).
fn two_rules(a: u32, b: u32) -> SchedulerConfig {
    let bp = shares::normalize(&[a * 100, b * 100]);
    shaped_config(
        RATE,
        vec![
            ClassConfig::new("a", bp[0]),
            ClassConfig::new("b", bp[1]),
            ClassConfig::new("Everything else", bp[2]),
        ],
    )
}

fn sim(config: SchedulerConfig) -> Simulation {
    Simulation::new(SimConfig::new(LINE), config).expect("valid config")
}

fn fraction(r: &Report, class: &str, from: Nanos, to: Nanos) -> f64 {
    r.class_fractions(from, to)
        .get(class)
        .copied()
        .unwrap_or(0.0)
}

fn assert_near(what: &str, got: f64, want: f64, tol: f64) {
    assert!(
        (got - want).abs() <= tol,
        "{what}: got {got:.4}, want {want:.4} ± {tol}"
    );
}

/// Four TCP flows in `class` named `<prefix>0..3`, with RTTs from 15 to
/// 120 ms: a saturating app (Steam, a torrent client) opens several.
fn four_flows(s: &mut Simulation, prefix: &str, class: &str, ecn: bool) {
    for (i, rtt) in [15, 35, 70, 120].into_iter().enumerate() {
        let f = FlowSpec::tcp(format!("{prefix}{i}"), class).rtt(ms(rtt));
        s.add(if ecn { f.ecn() } else { f });
    }
}

/// Every 2 s window from `from` to `to`, in 500 ms steps.
fn windows(from: Nanos, to: Nanos) -> impl Iterator<Item = (Nanos, Nanos)> {
    (from..=to - 2 * SEC)
        .step_by((SEC / 2) as usize)
        .map(|t| (t, t + 2 * SEC))
}

#[test]
fn forty_sixty_split_holds_whatever_the_rtts() {
    // 40/60 normalises to 38/57/5; with Everything else idle, a and b split
    // the line 40:60. Reno is RTT-unfair on a shared queue; per-class
    // queues must not be.
    let mut s = sim(two_rules(40, 60));
    four_flows(&mut s, "a", "a", false);
    s.add(FlowSpec::tcp("b0", "b").rtt(ms(40)));
    s.add(FlowSpec::tcp("b1", "b").rtt(ms(25)));
    s.add(FlowSpec::quic("b2", "b").rtt(ms(15)));
    s.add(FlowSpec::tcp("b3", "b").rtt(ms(90)));
    let r = s.run(10 * SEC);
    for (from, to) in windows(2 * SEC, 10 * SEC) {
        assert_near(
            &format!("a's share in [{from}, {to})"),
            fraction(&r, "a", from, to),
            0.40,
            0.03,
        );
        let util = r.throughput(from, to) / RATE as f64;
        assert!(util >= 0.95, "line use {util:.3} in [{from}, {to})");
    }
}

#[test]
fn a_lone_reno_flow_underfills_its_share_and_the_rest_is_not_wasted() {
    // One Reno flow per class: after each halving its window is below its
    // path's BDP, and CoDel keeps only ~5 ms queued, so its class runs dry
    // for a moment. The scheduler hands that slack to the other class, so
    // the line stays full but the split reads high for a (b, at 60%, has
    // the bigger BDP to fill). Only when both dip at once is the line
    // briefly underused. Real apps open several connections or use
    // CUBIC/BBR; this pins the behaviour so a change in it shows up.
    let mut s = sim(two_rules(40, 60));
    s.add(FlowSpec::tcp("a", "a").rtt(ms(30)));
    s.add(FlowSpec::tcp("b", "b").rtt(ms(40)));
    let r = s.run(10 * SEC);
    let (from, to) = (2 * SEC, 10 * SEC);
    let a = fraction(&r, "a", from, to);
    assert!((0.40..0.48).contains(&a), "a's share {a:.3}");
    let util = r.throughput(from, to) / RATE as f64;
    assert!(util >= 0.97, "line use {util:.3}");
}

#[test]
fn idle_share_goes_to_busy_classes_by_weight() {
    let config = shaped_config(
        RATE,
        vec![
            ClassConfig::new("a", 2000),
            ClassConfig::new("b", 3000),
            ClassConfig::new("Everything else", 5000),
        ],
    );
    let mut s = sim(config);
    s.add(FlowSpec::tcp("a1", "a").rtt(ms(20)));
    s.add(FlowSpec::tcp("a2", "a").rtt(ms(60)));
    s.add(FlowSpec::quic("o1", "Everything else").rtt(ms(30)));
    s.add(FlowSpec::tcp("o2", "Everything else").rtt(ms(45)));
    // b wakes up at 6 s.
    s.add(FlowSpec::quic("b1", "b").rtt(ms(25)).start(6 * SEC));
    let r = s.run(12 * SEC);

    // b idle: a and Everything else split 20:50.
    let (from, to) = (2 * SEC, 6 * SEC);
    assert_near("a, b idle", fraction(&r, "a", from, to), 2.0 / 7.0, 0.03);
    let util = r.throughput(from, to) / RATE as f64;
    assert!(util >= 0.95, "line use {util:.3} with b idle");

    // b busy: back to 20/30/50.
    let (from, to) = (8 * SEC, 12 * SEC);
    assert_near("a, all busy", fraction(&r, "a", from, to), 0.20, 0.03);
    assert_near("b, all busy", fraction(&r, "b", from, to), 0.30, 0.03);
}

#[test]
fn ceiling_holds_within_one_burst() {
    let ceil = mbps(5);
    let config = shaped_config(
        RATE,
        vec![
            ClassConfig {
                ceil_bytes_per_sec: Some(ceil),
                ..ClassConfig::new("capped", 6000)
            },
            ClassConfig::new("Everything else", 4000),
        ],
    );
    let mut s = sim(config);
    for (i, rtt) in [15, 40, 90].into_iter().enumerate() {
        s.add(FlowSpec::tcp(format!("c{i}"), "capped").rtt(ms(rtt)));
    }
    s.add(FlowSpec::tcp("o1", "Everything else").rtt(ms(30)));
    s.add(FlowSpec::tcp("o2", "Everything else").rtt(ms(45)));
    let r = s.run(10 * SEC);

    // The ceiling's bucket holds 10 ms at the ceiling; one packet more can
    // start on the last token.
    let burst = ceil / 100 + MTU;
    let bins = &r.classes["capped"];
    let first = (2 * SEC / r.bin) as usize;
    for width in [1, 5, 10, 50, 100] {
        for start in first..bins.len() - width {
            let got: u64 = bins[start..start + width].iter().sum();
            let allowed = ceil * width as u64 * r.bin / SEC + burst;
            assert!(
                got <= allowed,
                "{got} B in {width} bins from bin {start}, allowed {allowed}"
            );
        }
    }
    // Work-conserving under the cap: it gets close to its ceiling and the
    // rest goes to Everything else.
    let got = r.class_bytes("capped", 2 * SEC, 10 * SEC) as f64 / 8.0;
    assert!(got >= 0.95 * ceil as f64, "capped class got {got:.0} B/s");
    let util = r.throughput(2 * SEC, 10 * SEC) / RATE as f64;
    assert!(util >= 0.95, "line use {util:.3}");
}

/// The motivating mix at 92% of the line, with a game in it.
fn busy_evening(game_class: &str, timer: Nanos) -> Report {
    let config = shaped_config(
        RATE,
        vec![
            class("Game", 1000, Priority::High),
            class("Steam", 3500, Priority::Low),
            class("Brave", 5000, Priority::Normal),
            class("Everything else", 500, Priority::Normal),
        ],
    );
    let mut s = Simulation::new(
        SimConfig {
            timer_granularity: timer,
            ..SimConfig::new(LINE)
        },
        config,
    )
    .expect("valid config");
    for (i, rtt) in [18, 22, 30, 41].into_iter().enumerate() {
        s.add(FlowSpec::tcp(format!("steam{i}"), "Steam").rtt(ms(rtt)));
    }
    s.add(FlowSpec::quic("youtube", "Brave").rtt(ms(15)));
    s.add(FlowSpec::web("browsing", "Brave", 1_500_000, ms(3000)).rtt(ms(35)));
    s.add(FlowSpec::web("windows", "Everything else", 300_000, ms(2000)).rtt(ms(50)));
    // 60 Hz game ticks.
    s.add(
        FlowSpec::cbr("game", game_class, 120, ms(16))
            .prio(Priority::High)
            .rtt(ms(25)),
    );
    s.run(20 * SEC)
}

#[test]
fn light_high_priority_flow_waits_under_2ms_at_92_percent_load() {
    // A 1 ms scheduler timer is coarser than the service's; the bound must
    // hold anyway.
    for (game_class, timer) in [("Game", 0), ("Game", ms(1)), ("Everything else", ms(1))] {
        let r = busy_evening(game_class, timer);
        let game = r.flow("game");
        let p99 = percentile(&game.shaper_delay, 2 * SEC, 20 * SEC, 0.99).expect("game delivered");
        assert!(
            p99 <= ms(2),
            "game in {game_class:?}, timer {timer} ns: p99 {p99} ns in the scheduler"
        );
        // All flows start at once, so their slow starts overflow the ISP's
        // buffer in the first fraction of a second, with or without
        // SnailNet; which packet that drops is chance. From the warm-up on,
        // the game loses nothing.
        let late: Vec<_> = game.loss_times.iter().filter(|&&t| t >= 2 * SEC).collect();
        assert!(
            late.is_empty(),
            "game in {game_class:?} lost packets at {late:?}"
        );
        let util = r.throughput(2 * SEC, 20 * SEC) / RATE as f64;
        assert!(util >= 0.95, "line use {util:.3}");
    }
}

#[test]
fn low_priority_class_is_not_starved_and_the_floor_holds() {
    let config = shaped_config(
        RATE,
        vec![
            class("hi", 8500, Priority::High),
            class("lo", 1000, Priority::Low),
            class("Everything else", 500, Priority::Normal),
        ],
    );
    let mut s = sim(config);
    s.add(FlowSpec::tcp("hi1", "hi").rtt(ms(15)));
    s.add(FlowSpec::quic("hi2", "hi").rtt(ms(20)));
    s.add(FlowSpec::tcp("lo", "lo").rtt(ms(80)));
    s.add(FlowSpec::tcp("o1", "Everything else").rtt(ms(60)));
    s.add(FlowSpec::tcp("o2", "Everything else").rtt(ms(100)));
    let r = s.run(10 * SEC);
    for (from, to) in windows(2 * SEC, 10 * SEC) {
        assert_near("lo", fraction(&r, "lo", from, to), 0.10, 0.03);
        assert_near(
            "Everything else",
            fraction(&r, "Everything else", from, to),
            0.05,
            0.015,
        );
    }
}

#[test]
fn live_weight_change_loses_nothing_and_converges_within_500ms() {
    let change = 5 * SEC;
    let mut s = sim(two_rules(40, 60));
    four_flows(&mut s, "a", "a", true);
    four_flows(&mut s, "b", "b", true);
    s.publish_at(change, two_rules(70, 30)).unwrap();
    let r = s.run(10 * SEC);

    assert_near("a before", fraction(&r, "a", 3 * SEC, change), 0.40, 0.03);
    // ECN senders back off without losing a packet, and the swap itself
    // drops nothing (slow start at the beginning may).
    assert_eq!(r.lost_between(2 * SEC, 10 * SEC), 0, "losses after warm-up");
    // From 500 ms after the change, every 500 ms window is at the new
    // split. (Shorter windows swing with the senders' sawtooth.)
    let step = ms(500);
    for from in (change + step..10 * SEC).step_by(step as usize) {
        assert_near(
            &format!("a in the 500 ms from {from}"),
            fraction(&r, "a", from, from + step),
            0.70,
            0.05,
        );
    }
}

#[test]
fn removing_a_rule_moves_its_traffic_to_everything_else_without_loss() {
    let change = 5 * SEC;
    let mut s = sim(two_rules(40, 60));
    four_flows(&mut s, "a", "a", true);
    four_flows(&mut s, "b", "b", true);
    // Rule b removed: its share goes to a and Everything else in proportion.
    let bp = shares::remove(&shares::normalize(&[4000, 6000]), 1);
    let after = shaped_config(
        RATE,
        vec![
            ClassConfig::new("a", bp[0]),
            ClassConfig::new("Everything else", bp[1]),
        ],
    );
    s.publish_at(change, after).unwrap();
    let r = s.run(9 * SEC);

    assert_eq!(r.lost_between(2 * SEC, 9 * SEC), 0, "losses after warm-up");
    let want = bp[0] as f64 / 10_000.0;
    assert_near(
        "a after b's removal",
        fraction(&r, "a", change + ms(500), 9 * SEC),
        want,
        0.03,
    );
    assert!(
        r.flow_bytes("b0", change + ms(500), 9 * SEC) > 0,
        "b's flow keeps going in Everything else"
    );
}

#[test]
fn shaping_below_the_line_keeps_the_isp_buffer_empty() {
    // At the line rate the ISP's buffer is the bottleneck: it fills, and a
    // ping waits behind it. A little below, the scheduler is the bottleneck
    // and CoDel keeps its queue short.
    let ping_p50 = |factor: u64| {
        let rate = LINE * factor / 100;
        let config = shaped_config(
            rate,
            vec![
                ClassConfig::new("a", 4000),
                ClassConfig::new("Everything else", 6000),
            ],
        );
        let mut s = sim(config);
        s.add(FlowSpec::tcp("a1", "a").rtt(ms(20)));
        s.add(FlowSpec::tcp("a2", "a").rtt(ms(50)));
        s.add(FlowSpec::cbr("ping", "Everything else", 74, ms(100)).rtt(ms(11)));
        let r = s.run(10 * SEC);
        percentile(&r.flow("ping").queue_delay, 2 * SEC, 10 * SEC, 0.5).expect("pings")
    };
    let at_line = ping_p50(100);
    let shaped = ping_p50(92);
    assert!(at_line >= ms(50), "at line rate: ping +{at_line} ns");
    assert!(shaped <= ms(5), "at 92%: ping +{shaped} ns");
}

#[test]
fn runs_are_repeatable() {
    let a = busy_evening("Game", ms(1));
    let b = busy_evening("Game", ms(1));
    assert!(a == b, "two runs of the same scenario differ");
    assert!(a.flow("browsing").transfers > 0, "web transfers finished");
}

#[test]
fn a_lone_download_keeps_its_speed_when_the_timer_wakes_late() {
    // Windows wakes a waiting thread late when the PC is busy (a build on
    // every core). The root's burst must cover the lateness, or every late
    // wake-up loses speed for good: SnailNet would make the line slower
    // than it is without it (the user, 2026-11-20).
    for late in [ms(1), ms(3), ms(6)] {
        let config = shaped_config(RATE, vec![ClassConfig::new("Everything else", 10_000)]);
        let mut s = Simulation::new(
            SimConfig {
                timer_granularity: ms(1),
                wake_late: late,
                ..SimConfig::new(LINE)
            },
            config,
        )
        .expect("valid config");
        four_flows(&mut s, "dl", "Everything else", false);
        let r = s.run(10 * SEC);
        let util = r.throughput(2 * SEC, 10 * SEC) / RATE as f64;
        assert!(util >= 0.97, "waking {late} ns late: line use {util:.3}");
    }
}
