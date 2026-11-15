//! `parse::tls`: any bytes, as a TCP stream start and as a bare handshake
//! message. Never panics; a name it returns is a valid lowercase host name.
#![no_main]

use libfuzzer_sys::fuzz_target;
use snailnet_core::parse::tls::{self, Hello, MAX_HELLO};

fn check(h: Hello) {
    if let Hello::Complete(c) = h
        && let Some(sni) = c.sni
    {
        assert_eq!(tls::host_name(sni.as_bytes()).as_deref(), Some(&*sni));
    }
}

fuzz_target!(|data: &[u8]| {
    check(tls::from_tcp(data));
    check(tls::from_handshake(data));
    // The way `sniff` feeds TCP: growing prefixes, one segment at a time.
    let mut buf = Vec::new();
    for seg in data.chunks(1400) {
        if buf.len() + seg.len() > MAX_HELLO {
            break;
        }
        buf.extend_from_slice(seg);
        check(tls::from_tcp(&buf));
    }
});
