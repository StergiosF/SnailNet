//! `snailnet`: developer and diagnostic CLI. In Phase 0 it hosts the spike
//! that proves download shaping works before any UI exists (PLAN.md §21).

mod attrib;
mod check;
mod rules;
mod spike;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "snailnet", version, about = "SnailNet developer CLI (Phase 0)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
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
    /// Shape traffic between apps by percentage. Needs Administrator.
    /// Ctrl+C stops it and the network returns to normal immediately.
    Spike(spike::SpikeArgs),
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Check { seconds, max } => check::run(seconds, max),
        Command::Spike(args) => spike::run(args),
    }
}
