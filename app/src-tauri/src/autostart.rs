//! Start with Windows (PLAN.md §4, §14.5): the tray app autostarts at
//! login, minimised to the tray. Registration is the per-user `HKCU\…\Run`
//! value written by `tauri-plugin-autostart`; the command line it registers
//! carries [`MINIMIZED`], and a launch with that flag leaves the main window
//! hidden (it starts hidden, `tauri.conf.json`) instead of showing it.
//!
//! PLAN.md §15 has the installer register autostart. Until there is one,
//! the app does it itself, once: the first launch turns autostart on and
//! leaves a marker file ([`MARKER`]) in the app's config directory, so a
//! user who later turns it off (tray menu "Start with Windows") stays off.
//! Release builds only; in dev the tray item still turns it on by hand.

use std::path::Path;

use tauri::plugin::TauriPlugin;
use tauri::{App, AppHandle, Manager, Runtime};
use tauri_plugin_autostart::ManagerExt;

/// The argument the autostart entry launches the app with.
pub const MINIMIZED: &str = "--minimized";
/// Present once the app has turned autostart on by default.
const MARKER: &str = "autostart-default-applied";
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

/// First-run default, then show the main window unless launched minimised.
pub fn setup(app: &App) {
    // Not for dev builds: `tauri dev` would register `target\debug` at login.
    if !cfg!(debug_assertions)
        && let Ok(dir) = app.path().app_config_dir()
    {
        apply_default_once(&dir, || set(app.handle(), true));
    }
    if !launched_minimized(std::env::args())
        && let Some(w) = app.get_webview_window(MAIN)
    {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Run `enable` unless `dir` already holds the marker; write the marker
/// once `enable` succeeds, so a failed attempt is retried next launch.
fn apply_default_once(dir: &Path, enable: impl FnOnce() -> Result<(), String>) -> bool {
    let marker = dir.join(MARKER);
    if marker.exists() || enable().is_err() {
        return false;
    }
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(&marker, b"");
    true
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

#[tauri::command]
pub fn autostart_enabled(app: AppHandle) -> bool {
    enabled(&app)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, on: bool) -> Result<bool, String> {
    set(&app, on)?;
    Ok(enabled(&app))
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
    fn the_default_applies_once() {
        let dir = scratch("once");
        let mut calls = 0;
        assert!(apply_default_once(&dir, || {
            calls += 1;
            Ok(())
        }));
        assert!(!apply_default_once(&dir, || {
            calls += 1;
            Ok(())
        }));
        assert_eq!(calls, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_default_is_retried() {
        let dir = scratch("retry");
        assert!(!apply_default_once(&dir, || Err("denied".into())));
        assert!(apply_default_once(&dir, || Ok(())));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
