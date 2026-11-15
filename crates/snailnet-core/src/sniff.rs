//! Which name a flow was opened for (PLAN.md §8.1, §8.4): read from the
//! flow's first outbound payloads (TLS ClientHello on TCP, QUIC Initial on
//! UDP), else from the IP → domain cache, else unidentified.
//!
//! Single-threaded; the caller shares it behind a lock and only consults
//! it when some rule needs site names.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use crate::classify::FlowKey;
use crate::clock::{NANOS_PER_SEC, Nanos};
use crate::parse::ip::{PROTO_TCP, PROTO_UDP, PacketInfo};
use crate::parse::quic::{CryptoAssembler, open_initial};
use crate::parse::tls::{self, Hello, MAX_HELLO};
use crate::sites::{DomainCache, Names};

/// Port whose first payloads are read for a name.
const HTTPS: u16 = 443;
/// Outbound payload packets read before giving up on a hello.
const MAX_TCP_SEGMENTS: u8 = 12;
const MAX_QUIC_PACKETS: u8 = 6;
/// Entries idle this long are dropped by [`FlowNames::sweep`]. Long, since
/// a dropped live flow can't be named again (its hello has passed).
pub const IDLE_EXPIRY: Nanos = 15 * 60 * NANOS_PER_SEC;

/// How a flow got its name (or why it has none).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    /// TLS or QUIC server name.
    Sni,
    /// The IP → domain cache (DNS answers).
    Dns,
    /// Encrypted Client Hello hid the name, and the cache had none.
    Ech,
    /// Nothing usable.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Named {
    /// Still reading the hello: classify by app for now.
    Pending,
    /// Decided. `names` is the SNI, or a DNS CNAME chain (question first).
    Known { names: Option<Names>, via: Via },
}

#[derive(Debug)]
enum State {
    Tcp { buf: Vec<u8>, segments: u8 },
    Quic { asm: CryptoAssembler, packets: u8 },
    Known { names: Option<Names>, via: Via },
}

#[derive(Debug)]
struct Entry {
    state: State,
    last_seen: Nanos,
}

/// Per-flow name state, capped.
#[derive(Debug)]
pub struct FlowNames {
    flows: HashMap<FlowKey, Entry>,
    cap: usize,
}

impl FlowNames {
    pub fn new(cap: usize) -> Self {
        Self {
            flows: HashMap::new(),
            cap: cap.max(1),
        }
    }

    /// Feed one packet of flow `key`; `pkt` is the whole IP packet and
    /// `info` its parse.
    pub fn observe(
        &mut self,
        key: FlowKey,
        pkt: &[u8],
        info: &PacketInfo,
        outbound: bool,
        cache: &DomainCache,
        now: Nanos,
    ) -> Named {
        if let Some(e) = self.flows.get_mut(&key) {
            e.last_seen = now;
            if let State::Known { names, via } = &e.state {
                return Named::Known {
                    names: names.clone(),
                    via: *via,
                };
            }
        }
        let sniffable = key.remote.1 == HTTPS && matches!(key.protocol, PROTO_TCP | PROTO_UDP);
        if !sniffable {
            return self.settle(key, from_cache(cache, key.remote.0, now, Via::None), now);
        }
        let payload = info.payload(pkt);
        if !outbound || payload.is_empty() {
            // Handshake, ACKs, or a reply before we saw the hello. A reply
            // with no state means we missed the start: use the cache.
            if !outbound && !payload.is_empty() && !self.flows.contains_key(&key) {
                return self.settle(key, from_cache(cache, key.remote.0, now, Via::None), now);
            }
            return Named::Pending;
        }

        let fresh = || {
            if key.protocol == PROTO_TCP {
                State::Tcp {
                    buf: Vec::new(),
                    segments: 0,
                }
            } else {
                State::Quic {
                    asm: CryptoAssembler::default(),
                    packets: 0,
                }
            }
        };
        // Full of live flows: still read this packet, but keep no state, so
        // a hello that spans packets goes unnamed.
        let tracked = self.flows.contains_key(&key) || self.make_room(now);
        let mut scratch;
        let state = if tracked {
            let entry = self.flows.entry(key).or_insert_with(|| Entry {
                state: fresh(),
                last_seen: now,
            });
            entry.last_seen = now;
            &mut entry.state
        } else {
            scratch = fresh();
            &mut scratch
        };
        let hello = match state {
            State::Tcp { buf, segments } => {
                *segments += 1;
                if buf.len() + payload.len() > MAX_HELLO {
                    Hello::NotHello
                } else {
                    buf.extend_from_slice(payload);
                    match tls::from_tcp(buf) {
                        Hello::NeedMore if *segments >= MAX_TCP_SEGMENTS => Hello::NotHello,
                        h => h,
                    }
                }
            }
            State::Quic { asm, packets } => {
                *packets += 1;
                if let Some(init) = open_initial(payload) {
                    for (off, data) in &init.crypto {
                        asm.push(*off, data);
                    }
                }
                match tls::from_handshake(asm.prefix()) {
                    Hello::NeedMore if *packets >= MAX_QUIC_PACKETS => Hello::NotHello,
                    // Nothing yet but the packet wasn't an Initial we
                    // could open: keep trying until the limit.
                    Hello::NotHello if asm.prefix().is_empty() && *packets < MAX_QUIC_PACKETS => {
                        Hello::NeedMore
                    }
                    h => h,
                }
            }
            State::Known { .. } => unreachable!("returned above"),
        };
        match hello {
            Hello::NeedMore if tracked => Named::Pending,
            Hello::NeedMore => Named::Known {
                names: None,
                via: Via::None,
            },
            Hello::Complete(h) => {
                let decided = match h.sni {
                    Some(sni) if !h.ech => (Some(Arc::from(vec![sni])), Via::Sni),
                    _ => from_cache(
                        cache,
                        key.remote.0,
                        now,
                        if h.ech { Via::Ech } else { Via::None },
                    ),
                };
                self.settle(key, decided, now)
            }
            Hello::NotHello => {
                self.settle(key, from_cache(cache, key.remote.0, now, Via::None), now)
            }
        }
    }

    fn settle(&mut self, key: FlowKey, (names, via): (Option<Names>, Via), now: Nanos) -> Named {
        if self.flows.contains_key(&key) || self.make_room(now) {
            self.flows.insert(
                key,
                Entry {
                    state: State::Known {
                        names: names.clone(),
                        via,
                    },
                    last_seen: now,
                },
            );
        }
        Named::Known { names, via }
    }

    /// Ensure there is room for one more entry. False if every entry is
    /// live.
    fn make_room(&mut self, now: Nanos) -> bool {
        if self.flows.len() < self.cap {
            return true;
        }
        self.sweep(now);
        self.flows.len() < self.cap
    }

    /// The flow ended (FLOW delete / socket close).
    pub fn forget(&mut self, key: &FlowKey) {
        self.flows.remove(key);
    }

    /// Drop entries idle for [`IDLE_EXPIRY`].
    pub fn sweep(&mut self, now: Nanos) {
        self.flows
            .retain(|_, e| now.saturating_sub(e.last_seen) < IDLE_EXPIRY);
    }

    pub fn len(&self) -> usize {
        self.flows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.flows.is_empty()
    }
}

fn from_cache(cache: &DomainCache, ip: IpAddr, now: Nanos, otherwise: Via) -> (Option<Names>, Via) {
    match cache.get(&ip, now) {
        Some(n) => (Some(n.clone()), Via::Dns),
        None => (None, otherwise),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ip;
    use crate::parse::tls::build::{Spec, handshake, records};
    use proptest::prelude::*;

    const ME: [u8; 4] = [192, 168, 1, 5];
    const THEM: [u8; 4] = [142, 250, 1, 1];

    /// An IPv4 TCP/UDP packet between ME:50000 and THEM:port.
    fn packet(proto: u8, port: u16, outbound: bool, payload: &[u8]) -> Vec<u8> {
        let th = if proto == PROTO_TCP { 20 } else { 8 };
        let total = 20 + th + payload.len();
        let mut p = vec![0u8; total];
        p[0] = 0x45;
        p[2..4].copy_from_slice(&(total as u16).to_be_bytes());
        p[9] = proto;
        let (src, dst, sp, dp) = if outbound {
            (ME, THEM, 50_000u16, port)
        } else {
            (THEM, ME, port, 50_000)
        };
        p[12..16].copy_from_slice(&src);
        p[16..20].copy_from_slice(&dst);
        p[20..22].copy_from_slice(&sp.to_be_bytes());
        p[22..24].copy_from_slice(&dp.to_be_bytes());
        if proto == PROTO_TCP {
            p[32] = 5 << 4;
        }
        p[20 + th..].copy_from_slice(payload);
        p
    }

    struct Rig {
        names: FlowNames,
        cache: DomainCache,
    }

    impl Rig {
        fn new() -> Self {
            Self {
                names: FlowNames::new(64),
                cache: DomainCache::new(64),
            }
        }
        fn feed(&mut self, proto: u8, port: u16, outbound: bool, payload: &[u8]) -> Named {
            let p = packet(proto, port, outbound, payload);
            let info = ip::parse(&p).unwrap();
            let key = FlowKey::from_packet(&info, outbound);
            self.names.observe(key, &p, &info, outbound, &self.cache, 0)
        }
    }

    fn known(name: &str, via: Via) -> Named {
        Named::Known {
            names: Some(Arc::from(vec![name.to_string()])),
            via,
        }
    }

    fn hello(sni: &[u8], ech: bool, pad: usize) -> Vec<u8> {
        records(
            &handshake(&Spec {
                sni: Some(sni),
                ech,
                pad,
            }),
            16384,
        )
    }

    #[test]
    fn tcp_hello_over_two_segments() {
        let mut r = Rig::new();
        assert_eq!(r.feed(PROTO_TCP, 443, true, b""), Named::Pending, "SYN");
        assert_eq!(
            r.feed(PROTO_TCP, 443, false, b""),
            Named::Pending,
            "SYN-ACK"
        );
        let h = hello(b"www.youtube.com", false, 1600);
        assert_eq!(r.feed(PROTO_TCP, 443, true, &h[..1400]), Named::Pending);
        assert_eq!(
            r.feed(PROTO_TCP, 443, true, &h[1400..]),
            known("www.youtube.com", Via::Sni)
        );
        // Later packets either way: remembered.
        assert_eq!(
            r.feed(PROTO_TCP, 443, false, b"data"),
            known("www.youtube.com", Via::Sni)
        );
    }

    #[test]
    fn ech_falls_back_to_the_cache() {
        let mut r = Rig::new();
        let h = hello(b"cloudflare-ech.com", true, 0);
        assert_eq!(
            r.feed(PROTO_TCP, 443, true, &h),
            Named::Known {
                names: None,
                via: Via::Ech
            }
        );
        let mut r = Rig::new();
        r.cache.insert(
            THEM.into(),
            Arc::from(vec!["www.twitch.tv".to_string()]),
            60,
            0,
        );
        assert_eq!(
            r.feed(PROTO_TCP, 443, true, &h),
            known("www.twitch.tv", Via::Dns)
        );
    }

    #[test]
    fn other_ports_use_the_cache_at_once() {
        let mut r = Rig::new();
        assert_eq!(
            r.feed(PROTO_TCP, 80, true, b""),
            Named::Known {
                names: None,
                via: Via::None
            }
        );
        let mut r = Rig::new();
        r.cache.insert(
            THEM.into(),
            Arc::from(vec!["example.com".to_string()]),
            60,
            0,
        );
        assert_eq!(
            r.feed(PROTO_UDP, 3478, true, b"x"),
            known("example.com", Via::Dns)
        );
    }

    #[test]
    fn non_tls_on_443_settles_without_a_name() {
        let mut r = Rig::new();
        assert_eq!(
            r.feed(PROTO_TCP, 443, true, b"GET / HTTP/1.1\r\n"),
            Named::Known {
                names: None,
                via: Via::None
            }
        );
    }

    #[test]
    fn a_reply_before_any_hello_uses_the_cache() {
        let mut r = Rig::new();
        assert_eq!(
            r.feed(PROTO_TCP, 443, false, b"midstream"),
            Named::Known {
                names: None,
                via: Via::None
            }
        );
    }

    #[test]
    fn quic_gives_up_after_unopenable_packets() {
        let mut r = Rig::new();
        for _ in 0..MAX_QUIC_PACKETS - 1 {
            assert_eq!(r.feed(PROTO_UDP, 443, true, &[0x40; 100]), Named::Pending);
        }
        assert_eq!(
            r.feed(PROTO_UDP, 443, true, &[0x40; 100]),
            Named::Known {
                names: None,
                via: Via::None
            }
        );
    }

    #[test]
    fn capped_and_swept() {
        let mut names = FlowNames::new(2);
        let cache = DomainCache::new(4);
        let h = hello(b"a.example", false, 0);
        for port in 1..=3u16 {
            let mut p = packet(PROTO_TCP, 443, true, &h);
            p[20..22].copy_from_slice(&port.to_be_bytes()); // new local port
            let info = ip::parse(&p).unwrap();
            let key = FlowKey::from_packet(&info, true);
            let got = names.observe(key, &p, &info, true, &cache, 0);
            assert_eq!(got, known("a.example", Via::Sni), "still answered");
        }
        assert_eq!(names.len(), 2, "third not stored while the others are live");
        names.sweep(IDLE_EXPIRY);
        assert!(names.is_empty());
    }

    proptest! {
        #[test]
        fn never_panics(
            pkts in proptest::collection::vec(
                (any::<bool>(), any::<bool>(), proptest::collection::vec(any::<u8>(), 0..1500)),
                0..20,
            ),
        ) {
            let mut r = Rig::new();
            for (tcp, outbound, payload) in pkts {
                let proto = if tcp { PROTO_TCP } else { PROTO_UDP };
                let _ = r.feed(proto, 443, outbound, &payload);
            }
        }
    }
}
