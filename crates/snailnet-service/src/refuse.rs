//! Refusing a blocked app's new internet connections at the SOCKET layer
//! (PLAN.md §11.3): its `connect()` fails at once with a normal network
//! error, instead of hanging on dropped packets. The NETWORK layer still
//! drops what gets past this (flows opened before, the first connect of a
//! process not resolved yet, traffic without a connect).
//!
//! One WinDivert handle in DROP mode whose filter lists the blocked apps'
//! process ids. It's rebuilt when that list changes (the new handle opens
//! before the old one closes, so there's no gap). It sits below the
//! sniffing SOCKET handle, which still sees every connect, so a new
//! process (or a reused PID) is attributed before anything is refused.
//! Closing it, also when the service dies, refuses nothing any more: it
//! fails open.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::thread::JoinHandle;
use std::time::Duration;

use snailnet_engine::windivert::{Layer, WinDivert, WinDivertLib, flags};

use snailnet_core::classify::{Attribution, ProcessSource};

use crate::shaper::{LiveRules, Rules, WinAttribution};

/// Process ids in one filter at most (WinDivert filters are short); a
/// blocked app with more processes has the rest dropped at the NETWORK
/// layer only.
pub const MAX_PIDS: usize = 150;
/// How often the list of blocked processes is checked.
const EVERY: Duration = Duration::from_millis(500);
/// Below the sniffing handles (priority 0), so they see each event first.
const PRIORITY: i16 = -100;

/// Addresses that stay on the local network, as
/// [`snailnet_core::parse::ip::is_local_network`] has them (blocking is
/// internet only). Loopback is excluded separately.
const LOCAL: &[(&str, &str)] = &[
    ("10.0.0.0", "10.255.255.255"),
    ("172.16.0.0", "172.31.255.255"),
    ("192.168.0.0", "192.168.255.255"),
    ("169.254.0.0", "169.254.255.255"),
    // Multicast and broadcast.
    ("224.0.0.0", "255.255.255.255"),
    // Unique local, link-local and multicast IPv6.
    ("fc00::", "fdff:ffff:ffff:ffff:ffff:ffff:ffff:ffff"),
    ("fe80::", "febf:ffff:ffff:ffff:ffff:ffff:ffff:ffff"),
    ("ff00::", "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff"),
];

/// The SOCKET-layer filter refusing internet connects of `pids`; `None`
/// when there are none.
pub fn filter(pids: &[u32]) -> Option<String> {
    if pids.is_empty() {
        return None;
    }
    let who = pids
        .iter()
        .map(|p| format!("processId == {p}"))
        .collect::<Vec<_>>()
        .join(" or ");
    let mut f = format!("event == CONNECT and !loopback and ({who})");
    // WinDivert has no `!(…)`: each range is "below or above".
    for (lo, hi) in LOCAL {
        f.push_str(&format!(" and (remoteAddr < {lo} or remoteAddr > {hi})"));
    }
    Some(f)
}

/// The blocked apps' live processes (sorted, at most [`MAX_PIDS`]).
/// `alive(pid, created)`: the process still runs (not a reused PID).
fn blocked_pids<S: ProcessSource>(
    attribution: &Attribution<S>,
    rules: &dyn Rules,
    alive: impl Fn(u32, u64) -> bool,
) -> Vec<u32> {
    if !rules.blocks_any() {
        return Vec::new();
    }
    let mut pids: Vec<u32> = attribution
        .processes()
        .into_iter()
        .filter(|(key, app)| rules.blocks(&attribution.app(*app)) && alive(key.pid, key.created))
        .map(|(key, _)| key.pid)
        .collect();
    pids.sort_unstable();
    pids.truncate(MAX_PIDS);
    pids
}

/// The running refuser thread.
pub struct Refuser {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Refuser {
    pub(crate) fn start(
        lib: Arc<WinDivertLib>,
        attribution: Arc<WinAttribution>,
        rules: Arc<LiveRules>,
    ) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("socket-refuser".into())
            .spawn(move || {
                let alive = |pid, created| snailnet_win::process::created(pid) == Some(created);
                let mut handle: Option<WinDivert> = None;
                let mut current: Vec<u32> = Vec::new();
                let mut warned = false;
                while !flag.load(Relaxed) {
                    let pids = blocked_pids(&attribution, &*rules.load(), alive);
                    if pids != current {
                        match filter(&pids) {
                            None => {
                                handle = None;
                                current = pids;
                            }
                            Some(f) => {
                                match WinDivert::open(
                                    &lib,
                                    &f,
                                    Layer::Socket,
                                    PRIORITY,
                                    flags::DROP,
                                ) {
                                    Ok(h) => {
                                        // The old handle closes after the
                                        // new one is open.
                                        handle = Some(h);
                                        tracing::info!(
                                            "refusing new connections of {} blocked processes",
                                            pids.len()
                                        );
                                        current = pids;
                                        warned = false;
                                    }
                                    Err(e) if !warned => {
                                        warned = true;
                                        tracing::warn!(
                                            "could not refuse blocked apps' connections \
                                             (their packets are still dropped): {e}"
                                        );
                                    }
                                    Err(_) => {}
                                }
                            }
                        }
                    }
                    std::thread::sleep(EVERY);
                }
                drop(handle);
            })?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for Refuser {
    fn drop(&mut self) {
        self.stop.store(true, Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;

    use snailnet_core::classify::{Pack, ProcessFacts, Resolver};
    use snailnet_core::rules::Config;

    use crate::shaper::PlanRules;

    struct Procs(HashMap<u32, ProcessFacts>);

    impl ProcessSource for Procs {
        fn created(&mut self, pid: u32) -> Option<u64> {
            self.0.get(&pid).map(|f| f.created)
        }
        fn facts(&mut self, pid: u32) -> Option<ProcessFacts> {
            self.0.get(&pid).cloned()
        }
    }

    #[test]
    fn only_live_processes_of_blocked_apps() {
        let fact = |pid, path: &str| {
            (
                pid,
                ProcessFacts {
                    pid,
                    created: 1,
                    path: Some(path.into()),
                    ..Default::default()
                },
            )
        };
        let procs = Procs(HashMap::from([
            fact(30, r"C:\Steam\bin\cef\steamwebhelper.exe"),
            fact(10, r"C:\Steam\steam.exe"),
            fact(20, r"C:\Brave\brave.exe"),
            fact(40, r"C:\Steam\steam.exe"),
        ]));
        let a = Attribution::new(Resolver::new(Pack::builtin(), r"C:\Windows"), procs);
        for pid in [10, 20, 30, 40] {
            a.owner_of(pid);
        }
        let mut c = Config::default();
        let none = PlanRules::new(c.plan(0));
        assert!(blocked_pids(&a, &none, |_, _| true).is_empty());
        c.blocked = vec!["steam".into()];
        let steam = PlanRules::new(c.plan(0));
        assert_eq!(blocked_pids(&a, &steam, |_, _| true), [10, 30, 40]);
        // 40 exited (or its PID went to another process).
        assert_eq!(blocked_pids(&a, &steam, |pid, _| pid != 40), [10, 30]);
    }

    #[test]
    fn no_pids_no_filter() {
        assert_eq!(filter(&[]), None);
    }

    #[test]
    fn the_filter_lists_the_pids_and_spares_the_lan() {
        let f = filter(&[12, 34]).unwrap();
        assert!(f.starts_with(
            "event == CONNECT and !loopback and (processId == 12 or processId == 34)"
        ));
        assert!(f.contains("(remoteAddr < 192.168.0.0 or remoteAddr > 192.168.255.255)"));
    }

    /// WinDivert itself accepts it, at the longest it gets.
    #[test]
    fn windivert_compiles_the_longest_filter() {
        let lib = WinDivertLib::load().unwrap();
        let pids: Vec<u32> = (1000..1000 + MAX_PIDS as u32).collect();
        let f = filter(&pids).unwrap();
        assert_eq!(lib.check_filter(&f, Layer::Socket), Ok(()), "{f}");
        assert_eq!(
            lib.check_filter(&filter(&[7]).unwrap(), Layer::Socket),
            Ok(())
        );
    }
}
