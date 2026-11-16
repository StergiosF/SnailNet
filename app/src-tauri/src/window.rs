//! Window polish (PLAN.md §14.1): the system accent colour.
//!
//! Read from WinRT `UISettings` (the same source Windows Settings and WinUI
//! use) and pushed to the page as the `system` event whenever
//! `ColorValuesChanged` fires with new values. [`system`] returns the
//! latest for a page that loads late.
//!
//! No Mica: wry hosts WebView2 in a child window, and a transparent page
//! showed plain white there instead of the backdrop (seen 2026-11-16, also
//! with the dark variant named). The page paints Fluent's solid layer
//! colour instead, which is what Windows itself shows when Mica is off.
//!
//! | event    | payload    |
//! |----------|------------|
//! | `system` | [`System`] |

use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;
use tauri::{App, AppHandle, Emitter, Manager};
use windows::Foundation::TypedEventHandler;
use windows::UI::Color;
use windows::UI::ViewManagement::{UIColorType, UISettings};
use windows::core::IInspectable;

/// The user's accent colour and the shades Fluent derives from it, as
/// `#rrggbb`. Light theme uses `dark1` for fills and `dark2` for text, dark
/// theme `light2` and `light3` (WinUI's `AccentFillColorDefault` /
/// `AccentTextFillColorPrimary`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Accent {
    pub accent: String,
    pub dark1: String,
    pub dark2: String,
    pub dark3: String,
    pub light1: String,
    pub light2: String,
    pub light3: String,
}

/// Everything the page needs to match the system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct System {
    /// `None` when `UISettings` can't be read; the page keeps its defaults.
    pub accent: Option<Accent>,
}

fn hex(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.R, c.G, c.B)
}

fn read_accent(s: &UISettings) -> windows::core::Result<Accent> {
    let get = |t: UIColorType| s.GetColorValue(t).map(hex);
    Ok(Accent {
        accent: get(UIColorType::Accent)?,
        dark1: get(UIColorType::AccentDark1)?,
        dark2: get(UIColorType::AccentDark2)?,
        dark3: get(UIColorType::AccentDark3)?,
        light1: get(UIColorType::AccentLight1)?,
        light2: get(UIColorType::AccentLight2)?,
        light3: get(UIColorType::AccentLight3)?,
    })
}

/// The latest [`System`], shared with the `ColorValuesChanged` handler.
pub struct Shared(Arc<Mutex<System>>);

/// Keeps `UISettings` (and with it the change subscription) alive for the
/// life of the app.
struct Watch {
    _settings: UISettings,
}

/// Read the accent, start watching for changes.
pub fn setup(app: &App) {
    let settings = UISettings::new().ok();
    let system = System {
        accent: settings.as_ref().and_then(|s| read_accent(s).ok()),
    };
    let shared = Arc::new(Mutex::new(system));
    app.manage(Shared(shared.clone()));

    let Some(settings) = settings else { return };
    let handle: AppHandle = app.handle().clone();
    let handler = TypedEventHandler::<UISettings, IInspectable>::new(move |sender, _| {
        // Fires on a background thread, often several times per change.
        let Some(s) = sender.as_ref() else {
            return Ok(());
        };
        let accent = read_accent(s).ok();
        let changed = {
            let mut cur = shared.lock().unwrap_or_else(PoisonError::into_inner);
            if cur.accent == accent {
                None
            } else {
                cur.accent = accent;
                Some(cur.clone())
            }
        };
        if let Some(system) = changed {
            // Only fails when the app is shutting down.
            let _ = handle.emit("system", system);
        }
        Ok(())
    });
    // Without the subscription the accent is still right at start-up.
    if settings.ColorValuesChanged(&handler).is_ok() {
        app.manage(Watch {
            _settings: settings,
        });
    }
}

/// The latest accent, for a page that loads after start-up.
#[tauri::command]
pub fn system(shared: tauri::State<'_, Shared>) -> System {
    shared
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_lowercase_hex_without_alpha() {
        let c = Color {
            A: 0x80,
            R: 0x00,
            G: 0x5f,
            B: 0xB8,
        };
        assert_eq!(hex(c), "#005fb8");
    }

    #[test]
    fn serializes_for_the_page() {
        let s = System { accent: None };
        assert_eq!(serde_json::to_string(&s).unwrap(), r#"{"accent":null}"#);
    }

    #[test]
    fn reads_the_live_accent() {
        // Real UISettings on the test machine: every shade is a colour.
        let s = UISettings::new().unwrap();
        let a = read_accent(&s).unwrap();
        for c in [
            a.accent, a.dark1, a.dark2, a.dark3, a.light1, a.light2, a.light3,
        ] {
            assert!(c.len() == 7 && c.starts_with('#'), "{c}");
        }
    }
}
