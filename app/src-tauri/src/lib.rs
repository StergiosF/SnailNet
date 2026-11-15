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

pub mod bridge;

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
    }
    fn snapshot(&self, snap: &Snapshot) {
        self.shared.update(|l| l.snapshot = Some(snap.clone()));
        self.emit("snapshot", snap);
    }
    fn state(&self, state: &ServiceState) {
        self.shared.update(|l| {
            if let Some(s) = &mut l.snapshot {
                s.state = state.clone();
            }
        });
        self.emit("service-state", state);
    }
    fn stats(&self, tick: &StatsTick) {
        self.shared.update(|l| l.stats = Some(tick.clone()));
        self.emit("stats", tick);
    }
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
        .manage(shared.clone())
        .setup(move |app| {
            let sink = TauriSink {
                app: app.handle().clone(),
                shared,
            };
            let stop = Arc::new(AtomicBool::new(false));
            app.manage(stop.clone());
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
        .invoke_handler(tauri::generate_handler![current])
        .run(tauri::generate_context!())
        .expect("error while running the SnailNet app");
}
