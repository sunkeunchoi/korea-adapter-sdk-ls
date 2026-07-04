//! Agent intent — what an agent *wants* done, before capability / guardrail /
//! lowering stages run (R1, R5).
//!
//! Wire-compatible with upstream `nautilus_agents` intent tagging (KTD2:
//! `#[serde(tag = "type")]`, variant names are the wire tags), typed natively on
//! pinned 0.60 `nautilus_model` types (which serde as strings).
//!
//! Only [`AgentIntent::ProposeParameterChange`] has a producing policy this
//! increment; the management-tier variants (reduce/close/cancel/pause/resume/
//! adjust/escalate) are deliberate wire-compat placeholders exercised by unit
//! tests only.

use nautilus_model::identifiers::InstrumentId;
use nautilus_model::types::Quantity;
use serde::{Deserialize, Serialize};

/// What an agent wants done. Tagged on `"type"` to mirror the upstream wire
/// format (KTD2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum AgentIntent {
    /// Reduce an open position to a target quantity.
    ReducePosition {
        /// The instrument whose position to reduce.
        instrument_id: InstrumentId,
        /// The quantity to reduce the position *to*.
        target_quantity: Quantity,
        /// Why the agent wants the reduction.
        reason: String,
    },
    /// Close an open position entirely.
    ClosePosition {
        /// The instrument whose position to close.
        instrument_id: InstrumentId,
        /// Why the agent wants the close.
        reason: String,
    },
    /// Cancel one resting order.
    CancelOrder {
        /// The instrument the order rests on.
        instrument_id: InstrumentId,
        /// The client order id of the order to cancel.
        client_order_id: String,
        /// Why the agent wants the cancel.
        reason: String,
    },
    /// Cancel every resting order on an instrument.
    CancelAllOrders {
        /// The instrument whose orders to cancel.
        instrument_id: InstrumentId,
        /// Why the agent wants the sweep.
        reason: String,
    },
    /// Pause a running strategy.
    PauseStrategy {
        /// The strategy to pause.
        strategy_id: String,
        /// Why the agent wants the pause.
        reason: String,
    },
    /// Resume a paused strategy.
    ResumeStrategy {
        /// The strategy to resume.
        strategy_id: String,
        /// Why the agent wants the resume.
        reason: String,
    },
    /// Adjust risk limits (free-form description this increment).
    AdjustRiskLimits {
        /// What adjustment the agent wants.
        description: String,
    },
    /// Escalate a situation to a human operator.
    EscalateToHuman {
        /// Why the agent is escalating.
        reason: String,
    },
    /// Propose a strategy parameter change — the one research intent with a
    /// producing policy this increment.
    ProposeParameterChange {
        /// The strategy whose parameter to change.
        strategy_id: String,
        /// The parameter's name.
        parameter: String,
        /// The parameter's current value.
        current_value: f64,
        /// The proposed new value.
        proposed_value: f64,
        /// The evidence-backed rationale for the change.
        rationale: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iid() -> InstrumentId {
        InstrumentId::from("005930.XKRX")
    }

    #[test]
    fn every_variant_round_trips() {
        let all: Vec<AgentIntent> = vec![
            AgentIntent::ReducePosition {
                instrument_id: iid(),
                target_quantity: Quantity::from(5),
                reason: "risk".to_string(),
            },
            AgentIntent::ClosePosition { instrument_id: iid(), reason: "eod".to_string() },
            AgentIntent::CancelOrder {
                instrument_id: iid(),
                client_order_id: "O-001".to_string(),
                reason: "stale".to_string(),
            },
            AgentIntent::CancelAllOrders { instrument_id: iid(), reason: "halt".to_string() },
            AgentIntent::PauseStrategy {
                strategy_id: "orb-v0".to_string(),
                reason: "drawdown".to_string(),
            },
            AgentIntent::ResumeStrategy {
                strategy_id: "orb-v0".to_string(),
                reason: "recovered".to_string(),
            },
            AgentIntent::AdjustRiskLimits { description: "halve per-trade risk".to_string() },
            AgentIntent::EscalateToHuman { reason: "ambiguous fill state".to_string() },
            AgentIntent::ProposeParameterChange {
                strategy_id: "orb-v0".to_string(),
                parameter: "gap_min_pct".to_string(),
                current_value: 3.0,
                proposed_value: 4.0,
                rationale: "gap<4% universe picks lost money in runs 12-18".to_string(),
            },
        ];
        for intent in all {
            let line = serde_json::to_string(&intent).unwrap();
            assert!(line.contains("\"type\""), "tagged on type: {line}");
            let back: AgentIntent = serde_json::from_str(&line).unwrap();
            assert_eq!(back, intent);
        }
    }
}
