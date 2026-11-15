//! SnailNet core: everything that can be tested without Windows.
//!
//! This crate has no OS dependencies (PLAN.md §5). The scheduler is driven
//! by a [`clock::Clock`] so tests can run it on simulated time.

pub mod classify;
pub mod clock;
pub mod parse;
pub mod rules;
pub mod scheduler;
pub mod sites;
pub mod sniff;
