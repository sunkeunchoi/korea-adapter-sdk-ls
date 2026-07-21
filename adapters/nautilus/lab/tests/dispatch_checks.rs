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
    probe_stranded_orders, run_checks, BudgetHeadroom, CalendarDateFact, DispatchContext,
    GatewayProbe, GateResult, LanePosture, TradingCalendar, WeekdayKrxCalendar,
    CHECK_ADVISORY_LOCK, CHECK_BUDGET, CHECK_CALENDAR_DATE, CHECK_KILL_SWITCH, CHECK_STRANDED,
    CHECK_TRADING_ENV,
};
use nautilus_ls_lab::dispatch::readiness::ReadinessVerdict;
use nautilus_ls_lab::dispatch::{CheckStatus, UnknownOverride};
use nautilus_ls_calendar::schema::Citation;
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
        date_fact: CalendarDateFact::TradingSession,
        window_open: true,
        run_id: "run-1".into(),
        unknown_override: None,
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
        readiness: ReadinessVerdict::Green,
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

// (The weekday DATE-fact tests `weekday_calendar_seam` (is_trading_session) and
//  `weekday_seam_splits_date_fact_from_time_window` (date_fact) were retired with the Ladder
//  Enforced-only cutover — the weekday date decision is gone. Only the PRESERVED time-of-day
//  window remains, asserted below (KTD7).)

#[test]
fn weekday_time_window_is_preserved() {
    use chrono::{TimeZone, Utc};
    let cal = WeekdayKrxCalendar;
    // KTD7: the weekday DATE decision is retired, but the PRESERVED 09:00–15:30 KST time
    // window (`in_time_window`) is kept unchanged.
    let thu_open = Utc.with_ymd_and_hms(2026, 7, 16, 1, 0, 0).unwrap(); // 10:00 KST
    let thu_after = Utc.with_ymd_and_hms(2026, 7, 16, 7, 0, 0).unwrap(); // 16:00 KST
    assert!(cal.in_time_window(thu_open), "10:00 KST is inside the window");
    assert!(!cal.in_time_window(thu_after), "16:00 KST is outside the window");
}

// ---------------------------------------------------------------------------
// U12 — Production Ladder date gate + attended Unknown override (KTD8)
// ---------------------------------------------------------------------------

fn citation() -> Citation {
    Citation {
        reference: "KRX-NOTICE-2026-EX-01".into(),
        issuer: "KRX".into(),
        note: Some("synthetic first-party basis".into()),
    }
}

/// A well-formed attended Unknown override bound to `kst_date` + `run_id`.
fn override_for(kst_date: &str, run_id: &str) -> UnknownOverride {
    UnknownOverride {
        kst_date: kst_date.into(),
        run_id: run_id.into(),
        operator: "operator-alice".into(),
        authorized_at_unix: 1_000_000,
        snapshot_artifact_id: "artifact-abc".into(),
        snapshot_calendar_id: "calendar-abc".into(),
        alerts: vec!["alert-witness-vs-closure".into()],
        reason: "reviewed the cited first-party basis for the Unknown date".into(),
        citation: citation(),
    }
}

#[test]
fn u12_failure_inversion_unknown_refuses_but_trading_session_greens() {
    // Unknown date, no override → NO authorized dispatch.
    let mut ctx = green_ctx();
    ctx.date_fact = CalendarDateFact::Unknown;
    assert_eq!(
        verdict(&ctx, &[], false),
        GateResult::Refused,
        "an Unknown calendar date emits no authorized dispatch"
    );

    // Flip ONLY the row to Trading Session (window open + all other gates green) → Green.
    ctx.date_fact = CalendarDateFact::TradingSession;
    assert_eq!(
        verdict(&ctx, &[], false),
        GateResult::Green,
        "the same context with a proven Trading Session authorizes"
    );
}

#[test]
fn u12_closed_refuses_and_no_override_can_green_it() {
    let mut ctx = green_ctx();
    ctx.date_fact = CalendarDateFact::Closed;
    assert_eq!(verdict(&ctx, &[], false), GateResult::Refused);
    // The blunt deferral surface cannot proceed a proven closure.
    assert_eq!(verdict(&ctx, &[CHECK_CALENDAR_DATE], true), GateResult::Refused);
    // Even a well-formed override bound to this exact date + run cannot proceed a closure.
    let (d, r) = (ctx.today_kst.clone(), ctx.run_id.clone());
    ctx.unknown_override = Some(override_for(&d, &r));
    assert_eq!(
        verdict(&ctx, &[], false),
        GateResult::Refused,
        "the override cannot proceed a proven Closed date"
    );
}

#[test]
fn u12_unavailable_refuses_without_override() {
    let mut ctx = green_ctx();
    ctx.date_fact = CalendarDateFact::Unavailable;
    assert_eq!(verdict(&ctx, &[], false), GateResult::Refused);
    assert_eq!(verdict(&ctx, &[CHECK_CALENDAR_DATE], true), GateResult::Refused);
    let (d, r) = (ctx.today_kst.clone(), ctx.run_id.clone());
    ctx.unknown_override = Some(override_for(&d, &r));
    assert_eq!(
        verdict(&ctx, &[], false),
        GateResult::Refused,
        "a calendar load/use/query failure refuses without override"
    );
}

#[test]
fn u12_unknown_override_binds_to_exact_date_and_run() {
    let mut ctx = green_ctx(); // today_kst = 2026-07-16, run_id = run-1
    ctx.date_fact = CalendarDateFact::Unknown;

    // Exact date + run → proceeds to Green.
    ctx.unknown_override = Some(override_for("2026-07-16", "run-1"));
    assert_eq!(
        verdict(&ctx, &[], false),
        GateResult::Green,
        "a bound, well-formed override proceeds the Unknown date"
    );

    // A different KST date is NOT covered.
    ctx.unknown_override = Some(override_for("2026-07-17", "run-1"));
    assert_eq!(verdict(&ctx, &[], false), GateResult::Refused, "a different date is not covered");

    // A different run is NOT covered.
    ctx.unknown_override = Some(override_for("2026-07-16", "run-2"));
    assert_eq!(verdict(&ctx, &[], false), GateResult::Refused, "a different run is not covered");
}

#[test]
fn u12_override_requires_all_audit_fields_including_structured_citation() {
    let mut ctx = green_ctx();
    ctx.date_fact = CalendarDateFact::Unknown;
    let base = override_for("2026-07-16", "run-1");

    // A blank structured-citation reference is not authorizing (no unverifiable basis).
    let mut ov = base.clone();
    ov.citation.reference = String::new();
    ctx.unknown_override = Some(ov);
    assert_eq!(verdict(&ctx, &[], false), GateResult::Refused, "blank citation reference refuses");

    // A blank citation issuer is not authorizing.
    let mut ov = base.clone();
    ov.citation.issuer = String::new();
    ctx.unknown_override = Some(ov);
    assert_eq!(verdict(&ctx, &[], false), GateResult::Refused, "blank citation issuer refuses");

    // A blank operator is not authorizing.
    let mut ov = base.clone();
    ov.operator = String::new();
    ctx.unknown_override = Some(ov);
    assert_eq!(verdict(&ctx, &[], false), GateResult::Refused, "blank operator refuses");

    // A blank reason is not authorizing.
    let mut ov = base.clone();
    ov.reason = "   ".into();
    ctx.unknown_override = Some(ov);
    assert_eq!(verdict(&ctx, &[], false), GateResult::Refused, "blank reason refuses");

    // Fully-formed → proceeds.
    ctx.unknown_override = Some(base);
    assert_eq!(verdict(&ctx, &[], false), GateResult::Green, "a complete override proceeds");
}

#[test]
fn u12_time_window_preserved_for_a_proven_session_and_an_overridden_unknown() {
    // A proven Trading Session OUTSIDE 09:00–15:30 KST still defers on the time window.
    let mut ctx = green_ctx();
    ctx.date_fact = CalendarDateFact::TradingSession;
    ctx.window_open = false;
    assert_eq!(verdict(&ctx, &[], false), GateResult::Refused, "outside the time window still refuses");

    // An overridden Unknown ALSO cannot escape the time-window check.
    ctx.date_fact = CalendarDateFact::Unknown;
    ctx.unknown_override = Some(override_for("2026-07-16", "run-1"));
    ctx.window_open = false;
    assert_eq!(
        verdict(&ctx, &[], false),
        GateResult::Refused,
        "the override cannot proceed a closed time window"
    );
}

#[test]
fn u12_calendar_date_check_is_non_deferrable_for_every_non_session_fact() {
    for fact in [
        CalendarDateFact::Closed,
        CalendarDateFact::Unknown,
        CalendarDateFact::Unavailable,
    ] {
        let mut ctx = green_ctx();
        ctx.date_fact = fact;
        assert_eq!(
            verdict(&ctx, &[CHECK_CALENDAR_DATE], true),
            GateResult::Refused,
            "calendar_date is non-deferrable for {fact:?}"
        );
    }
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
