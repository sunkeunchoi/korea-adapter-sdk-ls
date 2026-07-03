//! Differential negative probe (error-resilience gate U4, R10, KTD4).
//!
//! Two parts, mirroring the split the plan requires:
//!
//! - An **offline twin** (`negative_probe_offline_twin`, always run) that
//!   exercises invalid-variant *generation* from a constraint schema and the
//!   differential HELD/Clean/Divergent comparator, deterministically and with no
//!   network — the only part in the CI gate. Mirrors the `negative_control`
//!   deterministic twin pattern.
//! - An **operator-run live probe** (`live_smoke_t8412_negative`, `#[ignore]`)
//!   that runs a valid control plus each mechanically-generated invalid variant
//!   against the REAL paper gateway in the same session, classifies each result,
//!   and prints a credential-free `NEG-PROBE` line. A valid-control failure
//!   (session-closed / unfunded / stale seed / paper-incompatible) is HELD, not a
//!   divergence. This gates re-promotion to Recommended (U8), never CI.
//!
//! Safety: the live probe calls [`paper_guard`] first (explicit `LS_TRADING_ENV=
//! paper`) and is credential-free by construction — it prints only the HTTP
//! status, business `rsp_cd`, and the injected field/class, never the token,
//! `rsp_msg`, or body content.

use std::time::Duration;

use ls_core::{
    classify_probe, generate_invalid_variants, ConstraintSchema, InvalidVariant, LsConfig, LsError,
    LsResult, ProbeOutcome,
};
use ls_sdk::market_session::T1102Request;
use ls_sdk::orders::{
    CSPAT00601Request, CSPAT00801Request, T0425InBlock, T0425OutBlock1, T0425Request,
};
use ls_sdk::LsSdk;

/// Pre-flight production guard — requires `LS_TRADING_ENV` explicitly `paper`.
fn paper_guard() -> LsResult<()> {
    match std::env::var("LS_TRADING_ENV") {
        Ok(v) if v.eq_ignore_ascii_case("paper") => Ok(()),
        Ok(v) => Err(LsError::Config(format!(
            "negative probe refuses to run: LS_TRADING_ENV must be explicitly 'paper', got '{v}'"
        ))),
        Err(_) => Err(LsError::Config(
            "negative probe refuses to run: LS_TRADING_ENV must be explicitly set to 'paper'".into(),
        )),
    }
}

/// The exemplar constraint schema for the offline twin — the embedded t8412
/// schema, proving the runtime registry and the generator agree.
fn t8412_schema() -> ConstraintSchema {
    ls_core::schema_for("t8412")
        .expect("t8412 carries an embedded constraint schema")
        .clone()
}

/// A valid t8412 InBlock seed (the differential control). Numeric fields are JSON
/// numbers so the control itself is well-formed (a quoted numeric would trip
/// IGW40011 and mask the control).
fn valid_seed() -> serde_json::Value {
    serde_json::json!({
        "shcode": "005930",
        "ncnt": 1,
        "qrycnt": 20,
        "nday": "1",
        "sdate": "20260601",
        "edate": "20260605",
        "cts_date": "",
        "cts_time": "",
        "comp_yn": "N"
    })
}

#[test]
fn negative_probe_offline_twin() {
    // Generation covers every declared class; each variant genuinely violates the
    // schema (checked with the class confirmed); the comparator classifies the
    // three outcomes. No network, deterministic.
    let schema = t8412_schema();
    let seed = valid_seed();
    let variants = generate_invalid_variants(&schema, &seed);
    assert!(
        !variants.is_empty(),
        "the exemplar schema must yield invalid variants"
    );

    // Every declared field/class shows up as a variant.
    let generated: std::collections::BTreeSet<(String, String)> = variants
        .iter()
        .map(|v| (v.field.clone(), v.class.clone()))
        .collect();
    assert!(generated.contains(&("shcode".into(), "required".into())));
    assert!(generated.contains(&("shcode".into(), "format".into())));
    assert!(generated.contains(&("ncnt".into(), "type".into())));
    assert!(generated.contains(&("nday".into(), "enum".into())));
    assert!(generated.contains(&("sdate".into(), "format".into())));
    assert!(generated.contains(&("sdate/edate".into(), "cross_field".into())));

    // Determinism: regenerating yields an identical sequence.
    let again = generate_invalid_variants(&schema, &seed);
    assert_eq!(variants, again, "variant generation is deterministic");

    // Differential comparator (AE2).
    assert_eq!(classify_probe(false, true), ProbeOutcome::Held);
    assert_eq!(classify_probe(true, true), ProbeOutcome::Clean);
    assert_eq!(classify_probe(true, false), ProbeOutcome::Divergent);
}

/// `true` if a gateway response classifies as a read success (control passes).
fn is_success(rsp_cd: &str) -> bool {
    matches!(rsp_cd, "" | "00000" | "00136" | "00707")
}

#[tokio::test]
#[ignore = "live probe: needs real LS paper credentials + in-window session; run via `make live-smoke-t8412-negative`"]
async fn live_smoke_t8412_negative() {
    paper_guard().expect("paper guard must pass");
    let config = LsConfig::from_env().expect("config from env");
    let sdk = LsSdk::new(config.clone()).expect("sdk builds");

    let token = match sdk.standalone().token().await {
        Ok(t) if !t.is_empty() => t,
        _ => {
            eprintln!("NEG-PROBE-FAIL target=t8412-negative token acquisition failed (not evidence)");
            panic!("negative probe could not acquire an OAuth token");
        }
    };

    let base = ls_core::config::Environment::resolve_base_url(&config);
    let url = format!("{base}/stock/chart");
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .expect("probe client builds");

    // Fire one raw t8412 request. Returns `Some((http_status, rsp_cd))` when the
    // gateway ANSWERED, or `None` on a transport failure (timeout / connection /
    // body-read error) — never rsp_msg or body content. A transport failure is
    // NOT a gateway rejection: collapsing it to a rejection would let a network
    // blip on an invalid variant print a false CLEAN and certify a constraint the
    // gateway never actually enforced.
    async fn fire(
        client: &reqwest::Client,
        url: &str,
        token: &str,
        inblock: &serde_json::Value,
    ) -> Option<(u16, String)> {
        let body = serde_json::json!({ "t8412InBlock": inblock }).to_string();
        let resp = client
            .post(url)
            .bearer_auth(token)
            .header("tr_cd", "t8412")
            .header("tr_cont", "N")
            .header("tr_cont_key", "")
            .header("content-type", "application/json; charset=utf-8")
            .body(body)
            .send()
            .await
            .ok()?;
        let status = resp.status().as_u16();
        let text = resp.text().await.ok()?;
        let rsp_cd = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("rsp_cd").and_then(|c| c.as_str()).map(String::from))
            .unwrap_or_default();
        Some((status, rsp_cd))
    }

    let responded_ok = |r: &Option<(u16, String)>| {
        matches!(r, Some((http, cd)) if *http >= 200 && *http < 300 && is_success(cd))
    };

    let seed = valid_seed();
    let schema = t8412_schema();

    // Valid control, same session.
    let control = fire(&client, &url, &token, &seed).await;
    let control_ok = responded_ok(&control);
    match &control {
        Some((http, cd)) => println!(
            "NEG-PROBE target=t8412-negative control=[http={http} rsp_cd={cd} ok={control_ok}]"
        ),
        None => println!("NEG-PROBE target=t8412-negative control=[transport-failure ok=false]"),
    }

    // Each mechanically-generated invalid variant.
    for variant in generate_invalid_variants(&schema, &seed) {
        let field = &variant.field;
        let class = &variant.class;
        match fire(&client, &url, &token, &variant.request).await {
            Some((http, rsp_cd)) => {
                // The gateway answered: a non-success response is a rejection.
                let variant_rejected = !(http >= 200 && http < 300 && is_success(&rsp_cd));
                let outcome = classify_probe(control_ok, variant_rejected);
                println!(
                    "NEG-PROBE target=t8412-negative variant field={field} class={class} result=[http={http} rsp_cd={rsp_cd}] outcome={outcome:?}"
                );
            }
            None => {
                // Transport failure on the variant: inconclusive, NOT a rejection.
                // Never certify (CLEAN) a constraint the gateway never judged.
                println!(
                    "NEG-PROBE target=t8412-negative variant field={field} class={class} result=[transport-failure] outcome=Held"
                );
            }
        }
    }

    if !control_ok {
        eprintln!(
            "NEG-PROBE target=t8412-negative HELD: valid control failed \
             (session-closed / stale seed / env / transport) — inconclusive, not a divergence"
        );
    }
}

// ===========================================================================
// Shared probe plumbing (extends the t8412 exemplar to every InBlock TR)
// ===========================================================================

/// A credential-free probe HTTP client with the same bounded timeouts as the
/// t8412 exemplar's inline client.
fn probe_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .expect("probe client builds")
}

/// Fire one raw InBlock request for `tr_cd` at `url`, wrapping `inblock` under
/// `inblock_key` (`{"<TR>InBlock…": inblock}`) — the parametrized generalization
/// of the t8412 exemplar's inline `fire`. Returns `Some((http_status, rsp_cd))`
/// when the gateway ANSWERED, or `None` on a transport failure (timeout /
/// connection / body-read error). A transport failure is NOT a rejection —
/// collapsing it would let a network blip print a false CLEAN. Never emits
/// `rsp_msg` or body content.
async fn fire_inblock(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    tr_cd: &str,
    inblock_key: &str,
    inblock: &serde_json::Value,
) -> Option<(u16, String)> {
    let mut wrapper = serde_json::Map::new();
    wrapper.insert(inblock_key.to_string(), inblock.clone());
    let body = serde_json::Value::Object(wrapper).to_string();
    let resp = client
        .post(url)
        .bearer_auth(token)
        .header("tr_cd", tr_cd)
        .header("tr_cont", "N")
        .header("tr_cont_key", "")
        .header("content-type", "application/json; charset=utf-8")
        .body(body)
        .send()
        .await
        .ok()?;
    let status = resp.status().as_u16();
    let text = resp.text().await.ok()?;
    let rsp_cd = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v.get("rsp_cd").and_then(|c| c.as_str()).map(String::from))
        .unwrap_or_default();
    Some((status, rsp_cd))
}

/// Run the differential negative probe for a READ InBlock TR: fire the valid
/// control plus EVERY mechanically-generated invalid variant (like the t8412
/// exemplar) against the live paper gateway, classify each, and print
/// credential-free `NEG-PROBE target=<tr>-negative …` lines. A valid-control
/// failure is HELD (session-closed / stale seed / env / transport), never a
/// divergence.
async fn run_inblock_negative_probe(
    tr_cd: &str,
    path: &str,
    inblock_key: &str,
    seed: serde_json::Value,
) {
    paper_guard().expect("paper guard must pass");
    let config = LsConfig::from_env().expect("config from env");
    let sdk = LsSdk::new(config.clone()).expect("sdk builds");
    let schema = ls_core::schema_for(tr_cd)
        .unwrap_or_else(|| panic!("{tr_cd} carries an embedded constraint schema"))
        .clone();

    let token = match sdk.standalone().token().await {
        Ok(t) if !t.is_empty() => t,
        _ => {
            eprintln!(
                "NEG-PROBE-FAIL target={tr_cd}-negative token acquisition failed (not evidence)"
            );
            panic!("negative probe could not acquire an OAuth token");
        }
    };

    let base = ls_core::config::Environment::resolve_base_url(&config);
    let url = format!("{base}{path}");
    let client = probe_client();

    // Valid control, same session.
    let control = fire_inblock(&client, &url, &token, tr_cd, inblock_key, &seed).await;
    let control_ok =
        matches!(&control, Some((http, cd)) if *http >= 200 && *http < 300 && is_success(cd));
    match &control {
        Some((http, cd)) => println!(
            "NEG-PROBE target={tr_cd}-negative control=[http={http} rsp_cd={cd} ok={control_ok}]"
        ),
        None => {
            println!("NEG-PROBE target={tr_cd}-negative control=[transport-failure ok=false]")
        }
    }

    for variant in generate_invalid_variants(&schema, &seed) {
        let field = &variant.field;
        let class = &variant.class;
        match fire_inblock(&client, &url, &token, tr_cd, inblock_key, &variant.request).await {
            Some((http, rsp_cd)) => {
                let variant_rejected = !(http >= 200 && http < 300 && is_success(&rsp_cd));
                let outcome = classify_probe(control_ok, variant_rejected);
                println!(
                    "NEG-PROBE target={tr_cd}-negative variant field={field} class={class} \
                     result=[http={http} rsp_cd={rsp_cd}] outcome={outcome:?}"
                );
            }
            None => println!(
                "NEG-PROBE target={tr_cd}-negative variant field={field} class={class} \
                 result=[transport-failure] outcome=Held"
            ),
        }
    }

    if !control_ok {
        eprintln!(
            "NEG-PROBE target={tr_cd}-negative HELD: valid control failed \
             (session-closed / stale seed / env / transport) — inconclusive, not a divergence"
        );
    }
}

// ===========================================================================
// A. Read legs — every generated class (like t8412), one thin wrapper each
// ===========================================================================

#[tokio::test]
#[ignore = "live probe: needs real LS paper credentials + in-window session; run via `make live-smoke-t1101-negative`"]
async fn live_smoke_t1101_negative() {
    run_inblock_negative_probe(
        "t1101",
        "/stock/market-data",
        "t1101InBlock",
        serde_json::json!({ "shcode": "005930" }),
    )
    .await;
}

#[tokio::test]
#[ignore = "live probe: needs real LS paper credentials + in-window session; run via `make live-smoke-t1102-negative`"]
async fn live_smoke_t1102_negative() {
    run_inblock_negative_probe(
        "t1102",
        "/stock/market-data",
        "t1102InBlock",
        serde_json::json!({ "shcode": "005930", "exchgubun": "1" }),
    )
    .await;
}

#[tokio::test]
#[ignore = "live probe: needs real LS paper credentials + in-window session; run via `make live-smoke-cspaq12200-negative`"]
async fn live_smoke_cspaq12200_negative() {
    // BalCreTp "0" is the well-formed control; a live HELD on this value (a funding-
    // dependent account) is reconciled by the operator — offline this never runs.
    run_inblock_negative_probe(
        "CSPAQ12200",
        "/stock/accno",
        "CSPAQ12200InBlock1",
        serde_json::json!({ "BalCreTp": "0" }),
    )
    .await;
}

#[tokio::test]
#[ignore = "live probe: needs real LS paper credentials + in-window session; run via `make live-smoke-t0425-negative`"]
async fn live_smoke_t0425_negative() {
    // t0425 is a READ (is_order:false), so it fires every generated class like t8412.
    run_inblock_negative_probe(
        "t0425",
        "/stock/accno",
        "t0425InBlock",
        serde_json::json!({
            "expcode": "005930", "chegb": "0", "medosu": "0",
            "sortgb": "2", "cts_ordno": " "
        }),
    )
    .await;
}

// ===========================================================================
// B. token leg — bespoke FORM probe against /oauth2/token (KTD2)
// ===========================================================================

/// Fire one `application/x-www-form-urlencoded` (or, for the shape variant, JSON)
/// token request. Returns `Some((http, rsp_cd, ok))` on a gateway answer, `None`
/// on transport failure. `ok` is `true` only for a 2xx that carries a non-empty
/// `access_token`. CREDENTIAL-FREE by construction: the token itself and any
/// localized `rsp_msg` are NEVER returned — only the HTTP status, a business error
/// code (safe), and the success flag.
async fn token_fire(
    client: &reqwest::Client,
    url: &str,
    pairs: &[(String, String)],
    as_json: bool,
) -> Option<(u16, String, bool)> {
    let builder = if as_json {
        let map: serde_json::Map<String, serde_json::Value> = pairs
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect();
        client.post(url).json(&serde_json::Value::Object(map))
    } else {
        let form: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        client.post(url).form(&form)
    };
    let resp = builder.send().await.ok()?;
    let status = resp.status().as_u16();
    let text = resp.text().await.ok()?;
    let val: Option<serde_json::Value> = serde_json::from_str(&text).ok();
    let has_token = val
        .as_ref()
        .and_then(|v| v.get("access_token"))
        .and_then(|t| t.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    // A safe business error code (never the localized message).
    let code = val
        .as_ref()
        .and_then(|v| {
            v.get("rsp_cd")
                .or_else(|| v.get("code"))
                .or_else(|| v.get("error"))
        })
        .and_then(|c| c.as_str())
        .map(String::from)
        .unwrap_or_default();
    let ok = (200..300).contains(&status) && has_token;
    Some((status, code, ok))
}

/// Build a copy of `base` with `field` set to `value`, or removed when `value` is
/// `None` (the credential required-class is tested by REMOVAL only).
fn form_with(base: &[(String, String)], field: &str, value: Option<&str>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = base.iter().filter(|(k, _)| k != field).cloned().collect();
    if let Some(v) = value {
        out.push((field.to_string(), v.to_string()));
    }
    out
}

#[tokio::test]
#[ignore = "live probe: needs real LS paper credentials; run via `make live-smoke-token-negative`. Conceptually LAST — a token-flow probe can disturb the session token; run it after the read/order legs."]
async fn live_smoke_token_negative() {
    paper_guard().expect("paper guard must pass");
    let config = LsConfig::from_env().expect("config from env");
    let base = ls_core::config::Environment::resolve_base_url(&config);
    let url = format!("{base}/oauth2/token");
    let client = probe_client();

    // The full valid control form. `appkey`/`appsecretkey` appear ONLY at their real
    // value (control) or removed (required-by-removal) — never with a mutated value.
    let base_form: Vec<(String, String)> = vec![
        ("grant_type".into(), "client_credentials".into()),
        ("appkey".into(), config.appkey.clone()),
        ("appsecretkey".into(), config.appsecretkey.clone()),
        ("scope".into(), "oob".into()),
    ];

    let control = token_fire(&client, &url, &base_form, false).await;
    let control_ok = matches!(&control, Some((_, _, ok)) if *ok);
    match &control {
        Some((http, cd, ok)) => println!(
            "NEG-PROBE target=token-negative control=[http={http} rsp_cd={cd} ok={ok}]"
        ),
        None => println!("NEG-PROBE target=token-negative control=[transport-failure ok=false]"),
    }

    // Variants: mutate ONLY the two non-credential fields (grant_type, scope), one
    // content-type-shape variant, and the credential required-class by REMOVAL only.
    let variants: Vec<(&str, &str, Vec<(String, String)>, bool)> = vec![
        (
            "grant_type",
            "enum",
            form_with(&base_form, "grant_type", Some("bad_grant")),
            false,
        ),
        (
            "grant_type",
            "required",
            form_with(&base_form, "grant_type", None),
            false,
        ),
        (
            "scope",
            "enum",
            form_with(&base_form, "scope", Some("bad_scope")),
            false,
        ),
        ("scope", "required", form_with(&base_form, "scope", None), false),
        // Content-type shape: send the valid fields as JSON instead of a form.
        ("content-type", "format", base_form.clone(), true),
        // Credential required-class — REMOVAL only (never a mutated secret value).
        ("appkey", "required", form_with(&base_form, "appkey", None), false),
        (
            "appsecretkey",
            "required",
            form_with(&base_form, "appsecretkey", None),
            false,
        ),
    ];

    for (field, class, pairs, as_json) in &variants {
        match token_fire(&client, &url, pairs, *as_json).await {
            Some((http, code, ok)) => {
                let outcome = classify_probe(control_ok, !ok);
                println!(
                    "NEG-PROBE target=token-negative variant field={field} class={class} \
                     result=[http={http} rsp_cd={code}] outcome={outcome:?}"
                );
            }
            None => println!(
                "NEG-PROBE target=token-negative variant field={field} class={class} \
                 result=[transport-failure] outcome=Held"
            ),
        }
    }

    if !control_ok {
        eprintln!(
            "NEG-PROBE target=token-negative HELD: valid control failed — inconclusive, not a \
             divergence"
        );
    }
}

// ===========================================================================
// C. Order legs — KTD3 semantics (fail-closed autonomy + type/required only)
// ===========================================================================
//
// The order-safety helpers below are a FAITHFUL COPY of `tests/order_smoke.rs`
// (each test binary is standalone and cannot import another binary's private
// fns). Kept minimal and credential-free.

/// Order opt-in guard — an EXPLICIT second opt-in beyond the paper guard.
fn order_smoke_guard() -> LsResult<()> {
    paper_guard()?;
    match std::env::var("LS_ORDER_SMOKE") {
        Ok(v) if v == "1" || v.eq_ignore_ascii_case("true") => Ok(()),
        _ => Err(LsError::Config(
            "order negative probe refuses to run: LS_ORDER_SMOKE must be explicitly '1'".into(),
        )),
    }
}

/// Assert the RESOLVED environment is paper after credential load.
fn assert_resolved_paper(env: &ls_core::Environment) -> LsResult<()> {
    if env.is_paper() {
        Ok(())
    } else {
        Err(LsError::Config(format!(
            "order negative probe refuses to place: resolved environment is '{env}', not paper"
        )))
    }
}

/// TTL for a per-wave human-minted nonce (seconds).
const NONCE_TTL_SECS: i64 = 600;
/// Forward-skew tolerance (seconds) for a nonce timestamp.
const NONCE_MAX_SKEW_SECS: i64 = 60;

/// The decision inputs for the autonomy precondition (pure, offline-testable).
struct AutonomyContext {
    unattended_marker: Option<String>,
    nonce: Option<String>,
    now_unix: i64,
}

/// The fail-closed autonomy decision (R1/KTD1): refuses unless no unattended/CI
/// marker is present AND a fresh per-wave human nonce is present within TTL.
fn check_autonomy(ctx: &AutonomyContext) -> Result<(), String> {
    if let Some(reason) = &ctx.unattended_marker {
        return Err(format!(
            "refusing autonomous order placement: detected unattended context ({reason})"
        ));
    }
    let Some(nonce) = ctx.nonce.as_deref() else {
        return Err(
            "refusing autonomous order placement: per-wave human nonce absent \
             (`export LS_ORDER_SMOKE_NONCE=$(date +%s)`)"
                .to_string(),
        );
    };
    validate_nonce(nonce, ctx.now_unix)
}

/// Validate a per-wave nonce: a fresh unix-seconds timestamp within TTL. A
/// non-numeric constant fails to parse; a stale one is expired; a far-future one
/// is rejected as implausible skew — so "valid nonce" never degenerates to
/// "env var present".
fn validate_nonce(nonce: &str, now_unix: i64) -> Result<(), String> {
    let nonce = nonce.trim();
    if nonce.is_empty() {
        return Err("refusing: LS_ORDER_SMOKE_NONCE is empty".into());
    }
    let issued: i64 = nonce.parse().map_err(|_| {
        "refusing: LS_ORDER_SMOKE_NONCE must be a fresh unix-seconds timestamp (`date +%s`)"
            .to_string()
    })?;
    let age = now_unix - issued;
    if age > NONCE_TTL_SECS {
        return Err(format!(
            "refusing: LS_ORDER_SMOKE_NONCE is stale ({age}s old > {NONCE_TTL_SECS}s TTL)"
        ));
    }
    if age < -NONCE_MAX_SKEW_SECS {
        return Err(format!(
            "refusing: LS_ORDER_SMOKE_NONCE is from the future (skew {}s)",
            -age
        ));
    }
    Ok(())
}

/// Gather the live autonomy context and decide.
fn autonomy_guard() -> LsResult<()> {
    let ctx = AutonomyContext {
        unattended_marker: detect_unattended_marker(),
        nonce: std::env::var("LS_ORDER_SMOKE_NONCE").ok(),
        now_unix: now_unix(),
    };
    check_autonomy(&ctx).map_err(LsError::Config)
}

/// Detect an unattended/CI context: a CI env var set, or no TTY on stdin.
fn detect_unattended_marker() -> Option<String> {
    for var in ["CI", "GITHUB_ACTIONS"] {
        if std::env::var_os(var).is_some_and(|v| !v.is_empty()) {
            return Some(format!("{var} env set"));
        }
    }
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        return Some("no TTY on stdin".into());
    }
    None
}

/// Current wall-clock unix time (seconds).
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Install a process-global tracing subscriber that DROPS the `ls_core` dispatch
/// debug events (unscrubbed whole-body / `rsp_msg`). FAIL-CLOSED: if a foreign
/// global subscriber is already installed we cannot guarantee suppression, so we
/// refuse rather than fail open on a known leak (KTD4).
fn install_dispatch_log_suppressor() -> LsResult<()> {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::new("error,ls_core=off");
    let subscriber = tracing_subscriber::fmt().with_env_filter(filter).finish();
    tracing::subscriber::set_global_default(subscriber).map_err(|_| {
        LsError::Config(
            "refusing autonomous order negative probe: a foreign global tracing subscriber is \
             already installed (KTD4) — failing closed rather than risking a secret leak"
                .into(),
        )
    })
}

/// Widened secret scrubbing for autonomous-run output (R5): masks any maximal
/// `[A-Za-z0-9-]` token that contains a 6+ consecutive-digit substring (account
/// number, with or without a `-NN` suffix) or is 20+ alphanumeric chars (bearer
/// token / appkey). Short numbers and order numbers survive.
fn scrub_secrets(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut run = String::new();
    let flush = |out: &mut String, run: &mut String| {
        if run_is_sensitive(run) {
            out.push_str("***");
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '-' {
            run.push(c);
        } else {
            flush(&mut out, &mut run);
            out.push(c);
        }
    }
    flush(&mut out, &mut run);
    out
}

/// `true` if a `[A-Za-z0-9-]` token is account- or secret-like.
fn run_is_sensitive(run: &str) -> bool {
    let mut digits = 0usize;
    for c in run.chars() {
        if c.is_ascii_digit() {
            digits += 1;
            if digits >= 6 {
                return true;
            }
        } else {
            digits = 0;
        }
    }
    run.chars().filter(|c| c.is_ascii_alphanumeric()).count() >= 20
}

/// A validated daily price band from `t1102`.
#[derive(Debug, Clone, Copy)]
struct Band {
    uplmt: u64,
    dnlmt: u64,
}

/// Validate a band: both prices parse, are non-zero, and `up > dn`. A degenerate
/// band (halted / limit-locked / newly-listed) is rejected.
fn validate_band(uplmtprice: &str, dnlmtprice: &str) -> Result<Band, String> {
    let up: u64 = uplmtprice
        .trim()
        .parse()
        .map_err(|_| format!("unparseable uplmtprice '{uplmtprice}'"))?;
    let dn: u64 = dnlmtprice
        .trim()
        .parse()
        .map_err(|_| format!("unparseable dnlmtprice '{dnlmtprice}'"))?;
    if up == 0 || dn == 0 {
        return Err(format!("degenerate band (zero): up={up} dn={dn}"));
    }
    if up <= dn {
        return Err(format!("degenerate band (up<=dn): up={up} dn={dn}"));
    }
    Ok(Band { uplmt: up, dnlmt: dn })
}

/// KRX price tick ladder (2023+) — the on-tick increment for a given price.
fn tick(price: u64) -> u64 {
    match price {
        p if p < 2_000 => 1,
        p if p < 5_000 => 5,
        p if p < 20_000 => 10,
        p if p < 50_000 => 50,
        p if p < 200_000 => 100,
        p if p < 500_000 => 500,
        _ => 1_000,
    }
}

impl Band {
    /// Resting BUY price — at the floor (`dnlmtprice`): valid, far below market.
    fn resting_buy_price(&self) -> u64 {
        self.dnlmt
    }
}

/// The account-flatness verdict from a symbol-scoped `t0425` working-orders scan.
#[derive(Debug, Clone, PartialEq, Eq)]
enum FlatVerdict {
    Flat,
    Resting(Vec<String>),
    Fill(Vec<String>),
}

/// Parse a `t0425` quantity field.
fn parse_qty(s: &str) -> u64 {
    s.trim().parse().unwrap_or(0)
}

/// Classify a `t0425` row set into a flatness verdict (KTD2/KTD3): keys on
/// QUANTITIES, never status text. A fill outranks a resting remainder.
fn flat_verdict(rows: &[T0425OutBlock1]) -> FlatVerdict {
    let mut fills = Vec::new();
    let mut resting = Vec::new();
    for r in rows {
        let cheqty = parse_qty(&r.cheqty);
        let ordrem = parse_qty(&r.ordrem);
        if cheqty > 0 {
            fills.push(r.ordno.trim().to_string());
        } else if ordrem > 0 {
            resting.push(r.ordno.trim().to_string());
        }
    }
    if !fills.is_empty() {
        FlatVerdict::Fill(fills)
    } else if !resting.is_empty() {
        FlatVerdict::Resting(resting)
    } else {
        FlatVerdict::Flat
    }
}

/// Run the `t0425` working-orders scan for the traded symbol (KTD2), unfilled-only
/// (`chegb="2"`), single page, with a 1500ms pre-pace so the per-TR budget refills.
/// Returns `Err` on any failure (treated as NOT flat — positive confirmation only).
async fn scan_symbol_working_orders(
    sdk: &LsSdk,
    symbol: &str,
) -> Result<Vec<T0425OutBlock1>, String> {
    use ls_core::HasPagination;
    let req = T0425Request {
        inblock: T0425InBlock {
            expcode: symbol.into(),
            chegb: "2".into(),
            medosu: "0".into(),
            sortgb: "2".into(),
            cts_ordno: " ".into(),
        },
        tr_cont: String::new(),
        tr_cont_key: String::new(),
    };
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    match sdk.orders().inquiry(&req).await {
        Ok(resp) => {
            let cont = resp.tr_cont().trim().to_string();
            if !cont.is_empty() && !cont.eq_ignore_ascii_case("N") {
                return Err(format!(
                    "traded-symbol t0425 working-order scan is paginated (tr_cont={cont}) — \
                     a single page cannot positively confirm flat"
                ));
            }
            Ok(resp.outblock1)
        }
        Err(e) => Err(format!(
            "traded-symbol t0425 scan did not complete ({}) — cannot positively confirm flat",
            scrub_secrets(&e.to_string())
        )),
    }
}

/// `true` if a raw order response is a confirmed placement (an ack code on a 2xx).
/// Narrow by design — a malformed type/required variant must be REJECTED, so any
/// ack here is the "WAVE BLOCKED" tripwire, not a probe result (KTD3).
fn is_order_placement_success(http: u16, rsp_cd: &str) -> bool {
    (200..300).contains(&http) && matches!(rsp_cd, "00000" | "00039" | "00040")
}

/// The order-TR negative-probe class filter (KTD3): fire ONLY type/required
/// variants against a live order endpoint; enum/range/format are recorded `held`.
/// The single shared definition used by both the live leg and the offline test.
fn order_probe_classes(v: &InvalidVariant) -> bool {
    v.class == "type" || v.class == "required"
}

/// Best-effort symbol-scoped reconcile + cancel of any resting residue (KTD3): the
/// may-rest / final teardown. Never leaves a resting order behind.
async fn order_reconcile_teardown(sdk: &LsSdk, symbol: &str) {
    match scan_symbol_working_orders(sdk, symbol).await {
        Ok(rows) => {
            for r in rows
                .iter()
                .filter(|r| parse_qty(&r.cheqty) == 0 && parse_qty(&r.ordrem) > 0)
            {
                let cancel =
                    CSPAT00801Request::new(r.ordno.trim(), r.expcode.trim(), r.ordrem.trim());
                match sdk.orders().cancel(&cancel).await {
                    Ok(_) => {
                        println!("NEG-PROBE reconcile ordno={} result=canceled", r.ordno.trim())
                    }
                    Err(e) => println!(
                        "NEG-PROBE reconcile ordno={} result=[{}]",
                        r.ordno.trim(),
                        scrub_secrets(&e.to_string())
                    ),
                }
            }
            if let FlatVerdict::Fill(f) = flat_verdict(&rows) {
                println!(
                    "NEG-PROBE reconcile UNEXPECTED-FILL ordnos=[{}] — a fill cannot be canceled; \
                     reset the paper book",
                    f.join(",")
                );
            }
        }
        Err(e) => println!("NEG-PROBE reconcile-scan failed [{}]", scrub_secrets(&e.to_string())),
    }
}

// ---- order-leg seeds (shared by the live legs + offline twins) ------------

/// A well-formed `CSPAT00601` submit InBlock at the given (band-safe) resting price.
fn order_seed_00601(price: u64) -> serde_json::Value {
    serde_json::json!({
        "IsuNo": "005930", "OrdQty": 1, "OrdPrc": price, "BnsTpCode": "2",
        "OrdprcPtnCode": "00", "MgntrnCode": "000", "LoanDt": "",
        "OrdCndiTpCode": "0", "MbrNo": "NXT"
    })
}

/// A well-formed `CSPAT00701` modify InBlock referencing the live control `ordno`.
fn order_seed_00701(ordno: &str, price: u64) -> serde_json::Value {
    serde_json::json!({
        "OrgOrdNo": order_no_json(ordno), "IsuNo": "005930", "OrdQty": 1,
        "OrdprcPtnCode": "00", "OrdCndiTpCode": "0", "OrdPrc": price
    })
}

/// A well-formed `CSPAT00801` cancel InBlock referencing the live control `ordno`.
fn order_seed_00801(ordno: &str) -> serde_json::Value {
    serde_json::json!({ "OrgOrdNo": order_no_json(ordno), "IsuNo": "005930", "OrdQty": 1 })
}

/// Render an order number as a JSON number (the well-formed `OrgOrdNo` shape) when
/// it parses, else as a string (still a valid seed for variant generation).
fn order_no_json(ordno: &str) -> serde_json::Value {
    match ordno.trim().parse::<u64>() {
        Ok(n) => serde_json::json!(n),
        Err(_) => serde_json::json!(ordno.trim()),
    }
}

/// Run the order-TR negative probe (KTD3). Fail-closed autonomy chain → place a
/// real resting control via `CSPAT00601` submit → cancel + flat-verify BEFORE any
/// variant → fire ONLY type/required raw variants (enum/range/format recorded
/// `held`) → may-rest halt on transport/5xx → WAVE-BLOCKED on an accepted variant →
/// final reconcile. Never leaves a resting order.
async fn run_order_negative_probe(
    tr_cd: &str,
    inblock_key: &str,
    seed_builder: impl Fn(&str, &Band) -> serde_json::Value,
) {
    // U4: install the fail-closed dispatch-log suppressor BEFORE any dispatch.
    if let Err(e) = install_dispatch_log_suppressor() {
        panic!("{}", scrub_secrets(&e.to_string()));
    }
    // U1+U2: autonomy precondition (CI/no-TTY + fresh nonce) + paper opt-in + resolved.
    if let Err(e) = autonomy_guard() {
        panic!("{}", scrub_secrets(&e.to_string()));
    }
    if let Err(e) = order_smoke_guard() {
        panic!("{}", scrub_secrets(&e.to_string()));
    }
    let config = match LsConfig::from_env() {
        Ok(c) => c,
        Err(e) => panic!("{}", scrub_secrets(&e.to_string())),
    };
    if let Err(e) = assert_resolved_paper(&config.environment) {
        panic!("{}", scrub_secrets(&e.to_string()));
    }
    let sdk = match LsSdk::new(config.clone()) {
        Ok(s) => s,
        Err(e) => panic!("{}", scrub_secrets(&e.to_string())),
    };

    let symbol = "005930";
    let member = "NXT";

    // Daily band (KTD8) — a degenerate band HELDs (no placement).
    let band = match sdk
        .market_session()
        .quote(&T1102Request::new(symbol, "K"))
        .await
    {
        Ok(resp) => match validate_band(&resp.outblock.uplmtprice, &resp.outblock.dnlmtprice) {
            Ok(b) => b,
            Err(e) => {
                println!("NEG-PROBE target={tr_cd}-negative HELD: {e} — no placement, no variants");
                return;
            }
        },
        Err(e) => {
            println!(
                "NEG-PROBE target={tr_cd}-negative HELD: band fetch failed [{}] — no placement",
                scrub_secrets(&e.to_string())
            );
            return;
        }
    };

    let token = match sdk.standalone().token().await {
        Ok(t) if !t.is_empty() => t,
        _ => {
            println!("NEG-PROBE target={tr_cd}-negative HELD: token acquisition failed");
            return;
        }
    };
    let base = ls_core::config::Environment::resolve_base_url(&config);
    let url = format!("{base}/stock/order");
    let client = probe_client();

    // Place the VALID CONTROL: a non-marketable resting buy at the band floor. An
    // ambiguous/failed submit may have rested — reconcile before returning (R3).
    let control_req =
        CSPAT00601Request::limit(symbol, "1", band.resting_buy_price().to_string(), "2", member);
    let placed_ordno = match sdk.orders().submit(&control_req).await {
        Ok(resp) => resp.order_no().to_string(),
        Err(e) => {
            println!(
                "NEG-PROBE target={tr_cd}-negative HELD: control submit failed/ambiguous [{}] — \
                 reconciling, no variants",
                scrub_secrets(&e.to_string())
            );
            order_reconcile_teardown(&sdk, symbol).await;
            return;
        }
    };
    if placed_ordno.trim().is_empty() || placed_ordno.trim() == "0" {
        println!(
            "NEG-PROBE target={tr_cd}-negative HELD: control returned no usable order number"
        );
        order_reconcile_teardown(&sdk, symbol).await;
        return;
    }

    // Cancel the control and FLAT-VERIFY before any variant fires.
    let cancel = CSPAT00801Request::new(placed_ordno.trim(), symbol, "1");
    if let Err(e) = sdk.orders().cancel(&cancel).await {
        println!(
            "NEG-PROBE target={tr_cd}-negative control-cancel error [{}] — flat-verify decides",
            scrub_secrets(&e.to_string())
        );
    }
    match scan_symbol_working_orders(&sdk, symbol).await {
        Ok(rows) => match flat_verdict(&rows) {
            FlatVerdict::Flat => println!(
                "NEG-PROBE target={tr_cd}-negative control=[placed+canceled ok=true flat=confirmed]"
            ),
            _ => {
                println!(
                    "NEG-PROBE target={tr_cd}-negative HELD: control not positively flat after \
                     cancel — reconciling, no variants"
                );
                order_reconcile_teardown(&sdk, symbol).await;
                return;
            }
        },
        Err(e) => {
            println!(
                "NEG-PROBE target={tr_cd}-negative HELD: control flat-verify failed [{e}] — \
                 reconciling, no variants"
            );
            order_reconcile_teardown(&sdk, symbol).await;
            return;
        }
    }
    // The control positively placed and the book is flat.
    let control_ok = true;

    let schema = ls_core::schema_for(tr_cd)
        .unwrap_or_else(|| panic!("{tr_cd} carries an embedded constraint schema"))
        .clone();
    let seed = seed_builder(placed_ordno.trim(), &band);
    let variants = generate_invalid_variants(&schema, &seed);

    // Record the classes NOT fired against a live order endpoint (KTD3).
    for v in variants.iter().filter(|v| !order_probe_classes(v)) {
        println!(
            "NEG-PROBE target={tr_cd}-negative variant field={} class={} outcome=held \
             [enum/range/format not fired at a live order endpoint (KTD3)]",
            v.field, v.class
        );
    }

    for v in variants.iter().filter(|v| order_probe_classes(v)) {
        match fire_inblock(&client, &url, &token, tr_cd, inblock_key, &v.request).await {
            // Transport failure / timeout = MAY-REST: stop, reconcile, halt (KTD3).
            None => {
                println!(
                    "NEG-PROBE target={tr_cd}-negative variant field={} class={} \
                     result=[transport-failure] outcome=Held-may-rest halt=true",
                    v.field, v.class
                );
                order_reconcile_teardown(&sdk, symbol).await;
                return;
            }
            // A 5xx is also MAY-REST — the gateway may have accepted before failing.
            Some((http, rsp_cd)) if http >= 500 => {
                println!(
                    "NEG-PROBE target={tr_cd}-negative variant field={} class={} \
                     result=[http={http} rsp_cd={rsp_cd}] outcome=Held-may-rest halt=true",
                    v.field, v.class
                );
                order_reconcile_teardown(&sdk, symbol).await;
                return;
            }
            Some((http, rsp_cd)) => {
                if is_order_placement_success(http, &rsp_cd) {
                    // A malformed variant was ACCEPTED — do NOT classify; teardown + block.
                    println!(
                        "NEG-PROBE target={tr_cd}-negative WAVE BLOCKED pending investigation: \
                         variant field={} class={} was ACCEPTED [http={http} rsp_cd={rsp_cd}]",
                        v.field, v.class
                    );
                    order_reconcile_teardown(&sdk, symbol).await;
                    return;
                }
                // The normal case: a non-success rsp_cd = the variant placed nothing = Clean.
                let outcome = classify_probe(control_ok, true);
                println!(
                    "NEG-PROBE target={tr_cd}-negative variant field={} class={} \
                     result=[http={http} rsp_cd={rsp_cd}] outcome={outcome:?}",
                    v.field, v.class
                );
            }
        }
    }

    // Final flat-verify + cancel any residue — never leave a resting order.
    order_reconcile_teardown(&sdk, symbol).await;
    println!(
        "NEG-PROBE target={tr_cd}-negative teardown=done \
         note=[control canceled pre-variants; residue reconciled]"
    );
}

#[tokio::test]
#[ignore = "live probe: needs real LS paper ORDER-account + open KRX window + LS_ORDER_SMOKE=1 + a fresh LS_ORDER_SMOKE_NONCE (attended TTY); run via `make live-smoke-cspat00601-negative`"]
async fn live_smoke_cspat00601_negative() {
    run_order_negative_probe("CSPAT00601", "CSPAT00601InBlock1", |_ordno, band| {
        order_seed_00601(band.resting_buy_price())
    })
    .await;
}

#[tokio::test]
#[ignore = "live probe: needs real LS paper ORDER-account + open KRX window + LS_ORDER_SMOKE=1 + a fresh LS_ORDER_SMOKE_NONCE (attended TTY); run via `make live-smoke-cspat00701-negative`"]
async fn live_smoke_cspat00701_negative() {
    run_order_negative_probe("CSPAT00701", "CSPAT00701InBlock1", |ordno, band| {
        // A band-safe absolute modify price (one tick above the floor) so any tolerated
        // coercion rests, never fills.
        let price = band.dnlmt.saturating_add(tick(band.dnlmt)).min(band.uplmt);
        order_seed_00701(ordno, price)
    })
    .await;
}

#[tokio::test]
#[ignore = "live probe: needs real LS paper ORDER-account + open KRX window + LS_ORDER_SMOKE=1 + a fresh LS_ORDER_SMOKE_NONCE (attended TTY); run via `make live-smoke-cspat00801-negative`"]
async fn live_smoke_cspat00801_negative() {
    run_order_negative_probe("CSPAT00801", "CSPAT00801InBlock1", |ordno, _band| {
        order_seed_00801(ordno)
    })
    .await;
}

// ===========================================================================
// D. Offline twins (these RUN in CI — no network)
// ===========================================================================

#[test]
fn new_schema_offline_twins() {
    // Every new schema yields a non-empty, DETERMINISTIC variant sequence from its
    // valid control seed (mirrors `negative_probe_offline_twin`). No network.
    let cases: Vec<(&str, serde_json::Value)> = vec![
        ("t1101", serde_json::json!({ "shcode": "005930" })),
        ("t1102", serde_json::json!({ "shcode": "005930", "exchgubun": "1" })),
        ("CSPAQ12200", serde_json::json!({ "BalCreTp": "0" })),
        (
            "t0425",
            serde_json::json!({
                "expcode": "005930", "chegb": "0", "medosu": "0",
                "sortgb": "2", "cts_ordno": " "
            }),
        ),
        (
            "token",
            serde_json::json!({
                "grant_type": "client_credentials", "appkey": "K",
                "appsecretkey": "S", "scope": "oob"
            }),
        ),
        ("CSPAT00601", order_seed_00601(50_000)),
        ("CSPAT00701", order_seed_00701("12345", 50_000)),
        ("CSPAT00801", order_seed_00801("12345")),
    ];
    for (tr, seed) in &cases {
        let schema = ls_core::schema_for(tr)
            .unwrap_or_else(|| panic!("{tr} carries an embedded constraint schema"))
            .clone();
        let first = generate_invalid_variants(&schema, seed);
        assert!(!first.is_empty(), "{tr} must yield invalid variants");
        let again = generate_invalid_variants(&schema, seed);
        assert_eq!(first, again, "{tr} variant generation must be deterministic");
    }
}

#[test]
fn order_tr_variants_are_type_and_required_only() {
    // The live order legs fire ONLY the `order_probe_classes` subset. The SAME
    // predicate is asserted here: the fired set is type/required only, and each
    // order TR (all carry numeric fields) yields at least one of each.
    let cases: Vec<(&str, serde_json::Value)> = vec![
        ("CSPAT00601", order_seed_00601(50_000)),
        ("CSPAT00701", order_seed_00701("12345", 50_000)),
        ("CSPAT00801", order_seed_00801("12345")),
    ];
    for (tr, seed) in &cases {
        let schema = ls_core::schema_for(tr)
            .unwrap_or_else(|| panic!("{tr} carries an embedded constraint schema"))
            .clone();
        let variants = generate_invalid_variants(&schema, seed);
        let fired: Vec<&InvalidVariant> =
            variants.iter().filter(|v| order_probe_classes(v)).collect();
        assert!(
            fired.iter().all(|v| v.class == "type" || v.class == "required"),
            "{tr}: only type/required variants may fire at a live order endpoint (KTD3)"
        );
        assert!(
            fired.iter().any(|v| v.class == "required"),
            "{tr}: at least one required-class variant must survive the filter"
        );
        assert!(
            fired.iter().any(|v| v.class == "type"),
            "{tr}: at least one type-class variant must survive the filter"
        );
    }
}
