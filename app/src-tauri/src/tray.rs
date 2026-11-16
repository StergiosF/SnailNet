//! The tray icon and its flyout (PLAN.md §14.4). SnailNet lives in the
//! tray: closing the main window hides it, the tray's menu has "Open" and
//! "Quit", and a left click toggles the flyout, a small borderless window
//! anchored above the tray icon (Windows 11 quick-panel style) that hides
//! when it loses focus. The flyout loads the same page as the main window;
//! the page picks its view from the window label ([`FLYOUT`]).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::bridge::Link;
use crate::tray_icon;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::utils::WindowEffect;
use tauri::utils::config::WindowEffectsConfig;
use tauri::{
    App, AppHandle, Manager, PhysicalPosition, Rect, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

/// Window label of the flyout.
pub const FLYOUT: &str = "flyout";
const MAIN: &str = "main";
/// Logical size of the flyout (PLAN.md §14.4: 360 × ~420).
const FLYOUT_W: f64 = 360.0;
const FLYOUT_H: f64 = 420.0;
/// Logical gap between the flyout and the work area's edge.
const GAP: f64 = 12.0;
/// A tray click this soon after the flyout hid on blur is the click that
/// blurred it: don't open it again.
const REOPEN_GUARD: Duration = Duration::from_millis(250);

/// When the flyout last hid because it lost focus.
#[derive(Default)]
struct Blurred(Mutex<Option<Instant>>);

pub fn setup(app: &mut App) -> tauri::Result<()> {
    app.manage(Blurred::default());
    app.manage(tray_icon::Shown::default());

    let flyout = WebviewWindowBuilder::new(app, FLYOUT, WebviewUrl::App("index.html".into()))
        .title("SnailNet")
        .inner_size(FLYOUT_W, FLYOUT_H)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible(false)
        .effects(WindowEffectsConfig {
            effects: vec![WindowEffect::Mica],
            ..Default::default()
        })
        .build()?;
    let handle = app.handle().clone();
    flyout.on_window_event(move |e| {
        if let WindowEvent::Focused(false) = e {
            hide_flyout(&handle, true);
        }
    });

    // Closing the main window keeps SnailNet in the tray.
    if let Some(main) = app.get_webview_window(MAIN) {
        let w = main.clone();
        main.on_window_event(move |e| {
            if let WindowEvent::CloseRequested { api, .. } = e {
                api.prevent_close();
                let _ = w.hide();
            }
        });
    }

    let open = MenuItem::with_id(app, "open", "Open SnailNet", true, None::<&str>)?;
    let startup = tauri::menu::CheckMenuItem::with_id(
        app,
        "startup",
        "Start with Windows",
        true,
        crate::autostart::enabled(app.handle()),
        None::<&str>,
    )?;
    let sep = tauri::menu::PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit SnailNet", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &startup, &sep, &quit])?;
    let mut tray = TrayIconBuilder::with_id(tray_icon::TRAY_ID)
        .tooltip("SnailNet")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, e| match e.id().as_ref() {
            "open" => open_main(app),
            // The click already flipped the check; follow it, then show
            // what the registry really says (the write can fail).
            "startup" => {
                let want = startup.is_checked().unwrap_or(false);
                let _ = crate::autostart::set(app, want);
                let _ = startup.set_checked(crate::autostart::enabled(app));
            }
            // The service keeps shaping; only the app goes.
            "quit" => app.exit(0),
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
    tray_icon::update(app.handle(), &Link::Connecting, None);
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
}
