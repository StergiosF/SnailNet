//! Is the internet connection metered? (PLAN.md §10.3.) Windows' own flag
//! (Settings → Network → "Metered connection", or set by the carrier),
//! read with WinRT `ConnectionCost`. Only the app shows it: a note in
//! Settings → Connection, since measuring the line uses its data.

use windows::Networking::Connectivity::{NetworkCostType, NetworkInformation};

/// Whether a connection of this cost should be treated as metered: a
/// fixed or per-use plan, roaming, or already over its limit.
fn is_metered(cost: NetworkCostType, roaming: bool, over_limit: bool) -> bool {
    cost == NetworkCostType::Fixed || cost == NetworkCostType::Variable || roaming || over_limit
}

/// An error when there's no internet connection or Windows won't say.
fn read() -> windows::core::Result<bool> {
    let cost = NetworkInformation::GetInternetConnectionProfile()?.GetConnectionCost()?;
    Ok(is_metered(
        cost.NetworkCostType()?,
        cost.Roaming().unwrap_or(false),
        cost.OverDataLimit().unwrap_or(false),
    ))
}

/// For the page: true if metered; false if not, or unknown.
#[tauri::command]
pub fn network_metered() -> bool {
    read().unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_with_a_limit_are_metered() {
        assert!(!is_metered(NetworkCostType::Unrestricted, false, false));
        assert!(!is_metered(NetworkCostType::Unknown, false, false));
        assert!(is_metered(NetworkCostType::Fixed, false, false));
        assert!(is_metered(NetworkCostType::Variable, false, false));
        assert!(
            is_metered(NetworkCostType::Unrestricted, true, false),
            "roaming"
        );
        assert!(
            is_metered(NetworkCostType::Unrestricted, false, true),
            "over the limit"
        );
    }

    #[test]
    fn reading_it_here_does_not_fail() {
        // Whatever this PC's connection is, asking must not panic.
        let _ = network_metered();
    }
}
