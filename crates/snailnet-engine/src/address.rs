//! `WINDIVERT_ADDRESS` (windivert.h, 2.2.x): per-packet metadata, 80 bytes.

use std::net::{IpAddr, Ipv6Addr};

use snailnet_core::parse::ip::canonical;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Address {
    pub timestamp: i64,
    /// Bit fields: Layer:8, Event:8, Sniffed, Outbound, Loopback, Impostor,
    /// IPv6, IPChecksum, TCPChecksum, UDPChecksum, Reserved1:8 (LSB first).
    bits: u32,
    reserved2: u32,
    /// The layer-specific union (64 bytes, 8-byte aligned).
    data: [u64; 8],
}

const _: () = assert!(std::mem::size_of::<Address>() == 80);

impl std::fmt::Debug for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Address")
            .field("layer", &self.layer())
            .field("event", &self.event())
            .field("outbound", &self.outbound())
            .field("loopback", &self.loopback())
            .field("ipv6", &self.ipv6())
            .finish()
    }
}

/// `WINDIVERT_DATA_FLOW` / `WINDIVERT_DATA_SOCKET` (same layout).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FlowData {
    pub endpoint_id: u64,
    pub parent_endpoint_id: u64,
    pub process_id: u32,
    /// IPv6 in host byte order; IPv4 is IPv4-mapped (`::ffff:a.b.c.d`).
    pub local_addr: [u32; 4],
    pub remote_addr: [u32; 4],
    pub local_port: u16,
    pub remote_port: u16,
    pub protocol: u8,
}

const _: () = assert!(std::mem::size_of::<FlowData>() <= 64);

impl FlowData {
    pub fn local_ip(&self) -> IpAddr {
        words_to_ip(self.local_addr)
    }
    pub fn remote_ip(&self) -> IpAddr {
        words_to_ip(self.remote_addr)
    }
}

/// WinDivert stores a 128-bit address as four host-order words, least
/// significant first.
fn words_to_ip(w: [u32; 4]) -> IpAddr {
    let v = (w[3] as u128) << 96 | (w[2] as u128) << 64 | (w[1] as u128) << 32 | w[0] as u128;
    canonical(IpAddr::V6(Ipv6Addr::from(v)))
}

impl Address {
    pub fn layer(&self) -> u8 {
        self.bits as u8
    }
    pub fn event(&self) -> u8 {
        (self.bits >> 8) as u8
    }
    fn bit(&self, n: u32) -> bool {
        self.bits & (1 << n) != 0
    }
    pub fn sniffed(&self) -> bool {
        self.bit(16)
    }
    pub fn outbound(&self) -> bool {
        self.bit(17)
    }
    pub fn loopback(&self) -> bool {
        self.bit(18)
    }
    pub fn impostor(&self) -> bool {
        self.bit(19)
    }
    pub fn ipv6(&self) -> bool {
        self.bit(20)
    }

    pub fn set_outbound(&mut self, on: bool) {
        self.bits = (self.bits & !(1 << 17)) | ((on as u32) << 17);
    }

    /// Layer data for FLOW and SOCKET events. Meaningless for NETWORK packets.
    pub fn flow(&self) -> FlowData {
        // SAFETY: FlowData is plain-old-data no larger than the 64-byte,
        // 8-byte-aligned union, and any bit pattern is a valid FlowData.
        unsafe { std::ptr::read(self.data.as_ptr().cast::<FlowData>()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_bitfields() {
        let mut a = Address {
            bits: 2 | (1 << 8) | (1 << 17) | (1 << 20),
            ..Default::default()
        };
        assert_eq!(a.layer(), 2);
        assert_eq!(a.event(), 1);
        assert!(a.outbound() && a.ipv6() && !a.loopback() && !a.sniffed());
        a.set_outbound(false);
        assert!(!a.outbound());
        assert_eq!(a.layer(), 2);
    }

    #[test]
    fn decodes_ipv4_mapped_flow_address() {
        // 192.168.1.10 as ::ffff:c0a8:010a in WinDivert's word order.
        let ip = words_to_ip([0xc0a8_010a, 0x0000_ffff, 0, 0]);
        assert_eq!(ip, IpAddr::from([192, 168, 1, 10]));
    }

    #[test]
    fn decodes_ipv6_flow_address() {
        // 2001:db8::1 → most significant word last.
        let ip = words_to_ip([1, 0, 0, 0x2001_0db8]);
        assert_eq!(ip, "2001:db8::1".parse::<IpAddr>().unwrap());
    }
}
