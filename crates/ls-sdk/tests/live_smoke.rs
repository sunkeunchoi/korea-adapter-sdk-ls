//! Paper Live Smoke — `#[ignore]` integration tests that hit the REAL LS paper
//! gateway with real credentials read from the environment.
//!
//! These are excluded from the default `cargo test` run by `#[ignore]`; the repo
//! Makefile loads the gitignored `.env` and invokes them explicitly
//! (`make live-smoke`, `make live-smoke-chart`, `make live-smoke-account`,
//! `make live-smoke-ws`).
//!
//! Safety: every smoke target calls [`paper_guard`] first. It requires
//! `LS_TRADING_ENV` to be set *explicitly* to `paper` and refuses otherwise.
//! LS exposes no server-side paper/real signal and serves both from one REST
//! host, so this client-side gate is the only structural protection against
//! hitting production. See `docs/plans/2026-06-15-002-feat-paper-live-smoke-plan.md`.

use std::time::Duration;

use chrono::{Datelike, FixedOffset, NaiveDate, Utc, Weekday};
use futures::StreamExt;
use ls_core::{LsConfig, LsError, LsResult};
use ls_sdk::account::CSPAQ12200Request;
use ls_sdk::market_session::{
    T1101Request, T1102Request, T1531Request, T1537Request, T8425Request, T8436Request,
};
use ls_sdk::paginated::{
    T1403Request, T1441Request, T1452Request, T1463Request, T1466Request, T1489Request,
    T1492Request, T8412Request,
};
use ls_sdk::realtime::S3Trade;
use ls_sdk::LsSdk;
use tokio::time::timeout;

/// Default market-data symbol when `LS_LIVE_SMOKE_SHCODE` is unset
/// (Samsung Electronics, a liquid KOSPI symbol).
const DEFAULT_SHCODE: &str = "005930";

/// Resolve the smoke symbol: `LS_LIVE_SMOKE_SHCODE` override, else [`DEFAULT_SHCODE`].
fn resolve_symbol() -> String {
    std::env::var("LS_LIVE_SMOKE_SHCODE").unwrap_or_else(|_| DEFAULT_SHCODE.to_string())
}

/// Pre-flight production guard — requires `LS_TRADING_ENV` to be *explicitly* `paper`.
///
/// Reads the raw env var rather than the resolved [`ls_core::Environment`] so an
/// unset or misspelled value fails instead of silently defaulting to paper. Runs
/// before any SDK construction or network I/O.
fn paper_guard() -> LsResult<()> {
    match std::env::var("LS_TRADING_ENV") {
        Ok(v) if v.eq_ignore_ascii_case("paper") => Ok(()),
        Ok(v) => Err(LsError::Config(format!(
            "live smoke refuses to run: LS_TRADING_ENV must be explicitly 'paper', got '{v}'"
        ))),
        Err(_) => Err(LsError::Config(
            "live smoke refuses to run: LS_TRADING_ENV must be explicitly set to 'paper' \
             (unset is not allowed)"
                .into(),
        )),
    }
}

/// Build a real, gateway-pointed SDK after the paper guard passes.
///
/// `from_env` reads ordinary env vars (no dotenv, no I/O); `LsSdk::new` validates
/// credentials and URL schemes but performs no network call. No `base_url`
/// override, so dispatch reaches the live paper gateway. Missing credentials
/// surface here as an `Err` (an explicit test failure), never a silent skip.
fn paper_sdk() -> LsResult<LsSdk> {
    paper_guard()?;
    let config = LsConfig::from_env()?;
    // Defense in depth: the resolved environment must also be Paper.
    if !config.environment.is_paper() {
        return Err(LsError::Config(
            "resolved environment is not Paper after the guard passed — refusing".into(),
        ));
    }
    LsSdk::new(config)
}

/// Print a structured, credential-free evidence line so a green run records its
/// target, inputs, and result (a smoke is not Focused Evidence until recorded).
///
/// `inputs` and `result` MUST NOT carry the OAuth token, appkey, secret, or
/// account number — only symbols, dates, environment, business codes, and lengths.
fn record(target: &str, inputs: &str, result: &str) {
    println!("LIVE-SMOKE target={target} inputs=[{inputs}] result=[{result}]");
}

/// Guard logic, exercised without the network (non-ignored, runs in `cargo test`).
///
/// Covers AE1 and the unset/misspelled cases: only an explicit `paper` passes.
#[test]
fn paper_guard_requires_explicit_paper() {
    let saved = std::env::var("LS_TRADING_ENV").ok();

    std::env::set_var("LS_TRADING_ENV", "paper");
    assert!(paper_guard().is_ok(), "explicit paper must pass");
    std::env::set_var("LS_TRADING_ENV", "Paper");
    assert!(paper_guard().is_ok(), "case-insensitive paper must pass");

    std::env::set_var("LS_TRADING_ENV", "real");
    assert!(paper_guard().is_err(), "real must be refused");
    std::env::set_var("LS_TRADING_ENV", "papr");
    assert!(paper_guard().is_err(), "a typo must be refused");
    std::env::remove_var("LS_TRADING_ENV");
    assert!(paper_guard().is_err(), "unset must be refused");

    match saved {
        Some(v) => std::env::set_var("LS_TRADING_ENV", v),
        None => std::env::remove_var("LS_TRADING_ENV"),
    }
}

/// Default `make live-smoke`: paper guard → OAuth token → one `t1102` quote.
///
/// Covers AE2. A non-empty token proves a live round-trip (liveness signal);
/// `quote` returning `Ok` proves market-data transport (a `01900` would `Err`).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke`"]
async fn live_smoke_default() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let symbol = resolve_symbol();

    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(
        !token.is_empty(),
        "token must be non-empty — proves a live round-trip"
    );

    let req = T1102Request::new(&symbol, "K");
    let resp = sdk
        .market_session()
        .quote(&req)
        .await
        .expect("t1102 quote failed");

    // The recorded line is the Focused Evidence for `token` (see
    // metadata/evidence/token.yaml). It is credential-free and dated *by
    // construction*: `rsp_msg` is dropped (it carries localized,
    // account-identifying text), only the numeric `rsp_cd` proves success, and
    // the run stamps its own UTC date so a verbatim capture cannot reintroduce a
    // secret or a hand-typed date.
    let date = Utc::now().format("%Y-%m-%d");
    record(
        "live-smoke",
        &format!("env=paper symbol={symbol} date={date}"),
        &format!(
            "token_len={} rsp_cd={} price={}",
            token.len(),
            resp.rsp_cd,
            resp.outblock.price
        ),
    );
}

/// `make live-smoke-book`: paper guard → OAuth token → one `t1101` order-book
/// quote. The recorded line is the Focused Evidence candidate for `t1101`
/// (`metadata/evidence/t1101.yaml` on a green run) — credential-free and
/// self-dated by construction. `order_book` returning `Ok` proves market-data
/// transport; a `01900` would `Err` here and drive the AE2 paper-incompatible
/// reclassification (`paper_incompatible: true`, stay `implemented`).
///
/// `symbol` is a public market ticker (Samsung by default); any
/// `LS_LIVE_SMOKE_SHCODE` override must also be a public ticker, never an
/// account number or internal identifier.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-book`"]
async fn live_smoke_book() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let symbol = resolve_symbol();

    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(
        !token.is_empty(),
        "token must be non-empty — proves a live round-trip"
    );

    let req = T1101Request::new(&symbol);
    let resp = sdk
        .market_session()
        .order_book(&req)
        .await
        .expect("t1101 order_book failed");

    let date = Utc::now().format("%Y-%m-%d");
    record(
        "live-smoke-book",
        &format!("env=paper symbol={symbol} date={date}"),
        &format!(
            "token_len={} rsp_cd={} price={} offerho1={} bidho1={}",
            token.len(),
            resp.rsp_cd,
            resp.outblock.price,
            resp.outblock.offerho1,
            resp.outblock.bidho1
        ),
    );
}

// ---------------------------------------------------------------------------
// t8425 — 전체테마 (all-themes) smoke. First implement-tr pilot: market_session,
// non-paginated, no caller input, reliably non-empty.
// ---------------------------------------------------------------------------

/// Map a smoke outcome to the optional credential-free `LIVE-SMOKE` result
/// fragment. `Ok((rsp_cd, count))` → `Some(line)`; any `Err` → `None`.
///
/// This is the offline-testable seam for the R3a Err-path guarantee, shared by
/// every `implement-tr` market_session smoke: on failure it yields `None`, so the
/// smoke fn never calls [`record`] and no `LIVE-SMOKE` line can be captured from a
/// failed run. The fragment carries only the business `rsp_cd` and a public
/// structural count under `count_label` — never `rsp_msg`.
fn smoke_result(outcome: Result<(String, usize), &LsError>, count_label: &str) -> Option<String> {
    match outcome {
        Ok((rsp_cd, count)) => Some(format!("rsp_cd={rsp_cd} {count_label}={count}")),
        Err(_) => None,
    }
}

/// Err-path safety + line shape, exercised without the network (non-ignored).
///
/// Covers R3a: a simulated gateway error yields no `LIVE-SMOKE` line, while the
/// success path yields a credential-free fragment.
#[test]
fn smoke_result_err_path_emits_no_live_smoke_line() {
    let err = LsError::Config("simulated gateway error".into());
    assert!(
        smoke_result(Err(&err), "themes").is_none(),
        "an Err outcome must not build a LIVE-SMOKE line"
    );
    let line = smoke_result(Ok(("00000".into(), 42)), "themes").expect("Ok yields a line");
    assert_eq!(line, "rsp_cd=00000 themes=42");
    assert!(!line.contains("rsp_msg"), "result fragment must not carry rsp_msg");
}

/// `make live-smoke-t8425`: paper guard → OAuth token → one `t8425` all-themes
/// read. The pilot for the `tracked → implemented` recipe.
///
/// `all_themes` returning `Ok` with a non-empty `outblock` proves the read is
/// callable and the response shape round-trips. The recorded line is
/// credential-free by construction (only `rsp_cd` + a public theme count; no
/// `rsp_msg`, token, or account text) and self-dated. A failed run emits a
/// distinct `SMOKE-FAIL` stderr line — never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8425`"]
async fn live_smoke_t8425() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(
        !token.is_empty(),
        "token must be non-empty — proves a live round-trip"
    );

    let req = T8425Request::new();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().all_themes(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "themes")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8425",
                &format!("env=paper date={date}"),
                &line,
            );
        }
        Err(e) => {
            // No capturable LIVE-SMOKE line on failure (R3a): the Err arm never
            // calls record(); the smoke_result(Err) -> None contract is proven by
            // the offline test `smoke_result_err_path_emits_no_live_smoke_line`.
            eprintln!("SMOKE-FAIL target=live-smoke-t8425 market-data failure (not evidence)");
            panic!("live-smoke-t8425 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t8436 — 주식종목조회 (stock master list). market_session, non-paginated; takes
// a `gubun` market-segment filter (not an instrument identifier).
// ---------------------------------------------------------------------------

/// `make live-smoke-t8436`: paper guard → OAuth token → one `t8436` stock-list
/// read for `gubun="0"` (전체/all segments).
///
/// `stock_list` returning `Ok` with a non-empty `outblock` proves the read is
/// callable and the row shape round-trips. The recorded line is credential-free
/// (only `rsp_cd` + a public row count) and self-dated; a failed run emits a
/// distinct `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8436`"]
async fn live_smoke_t8436() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8436Request::new("0");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().stock_list(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "stocks")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8436",
                &format!("env=paper gubun=0 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8436 market-data failure (not evidence)");
            panic!("live-smoke-t8436 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t1531 / t1537 — theme-keyed reads. market_session, non-paginated. Each smoke
// self-sources a representative theme from t8425 (the plan's "one-off t8425 call"
// input source), so it needs no hardcoded theme code.
// ---------------------------------------------------------------------------

/// `make live-smoke-t1531`: paper guard → token → fetch one theme via `t8425` →
/// one `t1531` theme-constituents read for that theme.
///
/// `tmcode` is public theme reference data (printed); `tmname` is not printed.
/// Credential-free, self-dated; failure emits SMOKE-FAIL, never a LIVE-SMOKE line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1531`"]
async fn live_smoke_t1531() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let themes = sdk
        .market_session()
        .all_themes(&T8425Request::new())
        .await
        .expect("t8425 all_themes (theme input source) failed");
    // all_themes returns Ok with an empty out-block on a 00707 (success-but-empty);
    // surface that as a credential-safe SMOKE-FAIL with the rsp_cd, not an opaque
    // .expect() panic, so an off-session empty is distinguishable from a defect.
    if themes.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t1531 t8425 theme source empty (rsp_cd={})",
            themes.rsp_cd
        );
        panic!("live-smoke-t1531: no theme to key the read");
    }
    let theme = &themes.outblock[0];
    let (tmname, tmcode) = (theme.tmname.clone(), theme.tmcode.clone());

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .theme_stocks(&T1531Request::new(&tmname, &tmcode))
        .await
    {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t1531",
                &format!("env=paper tmcode={tmcode} date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1531 market-data failure (not evidence)");
            panic!("live-smoke-t1531 failed: {e}");
        }
    }
}

/// `make live-smoke-t1537`: paper guard → token → fetch one theme via `t8425` →
/// one `t1537` per-stock-quotes read for that theme code.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1537`"]
async fn live_smoke_t1537() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let themes = sdk
        .market_session()
        .all_themes(&T8425Request::new())
        .await
        .expect("t8425 all_themes (theme input source) failed");
    if themes.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t1537 t8425 theme source empty (rsp_cd={})",
            themes.rsp_cd
        );
        panic!("live-smoke-t1537: no theme to key the read");
    }
    let tmcode = themes.outblock[0].tmcode.clone();

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .theme_quotes(&T1537Request::new(&tmcode))
        .await
    {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t1537",
                &format!("env=paper tmcode={tmcode} date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1537 market-data failure (not evidence)");
            panic!("live-smoke-t1537 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// U4 — chart smoke + offline date validation
// ---------------------------------------------------------------------------

/// Validate the chart date offline before any network I/O: `YYYYMMDD` format, a
/// weekday, and not after "today in KST". Holiday / real-trading-day correctness
/// is the gateway's verdict (`01715`), not a shipped KRX calendar.
///
/// KST is UTC+9 with no DST, so a fixed offset is exact — the base `chrono`
/// dependency has no IANA timezone database and `chrono-tz` is not added.
fn validate_t8412_date(raw: &str) -> LsResult<NaiveDate> {
    if raw.len() != 8 || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(LsError::Config(format!(
            "LS_LIVE_SMOKE_T8412_DATE must be YYYYMMDD, got '{raw}'"
        )));
    }
    let date = NaiveDate::parse_from_str(raw, "%Y%m%d")
        .map_err(|e| LsError::Config(format!("LS_LIVE_SMOKE_T8412_DATE '{raw}' invalid: {e}")))?;
    if matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
        return Err(LsError::Config(format!(
            "LS_LIVE_SMOKE_T8412_DATE {raw} is a weekend — supply a trading day"
        )));
    }
    let kst = FixedOffset::east_opt(9 * 3600).expect("KST offset is valid");
    let today_kst = Utc::now().with_timezone(&kst).date_naive();
    if date > today_kst {
        return Err(LsError::Config(format!(
            "LS_LIVE_SMOKE_T8412_DATE {raw} is in the future (KST today is {today_kst})"
        )));
    }
    Ok(date)
}

/// Offline date-validation logic, exercised without the network (non-ignored).
///
/// Covers AE3-adjacent format checks, AE4 (malformed / weekend), and the
/// future-date bound.
#[test]
fn chart_date_validation_offline() {
    // Malformed: wrong shape, non-digits, impossible calendar day.
    assert!(validate_t8412_date("2026-06-12").is_err());
    assert!(validate_t8412_date("abcdefgh").is_err());
    assert!(validate_t8412_date("20260631").is_err()); // June has 30 days
                                                       // Weekend: 2026-06-13 is a Saturday.
    assert!(validate_t8412_date("20260613").is_err());
    // Future: far-future date is rejected regardless of "today".
    assert!(validate_t8412_date("29991231").is_err());
    // Valid past weekday: 2024-01-02 was a Tuesday.
    assert!(validate_t8412_date("20240102").is_ok());
}

/// `make live-smoke-chart`: paper guard → offline date validation → one `t8412`
/// page (never `chart_all`). Covers AE3 (missing date), AE5 (gateway holiday).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials + a trading day; run via `make live-smoke-chart`"]
async fn live_smoke_chart() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let symbol = resolve_symbol();

    let raw_date = std::env::var("LS_LIVE_SMOKE_T8412_DATE")
        .expect("LS_LIVE_SMOKE_T8412_DATE is required for the chart smoke (no default)");
    let date = validate_t8412_date(&raw_date).expect("chart date failed offline validation");
    let d = date.format("%Y%m%d").to_string();

    // One page only: ncnt=1 (1-minute), qrycnt=20 rows, nday=1, comp_yn=N.
    let req = T8412Request::new(&symbol, "1", "20", "1", &d, &d, "N");
    let resp = sdk
        .paginated()
        .chart_page(&req)
        .await
        .expect("t8412 chart_page failed (a gateway 01715 means a non-trading day)");

    // Credential-free by construction: `rsp_msg` is dropped (it can carry
    // localized, account-identifying text and is excluded from the
    // token/t1101 evidence pattern); only the numeric `rsp_cd` proves success
    // and `rows` is a public structural count. Mirrors `live_smoke_default`.
    record(
        "live-smoke-chart",
        &format!("symbol={symbol} date={d}"),
        &format!("rsp_cd={} rows={}", resp.rsp_cd, resp.outblock1.len()),
    );
}

// ---------------------------------------------------------------------------
// t1452 — 거래량상위 (top trading volume). First single-page body-`idx` paginated
// TR (the implement-tr second-freeze sub-pattern). Intraday rank screen: on a
// non-trading day the gateway returns an empty success (00707) → PENDING.
// ---------------------------------------------------------------------------

/// `make live-smoke-t1452`: paper guard → token → one single-page `t1452`
/// top-volume read (all-segment, permissive filters, first-page `idx`).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1452`"]
async fn live_smoke_t1452() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    // All-segment, no price/volume/change-rate filter, first page.
    let req = T1452Request::new("0", "0", "0", "0", "0", "0", "0", "0");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().top_volume(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t1452",
                &format!("env=paper gubun=0 idx=0 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1452 market-data failure (not evidence)");
            panic!("live-smoke-t1452 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t1403 / t1441 / t1463 / t1466 / t1489 / t1492 — the remaining single-page
// body-`idx` paginated rank/screen TRs. Same sub-pattern as t1452. Intraday
// rank screens may return an empty success (00707) outside a session → PENDING.
// ---------------------------------------------------------------------------

/// `make live-smoke-t1403`: single-page `t1403` newly-listed stocks over a wide
/// listing-month range (a historical range query, non-empty off-session).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1403`"]
async fn live_smoke_t1403() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    // Wide listing-month range to maximize hits regardless of session.
    let req = T1403Request::new("0", "202401", "202612");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().new_listings(&req).await {
        Ok(resp) => record(
            "live-smoke-t1403",
            &format!("env=paper range=202401-202612 idx=0 date={date}"),
            &smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line"),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1403 market-data failure (not evidence)");
            panic!("live-smoke-t1403 failed: {e}");
        }
    }
}

/// `make live-smoke-t1441`: single-page `t1441` top change-rate (up, today, KRX).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1441`"]
async fn live_smoke_t1441() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1441Request::new("0", "1", "1", "0", "0", "0", "0", "0", "1");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().top_change_rate(&req).await {
        Ok(resp) => record(
            "live-smoke-t1441",
            &format!("env=paper idx=0 date={date}"),
            &smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line"),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1441 market-data failure (not evidence)");
            panic!("live-smoke-t1441 failed: {e}");
        }
    }
}

/// `make live-smoke-t1463`: single-page `t1463` top trading value (KRX).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1463`"]
async fn live_smoke_t1463() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1463Request::new("0", "0", "0", "0", "0", "0", "0", "1");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().top_value(&req).await {
        Ok(resp) => record(
            "live-smoke-t1463",
            &format!("env=paper idx=0 date={date}"),
            &smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line"),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1463 market-data failure (not evidence)");
            panic!("live-smoke-t1463 failed: {e}");
        }
    }
}

/// `make live-smoke-t1466`: single-page `t1466` volume-surge screen (KRX).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1466`"]
async fn live_smoke_t1466() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1466Request::new("0", "1", "1", "0", "0", "0", "0", "0", "1");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().volume_surge(&req).await {
        Ok(resp) => record(
            "live-smoke-t1466",
            &format!("env=paper idx=0 date={date}"),
            &smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line"),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1466 market-data failure (not evidence)");
            panic!("live-smoke-t1466 failed: {e}");
        }
    }
}

/// `make live-smoke-t1489`: single-page `t1489` top expected-execution volume.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1489`"]
async fn live_smoke_t1489() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1489Request::new("0", "0", "000000000000", "0", "0", "0");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().top_expected_volume(&req).await {
        Ok(resp) => record(
            "live-smoke-t1489",
            &format!("env=paper idx=0 date={date}"),
            &smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line"),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1489 market-data failure (not evidence)");
            panic!("live-smoke-t1489 failed: {e}");
        }
    }
}

/// `make live-smoke-t1492`: single-page `t1492` single-price expected change rate.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1492`"]
async fn live_smoke_t1492() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1492Request::new("0", "1", "0", "0");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().single_price_expected(&req).await {
        Ok(resp) => record(
            "live-smoke-t1492",
            &format!("env=paper idx=0 date={date}"),
            &smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line"),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1492 market-data failure (not evidence)");
            panic!("live-smoke-t1492 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// U5 — account smoke (CSPAQ12200, read-only)
// ---------------------------------------------------------------------------

/// `make live-smoke-account`: paper guard → read-only `CSPAQ12200` balance.
///
/// Covers AE7. The account number comes from config, never the caller. An
/// account-state failure is recorded under the account label, distinct from
/// market-data / transport failures (it may reflect paper-account provisioning).
#[tokio::test]
#[ignore = "live smoke: needs a provisioned LS paper account; run via `make live-smoke-account`"]
async fn live_smoke_account() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    match sdk.account().balance(&CSPAQ12200Request::new("1")).await {
        // Credential-free by construction: `rsp_msg` is dropped (it can carry
        // localized, account-identifying text); only the numeric `rsp_cd`
        // proves success and `reccnt` is a structural record count. Mirrors
        // `live_smoke_default`.
        Ok(resp) => record(
            "live-smoke-account",
            "balcretp=1",
            &format!("rsp_cd={} reccnt={}", resp.rsp_cd, resp.outblock1.reccnt),
        ),
        // A failed run must NOT emit a capturable `LIVE-SMOKE` line: the raw
        // gateway error can carry account-identifying text and would otherwise
        // pattern-match the evidence-capture recipe. Use a distinct
        // non-`LIVE-SMOKE` prefix on stderr; the panic is unchanged.
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-account account-state failure (not transport)");
            panic!("live-smoke-account failed (account-state, may be paper-account setup): {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// U6 — WebSocket smoke (S3_ lifecycle)
// ---------------------------------------------------------------------------

/// `make live-smoke-ws`: paper guard → assert the paper WS port → connect /
/// subscribe `S3_` / unsubscribe. Covers AE6.
///
/// The connect → subscribe → unsubscribe lifecycle is the blocking assertion;
/// receiving a row is extra evidence, and its absence within the timeout is not
/// a failure. Asserting the resolved WS URL carries the paper port `29443` turns
/// a silent wrong-target run (WS ports differ by environment) into a failure.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-ws`"]
async fn live_smoke_ws() {
    paper_guard().expect("paper guard must pass for a paper run");
    let config = LsConfig::from_env().expect("config from env");
    assert!(
        config.environment.is_paper(),
        "resolved environment must be Paper"
    );

    let ws_url = ls_core::config::Environment::resolve_ws_url(&config);
    assert!(
        ws_url.contains("29443"),
        "expected the paper WS port 29443, got {ws_url}"
    );

    let symbol = resolve_symbol();
    let sdk = LsSdk::new(config).expect("sdk construction");
    let ws = sdk.realtime();

    let (handle, mut stream) = ws
        .subscribe_typed::<S3Trade>("S3_", &symbol)
        .await
        .expect("subscribe_typed S3_ failed (connect/subscribe lifecycle)");

    // Extra evidence: a row may or may not arrive inside the timebox.
    let row_note = match timeout(Duration::from_secs(5), stream.next()).await {
        Ok(Some(Ok(row))) => format!("row received price={}", row.price),
        Ok(Some(Err(e))) => format!("frame decode error: {e}"),
        Ok(None) => "stream ended without a row".to_string(),
        Err(_) => "no row within timeout (not a failure)".to_string(),
    };

    handle
        .unsubscribe()
        .await
        .expect("unsubscribe must complete cleanly");

    record(
        "live-smoke-ws",
        &format!("symbol={symbol} ws_port=29443"),
        &row_note,
    );
}

// ---------------------------------------------------------------------------
// Failure classifier — credential-safe raw-HTTP probe (implement-tr R6)
// ---------------------------------------------------------------------------

/// `make raw-probe`: classify a smoke failure as environmental vs TR-defect.
///
/// Acquires the OAuth token through the SDK (never a hand-built auth header),
/// then issues ONE bare `reqwest` POST mirroring `dispatch_once`'s headers —
/// deliberately BYPASSING the SDK's typed deserialize. If this raw POST returns
/// a business `rsp_cd` but the typed smoke failed, the failure is a TR defect
/// (struct shape); if the raw POST also fails, the failure is environmental.
///
/// Driven by env so it works for any TR without a per-TR test:
/// - `LS_PROBE_TR_CD` — the `tr_cd` header (e.g. `t8425`)
/// - `LS_PROBE_PATH`  — the REST path (e.g. `/stock/sector`)
/// - `LS_PROBE_BODY`  — the raw JSON request body
///
/// The recorded line uses a distinct `RAW-PROBE` prefix — never `LIVE-SMOKE` —
/// so the classifier output can never be mistaken for Focused Evidence. It is
/// credential-free by construction: only the HTTP status, the business `rsp_cd`,
/// and body lengths are printed — never the token, `rsp_msg`, or body content.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials + LS_PROBE_* env; run via `make raw-probe`"]
async fn raw_http_probe() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let config = LsConfig::from_env().expect("config from env");

    let tr_cd = std::env::var("LS_PROBE_TR_CD").expect("LS_PROBE_TR_CD is required for the probe");
    let path = std::env::var("LS_PROBE_PATH").expect("LS_PROBE_PATH is required for the probe");
    let body = std::env::var("LS_PROBE_BODY").expect("LS_PROBE_BODY is required for the probe");

    // Token via the SDK's real OAuth path — not a hand-built auth header (which
    // would risk the credential leaks R3a guards).
    let token = match sdk.standalone().token().await {
        Ok(t) if !t.is_empty() => t,
        _ => {
            eprintln!("SMOKE-FAIL target=raw-probe token acquisition failed (not evidence)");
            panic!("raw-probe could not acquire an OAuth token");
        }
    };

    let url = format!(
        "{}{}",
        ls_core::config::Environment::resolve_base_url(&config),
        path
    );
    // Bound the probe like the SDK client (Inner uses 10s connect / 30s request)
    // so a slow/unreachable gateway can't hang `make raw-probe` indefinitely.
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .expect("probe client builds");
    let result = client
        .post(url)
        .bearer_auth(&token)
        .header("tr_cd", &tr_cd)
        .header("tr_cont", "N")
        .header("tr_cont_key", "")
        .header("content-type", "application/json; charset=utf-8")
        .body(body.clone())
        .send()
        .await;

    match result {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let text = resp.text().await.unwrap_or_default();
            // Parse ONLY rsp_cd; never surface rsp_msg or the raw body content.
            let rsp_cd = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.get("rsp_cd").and_then(|c| c.as_str()).map(String::from))
                .unwrap_or_default();
            println!(
                "RAW-PROBE target=raw-probe inputs=[tr_cd={tr_cd} path={path} body_len={}] \
                 result=[http={status} rsp_cd={rsp_cd} body_len={}]",
                body.len(),
                text.len()
            );
        }
        // A transport failure here is the environmental signal. Emit no
        // capturable evidence line; the distinct stderr prefix mirrors
        // `live_smoke_account`'s `SMOKE-FAIL`.
        Err(_) => {
            eprintln!(
                "SMOKE-FAIL target=raw-probe transport failure (environmental, not evidence)"
            );
            panic!("raw-probe transport failed — classify as environmental");
        }
    }
}
