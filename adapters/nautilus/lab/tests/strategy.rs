//! U3 — ORB v0 strategy tests. Pure, offline, zero network: the universe scan and
//! the range/entry/exit state machine are exercised in isolation of the engine
//! (the state machine is the unit most likely to hide off-by-one session-time bugs,
//! so it is built and tested test-first).

use chrono::NaiveTime;
use nautilus_ls_lab::params::OrbParams;
use nautilus_ls_lab::signals::{Decision, SignalKind, SignalSink};
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

/// AE2: a candidate failing the gap filter produces a rejection signal naming the
/// filter and carrying the signal values at decision time.
#[test]
fn gap_reject_names_filter_and_values() {
    let p = OrbParams::default(); // gap_min_pct 3.0
    let sink = SignalSink::new();
    // 60000 → 60500 is +0.83%, below the 3% gap floor.
    let cands = vec![candidate("005930.XKRX", 60_000.0, 60_500.0, 1_000.0)];
    let selected = select_universe(&cands, &p, &sink, 42);
    assert!(selected.is_empty(), "a sub-gap candidate is not selected");

    let events = sink.snapshot();
    assert_eq!(events.len(), 1);
    let e = &events[0];
    assert_eq!(e.kind, SignalKind::Universe);
    assert_eq!(e.decision, Some(Decision::Reject));
    assert_eq!(e.filter.as_deref(), Some("gap"));
    let gap = e.values.get("gap_pct").copied().unwrap();
    assert!((gap - 0.8333).abs() < 0.01, "gap_pct recorded: {gap}");
}

/// The universe cap (top-N) is enforced when more candidates qualify, and the
/// survivors are ranked by prior-session turnover.
#[test]
fn universe_caps_top_n_by_turnover() {
    let p = OrbParams::default(); // universe_top_n defaults to 20
    let sink = SignalSink::new();
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
        .filter(|e| e.filter.as_deref() == Some("turnover_rank"))
        .count();
    assert_eq!(rank_rejects, 5);
}
