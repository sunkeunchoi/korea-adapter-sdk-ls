//! U1 — the daily-resolution, multi-session-hold backtest path. Offline: a fixture
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

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, NaiveDate, Utc};
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::ingest::checkpoint::Checkpoint;
use nautilus_ls::ingest::{build_daily_bar, write_bars, write_instruments, BarKind};
use nautilus_ls::instruments::{InstrumentDomain, InstrumentProvider};
use nautilus_ls_lab::agent::envelope::{Decision, DecisionEnvelope};
use nautilus_ls_lab::agent::sink::DecisionSink;
use nautilus_ls_lab::runner::backtest_daily::{
    run_daily, select_daily_sessions, DailyBacktestConfig, DailyPathStrategy, MountedSymbol,
    OpenPositionBook,
};
use nautilus_ls_lab::strategy::orb::UniverseCandidate;
use nautilus_model::data::Bar;
use nautilus_model::enums::{OrderSide, TimeInForce};
use nautilus_model::events::{PositionClosed, PositionOpened};
use nautilus_model::identifiers::{InstrumentId, PositionId, StrategyId};
use nautilus_model::types::{Price, Quantity};
use nautilus_trading::nautilus_strategy;
use nautilus_trading::strategy::{Strategy, StrategyConfig, StrategyCore};
use nautilus_common::actor::{DataActor, DataActorNative};
use serde_json::json;
use tempfile::tempdir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ---------------------------------------------------------------------------
// Catalog scaffold (duplicated — see the module doc)
// ---------------------------------------------------------------------------

fn json_response(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_string(body.to_string())
        .insert_header("content-type", "application/json")
}

fn t8430_body() -> serde_json::Value {
    let sym = |hname: &str, shcode: &str, expcode: &str| {
        json!({ "hname": hname, "shcode": shcode, "expcode": expcode,
            "etfgubun": "0", "uplmtprice": "82000", "dnlmtprice": "44000",
            "jnilclose": "63000", "memedan": "1", "recprice": "63000", "gubun": "1" })
    };
    json!({
        "rsp_cd": "00000",
        "t8430OutBlock": [
            sym("삼성전자", "005930", "KR7005930003"),
            sym("에스케이하이닉스", "000660", "KR7000660001"),
        ]
    })
}

fn t9945_body() -> serde_json::Value {
    json!({
        "rsp_cd": "00000",
        "t9945OutBlock": [
            { "hname": "삼성전자", "shcode": "005930", "expcode": "KR7005930003",
              "etfchk": "0", "nxt_chk": "1", "filler": "" },
            { "hname": "에스케이하이닉스", "shcode": "000660", "expcode": "KR7000660001",
              "etfchk": "0", "nxt_chk": "1", "filler": "" },
        ]
    })
}

fn daily_json(date: &str, o: &str, h: &str, l: &str, c: &str, v: &str) -> serde_json::Value {
    json!({ "date": date, "open": o, "high": h, "low": l, "close": c, "jdiff_vol": v,
        "value": "0", "jongchk": "0", "rate": "0", "pricechk": "0", "ratevalue": "0", "sign": "0" })
}

async fn write_daily_series(catalog: &Path, id: &str, rows: &[serde_json::Value]) {
    let bt = BarKind::Daily.bar_type(InstrumentId::from(id)).unwrap();
    let bars: Vec<Bar> = rows
        .iter()
        .map(|r| build_daily_bar(bt, &serde_json::from_value(r.clone()).unwrap()).unwrap().unwrap())
        .collect();
    write_bars(catalog, bars).await.unwrap();
}

async fn write_masters(catalog: &Path) {
    let server = MockServer::start().await;
    mount_token(&server).await;
    for (p, tr, body) in [
        ("/stock/etc", "t8430", t8430_body()),
        ("/stock/market-data", "t9945", t9945_body()),
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

/// The 22 consecutive KST weekdays the multi-session fixtures use. Index 0 is the
/// PRE-RANGE prior session (the `select_prior_today` lookback); index 1 onward are
/// the 21 in-range sessions the pinned range covers.
const SESSION_DAYS: [&str; 22] = [
    "20240102", "20240103", "20240104", "20240105", "20240108", "20240109", "20240110",
    "20240111", "20240112", "20240115", "20240116", "20240117", "20240118", "20240119",
    "20240122", "20240123", "20240124", "20240125", "20240126", "20240129", "20240130",
    "20240131",
];

/// The pinned range: session index 1 through 21 (0 is the out-of-range prior).
const RANGE_START: &str = "20240103";
const RANGE_END: &str = "20240131";

/// A hand-chained flat daily series long enough to reach hold expiry: a constant
/// close with a small per-session drift, so nothing ever breaches a stop placed a
/// long way below. `low_overrides` injects a crash low on a chosen session index.
///
/// Every price is a multiple of 100 — the KRX instrument masters this fixture
/// ingests carry `price_increment = 100`, and the matching engine *skips the fill*
/// (a WARN, not an error) for any price off that grid, so an off-grid fixture
/// silently trades nothing.
fn flat_series(base: i64, low_overrides: &HashMap<usize, i64>) -> Vec<serde_json::Value> {
    SESSION_DAYS
        .iter()
        .enumerate()
        .map(|(i, date)| {
            let c = base + (i as i64) * 100;
            let low = low_overrides.get(&i).copied().unwrap_or(c - 500);
            daily_json(
                date,
                &c.to_string(),
                &(c + 500).to_string(),
                &low.to_string(),
                &c.to_string(),
                "1000000",
            )
        })
        .collect()
}

/// The two-symbol, 22-session daily-only fixture. `crash` maps a symbol to the
/// (session index → low) overrides that drive a stop-out.
async fn build_daily_fixture(
    data_home: &Path,
    crash: &HashMap<&str, HashMap<usize, i64>>,
) {
    let catalog = data_home.join("catalog");
    write_masters(&catalog).await;
    let empty = HashMap::new();
    write_daily_series(
        &catalog,
        "005930.XKRX",
        &flat_series(60_000, crash.get("005930.XKRX").unwrap_or(&empty)),
    )
    .await;
    write_daily_series(
        &catalog,
        "000660.XKRX",
        &flat_series(50_000, crash.get("000660.XKRX").unwrap_or(&empty)),
    )
    .await;
    let mut cp = Checkpoint::default();
    cp.adjusted_prices = true;
    cp.save(&catalog.join("ingest-checkpoint.json")).unwrap();
}

fn cfg(data_home: &Path, target_m: usize) -> DailyBacktestConfig {
    DailyBacktestConfig::new(data_home, RANGE_START, RANGE_END, target_m)
}

/// The KST calendar date of a UTC-nanosecond timestamp (KST is UTC+9, no DST).
fn kst_date(ns: u64) -> NaiveDate {
    let dt = DateTime::<Utc>::from_timestamp_nanos(ns as i64);
    (dt + chrono::Duration::hours(9)).date_naive()
}

// ---------------------------------------------------------------------------
// Rankers (the daily selection RULE is the caller's — KTD15)
// ---------------------------------------------------------------------------

/// Rank every candidate by prior turnover descending, symbol-ascending on ties.
fn rank_all(candidates: &[UniverseCandidate]) -> Vec<String> {
    let mut c: Vec<&UniverseCandidate> = candidates.iter().collect();
    c.sort_by(|a, b| {
        b.prior_turnover
            .partial_cmp(&a.prior_turnover)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.symbol.cmp(&b.symbol))
    });
    c.into_iter().map(|c| c.symbol.clone()).collect()
}

/// Rank only `only`, in the given order — a fixed script the tests use to drive
/// exactly which symbol is takeable on which session.
fn rank_only(only: &'static [&'static str]) -> impl Fn(&[UniverseCandidate]) -> Vec<String> {
    move |candidates: &[UniverseCandidate]| {
        only.iter()
            .filter(|s| candidates.iter().any(|c| c.symbol == **s))
            .map(|s| (*s).to_string())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// The test-only always-enter strategy
// ---------------------------------------------------------------------------

/// How the test strategy behaves. Deliberately trivial: batch membership IS the
/// instruction, because the runner only ever delivers a bar for a symbol that is
/// either already held or newly taken this session.
#[derive(Debug, Clone)]
struct AlwaysEnterConfig {
    /// Exit after this many session bars observed while the position is open.
    hold_sessions: usize,
    /// The entry-fixed stop, as KRW below the entry bar's close. `None` disables it.
    stop_below: Option<i64>,
    /// Whether a symbol may be entered again after a completed round trip.
    reenter: bool,
    /// The fixed order quantity.
    qty: i64,
}

impl Default for AlwaysEnterConfig {
    fn default() -> Self {
        AlwaysEnterConfig { hold_sessions: 6, stop_below: None, reenter: true, qty: 10 }
    }
}

/// Every bar the strategy actually saw, in arrival order — the stream-side witness
/// the cache-read count is checked against.
#[derive(Debug, Clone, Default)]
struct BarWitness(Arc<Mutex<Vec<(InstrumentId, u64)>>>);

impl BarWitness {
    fn observe(&self, id: InstrumentId, ts: u64) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).push((id, ts));
    }
    fn snapshot(&self) -> Vec<(InstrumentId, u64)> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// The test-only strategy: enter every symbol whose bar arrives while it is not
/// held, hold it for a fixed number of session bars, and exit on the entry-fixed
/// stop or at hold expiry.
///
/// Every exit carries `Some(position.id)` via [`Strategy::close_position`]. Under
/// `OmsType::Hedging` a fill whose client order id has no cached position mints a
/// *fresh* position and the netting fallback is disabled, so an exit submitted
/// without a position id would open an opposite-side short instead of closing the
/// long — ORB's `reduce_only`-only exit is a Netting-only pattern (KTD12).
struct AlwaysEnter {
    core: StrategyCore,
    cfg: AlwaysEnterConfig,
    mounted: Vec<MountedSymbol>,
    book: OpenPositionBook,
    witness: BarWitness,
    /// Symbols with an entry order submitted but not yet opened.
    pending: std::collections::BTreeSet<InstrumentId>,
    /// Symbols that have completed at least one round trip.
    done: std::collections::BTreeSet<InstrumentId>,
    /// Session bars observed since each open position opened.
    held_sessions: HashMap<InstrumentId, usize>,
    /// The entry-fixed stop price per open position.
    stops: HashMap<InstrumentId, i64>,
    /// The pending stop for a symbol whose entry has not filled yet.
    pending_stop: HashMap<InstrumentId, i64>,
    /// The live position id per held symbol.
    position_of: HashMap<InstrumentId, PositionId>,
}

impl AlwaysEnter {
    fn new(mounted: Vec<MountedSymbol>, cfg: AlwaysEnterConfig, witness: BarWitness) -> Self {
        AlwaysEnter {
            core: StrategyCore::new(StrategyConfig {
                strategy_id: Some(StrategyId::from("always-enter-v1")),
                ..Default::default()
            }),
            cfg,
            mounted,
            book: OpenPositionBook::new(),
            witness,
            pending: Default::default(),
            done: Default::default(),
            held_sessions: HashMap::new(),
            stops: HashMap::new(),
            pending_stop: HashMap::new(),
            position_of: HashMap::new(),
        }
    }

    fn exit(&mut self, id: InstrumentId) {
        let Some(pos_id) = self.position_of.get(&id).copied() else {
            return;
        };
        let position = {
            let cache = self.core.cache_rc();
            let cache = cache.borrow();
            cache.position(&pos_id).map(|p| p.cloned())
        };
        if let Some(position) = position {
            // The framework's close-position helper threads `Some(position.id)`
            // through submission — mandatory under Hedging (KTD12).
            self.close_position(&position, None, None, Some(TimeInForce::Gtc), None, None)
                .expect("close_position");
        }
    }
}

impl DailyPathStrategy for AlwaysEnter {
    fn open_position_book(&self) -> OpenPositionBook {
        self.book.clone()
    }
}

impl std::fmt::Debug for AlwaysEnter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlwaysEnter").field("mounted", &self.mounted.len()).finish()
    }
}

nautilus_strategy!(AlwaysEnter, core, {
    fn on_position_opened(&mut self, event: PositionOpened) {
        self.pending.remove(&event.instrument_id);
        self.book.record_opened(event.instrument_id, event.position_id);
        self.position_of.insert(event.instrument_id, event.position_id);
        self.held_sessions.insert(event.instrument_id, 0);
        if let Some(stop) = self.pending_stop.remove(&event.instrument_id) {
            self.stops.insert(event.instrument_id, stop);
        }
    }

    fn on_position_closed(&mut self, event: PositionClosed) {
        self.book.record_closed(&event.instrument_id);
        self.position_of.remove(&event.instrument_id);
        self.held_sessions.remove(&event.instrument_id);
        self.stops.remove(&event.instrument_id);
        self.done.insert(event.instrument_id);
    }
});

impl DataActor for AlwaysEnter {
    fn on_start(&mut self) -> anyhow::Result<()> {
        for m in self.mounted.clone() {
            self.subscribe_bars(m.bar_type, None, None);
        }
        Ok(())
    }

    fn on_bar(&mut self, bar: &Bar) -> anyhow::Result<()> {
        let id = bar.bar_type.instrument_id();
        self.witness.observe(id, bar.ts_event.as_u64());

        if self.book.is_held(&id) {
            let n = self.held_sessions.entry(id).or_insert(0);
            *n += 1;
            let elapsed = *n;
            let stopped = self
                .stops
                .get(&id)
                .is_some_and(|stop| (bar.low.as_f64() as i64) <= *stop);
            if stopped || elapsed >= self.cfg.hold_sessions {
                self.exit(id);
            }
            return Ok(());
        }
        if self.pending.contains(&id) || (!self.cfg.reenter && self.done.contains(&id)) {
            return Ok(());
        }

        // Enter: a marketable limit BUY a long way through the bar's close.
        let close = bar.close.as_f64() as i64;
        let order = self.order().limit(
            id,
            OrderSide::Buy,
            Quantity::from(self.cfg.qty),
            Price::from((close + 5_000).to_string().as_str()),
            Some(TimeInForce::Gtc),
            None, None, Some(false),
            None, None, None, None, None, None, None, None,
        );
        self.submit_order(order, None, None, None)?;
        self.pending.insert(id);
        if let Some(below) = self.cfg.stop_below {
            self.pending_stop.insert(id, close - below);
        }
        Ok(())
    }
}

/// A factory for the runner: one closure that builds a fresh strategy sharing the
/// supplied witness.
fn always_enter(
    cfg: AlwaysEnterConfig,
    witness: BarWitness,
) -> impl Fn(&[MountedSymbol]) -> AlwaysEnter + Send + 'static {
    move |mounted: &[MountedSymbol]| AlwaysEnter::new(mounted.to_vec(), cfg.clone(), witness.clone())
}

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
    let bare = select_daily_sessions(
        &instruments,
        &all_bars,
        &nautilus_ls_lab::params::OrbParams::default(),
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
