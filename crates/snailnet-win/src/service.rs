//! A Windows service's own permissions (who may start, stop or change it).

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR};
use windows_sys::Win32::Storage::FileSystem::{READ_CONTROL, WRITE_DAC};
use windows_sys::Win32::System::Services::{
    CloseServiceHandle, OpenSCManagerW, OpenServiceW, SC_MANAGER_CONNECT, SetServiceObjectSecurity,
};

/// Windows' default service DACL, except that interactive users (`IU`) may
/// also *start* it (`RP`). They still can't stop, pause or reconfigure it.
/// This lets the app start a stopped SnailNet without a UAC prompt.
pub const USERS_MAY_START_SDDL: &str = "D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)\
(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWRPLOCRRC;;;IU)(A;;CCLCSWLOCRRC;;;SU)";

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

/// Replaces the DACL of the service `name` with `sddl`. Needs Administrator.
pub fn set_dacl(name: &str, sddl: &str) -> io::Result<()> {
    let sddl = wide(sddl);
    let name = wide(name);
    // SAFETY: plain Win32 calls; every handle and the descriptor are freed
    // on every path below, and the wide strings outlive the calls.
    unsafe {
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut sd,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let scm = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            let e = io::Error::last_os_error();
            LocalFree(sd);
            return Err(e);
        }
        let svc = OpenServiceW(scm, name.as_ptr(), WRITE_DAC | READ_CONTROL);
        let result = if svc.is_null() {
            Err(io::Error::last_os_error())
        } else {
            let r = if SetServiceObjectSecurity(svc, DACL_SECURITY_INFORMATION, sd) == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            };
            CloseServiceHandle(svc);
            r
        };
        CloseServiceHandle(scm);
        LocalFree(sd);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sddl_parses() {
        let s = wide(USERS_MAY_START_SDDL);
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: `s` is NUL-terminated; the descriptor is freed below.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                s.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(ok, 0);
        // SAFETY: allocated by the call above.
        unsafe { LocalFree(sd) };
    }

    #[test]
    fn a_missing_service_is_an_error() {
        assert!(set_dacl("SnailNet-no-such-service", USERS_MAY_START_SDDL).is_err());
    }
}
