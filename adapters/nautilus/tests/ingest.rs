//! U3 offline integration: ingest against wiremock-served chart bodies into a real
//! `ParquetDataCatalog`, resumable via the checkpoint. Covers AE2 (resume without
//! refetch). No live calls.

use std::path::Path;

use chrono::NaiveDate;
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::ingest::checkpoint::Checkpoint;
use nautilus_ls::ingest::{BarKind, IngestConfig, Ingestor};
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_model::identifiers::InstrumentId;
use tempfile::tempdir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

const CHART_PATH: &str = "/stock/chart";

fn json_response(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_string(body.to_string())
        .insert_header("content-type", "application/json")
}

/// A single-page daily response (cts_date "" terminates the cursor) with three
/// ascending candles.
fn daily_body_three_rows() -> serde_json::Value {
    serde_json::json!({
        "rsp_cd": "00000", "rsp_msg": "정상",
        "t8410OutBlock": { "shcode": "005930", "cts_date": "", "rec_count": "3" },
        "t8410OutBlock1": [
            { "date": "20240103", "open": "60000", "high": "61000", "low": "59500", "close": "60500", "jdiff_vol": "1000000" },
            { "date": "20240104", "open": "60500", "high": "62000", "low": "60000", "close": "61800", "jdiff_vol": "1200000" },
            { "date": "20240105", "open": "61800", "high": "62500", "low": "61000", "close": "62000", "jdiff_vol": "900000" }
        ]
    })
}

/// A single-page daily response with no candles (short/empty history).
fn daily_body_empty() -> serde_json::Value {
    serde_json::json!({
        "rsp_cd": "00000", "rsp_msg": "정상",
        "t8410OutBlock": { "shcode": "005930", "cts_date": "", "rec_count": "0" },
        "t8410OutBlock1": []
    })
}

async fn sdk_over(server: &MockServer, body: serde_json::Value) -> LsSdk {
    mount_token(server).await;
    Mock::given(method("POST"))
        .and(path(CHART_PATH))
        .and(header("tr_cd", "t8410"))
        .respond_with(json_response(body))
        .mount(server)
        .await;
    LsSdk::new(mock_config(&server.uri())).expect("sdk builds")
}

fn daily_config(catalog: &Path) -> IngestConfig {
    IngestConfig {
        catalog_path: catalog.to_path_buf(),
        bar_kinds: vec![BarKind::Daily],
        sdate: "20240101".to_string(),
        edate: "20240105".to_string(),
        adjusted_prices: true,
    }
}

async fn count_t8410(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| {
            r.url.path() == CHART_PATH
                && r.headers
                    .get("tr_cd")
                    .and_then(|v| v.to_str().ok())
                    == Some("t8410")
        })
        .count()
}

#[tokio::test]
async fn ingests_daily_bars_and_round_trips_through_catalog() {
    let dir = tempdir().unwrap();
    let catalog_path = dir.path().join("catalog");
    let server = MockServer::start().await;
    let sdk = sdk_over(&server, daily_body_three_rows()).await;

    let mut ingestor = Ingestor::new(sdk, daily_config(&catalog_path));
    let report = ingestor
        .run(&[InstrumentId::from("005930.XKRX")])
        .await
        .expect("ingest runs");
    assert_eq!(report.bars_written, 3);
    assert_eq!(report.triples_ingested, 1);

    // Round-trip: read the bars back and assert ts_event is monotonic ascending.
    let bars = nautilus_ls::ingest::read_all_bars(&catalog_path)
        .await
        .expect("read bars back");
    assert_eq!(bars.len(), 3, "all three candles are persisted + readable");
    for w in bars.windows(2) {
        assert!(
            w[0].ts_event.as_u64() <= w[1].ts_event.as_u64(),
            "bars ordered by ts_event"
        );
    }
}

/// Covers AE2: an interrupted/repeated run resumes after the already-ingested data
/// and issues no fresh gateway requests for done symbols.
#[tokio::test]
async fn resume_skips_done_symbols_without_refetch() {
    let dir = tempdir().unwrap();
    let catalog_path = dir.path().join("catalog");
    let server = MockServer::start().await;
    let sdk = sdk_over(&server, daily_body_three_rows()).await;
    let universe = [
        InstrumentId::from("005930.XKRX"),
        InstrumentId::from("000660.XKRX"),
    ];

    // First run: two symbols → two t8410 requests.
    let mut ingestor = Ingestor::new(sdk.clone(), daily_config(&catalog_path));
    let first = ingestor.run(&universe).await.unwrap();
    assert_eq!(first.triples_ingested, 2);
    let after_first = count_t8410(&server).await;
    assert_eq!(after_first, 2, "one page per symbol on the first run");

    // Second run over the SAME catalog: everything is checkpoint-done → skipped,
    // and NO new requests reach the gateway.
    let mut ingestor2 = Ingestor::new(sdk, daily_config(&catalog_path));
    let second = ingestor2.run(&universe).await.unwrap();
    assert_eq!(second.triples_skipped, 2, "both symbols skipped on resume");
    assert_eq!(second.bars_written, 0);
    assert_eq!(count_t8410(&server).await, after_first, "no refetch on resume");
}

#[tokio::test]
async fn empty_history_records_a_gap_without_failing() {
    let dir = tempdir().unwrap();
    let catalog_path = dir.path().join("catalog");
    let server = MockServer::start().await;
    let sdk = sdk_over(&server, daily_body_empty()).await;

    let mut ingestor = Ingestor::new(sdk, daily_config(&catalog_path));
    let report = ingestor
        .run(&[InstrumentId::from("005930.XKRX")])
        .await
        .expect("empty history does not fail the run");
    assert_eq!(report.bars_written, 0);
    assert_eq!(report.gaps.len(), 1, "a coverage gap is recorded");
}

#[tokio::test]
async fn adjusted_price_flag_lands_in_checkpoint_metadata() {
    let dir = tempdir().unwrap();
    let catalog_path = dir.path().join("catalog");
    let server = MockServer::start().await;
    let sdk = sdk_over(&server, daily_body_three_rows()).await;

    let mut ingestor = Ingestor::new(sdk, daily_config(&catalog_path));
    ingestor
        .run(&[InstrumentId::from("005930.XKRX")])
        .await
        .unwrap();

    let cp = Checkpoint::load(&catalog_path.join("ingest-checkpoint.json")).unwrap();
    assert!(cp.adjusted_prices, "sujung=Y basis recorded in the checkpoint");
}

/// AE4: accumulate-forward run twice with coverage current → the second run makes
/// ZERO bar fetches (the watermark is the sole skip authority).
#[tokio::test]
async fn accumulate_second_run_is_a_noop() {
    let dir = tempdir().unwrap();
    let catalog_path = dir.path().join("catalog");
    let server = MockServer::start().await;
    let sdk = sdk_over(&server, daily_body_three_rows()).await;
    let universe = [InstrumentId::from("005930.XKRX")];
    let last_closed = ymd(2024, 1, 5);
    let floor = ymd(2024, 1, 1);

    let mut ingestor = Ingestor::new(sdk.clone(), daily_config(&catalog_path));
    let first = ingestor.run_accumulate(&universe, last_closed, floor).await.unwrap();
    assert_eq!(first.triples_ingested, 1, "first run covers the range");
    let after_first = count_t8410(&server).await;
    assert!(after_first >= 1);

    // Second run, same last-closed session → already current → no bar fetch.
    let mut ingestor2 = Ingestor::new(sdk, daily_config(&catalog_path));
    let second = ingestor2.run_accumulate(&universe, last_closed, floor).await.unwrap();
    assert_eq!(second.triples_skipped, 1, "already current → skipped");
    assert_eq!(second.bars_written, 0);
    assert_eq!(count_t8410(&server).await, after_first, "no refetch when current (AE4)");
}

/// AE5: a symbol newly listed since the last run enters the universe and begins
/// coverage at the lookback floor.
#[tokio::test]
async fn accumulate_new_instrument_begins_coverage() {
    let dir = tempdir().unwrap();
    let catalog_path = dir.path().join("catalog");
    let server = MockServer::start().await;
    let sdk = sdk_over(&server, daily_body_three_rows()).await;
    let last_closed = ymd(2024, 1, 5);
    let floor = ymd(2024, 1, 1);

    // Run 1: only 005930 exists.
    let mut ingestor = Ingestor::new(sdk.clone(), daily_config(&catalog_path));
    ingestor
        .run_accumulate(&[InstrumentId::from("005930.XKRX")], last_closed, floor)
        .await
        .unwrap();

    // Run 2: a newly-listed 000660 appears → it begins coverage; 005930 is current.
    let universe = [InstrumentId::from("005930.XKRX"), InstrumentId::from("000660.XKRX")];
    let mut ingestor2 = Ingestor::new(sdk, daily_config(&catalog_path));
    let second = ingestor2.run_accumulate(&universe, last_closed, floor).await.unwrap();
    assert_eq!(second.triples_ingested, 1, "only the newly-listed symbol is fetched");
    assert_eq!(second.triples_skipped, 1, "the already-covered symbol is skipped");
}

/// A gap-reason triple (empty history) advances the watermark and is reported once,
/// not retried forever.
#[tokio::test]
async fn accumulate_gap_advances_watermark_and_reports_once() {
    let dir = tempdir().unwrap();
    let catalog_path = dir.path().join("catalog");
    let server = MockServer::start().await;
    let sdk = sdk_over(&server, daily_body_empty()).await;
    let universe = [InstrumentId::from("005930.XKRX")];
    let last_closed = ymd(2024, 1, 5);
    let floor = ymd(2024, 1, 1);

    let mut ingestor = Ingestor::new(sdk.clone(), daily_config(&catalog_path));
    let first = ingestor.run_accumulate(&universe, last_closed, floor).await.unwrap();
    assert_eq!(first.gaps.len(), 1, "the empty history is reported as a gap");
    let after_first = count_t8410(&server).await;

    // The watermark advanced to last_closed → a re-run at the same session skips.
    let mut ingestor2 = Ingestor::new(sdk, daily_config(&catalog_path));
    let second = ingestor2.run_accumulate(&universe, last_closed, floor).await.unwrap();
    assert_eq!(second.triples_skipped, 1, "the gap day is not retried forever");
    assert_eq!(count_t8410(&server).await, after_first, "no refetch of the gap day");
}

#[tokio::test]
async fn ingest_refuses_to_start_while_live_lock_held() {
    let dir = tempdir().unwrap();
    let catalog_path = dir.path().join("catalog");
    std::fs::create_dir_all(&catalog_path).unwrap();
    let server = MockServer::start().await;
    let sdk = sdk_over(&server, daily_body_three_rows()).await;

    // Hold the live-session lock, then attempt a locked ingest run.
    let _live = AdvisoryLock::acquire(&catalog_path, LockKind::Live).unwrap();
    let mut ingestor = Ingestor::new(sdk, daily_config(&catalog_path));
    let err = ingestor
        .run_locked(&[InstrumentId::from("005930.XKRX")])
        .await
        .expect_err("ingest must refuse while a live session is running");
    assert!(err.to_string().contains("mutually exclusive"));
}

// ---------------------------------------------------------------------------
// Per-symbol delete + scoped read (data-fidelity U2) — the heal's wipe and
// overlap-compare primitives. The wipe must TRUE-delete (files gone, not
// overwritten) and stay scoped to one bar type / one symbol.
// ---------------------------------------------------------------------------

mod catalog_primitives {
    use super::*;
    use nautilus_core::UnixNanos;
    use nautilus_ls::ingest::{
        delete_bar_series, kst_to_unix_nanos, read_all_bars, read_bars_scoped, write_bars,
    };
    use nautilus_ls::rules::KRX_REGULAR_CLOSE;
    use nautilus_model::data::{Bar, BarType};
    use nautilus_model::types::{Price, Quantity};

    fn daily_bar(bar_type: BarType, date: NaiveDate, close: i64) -> Bar {
        let ts = kst_to_unix_nanos(date, KRX_REGULAR_CLOSE).unwrap();
        Bar::new(
            bar_type,
            Price::from((close - 5).to_string().as_str()),
            Price::from((close + 10).to_string().as_str()),
            Price::from((close - 10).to_string().as_str()),
            Price::from(close.to_string().as_str()),
            Quantity::from(1000),
            ts,
            ts,
        )
    }

    fn minute_bar(bar_type: BarType, date: NaiveDate, hh: u32, mm: u32, close: i64) -> Bar {
        let ts =
            kst_to_unix_nanos(date, chrono::NaiveTime::from_hms_opt(hh, mm, 0).unwrap()).unwrap();
        Bar::new(
            bar_type,
            Price::from(close.to_string().as_str()),
            Price::from((close + 1).to_string().as_str()),
            Price::from((close - 1).to_string().as_str()),
            Price::from(close.to_string().as_str()),
            Quantity::from(10),
            ts,
            ts,
        )
    }

    #[tokio::test]
    async fn delete_wipes_one_symbols_daily_series_and_nothing_else() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let samsung = InstrumentId::from("005930.XKRX");
        let hynix = InstrumentId::from("000660.XKRX");
        let samsung_daily = BarKind::Daily.bar_type(samsung).unwrap();
        let samsung_minute = BarKind::Minute(1).bar_type(samsung).unwrap();
        let hynix_daily = BarKind::Daily.bar_type(hynix).unwrap();

        write_bars(
            &catalog,
            vec![
                daily_bar(samsung_daily, ymd(2024, 1, 3), 60000),
                daily_bar(samsung_daily, ymd(2024, 1, 4), 60500),
            ],
        )
        .await
        .unwrap();
        write_bars(&catalog, vec![minute_bar(samsung_minute, ymd(2024, 1, 3), 9, 1, 60100)])
            .await
            .unwrap();
        write_bars(&catalog, vec![daily_bar(hynix_daily, ymd(2024, 1, 3), 130000)])
            .await
            .unwrap();

        delete_bar_series(&catalog, samsung_daily).await.unwrap();

        let remaining = read_all_bars(&catalog).await.unwrap();
        assert_eq!(remaining.len(), 2, "only the wiped series is gone");
        assert!(
            remaining.iter().all(|b| b.bar_type != samsung_daily),
            "no samsung daily bar survives the wipe"
        );
        assert!(remaining.iter().any(|b| b.bar_type == samsung_minute), "minute bars intact (KTD-8)");
        assert!(remaining.iter().any(|b| b.bar_type == hynix_daily), "other symbols intact");
    }

    #[tokio::test]
    async fn delete_of_an_unstored_series_is_a_noop_ok() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        std::fs::create_dir_all(&catalog).unwrap();
        let bar_type = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        delete_bar_series(&catalog, bar_type)
            .await
            .expect("deleting a series with no stored bars is Ok");
    }

    #[tokio::test]
    async fn scoped_read_returns_only_the_window_of_one_series() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let samsung = InstrumentId::from("005930.XKRX");
        let samsung_daily = BarKind::Daily.bar_type(samsung).unwrap();
        let hynix_daily = BarKind::Daily.bar_type(InstrumentId::from("000660.XKRX")).unwrap();

        write_bars(
            &catalog,
            vec![
                daily_bar(samsung_daily, ymd(2024, 1, 3), 60000),
                daily_bar(samsung_daily, ymd(2024, 1, 4), 60500),
                daily_bar(samsung_daily, ymd(2024, 1, 5), 62000),
            ],
        )
        .await
        .unwrap();
        write_bars(&catalog, vec![daily_bar(hynix_daily, ymd(2024, 1, 4), 130000)])
            .await
            .unwrap();

        // Window covering only Jan 4–5.
        let start = kst_to_unix_nanos(ymd(2024, 1, 4), chrono::NaiveTime::MIN).unwrap();
        let bars = read_bars_scoped(&catalog, samsung_daily, Some(start), Some(UnixNanos::from(u64::MAX)))
            .await
            .unwrap();
        assert_eq!(bars.len(), 2, "only the windowed samsung dailies");
        assert!(bars.iter().all(|b| b.bar_type == samsung_daily));
        assert!(bars.iter().all(|b| b.ts_event >= start));
    }
}

// ---------------------------------------------------------------------------
// U7 — max-lookback probe (KTD10, R10). A dynamic t8412 responder serves minute
// rows only for dates at/after a known earliest served date, so the windowed
// backward search must converge on that date without being derailed by the
// weekends/holidays inside each ≥7-day window.
// ---------------------------------------------------------------------------

use chrono::{Datelike, Duration as ChronoDuration};
use nautilus_ls::ingest::{probes_dir_for, read_minute_lookback, write_minute_lookback, MinuteLookback};

/// Mount a t8412 responder that serves one weekday row per date in
/// `[max(sdate, earliest), edate]` — an all-empty window means the request range is
/// entirely before `earliest` (beyond lookback).
async fn sdk_with_probe(server: &MockServer, earliest: NaiveDate) -> LsSdk {
    mount_token(server).await;
    Mock::given(method("POST"))
        .and(path(CHART_PATH))
        .and(header("tr_cd", "t8412"))
        .respond_with(move |req: &wiremock::Request| {
            let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
            let ib = &body["t8412InBlock"];
            let s = NaiveDate::parse_from_str(ib["sdate"].as_str().unwrap_or(""), "%Y%m%d").unwrap();
            let e = NaiveDate::parse_from_str(ib["edate"].as_str().unwrap_or(""), "%Y%m%d").unwrap();
            let mut rows = Vec::new();
            let mut d = s.max(earliest);
            while d <= e {
                // Weekdays only — weekends inside the window are non-trading gaps.
                if d.weekday().num_days_from_monday() < 5 {
                    rows.push(serde_json::json!({
                        "date": d.format("%Y%m%d").to_string(), "time": "0900",
                        "open": "1000", "high": "1010", "low": "990", "close": "1005",
                        "jdiff_vol": "100", "value": "0", "jongchk": "0", "rate": "0", "sign": "0"
                    }));
                }
                d += ChronoDuration::days(1);
            }
            json_response(serde_json::json!({ "rsp_cd": "00000", "t8412OutBlock1": rows }))
        })
        .mount(server)
        .await;
    LsSdk::new(mock_config(&server.uri())).expect("sdk builds")
}

fn probe_config(catalog: &Path) -> IngestConfig {
    IngestConfig {
        catalog_path: catalog.to_path_buf(),
        bar_kinds: vec![BarKind::Minute(1)],
        sdate: String::new(),
        edate: String::new(),
        adjusted_prices: true,
    }
}

#[tokio::test]
async fn probe_converges_on_earliest_served_date_and_writes_file() {
    let dir = tempdir().unwrap();
    let data = dir.path().join("data");
    let catalog = data.join("catalog");
    let server = MockServer::start().await;
    let earliest = ymd(2024, 1, 10);
    let sdk = sdk_with_probe(&server, earliest).await;

    let ingestor = Ingestor::new(sdk, probe_config(&catalog));
    let anchor = ymd(2024, 1, 31);
    let out = ingestor
        .run_probe_lookback("005930", 1, anchor, "2024-01-31T16:30:00+09:00".into())
        .await
        .unwrap()
        .expect("the pilot serves history");
    assert_eq!(out.earliest_date, "20240110", "converges on the earliest served date");
    assert_eq!(out.depth_days, 21, "depth = anchor − earliest");

    // The result was written to <data>/probes/minute-lookback.json and round-trips.
    let read = read_minute_lookback(&probes_dir_for(&catalog)).unwrap();
    assert_eq!(read, out);
}

#[tokio::test]
async fn probe_reports_nothing_when_pilot_serves_no_history() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("data").join("catalog");
    let server = MockServer::start().await;
    // Earliest is in the far future → every window is empty.
    let sdk = sdk_with_probe(&server, ymd(2030, 1, 1)).await;

    let ingestor = Ingestor::new(sdk, probe_config(&catalog));
    let out = ingestor
        .run_probe_lookback("005930", 1, ymd(2024, 1, 31), "2024-01-31T16:30:00+09:00".into())
        .await
        .unwrap();
    assert!(out.is_none(), "an empty pilot records nothing");
    // No file was written.
    assert!(read_minute_lookback(&probes_dir_for(&catalog)).is_err());
}

#[test]
fn minute_lookback_file_round_trips() {
    let dir = tempdir().unwrap();
    let probes = dir.path().join("probes");
    let lb = MinuteLookback {
        earliest_date: "20220103".into(),
        depth_days: 730,
        probed_at: "2024-01-31T16:30:00+09:00".into(),
    };
    write_minute_lookback(&probes, &lb).unwrap();
    assert_eq!(read_minute_lookback(&probes).unwrap(), lb);
}
