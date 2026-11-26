//! The global hotkey (PLAN.md §11.2): Ctrl+Alt+S switches to the next
//! mode, wherever you are, and opens the tray flyout so you see which one
//! is on now. On by default; Settings → Windows turns it off or picks one
//! of a few other key combinations ([`CHOICES`]). The choice is this
//! Windows user's, so the app keeps it in its config directory: an empty
//! marker file ([`OFF_MARKER`]) means off, and [`KEYS_FILE`] holds the
//! keys when they aren't the default.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use snailnet_core::modes::{MODES, is_mode};
use snailnet_ipc::Request;
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::Shared;

/// The keys the user may pick (as the plugin parses them, and as shown).
/// A short list rather than "press any keys": these stay clear of what
/// programs use every day (Ctrl+Shift+S is Save as), so the hotkey doesn't
/// take a shortcut away from them.
pub const CHOICES: [(&str, &str); 4] = [
    ("ctrl+alt+s", "Ctrl+Alt+S"),
    ("ctrl+shift+alt+s", "Ctrl+Shift+Alt+S"),
    ("ctrl+alt+m", "Ctrl+Alt+M"),
    ("ctrl+alt+n", "Ctrl+Alt+N"),
];
/// The default keys.
pub const SHORTCUT: &str = CHOICES[0].0;
const OFF_MARKER: &str = "hotkey-off";
const KEYS_FILE: &str = "hotkey-keys";

/// The keys registered now (or to register when turned on).
#[derive(Default)]
pub struct Keys(Mutex<Option<&'static str>>);

impl Keys {
    fn get(&self) -> &'static str {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .unwrap_or(SHORTCUT)
    }

    fn set(&self, keys: &'static str) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(keys);
    }
}

pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new().build()
}

/// One of [`CHOICES`], by its keys.
fn choice(keys: &str) -> Option<&'static str> {
    let keys = keys.trim();
    CHOICES.iter().map(|(k, _)| *k).find(|k| *k == keys)
}

/// The keys saved in `file`; the default when there's none or it isn't
/// one of the choices (any more).
fn saved_keys(file: Option<&Path>) -> &'static str {
    file.and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|s| choice(&s))
        .unwrap_or(SHORTCUT)
}

/// The mode after `active`: the built-in modes the config has, in the
/// order the app shows them, then the user's own in config order; the
/// first one when `active` isn't among them.
pub fn next_mode<'a>(profiles: impl IntoIterator<Item = &'a str>, active: &str) -> Option<String> {
    let have: Vec<&str> = profiles.into_iter().collect();
    let modes: Vec<&str> = MODES
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| have.contains(id))
        .chain(have.iter().copied().filter(|id| !is_mode(id)))
        .collect();
    let next = match modes.iter().position(|m| *m == active) {
        Some(i) => modes.get((i + 1) % modes.len()),
        None => modes.first(),
    };
    next.map(|s| (*s).to_owned())
}

fn marker(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join(OFF_MARKER))
}

fn keys_file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join(KEYS_FILE))
}

fn keys(app: &AppHandle) -> &'static str {
    app.state::<Keys>().get()
}

fn is_on(marker: Option<&Path>) -> bool {
    !marker.is_some_and(Path::exists)
}

/// Register the hotkey unless the user turned it off. Another program
/// may own the keys already; then there's simply no hotkey.
pub fn setup(app: &AppHandle) {
    app.manage(Keys::default());
    app.state::<Keys>()
        .set(saved_keys(keys_file(app).as_deref()));
    if is_on(marker(app).as_deref()) {
        let _ = register(app, keys(app));
    }
}

fn register(app: &AppHandle, keys: &str) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(keys, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                cycle(app);
            }
        })
        .map_err(|e| e.to_string())
}

fn next_from_snapshot(snap: &snailnet_ipc::Snapshot) -> Option<(String, u64)> {
    let ids = snap.config.profiles.iter().map(|p| p.id.as_str());
    next_mode(ids, &snap.config.active_profile).map(|id| (id, snap.rev))
}

/// Try switching to `(id, rev)`; on a stale-revision conflict (such as rapid
/// hotkey presses before the bridge refreshes its snapshot), fetch a fresh
/// snapshot and retry once from the latest active mode and revision.
async fn switch_with_retry<S, SFut, G, GFut>(
    id: String,
    rev: u64,
    mut switch: S,
    fetch_snapshot: G,
) -> Result<u64, crate::SaveError>
where
    S: FnMut(String, u64) -> SFut,
    SFut: std::future::Future<Output = Result<u64, crate::SaveError>>,
    G: FnOnce() -> GFut,
    GFut: std::future::Future<Output = Option<snailnet_ipc::Snapshot>>,
{
    match switch(id, rev).await {
        Err(crate::SaveError::Conflict) => {
            let Some(snap) = fetch_snapshot().await else {
                return Err(crate::SaveError::Conflict);
            };
            let Some((retry_id, retry_rev)) = next_from_snapshot(&snap) else {
                return Err(crate::SaveError::Conflict);
            };
            switch(retry_id, retry_rev).await
        }
        other => other,
    }
}

/// Switch to the next mode and show the flyout without stealing focus.
fn cycle(app: &AppHandle) {
    let next = app
        .state::<Arc<Shared>>()
        .with_snapshot(next_from_snapshot)
        .flatten();
    crate::tray::show_flyout_passive(app);
    if let Some((id, rev)) = next {
        tauri::async_runtime::spawn(async move {
            // A failure leaves the old mode on, which the flyout shows.
            let _ = switch_with_retry(
                id,
                rev,
                |id, rev| crate::changes::send(Request::SwitchProfile { id, rev }),
                crate::changes::get_snapshot,
            )
            .await;
        });
    }
}

fn enabled(app: &AppHandle) -> bool {
    is_on(marker(app).as_deref()) && app.global_shortcut().is_registered(keys(app))
}

/// What Settings shows: on or not, the keys, and the keys to pick from.
#[derive(serde::Serialize)]
pub struct HotkeyInfo {
    on: bool,
    keys: &'static str,
    choices: Vec<Choice>,
}

#[derive(serde::Serialize)]
struct Choice {
    keys: &'static str,
    label: &'static str,
}

#[tauri::command]
pub fn hotkey_info(app: AppHandle) -> HotkeyInfo {
    HotkeyInfo {
        on: enabled(&app),
        keys: keys(&app),
        choices: CHOICES
            .iter()
            .map(|&(keys, label)| Choice { keys, label })
            .collect(),
    }
}

/// Use other keys; returns what Settings shows afterwards. When another
/// program holds the new keys, the old ones stay (and stay registered).
#[tauri::command]
pub fn set_hotkey_keys(app: AppHandle, keys: String) -> Result<HotkeyInfo, String> {
    let new = choice(&keys).ok_or("not one of the choices")?;
    let old = self::keys(&app);
    if new != old {
        let shortcuts = app.global_shortcut();
        if is_on(marker(&app).as_deref()) {
            if shortcuts.is_registered(old) {
                shortcuts.unregister(old).map_err(|e| e.to_string())?;
            }
            if let Err(e) = register(&app, new) {
                let _ = register(&app, old);
                return Err(e);
            }
        }
        app.state::<Keys>().set(new);
        let file = keys_file(&app).ok_or("no app config folder")?;
        if new == SHORTCUT {
            let _ = std::fs::remove_file(&file);
        } else {
            if let Some(dir) = file.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            std::fs::write(&file, new).map_err(|e| e.to_string())?;
        }
    }
    Ok(hotkey_info(app))
}

/// Turn the hotkey on or off; returns whether it's on now (it can't be
/// when another program holds the keys).
#[tauri::command]
pub fn set_hotkey(app: AppHandle, on: bool) -> Result<bool, String> {
    let marker = marker(&app).ok_or("no app config folder")?;
    let keys = keys(&app);
    if on {
        let _ = std::fs::remove_file(&marker);
        if !app.global_shortcut().is_registered(keys) {
            register(&app, keys)?;
        }
    } else {
        if let Some(dir) = marker.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&marker, b"").map_err(|e| e.to_string())?;
        if app.global_shortcut().is_registered(keys) {
            app.global_shortcut()
                .unregister(keys)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(enabled(&app))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_steps_through_the_modes_and_wraps() {
        let order: Vec<&str> = MODES.iter().map(|(id, _)| *id).collect();
        let mut at = order[0].to_owned();
        for want in order.iter().cycle().skip(1).take(order.len()) {
            at = next_mode(order.iter().copied(), &at).unwrap();
            assert_eq!(at, *want);
        }
    }

    #[test]
    fn it_skips_missing_modes_and_ends_with_the_users_own() {
        let order: Vec<&str> = MODES.iter().map(|(id, _)| *id).collect();
        let have = ["mine", order[0], order[2]];
        let next = |at: &str| next_mode(have, at);
        assert_eq!(next(order[0]).as_deref(), Some(order[2]));
        assert_eq!(
            next(order[2]).as_deref(),
            Some("mine"),
            "the user's own last"
        );
        assert_eq!(next("mine").as_deref(), Some(order[0]));
        assert_eq!(
            next("gone").as_deref(),
            Some(order[0]),
            "unknown: start over"
        );
        assert_eq!(next_mode([], "x"), None);
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

    #[test]
    fn saved_keys_must_be_a_choice() {
        let dir = std::env::temp_dir().join(format!("snailnet-hotkey-keys-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join(KEYS_FILE);
        assert_eq!(saved_keys(Some(&f)), SHORTCUT, "none saved");
        std::fs::write(&f, "ctrl+alt+m\n").unwrap();
        assert_eq!(saved_keys(Some(&f)), "ctrl+alt+m");
        std::fs::write(&f, "alt+f4").unwrap();
        assert_eq!(saved_keys(Some(&f)), SHORTCUT, "not a choice");
        assert_eq!(saved_keys(None), SHORTCUT);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_choice_parses() {
        use tauri_plugin_global_shortcut::Shortcut;
        for (keys, _) in CHOICES {
            assert!(keys.parse::<Shortcut>().is_ok(), "{keys}");
        }
    }

    #[test]
    fn conflict_fetches_fresh_snapshot_and_retries_once() {
        let mut config = snailnet_core::rules::Config::default();
        let order: Vec<&str> = MODES.iter().map(|(id, _)| *id).collect();
        config.active_profile = order[1].into();
        let fresh = snailnet_ipc::Snapshot {
            rev: 11,
            state: snailnet_ipc::ServiceState::Shaping,
            config,
            apps: Vec::new(),
            sites: Vec::new(),
            prompts: Vec::new(),
        };
        let mut seen = Vec::new();
        let mut fetched = false;
        let res = tauri::async_runtime::block_on(switch_with_retry(
            order[1].into(),
            10,
            |id, rev| {
                seen.push((id, rev));
                let call_no = seen.len();
                async move {
                    if call_no == 1 {
                        Err(crate::SaveError::Conflict)
                    } else {
                        Ok(rev + 1)
                    }
                }
            },
            || {
                fetched = true;
                let s = fresh.clone();
                async move { Some(s) }
            },
        ));
        assert!(fetched);
        assert!(matches!(res, Ok(12)));
        assert_eq!(seen, vec![(order[1].into(), 10), (order[2].into(), 11)]);
    }

    #[test]
    fn non_conflict_error_does_not_retry() {
        let mut fetched = false;
        let res = tauri::async_runtime::block_on(switch_with_retry(
            "gaming".into(),
            10,
            |_, _| async {
                Err(crate::SaveError::Failed {
                    message: "offline".into(),
                })
            },
            || {
                fetched = true;
                async { None }
            },
        ));
        assert!(!fetched);
        assert!(matches!(res, Err(crate::SaveError::Failed { .. })));
    }
}
