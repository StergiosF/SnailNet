//! New-app notifications, the app's half (PLAN.md §11.4): a Windows toast
//! "{app} went online" with Fine · Give it a slice · Block.
//!
//! The service decides what is new and sends `Event::NewApp` (and lists the
//! open questions in every snapshot). Here: one toast per app, none while
//! a game or presentation is fullscreen (they wait, then come as one
//! summary), and the buttons answer the service (`AnswerPrompt`). "Give it
//! a slice" also opens the window on the split with the app added (the
//! webview gets `give-slice` with the family id).
//!
//! Toasts are plain WinRT `ToastNotification`s under the app's own
//! AppUserModelID, registered per user at start. Button clicks reach the
//! `Activated` handler while the app runs (it lives in the tray).

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde::Serialize;
use snailnet_ipc::{Client, NewApp, PIPE_NAME, PromptAction, Request};
use tauri::{AppHandle, Emitter, Manager};
use windows::Data::Xml::Dom::XmlDocument;
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::{
    ToastActivatedEventArgs, ToastNotification, ToastNotificationManager,
};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};
use windows::Win32::UI::Shell::{
    QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
    SetCurrentProcessExplicitAppUserModelID,
};
use windows::core::{HSTRING, IInspectable, Interface};

/// The app's AppUserModelID: its bundle identifier.
const AUMID: &str = "net.snailnet.app";
/// How often waiting toasts check whether the game is over.
const WAIT: Duration = Duration::from_secs(2);
/// Toasts kept alive so their buttons still answer.
const KEEP: usize = 20;

/// What to put on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Show {
    One(NewApp),
    /// Several came while the screen was busy.
    Summary(Vec<NewApp>),
}

/// Which questions were shown, and which wait for a fullscreen game to end.
#[derive(Debug, Default)]
pub struct Quiet {
    shown: HashSet<String>,
    waiting: Vec<NewApp>,
}

impl Quiet {
    /// A question arrived; `busy`: a game or presentation is fullscreen.
    pub fn offer(&mut self, p: NewApp, busy: bool) -> Option<Show> {
        if !self.shown.insert(p.family.clone()) {
            return None;
        }
        if busy {
            self.waiting.push(p);
            return None;
        }
        Some(Show::One(p))
    }

    /// Called now and then: once the screen is free, what waited.
    pub fn settle(&mut self, busy: bool) -> Option<Show> {
        if busy || self.waiting.is_empty() {
            return None;
        }
        let mut all = std::mem::take(&mut self.waiting);
        Some(if all.len() == 1 {
            Show::One(all.remove(0))
        } else {
            Show::Summary(all)
        })
    }
}

/// What a click on a toast means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Click {
    Answer {
        action: PromptAction,
        prompt_id: u64,
        family: String,
    },
    /// The toast itself: open the window.
    Open,
}

fn args(action: PromptAction, p: &NewApp) -> String {
    format!("{} {} {}", action.name(), p.prompt_id, p.family)
}

/// Read a toast's `arguments` back.
pub fn parse_click(s: &str) -> Click {
    let mut parts = s.splitn(3, ' ');
    let action = parts.next().and_then(PromptAction::parse);
    let id = parts.next().and_then(|i| i.parse().ok());
    match (action, id, parts.next()) {
        (Some(action), Some(prompt_id), Some(family)) if !family.is_empty() => Click::Answer {
            action,
            prompt_id,
            family: family.to_owned(),
        },
        _ => Click::Open,
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// The toast's XML.
pub fn toast_xml(show: &Show) -> String {
    match show {
        Show::One(p) => {
            let name = xml_escape(&p.name);
            let button = |label: &str, a: PromptAction| {
                format!(
                    r#"<action content="{label}" arguments="{}" activationType="foreground"/>"#,
                    xml_escape(&args(a, p))
                )
            };
            // Strict mode: its connections wait for this answer (§11.4).
            let (title, body, fine) = if p.held {
                (
                    "wants to go online",
                    "It waits for your answer, 20 seconds at most.",
                    "Allow",
                )
            } else {
                ("went online", "It shares Everything else for now.", "Fine")
            };
            format!(
                r#"<toast launch="open"><visual><binding template="ToastGeneric"><text>{name} {title}</text><text>{body}</text></binding></visual><actions>{}{}{}</actions></toast>"#,
                button(fine, PromptAction::Fine),
                button("Give it a slice", PromptAction::Slice),
                button("Block", PromptAction::Block),
            )
        }
        Show::Summary(ps) => {
            let names: Vec<&str> = ps.iter().map(|p| p.name.as_str()).collect();
            let list = match names.split_last() {
                Some((last, rest)) if !rest.is_empty() => {
                    format!("{} and {last}", rest.join(", "))
                }
                _ => names.concat(),
            };
            format!(
                r#"<toast launch="open"><visual><binding template="ToastGeneric"><text>{} apps went online while you played</text><text>{}. They share Everything else; open SnailNet to change that.</text></binding></visual></toast>"#,
                ps.len(),
                xml_escape(&list)
            )
        }
    }
}

/// The webview's `give-slice` payload.
#[derive(Debug, Clone, Serialize)]
struct GiveSlice {
    family: String,
}

pub struct Toaster {
    app: AppHandle,
    quiet: Mutex<Quiet>,
    alive: Mutex<Vec<ToastNotification>>,
}

impl Toaster {
    /// Register the AppUserModelID and start watching for the end of a
    /// fullscreen game.
    pub fn start(app: AppHandle) -> Arc<Self> {
        register_aumid();
        let t = Arc::new(Self {
            app,
            quiet: Mutex::new(Quiet::default()),
            alive: Mutex::new(Vec::new()),
        });
        let weak = Arc::downgrade(&t);
        let _ = std::thread::Builder::new()
            .name("toast-wait".into())
            .spawn(move || {
                while let Some(t) = weak.upgrade() {
                    let show = t.lock_quiet().settle(screen_busy());
                    if let Some(show) = show {
                        t.show(&show);
                    }
                    drop(t);
                    std::thread::sleep(WAIT);
                }
            });
        t
    }

    fn lock_quiet(&self) -> std::sync::MutexGuard<'_, Quiet> {
        self.quiet.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A question from the service (an event, or one listed in a
    /// snapshot); each app is shown once.
    pub fn offer(&self, p: &NewApp) {
        let show = self.lock_quiet().offer(p.clone(), screen_busy());
        if let Some(show) = show {
            self.show(&show);
        }
    }

    fn show(&self, show: &Show) {
        if let Err(e) = self.try_show(show) {
            eprintln!("SnailNet: could not show a notification: {e}");
        }
    }

    fn try_show(&self, show: &Show) -> windows::core::Result<()> {
        let doc = XmlDocument::new()?;
        doc.LoadXml(&HSTRING::from(toast_xml(show)))?;
        let toast = ToastNotification::CreateToastNotification(&doc)?;
        let app = self.app.clone();
        toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
            move |_, args| {
                let click = args
                    .as_ref()
                    .and_then(|a| a.cast::<ToastActivatedEventArgs>().ok())
                    .and_then(|a| a.Arguments().ok())
                    .map_or(Click::Open, |s| parse_click(&s.to_string_lossy()));
                clicked(&app, click);
                Ok(())
            },
        ))?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))?.Show(&toast)?;
        let mut alive = self.alive.lock().unwrap_or_else(PoisonError::into_inner);
        alive.push(toast);
        if alive.len() > KEEP {
            alive.remove(0);
        }
        Ok(())
    }
}

/// Act on a click (runs on a Windows thread-pool thread).
fn clicked(app: &AppHandle, click: Click) {
    match click {
        Click::Open => crate::tray::open_main(app),
        Click::Answer {
            action,
            prompt_id,
            family,
        } => {
            if let Ok(mut c) = Client::connect(PIPE_NAME, env!("CARGO_PKG_VERSION")) {
                let _ = c.call(Request::AnswerPrompt {
                    prompt_id,
                    action: action.name().into(),
                });
            }
            if action == PromptAction::Slice {
                crate::tray::open_main(app);
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.emit("give-slice", GiveSlice { family });
                }
            }
        }
    }
}

/// A game, a presentation or another fullscreen program is in front:
/// toasts wait (Windows would hold them back anyway).
fn screen_busy() -> bool {
    // SAFETY: no arguments; the result is checked.
    unsafe { SHQueryUserNotificationState() }.is_ok_and(|s| {
        s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_BUSY || s == QUNS_PRESENTATION_MODE
    })
}

/// Windows shows toasts of an unpackaged program under an AppUserModelID
/// registered in HKCU (the installer's shortcut carries the same id).
fn register_aumid() {
    let key = HSTRING::from(format!(r"Software\Classes\AppUserModelId\{AUMID}"));
    let name: Vec<u16> = "SnailNet\0".encode_utf16().collect();
    // SAFETY: the strings outlive the calls; the data is a NUL-terminated
    // UTF-16 string of the given byte length.
    unsafe {
        let _ = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &key,
            &HSTRING::from("DisplayName"),
            REG_SZ.0,
            Some(name.as_ptr().cast()),
            (name.len() * 2) as u32,
        );
        let _ = SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(AUMID));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(id: u64, family: &str) -> NewApp {
        NewApp {
            prompt_id: id,
            family: family.into(),
            name: family.to_uppercase(),
            exe: format!("{family}.exe"),
            held: false,
        }
    }

    #[test]
    fn each_app_is_shown_once() {
        let mut q = Quiet::default();
        assert_eq!(
            q.offer(p(1, "hades"), false),
            Some(Show::One(p(1, "hades")))
        );
        assert_eq!(q.offer(p(1, "hades"), false), None, "again from a snapshot");
    }

    #[test]
    fn while_playing_they_wait_and_come_as_one() {
        let mut q = Quiet::default();
        assert_eq!(q.offer(p(1, "a"), true), None);
        assert_eq!(q.offer(p(2, "b"), true), None);
        assert_eq!(q.settle(true), None, "still playing");
        assert_eq!(
            q.settle(false),
            Some(Show::Summary(vec![p(1, "a"), p(2, "b")]))
        );
        assert_eq!(q.settle(false), None);
        q.offer(p(3, "c"), true);
        assert_eq!(
            q.settle(false),
            Some(Show::One(p(3, "c"))),
            "one is a normal toast"
        );
    }

    #[test]
    fn clicks_round_trip_through_the_arguments() {
        let app = p(7, r"c:\my games\x y.exe");
        for a in [PromptAction::Fine, PromptAction::Slice, PromptAction::Block] {
            assert_eq!(
                parse_click(&args(a, &app)),
                Click::Answer {
                    action: a,
                    prompt_id: 7,
                    family: app.family.clone()
                }
            );
        }
        assert_eq!(parse_click("open"), Click::Open);
        assert_eq!(parse_click(""), Click::Open);
        assert_eq!(parse_click("block x hades"), Click::Open);
    }

    #[test]
    fn the_xml_escapes_names_and_lists_the_summary() {
        let mut a = p(1, "a");
        a.name = "Tom & \"Jerry\" <3".into();
        let x = toast_xml(&Show::One(a));
        assert!(x.contains("Tom &amp; &quot;Jerry&quot; &lt;3 went online"));
        assert!(x.contains(r#"arguments="block 1 a""#));
        let mut h = p(2, "b");
        h.held = true;
        let x = toast_xml(&Show::One(h));
        assert!(x.contains("B wants to go online"));
        assert!(x.contains(r#"content="Allow" arguments="fine 2 b""#));
        let s = toast_xml(&Show::Summary(vec![p(1, "a"), p(2, "b"), p(3, "c")]));
        assert!(s.contains("3 apps went online"));
        assert!(s.contains("A, B and C."));
    }
}
