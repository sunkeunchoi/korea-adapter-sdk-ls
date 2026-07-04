//! Research-tier demonstrator policy (R8, R9 partial) — a deterministic
//! heuristic over a finalized run's summary stats that proposes widening the
//! ORB universe when the run traded too little.
//!
//! The heuristic is deliberately the simplest that produces a visible proposal
//! (plan Open Questions): if the run closed fewer than [`ResearchPolicy::min_trades`]
//! trades, propose lowering the [`PARAM_GAP_MIN_PCT`] universe filter by
//! [`ResearchPolicy::gap_widen_factor`]; otherwise do nothing. The policy is a
//! pure function of the [`AgentContext::RunState`] form — no randomness, no
//! clock, no filesystem reads inside `evaluate` — so the same context always
//! yields the same intent, and a recorded envelope's captured context alone is
//! sufficient to reconstruct the decision (the policy-replay deferral premise).
//!
//! [`ResearchPolicy::context_from_run`] is the bridge from the append-only run
//! registry: it reads a finalized run's `manifest.json` + `performance.json`
//! and builds the credential-free `RunState` context (R9 — positions are empty
//! because a finalized backtest is flat, and the params map carries only
//! numeric [`crate::params::OrbParams`] fields).

use std::path::Path;

use anyhow::Context as _;

use crate::agent::context::AgentContext;
use crate::agent::intent::AgentIntent;
use crate::agent::policy::{AgentPolicy, PolicyDecision, PolicyError};
use crate::artifacts::manifest::Manifest;
use crate::artifacts::performance::PerformanceReport;
use crate::artifacts::{MANIFEST_FILE, PERFORMANCE_FILE};

/// The `run_summary` key the policy reads: the closed-trade count, written by
/// [`PerformanceReport::assemble`] into every performance summary.
pub const KEY_NUM_TRADES: &str = "num_trades";

/// The parameter the policy proposes changing:
/// [`crate::params::OrbParams::gap_min_pct`], the
/// universe gap filter. The string must match the real `OrbParams` field name
/// (its serde key) — [`ResearchPolicy::context_from_run`] builds the context's
/// params map from the manifest's serialized `OrbParams`.
pub const PARAM_GAP_MIN_PCT: &str = "gap_min_pct";

/// The deterministic Research-tier demonstrator policy (R8).
///
/// Reads only the [`AgentContext::RunState`] form: when the run's
/// [`KEY_NUM_TRADES`] is below `min_trades`, it proposes lowering
/// [`PARAM_GAP_MIN_PCT`] to `current * gap_widen_factor` (a wider universe
/// admits more candidates); otherwise it returns
/// [`PolicyDecision::NoAction`]. The proposal's `strategy_id` is pinned to the
/// lab's sole strategy ([`crate::params::STRATEGY_ID`]) so the decision is a
/// function of the captured context alone.
#[derive(Clone, Debug, PartialEq)]
pub struct ResearchPolicy {
    /// The closed-trade floor: runs below it trigger a widen proposal.
    pub min_trades: u64,
    /// The multiplicative factor applied to the current gap filter
    /// (e.g. `0.8` lowers a `3.0` filter to `2.4`).
    pub gap_widen_factor: f64,
}

impl Default for ResearchPolicy {
    fn default() -> Self {
        ResearchPolicy { min_trades: 5, gap_widen_factor: 0.8 }
    }
}

impl ResearchPolicy {
    /// Build the policy's [`AgentContext::RunState`] from a finalized run
    /// under `<data_home>/runs/<run_id>/`:
    ///
    /// - `balance_krw` — the run's ending equity (the equity curve's last
    ///   point), `0.0` when the curve is empty;
    /// - `positions` — empty (a finalized backtest is flat);
    /// - `params` — the numeric (f64-able) fields of the manifest's
    ///   [`crate::params::OrbParams`], keyed by their serde field names (so
    ///   [`PARAM_GAP_MIN_PCT`] is present);
    /// - `run_summary` — the performance report's `summary` map, with
    ///   [`KEY_NUM_TRADES`] backfilled from the closed-trade ledger if a
    ///   hand-built fixture omitted it.
    ///
    /// # Errors
    ///
    /// Errors when `manifest.json` / `performance.json` cannot be read or
    /// parsed.
    pub fn context_from_run(data_home: &Path, run_id: &str) -> anyhow::Result<AgentContext> {
        let run_dir = data_home.join("runs").join(run_id);
        let manifest: Manifest = read_json(&run_dir.join(MANIFEST_FILE))?;
        let performance: PerformanceReport = read_json(&run_dir.join(PERFORMANCE_FILE))?;

        let params = manifest.params.numeric_summary();
        let mut run_summary = performance.summary.clone();
        run_summary.entry(KEY_NUM_TRADES.to_string()).or_insert_with(|| {
            performance.trades.iter().filter(|t| t.ts_closed.is_some()).count() as f64
        });
        let balance_krw = performance.equity_curve.last().map(|p| p.equity).unwrap_or(0.0);

        Ok(AgentContext::run_state(balance_krw, Vec::new(), params, run_summary))
    }
}

impl AgentPolicy for ResearchPolicy {
    fn name(&self) -> &str {
        "research"
    }

    /// Decide deterministically from the `RunState` form. The `Telemetry`
    /// form — or a `RunState` lacking [`KEY_NUM_TRADES`] /
    /// [`PARAM_GAP_MIN_PCT`] — is a typed
    /// [`PolicyError::InsufficientContext`].
    fn evaluate(&self, context: &AgentContext) -> Result<PolicyDecision, PolicyError> {
        let AgentContext::RunState { params, run_summary, .. } = context else {
            return Err(PolicyError::InsufficientContext {
                message: "research policy reads the RunState context form only".to_string(),
            });
        };
        let num_trades = *run_summary.get(KEY_NUM_TRADES).ok_or_else(|| {
            PolicyError::InsufficientContext {
                message: format!("run_summary lacks '{KEY_NUM_TRADES}'"),
            }
        })?;
        let current_value = *params.get(PARAM_GAP_MIN_PCT).ok_or_else(|| {
            PolicyError::InsufficientContext {
                message: format!("params lack '{PARAM_GAP_MIN_PCT}'"),
            }
        })?;
        if num_trades < self.min_trades as f64 {
            let proposed_value = current_value * self.gap_widen_factor;
            Ok(PolicyDecision::execute(AgentIntent::ProposeParameterChange {
                strategy_id: crate::params::STRATEGY_ID.to_string(),
                parameter: PARAM_GAP_MIN_PCT.to_string(),
                current_value,
                proposed_value,
                // Fixed-precision formatting keeps the free text scrub-stable:
                // a raw f64 like 2.4000000000000004 carries a 6+ digit run the
                // write-time scrub (R9) would mask as account-like.
                rationale: format!(
                    "run closed {num_trades} trades, below the {} floor — widen the \
                     universe by lowering {PARAM_GAP_MIN_PCT} from {current_value:.4} to \
                     {proposed_value:.4}",
                    self.min_trades,
                ),
            }))
        } else {
            Ok(PolicyDecision::NoAction)
        }
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use tempfile::TempDir;

    use super::*;
    use crate::agent::capability::{ActionCapability, CapabilitySet};
    use crate::agent::envelope::{
        CapabilityOutcome, DecisionTrigger, GuardrailResult, LoweringOutcome,
    };
    use crate::agent::guardrails::proposal_bounds::ProposalBoundsGuardrail;
    use crate::agent::pipeline::DecisionPipeline;
    use crate::artifacts::manifest::DataRange;
    use crate::artifacts::performance::TradeRecord;
    use crate::artifacts::{RunSource, RunWriter};
    use crate::params::OrbParams;

    fn fixture_context(num_trades: f64) -> AgentContext {
        AgentContext::run_state(
            1_000_000.0,
            Vec::new(),
            BTreeMap::from([(PARAM_GAP_MIN_PCT.to_string(), 3.0)]),
            BTreeMap::from([
                (KEY_NUM_TRADES.to_string(), num_trades),
                ("pnl_total".to_string(), 0.0),
            ]),
        )
    }

    fn research_pipeline(capabilities: CapabilitySet) -> DecisionPipeline {
        DecisionPipeline::new(
            capabilities,
            vec![Box::new(ProposalBoundsGuardrail { max_relative_change: 0.5 })],
        )
    }

    fn research_grant() -> CapabilitySet {
        CapabilitySet {
            observations: BTreeSet::new(),
            actions: BTreeSet::from([ActionCapability::Research]),
            instrument_scope: BTreeSet::new(),
        }
    }

    fn trigger() -> DecisionTrigger {
        DecisionTrigger::Manual { reason: "unit test".to_string() }
    }

    fn intent_of(decision: &PolicyDecision) -> &AgentIntent {
        let PolicyDecision::Execute(planned) = decision else {
            panic!("expected Execute, got {decision:?}");
        };
        &planned.intent
    }

    #[test]
    fn same_context_yields_the_same_proposal_twice() {
        // Deterministic (R8): same input → same intent. intent_id differs per
        // plan, so compare the AgentIntent, not the PlannedIntent.
        let policy = ResearchPolicy::default();
        let context = fixture_context(2.0);
        let first = policy.evaluate(&context).unwrap();
        let second = policy.evaluate(&context).unwrap();
        let intent = intent_of(&first);
        assert_eq!(intent, intent_of(&second));
        let AgentIntent::ProposeParameterChange {
            strategy_id, parameter, current_value, proposed_value, rationale,
        } = intent
        else {
            panic!("expected a ProposeParameterChange, got {intent:?}");
        };
        assert_eq!(strategy_id, crate::params::STRATEGY_ID);
        assert_eq!(parameter, PARAM_GAP_MIN_PCT);
        assert_eq!(*current_value, 3.0);
        assert_eq!(*proposed_value, 3.0 * 0.8);
        assert!(rationale.contains("2 trades"), "evidence-backed rationale: {rationale}");
    }

    #[test]
    fn proposal_through_pipeline_with_research_grant_is_granted_and_lowered() {
        let policy = ResearchPolicy::default();
        let context = fixture_context(2.0);
        let decision = policy.evaluate(&context).unwrap();
        let envelope =
            research_pipeline(research_grant()).run(42, trigger(), context, decision);
        assert_eq!(envelope.capability, CapabilityOutcome::Granted);
        assert_eq!(envelope.guardrail, GuardrailResult::Approved);
        assert_eq!(envelope.lowering, LoweringOutcome::Success);
        assert!(envelope.action.is_some(), "action recorded");
    }

    #[test]
    fn proposal_without_research_capability_is_denied() {
        // Deny-by-default (R3): the same proposal under a grant set lacking
        // Research records a capability denial and no action.
        let policy = ResearchPolicy::default();
        let context = fixture_context(2.0);
        let decision = policy.evaluate(&context).unwrap();
        let envelope = research_pipeline(CapabilitySet::default())
            .run(42, trigger(), context, decision);
        let CapabilityOutcome::Denied { reason } = &envelope.capability else {
            panic!("expected a capability denial, got {:?}", envelope.capability);
        };
        assert!(reason.contains("Research"), "names the required capability: {reason}");
        assert_eq!(envelope.guardrail, GuardrailResult::NotEvaluated);
        assert!(envelope.action.is_none());
    }

    #[test]
    fn telemetry_context_is_insufficient() {
        let policy = ResearchPolicy::default();
        let context = AgentContext::telemetry(
            "orb",
            0,
            BTreeMap::from([(PARAM_GAP_MIN_PCT.to_string(), 3.0)]),
            BTreeMap::new(),
        );
        let err = policy.evaluate(&context).unwrap_err();
        assert!(
            matches!(err, PolicyError::InsufficientContext { .. }),
            "telemetry form is insufficient: {err:?}"
        );
    }

    #[test]
    fn run_state_missing_needed_keys_is_insufficient() {
        let policy = ResearchPolicy::default();
        // No num_trades in the summary.
        let no_trades = AgentContext::run_state(
            0.0,
            Vec::new(),
            BTreeMap::from([(PARAM_GAP_MIN_PCT.to_string(), 3.0)]),
            BTreeMap::new(),
        );
        assert!(matches!(
            policy.evaluate(&no_trades).unwrap_err(),
            PolicyError::InsufficientContext { .. }
        ));
        // No gap filter in the params.
        let no_gap = AgentContext::run_state(
            0.0,
            Vec::new(),
            BTreeMap::new(),
            BTreeMap::from([(KEY_NUM_TRADES.to_string(), 2.0)]),
        );
        assert!(matches!(
            policy.evaluate(&no_gap).unwrap_err(),
            PolicyError::InsufficientContext { .. }
        ));
    }

    #[test]
    fn enough_trades_is_no_action() {
        let policy = ResearchPolicy::default(); // min_trades 5
        let decision = policy.evaluate(&fixture_context(5.0)).unwrap();
        assert_eq!(decision, PolicyDecision::NoAction);
    }

    #[test]
    fn gap_parameter_const_matches_the_real_orb_params_field() {
        // PARAM_GAP_MIN_PCT must be OrbParams's actual serde key, or
        // context_from_run's params map and the heuristic silently diverge.
        let value = serde_json::to_value(OrbParams::default()).unwrap();
        assert!(
            value.get(PARAM_GAP_MIN_PCT).is_some_and(serde_json::Value::is_number),
            "OrbParams carries a numeric '{PARAM_GAP_MIN_PCT}' field: {value}"
        );
    }

    fn closed_trade(pnl: f64, ts_open: u64, ts_close: u64) -> TradeRecord {
        TradeRecord {
            symbol: "005930.XKRX".to_string(),
            entry_side: "BUY".to_string(),
            quantity: 10.0,
            avg_px_open: 60_000.0,
            avg_px_close: Some(60_000.0 + pnl / 10.0),
            realized_pnl: pnl,
            ts_opened: ts_open,
            ts_closed: Some(ts_close),
            fills: vec![],
        }
    }

    /// Write a synthetic finalized run (manifest.json + performance.json)
    /// under `<data_home>/runs/<run_id>/` and return its performance report.
    fn write_fixture_run(data_home: &Path, run_id: &str) -> PerformanceReport {
        let manifest = Manifest {
            run_id: run_id.to_string(),
            source: RunSource::Backtest,
            strategy_id: crate::params::STRATEGY_ID.to_string(),
            strategy_version: 0,
            params: OrbParams::default(),
            data_range: DataRange { start: "20260101".to_string(), end: "20260131".to_string() },
            catalog_fingerprint: "fixture-fingerprint".to_string(),
            universe_hash: "fixture-universe".to_string(),
            strategy_code_hash: "fixture-code".to_string(),
            checkpoint_hash: None,
            created_utc: "2026-07-03T00:00:00Z".to_string(),
        };
        let performance = PerformanceReport::assemble(
            vec![closed_trade(100.0, 1, 2), closed_trade(-50.0, 3, 4)],
            1_000_000.0,
        );
        let writer = RunWriter::new(data_home, run_id).unwrap();
        writer.write_manifest(&manifest).unwrap();
        writer.write_performance(&performance).unwrap();
        writer.finalize().unwrap();
        performance
    }

    #[test]
    fn context_from_run_builds_a_run_state_carrying_the_gap_field() {
        let tmp = TempDir::new().unwrap();
        let run_id = "20260703T000000Z-backtest-orb-v0";
        let performance = write_fixture_run(tmp.path(), run_id);

        let context = ResearchPolicy::context_from_run(tmp.path(), run_id).unwrap();
        let AgentContext::RunState { balance_krw, positions, params, run_summary } = &context
        else {
            panic!("expected the RunState form, got {context:?}");
        };
        assert_eq!(params.get(PARAM_GAP_MIN_PCT), Some(&3.0), "gap field present");
        assert_eq!(run_summary.get(KEY_NUM_TRADES), Some(&2.0));
        assert!(positions.is_empty(), "a finalized backtest is flat");
        let ending_equity = performance.equity_curve.last().unwrap().equity;
        assert_eq!(*balance_krw, ending_equity);

        // The built context is directly decidable: 2 trades < the 5 floor.
        let decision = ResearchPolicy::default().evaluate(&context).unwrap();
        assert!(
            matches!(decision, PolicyDecision::Execute(_)),
            "fixture run yields a visible proposal: {decision:?}"
        );
    }

    #[test]
    fn context_from_run_on_a_missing_run_dir_is_an_error() {
        // The documented error path a typo'd run_id (or an aborted run) hits:
        // an Err naming the unreadable artifact, never a panic.
        let tmp = TempDir::new().unwrap();
        let err = ResearchPolicy::context_from_run(tmp.path(), "no-such-run").unwrap_err();
        assert!(err.to_string().contains("manifest.json"), "names the missing artifact: {err}");
    }

    #[test]
    fn context_from_run_on_a_malformed_manifest_is_an_error() {
        let tmp = TempDir::new().unwrap();
        let run_id = "20260703T000000Z-backtest-orb-v0";
        let run_dir = tmp.path().join("runs").join(run_id);
        std::fs::create_dir_all(&run_dir).unwrap();
        std::fs::write(run_dir.join(MANIFEST_FILE), "{not json").unwrap();
        let err = ResearchPolicy::context_from_run(tmp.path(), run_id).unwrap_err();
        assert!(err.to_string().contains("parsing"), "surfaces the parse failure: {err}");
    }
}
