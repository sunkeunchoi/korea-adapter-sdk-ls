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
    CSPAQ12300Request, CSPAQ22200Request, CSPBQ00200Request, CLNAQ00100Request, CFOEQ11100Request,
    T0424Request, T0441Request, CIDBQ01400Request, CIDBQ03000Request, CIDBQ05300Request,
};
use ls_sdk::market_session::{
    T1101Request, T1102Request, T1485Request, T1511Request, T1516Request, T1531Request,
    T1537Request, T1601Request, T1615Request, T1640Request, T1662Request, T1664Request,
    T1104Request, T1105Request, T1825Request, T1826Request, T1859Request, T1901Request,
    T1906Request, T8450Request, T1638Request, T1308Request, T1449Request, T1621Request, T2545Request, T8406Request, T8407Request, T1631Request, T1632Request, T1633Request, T1716Request, T1902Request, T1904Request, T1927Request, T1941Request, T1702Request, T1717Request, T1665Request, T1471Request, T1475Request, T1959Request, T1950Request, T1954Request, T1971Request, T1972Request, T1974Request, T1956Request, T1969Request,
    T1302Request, T2216Request,
    T1532Request, T1533Request, T1926Request, T1764Request, T1903Request,
    T1958Request, T1964Request, T2301Request,
    T2522Request, T8401Request, T8424Request, T8425Request, T8426Request, T8433Request,
    T8435Request, T8467Request, T9943Request, T9944Request,
    T8430Request,
    T8431Request,
    T8436Request,
    T9905Request, T9907Request, T9942Request,
    T2106Request, T2111Request, T2112Request, T8402Request, T8403Request, T8434Request,
    T1988Request, T3320Request,
    T8455Request, T8460Request, T8463Request,
    G3101Request, G3102Request, G3103Request, G3104Request, G3106Request, G3190Request,
    O3101Request, O3105Request, O3106Request, O3121Request, O3125Request, O3126Request,
    O3104Request, O3127Request, T8462Request,
    T8427Request, T2210Request, T2424Request, T8428Request,
    T9945Request, T3202Request, T3521Request,
    T0167Request, T3102Request,
};
use ls_sdk::paginated::{
    T1403Request, T1441Request, T1452Request, T1463Request, T1466Request, T1481Request,
    T1482Request, T1489Request, T1492Request, T1514Request, T1866Request, T3341Request,
    T8412Request,
    T1305Request, T8410Request, T8451Request, T8419Request, T4203Request, T3401Request,
    T1310Request, T1404Request, T1410Request, T1411Request, T1488Request, T1636Request,
    T1809Request,
    T1109Request, T1301Request, T1486Request, T8454Request, T1637Request,
    T1602Request, T1603Request, T1617Request, T1752Request, T1771Request,
    T8417Request, T8418Request, T8411Request, T8452Request, T8453Request,
    T8464Request, T8465Request, T8466Request, T8405Request,
    T1444Request, T1422Request, T1427Request, T1442Request, T1405Request, T1960Request, T1961Request, T1966Request, T1921Request,
    T3518Request,
    O3103Request, O3108Request, O3116Request, O3117Request, O3123Request, O3128Request,
    O3136Request, O3137Request, O3139Request,
    T2541Request, T2214Request,
};
use ls_sdk_test_support::{assert_nonempty_witness, scrub_secrets};
use ls_sdk::realtime::{NwsRow, WsLane};
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

/// A `string_or_number`-decoded field holds a substantive (non-default) value: a
/// non-empty string that is not the zero default. The R5/KTD5 witness predicate the
/// account smokes report.
fn is_non_default_str(s: &str) -> bool {
    !s.is_empty() && s != "0"
}

/// Install a process-global tracing subscriber that DROPS the `ls_core` dispatch
/// debug events (KTD7). The dispatch path logs `rsp_msg` and the whole response
/// body on error paths — broker text that carries account-identifying content the
/// field-level smoke scrubbing never sees. For an AUTONOMOUS account read (this
/// wave runs the smokes in-session, not via an operator) these are suppressed
/// entirely. FAIL-CLOSED: `tracing` allows exactly one global default per process,
/// so if a foreign subscriber is already installed we cannot guarantee suppression
/// — we refuse the run rather than fail OPEN on a known leak. Each account smoke
/// installs this before its first dispatch; `make live-smoke-<tr>` runs exactly one
/// test per process (`--ignored --exact`), so the global install always succeeds.
fn install_dispatch_log_suppressor() -> LsResult<()> {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::new("error,ls_core=off");
    let subscriber = tracing_subscriber::fmt().with_env_filter(filter).finish();
    tracing::subscriber::set_global_default(subscriber).map_err(|_| {
        LsError::Config(
            "refusing autonomous account read: a foreign global tracing subscriber is already \
             installed, so the unscrubbed ls_core dispatch debug log cannot be guaranteed \
             suppressed (KTD7) — failing closed rather than risking an account-number leak"
                .into(),
        )
    })
}

/// KTD7: the dispatch-log suppressor's filter DROPS `ls_core` debug events, so an
/// errored account dispatch (which the `ls_core` path logs at debug with `rsp_msg`
/// + the raw body) leaks no account number into captured logs. Uses a thread-local
/// subscriber (`with_default`) so it neither claims nor needs the process-global
/// default — proving the FILTER, the mechanism the autonomous smokes rely on.
#[test]
fn dispatch_log_suppressor_drops_ls_core_account_events() {
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;
    use tracing_subscriber::EnvFilter;

    #[derive(Clone)]
    struct CapWriter(Arc<Mutex<Vec<u8>>>);
    impl Write for CapWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> MakeWriter<'a> for CapWriter {
        type Writer = CapWriter;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("error,ls_core=off"))
        .with_writer(CapWriter(buf.clone()))
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        // Mirror the ls_core dispatch error-path events: an account-bearing rsp_msg
        // and the raw body, logged at debug on the `ls_core` target.
        tracing::debug!(target: "ls_core", rsp_msg = "계좌 12345678-01 거부", "dispatch error");
        tracing::debug!(target: "ls_core", body = "{\"AcntNo\":\"12345678\"}", "raw body");
    });

    let captured = String::from_utf8(buf.lock().unwrap().clone()).expect("utf8");
    assert!(
        captured.is_empty(),
        "ls_core debug events must be fully suppressed, got: {captured}"
    );
    assert!(
        !captured.contains("12345678"),
        "no account number may reach captured logs"
    );
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

// ---------------------------------------------------------------------------
// U3/U6 — WebSocket lifecycle smoke (generic helper + S3_ + negative control)
// ---------------------------------------------------------------------------

/// GENERIC WS lifecycle smoke, parameterized by `(tr_cd, tr_key, lane)` — the
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
async fn ws_lifecycle_smoke(tr_cd: &str, tr_key: &str, lane: WsLane) -> String {
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
        .subscribe_typed::<WsLifecycleRow>(tr_cd, tr_key, lane)
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

/// Non-panicking sibling of [`ws_lifecycle_smoke`] for the combined P1 wave run.
///
/// Wraps the FULL lifecycle (paper guard → port-29443 assertion → fresh-manager
/// subscribe → timeboxed bonus row → unsubscribe) in a `Result`, so a single bad
/// TR records its failure as a line and does NOT panic-abort the whole 14-TR
/// sweep (the resilience requirement of `live_smoke_ws_p1`). Mirrors
/// `ws_lifecycle_smoke` step-for-step; `ws_lifecycle_smoke`'s own callers are
/// untouched (they still want the fail-fast panic). NO raw-frame logging.
async fn ws_lifecycle_try(tr_cd: &str, tr_key: &str, lane: WsLane) -> Result<String, String> {
    paper_guard().map_err(|e| format!("paper guard failed: {e}"))?;
    let config = LsConfig::from_env().map_err(|e| format!("config from env failed: {e}"))?;
    if !config.environment.is_paper() {
        return Err("resolved environment is not Paper".to_string());
    }

    let ws_url = ls_core::config::Environment::resolve_ws_url(&config);
    if !ws_url.contains("29443") {
        return Err("resolved WS URL is not the paper port 29443".to_string());
    }

    // Fresh SDK → fresh, isolated WsManager (KTD2 — no shared-manager poisoning).
    let sdk = LsSdk::new(config).map_err(|e| format!("sdk construction failed: {e}"))?;
    let ws = sdk.realtime();

    let (handle, mut stream) = ws
        .subscribe_typed::<WsLifecycleRow>(tr_cd, tr_key, lane)
        .await
        .map_err(|e| format!("subscribe/lifecycle failed: {e}"))?;

    // BONUS: a row may or may not arrive; absence is not a failure.
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
        .map_err(|e| format!("unsubscribe failed: {e}"))?;

    Ok(row_note)
}

/// Offline (U2 / R12): the P3 sweep's Err branch records only a STRUCTURAL note —
/// no response body, `rsp_msg`, account number, or token. `ws_lifecycle_try`'s
/// error strings are transport/port-level only; this asserts the formatted
/// `LIFECYCLE-FAIL` note a failed P3 leg would emit carries no payload.
#[test]
fn ws_p3_failure_note_carries_no_payload() {
    let err = "resolved WS URL is not the paper port 29443".to_string();
    let note = format!("LIFECYCLE-FAIL: {err}");
    assert!(note.starts_with("LIFECYCLE-FAIL:"));
    // No JSON body / rsp_msg / account-shaped digit run leaks through.
    assert!(!note.contains("rsp_msg"));
    assert!(!note.contains("body"));
    assert!(!note.contains("token"));
    assert!(!note.contains("AcntNo"));
}

/// Offline: the P4 sweep's Err branch records only a STRUCTURAL note — no response
/// body, `rsp_msg`, account number, or token. `ws_lifecycle_try`'s error strings
/// are transport/port-level only; this asserts the formatted `LIFECYCLE-FAIL` note
/// a failed P4 leg would emit carries no payload.
#[test]
fn ws_p4_failure_note_carries_no_payload() {
    let err = "resolved WS URL is not the paper port 29443".to_string();
    let note = format!("LIFECYCLE-FAIL: {err}");
    assert!(note.starts_with("LIFECYCLE-FAIL:"));
    // No JSON body / rsp_msg / account-shaped digit run leaks through.
    assert!(!note.contains("rsp_msg"));
    assert!(!note.contains("body"));
    assert!(!note.contains("token"));
    assert!(!note.contains("AcntNo"));
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
// F-O market-data reads (plan -001 open-window flip wave). The contract code is
// self-sourced at runtime from the t8467 index-futures master (front-month codes
// expire — never hard-coded), mirroring live_smoke_t8406/t8407. Each asserts a
// NAMED market-data witness (close/volume/price) is substantive before record().
// ---------------------------------------------------------------------------

/// Fetch the first live front-month F/O contract from the t8467 index-futures
/// master, or emit a credential-safe SMOKE-FAIL and panic. Returns the `shcode`.
async fn fo_front_month_shcode(sdk: &LsSdk, target: &str) -> String {
    let masters = sdk
        .market_session()
        .index_futures_master(&T8467Request::new("Q"))
        .await
        .expect("t8467 index-futures master (contract source) failed");
    if masters.outblock.is_empty() {
        eprintln!("SMOKE-FAIL target={target} t8467 contract source empty (rsp_cd={})", masters.rsp_cd);
        panic!("{target}: no contract to key the read");
    }
    masters.outblock[0].shcode.clone()
}

// --- plan -004 batch B — F/O charts. Each sources a CURRENT front-month contract
//     from a derivatives master (t8467 index-futures / t8401 stock-futures), since
//     stale contract codes return an empty board under closure.

/// Fetch a current index-futures contract code via `t8467` (master) for the F/O
/// chart smokes. Panics with a credential-free SMOKE-FAIL on an empty master.
async fn current_index_future(sdk: &LsSdk, target: &str) -> String {
    let masters = sdk
        .market_session()
        .index_futures_master(&ls_sdk::market_session::T8467Request::new("Q"))
        .await
        .expect("t8467 index-futures master (contract source) failed");
    if masters.outblock.is_empty() {
        eprintln!("SMOKE-FAIL target={target} t8467 contract source empty (rsp_cd={})", masters.rsp_cd);
        panic!("{target}: no contract to key the read");
    }
    masters.outblock[0].shcode.clone()
}

#[path = "live/market_session_price.rs"]
mod market_session_price;
#[path = "live/market_session_charts.rs"]
mod market_session_charts;
#[path = "live/market_session_flow.rs"]
mod market_session_flow;
#[path = "live/market_session_masters.rs"]
mod market_session_masters;
#[path = "live/paginated_boards.rs"]
mod paginated_boards;
#[path = "live/paginated_misc.rs"]
mod paginated_misc;
#[path = "live/account.rs"]
mod account;
#[path = "live/realtime.rs"]
mod realtime;
