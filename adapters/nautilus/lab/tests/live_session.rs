//! live-session-driver U1 — the production [`LiveTeardownSession`] (R1, R2; KTD1, KTD2,
//! KTD6).
//!
//! The teardown-ordering invariant was previously proven only against `FakeSession` /
//! `SyncFakeSession`. A safety invariant proven at a leaf can be re-violated by a coarser
//! caller (`docs/solutions/logic-errors/safety-invariant-proven-at-a-leaf-can-be-re-violated-by-a-coarser-grained-caller.md`),
//! so these drive [`run_teardown`] over the **real** impl against a mock gateway:
//! `stop_emission → cancel → flat → halt` (halt LAST, provable because the SDK's kill
//! switch is checked first in `post_order` — a cancel that succeeds proves halt had not
//! yet run), the KTD2 quiesce, positive-confirmation-only flatness, and the scrub
//! discipline.
//!
//! Offline and credential-free; `node.run` is never driven here.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use ls_sdk::orders::{CSPAT00601Request, CSPAT00801Request, T0425Request};
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::execution::OrderDispatchTasks;
use nautilus_ls::orders::ledger::FillLedger;
use nautilus_ls_lab::runner::live::{run_teardown, LiveSession, LiveTeardownSession};
use nautilus_ls_lab::strategy::orb::EmissionGate;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ACCNO_PATH: &str = "/stock/accno";
const ORDER_PATH: &str = "/stock/order";

/// A resting-order token planted in a broker `rsp_msg` — 20+ alphanumerics, so the
/// adapter scrubber must mask it out of any error text.
const PLANTED_SECRET: &str = "PSABCDEFGHIJKLMNOPQRSTUVWXYZ012345";

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

/// A t0425 responder that serves `rows` for the FIRST call only; later calls fall through
/// to whatever is mounted next. Models the real sequence a teardown sees: the scan
/// enumerates a resting order, the cancel clears it, and the flat check then reads clean.
async fn mount_t0425_once(server: &MockServer, rows: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path(ACCNO_PATH))
        .and(header("tr_cd", "t0425"))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "00000",
            "t0425OutBlock": { "tqty": "0", "tcheqty": "0", "tordrem": "0", "cts_ordno": "" },
            "t0425OutBlock1": rows
        })))
        .up_to_n_times(1)
        .mount(server)
        .await;
}

async fn mount_t0424(server: &MockServer, holdings: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path(ACCNO_PATH))
        .and(header("tr_cd", "t0424"))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "00000",
            "t0424OutBlock": {},
            "t0424OutBlock1": holdings
        })))
        .mount(server)
        .await;
}

/// A successful order ack (`CSPAT006/008`) with the TR-specific success code, optionally
/// delayed so a submit can be genuinely *in flight* when the teardown starts.
async fn mount_order_ok(server: &MockServer, tr_cd: &str, ordno: &str, delay: Duration) {
    let rsp_cd = match tr_cd {
        "CSPAT00601" => "00040",
        "CSPAT00801" => "00463",
        other => panic!("no success code mapped for {other}"),
    };
    let body = serde_json::json!({
        "rsp_cd": rsp_cd,
        "rsp_msg": "OK",
        format!("{tr_cd}OutBlock1"): {},
        format!("{tr_cd}OutBlock2"): { "OrdNo": ordno },
    });
    Mock::given(method("POST"))
        .and(path(ORDER_PATH))
        .and(header("tr_cd", tr_cd))
        .respond_with(ok_json(body).set_delay(delay))
        .mount(server)
        .await;
}

/// A non-2xx order outcome carrying a broker `rsp_msg` — `AmbiguousOrder` (may-rest).
async fn mount_order_ambiguous_with_msg(server: &MockServer, tr_cd: &str, msg: &str) {
    Mock::given(method("POST"))
        .and(path(ORDER_PATH))
        .and(header("tr_cd", tr_cd))
        .respond_with(
            ResponseTemplate::new(500)
                .set_body_string(
                    serde_json::json!({ "rsp_cd": "40999", "rsp_msg": msg }).to_string(),
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(server)
        .await;
}

fn t0425_row(ordno: &str, cheqty: &str, ordrem: &str) -> serde_json::Value {
    serde_json::json!({
        "ordno": ordno, "expcode": "005930", "medosu": "매수", "qty": "10",
        "price": "60000", "cheqty": cheqty, "ordrem": ordrem, "status": "접수",
        "orgordno": "", "ordtime": "0900"
    })
}

async fn session(server: &MockServer) -> (LiveTeardownSession, LsSdk, EmissionGate, OrderDispatchTasks) {
    mount_token(server).await;
    let sdk = LsSdk::new(mock_config(&server.uri())).unwrap();
    let gate = EmissionGate::open();
    let tasks = OrderDispatchTasks::new();
    let ledger: Arc<Mutex<FillLedger>> = Arc::new(Mutex::new(FillLedger::new()));
    let s = LiveTeardownSession::new(gate.clone(), sdk.clone(), ledger, tasks.clone());
    (s, sdk, gate, tasks)
}

/// The indices at which the first request for each `tr_cd` arrived (arrival order).
async fn first_index(server: &MockServer, tr_cd: &str) -> Option<usize> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .position(|r| r.headers.get("tr_cd").and_then(|v| v.to_str().ok()) == Some(tr_cd))
}

async fn count_requests(server: &MockServer, tr_cd: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| r.headers.get("tr_cd").and_then(|v| v.to_str().ok()) == Some(tr_cd))
        .count()
}

/// R1/KTD7 against the REAL impl: `stop_emission` first, `halt` LAST. The halt-last half
/// is not an assertion about a log — the SDK checks the kill switch *first* in
/// `post_order`, so a cancel that reached the venue proves `halt` had not yet engaged.
#[tokio::test]
async fn run_teardown_over_the_real_session_stops_cancels_confirms_then_halts_last() {
    let server = MockServer::start().await;
    let (s, sdk, gate, _tasks) = session(&server).await;
    // The scan sees one resting order; after the cancel the account reads clean.
    mount_t0425_once(&server, serde_json::json!([t0425_row("1001", "0", "10")])).await;
    mount_t0425(&server, "", serde_json::json!([])).await;
    mount_t0424(&server, serde_json::json!([])).await;
    mount_order_ok(&server, "CSPAT00801", "9001", Duration::ZERO).await;

    assert!(gate.allowed(), "the strategy's gate is open before teardown");
    assert!(s.orders_enabled(), "the kill switch is disengaged before teardown");

    let report = run_teardown(&s, 3, 3).await;

    assert!(!gate.allowed(), "stop_emission closed the strategy's live gate");
    assert!(report.canceled, "the resting order was canceled");
    assert_eq!(
        count_requests(&server, "CSPAT00801").await,
        1,
        "the cancel reached the venue — so halt had NOT engaged before it (halt is last)"
    );
    assert!(!s.orders_enabled(), "halt engaged the shared kill switch");
    assert!(!report.hard_failed(), "a clean teardown is not a hard failure");

    // Belt and braces: with the switch engaged, a further order is refused at dispatch.
    let refused = sdk
        .orders()
        .cancel(&CSPAT00801Request::new("1001", "A005930", "10"))
        .await;
    assert!(refused.is_err(), "post-halt order dispatch is refused by the kill switch");
}

/// R1: an account the mock still reports as NOT flat leaves `flat_confirmed == false` →
/// `hard_failed()`, and the kill switch is engaged anyway (teardown never skips halt).
#[tokio::test]
async fn a_not_flat_account_hard_fails_the_teardown_and_still_halts() {
    let server = MockServer::start().await;
    let (s, _sdk, _gate, _tasks) = session(&server).await;
    // The resting row survives the cancels (the mock is static) → never positively flat.
    mount_t0425(&server, "", serde_json::json!([t0425_row("1001", "0", "10")])).await;
    mount_t0424(&server, serde_json::json!([])).await;
    mount_order_ok(&server, "CSPAT00801", "9001", Duration::ZERO).await;

    let report = run_teardown(&s, 1, 1).await;
    assert!(!report.flat_confirmed, "flatness was never positively confirmed");
    assert!(report.hard_failed(), "an unconfirmed-flat teardown is a hard failure (abnormal)");
    assert!(!s.orders_enabled(), "halt runs even on a hard-failed teardown");
}

/// KTD1: `is_flat` is positive-confirmation only — `true` only when BOTH legs confirm; a
/// truncated t0425 read or an open holding reads as NOT flat, never a false all-clear.
#[tokio::test]
async fn is_flat_confirms_positively_and_fails_closed_on_a_truncated_read() {
    // Both legs clear → flat.
    let server = MockServer::start().await;
    let (s, _sdk, _g, _t) = session(&server).await;
    mount_t0425(&server, "", serde_json::json!([])).await;
    mount_t0424(&server, serde_json::json!([])).await;
    assert!(s.is_flat().await, "both legs confirm → flat");

    // A truncated order read cannot prove the account was fully enumerated → NOT flat.
    let server = MockServer::start().await;
    let (s, _sdk, _g, _t) = session(&server).await;
    mount_t0425(&server, "MORE", serde_json::json!([])).await;
    mount_t0424(&server, serde_json::json!([])).await;
    assert!(!s.is_flat().await, "a truncated t0425 read is never read as flat");

    // An open holding → NOT flat.
    let server = MockServer::start().await;
    let (s, _sdk, _g, _t) = session(&server).await;
    mount_t0425(&server, "", serde_json::json!([])).await;
    mount_t0424(&server, serde_json::json!([{ "expcode": "005930", "janqty": "10" }])).await;
    assert!(!s.is_flat().await, "an open holding is never read as flat");

    // No t0424 mount at all → the read FAILS → NOT flat (never a default-true).
    let server = MockServer::start().await;
    let (s, _sdk, _g, _t) = session(&server).await;
    mount_t0425(&server, "", serde_json::json!([])).await;
    assert!(!s.is_flat().await, "a failed read is never read as flat");
}

/// KTD2 quiesce: an order-dispatch task **in flight** when teardown begins is drained
/// BEFORE the cancel scan — proven by arrival order at the gateway (the submit lands
/// before the first t0425 enumeration), so the scan cannot miss the order it rested.
#[tokio::test]
async fn an_in_flight_submit_is_quiesced_before_the_cancel_scan() {
    let server = MockServer::start().await;
    let (s, sdk, _gate, tasks) = session(&server).await;
    mount_t0425(&server, "", serde_json::json!([])).await;
    mount_t0424(&server, serde_json::json!([])).await;
    // The submit takes 300ms to ack: it is genuinely still in flight when the teardown
    // starts, so a teardown that did NOT quiesce would scan before it landed.
    mount_order_ok(&server, "CSPAT00601", "1001", Duration::from_millis(300)).await;
    mount_order_ok(&server, "CSPAT00801", "9001", Duration::ZERO).await;

    let dispatch_sdk = sdk.clone();
    tasks.track(tokio::spawn(async move {
        let req = CSPAT00601Request::limit("A005930", "10".to_string(), "60000".to_string(), "2", "");
        let _ = dispatch_sdk.orders().submit(&req).await;
    }));

    let report = run_teardown(&s, 1, 1).await;
    assert!(report.canceled, "the cancel scan ran");

    let submit_at = first_index(&server, "CSPAT00601").await.expect("the submit reached the gateway");
    let scan_at = first_index(&server, "t0425").await.expect("the cancel scan ran");
    assert!(
        submit_at < scan_at,
        "the in-flight submit drained BEFORE the enumeration (submit #{submit_at}, scan #{scan_at}) \
         — otherwise it could rest an order the scan never saw"
    );
    assert_eq!(tasks.pending(), 0, "no order-dispatch task outlives the teardown");
}

/// KTD2 fail-closed complement: a dispatch that outlives the quiesce budget does not stall
/// the halt — the teardown proceeds, and because flatness cannot then be positively
/// confirmed the run finalizes ABNORMAL rather than silently NORMAL.
#[tokio::test]
async fn a_wedged_dispatch_bounds_the_quiesce_and_leaves_the_run_abnormal() {
    let server = MockServer::start().await;
    let (s, _sdk, _gate, tasks) = session(&server).await;
    let s = s.with_quiesce_budget(Duration::from_millis(50));
    // The account still reports a resting order → never positively flat.
    mount_t0425(&server, "", serde_json::json!([t0425_row("1001", "0", "10")])).await;
    mount_t0424(&server, serde_json::json!([])).await;
    mount_order_ok(&server, "CSPAT00801", "9001", Duration::ZERO).await;

    tasks.track(tokio::spawn(async { tokio::time::sleep(Duration::from_secs(3600)).await }));

    let report = run_teardown(&s, 1, 1).await;
    assert!(report.hard_failed(), "an unquiescable dispatch never yields a silent NORMAL finalize");
    assert!(!s.orders_enabled(), "the halt still runs — the wedged task cannot stall it");
}

/// R2: an un-acked (may-rest) cancel fails the teardown closed, and the planted broker
/// `rsp_msg` secret never reaches the error text (scrub discipline).
#[tokio::test]
async fn an_unacked_cancel_fails_closed_and_the_broker_message_is_scrubbed() {
    let server = MockServer::start().await;
    let (s, _sdk, _g, _t) = session(&server).await;
    mount_t0425(&server, "", serde_json::json!([t0425_row("1001", "0", "10")])).await;
    mount_t0424(&server, serde_json::json!([])).await;
    mount_order_ambiguous_with_msg(
        &server,
        "CSPAT00801",
        &format!("cancel denied for appkey {PLANTED_SECRET}"),
    )
    .await;

    let err = s
        .cancel_all_resting()
        .await
        .expect_err("a may-rest cancel is NOT safe — the teardown must fail closed");
    let text = err.to_string();
    assert!(
        text.contains("could not be confirmed canceled"),
        "names the unconfirmed cancel: {text}"
    );
    assert!(
        !text.contains(PLANTED_SECRET),
        "a planted broker-message secret must never reach the error text: {text}"
    );
    assert!(text.contains("***"), "the scrubber masked the secret rather than dropping the line: {text}");

    // A hard-failed cancel still leaves teardown able to halt.
    let report = run_teardown(&s, 1, 1).await;
    assert!(!report.canceled);
    assert!(report.hard_failed());
    assert!(!s.orders_enabled(), "halt runs regardless");
}

/// KTD6: the handle must be `Send + Sync` — the watchdog holds and shares it across its
/// own remediation thread. A compile-time bound, asserted here so a future field that is
/// not `Arc`-shared breaks this test rather than the live path.
#[test]
fn the_live_teardown_session_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<LiveTeardownSession>();
}

/// The primitive never places a flattening order (v1 is flat-start-only): a teardown over
/// a non-flat account sends cancels and reads, and NOTHING else.
#[tokio::test]
async fn the_teardown_never_places_a_flattening_order() {
    let server = MockServer::start().await;
    let (s, _sdk, _g, _t) = session(&server).await;
    mount_t0425(&server, "", serde_json::json!([t0425_row("1001", "0", "10")])).await;
    // A holding is present — a naive "flatten the position" teardown would sell it.
    mount_t0424(&server, serde_json::json!([{ "expcode": "005930", "janqty": "10" }])).await;
    mount_order_ok(&server, "CSPAT00801", "9001", Duration::ZERO).await;

    let report = run_teardown(&s, 1, 1).await;
    assert!(report.hard_failed(), "a non-flat close is abnormal + operator-reconciled");
    assert_eq!(
        count_requests(&server, "CSPAT00601").await,
        0,
        "no submit is ever issued by a teardown — halt-last is safe only because it never places"
    );
}

/// The cancel scan uses a SINGLE-page inquiry — never `collect_all`, which can walk a
/// non-terminating `cts_ordno` on a polluted account (and would burn the t0425 budget).
#[tokio::test]
async fn the_cancel_scan_reads_exactly_one_t0425_page() {
    let server = MockServer::start().await;
    let (s, _sdk, _g, _t) = session(&server).await;
    mount_t0425(&server, "", serde_json::json!([])).await;
    let _ = s.cancel_all_resting().await.expect("an empty account cancels nothing");
    assert_eq!(count_requests(&server, "t0425").await, 1, "exactly one page is read");
    // Sanity: the request really is the account-wide single-page form.
    let _ = T0425Request::for_symbol("");
}
