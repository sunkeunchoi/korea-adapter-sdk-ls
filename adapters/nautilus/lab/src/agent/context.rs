//! Agent context — the state snapshot a decision was made against (R9).
//!
//! Two forms, one enum: a minimal [`AgentContext::Telemetry`] form constructible
//! inside the engine thread (in-run strategy telemetry), and a fuller
//! [`AgentContext::RunState`] form for post-run / management decisions. Position
//! state is carried as the purpose-built [`PositionSummary`] — **never** a
//! serialized nautilus `Position`, which embeds `account_id` (R9: no
//! account-like tokens on the wire).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A credential-free summary of one open position (R9). Carries only what a
/// decision needs — symbol, side, quantity — and nothing account-identifying.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PositionSummary {
    /// The instrument symbol (`{shcode}.XKRX`).
    pub symbol: String,
    /// The position side (`"LONG"` / `"SHORT"` / `"FLAT"`).
    pub side: String,
    /// The signed-magnitude position quantity.
    pub quantity: f64,
}

/// The state snapshot a decision was made against, in one of two forms
/// (serde-tagged on `"form"`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "form")]
#[non_exhaustive]
pub enum AgentContext {
    /// The minimal in-run form, constructible inside the engine thread: which
    /// strategy decided, under which parameters, with running counts.
    Telemetry {
        /// The deciding strategy's id.
        strategy_id: String,
        /// The strategy's parameter-set version.
        strategy_version: u32,
        /// A hash-or-summary of the numeric parameters in force (sorted keys →
        /// deterministic output).
        params_hash_or_summary: BTreeMap<String, f64>,
        /// Running decision/event counts at decision time.
        counts: BTreeMap<String, u64>,
    },
    /// The fuller run-state form for post-run / management decisions.
    RunState {
        /// The account balance in KRW at decision time.
        balance_krw: f64,
        /// Open positions as credential-free summaries (R9).
        positions: Vec<PositionSummary>,
        /// The numeric parameters in force.
        params: BTreeMap<String, f64>,
        /// Run-level summary metrics (pnl, fill counts, ...).
        run_summary: BTreeMap<String, f64>,
    },
}

impl AgentContext {
    /// The minimal in-run telemetry context.
    pub fn telemetry(
        strategy_id: impl Into<String>,
        strategy_version: u32,
        params_hash_or_summary: BTreeMap<String, f64>,
        counts: BTreeMap<String, u64>,
    ) -> Self {
        AgentContext::Telemetry {
            strategy_id: strategy_id.into(),
            strategy_version,
            params_hash_or_summary,
            counts,
        }
    }

    /// The fuller run-state context for post-run / management decisions.
    pub fn run_state(
        balance_krw: f64,
        positions: Vec<PositionSummary>,
        params: BTreeMap<String, f64>,
        run_summary: BTreeMap<String, f64>,
    ) -> Self {
        AgentContext::RunState { balance_krw, positions, params, run_summary }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_summary_serializes_only_symbol_side_quantity() {
        let p = PositionSummary {
            symbol: "005930.XKRX".to_string(),
            side: "LONG".to_string(),
            quantity: 10.0,
        };
        let value = serde_json::to_value(&p).unwrap();
        let keys: Vec<&str> =
            value.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["quantity", "side", "symbol"], "R9: no account-like keys");
    }

    #[test]
    fn context_forms_round_trip_with_form_tag() {
        let telemetry = AgentContext::telemetry(
            "orb-v0",
            3,
            BTreeMap::from([("gap_min_pct".to_string(), 3.0)]),
            BTreeMap::from([("decisions".to_string(), 7u64)]),
        );
        let run_state = AgentContext::run_state(
            1_000_000.0,
            vec![PositionSummary {
                symbol: "005930.XKRX".to_string(),
                side: "LONG".to_string(),
                quantity: 10.0,
            }],
            BTreeMap::new(),
            BTreeMap::from([("pnl_krw".to_string(), 1234.0)]),
        );
        for ctx in [telemetry, run_state] {
            let line = serde_json::to_string(&ctx).unwrap();
            assert!(line.contains("\"form\""), "tagged on form: {line}");
            let back: AgentContext = serde_json::from_str(&line).unwrap();
            assert_eq!(back, ctx);
        }
    }
}
