//! Shared live-strategy hooks.
//!
//! The implementations remain at their established homes so introducing the daily
//! consumer does not move ORB's identity-bearing source.

pub use super::orb::{EmissionGate, MarkFeed};
pub use crate::runner::watchdog::Heartbeats;

