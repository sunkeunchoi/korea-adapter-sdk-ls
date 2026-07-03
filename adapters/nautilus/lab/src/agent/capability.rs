//! Deny-by-default capability model — gates each [`AgentIntent`] by action
//! capability and instrument scope (R3).
//!
//! Absence of a grant is a denial: an empty [`CapabilitySet`] denies every
//! intent, and a denied intent is returned as a typed [`CapabilityError`] so
//! the pipeline can record it in the envelope rather than silently drop it.
//!
//! `instrument_scope` and the management-tier capabilities are wire-compat
//! placeholders exercised by unit tests only this increment (plan U1/KTD3) —
//! the deferred risk-monitor is their first live consumer; only
//! [`ActionCapability::Research`] has a producing policy this round.

use std::collections::BTreeSet;

use nautilus_model::identifiers::InstrumentId;
use serde::{Deserialize, Serialize};

use crate::agent::intent::AgentIntent;

/// What an agent is allowed to observe. Mirrors the upstream capability
/// vocabulary (KTD2); not consulted by [`CapabilitySet::check_intent`] (intents
/// are actions), carried for wire-compat and future observation gating.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[non_exhaustive]
pub enum ObservationCapability {
    /// Observe top-of-book quotes.
    Quotes,
    /// Observe bar data.
    Bars,
    /// Observe account state (balances, margins).
    AccountState,
    /// Observe open positions.
    Positions,
    /// Observe order state.
    Orders,
    /// Observe position reports.
    PositionReports,
}

/// What an agent is allowed to do. Each [`AgentIntent`] variant maps to
/// exactly one required action capability (see
/// [`CapabilitySet::check_intent`]).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[non_exhaustive]
pub enum ActionCapability {
    /// Reduce or close open positions.
    ManagePositions,
    /// Cancel resting orders.
    ManageOrders,
    /// Pause or resume strategies.
    ManageStrategies,
    /// Adjust risk limits.
    AdjustRisk,
    /// Escalate to a human operator.
    Escalate,
    /// Emit research proposals (the one capability with a producing policy
    /// this increment).
    Research,
}

/// Why a capability check denied an intent (R3: typed, recordable — never a
/// silent drop).
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum CapabilityError {
    /// The intent's required action capability is not granted.
    #[error("action capability {required:?} not granted (deny-by-default)")]
    ActionDenied {
        /// The capability the intent requires.
        required: ActionCapability,
    },
    /// The intent targets an instrument outside the granted scope.
    #[error("instrument {instrument_id} not in granted instrument scope")]
    InstrumentDenied {
        /// The out-of-scope instrument.
        instrument_id: InstrumentId,
    },
}

/// A deny-by-default grant set: an intent passes [`CapabilitySet::check_intent`]
/// only when its required action capability — and, for instrument-targeted
/// intents, its instrument — is explicitly granted (R3).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySet {
    /// Granted observation capabilities.
    pub observations: BTreeSet<ObservationCapability>,
    /// Granted action capabilities.
    pub actions: BTreeSet<ActionCapability>,
    /// Instruments the agent may act on (consulted by instrument-targeted
    /// intents only).
    pub instrument_scope: BTreeSet<InstrumentId>,
}

impl CapabilitySet {
    /// Whether the observation capability is granted.
    pub fn can_observe(&self, capability: ObservationCapability) -> bool {
        self.observations.contains(&capability)
    }

    /// Whether the action capability is granted.
    pub fn can_act(&self, capability: ActionCapability) -> bool {
        self.actions.contains(&capability)
    }

    /// Whether the instrument is inside the granted scope.
    pub fn instrument_allowed(&self, instrument_id: &InstrumentId) -> bool {
        self.instrument_scope.contains(instrument_id)
    }

    /// Check an intent against the grant set, deny-by-default: the intent's
    /// required [`ActionCapability`] must be granted, and instrument-targeted
    /// intents (position/order management) must also be inside
    /// `instrument_scope`. Strategy/risk/escalation/research intents carry no
    /// instrument and skip the scope check.
    pub fn check_intent(&self, intent: &AgentIntent) -> Result<(), CapabilityError> {
        match intent {
            AgentIntent::ReducePosition { instrument_id, .. }
            | AgentIntent::ClosePosition { instrument_id, .. } => {
                self.require_action(ActionCapability::ManagePositions)?;
                self.require_instrument(instrument_id)
            }
            AgentIntent::CancelOrder { instrument_id, .. }
            | AgentIntent::CancelAllOrders { instrument_id, .. } => {
                self.require_action(ActionCapability::ManageOrders)?;
                self.require_instrument(instrument_id)
            }
            AgentIntent::PauseStrategy { .. } | AgentIntent::ResumeStrategy { .. } => {
                self.require_action(ActionCapability::ManageStrategies)
            }
            AgentIntent::AdjustRiskLimits { .. } => {
                self.require_action(ActionCapability::AdjustRisk)
            }
            AgentIntent::EscalateToHuman { .. } => {
                self.require_action(ActionCapability::Escalate)
            }
            AgentIntent::ProposeParameterChange { .. } => {
                self.require_action(ActionCapability::Research)
            }
        }
    }

    fn require_action(&self, required: ActionCapability) -> Result<(), CapabilityError> {
        if self.can_act(required) {
            Ok(())
        } else {
            Err(CapabilityError::ActionDenied { required })
        }
    }

    fn require_instrument(&self, instrument_id: &InstrumentId) -> Result<(), CapabilityError> {
        if self.instrument_allowed(instrument_id) {
            Ok(())
        } else {
            Err(CapabilityError::InstrumentDenied { instrument_id: *instrument_id })
        }
    }
}

#[cfg(test)]
mod tests {
    use nautilus_model::types::Quantity;

    use super::*;

    fn iid() -> InstrumentId {
        InstrumentId::from("005930.XKRX")
    }

    fn other_iid() -> InstrumentId {
        InstrumentId::from("000660.XKRX")
    }

    /// One intent per variant — all 9 (kept in sync with `intent.rs`).
    fn every_intent() -> Vec<AgentIntent> {
        vec![
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
                rationale: "widen universe".to_string(),
            },
        ]
    }

    fn full_grant() -> CapabilitySet {
        CapabilitySet {
            observations: BTreeSet::from([
                ObservationCapability::Quotes,
                ObservationCapability::Bars,
                ObservationCapability::AccountState,
                ObservationCapability::Positions,
                ObservationCapability::Orders,
                ObservationCapability::PositionReports,
            ]),
            actions: BTreeSet::from([
                ActionCapability::ManagePositions,
                ActionCapability::ManageOrders,
                ActionCapability::ManageStrategies,
                ActionCapability::AdjustRisk,
                ActionCapability::Escalate,
                ActionCapability::Research,
            ]),
            instrument_scope: BTreeSet::from([iid()]),
        }
    }

    #[test]
    fn missing_action_capability_is_denied_naming_the_required_capability() {
        // AE1: capability denial is typed and names what was required.
        let mut set = full_grant();
        set.actions.remove(&ActionCapability::ManagePositions);
        let err = set
            .check_intent(&AgentIntent::ClosePosition {
                instrument_id: iid(),
                reason: "eod".to_string(),
            })
            .unwrap_err();
        assert_eq!(
            err,
            CapabilityError::ActionDenied { required: ActionCapability::ManagePositions }
        );
        assert!(err.to_string().contains("ManagePositions"), "{err}");
    }

    #[test]
    fn instrument_outside_scope_is_denied() {
        let set = full_grant();
        let err = set
            .check_intent(&AgentIntent::CancelAllOrders {
                instrument_id: other_iid(),
                reason: "halt".to_string(),
            })
            .unwrap_err();
        assert_eq!(err, CapabilityError::InstrumentDenied { instrument_id: other_iid() });
        assert!(err.to_string().contains("000660.XKRX"), "{err}");
    }

    #[test]
    fn fully_granted_intents_pass() {
        let set = full_grant();
        for intent in every_intent() {
            assert_eq!(set.check_intent(&intent), Ok(()), "granted intent passes: {intent:?}");
        }
    }

    #[test]
    fn empty_capability_set_denies_every_intent_variant() {
        // Deny-by-default (R3): absence of a grant is a denial, for all 9
        // variants.
        let empty = CapabilitySet::default();
        let intents = every_intent();
        assert_eq!(intents.len(), 9, "one intent per AgentIntent variant");
        for intent in intents {
            let err = empty.check_intent(&intent).unwrap_err();
            assert!(
                matches!(err, CapabilityError::ActionDenied { .. }),
                "empty set denies on the action grant first: {intent:?} -> {err}"
            );
        }
    }

    #[test]
    fn observation_and_scope_helpers_answer_membership() {
        let set = full_grant();
        assert!(set.can_observe(ObservationCapability::Bars));
        assert!(set.can_act(ActionCapability::Research));
        assert!(set.instrument_allowed(&iid()));
        assert!(!set.instrument_allowed(&other_iid()));
        let empty = CapabilitySet::default();
        assert!(!empty.can_observe(ObservationCapability::Bars));
        assert!(!empty.can_act(ActionCapability::Research));
        assert!(!empty.instrument_allowed(&iid()));
    }
}
