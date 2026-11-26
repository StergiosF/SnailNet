//! The user's changes to app families (PLAN.md §7.2, §11.6): "Split out"
//! one program from its family, "Merge into…" another family.
//!
//! Applied to each resolved [`App`] after the packs and heuristics: split
//! first, then merge, so a program split out can also be merged elsewhere
//! (that's "move this program to another app"). Merges don't chain: the
//! family a merge goes into can't itself be merged away.
//!
//! ```toml
//! [families]
//! split = ["steamwebhelper.exe"]
//!
//! [[families.merge]]
//! from = "c:\\tools\\sync\\sync.exe"
//! from_name = "Sync"
//! into = "dropbox"
//! name = "Dropbox"
//! ```

use serde::{Deserialize, Serialize};

use crate::classify::App;

/// The family id a program split out gets: `split:<exe>`.
pub const SPLIT_PREFIX: &str = "split:";

/// Something a family id can be: not empty, not a rule target string
/// (`family:…`, `exe:…`, `site:…`). Paths (`c:\…`) and `pkg:`/`folder:`
/// ids are fine.
pub fn is_family_id(id: &str) -> bool {
    let id = id.trim().to_lowercase();
    !id.is_empty()
        && !["family:", "exe:", "site:"]
            .iter()
            .any(|p| id.starts_with(p))
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Families {
    /// Programs (lowercase exe file names) that are apps of their own.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub split: Vec<String>,
    /// Families that count as part of another.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub merge: Vec<Merge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Merge {
    /// The family merged away (lowercase).
    pub from: String,
    /// Its display name, so the settings can still say what was merged.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub from_name: String,
    /// The family it joins (lowercase).
    pub into: String,
    /// `into`'s display name, for when none of its own programs ran yet.
    pub name: String,
}

impl Families {
    pub fn is_empty(&self) -> bool {
        self.split.is_empty() && self.merge.is_empty()
    }

    /// `app` as the user wants it grouped.
    pub fn apply(&self, mut app: App) -> App {
        if self.is_empty() || app.exe.is_empty() {
            return app;
        }
        if self.split.iter().any(|e| e.eq_ignore_ascii_case(&app.exe)) {
            app.name = format!("{} ({})", app.name, app.exe);
            app.family = format!("{SPLIT_PREFIX}{}", app.exe);
        }
        if let Some(m) = self
            .merge
            .iter()
            .find(|m| m.from.eq_ignore_ascii_case(&app.family))
        {
            app.family = m.into.clone();
            app.name = m.name.clone();
        }
        app
    }

    pub fn validate(&self) -> Result<(), String> {
        for (i, exe) in self.split.iter().enumerate() {
            let ok = !exe.trim().is_empty()
                && !exe.contains(['\\', '/', ':'])
                && *exe == exe.to_lowercase();
            if !ok {
                return Err(format!("{exe:?} is not a lowercase program file name"));
            }
            if self.split[..i].contains(exe) {
                return Err(format!("{exe:?} is split out twice"));
            }
        }
        for (i, m) in self.merge.iter().enumerate() {
            for id in [&m.from, &m.into] {
                if !is_family_id(id) || *id != id.to_lowercase() {
                    return Err(format!("{id:?} is not a lowercase family id"));
                }
            }
            if m.from == m.into {
                return Err(format!("{:?} can't be merged into itself", m.from));
            }
            if m.name.trim().is_empty() {
                return Err(format!("the merge into {:?} has no name", m.into));
            }
            if self.merge[..i].iter().any(|o| o.from == m.from) {
                return Err(format!("{:?} is merged twice", m.from));
            }
            if self.merge.iter().any(|o| o.from == m.into) {
                return Err(format!(
                    "{:?} is merged away itself, so nothing can join it",
                    m.into
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::How;

    fn app(family: &str, name: &str, exe: &str) -> App {
        App {
            family: family.into(),
            name: name.into(),
            exe: exe.into(),
            path: None,
            how: How::Pack,
        }
    }

    fn merge(from: &str, into: &str, name: &str) -> Merge {
        Merge {
            from: from.into(),
            from_name: String::new(),
            into: into.into(),
            name: name.into(),
        }
    }

    #[test]
    fn nothing_changes_by_default() {
        let a = app("steam", "Steam", "steam.exe");
        assert_eq!(Families::default().apply(a.clone()), a);
    }

    #[test]
    fn a_split_program_is_an_app_of_its_own() {
        let f = Families {
            split: vec!["steamwebhelper.exe".into()],
            ..Families::default()
        };
        let a = f.apply(app("steam", "Steam", "steamwebhelper.exe"));
        assert_eq!(a.family, "split:steamwebhelper.exe");
        assert_eq!(a.name, "Steam (steamwebhelper.exe)");
        // Its siblings stay.
        assert_eq!(f.apply(app("steam", "Steam", "steam.exe")).family, "steam");
        // System and Unknown have no exe: never split.
        let sys = app("system", "System", "");
        assert_eq!(f.apply(sys.clone()), sys);
    }

    #[test]
    fn a_merged_family_joins_the_other_after_splits() {
        let f = Families {
            split: vec!["helper.exe".into()],
            merge: vec![
                merge(r"c:\tools\sync.exe", "dropbox", "Dropbox"),
                merge("split:helper.exe", "zoom", "Zoom"),
            ],
        };
        let a = f.apply(app(r"c:\tools\sync.exe", "Sync", "sync.exe"));
        assert_eq!((a.family.as_str(), a.name.as_str()), ("dropbox", "Dropbox"));
        let h = f.apply(app("teams", "Teams", "helper.exe"));
        assert_eq!((h.family.as_str(), h.name.as_str()), ("zoom", "Zoom"));
        assert!(f.validate().is_ok());
    }

    #[test]
    fn bad_changes_are_refused() {
        let bad = |f: Families| f.validate().unwrap_err();
        let split = |s: &str| Families {
            split: vec![s.into()],
            ..Families::default()
        };
        assert!(bad(split(r"c:\x\a.exe")).contains("file name"));
        assert!(bad(split("A.exe")).contains("file name"));
        assert!(
            bad(Families {
                split: vec!["a.exe".into(), "a.exe".into()],
                ..Families::default()
            })
            .contains("twice")
        );
        let m = |v: Vec<Merge>| Families {
            merge: v,
            ..Families::default()
        };
        assert!(bad(m(vec![merge("a", "a", "A")])).contains("itself"));
        assert!(bad(m(vec![merge("site:x", "a", "A")])).contains("family id"));
        assert!(bad(m(vec![merge("a", "b", " ")])).contains("no name"));
        assert!(bad(m(vec![merge("a", "b", "B"), merge("a", "c", "C")])).contains("twice"));
        assert!(bad(m(vec![merge("a", "b", "B"), merge("b", "c", "C")])).contains("merged away"));
    }

    #[test]
    fn family_ids() {
        assert!(is_family_id("steam"));
        assert!(is_family_id(r"c:\games\celeste.exe"));
        assert!(is_family_id("pkg:Microsoft.Zune_8wekyb3d8bbwe"));
        assert!(is_family_id("split:helper.exe"));
        assert!(!is_family_id(" "));
        assert!(!is_family_id("site:youtube"));
        assert!(!is_family_id("Family:steam"));
    }
}
