//! Focus boost (PLAN.md §11.5): which processes go first right now. The
//! app reports the window in front (`FocusChanged`); its app gets High
//! priority, never a bigger share. A game stays High for as long as it
//! runs, alt-tabbed or not. Runtime state only: nothing is saved.
//!
//! The user's overrides (`Settings::games`, `Settings::never_first`) are
//! by app family: "a game" is applied when the process comes to the front,
//! "never first" whenever the plan is made, so it takes effect at once.

use snailnet_core::rules::Settings;
use snailnet_ipc::{GameTick, GameWhy};

/// A process, told apart from a later one with the same PID by its
/// creation time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Proc {
    pub pid: u32,
    pub created: u64,
}

#[derive(Debug, Clone)]
struct Seen {
    pid: u32,
    /// Its app family, if it could be resolved.
    family: Option<String>,
}

impl Seen {
    fn allowed(&self, s: &Settings) -> bool {
        !self.family.as_deref().is_some_and(|f| s.never_first(f))
    }
}

#[derive(Debug, Default)]
pub struct Focus {
    foreground: Option<Seen>,
    /// With why the app took it for one (`None`: only the user's mark).
    games: Vec<(Proc, Seen, Option<GameWhy>)>,
}

impl Focus {
    /// The window in front is now `pid`'s, of app `family`; a game if the
    /// app said so (`detected`, with why) or the user did. `created` is
    /// `None` when the process can't be opened. True if anything changed.
    pub fn changed(
        &mut self,
        pid: u32,
        detected: Option<GameWhy>,
        created: Option<u64>,
        family: Option<String>,
        s: &Settings,
    ) -> bool {
        let is_game = detected.is_some() || family.as_deref().is_some_and(|f| s.is_game(f));
        let seen = Seen { pid, family };
        let mut changed = self.foreground.as_ref().map(|f| f.pid) != Some(pid);
        self.foreground = Some(seen.clone());
        if let Some((_, _, why)) = self.games.iter_mut().find(|(g, ..)| g.pid == pid) {
            // Library beats fullscreen: it's what the program is, not what
            // it did a moment ago.
            if detected.is_some() && *why != Some(GameWhy::Library) {
                *why = detected;
            }
        } else if is_game && let Some(created) = created {
            self.games.push((Proc { pid, created }, seen, detected));
            changed = true;
        }
        changed
    }

    /// Forget games that have exited (`alive` says whether that process is
    /// still the one seen). True if any went.
    pub fn prune(&mut self, alive: impl Fn(Proc) -> bool) -> bool {
        let before = self.games.len();
        self.games.retain(|(g, ..)| alive(*g));
        self.games.len() != before
    }

    /// Any game still running (and not one the user said never goes first).
    pub fn has_games(&self, s: &Settings) -> bool {
        self.games.iter().any(|(_, g, _)| g.allowed(s))
    }

    /// The games running now, one per app family, with why each is one:
    /// the user's mark first, then what the app saw.
    pub fn games(&self, s: &Settings) -> Vec<GameTick> {
        let mut out: Vec<GameTick> = Vec::new();
        for (_, seen, why) in &self.games {
            let Some(family) = seen.family.as_deref().filter(|_| seen.allowed(s)) else {
                continue;
            };
            let why = match why {
                _ if s.is_game(family) => GameWhy::Marked,
                Some(w) => *w,
                None => GameWhy::Marked,
            };
            match out.iter_mut().find(|g| g.family == family) {
                Some(g) if g.why == GameWhy::Fullscreen => g.why = why,
                Some(_) => {}
                None => out.push(GameTick {
                    family: family.to_owned(),
                    why,
                }),
            }
        }
        out
    }

    /// Any game process to watch for exit.
    pub fn tracks_games(&self) -> bool {
        !self.games.is_empty()
    }

    /// The processes to put first: the one in front, then the games;
    /// never-first apps left out.
    pub fn pids(&self, s: &Settings) -> Vec<u32> {
        let front = self.foreground.as_ref().map(|f| f.pid);
        let mut out: Vec<u32> = self
            .foreground
            .iter()
            .filter(|f| f.allowed(s))
            .map(|f| f.pid)
            .collect();
        out.extend(
            self.games
                .iter()
                .filter(|(g, seen, _)| Some(g.pid) != front && seen.allowed(s))
                .map(|(g, ..)| g.pid),
        );
        out
    }

    /// The resolved app families to put first: the one in front, then the
    /// running games; never-first apps left out, deduplicated.
    pub fn families(&self, s: &Settings) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut push = |seen: &Seen| {
            if seen.allowed(s)
                && let Some(fam) = &seen.family
                && !out.contains(fam)
            {
                out.push(fam.clone());
            }
        };
        if let Some(front) = &self.foreground {
            push(front);
        }
        for (_, seen, _) in &self.games {
            push(seen);
        }
        out
    }

    /// The app family in front, for the app to label (never a "never
    /// first" one, nor one whose family couldn't be resolved).
    pub fn front(&self, s: &Settings) -> Option<String> {
        self.foreground
            .as_ref()
            .filter(|f| f.allowed(s))
            .and_then(|f| f.family.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIB: Option<GameWhy> = Some(GameWhy::Library);

    fn none() -> Settings {
        Settings::default()
    }

    #[test]
    fn the_app_in_front_goes_first_and_alt_tab_moves_it() {
        let s = none();
        let mut f = Focus::default();
        assert!(f.changed(10, None, Some(1), None, &s));
        assert!(
            !f.changed(10, None, Some(1), None, &s),
            "same window, nothing new"
        );
        assert!(f.changed(20, None, Some(2), None, &s));
        assert_eq!(f.pids(&s), vec![20]);
    }

    #[test]
    fn the_front_family_is_named_unless_never_first() {
        let mut s = none();
        let mut f = Focus::default();
        assert_eq!(f.front(&s), None);
        f.changed(10, None, Some(1), None, &s);
        assert_eq!(f.front(&s), None, "family unknown");
        f.changed(20, None, Some(2), Some("firefox".into()), &s);
        assert_eq!(f.front(&s).as_deref(), Some("firefox"));
        s.never_first = vec!["firefox".into()];
        assert_eq!(f.front(&s), None);
    }

    #[test]
    fn a_game_stays_first_while_it_runs() {
        let s = none();
        let mut f = Focus::default();
        f.changed(7, LIB, Some(70), None, &s);
        f.changed(20, None, Some(2), None, &s);
        assert_eq!(f.pids(&s), vec![20, 7], "alt-tabbed, still first");
        assert!(!f.prune(|_| true));
        assert!(f.prune(|p| p.pid != 7), "it exited");
        assert_eq!(f.pids(&s), vec![20]);
        assert!(!f.has_games(&s));
    }

    #[test]
    fn a_game_in_front_is_listed_once() {
        let s = none();
        let mut f = Focus::default();
        f.changed(7, LIB, Some(70), None, &s);
        assert!(!f.changed(7, LIB, Some(70), None, &s));
        assert_eq!(f.pids(&s), vec![7]);
    }

    #[test]
    fn a_game_that_cant_be_opened_is_only_first_while_in_front() {
        let s = none();
        let mut f = Focus::default();
        f.changed(7, LIB, None, None, &s);
        f.changed(20, None, Some(2), None, &s);
        assert_eq!(f.pids(&s), vec![20]);
    }

    #[test]
    fn the_user_can_say_an_app_is_a_game() {
        let s = Settings {
            games: vec!["hades".into()],
            ..none()
        };
        let mut f = Focus::default();
        f.changed(7, None, Some(70), Some("Hades".into()), &s);
        f.changed(20, None, Some(2), Some("brave".into()), &s);
        assert_eq!(f.pids(&s), vec![20, 7], "not detected, still a game");
        assert!(f.has_games(&s));
    }

    #[test]
    fn never_first_apps_are_left_out_at_once() {
        let mut s = none();
        let mut f = Focus::default();
        f.changed(7, LIB, Some(70), Some("hades".into()), &s);
        f.changed(20, None, Some(2), Some("obs".into()), &s);
        assert_eq!(f.pids(&s), vec![20, 7]);
        s.never_first = vec!["obs".into(), "hades".into()];
        assert!(f.pids(&s).is_empty(), "applied without a new focus change");
        assert!(!f.has_games(&s), "nor counted as a game");
        assert!(
            f.tracks_games(),
            "but still watched, in case it's allowed again"
        );
    }

    #[test]
    fn it_says_why_each_game_is_one() {
        let mut s = none();
        let mut f = Focus::default();
        f.changed(
            7,
            Some(GameWhy::Fullscreen),
            Some(70),
            Some("hades".into()),
            &s,
        );
        f.changed(8, None, Some(80), Some("obs".into()), &s);
        assert_eq!(
            f.games(&s),
            vec![GameTick {
                family: "hades".into(),
                why: GameWhy::Fullscreen
            }]
        );
        f.changed(7, LIB, Some(70), Some("hades".into()), &s);
        f.changed(
            7,
            Some(GameWhy::Fullscreen),
            Some(70),
            Some("hades".into()),
            &s,
        );
        assert_eq!(f.games(&s)[0].why, GameWhy::Library, "library sticks");
        s.games = vec!["hades".into()];
        assert_eq!(f.games(&s)[0].why, GameWhy::Marked, "the user's word first");
        s.never_first = vec!["hades".into()];
        assert!(f.games(&s).is_empty(), "never first: not shown as a game");
    }

    #[test]
    fn a_game_known_only_by_the_users_mark_stays_marked() {
        let s = Settings {
            games: vec!["hades".into()],
            ..none()
        };
        let mut f = Focus::default();
        f.changed(7, None, Some(70), Some("hades".into()), &s);
        assert_eq!(f.games(&none())[0].why, GameWhy::Marked);
    }
}
