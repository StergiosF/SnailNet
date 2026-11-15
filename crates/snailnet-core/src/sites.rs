//! Website recognition data (PLAN.md §8): site packs, the domain → site
//! matcher, and the IP → domain cache built from DNS answers and SNI.

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::clock::{NANOS_PER_SEC, Nanos};
use crate::parse::tls::host_name;

#[derive(Debug, thiserror::Error)]
pub enum SiteError {
    #[error("site pack is not valid TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("site pack: {0}")]
    Invalid(String),
}

/// One website: an id for rules (`site:youtube`), a display name, and the
/// domain patterns that belong to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteDef {
    pub id: String,
    pub name: String,
    pub domains: Vec<String>,
}

/// The shipped site list: `assets/sites.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SitePack {
    #[serde(rename = "site")]
    pub sites: Vec<SiteDef>,
}

const BUILTIN_PACK: &str = include_str!("../../../assets/sites.toml");

impl SitePack {
    pub fn builtin() -> Self {
        Self::parse(BUILTIN_PACK).expect("assets/sites.toml is checked by the tests")
    }

    pub fn parse(text: &str) -> Result<Self, SiteError> {
        let pack: SitePack = toml::from_str(text)?;
        let mut seen = Vec::new();
        for site in &pack.sites {
            site.validate()?;
            if seen.contains(&&site.id) {
                return Err(SiteError::Invalid(format!(
                    "site id {:?} repeated",
                    site.id
                )));
            }
            seen.push(&site.id);
        }
        // One pattern may belong to one site only.
        let mut owner: HashMap<String, &str> = HashMap::new();
        for site in &pack.sites {
            for d in &site.domains {
                let p = Pattern::parse(d).expect("validated above");
                if let Some(other) = owner.insert(p.key(), &site.id)
                    && other != site.id
                {
                    return Err(SiteError::Invalid(format!(
                        "{d:?} is in both {other} and {}",
                        site.id
                    )));
                }
            }
        }
        Ok(pack)
    }
}

impl SiteDef {
    /// Ids are lowercase letters, digits and '-'; every pattern is a valid
    /// host name, optionally with a leading `*.`.
    pub fn validate(&self) -> Result<(), SiteError> {
        let bad = |c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if self.id.is_empty() || self.id.contains(bad) {
            return Err(SiteError::Invalid(format!(
                "site id {:?} must be lowercase letters, digits and '-'",
                self.id
            )));
        }
        if self.name.trim().is_empty() {
            return Err(SiteError::Invalid(format!("site {} has no name", self.id)));
        }
        if self.domains.is_empty() {
            return Err(SiteError::Invalid(format!(
                "site {} has no domains",
                self.id
            )));
        }
        for d in &self.domains {
            Pattern::parse(d).ok_or_else(|| {
                SiteError::Invalid(format!("site {}: {d:?} is not a domain pattern", self.id))
            })?;
        }
        Ok(())
    }
}

/// A parsed domain pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Pattern {
    Exact(String),
    /// `*.suffix`: names strictly under `suffix`.
    Under(String),
}

impl Pattern {
    fn parse(p: &str) -> Option<Pattern> {
        match p.strip_prefix("*.") {
            Some(rest) => Some(Pattern::Under(host_name(rest.as_bytes())?)),
            None => Some(Pattern::Exact(host_name(p.as_bytes())?)),
        }
    }
    fn key(&self) -> String {
        match self {
            Pattern::Exact(d) => d.clone(),
            Pattern::Under(d) => format!("*.{d}"),
        }
    }
}

/// Domain name → site id. The longest matching pattern wins; an exact name
/// beats a wildcard over it.
#[derive(Debug, Clone, Default)]
pub struct SiteMatcher {
    sites: Vec<(String, String)>,
    exact: HashMap<String, usize>,
    under: HashMap<String, usize>,
}

impl SiteMatcher {
    /// The shipped packs plus the user's custom sites. A custom site with a
    /// shipped site's id replaces it; a custom pattern overrides a shipped
    /// one.
    pub fn new(pack: &SitePack, custom: &[SiteDef]) -> Result<Self, SiteError> {
        let mut m = SiteMatcher::default();
        let custom_ids: Vec<&str> = custom.iter().map(|s| s.id.as_str()).collect();
        for site in &pack.sites {
            if !custom_ids.contains(&site.id.as_str()) {
                m.add(site);
            }
        }
        for site in custom {
            site.validate()?;
            m.add(site);
        }
        Ok(m)
    }

    fn add(&mut self, site: &SiteDef) {
        let idx = match self.sites.iter().position(|(id, _)| *id == site.id) {
            Some(i) => i,
            None => {
                self.sites.push((site.id.clone(), site.name.clone()));
                self.sites.len() - 1
            }
        };
        for d in &site.domains {
            match Pattern::parse(d) {
                Some(Pattern::Exact(d)) => self.exact.insert(d, idx),
                Some(Pattern::Under(d)) => self.under.insert(d, idx),
                None => None,
            };
        }
    }

    /// The site a (lowercased) domain name belongs to.
    pub fn site_of(&self, domain: &str) -> Option<&str> {
        let idx = self.exact.get(domain).copied().or_else(|| {
            let mut rest = domain;
            loop {
                rest = rest.split_once('.')?.1;
                if let Some(&i) = self.under.get(rest) {
                    return Some(i);
                }
            }
        })?;
        Some(&self.sites[idx].0)
    }

    /// The site of the first name that has one (e.g. a DNS CNAME chain,
    /// question name first).
    pub fn site_of_any<'a>(&self, names: impl IntoIterator<Item = &'a str>) -> Option<&str> {
        names.into_iter().find_map(|n| self.site_of(n))
    }

    /// Display name for a site id.
    pub fn name(&self, id: &str) -> Option<&str> {
        self.sites
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, n)| n.as_str())
    }

    pub fn ids(&self) -> impl Iterator<Item = (&str, &str)> {
        self.sites.iter().map(|(i, n)| (i.as_str(), n.as_str()))
    }
}

/// Names an address was last seen for. Shared, so one DNS answer with many
/// addresses stores the chain once.
pub type Names = Arc<[String]>;

/// IP → the names it was last resolved from (PLAN.md §8.1), TTL-bounded
/// and capped. The newest answer for an address wins: CDN addresses are
/// shared, so this is a hint, used only when a flow has no usable SNI.
#[derive(Debug)]
pub struct DomainCache {
    map: HashMap<IpAddr, Entry>,
    /// Insertion order for eviction: (address, generation when inserted).
    order: VecDeque<(IpAddr, u64)>,
    cap: usize,
    next_gen: u64,
}

#[derive(Debug)]
struct Entry {
    names: Names,
    expires: Nanos,
    generation: u64,
}

impl DomainCache {
    /// TTLs are raised to this: apps resolve once and reconnect later from
    /// their own cache, often past a short TTL.
    pub const MIN_TTL_SECS: u32 = 300;
    pub const MAX_TTL_SECS: u32 = 3600;

    pub fn new(cap: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            cap: cap.max(1),
            next_gen: 0,
        }
    }

    pub fn insert(&mut self, ip: IpAddr, names: Names, ttl_secs: u32, now: Nanos) {
        let ttl = ttl_secs.clamp(Self::MIN_TTL_SECS, Self::MAX_TTL_SECS);
        let generation = self.next_gen;
        self.next_gen += 1;
        self.map.insert(
            ip,
            Entry {
                names,
                expires: now.saturating_add(Nanos::from(ttl) * NANOS_PER_SEC),
                generation,
            },
        );
        self.order.push_back((ip, generation));
        while self.map.len() > self.cap {
            self.evict_oldest();
        }
        // Re-inserting the same addresses leaves stale order entries; keep
        // the queue bounded.
        if self.order.len() > 2 * self.cap {
            self.order
                .retain(|(ip, g)| self.map.get(ip).is_some_and(|e| e.generation == *g));
        }
    }

    fn evict_oldest(&mut self) {
        while let Some((ip, g)) = self.order.pop_front() {
            if self.map.get(&ip).is_some_and(|e| e.generation == g) {
                self.map.remove(&ip);
                return;
            }
        }
    }

    pub fn get(&self, ip: &IpAddr, now: Nanos) -> Option<&Names> {
        self.map
            .get(ip)
            .filter(|e| e.expires > now)
            .map(|e| &e.names)
    }

    /// Drop expired entries (call now and then, e.g. once a minute).
    pub fn purge(&mut self, now: Nanos) {
        self.map.retain(|_, e| e.expires > now);
        self.order
            .retain(|(ip, g)| self.map.get(ip).is_some_and(|e| e.generation == *g));
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn matcher() -> SiteMatcher {
        SiteMatcher::new(&SitePack::builtin(), &[]).unwrap()
    }

    #[test]
    fn the_builtin_pack_parses_and_has_the_v1_sites() {
        let m = matcher();
        let ids: Vec<_> = m.ids().map(|(i, _)| i).collect();
        for want in [
            "youtube",
            "twitch",
            "netflix",
            "disney-plus",
            "prime-video",
            "spotify",
            "kick",
            "zoom",
            "google-meet",
            "teams",
        ] {
            assert!(ids.contains(&want), "{want}");
        }
        assert_eq!(m.name("disney-plus"), Some("Disney+"));
    }

    #[test]
    fn matching() {
        let m = matcher();
        assert_eq!(m.site_of("youtube.com"), Some("youtube"));
        assert_eq!(m.site_of("www.youtube.com"), Some("youtube"));
        assert_eq!(
            m.site_of("rr3---sn-4g5e6nsz.googlevideo.com"),
            Some("youtube")
        );
        assert_eq!(m.site_of("googlevideo.com"), None, "*. needs a label");
        assert_eq!(m.site_of("notyoutube.com"), None);
        assert_eq!(m.site_of("video-edge-1.ttvnw.net"), Some("twitch"));
        assert_eq!(m.site_of("google.com"), None);
        assert_eq!(m.site_of(""), None);
        assert_eq!(
            m.site_of_any(["youtube-ui.l.google.com", "www.youtube.com"]),
            Some("youtube")
        );
    }

    #[test]
    fn longest_pattern_wins_and_custom_sites_override() {
        let pack = SitePack::parse(
            r#"
            [[site]]
            id = "google"
            name = "Google"
            domains = ["*.google.com"]
            [[site]]
            id = "meet"
            name = "Meet"
            domains = ["*.meet.google.com", "meet.google.com"]
            "#,
        )
        .unwrap();
        let custom = [SiteDef {
            id: "work".into(),
            name: "Work".into(),
            domains: vec!["*.corp.google.com".into(), "*.meet.google.com".into()],
        }];
        let m = SiteMatcher::new(&pack, &custom).unwrap();
        assert_eq!(m.site_of("mail.google.com"), Some("google"));
        assert_eq!(m.site_of("meet.google.com"), Some("meet"));
        assert_eq!(m.site_of("a.meet.google.com"), Some("work"), "custom wins");
        assert_eq!(m.site_of("x.y.corp.google.com"), Some("work"));

        // A custom site with a shipped id replaces it entirely.
        let mine = [SiteDef {
            id: "google".into(),
            name: "Mine".into(),
            domains: vec!["mail.google.com".into()],
        }];
        let m = SiteMatcher::new(&pack, &mine).unwrap();
        assert_eq!(m.site_of("drive.google.com"), None);
        assert_eq!(m.site_of("mail.google.com"), Some("google"));
        assert_eq!(m.name("google"), Some("Mine"));
    }

    #[test]
    fn bad_packs_are_refused() {
        for bad in [
            r#"[[site]]
               id = "Bad"
               name = "x"
               domains = ["a.com"]"#,
            r#"[[site]]
               id = "a"
               name = "x"
               domains = ["*.*.a.com"]"#,
            r#"[[site]]
               id = "a"
               name = "x"
               domains = []"#,
            r#"[[site]]
               id = "a"
               name = "x"
               domains = ["a.com"]
               [[site]]
               id = "b"
               name = "y"
               domains = ["A.com"]"#,
            r#"[[site]]
               id = "a"
               name = "x"
               domains = ["a.com"]
               extra = 1"#,
        ] {
            assert!(SitePack::parse(bad).is_err(), "{bad}");
        }
    }

    fn names(n: &str) -> Names {
        Arc::from(vec![n.to_string()])
    }

    #[test]
    fn cache_ttl_is_clamped_and_entries_expire() {
        let mut c = DomainCache::new(10);
        let ip: IpAddr = "142.250.1.1".parse().unwrap();
        c.insert(ip, names("www.youtube.com"), 5, 0);
        let s = NANOS_PER_SEC;
        assert!(c.get(&ip, 299 * s).is_some(), "raised to 5 min");
        assert!(c.get(&ip, 300 * s).is_none());
        c.insert(ip, names("a"), 86_400, 0);
        assert!(c.get(&ip, 3599 * s).is_some());
        assert!(c.get(&ip, 3600 * s).is_none(), "capped at 1 h");
        c.purge(3600 * s);
        assert!(c.is_empty());
    }

    #[test]
    fn newest_answer_wins_and_the_oldest_is_evicted() {
        let mut c = DomainCache::new(2);
        let a: IpAddr = "10.0.0.1".parse().unwrap();
        let b: IpAddr = "10.0.0.2".parse().unwrap();
        let d: IpAddr = "10.0.0.3".parse().unwrap();
        c.insert(a, names("one"), 60, 0);
        c.insert(b, names("two"), 60, 0);
        c.insert(a, names("three"), 60, 0); // a is now the newest
        c.insert(d, names("four"), 60, 0); // evicts b
        assert_eq!(c.get(&a, 0).map(|n| n[0].as_str()), Some("three"));
        assert!(c.get(&b, 0).is_none());
        assert!(c.get(&d, 0).is_some());
    }

    proptest! {
        #[test]
        fn cache_stays_bounded(ops in proptest::collection::vec(0u8..20, 0..500)) {
            let mut c = DomainCache::new(8);
            for (t, o) in ops.into_iter().enumerate() {
                let ip = IpAddr::from([10, 0, 0, o]);
                c.insert(ip, names("x"), 60, t as Nanos);
                prop_assert!(c.len() <= 8);
                prop_assert!(c.order.len() <= 16 + 1);
                prop_assert!(c.get(&ip, t as Nanos).is_some(), "just inserted");
            }
        }
    }
}
