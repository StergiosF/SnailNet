//! The SnailNet service (PLAN.md §4, §13, §17): everything that needs admin
//! rights and must keep running.
//!
//! - [`autorate`]: the live line speed (estimator + ping probe, §10.2).
//! - [`paths`]: where the service keeps its files (`C:\ProgramData\SnailNet`).
//! - [`store`]: `config.toml` load/save, atomic, with a `.bak` fallback.
//! - [`crashguard`]: crash-loop detection → Safe mode (§6.4).
//! - [`logging`]: daily log files, 7 kept.
//! - [`shaper`]: WinDivert + classification + fast path, wired together
//!   (also used by `snailnet spike`).
//! - [`runner`]: the supervisor loop: load config, shape, fail open, retry,
//!   fall back to Safe mode.
//! - [`speedtest`]: the speed test against Cloudflare (§10.1).
//! - [`tags`]: what the fast path's per-packet tag means (app + site).
//! - [`hub`], [`ipc`]: shared state and the named-pipe server for the app
//!   and CLI (§12).
//! - [`svc`], [`install`]: the Windows service itself.
#![cfg(windows)]

pub mod autorate;
pub mod crashguard;
pub mod focus;
pub mod hub;
pub mod install;
pub mod ipc;
pub mod logging;
pub mod netwatch;
pub mod newapps;
pub mod packs;
pub mod paths;
pub mod refuse;
pub mod runner;
pub mod shaper;
pub mod speedtest;
pub mod store;
pub mod svc;
pub mod tags;
