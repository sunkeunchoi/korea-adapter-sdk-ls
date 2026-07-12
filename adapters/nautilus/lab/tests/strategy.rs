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
    UniverseCandidate,
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
    st.on_bar(t(9, 20), 62_000, 61_000, &p); // entry 62_000
    // A mid-hold peak below the target fixes the high-water at 63_000.
    assert!(st.on_bar(t(9, 30), 63_000, 61_000, &p).is_empty());
    // A stop bar whose HIGH (63_400) tops all prior highs but whose LOW breaches
    // the stop → Stop exit, and the bar high is excluded from MFE.
    let acts = st.on_bar(t(9, 40), 63_400, 59_900, &p);
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
    st.on_bar(t(9, 20), 62_000, 61_000, &p); // entry 62_000
    // A target-exit bar with a wick far above the 63_500 target (high 64_800).
    let acts = st.on_bar(t(10, 0), 64_800, 62_500, &p);
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
    assert!(st.on_bar(t(9, 0), 61_000, 61_000, &p).is_empty());
    assert_eq!(st.range(), Some((61_000, 61_000)));
    // Breakout above the flat range → entry (no target with R ≤ 0).
    st.on_bar(t(9, 20), 62_000, 61_500, &p);
    // A higher bar folds into high_water, but the degenerate range makes mfe_r
    // report 0.0 via the sentinel guard.
    st.on_bar(t(10, 0), 63_000, 61_800, &p);
    assert_eq!(st.mfe_r(), 0.0, "degenerate range → mfe_r 0.0");
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

fn candidate(sym: &str, prior_close: f64, today_open: f64, turnover: f64) -> UniverseCandidate {
    UniverseCandidate {
        symbol: sym.to_string(),
        prior_close,
        today_open,
        prior_turnover: turnover,
        meta: CandidateMeta::Untagged,
        prior_atr: None,
        prior_open_vol_mean: None,
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
    today_open: f64,
    turnover: f64,
    tradable: bool,
    market: MarketClass,
    cap: CapTier,
) -> UniverseCandidate {
    UniverseCandidate {
        symbol: sym.to_string(),
        prior_close: 100.0,
        today_open,
        prior_turnover: turnover,
        meta: CandidateMeta::Tagged { tradable, tags: tags(market, cap) },
        prior_atr: None,
        prior_open_vol_mean: None,
    }
}

/// Covers AE3: a non-tradable (designated) symbol is excluded even when its gap
/// and turnover qualify, with the rejection naming the gate.
#[test]
fn non_tradable_candidate_is_excluded_even_when_gap_and_turnover_qualify() {
    let p = OrbParams::default(); // gap_min_pct 3.0
    let sink = DecisionSink::new();
    let cands = vec![
        tagged("111111.XKRX", 105.0, 9_999_999.0, false, MarketClass::Kospi, CapTier::Top),
        tagged("222222.XKRX", 105.0, 1_000.0, true, MarketClass::Kospi, CapTier::Top),
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
        tagged("111111.XKRX", 105.0, 999.0, true, MarketClass::Kosdaq, CapTier::Mid),
        tagged("222222.XKRX", 105.0, 1_000.0, true, MarketClass::Kosdaq, CapTier::Mid),
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
        vec![tagged("300001.XKRX", 105.0, 5_000.0, true, MarketClass::Kosdaq, CapTier::BelowBoard)];
    let selected = select_universe(&cands, &p, &sink, 1);
    assert_eq!(selected, vec!["300001.XKRX".to_string()]);
}

/// R4: a candidate the artifact does not cover is non-selectable and recorded,
/// never silently defaulted into the tradeable set.
#[test]
fn missing_metadata_candidate_is_dropped_and_recorded() {
    let p = OrbParams::default();
    let sink = DecisionSink::new();
    let mut c = candidate("111111.XKRX", 100.0, 105.0, 9_999.0);
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
        tagged("111111.XKRX", 105.0, 10_000.0, false, MarketClass::Kospi, CapTier::Top),
        tagged("222222.XKRX", 105.0, 3_000.0, true, MarketClass::Kospi, CapTier::Top),
        tagged("333333.XKRX", 105.0, 5_000.0, true, MarketClass::Kospi, CapTier::Mid),
        tagged("444444.XKRX", 105.0, 1_000.0, true, MarketClass::Kosdaq, CapTier::Mid),
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
        tagged("111111.XKRX", 105.0, 5_000.0, true, MarketClass::Kospi, CapTier::Top),
        candidate("222222.XKRX", 100.0, 105.0, 1_000.0), // legacy Untagged
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
