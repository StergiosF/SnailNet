//! Just enough HTTPS for the speed test (PLAN.md §10.1) and the pack feed
//! (§17.2), through Windows' own WinHTTP: system TLS and the system proxy,
//! no extra dependencies. The speed test downloads a body and throws it
//! away, and uploads a body of zeros, each reporting bytes as they move
//! and stopping at a deadline; the pack feed GETs a small body.

use std::ffi::c_void;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use windows_sys::Win32::Networking::WinHttp::{
    INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect,
    WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
    WinHttpSendRequest, WinHttpSetTimeouts, WinHttpWriteData,
};

/// Read and write in pieces of this size, so progress is reported often.
const CHUNK: usize = 64 * 1024;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// A WinHTTP handle, closed on drop.
struct Handle(*mut c_void);

impl Handle {
    fn new(h: *mut c_void) -> io::Result<Self> {
        if h.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(h))
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: a handle WinHTTP gave us, closed once.
        unsafe { WinHttpCloseHandle(self.0) };
    }
}

fn check(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// A connection to one HTTPS host. Safe to share between threads: each
/// call opens its own request.
pub struct Https {
    // Field order: the connection closes before the session.
    conn: Handle,
    _session: Handle,
}

// SAFETY: WinHTTP session and connection handles are thread-safe; each
// request handle stays on the thread that opened it.
unsafe impl Send for Https {}
// SAFETY: as above.
unsafe impl Sync for Https {}

impl Https {
    /// `agent`: the User-Agent; `host`: e.g. `speed.cloudflare.com`.
    pub fn new(agent: &str, host: &str) -> io::Result<Self> {
        Self::with_timeout(agent, host, 10_000)
    }

    /// As [`Self::new`], with a custom WinHTTP per-operation timeout (ms).
    pub fn with_timeout(agent: &str, host: &str, timeout_ms: i32) -> io::Result<Self> {
        let agent = wide(agent);
        let host = wide(host);
        // SAFETY: the strings are NUL-terminated and outlive the calls.
        let session = Handle::new(unsafe {
            WinHttpOpen(
                agent.as_ptr(),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                std::ptr::null(),
                std::ptr::null(),
                0,
            )
        })?;
        // SAFETY: an open session handle.
        check(unsafe {
            WinHttpSetTimeouts(session.0, timeout_ms, timeout_ms, timeout_ms, timeout_ms)
        })?;
        // SAFETY: as above.
        let conn = Handle::new(unsafe {
            WinHttpConnect(session.0, host.as_ptr(), INTERNET_DEFAULT_HTTPS_PORT, 0)
        })?;
        Ok(Self {
            conn,
            _session: session,
        })
    }

    fn request(&self, verb: &str, path: &str) -> io::Result<Handle> {
        let verb = wide(verb);
        let path = wide(path);
        // SAFETY: an open connection; NUL-terminated strings; no accept types.
        Handle::new(unsafe {
            WinHttpOpenRequest(
                self.conn.0,
                verb.as_ptr(),
                path.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            )
        })
    }

    fn status(req: &Handle) -> io::Result<u32> {
        // SAFETY: a request whose body (if any) has been sent.
        check(unsafe { WinHttpReceiveResponse(req.0, std::ptr::null_mut()) })?;
        let mut code = 0u32;
        let mut len = size_of::<u32>() as u32;
        // SAFETY: `code` is a u32 buffer of `len` bytes.
        check(unsafe {
            WinHttpQueryHeaders(
                req.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                std::ptr::null(),
                (&raw mut code).cast(),
                &mut len,
                std::ptr::null_mut(),
            )
        })?;
        Ok(code)
    }

    /// GET `path`, discarding the body, until it ends or `deadline`.
    /// Calls `moved` with each piece's size; returns the bytes read.
    pub fn download(&self, path: &str, deadline: Instant, moved: &dyn Fn(u64)) -> io::Result<u64> {
        self.download_cancellable(path, deadline, &AtomicBool::new(false), moved)
    }

    /// As [`Self::download`], stopping early between chunks when `cancel` is set.
    pub fn download_cancellable(
        &self,
        path: &str,
        deadline: Instant,
        cancel: &AtomicBool,
        moved: &dyn Fn(u64),
    ) -> io::Result<u64> {
        if cancel.load(Ordering::Relaxed) {
            return Ok(0);
        }
        let req = self.request("GET", path)?;
        // SAFETY: an open request; no headers or body.
        check(unsafe {
            WinHttpSendRequest(req.0, std::ptr::null(), 0, std::ptr::null(), 0, 0, 0)
        })?;
        if cancel.load(Ordering::Relaxed) {
            return Ok(0);
        }
        let code = Self::status(&req)?;
        if code != 200 {
            return Err(io::Error::other(format!("HTTP {code}")));
        }
        let mut buf = vec![0u8; CHUNK];
        let mut total = 0u64;
        while Instant::now() < deadline && !cancel.load(Ordering::Relaxed) {
            let mut n = 0u32;
            // SAFETY: `buf` holds `CHUNK` writable bytes.
            check(unsafe {
                WinHttpReadData(req.0, buf.as_mut_ptr().cast(), CHUNK as u32, &mut n)
            })?;
            if n == 0 {
                break;
            }
            total += u64::from(n);
            moved(u64::from(n));
        }
        Ok(total)
    }

    /// GET `path` and return its body, refusing one over `limit` bytes.
    /// Redirects are followed (WinHTTP's default, never HTTPS → HTTP).
    pub fn get(&self, path: &str, limit: usize) -> io::Result<Vec<u8>> {
        let req = self.request("GET", path)?;
        // SAFETY: an open request; no headers or body.
        check(unsafe {
            WinHttpSendRequest(req.0, std::ptr::null(), 0, std::ptr::null(), 0, 0, 0)
        })?;
        let code = Self::status(&req)?;
        if code != 200 {
            return Err(io::Error::other(format!("HTTP {code}")));
        }
        let mut body = Vec::new();
        let mut buf = vec![0u8; CHUNK];
        loop {
            let mut n = 0u32;
            // SAFETY: `buf` holds `CHUNK` writable bytes.
            check(unsafe {
                WinHttpReadData(req.0, buf.as_mut_ptr().cast(), CHUNK as u32, &mut n)
            })?;
            if n == 0 {
                return Ok(body);
            }
            if body.len() + n as usize > limit {
                return Err(io::Error::other(format!("over {limit} bytes")));
            }
            body.extend_from_slice(&buf[..n as usize]);
        }
    }

    /// POST `len` zero bytes to `path`, until done or `deadline` (then the
    /// request is dropped unfinished). Calls `moved` per piece written;
    /// returns the bytes written.
    pub fn upload(
        &self,
        path: &str,
        len: u32,
        deadline: Instant,
        moved: &dyn Fn(u64),
    ) -> io::Result<u64> {
        self.upload_cancellable(path, len, deadline, &AtomicBool::new(false), moved)
    }

    /// As [`Self::upload`], stopping early between chunks when `cancel` is set.
    pub fn upload_cancellable(
        &self,
        path: &str,
        len: u32,
        deadline: Instant,
        cancel: &AtomicBool,
        moved: &dyn Fn(u64),
    ) -> io::Result<u64> {
        if cancel.load(Ordering::Relaxed) {
            return Ok(0);
        }
        let req = self.request("POST", path)?;
        let headers = wide("Content-Type: application/octet-stream");
        // SAFETY: an open request; the header string is NUL-terminated
        // (length -1 = up to the NUL); the body follows with WriteData.
        check(unsafe {
            WinHttpSendRequest(
                req.0,
                headers.as_ptr(),
                u32::MAX,
                std::ptr::null(),
                0,
                len,
                0,
            )
        })?;
        let zeros = vec![0u8; CHUNK];
        let mut left = len as usize;
        let mut total = 0u64;
        while left > 0 {
            if Instant::now() >= deadline || cancel.load(Ordering::Relaxed) {
                return Ok(total);
            }
            let piece = left.min(CHUNK);
            let mut n = 0u32;
            // SAFETY: `zeros` holds at least `piece` readable bytes.
            check(unsafe { WinHttpWriteData(req.0, zeros.as_ptr().cast(), piece as u32, &mut n) })?;
            left -= n as usize;
            total += u64::from(n);
            moved(u64::from(n));
        }
        if cancel.load(Ordering::Relaxed) {
            return Ok(total);
        }
        let code = Self::status(&req)?;
        if !(200..300).contains(&code) {
            return Err(io::Error::other(format!("HTTP {code}")));
        }
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    /// Talks to Cloudflare: run by hand with `--ignored`.
    #[test]
    #[ignore]
    fn downloads_and_uploads_against_cloudflare() {
        let h = Https::new("SnailNet-test", "speed.cloudflare.com").unwrap();
        let seen = AtomicU64::new(0);
        let deadline = Instant::now() + Duration::from_secs(10);
        let got = h
            .download("/__down?bytes=1000000", deadline, &|n| {
                seen.fetch_add(n, Ordering::Relaxed);
            })
            .unwrap();
        assert_eq!(got, 1_000_000);
        assert_eq!(seen.load(Ordering::Relaxed), 1_000_000);
        let sent = h.upload("/__up", 500_000, deadline, &|_| {}).unwrap();
        assert_eq!(sent, 500_000);
    }

    #[test]
    fn a_host_that_does_not_exist_fails() {
        let h = Https::new("SnailNet-test", "nonexistent.invalid").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        assert!(h.download("/", deadline, &|_| {}).is_err());
    }
}
