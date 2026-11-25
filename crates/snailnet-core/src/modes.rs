//! Modes (PLAN.md §11.2): the four built-in profiles the app shows as one
//! click each. A mode is an ordinary profile with a known id; its rules
//! start from a template here and the user can fine-tune them. "Reset the
//! split" puts a mode back to its template.
//!
//! The templates name only shipped families and sites, so they work on any
//! PC: a rule for an app that isn't installed just stays quiet.

use serde::{Deserialize, Serialize};

use crate::rules::{Config, Profile, Rule, Target};
use crate::scheduler::Priority;

/// The Gaming mode's id.
pub const GAMING: &str = "gaming";

/// (id, name) of each mode, in the order the app shows them.
pub const MODES: [(&str, &str); 6] = [
    ("balanced", "Balanced"),
    ("calls", "Calls"),
    ("focus", "Focus"),
    ("streaming", "Streaming"),
    ("gaming", "Gaming"),
    ("quiet-downloads", "Quiet downloads"),
];

/// What a mode puts first (2026-11-25: the old "What goes first" switches
/// became modes, one active at a time; Balanced puts nothing first).
/// `calls`: live calls go first in any app; `focus`: the app in front and
/// running games go first; `gaming`: use the Gaming mode while a game runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct First {
    pub calls: bool,
    pub focus: bool,
    pub gaming: bool,
}

/// The behavior a built-in mode carries. A user's own mode keeps the
/// behavior of the mode it was copied from (`Profile::first`).
pub fn first_for(id: &str) -> First {
    match id {
        "calls" => First {
            calls: true,
            ..First::default()
        },
        "focus" => First {
            focus: true,
            ..First::default()
        },
        // Video and calls stay smooth.
        "streaming" => First {
            calls: true,
            ..First::default()
        },
        // The game and voice chat come first, and Gaming takes over while
        // a game runs.
        "gaming" => First {
            calls: true,
            focus: true,
            gaming: true,
        },
        _ => First::default(),
    }
}

/// Launchers, stores, updaters and sync clients: big transfers nobody is
/// watching.
const DOWNLOADERS: [&str; 10] = [
    "steam",
    "epic",
    "battlenet",
    "ea",
    "ubisoft",
    "gog",
    "windows-update",
    "onedrive",
    "dropbox",
    "qbittorrent",
];

fn rule(target: Target, share: u32, priority: Priority) -> Rule {
    Rule {
        target,
        share,
        priority,
        ceiling: None,
        enabled: true,
    }
}

/// Downloads that only use room nobody else wants.
fn downloads_when_free() -> impl Iterator<Item = Rule> {
    DOWNLOADERS
        .iter()
        .map(|id| rule(Target::family(id), 0, Priority::Low))
}

/// The starting rules of mode `id`, or `None` if `id` isn't a mode.
pub fn template(id: &str) -> Option<Vec<Rule>> {
    let rules = match id {
        // Everything shares the line fairly; what goes first is the mode's
        // own behavior (first_for), not a rule.
        "balanced" | "calls" | "focus" => Vec::new(),
        // Video and calls first; downloads in the spare room.
        "streaming" => [
            rule(Target::family("discord"), 1000, Priority::High),
            rule(Target::family("zoom"), 1000, Priority::High),
            rule(Target::site("youtube"), 3000, Priority::Normal),
            rule(Target::site("twitch"), 1500, Priority::Normal),
            rule(Target::site("netflix"), 1500, Priority::Normal),
        ]
        .into_iter()
        .chain(downloads_when_free())
        .collect(),
        // Games live in Everything else (they have no family): voice chat
        // goes first, video and downloads step back.
        "gaming" => [
            rule(Target::family("discord"), 1000, Priority::High),
            rule(Target::site("youtube"), 2000, Priority::Low),
            rule(Target::site("twitch"), 1000, Priority::Low),
        ]
        .into_iter()
        .chain(downloads_when_free())
        .collect(),
        "quiet-downloads" => downloads_when_free().collect(),
        _ => return None,
    };
    Some(rules)
}

pub fn is_mode(id: &str) -> bool {
    MODES.iter().any(|(m, _)| *m == id)
}

/// All four modes with their templates.
pub fn all() -> Vec<Profile> {
    MODES
        .iter()
        .map(|(id, name)| Profile {
            id: (*id).into(),
            name: (*name).into(),
            rules: template(id).unwrap_or_default(),
            first: None,
        })
        .collect()
}

impl Config {
    /// Add the modes this config doesn't have yet (an older config, or one
    /// written by hand), after its own profiles. Existing profiles, the
    /// user's Balanced included, are left as they are. Returns whether
    /// anything was added.
    pub fn add_missing_modes(&mut self) -> bool {
        let missing: Vec<Profile> = all()
            .into_iter()
            .filter(|m| !self.profiles.iter().any(|p| p.id == m.id))
            .collect();
        let added = !missing.is_empty();
        self.profiles.extend(missing);
        added
    }

    /// "Duplicate & edit" (PLAN.md §11.2): a new mode of the user's own,
    /// named `name`, with `from`'s rules, listed after the others. Returns
    /// its id (made from the name, never a built-in mode's).
    pub fn copy_profile(&mut self, from: &str, name: &str) -> Result<String, String> {
        let source = self
            .profiles
            .iter()
            .find(|p| p.id == from)
            .ok_or_else(|| format!("there is no profile {from:?}"))?;
        let rules = source.rules.clone();
        // A copy keeps what its source puts first.
        let first = if is_mode(from) {
            first_for(from)
        } else {
            source.first.unwrap_or_default()
        };
        if self.profiles.len() >= MAX_PROFILES {
            return Err(format!("at most {MAX_PROFILES} modes"));
        }
        let name = self.free_name(name, None)?;
        let base = slug(&name);
        let id = (1..)
            .map(|n| {
                if n == 1 {
                    base.clone()
                } else {
                    format!("{base}-{n}")
                }
            })
            .find(|id| !is_mode(id) && !self.profiles.iter().any(|p| &p.id == id))
            .expect("some id is free");
        self.profiles.push(Profile {
            id: id.clone(),
            name,
            rules,
            first: Some(first),
        });
        Ok(id)
    }

    /// Rename one of the user's own modes (built-in ones keep their names).
    pub fn rename_profile(&mut self, id: &str, name: &str) -> Result<(), String> {
        if is_mode(id) {
            return Err("a built-in mode keeps its name".into());
        }
        let name = self.free_name(name, Some(id))?;
        let p = self
            .profiles
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("there is no profile {id:?}"))?;
        p.name = name;
        Ok(())
    }

    /// Delete one of the user's own modes; if it was on, Balanced takes
    /// over (the first profile when there's no Balanced).
    pub fn delete_profile(&mut self, id: &str) -> Result<(), String> {
        if is_mode(id) {
            return Err("a built-in mode can't be deleted".into());
        }
        let at = self
            .profiles
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| format!("there is no profile {id:?}"))?;
        self.profiles.remove(at);
        if self.active_profile == id {
            self.active_profile = if self.profiles.iter().any(|p| p.id == MODES[0].0) {
                MODES[0].0.into()
            } else {
                self.profiles
                    .first()
                    .map(|p| p.id.clone())
                    .unwrap_or_default()
            };
        }
        Ok(())
    }

    /// `name` trimmed, if it's a usable name no other profile (but `except`)
    /// has.
    fn free_name(&self, name: &str, except: Option<&str>) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > MAX_NAME {
            return Err(format!("a mode's name is 1 to {MAX_NAME} characters"));
        }
        let taken = self
            .profiles
            .iter()
            .any(|p| Some(p.id.as_str()) != except && p.name.trim().eq_ignore_ascii_case(name));
        if taken {
            return Err(format!("there's already a mode called {name:?}"));
        }
        Ok(name.into())
    }
}

/// How many profiles a config may hold: the four modes and the user's own.
pub const MAX_PROFILES: usize = 20;
/// Longest name of a mode, in characters.
const MAX_NAME: usize = 40;

/// A profile id from a name: lowercase ASCII letters and digits, other runs
/// as one `-`, at most 24 long; "mode" when nothing is left.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out: String = out.chars().take(24).collect();
    let out = out.trim_end_matches('-');
    if out.is_empty() {
        "mode".into()
    } else {
        out.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_is_a_valid_profile() {
        let mut c = Config::default();
        assert_eq!(c.profiles.len(), MODES.len(), "the default has every mode");
        c.validate().unwrap();
        for (id, _) in MODES {
            c.active_profile = id.into();
            c.line.down_mbps = Some(25.0);
            c.plan(0).down.unwrap().validate().unwrap();
        }
        assert!(template("nope").is_none());
    }

    #[test]
    fn a_copied_mode_is_the_users_own() {
        let mut c = Config::default();
        let id = c.copy_profile("gaming", "  Gaming, my way ").unwrap();
        assert_eq!(id, "gaming-my-way");
        let p = c.profiles.last().unwrap();
        assert_eq!(p.name, "Gaming, my way");
        assert_eq!(p.rules, template("gaming").unwrap(), "the rules come along");
        c.validate().unwrap();

        // Ids never clash, with a built-in mode or another copy.
        assert_eq!(
            c.copy_profile("balanced", "Balanced!").unwrap(),
            "balanced-2"
        );
        assert_eq!(
            c.copy_profile("balanced", "balanced 2").unwrap(),
            "balanced-2-2"
        );
        assert_eq!(
            c.copy_profile("balanced", "Gaming my way").unwrap(),
            "gaming-my-way-2"
        );
        assert_eq!(c.copy_profile("balanced", "Über ★").unwrap(), "ber");
        assert_eq!(c.copy_profile("balanced", "★★").unwrap(), "mode");
        c.validate().unwrap();

        assert!(c.copy_profile("nope", "x").is_err());
        assert!(c.copy_profile("balanced", "   ").is_err());
        assert!(
            c.copy_profile("balanced", "gaming").is_err(),
            "names are unique, any case"
        );
    }

    #[test]
    fn only_the_users_own_modes_are_renamed_or_deleted() {
        let mut c = Config::default();
        let id = c.copy_profile("streaming", "Movie night").unwrap();
        c.rename_profile(&id, "Film night").unwrap();
        c.rename_profile(&id, "film NIGHT").unwrap();
        assert_eq!(c.active_profile, "balanced");
        assert!(c.rename_profile("gaming", "Games").is_err());
        assert!(c.rename_profile(&id, "Gaming").is_err(), "taken");

        c.active_profile = id.clone();
        c.delete_profile(&id).unwrap();
        assert_eq!(c.active_profile, "balanced", "Balanced takes over");
        assert!(c.delete_profile("gaming").is_err());
        assert!(c.delete_profile(&id).is_err(), "already gone");
        c.validate().unwrap();
    }

    #[test]
    fn there_is_a_limit() {
        let mut c = Config::default();
        for i in c.profiles.len()..MAX_PROFILES {
            c.copy_profile("balanced", &format!("Mine {i}")).unwrap();
        }
        assert!(c.copy_profile("balanced", "One more").is_err());
        c.validate().unwrap();
    }

    #[test]
    fn missing_modes_are_added_and_existing_ones_kept() {
        let mut c = Config::default();
        c.profiles.retain(|p| p.id == "balanced");
        c.profiles[0].rules = vec![rule(Target::family("brave"), 3000, Priority::Normal)];
        assert!(c.add_missing_modes());
        assert_eq!(
            c.profiles.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            [
                "balanced",
                "calls",
                "focus",
                "streaming",
                "gaming",
                "quiet-downloads"
            ]
        );
        assert_eq!(c.profiles[0].rules.len(), 1, "the user's Balanced is kept");
        assert!(!c.add_missing_modes(), "nothing to add twice");
        c.validate().unwrap();
    }
}
