//! Start with Windows (PLAN.md §4, §14.5): the tray app can autostart at
//! login, minimised to the tray. Registration is the per-user `HKCU\…\Run`
//! value written by `tauri-plugin-autostart`; the command line it registers
//! carries [`MINIMIZED`], and a launch with that flag leaves the main window
//! hidden (it starts hidden, `tauri.conf.json`) instead of showing it.
//!
//! A fresh install asks once (the user's call, 2026-11-20): the main
//! window shows "Start SnailNet with Windows?" after first run, and the
//! answer leaves a marker ([`ASKED`]) in the app's config directory so it is
//! never asked again. Installs from before the question had autostart
//! turned on silently and left [`LEGACY`]; they already decided, so they
//! aren't asked. Settings and the tray menu change it any time and stay in
//! step ([`sync`]). Release builds only; in dev the question never shows
//! (it would register `target\debug` at login).

use std::path::Path;

use tauri::plugin::TauriPlugin;
use tauri::{App, AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_autostart::ManagerExt;

/// The argument the autostart entry launches the app with.
pub const MINIMIZED: &str = "--minimized";
/// Present once the user has answered the question.
const ASKED: &str = "autostart-asked";
/// Left by earlier releases, which turned autostart on without asking.
const LEGACY: &str = "autostart-default-applied";
/// Sent to the windows when autostart changes (`bool`: on).
pub const CHANGED: &str = "autostart-changed";
const MAIN: &str = "main";

/// The plugin, registering `app.exe --minimized` at login.
pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    tauri_plugin_autostart::Builder::new()
        .args([MINIMIZED])
        .build()
}

/// Whether this launch asked to stay in the tray.
pub fn launched_minimized<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().skip(1).any(|a| a.as_ref() == MINIMIZED)
}

/// Show the main window unless launched minimised.
pub fn setup(app: &App) {
    if !launched_minimized(std::env::args())
        && let Some(w) = app.get_webview_window(MAIN)
    {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Whether the question is still open in `dir`: neither answered nor
/// decided by an earlier release.
fn needs_asking(dir: &Path) -> bool {
    !dir.join(ASKED).exists() && !dir.join(LEGACY).exists()
}

/// Remember that the question was answered.
fn mark_asked(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(dir.join(ASKED), b"");
}

/// Whether the app is registered to start at login.
pub fn enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// Register or unregister the app to start at login.
pub fn set<R: Runtime>(app: &AppHandle<R>, on: bool) -> Result<(), String> {
    let al = app.autolaunch();
    if on { al.enable() } else { al.disable() }.map_err(|e| e.to_string())
}

/// Tell the tray's check and the windows what the registry says now.
pub fn sync(app: &AppHandle) -> bool {
    let on = enabled(app);
    crate::tray::sync_startup(app, on);
    let _ = app.emit(CHANGED, on);
    on
}

#[tauri::command]
pub fn autostart_enabled(app: AppHandle) -> bool {
    enabled(&app)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, on: bool) -> Result<bool, String> {
    let r = set(&app, on);
    let now = sync(&app);
    r.map(|()| now)
}

/// Whether to ask "Start SnailNet with Windows?" now (release builds, once).
#[tauri::command]
pub fn autostart_ask(app: AppHandle) -> bool {
    !cfg!(debug_assertions)
        && app
            .path()
            .app_config_dir()
            .is_ok_and(|dir| needs_asking(&dir))
}

/// The user's answer: turn autostart on (or leave it off) and never ask
/// again, even if the registry write fails (Settings then shows the truth).
#[tauri::command]
pub fn autostart_answer(app: AppHandle, on: bool) -> bool {
    if on {
        let _ = set(&app, true);
    }
    if let Ok(dir) = app.path().app_config_dir() {
        mark_asked(&dir);
    }
    sync(&app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_marks_a_minimised_launch() {
        assert!(launched_minimized(["app.exe", "--minimized"]));
        assert!(!launched_minimized(["app.exe"]));
        assert!(!launched_minimized(["app.exe", "--minimised"]));
        // The program path is not an argument.
        assert!(!launched_minimized(["--minimized"]));
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("snailnet-autostart-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_fresh_install_is_asked_once() {
        let dir = scratch("fresh");
        assert!(needs_asking(&dir));
        mark_asked(&dir);
        assert!(!needs_asking(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_earlier_install_already_decided() {
        let dir = scratch("legacy");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(LEGACY), b"").unwrap();
        assert!(!needs_asking(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
