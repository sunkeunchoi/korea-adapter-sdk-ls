//! Catalog scaffold — duplicated, not shared: see the `strategy_daily` module doc for
//! why (each `lab/tests/*.rs` is its own binary and there is no shared test-support
//! module), and for the two fixture facts that look exactly like logic bugs.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use chrono::{DateTime, NaiveDate, Utc};
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::ingest::checkpoint::Checkpoint;
use nautilus_ls::ingest::{build_daily_bar, write_bars, write_instruments, BarKind};
use nautilus_ls::instruments::{InstrumentDomain, InstrumentProvider};
use nautilus_ls_lab::params_daily::FROZEN_ATR_WINDOW_SESSIONS;
use nautilus_ls_lab::runner::backtest_daily::DailyBacktestConfig;
use nautilus_model::data::Bar;
use nautilus_model::identifiers::InstrumentId;
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The KST weekdays the fixtures use. Indices 0 and 1 are **pre-range** — ATR(1)
/// needs two prior sessions strictly before the session date, so a range starting at
/// index 0 or 1 would resolve every prior ATR to `None` and refuse every entry.
pub(crate) const SESSION_DAYS: [&str; 24] = [
    "20240102", "20240103", // pre-range (the ATR(1) lookback)
    "20240104", "20240105", "20240108", "20240109", "20240110", "20240111", "20240112",
    "20240115", "20240116", "20240117", "20240118", "20240119", "20240122", "20240123",
    "20240124", "20240125", "20240126", "20240129", "20240130", "20240131", "20240201",
    "20240202",
];

/// The first session index a pinned range may start on (see [`SESSION_DAYS`]).
pub(crate) const FIRST_IN_RANGE: usize = 2;

/// Real KRX common-share codes (every one ends in the `0` issue-sequence digit, so
/// nothing here reads as a preferred share).
pub(crate) const CODES: [&str; 12] = [
    "005930", "000660", "035420", "035720", "051910", "006400", "005380", "000270", "068270",
    "207940", "012330", "028260",
];

/// One fixture symbol's daily series.
#[derive(Debug, Clone)]
pub(crate) struct SymbolSpec {
    /// The 6-digit KRX code; the instrument id is `{code}.XKRX`.
    pub(crate) code: &'static str,
    /// The base close in KRW. The series drifts `+100` per session, so **every**
    /// price stays on the masters' 100 KRW grid.
    pub(crate) base: i64,
    /// Daily volume. `prior_turnover = prior_close × prior_volume`, so this is what
    /// orders the placeholder ranking signal.
    pub(crate) volume: i64,
    /// `SESSION_DAYS` index → that session's low, overriding the default `close − 500`.
    pub(crate) lows: HashMap<usize, i64>,
    /// The first `SESSION_DAYS` index that carries a bar — a symbol that starts
    /// inside the range has no derivable prior ATR on its first candidate session.
    pub(crate) first_session: usize,
    /// `SESSION_DAYS` indices this symbol carries **no bar at all** on — the data gap
    /// a KRX trading halt, a suspension, or an incomplete ingest leaves behind. The
    /// symbol keeps trading afterwards, so this is a hole in the series rather than a
    /// truncation.
    pub(crate) gaps: BTreeSet<usize>,
    /// A limit-locked series: `O = H = L = C` on every session, so ATR(1) is exactly
    /// zero — *available*, and it passes an `is_some` check (KTD9).
    pub(crate) locked: bool,
}

impl SymbolSpec {
    pub(crate) fn new(code: &'static str, base: i64, volume: i64) -> Self {
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

    pub(crate) fn id(&self) -> InstrumentId {
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

pub(crate) fn daily_json(date: &str, o: i64, h: i64, l: i64, c: i64, v: i64) -> serde_json::Value {
    json!({ "date": date, "open": o.to_string(), "high": h.to_string(), "low": l.to_string(),
        "close": c.to_string(), "jdiff_vol": v.to_string(),
        "value": "0", "jongchk": "0", "rate": "0", "pricechk": "0", "ratevalue": "0", "sign": "0" })
}

pub(crate) fn daily_bar(id: InstrumentId, row: serde_json::Value) -> Bar {
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
pub(crate) fn series(spec: &SymbolSpec) -> Vec<serde_json::Value> {
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
pub(crate) async fn build_fixture(data_home: &Path, specs: &[SymbolSpec]) {
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
pub(crate) fn cfg_range(dir: &Path, from: usize, to: usize, target_m: usize) -> DailyBacktestConfig {
    let mut c = DailyBacktestConfig::new(dir, SESSION_DAYS[from], SESSION_DAYS[to], target_m);
    c.params.atr_window = FROZEN_ATR_WINDOW_SESSIONS;
    c
}

/// `n` symbols on a common price series with **strictly descending** turnover, so
/// the placeholder ranking signal's order is exactly `CODES[0..n]`.
pub(crate) fn descending_turnover(n: usize) -> Vec<SymbolSpec> {
    (0..n)
        .map(|i| SymbolSpec::new(CODES[i], 50_000, ((n - i) as i64) * 100_000))
        .collect()
}

/// The KST calendar date of a UTC-nanosecond timestamp (KST is UTC+9, no DST).
pub(crate) fn kst_date(ns: u64) -> NaiveDate {
    let dt = DateTime::<Utc>::from_timestamp_nanos(ns as i64);
    (dt + chrono::Duration::hours(9)).date_naive()
}
