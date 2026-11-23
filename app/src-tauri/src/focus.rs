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
/// launcher's library or a known game exe (what the program is, so it
/// comes first), exclusive fullscreen Direct3D, or a borderless window
/// covering its whole screen that draws with a 3D API (how most games run
/// "fullscreen" today; Windows' own flag only sees the exclusive kind).
fn game_why(pid: u32) -> Option<GameWhy> {
    let path = exe_path(pid);
    if path
        .as_deref()
        .is_some_and(snailnet_core::games::in_game_library)
    {
        return Some(GameWhy::Library);
    }
    // SAFETY: no arguments; the result is checked.
    let exclusive =
        unsafe { SHQueryUserNotificationState() }.is_ok_and(|s| s == QUNS_RUNNING_D3D_FULL_SCREEN);
    if exclusive {
        return Some(GameWhy::Fullscreen);
    }
    let exe = path
        .as_deref()
        .and_then(|p| p.rsplit(['\\', '/']).next())
        .map(str::to_lowercase)
        .unwrap_or_default();
    let borderless = !exe.is_empty()
        && !NOT_GAMES.contains(&exe.as_str())
        && foreground_covers_screen()
        && draws_3d(pid) != Some(false);
    borderless.then_some(GameWhy::Fullscreen)
}

/// Programs that often fill the screen without being games (video,
/// browsers in full screen, slideshows, the desktop itself).
const NOT_GAMES: &[&str] = &[
    "explorer.exe",
    "applicationframehost.exe",
    "chrome.exe",
    "msedge.exe",
    "firefox.exe",
    "brave.exe",
    "opera.exe",
    "opera_gx.exe",
    "vivaldi.exe",
    "vlc.exe",
    "mpc-hc64.exe",
    "mpc-hc.exe",
    "mpc-be64.exe",
    "mpv.exe",
    "potplayermini64.exe",
    "wmplayer.exe",
    "video.ui.exe",
    "microsoft.media.player.exe",
    "powerpnt.exe",
    "obs64.exe",
    "discord.exe",
    "spotify.exe",
    "netflix.exe",
    "snailnet.exe",
];

/// The window in front fills its whole monitor with no title bar
/// (borderless fullscreen), and isn't the desktop.
fn foreground_covers_screen() -> bool {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_STYLE, GetClassNameW, GetWindowLongW, GetWindowRect, WS_CAPTION,
    };
    // SAFETY: plain Win32 queries on the foreground window; every out
    // buffer is valid for the size passed.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return false;
        }
        let mut class = [0u16; 64];
        let n = GetClassNameW(hwnd, &mut class).max(0) as usize;
        let class = String::from_utf16_lossy(&class[..n]);
        if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") {
            return false;
        }
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        if style & WS_CAPTION.0 == WS_CAPTION.0 {
            return false;
        }
        let mut win = RECT::default();
        if GetWindowRect(hwnd, &mut win).is_err() {
            return false;
        }
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(mon, &mut info).as_bool() {
            return false;
        }
        covers(
            (win.left, win.top, win.right, win.bottom),
            (
                info.rcMonitor.left,
                info.rcMonitor.top,
                info.rcMonitor.right,
                info.rcMonitor.bottom,
            ),
        )
    }
}

/// Whether window rect `w` covers monitor rect `m` (left, top, right,
/// bottom), allowing the few pixels some games hang over the edge.
fn covers(w: (i32, i32, i32, i32), m: (i32, i32, i32, i32)) -> bool {
    const SLACK: i32 = 8;
    m.2 > m.0
        && m.3 > m.1
        && w.0 <= m.0 + SLACK
        && w.1 <= m.1 + SLACK
        && w.2 >= m.2 - SLACK
        && w.3 >= m.3 - SLACK
}

/// Whether `pid` has a 3D graphics runtime loaded. `None` when Windows
/// won't let us look (anti-cheat protected games): treated as yes.
fn draws_3d(pid: u32) -> Option<bool> {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW, TH32CS_SNAPMODULE,
        TH32CS_SNAPMODULE32,
    };
    const RUNTIMES: &[&str] = &[
        "d3d9.dll",
        "d3d11.dll",
        "d3d12.dll",
        "vulkan-1.dll",
        "opengl32.dll",
    ];
    // SAFETY: the snapshot handle is checked and closed once; the entry
    // is sized as the API requires.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid).ok()?;
        let mut entry = MODULEENTRY32W {
            dwSize: size_of::<MODULEENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = false;
        let mut more = Module32FirstW(snap, &mut entry).is_ok();
        while more {
            let len = entry
                .szModule
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szModule.len());
            let name = String::from_utf16_lossy(&entry.szModule[..len]).to_lowercase();
            if RUNTIMES.contains(&name.as_str()) {
                found = true;
                break;
            }
            more = Module32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
        Some(found)
    }
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
    fn a_window_covering_its_monitor_counts_as_fullscreen() {
        let screen = (0, 0, 1920, 1080);
        assert!(covers((0, 0, 1920, 1080), screen));
        assert!(covers((-8, -8, 1928, 1088), screen), "hangs over the edge");
        assert!(!covers((0, 0, 1920, 1040), screen), "taskbar showing");
        assert!(!covers((100, 100, 900, 700), screen), "a window");
        let second = (1920, 0, 4480, 1440);
        assert!(covers((1920, 0, 4480, 1440), second));
        assert!(!covers((0, 0, 1920, 1080), second));
    }

    #[test]
    fn video_players_and_browsers_are_never_fullscreen_games() {
        for exe in ["chrome.exe", "vlc.exe", "explorer.exe", "powerpnt.exe"] {
            assert!(NOT_GAMES.contains(&exe));
        }
        assert!(!NOT_GAMES.contains(&"cs2.exe"));
    }

    #[test]
    fn this_test_process_draws_no_3d() {
        assert_eq!(draws_3d(std::process::id()), Some(false));
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
