//! `parse::quic`: any datagram through `open_initial`, and any CRYPTO frame
//! layout through `CryptoAssembler` into `tls::from_handshake`. Never
//! panics; the assembled prefix stays within `MAX_HELLO`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use snailnet_core::parse::quic::{CryptoAssembler, open_initial};
use snailnet_core::parse::tls::{self, MAX_HELLO};

fuzz_target!(|data: &[u8]| {
    if let Some(init) = open_initial(data) {
        let mut asm = CryptoAssembler::default();
        for (off, frame) in &init.crypto {
            asm.push(*off, frame);
        }
        let _ = tls::from_handshake(asm.prefix());
    }
    // Frames as [offset: u16 BE][len: u8][bytes], in input order (so any
    // order, overlap or gap).
    let mut asm = CryptoAssembler::default();
    let mut rest = data;
    while let [hi, lo, len, tail @ ..] = rest {
        let n = usize::from(*len).min(tail.len());
        let (frame, next) = tail.split_at(n);
        asm.push(u64::from(u16::from_be_bytes([*hi, *lo])), frame);
        rest = next;
    }
    assert!(asm.prefix().len() <= MAX_HELLO);
    let _ = tls::from_handshake(asm.prefix());
});
