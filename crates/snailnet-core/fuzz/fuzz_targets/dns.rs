//! `parse::dns`: any message. Never panics, never loops on compression
//! pointers; names it returns are valid lowercase host names.
#![no_main]

use libfuzzer_sys::fuzz_target;
use snailnet_core::parse::{dns, tls};

fuzz_target!(|data: &[u8]| {
    if let Some(res) = dns::answers(data) {
        for name in &res.names {
            assert_eq!(tls::host_name(name.as_bytes()).as_deref(), Some(&**name));
        }
    }
});
