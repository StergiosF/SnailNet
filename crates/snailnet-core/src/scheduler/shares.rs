//! Share arithmetic in basis points (PLAN.md §9.7), the contract between the
//! UI's split bar and the scheduler.
//!
//! A share list has one entry per rule, in order, then "Everything else"
//! last. It always sums to exactly [`TOTAL_BP`], and "Everything else" never
//! has less than [`OTHER_FLOOR_BP`].

/// 100% in basis points.
pub const TOTAL_BP: u32 = 10_000;
/// "Everything else" holds DNS, Windows and new apps; starving it breaks
/// things, so it never gets less than 5%.
pub const OTHER_FLOOR_BP: u32 = 500;

/// Turn requested rule shares into a valid share list (rules, then
/// "Everything else"). Rules keep what they asked for when it fits; when they
/// ask for more than 95% they're scaled down proportionally so "Everything
/// else" keeps its floor.
pub fn normalize(rules: &[u32]) -> Vec<u32> {
    let budget = (TOTAL_BP - OTHER_FLOOR_BP) as u64;
    let asked: u64 = rules.iter().map(|&r| r as u64).sum();
    let mut out: Vec<u32> = if asked <= budget {
        rules.to_vec()
    } else {
        let weights: Vec<u64> = rules.iter().map(|&r| r as u64).collect();
        apportion(budget, &weights)
    };
    let used: u32 = out.iter().sum();
    out.push(TOTAL_BP - used);
    out
}

/// Remove rule `index` from a valid share list, giving its share back to
/// every remaining entry (including "Everything else") in proportion to what
/// each already has.
///
/// # Panics
/// If `index` is "Everything else" (the last entry) or out of range.
pub fn remove(shares: &[u32], index: usize) -> Vec<u32> {
    assert!(
        index + 1 < shares.len(),
        "can only remove a rule, not Everything else"
    );
    let freed = shares[index] as u64;
    let mut rest: Vec<u32> = shares
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != index)
        .map(|(_, &s)| s)
        .collect();
    let weights: Vec<u64> = rest.iter().map(|&s| s as u64).collect();
    // "Everything else" is ≥ 5%, so the weights never sum to zero.
    for (s, extra) in rest.iter_mut().zip(apportion(freed, &weights)) {
        *s += extra;
    }
    rest
}

/// True when `shares` is a valid share list.
pub fn is_valid(shares: &[u32]) -> bool {
    let sum: u64 = shares.iter().map(|&s| s as u64).sum();
    sum == TOTAL_BP as u64 && shares.last().is_some_and(|&o| o >= OTHER_FLOOR_BP)
}

/// Split `amount` into integer parts proportional to `weights` (largest
/// remainder method, ties to the lower index). Parts sum to exactly `amount`.
/// All-zero weights get nothing, so callers must not pass them with a
/// non-zero amount.
fn apportion(amount: u64, weights: &[u64]) -> Vec<u32> {
    let total: u64 = weights.iter().sum();
    if total == 0 {
        return vec![0; weights.len()];
    }
    let mut parts: Vec<u64> = weights.iter().map(|&w| amount * w / total).collect();
    let mut left = amount - parts.iter().sum::<u64>();
    let mut order: Vec<usize> = (0..weights.len()).collect();
    // Remainder of i is (amount*w_i) mod total; biggest first.
    order.sort_by_key(|&i| std::cmp::Reverse((amount * weights[i]) % total));
    for &i in &order {
        if left == 0 {
            break;
        }
        parts[i] += 1;
        left -= 1;
    }
    parts.into_iter().map(|p| p as u32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn fits_unchanged() {
        assert_eq!(normalize(&[4000, 4500]), [4000, 4500, 1500]);
        assert_eq!(normalize(&[]), [10_000]);
        assert_eq!(normalize(&[0, 9500]), [0, 9500, 500]);
    }

    #[test]
    fn overcommit_scales_to_floor() {
        // The Phase 0 40/60 example: Everything else keeps exactly 5%.
        assert_eq!(normalize(&[4000, 6000]), [3800, 5700, 500]);
    }

    #[test]
    fn remove_gives_share_back_proportionally() {
        // 40/45/15, drop the 45: 40 and 15 grow in a 40:15 ratio.
        let r = remove(&[4000, 4500, 1500], 1);
        assert_eq!(r, [7273, 2727]);
    }

    #[test]
    #[should_panic(expected = "Everything else")]
    fn cannot_remove_everything_else() {
        remove(&[5000, 5000], 1);
    }

    proptest! {
        #[test]
        fn normalize_always_valid(rules in prop::collection::vec(0u32..20_000, 0..12)) {
            let s = normalize(&rules);
            prop_assert!(is_valid(&s));
            prop_assert_eq!(s.len(), rules.len() + 1);
            for (got, asked) in s.iter().zip(&rules) {
                prop_assert!(got <= asked, "a rule grew: {} > {}", got, asked);
            }
        }

        #[test]
        fn removals_stay_valid(
            rules in prop::collection::vec(0u32..5_000, 1..10),
            picks in prop::collection::vec(any::<prop::sample::Index>(), 0..10),
        ) {
            let mut s = normalize(&rules);
            for pick in picks {
                if s.len() < 2 {
                    break;
                }
                let i = pick.index(s.len() - 1);
                let before = s.clone();
                s = remove(&s, i);
                prop_assert!(is_valid(&s), "{:?} → {:?}", before, s);
                let kept: Vec<u32> = before.iter().enumerate()
                    .filter(|&(j, _)| j != i).map(|(_, &v)| v).collect();
                for (now, was) in s.iter().zip(&kept) {
                    prop_assert!(now >= was, "a share shrank on removal");
                }
            }
        }
    }
}
