//! Which network this PC is on (PLAN.md §10.3), so each keeps its own line
//! speed. The network is the router the internet route goes through: its
//! hardware address is the id (the same line by Wi-Fi or by cable), and
//! the name is the Wi-Fi name, or the adapter's ("Ethernet") on a cable.
//! IPv4 only, like the ping probe; no admin rights needed.

use std::net::Ipv4Addr;

use snailnet_core::rules::Network;
use windows_sys::Win32::Foundation::{HANDLE, NO_ERROR};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetBestRoute, GetIfEntry2, MIB_IF_ROW2, MIB_IPFORWARDROW, SendARP,
};
use windows_sys::Win32::NetworkManagement::WiFi::{
    WLAN_CONNECTION_ATTRIBUTES, WlanCloseHandle, WlanFreeMemory, WlanOpenHandle,
    WlanQueryInterface, wlan_intf_opcode_current_connection,
};
use windows_sys::core::GUID;

/// Where the internet route leads: 1.1.1.1, as the ping probe uses.
const PROBE: Ipv4Addr = Ipv4Addr::new(1, 1, 1, 1);
/// IF_TYPE_IEEE80211: a Wi-Fi adapter.
const IF_WIFI: u32 = 71;

/// The route to the internet: the router and the adapter that reaches it.
/// Cheap (no packets sent): ask it often, and look closer when it changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Route {
    /// The router, or `None` for an on-link route: a full-tunnel VPN
    /// (WireGuard, wintun) or PPPoE dialled on the PC itself. Online all
    /// the same, but there is no router to tell the line by.
    pub gateway: Option<Ipv4Addr>,
    pub if_index: u32,
}

/// The current route to the internet, or `None` when offline.
pub fn route() -> Option<Route> {
    // SAFETY: MIB_IPFORWARDROW is plain data; all-zero is a valid value.
    let mut row: MIB_IPFORWARDROW = unsafe { std::mem::zeroed() };
    // SAFETY: `row` is a valid out pointer for the call.
    let rc = unsafe { GetBestRoute(u32::from_ne_bytes(PROBE.octets()), 0, &mut row) };
    if rc != NO_ERROR || row.dwForwardIfIndex == 0 {
        return None;
    }
    // An on-link route names no next hop, or the probe itself.
    let hop = Ipv4Addr::from(row.dwForwardNextHop.to_ne_bytes());
    Some(Route {
        gateway: (!hop.is_unspecified() && hop != PROBE).then_some(hop),
        if_index: row.dwForwardIfIndex,
    })
}

/// The network `route` belongs to, or `None` for an on-link route (no
/// router to tell it by; the line stays the last network's). Asks the
/// router for its hardware address (usually already known, else one ARP
/// round trip), so call it off any hot path.
pub fn identify(route: Route) -> Option<Network> {
    let gateway = route.gateway?;
    let adapter = adapter(route.if_index);
    let ssid = adapter
        .as_ref()
        .filter(|a| a.wifi)
        .and_then(|a| wifi_name(&a.guid));
    let id = match router_mac(gateway) {
        Some(mac) => format!("gw-{}", hex(&mac, ":")),
        // No answer from the router: the adapter and router address still
        // tell lines apart, if less surely.
        None => format!(
            "if-{}-{}",
            adapter
                .as_ref()
                .map_or(route.if_index.to_string(), |a| guid_text(&a.guid)),
            gateway
        ),
    };
    let name = ssid
        .or_else(|| adapter.map(|a| a.alias))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| format!("Network at {gateway}"));
    Some(Network { id, name })
}

/// The network the internet route goes through now, if online through a
/// router.
pub fn current() -> Option<Network> {
    route().and_then(identify)
}

struct Adapter {
    guid: GUID,
    alias: String,
    wifi: bool,
}

fn adapter(if_index: u32) -> Option<Adapter> {
    // SAFETY: MIB_IF_ROW2 is plain data; all-zero is a valid value.
    let mut row: MIB_IF_ROW2 = unsafe { std::mem::zeroed() };
    row.InterfaceIndex = if_index;
    // SAFETY: `row` is valid and names the interface by index, as asked.
    if unsafe { GetIfEntry2(&mut row) } != NO_ERROR {
        return None;
    }
    Some(Adapter {
        guid: row.InterfaceGuid,
        alias: wide(&row.Alias),
        wifi: row.Type == IF_WIFI,
    })
}

/// The router's hardware address (6 bytes), by ARP.
fn router_mac(gateway: Ipv4Addr) -> Option<[u8; 6]> {
    let mut buf = [0u8; 8];
    let mut len = buf.len() as u32;
    // SAFETY: `buf` holds `len` bytes; the call writes at most that many.
    let rc = unsafe {
        SendARP(
            u32::from_ne_bytes(gateway.octets()),
            0,
            buf.as_mut_ptr().cast(),
            &mut len,
        )
    };
    if rc != NO_ERROR || len != 6 || buf[..6].iter().all(|b| *b == 0) {
        return None;
    }
    Some([buf[0], buf[1], buf[2], buf[3], buf[4], buf[5]])
}

/// The name (SSID) of the Wi-Fi network adapter `guid` is connected to.
fn wifi_name(guid: &GUID) -> Option<String> {
    let mut version = 0u32;
    let mut h: HANDLE = std::ptr::null_mut();
    // SAFETY: valid out pointers; version 2 is the Vista+ API.
    if unsafe { WlanOpenHandle(2, std::ptr::null(), &mut version, &mut h) } != NO_ERROR {
        return None;
    }
    let mut size = 0u32;
    let mut data: *mut core::ffi::c_void = std::ptr::null_mut();
    // SAFETY: the handle is open; `guid` and the out pointers are valid.
    let rc = unsafe {
        WlanQueryInterface(
            h,
            guid,
            wlan_intf_opcode_current_connection,
            std::ptr::null(),
            &mut size,
            &mut data,
            std::ptr::null_mut(),
        )
    };
    let name = (rc == NO_ERROR
        && !data.is_null()
        && size as usize >= size_of::<WLAN_CONNECTION_ATTRIBUTES>())
    .then(|| {
        // SAFETY: success with a large enough buffer means `data` points
        // to a WLAN_CONNECTION_ATTRIBUTES the API allocated.
        let attrs = unsafe { &*data.cast::<WLAN_CONNECTION_ATTRIBUTES>() };
        let ssid = &attrs.wlanAssociationAttributes.dot11Ssid;
        let n = (ssid.uSSIDLength as usize).min(ssid.ucSSID.len());
        String::from_utf8_lossy(&ssid.ucSSID[..n]).into_owned()
    });
    // SAFETY: `data` came from the API (null is skipped); the handle is
    // closed once.
    unsafe {
        if !data.is_null() {
            WlanFreeMemory(data);
        }
        WlanCloseHandle(h, std::ptr::null());
    }
    name
}

fn wide(s: &[u16]) -> String {
    let n = s.iter().position(|c| *c == 0).unwrap_or(s.len());
    String::from_utf16_lossy(&s[..n])
}

fn hex(bytes: &[u8], sep: &str) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(sep)
}

fn guid_text(g: &GUID) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{}",
        g.data1,
        g.data2,
        g.data3,
        hex(&g.data4, "")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_guid_text_read_as_expected() {
        assert_eq!(hex(&[0, 0x1a, 0xff], ":"), "00:1a:ff");
        let g = GUID {
            data1: 1,
            data2: 2,
            data3: 3,
            data4: [4, 5, 6, 7, 8, 9, 10, 11],
        };
        assert_eq!(guid_text(&g), "00000001-0002-0003-0405060708090a0b");
    }

    #[test]
    fn wide_stops_at_the_first_nul() {
        let mut buf = [0u16; 8];
        for (i, c) in "Wi-Fi".encode_utf16().enumerate() {
            buf[i] = c;
        }
        assert_eq!(wide(&buf), "Wi-Fi");
    }

    /// On a PC with a router, the network has an id and a name.
    #[test]
    fn this_pc_has_a_network_when_online() {
        if let Some(n) = route().and_then(identify) {
            assert!(n.id.starts_with("gw-") || n.id.starts_with("if-"), "{n:?}");
            assert!(!n.name.is_empty());
            println!("{n:?}");
        }
    }
}
