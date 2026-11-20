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
use crate::families::{Families, is_family_id};
use crate::scheduler::{ClassConfig, Priority, SchedulerConfig, shares};
use crate::sites::SiteDef;

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
    /// The network `line` belongs to (PLAN.md §10.3); absent until the
    /// service first sees one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<Network>,
    /// The speeds of the other networks seen, kept for when they're back.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub networks: Vec<SavedNetwork>,
    /// One app or site going first for a while, in every mode (PLAN.md
    /// §11.5 "Boost").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boost: Option<Boost>,
    /// SnailNet steps aside (no shaping, no blocking) until it ends
    /// (PLAN.md §11.8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pause: Option<Pause>,
    /// Apps (family ids) with no internet, in every mode (PLAN.md §11.3).
    /// Their local-network traffic still passes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked: Vec<String>,
    /// Programs split out of their family and families merged into
    /// another (PLAN.md §7.2, §11.6).
    #[serde(default, skip_serializing_if = "Families::is_empty")]
    pub families: Families,
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
    /// The app in front, and games while they run, go first (High
    /// priority, never a bigger share; PLAN.md §11.5).
    pub focus_boost: bool,
    /// Use the Gaming mode while a game runs, then go back (PLAN.md §11.5
    /// "Optional automation"). Off by default.
    pub gaming_while_playing: bool,
    /// Live calls (and games' server streams), spotted by their traffic,
    /// go first in any app and any mode (PLAN.md §11.9). On by default.
    pub calls_first: bool,
    /// Apps (family ids) that are games whatever the detection says: they
    /// stay first while they run (PLAN.md §11.5 overrides).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub games: Vec<String>,
    /// Apps (family ids) never put first, in front or not, and never
    /// counted as a game.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub never_first: Vec<String>,
    /// What happens the first time an app goes online (PLAN.md §11.4).
    pub new_apps: NewApps,
    /// Fetch newer signed site, app and game lists once a day (PLAN.md
    /// §17.2). On by default.
    pub pack_updates: bool,
}

/// What SnailNet does when an app it hasn't seen before goes online
/// (PLAN.md §11.4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NewApps {
    /// Let it go online and say so in a notification.
    #[default]
    Notify,
    /// Hold its new connections until the user answers, at most
    /// [`STRICT_HOLD_SECS`]; then let it go online (as Notify would).
    Strict,
    /// Say nothing.
    Off,
}

/// How long strict mode holds a new app's connections (PLAN.md §11.4).
/// Short enough that a held TCP connect is still retrying its SYN.
pub const STRICT_HOLD_SECS: u64 = 20;

impl Default for Settings {
    fn default() -> Self {
        Self {
            low_latency: true,
            lan_exempt: true,
            focus_boost: true,
            gaming_while_playing: false,
            calls_first: true,
            games: Vec::new(),
            never_first: Vec::new(),
            new_apps: NewApps::Notify,
            pack_updates: true,
        }
    }
}

impl Settings {
    /// The user said `family` is a game.
    pub fn is_game(&self, family: &str) -> bool {
        self.games.iter().any(|g| g.eq_ignore_ascii_case(family))
    }

    /// The user said `family` never goes first.
    pub fn never_first(&self, family: &str) -> bool {
        self.never_first
            .iter()
            .any(|g| g.eq_ignore_ascii_case(family))
    }

    fn validate(&self) -> Result<(), String> {
        for (what, list) in [
            ("game", &self.games),
            ("never-first app", &self.never_first),
        ] {
            for (i, id) in list.iter().enumerate() {
                if !is_family_id(id) {
                    return Err(format!("{what} {id:?} is not a family id"));
                }
                if list[..i].iter().any(|b| b.eq_ignore_ascii_case(id)) {
                    return Err(format!("{id:?} is listed twice as a {what}"));
                }
            }
        }
        if let Some(id) = self.games.iter().find(|g| self.never_first(g)) {
            return Err(format!("{id:?} can't be a game and never first"));
        }
        Ok(())
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
    /// Keep the typed speeds as they are: the estimator (§10.2) doesn't
    /// adapt them. Absent = adapt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    /// The last speed test (§10.1); its speeds were saved as the typed ones.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measured: Option<Measured>,
}

/// What a speed test found (PLAN.md §10.1), run with shaping off.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Measured {
    /// Unix seconds.
    pub at: u64,
    pub down_mbps: f64,
    pub up_mbps: f64,
    /// Ping to the public host before the test, ms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_ms: Option<f64>,
    /// Ping during the download, without SnailNet, ms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loaded_ms: Option<f64>,
    /// Ping during the upload, without SnailNet, ms (uploads often bloat
    /// worse than downloads).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loaded_up_ms: Option<f64>,
    /// Ping during the check download right after the test, *through*
    /// SnailNet, ms (§11.10). Absent until that check finishes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub with_ms: Option<f64>,
}

impl Line {
    pub fn is_pinned(&self) -> bool {
        self.pinned == Some(true)
    }

    fn validate(&self, at: &str) -> Result<(), String> {
        for (name, v) in [("down_mbps", self.down_mbps), ("up_mbps", self.up_mbps)] {
            if let Some(v) = v
                && !LINE_MBPS.contains(&v)
            {
                return Err(format!("{at}.{name} = {v} is outside 0.1–10000 Mbps"));
            }
        }
        if let Some(f) = self.factor
            && !FACTOR.contains(&f)
        {
            return Err(format!("{at}.factor = {f} is outside 0.5–1.0"));
        }
        Ok(())
    }
}

/// A network, as the service tells them apart (PLAN.md §10.3): `id` is
/// the router's hardware address, so one line is one id whether reached by
/// Wi-Fi or cable; `name` is the Wi-Fi name or the adapter's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Network {
    pub id: String,
    pub name: String,
}

/// Another network's line, kept while away from it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedNetwork {
    pub id: String,
    pub name: String,
    /// When it was left, Unix seconds.
    pub left: u64,
    #[serde(default)]
    pub line: Line,
}

/// How many other networks' speeds are kept; the longest-gone goes first.
pub const MAX_SAVED_NETWORKS: usize = 20;

/// A boost: `target` goes first (High) with at least
/// [`BOOST_SHARE_BP`] and no ceiling, until `until` (Unix seconds) or until
/// it's stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Boost {
    pub target: Target,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<u64>,
}

impl Boost {
    pub fn is_on(&self, now: u64) -> bool {
        self.until.is_none_or(|u| now < u)
    }
}

/// A pause: until `until` (Unix seconds), or until resumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pause {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<u64>,
}

impl Pause {
    pub fn is_on(&self, now: u64) -> bool {
        self.until.is_none_or(|u| now < u)
    }
}

/// What a boosted target is guaranteed at least: half the line.
pub const BOOST_SHARE_BP: u32 = 5000;

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
    /// The four modes ([`crate::modes`]) with Balanced active: everything
    /// shares the line fairly, and nothing is shaped until a line speed is
    /// set.
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            active_profile: "balanced".into(),
            settings: Settings::default(),
            line: Line::default(),
            network: None,
            networks: Vec::new(),
            boost: None,
            pause: None,
            blocked: Vec::new(),
            families: Families::default(),
            profiles: crate::modes::all(),
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
        self.line.validate("line").map_err(RulesError::Invalid)?;
        for n in &self.networks {
            n.line
                .validate(&format!("networks {:?} line", n.name))
                .map_err(RulesError::Invalid)?;
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
        for (i, id) in self.blocked.iter().enumerate() {
            if !is_family_id(id) {
                return bad(format!("blocked app {id:?} is not a family id"));
            }
            if self.blocked[..i].iter().any(|b| b.eq_ignore_ascii_case(id)) {
                return bad(format!("{id:?} is blocked twice"));
            }
        }
        self.settings.validate().or_else(bad)?;
        self.families.validate().or_else(bad)?;
        if let Some(b) = &self.boost
            && b.target.id.trim().is_empty()
        {
            return bad("the boost has an empty target id".into());
        }
        Ok(())
    }

    /// Every site a rule or the boost names is a known one: in the packs
    /// in use, the compiled-in ones, or the custom sites. Checked on a
    /// change, not on load: a saved rule whose site later left the packs
    /// just matches nothing, rather than making the whole config invalid.
    pub fn check_sites(&self) -> Result<(), RulesError> {
        let packs = crate::packs::current();
        let known = |id: &str| packs.knows_site(id) || self.sites.iter().any(|s| s.id == id);
        for p in &self.profiles {
            for r in &p.rules {
                if r.target.kind == TargetKind::Site && !known(&r.target.id.to_lowercase()) {
                    return Err(RulesError::Invalid(format!(
                        "profile {:?}: {} is not a known site",
                        p.id, r.target
                    )));
                }
            }
        }
        if let Some(b) = &self.boost
            && b.target.kind == TargetKind::Site
            && !known(&b.target.id.to_lowercase())
        {
            return Err(RulesError::Invalid(format!(
                "boost: {} is not a known site",
                b.target
            )));
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

    /// The pause, if it's still on at `now` (Unix seconds).
    pub fn pause_at(&self, now: u64) -> Option<Pause> {
        self.pause.filter(|p| p.is_on(now))
    }

    /// The boost, if it's still on at `now` (Unix seconds).
    pub fn boost_at(&self, now: u64) -> Option<&Boost> {
        self.boost.as_ref().filter(|b| b.is_on(now))
    }

    /// The active profile's rules with the boost (if on at `now`) applied:
    /// the boosted target first, High, at least [`BOOST_SHARE_BP`], no
    /// ceiling; the other enabled rules shrink in proportion when they no
    /// longer fit above Everything else's 5%.
    pub fn effective_rules(&self, now: u64) -> Vec<Rule> {
        let mut rules = self.active().rules.clone();
        let Some(b) = self.boost_at(now) else {
            return rules;
        };
        let key = b.target.to_string();
        let mut boosted = match rules.iter().position(|r| r.target.to_string() == key) {
            Some(i) => rules.remove(i),
            None => Rule {
                target: b.target.clone(),
                share: 0,
                priority: Priority::Normal,
                ceiling: None,
                enabled: true,
            },
        };
        boosted.enabled = true;
        boosted.priority = Priority::High;
        boosted.ceiling = None;
        boosted.share = boosted.share.max(BOOST_SHARE_BP);
        let room = u64::from(MAX_RULE_SHARE_BP - boosted.share);
        let others: u64 = rules
            .iter()
            .filter(|r| r.enabled)
            .map(|r| u64::from(r.share))
            .sum();
        if others > room {
            for r in rules.iter_mut().filter(|r| r.enabled) {
                r.share = (u64::from(r.share) * room / others) as u32;
            }
        }
        rules.insert(0, boosted);
        rules
    }

    /// What the scheduler should do under the active profile at `now`
    /// (Unix seconds; it decides whether the boost and the pause are still
    /// on). While paused: nothing at all.
    pub fn plan(&self, now: u64) -> Plan {
        self.plan_live(now, [None, None])
    }

    /// [`Config::plan`], with each shaped direction's rate replaced by the
    /// estimator's (`[down, up]`, bits per second, Low-latency factor
    /// already applied). Percentage speed limits follow the new rate.
    pub fn plan_live(&self, now: u64, live: [Option<f64>; 2]) -> Plan {
        if self.pause_at(now).is_some() {
            return Plan {
                down: None,
                up: None,
                targets: Vec::new(),
                prios: Vec::new(),
                blocked: Vec::new(),
                strict: false,
                families: self.families.clone(),
                shape_lan: !self.settings.lan_exempt,
                custom_sites: self.sites.clone(),
                focus: Vec::new(),
                calls_first: false,
                never_first: Vec::new(),
            };
        }
        let all = self.effective_rules(now);
        let rules: Vec<&Rule> = all.iter().filter(|r| r.enabled).collect();
        let bp = shares::normalize(&rules.iter().map(|r| r.share).collect::<Vec<_>>());
        let factor = |default: f64| {
            self.line.factor.unwrap_or(if self.settings.low_latency {
                default
            } else {
                1.0
            })
        };
        let direction = |mbps: Option<f64>, factor: f64, down: bool| {
            let typed = mbps? * factor * 1e6;
            let bps = live[usize::from(!down)].filter(|b| b.is_finite() && *b > 0.0);
            let rate = (bps.unwrap_or(typed) / 8.0) as u64;
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
                burst_bytes: crate::scheduler::root_burst_bytes(rate),
                classes,
            })
        };
        Plan {
            down: direction(self.line.down_mbps, factor(LOW_LATENCY_FACTOR_DOWN), true),
            up: direction(self.line.up_mbps, factor(LOW_LATENCY_FACTOR_UP), false),
            targets: rules.iter().map(|r| r.target.clone()).collect(),
            prios: rules.iter().map(|r| r.priority).collect(),
            blocked: self.blocked.iter().map(|id| id.to_lowercase()).collect(),
            strict: self.settings.new_apps == NewApps::Strict,
            families: self.families.clone(),
            shape_lan: !self.settings.lan_exempt,
            custom_sites: self.sites.clone(),
            focus: Vec::new(),
            calls_first: self.settings.calls_first,
            never_first: self
                .settings
                .never_first
                .iter()
                .map(|f| f.to_lowercase())
                .collect(),
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
    /// Lowercase family ids whose internet traffic is dropped.
    pub blocked: Vec<String>,
    /// Strict new-app mode (PLAN.md §11.4): the service may add held apps
    /// to `blocked` at any time, so a running engine diverts both
    /// directions from the start (a hold then never restarts shaping).
    /// Strict alone doesn't start the engine.
    pub strict: bool,
    /// How the user groups apps (PLAN.md §11.6); the engine shows its
    /// apps this way, so rules and blocks name the grouped families.
    pub families: Families,
    pub shape_lan: bool,
    /// For the domain → site matcher, with the shipped packs.
    pub custom_sites: Vec<SiteDef>,
    /// Processes whose apps go first (High priority) whatever their class:
    /// the app in front and running games (PLAN.md §11.5). Not from the
    /// config; the service fills it in.
    pub focus: Vec<u32>,
    /// Live calls go first (PLAN.md §11.9), from `Settings::calls_first`.
    pub calls_first: bool,
    /// Lowercase family ids that never go first, calls included.
    pub never_first: Vec<String>,
}

impl Plan {
    pub fn shapes_anything(&self) -> bool {
        self.down.is_some() || self.up.is_some()
    }

    /// Whether packets need diverting at all: shaping or blocking.
    pub fn needs_engine(&self) -> bool {
        self.shapes_anything() || !self.blocked.is_empty()
    }

    /// Whether the engine diverts both directions (apps may be blocked).
    pub fn blocking(&self) -> bool {
        !self.blocked.is_empty() || self.strict
    }

    /// Is `app` blocked?
    pub fn blocks(&self, app: &App) -> bool {
        self.blocked.contains(&app.family)
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

    /// Whether the user said `family` never goes first.
    pub fn never_first(&self, family: &str) -> bool {
        self.never_first
            .iter()
            .any(|f| f.eq_ignore_ascii_case(family))
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
        let p = c.plan(0);
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
        let p = Config::parse(SAMPLE).unwrap().plan(0);
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
            c.plan(0).down.unwrap().rate_bytes_per_sec,
            (25e6 / 8.0) as u64
        );
        c.line.factor = Some(0.85);
        let p = c.plan(0);
        assert_eq!(
            p.down.unwrap().rate_bytes_per_sec,
            (25e6 * 0.85 / 8.0) as u64
        );
        assert_eq!(p.up.unwrap().rate_bytes_per_sec, (10e6 * 0.85 / 8.0) as u64);
    }

    #[test]
    fn a_live_rate_replaces_the_typed_one_only_where_shaped() {
        let mut c = Config::parse(SAMPLE).unwrap();
        c.line.up_mbps = None;
        let p = c.plan_live(0, [Some(30e6), Some(5e6)]);
        assert_eq!(p.down.unwrap().rate_bytes_per_sec, (30e6 / 8.0) as u64);
        assert!(p.up.is_none(), "an unshaped direction stays unshaped");
        // Nonsense estimates are ignored.
        let typed = c.plan(0).down.unwrap().rate_bytes_per_sec;
        let p = c.plan_live(0, [Some(f64::NAN), None]);
        assert_eq!(p.down.unwrap().rate_bytes_per_sec, typed);
    }

    #[test]
    fn pinned_round_trips_and_defaults_to_adapting() {
        let mut c = Config::parse(SAMPLE).unwrap();
        assert!(!c.line.is_pinned());
        c.line.pinned = Some(true);
        let back = Config::parse(&c.to_toml()).unwrap();
        assert!(back.line.is_pinned());
    }

    #[test]
    fn rules_over_95_percent_keep_the_floor() {
        let text = with_profile(&(rule("family", "a", 6000) + &rule("family", "b", 4000)));
        let mut c = Config::parse(&text).unwrap();
        c.line.down_mbps = Some(25.0);
        let bp: Vec<_> = c
            .plan(0)
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
        c.plan(0).down.unwrap().validate().unwrap();
    }

    #[test]
    fn matching() {
        let p = Config::parse(&with_profile(
            &(rule("family", "Steam", 100)
                + &rule("exe", "game", 100)
                + &rule("site", "youtube", 100)),
        ))
        .unwrap()
        .plan(0);
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
    fn a_boost_goes_first_with_half_the_line_until_it_ends() {
        let mut c = Config::parse(&with_profile(
            &(rule("site", "youtube", 4500) + &rule("family", "brave", 3000)),
        ))
        .unwrap();
        c.line.down_mbps = Some(25.0);
        c.boost = Some(Boost {
            target: Target::family("steam"),
            until: Some(100),
        });
        c.validate().unwrap();
        assert_eq!(Config::parse(&c.to_toml()).unwrap(), c, "round trip");

        let p = c.plan(99);
        assert_eq!(
            p.targets[0],
            Target::family("steam"),
            "the boost comes first"
        );
        let down = p.down.unwrap();
        assert_eq!(down.classes[0].prio, Priority::High);
        let bp: Vec<_> = down.classes.iter().map(|c| c.share_bp).collect();
        // 4500 + 3000 shrink into the 4500 left above Everything else's 5%.
        assert_eq!(bp, [5000, 2700, 1800, 500]);

        // Over at `until`: the profile as it is.
        let after = c.plan(100);
        assert_eq!(after.targets.len(), 2);
        assert_eq!(c.boost_at(100), None);
    }

    #[test]
    fn boosting_a_rule_lifts_it_and_drops_its_ceiling() {
        let mut c = Config::parse(&with_profile(
            &(rule("family", "steam", 7000)
                + "priority = \"low\"
ceiling = { down = \"10%\" }
"),
        ))
        .unwrap();
        c.boost = Some(Boost {
            target: Target::family("Steam"),
            until: None,
        });
        let r = c.effective_rules(u64::MAX);
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].share, r[0].priority), (7000, Priority::High));
        assert_eq!(r[0].ceiling, None);
        c.boost.as_mut().unwrap().target = Target::site("nope");
        assert!(
            c.check_sites()
                .unwrap_err()
                .to_string()
                .contains("not a known site")
        );
    }

    #[test]
    fn blocked_apps_and_pause() {
        let mut c = Config::parse(&with_profile(&rule("family", "brave", 3000))).unwrap();
        c.blocked = vec!["Steam".into()];
        c.pause = Some(Pause { until: Some(100) });
        c.validate().unwrap();
        assert_eq!(Config::parse(&c.to_toml()).unwrap(), c, "round trip");

        // Paused: nothing shaped, nothing blocked.
        c.line.down_mbps = Some(25.0);
        let p = c.plan(99);
        assert!(!p.needs_engine() && p.targets.is_empty());
        // Over: rules and blocks are back.
        let p = c.plan(100);
        assert!(p.shapes_anything());
        assert!(p.blocks(&app("steam", "steam.exe")));
        assert!(!p.blocks(&app("brave", "brave.exe")));
        // Blocking alone needs the engine, with no line speed.
        c.line.down_mbps = None;
        assert!(c.plan(100).needs_engine() && !c.plan(100).shapes_anything());

        c.blocked = vec!["steam".into(), "STEAM".into()];
        assert!(c.validate().unwrap_err().to_string().contains("twice"));
        c.blocked = vec!["site:youtube".into()];
        assert!(c.validate().unwrap_err().to_string().contains("family id"));
        // Apps known by their path or Store package can be blocked too.
        c.blocked = vec![
            r"c:\games\celeste.exe".into(),
            "pkg:spotify_zpdnekdrzrea0".into(),
        ];
        assert!(c.validate().is_ok());
    }

    #[test]
    fn the_plan_carries_calls_first_and_never_first() {
        let mut c = Config::default();
        assert!(c.plan(0).calls_first, "on by default");
        assert!(!c.to_toml().contains("calls_first = false"));
        c.settings.never_first = vec!["OBS".into()];
        let p = c.plan(0);
        assert!(p.never_first("obs") && !p.never_first("discord"));
        c.settings.calls_first = false;
        assert!(!c.plan(0).calls_first);
        // An older config without the key gets it on.
        let old = c.to_toml().replace(
            "calls_first = false
",
            "",
        );
        assert!(Config::parse(&old).unwrap().settings.calls_first);
    }

    #[test]
    fn games_and_never_first_overrides() {
        let mut c = Config::default();
        c.settings.games = vec!["Hades".into()];
        c.settings.never_first = vec!["obs".into()];
        c.validate().unwrap();
        assert_eq!(Config::parse(&c.to_toml()).unwrap(), c, "round trip");
        assert!(c.settings.is_game("hades") && !c.settings.is_game("obs"));
        assert!(c.settings.never_first("OBS") && !c.settings.never_first("hades"));
        assert!(
            !Config::default().to_toml().contains("never_first"),
            "empty lists stay out of the file"
        );

        c.settings.never_first = vec!["hades".into()];
        assert!(c.validate().unwrap_err().to_string().contains("can't be"));
        c.settings.never_first.clear();
        c.settings.games = vec!["hades".into(), "HADES".into()];
        assert!(c.validate().unwrap_err().to_string().contains("twice"));
        c.settings.games = vec!["site:youtube".into()];
        assert!(c.validate().unwrap_err().to_string().contains("family id"));
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
        let p = Config::parse(&text).unwrap().plan(0);
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
            .plan(0);
        assert!(!no_sites.has_site_rules());
    }

    #[test]
    fn unknown_sites_are_refused_and_custom_ones_accepted() {
        // Loading keeps a rule for a site the packs don't have (it matches
        // nothing); a change naming one is refused.
        let unknown = Config::parse(&with_profile(&rule("site", "myintranet", 1000))).unwrap();
        let e = unknown.check_sites().unwrap_err().to_string();
        assert!(e.contains("site:myintranet is not a known site"), "{e}");
        let known = Config::parse(&with_profile(&rule("site", "youtube", 1000))).unwrap();
        known.check_sites().unwrap();
        let text = with_profile(&rule("site", "myintranet", 1000))
            + "[[sites]]\nid = \"myintranet\"\nname = \"Intranet\"\ndomains = [\"*.corp.example\"]\n";
        let c = Config::parse(&text).unwrap();
        c.check_sites().unwrap();
        assert_eq!(c.plan(0).custom_sites[0].id, "myintranet");
        assert_eq!(Config::parse(&c.to_toml()).unwrap(), c, "round trip");
        let twice = text.clone()
            + "[[sites]]\nid = \"myintranet\"\nname = \"Again\"\ndomains = [\"a.example\"]\n";
        assert!(err(&twice).contains("defined twice"));
        let bad = with_profile("")
            + "[[sites]]\nid = \"x\"\nname = \"X\"\ndomains = [\"not a domain\"]\n";
        assert!(err(&bad).contains("not a domain pattern"));
    }

    /// A config saved before the proof (§11.10) has no `loaded_up_ms` or
    /// `with_ms`: it still loads, and a record without them writes neither.
    #[test]
    fn a_measured_record_without_the_proof_fields_still_loads() {
        let old = with_profile("")
            + "[line]
down_mbps = 24.6
up_mbps = 4.8
               [line.measured]
at = 1700000000
down_mbps = 24.6
up_mbps = 4.8
               idle_ms = 14.0
loaded_ms = 142.0
";
        let c = Config::parse(&old).unwrap();
        let m = c.line.measured.as_ref().unwrap();
        assert_eq!(m.loaded_ms, Some(142.0));
        assert_eq!((m.loaded_up_ms, m.with_ms), (None, None));
        let text = c.to_toml();
        assert!(!text.contains("loaded_up_ms") && !text.contains("with_ms"));

        let mut c = c;
        let m = c.line.measured.as_mut().unwrap();
        m.loaded_up_ms = Some(310.0);
        m.with_ms = Some(21.0);
        let back = Config::parse(&c.to_toml()).unwrap();
        assert_eq!(back, c);
    }
}
