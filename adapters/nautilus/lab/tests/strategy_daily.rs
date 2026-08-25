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
//!    multiple of 100.
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

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use chrono::{DateTime, NaiveDate, Utc};
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_common::actor::DataActor;
use nautilus_ls::ingest::checkpoint::Checkpoint;
use nautilus_ls::ingest::{build_daily_bar, write_bars, write_instruments, BarKind};
use nautilus_ls::instruments::{InstrumentDomain, InstrumentProvider};
use nautilus_ls_lab::agent::envelope::SignalKind;
use nautilus_ls_lab::agent::sink::DecisionSink;
use nautilus_ls_lab::params_daily::{DailyParams, FROZEN_ATR_WINDOW_SESSIONS};
use nautilus_ls_lab::runner::backtest_daily::{
    run_daily, DailyBacktestConfig, DailyPathStrategy, DailyRunOutcome, DailySessionContext,
    MountedSymbol,
};
use nautilus_ls_lab::strategy::daily::{
    rank_by_placeholder_signal, AdjustmentBasisShifts, DailyStrategy, EntryRefusal,
};
use nautilus_ls_lab::strategy::orb::UniverseCandidate;
use nautilus_model::data::Bar;
use nautilus_model::enums::{OrderSide, PositionSide};
use nautilus_model::identifiers::InstrumentId;
use serde_json::json;
use tempfile::tempdir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ---------------------------------------------------------------------------
// Catalog scaffold (duplicated — see the module doc)
// ---------------------------------------------------------------------------

/// The KST weekdays the fixtures use. Indices 0 and 1 are **pre-range** — ATR(1)
/// needs two prior sessions strictly before the session date, so a range starting at
/// index 0 or 1 would resolve every prior ATR to `None` and refuse every entry.
const SESSION_DAYS: [&str; 24] = [
    "20240102", "20240103", // pre-range (the ATR(1) lookback)
    "20240104", "20240105", "20240108", "20240109", "20240110", "20240111", "20240112",
    "20240115", "20240116", "20240117", "20240118", "20240119", "20240122", "20240123",
    "20240124", "20240125", "20240126", "20240129", "20240130", "20240131", "20240201",
    "20240202",
];

/// The first session index a pinned range may start on (see [`SESSION_DAYS`]).
const FIRST_IN_RANGE: usize = 2;

/// Real KRX common-share codes (every one ends in the `0` issue-sequence digit, so
/// nothing here reads as a preferred share).
const CODES: [&str; 12] = [
    "005930", "000660", "035420", "035720", "051910", "006400", "005380", "000270", "068270",
    "207940", "012330", "028260",
];

/// One fixture symbol's daily series.
#[derive(Debug, Clone)]
struct SymbolSpec {
    /// The 6-digit KRX code; the instrument id is `{code}.XKRX`.
    code: &'static str,
    /// The base close in KRW. The series drifts `+100` per session, so **every**
    /// price stays on the masters' 100 KRW grid.
    base: i64,
    /// Daily volume. `prior_turnover = prior_close × prior_volume`, so this is what
    /// orders the placeholder ranking signal.
    volume: i64,
    /// `SESSION_DAYS` index → that session's low, overriding the default `close − 500`.
    lows: HashMap<usize, i64>,
    /// The first `SESSION_DAYS` index that carries a bar — a symbol that starts
    /// inside the range has no derivable prior ATR on its first candidate session.
    first_session: usize,
    /// `SESSION_DAYS` indices this symbol carries **no bar at all** on — the data gap
    /// a KRX trading halt, a suspension, or an incomplete ingest leaves behind. The
    /// symbol keeps trading afterwards, so this is a hole in the series rather than a
    /// truncation.
    gaps: BTreeSet<usize>,
    /// A limit-locked series: `O = H = L = C` on every session, so ATR(1) is exactly
    /// zero — *available*, and it passes an `is_some` check (KTD9).
    locked: bool,
}

impl SymbolSpec {
    fn new(code: &'static str, base: i64, volume: i64) -> Self {
        SymbolSpec {
            code,
            base,
            volume,
            lows: HashMap::new(),
            first_session: 0,
            gaps: BTreeSet::new(),
            locked: false,
        }
    }

    fn id(&self) -> InstrumentId {
        InstrumentId::from(format!("{}.XKRX", self.code).as_str())
    }
}

fn json_response(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_string(body.to_string())
        .insert_header("content-type", "application/json")
}

fn t8430_body(specs: &[SymbolSpec]) -> serde_json::Value {
    let rows: Vec<serde_json::Value> = specs
        .iter()
        .map(|s| {
            json!({ "hname": s.code, "shcode": s.code, "expcode": format!("KR7{}003", s.code),
                "etfgubun": "0", "uplmtprice": "82000", "dnlmtprice": "44000",
                "jnilclose": "63000", "memedan": "1", "recprice": "63000", "gubun": "1" })
        })
        .collect();
    json!({ "rsp_cd": "00000", "t8430OutBlock": rows })
}

fn t9945_body(specs: &[SymbolSpec]) -> serde_json::Value {
    let rows: Vec<serde_json::Value> = specs
        .iter()
        .map(|s| {
            json!({ "hname": s.code, "shcode": s.code, "expcode": format!("KR7{}003", s.code),
                "etfchk": "0", "nxt_chk": "1", "filler": "" })
        })
        .collect();
    json!({ "rsp_cd": "00000", "t9945OutBlock": rows })
}

fn daily_json(date: &str, o: i64, h: i64, l: i64, c: i64, v: i64) -> serde_json::Value {
    json!({ "date": date, "open": o.to_string(), "high": h.to_string(), "low": l.to_string(),
        "close": c.to_string(), "jdiff_vol": v.to_string(),
        "value": "0", "jongchk": "0", "rate": "0", "pricechk": "0", "ratevalue": "0", "sign": "0" })
}

fn daily_bar(id: InstrumentId, row: serde_json::Value) -> Bar {
    let bt = BarKind::Daily.bar_type(id).unwrap();
    build_daily_bar(bt, &serde_json::from_value(row).unwrap()).unwrap().unwrap()
}

async fn write_masters(catalog: &Path, specs: &[SymbolSpec]) {
    let server = MockServer::start().await;
    mount_token(&server).await;
    for (p, tr, body) in [
        ("/stock/etc", "t8430", t8430_body(specs)),
        ("/stock/market-data", "t9945", t9945_body(specs)),
    ] {
        Mock::given(method("POST"))
            .and(path(p))
            .and(header("tr_cd", tr))
            .respond_with(json_response(body))
            .mount(&server)
            .await;
    }
    let sdk = LsSdk::new(mock_config(&server.uri())).unwrap();
    let mut provider = InstrumentProvider::new(sdk.clone());
    provider.load_domain(InstrumentDomain::DomesticEquity).await.unwrap();
    write_instruments(catalog, provider.all_any()).await.unwrap();
}

/// The daily series for one spec. Every price is a multiple of 100.
fn series(spec: &SymbolSpec) -> Vec<serde_json::Value> {
    SESSION_DAYS
        .iter()
        .enumerate()
        .filter(|(i, _)| *i >= spec.first_session && !spec.gaps.contains(i))
        .map(|(i, date)| {
            if spec.locked {
                // A KRX limit-locked session: O = H = L = C, so the true range is
                // exactly zero and so is ATR(1).
                let c = spec.base;
                daily_json(date, c, c, c, c, spec.volume)
            } else {
                let c = spec.base + (i as i64) * 100;
                let low = spec.lows.get(&i).copied().unwrap_or(c - 500);
                daily_json(date, c, c + 500, low, c, spec.volume)
            }
        })
        .collect()
}

/// Build the offline catalog for `specs` and mark it price-adjusted.
async fn build_fixture(data_home: &Path, specs: &[SymbolSpec]) {
    let catalog = data_home.join("catalog");
    write_masters(&catalog, specs).await;
    for spec in specs {
        let bars: Vec<Bar> = series(spec).into_iter().map(|r| daily_bar(spec.id(), r)).collect();
        write_bars(&catalog, bars).await.unwrap();
    }
    let mut cp = Checkpoint::default();
    cp.adjusted_prices = true;
    cp.save(&catalog.join("ingest-checkpoint.json")).unwrap();
}

/// A config pinned to `SESSION_DAYS[from..=to]`, with the ATR window pinned to the
/// one the frozen stop rule names (see the module doc).
fn cfg_range(dir: &Path, from: usize, to: usize, target_m: usize) -> DailyBacktestConfig {
    let mut c = DailyBacktestConfig::new(dir, SESSION_DAYS[from], SESSION_DAYS[to], target_m);
    c.params.atr_window = FROZEN_ATR_WINDOW_SESSIONS;
    c
}

/// `n` symbols on a common price series with **strictly descending** turnover, so
/// the placeholder ranking signal's order is exactly `CODES[0..n]`.
fn descending_turnover(n: usize) -> Vec<SymbolSpec> {
    (0..n)
        .map(|i| SymbolSpec::new(CODES[i], 50_000, ((n - i) as i64) * 100_000))
        .collect()
}

/// The KST calendar date of a UTC-nanosecond timestamp (KST is UTC+9, no DST).
fn kst_date(ns: u64) -> NaiveDate {
    let dt = DateTime::<Utc>::from_timestamp_nanos(ns as i64);
    (dt + chrono::Duration::hours(9)).date_naive()
}

// ---------------------------------------------------------------------------
// Reading what the strategy recorded
// ---------------------------------------------------------------------------

/// One strategy decision record, flattened for assertion. The record on the refusal
/// path is the **only** evidence a fail-closed gate ran (AE3), so every gate scenario
/// asserts its presence rather than only the absence of a trade.
#[derive(Debug, Clone)]
struct Rec {
    date: NaiveDate,
    symbol: String,
    kind: SignalKind,
    filter: Option<String>,
    values: BTreeMap<String, f64>,
}

/// Every record the *strategy* emitted, dropping the runner's universe envelopes.
fn strategy_records(sink: &DecisionSink) -> Vec<Rec> {
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

fn refusals<'a>(recs: &'a [Rec], reason: EntryRefusal) -> Vec<&'a Rec> {
    recs.iter()
        .filter(|r| {
            matches!(r.kind, SignalKind::OrderRejectedSizing)
                && r.filter.as_deref() == Some(reason.as_str())
        })
        .collect()
}

fn placed<'a>(recs: &'a [Rec]) -> Vec<&'a Rec> {
    recs.iter().filter(|r| matches!(r.kind, SignalKind::OrderPlaced)).collect()
}

/// The in-range session index of a timestamp, on the run's own session calendar.
fn session_index(outcome: &DailyRunOutcome, ns: u64) -> usize {
    let d = kst_date(ns);
    outcome
        .selection
        .sessions
        .iter()
        .position(|s| s.date == d)
        .unwrap_or_else(|| panic!("{d} is not an in-range session"))
}

fn close_idx(outcome: &DailyRunOutcome, p: &nautilus_model::position::Position) -> Option<usize> {
    p.ts_closed.map(|t| session_index(outcome, t.as_u64()))
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0)
}

// ---------------------------------------------------------------------------
// Focused unit tests — the gates that return before `self.order()`
// ---------------------------------------------------------------------------

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

fn day(i: usize) -> NaiveDate {
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

// ---------------------------------------------------------------------------
// Engine scenarios — everything that needs a fill
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
