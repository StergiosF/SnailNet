//! Starting a stopped service (PLAN.md §14.8). The installer lets signed-in
//! users start SnailNet (only start it), so the app starts it by itself when
//! it opens and finds it stopped, with no prompt. "Start SnailNet" on the
//! not-running notice tries the same first, then falls back to `sc.exe start
//! SnailNet` through the UAC prompt (for installs older than 0.0.5). The
//! bridge reconnects by itself once the service answers.

use serde::Serialize;
use windows::Win32::System::Services::{
    CloseServiceHandle, ControlService, OpenSCManagerW, OpenServiceW, SC_MANAGER_CONNECT,
    SERVICE_CONTROL_STOP, SERVICE_QUERY_STATUS, SERVICE_START, SERVICE_STATUS, SERVICE_STOP,
    StartServiceW,
};
use windows::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
use windows::core::{HRESULT, HSTRING, PCWSTR, w};

/// The service's name, as `snailnet-service install` registers it.
const SERVICE_NAME: &str = "SnailNet";

/// What came of asking.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StartOutcome {
    /// Windows was asked; the bridge will see the service when it's up.
    Asked,
    /// The user said no at the UAC prompt.
    Cancelled,
    /// There is no SnailNet service on this PC (a dev build, or uninstalled).
    NotInstalled,
    /// Something else went wrong.
    Failed { message: String },
}

/// Whether the service is registered with Windows (no admin needed).
fn installed() -> bool {
    // SAFETY: plain Win32 calls; every handle opened here is closed here.
    unsafe {
        let Ok(scm) = OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) else {
            // Can't tell: let the elevated start say what's wrong.
            return true;
        };
        let found = OpenServiceW(scm, &HSTRING::from(SERVICE_NAME), SERVICE_QUERY_STATUS);
        if let Ok(svc) = found {
            let _ = CloseServiceHandle(svc);
        }
        let _ = CloseServiceHandle(scm);
        found.is_ok()
    }
}

/// ERROR_SERVICE_ALREADY_RUNNING.
const ALREADY_RUNNING: u32 = 1056;

/// Starts the service without elevation. Works when the installer granted
/// users the right to start it; fails with access denied on older installs,
/// and with "does not exist" in a dev build.
fn start_plain() -> windows::core::Result<()> {
    // SAFETY: plain Win32 calls; every handle opened here is closed here.
    unsafe {
        let scm = OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT)?;
        let result = match OpenServiceW(scm, &HSTRING::from(SERVICE_NAME), SERVICE_START) {
            Ok(svc) => {
                let r = StartServiceW(svc, None);
                let _ = CloseServiceHandle(svc);
                r
            }
            Err(e) => Err(e),
        };
        let _ = CloseServiceHandle(scm);
        match result {
            Err(e) if e.code() == HRESULT::from_win32(ALREADY_RUNNING) => Ok(()),
            r => r,
        }
    }
}

/// On app start: if SnailNet is installed but stopped, start it, quietly.
/// Whatever fails just leaves the not-running notice and its button.
pub fn start_if_stopped() {
    std::thread::spawn(|| {
        let _ = start_plain();
    });
}

/// ERROR_SERVICE_NOT_ACTIVE.
const NOT_ACTIVE: u32 = 1062;

/// Stops the installed service through SCM without elevation. Used only as
/// a quiet fallback if IPC couldn't stop it (e.g. an elevated dev run or a
/// service that couldn't open its pipe).
pub fn stop_plain() -> windows::core::Result<()> {
    // SAFETY: plain Win32 calls; every handle opened here is closed here.
    unsafe {
        let scm = OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT)?;
        let result = match OpenServiceW(scm, &HSTRING::from(SERVICE_NAME), SERVICE_STOP) {
            Ok(svc) => {
                let mut status = SERVICE_STATUS::default();
                let r = ControlService(svc, SERVICE_CONTROL_STOP, &mut status);
                let _ = CloseServiceHandle(svc);
                r
            }
            Err(e) => Err(e),
        };
        let _ = CloseServiceHandle(scm);
        match result {
            Err(e) if e.code() == HRESULT::from_win32(NOT_ACTIVE) => Ok(()),
            r => r,
        }
    }
}

/// ERROR_CANCELLED: the UAC prompt was declined.
const ERROR_CANCELLED: u32 = 1223;

/// Full path to `%SystemRoot%\System32\sc.exe`, avoiding PATH lookup when
/// elevating via `ShellExecuteExW`.
fn sc_exe_path() -> std::path::PathBuf {
    std::env::var_os("SystemRoot")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"))
        .join("System32")
        .join("sc.exe")
}

#[tauri::command]
pub async fn start_service() -> StartOutcome {
    tauri::async_runtime::spawn_blocking(|| {
        if !installed() {
            return StartOutcome::NotInstalled;
        }
        if start_plain().is_ok() {
            return StartOutcome::Asked;
        }
        let sc_path = HSTRING::from(sc_exe_path().as_os_str());
        let params = HSTRING::from(format!("start {SERVICE_NAME}"));
        let mut info = SHELLEXECUTEINFOW {
            cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            lpVerb: w!("runas"),
            lpFile: PCWSTR(sc_path.as_ptr()),
            lpParameters: PCWSTR(params.as_ptr()),
            nShow: SW_HIDE.0,
            ..Default::default()
        };
        // SAFETY: `info` and the strings it points to outlive the call.
        match unsafe { ShellExecuteExW(&mut info) } {
            Ok(()) => {
                if !info.hProcess.is_invalid() {
                    // SAFETY: the process handle was opened for us (NOCLOSEPROCESS).
                    let _ = unsafe { windows::Win32::Foundation::CloseHandle(info.hProcess) };
                }
                StartOutcome::Asked
            }
            Err(e) if e.code() == HRESULT::from_win32(ERROR_CANCELLED) => StartOutcome::Cancelled,
            Err(e) => StartOutcome::Failed {
                message: e.message(),
            },
        }
    })
    .await
    .unwrap_or_else(|e| StartOutcome::Failed {
        message: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sc_path_is_absolute_in_system32() {
        let p = sc_exe_path();
        assert!(p.is_absolute(), "{p:?} should be absolute");
        assert!(
            p.to_string_lossy()
                .to_ascii_lowercase()
                .ends_with(r"\system32\sc.exe"),
            "unexpected sc path: {p:?}"
        );
    }
}
