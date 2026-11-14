//! Crash-loop detection (PLAN.md §6.4): after 3 crashes in 10 minutes the
//! service starts in **Safe mode**: monitor only, no shaping.
//!
//! A crash is noticed on the *next* start: the service writes a `running`
//! marker when it starts and deletes it on a clean stop, so a marker that's
//! still there means the last run died (panic, kill, power loss). A
//! fail-open while running (stalled scheduler, driver error) counts the
//! same way. Times are kept in `crashes.txt`, one Unix time per line, and
//! entries older than the window are dropped.

use std::io;
use std::path::PathBuf;

use crate::paths::DataDir;

/// How far back crashes count.
pub const WINDOW_SECS: u64 = 10 * 60;
/// This many crashes inside the window → Safe mode.
pub const LIMIT: usize = 3;

pub struct CrashGuard {
    marker: PathBuf,
    log: PathBuf,
    /// Unix seconds of recent crashes and fail-opens.
    times: Vec<u64>,
}

impl CrashGuard {
    /// Call once at startup. Returns the guard and whether the previous run
    /// ended without a clean stop.
    pub fn start(dir: &DataDir, now: u64) -> io::Result<(Self, bool)> {
        let marker = dir.running_marker();
        let log = dir.crash_log();
        let times = match std::fs::read_to_string(&log) {
            Ok(t) => t.lines().filter_map(|l| l.trim().parse().ok()).collect(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        let crashed = marker.exists();
        let mut guard = Self { marker, log, times };
        if crashed {
            guard.times.push(now);
        }
        guard.prune(now);
        guard.persist()?;
        std::fs::write(&guard.marker, std::process::id().to_string())?;
        Ok((guard, crashed))
    }

    /// Count a fail-open (or a failure to start shaping) at `now`.
    pub fn record_failure(&mut self, now: u64) -> io::Result<()> {
        self.times.push(now);
        self.prune(now);
        self.persist()
    }

    /// Crashes and fail-opens inside the window ending at `now`.
    pub fn recent(&self, now: u64) -> usize {
        self.times
            .iter()
            .filter(|&&t| t + WINDOW_SECS > now && t <= now)
            .count()
    }

    pub fn safe_mode(&self, now: u64) -> bool {
        self.recent(now) >= LIMIT
    }

    /// The service is stopping normally: this run doesn't count as a crash.
    pub fn clean_exit(self) -> io::Result<()> {
        match std::fs::remove_file(&self.marker) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    fn prune(&mut self, now: u64) {
        // A clock set backwards leaves "future" times; drop those too.
        self.times.retain(|&t| t + WINDOW_SECS > now && t <= now);
    }

    fn persist(&self) -> io::Result<()> {
        let text: String = self.times.iter().map(|t| format!("{t}\n")).collect();
        std::fs::write(&self.log, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> (tempfile::TempDir, DataDir) {
        let t = tempfile::tempdir().unwrap();
        let d = DataDir::new(t.path());
        d.create().unwrap();
        (t, d)
    }

    /// Start, then die without a clean exit.
    fn crash(d: &DataDir, now: u64) -> bool {
        let (_g, crashed) = CrashGuard::start(d, now).unwrap();
        crashed
    }

    #[test]
    fn clean_runs_never_count() {
        let (_t, d) = dir();
        for i in 0..10 {
            let (g, crashed) = CrashGuard::start(&d, 1_000 + i).unwrap();
            assert!(!crashed);
            assert!(!g.safe_mode(1_000 + i));
            g.clean_exit().unwrap();
        }
    }

    #[test]
    fn three_crashes_in_ten_minutes_mean_safe_mode() {
        let (_t, d) = dir();
        assert!(!crash(&d, 1_000)); // first start: nothing before it
        assert!(crash(&d, 1_010)); // 1
        assert!(crash(&d, 1_020)); // 2
        let (g, crashed) = CrashGuard::start(&d, 1_030).unwrap(); // 3
        assert!(crashed);
        assert_eq!(g.recent(1_030), 3);
        assert!(g.safe_mode(1_030));
        // A clean exit from Safe mode adds nothing, and the crashes age out.
        g.clean_exit().unwrap();
        let (g, crashed) = CrashGuard::start(&d, 1_035).unwrap();
        assert!(!crashed && g.safe_mode(1_035));
        g.clean_exit().unwrap();
        let (g, _) = CrashGuard::start(&d, 1_010 + WINDOW_SECS).unwrap();
        assert_eq!(g.recent(1_010 + WINDOW_SECS), 2);
        assert!(!g.safe_mode(1_010 + WINDOW_SECS));
    }

    #[test]
    fn spread_out_crashes_dont_add_up() {
        let (_t, d) = dir();
        crash(&d, 0);
        for i in 1..=5 {
            let now = i * (WINDOW_SECS / 2 + 1);
            let (g, crashed) = CrashGuard::start(&d, now).unwrap();
            assert!(crashed);
            assert!(!g.safe_mode(now), "start {i}");
        }
    }

    #[test]
    fn fail_opens_count_with_crashes() {
        let (_t, d) = dir();
        crash(&d, 100);
        let (mut g, crashed) = CrashGuard::start(&d, 200).unwrap();
        assert!(crashed);
        g.record_failure(250).unwrap();
        assert!(!g.safe_mode(250));
        g.record_failure(300).unwrap();
        assert!(g.safe_mode(300));
        // Persisted for the next start.
        g.clean_exit().unwrap();
        let (g, _) = CrashGuard::start(&d, 310).unwrap();
        assert!(g.safe_mode(310));
    }

    #[test]
    fn junk_and_future_times_are_ignored() {
        let (_t, d) = dir();
        std::fs::write(d.crash_log(), "x\n99999999\n\n5\n").unwrap();
        let (g, _) = CrashGuard::start(&d, 10).unwrap();
        assert_eq!(g.recent(10), 1);
        assert_eq!(std::fs::read_to_string(d.crash_log()).unwrap(), "5\n");
    }
}
