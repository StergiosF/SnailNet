//! Settings → Check for updates (PLAN.md §17.2). `tauri-plugin-updater`
//! reads `latest.json` from the GitHub release feed (tauri.conf.json) and
//! verifies the package against the public key there before running it.
//!
//! On Windows the update is the NSIS installer itself, run passively: its
//! hooks stop the service, replace the files, register and start the
//! service again, and relaunch the app. Installing exits this app.

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

/// What a check found.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdateCheck {
    /// This is the newest version.
    UpToDate,
    /// A newer version is out; `notes` are the release notes, if any.
    Available {
        version: String,
        notes: Option<String>,
    },
    /// The feed couldn't be read (offline, no release yet, a dev build).
    Failed { message: String },
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> UpdateCheck {
    let checked = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(e) => Err(e),
    };
    match checked {
        Ok(Some(update)) => UpdateCheck::Available {
            version: update.version,
            notes: update.body,
        },
        Ok(None) => UpdateCheck::UpToDate,
        Err(e) => UpdateCheck::Failed {
            message: e.to_string(),
        },
    }
}

/// Download, verify and run the update. Returns only if something went
/// wrong; on success the installer takes over and this app exits.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "SnailNet is already up to date.".to_string())?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    // Windows exits inside download_and_install; anywhere else, restart.
    app.restart()
}
