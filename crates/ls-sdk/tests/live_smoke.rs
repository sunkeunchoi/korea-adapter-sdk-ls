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
use ls_sdk::account::{
    CCENQ10100Request, CCENQ90200Request, CFOAQ10100Request, CFOBQ10500Request, CSPAQ12200Request,
    CSPAQ12300Request, CSPAQ22200Request,
};
use ls_sdk::market_session::{
    T1101Request, T1102Request, T1485Request, T1511Request, T1516Request, T1531Request,
    T1537Request, T1601Request, T1615Request, T1640Request, T1662Request, T1664Request,
    T1825Request, T1826Request, T1859Request, T1958Request, T1964Request, T2301Request,
    T2522Request, T8401Request, T8424Request, T8425Request, T8426Request, T8433Request,
    T8435Request, T8467Request, T9943Request, T9944Request,
    T8431Request,
    T8436Request,
    T9905Request, T9907Request, T9942Request,
    T2106Request, T2111Request, T2112Request, T8402Request, T8403Request, T8434Request,
    T1988Request, T3320Request,
    T8455Request, T8460Request, T8463Request,
    G3101Request, G3102Request, G3103Request, G3104Request, G3106Request, G3190Request,
    O3101Request, O3105Request, O3106Request, O3121Request, O3125Request, O3126Request,
};
use ls_sdk::paginated::{
    T1403Request, T1441Request, T1452Request, T1463Request, T1466Request, T1481Request,
    T1482Request, T1489Request, T1492Request, T1514Request, T1866Request, T3341Request,
    T8412Request,
};
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
///
/// NOT trading-day-gated: despite `facets.date_sensitive: true`, `t1403`'s inputs
/// are listing MONTHS (`styymm`/`enyymm`, `YYYYMM`), not a trading DAY, so the
/// `01715` non-trading-day error structurally cannot apply — verified live across
/// weekday/weekend/future ranges (it never returns `01715`). Unlike `t8412`, this
/// smoke needs no weekday pin and no `01715` prior-weekday retry. A wide range is
/// used so past listings keep it non-empty regardless of when it runs; a TR-level
/// `IGW00201` gateway error is transient throttling (clears on retry / spacing),
/// classified environmental by the R6 probe, never a TR defect.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1403`"]
async fn live_smoke_t1403() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    // Wide listing-month range so past listings keep it non-empty regardless of
    // when it runs (no trading-day/01715 concept applies — see fn doc).
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

/// `make live-smoke-t1481`: single-page `t1481` after-hours top change-rate
/// (시간외등락율상위; all-segment, up, permissive filters, first-page `idx`).
///
/// `after_hours_top_change_rate` returning `Ok` with a non-empty `outblock1`
/// proves the read is callable and the raw-capture row shape round-trips. The
/// recorded line carries only `rsp_cd` + a public row count (no `rsp_msg`, token,
/// or account text) and is self-dated; a failed run emits a distinct `SMOKE-FAIL`
/// stderr line, never a capturable `LIVE-SMOKE` line. An empty success (`00707`)
/// outside an after-hours session is the PENDING case, not a defect.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1481`"]
async fn live_smoke_t1481() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    // All-segment, up, no min-volume filter, first page.
    let req = T1481Request::new("0", "1", "0", "0");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().after_hours_top_change_rate(&req).await {
        Ok(resp) if resp.outblock1.is_empty() => {
            // Empty success (`00707`) outside an after-hours session is the PENDING
            // case, not Implemented evidence — emit no capturable LIVE-SMOKE line
            // (mirrors live_smoke_t1866's non-empty guard).
            eprintln!("SMOKE-FAIL target=live-smoke-t1481 empty result (00707); PENDING not evidence");
            panic!("live-smoke-t1481: empty result (00707) — PENDING, not Implemented");
        }
        Ok(resp) => record(
            "live-smoke-t1481",
            &format!("env=paper gubun1=0 idx=0 date={date}"),
            &smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line"),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1481 market-data failure (not evidence)");
            panic!("live-smoke-t1481 failed: {e}");
        }
    }
}

/// `make live-smoke-t1482`: single-page `t1482` after-hours top volume
/// (시간외거래량상위; all-segment, ascending sort, permissive filters, first-page
/// `idx`).
///
/// `after_hours_top_volume` returning `Ok` with a non-empty `outblock1` proves the
/// read is callable and the raw-capture row shape round-trips. The recorded line
/// carries only `rsp_cd` + a public row count (no `rsp_msg`, token, or account
/// text) and is self-dated; a failed run emits a distinct `SMOKE-FAIL` stderr line,
/// never a capturable `LIVE-SMOKE` line. An empty success (`00707`) outside an
/// after-hours session is the PENDING case, not a defect.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1482`"]
async fn live_smoke_t1482() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    // sort_gbn=0, all-segment, permissive volume flag, first page.
    let req = T1482Request::new("0", "0", "0");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().after_hours_top_volume(&req).await {
        Ok(resp) if resp.outblock1.is_empty() => {
            // Empty success (`00707`) outside an after-hours session is the PENDING
            // case, not Implemented evidence — emit no capturable LIVE-SMOKE line
            // (mirrors live_smoke_t1866's non-empty guard).
            eprintln!("SMOKE-FAIL target=live-smoke-t1482 empty result (00707); PENDING not evidence");
            panic!("live-smoke-t1482: empty result (00707) — PENDING, not Implemented");
        }
        Ok(resp) => record(
            "live-smoke-t1482",
            &format!("env=paper sort_gbn=0 idx=0 date={date}"),
            &smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line"),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1482 market-data failure (not evidence)");
            panic!("live-smoke-t1482 failed: {e}");
        }
    }
}

/// `make live-smoke-t1866`: paper guard → server-saved condition list (the
/// saved-condition spine producer). `user_id` comes from `LS_PAPER_USER_ID`
/// (never the caller, never recorded — it is account-identifying). The recorded
/// line carries only `rsp_cd` and the structural condition count; an empty list
/// (no seeded condition) surfaces as a credential-safe `SMOKE-FAIL` so it is
/// distinguishable from a defect.
#[tokio::test]
#[ignore = "live smoke: needs LS_PAPER_USER_ID + a seeded server-saved condition; run via `make live-smoke-t1866`"]
async fn live_smoke_t1866() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let user_id = match std::env::var("LS_PAPER_USER_ID") {
        Ok(u) if !u.is_empty() => u,
        _ => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1866 LS_PAPER_USER_ID unset (not evidence)");
            panic!("live-smoke-t1866: LS_PAPER_USER_ID required (the LS login id)");
        }
    };
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .paginated()
        .saved_conditions(&T1866Request::new(user_id))
        .await
    {
        Ok(resp) if resp.outblock1.is_empty() => {
            // Success transport but no saved condition exists → spine-input-unavailable.
            eprintln!(
                "SMOKE-FAIL target=live-smoke-t1866 no saved condition (rsp_cd={})",
                resp.rsp_cd
            );
            panic!("live-smoke-t1866: no server-saved condition to yield a query_index");
        }
        Ok(resp) => record(
            "live-smoke-t1866",
            &format!("env=paper gb=0 date={date}"),
            &smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "conditions")
                .expect("an Ok outcome yields a result line"),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1866 market-data failure (not evidence)");
            panic!("live-smoke-t1866 failed: {e}");
        }
    }
}

/// `make live-smoke-t1859`: paper guard → token → `t1866` saved-condition list →
/// `t1859` condition search keyed by the first saved condition's `query_index`.
///
/// CHAINED, self-sourcing (R8): the consumer never receives a fabricated
/// `query_index` — it is read from a live `t1866` call (mirrors `live_smoke_t1531`
/// self-sourcing a `tmcode` from `t8425`). `LS_PAPER_USER_ID` (the LS login id) is
/// required and never recorded; the `query_index` itself is account-saved-condition
/// data and is NOT printed. An empty `t1866` (no seeded condition) surfaces as a
/// credential-safe `SMOKE-FAIL` (spine-input-unavailable), never a fabricated key.
#[tokio::test]
#[ignore = "live smoke: needs LS_PAPER_USER_ID + a seeded server-saved condition; run via `make live-smoke-t1859`"]
async fn live_smoke_t1859() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let user_id = match std::env::var("LS_PAPER_USER_ID") {
        Ok(u) if !u.is_empty() => u,
        _ => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1859 LS_PAPER_USER_ID unset (not evidence)");
            panic!("live-smoke-t1859: LS_PAPER_USER_ID required (the LS login id)");
        }
    };

    // Self-source the query_index from a live t1866 saved-condition list.
    let conditions = sdk
        .paginated()
        .saved_conditions(&T1866Request::new(user_id))
        .await
        .expect("t1866 saved_conditions (query_index source) failed");
    if conditions.outblock1.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t1859 t1866 spine source empty (rsp_cd={})",
            conditions.rsp_cd
        );
        panic!("live-smoke-t1859: no server-saved condition to key the search");
    }
    let query_index = conditions.outblock1[0].query_index.clone();

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .condition_search(&T1859Request::new(query_index))
        .await
    {
        Ok(resp) => {
            // The query_index is NOT recorded — it is account-saved-condition data.
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1859", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1859 market-data failure (not evidence)");
            panic!("live-smoke-t1859 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t1826 / t1825 — ThinQ Q-click search (Wave 3 spine). t1826 lists the available
// searches (producer); t1825 runs one search keyed by a `search_cd` self-sourced
// from t1826 (consumer, chained). The `search_cd` is a server-assigned catalog
// key and is NEVER recorded (treated like the saved-condition `query_index`).
// ---------------------------------------------------------------------------

/// `make live-smoke-t1826`: paper guard → OAuth token → one `t1826` Q-click
/// search-list read for `search_gb="0"` (핵심검색/core search; the Wave 3
/// producer).
///
/// `qclick_search_list` returning `Ok` with a non-empty `outblock` proves the
/// read is callable and the row shape round-trips. The recorded line is
/// credential-free (only `rsp_cd` + a public search count; `search_cd` values are
/// NOT recorded) and self-dated; a failed run emits a distinct `SMOKE-FAIL`
/// stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1826`"]
async fn live_smoke_t1826() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1826Request::new("0");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().qclick_search_list(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "searches")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t1826",
                &format!("env=paper search_gb=0 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1826 market-data failure (not evidence)");
            panic!("live-smoke-t1826 failed: {e}");
        }
    }
}

/// `make live-smoke-t1825`: paper guard → token → `t1826` search-list →
/// `t1825` Q-click search keyed by the first available `search_cd`.
///
/// CHAINED, self-sourcing (R8): the consumer never receives a fabricated
/// `search_cd` — it is read from a live `t1826` call (mirrors `live_smoke_t1859`
/// self-sourcing a `query_index` from `t1866`). The `search_cd` is a
/// server-assigned catalog key and is NOT recorded. An empty `t1826` (no
/// available search) surfaces as a credential-safe `SMOKE-FAIL`
/// (spine-input-unavailable), never a fabricated key.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1825`"]
async fn live_smoke_t1825() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    // Self-source the search_cd from a live t1826 search-list.
    let list = sdk
        .market_session()
        .qclick_search_list(&T1826Request::new("0"))
        .await
        .expect("t1826 qclick_search_list (search_cd source) failed");
    if list.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t1825 t1826 spine source empty (rsp_cd={})",
            list.rsp_cd
        );
        panic!("live-smoke-t1825: no available search to key the Q-click search");
    }
    let search_cd = list.outblock[0].search_cd.clone();

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .qclick_search(&T1825Request::new(search_cd, "0"))
        .await
    {
        Ok(resp) => {
            // The search_cd is NOT recorded — it is a server-assigned catalog key.
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t1825",
                &format!("env=paper gubun=0 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1825 market-data failure (not evidence)");
            panic!("live-smoke-t1825 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Wave 1 — ELW universe/list reads (t9905, t9907, t8431, t9942). No-caller-input
// `dummy` reads; each gates on a non-empty success.
// ---------------------------------------------------------------------------

/// `make live-smoke-t9905`: paper guard → token → one `t9905` underlying-asset
/// list read (no caller input). Non-empty success → flip.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t9905`"]
async fn live_smoke_t9905() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().underlying_list(&T9905Request::new()).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "underlyings")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t9905", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t9905 market-data failure (not evidence)");
            panic!("live-smoke-t9905 failed: {e}");
        }
    }
}

/// `make live-smoke-t9907`: paper guard → token → one `t9907` ELW expiry-month
/// list read (no caller input). Non-empty success → flip.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t9907`"]
async fn live_smoke_t9907() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .elw_expiry_months(&T9907Request::new())
        .await
    {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "months")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t9907", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t9907 market-data failure (not evidence)");
            panic!("live-smoke-t9907 failed: {e}");
        }
    }
}

/// `make live-smoke-t8431`: paper guard → token → one `t8431` ELW-symbol list
/// read (no caller input; the Wave 1 spine producer). Non-empty success → flip.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8431`"]
async fn live_smoke_t8431() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().elw_symbols(&T8431Request::new()).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "elws")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8431", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8431 market-data failure (not evidence)");
            panic!("live-smoke-t8431 failed: {e}");
        }
    }
}

/// `make live-smoke-t9942`: paper guard → token → one `t9942` ELW master list
/// read (no caller input). Non-empty success → flip.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t9942`"]
async fn live_smoke_t9942() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().elw_master(&T9942Request::new()).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "elws")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t9942", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t9942 market-data failure (not evidence)");
            panic!("live-smoke-t9942 failed: {e}");
        }
    }
}

/// `make live-smoke-t1958`: paper guard → token → `t8431` ELW-symbol list →
/// `t1958` comparison of the first two ELW `shcode`s.
///
/// CHAINED, self-sourcing (R8): the two `shcode`s come from a live `t8431` call,
/// never fabricated. ELW `shcode`s are public market identifiers (may appear in
/// `inputs`). The gate is the symbol-1 detail block (`outblock.hname`) being
/// populated — the comparison ran. An empty/short `t8431` surfaces as a
/// credential-safe `SMOKE-FAIL` (spine-input-unavailable).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1958`"]
async fn live_smoke_t1958() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    // Self-source two ELW shcodes from a live t8431 list.
    let elws = sdk
        .market_session()
        .elw_symbols(&T8431Request::new())
        .await
        .expect("t8431 elw_symbols (shcode source) failed");
    if elws.outblock.len() < 2 {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t1958 t8431 spine source <2 codes (rsp_cd={})",
            elws.rsp_cd
        );
        panic!("live-smoke-t1958: need two ELW shcodes to compare");
    }
    let (shcode1, shcode2) = (elws.outblock[0].shcode.clone(), elws.outblock[1].shcode.clone());

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .elw_compare(&T1958Request::new(&shcode1, &shcode2))
        .await
    {
        Ok(resp) if resp.outblock.hname.is_empty() => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-t1958 empty comparison payload (rsp_cd={})",
                resp.rsp_cd
            );
            panic!("live-smoke-t1958: comparison block empty (shape-unconfirmed)");
        }
        Ok(resp) => {
            // shcodes are public ELW identifiers — OK to record.
            record(
                "live-smoke-t1958",
                &format!("env=paper shcode1={shcode1} shcode2={shcode2} date={date}"),
                &format!("rsp_cd={} compared=2", resp.rsp_cd),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1958 market-data failure (not evidence)");
            panic!("live-smoke-t1958 failed: {e}");
        }
    }
}

/// `make live-smoke-t1964`: paper guard → token → `t9905` underlying list →
/// `t1964` ELW board for the first underlying (broad/default filters).
///
/// CHAINED, self-sourcing (R8): the `item` underlying code comes from a live
/// `t9905` call, never fabricated. The smoke walks the first several underlyings
/// until one returns a non-empty board (an underlying with no listed ELWs is not
/// a failure). An empty `t9905`, or no underlying yielding a board, surfaces as a
/// credential-safe `SMOKE-FAIL`.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1964`"]
async fn live_smoke_t1964() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let underlyings = sdk
        .market_session()
        .underlying_list(&T9905Request::new())
        .await
        .expect("t9905 underlying_list (item source) failed");
    if underlyings.outblock1.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t1964 t9905 spine source empty (rsp_cd={})",
            underlyings.rsp_cd
        );
        panic!("live-smoke-t1964: no underlying to key the board");
    }

    let date = Utc::now().format("%Y-%m-%d");
    // Walk the first several underlyings until one has a non-empty board. Pace
    // the calls (t1964 is 2/sec) so the walk does not self-trigger IGW00201
    // throttling (transient, environmental — not a TR defect).
    for u in underlyings.outblock1.iter().take(10) {
        tokio::time::sleep(Duration::from_millis(700)).await;
        let item = u.shcode.clone();
        match sdk
            .market_session()
            .elw_board(&T1964Request::new(&item))
            .await
        {
            Ok(resp) if !resp.outblock1.is_empty() => {
                record(
                    "live-smoke-t1964",
                    &format!("env=paper item={item} date={date}"),
                    &format!("rsp_cd={} elws={}", resp.rsp_cd, resp.outblock1.len()),
                );
                return;
            }
            Ok(_) => continue, // this underlying has no listed ELWs; try the next
            Err(e) => {
                eprintln!("SMOKE-FAIL target=live-smoke-t1964 market-data failure (not evidence)");
                panic!("live-smoke-t1964 failed: {e}");
            }
        }
    }
    eprintln!("SMOKE-FAIL target=live-smoke-t1964 no underlying yielded a non-empty board");
    panic!("live-smoke-t1964: no non-empty board among the first underlyings (shape-unconfirmed)");
}

// ---------------------------------------------------------------------------
// Wave 2 — market-flow analytics reads (t1601, t1615, t1640, t1662, t1664).
// Standalone gubun-filter reads with documented defaults; non-empty success gate.
// ---------------------------------------------------------------------------

/// `make live-smoke-t1601`: token → one `t1601` investor-by-type aggregate.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1601`"]
async fn live_smoke_t1601() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().investor_aggregate(&T1601Request::new()).await {
        Ok(resp) if resp.outblock1.svolume_08.is_empty() && resp.outblock1.svolume_17.is_empty() => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1601 empty aggregate (rsp_cd={})", resp.rsp_cd);
            panic!("live-smoke-t1601: empty investor aggregate (shape-unconfirmed)");
        }
        Ok(resp) => record(
            "live-smoke-t1601",
            &format!("env=paper exchgubun=K date={date}"),
            &format!("rsp_cd={} aggregate=populated", resp.rsp_cd),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1601 market-data failure (not evidence)");
            panic!("live-smoke-t1601 failed: {e}");
        }
    }
}

/// `make live-smoke-t1615`: token → one `t1615` investor trading aggregate.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1615`"]
async fn live_smoke_t1615() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().investor_trading(&T1615Request::new()).await {
        Ok(resp) if resp.outblock1.is_empty() && resp.outblock.sum_value.is_empty() => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1615 empty aggregate (rsp_cd={})", resp.rsp_cd);
            panic!("live-smoke-t1615: empty trading aggregate (shape-unconfirmed)");
        }
        Ok(resp) => record(
            "live-smoke-t1615",
            &format!("env=paper exchgubun=K date={date}"),
            &format!("rsp_cd={} markets={}", resp.rsp_cd, resp.outblock1.len()),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1615 market-data failure (not evidence)");
            panic!("live-smoke-t1615 failed: {e}");
        }
    }
}

/// `make live-smoke-t1640`: token → one `t1640` program-trading aggregate.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1640`"]
async fn live_smoke_t1640() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().program_aggregate(&T1640Request::new()).await {
        Ok(resp) if resp.outblock.value.is_empty() && resp.outblock.volume.is_empty() => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1640 empty aggregate (rsp_cd={})", resp.rsp_cd);
            panic!("live-smoke-t1640: empty program aggregate (shape-unconfirmed)");
        }
        Ok(resp) => record(
            "live-smoke-t1640",
            &format!("env=paper gubun=11 date={date}"),
            &format!("rsp_cd={} aggregate=populated", resp.rsp_cd),
        ),
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1640 market-data failure (not evidence)");
            panic!("live-smoke-t1640 failed: {e}");
        }
    }
}

/// `make live-smoke-t1662`: token → one `t1662` by-time program-trading chart.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1662`"]
async fn live_smoke_t1662() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().program_chart(&T1662Request::new()).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1662", &format!("env=paper gubun=0 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1662 market-data failure (not evidence)");
            panic!("live-smoke-t1662 failed: {e}");
        }
    }
}

/// `make live-smoke-t1664`: token → one `t1664` investor trading chart.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1664`"]
async fn live_smoke_t1664() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().investor_chart(&T1664Request::new()).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t1664", &format!("env=paper mgubun=1 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1664 market-data failure (not evidence)");
            panic!("live-smoke-t1664 failed: {e}");
        }
    }
}

/// `make live-smoke-t3341`: token → one single-page `t3341` financial-ranking
/// read (body `idx`=0 as a number; single-page scope, KTD-5).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t3341`"]
async fn live_smoke_t3341() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().financial_ranking(&T3341Request::new()).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "ranks")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t3341", &format!("env=paper gubun=0 idx=0 date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t3341 market-data failure (not evidence)");
            panic!("live-smoke-t3341 failed: {e}");
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

/// `make live-smoke-cspaq12300`: paper guard → read-only `CSPAQ12300` BEP/balance.
///
/// The account number comes from config, never the caller — the four query-shape
/// enums (`balcretp,cmsnapptpcode,d2balbaseqrytp,uprctpcode`) are the only inputs.
/// A success records a credential-free line (only the numeric `rsp_cd` and a
/// structural `outblock2` row count; `rsp_msg` is dropped because it can carry
/// account-identifying text). A failed run emits a distinct `SMOKE-FAIL` stderr
/// line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs a provisioned LS paper account; run via `make live-smoke-cspaq12300`"]
async fn live_smoke_cspaq12300() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let req = CSPAQ12300Request::new("1", "0", "0", "0");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.account().bep(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock2.len())), "balrows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-cspaq12300",
                &format!("env=paper balcretp=1 date={date}"),
                &line,
            );
        }
        // A failed run must NOT emit a capturable `LIVE-SMOKE` line: the raw
        // gateway error can carry account-identifying text. Use a distinct
        // non-`LIVE-SMOKE` stderr prefix; the panic is unchanged.
        Err(e) => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-cspaq12300 account-state failure (not transport)"
            );
            panic!("live-smoke-cspaq12300 failed (account-state, may be paper-account setup): {e}");
        }
    }
}

/// `make live-smoke-cspaq22200`: paper guard → read-only `CSPAQ22200`
/// orderable-amount / valuation inquiry.
///
/// The account number comes from config, never the caller — `balcretp` is the only
/// input. A success records a credential-free line (only the numeric `rsp_cd` and a
/// structural `outblock2` row count; `rsp_msg` is dropped because it can carry
/// account-identifying text). A failed run emits a distinct `SMOKE-FAIL` stderr
/// line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs a provisioned LS paper account; run via `make live-smoke-cspaq22200`"]
async fn live_smoke_cspaq22200() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let req = CSPAQ22200Request::new("1");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.account().orderable(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock2.len())), "balrows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-cspaq22200",
                &format!("env=paper balcretp=1 date={date}"),
                &line,
            );
        }
        // A failed run must NOT emit a capturable `LIVE-SMOKE` line: the raw
        // gateway error can carry account-identifying text. Use a distinct
        // non-`LIVE-SMOKE` stderr prefix; the panic is unchanged.
        Err(e) => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-cspaq22200 account-state failure (not transport)"
            );
            panic!("live-smoke-cspaq22200 failed (account-state, may be paper-account setup): {e}");
        }
    }
}

/// `make live-smoke-cfobq10500`: paper guard → read-only `CFOBQ10500` F/O account
/// deposit / margin inquiry.
///
/// The account number comes from config, never the caller — this is a header-only
/// read with no caller input. A success records a credential-free line (only the
/// numeric `rsp_cd` and a structural `outblock2` row count; `rsp_msg` is dropped
/// because it can carry account-identifying text). A position-less paper account
/// may return an empty `00707` deposit (the PENDING case), which still records a
/// credential-free line with the row count = 0. A failed run emits a distinct
/// `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs a provisioned LS paper account; run via `make live-smoke-cfobq10500`"]
async fn live_smoke_cfobq10500() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let req = CFOBQ10500Request::new();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.account().fo_deposit(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock2.len())), "deprows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-cfobq10500",
                &format!("env=paper date={date}"),
                &line,
            );
        }
        // A failed run must NOT emit a capturable `LIVE-SMOKE` line: the raw
        // gateway error can carry account-identifying text. Use a distinct
        // non-`LIVE-SMOKE` stderr prefix; the panic is unchanged.
        Err(e) => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-cfobq10500 account-state failure (not transport)"
            );
            panic!("live-smoke-cfobq10500 failed (account-state, may be paper-account setup): {e}");
        }
    }
}

/// `make live-smoke-ccenq90200`: paper guard → read-only `CCENQ90200`
/// KRX night-derivatives account balance inquiry (krx_extended session).
///
/// The account number comes from config, never the caller — the only inputs are
/// the record count and two evaluation-shape enums. This is a night (krx_extended)
/// read: an empty `00707`/empty result OFF the night window is the PENDING case
/// (callable, shape unconfirmed), NOT a defect — the regular ~09:00–15:30 KST clock
/// does not apply. A success records a credential-free line (only the numeric
/// `rsp_cd` and a structural `outblock2` row count; `rsp_msg` is dropped because it
/// can carry account-identifying text). A failed run emits a distinct `SMOKE-FAIL`
/// stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs a provisioned LS paper account; run via `make live-smoke-ccenq90200`"]
async fn live_smoke_ccenq90200() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let req = CCENQ90200Request::new("1", "0", "0");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.account().night_balance(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock2.len())), "balrows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-ccenq90200",
                &format!("env=paper balevaltp=0 futsprcevaltp=0 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-ccenq90200 account-state failure (not transport)"
            );
            panic!("live-smoke-ccenq90200 failed (account-state, may be paper-account setup): {e}");
        }
    }
}

/// `make live-smoke-cfoaq10100`: paper guard → read-only `CFOAQ10100` F/O
/// orderable-quantity INQUIRY (조회, NOT an order).
///
/// The account number comes from config, never the caller. The F/O instrument is
/// taken from `LS_LIVE_SMOKE_FNOISU` (a current KOSPI200-futures code, e.g.
/// `101V6000`) so the smoke targets a live contract; the rest are conservative
/// query-shape inputs. A position-less paper account may return an empty `00707`
/// (the PENDING case). A success records a credential-free line (only the numeric
/// `rsp_cd` and a structural `outblock2` row count; `rsp_msg` is dropped). A failed
/// run emits a distinct `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE`
/// line.
#[tokio::test]
#[ignore = "live smoke: needs a provisioned LS paper account + current FnoIsuNo; run via `make live-smoke-cfoaq10100`"]
async fn live_smoke_cfoaq10100() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let fnoisu = std::env::var("LS_LIVE_SMOKE_FNOISU").unwrap_or_else(|_| "101V6000".to_string());
    let req = CFOAQ10100Request::new("1", "1", "0", "0", &fnoisu, "1", "0", "00");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.account().fo_orderable_qty(&req).await {
        Ok(resp) if resp.outblock2.is_empty() => {
            // Empty success (`00707`) on a position-less paper account is the PENDING
            // case, not Implemented evidence — emit no capturable LIVE-SMOKE line
            // (mirrors live_smoke_t1866's non-empty guard).
            eprintln!("SMOKE-FAIL target=live-smoke-cfoaq10100 empty result (00707); PENDING not evidence");
            panic!("live-smoke-cfoaq10100: empty result (00707) — PENDING, not Implemented");
        }
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock2.len())), "qtyrows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-cfoaq10100",
                &format!("env=paper fnoisu={fnoisu} qrytp=1 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-cfoaq10100 account-state failure (not transport)"
            );
            panic!("live-smoke-cfoaq10100 failed (account-state, may be paper-account setup): {e}");
        }
    }
}

/// `make live-smoke-ccenq10100`: paper guard → read-only `CCENQ10100` KRX
/// night-derivatives orderable-quantity INQUIRY (조회, NOT an order; krx_extended).
///
/// The account number comes from config, never the caller. The F/O instrument is
/// taken from `LS_LIVE_SMOKE_FNOISU`. This is a night (krx_extended) read: an empty
/// `00707`/empty result off the night window is the PENDING case, NOT a defect —
/// the regular clock does not apply. A success records a credential-free line (only
/// the numeric `rsp_cd` and a structural `outblock2` row count; `rsp_msg` dropped).
/// A failed run emits a distinct `SMOKE-FAIL` stderr line, never a capturable
/// `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs a provisioned LS paper account + current FnoIsuNo; run via `make live-smoke-ccenq10100`"]
async fn live_smoke_ccenq10100() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");

    let fnoisu = std::env::var("LS_LIVE_SMOKE_FNOISU").unwrap_or_else(|_| "101V6000".to_string());
    let req = CCENQ10100Request::new("1", "1", "0", "0", &fnoisu, "1", "0", "00");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.account().night_orderable_qty(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock2.len())), "qtyrows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-ccenq10100",
                &format!("env=paper fnoisu={fnoisu} qrytp=1 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!(
                "SMOKE-FAIL target=live-smoke-ccenq10100 account-state failure (not transport)"
            );
            panic!("live-smoke-ccenq10100 failed (account-state, may be paper-account setup): {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// U3/U6 — WebSocket lifecycle smoke (generic helper + S3_ + negative control)
// ---------------------------------------------------------------------------

/// GENERIC WS lifecycle smoke, parameterized by `(tr_cd, tr_key, tr_type)` — the
/// reusable helper the per-TR U5/U6 smokes call (KTD2).
///
/// Runs the full lifecycle on a FRESH/isolated `WsManager` per call (a fresh
/// `LsSdk` whose `.realtime()` builds a new manager — the Phase 83/84 lesson: a
/// shared manager poisons later TRs). Steps:
///   1. paper guard (refuses unless `LS_TRADING_ENV=paper`),
///   2. assert the resolved WS URL carries the paper port `29443` (fail fast on a
///      wrong target — WS ports differ by environment),
///   3. subscribe via a fresh manager, decoding into a PERMISSIVE row type
///      ([`WsLifecycleRow`]) since lifecycle-only smokes never require a real row,
///   4. timebox a row as BONUS — absence is NOT a failure,
///   5. unsubscribe cleanly (the blocking lifecycle assertion).
///
/// Returns the credential-free `row_note` for the caller to `record(...)`. NO
/// raw-frame logging anywhere on this path (ACK frames echo the bearer token).
async fn ws_lifecycle_smoke(tr_cd: &str, tr_key: &str, tr_type: &str) -> String {
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

    // Fresh SDK → fresh, isolated WsManager (KTD2 — no shared-manager poisoning).
    let sdk = LsSdk::new(config).expect("sdk construction");
    let ws = sdk.realtime();

    let (handle, mut stream) = ws
        .subscribe_typed::<WsLifecycleRow>(tr_cd, tr_key, tr_type)
        .await
        .unwrap_or_else(|e| panic!("subscribe_typed {tr_cd} failed (connect/subscribe lifecycle): {e}"));

    // BONUS: a row may or may not arrive inside the timebox; absence is not a
    // failure. We surface only WHETHER a body arrived and whether it looked like
    // a rejection (non-empty rsp_cd) — never the body contents.
    let row_note = match timeout(Duration::from_secs(5), stream.next()).await {
        Ok(Some(Ok(row))) if !row.rsp_cd.is_empty() => {
            format!("inbound body carried rsp_cd={} (rejection-shaped)", row.rsp_cd)
        }
        Ok(Some(Ok(_))) => "row received (lifecycle bonus)".to_string(),
        Ok(Some(Err(e))) => format!("frame decode error: {e}"),
        Ok(None) => "stream ended without a row".to_string(),
        Err(_) => "no row within timeout (not a failure)".to_string(),
    };

    handle
        .unsubscribe()
        .await
        .expect("unsubscribe must complete cleanly");

    row_note
}

/// Permissive lifecycle row for [`ws_lifecycle_smoke`] — lifecycle-only smokes
/// never require a real row, so any body deserializes here. `rsp_cd` is the one
/// field we read: a non-empty value on an inbound body is the observable signal
/// of a gateway rejection (the live half of KTD6's open question).
#[derive(serde::Deserialize, Debug, Default)]
struct WsLifecycleRow {
    #[serde(default)]
    rsp_cd: String,
}

/// `make live-smoke-ws`: generic lifecycle smoke for `S3_` (market-data, "3").
/// Covers AE6. Delegates to [`ws_lifecycle_smoke`] so the single-TR smoke and the
/// per-TR U5/U6 smokes share one code path.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-ws`"]
async fn live_smoke_ws() {
    let symbol = resolve_symbol();
    let row_note = ws_lifecycle_smoke("S3_", &symbol, "3").await;
    record(
        "live-smoke-ws",
        &format!("symbol={symbol} ws_port=29443 tr_type=3"),
        &row_note,
    );
}

/// `make live-smoke-ws-negative`: LIVE half of the KTD6 negative control.
///
/// Subscribes a deliberately-INVALID `tr_cd` and reports whether the paper
/// gateway emits an OBSERVABLE rejection within the timebox. This is the live
/// complement of the DETERMINISTIC mock-WS negative control in
/// `realtime_tests.rs::negative_control_rejected_tr_cd_is_observably_distinct_from_accepted`.
///
/// WHAT "observable" means here, and WHY it is uncertain: the SDK subscribe path
/// is FIRE-AND-FORGET — it never reads the subscribe ACK — so `subscribe_typed`
/// returns `Ok` for a valid AND an invalid `tr_cd` alike. The only live signal a
/// rejection can produce, given today's code, is an INBOUND frame the gateway
/// pushes whose `header.tr_cd`/`tr_key` route it back to this subscriber's
/// stream (surfacing as a body with a non-empty `rsp_cd`), OR a CLOSED stream.
/// If neither arrives — only silence — then a rejected and an accepted subscribe
/// are indistinguishable on the live paper path, which means every U5/U6 flip
/// can claim only CONNECTION-REACHABLE-ONLY, not per-TR reachability.
///
/// OPEN-QUESTION STATUS: UNRESOLVED in this environment — there is no market
/// session or live credentials here, so this smoke is `#[ignore]` and a human
/// runs it later via `make live-smoke-ws-negative`. The recorded `result=[...]`
/// line is the answer; we do NOT fabricate it. Until then the wave's flip claim
/// stays the weaker connection-reachable-only per KTD6.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials + a session to observe a rejection; run via `make live-smoke-ws-negative`"]
async fn live_smoke_ws_negative() {
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

    // A deliberately-invalid TR code: not a real LS realtime channel. tr_type "3"
    // (market-data register) is used arbitrarily — the lane is irrelevant when the
    // code itself is bogus.
    const INVALID_TR_CD: &str = "ZZ9";

    let sdk = LsSdk::new(config).expect("sdk construction");
    let ws = sdk.realtime();

    // subscribe_typed may well return Ok even for an invalid code (fire-and-forget).
    let subscribe_outcome = ws
        .subscribe_typed::<WsLifecycleRow>(INVALID_TR_CD, &resolve_symbol(), "3")
        .await;

    let observation = match subscribe_outcome {
        Err(e) => format!("subscribe returned Err immediately: {e}"),
        Ok((handle, mut stream)) => {
            // Timebox for an observable rejection: an inbound rejection-shaped
            // body, a decode error, or a closed stream. Silence ⇒ NOT observable.
            let note = match timeout(Duration::from_secs(5), stream.next()).await {
                Ok(Some(Ok(row))) if !row.rsp_cd.is_empty() => {
                    format!("OBSERVABLE: inbound rejection body rsp_cd={}", row.rsp_cd)
                }
                Ok(Some(Ok(_))) => "inbound body with no rsp_cd (ambiguous)".to_string(),
                Ok(Some(Err(_))) => "OBSERVABLE: stream yielded an error item".to_string(),
                Ok(None) => "OBSERVABLE: stream closed (gateway severed)".to_string(),
                Err(_) => {
                    "NOT-OBSERVABLE: silence in timebox — flips are connection-reachable-only"
                        .to_string()
                }
            };
            // Clean up regardless of outcome.
            let _ = handle.unsubscribe().await;
            note
        }
    };

    record(
        "live-smoke-ws-negative",
        &format!("invalid_tr_cd={INVALID_TR_CD} ws_port=29443"),
        &observation,
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

// ---------------------------------------------------------------------------
// [업종] 시세 — sector/index cluster (Wave A). All on /indtp/market-data.
// t8424 is the anchor + upcode source; the four consumers smoke standalone with
// upcode="001" (코스피종합), confirmed accepted by the U1 raw-probe.
// ---------------------------------------------------------------------------

/// `make live-smoke-t8424`: paper guard → OAuth token → one `t8424` all-sectors
/// read. A non-empty sector array proves the anchor is callable and round-trips.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8424`"]
async fn live_smoke_t8424() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8424Request::new();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().sectors(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "sectors")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8424", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8424 market-data failure (not evidence)");
            panic!("live-smoke-t8424 failed: {e}");
        }
    }
}

/// `make live-smoke-t1511`: paper guard → OAuth token → one `t1511` index
/// snapshot for `upcode="001"`. A single OutBlock with a success `rsp_cd` proves
/// the read is callable and the snapshot round-trips. KRX-session-dependent.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1511`"]
async fn live_smoke_t1511() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1511Request::new("001");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().sector_quote(&req).await {
        Ok(resp) => {
            record(
                "live-smoke-t1511",
                &format!("env=paper upcode=001 date={date}"),
                &format!(
                    "rsp_cd={} hname_len={} pricejisu={}",
                    resp.rsp_cd,
                    resp.outblock.hname.len(),
                    resp.outblock.pricejisu
                ),
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1511 market-data failure (not evidence)");
            panic!("live-smoke-t1511 failed: {e}");
        }
    }
}

/// `make live-smoke-t1485`: paper guard → OAuth token → one `t1485` expected-index
/// read for `upcode="001"`, `gubun="1"`. The time-row array `t1485OutBlock1`
/// proves the read round-trips. Expected/auction screen — KRX-session-dependent.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1485`"]
async fn live_smoke_t1485() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1485Request::new("001", "1");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().sector_expected_index(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t1485",
                &format!("env=paper upcode=001 gubun=1 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1485 market-data failure (not evidence)");
            panic!("live-smoke-t1485 failed: {e}");
        }
    }
}

/// `make live-smoke-t1516`: paper guard → OAuth token → one `t1516` per-sector
/// stock-board read for `upcode="001"` + a representative `shcode="005930"`. The
/// per-stock array `t1516OutBlock1` proves the read round-trips. Session-dependent.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1516`"]
async fn live_smoke_t1516() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1516Request::new("001", "1", "005930");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().sector_stocks(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "stocks")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t1516",
                &format!("env=paper upcode=001 shcode=005930 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1516 market-data failure (not evidence)");
            panic!("live-smoke-t1516 failed: {e}");
        }
    }
}

/// `make live-smoke-t1514`: paper guard → OAuth token → one first-page `t1514`
/// period-trend read for `upcode="001"`. Self-paginated (`cts_date` cursor, `cnt`
/// serialized as a number); a non-empty first page proves the paginated path.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1514`"]
async fn live_smoke_t1514() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T1514Request::new("001");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.paginated().sector_trend(&req).await {
        Ok(resp) => {
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t1514",
                &format!("env=paper upcode=001 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1514 market-data failure (not evidence)");
            panic!("live-smoke-t1514 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t2301 — 옵션전광판 (option board; F/O). market_session, non-paginated. Keyed by
// a near-quarterly contract month (`yyyymm`) + a `gubun` mini/regular selector.
// Master/board read — non-empty regardless of the KRX session (venue facet stays
// provisional). The structural signal is the canonical field's length (a single
// out-block, not an array), kept credential-free.
// ---------------------------------------------------------------------------

/// `make live-smoke-t2301`: paper guard → OAuth token → one `t2301` option-board
/// read for `yyyymm="202609"`, `gubun="G"` (정규/regular). A success `rsp_cd`
/// with a populated board header proves the read is callable and round-trips. The
/// recorded line is credential-free (only `rsp_cd` + the canonical `gmprice`
/// field's length, never `rsp_msg`) and self-dated; a failed run emits a distinct
/// `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t2301`"]
async fn live_smoke_t2301() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T2301Request::new("202609", "G");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().option_board(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock.gmprice.is_empty(),
                "live-smoke-t2301: empty result (00707) — PENDING, not Implemented"
            );
            let line =
                smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.gmprice.len())), "gmprice_len")
                    .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t2301",
                &format!("env=paper yyyymm=202609 gubun=G date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t2301 market-data failure (not evidence)");
            panic!("live-smoke-t2301 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t2522 — 주식선물기초자산조회 (stock-futures underlying-asset master; F/O).
// market_session, non-paginated, no caller input (a single `dummy` placeholder).
// Master read — non-empty regardless of the KRX session (venue facet stays
// provisional). The structural signal is the canonical field's length (a single
// out-block, not an array), kept credential-free.
// ---------------------------------------------------------------------------

/// `make live-smoke-t2522`: paper guard → OAuth token → one `t2522`
/// underlying-asset master read (no caller input). A success `rsp_cd` with a
/// populated `t2522OutBlock1` row array proves the read is callable and
/// round-trips. The recorded line is credential-free (only `rsp_cd` + the row
/// count, never `rsp_msg`) and self-dated; a failed run emits a distinct
/// `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t2522`"]
async fn live_smoke_t2522() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T2522Request::new();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().stock_futures_underlying_master(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock1.is_empty(),
                "live-smoke-t2522: empty result (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(
                Ok((resp.rsp_cd.clone(), resp.outblock1.len())),
                "rows",
            )
            .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t2522",
                &format!("env=paper date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t2522 market-data failure (not evidence)");
            panic!("live-smoke-t2522 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t8401 — 주식선물마스터조회 (stock-futures master; F/O). market_session,
// non-paginated, no caller input (a single `dummy` placeholder). Master read —
// non-empty regardless of the KRX session (venue facet stays provisional). The
// structural signal is the out-block row count (a single row-array out-block),
// kept credential-free.
// ---------------------------------------------------------------------------

/// `make live-smoke-t8401`: paper guard → OAuth token → one `t8401`
/// stock-futures master read (no caller input). A success `rsp_cd` with a
/// populated `t8401OutBlock` row array proves the read is callable and
/// round-trips. The recorded line is credential-free (only `rsp_cd` + the row
/// count, never `rsp_msg`) and self-dated; a failed run emits a distinct
/// `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8401`"]
async fn live_smoke_t8401() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8401Request::new();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().stock_futures_master(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock.is_empty(),
                "live-smoke-t8401: empty result (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8401", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8401 market-data failure (not evidence)");
            panic!("live-smoke-t8401 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t8426 — 상품선물마스터조회 (commodity-futures master; F/O). market_session,
// non-paginated, no caller input (a single `dummy` placeholder). Master read —
// non-empty regardless of the KRX session (venue facet stays provisional). The
// structural signal is the out-block row count (a single row-array out-block),
// kept credential-free.
// ---------------------------------------------------------------------------

/// `make live-smoke-t8426`: paper guard → OAuth token → one `t8426`
/// commodity-futures master read (no caller input). A success `rsp_cd` with a
/// populated `t8426OutBlock` row array proves the read is callable and
/// round-trips. The recorded line is credential-free (only `rsp_cd` + the row
/// count, never `rsp_msg`) and self-dated; a failed run emits a distinct
/// `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8426`"]
async fn live_smoke_t8426() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8426Request::new();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().commodity_futures_master(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock.is_empty(),
                "live-smoke-t8426: empty result (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8426", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8426 market-data failure (not evidence)");
            panic!("live-smoke-t8426 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t8433 — 지수옵션마스터조회API용 (index-option master; F/O). market_session,
// non-paginated, no caller input (a single `dummy` placeholder). Master read —
// non-empty regardless of the KRX session (venue facet stays provisional). The
// structural signal is the out-block row count (a single row-array out-block),
// kept credential-free.
// ---------------------------------------------------------------------------

/// `make live-smoke-t8433`: paper guard → OAuth token → one `t8433` index-option
/// master read (no caller input). A success `rsp_cd` with a populated
/// `t8433OutBlock` row array proves the read is callable and round-trips. The
/// recorded line is credential-free (only `rsp_cd` + the row count, never
/// `rsp_msg`) and self-dated; a failed run emits a distinct `SMOKE-FAIL` stderr
/// line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8433`"]
async fn live_smoke_t8433() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8433Request::new();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().index_option_master(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock.is_empty(),
                "live-smoke-t8433: empty result (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t8433", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8433 market-data failure (not evidence)");
            panic!("live-smoke-t8433 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t8435 — 파생종목마스터조회API용 (derivatives master; F/O). market_session,
// non-paginated. Keyed by a `gubun` segment selector — the LS spec defines these
// as the MINI/weekly segments: `"MF"` 미니선물 / `"MO"` 미니옵션 /
// `"WK"` 코스피200위클리옵션 / `"SF"` 코스닥150선물 / `"QW"` 코스닥150위클리옵션.
// Master read — non-empty regardless of the KRX session (venue facet stays
// provisional). The out-block is a row array (KTD3), so the structural signal is
// the row count, kept credential-free.
// ---------------------------------------------------------------------------

/// `make live-smoke-t8435`: paper guard → OAuth token → one `t8435` derivatives
/// master read for `gubun="MF"` (미니선물/mini futures). A success `rsp_cd` with a
/// populated `t8435OutBlock` row array proves the read is callable and
/// round-trips. The recorded line is credential-free (only `rsp_cd` + the row
/// count, never `rsp_msg`) and self-dated; a failed run emits a distinct
/// `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8435`"]
async fn live_smoke_t8435() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8435Request::new("MF");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().derivatives_master(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock.is_empty(),
                "live-smoke-t8435: empty result (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8435",
                &format!("env=paper gubun=MF date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8435 market-data failure (not evidence)");
            panic!("live-smoke-t8435 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t8467 — 지수선물마스터조회API용 (index-futures master; F/O). market_session,
// non-paginated. Keyed by a `gubun` segment selector (`"V"` volatility / `"S"`
// sector / `"Q"` KOSDAQ150 / any other value → KOSPI200 index futures). Master
// read — non-empty regardless of the KRX session (venue facet stays
// provisional). The out-block is a row array (KTD3), so the structural signal is
// the row count, kept credential-free.
// ---------------------------------------------------------------------------

/// `make live-smoke-t8467`: paper guard → OAuth token → one `t8467` index-futures
/// master read for `gubun="Q"` (KOSDAQ150-index futures). A success `rsp_cd` with
/// a populated `t8467OutBlock` row array proves the read is callable and
/// round-trips. The recorded line is credential-free (only `rsp_cd` + the row
/// count, never `rsp_msg`) and self-dated; a failed run emits a distinct
/// `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8467`"]
async fn live_smoke_t8467() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T8467Request::new("Q");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().index_futures_master(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock.is_empty(),
                "live-smoke-t8467: empty result (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8467",
                &format!("env=paper gubun=Q date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8467 market-data failure (not evidence)");
            panic!("live-smoke-t8467 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t9943 — 지수선물마스터조회API용 (index-futures master; F/O). market_session,
// non-paginated. Keyed by a `gubun` segment selector (`"V"` volatility / `"S"`
// sector / any other value → KOSPI200 index futures). Master read — non-empty
// regardless of the KRX session (venue facet stays provisional). The out-block is
// a row array (KTD3, true wire key `t9943OutBlock` from the raw capture), so the
// structural signal is the row count, kept credential-free.
// ---------------------------------------------------------------------------

/// `make live-smoke-t9943`: paper guard → OAuth token → one `t9943` index-futures
/// master read for `gubun="V"` (volatility-index futures). A success `rsp_cd` with
/// a populated `t9943OutBlock` row array proves the read is callable and
/// round-trips. The recorded line is credential-free (only `rsp_cd` + the row
/// count, never `rsp_msg`) and self-dated; a failed run emits a distinct
/// `SMOKE-FAIL` stderr line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t9943`"]
async fn live_smoke_t9943() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T9943Request::new("V");
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().index_futures_master_codes(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock.is_empty(),
                "live-smoke-t9943: empty result (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t9943",
                &format!("env=paper gubun=V date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t9943 market-data failure (not evidence)");
            panic!("live-smoke-t9943 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// t9944 — 지수옵션마스터조회API용 (index-option master; F/O). market_session,
// non-paginated, no caller input (a single `dummy` placeholder). Master read —
// non-empty regardless of the KRX session (venue facet stays provisional). The
// out-block is a row array (KTD3, true wire key `t9944OutBlock` from the raw
// capture), so the structural signal is the row count, kept credential-free.
// ---------------------------------------------------------------------------

/// `make live-smoke-t9944`: paper guard → OAuth token → one `t9944` index-option
/// master read (no caller input). A success `rsp_cd` with a populated
/// `t9944OutBlock` row array proves the read is callable and round-trips. The
/// recorded line is credential-free (only `rsp_cd` + the row count, never
/// `rsp_msg`) and self-dated; a failed run emits a distinct `SMOKE-FAIL` stderr
/// line, never a capturable `LIVE-SMOKE` line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t9944`"]
async fn live_smoke_t9944() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk
        .standalone()
        .token()
        .await
        .expect("OAuth token acquisition failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let req = T9944Request::new();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().index_option_master_codes(&req).await {
        Ok(resp) => {
            assert!(
                !resp.outblock.is_empty(),
                "live-smoke-t9944: empty result (00707) — PENDING, not Implemented"
            );
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record("live-smoke-t9944", &format!("env=paper date={date}"), &line);
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t9944 market-data failure (not evidence)");
            panic!("live-smoke-t9944 failed: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// U5 (reach wave) — F/O quote/master reads. All `/futureoption/market-data`,
// `[선물/옵션] 시세`, non-paginated market_session. Each smoke self-sources a live
// contract code from an F/O master (t8467 index-futures master / t8401
// stock-futures master) so it needs no hardcoded contract code; the example
// codes in the raw capture are stale. One "anytime F/O" probe covers the lane.
// Out-block keys + array-ness were read from the RAW capture (KTD5).
// ---------------------------------------------------------------------------

/// `make live-smoke-t2111`: paper guard → token → fetch one index-futures contract
/// via `t8467` → one `t2111` F/O current-price read for that contract.
///
/// `focode` is public contract reference data (printed); credential-free, self-dated.
/// A failure emits SMOKE-FAIL, never a LIVE-SMOKE line.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t2111`"]
async fn live_smoke_t2111() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let masters = sdk
        .market_session()
        .index_futures_master(&T8467Request::new("Q"))
        .await
        .expect("t8467 index-futures master (contract source) failed");
    if masters.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t2111 t8467 contract source empty (rsp_cd={})",
            masters.rsp_cd
        );
        panic!("live-smoke-t2111: no contract to key the read");
    }
    let focode = masters.outblock[0].shcode.clone();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().fo_quote(&T2111Request::new(&focode)).await {
        Ok(resp) => {
            if resp.outblock.price.is_empty() && resp.outblock.focode.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t2111 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-t2111: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "quote")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t2111",
                &format!("env=paper focode={focode} date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t2111 market-data failure (not evidence)");
            panic!("live-smoke-t2111 failed: {e}");
        }
    }
}

/// `make live-smoke-t2112`: paper guard → token → fetch one index-futures contract
/// via `t8467` → one `t2112` F/O order-book read for that contract.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t2112`"]
async fn live_smoke_t2112() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let masters = sdk
        .market_session()
        .index_futures_master(&T8467Request::new("Q"))
        .await
        .expect("t8467 index-futures master (contract source) failed");
    if masters.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t2112 t8467 contract source empty (rsp_cd={})",
            masters.rsp_cd
        );
        panic!("live-smoke-t2112: no contract to key the read");
    }
    let shcode = masters.outblock[0].shcode.clone();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().fo_order_book(&T2112Request::new(&shcode)).await {
        Ok(resp) => {
            if resp.outblock.price.is_empty() && resp.outblock.shcode.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t2112 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-t2112: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "book")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t2112",
                &format!("env=paper shcode={shcode} date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t2112 market-data failure (not evidence)");
            panic!("live-smoke-t2112 failed: {e}");
        }
    }
}

/// `make live-smoke-t2106`: paper guard → token → fetch one index-futures contract
/// via `t8467` → one `t2106` F/O price-memo read for that contract.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t2106`"]
async fn live_smoke_t2106() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let masters = sdk
        .market_session()
        .index_futures_master(&T8467Request::new("Q"))
        .await
        .expect("t8467 index-futures master (contract source) failed");
    if masters.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t2106 t8467 contract source empty (rsp_cd={})",
            masters.rsp_cd
        );
        panic!("live-smoke-t2106: no contract to key the read");
    }
    let code = masters.outblock[0].shcode.clone();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().fo_price_memo(&T2106Request::new(&code)).await {
        Ok(resp) => {
            if resp.outblock1.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t2106 empty memo array (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-t2106: empty memo array (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "memos")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t2106",
                &format!("env=paper code={code} date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t2106 market-data failure (not evidence)");
            panic!("live-smoke-t2106 failed: {e}");
        }
    }
}

/// `make live-smoke-t8402`: paper guard → token → fetch one stock-futures contract
/// via `t8401` → one `t8402` stock-futures current-price read for that contract.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8402`"]
async fn live_smoke_t8402() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let masters = sdk
        .market_session()
        .stock_futures_master(&T8401Request::new())
        .await
        .expect("t8401 stock-futures master (contract source) failed");
    if masters.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t8402 t8401 contract source empty (rsp_cd={})",
            masters.rsp_cd
        );
        panic!("live-smoke-t8402: no contract to key the read");
    }
    let focode = masters.outblock[0].shcode.clone();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().stock_futures_quote(&T8402Request::new(&focode)).await {
        Ok(resp) => {
            if resp.outblock.price.is_empty() && resp.outblock.hname.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t8402 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-t8402: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "quote")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8402",
                &format!("env=paper focode={focode} date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8402 market-data failure (not evidence)");
            panic!("live-smoke-t8402 failed: {e}");
        }
    }
}

/// `make live-smoke-t8403`: paper guard → token → fetch one stock-futures contract
/// via `t8401` → one `t8403` stock-futures order-book read for that contract.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8403`"]
async fn live_smoke_t8403() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let masters = sdk
        .market_session()
        .stock_futures_master(&T8401Request::new())
        .await
        .expect("t8401 stock-futures master (contract source) failed");
    if masters.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t8403 t8401 contract source empty (rsp_cd={})",
            masters.rsp_cd
        );
        panic!("live-smoke-t8403: no contract to key the read");
    }
    let shcode = masters.outblock[0].shcode.clone();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().stock_futures_order_book(&T8403Request::new(&shcode)).await {
        Ok(resp) => {
            if resp.outblock.price.is_empty() && resp.outblock.hname.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t8403 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-t8403: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "book")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8403",
                &format!("env=paper shcode={shcode} date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8403 market-data failure (not evidence)");
            panic!("live-smoke-t8403 failed: {e}");
        }
    }
}

/// `make live-smoke-t8434`: paper guard → token → fetch one index-futures contract
/// via `t8467` → one `t8434` F/O multi current-price read (qrycnt=1) for it.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8434`"]
async fn live_smoke_t8434() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let masters = sdk
        .market_session()
        .index_futures_master(&T8467Request::new("Q"))
        .await
        .expect("t8467 index-futures master (contract source) failed");
    if masters.outblock.is_empty() {
        eprintln!(
            "SMOKE-FAIL target=live-smoke-t8434 t8467 contract source empty (rsp_cd={})",
            masters.rsp_cd
        );
        panic!("live-smoke-t8434: no contract to key the read");
    }
    let focode = masters.outblock[0].shcode.clone();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().fo_multi_quote(&T8434Request::new("1", &focode)).await {
        Ok(resp) => {
            if resp.outblock1.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t8434 empty result array (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-t8434: empty result array (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8434",
                &format!("env=paper qrycnt=1 focode={focode} date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8434 market-data failure (not evidence)");
            panic!("live-smoke-t8434 failed: {e}");
        }
    }
}

/// `make live-smoke-t1988`: paper guard → token → one ELW underlying-asset list
/// read (all markets, filters off). Routes through `market_session` (KTD3).
/// Numeric request fields `from_rate`/`to_rate` serialize as JSON numbers (KTD4),
/// the prior IGW40011 wire-type fix.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t1988`"]
async fn live_smoke_t1988() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().elw_underlying_list(&T1988Request::new("0")).await {
        Ok(resp) => {
            if resp.outblock.ksp_cnt.is_empty() && resp.outblock1.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t1988 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-t1988: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "assets")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t1988",
                &format!("env=paper mkt_gb=0 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t1988 market-data failure (not evidence)");
            panic!("live-smoke-t1988 failed: {e}");
        }
    }
}

/// `make live-smoke-t3320`: paper guard → token → one FnGuide company-summary
/// read keyed by a public FnGuide company code (`A005930` = 삼성전자). Routes
/// through `market_session` (KTD3).
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t3320`"]
async fn live_smoke_t3320() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let gicode = "005930";
    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().company_summary(&T3320Request::new(gicode)).await {
        Ok(resp) => {
            if resp.outblock.company.is_empty() && resp.outblock1.gicode.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t3320 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-t3320: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "summary")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t3320",
                &format!("env=paper gicode={gicode} date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t3320 market-data failure (not evidence)");
            panic!("live-smoke-t3320 failed: {e}");
        }
    }
}

/// `make live-smoke-t8455`: paper guard → token → one KRX night-derivatives
/// master read (`gubun="NF"` 야간선물). `venue_session: krx_extended` (KTD7) — the
/// night session is ~18:00–05:00 KST, NOT the regular clock; an off-window empty
/// result is NOT a valid attempt (re-run in-window, do NOT flip, do NOT DROP). A
/// definitive `01900` is paper-incompatible regardless of window.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8455`"]
async fn live_smoke_t8455() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk.market_session().night_derivatives_master(&T8455Request::new("NF")).await {
        Ok(resp) => {
            if resp.outblock.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t8455 empty master array (rsp_cd={}) — night window closed? re-run ~18:00–05:00 KST",
                    resp.rsp_cd
                );
                panic!("live-smoke-t8455: empty master array (00707) — off-window/PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8455",
                &format!("env=paper gubun=NF date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8455 market-data failure (not evidence)");
            panic!("live-smoke-t8455 failed: {e}");
        }
    }
}

/// `make live-smoke-t8460`: paper guard → token → one KRX night-derivatives
/// option-board read (`gubun="G"` 원지수, near contract month). `venue_session:
/// krx_extended` (KTD7) — off-window empty is a re-run, not a flip/DROP.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8460`"]
async fn live_smoke_t8460() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let now = Utc::now();
    let yyyymm = now.format("%Y%m").to_string();
    let date = now.format("%Y-%m-%d");
    match sdk.market_session().night_option_board(&T8460Request::new(&yyyymm, "G")).await {
        Ok(resp) => {
            let rows = resp.outblock1.len() + resp.outblock2.len();
            if rows == 0 && resp.outblock.gmprice.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t8460 empty board (rsp_cd={}) — night window closed? re-run ~18:00–05:00 KST",
                    resp.rsp_cd
                );
                panic!("live-smoke-t8460: empty board (00707) — off-window/PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), rows)), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8460",
                &format!("env=paper yyyymm={yyyymm} gubun=G date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8460 market-data failure (not evidence)");
            panic!("live-smoke-t8460 failed: {e}");
        }
    }
}

/// `make live-smoke-t8463`: paper guard → token → one KRX night-derivatives
/// investor-by-timeslot read (`tm_rng="N"` 야간, `fot_clsf_cd="F"` 선물,
/// `bsc_asts_id="101"` KOSPI200). `venue_session: krx_extended` (KTD7) —
/// off-window empty is a re-run, not a flip/DROP.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-t8463`"]
async fn live_smoke_t8463() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .night_investor_timeslot(&T8463Request::new("N", "F", "101"))
        .await
    {
        Ok(resp) => {
            if resp.outblock1.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-t8463 empty time-series array (rsp_cd={}) — night window closed? re-run ~18:00–05:00 KST",
                    resp.rsp_cd
                );
                panic!("live-smoke-t8463: empty time-series array (00707) — off-window/PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-t8463",
                &format!("env=paper tm_rng=N fot_clsf_cd=F bsc_asts_id=101 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-t8463 market-data failure (not evidence)");
            panic!("live-smoke-t8463 failed: {e}");
        }
    }
}

/// `make live-smoke-g3101`: paper guard → token → one overseas current-price
/// read keyed by a public US ticker (`82`/`TSLA` = TSLA on NASDAQ). Domain
/// `overseas_stock`, routes through `market_session` (KTD3). An empty out-block
/// (`price` empty) is the `00707` PENDING case, not Implemented.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-g3101`"]
async fn live_smoke_g3101() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_quote(&G3101Request::new("R", "82TSLA", "82", "TSLA"))
        .await
    {
        Ok(resp) => {
            if resp.outblock.price.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-g3101 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-g3101: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "quote")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-g3101",
                &format!("env=paper exchcd=82 symbol=TSLA date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-g3101 market-data failure (not evidence)");
            panic!("live-smoke-g3101 failed: {e}");
        }
    }
}

/// `make live-smoke-g3104`: paper guard → token → one overseas stock-info master
/// read (`82`/`TSLA`). Routes through `market_session` (KTD3). Empty `korname`
/// out-block is the `00707` PENDING case.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-g3104`"]
async fn live_smoke_g3104() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_stock_info(&G3104Request::new("R", "82TSLA", "82", "TSLA"))
        .await
    {
        Ok(resp) => {
            if resp.outblock.korname.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-g3104 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-g3104: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "master")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-g3104",
                &format!("env=paper exchcd=82 symbol=TSLA date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-g3104 market-data failure (not evidence)");
            panic!("live-smoke-g3104 failed: {e}");
        }
    }
}

/// `make live-smoke-g3106`: paper guard → token → one overseas current-price +
/// order-book read (`82`/`TSLA`). Routes through `market_session` (KTD3). Empty
/// `price` out-block is the `00707` PENDING case.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-g3106`"]
async fn live_smoke_g3106() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_order_book(&G3106Request::new("R", "82TSLA", "82", "TSLA"))
        .await
    {
        Ok(resp) => {
            if resp.outblock.price.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-g3106 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-g3106: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "book")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-g3106",
                &format!("env=paper exchcd=82 symbol=TSLA date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-g3106 market-data failure (not evidence)");
            panic!("live-smoke-g3106 failed: {e}");
        }
    }
}

/// `make live-smoke-g3102`: paper guard → token → one overseas time-series read
/// (`82`/`TSLA`, 30 rows, first page). `readcnt`/`cts_seq` serialize as JSON
/// numbers (KTD4). Empty row array is the `00707` PENDING case.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-g3102`"]
async fn live_smoke_g3102() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_time_series(&G3102Request::new("R", "82TSLA", "82", "TSLA", "30", "0"))
        .await
    {
        Ok(resp) => {
            if resp.outblock1.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-g3102 empty result array (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-g3102: empty result array (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "ticks")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-g3102",
                &format!("env=paper exchcd=82 symbol=TSLA readcnt=30 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-g3102 market-data failure (not evidence)");
            panic!("live-smoke-g3102 failed: {e}");
        }
    }
}

/// `make live-smoke-g3103`: paper guard → token → one overseas period-chart read
/// (`82`/`TSLA`, monthly `gubun="4"`). Empty bar array is the `00707` PENDING
/// case. `date` is the reference date; the public ticker keys the read.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-g3103`"]
async fn live_smoke_g3103() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let today = Utc::now().format("%Y%m%d").to_string();
    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_period_chart(&G3103Request::new("R", "82TSLA", "82", "TSLA", "4", &today))
        .await
    {
        Ok(resp) => {
            if resp.outblock1.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-g3103 empty result array (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-g3103: empty result array (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "bars")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-g3103",
                &format!("env=paper exchcd=82 symbol=TSLA gubun=4 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-g3103 market-data failure (not evidence)");
            panic!("live-smoke-g3103 failed: {e}");
        }
    }
}

/// `make live-smoke-g3190`: paper guard → token → one overseas master-list read
/// (US, exchange `2`, 10 rows, first page). `readcnt` serializes as a JSON
/// number (KTD4). Empty row array is the `00707` PENDING case.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-g3190`"]
async fn live_smoke_g3190() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_master(&G3190Request::new("R", "US", "2", "10", ""))
        .await
    {
        Ok(resp) => {
            if resp.outblock1.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-g3190 empty result array (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-g3190: empty result array (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock1.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-g3190",
                &format!("env=paper natcode=US exgubun=2 readcnt=10 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-g3190 market-data failure (not evidence)");
            panic!("live-smoke-g3190 failed: {e}");
        }
    }
}

/// `make live-smoke-o3101`: paper guard → token → one overseas-futures master
/// read (`gubun=""` = all). Domain `overseas_futures`, routes through
/// `market_session` (KTD3). Empty row array is the `00707` PENDING case, not
/// Implemented.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3101`"]
async fn live_smoke_o3101() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_futures_master(&O3101Request::new(""))
        .await
    {
        Ok(resp) => {
            if resp.outblock.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-o3101 empty result array (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-o3101: empty result array (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-o3101",
                &format!("env=paper gubun=all date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3101 market-data failure (not evidence)");
            panic!("live-smoke-o3101 failed: {e}");
        }
    }
}

/// `make live-smoke-o3121`: paper guard → token → one overseas-future-option
/// master read (`MktGb="O"` = option, all base products). Routes through
/// `market_session` (KTD3). Empty row array is the `00707` PENDING case.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3121`"]
async fn live_smoke_o3121() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_option_master(&O3121Request::new("O", ""))
        .await
    {
        Ok(resp) => {
            if resp.outblock.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-o3121 empty result array (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-o3121: empty result array (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), resp.outblock.len())), "rows")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-o3121",
                &format!("env=paper mktgb=O date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3121 market-data failure (not evidence)");
            panic!("live-smoke-o3121 failed: {e}");
        }
    }
}

/// `make live-smoke-o3105`: paper guard → token → one overseas-futures
/// current-price read keyed by a public symbol (`CUSN23`). Routes through
/// `market_session` (KTD3). Empty `trd_p` out-block is the `00707` PENDING case.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3105`"]
async fn live_smoke_o3105() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_futures_quote(&O3105Request::new("CUSN23  "))
        .await
    {
        Ok(resp) => {
            if resp.outblock.trd_p.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-o3105 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-o3105: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "quote")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-o3105",
                &format!("env=paper symbol=CUSN23 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3105 market-data failure (not evidence)");
            panic!("live-smoke-o3105 failed: {e}");
        }
    }
}

/// `make live-smoke-o3106`: paper guard → token → one overseas-futures
/// current-price + order-book read (`ADM23`). Routes through `market_session`
/// (KTD3). Empty `price` out-block is the `00707` PENDING case.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3106`"]
async fn live_smoke_o3106() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_futures_order_book(&O3106Request::new("ADM23"))
        .await
    {
        Ok(resp) => {
            if resp.outblock.price.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-o3106 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-o3106: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "book")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-o3106",
                &format!("env=paper symbol=ADM23 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3106 market-data failure (not evidence)");
            panic!("live-smoke-o3106 failed: {e}");
        }
    }
}

/// `make live-smoke-o3125`: paper guard → token → one overseas-future-option
/// current-price read (`mktgb="F"`, `HSIM23`). Routes through `market_session`
/// (KTD3). Empty `trd_p` out-block is the `00707` PENDING case.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3125`"]
async fn live_smoke_o3125() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_option_quote(&O3125Request::new("F", "HSIM23          "))
        .await
    {
        Ok(resp) => {
            if resp.outblock.trd_p.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-o3125 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-o3125: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "quote")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-o3125",
                &format!("env=paper mktgb=F symbol=HSIM23 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3125 market-data failure (not evidence)");
            panic!("live-smoke-o3125 failed: {e}");
        }
    }
}

/// `make live-smoke-o3126`: paper guard → token → one overseas-future-option
/// current-price + order-book read (`mktgb="F"`, `ADM23`). Routes through
/// `market_session` (KTD3). Empty `price` out-block is the `00707` PENDING case.
#[tokio::test]
#[ignore = "live smoke: needs real LS paper credentials; run via `make live-smoke-o3126`"]
async fn live_smoke_o3126() {
    let sdk = paper_sdk().expect("paper guard + config must succeed for a paper run");
    let token = sdk.standalone().token().await.expect("OAuth token failed");
    assert!(!token.is_empty(), "token must be non-empty");

    let date = Utc::now().format("%Y-%m-%d");
    match sdk
        .market_session()
        .overseas_option_order_book(&O3126Request::new("F", "ADM23"))
        .await
    {
        Ok(resp) => {
            if resp.outblock.price.is_empty() {
                eprintln!(
                    "SMOKE-FAIL target=live-smoke-o3126 empty out-block (rsp_cd={})",
                    resp.rsp_cd
                );
                panic!("live-smoke-o3126: empty out-block (00707) — PENDING, not Implemented");
            }
            let line = smoke_result(Ok((resp.rsp_cd.clone(), 1)), "book")
                .expect("an Ok outcome yields a result line");
            record(
                "live-smoke-o3126",
                &format!("env=paper mktgb=F symbol=ADM23 date={date}"),
                &line,
            );
        }
        Err(e) => {
            eprintln!("SMOKE-FAIL target=live-smoke-o3126 market-data failure (not evidence)");
            panic!("live-smoke-o3126 failed: {e}");
        }
    }
}
