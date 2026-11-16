//! What the tray icon shows (PLAN.md §14.4 "Tray icon states"). The icon is
//! the app icon with a mark drawn on it:
//!
//! | status    | when                                   | icon                    |
//! |-----------|----------------------------------------|-------------------------|
//! | `Normal`  | connected, shaping or watching only    | the snail               |
//! | `Warning` | safe mode, or shaping failed open      | snail + amber dot       |
//! | `Offline` | no connection to the service           | faded grey snail        |
//!
//! Paused (amber glyph) and the blocked-apps dot come with those features:
//! the service has neither yet.

use std::sync::Mutex;

use snailnet_ipc::ServiceState;
use tauri::image::Image;
use tauri::{AppHandle, Manager};

use crate::bridge::Link;

/// Id of SnailNet's one tray icon.
pub const TRAY_ID: &str = "snailnet";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Normal,
    Warning,
    Offline,
}

/// The tray's status and tooltip, from the connection and the service's
/// state (`None` until the first snapshot arrives).
pub fn status(link: &Link, state: Option<&ServiceState>) -> (Status, &'static str) {
    match (link, state) {
        (Link::Connected { .. }, Some(ServiceState::SafeMode { .. })) => {
            (Status::Warning, "SnailNet: safe mode, not shaping")
        }
        (Link::Connected { .. }, Some(ServiceState::Retrying { .. })) => {
            (Status::Warning, "SnailNet: shaping stopped, retrying")
        }
        (Link::Connected { .. }, Some(ServiceState::Unshaped)) => {
            (Status::Normal, "SnailNet: watching only")
        }
        (Link::Connected { .. }, _) => (Status::Normal, "SnailNet"),
        (Link::Connecting, _) => (Status::Offline, "SnailNet: connecting to the service"),
        _ => (Status::Offline, "SnailNet: the service isn't running"),
    }
}

/// The amber of Windows' caution glyphs.
const AMBER: [u8; 3] = [0xFF, 0xB9, 0x00];

/// Draw `status` onto an RGBA icon of `w` × `h`, in place.
pub fn paint(rgba: &mut [u8], w: u32, h: u32, status: Status) {
    match status {
        Status::Normal => {}
        Status::Offline => {
            for px in rgba.chunks_exact_mut(4) {
                let [r, g, b, a] = [px[0], px[1], px[2], px[3]].map(u32::from);
                let grey = ((r * 30 + g * 59 + b * 11) / 100) as u8;
                px[..3].fill(grey);
                px[3] = (a * 55 / 100) as u8;
            }
        }
        Status::Warning => {
            // A dot in the bottom-right corner with a clear ring around it,
            // so it reads on any taskbar colour.
            let size = w.min(h) as f64;
            let r = size * 0.22;
            let ring = (size * 0.07).max(1.0);
            let (cx, cy) = (w as f64 - r - ring, h as f64 - r - ring);
            for y in 0..h {
                for x in 0..w {
                    let d = ((x as f64 + 0.5 - cx).powi(2) + (y as f64 + 0.5 - cy).powi(2)).sqrt();
                    let i = ((y * w + x) * 4) as usize;
                    if d <= r {
                        rgba[i..i + 3].copy_from_slice(&AMBER);
                        rgba[i + 3] = 255;
                    } else if d <= r + ring {
                        rgba[i + 3] = 0;
                    }
                }
            }
        }
    }
}

/// The last status shown, so repeated updates don't redraw the icon.
#[derive(Default)]
pub struct Shown(Mutex<Option<Status>>);

/// Show `link` / `state` on the tray icon. Safe to call from any thread.
pub fn update(app: &AppHandle, link: &Link, state: Option<&ServiceState>) {
    let (status, tip) = status(link, state);
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let _ = tray.set_tooltip(Some(tip));
    let Some(shown) = app.try_state::<Shown>() else {
        return;
    };
    let mut last = shown.0.lock().unwrap_or_else(|e| e.into_inner());
    if *last == Some(status) {
        return;
    }
    let Some(base) = app.default_window_icon() else {
        return;
    };
    let (w, h) = (base.width(), base.height());
    let mut rgba = base.rgba().to_vec();
    paint(&mut rgba, w, h, status);
    if tray.set_icon(Some(Image::new_owned(rgba, w, h))).is_ok() {
        *last = Some(status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connected() -> Link {
        Link::Connected {
            service_version: "0.0.1".into(),
        }
    }

    #[test]
    fn status_follows_link_then_state() {
        assert_eq!(
            status(&connected(), Some(&ServiceState::Shaping)).0,
            Status::Normal
        );
        assert_eq!(status(&connected(), None).0, Status::Normal);
        assert_eq!(
            status(&connected(), Some(&ServiceState::SafeMode { failures: 3 })).0,
            Status::Warning
        );
        assert_eq!(
            status(
                &connected(),
                Some(&ServiceState::Retrying { reason: "x".into() })
            )
            .0,
            Status::Warning
        );
        // A stale safe-mode snapshot doesn't outrank a lost connection.
        assert_eq!(
            status(
                &Link::NotRunning,
                Some(&ServiceState::SafeMode { failures: 3 })
            )
            .0,
            Status::Offline
        );
        assert_eq!(status(&Link::Rejected, None).0, Status::Offline);
        assert_eq!(status(&Link::Connecting, None).0, Status::Offline);
    }

    fn solid(w: u32, h: u32) -> Vec<u8> {
        [10, 120, 100, 255].repeat((w * h) as usize)
    }

    fn px(rgba: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * w + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    }

    #[test]
    fn normal_leaves_the_icon_alone() {
        let mut icon = solid(32, 32);
        paint(&mut icon, 32, 32, Status::Normal);
        assert_eq!(icon, solid(32, 32));
    }

    #[test]
    fn warning_puts_an_amber_dot_bottom_right_with_a_clear_ring() {
        let mut icon = solid(32, 32);
        paint(&mut icon, 32, 32, Status::Warning);
        // Dot centre: 32 - 7.04 - 2.24 ≈ 22.7.
        assert_eq!(px(&icon, 32, 22, 22), [0xFF, 0xB9, 0x00, 255]);
        // Just outside the dot, towards the top-left: the clear ring.
        assert_eq!(px(&icon, 32, 15, 22)[3], 0);
        // Top-left untouched.
        assert_eq!(px(&icon, 32, 2, 2), [10, 120, 100, 255]);
    }

    #[test]
    fn offline_fades_to_grey() {
        let mut icon = solid(4, 4);
        paint(&mut icon, 4, 4, Status::Offline);
        let [r, g, b, a] = px(&icon, 4, 1, 1);
        assert!(r == g && g == b);
        assert_eq!(a, 140);
    }
}
