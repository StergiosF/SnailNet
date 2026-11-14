//! PID → executable path (PLAN.md §7.1).

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};

/// Full image path of a running process, or `None` if it has exited or we
/// lack access (e.g. protected processes). PID 4 ("System") has no image.
pub fn image_path(pid: u32) -> Option<PathBuf> {
    if pid == 0 || pid == 4 {
        return None;
    }
    // SAFETY: plain FFI call; a null return means failure and is handled.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut buf = vec![0u16; 32 * 1024];
    let mut len = buf.len() as u32;
    // SAFETY: `buf` is valid for `len` u16s and `handle` is a live process handle.
    let ok = unsafe {
        QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
    };
    // SAFETY: `handle` came from OpenProcess and is closed exactly once.
    unsafe { CloseHandle(handle) };
    if ok == 0 {
        return None;
    }
    Some(PathBuf::from(OsString::from_wide(&buf[..len as usize])))
}

/// Lowercased file name (`steam.exe`) of a process, for matching rules.
pub fn exe_name(pid: u32) -> Option<String> {
    let path = image_path(pid)?;
    Some(path.file_name()?.to_string_lossy().to_lowercase())
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
}
