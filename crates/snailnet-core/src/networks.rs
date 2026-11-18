//! Speeds per network (PLAN.md §10.3): home Wi-Fi, a phone hotspot and the
//! office each keep their own line speed. `[line]` is always the current
//! network's; the others wait in `[[networks]]` until they're back.

use crate::rules::{Config, Line, MAX_SAVED_NETWORKS, Network, SavedNetwork};

/// What moving to a network did to the line speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    /// Already on it (perhaps under a new name).
    Same,
    /// The first network seen: the speeds already set are its own.
    Adopted,
    /// Back on a network seen before: its saved speeds are the line's again.
    Known,
    /// A network not seen before: no speed yet, so SnailNet only watches
    /// until it's measured or typed (a speed from elsewhere could be far too
    /// low here and slow everything down).
    New,
}

impl Config {
    /// Make `net` the current network, saving the line speed of the one
    /// left (as of `now`, Unix seconds) and bringing back `net`'s.
    pub fn switch_network(&mut self, net: Network, now: u64) -> Switch {
        let Some(cur) = self.network.take() else {
            self.networks.retain(|n| n.id != net.id);
            self.network = Some(net);
            return Switch::Adopted;
        };
        if cur.id == net.id {
            self.network = Some(net);
            return Switch::Same;
        }
        // Low-latency tuning by hand is a preference, not a property of
        // the line: it comes along.
        let factor = self.line.factor;
        let left = std::mem::take(&mut self.line);
        self.networks.retain(|n| n.id != cur.id);
        self.networks.push(SavedNetwork {
            id: cur.id,
            name: cur.name,
            left: now,
            line: left,
        });
        let switch = match self.networks.iter().position(|n| n.id == net.id) {
            Some(i) => {
                self.line = self.networks.remove(i).line;
                Switch::Known
            }
            None => {
                self.line = Line {
                    factor,
                    ..Line::default()
                };
                Switch::New
            }
        };
        if self.networks.len() > MAX_SAVED_NETWORKS {
            self.networks.sort_by_key(|n| std::cmp::Reverse(n.left));
            self.networks.truncate(MAX_SAVED_NETWORKS);
        }
        self.network = Some(net);
        switch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn net(id: &str, name: &str) -> Network {
        Network {
            id: id.into(),
            name: name.into(),
        }
    }

    fn with_speed(down: f64) -> Config {
        let mut c = Config::default();
        c.line.down_mbps = Some(down);
        c
    }

    #[test]
    fn the_first_network_seen_keeps_the_speeds_already_set() {
        let mut c = with_speed(25.0);
        assert_eq!(c.switch_network(net("a", "Home"), 1), Switch::Adopted);
        assert_eq!(c.line.down_mbps, Some(25.0));
        assert_eq!(c.network, Some(net("a", "Home")));
        assert!(c.networks.is_empty());
    }

    #[test]
    fn the_same_network_changes_nothing_but_its_name() {
        let mut c = with_speed(25.0);
        c.switch_network(net("a", "Home"), 1);
        assert_eq!(c.switch_network(net("a", "Home 5G"), 2), Switch::Same);
        assert_eq!(c.line.down_mbps, Some(25.0));
        assert_eq!(c.network.unwrap().name, "Home 5G");
    }

    #[test]
    fn a_new_network_starts_with_no_speed_and_the_old_one_is_kept() {
        let mut c = with_speed(25.0);
        c.line.factor = Some(0.85);
        c.line.pinned = Some(true);
        c.switch_network(net("a", "Home"), 1);
        assert_eq!(c.switch_network(net("b", "Phone"), 2), Switch::New);
        assert_eq!(
            c.line,
            Line {
                factor: Some(0.85),
                ..Line::default()
            }
        );
        assert_eq!(c.networks.len(), 1);
        assert_eq!(c.networks[0].id, "a");
        assert_eq!(c.networks[0].left, 2);
        assert_eq!(c.networks[0].line.down_mbps, Some(25.0));
        assert!(c.networks[0].line.is_pinned());
    }

    #[test]
    fn coming_back_brings_the_saved_speed_back() {
        let mut c = with_speed(25.0);
        c.switch_network(net("a", "Home"), 1);
        c.switch_network(net("b", "Phone"), 2);
        c.line.down_mbps = Some(8.0);
        assert_eq!(c.switch_network(net("a", "Home"), 3), Switch::Known);
        assert_eq!(c.line.down_mbps, Some(25.0));
        assert_eq!(c.networks.len(), 1);
        assert_eq!(c.networks[0].id, "b");
        assert_eq!(c.networks[0].line.down_mbps, Some(8.0));
        c.validate().unwrap();
    }

    #[test]
    fn only_the_most_recent_networks_are_kept() {
        let mut c = with_speed(25.0);
        for i in 0..(MAX_SAVED_NETWORKS as u64 + 5) {
            c.switch_network(net(&format!("n{i}"), "x"), i);
        }
        assert_eq!(c.networks.len(), MAX_SAVED_NETWORKS);
        assert!(c.networks.iter().all(|n| n.id != "n0"), "the oldest went");
    }

    #[test]
    fn networks_round_trip_through_toml() {
        let mut c = with_speed(25.0);
        c.switch_network(net("a", "Home"), 1);
        c.switch_network(net("b", "Phone"), 2);
        let back = Config::parse(&c.to_toml()).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn a_saved_line_out_of_range_is_refused() {
        let mut c = with_speed(25.0);
        c.switch_network(net("a", "Home"), 1);
        c.switch_network(net("b", "Phone"), 2);
        c.networks[0].line.down_mbps = Some(0.0);
        assert!(c.validate().is_err());
    }
}
