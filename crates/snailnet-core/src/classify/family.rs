//! Process → app family (PLAN.md §7).
//!
//! Users think "Steam", not `steam.exe` + `steamwebhelper.exe` +
//! `steamservice.exe`. [`Resolver::resolve`] turns a PID into an [`App`]:
//! a family (what the user sees and writes rules for) plus the process's own
//! exe name (so a rule can still name one exe). Resolution order:
//!
//! 1. the family pack (`assets/families.toml`),
//! 2. `svchost.exe -s <Service>` → "Windows Update" or "Windows services",
//! 3. generic hosts (`node.exe`, `msedgewebview2.exe`, …) → the nearest
//!    ancestor that isn't one,
//! 4. the Store package family,
//! 5. same install folder + same Authenticode publisher as an app already
//!    seen there,
//! 6. the exe on its own.
//!
//! Results are cached by `(pid, creation time)`, so a reused PID is never
//! given the old process's app. The OS facts come from a [`ProcessSource`]
//! (`snailnet-win` on Windows, a table in tests).

use std::collections::HashMap;

use serde::Deserialize;

/// What the OS can tell us about one process. Paths are as reported; the
/// resolver normalises them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessFacts {
    pub pid: u32,
    /// Creation time in any monotonic unit (FILETIME on Windows). Together
    /// with the PID it identifies a process across PID reuse.
    pub created: u64,
    /// Full image path. `None` for protected processes we can't open.
    pub path: Option<String>,
    pub parent: Option<u32>,
    /// Only needed for `svchost.exe`; sources may skip it for other exes.
    pub command_line: Option<String>,
    /// Signer's display name from the exe's embedded Authenticode signature.
    pub publisher: Option<String>,
    /// Store package family name (UWP / MSIX apps).
    pub package: Option<String>,
    /// `FileDescription` from the exe's version info.
    pub description: Option<String>,
}

/// Where process facts come from.
pub trait ProcessSource {
    /// Creation time only (cheap), or `None` if the process has exited or
    /// can't be opened.
    fn created(&mut self, pid: u32) -> Option<u64>;
    /// Everything else (slow: reads the exe's signature and version info).
    fn facts(&mut self, pid: u32) -> Option<ProcessFacts>;
}

/// One process, safe against PID reuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessKey {
    pub pid: u32,
    pub created: u64,
}

/// How a process's family was decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    Pack,
    Service,
    Parent,
    Package,
    Folder,
    Own,
    /// PID 0 or 4: the kernel (file sharing, some system traffic).
    System,
    /// The process exited before we looked, or we can't open it.
    Unknown,
}

/// The app a process belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    /// Stable family id: a pack id (`steam`), a service group
    /// (`windows-update`), `pkg:<family name>`, `folder:<dir>|<publisher>`, or the
    /// exe's full lowercase path.
    pub family: String,
    /// Display name of the family.
    pub name: String,
    /// This process's lowercase exe file name (`steamwebhelper.exe`).
    /// Empty for System and Unknown.
    pub exe: String,
    pub how: How,
}

pub const SYSTEM_ID: &str = "system";
pub const UNKNOWN_ID: &str = "unknown";
pub const WINDOWS_SERVICES_ID: &str = "windows-services";

impl App {
    pub fn system() -> Self {
        Self::fixed(SYSTEM_ID, "System", How::System)
    }

    pub fn unknown() -> Self {
        Self::fixed(UNKNOWN_ID, "Unknown", How::Unknown)
    }

    fn fixed(id: &str, name: &str, how: How) -> Self {
        Self {
            family: id.into(),
            name: name.into(),
            exe: String::new(),
            how,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("family pack is not valid TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("family pack: {0}")]
    Invalid(String),
}

/// The family pack: `assets/families.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    #[serde(default)]
    generic_hosts: Vec<String>,
    #[serde(default)]
    stop_at: Vec<String>,
    #[serde(default, rename = "service")]
    services: Vec<ServiceGroup>,
    #[serde(default, rename = "family")]
    families: Vec<FamilyDef>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServiceGroup {
    id: String,
    name: String,
    services: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct FamilyDef {
    id: String,
    name: String,
    #[serde(default)]
    exes: Vec<String>,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    not_paths: Vec<String>,
}

const BUILTIN_PACK: &str = include_str!("../../../../assets/families.toml");

impl Pack {
    /// The pack shipped with SnailNet.
    pub fn builtin() -> Self {
        Self::parse(BUILTIN_PACK).expect("assets/families.toml is checked by the tests")
    }

    pub fn parse(text: &str) -> Result<Self, PackError> {
        let mut pack: Pack = toml::from_str(text)?;
        pack.normalise();
        pack.validate()?;
        Ok(pack)
    }

    fn normalise(&mut self) {
        let lower = |v: &mut Vec<String>| v.iter_mut().for_each(|s| *s = normalise_path(s));
        lower(&mut self.generic_hosts);
        lower(&mut self.stop_at);
        for g in &mut self.services {
            lower(&mut g.services);
        }
        for f in &mut self.families {
            lower(&mut f.exes);
            lower(&mut f.paths);
            lower(&mut f.not_paths);
        }
    }

    fn validate(&self) -> Result<(), PackError> {
        let reserved = [SYSTEM_ID, UNKNOWN_ID, WINDOWS_SERVICES_ID];
        let mut seen: Vec<&str> = Vec::new();
        let ids = self
            .families
            .iter()
            .map(|f| (&f.id, &f.name))
            .chain(self.services.iter().map(|g| (&g.id, &g.name)));
        for (id, name) in ids {
            let bad_char = |c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if id.is_empty() || id.contains(bad_char) {
                return Err(PackError::Invalid(format!(
                    "id {id:?} must be lowercase letters, digits and '-'"
                )));
            }
            if reserved.contains(&id.as_str()) || seen.contains(&id.as_str()) {
                return Err(PackError::Invalid(format!(
                    "id {id:?} is reserved or repeated"
                )));
            }
            if name.trim().is_empty() {
                return Err(PackError::Invalid(format!("{id} has no name")));
            }
            seen.push(id);
        }
        for f in &self.families {
            if f.exes.is_empty() && f.paths.is_empty() {
                return Err(PackError::Invalid(format!("{} matches nothing", f.id)));
            }
        }
        Ok(())
    }

    fn family_for(&self, exe: &str, path: &str) -> Option<&FamilyDef> {
        self.families.iter().find(|f| {
            (f.exes.iter().any(|e| e == exe) || f.paths.iter().any(|p| glob(p, path)))
                && !f.not_paths.iter().any(|p| glob(p, path))
        })
    }

    /// True if some family's `not_paths` covers `path`: such folders are
    /// never grouped by folder either (games under `steamapps\common`).
    fn vetoed(&self, path: &str) -> bool {
        self.families
            .iter()
            .any(|f| f.not_paths.iter().any(|p| glob(p, path)))
    }

    fn service_group(&self, service: &str) -> Option<&ServiceGroup> {
        self.services
            .iter()
            .find(|g| g.services.iter().any(|s| s == service))
    }

    /// Every family and service-group id the pack defines, with its name.
    pub fn ids(&self) -> impl Iterator<Item = (&str, &str)> {
        self.families
            .iter()
            .map(|f| (f.id.as_str(), f.name.as_str()))
            .chain(
                self.services
                    .iter()
                    .map(|g| (g.id.as_str(), g.name.as_str())),
            )
    }
}

/// Resolution result: the app, and the process key when the process could
/// be identified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub process: Option<ProcessKey>,
    pub app: App,
}

/// Bounded so a long uptime with many short-lived processes can't grow it
/// without limit; when full it simply starts over.
const MAX_CACHE: usize = 8192;
/// Generic hosts nested deeper than this are their own app.
const MAX_DEPTH: usize = 8;

pub struct Resolver {
    pack: Pack,
    /// Lowercase, no trailing `\`, e.g. `c:\windows`.
    windows_dir: String,
    /// (install folder, publisher) → (family id, name), learned as apps are
    /// resolved.
    folders: HashMap<(String, String), (String, String)>,
    cache: HashMap<ProcessKey, App>,
}

impl Resolver {
    pub fn new(pack: Pack, windows_dir: &str) -> Self {
        Self {
            pack,
            windows_dir: normalise_path(windows_dir).trim_end_matches('\\').into(),
            folders: HashMap::new(),
            cache: HashMap::new(),
        }
    }

    pub fn pack(&self) -> &Pack {
        &self.pack
    }

    pub fn resolve(&mut self, pid: u32, src: &mut dyn ProcessSource) -> Resolved {
        self.resolve_at(pid, src, 0)
    }

    fn resolve_at(&mut self, pid: u32, src: &mut dyn ProcessSource, depth: usize) -> Resolved {
        if pid == 0 || pid == 4 {
            return Resolved {
                process: None,
                app: App::system(),
            };
        }
        let unknown = Resolved {
            process: None,
            app: App::unknown(),
        };
        let Some(created) = src.created(pid) else {
            return unknown;
        };
        let key = ProcessKey { pid, created };
        if let Some(app) = self.cache.get(&key) {
            return Resolved {
                process: Some(key),
                app: app.clone(),
            };
        }
        let Some(facts) = src.facts(pid) else {
            return unknown;
        };
        let app = self.classify(&facts, created, src, depth);
        if self.cache.len() >= MAX_CACHE {
            self.cache.clear();
        }
        self.cache.insert(key, app.clone());
        Resolved {
            process: Some(key),
            app,
        }
    }

    fn classify(
        &mut self,
        facts: &ProcessFacts,
        created: u64,
        src: &mut dyn ProcessSource,
        depth: usize,
    ) -> App {
        let Some(path) = facts.path.as_deref().map(normalise_path) else {
            return App::unknown();
        };
        let exe = file_name(&path).to_owned();
        let dir = parent_dir(&path).to_owned();
        let publisher = facts.publisher.as_deref().map(str::trim).unwrap_or("");

        if let Some(f) = self.pack.family_for(&exe, &path) {
            let (id, name) = (f.id.clone(), f.name.clone());
            self.learn_folder(&dir, &path, publisher, &id, &name);
            return App {
                family: id,
                name,
                exe,
                how: How::Pack,
            };
        }
        if exe == "svchost.exe" && self.in_windows(&path) {
            return self.service(facts.command_line.as_deref(), exe);
        }
        if self.pack.generic_hosts.contains(&exe)
            && depth < MAX_DEPTH
            && let Some(parent) = self.parent_app(facts, created, src, depth)
        {
            return App {
                exe,
                how: How::Parent,
                ..parent
            };
        }
        let display = facts
            .description
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty());
        if let Some(pkg) = facts.package.as_deref().filter(|p| !p.is_empty()) {
            // `Microsoft.ZuneVideo_8wekyb3d8bbwe` → "Microsoft.ZuneVideo".
            let short = pkg.split('_').next().unwrap_or(pkg);
            return App {
                family: format!("pkg:{}", pkg.to_lowercase()),
                name: display.unwrap_or(short).into(),
                exe,
                how: How::Package,
            };
        }
        let groupable = !publisher.is_empty() && self.groupable(&dir, &path);
        if groupable {
            if let Some((id, name)) = self.folders.get(&(dir.clone(), publisher.to_owned())) {
                return App {
                    family: id.clone(),
                    name: name.clone(),
                    exe,
                    how: How::Folder,
                };
            }
            let id = format!("folder:{dir}|{}", publisher.to_lowercase());
            let name = display.map_or_else(|| display_exe(facts), str::to_owned);
            self.folders
                .insert((dir, publisher.to_owned()), (id.clone(), name.clone()));
            return App {
                family: id,
                name,
                exe,
                how: How::Folder,
            };
        }
        App {
            family: path,
            name: display.map_or_else(|| display_exe(facts), str::to_owned),
            exe,
            how: How::Own,
        }
    }

    /// Remember a pack family's folder so unlisted helpers next to it (same
    /// publisher) join it.
    fn learn_folder(&mut self, dir: &str, path: &str, publisher: &str, id: &str, name: &str) {
        if !publisher.is_empty() && self.groupable(dir, path) {
            self.folders
                .entry((dir.to_owned(), publisher.to_owned()))
                .or_insert_with(|| (id.to_owned(), name.to_owned()));
        }
    }

    /// Folder grouping is off inside Windows (System32 holds hundreds of
    /// unrelated Microsoft exes) and in vetoed folders.
    fn groupable(&self, dir: &str, path: &str) -> bool {
        !dir.is_empty() && !self.in_windows(path) && !self.pack.vetoed(path)
    }

    fn in_windows(&self, path: &str) -> bool {
        path.strip_prefix(self.windows_dir.as_str())
            .is_some_and(|rest| rest.starts_with('\\'))
    }

    fn service(&self, command_line: Option<&str>, exe: String) -> App {
        let group = command_line
            .and_then(service_name)
            .and_then(|s| self.pack.service_group(&s));
        let (family, name) = match group {
            Some(g) => (g.id.clone(), g.name.clone()),
            None => (WINDOWS_SERVICES_ID.into(), "Windows services".into()),
        };
        App {
            family,
            name,
            exe,
            how: How::Service,
        }
    }

    /// The nearest ancestor's app, if the parent is still the process that
    /// started us (created no later than we were) and isn't a stop point.
    fn parent_app(
        &mut self,
        facts: &ProcessFacts,
        created: u64,
        src: &mut dyn ProcessSource,
        depth: usize,
    ) -> Option<App> {
        let ppid = facts.parent.filter(|&p| p != facts.pid)?;
        if ppid == 0 || ppid == 4 || src.created(ppid)? > created {
            return None;
        }
        let parent = self.resolve_at(ppid, src, depth + 1).app;
        let stop = matches!(parent.how, How::System | How::Unknown)
            || self.pack.stop_at.contains(&parent.exe);
        (!stop).then_some(parent)
    }
}

/// The service name after `-s` in an svchost command line, lowercased.
fn service_name(command_line: &str) -> Option<String> {
    let mut words = command_line.split_whitespace();
    while let Some(w) = words.next() {
        if w.eq_ignore_ascii_case("-s") || w.eq_ignore_ascii_case("/s") {
            return words.next().map(|s| s.trim_matches('"').to_lowercase());
        }
    }
    None
}

/// Lowercase, `\` separators, no `\\?\` prefix.
pub fn normalise_path(p: &str) -> String {
    let p = p.strip_prefix(r"\\?\").unwrap_or(p);
    p.replace('/', "\\").to_lowercase()
}

fn file_name(path: &str) -> &str {
    path.rsplit('\\').next().unwrap_or(path)
}

fn parent_dir(path: &str) -> &str {
    path.rsplit_once('\\').map_or("", |(d, _)| d)
}

/// The exe file name as the OS spells it (`Spotify.exe`), for display.
fn display_exe(facts: &ProcessFacts) -> String {
    facts
        .path
        .as_deref()
        .map(|p| p.rsplit(['\\', '/']).next().unwrap_or(p).to_owned())
        .unwrap_or_default()
}

/// `*` matches any run of characters (including `\`); everything else
/// matches itself. Both sides are already lowercase.
fn glob(pattern: &str, text: &str) -> bool {
    let (p, t) = (pattern.as_bytes(), text.as_bytes());
    let (mut pi, mut ti) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && p[pi] == b'*' {
            star = Some((pi, ti));
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == b'*')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A process table: pid → facts. `created` doubles as start order.
    #[derive(Default)]
    struct Table {
        procs: HashMap<u32, ProcessFacts>,
        facts_calls: usize,
    }

    impl Table {
        fn add(&mut self, pid: u32, created: u64, path: &str, parent: u32) -> &mut ProcessFacts {
            self.procs.insert(
                pid,
                ProcessFacts {
                    pid,
                    created,
                    path: Some(path.into()),
                    parent: Some(parent),
                    ..Default::default()
                },
            );
            self.procs.get_mut(&pid).unwrap()
        }
    }

    impl ProcessSource for Table {
        fn created(&mut self, pid: u32) -> Option<u64> {
            self.procs.get(&pid).map(|f| f.created)
        }
        fn facts(&mut self, pid: u32) -> Option<ProcessFacts> {
            self.facts_calls += 1;
            self.procs.get(&pid).cloned()
        }
    }

    fn resolver() -> Resolver {
        Resolver::new(Pack::builtin(), r"C:\Windows")
    }

    fn app(r: &mut Resolver, t: &mut Table, pid: u32) -> App {
        r.resolve(pid, t).app
    }

    const STEAM: &str = r"C:\Program Files (x86)\Steam";

    #[test]
    fn builtin_pack_parses_and_has_the_plan_families() {
        let pack = Pack::builtin();
        let ids: Vec<&str> = pack.ids().map(|(id, _)| id).collect();
        for want in [
            "steam",
            "chrome",
            "edge",
            "brave",
            "discord",
            "docker",
            "opencode",
            "windows-update",
        ] {
            assert!(ids.contains(&want), "{want} missing from {ids:?}");
        }
    }

    #[test]
    fn steam_helpers_are_steam_but_its_games_are_not() {
        let (mut r, mut t) = (resolver(), Table::default());
        t.add(10, 1, &format!(r"{STEAM}\steam.exe"), 1).publisher = Some("Valve".into());
        t.add(
            11,
            2,
            &format!(r"{STEAM}\bin\cef\cef.win7x64\steamwebhelper.exe"),
            10,
        );
        t.add(
            12,
            3,
            &format!(r"{STEAM}\steamapps\common\Dota 2\game\bin\win64\dota2.exe"),
            10,
        )
        .publisher = Some("Valve".into());
        let a = app(&mut r, &mut t, 10);
        assert_eq!(
            (a.family.as_str(), a.name.as_str(), a.how),
            ("steam", "Steam", How::Pack)
        );
        let b = app(&mut r, &mut t, 11);
        assert_eq!(
            (b.family.as_str(), b.exe.as_str()),
            ("steam", "steamwebhelper.exe")
        );
        let game = app(&mut r, &mut t, 12);
        assert_eq!(game.how, How::Own, "{game:?}");
        assert!(game.family.ends_with("dota2.exe"));
    }

    #[test]
    fn discord_updater_joins_discord_only_in_its_folder() {
        let (mut r, mut t) = (resolver(), Table::default());
        t.add(20, 1, r"C:\Users\me\AppData\Local\Discord\Update.exe", 1);
        t.add(21, 2, r"C:\Users\me\AppData\Local\Other\Update.exe", 1);
        assert_eq!(app(&mut r, &mut t, 20).family, "discord");
        assert_eq!(app(&mut r, &mut t, 21).how, How::Own);
    }

    #[test]
    fn svchost_services() {
        let (mut r, mut t) = (resolver(), Table::default());
        let svchost = r"C:\WINDOWS\system32\svchost.exe";
        t.add(30, 1, svchost, 700).command_line =
            Some(r"C:\WINDOWS\system32\svchost.exe -k netsvcs -p -s wuauserv".into());
        t.add(31, 1, svchost, 700).command_line =
            Some(r"C:\WINDOWS\System32\svchost.exe -k NetworkService -p -s DoSvc".into());
        t.add(32, 1, svchost, 700).command_line =
            Some(r"C:\WINDOWS\system32\svchost.exe -k LocalService -p -s nsi".into());
        t.add(33, 1, svchost, 700);
        // A fake svchost outside Windows is just an exe.
        t.add(34, 1, r"C:\Temp\svchost.exe", 1).command_line =
            Some("svchost.exe -s wuauserv".into());
        for (pid, family) in [
            (30, "windows-update"),
            (31, "windows-update"),
            (32, "windows-services"),
            (33, "windows-services"),
        ] {
            let a = app(&mut r, &mut t, pid);
            assert_eq!(
                (a.family.as_str(), a.how),
                (family, How::Service),
                "pid {pid}"
            );
        }
        assert_eq!(app(&mut r, &mut t, 31).name, "Windows Update");
        assert_eq!(app(&mut r, &mut t, 34).how, How::Own);
    }

    #[test]
    fn generic_hosts_take_the_nearest_real_ancestor() {
        let (mut r, mut t) = (resolver(), Table::default());
        t.add(1, 0, r"C:\Windows\explorer.exe", 0);
        t.add(40, 1, r"C:\Program Files\Microsoft VS Code\Code.exe", 1)
            .description = Some("Visual Studio Code".into());
        t.add(41, 2, r"C:\Windows\System32\cmd.exe", 40);
        t.add(42, 3, r"C:\Program Files\nodejs\node.exe", 41);
        t.add(43, 4, r"C:\Program Files\nodejs\node.exe", 1);
        let n = app(&mut r, &mut t, 42);
        assert_eq!(n.name, "Visual Studio Code");
        assert_eq!((n.exe.as_str(), n.how), ("node.exe", How::Parent));
        assert_eq!(n.family, app(&mut r, &mut t, 40).family);
        // Started from Explorer: its own app, not "Explorer".
        assert_eq!(app(&mut r, &mut t, 43).how, How::Own);
    }

    #[test]
    fn webview2_belongs_to_its_host_app() {
        let (mut r, mut t) = (resolver(), Table::default());
        let wv =
            r"C:\Program Files (x86)\Microsoft\EdgeWebView\Application\131.0\msedgewebview2.exe";
        t.add(
            50,
            1,
            r"C:\Users\me\AppData\Local\Microsoft\Teams\ms-teams.exe",
            1,
        );
        t.add(51, 2, wv, 50);
        t.add(52, 3, wv, 51); // renderer: child of the browser process
        assert_eq!(app(&mut r, &mut t, 52).family, "teams");
    }

    #[test]
    fn a_reused_parent_pid_is_not_our_parent() {
        let (mut r, mut t) = (resolver(), Table::default());
        // pid 60 was reused by Spotify *after* node started.
        t.add(60, 9, r"C:\Users\me\AppData\Roaming\Spotify\Spotify.exe", 1);
        t.add(61, 5, r"C:\tools\node.exe", 60);
        assert_eq!(app(&mut r, &mut t, 61).how, How::Own);
    }

    #[test]
    fn pid_reuse_never_returns_the_old_app() {
        let (mut r, mut t) = (resolver(), Table::default());
        t.add(
            70,
            1,
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            1,
        );
        assert_eq!(app(&mut r, &mut t, 70).family, "chrome");
        let calls = t.facts_calls;
        assert_eq!(app(&mut r, &mut t, 70).family, "chrome");
        assert_eq!(t.facts_calls, calls, "second lookup is cached");
        t.add(
            70,
            2,
            r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
            1,
        );
        assert_eq!(app(&mut r, &mut t, 70).family, "brave");
    }

    #[test]
    fn same_folder_and_publisher_form_one_family() {
        let (mut r, mut t) = (resolver(), Table::default());
        let dir = r"C:\Program Files\Acme Sync";
        t.add(80, 1, &format!(r"{dir}\AcmeSync.exe"), 1).publisher = Some("Acme Ltd".into());
        t.procs.get_mut(&80).unwrap().description = Some("Acme Sync".into());
        t.add(81, 2, &format!(r"{dir}\acme-uploader.exe"), 80)
            .publisher = Some("Acme Ltd".into());
        t.add(82, 3, &format!(r"{dir}\unsigned.exe"), 80);
        t.add(83, 4, &format!(r"{dir}\other.exe"), 80).publisher = Some("Someone Else".into());
        let a = app(&mut r, &mut t, 80);
        let b = app(&mut r, &mut t, 81);
        assert_eq!((a.how, a.name.as_str()), (How::Folder, "Acme Sync"));
        assert_eq!(
            (b.family.as_str(), b.exe.as_str()),
            (a.family.as_str(), "acme-uploader.exe")
        );
        assert_eq!(app(&mut r, &mut t, 82).how, How::Own);
        assert_ne!(app(&mut r, &mut t, 83).family, a.family);
    }

    #[test]
    fn pack_apps_adopt_signed_neighbours_but_windows_never_groups() {
        let (mut r, mut t) = (resolver(), Table::default());
        t.add(90, 1, r"C:\Spotify\Spotify.exe", 1).publisher = Some("Spotify AB".into());
        t.add(91, 2, r"C:\Spotify\SpotifyCrashReporter.exe", 90)
            .publisher = Some("Spotify AB".into());
        assert_eq!(app(&mut r, &mut t, 90).family, "spotify");
        assert_eq!(app(&mut r, &mut t, 91).family, "spotify");
        let ms = Some("Microsoft Windows".to_owned());
        t.add(92, 1, r"C:\Windows\System32\a.exe", 1).publisher = ms.clone();
        t.add(93, 1, r"C:\Windows\System32\b.exe", 1).publisher = ms;
        assert_ne!(
            app(&mut r, &mut t, 92).family,
            app(&mut r, &mut t, 93).family
        );
    }

    #[test]
    fn store_packages_group_by_package() {
        let (mut r, mut t) = (resolver(), Table::default());
        let p = r"C:\Program Files\WindowsApps\4DF9E0F8.Netflix_6.99.5.0_x64__mcm4njqhnhss8";
        t.add(100, 1, &format!(r"{p}\Netflix.exe"), 1).package =
            Some("4DF9E0F8.Netflix_mcm4njqhnhss8".into());
        t.add(101, 2, &format!(r"{p}\helper.exe"), 100).package =
            Some("4DF9E0F8.Netflix_mcm4njqhnhss8".into());
        let a = app(&mut r, &mut t, 100);
        assert_eq!((a.how, a.name.as_str()), (How::Package, "4DF9E0F8.Netflix"));
        assert_eq!(a.family, app(&mut r, &mut t, 101).family);
    }

    #[test]
    fn system_unknown_and_unopenable() {
        let (mut r, mut t) = (resolver(), Table::default());
        assert_eq!(app(&mut r, &mut t, 4).how, How::System);
        assert_eq!(app(&mut r, &mut t, 999).how, How::Unknown);
        t.add(110, 1, "", 1).path = None;
        assert_eq!(app(&mut r, &mut t, 110).how, How::Unknown);
    }

    #[test]
    fn display_names() {
        let (mut r, mut t) = (resolver(), Table::default());
        t.add(120, 1, r"D:\Games\MyGame\MyGame.exe", 1);
        t.add(121, 1, r"D:\Tools\thing.exe", 1).description = Some("  Thing Tool ".into());
        assert_eq!(app(&mut r, &mut t, 120).name, "MyGame.exe");
        assert_eq!(app(&mut r, &mut t, 121).name, "Thing Tool");
        assert_eq!(
            app(&mut r, &mut t, 120).family,
            r"d:\games\mygame\mygame.exe"
        );
    }

    #[test]
    fn docker_helpers_and_opencode_are_named_apps() {
        let (mut r, mut t) = (resolver(), Table::default());
        t.add(
            130,
            1,
            r"C:\Program Files\Docker\Docker\resources\com.docker.build.exe",
            1,
        );
        t.add(
            131,
            2,
            r"C:\Program Files\Docker\Docker\resources\com.docker.backend.exe",
            130,
        );
        t.add(132, 1, r"C:\Users\me\.opencode\bin\opencode.exe", 1);
        let build = app(&mut r, &mut t, 130);
        assert_eq!(
            (build.family.as_str(), build.name.as_str()),
            ("docker", "Docker")
        );
        assert_eq!(app(&mut r, &mut t, 131).family, "docker");
        assert_eq!(app(&mut r, &mut t, 132).name, "OpenCode");
    }

    #[test]
    fn glob_matching() {
        assert!(glob(
            r"*\steamapps\common\*",
            r"c:\steam\steamapps\common\x\y.exe"
        ));
        assert!(!glob(r"*\steamapps\common\*", r"c:\steam\steam.exe"));
        assert!(glob(
            r"*\battle.net\agent\*agent.exe",
            r"c:\programdata\battle.net\agent\agent.9010\agent.exe"
        ));
        assert!(glob("a*b*c", "aXbYbZc"));
        assert!(!glob("a*b", "aXbYc"));
        assert!(glob("*", ""));
        assert!(glob("abc", "abc"));
        assert!(!glob("abc", "abcd"));
    }

    #[test]
    fn pack_rejects_bad_ids() {
        let bad = [
            "[[family]]\nid = \"Steam\"\nname = \"S\"\nexes = [\"s.exe\"]",
            "[[family]]\nid = \"system\"\nname = \"S\"\nexes = [\"s.exe\"]",
            "[[family]]\nid = \"a\"\nname = \"A\"\nexes = [\"a.exe\"]\n[[family]]\nid = \"a\"\nname = \"B\"\nexes = [\"b.exe\"]",
            "[[family]]\nid = \"a\"\nname = \"A\"",
            "[[family]]\nid = \"a\"\nname = \"A\"\nexes = [\"a.exe\"]\nbogus = 1",
        ];
        for text in bad {
            assert!(Pack::parse(text).is_err(), "{text}");
        }
    }

    #[test]
    fn service_name_parsing() {
        assert_eq!(
            service_name("svchost.exe -k netsvcs -p -s Wuauserv").as_deref(),
            Some("wuauserv")
        );
        assert_eq!(service_name("svchost.exe -k netsvcs -p"), None);
        assert_eq!(service_name("svchost.exe -s"), None);
    }
}
