//! `--app steam=40[:low]` rules → scheduler classes.
//!
//! A rule names apps by family id (`steam`: Steam and all its helpers, from
//! `assets/families.toml`) or by exe (`steamwebhelper.exe`, or just
//! `steamwebhelper`).

use snailnet_core::classify::App;
use snailnet_core::scheduler::{ClassConfig, Priority, shares};

pub const OTHER_NAME: &str = "everything else";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppRule {
    /// Lowercased family ids or exe names, e.g. `["steam", "game.exe"]`.
    pub names: Vec<String>,
    pub percent: u32,
    pub prio: Priority,
}

impl AppRule {
    pub fn label(&self) -> String {
        self.names.join("+")
    }

    pub fn matches(&self, app: &App) -> bool {
        self.names.iter().any(|n| names(n, app))
    }
}

/// Does rule name `n` cover `app`? Family id, exe, or exe without `.exe`.
fn names(n: &str, app: &App) -> bool {
    n == app.family
        || (!app.exe.is_empty() && (n == app.exe || app.exe.strip_suffix(".exe") == Some(n)))
}

/// clap value parser for `--app`: `APP[,APP…]=PERCENT[:high|normal|low]`.
pub fn parse_rule(s: &str) -> Result<AppRule, String> {
    let (names, rest) = s
        .rsplit_once('=')
        .ok_or_else(|| format!("expected APP=PERCENT, got {s:?}"))?;
    let (pct, prio) = match rest.split_once(':') {
        None => (rest, Priority::Normal),
        Some((pct, p)) => (pct, parse_prio(p)?),
    };
    let percent: u32 = pct
        .trim()
        .trim_end_matches('%')
        .parse()
        .map_err(|_| format!("{pct:?} is not a whole percentage"))?;
    if !(1..=95).contains(&percent) {
        return Err(format!("share must be 1–95%, got {percent}%"));
    }
    let names: Vec<String> = names
        .split(',')
        .map(|n| n.trim().to_lowercase())
        .filter(|n| !n.is_empty())
        .collect();
    if names.is_empty() {
        return Err(format!("no app names in {s:?}"));
    }
    Ok(AppRule {
        names,
        percent,
        prio,
    })
}

fn parse_prio(s: &str) -> Result<Priority, String> {
    match s.trim().to_lowercase().as_str() {
        "high" => Ok(Priority::High),
        "normal" => Ok(Priority::Normal),
        "low" => Ok(Priority::Low),
        other => Err(format!(
            "priority must be high, normal or low, got {other:?}"
        )),
    }
}

/// One class per rule, in order, then "everything else" last. If the rules
/// leave Everything else less than its 5% floor, all rules shrink
/// proportionally (PLAN.md §9.7).
pub fn classes(rules: &[AppRule]) -> Result<Vec<ClassConfig>, String> {
    let sum: u32 = rules.iter().map(|r| r.percent).sum();
    if sum > 100 {
        return Err(format!("shares add up to {sum}%, more than 100%"));
    }
    for (i, r) in rules.iter().enumerate() {
        for n in &r.names {
            if rules[..i].iter().any(|p| p.names.contains(n)) {
                return Err(format!("{n} appears in more than one rule"));
            }
        }
    }
    let asked: Vec<u32> = rules.iter().map(|r| r.percent * 100).collect();
    let bp = shares::normalize(&asked);
    let mut out: Vec<ClassConfig> = rules
        .iter()
        .zip(&bp)
        .map(|(r, &share_bp)| ClassConfig {
            prio: r.prio,
            ..ClassConfig::new(r.label(), share_bp)
        })
        .collect();
    out.push(ClassConfig::new(OTHER_NAME, bp[rules.len()]));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rules() {
        assert_eq!(
            parse_rule("Steam.exe, steamwebhelper=40%").unwrap(),
            AppRule {
                names: vec!["steam.exe".into(), "steamwebhelper".into()],
                percent: 40,
                prio: Priority::Normal,
            }
        );
        assert_eq!(parse_rule("game=10:High").unwrap().prio, Priority::High);
        assert_eq!(parse_rule("steam=40%:low").unwrap().prio, Priority::Low);
        assert!(parse_rule("game=10:urgent").is_err());
        assert!(parse_rule("steam.exe").is_err());
        assert!(parse_rule("steam.exe=0").is_err());
        assert!(parse_rule("steam.exe=96").is_err());
        assert!(parse_rule("=40").is_err());
        assert!(parse_rule("steam.exe=4.5").is_err());
    }

    #[test]
    fn names_match_families_and_exes() {
        let app = |family: &str, exe: &str| App {
            family: family.into(),
            name: String::new(),
            exe: exe.into(),
            path: None,
            how: snailnet_core::classify::How::Pack,
        };
        let helper = app("steam", "steamwebhelper.exe");
        assert!(parse_rule("steam=40").unwrap().matches(&helper));
        assert!(parse_rule("steamwebhelper=40").unwrap().matches(&helper));
        assert!(
            parse_rule("x, steamwebhelper.exe=40")
                .unwrap()
                .matches(&helper)
        );
        assert!(!parse_rule("steam.exe=40").unwrap().matches(&helper));
        assert!(!parse_rule("brave=40").unwrap().matches(&helper));
        // System has no exe: an empty name can't sneak in.
        assert!(!parse_rule(".exe=40").unwrap().matches(&app("system", "")));
        assert!(parse_rule("system=10").unwrap().matches(&app("system", "")));
    }

    #[test]
    fn other_gets_the_remainder() {
        let r = [parse_rule("a=40").unwrap(), parse_rule("b=45").unwrap()];
        let c = classes(&r).unwrap();
        assert_eq!(
            c.iter().map(|c| c.share_bp).collect::<Vec<_>>(),
            [4000, 4500, 1500]
        );
        assert_eq!(c[2].name, OTHER_NAME);
    }

    #[test]
    fn other_floor_scales_everyone_down() {
        // The plan's 40/60 spike example leaves 0% → floor 5%.
        let r = [
            parse_rule("steam=40:low").unwrap(),
            parse_rule("brave=60").unwrap(),
        ];
        let c = classes(&r).unwrap();
        assert_eq!(
            c.iter().map(|c| c.share_bp).collect::<Vec<_>>(),
            [3800, 5700, 500]
        );
        assert_eq!(c[0].prio, Priority::Low);
    }

    #[test]
    fn rejects_overcommit_and_duplicates() {
        let r = [parse_rule("a=60").unwrap(), parse_rule("b=50").unwrap()];
        assert!(classes(&r).is_err());
        let r = [parse_rule("a=10").unwrap(), parse_rule("b,a=10").unwrap()];
        assert!(classes(&r).is_err());
    }
}
