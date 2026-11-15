//! Small, safe wrappers over the Windows APIs SnailNet needs outside the
//! packet engine: process names, the PID-by-port tables, and a
//! high-resolution sleep. All `unsafe` FFI for these lives here.
#![cfg(windows)]

pub mod fs;
pub mod pipe;
pub mod ports;
pub mod process;
pub mod timer;
