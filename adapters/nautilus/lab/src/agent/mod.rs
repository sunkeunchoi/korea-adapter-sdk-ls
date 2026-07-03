//! Agent decision layer — native, wire-compatible core types (R1, R5, R9).
//!
//! This module borrows the *shape* of upstream `nautechsystems/nautilus_agents`
//! (built against nautilus 0.55) natively on our pinned 0.60 types: the lab does
//! not depend on the upstream crate (KTD1 — churn stays isolated here), but its
//! serde tag layout mirrors the upstream wire format exactly (KTD2 — envelopes
//! written by this lab remain readable by upstream-shaped tooling).
//!
//! U1–U4 substrate — later units add the research policy / replay on top of
//! these types:
//!
//! - [`envelope`] — the [`envelope::DecisionEnvelope`] record of one decision
//!   cycle, every stage explicit (never a fake `Approved`).
//! - [`intent`] — [`intent::AgentIntent`], what an agent *wants* done.
//! - [`action`] — [`action::RuntimeAction`], the lowered runtime form.
//! - [`context`] — [`context::AgentContext`], the state snapshot a decision was
//!   made against (purpose-built, never a serialized nautilus `Position`; R9).
//! - [`capability`] — the deny-by-default [`capability::CapabilitySet`] gating
//!   each intent (R3).
//! - [`guardrail`] / [`guardrails`] — the [`guardrail::IntentGuardrail`] seam
//!   and its concrete implementations (R4).
//! - [`policy`] — the [`policy::AgentPolicy`] seam and its
//!   [`policy::PolicyDecision`] outcome (R2).
//! - [`pipeline`] — the [`pipeline::DecisionPipeline`] running each decision
//!   through capability → guardrail → lowering, one envelope per cycle (R5).
//! - [`policies`] — concrete policies: the deterministic Research-tier
//!   demonstrator [`policies::ResearchPolicy`] (R8).
//! - [`recording`] — the cross-run decisions registry
//!   [`recording::DecisionRecorder`], append-only + scrubbed at write time
//!   (KTD5, R9).
//! - [`replay`] — engine-free guardrail-swap re-evaluation of a recorded
//!   stream, with the first-divergence audit boundary (R7).

pub mod action;
pub mod capability;
pub mod context;
pub mod envelope;
pub mod guardrail;
pub mod guardrails;
pub mod intent;
pub mod pipeline;
pub mod policies;
pub mod policy;
pub mod recording;
pub mod replay;

pub use action::*;
pub use capability::*;
pub use context::*;
pub use envelope::*;
pub use guardrail::*;
pub use guardrails::*;
pub use intent::*;
pub use pipeline::*;
pub use policies::*;
pub use policy::*;
pub use recording::*;
pub use replay::*;
