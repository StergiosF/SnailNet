//! Scheduler properties from PLAN.md §9.2 and §19.1, on simulated time.

use super::*;
use crate::clock::NANOS_PER_MILLI;
use proptest::prelude::*;

const MB: u64 = 1_000_000;
const SEC: Nanos = NANOS_PER_SEC;
const MS: Nanos = NANOS_PER_MILLI;

#[derive(Debug)]
struct P {
    len: usize,
    born: Nanos,
    ect: bool,
    /// Which traffic source made it.
    src: usize,
}

impl Packet for P {
    fn wire_len(&self) -> usize {
        self.len
    }
    fn mark_ce(&mut self) -> bool {
        self.ect
    }
    /// The source is the owner.
    fn tag(&self) -> Tag {
        self.src as Tag
    }
}

fn p(len: usize, src: usize) -> P {
    P {
        len,
        born: 0,
        ect: false,
        src,
    }
}

fn class(name: &str, bp: u32, prio: Priority) -> ClassConfig {
    ClassConfig {
        prio,
        ..ClassConfig::new(name, bp)
    }
}

fn config(rate: u64, classes: Vec<ClassConfig>) -> SchedulerConfig {
    SchedulerConfig {
        rate_bytes_per_sec: rate,
        burst_bytes: 2 * 1514,
        classes,
    }
}

/// Classes named c0, c1, … with these shares, Normal priority, the last one
/// being Everything else.
fn plain(rate: u64, bps: &[u32]) -> Scheduler<P> {
    let classes = bps
        .iter()
        .enumerate()
        .map(|(i, &bp)| ClassConfig::new(format!("c{i}"), bp))
        .collect();
    Scheduler::new(config(rate, classes), 0).unwrap()
}

#[derive(Debug, Clone, Copy)]
enum Mode {
    /// Keep this many bytes of the source's packets waiting (a saturating
    /// sender).
    Backlog(u64),
    /// One packet every this many ns (a game tick, a light flow).
    Every(Nanos),
}

#[derive(Debug, Clone, Copy)]
struct Src {
    meta: PacketMeta,
    size: usize,
    mode: Mode,
}

fn saturating(class: ClassId, size: usize) -> Src {
    Src {
        meta: PacketMeta::new(class, class as u64),
        size,
        mode: Mode::Backlog(32 * 1024),
    }
}

/// Drives a scheduler with traffic sources on simulated time. Every packet
/// is ECN-capable, so CoDel marks instead of dropping and nothing is lost:
/// each source's waiting bytes are exactly enqueued − sent.
struct Sim {
    s: Scheduler<P>,
    now: Nanos,
    srcs: Vec<Src>,
    enq: Vec<u64>,
    sent: Vec<u64>,
    next_at: Vec<Nanos>,
    delays: Vec<Vec<Nanos>>,
}

impl Sim {
    fn new(s: Scheduler<P>, srcs: Vec<Src>) -> Self {
        let n = srcs.len();
        Self {
            s,
            now: 0,
            srcs,
            enq: vec![0; n],
            sent: vec![0; n],
            next_at: vec![0; n],
            delays: vec![Vec::new(); n],
        }
    }

    fn push(&mut self, i: usize) {
        let src = self.srcs[i];
        let packet = P {
            len: src.size,
            born: self.now,
            ect: true,
            src: i,
        };
        self.s.enqueue(self.now, src.meta, packet);
        self.enq[i] += src.size as u64;
    }

    /// Run for `dur`; returns bytes sent per source in that time.
    fn run(&mut self, dur: Nanos) -> Vec<u64> {
        let end = self.now + dur;
        let mut window = vec![0u64; self.srcs.len()];
        while self.now < end {
            for i in 0..self.srcs.len() {
                match self.srcs[i].mode {
                    Mode::Backlog(b) => {
                        while self.enq[i] - self.sent[i] < b {
                            self.push(i);
                        }
                    }
                    Mode::Every(period) => {
                        while self.next_at[i] <= self.now {
                            self.push(i);
                            self.next_at[i] += period;
                        }
                    }
                }
            }
            let next_arrival = self
                .srcs
                .iter()
                .zip(&self.next_at)
                .filter(|(s, _)| matches!(s.mode, Mode::Every(_)))
                .map(|(_, &t)| t)
                .min()
                .unwrap_or(Nanos::MAX);
            match self.s.dequeue(self.now) {
                Dequeue::Packet { packet, .. } => {
                    let len = packet.len as u64;
                    self.sent[packet.src] += len;
                    window[packet.src] += len;
                    self.delays[packet.src].push(self.now - packet.born);
                }
                Dequeue::WaitUntil(t) => {
                    assert!(t > self.now, "WaitUntil must be in the future");
                    self.now = t.min(next_arrival);
                }
                Dequeue::Empty => self.now = next_arrival.min(self.now + MS),
            }
        }
        window
    }
}

fn shares_of(sent: &[u64]) -> Vec<f64> {
    let total: u64 = sent.iter().sum();
    sent.iter().map(|&b| b as f64 / total as f64).collect()
}

fn assert_shares(sent: &[u64], expected: &[f64], tol: f64) {
    let got = shares_of(sent);
    for (i, (&g, &e)) in got.iter().zip(expected).enumerate() {
        assert!(
            (g - e).abs() <= tol,
            "source {i}: got {g:.4}, expected {e:.4} (all: {got:.4?})"
        );
    }
}

fn percentile(v: &[Nanos], p: f64) -> Nanos {
    let mut v = v.to_vec();
    v.sort_unstable();
    v[((v.len() - 1) as f64 * p) as usize]
}

#[test]
fn saturated_classes_get_their_shares() {
    let s = plain(10 * MB, &[4000, 4500, 1500]);
    let srcs = vec![
        saturating(0, 1500),
        saturating(1, 1500),
        saturating(2, 1500),
    ];
    let mut sim = Sim::new(s, srcs);
    sim.run(SEC / 2);
    let sent = sim.run(2 * SEC);
    assert_shares(&sent, &[0.40, 0.45, 0.15], 0.01);
    let total: u64 = sent.iter().sum();
    assert!(total.abs_diff(20 * MB) <= 20 * MB / 100, "total {total}");
}

#[test]
fn lone_class_gets_the_whole_line() {
    let s = plain(10 * MB, &[4000, 6000]);
    let mut sim = Sim::new(s, vec![saturating(0, 1500)]);
    sim.run(SEC / 2);
    let sent = sim.run(2 * SEC);
    assert!(sent[0].abs_diff(20 * MB) <= 20 * MB / 100, "sent {sent:?}");
}

#[test]
fn idle_share_is_redistributed_by_weight() {
    // Steam 40, YouTube 45 (idle), Everything else 15 → Steam:Other = 40:15.
    let s = plain(10 * MB, &[4000, 4500, 1500]);
    let mut sim = Sim::new(s, vec![saturating(0, 1500), saturating(2, 1500)]);
    sim.run(SEC / 2);
    let sent = sim.run(2 * SEC);
    assert_shares(&sent, &[40.0 / 55.0, 15.0 / 55.0], 0.01);
}

#[test]
fn fairness_is_in_bytes_not_packets() {
    let s = plain(10 * MB, &[5000, 5000]);
    let mut sim = Sim::new(s, vec![saturating(0, 1500), saturating(1, 100)]);
    sim.run(SEC / 2);
    let sent = sim.run(2 * SEC);
    assert_shares(&sent, &[0.5, 0.5], 0.01);
}

#[test]
fn zero_share_only_when_free() {
    let s = plain(10 * MB, &[0, 9500, 500]);
    let mut sim = Sim::new(s, vec![saturating(0, 1500), saturating(1, 1500)]);
    sim.run(SEC / 2);
    let sent = sim.run(2 * SEC);
    assert!(shares_of(&sent)[0] < 0.001, "0% class got {sent:?}");

    let s = plain(10 * MB, &[0, 9500, 500]);
    let mut sim = Sim::new(s, vec![saturating(0, 1500)]);
    sim.run(SEC / 2);
    let sent = sim.run(2 * SEC);
    assert!(
        sent[0] >= 20 * MB * 99 / 100,
        "alone it gets the line: {sent:?}"
    );
}

fn capped_config() -> SchedulerConfig {
    let mut capped = ClassConfig::new("capped", 5000);
    capped.ceil_bytes_per_sec = Some(MB);
    config(10 * MB, vec![capped, ClassConfig::new("other", 5000)])
}

#[test]
fn ceiling_holds_and_the_rest_goes_elsewhere() {
    let s = Scheduler::new(capped_config(), 0).unwrap();
    let mut sim = Sim::new(s, vec![saturating(0, 1500), saturating(1, 1500)]);
    let sent = sim.run(2 * SEC);
    // One ceiling burst (10 ms at 1 MB/s) plus one packet of slack.
    let burst = 10_000 + 1514;
    assert!(sent[0] <= 2 * MB + burst, "capped sent {}", sent[0]);
    assert!(sent[0] >= 2 * MB * 98 / 100, "capped sent {}", sent[0]);
    assert!(sent[1] >= 18 * MB * 98 / 100, "other sent {}", sent[1]);
}

#[test]
fn ceiling_alone_waits_instead_of_spinning() {
    let s = Scheduler::new(capped_config(), 0).unwrap();
    let mut sim = Sim::new(s, vec![saturating(0, 1500)]);
    sim.run(SEC / 2);
    let sent = sim.run(2 * SEC);
    assert!(sent[0].abs_diff(2 * MB) <= 2 * MB / 50, "sent {}", sent[0]);
}

/// Five saturated classes and one light game class sending a 600 B packet
/// every 5 ms. Returns the game packets' queueing delays.
fn game_delays(game_prio: Priority) -> Vec<Nanos> {
    let mut classes: Vec<ClassConfig> = (0..4)
        .map(|i| ClassConfig::new(format!("bulk{i}"), 1700))
        .collect();
    classes.push(class("game", 1000, game_prio));
    classes.push(ClassConfig::new("other", 2200));
    let s = Scheduler::new(config(10 * MB, classes), 0).unwrap();
    let mut srcs: Vec<Src> = [0, 1, 2, 3, 5].map(|c| saturating(c, 1500)).to_vec();
    srcs.push(Src {
        meta: PacketMeta::new(4, 99),
        size: 600, // too big for the express lane: this tests the queues
        mode: Mode::Every(5 * MS),
    });
    let mut sim = Sim::new(s, srcs);
    sim.run(2 * SEC);
    sim.delays.pop().unwrap()
}

#[test]
fn high_priority_light_flow_skips_the_line() {
    let high = game_delays(Priority::High);
    let normal = game_delays(Priority::Normal);
    assert!(high.len() >= 390, "game packets lost: {}", high.len());
    // One full packet at 10 MB/s is ~151 µs: the game waits for at most the
    // packet already being sent, plus the root bucket's refill.
    let packet_time = 1514 * SEC / (10 * MB);
    let worst = *high.iter().max().unwrap();
    assert!(worst <= 2 * packet_time, "High worst delay {worst} ns");
    assert!(percentile(&high, 0.99) <= 2 * MS);
    // Priority is what buys this: at Normal it queues behind the others.
    let normal_worst = *normal.iter().max().unwrap();
    assert!(
        normal_worst > worst,
        "Normal worst {normal_worst} vs High {worst}"
    );
}

#[test]
fn priority_does_not_buy_bandwidth() {
    let classes = vec![
        class("high", 5000, Priority::High),
        class("low", 4500, Priority::Low),
        ClassConfig::new("other", 500),
    ];
    let s = Scheduler::new(config(10 * MB, classes), 0).unwrap();
    let mut sim = Sim::new(s, vec![saturating(0, 1500), saturating(1, 1500)]);
    sim.run(SEC / 2);
    let sent = sim.run(2 * SEC);
    assert_shares(&sent, &[50.0 / 95.0, 45.0 / 95.0], 0.01);
}

#[test]
fn control_packets_jump_the_queue() {
    let mut s = plain(MB, &[5000, 5000]);
    for _ in 0..50 {
        s.enqueue(0, PacketMeta::new(0, 1), p(1500, 0));
    }
    // Use up the root burst.
    while let Dequeue::Packet { .. } = s.dequeue(0) {}
    let ack = PacketMeta {
        control: true,
        ..PacketMeta::new(0, 1)
    };
    s.enqueue(0, ack, p(52, 7));
    let Dequeue::WaitUntil(t) = s.dequeue(0) else {
        panic!("root should be empty")
    };
    match s.dequeue(t) {
        Dequeue::Packet { packet, .. } => assert_eq!(packet.src, 7),
        other => panic!("expected the ACK, got {other:?}"),
    }
    assert_eq!(s.stats(0).express_packets, 1);
}

#[test]
fn express_lane_cannot_dodge_a_share() {
    // A High class flooding 200 B packets (all express-sized) against a
    // saturated Normal class: it still gets only its share.
    let classes = vec![
        class("game", 2000, Priority::High),
        ClassConfig::new("bulk", 7500),
        ClassConfig::new("other", 500),
    ];
    let s = Scheduler::new(config(10 * MB, classes), 0).unwrap();
    let mut sim = Sim::new(s, vec![saturating(0, 200), saturating(1, 1500)]);
    sim.run(SEC / 2);
    let sent = sim.run(2 * SEC);
    assert_shares(&sent, &[20.0 / 95.0, 75.0 / 95.0], 0.01);
    assert!(sim.s.stats(0).express_packets > 0);
}

#[test]
fn flows_inside_a_class_are_weighted() {
    let s = plain(10 * MB, &[5000, 5000]);
    let flow = |id: u64, prio| Src {
        meta: PacketMeta {
            prio,
            ..PacketMeta::new(0, id)
        },
        size: 1500,
        mode: Mode::Backlog(32 * 1024),
    };
    let mut sim = Sim::new(s, vec![flow(1, Priority::High), flow(2, Priority::Low)]);
    sim.run(SEC / 2);
    let sent = sim.run(2 * SEC);
    assert_shares(&sent, &[0.8, 0.2], 0.01);
}

#[test]
fn codel_marks_a_standing_queue() {
    let s = plain(MB, &[5000, 5000]);
    let mut src = saturating(0, 1500);
    src.mode = Mode::Backlog(60 * 1024); // ~60 ms at the full 1 MB/s
    let mut sim = Sim::new(s, vec![src]);
    sim.run(2 * SEC);
    let st = sim.s.stats(0);
    assert!(st.ecn_marked > 0, "{st:?}");
    assert_eq!(st.dropped_packets, 0);
}

#[test]
fn codel_drops_non_ecn_and_queue_stays_bounded() {
    let mut s = plain(MB, &[5000, 5000]);
    let mut now = 0;
    while now < 2 * SEC {
        // 3× the line rate, never backing off.
        for _ in 0..2 {
            s.enqueue(now, PacketMeta::new(0, 1), p(1500, 0));
        }
        while let Dequeue::Packet { .. } = s.dequeue(now) {}
        now += MS;
    }
    let st = s.stats(0);
    assert!(st.dropped_packets > 0, "{st:?}");
    assert!(st.queued_bytes <= 150_000 + 1500, "{st:?}");
    s.check_invariants(1500);
}

#[test]
fn hard_limit_drops_from_the_bulk_flow() {
    let mut s = plain(100_000, &[5000, 5000]);
    s.enqueue(0, PacketMeta::new(0, 1), p(100, 42));
    for _ in 0..100 {
        s.enqueue(0, PacketMeta::new(0, 2), p(1500, 0));
    }
    assert!(s.stats(0).dropped_packets > 0);
    assert!(s.stats(0).queued_bytes <= QUEUE_MIN_BYTES);
    let left = s.drain_all();
    assert!(left.iter().any(|(_, p)| p.src == 42), "light flow lost");
}

#[test]
fn oversized_packets_pass() {
    // A coalesced 64 KB receive is bigger than burst and quantum.
    let s = plain(10 * MB, &[5000, 5000]);
    let mut sim = Sim::new(s, vec![saturating(0, 65_000), saturating(1, 1500)]);
    let sent = sim.run(SEC);
    assert!(sent[0] > 0 && sent[1] > 0, "sent {sent:?}");
}

#[test]
fn empty_and_wait_states() {
    let mut s = plain(1_000, &[10_000]);
    assert!(matches!(s.dequeue(0), Dequeue::Empty));
    s.enqueue(0, PacketMeta::new(0, 0), p(5_000, 0));
    s.enqueue(0, PacketMeta::new(0, 0), p(1_000, 0));
    assert!(matches!(s.dequeue(0), Dequeue::Packet { .. }));
    // 5000 B against a 3028 B burst at 1000 B/s → wait ~1.97 s.
    match s.dequeue(0) {
        Dequeue::WaitUntil(t) => assert_eq!(t, 1_972_000_000),
        other => panic!("expected wait, got {other:?}"),
    }
}

#[test]
fn unknown_class_goes_to_everything_else() {
    let mut s = plain(MB, &[5000, 5000]);
    s.enqueue(0, PacketMeta::new(17, 0), p(1500, 0));
    assert_eq!(s.stats(1).queued_bytes, 1500);
}

#[test]
fn live_change_converges_and_loses_nothing() {
    let shares = |a, b| {
        config(
            10 * MB,
            vec![
                ClassConfig::new("a", a),
                ClassConfig::new("b", b),
                ClassConfig::new("other", 500),
            ],
        )
    };
    let live = LiveConfig::new(shares(4000, 5500)).unwrap();
    let s = Scheduler::from_live(&live, 0);
    let mut sim = Sim::new(s, vec![saturating(0, 1500), saturating(1, 1500)]);
    sim.run(SEC);

    live.publish(shares(7000, 2500)).unwrap();
    sim.s.sync(&live, sim.now);
    // Converged within 500 ms.
    sim.run(SEC / 2);
    let sent = sim.run(SEC);
    assert_shares(&sent, &[70.0 / 95.0, 25.0 / 95.0], 0.01);

    // Every byte ever queued was sent or is still waiting.
    let waiting: u64 = sim.s.drain_all().iter().map(|(_, p)| p.len as u64).sum();
    let enq: u64 = sim.enq.iter().sum();
    let sent: u64 = sim.sent.iter().sum();
    assert_eq!(enq, sent + waiting);
    assert_eq!(
        sim.s.stats(0).dropped_packets + sim.s.stats(1).dropped_packets,
        0
    );
}

#[test]
fn live_rate_change_takes_effect() {
    let one = |rate| config(rate, vec![ClassConfig::new("o", 10_000)]);
    let live = LiveConfig::new(one(10 * MB)).unwrap();
    let mut sim = Sim::new(Scheduler::from_live(&live, 0), vec![saturating(0, 1500)]);
    sim.run(SEC / 2);
    live.publish(one(5 * MB)).unwrap();
    sim.s.sync(&live, sim.now);
    let sent = sim.run(2 * SEC);
    assert!(
        sent[0].abs_diff(10 * MB) <= 10 * MB / 100,
        "sent {}",
        sent[0]
    );
}

#[test]
fn removed_rule_hands_its_packets_to_everything_else() {
    let mut s = plain(MB, &[3000, 3000, 4000]);
    for _ in 0..10 {
        s.enqueue(0, PacketMeta::new(1, 5), p(1500, 1));
    }
    let ack = PacketMeta {
        control: true,
        ..PacketMeta::new(1, 5)
    };
    s.enqueue(0, ack, p(52, 1));
    let cfg = config(
        MB,
        vec![ClassConfig::new("c0", 3000), ClassConfig::new("c2", 7000)],
    );
    s.reconfigure(Arc::new(cfg), 0).unwrap();
    s.check_invariants(1500);
    assert_eq!(s.class_name(1), "c2");
    assert_eq!(s.stats(1).queued_bytes, 15_000);
    let all = s.drain_all();
    assert_eq!(all.len(), 11);
    assert!(all.iter().all(|&(c, _)| c == 1));
}

#[test]
fn drain_all_returns_everything() {
    let mut s = plain(1, &[5000, 5000]);
    s.enqueue(0, PacketMeta::new(0, 0), p(10, 0));
    s.enqueue(0, PacketMeta::new(1, 0), p(20, 0));
    s.enqueue(0, PacketMeta::new(1, 3), p(30, 0));
    assert_eq!(s.drain_all().len(), 3);
    assert!(s.is_empty());
    assert!(matches!(s.dequeue(0), Dequeue::Empty));
}

#[derive(Debug, Clone)]
enum Op {
    Enqueue {
        class: usize,
        flow: u64,
        len: usize,
        prio: Priority,
        control: bool,
        ect: bool,
    },
    Advance(Nanos),
    Dequeue,
    /// Switch to one of a few configs over the same class names.
    Reconfigure(usize),
}

fn op() -> impl Strategy<Value = Op> {
    let prio = prop_oneof![
        Just(Priority::Low),
        Just(Priority::Normal),
        Just(Priority::High)
    ];
    let len = prop_oneof![8 => 40usize..1515, 1 => Just(65_000usize)];
    prop_oneof![
        4 => (0usize..5, 0u64..8, len, prio, any::<bool>(), any::<bool>()).prop_map(
            |(class, flow, len, prio, control, ect)| Op::Enqueue { class, flow, len, prio, control, ect }
        ),
        2 => (0..20 * MS).prop_map(Op::Advance),
        4 => Just(Op::Dequeue),
        1 => (0usize..3).prop_map(Op::Reconfigure),
    ]
}

fn variant(i: usize) -> SchedulerConfig {
    let prio = [Priority::High, Priority::Low, Priority::Normal][i];
    let mut a = class("a", [4000, 0, 9000][i], prio);
    a.ceil_bytes_per_sec = [None, Some(200_000), None][i];
    let b = class("b", [4500, 9000, 500][i], Priority::Normal);
    let o = ClassConfig::new("other", [1500, 1000, 500][i]);
    config([MB, 2 * MB, 300_000][i], vec![a, b, o])
}

proptest! {
    #[test]
    fn random_operations_keep_invariants(ops in prop::collection::vec(op(), 1..400)) {
        let mut s: Scheduler<P> = Scheduler::new(variant(0), 0).unwrap();
        let mut now = 0;
        let (mut enq, mut out) = (0u64, 0u64);
        for op in ops {
            match op {
                Op::Enqueue { class, flow, len, prio, control, ect } => {
                    let meta = PacketMeta { class, flow, prio, control };
                    s.enqueue(now, meta, P { len, born: now, ect, src: flow as usize % 3 });
                    enq += 1;
                }
                Op::Advance(dt) => now += dt,
                Op::Dequeue => match s.dequeue(now) {
                    Dequeue::Packet { class, .. } => {
                        prop_assert!(class < s.class_count());
                        out += 1;
                    }
                    Dequeue::WaitUntil(t) => prop_assert!(t > now),
                    Dequeue::Empty => prop_assert!(s.is_empty()),
                },
                Op::Reconfigure(i) => s.reconfigure(Arc::new(variant(i)), now).unwrap(),
            }
            s.check_invariants(65_000);
        }
        let dropped: u64 = (0..s.class_count()).map(|c| s.stats(c).dropped_packets).sum();
        let left = s.drain_all().len() as u64;
        prop_assert_eq!(enq, out + dropped + left, "packets leaked or duplicated");
    }
}

fn pressure_of(s: &Scheduler<P>, tag: Tag) -> Pressure {
    let mut out = Vec::new();
    s.pressure(&mut out);
    out.iter()
        .find(|(t, _)| *t == tag)
        .map_or(Pressure::default(), |&(_, p)| p)
}

#[test]
fn codel_actions_land_on_the_flows_owner() {
    // Two bulk owners in different classes, and a light one sharing a
    // class with the first: CoDel acts on the bulk flows only, and every
    // action is counted on the owner it hit.
    let s = plain(10 * MB, &[5000, 5000]);
    let bulk = |class: ClassId, flow: u64| Src {
        meta: PacketMeta::new(class, flow),
        size: 1500,
        mode: Mode::Backlog(256 * 1024),
    };
    let light = Src {
        meta: PacketMeta::new(1, 2),
        size: 200,
        mode: Mode::Every(20 * MS),
    };
    let mut sim = Sim::new(s, vec![bulk(1, 1), light, bulk(0, 3)]);
    sim.run(2 * SEC);
    let s = &sim.s;
    let acted = |c: ClassId| s.stats(c).ecn_marked + s.stats(c).dropped_packets;
    assert!(
        acted(0) > 10 && acted(1) > 10,
        "CoDel acted: {} {}",
        acted(0),
        acted(1)
    );
    assert_eq!(pressure_of(s, 0).acted, acted(1));
    assert_eq!(pressure_of(s, 1).acted, 0, "the light flow was left alone");
    assert_eq!(pressure_of(s, 2).acted, acted(0));
    assert!(pressure_of(s, 0).queued_bytes > 200 * 1024);
    s.check_invariants(1500);
}

#[test]
fn hard_limit_drops_count_on_the_fattest_owner() {
    // At 1 MB/s the queue limit is 150 KB; 200 packets overflow it.
    let mut s = plain(MB, &[10000]);
    for i in 0..200 {
        s.enqueue(0, PacketMeta::new(0, 1), p(1500, 7));
        if i % 10 == 0 {
            s.enqueue(0, PacketMeta::new(0, 2), p(100, 8));
        }
    }
    let dropped = s.stats(0).dropped_packets;
    assert!(dropped > 0);
    assert_eq!(pressure_of(&s, 7).acted, dropped);
    assert_eq!(pressure_of(&s, 8).acted, 0);
    s.check_invariants(1500);
}

#[test]
fn pressure_survives_a_removed_rule() {
    // The rule's class goes away; what CoDel did to its owner stays counted
    // (now in Everything else), and its queued bytes move along.
    let two = || {
        config(
            MB,
            vec![
                class("a", 5000, Priority::Normal),
                ClassConfig::new("o", 5000),
            ],
        )
    };
    let mut s: Scheduler<P> = Scheduler::new(two(), 0).unwrap();
    for _ in 0..200 {
        s.enqueue(0, PacketMeta::new(0, 1), p(1500, 4));
    }
    let before = pressure_of(&s, 4);
    assert!(before.acted > 0 && before.queued_bytes > 0, "{before:?}");
    let only_other = config(MB, vec![ClassConfig::new("o", 10000)]);
    s.reconfigure(Arc::new(only_other), 0).unwrap();
    assert_eq!(pressure_of(&s, 4), before);
    s.check_invariants(1500);
}

#[test]
fn high_priority_small_packets_cannot_bypass_ceiling_via_express() {
    // High-priority class capped at 200 KB/s alongside a saturating bulk class
    // on a 2 MB/s line: sending 200-byte packets must stay within the ceiling.
    let mut capped = class("capped", 5000, Priority::High);
    capped.ceil_bytes_per_sec = Some(200_000);
    let cfg = config(2 * MB, vec![capped, ClassConfig::new("other", 5000)]);
    let s = Scheduler::new(cfg, 0).unwrap();
    let mut sim = Sim::new(
        s,
        vec![
            Src {
                meta: PacketMeta {
                    prio: Priority::High,
                    ..PacketMeta::new(0, 1)
                },
                size: 200,
                mode: Mode::Backlog(16 * 1024),
            },
            saturating(1, 1500),
        ],
    );
    sim.run(2 * SEC);
    let rate = sim.sent[0] as f64 / 2.0;
    assert!(
        rate <= 215_000.0,
        "ceiling bypassed via express lane: {rate:.0} B/s (limit 200000)"
    );
    sim.s.check_invariants(1500);
}

#[test]
fn single_batch_of_urgent_packets_respects_ceiling_burst() {
    let mut capped = class("capped", 5000, Priority::High);
    capped.ceil_bytes_per_sec = Some(200_000);
    let cfg = config(2 * MB, vec![capped, ClassConfig::new("other", 5000)]);
    let mut s = Scheduler::new(cfg, 0).unwrap();
    let meta = PacketMeta {
        prio: Priority::High,
        ..PacketMeta::new(0, 1)
    };
    // Enqueue 100 small urgent packets (20 KB) at the exact same timestamp.
    for _ in 0..100 {
        s.enqueue(0, meta, p(200, 0));
    }
    // Ceiling burst is 2*MTU (~3028 B), so only ~16 packets may take the express
    // lane; the rest must sit in the class queue until the ceiling refills.
    let mut express_sent = 0usize;
    let mut t = 0;
    while let Dequeue::Packet { .. } = s.dequeue(t) {
        express_sent += 1;
        t += 1;
    }
    assert!(
        express_sent <= 20,
        "single batch flooded express lane past ceiling burst: {express_sent} packets"
    );
    assert!(
        s.backlog_bytes() > 0,
        "excess packets should queue normally"
    );
    s.check_invariants(1500);
}
