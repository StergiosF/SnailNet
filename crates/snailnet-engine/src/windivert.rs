//! Thin, dynamically loaded binding to WinDivert 2.2 (PLAN.md §16 allows
//! this instead of a third-party crate; the API surface is tiny).
//!
//! The DLL is loaded at runtime so the rest of the CLI works without it and
//! a missing or blocked driver gives a clear error instead of a loader crash.

use std::ffi::{CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use libloading::os::windows::{LOAD_WITH_ALTERED_SEARCH_PATH, Library};

use crate::{Address, EngineError, PacketEngine, RecvBatch, SendBatch};

type Handle = *mut c_void;
const INVALID_HANDLE: Handle = -1isize as Handle;

type OpenFn = unsafe extern "C" fn(*const c_char, i32, i16, u64) -> Handle;
type RecvExFn = unsafe extern "C" fn(
    Handle,
    *mut c_void,
    u32,
    *mut u32,
    u64,
    *mut Address,
    *mut u32,
    *mut c_void,
) -> i32;
type SendExFn = unsafe extern "C" fn(
    Handle,
    *const c_void,
    u32,
    *mut u32,
    u64,
    *const Address,
    u32,
    *mut c_void,
) -> i32;
type ShutdownFn = unsafe extern "C" fn(Handle, i32) -> i32;
type CloseFn = unsafe extern "C" fn(Handle) -> i32;
type GetParamFn = unsafe extern "C" fn(Handle, i32, *mut u64) -> i32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Layer {
    Network = 0,
    NetworkForward = 1,
    Flow = 2,
    Socket = 3,
    Reflect = 4,
}

pub mod flags {
    pub const SNIFF: u64 = 0x0001;
    pub const DROP: u64 = 0x0002;
    pub const RECV_ONLY: u64 = 0x0004;
    pub const SEND_ONLY: u64 = 0x0008;
    pub const NO_INSTALL: u64 = 0x0010;
    pub const FRAGMENTS: u64 = 0x0020;
}

pub mod event {
    pub const FLOW_ESTABLISHED: u8 = 1;
    pub const FLOW_DELETED: u8 = 2;
    pub const SOCKET_BIND: u8 = 3;
    pub const SOCKET_CONNECT: u8 = 4;
    pub const SOCKET_LISTEN: u8 = 5;
    pub const SOCKET_ACCEPT: u8 = 6;
    pub const SOCKET_CLOSE: u8 = 7;
}

const SHUTDOWN_RECV: i32 = 0x1;
const PARAM_VERSION_MAJOR: i32 = 3;
const PARAM_VERSION_MINOR: i32 = 4;
const ERROR_NO_DATA: i32 = 232;

/// The loaded DLL. Keep it alive (via `Arc`) as long as any handle exists.
pub struct WinDivertLib {
    path: PathBuf,
    open: OpenFn,
    recv_ex: RecvExFn,
    send_ex: SendExFn,
    shutdown: ShutdownFn,
    close: CloseFn,
    get_param: GetParamFn,
    _lib: Library,
}

impl WinDivertLib {
    /// Find and load WinDivert.dll. Search order:
    /// `$SNAILNET_WINDIVERT_DIR`, the executable's folder, then the repo's
    /// pinned `third_party/windivert/x64` (dev builds).
    pub fn load() -> Result<Arc<Self>, EngineError> {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(d) = std::env::var_os("SNAILNET_WINDIVERT_DIR") {
            dirs.push(d.into());
        }
        if let Some(d) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
        {
            dirs.push(d);
        }
        dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party/windivert/x64"));

        let dll = dirs
            .iter()
            .map(|d| d.join("WinDivert.dll"))
            .find(|p| p.is_file())
            .ok_or_else(|| {
                EngineError::LibraryNotFound(
                    dirs.iter()
                        .map(|d| d.display().to_string())
                        .collect::<Vec<_>>()
                        .join("; "),
                )
            })?;
        // `absolute` resolves `..` without canonicalize's `\\?\` prefix.
        let dll = std::path::absolute(&dll).unwrap_or(dll);
        Self::load_from(&dll).map(Arc::new)
    }

    fn load_from(path: &Path) -> Result<Self, EngineError> {
        let err = |e: libloading::Error| EngineError::LibraryLoad {
            path: path.display().to_string(),
            source: Box::new(e),
        };
        // SAFETY: loading WinDivert.dll runs only its trivial DllMain. The
        // absolute path plus ALTERED_SEARCH_PATH makes Windows resolve its
        // dependencies (and WinDivert its .sys) from the same folder.
        let lib = unsafe { Library::load_with_flags(path, LOAD_WITH_ALTERED_SEARCH_PATH) }
            .map_err(err)?;
        // SAFETY: each signature matches windivert.h for 2.2.x (x64, where
        // the default C calling convention is the only one).
        unsafe {
            Ok(Self {
                path: path.to_path_buf(),
                open: *lib.get::<OpenFn>(c"WinDivertOpen").map_err(err)?,
                recv_ex: *lib.get::<RecvExFn>(c"WinDivertRecvEx").map_err(err)?,
                send_ex: *lib.get::<SendExFn>(c"WinDivertSendEx").map_err(err)?,
                shutdown: *lib.get::<ShutdownFn>(c"WinDivertShutdown").map_err(err)?,
                close: *lib.get::<CloseFn>(c"WinDivertClose").map_err(err)?,
                get_param: *lib.get::<GetParamFn>(c"WinDivertGetParam").map_err(err)?,
                _lib: lib,
            })
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// One open WinDivert handle. Closing it (drop, or process exit) returns the
/// traffic it matched to the normal path.
pub struct WinDivert {
    lib: Arc<WinDivertLib>,
    raw: Handle,
}

// SAFETY: WinDivert handles are kernel file handles; the docs allow
// concurrent recv/send from multiple threads.
unsafe impl Send for WinDivert {}
// SAFETY: see above; no interior state on our side.
unsafe impl Sync for WinDivert {}

impl WinDivert {
    pub fn open(
        lib: &Arc<WinDivertLib>,
        filter: &str,
        layer: Layer,
        priority: i16,
        flags: u64,
    ) -> Result<Self, EngineError> {
        let filter = CString::new(filter).map_err(|_| EngineError::Open {
            code: 87,
            hint: "filter contains a NUL byte",
        })?;
        // SAFETY: `filter` is a valid C string for the duration of the call.
        let raw = unsafe { (lib.open)(filter.as_ptr(), layer as i32, priority, flags) };
        if raw == INVALID_HANDLE || raw.is_null() {
            let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            return Err(EngineError::Open {
                code,
                hint: open_hint(code),
            });
        }
        Ok(Self {
            lib: lib.clone(),
            raw,
        })
    }

    /// Blocking batched receive into `batch`. Returns the packet count; for
    /// FLOW/SOCKET handles use [`RecvBatch::addresses`] instead.
    pub fn recv(&self, batch: &mut RecvBatch) -> Result<usize, EngineError> {
        let (buf, addrs) = batch.raw_mut();
        let mut recv_len: u32 = 0;
        let mut addr_len = std::mem::size_of_val(addrs) as u32;
        // SAFETY: `buf` and `addrs` are valid for the lengths passed;
        // synchronous call (no OVERLAPPED).
        let ok = unsafe {
            (self.lib.recv_ex)(
                self.raw,
                buf.as_mut_ptr().cast(),
                buf.len() as u32,
                &mut recv_len,
                0,
                addrs.as_mut_ptr(),
                &mut addr_len,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            let e = std::io::Error::last_os_error();
            return Err(if e.raw_os_error() == Some(ERROR_NO_DATA) {
                EngineError::Shutdown
            } else {
                e.into()
            });
        }
        let count = addr_len as usize / std::mem::size_of::<Address>();
        Ok(batch.fill(recv_len as usize, count))
    }

    pub fn send(&self, batch: &mut SendBatch) -> Result<(), EngineError> {
        if batch.is_empty() {
            return Ok(());
        }
        let (buf, addrs) = batch.raw();
        // SAFETY: both slices are valid for the lengths passed.
        let ok = unsafe {
            (self.lib.send_ex)(
                self.raw,
                buf.as_ptr().cast(),
                buf.len() as u32,
                std::ptr::null_mut(),
                0,
                addrs.as_ptr(),
                std::mem::size_of_val(addrs) as u32,
                std::ptr::null_mut(),
            )
        };
        batch.clear();
        if ok == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }

    /// Stop receiving (wakes a blocked `recv` with [`EngineError::Shutdown`]).
    pub fn shutdown_recv(&self) {
        // SAFETY: live handle.
        unsafe { (self.lib.shutdown)(self.raw, SHUTDOWN_RECV) };
    }

    /// Driver version, e.g. `(2, 2)`.
    pub fn driver_version(&self) -> Option<(u64, u64)> {
        let get = |p| {
            let mut v = 0u64;
            // SAFETY: live handle, valid out-pointer.
            (unsafe { (self.lib.get_param)(self.raw, p, &mut v) } != 0).then_some(v)
        };
        Some((get(PARAM_VERSION_MAJOR)?, get(PARAM_VERSION_MINOR)?))
    }
}

impl Drop for WinDivert {
    fn drop(&mut self) {
        // SAFETY: closed exactly once; the Arc keeps the DLL loaded.
        unsafe { (self.lib.close)(self.raw) };
    }
}

impl PacketEngine for WinDivert {
    fn recv_batch(&self, batch: &mut RecvBatch) -> Result<usize, EngineError> {
        self.recv(batch)
    }
    fn send_batch(&self, batch: &mut SendBatch) -> Result<(), EngineError> {
        self.send(batch)
    }
    fn shutdown(&self) {
        self.shutdown_recv();
    }
}

/// Turn WinDivertOpen's error codes into something a person can act on.
/// These are exactly the Phase 0 compatibility questions (HVCI, other users).
fn open_hint(code: i32) -> &'static str {
    match code {
        2 => "WinDivert64.sys was not found next to WinDivert.dll",
        5 => "access denied: run this as Administrator",
        87 => "the filter or a parameter is invalid",
        577 => {
            "Windows rejected the driver signature (Secure Boot, Memory integrity/HVCI, or security software)"
        }
        654 => {
            "another app has a different WinDivert version loaded (VPN, game booster, GoodbyeDPI…); close it or reboot"
        }
        1060 => "the WinDivert driver service is missing",
        1275 => {
            "Windows blocked the driver (Memory integrity/HVCI or the vulnerable-driver blocklist)"
        }
        1753 => "the Base Filtering Engine service is not running",
        _ => "unexpected error; see the WinDivert docs for this code",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_pinned_dll_without_admin() {
        // Loading the DLL and resolving symbols needs no privileges; only
        // WinDivertOpen (which installs the driver) does.
        let lib = WinDivertLib::load().expect("third_party/windivert/x64/WinDivert.dll");
        assert!(lib.path().ends_with("WinDivert.dll"));
    }
}
