//! Changes the app asks the service for (PLAN.md §12.2): rules, modes,
//! boost, pause, blocking, resets and the line speed. Each uses its own
//! short connection (the bridge's one is busy reading the stats stream)
//! and returns the new config revision; the new snapshot arrives through
//! the bridge.

use snailnet_core::rules::{Line, Rule, Settings, Target};
use snailnet_ipc::{Client, ClientError, ErrorCode, PIPE_NAME, Reply, Request};

use crate::SaveError;

/// Send one mutating request; its reply as the UI needs it.
pub(crate) async fn send(req: Request) -> Result<u64, SaveError> {
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
#[tauri::command]
pub async fn run_speed_test(rev: u64) -> Result<u64, SaveError> {
    let _ = rev;
    send(Request::RunSpeedTest).await
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
    fn the_ui_sends_a_boost_target_as_the_service_reads_it() {
        let t: Target = serde_json::from_str(r#"{"kind":"site","id":"youtube"}"#).unwrap();
        assert_eq!(t, Target::site("youtube"));
    }
}
