//! Watching which network the PC is on (PLAN.md §10.3). Every few seconds
//! it asks Windows for the internet route (cheap, sends nothing); when the
//! router or adapter changes it identifies the network and tells the hub,
//! which swaps in that network's line speed. A config change wakes the
//! runner, which follows it like any other.
//!
//! Offline (no route) changes nothing: the line stays the last network's
//! until another one appears.

use std::time::Duration;

use snailnet_core::networks::Switch;
use snailnet_win::network::{self, Route};
use tracing::{info, warn};

use crate::hub::Hub;
use crate::runner::unix_now;

/// How often to look at the route.
const EVERY: Duration = Duration::from_secs(5);

/// Run until the hub's stop signal.
pub fn watch(hub: &Hub) {
    let stop = hub.signal();
    // The route last told to the hub; a refusal (the service still
    // starting, or a client's change in between) tries again next round.
    let mut told: Option<Route> = None;
    loop {
        if let Some(route) = network::route()
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
