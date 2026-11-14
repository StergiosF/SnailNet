//! Logs (PLAN.md §13.1): `logs\service.YYYY-MM-DD.log`, a new file each day
//! (UTC), the newest 7 kept. `SNAILNET_LOG` overrides the level filter
//! (`debug`, `snailnet_service=trace`, …); the default is `info`.
//!
//! Writes are synchronous. The service logs a few lines a minute, and a
//! synchronous writer can't lose the last lines before a crash (the panic
//! hook logs, then aborts).

use std::path::Path;

use anyhow::{Context, Result};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

pub const KEEP_DAYS: usize = 7;

/// Log to `logs_dir`, and also to the terminal when `console` is set.
pub fn init(logs_dir: &Path, console: bool) -> Result<()> {
    let file = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("service")
        .filename_suffix("log")
        .max_log_files(KEEP_DAYS)
        .build(logs_dir)
        .with_context(|| format!("opening the log folder {}", logs_dir.display()))?;
    let filter = EnvFilter::try_from_env("SNAILNET_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(file).with_ansi(false))
        .with(console.then(|| fmt::layer().with_writer(std::io::stderr)))
        .try_init()
        .context("setting up logging")?;
    Ok(())
}

/// Log a panic, then abort the whole process. A half-dead service is worse
/// than a dead one: abort closes every WinDivert handle (traffic flows
/// normally), the next start counts the crash, and the service manager
/// restarts us.
pub fn abort_on_panic() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        tracing::error!(
            thread = thread.name().unwrap_or("?"),
            "panic: {info}; aborting (traffic flows normally until the service restarts)"
        );
        default(info);
        std::process::abort();
    }));
}
