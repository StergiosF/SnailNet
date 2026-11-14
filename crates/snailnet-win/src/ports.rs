//! Owner PID by local port, from the IP Helper tables.
//!
//! WinDivert's FLOW layer only reports flows that start after SnailNet opens
//! it. Connections that were already open (a running Steam download) are
//! attributed from this snapshot instead (PLAN.md §6.3, step 3).

use std::collections::HashMap;

use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, NO_ERROR};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, GetExtendedUdpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID,
    MIB_UDP6ROW_OWNER_PID, MIB_UDPROW_OWNER_PID, TCP_TABLE_OWNER_PID_ALL, UDP_TABLE_OWNER_PID,
};
use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};

pub const PROTO_TCP: u8 = 6;
pub const PROTO_UDP: u8 = 17;

/// `(protocol, local port) → owning PID`.
pub type PortOwners = HashMap<(u8, u16), u32>;

/// Read all four tables (TCP/UDP × v4/v6). Errors on individual tables are
/// skipped: a partial snapshot is still useful.
pub fn snapshot() -> PortOwners {
    let mut map = PortOwners::new();
    if let Some(rows) = read_table::<MIB_TCPROW_OWNER_PID>(Table::Tcp, AF_INET as u32) {
        map.extend(
            rows.iter()
                .map(|r| ((PROTO_TCP, port(r.dwLocalPort)), r.dwOwningPid)),
        );
    }
    if let Some(rows) = read_table::<MIB_TCP6ROW_OWNER_PID>(Table::Tcp, AF_INET6 as u32) {
        map.extend(
            rows.iter()
                .map(|r| ((PROTO_TCP, port(r.dwLocalPort)), r.dwOwningPid)),
        );
    }
    if let Some(rows) = read_table::<MIB_UDPROW_OWNER_PID>(Table::Udp, AF_INET as u32) {
        map.extend(
            rows.iter()
                .map(|r| ((PROTO_UDP, port(r.dwLocalPort)), r.dwOwningPid)),
        );
    }
    if let Some(rows) = read_table::<MIB_UDP6ROW_OWNER_PID>(Table::Udp, AF_INET6 as u32) {
        map.extend(
            rows.iter()
                .map(|r| ((PROTO_UDP, port(r.dwLocalPort)), r.dwOwningPid)),
        );
    }
    map
}

/// Ports are stored in network byte order in the low 16 bits.
fn port(raw: u32) -> u16 {
    u16::from_be(raw as u16)
}

#[derive(Clone, Copy)]
enum Table {
    Tcp,
    Udp,
}

/// Fetch one table and copy its rows out. Every MIB_*TABLE_OWNER_PID is
/// `{ dwNumEntries: u32, table: [Row; N] }` with the rows aligned after it.
fn read_table<Row: Copy>(table: Table, family: u32) -> Option<Vec<Row>> {
    let mut size: u32 = 0;
    // u64 storage keeps the rows (which hold u32s) correctly aligned.
    let mut buf: Vec<u64> = Vec::new();
    for _ in 0..4 {
        let ptr = buf.as_mut_ptr().cast();
        // SAFETY: `ptr` is valid for `size` bytes (both start at 0, then the
        // buffer is grown to the size the API asked for).
        let rc = unsafe {
            match table {
                Table::Tcp => {
                    GetExtendedTcpTable(ptr, &mut size, 0, family, TCP_TABLE_OWNER_PID_ALL, 0)
                }
                Table::Udp => {
                    GetExtendedUdpTable(ptr, &mut size, 0, family, UDP_TABLE_OWNER_PID, 0)
                }
            }
        };
        if rc == ERROR_INSUFFICIENT_BUFFER {
            // Tables can grow between calls; leave some headroom.
            buf = vec![0u64; (size as usize + 4096).div_ceil(8)];
            size = (buf.len() * 8) as u32;
            continue;
        }
        if rc != NO_ERROR {
            return None;
        }
        if size < 4 {
            return Some(Vec::new());
        }
        let bytes = buf.as_ptr().cast::<u8>();
        // SAFETY: the API wrote a valid table into `buf`.
        let count = unsafe { bytes.cast::<u32>().read() } as usize;
        let offset = std::mem::align_of::<Row>().max(4);
        let available = (size as usize).saturating_sub(offset) / std::mem::size_of::<Row>();
        let count = count.min(available);
        // SAFETY: `count` rows fit inside the bytes the API reported, the
        // first row starts at `offset`, and `buf` is 8-byte aligned.
        let rows = unsafe { std::slice::from_raw_parts(bytes.add(offset).cast::<Row>(), count) };
        return Some(rows.to_vec());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, UdpSocket};

    #[test]
    fn finds_our_own_sockets() {
        let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
        let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
        let map = snapshot();
        let me = std::process::id();
        assert_eq!(
            map.get(&(PROTO_TCP, tcp.local_addr().unwrap().port())),
            Some(&me)
        );
        assert_eq!(
            map.get(&(PROTO_UDP, udp.local_addr().unwrap().port())),
            Some(&me)
        );
    }
}
