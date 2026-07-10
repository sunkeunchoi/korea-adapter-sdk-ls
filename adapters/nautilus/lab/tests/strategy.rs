//! U3 — ORB v0 strategy tests. Pure, offline, zero network: the universe scan and
//! the range/entry/exit state machine are exercised in isolation of the engine
//! (the state machine is the unit most likely to hide off-by-one session-time bugs,
//! so it is built and tested test-first).

use chrono::NaiveTime;
use nautilus_ls_lab::agent::envelope::{Decision, DecisionTrigger, SignalKind};
use nautilus_ls_lab::agent::sink::DecisionSink;
use nautilus_ls_lab::params::OrbParams;
use nautilus_ls_lab::strategy::orb::{
    select_universe, ExitReason, OrbAction, OrbState, Phase, UniverseCandidate,
};

fn t(h: u32, m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, 0).unwrap()
}

/// Drive the opening-range window (09:00–09:14) so a range of [low..high] is fixed.
fn set_range(st: &mut OrbState, params: &OrbParams, high: i64, low: i64) {
    assert!(st.on_bar(t(9, 0), high, low, params).is_empty());
    assert!(st.on_bar(t(9, 10), high, low, params).is_empty());
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
    let acts = st.on_bar(t(9, 20), 62_000, 61_000, &p);
    assert_eq!(acts, vec![OrbAction::Enter { limit_price: 62_000 }]);
    assert_eq!(st.phase(), Phase::Long);

    // A later bar that stays above the stop → no action.
    assert!(st.on_bar(t(10, 0), 62_500, 61_800, &p).is_empty());

    // The time-flat deadline closes the position at a marketable limit.
    let acts = st.on_bar(t(15, 0), 62_200, 62_000, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 62_000, reason: ExitReason::TimeFlat }]);
    assert_eq!(st.phase(), Phase::Done);
}

/// The stop (range low) is honored: after entry, a bar that breaches the range low
/// exits with reason Stop.
#[test]
fn stop_exit_fires_on_range_low_breach() {
    let p = OrbParams::default();
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    assert_eq!(st.on_bar(t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // Next bar breaches the stop → marketable sell at the breaching bar's low.
    let acts = st.on_bar(t(9, 30), 61_800, 59_900, &p);
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
    assert!(st.on_bar(t(10, 0), 61_400, 60_500, &p).is_empty());
    assert!(st.on_bar(t(12, 0), 61_499, 60_800, &p).is_empty());
    // Time-flat with no position → no exit.
    assert!(st.on_bar(t(15, 0), 61_200, 60_900, &p).is_empty());
    assert_eq!(st.phase(), Phase::Done);
}

/// A whipsaw bar (breaks the range high AND breaches the range low) enters then stops
/// out in the same bar sequence.
#[test]
fn whipsaw_bar_enters_then_stops_same_bar() {
    let p = OrbParams::default();
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    let acts = st.on_bar(t(9, 20), 62_000, 59_000, &p);
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
    assert!(st.on_bar(t(9, 20), 62_000, 61_000, &p).is_empty());
    assert_eq!(st.phase(), Phase::Done);
    assert_eq!(st.range(), None);
}

/// The session summary reports the extreme values observed.
#[test]
fn session_extremes_track_high_and_low() {
    let p = OrbParams::default();
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000);
    st.on_bar(t(9, 20), 63_000, 61_000, &p);
    st.on_bar(t(11, 0), 62_000, 58_500, &p);
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
    assert_eq!(st.on_bar(t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // A later bar whose high clears the 63_500 target (and does not breach the stop).
    let acts = st.on_bar(t(10, 0), 63_600, 62_500, &p);
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
    assert_eq!(st.on_bar(t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // Nears the target (63_400 < 63_500) but does not reach it → no action.
    assert!(st.on_bar(t(9, 30), 63_400, 61_200, &p).is_empty());
    // A later bar breaches the stop → Stop, not Target.
    let acts = st.on_bar(t(9, 40), 61_000, 59_900, &p);
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
    assert_eq!(st.on_bar(t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // One bar clears the target (high 63_600 ≥ 63_500) AND breaches the stop
    // (low 59_900 ≤ 60_000) → Stop wins.
    let acts = st.on_bar(t(9, 30), 63_600, 59_900, &p);
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
    st.on_bar(t(9, 20), 62_000, 61_000, &p); // entry_price 62_000
    // A bar peaking at 63_000 (below the 63_500 target) sets the high-water mark.
    assert!(st.on_bar(t(10, 0), 63_000, 61_500, &p).is_empty());
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
    assert_eq!(st.on_bar(t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    // 63_600 would trip a 1.0R target but not the 1.5R one → held.
    assert!(st.on_bar(t(10, 0), 63_600, 62_500, &p).is_empty());
    // 64_300 clears 64_250 → Target at the wider level.
    let acts = st.on_bar(t(11, 0), 64_300, 63_000, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 64_250, reason: ExitReason::Target }]);
}

/// (review fix) A TimeFlat exit's MFE includes the flat bar's high — the flat bar
/// is part of the hold, so its excursion counts (symmetry with Stop/Target exits).
#[test]
fn timeflat_mfe_includes_the_flat_bar_high() {
    let p = OrbParams::default();
    let mut st = OrbState::new();
    set_range(&mut st, &p, 61_500, 60_000); // R = 1500, target 63_500
    st.on_bar(t(9, 20), 62_000, 61_000, &p); // entry_price 62_000
    // A mid-hold peak below the target.
    assert!(st.on_bar(t(10, 0), 63_000, 61_500, &p).is_empty());
    // The 15:00 flat bar prints a NEW high (still below the target) then closes flat.
    let acts = st.on_bar(t(15, 0), 63_200, 62_000, &p);
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
    assert_eq!(st.on_bar(t(9, 20), 62_000, 61_000, &p), vec![OrbAction::Enter { limit_price: 62_000 }]);
    assert_eq!(st.phase(), Phase::Long);
    // A bar far above any conceivable target still does not exit.
    assert!(st.on_bar(t(10, 0), 70_000, 61_000, &p).is_empty());
    // The position holds to the time-flat backstop.
    let acts = st.on_bar(t(15, 0), 69_000, 68_000, &p);
    assert_eq!(acts, vec![OrbAction::Exit { limit_price: 68_000, reason: ExitReason::TimeFlat }]);
}

// ---------------------------------------------------------------------------
// Universe scan
// ---------------------------------------------------------------------------

fn candidate(sym: &str, prior_close: f64, today_open: f64, turnover: f64) -> UniverseCandidate {
    UniverseCandidate {
        symbol: sym.to_string(),
        prior_close,
        today_open,
        prior_turnover: turnover,
    }
}

/// AE2: a candidate failing the gap filter produces a rejection envelope naming the
/// filter and carrying the signal values at decision time.
#[test]
fn gap_reject_names_filter_and_values() {
    let p = OrbParams::default(); // gap_min_pct 3.0
    let sink = DecisionSink::new();
    // 60000 → 60500 is +0.83%, below the 3% gap floor.
    let cands = vec![candidate("005930.XKRX", 60_000.0, 60_500.0, 1_000.0)];
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
        .map(|i| candidate(&format!("{:06}.XKRX", i), 100.0, 105.0, i as f64))
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
