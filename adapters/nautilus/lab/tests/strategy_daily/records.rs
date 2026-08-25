//! Reading what the strategy recorded — the flattened decision record and the
//! readers every scenario in this suite asserts through.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use nautilus_ls_lab::agent::envelope::SignalKind;
use nautilus_ls_lab::agent::sink::DecisionSink;
use nautilus_ls_lab::runner::backtest_daily::DailyRunOutcome;
use nautilus_ls_lab::strategy::daily::EntryRefusal;

use crate::fixture::kst_date;

/// One strategy decision record, flattened for assertion. The record on the refusal
/// path is the **only** evidence a fail-closed gate ran (AE3), so every gate scenario
/// asserts its presence rather than only the absence of a trade.
#[derive(Debug, Clone)]
pub(crate) struct Rec {
    pub(crate) date: NaiveDate,
    pub(crate) symbol: String,
    pub(crate) kind: SignalKind,
    pub(crate) filter: Option<String>,
    pub(crate) values: BTreeMap<String, f64>,
}

/// Every record the *strategy* emitted, dropping the runner's universe envelopes.
pub(crate) fn strategy_records(sink: &DecisionSink) -> Vec<Rec> {
    sink.snapshot()
        .into_iter()
        .filter_map(|e| {
            let ts = e.ts_event;
            let d = e.decision_detail?;
            if matches!(d.kind, SignalKind::Universe) {
                return None;
            }
            Some(Rec {
                date: kst_date(ts),
                symbol: d.symbol,
                kind: d.kind,
                filter: d.filter,
                values: d.values,
            })
        })
        .collect()
}

pub(crate) fn refusals<'a>(recs: &'a [Rec], reason: EntryRefusal) -> Vec<&'a Rec> {
    recs.iter()
        .filter(|r| {
            matches!(r.kind, SignalKind::OrderRejectedSizing)
                && r.filter.as_deref() == Some(reason.as_str())
        })
        .collect()
}

pub(crate) fn placed<'a>(recs: &'a [Rec]) -> Vec<&'a Rec> {
    recs.iter().filter(|r| matches!(r.kind, SignalKind::OrderPlaced)).collect()
}

/// The in-range session index of a timestamp, on the run's own session calendar.
pub(crate) fn session_index(outcome: &DailyRunOutcome, ns: u64) -> usize {
    let d = kst_date(ns);
    outcome
        .selection
        .sessions
        .iter()
        .position(|s| s.date == d)
        .unwrap_or_else(|| panic!("{d} is not an in-range session"))
}

pub(crate) fn close_idx(outcome: &DailyRunOutcome, p: &nautilus_model::position::Position) -> Option<usize> {
    p.ts_closed.map(|t| session_index(outcome, t.as_u64()))
}

pub(crate) fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0)
}
