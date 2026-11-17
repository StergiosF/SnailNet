//! Modes (PLAN.md §11.2): the four built-in profiles the app shows as one
//! click each. A mode is an ordinary profile with a known id; its rules
//! start from a template here and the user can fine-tune them. "Reset the
//! split" puts a mode back to its template.
//!
//! The templates name only shipped families and sites, so they work on any
//! PC: a rule for an app that isn't installed just stays quiet.

use crate::rules::{Config, Profile, Rule, Target};
use crate::scheduler::Priority;

/// (id, name) of each mode, in the order the app shows them.
pub const MODES: [(&str, &str); 4] = [
    ("balanced", "Balanced"),
    ("streaming", "Streaming"),
    ("gaming", "Gaming"),
    ("quiet-downloads", "Quiet downloads"),
];

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
        // Everything shares the line fairly.
        "balanced" => Vec::new(),
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
    fn missing_modes_are_added_and_existing_ones_kept() {
        let mut c = Config::default();
        c.profiles.retain(|p| p.id == "balanced");
        c.profiles[0].rules = vec![rule(Target::family("brave"), 3000, Priority::Normal)];
        assert!(c.add_missing_modes());
        assert_eq!(
            c.profiles.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["balanced", "streaming", "gaming", "quiet-downloads"]
        );
        assert_eq!(c.profiles[0].rules.len(), 1, "the user's Balanced is kept");
        assert!(!c.add_missing_modes(), "nothing to add twice");
        c.validate().unwrap();
    }
}
