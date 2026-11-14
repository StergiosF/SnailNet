//! Minimal zero-copy IPv4/IPv6 + TCP/UDP header view.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub const PROTO_TCP: u8 = 6;
pub const PROTO_UDP: u8 = 17;

/// What the scheduler and classifier need to know about one packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketInfo {
    /// Total length of this IP packet (header + payload).
    pub len: usize,
    pub protocol: u8,
    pub src: IpAddr,
    pub dst: IpAddr,
    /// Zero unless TCP/UDP with the transport header present.
    pub src_port: u16,
    pub dst_port: u16,
    /// TCP payload bytes (0 for a pure control packet). `None` if not TCP.
    pub tcp_payload_len: Option<usize>,
}

/// Length of the IP packet at the start of `buf`, from its header. Used to
/// split a WinDivert batch, which is several packets back to back.
pub fn packet_len(buf: &[u8]) -> Option<usize> {
    let version = buf.first()? >> 4;
    let len = match version {
        4 => u16::from_be_bytes([*buf.get(2)?, *buf.get(3)?]) as usize,
        6 => 40 + u16::from_be_bytes([*buf.get(4)?, *buf.get(5)?]) as usize,
        _ => return None,
    };
    (len >= 20 && len <= buf.len()).then_some(len)
}

pub fn parse(buf: &[u8]) -> Option<PacketInfo> {
    let len = packet_len(buf)?;
    let pkt = &buf[..len];
    match pkt[0] >> 4 {
        4 => parse_v4(pkt),
        6 => parse_v6(pkt),
        _ => None,
    }
}

fn parse_v4(pkt: &[u8]) -> Option<PacketInfo> {
    let ihl = ((pkt[0] & 0x0f) as usize) * 4;
    if ihl < 20 || pkt.len() < ihl {
        return None;
    }
    let protocol = pkt[9];
    let src = Ipv4Addr::new(pkt[12], pkt[13], pkt[14], pkt[15]).into();
    let dst = Ipv4Addr::new(pkt[16], pkt[17], pkt[18], pkt[19]).into();
    let frag_offset = u16::from_be_bytes([pkt[6], pkt[7]]) & 0x1fff;
    let transport = if frag_offset == 0 {
        Some(&pkt[ihl..])
    } else {
        None
    };
    Some(finish(pkt.len(), protocol, src, dst, transport))
}

fn parse_v6(pkt: &[u8]) -> Option<PacketInfo> {
    let src = Ipv6Addr::from(<[u8; 16]>::try_from(pkt.get(8..24)?).ok()?).into();
    let dst = Ipv6Addr::from(<[u8; 16]>::try_from(pkt.get(24..40)?).ok()?).into();
    let mut next = pkt[6];
    let mut off = 40;
    let mut first_fragment = true;
    // Walk the common extension headers; give up after a few.
    for _ in 0..8 {
        match next {
            0 | 43 | 60 => {
                let hdr = pkt.get(off..off + 2)?;
                next = hdr[0];
                off += (hdr[1] as usize + 1) * 8;
            }
            44 => {
                let hdr = pkt.get(off..off + 8)?;
                next = hdr[0];
                first_fragment = u16::from_be_bytes([hdr[2], hdr[3]]) >> 3 == 0;
                off += 8;
            }
            _ => break,
        }
    }
    let transport = if first_fragment { pkt.get(off..) } else { None };
    Some(finish(pkt.len(), next, src, dst, transport))
}

fn finish(
    len: usize,
    protocol: u8,
    src: IpAddr,
    dst: IpAddr,
    transport: Option<&[u8]>,
) -> PacketInfo {
    let mut info = PacketInfo {
        len,
        protocol,
        src,
        dst,
        src_port: 0,
        dst_port: 0,
        tcp_payload_len: None,
    };
    let Some(t) = transport else { return info };
    let need = match protocol {
        PROTO_TCP => 20,
        PROTO_UDP => 8,
        _ => return info,
    };
    if t.len() < need {
        return info;
    }
    info.src_port = u16::from_be_bytes([t[0], t[1]]);
    info.dst_port = u16::from_be_bytes([t[2], t[3]]);
    if protocol == PROTO_TCP {
        let data_off = ((t[12] >> 4) as usize) * 4;
        info.tcp_payload_len = Some(t.len().saturating_sub(data_off));
    }
    info
}

/// Traffic that never leaves the local network (PLAN.md §6.2: LAN exempt).
pub fn is_local_network(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(a) => {
            a.is_private()
                || a.is_link_local()
                || a.is_loopback()
                || a.is_multicast()
                || a.is_broadcast()
        }
        IpAddr::V6(a) => match a.to_ipv4_mapped() {
            Some(v4) => is_local_network(v4.into()),
            None => {
                a.is_loopback()
                    || a.is_unicast_link_local()
                    || a.is_unique_local()
                    || a.is_multicast()
            }
        },
    }
}

/// Collapse IPv4-mapped IPv6 (`::ffff:a.b.c.d`) to plain IPv4 so the same
/// connection gets one key no matter which API reported it.
pub fn canonical(addr: IpAddr) -> IpAddr {
    match addr {
        IpAddr::V6(a) => a.to_ipv4_mapped().map_or(addr, IpAddr::V4),
        v4 => v4,
    }
}

/// Set ECN "Congestion Experienced" on the IP packet at the start of `buf`,
/// if it is ECN-capable (ECT(0) or ECT(1)) — CoDel's gentler alternative to a
/// drop (PLAN.md §9.4). Returns false for Not-ECT or unparseable packets,
/// which the caller should drop instead. The IPv4 header checksum is patched
/// incrementally (RFC 1624), so it stays valid.
pub fn set_ce(buf: &mut [u8]) -> bool {
    const CE: u8 = 0b11;
    if packet_len(buf).is_none() {
        return false;
    }
    match buf[0] >> 4 {
        4 => {
            if buf.len() < 20 {
                return false;
            }
            let ecn = buf[1] & 0b11;
            if ecn == 0 {
                return false;
            }
            if ecn == CE {
                return true;
            }
            let old = u16::from_be_bytes([buf[0], buf[1]]);
            buf[1] |= CE;
            let new = u16::from_be_bytes([buf[0], buf[1]]);
            // HC' = ~(~HC + ~m + m')  (RFC 1624, eqn. 3)
            let hc = u16::from_be_bytes([buf[10], buf[11]]);
            let mut sum = (!hc) as u32 + (!old) as u32 + new as u32;
            while sum > 0xffff {
                sum = (sum & 0xffff) + (sum >> 16);
            }
            buf[10..12].copy_from_slice(&(!(sum as u16)).to_be_bytes());
            true
        }
        6 => {
            // Traffic class straddles bytes 0–1; ECN is bits 4–5 of byte 1.
            let ecn = (buf[1] >> 4) & 0b11;
            if ecn == 0 {
                return false;
            }
            buf[1] |= CE << 4;
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Full IPv4 header checksum, for checking the incremental update.
    fn v4_checksum(h: &[u8]) -> u16 {
        let mut sum: u32 = h[..20]
            .chunks(2)
            .enumerate()
            .filter(|&(i, _)| i != 5)
            .map(|(_, w)| u16::from_be_bytes([w[0], w[1]]) as u32)
            .sum();
        while sum > 0xffff {
            sum = (sum & 0xffff) + (sum >> 16);
        }
        !(sum as u16)
    }

    #[test]
    fn set_ce_ipv4_keeps_checksum_valid() {
        for ect in [0b01u8, 0b10] {
            let mut p = v4_tcp(10);
            p[1] = 0xb8 | ect; // DSCP EF + ECT
            let c = v4_checksum(&p);
            p[10..12].copy_from_slice(&c.to_be_bytes());
            assert!(set_ce(&mut p));
            assert_eq!(p[1] & 0b11, 0b11);
            assert_eq!(p[1] & !0b11, 0xb8, "DSCP must be untouched");
            assert_eq!(u16::from_be_bytes([p[10], p[11]]), v4_checksum(&p));
        }
    }

    #[test]
    fn set_ce_refuses_not_ect() {
        let mut p = v4_tcp(10);
        assert!(!set_ce(&mut p));
        assert_eq!(p, v4_tcp(10));
        let mut q = v6_udp();
        assert!(!set_ce(&mut q));
        assert!(!set_ce(&mut [0x45, 0x01]));
    }

    #[test]
    fn set_ce_ipv6() {
        let mut p = v6_udp();
        p[1] = 0x2a; // TC low nibble ECT(0), flow label bits 0xa
        assert!(set_ce(&mut p));
        assert_eq!(p[1], 0x3a);
        assert_eq!(p[0], 0x60);
    }

    fn v4_tcp(payload: usize) -> Vec<u8> {
        let total = 20 + 20 + payload;
        let mut p = vec![0u8; total];
        p[0] = 0x45;
        p[2..4].copy_from_slice(&(total as u16).to_be_bytes());
        p[9] = PROTO_TCP;
        p[12..16].copy_from_slice(&[93, 184, 216, 34]);
        p[16..20].copy_from_slice(&[192, 168, 1, 10]);
        p[20..22].copy_from_slice(&443u16.to_be_bytes());
        p[22..24].copy_from_slice(&50_000u16.to_be_bytes());
        p[32] = 5 << 4;
        p
    }

    fn v6_udp() -> Vec<u8> {
        let mut p = vec![0u8; 40 + 8 + 4];
        p[0] = 0x60;
        p[4..6].copy_from_slice(&12u16.to_be_bytes());
        p[6] = PROTO_UDP;
        p[8] = 0x20; // 2000::1
        p[23] = 1;
        p[24] = 0x20; // 2000::2
        p[39] = 2;
        p[40..42].copy_from_slice(&443u16.to_be_bytes());
        p[42..44].copy_from_slice(&60_000u16.to_be_bytes());
        p
    }

    #[test]
    fn parses_ipv4_tcp() {
        let info = parse(&v4_tcp(100)).unwrap();
        assert_eq!(info.len, 140);
        assert_eq!(info.src, IpAddr::from([93, 184, 216, 34]));
        assert_eq!((info.src_port, info.dst_port), (443, 50_000));
        assert_eq!(info.tcp_payload_len, Some(100));
    }

    #[test]
    fn pure_ack_has_zero_payload() {
        assert_eq!(parse(&v4_tcp(0)).unwrap().tcp_payload_len, Some(0));
    }

    #[test]
    fn parses_ipv6_udp() {
        let info = parse(&v6_udp()).unwrap();
        assert_eq!(info.protocol, PROTO_UDP);
        assert_eq!((info.src_port, info.dst_port), (443, 60_000));
        assert_eq!(info.tcp_payload_len, None);
    }

    #[test]
    fn splits_a_batch() {
        let mut batch = v4_tcp(10);
        batch.extend(v6_udp());
        let first = packet_len(&batch).unwrap();
        assert_eq!(first, 50);
        assert_eq!(packet_len(&batch[first..]), Some(52));
    }

    #[test]
    fn non_first_fragment_has_no_ports() {
        let mut p = v4_tcp(10);
        p[6..8].copy_from_slice(&100u16.to_be_bytes());
        let info = parse(&p).unwrap();
        assert_eq!((info.src_port, info.dst_port), (0, 0));
    }

    #[test]
    fn rejects_garbage_without_panicking() {
        for len in 0..64 {
            for fill in [0x00, 0x45, 0x60, 0xff] {
                let buf = vec![fill; len];
                let _ = parse(&buf);
                let _ = packet_len(&buf);
            }
        }
        // Claimed length longer than the buffer.
        let mut p = v4_tcp(10);
        p[2..4].copy_from_slice(&9_000u16.to_be_bytes());
        assert_eq!(parse(&p), None);
        // IHL pointing past the end.
        let mut p = v4_tcp(0);
        p[0] = 0x4f;
        let _ = parse(&p);
    }

    #[test]
    fn local_network_detection() {
        assert!(is_local_network([192, 168, 1, 1].into()));
        assert!(is_local_network([10, 0, 0, 1].into()));
        assert!(!is_local_network([8, 8, 8, 8].into()));
        let mapped: IpAddr = Ipv4Addr::new(192, 168, 0, 5).to_ipv6_mapped().into();
        assert!(is_local_network(mapped));
        assert!(!is_local_network("2606:4700::1111".parse().unwrap()));
        assert!(is_local_network("fe80::1".parse().unwrap()));
    }

    #[test]
    fn canonical_unmaps_v4() {
        let mapped: IpAddr = Ipv4Addr::new(1, 2, 3, 4).to_ipv6_mapped().into();
        assert_eq!(canonical(mapped), IpAddr::from([1, 2, 3, 4]));
    }
}
