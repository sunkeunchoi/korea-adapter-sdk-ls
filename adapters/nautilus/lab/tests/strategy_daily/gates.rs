//! Focused unit tests — the gates that return before `self.order()`.
//!
//! The two fail-closed gates, the stop and sizing arms, and the per-session take
//! refusals all return before the order factory is ever touched, so each is driven
//! straight against `DataActor::on_bar` on an **unmounted** strategy: no engine, no
//! catalog. Everything that needs a *fill* lives in the root module's engine scenarios.

use std::collections::HashMap;

use chrono::NaiveDate;
use nautilus_common::actor::DataActor;
use nautilus_ls_lab::agent::sink::DecisionSink;
use nautilus_ls_lab::params_daily::DailyParams;
use nautilus_ls_lab::runner::backtest_daily::{
    DailyPathStrategy, DailySessionContext, MountedSymbol,
};
use nautilus_ls_lab::strategy::daily::{AdjustmentBasisShifts, DailyStrategy, EntryRefusal};
use nautilus_model::data::Bar;
use nautilus_model::identifiers::InstrumentId;

use crate::fixture::{daily_bar, daily_json, CODES, FIRST_IN_RANGE, SESSION_DAYS};
use crate::records::{placed, refusals, strategy_records, Rec};

/// Drive `on_bar` against an **unmounted** strategy. Legal exactly as far as the
/// gates: the concurrency, adjustment-basis, ATR, stop and sizing refusals all return
/// before the order factory is touched, and the per-session take refusals never touch
/// it at all.
fn drive_gate(
    params: DailyParams,
    shifts: AdjustmentBasisShifts,
    ctx: DailySessionContext,
    sessions: Vec<NaiveDate>,
    bar: &Bar,
) -> Vec<Rec> {
    let sink = DecisionSink::new();
    let mut strategy =
        DailyStrategy::new(Vec::<MountedSymbol>::new(), params, sink.clone(), shifts);
    let signals = strategy.session_signals();
    signals.publish_sessions(sessions);
    signals.publish_session(ctx);
    <DailyStrategy as DataActor>::on_bar(&mut strategy, bar).unwrap();
    strategy_records(&sink)
}

pub(crate) fn day(i: usize) -> NaiveDate {
    NaiveDate::parse_from_str(SESSION_DAYS[i], "%Y%m%d").unwrap()
}

fn ids(n: usize) -> Vec<InstrumentId> {
    (0..n)
        .map(|i| InstrumentId::from(format!("{}.XKRX", CODES[i]).as_str()))
        .collect()
}

/// **Scenario 1 (refusal half).** Twelve ranked candidates against a `target_m` of 8:
/// the four lowest-ranked carry a recorded `rank_beyond_entry_budget` refusal, and
/// nothing else does. The take itself is resolved by the runner, so this is the only
/// place the two non-take reasons are distinguishable.
#[test]
fn the_four_lowest_ranked_of_twelve_are_refused_with_a_recorded_reason() {
    let all = ids(12);
    let taken: Vec<InstrumentId> = all[..8].to_vec();
    let ctx = DailySessionContext {
        index: 0,
        date: day(FIRST_IN_RANGE),
        ranked: all.clone(),
        taken: taken.clone(),
        held: Vec::new(),
        // Deliberately empty: the ONE bar delivered below then refuses on the ATR
        // arm rather than reaching the order factory, which an unmounted strategy
        // has no access to.
        prior_atr: HashMap::new(),
    };
    let bar = daily_bar(all[0], daily_json(SESSION_DAYS[FIRST_IN_RANGE], 50_000, 50_500, 49_500, 50_000, 1_000));
    let recs = drive_gate(
        DailyParams::default(),
        AdjustmentBasisShifts::none(),
        ctx,
        vec![day(FIRST_IN_RANGE)],
        &bar,
    );

    let beyond = refusals(&recs, EntryRefusal::RankBeyondEntryBudget);
    let refused: Vec<&str> = beyond.iter().map(|r| r.symbol.as_str()).collect();
    let expected: Vec<String> = all[8..].iter().map(|i| i.to_string()).collect();
    assert_eq!(
        refused,
        expected.iter().map(String::as_str).collect::<Vec<_>>(),
        "exactly the four lowest-ranked are refused, in rank order: {recs:#?}"
    );
    for (n, rec) in beyond.iter().enumerate() {
        assert_eq!(rec.values["rank"], (8 + n) as f64, "the record carries the rank");
        assert_eq!(rec.values["target_m"], 8.0);
    }
    assert!(
        refusals(&recs, EntryRefusal::AlreadyHeld).is_empty(),
        "nothing is held this session, so no already-held refusal: {recs:#?}"
    );
}

/// **Scenario 2 (refusal half).** A symbol that ranks **first** but already holds a
/// position is refused on the `already_held` arm — a distinct reason from simply
/// falling outside the entry budget, so the two cannot be confused when counted.
#[test]
fn a_held_symbol_that_ranks_first_is_refused_as_already_held_not_as_out_of_budget() {
    let all = ids(3);
    let ctx = DailySessionContext {
        index: 1,
        date: day(FIRST_IN_RANGE + 1),
        ranked: all.clone(),
        taken: vec![all[1]],
        held: vec![all[0]],
        prior_atr: HashMap::new(),
    };
    let bar = daily_bar(all[1], daily_json(SESSION_DAYS[FIRST_IN_RANGE + 1], 50_100, 50_600, 49_600, 50_100, 1_000));
    let recs = drive_gate(
        DailyParams { target_m: 1, ..DailyParams::default() },
        AdjustmentBasisShifts::none(),
        ctx,
        vec![day(FIRST_IN_RANGE), day(FIRST_IN_RANGE + 1)],
        &bar,
    );

    let held = refusals(&recs, EntryRefusal::AlreadyHeld);
    assert_eq!(held.len(), 1, "one already-held refusal: {recs:#?}");
    assert_eq!(held[0].symbol, all[0].to_string(), "the rank-0 symbol, not the taken one");
    assert_eq!(held[0].values["rank"], 0.0, "it ranked FIRST and was still not takeable");

    let beyond = refusals(&recs, EntryRefusal::RankBeyondEntryBudget);
    assert_eq!(beyond.len(), 1, "the rank-2 symbol fell outside the budget: {recs:#?}");
    assert_eq!(beyond[0].symbol, all[2].to_string());
}

/// **Scenario 3 (record half).** No prior ATR at all → `atr_unavailable`, recorded on
/// the refusal path.
#[test]
fn a_candidate_with_no_prior_atr_is_refused_with_a_decision_record() {
    let all = ids(1);
    for prior in [None, Some(None)] {
        // An ABSENT key (not a candidate) and a `None` value (a candidate with no
        // derivable ATR) collapse onto the same fail-closed arm.
        let mut prior_atr: HashMap<InstrumentId, Option<f64>> = HashMap::new();
        if let Some(v) = prior {
            prior_atr.insert(all[0], v);
        }
        let ctx = DailySessionContext {
            index: 0,
            date: day(FIRST_IN_RANGE),
            ranked: all.clone(),
            taken: all.clone(),
            held: Vec::new(),
            prior_atr,
        };
        let bar = daily_bar(all[0], daily_json(SESSION_DAYS[FIRST_IN_RANGE], 50_000, 50_500, 49_500, 50_000, 1_000));
        let recs = drive_gate(
            DailyParams::default(),
            AdjustmentBasisShifts::none(),
            ctx,
            vec![day(FIRST_IN_RANGE)],
            &bar,
        );
        let refused = refusals(&recs, EntryRefusal::AtrUnavailable);
        assert_eq!(refused.len(), 1, "prior = {prior:?}: {recs:#?}");
        assert_eq!(refused[0].symbol, all[0].to_string());
        assert!(placed(&recs).is_empty(), "no entry was placed: {recs:#?}");
    }
}

/// **Scenario 4 (record half).** A prior ATR of exactly zero — the limit-locked
/// `O = H = L = C` session — is *available* and passes an `is_some` check, so it is
/// refused on its own `atr_non_positive` arm (KTD9).
#[test]
fn a_zero_prior_atr_is_refused_on_the_non_positive_arm_not_the_unavailable_one() {
    let all = ids(1);
    for atr in [0.0_f64, -1.0, f64::NAN] {
        let ctx = DailySessionContext {
            index: 0,
            date: day(FIRST_IN_RANGE),
            ranked: all.clone(),
            taken: all.clone(),
            held: Vec::new(),
            prior_atr: HashMap::from([(all[0], Some(atr))]),
        };
        // A limit-locked session prints O = H = L = C.
        let bar = daily_bar(all[0], daily_json(SESSION_DAYS[FIRST_IN_RANGE], 50_000, 50_000, 50_000, 50_000, 1_000));
        let recs = drive_gate(
            DailyParams::default(),
            AdjustmentBasisShifts::none(),
            ctx,
            vec![day(FIRST_IN_RANGE)],
            &bar,
        );
        assert_eq!(
            refusals(&recs, EntryRefusal::AtrNonPositive).len(),
            1,
            "atr = {atr}: refused as non-positive, NOT as unavailable: {recs:#?}"
        );
        assert!(
            refusals(&recs, EntryRefusal::AtrUnavailable).is_empty(),
            "atr = {atr} is available — it just is not usable: {recs:#?}"
        );
        assert!(placed(&recs).is_empty(), "no entry was placed: {recs:#?}");
    }
}

/// **Scenario 9 (record half).** A recorded adjustment-basis shift inside the
/// prospective hold window refuses the entry, and the refusal carries the window it
/// was measured over. A shift one session past the window does not.
#[test]
fn a_shift_inside_the_prospective_hold_window_is_refused_and_one_outside_is_not() {
    let all = ids(1);
    let sessions: Vec<NaiveDate> = (FIRST_IN_RANGE..FIRST_IN_RANGE + 6).map(day).collect();
    let params = DailyParams { holding_period_sessions: 3, ..DailyParams::default() };
    let bar = daily_bar(all[0], daily_json(SESSION_DAYS[FIRST_IN_RANGE], 50_000, 50_500, 49_500, 50_000, 1_000));
    // The shift gate runs BEFORE the ATR gate, so leaving the prior ATR unavailable
    // keeps the refusal short of the order factory (which an unmounted strategy has
    // no access to) without weakening either assertion: a shift inside the window
    // still refuses on its own arm, and one outside falls through to the ATR arm.
    let ctx = || DailySessionContext {
        index: 0,
        date: sessions[0],
        ranked: all.clone(),
        taken: all.clone(),
        held: Vec::new(),
        prior_atr: HashMap::new(),
    };

    // Session 0 + a hold of 3 → the window is [session 0, session 3], inclusive.
    let inside = AdjustmentBasisShifts::from_pairs([(all[0].to_string(), sessions[3])]);
    let recs = drive_gate(params.clone(), inside, ctx(), sessions.clone(), &bar);
    let refused = refusals(&recs, EntryRefusal::AdjustmentBasisShift);
    assert_eq!(refused.len(), 1, "the gate ran and RECORDED: {recs:#?}");
    assert_eq!(refused[0].values["hold_sessions"], 3.0);
    assert!(placed(&recs).is_empty(), "no entry was placed: {recs:#?}");

    // One session past the window: the hold cannot straddle it, so the entry passes
    // this gate. (It is refused later on sizing/order grounds we do not reach here —
    // what matters is that the shift arm did not fire.)
    let outside = AdjustmentBasisShifts::from_pairs([(all[0].to_string(), sessions[4])]);
    let recs = drive_gate(params, outside, ctx(), sessions, &bar);
    assert!(
        refusals(&recs, EntryRefusal::AdjustmentBasisShift).is_empty(),
        "a shift past the hold window does not refuse: {recs:#?}"
    );
    assert_eq!(
        refusals(&recs, EntryRefusal::AtrUnavailable).len(),
        1,
        "it fell through the shift gate to the next one: {recs:#?}"
    );
}

/// **The `non_positive_stop` arm.** A prior ATR large enough relative to the bar close
/// puts `entry − stop_atr_mult × ATR` at or below zero, and the entry is refused on its
/// own arm with the stop it computed recorded.
///
/// The defect this converts into a failure is an entry taken at a stop of zero or
/// below. Its `risk_per_share` is then at least the whole entry price, so the
/// entry-fixed risk capital is at least the position's entire notional, and that is what
/// lands in the denominator of the frozen verdict statistic
/// (`Σ realized_pnl / Σ risk_capital`) — flattering the run rather than collapsing it,
/// which is the failure mode a fail-closed gate exists to prevent. A stop *at* zero is
/// the same defect as one below it: a KRW price of zero is not a level any position can
/// be stopped out at.
///
/// `stop_atr_mult` is a frozen term — [`DailyParams::validate`] rejects any value but
/// 1.5 — so the arm is reached through the prior ATR and the bar's close, never by
/// widening the multiple.
#[test]
fn a_prior_atr_that_swallows_the_entry_price_is_refused_on_the_non_positive_stop_arm() {
    let all = ids(1);
    // Against a close of 45,000: an ATR of 30,000 lands the stop exactly ON zero, and
    // 40,000 lands it below. Both are finite and positive, so each gets PAST the
    // `atr_non_positive` arm — this is the stop that is unusable, not the ATR.
    for (atr, stop) in [(30_000.0_f64, 0.0_f64), (40_000.0, -15_000.0)] {
        let ctx = DailySessionContext {
            index: 0,
            date: day(FIRST_IN_RANGE),
            ranked: all.clone(),
            taken: all.clone(),
            held: Vec::new(),
            prior_atr: HashMap::from([(all[0], Some(atr))]),
        };
        let bar = daily_bar(all[0], daily_json(SESSION_DAYS[FIRST_IN_RANGE], 45_000, 45_500, 44_500, 45_000, 1_000));
        let recs = drive_gate(
            DailyParams::default(),
            AdjustmentBasisShifts::none(),
            ctx,
            vec![day(FIRST_IN_RANGE)],
            &bar,
        );
        let refused = refusals(&recs, EntryRefusal::NonPositiveStop);
        assert_eq!(refused.len(), 1, "atr = {atr}: the gate ran and RECORDED: {recs:#?}");
        assert_eq!(refused[0].symbol, all[0].to_string());
        assert_eq!(
            refused[0].values["stop"], stop,
            "the record carries the stop it refused on: {refused:#?}"
        );
        assert_eq!(refused[0].values["prior_atr"], atr);
        assert!(
            refusals(&recs, EntryRefusal::AtrNonPositive).is_empty(),
            "atr = {atr} is finite and positive — it is the STOP that is not: {recs:#?}"
        );
        assert!(placed(&recs).is_empty(), "no entry was placed: {recs:#?}");
    }
}

/// **The `zero_quantity` arm.** A bar close above the whole per-position notional
/// floors the integer share quantity to zero, and the entry is refused on its own arm.
///
/// The defect this converts into a failure is an entry submitted at a zero share
/// quantity. It fills nothing, so no position and no realized P&L ever appear — but the
/// strategy has already recorded an `OrderPlaced`, taken one of `max_concurrent`'s
/// slots for a leg that will never open, and carried a `risk_capital` of exactly zero
/// into the entry-risk ledger. The run then reads as one that took its entries and made
/// nothing on them, which is a hypothesis result, rather than as one that never sized a
/// share, which is a bug.
///
/// `notional_per_position` is the sizing term the freeze derives from the starting
/// balance and the frozen steady-state concurrency, and [`DailyParams::validate`] only
/// bounds it away from non-positive values — but moving it would size a capital
/// envelope the lineage was never judged under (R27), so the arm is reached through the
/// bar's close instead: one share priced above the whole budget buys none of itself.
#[test]
fn a_close_above_the_per_position_notional_is_refused_on_the_zero_quantity_arm() {
    let all = ids(1);
    let params = DailyParams::default();
    // 800,000 KRW a share against the default 781,250 KRW budget: floor(781250/800000)
    // is 0. Still on the masters' 100 KRW grid, so nothing here is off-grid.
    let close = 800_000_i64;
    assert_eq!(
        params.position_qty(close as f64),
        0,
        "the fixture price really does floor the quantity to zero"
    );
    let ctx = DailySessionContext {
        index: 0,
        date: day(FIRST_IN_RANGE),
        ranked: all.clone(),
        taken: all.clone(),
        held: Vec::new(),
        // Small enough that the stop (800,000 − 1.5 × 1,000 = 798,500) stays positive:
        // the sizing arm is reached only by getting past the stop arm.
        prior_atr: HashMap::from([(all[0], Some(1_000.0))]),
    };
    let bar = daily_bar(all[0], daily_json(SESSION_DAYS[FIRST_IN_RANGE], close, close + 500, close - 500, close, 1_000));
    let recs = drive_gate(
        params.clone(),
        AdjustmentBasisShifts::none(),
        ctx,
        vec![day(FIRST_IN_RANGE)],
        &bar,
    );

    let refused = refusals(&recs, EntryRefusal::ZeroQuantity);
    assert_eq!(refused.len(), 1, "the gate ran and RECORDED: {recs:#?}");
    assert_eq!(refused[0].symbol, all[0].to_string());
    assert_eq!(refused[0].values["entry_price"], close as f64);
    assert_eq!(
        refused[0].values["notional_per_position"], params.notional_per_position,
        "the record carries the budget the price outran: {refused:#?}"
    );
    assert!(
        refusals(&recs, EntryRefusal::NonPositiveStop).is_empty(),
        "the stop is 798,500 — positive, so the entry reached the SIZING arm: {recs:#?}"
    );
    assert!(placed(&recs).is_empty(), "no entry was placed: {recs:#?}");
}
