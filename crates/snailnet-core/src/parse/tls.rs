//! TLS ClientHello reader: the server name (SNI) a client asks for, and
//! whether it used Encrypted Client Hello (PLAN.md §8.1, §8.4).
//!
//! Two entry points:
//! - [`from_tcp`] reads the start of a TCP stream (TLS records). A modern
//!   ClientHello with a post-quantum key share is ~1.8 KB, more than one
//!   segment, so it answers [`Hello::NeedMore`] until the caller has
//!   appended enough segments (it should stop at [`MAX_HELLO`] bytes).
//! - [`from_handshake`] reads a bare handshake message, which is how QUIC
//!   carries it in CRYPTO frames.
//!
//! Only names are read; nothing is decrypted here. Malformed input is
//! [`Hello::NotHello`], never a panic.

/// Give up on a ClientHello larger than this (TCP: bytes of stream to buffer).
pub const MAX_HELLO: usize = 16 * 1024;

const CONTENT_HANDSHAKE: u8 = 22;
const HANDSHAKE_CLIENT_HELLO: u8 = 1;
const EXT_SERVER_NAME: u16 = 0;
const EXT_ECH: u16 = 0xfe0d;
const NAME_TYPE_HOST: u8 = 0;
/// Records may be up to 2^14 bytes of plaintext; allow a little slack.
const MAX_RECORD: usize = 16 * 1024 + 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hello {
    Complete(ClientHello),
    /// Looks like a ClientHello so far but is cut off: append the next
    /// segment and ask again.
    NeedMore,
    /// Not a ClientHello, or malformed.
    NotHello,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClientHello {
    /// The host name, lowercased, without a trailing dot. `None` when the
    /// extension is absent or its name is not a valid host name.
    pub sni: Option<String>,
    /// The client sent `encrypted_client_hello`: `sni` is then the public
    /// outer name (e.g. `cloudflare-ech.com`), not the real site.
    pub ech: bool,
}

/// Read a ClientHello from the first bytes of a TCP stream.
pub fn from_tcp(stream: &[u8]) -> Hello {
    // Fast path: the whole handshake message is in the first record.
    let Some((first, rest)) = record(stream) else {
        return match record_prefix_ok(stream) {
            true => Hello::NeedMore,
            false => Hello::NotHello,
        };
    };
    match handshake_len(first) {
        None if first.len() >= 4 => return Hello::NotHello,
        Some(n) if 4 + n <= first.len() => return from_handshake(&first[..4 + n]),
        Some(n) if 4 + n > MAX_HELLO => return Hello::NotHello,
        _ => {}
    }
    // Slow path: the handshake spans records; join their fragments.
    let mut msg = first.to_vec();
    let mut rest = rest;
    loop {
        if let Some(n) = handshake_len(&msg) {
            if 4 + n <= msg.len() {
                return from_handshake(&msg[..4 + n]);
            }
            if 4 + n > MAX_HELLO {
                return Hello::NotHello;
            }
        } else if msg.len() >= 4 {
            return Hello::NotHello;
        }
        match record(rest) {
            Some((frag, next)) => {
                msg.extend_from_slice(frag);
                rest = next;
            }
            None if record_prefix_ok(rest) => return Hello::NeedMore,
            None => return Hello::NotHello,
        }
    }
}

/// Read a bare handshake message (type, 24-bit length, body).
pub fn from_handshake(msg: &[u8]) -> Hello {
    match handshake_len(msg) {
        Some(n) if 4 + n <= msg.len() => match client_hello(&msg[4..4 + n]) {
            Some(hello) => Hello::Complete(hello),
            None => Hello::NotHello,
        },
        Some(n) if 4 + n <= MAX_HELLO => Hello::NeedMore,
        Some(_) => Hello::NotHello,
        None if msg.len() < 4 && msg.first().is_none_or(|&t| t == HANDSHAKE_CLIENT_HELLO) => {
            Hello::NeedMore
        }
        None => Hello::NotHello,
    }
}

/// Length of a ClientHello handshake message's body, if `msg` starts with one.
fn handshake_len(msg: &[u8]) -> Option<usize> {
    let h = msg.get(..4)?;
    (h[0] == HANDSHAKE_CLIENT_HELLO)
        .then(|| usize::from_be_bytes([0, 0, 0, 0, 0, h[1], h[2], h[3]]))
}

/// One complete handshake record at the start of `buf`: (fragment, rest).
fn record(buf: &[u8]) -> Option<(&[u8], &[u8])> {
    let h = buf.get(..5)?;
    if !header_ok(h) {
        return None;
    }
    let len = u16::from_be_bytes([h[3], h[4]]) as usize;
    let frag = buf.get(5..5 + len)?;
    Some((frag, &buf[5 + len..]))
}

/// The bytes so far could be the start of a handshake record.
fn record_prefix_ok(buf: &[u8]) -> bool {
    match buf.len() {
        0 => true,
        1..5 => {
            let mut h = [CONTENT_HANDSHAKE, 3, 1, 0, 1];
            h[..buf.len()].copy_from_slice(buf);
            header_ok(&h)
        }
        _ => header_ok(&buf[..5]),
    }
}

fn header_ok(h: &[u8]) -> bool {
    let len = u16::from_be_bytes([h[3], h[4]]) as usize;
    h[0] == CONTENT_HANDSHAKE && h[1] == 3 && h[2] <= 4 && (1..=MAX_RECORD).contains(&len)
}

/// Parse a ClientHello body. `None` if malformed.
fn client_hello(body: &[u8]) -> Option<ClientHello> {
    let mut r = Reader(body);
    let version = r.u16()?;
    if version >> 8 != 3 {
        return None;
    }
    r.take(32)?; // random
    let session_id = r.vec8()?;
    if session_id.len() > 32 {
        return None;
    }
    let suites = r.vec16()?;
    if suites.is_empty() || suites.len() % 2 != 0 {
        return None;
    }
    if r.vec8()?.is_empty() {
        return None; // compression methods
    }
    let mut hello = ClientHello::default();
    if r.0.is_empty() {
        return Some(hello); // no extensions at all
    }
    let mut exts = Reader(r.vec16()?);
    let mut seen_sni = false;
    while !exts.0.is_empty() {
        let ty = exts.u16()?;
        let data = exts.vec16()?;
        match ty {
            EXT_SERVER_NAME if !seen_sni => {
                seen_sni = true;
                hello.sni = server_name(data)?;
            }
            EXT_SERVER_NAME => return None, // duplicate
            EXT_ECH => hello.ech = true,
            _ => {}
        }
    }
    Some(hello)
}

/// The `server_name` extension: the first `host_name` entry, if valid.
/// `None` if the extension itself is malformed.
fn server_name(data: &[u8]) -> Option<Option<String>> {
    let mut r = Reader(data);
    let mut list = Reader(r.vec16()?);
    if !r.0.is_empty() {
        return None;
    }
    while !list.0.is_empty() {
        let ty = list.u8()?;
        let name = list.vec16()?;
        if ty == NAME_TYPE_HOST {
            return Some(host_name(name));
        }
    }
    Some(None)
}

/// A lowercased host name, if `raw` is one: letters, digits, `-`, `_`, `.`;
/// no empty labels; at most 253 bytes (a trailing dot is dropped).
pub fn host_name(raw: &[u8]) -> Option<String> {
    let raw = raw.strip_suffix(b".").unwrap_or(raw);
    let ok = (1..=253).contains(&raw.len())
        && raw
            .split(|&b| b == b'.')
            .all(|label| (1..=63).contains(&label.len()))
        && raw
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    ok.then(|| {
        raw.to_ascii_lowercase()
            .into_iter()
            .map(char::from)
            .collect()
    })
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (head, tail) = self.0.split_at_checked(n)?;
        self.0 = tail;
        Some(head)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        let b = self.take(2)?;
        Some(u16::from_be_bytes([b[0], b[1]]))
    }
    fn vec8(&mut self) -> Option<&'a [u8]> {
        let n = self.u8()? as usize;
        self.take(n)
    }
    fn vec16(&mut self) -> Option<&'a [u8]> {
        let n = self.u16()? as usize;
        self.take(n)
    }
}

/// Builds ClientHellos for tests (here and in the QUIC reader).
#[cfg(test)]
pub(crate) mod build {
    pub struct Spec<'a> {
        pub sni: Option<&'a [u8]>,
        pub ech: bool,
        /// Extra padding extension bytes, to make the hello large.
        pub pad: usize,
    }

    /// The handshake message: type 1, 24-bit length, body.
    pub fn handshake(spec: &Spec) -> Vec<u8> {
        let mut exts = Vec::new();
        ext(&mut exts, 0x002b, &[2, 3, 4]); // supported_versions
        if let Some(name) = spec.sni {
            let mut entry = vec![0u8];
            entry.extend_from_slice(&(name.len() as u16).to_be_bytes());
            entry.extend_from_slice(name);
            let mut list = (entry.len() as u16).to_be_bytes().to_vec();
            list.extend_from_slice(&entry);
            ext(&mut exts, 0, &list);
        }
        if spec.ech {
            ext(&mut exts, 0xfe0d, &[0; 40]);
        }
        if spec.pad > 0 {
            ext(&mut exts, 0x0015, &vec![0; spec.pad]);
        }
        let mut body = vec![3, 3];
        body.extend_from_slice(&[7; 32]);
        body.push(32);
        body.extend_from_slice(&[9; 32]);
        body.extend_from_slice(&[0, 4, 0x13, 0x01, 0x13, 0x02]);
        body.extend_from_slice(&[1, 0]);
        body.extend_from_slice(&(exts.len() as u16).to_be_bytes());
        body.extend_from_slice(&exts);
        let mut msg = vec![1];
        msg.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
        msg.extend_from_slice(&body);
        msg
    }

    /// `msg` split into TLS records of at most `max` bytes each.
    pub fn records(msg: &[u8], max: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for chunk in msg.chunks(max) {
            out.extend_from_slice(&[22, 3, 1]);
            out.extend_from_slice(&(chunk.len() as u16).to_be_bytes());
            out.extend_from_slice(chunk);
        }
        out
    }

    fn ext(out: &mut Vec<u8>, ty: u16, data: &[u8]) {
        out.extend_from_slice(&ty.to_be_bytes());
        out.extend_from_slice(&(data.len() as u16).to_be_bytes());
        out.extend_from_slice(data);
    }
}

#[cfg(test)]
mod tests {
    use super::build::{Spec, handshake, records};
    use super::*;
    use proptest::prelude::*;

    fn hello(sni: &str, ech: bool) -> Hello {
        Hello::Complete(ClientHello {
            sni: Some(sni.into()),
            ech,
        })
    }

    #[test]
    fn reads_sni_from_one_record() {
        let msg = handshake(&Spec {
            sni: Some(b"WWW.YouTube.com."),
            ech: false,
            pad: 0,
        });
        assert_eq!(
            from_tcp(&records(&msg, 16384)),
            hello("www.youtube.com", false)
        );
        assert_eq!(from_handshake(&msg), hello("www.youtube.com", false));
    }

    #[test]
    fn flags_ech() {
        let msg = handshake(&Spec {
            sni: Some(b"cloudflare-ech.com"),
            ech: true,
            pad: 0,
        });
        assert_eq!(
            from_tcp(&records(&msg, 16384)),
            hello("cloudflare-ech.com", true)
        );
    }

    #[test]
    fn no_sni_extension() {
        let msg = handshake(&Spec {
            sni: None,
            ech: false,
            pad: 0,
        });
        assert_eq!(
            from_handshake(&msg),
            Hello::Complete(ClientHello::default())
        );
    }

    #[test]
    fn a_large_hello_across_segments_needs_more_until_complete() {
        // ~1.8 KB, like Chrome's with an ML-KEM key share, split at a
        // 1460-byte MSS.
        let stream = records(
            &handshake(&Spec {
                sni: Some(b"rr3---sn-4g5e6nsz.googlevideo.com"),
                ech: false,
                pad: 1600,
            }),
            16384,
        );
        assert!(stream.len() > 1460);
        assert_eq!(from_tcp(&stream[..1460]), Hello::NeedMore);
        assert_eq!(
            from_tcp(&stream),
            hello("rr3---sn-4g5e6nsz.googlevideo.com", false)
        );
    }

    #[test]
    fn a_hello_split_across_records_is_joined() {
        let msg = handshake(&Spec {
            sni: Some(b"twitch.tv"),
            ech: false,
            pad: 300,
        });
        let stream = records(&msg, 100);
        for cut in 0..stream.len() {
            assert_eq!(from_tcp(&stream[..cut]), Hello::NeedMore, "cut at {cut}");
        }
        assert_eq!(from_tcp(&stream), hello("twitch.tv", false));
    }

    #[test]
    fn rejects_other_traffic() {
        assert_eq!(from_tcp(b"GET / HTTP/1.1\r\n"), Hello::NotHello);
        assert_eq!(from_tcp(&[23, 3, 3, 0, 10]), Hello::NotHello); // app data
        assert_eq!(from_tcp(&[22, 3, 3, 0, 4, 2, 0, 0, 0]), Hello::NotHello); // ServerHello
        assert_eq!(from_handshake(&[2, 0, 0, 0]), Hello::NotHello);
    }

    #[test]
    fn an_oversized_hello_is_given_up() {
        let mut msg = vec![1];
        msg.extend_from_slice(&(MAX_HELLO as u32).to_be_bytes()[1..]);
        assert_eq!(from_handshake(&msg), Hello::NotHello);
        assert_eq!(from_tcp(&records(&msg, 16384)), Hello::NotHello);
    }

    #[test]
    fn invalid_names_are_dropped_but_the_hello_still_parses() {
        for bad in [
            &b""[..],
            b"a..b",
            b"exa mple.com",
            b"\xc3\xa9.com",
            &[b'a'; 64],
        ] {
            let msg = handshake(&Spec {
                sni: Some(bad),
                ech: false,
                pad: 0,
            });
            assert_eq!(
                from_handshake(&msg),
                Hello::Complete(ClientHello::default()),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn host_name_rules() {
        assert_eq!(host_name(b"Example.COM").as_deref(), Some("example.com"));
        assert_eq!(host_name(b"_dmarc.x.y").as_deref(), Some("_dmarc.x.y"));
        assert_eq!(host_name(b"."), None);
        assert_eq!(host_name(b".a"), None);
        assert_eq!(host_name(&[b'a'; 254]), None);
    }

    proptest! {
        #[test]
        fn never_panics_on_garbage(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
            let _ = from_tcp(&bytes);
            let _ = from_handshake(&bytes);
        }

        #[test]
        fn never_panics_on_a_corrupted_hello(
            pad in 0usize..2000,
            flips in proptest::collection::vec((any::<usize>(), any::<u8>()), 1..8),
            cut in any::<usize>(),
        ) {
            let mut stream = records(&handshake(&Spec { sni: Some(b"example.com"), ech: false, pad }), 512);
            for (at, v) in flips {
                let i = at % stream.len();
                stream[i] ^= v;
            }
            let cut = cut % (stream.len() + 1);
            let _ = from_tcp(&stream[..cut]);
        }

        #[test]
        fn finds_any_valid_name_at_any_record_size(
            name in "[a-z0-9]{1,20}(\\.[a-z0-9-]{1,20}){0,4}",
            pad in 0usize..3000,
            max in 16usize..16384,
        ) {
            let stream = records(&handshake(&Spec { sni: Some(name.as_bytes()), ech: false, pad }), max);
            prop_assert_eq!(from_tcp(&stream), hello(&name, false));
        }
    }
}
