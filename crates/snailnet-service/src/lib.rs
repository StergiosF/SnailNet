//! The SnailNet service (PLAN.md §4, §13, §17): everything that needs admin
//! rights and must keep running.
//!
//! - [`paths`]: where the service keeps its files (`C:\ProgramData\SnailNet`).
//! - [`store`]: `config.toml` load/save, atomic, with a `.bak` fallback.
//! - [`crashguard`]: crash-loop detection → Safe mode (§6.4).
//! - [`logging`]: daily log files, 7 kept.
//! - [`shaper`]: WinDivert + classification + fast path, wired together
//!   (also used by `snailnet spike`).
//! - [`runner`]: the supervisor loop: load config, shape, fail open, retry,
//!   fall back to Safe mode.
//! - [`svc`], [`install`]: the Windows service itself.
#![cfg(windows)]

pub mod crashguard;
pub mod install;
pub mod logging;
pub mod paths;
pub mod runner;
pub mod shaper;
pub mod store;
pub mod svc;
