//! Watching which network the PC is on (PLAN.md §10.3). Every few seconds
//! it asks Windows for the internet route (cheap, sends nothing); when the
//! router or adapter changes it identifies the network and tells the hub,
//! which swaps in that network's line speed. A config change wakes the
//! runner, which follows it like any other.
//!
//! Offline (no route) changes nothing: the line stays the last network's
//! until another one appears.

use std::time::{Duration, SystemTime};

use snailnet_core::networks::Switch;
use snailnet_win::network::{self, Route};
use tracing::{info, warn};

use crate::hub::Hub;
use crate::runner::unix_now;

/// How often to look at the route.
const EVERY: Duration = Duration::from_secs(5);
/// A round this much later than [`EVERY`] means the PC slept in between.
const WOKE_AFTER: Duration = Duration::from_secs(30);

/// Run until the hub's stop signal.
pub fn watch(hub: &Hub) {
    let stop = hub.signal();
    // The route last told to the hub; a refusal (the service still
    // starting, or a client's change in between) tries again next round.
    let mut told: Option<Route> = None;
    // The route seen last round (`None` inside: offline), to notice the
    // PC coming back online or moving to another adapter or router.
    let mut last: Option<Option<Route>> = None;
    let mut last_wall = SystemTime::now();
    loop {
        let now = network::route();
        // Wall time keeps running while the PC sleeps; this thread's waits
        // don't. A jump far past EVERY means the PC just woke up.
        let wall = SystemTime::now();
        let woke = wall
            .duration_since(last_wall)
            .is_ok_and(|d| d > EVERY + WOKE_AFTER);
        last_wall = wall;
        if let Some(prev) = last
            && now.is_some()
            && (prev != now || woke)
        {
            // Back online after sleep, a Wi-Fi drop, or a new adapter:
            // whatever was diverted on the old route is stale. Shaping
            // restarts once the new route is up (runner waits for it).
            info!(
                "network: {}; restarting shaping",
                if woke {
                    "the PC woke up"
                } else {
                    "the route to the internet changed"
                }
            );
            hub.route_changed();
        }
        last = Some(now);
        if let Some(route) = now
            && told != Some(route)
        {
            let net = network::identify(route);
            let name = net.name.clone();
            match hub.network_seen(net, unix_now()) {
                Ok(switch) => {
                    told = Some(route);
                    log(switch, &name, hub);
                }
                Err((_, e)) => warn!("network: could not switch to {name:?} yet: {e}"),
            }
        }
        if stop.wait(EVERY) {
            return;
        }
    }
}

fn log(switch: Switch, name: &str, hub: &Hub) {
    let line = hub.config().line;
    let speed = || {
        let f = |v: Option<f64>| v.map_or("–".into(), |v| format!("{v}"));
        format!("down {} / up {} Mbps", f(line.down_mbps), f(line.up_mbps))
    };
    match switch {
        Switch::Same => {}
        Switch::Adopted => info!("network: on {name:?}; the line speed set is this network's"),
        Switch::Known => info!("network: back on {name:?}; its line speed is {}", speed()),
        Switch::New => info!(
            "network: on {name:?}, new here; not shaping until its speed is measured or typed"
        ),
    }
}
