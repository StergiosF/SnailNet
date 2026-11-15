//! The fast path on real threads and a real clock, against [`MockEngine`]:
//! shaping rate and split, express lane, bypass rules, the stall watchdog
//! (PLAN.md §6.4), pool exhaustion, and "nothing is lost" on every exit.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use snailnet_core::parse::ip::PacketInfo;
use snailnet_core::scheduler::{ClassConfig, PacketMeta, SchedulerConfig};
use snailnet_engine::fastpath::{Classifier, FastPath, FastPathConfig, Nap, ReconfigureError, Tag};
use snailnet_engine::mock::MockEngine;

const REMOTE: [u8; 4] = [8, 8, 4, 4];
const LOCAL: [u8; 4] = [192, 168, 1, 10];
const LAN_PEER: [u8; 4] = [192, 168, 1, 5];
/// Remote port of the packets that belong to rule "a"; anything else is
/// Everything else.
const PORT_A: u16 = 1000;
const PORT_B: u16 = 2000;

/// An IPv4 TCP packet `remote → local` (inbound), ECN-capable so CoDel marks
/// instead of dropping. `id` goes in the first payload bytes.
fn inbound(from: [u8; 4], port: u16, payload: usize, id: u32) -> Vec<u8> {
    let total = 40 + payload;
    let mut p = vec![0u8; total];
    p[0] = 0x45;
    p[1] = 0x02; // ECT(0)
    p[2..4].copy_from_slice(&(total as u16).to_be_bytes());
    p[8] = 64;
    p[9] = 6;
    p[12..16].copy_from_slice(&from);
    p[16..20].copy_from_slice(&LOCAL);
    p[20..22].copy_from_slice(&port.to_be_bytes());
    p[22..24].copy_from_slice(&50_000u16.to_be_bytes());
    p[32] = 0x50; // data offset: 5 words
    p[33] = if payload == 0 { 0x10 } else { 0x18 }; // ACK / PSH+ACK
    if payload >= 4 {
        p[40..44].copy_from_slice(&id.to_be_bytes());
    }
    p
}

fn id_of(p: &[u8]) -> u32 {
    u32::from_be_bytes(p[40..44].try_into().unwrap())
}

fn port_of(p: &[u8]) -> u16 {
    u16::from_be_bytes([p[20], p[21]])
}

#[derive(Clone)]
struct ByPort;

impl Classifier for ByPort {
    fn classify(&mut self, _pkt: &[u8], info: &PacketInfo, outbound: bool) -> (PacketMeta, Tag) {
        let remote = if outbound {
            info.dst_port
        } else {
            info.src_port
        };
        // Tagged by remote port, so the per-tag counts can be checked.
        (
            PacketMeta::new(usize::from(remote != PORT_A), remote as u64),
            Tag::from(remote),
        )
    }
}

/// Precise waits without a Windows high-resolution timer.
struct SpinNap;

impl Nap for SpinNap {
    fn nap(&mut self, d: Duration) {
        let end = Instant::now() + d;
        while Instant::now() < end {
            std::thread::yield_now();
        }
    }
}

/// Rule "a" 40%, Everything else 60%.
fn config(rate: u64) -> SchedulerConfig {
    SchedulerConfig {
        rate_bytes_per_sec: rate,
        burst_bytes: (rate / 50).max(3028),
        classes: vec![ClassConfig::new("a", 4000), ClassConfig::new("other", 6000)],
    }
}

fn start(
    engine: &Arc<MockEngine>,
    rate: u64,
    tweak: impl FnOnce(&mut FastPathConfig),
) -> FastPath<MockEngine> {
    let mut fc = FastPathConfig {
        small_buffers: 1024,
        large_buffers: 4,
        ..FastPathConfig::default()
    };
    tweak(&mut fc);
    FastPath::start(
        engine.clone(),
        Some(config(rate)),
        None,
        ByPort,
        Box::new(SpinNap),
        fc,
    )
    .unwrap()
}

fn wait(mut cond: impl FnMut() -> bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    cond()
}

#[test]
fn shapes_at_the_rate_splits_by_share_and_returns_every_buffer() {
    let engine = Arc::new(MockEngine::new());
    let rate = 2_000_000;
    let fp = start(&engine, rate, |_| {});
    let per_class = 200;
    for i in 0..per_class {
        engine.inject(&inbound(REMOTE, PORT_A, 1000, i), false);
        engine.inject(&inbound(REMOTE, PORT_B, 1000, per_class + i), false);
    }
    let total = 2 * per_class as usize;
    assert!(engine.wait_for_sent(total, Duration::from_secs(5)));

    let sent = engine.sent();
    let ids: HashSet<u32> = sent.iter().map(|s| id_of(&s.data)).collect();
    assert_eq!(ids.len(), total, "every packet exactly once");

    // 416 kB at 2 MB/s is ~208 ms; the 40 kB root burst may leave at once.
    let span = sent.last().unwrap().at - sent[0].at;
    let bytes: usize = sent.iter().map(|s| s.data.len()).sum();
    let floor = Duration::from_secs_f64((bytes as f64 - (rate / 50) as f64) / rate as f64 * 0.9);
    assert!(span >= floor, "too fast: {span:?} < {floor:?}");
    assert!(span < Duration::from_secs(2), "too slow: {span:?}");

    // While both classes are backlogged, the split follows the shares.
    let first_half = &sent[..total / 2];
    let a = first_half
        .iter()
        .filter(|s| port_of(&s.data) == PORT_A)
        .count();
    let share = a as f64 / first_half.len() as f64;
    assert!((0.30..=0.50).contains(&share), "rule a got {share:.2}");

    assert!(wait(
        || fp.pool_available() == (1024, 4),
        Duration::from_secs(1)
    ));
    let c = fp.counters();
    assert_eq!(c.queued, total as u64);
    assert_eq!(c.bypassed + c.pool_exhausted + c.channel_full, 0);

    std::thread::sleep(Duration::from_millis(150));
    let snap = fp.snapshot();
    let down = snap.down.expect("download is shaped");
    let sent_bytes: u64 = down.classes.iter().map(|(_, s)| s.sent_bytes).sum();
    assert_eq!(sent_bytes, bytes as u64);
    // Sent bytes per tag (remote port): 200 packets of 1040 bytes each.
    let each = per_class as u64 * 1040;
    assert_eq!(
        down.tags,
        vec![(Tag::from(PORT_A), each), (Tag::from(PORT_B), each)]
    );
    assert!(snap.up.is_none());
    assert_eq!(fp.stop(), None);
}

#[test]
fn a_pure_ack_jumps_the_queue() {
    let engine = Arc::new(MockEngine::new());
    let fp = start(&engine, 1_000_000, |_| {});
    for i in 0..100 {
        engine.inject(&inbound(REMOTE, PORT_B, 1000, i), false);
    }
    // Let the bulk reach the scheduler's queue first.
    assert!(wait(|| fp.counters().queued == 100, Duration::from_secs(2)));
    engine.inject(&inbound(REMOTE, PORT_B, 0, 0), false);
    assert!(engine.wait_for_sent(101, Duration::from_secs(5)));
    let pos = engine
        .sent()
        .iter()
        .position(|s| s.data.len() == 40)
        .unwrap();
    assert!(pos < 60, "the ACK waited behind {pos} packets");
    fp.stop();
}

#[test]
fn lan_and_unshaped_directions_pass_straight_through() {
    let engine = Arc::new(MockEngine::new());
    let fp = start(&engine, 10_000, |_| {});
    // Upload isn't shaped in this run; LAN peers never are by default.
    engine.inject(&inbound(REMOTE, PORT_A, 100, 1), true);
    engine.inject(&inbound(LAN_PEER, PORT_A, 100, 2), false);
    assert!(engine.wait_for_sent(2, Duration::from_secs(2)));
    let c = fp.counters();
    assert_eq!((c.bypassed, c.queued), (2, 0));
    fp.stop();
}

#[test]
fn stop_flushes_everything_still_queued() {
    let engine = Arc::new(MockEngine::new());
    // 10 kB/s: 50 kB would take five seconds to release.
    let fp = start(&engine, 10_000, |_| {});
    for i in 0..50 {
        engine.inject(&inbound(REMOTE, PORT_B, 1000, i), false);
    }
    assert!(wait(|| fp.counters().queued == 50, Duration::from_secs(2)));
    assert!(engine.sent_count() < 50);
    let started = Instant::now();
    assert_eq!(fp.stop(), None);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(engine.sent_count(), 50);
}

#[test]
fn a_stalled_scheduler_fails_open_and_loses_nothing() {
    let engine = Arc::new(MockEngine::new());
    let fp = start(&engine, 1_000_000, |c| {
        c.stall_limit = Duration::from_millis(100);
    });
    engine.stall_sends(true);
    for i in 0..50 {
        engine.inject(&inbound(REMOTE, PORT_B, 1000, i), false);
    }
    let started = Instant::now();
    assert!(wait(|| fp.failure().is_some(), Duration::from_secs(2)));
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(engine.is_shut_down(), "the engine stops diverting");
    assert!(
        !engine.inject(&inbound(REMOTE, PORT_B, 1000, 99), false),
        "new traffic bypasses us"
    );

    // The wedged thread recovers; everything it held still goes out.
    engine.stall_sends(false);
    let reason = fp.stop().expect("the failure is reported");
    assert!(reason.contains("stalled"), "{reason}");
    assert_eq!(engine.sent_count(), 50);
}

#[test]
fn an_empty_pool_passes_packets_through_instead_of_allocating() {
    let engine = Arc::new(MockEngine::new());
    let fp = start(&engine, 10_000, |c| c.small_buffers = 4);
    for i in 0..30 {
        engine.inject(&inbound(REMOTE, PORT_B, 1000, i), false);
    }
    assert!(wait(
        || {
            let c = fp.counters();
            c.queued + c.pool_exhausted == 30
        },
        Duration::from_secs(2)
    ));
    let c = fp.counters();
    assert!(c.pool_exhausted > 0);
    fp.stop();
    assert_eq!(engine.sent_count(), 30);
}

#[test]
fn starting_without_a_direction_is_an_error() {
    let engine = Arc::new(MockEngine::new());
    let r = FastPath::start(
        engine,
        None,
        None,
        ByPort,
        Box::new(SpinNap),
        FastPathConfig::default(),
    );
    assert!(r.is_err());
}

#[test]
fn reconfigure_applies_live_and_keeps_every_queued_packet() {
    let engine = Arc::new(MockEngine::new());
    // 10 kB/s: 50 kB queued would take five seconds.
    let fp = start(&engine, 10_000, |_| {});
    for i in 0..50 {
        engine.inject(&inbound(REMOTE, PORT_B, 1000, i), false);
        engine.inject(&inbound(REMOTE, PORT_A, 1000, 100 + i), false);
    }
    assert!(wait(|| fp.counters().queued == 100, Duration::from_secs(2)));
    assert!(engine.sent_count() < 100);

    // A direction that wasn't shaped can't be added live; a bad config is
    // refused; neither changes anything.
    assert!(matches!(
        fp.reconfigure(Some(config(10_000)), Some(config(10_000))),
        Err(ReconfigureError::DirectionsChanged)
    ));
    let mut bad = config(10_000);
    bad.classes.clear();
    assert!(matches!(
        fp.reconfigure(Some(bad), None),
        Err(ReconfigureError::Config(_))
    ));

    // 100× faster, and rule "a" removed: its queue moves to "other".
    let started = Instant::now();
    fp.reconfigure(
        Some(SchedulerConfig {
            rate_bytes_per_sec: 1_000_000,
            burst_bytes: 20_000,
            classes: vec![ClassConfig::new("other", 10_000)],
        }),
        None,
    )
    .unwrap();
    assert!(engine.wait_for_sent(100, Duration::from_secs(2)));
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "new rate applied"
    );
    let ids: HashSet<u32> = engine.sent().iter().map(|s| id_of(&s.data)).collect();
    assert_eq!(ids.len(), 100, "every packet exactly once");
    std::thread::sleep(Duration::from_millis(150));
    let down = fp.snapshot().down.unwrap();
    assert_eq!(down.rate_bytes_per_sec, 1_000_000);
    assert_eq!(down.classes.len(), 1);
    assert_eq!(fp.stop(), None);
}
