//! Parsers for bytes that come off the wire. Everything here reads
//! attacker-controlled input, so it is bounds-checked and free of `unsafe`
//! (PLAN.md §18.2).

pub mod dns;
pub mod ip;
pub mod quic;
pub mod tls;
