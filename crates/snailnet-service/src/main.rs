//! `snailnet-service`: the SnailNet Windows service (PLAN.md §4, §17).
//!
//! ```text
//! snailnet-service install                  register + start at boot (admin)
//! snailnet-service uninstall [--remove-data]
//! snailnet-service run --console [--data-dir DIR]   run in this terminal (admin)
//! snailnet-service service                  what the service manager runs
//! ```

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use snailnet_service::hub::Hub;
use snailnet_service::ipc::IpcServer;
use snailnet_service::paths::DataDir;
use snailnet_service::runner::{self, Ended, StopSignal, Timing};
use snailnet_service::shaper::WinEngine;
use snailnet_service::{install, logging, svc};

#[derive(Parser)]
#[command(
    name = "snailnet-service",
    version,
    about = "The SnailNet Windows service"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Register the SnailNet service (starts at boot, restarts after a
    /// crash). Running it again updates the registration. Needs Administrator.
    Install,
    /// Stop and remove the SnailNet service. Needs Administrator.
    Uninstall {
        /// Also delete C:\ProgramData\SnailNet (config and logs).
        #[arg(long)]
        remove_data: bool,
    },
    /// Run the service's work in this terminal instead of as a service.
    /// Ctrl+C stops it and the network returns to normal. Needs
    /// Administrator.
    Run {
        /// Required: makes it explicit that this is not the real service.
        #[arg(long)]
        console: bool,
        /// Use this folder for config and logs instead of
        /// C:\ProgramData\SnailNet (for testing).
        #[arg(long, value_name = "DIR")]
        data_dir: Option<PathBuf>,
        /// Receive threads (1 or 2). 2 can reorder packets within a flow.
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=2))]
        rx_threads: u8,
        /// Serve clients on this pipe instead of \.\pipe\snailnet.v1
        /// (for testing next to a running service).
        #[arg(long, value_name = "NAME", default_value = snailnet_ipc::PIPE_NAME)]
        pipe: String,
    },
    /// Entry point for the Windows service manager. Not for interactive use.
    #[command(hide = true)]
    Service,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Install => install::install(),
        Command::Uninstall { remove_data } => install::uninstall(remove_data),
        Command::Run {
            console,
            data_dir,
            rx_threads,
            pipe,
        } => {
            if !console {
                bail!(
                    "use `run --console` to run in this terminal; the service manager starts the real service"
                );
            }
            run_console(data_dir, rx_threads.into(), &pipe)
        }
        // The data folder and logging are set up inside the service, once
        // the service manager has connected, so running this by hand
        // creates nothing.
        Command::Service => svc::dispatch().context(
            "this command is for the Windows service manager; use `run --console` to run in a terminal",
        ),
    }
}

fn run_console(data_dir: Option<PathBuf>, rx_threads: usize, pipe: &str) -> Result<()> {
    let dir = match data_dir {
        Some(d) => {
            let dir = DataDir::new(d);
            dir.create()
                .with_context(|| format!("creating {}", dir.root().display()))?;
            dir
        }
        None => {
            let dir = DataDir::system();
            dir.create_protected().with_context(|| {
                format!(
                    "creating {} (run this as Administrator)",
                    dir.root().display()
                )
            })?;
            dir
        }
    };
    logging::init(&dir.logs(), true)?;
    logging::abort_on_panic();
    let hub = Hub::new(dir.clone(), StopSignal::new());
    let s = hub.signal().clone();
    ctrlc::set_handler(move || s.request()).context("installing the Ctrl+C handler")?;
    let ipc = IpcServer::start(hub.clone(), pipe)?;
    let ended = runner::run(
        &hub,
        &mut WinEngine { rx_threads },
        Timing::default(),
        &runner::unix_now,
    );
    ipc.stop();
    let ended = ended?;
    if ended == Ended::StoppedInSafeMode {
        eprintln!(
            "Stopped (was in Safe mode; see the log in {}).",
            dir.logs().display()
        );
    }
    Ok(())
}
