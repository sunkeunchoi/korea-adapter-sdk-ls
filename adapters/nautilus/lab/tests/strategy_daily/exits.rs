//! Engine scenarios — the exit mechanics.
//!
//! The half of the root module's engine group that is about *when a position leaves*:
//! hold expiry on the loop's own session calendar, the data-gap gate that keeps that
//! calendar enforceable, the stop breach, and the duplicate bar that must not shorten a
//! frozen hold. Split out of `strategy_daily.rs` for size only — every scenario here
//! needs a real fill and runs through a real `run_daily`, exactly as the root's do.

use std::collections::BTreeSet;

use nautilus_ls::ingest::write_bars;
use nautilus_ls_lab::agent::envelope::SignalKind;
use nautilus_ls_lab::agent::sink::DecisionSink;
use nautilus_ls_lab::params_daily::DailyParams;
use nautilus_ls_lab::runner::backtest_daily::run_daily;
use nautilus_ls_lab::strategy::daily::{
    rank_by_placeholder_signal, AdjustmentBasisShifts, DailyStrategy,
};
use nautilus_model::data::Bar;
use tempfile::tempdir;

use crate::fixture::{
    build_fixture, cfg_range, daily_bar, descending_turnover, series, SymbolSpec, CODES,
    FIRST_IN_RANGE,
};
use crate::gates::day;
use crate::records::{close_idx, session_index, strategy_records};

/// **Scenario 5.** With the stop unbreached, a position opened at session `N` closes
/// at exactly `N + hold` — not earlier, not later.
#[tokio::test]
async fn an_unbreached_position_closes_at_exactly_entry_plus_hold() {
    let dir = tempdir().unwrap();
    let specs = descending_turnover(1);
    build_fixture(dir.path(), &specs).await;

    let hold = 3;
    let sink = DecisionSink::new();
    let params = DailyParams {
        holding_period_sessions: hold,
        target_m: 1,
        max_concurrent: 8,
        ..DailyParams::default()
    };
    let outcome = run_daily(
        cfg_range(dir.path(), FIRST_IN_RANGE, FIRST_IN_RANGE + 11, 1),
        sink.clone(),
        rank_by_placeholder_signal,
        DailyStrategy::factory(params, sink.clone(), AdjustmentBasisShifts::none()),
    )
    .await
    .unwrap();

    let closed: Vec<&nautilus_model::position::Position> =
        outcome.positions.iter().filter(|p| p.is_closed()).collect();
    assert!(closed.len() >= 2, "several completed round trips: {}", outcome.positions.len());
    for p in &closed {
        let opened = session_index(&outcome, p.ts_opened.as_u64());
        let shut = close_idx(&outcome, p).unwrap();
        assert_eq!(
            shut - opened,
            hold,
            "opened at session {opened}, closed at {shut}: hold elapsed is counted in \
             loop-supplied session ordinals (R23)"
        );
    }
    let recs = strategy_records(&sink);
    assert!(
        recs.iter().any(|r| matches!(r.kind, SignalKind::TimeExit)),
        "the exits are hold-expiry exits: {recs:#?}"
    );
    assert!(
        !recs.iter().any(|r| matches!(r.kind, SignalKind::StopHit)),
        "nothing breached its stop in this fixture: {recs:#?}"
    );
}

/// **Scenario 5b (the data-gap gate).** A held symbol that contributes no bar to a
/// session **aborts the run** rather than silently outliving its frozen hold.
///
/// The frozen `holding_period_sessions` is a pre-registered term. Both exits fire from
/// [`DataActor::on_bar`], so a session that delivers no bar for a held position hands
/// the strategy no callback for it at all: before this gate the exit slid to whichever
/// later session did deliver one, and the run still finalized green. Measured on this
/// exact fixture, the position opened at session 0 under a 3-session hold exited at
/// `elapsed_sessions = 5` — a 67% overrun of a term that is not supposed to move.
///
/// The session the gap falls on is deliberately one whose batch is **non-empty**: the
/// second symbol keeps printing, so the empty-batch skip is not what catches this.
#[tokio::test]
async fn a_held_symbol_with_no_bar_aborts_the_run_instead_of_outliving_its_frozen_hold() {
    let dir = tempdir().unwrap();
    let mut specs = descending_turnover(2);
    // The top-ranked name goes dark for the two sessions straddling its hold expiry,
    // then trades again. The second name keeps printing, so every batch is non-empty.
    specs[0].gaps = BTreeSet::from([FIRST_IN_RANGE + 3, FIRST_IN_RANGE + 4]);
    build_fixture(dir.path(), &specs).await;

    let sink = DecisionSink::new();
    let params = DailyParams {
        holding_period_sessions: 3,
        target_m: 1,
        max_concurrent: 8,
        ..DailyParams::default()
    };
    let error = run_daily(
        cfg_range(dir.path(), FIRST_IN_RANGE, FIRST_IN_RANGE + 11, 1),
        sink.clone(),
        rank_by_placeholder_signal,
        DailyStrategy::factory(params, sink.clone(), AdjustmentBasisShifts::none()),
    )
    .await
    .expect_err("the run must fail closed on the gap, not finalize green over it");

    let message = format!("{error:#}");
    assert!(
        message.contains(&specs[0].id().to_string()),
        "the error names the held symbol whose bar was missing: {message}"
    );
    // The FIRST gap session, not a later one: the gate fires on the session the
    // contract is first unenforceable on.
    assert!(
        message.contains(&day(FIRST_IN_RANGE + 3).to_string()),
        "the error names the session the bar was missing from: {message}"
    );
    assert!(
        !message.contains(&specs[1].id().to_string()),
        "the name that kept printing is not implicated: {message}"
    );
}

/// **Scenario 6.** A daily bar whose low breaches the entry-fixed stop closes the
/// position on that session, well before hold expiry.
#[tokio::test]
async fn a_bar_breaching_the_entry_stop_closes_the_position_that_session() {
    let dir = tempdir().unwrap();
    let mut spec = SymbolSpec::new(CODES[0], 50_000, 1_000_000);
    // The entry fills at session 0's close (50,200) with a stop 1.5 × ATR(1) = 1,500
    // below it, at 48,700. Session 2's low of 47,800 breaches it; session 1's
    // (49,800) does not.
    spec.lows.insert(FIRST_IN_RANGE + 2, 47_800);
    build_fixture(dir.path(), std::slice::from_ref(&spec)).await;

    let sink = DecisionSink::new();
    let params = DailyParams { target_m: 1, ..DailyParams::default() }; // hold 16
    let outcome = run_daily(
        cfg_range(dir.path(), FIRST_IN_RANGE, FIRST_IN_RANGE + 7, 1),
        sink.clone(),
        rank_by_placeholder_signal,
        DailyStrategy::factory(params, sink.clone(), AdjustmentBasisShifts::none()),
    )
    .await
    .unwrap();

    let first = outcome
        .positions
        .iter()
        .min_by_key(|p| p.ts_opened.as_u64())
        .expect("a position opened");
    assert_eq!(session_index(&outcome, first.ts_opened.as_u64()), 0);
    assert_eq!(
        close_idx(&outcome, first),
        Some(2),
        "closed on the breaching session, not at the 16-session hold expiry"
    );

    let recs = strategy_records(&sink);
    let stop = recs
        .iter()
        .find(|r| matches!(r.kind, SignalKind::StopHit))
        .unwrap_or_else(|| panic!("a stop-hit record: {recs:#?}"));
    assert_eq!(stop.date, day(FIRST_IN_RANGE + 2));
    assert!(stop.values["bar_low"] <= stop.values["stop"], "the low breached the stop: {stop:#?}");
    assert!(
        stop.values["elapsed_sessions"] < 16.0,
        "the stop fired BEFORE hold expiry: {stop:#?}"
    );
}

/// **Scenario 7.** A second bar delivered for the **same session date** (a duplicate
/// the runner's `ts_event`-keyed dedupe does not catch) leaves hold elapsed
/// unchanged: it is counted on the loop-supplied session ordinal, never on bar
/// callbacks.
#[tokio::test]
async fn a_duplicate_bar_for_the_same_session_date_does_not_shorten_the_hold() {
    let dir = tempdir().unwrap();
    let spec = SymbolSpec::new(CODES[0], 50_000, 1_000_000);
    build_fixture(dir.path(), std::slice::from_ref(&spec)).await;

    // A second copy of session 2's bar, one nanosecond later — the same KST session
    // date, so the runner buckets it into the same batch, but a DIFFERENT `ts_event`,
    // so its (instrument, ts) dedupe key does not collide and both are delivered.
    let dup_day = FIRST_IN_RANGE + 2;
    let original = daily_bar(spec.id(), series(&spec)[dup_day].clone());
    let ts = original.ts_event.as_u64() + 1;
    let duplicate = Bar::new(
        original.bar_type,
        original.open,
        original.high,
        original.low,
        original.close,
        original.volume,
        ts.into(),
        ts.into(),
    );
    write_bars(&dir.path().join("catalog"), vec![duplicate]).await.unwrap();

    let hold = 4;
    let sink = DecisionSink::new();
    let params = DailyParams {
        holding_period_sessions: hold,
        target_m: 1,
        max_concurrent: 8,
        ..DailyParams::default()
    };
    let outcome = run_daily(
        cfg_range(dir.path(), FIRST_IN_RANGE, FIRST_IN_RANGE + 7, 1),
        sink.clone(),
        rank_by_placeholder_signal,
        DailyStrategy::factory(params, sink.clone(), AdjustmentBasisShifts::none()),
    )
    .await
    .unwrap();

    assert!(
        outcome.duplicate_drops.is_empty(),
        "the duplicate was NOT deduped away — it really reached the strategy: {:?}",
        outcome.duplicate_drops
    );
    assert_eq!(
        outcome.batches[2].bars, 2,
        "session 2's batch carried two bars for one session date: {:?}",
        outcome.batches[2]
    );

    let first = outcome
        .positions
        .iter()
        .min_by_key(|p| p.ts_opened.as_u64())
        .expect("a position opened");
    assert_eq!(session_index(&outcome, first.ts_opened.as_u64()), 0);
    assert_eq!(
        close_idx(&outcome, first),
        Some(hold),
        "the frozen hold is unchanged by the duplicate callback"
    );

    // The per-session take refusals are likewise emitted once per session ordinal,
    // not once per callback.
    let recs = strategy_records(&sink);
    assert!(
        !recs.iter().any(|r| matches!(r.kind, SignalKind::StopHit)),
        "the duplicate did not fire a second stop check into an exit: {recs:#?}"
    );
}
