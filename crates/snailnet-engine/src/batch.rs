//! Buffers for batched receive and send (WinDivertRecvEx / SendEx).

use crate::Address;
use snailnet_core::parse::ip::packet_len;

/// WINDIVERT_BATCH_MAX is 255; 64 balances syscall savings against latency.
pub const DEFAULT_BATCH: usize = 64;
/// WINDIVERT_MTU_MAX: the largest packet WinDivert can hand us.
pub const MTU_MAX: usize = 40 + 0xFFFF;

pub struct RecvBatch {
    pub(crate) buf: Vec<u8>,
    pub(crate) addrs: Vec<Address>,
    /// (offset, len) of each received packet in `buf`.
    spans: Vec<(usize, usize)>,
    /// Addresses received. Equals `spans.len()` at the NETWORK layer; FLOW
    /// and SOCKET events carry an address but no packet bytes.
    addr_count: usize,
}

impl RecvBatch {
    pub fn new(max_packets: usize) -> Self {
        Self {
            // Room for one maximum-size packet plus typical MTU-size ones.
            buf: vec![0; MTU_MAX + max_packets * 1600],
            addrs: vec![Address::default(); max_packets],
            spans: Vec::with_capacity(max_packets),
            addr_count: 0,
        }
    }

    /// Record what the engine wrote: `bytes` of packet data, `count`
    /// addresses. Splits the buffer on IP header lengths. Returns the number
    /// of packets.
    pub fn fill(&mut self, bytes: usize, count: usize) -> usize {
        self.addr_count = count.min(self.addrs.len());
        self.spans.clear();
        let mut off = 0;
        let bytes = bytes.min(self.buf.len());
        while self.spans.len() < count.min(self.addrs.len()) && off < bytes {
            let Some(len) = packet_len(&self.buf[off..bytes]) else {
                break;
            };
            self.spans.push((off, len));
            off += len;
        }
        self.spans.len()
    }

    pub fn len(&self) -> usize {
        self.spans.len()
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// Every address received, including packet-less FLOW/SOCKET events.
    pub fn addresses(&self) -> &[Address] {
        &self.addrs[..self.addr_count]
    }

    pub fn iter(&self) -> impl Iterator<Item = (&[u8], &Address)> {
        self.spans
            .iter()
            .zip(&self.addrs)
            .map(|(&(o, l), a)| (&self.buf[o..o + l], a))
    }

    /// Raw buffers for an engine to receive into.
    pub fn raw_mut(&mut self) -> (&mut [u8], &mut [Address]) {
        (&mut self.buf, &mut self.addrs)
    }
}

pub struct SendBatch {
    pub(crate) buf: Vec<u8>,
    pub(crate) addrs: Vec<Address>,
    max_packets: usize,
}

impl SendBatch {
    pub fn new(max_packets: usize) -> Self {
        Self {
            buf: Vec::with_capacity(max_packets * 1600),
            addrs: Vec::with_capacity(max_packets),
            max_packets,
        }
    }

    pub fn push(&mut self, packet: &[u8], addr: &Address) {
        self.buf.extend_from_slice(packet);
        self.addrs.push(*addr);
    }

    pub fn is_full(&self) -> bool {
        self.addrs.len() >= self.max_packets
    }

    pub fn is_empty(&self) -> bool {
        self.addrs.is_empty()
    }

    pub fn len(&self) -> usize {
        self.addrs.len()
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.addrs.clear();
    }

    pub fn raw(&self) -> (&[u8], &[Address]) {
        (&self.buf, &self.addrs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4(total: u16) -> Vec<u8> {
        let mut p = vec![0u8; total as usize];
        p[0] = 0x45;
        p[2..4].copy_from_slice(&total.to_be_bytes());
        p
    }

    #[test]
    fn splits_received_bytes_into_packets() {
        let mut b = RecvBatch::new(4);
        let (a, c) = (v4(60), v4(1500));
        b.buf[..60].copy_from_slice(&a);
        b.buf[60..1560].copy_from_slice(&c);
        assert_eq!(b.fill(1560, 2), 2);
        let lens: Vec<_> = b.iter().map(|(p, _)| p.len()).collect();
        assert_eq!(lens, [60, 1500]);
    }

    #[test]
    fn stops_at_garbage() {
        let mut b = RecvBatch::new(4);
        b.buf[..60].copy_from_slice(&v4(60));
        b.buf[60] = 0xff;
        assert_eq!(b.fill(200, 3), 1);
    }
}
