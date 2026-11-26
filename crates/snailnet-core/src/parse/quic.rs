//! QUIC Initial reader: opens a client's Initial packet and returns the
//! CRYPTO data (the TLS ClientHello) inside, for [`super::tls::from_handshake`]
//! (PLAN.md §8.1).
//!
//! Initial packets are encrypted with keys derived from a **public** salt and
//! the packet's own Destination Connection ID (RFC 9001 §5.2, RFC 9369 §3.3
//! for v2), so anyone on the path can open them. Nothing else is decrypted.
//!
//! Browsers spread the ClientHello over several CRYPTO frames, out of order,
//! and a post-quantum hello needs two Initial packets. [`CryptoAssembler`]
//! joins the pieces; feed it every Initial of a flow until
//! [`CryptoAssembler::prefix`] parses as a complete hello.

use aes::cipher::{BlockCipherEncrypt, KeyInit};
use aes_gcm::{AeadInOut, Aes128Gcm};
use hkdf::Hkdf;
use sha2::Sha256;

use super::tls::MAX_HELLO;

pub const VERSION_1: u32 = 0x0000_0001;
pub const VERSION_2: u32 = 0x6b33_43cf;

const SALT_V1: [u8; 20] = [
    0x38, 0x76, 0x2c, 0xf7, 0xf5, 0x59, 0x34, 0xb3, 0x4d, 0x17, 0x9a, 0xe6, 0xa4, 0xc8, 0x0c, 0xad,
    0xcc, 0xbb, 0x7f, 0x0a,
];
const SALT_V2: [u8; 20] = [
    0x0d, 0xed, 0xe3, 0xde, 0xf7, 0x00, 0xa6, 0xdb, 0x81, 0x93, 0x81, 0xbe, 0x6e, 0x26, 0x9d, 0xcb,
    0xf9, 0xbd, 0x2e, 0xd9,
];
const TAG_LEN: usize = 16;
const SAMPLE_LEN: usize = 16;
const MAX_CID: usize = 20;

/// The opened payload of one client Initial packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Initial {
    pub version: u32,
    pub dcid: Vec<u8>,
    /// CRYPTO frames as (stream offset, data), in packet order.
    pub crypto: Vec<(u64, Vec<u8>)>,
}

/// Open the first packet of a UDP datagram if it is a client Initial of a
/// QUIC version we know. `None` for anything else, including a bad tag.
pub fn open_initial(datagram: &[u8]) -> Option<Initial> {
    let mut r = Cursor {
        buf: datagram,
        pos: 0,
    };
    let first = r.u8()?;
    if first & 0xc0 != 0xc0 {
        return None; // short header, or fixed bit clear
    }
    let version = u32::from_be_bytes(r.take(4)?.try_into().ok()?);
    let initial_type = match version {
        VERSION_1 => 0,
        VERSION_2 => 1,
        _ => return None,
    };
    if (first >> 4) & 0x03 != initial_type {
        return None;
    }
    let dcid = r.vec8()?;
    // A client's first DCID is at least 8 bytes (RFC 9000 §7.2).
    if !(8..=MAX_CID).contains(&dcid.len()) || r.vec8()?.len() > MAX_CID {
        return None;
    }
    let token_len = usize::try_from(r.varint()?).ok()?;
    r.take(token_len)?;
    let length = usize::try_from(r.varint()?).ok()?;
    let pn_offset = r.pos;
    let end = pn_offset.checked_add(length)?;
    if end > datagram.len() || length < 4 + SAMPLE_LEN {
        return None;
    }

    let keys = Keys::client_initial(version, dcid)?;
    let mut pkt = datagram[..end].to_vec();

    // Remove header protection (RFC 9001 §5.4).
    let sample: [u8; SAMPLE_LEN] = pkt[pn_offset + 4..pn_offset + 4 + SAMPLE_LEN]
        .try_into()
        .ok()?;
    let mask = keys.hp_mask(sample);
    pkt[0] ^= mask[0] & 0x0f;
    let pn_len = (pkt[0] & 0x03) as usize + 1;
    let mut pn = 0u64;
    for i in 0..pn_len {
        pkt[pn_offset + i] ^= mask[1 + i];
        pn = pn << 8 | u64::from(pkt[pn_offset + i]);
    }

    // Decrypt the payload (RFC 9001 §5.3): AAD is the unprotected header.
    let payload_start = pn_offset + pn_len;
    if end - payload_start < TAG_LEN {
        return None;
    }
    let (header, rest) = pkt.split_at_mut(payload_start);
    let (payload, tag) = rest.split_at_mut(rest.len() - TAG_LEN);
    let mut nonce = keys.iv;
    for (n, p) in nonce[4..].iter_mut().zip(pn.to_be_bytes()) {
        *n ^= p;
    }
    let tag: [u8; TAG_LEN] = (*tag).try_into().ok()?;
    let aead = Aes128Gcm::new_from_slice(&keys.key).ok()?;
    aead.decrypt_inout_detached(&nonce.into(), header, payload.into(), &tag.into())
        .ok()?;

    Some(Initial {
        version,
        dcid: dcid.to_vec(),
        crypto: crypto_frames(payload)?,
    })
}

/// The CRYPTO frames of a decrypted Initial payload. `None` on a frame that
/// may not appear in an Initial packet, or a malformed one.
fn crypto_frames(payload: &[u8]) -> Option<Vec<(u64, Vec<u8>)>> {
    let mut r = Cursor {
        buf: payload,
        pos: 0,
    };
    let mut out = Vec::new();
    while r.pos < payload.len() {
        match r.varint()? {
            0x00 | 0x01 => {} // PADDING, PING
            ty @ (0x02 | 0x03) => {
                // ACK: largest, delay, range count, first range, ranges.
                r.varint()?;
                r.varint()?;
                let ranges = r.varint()?;
                r.varint()?;
                for _ in 0..ranges {
                    r.varint()?;
                    r.varint()?;
                }
                if ty == 0x03 {
                    for _ in 0..3 {
                        r.varint()?; // ECN counts
                    }
                }
            }
            0x06 => {
                let offset = r.varint()?;
                let len = usize::try_from(r.varint()?).ok()?;
                out.push((offset, r.take(len)?.to_vec()));
            }
            0x1c => {
                // CONNECTION_CLOSE: error, frame type, reason.
                r.varint()?;
                r.varint()?;
                let len = usize::try_from(r.varint()?).ok()?;
                r.take(len)?;
            }
            _ => return None,
        }
    }
    Some(out)
}

/// Client Initial keys for one connection.
struct Keys {
    key: [u8; 16],
    iv: [u8; 12],
    hp: [u8; 16],
}

impl Keys {
    fn client_initial(version: u32, dcid: &[u8]) -> Option<Keys> {
        let (salt, key_label, iv_label, hp_label): (&[u8], &[u8], &[u8], &[u8]) = match version {
            VERSION_1 => (
                &SALT_V1,
                b"tls13 quic key",
                b"tls13 quic iv",
                b"tls13 quic hp",
            ),
            VERSION_2 => (
                &SALT_V2,
                b"tls13 quicv2 key",
                b"tls13 quicv2 iv",
                b"tls13 quicv2 hp",
            ),
            _ => return None,
        };
        let (initial_secret, _) = Hkdf::<Sha256>::extract(Some(salt), dcid);
        let mut client = [0u8; 32];
        expand_label(&initial_secret, b"tls13 client in", &mut client)?;
        let mut keys = Keys {
            key: [0; 16],
            iv: [0; 12],
            hp: [0; 16],
        };
        expand_label(&client, key_label, &mut keys.key)?;
        expand_label(&client, iv_label, &mut keys.iv)?;
        expand_label(&client, hp_label, &mut keys.hp)?;
        Some(keys)
    }

    /// AES-ECB of the sample under the header-protection key.
    fn hp_mask(&self, sample: [u8; SAMPLE_LEN]) -> [u8; SAMPLE_LEN] {
        let cipher = aes::Aes128::new(&self.hp.into());
        let mut block = sample.into();
        cipher.encrypt_block(&mut block);
        block.into()
    }
}

/// TLS 1.3 HKDF-Expand-Label with an empty context (RFC 8446 §7.1).
/// `full_label` already carries the `"tls13 "` prefix.
fn expand_label(secret: &[u8], full_label: &[u8], out: &mut [u8]) -> Option<()> {
    let mut info = [0u8; 32];
    let total = 4 + full_label.len();
    if total > info.len() {
        return None;
    }
    let len = u16::try_from(out.len()).ok()?.to_be_bytes();
    info[0] = len[0];
    info[1] = len[1];
    info[2] = u8::try_from(full_label.len()).ok()?;
    info[3..3 + full_label.len()].copy_from_slice(full_label);
    info[3 + full_label.len()] = 0;
    Hkdf::<Sha256>::from_prk(secret)
        .ok()?
        .expand(&info[..total], out)
        .ok()
}

/// Joins CRYPTO frames (possibly out of order, overlapping, or spread over
/// several packets) into the contiguous stream from offset 0. Bytes past
/// [`MAX_HELLO`] are ignored.
#[derive(Debug, Default)]
pub struct CryptoAssembler {
    buf: Vec<u8>,
    /// Sorted, non-overlapping, non-adjacent filled ranges.
    filled: Vec<(usize, usize)>,
}

impl CryptoAssembler {
    pub fn push(&mut self, offset: u64, data: &[u8]) {
        let Ok(start) = usize::try_from(offset) else {
            return;
        };
        let end = start.saturating_add(data.len()).min(MAX_HELLO);
        if start >= end {
            return;
        }
        if self.buf.len() < end {
            self.buf.resize(end, 0);
        }
        self.buf[start..end].copy_from_slice(&data[..end - start]);
        // Merge (start, end) into the range list.
        let (mut s, mut e) = (start, end);
        self.filled.retain(|&(a, b)| {
            if b < s || a > e {
                true
            } else {
                s = s.min(a);
                e = e.max(b);
                false
            }
        });
        let at = self.filled.partition_point(|&(a, _)| a < s);
        self.filled.insert(at, (s, e));
    }

    /// The bytes received contiguously from offset 0.
    pub fn prefix(&self) -> &[u8] {
        match self.filled.first() {
            Some(&(0, end)) => &self.buf[..end],
            _ => &[],
        }
    }
}

struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.buf.get(self.pos..self.pos.checked_add(n)?)?;
        self.pos += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn vec8(&mut self) -> Option<&'a [u8]> {
        let n = self.u8()? as usize;
        self.take(n)
    }
    /// QUIC variable-length integer (RFC 9000 §16).
    fn varint(&mut self) -> Option<u64> {
        let first = self.u8()?;
        let len = 1usize << (first >> 6);
        let mut v = u64::from(first & 0x3f);
        for &b in self.take(len - 1)? {
            v = v << 8 | u64::from(b);
        }
        Some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tls::{self, ClientHello, Hello, build};
    use super::*;
    use proptest::prelude::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    const RFC_DCID: &str = "8394c8f03e515708";

    /// RFC 9001 Appendix A.1.
    #[test]
    fn v1_keys_match_the_rfc() {
        let k = Keys::client_initial(VERSION_1, &hex(RFC_DCID)).unwrap();
        assert_eq!(k.key.to_vec(), hex("1f369613dd76d5467730efcbe3b1a22d"));
        assert_eq!(k.iv.to_vec(), hex("fa044b2f42a3fd3b46fb255c"));
        assert_eq!(k.hp.to_vec(), hex("9f50449e04a0e810283a1e9933adedd2"));
    }

    /// RFC 9369 Appendix A.1.
    #[test]
    fn v2_keys_match_the_rfc() {
        let k = Keys::client_initial(VERSION_2, &hex(RFC_DCID)).unwrap();
        assert_eq!(k.key.to_vec(), hex("8b1a0bc121284290a29e0971b5cd045d"));
        assert_eq!(k.iv.to_vec(), hex("91f73e2351d8fa91660e909f"));
        assert_eq!(k.hp.to_vec(), hex("45b95e15235d6f45a6b19cbcb0294ba9"));
    }

    /// RFC 9001 Appendix A.2: the header-protection mask from its sample.
    #[test]
    fn v1_header_mask_matches_the_rfc() {
        let k = Keys::client_initial(VERSION_1, &hex(RFC_DCID)).unwrap();
        let sample: [u8; 16] = hex("d1b1c98dd7689fb8ec11d242b123dc9b").try_into().unwrap();
        assert_eq!(k.hp_mask(sample)[..5].to_vec(), hex("437b9aec36"));
    }

    /// Build a protected client Initial the way a client does (the
    /// inverse of `open_initial`), padded to 1200 bytes.
    fn seal(version: u32, dcid: &[u8], pn: u32, frames: &[u8]) -> Vec<u8> {
        let ty = if version == VERSION_2 { 1 } else { 0 };
        let pn_len = 4;
        let mut payload = frames.to_vec();
        let mut header = vec![0xc0 | ty << 4 | (pn_len as u8 - 1)];
        header.extend_from_slice(&version.to_be_bytes());
        header.push(dcid.len() as u8);
        header.extend_from_slice(dcid);
        header.push(0); // SCID
        header.push(0); // token length
        let fixed = header.len() + 2 + pn_len + TAG_LEN;
        if fixed + payload.len() < 1200 {
            payload.resize(1200 - fixed, 0); // PADDING frames
        }
        let length = (pn_len + payload.len() + TAG_LEN) as u16;
        header.extend_from_slice(&(0x4000 | length).to_be_bytes());
        let pn_offset = header.len();
        header.extend_from_slice(&pn.to_be_bytes());

        let keys = Keys::client_initial(version, dcid).unwrap();
        let mut nonce = keys.iv;
        for (n, p) in nonce[4..].iter_mut().zip(u64::from(pn).to_be_bytes()) {
            *n ^= p;
        }
        let aead = Aes128Gcm::new_from_slice(&keys.key).unwrap();
        let tag = aead
            .encrypt_inout_detached(&nonce.into(), &header, payload.as_mut_slice().into())
            .unwrap();
        let mut pkt = header;
        pkt.extend_from_slice(&payload);
        pkt.extend_from_slice(&tag);
        let sample: [u8; 16] = pkt[pn_offset + 4..pn_offset + 20].try_into().unwrap();
        let mask = keys.hp_mask(sample);
        pkt[0] ^= mask[0] & 0x0f;
        for i in 0..pn_len {
            pkt[pn_offset + i] ^= mask[1 + i];
        }
        pkt
    }

    fn crypto_frame(offset: u64, data: &[u8]) -> Vec<u8> {
        let mut f = vec![0x06];
        f.extend_from_slice(&(0x8000_0000u32 | offset as u32).to_be_bytes());
        f.extend_from_slice(&(0x4000u16 | data.len() as u16).to_be_bytes());
        f.extend_from_slice(data);
        f
    }

    fn sni_of(asm: &CryptoAssembler) -> Hello {
        tls::from_handshake(asm.prefix())
    }

    #[test]
    fn opens_a_v1_and_a_v2_initial_and_reads_the_sni() {
        let hello = build::handshake(&build::Spec {
            sni: Some(b"www.youtube.com"),
            ech: false,
            pad: 0,
        });
        for version in [VERSION_1, VERSION_2] {
            let pkt = seal(version, &hex(RFC_DCID), 2, &crypto_frame(0, &hello));
            let init = open_initial(&pkt).expect("opens");
            assert_eq!(init.version, version);
            let mut asm = CryptoAssembler::default();
            for (off, data) in &init.crypto {
                asm.push(*off, data);
            }
            assert_eq!(
                sni_of(&asm),
                Hello::Complete(ClientHello {
                    sni: Some("www.youtube.com".into()),
                    ech: false
                })
            );
        }
    }

    #[test]
    fn a_shuffled_hello_over_two_packets_is_joined() {
        // Like Chrome: a ~1.9 KB hello, split into out-of-order CRYPTO
        // frames with PING/PADDING between, over two Initials.
        let hello = build::handshake(&build::Spec {
            sni: Some(b"rr1---sn-ab5l6nrz.googlevideo.com"),
            ech: false,
            pad: 1700,
        });
        let dcid = hex("0011223344556677aabb");
        let cut = [0, 300, 700, 1000, 1500, hello.len()];
        let piece = |i: usize| crypto_frame(cut[i] as u64, &hello[cut[i]..cut[i + 1]]);
        let mut first = piece(2);
        first.extend_from_slice(&[0x01, 0x00, 0x00]);
        first.extend_from_slice(&piece(0));
        let mut second = piece(4);
        second.push(0x01);
        second.extend_from_slice(&piece(3));
        second.extend_from_slice(&piece(1));

        let mut asm = CryptoAssembler::default();
        for (off, data) in open_initial(&seal(VERSION_1, &dcid, 0, &first))
            .unwrap()
            .crypto
        {
            asm.push(off, &data);
        }
        assert_eq!(sni_of(&asm), Hello::NeedMore);
        for (off, data) in open_initial(&seal(VERSION_1, &dcid, 1, &second))
            .unwrap()
            .crypto
        {
            asm.push(off, &data);
        }
        assert_eq!(asm.prefix(), &hello[..]);
        assert!(
            matches!(sni_of(&asm), Hello::Complete(h) if h.sni.as_deref() == Some("rr1---sn-ab5l6nrz.googlevideo.com"))
        );
    }

    #[test]
    fn rejects_tampering_and_other_packets() {
        let hello = build::handshake(&build::Spec {
            sni: Some(b"example.com"),
            ech: false,
            pad: 0,
        });
        let pkt = seal(VERSION_1, &hex(RFC_DCID), 0, &crypto_frame(0, &hello));
        let mut bad = pkt.clone();
        bad[600] ^= 1;
        assert_eq!(open_initial(&bad), None, "tag check");
        let mut other_version = pkt.clone();
        other_version[4] = 0x02;
        assert_eq!(open_initial(&other_version), None);
        let mut handshake_type = pkt.clone();
        handshake_type[0] = (handshake_type[0] & 0xcf) | 0x20;
        assert_eq!(open_initial(&handshake_type), None);
        assert_eq!(open_initial(&pkt[..pkt.len() - 1]), None, "truncated");
        assert_eq!(open_initial(&[0x40; 1200]), None, "short header");
        assert!(open_initial(&pkt).is_some());
    }

    #[test]
    fn assembler_handles_overlap_gaps_and_the_cap() {
        let mut a = CryptoAssembler::default();
        a.push(5, b"fghij");
        assert_eq!(a.prefix(), b"");
        a.push(0, b"abc");
        assert_eq!(a.prefix(), b"abc");
        a.push(2, b"cde");
        assert_eq!(a.prefix(), b"abcdefghij");
        a.push(u64::MAX, b"x");
        a.push(MAX_HELLO as u64 - 1, b"yz");
        assert_eq!(a.prefix().len(), 10);
        assert_eq!(a.buf.len(), MAX_HELLO);
    }

    proptest! {
        #[test]
        fn never_panics_on_garbage(bytes in proptest::collection::vec(any::<u8>(), 0..1500)) {
            let _ = open_initial(&bytes);
        }

        #[test]
        fn never_panics_on_a_corrupted_header(
            flips in proptest::collection::vec((0usize..64, any::<u8>()), 1..6),
            cut in 0usize..1300,
        ) {
            let hello = build::handshake(&build::Spec { sni: Some(b"example.com"), ech: false, pad: 0 });
            let mut pkt = seal(VERSION_1, &hex(RFC_DCID), 0, &crypto_frame(0, &hello));
            for (at, v) in flips {
                pkt[at] ^= v;
            }
            let _ = open_initial(&pkt[..cut.min(pkt.len())]);
        }

        #[test]
        fn assembler_matches_a_reference(
            pieces in proptest::collection::vec((0usize..200, 0usize..60), 0..30),
        ) {
            let truth: Vec<u8> = (0..300u32).map(|i| (i * 7) as u8).collect();
            let mut have = vec![false; 300];
            let mut a = CryptoAssembler::default();
            for (start, len) in pieces {
                let end = (start + len).min(300);
                a.push(start as u64, &truth[start..end]);
                have[start..end].iter_mut().for_each(|h| *h = true);
            }
            let want = have.iter().take_while(|&&h| h).count();
            prop_assert_eq!(a.prefix(), &truth[..want]);
        }
    }
}
