//! Replay — re-evaluate a recorded envelope stream under a caller-supplied
//! guardrail, engine-free (R7).
//!
//! Guardrail-swap replay **only**: the deciding policy is never re-run —
//! policy-level replay is deferred, and the context captured in every envelope
//! (R5) is what unlocks it later. Replay reuses each envelope's **recorded
//! capability outcome verbatim** (capability re-evaluation is out of scope);
//! only the guardrail stage re-evaluates, so the delta is defined as the
//! guardrail-stage delta.
//!
//! **Counterfactual caveat:** on causally-chained streams (each approved
//! proposal shaped the next run), the per-envelope delta is an audit
//! trustworthy up to the first divergence — later envelopes were produced in a
//! world the stricter guardrail would have changed.
//! [`ReplayResult::first_divergence`] marks that boundary.
//!
//! This module never touches `ParquetDataCatalog`, `BacktestEngine`, bars, or
//! the catalog — replay works from envelopes alone (the whole point of R7).

use std::mem::discriminant;
use std::path::Path;

use crate::agent::envelope::{
    CapabilityOutcome, DecisionEnvelope, GuardrailResult, LoweringOutcome, PolicyDecisionRecord,
    ENVELOPE_SCHEMA_VERSION,
};
use crate::agent::guardrail::IntentGuardrail;
use crate::agent::pipeline::lower;

/// Why a recorded envelope stream could not be loaded (R7: typed, per-line).
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    /// The stream file could not be read.
    #[error("I/O error: {message}")]
    Io {
        /// The underlying I/O failure.
        message: String,
    },
    /// A line was not a valid envelope.
    #[error("malformed JSON on line {line}: {message}")]
    MalformedLine {
        /// The 1-indexed offending line.
        line: usize,
        /// The parse failure.
        message: String,
    },
    /// A line carried an envelope schema this reader does not understand.
    #[error("unsupported schema version {version} on line {line} (expected {expected})")]
    UnsupportedSchema {
        /// The 1-indexed offending line.
        line: usize,
        /// The version found on the line.
        version: u32,
        /// The version this reader expects.
        expected: u32,
    },
}

/// Load a recorded envelope stream (JSONL, blank lines tolerated) with typed
/// per-line errors and a per-line schema-version check.
pub fn read_envelopes(path: &Path) -> Result<Vec<DecisionEnvelope>, ReplayError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| ReplayError::Io { message: e.to_string() })?;
    let mut envelopes = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let line_num = idx + 1;
        let envelope: DecisionEnvelope = serde_json::from_str(line).map_err(|e| {
            ReplayError::MalformedLine { line: line_num, message: e.to_string() }
        })?;
        if envelope.schema_version != ENVELOPE_SCHEMA_VERSION {
            return Err(ReplayError::UnsupportedSchema {
                line: line_num,
                version: envelope.schema_version,
                expected: ENVELOPE_SCHEMA_VERSION,
            });
        }
        envelopes.push(envelope);
    }
    Ok(envelopes)
}

/// One replayed cycle: the recorded envelope and its re-evaluation under the
/// new guardrail. The replayed envelope keeps the original `envelope_id` — the
/// id identifies the *cycle*, not the evaluation.
#[derive(Debug)]
pub struct ReplayedDecision {
    /// The envelope as recorded.
    pub original: DecisionEnvelope,
    /// The same cycle re-evaluated under the new guardrail.
    pub replayed: DecisionEnvelope,
    /// Whether the guardrail stage's outcome *variant* changed (Approved vs
    /// Rejected vs NotEvaluated). Reason-only differences within the same
    /// variant do not count as divergence.
    pub diverged: bool,
    /// Whether this cycle was actually re-evaluated (an `Execute` decision
    /// with a `Granted` capability outcome) rather than reproduced verbatim.
    pub evaluated: bool,
}

/// The outcome of replaying a stream under a new guardrail.
#[derive(Debug)]
pub struct ReplayResult {
    /// Every cycle, in stream order.
    pub decisions: Vec<ReplayedDecision>,
    /// How many cycles diverged.
    pub delta_count: usize,
    /// How many cycles were actually re-evaluated under the new guardrail.
    /// `delta_count` is meaningful only against this: an all-telemetry stream
    /// (e.g. a run dir's `decisions.jsonl`) replays with `delta_count == 0`
    /// AND `evaluated_count == 0` — no guardrail agreement was tested — which
    /// must not be read as "the new guardrail changes nothing".
    pub evaluated_count: usize,
    /// The index (into [`ReplayResult::decisions`]) of the first divergence —
    /// the boundary beyond which the counterfactual caveat applies.
    pub first_divergence: Option<usize>,
}

/// Re-evaluate a recorded stream under `guardrail` (guardrail-swap replay,
/// R7). Only cycles whose original pipeline actually reached the guardrail
/// stage — an `Execute` decision with a `Granted` capability outcome — are
/// re-evaluated, against the envelope's captured `context`. Everything else
/// (`NoAction`, `Failed`, capability-denied, telemetry) is reproduced verbatim
/// and contributes no delta.
pub fn replay(envelopes: &[DecisionEnvelope], guardrail: &dyn IntentGuardrail) -> ReplayResult {
    let mut decisions = Vec::with_capacity(envelopes.len());
    let mut delta_count = 0;
    let mut evaluated_count = 0;
    let mut first_divergence = None;
    for (idx, original) in envelopes.iter().enumerate() {
        let replayed_cycle = replay_one(original, guardrail);
        if replayed_cycle.evaluated {
            evaluated_count += 1;
        }
        if replayed_cycle.diverged {
            delta_count += 1;
            if first_divergence.is_none() {
                first_divergence = Some(idx);
            }
        }
        decisions.push(replayed_cycle);
    }
    ReplayResult { decisions, delta_count, evaluated_count, first_divergence }
}

/// Re-evaluate one cycle. See [`replay`] for the eligibility rule.
fn replay_one(original: &DecisionEnvelope, guardrail: &dyn IntentGuardrail) -> ReplayedDecision {
    let eligible_intent = match (&original.policy_decision, &original.capability) {
        (PolicyDecisionRecord::Execute { intent, .. }, CapabilityOutcome::Granted) => {
            Some(intent.clone())
        }
        _ => None,
    };
    let Some(intent) = eligible_intent else {
        return ReplayedDecision {
            original: original.clone(),
            replayed: original.clone(),
            diverged: false,
            evaluated: false,
        };
    };

    // Mirror the pipeline's fail-closed handling: a guardrail must never
    // return NotEvaluated (that representation belongs to the pipeline for
    // stages that did not run), so an out-of-contract return replays as a
    // rejection — never as "the stage did not run" (R5).
    let new_guardrail = match guardrail.evaluate(&intent, &original.context) {
        GuardrailResult::NotEvaluated => GuardrailResult::Rejected {
            reason: format!(
                "{}: guardrail returned NotEvaluated (out of contract) — failing closed",
                guardrail.name()
            ),
        },
        verdict => verdict,
    };
    let mut replayed = original.clone();
    match &new_guardrail {
        GuardrailResult::Approved => {
            replayed.lowering = LoweringOutcome::Success;
            replayed.action = Some(lower(&intent));
        }
        _ => {
            replayed.lowering = LoweringOutcome::NotEvaluated;
            replayed.action = None;
        }
    }
    let diverged = discriminant(&new_guardrail) != discriminant(&original.guardrail);
    replayed.guardrail = new_guardrail;
    ReplayedDecision { original: original.clone(), replayed, diverged, evaluated: true }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::io::Write;

    use super::*;
    use crate::agent::capability::{ActionCapability, CapabilitySet};
    use crate::agent::context::{AgentContext, PositionSummary};
    use crate::agent::envelope::{to_jsonl, Decision, DecisionDetail, DecisionTrigger};
    use crate::agent::guardrails::proposal_bounds::ProposalBoundsGuardrail;
    use crate::agent::intent::AgentIntent;
    use crate::agent::pipeline::DecisionPipeline;
    use crate::agent::policy::PolicyDecision;

    fn research_capabilities() -> CapabilitySet {
        CapabilitySet {
            observations: BTreeSet::new(),
            actions: BTreeSet::from([ActionCapability::Research]),
            instrument_scope: BTreeSet::new(),
        }
    }

    fn context(balance_krw: f64) -> AgentContext {
        AgentContext::run_state(
            balance_krw,
            vec![PositionSummary {
                symbol: "005930.XKRX".to_string(),
                side: "FLAT".to_string(),
                quantity: 0.0,
            }],
            BTreeMap::from([("gap_min_pct".to_string(), 3.0)]),
            BTreeMap::from([("num_trades".to_string(), 2.0)]),
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

    /// A recorded stream produced under a ±50% bound: three proposals with
    /// relative changes of +10%, +40%, and +45% — all approved as recorded.
    fn approved_stream() -> Vec<DecisionEnvelope> {
        let pipeline = DecisionPipeline::new(
            research_capabilities(),
            vec![Box::new(ProposalBoundsGuardrail { max_relative_change: 0.5 })],
        );
        [(3.0, 3.3), (3.0, 4.2), (3.0, 4.35)]
            .into_iter()
            .enumerate()
            .map(|(i, (current, proposed))| {
                pipeline.run(
                    i as u64,
                    trigger(),
                    context(1_000_000.0),
                    PolicyDecision::execute(proposal(current, proposed)),
                )
            })
            .collect()
    }

    /// A guardrail that reads the captured context (not just the intent):
    /// rejects any proposal decided under a balance below the floor.
    struct BalanceFloorGuardrail {
        floor_krw: f64,
    }

    impl IntentGuardrail for BalanceFloorGuardrail {
        fn name(&self) -> &str {
            "balance_floor"
        }

        fn evaluate(&self, intent: &AgentIntent, context: &AgentContext) -> GuardrailResult {
            if !matches!(intent, AgentIntent::ProposeParameterChange { .. }) {
                return GuardrailResult::Approved;
            }
            match context {
                AgentContext::RunState { balance_krw, .. } if *balance_krw < self.floor_krw => {
                    GuardrailResult::Rejected {
                        reason: format!(
                            "balance_floor: balance {balance_krw} below floor {}",
                            self.floor_krw
                        ),
                    }
                }
                _ => GuardrailResult::Approved,
            }
        }
    }

    #[test]
    fn stricter_guardrail_reports_a_non_empty_delta_without_the_engine() {
        // AE2: recorded approvals at +10%/+40%/+45%, replayed under a stricter
        // ±25% bound → the last two flip to Rejected. No engine, no catalog —
        // this module has no such imports.
        let stream = approved_stream();
        let stricter = ProposalBoundsGuardrail { max_relative_change: 0.25 };
        let result = replay(&stream, &stricter);
        assert_eq!(result.delta_count, 2);
        assert_eq!(result.evaluated_count, 3, "every recorded cycle was re-evaluated");
        assert!(!result.decisions[0].diverged);
        assert!(result.decisions[1].diverged);
        assert!(result.decisions[2].diverged);
        let replayed = &result.decisions[1].replayed;
        assert!(matches!(replayed.guardrail, GuardrailResult::Rejected { .. }));
        assert_eq!(replayed.lowering, LoweringOutcome::NotEvaluated);
        assert!(replayed.action.is_none());
        assert_eq!(
            replayed.envelope_id, result.decisions[1].original.envelope_id,
            "the id identifies the cycle, not the evaluation"
        );
    }

    #[test]
    fn identical_guardrail_yields_a_zero_delta() {
        let stream = approved_stream();
        let same = ProposalBoundsGuardrail { max_relative_change: 0.5 };
        let result = replay(&stream, &same);
        assert_eq!(result.delta_count, 0);
        assert_eq!(result.first_divergence, None);
        assert!(result.decisions.iter().all(|d| !d.diverged));
    }

    #[test]
    fn first_divergence_indexes_the_first_rejected_envelope() {
        // ±35% bound: +10% and (exactly-bound) approvals differ — 3.0→4.2 is
        // +40%, the first out-of-bounds envelope, at index 1.
        let stream = approved_stream();
        let stricter = ProposalBoundsGuardrail { max_relative_change: 0.35 };
        let result = replay(&stream, &stricter);
        assert_eq!(result.first_divergence, Some(1));
    }

    #[test]
    fn context_reading_guardrail_evaluates_the_captured_context_on_replay() {
        // The context-capture payoff (R5→R7): a guardrail keyed on the
        // captured balance approves cycles decided at 1_000_000 KRW and
        // rejects the same stream when the floor is above it.
        let stream = approved_stream();
        let below_floor = BalanceFloorGuardrail { floor_krw: 500_000.0 };
        assert_eq!(replay(&stream, &below_floor).delta_count, 0);
        let above_floor = BalanceFloorGuardrail { floor_krw: 2_000_000.0 };
        let result = replay(&stream, &above_floor);
        assert_eq!(result.delta_count, 3, "every cycle's captured balance is below the floor");
        assert!(matches!(
            result.decisions[0].replayed.guardrail,
            GuardrailResult::Rejected { .. }
        ));
    }

    #[test]
    fn capability_denied_cycle_reproduces_verbatim_and_contributes_no_delta() {
        // A management intent on a research-only grant: capability-denied as
        // recorded → replay reuses the recorded outcome, the guardrail never
        // runs, no delta.
        let pipeline = DecisionPipeline::new(
            research_capabilities(),
            vec![Box::new(ProposalBoundsGuardrail { max_relative_change: 0.5 })],
        );
        let denied = pipeline.run(
            0,
            trigger(),
            context(1_000_000.0),
            PolicyDecision::execute(AgentIntent::PauseStrategy {
                strategy_id: "orb-v0".to_string(),
                reason: "drawdown".to_string(),
            }),
        );
        assert!(matches!(denied.capability, CapabilityOutcome::Denied { .. }));
        // A guardrail that would reject anything it sees — it must never run.
        let reject_all = ProposalBoundsGuardrail { max_relative_change: 0.0 };
        let result = replay(&[denied], &reject_all);
        assert_eq!(result.delta_count, 0);
        assert_eq!(result.evaluated_count, 0, "a denied cycle is reproduced, not re-evaluated");
        assert_eq!(result.first_divergence, None);
        assert!(matches!(
            result.decisions[0].replayed.capability,
            CapabilityOutcome::Denied { .. }
        ));
        assert_eq!(result.decisions[0].replayed.guardrail, GuardrailResult::NotEvaluated);
    }

    #[test]
    fn reason_only_differences_within_the_same_variant_are_not_divergence() {
        // Record under a tight bound so every proposal is Rejected, then
        // replay under a different tight bound that also rejects (different
        // reason text). Same variant, different reason -> zero delta.
        let pipeline = DecisionPipeline::new(
            research_capabilities(),
            vec![Box::new(ProposalBoundsGuardrail { max_relative_change: 0.05 })],
        );
        let stream: Vec<DecisionEnvelope> = [(3.0, 3.3), (3.0, 4.2)]
            .into_iter()
            .map(|(current, proposed)| {
                pipeline.run(
                    0,
                    trigger(),
                    context(1_000_000.0),
                    PolicyDecision::execute(proposal(current, proposed)),
                )
            })
            .collect();
        assert!(stream
            .iter()
            .all(|e| matches!(e.guardrail, GuardrailResult::Rejected { .. })));
        let also_rejecting = ProposalBoundsGuardrail { max_relative_change: 0.01 };
        let result = replay(&stream, &also_rejecting);
        assert_eq!(result.delta_count, 0, "reason-only change is not divergence");
        assert_eq!(result.first_divergence, None);
    }

    #[test]
    fn read_envelopes_on_a_missing_path_is_a_typed_io_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = read_envelopes(&dir.path().join("absent.jsonl")).unwrap_err();
        assert!(matches!(err, ReplayError::Io { .. }), "got {err:?}");
    }

    #[test]
    fn read_envelopes_round_trips_a_jsonl_file_with_blank_lines() {
        let stream = approved_stream();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("decisions.jsonl");
        let mut text = to_jsonl(&stream).unwrap();
        text.push('\n'); // trailing blank line tolerated
        std::fs::write(&path, &text).unwrap();
        let back = read_envelopes(&path).unwrap();
        assert_eq!(back.len(), stream.len());
        assert_eq!(to_jsonl(&back).unwrap().trim_end(), text.trim_end());
    }

    #[test]
    fn unsupported_schema_names_the_line_and_both_versions() {
        let stream = approved_stream();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("decisions.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        write!(file, "{}", to_jsonl(&stream[..1]).unwrap()).unwrap();
        let mut bumped = serde_json::to_value(&stream[1]).unwrap();
        bumped["schema_version"] = serde_json::json!(999);
        writeln!(file, "{bumped}").unwrap();
        drop(file);
        let err = read_envelopes(&path).unwrap_err();
        let ReplayError::UnsupportedSchema { line, version, expected } = err else {
            panic!("expected UnsupportedSchema, got {err:?}");
        };
        assert_eq!((line, version, expected), (2, 999, ENVELOPE_SCHEMA_VERSION));
    }

    #[test]
    fn malformed_line_names_the_line() {
        let stream = approved_stream();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("decisions.jsonl");
        let mut text = to_jsonl(&stream[..2]).unwrap();
        text.push_str("{not json\n");
        std::fs::write(&path, &text).unwrap();
        let err = read_envelopes(&path).unwrap_err();
        let ReplayError::MalformedLine { line, .. } = err else {
            panic!("expected MalformedLine, got {err:?}");
        };
        assert_eq!(line, 3);
    }

    #[test]
    fn out_of_contract_not_evaluated_guardrail_fails_closed_on_replay() {
        // Mirrors the pipeline's out-of-contract handling: a guardrail
        // returning NotEvaluated replays as a fail-closed rejection, never as
        // "the stage did not run" (R5).
        struct BrokenGuardrail;
        impl IntentGuardrail for BrokenGuardrail {
            fn name(&self) -> &str {
                "broken"
            }
            fn evaluate(&self, _: &AgentIntent, _: &AgentContext) -> GuardrailResult {
                GuardrailResult::NotEvaluated
            }
        }
        let stream = approved_stream();
        let result = replay(&stream, &BrokenGuardrail);
        assert_eq!(result.delta_count, 3, "fail-closed rejection diverges from every approval");
        let replayed = &result.decisions[0].replayed;
        let GuardrailResult::Rejected { reason } = &replayed.guardrail else {
            panic!("expected fail-closed rejection, got {:?}", replayed.guardrail);
        };
        assert!(reason.contains("out of contract"), "{reason}");
        assert_eq!(replayed.lowering, LoweringOutcome::NotEvaluated);
        assert!(replayed.action.is_none());
    }

    #[test]
    fn telemetry_only_stream_reports_zero_evaluated_not_agreement() {
        // Replaying a run dir's all-telemetry decisions.jsonl yields delta 0
        // — evaluated_count 0 is what distinguishes "nothing was tested" from
        // "the stricter guardrail agrees with every recorded approval".
        let telemetry = DecisionEnvelope::telemetry(
            7,
            DecisionTrigger::StateChange { description: "universe selection scan".to_string() },
            DecisionDetail::universe("005930.XKRX", Decision::Accept, None, BTreeMap::new()),
            context(1_000_000.0),
        );
        let reject_all = ProposalBoundsGuardrail { max_relative_change: 0.0 };
        let result = replay(&[telemetry], &reject_all);
        assert_eq!(result.delta_count, 0);
        assert_eq!(
            result.evaluated_count, 0,
            "zero delta over zero evaluated cycles is not guardrail agreement"
        );
        assert!(!result.decisions[0].evaluated);
    }
}
