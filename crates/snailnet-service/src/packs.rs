//! The pack feed (PLAN.md §17.2): newer site, app-family and game lists
//! without a new build. Once a day (first 2 minutes after start) the
//! service GETs `packs.toml` and its minisign signature from the rolling
//! GitHub release `packs`, checks the signature against the updater key
//! compiled in ([`snailnet_core::packs`]), and keeps it only if it is
//! newer than what runs. Nothing is sent but the two requests themselves.
//!
//! A kept bundle is saved in the data folder, so the next start uses it
//! at once (checked again), and the shaper restarts once to pick up the
//! new families and sites. Anything wrong (offline, 404 while the repo
//! is private, a bad signature) leaves the packs as they are: the
//! compiled-in ones, or the last good bundle.
//!
//! [`Settings::pack_updates`](snailnet_core::rules::Settings) off: no
//! requests at all; a saved bundle is still used.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use snailnet_core::packs::{self, FEED_HOST, FEED_PATH, FILE, MAX_BUNDLE, Packs, SIG_FILE};
use snailnet_win::http::Https;
use tracing::{info, warn};

use crate::hub::Hub;
use crate::paths::DataDir;

/// The first check, after the service has settled.
const FIRST: Duration = Duration::from_secs(120);
/// After a check that worked, whatever it found.
const EVERY: Duration = Duration::from_secs(24 * 3600);
/// After a check that failed (offline, the host down).
const RETRY: Duration = Duration::from_secs(3 * 3600);
/// While the setting is off: just look at it again.
const OFF: Duration = Duration::from_secs(3600);

/// A signature file is a few hundred bytes.
const MAX_SIG: usize = 4096;

/// Set when newer packs were installed; the runner restarts shaping once
/// so the classifier and site matcher are built from them.
static RESTART: AtomicBool = AtomicBool::new(false);

/// True (once) if the packs changed since the last call.
pub fn take_restart() -> bool {
    RESTART.swap(false, Ordering::SeqCst)
}

/// Use the bundle saved by an earlier run, if it checks out and is newer
/// than the compiled-in packs. Call before loading the config.
pub fn load_saved(dir: &DataDir) {
    match packs::read_saved(dir.root()) {
        Ok(None) => {}
        Ok(Some(p)) => {
            let serial = p.serial;
            if packs::install(p) {
                info!("packs: using the saved lists, serial {serial}");
            } else {
                info!("packs: the saved lists (serial {serial}) are older than this build's");
            }
        }
        Err(e) => warn!("packs: ignoring the saved lists: {e}"),
    }
}

/// Run until the hub's stop signal.
pub fn watch(hub: &Hub) {
    let stop = hub.signal();
    let mut wait = FIRST;
    loop {
        if stop.wait(wait) {
            return;
        }
        if !hub.config().settings.pack_updates {
            wait = OFF;
            continue;
        }
        wait = match fetch(hub.dir()) {
            Ok(Some(serial)) => {
                info!("packs: new site, app and game lists, serial {serial}");
                RESTART.store(true, Ordering::SeqCst);
                hub.packs_changed();
                EVERY
            }
            Ok(None) => EVERY,
            // Nothing published (or the repo not public yet): not a fault.
            Err(e) if format!("{e:#}").contains("HTTP 404") => {
                info!("packs: no lists published yet");
                EVERY
            }
            Err(e) => {
                warn!("packs: could not check for new lists: {e:#}");
                RETRY
            }
        };
    }
}

/// Fetch, check and save the bundle. `Some(serial)` if it was newer and
/// is now in use.
fn fetch(dir: &DataDir) -> Result<Option<u64>> {
    let agent = concat!("SnailNet/", env!("CARGO_PKG_VERSION"));
    let https = Https::new(agent, FEED_HOST).context("opening WinHTTP")?;
    let body = https
        .get(&format!("{FEED_PATH}{FILE}"), MAX_BUNDLE)
        .with_context(|| format!("GET {FILE}"))?;
    let sig = https
        .get(&format!("{FEED_PATH}{SIG_FILE}"), MAX_SIG)
        .with_context(|| format!("GET {SIG_FILE}"))?;
    let sig = String::from_utf8(sig).map_err(|_| anyhow!("{SIG_FILE} is not text"))?;
    keep(dir, &body, &sig)
}

/// Check a fetched bundle; if newer than the packs in use, save it and
/// use it.
fn keep(dir: &DataDir, body: &[u8], sig: &str) -> Result<Option<u64>> {
    let p = Packs::from_signed(body, sig)?;
    let serial = p.serial;
    if serial <= packs::current().serial.max(packs::builtin_serial()) {
        return Ok(None);
    }
    save(dir, body, sig).context("saving the lists")?;
    Ok(packs::install(p).then_some(serial))
}

/// Write both files beside the final ones, then move them in, signature
/// first. A crash in between leaves a pair that fails the check on the
/// next start: the packs fall back and the next fetch puts it right.
fn save(dir: &DataDir, body: &[u8], sig: &str) -> std::io::Result<()> {
    let tmp = |p: &std::path::Path| p.with_extension("tmp-new");
    let (b, s) = (dir.packs(), dir.packs_sig());
    std::fs::write(tmp(&s), sig)?;
    std::fs::write(tmp(&b), body)?;
    std::fs::rename(tmp(&s), &s)?;
    std::fs::rename(tmp(&b), &b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bad_bundle_is_neither_saved_nor_used() {
        let t = tempfile::tempdir().unwrap();
        let d = DataDir::new(t.path());
        let before = packs::current().serial;
        assert!(keep(&d, b"format = 1\nserial = 99999999999999", "junk").is_err());
        assert!(!d.packs().exists());
        assert_eq!(packs::current().serial, before);
        // Nothing saved: nothing loaded.
        load_saved(&d);
        assert_eq!(packs::current().serial, before);
    }

    #[test]
    fn a_saved_bundle_that_fails_its_check_is_ignored() {
        let t = tempfile::tempdir().unwrap();
        let d = DataDir::new(t.path());
        std::fs::write(d.packs(), "format = 1\nserial = 99999999999999").unwrap();
        std::fs::write(d.packs_sig(), "junk").unwrap();
        let before = packs::current().serial;
        load_saved(&d);
        assert_eq!(packs::current().serial, before);
    }
}
