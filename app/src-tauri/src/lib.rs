//! The SnailNet tray app (PLAN.md §4): a Tauri 2 shell around the Svelte
//! UI. The Rust side talks to the service over the pipe ([`bridge`]) and
//! pushes what it hears to the webview as events:
//!
//! | event          | payload                         |
//! |----------------|---------------------------------|
//! | `link`         | [`bridge::Link`]                |
//! | `snapshot`     | `snailnet_ipc::Snapshot`        |
//! | `service-state`| `snailnet_ipc::ServiceState`    |
//! | `stats`        | `snailnet_ipc::StatsTick`       |
//! | `stats-catchup`| `Vec<snailnet_ipc::StatsTick>`  |
//!
//! The webview may load after the first events went out, so [`current`]
//! returns the latest of each.

pub mod autostart;
pub mod bridge;
pub mod changes;
pub mod focus;
pub mod hotkey;
pub mod icon;
pub mod metered;
pub mod service_start;
pub mod tray;
pub mod tray_icon;
pub mod updates;

use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;
use snailnet_ipc::{Client, PIPE_NAME, ServiceState, Snapshot, StatsTick};
use tauri::{AppHandle, Emitter, Manager};

use bridge::{Link, Sink};

/// Hidden-window timeline sampling interval: half of the UI's 5 s step so
/// no step is missed while the main window stays asleep in the tray.
const MISSED_STEP_MS: u64 = 2_500;
/// Ten minutes of timeline samples at [`MISSED_STEP_MS`].
const MAX_MISSED_STATS: usize = 240;

/// The latest of everything the bridge reported.
#[derive(Debug, Clone, Serialize)]
pub struct Latest {
    pub link: Link,
    pub snapshot: Option<Snapshot>,
    pub stats: Option<StatsTick>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missed_stats: Vec<StatsTick>,
}

impl Default for Latest {
    fn default() -> Self {
        Self {
            link: Link::Connecting,
            snapshot: None,
            stats: None,
            missed_stats: Vec::new(),
        }
    }
}

#[derive(Default)]
struct SharedState {
    latest: Latest,
    /// Downsampled ticks accumulated while the main window is hidden, so
    /// the timeline catches up cleanly when it reopens.
    missed_stats: VecDeque<StatsTick>,
    /// `t_ms` of the last tick appended to `missed_stats`.
    last_missed_ms: Option<u64>,
    /// True if at least one stats tick was skipped while `"main"` was hidden
    /// or minimized (even if shorter than `MISSED_STEP_MS`).
    main_missed_any: bool,
}

impl SharedState {
    fn clear_missed(&mut self) {
        self.missed_stats.clear();
        self.last_missed_ms = None;
        self.main_missed_any = false;
    }

    fn record_missed(&mut self, tick: &StatsTick) {
        self.main_missed_any = true;
        if self.last_missed_ms.is_some_and(|t| tick.t_ms < t) {
            self.missed_stats.clear();
            self.last_missed_ms = None;
        }
        if self
            .last_missed_ms
            .is_none_or(|t| tick.t_ms.saturating_sub(t) >= MISSED_STEP_MS)
        {
            if self.missed_stats.len() >= MAX_MISSED_STATS {
                self.missed_stats.pop_front();
            }
            self.missed_stats.push_back(tick.clone());
            self.last_missed_ms = Some(tick.t_ms);
        }
    }

    fn drain_missed(&mut self) -> Vec<StatsTick> {
        self.last_missed_ms = None;
        self.main_missed_any = false;
        self.missed_stats.drain(..).collect()
    }
}

#[derive(Default)]
pub(crate) struct Shared {
    state: Mutex<SharedState>,
    /// Serialises `stats-catchup` / `stats` delivery to the main window so
    /// `flush_window` and the bridge thread can never interleave out of order.
    main_emit: Mutex<()>,
}

/// The config's revision and whether SnailNet is paused, while connected:
/// what the tray menu's Pause / Resume needs (PLAN.md §14.4).
pub(crate) fn pause_state(app: &AppHandle) -> Option<(u64, bool)> {
    let shared = app.try_state::<Arc<Shared>>()?;
    let s = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
    if !matches!(s.latest.link, Link::Connected { .. }) {
        return None;
    }
    let snap = s.latest.snapshot.as_ref()?;
    Some((snap.rev, matches!(snap.state, ServiceState::Paused { .. })))
}

impl Shared {
    fn update(&self, f: impl FnOnce(&mut SharedState)) {
        f(&mut self.state.lock().unwrap_or_else(PoisonError::into_inner));
    }

    pub(crate) fn with_snapshot<R>(&self, f: impl FnOnce(&Snapshot) -> R) -> Option<R> {
        let s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        s.latest.snapshot.as_ref().map(f)
    }
}

/// Push buffered and latest stats to a window as it becomes visible so it
/// renders up-to-date data immediately instead of waiting for the next tick.
pub(crate) fn flush_window(app: &AppHandle, label: &str) {
    let Some(shared) = app.try_state::<Arc<Shared>>() else {
        return;
    };
    match label {
        tray::MAIN => {
            let _emit_guard = shared
                .main_emit
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let (missed, latest) = {
                let mut s = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
                let latest = s.latest.stats.clone();
                let mut missed = s.drain_missed();
                if let Some(ref t) = latest
                    && missed.last().is_some_and(|m| t.t_ms < m.t_ms)
                {
                    missed.clear();
                }
                (missed, latest)
            };
            if !missed.is_empty() {
                let _ = app.emit_to(tray::MAIN, "stats-catchup", missed);
            }
            if let Some(tick) = latest {
                let _ = app.emit_to(tray::MAIN, "stats", tick);
            }
        }
        tray::FLYOUT => {
            let latest = shared
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .latest
                .stats
                .clone();
            if let Some(tick) = latest {
                let _ = app.emit_to(tray::FLYOUT, "stats", tick);
            }
        }
        _ => {}
    }
}

/// Flush `"main"` only if it missed stats while hidden or minimized (e.g.
/// when restored from the Windows taskbar or opened from the flyout UI).
pub(crate) fn flush_if_missed(app: &AppHandle) {
    let Some(shared) = app.try_state::<Arc<Shared>>() else {
        return;
    };
    let should_flush = {
        let s = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
        s.main_missed_any || !s.missed_stats.is_empty()
    };
    if should_flush {
        flush_window(app, tray::MAIN);
    }
}

struct TauriSink {
    app: AppHandle,
    shared: Arc<Shared>,
    snail: Mutex<Snail>,
}

/// The tray snail's mood (PLAN.md §14.4), fed by the stats.
#[derive(Default)]
struct Snail {
    watch: tray_icon::MoodWatch,
    feel: tray_icon::Feel,
    /// When the tooltip's ping was last refreshed (`t_ms`).
    tip_at: u64,
}

/// The tooltip's ping changes at most this often.
const TIP_EVERY_MS: u64 = 2_000;

impl TauriSink {
    fn emit<T: Serialize + Clone>(&self, event: &str, payload: &T) {
        // Only fails when the app is shutting down.
        let _ = self.app.emit(event, payload.clone());
    }

    fn window_visible(&self, label: &str) -> bool {
        self.app
            .get_webview_window(label)
            .map(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false))
            .unwrap_or(false)
    }

    /// Bring the tray icon in line with the latest link and state.
    fn refresh_tray(&self) {
        let (link, state, blocked) = {
            let s = self
                .shared
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            (
                s.latest.link.clone(),
                s.latest.snapshot.as_ref().map(|s| s.state.clone()),
                s.latest
                    .snapshot
                    .as_ref()
                    .map_or(0, |s| s.config.blocked.len()),
            )
        };
        let feel = self
            .snail
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .feel;
        tray::sync_pause(
            &self.app,
            matches!(state, Some(ServiceState::Paused { .. })),
        );
        tray_icon::update(&self.app, &link, state.as_ref(), blocked, feel);
    }
}

impl Sink for TauriSink {
    fn link(&self, link: &Link) {
        self.shared.update(|s| {
            s.latest.link = link.clone();
            if !matches!(link, Link::Connected { .. }) {
                s.latest.stats = None;
                s.clear_missed();
            }
        });
        if !matches!(link, Link::Connected { .. }) {
            *self.snail.lock().unwrap_or_else(PoisonError::into_inner) = Snail::default();
        }
        self.emit("link", link);
        self.refresh_tray();
    }
    fn snapshot(&self, snap: &Snapshot) {
        self.shared
            .update(|s| s.latest.snapshot = Some(snap.clone()));
        self.emit("snapshot", snap);
        self.refresh_tray();
    }
    fn state(&self, state: &ServiceState) {
        self.shared.update(|s| {
            if let Some(snap) = &mut s.latest.snapshot {
                snap.state = state.clone();
            }
        });
        self.emit("service-state", state);
        self.refresh_tray();
    }
    fn stats(&self, tick: &StatsTick) {
        // Avoid waking hidden WebView2 windows at 4 Hz: buffer downsampled
        // ticks for the main window's 10-minute timeline while it sits in
        // the tray, and only emit live ticks to visible windows.
        if self.window_visible(tray::MAIN) {
            let _emit_guard = self
                .shared
                .main_emit
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let missed = {
                let mut s = self
                    .shared
                    .state
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                s.latest.stats = Some(tick.clone());
                let missed = s.drain_missed();
                if missed.last().is_some_and(|m| tick.t_ms < m.t_ms) {
                    Vec::new()
                } else {
                    missed
                }
            };
            if !missed.is_empty() {
                let _ = self.app.emit_to(tray::MAIN, "stats-catchup", missed);
            }
            let _ = self.app.emit_to(tray::MAIN, "stats", tick.clone());
        } else {
            self.shared.update(|s| {
                s.latest.stats = Some(tick.clone());
                s.record_missed(tick);
            });
        }
        if self.window_visible(tray::FLYOUT) {
            let _ = self.app.emit_to(tray::FLYOUT, "stats", tick.clone());
        }
        // Redraw the tray only when the mood changes; the tooltip's ping
        // at most every couple of seconds.
        let redraw = {
            let mut s = self.snail.lock().unwrap_or_else(PoisonError::into_inner);
            let (full, rise) = tray_icon::mood_of(tick);
            let mood = s.watch.step(tick.t_ms, full, rise);
            let moved = mood != s.feel.mood;
            let tip_due =
                mood != tray_icon::Mood::Calm && tick.t_ms.saturating_sub(s.tip_at) >= TIP_EVERY_MS;
            if moved || tip_due {
                s.feel = tray_icon::Feel {
                    mood,
                    ping: tick.ping,
                };
                s.tip_at = tick.t_ms;
            }
            moved || tip_due
        };
        if redraw {
            self.refresh_tray();
        }
    }
}

/// Why a change didn't save, for the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SaveError {
    /// Someone else changed the rules first; the UI gets the new snapshot
    /// through the bridge and the user tries again.
    Conflict,
    /// The service said no (`message` says why) or couldn't be reached.
    Failed { message: String },
}

#[tauri::command]
fn current(window: tauri::WebviewWindow, shared: tauri::State<'_, Arc<Shared>>) -> Latest {
    if window.label() == tray::MAIN {
        // Hold `main_emit` so `current()` cannot interleave with an in-flight
        // `flush_window` or `TauriSink::stats` catchup delivery.
        let _emit_guard = shared
            .main_emit
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut s = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
        let mut latest = s.latest.clone();
        let mut missed = s.drain_missed();
        if let Some(ref t) = latest.stats
            && missed.last().is_some_and(|m| t.t_ms < m.t_ms)
        {
            missed.clear();
        }
        latest.missed_stats = missed;
        latest
    } else {
        shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .latest
            .clone()
    }
}

pub fn run() {
    let shared = Arc::new(Shared::default());
    tauri::Builder::default()
        // First, so a second copy hands over before doing anything else.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::open_main(app);
        }))
        .plugin(autostart::plugin())
        // The one notification: "still running in the tray" (tray.rs).
        .plugin(tauri_plugin_notification::init())
        .plugin(hotkey::plugin())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(shared.clone())
        .manage(icon::Icons::default())
        .setup(move |app| {
            // A stopped SnailNet comes back as the app opens (no prompt).
            service_start::start_if_stopped();
            // Before the tray, so its "Start with Windows" check is current.
            autostart::setup(app);
            tray::setup(app)?;
            hotkey::setup(app.handle());
            let sink = TauriSink {
                app: app.handle().clone(),
                shared,
                snail: Mutex::default(),
            };
            let stop = Arc::new(AtomicBool::new(false));
            app.manage(stop.clone());
            focus::spawn(stop.clone())?;
            std::thread::Builder::new()
                .name("service-link".into())
                .spawn(move || {
                    let version = env!("CARGO_PKG_VERSION");
                    bridge::run(
                        || Client::connect_with(PIPE_NAME, version, &[]),
                        &sink,
                        &stop,
                        std::thread::sleep,
                    );
                })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            current,
            icon::app_icon,
            changes::set_rules,
            changes::set_families,
            changes::set_line,
            changes::switch_profile,
            changes::reset_profile,
            changes::copy_profile,
            changes::rename_profile,
            changes::delete_profile,
            changes::reset_all,
            changes::run_speed_test,
            changes::boost,
            changes::stop_boost,
            changes::pause,
            changes::resume,
            changes::set_blocked,
            changes::set_settings,
            autostart::autostart_enabled,
            autostart::set_autostart,
            autostart::autostart_ask,
            autostart::autostart_answer,
            metered::network_metered,
            hotkey::hotkey_info,
            hotkey::set_hotkey,
            hotkey::set_hotkey_keys,
            service_start::start_service,
            updates::check_update,
            updates::install_update
        ])
        .run(tauri::generate_context!())
        .expect("error while running the SnailNet app");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick(t_ms: u64) -> StatsTick {
        StatsTick {
            t_ms,
            state: ServiceState::Shaping,
            down: None,
            up: None,
            names: None,
            playing: false,
            games: Vec::new(),
            calls: Vec::new(),
            front: None,
            ping: None,
        }
    }

    #[test]
    fn missed_stats_samples_every_2500_ms_and_drains_once() {
        let mut s = SharedState::default();
        for t in [1_000, 1_250, 2_000, 3_499, 3_500, 5_999, 6_000] {
            s.record_missed(&tick(t));
        }
        let got: Vec<u64> = s.drain_missed().into_iter().map(|t| t.t_ms).collect();
        assert_eq!(got, [1_000, 3_500, 6_000]);
        assert!(s.drain_missed().is_empty());
        assert_eq!(s.last_missed_ms, None);
    }

    #[test]
    fn missed_stats_clears_when_clock_goes_backwards() {
        let mut s = SharedState::default();
        s.record_missed(&tick(10_000));
        s.record_missed(&tick(12_500));
        // Service restarted: t_ms reset to near zero.
        s.record_missed(&tick(250));
        s.record_missed(&tick(2_750));
        let got: Vec<u64> = s.drain_missed().into_iter().map(|t| t.t_ms).collect();
        assert_eq!(got, [250, 2_750]);
    }

    #[test]
    fn missed_stats_caps_at_240_entries() {
        let mut s = SharedState::default();
        for i in 0..300u64 {
            s.record_missed(&tick(i * MISSED_STEP_MS));
        }
        let got = s.drain_missed();
        assert_eq!(got.len(), MAX_MISSED_STATS);
        assert_eq!(got.first().unwrap().t_ms, 60 * MISSED_STEP_MS);
        assert_eq!(got.last().unwrap().t_ms, 299 * MISSED_STEP_MS);
    }

    #[test]
    fn latest_omits_empty_missed_stats_in_json() {
        let empty = serde_json::to_value(Latest::default()).unwrap();
        assert!(empty.get("missed_stats").is_none());

        let with = Latest {
            missed_stats: vec![tick(250)],
            ..Latest::default()
        };
        let v = serde_json::to_value(with).unwrap();
        assert_eq!(v["missed_stats"].as_array().unwrap().len(), 1);
    }
}
