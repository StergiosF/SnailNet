//! Keeps one connection to the service and forwards what it says to the UI
//! (PLAN.md §15 data flow): the link state, the snapshot (again after every
//! `ConfigChanged`), state changes and the 4 Hz stats stream. When the
//! connection drops it reconnects with a short backoff, so starting the
//! service after the app needs nothing from the user.
//!
//! Written against [`Conn`] and [`Sink`] so the session logic is tested
//! without a pipe or a webview.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;
use snailnet_ipc::{
    Client, ClientError, Event, Reply, Request, ServerMsg, ServiceState, Snapshot, StatsTick,
};

/// Stats rate the UI asks for (PLAN.md §12.2).
pub const STATS_HZ: u32 = 4;

/// Waits between connection attempts: quick at first (the service may be
/// starting), then every few seconds.
const BACKOFF: [Duration; 4] = [
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
];

/// What the UI shows about the connection itself (the service's own state
/// travels separately as [`ServiceState`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Link {
    Connecting,
    Connected {
        service_version: String,
    },
    /// No pipe: the service isn't installed or isn't running.
    NotRunning,
    /// Windows refused access to the pipe.
    Denied {
        detail: String,
    },
    /// The service hung up on us: this exe isn't installed next to it.
    Rejected,
    /// Something else broke the connection; retrying.
    Lost {
        detail: String,
    },
}

impl Link {
    pub fn from_error(e: &ClientError) -> Self {
        match e {
            ClientError::NotRunning(_) => Self::NotRunning,
            ClientError::Denied(d) => Self::Denied { detail: d.clone() },
            ClientError::Impostor(_) => Self::Denied {
                detail: e.to_string(),
            },
            ClientError::Rejected => Self::Rejected,
            other => Self::Lost {
                detail: other.to_string(),
            },
        }
    }
}

/// One open connection.
pub trait Conn {
    fn call(&mut self, req: Request) -> Result<Reply, ClientError>;
    fn recv(&mut self) -> Result<ServerMsg, ClientError>;
    fn service_version(&self) -> String;
}

impl Conn for Client {
    fn call(&mut self, req: Request) -> Result<Reply, ClientError> {
        Client::call(self, req)
    }
    fn recv(&mut self) -> Result<ServerMsg, ClientError> {
        Client::recv(self)
    }
    fn service_version(&self) -> String {
        self.service_version.clone()
    }
}

/// Where the bridge sends what it learns.
pub trait Sink {
    fn link(&self, link: &Link);
    fn snapshot(&self, snap: &Snapshot);
    fn state(&self, state: &ServiceState);
    fn stats(&self, tick: &StatsTick);
}

fn fetch_snapshot<C: Conn, S: Sink>(conn: &mut C, sink: &S) -> Result<(), ClientError> {
    match conn.call(Request::GetSnapshot)? {
        Reply::Snapshot(s) => {
            sink.snapshot(&s);
            Ok(())
        }
        other => Err(ClientError::Unexpected(format!("{other:?}"))),
    }
}

/// Serve one open connection until it fails; returns why it ended.
pub fn session<C: Conn, S: Sink>(conn: &mut C, sink: &S, stop: &AtomicBool) -> ClientError {
    sink.link(&Link::Connected {
        service_version: conn.service_version(),
    });
    if let Err(e) = fetch_snapshot(conn, sink) {
        return e;
    }
    if let Err(e) = conn.call(Request::Subscribe { stats_hz: STATS_HZ }) {
        return e;
    }
    while !stop.load(Ordering::Relaxed) {
        match conn.recv() {
            Ok(ServerMsg::Stats(t)) => sink.stats(&t),
            Ok(ServerMsg::Event(Event::StateChanged(s))) => sink.state(&s),
            Ok(ServerMsg::Event(Event::ConfigChanged { .. })) => {
                if let Err(e) = fetch_snapshot(conn, sink) {
                    return e;
                }
            }
            // A stray reply (none are outstanding here): ignore it.
            Ok(ServerMsg::Reply { .. }) => {}
            Err(e) => return e,
        }
    }
    ClientError::Unexpected("stopped".into())
}

/// Connect, serve, reconnect, until `stop` is set. Reports every change of
/// link state once (a service that stays down isn't re-announced on every
/// attempt).
pub fn run<C, S, F>(connect: F, sink: &S, stop: &AtomicBool, sleep: impl Fn(Duration))
where
    C: Conn,
    S: Sink,
    F: Fn() -> Result<C, ClientError>,
{
    fn announce<S: Sink>(last: &mut Option<Link>, link: Link, sink: &S) {
        if last.as_ref() != Some(&link) {
            sink.link(&link);
            *last = Some(link);
        }
    }
    let mut last: Option<Link> = None;
    announce(&mut last, Link::Connecting, sink);
    let mut attempt = 0usize;
    while !stop.load(Ordering::Relaxed) {
        match connect() {
            Ok(mut conn) => {
                attempt = 0;
                let why = session(&mut conn, sink, stop);
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                // The session announced Connected itself, so whatever ended
                // it is always news.
                last = None;
                announce(&mut last, Link::from_error(&why), sink);
            }
            Err(e) => announce(&mut last, Link::from_error(&e), sink),
        }
        sleep(BACKOFF[attempt.min(BACKOFF.len() - 1)]);
        attempt += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::io;

    use snailnet_ipc::FrameError;

    #[derive(Default)]
    struct Log(RefCell<Vec<String>>);

    impl Sink for Log {
        fn link(&self, link: &Link) {
            self.0.borrow_mut().push(format!("link {link:?}"));
        }
        fn snapshot(&self, snap: &Snapshot) {
            self.0
                .borrow_mut()
                .push(format!("snapshot rev {}", snap.rev));
        }
        fn state(&self, state: &ServiceState) {
            self.0.borrow_mut().push(format!("state {state:?}"));
        }
        fn stats(&self, tick: &StatsTick) {
            self.0.borrow_mut().push(format!("stats {}", tick.t_ms));
        }
    }

    fn snapshot(rev: u64) -> Snapshot {
        Snapshot {
            rev,
            state: ServiceState::Shaping,
            config: Default::default(),
            apps: Vec::new(),
            sites: Vec::new(),
        }
    }

    fn tick(t_ms: u64) -> StatsTick {
        StatsTick {
            t_ms,
            state: ServiceState::Shaping,
            down: None,
            up: None,
            names: None,
        }
    }

    fn broken() -> ClientError {
        FrameError::Io(io::Error::from(io::ErrorKind::BrokenPipe)).into()
    }

    /// Replies to calls in order, then streams `msgs`, then fails.
    struct Fake {
        replies: VecDeque<Reply>,
        msgs: VecDeque<ServerMsg>,
        calls: Vec<&'static str>,
    }

    impl Conn for Fake {
        fn call(&mut self, req: Request) -> Result<Reply, ClientError> {
            self.calls.push(req.name());
            self.replies.pop_front().ok_or_else(broken)
        }
        fn recv(&mut self) -> Result<ServerMsg, ClientError> {
            self.msgs.pop_front().ok_or_else(broken)
        }
        fn service_version(&self) -> String {
            "0.0.1".into()
        }
    }

    #[test]
    fn session_forwards_snapshot_stats_and_refetches_on_config_change() {
        let mut conn = Fake {
            replies: VecDeque::from([
                Reply::Snapshot(snapshot(3)),
                Reply::Ack { rev: 3 },
                Reply::Snapshot(snapshot(4)),
            ]),
            msgs: VecDeque::from([
                ServerMsg::Stats(tick(250)),
                ServerMsg::Event(Event::ConfigChanged { rev: 4 }),
                ServerMsg::Event(Event::StateChanged(ServiceState::Unshaped)),
                ServerMsg::Stats(tick(500)),
            ]),
            calls: Vec::new(),
        };
        let log = Log::default();
        let why = session(&mut conn, &log, &AtomicBool::new(false));
        assert!(matches!(why, ClientError::Frame(_)));
        assert_eq!(conn.calls, ["GetSnapshot", "Subscribe", "GetSnapshot"]);
        assert_eq!(
            *log.0.borrow(),
            [
                r#"link Connected { service_version: "0.0.1" }"#,
                "snapshot rev 3",
                "stats 250",
                "snapshot rev 4",
                "state Unshaped",
                "stats 500",
            ]
        );
    }

    #[test]
    fn errors_map_to_link_states() {
        assert_eq!(
            Link::from_error(&ClientError::NotRunning("p".into())),
            Link::NotRunning
        );
        assert_eq!(Link::from_error(&ClientError::Rejected), Link::Rejected);
        assert!(matches!(
            Link::from_error(&ClientError::Impostor("p".into())),
            Link::Denied { .. }
        ));
        assert!(matches!(Link::from_error(&broken()), Link::Lost { .. }));
    }

    #[test]
    fn run_announces_each_link_change_once_and_backs_off() {
        // Down three times, then up for one session, then stop.
        let attempts = RefCell::new(0);
        let stop = AtomicBool::new(false);
        let sleeps = RefCell::new(Vec::new());
        let log = Log::default();
        run(
            || {
                *attempts.borrow_mut() += 1;
                match *attempts.borrow() {
                    1..=3 => Err(ClientError::NotRunning("p".into())),
                    _ => Ok(Fake {
                        replies: VecDeque::from([
                            Reply::Snapshot(snapshot(1)),
                            Reply::Ack { rev: 1 },
                        ]),
                        msgs: VecDeque::new(),
                        calls: Vec::new(),
                    }),
                }
            },
            &log,
            &stop,
            |d| {
                sleeps.borrow_mut().push(d);
                if sleeps.borrow().len() == 5 {
                    stop.store(true, Ordering::Relaxed);
                }
            },
        );
        let log = log.0.borrow();
        assert_eq!(log[0], "link Connecting");
        assert_eq!(log[1], "link NotRunning");
        assert_eq!(log[2], r#"link Connected { service_version: "0.0.1" }"#);
        assert_eq!(log[3], "snapshot rev 1");
        assert!(log[4].starts_with("link Lost"));
        // Backoff grows while down and resets after a connection.
        let s = sleeps.borrow();
        assert_eq!(s[0], BACKOFF[0]);
        assert_eq!(s[1], BACKOFF[1]);
        assert_eq!(s[2], BACKOFF[2]);
        assert_eq!(s[3], BACKOFF[0]);
    }
}
