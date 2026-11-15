//! Rules, profiles and settings: the model behind `config.toml` (PLAN.md
//! §11.1, §11.2, §13.2), its validation, and how the active profile becomes
//! scheduler classes.
//!
//! ```toml
//! version = 1
//! active_profile = "gaming"
//!
//! [settings]
//! low_latency = true
//!
//! [line]               # until the estimator exists (§10), speeds are typed in
//! down_mbps = 25.0
//!
//! [[profiles]]
//! id = "gaming"
//! name = "Gaming"
//!
//!   [[profiles.rules]]
//!   target = { kind = "family", id = "steam" }
//!   share = 4000                       # basis points
//!   priority = "low"
//!   ceiling = { down = "80%" }         # or "5 Mbps"
//! ```
//!
//! "Everything else" gets whatever the enabled rules leave, never less than
//! 5% ([`shares::normalize`]), so the file stores rule shares only.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::classify::App;
use crate::scheduler::{ClassConfig, Priority, SchedulerConfig, shares};
use crate::sites::{SiteDef, SitePack};

/// The schema version this build writes and reads.
pub const CONFIG_VERSION: u32 = 1;
/// Name of the last class, which takes every packet no rule claims.
pub const OTHER_CLASS: &str = "everything else";
/// A single rule can't take more than 95%: Everything else keeps 5%.
pub const MAX_RULE_SHARE_BP: u32 = shares::TOTAL_BP - shares::OTHER_FLOOR_BP;
/// Low-latency mode's shaping factors (PLAN.md §10.4).
pub const LOW_LATENCY_FACTOR_DOWN: f64 = 0.92;
pub const LOW_LATENCY_FACTOR_UP: f64 = 0.90;

const LINE_MBPS: std::ops::RangeInclusive<f64> = 0.1..=10_000.0;
const FACTOR: std::ops::RangeInclusive<f64> = 0.5..=1.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub active_profile: String,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub line: Line,
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// The user's own sites (PLAN.md §8.2 "Add a custom site"). One with a
    /// shipped site's id replaces that site.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sites: Vec<SiteDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Shape below the line rate so the router's buffer stays empty.
    pub low_latency: bool,
    /// Leave traffic to the local network alone.
    pub lan_exempt: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            low_latency: true,
            lan_exempt: true,
        }
    }
}

/// The line's speed. A direction without a speed is not shaped.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Line {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub down_mbps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub up_mbps: Option<f64>,
    /// Overrides Low-latency mode's factors for both directions (0.5–1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub factor: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Lowercase letters, digits, `-` and `_`.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub target: Target,
    /// Guaranteed share in basis points (0–9500). 0 = "only when free".
    pub share: u32,
    #[serde(default)]
    pub priority: Priority,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ceiling: Option<Ceiling>,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub kind: TargetKind,
    /// Family id (`steam`), exe (`game.exe` or `game`), or site (`youtube`).
    /// Matched case-insensitively.
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetKind {
    Family,
    Exe,
    /// A website from the site packs or the user's custom sites (PLAN.md
    /// §8). Matched per flow by name, in any app, before app rules (§8.3).
    Site,
}

impl Target {
    pub fn family(id: &str) -> Self {
        Self {
            kind: TargetKind::Family,
            id: id.into(),
        }
    }

    pub fn exe(id: &str) -> Self {
        Self {
            kind: TargetKind::Exe,
            id: id.into(),
        }
    }

    pub fn site(id: &str) -> Self {
        Self {
            kind: TargetKind::Site,
            id: id.into(),
        }
    }

    /// Does this target cover `app`? (Site targets never do: they match
    /// flows, see [`Plan::site_class`].)
    pub fn matches(&self, app: &App) -> bool {
        let id = self.id.to_lowercase();
        match self.kind {
            TargetKind::Family => app.family == id,
            TargetKind::Exe => {
                !app.exe.is_empty()
                    && (app.exe == id || app.exe.strip_suffix(".exe") == Some(id.as_str()))
            }
            TargetKind::Site => false,
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.kind {
            TargetKind::Family => "family",
            TargetKind::Exe => "exe",
            TargetKind::Site => "site",
        };
        write!(f, "{kind}:{}", self.id.to_lowercase())
    }
}

/// Optional hard caps per direction (PLAN.md D2).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ceiling {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub down: Option<Limit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub up: Option<Limit>,
}

/// `"20%"` of the shaped rate, or `"5 Mbps"`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Limit {
    /// 1–100.
    Percent(u32),
    Mbps(f64),
}

impl Limit {
    fn bytes_per_sec(self, shaped_rate: u64) -> u64 {
        match self {
            Self::Percent(p) => shaped_rate * p as u64 / 100,
            Self::Mbps(m) => (m * 1e6 / 8.0) as u64,
        }
        .max(1)
    }
}

impl TryFrom<String> for Limit {
    type Error = String;

    fn try_from(s: String) -> Result<Self, String> {
        let t = s.trim().to_lowercase();
        if let Some(p) = t.strip_suffix('%') {
            let p: u32 = p
                .trim()
                .parse()
                .map_err(|_| format!("{s:?}: the percentage must be a whole number"))?;
            if !(1..=100).contains(&p) {
                return Err(format!("{s:?}: a ceiling must be 1–100%"));
            }
            return Ok(Self::Percent(p));
        }
        let m = t.strip_suffix("mbps").ok_or_else(|| {
            format!("{s:?}: write a ceiling as a percentage (\"20%\") or a speed (\"5 Mbps\")")
        })?;
        let m: f64 = m
            .trim()
            .parse()
            .map_err(|_| format!("{s:?}: not a number of Mbps"))?;
        if !LINE_MBPS.contains(&m) {
            return Err(format!("{s:?}: a ceiling must be 0.1–10000 Mbps"));
        }
        Ok(Self::Mbps(m))
    }
}

impl From<Limit> for String {
    fn from(l: Limit) -> Self {
        match l {
            Limit::Percent(p) => format!("{p}%"),
            Limit::Mbps(m) => format!("{m} Mbps"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RulesError {
    #[error("not valid TOML: {0}")]
    Syntax(String),
    #[error("the version field is missing")]
    NoVersion,
    #[error(
        "version {0} was written by a newer SnailNet (this one reads version {CONFIG_VERSION})"
    )]
    TooNew(i64),
    #[error("version {0} is not a config version")]
    BadVersion(i64),
    #[error("{0}")]
    Invalid(String),
}

impl Default for Config {
    /// One "Balanced" profile with no rules: everything shares the line
    /// fairly, and nothing is shaped until a line speed is set.
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            active_profile: "balanced".into(),
            settings: Settings::default(),
            line: Line::default(),
            profiles: vec![Profile {
                id: "balanced".into(),
                name: "Balanced".into(),
                rules: Vec::new(),
            }],
            sites: Vec::new(),
        }
    }
}

impl Config {
    /// Parse, migrate and validate a config file's text.
    pub fn parse(text: &str) -> Result<Self, RulesError> {
        let table: toml::Table = text
            .parse()
            .map_err(|e: toml::de::Error| RulesError::Syntax(e.to_string()))?;
        let version = table
            .get("version")
            .ok_or(RulesError::NoVersion)?
            .as_integer()
            .ok_or(RulesError::NoVersion)?;
        match version {
            v if v == CONFIG_VERSION as i64 => {}
            v if v > CONFIG_VERSION as i64 => return Err(RulesError::TooNew(v)),
            // Migrations from older versions go here once there is a v2.
            v => return Err(RulesError::BadVersion(v)),
        }
        let config: Self = toml::from_str(text).map_err(|e| RulesError::Syntax(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("a config always serialises")
    }

    pub fn validate(&self) -> Result<(), RulesError> {
        let bad = |m: String| Err(RulesError::Invalid(m));
        if self.version != CONFIG_VERSION {
            return Err(RulesError::BadVersion(self.version.into()));
        }
        for (name, v) in [
            ("down_mbps", self.line.down_mbps),
            ("up_mbps", self.line.up_mbps),
        ] {
            if let Some(v) = v
                && !LINE_MBPS.contains(&v)
            {
                return bad(format!("line.{name} = {v} is outside 0.1–10000 Mbps"));
            }
        }
        if let Some(f) = self.line.factor
            && !FACTOR.contains(&f)
        {
            return bad(format!("line.factor = {f} is outside 0.5–1.0"));
        }
        if self.profiles.is_empty() {
            return bad("there are no profiles".into());
        }
        for (i, p) in self.profiles.iter().enumerate() {
            let ok_id = !p.id.is_empty()
                && p.id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
            if !ok_id {
                return bad(format!(
                    "profile id {:?} must be lowercase letters, digits, - or _",
                    p.id
                ));
            }
            if self.profiles[..i].iter().any(|q| q.id == p.id) {
                return bad(format!("profile id {:?} is used twice", p.id));
            }
            p.validate()
                .map_err(|m| RulesError::Invalid(format!("profile {:?}: {m}", p.id)))?;
        }
        if !self.profiles.iter().any(|p| p.id == self.active_profile) {
            return bad(format!(
                "active_profile {:?} is not one of the profiles",
                self.active_profile
            ));
        }
        for (i, s) in self.sites.iter().enumerate() {
            s.validate()
                .map_err(|e| RulesError::Invalid(e.to_string()))?;
            if self.sites[..i].iter().any(|t| t.id == s.id) {
                return bad(format!("custom site {:?} is defined twice", s.id));
            }
        }
        let pack = SitePack::builtin();
        let known = |id: &str| {
            pack.sites.iter().any(|s| s.id == id) || self.sites.iter().any(|s| s.id == id)
        };
        for p in &self.profiles {
            for r in &p.rules {
                if r.target.kind == TargetKind::Site && !known(&r.target.id.to_lowercase()) {
                    return bad(format!(
                        "profile {:?}: {} is not a known site",
                        p.id, r.target
                    ));
                }
            }
        }
        Ok(())
    }

    /// The active profile. Validation guarantees it exists.
    pub fn active(&self) -> &Profile {
        self.profiles
            .iter()
            .find(|p| p.id == self.active_profile)
            .unwrap_or(&self.profiles[0])
    }

    /// What the scheduler should do under the active profile.
    pub fn plan(&self) -> Plan {
        let profile = self.active();
        let rules: Vec<&Rule> = profile.rules.iter().filter(|r| r.enabled).collect();
        let bp = shares::normalize(&rules.iter().map(|r| r.share).collect::<Vec<_>>());
        let factor = |default: f64| {
            self.line.factor.unwrap_or(if self.settings.low_latency {
                default
            } else {
                1.0
            })
        };
        let direction = |mbps: Option<f64>, factor: f64, down: bool| {
            let rate = (mbps? * factor * 1e6 / 8.0) as u64;
            let mut classes: Vec<ClassConfig> = rules
                .iter()
                .zip(&bp)
                .map(|(r, &share_bp)| {
                    let limit = r
                        .ceiling
                        .as_ref()
                        .and_then(|c| if down { c.down } else { c.up });
                    ClassConfig {
                        prio: r.priority,
                        ceil_bytes_per_sec: limit.map(|l| l.bytes_per_sec(rate)),
                        ..ClassConfig::new(r.target.to_string(), share_bp)
                    }
                })
                .collect();
            classes.push(ClassConfig::new(OTHER_CLASS, bp[rules.len()]));
            Some(SchedulerConfig {
                rate_bytes_per_sec: rate.max(1),
                // ~2 ms of line rate keeps the release smooth.
                burst_bytes: (rate / 500).max(2 * 1514),
                classes,
            })
        };
        Plan {
            down: direction(self.line.down_mbps, factor(LOW_LATENCY_FACTOR_DOWN), true),
            up: direction(self.line.up_mbps, factor(LOW_LATENCY_FACTOR_UP), false),
            targets: rules.iter().map(|r| r.target.clone()).collect(),
            prios: rules.iter().map(|r| r.priority).collect(),
            shape_lan: !self.settings.lan_exempt,
            custom_sites: self.sites.clone(),
        }
    }
}

impl Profile {
    fn validate(&self) -> Result<(), String> {
        let mut sum = 0u32;
        for (i, r) in self.rules.iter().enumerate() {
            if r.target.id.trim().is_empty() {
                return Err(format!("rule {} has an empty target id", i + 1));
            }
            if self.rules[..i]
                .iter()
                .any(|q| q.target.to_string() == r.target.to_string())
            {
                return Err(format!("{} has more than one rule", r.target));
            }
            if r.share > MAX_RULE_SHARE_BP {
                return Err(format!(
                    "{} has share {} bp; one rule can have at most {MAX_RULE_SHARE_BP}",
                    r.target, r.share
                ));
            }
            if r.enabled {
                sum += r.share;
            }
        }
        if sum > shares::TOTAL_BP {
            return Err(format!("enabled rules add up to {sum} bp, more than 10000"));
        }
        Ok(())
    }
}

/// The active profile, ready for the fast path. Class `i < targets.len()`
/// is rule `i`; the last class is Everything else.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// `None`: that direction isn't shaped (no line speed).
    pub down: Option<SchedulerConfig>,
    pub up: Option<SchedulerConfig>,
    pub targets: Vec<Target>,
    pub prios: Vec<Priority>,
    pub shape_lan: bool,
    /// For the domain → site matcher, with the shipped packs.
    pub custom_sites: Vec<SiteDef>,
}

impl Plan {
    pub fn shapes_anything(&self) -> bool {
        self.down.is_some() || self.up.is_some()
    }

    /// Everything else's class id.
    pub fn other_class(&self) -> usize {
        self.targets.len()
    }

    /// The class `app`'s traffic goes to: the first matching rule, else
    /// Everything else.
    pub fn class_of(&self, app: &App) -> usize {
        self.targets
            .iter()
            .position(|t| t.matches(app))
            .unwrap_or(self.other_class())
    }

    /// The class of a flow recognised as site `id`, if a rule claims that
    /// site. Takes precedence over [`Plan::class_of`] (PLAN.md §8.3).
    pub fn site_class(&self, id: &str) -> Option<usize> {
        self.targets
            .iter()
            .position(|t| t.kind == TargetKind::Site && t.id.eq_ignore_ascii_case(id))
    }

    /// Whether flows need to be named at all.
    pub fn has_site_rules(&self) -> bool {
        self.targets.iter().any(|t| t.kind == TargetKind::Site)
    }

    pub fn prio_of(&self, class: usize) -> Priority {
        self.prios.get(class).copied().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::How;

    const SAMPLE: &str = r#"
version = 1
active_profile = "gaming"

[settings]
low_latency = true
lan_exempt = true

[line]
down_mbps = 25.0
up_mbps = 10.0

[[profiles]]
id = "balanced"
name = "Balanced"

[[profiles]]
id = "gaming"
name = "Gaming"

  [[profiles.rules]]
  target = { kind = "family", id = "steam" }
  share = 4000
  priority = "low"
  ceiling = { down = "80%", up = "2 Mbps" }

  [[profiles.rules]]
  target = { kind = "site", id = "youtube" }
  share = 4500

  [[profiles.rules]]
  target = { kind = "exe", id = "Game.exe" }
  share = 1000
  priority = "high"
  enabled = false
"#;

    fn app(family: &str, exe: &str) -> App {
        App {
            family: family.into(),
            name: String::new(),
            exe: exe.into(),
            how: How::Pack,
        }
    }

    fn with_profile(rules: &str) -> String {
        format!(
            "version = 1\nactive_profile = \"p\"\n[[profiles]]\nid = \"p\"\nname = \"P\"\n{rules}"
        )
    }

    fn rule(kind: &str, id: &str, share: u32) -> String {
        format!(
            "[[profiles.rules]]\ntarget = {{ kind = \"{kind}\", id = \"{id}\" }}\nshare = {share}\n"
        )
    }

    fn err(text: &str) -> String {
        Config::parse(text).unwrap_err().to_string()
    }

    #[test]
    fn default_round_trips() {
        let c = Config::default();
        c.validate().unwrap();
        assert_eq!(Config::parse(&c.to_toml()).unwrap(), c);
        let p = c.plan();
        assert!(!p.shapes_anything(), "no line speed, nothing shaped");
    }

    #[test]
    fn sample_parses_and_round_trips() {
        let c = Config::parse(SAMPLE).unwrap();
        assert_eq!(c.active().name, "Gaming");
        assert_eq!(c.active().rules.len(), 3);
        assert_eq!(
            c.active().rules[0].ceiling.as_ref().unwrap().up,
            Some(Limit::Mbps(2.0))
        );
        assert!(!c.active().rules[2].enabled);
        assert_eq!(Config::parse(&c.to_toml()).unwrap(), c);
    }

    #[test]
    fn plan_from_sample() {
        let p = Config::parse(SAMPLE).unwrap().plan();
        // The disabled rule takes no class.
        assert_eq!(
            p.targets.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
            ["family:steam", "site:youtube"]
        );
        let down = p.down.as_ref().unwrap();
        assert_eq!(down.rate_bytes_per_sec, (25e6 * 0.92 / 8.0) as u64);
        let names: Vec<_> = down.classes.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["family:steam", "site:youtube", OTHER_CLASS]);
        let bp: Vec<_> = down.classes.iter().map(|c| c.share_bp).collect();
        assert_eq!(bp, [4000, 4500, 1500]);
        assert_eq!(down.classes[0].prio, Priority::Low);
        assert_eq!(
            down.classes[0].ceil_bytes_per_sec,
            Some(down.rate_bytes_per_sec * 80 / 100)
        );
        assert_eq!(down.classes[1].ceil_bytes_per_sec, None);
        down.validate().unwrap();
        let up = p.up.as_ref().unwrap();
        assert_eq!(up.rate_bytes_per_sec, (10e6 * 0.90 / 8.0) as u64);
        assert_eq!(up.classes[0].ceil_bytes_per_sec, Some(250_000));
        up.validate().unwrap();
        assert!(!p.shape_lan);
    }

    #[test]
    fn low_latency_off_and_factor_override() {
        let mut c = Config::parse(SAMPLE).unwrap();
        c.settings.low_latency = false;
        assert_eq!(
            c.plan().down.unwrap().rate_bytes_per_sec,
            (25e6 / 8.0) as u64
        );
        c.line.factor = Some(0.85);
        let p = c.plan();
        assert_eq!(
            p.down.unwrap().rate_bytes_per_sec,
            (25e6 * 0.85 / 8.0) as u64
        );
        assert_eq!(p.up.unwrap().rate_bytes_per_sec, (10e6 * 0.85 / 8.0) as u64);
    }

    #[test]
    fn rules_over_95_percent_keep_the_floor() {
        let text = with_profile(&(rule("family", "a", 6000) + &rule("family", "b", 4000)));
        let mut c = Config::parse(&text).unwrap();
        c.line.down_mbps = Some(25.0);
        let bp: Vec<_> = c
            .plan()
            .down
            .unwrap()
            .classes
            .iter()
            .map(|c| c.share_bp)
            .collect();
        assert_eq!(bp, [5700, 3800, 500]);
    }

    #[test]
    fn zero_share_is_allowed() {
        let mut c = Config::parse(&with_profile(&rule("family", "windows-update", 0))).unwrap();
        c.line.down_mbps = Some(25.0);
        c.plan().down.unwrap().validate().unwrap();
    }

    #[test]
    fn matching() {
        let p = Config::parse(&with_profile(
            &(rule("family", "Steam", 100)
                + &rule("exe", "game", 100)
                + &rule("site", "youtube", 100)),
        ))
        .unwrap()
        .plan();
        assert_eq!(p.class_of(&app("steam", "steamwebhelper.exe")), 0);
        assert_eq!(p.class_of(&app("c:\\games\\game.exe", "game.exe")), 1);
        assert_eq!(p.class_of(&app("brave", "brave.exe")), p.other_class());
        // System has no exe; an empty name never matches an exe rule.
        assert!(!Target::exe("").matches(&app("system", "")));
        assert!(!Target::exe(".exe").matches(&app("system", "")));
        assert_eq!(p.prio_of(p.other_class()), Priority::Normal);
    }

    #[test]
    fn version_checks() {
        assert_eq!(
            err("active_profile = \"x\""),
            RulesError::NoVersion.to_string()
        );
        assert!(err(&SAMPLE.replace("version = 1", "version = 2")).contains("newer SnailNet"));
        assert!(
            err(&SAMPLE.replace("version = 1", "version = 0")).contains("not a config version")
        );
        assert!(err("version = 1\nactive_profile = ").contains("TOML"));
    }

    #[test]
    fn rejects_bad_configs() {
        // Typos are errors, not silently ignored.
        assert!(err(&SAMPLE.replace("low_latency", "low_latncy")).contains("unknown field"));
        assert!(err(&SAMPLE.replace("\"80%\"", "\"0%\"")).contains("1–100%"));
        assert!(err(&SAMPLE.replace("\"80%\"", "\"80\"")).contains("percentage"));
        assert!(err(&SAMPLE.replace("down_mbps = 25.0", "down_mbps = 0.0")).contains("down_mbps"));
        assert!(
            err(&SAMPLE.replace("active_profile = \"gaming\"", "active_profile = \"x\""))
                .contains("active_profile")
        );
        assert!(
            err(&SAMPLE.replace("id = \"balanced\"", "id = \"gaming\"")).contains("used twice")
        );
        assert!(
            err(&SAMPLE.replace("id = \"balanced\"", "id = \"Bal anced\"")).contains("lowercase")
        );
        assert!(
            err(&with_profile(
                &(rule("family", "a", 6000) + &rule("family", "b", 5000))
            ))
            .contains("more than 10000")
        );
        assert!(err(&with_profile(&rule("family", "a", 9600))).contains("at most 9500"));
        assert!(
            err(&with_profile(
                &(rule("family", "a", 10) + &rule("family", "A", 10))
            ))
            .contains("more than one rule")
        );
        assert!(err(&with_profile(&rule("family", " ", 10))).contains("empty target"));
        assert!(err("version = 1\nactive_profile = \"x\"").contains("no profiles"));
    }

    #[test]
    fn limits_parse_and_print() {
        let l = |s: &str| Limit::try_from(s.to_string());
        assert_eq!(l(" 20 % "), Ok(Limit::Percent(20)));
        assert_eq!(l("5 Mbps"), Ok(Limit::Mbps(5.0)));
        assert_eq!(l("2.5mbps"), Ok(Limit::Mbps(2.5)));
        assert!(l("101%").is_err());
        assert!(l("0 Mbps").is_err());
        assert!(l("fast").is_err());
        assert_eq!(String::from(Limit::Mbps(2.5)), "2.5 Mbps");
        assert_eq!(String::from(Limit::Percent(20)), "20%");
    }

    #[test]
    fn site_rules_take_their_own_class() {
        let text = with_profile(&(rule("family", "brave", 3000) + &rule("site", "YouTube", 4000)));
        let p = Config::parse(&text).unwrap().plan();
        assert!(p.has_site_rules());
        assert_eq!(p.site_class("youtube"), Some(1));
        assert_eq!(p.site_class("twitch"), None);
        let app = App {
            family: "youtube".into(),
            ..App::unknown()
        };
        assert_eq!(
            p.class_of(&app),
            p.other_class(),
            "a site never matches an app"
        );
        let no_sites = Config::parse(&with_profile(&rule("family", "brave", 3000)))
            .unwrap()
            .plan();
        assert!(!no_sites.has_site_rules());
    }

    #[test]
    fn unknown_sites_are_refused_and_custom_ones_accepted() {
        let e = err(&with_profile(&rule("site", "myintranet", 1000)));
        assert!(e.contains("site:myintranet is not a known site"), "{e}");
        let text = with_profile(&rule("site", "myintranet", 1000))
            + "[[sites]]\nid = \"myintranet\"\nname = \"Intranet\"\ndomains = [\"*.corp.example\"]\n";
        let c = Config::parse(&text).unwrap();
        assert_eq!(c.plan().custom_sites[0].id, "myintranet");
        assert_eq!(Config::parse(&c.to_toml()).unwrap(), c, "round trip");
        let twice = text.clone()
            + "[[sites]]\nid = \"myintranet\"\nname = \"Again\"\ndomains = [\"a.example\"]\n";
        assert!(err(&twice).contains("defined twice"));
        let bad = with_profile("")
            + "[[sites]]\nid = \"x\"\nname = \"X\"\ndomains = [\"not a domain\"]\n";
        assert!(err(&bad).contains("not a domain pattern"));
    }
}
