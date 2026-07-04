//! Runtime action — the *lowered* form of an approved [`crate::agent::intent::AgentIntent`]
//! (R1). Kept close to intent granularity: this increment lowers research
//! intents to a recorded [`RuntimeAction::ResearchCommand`] (a research proposal
//! **never places orders**), and management intents to a
//! [`RuntimeAction::ManagementCommand`] placeholder.

use serde::{Deserialize, Serialize};

/// The lowered runtime form of an approved intent. Tagged on `"type"` to mirror
/// the upstream wire format (KTD2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum RuntimeAction {
    /// A recorded research command (e.g. a parameter-change proposal). Lowering
    /// a research intent produces a record, not an order.
    ResearchCommand {
        /// What the research command records.
        description: String,
    },
    /// A management command placeholder for management-tier intents (no
    /// producing policy this increment).
    ManagementCommand {
        /// What the management command would do.
        description: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_round_trip_with_type_tag() {
        let all = vec![
            RuntimeAction::ResearchCommand {
                description: "propose gap_min_pct 3.0 -> 4.0".to_string(),
            },
            RuntimeAction::ManagementCommand {
                description: "pause orb-v0".to_string(),
            },
        ];
        for action in all {
            let line = serde_json::to_string(&action).unwrap();
            assert!(line.contains("\"type\""), "tagged on type: {line}");
            let back: RuntimeAction = serde_json::from_str(&line).unwrap();
            assert_eq!(back, action);
        }
    }
}
