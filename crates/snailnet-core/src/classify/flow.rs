//! Which process owns a packet's connection (PLAN.md §6.3 step 3).
//!
//! The NETWORK layer, where packets are shaped, carries no process ID, so
//! the owner comes from the other layers, best first:
//!
//! 1. **FLOW** events: exact 5-tuple → process, for every flow established
//!    after SnailNet started.
//! 2. **SOCKET** events: `connect`/`accept` give the exact 5-tuple *before*
//!    the first packet; `bind`/`listen` give the local port (UDP sockets
//!    often only ever bind).
//! 3. **Port snapshot** (IP Helper tables, refreshed about once a second):
//!    covers connections that were already open when SnailNet started.

use std::collections::HashMap;
use std::net::IpAddr;

use crate::parse::ip::{PROTO_TCP, PROTO_UDP, PacketInfo, canonical};

/// A connection as seen from this machine. Both directions of a flow map to
/// the same key; IPv4-mapped IPv6 addresses are collapsed to IPv4.
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

/// Interned app identity (see [`super::Apps`]).
pub type AppId = u32;

/// Who owns a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owner {
    pub pid: u32,
    pub app: AppId,
}

/// Which layer told us the owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Flow,
    Socket,
    Port,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    owner: Owner,
    source: Source,
    /// WinDivert endpoint id for SOCKET entries, so `close` removes only
    /// what that socket added.
    endpoint: u64,
    /// Consecutive port snapshots without this entry's local port.
    misses: u8,
}

/// An entry whose local port is missing from this many port snapshots in
/// a row is dead (its delete/close event never matched) and is dropped.
/// The 24 h soak (2026-11-15) found ~1000 such FLOW entries after 6 h
/// against ~130 live connections.
pub const STALE_AFTER_SNAPSHOTS: u8 = 3;

/// `(protocol, local port)`.
pub type PortKey = (u8, u16);

/// Hard caps. Entries normally leave on FLOW-deleted / socket-close; the
/// caps only guard against a missed event storm. Past them, new entries
/// are skipped (lookups fall back to the next source) and counted.
pub const MAX_FLOWS: usize = 1 << 17;
pub const MAX_ENDPOINTS: usize = 1 << 16;

#[derive(Debug, Default)]
pub struct FlowTable {
    flows: HashMap<FlowKey, Entry>,
    endpoints: HashMap<PortKey, Entry>,
    ports: HashMap<PortKey, Owner>,
    overflow: u64,
}

impl FlowTable {
    /// FLOW established: authoritative, replaces any SOCKET guess.
    pub fn flow_established(&mut self, key: FlowKey, owner: Owner) {
        self.insert_flow(key, owner, Source::Flow, 0);
    }

    pub fn flow_deleted(&mut self, key: &FlowKey) {
        self.flows.remove(key);
    }

    /// SOCKET connect / accept: the exact flow, before its first packet.
    /// Never overrides a FLOW entry.
    pub fn socket_connected(&mut self, key: FlowKey, owner: Owner, endpoint: u64) {
        if self
            .flows
            .get(&key)
            .is_some_and(|e| e.source == Source::Flow)
        {
            return;
        }
        self.insert_flow(key, owner, Source::Socket, endpoint);
    }

    /// SOCKET bind / listen: only the local port is known.
    pub fn socket_bound(&mut self, port: PortKey, owner: Owner, endpoint: u64) {
        if self.endpoints.len() >= MAX_ENDPOINTS && !self.endpoints.contains_key(&port) {
            self.overflow += 1;
            return;
        }
        let entry = Entry {
            owner,
            source: Source::Socket,
            endpoint,
            misses: 0,
        };
        self.endpoints.insert(port, entry);
    }

    /// SOCKET close: forget what this endpoint told us. `key.remote` may be
    /// unspecified (a socket that never connected).
    pub fn socket_closed(&mut self, key: &FlowKey, endpoint: u64) {
        let ours = |e: &Entry| e.source == Source::Socket && e.endpoint == endpoint;
        if self.flows.get(key).is_some_and(ours) {
            self.flows.remove(key);
        }
        let port = (key.protocol, key.local.1);
        if self.endpoints.get(&port).is_some_and(ours) {
            self.endpoints.remove(&port);
        }
    }

    /// Replace the port snapshot, and drop flow and endpoint entries whose
    /// local port has been gone for [`STALE_AFTER_SNAPSHOTS`] snapshots.
    /// A protocol with no rows at all is left alone (its table read may
    /// have failed).
    pub fn set_ports(&mut self, ports: HashMap<PortKey, Owner>) {
        let has_rows = |proto: u8| ports.keys().any(|&(p, _)| p == proto);
        let (tcp, udp) = (has_rows(PROTO_TCP), has_rows(PROTO_UDP));
        let checked = |proto: u8| (proto == PROTO_TCP && tcp) || (proto == PROTO_UDP && udp);
        let age = |port: PortKey, e: &mut Entry| {
            if !checked(port.0) || ports.contains_key(&port) {
                e.misses = 0;
                true
            } else {
                e.misses = e.misses.saturating_add(1);
                e.misses < STALE_AFTER_SNAPSHOTS
            }
        };
        self.flows.retain(|k, e| age((k.protocol, k.local.1), e));
        self.endpoints.retain(|&port, e| age(port, e));
        self.ports = ports;
    }

    pub fn ports(&self) -> &HashMap<PortKey, Owner> {
        &self.ports
    }

    pub fn lookup(&self, key: &FlowKey) -> Option<(Owner, Source)> {
        if let Some(e) = self.flows.get(key) {
            return Some((e.owner, e.source));
        }
        let port = (key.protocol, key.local.1);
        if let Some(e) = self.endpoints.get(&port) {
            return Some((e.owner, Source::Socket));
        }
        self.ports.get(&port).map(|&o| (o, Source::Port))
    }

    /// (flows, socket endpoints, snapshot ports, skipped because full).
    pub fn sizes(&self) -> (usize, usize, usize, u64) {
        (
            self.flows.len(),
            self.endpoints.len(),
            self.ports.len(),
            self.overflow,
        )
    }

    fn insert_flow(&mut self, key: FlowKey, owner: Owner, source: Source, endpoint: u64) {
        if self.flows.len() >= MAX_FLOWS && !self.flows.contains_key(&key) {
            self.overflow += 1;
            return;
        }
        self.flows.insert(
            key,
            Entry {
                owner,
                source,
                endpoint,
                misses: 0,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    const ME: [u8; 4] = [192, 168, 1, 5];
    const THEM: [u8; 4] = [1, 1, 1, 1];

    fn packet(outbound: bool) -> PacketInfo {
        let (me, them) = (IpAddr::from(ME), IpAddr::from(THEM));
        PacketInfo {
            len: 60,
            protocol: 6,
            src: if outbound { me } else { them },
            dst: if outbound { them } else { me },
            src_port: if outbound { 50_000 } else { 443 },
            dst_port: if outbound { 443 } else { 50_000 },
            tcp_payload_len: Some(0),
            payload_offset: Some(40),
        }
    }

    fn owner(pid: u32) -> Owner {
        Owner { pid, app: pid }
    }

    fn key() -> FlowKey {
        FlowKey::from_packet(&packet(true), true)
    }

    #[test]
    fn both_directions_map_to_one_key() {
        assert_eq!(
            FlowKey::from_packet(&packet(true), true),
            FlowKey::from_packet(&packet(false), false)
        );
    }

    #[test]
    fn sources_in_order_and_mapped_v4_matches() {
        let mut t = FlowTable::default();
        assert_eq!(t.lookup(&key()), None);
        t.set_ports(HashMap::from([((6, 50_000), owner(7))]));
        assert_eq!(t.lookup(&key()), Some((owner(7), Source::Port)));
        t.socket_bound((6, 50_000), owner(8), 1);
        assert_eq!(t.lookup(&key()), Some((owner(8), Source::Socket)));
        let mapped = |a: [u8; 4]| IpAddr::V6(Ipv4Addr::from(a).to_ipv6_mapped());
        let flow = FlowKey::new(6, (mapped(ME), 50_000), (mapped(THEM), 443));
        t.flow_established(flow, owner(42));
        assert_eq!(t.lookup(&key()), Some((owner(42), Source::Flow)));
        t.flow_deleted(&key());
        assert_eq!(t.lookup(&key()), Some((owner(8), Source::Socket)));
    }

    #[test]
    fn connect_is_overridden_by_flow_but_not_the_other_way() {
        let mut t = FlowTable::default();
        t.socket_connected(key(), owner(1), 9);
        assert_eq!(t.lookup(&key()), Some((owner(1), Source::Socket)));
        t.flow_established(key(), owner(2));
        t.socket_connected(key(), owner(3), 10);
        assert_eq!(t.lookup(&key()), Some((owner(2), Source::Flow)));
        // Closing the socket leaves the FLOW entry alone.
        t.socket_closed(&key(), 10);
        assert_eq!(t.lookup(&key()), Some((owner(2), Source::Flow)));
    }

    #[test]
    fn close_removes_only_that_endpoints_entries() {
        let mut t = FlowTable::default();
        t.socket_connected(key(), owner(1), 5);
        t.socket_bound((6, 50_000), owner(1), 5);
        t.socket_closed(&key(), 6); // someone else's socket
        assert!(t.lookup(&key()).is_some());
        t.socket_closed(&key(), 5);
        assert_eq!(t.lookup(&key()), None);
        // A UDP socket that only bound: close comes with no remote address.
        let any = IpAddr::V6(Ipv6Addr::UNSPECIFIED);
        let unconnected = FlowKey::new(17, (any, 5353), (any, 0));
        t.socket_bound((17, 5353), owner(4), 11);
        t.socket_closed(&unconnected, 11);
        assert_eq!(t.sizes(), (0, 0, 0, 0));
    }

    #[test]
    fn caps_skip_and_count() {
        let mut t = FlowTable::default();
        // Every UDP port: exactly MAX_ENDPOINTS entries.
        for port in 0..=u16::MAX {
            t.socket_bound((17, port), owner(1), port.into());
        }
        assert_eq!(t.sizes().1, MAX_ENDPOINTS);
        t.socket_bound((17, 80), owner(2), 0); // an update still lands
        assert_eq!(t.sizes().3, 0);
        t.socket_bound((6, 1), owner(1), 0); // a new entry doesn't
        assert_eq!(t.sizes().3, 1);
    }
    fn key_at(proto: u8, local_port: u16) -> FlowKey {
        FlowKey::new(
            proto,
            (IpAddr::from(ME), local_port),
            (IpAddr::from(THEM), 443),
        )
    }

    #[test]
    fn dead_entries_leave_after_three_snapshots_without_their_port() {
        let mut t = FlowTable::default();
        t.flow_established(key_at(6, 1000), owner(1)); // live
        t.flow_established(key_at(6, 2000), owner(1)); // its delete was missed
        t.socket_connected(key_at(17, 3000), owner(2), 7); // live UDP
        t.socket_bound((17, 4000), owner(2), 8); // closed, close missed
        let live = HashMap::from([((6, 1000), owner(1)), ((17, 3000), owner(2))]);
        for _ in 0..STALE_AFTER_SNAPSHOTS - 1 {
            t.set_ports(live.clone());
        }
        assert_eq!(t.sizes().0, 3, "two misses: still kept");
        // The port comes back once: the count starts over.
        let mut back = live.clone();
        back.insert((6, 2000), owner(1));
        t.set_ports(back);
        for _ in 0..STALE_AFTER_SNAPSHOTS - 1 {
            t.set_ports(live.clone());
        }
        assert!(t.lookup(&key_at(6, 2000)).is_some());
        t.set_ports(live.clone());
        assert!(t.lookup(&key_at(6, 2000)).is_none(), "gone after three");
        assert!(t.lookup(&key_at(6, 1000)).is_some());
        assert!(t.lookup(&key_at(17, 3000)).is_some());
        assert_eq!(t.sizes().1, 0, "the endpoint went too");
    }

    #[test]
    fn a_protocol_missing_from_the_snapshot_is_not_aged() {
        let mut t = FlowTable::default();
        t.flow_established(key_at(6, 1000), owner(1));
        t.flow_established(key_at(17, 1000), owner(1));
        // The TCP table read failed: only UDP rows.
        let udp_only = HashMap::from([((17, 1000), owner(1))]);
        for _ in 0..10 {
            t.set_ports(udp_only.clone());
        }
        assert!(t.lookup(&key_at(6, 1000)).is_some());
        for _ in 0..10 {
            t.set_ports(HashMap::new());
        }
        assert_eq!(t.sizes().0, 2, "an empty snapshot ages nothing");
    }
}
