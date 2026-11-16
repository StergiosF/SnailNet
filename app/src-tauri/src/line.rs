//! Setting the line speed from the app (PLAN.md §12.2 `SetLine`, §14.6
//! Connection). Until the speed test and estimator exist (Phase 4) this is
//! how shaping gets a rate: a direction without a speed isn't shaped.

use snailnet_core::rules::Line;
use snailnet_ipc::{Client, ClientError, ErrorCode, PIPE_NAME, Reply, Request};

use crate::SaveError;

/// Replace the line speed; returns the new revision. The UI sends the
/// whole [`Line`], keeping the factor it had. Like `set_rules`, it uses its
/// own short connection: the bridge's one is busy reading the stats stream.
#[tauri::command]
pub async fn set_line(line: Line, rev: u64) -> Result<u64, SaveError> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut c = Client::connect(PIPE_NAME, env!("CARGO_PKG_VERSION"))?;
        c.call(Request::SetLine { line, rev })
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
}
