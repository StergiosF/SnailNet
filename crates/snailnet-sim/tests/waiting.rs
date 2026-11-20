//! The one-tap fix's signal (docs/HANDOFF-wow.md, Feature 2, step 2.0).
//! With every app in Everything else, per-connection fair queuing gives a
//! 16-connection download 16 times what a one-connection video gets. The
//! scheduler's per-app pressure, read at the service's 250 ms stats tick
//! and judged by [`RULE`], must name the video as waiting within 15 s, and
//! must name neither the download nor a video whose buffer is full.
//!
//! Line 25 Mbps shaped at 92%: 23 Mbps, so with 17 flows each gets about
//! 1.35 Mbps.

use snailnet_sim::{
    AppSample, ClassConfig, FlowSpec, NANOS_PER_SEC, Nanos, RULE, Report, SimConfig, Simulation,
    TICK_MS, WaitRule, WaitWatch, mbps, ms, shaped_config,
};

const SEC: Nanos = NANOS_PER_SEC;
const LINE: u64 = mbps(25);
/// The default shaped rate: 92% of the line.
const RATE: u64 = LINE * 92 / 100;
const OTHER: &str = "Everything else";
/// The video joins the downloads here.
const VIDEO_AT: Nanos = 10 * SEC;
const END: Nanos = 60 * SEC;

/// A download of `n` TCP connections, RTTs 15 ms and up in 7 ms steps.
fn download(app: &str, n: u64) -> Vec<FlowSpec> {
    (0..n)
        .map(|i| {
            FlowSpec::tcp(format!("{app}{i}"), OTHER)
                .app(app)
                .rtt(ms(15 + i * 7))
        })
        .collect()
}

fn steam() -> Vec<FlowSpec> {
    download("steam", 16)
}

/// YouTube in Brave wanting 8 Mbps, one QUIC connection.
fn video(rtt: u64) -> FlowSpec {
    FlowSpec::video("youtube", OTHER, mbps(8))
        .app("brave")
        .rtt(ms(rtt))
        .start(VIDEO_AT)
}

/// The same video with a full buffer, topping it up: every 4 s, the next
/// 4 s of a `kbps` stream, fetched as fast as allowed.
fn topping_up(kbps: u64) -> FlowSpec {
    FlowSpec::video("youtube", OTHER, mbps(8))
        .chunks(kbps * 1000 / 8 * 4, 4 * SEC)
        .app("brave")
        .rtt(ms(20))
        .start(VIDEO_AT)
}

/// Everything in Everything else, sampled at the stats tick.
fn run(flows: impl IntoIterator<Item = FlowSpec>) -> Report {
    let config = shaped_config(RATE, vec![ClassConfig::new(OTHER, 10_000)]);
    let mut s = Simulation::new(SimConfig::new(LINE), config).expect("valid config");
    for f in flows {
        s.add(f);
    }
    s.sample_every(ms(TICK_MS));
    s.run(END)
}

fn with(mut flows: Vec<FlowSpec>, extra: FlowSpec) -> Vec<FlowSpec> {
    flows.push(extra);
    flows
}

/// Per tick: `(time, waiting per app)`.
fn verdicts(r: &Report, rule: WaitRule) -> Vec<(Nanos, Vec<bool>)> {
    let mut w = WaitWatch::new(rule);
    r.samples.iter().map(|s| (s.at, w.feed(&s.apps))).collect()
}

/// How long after the video started `app` first showed as waiting, and in
/// what fraction of the ticks from then on.
fn first_waiting(r: &Report, rule: WaitRule, app: &str) -> Option<(f64, f64)> {
    let a = r.app(app);
    let v = verdicts(r, rule);
    let first = v.iter().position(|(_, w)| w[a])?;
    let held = v[first..].iter().filter(|(_, w)| w[a]).count();
    let after = v[first].0.saturating_sub(VIDEO_AT) as f64 / 1e9;
    Some((after, held as f64 / (v.len() - first) as f64))
}

/// Ticks in which `app` showed as waiting.
fn waiting_ticks(r: &Report, rule: WaitRule, app: &str) -> usize {
    let a = r.app(app);
    verdicts(r, rule).iter().filter(|(_, w)| w[a]).count()
}

/// What `app` got from 20 s to the end, Mbps.
fn got(r: &Report, app: &str) -> f64 {
    let a = r.app(app);
    let bytes: u64 = r
        .samples
        .iter()
        .filter(|s| s.at > 20 * SEC)
        .map(|s| s.apps[a].sent_bytes)
        .sum();
    bytes as f64 * 8.0 / ((END - 20 * SEC) as f64 / 1e9) / 1e6
}

#[test]
fn a_starved_video_shows_as_waiting_within_15_s() {
    // Measured: 3.50 s at 20 and 50 ms, 3.75 s at 100 ms, 4.25 s at 150 ms;
    // then waiting in every tick (150 ms: 92%). At 250 ms it is too
    // intermittent (49%) for the rule; see `sweep`.
    for (rtt, min_held) in [(20, 0.99), (50, 0.99), (100, 0.99), (150, 0.85)] {
        let r = run(with(steam(), video(rtt)));
        let (after, held) = first_waiting(&r, RULE, "brave")
            .unwrap_or_else(|| panic!("rtt {rtt}: the video never showed as waiting"));
        let mbps = got(&r, "brave");
        assert!(after <= 15.0, "rtt {rtt}: waiting only after {after:.2} s");
        assert!(
            held >= min_held,
            "rtt {rtt}: then waiting in only {held:.2} of ticks"
        );
        assert!(
            mbps < 2.0,
            "rtt {rtt}: the video wasn't starved ({mbps:.2} Mbps)"
        );
        assert_eq!(
            waiting_ticks(&r, RULE, "steam"),
            0,
            "rtt {rtt}: Steam waiting"
        );
    }
}

#[test]
fn the_hog_never_shows_as_waiting() {
    for (what, r) in [
        ("alone", run(steam())),
        ("with a starved video", run(with(steam(), video(20)))),
        (
            "with a topping-up video",
            run(with(steam(), topping_up(500))),
        ),
    ] {
        assert_eq!(waiting_ticks(&r, RULE, "steam"), 0, "Steam waiting {what}");
    }
}

#[test]
fn a_video_with_a_full_buffer_does_not_show_as_waiting() {
    // A top-up stream of 250–750 kbps keeps SnailNet busy for 19–54% of
    // the ticks (its share is ~1.35 Mbps): never waiting. At 1000 kbps
    // (74% busy) it shows as waiting in 123 of 199 ticks, and at 1250 kbps
    // it is starved in all but name.
    for kbps in [250, 500, 750] {
        let r = run(with(steam(), topping_up(kbps)));
        let mbps = got(&r, "brave");
        let want = kbps as f64 / 1000.0;
        assert!((mbps - want).abs() < 0.1, "{kbps} kbps: got {mbps:.2} Mbps");
        assert_eq!(waiting_ticks(&r, RULE, "brave"), 0, "{kbps} kbps: waiting");
    }
}

#[test]
fn a_video_alone_on_the_line_does_not_show_as_waiting() {
    let r = run([video(20)]);
    let mbps = got(&r, "brave");
    assert!(mbps > 7.5, "the video should get its 8 Mbps: {mbps:.2}");
    assert_eq!(waiting_ticks(&r, RULE, "brave"), 0);
}

#[test]
fn equal_downloads_do_not_wait_on_each_other() {
    let mut flows = download("steam", 8);
    flows.extend(download("epic", 8));
    let r = run(flows);
    assert_eq!(waiting_ticks(&r, RULE, "steam"), 0);
    assert_eq!(waiting_ticks(&r, RULE, "epic"), 0);
}

#[test]
fn with_two_hogs_only_the_video_waits() {
    let mut flows = download("steam", 8);
    flows.extend(download("epic", 8));
    let r = run(with(flows, video(20)));
    let (after, held) = first_waiting(&r, RULE, "brave").expect("the video never waited");
    println!("two hogs: the video waits after {after:.2} s, then in {held:.2} of ticks");
    assert!(after <= 15.0, "waiting only after {after:.2} s");
    assert_eq!(waiting_ticks(&r, RULE, "steam"), 0);
    assert_eq!(waiting_ticks(&r, RULE, "epic"), 0);
}

/// Tuning aid: `cargo test --release -p snailnet-sim --test waiting sweep
/// -- --ignored --nocapture` tries other thresholds on every scenario.
#[test]
#[ignore]
fn sweep() {
    let rtts: Vec<(u64, Report)> = [20, 50, 100, 150, 250]
        .into_iter()
        .map(|rtt| (rtt, run(with(steam(), video(rtt)))))
        .collect();
    let ups: Vec<(u64, Report)> = [250, 500, 750, 1000, 1250]
        .into_iter()
        .map(|k| (k, run(with(steam(), topping_up(k)))))
        .collect();
    let alone = run(steam());
    for window in [12, 20, 28, 40] {
        for min_pressed in window / 2..=window {
            for losing in [0.5, 0.75] {
                let rule = WaitRule {
                    window,
                    min_pressed,
                    losing,
                };
                let starved: Vec<String> = rtts
                    .iter()
                    .map(|(rtt, r)| match first_waiting(r, rule, "brave") {
                        Some((after, held)) => format!("{rtt}ms {after:.2}s/{held:.2}"),
                        None => format!("{rtt}ms never"),
                    })
                    .collect();
                let hog: usize = rtts
                    .iter()
                    .chain(&ups)
                    .map(|(_, r)| r)
                    .chain([&alone])
                    .map(|r| waiting_ticks(r, rule, "steam"))
                    .sum();
                let fp: Vec<String> = ups
                    .iter()
                    .map(|(k, r)| format!("{k}:{}", waiting_ticks(r, rule, "brave")))
                    .collect();
                println!(
                    "{window}/{min_pressed} losing {losing}: starved {} | hog {hog} | top-ups {}",
                    starved.join(", "),
                    fp.join(" ")
                );
            }
        }
    }
}

/// Tuning aid: how often each per-tick signal fires for the video, after
/// it settled. Shows why "pressed" includes any queued bytes: at long RTTs
/// CoDel acts on a starved flow in too few ticks.
#[test]
#[ignore]
fn signal_rates() {
    let mut runs: Vec<(String, Report)> = Vec::new();
    for rtt in [20, 50, 100, 150, 250] {
        runs.push((
            format!("starved, rtt {rtt}"),
            run(with(steam(), video(rtt))),
        ));
    }
    for k in [250, 500, 750, 1000, 1250] {
        runs.push((
            format!("top-up {k} kbps"),
            run(with(steam(), topping_up(k))),
        ));
    }
    for (what, r) in &runs {
        let a = r.app("brave");
        let ticks: Vec<AppSample> = r
            .samples
            .iter()
            .filter(|s| s.at > VIDEO_AT + 2 * SEC)
            .map(|s| s.apps[a])
            .collect();
        let rate = |p: fn(&AppSample) -> bool| {
            ticks.iter().filter(|s| p(s)).count() as f64 / ticks.len() as f64
        };
        println!(
            "{what:>20}: acted {:.2}  queued {:.2}  queued>6000 {:.2}  either {:.2}  {:.2} Mbps",
            rate(|s| s.acted > 0),
            rate(|s| s.queued_bytes > 0),
            rate(|s| s.queued_bytes > 6000),
            rate(|s| s.acted > 0 || s.queued_bytes > 0),
            got(r, "brave"),
        );
    }
}

/// Tuning aid: every tick of one scenario (rate, CoDel actions, queued
/// bytes per app; W = waiting).
#[test]
#[ignore]
fn print_ticks() {
    let r = run(with(steam(), video(20)));
    println!("apps {:?}", r.apps);
    for (s, (_, w)) in r.samples.iter().zip(verdicts(&r, RULE)) {
        let cols: Vec<String> = s
            .apps
            .iter()
            .zip(w)
            .map(|(a, w)| {
                let kbps = a.sent_bytes * 8 / TICK_MS;
                let mark = if w { "W" } else { " " };
                format!("{kbps:>6}/{:>3}/{:>6}{mark}", a.acted, a.queued_bytes)
            })
            .collect();
        println!("{:>6.2}  {}", s.at as f64 / 1e9, cols.join("  "));
    }
}
