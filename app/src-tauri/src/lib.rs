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

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;
use snailnet_ipc::{Client, PIPE_NAME, ServiceState, Snapshot, StatsTick};
use tauri::{AppHandle, Emitter, Manager};

use bridge::{Link, Sink};

/// The latest of everything the bridge reported.
#[derive(Debug, Clone, Serialize)]
pub struct Latest {
    pub link: Link,
    pub snapshot: Option<Snapshot>,
    pub stats: Option<StatsTick>,
}

impl Default for Latest {
    fn default() -> Self {
        Self {
            link: Link::Connecting,
            snapshot: None,
            stats: None,
        }
    }
}

#[derive(Default)]
struct Shared(Mutex<Latest>);

/// The config's revision and whether SnailNet is paused, while connected:
/// what the tray menu's Pause / Resume needs (PLAN.md §14.4).
pub(crate) fn pause_state(app: &AppHandle) -> Option<(u64, bool)> {
    let shared = app.try_state::<Arc<Shared>>()?;
    let l = shared.0.lock().unwrap_or_else(PoisonError::into_inner);
    if !matches!(l.link, Link::Connected { .. }) {
        return None;
    }
    let s = l.snapshot.as_ref()?;
    Some((s.rev, matches!(s.state, ServiceState::Paused { .. })))
}

impl Shared {
    fn update(&self, f: impl FnOnce(&mut Latest)) {
        f(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner));
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

    /// Bring the tray icon in line with the latest link and state.
    fn refresh_tray(&self) {
        let (link, state, blocked) = {
            let l = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
            (
                l.link.clone(),
                l.snapshot.as_ref().map(|s| s.state.clone()),
                l.snapshot.as_ref().map_or(0, |s| s.config.blocked.len()),
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
        self.shared.update(|l| {
            l.link = link.clone();
            if !matches!(link, Link::Connected { .. }) {
                l.stats = None;
            }
        });
        if !matches!(link, Link::Connected { .. }) {
            *self.snail.lock().unwrap_or_else(PoisonError::into_inner) = Snail::default();
        }
        self.emit("link", link);
        self.refresh_tray();
    }
    fn snapshot(&self, snap: &Snapshot) {
        self.shared.update(|l| l.snapshot = Some(snap.clone()));
        self.emit("snapshot", snap);
        self.refresh_tray();
    }
    fn state(&self, state: &ServiceState) {
        self.shared.update(|l| {
            if let Some(s) = &mut l.snapshot {
                s.state = state.clone();
            }
        });
        self.emit("service-state", state);
        self.refresh_tray();
    }
    fn stats(&self, tick: &StatsTick) {
        self.shared.update(|l| l.stats = Some(tick.clone()));
        self.emit("stats", tick);
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
fn current(shared: tauri::State<'_, Arc<Shared>>) -> Latest {
    shared
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

pub fn run() {
    let shared = Arc::new(Shared::default());
    tauri::Builder::default()
        // First, so a second copy hands over before doing anything else.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::open_main(app);
        }))
        .plugin(autostart::plugin())
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
