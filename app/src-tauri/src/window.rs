//! Window polish (PLAN.md §14.1): the system accent colour and which
//! backdrop the window really has.
//!
//! - **Accent:** read from WinRT `UISettings` (the same source Windows
//!   Settings and WinUI use) and pushed to the page as the `system` event
//!   whenever `ColorValuesChanged` fires with new values. [`system`]
//!   returns the latest for a page that loads late.
//! - **Backdrop:** the window is transparent with Mica (`tauri.conf.json`).
//!   Mica needs Windows 11 (build 22000); on Windows 10 Tauri's call fails
//!   silently and the desktop would show through, so the page is told to
//!   paint a solid layer colour instead ([`Backdrop::Solid`]).
//!
//! | event    | payload    |
//! |----------|------------|
//! | `system` | [`System`] |

use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;
use tauri::{App, AppHandle, Emitter, Manager};
use windows::Foundation::TypedEventHandler;
use windows::System::Profile::AnalyticsInfo;
use windows::UI::Color;
use windows::UI::ViewManagement::{UIColorType, UISettings};
use windows::core::IInspectable;

/// First Windows build with Mica (Windows 11 21H2).
pub const MICA_MIN_BUILD: u32 = 22000;

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

/// What sits behind the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Backdrop {
    /// Windows 11: Mica shows through the transparent page.
    Mica,
    /// Windows 10 (or unknown): the page paints its own solid layer colour.
    Solid,
}

/// Everything the page needs to match the system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct System {
    /// `None` when `UISettings` can't be read; the page keeps its defaults.
    pub accent: Option<Accent>,
    pub backdrop: Backdrop,
}

/// The Windows build from `AnalyticsVersionInfo.DeviceFamilyVersion`, a
/// decimal u64 packing major.minor.build.revision as four u16s.
pub fn parse_build(device_family_version: &str) -> Option<u32> {
    let v: u64 = device_family_version.trim().parse().ok()?;
    Some(((v >> 16) & 0xFFFF) as u32)
}

/// Mica when the build has it; solid when it doesn't or is unknown (a
/// solid window is plainer, a see-through one is broken).
pub fn backdrop_for(build: Option<u32>) -> Backdrop {
    match build {
        Some(b) if b >= MICA_MIN_BUILD => Backdrop::Mica,
        _ => Backdrop::Solid,
    }
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

fn os_build() -> Option<u32> {
    let v = AnalyticsInfo::VersionInfo()
        .and_then(|i| i.DeviceFamilyVersion())
        .ok()?;
    parse_build(&v.to_string_lossy())
}

/// The latest [`System`], shared with the `ColorValuesChanged` handler.
pub struct Shared(Arc<Mutex<System>>);

/// Keeps `UISettings` (and with it the change subscription) alive for the
/// life of the app.
struct Watch {
    _settings: UISettings,
}

/// Read the accent and backdrop, start watching for accent changes.
pub fn setup(app: &App) {
    let settings = UISettings::new().ok();
    let system = System {
        accent: settings.as_ref().and_then(|s| read_accent(s).ok()),
        backdrop: backdrop_for(os_build()),
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

/// The latest accent and backdrop, for a page that loads after start-up.
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

    fn packed(major: u64, minor: u64, build: u64, rev: u64) -> String {
        ((major << 48) | (minor << 32) | (build << 16) | rev).to_string()
    }

    #[test]
    fn parses_the_build_out_of_device_family_version() {
        assert_eq!(parse_build(&packed(10, 0, 22631, 4460)), Some(22631));
        assert_eq!(parse_build(&packed(10, 0, 19045, 5011)), Some(19045));
        assert_eq!(
            parse_build(&format!(" {} ", packed(10, 0, 19041, 1))),
            Some(19041)
        );
        assert_eq!(parse_build(""), None);
        assert_eq!(parse_build("10.0.22631"), None);
    }

    #[test]
    fn mica_only_from_windows_11() {
        assert_eq!(backdrop_for(Some(22000)), Backdrop::Mica);
        assert_eq!(backdrop_for(Some(26100)), Backdrop::Mica);
        assert_eq!(backdrop_for(Some(21999)), Backdrop::Solid);
        assert_eq!(backdrop_for(Some(19045)), Backdrop::Solid);
        assert_eq!(backdrop_for(None), Backdrop::Solid);
    }

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
        let s = System {
            accent: None,
            backdrop: Backdrop::Solid,
        };
        assert_eq!(
            serde_json::to_string(&s).unwrap(),
            r#"{"accent":null,"backdrop":"solid"}"#
        );
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
        assert!(os_build().is_some_and(|b| b >= 10240));
    }
}
