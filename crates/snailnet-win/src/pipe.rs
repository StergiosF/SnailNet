//! Who is on the other end of a named pipe (PLAN.md §12.1).
//!
//! - The **service** checks each client with [`peer_pid`] +
//!   [`in_my_folder`]: only programs installed next to it may connect
//!   (the CLI now, `SnailNet.exe` later). It runs as SYSTEM, so it can
//!   read any client's image path.
//! - A **client** can't read the service's image path (a normal user may
//!   not open a SYSTEM service's process at all), so it checks the pipe's
//!   owner instead with [`owner_is_trusted`]: a pipe the service created is
//!   owned by SYSTEM or Administrators, which a non-admin can't fake.

use std::io;
use std::os::windows::io::RawHandle;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT};
use windows_sys::Win32::Security::{
    EqualSid, GetTokenInformation, IsWellKnownSid, OWNER_SECURITY_INFORMATION, PSID, TOKEN_QUERY,
    TOKEN_USER, TokenUser, WinBuiltinAdministratorsSid, WinLocalSystemSid,
};
use windows_sys::Win32::System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Which end of the pipe to ask about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Client,
    Server,
}

/// The process id at `end` of the pipe `handle`.
pub fn peer_pid(handle: RawHandle, end: End) -> io::Result<u32> {
    let mut pid = 0u32;
    // SAFETY: `handle` is a live pipe handle owned by the caller; `pid` is a
    // valid out pointer.
    let ok = unsafe {
        match end {
            End::Client => GetNamedPipeClientProcessId(handle as HANDLE, &mut pid),
            End::Server => GetNamedPipeServerProcessId(handle as HANDLE, &mut pid),
        }
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(pid)
}

/// The folder of this program's exe, resolved (no `..`, symlinks or 8.3
/// names) so it compares reliably.
pub fn my_folder() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or_else(|| io::Error::other("the exe has no parent folder"))?;
    std::fs::canonicalize(dir)
}

/// True if `exe` lives directly in `folder` (an already-resolved folder).
pub fn is_in_folder(exe: &Path, folder: &Path) -> bool {
    let Some(dir) = exe.parent() else {
        return false;
    };
    let Ok(dir) = std::fs::canonicalize(dir) else {
        return false;
    };
    // NTFS names are case-insensitive.
    dir.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&folder.as_os_str().to_string_lossy())
}

/// The image path of process `pid` if it lives in this program's folder;
/// `Err` says why not (gone, unreadable, or elsewhere).
pub fn in_my_folder(pid: u32) -> Result<PathBuf, String> {
    let folder = my_folder().map_err(|e| format!("can't resolve our own folder: {e}"))?;
    let exe = crate::process::image_path(pid)
        .ok_or_else(|| format!("can't read the image path of process {pid}"))?;
    if is_in_folder(&exe, &folder) {
        Ok(exe)
    } else {
        let folder = folder.to_string_lossy();
        let folder = folder.strip_prefix(r"\\?\").unwrap_or(&folder);
        Err(format!("{} is not in {folder}", exe.display()))
    }
}

/// True if the object behind `handle` is owned by SYSTEM, Administrators or
/// the user this process runs as. Needs `READ_CONTROL` on the handle.
pub fn owner_is_trusted(handle: RawHandle) -> io::Result<bool> {
    let mut owner: PSID = std::ptr::null_mut();
    let mut sd = std::ptr::null_mut();
    // SAFETY: valid handle; out pointers are valid; `sd` is freed below and
    // `owner` points into it.
    let err = unsafe {
        GetSecurityInfo(
            handle as HANDLE,
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut sd,
        )
    };
    if err != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(err as i32));
    }
    let trusted = (|| {
        // SAFETY: `owner` is a valid SID inside `sd`.
        let well_known = unsafe {
            IsWellKnownSid(owner, WinLocalSystemSid) != 0
                || IsWellKnownSid(owner, WinBuiltinAdministratorsSid) != 0
        };
        if well_known {
            return Ok(true);
        }
        let me = current_user_sid()?;
        // SAFETY: both SIDs are valid for the duration of the call.
        Ok(unsafe { EqualSid(owner, me.as_ptr()) } != 0)
    })();
    // SAFETY: `sd` came from GetSecurityInfo and is freed exactly once.
    unsafe { LocalFree(sd) };
    trusted
}

/// This process's user SID, kept inside its `TOKEN_USER` buffer.
struct UserSid(Vec<u64>);

impl UserSid {
    fn as_ptr(&self) -> PSID {
        // SAFETY: the buffer holds a TOKEN_USER filled by GetTokenInformation.
        unsafe { (*(self.0.as_ptr() as *const TOKEN_USER)).User.Sid }
    }
}

fn current_user_sid() -> io::Result<UserSid> {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: the pseudo handle needs no closing; `token` is closed below.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let result = (|| {
        let mut len = 0u32;
        // SAFETY: size query with a null buffer.
        unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut len) };
        if len == 0 {
            return Err(io::Error::last_os_error());
        }
        // u64s keep TOKEN_USER's pointer field aligned.
        let mut buf = vec![0u64; (len as usize).div_ceil(8)];
        // SAFETY: `buf` is at least `len` bytes.
        let ok = unsafe {
            GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), len, &mut len)
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(UserSid(buf))
    })();
    // SAFETY: opened above, closed exactly once.
    unsafe { CloseHandle(token) };
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_is_in_its_own_folder() {
        let exe = in_my_folder(std::process::id()).unwrap();
        assert_eq!(
            exe.file_name(),
            std::env::current_exe().unwrap().file_name()
        );
    }

    #[test]
    fn a_system_exe_is_not_in_our_folder() {
        let folder = my_folder().unwrap();
        let cmd = Path::new(&crate::process::windows_dir()).join(r"System32\cmd.exe");
        assert!(!is_in_folder(&cmd, &folder));
        assert!(is_in_folder(&std::env::current_exe().unwrap(), &folder));
    }

    #[test]
    fn a_file_we_created_has_a_trusted_owner() {
        use std::os::windows::io::AsRawHandle;
        let t = std::env::temp_dir().join(format!("snailnet-owner-{}", std::process::id()));
        let f = std::fs::File::create(&t).unwrap();
        // Owned by us (or Administrators when elevated).
        assert!(owner_is_trusted(f.as_raw_handle()).unwrap());
        drop(f);
        let _ = std::fs::remove_file(&t);
    }
}
