//! U1/U2 — the daily-resolution, multi-session-hold backtest path. Offline: a fixture
//! `ParquetDataCatalog` (wiremock-ingested instruments + directly-written daily bars)
//! feeds the streaming daily runner. No credentials, no network beyond the wiremock
//! instrument masters.
//!
//! Each `lab/tests/*.rs` is its own binary and there is no shared test-support module,
//! so the catalog scaffold below is deliberately duplicated from `backtest_run.rs`
//! rather than imported. A daily-only fixture is roughly one bar per symbol-session,
//! so it is sized to reach hold expiry rather than truncated (KTD10).
//!
//! The strategy under the runner is the test-only [`AlwaysEnter`] below, not the ORB
//! strategy and not the (unbuilt) daily strategy: U1's carry-over proof must not
//! depend on U4's ranking, stop, or hold semantics.

// A test target's crate root resolves `mod` against `tests/`, so each child needs its
// path spelled out. The children stay in `tests/backtest_daily_run/`, which cargo does
// not treat as a test target, so this suite remains ONE test binary.
#[path = "backtest_daily_run/always_enter.rs"]
mod always_enter;
#[path = "backtest_daily_run/entry_point.rs"]
mod entry_point;
#[path = "backtest_daily_run/entry_risk.rs"]
mod entry_risk;
#[path = "backtest_daily_run/fixture.rs"]
mod fixture;
#[path = "backtest_daily_run/observation.rs"]
mod observation;

use std::collections::{BTreeSet, HashMap};

use chrono::NaiveDate;
use nautilus_ls::ingest::{build_daily_bar, write_bars, BarKind};
use nautilus_ls_lab::agent::envelope::{Decision, DecisionEnvelope};
use nautilus_ls_lab::agent::sink::DecisionSink;
use nautilus_ls_lab::runner::backtest_daily::{run_daily, select_daily_sessions};
use nautilus_ls_lab::strategy::orb::UniverseCandidate;
use nautilus_model::identifiers::{InstrumentId, PositionId};
use tempfile::tempdir;

use always_enter::{always_enter, AlwaysEnterConfig, BarWitness};
use fixture::{
    build_daily_fixture, build_daily_fixture_with_gaps, cfg, daily_json, kst_date, rank_all,
    rank_only, RANGE_END, RANGE_START, SESSION_DAYS,
};

// ---------------------------------------------------------------------------
// E. Engine-phase scenarios
// ---------------------------------------------------------------------------

/// **The carry-over test.** A position entered on the first session of a
/// 21-session fixture is still open at session 5 and closes at hold expiry,
/// appearing exactly once in the single post-`end()` cache read (R1, R4, KTD1).
///
/// This is U1's reason to exist: against a per-session engine the position cannot
/// survive a session boundary at all.
#[tokio::test]
async fn a_position_entered_on_session_one_is_still_open_at_session_five_and_closes_at_hold_expiry()
{
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    let witness = BarWitness::default();
    let sink = DecisionSink::new();
    let hold = 6;
    let outcome = run_daily(
        cfg(dir.path(), 1),
        sink,
        rank_only(&["005930.XKRX"]),
        always_enter(
            AlwaysEnterConfig { hold_sessions: hold, reenter: false, ..Default::default() },
            witness.clone(),
        ),
    )
    .await
    .unwrap();

    let id = InstrumentId::from("005930.XKRX");
    let dates: Vec<NaiveDate> = outcome.selection.sessions.iter().map(|s| s.date).collect();
    let index_of = |d: NaiveDate| dates.iter().position(|x| *x == d).unwrap();

    assert_eq!(
        outcome.positions.len(),
        1,
        "exactly one position survives the whole stream and is read once: {:?}",
        outcome.positions.iter().map(|p| (p.id, p.ts_opened, p.ts_closed)).collect::<Vec<_>>()
    );
    let p = &outcome.positions[0];
    assert!(p.is_closed(), "the carried position closes at hold expiry, not at range end");
    // KTD12: the venue mints a distinct position per open. Under `OmsType::Netting`
    // the id is the constant `{instrument_id}-{strategy_id}`, one per symbol for the
    // whole run, and a re-entry silently snapshots the earlier round trip out of the
    // live index `cache.positions()` reads.
    assert_ne!(
        p.id.to_string(),
        format!("{id}-always-enter-v1"),
        "the daily venue must not be Netting: a constant per-symbol position id \
         collapses every round trip on that symbol"
    );
    let opened = index_of(kst_date(p.ts_opened.as_u64()));
    let closed = index_of(kst_date(p.ts_closed.unwrap().as_u64()));
    assert_eq!(
        closed - opened,
        hold,
        "hold elapsed is counted in distinct session dates supplied by the loop (R23): \
         opened at session {opened}, closed at session {closed}"
    );
    assert!(
        closed >= 4,
        "the position is still open at session 5 (index 4); it closed at index {closed}"
    );
    // Held on session index 4, so the runner kept it in that session's batch.
    assert!(
        outcome.batches[4].held.contains(&id),
        "session 5's pre-batch step sees the position still held: {:?}",
        outcome.batches[4]
    );
    assert!(
        outcome.batches[4].bars > 0 && !outcome.batches[4].skipped,
        "the held symbol's daily bar is delivered on session 5: {:?}",
        outcome.batches[4]
    );
}

/// A position is stopped out by a bar for a symbol that was **not** re-selected on
/// that session — held symbols stay in the batch regardless of the ranking.
#[tokio::test]
async fn a_held_symbol_stops_out_on_a_session_it_was_not_reselected_on() {
    let dir = tempdir().unwrap();
    // 005930 crashes on session index 6 (the 6th in-range session).
    let crash = HashMap::from([("005930.XKRX", HashMap::from([(6usize, 40_000i64)]))]);
    build_daily_fixture(dir.path(), &crash).await;
    let witness = BarWitness::default();
    let outcome = run_daily(
        cfg(dir.path(), 1),
        DecisionSink::new(),
        // Only rankable on the first in-range session: never re-selected afterwards.
        {
            let seen = std::sync::atomic::AtomicUsize::new(0);
            move |candidates: &[UniverseCandidate]| {
                let n = seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n == 0 && candidates.iter().any(|c| c.symbol == "005930.XKRX") {
                    vec!["005930.XKRX".to_string()]
                } else {
                    Vec::new()
                }
            }
        },
        always_enter(
            AlwaysEnterConfig {
                hold_sessions: 20,
                stop_below: Some(5_000),
                reenter: false,
                ..Default::default()
            },
            witness.clone(),
        ),
    )
    .await
    .unwrap();

    let id = InstrumentId::from("005930.XKRX");
    assert_eq!(outcome.positions.len(), 1, "one entry, one stop-out");
    let p = &outcome.positions[0];
    assert!(p.is_closed(), "the stop fires on a session the symbol was never re-ranked on");
    let dates: Vec<NaiveDate> = outcome.selection.sessions.iter().map(|s| s.date).collect();
    let closed = dates.iter().position(|d| *d == kst_date(p.ts_closed.unwrap().as_u64())).unwrap();
    assert!(closed <= 7, "closed at the crash session, not at hold expiry: index {closed}");
    // Every session after the first has an empty take yet keeps the held symbol.
    let stop_session = &outcome.batches[closed.min(outcome.batches.len() - 1)];
    assert!(stop_session.taken.is_empty(), "the symbol was NOT re-selected: {stop_session:?}");
    // Session 0 is the entry session (nothing held yet); from session 1 to the
    // stop-out the symbol is held and therefore in every batch despite never being
    // re-ranked.
    assert!(
        outcome.batches[1..=closed].iter().all(|b| b.held.contains(&id) && !b.skipped),
        "the held symbol stayed in every batch up to the stop-out: {:?}",
        &outcome.batches[1..=closed]
    );
}

/// A symbol entered, held to expiry, and entered again later yields two DISTINCT
/// positions in the single cache read — the Hedging OMS mints a position per open
/// (KTD12, R19, AE1).
#[tokio::test]
async fn a_reentered_symbol_yields_two_distinct_positions() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    let outcome = run_daily(
        cfg(dir.path(), 1),
        DecisionSink::new(),
        rank_only(&["005930.XKRX"]),
        always_enter(
            AlwaysEnterConfig { hold_sessions: 5, reenter: true, ..Default::default() },
            BarWitness::default(),
        ),
    )
    .await
    .unwrap();

    let closed: Vec<&nautilus_model::position::Position> =
        outcome.positions.iter().filter(|p| p.is_closed()).collect();
    assert!(
        closed.len() >= 2,
        "a re-entry mints a SECOND position rather than reopening the first: {:?}",
        outcome.positions.iter().map(|p| (p.id, p.ts_opened, p.ts_closed)).collect::<Vec<_>>()
    );
    let ids: std::collections::BTreeSet<PositionId> =
        outcome.positions.iter().map(|p| p.id).collect();
    assert_eq!(ids.len(), outcome.positions.len(), "every position id is distinct");
    assert!(
        outcome.positions.iter().all(|p| p.instrument_id == InstrumentId::from("005930.XKRX")),
        "all on the same symbol"
    );
}

/// A session whose batch is empty skips the whole `clear_data` / `add_data` / `run`
/// cycle without erroring — `add_data` errors on an empty slice.
#[tokio::test]
async fn an_empty_batch_session_skips_the_cycle_without_erroring() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    // Nothing is ever rankable → every session's take is empty and nothing is held.
    let outcome = run_daily(
        cfg(dir.path(), 8),
        DecisionSink::new(),
        |_: &[UniverseCandidate]| Vec::new(),
        always_enter(AlwaysEnterConfig::default(), BarWitness::default()),
    )
    .await
    .unwrap();

    assert!(!outcome.batches.is_empty(), "every in-range session is still visited");
    assert!(outcome.batches.iter().all(|b| b.skipped && b.bars == 0), "every batch was skipped");
    assert!(outcome.positions.is_empty(), "no engine cycle ran, so no positions");
}

/// The empty-batch skip must not swallow a **held** symbol's data gap. With one
/// symbol held and no other name in the batch, the session's batch is empty — the
/// same shape as the skip above, but with a frozen hold in flight — so the run fails
/// closed instead of skipping the session.
#[tokio::test]
async fn a_held_symbol_absent_from_an_empty_batch_aborts_instead_of_skipping_the_session() {
    let dir = tempdir().unwrap();
    // 005930 is the only rankable name, and it goes dark on session index 5.
    build_daily_fixture_with_gaps(
        dir.path(),
        &HashMap::new(),
        &HashMap::from([("005930.XKRX", BTreeSet::from([5usize]))]),
    )
    .await;

    let error = run_daily(
        cfg(dir.path(), 1),
        DecisionSink::new(),
        rank_only(&["005930.XKRX"]),
        always_enter(
            AlwaysEnterConfig { hold_sessions: 20, reenter: false, ..Default::default() },
            BarWitness::default(),
        ),
    )
    .await
    .expect_err("a held symbol's gap fails the run closed even when the batch is empty");

    let message = format!("{error:#}");
    assert!(
        message.contains("005930.XKRX") && message.contains("2024-01-09"),
        "the error names the held symbol and the session it had no bar on: {message}"
    );
}

/// The gate is scoped to **held** symbols. A name that is absent from a session while
/// holding no position has no frozen term in flight, so its gap is not the runner's
/// business and the run completes normally.
#[tokio::test]
async fn a_gap_in_a_symbol_that_holds_no_position_does_not_abort_the_run() {
    let dir = tempdir().unwrap();
    // 000660 is never ranked and therefore never held; it is missing on three
    // sessions that 005930 is held across.
    build_daily_fixture_with_gaps(
        dir.path(),
        &HashMap::new(),
        &HashMap::from([("000660.XKRX", BTreeSet::from([5usize, 6, 7]))]),
    )
    .await;

    let outcome = run_daily(
        cfg(dir.path(), 1),
        DecisionSink::new(),
        rank_only(&["005930.XKRX"]),
        always_enter(
            AlwaysEnterConfig { hold_sessions: 6, reenter: false, ..Default::default() },
            BarWitness::default(),
        ),
    )
    .await
    .expect("an unheld symbol's gap is not a data gap in any hold");

    let held = InstrumentId::from("005930.XKRX");
    assert_eq!(outcome.positions.len(), 1, "the held name still completes its round trip");
    assert!(outcome.positions[0].is_closed(), "and closes at hold expiry");
    assert!(
        outcome.batches.iter().all(|b| b.held.is_empty() || b.held == vec![held]),
        "000660 never held a position: {:?}",
        outcome.batches
    );
}

/// A value-divergent duplicate bar at the same `ts_event` mid-hold is deduped, the
/// drop is recorded, and the position still exits at exactly N + hold (R23). A
/// surviving duplicate would deliver two callbacks for one session, shortening the
/// hold and firing the stop check twice.
#[tokio::test]
async fn a_value_divergent_duplicate_bar_is_deduped_and_the_hold_is_unchanged() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    // Inject a second, value-divergent daily row for 005930 on session index 3.
    let catalog = dir.path().join("catalog");
    let bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
    let divergent = build_daily_bar(
        bt,
        &serde_json::from_value(daily_json(SESSION_DAYS[4], "100", "200", "100", "100", "7"))
            .unwrap(),
    )
    .unwrap()
    .unwrap();
    write_bars(&catalog, vec![divergent]).await.unwrap();

    let hold = 6;
    let witness = BarWitness::default();
    let outcome = run_daily(
        cfg(dir.path(), 1),
        DecisionSink::new(),
        rank_only(&["005930.XKRX"]),
        always_enter(
            AlwaysEnterConfig { hold_sessions: hold, reenter: false, ..Default::default() },
            witness.clone(),
        ),
    )
    .await
    .unwrap();

    // The strategy saw exactly ONE bar for the duplicated session — a surviving
    // duplicate would deliver two callbacks for one session, shortening the hold and
    // firing the stop check twice.
    let dup_ts = outcome.duplicate_drops.first().map(|d| d.ts_event);
    assert!(dup_ts.is_some(), "the duplicate was dropped: {:?}", outcome.duplicate_drops);
    assert_eq!(
        witness.snapshot().iter().filter(|(_, ts)| Some(*ts) == dup_ts).count(),
        1,
        "one callback for the duplicated session"
    );

    assert!(
        outcome.duplicate_drops.iter().any(|d| d.divergent
            && d.instrument_id == InstrumentId::from("005930.XKRX")),
        "the divergent duplicate was dropped AND recorded: {:?}",
        outcome.duplicate_drops
    );
    assert_eq!(outcome.positions.len(), 1);
    let p = &outcome.positions[0];
    let dates: Vec<NaiveDate> = outcome.selection.sessions.iter().map(|s| s.date).collect();
    let idx = |ns: u64| dates.iter().position(|d| *d == kst_date(ns)).unwrap();
    assert_eq!(
        idx(p.ts_closed.unwrap().as_u64()) - idx(p.ts_opened.as_u64()),
        hold,
        "the hold is exactly N + hold despite the duplicate"
    );
}

/// Two symbols entered on different sessions hold concurrently and close on
/// different sessions.
#[tokio::test]
async fn two_symbols_entered_on_different_sessions_hold_concurrently() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    let outcome = run_daily(
        cfg(dir.path(), 2),
        DecisionSink::new(),
        {
            // 005930 rankable from the first session; 000660 only from the fourth.
            let seen = std::sync::atomic::AtomicUsize::new(0);
            move |_c: &[UniverseCandidate]| {
                let n = seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n < 3 {
                    vec!["005930.XKRX".to_string()]
                } else {
                    vec!["005930.XKRX".to_string(), "000660.XKRX".to_string()]
                }
            }
        },
        always_enter(
            AlwaysEnterConfig { hold_sessions: 6, reenter: false, ..Default::default() },
            BarWitness::default(),
        ),
    )
    .await
    .unwrap();

    let closed: HashMap<InstrumentId, u64> = outcome
        .positions
        .iter()
        .filter(|p| p.is_closed())
        .map(|p| (p.instrument_id, p.ts_closed.unwrap().as_u64()))
        .collect();
    let a = InstrumentId::from("005930.XKRX");
    let b = InstrumentId::from("000660.XKRX");
    assert!(closed.contains_key(&a) && closed.contains_key(&b), "both closed: {closed:?}");
    assert_ne!(
        kst_date(closed[&a]),
        kst_date(closed[&b]),
        "the two positions close on different sessions"
    );
    // They overlapped: some session's held set carries both.
    assert!(
        outcome
            .batches
            .iter()
            .any(|s| s.held.contains(&a) && s.held.contains(&b)),
        "the two positions were concurrently held on at least one session"
    );
}

/// A run over a range with no daily bars returns an empty position set, with no
/// partial run written and no staging directory left behind.
#[tokio::test]
async fn a_range_with_no_daily_bars_returns_an_empty_position_set() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    let mut c = cfg(dir.path(), 8);
    c.range.start = "20250601".to_string();
    c.range.end = "20250630".to_string();
    let outcome = run_daily(
        c,
        DecisionSink::new(),
        rank_all,
        always_enter(AlwaysEnterConfig::default(), BarWitness::default()),
    )
    .await
    .unwrap();

    assert!(outcome.positions.is_empty(), "no in-range daily bars → no positions");
    assert!(outcome.selection.sessions.is_empty(), "no in-range session dates");
    assert!(outcome.batches.is_empty(), "no batches attempted");
    assert!(!dir.path().join("runs").exists(), "no registry residue");
    assert!(!dir.path().join("runs.staging").exists(), "no staging directory left behind");
}

/// Reading the cache once after `end()` yields the same count as summing the
/// distinct positions observed across the stream.
#[tokio::test]
async fn the_single_cache_read_matches_the_positions_observed_across_the_stream() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    let outcome = run_daily(
        cfg(dir.path(), 2),
        DecisionSink::new(),
        rank_all,
        always_enter(
            AlwaysEnterConfig { hold_sessions: 4, reenter: true, ..Default::default() },
            BarWitness::default(),
        ),
    )
    .await
    .unwrap();

    let observed: std::collections::BTreeSet<PositionId> =
        outcome.observed_position_ids.iter().copied().collect();
    assert_eq!(
        observed.len(),
        outcome.observed_position_ids.len(),
        "each position was observed opening exactly once"
    );
    let read: std::collections::BTreeSet<PositionId> =
        outcome.positions.iter().map(|p| p.id).collect();
    assert!(!read.is_empty(), "the fixture actually trades");
    assert_eq!(
        read, observed,
        "the single post-end() cache read holds exactly the positions the stream opened"
    );
}

// ---------------------------------------------------------------------------
// S. Selection-phase scenarios
// ---------------------------------------------------------------------------

/// The selection output — sequence and envelopes — is identical whether the engine
/// phase runs or is skipped: the selection pass has no engine dependency (KTD11).
#[tokio::test]
async fn selection_output_is_identical_with_and_without_the_engine_phase() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;

    let with_engine_sink = DecisionSink::new();
    let with_engine = run_daily(
        cfg(dir.path(), 2),
        with_engine_sink.clone(),
        rank_all,
        always_enter(AlwaysEnterConfig::default(), BarWitness::default()),
    )
    .await
    .unwrap();

    // The same selection, standalone — no engine anywhere in the call.
    let catalog = dir.path().join("catalog");
    let instruments = nautilus_ls::ingest::read_all_instruments(&catalog).await.unwrap();
    let all_bars = nautilus_ls::ingest::read_all_bars(&catalog).await.unwrap();
    let start_ns = nautilus_ls::ingest::kst_to_unix_nanos(
        NaiveDate::parse_from_str(RANGE_START, "%Y%m%d").unwrap(),
        chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
    )
    .unwrap()
    .as_u64();
    let end_ns = nautilus_ls::ingest::kst_to_unix_nanos(
        NaiveDate::parse_from_str(RANGE_END, "%Y%m%d").unwrap(),
        chrono::NaiveTime::from_hms_opt(23, 59, 59).unwrap(),
    )
    .unwrap()
    .as_u64();
    let bare_sink = DecisionSink::new();
    // The SAME assembly parameters the runner uses — `cfg.assembly_params()`, not
    // `OrbParams::default()`. The claim under test is engine-independence, so the
    // parameters have to be held constant; passing the raw default instead would compare
    // an ATR(14) selection against the runner's bridged ATR(1) one and fail for a reason
    // that has nothing to do with the engine.
    let bare = select_daily_sessions(
        &instruments,
        &all_bars,
        &cfg(dir.path(), 2).assembly_params(),
        &bare_sink,
        start_ns,
        end_ns,
        &rank_all,
    )
    .unwrap();

    assert_eq!(bare, with_engine.selection, "the selection is engine-independent");
    assert_eq!(
        bare.selection_sequence(),
        with_engine.selection.selection_sequence(),
        "the per-session selection sequence is identical"
    );
    let comparable = |envelopes: Vec<DecisionEnvelope>| -> Vec<(u64, String, Option<Decision>, Option<String>)> {
        envelopes
            .into_iter()
            .map(|e| {
                let d = e.decision_detail.expect("a universe envelope carries its detail");
                (e.ts_event, d.symbol, d.decision, d.filter)
            })
            .collect()
    };
    assert_eq!(
        comparable(bare_sink.snapshot()),
        comparable(with_engine_sink.snapshot()),
        "the emitted universe envelopes are identical"
    );
}

/// The session-open equity multiplier is exactly 1.0 on every session (KTD7).
#[tokio::test]
async fn the_equity_multiplier_is_exactly_one_on_every_session() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    let outcome = run_daily(
        cfg(dir.path(), 2),
        DecisionSink::new(),
        rank_all,
        always_enter(AlwaysEnterConfig::default(), BarWitness::default()),
    )
    .await
    .unwrap();

    assert!(!outcome.selection.sessions.is_empty());
    for s in &outcome.selection.sessions {
        assert_eq!(
            s.equity_multiplier, 1.0,
            "session {} carries a non-unit equity multiplier",
            s.date
        );
    }
    // And realized P&L accrued: the multiplier is fixed, not merely un-exercised.
    assert!(
        outcome.positions.iter().any(|p| p.is_closed()),
        "the fixture books realized P&L, so a compounding edge would have shown"
    );
}
