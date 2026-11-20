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
//!
//! The living snail (PLAN.md §14.4): while `Normal`, the snail shows how
//! the line is doing ([`Mood`]): calm (as above), working (line full, the
//! spiral turns cream) or struggling (line full and ping up ≥ 40 ms, a
//! coral shell). The tooltip says the same in words. No animation: the
//! icon changes only when the mood does.

use std::sync::Mutex;

use snailnet_ipc::{DirStats, PingTick, ServiceState, StatsTick};
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

/// How the line is doing, for the snail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mood {
    /// The line isn't full: the snail as always.
    #[default]
    Calm,
    /// The line is full and ping is fine: SnailNet is sharing.
    Working,
    /// The line is full and ping is well up: the line itself is in trouble.
    Struggling,
}

/// "Full": what flows is at least this share of the shaped rate (the same
/// `BUSY` as the app's ledger.ts).
const BUSY: f64 = 0.9;
/// Working after this long full.
const WORK_AFTER_MS: u64 = 3_000;
/// Calm again after this long not full.
const CALM_AFTER_MS: u64 = 10_000;
/// Struggling: ping up by at least this much …
const UP_MS: f64 = 40.0;
/// … for this long.
const STRUGGLE_AFTER_MS: u64 = 5_000;
/// Back to working once ping is up by less than this …
const DOWN_MS: f64 = 25.0;
/// … for this long.
const RECOVER_AFTER_MS: u64 = 10_000;

fn dir_full(d: Option<&DirStats>) -> bool {
    d.is_some_and(|d| {
        let total: u64 = d.classes.iter().map(|c| c.bps).sum();
        d.shaped_bps > 0 && total as f64 >= BUSY * d.shaped_bps as f64
    })
}

/// Whether the line is full (in either direction) and how far ping rose
/// above its lowest, from one tick. Only while shaping.
pub fn mood_of(tick: &StatsTick) -> (bool, Option<f64>) {
    if tick.state != ServiceState::Shaping {
        return (false, None);
    }
    let full = dir_full(tick.down.as_ref()) || dir_full(tick.up.as_ref());
    let rise = tick.ping.map(|p| (p.now_ms - p.low_ms).max(0.0));
    (full, rise)
}

/// The mood with hysteresis, fed every tick, so the icon doesn't flicker
/// at the edges.
#[derive(Debug, Default)]
pub struct MoodWatch {
    mood: Mood,
    full_since: Option<u64>,
    quiet_since: Option<u64>,
    up_since: Option<u64>,
    down_since: Option<u64>,
}

fn held(since: Option<u64>, t_ms: u64, ms: u64) -> bool {
    since.is_some_and(|s| t_ms.saturating_sub(s) >= ms)
}

fn track(slot: &mut Option<u64>, on: bool, t_ms: u64) {
    if !on {
        *slot = None;
    } else if slot.is_none() {
        *slot = Some(t_ms);
    }
}

impl MoodWatch {
    /// One tick at `t_ms`: the line `full` or not, ping `rise` (none:
    /// no ping, so never struggling). Returns the mood now.
    pub fn step(&mut self, t_ms: u64, full: bool, rise: Option<f64>) -> Mood {
        track(&mut self.full_since, full, t_ms);
        track(&mut self.quiet_since, !full, t_ms);
        track(&mut self.up_since, rise.is_some_and(|r| r >= UP_MS), t_ms);
        track(
            &mut self.down_since,
            rise.is_some_and(|r| r < DOWN_MS),
            t_ms,
        );
        self.mood = match self.mood {
            Mood::Calm if held(self.full_since, t_ms, WORK_AFTER_MS) => Mood::Working,
            Mood::Working | Mood::Struggling if held(self.quiet_since, t_ms, CALM_AFTER_MS) => {
                Mood::Calm
            }
            Mood::Working if held(self.up_since, t_ms, STRUGGLE_AFTER_MS) => Mood::Struggling,
            // No ping: nothing true to say about it.
            Mood::Struggling if rise.is_none() => Mood::Working,
            Mood::Struggling if held(self.down_since, t_ms, RECOVER_AFTER_MS) => Mood::Working,
            m => m,
        };
        self.mood
    }
}

/// What the snail says right now: its mood and the live ping.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Feel {
    pub mood: Mood,
    pub ping: Option<PingTick>,
}

/// The tooltip's first part: the mood in words. Kept short (Windows cuts
/// tray tooltips at 127 characters).
pub fn mood_tip(tip: &str, feel: Feel) -> String {
    match (feel.mood, feel.ping) {
        (Mood::Calm, _) => tip.to_owned(),
        (Mood::Working, Some(p)) => format!("{tip} · line full · ping {:.0} ms", p.now_ms),
        (Mood::Working | Mood::Struggling, None) => format!("{tip} · line full"),
        (Mood::Struggling, Some(p)) => format!(
            "{tip} · ping is up {:.0} ms · slowing the line a little to bring it down",
            (p.now_ms - p.low_ms).max(0.0)
        ),
    }
}

/// The art for a mood at about `size` px (`None`: the app icon as is).
fn mood_art(mood: Mood, size: u32) -> Option<&'static [u8]> {
    let big = size > 32;
    match mood {
        Mood::Calm => None,
        Mood::Working if big => Some(include_bytes!("../icons/tray/snail-working-64.png")),
        Mood::Working => Some(include_bytes!("../icons/tray/snail-working-32.png")),
        Mood::Struggling if big => Some(include_bytes!("../icons/tray/snail-struggling-64.png")),
        Mood::Struggling => Some(include_bytes!("../icons/tray/snail-struggling-32.png")),
    }
}

/// A PNG to RGBA8, with its size.
fn decode(png_bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(png_bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    (info.color_type == png::ColorType::Rgba).then_some((buf, info.width, info.height))
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

/// The last status, badge and mood shown, so repeated updates don't
/// redraw the icon.
#[derive(Default)]
pub struct Shown(Mutex<Option<(Status, usize, Mood)>>);

/// Show `link` / `state`, the number of blocked apps and the snail's
/// `feel` on the tray icon. Safe to call from any thread.
pub fn update(
    app: &AppHandle,
    link: &Link,
    state: Option<&ServiceState>,
    blocked: usize,
    feel: Feel,
) {
    let (status, tip) = status(link, state);
    let blocked = blocked_count(link, state, blocked);
    // The marks win: a warning or no service shows the calm snail under them.
    let feel = if status == Status::Normal {
        feel
    } else {
        Feel::default()
    };
    // Past 9 the badge shows "+": no redraw for 10 → 11.
    let badge = blocked.min(10);
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let _ = tray.set_tooltip(Some(tooltip(&mood_tip(tip, feel), blocked)));
    let Some(shown) = app.try_state::<Shown>() else {
        return;
    };
    let mut last = shown.0.lock().unwrap_or_else(|e| e.into_inner());
    if *last == Some((status, badge, feel.mood)) {
        return;
    }
    let Some(base) = app.default_window_icon() else {
        return;
    };
    let (mut rgba, w, h) = mood_art(feel.mood, base.width())
        .and_then(decode)
        .unwrap_or_else(|| (base.rgba().to_vec(), base.width(), base.height()));
    paint(&mut rgba, w, h, status);
    paint_badge(&mut rgba, w, h, badge);
    if tray.set_icon(Some(Image::new_owned(rgba, w, h))).is_ok() {
        *last = Some((status, badge, feel.mood));
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

    fn shaping_tick(down_bps: u64, ping: Option<(f64, f64)>) -> StatsTick {
        StatsTick {
            t_ms: 0,
            state: ServiceState::Shaping,
            down: Some(DirStats {
                shaped_bps: 10_000_000,
                classes: vec![snailnet_ipc::ClassTick {
                    name: "Everything else".into(),
                    bps: down_bps,
                    drops: 0,
                    queued_bytes: 0,
                }],
                families: Vec::new(),
                sites: Vec::new(),
            }),
            up: None,
            names: None,
            playing: false,
            games: Vec::new(),
            calls: Vec::new(),
            ping: ping.map(|(now_ms, low_ms)| PingTick { now_ms, low_ms }),
        }
    }

    #[test]
    fn mood_of_reads_full_and_rise() {
        assert_eq!(mood_of(&shaping_tick(9_000_000, None)), (true, None));
        assert!(!mood_of(&shaping_tick(8_999_999, None)).0);
        assert_eq!(
            mood_of(&shaping_tick(9_500_000, Some((60.0, 12.0)))),
            (true, Some(48.0))
        );
        let mut paused = shaping_tick(9_500_000, Some((60.0, 12.0)));
        paused.state = ServiceState::Paused { until: None };
        assert_eq!(mood_of(&paused), (false, None));
    }

    /// Feed `(t_ms, full, rise)` steps; the moods after each.
    fn run(steps: &[(u64, bool, Option<f64>)]) -> Vec<Mood> {
        let mut w = MoodWatch::default();
        steps.iter().map(|&(t, f, r)| w.step(t, f, r)).collect()
    }

    #[test]
    fn working_after_3_s_full_and_calm_after_10_s_quiet() {
        use Mood::*;
        let m = run(&[
            (0, true, Some(5.0)),
            (2_999, true, Some(5.0)),
            (3_000, true, Some(5.0)),
            (4_000, false, Some(5.0)),
            (13_999, false, None),
            (14_000, false, None),
        ]);
        assert_eq!(m, [Calm, Calm, Working, Working, Working, Calm]);
    }

    #[test]
    fn short_bursts_and_short_dips_do_not_flicker() {
        use Mood::*;
        // Full in bursts shorter than 3 s: never working.
        let m = run(&[
            (0, true, None),
            (2_000, false, None),
            (2_500, true, None),
            (5_000, true, None),
        ]);
        assert_eq!(m, [Calm, Calm, Calm, Calm]);
        // Working, then quiet dips shorter than 10 s: stays working.
        let m = run(&[
            (0, true, None),
            (3_000, true, None),
            (4_000, false, None),
            (13_000, true, None),
            (14_000, false, None),
            (23_000, false, None),
        ]);
        assert_eq!(m, [Calm, Working, Working, Working, Working, Working]);
    }

    #[test]
    fn struggling_after_5_s_up_and_back_after_10_s_down() {
        use Mood::*;
        let m = run(&[
            (0, true, Some(50.0)),
            (3_000, true, Some(50.0)),
            (4_999, true, Some(50.0)),
            (5_000, true, Some(50.0)),
            (6_000, true, Some(30.0)), // in between: neither clock runs
            (7_000, true, Some(10.0)),
            (16_999, true, Some(10.0)),
            (17_000, true, Some(10.0)),
        ]);
        assert_eq!(
            m,
            [
                Calm, Working, Working, Struggling, Struggling, Struggling, Struggling, Working
            ]
        );
    }

    #[test]
    fn no_ping_never_struggles() {
        use Mood::*;
        let m = run(&[(0, true, None), (3_000, true, None), (60_000, true, None)]);
        assert_eq!(m, [Calm, Working, Working]);
        let m = run(&[
            (0, true, Some(80.0)),
            (3_000, true, Some(80.0)),
            (5_000, true, Some(80.0)),
            (6_000, true, None),
        ]);
        assert_eq!(m, [Calm, Working, Struggling, Working]);
    }

    #[test]
    fn the_tooltip_says_the_mood() {
        assert_eq!(mood_tip("SnailNet", Feel::default()), "SnailNet");
        let working = Feel {
            mood: Mood::Working,
            ping: Some(PingTick {
                now_ms: 19.2,
                low_ms: 12.0,
            }),
        };
        assert_eq!(
            mood_tip("SnailNet", working),
            "SnailNet · line full · ping 19 ms"
        );
        let struggling = Feel {
            mood: Mood::Struggling,
            ping: Some(PingTick {
                now_ms: 72.4,
                low_ms: 12.0,
            }),
        };
        let t = tooltip(&mood_tip("SnailNet", struggling), 3);
        assert_eq!(
            t,
            "SnailNet · ping is up 60 ms · slowing the line a little to bring it down · 3 apps blocked"
        );
        assert!(t.chars().count() <= 127);
    }

    #[test]
    fn the_mood_art_decodes_at_both_sizes() {
        for mood in [Mood::Working, Mood::Struggling] {
            let (rgba, w, h) = decode(mood_art(mood, 32).unwrap()).unwrap();
            assert_eq!((w, h, rgba.len()), (32, 32, 32 * 32 * 4));
            let (_, w, _) = decode(mood_art(mood, 64).unwrap()).unwrap();
            assert_eq!(w, 64);
        }
        assert!(mood_art(Mood::Calm, 32).is_none());
    }
}
