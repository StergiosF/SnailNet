//! DNS response reader: which addresses a name resolved to, for the
//! IP → domain cache (PLAN.md §8.1). Answers only; the question is just
//! the start of the CNAME chain.
//!
//! Takes the DNS message itself (a UDP payload, or a TCP one after its
//! 2-byte length). Name compression is followed backwards only, with a hop
//! limit, so a crafted message can't loop.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::tls::host_name;

const TYPE_A: u16 = 1;
const TYPE_CNAME: u16 = 5;
const TYPE_AAAA: u16 = 28;
const CLASS_IN: u16 = 1;
/// Longest CNAME chain followed.
const MAX_CHAIN: usize = 8;
/// Most answer records read from one message.
const MAX_ANSWERS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    /// The CNAME chain, starting with the name asked for. Lowercased,
    /// without the trailing dot.
    pub names: Vec<String>,
    /// Addresses at the end of the chain, with their TTL in seconds.
    pub addrs: Vec<(IpAddr, u32)>,
}

/// Read a successful response to an A/AAAA question. `None` for queries,
/// errors, other question types, empty answers or malformed messages.
pub fn answers(msg: &[u8]) -> Option<Resolution> {
    let h = msg.get(..12)?;
    let flags = u16::from_be_bytes([h[2], h[3]]);
    let qr = flags & 0x8000 != 0;
    let opcode = (flags >> 11) & 0x0f;
    let rcode = flags & 0x000f;
    let qdcount = u16::from_be_bytes([h[4], h[5]]);
    let ancount = u16::from_be_bytes([h[6], h[7]]) as usize;
    if !qr || opcode != 0 || rcode != 0 || qdcount != 1 || ancount == 0 {
        return None;
    }

    let mut pos = 12;
    let qname = name(msg, &mut pos)?;
    let qtype = u16_at(msg, pos)?;
    let qclass = u16_at(msg, pos + 2)?;
    pos += 4;
    if !matches!(qtype, TYPE_A | TYPE_AAAA) || qclass != CLASS_IN {
        return None;
    }

    // Read the answer section (owner, type, ttl, data) before following
    // the chain: servers usually order it, but needn't.
    let mut records = Vec::new();
    for _ in 0..ancount.min(MAX_ANSWERS) {
        let owner = name(msg, &mut pos)?;
        let ty = u16_at(msg, pos)?;
        let class = u16_at(msg, pos + 2)?;
        let ttl = u32::from_be_bytes(msg.get(pos + 4..pos + 8)?.try_into().ok()?);
        let len = u16_at(msg, pos + 8)? as usize;
        pos += 10;
        let data_start = pos;
        let data = msg.get(pos..pos + len)?;
        pos += len;
        if class != CLASS_IN {
            continue;
        }
        let rdata = match ty {
            TYPE_A => Rdata::Addr(IpAddr::V4(Ipv4Addr::from(<[u8; 4]>::try_from(data).ok()?))),
            TYPE_AAAA => Rdata::Addr(IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(data).ok()?))),
            TYPE_CNAME => {
                let mut p = data_start;
                let target = name(msg, &mut p)?;
                if p != data_start + len {
                    return None;
                }
                Rdata::Cname(target)
            }
            _ => continue,
        };
        records.push((owner, ttl, rdata));
    }

    let mut names = vec![qname];
    while names.len() <= MAX_CHAIN {
        let last = names.last()?;
        let next = records.iter().find_map(|(owner, _, r)| match r {
            Rdata::Cname(t) if owner == last => Some(t.clone()),
            _ => None,
        });
        match next {
            Some(t) if !names.contains(&t) => names.push(t),
            _ => break,
        }
    }
    let end = names.last()?;
    let addrs: Vec<_> = records
        .iter()
        .filter_map(|(owner, ttl, r)| match r {
            Rdata::Addr(a) if owner == end => Some((*a, *ttl)),
            _ => None,
        })
        .collect();
    (!addrs.is_empty()).then_some(Resolution { names, addrs })
}

enum Rdata {
    Addr(IpAddr),
    Cname(String),
}

fn u16_at(msg: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(msg.get(at..at + 2)?.try_into().ok()?))
}

/// Read a (possibly compressed) name at `*pos`, advancing `*pos` past it.
fn name(msg: &[u8], pos: &mut usize) -> Option<String> {
    let mut out = Vec::with_capacity(64);
    let mut at = *pos;
    let mut jumped = false;
    let mut hops = 0;
    loop {
        let len = *msg.get(at)?;
        match len >> 6 {
            0b00 if len == 0 => {
                if !jumped {
                    *pos = at + 1;
                }
                break;
            }
            0b00 => {
                let label = msg.get(at + 1..at + 1 + len as usize)?;
                if !out.is_empty() {
                    out.push(b'.');
                }
                out.extend_from_slice(label);
                if out.len() > 253 {
                    return None;
                }
                at += 1 + len as usize;
            }
            0b11 => {
                let target = (u16_at(msg, at)? & 0x3fff) as usize;
                // Only backwards, and not too often: no loops.
                hops += 1;
                if target >= at || hops > 16 {
                    return None;
                }
                if !jumped {
                    *pos = at + 2;
                    jumped = true;
                }
                at = target;
            }
            _ => return None,
        }
    }
    host_name(&out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// A tiny message builder: names are written uncompressed unless a
    /// pointer is given.
    struct Msg(Vec<u8>);

    impl Msg {
        fn response(qname: &str, qtype: u16, ancount: u16) -> Msg {
            let mut m = vec![0x12, 0x34, 0x81, 0x80, 0, 1];
            m.extend_from_slice(&ancount.to_be_bytes());
            m.extend_from_slice(&[0, 0, 0, 0]);
            let mut msg = Msg(m);
            msg.name(qname);
            msg.0.extend_from_slice(&qtype.to_be_bytes());
            msg.0.extend_from_slice(&CLASS_IN.to_be_bytes());
            msg
        }
        fn name(&mut self, n: &str) {
            for label in n.split('.') {
                self.0.push(label.len() as u8);
                self.0.extend_from_slice(label.as_bytes());
            }
            self.0.push(0);
        }
        fn rr(&mut self, owner: &[u8], ty: u16, ttl: u32, data: &[u8]) {
            self.0.extend_from_slice(owner);
            self.0.extend_from_slice(&ty.to_be_bytes());
            self.0.extend_from_slice(&CLASS_IN.to_be_bytes());
            self.0.extend_from_slice(&ttl.to_be_bytes());
            self.0.extend_from_slice(&(data.len() as u16).to_be_bytes());
            self.0.extend_from_slice(data);
        }
    }

    fn encoded(n: &str) -> Vec<u8> {
        let mut m = Msg(Vec::new());
        m.name(n);
        m.0
    }

    /// Pointer to the question name (always at offset 12).
    const QPTR: [u8; 2] = [0xc0, 12];

    #[test]
    fn a_plain_a_answer() {
        let mut m = Msg::response("Example.COM", TYPE_A, 1);
        m.rr(&QPTR, TYPE_A, 300, &[93, 184, 215, 14]);
        assert_eq!(
            answers(&m.0),
            Some(Resolution {
                names: vec!["example.com".into()],
                addrs: vec![("93.184.215.14".parse().unwrap(), 300)],
            })
        );
    }

    #[test]
    fn follows_a_cname_chain_to_aaaa_records() {
        let mut m = Msg::response("www.youtube.com", TYPE_AAAA, 4);
        let ui = encoded("youtube-ui.l.google.com");
        let ui_at = m.0.len() + QPTR.len() + 10; // where the CNAME target lands
        m.rr(&QPTR, TYPE_CNAME, 3000, &ui);
        let v6a: Ipv6Addr = "2a00:1450:4017:80b::200e".parse().unwrap();
        let v6b: Ipv6Addr = "2a00:1450:4017:80c::200e".parse().unwrap();
        let ptr = [0xc0, ui_at as u8];
        m.rr(&ptr, TYPE_AAAA, 60, &v6a.octets());
        m.rr(&ptr, TYPE_AAAA, 60, &v6b.octets());
        m.rr(&QPTR, 46, 60, &[0; 20]); // RRSIG: ignored
        let r = answers(&m.0).unwrap();
        assert_eq!(r.names, ["www.youtube.com", "youtube-ui.l.google.com"]);
        assert_eq!(r.addrs, [(v6a.into(), 60), (v6b.into(), 60)]);
    }

    #[test]
    fn an_out_of_order_chain_is_still_followed() {
        let mut m = Msg::response("a.example", TYPE_A, 2);
        m.rr(&encoded("b.example"), TYPE_A, 5, &[10, 0, 0, 1]);
        m.rr(&QPTR, TYPE_CNAME, 5, &encoded("b.example"));
        let r = answers(&m.0).unwrap();
        assert_eq!(r.names, ["a.example", "b.example"]);
        assert_eq!(r.addrs, [("10.0.0.1".parse().unwrap(), 5)]);
    }

    #[test]
    fn a_cname_loop_stops() {
        let mut m = Msg::response("a.example", TYPE_A, 2);
        m.rr(&QPTR, TYPE_CNAME, 5, &encoded("b.example"));
        m.rr(&encoded("b.example"), TYPE_CNAME, 5, &encoded("a.example"));
        assert_eq!(answers(&m.0), None, "no addresses");
    }

    #[test]
    fn ignores_queries_errors_and_other_types() {
        let mut q = Msg::response("example.com", TYPE_A, 1);
        q.rr(&QPTR, TYPE_A, 1, &[1, 2, 3, 4]);
        let mut query = q.0.clone();
        query[2] &= 0x7f;
        assert_eq!(answers(&query), None);
        let mut nxdomain = q.0.clone();
        nxdomain[3] |= 3;
        assert_eq!(answers(&nxdomain), None);
        let mut txt = Msg::response("example.com", 16, 1);
        txt.rr(&QPTR, 16, 1, b"\x02hi");
        assert_eq!(answers(&txt.0), None);
        assert!(answers(&q.0).is_some());
    }

    #[test]
    fn forward_and_self_pointers_are_rejected() {
        let mut m = Msg::response("example.com", TYPE_A, 1);
        let here = m.0.len() as u8;
        m.rr(&[0xc0, here], TYPE_A, 1, &[1, 2, 3, 4]);
        assert_eq!(answers(&m.0), None);
    }

    #[test]
    fn a_bad_address_length_is_malformed() {
        let mut m = Msg::response("example.com", TYPE_A, 1);
        m.rr(&QPTR, TYPE_A, 1, &[1, 2, 3]);
        assert_eq!(answers(&m.0), None);
    }

    proptest! {
        #[test]
        fn never_panics_on_garbage(bytes in proptest::collection::vec(any::<u8>(), 0..600)) {
            let _ = answers(&bytes);
        }

        #[test]
        fn never_panics_on_a_corrupted_response(
            flips in proptest::collection::vec((any::<usize>(), any::<u8>()), 1..6),
            cut in any::<usize>(),
        ) {
            let mut m = Msg::response("www.youtube.com", TYPE_A, 2);
            m.rr(&QPTR, TYPE_CNAME, 30, &encoded("youtube-ui.l.google.com"));
            m.rr(&encoded("youtube-ui.l.google.com"), TYPE_A, 30, &[142, 250, 1, 1]);
            for (at, v) in flips {
                let i = at % m.0.len();
                m.0[i] ^= v;
            }
            let cut = cut % (m.0.len() + 1);
            let _ = answers(&m.0[..cut]);
        }
    }
}
