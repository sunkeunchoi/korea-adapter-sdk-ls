//! Realtime (S3_) WebSocket integration suite — against the mock WS server.
//!
//! These tests exercise the REAL connect / replay / dispatch / reconnect code
//! paths by injecting the mock WS server URL through `LsConfig.ws_base_url` and a
//! wiremock token endpoint through `LsConfig.base_url`. They own eventual
//! delivery, replay, exhaustion, RAII, and latest-value semantics; the
//! wake-on-write and record-before-send invariants are owned by the deterministic
//! unit tests in `overflow.rs` and `realtime::mod.rs` respectively.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use ls_core::config::WsOverflowPolicy;
use ls_core::{Inner, LsConfig, LsError};
use ls_sdk::realtime::{S3Trade, WsManager};
use ls_sdk_test_support::{mock_config, mount_token, MockWsServer, MOCK_REJECTION_RSP_CD};
use tokio::time::timeout;
use wiremock::MockServer;

/// Build an `Arc<WsManager>` whose token refreshes hit `http_server` and whose
/// WebSocket connects to `ws_url`, with the given overflow policy.
async fn ws_manager_for(
    http_server: &MockServer,
    ws_url: &str,
    policy: WsOverflowPolicy,
) -> Arc<WsManager> {
    let config = LsConfig {
        ws_base_url: Some(ws_url.to_string()),
        ws_overflow_policy: Some(policy),
        // A short WS connect timeout keeps the exhaustion test's failed attempts
        // snappy without changing the (verbatim) 1+2+3+4s reconnect backoff.
        ws_connect_timeout_secs: Some(2),
        ..mock_config(&http_server.uri())
    };
    let inner = Inner::new(config).expect("inner builds");
    WsManager::from_inner(&inner)
}

/// Happy path: subscribe to S3_ (tr_type "3"); a pushed frame decodes to the
/// typed row and routes by composite key.
#[tokio::test]
async fn subscribe_s3_decodes_pushed_frame_and_routes_by_composite_key() {
    let http = MockServer::start().await;
    mount_token(&http).await;
    let ws = MockWsServer::start().await;

    let wm = ws_manager_for(&http, &ws.ws_url(), WsOverflowPolicy::DropNewest).await;

    let (_handle, mut stream) = wm
        .subscribe_typed::<S3Trade>("S3_", "005930", "3")
        .await
        .expect("subscribe_typed S3_");

    // The subscribe frame must reach the server as a tr_type "3" market-data
    // registration for S3_.
    wait_for(|| async { ws.count_subscribe_frames("S3_", "3").await >= 1 }).await;

    // Push an S3_ trade row keyed to 005930 and assert it decodes + routes.
    ws.push_s3(
        "005930",
        serde_json::json!({
            "price": "55550",
            "cvolume": "1",
            "volume": "10887",
            "cgubun": "+",
            "shcode": "005930",
        }),
    );

    let item = timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("a frame should arrive within 5s")
        .expect("stream should yield an item");
    let row: S3Trade = item.expect("frame should decode to S3Trade");
    assert_eq!(row.price, "55550");
    assert_eq!(row.cvolume, "1");
    assert_eq!(row.cgubun, "+");
    assert_eq!(row.shcode, "005930");
}

/// Integration: reconnect refreshes the token and replays the S3_ subscription.
///
/// The reconnect-token-refresh proof: the manager force-clears the token on each
/// (re)connect, so a kill-then-reconnect produces a SECOND token fetch. We assert
/// the replayed subscribe frame arrives after the kill (proving replay) and that
/// the token endpoint was hit more than once (proving the per-reconnect refresh).
#[tokio::test]
async fn reconnect_refreshes_token_and_replays_subscription() {
    let http = MockServer::start().await;
    // No `.expect(n)` — we read the request count from the server afterwards.
    mount_token(&http).await;
    let ws = MockWsServer::start().await;

    let wm = ws_manager_for(&http, &ws.ws_url(), WsOverflowPolicy::DropNewest).await;

    let (_handle, mut stream) = wm
        .subscribe_typed::<S3Trade>("S3_", "005930", "3")
        .await
        .expect("subscribe_typed S3_");

    wait_for(|| async { ws.count_subscribe_frames("S3_", "3").await >= 1 }).await;
    let token_hits_before = token_request_count(&http).await;
    assert!(token_hits_before >= 1, "initial connect must fetch a token");

    // Sever the live connection — the inbound EOF triggers auto-reconnect, which
    // refreshes the token and replays the stored S3_ subscription.
    ws.kill_connections();

    // After reconnect, a SECOND S3_ subscribe frame (the replay) must arrive.
    wait_for(|| async { ws.count_subscribe_frames("S3_", "3").await >= 2 }).await;

    // The replayed subscription delivers a freshly pushed frame end-to-end.
    ws.push_s3(
        "005930",
        serde_json::json!({ "price": "60000", "volume": "1" }),
    );
    let item = timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("a frame should arrive after reconnect")
        .expect("stream yields after reconnect");
    let row: S3Trade = item.expect("decodes after reconnect");
    assert_eq!(row.price, "60000");

    // The token was re-fetched on reconnect (clear-then-refresh).
    let token_hits_after = token_request_count(&http).await;
    assert!(
        token_hits_after > token_hits_before,
        "reconnect must refresh the token (clear + get_or_refresh): \
         before={token_hits_before} after={token_hits_after}"
    );
}

/// Integration: an order-event subscription (tr_type "1") is replayed with
/// tr_type "1" after a reconnect — NOT the market-data "3".
///
/// The per-subscription `tr_type` (U2) must survive reconnect replay so an
/// order-event channel re-registers as 실시간 계좌 등록 ("1"), not 실시간 시세 등록
/// ("3"). A subscription stored with only its `tr_cd` (the pre-U2 shape) would
/// replay the hardcoded "3" and silently re-register on the wrong lane.
#[tokio::test]
async fn reconnect_replays_order_event_subscription_with_tr_type_1() {
    let http = MockServer::start().await;
    mount_token(&http).await;
    let ws = MockWsServer::start().await;

    let wm = ws_manager_for(&http, &ws.ws_url(), WsOverflowPolicy::DropNewest).await;

    // Subscribe an order-event channel: tr_type "1", account-bound empty tr_key.
    // The decode type is irrelevant for a lifecycle/replay test (no row pushed).
    let (_handle, _stream) = wm
        .subscribe_typed::<S3Trade>("SC0", "", "1")
        .await
        .expect("subscribe_typed SC0 (order-event)");

    // The initial subscribe registers on the order-event lane ("1"), never "3".
    wait_for(|| async { ws.count_subscribe_frames("SC0", "1").await >= 1 }).await;
    assert_eq!(
        ws.count_subscribe_frames("SC0", "3").await,
        0,
        "an order-event subscription must never register with the market-data tr_type \"3\""
    );

    // Sever the connection; auto-reconnect replays the stored subscription.
    ws.kill_connections();

    // The replay re-registers with tr_type "1" (proving the lane survived),
    // and still never emits a "3" frame for this TR.
    wait_for(|| async { ws.count_subscribe_frames("SC0", "1").await >= 2 }).await;
    assert_eq!(
        ws.count_subscribe_frames("SC0", "3").await,
        0,
        "reconnect replay must reuse the stored tr_type \"1\", not the hardcoded \"3\""
    );
}

/// Edge: reconnect-budget exhaustion (4 attempts) delivers the terminal
/// `WebSocket` error to subscribers and cleans up.
///
/// After the connection is severed AND the server's port is closed, all four
/// reconnect attempts fail; the manager then delivers
/// `LsError::WebSocket("reconnect budget exhausted")` and removes the dispatch +
/// subscription entries.
#[tokio::test]
async fn reconnect_budget_exhaustion_delivers_terminal_error_and_cleans_up() {
    let http = MockServer::start().await;
    mount_token(&http).await;
    let ws = MockWsServer::start().await;

    let wm = ws_manager_for(&http, &ws.ws_url(), WsOverflowPolicy::DropNewest).await;

    let (_handle, mut stream) = wm
        .subscribe_typed::<S3Trade>("S3_", "005930", "3")
        .await
        .expect("subscribe_typed S3_");
    wait_for(|| async { ws.count_subscribe_frames("S3_", "3").await >= 1 }).await;
    assert_eq!(wm.dispatch_len(), 1);

    // Sever the live connection AND close the port so every reconnect fails.
    ws.shutdown();

    // The reconnect loop sleeps 1+2+3+4s between attempts; with a 2s connect
    // timeout the failures are fast, so the terminal error arrives within ~15s.
    let item = timeout(Duration::from_secs(20), stream.next())
        .await
        .expect("terminal error must arrive before the timeout bound")
        .expect("stream yields the terminal error item");
    match item {
        Err(LsError::WebSocket(msg)) => {
            assert!(
                msg.contains("reconnect budget exhausted"),
                "wrong terminal error: {msg}"
            );
        }
        other => panic!("expected terminal WebSocket error, got {other:?}"),
    }

    // After the terminal error, the stream ends (None) and state is cleaned up.
    let end = timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("stream should end promptly after the terminal error");
    assert!(end.is_none(), "stream must end after the terminal error");

    wait_for(|| async { wm.dispatch_len() == 0 && wm.subscription_count() == 0 }).await;
}

/// Edge: `LatestOnly` overflow yields the newest frame and an explicit terminal
/// `None` on unsubscribe (lost-wakeup regression at the integration level).
#[tokio::test]
async fn latest_only_yields_newest_then_terminal_none_on_unsubscribe() {
    let http = MockServer::start().await;
    mount_token(&http).await;
    let ws = MockWsServer::start().await;

    let wm = ws_manager_for(&http, &ws.ws_url(), WsOverflowPolicy::LatestOnly).await;

    let (handle, mut stream) = wm
        .subscribe_typed::<S3Trade>("S3_", "005930", "3")
        .await
        .expect("subscribe_typed S3_ latest-only");
    wait_for(|| async { ws.count_subscribe_frames("S3_", "3").await >= 1 }).await;

    // Push a frame; the latest-only slot holds it and wakes the consumer.
    ws.push_s3(
        "005930",
        serde_json::json!({ "price": "70000", "volume": "5" }),
    );
    let item = timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("latest-only frame should arrive")
        .expect("stream yields the latest frame");
    let row: S3Trade = item.expect("latest frame decodes");
    assert_eq!(row.price, "70000");

    // Eager unsubscribe closes the slot; the stream must then end with None.
    handle.unsubscribe().await.expect("unsubscribe ok");
    let end = timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("latest-only stream must end promptly after unsubscribe");
    assert!(
        end.is_none(),
        "LatestOnly stream must yield an explicit terminal None after unsubscribe"
    );
}

/// Edge: RAII `SubscriptionHandle` drop unsubscribes — the dispatch + replay
/// entries are removed without an explicit `.unsubscribe()` call.
#[tokio::test]
async fn dropping_subscription_handle_unsubscribes() {
    let http = MockServer::start().await;
    mount_token(&http).await;
    let ws = MockWsServer::start().await;

    let wm = ws_manager_for(&http, &ws.ws_url(), WsOverflowPolicy::DropNewest).await;

    let (handle, _stream) = wm
        .subscribe_typed::<S3Trade>("S3_", "005930", "3")
        .await
        .expect("subscribe_typed S3_");
    wait_for(|| async { ws.count_subscribe_frames("S3_", "3").await >= 1 }).await;
    assert_eq!(wm.dispatch_len(), 1);
    assert!(wm.has_subscription("S3_", "005930"));

    // Drop the handle — RAII fires a fire-and-forget unsubscribe.
    drop(handle);

    // The unsubscribe frame (tr_type "4") reaches the server, and local state is
    // removed.
    wait_for(|| async { ws.count_subscribe_frames("S3_", "4").await >= 1 }).await;
    wait_for(|| async { wm.dispatch_len() == 0 && !wm.has_subscription("S3_", "005930") }).await;
}

// ── U3: generic lifecycle (positive) + executable negative control ──────────

/// A permissive lifecycle row: lifecycle-only smokes never require a real row,
/// so any inbound body — including an error-shaped rejection ACK — decodes here
/// without aborting the stream. Mirrors the permissive decode the generic
/// lifecycle smoke (`live_smoke.rs`) uses.
#[derive(serde::Deserialize, Debug, Default)]
struct LifecycleRow {
    /// Present only on a rejection ACK body; empty/absent on a real push or a
    /// silent (accepted, no-row) lifecycle.
    #[serde(default)]
    rsp_cd: String,
}

/// Positive lifecycle, market-data lane (tr_type "3"): subscribe → push nothing
/// → unsubscribe completes cleanly, with no inbound frame observed in the
/// timebox (row absence is bonus-not-required). `Covers AE1`.
#[tokio::test]
async fn lifecycle_tr_type_3_subscribe_no_push_unsubscribe_clean() {
    let http = MockServer::start().await;
    mount_token(&http).await;
    let ws = MockWsServer::start().await;

    let wm = ws_manager_for(&http, &ws.ws_url(), WsOverflowPolicy::DropNewest).await;

    let (handle, mut stream) = wm
        .subscribe_typed::<LifecycleRow>("S3_", "005930", "3")
        .await
        .expect("market-data subscribe lifecycle");
    wait_for(|| async { ws.count_subscribe_frames("S3_", "3").await >= 1 }).await;

    // No push: a row may or may not arrive; absence within the timebox is NOT a
    // failure (lifecycle gate is connect/subscribe/unsubscribe).
    let row = timeout(Duration::from_millis(300), stream.next()).await;
    assert!(
        row.is_err(),
        "no inbound frame is expected when the server pushes nothing; got {row:?}"
    );

    handle
        .unsubscribe()
        .await
        .expect("unsubscribe must complete cleanly");
    wait_for(|| async { ws.count_subscribe_frames("S3_", "4").await >= 1 }).await;
    wait_for(|| async { !wm.has_subscription("S3_", "005930") }).await;
}

/// Positive lifecycle, order-event lane (tr_type "1"): subscribe → push nothing
/// → unsubscribe completes cleanly, registering "1" and deregistering "2".
#[tokio::test]
async fn lifecycle_tr_type_1_subscribe_no_push_unsubscribe_clean() {
    let http = MockServer::start().await;
    mount_token(&http).await;
    let ws = MockWsServer::start().await;

    let wm = ws_manager_for(&http, &ws.ws_url(), WsOverflowPolicy::DropNewest).await;

    // Order-event channel: account-bound empty tr_key, tr_type "1".
    let (handle, mut stream) = wm
        .subscribe_typed::<LifecycleRow>("SC0", "", "1")
        .await
        .expect("order-event subscribe lifecycle");
    wait_for(|| async { ws.count_subscribe_frames("SC0", "1").await >= 1 }).await;

    let row = timeout(Duration::from_millis(300), stream.next()).await;
    assert!(row.is_err(), "no inbound frame expected; got {row:?}");

    handle
        .unsubscribe()
        .await
        .expect("unsubscribe must complete cleanly");
    wait_for(|| async { ws.count_subscribe_frames("SC0", "2").await >= 1 }).await;
    wait_for(|| async { !wm.has_subscription("SC0", "") }).await;
}

/// THE EXECUTABLE NEGATIVE CONTROL (KTD6): a rejected `tr_cd` is observably
/// distinguishable from an accepted one, so a smoke built on the lifecycle gate
/// CAN FAIL. `Covers AE2`.
///
/// The mock gateway is configured to reject `BAD` (and accept `S3_`). A subscribe
/// for `BAD` triggers an in-band, composite-key-routed error ACK; subscribing
/// `S3_` triggers nothing. We prove the two are distinguishable on the SUBSCRIBER
/// STREAM within one timebox: the rejected stream yields a body carrying a
/// non-empty `rsp_cd`; the accepted stream yields nothing. A lifecycle smoke that
/// treated "subscribe returned Ok" as the gate would flip BAD; this test shows
/// the observable signal a real smoke would assert on to refuse the flip.
#[tokio::test]
async fn negative_control_rejected_tr_cd_is_observably_distinct_from_accepted() {
    let http = MockServer::start().await;
    mount_token(&http).await;
    let ws = MockWsServer::start_rejecting(&["BAD"]).await;
    assert_eq!(ws.rejected_tr_cds(), &["BAD".to_string()]);

    let wm = ws_manager_for(&http, &ws.ws_url(), WsOverflowPolicy::DropNewest).await;

    // The bad subscribe still returns Ok — the subscribe path is fire-and-forget
    // and never reads an ACK. This is exactly the trap KTD6 names: Ok alone does
    // NOT prove reachability.
    let (_bad_handle, mut bad_stream) = wm
        .subscribe_typed::<LifecycleRow>("BAD", "005930", "3")
        .await
        .expect("subscribe call itself returns Ok (fire-and-forget)");

    // The rejection ACK is routed back to THIS subscriber by composite key and
    // surfaces on the stream as a body carrying a non-empty rsp_cd.
    let rejected_item = timeout(Duration::from_secs(5), bad_stream.next())
        .await
        .expect("a rejection ACK must arrive for a rejected tr_cd")
        .expect("stream yields the rejection item");
    let rejected_row = rejected_item.expect("rejection body decodes into the permissive row");
    assert_eq!(
        rejected_row.rsp_cd, MOCK_REJECTION_RSP_CD,
        "a rejected subscribe must surface a non-empty business rsp_cd to the subscriber"
    );

    // Control: an ACCEPTED subscribe (S3_, not rejected) yields NOTHING in the
    // same timebox — no rsp_cd, no row — so the rejection is a genuine signal,
    // not noise the accepted path also produces.
    let (_good_handle, mut good_stream) = wm
        .subscribe_typed::<LifecycleRow>("S3_", "005930", "3")
        .await
        .expect("good subscribe");
    wait_for(|| async { ws.count_subscribe_frames("S3_", "3").await >= 1 }).await;
    let accepted = timeout(Duration::from_millis(500), good_stream.next()).await;
    assert!(
        accepted.is_err(),
        "an accepted subscribe must NOT produce a rejection ACK; got {accepted:?}"
    );
}

// ── helpers ────────────────────────────────────────────────────────────────

/// Poll `cond` every 25ms until it returns true, bounded to ~5s. The bound is a
/// fail-fast guard, not part of the observation path of any wakeup invariant
/// (those are owned by the deterministic unit tests).
async fn wait_for<F, Fut>(mut cond: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..200 {
        if cond().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("condition not met within ~5s");
}

/// Count requests the wiremock server has received against `/oauth2/token`.
async fn token_request_count(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| r.url.path() == "/oauth2/token")
        .count()
}
