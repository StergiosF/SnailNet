//! `snailnet`: developer and diagnostic CLI.
//!
//! - Talks to the running service over IPC (PLAN.md §12): `status`,
//!   `apps`, `rules`, `profile`, `line`, `top`. No admin rights needed.
//! - Phase 0 tools that drive WinDivert themselves (need Administrator,
//!   and shouldn't run while the service is shaping): `check`, `spike`.

mod check;
mod ipc_cmd;
mod rules;
mod spike;

use clap::{Parser, Subcommand};
use snailnet_core::rules::Rule;

#[derive(Parser)]
#[command(name = "snailnet", version, about = "SnailNet developer CLI")]
struct Cli {
    /// The service's pipe (for testing against `snailnet-service run
    /// --console --pipe NAME`).
    #[arg(long, global = true, hide = true, default_value = snailnet_ipc::PIPE_NAME)]
    pipe: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// What the service is doing, the line speed, and every profile's rules
    /// with the shares they actually get.
    Status,
    /// Apps the service has seen (their family ids are what rules name).
    Apps,
    /// Show or replace a profile's rules.
    ///
    /// `snailnet rules steam=40:low brave=60` replaces the active
    /// profile's rules with these two. TARGET=PERCENT[:high|normal|low];
    /// TARGET is a family id (`steam`), an exe (`game.exe`), or
    /// `family:ID` / `exe:NAME` / `site:ID` (`site:youtube`, in any app).
    /// "Everything else" gets the rest (at least
    /// 5%; if the rules ask for more, they all shrink to fit).
    Rules {
        /// Which profile (default: the active one).
        #[arg(long)]
        profile: Option<String>,
        /// Remove every rule from the profile.
        #[arg(long, conflicts_with = "rules")]
        clear: bool,
        #[arg(value_name = "TARGET=PERCENT[:PRIO]", value_parser = ipc_cmd::parse_target_rule)]
        rules: Vec<Rule>,
    },
    /// List profiles, or switch to one.
    Profile { id: Option<String> },
    /// Show or set the line speed. Shaping starts once a speed is set.
    Line {
        /// Download speed in Mbps.
        #[arg(long)]
        down: Option<f64>,
        /// Upload speed in Mbps.
        #[arg(long)]
        up: Option<f64>,
        /// Shape at this fraction of the line speed (0.5–1.0), overriding
        /// low-latency mode's defaults.
        #[arg(long)]
        factor: Option<f64>,
        /// Forget the line speed (stops shaping).
        #[arg(long, conflicts_with_all = ["down", "up", "factor"])]
        clear: bool,
    },
    /// Measure the line against Cloudflare (~20 s, shaping off meanwhile)
    /// and save the result as the line speed.
    Speedtest,
    /// Live per-class rates. Ctrl+C to quit.
    Top {
        /// Updates per second (1–10).
        #[arg(long, default_value_t = 2)]
        hz: u32,
        /// Stop after this many updates.
        #[arg(long)]
        count: Option<u64>,
    },
    /// Check that the WinDivert driver loads here (HVCI, signatures, other
    /// WinDivert users) and that connections map to apps. Sniff-only: never
    /// delays or changes traffic. Needs Administrator.
    Check {
        /// How long to watch for new connections.
        #[arg(long, default_value_t = 10)]
        seconds: u64,
        /// Print at most this many connections.
        #[arg(long, default_value_t = 25)]
        max: usize,
    },
    /// Shape traffic between apps by percentage, without the service.
    /// Needs Administrator. Ctrl+C stops it and the network returns to
    /// normal immediately.
    Spike(spike::SpikeArgs),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let pipe = cli.pipe.as_str();
    match cli.command {
        Command::Status => ipc_cmd::status(pipe),
        Command::Apps => ipc_cmd::apps(pipe),
        Command::Rules {
            profile,
            clear,
            rules,
        } => ipc_cmd::rules(pipe, profile, rules, clear),
        Command::Profile { id } => ipc_cmd::profile(pipe, id),
        Command::Line {
            down,
            up,
            factor,
            clear,
        } => ipc_cmd::line(
            pipe,
            ipc_cmd::LineArgs {
                down,
                up,
                factor,
                clear,
            },
        ),
        Command::Speedtest => ipc_cmd::speedtest(pipe),
        Command::Top { hz, count } => ipc_cmd::top(pipe, hz, count),
        Command::Check { seconds, max } => check::run(seconds, max),
        Command::Spike(args) => spike::run(args),
    }
}
