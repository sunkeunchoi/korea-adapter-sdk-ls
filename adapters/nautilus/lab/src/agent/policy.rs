//! Policy seam — what decides, separated from what enforces (R2).
//!
//! An [`AgentPolicy`] looks at an [`AgentContext`] and produces a
//! [`PolicyDecision`]; the [`crate::agent::pipeline::DecisionPipeline`] then
//! runs that decision through capability → guardrail → lowering and records
//! every stage (R5). Per KTD3, ORB strategy decisions do **not** flow through
//! this seam as intents this increment — the Research policy (U6) is the one
//! producer.
//!
//! **Divergence from upstream:** upstream `nautilus_agents` policies return an
//! async `PolicyFuture` (they may call out to an LLM). The lab is offline and
//! deterministic, so [`AgentPolicy::evaluate`] is synchronous — same seam,
//! no executor.

use nautilus_core::UUID4;
use serde::{Deserialize, Serialize};

use crate::agent::context::AgentContext;
use crate::agent::intent::AgentIntent;

/// An [`AgentIntent`] stamped with a stable id at planning time (mirrors
/// upstream). The id correlates the intent across follow-up envelopes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlannedIntent {
    /// The intent's stable id, minted when the policy planned it.
    pub intent_id: UUID4,
    /// The planned intent itself.
    pub intent: AgentIntent,
}

impl PlannedIntent {
    /// Plan an intent, minting a fresh id.
    pub fn new(intent: AgentIntent) -> Self {
        PlannedIntent { intent_id: UUID4::new(), intent }
    }
}

impl From<AgentIntent> for PlannedIntent {
    fn from(intent: AgentIntent) -> Self {
        PlannedIntent::new(intent)
    }
}

/// Why a policy evaluation failed (recorded, never silently dropped — R5).
#[derive(Clone, Debug, PartialEq, thiserror::Error, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PolicyError {
    /// The policy exceeded its evaluation budget.
    #[error("policy evaluation timed out after {timeout_ms}ms")]
    Timeout {
        /// The exceeded budget in milliseconds.
        timeout_ms: u64,
    },
    /// The policy failed internally.
    #[error("policy internal error: {message}")]
    Internal {
        /// What went wrong.
        message: String,
    },
    /// The context lacked what the policy needed to decide.
    #[error("insufficient context: {message}")]
    InsufficientContext {
        /// What was missing.
        message: String,
    },
}

/// The runtime outcome of one policy evaluation. Mapped into the envelope's
/// [`crate::agent::envelope::PolicyDecisionRecord`] by the pipeline — this is
/// the in-process form, not the wire form.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum PolicyDecision {
    /// The policy produced an intent to execute.
    Execute(PlannedIntent),
    /// The policy decided to do nothing this cycle.
    NoAction,
    /// The policy itself failed.
    Failed(PolicyError),
}

impl PolicyDecision {
    /// An `Execute` decision over a freshly planned intent.
    pub fn execute(intent: AgentIntent) -> Self {
        PolicyDecision::Execute(PlannedIntent::new(intent))
    }
}

/// A policy: looks at the context a cycle was triggered under and decides
/// what, if anything, the agent wants done (R2).
///
/// Synchronous by design — see the module docs for the divergence from
/// upstream's async `PolicyFuture`.
pub trait AgentPolicy {
    /// The policy's stable name (recorded alongside its decisions).
    fn name(&self) -> &str;

    /// Evaluate the context and decide. Failures are returned as typed
    /// [`PolicyError`]s so the pipeline can record them (R5).
    fn evaluate(&self, context: &AgentContext) -> Result<PolicyDecision, PolicyError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proposal() -> AgentIntent {
        AgentIntent::ProposeParameterChange {
            strategy_id: "orb-v0".to_string(),
            parameter: "gap_min_pct".to_string(),
            current_value: 3.0,
            proposed_value: 4.0,
            rationale: "unit test".to_string(),
        }
    }

    #[test]
    fn planned_intent_mints_a_fresh_id_per_plan() {
        let a = PlannedIntent::new(proposal());
        let b = PlannedIntent::from(proposal());
        assert_eq!(a.intent, b.intent);
        assert_ne!(a.intent_id, b.intent_id, "each plan gets its own id");
    }

    #[test]
    fn execute_helper_wraps_a_planned_intent() {
        let decision = PolicyDecision::execute(proposal());
        let PolicyDecision::Execute(planned) = decision else {
            panic!("expected Execute");
        };
        assert_eq!(planned.intent, proposal());
    }

    #[test]
    fn policy_errors_display_their_payload_and_round_trip() {
        let all = vec![
            PolicyError::Timeout { timeout_ms: 250 },
            PolicyError::Internal { message: "boom".to_string() },
            PolicyError::InsufficientContext { message: "no run summary".to_string() },
        ];
        for err in all {
            let text = err.to_string();
            assert!(!text.is_empty());
            let line = serde_json::to_string(&err).unwrap();
            let back: PolicyError = serde_json::from_str(&line).unwrap();
            assert_eq!(back, err);
        }
        assert_eq!(
            PolicyError::Timeout { timeout_ms: 250 }.to_string(),
            "policy evaluation timed out after 250ms"
        );
    }
}
