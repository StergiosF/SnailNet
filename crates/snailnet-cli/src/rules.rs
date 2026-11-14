//! `--app steam.exe,steamwebhelper.exe=40` rules → scheduler classes.

use snailnet_core::scheduler::ClassConfig;

/// "Everything else" never drops below this (PLAN.md §9.7).
pub const OTHER_FLOOR_PERCENT: u32 = 5;
pub const OTHER_NAME: &str = "everything else";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppRule {
    /// Lowercased exe file names, e.g. `["steam.exe", "steamwebhelper.exe"]`.
    pub exes: Vec<String>,
    pub percent: u32,
}

impl AppRule {
    pub fn label(&self) -> String {
        self.exes.join("+")
    }

    pub fn matches(&self, exe: &str) -> bool {
        self.exes.iter().any(|e| e == exe)
    }
}

/// clap value parser for `--app`.
pub fn parse_rule(s: &str) -> Result<AppRule, String> {
    let (names, pct) = s
        .rsplit_once('=')
        .ok_or_else(|| format!("expected EXE=PERCENT, got {s:?}"))?;
    let percent: u32 = pct
        .trim()
        .trim_end_matches('%')
        .parse()
        .map_err(|_| format!("{pct:?} is not a whole percentage"))?;
    if !(1..=95).contains(&percent) {
        return Err(format!("share must be 1–95%, got {percent}%"));
    }
    let exes: Vec<String> = names
        .split(',')
        .map(|n| n.trim().to_lowercase())
        .filter(|n| !n.is_empty())
        .map(|n| {
            if n.ends_with(".exe") {
                n
            } else {
                format!("{n}.exe")
            }
        })
        .collect();
    if exes.is_empty() {
        return Err(format!("no executable names in {s:?}"));
    }
    Ok(AppRule { exes, percent })
}

/// One class per rule, in order, then "everything else" last. Weights are
/// basis points. If the rules leave less than the floor, "everything else"
/// still gets the floor and all shares shrink proportionally (DRR weights are
/// relative, so that falls out naturally).
pub fn classes(
    rules: &[AppRule],
    queue_limit: impl Fn(u32) -> u64,
) -> Result<Vec<ClassConfig>, String> {
    let sum: u32 = rules.iter().map(|r| r.percent).sum();
    if sum > 100 {
        return Err(format!("shares add up to {sum}%, more than 100%"));
    }
    for (i, r) in rules.iter().enumerate() {
        for e in &r.exes {
            if rules[..i].iter().any(|p| p.matches(e)) {
                return Err(format!("{e} appears in more than one rule"));
            }
        }
    }
    let other = (100 - sum).max(OTHER_FLOOR_PERCENT);
    let mut out: Vec<ClassConfig> = rules
        .iter()
        .map(|r| ClassConfig {
            name: r.label(),
            weight: r.percent * 100,
            queue_limit_bytes: queue_limit(r.percent),
        })
        .collect();
    out.push(ClassConfig {
        name: OTHER_NAME.into(),
        weight: other * 100,
        queue_limit_bytes: queue_limit(other),
    });
    Ok(out)
}

/// Effective share of each class in percent after normalisation.
pub fn effective_percent(classes: &[ClassConfig]) -> Vec<f64> {
    let total: u32 = classes.iter().map(|c| c.weight).sum();
    classes
        .iter()
        .map(|c| c.weight as f64 * 100.0 / total as f64)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rules() {
        assert_eq!(
            parse_rule("Steam.exe, steamwebhelper=40%").unwrap(),
            AppRule {
                exes: vec!["steam.exe".into(), "steamwebhelper.exe".into()],
                percent: 40
            }
        );
        assert!(parse_rule("steam.exe").is_err());
        assert!(parse_rule("steam.exe=0").is_err());
        assert!(parse_rule("steam.exe=96").is_err());
        assert!(parse_rule("=40").is_err());
        assert!(parse_rule("steam.exe=4.5").is_err());
    }

    #[test]
    fn other_gets_the_remainder() {
        let r = [parse_rule("a=40").unwrap(), parse_rule("b=45").unwrap()];
        let c = classes(&r, |_| 1).unwrap();
        assert_eq!(
            c.iter().map(|c| c.weight).collect::<Vec<_>>(),
            [4000, 4500, 1500]
        );
        assert_eq!(c[2].name, OTHER_NAME);
    }

    #[test]
    fn other_floor_scales_everyone_down() {
        // The plan's 40/60 spike example leaves 0% → floor 5%.
        let r = [
            parse_rule("steam=40").unwrap(),
            parse_rule("chrome=60").unwrap(),
        ];
        let c = classes(&r, |_| 1).unwrap();
        let p = effective_percent(&c);
        assert!(
            (p[0] - 38.095).abs() < 0.01
                && (p[1] - 57.143).abs() < 0.01
                && (p[2] - 4.762).abs() < 0.01
        );
    }

    #[test]
    fn rejects_overcommit_and_duplicates() {
        let r = [parse_rule("a=60").unwrap(), parse_rule("b=50").unwrap()];
        assert!(classes(&r, |_| 1).is_err());
        let r = [parse_rule("a=10").unwrap(), parse_rule("b,a=10").unwrap()];
        assert!(classes(&r, |_| 1).is_err());
    }
}
