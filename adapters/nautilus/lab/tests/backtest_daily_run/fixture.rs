//! The offline catalog fixture and the ranking scripts every scenario in this suite
//! builds on. Split out of the crate root so the scenario bodies read as scenarios; the
//! scaffold is still the deliberate duplicate of `backtest_run.rs`'s described in the
//! root module doc, not a shared support crate.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use chrono::{DateTime, NaiveDate, Utc};
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::ingest::checkpoint::Checkpoint;
use nautilus_ls::ingest::{build_daily_bar, write_bars, write_instruments, BarKind};
use nautilus_ls::instruments::{InstrumentDomain, InstrumentProvider};
use nautilus_ls_lab::runner::backtest_daily::DailyBacktestConfig;
use nautilus_ls_lab::strategy::orb::UniverseCandidate;
use nautilus_model::data::Bar;
use nautilus_model::identifiers::InstrumentId;
use serde_json::json;
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

pub(crate) fn daily_json(date: &str, o: &str, h: &str, l: &str, c: &str, v: &str) -> serde_json::Value {
    json!({ "date": date, "open": o, "high": h, "low": l, "close": c, "jdiff_vol": v,
        "value": "0", "jongchk": "0", "rate": "0", "pricechk": "0", "ratevalue": "0", "sign": "0" })
}

pub(crate) async fn write_daily_series(catalog: &Path, id: &str, rows: &[serde_json::Value]) {
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
pub(crate) const SESSION_DAYS: [&str; 22] = [
    "20240102", "20240103", "20240104", "20240105", "20240108", "20240109", "20240110",
    "20240111", "20240112", "20240115", "20240116", "20240117", "20240118", "20240119",
    "20240122", "20240123", "20240124", "20240125", "20240126", "20240129", "20240130",
    "20240131",
];

/// The pinned range: session index 1 through 21 (0 is the out-of-range prior).
pub(crate) const RANGE_START: &str = "20240103";
pub(crate) const RANGE_END: &str = "20240131";

/// A hand-chained flat daily series long enough to reach hold expiry: a constant
/// close with a small per-session drift, so nothing ever breaches a stop placed a
/// long way below. `low_overrides` injects a crash low on a chosen session index;
/// `gaps` removes a session's bar entirely (a halt, a suspension, or an incomplete
/// ingest — the symbol keeps trading afterwards).
///
/// Every price is a multiple of 100 — the KRX instrument masters this fixture
/// ingests carry `price_increment = 100`, and the matching engine *skips the fill*
/// (a WARN, not an error) for any price off that grid, so an off-grid fixture
/// silently trades nothing.
fn flat_series(
    base: i64,
    low_overrides: &HashMap<usize, i64>,
    gaps: &BTreeSet<usize>,
) -> Vec<serde_json::Value> {
    SESSION_DAYS
        .iter()
        .enumerate()
        .filter(|(i, _)| !gaps.contains(i))
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
pub(crate) async fn build_daily_fixture(
    data_home: &Path,
    crash: &HashMap<&str, HashMap<usize, i64>>,
) {
    build_daily_fixture_with_gaps(data_home, crash, &HashMap::new()).await;
}

/// [`build_daily_fixture`] with per-symbol data gaps: the `SESSION_DAYS` indices a
/// symbol carries no bar on at all.
pub(crate) async fn build_daily_fixture_with_gaps(
    data_home: &Path,
    crash: &HashMap<&str, HashMap<usize, i64>>,
    gaps: &HashMap<&str, BTreeSet<usize>>,
) {
    let catalog = data_home.join("catalog");
    write_masters(&catalog).await;
    let empty = HashMap::new();
    let no_gaps = BTreeSet::new();
    for (id, base) in [("005930.XKRX", 60_000i64), ("000660.XKRX", 50_000)] {
        write_daily_series(
            &catalog,
            id,
            &flat_series(
                base,
                crash.get(id).unwrap_or(&empty),
                gaps.get(id).unwrap_or(&no_gaps),
            ),
        )
        .await;
    }
    let mut cp = Checkpoint::default();
    cp.adjusted_prices = true;
    cp.save(&catalog.join("ingest-checkpoint.json")).unwrap();
}

pub(crate) fn cfg(data_home: &Path, target_m: usize) -> DailyBacktestConfig {
    DailyBacktestConfig::new(data_home, RANGE_START, RANGE_END, target_m)
}

/// The KST calendar date of a UTC-nanosecond timestamp (KST is UTC+9, no DST).
pub(crate) fn kst_date(ns: u64) -> NaiveDate {
    let dt = DateTime::<Utc>::from_timestamp_nanos(ns as i64);
    (dt + chrono::Duration::hours(9)).date_naive()
}

// ---------------------------------------------------------------------------
// Rankers (the daily selection RULE is the caller's — KTD15)
// ---------------------------------------------------------------------------

/// Rank every candidate by prior turnover descending, symbol-ascending on ties.
pub(crate) fn rank_all(candidates: &[UniverseCandidate]) -> Vec<String> {
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
pub(crate) fn rank_only(only: &'static [&'static str]) -> impl Fn(&[UniverseCandidate]) -> Vec<String> {
    move |candidates: &[UniverseCandidate]| {
        only.iter()
            .filter(|s| candidates.iter().any(|c| c.symbol == **s))
            .map(|s| (*s).to_string())
            .collect()
    }
}
