//! Agent decision layer — native, wire-compatible core types (R1, R5, R9).
//!
//! This module borrows the *shape* of upstream `nautechsystems/nautilus_agents`
//! (built against nautilus 0.55) natively on our pinned 0.60 types: the lab does
//! not depend on the upstream crate (KTD1 — churn stays isolated here), but its
//! serde tag layout mirrors the upstream wire format exactly (KTD2 — envelopes
//! written by this lab remain readable by upstream-shaped tooling).
//!
//! U1 substrate only — later units add capability / guardrail / pipeline /
//! policy / replay on top of these types:
//!
//! - [`envelope`] — the [`envelope::DecisionEnvelope`] record of one decision
//!   cycle, every stage explicit (never a fake `Approved`).
//! - [`intent`] — [`intent::AgentIntent`], what an agent *wants* done.
//! - [`action`] — [`action::RuntimeAction`], the lowered runtime form.
//! - [`context`] — [`context::AgentContext`], the state snapshot a decision was
//!   made against (purpose-built, never a serialized nautilus `Position`; R9).

pub mod action;
pub mod context;
pub mod envelope;
pub mod intent;

pub use action::*;
pub use context::*;
pub use envelope::*;
pub use intent::*;
