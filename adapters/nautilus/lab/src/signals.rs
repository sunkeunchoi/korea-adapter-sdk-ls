//! Per-decision signal log (KTD9, R6, AE2) — one event per *decision*, never per
//! bar. Volume is O(universe × transitions), so the log stays agent-readable for a
//! whole session. Serialized as JSONL, one event per line.
//!
//! Emitted events:
//! - one [`SignalKind::Universe`] per candidate at selection (accept/reject + the
//!   rejecting filter + the signal values at decision time),
//! - one per state transition on a selected symbol (breakout, order placed / rejected
//!   by sizing, stop hit, time exit),
//! - one [`SignalKind::SessionSummary`] per selected symbol at end of session
//!   carrying the extreme signal values observed.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

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
    /// A held position was flattened at the time-flat deadline.
    TimeExit,
    /// End-of-session summary for a selected symbol (extreme values observed).
    SessionSummary,
}

/// One decision event. `values` carries the signal readings at decision time
/// (gap %, turnover, range high/low, breakout price, …) as a sorted map so the log
/// is deterministic and self-describing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalEvent {
    /// Event time as UTC unix nanoseconds (the bar's `ts_event` for intraday
    /// decisions, or the session-open instant for universe decisions).
    pub ts_event: u64,
    /// The instrument the decision concerns (`{shcode}.XKRX`).
    pub symbol: String,
    /// What kind of decision this is.
    pub kind: SignalKind,
    /// For a [`SignalKind::Universe`] event, whether the candidate was accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<Decision>,
    /// The rejecting filter's name when a candidate was rejected (`gap`,
    /// `turnover_rank`), or the reason a transition fired.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// The signal values at decision time (sorted keys → deterministic output).
    pub values: BTreeMap<String, f64>,
}

impl SignalEvent {
    /// A universe accept/reject event.
    pub fn universe(
        ts_event: u64,
        symbol: impl Into<String>,
        decision: Decision,
        filter: Option<String>,
        values: BTreeMap<String, f64>,
    ) -> Self {
        SignalEvent {
            ts_event,
            symbol: symbol.into(),
            kind: SignalKind::Universe,
            decision: Some(decision),
            filter,
            values,
        }
    }

    /// A state-transition event on a selected symbol.
    pub fn transition(
        ts_event: u64,
        symbol: impl Into<String>,
        kind: SignalKind,
        values: BTreeMap<String, f64>,
    ) -> Self {
        SignalEvent {
            ts_event,
            symbol: symbol.into(),
            kind,
            decision: None,
            filter: None,
            values,
        }
    }
}

/// A thread-safe collector for signal events. The nautilus engine owns the strategy
/// (and runs it on a blocking thread), so the runner holds a clone of this sink and
/// drains it after the run to write `signals.jsonl` (KTD2 atomic finalize).
#[derive(Debug, Clone, Default)]
pub struct SignalSink {
    events: Arc<Mutex<Vec<SignalEvent>>>,
}

impl SignalSink {
    /// A fresh, empty sink.
    pub fn new() -> Self {
        SignalSink::default()
    }

    /// Record one decision event.
    pub fn emit(&self, event: SignalEvent) {
        self.events.lock().unwrap_or_else(|e| e.into_inner()).push(event);
    }

    /// A snapshot copy of all events recorded so far (insertion order).
    pub fn snapshot(&self) -> Vec<SignalEvent> {
        self.events.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// The number of events recorded.
    pub fn len(&self) -> usize {
        self.events.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Whether no events have been recorded.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Render a slice of events as JSONL (one compact JSON object per line, trailing
/// newline). Used by the artifact writer to produce `signals.jsonl`.
pub fn to_jsonl(events: &[SignalEvent]) -> serde_json::Result<String> {
    let mut out = String::new();
    for e in events {
        out.push_str(&serde_json::to_string(e)?);
        out.push('\n');
    }
    Ok(out)
}

/// Parse a JSONL signal log back into events (round-trip helper for tests + agents).
pub fn from_jsonl(s: &str) -> serde_json::Result<Vec<SignalEvent>> {
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vals(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn universe_reject_names_filter_and_values() {
        let e = SignalEvent::universe(
            1,
            "005930.XKRX",
            Decision::Reject,
            Some("gap".to_string()),
            vals(&[("gap_pct", 1.2), ("prior_close", 60000.0)]),
        );
        let line = serde_json::to_string(&e).unwrap();
        assert!(line.contains("\"reject\""));
        assert!(line.contains("\"gap\""));
        assert!(line.contains("\"gap_pct\":1.2"));
    }

    #[test]
    fn jsonl_round_trips() {
        let sink = SignalSink::new();
        sink.emit(SignalEvent::universe(1, "A.XKRX", Decision::Accept, None, vals(&[("g", 5.0)])));
        sink.emit(SignalEvent::transition(2, "A.XKRX", SignalKind::Breakout, vals(&[("hi", 61000.0)])));
        let events = sink.snapshot();
        let text = to_jsonl(&events).unwrap();
        assert_eq!(text.lines().count(), 2);
        let back = from_jsonl(&text).unwrap();
        assert_eq!(back, events);
    }

    #[test]
    fn accept_event_omits_filter() {
        let e = SignalEvent::universe(1, "A.XKRX", Decision::Accept, None, BTreeMap::new());
        let line = serde_json::to_string(&e).unwrap();
        assert!(!line.contains("filter"), "an accept has no rejecting filter: {line}");
    }
}
