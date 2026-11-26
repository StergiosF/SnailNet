//! Packet → app (PLAN.md §6.3, §7).
//!
//! - [`flow`]: which process owns a connection (FLOW, SOCKET, port snapshot).
//! - [`family`]: which app a process belongs to (packs, services, parents,
//!   packages, folders).
//! - [`Attribution`] ties them together. The event threads (FLOW, SOCKET,
//!   port snapshot) call [`Attribution::apply`] / [`Attribution::set_ports`],
//!   which resolve the process to an app *there*, off the fast path. The rx
//!   threads only do [`Attribution::lookup`]: one hash lookup under a read
//!   lock, returning an interned [`AppId`].
//! - [`Pending`]: per-rx-thread memory of flows whose first packets beat
//!   their FLOW/SOCKET event. They go to Everything else for up to
//!   [`PENDING_WINDOW`] and are re-classified as soon as the event lands.

pub mod family;
pub mod flow;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use crate::clock::{NANOS_PER_MILLI, NANOS_PER_SEC, Nanos};
use crate::families::Families;
pub use family::{App, How, Pack, ProcessFacts, ProcessKey, ProcessSource, Resolver};
pub use flow::{AppId, FlowKey, FlowTable, Owner, PortKey, Source};

/// A FLOW- or SOCKET-layer event, already decoded from the engine's format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    FlowEstablished {
        key: FlowKey,
        pid: u32,
    },
    FlowDeleted {
        key: FlowKey,
    },
    /// SOCKET connect or accept: the full 5-tuple.
    Connected {
        key: FlowKey,
        pid: u32,
        endpoint: u64,
    },
    /// SOCKET bind or listen: `key.remote` is unspecified.
    Bound {
        key: FlowKey,
        pid: u32,
        endpoint: u64,
    },
    Closed {
        key: FlowKey,
        endpoint: u64,
    },
}

/// Interned app identities: one id per (family, exe) pair, never reused,
/// so the fast path can cache `AppId → class` without invalidation.
///
/// Ids are for the app as resolved; what [`Attribution::app`] shows is it
/// grouped as the user wants ([`Families`]), so splitting or merging keeps
/// every id (and the flows that carry it).
#[derive(Default)]
struct Apps {
    list: Vec<App>,
    shown: Vec<Arc<App>>,
    /// family → `[(exe, AppId)]`: zero-allocation read lookup before taking a
    /// write lock (most families only have 1–3 exes).
    index: HashMap<String, Vec<(String, AppId)>>,
    families: Families,
}

impl Apps {
    fn get(&self, family: &str, exe: &str) -> Option<AppId> {
        self.index
            .get(family)?
            .iter()
            .find(|(e, _)| e == exe)
            .map(|&(_, id)| id)
    }

    fn intern(&mut self, app: App) -> (AppId, bool) {
        if let Some(id) = self.get(&app.family, &app.exe) {
            return (id, false);
        }
        let id = self.list.len() as AppId;
        let family = app.family.clone();
        let exe = app.exe.clone();
        self.shown.push(Arc::new(self.families.apply(app.clone())));
        self.list.push(app);
        self.index.entry(family).or_default().push((exe, id));
        (id, true)
    }

    fn set_families(&mut self, families: &Families) -> bool {
        if *families == self.families {
            return false;
        }
        self.families = families.clone();
        self.shown = self
            .list
            .iter()
            .map(|a| Arc::new(self.families.apply(a.clone())))
            .collect();
        true
    }
}

struct Slow<S> {
    resolver: Resolver,
    source: S,
}

/// The shared connection → app map.
pub struct Attribution<S> {
    table: RwLock<FlowTable>,
    slow: Mutex<Slow<S>>,
    apps: RwLock<Apps>,
    /// Bumped whenever a new app is interned or family groupings change.
    apps_version: AtomicU64,
    /// Bumped when flow/port ownership entries are removed or replaced.
    table_epoch: AtomicU64,
    /// Every process resolved so far and its app (PID → key, app), for
    /// refusing a blocked app's connects by PID (PLAN.md §11.3).
    procs: Mutex<HashMap<u32, (ProcessKey, AppId)>>,
}

/// Processes remembered at most; past it the list starts afresh (each
/// process is added back at its next socket event).
const MAX_PROCS: usize = 4096;

impl<S: ProcessSource> Attribution<S> {
    pub fn new(resolver: Resolver, source: S) -> Self {
        Self {
            table: RwLock::new(FlowTable::default()),
            slow: Mutex::new(Slow { resolver, source }),
            apps: RwLock::new(Apps::default()),
            apps_version: AtomicU64::new(0),
            table_epoch: AtomicU64::new(0),
            procs: Mutex::new(HashMap::new()),
        }
    }

    /// Resolve a PID to its app (cached per process) and intern it.
    pub fn owner_of(&self, pid: u32) -> Owner {
        let resolved = {
            let mut slow = self.slow.lock().unwrap();
            let Slow { resolver, source } = &mut *slow;
            resolver.resolve(pid, source)
        };
        let app = {
            let existing = self
                .apps
                .read()
                .unwrap()
                .get(&resolved.app.family, &resolved.app.exe);
            match existing {
                Some(id) => id,
                None => {
                    let (id, added) = self.apps.write().unwrap().intern(resolved.app);
                    if added {
                        self.apps_version.fetch_add(1, Ordering::Release);
                    }
                    id
                }
            }
        };
        if let Some(key) = resolved.process {
            let mut procs = self.procs.lock().unwrap();
            if procs.len() >= MAX_PROCS && !procs.contains_key(&pid) {
                procs.clear();
            }
            procs.insert(pid, (key, app));
        }
        Owner { pid, app }
    }

    /// Apply one FLOW/SOCKET event. Resolves the process first, outside the
    /// table lock, so the rx threads are never held up by it.
    pub fn apply(&self, event: Event) {
        let changed = match event {
            Event::FlowEstablished { key, pid } => {
                let owner = self.owner_of(pid);
                self.table.write().unwrap().flow_established(key, owner)
            }
            Event::FlowDeleted { key } => self.table.write().unwrap().flow_deleted(&key),
            Event::Connected { key, pid, endpoint } => {
                let owner = self.owner_of(pid);
                self.table
                    .write()
                    .unwrap()
                    .socket_connected(key, owner, endpoint)
            }
            Event::Bound { key, pid, endpoint } => {
                let owner = self.owner_of(pid);
                let port = (key.protocol, key.local.1);
                self.table
                    .write()
                    .unwrap()
                    .socket_bound(port, owner, endpoint)
            }
            Event::Closed { key, endpoint } => {
                self.table.write().unwrap().socket_closed(&key, endpoint)
            }
        };
        if changed {
            self.table_epoch.fetch_add(1, Ordering::Release);
        }
    }

    /// Replace the port snapshot: `(protocol, local port) → pid`.
    pub fn set_ports(&self, snapshot: &HashMap<PortKey, u32>) {
        let mut by_pid: HashMap<u32, Owner> = HashMap::new();
        let ports = snapshot
            .iter()
            .map(|(&port, &pid)| {
                (
                    port,
                    *by_pid.entry(pid).or_insert_with(|| self.owner_of(pid)),
                )
            })
            .collect();
        self.table.write().unwrap().set_ports(ports);
        self.table_epoch.fetch_add(1, Ordering::Release);
    }

    pub fn lookup(&self, key: &FlowKey) -> Option<(Owner, Source)> {
        self.table.read().unwrap().lookup(key)
    }

    /// The app behind an id from [`Self::lookup`] or [`Self::owner_of`].
    pub fn app(&self, id: AppId) -> Arc<App> {
        self.apps.read().unwrap().shown[id as usize].clone()
    }

    /// Group apps as the user wants from now on (PLAN.md §11.6). Ids stay;
    /// callers caching per-id results (classes, blocks) must drop them.
    pub fn set_families(&self, families: &Families) {
        if self.apps.write().unwrap().set_families(families) {
            self.apps_version.fetch_add(1, Ordering::Release);
        }
    }

    /// Monotonic version of the seen-apps list and user family groupings.
    pub fn apps_version(&self) -> u64 {
        self.apps_version.load(Ordering::Acquire)
    }

    /// Monotonic epoch bumped whenever flow/port table entries are removed or replaced.
    pub fn table_epoch(&self) -> u64 {
        self.table_epoch.load(Ordering::Acquire)
    }

    /// (flows, socket endpoints, snapshot ports, skipped because full).
    pub fn sizes(&self) -> (usize, usize, usize, u64) {
        self.table.read().unwrap().sizes()
    }

    pub fn app_count(&self) -> usize {
        self.apps.read().unwrap().list.len()
    }

    /// The processes resolved so far with their apps. Some may have exited
    /// (check the key's creation time before trusting a PID).
    pub fn processes(&self) -> Vec<(ProcessKey, AppId)> {
        self.procs.lock().unwrap().values().copied().collect()
    }

    /// Every app seen so far, in the order first seen.
    pub fn apps(&self) -> Vec<Arc<App>> {
        self.apps.read().unwrap().shown.clone()
    }
}

/// How long a flow with no known owner stays "pending" (PLAN.md §6.3).
pub const PENDING_WINDOW: Nanos = 50 * NANOS_PER_MILLI;
/// Per-thread bound on remembered misses.
const PENDING_CAP: usize = 16_384;
/// Misses older than this are forgotten when the map is pruned.
const PENDING_FORGET: Nanos = 60 * NANOS_PER_SEC;

/// What a lookup miss means for this packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Miss {
    /// Within the window: the FLOW/SOCKET event is probably on its way.
    Pending,
    /// Past the window: nobody claimed this flow.
    Unknown,
}

/// One rx thread's record of flows it couldn't attribute yet.
#[derive(Debug, Clone)]
pub struct Pending {
    window: Nanos,
    first_miss: HashMap<FlowKey, Nanos>,
}

impl Default for Pending {
    fn default() -> Self {
        Self::new(PENDING_WINDOW)
    }
}

impl Pending {
    pub fn new(window: Nanos) -> Self {
        Self {
            window,
            first_miss: HashMap::new(),
        }
    }

    pub fn miss(&mut self, key: FlowKey, now: Nanos) -> Miss {
        if self.first_miss.len() >= PENDING_CAP && !self.first_miss.contains_key(&key) {
            self.first_miss
                .retain(|_, &mut t| now.saturating_sub(t) < PENDING_FORGET);
            if self.first_miss.len() >= PENDING_CAP {
                self.first_miss.clear();
            }
        }
        let first = *self.first_miss.entry(key).or_insert(now);
        if now.saturating_sub(first) < self.window {
            Miss::Pending
        } else {
            Miss::Unknown
        }
    }

    /// The flow is attributed now. True if it had been pending, i.e. its
    /// first packets beat the event.
    pub fn hit(&mut self, key: &FlowKey) -> bool {
        !self.first_miss.is_empty() && self.first_miss.remove(key).is_some()
    }

    pub fn len(&self) -> usize {
        self.first_miss.len()
    }

    pub fn is_empty(&self) -> bool {
        self.first_miss.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    #[derive(Default)]
    struct Procs(HashMap<u32, ProcessFacts>);

    impl Procs {
        fn with(mut self, pid: u32, created: u64, path: &str) -> Self {
            self.0.insert(
                pid,
                ProcessFacts {
                    pid,
                    created,
                    path: Some(path.into()),
                    ..Default::default()
                },
            );
            self
        }
    }

    impl ProcessSource for Procs {
        fn created(&mut self, pid: u32) -> Option<u64> {
            self.0.get(&pid).map(|f| f.created)
        }
        fn facts(&mut self, pid: u32) -> Option<ProcessFacts> {
            self.0.get(&pid).cloned()
        }
    }

    fn attribution() -> Attribution<Procs> {
        let procs = Procs::default()
            .with(10, 1, r"C:\Steam\steam.exe")
            .with(11, 2, r"C:\Steam\bin\cef\steamwebhelper.exe")
            .with(20, 3, r"C:\Brave\brave.exe");
        Attribution::new(Resolver::new(Pack::builtin(), r"C:\Windows"), procs)
    }

    fn key(local_port: u16, remote_port: u16) -> FlowKey {
        let me = IpAddr::from([10, 0, 0, 2]);
        let them = IpAddr::from([93, 184, 216, 34]);
        FlowKey::new(6, (me, local_port), (them, remote_port))
    }

    #[test]
    fn events_map_flows_to_interned_apps() {
        let a = attribution();
        a.apply(Event::FlowEstablished {
            key: key(5000, 443),
            pid: 10,
        });
        a.apply(Event::Connected {
            key: key(5001, 443),
            pid: 11,
            endpoint: 1,
        });
        a.apply(Event::Connected {
            key: key(5002, 443),
            pid: 10,
            endpoint: 2,
        });
        let (o1, s1) = a.lookup(&key(5000, 443)).unwrap();
        let (o2, s2) = a.lookup(&key(5001, 443)).unwrap();
        let (o3, _) = a.lookup(&key(5002, 443)).unwrap();
        assert_eq!((s1, s2), (Source::Flow, Source::Socket));
        assert_eq!(o1.app, o3.app, "same process → same id");
        assert_ne!(o1.app, o2.app, "same family, different exe → different id");
        assert_eq!(a.app(o1.app).family, "steam");
        assert_eq!(a.app(o2.app).family, "steam");
        assert_eq!(a.app(o2.app).exe, "steamwebhelper.exe");
        a.apply(Event::Closed {
            key: key(5001, 443),
            endpoint: 1,
        });
        assert_eq!(a.lookup(&key(5001, 443)), None);
        a.apply(Event::FlowDeleted {
            key: key(5000, 443),
        });
        assert_eq!(a.sizes(), (1, 0, 0, 0));
    }

    #[test]
    fn families_regroup_apps_and_keep_their_ids() {
        let a = attribution();
        let web = a.owner_of(11).app;
        let steam = a.owner_of(10).app;
        a.set_families(&Families {
            split: vec!["steamwebhelper.exe".into()],
            ..Families::default()
        });
        assert_eq!(a.app(web).family, "split:steamwebhelper.exe");
        assert_eq!(a.app(steam).family, "steam");
        // Resolved after the change: grouped the same way, same id again.
        assert_eq!(a.owner_of(11).app, web);
        let brave = a.owner_of(20).app;
        assert_eq!(a.apps().len(), 3);
        a.set_families(&Families::default());
        assert_eq!(a.app(web).family, "steam");
        assert_eq!(a.app(brave).family, "brave");
    }

    #[test]
    fn resolved_processes_are_remembered_with_their_app() {
        let a = attribution();
        a.owner_of(10);
        a.owner_of(20);
        a.owner_of(99); // gone already: nothing to remember
        let mut p = a.processes();
        p.sort_by_key(|(k, _)| k.pid);
        let got: Vec<_> = p
            .iter()
            .map(|(k, id)| (k.pid, k.created, a.app(*id).family.clone()))
            .collect();
        assert_eq!(
            got,
            [(10, 1, "steam".to_owned()), (20, 3, "brave".to_owned())]
        );
    }

    #[test]
    fn bind_and_ports_cover_the_local_port() {
        let a = attribution();
        a.set_ports(&HashMap::from([((6, 6000), 20), ((17, 6001), 999)]));
        let brave = a.lookup(&key(6000, 80)).unwrap();
        assert_eq!(
            (a.app(brave.0.app).family.as_str(), brave.1),
            ("brave", Source::Port)
        );
        // A PID that exited before we looked is still attributed, as Unknown.
        let gone = a
            .lookup(&FlowKey {
                protocol: 17,
                ..key(6001, 53)
            })
            .unwrap();
        assert_eq!(a.app(gone.0.app).how, How::Unknown);
        a.apply(Event::Bound {
            key: key(6000, 0),
            pid: 10,
            endpoint: 7,
        });
        let (o, s) = a.lookup(&key(6000, 80)).unwrap();
        assert_eq!((a.app(o.app).family.as_str(), s), ("steam", Source::Socket));
    }

    #[test]
    fn pending_window() {
        let mut p = Pending::default();
        let k = key(7000, 443);
        assert_eq!(p.miss(k, 1_000), Miss::Pending);
        assert_eq!(p.miss(k, 1_000 + PENDING_WINDOW - 1), Miss::Pending);
        assert_eq!(p.miss(k, 1_000 + PENDING_WINDOW), Miss::Unknown);
        assert!(p.hit(&k), "the event arrived: it was pending");
        assert!(!p.hit(&k));
        assert!(p.is_empty());
    }

    #[test]
    fn pending_is_bounded() {
        let mut p = Pending::default();
        for i in 0..PENDING_CAP as u16 {
            p.miss(key(i, 1), 0);
        }
        assert_eq!(p.len(), PENDING_CAP);
        // Full of fresh entries: one more starts the map over.
        p.miss(key(1, 2), 1);
        assert_eq!(p.len(), 1);
        // Full again, all stale by `now`: they're forgotten, the new one kept.
        for i in 0..PENDING_CAP as u16 - 1 {
            p.miss(key(i, 3), 0);
        }
        assert_eq!(p.len(), PENDING_CAP);
        p.miss(key(1, 4), PENDING_FORGET + 1);
        assert_eq!(p.len(), 1);
    }

    #[test]
    fn table_epoch_bumps_when_owner_changes_but_not_on_fresh_flow() {
        let a = attribution();
        let e0 = a.table_epoch();
        // Brand-new flow: nothing cached could be invalidated.
        a.apply(Event::FlowEstablished {
            key: key(8000, 443),
            pid: 10,
        });
        assert_eq!(a.table_epoch(), e0);

        // Stale port snapshot claims port 8001 belongs to Brave (pid 20).
        a.set_ports(&HashMap::from([((6, 8001), 20)]));
        let e1 = a.table_epoch();
        assert!(e1 > e0);

        // FlowEstablished overrides port 8001 with Steam (pid 10) -> must bump epoch
        // so rx-thread flow caches drop the stale Brave attribution immediately.
        a.apply(Event::FlowEstablished {
            key: key(8001, 443),
            pid: 10,
        });
        let e2 = a.table_epoch();
        assert!(e2 > e1, "owner override must advance table_epoch");

        // Socket Bound overriding a port with a different app also bumps epoch.
        a.set_ports(&HashMap::from([((6, 8002), 20)]));
        let e3 = a.table_epoch();
        a.apply(Event::Bound {
            key: key(8002, 0),
            pid: 10,
            endpoint: 55,
        });
        assert!(a.table_epoch() > e3);
    }
}
