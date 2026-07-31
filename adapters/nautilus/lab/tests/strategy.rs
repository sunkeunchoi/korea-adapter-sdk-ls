//! U3 — ORB v0 strategy tests. Pure, offline, zero network: the universe scan and
//! the range/entry/exit state machine are exercised in isolation of the engine
//! (the state machine is the unit most likely to hide off-by-one session-time bugs,
//! so it is built and tested test-first).

use chrono::NaiveTime;
use nautilus_ls_lab::agent::envelope::{Decision, DecisionTrigger, SignalKind};
use nautilus_ls_lab::agent::sink::DecisionSink;
use nautilus_ls_lab::params::OrbParams;
use nautilus_ls_lab::strategy::orb::{
    breakout_strength, select_universe, CandidateMeta, ExitReason, OrbAction, OrbState, Phase,
    SessionGapPrices, UniverseCandidate,
};

fn t(h: u32, m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, 0).unwrap()
}

/// Feed a bar under the v9 default path: `close == high`, zero volume. The
/// default-path (wick-touch, no RVOL) tests do not depend on `close` or `volume`,
/// so this keeps them as characterization of v9 behavior under the new signature.
fn bar(st: &mut OrbState, tm: NaiveTime, high: i64, low: i64, p: &OrbParams) -> Vec<OrbAction> {
    st.on_bar(tm, high, low, high, 0.0, p)
}

/// Drive the opening-range window (09:00–09:14) so a range of [low..high] is fixed.
fn set_range(st: &mut OrbState, params: &OrbParams, high: i64, low: i64) {
    assert!(bar(st, t(9, 0), high, low, params).is_empty());
    assert!(bar(st, t(9, 10), high, low, params).is_empty());
    assert_eq!(st.range(), Some((high, low)));
    assert_eq!(st.phase(), Phase::InRange);
}

/// Happy path: a clean range breakout enters once at the range-high break; the stop
/// and the time-flat exit are honored.
#[test]
fn clean_breakout_enters_once_and_time_exits() {
    let p = OrbParams::default();
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);

    // A breakout bar above the range high → one entry at a marketable limit (the
    // breakout bar's high).
    let acts = bar(&mut st, t(9, 20), 62_000, 61_000, &p);
    assert_eq!(acts, vec![OrbAction::Enter { limit_price: 62_000 }]);
    assert_eq!(st.phase(), Phase::Long);

    // A later bar that stays above the stop → no action.
    assert!(bar(&mut st, t(10, 0), 62_500, 61_800, &p).is_empty());

    // The time-flat deadline closes the position at a marketable limit.
    let acts = bar(&mut st, t(15, 0), 62_200, 62_000, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 62_000, reason: ExitReason::TimeFlat }]);
    assert_eq!(st.phase(), Phase::Done);
}

#[test]
fn gap_retention_off_ignores_session_inputs_and_preserves_complete_session_actions() {
    fn complete_session(prior_close: i64, today_open: i64) -> Vec<OrbAction> {
        let params = OrbParams::default();
        let mut state = OrbState::with_session_inputs(
            SessionGapPrices::new(prior_close, today_open),
            None,
            None,
            None,
        );
        let mut actions = Vec::new();
        actions.extend(bar(&mut state, t(9, 0), 61_500, 60_000, &params));
        actions.extend(bar(&mut state, t(9, 10), 61_500, 60_000, &params));
        actions.extend(bar(&mut state, t(9, 20), 62_000, 61_000, &params));
        actions.extend(bar(&mut state, t(10, 0), 63_000, 61_800, &params));
        actions.extend(bar(&mut state, t(15, 0), 63_200, 62_000, &params));
        actions
    }

    let expected = vec![
        OrbAction::Enter { limit_price: 62_000 },
        OrbAction::Exit { limit_price: 62_000, reason: ExitReason::TimeFlat },
    ];
    assert_eq!(
        complete_session(60_000, 63_000),
        expected,
        "OFF does not observe a below-0.50 retention input"
    );
    assert_eq!(
        complete_session(0, 0),
        expected,
        "OFF does not observe unavailable/not-applicable retention inputs"
    );
}

// ---------------------------------------------------------------------------
// #168 — the armed gap-retention session gate (complete-session decision streams)
// ---------------------------------------------------------------------------

/// The armed parameter set: the frozen 0.50 cutoff, everything else at the
/// filter-off defaults so the retention arm is the only active gate.
fn gap_armed() -> OrbParams {
    OrbParams { gap_retention_min: 0.50, ..Default::default() }
}

/// A session state carrying only the canonical gap prices (no ATR/RVOL priors).
fn gap_state(prior_close: i64, today_open: i64) -> OrbState {
    OrbState::with_session_inputs(SessionGapPrices::new(prior_close, today_open), None, None, None)
}

/// R2 leakage: the gate reads the frozen opening-range low, never a post-range
/// low. A first post-range bar that dips below the prior close (which would
/// measure retention −0.5 if it leaked into the observation) passes silently on
/// the frozen 0.5, and the session still enters on a later breakout.
#[test]
fn gap_retention_reads_the_frozen_range_low_never_a_post_range_low() {
    let p = gap_armed();
    let mut st = gap_state(60_000, 63_000);
    set_range(&mut st, &p, 63_500, 61_500); // frozen retention = 1_500/3_000 = 0.5
    let acts = bar(&mut st, t(9, 20), 63_400, 58_500, &p); // post-range low < prior close
    assert!(acts.is_empty(), "the frozen 0.5 passes; the post-range low changes nothing: {acts:?}");
    assert_eq!(st.range(), Some((63_500, 61_500)), "the recorded range is the frozen window");
    let acts = bar(&mut st, t(9, 30), 63_600, 63_000, &p);
    assert_eq!(acts, vec![OrbAction::Enter { limit_price: 63_600 }], "entry still proceeds");
}

/// Boundary: retention exactly 0.50 from system-produced canonical prices passes
/// with no envelope, and entry logic proceeds on that same bar's trigger rules.
#[test]
fn gap_retention_equality_at_the_cutoff_passes_quietly_and_enters_same_bar() {
    let p = gap_armed();
    let mut st = gap_state(60_000, 63_000);
    set_range(&mut st, &p, 61_800, 61_500); // (61_500 − 60_000)·2 == 63_000 − 60_000
    let acts = bar(&mut st, t(9, 20), 62_000, 61_700, &p);
    assert_eq!(
        acts,
        vec![OrbAction::Enter { limit_price: 62_000 }],
        "an exactly-0.50 session emits no envelope and takes the breakout"
    );
}

/// Measured reject: one tick below the boundary records a single
/// `gap_retention_min` rejection carrying all five values, transitions directly
/// to Done, and no later bar can act, break out, or order.
#[test]
fn gap_retention_one_tick_below_rejects_once_with_full_components() {
    let p = gap_armed();
    let mut st = gap_state(60_000, 63_000);
    set_range(&mut st, &p, 63_500, 61_499);
    let acts = bar(&mut st, t(9, 20), 64_000, 63_300, &p); // a would-be breakout bar
    assert_eq!(
        acts,
        vec![OrbAction::SessionReject {
            filter: "gap_retention_min",
            values: vec![
                ("gap_retention_min", 0.5),
                ("retention", 1_499.0 / 3_000.0),
                ("prior_close", 60_000.0),
                ("today_open", 63_000.0),
                ("range_low", 61_499.0),
            ],
        }]
    );
    assert_eq!(st.phase(), Phase::Done);
    // Single rejection, never per-bar; no breakout or order ever fires.
    assert!(bar(&mut st, t(9, 30), 65_000, 64_000, &p).is_empty());
    assert!(bar(&mut st, t(15, 0), 65_500, 65_000, &p).is_empty());
}

/// Failure class: a non-positive gap with an observed range rejects
/// `gap_retention_not_applicable`, recording the cutoff and every component
/// (all three exist here).
#[test]
fn gap_retention_non_positive_gap_rejects_not_applicable() {
    let p = gap_armed();
    let mut st = gap_state(63_000, 63_000); // zero gap
    set_range(&mut st, &p, 63_500, 62_500);
    let acts = bar(&mut st, t(9, 20), 64_000, 63_300, &p);
    assert_eq!(
        acts,
        vec![OrbAction::SessionReject {
            filter: "gap_retention_not_applicable",
            values: vec![
                ("gap_retention_min", 0.5),
                ("prior_close", 63_000.0),
                ("today_open", 63_000.0),
                ("range_low", 62_500.0),
            ],
        }]
    );
    assert_eq!(st.phase(), Phase::Done);
}

/// Failure class (KTD4): an armed session that never observed a range bar emits
/// `gap_retention_unavailable` before Done — missingness cannot silently pass.
/// `range_low` does not exist, so its key is omitted, never a numeric sentinel.
#[test]
fn gap_retention_no_range_session_rejects_unavailable_when_armed() {
    let p = gap_armed();
    let mut st = gap_state(60_000, 63_000); // positive gap, no range bars
    let acts = bar(&mut st, t(9, 20), 64_000, 63_300, &p);
    assert_eq!(
        acts,
        vec![OrbAction::SessionReject {
            filter: "gap_retention_unavailable",
            values: vec![
                ("gap_retention_min", 0.5),
                ("prior_close", 60_000.0),
                ("today_open", 63_000.0),
            ],
        }]
    );
    assert_eq!(st.phase(), Phase::Done);
    assert!(bar(&mut st, t(9, 30), 65_000, 64_000, &p).is_empty(), "one rejection only");
}

/// Failure class ordering (KTD4 routes through the KTD3 classifier): a no-range
/// session whose gap is also non-positive records `gap_retention_not_applicable`
/// — applicability outranks availability.
#[test]
fn gap_retention_no_range_non_positive_gap_rejects_not_applicable() {
    let p = gap_armed();
    let mut st = gap_state(63_000, 62_000); // gap-down, no range bars
    let acts = bar(&mut st, t(9, 20), 64_000, 63_300, &p);
    assert_eq!(
        acts,
        vec![OrbAction::SessionReject {
            filter: "gap_retention_not_applicable",
            values: vec![
                ("gap_retention_min", 0.5),
                ("prior_close", 63_000.0),
                ("today_open", 62_000.0),
            ],
        }]
    );
}

/// Failure class: a frozen range low above today's open measures above 1.0 —
/// inconsistent data rejects `gap_retention_invalid` with the cutoff and all
/// three components (the non-finite/above-one retention itself is never a value).
#[test]
fn gap_retention_above_one_rejects_invalid() {
    let p = gap_armed();
    let mut st = gap_state(60_000, 61_000);
    set_range(&mut st, &p, 62_000, 61_500); // range low 61_500 > today_open 61_000
    let acts = bar(&mut st, t(9, 20), 62_500, 61_800, &p);
    assert_eq!(
        acts,
        vec![OrbAction::SessionReject {
            filter: "gap_retention_invalid",
            values: vec![
                ("gap_retention_min", 0.5),
                ("prior_close", 60_000.0),
                ("today_open", 61_000.0),
                ("range_low", 61_500.0),
            ],
        }]
    );
    assert_eq!(st.phase(), Phase::Done);
}

/// Gate ordering: retention is the FINAL arm — a session failing both RVOL and
/// retention records only the RVOL filter (first-failing-gate-records-only).
#[test]
fn gate_order_records_rvol_before_gap_retention() {
    let p = OrbParams { rvol_min: 0.5, ..gap_armed() };
    // Retention would fail (61_000 → 1_000/3_000 ≈ 0.33) and so does RVOL
    // (open-window volume 0 < 0.5 · 10_000).
    let mut st = OrbState::with_session_inputs(
        SessionGapPrices::new(60_000, 63_000),
        None,
        Some(10_000.0),
        None,
    );
    set_range(&mut st, &p, 63_500, 61_000);
    let acts = bar(&mut st, t(9, 20), 64_000, 63_300, &p);
    assert_eq!(acts.len(), 1, "exactly one rejection: {acts:?}");
    match &acts[0] {
        OrbAction::SessionReject { filter, .. } => {
            assert_eq!(*filter, "rvol_min", "the first failing gate records, not retention");
        }
        other => panic!("expected a session reject, got {other:?}"),
    }
}

/// OFF compatibility for the KTD4 hook: while OFF, a no-range session still
/// rolls to Done silently — no envelope, byte-stable with the pre-#168 stream.
#[test]
fn gap_retention_off_no_range_session_stays_silent() {
    let p = OrbParams::default();
    let mut st = gap_state(60_000, 63_000); // positive gap, no range bars, OFF
    assert!(bar(&mut st, t(9, 20), 64_000, 63_300, &p).is_empty(), "OFF stays silent");
    assert_eq!(st.phase(), Phase::Done);
}

/// The stop (range low) is honored: after entry, a bar that breaches the range low
/// exits with reason Stop.
#[test]
fn stop_exit_fires_on_range_low_breach() {
    let p = OrbParams::default();
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // Next bar breaches the stop → marketable sell at the breaching bar's low.
    let acts = bar(&mut st, t(9, 30), 61_800, 59_900, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 59_900, reason: ExitReason::Stop }]);
    assert_eq!(st.phase(), Phase::Done);
}

/// No breakout all session → zero entries and a time-flat no-op (nothing to close).
#[test]
fn no_breakout_is_a_time_flat_no_op() {
    let p = OrbParams::default();
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    // Bars never exceed the range high.
    assert!(bar(&mut st, t(10, 0), 61_400, 60_500, &p).is_empty());
    assert!(bar(&mut st, t(12, 0), 61_499, 60_800, &p).is_empty());
    // Time-flat with no position → no exit.
    assert!(bar(&mut st, t(15, 0), 61_200, 60_900, &p).is_empty());
    assert_eq!(st.phase(), Phase::Done);
}

/// A whipsaw bar (breaks the range high AND breaches the range low) enters then stops
/// out in the same bar sequence.
#[test]
fn whipsaw_bar_enters_then_stops_same_bar() {
    let p = OrbParams::default();
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    let acts = bar(&mut st, t(9, 20), 62_000, 59_000, &p);
    assert_eq!(
        acts,
        vec![
            OrbAction::Enter { limit_price: 62_000 },
            OrbAction::Exit { limit_price: 59_000, reason: ExitReason::Stop },
        ]
    );
    assert_eq!(st.phase(), Phase::Done);
}

/// A data gap over the opening-range window (first bar arrives after 09:15) → the
/// symbol never trades; it is marked Done rather than guessing a range.
#[test]
fn missing_opening_range_never_trades() {
    let p = OrbParams::default();
    let mut st = OrbState::new();
    // First bar is at 09:20 — the range window was never observed.
    assert!(bar(&mut st, t(9, 20), 62_000, 61_000, &p).is_empty());
    assert_eq!(st.phase(), Phase::Done);
    assert_eq!(st.range(), None);
}

/// The session summary reports the extreme values observed.
#[test]
fn session_extremes_track_high_and_low() {
    let p = OrbParams::default();
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    bar(&mut st, t(9, 20), 63_000, 61_000, &p);
    bar(&mut st, t(11, 0), 62_000, 58_500, &p);
    assert_eq!(st.session_extremes(), Some((63_000, 58_500)));
}

// ---------------------------------------------------------------------------
// Fixed profit target (Turn 8 / v9)
// ---------------------------------------------------------------------------

/// (U2 scenario 1) A held long that reaches the fixed target exits at the target
/// price — a favorable limit `entry_price + round(profit_target_r · R)`, not the
/// bar wick.
#[test]
fn target_exit_fires_at_the_target_price() {
    let p = OrbParams::default(); // profit_target_r 1.0
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000); // R = 1500
    // Entry at the breakout high → entry_price 62_000, target = 62_000 + 1.0*1500.
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // A later bar whose high clears the 63_500 target (and does not breach the stop).
    let acts = bar(&mut st, t(10, 0), 63_600, 62_500, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 63_500, reason: ExitReason::Target }]);
    assert_eq!(st.phase(), Phase::Done);
}

/// (U2 scenario 2) A long that approaches but never reaches the target, then breaches
/// the stop, exits Stop — never Target.
#[test]
fn approach_then_revert_stops_never_targets() {
    let p = OrbParams::default();
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000); // R = 1500, target 63_500
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // Nears the target (63_400 < 63_500) but does not reach it → no action.
    assert!(bar(&mut st, t(9, 30), 63_400, 61_200, &p).is_empty());
    // A later bar breaches the stop → Stop, not Target.
    let acts = bar(&mut st, t(9, 40), 61_000, 59_900, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 59_900, reason: ExitReason::Stop }]);
    assert_eq!(st.phase(), Phase::Done);
}

/// (U2 scenario 5, Covers R4 / KTD2) When one Long bar breaches both the target and
/// the stop, Stop wins — the pessimistic precedence, since intrabar order is unknowable.
#[test]
fn same_bar_target_and_stop_resolves_to_stop() {
    let p = OrbParams::default();
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000); // R = 1500, target 63_500
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // One bar clears the target (high 63_600 ≥ 63_500) AND breaches the stop
    // (low 59_900 ≤ 60_000) → Stop wins.
    let acts = bar(&mut st, t(9, 30), 63_600, 59_900, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 59_900, reason: ExitReason::Stop }]);
    assert_eq!(st.phase(), Phase::Done);
}

/// (U2 scenario 6, Covers R5) `mfe_r()` reports the post-entry high-water excursion in
/// R-multiples: `(high_water − entry_price) / R`, and stays 0.0 before an entry.
#[test]
fn mfe_r_reports_post_entry_excursion() {
    let p = OrbParams::default();
    let mut st = OrbState::new();
    assert_eq!(st.mfe_r(), 0.0, "no excursion before a range/entry");
    set_range(&mut st, &p, 61_500, 60_000); // R = 1500
    assert_eq!(st.mfe_r(), 0.0, "no excursion before an entry");
    bar(&mut st, t(9, 20), 62_000, 61_000, &p); // entry_price 62_000
    // A bar peaking at 63_000 (below the 63_500 target) sets the high-water mark.
    assert!(bar(&mut st, t(10, 0), 63_000, 61_500, &p).is_empty());
    // (63_000 − 62_000) / 1500 = 0.6667.
    assert!((st.mfe_r() - (1_000.0 / 1_500.0)).abs() < 1e-9, "mfe_r = {}", st.mfe_r());
    assert_eq!(st.entry_price(), 62_000);
}

/// A wider `profit_target_r` (the 1.5 sim optimum) moves the target proportionally —
/// the param actually drives the exit level.
#[test]
fn profit_target_r_scales_the_target_level() {
    let mut p = OrbParams::default();
    p.profit_target_r = 1.5;
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000); // R = 1500, target = 62_000 + 1.5*1500 = 64_250
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // 63_600 would trip a 1.0R target but not the 1.5R one → held.
    assert!(bar(&mut st, t(10, 0), 63_600, 62_500, &p).is_empty());
    // 64_300 clears 64_250 → Target at the wider level.
    let acts = bar(&mut st, t(11, 0), 64_300, 63_000, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 64_250, reason: ExitReason::Target }]);
}

/// (review fix) A TimeFlat exit's MFE includes the flat bar's high — the flat bar
/// is part of the hold, so its excursion counts (symmetry with Stop/Target exits).
#[test]
fn timeflat_mfe_includes_the_flat_bar_high() {
    let p = OrbParams::default();
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000); // R = 1500, target 63_500
    bar(&mut st, t(9, 20), 62_000, 61_000, &p); // entry_price 62_000
    // A mid-hold peak below the target.
    assert!(bar(&mut st, t(10, 0), 63_000, 61_500, &p).is_empty());
    // The 15:00 flat bar prints a NEW high (still below the target) then closes flat.
    let acts = bar(&mut st, t(15, 0), 63_200, 62_000, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 62_000, reason: ExitReason::TimeFlat }]);
    // MFE reflects the flat bar's 63_200 high, not the earlier 63_000 peak.
    assert!((st.mfe_r() - (1_200.0 / 1_500.0)).abs() < 1e-9, "mfe_r = {}", st.mfe_r());
}

/// (review fix) A non-positive `profit_target_r` (e.g. a hand-seeded manifest) must
/// NOT fire an immediate same-bar breakeven target — the position holds to the stop
/// or the bell as if no target were configured.
#[test]
fn non_positive_profit_target_r_never_fires() {
    let mut p = OrbParams::default();
    p.profit_target_r = 0.0;
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    // Entry bar emits ONLY an Enter — no same-bar breakeven Target exit.
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    assert_eq!(st.phase(), Phase::Long);
    // A bar far above any conceivable target still does not exit.
    assert!(bar(&mut st, t(10, 0), 70_000, 61_000, &p).is_empty());
    // The position holds to the time-flat backstop.
    let acts = bar(&mut st, t(15, 0), 69_000, 68_000, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 68_000, reason: ExitReason::TimeFlat }]);
}

// ---------------------------------------------------------------------------
// MFE fold semantics (Turn 10 / v12, R6 / KTD5): mfe_r folds only the excursion
// provably observed while the position was open. Exit determination precedes the
// fold — a Stop-exit bar's high is excluded (not provably pre-stop); a
// Target-exit bar caps at the target price (the above-target wick is not provably
// pre-exit). Reporting-only: no entry/exit decision or P&L changes (exits read
// bar high/low directly, never high_water).
// ---------------------------------------------------------------------------

/// KTD5: a Stop-exit bar whose high exceeds all prior highs is EXCLUDED from
/// `mfe_r` — under stop-first pessimism its high is not provably pre-stop.
#[test]
fn stop_exit_excludes_the_stop_bar_high_from_mfe() {
    let p = OrbParams::default(); // profit_target_r 1.0
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000); // R = 1500, target 63_500, stop 60_000
    bar(&mut st, t(9, 20), 62_000, 61_000, &p); // entry 62_000
    // A mid-hold peak below the target fixes the high-water at 63_000.
    assert!(bar(&mut st, t(9, 30), 63_000, 61_000, &p).is_empty());
    // A stop bar whose HIGH (63_400) tops all prior highs but whose LOW breaches
    // the stop → Stop exit, and the bar high is excluded from MFE.
    let acts = bar(&mut st, t(9, 40), 63_400, 59_900, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 59_900, reason: ExitReason::Stop }]);
    // MFE stays at the 63_000 peak, not the 63_400 stop-bar high.
    assert!((st.mfe_r() - (1_000.0 / 1_500.0)).abs() < 1e-9, "stop bar high excluded: {}", st.mfe_r());
}

/// KTD5: a Target-exit bar with an above-target wick caps `mfe_r` at exactly
/// `profit_target_r` — price provably reached the target, but the wick above it
/// is not provably pre-exit. This makes the report's right-censoring claim exact.
#[test]
fn target_exit_caps_mfe_at_profit_target_r() {
    let p = OrbParams::default(); // profit_target_r 1.0 → cap at 1.0R
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000); // R = 1500, entry→target 63_500
    bar(&mut st, t(9, 20), 62_000, 61_000, &p); // entry 62_000
    // A target-exit bar with a wick far above the 63_500 target (high 64_800).
    let acts = bar(&mut st, t(10, 0), 64_800, 62_500, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 63_500, reason: ExitReason::Target }]);
    // MFE caps at exactly 1.0R — the above-target wick is excluded.
    assert!((st.mfe_r() - 1.0).abs() < 1e-9, "mfe caps at profit_target_r: {}", st.mfe_r());
}

/// KTD5 / R6: a degenerate-range trade still reports `mfe_r = 0.0` — the
/// `saw_range`/`R ≤ 0` sentinel guard survives the fold restructure.
#[test]
fn degenerate_range_trade_reports_zero_mfe() {
    let p = OrbParams::default();
    let mut st = OrbState::new();
    // A flat opening range (high == low) → R = 0.
    assert!(bar(&mut st, t(9, 0), 61_000, 61_000, &p).is_empty());
    assert_eq!(st.range(), Some((61_000, 61_000)));
    // Breakout above the flat range → entry (no target with R ≤ 0).
    bar(&mut st, t(9, 20), 62_000, 61_500, &p);
    // A higher bar folds into high_water, but the degenerate range makes mfe_r
    // report 0.0 via the sentinel guard.
    bar(&mut st, t(10, 0), 63_000, 61_800, &p);
    assert_eq!(st.mfe_r(), 0.0, "degenerate range → mfe_r 0.0");
}

// ---------------------------------------------------------------------------
// U3 lever queue: stop modes (KTD4/KTD5) + close-confirmed entry (KTD6)
// ---------------------------------------------------------------------------

/// U4 sizing precondition: the entry-fixed stop is populated on the state at the
/// moment the `Enter` action is returned, so `risk_per_share = entry − stop` is
/// available for `position_qty_risked` when the strategy handles the action on the
/// same bar (the Implementation-Time Unknown the plan flags). Verified across all
/// three stop modes.
#[test]
fn stop_and_risk_per_share_are_set_at_entry_across_stop_modes() {
    // v9 range-low: entry 62_000, stop = range low 60_000 → risk_per_share 2_000.
    let p = OrbParams::default();
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    assert_eq!(
        st.on_bar(t(9, 20), 62_000, 61_000, 61_500, 0.0, &p),
        vec![OrbAction::Enter { limit_price: 62_000 }]
    );
    assert_eq!(st.stop_price(), 60_000, "range-low stop set at entry");
    assert_eq!(st.risk_per_share(), 2_000, "entry − stop available at Enter time");

    // OR-midpoint: entry 62_000, stop = round((61_500+60_000)/2) = 60_750 → 1_250.
    let mut p_mid = OrbParams::default();
    p_mid.stop_mode = 1.0;
    let mut mid = OrbState::new();
    set_range(&mut mid, &p_mid, 61_500, 60_000);
    assert_eq!(
        mid.on_bar(t(9, 20), 62_000, 61_000, 61_500, 0.0, &p_mid),
        vec![OrbAction::Enter { limit_price: 62_000 }]
    );
    assert_eq!(mid.stop_price(), 60_750);
    assert_eq!(mid.risk_per_share(), 1_250);

    // Before any entry the accessors are zero/non-positive → sizing falls back to
    // notional (never a spurious tiny-stop blow-up).
    let fresh = OrbState::new();
    assert_eq!(fresh.stop_price(), 0);
    assert!(fresh.risk_per_share() <= 0);
}

/// Midpoint stop mode: the stop sits at the rounded OR midpoint, a pullback into
/// the lower half stops out (intended failed-break semantics), and both target
/// and MFE denominate by trade-R = entry − midpoint.
#[test]
fn midpoint_stop_mode_places_stop_at_midpoint_and_rescales_r() {
    let mut p = OrbParams::default();
    p.stop_mode = 1.0; // OR-midpoint
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000); // midpoint = round(60_750) = 60_750
    // Entry at the wick break → entry 62_000; trade-R = 62_000 − 60_750 = 1_250.
    assert_eq!(st.on_bar(t(9, 20), 62_000, 61_000, 61_500, 0.0, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // A pullback into the lower half (low 60_700 ≤ 60_750 midpoint, but well above
    // the 60_000 range low) stops out — the v9 range-low stop would NOT have fired.
    let acts = st.on_bar(t(9, 30), 61_800, 60_700, 61_000, 0.0, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 60_700, reason: ExitReason::Stop }]);
    assert_eq!(st.phase(), Phase::Done);
}

/// Midpoint mode: target = entry + profit_target_r × trade-R (trade-R = entry −
/// midpoint), strictly tighter than the v9 range-R target.
#[test]
fn midpoint_stop_mode_target_uses_trade_r() {
    let mut p = OrbParams::default(); // profit_target_r 1.0
    p.stop_mode = 1.0;
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000); // midpoint 60_750
    // entry 62_000, trade-R = 1_250 → target = 62_000 + 1_250 = 63_250.
    assert_eq!(st.on_bar(t(9, 20), 62_000, 61_000, 61_500, 0.0, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // A bar clearing 63_250 (but below the v9 63_500 range-R target) banks Target.
    let acts = st.on_bar(t(10, 0), 63_300, 62_800, 63_000, 0.0, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 63_250, reason: ExitReason::Target }]);
    // MFE caps at trade-R (1.0R of the tighter denominator).
    assert!((st.mfe_r() - 1.0).abs() < 1e-9, "mfe_r = {}", st.mfe_r());
}

/// ATR stop mode: stop = entry − round(stop_atr_mult × ATR) when that is above the
/// range low; trade-R and MFE re-scale to it.
#[test]
fn atr_stop_mode_places_stop_below_entry_by_atr() {
    let mut p = OrbParams::default();
    p.stop_mode = 2.0; // ATR
    p.stop_atr_mult = 2.0;
    let mut st = OrbState::with_priors(Some(300.0), None, None); // 2 × 300 = 600 below entry
    set_range(&mut st, &p, 61_500, 60_000);
    // entry 62_000 → stop = max(62_000 − 600, 60_000) = 61_400; trade-R = 600.
    // The entry bar's low (61_600) stays above the ATR stop → clean entry.
    assert_eq!(st.on_bar(t(9, 20), 62_000, 61_600, 61_500, 0.0, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // A bar dipping to 61_390 ≤ 61_400 stops out (the range-low stop would hold).
    let acts = st.on_bar(t(9, 30), 61_900, 61_390, 61_600, 0.0, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 61_390, reason: ExitReason::Stop }]);
}

/// ATR mode clamps the stop to the range low when the ATR distance would be wider
/// than the v9 stop — ATR only ever narrows the stop (KTD5), never widens it.
#[test]
fn atr_stop_mode_clamps_to_range_low_when_wider() {
    let mut p = OrbParams::default();
    p.stop_mode = 2.0;
    p.stop_atr_mult = 2.0;
    // 2 × 5_000 = 10_000 below entry (62_000 − 10_000 = 52_000) is far below the
    // 60_000 range low → clamp to 60_000 (the v9 stop).
    let mut st = OrbState::with_priors(Some(5_000.0), None, None);
    set_range(&mut st, &p, 61_500, 60_000);
    assert_eq!(st.on_bar(t(9, 20), 62_000, 61_000, 61_500, 0.0, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // A dip to 60_100 (above the 60_000 clamped stop) does NOT stop out.
    assert!(st.on_bar(t(9, 30), 61_800, 60_100, 61_000, 0.0, &p).is_empty());
    // A dip to 59_900 ≤ 60_000 does.
    let acts = st.on_bar(t(9, 40), 61_000, 59_900, 60_500, 0.0, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 59_900, reason: ExitReason::Stop }]);
}

/// ATR mode: `mfe_r` denominates by trade-R (`entry − stop`), distinct from the
/// range-R the v9 mode-0 path uses (KTD4).
#[test]
fn atr_stop_mode_mfe_denominates_by_trade_r() {
    let mut p = OrbParams::default();
    p.stop_mode = 2.0;
    p.stop_atr_mult = 2.0;
    let mut st = OrbState::with_priors(Some(300.0), None, None); // stop 600 below entry
    set_range(&mut st, &p, 61_500, 60_000); // range-R 1500 (NOT the denominator here)
    // entry 62_000, stop 61_400, trade-R = 600.
    st.on_bar(t(9, 20), 62_000, 61_600, 61_500, 0.0, &p);
    // A peak at 62_300 (below the 62_600 target) folds high-water; no stop (low > 61_400).
    assert!(st.on_bar(t(9, 30), 62_300, 61_700, 62_200, 0.0, &p).is_empty());
    // mfe = (62_300 − 62_000) / 600 = 0.5 — trade-R, not (300/1500 = 0.2) range-R.
    assert!((st.mfe_r() - 0.5).abs() < 1e-9, "mfe denominated by trade-R 600: {}", st.mfe_r());
}

/// KTD4 decoupling: breakout strength keys on range-R (a degenerate range → `None`
/// → the band is bypassed), while the midpoint stop's trade-R is separately
/// well-defined and denominates MFE. Mode 0 would report `mfe_r = 0.0` here; the
/// non-default mode does not, because its denominator is trade-R, not range-R.
#[test]
fn degenerate_range_midpoint_defines_trade_r_while_strength_stays_none() {
    let mut p = OrbParams::default();
    p.stop_mode = 1.0; // midpoint
    let mut st = OrbState::new();
    assert!(st.on_bar(t(9, 0), 61_000, 61_000, 61_000, 0.0, &p).is_empty()); // flat range
    // Enter at 62_000; midpoint stop = round((61_000+61_000)/2) = 61_000, trade-R = 1_000.
    assert_eq!(
        st.on_bar(t(9, 20), 62_000, 61_500, 61_800, 0.0, &p),
        vec![OrbAction::Enter { limit_price: 62_000 }]
    );
    // Peak 62_500 (< the 63_000 target) folds; mfe = 500/1_000 = 0.5 (trade-R defined).
    assert!(st.on_bar(t(9, 30), 62_500, 61_500, 62_200, 0.0, &p).is_empty());
    assert!((st.mfe_r() - 0.5).abs() < 1e-9, "trade-R defined despite degenerate range: {}", st.mfe_r());
    // Strength itself keys on range-R and stays None on the degenerate range — the
    // band-bypass invariant, independent of the well-defined trade-R.
    assert_eq!(breakout_strength(62_000, 61_000, 61_000), None);
}

// ---------------------------------------------------------------------------
// Breakeven-move exit lever (lever 6, KTD11): once observed MFE reaches
// `breakeven_trigger_r · R`, the stop ratchets to entry for SUBSEQUENT bars — a
// runner that peaks then reverts books at breakeven instead of decaying to the
// time-flat exit. Off (0.0) is byte-identical to v9. The ratchet never fires on
// the bar that triggers it (same-bar stop-first pessimism, KTD2), only tightens
// the stop, and arms once.
// ---------------------------------------------------------------------------

/// Off (`breakeven_trigger_r == 0.0`): a give-back trade that peaks well above the
/// trigger-that-would-be, then reverts toward entry, does NOT stop at breakeven —
/// it rides to the time-flat exit exactly as v9 (byte-identical no-op).
#[test]
fn breakeven_off_is_a_no_op_giveback_rides_to_time_flat() {
    let p = OrbParams::default(); // breakeven_trigger_r 0.0
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000); // R 1500, entry→stop 60_000, target 63_500
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // A bar peaks at 62_800 (would arm a 0.5R=62_750 breakeven) — but the lever is off.
    assert!(bar(&mut st, t(9, 30), 62_800, 62_000, &p).is_empty());
    // Reverts to 61_900 — above the 60_000 range-low stop, so with the lever off no
    // exit fires (a breakeven stop at 62_000 WOULD have stopped here).
    assert!(bar(&mut st, t(10, 0), 62_400, 61_900, &p).is_empty());
    // Rides to the time-flat exit.
    let acts = bar(&mut st, t(15, 0), 62_100, 62_000, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 62_000, reason: ExitReason::TimeFlat }]);
}

/// On: a runner that reaches the `breakeven_trigger_r · R` MFE then reverts to entry
/// books a Stop at the ratcheted breakeven stop on a LATER bar — not a time-flat
/// give-back. This is the whole mechanism.
#[test]
fn breakeven_arms_then_books_at_the_ratcheted_stop() {
    let mut p = OrbParams::default();
    p.breakeven_trigger_r = 0.5; // trigger = 62_000 + round(0.5 * 1500) = 62_750
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000); // R 1500, entry 62_000, initial stop 60_000
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // Bar A peaks at 62_800 ≥ 62_750 (below the 63_500 target) and holds above the
    // stop → no exit, but the ratchet arms: the stop moves to entry (62_000).
    assert!(bar(&mut st, t(9, 30), 62_800, 62_000, &p).is_empty());
    // Bar B dips to 61_900 ≤ 62_000 (the ratcheted stop) → Stop at the bar low. Under
    // the v9 60_000 stop this bar would NOT have exited.
    let acts = bar(&mut st, t(10, 0), 62_400, 61_900, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 61_900, reason: ExitReason::Stop }]);
    assert_eq!(st.phase(), Phase::Done);
}

/// KTD2 same-bar pessimism: the ratchet is NOT applied to the bar that triggers it,
/// even when that same bar dips below entry. Same-bar order is unknowable — the low
/// may precede the high that arms the move — so booking a breakeven stop on the
/// triggering bar would be a stop the position never provably reached. The stop
/// binds only from the NEXT bar.
#[test]
fn breakeven_does_not_book_on_the_triggering_bar() {
    let mut p = OrbParams::default();
    p.breakeven_trigger_r = 0.5; // trigger 62_750
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // Bar A both arms the ratchet (high 62_800 ≥ 62_750) AND dips below entry
    // (low 61_800 < 62_000) in the same bar. The stop check uses the OLD 60_000 stop
    // (61_800 > 60_000 → no exit); the ratchet arms only AFTER, for later bars.
    assert!(bar(&mut st, t(9, 30), 62_800, 61_800, &p).is_empty());
    assert_eq!(st.phase(), Phase::Long, "no same-bar breakeven stop-out");
    // The NEXT bar's dip to 61_900 ≤ 62_000 does book the breakeven stop.
    let acts = bar(&mut st, t(10, 0), 62_200, 61_900, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 61_900, reason: ExitReason::Stop }]);
}

/// The trigger binds on its threshold: a peak BELOW `breakeven_trigger_r · R` does
/// not arm the ratchet, so a later revert toward entry is not stopped at breakeven.
#[test]
fn breakeven_does_not_arm_below_the_trigger() {
    let mut p = OrbParams::default();
    p.breakeven_trigger_r = 0.5; // trigger 62_750
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // A peak of 62_700 < 62_750 does NOT arm the ratchet.
    assert!(bar(&mut st, t(9, 30), 62_700, 62_000, &p).is_empty());
    // A dip to 61_900 is above the un-ratcheted 60_000 stop → no exit (the lever
    // never armed, so nothing tightened).
    assert!(bar(&mut st, t(10, 0), 62_400, 61_900, &p).is_empty());
    let acts = bar(&mut st, t(15, 0), 62_100, 62_000, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 62_000, reason: ExitReason::TimeFlat }]);
}

/// An armed breakeven ratchet never blocks a subsequent Target: a winner that arms
/// the move on one bar and then clears the target on a later bar (staying above the
/// ratcheted stop) still banks Target, with MFE capped at `profit_target_r`. The
/// ratchet neither corrupts the high-water fold nor pre-empts the target.
#[test]
fn breakeven_armed_trade_still_targets() {
    let mut p = OrbParams::default(); // profit_target_r 1.0 → target 63_500
    p.breakeven_trigger_r = 0.5; // trigger 62_750
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // Bar A arms the ratchet (high 62_800), holds above entry → no exit, stop → 62_000.
    assert!(bar(&mut st, t(9, 30), 62_800, 62_100, &p).is_empty());
    // Bar B clears the 63_500 target and stays above the 62_000 ratcheted stop → Target.
    let acts = bar(&mut st, t(10, 0), 63_600, 62_500, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 63_500, reason: ExitReason::Target }]);
    assert!((st.mfe_r() - 1.0).abs() < 1e-9, "mfe caps at profit_target_r: {}", st.mfe_r());
}

/// The trigger is inclusive (`high_water >= trigger`): a peak landing EXACTLY on
/// `breakeven_trigger_r · R` arms the ratchet. This pins the `>=` boundary — a
/// subtly-wrong strict `>` would leave the stop un-ratcheted and pass every other
/// breakeven test.
#[test]
fn breakeven_arms_at_exactly_the_trigger() {
    let mut p = OrbParams::default();
    p.breakeven_trigger_r = 0.5; // trigger = 62_000 + round(0.5 * 1500) = 62_750
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // Peak lands EXACTLY on the 62_750 trigger → arms (stop → 62_000).
    assert!(bar(&mut st, t(9, 30), 62_750, 62_000, &p).is_empty());
    // The next bar's dip to 61_900 ≤ 62_000 books the breakeven stop — proving the
    // exact-boundary peak did arm.
    let acts = bar(&mut st, t(10, 0), 62_400, 61_900, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 61_900, reason: ExitReason::Stop }]);
}

/// Stop-first pessimism survives the ratchet: after breakeven arms, a later bar that
/// breaches the ratcheted (entry) stop AND clears the target resolves to Stop, not
/// Target — the ratcheted stop is checked first (KTD2), booking the breakeven loss.
#[test]
fn breakeven_stop_beats_target_after_arming() {
    let mut p = OrbParams::default(); // target 63_500
    p.breakeven_trigger_r = 0.5; // trigger 62_750
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // Bar A arms the ratchet (high 62_800), holds above entry → stop moves to 62_000.
    assert!(bar(&mut st, t(9, 30), 62_800, 62_100, &p).is_empty());
    // Bar B clears the 63_500 target (high 63_600) AND breaches the 62_000 ratcheted
    // stop (low 61_900) → Stop wins (checked first), booked at the bar low.
    let acts = bar(&mut st, t(10, 0), 63_600, 61_900, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 61_900, reason: ExitReason::Stop }]);
}

/// Under a non-default stop mode the trigger denominates by trade-R (`entry − stop`),
/// not range-R — matching the entry-fixed `r_denom`. A peak that arms the trade-R
/// trigger (62_625) but sits below the range-R trigger (62_750) proves the
/// denominator is trade-R.
#[test]
fn breakeven_arms_on_trade_r_under_midpoint_stop() {
    let mut p = OrbParams::default();
    p.stop_mode = 1.0; // OR-midpoint → midpoint 60_750, trade-R = 62_000 − 60_750 = 1_250
    p.breakeven_trigger_r = 0.5; // trade-R trigger = 62_000 + round(0.5 * 1250) = 62_625
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    assert_eq!(st.on_bar(t(9, 20), 62_000, 61_000, 61_500, 0.0, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // Peak 62_700 ≥ the 62_625 trade-R trigger (but < the 62_750 range-R trigger) →
    // arms off trade-R. Low 61_000 stays above the 60_750 midpoint stop.
    assert!(st.on_bar(t(9, 30), 62_700, 61_000, 62_200, 0.0, &p).is_empty());
    // The next bar dips to 61_900 ≤ 62_000 (ratcheted stop) → breakeven Stop.
    let acts = st.on_bar(t(10, 0), 62_400, 61_900, 62_000, 0.0, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 61_900, reason: ExitReason::Stop }]);
}

/// The ratchet arms once and never re-widens or drifts: after the stop moves to entry,
/// a later even-higher peak leaves the stop at entry (with the trail off the new stop
/// is exactly entry, and `stop_price.max(entry)` is a no-op once armed), so a
/// subsequent dip still stops at breakeven.
#[test]
fn breakeven_ratchet_is_idempotent_and_never_rewidens() {
    let mut p = OrbParams::default(); // target 63_500
    p.breakeven_trigger_r = 0.5; // trigger 62_750
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // Bar A arms (high 62_800 → stop 62_000).
    assert!(bar(&mut st, t(9, 30), 62_800, 62_100, &p).is_empty());
    // Bar B sets a new, higher high (63_000, below the 63_500 target); the ratchet
    // must NOT move the stop again (stays at entry 62_000, neither widened nor lifted).
    assert!(bar(&mut st, t(10, 0), 63_000, 62_200, &p).is_empty());
    // Bar C dips to 61_950 ≤ 62_000 → Stop at the unchanged breakeven level.
    let acts = bar(&mut st, t(10, 30), 62_400, 61_950, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 61_950, reason: ExitReason::Stop }]);
}

/// A tiny positive trigger whose rounded R-offset is 0 is treated as OFF (not an
/// instant first-bar breakeven): `breakeven_trigger_price` requires the effective
/// trigger to sit strictly above entry, so the give-back rides to the time-flat exit.
#[test]
fn breakeven_tiny_trigger_that_rounds_to_zero_is_off() {
    let mut p = OrbParams::default();
    p.breakeven_trigger_r = 0.0001; // round(0.0001 * 1500) = round(0.15) = 0 → off
    assert!(p.validate().is_ok(), "a positive trigger validates");
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // A peak far above entry does NOT arm a breakeven (rounded offset is 0 → None).
    assert!(bar(&mut st, t(9, 30), 62_800, 62_000, &p).is_empty());
    // A dip toward entry is not stopped — the range-low 60_000 stop still governs.
    assert!(bar(&mut st, t(10, 0), 62_400, 61_900, &p).is_empty());
    let acts = bar(&mut st, t(15, 0), 62_100, 62_000, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 62_000, reason: ExitReason::TimeFlat }]);
}

/// The breakeven ratchet composes with close-confirmed entry (the v21 baseline's
/// active entry mode, `entry_confirm=1.0`) — the actual flip run path. The entry bar
/// anchors at close and skips its same-bar stop; the ratchet then arms on a later bar
/// exactly as in wick mode (it reads only high_water / entry / stop, not entry mode).
#[test]
fn breakeven_arms_under_close_confirm_entry() {
    let mut p = OrbParams::default();
    p.entry_confirm = 1.0; // close-confirmed entry (v21's mode)
    p.breakeven_trigger_r = 0.5; // trigger 62_750
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    // Close-confirm enters at the CLOSE (62_000 > the 61_500 range high); the entry
    // bar's 62_200 high is not folded and its same-bar stop is skipped.
    assert_eq!(
        st.on_bar(t(9, 20), 62_200, 61_000, 62_000, 0.0, &p),
        vec![OrbAction::Enter { limit_price: 62_000 }]
    );
    // Bar A peaks at 62_800 ≥ 62_750 → arms the ratchet (stop → entry 62_000).
    assert!(st.on_bar(t(9, 30), 62_800, 62_000, 62_400, 0.0, &p).is_empty());
    // Bar B dips to 61_900 ≤ 62_000 → breakeven Stop, just as in wick mode.
    let acts = st.on_bar(t(10, 0), 62_400, 61_900, 62_100, 0.0, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 61_900, reason: ExitReason::Stop }]);
}

// ---------------------------------------------------------------------------
// Breakeven-TRAIL exit lever (candidate A on top of lever 6, KTD12): once the
// breakeven ratchet has armed (`high_water ≥ breakeven_trigger_r · R`), the stop
// trails `trail_frac_r · R` below the high-water mark for SUBSEQUENT bars, floored
// at entry — a runner that peaks well past the trigger then reverts books a PARTIAL
// win at the trailed stop, not just a scratch at breakeven. Off (`trail_frac_r`
// 0.0) is byte-identical to v23's flat breakeven move. The trail never engages
// before the ratchet arms, never applies on the bar that raised high_water (KTD2),
// only tightens, and never loosens below entry.
// ---------------------------------------------------------------------------

/// The whole mechanism: an armed runner peaks well past the trigger, then reverts.
/// With the trail on it books a Stop at the trailed level (a PARTIAL WIN above entry)
/// on a later bar — where v23's flat breakeven would have booked a scratch at entry.
#[test]
fn trail_books_a_partial_win_above_breakeven() {
    let mut p = OrbParams::default();
    p.breakeven_trigger_r = 0.5; // trigger = 62_000 + round(0.5 * 1500) = 62_750
    p.trail_frac_r = 0.25; // give-back = round(0.25 * 1500) = 375
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000); // R 1500, entry 62_000, target 63_500
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // Bar A peaks at 63_400 (arms: ≥ 62_750; below the 63_500 target), holds above the
    // stop → no exit. Trail sets the stop to 63_400 − 375 = 63_025 for later bars.
    assert!(bar(&mut st, t(9, 30), 63_400, 62_500, &p).is_empty());
    // Bar B dips to 63_000 ≤ 63_025 (the trailed stop) → Stop at the bar low 63_000 —
    // a partial win of (63_000 − 62_000)/1500 ≈ 0.67R, NOT a breakeven scratch.
    let acts = bar(&mut st, t(10, 0), 63_200, 63_000, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 63_000, reason: ExitReason::Stop }]);
    assert!(st.realized_exit_r(63_000) > 0.0, "the trail books a positive partial: {}", st.realized_exit_r(63_000));
}

/// The trail engages ONLY after the breakeven ratchet arms: a peak BELOW the
/// `breakeven_trigger_r · R` trigger neither arms breakeven nor trails, so a dip that
/// would hit a hypothetical trailed stop but stays above the range-low stop does not
/// exit. The trail rides on top of the kept trigger — never independently of it.
#[test]
fn trail_does_not_engage_before_breakeven_arms() {
    let mut p = OrbParams::default();
    p.breakeven_trigger_r = 0.5; // trigger 62_750
    p.trail_frac_r = 0.1; // a very tight trail — must still not engage un-armed
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // Peak 62_700 < 62_750 → the ratchet never arms, so the trail never engages.
    assert!(bar(&mut st, t(9, 30), 62_700, 62_000, &p).is_empty());
    // A dip to 62_100 — which a trail off the 62_700 peak WOULD have stopped — is above
    // the un-ratcheted 60_000 range-low stop, so no exit fires.
    assert!(bar(&mut st, t(10, 0), 62_400, 62_100, &p).is_empty());
    let acts = bar(&mut st, t(15, 0), 62_100, 62_000, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 62_000, reason: ExitReason::TimeFlat }]);
}

/// The trailed stop is floored at entry: when `high_water − round(trail_frac_r · R)`
/// sits below entry (a modest peak with a wide trail), the stop clamps to entry — the
/// flat breakeven of lever 6, never a stop loosened below breakeven.
#[test]
fn trail_floors_at_entry_never_below_breakeven() {
    let mut p = OrbParams::default();
    p.breakeven_trigger_r = 0.5; // trigger 62_750
    p.trail_frac_r = 0.6; // give-back = round(0.6 * 1500) = 900
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // Bar A peaks at 62_800 (arms). Trailed = 62_800 − 900 = 61_900 < entry 62_000 →
    // floored at entry 62_000 (flat breakeven), NOT loosened to 61_900.
    assert!(bar(&mut st, t(9, 30), 62_800, 62_100, &p).is_empty());
    // A dip to 61_950 (< the floored 62_000 stop, but > the un-floored 61_900) books a
    // Stop at breakeven — proving the floor held.
    let acts = bar(&mut st, t(10, 0), 62_400, 61_950, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 61_950, reason: ExitReason::Stop }]);
}

/// The trail only ever tightens: once it lifts the stop off a peak, a later bar with a
/// LOWER high does not fold a new high_water, so the trailed stop holds (never drifts
/// back down toward entry). A dip to the held level still books there.
#[test]
fn trail_only_tightens_never_loosens() {
    let mut p = OrbParams::default();
    p.breakeven_trigger_r = 0.5; // trigger 62_750
    p.trail_frac_r = 0.25; // give-back 375
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000); // R 1500, entry 62_000, target 63_500
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // Bar A peaks 63_400 (arms; < 63_500 target) → trailed stop 63_400 − 375 = 63_025.
    assert!(bar(&mut st, t(9, 30), 63_400, 62_500, &p).is_empty());
    // Bar B has a LOWER high (63_100, no new high_water) and holds above 63_025 → no
    // exit, and the trailed stop must NOT drift down.
    assert!(bar(&mut st, t(10, 0), 63_100, 63_050, &p).is_empty());
    // Bar C dips to 63_000 ≤ 63_025 → Stop at the still-held trailed level.
    let acts = bar(&mut st, t(10, 30), 63_100, 63_000, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 63_000, reason: ExitReason::Stop }]);
}

/// KTD2 same-bar: the trailed stop is NOT applied on the bar that raised high_water.
/// A bar prints a new peak AND a low below the would-be-trailed level in the same bar;
/// the low may precede the high, so no same-bar stop — the trail binds only next bar.
#[test]
fn trail_does_not_book_on_the_high_raising_bar() {
    let mut p = OrbParams::default();
    p.breakeven_trigger_r = 0.5; // trigger 62_750
    p.trail_frac_r = 0.25; // give-back 375
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // Bar A arms the ratchet on an earlier bar (peak 63_000 → trailed 63_000 − 375 =
    // 62_625, above entry).
    assert!(bar(&mut st, t(9, 30), 63_000, 62_500, &p).is_empty());
    // Bar B raises high_water to 63_400 (new trailed 63_025) AND dips to 62_700 in the
    // same bar. The stop check uses the PRIOR trailed 62_625 (62_700 > 62_625 → no
    // exit); the tighter 63_025 arms only AFTER, for later bars.
    assert!(bar(&mut st, t(10, 0), 63_400, 62_700, &p).is_empty());
    assert_eq!(st.phase(), Phase::Long, "no same-bar trailed stop-out on the high-raising bar");
    // The NEXT bar's dip to 63_000 ≤ 63_025 books the tightened trailed stop.
    let acts = bar(&mut st, t(10, 30), 63_100, 63_000, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 63_000, reason: ExitReason::Stop }]);
}

/// A tiny positive `trail_frac_r` whose rounded give-back is 0 is treated as no trail
/// (flat breakeven), not a peak-tight stop: the give-back rounds to 0, so the stop
/// stays at entry and a give-back trade books the breakeven scratch — never a stop
/// pinned at the high-water mark.
#[test]
fn trail_tiny_give_back_that_rounds_to_zero_is_flat_breakeven() {
    let mut p = OrbParams::default();
    p.breakeven_trigger_r = 0.5; // trigger 62_750
    p.trail_frac_r = 0.0001; // round(0.0001 * 1500) = round(0.15) = 0 → no trail
    assert!(p.validate().is_ok(), "a positive trail validates");
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // Bar A peaks 63_400 (arms). Give-back rounds to 0 → the stop is entry 62_000
    // (flat), NOT 63_400 (a peak-tight trail).
    assert!(bar(&mut st, t(9, 30), 63_400, 62_500, &p).is_empty());
    // A dip to 63_000 — which a peak-tight trail WOULD have stopped — is above the
    // flat-breakeven 62_000 stop, so no exit.
    assert!(bar(&mut st, t(10, 0), 63_200, 63_000, &p).is_empty());
    // A dip to entry books the breakeven scratch.
    let acts = bar(&mut st, t(10, 30), 62_100, 61_950, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 61_950, reason: ExitReason::Stop }]);
}

/// Off (`trail_frac_r == 0.0`) is byte-identical to v23's flat breakeven move: with
/// the trail off, an armed give-back trade books a Stop at entry (breakeven), exactly
/// as lever 6 alone — the trail arm changes nothing until it is switched on.
#[test]
fn trail_off_is_flat_breakeven_byte_identical() {
    let mut p = OrbParams::default();
    p.breakeven_trigger_r = 0.5; // trigger 62_750
    // trail_frac_r stays 0.0 (off)
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // Bar A peaks 63_400 (arms) — with the trail off the stop moves only to entry.
    assert!(bar(&mut st, t(9, 30), 63_400, 62_500, &p).is_empty());
    // A dip to 63_000 does NOT stop (the flat breakeven stop is entry 62_000, not a
    // trailed 63_025) — the exact v23 behavior.
    assert!(bar(&mut st, t(10, 0), 63_200, 63_000, &p).is_empty());
    // A dip to entry books the breakeven Stop.
    let acts = bar(&mut st, t(10, 30), 62_100, 61_950, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 61_950, reason: ExitReason::Stop }]);
}

/// `realized_exit_r` reports the booked R at a fill: positive above entry (a trailed
/// partial or target), ≈0 at breakeven, negative below (a stop-out). Pure telemetry,
/// guarded like `mfe_r` (0.0 before a range / entry / on a degenerate R).
#[test]
fn realized_exit_r_reports_booked_r() {
    let p = OrbParams::default();
    let mut st = OrbState::new();
    // Before a range: 0.0 (guarded).
    assert_eq!(st.realized_exit_r(62_000), 0.0);
    set_range(&mut st, &p, 61_500, 60_000); // R 1500
    assert_eq!(bar(&mut st, t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // entry 62_000, R 1500.
    assert!((st.realized_exit_r(62_000) - 0.0).abs() < 1e-9, "breakeven books ≈0");
    assert!((st.realized_exit_r(62_750) - 0.5).abs() < 1e-9, "a +750 fill books +0.5R");
    assert!((st.realized_exit_r(61_250) + 0.5).abs() < 1e-9, "a −750 fill books −0.5R");
}

/// AE5: ATR mode with no prior ATR fails closed at range fix — one recorded
/// `atr_unavailable` reject, done-for-day, never a silent range-low fallback.
#[test]
fn atr_stop_mode_fails_closed_without_atr() {
    let mut p = OrbParams::default();
    p.stop_mode = 2.0;
    let mut st = OrbState::with_priors(None, None, None); // ATR unavailable
    set_range(&mut st, &p, 61_500, 60_000);
    // The first trading-window bar fixes the range → the gate rejects immediately.
    let acts = st.on_bar(t(9, 20), 62_000, 61_000, 61_500, 0.0, &p);
    assert_eq!(
        acts,
        vec![OrbAction::SessionReject { filter: "atr_unavailable", values: vec![("atr_window", 14.0)] }]
    );
    assert_eq!(st.phase(), Phase::Done);
    // No entry ever — a later strong break takes no trade.
    assert!(st.on_bar(t(10, 0), 70_000, 69_000, 69_500, 0.0, &p).is_empty());
}

/// F1 flip-precondition: a non-positive prior ATR (`Some(0.0)` — flat / halted
/// priors dedup to zero range) is treated as unavailable in ATR stop mode, failing
/// closed at range fix rather than passing the gate and collapsing the stop onto
/// the entry (which would fabricate a same-bar full-range loss).
#[test]
fn atr_stop_mode_treats_zero_atr_as_unavailable() {
    let mut p = OrbParams::default();
    p.stop_mode = 2.0; // ATR
    let mut st = OrbState::with_priors(Some(0.0), None, None); // flat priors → ATR 0.0
    set_range(&mut st, &p, 61_500, 60_000);
    let acts = st.on_bar(t(9, 20), 62_000, 61_000, 61_500, 0.0, &p);
    assert_eq!(
        acts,
        vec![OrbAction::SessionReject { filter: "atr_unavailable", values: vec![("atr_window", 14.0)] }]
    );
    assert_eq!(st.phase(), Phase::Done);
}

/// OR-width/ATR decouple (code turn): the OR-width gate is genuinely OPTIONAL for a
/// session that lacks a positive prior ATR — a non-positive ATR (`Some(0.0)` from
/// flat / halted priors) SKIPS the width gate and lets the session proceed, rather
/// than failing closed as `atr_unavailable`. This isolates the width signal from the
/// ATR-coverage cull (lever 3's confound); the ATR-STOP arm keeps its fail-closed
/// reject (a stop needs its ATR — see `atr_stop_mode_treats_zero_atr_as_unavailable`).
#[test]
fn or_width_gate_skips_when_atr_non_positive() {
    let mut p = OrbParams::default();
    p.or_width_max_atr = 5.0; // gate on, but no positive ATR to normalize against
    let mut st = OrbState::with_priors(Some(0.0), None, None); // flat priors → ATR 0.0
    set_range(&mut st, &p, 61_500, 60_000); // a wide range — would fail a live width gate
    assert_eq!(
        st.on_bar(t(9, 20), 62_000, 61_000, 61_500, 0.0, &p),
        vec![OrbAction::Enter { limit_price: 62_000 }],
        "no positive ATR → width gate skipped, session proceeds"
    );
    assert_eq!(st.phase(), Phase::Long);
}

/// F1 flip-precondition: a tiny-but-positive ATR whose `stop_atr_mult · ATR`
/// rounds to 0 has its stop distance floored at 1, so the stop sits one tick below
/// entry (trade-R = 1) instead of collapsing onto it. Without the floor the stop
/// would equal the entry (`dist = 0`), zeroing trade-R (no target) and forcing a
/// guaranteed same-bar stop-out at the bar low. Verified via a flat entry bar (the
/// only wick-mode bar a one-tick stop survives): the position enters clean and a
/// later bar reaches the now-defined one-tick target.
#[test]
fn atr_stop_distance_floored_at_one_never_collapses_onto_entry() {
    let mut p = OrbParams::default();
    p.stop_mode = 2.0; // ATR
    p.stop_atr_mult = 2.0;
    // ATR 0.1 → 2 × 0.1 = 0.2 → round = 0 → floored to 1 (dist = 1 tick).
    let mut st = OrbState::with_priors(Some(0.1), None, None);
    set_range(&mut st, &p, 61_500, 61_000);
    // A flat entry bar (high = low = close = 62_000) breaks the range high. Stop =
    // max(62_000 − 1, 61_000) = 61_999; the entry bar low 62_000 > 61_999 → no
    // phantom same-bar stop. Without the floor: stop = 62_000 = entry → same-bar stop.
    assert_eq!(
        st.on_bar(t(9, 20), 62_000, 62_000, 62_000, 0.0, &p),
        vec![OrbAction::Enter { limit_price: 62_000 }]
    );
    assert_eq!(st.phase(), Phase::Long, "floored stop leaves the entry open");
    // trade-R = 1 → target = 62_000 + round(1.0 × 1) = 62_001, now reachable (a
    // collapsed stop would give r_denom 0 and no target at all). The next bar's low
    // (62_000) stays above the 61_999 stop, so the target — not a stop — fires.
    let acts = st.on_bar(t(9, 30), 62_500, 62_000, 62_400, 0.0, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 62_001, reason: ExitReason::Target }]);
}

/// AE4: close-confirmed entry — a wick above the range high whose CLOSE is at or
/// inside it does NOT enter (stays Armed); a later bar closing strictly above does,
/// at that close, with the high-water seeded at the close (the wick not folded).
#[test]
fn close_confirm_entry_waits_for_close_above_range() {
    let mut p = OrbParams::default();
    p.entry_confirm = 1.0; // close-confirmed
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    // Wick above the range high (62_000) but close inside (61_400 ≤ 61_500) → no
    // entry, still Armed.
    assert!(st.on_bar(t(9, 20), 62_000, 61_000, 61_400, 0.0, &p).is_empty());
    assert_eq!(st.phase(), Phase::Armed);
    // A later bar closes strictly above the range high → enter at that close.
    let acts = st.on_bar(t(9, 30), 62_400, 61_600, 61_800, 0.0, &p);
    assert_eq!(acts, vec![OrbAction::Enter { limit_price: 61_800 }]);
    assert_eq!(st.entry_price(), 61_800, "entry at the confirm close, not the wick high");
    // The confirm bar's above-close wick (62_400) is NOT folded into MFE.
    // range-R stop mode 0 → R = 1500; mfe from a later flat bar stays at entry.
    assert!(st.on_bar(t(9, 40), 61_800, 61_700, 61_750, 0.0, &p).is_empty());
    assert_eq!(st.mfe_r(), 0.0, "wick not folded → no excursion above the close entry");
}

/// F2 flip-precondition: a close-confirm bar whose low touches the stop does NOT
/// book a same-bar stop. The fill is anchored at the bar close (the bar's last
/// event), so the stop-touching low printed BEFORE the position existed — it is
/// provably pre-fill. This deliberately deviates from KTD6's wick-entry
/// "same-bar stop-first wins": the low is only foldable in wick mode, where the
/// fill sits mid-bar. The stop still binds from the next bar onward.
#[test]
fn close_confirm_skips_same_bar_pre_fill_stop() {
    let mut p = OrbParams::default();
    p.entry_confirm = 1.0;
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    // Close 61_800 > 61_500 confirms entry; low 59_900 ≤ 60_000 would breach the
    // stop, but that low is pre-fill in close-confirm mode → no same-bar stop.
    let acts = st.on_bar(t(9, 20), 62_000, 59_900, 61_800, 0.0, &p);
    assert_eq!(acts, vec![OrbAction::Enter { limit_price: 61_800 }]);
    assert_eq!(st.phase(), Phase::Long, "position stays open — the low was pre-fill");
    // From the NEXT bar the stop binds normally: a low ≤ the range-low stop exits.
    let acts = st.on_bar(t(9, 30), 61_000, 59_800, 60_500, 0.0, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 59_800, reason: ExitReason::Stop }]);
    assert_eq!(st.phase(), Phase::Done);
}

/// F2 value path: a close-confirm entry bar whose low breaches the stop is NOT
/// stopped same-bar (the low is pre-fill), and the position is then free to run to
/// the TARGET on a later bar — the winner that wick mode (or the old close-confirm
/// stop-first) would have booked as a loss. This is the mechanism behind the flip's
/// expectancy lift: F2 converts phantom same-bar stop-outs into surviving trades.
#[test]
fn close_confirm_carried_entry_can_reach_target() {
    let mut p = OrbParams::default();
    p.entry_confirm = 1.0; // close-confirmed
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    // Confirm entry at close 61_800; low 59_900 ≤ range-low stop 60_000 is pre-fill
    // → skipped. Old behavior (wick / stop-first) would exit Stop at 59_900 here.
    assert_eq!(
        st.on_bar(t(9, 20), 62_000, 59_900, 61_800, 0.0, &p),
        vec![OrbAction::Enter { limit_price: 61_800 }]
    );
    assert_eq!(st.phase(), Phase::Long);
    // Range-low stop mode → r_denom = range-R = 1_500; target = 61_800 + 1_500 =
    // 63_300. A later bar rises to the target (low stays above the 60_000 stop) →
    // the carried-through trade banks a WIN, not the phantom same-bar loss.
    let acts = st.on_bar(t(9, 30), 63_400, 61_000, 63_200, 0.0, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 63_300, reason: ExitReason::Target }]);
    assert_eq!(st.phase(), Phase::Done);
}

/// Wick-touch entry is unchanged by the F2 close-confirm carve-out: a wick-mode
/// bar that both breaks the range high and breaches the stop still resolves
/// Stop-first same-bar (the fill sits mid-bar, so a lower tick can follow it).
#[test]
fn wick_entry_still_stops_first_same_bar() {
    let p = OrbParams::default(); // entry_confirm 0.0 → wick-touch
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    // High 62_000 breaks the range; low 59_900 ≤ 60_000 breaches the stop same bar.
    let acts = st.on_bar(t(9, 20), 62_000, 59_900, 61_800, 0.0, &p);
    assert_eq!(
        acts,
        vec![
            OrbAction::Enter { limit_price: 62_000 },
            OrbAction::Exit { limit_price: 59_900, reason: ExitReason::Stop },
        ]
    );
    assert_eq!(st.phase(), Phase::Done);
}

// ---------------------------------------------------------------------------
// U4 entry-quality gates: OR-width, RVOL, cutoff (KTD7/KTD10). Each off (default)
// is a no-op; on, each rejects done-for-day with one canonical recorded filter.
// ---------------------------------------------------------------------------

/// Drive the opening-range window with a per-bar volume so the RVOL numerator
/// (today's opening-window volume) accumulates.
fn set_range_vol(st: &mut OrbState, p: &OrbParams, high: i64, low: i64, vol_per_bar: f64) {
    st.on_bar(t(9, 0), high, low, high, vol_per_bar, p);
    st.on_bar(t(9, 10), high, low, high, vol_per_bar, p);
}

#[test]
fn or_width_gate_off_is_a_no_op() {
    let p = OrbParams::default(); // or_width_max_atr 0.0
    let mut st = OrbState::with_priors(Some(100.0), None, None);
    set_range(&mut st, &p, 61_500, 60_000); // a wide range
    assert_eq!(
        st.on_bar(t(9, 20), 62_000, 61_000, 61_500, 0.0, &p),
        vec![OrbAction::Enter { limit_price: 62_000 }],
        "gate off → enters regardless of width"
    );
}

#[test]
fn or_width_gate_rejects_a_wide_range() {
    let mut p = OrbParams::default();
    p.or_width_max_atr = 5.0; // range-R must be ≤ 5 × ATR = 500
    let mut st = OrbState::with_priors(Some(100.0), None, None);
    set_range(&mut st, &p, 61_500, 60_000); // range-R 1500 > 500
    let acts = st.on_bar(t(9, 20), 62_000, 61_000, 61_500, 0.0, &p);
    assert_eq!(
        acts,
        vec![OrbAction::SessionReject {
            filter: "or_width_atr",
            values: vec![("range_r", 1500.0), ("atr", 100.0), ("or_width_max_atr", 5.0)],
        }]
    );
    assert_eq!(st.phase(), Phase::Done);
}

#[test]
fn or_width_gate_passes_a_tight_range() {
    let mut p = OrbParams::default();
    p.or_width_max_atr = 5.0; // threshold 500
    let mut st = OrbState::with_priors(Some(100.0), None, None);
    set_range(&mut st, &p, 60_300, 60_000); // range-R 300 ≤ 500 → pass
    assert_eq!(
        st.on_bar(t(9, 20), 60_800, 60_100, 60_500, 0.0, &p),
        vec![OrbAction::Enter { limit_price: 60_800 }]
    );
}

/// OR-width/ATR decouple (code turn): a session with NO prior ATR (`None`) SKIPS the
/// width gate and proceeds — the gate is optional when there is no ATR to normalize
/// against. (Before the decouple this failed closed as `atr_unavailable`, which
/// conflated "no ATR history" with "too-wide range" and swamped lever 3's clean width
/// signal with a winner-rich coverage cull.)
#[test]
fn or_width_gate_skips_when_atr_missing() {
    let mut p = OrbParams::default();
    p.or_width_max_atr = 5.0;
    let mut st = OrbState::with_priors(None, None, None); // ATR unavailable
    set_range(&mut st, &p, 61_500, 60_000);
    assert_eq!(
        st.on_bar(t(9, 20), 62_000, 61_000, 61_500, 0.0, &p),
        vec![OrbAction::Enter { limit_price: 62_000 }],
        "missing ATR → width gate skipped, session proceeds"
    );
    assert_eq!(st.phase(), Phase::Long);
}

#[test]
fn rvol_gate_off_is_a_no_op() {
    let p = OrbParams::default(); // rvol_min 0.0
    let mut st = OrbState::with_priors(None, Some(1_000.0), None);
    set_range_vol(&mut st, &p, 61_500, 60_000, 1.0); // trivial volume
    assert_eq!(
        st.on_bar(t(9, 20), 62_000, 61_000, 61_500, 0.0, &p),
        vec![OrbAction::Enter { limit_price: 62_000 }]
    );
}

#[test]
fn rvol_gate_rejects_low_participation() {
    let mut p = OrbParams::default();
    p.rvol_min = 1.0;
    let mut st = OrbState::with_priors(None, Some(1_000.0), None); // prior mean 1000
    set_range_vol(&mut st, &p, 61_500, 60_000, 400.0); // today 800 < 1.0 × 1000
    let acts = st.on_bar(t(9, 20), 62_000, 61_000, 61_500, 0.0, &p);
    assert_eq!(
        acts,
        vec![OrbAction::SessionReject {
            filter: "rvol_min",
            values: vec![("open_window_vol", 800.0), ("prior_open_vol_mean", 1_000.0), ("rvol_min", 1.0)],
        }]
    );
}

#[test]
fn rvol_gate_passes_high_participation() {
    let mut p = OrbParams::default();
    p.rvol_min = 1.0;
    let mut st = OrbState::with_priors(None, Some(1_000.0), None);
    set_range_vol(&mut st, &p, 61_500, 60_000, 600.0); // today 1200 ≥ 1000
    assert_eq!(
        st.on_bar(t(9, 20), 62_000, 61_000, 61_500, 0.0, &p),
        vec![OrbAction::Enter { limit_price: 62_000 }]
    );
}

#[test]
fn rvol_gate_fails_closed_without_history() {
    let mut p = OrbParams::default();
    p.rvol_min = 1.0;
    let mut st = OrbState::with_priors(None, None, None); // no prior mean
    set_range_vol(&mut st, &p, 61_500, 60_000, 600.0);
    let acts = st.on_bar(t(9, 20), 62_000, 61_000, 61_500, 0.0, &p);
    assert_eq!(
        acts,
        vec![OrbAction::SessionReject {
            filter: "rvol_insufficient_history",
            values: vec![("rvol_min_history", 5.0)],
        }]
    );
}

#[test]
fn rvol_gate_fails_closed_on_zero_prior_mean() {
    let mut p = OrbParams::default();
    p.rvol_min = 1.0;
    let mut st = OrbState::with_priors(None, Some(0.0), None); // zero prior mean
    set_range_vol(&mut st, &p, 61_500, 60_000, 600.0);
    let acts = st.on_bar(t(9, 20), 62_000, 61_000, 61_500, 0.0, &p);
    assert_eq!(
        acts,
        vec![OrbAction::SessionReject {
            filter: "rvol_insufficient_history",
            values: vec![("rvol_min_history", 5.0)],
        }],
        "a zero prior mean fails closed, never divides"
    );
}

#[test]
fn gate_order_records_or_width_before_rvol() {
    let mut p = OrbParams::default();
    p.or_width_max_atr = 5.0; // fails: wide range
    p.rvol_min = 1.0; // would also fail: low volume
    let mut st = OrbState::with_priors(Some(100.0), Some(1_000.0), None); // threshold 500
    set_range_vol(&mut st, &p, 61_500, 60_000, 100.0); // today 200 < 1000 too
    let acts = st.on_bar(t(9, 20), 62_000, 61_000, 61_500, 0.0, &p);
    assert_eq!(acts.len(), 1, "one canonical filter, not both");
    assert!(
        matches!(&acts[0], OrbAction::SessionReject { filter, .. } if *filter == "or_width_atr"),
        "the first failing gate (OR-width) wins: {:?}",
        acts[0]
    );
}

#[test]
fn entry_cutoff_rejects_at_or_after_the_cutoff_time() {
    let mut p = OrbParams::default();
    p.entry_cutoff_min = 30.0; // cutoff 09:30
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    // A breakout bar exactly at the cutoff (09:30) is rejected (≥ boundary).
    let acts = st.on_bar(t(9, 30), 62_000, 61_000, 61_500, 0.0, &p);
    assert_eq!(
        acts,
        vec![OrbAction::SessionReject { filter: "entry_cutoff", values: vec![("entry_cutoff_min", 30.0)] }]
    );
    assert_eq!(st.phase(), Phase::Done);
    // Done thereafter — one envelope, never one per bar.
    assert!(st.on_bar(t(9, 40), 62_500, 61_000, 62_000, 0.0, &p).is_empty());
    assert!(st.on_bar(t(10, 0), 63_000, 61_000, 62_500, 0.0, &p).is_empty());
}

#[test]
fn entry_before_cutoff_enters_and_open_position_is_untouched() {
    let mut p = OrbParams::default();
    p.entry_cutoff_min = 30.0; // cutoff 09:30
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    // 09:20 < 09:30 → a normal entry.
    assert_eq!(
        st.on_bar(t(9, 20), 62_000, 61_000, 61_500, 0.0, &p),
        vec![OrbAction::Enter { limit_price: 62_000 }]
    );
    // The open long is untouched by the cutoff — it exits at the time-flat bell.
    let acts = st.on_bar(t(15, 0), 62_200, 62_000, 62_100, 0.0, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 62_000, reason: ExitReason::TimeFlat }]);
}

// ---------------------------------------------------------------------------
// Breakout strength (Turn 10 / v12 band-pass, R2 / KTD6)
// ---------------------------------------------------------------------------

/// Strength `= (breakout_price − range_high) / R`. For the range [62500, 63500]
/// (R = 1000), a breakout bar high of 64000 is strength 0.5.
#[test]
fn breakout_strength_is_break_over_range() {
    assert_eq!(breakout_strength(64_000, 63_500, 62_500), Some(0.5));
    // A marginal break just above the high is near-zero strength (a q1/q2 entry).
    let s = breakout_strength(63_540, 63_500, 62_500).unwrap();
    assert!((s - 0.04).abs() < 1e-9, "strength = {s}");
    // A real break is always strictly positive (breakout_price > range_high).
    assert!(breakout_strength(63_501, 63_500, 62_500).unwrap() > 0.0);
}

/// KTD6: a degenerate range (`R ≤ 0`) yields `None` — no division — so the
/// caller bypasses the band-pass filter and preserves legacy entry.
#[test]
fn breakout_strength_none_on_degenerate_range() {
    assert_eq!(breakout_strength(64_000, 63_500, 63_500), None, "R == 0 → None");
    assert_eq!(breakout_strength(64_000, 63_000, 63_500), None, "R < 0 → None");
}

// ---------------------------------------------------------------------------
// Universe scan
// ---------------------------------------------------------------------------

fn candidate(sym: &str, prior_close: i64, today_open: i64, turnover: f64) -> UniverseCandidate {
    UniverseCandidate {
        symbol: sym.to_string(),
        gap_prices: SessionGapPrices::new(prior_close, today_open),
        prior_turnover: turnover,
        meta: CandidateMeta::Untagged,
        prior_atr: None,
        prior_open_vol_mean: None,
        prior_illiq: None,
    }
}

/// AE2: a candidate failing the gap filter produces a rejection envelope naming the
/// filter and carrying the signal values at decision time.
#[test]
fn gap_reject_names_filter_and_values() {
    let p = OrbParams::default(); // gap_min_pct 3.0
    let sink = DecisionSink::new();
    // 60000 → 60500 is +0.83%, below the 3% gap floor.
    let cands = vec![candidate("005930.XKRX", 60_000, 60_500, 1_000.0)];
    let selected = select_universe(&cands, &p, &sink, 42);
    assert!(selected.is_empty(), "a sub-gap candidate is not selected");

    let envelopes = sink.snapshot();
    assert_eq!(envelopes.len(), 1, "one envelope per decision");
    let e = &envelopes[0];
    assert_eq!(e.ts_event, 42);
    assert!(
        matches!(e.trigger, DecisionTrigger::StateChange { .. }),
        "universe decisions trigger on the scan state change: {:?}",
        e.trigger
    );
    let d = e.decision_detail.as_ref().expect("telemetry envelope carries the detail");
    assert_eq!(d.kind, SignalKind::Universe);
    assert_eq!(d.symbol, "005930.XKRX");
    assert_eq!(d.decision, Some(Decision::Reject));
    assert_eq!(d.filter.as_deref(), Some("gap"));
    let gap = d.values.get("gap_pct").copied().unwrap();
    assert!((gap - 0.8333).abs() < 0.01, "gap_pct recorded: {gap}");
}

/// The universe cap (top-N) is enforced when more candidates qualify, and the
/// survivors are ranked by prior-session turnover.
#[test]
fn universe_caps_top_n_by_turnover() {
    let p = OrbParams::default(); // universe_top_n defaults to 20
    let sink = DecisionSink::new();
    // 25 candidates all clearing the gap (+5%), with turnover = index so ranking is
    // unambiguous.
    let cands: Vec<UniverseCandidate> = (0..25)
        .map(|i| candidate(&format!("{:06}.XKRX", i), 100, 105, i as f64))
        .collect();
    let selected = select_universe(&cands, &p, &sink, 1);
    assert_eq!(selected.len(), 20, "top-20 cap enforced");
    // The highest-turnover candidate (index 24) ranks first.
    assert_eq!(selected[0], "000024.XKRX");
    assert_eq!(selected[19], "000005.XKRX");

    // The five lowest-turnover candidates are rejected by the turnover-rank filter.
    let rank_rejects = sink
        .snapshot()
        .into_iter()
        .filter(|e| {
            e.decision_detail.as_ref().and_then(|d| d.filter.as_deref()) == Some("turnover_rank")
        })
        .count();
    assert_eq!(rank_rejects, 5);
}

// ---------------------------------------------------------------------------
// Metadata-gated selection (plan 2026-07-10-003, U4)
// ---------------------------------------------------------------------------

use nautilus_ls::reference::universe_metadata::{
    CapTier, ConditionerTags, IndexMembership, LiquidityTier, MarketClass, Resolved, Stratum,
};

fn tags(market: MarketClass, cap: CapTier) -> ConditionerTags {
    ConditionerTags {
        cap_tier: cap,
        // Unknown mirrors a record whose capture-time turnover is Unavailable
        // (the t1463 walk is deferred this turn, R2).
        liquidity_tier: LiquidityTier::Unknown,
        market_class: market,
        index_membership: Resolved::Proxy(IndexMembership::NotMember),
        has_derivative: Resolved::Value(false),
    }
}

fn tagged(
    sym: &str,
    today_open: i64,
    turnover: f64,
    tradable: bool,
    market: MarketClass,
    cap: CapTier,
) -> UniverseCandidate {
    UniverseCandidate {
        symbol: sym.to_string(),
        gap_prices: SessionGapPrices::new(100, today_open),
        prior_turnover: turnover,
        meta: CandidateMeta::Tagged { tradable, tags: tags(market, cap) },
        prior_atr: None,
        prior_open_vol_mean: None,
        prior_illiq: None,
    }
}

/// Covers AE3: a non-tradable (designated) symbol is excluded even when its gap
/// and turnover qualify, with the rejection naming the gate.
#[test]
fn non_tradable_candidate_is_excluded_even_when_gap_and_turnover_qualify() {
    let p = OrbParams::default(); // gap_min_pct 3.0
    let sink = DecisionSink::new();
    let cands = vec![
        tagged("111111.XKRX", 105, 9_999_999.0, false, MarketClass::Kospi, CapTier::Top),
        tagged("222222.XKRX", 105, 1_000.0, true, MarketClass::Kospi, CapTier::Top),
    ];
    let selected = select_universe(&cands, &p, &sink, 1);
    assert_eq!(selected, vec!["222222.XKRX".to_string()], "only the clean symbol is selected");
    let rejects: Vec<_> = sink
        .snapshot()
        .into_iter()
        .filter_map(|e| e.decision_detail)
        .filter(|d| d.filter.as_deref() == Some("not_tradable"))
        .collect();
    assert_eq!(rejects.len(), 1);
    assert_eq!(rejects[0].symbol, "111111.XKRX");
    assert_eq!(rejects[0].decision, Some(Decision::Reject));
}

/// R5: the liquidity floor excludes a below-floor candidate and admits one at
/// the floor; the floor evaluates the daily-bar prior_turnover.
#[test]
fn below_floor_candidate_is_excluded_and_at_floor_passes() {
    let mut p = OrbParams::default();
    p.turnover_floor_krw = 1_000.0;
    let sink = DecisionSink::new();
    let cands = vec![
        tagged("111111.XKRX", 105, 999.0, true, MarketClass::Kosdaq, CapTier::Mid),
        tagged("222222.XKRX", 105, 1_000.0, true, MarketClass::Kosdaq, CapTier::Mid),
    ];
    let selected = select_universe(&cands, &p, &sink, 1);
    assert_eq!(selected, vec!["222222.XKRX".to_string()], "at-floor passes, below-floor is cut");
    let reject = sink
        .snapshot()
        .into_iter()
        .filter_map(|e| e.decision_detail)
        .find(|d| d.filter.as_deref() == Some("turnover_floor"))
        .expect("floor rejection recorded");
    assert_eq!(reject.symbol, "111111.XKRX");
    assert_eq!(reject.values.get("turnover_floor_krw").copied(), Some(1_000.0));
}

/// R5: a candidate whose CAPTURE-time turnover was Unavailable (liquidity tier
/// Unknown) is floor-gated on its daily-bar prior_turnover — admitted when that
/// clears the floor, never dropped for the unresolved capture attribute.
#[test]
fn unavailable_capture_turnover_gates_on_daily_bar_prior_turnover() {
    let mut p = OrbParams::default();
    p.turnover_floor_krw = 1_000.0;
    let sink = DecisionSink::new();
    // Unknown liquidity tier (capture turnover Unavailable) but a healthy
    // daily-bar prior_turnover → selected.
    let cands =
        vec![tagged("300001.XKRX", 105, 5_000.0, true, MarketClass::Kosdaq, CapTier::BelowBoard)];
    let selected = select_universe(&cands, &p, &sink, 1);
    assert_eq!(selected, vec!["300001.XKRX".to_string()]);
}

/// R4: a candidate the artifact does not cover is non-selectable and recorded,
/// never silently defaulted into the tradeable set.
#[test]
fn missing_metadata_candidate_is_dropped_and_recorded() {
    let p = OrbParams::default();
    let sink = DecisionSink::new();
    let mut c = candidate("111111.XKRX", 100, 105, 9_999.0);
    c.meta = CandidateMeta::Missing;
    let selected = select_universe(&[c], &p, &sink, 1);
    assert!(selected.is_empty(), "missing metadata is non-selectable");
    let d = sink.snapshot()[0].decision_detail.clone().unwrap();
    assert_eq!(d.filter.as_deref(), Some("missing_metadata"));
    assert_eq!(d.decision, Some(Decision::Reject));
}

/// The gap + turnover-rank ordering is preserved within the gated set: the
/// metadata gate removes candidates but never reorders the survivors.
#[test]
fn gap_and_turnover_ranking_preserved_within_the_filtered_set() {
    let mut p = OrbParams::default();
    p.universe_top_n = 2;
    let sink = DecisionSink::new();
    let cands = vec![
        // Highest turnover but designated — gated out before ranking.
        tagged("111111.XKRX", 105, 10_000.0, false, MarketClass::Kospi, CapTier::Top),
        tagged("222222.XKRX", 105, 3_000.0, true, MarketClass::Kospi, CapTier::Top),
        tagged("333333.XKRX", 105, 5_000.0, true, MarketClass::Kospi, CapTier::Mid),
        tagged("444444.XKRX", 105, 1_000.0, true, MarketClass::Kosdaq, CapTier::Mid),
    ];
    let selected = select_universe(&cands, &p, &sink, 1);
    assert_eq!(
        selected,
        vec!["333333.XKRX".to_string(), "222222.XKRX".to_string()],
        "survivors rank by prior_turnover, top-N capped"
    );
}

/// R9/KTD4: the accept envelope carries the full conditioner-tag set and the
/// implied stratum; a legacy (untagged) accept carries none.
#[test]
fn accept_envelope_carries_conditioner_tags_and_the_correct_tier() {
    let p = OrbParams::default();
    let sink = DecisionSink::new();
    let cands = vec![
        tagged("111111.XKRX", 105, 5_000.0, true, MarketClass::Kospi, CapTier::Top),
        candidate("222222.XKRX", 100, 105, 1_000.0), // legacy Untagged
    ];
    let selected = select_universe(&cands, &p, &sink, 1);
    assert_eq!(selected.len(), 2);
    let accepts: Vec<_> = sink
        .snapshot()
        .into_iter()
        .filter_map(|e| e.decision_detail)
        .filter(|d| d.decision == Some(Decision::Accept))
        .collect();
    let tagged_accept = accepts.iter().find(|d| d.symbol == "111111.XKRX").unwrap();
    let t = tagged_accept.tags.as_ref().expect("accept carries the tag set");
    assert_eq!(t.cap_tier, CapTier::Top);
    assert_eq!(t.market_class, MarketClass::Kospi);
    assert_eq!(t.liquidity_tier, LiquidityTier::Unknown);
    assert_eq!(t.index_membership, Resolved::Proxy(IndexMembership::NotMember));
    assert_eq!(t.has_derivative, Resolved::Value(false));
    assert_eq!(t.stratum(), Stratum::KospiBlueChip, "the tier the report buckets on");
    let legacy_accept = accepts.iter().find(|d| d.symbol == "222222.XKRX").unwrap();
    assert!(legacy_accept.tags.is_none(), "a legacy run carries no tags");
}

// ---------------------------------------------------------------------------
// Transaction-cost model (orb-transaction-cost-model)
// ---------------------------------------------------------------------------

mod transaction_costs {
    use nautilus_ls_lab::params::OrbParams;
    use nautilus_ls_lab::strategy::orb::{TransactionCostConfig, TransactionCostModel};

    /// The tax is sell-side by statute: a buy fill pays commission only, a sell fill
    /// pays commission + tax. A symmetric per-side model would misprice every round
    /// trip by the tax on the buy leg.
    #[test]
    fn fill_cost_is_sell_side_asymmetric() {
        let m = TransactionCostModel { commission_rate_per_side: 0.00015, sell_tax_rate: 0.0020 };
        let notional = 10_000_000.0;
        assert_eq!(m.fill_cost(false, notional), 0.00015 * notional, "buy: commission only");
        assert_eq!(
            m.fill_cost(true, notional),
            (0.00015 + 0.0020) * notional,
            "sell: commission + statutory tax"
        );
    }

    /// Zero rates are the zero-cost sentinel: `from_params` yields no model at all,
    /// so a zero-rate run takes the pre-model code path untouched (byte-identical
    /// artifacts — the historical-reproducibility contract).
    #[test]
    fn zero_rates_yield_no_model() {
        let p = OrbParams::default();
        assert_eq!(p.cost_commission_rate_per_side, 0.0);
        assert_eq!(p.cost_sell_tax_rate, 0.0);
        assert!(TransactionCostModel::from_params(&p).is_none(), "0.0/0.0 → None");
        let armed = OrbParams { cost_sell_tax_rate: 0.0020, ..OrbParams::default() };
        let m = TransactionCostModel::from_params(&armed).expect("one nonzero rate arms the model");
        assert_eq!(m.sell_tax_rate, 0.0020);
        assert_eq!(m.commission_rate_per_side, 0.0);
    }

    /// A zero-rate model (constructed directly, not via `from_params`) charges
    /// exactly nothing — rate=0 must reproduce the zero-cost number exactly.
    #[test]
    fn zero_rate_model_charges_nothing() {
        let m = TransactionCostModel { commission_rate_per_side: 0.0, sell_tax_rate: 0.0 };
        assert_eq!(m.fill_cost(false, 5_000_000.0), 0.0);
        assert_eq!(m.fill_cost(true, 5_000_000.0), 0.0);
    }

    /// The config artifact loader schema-gates and surfaces parse errors with the
    /// path; an unsupported schema_version is refused.
    #[test]
    fn config_loader_schema_gates() {
        let dir = tempfile::TempDir::new().unwrap();
        let ok = dir.path().join("ok.json");
        std::fs::write(
            &ok,
            br#"{"schema_version":1,"commission_rate_per_side":0.00015,"sell_tax_rate":0.0020,"sources":[],"notes":[]}"#,
        )
        .unwrap();
        let cfg = TransactionCostConfig::load(&ok).unwrap();
        assert_eq!(cfg.commission_rate_per_side, 0.00015);
        assert_eq!(cfg.sell_tax_rate, 0.0020);

        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, br#"{"schema_version":2,"commission_rate_per_side":0.0,"sell_tax_rate":0.0}"#).unwrap();
        let err = TransactionCostConfig::load(&bad).unwrap_err();
        assert!(err.contains("schema_version"), "{err}");

        let missing = dir.path().join("missing.json");
        let err = TransactionCostConfig::load(&missing).unwrap_err();
        assert!(err.contains("missing.json"), "the path rides in the error: {err}");
    }

    /// The COMMITTED rate artifact stays loadable and carries the cited 2026 rates:
    /// LS증권 xing/OPEN API commission 0.015%/side and the uniform 20 bps sell-side
    /// tax (KOSPI 0.05% 거래세 + 0.15% 농특세; KOSDAQ 0.20% 거래세). Applied to params,
    /// the rates must pass `validate()` — the artifact and the plausibility gate can
    /// never drift apart silently.
    #[test]
    fn committed_config_artifact_parses_and_validates() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config/transaction-costs.json");
        let cfg = TransactionCostConfig::load(&path).expect("committed artifact loads");
        assert_eq!(cfg.commission_rate_per_side, 0.00015, "LS증권 xing API KRX 0.015%/side");
        assert_eq!(cfg.sell_tax_rate, 0.0020, "2026 sell-side 20 bps (both boards)");
        let p = OrbParams {
            cost_commission_rate_per_side: cfg.commission_rate_per_side,
            cost_sell_tax_rate: cfg.sell_tax_rate,
            ..OrbParams::default()
        };
        p.validate().expect("committed rates pass the plausibility gate");
    }
}
