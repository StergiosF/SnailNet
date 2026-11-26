//! Changes the app asks the service for (PLAN.md §12.2): rules, modes,
//! boost, pause, blocking, resets and the line speed. Quick commands share
//! one persistent connection (reconnecting once if the service restarted) so
//! they don't pay a pipe handshake per click; multi-second operations such as
//! speed tests use their own ephemeral connection so they never block quick
//! UI actions. The bridge's connection stays dedicated to the stats stream.

use std::sync::{Mutex, OnceLock, PoisonError};

use snailnet_core::families::Families;
use snailnet_core::rules::{Line, Rule, Settings, Target};
use snailnet_ipc::{
    CAP_NO_EVENTS, Client, ClientError, ErrorCode, PIPE_NAME, Reply, Request, Snapshot,
};

use crate::SaveError;

/// Shared persistent pipe for quick UI requests.
static CLIENT: OnceLock<Mutex<Option<Client>>> = OnceLock::new();

fn client_slot() -> &'static Mutex<Option<Client>> {
    CLIENT.get_or_init(|| Mutex::new(None))
}

/// Run `req` over a persistent client slot, dropping and reconnecting once on
/// broken pipe / IO errors (`Frame` or `NotRunning`). Application refusals
/// (`Refused`) leave the healthy connection intact without retrying.
fn call_with_reconnect<C>(
    slot: &Mutex<Option<C>>,
    connect: impl Fn() -> Result<C, ClientError>,
    call: impl Fn(&mut C, Request) -> Result<Reply, ClientError>,
    req: Request,
) -> Result<Reply, ClientError> {
    let mut guard = slot.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(c) = guard.as_mut() {
        match call(c, req.clone()) {
            Ok(reply) => return Ok(reply),
            Err(ClientError::Frame(_) | ClientError::NotRunning(_)) => {
                *guard = None;
            }
            Err(e @ ClientError::Refused { .. }) => return Err(e),
            Err(e) => {
                *guard = None;
                return Err(e);
            }
        }
    }
    let mut fresh = connect()?;
    match call(&mut fresh, req) {
        Ok(reply) => {
            *guard = Some(fresh);
            Ok(reply)
        }
        Err(e @ ClientError::Refused { .. }) => {
            *guard = Some(fresh);
            Err(e)
        }
        Err(e) => Err(e),
    }
}

async fn call_persistent(req: Request) -> Result<Result<Reply, ClientError>, SaveError> {
    tauri::async_runtime::spawn_blocking(move || {
        call_with_reconnect(
            client_slot(),
            || Client::connect_with(PIPE_NAME, env!("CARGO_PKG_VERSION"), &[CAP_NO_EVENTS]),
            |c, r| {
                let res = c.call(r);
                c.clear_backlog();
                res
            },
            req,
        )
    })
    .await
    .map_err(|e| SaveError::Failed {
        message: e.to_string(),
    })
}

/// Send one quick mutating request over the persistent connection; its reply
/// as the UI needs it.
pub(crate) async fn send(req: Request) -> Result<u64, SaveError> {
    call_persistent(req).await.and_then(ack)
}

/// Send a potentially long-running request on its own ephemeral connection so
/// it never holds the shared pipe lock while quick UI commands run.
async fn send_ephemeral(req: Request) -> Result<u64, SaveError> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut c = Client::connect(PIPE_NAME, env!("CARGO_PKG_VERSION"))?;
        c.call(req)
    })
    .await
    .map_err(|e| SaveError::Failed {
        message: e.to_string(),
    })
    .and_then(ack)
}

/// Fetch a fresh snapshot over the persistent connection (used to recover
/// from a stale-revision conflict on rapid hotkey presses).
pub(crate) async fn get_snapshot() -> Option<Snapshot> {
    match call_persistent(Request::GetSnapshot).await {
        Ok(Ok(Reply::Snapshot(s))) => Some(*s),
        _ => None,
    }
}

/// What a mutating call's reply means for the UI.
fn ack(reply: Result<Reply, ClientError>) -> Result<u64, SaveError> {
    match reply {
        Ok(Reply::Ack { rev }) => Ok(rev),
        Ok(other) => Err(SaveError::Failed {
            message: format!("unexpected reply: {other:?}"),
        }),
        Err(ClientError::Refused {
            code: ErrorCode::Conflict,
            ..
        }) => Err(SaveError::Conflict),
        Err(other) => Err(SaveError::Failed {
            message: other.to_string(),
        }),
    }
}

/// Replace one profile's rules (`SetRules`).
#[tauri::command]
pub async fn set_rules(profile_id: String, rules: Vec<Rule>, rev: u64) -> Result<u64, SaveError> {
    send(Request::SetRules {
        profile_id,
        rules,
        rev,
    })
    .await
}

/// Replace the line speed. The UI sends the whole [`Line`], keeping the
/// factor it had.
#[tauri::command]
pub async fn set_line(line: Line, rev: u64) -> Result<u64, SaveError> {
    send(Request::SetLine { line, rev }).await
}

/// Switch mode (a profile id).
#[tauri::command]
pub async fn switch_profile(id: String, rev: u64) -> Result<u64, SaveError> {
    send(Request::SwitchProfile { id, rev }).await
}

/// Put a mode's split back to where it started.
#[tauri::command]
pub async fn reset_profile(id: String, rev: u64) -> Result<u64, SaveError> {
    send(Request::ResetProfile { id, rev }).await
}

/// "Duplicate & edit" (§11.2): copy mode `from` as `name`; the copy
/// becomes the active mode.
#[tauri::command]
pub async fn copy_profile(from: String, name: String, rev: u64) -> Result<u64, SaveError> {
    send(Request::CopyProfile { from, name, rev }).await
}

/// Rename one of the user's own modes.
#[tauri::command]
pub async fn rename_profile(id: String, name: String, rev: u64) -> Result<u64, SaveError> {
    send(Request::RenameProfile { id, name, rev }).await
}

/// Delete one of the user's own modes.
#[tauri::command]
pub async fn delete_profile(id: String, rev: u64) -> Result<u64, SaveError> {
    send(Request::DeleteProfile { id, rev }).await
}

/// Forget everything: rules, line speed, settings, boost.
#[tauri::command]
pub async fn reset_all(rev: u64) -> Result<u64, SaveError> {
    send(Request::ResetAll { rev }).await
}

/// Ask for a speed test (§10.1). The service runs it with shaping off and
/// saves the result as the line speed; the state says `Measuring` meanwhile.
/// Uses an ephemeral connection so a speed test never blocks quick commands.
#[tauri::command]
pub async fn run_speed_test(rev: u64) -> Result<u64, SaveError> {
    let _ = rev;
    send_ephemeral(Request::RunSpeedTest).await
}

/// Let `target` go first for `minutes` (`None`: until stopped).
#[tauri::command]
pub async fn boost(target: Target, minutes: Option<u32>, rev: u64) -> Result<u64, SaveError> {
    send(Request::Boost {
        target,
        minutes,
        rev,
    })
    .await
}

#[tauri::command]
pub async fn stop_boost(rev: u64) -> Result<u64, SaveError> {
    send(Request::StopBoost { rev }).await
}

/// Step aside for `minutes` (`None`: until resumed).
#[tauri::command]
pub async fn pause(minutes: Option<u32>, rev: u64) -> Result<u64, SaveError> {
    send(Request::Pause { minutes, rev }).await
}

#[tauri::command]
pub async fn resume(rev: u64) -> Result<u64, SaveError> {
    send(Request::Resume { rev }).await
}

/// Block or unblock an app's internet (a family id).
#[tauri::command]
pub async fn set_blocked(family_id: String, blocked: bool, rev: u64) -> Result<u64, SaveError> {
    send(Request::SetBlocked {
        family_id,
        blocked,
        rev,
    })
    .await
}

/// Replace the general settings (low latency, local network, focus boost).
#[tauri::command]
pub async fn set_settings(settings: Settings, rev: u64) -> Result<u64, SaveError> {
    send(Request::SetSettings { settings, rev }).await
}

/// Replace how apps are grouped (programs split out, families merged).
#[tauri::command]
pub async fn set_families(families: Families, rev: u64) -> Result<u64, SaveError> {
    send(Request::SetFamilies { families, rev }).await
}

/// Ask `snailnet-service` to shut down cleanly when the user quits from
/// the tray. Tries IPC first (works for both the Windows service and
/// `run --console`, with no UAC prompt), then falls back to a quiet SCM
/// stop if the pipe wasn't reachable.
pub(crate) fn stop_service_blocking() {
    if let Some(slot) = CLIENT.get()
        && let Ok(mut guard) = slot.try_lock()
    {
        *guard = None;
    }
    match Client::connect(PIPE_NAME, env!("CARGO_PKG_VERSION")) {
        Ok(mut c) => {
            if c.call(Request::Stop).is_ok() {
                return;
            }
            let _ = crate::service_start::stop_plain();
        }
        Err(ClientError::NotRunning(_)) => {}
        Err(_) => {
            let _ = crate::service_start::stop_plain();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ack_gives_the_new_revision() {
        assert!(matches!(ack(Ok(Reply::Ack { rev: 7 })), Ok(7)));
    }

    #[test]
    fn a_stale_revision_is_a_conflict() {
        let refused = ClientError::Refused {
            code: ErrorCode::Conflict,
            msg: "the settings changed".into(),
        };
        assert!(matches!(ack(Err(refused)), Err(SaveError::Conflict)));
    }

    #[test]
    fn other_refusals_say_why() {
        let refused = ClientError::Refused {
            code: ErrorCode::Invalid,
            msg: "line.down_mbps = 0 is outside 0.1–10000 Mbps".into(),
        };
        match ack(Err(refused)) {
            Err(SaveError::Failed { message }) => assert!(message.contains("0.1–10000")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn the_ui_sends_a_line_as_the_service_stores_it() {
        let line: Line =
            serde_json::from_str(r#"{"down_mbps":25,"up_mbps":40.5,"factor":0.85}"#).unwrap();
        assert_eq!(line.down_mbps, Some(25.0));
        assert_eq!(line.up_mbps, Some(40.5));
        assert_eq!(line.factor, Some(0.85));
        let cleared: Line = serde_json::from_str("{}").unwrap();
        assert_eq!(cleared, Line::default());
    }

    #[test]
    fn the_ui_sends_families_as_the_service_reads_them() {
        let f: Families = serde_json::from_str(
            r#"{"split":["helper.exe"],"merge":[{"from":"a","into":"b","name":"B"}]}"#,
        )
        .unwrap();
        assert_eq!(f.split, ["helper.exe"]);
        assert_eq!(f.merge[0].into, "b");
        assert_eq!(
            serde_json::from_str::<Families>("{}").unwrap(),
            Families::default()
        );
    }

    #[test]
    fn the_ui_sends_a_boost_target_as_the_service_reads_it() {
        let t: Target = serde_json::from_str(r#"{"kind":"site","id":"youtube"}"#).unwrap();
        assert_eq!(t, Target::site("youtube"));
    }

    #[test]
    fn persistent_client_reuses_keeps_on_refusal_and_reconnects_once_on_broken_pipe() {
        use std::cell::Cell;
        use std::io;

        let slot: Mutex<Option<u32>> = Mutex::new(None);
        let connects = Cell::new(0u32);
        let connect = || {
            connects.set(connects.get() + 1);
            Ok(connects.get())
        };
        let broken = || {
            ClientError::Frame(snailnet_ipc::FrameError::Io(io::Error::from(
                io::ErrorKind::BrokenPipe,
            )))
        };

        // First two calls share connection #1.
        let r1 = call_with_reconnect(
            &slot,
            connect,
            |_, _| Ok(Reply::Ack { rev: 1 }),
            Request::Resume { rev: 0 },
        );
        let r2 = call_with_reconnect(
            &slot,
            connect,
            |_, _| Ok(Reply::Ack { rev: 2 }),
            Request::Resume { rev: 1 },
        );
        assert!(matches!(r1, Ok(Reply::Ack { rev: 1 })));
        assert!(matches!(r2, Ok(Reply::Ack { rev: 2 })));
        assert_eq!(connects.get(), 1);

        // Application-level refusal keeps connection #1 without reconnecting.
        let r3 = call_with_reconnect(
            &slot,
            connect,
            |_, _| {
                Err(ClientError::Refused {
                    code: ErrorCode::Conflict,
                    msg: "stale".into(),
                })
            },
            Request::Resume { rev: 1 },
        );
        assert!(matches!(
            r3,
            Err(ClientError::Refused {
                code: ErrorCode::Conflict,
                ..
            })
        ));
        assert_eq!(connects.get(), 1);
        assert_eq!(*slot.lock().unwrap(), Some(1));

        // Broken pipe drops #1 and reconnects once (#2) immediately.
        let r4 = call_with_reconnect(
            &slot,
            connect,
            |conn, _| {
                if *conn == 1 {
                    Err(broken())
                } else {
                    Ok(Reply::Ack { rev: 3 })
                }
            },
            Request::Resume { rev: 2 },
        );
        assert!(matches!(r4, Ok(Reply::Ack { rev: 3 })));
        assert_eq!(connects.get(), 2);
        assert_eq!(*slot.lock().unwrap(), Some(2));
    }
}
