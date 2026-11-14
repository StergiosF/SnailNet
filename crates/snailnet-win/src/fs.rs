//! Files the service owns (PLAN.md §13.1): atomic replacement with a
//! backup, and a protected ACL on the data folder.

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::{ERROR_SUCCESS, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1, SE_FILE_OBJECT,
    SetNamedSecurityInfoW,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
};
use windows_sys::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    REPLACEFILE_IGNORE_MERGE_ERRORS, ReplaceFileW,
};

/// The data folder's ACL: SYSTEM and Administrators full control, Users
/// read-only, inherited by everything inside; parent permissions (which let
/// any user create files in `C:\ProgramData`) are not inherited.
pub const DATA_DIR_SDDL: &str = "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FR;;;BU)";

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

/// Put `replacement` in `target`'s place in one step. If `target` existed,
/// its old contents end up in `backup` (overwriting any older backup).
/// `replacement` is gone afterwards.
pub fn replace_file(target: &Path, replacement: &Path, backup: &Path) -> io::Result<()> {
    let (t, r, b) = (
        wide(target.as_os_str()),
        wide(replacement.as_os_str()),
        wide(backup.as_os_str()),
    );
    if target.exists() {
        // SAFETY: three NUL-terminated paths that outlive the call; the
        // reserved pointers are null as required.
        let ok = unsafe {
            ReplaceFileW(
                t.as_ptr(),
                r.as_ptr(),
                b.as_ptr(),
                REPLACEFILE_IGNORE_MERGE_ERRORS,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
    } else {
        // SAFETY: two NUL-terminated paths that outlive the call.
        let ok = unsafe {
            MoveFileExW(
                r.as_ptr(),
                t.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Replace `dir`'s DACL with `sddl`'s, marked protected, and push the
/// inheritable entries down to everything already inside.
pub fn protect_dir(dir: &Path, sddl: &str) -> io::Result<()> {
    let s = wide(OsStr::new(sddl));
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: `s` is NUL-terminated; on success `sd` receives a LocalAlloc'd
    // descriptor that we free below.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            s.as_ptr(),
            SDDL_REVISION_1,
            &mut sd,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let result = (|| {
        let (mut present, mut defaulted) = (0, 0);
        let mut dacl: *mut ACL = std::ptr::null_mut();
        // SAFETY: `sd` is a valid descriptor; the out pointers are live.
        let ok = unsafe { GetSecurityDescriptorDacl(sd, &mut present, &mut dacl, &mut defaulted) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        if present == 0 || dacl.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the SDDL has no DACL",
            ));
        }
        let path = wide(dir.as_os_str());
        // SAFETY: `path` is NUL-terminated; `dacl` points into `sd`, which is
        // alive until after this call. Owner, group and SACL are untouched.
        let err = unsafe {
            SetNamedSecurityInfoW(
                path.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                dacl,
                std::ptr::null(),
            )
        };
        if err != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(err as i32));
        }
        Ok(())
    })();
    // SAFETY: `sd` came from ConvertStringSecurityDescriptor…, which says to
    // free it with LocalFree.
    unsafe { LocalFree(sd) };
    result
}

/// A `SECURITY_ATTRIBUTES` built from SDDL, for creating kernel objects
/// such as named pipes. Frees its descriptor on drop.
pub struct SecurityAttributes {
    sa: SECURITY_ATTRIBUTES,
}

// SAFETY: the descriptor is a private LocalAlloc'd block that nothing else
// references; it is only read by the API calls we pass it to.
unsafe impl Send for SecurityAttributes {}
// SAFETY: as above; shared references never write through the pointer.
unsafe impl Sync for SecurityAttributes {}

impl SecurityAttributes {
    pub fn from_sddl(sddl: &str) -> io::Result<Self> {
        let s = wide(OsStr::new(sddl));
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: `s` is NUL-terminated; on success `sd` receives a
        // LocalAlloc'd descriptor, freed in Drop.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                s.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            sa: SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: sd,
                bInheritHandle: 0,
            },
        })
    }

    /// For APIs that take `LPSECURITY_ATTRIBUTES`. Valid while `self` lives;
    /// the callee must only read through it.
    pub fn as_ptr(&self) -> *mut std::ffi::c_void {
        (&self.sa as *const SECURITY_ATTRIBUTES).cast_mut().cast()
    }
}

impl Drop for SecurityAttributes {
    fn drop(&mut self) {
        // SAFETY: the descriptor came from ConvertStringSecurityDescriptor…,
        // which says to free it with LocalFree; it is freed once.
        unsafe { LocalFree(self.sa.lpSecurityDescriptor) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_attributes_from_sddl() {
        let sa = SecurityAttributes::from_sddl("D:P(A;;GA;;;SY)").unwrap();
        assert!(!sa.as_ptr().is_null());
        assert!(SecurityAttributes::from_sddl("not sddl").is_err());
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("snailnet-win-fs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn replace_keeps_a_backup() {
        let d = tmp("replace");
        let (t, r, b) = (d.join("c.toml"), d.join("c.toml.tmp"), d.join("c.toml.bak"));
        std::fs::write(&r, "one").unwrap();
        replace_file(&t, &r, &b).unwrap();
        assert_eq!(std::fs::read_to_string(&t).unwrap(), "one");
        assert!(!r.exists() && !b.exists());
        std::fs::write(&r, "two").unwrap();
        replace_file(&t, &r, &b).unwrap();
        assert_eq!(std::fs::read_to_string(&t).unwrap(), "two");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "one");
        std::fs::write(&r, "three").unwrap();
        replace_file(&t, &r, &b).unwrap();
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "two");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rejects_sddl_without_dacl() {
        let d = tmp("sddl");
        assert!(protect_dir(&d, "O:BA").is_err());
        assert!(protect_dir(&d, "not sddl").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
