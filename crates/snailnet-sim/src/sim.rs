//! The event loop: senders, the ISP link, and the scheduler, on simulated
//! time.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap, VecDeque};
use std::rc::Rc;

use snailnet_core::clock::{Clock, NANOS_PER_SEC, Nanos, SimClock};
use snailnet_core::scheduler::{
    ClassConfig, ConfigError, Dequeue, LiveConfig, Packet, PacketMeta, Scheduler, SchedulerConfig,
};

use crate::ms;
use crate::report::{FlowReport, Report};
use crate::rng::{Rng, mix};
use crate::traffic::{Cc, FlowSpec, MSS, Model};

/// Smallest packet a sender puts on the wire.
const MIN_PACKET: usize = 64;

/// The path around the scheduler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimConfig {
    /// The line's real capacity (the ISP's rate), bytes per second. The
    /// scheduler's own rate, in its config, is usually a bit below this.
    pub line_rate: u64,
    /// The ISP's buffer, as time at line rate. Home lines often hold
    /// 100–250 ms; it fills whenever traffic arrives faster than the line.
    pub isp_buffer: Nanos,
    /// The scheduler thread's timer: wake-ups are rounded up to a multiple
    /// of this. 0 means exact.
    pub timer_granularity: Nanos,
    /// Width of the throughput bins in the [`Report`].
    pub bin: Nanos,
    /// Seeds the random traffic (web transfers).
    pub seed: u64,
}

impl SimConfig {
    /// A line of this rate with a 150 ms ISP buffer, an exact timer, 10 ms
    /// bins.
    pub fn new(line_rate: u64) -> Self {
        Self {
            line_rate,
            isp_buffer: ms(150),
            timer_granularity: 0,
            bin: ms(10),
            seed: 1,
        }
    }
}

/// A scheduler config shaped at `rate`, with the root burst `spike` uses
/// (2 ms of line).
pub fn shaped_config(rate: u64, classes: Vec<ClassConfig>) -> SchedulerConfig {
    SchedulerConfig {
        rate_bytes_per_sec: rate,
        burst_bytes: (rate / 500).max(2 * 1514),
        classes,
    }
}

/// Lost packets, `(flow, wire length)`, as reported by their `Drop`.
type LossLog = Rc<RefCell<Vec<(usize, usize)>>>;

struct SimPacket {
    flow: usize,
    len: usize,
    sent_at: Nanos,
    /// When it reached the scheduler.
    shaper_at: Nanos,
    ect: bool,
    ce: bool,
    delivered: bool,
    losses: LossLog,
}

impl Packet for SimPacket {
    fn wire_len(&self) -> usize {
        self.len
    }

    fn mark_ce(&mut self) -> bool {
        self.ce |= self.ect;
        self.ect
    }
}

/// Wherever a packet is dropped (ISP buffer, CoDel, the scheduler's queue
/// limit), this tells its sender, the way a gap in sequence numbers would.
impl Drop for SimPacket {
    fn drop(&mut self) {
        if !self.delivered {
            self.losses.borrow_mut().push((self.flow, self.len));
        }
    }
}

enum Event {
    /// The flow may send (start, CBR tick, pacing, end of think time).
    Send(usize),
    /// A packet reaches the ISP link.
    Arrive(SimPacket),
    /// The ISP link finished putting its head packet on the wire.
    LinkDone,
    /// The scheduler's timer fired.
    Shaper,
    Ack {
        flow: usize,
        sent_at: Nanos,
        ce: bool,
    },
    Loss {
        flow: usize,
        len: usize,
    },
    Publish(SchedulerConfig),
    Stop(usize),
}

struct Timed {
    at: Nanos,
    seq: u64,
    ev: Event,
}

impl PartialEq for Timed {
    fn eq(&self, other: &Self) -> bool {
        (self.at, self.seq) == (other.at, other.seq)
    }
}

impl Eq for Timed {}

impl PartialOrd for Timed {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Reversed, so the `BinaryHeap` pops the earliest event first, and events
/// at the same time in the order they were scheduled.
impl Ord for Timed {
    fn cmp(&self, other: &Self) -> Ordering {
        (other.at, other.seq).cmp(&(self.at, self.seq))
    }
}

struct Flow {
    spec: FlowSpec,
    cc: Cc,
    /// Bytes left to send in the current transfer; `None` sends forever.
    remaining: Option<u64>,
    /// Connection number: a new one per web transfer, so a new fair-queue
    /// flow too.
    conn: u64,
    rng: Rng,
    active: bool,
    /// The current transfer is done (and, for web, the next is pending).
    idle: bool,
    /// When the pending `Send` event is due. Older ones are stale.
    wake: Option<Nanos>,
    next_pace: Nanos,
    r: FlowReport,
}

pub struct Simulation {
    cfg: SimConfig,
    clock: SimClock,
    live: LiveConfig,
    sched: Scheduler<SimPacket>,
    flows: Vec<Flow>,
    events: BinaryHeap<Timed>,
    seq: u64,
    link: VecDeque<SimPacket>,
    link_bytes: u64,
    link_busy: bool,
    isp_drops: u64,
    /// When the pending `Shaper` event is due. Older ones are stale.
    shaper_wake: Option<Nanos>,
    losses: LossLog,
    classes: BTreeMap<String, Vec<u64>>,
}

impl Simulation {
    pub fn new(cfg: SimConfig, sched: SchedulerConfig) -> Result<Self, ConfigError> {
        assert!(
            cfg.line_rate > 0 && cfg.bin > 0,
            "line rate and bin must be > 0"
        );
        let live = LiveConfig::new(sched)?;
        let clock = SimClock::new();
        let sched = Scheduler::from_live(&live, clock.now());
        Ok(Self {
            cfg,
            clock,
            live,
            sched,
            flows: Vec::new(),
            events: BinaryHeap::new(),
            seq: 0,
            link: VecDeque::new(),
            link_bytes: 0,
            link_busy: false,
            isp_drops: 0,
            shaper_wake: None,
            losses: Rc::default(),
            classes: BTreeMap::new(),
        })
    }

    /// Add a traffic source. Names must be unique.
    pub fn add(&mut self, spec: FlowSpec) -> &mut Self {
        assert!(
            self.flows.iter().all(|f| f.spec.name != spec.name),
            "flow name {:?} used twice",
            spec.name
        );
        let i = self.flows.len();
        let remaining = match spec.model {
            Model::Tcp { bytes, .. } | Model::Quic { bytes } => bytes,
            Model::Web { .. } => Some(0),
            Model::Cbr { .. } => None,
        };
        self.push(spec.start, Event::Send(i));
        if let Some(stop) = spec.stop {
            self.push(stop, Event::Stop(i));
        }
        self.flows.push(Flow {
            cc: Cc::new(spec.rtt, spec.beta()),
            remaining,
            conn: 0,
            rng: Rng::new(mix(self.cfg.seed ^ mix(i as u64))),
            active: true,
            idle: matches!(spec.model, Model::Web { .. }),
            wake: Some(spec.start),
            next_pace: 0,
            r: FlowReport {
                name: spec.name.clone(),
                class: spec.class.clone(),
                ..FlowReport::default()
            },
            spec,
        });
        self
    }

    /// Change the rules at `at`, through [`LiveConfig`] as the service will.
    pub fn publish_at(&mut self, at: Nanos, config: SchedulerConfig) -> Result<(), ConfigError> {
        config.validate()?;
        self.push(at, Event::Publish(config));
        Ok(())
    }

    /// Run until `until` and report.
    pub fn run(mut self, until: Nanos) -> Report {
        while let Some(top) = self.events.peek() {
            if top.at >= until {
                break;
            }
            let Timed { at, ev, .. } = self.events.pop().expect("peeked");
            self.clock.set(at);
            self.handle(ev);
            self.collect_losses();
        }
        let shaper: Vec<_> = (0..self.sched.class_count())
            .map(|c| (self.sched.class_name(c).to_owned(), self.sched.stats(c)))
            .collect();
        self.count_in_network();
        Report {
            bin: self.cfg.bin,
            end: until,
            flows: std::mem::take(&mut self.flows)
                .into_iter()
                .map(|f| f.r)
                .collect(),
            classes: std::mem::take(&mut self.classes),
            shaper,
            isp_drops: self.isp_drops,
        }
    }

    /// Take every packet still on its way out of the path, the ISP link
    /// and the scheduler, count it, and check that no packet was leaked or
    /// duplicated anywhere.
    fn count_in_network(&mut self) {
        let mut left: Vec<SimPacket> = self.sched.drain_all().into_iter().map(|(_, p)| p).collect();
        left.extend(self.link.drain(..));
        left.extend(
            std::mem::take(&mut self.events)
                .into_iter()
                .filter_map(|t| match t.ev {
                    Event::Arrive(p) => Some(p),
                    _ => None,
                }),
        );
        for mut p in left {
            p.delivered = true; // not lost, just cut off by the end of the run
            self.flows[p.flow].r.in_network += 1;
        }
        for f in &self.flows {
            let r = &f.r;
            assert_eq!(
                r.sent_packets,
                r.delivered_packets + r.lost_packets + r.in_network,
                "flow {:?}: packets leaked or duplicated",
                r.name
            );
        }
    }

    fn now(&self) -> Nanos {
        self.clock.now()
    }

    fn push(&mut self, at: Nanos, ev: Event) {
        self.seq += 1;
        self.events.push(Timed {
            at,
            seq: self.seq,
            ev,
        });
    }

    fn handle(&mut self, ev: Event) {
        let now = self.now();
        match ev {
            Event::Send(i) => {
                let f = &mut self.flows[i];
                if f.wake != Some(now) || !f.active {
                    return;
                }
                f.wake = None;
                if f.idle {
                    let Model::Web { mean_bytes, .. } = f.spec.model else {
                        return;
                    };
                    let size = f.rng.exp(mean_bytes as f64).max(MSS as f64) as u64;
                    f.remaining = Some(size);
                    f.cc.reset();
                    f.conn += 1;
                    f.idle = false;
                }
                self.send(i);
            }
            Event::Arrive(p) => self.arrive(p),
            Event::LinkDone => self.link_done(),
            Event::Shaper => {
                if self.shaper_wake == Some(now) {
                    self.shaper_wake = None;
                    self.pump();
                }
            }
            Event::Ack { flow, sent_at, ce } => {
                let f = &mut self.flows[flow];
                let congested = ce && f.spec.ecn_capable();
                f.cc.on_ack(now - sent_at, congested, now);
                self.progress(flow);
            }
            Event::Loss { flow, len } => {
                let f = &mut self.flows[flow];
                f.cc.on_loss(now);
                if let Some(r) = &mut f.remaining {
                    // Resend what was lost.
                    *r += len as u64;
                }
                self.progress(flow);
            }
            Event::Publish(config) => {
                self.live.publish(config).expect("validated by publish_at");
                self.sched.sync(&self.live, now);
                self.pump();
            }
            Event::Stop(i) => {
                self.flows[i].active = false;
                self.flows[i].wake = None;
            }
        }
    }

    /// Schedule flow `i` to send at `at`, unless it already will sooner.
    fn wake_at(&mut self, i: usize, at: Nanos) {
        let f = &mut self.flows[i];
        if f.active && f.wake.is_none_or(|w| at < w) {
            f.wake = Some(at);
            self.push(at, Event::Send(i));
        }
    }

    /// Send whatever flow `i` may send now.
    fn send(&mut self, i: usize) {
        let now = self.now();
        if !self.flows[i].active {
            return;
        }
        if let Model::Cbr { size, period } = self.flows[i].spec.model {
            self.emit(i, size);
            self.wake_at(i, now + period);
            return;
        }
        let paced = matches!(self.flows[i].spec.model, Model::Quic { .. });
        loop {
            let f = &mut self.flows[i];
            if !f.cc.can_send() || f.remaining == Some(0) {
                return;
            }
            if paced && now < f.next_pace {
                let at = f.next_pace;
                self.wake_at(i, at);
                return;
            }
            let len = f
                .remaining
                .map_or(MSS, |r| r.min(MSS as u64) as usize)
                .max(MIN_PACKET);
            f.remaining = f.remaining.map(|r| r.saturating_sub(len as u64));
            f.cc.inflight += 1;
            if paced {
                f.next_pace = f.next_pace.max(now) + f.cc.pacing_gap(len);
            }
            self.emit(i, len);
        }
    }

    /// After an ack or a loss: finish the transfer, or send more.
    fn progress(&mut self, i: usize) {
        let now = self.now();
        let f = &mut self.flows[i];
        if f.remaining == Some(0) && f.cc.inflight == 0 {
            if !f.idle {
                f.idle = true;
                f.r.transfers += 1;
                if let Model::Web { mean_think, .. } = f.spec.model {
                    let think = f.rng.exp(mean_think as f64) as Nanos;
                    self.wake_at(i, now + think.max(1));
                }
            }
            return;
        }
        self.send(i);
    }

    fn emit(&mut self, i: usize, len: usize) {
        let now = self.now();
        let f = &mut self.flows[i];
        f.r.sent_packets += 1;
        let packet = SimPacket {
            flow: i,
            len,
            sent_at: now,
            shaper_at: 0,
            ect: f.spec.ecn_capable(),
            ce: false,
            delivered: false,
            losses: Rc::clone(&self.losses),
        };
        let at = now + f.spec.rtt / 2;
        self.push(at, Event::Arrive(packet));
    }

    fn arrive(&mut self, p: SimPacket) {
        let cap = bytes_in(self.cfg.line_rate, self.cfg.isp_buffer).max(MSS as u64);
        if self.link_bytes + p.len as u64 > cap {
            self.isp_drops += 1;
            return; // `p` drops here and reports itself lost.
        }
        self.link_bytes += p.len as u64;
        self.link.push_back(p);
        if !self.link_busy {
            self.start_link();
        }
    }

    fn start_link(&mut self) {
        let len = self.link.front().expect("called with packets waiting").len;
        let at = self.now() + serialise(len, self.cfg.line_rate);
        self.push(at, Event::LinkDone);
        self.link_busy = true;
    }

    fn link_done(&mut self) {
        let now = self.now();
        let mut p = self.link.pop_front().expect("LinkDone with an empty link");
        self.link_bytes -= p.len as u64;
        if self.link.is_empty() {
            self.link_busy = false;
        } else {
            self.start_link();
        }
        p.shaper_at = now;
        let meta = self.meta(&p);
        self.sched.enqueue(now, meta, p);
        self.pump();
    }

    /// Classify against the scheduler's current config, as the service's
    /// classifier will.
    fn meta(&self, p: &SimPacket) -> PacketMeta {
        let f = &self.flows[p.flow];
        let class = self
            .sched
            .config()
            .classes
            .iter()
            .position(|c| c.name == f.spec.class)
            .unwrap_or(self.sched.other_class());
        PacketMeta {
            prio: f.spec.prio,
            ..PacketMeta::new(class, mix(((p.flow as u64) << 32) | f.conn))
        }
    }

    /// Send everything the scheduler lets out now, then set its timer.
    fn pump(&mut self) {
        loop {
            match self.sched.dequeue(self.now()) {
                Dequeue::Packet { class, packet } => self.deliver(class, packet),
                Dequeue::WaitUntil(t) => {
                    let g = self.cfg.timer_granularity;
                    let t = if g > 0 { t.div_ceil(g) * g } else { t };
                    if self.shaper_wake.is_none_or(|w| t < w) {
                        self.shaper_wake = Some(t);
                        self.push(t, Event::Shaper);
                    }
                    return;
                }
                Dequeue::Empty => return,
            }
        }
    }

    fn deliver(&mut self, class: usize, mut p: SimPacket) {
        p.delivered = true;
        let now = self.now();
        let len = p.len as u64;
        let bin = (now / self.cfg.bin) as usize;
        let name = self.sched.class_name(class);
        match self.classes.get_mut(name) {
            Some(bins) => add(bins, bin, len),
            None => add(self.classes.entry(name.to_owned()).or_default(), bin, len),
        }
        let f = &mut self.flows[p.flow];
        add(&mut f.r.bins, bin, len);
        f.r.delivered_packets += 1;
        f.r.delivered_bytes += len;
        f.r.marked_packets += u64::from(p.ce);
        f.r.shaper_delay.push((now, now - p.shaper_at));
        let base = f.spec.rtt / 2;
        f.r.queue_delay
            .push((now, (now - p.sent_at).saturating_sub(base)));
        if f.spec.model.congestion_controlled() {
            let at = now + (f.spec.rtt - base);
            let ev = Event::Ack {
                flow: p.flow,
                sent_at: p.sent_at,
                ce: p.ce,
            };
            self.push(at, ev);
        }
    }

    /// Tell senders about packets dropped since the last call. The signal
    /// takes the rest of the path's half RTT to reach them.
    fn collect_losses(&mut self) {
        let lost = std::mem::take(&mut *self.losses.borrow_mut());
        let now = self.now();
        for (flow, len) in lost {
            let f = &mut self.flows[flow];
            f.r.lost_packets += 1;
            f.r.loss_times.push(now);
            if f.spec.model.congestion_controlled() {
                let at = now + f.spec.rtt / 2;
                self.push(at, Event::Loss { flow, len });
            }
        }
    }
}

fn add(bins: &mut Vec<u64>, i: usize, bytes: u64) {
    if bins.len() <= i {
        bins.resize(i + 1, 0);
    }
    bins[i] += bytes;
}

fn bytes_in(rate: u64, window: Nanos) -> u64 {
    (rate as u128 * window as u128 / NANOS_PER_SEC as u128) as u64
}

fn serialise(len: usize, rate: u64) -> Nanos {
    ((len as u128 * NANOS_PER_SEC as u128).div_ceil(rate as u128) as Nanos).max(1)
}
