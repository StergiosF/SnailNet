//! ICMP echo for the line-speed estimator's latency probe (PLAN.md §10.2):
//! one tiny ping to the default gateway and one to 1.1.1.1, once a second
//! while the line is busy. The packet engine only captures TCP and UDP, so
//! these skip SnailNet's own queues and see just the router's and ISP's.
//! IPv4 only, and no admin rights needed.

use std::net::Ipv4Addr;
use std::time::Duration;

use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE, NO_ERROR};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetBestRoute, ICMP_ECHO_REPLY, IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, MIB_IPFORWARDROW,
};

/// IP_SUCCESS in an echo reply's status.
const IP_SUCCESS: u32 = 0;
/// What each echo carries: a few bytes, enough to be a normal ping.
const PAYLOAD: [u8; 8] = *b"snailnet";

/// An open ICMP handle; closed on drop.
pub struct Pinger(HANDLE);

// SAFETY: an ICMP handle may be used from any thread (one call at a time,
// which `&mut self` on `ping` ensures).
unsafe impl Send for Pinger {}

impl Pinger {
    pub fn new() -> std::io::Result<Self> {
        // SAFETY: no arguments; the result is checked.
        let h = unsafe { IcmpCreateFile() };
        if h == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self(h))
    }

    /// One echo to `to`; its round trip, or `None` on timeout or error.
    pub fn ping(&mut self, to: Ipv4Addr, timeout: Duration) -> Option<Duration> {
        // A reply holds the header, our payload and room for an ICMP error.
        let mut reply = [0u8; size_of::<ICMP_ECHO_REPLY>() + PAYLOAD.len() + 8];
        let ms = timeout.as_millis().clamp(1, u32::MAX as u128) as u32;
        // SAFETY: the handle is open; the request and reply buffers are valid
        // for the stated lengths for the whole (blocking) call.
        let n = unsafe {
            IcmpSendEcho(
                self.0,
                u32::from_ne_bytes(to.octets()),
                PAYLOAD.as_ptr().cast(),
                PAYLOAD.len() as u16,
                std::ptr::null(),
                reply.as_mut_ptr().cast(),
                reply.len() as u32,
                ms,
            )
        };
        if n == 0 {
            return None;
        }
        // SAFETY: n ≥ 1 means the buffer starts with a filled ICMP_ECHO_REPLY;
        // read unaligned since a byte array has no alignment guarantee.
        let r = unsafe { std::ptr::read_unaligned(reply.as_ptr().cast::<ICMP_ECHO_REPLY>()) };
        (r.Status == IP_SUCCESS).then(|| Duration::from_millis(u64::from(r.RoundTripTime)))
    }
}

impl Drop for Pinger {
    fn drop(&mut self) {
        // SAFETY: the handle came from IcmpCreateFile and is closed once.
        unsafe { IcmpCloseHandle(self.0) };
    }
}

/// The default gateway towards `dest` (the router), if there is one.
pub fn gateway_towards(dest: Ipv4Addr) -> Option<Ipv4Addr> {
    // SAFETY: MIB_IPFORWARDROW is plain data; all-zero is a valid value.
    let mut row: MIB_IPFORWARDROW = unsafe { std::mem::zeroed() };
    // SAFETY: `row` is a valid out pointer for the call.
    let rc = unsafe { GetBestRoute(u32::from_ne_bytes(dest.octets()), 0, &mut row) };
    if rc != NO_ERROR {
        return None;
    }
    let hop = Ipv4Addr::from(row.dwForwardNextHop.to_ne_bytes());
    // An on-link destination has itself (or 0.0.0.0) as the next hop.
    (!hop.is_unspecified() && hop != dest).then_some(hop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pings_this_pc() {
        let mut p = Pinger::new().unwrap();
        let rtt = p.ping(Ipv4Addr::LOCALHOST, Duration::from_secs(1));
        assert!(
            rtt.is_some_and(|d| d < Duration::from_millis(100)),
            "{rtt:?}"
        );
    }

    #[test]
    fn an_unreachable_host_times_out() {
        let mut p = Pinger::new().unwrap();
        // TEST-NET-1: reserved, never routed.
        assert_eq!(
            p.ping(Ipv4Addr::new(192, 0, 2, 1), Duration::from_millis(200)),
            None
        );
    }

    #[test]
    fn a_gateway_is_private_when_found() {
        if let Some(gw) = gateway_towards(Ipv4Addr::new(1, 1, 1, 1)) {
            // Private, link-local or CGNAT (100.64.0.0/10).
            let cgnat = gw.octets()[0] == 100 && (gw.octets()[1] & 0xC0) == 64;
            assert!(gw.is_private() || gw.is_link_local() || cgnat, "{gw}");
        }
    }
}
