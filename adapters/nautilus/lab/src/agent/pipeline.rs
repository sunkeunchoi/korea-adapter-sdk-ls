//! Decision pipeline — runs one policy decision through capability →
//! guardrail → lowering and emits exactly one [`DecisionEnvelope`] per cycle
//! (R2, R5).
//!
//! The no-gaps invariant (R5): **every** path through [`DecisionPipeline::run`]
//! returns exactly one envelope, and stages that did not run carry an explicit
//! `NotEvaluated` — never a fake approval. Per KTD3, ORB strategy decisions do
//! **not** flow through this pipeline as intents this increment; the Research
//! policy (U6) is the producer. Scrubbing (R9) happens at write time in the
//! recording layer (U6) — the pipeline builds typed data only.

use nautilus_core::UUID4;

use crate::agent::action::RuntimeAction;
use crate::agent::capability::CapabilitySet;
use crate::agent::context::AgentContext;
use crate::agent::envelope::{
    CapabilityOutcome, DecisionEnvelope, DecisionTrigger, GuardrailResult, LoweringOutcome,
    PolicyDecisionRecord, ENVELOPE_SCHEMA_VERSION,
};
use crate::agent::guardrail::IntentGuardrail;
use crate::agent::intent::AgentIntent;
use crate::agent::policy::PolicyDecision;

/// Lower an approved intent to its runtime form. Total over the defined
/// [`AgentIntent`] variants — lowering cannot fail today, so the pipeline
/// always records [`LoweringOutcome::Success`] when it reaches this stage;
/// [`LoweringOutcome::Failed`] stays on the wire for future intents that
/// cannot lower. Research intents lower to a recorded
/// [`RuntimeAction::ResearchCommand`] (never an order); management-tier
/// intents lower to a [`RuntimeAction::ManagementCommand`] placeholder.
pub fn lower(intent: &AgentIntent) -> RuntimeAction {
    match intent {
        AgentIntent::ProposeParameterChange {
            strategy_id, parameter, current_value, proposed_value, rationale,
        } => RuntimeAction::ResearchCommand {
            // {:.4}: raw f64 Display can emit 16-digit runs (3.0 * 0.8 Displays
            // as 2.4000000000000004) that the write-time free-text scrub masks
            // as account-like, corrupting the recorded audit text — the same
            // discipline research.rs applies to the rationale.
            description: format!(
                "propose {strategy_id} parameter '{parameter}' {current_value:.4} -> \
                 {proposed_value:.4}: {rationale}"
            ),
        },
        AgentIntent::ReducePosition { instrument_id, target_quantity, reason } => {
            RuntimeAction::ManagementCommand {
                description: format!("reduce {instrument_id} to {target_quantity} ({reason})"),
            }
        }
        AgentIntent::ClosePosition { instrument_id, reason } => {
            RuntimeAction::ManagementCommand {
                description: format!("close {instrument_id} ({reason})"),
            }
        }
        AgentIntent::CancelOrder { instrument_id, client_order_id, reason } => {
            RuntimeAction::ManagementCommand {
                description: format!("cancel {client_order_id} on {instrument_id} ({reason})"),
            }
        }
        AgentIntent::CancelAllOrders { instrument_id, reason } => {
            RuntimeAction::ManagementCommand {
                description: format!("cancel all orders on {instrument_id} ({reason})"),
            }
        }
        AgentIntent::PauseStrategy { strategy_id, reason } => {
            RuntimeAction::ManagementCommand {
                description: format!("pause {strategy_id} ({reason})"),
            }
        }
        AgentIntent::ResumeStrategy { strategy_id, reason } => {
            RuntimeAction::ManagementCommand {
                description: format!("resume {strategy_id} ({reason})"),
            }
        }
        AgentIntent::AdjustRiskLimits { description } => {
            RuntimeAction::ManagementCommand {
                description: format!("adjust risk limits: {description}"),
            }
        }
        AgentIntent::EscalateToHuman { reason } => {
            RuntimeAction::ManagementCommand {
                description: format!("escalate to human: {reason}"),
            }
        }
    }
}

/// The ordered enforcement stages a policy decision runs through: the
/// deny-by-default [`CapabilitySet`] first (R3), then each
/// [`IntentGuardrail`] in registration order (R4), then lowering.
pub struct DecisionPipeline {
    /// The deny-by-default grant set checked before any guardrail runs.
    capabilities: CapabilitySet,
    /// Guardrails, evaluated in order; the first rejection short-circuits.
    guardrails: Vec<Box<dyn IntentGuardrail>>,
}

impl DecisionPipeline {
    /// A pipeline over the given grant set and ordered guardrails.
    pub fn new(capabilities: CapabilitySet, guardrails: Vec<Box<dyn IntentGuardrail>>) -> Self {
        DecisionPipeline { capabilities, guardrails }
    }

    /// Run one policy decision through the stages and return exactly one
    /// [`DecisionEnvelope`] — the no-gaps invariant (R5): every path records
    /// what ran, and stages that did not run are explicit `NotEvaluated`.
    ///
    /// - `NoAction` / `Failed` decisions never reach the stages.
    /// - An `Execute` decision is checked against the capability set first; a
    ///   denial (recording the required capability via the error's `Display`)
    ///   short-circuits guardrails and lowering.
    /// - Guardrails run in order; the first rejection is recorded and
    ///   short-circuits lowering.
    /// - A fully approved intent lowers via [`lower`] (total today → always
    ///   [`LoweringOutcome::Success`]) into the envelope's action.
    pub fn run(
        &self,
        ts_event: u64,
        trigger: DecisionTrigger,
        context: AgentContext,
        decision: PolicyDecision,
    ) -> DecisionEnvelope {
        let (policy_decision, capability, guardrail, lowering, action) = match decision {
            PolicyDecision::NoAction => (
                PolicyDecisionRecord::NoAction,
                CapabilityOutcome::NotEvaluated,
                GuardrailResult::NotEvaluated,
                LoweringOutcome::NotEvaluated,
                None,
            ),
            PolicyDecision::Failed(err) => (
                PolicyDecisionRecord::Failed { reason: err.to_string() },
                CapabilityOutcome::NotEvaluated,
                GuardrailResult::NotEvaluated,
                LoweringOutcome::NotEvaluated,
                None,
            ),
            PolicyDecision::Execute(planned) => {
                let record = PolicyDecisionRecord::Execute {
                    intent_id: planned.intent_id,
                    intent: planned.intent.clone(),
                };
                match self.capabilities.check_intent(&planned.intent) {
                    Err(err) => (
                        record,
                        CapabilityOutcome::Denied { reason: err.to_string() },
                        GuardrailResult::NotEvaluated,
                        LoweringOutcome::NotEvaluated,
                        None,
                    ),
                    Ok(()) => {
                        let rejection = self.guardrails.iter().find_map(|g| {
                            match g.evaluate(&planned.intent, &context) {
                                GuardrailResult::Rejected { reason } => Some(reason),
                                GuardrailResult::Approved => None,
                                // Out-of-contract: a guardrail must never
                                // return NotEvaluated (that representation
                                // belongs to the pipeline). Treating it as
                                // approval would be the fake-Approved R5
                                // forbids — fail closed instead.
                                GuardrailResult::NotEvaluated => Some(format!(
                                    "{}: guardrail returned NotEvaluated (out of contract) — \
                                     failing closed",
                                    g.name()
                                )),
                            }
                        });
                        match rejection {
                            Some(reason) => (
                                record,
                                CapabilityOutcome::Granted,
                                GuardrailResult::Rejected { reason },
                                LoweringOutcome::NotEvaluated,
                                None,
                            ),
                            None => (
                                record,
                                CapabilityOutcome::Granted,
                                GuardrailResult::Approved,
                                LoweringOutcome::Success,
                                Some(lower(&planned.intent)),
                            ),
                        }
                    }
                }
            }
        };
        DecisionEnvelope {
            schema_version: ENVELOPE_SCHEMA_VERSION,
            envelope_id: UUID4::new(),
            ts_event,
            trigger,
            context,
            policy_decision,
            capability,
            guardrail,
            lowering,
            action,
            decision_detail: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;
    use crate::agent::capability::ActionCapability;
    use crate::agent::context::PositionSummary;
    use crate::agent::envelope::{from_jsonl, to_jsonl};
    use crate::agent::guardrails::proposal_bounds::ProposalBoundsGuardrail;
    use crate::agent::policy::PolicyError;

    fn research_capabilities() -> CapabilitySet {
        CapabilitySet {
            observations: BTreeSet::new(),
            actions: BTreeSet::from([ActionCapability::Research]),
            instrument_scope: BTreeSet::new(),
        }
    }

    fn pipeline() -> DecisionPipeline {
        DecisionPipeline::new(
            research_capabilities(),
            vec![Box::new(ProposalBoundsGuardrail { max_relative_change: 0.5 })],
        )
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

    fn trigger() -> DecisionTrigger {
        DecisionTrigger::Manual { reason: "unit test".to_string() }
    }

    fn run_state_context() -> AgentContext {
        AgentContext::run_state(
            1_000_000.0,
            vec![PositionSummary {
                symbol: "005930.XKRX".to_string(),
                side: "LONG".to_string(),
                quantity: 10.0,
            }],
            BTreeMap::from([("gap_min_pct".to_string(), 3.0)]),
            BTreeMap::from([("pnl_krw".to_string(), -500.0)]),
        )
    }

    #[test]
    fn granted_in_bounds_execute_lowers_to_a_research_command() {
        let envelope = pipeline().run(
            42,
            trigger(),
            run_state_context(),
            PolicyDecision::execute(proposal(3.0, 4.0)),
        );
        assert_eq!(envelope.capability, CapabilityOutcome::Granted);
        assert_eq!(envelope.guardrail, GuardrailResult::Approved);
        assert_eq!(envelope.lowering, LoweringOutcome::Success);
        let Some(RuntimeAction::ResearchCommand { description }) = &envelope.action else {
            panic!("expected a ResearchCommand action, got {:?}", envelope.action);
        };
        assert!(description.contains("gap_min_pct"), "{description}");
        assert!(matches!(envelope.policy_decision, PolicyDecisionRecord::Execute { .. }));
    }

    #[test]
    fn capability_denied_execute_records_the_required_capability_and_short_circuits() {
        // AE1: a management intent on a research-only grant set is denied with
        // a reason naming the required capability; guardrails and lowering
        // never run and no action is produced.
        let envelope = pipeline().run(
            42,
            trigger(),
            run_state_context(),
            PolicyDecision::execute(AgentIntent::PauseStrategy {
                strategy_id: "orb-v0".to_string(),
                reason: "drawdown".to_string(),
            }),
        );
        let CapabilityOutcome::Denied { reason } = &envelope.capability else {
            panic!("expected a capability denial, got {:?}", envelope.capability);
        };
        assert!(reason.contains("ManageStrategies"), "names the required capability: {reason}");
        assert_eq!(envelope.guardrail, GuardrailResult::NotEvaluated);
        assert_eq!(envelope.lowering, LoweringOutcome::NotEvaluated);
        assert!(envelope.action.is_none());
    }

    #[test]
    fn guardrail_rejection_records_the_reason_and_short_circuits_lowering() {
        // 3.0 -> 6.0 is a +100% change, outside proposal_bounds' ±50%.
        let envelope = pipeline().run(
            42,
            trigger(),
            run_state_context(),
            PolicyDecision::execute(proposal(3.0, 6.0)),
        );
        assert_eq!(envelope.capability, CapabilityOutcome::Granted);
        let GuardrailResult::Rejected { reason } = &envelope.guardrail else {
            panic!("expected a guardrail rejection, got {:?}", envelope.guardrail);
        };
        assert!(reason.contains("proposal_bounds"), "names the guardrail: {reason}");
        assert_eq!(envelope.lowering, LoweringOutcome::NotEvaluated);
        assert!(envelope.action.is_none());
    }

    #[test]
    fn no_action_decision_records_every_stage_as_not_evaluated() {
        let envelope =
            pipeline().run(42, trigger(), run_state_context(), PolicyDecision::NoAction);
        assert_eq!(envelope.policy_decision, PolicyDecisionRecord::NoAction);
        assert_eq!(envelope.capability, CapabilityOutcome::NotEvaluated);
        assert_eq!(envelope.guardrail, GuardrailResult::NotEvaluated);
        assert_eq!(envelope.lowering, LoweringOutcome::NotEvaluated);
        assert!(envelope.action.is_none());
    }

    #[test]
    fn failed_decision_records_the_policy_error_reason() {
        let envelope = pipeline().run(
            42,
            trigger(),
            run_state_context(),
            PolicyDecision::Failed(PolicyError::Internal { message: "boom".to_string() }),
        );
        let PolicyDecisionRecord::Failed { reason } = &envelope.policy_decision else {
            panic!("expected a Failed record, got {:?}", envelope.policy_decision);
        };
        assert!(reason.contains("boom"), "{reason}");
        assert_eq!(envelope.capability, CapabilityOutcome::NotEvaluated);
        assert_eq!(envelope.guardrail, GuardrailResult::NotEvaluated);
        assert_eq!(envelope.lowering, LoweringOutcome::NotEvaluated);
        assert!(envelope.action.is_none());
    }

    #[test]
    fn every_path_yields_exactly_one_envelope_that_round_trips_through_jsonl() {
        // The no-gaps invariant (R5): one envelope per run() call on every
        // path, each valid against the U1 schema (serde round-trip).
        let p = pipeline();
        let decisions = vec![
            PolicyDecision::execute(proposal(3.0, 4.0)),
            PolicyDecision::execute(AgentIntent::PauseStrategy {
                strategy_id: "orb-v0".to_string(),
                reason: "drawdown".to_string(),
            }),
            PolicyDecision::execute(proposal(3.0, 6.0)),
            PolicyDecision::NoAction,
            PolicyDecision::Failed(PolicyError::Timeout { timeout_ms: 250 }),
        ];
        let envelopes: Vec<DecisionEnvelope> = decisions
            .into_iter()
            .map(|d| p.run(42, trigger(), run_state_context(), d))
            .collect();
        assert_eq!(envelopes.len(), 5, "exactly one envelope per run() call");
        for e in &envelopes {
            assert_eq!(e.schema_version, ENVELOPE_SCHEMA_VERSION);
        }
        let text = to_jsonl(&envelopes).unwrap();
        assert_eq!(text.lines().count(), 5);
        let back = from_jsonl(&text).unwrap();
        assert_eq!(to_jsonl(&back).unwrap(), text, "round-trip preserves every field");
    }

    #[test]
    fn lower_is_total_over_every_intent_variant() {
        // 8 management arms + 1 research arm: each lowers to the expected
        // action kind with its identifying fragment in the description.
        use nautilus_model::identifiers::InstrumentId;
        use nautilus_model::types::Quantity;
        let id = InstrumentId::from("005930.XKRX");
        let cases: Vec<(AgentIntent, bool, &str)> = vec![
            (proposal(3.0, 4.0), true, "gap_min_pct"),
            (
                AgentIntent::ReducePosition {
                    instrument_id: id,
                    target_quantity: Quantity::from(5),
                    reason: "drawdown".to_string(),
                },
                false,
                "reduce",
            ),
            (
                AgentIntent::ClosePosition { instrument_id: id, reason: "eod".to_string() },
                false,
                "close",
            ),
            (
                AgentIntent::CancelOrder {
                    instrument_id: id,
                    client_order_id: "co-1".to_string(),
                    reason: "stale".to_string(),
                },
                false,
                "cancel co-1",
            ),
            (
                AgentIntent::CancelAllOrders { instrument_id: id, reason: "halt".to_string() },
                false,
                "cancel all",
            ),
            (
                AgentIntent::PauseStrategy {
                    strategy_id: "orb-v0".to_string(),
                    reason: "anomaly".to_string(),
                },
                false,
                "pause",
            ),
            (
                AgentIntent::ResumeStrategy {
                    strategy_id: "orb-v0".to_string(),
                    reason: "cleared".to_string(),
                },
                false,
                "resume",
            ),
            (
                AgentIntent::AdjustRiskLimits { description: "halve size".to_string() },
                false,
                "adjust risk",
            ),
            (
                AgentIntent::EscalateToHuman { reason: "ambiguous".to_string() },
                false,
                "escalate",
            ),
        ];
        assert_eq!(cases.len(), 9, "one case per AgentIntent variant");
        for (intent, is_research, fragment) in cases {
            match (lower(&intent), is_research) {
                (RuntimeAction::ResearchCommand { description }, true)
                | (RuntimeAction::ManagementCommand { description }, false) => {
                    assert!(
                        description.contains(fragment),
                        "{fragment:?} not in {description:?}"
                    );
                }
                (action, _) => panic!("wrong action kind for {intent:?}: {action:?}"),
            }
        }
    }

    #[test]
    fn lowered_research_description_uses_scrub_safe_fixed_precision() {
        // 3.0 * 0.8 Displays as 2.4000000000000004 under raw f64 formatting —
        // a 16-digit run the write-time scrub would mask. lower() must emit
        // fixed precision so the recorded audit text survives the scrub.
        let action = lower(&proposal(3.0, 3.0 * 0.8));
        let RuntimeAction::ResearchCommand { description } = action else {
            panic!("research intent must lower to a ResearchCommand");
        };
        assert!(description.contains("2.4000"), "{description}");
        assert!(!description.contains("2.4000000000000004"), "{description}");
    }

    #[test]
    fn out_of_contract_not_evaluated_guardrail_fails_closed() {
        // A guardrail must never return NotEvaluated; if one does, the
        // pipeline records a rejection (never a fake approval, R5).
        struct BrokenGuardrail;
        impl IntentGuardrail for BrokenGuardrail {
            fn name(&self) -> &str {
                "broken"
            }
            fn evaluate(&self, _: &AgentIntent, _: &AgentContext) -> GuardrailResult {
                GuardrailResult::NotEvaluated
            }
        }
        let p = DecisionPipeline::new(research_capabilities(), vec![Box::new(BrokenGuardrail)]);
        let envelope = p.run(
            42,
            trigger(),
            run_state_context(),
            PolicyDecision::execute(proposal(3.0, 4.0)),
        );
        let GuardrailResult::Rejected { reason } = &envelope.guardrail else {
            panic!("expected fail-closed rejection, got {:?}", envelope.guardrail);
        };
        assert!(reason.contains("out of contract"), "{reason}");
        assert_eq!(envelope.lowering, LoweringOutcome::NotEvaluated);
        assert!(envelope.action.is_none());
    }

    #[test]
    fn pipeline_envelopes_over_run_state_context_carry_no_account_like_keys() {
        // R9 through the PIPELINE path (extends U1's
        // run_state_context_carries_no_account_like_keys): the position
        // summary, not a nautilus Position, is what serializes.
        let envelope = pipeline().run(
            42,
            trigger(),
            run_state_context(),
            PolicyDecision::execute(proposal(3.0, 4.0)),
        );
        let line = serde_json::to_string(&envelope).unwrap();
        assert!(!line.contains("account"), "no account-like token: {line}");
    }
}
