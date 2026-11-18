//! Focus boost (PLAN.md §11.5): which processes go first right now. The
//! app reports the window in front (`FocusChanged`); its app gets High
//! priority, never a bigger share. A game stays High for as long as it
//! runs, alt-tabbed or not. Runtime state only: nothing is saved.

/// A process, told apart from a later one with the same PID by its
/// creation time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Proc {
    pub pid: u32,
    pub created: u64,
}

#[derive(Debug, Default)]
pub struct Focus {
    foreground: Option<u32>,
    games: Vec<Proc>,
}

impl Focus {
    /// The window in front is now `pid`'s (a game if `is_game`; `created`
    /// is `None` when the process can't be opened). True if anything
    /// changed.
    pub fn changed(&mut self, pid: u32, is_game: bool, created: Option<u64>) -> bool {
        let mut changed = self.foreground != Some(pid);
        self.foreground = Some(pid);
        if is_game
            && let Some(created) = created
            && !self.games.iter().any(|g| g.pid == pid)
        {
            self.games.push(Proc { pid, created });
            changed = true;
        }
        changed
    }

    /// Forget games that have exited (`alive` says whether that process is
    /// still the one seen). True if any went.
    pub fn prune(&mut self, alive: impl Fn(Proc) -> bool) -> bool {
        let before = self.games.len();
        self.games.retain(|g| alive(*g));
        self.games.len() != before
    }

    pub fn has_games(&self) -> bool {
        !self.games.is_empty()
    }

    /// The processes to put first: the one in front, then the games.
    pub fn pids(&self) -> Vec<u32> {
        let mut out: Vec<u32> = self.foreground.into_iter().collect();
        out.extend(
            self.games
                .iter()
                .map(|g| g.pid)
                .filter(|p| Some(*p) != self.foreground),
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_in_front_goes_first_and_alt_tab_moves_it() {
        let mut f = Focus::default();
        assert!(f.changed(10, false, Some(1)));
        assert!(!f.changed(10, false, Some(1)), "same window, nothing new");
        assert!(f.changed(20, false, Some(2)));
        assert_eq!(f.pids(), vec![20]);
    }

    #[test]
    fn a_game_stays_first_while_it_runs() {
        let mut f = Focus::default();
        f.changed(7, true, Some(70));
        f.changed(20, false, Some(2));
        assert_eq!(f.pids(), vec![20, 7], "alt-tabbed, still first");
        assert!(!f.prune(|_| true));
        assert!(f.prune(|p| p.pid != 7), "it exited");
        assert_eq!(f.pids(), vec![20]);
        assert!(!f.has_games());
    }

    #[test]
    fn a_game_in_front_is_listed_once() {
        let mut f = Focus::default();
        f.changed(7, true, Some(70));
        assert!(!f.changed(7, true, Some(70)));
        assert_eq!(f.pids(), vec![7]);
    }

    #[test]
    fn a_game_that_cant_be_opened_is_only_first_while_in_front() {
        let mut f = Focus::default();
        f.changed(7, true, None);
        f.changed(20, false, Some(2));
        assert_eq!(f.pids(), vec![20]);
    }
}
