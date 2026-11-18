//! Focus boost (PLAN.md §11.5): which processes go first right now. The
//! app reports the window in front (`FocusChanged`); its app gets High
//! priority, never a bigger share. A game stays High for as long as it
//! runs, alt-tabbed or not. Runtime state only: nothing is saved.
//!
//! The user's overrides (`Settings::games`, `Settings::never_first`) are
//! by app family: "a game" is applied when the process comes to the front,
//! "never first" whenever the plan is made, so it takes effect at once.

use snailnet_core::rules::Settings;

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
    games: Vec<(Proc, Seen)>,
}

impl Focus {
    /// The window in front is now `pid`'s, of app `family`; a game if
    /// `is_game` or the user said so. `created` is `None` when the process
    /// can't be opened. True if anything changed.
    pub fn changed(
        &mut self,
        pid: u32,
        is_game: bool,
        created: Option<u64>,
        family: Option<String>,
        s: &Settings,
    ) -> bool {
        let is_game = is_game || family.as_deref().is_some_and(|f| s.is_game(f));
        let seen = Seen { pid, family };
        let mut changed = self.foreground.as_ref().map(|f| f.pid) != Some(pid);
        self.foreground = Some(seen.clone());
        if is_game
            && let Some(created) = created
            && !self.games.iter().any(|(g, _)| g.pid == pid)
        {
            self.games.push((Proc { pid, created }, seen));
            changed = true;
        }
        changed
    }

    /// Forget games that have exited (`alive` says whether that process is
    /// still the one seen). True if any went.
    pub fn prune(&mut self, alive: impl Fn(Proc) -> bool) -> bool {
        let before = self.games.len();
        self.games.retain(|(g, _)| alive(*g));
        self.games.len() != before
    }

    /// Any game still running (and not one the user said never goes first).
    pub fn has_games(&self, s: &Settings) -> bool {
        self.games.iter().any(|(_, g)| g.allowed(s))
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
                .filter(|(g, seen)| Some(g.pid) != front && seen.allowed(s))
                .map(|(g, _)| g.pid),
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none() -> Settings {
        Settings::default()
    }

    #[test]
    fn the_app_in_front_goes_first_and_alt_tab_moves_it() {
        let s = none();
        let mut f = Focus::default();
        assert!(f.changed(10, false, Some(1), None, &s));
        assert!(
            !f.changed(10, false, Some(1), None, &s),
            "same window, nothing new"
        );
        assert!(f.changed(20, false, Some(2), None, &s));
        assert_eq!(f.pids(&s), vec![20]);
    }

    #[test]
    fn a_game_stays_first_while_it_runs() {
        let s = none();
        let mut f = Focus::default();
        f.changed(7, true, Some(70), None, &s);
        f.changed(20, false, Some(2), None, &s);
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
        f.changed(7, true, Some(70), None, &s);
        assert!(!f.changed(7, true, Some(70), None, &s));
        assert_eq!(f.pids(&s), vec![7]);
    }

    #[test]
    fn a_game_that_cant_be_opened_is_only_first_while_in_front() {
        let s = none();
        let mut f = Focus::default();
        f.changed(7, true, None, None, &s);
        f.changed(20, false, Some(2), None, &s);
        assert_eq!(f.pids(&s), vec![20]);
    }

    #[test]
    fn the_user_can_say_an_app_is_a_game() {
        let s = Settings {
            games: vec!["hades".into()],
            ..none()
        };
        let mut f = Focus::default();
        f.changed(7, false, Some(70), Some("Hades".into()), &s);
        f.changed(20, false, Some(2), Some("brave".into()), &s);
        assert_eq!(f.pids(&s), vec![20, 7], "not detected, still a game");
        assert!(f.has_games(&s));
    }

    #[test]
    fn never_first_apps_are_left_out_at_once() {
        let mut s = none();
        let mut f = Focus::default();
        f.changed(7, true, Some(70), Some("hades".into()), &s);
        f.changed(20, false, Some(2), Some("obs".into()), &s);
        assert_eq!(f.pids(&s), vec![20, 7]);
        s.never_first = vec!["obs".into(), "hades".into()];
        assert!(f.pids(&s).is_empty(), "applied without a new focus change");
        assert!(!f.has_games(&s), "nor counted as a game");
        assert!(
            f.tracks_games(),
            "but still watched, in case it's allowed again"
        );
    }
}
