//! U3 offline integration: ingest against wiremock-served chart bodies into a real
//! `ParquetDataCatalog`, resumable via the checkpoint. Covers AE2 (resume without
//! refetch). No live calls.

use std::path::Path;

use chrono::NaiveDate;
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::ingest::checkpoint::{Checkpoint, GapReason, RebaseOrigin};
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
    async fn read_all_bars_dedups_an_overlapping_reingest() {
        // End-to-end proof of the dedup premise: an overlapping re-ingest writes a
        // SECOND parquet file for the overlap window (write_to_parquet skips the
        // disjoint check), so the aggregate read surfaces the overlap twice. Range
        // [Jan3,Jan4,Jan5] then an overlapping-forward [Jan4,Jan5,Jan8] must
        // round-trip to 4 unique daily bars, not 6 — without dedup this is 6.
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        write_bars(
            &catalog,
            vec![
                daily_bar(bt, ymd(2024, 1, 3), 60000),
                daily_bar(bt, ymd(2024, 1, 4), 60500),
                daily_bar(bt, ymd(2024, 1, 5), 61000),
            ],
        )
        .await
        .unwrap();
        write_bars(
            &catalog,
            vec![
                daily_bar(bt, ymd(2024, 1, 4), 60500), // overlap — byte-identical re-pull
                daily_bar(bt, ymd(2024, 1, 5), 61000), // overlap
                daily_bar(bt, ymd(2024, 1, 8), 61500), // new forward bar
            ],
        )
        .await
        .unwrap();

        let bars = read_all_bars(&catalog).await.unwrap();
        assert_eq!(bars.len(), 4, "overlap (Jan4/Jan5) deduped: 4 unique sessions, not 6");
        let distinct: std::collections::BTreeSet<u64> =
            bars.iter().map(|b| b.ts_event.as_u64()).collect();
        assert_eq!(distinct.len(), 4, "one bar per distinct session after dedup");
    }

    #[tokio::test]
    async fn delete_of_an_unstored_series_is_a_noop_ok() {
        let dir = tempdir().unwrap();
        // Deliberately NOT pre-created: the delete entry point must be
        // self-sufficient on a fresh path (the catalog-construction gotcha) and
        // still honor its no-op contract.
        let catalog = dir.path().join("catalog");
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
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use nautilus_ls::ingest::{delete_bar_series, kst_to_unix_nanos, read_all_bars, write_bars};
    use nautilus_ls::rules::KRX_REGULAR_CLOSE;
    use nautilus_model::data::{Bar, BarType};
    use nautilus_model::types::{Price, Quantity};

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
        // AE4: an organic detection stamps heal origin and increments organic by 1.
        assert_eq!(cp.rebase_events()[0].origin, RebaseOrigin::Heal, "organic detection is heal-origin");
        assert_eq!(cp.rebase_origin_totals().organic(), 1);

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
        cp.mark_shifted(SAMSUNG, "1-DAY", ymd(2024, 1, 5), RebaseOrigin::Heal);
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
        cp.mark_shifted(SAMSUNG, "1-DAY", ymd(2024, 1, 5), RebaseOrigin::Heal);
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
        cp.mark_shifted(SAMSUNG, "1-DAY", ymd(2024, 1, 5), RebaseOrigin::Heal);
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
        cp.mark_shifted(SAMSUNG, "1-DAY", ymd(2024, 1, 5), RebaseOrigin::Heal);
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
        cp.mark_shifted(SAMSUNG, "1-DAY", ymd(2024, 1, 5), RebaseOrigin::Heal);
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

    /// Edge: a zero-bar re-pull for a series that HAD stored bars must NOT
    /// complete the heal — completing would pin the watermark over the wiped
    /// store and permanently lose the history to a transient empty gateway
    /// response. The mark stays; a later run re-pulls when the server recovers.
    #[tokio::test]
    async fn empty_repull_of_a_nonempty_series_keeps_the_mark() {
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
        cp.mark_shifted(SAMSUNG, "1-DAY", ymd(2024, 1, 5), RebaseOrigin::Heal);
        cp.save(&cp_path).unwrap();
        // A transient gateway hiccup: the server serves NOTHING for the symbol.
        shared.set(series(&[]));

        let mut ing2 = Ingestor::new(sdk.clone(), daily_config(&catalog));
        let report = ing2.run_accumulate(&universe, ymd(2024, 1, 8), floor).await.unwrap();
        let cp = checkpoint_at(&catalog);
        assert!(cp.is_shifted(SAMSUNG, "1-DAY"), "an empty re-pull must not complete the heal");
        assert!(cp.rebase_events().is_empty());
        assert!(cp.watermark(SAMSUNG, "1-DAY").is_none(), "no watermark pinned over the wiped store");
        assert_eq!(report.gaps.len(), 1, "the incomplete heal is reported");

        // The server recovers → the next run re-pulls and completes.
        shared.set(v2());
        let mut ing3 = Ingestor::new(sdk, daily_config(&catalog));
        ing3.run_accumulate(&universe, ymd(2024, 1, 8), floor).await.unwrap();
        assert!(!checkpoint_at(&catalog).is_shifted(SAMSUNG, "1-DAY"));
        assert_eq!(stored_closes(&catalog).await, vec![30000, 30900, 31000, 31500]);
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
        // AE4: every epoch event carries epoch origin and the organic bucket is 0.
        assert!(cp.rebase_events().iter().all(|e| e.origin == RebaseOrigin::Epoch), "all events are epoch-origin");
        let totals = cp.rebase_origin_totals();
        assert_eq!(totals.epoch, 2);
        assert_eq!(totals.organic(), 0, "an epoch re-base leaves the organic metric unchanged");
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
        cp.mark_shifted(SAMSUNG, "1-DAY", ymd(2024, 1, 8), RebaseOrigin::Heal);
        cp.mark_shifted(HYNIX, "1-DAY", ymd(2024, 1, 8), RebaseOrigin::Heal);
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

    /// AE4: an epoch re-base that crashes after the atomic mark-all and is resumed
    /// under ACCUMULATE mode still stamps epoch origin on every event — origin is
    /// recorded at mark time, so the running mode at heal time is irrelevant. A
    /// mode-derived origin would (wrongly) stamp heal here (red-then-green).
    #[tokio::test]
    async fn epoch_crash_resume_under_accumulate_keeps_epoch_origin() {
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

        // Simulate the epoch's atomic mark-all landing (epoch origin) then a crash
        // before any heal — exactly what `run_rebase` writes before healing.
        let cp_path = catalog.join("ingest-checkpoint.json");
        let mut cp = Checkpoint::load(&cp_path).unwrap();
        cp.mark_shifted(SAMSUNG, "1-DAY", ymd(2024, 1, 8), RebaseOrigin::Epoch);
        cp.mark_shifted(HYNIX, "1-DAY", ymd(2024, 1, 8), RebaseOrigin::Epoch);
        cp.save(&cp_path).unwrap();

        // Resume under ACCUMULATE mode (not run_rebase) — the mode cannot tell why
        // the mark exists; only the stored origin can.
        let mut ing2 = Ingestor::new(sdk, daily_config(&catalog));
        ing2.run_accumulate(&universe, ymd(2024, 1, 8), floor).await.unwrap();
        let cp = checkpoint_at(&catalog);
        assert_eq!(cp.rebase_events().len(), 2);
        assert!(cp.rebase_events().iter().all(|e| e.origin == RebaseOrigin::Epoch), "crash-resumed events keep epoch origin");
        assert_eq!(cp.rebase_origin_totals().organic(), 0, "the organic metric stays clean across crash-resume");
    }

    /// AE4: a series already organically heal-marked at epoch time keeps its heal
    /// origin through the epoch re-base (keep-original-on-re-mark) and still counts
    /// organic; a subsequent independent organic heal increments organic by one.
    #[tokio::test]
    async fn already_heal_marked_series_keeps_heal_origin_through_epoch() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let shared = SharedSeries::one(v1());
        let sdk = sdk_with_series(&server, shared.clone()).await;
        let universe = [InstrumentId::from(SAMSUNG)];
        let floor = ymd(2024, 1, 1);

        let mut ing = Ingestor::new(sdk.clone(), daily_config(&catalog));
        ing.run_accumulate(&universe, ymd(2024, 1, 5), floor).await.unwrap();
        shared.set(v2());

        // The symbol was organically heal-marked before the epoch runs.
        let cp_path = catalog.join("ingest-checkpoint.json");
        let mut cp = Checkpoint::load(&cp_path).unwrap();
        cp.mark_shifted(SAMSUNG, "1-DAY", ymd(2024, 1, 6), RebaseOrigin::Heal);
        cp.save(&cp_path).unwrap();

        // The epoch re-base marks-all (epoch), but keep-original leaves this series heal.
        let mut ing2 = Ingestor::new(sdk, daily_config(&catalog));
        ing2.run_rebase(&universe, ymd(2024, 1, 8), floor).await.unwrap();
        let cp = checkpoint_at(&catalog);
        assert_eq!(cp.rebase_events().len(), 1);
        assert_eq!(cp.rebase_events()[0].origin, RebaseOrigin::Heal, "the pre-existing heal origin is kept");
        assert_eq!(cp.rebase_origin_totals().organic(), 1, "the heal-origin event counts organic");
    }

    // --- U1 (#104/R7): heal re-pull append overlap is caught per-triple ---

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

    /// An sdk whose t8410 responder serves the shared series AND, on the first
    /// *armed* `005930` call, injects an overlapping stored daily leaf just before
    /// responding. The heal wipe guarantees the re-pull is disjoint by
    /// construction, so this simulates the one anomaly #104 defends against — an
    /// overlap that survives the wipe (a delete that failed to clear, residual
    /// pollution) — by writing a leaf *after* the wipe (during the post-wipe
    /// re-pull) so the subsequent heal append is refused. The write runs on a
    /// dedicated OS thread with a fresh runtime: the catalog's internal `block_on`
    /// panics if called from within the mock's async worker.
    async fn sdk_with_injecting_series(
        server: &MockServer,
        shared: SharedSeries,
        catalog: &Path,
        inject_armed: Arc<AtomicBool>,
    ) -> LsSdk {
        mount_token(server).await;
        let catalog = catalog.to_path_buf();
        Mock::given(method("POST"))
            .and(path(CHART_PATH))
            .and(header("tr_cd", "t8410"))
            .respond_with(move |req: &wiremock::Request| {
                let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
                let ib = &body["t8410InBlock"];
                let shcode = ib["shcode"].as_str().unwrap_or("").to_string();
                let s = ib["sdate"].as_str().unwrap_or("").to_string();
                let e = ib["edate"].as_str().unwrap_or("").to_string();
                if shcode == "005930" && inject_armed.swap(false, Ordering::SeqCst) {
                    let cat = catalog.clone();
                    std::thread::spawn(move || {
                        let bt = BarKind::Daily
                            .bar_type(InstrumentId::from(SAMSUNG))
                            .unwrap();
                        let leaf = vec![
                            daily_bar(bt, ymd(2024, 1, 3), 100),
                            daily_bar(bt, ymd(2024, 1, 5), 102),
                        ];
                        tokio::runtime::Runtime::new()
                            .unwrap()
                            .block_on(write_bars(&cat, leaf))
                            .unwrap();
                    })
                    .join()
                    .unwrap();
                }
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
                    "t8410OutBlock": { "shcode": shcode, "cts_date": "", "rec_count": rows.len().to_string() },
                    "t8410OutBlock1": rows
                }))
            })
            .mount(server)
            .await;
        LsSdk::new(mock_config(&server.uri())).expect("sdk builds")
    }

    /// Covers AE3 (heal half) / R7: a heal re-pull whose append overlaps stored
    /// coverage records an `AppendRefusal` and lets the run continue — the clean
    /// sibling still ingests, the marked symbol stays marked, and its watermark is
    /// not advanced (so the next run re-heals). No propagated fatal error.
    #[tokio::test]
    async fn heal_append_overlap_is_recorded_and_run_continues() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let shared = SharedSeries::one(v1());
        let inject_armed = Arc::new(AtomicBool::new(false));
        let sdk =
            sdk_with_injecting_series(&server, shared.clone(), &catalog, inject_armed.clone()).await;
        let samsung = InstrumentId::from(SAMSUNG);
        let hynix = InstrumentId::from("000660.XKRX");
        let floor = ymd(2024, 1, 1);

        // Run 1: samsung gets stored bars + a watermark (a normal accumulate).
        let mut ing = Ingestor::new(sdk.clone(), daily_config(&catalog));
        ing.run_accumulate(&[samsung], ymd(2024, 1, 5), floor).await.unwrap();

        // The gateway rewrites the basis, and we mark samsung shifted (a detected
        // shift, saved before the wipe). Arm the overlap injection for run 2.
        shared.set(v2());
        let cp_path = catalog.join("ingest-checkpoint.json");
        let mut cp = Checkpoint::load(&cp_path).unwrap();
        cp.mark_shifted(SAMSUNG, "1-DAY", ymd(2024, 1, 5), RebaseOrigin::Heal);
        cp.save(&cp_path).unwrap();
        inject_armed.store(true, Ordering::SeqCst);

        // Run 2: samsung heals → wipe → re-pull (injects overlap) → append refused;
        // hynix appends cleanly. The refused triple does not abort the run.
        let mut ing2 = Ingestor::new(sdk, daily_config(&catalog));
        let report = ing2
            .run_accumulate(&[samsung, hynix], ymd(2024, 1, 8), floor)
            .await
            .unwrap();

        assert_eq!(report.append_refusals.len(), 1, "the heal overlap is recorded, not fatal");
        assert_eq!(report.append_refusals[0].instrument, SAMSUNG);
        assert_eq!(report.triples_ingested, 1, "the clean sibling still ingests");
        assert!(report.heal_refusals.is_empty(), "not a wipe-precondition refusal");

        let cp = checkpoint_at(&catalog);
        assert!(cp.is_shifted(SAMSUNG, "1-DAY"), "the marked symbol stays marked → next run re-heals");
        assert!(cp.watermark(SAMSUNG, "1-DAY").is_none(), "the refused heal did not advance the watermark");
        assert!(cp.watermark("000660.XKRX", "1-DAY").is_some(), "the clean sibling advanced");
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

// ---------------------------------------------------------------------------
// U4: range-mode per-series refusal (R5/R6, AE3). A daily series carrying an
// unhealed basis-shift mark must be refused pending heal — never served or
// completed on a stale adjustment basis — while unmarked series proceed and the
// run still exits successfully.
// ---------------------------------------------------------------------------

/// Seed a shifted mark for one series into the run's checkpoint on disk.
fn mark_series_shifted(catalog: &Path, instrument: &str, label: &str, detected: NaiveDate) {
    let cp_path = catalog.join("ingest-checkpoint.json");
    let mut cp = Checkpoint::load(&cp_path).unwrap();
    cp.mark_shifted(instrument, label, detected, RebaseOrigin::Heal);
    cp.save(&cp_path).unwrap();
}

/// AE3: a marked daily series is refused — no fetch, not marked done, and the
/// report carries its instrument/bar-type/detection date.
#[tokio::test]
async fn range_mode_refuses_a_marked_series() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let server = MockServer::start().await;
    let sdk = sdk_over(&server, daily_body_three_rows()).await;

    mark_series_shifted(&catalog, "005930.XKRX", "1-DAY", ymd(2024, 1, 5));

    let mut ingestor = Ingestor::new(sdk, daily_config(&catalog));
    let report = ingestor.run(&[InstrumentId::from("005930.XKRX")]).await.unwrap();

    assert_eq!(report.bars_written, 0, "a refused series writes nothing");
    assert_eq!(report.triples_ingested, 0);
    assert_eq!(report.range_refusals.len(), 1, "the marked series is refused pending heal");
    assert_eq!(report.range_refusals[0].instrument, "005930.XKRX");
    assert_eq!(report.range_refusals[0].bar_type, "1-DAY");
    assert_eq!(report.range_refusals[0].detected, "20240105");
    assert_eq!(count_t8410(&server).await, 0, "a refused series makes no gateway call");

    let cp = Checkpoint::load(&catalog.join("ingest-checkpoint.json")).unwrap();
    assert!(!cp.is_done("005930.XKRX", "1-DAY", "20240101..20240105"), "a refused series is not marked done");
    assert!(cp.is_shifted("005930.XKRX", "1-DAY"), "the mark stays until an accumulate/rebase heal");
}

/// AE3 (ordering, red-then-green): a series both marked AND already recorded done
/// for the range is still refused — the shifted check outranks `is_done`. A naive
/// `is_done`-first would wrongly skip it (and serve stale bars on the next read).
#[tokio::test]
async fn range_mode_refuses_a_marked_series_even_when_already_done() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let server = MockServer::start().await;
    let sdk = sdk_over(&server, daily_body_three_rows()).await;

    let cp_path = catalog.join("ingest-checkpoint.json");
    let mut cp = Checkpoint::default();
    cp.mark_done("005930.XKRX", "1-DAY", "20240101..20240105");
    cp.mark_shifted("005930.XKRX", "1-DAY", ymd(2024, 1, 5), RebaseOrigin::Heal);
    cp.save(&cp_path).unwrap();

    let mut ingestor = Ingestor::new(sdk, daily_config(&catalog));
    let report = ingestor.run(&[InstrumentId::from("005930.XKRX")]).await.unwrap();

    assert_eq!(report.range_refusals.len(), 1, "shifted outranks done — refused, not skipped");
    assert_eq!(report.triples_skipped, 0, "an is_done-first bug would have skipped it here");
    assert_eq!(count_t8410(&server).await, 0);
}

/// R6: an unmarked sibling in the same universe is pulled normally, and a run
/// containing refusals still exits successfully.
#[tokio::test]
async fn range_mode_pulls_unmarked_sibling_and_exits_ok() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let server = MockServer::start().await;
    let sdk = sdk_over(&server, daily_body_three_rows()).await;

    // 005930 marked (refused); 000660 unmarked (pulled).
    mark_series_shifted(&catalog, "005930.XKRX", "1-DAY", ymd(2024, 1, 5));

    let mut ingestor = Ingestor::new(sdk, daily_config(&catalog));
    let report = ingestor
        .run(&[InstrumentId::from("005930.XKRX"), InstrumentId::from("000660.XKRX")])
        .await
        .expect("a run with refusals still exits Ok");

    assert_eq!(report.range_refusals.len(), 1, "only the marked series is refused");
    assert_eq!(report.range_refusals[0].instrument, "005930.XKRX");
    assert_eq!(report.triples_ingested, 1, "the unmarked sibling is pulled");
    assert_eq!(report.bars_written, 3);
    assert_eq!(count_t8410(&server).await, 1, "only the unmarked sibling hits the gateway");

    let cp = Checkpoint::load(&catalog.join("ingest-checkpoint.json")).unwrap();
    assert!(cp.is_done("000660.XKRX", "1-DAY", "20240101..20240105"), "the sibling is completed");
}

// ---------------------------------------------------------------------------
// Minute-chunk continuation drive: pages are fetched one dispatch at a time
// (each passing the 1/s pacer) and the BODY cts_date/cts_time cursor is carried
// onto the next page request, mirroring the daily walk. Regression guard for two
// live-observed defects in the old `chart_all` delegation: back-to-back pages
// tripped t8412's 1/s gateway cap (IGW00201), and walking the tr_cont HTTP
// headers (which the live gateway terminates after page 1) silently truncated
// the range to its newest page.
// ---------------------------------------------------------------------------

async fn count_t8412(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| {
            r.url.path() == CHART_PATH
                && r.headers.get("tr_cd").and_then(|v| v.to_str().ok()) == Some("t8412")
        })
        .count()
}

fn t8412_row(date: &str, time: &str, close: &str) -> serde_json::Value {
    serde_json::json!({
        "date": date, "time": time, "open": close, "high": close, "low": close,
        "close": close, "jdiff_vol": "100", "value": "0", "jongchk": "0",
        "rate": "0", "sign": "0"
    })
}

#[tokio::test]
async fn minute_chunk_drives_continuation_page_by_page() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let server = MockServer::start().await;
    mount_token(&server).await;

    // Page 2 is served ONLY to a request whose body carries the FULL page-1 cts
    // cursor (date AND time) plus the tr_cont: Y header; a drive that fails to
    // thread any of the three re-receives page 1, trips the cursor-echo guard,
    // and fail-closes to a PaperThin gap (zero bars written) — failing every
    // assertion below. Page 1's out-block echoes a non-empty cursor exactly as
    // the live gateway does (its tr_cont HTTP header stays terminal — the header
    // walk this test guards against would stop after one page).
    Mock::given(method("POST"))
        .and(path(CHART_PATH))
        .and(header("tr_cd", "t8412"))
        .respond_with(move |req: &wiremock::Request| {
            let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or_default();
            let cts = body["t8412InBlock"]["cts_date"].as_str().unwrap_or("");
            let cts_t = body["t8412InBlock"]["cts_time"].as_str().unwrap_or("");
            let hdr = req.headers.get("tr_cont").and_then(|v| v.to_str().ok()).unwrap_or("");
            if cts == "20240103" && cts_t == "090200" && hdr == "Y" {
                json_response(serde_json::json!({
                    "rsp_cd": "00000",
                    "t8412OutBlock": { "cts_date": "", "cts_time": "" },
                    "t8412OutBlock1": [t8412_row("20240103", "0901", "60100")]
                }))
            } else {
                json_response(serde_json::json!({
                    "rsp_cd": "00000",
                    "t8412OutBlock": { "cts_date": "20240103", "cts_time": "090200" },
                    "t8412OutBlock1": [t8412_row("20240103", "0902", "60200")]
                }))
            }
        })
        .mount(&server)
        .await;
    let sdk = LsSdk::new(mock_config(&server.uri())).expect("sdk builds");

    let config = IngestConfig {
        catalog_path: catalog.clone(),
        bar_kinds: vec![BarKind::Minute(1)],
        sdate: "20240103".to_string(),
        edate: "20240103".to_string(),
        adjusted_prices: true,
        overlap_days: DEFAULT_OVERLAP_DAYS,
    };
    let mut ingestor = Ingestor::new(sdk, config);
    let report = ingestor
        .run(&[InstrumentId::from("005930.XKRX")])
        .await
        .expect("ingest runs");

    assert_eq!(report.bars_written, 2, "both pages' bars are persisted");
    assert_eq!(count_t8412(&server).await, 2, "exactly one dispatch per page");
    // Content, not just counts: a broken drive that re-received page 1 would
    // produce two DUPLICATE bars (same close, same ts) — assert the two bars
    // are the two DISTINCT pages' rows.
    let bars = nautilus_ls::ingest::read_all_bars(&catalog).await.expect("read back");
    assert_eq!(bars.len(), 2);
    assert_ne!(bars[0].ts_init, bars[1].ts_init, "two distinct minutes, not a duplicated page");
    let mut closes: Vec<String> = bars.iter().map(|b| b.close.to_string()).collect();
    closes.sort();
    assert_eq!(closes, vec!["60100", "60200"], "both pages' distinct rows persisted");
}

/// A zero-row page whose echoed cursor is still live is a SUSPECT PARTIAL, not a
/// clean completion: the chunk fail-closes (PaginationLimit -> split -> PaperThin
/// on a single day), the triple is recorded as a gap, and the range is NOT marked
/// done — the silent-truncation guard.
#[tokio::test]
async fn minute_empty_page_with_live_cursor_fails_closed_as_gap() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let server = MockServer::start().await;
    mount_token(&server).await;

    Mock::given(method("POST"))
        .and(path(CHART_PATH))
        .and(header("tr_cd", "t8412"))
        .respond_with(move |req: &wiremock::Request| {
            let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or_default();
            let cts = body["t8412InBlock"]["cts_date"].as_str().unwrap_or("");
            if cts.is_empty() {
                // First page: one row, live cursor.
                json_response(serde_json::json!({
                    "rsp_cd": "00000",
                    "t8412OutBlock": { "cts_date": "20240103", "cts_time": "090200" },
                    "t8412OutBlock1": [t8412_row("20240103", "0902", "60200")]
                }))
            } else {
                // Continuation: ZERO rows but the cursor still claims more.
                json_response(serde_json::json!({
                    "rsp_cd": "00000",
                    "t8412OutBlock": { "cts_date": "20240102", "cts_time": "150000" },
                    "t8412OutBlock1": []
                }))
            }
        })
        .mount(&server)
        .await;
    let sdk = LsSdk::new(mock_config(&server.uri())).expect("sdk builds");

    let config = IngestConfig {
        catalog_path: catalog.clone(),
        bar_kinds: vec![BarKind::Minute(1)],
        sdate: "20240103".to_string(),
        edate: "20240103".to_string(),
        adjusted_prices: true,
        overlap_days: DEFAULT_OVERLAP_DAYS,
    };
    let mut ingestor = Ingestor::new(sdk, config);
    let report = ingestor
        .run(&[InstrumentId::from("005930.XKRX")])
        .await
        .expect("a suspect-partial chunk exits Ok with a recorded gap");

    assert_eq!(report.bars_written, 0, "no partial bars persisted as complete");
    assert_eq!(report.gaps.len(), 1, "the truncated triple is a recorded gap");
    // Range-mode bookkeeping: the gap IS the signal (recorded in the report and
    // checkpoint, never silent); record_gap marks the triple done so re-runs
    // skip the known-bad feed — the documented retry is a catalog wipe.
    assert!(
        matches!(report.gaps[0].reason, GapReason::PaperThin),
        "recorded as a suspect-partial (PaperThin) gap, got {:?}",
        report.gaps[0].reason
    );
}

/// A re-served page (cursor echo) is dropped, never double-ingested: the chunk
/// fail-closes as a gap instead of persisting duplicate bars as a completion.
#[tokio::test]
async fn minute_cursor_echo_drops_duplicate_page_and_fails_closed() {
    let dir = tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    let server = MockServer::start().await;
    mount_token(&server).await;

    // Every page: same row, same echoed cursor — a gateway stuck re-serving.
    Mock::given(method("POST"))
        .and(path(CHART_PATH))
        .and(header("tr_cd", "t8412"))
        .respond_with(json_response(serde_json::json!({
            "rsp_cd": "00000",
            "t8412OutBlock": { "cts_date": "20240103", "cts_time": "090200" },
            "t8412OutBlock1": [t8412_row("20240103", "0902", "60200")]
        })))
        .mount(&server)
        .await;
    let sdk = LsSdk::new(mock_config(&server.uri())).expect("sdk builds");

    let config = IngestConfig {
        catalog_path: catalog.clone(),
        bar_kinds: vec![BarKind::Minute(1)],
        sdate: "20240103".to_string(),
        edate: "20240103".to_string(),
        adjusted_prices: true,
        overlap_days: DEFAULT_OVERLAP_DAYS,
    };
    let mut ingestor = Ingestor::new(sdk, config);
    let report = ingestor
        .run(&[InstrumentId::from("005930.XKRX")])
        .await
        .expect("a cursor-echo chunk exits Ok with a recorded gap");

    assert_eq!(report.bars_written, 0, "the re-served page's rows are never persisted");
    assert_eq!(count_t8412(&server).await, 2, "echo detected on the second dispatch");
    // Range-mode bookkeeping: the recorded PaperThin gap is the signal; the
    // triple is done-marked so re-runs skip it (retry = catalog wipe).
    assert_eq!(report.gaps.len(), 1, "the echoed triple is a recorded gap");
    assert!(
        matches!(report.gaps[0].reason, GapReason::PaperThin),
        "recorded as a suspect-partial (PaperThin) gap, got {:?}",
        report.gaps[0].reason
    );
}

// ---------------------------------------------------------------------------
// U1 — interval-metadata wrapper + checked-append overlap guard (R5/R6, AE3/AE4).
// U3 — backward-widen loud no-op (R4, AE2). U5 — catalog compaction (R8/R9/R10,
// AE6). All offline; no gateway beyond the wiremock chart bodies.
// ---------------------------------------------------------------------------

mod checked_append_and_compact {
    use super::*;
    use nautilus_ls::ingest::{
        append_bars_checked, compact_catalog, kst_to_unix_nanos, read_all_bars,
        stored_bar_intervals, write_bars, CompactOutcome,
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

    fn series(bt: BarType, dates: &[(NaiveDate, i64)]) -> Vec<Bar> {
        dates.iter().map(|(d, c)| daily_bar(bt, *d, *c)).collect()
    }

    fn closes(bars: &[Bar], bt: BarType) -> Vec<i64> {
        let mut v: Vec<i64> = bars
            .iter()
            .filter(|b| b.bar_type == bt)
            .map(|b| b.close.to_string().parse().unwrap())
            .collect();
        v.sort();
        v
    }

    // --- U1: the checked append guard ---

    /// AE3: an overlapping checked append is refused with an error naming both
    /// remediations, and no file is written (assert content, not counts).
    #[tokio::test]
    async fn ae3_overlapping_checked_append_is_refused_naming_both_remediations() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        write_bars(
            &catalog,
            series(bt, &[(ymd(2024, 6, 18), 100), (ymd(2024, 6, 25), 101), (ymd(2024, 7, 3), 102)]),
        )
        .await
        .unwrap();
        let intervals_before = stored_bar_intervals(&catalog, bt).await.unwrap();

        let overlap = series(bt, &[(ymd(2024, 6, 1), 90), (ymd(2024, 6, 18), 100), (ymd(2024, 7, 3), 102)]);
        let err = append_bars_checked(&catalog, bt, overlap)
            .await
            .expect_err("an overlapping append is refused");
        let msg = err.to_string();
        assert!(msg.contains("compact"), "names the compaction remediation: {msg}");
        assert!(msg.contains("re-pull"), "names the wipe + full re-pull remediation: {msg}");

        assert_eq!(
            stored_bar_intervals(&catalog, bt).await.unwrap(),
            intervals_before,
            "no new parquet file was written"
        );
        assert_eq!(
            closes(&read_all_bars(&catalog).await.unwrap(), bt),
            vec![100, 101, 102],
            "stored content is unchanged"
        );
    }

    /// Inclusive-bounds: a write sharing a single boundary timestamp with stored
    /// coverage is refused.
    #[tokio::test]
    async fn checked_append_refuses_a_single_shared_boundary_timestamp() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        write_bars(&catalog, series(bt, &[(ymd(2024, 6, 18), 100), (ymd(2024, 6, 20), 101)]))
            .await
            .unwrap();
        // New bars whose earliest ts == the stored interval's latest boundary.
        let touching = series(bt, &[(ymd(2024, 6, 20), 101), (ymd(2024, 6, 21), 102)]);
        let err = append_bars_checked(&catalog, bt, touching)
            .await
            .expect_err("a shared boundary timestamp is an inclusive-bounds overlap");
        assert!(err.to_string().contains("compact"));
    }

    /// Disjoint prefix, disjoint forward append, and an empty write all succeed.
    #[tokio::test]
    async fn checked_append_allows_disjoint_prefix_forward_and_empty() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        write_bars(&catalog, series(bt, &[(ymd(2024, 6, 18), 100), (ymd(2024, 7, 3), 102)]))
            .await
            .unwrap();
        // Disjoint prefix (a valid backward-widen escape hatch).
        append_bars_checked(&catalog, bt, series(bt, &[(ymd(2024, 6, 1), 90), (ymd(2024, 6, 17), 91)]))
            .await
            .expect("a disjoint prefix append is legal");
        // Disjoint forward append.
        append_bars_checked(&catalog, bt, series(bt, &[(ymd(2024, 7, 10), 110)]))
            .await
            .expect("a disjoint forward append is legal");
        // Empty write is a no-op Ok.
        append_bars_checked(&catalog, bt, vec![]).await.expect("an empty write is a no-op");
        assert_eq!(read_all_bars(&catalog).await.unwrap().len(), 5, "prefix + stored + forward all present");
    }

    /// `stored_bar_intervals` on a never-written path returns empty without error.
    #[tokio::test]
    async fn stored_bar_intervals_on_missing_series_is_empty() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog"); // never created
        let bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        assert!(stored_bar_intervals(&catalog, bt).await.unwrap().is_empty());
    }

    /// A refused triple does not abort the run: the clean triple ingests, the
    /// refusal lands in the report vec, and the refused triple's watermark is
    /// unchanged.
    #[tokio::test]
    async fn accumulate_refused_triple_does_not_abort_the_run() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;
        let samsung = InstrumentId::from("005930.XKRX");
        let hynix = InstrumentId::from("000660.XKRX");
        let sbt = BarKind::Daily.bar_type(samsung).unwrap();
        // Legacy-pollution shape: samsung has stored bars but NO checkpoint
        // watermark, so accumulate re-fetches from the floor and overlaps them.
        write_bars(&catalog, series(sbt, &[(ymd(2024, 1, 3), 60000), (ymd(2024, 1, 4), 61000), (ymd(2024, 1, 5), 62000)]))
            .await
            .unwrap();

        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        let report = ing
            .run_accumulate(&[samsung, hynix], ymd(2024, 1, 5), ymd(2024, 1, 1))
            .await
            .unwrap();

        assert_eq!(report.append_refusals.len(), 1, "the overlapping triple is refused, not fatal");
        assert_eq!(report.append_refusals[0].instrument, "005930.XKRX");
        assert_eq!(report.triples_ingested, 1, "the clean sibling still ingests");
        let cp = Checkpoint::load(&catalog.join("ingest-checkpoint.json")).unwrap();
        assert!(cp.watermark("005930.XKRX", "1-DAY").is_none(), "refused triple's watermark unchanged");
        assert!(cp.watermark("000660.XKRX", "1-DAY").is_some(), "clean triple advanced");
    }

    // --- U2 integration: migration makes accumulate skip a covered range ---

    /// A legacy `completed`-only checkpoint migrates on load; a subsequent
    /// accumulate skips the covered range instead of re-fetching from the floor.
    #[tokio::test]
    async fn legacy_checkpoint_migrates_and_accumulate_skips_covered_range() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;
        let samsung = InstrumentId::from("005930.XKRX");
        let bt = BarKind::Daily.bar_type(samsung).unwrap();
        std::fs::create_dir_all(&catalog).unwrap();
        write_bars(&catalog, series(bt, &[(ymd(2024, 1, 3), 60000), (ymd(2024, 1, 4), 61000), (ymd(2024, 1, 5), 62000)]))
            .await
            .unwrap();
        std::fs::write(
            catalog.join("ingest-checkpoint.json"),
            r#"{"completed":["005930.XKRX|1-DAY|20240101..20240105"],"gaps":[],"adjusted_prices":true}"#,
        )
        .unwrap();

        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        // Floor at the earliest stored session so no backward-widen warning fires.
        let report = ing.run_accumulate(&[samsung], ymd(2024, 1, 5), ymd(2024, 1, 3)).await.unwrap();
        assert_eq!(report.triples_skipped, 1, "the migrated watermark skips the covered range");
        assert_eq!(count_t8410(&server).await, 0, "no re-fetch from the floor");
        assert!(report.append_refusals.is_empty(), "no overlapping write attempted");
    }

    // --- U3: backward-widen loud no-op ---

    /// AE2: a migrated catalog covering 0618..0703, accumulate with an earlier
    /// floor (0601): no fetch below the watermark, and the report names the
    /// unreachable region + the escape hatch.
    #[tokio::test]
    async fn ae2_backward_widen_floor_warns_and_names_escape_hatch() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;
        let samsung = InstrumentId::from("005930.XKRX");
        let bt = BarKind::Daily.bar_type(samsung).unwrap();
        std::fs::create_dir_all(&catalog).unwrap();
        write_bars(&catalog, series(bt, &[(ymd(2024, 6, 18), 100), (ymd(2024, 6, 25), 101), (ymd(2024, 7, 3), 102)]))
            .await
            .unwrap();
        std::fs::write(
            catalog.join("ingest-checkpoint.json"),
            r#"{"completed":["005930.XKRX|1-DAY|20240618..20240703"],"gaps":[],"adjusted_prices":true}"#,
        )
        .unwrap();

        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        let report = ing.run_accumulate(&[samsung], ymd(2024, 7, 3), ymd(2024, 6, 1)).await.unwrap();
        assert_eq!(report.backward_widen_warnings.len(), 1, "the unreachable region is surfaced");
        let w = &report.backward_widen_warnings[0];
        assert_eq!(w.instrument, "005930.XKRX");
        assert_eq!(w.floor, "20240601");
        assert_eq!(w.earliest_stored, "20240618");
        assert_eq!(count_t8410(&server).await, 0, "no fetch below the watermark");
    }

    /// AE2 (R4/R5/R6): the backward-widen warning fires at most once per triple
    /// per floor. Run 1 warns and records the marker; run 2 at the same floor is
    /// silent — and because the warning fires *only* as part of the gated
    /// `stored_bar_intervals` read, a silent run 2 proves that read was skipped
    /// (the floor still precedes coverage, so the read WOULD warn if it ran). Run 3
    /// at a deeper floor re-warns and updates the marker.
    #[tokio::test]
    async fn ae2_backward_widen_warns_once_per_floor_and_skips_the_read() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;
        let samsung = InstrumentId::from("005930.XKRX");
        let bt = BarKind::Daily.bar_type(samsung).unwrap();
        std::fs::create_dir_all(&catalog).unwrap();
        write_bars(&catalog, series(bt, &[(ymd(2024, 6, 18), 100), (ymd(2024, 6, 25), 101), (ymd(2024, 7, 3), 102)]))
            .await
            .unwrap();
        std::fs::write(
            catalog.join("ingest-checkpoint.json"),
            r#"{"completed":["005930.XKRX|1-DAY|20240618..20240703"],"gaps":[],"adjusted_prices":true}"#,
        )
        .unwrap();
        let cp_path = catalog.join("ingest-checkpoint.json");

        // Run 1: floor 0601 precedes earliest coverage 0618 → warns once, records marker.
        let mut ing = Ingestor::new(sdk.clone(), daily_config(&catalog));
        let r1 = ing.run_accumulate(&[samsung], ymd(2024, 7, 3), ymd(2024, 6, 1)).await.unwrap();
        assert_eq!(r1.backward_widen_warnings.len(), 1, "run 1 warns once");
        assert_eq!(
            Checkpoint::load(&cp_path).unwrap().history_floor("005930.XKRX", "1-DAY"),
            Some(ymd(2024, 6, 1)),
            "the warned floor is recorded and persisted (survives the already-current skip)"
        );

        // Run 2: SAME floor → silent. No warning ⟹ needs_check was false ⟹ the
        // per-triple interval read was skipped (else the still-below floor re-warns).
        let mut ing2 = Ingestor::new(sdk.clone(), daily_config(&catalog));
        let r2 = ing2.run_accumulate(&[samsung], ymd(2024, 7, 3), ymd(2024, 6, 1)).await.unwrap();
        assert!(r2.backward_widen_warnings.is_empty(), "run 2 at the same floor is silent (read skipped)");

        // Run 3: DEEPER floor (0525 < recorded 0601) → new information, re-warns.
        let mut ing3 = Ingestor::new(sdk, daily_config(&catalog));
        let r3 = ing3.run_accumulate(&[samsung], ymd(2024, 7, 3), ymd(2024, 5, 25)).await.unwrap();
        assert_eq!(r3.backward_widen_warnings.len(), 1, "a deeper floor re-warns (R5)");
        assert_eq!(
            Checkpoint::load(&cp_path).unwrap().history_floor("005930.XKRX", "1-DAY"),
            Some(ymd(2024, 5, 25)),
            "the marker updates to the deeper floor"
        );
        assert_eq!(count_t8410(&server).await, 0, "already-current: no bar fetch across any run");
    }

    /// A floor within existing coverage does not warn.
    #[tokio::test]
    async fn backward_widen_floor_within_coverage_does_not_warn() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;
        let samsung = InstrumentId::from("005930.XKRX");
        let bt = BarKind::Daily.bar_type(samsung).unwrap();
        std::fs::create_dir_all(&catalog).unwrap();
        write_bars(&catalog, series(bt, &[(ymd(2024, 6, 18), 100), (ymd(2024, 7, 3), 102)]))
            .await
            .unwrap();
        std::fs::write(
            catalog.join("ingest-checkpoint.json"),
            r#"{"completed":["005930.XKRX|1-DAY|20240618..20240703"],"gaps":[],"adjusted_prices":true}"#,
        )
        .unwrap();
        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        let report = ing.run_accumulate(&[samsung], ymd(2024, 7, 3), ymd(2024, 6, 18)).await.unwrap();
        assert!(report.backward_widen_warnings.is_empty(), "floor at earliest coverage does not warn");
    }

    /// A fresh instrument (no watermark) does not warn — its floor fetch is normal.
    #[tokio::test]
    async fn backward_widen_fresh_instrument_does_not_warn() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;
        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        let report = ing
            .run_accumulate(&[InstrumentId::from("005930.XKRX")], ymd(2024, 1, 5), ymd(2024, 1, 1))
            .await
            .unwrap();
        assert!(report.backward_widen_warnings.is_empty(), "an unseen instrument is not a backward widen");
    }

    // --- U5: catalog compaction core ---

    /// AE6: one byte-identical-duplicated series is rewritten clean; a second
    /// value-divergent series is refused with its files untouched.
    #[tokio::test]
    async fn ae6_compact_collapses_duplicates_and_refuses_divergent() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let dup_bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let div_bt = BarKind::Daily.bar_type(InstrumentId::from("000660.XKRX")).unwrap();
        // Two OVERLAPPING (not identical) ranges → distinct parquet filenames → the
        // Jan4/Jan5 rows land in both files (the real re-ingest pollution shape; an
        // identical range would overwrite the same file, not duplicate rows).
        write_bars(&catalog, series(dup_bt, &[(ymd(2024, 1, 3), 100), (ymd(2024, 1, 4), 101), (ymd(2024, 1, 5), 102)]))
            .await
            .unwrap();
        write_bars(&catalog, series(dup_bt, &[(ymd(2024, 1, 4), 101), (ymd(2024, 1, 5), 102), (ymd(2024, 1, 8), 103)]))
            .await
            .unwrap();
        // A value-divergent series: two overlapping files disagree on Jan 3's close.
        write_bars(&catalog, series(div_bt, &[(ymd(2024, 1, 3), 200), (ymd(2024, 1, 4), 201)])).await.unwrap();
        write_bars(&catalog, series(div_bt, &[(ymd(2024, 1, 3), 999), (ymd(2024, 1, 6), 203)])).await.unwrap();

        let div_files_before = stored_bar_intervals(&catalog, div_bt).await.unwrap();
        let report = compact_catalog(&catalog).await.unwrap();

        let dup = report.series.iter().find(|s| s.bar_type == dup_bt.to_string()).unwrap();
        assert_eq!(dup.outcome, CompactOutcome::Compacted);
        assert_eq!(dup.bars_before, 6);
        assert_eq!(dup.bars_after, 4);
        assert_eq!(dup.files_after, 1, "duplicates collapse to one file");

        let div = report.series.iter().find(|s| s.bar_type == div_bt.to_string()).unwrap();
        assert_eq!(div.outcome, CompactOutcome::RefusedDivergent);
        assert_eq!(
            stored_bar_intervals(&catalog, div_bt).await.unwrap(),
            div_files_before,
            "the divergent series is left untouched"
        );

        // Content: the duplicated series is the 4 distinct sessions.
        let back = read_all_bars(&catalog).await.unwrap();
        assert_eq!(closes(&back, dup_bt), vec![100, 101, 102, 103], "deduped content preserved");
    }

    /// A second compact run reports the series clean and changes nothing.
    #[tokio::test]
    async fn compact_is_idempotent() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        // Overlapping (not identical) ranges → genuine duplicate rows.
        write_bars(&catalog, series(bt, &[(ymd(2024, 1, 3), 100), (ymd(2024, 1, 4), 101)])).await.unwrap();
        write_bars(&catalog, series(bt, &[(ymd(2024, 1, 4), 101), (ymd(2024, 1, 5), 102)])).await.unwrap();
        let first = compact_catalog(&catalog).await.unwrap();
        assert_eq!(first.series[0].outcome, CompactOutcome::Compacted);
        let second = compact_catalog(&catalog).await.unwrap();
        assert_eq!(second.series[0].outcome, CompactOutcome::Clean);
        assert_eq!(second.series[0].bars_before, second.series[0].bars_after);
    }

    /// Compaction against a catalog whose ingest advisory lock is held refuses
    /// loudly without touching files.
    #[tokio::test]
    async fn compact_refuses_while_ingest_lock_held() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        write_bars(&catalog, series(bt, &[(ymd(2024, 1, 3), 100)])).await.unwrap();
        let _held = AdvisoryLock::acquire(&catalog, LockKind::Ingest).unwrap();
        let err = compact_catalog(&catalog).await.expect_err("a held ingest lock refuses compaction");
        assert!(err.to_string().contains("in progress") || err.to_string().contains("lock"), "{err}");
    }

    /// R10: the checkpoint file bytes are identical before and after compaction.
    #[tokio::test]
    async fn compact_never_touches_the_checkpoint() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        // Overlapping ranges → a real rewrite, so R10 is tested against actual work.
        write_bars(&catalog, series(bt, &[(ymd(2024, 1, 3), 100), (ymd(2024, 1, 4), 101)])).await.unwrap();
        write_bars(&catalog, series(bt, &[(ymd(2024, 1, 4), 101), (ymd(2024, 1, 5), 102)])).await.unwrap();
        let cp_path = catalog.join("ingest-checkpoint.json");
        let mut cp = Checkpoint::default();
        cp.set_watermark("005930.XKRX", "1-DAY", ymd(2024, 1, 4));
        cp.save(&cp_path).unwrap();
        let before = std::fs::read(&cp_path).unwrap();
        compact_catalog(&catalog).await.unwrap();
        assert_eq!(std::fs::read(&cp_path).unwrap(), before, "R10: checkpoint bytes unchanged");
    }

    /// Derived-value stability: a backtest-level read returns the same bar set
    /// before (deduped view) and after compaction.
    #[tokio::test]
    async fn compact_preserves_the_backtest_read() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        // Overlapping ranges → duplicate rows the read deduplicates; compaction must
        // leave that deduped view identical.
        write_bars(&catalog, series(bt, &[(ymd(2024, 1, 3), 100), (ymd(2024, 1, 4), 101), (ymd(2024, 1, 5), 102)]))
            .await
            .unwrap();
        write_bars(&catalog, series(bt, &[(ymd(2024, 1, 4), 101), (ymd(2024, 1, 5), 102), (ymd(2024, 1, 8), 103)]))
            .await
            .unwrap();
        let before = closes(&read_all_bars(&catalog).await.unwrap(), bt);
        compact_catalog(&catalog).await.unwrap();
        let after = closes(&read_all_bars(&catalog).await.unwrap(), bt);
        assert_eq!(before, after, "the deduped backtest read is unchanged by compaction");
        assert_eq!(after, vec![100, 101, 102, 103]);
    }

    // --- U5: crash-recovery windows (leftover sidecar) ---

    fn stage_sidecar(catalog: &Path, bt: BarType, bars: &[Bar]) {
        let dir = catalog.join("compact-sidecars");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{}.json", bt)), serde_json::to_string(bars).unwrap()).unwrap();
    }

    fn sidecar_exists(catalog: &Path, bt: BarType) -> bool {
        catalog.join("compact-sidecars").join(format!("{}.json", bt)).exists()
    }

    /// Window (b): a leftover sidecar with no series files — the sidecar bars are
    /// restored and the sidecar removed.
    #[tokio::test]
    async fn compact_recovers_a_sidecar_with_no_series_files() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        std::fs::create_dir_all(&catalog).unwrap();
        let bars = series(bt, &[(ymd(2024, 1, 3), 100), (ymd(2024, 1, 4), 101)]);
        stage_sidecar(&catalog, bt, &bars);

        compact_catalog(&catalog).await.unwrap();
        assert_eq!(closes(&read_all_bars(&catalog).await.unwrap(), bt), vec![100, 101], "no bar lost");
        assert!(!sidecar_exists(&catalog, bt), "sidecar removed on success");
    }

    /// Window (a): a sidecar beside an intact series — union + dedup + rewrite,
    /// nothing lost.
    #[tokio::test]
    async fn compact_recovers_a_sidecar_beside_an_intact_series() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let bars = series(bt, &[(ymd(2024, 1, 3), 100), (ymd(2024, 1, 4), 101), (ymd(2024, 1, 5), 102)]);
        write_bars(&catalog, bars.clone()).await.unwrap();
        stage_sidecar(&catalog, bt, &bars);

        compact_catalog(&catalog).await.unwrap();
        assert_eq!(closes(&read_all_bars(&catalog).await.unwrap(), bt), vec![100, 101, 102]);
        assert!(!sidecar_exists(&catalog, bt), "sidecar removed on success");
    }

    /// Window (d): a sidecar plus bars appended after the crash — the union
    /// preserves both the sidecar rows and the newly-appended forward bar.
    #[tokio::test]
    async fn compact_recovery_preserves_bars_appended_after_the_crash() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let old = series(bt, &[(ymd(2024, 1, 3), 100), (ymd(2024, 1, 4), 101)]);
        write_bars(&catalog, old.clone()).await.unwrap(); // post-crash rewritten series
        write_bars(&catalog, series(bt, &[(ymd(2024, 1, 5), 102)])).await.unwrap(); // appended after crash
        stage_sidecar(&catalog, bt, &old); // leftover sidecar holds the old data

        compact_catalog(&catalog).await.unwrap();
        assert_eq!(
            closes(&read_all_bars(&catalog).await.unwrap(), bt),
            vec![100, 101, 102],
            "sidecar rows AND the appended forward bar both survive"
        );
        assert!(!sidecar_exists(&catalog, bt), "sidecar removed on success");
    }
}

// ---------------------------------------------------------------------------
// U4 (#102/KTD-1) — accumulate fetch trim against recorded coverage. A legacy
// multi-range checkpoint whose far ranges survive above a prefix watermark must
// accumulate to a stable state without re-fetching (and re-overlapping) a range
// it already records — no calendar, and never skipping a genuine trading-day hole.
// A date-aware t8410 responder serves rows only for the dates in a fixed map, so a
// holiday/weekend gap returns empty while a far range returns its bars.
// ---------------------------------------------------------------------------
mod reingest_trim {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use nautilus_ls::ingest::{kst_to_unix_nanos, read_all_bars, write_bars};
    use nautilus_ls::rules::KRX_REGULAR_CLOSE;
    use nautilus_model::data::{Bar, BarType};
    use nautilus_model::types::{Price, Quantity};

    const SAMSUNG: &str = "005930.XKRX";

    fn daily_bar(bt: BarType, date: NaiveDate, close: i64) -> Bar {
        let ts = kst_to_unix_nanos(date, KRX_REGULAR_CLOSE).unwrap();
        Bar::new(
            bt,
            Price::from((close - 5).to_string().as_str()),
            Price::from((close + 10).to_string().as_str()),
            Price::from((close - 10).to_string().as_str()),
            Price::from(close.to_string().as_str()),
            Quantity::from(1000),
            ts,
            ts,
        )
    }

    fn bars(bt: BarType, dates: &[(NaiveDate, i64)]) -> Vec<Bar> {
        dates.iter().map(|(d, c)| daily_bar(bt, *d, *c)).collect()
    }

    fn map(pairs: &[(&str, i64)]) -> BTreeMap<String, i64> {
        pairs.iter().map(|(d, c)| (d.to_string(), *c)).collect()
    }

    async fn closes(catalog: &Path) -> Vec<i64> {
        let mut b = read_all_bars(catalog).await.unwrap();
        b.sort_by_key(|x| x.ts_event.as_u64());
        b.iter().map(|x| x.close.to_string().parse().unwrap()).collect()
    }

    fn cp_path(catalog: &Path) -> PathBuf {
        catalog.join("ingest-checkpoint.json")
    }

    /// A date-aware t8410 responder: one ascending candle per date in `served`
    /// that falls inside the requested `[sdate, edate]`. A window covering no
    /// served date returns an empty page (a holiday/weekend gap).
    async fn sdk_daily_map(server: &MockServer, served: BTreeMap<String, i64>) -> LsSdk {
        mount_token(server).await;
        Mock::given(method("POST"))
            .and(path(CHART_PATH))
            .and(header("tr_cd", "t8410"))
            .respond_with(move |req: &wiremock::Request| {
                let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
                let ib = &body["t8410InBlock"];
                let s = ib["sdate"].as_str().unwrap_or("").to_string();
                let e = ib["edate"].as_str().unwrap_or("").to_string();
                let rows: Vec<serde_json::Value> = served
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

    /// Stage the prefix + far coverage as stored parquet and a checkpoint whose
    /// watermark sits at the prefix edate while the far range stays in `completed`
    /// (the exact post-migration shape a holiday cluster leaves behind).
    async fn stage(catalog: &Path, bt: BarType, prefix: &[(NaiveDate, i64)], far: &[(NaiveDate, i64)], completed: &str, watermark: &str) {
        std::fs::create_dir_all(catalog).unwrap();
        write_bars(catalog, bars(bt, prefix)).await.unwrap();
        write_bars(catalog, bars(bt, far)).await.unwrap();
        std::fs::write(
            cp_path(catalog),
            format!(
                r#"{{"completed":[{completed}],"gaps":[],"watermarks":{{"005930.XKRX|1-DAY":"{watermark}"}},"adjusted_prices":true}}"#
            ),
        )
        .unwrap();
    }

    /// Covers AE1 (holiday half) + the stall-flip: a prefix watermark and one far
    /// `completed` range separated by a non-trading gap. The far range is trimmed
    /// out (never re-fetched), the empty gap is probed once, no overlap is refused,
    /// and the watermark advances past the far range — the fixture that stalled the
    /// pre-trim code now stabilizes.
    #[tokio::test]
    async fn legacy_multi_range_stall_flips_to_stable_and_advanced() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        // Gap 0106..0109 is a weekend + holiday cluster → no served dates.
        let sdk = sdk_daily_map(&server, map(&[("20240103", 103), ("20240104", 104), ("20240105", 105), ("20240110", 110), ("20240111", 111), ("20240112", 112)])).await;
        let bt = BarKind::Daily.bar_type(InstrumentId::from(SAMSUNG)).unwrap();
        stage(&catalog, bt, &[(ymd(2024, 1, 3), 103), (ymd(2024, 1, 4), 104), (ymd(2024, 1, 5), 105)], &[(ymd(2024, 1, 10), 110), (ymd(2024, 1, 11), 111), (ymd(2024, 1, 12), 112)], r#""005930.XKRX|1-DAY|20240110..20240112""#, "20240105").await;

        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        let report = ing.run_accumulate(&[InstrumentId::from(SAMSUNG)], ymd(2024, 1, 12), ymd(2024, 1, 3)).await.unwrap();

        assert!(report.append_refusals.is_empty(), "no overlap is refused — the far range is trimmed, not re-fetched");
        assert!(report.backward_widen_warnings.is_empty(), "floor at earliest coverage → no widen warning");
        // 1 basis-shift detection overlap fetch (KTD-3) + 1 empty-gap probe; the
        // far range is never re-fetched (that would be a 3rd call and an overlap).
        assert_eq!(count_t8410(&server).await, 2, "detect + gap probe only; far range not re-fetched");
        assert_eq!(
            Checkpoint::load(&cp_path(&catalog)).unwrap().watermark(SAMSUNG, "1-DAY"),
            Some(ymd(2024, 1, 12)),
            "the watermark advances past the far range (max(last_closed, far edate))"
        );
        assert_eq!(closes(&catalog).await, vec![103, 104, 105, 110, 111, 112], "stored content unchanged — no duplicate, no gap bars");
    }

    /// Covers AE1 (real-hole half): the same shape but with a genuine trading day
    /// in the gap — that day is fetched and written (the un-covered sub-range), the
    /// far range is not re-fetched, and coverage becomes contiguous. Failure
    /// inversion: a trim that skipped the gap fetch drops 0108 here.
    #[tokio::test]
    async fn genuine_gap_day_is_fetched_far_range_is_not() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        // 0108 is a genuine trading day inside the gap.
        let sdk = sdk_daily_map(&server, map(&[("20240103", 103), ("20240104", 104), ("20240105", 105), ("20240108", 108), ("20240110", 110), ("20240111", 111), ("20240112", 112)])).await;
        let bt = BarKind::Daily.bar_type(InstrumentId::from(SAMSUNG)).unwrap();
        stage(&catalog, bt, &[(ymd(2024, 1, 3), 103), (ymd(2024, 1, 4), 104), (ymd(2024, 1, 5), 105)], &[(ymd(2024, 1, 10), 110), (ymd(2024, 1, 11), 111), (ymd(2024, 1, 12), 112)], r#""005930.XKRX|1-DAY|20240110..20240112""#, "20240105").await;

        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        let report = ing.run_accumulate(&[InstrumentId::from(SAMSUNG)], ymd(2024, 1, 12), ymd(2024, 1, 3)).await.unwrap();

        assert!(report.append_refusals.is_empty());
        // 1 detect-overlap fetch + 1 gap sub-range fetch (0106..0109); far untouched.
        assert_eq!(count_t8410(&server).await, 2, "detect + the gap sub-range only");
        assert_eq!(
            closes(&catalog).await,
            vec![103, 104, 105, 108, 110, 111, 112],
            "the genuine gap day 0108 is written; the far range is not re-fetched"
        );
        assert_eq!(Checkpoint::load(&cp_path(&catalog)).unwrap().watermark(SAMSUNG, "1-DAY"), Some(ymd(2024, 1, 12)));
    }

    /// Steady state: a single contiguous coverage block ending at the watermark
    /// yields exactly one fetch of [watermark+1, last_closed] — behavior identical
    /// to pre-change (one fetch, appended content is exactly what was served).
    #[tokio::test]
    async fn steady_state_is_a_single_unchanged_fetch() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_daily_map(&server, map(&[("20240108", 108), ("20240109", 109), ("20240110", 110)])).await;
        let bt = BarKind::Daily.bar_type(InstrumentId::from(SAMSUNG)).unwrap();
        // No far `completed` range — just a prefix watermark at 0105.
        std::fs::create_dir_all(&catalog).unwrap();
        write_bars(&catalog, bars(bt, &[(ymd(2024, 1, 3), 103), (ymd(2024, 1, 4), 104), (ymd(2024, 1, 5), 105)])).await.unwrap();
        std::fs::write(cp_path(&catalog), r#"{"completed":[],"gaps":[],"watermarks":{"005930.XKRX|1-DAY":"20240105"},"adjusted_prices":true}"#).unwrap();

        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        let report = ing.run_accumulate(&[InstrumentId::from(SAMSUNG)], ymd(2024, 1, 10), ymd(2024, 1, 3)).await.unwrap();

        assert!(report.append_refusals.is_empty());
        // 1 detect-overlap fetch + 1 forward-window fetch — the single-segment
        // steady-state path (no trim sub-ranges), identical to pre-change.
        assert_eq!(count_t8410(&server).await, 2, "detect + one forward fetch");
        assert_eq!(report.bars_written, 3, "the three served forward candles");
        assert_eq!(closes(&catalog).await, vec![103, 104, 105, 108, 109, 110]);
        assert_eq!(Checkpoint::load(&cp_path(&catalog)).unwrap().watermark(SAMSUNG, "1-DAY"), Some(ymd(2024, 1, 10)));
    }

    /// Multiple covered spans above the watermark subtract to multiple un-covered
    /// sub-ranges, each fetched and appended disjointly.
    #[tokio::test]
    async fn multiple_covered_spans_fetch_each_gap_disjointly() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_daily_map(&server, map(&[
            ("20240103", 103), ("20240104", 104), ("20240105", 105),
            ("20240108", 108), // gap 1
            ("20240110", 110), ("20240111", 111), // far 1
            ("20240115", 115), // gap 2
            ("20240116", 116), ("20240117", 117), // far 2
            ("20240119", 119), // gap 3 (tail)
        ])).await;
        let bt = BarKind::Daily.bar_type(InstrumentId::from(SAMSUNG)).unwrap();
        std::fs::create_dir_all(&catalog).unwrap();
        write_bars(&catalog, bars(bt, &[(ymd(2024, 1, 3), 103), (ymd(2024, 1, 4), 104), (ymd(2024, 1, 5), 105)])).await.unwrap();
        write_bars(&catalog, bars(bt, &[(ymd(2024, 1, 10), 110), (ymd(2024, 1, 11), 111)])).await.unwrap();
        write_bars(&catalog, bars(bt, &[(ymd(2024, 1, 16), 116), (ymd(2024, 1, 17), 117)])).await.unwrap();
        std::fs::write(
            cp_path(&catalog),
            r#"{"completed":["005930.XKRX|1-DAY|20240110..20240111","005930.XKRX|1-DAY|20240116..20240117"],"gaps":[],"watermarks":{"005930.XKRX|1-DAY":"20240105"},"adjusted_prices":true}"#,
        )
        .unwrap();

        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        let report = ing.run_accumulate(&[InstrumentId::from(SAMSUNG)], ymd(2024, 1, 19), ymd(2024, 1, 3)).await.unwrap();

        assert!(report.append_refusals.is_empty(), "each sub-range is disjoint from stored coverage");
        // 1 detect-overlap fetch + 3 un-covered sub-range fetches; neither far range re-fetched.
        assert_eq!(count_t8410(&server).await, 4, "detect + three sub-range fetches");
        assert_eq!(
            closes(&catalog).await,
            vec![103, 104, 105, 108, 110, 111, 115, 116, 117, 119],
            "gap days written, far ranges untouched, all contiguous"
        );
        assert_eq!(Checkpoint::load(&cp_path(&catalog)).unwrap().watermark(SAMSUNG, "1-DAY"), Some(ymd(2024, 1, 19)));
    }

    /// `last_closed` at or below the far range's edate: the far range spans across
    /// last_closed, so the watermark still advances to the far edate — the next run
    /// starts above it and does not re-overlap it.
    #[tokio::test]
    async fn watermark_advances_to_far_edate_when_last_closed_is_lower() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_daily_map(&server, map(&[("20240103", 103), ("20240104", 104), ("20240105", 105), ("20240108", 108), ("20240110", 110), ("20240114", 114)])).await;
        let bt = BarKind::Daily.bar_type(InstrumentId::from(SAMSUNG)).unwrap();
        // Far range 0110..0114 straddles last_closed (0112).
        stage(&catalog, bt, &[(ymd(2024, 1, 3), 103), (ymd(2024, 1, 4), 104), (ymd(2024, 1, 5), 105)], &[(ymd(2024, 1, 10), 110), (ymd(2024, 1, 14), 114)], r#""005930.XKRX|1-DAY|20240110..20240114""#, "20240105").await;

        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        let report = ing.run_accumulate(&[InstrumentId::from(SAMSUNG)], ymd(2024, 1, 12), ymd(2024, 1, 3)).await.unwrap();

        assert!(report.append_refusals.is_empty());
        // 1 detect-overlap fetch + 1 pre-far gap fetch; the straddling far range is not re-fetched.
        assert_eq!(count_t8410(&server).await, 2, "detect + the pre-far gap only");
        assert_eq!(
            Checkpoint::load(&cp_path(&catalog)).unwrap().watermark(SAMSUNG, "1-DAY"),
            Some(ymd(2024, 1, 14)),
            "advances to the far edate, not the lower last_closed, so the next run skips the far range"
        );
    }

    /// A halting sub-range stops the loop and never writes (orphans) a higher
    /// disjoint sub-range above the pinned watermark. Here the halt is an
    /// unforeseen overlap (a polluting leaf not recorded in `completed`, so the
    /// trim does not subtract it); the `halt_before` path is exactly the one a
    /// `PaperThin` truncation also takes — pin before, break, no higher fetch.
    #[tokio::test]
    async fn earlier_subrange_halt_does_not_orphan_a_higher_subrange() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_daily_map(&server, map(&[
            ("20240103", 103), ("20240104", 104), ("20240105", 105),
            ("20240108", 108), ("20240109", 109), // first sub-range (will overlap pollution)
            ("20240116", 116), ("20240117", 117), // far range (completed)
            ("20240118", 118), ("20240119", 119), // higher sub-range (must NOT be fetched/written)
        ])).await;
        let bt = BarKind::Daily.bar_type(InstrumentId::from(SAMSUNG)).unwrap();
        std::fs::create_dir_all(&catalog).unwrap();
        write_bars(&catalog, bars(bt, &[(ymd(2024, 1, 3), 103), (ymd(2024, 1, 4), 104), (ymd(2024, 1, 5), 105)])).await.unwrap();
        // Polluting leaf inside the first gap — stored but NOT in `completed`, so the
        // trim cannot subtract it; the first sub-range's append overlaps it.
        write_bars(&catalog, bars(bt, &[(ymd(2024, 1, 8), 108), (ymd(2024, 1, 9), 109)])).await.unwrap();
        write_bars(&catalog, bars(bt, &[(ymd(2024, 1, 16), 116), (ymd(2024, 1, 17), 117)])).await.unwrap();
        std::fs::write(
            cp_path(&catalog),
            r#"{"completed":["005930.XKRX|1-DAY|20240116..20240117"],"gaps":[],"watermarks":{"005930.XKRX|1-DAY":"20240105"},"adjusted_prices":true}"#,
        )
        .unwrap();

        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        let report = ing.run_accumulate(&[InstrumentId::from(SAMSUNG)], ymd(2024, 1, 19), ymd(2024, 1, 3)).await.unwrap();

        assert_eq!(report.append_refusals.len(), 1, "the first sub-range's overlap halts the loop");
        // 1 detect-overlap fetch + 1 first-sub-range fetch (which then refuses on
        // append); the higher sub-range is NEVER fetched because the loop halted.
        assert_eq!(count_t8410(&server).await, 2, "detect + the halting sub-range only — no higher fetch");
        let stored = closes(&catalog).await;
        assert!(!stored.contains(&118) && !stored.contains(&119), "no bars orphaned above the pinned watermark");
        assert_eq!(
            Checkpoint::load(&cp_path(&catalog)).unwrap().watermark(SAMSUNG, "1-DAY"),
            Some(ymd(2024, 1, 5)),
            "the watermark pins before the halting sub-range (no advance) → the next run re-derives it"
        );
    }
}

// ---------------------------------------------------------------------------
// U9 (KTD8) — accumulate + max-lookback probe migration behind the calendar
// adoption seam. Enforced routes the next-fetch/anchor through proven calendar
// facts; Shadow records the decision but stays byte-identical to Legacy; the
// fixture-loaded real `KrxCalendar` drives every case. PROOF-FIRST: assert
// ACTUAL gateway-request counts (wiremock) + watermark state, never a helper.
//
// Fixture facts (nautilus-ls-calendar/fixtures/base_2010_2012.json):
//   Trading Session : 2010-06-15, 2010-06-17, 2011-06-15
//   Closed          : 2010-06-19, 2010-06-20, 2011-02-02..04, 2012-05-01, ...
//   Unknown         : nearly every other weekday (e.g. 2010-01-05)
//   Coverage        : 2010-01-01 .. 2012-12-31
// ---------------------------------------------------------------------------
mod calendar_gate_migration {
    use super::*;
    use std::path::PathBuf;
    use chrono::{DateTime, TimeZone, Utc};
    use nautilus_ls::ingest::{CalendarGate, GateAction, ProbeAnchor};
    use nautilus_ls_calendar::{
        compute_artifact_id, compute_calendar_id, CalendarAdoption, KrxCalendar,
    };
    use nautilus_ls_calendar::schema::DayStatus;

    const SAMSUNG: &str = "005930.XKRX";
    const DAILY: &str = "1-DAY";

    /// An instant comfortably inside the fixture's authorization grant (2013-01-01 ..
    /// 2099-01-01), so the as-of view loads and authorizes.
    fn as_of() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2013, 6, 1, 0, 0, 0).unwrap()
    }

    fn fresh_history_as_of() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2013, 4, 1, 0, 0, 0).unwrap()
    }

    fn fixture_calendar() -> KrxCalendar {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("nautilus-ls-calendar/fixtures/base_2010_2012.json");
        KrxCalendar::load_from_path(&path, as_of()).expect("fixture calendar loads")
    }

    fn calendar_with_statuses(statuses: &[(NaiveDate, DayStatus)]) -> KrxCalendar {
        let mut snapshot = fixture_calendar().snapshot().clone();
        for (date, status) in statuses {
            snapshot.rows.iter_mut().find(|row| row.date == *date).unwrap().status = *status;
        }
        snapshot.artifact_id.clear();
        snapshot.calendar_id.clear();
        snapshot.artifact_id = compute_artifact_id(&snapshot);
        snapshot.calendar_id = compute_calendar_id(&snapshot);
        let dir = tempdir().unwrap();
        let path = dir.path().join("counterfactual-calendar.json");
        std::fs::write(&path, serde_json::to_vec_pretty(&snapshot).unwrap()).unwrap();
        KrxCalendar::load_from_path(&path, as_of()).unwrap()
    }

    async fn t8410_ranges(server: &MockServer) -> Vec<(String, String)> {
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|request| {
                request.url.path() == CHART_PATH
                    && request.headers.get("tr_cd").and_then(|v| v.to_str().ok())
                        == Some("t8410")
            })
            .map(|request| {
                let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                let input = &body["t8410InBlock"];
                (
                    input["sdate"].as_str().unwrap().to_string(),
                    input["edate"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    fn cp_path(catalog: &Path) -> PathBuf {
        catalog.join("ingest-checkpoint.json")
    }

    fn seed_watermark(catalog: &Path, wm: NaiveDate) {
        std::fs::create_dir_all(catalog).unwrap();
        let mut cp = Checkpoint::load(&cp_path(catalog)).unwrap();
        // Match daily_config's adjusted_prices so the byte-for-byte comparison isolates a
        // genuine state advance — accumulate rewrites this metadata field on every run.
        cp.adjusted_prices = true;
        cp.set_watermark(SAMSUNG, DAILY, wm);
        cp.save(&cp_path(catalog)).unwrap();
    }

    fn read_watermark(catalog: &Path) -> Option<NaiveDate> {
        Checkpoint::load(&cp_path(catalog)).unwrap().watermark(SAMSUNG, DAILY)
    }

    // -- Pure gate decision/action (Shadow records the decision but never acts) --

    /// The calendar decision is adoption-INDEPENDENT: a proven Closed date reads
    /// `SkipAdvance` under Enforced, but Shadow computes the SAME decision yet still
    /// yields `Proceed` (weekday authoritative) — the recorded-vs-acted split.
    #[test]
    fn shadow_records_the_disagreeing_decision_but_proceeds() {
        let cal = fixture_calendar();
        let view = cal.as_of(as_of()).unwrap();
        let closed = ymd(2010, 6, 19);

        let enforced = CalendarGate::new(CalendarAdoption::Enforced, Some(view));
        let shadow = CalendarGate::new(CalendarAdoption::Shadow, Some(view));
        // Same underlying calendar decision …
        assert_eq!(enforced.calendar_decision(closed), shadow.calendar_decision(closed));
        // … but only Enforced ACTS on it; Shadow proceeds (weekday authoritative).
        assert_eq!(enforced.action(closed), GateAction::SkipAdvance);
        assert_eq!(shadow.action(closed), GateAction::Proceed);
        // Legacy never consults the calendar at all.
        let legacy = CalendarGate::new(CalendarAdoption::Legacy, Some(view));
        assert_eq!(legacy.action(closed), GateAction::Proceed);
    }

    // -- Enforced accumulate next-fetch --

    /// Enforced, Unknown target: ZERO gateway requests for that date, and the seeded
    /// watermark is preserved byte-for-byte (no advance on Unknown — the provenance guard).
    #[tokio::test]
    async fn enforced_unknown_target_makes_no_request_and_no_advance() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;
        seed_watermark(&catalog, ymd(2010, 1, 2));

        let cal = fixture_calendar();
        let view = cal.as_of(as_of()).unwrap();
        let gate = CalendarGate::new(CalendarAdoption::Enforced, Some(view));

        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        // last_closed = 2010-01-05 is Unknown in the fixture.
        let report = ing
            .run_accumulate_gated(&[InstrumentId::from(SAMSUNG)], ymd(2010, 1, 5), ymd(2010, 1, 1), gate)
            .await
            .unwrap();

        assert_eq!(count_t8410(&server).await, 0, "Unknown target → zero gateway requests");
        assert_eq!(report.bars_written, 0);
        assert_eq!(report.triples_ingested, 0);
        assert_eq!(read_watermark(&catalog), Some(ymd(2010, 1, 2)), "watermark preserved (no advance on Unknown)");
    }

    /// Enforced, proven Trading Session (change only the target row): the request becomes
    /// observable and the watermark advances to the session.
    #[tokio::test]
    async fn enforced_trading_session_target_fetches() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;

        let cal = fixture_calendar();
        let view = cal.as_of(as_of()).unwrap();
        let gate = CalendarGate::new(CalendarAdoption::Enforced, Some(view));

        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        // last_closed = 2010-06-15 is a proven Trading Session.
        let report = ing
            .run_accumulate_gated(&[InstrumentId::from(SAMSUNG)], ymd(2010, 6, 15), ymd(2010, 6, 15), gate)
            .await
            .unwrap();

        assert_eq!(count_t8410(&server).await, 1, "Trading Session target → the fetch is observable");
        assert_eq!(report.triples_ingested, 1);
        assert_eq!(read_watermark(&catalog), Some(ymd(2010, 6, 15)), "watermark advances to the session");
    }

    /// Enforced, SINGLE-DATE proven Closed target: the watermark advances FROM closure
    /// evidence with NO gateway request. Seed a watermark of 2010-06-18 so the fetch range
    /// [start, last_closed] collapses to the single date 2010-06-19 (start = watermark+1),
    /// isolating the endpoint's single-date advance from the whole-range guard.
    #[tokio::test]
    async fn enforced_closed_target_advances_without_request() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;
        seed_watermark(&catalog, ymd(2010, 6, 18));

        let cal = fixture_calendar();
        let view = cal.as_of(as_of()).unwrap();
        let gate = CalendarGate::new(CalendarAdoption::Enforced, Some(view));

        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        // last_closed = 2010-06-19 is a proven Closed date; watermark 2010-06-18 → start = 06-19.
        let report = ing
            .run_accumulate_gated(&[InstrumentId::from(SAMSUNG)], ymd(2010, 6, 19), ymd(2010, 6, 14), gate)
            .await
            .unwrap();

        assert_eq!(count_t8410(&server).await, 0, "Closed target → no gateway request");
        assert_eq!(report.bars_written, 0);
        assert_eq!(report.triples_ingested, 0);
        assert_eq!(report.triples_skipped, 1);
        assert_eq!(read_watermark(&catalog), Some(ymd(2010, 6, 19)), "coverage advances FROM closure evidence");
    }

    /// Enforced, calendar unavailable (no view injected): stop before dispatch — checkpoint +
    /// watermark preserved byte-for-byte, zero gateway requests.
    #[tokio::test]
    async fn enforced_unavailable_calendar_stops_and_preserves_state() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;
        seed_watermark(&catalog, ymd(2010, 1, 2));
        let before = std::fs::read(&cp_path(&catalog)).unwrap();

        let gate = CalendarGate::new(CalendarAdoption::Enforced, None);
        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        let report = ing
            .run_accumulate_gated(&[InstrumentId::from(SAMSUNG)], ymd(2010, 1, 5), ymd(2010, 1, 1), gate)
            .await
            .unwrap();

        assert_eq!(count_t8410(&server).await, 0, "unavailable calendar → fail closed, zero requests");
        assert_eq!(report.triples_ingested, 0);
        assert_eq!(read_watermark(&catalog), Some(ymd(2010, 1, 2)), "watermark preserved");
        let after = std::fs::read(&cp_path(&catalog)).unwrap();
        assert_eq!(before, after, "checkpoint file byte-for-byte identical");
    }

    #[tokio::test]
    async fn enforced_stop_preserves_raw_legacy_checkpoint_bytes() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        std::fs::create_dir_all(&catalog).unwrap();
        let raw = br#"{"completed":["005930.XKRX|1-DAY|20100101..20100102"],"gaps":[],"adjusted_prices":true}"#;
        std::fs::write(cp_path(&catalog), raw).unwrap();
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;
        let cal = fixture_calendar();
        let gate = CalendarGate::new(CalendarAdoption::Enforced, Some(cal.as_of(as_of()).unwrap()));

        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        ing.run_accumulate_gated(
            &[InstrumentId::from(SAMSUNG)],
            ymd(2010, 1, 5),
            ymd(2010, 1, 1),
            gate,
        )
        .await
        .unwrap();

        assert_eq!(count_t8410(&server).await, 0);
        assert_eq!(std::fs::read(cp_path(&catalog)).unwrap(), raw);
    }

    #[tokio::test]
    async fn enforced_stop_does_not_create_a_missing_checkpoint() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;
        let gate = CalendarGate::new(CalendarAdoption::Enforced, None);

        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        ing.run_accumulate_gated(
            &[InstrumentId::from(SAMSUNG)],
            ymd(2010, 1, 5),
            ymd(2010, 1, 1),
            gate,
        )
        .await
        .unwrap();

        assert_eq!(count_t8410(&server).await, 0);
        assert!(!cp_path(&catalog).exists());
    }

    // -- Enforced accumulate next-fetch over the WHOLE RANGE (not just the endpoint) --
    //
    // The SkipAdvance the endpoint gate proposes replaces a fetch of the WHOLE range
    // [start, last_closed] (start = watermark+1, or lookback_floor for the initial
    // backfill). Advancing coverage over that span without a fetch is only safe when
    // EVERY date in it is proven Closed — a proven Trading Session in the span would
    // otherwise be silently skipped and marked covered with zero bars (false coverage).

    /// A later Trading Session cannot outrank the first Unknown. The authorized prefix ends
    /// at 06-15; 06-17 and the trailing Closed dates remain beyond the stop boundary.
    #[tokio::test]
    async fn enforced_later_session_does_not_cross_the_first_unknown() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;
        let cal = fixture_calendar();
        let view = cal.as_of(as_of()).unwrap();
        let gate = CalendarGate::new(CalendarAdoption::Enforced, Some(view));

        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        ing
            .run_accumulate_gated(&[InstrumentId::from(SAMSUNG)], ymd(2010, 6, 19), ymd(2010, 6, 15), gate)
            .await
            .unwrap();

        assert_eq!(
            t8410_ranges(&server).await,
            vec![("20100615".to_string(), "20100615".to_string())]
        );
        assert_eq!(read_watermark(&catalog), Some(ymd(2010, 6, 15)));
    }

    /// Enforced, MULTI-DAY range whose EVERY date is proven Closed (no watermark, start =
    /// lookback_floor): still SkipAdvance — no gateway request, watermark advances FROM
    /// closure evidence. Range [2011-02-02 .. 2011-02-04] is fully Closed in the fixture.
    #[tokio::test]
    async fn enforced_all_closed_range_advances_without_request() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;

        let cal = fixture_calendar();
        let view = cal.as_of(as_of()).unwrap();
        let gate = CalendarGate::new(CalendarAdoption::Enforced, Some(view));

        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        let report = ing
            .run_accumulate_gated(&[InstrumentId::from(SAMSUNG)], ymd(2011, 2, 4), ymd(2011, 2, 2), gate)
            .await
            .unwrap();

        assert_eq!(count_t8410(&server).await, 0, "all-Closed range → no gateway request");
        assert_eq!(report.bars_written, 0);
        assert_eq!(report.triples_ingested, 0);
        assert_eq!(report.triples_skipped, 1);
        assert_eq!(read_watermark(&catalog), Some(ymd(2011, 2, 4)), "coverage advances FROM closure evidence");
    }

    /// Enforced, MULTI-DAY range with an intervening UNKNOWN and no proven Trading Session
    /// (endpoint Closed): Stop before dispatch — no advance, checkpoint preserved byte-for-byte.
    /// Watermark 2010-06-17 → start 2010-06-18 (Unknown); last_closed 2010-06-19 (Closed).
    #[tokio::test]
    async fn enforced_range_with_intervening_unknown_stops_and_preserves_state() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;
        seed_watermark(&catalog, ymd(2010, 6, 17));
        let before = std::fs::read(&cp_path(&catalog)).unwrap();

        let cal = fixture_calendar();
        let view = cal.as_of(as_of()).unwrap();
        let gate = CalendarGate::new(CalendarAdoption::Enforced, Some(view));

        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        // start = 2010-06-18 (Unknown), last_closed = 2010-06-19 (Closed): the range holds an
        // Unknown and no proven Trading Session → Indeterminate → Stop.
        let report = ing
            .run_accumulate_gated(&[InstrumentId::from(SAMSUNG)], ymd(2010, 6, 19), ymd(2010, 6, 1), gate)
            .await
            .unwrap();

        assert_eq!(count_t8410(&server).await, 0, "Unknown in the range → fail closed, zero requests");
        assert_eq!(report.triples_ingested, 0);
        assert_eq!(read_watermark(&catalog), Some(ymd(2010, 6, 17)), "watermark preserved (no advance over Unknown)");
        let after = std::fs::read(&cp_path(&catalog)).unwrap();
        assert_eq!(before, after, "checkpoint file byte-for-byte identical");
    }

    #[tokio::test]
    async fn enforced_mixed_span_fetches_and_commits_only_the_established_prefix() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;
        let cal = calendar_with_statuses(&[
            (ymd(2010, 6, 15), DayStatus::TradingSession),
            (ymd(2010, 6, 16), DayStatus::Closed),
            (ymd(2010, 6, 17), DayStatus::Unknown),
            (ymd(2010, 6, 18), DayStatus::TradingSession),
        ]);
        let gate = CalendarGate::new(CalendarAdoption::Enforced, Some(cal.as_of(as_of()).unwrap()));

        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        ing.run_accumulate_gated(
            &[InstrumentId::from(SAMSUNG)],
            ymd(2010, 6, 18),
            ymd(2010, 6, 15),
            gate,
        )
        .await
        .unwrap();

        assert_eq!(
            t8410_ranges(&server).await,
            vec![("20100615".to_string(), "20100615".to_string())],
            "the request ends on the last session before Unknown"
        );
        assert_eq!(
            read_watermark(&catalog),
            Some(ymd(2010, 6, 16)),
            "successful fetch may commit the proven trailing Closed date, but not Unknown"
        );
    }

    #[tokio::test]
    async fn enforced_incomplete_fetch_does_not_commit_trailing_closures() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_empty()).await;
        let cal = calendar_with_statuses(&[
            (ymd(2010, 6, 15), DayStatus::TradingSession),
            (ymd(2010, 6, 16), DayStatus::Closed),
        ]);
        let gate = CalendarGate::new(CalendarAdoption::Enforced, Some(cal.as_of(as_of()).unwrap()));

        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        ing.run_accumulate_gated(
            &[InstrumentId::from(SAMSUNG)],
            ymd(2010, 6, 16),
            ymd(2010, 6, 15),
            gate,
        )
        .await
        .unwrap();

        assert_eq!(
            t8410_ranges(&server).await,
            vec![("20100615".to_string(), "20100615".to_string())]
        );
        assert_eq!(
            read_watermark(&catalog),
            None,
            "empty-history uncertainty cannot authorize the session or trailing closure"
        );
    }

    // -- Shadow byte-equivalence to Legacy --

    /// Run the same accumulate once under each gate over independent catalogs/servers and
    /// return (request count, watermark) so equivalence can be asserted.
    async fn run_once(gate_of: impl Fn() -> CalendarGate<'static>, cal: &'static KrxCalendar, last_closed: NaiveDate) -> (usize, Option<NaiveDate>) {
        let _ = cal;
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;
        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        ing.run_accumulate_gated(&[InstrumentId::from(SAMSUNG)], last_closed, ymd(2010, 6, 14), gate_of())
            .await
            .unwrap();
        (count_t8410(&server).await, read_watermark(&catalog))
    }

    /// Shadow is byte-identical to Legacy even when the calendar DISAGREES (a Closed target
    /// that Enforced would skip-advance): both fetch and advance identically, the calendar
    /// decision goes only to the non-persisted diagnostic channel.
    #[tokio::test]
    async fn shadow_disagreement_is_byte_identical_to_legacy() {
        // `cal` must outlive the borrowed views → leak one fixture for 'static.
        let cal: &'static KrxCalendar = Box::leak(Box::new(fixture_calendar()));
        let closed = ymd(2010, 6, 19); // Closed → Enforced would skip; Shadow/Legacy fetch.

        let legacy = run_once(|| CalendarGate::legacy(), cal, closed).await;
        let shadow = run_once(
            move || CalendarGate::new(CalendarAdoption::Shadow, Some(cal.as_of(as_of()).unwrap())),
            cal,
            closed,
        )
        .await;

        assert_eq!(legacy.0, shadow.0, "same gateway request count as Legacy");
        assert_eq!(legacy.1, shadow.1, "same watermark as Legacy");
        assert_eq!(legacy.0, 1, "Legacy/Shadow weekday path still fetches the Closed target");
    }

    /// Shadow with an UNAVAILABLE calendar is byte-identical to Legacy: the weekday path acts.
    #[tokio::test]
    async fn shadow_unavailable_is_byte_identical_to_legacy() {
        let cal: &'static KrxCalendar = Box::leak(Box::new(fixture_calendar()));
        let target = ymd(2010, 6, 19);

        let legacy = run_once(|| CalendarGate::legacy(), cal, target).await;
        let shadow = run_once(|| CalendarGate::new(CalendarAdoption::Shadow, None), cal, target).await;

        assert_eq!(legacy.0, shadow.0, "same request count as Legacy under an unavailable calendar");
        assert_eq!(legacy.1, shadow.1, "same watermark as Legacy");
    }

    // -- Probe anchor --

    /// Enforced probe anchor selects the most recent proven Trading Session; Unknown and
    /// unavailable stop (pure gate assertion).
    #[test]
    fn enforced_probe_anchor_selects_session_or_stops() {
        let cal = fixture_calendar();
        let view = cal.as_of(as_of()).unwrap();
        let gate = CalendarGate::new(CalendarAdoption::Enforced, Some(view));

        // Anchor IS a proven session → select it.
        assert_eq!(gate.probe_anchor(ymd(2010, 6, 15)), ProbeAnchor::Use(ymd(2010, 6, 15)));
        // Anchor is Unknown → stop (an Unknown at the boundary never manufactures an anchor).
        assert_eq!(gate.probe_anchor(ymd(2010, 1, 5)), ProbeAnchor::Stop);
        // Unavailable calendar → stop.
        let blind = CalendarGate::new(CalendarAdoption::Enforced, None);
        assert_eq!(blind.probe_anchor(ymd(2010, 6, 15)), ProbeAnchor::Stop);
        // Shadow keeps the weekday anchor authoritative even on a disagreeing (Unknown) date.
        let shadow = CalendarGate::new(CalendarAdoption::Shadow, Some(view));
        assert_eq!(shadow.probe_anchor(ymd(2010, 1, 5)), ProbeAnchor::Use(ymd(2010, 1, 5)));
    }

    #[test]
    fn enforced_probe_walks_only_the_reachable_established_suffix() {
        let cal = calendar_with_statuses(&[
            (ymd(2010, 6, 15), DayStatus::TradingSession),
            (ymd(2010, 6, 16), DayStatus::Closed),
            (ymd(2010, 6, 17), DayStatus::Unknown),
        ]);
        let gate = CalendarGate::new(CalendarAdoption::Enforced, Some(cal.as_of(as_of()).unwrap()));

        assert_eq!(gate.probe_anchor(ymd(2010, 6, 16)), ProbeAnchor::Use(ymd(2010, 6, 15)));
        assert_eq!(
            gate.probe_anchor(ymd(2010, 6, 17)),
            ProbeAnchor::Stop,
            "the probe cannot jump backward across an Unknown boundary"
        );
    }

    /// Enforced probe: an Unknown anchor STOPS before dispatch — zero t8412 requests, nothing
    /// recorded.
    #[tokio::test]
    async fn enforced_probe_unknown_anchor_makes_no_request() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("data").join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_with_probe(&server, ymd(2010, 6, 1)).await;

        let cal = fixture_calendar();
        let view = cal.as_of(as_of()).unwrap();
        let gate = CalendarGate::new(CalendarAdoption::Enforced, Some(view));

        let ing = Ingestor::new(sdk, probe_config(&catalog));
        let out = ing
            .run_probe_lookback_gated("005930", 1, ymd(2010, 1, 5), "2013-06-01T00:00:00Z".into(), gate)
            .await
            .unwrap();

        assert!(out.is_none(), "Unknown anchor → nothing recorded");
        assert_eq!(count_t8412(&server).await, 0, "Unknown anchor → zero gateway requests");
    }

    /// Enforced probe: a proven-session anchor probes (request observable), anchored at the
    /// selected session.
    #[tokio::test]
    async fn enforced_probe_session_anchor_probes() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("data").join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_with_probe(&server, ymd(2010, 6, 1)).await;

        let cal = fixture_calendar();
        let view = cal.as_of(as_of()).unwrap();
        let gate = CalendarGate::new(CalendarAdoption::Enforced, Some(view));

        let ing = Ingestor::new(sdk, probe_config(&catalog));
        let out = ing
            .run_probe_lookback_gated("005930", 1, ymd(2010, 6, 15), "2013-06-01T00:00:00Z".into(), gate)
            .await
            .unwrap()
            .expect("the pilot serves history from the selected session anchor");

        assert!(count_t8412(&server).await >= 1, "session anchor → the probe dispatches");
        // depth = selected anchor (2010-06-15) − earliest served (2010-06-01) = 14 days.
        assert_eq!(out.depth_days, 14, "anchored at the proven session, not a later weekday");
    }

    // -----------------------------------------------------------------------
    // U10 (KTD8) — checkpoint merge continuity + backward-widen migration.
    // Enforced merges a legacy `completed` gap ONLY when every intervening date
    // is proven Closed; a proven Trading Session (un-attested history) or an
    // Unknown/unavailable date (conservative over-fetch) keeps the ranges
    // separate. Shadow records the calendar verdict but stays byte-identical to
    // the weekday-authoritative Legacy path. The disagreement cases are chosen so
    // the calendar verdict DIFFERS from the weekday result, proving the seam acts.
    // -----------------------------------------------------------------------

    use nautilus_ls::ingest::{kst_to_unix_nanos, write_bars};
    use nautilus_ls::rules::KRX_REGULAR_CLOSE;
    use nautilus_model::data::{Bar, BarType};
    use nautilus_model::types::{Price, Quantity};

    /// Migrate `ranges` for SAMSUNG/1-DAY under `gate` and return (derived watermark, the
    /// per-triple remainder range lists).
    fn migrate_with(gate: &CalendarGate, ranges: &[&str]) -> (Option<NaiveDate>, Vec<Vec<String>>) {
        let mut cp = Checkpoint::default();
        for r in ranges {
            cp.mark_done(SAMSUNG, DAILY, r);
        }
        let rem = cp.migrate_completed_watermarks_gated(gate);
        (
            cp.watermark(SAMSUNG, DAILY),
            rem.into_iter().map(|r| r.ranges).collect(),
        )
    }

    /// Enforced merges a gap whose every intervening date is a proven Closed date (the
    /// 2011-02-02..04 holiday cluster) — where Legacy's weekday hole test would SPLIT it.
    #[test]
    fn enforced_merges_an_all_closed_gap_that_legacy_splits() {
        let cal = fixture_calendar();
        let view = cal.as_of(fresh_history_as_of()).unwrap();
        let enforced = CalendarGate::new(CalendarAdoption::Enforced, Some(view));
        // Gap open interval (2011-02-01, 2011-02-05) = {02-02, 02-03, 02-04}, all proven Closed.
        let ranges = ["20110115..20110201", "20110205..20110210"];
        let (wm, rem) = migrate_with(&enforced, &ranges);
        assert_eq!(wm, Some(ymd(2011, 2, 10)), "an all-Closed holiday-cluster gap chains into one watermark");
        assert!(rem.is_empty(), "no remainder — the ranges merged");

        // Legacy (weekday) SPLITS the same gap: 02-02..04 are weekdays.
        let (lwm, lrem) = migrate_with(&CalendarGate::legacy(), &ranges);
        assert_eq!(lwm, Some(ymd(2011, 2, 1)), "Legacy stops before the weekday hole");
        assert_eq!(lrem, vec![vec!["20110205..20110210".to_string()]]);
    }

    #[test]
    fn enforced_stale_full_history_keeps_all_closed_ranges_separate() {
        let cal = fixture_calendar();
        let stale = CalendarGate::new(CalendarAdoption::Enforced, Some(cal.as_of(as_of()).unwrap()));
        let ranges = ["20110115..20110201", "20110205..20110210"];

        let (wm, rem) = migrate_with(&stale, &ranges);

        assert_eq!(wm, Some(ymd(2011, 2, 1)));
        assert_eq!(rem, vec![vec!["20110205..20110210".to_string()]]);
    }

    /// A proven Trading Session in the gap PREVENTS the merge under Enforced (un-attested
    /// history must not be folded into the watermark).
    #[test]
    fn enforced_trading_session_in_the_gap_prevents_merge() {
        let cal = fixture_calendar();
        let view = cal.as_of(as_of()).unwrap();
        let enforced = CalendarGate::new(CalendarAdoption::Enforced, Some(view));
        // Gap open (2010-06-14, 2010-06-16) = {2010-06-15}, a proven Trading Session.
        let (wm, rem) = migrate_with(&enforced, &["20100610..20100614", "20100616..20100620"]);
        assert_eq!(wm, Some(ymd(2010, 6, 14)), "the chain stops before the un-attested session");
        assert_eq!(rem, vec![vec!["20100616..20100620".to_string()]]);
    }

    /// Unknown/unavailable evidence keeps the ranges SEPARATE under Enforced (conservative
    /// over-fetch) — even a weekend-only gap that Legacy would MERGE, because the calendar has
    /// no positive proof the weekend dates are non-trading.
    #[test]
    fn enforced_keeps_separate_across_an_unknown_gap_that_legacy_merges() {
        let cal = fixture_calendar();
        let view = cal.as_of(as_of()).unwrap();
        let enforced = CalendarGate::new(CalendarAdoption::Enforced, Some(view));
        // Gap open (2010-01-08 Fri, 2010-01-11 Mon) = {01-09 Sat, 01-10 Sun}, both Unknown.
        let ranges = ["20100104..20100108", "20100111..20100115"];
        let (wm, rem) = migrate_with(&enforced, &ranges);
        assert_eq!(wm, Some(ymd(2010, 1, 8)), "an unproven gap is not chained (conservative over-fetch)");
        assert_eq!(rem, vec![vec!["20100111..20100115".to_string()]]);

        // Legacy MERGES the weekend gap (no weekday strictly between Fri and Mon).
        let (lwm, lrem) = migrate_with(&CalendarGate::legacy(), &ranges);
        assert_eq!(lwm, Some(ymd(2010, 1, 15)), "Legacy merges across the weekend");
        assert!(lrem.is_empty());
    }

    /// Shadow migration is byte-identical to Legacy even when the calendar DISAGREES: the
    /// all-Closed gap Enforced would merge stays SPLIT under Shadow (weekday authoritative;
    /// the calendar verdict is recorded only).
    #[test]
    fn shadow_migration_is_byte_identical_to_legacy_even_when_calendar_disagrees() {
        let cal = fixture_calendar();
        let view = cal.as_of(as_of()).unwrap();
        let shadow = CalendarGate::new(CalendarAdoption::Shadow, Some(view));
        let ranges = ["20110115..20110201", "20110205..20110210"];

        let (swm, srem) = migrate_with(&shadow, &ranges);
        let (lwm, lrem) = migrate_with(&CalendarGate::legacy(), &ranges);
        assert_eq!(swm, lwm, "Shadow watermark identical to Legacy");
        assert_eq!(srem, lrem, "Shadow remainders identical to Legacy");
        assert_eq!(swm, Some(ymd(2011, 2, 1)), "…the weekday SPLIT result, not the Enforced merge");
    }

    // -- Backward-widen under the calendar seam --

    fn samsung_daily_bt() -> BarType {
        BarKind::Daily.bar_type(InstrumentId::from(SAMSUNG)).unwrap()
    }

    fn daily_bar(bt: BarType, date: NaiveDate, close: i64) -> Bar {
        let ts = kst_to_unix_nanos(date, KRX_REGULAR_CLOSE).unwrap();
        Bar::new(
            bt,
            Price::from((close - 5).to_string().as_str()),
            Price::from((close + 10).to_string().as_str()),
            Price::from((close - 10).to_string().as_str()),
            Price::from(close.to_string().as_str()),
            Quantity::from(1000),
            ts,
            ts,
        )
    }

    async fn seed_bars(catalog: &Path, dates: &[NaiveDate]) {
        let bt = samsung_daily_bt();
        let bars: Vec<Bar> = dates.iter().map(|d| daily_bar(bt, *d, 60000)).collect();
        write_bars(catalog, bars).await.unwrap();
    }

    fn read_history_floor(catalog: &Path) -> Option<NaiveDate> {
        Checkpoint::load(&cp_path(catalog)).unwrap().history_floor(SAMSUNG, DAILY)
    }

    /// Enforced backward-widen: a proven Trading Session in the pre-coverage region emits +
    /// PERSISTS the normal warning (real un-fetched history).
    #[tokio::test]
    async fn enforced_backward_widen_trading_session_warns_and_persists() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;
        // earliest stored = 2010-06-16; floor 2010-06-15 precedes it; [06-15, 06-16) = a proven
        // Trading Session (2010-06-15).
        seed_bars(&catalog, &[ymd(2010, 6, 16), ymd(2010, 6, 17)]).await;
        seed_watermark(&catalog, ymd(2011, 6, 15));

        let cal = fixture_calendar();
        let view = cal.as_of(as_of()).unwrap();
        let gate = CalendarGate::new(CalendarAdoption::Enforced, Some(view));
        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        let report = ing
            .run_accumulate_gated(&[InstrumentId::from(SAMSUNG)], ymd(2011, 6, 15), ymd(2010, 6, 15), gate)
            .await
            .unwrap();

        assert_eq!(report.backward_widen_warnings.len(), 1, "proven Trading Session → the normal warning");
        assert!(report.backward_widen_uncertainties.is_empty());
        assert_eq!(count_t8410(&server).await, 0, "no fetch below the watermark");
        assert_eq!(read_history_floor(&catalog), Some(ymd(2010, 6, 15)), "the floor is persisted (silences re-warns)");
    }

    /// Enforced backward-widen: an all-Closed pre-coverage region emits NOTHING and persists no
    /// floor (no trading history was missed).
    #[tokio::test]
    async fn enforced_backward_widen_all_closed_region_is_silent() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;
        // earliest stored = 2011-02-05; floor 2011-02-02; [02-02, 02-05) = {02-02..04} all Closed.
        seed_bars(&catalog, &[ymd(2011, 2, 5)]).await;
        seed_watermark(&catalog, ymd(2011, 6, 15));

        let cal = fixture_calendar();
        let view = cal.as_of(as_of()).unwrap();
        let gate = CalendarGate::new(CalendarAdoption::Enforced, Some(view));
        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        let report = ing
            .run_accumulate_gated(&[InstrumentId::from(SAMSUNG)], ymd(2011, 6, 15), ymd(2011, 2, 2), gate)
            .await
            .unwrap();

        assert!(report.backward_widen_warnings.is_empty(), "all-Closed region → no warning");
        assert!(report.backward_widen_uncertainties.is_empty(), "…and not an uncertainty either");
        assert_eq!(count_t8410(&server).await, 0);
        assert_eq!(read_history_floor(&catalog), None, "no floor recorded for an all-Closed region");
    }

    /// Enforced backward-widen: an Unknown/unavailable pre-coverage region emits the DISTINCT
    /// non-persisted uncertainty warning — and because it is not persisted, a later run
    /// RE-EVALUATES it (never silenced by a recorded floor).
    #[tokio::test]
    async fn enforced_backward_widen_unknown_region_is_uncertain_and_reevaluates() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;
        // earliest stored = 2010-01-06; floor 2010-01-04; [01-04, 01-06) = {01-04, 01-05} Unknown.
        seed_bars(&catalog, &[ymd(2010, 1, 6)]).await;
        seed_watermark(&catalog, ymd(2011, 6, 15));

        let cal = fixture_calendar();
        let view = cal.as_of(as_of()).unwrap();
        let gate = CalendarGate::new(CalendarAdoption::Enforced, Some(view));
        let mut ing = Ingestor::new(sdk, daily_config(&catalog));

        let r1 = ing
            .run_accumulate_gated(&[InstrumentId::from(SAMSUNG)], ymd(2011, 6, 15), ymd(2010, 1, 4), gate)
            .await
            .unwrap();
        assert!(r1.backward_widen_warnings.is_empty(), "Unknown region → not the normal warning");
        assert_eq!(r1.backward_widen_uncertainties.len(), 1, "…but the distinct uncertainty warning");
        assert_eq!(read_history_floor(&catalog), None, "the uncertainty is NOT persisted");

        // A later run (same gate; the evidence has not resolved) re-evaluates and re-emits — the
        // non-persistence is what lets newly-resolved evidence be reconsidered.
        let r2 = ing
            .run_accumulate_gated(&[InstrumentId::from(SAMSUNG)], ymd(2011, 6, 15), ymd(2010, 1, 4), gate)
            .await
            .unwrap();
        assert_eq!(r2.backward_widen_uncertainties.len(), 1, "re-evaluated on the next run (not silenced)");
    }

    /// Run one backward-widen accumulate under `gate_for` over an all-Closed pre-coverage region
    /// and return (normal warnings, uncertainties, persisted floor).
    async fn widen_once(
        gate_for: impl Fn(&'static KrxCalendar) -> CalendarGate<'static>,
        cal: &'static KrxCalendar,
    ) -> (usize, usize, Option<NaiveDate>) {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("catalog");
        let server = MockServer::start().await;
        let sdk = sdk_over(&server, daily_body_three_rows()).await;
        seed_bars(&catalog, &[ymd(2011, 2, 5)]).await;
        seed_watermark(&catalog, ymd(2011, 6, 15));
        let mut ing = Ingestor::new(sdk, daily_config(&catalog));
        let report = ing
            .run_accumulate_gated(&[InstrumentId::from(SAMSUNG)], ymd(2011, 6, 15), ymd(2011, 2, 2), gate_for(cal))
            .await
            .unwrap();
        (
            report.backward_widen_warnings.len(),
            report.backward_widen_uncertainties.len(),
            read_history_floor(&catalog),
        )
    }

    /// Shadow backward-widen is byte-identical to Legacy even where Enforced would SUPPRESS: an
    /// all-Closed region still warns + persists under Shadow (weekday authoritative).
    #[tokio::test]
    async fn shadow_backward_widen_is_byte_identical_to_legacy() {
        let cal: &'static KrxCalendar = Box::leak(Box::new(fixture_calendar()));
        let legacy = widen_once(|_| CalendarGate::legacy(), cal).await;
        let shadow = widen_once(
            |c| CalendarGate::new(CalendarAdoption::Shadow, Some(c.as_of(as_of()).unwrap())),
            cal,
        )
        .await;
        assert_eq!(legacy, shadow, "Shadow backward-widen identical to Legacy (warns + persists), not the Enforced suppress");
        assert_eq!(legacy, (1, 0, Some(ymd(2011, 2, 2))), "Legacy/Shadow warn once and persist the floor");
    }
}
