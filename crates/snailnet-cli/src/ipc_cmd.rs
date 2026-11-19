//! Commands that talk to the running service over IPC (PLAN.md §12):
//! `status`, `apps`, `rules`, `profile`, `line`, `top`. They need no admin
//! rights: the pipe lets interactive users in.

use anyhow::{Context, Result, bail};
use snailnet_core::rules::{Config, Line, OTHER_CLASS, Profile, Rule, Target};
use snailnet_core::scheduler::{Priority, shares};
use snailnet_ipc::{
    Client, FamilyTick, NameTick, Reply, Request, ServerMsg, ServiceState, SiteTick, Snapshot,
    StatsTick,
};

fn connect(pipe: &str) -> Result<Client> {
    Ok(Client::connect(pipe, env!("CARGO_PKG_VERSION"))?)
}

fn snapshot(c: &mut Client) -> Result<Snapshot> {
    match c.call(Request::GetSnapshot)? {
        Reply::Snapshot(s) => Ok(*s),
        r => bail!("unexpected reply: {r:?}"),
    }
}

fn state_text(s: &ServiceState) -> String {
    match s {
        ServiceState::Starting => "starting".into(),
        ServiceState::Shaping => "shaping".into(),
        ServiceState::Unshaped => {
            "not shaping (no line speed set; try `snailnet line --down <Mbps>`)".into()
        }
        ServiceState::Retrying { reason } => format!("retrying after a fail-open ({reason})"),
        ServiceState::SafeMode { failures } => format!(
            "Safe mode: {failures} crashes or fail-opens recently; not shaping until the service restarts"
        ),
        ServiceState::Paused { until: Some(u) } => {
            format!("paused until {u} (Unix seconds): not shaping or blocking")
        }
        ServiceState::Paused { until: None } => {
            "paused until resumed: not shaping or blocking".into()
        }
        ServiceState::Measuring => "measuring the line speed (shaping off for ~20 s)".into(),
    }
}

fn mbps(v: Option<f64>) -> String {
    v.map_or("not set".into(), |m| format!("{m} Mbps"))
}

fn prio_text(p: Priority) -> &'static str {
    match p {
        Priority::High => "high",
        Priority::Normal => "normal",
        Priority::Low => "low",
    }
}

/// A profile's rules with the shares the scheduler will actually use.
fn print_profile(p: &Profile, active: bool) {
    println!(
        "Profile {:?} ({}){}",
        p.name,
        p.id,
        if active { ", active" } else { "" }
    );
    let enabled: Vec<&Rule> = p.rules.iter().filter(|r| r.enabled).collect();
    let bp = shares::normalize(&enabled.iter().map(|r| r.share).collect::<Vec<_>>());
    let mut k = 0;
    for r in &p.rules {
        let mut line = format!(
            "  {:<28} asked {:>5.1}%",
            r.target.to_string(),
            r.share as f64 / 100.0
        );
        if r.enabled {
            line += &format!("  gets {:>5.1}%", bp[k] as f64 / 100.0);
            k += 1;
        } else {
            line += "  (off)";
        }
        line += &format!("  {}", prio_text(r.priority));
        if let Some(c) = &r.ceiling {
            for (dir, l) in [("down", c.down), ("up", c.up)] {
                if let Some(l) = l {
                    line += &format!("  max {dir} {}", String::from(l));
                }
            }
        }
        println!("{line}");
    }
    println!(
        "  {:<28}              gets {:>5.1}%",
        OTHER_CLASS,
        bp[k] as f64 / 100.0
    );
}

fn print_config(c: &Config) {
    println!(
        "Line: down {}, up {}{}; low-latency mode {}",
        mbps(c.line.down_mbps),
        mbps(c.line.up_mbps),
        c.line
            .factor
            .map_or(String::new(), |f| format!(", factor {f}")),
        if c.settings.low_latency { "on" } else { "off" },
    );
    for p in &c.profiles {
        print_profile(p, p.id == c.active_profile);
    }
}

pub fn status(pipe: &str) -> Result<()> {
    let mut c = connect(pipe)?;
    let s = snapshot(&mut c)?;
    println!(
        "SnailNet service {}: {}",
        c.service_version,
        state_text(&s.state)
    );
    println!("Settings revision {}; {} apps seen", s.rev, s.apps.len());
    print_config(&s.config);
    Ok(())
}

pub fn apps(pipe: &str) -> Result<()> {
    let mut c = connect(pipe)?;
    let s = snapshot(&mut c)?;
    if s.apps.is_empty() {
        println!("No apps seen yet (apps are tracked while shaping).");
    }
    let mut apps = s.apps;
    apps.sort_by(|a, b| a.family.cmp(&b.family).then(a.exe.cmp(&b.exe)));
    for a in apps {
        let exe = if a.exe.is_empty() { "-" } else { &a.exe };
        println!("{:<24} {:<32} {}", a.family, exe, a.name);
    }
    Ok(())
}

/// clap value parser: `TARGET=PERCENT[:high|normal|low]`. TARGET is
/// `family:ID`, `exe:NAME`, `site:ID`, or a bare name (an exe if it ends in `.exe`,
/// else a family).
pub fn parse_target_rule(s: &str) -> Result<Rule, String> {
    let (target, rest) = s
        .rsplit_once('=')
        .ok_or_else(|| format!("expected TARGET=PERCENT, got {s:?}"))?;
    let (pct, prio) = match rest.split_once(':') {
        None => (rest, Priority::Normal),
        Some((pct, p)) => (
            pct,
            match p.trim().to_lowercase().as_str() {
                "high" => Priority::High,
                "normal" => Priority::Normal,
                "low" => Priority::Low,
                other => {
                    return Err(format!(
                        "priority must be high, normal or low, got {other:?}"
                    ));
                }
            },
        ),
    };
    let pct: f64 = pct
        .trim()
        .trim_end_matches('%')
        .parse()
        .map_err(|_| format!("{pct:?} is not a percentage"))?;
    if !(0.0..=95.0).contains(&pct) {
        return Err(format!("share must be 0–95%, got {pct}%"));
    }
    let t = target.trim().to_lowercase();
    let target = match t.split_once(':') {
        Some(("family", id)) => Target::family(id.trim()),
        Some(("exe", id)) => Target::exe(id.trim()),
        Some(("site", id)) => Target::site(id.trim()),
        Some((kind, _)) => {
            return Err(format!(
                "unknown target kind {kind:?} (family, exe or site)"
            ));
        }
        None if t.ends_with(".exe") => Target::exe(&t),
        None => Target::family(&t),
    };
    if target.id.is_empty() {
        return Err(format!("no app named in {s:?}"));
    }
    Ok(Rule {
        target,
        share: (pct * 100.0).round() as u32,
        priority: prio,
        ceiling: None,
        enabled: true,
    })
}

pub fn rules(pipe: &str, profile: Option<String>, specs: Vec<Rule>, clear: bool) -> Result<()> {
    let mut c = connect(pipe)?;
    let s = snapshot(&mut c)?;
    let id = profile.unwrap_or_else(|| s.config.active_profile.clone());
    let Some(p) = s.config.profiles.iter().find(|p| p.id == id) else {
        bail!("there is no profile {id:?}");
    };
    if specs.is_empty() && !clear {
        print_profile(p, id == s.config.active_profile);
        return Ok(());
    }
    let rev = match c.call(Request::SetRules {
        profile_id: id.clone(),
        rules: specs,
        rev: s.rev,
    })? {
        Reply::Ack { rev } => rev,
        r => bail!("unexpected reply: {r:?}"),
    };
    let s = snapshot(&mut c)?;
    println!("Saved (revision {rev}).");
    if let Some(p) = s.config.profiles.iter().find(|p| p.id == id) {
        print_profile(p, id == s.config.active_profile);
    }
    Ok(())
}

pub fn profile(pipe: &str, id: Option<String>) -> Result<()> {
    let mut c = connect(pipe)?;
    let s = snapshot(&mut c)?;
    let Some(id) = id else {
        for p in &s.config.profiles {
            let mark = if p.id == s.config.active_profile {
                "*"
            } else {
                " "
            };
            println!("{mark} {:<16} {} ({} rules)", p.id, p.name, p.rules.len());
        }
        return Ok(());
    };
    c.call(Request::SwitchProfile {
        id: id.clone(),
        rev: s.rev,
    })?;
    println!("Switched to {id}.");
    Ok(())
}

/// Ask for a speed test and wait for its result.
pub fn speedtest(pipe: &str) -> Result<()> {
    let mut c = connect(pipe)?;
    let before = snapshot(&mut c)?.config.line.measured.map(|m| m.at);
    c.call(Request::RunSpeedTest)
        .context("asking for a speed test")?;
    println!("Measuring against Cloudflare (~20 s; shaping is off meanwhile)…");
    let started = std::time::Instant::now();
    let mut seen_measuring = false;
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        let s = snapshot(&mut c)?;
        let measuring = s.state == ServiceState::Measuring;
        seen_measuring |= measuring;
        let done = seen_measuring && !measuring;
        if done || started.elapsed() > std::time::Duration::from_secs(60) {
            match s.config.line.measured {
                Some(m) if Some(m.at) != before => {
                    println!("Down {} Mbps · up {} Mbps", m.down_mbps, m.up_mbps);
                    if let (Some(i), Some(l)) = (m.idle_ms, m.loaded_ms) {
                        println!("Ping {i} ms idle, {l} ms while downloading (without SnailNet)");
                    }
                    println!("Saved as the line speed.");
                    return Ok(());
                }
                _ => bail!("the speed test didn't finish; see the service log"),
            }
        }
    }
}

pub struct LineArgs {
    pub down: Option<f64>,
    pub up: Option<f64>,
    pub factor: Option<f64>,
    pub clear: bool,
}

pub fn line(pipe: &str, a: LineArgs) -> Result<()> {
    let mut c = connect(pipe)?;
    let s = snapshot(&mut c)?;
    if a.down.is_none() && a.up.is_none() && a.factor.is_none() && !a.clear {
        let l = &s.config.line;
        println!("down {}, up {}", mbps(l.down_mbps), mbps(l.up_mbps));
        if let Some(f) = l.factor {
            println!("factor {f}");
        }
        return Ok(());
    }
    let mut line = if a.clear {
        Line::default()
    } else {
        s.config.line.clone()
    };
    line.down_mbps = a.down.or(line.down_mbps);
    line.up_mbps = a.up.or(line.up_mbps);
    line.factor = a.factor.or(line.factor);
    c.call(Request::SetLine {
        line: line.clone(),
        rev: s.rev,
    })
    .context("setting the line speed")?;
    println!(
        "Line: down {}, up {}.",
        mbps(line.down_mbps),
        mbps(line.up_mbps)
    );
    Ok(())
}

fn print_tick(t: &StatsTick) {
    let secs = t.t_ms as f64 / 1000.0;
    if t.down.is_none() && t.up.is_none() {
        println!("{secs:>8.1}s  {}", state_text(&t.state));
        return;
    }
    for (arrow, d) in [("↓", &t.down), ("↑", &t.up)] {
        let Some(d) = d else { continue };
        let total: u64 = d.classes.iter().map(|c| c.bps).sum();
        let drops: u64 = d.classes.iter().map(|c| c.drops).sum();
        let mut line = format!(
            "{secs:>8.1}s {arrow} {:>6.2}/{:.1} Mbps",
            total as f64 / 1e6,
            d.shaped_bps as f64 / 1e6
        );
        for c in &d.classes {
            let pct = if total > 0 {
                c.bps as f64 * 100.0 / total as f64
            } else {
                0.0
            };
            line += &format!(" | {} {:.2} ({pct:.0}%)", c.name, c.bps as f64 / 1e6);
        }
        line += &format!(" | drops {drops}");
        println!("{line}");
        if let Some(apps) = family_line(&d.families) {
            println!("{:>12}{apps}", "");
        }
        if let Some(sites) = site_line(&d.sites) {
            println!("{:>12}sites: {sites}", "");
        }
    }
    if let Some(names) = t.names.as_ref().and_then(name_line) {
        println!("{:>12}{names}", "");
    }
    if !t.calls.is_empty() {
        println!("{:>12}calls: {}", "", t.calls.join(", "));
    }
}

/// How the tick's packets got their site names, when any were named:
/// `names: sni 900, dns 40, unidentified 15 (ech 12, none 3), naming 8`.
fn name_line(n: &NameTick) -> Option<String> {
    if *n == NameTick::default() {
        return None;
    }
    Some(format!(
        "names: sni {}, dns {}, unidentified {} (ech {}, none {}), naming {}",
        n.sni,
        n.dns,
        n.unidentified(),
        n.ech,
        n.none,
        n.naming
    ))
}

/// The busiest sites, each under its app: `YouTube (brave) 7.00, ...`.
fn site_line(sites: &[SiteTick]) -> Option<String> {
    const SHOWN: usize = 5;
    if sites.is_empty() {
        return None;
    }
    let mut line = sites
        .iter()
        .take(SHOWN)
        .map(|s| format!("{} ({}) {:.2}", s.name, s.family, s.bps as f64 / 1e6))
        .collect::<Vec<_>>()
        .join(", ");
    if sites.len() > SHOWN {
        line += &format!(", +{} more", sites.len() - SHOWN);
    }
    Some(line)
}

/// The busiest families on one line: `Brave 8.00, Steam 6.00, +2 more`.
fn family_line(families: &[FamilyTick]) -> Option<String> {
    const SHOWN: usize = 5;
    if families.is_empty() {
        return None;
    }
    let mut line = families
        .iter()
        .take(SHOWN)
        .map(|f| {
            let mut s = format!("{} {:.2}", f.name, f.bps as f64 / 1e6);
            if f.unidentified_bps > 0 {
                s += &format!(" ({:.2} unidentified)", f.unidentified_bps as f64 / 1e6);
            }
            s
        })
        .collect::<Vec<_>>()
        .join(", ");
    if families.len() > SHOWN {
        line += &format!(", +{} more", families.len() - SHOWN);
    }
    Some(line)
}

pub fn top(pipe: &str, hz: u32, count: Option<u64>) -> Result<()> {
    let mut c = connect(pipe)?;
    c.call(Request::Subscribe { stats_hz: hz })?;
    let mut n = 0;
    loop {
        match c.recv()? {
            ServerMsg::Stats(t) => {
                print_tick(&t);
                n += 1;
                if count.is_some_and(|k| n >= k) {
                    return Ok(());
                }
            }
            ServerMsg::Event(e) => println!("          event: {e:?}"),
            ServerMsg::Reply { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use snailnet_core::rules::TargetKind;

    use super::*;

    #[test]
    fn parses_target_rules() {
        let r = parse_target_rule("Steam=40:low").unwrap();
        assert_eq!(r.target, Target::family("steam"));
        assert_eq!((r.share, r.priority), (4000, Priority::Low));
        let r = parse_target_rule("game.exe=12.5%").unwrap();
        assert_eq!((r.target.kind, r.share), (TargetKind::Exe, 1250));
        let r = parse_target_rule("exe:helper=0").unwrap();
        assert_eq!(r.target, Target::exe("helper"));
        let r = parse_target_rule("site:YouTube=45").unwrap();
        assert_eq!(r.target, Target::site("youtube"));
        assert!(parse_target_rule("web:youtube=10").is_err());
        assert!(parse_target_rule("steam").is_err());
        assert!(parse_target_rule("steam=96").is_err());
        assert!(parse_target_rule("steam=10:urgent").is_err());
        assert!(parse_target_rule("family:=10").is_err());
    }

    #[test]
    fn family_line_shows_the_busiest_five() {
        assert_eq!(family_line(&[]), None);
        let f = |name: &str, mbps: u64| FamilyTick {
            family: name.to_lowercase(),
            name: name.into(),
            bps: mbps * 1_000_000,
            unidentified_bps: 0,
        };
        let mut list: Vec<_> = (0..7).map(|k| f(&format!("App{k}"), 7 - k)).collect();
        assert_eq!(
            family_line(&list).unwrap(),
            "App0 7.00, App1 6.00, App2 5.00, App3 4.00, App4 3.00, +2 more"
        );
        list[1].unidentified_bps = 250_000;
        assert!(
            family_line(&list)
                .unwrap()
                .starts_with("App0 7.00, App1 6.00 (0.25 unidentified), App2")
        );
    }

    #[test]
    fn site_and_name_lines() {
        assert_eq!(site_line(&[]), None);
        let s = |site: &str, family: &str, mbps: u64| SiteTick {
            site: site.to_lowercase(),
            name: site.into(),
            family: family.into(),
            bps: mbps * 1_000_000,
        };
        assert_eq!(
            site_line(&[s("YouTube", "brave", 7), s("YouTube", "discord", 1)]).unwrap(),
            "YouTube (brave) 7.00, YouTube (discord) 1.00"
        );
        let many: Vec<_> = (0..6).map(|k| s("S", "a", k)).collect();
        assert!(site_line(&many).unwrap().ends_with(", +1 more"));

        assert_eq!(name_line(&NameTick::default()), None);
        let n = NameTick {
            sni: 900,
            dns: 40,
            ech: 12,
            none: 3,
            naming: 8,
        };
        assert_eq!(
            name_line(&n).unwrap(),
            "names: sni 900, dns 40, unidentified 15 (ech 12, none 3), naming 8"
        );
    }
}
