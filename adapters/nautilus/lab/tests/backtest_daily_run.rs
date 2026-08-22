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

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::ingest::checkpoint::Checkpoint;
use nautilus_ls::ingest::{build_daily_bar, write_bars, write_instruments, BarKind};
use nautilus_ls::instruments::{InstrumentDomain, InstrumentProvider};
use nautilus_ls_lab::agent::envelope::{self as envelope, Decision, DecisionEnvelope};
use nautilus_ls_lab::agent::sink::DecisionSink;
use nautilus_ls_lab::artifacts::manifest::Manifest;
use nautilus_ls_lab::artifacts::observation::RunObservation;
use nautilus_ls_lab::artifacts::performance::{
    ClientOrderEntryRiskLedger, EntryRisk, PerformanceReport,
};
use nautilus_ls_lab::artifacts::{
    aborted_runs, list_runs, run_id, RunSource, DECISIONS_FILE, MANIFEST_FILE, OBSERVATION_FILE,
    PERFORMANCE_FILE,
};
use nautilus_ls_lab::params_daily::{DAILY_STRATEGY_ID, FROZEN_ATR_WINDOW_SESSIONS};
use nautilus_ls_lab::strategy::daily::PLACEHOLDER_RANKING_SIGNAL;
use nautilus_ls_lab::runner::backtest_daily::{
    run, run_daily, run_inner, select_daily_sessions, DailyBacktestConfig, DailyPathStrategy,
    DailyRunOutcome, EntryRiskProjection, MountedSymbol, OpenPositionBook,
};
use nautilus_ls_lab::strategy::orb::UniverseCandidate;
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_model::data::Bar;
use nautilus_model::enums::{OrderSide, TimeInForce};
use nautilus_model::events::{PositionClosed, PositionOpened};
use nautilus_model::identifiers::{ClientOrderId, InstrumentId, PositionId, StrategyId};
use nautilus_model::orders::Order;
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
    /// The first entry's recorded `risk_per_share`. Every subsequent entry records
    /// `risk_base + n · risk_step`, so **every entry in a run carries a distinct
    /// risk value** — a uniform-value fixture would hide a mis-ordered projection
    /// entirely (KTD3 assertion 3).
    risk_base: f64,
    /// The per-entry increment that makes the recorded risks distinct.
    risk_step: f64,
    /// A symbol entered **without** recording an entry risk — the position then has
    /// no ledger entry and must resolve to `None` (the legacy P&L path).
    skip_risk: Option<&'static str>,
    /// A symbol whose entry order is submitted at an off-precision price. KRX
    /// equities carry `price_precision = 0`, so the risk engine denies a
    /// precision-1 price: a recorded ledger entry that never opens a position.
    reject_entry: Option<&'static str>,
}

impl Default for AlwaysEnterConfig {
    fn default() -> Self {
        AlwaysEnterConfig {
            hold_sessions: 6,
            stop_below: None,
            reenter: true,
            qty: 10,
            risk_base: 1_000.0,
            risk_step: 250.0,
            skip_risk: None,
            reject_entry: None,
        }
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
    /// The shared, client-order-keyed entry-risk ledger (KTD3). `ClientOrderId` is
    /// the only identity available here at submit time.
    entry_risk: ClientOrderEntryRiskLedger,
    /// How many entries have been recorded — the distinct-risk counter.
    entries_recorded: usize,
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
            entry_risk: ClientOrderEntryRiskLedger::new(),
            entries_recorded: 0,
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

    fn entry_risk_ledger(&self) -> ClientOrderEntryRiskLedger {
        self.entry_risk.clone()
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
        // The stream-side witness for the reconciliation (KTD3 assertion 2): which
        // recorded entries actually opened a position, known independently of the
        // cache read the assertion checks.
        self.entry_risk.record_opened(event.opening_order_id);
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
        let symbol = id.to_string();
        // KRX equities carry `price_precision = 0`; a precision-1 price is denied by
        // the risk engine, so the entry is recorded but never opens a position.
        let price = if self.cfg.reject_entry == Some(symbol.as_str()) {
            Price::new((close + 5_000) as f64 + 0.5, 1)
        } else {
            Price::from((close + 5_000).to_string().as_str())
        };
        let order = self.order().limit(
            id,
            OrderSide::Buy,
            Quantity::from(self.cfg.qty),
            price,
            Some(TimeInForce::Gtc),
            None, None, Some(false),
            None, None, None, None, None, None, None, None,
        );
        // Capture the entry-fixed risk keyed by CLIENT ORDER ID (KTD3) — the only
        // identity available at submit time, and exactly the key the read side
        // carries as `Position.opening_order_id`. Each entry gets a DISTINCT
        // `risk_per_share` so a mis-ordered projection cannot hide.
        if self.cfg.skip_risk != Some(symbol.as_str()) {
            let n = self.entries_recorded;
            self.entries_recorded += 1;
            self.entry_risk.record(
                order.client_order_id(),
                EntryRisk {
                    risk_per_share: self.cfg.risk_base + (n as f64) * self.cfg.risk_step,
                    qty: self.cfg.qty as f64,
                },
            );
        }
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

/// Like [`always_enter`] but sharing a **caller-owned** entry-risk ledger, so a test
/// can read exactly what the strategy recorded at submit time and check the runner's
/// projection against it position by position.
fn always_enter_sharing(
    cfg: AlwaysEnterConfig,
    ledger: ClientOrderEntryRiskLedger,
) -> impl Fn(&[MountedSymbol]) -> AlwaysEnter + Send + 'static {
    move |mounted: &[MountedSymbol]| {
        let mut s = AlwaysEnter::new(mounted.to_vec(), cfg.clone(), BarWitness::default());
        s.entry_risk = ledger.clone();
        s
    }
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

// ---------------------------------------------------------------------------
// R. Entry-risk capture and the index-aligned join (U2 — KTD3, R12)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// U5 — the `lab-backtest-daily` entry point.
//
// The dead-code hazard is the point of this unit: a daily path reachable only from
// `#[test]` bodies is dead code with a green coverage report
// (docs/solutions/architecture-patterns/a-safety-escape-hatch-wired-to-none-at-the-
// composition-root-is-dead-code-its-unit-tests-still-pass.md). Each scenario below is
// therefore marked *(binary)* — driving the compiled bin through `CARGO_BIN_EXE_*`, which
// exercises the real composition root — or *(library)*, for the two seams that are
// deliberately unreachable from `main_cli`.
// ---------------------------------------------------------------------------

/// A `lab-backtest-daily` invocation over `data_home` with no `LS_BTD_*` set beyond the
/// three the caller supplies. The environment is cleared rather than inherited: this
/// shell exports a dozen `LS_*` variables, and an inherited `LS_DATA_HOME` would make the
/// missing-variable scenario pass for the wrong reason.
fn daily_bin(data_home: Option<&Path>) -> std::process::Command {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_lab-backtest-daily"));
    cmd.env_clear();
    if let Some(home) = data_home {
        cmd.env("LS_DATA_HOME", home);
    }
    cmd.env("LS_BTD_SDATE", RANGE_START).env("LS_BTD_EDATE", RANGE_END);
    cmd
}

/// *(binary)* The compiled bin lands a finalized run in the registry, and that run holds a
/// position across session boundaries — the whole point of the path, proven through the
/// composition root rather than through `run_daily` directly.
#[tokio::test]
async fn compiled_bin_lands_a_finalized_run_holding_across_sessions() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;

    let out = daily_bin(Some(dir.path())).env("LS_BTD_TARGET_M", "2").output().unwrap();
    assert!(
        out.status.success(),
        "bin failed: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let runs = list_runs(dir.path());
    assert_eq!(runs.len(), 1, "exactly one finalized run: {runs:?}");
    assert!(aborted_runs(dir.path()).is_empty(), "no staging residue");

    let run_dir = dir.path().join("runs").join(&runs[0]);
    let manifest: Manifest =
        serde_json::from_str(&std::fs::read_to_string(run_dir.join(MANIFEST_FILE)).unwrap())
            .unwrap();
    // The registry discriminator, written by the real binary (KTD14). U8's filters key on
    // exactly this field; if the bin wrote "orb" here they would all be no-ops.
    assert_eq!(manifest.strategy_id, DAILY_STRATEGY_ID);
    assert!(runs[0].contains(DAILY_STRATEGY_ID), "the run id carries the discriminator: {}", runs[0]);
    assert!(manifest.daily_params.is_some(), "the daily terms are carried");
    let decision_text = std::fs::read_to_string(run_dir.join(DECISIONS_FILE)).unwrap();
    let decisions = envelope::from_jsonl(&decision_text).unwrap();
    assert!(!decisions.is_empty(), "the finalized decision stream is non-empty");
    let line_order: Vec<_> = decision_text
        .lines()
        .map(|line| serde_json::from_str::<DecisionEnvelope>(line).unwrap().envelope_id)
        .collect();
    let parsed_order: Vec<_> = decisions.iter().map(|decision| decision.envelope_id).collect();
    assert_eq!(parsed_order, line_order, "the JSONL parser preserves append order");
    // The ATR bridge reached the manifest: `OrbParams::atr_window` defaults to 14, and an
    // unbridged run would record that and refuse every entry as `atr_unavailable`.
    assert_eq!(
        manifest.params.atr_window,
        FROZEN_ATR_WINDOW_SESSIONS,
        "assembly ran with the frozen daily ATR window, not OrbParams' default 14"
    );

    // A position held across session boundaries. The fixture is 21 in-range sessions
    // against a 16-session hold, so a session-1 entry is still open on session 16.
    let report: PerformanceReport =
        serde_json::from_str(&std::fs::read_to_string(run_dir.join(PERFORMANCE_FILE)).unwrap())
            .unwrap();
    let spans_sessions = report.trades.iter().any(|t| match t.ts_closed {
        // Closed on a later KST session than it opened on — a genuine overnight hold,
        // which the per-session ORB path cannot produce at all.
        Some(closed) => kst_date(closed) > kst_date(t.ts_opened),
        // Still open at range end, having opened before the last session: censored, and
        // equally proof the position outlived an engine batch.
        None => kst_date(t.ts_opened) < NaiveDate::from_ymd_opt(2024, 1, 31).unwrap(),
    });
    assert!(
        spans_sessions,
        "at least one position outlives its opening session: {:?}",
        report.trades
    );
    assert!(
        out.stdout.windows(b"lab-backtest-daily summary".len()).any(|w| w == b"lab-backtest-daily summary"),
        "the trailing summary block prints"
    );
}

/// *(binary)* A missing `LS_DATA_HOME` names that variable rather than failing obscurely
/// downstream on a path that does not exist.
#[tokio::test]
async fn compiled_bin_names_the_missing_data_home() {
    let out = daily_bin(None).output().unwrap();
    assert!(!out.status.success(), "the bin must not succeed without LS_DATA_HOME");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("LS_DATA_HOME"), "the error names the variable: {err}");
}

/// *(binary)* A malformed numeric variable errors rather than silently defaulting.
///
/// This is the `backtest.rs:1049` `unwrap_or(1)` anti-pattern, pinned by
/// `research_cli.rs:430`. Silently defaulting here would finalize a run at a concurrency
/// nobody chose and record the substituted value in the manifest as though intended.
#[tokio::test]
async fn compiled_bin_refuses_a_malformed_numeric_variable() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;

    let out = daily_bin(Some(dir.path())).env("LS_BTD_TARGET_M", "2x").output().unwrap();
    assert!(!out.status.success(), "a malformed LS_BTD_TARGET_M must not default");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("LS_BTD_TARGET_M"), "the error names the variable: {err}");
    assert!(err.contains("2x"), "the error quotes the offending value: {err}");
    assert!(list_runs(dir.path()).is_empty(), "no run was finalized");
}

/// *(library)* A catalog mutated in-range between the engine run and the finalize
/// fingerprint re-check aborts with no registry residue. The `before_finalize` seam is a
/// library-only surface — deliberately not reachable through `main_cli`.
#[tokio::test]
async fn mid_run_catalog_change_aborts_with_no_residue() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    let catalog = dir.path().join("catalog");
    let start = Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap();
    let config = cfg(dir.path(), 2);
    let staged_run_id = run_id(
        start,
        RunSource::Backtest,
        DAILY_STRATEGY_ID,
        config.daily.strategy_version,
    );
    let staged_decisions = dir
        .path()
        .join("runs")
        .join(format!(".tmp-{staged_run_id}"))
        .join(DECISIONS_FILE);

    // Append an extra in-range daily bar after the engine run, before the re-check.
    let mutate = async {
        let streamed = std::fs::read_to_string(&staged_decisions).unwrap();
        assert!(
            !envelope::from_jsonl(&streamed).unwrap().is_empty(),
            "decision envelopes reach staging before finalization"
        );
        write_daily_series(
            &catalog,
            "005930.XKRX",
            &[daily_json("20240130", "70000", "70500", "69500", "70000", "999")],
        )
        .await;
    };

    let err = run_inner(config, start, mutate).await.unwrap_err();
    assert!(err.to_string().contains("catalog changed in-range"), "err: {err}");
    assert!(list_runs(dir.path()).is_empty(), "no finalized run");
    assert!(aborted_runs(dir.path()).is_empty(), "no staging residue");
}

/// *(library)* A malformed range is discovered after the run writer opens staging but
/// before the blocking engine starts, so it is a graceful refusal rather than an abort.
#[tokio::test]
async fn pre_engine_parse_refusal_removes_staging() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    let mut config = cfg(dir.path(), 2);
    config.range.start = "not-a-date".to_string();

    let start = Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap();
    let err = run(config, start).await.unwrap_err();
    assert!(err.to_string().contains("input contains invalid characters"), "err: {err}");
    assert!(list_runs(dir.path()).is_empty(), "no finalized run");
    assert!(aborted_runs(dir.path()).is_empty(), "refusal removes staging");
}

/// *(library)* The run refuses to start while the ingest advisory lock is held, and the
/// single guard spans the engine phase and the finalize re-check.
#[tokio::test]
async fn refused_while_ingest_lock_held() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    let catalog = dir.path().join("catalog");
    let _held = AdvisoryLock::acquire(&catalog, LockKind::Ingest).unwrap();

    let start = Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap();
    let err = run(cfg(dir.path(), 2), start).await.unwrap_err();
    assert!(err.to_string().contains("refused"), "err: {err}");
    assert!(list_runs(dir.path()).is_empty());
}

/// An invalid daily parameter set is refused **before** the engine runs, not at manifest
/// assembly hours later. `Manifest::new_daily` remains the construction-point gate; this
/// is the fail-fast one.
#[tokio::test]
async fn an_off_freeze_parameter_set_is_refused_before_the_engine_runs() {
    let dir = tempdir().unwrap();
    // No catalog is built: reaching the "no catalog" error would prove validation ran
    // AFTER the catalog check, and reaching the engine would prove it ran after that.
    let mut c = cfg(dir.path(), 2);
    c.daily.holding_period_sessions = 5;

    let start = Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap();
    let err = run(c, start).await.unwrap_err();
    assert!(err.to_string().contains("invalid daily parameter set"), "err: {err}");
    assert!(
        !err.to_string().contains("no catalog"),
        "validation runs before the catalog check, so a bad set never reaches the engine: {err}"
    );
}

/// The ATR bridge, pinned directly: `DailyParams::atr_window_sessions` reaches the shared
/// candidate assembly, and `OrbParams::atr_window`'s default does not.
///
/// This is the unit's sharpest silent failure. The frozen daily stop is ATR(1); the
/// `OrbParams` the assembly reads defaults `atr_window` to 14, needing 15 prior sessions.
/// Nothing but `assembly_params()` connects the two. Unbridged, the first 14 in-range
/// sessions of every symbol derive **no** prior ATR — and because the stop fails closed
/// (KTD9), that is not a visible misconfiguration: every entry is refused
/// `atr_unavailable`, the run finalizes green with zero positions, and `return_on_risk`
/// is vacuous. The assertion below is the difference between those two worlds.
#[tokio::test]
async fn the_frozen_atr_window_reaches_candidate_assembly() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    let c = cfg(dir.path(), 2);

    assert_eq!(
        c.params.atr_window,
        nautilus_ls_lab::params::OrbParams::default().atr_window,
        "the raw config still carries ORB's window — the bridge is not a mutation of it"
    );
    assert_eq!(c.assembly_params().atr_window, FROZEN_ATR_WINDOW_SESSIONS);
    assert_ne!(
        c.assembly_params().atr_window,
        c.params.atr_window,
        "the two genuinely differ, so this test is not tautological"
    );

    let catalog = dir.path().join("catalog");
    let instruments = nautilus_ls::ingest::read_all_instruments(&catalog).await.unwrap();
    let all_bars = nautilus_ls::ingest::read_all_bars(&catalog).await.unwrap();
    let bounds = |d: &str, t: chrono::NaiveTime| {
        nautilus_ls::ingest::kst_to_unix_nanos(
            NaiveDate::parse_from_str(d, "%Y%m%d").unwrap(),
            t,
        )
        .unwrap()
        .as_u64()
    };
    let start_ns = bounds(RANGE_START, chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    let end_ns = bounds(RANGE_END, chrono::NaiveTime::from_hms_opt(23, 59, 59).unwrap());

    let derivable = |params: &nautilus_ls_lab::params::OrbParams| -> usize {
        select_daily_sessions(
            &instruments,
            &all_bars,
            params,
            &DecisionSink::new(),
            start_ns,
            end_ns,
            &rank_all,
        )
        .unwrap()
        .sessions
        .iter()
        .filter(|s| s.prior_atr.values().any(Option::is_some))
        .count()
    };

    let bridged = derivable(&c.assembly_params());
    let unbridged = derivable(&c.params);
    assert_eq!(bridged, 20, "ATR(1) is derivable from the second in-range session onward");
    assert_eq!(
        unbridged, 7,
        "ATR(14) leaves only the last 7 of 21 sessions with a derivable ATR — the other 13 \
         would refuse every entry, and the run would still finalize green"
    );
}

// ---------------------------------------------------------------------------
// U6 — the typed run observation and the per-session series.
//
// The unit-level behaviour (exit attribution, the R25 refusal, the fail-closed
// placeholder accessor, the empty-run cases) is covered by `artifacts::observation`'s own
// tests. What can only be checked here is that a REAL finalized run produces one, that it
// agrees with the sibling artifacts it must be consistent with, and that a refused run
// leaves none behind.
// ---------------------------------------------------------------------------

/// A finalized daily run writes the fifth artifact, and it agrees with the manifest and
/// the performance report it sits beside.
#[tokio::test]
async fn a_finalized_daily_run_writes_a_consistent_observation() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;

    let start = Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap();
    let result = run(cfg(dir.path(), 2), start).await.unwrap();

    // Written to disk, not merely returned in memory.
    let on_disk: RunObservation = serde_json::from_str(
        &std::fs::read_to_string(result.run_dir.join(OBSERVATION_FILE)).unwrap(),
    )
    .unwrap();

    // Everything except the statistic is bit-identical across the round trip.
    //
    // The statistic is compared to a tolerance because `serde_json`'s DEFAULT float parser
    // is not correctly rounded — it can land one ULP off — and the lab does not enable the
    // `float_roundtrip` feature that would fix it. Measured here: `1.0666666666666669` is
    // written and `1.0666666666666667` reads back. That is ~2e-16 relative, some fourteen
    // orders of magnitude below any hurdle this number is ever compared against, so it
    // cannot change a verdict. It is asserted rather than ignored because the alternative
    // is a future reader adding a bit-exact assertion here and getting a mystery.
    assert_eq!(
        RunObservation { observed_net_ror: result.observation.observed_net_ror, ..on_disk.clone() },
        result.observation,
        "every non-float field survives the round trip exactly"
    );
    assert!(
        (on_disk.observed_net_ror - result.observation.observed_net_ror).abs()
            < 1e-12 * result.observation.observed_net_ror.abs().max(1.0),
        "the statistic round-trips to within the JSON float parser's precision: {} vs {}",
        on_disk.observed_net_ror,
        result.observation.observed_net_ror
    );

    // R13: the observation carries its OWN range and fingerprint, so a consumer never has
    // to re-read the manifest to construct a judgment.
    assert_eq!(on_disk.data_range, result.manifest.data_range);
    assert_eq!(on_disk.catalog_fingerprint, result.manifest.catalog_fingerprint);
    assert_eq!(on_disk.run_id, result.run_id);

    // The closure check against performance.json: a dropped or double-counted session
    // shows up here and nowhere else.
    let edge = result.performance.edge_evaluation();
    assert_eq!(
        on_disk.series_risk_capital_total(),
        edge.risk_capital_total.expect("a daily run carries risk on every closed trade"),
        "Σ per-session risk_capital == performance.json's risk_capital_total"
    );
    // Compared in memory, because the on-disk float is one ULP off — see the round-trip
    // note above and `artifacts::observation`'s module docs.
    assert_eq!(result.observation.observed_net_ror, edge.return_on_risk.unwrap());

    // Every position is accounted for: closed on some session, or censored at range end.
    let closes: u32 = on_disk.sessions.iter().map(|s| s.closes).sum();
    assert_eq!(closes, on_disk.closed_positions);
    assert_eq!(
        (on_disk.closed_positions + on_disk.censored_positions) as usize,
        result.performance.trades.len()
    );

    // One row per in-range session, including the leading hold-length that exit
    // attribution necessarily leaves empty (KTD13).
    assert_eq!(on_disk.sessions.len(), result.outcome.selection.sessions.len());
}

/// R26/KTD6: the shipped run carries the placeholder marker, and the marker is enforced
/// rather than advisory — the run is unusable as a judgment.
#[tokio::test]
async fn a_placeholder_signal_run_is_marked_and_yields_no_judgment() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;

    let start = Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap();
    let result = run(cfg(dir.path(), 2), start).await.unwrap();

    assert!(
        result.observation.ranking_signal_is_placeholder,
        "the shipped signal is the placeholder — the signal carrying the hypothesis is \
         turn one's act"
    );
    assert_eq!(result.observation.ranking_signal, PLACEHOLDER_RANKING_SIGNAL.name);

    let err = result.observation.judgment_arguments().unwrap_err();
    assert!(
        err.to_string().contains("PLACEHOLDER"),
        "the only path to the judgment arguments refuses, naming why: {err}"
    );
}

/// A run aborted at the finalize fingerprint re-check leaves no observation anywhere —
/// not in a finalized run dir, and not in staging.
#[tokio::test]
async fn an_aborted_run_leaves_no_observation() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    let catalog = dir.path().join("catalog");

    let mutate = async {
        write_daily_series(
            &catalog,
            "005930.XKRX",
            &[daily_json("20240130", "70000", "70500", "69500", "70000", "999")],
        )
        .await;
    };
    let start = Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap();
    run_inner(cfg(dir.path(), 2), start, mutate).await.unwrap_err();

    assert!(list_runs(dir.path()).is_empty());
    assert!(aborted_runs(dir.path()).is_empty());
    let found: Vec<_> = walk_files(dir.path())
        .into_iter()
        .filter(|p| p.file_name().and_then(|n| n.to_str()) == Some(OBSERVATION_FILE))
        .collect();
    assert!(found.is_empty(), "no observation survives the abort: {found:?}");
}

/// Recursively list every file under `root` — used to prove an artifact exists *nowhere*,
/// which a check against one expected directory cannot do.
fn walk_files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else { return out };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk_files(&p));
        } else {
            out.push(p);
        }
    }
    out
}

/// The frozen pre-registration is byte-identical: the observation is a NEW artifact beside
/// the run, never a field added to a governance file whose content hash is cited by its
/// own loader and by the judgment ledger (KTD8, R15).
#[test]
fn the_frozen_lineage_preregistration_is_untouched() {
    use sha2::{Digest, Sha256};
    let path = nautilus_ls_lab::lineage_prereg::frozen_lineage_prereg_path();
    let bytes = std::fs::read(&path).expect("the frozen artifact is committed");
    let digest = format!("{:x}", Sha256::digest(&bytes));
    assert_eq!(
        digest, "0ecd9d1163075edc28336035f511807e192b5d5c780e09340841ee81794b3dd4",
        "lineage-preregistration.json moved — its content hash is cited, so a new field \
         goes in a new artifact (R15)"
    );
}
