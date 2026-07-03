//! U3 offline integration: ingest against wiremock-served chart bodies into a real
//! `ParquetDataCatalog`, resumable via the checkpoint. Covers AE2 (resume without
//! refetch). No live calls.

use std::path::Path;

use chrono::NaiveDate;
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::ingest::checkpoint::Checkpoint;
use nautilus_ls::ingest::{BarKind, IngestConfig, Ingestor, DEFAULT_OVERLAP_DAYS};
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
        overlap_days: DEFAULT_OVERLAP_DAYS,
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
// Basis-shift detection + heal (data-fidelity U3, AE1/AE2). A dynamic t8410
// responder serves a mutable daily series — pre-shift, then rewritten to a
// post-split basis — so one server exercises detect → wipe → re-pull →
// re-verify → clear without re-mounting.
// ---------------------------------------------------------------------------

mod basis_shift_heal {
    use super::*;
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::{Arc, Mutex};

    use nautilus_ls::ingest::{delete_bar_series, read_all_bars};

    /// A daily close series: date (`YYYYMMDD`) → close. OHLC derives from the
    /// close, so rewriting a close rewrites the whole candle.
    type Series = BTreeMap<String, i64>;

    fn series(pairs: &[(&str, i64)]) -> Series {
        pairs.iter().map(|(d, c)| (d.to_string(), *c)).collect()
    }

    /// The server's series over time: each t8410 call consumes the front entry,
    /// and the last entry serves forever. `one()`/`set()` model a series the
    /// test rewrites between runs; `scripted()` models a gateway that rewrites
    /// the series again WHILE a heal is in flight (re-pull sees one basis, the
    /// re-verify another).
    #[derive(Clone)]
    struct SharedSeries(Arc<Mutex<VecDeque<Series>>>);

    impl SharedSeries {
        fn one(s: Series) -> Self {
            SharedSeries(Arc::new(Mutex::new(VecDeque::from([s]))))
        }
        fn scripted(list: Vec<Series>) -> Self {
            assert!(!list.is_empty());
            SharedSeries(Arc::new(Mutex::new(VecDeque::from(list))))
        }
        fn set(&self, s: Series) {
            let mut q = self.0.lock().unwrap();
            q.clear();
            q.push_back(s);
        }
        fn next(&self) -> Series {
            let mut q = self.0.lock().unwrap();
            if q.len() > 1 {
                q.pop_front().unwrap()
            } else {
                q.front().cloned().unwrap()
            }
        }
    }

    async fn sdk_with_series(server: &MockServer, shared: SharedSeries) -> LsSdk {
        mount_token(server).await;
        Mock::given(method("POST"))
            .and(path(CHART_PATH))
            .and(header("tr_cd", "t8410"))
            .respond_with(move |req: &wiremock::Request| {
                let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
                let ib = &body["t8410InBlock"];
                let s = ib["sdate"].as_str().unwrap_or("").to_string();
                let e = ib["edate"].as_str().unwrap_or("").to_string();
                let rows: Vec<serde_json::Value> = shared
                    .next()
                    .range(s..=e)
                    .map(|(d, c)| {
                        serde_json::json!({
                            "date": d, "open": (c - 5).to_string(), "high": (c + 10).to_string(),
                            "low": (c - 10).to_string(), "close": c.to_string(), "jdiff_vol": "1000"
                        })
                    })
                    .collect();
                json_response(serde_json::json!({
                    "rsp_cd": "00000", "rsp_msg": "정상",
                    "t8410OutBlock": { "shcode": "005930", "cts_date": "", "rec_count": rows.len().to_string() },
                    "t8410OutBlock1": rows
                }))
            })
            .mount(server)
            .await;
        LsSdk::new(mock_config(&server.uri())).expect("sdk builds")
    }

    const SAMSUNG: &str = "005930.XKRX";

    fn checkpoint_at(catalog: &Path) -> Checkpoint {
        Checkpoint::load(&catalog.join("ingest-checkpoint.json")).unwrap()
    }

    /// The stored closes, ascending by date, as plain integers.
    async fn stored_closes(catalog: &Path) -> Vec<i64> {
        let mut bars = read_all_bars(catalog).await.unwrap();
        bars.sort_by_key(|b| b.ts_event.as_u64());
        bars.iter().map(|b| b.close.to_string().parse().unwrap()).collect()
    }

    fn v1() -> Series {
        series(&[("20240103", 60000), ("20240104", 61800), ("20240105", 62000)])
    }

    /// v1 rewritten to a post-split basis (halved-ish) plus a new session.
    fn v2() -> Series {
        series(&[("20240103", 30000), ("20240104", 30900), ("20240105", 31000), ("20240108", 31500)])
    }

    /// Covers AE1: detect the rewritten basis, re-pull wholesale, record the
    /// re-base; a subsequent run detects nothing and is a no-op.
    #[tokio::test]
    async fn ae1_shift_is_detected_healed_and_recorded() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let shared = SharedSeries::one(v1());
        let sdk = sdk_with_series(&server, shared.clone()).await;
        let universe = [InstrumentId::from(SAMSUNG)];
        let floor = ymd(2024, 1, 1);

        let mut ing = Ingestor::new(sdk.clone(), daily_config(&catalog));
        let first = ing.run_accumulate(&universe, ymd(2024, 1, 5), floor).await.unwrap();
        assert_eq!(first.bars_written, 3);

        // The gateway rewrites S's whole history (post-split basis) for both the
        // overlap and the new dates.
        shared.set(v2());

        let mut ing2 = Ingestor::new(sdk.clone(), daily_config(&catalog));
        let report = ing2.run_accumulate(&universe, ymd(2024, 1, 8), floor).await.unwrap();
        assert_eq!(report.bars_written, 4, "wholesale re-pull replaced the series, not an append");
        assert!(report.heal_refusals.is_empty());

        // Final read-back: EVERY stored bar equals the v2 series — single basis.
        assert_eq!(stored_closes(&catalog).await, vec![30000, 30900, 31000, 31500]);

        let cp = checkpoint_at(&catalog);
        assert!(!cp.is_shifted(SAMSUNG, "1-DAY"), "mark cleared on completion");
        assert_eq!(cp.rebase_events().len(), 1, "the re-base is durably recorded (R5)");
        assert_eq!(cp.rebase_events()[0].instrument, SAMSUNG);
        assert_eq!(cp.rebase_events()[0].healed, "20240108");

        // A post-heal accumulate detects nothing and is a no-op.
        let calls_before = count_t8410(&server).await;
        let mut ing3 = Ingestor::new(sdk, daily_config(&catalog));
        let third = ing3.run_accumulate(&universe, ymd(2024, 1, 8), floor).await.unwrap();
        assert_eq!(third.triples_skipped, 1);
        assert_eq!(count_t8410(&server).await, calls_before, "no fetch when current post-heal");
    }

    /// Covers AE2: the mark is saved but the re-pull never ran (crash after
    /// detection) — the next accumulate re-enters at the wipe and heals; the
    /// mark outranks a current watermark.
    #[tokio::test]
    async fn ae2_interrupted_heal_resumes_at_the_wipe() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let shared = SharedSeries::one(v1());
        let sdk = sdk_with_series(&server, shared.clone()).await;
        let universe = [InstrumentId::from(SAMSUNG)];
        let floor = ymd(2024, 1, 1);

        let mut ing = Ingestor::new(sdk.clone(), daily_config(&catalog));
        ing.run_accumulate(&universe, ymd(2024, 1, 5), floor).await.unwrap();

        // Simulated interruption: the shifted mark was durably saved, then the
        // process died before the wipe/re-pull.
        let cp_path = catalog.join("ingest-checkpoint.json");
        let mut cp = Checkpoint::load(&cp_path).unwrap();
        cp.mark_shifted(SAMSUNG, "1-DAY", ymd(2024, 1, 5));
        cp.save(&cp_path).unwrap();
        shared.set(v2());

        // Between the runs the symbol is still marked — no backtest consumes it
        // as clean.
        assert!(checkpoint_at(&catalog).is_shifted(SAMSUNG, "1-DAY"));

        // The watermark is CURRENT (20240105 == last_closed) — the mark must
        // still heal (it outranks the watermark as authority, KTD-2).
        let mut ing2 = Ingestor::new(sdk.clone(), daily_config(&catalog));
        let report = ing2.run_accumulate(&universe, ymd(2024, 1, 5), floor).await.unwrap();
        assert_eq!(report.triples_skipped, 0, "a marked symbol is never skipped as current");
        assert_eq!(stored_closes(&catalog).await, vec![30000, 30900, 31000], "healed onto the served basis");
        let cp = checkpoint_at(&catalog);
        assert!(!cp.is_shifted(SAMSUNG, "1-DAY"));
        assert_eq!(cp.rebase_events().len(), 1);
    }

    /// Edge: a wiped-but-not-pulled interruption (bars already deleted, no
    /// watermark) also converges — re-entry restarts at the (no-op) wipe.
    #[tokio::test]
    async fn wiped_not_pulled_interruption_converges() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let shared = SharedSeries::one(v1());
        let sdk = sdk_with_series(&server, shared.clone()).await;
        let universe = [InstrumentId::from(SAMSUNG)];
        let floor = ymd(2024, 1, 1);

        let mut ing = Ingestor::new(sdk.clone(), daily_config(&catalog));
        ing.run_accumulate(&universe, ymd(2024, 1, 5), floor).await.unwrap();

        // Simulate: marked, wiped, watermark cleared — then crash.
        let cp_path = catalog.join("ingest-checkpoint.json");
        let mut cp = Checkpoint::load(&cp_path).unwrap();
        cp.mark_shifted(SAMSUNG, "1-DAY", ymd(2024, 1, 5));
        cp.clear_watermark(SAMSUNG, "1-DAY");
        cp.save(&cp_path).unwrap();
        let bar_type = BarKind::Daily.bar_type(InstrumentId::from(SAMSUNG)).unwrap();
        delete_bar_series(&catalog, bar_type).await.unwrap();
        shared.set(v2());

        let mut ing2 = Ingestor::new(sdk, daily_config(&catalog));
        ing2.run_accumulate(&universe, ymd(2024, 1, 8), floor).await.unwrap();
        assert_eq!(stored_closes(&catalog).await, vec![30000, 30900, 31000, 31500]);
        assert!(!checkpoint_at(&catalog).is_shifted(SAMSUNG, "1-DAY"));
    }

    /// Edge: one-sided dates (a server-side gap-fill and a gap/holiday day with
    /// no stored bar) do not detect a shift.
    #[tokio::test]
    async fn gap_days_and_gap_fills_do_not_detect() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        // Jan 4 has no bar historically (gap day).
        let holey = series(&[("20240102", 59000), ("20240103", 60000), ("20240105", 62000)]);
        let shared = SharedSeries::one(holey.clone());
        let sdk = sdk_with_series(&server, shared.clone()).await;
        let universe = [InstrumentId::from(SAMSUNG)];
        let floor = ymd(2024, 1, 1);

        let mut ing = Ingestor::new(sdk.clone(), daily_config(&catalog));
        ing.run_accumulate(&universe, ymd(2024, 1, 5), floor).await.unwrap();

        // The server now gap-fills Jan 4 and serves a new session — same basis.
        let mut filled = holey;
        filled.insert("20240104".to_string(), 61000);
        filled.insert("20240108".to_string(), 63000);
        shared.set(filled);

        let mut ing2 = Ingestor::new(sdk, daily_config(&catalog));
        let report = ing2.run_accumulate(&universe, ymd(2024, 1, 8), floor).await.unwrap();
        let cp = checkpoint_at(&catalog);
        assert!(!cp.is_shifted(SAMSUNG, "1-DAY"), "one-sided dates are not a shift");
        assert!(cp.rebase_events().is_empty());
        assert_eq!(report.bars_written, 1, "normal append of the new session only");
        assert_eq!(stored_closes(&catalog).await, vec![59000, 60000, 62000, 63000]);
    }

    /// Edge: a symbol with too short an overlap (fewer than the minimum mutual
    /// dates) skips detection and never marks — even when values differ.
    #[tokio::test]
    async fn short_overlap_skips_detection_and_never_marks() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let short = series(&[("20240104", 61800), ("20240105", 62000)]);
        let shared = SharedSeries::one(short);
        let sdk = sdk_with_series(&server, shared.clone()).await;
        let universe = [InstrumentId::from(SAMSUNG)];
        let floor = ymd(2024, 1, 1);

        let mut ing = Ingestor::new(sdk.clone(), daily_config(&catalog));
        ing.run_accumulate(&universe, ymd(2024, 1, 5), floor).await.unwrap();

        // Rewritten values on only two mutual dates — below the minimum.
        shared.set(series(&[("20240104", 30900), ("20240105", 31000), ("20240108", 31500)]));

        let mut ing2 = Ingestor::new(sdk, daily_config(&catalog));
        ing2.run_accumulate(&universe, ymd(2024, 1, 8), floor).await.unwrap();
        let cp = checkpoint_at(&catalog);
        assert!(!cp.is_shifted(SAMSUNG, "1-DAY"), "insufficient overlap never marks");
        assert!(cp.rebase_events().is_empty());
    }

    /// Edge (KTD-2 wipe precondition): a marked symbol whose run floor is later
    /// than its earliest stored bar refuses the wipe, stays marked, surfaces the
    /// refusal, and deletes nothing.
    #[tokio::test]
    async fn heal_refuses_a_floor_that_would_truncate_history() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let shared = SharedSeries::one(v1());
        let sdk = sdk_with_series(&server, shared.clone()).await;
        let universe = [InstrumentId::from(SAMSUNG)];

        let mut ing = Ingestor::new(sdk.clone(), daily_config(&catalog));
        ing.run_accumulate(&universe, ymd(2024, 1, 5), ymd(2024, 1, 1)).await.unwrap();

        let cp_path = catalog.join("ingest-checkpoint.json");
        let mut cp = Checkpoint::load(&cp_path).unwrap();
        cp.mark_shifted(SAMSUNG, "1-DAY", ymd(2024, 1, 5));
        cp.save(&cp_path).unwrap();

        // Run floor Jan 4 > earliest stored bar Jan 3 — the wipe must refuse.
        let mut ing2 = Ingestor::new(sdk, daily_config(&catalog));
        let report = ing2.run_accumulate(&universe, ymd(2024, 1, 8), ymd(2024, 1, 4)).await.unwrap();
        assert_eq!(report.heal_refusals.len(), 1, "the refusal is surfaced, never silent");
        assert_eq!(report.heal_refusals[0].floor, "20240104");
        assert_eq!(report.heal_refusals[0].earliest_stored, "20240103");
        assert!(checkpoint_at(&catalog).is_shifted(SAMSUNG, "1-DAY"), "stays marked");
        assert_eq!(stored_closes(&catalog).await, vec![60000, 61800, 62000], "no bars deleted");
    }

    /// Edge: a shallow-history shifted symbol (server serves fewer bars than the
    /// floor depth) still clears its mark — completion keys on the fetch cursor
    /// completing, never on bar count (KTD-3).
    #[tokio::test]
    async fn shallow_history_heal_clears_the_mark() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let shared = SharedSeries::one(v1());
        let sdk = sdk_with_series(&server, shared.clone()).await;
        let universe = [InstrumentId::from(SAMSUNG)];
        let floor = ymd(2024, 1, 1);

        let mut ing = Ingestor::new(sdk.clone(), daily_config(&catalog));
        ing.run_accumulate(&universe, ymd(2024, 1, 5), floor).await.unwrap();

        let cp_path = catalog.join("ingest-checkpoint.json");
        let mut cp = Checkpoint::load(&cp_path).unwrap();
        cp.mark_shifted(SAMSUNG, "1-DAY", ymd(2024, 1, 5));
        cp.save(&cp_path).unwrap();
        // The rewritten symbol now serves only two sessions (listed-late shape).
        shared.set(series(&[("20240105", 31000), ("20240108", 31500)]));

        let mut ing2 = Ingestor::new(sdk, daily_config(&catalog));
        ing2.run_accumulate(&universe, ymd(2024, 1, 8), floor).await.unwrap();
        let cp = checkpoint_at(&catalog);
        assert!(!cp.is_shifted(SAMSUNG, "1-DAY"), "cursor completed → mark cleared despite short history");
        assert_eq!(cp.rebase_events().len(), 1);
        assert_eq!(stored_closes(&catalog).await, vec![31000, 31500]);
    }

    /// Edge: the gateway rewrites the series AGAIN while the heal is in flight —
    /// the re-verify mismatches, the mark stays, and the next run heals again.
    #[tokio::test]
    async fn failed_reverify_keeps_the_mark_and_the_next_run_heals() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let v3 = series(&[("20240103", 15000), ("20240104", 15450), ("20240105", 15500), ("20240108", 15750)]);
        // Call order: run1 initial pull sees v1; the heal's re-pull sees v2; its
        // re-verify sees v3 (mismatch); every later call serves v3.
        let shared = SharedSeries::scripted(vec![v1(), v2(), v3.clone()]);
        let sdk = sdk_with_series(&server, shared).await;
        let universe = [InstrumentId::from(SAMSUNG)];
        let floor = ymd(2024, 1, 1);

        let mut ing = Ingestor::new(sdk.clone(), daily_config(&catalog));
        ing.run_accumulate(&universe, ymd(2024, 1, 5), floor).await.unwrap();

        let cp_path = catalog.join("ingest-checkpoint.json");
        let mut cp = Checkpoint::load(&cp_path).unwrap();
        cp.mark_shifted(SAMSUNG, "1-DAY", ymd(2024, 1, 5));
        cp.save(&cp_path).unwrap();

        // Heal attempt: re-pull v2, re-verify v3 → mismatch → stays marked.
        let mut ing2 = Ingestor::new(sdk.clone(), daily_config(&catalog));
        let report = ing2.run_accumulate(&universe, ymd(2024, 1, 8), floor).await.unwrap();
        assert!(checkpoint_at(&catalog).is_shifted(SAMSUNG, "1-DAY"), "failed re-verify keeps the mark");
        assert!(checkpoint_at(&catalog).rebase_events().is_empty());
        assert_eq!(report.gaps.len(), 1, "the incomplete heal is reported");

        // Next run re-enters at the wipe against the now-stable v3 and completes.
        let mut ing3 = Ingestor::new(sdk, daily_config(&catalog));
        ing3.run_accumulate(&universe, ymd(2024, 1, 8), floor).await.unwrap();
        let cp = checkpoint_at(&catalog);
        assert!(!cp.is_shifted(SAMSUNG, "1-DAY"));
        assert_eq!(cp.rebase_events().len(), 1);
        assert_eq!(stored_closes(&catalog).await, vec![15000, 15450, 15500, 15750]);
    }

    // --- epoch re-base mode (data-fidelity U4, R6/KTD-4) ---

    const HYNIX: &str = "000660.XKRX";

    /// A rebase over a small fixture universe re-pulls every symbol and ends
    /// with zero marks and one event per symbol.
    #[tokio::test]
    async fn epoch_rebase_heals_every_symbol_and_ends_clean() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let shared = SharedSeries::one(v1());
        let sdk = sdk_with_series(&server, shared.clone()).await;
        let universe = [InstrumentId::from(SAMSUNG), InstrumentId::from(HYNIX)];
        let floor = ymd(2024, 1, 1);

        let mut ing = Ingestor::new(sdk.clone(), daily_config(&catalog));
        ing.run_accumulate(&universe, ymd(2024, 1, 5), floor).await.unwrap();

        // The pre-epoch catalog may hold years of baked-in splices — the server
        // now sits on a different basis, undetectable forward-only.
        shared.set(v2());

        let mut ing2 = Ingestor::new(sdk.clone(), daily_config(&catalog));
        let report = ing2.run_rebase(&universe, ymd(2024, 1, 8), floor).await.unwrap();
        assert_eq!(report.triples_ingested, 2, "every symbol was re-pulled");
        assert_eq!(report.bars_written, 8);

        let cp = checkpoint_at(&catalog);
        assert!(!cp.is_shifted(SAMSUNG, "1-DAY"));
        assert!(!cp.is_shifted(HYNIX, "1-DAY"));
        assert_eq!(cp.rebase_events().len(), 2, "one event per symbol");
        assert_eq!(stored_closes(&catalog).await, vec![30000, 30000, 30900, 30900, 31000, 31000, 31500, 31500]);

        // A post-epoch accumulate detects nothing.
        let calls_before = count_t8410(&server).await;
        let mut ing3 = Ingestor::new(sdk, daily_config(&catalog));
        let third = ing3.run_accumulate(&universe, ymd(2024, 1, 8), floor).await.unwrap();
        assert_eq!(third.triples_skipped, 2);
        assert_eq!(count_t8410(&server).await, calls_before);
    }

    /// An interrupted epoch (marks persist for un-healed symbols) resumes on the
    /// next accumulate run and heals only the remainder.
    #[tokio::test]
    async fn interrupted_epoch_resumes_and_heals_only_the_remainder() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let shared = SharedSeries::one(v1());
        let sdk = sdk_with_series(&server, shared.clone()).await;
        let universe = [InstrumentId::from(SAMSUNG), InstrumentId::from(HYNIX)];
        let floor = ymd(2024, 1, 1);

        let mut ing = Ingestor::new(sdk.clone(), daily_config(&catalog));
        ing.run_accumulate(&universe, ymd(2024, 1, 5), floor).await.unwrap();
        shared.set(v2());

        // Simulated interruption: the epoch's atomic mark-all save landed, then
        // only SAMSUNG healed before the crash (drive it via a one-symbol run).
        let cp_path = catalog.join("ingest-checkpoint.json");
        let mut cp = Checkpoint::load(&cp_path).unwrap();
        cp.mark_shifted(SAMSUNG, "1-DAY", ymd(2024, 1, 8));
        cp.mark_shifted(HYNIX, "1-DAY", ymd(2024, 1, 8));
        cp.save(&cp_path).unwrap();
        let mut ing2 = Ingestor::new(sdk.clone(), daily_config(&catalog));
        ing2.run_accumulate(&universe[..1], ymd(2024, 1, 8), floor).await.unwrap();
        assert!(checkpoint_at(&catalog).is_shifted(HYNIX, "1-DAY"), "the remainder is still marked");

        // Resume: heals only HYNIX (SAMSUNG is clean and current).
        let mut ing3 = Ingestor::new(sdk, daily_config(&catalog));
        let resumed = ing3.run_accumulate(&universe, ymd(2024, 1, 8), floor).await.unwrap();
        assert_eq!(resumed.triples_ingested, 1, "only the un-healed symbol is re-pulled");
        let cp = checkpoint_at(&catalog);
        assert!(!cp.is_shifted(SAMSUNG, "1-DAY"));
        assert!(!cp.is_shifted(HYNIX, "1-DAY"));
        assert_eq!(cp.rebase_events().len(), 2, "one event per symbol, none doubled");
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
        overlap_days: DEFAULT_OVERLAP_DAYS,
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
