//! U3 offline integration: ingest against wiremock-served chart bodies into a real
//! `ParquetDataCatalog`, resumable via the checkpoint. Covers AE2 (resume without
//! refetch). No live calls.

use std::path::Path;

use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::ingest::checkpoint::Checkpoint;
use nautilus_ls::ingest::{BarKind, IngestConfig, Ingestor};
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_model::identifiers::InstrumentId;
use tempfile::tempdir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
