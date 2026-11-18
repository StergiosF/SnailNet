//! Focus boost, the app's half (PLAN.md §11.5): which program is in front,
//! and whether it's a game, told to the service (`FocusChanged`), which
//! puts that app's traffic first.
//!
//! The window in front is polled every 300 ms rather than hooked: it costs
//! next to nothing and debounces by itself (a program must be in front for
//! two polls in a row, so alt-tabbing past windows sends nothing). A game
//! is a program in exclusive fullscreen (Direct3D), or one whose exe sits
//! in a game launcher's library ([`snailnet_core::games`]).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use snailnet_core::packs;
use snailnet_ipc::{Client, GameWhy, PIPE_NAME, Request};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::{QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
use windows::core::PWSTR;

const POLL: Duration = Duration::from_millis(300);
/// Say it again now and then: a restarted service starts with nothing first.
const REPEAT: Duration = Duration::from_secs(60);

/// When to tell the service: a program steady in front, once, and again
/// every [`REPEAT`].
#[derive(Debug, Default)]
struct Debounce {
    last_seen: Option<u32>,
    told: Option<(u32, Instant)>,
}

impl Debounce {
    /// One poll: `pid` is in front at `now`. Returns the PID to report.
    fn step(&mut self, pid: Option<u32>, now: Instant) -> Option<u32> {
        let steady = pid.filter(|p| self.last_seen == Some(*p));
        self.last_seen = pid;
        let pid = steady?;
        let due = match self.told {
            Some((p, at)) => p != pid || now.duration_since(at) >= REPEAT,
            None => true,
        };
        due.then(|| {
            self.told = Some((pid, now));
            pid
        })
    }
}

/// The game folders come from the packs the service keeps (PLAN.md
/// §17.2): its newest signed bundle in `%ProgramData%\SnailNet`, checked
/// here too, or the compiled-in ones. Looked at every [`REPEAT`], read
/// again only when the file changed.
#[derive(Debug, Default)]
struct SavedPacks {
    checked: Option<Instant>,
    modified: Option<SystemTime>,
}

impl SavedPacks {
    fn refresh(&mut self, now: Instant) {
        if self
            .checked
            .is_some_and(|at| now.duration_since(at) < REPEAT)
        {
            return;
        }
        self.checked = Some(now);
        let dir = std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
            .join("SnailNet");
        let modified = std::fs::metadata(dir.join(packs::FILE))
            .and_then(|m| m.modified())
            .ok();
        if modified.is_none() || modified == self.modified {
            return;
        }
        self.modified = modified;
        // A bad or older bundle leaves the packs as they are.
        if let Ok(Some(p)) = packs::read_saved(&dir) {
            packs::install(p);
        }
    }
}

/// Watch the window in front until `stop`.
pub fn spawn(stop: Arc<AtomicBool>) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("focus-watch".into())
        .spawn(move || {
            let mut d = Debounce::default();
            let mut packs = SavedPacks::default();
            let own = std::process::id();
            while !stop.load(Ordering::Relaxed) {
                packs.refresh(Instant::now());
                // SnailNet's own window isn't worth putting first.
                let pid = foreground_pid().filter(|p| *p != own);
                if let Some(pid) = d.step(pid, Instant::now()) {
                    tell(pid, game_why(pid));
                }
                std::thread::sleep(POLL);
            }
        })?;
    Ok(())
}

/// Best effort: if the service isn't there, the next change or repeat
/// tries again.
fn tell(pid: u32, why: Option<GameWhy>) {
    if let Ok(mut c) = Client::connect(PIPE_NAME, env!("CARGO_PKG_VERSION")) {
        let _ = c.call(Request::FocusChanged {
            pid,
            is_game: why.is_some(),
            confidence: if why.is_some() { 1.0 } else { 0.0 },
            why,
        });
    }
}

fn foreground_pid() -> Option<u32> {
    // SAFETY: plain Win32 queries; a null window gives PID 0.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        (pid != 0).then_some(pid)
    }
}

/// Why `pid` (the window in front) is a game, if it is: an exe in a game
/// launcher's library (what the program is, so it comes first), or
/// exclusive fullscreen Direct3D.
fn game_why(pid: u32) -> Option<GameWhy> {
    if exe_path(pid).is_some_and(|p| snailnet_core::games::in_game_library(&p)) {
        return Some(GameWhy::Library);
    }
    // SAFETY: no arguments; the result is checked.
    let fullscreen =
        unsafe { SHQueryUserNotificationState() }.is_ok_and(|s| s == QUNS_RUNNING_D3D_FULL_SCREEN);
    fullscreen.then_some(GameWhy::Fullscreen)
}

fn exe_path(pid: u32) -> Option<String> {
    // SAFETY: the handle is checked, used for one query and closed once;
    // the buffer is valid for `len` UTF-16 units.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok =
            QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len)
                .is_ok();
        let _ = CloseHandle(h);
        ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_is_told_once_it_stays_in_front() {
        let t0 = Instant::now();
        let mut d = Debounce::default();
        assert_eq!(d.step(Some(1), t0), None, "just arrived");
        assert_eq!(d.step(Some(1), t0 + POLL), Some(1));
        assert_eq!(d.step(Some(1), t0 + POLL * 2), None, "already told");
    }

    #[test]
    fn alt_tabbing_past_windows_tells_nothing() {
        let t0 = Instant::now();
        let mut d = Debounce::default();
        d.step(Some(1), t0);
        d.step(Some(1), t0 + POLL);
        assert_eq!(d.step(Some(2), t0 + POLL * 2), None);
        assert_eq!(d.step(Some(3), t0 + POLL * 3), None);
        assert_eq!(d.step(None, t0 + POLL * 4), None);
        assert_eq!(d.step(Some(1), t0 + POLL * 5), None);
        assert_eq!(
            d.step(Some(1), t0 + POLL * 6),
            None,
            "1 is still the one told"
        );
        assert_eq!(d.step(Some(4), t0 + POLL * 7), None);
        assert_eq!(d.step(Some(4), t0 + POLL * 8), Some(4));
    }

    #[test]
    fn it_repeats_now_and_then() {
        let t0 = Instant::now();
        let mut d = Debounce::default();
        d.step(Some(1), t0);
        d.step(Some(1), t0 + POLL);
        assert_eq!(d.step(Some(1), t0 + REPEAT + POLL), Some(1));
    }
}
