//! Which process owns a packet (PLAN.md §6.3 step 3, Phase 0 version).
//!
//! 1. FLOW-layer events give an exact 5-tuple → PID map for new flows.
//! 2. A once-a-second IP Helper snapshot covers flows that were already
//!    open when the spike started, keyed by (protocol, local port).

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::RwLock;

use snailnet_core::parse::ip::{PacketInfo, canonical};
use snailnet_win::ports::PortOwners;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FlowKey {
    pub protocol: u8,
    pub local: (IpAddr, u16),
    pub remote: (IpAddr, u16),
}

impl FlowKey {
    pub fn new(protocol: u8, local: (IpAddr, u16), remote: (IpAddr, u16)) -> Self {
        Self {
            protocol,
            local: (canonical(local.0), local.1),
            remote: (canonical(remote.0), remote.1),
        }
    }

    pub fn from_packet(p: &PacketInfo, outbound: bool) -> Self {
        let (src, dst) = ((p.src, p.src_port), (p.dst, p.dst_port));
        if outbound {
            Self::new(p.protocol, src, dst)
        } else {
            Self::new(p.protocol, dst, src)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Flow,
    Port,
}

#[derive(Default)]
pub struct Attribution {
    flows: RwLock<HashMap<FlowKey, u32>>,
    ports: RwLock<PortOwners>,
}

impl Attribution {
    pub fn flow_established(&self, key: FlowKey, pid: u32) {
        self.flows.write().unwrap().insert(key, pid);
    }

    pub fn flow_deleted(&self, key: &FlowKey) {
        self.flows.write().unwrap().remove(key);
    }

    pub fn set_ports(&self, ports: PortOwners) {
        *self.ports.write().unwrap() = ports;
    }

    pub fn flow_count(&self) -> usize {
        self.flows.read().unwrap().len()
    }

    pub fn lookup(&self, key: &FlowKey) -> Option<(u32, Source)> {
        if let Some(&pid) = self.flows.read().unwrap().get(key) {
            return Some((pid, Source::Flow));
        }
        let ports = self.ports.read().unwrap();
        ports
            .get(&(key.protocol, key.local.1))
            .map(|&pid| (pid, Source::Port))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn packet(outbound: bool) -> PacketInfo {
        let (me, them) = (IpAddr::from([192, 168, 1, 5]), IpAddr::from([1, 1, 1, 1]));
        PacketInfo {
            len: 60,
            protocol: 6,
            src: if outbound { me } else { them },
            dst: if outbound { them } else { me },
            src_port: if outbound { 50_000 } else { 443 },
            dst_port: if outbound { 443 } else { 50_000 },
            tcp_payload_len: Some(0),
        }
    }

    #[test]
    fn both_directions_map_to_one_key() {
        assert_eq!(
            FlowKey::from_packet(&packet(true), true),
            FlowKey::from_packet(&packet(false), false)
        );
    }

    #[test]
    fn flow_beats_port_and_mapped_v4_matches() {
        let a = Attribution::default();
        let mapped = IpAddr::V6(Ipv4Addr::new(192, 168, 1, 5).to_ipv6_mapped());
        let remote = IpAddr::V6(Ipv4Addr::new(1, 1, 1, 1).to_ipv6_mapped());
        a.set_ports(HashMap::from([((6, 50_000), 7)]));
        let key = FlowKey::from_packet(&packet(false), false);
        assert_eq!(a.lookup(&key), Some((7, Source::Port)));
        a.flow_established(FlowKey::new(6, (mapped, 50_000), (remote, 443)), 42);
        assert_eq!(a.lookup(&key), Some((42, Source::Flow)));
        a.flow_deleted(&key);
        assert_eq!(a.lookup(&key), Some((7, Source::Port)));
    }
}
