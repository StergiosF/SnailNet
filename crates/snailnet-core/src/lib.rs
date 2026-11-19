//! SnailNet core: everything that can be tested without Windows.
//!
//! This crate has no OS dependencies (PLAN.md §5). The scheduler is driven
//! by a [`clock::Clock`] so tests can run it on simulated time.

pub mod calls;
pub mod classify;
pub mod clock;
pub mod estimator;
pub mod families;
pub mod games;
pub mod modes;
pub mod networks;
pub mod packs;
pub mod parse;
pub mod rules;
pub mod scheduler;
pub mod sites;
pub mod sniff;
