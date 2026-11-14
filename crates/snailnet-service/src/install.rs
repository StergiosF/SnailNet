//! `install` / `uninstall` (PLAN.md §17.1). Both need Administrator.
//!
//! Install registers the service `SnailNet` to run this exe as LocalSystem,
//! started automatically at boot, with recovery actions *restart after 5 s,
//! restart after 30 s, then stop* (also on a non-crash failure exit), an
//! unrestricted service SID, and creates the protected data folder. Running
//! it again updates the registration in place (for upgrades).

use std::ffi::OsString;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use windows_service::service::{
    ServiceAccess, ServiceAction, ServiceActionType, ServiceErrorControl, ServiceFailureActions,
    ServiceFailureResetPeriod, ServiceInfo, ServiceSidType, ServiceStartType, ServiceState,
    ServiceType,
};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

use crate::paths::DataDir;
use crate::svc::{DESCRIPTION, DISPLAY_NAME, SERVICE_NAME};

/// `ERROR_SERVICE_DOES_NOT_EXIST`.
const NO_SUCH_SERVICE: i32 = 1060;

fn is_missing(e: &windows_service::Error) -> bool {
    matches!(e, windows_service::Error::Winapi(io) if io.raw_os_error() == Some(NO_SUCH_SERVICE))
}

pub fn install() -> Result<()> {
    let exe = std::env::current_exe().context("finding this exe's path")?;
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )
    .context("opening the service manager (run this as Administrator)")?;
    let info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe.clone(),
        launch_arguments: vec![OsString::from("service")],
        dependencies: vec![],
        account_name: None, // LocalSystem
        account_password: None,
    };
    let access = ServiceAccess::QUERY_CONFIG | ServiceAccess::CHANGE_CONFIG;
    let service = match manager.open_service(SERVICE_NAME, access) {
        Ok(s) => {
            s.change_config(&info).context("updating the service")?;
            println!("Updated the {SERVICE_NAME} service.");
            s
        }
        Err(e) if is_missing(&e) => {
            let s = manager
                .create_service(&info, access)
                .context("creating the service")?;
            println!("Installed the {SERVICE_NAME} service.");
            s
        }
        Err(e) => return Err(e).context("opening the service"),
    };
    service.set_description(DESCRIPTION)?;
    let restart = |secs| ServiceAction {
        action_type: ServiceActionType::Restart,
        delay: Duration::from_secs(secs),
    };
    service
        .update_failure_actions(ServiceFailureActions {
            reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(24 * 3600)),
            reboot_msg: None,
            command: None,
            actions: Some(vec![
                restart(5),
                restart(30),
                ServiceAction {
                    action_type: ServiceActionType::None,
                    delay: Duration::ZERO,
                },
            ]),
        })
        .context("setting recovery actions")?;
    service.set_failure_actions_on_non_crash_failures(true)?;
    service.set_config_service_sid_info(ServiceSidType::Unrestricted)?;
    service.set_delayed_auto_start(false)?;

    let dir = DataDir::system();
    dir.create_protected()
        .with_context(|| format!("creating {}", dir.root().display()))?;
    println!("  exe:    {}", exe.display());
    println!("  data:   {}", dir.root().display());
    println!("  config: {}", dir.config().display());
    println!("Start it with: sc start {SERVICE_NAME}");
    Ok(())
}

pub fn uninstall(remove_data: bool) -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .context("opening the service manager (run this as Administrator)")?;
    let access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE;
    match manager.open_service(SERVICE_NAME, access) {
        Ok(service) => {
            if service.query_status()?.current_state != ServiceState::Stopped {
                println!("Stopping {SERVICE_NAME}…");
                service.stop().context("stopping the service")?;
                let deadline = Instant::now() + Duration::from_secs(30);
                while service.query_status()?.current_state != ServiceState::Stopped {
                    if Instant::now() > deadline {
                        bail!("the service did not stop within 30 s; not removing it");
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
            service.delete().context("removing the service")?;
            println!("Removed the {SERVICE_NAME} service.");
        }
        Err(e) if is_missing(&e) => println!("The {SERVICE_NAME} service is not installed."),
        Err(e) => return Err(e).context("opening the service"),
    }
    let dir = DataDir::system();
    if remove_data {
        match std::fs::remove_dir_all(dir.root()) {
            Ok(()) => println!("Removed {}.", dir.root().display()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("removing {}", dir.root().display())),
        }
    } else if dir.root().exists() {
        println!(
            "Kept your settings in {} (uninstall --remove-data deletes them).",
            dir.root().display()
        );
    }
    Ok(())
}
