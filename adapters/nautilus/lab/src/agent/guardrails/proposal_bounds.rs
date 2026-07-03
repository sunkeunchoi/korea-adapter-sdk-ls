//! Proposal-bounds guardrail — rejects a research parameter-change proposal
//! whose relative magnitude falls outside the configured bound (R4, KTD3).
//!
//! Mirrors upstream's `guardrails/max_drawdown.rs` shape: approve every
//! non-matching intent first (a guardrail only gates its domain), reject with
//! a formatted reason naming the guardrail, the parameter, the bound, and the
//! offending relative change.

use crate::agent::context::AgentContext;
use crate::agent::envelope::GuardrailResult;
use crate::agent::guardrail::IntentGuardrail;
use crate::agent::intent::AgentIntent;

/// Rejects [`AgentIntent::ProposeParameterChange`] intents whose relative
/// change exceeds `max_relative_change`; approves every other intent.
///
/// Pure per-cycle function of the intent alone (the context is unused) — see
/// the [`IntentGuardrail`] pure-function contract.
#[derive(Clone, Debug, PartialEq)]
pub struct ProposalBoundsGuardrail {
    /// The maximum allowed `|proposed - current| / |current|`
    /// (e.g. `0.5` = ±50%). Any change away from a current value of exactly
    /// `0.0` is treated as out of bounds (relative change is undefined there),
    /// unless the proposal is also `0.0` (no change).
    pub max_relative_change: f64,
}

impl IntentGuardrail for ProposalBoundsGuardrail {
    fn name(&self) -> &str {
        "proposal_bounds"
    }

    fn evaluate(&self, intent: &AgentIntent, _context: &AgentContext) -> GuardrailResult {
        // A guardrail only gates its domain: everything but a parameter-change
        // proposal is approved untouched.
        let AgentIntent::ProposeParameterChange {
            parameter, current_value, proposed_value, ..
        } = intent
        else {
            return GuardrailResult::Approved;
        };
        let relative_change = if *current_value == 0.0 {
            // Relative change from zero is undefined; any real change is out
            // of bounds, a zero-to-zero "change" is none at all.
            if *proposed_value == 0.0 {
                0.0
            } else {
                f64::INFINITY
            }
        } else {
            ((proposed_value - current_value) / current_value).abs()
        };
        if relative_change > self.max_relative_change {
            GuardrailResult::Rejected {
                reason: format!(
                    "{}: parameter '{parameter}' relative change {relative_change} exceeds \
                     bound {} (current {current_value}, proposed {proposed_value})",
                    self.name(),
                    self.max_relative_change,
                ),
            }
        } else {
            GuardrailResult::Approved
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guardrail() -> ProposalBoundsGuardrail {
        ProposalBoundsGuardrail { max_relative_change: 0.5 }
    }

    fn context() -> AgentContext {
        AgentContext::telemetry("orb-v0", 1, Default::default(), Default::default())
    }

    fn proposal(current_value: f64, proposed_value: f64) -> AgentIntent {
        AgentIntent::ProposeParameterChange {
            strategy_id: "orb-v0".to_string(),
            parameter: "gap_min_pct".to_string(),
            current_value,
            proposed_value,
            rationale: "unit test".to_string(),
        }
    }

    #[test]
    fn proposal_within_bounds_is_approved() {
        // 3.0 -> 4.0 is a +33% change, inside the ±50% bound.
        let result = guardrail().evaluate(&proposal(3.0, 4.0), &context());
        assert_eq!(result, GuardrailResult::Approved);
    }

    #[test]
    fn proposal_outside_bounds_is_rejected_naming_bound_and_offending_change() {
        // 3.0 -> 6.0 is a +100% change, outside the ±50% bound.
        let result = guardrail().evaluate(&proposal(3.0, 6.0), &context());
        let GuardrailResult::Rejected { reason } = result else {
            panic!("expected rejection, got {result:?}");
        };
        assert!(reason.contains("proposal_bounds"), "names the guardrail: {reason}");
        assert!(reason.contains("gap_min_pct"), "names the parameter: {reason}");
        assert!(reason.contains("bound 0.5"), "names the bound: {reason}");
        assert!(reason.contains("relative change 1"), "names the offending change: {reason}");
    }

    #[test]
    fn non_research_intent_is_approved_untouched() {
        // The guardrail only gates its domain.
        let intent = AgentIntent::EscalateToHuman { reason: "ambiguous fill state".to_string() };
        let result = guardrail().evaluate(&intent, &context());
        assert_eq!(result, GuardrailResult::Approved);
    }

    #[test]
    fn any_change_away_from_zero_is_rejected() {
        // Relative change from a current value of 0.0 is undefined -> out of
        // bounds regardless of magnitude.
        let result = guardrail().evaluate(&proposal(0.0, 0.001), &context());
        assert!(matches!(result, GuardrailResult::Rejected { .. }), "got {result:?}");
    }

    #[test]
    fn zero_to_zero_proposal_is_approved() {
        let result = guardrail().evaluate(&proposal(0.0, 0.0), &context());
        assert_eq!(result, GuardrailResult::Approved);
    }
}
