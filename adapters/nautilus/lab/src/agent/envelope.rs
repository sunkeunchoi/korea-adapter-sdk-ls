//! Decision envelope — the append-only record of one decision cycle (R1, R5).
//!
//! Every cycle serializes one [`DecisionEnvelope`] carrying the trigger, the
//! context the decision was made against, the policy decision, and each
//! runtime stage's outcome. Stages that did not run carry an **explicit**
//! not-evaluated representation — never a fake `Approved` (R5). Serde tag
//! layout mirrors upstream `nautilus_agents` (KTD2): triggers tag on
//! `"type"`, stage outcomes tag on `"result"`; the `NotEvaluated` stage
//! variants are lab-superset additions under shape-mirroring.
//!
//! Serialized as JSONL, one envelope per line ([`to_jsonl`] / [`from_jsonl`]).

use std::collections::BTreeMap;

use nautilus_core::UUID4;
use nautilus_model::identifiers::InstrumentId;
use serde::{Deserialize, Serialize};

use crate::agent::action::RuntimeAction;
use crate::agent::context::AgentContext;
use crate::agent::intent::AgentIntent;

/// The decision taken on a universe candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Accept,
    Reject,
}

/// The kind of decision an event records (KTD9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalKind {
    /// A universe candidate was accepted or rejected at selection time.
    Universe,
    /// A selected symbol's price broke above the opening-range high.
    Breakout,
    /// An entry order was placed after a breakout.
    OrderPlaced,
    /// An entry was suppressed by the sizing / concurrency gate.
    OrderRejectedSizing,
    /// A held position hit its stop (range low).
    StopHit,
    /// A held position was banked at its fixed profit target.
    Target,
    /// A held position was flattened at the time-flat deadline.
    TimeExit,
    /// End-of-session summary for a selected symbol (extreme values observed).
    SessionSummary,
}

/// The committed envelope schema version, bumped on any wire-shape change.
pub const ENVELOPE_SCHEMA_VERSION: u32 = 1;

/// What started a decision cycle. Tagged on `"type"` (KTD2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum DecisionTrigger {
    /// A periodic timer fired.
    Timer {
        /// The timer interval in nanoseconds.
        interval_ns: u64,
    },
    /// A market-data event arrived.
    MarketData {
        /// The instrument the event concerns.
        instrument_id: InstrumentId,
    },
    /// An internal state change occurred.
    StateChange {
        /// What changed.
        description: String,
    },
    /// A human asked for a decision cycle.
    Manual {
        /// Why the cycle was requested.
        reason: String,
    },
}

/// The capability stage's outcome. Tagged on `"result"` (KTD2). The envelope
/// stores the capability *result*; cycles that never reach the stage record
/// [`CapabilityOutcome::NotEvaluated`] explicitly (R5).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result")]
#[non_exhaustive]
pub enum CapabilityOutcome {
    /// The intent was within the agent's granted capabilities.
    Granted,
    /// The intent exceeded the agent's capabilities.
    Denied {
        /// Which capability check denied it.
        reason: String,
    },
    /// The stage never ran this cycle (e.g. telemetry-only, no intent).
    NotEvaluated,
}

/// The guardrail stage's outcome. Tagged on `"result"` (KTD2).
/// [`GuardrailResult::NotEvaluated`] is a lab-superset addition for cycles
/// where the stage never ran (R5).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result")]
#[non_exhaustive]
pub enum GuardrailResult {
    /// Every guardrail approved the intent.
    Approved,
    /// A guardrail rejected the intent.
    Rejected {
        /// Which guardrail rejected it and why.
        reason: String,
    },
    /// The stage never ran this cycle (R5: explicit, never a fake Approved).
    NotEvaluated,
}

/// The lowering stage's outcome. Tagged on `"result"` (KTD2).
/// [`LoweringOutcome::NotEvaluated`] is a lab-superset addition for cycles
/// where the stage never ran (R5).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result")]
#[non_exhaustive]
pub enum LoweringOutcome {
    /// The intent lowered to a runtime action.
    Success,
    /// Lowering failed.
    Failed {
        /// Why lowering failed.
        reason: String,
    },
    /// The stage never ran this cycle (R5: explicit, never a fake Success).
    NotEvaluated,
}

/// The serializable record of what the policy decided. Tagged on `"decision"`.
/// U4's runtime `PolicyDecision` maps into this record for the envelope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "decision")]
#[non_exhaustive]
pub enum PolicyDecisionRecord {
    /// The policy produced an intent to execute.
    Execute {
        /// The intent's stable id (correlates follow-up envelopes).
        intent_id: UUID4,
        /// The intent itself.
        intent: AgentIntent,
    },
    /// The policy decided to do nothing this cycle.
    NoAction,
    /// The policy itself failed.
    Failed {
        /// Why the policy failed.
        reason: String,
    },
}

/// The in-run strategy telemetry payload — the per-decision strategy log
/// (KTD9, R6), one detail per *decision*, never per bar, riding the envelope
/// stream as `decision_detail`. `kind` uses [`SignalKind`]'s snake_case wire
/// tags, preserving the retired per-decision signal-log event shape
/// variant-for-variant.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DecisionDetail {
    /// What kind of strategy decision this is (snake_case on the wire).
    pub kind: SignalKind,
    /// The instrument the decision concerns (`{shcode}.XKRX`).
    pub symbol: String,
    /// For a universe decision, whether the candidate was accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<Decision>,
    /// The rejecting filter's name, or the reason a transition fired.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// The signal values at decision time (sorted keys → deterministic output).
    pub values: BTreeMap<String, f64>,
}

impl DecisionDetail {
    /// A universe accept/reject decision's detail.
    pub fn universe(
        symbol: impl Into<String>,
        decision: Decision,
        filter: Option<String>,
        values: BTreeMap<String, f64>,
    ) -> Self {
        DecisionDetail {
            kind: SignalKind::Universe,
            symbol: symbol.into(),
            decision: Some(decision),
            filter,
            values,
        }
    }

    /// A state-transition decision's detail on a selected symbol.
    pub fn transition(
        symbol: impl Into<String>,
        kind: SignalKind,
        values: BTreeMap<String, f64>,
    ) -> Self {
        DecisionDetail { kind, symbol: symbol.into(), decision: None, filter: None, values }
    }
}

/// The append-only record of one decision cycle (R1). Every stage field is
/// explicit; stages that did not run carry `NotEvaluated`, never a fake
/// approval (R5).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionEnvelope {
    /// The envelope schema version ([`ENVELOPE_SCHEMA_VERSION`] at write time).
    pub schema_version: u32,
    /// This envelope's unique id.
    pub envelope_id: UUID4,
    /// Decision time as UTC unix nanoseconds.
    pub ts_event: u64,
    /// What started the cycle.
    pub trigger: DecisionTrigger,
    /// The state snapshot the decision was made against (R9: credential-free).
    pub context: AgentContext,
    /// What the policy decided.
    pub policy_decision: PolicyDecisionRecord,
    /// The capability stage's outcome.
    pub capability: CapabilityOutcome,
    /// The guardrail stage's outcome.
    pub guardrail: GuardrailResult,
    /// The lowering stage's outcome.
    pub lowering: LoweringOutcome,
    /// The lowered runtime action, when lowering succeeded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<RuntimeAction>,
    /// In-run strategy telemetry riding this envelope, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_detail: Option<DecisionDetail>,
}

impl DecisionEnvelope {
    /// A pure-telemetry envelope: `NoAction` policy decision, every stage
    /// explicitly `NotEvaluated` (R5), no action — just the strategy telemetry
    /// payload and the context it was observed under.
    pub fn telemetry(
        ts_event: u64,
        trigger: DecisionTrigger,
        detail: DecisionDetail,
        context: AgentContext,
    ) -> Self {
        DecisionEnvelope {
            schema_version: ENVELOPE_SCHEMA_VERSION,
            envelope_id: UUID4::new(),
            ts_event,
            trigger,
            context,
            policy_decision: PolicyDecisionRecord::NoAction,
            capability: CapabilityOutcome::NotEvaluated,
            guardrail: GuardrailResult::NotEvaluated,
            lowering: LoweringOutcome::NotEvaluated,
            action: None,
            decision_detail: Some(detail),
        }
    }
}

/// Render a slice of envelopes as JSONL (one compact JSON object per line,
/// trailing newline). **Round-trip/test helper only — performs no scrubbing.**
/// Disk writers must go through `RunWriter::write_decisions` or
/// `DecisionRecorder::append`, which serialize via [`to_scrubbed_jsonl_line`]
/// so free-text fields are masked before the line hits disk (R9).
pub fn to_jsonl(envelopes: &[DecisionEnvelope]) -> serde_json::Result<String> {
    let mut out = String::new();
    for e in envelopes {
        out.push_str(&serde_json::to_string(e)?);
        out.push('\n');
    }
    Ok(out)
}

/// The envelope's free-text JSON keys, scrubbed at write time (R9). Everything
/// else in an envelope is typed (ids, numbers, tags) and stays intact so the
/// line parses back via [`from_jsonl`].
const FREE_TEXT_KEYS: [&str; 4] = ["reason", "rationale", "description", "message"];

/// Scrub the free-text string values (by [`FREE_TEXT_KEYS`]) anywhere in the
/// serialized envelope tree, delegating to [`crate::artifacts::scrub`].
pub(crate) fn scrub_free_text(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                match val {
                    serde_json::Value::String(s) if FREE_TEXT_KEYS.contains(&key.as_str()) => {
                        *s = crate::artifacts::scrub(s);
                    }
                    _ => scrub_free_text(val),
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                scrub_free_text(item);
            }
        }
        _ => {}
    }
}

/// Serialize one envelope as a compact, scrubbed JSONL line (trailing
/// newline): free-text fields are masked via [`scrub_free_text`] **before**
/// the line is produced, typed fields (UUIDs, numbers, tags) stay intact so
/// the line parses back. The single write seam shared by the in-run artifact
/// writer (`RunWriter::write_decisions`) and the cross-run recorder
/// (`DecisionRecorder::append`) — one scrub discipline for both destinations
/// (R9).
pub(crate) fn to_scrubbed_jsonl_line(envelope: &DecisionEnvelope) -> serde_json::Result<String> {
    let mut value = serde_json::to_value(envelope)?;
    scrub_free_text(&mut value);
    let mut line = serde_json::to_string(&value)?;
    line.push('\n');
    Ok(line)
}

/// Parse a JSONL decision log back into envelopes (round-trip helper for tests
/// + replay).
pub fn from_jsonl(s: &str) -> serde_json::Result<Vec<DecisionEnvelope>> {
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect()
}

#[cfg(test)]
mod tests {
    use nautilus_model::types::Quantity;

    use super::*;
    use crate::agent::context::PositionSummary;

    fn vals(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn detail() -> DecisionDetail {
        DecisionDetail {
            kind: SignalKind::Universe,
            symbol: "005930.XKRX".to_string(),
            decision: Some(Decision::Reject),
            filter: Some("gap".to_string()),
            values: vals(&[("gap_pct", 1.2)]),
        }
    }

    fn telemetry_context() -> AgentContext {
        AgentContext::telemetry(
            "orb-v0",
            1,
            vals(&[("gap_min_pct", 3.0)]),
            BTreeMap::from([("decisions".to_string(), 1u64)]),
        )
    }

    fn full_envelope() -> DecisionEnvelope {
        DecisionEnvelope {
            schema_version: ENVELOPE_SCHEMA_VERSION,
            envelope_id: UUID4::new(),
            ts_event: 42,
            trigger: DecisionTrigger::Timer { interval_ns: 1_000_000_000 },
            context: AgentContext::run_state(
                1_000_000.0,
                vec![PositionSummary {
                    symbol: "005930.XKRX".to_string(),
                    side: "LONG".to_string(),
                    quantity: 10.0,
                }],
                vals(&[("gap_min_pct", 3.0)]),
                vals(&[("pnl_krw", -500.0)]),
            ),
            policy_decision: PolicyDecisionRecord::Execute {
                intent_id: UUID4::new(),
                intent: AgentIntent::ReducePosition {
                    instrument_id: InstrumentId::from("005930.XKRX"),
                    target_quantity: Quantity::from(5),
                    reason: "drawdown".to_string(),
                },
            },
            capability: CapabilityOutcome::Granted,
            guardrail: GuardrailResult::Rejected { reason: "position delta too large".to_string() },
            lowering: LoweringOutcome::NotEvaluated,
            action: Some(RuntimeAction::ManagementCommand {
                description: "reduce 005930 to 5".to_string(),
            }),
            decision_detail: Some(detail()),
        }
    }

    #[test]
    fn envelope_jsonl_round_trip_preserves_every_stage_field() {
        let envelopes = vec![
            full_envelope(),
            DecisionEnvelope::telemetry(
                7,
                DecisionTrigger::MarketData {
                    instrument_id: InstrumentId::from("005930.XKRX"),
                },
                detail(),
                telemetry_context(),
            ),
        ];
        let text = to_jsonl(&envelopes).unwrap();
        assert_eq!(text.lines().count(), 2, "one compact line per envelope");
        for line in text.lines() {
            assert!(!line.contains('\n'));
        }
        let back = from_jsonl(&text).unwrap();
        // DecisionEnvelope does not derive PartialEq (upstream shape); compare
        // by re-serialization, which covers every field including stages.
        assert_eq!(to_jsonl(&back).unwrap(), text);
        assert_eq!(back[0].capability, CapabilityOutcome::Granted);
        assert!(matches!(back[0].guardrail, GuardrailResult::Rejected { .. }));
        assert_eq!(back[0].lowering, LoweringOutcome::NotEvaluated);
        assert!(matches!(back[0].policy_decision, PolicyDecisionRecord::Execute { .. }));
        assert!(back[0].action.is_some());
        assert!(back[0].decision_detail.is_some());
    }

    #[test]
    fn schema_version_is_one_and_serialized() {
        assert_eq!(ENVELOPE_SCHEMA_VERSION, 1);
        let line = serde_json::to_string(&full_envelope()).unwrap();
        assert!(line.contains("\"schema_version\":1"), "version on the wire: {line}");
    }

    #[test]
    fn enum_tags_match_upstream_wire_format() {
        // KTD2: triggers tag on "type", stage outcomes tag on "result".
        let trigger = serde_json::to_string(&DecisionTrigger::StateChange {
            description: "position opened".to_string(),
        })
        .unwrap();
        assert!(trigger.contains("\"type\":\"StateChange\""), "{trigger}");
        let guardrail = serde_json::to_string(&GuardrailResult::Rejected {
            reason: "too big".to_string(),
        })
        .unwrap();
        assert!(guardrail.contains("\"result\":\"Rejected\""), "{guardrail}");
        let lowering = serde_json::to_string(&LoweringOutcome::Success).unwrap();
        assert!(lowering.contains("\"result\":\"Success\""), "{lowering}");
        let capability = serde_json::to_string(&CapabilityOutcome::NotEvaluated).unwrap();
        assert!(capability.contains("\"result\":\"NotEvaluated\""), "{capability}");
    }

    #[test]
    fn telemetry_constructor_records_explicit_not_evaluated_stages() {
        let e = DecisionEnvelope::telemetry(
            9,
            DecisionTrigger::Manual { reason: "unit test".to_string() },
            detail(),
            telemetry_context(),
        );
        assert_eq!(e.schema_version, ENVELOPE_SCHEMA_VERSION);
        assert_eq!(e.policy_decision, PolicyDecisionRecord::NoAction);
        assert_eq!(e.capability, CapabilityOutcome::NotEvaluated);
        assert_eq!(e.guardrail, GuardrailResult::NotEvaluated);
        assert_eq!(e.lowering, LoweringOutcome::NotEvaluated);
        assert!(e.action.is_none());
        assert!(e.decision_detail.is_some());
    }

    #[test]
    fn universe_reject_detail_names_filter_and_values() {
        // Ported from the retired signal log: a rejection carries the rejecting
        // filter's name and the signal values at decision time, snake_case tags.
        let line = serde_json::to_string(&detail()).unwrap();
        assert!(line.contains("\"reject\""), "{line}");
        assert!(line.contains("\"gap\""), "{line}");
        assert!(line.contains("\"gap_pct\":1.2"), "{line}");
    }

    #[test]
    fn accept_detail_omits_filter() {
        // Ported from the retired signal log: an accept has no rejecting filter,
        // and the optional fields stay off the wire.
        let d = DecisionDetail::universe("A.XKRX", Decision::Accept, None, BTreeMap::new());
        let line = serde_json::to_string(&d).unwrap();
        assert!(!line.contains("filter"), "an accept has no rejecting filter: {line}");
        assert!(line.contains("\"accept\""), "{line}");
    }

    #[test]
    fn signal_kind_tags_stay_snake_case_on_the_wire() {
        // The relocated enums preserve the retired log's exact wire shape.
        let kind = serde_json::to_string(&SignalKind::OrderRejectedSizing).unwrap();
        assert_eq!(kind, "\"order_rejected_sizing\"");
        // The v9 target exit renders as "target" on the wire, alongside the existing
        // stop_hit / time_exit exit tags.
        assert_eq!(serde_json::to_string(&SignalKind::Target).unwrap(), "\"target\"");
        let decision = serde_json::to_string(&Decision::Reject).unwrap();
        assert_eq!(decision, "\"reject\"");
    }

    #[test]
    fn run_state_context_carries_no_account_like_keys() {
        // R9: the serialized envelope's position summaries expose only
        // symbol/side/quantity — no account-like tokens.
        let line = serde_json::to_string(&full_envelope()).unwrap();
        assert!(!line.contains("account"), "no account-like key: {line}");
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        let positions = &value["context"]["positions"];
        let keys: Vec<&str> = positions[0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["quantity", "side", "symbol"]);
    }
}
