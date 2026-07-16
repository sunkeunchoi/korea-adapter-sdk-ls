//! U2 — precondition checks + the deferral surface (R1, R3, KTD5, AE1).
//!
//! The pure checks are exercised in both directions plus their fail-closed arms via a
//! hand-built [`DispatchContext`]; the two gateway probes and their IGW00201
//! throttle-vs-terminal classification are exercised against wiremock through the split
//! `execution.rs` legs. No live calls.

use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::execution::LsExecClient;
use nautilus_ls_lab::dispatch::checks::{
    check_flat_start as pure_flat_start, decide, parse_deferrals, probe_flat_start,
    probe_stranded_orders, run_checks, BudgetHeadroom, DispatchContext, GatewayProbe, GateResult,
    LanePosture, TradingCalendar, WeekdayKrxCalendar, CHECK_ADVISORY_LOCK, CHECK_BUDGET,
    CHECK_KILL_SWITCH, CHECK_STRANDED, CHECK_TRADING_ENV,
};
use nautilus_ls_lab::dispatch::CheckStatus;
use nautilus_model::enums::AccountType;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ---------------------------------------------------------------------------
// Pure-check fixtures
// ---------------------------------------------------------------------------

fn green_ctx() -> DispatchContext {
    DispatchContext {
        now_unix: 1_000_000,
        today_kst: "2026-07-16".into(),
        trading_env: Some("paper".into()),
        lane_env_present: true,
        resolved_env_is_paper: Some(true),
        live_lock_held: false,
        window_open: true,
        watermark_fresh: true,
        bars_present: true,
        flat_start: GatewayProbe::Clear,
        stranded_orders: GatewayProbe::Clear,
        kill_switch_engaged: false,
        kill_switch_has_record: true,
        budget: BudgetHeadroom::Measured { remaining: 100, plan: 10 },
        chain_authorized_rung: 1,
        requested_rung: 1,
        lane: LanePosture::Live,
    }
}

fn verdict(ctx: &DispatchContext, deferrals: &[&str], nonce_ok: bool) -> GateResult {
    let items: Vec<String> = deferrals.iter().map(|s| s.to_string()).collect();
    decide(&run_checks(ctx), &items, nonce_ok).result
}

#[test]
fn all_green_context_authorizes() {
    assert_eq!(verdict(&green_ctx(), &[], false), GateResult::Green);
}

#[test]
fn ae1_stranded_order_refuses_then_an_explicit_deferral_proceeds() {
    let mut ctx = green_ctx();
    ctx.stranded_orders = GatewayProbe::Blocked("a resting order remains".into());

    // No deferral -> refused, naming the stranded item.
    let d = decide(&run_checks(&ctx), &[], false);
    assert_eq!(d.result, GateResult::Refused);
    assert!(d.refused_items.iter().any(|i| i == CHECK_STRANDED));

    // Explicit, nonce-authorized deferral of that named item -> proceeds, recorded.
    let d = decide(&run_checks(&ctx), &[CHECK_STRANDED.to_string()], true);
    assert_eq!(d.result, GateResult::Green);
    assert!(d.deferrals.iter().any(|x| x.item == CHECK_STRANDED));
    assert!(d.records.iter().any(|r| r.name == CHECK_STRANDED && r.deferred));
}

#[test]
fn kill_switch_trip_refuses_non_deferrably_even_when_everything_is_deferred() {
    let mut ctx = green_ctx();
    ctx.kill_switch_engaged = true;
    // Defer every check by name, with a valid nonce — the non-deferrable kill switch
    // still refuses.
    let all: Vec<String> = run_checks(&ctx).iter().map(|o| o.name.to_string()).collect();
    let d = decide(&run_checks(&ctx), &all, true);
    assert_eq!(d.result, GateResult::Refused);
    assert!(d.refused_items.iter().any(|i| i == CHECK_KILL_SWITCH));
    assert!(!d.records.iter().any(|r| r.name == CHECK_KILL_SWITCH && r.deferred), "not deferrable");
}

#[test]
fn advisory_lock_held_reds_and_is_deferrable() {
    let mut ctx = green_ctx();
    ctx.live_lock_held = true;
    assert_eq!(verdict(&ctx, &[], false), GateResult::Refused);
    assert_eq!(verdict(&ctx, &[CHECK_ADVISORY_LOCK], true), GateResult::Green);
}

#[test]
fn trading_env_faults_are_non_deferrable() {
    // Missing lane env file -> red, no fallback lane.
    let mut ctx = green_ctx();
    ctx.lane_env_present = false;
    assert_eq!(verdict(&ctx, &[CHECK_TRADING_ENV], true), GateResult::Refused);

    // LS_TRADING_ENV unset -> non-deferrable red.
    let mut ctx = green_ctx();
    ctx.trading_env = None;
    assert_eq!(verdict(&ctx, &[CHECK_TRADING_ENV], true), GateResult::Refused);

    // Resolved-env mismatch (shell paper, resolved live) -> non-deferrable red.
    let mut ctx = green_ctx();
    ctx.resolved_env_is_paper = Some(false);
    assert_eq!(verdict(&ctx, &[CHECK_TRADING_ENV], true), GateResult::Refused);
}

#[test]
fn weekend_and_watermark_traps_red() {
    // Weekend / outside window.
    let mut ctx = green_ctx();
    ctx.window_open = false;
    assert_eq!(verdict(&ctx, &[], false), GateResult::Refused);

    // Stale watermark.
    let mut ctx = green_ctx();
    ctx.watermark_fresh = false;
    assert_eq!(verdict(&ctx, &[], false), GateResult::Refused);

    // Current watermark, empty bar sample (destructive-heal trap).
    let mut ctx = green_ctx();
    ctx.bars_present = false;
    assert_eq!(verdict(&ctx, &[], false), GateResult::Refused);
}

#[test]
fn budget_shortfall_and_unmeasured_are_deferrable_reds() {
    let mut ctx = green_ctx();
    ctx.budget = BudgetHeadroom::Measured { remaining: 1, plan: 10 };
    assert_eq!(verdict(&ctx, &[], false), GateResult::Refused);
    assert_eq!(verdict(&ctx, &[CHECK_BUDGET], true), GateResult::Green);

    let mut ctx = green_ctx();
    ctx.budget = BudgetHeadroom::Unmeasured;
    // Unmeasured is a named red, never a silent green.
    let d = decide(&run_checks(&ctx), &[], false);
    assert_eq!(d.result, GateResult::Refused);
    assert!(d.records.iter().any(|r| r.name == CHECK_BUDGET && r.status == CheckStatus::Red));
    assert_eq!(verdict(&ctx, &[CHECK_BUDGET], true), GateResult::Green);
}

#[test]
fn rung_above_chain_authorization_refuses_on_a_live_lane() {
    let mut ctx = green_ctx();
    ctx.requested_rung = 3; // chain authorizes 1
    assert_eq!(verdict(&ctx, &[], false), GateResult::Refused);

    // On a paper pre-check the rung is informational and never blocks.
    ctx.lane = LanePosture::Paper;
    assert_eq!(verdict(&ctx, &[], false), GateResult::Green);
}

#[test]
fn throttle_short_circuits_to_rerun_never_terminal() {
    let mut ctx = green_ctx();
    ctx.flat_start = GatewayProbe::Throttled;
    let d = decide(&run_checks(&ctx), &[], false);
    assert_eq!(d.result, GateResult::Throttled, "a throttle is a re-run, not a refusal");
    // The flat-start check records Throttled, never a terminal red.
    let rec = d.records.iter().find(|r| r.name == "flat_start").unwrap();
    assert_eq!(rec.status, CheckStatus::Throttled);
}

#[test]
fn deferral_needs_a_valid_nonce() {
    let mut ctx = green_ctx();
    ctx.stranded_orders = GatewayProbe::Blocked("resting order".into());
    // Named, but nonce not authorized -> still refused.
    assert_eq!(verdict(&ctx, &[CHECK_STRANDED], false), GateResult::Refused);
}

#[test]
fn parse_deferrals_splits_and_trims() {
    assert_eq!(parse_deferrals(Some(" stranded_orders , budget_headroom ")),
               vec!["stranded_orders".to_string(), "budget_headroom".to_string()]);
    assert!(parse_deferrals(None).is_empty());
    assert!(parse_deferrals(Some("")).is_empty());
}

#[test]
fn weekday_calendar_seam() {
    use chrono::{TimeZone, Utc};
    let cal = WeekdayKrxCalendar;
    // 2026-07-16 is a Thursday. 10:00 KST = 01:00 UTC -> open; 16:00 KST = 07:00 UTC -> closed.
    assert!(cal.is_trading_session(Utc.with_ymd_and_hms(2026, 7, 16, 1, 0, 0).unwrap()));
    assert!(!cal.is_trading_session(Utc.with_ymd_and_hms(2026, 7, 16, 7, 0, 0).unwrap()));
    // 2026-07-18 is a Saturday -> closed all day.
    assert!(!cal.is_trading_session(Utc.with_ymd_and_hms(2026, 7, 18, 1, 0, 0).unwrap()));
}

// ---------------------------------------------------------------------------
// Gateway-probe integration (wiremock, split execution legs)
// ---------------------------------------------------------------------------

const ACCNO_PATH: &str = "/stock/accno";

fn ok_json(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_string(body.to_string())
        .insert_header("content-type", "application/json")
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

async fn mount_throttle(server: &MockServer, tr_cd: &'static str) {
    Mock::given(method("POST"))
        .and(path(ACCNO_PATH))
        .and(header("tr_cd", tr_cd))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "IGW00201",
            "rsp_msg": "rate limited"
        })))
        .mount(server)
        .await;
}

async fn client(server: &MockServer) -> LsExecClient {
    mount_token(server).await;
    let sdk = LsSdk::new(mock_config(&server.uri())).unwrap();
    LsExecClient::new("LS-KRX", "LS-TRADER-001", "00000000-01", sdk, AccountType::Cash)
}

#[tokio::test]
async fn flat_start_probe_clears_on_zero_balance_round_trip_row() {
    let server = MockServer::start().await;
    // A same-day buy+sell leaves a janqty=0 row — NOT an open holding (the t0424 trap).
    mount_t0424(&server, serde_json::json!([{ "janqty": "0", "hname": "X" }])).await;
    let c = client(&server).await;
    assert!(matches!(probe_flat_start(&c).await, GatewayProbe::Clear));
}

#[tokio::test]
async fn flat_start_probe_blocks_on_open_or_unparseable_balance() {
    let server = MockServer::start().await;
    mount_t0424(&server, serde_json::json!([{ "janqty": "5" }])).await;
    let c = client(&server).await;
    assert!(matches!(probe_flat_start(&c).await, GatewayProbe::Blocked(_)));

    let server = MockServer::start().await;
    mount_t0424(&server, serde_json::json!([{ "janqty": "garbage" }])).await;
    let c = client(&server).await;
    assert!(matches!(probe_flat_start(&c).await, GatewayProbe::Blocked(_)), "unparseable -> fail closed");
}

#[tokio::test]
async fn stranded_probe_blocks_on_truncation_cursor_and_open_row() {
    // A non-empty body cursor -> truncation -> Blocked, never a partial-page clear.
    let server = MockServer::start().await;
    mount_t0425(&server, "MORE_PAGES", serde_json::json!([])).await;
    let c = client(&server).await;
    assert!(matches!(probe_stranded_orders(&c).await, GatewayProbe::Blocked(_)));

    // An open resting order (ordrem > 0) -> Blocked.
    let server = MockServer::start().await;
    mount_t0425(&server, "", serde_json::json!([{ "ordno": "1", "ordrem": "3" }])).await;
    let c = client(&server).await;
    assert!(matches!(probe_stranded_orders(&c).await, GatewayProbe::Blocked(_)));
}

#[tokio::test]
async fn stranded_probe_clears_on_empty_single_page() {
    let server = MockServer::start().await;
    mount_t0425(&server, "", serde_json::json!([])).await;
    let c = client(&server).await;
    assert!(matches!(probe_stranded_orders(&c).await, GatewayProbe::Clear));
}

#[tokio::test]
async fn igw00201_on_a_live_read_is_throttled_not_terminal() {
    let server = MockServer::start().await;
    mount_throttle(&server, "t0424").await;
    let c = client(&server).await;
    assert!(matches!(probe_flat_start(&c).await, GatewayProbe::Throttled), "throttle, not a terminal red");

    // And it maps to a Throttled check status (a re-run), not a terminal red.
    let mut ctx = green_ctx();
    ctx.flat_start = probe_flat_start(&c).await;
    let out = pure_flat_start(&ctx);
    assert_eq!(out.status, CheckStatus::Throttled);
}
