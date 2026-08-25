//! U4 — the daily-resolution, multi-session-hold strategy
//! ([`nautilus_ls_lab::strategy::daily::DailyStrategy`]).
//!
//! Offline: a fixture `ParquetDataCatalog` (wiremock-ingested instrument masters +
//! directly-written daily bars) driven through the streaming daily runner. No
//! credentials, no network beyond the wiremock masters.
//!
//! Each `lab/tests/*.rs` is its own binary and there is **no** shared test-support
//! module, so the catalog scaffold below is deliberately duplicated from
//! `backtest_daily_run.rs` rather than imported.
//!
//! # Two fixture facts that look exactly like logic bugs
//!
//! 1. The ingested KRX masters carry `price_increment = 100`. The matching engine
//!    **skips** an off-grid fill with a WARN rather than erroring, so a fixture price
//!    that is not a multiple of 100 silently trades nothing. Every price below is a
//!    multiple of 100 — except in Scenario 12, which uses an off-grid close *as* the
//!    lever that submits an entry order no fill ever answers.
//! 2. The shared candidate assembly reads its ATR window off `OrbParams`, whose
//!    default is 14 — that needs 15 prior daily sessions before a symbol has any
//!    derivable prior ATR, and the daily stop **fails closed** on an unavailable one
//!    (KTD9), so an unpinned window refuses every entry for a whole run. Every config
//!    here pins `params.atr_window` to `FROZEN_ATR_WINDOW_SESSIONS` (= 1), which is
//!    the window the frozen stop rule actually names. ATR(1) still needs *two* prior
//!    sessions, which is why [`SESSION_DAYS`] carries two pre-range sessions.
//!
//! # Where each scenario is driven
//!
//! The two fail-closed gates and the per-session take refusals return **before**
//! `self.order()` is ever touched, so they can be driven as focused unit tests
//! straight against `DataActor::on_bar` on an unmounted strategy — no engine, no
//! catalog. Everything that needs a *fill* (entry, stop exit, hold expiry, position
//! identity, concurrency) goes through a real `run_daily`.

// A test target's crate root resolves `mod` against `tests/`, so each child needs its
// path spelled out. The children stay in `tests/strategy_daily/`, which cargo does not
// treat as a test target, so this suite remains ONE test binary.
#[path = "strategy_daily/exits.rs"]
mod exits;
#[path = "strategy_daily/fixture.rs"]
mod fixture;
#[path = "strategy_daily/gates.rs"]
mod gates;
#[path = "strategy_daily/records.rs"]
mod records;

use std::collections::BTreeSet;

use nautilus_ls_lab::agent::envelope::SignalKind;
use nautilus_ls_lab::agent::sink::DecisionSink;
use nautilus_ls_lab::params_daily::DailyParams;
use nautilus_ls_lab::runner::backtest_daily::run_daily;
use nautilus_ls_lab::strategy::daily::{
    rank_by_placeholder_signal, AdjustmentBasisShifts, DailyStrategy, EntryRefusal,
};
use nautilus_ls_lab::strategy::orb::UniverseCandidate;
use nautilus_model::enums::{OrderSide, PositionSide};
use tempfile::tempdir;

use fixture::{
    build_fixture, cfg_range, descending_turnover, SymbolSpec, CODES, FIRST_IN_RANGE,
    SESSION_DAYS,
};
use gates::day;
use records::{approx, close_idx, placed, refusals, session_index, strategy_records};

// ---------------------------------------------------------------------------
// Engine scenarios — everything that needs a fill
//
// The exit-mechanics half of this group (hold expiry, the data-gap gate, the stop
// breach, the duplicate bar) lives in `strategy_daily/exits.rs`.
// ---------------------------------------------------------------------------

/// **Scenario 1.** Twelve candidates against the frozen `target_m` of 8 over a
/// single in-range session: exactly eight positions open and the four lowest-ranked
/// carry a recorded refusal.
#[tokio::test]
async fn twelve_candidates_at_target_m_eight_open_exactly_eight_positions() {
    let dir = tempdir().unwrap();
    let specs = descending_turnover(12);
    build_fixture(dir.path(), &specs).await;

    let sink = DecisionSink::new();
    let params = DailyParams::default(); // target_m 8, the frozen set
    let outcome = run_daily(
        cfg_range(dir.path(), FIRST_IN_RANGE, FIRST_IN_RANGE, params.target_m),
        sink.clone(),
        rank_by_placeholder_signal,
        DailyStrategy::factory(params, sink.clone(), AdjustmentBasisShifts::none()),
    )
    .await
    .unwrap();

    assert_eq!(outcome.selection.sessions.len(), 1, "one in-range session");
    assert_eq!(
        outcome.selection.sessions[0].ranked.len(),
        12,
        "all twelve are ranked — ranking is never a take"
    );
    assert_eq!(
        outcome.positions.len(),
        8,
        "exactly target_m positions open: {:?}",
        outcome.positions.iter().map(|p| p.instrument_id.to_string()).collect::<Vec<_>>()
    );
    let opened: Vec<String> =
        outcome.positions.iter().map(|p| p.instrument_id.to_string()).collect();
    for spec in &specs[..8] {
        assert!(opened.contains(&spec.id().to_string()), "{} entered", spec.code);
    }

    let recs = strategy_records(&sink);
    assert_eq!(placed(&recs).len(), 8, "one entry record per position");
    let beyond: Vec<&str> = refusals(&recs, EntryRefusal::RankBeyondEntryBudget)
        .iter()
        .map(|r| r.symbol.as_str())
        .collect();
    let expected: Vec<String> = specs[8..].iter().map(|s| s.id().to_string()).collect();
    assert_eq!(
        beyond,
        expected.iter().map(String::as_str).collect::<Vec<_>>(),
        "the four lowest-ranked are refused WITH a record: {recs:#?}"
    );
}

/// **Scenario 2.** The top-ranked symbol is already held on the next session, so it
/// is excluded from the take and the next name down takes its slot.
#[tokio::test]
async fn a_held_symbol_is_excluded_from_the_take_and_a_different_name_takes_its_slot() {
    let dir = tempdir().unwrap();
    let specs = descending_turnover(3);
    build_fixture(dir.path(), &specs).await;

    let sink = DecisionSink::new();
    // A hold of 16 over a 3-session range: nothing ever exits, so every session's
    // take is decided purely by the already-held exclusion.
    let params = DailyParams { target_m: 1, ..DailyParams::default() };
    let outcome = run_daily(
        cfg_range(dir.path(), FIRST_IN_RANGE, FIRST_IN_RANGE + 2, 1),
        sink.clone(),
        rank_by_placeholder_signal,
        DailyStrategy::factory(params, sink.clone(), AdjustmentBasisShifts::none()),
    )
    .await
    .unwrap();

    let (a, b, c) = (specs[0].id(), specs[1].id(), specs[2].id());
    assert_eq!(outcome.batches[0].taken, vec![a], "session 0 takes the top-ranked name");
    assert_eq!(outcome.batches[1].held, vec![a], "it is held at session 1's pre-batch step");
    assert_eq!(
        outcome.batches[1].taken,
        vec![b],
        "the SECOND name takes the freed slot even though {a} still ranks first"
    );
    assert_eq!(outcome.batches[2].taken, vec![c]);

    let opened: Vec<String> =
        outcome.positions.iter().map(|p| p.instrument_id.to_string()).collect();
    assert_eq!(opened.len(), 3, "one position per name: {opened:?}");
    for id in [a, b, c] {
        assert!(opened.contains(&id.to_string()), "{id} opened exactly once");
    }

    let recs = strategy_records(&sink);
    let held = refusals(&recs, EntryRefusal::AlreadyHeld);
    assert!(
        held.iter().any(|r| r.symbol == a.to_string() && r.date == day(FIRST_IN_RANGE + 1)),
        "the exclusion is RECORDED on session 1, not merely implied by the absent trade: \
         {recs:#?}"
    );
    assert!(
        held.iter()
            .filter(|r| r.symbol == a.to_string())
            .all(|r| r.values["rank"] == 0.0),
        "the excluded name ranked FIRST on every session it was excluded on: {held:#?}"
    );
}

/// **Scenario 3.** A symbol whose history starts inside the range has no derivable
/// prior ATR on its first candidate session: the entry is refused with a record and
/// **no position opens**.
#[tokio::test]
async fn no_position_opens_for_a_candidate_whose_prior_atr_is_unavailable() {
    let dir = tempdir().unwrap();
    // The only symbol in the catalog starts on the range's first session, so it is
    // not a candidate at all on session 0 and is a candidate with only ONE prior
    // session on session 1 — one short of what ATR(1) needs.
    let mut spec = SymbolSpec::new(CODES[0], 50_000, 1_000_000);
    spec.first_session = FIRST_IN_RANGE;
    build_fixture(dir.path(), std::slice::from_ref(&spec)).await;

    let sink = DecisionSink::new();
    let outcome = run_daily(
        cfg_range(dir.path(), FIRST_IN_RANGE, FIRST_IN_RANGE + 1, 1),
        sink.clone(),
        rank_by_placeholder_signal,
        DailyStrategy::factory(
            DailyParams { target_m: 1, ..DailyParams::default() },
            sink.clone(),
            AdjustmentBasisShifts::none(),
        ),
    )
    .await
    .unwrap();

    assert_eq!(
        outcome.selection.sessions[1].prior_atr.get(&spec.id().to_string()),
        Some(&None),
        "the selection phase derived NO prior ATR for session 1"
    );
    assert!(
        outcome.positions.is_empty(),
        "the fail-closed stop opened nothing: {:?}",
        outcome.positions.iter().map(|p| p.id).collect::<Vec<_>>()
    );
    let recs = strategy_records(&sink);
    let refused = refusals(&recs, EntryRefusal::AtrUnavailable);
    assert_eq!(refused.len(), 1, "the gate RECORDED its refusal: {recs:#?}");
    assert_eq!(refused[0].symbol, spec.id().to_string());
    assert!(placed(&recs).is_empty(), "no entry was ever placed: {recs:#?}");
}

/// **Scenario 4.** A limit-locked symbol (`O = H = L = C` every session) has an ATR
/// of exactly zero — available, and it would pass an `is_some` check. It is refused
/// on the same fail-closed path and nothing opens (KTD9).
#[tokio::test]
async fn no_position_opens_for_a_limit_locked_symbol_whose_atr_is_exactly_zero() {
    let dir = tempdir().unwrap();
    let mut spec = SymbolSpec::new(CODES[0], 50_000, 1_000_000);
    spec.locked = true;
    build_fixture(dir.path(), std::slice::from_ref(&spec)).await;

    let sink = DecisionSink::new();
    let outcome = run_daily(
        cfg_range(dir.path(), FIRST_IN_RANGE, FIRST_IN_RANGE + 3, 1),
        sink.clone(),
        rank_by_placeholder_signal,
        DailyStrategy::factory(
            DailyParams { target_m: 1, ..DailyParams::default() },
            sink.clone(),
            AdjustmentBasisShifts::none(),
        ),
    )
    .await
    .unwrap();

    assert_eq!(
        outcome.selection.sessions[0].prior_atr.get(&spec.id().to_string()),
        Some(&Some(0.0)),
        "the limit-locked series derives an ATR that is present and exactly zero"
    );
    assert!(outcome.positions.is_empty(), "nothing opened on a zero-ATR stop");
    let recs = strategy_records(&sink);
    assert_eq!(
        refusals(&recs, EntryRefusal::AtrNonPositive).len(),
        4,
        "every session recorded the refusal on the NON-POSITIVE arm: {recs:#?}"
    );
    assert!(refusals(&recs, EntryRefusal::AtrUnavailable).is_empty());
    assert!(placed(&recs).is_empty());
}

/// **Scenario 8.** No short position is ever opened — including under an inverted
/// ranking signal — and an exit closes its **own** position rather than minting a
/// second, opposite-side one.
///
/// This is the KTD12 Hedging trap: under `OmsType::Hedging` an exit submitted without
/// a position id mints a fresh short instead of closing the long, and the account type
/// does not reject it. A regression there would double the position count, flip
/// `Position.entry` to `Sell`, and open a second position on the exit session's
/// timestamp — all three are asserted here (and the strategy's own
/// `on_position_opened` assertion would abort the run first).
#[tokio::test]
async fn no_short_is_ever_opened_and_an_exit_closes_its_own_position() {
    let dir = tempdir().unwrap();
    let specs = descending_turnover(4);
    build_fixture(dir.path(), &specs).await;

    let sink = DecisionSink::new();
    let params = DailyParams {
        holding_period_sessions: 2,
        target_m: 1,
        max_concurrent: 8,
        ..DailyParams::default()
    };
    // The ranking signal INVERTED: lowest prior turnover first.
    let inverted = |candidates: &[UniverseCandidate]| {
        let mut ranked = rank_by_placeholder_signal(candidates);
        ranked.reverse();
        ranked
    };
    let outcome = run_daily(
        cfg_range(dir.path(), FIRST_IN_RANGE, FIRST_IN_RANGE + 11, 1),
        sink.clone(),
        inverted,
        DailyStrategy::factory(params, sink.clone(), AdjustmentBasisShifts::none()),
    )
    .await
    .unwrap();

    assert_eq!(
        outcome.batches[0].taken,
        vec![specs[3].id()],
        "the inverted signal really took the LOWEST-turnover name first"
    );
    assert!(!outcome.positions.is_empty(), "the fixture traded");
    for p in &outcome.positions {
        assert_eq!(
            p.entry,
            OrderSide::Buy,
            "the daily path is long only (frozen directionality): {} entered {:?}",
            p.instrument_id,
            p.entry
        );
        assert_ne!(p.side, PositionSide::Short, "no short leg exists: {p:?}");
    }

    // One position per entry: a KTD12 regression would mint an EXTRA position per
    // exit rather than closing the long.
    let recs = strategy_records(&sink);
    assert_eq!(
        outcome.positions.len(),
        placed(&recs).len(),
        "one position per entry order, no phantom opposite-side legs: placed {:?}, positions {:?}",
        placed(&recs).iter().map(|r| (&r.symbol, r.date)).collect::<Vec<_>>(),
        outcome
            .positions
            .iter()
            .map(|p| (p.instrument_id.to_string(), p.entry, p.ts_opened))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        outcome.observed_position_ids.len(),
        outcome.positions.len(),
        "the stream observed exactly the positions the single cache read reports"
    );
    let closed: Vec<&nautilus_model::position::Position> =
        outcome.positions.iter().filter(|p| p.is_closed()).collect();
    assert!(!closed.is_empty(), "at least one exit fired");
    for p in &closed {
        assert_eq!(p.signed_qty, 0.0, "the exit FLATTENED its own leg: {p:?}");
        assert_eq!(p.side, PositionSide::Flat);
        // Nothing else opened on this symbol at the moment this one closed.
        let phantom = outcome
            .positions
            .iter()
            .filter(|q| q.instrument_id == p.instrument_id && q.ts_opened == p.ts_closed.unwrap())
            .count();
        assert_eq!(phantom, 0, "an exit must not open a position of its own: {p:?}");
    }
}

/// **Scenario 9.** A symbol carrying a recorded adjustment-basis shift inside its
/// prospective hold window is refused **with the reason recorded**, while an
/// unaffected name on the same session trades normally. Asserted by the PRESENCE of
/// the record, not only by the absence of the trade.
#[tokio::test]
async fn a_symbol_with_a_shift_inside_the_hold_window_is_refused_with_the_reason_recorded() {
    let dir = tempdir().unwrap();
    let specs = descending_turnover(2);
    build_fixture(dir.path(), &specs).await;

    let shifted = specs[0].id(); // ranks FIRST, and is still refused
    let clean = specs[1].id();
    let last = FIRST_IN_RANGE + 3;
    let shifts =
        AdjustmentBasisShifts::from_pairs([(shifted.to_string(), day(last))]);

    let sink = DecisionSink::new();
    let params = DailyParams {
        holding_period_sessions: 3,
        target_m: 2,
        max_concurrent: 8,
        ..DailyParams::default()
    };
    let outcome = run_daily(
        cfg_range(dir.path(), FIRST_IN_RANGE, last, 2),
        sink.clone(),
        rank_by_placeholder_signal,
        DailyStrategy::factory(params, sink.clone(), shifts),
    )
    .await
    .unwrap();

    assert!(
        outcome.batches.iter().all(|b| b.taken.contains(&shifted) || b.held.contains(&shifted)),
        "the shifted name WAS taken by the runner every session — the refusal is the \
         strategy's fail-closed gate, not a selection artefact: {:?}",
        outcome.batches
    );
    let recs = strategy_records(&sink);
    let refused = refusals(&recs, EntryRefusal::AdjustmentBasisShift);
    assert_eq!(
        refused.len(),
        outcome.selection.sessions.len(),
        "every session recorded the shift refusal: {recs:#?}"
    );
    assert!(refused.iter().all(|r| r.symbol == shifted.to_string()));
    assert!(
        refused.iter().all(|r| {
            r.values["window_start_ordinal"] <= r.values["shift_ordinal"]
                && r.values["shift_ordinal"] <= r.values["window_end_ordinal"]
        }),
        "the record carries the window the shift straddled: {refused:#?}"
    );

    assert!(
        outcome.positions.iter().all(|p| p.instrument_id == clean),
        "only the unaffected name holds a position: {:?}",
        outcome.positions.iter().map(|p| p.instrument_id.to_string()).collect::<Vec<_>>()
    );
    assert!(!outcome.positions.is_empty(), "the clean name did trade — the fixture is live");
}

/// **Scenario 10.** Risk capital is `quantity × (entry − stop)` at entry and is the
/// same number when read at exit: it is fixed at the open and never re-derived from a
/// later bar (R12). `joined_risk` returns `(None, None)` on a non-positive
/// `risk_per_share`, which would collapse `return_on_risk` for the whole run.
#[tokio::test]
async fn risk_capital_is_entry_fixed_and_unchanged_at_exit() {
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
        cfg_range(dir.path(), FIRST_IN_RANGE, FIRST_IN_RANGE + 5, 1),
        sink.clone(),
        rank_by_placeholder_signal,
        DailyStrategy::factory(params, sink.clone(), AdjustmentBasisShifts::none()),
    )
    .await
    .unwrap();

    let recs = strategy_records(&sink);
    let entry = placed(&recs).first().copied().expect("an entry record");
    let qty = entry.values["qty"];
    let risk_capital = entry.values["risk_capital"];
    assert!(qty > 0.0 && risk_capital > 0.0, "a real, positive risk capital: {entry:#?}");
    assert!(
        approx(risk_capital, qty * (entry.values["entry_price"] - entry.values["stop"])),
        "at entry, risk capital = quantity × (entry − stop): {entry:#?}"
    );
    assert!(
        approx(entry.values["risk_per_share"], entry.values["entry_price"] - entry.values["stop"])
    );

    let exit = recs
        .iter()
        .find(|r| matches!(r.kind, SignalKind::TimeExit))
        .unwrap_or_else(|| panic!("a hold-expiry exit record: {recs:#?}"));
    assert!(approx(exit.values["qty"], qty), "the quantity is unchanged at exit: {exit:#?}");
    assert!(
        approx(exit.values["qty"] * (exit.values["entry_price"] - exit.values["stop"]), risk_capital),
        "read at EXIT, quantity × (entry − stop) is the same risk capital: {exit:#?}"
    );

    // The same number as the ledger the performance report divides by.
    let first = outcome
        .positions
        .iter()
        .min_by_key(|p| p.ts_opened.as_u64())
        .expect("a position");
    let idx = outcome.positions.iter().position(|p| p.id == first.id).unwrap();
    let risk = outcome.entry_risks[idx].expect("the entry risk projected onto the position");
    assert!(approx(risk.risk_per_share * risk.qty, risk_capital), "{risk:?} vs {risk_capital}");
    // `quantity` is 0 once the leg is flat — `peak_qty` is the filled entry size.
    assert!(approx(risk.qty, first.peak_qty.as_f64()), "the recorded qty is the filled qty");
    assert!(
        approx(exit.values["entry_price"], first.avg_px_open),
        "the stop is fixed off the REALIZED fill, not the assumed close: {exit:#?}"
    );
}

/// **Scenario 11.** Concurrency reaches `target_m × hold` and does not exceed it, at
/// a scaled setting (`target_m` 2 × a hold of 3 = 6) — the frozen 8 × 16 = 128 would
/// need 128 distinct instruments.
///
/// The concurrency cap is deliberately set **non-binding**: on this path the cap is an
/// assertion, not a second selection rule, so the steady state asserted here has to be
/// the take-and-hold arithmetic's own, not the cap's. That the cap never bound is
/// asserted separately, by the absence of a `concurrency_cap` refusal.
///
/// **Measured, and load-bearing for the cap's default.** Setting the cap to
/// `target_m × hold` — which is exactly `DailyParams::default().max_concurrent` — makes
/// it bind *transiently* and drops the run below its own steady state: this fixture
/// then oscillates `[2, 4, 6, 5, 4, 4, 5, 6, …]` instead of holding at 6. The cause is
/// intra-session ordering. At session `s` the pre-batch held set is `target_m × hold`
/// (the expiring cohort has not exited yet), the runner takes `target_m` more, and the
/// batch is delivered in instrument-id order — so an entry whose symbol sorts before
/// the expiring legs sees `open + pending = target_m × (hold + 1)` and is refused with
/// `concurrency_cap`, whose own doc says "a refusal here means the take over-issued".
/// At the frozen 8 × 16 the same arithmetic reaches 136 against a cap of 128.
#[tokio::test]
async fn concurrency_reaches_target_m_times_hold_and_does_not_exceed_it() {
    let dir = tempdir().unwrap();
    let specs = descending_turnover(8);
    build_fixture(dir.path(), &specs).await;

    let (target_m, hold) = (2usize, 3usize);
    let sink = DecisionSink::new();
    let params = DailyParams {
        holding_period_sessions: hold,
        target_m,
        max_concurrent: 64, // non-binding on purpose — see the doc comment
        ..DailyParams::default()
    };
    let outcome = run_daily(
        cfg_range(dir.path(), FIRST_IN_RANGE, SESSION_DAYS.len() - 1, target_m),
        sink.clone(),
        rank_by_placeholder_signal,
        DailyStrategy::factory(params, sink.clone(), AdjustmentBasisShifts::none()),
    )
    .await
    .unwrap();

    let sessions = outcome.selection.sessions.len();
    let open_at_close_of: Vec<usize> = (0..sessions)
        .map(|s| {
            outcome
                .positions
                .iter()
                .filter(|p| {
                    session_index(&outcome, p.ts_opened.as_u64()) <= s
                        && close_idx(&outcome, p).is_none_or(|c| c > s)
                })
                .count()
        })
        .collect();

    let steady = target_m * hold;
    assert_eq!(
        open_at_close_of.iter().copied().max(),
        Some(steady),
        "concurrency reaches target_m × hold = {steady}: {open_at_close_of:?}"
    );
    assert!(
        open_at_close_of.iter().all(|n| *n <= steady),
        "and never exceeds it: {open_at_close_of:?}"
    );
    assert!(
        open_at_close_of[steady..].iter().all(|n| *n == steady),
        "the steady state holds once the ramp-up completes: {open_at_close_of:?}"
    );

    let recs = strategy_records(&sink);
    assert!(
        refusals(&recs, EntryRefusal::ConcurrencyCap).is_empty(),
        "the cap never bound, so {steady} is the take-and-hold arithmetic's own steady \
         state and not the cap's: {recs:#?}"
    );
}

/// **Scenario 12 (the stale-pending regression).** An entry order that is submitted and
/// never opens a position must not permanently consume one of `max_concurrent` slots.
///
/// The defect this converts into a failure: `evaluate_entry` counts
/// `open.len() + pending_leg.len()` against the cap, and `pending_leg` was only ever
/// cleared by the position callbacks — so an order that was submitted and never opened
/// held its slot for the **rest of the run** and made its symbol permanently
/// un-re-enterable, because `on_bar` returns early on an in-flight id. Neither shows up
/// as an error: the run finalizes green having quietly traded a smaller book than the
/// one it is judged as. The fix sweeps the pendings at session rollover, justified by
/// the module doc's fill mechanic — an order submitted inside `on_bar` is drained and
/// settled at the **same** bar's `ts_init`, so anything still in flight when a new
/// session ordinal arrives never opened and never will.
///
/// **The lever is the off-grid price** documented in this module's fixture facts: the
/// top-ranked name's whole series sits at `50,050 + i × 100`, off the masters' 100 KRW
/// grid, so the matching engine skips the fill with a WARN. The order really is
/// submitted — there is a recorded `OrderPlaced` and the run's own
/// `unopened_entry_orders` diagnostic carries its client order id — and no position ever
/// opens. (The alternative lever, a starting balance too small for the notional, denies
/// the order at the *risk engine* instead — a rejection rather than a silent non-fill,
/// and account-wide, so it cannot be aimed at one of the two symbols.)
///
/// `max_concurrent` is 1, so the leak is unambiguous: the off-grid name goes dark after
/// the first session, and the second name — takeable on the second session and priced on
/// the grid — MUST open a position. Before the fix its entry saw
/// `open + pending = 0 + 1` against that cap of 1 and was refused with
/// `concurrency_cap`, against a slot held by an order that had already failed to fill.
#[tokio::test]
async fn a_submitted_entry_that_never_opens_does_not_hold_a_concurrency_slot() {
    let dir = tempdir().unwrap();
    // The off-grid name outranks the on-grid one (turnover is prior close × prior
    // volume), so session 0's single take is the entry that will never fill. It then
    // contributes no bar to session 1 — legal, because it never opened a position, so
    // the held-symbol data-gap gate has nothing in flight to protect.
    let mut never_fills = SymbolSpec::new(CODES[0], 50_050, 900_000);
    never_fills.gaps = BTreeSet::from([FIRST_IN_RANGE + 1]);
    let on_grid = SymbolSpec::new(CODES[1], 50_000, 100_000);
    let specs = vec![never_fills, on_grid];
    build_fixture(dir.path(), &specs).await;

    let sink = DecisionSink::new();
    let params = DailyParams { target_m: 1, max_concurrent: 1, ..DailyParams::default() };
    let outcome = run_daily(
        cfg_range(dir.path(), FIRST_IN_RANGE, FIRST_IN_RANGE + 1, 1),
        sink.clone(),
        rank_by_placeholder_signal,
        DailyStrategy::factory(params, sink.clone(), AdjustmentBasisShifts::none()),
    )
    .await
    .unwrap();

    let (stale, later) = (specs[0].id(), specs[1].id());
    assert_eq!(outcome.batches[0].taken, vec![stale], "session 0 took the off-grid name");
    assert_eq!(outcome.batches[1].taken, vec![later], "session 1 took the on-grid one");

    // The order really was submitted: the strategy recorded it, and the run's own
    // unopened-entry diagnostic carries exactly one client order id that never opened.
    let recs = strategy_records(&sink);
    let entries: Vec<&str> = placed(&recs).iter().map(|r| r.symbol.as_str()).collect();
    assert_eq!(
        entries,
        vec![stale.to_string().as_str(), later.to_string().as_str()],
        "both entries were SUBMITTED — the first one simply never filled: {recs:#?}"
    );
    assert_eq!(
        outcome.unopened_entry_orders.len(),
        1,
        "exactly one submitted entry never opened a position: {:?}",
        outcome.unopened_entry_orders
    );

    // And the slot it never opened into was released, so the later name really entered.
    let opened: Vec<String> =
        outcome.positions.iter().map(|p| p.instrument_id.to_string()).collect();
    assert_eq!(
        opened,
        vec![later.to_string()],
        "the on-grid name opened the run's only position: {opened:?}"
    );
    assert_eq!(
        session_index(&outcome, outcome.positions[0].ts_opened.as_u64()),
        1,
        "it entered on session 1, the session after the unfilled order was submitted"
    );
    assert!(
        refusals(&recs, EntryRefusal::ConcurrencyCap).is_empty(),
        "a cap of 1 was never reached: the unfilled order does not hold a slot past its \
         own session: {recs:#?}"
    );
}
