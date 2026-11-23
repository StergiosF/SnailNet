//! The Windows service entry point (PLAN.md D16, §17.1).
//!
//! The service manager starts `snailnet-service.exe service`; `main` hands
//! the thread to the dispatcher, which calls [`service_main`] on a new
//! thread. Stop and Shutdown both request a stop; the runner stops shaping
//! (flushing what it holds) before we report Stopped.

use std::ffi::OsString;
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::{error, info};
use windows_service::service::{
    PowerEventParam, ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState,
    ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::{define_windows_service, service_dispatcher};

use crate::hub::Hub;
use crate::ipc::IpcServer;
use crate::logging;
use crate::paths::DataDir;
use crate::runner::{self, StopSignal, Timing};
use crate::shaper::WinEngine;

pub const SERVICE_NAME: &str = "SnailNet";
pub const DISPLAY_NAME: &str = "SnailNet";
pub const DESCRIPTION: &str = "Shares your internet connection between apps by percentage. \
     Stopping this service returns the network to normal.";
const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;
/// Exit code reported when the runner fails, so recovery actions restart us.
const EXIT_FAILED: u32 = 1;

define_windows_service!(ffi_service_main, service_main);

/// Hand this thread to the service manager. Fails when not started by it.
pub fn dispatch() -> windows_service::Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
}

fn service_main(_args: Vec<OsString>) {
    if let Err(e) = run_service() {
        error!("service failed: {e:#}");
    }
}

fn status(
    state: ServiceState,
    accept: ServiceControlAccept,
    exit: u32,
    wait: u64,
) -> ServiceStatus {
    ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: state,
        controls_accepted: accept,
        exit_code: if exit == 0 {
            ServiceExitCode::Win32(0)
        } else {
            ServiceExitCode::ServiceSpecific(exit)
        },
        checkpoint: 0,
        wait_hint: Duration::from_secs(wait),
        process_id: None,
    }
}

fn run_service() -> Result<()> {
    let dir = DataDir::system();
    let hub = Hub::new(dir.clone(), StopSignal::new());
    let handler_hub = hub.clone();
    let handle = service_control_handler::register(SERVICE_NAME, move |control| match control {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            handler_hub.signal().request();
            ServiceControlHandlerResult::NoError
        }
        // Sent on every resume from sleep. Under Modern Standby this
        // service keeps running, so netwatch may see no clock jump; this
        // is the signal that doesn't depend on one.
        ServiceControl::PowerEvent(PowerEventParam::ResumeAutomatic) => {
            info!("power: the PC woke up; restarting shaping");
            handler_hub.route_changed();
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::PowerEvent(_) => ServiceControlHandlerResult::NoError,
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    })?;
    handle.set_service_status(status(
        ServiceState::StartPending,
        ServiceControlAccept::empty(),
        0,
        10,
    ))?;

    let result = dir
        .create_protected()
        .with_context(|| format!("creating {}", dir.root().display()))
        .and_then(|()| logging::init(&dir.logs(), false))
        .and_then(|()| {
            logging::abort_on_panic();
            handle.set_service_status(status(
                ServiceState::Running,
                ServiceControlAccept::STOP
                    | ServiceControlAccept::SHUTDOWN
                    | ServiceControlAccept::POWER_EVENT,
                0,
                0,
            ))?;
            // Shaping doesn't need the pipe: if it can't be created, log
            // it and carry on (the app and CLI just can't connect).
            let ipc = IpcServer::start(hub.clone(), snailnet_ipc::PIPE_NAME)
                .inspect_err(|e| error!("{e:#}"))
                .ok();
            let ended = runner::run(
                &hub,
                &mut WinEngine::default(),
                Timing::default(),
                &runner::unix_now,
            );
            if let Some(ipc) = ipc {
                ipc.stop();
            }
            ended
        });
    let exit = match &result {
        Ok(_) => 0,
        Err(e) => {
            error!("{e:#}");
            EXIT_FAILED
        }
    };
    handle.set_service_status(status(
        ServiceState::Stopped,
        ServiceControlAccept::empty(),
        exit,
        0,
    ))?;
    result.map(|_| ())
}
