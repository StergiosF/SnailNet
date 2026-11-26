//! What Windows can tell us about a process (PLAN.md §7.1): image path,
//! creation time (for PID-reuse safety), parent, command line (for
//! `svchost -s`), Store package, and the exe's publisher and description.
//!
//! [`WinProcesses`] serves these to the app resolver in
//! `snailnet_core::classify`. Everything works with
//! `PROCESS_QUERY_LIMITED_INFORMATION`, which LocalSystem and elevated
//! admins get for nearly every process (protected ones excepted).

use std::collections::HashMap;
use std::ffi::{OsStr, OsString, c_void};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use snailnet_core::classify::{ProcessFacts, ProcessSource};
use windows_sys::Wdk::System::Threading::{
    NtQueryInformationProcess, ProcessBasicInformation, ProcessCommandLineInformation,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_INSUFFICIENT_BUFFER, FILETIME, GetLastError, HANDLE, UNICODE_STRING,
};
use windows_sys::Win32::Security::Cryptography::{
    CERT_FIND_SUBJECT_CERT, CERT_INFO, CERT_NAME_SIMPLE_DISPLAY_TYPE,
    CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED, CERT_QUERY_FORMAT_FLAG_BINARY,
    CERT_QUERY_OBJECT_FILE, CMSG_SIGNER_INFO, CMSG_SIGNER_INFO_PARAM, CertCloseStore,
    CertFindCertificateInStore, CertFreeCertificateContext, CertGetNameStringW, CryptMsgClose,
    CryptMsgGetParam, CryptQueryObject, PKCS_7_ASN_ENCODING, X509_ASN_ENCODING,
};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows_sys::Win32::Storage::Packaging::Appx::GetPackageFamilyName;
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_BASIC_INFORMATION, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};

/// An open process handle, closed on drop.
struct Process(HANDLE);

impl Process {
    fn open(pid: u32) -> Option<Self> {
        if pid == 0 || pid == 4 {
            return None;
        }
        // SAFETY: plain FFI call; a null return means failure and is handled.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        (!handle.is_null()).then_some(Self(handle))
    }

    fn image_path(&self) -> Option<PathBuf> {
        let mut stack = [0u16; 512];
        let mut len = stack.len() as u32;
        // SAFETY: `stack` is valid for `len` u16s and the handle is live.
        let ok = unsafe {
            QueryFullProcessImageNameW(self.0, PROCESS_NAME_WIN32, stack.as_mut_ptr(), &mut len)
        };
        if ok != 0 {
            return Some(PathBuf::from(OsString::from_wide(&stack[..len as usize])));
        }
        // SAFETY: reads thread-local last error set by QueryFullProcessImageNameW.
        if unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
            return None;
        }
        let mut buf = vec![0u16; 32 * 1024];
        let mut len = buf.len() as u32;
        // SAFETY: `buf` is valid for `len` u16s and the handle is live.
        let ok = unsafe {
            QueryFullProcessImageNameW(self.0, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
        };
        (ok != 0).then(|| PathBuf::from(OsString::from_wide(&buf[..len as usize])))
    }

    /// FILETIME as one u64 (100 ns ticks since 1601).
    fn created(&self) -> Option<u64> {
        let zero = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut c, mut e, mut k, mut u) = (zero, zero, zero, zero);
        // SAFETY: four valid FILETIME out-pointers and a live handle.
        let ok = unsafe { GetProcessTimes(self.0, &mut c, &mut e, &mut k, &mut u) };
        (ok != 0).then_some((c.dwHighDateTime as u64) << 32 | c.dwLowDateTime as u64)
    }

    fn parent(&self) -> Option<u32> {
        // SAFETY: an all-zero PROCESS_BASIC_INFORMATION (integers and a null
        // pointer) is valid.
        let mut pbi: PROCESS_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        let mut ret = 0u32;
        // SAFETY: `pbi` is valid for its own size; the handle is live.
        let status = unsafe {
            NtQueryInformationProcess(
                self.0,
                ProcessBasicInformation,
                (&raw mut pbi).cast(),
                size_of::<PROCESS_BASIC_INFORMATION>() as u32,
                &mut ret,
            )
        };
        (status >= 0).then_some(pbi.InheritedFromUniqueProcessId as u32)
    }

    fn command_line(&self) -> Option<String> {
        let mut len = 0u32;
        // SAFETY: a zero-length query only reports the size it needs.
        unsafe {
            NtQueryInformationProcess(
                self.0,
                ProcessCommandLineInformation,
                std::ptr::null_mut(),
                0,
                &mut len,
            )
        };
        let size = len as usize;
        if !(size_of::<UNICODE_STRING>()..=1 << 20).contains(&size) {
            return None;
        }
        // u64 storage keeps the UNICODE_STRING header aligned.
        let mut buf = vec![0u64; size.div_ceil(8)];
        // SAFETY: `buf` is valid for `len` bytes; the handle is live.
        let status = unsafe {
            NtQueryInformationProcess(
                self.0,
                ProcessCommandLineInformation,
                buf.as_mut_ptr().cast(),
                len,
                &mut len,
            )
        };
        if status < 0 {
            return None;
        }
        // SAFETY: on success the buffer starts with a UNICODE_STRING.
        let us = unsafe { buf.as_ptr().cast::<UNICODE_STRING>().read() };
        let base = buf.as_ptr() as usize;
        let start = us.Buffer as usize;
        let bytes = us.Length as usize;
        // The string must lie inside our buffer, as the API promises.
        if us.Buffer.is_null() || start < base || start + bytes > base + buf.len() * 8 {
            return None;
        }
        // SAFETY: bounds checked just above; UTF-16 units are 2-byte aligned.
        let chars = unsafe { std::slice::from_raw_parts(us.Buffer, bytes / 2) };
        Some(String::from_utf16_lossy(chars))
    }

    fn package_family(&self) -> Option<String> {
        let mut buf = [0u16; 256];
        let mut len = buf.len() as u32;
        // SAFETY: `buf` is valid for `len` u16s; the handle is live.
        let rc = unsafe { GetPackageFamilyName(self.0, &mut len, buf.as_mut_ptr()) };
        // `len` counts the terminating null.
        let len = (len as usize).min(buf.len()).saturating_sub(1);
        (rc == 0 && len > 0).then(|| String::from_utf16_lossy(&buf[..len]))
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: the handle came from OpenProcess and is closed exactly once.
        unsafe { CloseHandle(self.0) };
    }
}

/// Full image path of a running process, or `None` if it has exited or we
/// lack access (e.g. protected processes). PID 4 ("System") has no image.
pub fn image_path(pid: u32) -> Option<PathBuf> {
    Process::open(pid)?.image_path()
}

/// Lowercased file name (`steam.exe`) of a process, for matching rules.
pub fn exe_name(pid: u32) -> Option<String> {
    let path = image_path(pid)?;
    Some(path.file_name()?.to_string_lossy().to_lowercase())
}

/// Process creation time (FILETIME ticks).
pub fn created(pid: u32) -> Option<u64> {
    Process::open(pid)?.created()
}

/// Parent PID as recorded at creation. The parent may have exited since,
/// and its PID may have been reused: compare creation times.
pub fn parent(pid: u32) -> Option<u32> {
    Process::open(pid)?.parent()
}

pub fn command_line(pid: u32) -> Option<String> {
    Process::open(pid)?.command_line()
}

/// `%SystemRoot%`, normally `C:\Windows`.
pub fn windows_dir() -> String {
    static DIR: OnceLock<String> = OnceLock::new();
    DIR.get_or_init(|| std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()))
        .clone()
}

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

/// Display name of the signer of `path`'s embedded Authenticode signature.
/// Only reads the signature; trust isn't verified (PLAN.md §7.2 uses it to
/// group files in one folder, which already needs write access to fake).
/// Catalog-signed Windows files have no embedded signature → `None`.
pub fn publisher(path: &Path) -> Option<String> {
    let w = wide(path.as_os_str());
    let (mut encoding, mut content, mut format) = (0u32, 0u32, 0u32);
    let mut store: *mut c_void = std::ptr::null_mut();
    let mut msg: *mut c_void = std::ptr::null_mut();
    // SAFETY: `w` is a null-terminated path; all out-pointers are valid.
    let ok = unsafe {
        CryptQueryObject(
            CERT_QUERY_OBJECT_FILE,
            w.as_ptr().cast(),
            CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED,
            CERT_QUERY_FORMAT_FLAG_BINARY,
            0,
            &mut encoding,
            &mut content,
            &mut format,
            &mut store,
            &mut msg,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return None;
    }
    let name = signer_name(store, msg);
    // SAFETY: both came from a successful CryptQueryObject; closed once.
    unsafe {
        CryptMsgClose(msg);
        CertCloseStore(store, 0);
    }
    name
}

fn signer_name(store: *mut c_void, msg: *mut c_void) -> Option<String> {
    let mut size = 0u32;
    // SAFETY: a null data pointer asks for the size.
    let ok = unsafe {
        CryptMsgGetParam(
            msg,
            CMSG_SIGNER_INFO_PARAM,
            0,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if ok == 0 || (size as usize) < size_of::<CMSG_SIGNER_INFO>() || size > 1 << 20 {
        return None;
    }
    let mut buf = vec![0u64; (size as usize).div_ceil(8)];
    // SAFETY: `buf` is valid (and 8-byte aligned) for `size` bytes.
    let ok = unsafe {
        CryptMsgGetParam(
            msg,
            CMSG_SIGNER_INFO_PARAM,
            0,
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if ok == 0 {
        return None;
    }
    // SAFETY: on success the buffer starts with a CMSG_SIGNER_INFO, whose
    // blobs point into the same buffer.
    let signer = unsafe { buf.as_ptr().cast::<CMSG_SIGNER_INFO>().read() };
    // SAFETY: CERT_INFO is plain data; all-zero is valid.
    let mut info: CERT_INFO = unsafe { std::mem::zeroed() };
    info.Issuer = signer.Issuer;
    info.SerialNumber = signer.SerialNumber;
    // SAFETY: `info` outlives the call; `store` is a live store handle.
    let cert = unsafe {
        CertFindCertificateInStore(
            store,
            X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
            0,
            CERT_FIND_SUBJECT_CERT,
            (&raw const info).cast(),
            std::ptr::null(),
        )
    };
    if cert.is_null() {
        return None;
    }
    let mut name = [0u16; 512];
    // SAFETY: `cert` is live; `name` is valid for its length.
    let n = unsafe {
        CertGetNameStringW(
            cert,
            CERT_NAME_SIMPLE_DISPLAY_TYPE,
            0,
            std::ptr::null(),
            name.as_mut_ptr(),
            name.len() as u32,
        )
    };
    // SAFETY: from CertFindCertificateInStore; freed once.
    unsafe { CertFreeCertificateContext(cert) };
    // `n` counts the terminating null; 1 means an empty name.
    let n = (n as usize).min(name.len()).saturating_sub(1);
    let s = String::from_utf16_lossy(&name[..n]).trim().to_owned();
    (!s.is_empty()).then_some(s)
}

/// `FileDescription` from `path`'s version resource.
pub fn description(path: &Path) -> Option<String> {
    let w = wide(path.as_os_str());
    // SAFETY: null-terminated path; the handle argument may be null.
    let size = unsafe { GetFileVersionInfoSizeW(w.as_ptr(), std::ptr::null_mut()) };
    if size == 0 || size > 1 << 22 {
        return None;
    }
    let mut data = vec![0u64; (size as usize).div_ceil(8)];
    // SAFETY: `data` is valid for `size` bytes.
    let ok = unsafe { GetFileVersionInfoW(w.as_ptr(), 0, size, data.as_mut_ptr().cast()) };
    if ok == 0 {
        return None;
    }
    let block = Block {
        base: data.as_ptr().cast(),
        len: size as usize,
    };
    let mut langs: Vec<(u16, u16)> = block
        .query(r"\VarFileInfo\Translation")
        .map(|bytes| {
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .take(16)
                .map(|c| {
                    (
                        u16::from_le_bytes([c[0], c[1]]),
                        u16::from_le_bytes([c[2], c[3]]),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    // US English, Unicode and Windows-1252: what most files use anyway.
    langs.extend([(0x0409, 0x04b0), (0x0409, 0x04e4)]);
    langs.into_iter().find_map(|(lang, cp)| {
        let bytes = block.query(&format!(
            r"\StringFileInfo\{lang:04x}{cp:04x}\FileDescription"
        ))?;
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| u16::from_le_bytes(c))
            .take_while(|&u| u != 0)
            .collect();
        let s = String::from_utf16_lossy(&units).trim().to_owned();
        (!s.is_empty()).then_some(s)
    })
}

/// A version-info block, for bounds-checked `VerQueryValueW`.
struct Block {
    base: *const u8,
    len: usize,
}

impl Block {
    /// The value's bytes, from its start to the end of the block. Callers
    /// stop at their own terminator: the reported length's unit differs
    /// between string and binary values.
    fn query(&self, sub_block: &str) -> Option<&[u8]> {
        let q = wide(OsStr::new(sub_block));
        let mut ptr: *mut c_void = std::ptr::null_mut();
        let mut n = 0u32;
        // SAFETY: `base` holds a block from GetFileVersionInfoW; out-pointers
        // are valid.
        let ok = unsafe { VerQueryValueW(self.base.cast(), q.as_ptr(), &mut ptr, &mut n) };
        let start = ptr as usize;
        let (lo, hi) = (self.base as usize, self.base as usize + self.len);
        if ok == 0 || ptr.is_null() || n == 0 || start < lo || start >= hi {
            return None;
        }
        // SAFETY: [start, hi) lies inside the block, which outlives `self`.
        Some(unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), hi - start) })
    }
}

/// Per-exe facts that are slow to read (signature, version info).
#[derive(Clone)]
struct FileFacts {
    publisher: Option<String>,
    description: Option<String>,
}

const MAX_FILE_CACHE: usize = 4096;

/// [`ProcessSource`] backed by the live system.
#[derive(Default)]
pub struct WinProcesses {
    files: HashMap<PathBuf, FileFacts>,
}

impl WinProcesses {
    pub fn new() -> Self {
        Self::default()
    }

    fn file_facts(&mut self, path: &Path) -> FileFacts {
        if let Some(f) = self.files.get(path) {
            return f.clone();
        }
        let f = FileFacts {
            publisher: publisher(path),
            description: description(path),
        };
        if self.files.len() >= MAX_FILE_CACHE {
            self.files.clear();
        }
        self.files.insert(path.to_owned(), f.clone());
        f
    }
}

impl ProcessSource for WinProcesses {
    fn created(&mut self, pid: u32) -> Option<u64> {
        created(pid)
    }

    fn facts(&mut self, pid: u32) -> Option<ProcessFacts> {
        let p = Process::open(pid)?;
        let created = p.created()?;
        let path = p.image_path();
        let svchost = path
            .as_deref()
            .and_then(Path::file_name)
            .is_some_and(|n| n.eq_ignore_ascii_case("svchost.exe"));
        let file = path.as_deref().map(|path| self.file_facts(path));
        Some(ProcessFacts {
            pid,
            created,
            parent: p.parent(),
            command_line: if svchost { p.command_line() } else { None },
            package: p.package_family(),
            publisher: file.as_ref().and_then(|f| f.publisher.clone()),
            description: file.and_then(|f| f.description),
            path: path.map(|p| p.to_string_lossy().into_owned()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_own_exe() {
        let name = exe_name(std::process::id()).unwrap();
        assert!(name.ends_with(".exe"), "{name}");
    }

    #[test]
    fn system_and_idle_have_no_image() {
        assert_eq!(image_path(0), None);
        assert_eq!(image_path(4), None);
    }

    #[test]
    fn own_process_facts() {
        let me = std::process::id();
        let created = created(me).unwrap();
        assert_eq!(super::created(me), Some(created), "stable");
        let facts = WinProcesses::new().facts(me).unwrap();
        assert_eq!(facts.created, created);
        assert!(facts.path.unwrap().to_lowercase().ends_with(".exe"));
        assert!(facts.parent.is_some_and(|p| p != 0 && p != me));
        assert_eq!(facts.command_line, None, "only read for svchost");
        assert_eq!(facts.package, None);
        let cmd = command_line(me).unwrap();
        let exe = exe_name(me).unwrap();
        assert!(
            cmd.to_lowercase().contains(exe.trim_end_matches(".exe")),
            "{cmd}"
        );
    }

    #[test]
    fn child_knows_its_parent_and_starts_later() {
        let mut child = std::process::Command::new("cmd.exe")
            .args(["/c", "ping -n 3 127.0.0.1 >nul"])
            .spawn()
            .unwrap();
        let me = std::process::id();
        let parent_of_child = parent(child.id());
        let child_created = created(child.id());
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(parent_of_child, Some(me));
        assert!(child_created.unwrap() > created(me).unwrap());
    }

    #[test]
    fn reads_embedded_signature_publisher() {
        let dll = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../third_party/windivert/x64/WinDivert64.sys");
        let name =
            publisher(&dll).expect("the driver carries an embedded signature (the DLL does not)");
        assert!(!name.is_empty());
        let unsigned = std::env::current_exe().unwrap();
        assert_eq!(publisher(&unsigned), None);
    }

    #[test]
    fn reads_file_description() {
        let kernel32 = Path::new(&windows_dir()).join(r"System32\kernel32.dll");
        assert!(description(&kernel32).is_some_and(|d| !d.is_empty()));
        assert_eq!(description(Path::new(r"C:\does\not\exist.exe")), None);
    }
}
