//! Calls go first (PLAN.md §11.9): spot a live voice or video call (or a
//! game's server stream) by the shape of its traffic, so its packets go
//! first with no rule, in any app and any mode.
//!
//! A call is a UDP flow that keeps a steady beat: at least [`MIN_PPS`]
//! packets a second, of modest size, at no more than [`MAX_DIR_BPS`] each
//! way, for [`ENTER_WINDOWS`] seconds running. That fits Discord, Zoom,
//! Teams, Meet and WhatsApp (RTP every 20 ms, plus video frames under
//! 1 200 bytes) and games' tick streams, and not:
//!
//! - **QUIC** (YouTube, most HTTP/3): on port 443 with QUIC's fixed bit set,
//!   while RTP's first byte is `10xxxxxx`.
//! - **Bulk UDP** (a fast download over UDP, Remote Play): over the rate cap.
//! - **BitTorrent's uTP**: one app with more than [`MAX_CALL_FLOWS`] such
//!   flows at once is a swarm, not a call, and none of them count.
//!
//! A call stays one while it is quiet (nobody talking, a muted listener;
//! keep-alives still arrive) and ends after [`HOLD`] without a packet, or at
//! once if the flow turns bulky. Going first never makes a slice bigger:
//! the flow gets High priority inside its own class (PLAN.md §9.3, §9.5).

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::Hash;

use crate::clock::{NANOS_PER_SEC, Nanos};

/// Packets a second, both directions together, for a live second.
pub const MIN_PPS: u64 = 20;
/// Most bytes per packet on average: video calls cap packets near 1 200.
pub const MAX_MEAN_LEN: u64 = 1_250;
/// Most bits a second in either direction: a 1080p video call is ~4 Mbps.
pub const MAX_DIR_BPS: u64 = 6_000_000;
/// Live seconds in a row before a flow counts as a call.
pub const ENTER_WINDOWS: u8 = 3;
/// More call-shaped flows than this in one app is a swarm (uTP).
pub const MAX_CALL_FLOWS: u16 = 4;
/// A call ends after this long without a packet.
pub const HOLD: Nanos = 15 * NANOS_PER_SEC;
/// A flow is forgotten after this long without a packet.
const FORGET: Nanos = 60 * NANOS_PER_SEC;
const WINDOW: Nanos = NANOS_PER_SEC;
/// A window stretched past this by a gap says nothing about the beat.
const MAX_WINDOW: Nanos = 2 * NANOS_PER_SEC;
/// Most flows tracked; past this, new flows aren't (a scan or a swarm).
const MAX_FLOWS: usize = 4_096;

/// One UDP packet, as the watch needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Udp {
    pub outbound: bool,
    /// IP packet length.
    pub len: usize,
    pub remote_port: u16,
    /// The first payload byte, if there is one.
    pub first: Option<u8>,
}

impl Udp {
    /// QUIC on its usual port: long header `11xxxxxx` or short header
    /// `01xxxxxx`. RTP and RTCP are `10xxxxxx`, STUN `00xxxxxx`.
    fn quic_like(&self) -> bool {
        self.remote_port == 443 && self.first.is_some_and(|b| b & 0x40 != 0)
    }
}

#[derive(Debug, Clone)]
struct Shape<A> {
    app: A,
    start: Nanos,
    last: Nanos,
    pkts: u64,
    bytes: u64,
    /// [inbound, outbound] bytes this window.
    dir_bytes: [u64; 2],
    quic: u64,
    live: u8,
    call: bool,
}

impl<A> Shape<A> {
    fn new(app: A, now: Nanos) -> Self {
        Self {
            app,
            start: now,
            last: now,
            pkts: 0,
            bytes: 0,
            dir_bytes: [0; 2],
            quic: 0,
            live: 0,
            call: false,
        }
    }

    /// Close the window that started at `self.start`, ending at `now`.
    /// Returns whether the flow is a call afterwards.
    fn close_window(&mut self, now: Nanos) -> bool {
        let span = (now - self.start).max(1);
        let bps = |bytes: u64| ((bytes as u128 * 8 * NANOS_PER_SEC as u128) / span as u128) as u64;
        let bulky = self.dir_bytes.iter().any(|&b| bps(b) > MAX_DIR_BPS)
            || self.bytes > self.pkts * MAX_MEAN_LEN
            || self.quic * 2 > self.pkts;
        let beat = span <= MAX_WINDOW && self.pkts * NANOS_PER_SEC >= MIN_PPS * span;
        if bulky {
            self.live = 0;
            self.call = false;
        } else if beat {
            self.live = self.live.saturating_add(1);
            self.call |= self.live >= ENTER_WINDOWS;
        } else {
            // Quiet: nobody talking. A call stays one; a newcomer starts over.
            self.live = 0;
        }
        self.start = now;
        self.pkts = 0;
        self.bytes = 0;
        self.dir_bytes = [0; 2];
        self.quic = 0;
        self.call
    }
}

/// Which UDP flows are calls, keyed by flow (`K`) and owned by an app (`A`).
#[derive(Debug, Clone)]
pub struct CallWatch<K, A> {
    flows: HashMap<K, Shape<A>>,
    /// Call flows per app, for the swarm test.
    per_app: HashMap<A, u16>,
    swept: Nanos,
}

impl<K: Hash + Eq + Copy, A: Hash + Eq + Copy> Default for CallWatch<K, A> {
    fn default() -> Self {
        Self {
            flows: HashMap::new(),
            per_app: HashMap::new(),
            swept: 0,
        }
    }
}

impl<K: Hash + Eq + Copy, A: Hash + Eq + Copy> CallWatch<K, A> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Count a UDP packet of flow `key`, owned by `app`. Returns whether
    /// the flow is a call now and should go first.
    pub fn observe(&mut self, key: K, app: A, p: Udp, now: Nanos) -> bool {
        if now.saturating_sub(self.swept) >= WINDOW {
            self.sweep(now);
        }
        let full = self.flows.len() >= MAX_FLOWS;
        let shape = match self.flows.entry(key) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => {
                if full {
                    return false;
                }
                e.insert(Shape::new(app, now))
            }
        };
        let prev_app = shape.app;
        let was = shape.call;
        let mut is = was;
        let expired = was && now.saturating_sub(shape.last) >= HOLD;
        if prev_app != app || expired {
            *shape = Shape::new(app, now);
            is = false;
        } else if now.saturating_sub(shape.start) >= WINDOW {
            is = shape.close_window(now);
        }
        shape.last = now;
        shape.pkts += 1;
        shape.bytes += p.len as u64;
        shape.dir_bytes[usize::from(p.outbound)] += p.len as u64;
        shape.quic += u64::from(p.quic_like());
        if was && !is {
            self.count(prev_app, false);
        } else if !was && is {
            self.count(app, true);
        }
        is && self.per_app.get(&app).is_some_and(|&n| n <= MAX_CALL_FLOWS)
    }

    fn count(&mut self, app: A, up: bool) {
        let n = self.per_app.entry(app).or_default();
        if up {
            *n += 1;
        } else {
            *n = n.saturating_sub(1);
            if *n == 0 {
                self.per_app.remove(&app);
            }
        }
    }

    /// End calls that went silent and forget idle flows.
    fn sweep(&mut self, now: Nanos) {
        self.swept = now;
        let mut ended = Vec::new();
        self.flows.retain(|_, s| {
            let idle = now.saturating_sub(s.last);
            if s.call && idle >= HOLD {
                s.call = false;
                s.live = 0;
                ended.push(s.app);
            }
            idle < FORGET
        });
        for app in ended {
            self.count(app, false);
        }
    }

    /// The apps in a call at `now`: a call flow heard from within [`HOLD`],
    /// swarms left out. Each app once, in no particular order.
    pub fn apps(&self, now: Nanos) -> Vec<A> {
        let mut out: Vec<A> = Vec::new();
        for s in self.flows.values() {
            if s.call
                && now.saturating_sub(s.last) < HOLD
                && self
                    .per_app
                    .get(&s.app)
                    .is_some_and(|&n| n <= MAX_CALL_FLOWS)
                && !out.contains(&s.app)
            {
                out.push(s.app);
            }
        }
        out
    }

    /// Flows being watched.
    pub fn len(&self) -> usize {
        self.flows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.flows.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::NANOS_PER_MILLI;

    type Watch = CallWatch<u32, u8>;

    fn rtp(outbound: bool, len: usize) -> Udp {
        Udp {
            outbound,
            len,
            remote_port: 50_004,
            first: Some(0x80),
        }
    }

    /// Feed flow `key` of `app` with `per_sec` packets a second each way
    /// for `secs`, starting at `from`. Returns (whether it was a call at the
    /// last packet, the time after).
    fn feed(
        w: &mut Watch,
        key: u32,
        app: u8,
        from: Nanos,
        secs: u64,
        per_sec: u64,
        p: impl Fn(bool) -> Udp,
    ) -> (bool, Nanos) {
        let step = NANOS_PER_SEC / per_sec;
        let mut now = from;
        let mut call = false;
        for _ in 0..secs * per_sec {
            w.observe(key, app, p(false), now);
            call = w.observe(key, app, p(true), now + step / 2);
            now += step;
        }
        (call, now)
    }

    #[test]
    fn a_voice_call_goes_first_after_three_seconds() {
        let mut w = Watch::new();
        // Opus every 20 ms, both ways.
        let (call, now) = feed(&mut w, 1, 7, 0, 2, 50, |o| rtp(o, 160));
        assert!(!call, "two seconds is not enough");
        let (call, now) = feed(&mut w, 1, 7, now, 2, 50, |o| rtp(o, 160));
        assert!(call);
        assert_eq!(w.apps(now), vec![7]);
    }

    #[test]
    fn a_listen_only_call_counts_too() {
        // Muted: only the others' voices arrive.
        let mut w = Watch::new();
        let mut now = 0;
        let mut call = false;
        for _ in 0..250 {
            call = w.observe(1, 7, rtp(false, 160), now);
            now += 20 * NANOS_PER_MILLI;
        }
        assert!(call);
    }

    #[test]
    fn a_video_call_counts() {
        // 2.5 Mbps of ~1 100-byte frames in, voice out.
        let mut w = Watch::new();
        let mut now = 0;
        let mut call = false;
        for i in 0..1_400u64 {
            call = w.observe(1, 7, rtp(false, 1_100), now);
            if i % 6 == 0 {
                call = w.observe(1, 7, rtp(true, 160), now);
            }
            now += 3_500_000; // ~285 packets a second
        }
        assert!(call);
    }

    #[test]
    fn quic_is_not_a_call() {
        let mut w = Watch::new();
        let quic = |outbound| Udp {
            outbound,
            len: 1_200,
            remote_port: 443,
            first: Some(0x43),
        };
        let (call, now) = feed(&mut w, 1, 7, 0, 5, 60, quic);
        assert!(!call);
        assert!(w.apps(now).is_empty());
    }

    #[test]
    fn rtp_on_port_443_still_counts() {
        // Calls relayed over 443 keep RTP's first byte.
        let mut w = Watch::new();
        let relayed = |outbound| Udp {
            remote_port: 443,
            ..rtp(outbound, 200)
        };
        assert!(feed(&mut w, 1, 7, 0, 4, 50, relayed).0);
    }

    #[test]
    fn bulk_udp_is_not_a_call_and_ends_one() {
        let mut w = Watch::new();
        // 10 Mbps in, 1 000 packets a second each way.
        let bulk = |o| rtp(o, if o { 60 } else { 1_250 });
        let (call, now) = feed(&mut w, 1, 7, 0, 5, 1_000, bulk);
        assert!(!call);
        // A call that turns bulky stops being one at the next window.
        let (call, now) = feed(&mut w, 2, 7, now, 4, 50, |o| rtp(o, 160));
        assert!(call);
        let (call, _) = feed(&mut w, 2, 7, now, 2, 1_000, bulk);
        assert!(!call);
    }

    #[test]
    fn sparse_traffic_is_not_a_call() {
        let mut w = Watch::new();
        assert!(!feed(&mut w, 1, 7, 0, 10, 5, |o| rtp(o, 100)).0);
    }

    #[test]
    fn a_quiet_call_stays_until_it_goes_silent() {
        let mut w = Watch::new();
        let (call, mut now) = feed(&mut w, 1, 7, 0, 4, 50, |o| rtp(o, 160));
        assert!(call);
        // Nobody talks: a keep-alive every 5 s.
        for _ in 0..4 {
            now += 5 * NANOS_PER_SEC;
            assert!(w.observe(1, 7, rtp(true, 60), now));
        }
        assert_eq!(w.apps(now), vec![7]);
        // Then nothing at all: over once HOLD passes.
        assert!(w.apps(now + HOLD).is_empty());
        let later = now + HOLD + NANOS_PER_SEC;
        assert!(!w.observe(1, 7, rtp(true, 60), later));
        assert!(w.apps(later).is_empty());
    }

    #[test]
    fn a_swarm_of_call_shaped_flows_is_not_a_call() {
        let mut w = Watch::new();
        let mut now = 0;
        let mut calls = [false; 6];
        // Six peers at 30 small packets a second each way, interleaved.
        for _ in 0..(5 * 30) {
            for (k, c) in calls.iter_mut().enumerate() {
                w.observe(k as u32, 9, rtp(false, 400), now);
                *c = w.observe(k as u32, 9, rtp(true, 80), now);
            }
            now += NANOS_PER_SEC / 30;
        }
        assert!(calls.iter().all(|c| !c));
        assert!(w.apps(now).is_empty());
        // Another app's real call still counts.
        assert!(feed(&mut w, 100, 7, now, 4, 50, |o| rtp(o, 160)).0);
    }

    #[test]
    fn idle_flows_are_forgotten() {
        let mut w = Watch::new();
        let (_, now) = feed(&mut w, 1, 7, 0, 4, 50, |o| rtp(o, 160));
        assert_eq!(w.len(), 1);
        w.observe(2, 8, rtp(true, 100), now + FORGET + NANOS_PER_SEC);
        assert_eq!(w.len(), 1, "flow 1 is gone, flow 2 is new");
        assert!(w.apps(now + FORGET).is_empty());
    }

    #[test]
    fn port_reuse_by_another_app_resets_call_and_per_app_count() {
        let mut w = Watch::new();
        let (call, now) = feed(&mut w, 1, 7, 0, 4, 50, |o| rtp(o, 160));
        assert!(call);
        assert_eq!(w.apps(now), vec![7]);
        // Flow key 1 is now owned by app 8 (reused ephemeral port).
        assert!(!w.observe(1, 8, rtp(true, 160), now + 10 * NANOS_PER_MILLI));
        assert!(w.apps(now + 10 * NANOS_PER_MILLI).is_empty());
    }

    #[test]
    fn silent_call_past_hold_ends_even_without_intermediate_sweep() {
        let mut w = Watch::new();
        let (call, now) = feed(&mut w, 1, 7, 0, 4, 50, |o| rtp(o, 160));
        assert!(call);
        // Simulate sweep timestamp being recent while this flow sat idle >= HOLD.
        w.swept = now + HOLD;
        assert!(!w.observe(1, 7, rtp(true, 60), now + HOLD + 10 * NANOS_PER_MILLI));
        assert!(w.apps(now + HOLD + 10 * NANOS_PER_MILLI).is_empty());
    }
}
