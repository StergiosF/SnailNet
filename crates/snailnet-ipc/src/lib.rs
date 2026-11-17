//! The SnailNet service ↔ app protocol (PLAN.md §12).
//!
//! Transport: the named pipe [`PIPE_NAME`] (byte mode). Each message is a
//! frame: a 4-byte little-endian length, then that many bytes of
//! MessagePack (structs as maps with field names, so fields can be added
//! without breaking older peers). Frames are at most [`MAX_FRAME`] bytes.
//!
//! The client sends [`ClientMsg`]s, each with an `id`; the service answers
//! each with one [`ServerMsg::Reply`] carrying the same `id`, and may send
//! [`ServerMsg::Stats`] and [`ServerMsg::Event`] at any time in between.
//! The first request must be [`Request::Hello`].
//!
//! Every mutating request carries the config revision it was based on
//! (`rev`); if the config changed since, the service refuses with
//! [`ErrorCode::Conflict`] instead of silently overwriting someone else's
//! change.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use snailnet_core::rules::{Config, Line, Rule, Target};

#[cfg(windows)]
mod client;
#[cfg(windows)]
pub use client::{Client, ClientError};

/// `\\.\pipe\snailnet.v1`.
pub const PIPE_NAME: &str = r"\\.\pipe\snailnet.v1";
/// This build's protocol version.
pub const PROTOCOL: u32 = 1;
/// The oldest protocol version the service still accepts (N and N-1).
pub const MIN_PROTOCOL: u32 = 1;
/// Largest frame either side sends or accepts.
pub const MAX_FRAME: usize = 1 << 20;

/// The pipe's DACL. NETWORK denied (listed first, so it wins); SYSTEM,
/// Administrators and the pipe's owner (`OW`: whoever created it — the
/// service, so it can add instances) full control; interactive users read
/// and write data but **not** `FILE_CREATE_PIPE_INSTANCE`, so they can't
/// stand up a fake server instance. `0x12019b` = `FILE_GENERIC_READ |
/// FILE_WRITE_DATA | FILE_WRITE_EA | FILE_WRITE_ATTRIBUTES`.
pub const PIPE_SDDL: &str = "D:P(D;;GA;;;NU)(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;OW)(A;;0x12019b;;;IU)";
/// What a client asks for when opening the pipe: exactly what [`PIPE_SDDL`]
/// grants interactive users (`GENERIC_WRITE` would include the
/// create-instance right and be refused).
pub const CLIENT_ACCESS: u32 = 0x0012_019b;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientMsg {
    pub id: u32,
    pub req: Request,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Request {
    Hello {
        client_version: String,
        protocol: u32,
        #[serde(default)]
        ui_capabilities: Vec<String>,
    },
    /// Config, state and known apps.
    GetSnapshot,
    /// Start a [`StatsTick`] stream at `stats_hz` (clamped to 1–10).
    Subscribe {
        stats_hz: u32,
    },
    Unsubscribe,
    /// Replace one profile's rules.
    SetRules {
        profile_id: String,
        rules: Vec<Rule>,
        rev: u64,
    },
    SwitchProfile {
        id: String,
        rev: u64,
    },
    /// Set the line speed. Until the estimator exists (§10) this is how
    /// shaping gets a rate; a direction without a speed isn't shaped.
    SetLine {
        line: Line,
        rev: u64,
    },
    /// Put a profile's rules back to where they started: a mode's template
    /// ([`snailnet_core::modes`]), or none for any other profile.
    ResetProfile {
        id: String,
        rev: u64,
    },
    /// Forget everything (rules, line speed, settings, boost): the default
    /// config, as on a fresh install.
    ResetAll {
        rev: u64,
    },
    /// `target` goes first for `minutes` (`None`: until stopped). Replaces
    /// any boost already on.
    Boost {
        target: Target,
        minutes: Option<u32>,
        rev: u64,
    },
    StopBoost {
        rev: u64,
    },
    // Defined so the protocol is stable; the service answers Unsupported
    // until the features exist (§11).
    SetBlocked {
        family_id: String,
        blocked: bool,
        rev: u64,
    },
    FocusChanged {
        pid: u32,
        is_game: bool,
        confidence: f32,
    },
    /// Step aside (no shaping, no blocking) for `minutes`, or until
    /// [`Request::Resume`] (`None`).
    Pause {
        minutes: Option<u32>,
        rev: u64,
    },
    Resume {
        rev: u64,
    },
    RunSpeedTest,
    AnswerPrompt {
        prompt_id: u64,
        action: String,
    },
}

impl Request {
    /// Short name for logs.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Hello { .. } => "Hello",
            Self::GetSnapshot => "GetSnapshot",
            Self::Subscribe { .. } => "Subscribe",
            Self::Unsubscribe => "Unsubscribe",
            Self::SetRules { .. } => "SetRules",
            Self::SwitchProfile { .. } => "SwitchProfile",
            Self::SetLine { .. } => "SetLine",
            Self::ResetProfile { .. } => "ResetProfile",
            Self::ResetAll { .. } => "ResetAll",
            Self::Boost { .. } => "Boost",
            Self::StopBoost { .. } => "StopBoost",
            Self::SetBlocked { .. } => "SetBlocked",
            Self::FocusChanged { .. } => "FocusChanged",
            Self::Pause { .. } => "Pause",
            Self::Resume { .. } => "Resume",
            Self::RunSpeedTest => "RunSpeedTest",
            Self::AnswerPrompt { .. } => "AnswerPrompt",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServerMsg {
    Reply { id: u32, reply: Reply },
    Stats(StatsTick),
    Event(Event),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Reply {
    Welcome {
        service_version: String,
        protocol: u32,
        state: ServiceState,
    },
    Snapshot(Box<Snapshot>),
    /// Done; the config is now at `rev`.
    Ack {
        rev: u64,
    },
    Error {
        code: ErrorCode,
        msg: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    /// The first request wasn't Hello, or Hello's protocol isn't supported.
    Handshake,
    /// `rev` is stale: someone else changed the config. Fetch and retry.
    Conflict,
    /// The change would make an invalid config (message says why).
    Invalid,
    /// Not available in this version of the service.
    Unsupported,
    /// The service couldn't do it (e.g. couldn't save the file).
    Internal,
}

/// What the service is doing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceState {
    /// Loading the config.
    Starting,
    Shaping,
    /// No line speed set: watching only.
    Unshaped,
    /// Shaping failed open (the network kept working); trying again soon.
    Retrying {
        reason: String,
    },
    /// Too many crashes or fail-opens recently: not shaping until restarted.
    SafeMode {
        failures: u32,
    },
    /// The user paused SnailNet: nothing shaped or blocked until `until`
    /// (Unix seconds), or until resumed (`None`).
    Paused {
        until: Option<u64>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub rev: u64,
    pub state: ServiceState,
    pub config: Config,
    /// Apps seen since shaping last started.
    pub apps: Vec<AppInfo>,
    /// Every site a rule may name: the shipped pack plus the user's own,
    /// sorted by name. Missing from older services.
    #[serde(default)]
    pub sites: Vec<SiteInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteInfo {
    /// What a site rule names (`youtube`).
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInfo {
    /// What a family rule names (`steam`).
    pub family: String,
    pub name: String,
    /// Lowercase exe file name; empty for System.
    pub exe: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatsTick {
    /// Milliseconds since the service started.
    pub t_ms: u64,
    pub state: ServiceState,
    /// `None`: that direction isn't shaped.
    pub down: Option<DirStats>,
    pub up: Option<DirStats>,
    /// How flows got their site names since the previous tick (PLAN.md
    /// §8.4), both directions. `None` from a service that doesn't report
    /// it, or before the second sample.
    #[serde(default)]
    pub names: Option<NameTick>,
}

/// Packets by how their flow was named (PLAN.md §8.4). Flows are named only
/// while a site rule exists, so all zero otherwise.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameTick {
    /// TLS or QUIC server name.
    pub sni: u64,
    /// The IP → domain cache (DNS answers).
    pub dns: u64,
    /// Encrypted Client Hello hid the name and DNS didn't know it.
    pub ech: u64,
    /// No usable name.
    pub none: u64,
    /// Still reading the flow's hello.
    pub naming: u64,
}

impl NameTick {
    /// Packets whose flow couldn't be identified (ECH or nothing usable).
    pub fn unidentified(&self) -> u64 {
        self.ech + self.none
    }

    /// `self - before`, per field (counters that went backwards give 0).
    pub fn since(&self, before: &NameTick) -> NameTick {
        NameTick {
            sni: self.sni.saturating_sub(before.sni),
            dns: self.dns.saturating_sub(before.dns),
            ech: self.ech.saturating_sub(before.ech),
            none: self.none.saturating_sub(before.none),
            naming: self.naming.saturating_sub(before.naming),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirStats {
    /// The rate the scheduler releases at (line speed × factor).
    pub shaped_bps: u64,
    /// Rule classes in order, then Everything else.
    pub classes: Vec<ClassTick>,
    /// App families that sent anything since the previous tick, busiest
    /// first (PLAN.md §12.2 `per_family`). Traffic whose app isn't known
    /// (yet) is the family `unknown`.
    #[serde(default)]
    pub families: Vec<FamilyTick>,
    /// Sites that sent anything since the previous tick, one row per
    /// (site, family) so a UI can nest a site under its browser ("Brave ▸
    /// YouTube"), busiest first. Only while a site rule exists (flows are
    /// named only then).
    #[serde(default)]
    pub sites: Vec<SiteTick>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteTick {
    /// What a site rule names (`youtube`).
    pub site: String,
    pub name: String,
    /// The app family the traffic belongs to (`unknown` if not known).
    pub family: String,
    /// Bits per second sent since the previous tick.
    pub bps: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyTick {
    /// What a family rule names (`steam`).
    pub family: String,
    pub name: String,
    /// Bits per second sent since the previous tick.
    pub bps: u64,
    /// Part of `bps` in HTTPS connections whose site couldn't be identified
    /// (ECH, or no name in the hello or DNS; PLAN.md §8.4). Only while a
    /// site rule exists.
    #[serde(default)]
    pub unidentified_bps: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassTick {
    pub name: String,
    /// Bits per second sent since the previous tick.
    pub bps: u64,
    /// Packets dropped since the previous tick (CoDel or queue limit).
    pub drops: u64,
    pub queued_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
    StateChanged(ServiceState),
    /// The config changed (by any client); fetch a snapshot to see it.
    ConfigChanged {
        rev: u64,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("frame of {0} bytes is larger than the 1 MB limit")]
    TooLarge(usize),
    #[error("could not encode a message: {0}")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("could not decode a message: {0}")]
    Decode(#[from] rmp_serde::decode::Error),
}

/// A message's frame body (MessagePack, structs as maps).
pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, FrameError> {
    let body = rmp_serde::to_vec_named(msg)?;
    if body.len() > MAX_FRAME {
        return Err(FrameError::TooLarge(body.len()));
    }
    Ok(body)
}

pub fn decode<T: DeserializeOwned>(body: &[u8]) -> Result<T, FrameError> {
    Ok(rmp_serde::from_slice(body)?)
}

/// The 4-byte prefix for a body of `len` bytes, or an error if too large.
pub fn frame_len(len: usize) -> Result<[u8; 4], FrameError> {
    if len > MAX_FRAME {
        return Err(FrameError::TooLarge(len));
    }
    Ok((len as u32).to_le_bytes())
}

/// Parse a length prefix, refusing oversized frames before allocating.
pub fn parse_len(prefix: [u8; 4]) -> Result<usize, FrameError> {
    let len = u32::from_le_bytes(prefix) as usize;
    if len > MAX_FRAME {
        return Err(FrameError::TooLarge(len));
    }
    Ok(len)
}

/// Write one framed message (blocking).
pub fn write_frame<W: Write, T: Serialize>(w: &mut W, msg: &T) -> Result<(), FrameError> {
    let body = encode(msg)?;
    let mut buf = Vec::with_capacity(4 + body.len());
    buf.extend_from_slice(&frame_len(body.len())?);
    buf.extend_from_slice(&body);
    // One write per frame, so a frame is never interleaved or split by us.
    w.write_all(&buf)?;
    w.flush()?;
    Ok(())
}

/// Read one framed message (blocking).
pub fn read_frame<R: Read, T: DeserializeOwned>(r: &mut R) -> Result<T, FrameError> {
    let mut prefix = [0u8; 4];
    r.read_exact(&mut prefix)?;
    let mut body = vec![0u8; parse_len(prefix)?];
    r.read_exact(&mut body)?;
    decode(&body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use snailnet_core::rules::{Ceiling, Limit, Target};
    use snailnet_core::scheduler::Priority;

    fn rule() -> Rule {
        Rule {
            target: Target::family("steam"),
            share: 4000,
            priority: Priority::Low,
            ceiling: Some(Ceiling {
                down: Some(Limit::Percent(80)),
                up: None,
            }),
            enabled: true,
        }
    }

    fn roundtrip<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(v: T) {
        let mut buf = Vec::new();
        write_frame(&mut buf, &v).unwrap();
        assert_eq!(
            u32::from_le_bytes(buf[..4].try_into().unwrap()) as usize,
            buf.len() - 4
        );
        let back: T = read_frame(&mut buf.as_slice()).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn requests_roundtrip() {
        for req in [
            Request::Hello {
                client_version: "0.0.1".into(),
                protocol: PROTOCOL,
                ui_capabilities: vec!["toasts".into()],
            },
            Request::GetSnapshot,
            Request::Subscribe { stats_hz: 4 },
            Request::SetRules {
                profile_id: "gaming".into(),
                rules: vec![rule()],
                rev: 7,
            },
            Request::SetLine {
                line: Line {
                    down_mbps: Some(25.0),
                    up_mbps: None,
                    factor: None,
                },
                rev: 1,
            },
            Request::Pause {
                minutes: None,
                rev: 3,
            },
        ] {
            roundtrip(ClientMsg { id: 3, req });
        }
    }

    #[test]
    fn server_messages_roundtrip() {
        let mut config = Config::default();
        config.profiles[0].rules.push(rule());
        config.line.down_mbps = Some(25.0);
        for msg in [
            ServerMsg::Reply {
                id: 1,
                reply: Reply::Snapshot(Box::new(Snapshot {
                    rev: 4,
                    state: ServiceState::SafeMode { failures: 3 },
                    config,
                    apps: vec![AppInfo {
                        family: "steam".into(),
                        name: "Steam".into(),
                        exe: "steam.exe".into(),
                    }],
                    sites: vec![SiteInfo {
                        id: "youtube".into(),
                        name: "YouTube".into(),
                    }],
                })),
            },
            ServerMsg::Reply {
                id: 2,
                reply: Reply::Error {
                    code: ErrorCode::Conflict,
                    msg: "changed".into(),
                },
            },
            ServerMsg::Stats(StatsTick {
                t_ms: 250,
                state: ServiceState::Shaping,
                down: Some(DirStats {
                    shaped_bps: 23_000_000,
                    classes: vec![ClassTick {
                        name: "family:steam".into(),
                        bps: 9_000_000,
                        drops: 2,
                        queued_bytes: 3000,
                    }],
                    families: vec![FamilyTick {
                        family: "brave".into(),
                        name: "Brave".into(),
                        bps: 8_500_000,
                        unidentified_bps: 500_000,
                    }],
                    sites: vec![SiteTick {
                        site: "youtube".into(),
                        name: "YouTube".into(),
                        family: "brave".into(),
                        bps: 7_000_000,
                    }],
                }),
                up: None,
                names: Some(NameTick {
                    sni: 900,
                    dns: 40,
                    ech: 12,
                    none: 3,
                    naming: 8,
                }),
            }),
            ServerMsg::Event(Event::ConfigChanged { rev: 5 }),
        ] {
            roundtrip(msg);
        }
    }

    /// A tick from a service built before per-site rows and name counts
    /// still decodes (the new fields default), and a new tick decodes in an
    /// old client (unknown fields are skipped).
    #[test]
    fn stats_ticks_stay_compatible_both_ways() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct OldFamily {
            family: String,
            name: String,
            bps: u64,
        }
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct OldDir {
            shaped_bps: u64,
            classes: Vec<ClassTick>,
            families: Vec<OldFamily>,
        }
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct OldTick {
            t_ms: u64,
            state: ServiceState,
            down: Option<OldDir>,
            up: Option<OldDir>,
        }
        let old = OldTick {
            t_ms: 5,
            state: ServiceState::Shaping,
            down: Some(OldDir {
                shaped_bps: 1,
                classes: vec![],
                families: vec![OldFamily {
                    family: "steam".into(),
                    name: "Steam".into(),
                    bps: 2,
                }],
            }),
            up: None,
        };
        let new: StatsTick = decode(&encode(&old).unwrap()).unwrap();
        assert_eq!(new.names, None);
        let d = new.down.as_ref().unwrap();
        assert!(d.sites.is_empty());
        assert_eq!(d.families[0].unidentified_bps, 0);

        let mut tick = new;
        tick.names = Some(NameTick::default());
        tick.down.as_mut().unwrap().sites.push(SiteTick {
            site: "youtube".into(),
            name: "YouTube".into(),
            family: "steam".into(),
            bps: 1,
        });
        let back: OldTick = decode(&encode(&tick).unwrap()).unwrap();
        assert_eq!(back, old);
    }

    #[test]
    fn name_ticks_subtract_per_field() {
        let a = NameTick {
            sni: 10,
            dns: 5,
            ech: 3,
            none: 2,
            naming: 1,
        };
        let b = NameTick {
            sni: 15,
            dns: 5,
            ech: 4,
            none: 4,
            naming: 0,
        };
        let d = b.since(&a);
        assert_eq!(
            d,
            NameTick {
                sni: 5,
                dns: 0,
                ech: 1,
                none: 2,
                naming: 0
            }
        );
        assert_eq!(d.unidentified(), 3);
    }

    #[test]
    fn oversized_frames_are_refused_before_reading() {
        let mut buf = ((MAX_FRAME + 1) as u32).to_le_bytes().to_vec();
        buf.extend_from_slice(&[0; 16]);
        let r: Result<ServerMsg, _> = read_frame(&mut buf.as_slice());
        assert!(matches!(r, Err(FrameError::TooLarge(_))));
    }

    #[test]
    fn junk_is_a_decode_error_not_a_panic() {
        let mut buf = 3u32.to_le_bytes().to_vec();
        buf.extend_from_slice(&[0xc1, 0xff, 0x00]);
        let r: Result<ClientMsg, _> = read_frame(&mut buf.as_slice());
        assert!(matches!(r, Err(FrameError::Decode(_))));
    }

    #[test]
    fn truncated_frame_is_an_io_error() {
        let mut buf = 10u32.to_le_bytes().to_vec();
        buf.extend_from_slice(&[0x90]);
        let r: Result<ClientMsg, _> = read_frame(&mut buf.as_slice());
        assert!(matches!(r, Err(FrameError::Io(_))));
    }
}
