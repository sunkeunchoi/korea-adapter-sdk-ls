//! In-run decision sink (R6) — a thread-safe collector for per-decision
//! [`DecisionEnvelope`]s. The nautilus engine owns the strategy (and runs it on
//! a blocking thread), so the runner holds a clone of this sink and drains it
//! after the run to write `decisions.jsonl` (KTD2 atomic finalize). Volume is
//! O(universe × transitions) — one envelope per *decision*, never per bar, so
//! the stream stays agent-readable for a whole session.

use std::sync::{Arc, Mutex};

use crate::agent::envelope::DecisionEnvelope;

/// A thread-safe collector for decision envelopes (the in-run counterpart of
/// the cross-run [`crate::agent::recording::DecisionRecorder`]).
#[derive(Debug, Clone, Default)]
pub struct DecisionSink {
    envelopes: Arc<Mutex<Vec<DecisionEnvelope>>>,
}

impl DecisionSink {
    /// A fresh, empty sink.
    pub fn new() -> Self {
        DecisionSink::default()
    }

    /// Record one decision envelope.
    pub fn emit(&self, envelope: DecisionEnvelope) {
        self.envelopes.lock().unwrap_or_else(|e| e.into_inner()).push(envelope);
    }

    /// A snapshot copy of all envelopes recorded so far (insertion order).
    pub fn snapshot(&self) -> Vec<DecisionEnvelope> {
        self.envelopes.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// The number of envelopes recorded.
    pub fn len(&self) -> usize {
        self.envelopes.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Whether no envelopes have been recorded.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::agent::context::AgentContext;
    use crate::agent::envelope::{Decision, DecisionDetail, DecisionEnvelope, DecisionTrigger};

    fn telemetry(ts_event: u64) -> DecisionEnvelope {
        DecisionEnvelope::telemetry(
            ts_event,
            DecisionTrigger::StateChange { description: "unit test".to_string() },
            DecisionDetail::universe("005930.XKRX", Decision::Accept, None, BTreeMap::new()),
            AgentContext::telemetry("orb", 0, BTreeMap::new(), BTreeMap::new()),
        )
    }

    #[test]
    fn sink_collects_in_insertion_order() {
        let sink = DecisionSink::new();
        assert!(sink.is_empty());
        sink.emit(telemetry(1));
        sink.emit(telemetry(2));
        assert_eq!(sink.len(), 2);
        assert!(!sink.is_empty());
        let snap = sink.snapshot();
        assert_eq!(snap[0].ts_event, 1);
        assert_eq!(snap[1].ts_event, 2);
        // A clone shares the same underlying store (the runner drains the
        // engine thread's emissions through its own clone).
        let clone = sink.clone();
        clone.emit(telemetry(3));
        assert_eq!(sink.len(), 3);
    }
}
