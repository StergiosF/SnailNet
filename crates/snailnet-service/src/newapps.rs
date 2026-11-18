//! New-app notifications, the service's half (PLAN.md §11.4): which app
//! families SnailNet has met, and the questions not answered yet.
//!
//! Every family the shaper sees is checked once against the known list
//! (`known-apps.txt` in the data folder). A family seen for the first time
//! becomes known at once and, unless it's quiet, a prompt ("{app} went
//! online"). Quiet: parts of Windows, families the user already gave a
//! rule, a block or an override, and everything met during the first
//! [`LEARN_SECS`] (so a fresh install doesn't greet every running program).
//! The app is never held back: it already joined Everything else.

use std::collections::{BTreeSet, HashSet};
use std::io;
use std::path::Path;

use snailnet_core::classify::App;
use snailnet_core::classify::family::{
    How, SYSTEM_ID, UNKNOWN_ID, WINDOWS_SERVICES_ID, normalise_path,
};
use snailnet_core::rules::{Config, TargetKind};
use snailnet_ipc::{AppInfo, NewApp};

/// Apps met this soon after SnailNet first looked become known silently.
pub const LEARN_SECS: u64 = 10 * 60;
/// Unanswered prompts kept at most (the oldest go first).
pub const MAX_PENDING: usize = 20;

/// A part of Windows: the kernel, services, programs in the Windows
/// folder, and Windows' own Store packages. These never prompt.
pub fn windows_part(app: &App, windows_dir: &str) -> bool {
    if matches!(app.how, How::System | How::Unknown | How::Service) {
        return true;
    }
    let dir = normalise_path(windows_dir);
    let dir = dir.trim_end_matches('\\');
    let family = app.family.to_lowercase();
    family
        .strip_prefix(dir)
        .is_some_and(|rest| rest.starts_with('\\'))
        || family.starts_with("pkg:microsoft.windows")
}

#[derive(Debug, Default)]
pub struct Newcomers {
    known: BTreeSet<String>,
    /// When SnailNet first looked (Unix seconds); `None` until then.
    since: Option<u64>,
    pending: Vec<NewApp>,
    next_id: u64,
    /// How many of the shaper's apps were checked (its list only grows
    /// while it runs; a shorter list means it restarted).
    checked: usize,
}

impl Newcomers {
    /// Read `known-apps.txt` (missing: nothing known yet).
    pub fn load(path: &Path) -> io::Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e),
        };
        let mut n = Self::default();
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            if let Some(since) = line.strip_prefix("since ") {
                n.since = since.trim().parse().ok();
            } else if !line.starts_with('#') {
                n.known.insert(line.to_owned());
            }
        }
        Ok(n)
    }

    /// Write `known-apps.txt` (through a temporary file).
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let mut text = String::from("# Apps SnailNet has met (PLAN.md §11.4).\n");
        if let Some(since) = self.since {
            text.push_str(&format!("since {since}\n"));
        }
        for f in &self.known {
            text.push_str(f);
            text.push('\n');
        }
        let tmp = path.with_extension("txt.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }

    /// Look at the shaper's apps (in the order first seen) at `now`.
    /// `windows` are the families that are parts of Windows; `notify`:
    /// whether to ask about newcomers at all. Returns the new prompts and
    /// whether the known list changed (save it).
    pub fn observe(
        &mut self,
        apps: &[AppInfo],
        windows: &HashSet<String>,
        config: &Config,
        notify: bool,
        now: u64,
    ) -> (Vec<NewApp>, bool) {
        let mut changed = false;
        if self.since.is_none() {
            self.since = Some(now);
            changed = true;
        }
        if apps.len() < self.checked {
            self.checked = 0;
        }
        let learning = now < self.since.unwrap_or(now) + LEARN_SECS;
        let mut prompts = Vec::new();
        for app in &apps[self.checked..] {
            if self.known.contains(&app.family) || app.family.contains('\n') {
                continue;
            }
            self.known.insert(app.family.clone());
            changed = true;
            let quiet = learning
                || [SYSTEM_ID, UNKNOWN_ID, WINDOWS_SERVICES_ID].contains(&app.family.as_str())
                || windows.contains(&app.family)
                || chosen(config, app);
            if notify && !quiet {
                self.next_id += 1;
                let p = NewApp {
                    prompt_id: self.next_id,
                    family: app.family.clone(),
                    name: app.name.clone(),
                    exe: app.exe.clone(),
                };
                self.pending.push(p.clone());
                prompts.push(p);
            }
        }
        self.checked = apps.len();
        if self.pending.len() > MAX_PENDING {
            let extra = self.pending.len() - MAX_PENDING;
            self.pending.drain(..extra);
        }
        (prompts, changed)
    }

    /// Questions not answered yet, oldest first.
    pub fn pending(&self) -> &[NewApp] {
        &self.pending
    }

    /// Take prompt `id` off the list (`None`: answered already, or
    /// forgotten).
    pub fn answer(&mut self, id: u64) -> Option<NewApp> {
        let i = self.pending.iter().position(|p| p.prompt_id == id)?;
        Some(self.pending.remove(i))
    }

    /// Notifications were switched off: drop the open questions.
    pub fn clear_pending(&mut self) {
        self.pending.clear();
    }
}

/// The user already said something about `app`: a rule in any mode, a
/// block, a boost or one of the focus overrides.
fn chosen(config: &Config, app: &AppInfo) -> bool {
    let fam = |id: &str| id.eq_ignore_ascii_case(&app.family);
    let exe = |id: &str| {
        let id = id.to_lowercase();
        !app.exe.is_empty() && (id == app.exe || app.exe.strip_suffix(".exe") == Some(id.as_str()))
    };
    let target = |t: &snailnet_core::rules::Target| match t.kind {
        TargetKind::Family => fam(&t.id),
        TargetKind::Exe => exe(&t.id),
        _ => false,
    };
    config
        .profiles
        .iter()
        .flat_map(|p| &p.rules)
        .any(|r| target(&r.target))
        || config.boost.as_ref().is_some_and(|b| target(&b.target))
        || config.blocked.iter().any(|b| fam(b))
        || config.settings.games.iter().any(|g| fam(g))
        || config.settings.never_first.iter().any(|g| fam(g))
}

#[cfg(test)]
mod tests {
    use super::*;
    use snailnet_core::rules::{Rule, Target};
    use snailnet_core::scheduler::Priority;

    fn app(family: &str, exe: &str) -> AppInfo {
        AppInfo {
            family: family.into(),
            name: family.to_uppercase(),
            exe: exe.into(),
        }
    }

    const T0: u64 = 1_000_000;
    const LATER: u64 = T0 + LEARN_SECS;

    /// Known list started long ago, so nothing is learnt silently.
    fn settled() -> Newcomers {
        Newcomers {
            since: Some(0),
            ..Newcomers::default()
        }
    }

    fn names(p: &[NewApp]) -> Vec<&str> {
        p.iter().map(|p| p.family.as_str()).collect()
    }

    #[test]
    fn a_new_app_prompts_once() {
        let mut n = settled();
        let c = Config::default();
        let none = HashSet::new();
        let apps = vec![app("hades", "hades.exe")];
        let (p, changed) = n.observe(&apps, &none, &c, true, LATER);
        assert_eq!(names(&p), ["hades"]);
        assert!(changed);
        assert_eq!(n.pending().len(), 1);
        // The same list again, and after a restart of the shaper.
        assert_eq!(n.observe(&apps, &none, &c, true, LATER), (vec![], false));
        n.checked = 5;
        assert_eq!(n.observe(&apps, &none, &c, true, LATER), (vec![], false));
    }

    #[test]
    fn the_first_ten_minutes_are_learnt_quietly() {
        let mut n = Newcomers::default();
        let c = Config::default();
        let none = HashSet::new();
        let (p, changed) = n.observe(&[app("brave", "brave.exe")], &none, &c, true, T0);
        assert!(p.is_empty() && changed);
        let apps = [app("brave", "brave.exe"), app("hades", "hades.exe")];
        let (p, _) = n.observe(&apps, &none, &c, true, T0 + LEARN_SECS);
        assert_eq!(names(&p), ["hades"]);
    }

    #[test]
    fn windows_and_apps_the_user_chose_never_prompt() {
        let mut n = settled();
        let mut c = Config::default();
        c.profiles[0].rules.push(Rule {
            target: Target {
                kind: TargetKind::Exe,
                id: "game".into(),
            },
            share: 1000,
            priority: Priority::Normal,
            ceiling: None,
            enabled: true,
        });
        c.blocked.push("spy".into());
        c.settings.never_first.push("backup".into());
        let windows: HashSet<String> = [r"c:\windows\system32\x.exe".to_owned()].into();
        let apps = [
            app("system", ""),
            app("unknown", ""),
            app(r"c:\windows\system32\x.exe", "x.exe"),
            app(r"c:\games\game.exe", "game.exe"),
            app("spy", "spy.exe"),
            app("backup", "backup.exe"),
            app("steam", "steam.exe"),
            app("celeste", "celeste.exe"),
        ];
        let (p, _) = n.observe(&apps, &windows, &c, true, LATER);
        assert_eq!(names(&p), ["celeste"], "steam has a rule in a shipped mode");
    }

    #[test]
    fn switched_off_it_still_learns() {
        let mut n = settled();
        let c = Config::default();
        let none = HashSet::new();
        let (p, changed) = n.observe(&[app("zoom", "zoom.exe")], &none, &c, false, LATER);
        assert!(p.is_empty() && changed);
        let (p, _) = n.observe(&[app("zoom", "zoom.exe")], &none, &c, true, LATER);
        assert!(p.is_empty(), "met already");
    }

    #[test]
    fn answers_take_prompts_off_and_the_list_is_bounded() {
        let mut n = settled();
        let c = Config::default();
        let none = HashSet::new();
        let apps: Vec<_> = (0..MAX_PENDING + 3)
            .map(|i| app(&format!("a{i}"), "a.exe"))
            .collect();
        n.observe(&apps, &none, &c, true, LATER);
        assert_eq!(n.pending().len(), MAX_PENDING);
        assert_eq!(n.pending()[0].family, "a3", "the oldest went");
        let id = n.pending()[0].prompt_id;
        assert_eq!(n.answer(id).unwrap().family, "a3");
        assert_eq!(n.answer(id), None);
    }

    #[test]
    fn the_known_list_survives_a_restart() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("known-apps.txt");
        let mut n = Newcomers::load(&path).unwrap();
        let none = HashSet::new();
        n.observe(
            &[app("steam", "steam.exe")],
            &none,
            &Config::default(),
            true,
            T0,
        );
        n.save(&path).unwrap();
        let back = Newcomers::load(&path).unwrap();
        assert_eq!(back.since, Some(T0));
        assert!(back.known.contains("steam"));
    }

    #[test]
    fn parts_of_windows() {
        let mk = |family: &str, how| App {
            family: family.into(),
            name: String::new(),
            exe: String::new(),
            how,
        };
        let win = r"C:\Windows";
        assert!(windows_part(&mk("windows-update", How::Service), win));
        assert!(windows_part(
            &mk(r"c:\windows\system32\curl.exe", How::Own),
            win
        ));
        assert!(windows_part(
            &mk(
                "pkg:Microsoft.Windows.StartMenuExperienceHost_cw5n1h2txyewy",
                How::Package
            ),
            win
        ));
        assert!(!windows_part(&mk(r"c:\windowsapps\x.exe", How::Own), win));
        assert!(!windows_part(&mk("steam", How::Pack), win));
    }
}
