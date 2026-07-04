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

/// A comparison tolerance so float dust at the bound does not deny an intended
/// on-bound step (KTD3/AE2). A loop turn accumulates rounding — e.g. `3.0 * 0.8`
/// stores `2.4000000000000004`, so a subsequent clean half-step to `1.2`
/// computes a relative change of `0.5000000000000001`, which a bare `<=` would
/// reject by 1e-16 while the guardrail's own `{:.4}` reason prints
/// "0.5000 exceeds bound 0.5000". The tolerance enforces the bound at the
/// precision it is displayed and specified in (the 0.5 policy is not a
/// 0.5000000000000000-exact policy); it is far smaller than any intentional
/// proposal delta, so a genuinely over-bound change still rejects. NaN and the
/// zero-current INFINITY still fail closed (they are not within
/// `bound + epsilon` either).
const BOUND_EPSILON: f64 = 1e-9;

/// Rejects [`AgentIntent::ProposeParameterChange`] intents whose relative
/// change exceeds `max_relative_change` (within [`BOUND_EPSILON`]); approves
/// every other intent.
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
        // Fail closed on non-finite — on EITHER side of the comparison:
        // `partial_cmp` is `None` when either operand is NaN, so a NaN
        // proposal (reachable via a NaN param in a manifest or a
        // mis-configured policy factor), the zero-current INFINITY, and a
        // mis-configured NaN bound all fall outside `Less | Equal` and
        // reject; a plain `>` check would silently APPROVE the NaN cases.
        let within_bound = matches!(
            relative_change.partial_cmp(&(self.max_relative_change + BOUND_EPSILON)),
            Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
        );
        if !within_bound {
            GuardrailResult::Rejected {
                reason: format!(
                    "{}: parameter '{parameter}' relative change {relative_change:.4} exceeds \
                     bound {:.4} (current {current_value:.4}, proposed {proposed_value:.4})",
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
        // The bound is {:.4}-formatted like every other f64 in the reason —
        // a raw Display of a non-round bound (1.0/3.0) would emit a 16-digit
        // run the write-time scrub masks to `0.***`.
        assert!(reason.contains("bound 0.5000"), "names the bound, scrub-safe: {reason}");
        assert!(reason.contains("relative change 1"), "names the offending change: {reason}");
    }

    #[test]
    fn float_dust_at_the_bound_is_approved_not_denied_by_1e16() {
        // A loop turn accumulates rounding: turn 1's 3.0 * 0.8 stores
        // 2.4000000000000004, so the intended clean half-step to 1.2 computes a
        // relative change of 0.5000000000000001 — a bare `<=` would reject it
        // (and print the absurd "0.5000 exceeds bound 0.5000"). The bound
        // tolerance approves it; the 0.5 policy is not a 0.5-exact policy.
        let noisy_current = 3.0 * 0.8; // == 2.4000000000000004
        let result = guardrail().evaluate(&proposal(noisy_current, 1.2), &context());
        assert_eq!(result, GuardrailResult::Approved, "on-bound half-step must not be denied by float dust");
        // A genuinely over-bound change is still rejected (the tolerance is dust-sized).
        let over = guardrail().evaluate(&proposal(2.4, 1.0), &context()); // rel change 0.5833
        assert!(matches!(over, GuardrailResult::Rejected { .. }), "genuine over-bound still rejects: {over:?}");
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

    #[test]
    fn nan_bound_is_rejected_fail_closed() {
        // A mis-configured NaN BOUND must not fail open: `x <= NaN` is false,
        // so the `!(x <= bound)` form rejects every proposal under it (a
        // plain `x > NaN` comparison would approve them all).
        let broken = ProposalBoundsGuardrail { max_relative_change: f64::NAN };
        let result = broken.evaluate(&proposal(3.0, 3.1), &context());
        assert!(matches!(result, GuardrailResult::Rejected { .. }), "got {result:?}");
    }

    #[test]
    fn nan_proposal_is_rejected_fail_closed() {
        // `NaN > bound` is false — a plain `>` check approves NaN. The
        // `!(x <= bound)` form must reject a NaN on either side.
        for (current, proposed) in [(f64::NAN, 4.0), (3.0, f64::NAN), (f64::NAN, f64::NAN)] {
            let result = guardrail().evaluate(&proposal(current, proposed), &context());
            assert!(
                matches!(result, GuardrailResult::Rejected { .. }),
                "NaN must fail closed, got {result:?}"
            );
        }
    }

    #[test]
    fn rejection_reason_uses_fixed_precision_scrub_safe_numbers() {
        // Raw f64 Display can emit 16-digit runs (2.4000000000000004) that the
        // write-time scrub masks as account-like; {:.4} keeps digit runs short.
        let result = guardrail().evaluate(&proposal(3.0, 3.0 * 0.8), &context());
        let GuardrailResult::Rejected { reason } = guardrail_tight().evaluate(
            &proposal(3.0, 3.0 * 0.8),
            &context(),
        ) else {
            panic!("tight bound must reject");
        };
        assert!(reason.contains("2.4000"), "fixed precision on the wire: {reason}");
        assert!(!reason.contains("2.4000000000000004"), "no raw f64 runs: {reason}");
        // The ±50% default approves the same proposal (sanity of the fixture).
        assert_eq!(result, GuardrailResult::Approved);
    }

    fn guardrail_tight() -> ProposalBoundsGuardrail {
        ProposalBoundsGuardrail { max_relative_change: 0.1 }
    }
}
