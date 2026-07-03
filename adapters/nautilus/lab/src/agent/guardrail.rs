//! Guardrail seam — the trait a pipeline evaluates after the capability check
//! (R4). Concrete guardrails live in [`crate::agent::guardrails`].

use crate::agent::context::AgentContext;
use crate::agent::envelope::GuardrailResult;
use crate::agent::intent::AgentIntent;

/// A guardrail that approves or rejects an intent, with a recorded reason on
/// rejection (R4).
///
/// **Pure-function contract:** implementations must be pure per-cycle
/// functions of `(intent, context)` — same inputs, same result, no interior
/// state and no environment reads. This is what makes replay re-evaluate
/// recorded envelopes independently and engine-free (R7). Stateful guardrails
/// (cumulative drawdown, rate limits) are **out of contract** until replay is
/// redesigned for cross-cycle state (plan U3).
pub trait IntentGuardrail {
    /// The guardrail's stable name, used in recorded rejection reasons.
    fn name(&self) -> &str;

    /// Evaluate one intent against the context it was decided under. Returns
    /// [`GuardrailResult::Approved`] or [`GuardrailResult::Rejected`] with a
    /// reason naming this guardrail; never
    /// [`GuardrailResult::NotEvaluated`] (that representation belongs to the
    /// pipeline for stages that did not run).
    fn evaluate(&self, intent: &AgentIntent, context: &AgentContext) -> GuardrailResult;
}
