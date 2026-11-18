//! A blocking client: one request at a time, plus reading the stats/event
//! stream. Enough for the CLI; the tray app will have its own async one.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::time::{Duration, Instant};

use crate::{
    CLIENT_ACCESS, ClientMsg, ErrorCode, FrameError, PROTOCOL, Reply, Request, ServerMsg,
    read_frame, write_frame,
};

/// `ERROR_PIPE_BUSY`: every server instance is connected right now.
const PIPE_BUSY: i32 = 231;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("the SnailNet service isn't running (no pipe {0})")]
    NotRunning(String),
    #[error("not allowed to talk to the SnailNet service ({0})")]
    Denied(String),
    #[error("{0}")]
    Frame(#[from] FrameError),
    #[error("the service refused: {msg} ({code:?})")]
    Refused { code: ErrorCode, msg: String },
    #[error("unexpected reply from the service: {0}")]
    Unexpected(String),
    /// The pipe isn't owned by SYSTEM, Administrators or us: some other
    /// program created it, maybe to impersonate the service.
    #[error("the pipe {0} wasn't created by the SnailNet service; not talking to it")]
    Impostor(String),
    #[error(
        "the service closed the connection; only programs installed in the same folder as snailnet-service.exe may connect"
    )]
    Rejected,
}

pub struct Client {
    pipe: File,
    next_id: u32,
    /// Stats and events that arrived while waiting for a reply.
    backlog: VecDeque<ServerMsg>,
    /// From the Welcome.
    pub service_version: String,
}

impl Client {
    /// Connect to `pipe` (normally [`crate::PIPE_NAME`]) and say Hello.
    pub fn connect(pipe: &str, client_version: &str) -> Result<Self, ClientError> {
        Self::connect_with(pipe, client_version, &[])
    }

    /// [`Client::connect`], announcing `capabilities` (e.g.
    /// [`crate::CAP_NEW_APPS`]) in Hello.
    pub fn connect_with(
        pipe: &str,
        client_version: &str,
        capabilities: &[&str],
    ) -> Result<Self, ClientError> {
        let deadline = Instant::now() + Duration::from_secs(2);
        let file = loop {
            match OpenOptions::new().access_mode(CLIENT_ACCESS).open(pipe) {
                Ok(f) => break f,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    return Err(ClientError::NotRunning(pipe.into()));
                }
                Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                    return Err(ClientError::Denied(e.to_string()));
                }
                Err(e) if e.raw_os_error() == Some(PIPE_BUSY) && Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => return Err(FrameError::Io(e).into()),
            }
        };
        use std::os::windows::io::AsRawHandle;
        match snailnet_win::pipe::owner_is_trusted(file.as_raw_handle()) {
            Ok(true) => {}
            Ok(false) => return Err(ClientError::Impostor(pipe.into())),
            Err(e) => {
                return Err(ClientError::Denied(format!(
                    "reading the pipe's owner: {e}"
                )));
            }
        }
        let mut c = Self {
            pipe: file,
            next_id: 1,
            backlog: VecDeque::new(),
            service_version: String::new(),
        };
        let hello = c.call(Request::Hello {
            client_version: client_version.into(),
            protocol: PROTOCOL,
            ui_capabilities: capabilities.iter().map(|c| (*c).to_owned()).collect(),
        });
        // The service hangs up on clients it doesn't accept.
        let hello = match hello {
            Err(ClientError::Frame(FrameError::Io(_))) => return Err(ClientError::Rejected),
            r => r?,
        };
        match hello {
            Reply::Welcome {
                service_version, ..
            } => c.service_version = service_version,
            other => return Err(ClientError::Unexpected(format!("{other:?}"))),
        }
        Ok(c)
    }

    /// Send `req` and wait for its reply. A [`Reply::Error`] becomes
    /// [`ClientError::Refused`].
    pub fn call(&mut self, req: Request) -> Result<Reply, ClientError> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        write_frame(&mut self.pipe, &ClientMsg { id, req })?;
        loop {
            match read_frame(&mut self.pipe)? {
                ServerMsg::Reply { id: got, reply } if got == id => {
                    return match reply {
                        Reply::Error { code, msg } => Err(ClientError::Refused { code, msg }),
                        r => Ok(r),
                    };
                }
                ServerMsg::Reply { id: got, .. } => {
                    return Err(ClientError::Unexpected(format!(
                        "reply to request {got}, expected {id}"
                    )));
                }
                other => self.backlog.push_back(other),
            }
        }
    }

    /// The next stats tick or event (blocks).
    pub fn recv(&mut self) -> Result<ServerMsg, ClientError> {
        if let Some(m) = self.backlog.pop_front() {
            return Ok(m);
        }
        Ok(read_frame(&mut self.pipe)?)
    }
}
