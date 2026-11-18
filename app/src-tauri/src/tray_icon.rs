//! What the tray icon shows (PLAN.md §14.4 "Tray icon states"). The icon is
//! the app icon with a mark drawn on it:
//!
//! | status    | when                                   | icon                    |
//! |-----------|----------------------------------------|-------------------------|
//! | `Normal`  | connected, shaping or watching only    | the snail               |
//! | `Warning` | safe mode, failed open, or paused      | snail + amber dot       |
//! | `Offline` | no connection to the service           | faded grey snail        |
//!
//! Blocked apps (PLAN.md §11.3): a count badge in the top-right corner
//! (1–9, then "+") and "· N apps blocked" in the tooltip, while connected
//! and not paused.

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
        // PLAN.md §14.4: paused is the amber dot too.
        (Link::Connected { .. }, Some(ServiceState::Paused { .. })) => {
            (Status::Warning, "SnailNet: paused")
        }
        (Link::Connected { .. }, Some(ServiceState::Unshaped)) => {
            (Status::Normal, "SnailNet: watching only")
        }
        (Link::Connected { .. }, _) => (Status::Normal, "SnailNet"),
        (Link::Connecting, _) => (Status::Offline, "SnailNet: connecting to the service"),
        _ => (Status::Offline, "SnailNet: the service isn't running"),
    }
}

/// How many apps to show as blocked: none while paused (nothing is) or
/// while the service can't be reached (the count would be stale).
pub fn blocked_count(link: &Link, state: Option<&ServiceState>, blocked: usize) -> usize {
    match (link, state) {
        (Link::Connected { .. }, Some(ServiceState::Paused { .. })) => 0,
        (Link::Connected { .. }, _) => blocked,
        _ => 0,
    }
}

/// The tooltip, with the blocked count.
pub fn tooltip(tip: &str, blocked: usize) -> String {
    match blocked {
        0 => tip.to_owned(),
        1 => format!("{tip} · 1 app blocked"),
        n => format!("{tip} · {n} apps blocked"),
    }
}

/// 3 × 5 glyphs for the badge, one row per byte (bit 2 = left column).
const GLYPHS: [[u8; 5]; 11] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0 (unused)
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b111, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b010, 0b010, 0b010], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b111], // 9
    [0b000, 0b010, 0b111, 0b010, 0b000], // + (more than 9)
];

/// The badge: a near-black disc with a white figure (reads on light and
/// dark taskbars; red would say "error").
const BADGE: [u8; 3] = [0x20, 0x20, 0x20];
const FIGURE: [u8; 3] = [0xFF, 0xFF, 0xFF];

/// Draw the blocked-apps badge (`count` ≥ 1) in the top-right corner.
pub fn paint_badge(rgba: &mut [u8], w: u32, h: u32, count: usize) {
    if count == 0 {
        return;
    }
    let glyph = GLYPHS[count.min(10)];
    let size = w.min(h) as f64;
    let r = size * 0.26;
    let ring = (size * 0.05).max(1.0);
    let (cx, cy) = (w as f64 - r - ring, r + ring);
    // The figure: 3 × 5 cells, about the disc's inner square.
    let cell = (r * 1.3 / 5.0).max(1.0);
    let (gx, gy) = (cx - 1.5 * cell, cy - 2.5 * cell);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
            let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
            let i = ((y * w + x) * 4) as usize;
            if d <= r {
                let (col, row) = ((fx - gx) / cell, (fy - gy) / cell);
                let lit = (0.0..3.0).contains(&col)
                    && (0.0..5.0).contains(&row)
                    && glyph[row as usize] & (0b100 >> col as usize) != 0;
                rgba[i..i + 3].copy_from_slice(if lit { &FIGURE } else { &BADGE });
                rgba[i + 3] = 255;
            } else if d <= r + ring {
                rgba[i + 3] = 0;
            }
        }
    }
}

/// The amber of Windows' caution glyphs.
const AMBER: [u8; 3] = [0xFF, 0xB9, 0x00];

/// Draw `status` onto an RGBA icon of `w` × `h`, in place.
pub fn paint(rgba: &mut [u8], w: u32, h: u32, status: Status) {
    match status {
        Status::Normal => {}
        Status::Offline => {
            for px in rgba.as_chunks_mut::<4>().0 {
                let [r, g, b, a] = px.map(u32::from);
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

/// The last status and badge shown, so repeated updates don't redraw the
/// icon.
#[derive(Default)]
pub struct Shown(Mutex<Option<(Status, usize)>>);

/// Show `link` / `state` and the number of blocked apps on the tray icon.
/// Safe to call from any thread.
pub fn update(app: &AppHandle, link: &Link, state: Option<&ServiceState>, blocked: usize) {
    let (status, tip) = status(link, state);
    let blocked = blocked_count(link, state, blocked);
    // Past 9 the badge shows "+": no redraw for 10 → 11.
    let badge = blocked.min(10);
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let _ = tray.set_tooltip(Some(tooltip(tip, blocked)));
    let Some(shown) = app.try_state::<Shown>() else {
        return;
    };
    let mut last = shown.0.lock().unwrap_or_else(|e| e.into_inner());
    if *last == Some((status, badge)) {
        return;
    }
    let Some(base) = app.default_window_icon() else {
        return;
    };
    let (w, h) = (base.width(), base.height());
    let mut rgba = base.rgba().to_vec();
    paint(&mut rgba, w, h, status);
    paint_badge(&mut rgba, w, h, badge);
    if tray.set_icon(Some(Image::new_owned(rgba, w, h))).is_ok() {
        *last = Some((status, badge));
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
        assert_eq!(
            status(&connected(), Some(&ServiceState::Paused { until: None })),
            (Status::Warning, "SnailNet: paused")
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
    fn the_blocked_count_shows_only_while_it_holds() {
        let c = connected();
        assert_eq!(blocked_count(&c, Some(&ServiceState::Shaping), 2), 2);
        assert_eq!(blocked_count(&c, Some(&ServiceState::Unshaped), 1), 1);
        assert_eq!(
            blocked_count(&c, Some(&ServiceState::Paused { until: None }), 2),
            0
        );
        assert_eq!(blocked_count(&Link::NotRunning, None, 2), 0);
        assert_eq!(tooltip("SnailNet", 0), "SnailNet");
        assert_eq!(tooltip("SnailNet", 1), "SnailNet · 1 app blocked");
        assert_eq!(
            tooltip("SnailNet: watching only", 3),
            "SnailNet: watching only · 3 apps blocked"
        );
    }

    #[test]
    fn the_badge_is_a_dark_disc_top_right_with_a_white_figure() {
        let mut icon = solid(32, 32);
        paint_badge(&mut icon, 32, 32, 0);
        assert_eq!(icon, solid(32, 32), "no badge without blocked apps");
        paint_badge(&mut icon, 32, 32, 1);
        let disc: Vec<[u8; 4]> = (14..32)
            .flat_map(|x| (0..18).map(move |y| (x, y)))
            .map(|(x, y)| px(&icon, 32, x, y))
            .collect();
        assert!(disc.contains(&[0xFF, 0xFF, 0xFF, 255]), "a lit figure");
        assert!(disc.contains(&[0x20, 0x20, 0x20, 255]), "the disc");
        // The bottom corners stay (bottom-right is the warning dot's).
        assert_eq!(px(&icon, 32, 2, 29), [10, 120, 100, 255]);
        assert_eq!(px(&icon, 32, 29, 29), [10, 120, 100, 255]);
        // The figure really comes from the glyphs.
        let mut seven = solid(32, 32);
        paint_badge(&mut seven, 32, 32, 7);
        assert_ne!(icon, seven);
        let (mut many, mut ten) = (solid(32, 32), solid(32, 32));
        paint_badge(&mut many, 32, 32, 12);
        paint_badge(&mut ten, 32, 32, 10);
        assert_eq!(many, ten, "anything past 9 is +");
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
