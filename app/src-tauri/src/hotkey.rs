//! The global hotkey (PLAN.md §11.2): Ctrl+Alt+S switches to the next
//! mode, wherever you are, and opens the tray flyout so you see which one
//! is on now. On by default; Settings → Windows turns it off. The choice
//! is this Windows user's, so the app keeps it: an empty marker file
//! ([`OFF_MARKER`]) in the app's config directory means off.

use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError};

use snailnet_core::modes::MODES;
use snailnet_ipc::Request;
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::Shared;

/// The keys, as the plugin parses them.
pub const SHORTCUT: &str = "ctrl+alt+s";
const OFF_MARKER: &str = "hotkey-off";

pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new().build()
}

/// The mode after `active` among the modes the config has, in the order
/// the app shows them; the first one when `active` isn't among them.
pub fn next_mode<'a>(
    profiles: impl IntoIterator<Item = &'a str>,
    active: &str,
) -> Option<&'static str> {
    let have: Vec<&str> = profiles.into_iter().collect();
    let modes: Vec<&'static str> = MODES
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| have.contains(id))
        .collect();
    let at = modes.iter().position(|m| *m == active);
    match at {
        Some(i) => modes.get((i + 1) % modes.len()).copied(),
        None => modes.first().copied(),
    }
}

fn marker(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join(OFF_MARKER))
}

fn is_on(marker: Option<&Path>) -> bool {
    !marker.is_some_and(Path::exists)
}

/// Register the hotkey unless the user turned it off. Another program
/// may own Ctrl+Alt+S already; then there's simply no hotkey.
pub fn setup(app: &AppHandle) {
    if is_on(marker(app).as_deref()) {
        let _ = register(app);
    }
}

fn register(app: &AppHandle) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(SHORTCUT, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                cycle(app);
            }
        })
        .map_err(|e| e.to_string())
}

/// Switch to the next mode and show the flyout.
fn cycle(app: &AppHandle) {
    let next = {
        let shared = app.state::<Arc<Shared>>();
        let latest = shared.0.lock().unwrap_or_else(PoisonError::into_inner);
        latest.snapshot.as_ref().and_then(|s| {
            let ids = s.config.profiles.iter().map(|p| p.id.as_str());
            next_mode(ids, &s.config.active_profile).map(|id| (id, s.rev))
        })
    };
    crate::tray::show_flyout(app);
    if let Some((id, rev)) = next {
        tauri::async_runtime::spawn(async move {
            // A failure leaves the old mode on, which the flyout shows.
            let _ = crate::changes::send(Request::SwitchProfile { id: id.into(), rev }).await;
        });
    }
}

#[tauri::command]
pub fn hotkey_enabled(app: AppHandle) -> bool {
    is_on(marker(&app).as_deref()) && app.global_shortcut().is_registered(SHORTCUT)
}

/// Turn the hotkey on or off; returns whether it's on now (it can't be
/// when another program holds the keys).
#[tauri::command]
pub fn set_hotkey(app: AppHandle, on: bool) -> Result<bool, String> {
    let marker = marker(&app).ok_or("no app config folder")?;
    if on {
        let _ = std::fs::remove_file(&marker);
        if !app.global_shortcut().is_registered(SHORTCUT) {
            register(&app)?;
        }
    } else {
        if let Some(dir) = marker.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&marker, b"").map_err(|e| e.to_string())?;
        if app.global_shortcut().is_registered(SHORTCUT) {
            app.global_shortcut()
                .unregister(SHORTCUT)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(hotkey_enabled(app))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_steps_through_the_modes_and_wraps() {
        let order: Vec<&str> = MODES.iter().map(|(id, _)| *id).collect();
        let mut at = order[0];
        for want in order.iter().cycle().skip(1).take(order.len()) {
            at = next_mode(order.iter().copied(), at).unwrap();
            assert_eq!(at, *want);
        }
    }

    #[test]
    fn it_skips_modes_the_config_lacks_and_ignores_others() {
        let order: Vec<&str> = MODES.iter().map(|(id, _)| *id).collect();
        let have = [order[0], order[2], "mine"];
        assert_eq!(next_mode(have, order[0]), Some(order[2]));
        assert_eq!(next_mode(have, order[2]), Some(order[0]));
        assert_eq!(
            next_mode(have, "mine"),
            Some(order[0]),
            "not a mode: start over"
        );
        assert_eq!(next_mode(["mine"], "mine"), None);
    }

    #[test]
    fn the_marker_means_off() {
        let dir = std::env::temp_dir().join(format!("snailnet-hotkey-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let m = dir.join(OFF_MARKER);
        assert!(is_on(Some(&m)));
        std::fs::write(&m, b"").unwrap();
        assert!(!is_on(Some(&m)));
        assert!(is_on(None), "no config folder: on");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
