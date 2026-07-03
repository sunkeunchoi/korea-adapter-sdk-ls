//! Concrete [`crate::agent::guardrail::IntentGuardrail`] implementations (R4).
//!
//! One guardrail ships this increment — [`proposal_bounds`] gates the
//! research param-change intent (KTD3); the deferred risk-tier guardrails
//! slot in beside it unchanged.

pub mod proposal_bounds;

pub use proposal_bounds::*;
