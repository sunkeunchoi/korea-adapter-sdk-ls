//! U7 offline integration: the BOOK predicate (`verify_book_on`), the paginated t0424
//! enumeration behind it, the two start postures, and the D+2 deposit reader. Covers
//! AE3. No live calls.
//!
//! The predicate is the thing that can still say no while a refusal is cheap: it runs
//! BEFORE the node is built, so a mismatch costs an exit 71 and places no orders. Every
//! test here is therefore about a way it must refuse, plus the one shape it accepts.

use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_common::clients::ExecutionClient;
use nautilus_ls::execution::{
    read_d2_deposit_on, verify_book_on, ExpectedBook, LsExecClient, StartPosture,
};
use nautilus_model::enums::AccountType;
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ACCNO_PATH: &str = "/stock/accno";

fn ok_json(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_string(body.to_string())
        .insert_header("content-type", "application/json")
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

/// Mount ONE t0424 page, keyed on the `cts_expcode` the request carries: `""` is the
/// first page, and each page hands back the cursor its successor is mounted under.
/// That is exactly the continuation contract, so a client that fails to echo the
/// cursor simply never sees page 2.
async fn mount_t0424_page(
    server: &MockServer,
    request_cursor: &str,
    next_cursor: &str,
    rows: serde_json::Value,
) {
    Mock::given(method("POST"))
        .and(path(ACCNO_PATH))
        .and(header("tr_cd", "t0424"))
        .and(body_partial_json(serde_json::json!({
            "t0424InBlock": { "cts_expcode": request_cursor }
        })))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "00000",
            "t0424OutBlock": { "sunamt": "500000000", "cts_expcode": next_cursor },
            "t0424OutBlock1": rows
        })))
        .mount(server)
        .await;
}

/// A t0424 page that FAILS (HTTP 500) for the given request cursor.
async fn mount_t0424_page_failure(server: &MockServer, request_cursor: &str) {
    Mock::given(method("POST"))
        .and(path(ACCNO_PATH))
        .and(header("tr_cd", "t0424"))
        .and(body_partial_json(serde_json::json!({
            "t0424InBlock": { "cts_expcode": request_cursor }
        })))
        .respond_with(ResponseTemplate::new(500).set_body_string("upstream failure"))
        .mount(server)
        .await;
}

async fn sdk_for(server: &MockServer) -> LsSdk {
    mount_token(server).await;
    LsSdk::new(mock_config(&server.uri())).unwrap()
}

fn client_over(sdk: &LsSdk) -> LsExecClient {
    LsExecClient::new(
        "LS-KRX",
        "LS-TRADER-001",
        "00000000-01",
        sdk.clone(),
        AccountType::Cash,
    )
}

/// Synthetic six-digit issue codes: `100000`, `100001`, ... — enough to build the
/// plan's 128-holding book without pretending to know 128 real KRX symbols.
fn symbols(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("{:06}", 100_000 + i)).collect()
}

fn rows_for(syms: &[String], qty: i64) -> serde_json::Value {
    serde_json::Value::Array(
        syms.iter()
            .map(|s| {
                serde_json::json!({
                    "expcode": s, "janqty": qty.to_string(), "mdposqt": qty.to_string(),
                    "pamt": "60000", "price": "61000", "appamt": (61_000 * qty).to_string(),
                    "hname": "테스트종목"
                })
            })
            .collect::<Vec<_>>(),
    )
}

/// Mount a 128-symbol book across THREE pages (50 / 50 / 28), optionally dropping one
/// symbol from the middle page. Returns the full expected symbol list.
async fn mount_three_page_book(server: &MockServer, drop_index: Option<usize>) -> Vec<String> {
    let all = symbols(128);
    let mut pages: Vec<Vec<String>> = vec![
        all[0..50].to_vec(),
        all[50..100].to_vec(),
        all[100..128].to_vec(),
    ];
    if let Some(i) = drop_index {
        for page in &mut pages {
            page.retain(|s| *s != all[i]);
        }
    }
    // Each page's cursor is the last symbol the PREVIOUS page reported, mirroring the
    // gateway's `cts_expcode` semantics; the final page returns an empty cursor.
    mount_t0424_page(server, "", &all[49], rows_for(&pages[0], 10)).await;
    mount_t0424_page(server, &all[49], &all[99], rows_for(&pages[1], 10)).await;
    mount_t0424_page(server, &all[99], "", rows_for(&pages[2], 10)).await;
    all
}

/// The shape the predicate ACCEPTS: 128 expected holdings reported exactly, across
/// three pages, with no resting order.
#[tokio::test]
async fn a_128_holding_book_reported_across_three_pages_matches() {
    let server = MockServer::start().await;
    let sdk = sdk_for(&server).await;
    mount_t0425(&server, "", serde_json::json!([])).await;
    let all = mount_three_page_book(&server, None).await;

    let expected = ExpectedBook::from_pairs(all.iter().map(|s| (s.clone(), 10)));
    assert_eq!(expected.len(), 128);
    verify_book_on(&sdk, &expected)
        .await
        .expect("an exactly-matching book across three pages passes");
}

/// AE3: 128 expected, 127 reported across three pages → refuse, naming the missing
/// symbol. The pre-mount probe is the thing that can still refuse cheaply, so it must
/// say WHICH symbol is gone rather than only that something is.
#[tokio::test]
async fn a_missing_symbol_refuses_and_cites_it() {
    let server = MockServer::start().await;
    let sdk = sdk_for(&server).await;
    mount_t0425(&server, "", serde_json::json!([])).await;
    let all = mount_three_page_book(&server, Some(75)).await;
    let missing = all[75].clone();

    let expected = ExpectedBook::from_pairs(all.iter().map(|s| (s.clone(), 10)));
    let err = verify_book_on(&sdk, &expected)
        .await
        .expect_err("127 reported against 128 expected is a mismatch");
    let msg = err.to_string();
    assert!(msg.contains(&missing), "the missing symbol is cited: {msg}");
    assert!(msg.contains("absent"), "the reason says what is wrong: {msg}");
}

/// One quantity differing is a mismatch — the predicate compares `janqty`, not just
/// the symbol set. A book that is right about WHAT is held and wrong about HOW MUCH
/// sizes every exit wrong.
#[tokio::test]
async fn a_differing_quantity_refuses() {
    let server = MockServer::start().await;
    let sdk = sdk_for(&server).await;
    mount_t0425(&server, "", serde_json::json!([])).await;
    mount_t0424_page(
        &server,
        "",
        "",
        serde_json::json!([
            { "expcode": "005930", "janqty": "10", "pamt": "60000" },
            { "expcode": "035720", "janqty": "7", "pamt": "50000" }
        ]),
    )
    .await;

    let expected = ExpectedBook::from_pairs([("005930", 10), ("035720", 10)]);
    let err = verify_book_on(&sdk, &expected).await.expect_err("7 != 10");
    let msg = err.to_string();
    assert!(msg.contains("035720"), "the offending symbol is cited: {msg}");
    assert!(msg.contains("quantity mismatch"), "the reason is the quantity: {msg}");
}

/// An unexpected holding is a mismatch too. A rehearsal that adopts a position its
/// `book.json` never recorded has no entry ordinal, no stop and no `entered_under` for
/// it, so it cannot manage it — refusing is the only honest option.
#[tokio::test]
async fn an_unexpected_holding_refuses() {
    let server = MockServer::start().await;
    let sdk = sdk_for(&server).await;
    mount_t0425(&server, "", serde_json::json!([])).await;
    mount_t0424_page(
        &server,
        "",
        "",
        serde_json::json!([
            { "expcode": "005930", "janqty": "10", "pamt": "60000" },
            { "expcode": "000660", "janqty": "3", "pamt": "150000" }
        ]),
    )
    .await;

    let expected = ExpectedBook::from_pairs([("005930", 10)]);
    let err = verify_book_on(&sdk, &expected)
        .await
        .expect_err("a holding nobody expected is a mismatch");
    let msg = err.to_string();
    assert!(msg.contains("000660"), "the unexpected symbol is cited: {msg}");
    assert!(msg.contains("unexpected"), "the reason says so: {msg}");
}

/// A resting t0425 order is a mismatch whatever the holdings say: a working order
/// means the book is still moving, and a book that is still moving is not one a
/// session can assert it inherited.
#[tokio::test]
async fn a_resting_order_refuses_even_when_the_holdings_match() {
    let server = MockServer::start().await;
    let sdk = sdk_for(&server).await;
    mount_t0425(
        &server,
        "",
        serde_json::json!([
            { "ordno": "1001", "expcode": "005930", "medosu": "매수", "qty": "10",
              "price": "60000", "cheqty": "0", "ordrem": "10", "status": "접수",
              "orgordno": "", "ordtime": "0900" }
        ]),
    )
    .await;
    mount_t0424_page(
        &server,
        "",
        "",
        serde_json::json!([{ "expcode": "005930", "janqty": "10", "pamt": "60000" }]),
    )
    .await;

    let expected = ExpectedBook::from_pairs([("005930", 10)]);
    let err = verify_book_on(&sdk, &expected)
        .await
        .expect_err("a resting order refuses the book");
    assert!(err.to_string().contains("open"), "the reason names open orders: {err}");
}

/// A failed SECOND page is a mismatch, not a short read. The enumeration is only
/// evidence if it completed: page 1 alone would report a subset and "match" a book
/// that expected only that subset.
#[tokio::test]
async fn a_failed_continuation_page_refuses() {
    let server = MockServer::start().await;
    let sdk = sdk_for(&server).await;
    mount_t0425(&server, "", serde_json::json!([])).await;
    mount_t0424_page(
        &server,
        "",
        "PAGE2",
        serde_json::json!([{ "expcode": "005930", "janqty": "10", "pamt": "60000" }]),
    )
    .await;
    mount_t0424_page_failure(&server, "PAGE2").await;

    let expected = ExpectedBook::from_pairs([("005930", 10)]);
    verify_book_on(&sdk, &expected)
        .await
        .expect_err("a failed continuation page must refuse, never report page 1 as the book");
}

/// A gateway that keeps echoing the same cursor is a non-terminating enumeration. It
/// must be an error, not an infinite pre-mount probe: this runs before the node is
/// built, so a hang here is a session that never starts and never says why.
#[tokio::test]
async fn a_non_advancing_cursor_refuses_rather_than_spinning() {
    let server = MockServer::start().await;
    let sdk = sdk_for(&server).await;
    mount_t0425(&server, "", serde_json::json!([])).await;
    mount_t0424_page(
        &server,
        "",
        "STUCK",
        serde_json::json!([{ "expcode": "005930", "janqty": "10", "pamt": "60000" }]),
    )
    .await;
    mount_t0424_page(
        &server,
        "STUCK",
        "STUCK",
        serde_json::json!([{ "expcode": "005930", "janqty": "10", "pamt": "60000" }]),
    )
    .await;

    let err = verify_book_on(&sdk, &ExpectedBook::from_pairs([("005930", 10)]))
        .await
        .expect_err("a repeating cursor cannot enumerate the book");
    assert!(
        err.to_string().contains("non-advancing"),
        "the reason names the stuck cursor: {err}"
    );
}

/// A same-day round-tripped symbol lingers as a `janqty=0` row. It is not held, so an
/// empty expected book still matches — the flat gate's long-standing behavior, now
/// reached through the book predicate.
#[tokio::test]
async fn a_zero_quantity_row_is_not_a_holding_for_the_book_either() {
    let server = MockServer::start().await;
    let sdk = sdk_for(&server).await;
    mount_t0425(&server, "", serde_json::json!([])).await;
    mount_t0424_page(
        &server,
        "",
        "",
        serde_json::json!([{ "expcode": "005930", "janqty": "0", "mdposqt": "0" }]),
    )
    .await;

    verify_book_on(&sdk, &ExpectedBook::flat())
        .await
        .expect("a net-zero row is not a holding");
    // And the same row cannot satisfy a book that DOES expect the symbol.
    verify_book_on(&sdk, &ExpectedBook::from_pairs([("005930", 10)]))
        .await
        .expect_err("a zero row does not satisfy a 10-share expectation");
}

/// `BookAsserted` skips the flat assertion at connect — and skips ONLY that. `Flat`,
/// the default, still refuses the same non-empty account.
#[tokio::test]
async fn book_asserted_connects_on_a_non_empty_account_and_flat_refuses() {
    let server = MockServer::start().await;
    let sdk = sdk_for(&server).await;
    mount_t0425(&server, "", serde_json::json!([])).await;
    mount_t0424_page(
        &server,
        "",
        "",
        serde_json::json!([{ "expcode": "005930", "janqty": "10", "pamt": "60000" }]),
    )
    .await;

    let mut flat = client_over(&sdk);
    assert_eq!(flat.start_posture(), StartPosture::Flat, "the default is unchanged");
    flat.connect()
        .await
        .expect_err("the default posture still refuses a non-flat account (R14)");

    let mut asserted = client_over(&sdk).with_start_posture(StartPosture::BookAsserted);
    asserted
        .connect()
        .await
        .expect("BookAsserted inherits the book instead of refusing it");
    asserted.disconnect().await.expect("disconnect stops the spawned lanes");
}

/// The deposit reader takes t0424's **`sunamt1`** (추정D2예수금) — the settled D+2
/// cash — off the holdings inquiry the predicate already makes. The fixture carries the
/// exact R32 day-2 witness, where `sunamt1` had converged on the `d2dps` figure.
///
/// It costs no extra TR: the adapter may only newly consume an already-Recommended TR
/// (the Verification Bar), and `CSPAQ22200` — which carries the same number as `D2Dps`
/// — is not one.
#[tokio::test]
async fn the_deposit_reader_takes_the_settled_d2_figure() {
    let server = MockServer::start().await;
    let sdk = sdk_for(&server).await;
    Mock::given(method("POST"))
        .and(path(ACCNO_PATH))
        .and(header("tr_cd", "t0424"))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "00000",
            "t0424OutBlock": {
                "sunamt": "499977282", "sunamt1": "499717282", "tappamt": "260000",
                "cts_expcode": ""
            },
            "t0424OutBlock1": [
                { "expcode": "005930", "janqty": "1", "mdposqt": "1", "pamt": "265500" }
            ]
        })))
        .mount(&server)
        .await;

    assert_eq!(
        read_d2_deposit_on(&sdk).await.expect("the deposit reads"),
        499_717_282,
        "sunamt1 — the settled D+2 cash, not the 499,977,282 net-asset figure beside it"
    );
}

/// An unreadable deposit is never treated as sufficient.
#[tokio::test]
async fn an_unreadable_deposit_refuses() {
    let server = MockServer::start().await;
    let sdk = sdk_for(&server).await;
    Mock::given(method("POST"))
        .and(path(ACCNO_PATH))
        .and(header("tr_cd", "t0424"))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "00000",
            "t0424OutBlock": { "sunamt1": "n/a", "cts_expcode": "" },
            "t0424OutBlock1": []
        })))
        .mount(&server)
        .await;

    read_d2_deposit_on(&sdk)
        .await
        .expect_err("a deposit that does not parse is never proven sufficient");
}

/// A non-positive expected quantity is dropped rather than becoming an expectation
/// that no t0424 row can ever satisfy — `janqty=0` is how "not held" is spelled on
/// both sides of the comparison.
#[test]
fn expected_book_drops_non_positive_quantities() {
    let book = ExpectedBook::from_pairs([("005930", 10), ("035720", 0), ("000660", -1)]);
    assert_eq!(book.len(), 1);
    assert_eq!(book.iter().collect::<Vec<_>>(), vec![("005930", 10)]);
    assert!(ExpectedBook::flat().is_empty());
}
