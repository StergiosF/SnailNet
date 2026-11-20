//! The IPC server (PLAN.md §12): `\\.\pipe\snailnet.v1`, protocol v1.
//!
//! One thread runs a single-threaded tokio runtime: an accept loop that
//! always keeps one pipe instance listening, and one task per client. The
//! pipe's DACL ([`snailnet_ipc::PIPE_SDDL`]) denies network logons and lets
//! only SYSTEM, Administrators and interactive users in; remote clients are
//! rejected, and the first instance is created with
//! `FILE_FLAG_FIRST_PIPE_INSTANCE` so nobody can squat the name first.
//! Each client must be a program in the service's own folder (§12.1: the
//! CLI, later `SnailNet.exe`); others are disconnected before Hello.
//! Frames are bounded (1 MB) and every change is validated by the hub
//! before it is saved. Never touches the packet path.

use std::io;
use std::os::windows::io::AsRawHandle;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result};
use snailnet_ipc::{
    CAP_NEW_APPS, ClientMsg, ErrorCode, Event, FrameError, GameWhy, MIN_PROTOCOL, PIPE_SDDL,
    PROTOCOL, Reply, Request, ServerMsg, decode, encode, frame_len, parse_len,
};
use snailnet_win::fs::SecurityAttributes;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::{Interval, MissedTickBehavior};
use tracing::{debug, info, warn};

use crate::hub::Hub;
use crate::runner::unix_now;

/// More simultaneous clients than this are turned away.
const MAX_CLIENTS: usize = 16;
/// A client that doesn't take a frame within this long is dropped.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

pub struct IpcServer {
    shutdown: watch::Sender<bool>,
    thread: Option<JoinHandle<()>>,
}

impl IpcServer {
    /// Create the pipe and start serving. Fails if the pipe can't be
    /// created (e.g. another SnailNet service already owns the name).
    pub fn start(hub: Arc<Hub>, pipe: &str) -> Result<Self> {
        let sa =
            Arc::new(SecurityAttributes::from_sddl(PIPE_SDDL).context("building the pipe ACL")?);
        let (shutdown, rx) = watch::channel(false);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let pipe = pipe.to_owned();
        let thread = std::thread::Builder::new()
            .name("ipc".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                rt.block_on(async move {
                    let first = match create(&pipe, true, &sa) {
                        Ok(s) => {
                            let _ = ready_tx.send(Ok(()));
                            s
                        }
                        Err(e) => {
                            let _ = ready_tx.send(Err(e));
                            return;
                        }
                    };
                    accept_loop(hub, pipe, sa, first, rx).await;
                });
            })
            .context("starting the IPC thread")?;
        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let _ = thread.join();
                return Err(e).context("creating the IPC pipe (is another SnailNet running?)");
            }
            Err(_) => {
                let _ = thread.join();
                anyhow::bail!("the IPC thread ended before it started");
            }
        }
        Ok(Self {
            shutdown,
            thread: Some(thread),
        })
    }

    /// Close the pipe and every client connection.
    pub fn stop(mut self) {
        self.halt();
    }

    fn halt(&mut self) {
        let _ = self.shutdown.send(true);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.halt();
    }
}

fn create(pipe: &str, first: bool, sa: &SecurityAttributes) -> io::Result<NamedPipeServer> {
    let mut o = ServerOptions::new();
    o.first_pipe_instance(first).reject_remote_clients(true);
    // SAFETY: `sa` is a valid SECURITY_ATTRIBUTES that outlives this call.
    unsafe { o.create_with_security_attributes_raw(pipe, sa.as_ptr()) }
}

async fn accept_loop(
    hub: Arc<Hub>,
    pipe: String,
    sa: Arc<SecurityAttributes>,
    first: NamedPipeServer,
    mut shutdown: watch::Receiver<bool>,
) {
    info!("IPC: listening on {pipe}");
    let clients = Arc::new(AtomicUsize::new(0));
    let mut listening = Some(first);
    loop {
        // Keep an instance listening. Creating it right after the previous
        // one connected leaves only a microsecond gap without one.
        let server = match &mut listening {
            Some(s) => s,
            None => match create(&pipe, false, &sa) {
                Ok(s) => listening.insert(s),
                Err(e) => {
                    warn!("IPC: could not create a pipe instance: {e}; retrying");
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(1)) => continue,
                        _ = shutdown.changed() => break,
                    }
                }
            },
        };
        tokio::select! {
            r = server.connect() => {
                let client = listening.take().expect("listening");
                if let Err(e) = r {
                    debug!("IPC: a client failed to connect: {e}");
                    continue;
                }
                match verify_client(&client) {
                    Ok(exe) => debug!("IPC: client {}", exe.display()),
                    Err(why) => {
                        warn!("IPC: refused a client: {why}");
                        continue;
                    }
                }
                if clients.load(Ordering::Relaxed) >= MAX_CLIENTS {
                    warn!("IPC: more than {MAX_CLIENTS} clients; turning one away");
                    continue;
                }
                clients.fetch_add(1, Ordering::Relaxed);
                let (hub, shutdown, clients) = (hub.clone(), shutdown.clone(), clients.clone());
                tokio::spawn(async move {
                    if let Err(e) = serve(client, &hub, shutdown).await {
                        debug!("IPC: client gone: {e}");
                    }
                    clients.fetch_sub(1, Ordering::Relaxed);
                });
            }
            _ = shutdown.changed() => break,
        }
    }
    info!("IPC: stopped");
}

/// Only programs installed next to the service may talk to it.
fn verify_client(pipe: &NamedPipeServer) -> Result<PathBuf, String> {
    use snailnet_win::pipe::{End, in_my_folder, peer_pid};
    let pid = peer_pid(pipe.as_raw_handle(), End::Client)
        .map_err(|e| format!("can't tell which process it is: {e}"))?;
    in_my_folder(pid)
}

async fn read_msg<R: AsyncRead + Unpin>(r: &mut R) -> Result<ClientMsg, FrameError> {
    let mut prefix = [0u8; 4];
    r.read_exact(&mut prefix).await?;
    let mut body = vec![0u8; parse_len(prefix)?];
    r.read_exact(&mut body).await?;
    decode(&body)
}

async fn write_msg<W: AsyncWrite + Unpin>(w: &mut W, msg: &ServerMsg) -> Result<(), FrameError> {
    let body = encode(msg)?;
    let mut buf = Vec::with_capacity(4 + body.len());
    buf.extend_from_slice(&frame_len(body.len())?);
    buf.extend_from_slice(&body);
    match tokio::time::timeout(WRITE_TIMEOUT, async {
        w.write_all(&buf).await?;
        w.flush().await
    })
    .await
    {
        Ok(r) => Ok(r?),
        Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "client stopped reading").into()),
    }
}

async fn next_tick(t: &mut Option<Interval>) {
    match t {
        Some(i) => {
            i.tick().await;
        }
        None => std::future::pending().await,
    }
}

/// Per-client state.
struct Session {
    hello: bool,
    ticker: Option<Interval>,
    /// Said in Hello that it understands [`Event::NewApp`].
    new_apps: bool,
}

async fn serve(
    pipe: NamedPipeServer,
    hub: &Hub,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), FrameError> {
    let (mut rd, mut wr) = tokio::io::split(pipe);
    // Reading isn't cancel-safe, so it gets its own task and a channel.
    let (tx, mut rx) = mpsc::channel::<Result<ClientMsg, FrameError>>(8);
    let reader = tokio::spawn(async move {
        loop {
            let m = read_msg(&mut rd).await;
            let end = m.is_err();
            if tx.send(m).await.is_err() || end {
                break;
            }
        }
    });
    let mut events = hub.subscribe();
    let mut s = Session {
        hello: false,
        ticker: None,
        new_apps: false,
    };
    let result = loop {
        tokio::select! {
            m = rx.recv() => {
                let msg = match m {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => break Err(e),
                    None => break Ok(()),
                };
                let stop = s.hello && matches!(msg.req, Request::Stop);
                let reply = handle(hub, &mut s, msg.req);
                let bye = stop || matches!(reply, Reply::Error { code: ErrorCode::Handshake, .. });
                if let Err(e) = write_msg(&mut wr, &ServerMsg::Reply { id: msg.id, reply }).await {
                    if stop {
                        hub.signal().request();
                    }
                    break Err(e);
                }
                if stop {
                    hub.signal().request();
                }
                if bye {
                    break Ok(());
                }
            }
            _ = next_tick(&mut s.ticker) => {
                if let Err(e) = write_msg(&mut wr, &ServerMsg::Stats(Box::new(hub.latest_tick()))).await {
                    break Err(e);
                }
            }
            ev = events.recv() => match ev {
                Ok(Event::NewApp(_)) if !s.new_apps => {}
                Ok(ev) if s.hello => {
                    if let Err(e) = write_msg(&mut wr, &ServerMsg::Event(ev)).await {
                        break Err(e);
                    }
                }
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => break Ok(()),
            },
            _ = shutdown.changed() => break Ok(()),
        }
    };
    reader.abort();
    result
}

fn err(code: ErrorCode, msg: impl Into<String>) -> Reply {
    Reply::Error {
        code,
        msg: msg.into(),
    }
}

fn ack(r: Result<u64, (ErrorCode, String)>) -> Reply {
    match r {
        Ok(rev) => Reply::Ack { rev },
        Err((code, msg)) => err(code, msg),
    }
}

fn handle(hub: &Hub, s: &mut Session, req: Request) -> Reply {
    if !s.hello && !matches!(req, Request::Hello { .. }) {
        return err(ErrorCode::Handshake, "say Hello first");
    }
    debug!("IPC: {}", req.name());
    match req {
        Request::Hello {
            protocol,
            client_version,
            ui_capabilities,
        } => {
            if !(MIN_PROTOCOL..=PROTOCOL).contains(&protocol) {
                return err(
                    ErrorCode::Handshake,
                    format!(
                        "protocol {protocol} isn't supported (this service speaks {MIN_PROTOCOL}–{PROTOCOL})"
                    ),
                );
            }
            debug!("IPC: hello from client {client_version}");
            s.hello = true;
            s.new_apps = ui_capabilities.iter().any(|c| c == CAP_NEW_APPS);
            Reply::Welcome {
                service_version: env!("CARGO_PKG_VERSION").into(),
                protocol: PROTOCOL,
                state: hub.state(),
            }
        }
        Request::GetSnapshot => Reply::Snapshot(Box::new(hub.snapshot())),
        Request::Subscribe { stats_hz } => {
            let every = Duration::from_millis(1000 / u64::from(stats_hz.clamp(1, 10)));
            let mut i = tokio::time::interval(every);
            i.set_missed_tick_behavior(MissedTickBehavior::Skip);
            s.ticker = Some(i);
            Reply::Ack { rev: hub.rev() }
        }
        Request::Unsubscribe => {
            s.ticker = None;
            Reply::Ack { rev: hub.rev() }
        }
        Request::SetRules {
            profile_id,
            rules,
            rev,
        } => ack(hub.set_rules(&profile_id, rules, rev)),
        Request::SwitchProfile { id, rev } => ack(hub.switch_profile(&id, rev)),
        Request::SetLine { line, rev } => ack(hub.set_line(line, rev)),
        Request::ResetProfile { id, rev } => ack(hub.reset_profile(&id, rev)),
        Request::CopyProfile { from, name, rev } => ack(hub.copy_profile(&from, &name, rev)),
        Request::RenameProfile { id, name, rev } => ack(hub.rename_profile(&id, &name, rev)),
        Request::DeleteProfile { id, rev } => ack(hub.delete_profile(&id, rev)),
        Request::ResetAll { rev } => ack(hub.reset_all(rev)),
        Request::Boost {
            target,
            minutes,
            rev,
        } => ack(hub.boost(target, minutes, unix_now(), rev)),
        Request::StopBoost { rev } => ack(hub.stop_boost(rev)),
        Request::Pause { minutes, rev } => ack(hub.pause(minutes, unix_now(), rev)),
        Request::Resume { rev } => ack(hub.resume(rev)),
        Request::SetBlocked {
            family_id,
            blocked,
            rev,
        } => ack(hub.set_blocked(&family_id, blocked, rev)),
        Request::RunSpeedTest => ack(hub.request_speedtest()),
        Request::SetSettings { settings, rev } => ack(hub.set_settings(settings, rev)),
        Request::SetFamilies { families, rev } => ack(hub.set_families(families, rev)),
        Request::FocusChanged {
            pid, is_game, why, ..
        } => {
            // An older app says only whether: count it as fullscreen.
            hub.focus_changed(pid, is_game.then(|| why.unwrap_or(GameWhy::Fullscreen)));
            ack(Ok(hub.rev()))
        }
        Request::AnswerPrompt { prompt_id, action } => ack(hub.answer_prompt(prompt_id, &action)),
        Request::Stop => {
            info!("IPC: stop requested by client");
            Reply::Ack { rev: hub.rev() }
        }
    }
}

#[cfg(test)]
mod tests {
    use snailnet_core::rules::{Config, Line, Rule, Target};
    use snailnet_ipc::{Client, ClientError, Event, ServiceState};

    use super::*;
    use crate::paths::DataDir;
    use crate::runner::StopSignal;

    fn pipe_name(tag: &str) -> String {
        format!(r"\\.\pipe\snailnet-test-{tag}-{}", std::process::id())
    }

    fn setup(tag: &str) -> (tempfile::TempDir, Arc<Hub>, IpcServer, String) {
        let t = tempfile::tempdir().unwrap();
        let d = DataDir::new(t.path());
        d.create().unwrap();
        let hub = Hub::new(d, StopSignal::new());
        hub.set_loaded(Config::default());
        let pipe = pipe_name(tag);
        let server = IpcServer::start(hub.clone(), &pipe).unwrap();
        (t, hub, server, pipe)
    }

    fn snapshot(c: &mut Client) -> snailnet_ipc::Snapshot {
        match c.call(Request::GetSnapshot).unwrap() {
            Reply::Snapshot(s) => *s,
            r => panic!("{r:?}"),
        }
    }

    #[test]
    fn hello_snapshot_and_a_rule_change() {
        let (_t, hub, server, pipe) = setup("rules");
        let mut c = Client::connect(&pipe, "test").unwrap();
        assert_eq!(c.service_version, env!("CARGO_PKG_VERSION"));
        let snap = snapshot(&mut c);
        assert_eq!((snap.rev, &snap.state), (1, &ServiceState::Starting));

        let rule = Rule {
            target: Target::family("steam"),
            share: 4000,
            priority: Default::default(),
            ceiling: None,
            enabled: true,
        };
        let r = c.call(Request::SetRules {
            profile_id: "balanced".into(),
            rules: vec![rule.clone()],
            rev: 1,
        });
        assert_eq!(r.unwrap(), Reply::Ack { rev: 2 });
        assert!(hub.signal().take_reload(), "the runner is told");
        assert_eq!(snapshot(&mut c).config.profiles[0].rules, vec![rule]);

        // The change is broadcast to clients.
        match c.recv().unwrap() {
            ServerMsg::Event(Event::ConfigChanged { rev: 2 }) => {}
            m => panic!("{m:?}"),
        }
        server.stop();
    }

    #[test]
    fn two_clients_cant_overwrite_each_other() {
        let (_t, _hub, server, pipe) = setup("conflict");
        let mut a = Client::connect(&pipe, "a").unwrap();
        let mut b = Client::connect(&pipe, "b").unwrap();
        let line = |m| Line {
            down_mbps: Some(m),
            ..Line::default()
        };
        assert_eq!(
            a.call(Request::SetLine {
                line: line(25.0),
                rev: 1
            })
            .unwrap(),
            Reply::Ack { rev: 2 }
        );
        match b.call(Request::SetLine {
            line: line(50.0),
            rev: 1,
        }) {
            Err(ClientError::Refused {
                code: ErrorCode::Conflict,
                ..
            }) => {}
            r => panic!("{r:?}"),
        }
        assert_eq!(snapshot(&mut b).config.line.down_mbps, Some(25.0));
        server.stop();
    }

    #[test]
    fn invalid_and_unsupported_requests_are_refused() {
        let (_t, hub, server, pipe) = setup("refuse");
        let mut c = Client::connect(&pipe, "t").unwrap();
        match c.call(Request::SwitchProfile {
            id: "nope".into(),
            rev: 1,
        }) {
            Err(ClientError::Refused {
                code: ErrorCode::Invalid,
                ..
            }) => {}
            r => panic!("{r:?}"),
        }
        // The window in front is taken (§11.5); it wakes the runner.
        assert!(matches!(
            c.call(Request::FocusChanged {
                pid: std::process::id(),
                is_game: false,
                confidence: 0.0,
                why: None,
            }),
            Ok(Reply::Ack { rev: 1 })
        ));
        assert!(hub.signal().take_reload());
        match c.call(Request::AnswerPrompt {
            prompt_id: 1,
            action: "x".into(),
        }) {
            Err(ClientError::Refused {
                code: ErrorCode::Invalid,
                ..
            }) => {}
            r => panic!("{r:?}"),
        }
        // A speed test is queued for the runner, which takes it.
        assert!(matches!(
            c.call(Request::RunSpeedTest),
            Ok(Reply::Ack { rev: 1 })
        ));
        assert!(hub.take_speedtest());
        assert!(!hub.take_speedtest());
        // Still usable afterwards.
        assert_eq!(snapshot(&mut c).rev, 1);
        server.stop();
    }

    #[test]
    fn boost_and_resets() {
        let (_t, hub, server, pipe) = setup("boost");
        let mut c = Client::connect(&pipe, "t").unwrap();
        let ack = |r: Result<Reply, ClientError>| match r {
            Ok(Reply::Ack { rev }) => rev,
            r => panic!("{r:?}"),
        };
        let rev = ack(c.call(Request::Boost {
            target: Target::family("steam"),
            minutes: Some(60),
            rev: 1,
        }));
        let b = snapshot(&mut c).config.boost.unwrap();
        assert_eq!(b.target, Target::family("steam"));
        let until = b.until.unwrap();
        assert!(until > unix_now() + 3500, "an hour from now");
        assert!(!hub.end_expired(until - 1));
        assert!(hub.end_expired(until), "time's up");
        assert_eq!(snapshot(&mut c).config.boost, None);

        // Until stopped.
        let rev = ack(c.call(Request::Boost {
            target: Target::site("youtube"),
            minutes: None,
            rev: rev + 1,
        }));
        assert!(!hub.end_expired(u64::MAX));
        let rev = ack(c.call(Request::StopBoost { rev }));
        assert_eq!(snapshot(&mut c).config.boost, None);

        // Reset a mode to its template, then everything.
        let gaming = snailnet_core::modes::template("gaming").unwrap();
        let rev = ack(c.call(Request::SetRules {
            profile_id: "gaming".into(),
            rules: Vec::new(),
            rev,
        }));
        let rev = ack(c.call(Request::ResetProfile {
            id: "gaming".into(),
            rev,
        }));
        let snap = snapshot(&mut c);
        let p = snap
            .config
            .profiles
            .iter()
            .find(|p| p.id == "gaming")
            .unwrap();
        assert_eq!(p.rules, gaming);
        ack(c.call(Request::SetLine {
            line: Line {
                down_mbps: Some(25.0),
                ..Line::default()
            },
            rev,
        }));
        let rev = snapshot(&mut c).rev;
        ack(c.call(Request::ResetAll { rev }));
        assert_eq!(snapshot(&mut c).config, Config::default());
        server.stop();
    }

    #[test]
    fn pause_resume_and_block() {
        let (_t, _hub, server, pipe) = setup("pause");
        let mut c = Client::connect(&pipe, "t").unwrap();
        let ack = |r: Result<Reply, ClientError>| match r {
            Ok(Reply::Ack { rev }) => rev,
            r => panic!("{r:?}"),
        };
        let rev = ack(c.call(Request::Pause {
            minutes: Some(15),
            rev: 1,
        }));
        let until = snapshot(&mut c).config.pause.unwrap().until.unwrap();
        assert!(until >= unix_now() + 14 * 60);
        let rev = ack(c.call(Request::Resume { rev }));
        assert_eq!(snapshot(&mut c).config.pause, None);

        let rev = ack(c.call(Request::SetBlocked {
            family_id: "Steam".into(),
            blocked: true,
            rev,
        }));
        let rev = ack(c.call(Request::SetBlocked {
            family_id: "discord".into(),
            blocked: true,
            rev,
        }));
        assert_eq!(snapshot(&mut c).config.blocked, ["discord", "steam"]);
        ack(c.call(Request::SetBlocked {
            family_id: "steam".into(),
            blocked: false,
            rev,
        }));
        assert_eq!(snapshot(&mut c).config.blocked, ["discord"]);
        server.stop();
    }

    #[test]
    fn subscribe_streams_stats() {
        let (_t, hub, server, pipe) = setup("stats");
        hub.set_state(ServiceState::Unshaped);
        let mut c = Client::connect(&pipe, "t").unwrap();
        c.call(Request::Subscribe { stats_hz: 10 }).unwrap();
        let mut ticks = 0;
        while ticks < 3 {
            if let ServerMsg::Stats(t) = c.recv().unwrap() {
                assert_eq!(t.state, ServiceState::Unshaped);
                ticks += 1;
            }
        }
        server.stop();
    }

    #[test]
    fn requests_before_hello_are_refused_and_the_pipe_closed() {
        let (_t, _hub, server, pipe) = setup("hello");
        use std::os::windows::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .access_mode(snailnet_ipc::CLIENT_ACCESS)
            .open(&pipe)
            .unwrap();
        snailnet_ipc::write_frame(
            &mut f,
            &ClientMsg {
                id: 9,
                req: Request::GetSnapshot,
            },
        )
        .unwrap();
        let m: ServerMsg = snailnet_ipc::read_frame(&mut f).unwrap();
        assert!(matches!(
            m,
            ServerMsg::Reply {
                id: 9,
                reply: Reply::Error {
                    code: ErrorCode::Handshake,
                    ..
                }
            }
        ));
        let next: Result<ServerMsg, _> = snailnet_ipc::read_frame(&mut f);
        assert!(next.is_err(), "the service hung up");

        // A future protocol is refused too.
        let mut f = std::fs::OpenOptions::new()
            .access_mode(snailnet_ipc::CLIENT_ACCESS)
            .open(&pipe)
            .unwrap();
        let hello = Request::Hello {
            client_version: "x".into(),
            protocol: PROTOCOL + 1,
            ui_capabilities: vec![],
        };
        snailnet_ipc::write_frame(&mut f, &ClientMsg { id: 1, req: hello }).unwrap();
        let m: ServerMsg = snailnet_ipc::read_frame(&mut f).unwrap();
        assert!(matches!(
            m,
            ServerMsg::Reply {
                reply: Reply::Error {
                    code: ErrorCode::Handshake,
                    ..
                },
                ..
            }
        ));
        server.stop();
    }

    #[test]
    fn oversized_frames_drop_the_client_only() {
        let (_t, _hub, server, pipe) = setup("big");
        use std::io::Write;
        use std::os::windows::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .access_mode(snailnet_ipc::CLIENT_ACCESS)
            .open(&pipe)
            .unwrap();
        f.write_all(&(64u32 << 20).to_le_bytes()).unwrap();
        let r: Result<ServerMsg, _> = snailnet_ipc::read_frame(&mut f);
        assert!(r.is_err(), "the service hung up");
        // Others are unaffected.
        let mut c = Client::connect(&pipe, "t").unwrap();
        assert_eq!(snapshot(&mut c).rev, 1);
        server.stop();
    }

    #[test]
    fn a_second_server_on_the_same_name_is_refused() {
        let (_t, hub, server, pipe) = setup("twice");
        assert!(IpcServer::start(hub, &pipe).is_err());
        server.stop();
    }

    #[test]
    fn no_service_means_not_running() {
        match Client::connect(&pipe_name("absent"), "t") {
            Err(ClientError::NotRunning(_)) => {}
            r => panic!("{:?}", r.err()),
        }
    }

    #[test]
    fn stop_request_acks_and_signals_shutdown() {
        let (_t, hub, server, pipe) = setup("stop");
        // Before Hello: refused and must not stop the service.
        {
            use std::os::windows::fs::OpenOptionsExt;
            let mut f = std::fs::OpenOptions::new()
                .access_mode(snailnet_ipc::CLIENT_ACCESS)
                .open(&pipe)
                .unwrap();
            snailnet_ipc::write_frame(
                &mut f,
                &ClientMsg {
                    id: 1,
                    req: Request::Stop,
                },
            )
            .unwrap();
            let m: ServerMsg = snailnet_ipc::read_frame(&mut f).unwrap();
            assert!(matches!(
                m,
                ServerMsg::Reply {
                    id: 1,
                    reply: Reply::Error {
                        code: ErrorCode::Handshake,
                        ..
                    }
                }
            ));
            assert!(!hub.signal().is_requested());
        }

        // After Hello: acknowledged cleanly and triggers StopSignal.
        let mut c = Client::connect(&pipe, "t").unwrap();
        assert_eq!(c.call(Request::Stop).unwrap(), Reply::Ack { rev: 1 });
        assert!(hub.signal().is_requested());
        server.stop();
    }
}
