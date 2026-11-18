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

impl Shared {
    fn update(&self, f: impl FnOnce(&mut Latest)) {
        f(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner));
    }
}

struct TauriSink {
    app: AppHandle,
    shared: Arc<Shared>,
}

impl TauriSink {
    fn emit<T: Serialize + Clone>(&self, event: &str, payload: &T) {
        // Only fails when the app is shutting down.
        let _ = self.app.emit(event, payload.clone());
    }

    /// Bring the tray icon in line with the latest link and state.
    fn refresh_tray(&self) {
        let (link, state) = {
            let l = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
            (l.link.clone(), l.snapshot.as_ref().map(|s| s.state.clone()))
        };
        tray_icon::update(&self.app, &link, state.as_ref());
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
        .manage(shared.clone())
        .manage(icon::Icons::default())
        .setup(move |app| {
            // Before the tray, so its "Start with Windows" check is current.
            autostart::setup(app);
            tray::setup(app)?;
            hotkey::setup(app.handle());
            let sink = TauriSink {
                app: app.handle().clone(),
                shared,
            };
            let stop = Arc::new(AtomicBool::new(false));
            app.manage(stop.clone());
            focus::spawn(stop.clone())?;
            std::thread::Builder::new()
                .name("service-link".into())
                .spawn(move || {
                    let version = env!("CARGO_PKG_VERSION");
                    bridge::run(
                        || Client::connect(PIPE_NAME, version),
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
            changes::set_line,
            changes::switch_profile,
            changes::reset_profile,
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
            hotkey::hotkey_enabled,
            hotkey::set_hotkey,
            service_start::start_service
        ])
        .run(tauri::generate_context!())
        .expect("error while running the SnailNet app");
}
