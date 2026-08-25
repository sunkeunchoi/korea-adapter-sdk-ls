//! R. Entry-risk capture and the index-aligned join (U2 — KTD3, R12).
//!
//! The end-to-end scenario, the three mutation tests that pin one seam assertion of the
//! projection each, and the two resolution cases (a rejected entry, an unrecorded one).
//! Split out of the crate root as one self-contained group: nothing here is used by the
//! engine-phase or selection-phase scenarios.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use nautilus_ls_lab::agent::sink::DecisionSink;
use nautilus_ls_lab::artifacts::performance::{
    ClientOrderEntryRiskLedger, EntryRisk, PerformanceReport,
};
use nautilus_ls_lab::runner::backtest_daily::{run_daily, DailyRunOutcome, EntryRiskProjection};
use nautilus_model::identifiers::{ClientOrderId, InstrumentId};
use tempfile::tempdir;

use crate::always_enter::{always_enter_sharing, AlwaysEnterConfig};
use crate::fixture::{build_daily_fixture, cfg, rank_all, rank_only};

const STARTING_BALANCE: f64 = 100_000_000.0;

/// Rebuild the projection the runner built, in the outcome's cache-read order. The
/// runner already asserted this one; the mutation tests below break a **copy** of it
/// and check that the matching seam assertion fires.
fn projection_of(outcome: &DailyRunOutcome) -> EntryRiskProjection {
    let slots: Vec<Option<(ClientOrderId, EntryRisk)>> = outcome
        .positions
        .iter()
        .zip(outcome.entry_risks.iter())
        .map(|(p, r)| r.map(|r| (p.opening_order_id, r)))
        .collect();
    let opened = slots.iter().filter(|s| s.is_some()).count();
    EntryRiskProjection::from_parts(slots, opened, outcome.unopened_entry_orders.clone())
}

/// Serializes the panic-hook swap in [`assertion_message`]: `set_hook` is process
/// global and these tests run concurrently, so an unguarded swap could leave the
/// silencing hook installed and hide a *real* failure elsewhere in the binary.
static PANIC_HOOK_LOCK: Mutex<()> = Mutex::new(());

/// Run `assert_aligned` on a deliberately broken projection and return the panic
/// message. The panic hook is silenced for the duration so a *passing* mutation test
/// does not print a scary backtrace.
fn assertion_message(projection: &EntryRiskProjection, positions: &[nautilus_model::position::Position]) -> String {
    let _serialized = PANIC_HOOK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prior = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        projection.assert_aligned(positions);
    }))
    .expect_err("the broken projection must trip a seam assertion");
    std::panic::set_hook(prior);
    err.downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| (*s).to_string()))
        .unwrap_or_else(|| "<non-string panic payload>".to_string())
}

/// A re-entry run: one symbol entered, exited, and entered again, with `hold_sessions`
/// short enough to fit two round trips into the 21-session fixture.
async fn reentry_run(dir: &Path, ledger: ClientOrderEntryRiskLedger) -> DailyRunOutcome {
    build_daily_fixture(dir, &HashMap::new()).await;
    run_daily(
        cfg(dir, 1),
        DecisionSink::new(),
        rank_only(&["005930.XKRX"]),
        always_enter_sharing(
            AlwaysEnterConfig { hold_sessions: 5, reenter: true, ..Default::default() },
            ledger,
        ),
    )
    .await
    .unwrap()
}

/// **The end-to-end scenario.** A symbol entered, exited, and re-entered over the
/// range appears as two distinct trade records with two distinct risk values — the
/// defect a symbol-keyed ledger (ORB's) would collapse into one.
#[tokio::test]
async fn a_reentered_symbol_yields_two_trade_records_with_two_distinct_risk_values() {
    let dir = tempdir().unwrap();
    let outcome = reentry_run(dir.path(), ClientOrderEntryRiskLedger::new()).await;

    let report = PerformanceReport::from_positions_with_risk(
        &outcome.positions,
        &outcome.entry_risks,
        STARTING_BALANCE,
        None,
    );
    let trades: Vec<&nautilus_ls_lab::artifacts::performance::TradeRecord> =
        report.trades.iter().filter(|t| t.symbol == "005930.XKRX").collect();
    assert!(
        trades.len() >= 2,
        "the symbol was entered, exited, and re-entered: {:?}",
        report.trades.iter().map(|t| (&t.symbol, t.ts_opened, t.ts_closed)).collect::<Vec<_>>()
    );

    // U2's Verification: EVERY position in an end-to-end daily run carries risk.
    assert!(
        report.trades.iter().all(|t| t.risk_capital.is_some()),
        "every trade carries a non-None risk_capital: {:?}",
        report.trades.iter().map(|t| (&t.symbol, t.risk_capital)).collect::<Vec<_>>()
    );
    let caps: Vec<u64> = trades.iter().map(|t| t.risk_capital.unwrap().to_bits()).collect();
    let distinct: std::collections::BTreeSet<u64> = caps.iter().copied().collect();
    assert_eq!(
        distinct.len(),
        caps.len(),
        "the two round trips carry TWO distinct risk values — a symbol-keyed ledger would \
         collapse them onto one: {:?}",
        trades.iter().map(|t| t.risk_capital).collect::<Vec<_>>()
    );
}

/// The risk capital recorded at entry is unchanged at exit after a full hold of price
/// movement: it is entry-fixed, never re-derived from the exit bar.
#[tokio::test]
async fn risk_capital_recorded_at_entry_is_unchanged_at_exit_after_a_full_hold() {
    let dir = tempdir().unwrap();
    let ledger = ClientOrderEntryRiskLedger::new();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    let cfg_ae = AlwaysEnterConfig { hold_sessions: 6, reenter: false, ..Default::default() };
    let outcome = run_daily(
        cfg(dir.path(), 1),
        DecisionSink::new(),
        rank_only(&["005930.XKRX"]),
        always_enter_sharing(cfg_ae.clone(), ledger.clone()),
    )
    .await
    .unwrap();

    assert_eq!(outcome.positions.len(), 1);
    let recorded = ledger
        .get(&outcome.positions[0].opening_order_id)
        .expect("the entry order's risk was recorded at submit");
    assert_eq!(recorded.risk_per_share, cfg_ae.risk_base);
    assert_eq!(recorded.qty, cfg_ae.qty as f64);

    let report = PerformanceReport::from_positions_with_risk(
        &outcome.positions,
        &outcome.entry_risks,
        STARTING_BALANCE,
        None,
    );
    let trade = &report.trades[0];
    assert!(trade.ts_closed.is_some(), "the position closed at hold expiry");
    assert_ne!(
        trade.avg_px_close.unwrap(),
        trade.avg_px_open,
        "the fixture drifts, so the price genuinely moved over the hold"
    );
    assert_eq!(
        trade.risk_capital,
        Some(recorded.qty * recorded.risk_per_share),
        "risk_capital is the ENTRY-fixed qty · risk_per_share, untouched by the exit"
    );
}

/// Ordering: with deliberately **distinct** per-entry risk values, each position
/// carries its own. A uniform-value fixture would hide a permutation entirely, so
/// this test also asserts the fixture is non-uniform.
#[tokio::test]
async fn each_position_carries_its_own_entry_risk_under_distinct_per_entry_values() {
    let dir = tempdir().unwrap();
    let ledger = ClientOrderEntryRiskLedger::new();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    // Both symbols, re-entry on: several positions, several entries, one per order.
    let outcome = run_daily(
        cfg(dir.path(), 2),
        DecisionSink::new(),
        rank_all,
        always_enter_sharing(
            AlwaysEnterConfig { hold_sessions: 4, reenter: true, ..Default::default() },
            ledger.clone(),
        ),
    )
    .await
    .unwrap();

    assert!(outcome.positions.len() >= 4, "the fixture opens several positions");
    assert_eq!(outcome.entry_risks.len(), outcome.positions.len());
    for (i, p) in outcome.positions.iter().enumerate() {
        assert_eq!(
            outcome.entry_risks[i],
            ledger.get(&p.opening_order_id),
            "position {i} ({}, opened by {}) must carry the risk recorded for ITS OWN entry \
             order, not another position's",
            p.id,
            p.opening_order_id
        );
    }
    assert!(
        outcome.entry_risks.iter().all(|r| r.is_some()),
        "every position in an end-to-end daily run carries a non-None entry risk"
    );

    // The fixture is deliberately non-uniform: a uniform one would pass a permuted
    // projection too, so this assertion is what gives the check above its power.
    let values: Vec<u64> =
        outcome.entry_risks.iter().flatten().map(|r| r.risk_per_share.to_bits()).collect();
    let distinct: std::collections::BTreeSet<u64> = values.iter().copied().collect();
    assert_eq!(
        distinct.len(),
        values.len(),
        "the per-entry risk values are DISTINCT — a uniform fixture would hide a mis-ordered \
         projection: {:?}",
        outcome.entry_risks
    );
}

/// **Mutation 1 — truncation.** A deliberately shortened risk slice trips the length
/// assertion rather than silently producing risk-less trailing trades (which would
/// set `all_have_risk = false` and collapse `return_on_risk` to `None`).
#[tokio::test]
async fn a_shortened_risk_slice_trips_the_length_assertion() {
    let dir = tempdir().unwrap();
    let outcome = reentry_run(dir.path(), ClientOrderEntryRiskLedger::new()).await;
    let good = projection_of(&outcome);
    assert!(outcome.positions.len() >= 2);

    let mut slots = good.slots().to_vec();
    slots.pop();
    let broken = EntryRiskProjection::from_parts(slots, good.opened_entries(), Vec::new());
    let msg = assertion_message(&broken, &outcome.positions);
    assert!(msg.contains("KTD3 assertion 1"), "assertion 1 must fire, got: {msg}");
    assert!(msg.contains("collapses return_on_risk"), "the message names the statistic: {msg}");

    // And the unbroken projection passes.
    good.assert_aligned(&outcome.positions);
}

/// **Mutation 2 — collapse.** A ledger collapsed onto one entry per symbol (ORB's
/// key) trips the count assertion even though the length assertion passes and every
/// remaining slot sits on the right position.
#[tokio::test]
async fn a_collapsed_ledger_trips_the_count_assertion() {
    let dir = tempdir().unwrap();
    let outcome = reentry_run(dir.path(), ClientOrderEntryRiskLedger::new()).await;
    let good = projection_of(&outcome);
    assert!(
        outcome.positions.len() >= 2
            && outcome.positions[0].instrument_id == outcome.positions[1].instrument_id,
        "the fixture holds several positions on ONE symbol — the collapse this catches"
    );

    // One risk per SYMBOL, as an instrument-keyed join would produce.
    let mut seen: std::collections::BTreeSet<InstrumentId> = Default::default();
    let slots: Vec<Option<(ClientOrderId, EntryRisk)>> = outcome
        .positions
        .iter()
        .zip(good.slots())
        .map(|(p, s)| if seen.insert(p.instrument_id) { *s } else { None })
        .collect();
    assert_eq!(slots.len(), outcome.positions.len(), "the length assertion would still pass");

    let broken = EntryRiskProjection::from_parts(slots, good.opened_entries(), Vec::new());
    let msg = assertion_message(&broken, &outcome.positions);
    assert!(msg.contains("KTD3 assertion 2"), "assertion 2 must fire, got: {msg}");
    assert!(msg.contains("Σ risk_capital"), "the message names the statistic: {msg}");

    good.assert_aligned(&outcome.positions);
}

/// **Mutation 3 — permutation.** A projection rotated out of cache-read order (what
/// building it in *ledger* order produces) has the right length and every slot
/// filled, so assertions 1 and 2 both pass; only the opening-order-id check catches
/// it.
#[tokio::test]
async fn a_permuted_projection_trips_the_opening_order_id_assertion() {
    let dir = tempdir().unwrap();
    let outcome = reentry_run(dir.path(), ClientOrderEntryRiskLedger::new()).await;
    let good = projection_of(&outcome);
    assert!(outcome.positions.len() >= 2);
    assert!(good.slots().iter().all(|s| s.is_some()), "every slot is filled before permuting");

    let mut slots = good.slots().to_vec();
    slots.rotate_left(1);
    assert_eq!(slots.len(), outcome.positions.len(), "assertion 1 still passes");
    assert_eq!(
        slots.iter().filter(|s| s.is_some()).count(),
        good.opened_entries(),
        "assertion 2 still passes — a permutation leaves Σ risk_capital invariant"
    );

    let broken = EntryRiskProjection::from_parts(slots, good.opened_entries(), Vec::new());
    let msg = assertion_message(&broken, &outcome.positions);
    assert!(msg.contains("KTD3 assertion 3"), "assertion 3 must fire, got: {msg}");
    assert!(msg.contains("realized_r"), "the message names the corrupted statistic: {msg}");

    good.assert_aligned(&outcome.positions);
}

/// A recorded entry whose order the venue/risk engine rejected produces a **named
/// run-level diagnostic**, not an aborted run: the count assertion is defined over
/// the entries that actually opened a position.
#[tokio::test]
async fn a_rejected_entry_order_produces_a_named_diagnostic_not_an_aborted_run() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    let ledger = ClientOrderEntryRiskLedger::new();
    let outcome = run_daily(
        cfg(dir.path(), 2),
        DecisionSink::new(),
        rank_all,
        always_enter_sharing(
            AlwaysEnterConfig {
                hold_sessions: 6,
                reenter: false,
                // Submitted at a precision-1 price against a price_precision-0 KRX
                // equity: denied by the risk engine, so it never opens a position.
                reject_entry: Some("000660.XKRX"),
                ..Default::default()
            },
            ledger.clone(),
        ),
    )
    .await
    .unwrap(); // the run FINISHES — a rejection is not a hard failure

    assert_eq!(
        outcome.unopened_entry_orders.len(),
        1,
        "the rejected entry is named as a run-level diagnostic: {:?}",
        outcome.unopened_entry_orders
    );
    let rejected = outcome.unopened_entry_orders[0];
    assert!(ledger.get(&rejected).is_some(), "it WAS recorded at submit");
    assert!(
        outcome.positions.iter().all(|p| p.opening_order_id != rejected),
        "and it opened no position"
    );
    assert!(
        outcome.positions.iter().all(|p| p.instrument_id == InstrumentId::from("005930.XKRX")),
        "only the non-rejected symbol traded: {:?}",
        outcome.positions.iter().map(|p| p.instrument_id).collect::<Vec<_>>()
    );
    assert!(!outcome.positions.is_empty(), "the rest of the run is unaffected");
    assert!(
        outcome.entry_risks.iter().all(|r| r.is_some()),
        "every position that DID open still carries its risk"
    );
}

/// A position with no recorded entry risk resolves to `None` and takes the legacy
/// P&L path rather than panicking.
#[tokio::test]
async fn a_position_with_no_recorded_entry_risk_resolves_to_none() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    let outcome = run_daily(
        cfg(dir.path(), 2),
        DecisionSink::new(),
        rank_all,
        always_enter_sharing(
            AlwaysEnterConfig {
                hold_sessions: 4,
                reenter: true,
                // Entered normally, but never recorded in the ledger.
                skip_risk: Some("000660.XKRX"),
                ..Default::default()
            },
            ClientOrderEntryRiskLedger::new(),
        ),
    )
    .await
    .unwrap(); // no panic

    let unrisked = InstrumentId::from("000660.XKRX");
    let risked = InstrumentId::from("005930.XKRX");
    assert!(
        outcome.positions.iter().any(|p| p.instrument_id == unrisked),
        "the un-recorded symbol did trade"
    );
    for (i, p) in outcome.positions.iter().enumerate() {
        if p.instrument_id == unrisked {
            assert_eq!(outcome.entry_risks[i], None, "no recorded entry → None, not a panic");
        } else if p.instrument_id == risked {
            assert!(outcome.entry_risks[i].is_some(), "the recorded symbol still joins");
        }
    }

    let report = PerformanceReport::from_positions_with_risk(
        &outcome.positions,
        &outcome.entry_risks,
        STARTING_BALANCE,
        None,
    );
    assert!(
        report.trades.iter().any(|t| t.symbol == unrisked.to_string() && t.risk_capital.is_none()),
        "the un-recorded trades take the legacy P&L path"
    );
    assert!(
        report.trades.iter().any(|t| t.symbol == risked.to_string() && t.risk_capital.is_some()),
        "the recorded trades still carry risk_capital"
    );
}
