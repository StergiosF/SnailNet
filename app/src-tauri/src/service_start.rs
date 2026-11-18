//! "Start SnailNet" on the not-running notice (PLAN.md §14.8): ask Windows
//! to start the service, elevated. Starting a service needs admin rights,
//! so this runs `sc.exe start SnailNet` through the UAC prompt; the bridge
//! reconnects by itself once the service answers.

use serde::Serialize;
use windows::Win32::System::Services::{
    CloseServiceHandle, OpenSCManagerW, OpenServiceW, SC_MANAGER_CONNECT, SERVICE_QUERY_STATUS,
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

/// ERROR_CANCELLED: the UAC prompt was declined.
const ERROR_CANCELLED: u32 = 1223;

#[tauri::command]
pub async fn start_service() -> StartOutcome {
    tauri::async_runtime::spawn_blocking(|| {
        if !installed() {
            return StartOutcome::NotInstalled;
        }
        let params = HSTRING::from(format!("start {SERVICE_NAME}"));
        let mut info = SHELLEXECUTEINFOW {
            cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            lpVerb: w!("runas"),
            lpFile: w!("sc.exe"),
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
