//! The tray icon and its flyout (PLAN.md §14.4). SnailNet lives in the
//! tray: closing the main window hides it, the tray's menu has "Open",
//! Pause / Resume (always: the window shows Pause only while SnailNet is
//! doing something, the user's choice 2026-11-20) and "Quit", and a left
//! click toggles the flyout, a small borderless window
//! anchored above the tray icon (Windows 11 quick-panel style) that hides
//! when it loses focus. The flyout loads the same page as the main window;
//! the page picks its view from the window label ([`FLYOUT`]).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::bridge::Link;
use crate::tray_icon;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    App, AppHandle, Manager, PhysicalPosition, Rect, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

/// Window label of the flyout.
pub const FLYOUT: &str = "flyout";
const MAIN: &str = "main";
/// Logical size of the flyout (PLAN.md §14.4: 380 × 500).
const FLYOUT_W: f64 = 380.0;
const FLYOUT_H: f64 = 500.0;
/// Logical gap between the flyout and the work area's edge.
const GAP: f64 = 12.0;
/// A tray click this soon after the flyout hid on blur is the click that
/// blurred it: don't open it again.
const REOPEN_GUARD: Duration = Duration::from_millis(250);

/// The tray menu's Pause / Resume item, renamed as the service pauses.
pub struct PauseItem(MenuItem<tauri::Wry>);

const PAUSE_LABEL: &str = "Pause SnailNet for 1 hour";
const RESUME_LABEL: &str = "Resume SnailNet";

/// Name the tray menu's item for the state SnailNet is in.
pub fn sync_pause(app: &AppHandle, paused: bool) {
    if let Some(item) = app.try_state::<PauseItem>() {
        let _ = item
            .0
            .set_text(if paused { RESUME_LABEL } else { PAUSE_LABEL });
    }
}

/// The tray menu's "Start with Windows" check, kept in step with Settings.
pub struct StartupItem(tauri::menu::CheckMenuItem<tauri::Wry>);

/// Check the tray's "Start with Windows" as the registry says.
pub fn sync_startup(app: &AppHandle, on: bool) {
    if let Some(item) = app.try_state::<StartupItem>() {
        let _ = item.0.set_checked(on);
    }
}

/// Present once the "still running in the tray" notification was shown.
const TRAY_NOTICE: &str = "tray-notice-shown";

/// The first time the main window is closed, say once, in a Windows
/// notification, that SnailNet keeps running in the tray (the user's call,
/// 2026-11-20, the one notification SnailNet shows). Never again after.
fn first_close_notice(app: &AppHandle) {
    let Ok(dir) = app.path().app_config_dir() else {
        return;
    };
    if !first_time(&dir.join(TRAY_NOTICE)) {
        return;
    }
    use tauri_plugin_notification::NotificationExt;
    let _ = app
        .notification()
        .builder()
        .title("SnailNet is still running")
        .body("It keeps sharing your internet from the tray. Click the snail there to open it, or right-click it to quit.")
        .show();
}

/// True the first time it's called for `marker`, which it then creates.
fn first_time(marker: &std::path::Path) -> bool {
    if marker.exists() {
        return false;
    }
    if let Some(dir) = marker.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(marker, b"");
    true
}

/// When the flyout last hid because it lost focus.
#[derive(Default)]
struct Blurred(Mutex<Option<Instant>>);

/// Guards against clicking Quit twice while the service is stopping.
#[derive(Default)]
struct Quitting(AtomicBool);

/// Maximum time Quit waits for the service to acknowledge shutdown.
const QUIT_STOP_TIMEOUT: Duration = Duration::from_secs(3);

pub fn setup(app: &mut App) -> tauri::Result<()> {
    app.manage(Blurred::default());
    app.manage(Quitting::default());
    app.manage(tray_icon::Shown::default());

    let flyout = WebviewWindowBuilder::new(app, FLYOUT, WebviewUrl::App("index.html".into()))
        .title("SnailNet")
        .inner_size(FLYOUT_W, FLYOUT_H)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .shadow(true)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible(false)
        .build()?;
    let handle = app.handle().clone();
    flyout.on_window_event(move |e| {
        if let WindowEvent::Focused(false) = e {
            hide_flyout(&handle, true);
        }
    });

    // Closing the main window keeps SnailNet in the tray, and says so the
    // first time.
    if let Some(main) = app.get_webview_window(MAIN) {
        let w = main.clone();
        let handle = app.handle().clone();
        main.on_window_event(move |e| {
            if let WindowEvent::CloseRequested { api, .. } = e {
                api.prevent_close();
                let _ = w.hide();
                first_close_notice(&handle);
            }
        });
    }

    let open = MenuItem::with_id(app, "open", "Open SnailNet", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", PAUSE_LABEL, true, None::<&str>)?;
    app.manage(PauseItem(pause.clone()));
    let startup = tauri::menu::CheckMenuItem::with_id(
        app,
        "startup",
        "Start with Windows",
        true,
        crate::autostart::enabled(app.handle()),
        None::<&str>,
    )?;
    app.manage(StartupItem(startup.clone()));
    let sep = tauri::menu::PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit SnailNet", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &pause, &startup, &sep, &quit])?;
    let mut tray = TrayIconBuilder::with_id(tray_icon::TRAY_ID)
        .tooltip("SnailNet")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, e| match e.id().as_ref() {
            "open" => open_main(app),
            // Pause for an hour, or resume: the same requests as the
            // window's, against the latest revision. Not connected: nothing
            // to pause (the item says Pause and does nothing).
            "pause" => {
                if let Some((rev, paused)) = crate::pause_state(app) {
                    tauri::async_runtime::spawn(async move {
                        let _ = if paused {
                            crate::changes::resume(rev).await
                        } else {
                            crate::changes::pause(Some(60), rev).await
                        };
                    });
                }
            }
            // The click already flipped the check; follow it, then show
            // what the registry really says (the write can fail), here and
            // in Settings.
            "startup" => {
                let want = startup.is_checked().unwrap_or(false);
                let _ = crate::autostart::set(app, want);
                crate::autostart::sync(app);
            }
            // Quit stops snailnet-service cleanly before exiting the app.
            "quit" => quit_app(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = e
            {
                toggle_flyout(tray.app_handle(), rect);
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    // Faded until the bridge reaches the service.
    tray_icon::update(
        app.handle(),
        &Link::Connecting,
        None,
        0,
        tray_icon::Feel::default(),
    );
    Ok(())
}

/// Show, unminimise and focus the main window; hide the flyout.
pub fn open_main(app: &AppHandle) {
    hide_flyout(app, false);
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Hide the UI right away, ask `snailnet-service` to shut down cleanly,
/// and exit the tray app. Bounded by [`QUIT_STOP_TIMEOUT`] so Quit can
/// never hang even if the service is unresponsive.
fn quit_app(app: &AppHandle) {
    if let Some(q) = app.try_state::<Quitting>()
        && q.0.swap(true, Ordering::Relaxed)
    {
        return;
    }
    if let Some(stop) = app.try_state::<Arc<AtomicBool>>() {
        stop.store(true, Ordering::Relaxed);
    }
    hide_flyout(app, false);
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.hide();
    }
    if let Some(t) = app.tray_by_id(tray_icon::TRAY_ID) {
        let _ = t.set_visible(false);
    }
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            crate::changes::stop_service_blocking();
            let _ = tx.send(());
        });
        let _ = tauri::async_runtime::spawn_blocking(move || {
            let _ = rx.recv_timeout(QUIT_STOP_TIMEOUT);
        })
        .await;
        handle.exit(0);
    });
}

fn hide_flyout(app: &AppHandle, blurred: bool) {
    if let Some(w) = app.get_webview_window(FLYOUT)
        && w.is_visible().unwrap_or(false)
    {
        let _ = w.hide();
        if blurred {
            *app.state::<Blurred>()
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
        }
    }
}

fn toggle_flyout(app: &AppHandle, icon: Rect) {
    let Some(w) = app.get_webview_window(FLYOUT) else {
        return;
    };
    if w.is_visible().unwrap_or(false) {
        hide_flyout(app, false);
        return;
    }
    let recently = app
        .state::<Blurred>()
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
        .is_some_and(|t| t.elapsed() < REOPEN_GUARD);
    if recently {
        return;
    }
    let _ = place(&w, icon);
    let _ = w.show();
    let _ = w.set_focus();
}

/// Open the flyout by the tray icon (the hotkey); already open, it stays.
pub fn show_flyout(app: &AppHandle) {
    let Some(w) = app.get_webview_window(FLYOUT) else {
        return;
    };
    if w.is_visible().unwrap_or(false) {
        return;
    }
    if let Some(icon) = app
        .tray_by_id(tray_icon::TRAY_ID)
        .and_then(|t| t.rect().ok().flatten())
    {
        let _ = place(&w, icon);
    }
    let _ = w.show();
    let _ = w.set_focus();
}

/// Put the flyout next to the tray icon, inside the icon's monitor's work
/// area (above the taskbar, or below it when the taskbar is at the top).
fn place(w: &WebviewWindow, icon: Rect) -> tauri::Result<()> {
    let scale = w.scale_factor()?;
    let pos: PhysicalPosition<f64> = icon.position.to_physical(scale);
    let size: tauri::PhysicalSize<f64> = icon.size.to_physical(scale);
    let Some(monitor) = w.monitor_from_point(pos.x, pos.y)? else {
        return Ok(());
    };
    let s = monitor.scale_factor();
    let work = monitor.work_area();
    let (x, y) = flyout_origin(
        Area {
            x: pos.x,
            y: pos.y,
            w: size.width,
            h: size.height,
        },
        Area {
            x: work.position.x.into(),
            y: work.position.y.into(),
            w: work.size.width.into(),
            h: work.size.height.into(),
        },
        (FLYOUT_W * s, FLYOUT_H * s),
        GAP * s,
    );
    w.set_size(tauri::LogicalSize::new(FLYOUT_W, FLYOUT_H))?;
    w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32))
}

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Area {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Top-left corner for a `size` flyout: centred on the icon, kept `gap`
/// inside the work area; at the work area's bottom when the icon is in its
/// lower half (taskbar at the bottom, or a side taskbar's lower part),
/// else at its top.
fn flyout_origin(icon: Area, work: Area, size: (f64, f64), gap: f64) -> (f64, f64) {
    let (w, h) = size;
    let centre = icon.x + icon.w / 2.0 - w / 2.0;
    let x = centre.min(work.x + work.w - w - gap).max(work.x + gap);
    let lower = icon.y + icon.h / 2.0 > work.y + work.h / 2.0;
    let y = if lower {
        work.y + work.h - h - gap
    } else {
        work.y + gap
    };
    (x, y.max(work.y))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Area = Area {
        x: 0.0,
        y: 0.0,
        w: 1920.0,
        h: 1032.0,
    };

    #[test]
    fn sits_above_a_bottom_taskbar_centred_on_the_icon() {
        let icon = Area {
            x: 1700.0,
            y: 1040.0,
            w: 32.0,
            h: 40.0,
        };
        assert_eq!(
            flyout_origin(icon, WORK, (360.0, 420.0), 12.0),
            (1536.0, 600.0)
        );
    }

    #[test]
    fn stays_inside_the_work_area_near_the_corner() {
        let icon = Area {
            x: 1880.0,
            y: 1040.0,
            w: 32.0,
            h: 40.0,
        };
        assert_eq!(
            flyout_origin(icon, WORK, (360.0, 420.0), 12.0),
            (1548.0, 600.0)
        );
    }

    #[test]
    fn hangs_below_a_top_taskbar() {
        let work = Area { y: 48.0, ..WORK };
        let icon = Area {
            x: 1700.0,
            y: 4.0,
            w: 32.0,
            h: 40.0,
        };
        assert_eq!(
            flyout_origin(icon, work, (360.0, 420.0), 12.0),
            (1536.0, 60.0)
        );
    }

    #[test]
    fn a_second_monitor_left_of_the_primary() {
        let work = Area {
            x: -2560.0,
            y: 0.0,
            w: 2560.0,
            h: 1392.0,
        };
        let icon = Area {
            x: -100.0,
            y: 1400.0,
            w: 40.0,
            h: 48.0,
        };
        let (x, y) = flyout_origin(icon, work, (540.0, 630.0), 18.0);
        assert_eq!((x, y), (-558.0, 744.0));
    }

    #[test]
    fn the_tray_notice_is_shown_once() {
        let dir = std::env::temp_dir().join(format!("snailnet-tray-notice-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let marker = dir.join(TRAY_NOTICE);
        assert!(first_time(&marker));
        assert!(!first_time(&marker));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
