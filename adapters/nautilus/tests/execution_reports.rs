//! U7 offline integration: the position and order status reports nautilus needs to
//! rebuild yesterday's book at start (KTD14, R29).
//!
//! Without these the restarted node has no position for a held leg at all, so the
//! strategy's day-2 exit has nothing to exit. The load-bearing detail is
//! `avg_px_open = pamt`: R32 measured `pamt` (cost) and `price`/`appamt` (valuation)
//! separating across the session boundary, and a position restored at the marked
//! price carries the wrong stop.

use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_common::clients::ExecutionClient;
use nautilus_common::messages::execution::{
    GenerateOrderStatusReports, GeneratePositionStatusReports,
};
use nautilus_core::{UnixNanos, UUID4};
use nautilus_ls::execution::LsExecClient;
use nautilus_model::enums::{AccountType, OrderSide, OrderStatus, PositionSideSpecified};
use nautilus_model::identifiers::InstrumentId;
use rust_decimal::Decimal;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ACCNO_PATH: &str = "/stock/accno";

fn ok_json(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_string(body.to_string())
        .insert_header("content-type", "application/json")
}

async fn mount_t0424(server: &MockServer, cts_expcode: &str, holdings: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path(ACCNO_PATH))
        .and(header("tr_cd", "t0424"))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "00000",
            "t0424OutBlock": { "sunamt": "500000000", "cts_expcode": cts_expcode },
            "t0424OutBlock1": holdings
        })))
        .mount(server)
        .await;
}

async fn mount_t0425(server: &MockServer, cts_ordno: &str, rows: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path(ACCNO_PATH))
        .and(header("tr_cd", "t0425"))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "00000",
            "t0425OutBlock": { "tqty": "0", "tcheqty": "0", "tordrem": "0", "cts_ordno": cts_ordno },
            "t0425OutBlock1": rows
        })))
        .mount(server)
        .await;
}

async fn client(server: &MockServer) -> LsExecClient {
    mount_token(server).await;
    let sdk = LsSdk::new(mock_config(&server.uri())).unwrap();
    LsExecClient::new(
        "LS-KRX",
        "LS-TRADER-001",
        "00000000-01",
        sdk,
        AccountType::Cash,
    )
}

fn positions_cmd(instrument_id: Option<InstrumentId>) -> GeneratePositionStatusReports {
    GeneratePositionStatusReports::new(
        UUID4::new(),
        UnixNanos::default(),
        instrument_id,
        None,
        None,
        None,
        None,
    )
}

fn orders_cmd(open_only: bool, instrument_id: Option<InstrumentId>) -> GenerateOrderStatusReports {
    GenerateOrderStatusReports::new(
        UUID4::new(),
        UnixNanos::default(),
        open_only,
        instrument_id,
        None,
        None,
        None,
        None,
    )
}

/// Two held rows become two Long position reports whose `avg_px_open` is `pamt` — the
/// COST, not the mark. The fixture uses the R32 day-2 shape (pamt held at the average
/// buy while price/appamt marked down) so reading the wrong field is visible.
#[tokio::test]
async fn two_holdings_become_two_long_reports_priced_at_pamt() {
    let server = MockServer::start().await;
    let client = client(&server).await;
    mount_t0424(
        &server,
        "",
        serde_json::json!([
            { "expcode": "005930", "janqty": "1", "mdposqt": "1",
              "pamt": "265500", "price": "260000", "appamt": "260000" },
            { "expcode": "035720", "janqty": "4", "mdposqt": "4",
              "pamt": "50000", "price": "48000", "appamt": "192000" }
        ]),
    )
    .await;

    let reports = client
        .generate_position_status_reports(&positions_cmd(None))
        .await
        .expect("the holdings map to reports");
    assert_eq!(reports.len(), 2);

    let samsung = reports
        .iter()
        .find(|r| r.instrument_id == InstrumentId::from("005930.XKRX"))
        .expect("005930 reported");
    assert_eq!(samsung.position_side, PositionSideSpecified::Long);
    assert_eq!(samsung.quantity.as_f64() as i64, 1);
    assert_eq!(samsung.signed_decimal_qty, Decimal::from(1), "long is positive");
    assert_eq!(
        samsung.avg_px_open,
        Some(Decimal::from(265_500)),
        "the cost basis is pamt — NOT the 260,000 mark"
    );
    assert!(
        samsung.venue_position_id.is_none(),
        "LS assigns no venue position id; the netting book is per symbol"
    );

    let kakao = reports
        .iter()
        .find(|r| r.instrument_id == InstrumentId::from("035720.XKRX"))
        .expect("035720 reported");
    assert_eq!(kakao.quantity.as_f64() as i64, 4);
    assert_eq!(kakao.avg_px_open, Some(Decimal::from(50_000)));
}

/// A flat account reports no positions. That is a fact, not a failure — the first
/// rehearsal session of an arc starts exactly here.
#[tokio::test]
async fn a_flat_account_reports_no_positions() {
    let server = MockServer::start().await;
    let client = client(&server).await;
    mount_t0424(&server, "", serde_json::json!([])).await;

    let reports = client
        .generate_position_status_reports(&positions_cmd(None))
        .await
        .expect("a flat read succeeds");
    assert!(reports.is_empty());
}

/// A lingering `janqty=0` row is not a position, so it produces no report — the same
/// rule the flat gate and the book predicate apply.
#[tokio::test]
async fn a_zero_quantity_row_produces_no_position_report() {
    let server = MockServer::start().await;
    let client = client(&server).await;
    mount_t0424(
        &server,
        "",
        serde_json::json!([{ "expcode": "005930", "janqty": "0", "mdposqt": "0", "pamt": "0" }]),
    )
    .await;

    assert!(client
        .generate_position_status_reports(&positions_cmd(None))
        .await
        .expect("the read succeeds")
        .is_empty());
}

/// An unparseable `pamt` is an ERROR. Nautilus builds no position without an average
/// open price, so a report that silently drops it would restore a leg the strategy
/// cannot size an exit for — worse than refusing to start.
#[tokio::test]
async fn an_unparseable_pamt_fails_closed() {
    let server = MockServer::start().await;
    let client = client(&server).await;
    mount_t0424(
        &server,
        "",
        serde_json::json!([{ "expcode": "005930", "janqty": "1", "pamt": "??" }]),
    )
    .await;

    let err = client
        .generate_position_status_reports(&positions_cmd(None))
        .await
        .expect_err("an unreadable cost basis must refuse");
    assert!(err.to_string().contains("pamt"), "the reason names the field: {err}");
}

/// A truncated holdings read never becomes a short position report: reconciliation
/// would treat the positions it did not see as closed.
#[tokio::test]
async fn a_truncated_holdings_read_fails_closed() {
    let server = MockServer::start().await;
    let client = client(&server).await;
    // Every page echoes the same cursor — the enumeration never terminates.
    mount_t0424(
        &server,
        "MORE",
        serde_json::json!([{ "expcode": "005930", "janqty": "1", "pamt": "265500" }]),
    )
    .await;

    client
        .generate_position_status_reports(&positions_cmd(None))
        .await
        .expect_err("a non-terminating enumeration must refuse");
}

/// `cmd.instrument_id` narrows the reports to one instrument.
#[tokio::test]
async fn the_command_can_narrow_positions_to_one_instrument() {
    let server = MockServer::start().await;
    let client = client(&server).await;
    mount_t0424(
        &server,
        "",
        serde_json::json!([
            { "expcode": "005930", "janqty": "1", "pamt": "265500" },
            { "expcode": "035720", "janqty": "4", "pamt": "50000" }
        ]),
    )
    .await;

    let reports = client
        .generate_position_status_reports(&positions_cmd(Some(InstrumentId::from("035720.XKRX"))))
        .await
        .expect("the read succeeds");
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].instrument_id, InstrumentId::from("035720.XKRX"));
}

/// t0425 rows become order reports whose status is derived from the QUANTITIES, not
/// from the broker's Korean display text.
#[tokio::test]
async fn t0425_rows_become_order_reports_with_quantity_derived_status() {
    let server = MockServer::start().await;
    let client = client(&server).await;
    mount_t0425(
        &server,
        "",
        serde_json::json!([
            // Fully filled.
            { "ordno": "1001", "expcode": "005930", "medosu": "매수", "qty": "10",
              "price": "60000", "cheqty": "10", "ordrem": "0", "status": "체결",
              "orgordno": "", "ordtime": "0900" },
            // Partially filled, remainder resting.
            { "ordno": "1002", "expcode": "035720", "medosu": "매도", "qty": "8",
              "price": "50000", "cheqty": "3", "ordrem": "5", "status": "접수",
              "orgordno": "", "ordtime": "0901" },
            // Untouched and resting.
            { "ordno": "1003", "expcode": "000660", "medosu": "매수", "qty": "2",
              "price": "150000", "cheqty": "0", "ordrem": "2", "status": "접수",
              "orgordno": "", "ordtime": "0902" }
        ]),
    )
    .await;

    let reports = client
        .generate_order_status_reports(&orders_cmd(false, None))
        .await
        .expect("the orders map to reports");
    assert_eq!(reports.len(), 3);

    let filled = &reports[0];
    assert_eq!(filled.order_status, OrderStatus::Filled);
    assert_eq!(filled.order_side, OrderSide::Buy);
    assert_eq!(filled.filled_qty.as_f64() as i64, 10);
    assert_eq!(filled.venue_order_id.as_str(), "1001");
    assert_eq!(filled.price.map(|p| p.as_f64() as i64), Some(60_000));
    assert!(
        filled.client_order_id.is_none(),
        "an inherited order carries no client order id this process assigned"
    );

    assert_eq!(reports[1].order_status, OrderStatus::PartiallyFilled);
    assert_eq!(reports[1].order_side, OrderSide::Sell, "매도 maps to Sell");
    assert_eq!(reports[2].order_status, OrderStatus::Accepted);
}

/// `open_only` keeps just the orders that still rest.
#[tokio::test]
async fn open_only_keeps_the_resting_orders() {
    let server = MockServer::start().await;
    let client = client(&server).await;
    mount_t0425(
        &server,
        "",
        serde_json::json!([
            { "ordno": "1001", "expcode": "005930", "medosu": "매수", "qty": "10",
              "price": "60000", "cheqty": "10", "ordrem": "0", "status": "체결",
              "orgordno": "", "ordtime": "0900" },
            { "ordno": "1003", "expcode": "000660", "medosu": "매수", "qty": "2",
              "price": "150000", "cheqty": "0", "ordrem": "2", "status": "접수",
              "orgordno": "", "ordtime": "0902" }
        ]),
    )
    .await;

    let reports = client
        .generate_order_status_reports(&orders_cmd(true, None))
        .await
        .expect("the read succeeds");
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].venue_order_id.as_str(), "1003");
}

/// A flat account reports no orders.
#[tokio::test]
async fn a_flat_account_reports_no_orders() {
    let server = MockServer::start().await;
    let client = client(&server).await;
    mount_t0425(&server, "", serde_json::json!([])).await;

    assert!(client
        .generate_order_status_reports(&orders_cmd(false, None))
        .await
        .expect("a flat read succeeds")
        .is_empty());
}

/// A truncated t0425 read fails closed: a partial order list handed to reconciliation
/// makes nautilus treat the orders it did not see as gone.
#[tokio::test]
async fn a_truncated_order_read_fails_closed() {
    let server = MockServer::start().await;
    let client = client(&server).await;
    mount_t0425(&server, "NEXT", serde_json::json!([])).await;

    let err = client
        .generate_order_status_reports(&orders_cmd(false, None))
        .await
        .expect_err("truncation must refuse");
    assert!(err.to_string().contains("truncated"), "the reason names it: {err}");
}

/// An unrecognized 구분 is refused rather than defaulted — the same fail-closed rule
/// the outbound side mapping applies.
#[tokio::test]
async fn an_unrecognized_side_text_fails_closed() {
    let server = MockServer::start().await;
    let client = client(&server).await;
    mount_t0425(
        &server,
        "",
        serde_json::json!([
            { "ordno": "1001", "expcode": "005930", "medosu": "???", "qty": "10",
              "price": "60000", "cheqty": "0", "ordrem": "10", "status": "접수",
              "orgordno": "", "ordtime": "0900" }
        ]),
    )
    .await;

    client
        .generate_order_status_reports(&orders_cmd(false, None))
        .await
        .expect_err("an unknown side must refuse, never default to a live sell");
}
