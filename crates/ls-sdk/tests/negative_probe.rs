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
// The credential scrubber is the canonical shared one (masks 6+-digit runs and
// 20+-alnum tokens) — reused, not re-implemented, so a future scrub hardening
// lands in one place for every smoke/probe leg (mirrors `live_smoke.rs`).
use ls_sdk_test_support::scrub_secrets;

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
        // exchgubun "K" matches the certified `T1102Request::new(symbol, "K")`
        // call (the order-leg band fetch uses it); an unverified "1" risks a
        // persistent HELD control that certifies nothing.
        serde_json::json!({ "shcode": "005930", "exchgubun": "K" }),
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

/// Why a scanned book is not clear-to-proceed. Shared by the three fill-inclusive
/// scan consumers (KTD2/KTD3): pre-assert-flat, post-cancel flat-verify, teardown.
#[derive(Debug, PartialEq, Eq)]
enum NotClear {
    /// Cancelable resting probe rows remain.
    Resting(Vec<String>),
    /// An uncancelable fill surfaced — unrecoverable (reset the paper book).
    Fill(Vec<String>),
}

/// Require the scanned book to be **Flat and fill-free** (KTD3). `Ok(())` = clear;
/// `Err(NotClear::Resting)` = cancelable resting rows; `Err(NotClear::Fill)` = an
/// uncancelable fill. Pure over a scanned row set (reuses `flat_verdict`) so every
/// consumer's guard — pre-assert-flat (refuse to place), post-cancel flat-verify
/// (HELD), teardown (UNEXPECTED-FILL alarm) — is unit-testable without a live scan.
/// The `Fill` arm is the one KTD2's fill-inclusive `chegb="0"` newly surfaces.
fn require_flat_and_fill_free(rows: &[T0425OutBlock1]) -> Result<(), NotClear> {
    match flat_verdict(rows) {
        FlatVerdict::Flat => Ok(()),
        FlatVerdict::Resting(o) => Err(NotClear::Resting(o)),
        FlatVerdict::Fill(f) => Err(NotClear::Fill(f)),
    }
}

/// Build the single-symbol working-orders request, **fill-inclusive** (`chegb="0"`
/// = all states, KTD2). `chegb="0"` returns still-resting orders, partial fills, AND
/// fully-filled rows (`ordrem==0`, `cheqty>0`) — the last of which `chegb="2"`
/// (unfilled-only) hid, blinding `flat_verdict`'s `Fill` branch. Symbol-scoped so the
/// account-wide `chegb="0"` history-walk that overran the page cap is avoided; the
/// single-page `tr_cont` guard (below) still fails closed. `chegb` value semantics
/// per `docs/solutions/integration-issues/ls-gateway-t0425-rate-limit-and-pagination-flat-scan.md`
/// (the LS API-doc reference; the normalized baseline records `chegb` only as a
/// 1-char String with no value meanings). Pure so the offline twin can assert the
/// fill-inclusive class without a live call.
fn working_orders_request(symbol: &str) -> T0425Request {
    T0425Request {
        inblock: T0425InBlock {
            expcode: symbol.into(),
            chegb: "0".into(),
            medosu: "0".into(),
            sortgb: "2".into(),
            cts_ordno: " ".into(),
        },
        tr_cont: String::new(),
        tr_cont_key: String::new(),
    }
}

/// Run the `t0425` working-orders scan for the traded symbol (KTD2), **fill-inclusive**
/// (`chegb="0"`, see `working_orders_request`), single page, with a 1500ms pre-pace so
/// the per-TR budget refills. Returns `Err` on any failure (treated as NOT flat —
/// positive confirmation only). Sound only paired with U3's pre-assert-flat, which
/// proves no foreign/historical fill is present to misattribute.
async fn scan_symbol_working_orders(
    sdk: &LsSdk,
    symbol: &str,
) -> Result<Vec<T0425OutBlock1>, String> {
    use ls_core::HasPagination;
    let req = working_orders_request(symbol);
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
            safe_err(&e)
        )),
    }
}

/// `true` if a raw order response is a confirmed placement/acceptance (an ack
/// code on a 2xx). The "WAVE BLOCKED" tripwire (KTD3): any accepted malformed
/// variant must trip it, not be classified as a probe result. The code set is the
/// full order-acceptance set ls-core recognizes in `rsp_cd_is_order_success`
/// (submit `00039`/`00040`, **modify `00462`, cancel `00463`/`00156`, F/O modify
/// `00132`**) — the modify/cancel legs return those, so the submit-only set would
/// let an accepted modify/cancel variant slip through as "Clean". `00000` (the
/// ambiguous generic-success) is additionally treated as may-have-placed so the
/// tripwire fails toward blocked. Kept in sync with ls-core by
/// `is_order_placement_success_recognizes_the_ls_core_ack_set` below.
fn is_order_placement_success(http: u16, rsp_cd: &str) -> bool {
    (200..300).contains(&http)
        && matches!(
            rsp_cd,
            "00000" | "00039" | "00040" | "00462" | "00463" | "00156" | "00132"
        )
}

/// Render an `LsError` credential-free for a probe log line. For an `ApiError` /
/// `AmbiguousOrder` the `Display` is `"API error {code}: {message}"` where
/// `message` is the raw broker `rsp_msg` — localized text that can echo account
/// data — so this NEVER renders the Display for those variants; it prints only the
/// business `code` plus the offline error-catalog explanation (`LsError::explain`,
/// code-only by construction). `scrub_secrets` is a token masker, not an rsp_msg
/// filter, so it cannot be relied on to strip that message. Other variants
/// (transport / config; no broker message) are scrubbed as before. This upholds
/// the module's "never rsp_msg" contract on the error/HELD paths, not just the
/// happy path.
fn safe_err(e: &LsError) -> String {
    match e {
        LsError::ApiError { code, .. } | LsError::AmbiguousOrder { code, .. } => {
            format!("code={code} ({})", e.explain().unwrap_or("gateway error"))
        }
        other => scrub_secrets(&other.to_string()),
    }
}

/// The order-TR negative-probe class filter (KTD3): fire ONLY type/required
/// variants against a live order endpoint; enum/range/format are recorded `held`.
/// The single shared definition used by both the live leg and the offline test.
fn order_probe_classes(v: &InvalidVariant) -> bool {
    v.class == "type" || v.class == "required"
}

/// Best-effort symbol-scoped reconcile + cancel of any resting residue (KTD3): the
/// may-rest / final teardown. Cancels every resting row the single-page symbol
/// scan returns; a scan that fails or paginates is surfaced loudly
/// (`reconcile-scan failed`) rather than silently trusted, so teardown is
/// best-effort — an operator must confirm the book is flat on a scan failure, and
/// an unexpected fill is reported as unrecoverable (reset the paper book).
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
                        safe_err(&e)
                    ),
                }
            }
            // Teardown consumer of the fill-inclusive scan (KTD2): a fill the
            // fill-inclusive `chegb="0"` now surfaces is uncancelable → alarm.
            if let Err(NotClear::Fill(f)) = require_flat_and_fill_free(&rows) {
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
                safe_err(&e)
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

    // PRE-ASSERT-FLAT (KTD3): the symbol must be Flat AND fill-free BEFORE we place
    // the control. Scan fill-inclusive (KTD2); if any resting OR filled `005930` row
    // exists, HELD — refuse to place. Do NOT teardown here: a non-flat pre-state means
    // a FOREIGN row is present (or a stranded control from a prior leg — the operator
    // clears it between legs, KTD3), which the probe must not cancel. This proven
    // flat+fill-free baseline is what makes the later teardown's UNCONDITIONAL
    // cancel-every-resting-row sound: every resting row at teardown is then the probe's
    // by construction, so no ownership set is needed.
    match scan_symbol_working_orders(&sdk, symbol).await {
        Ok(rows) => match require_flat_and_fill_free(&rows) {
            Ok(()) => {}
            Err(NotClear::Resting(o)) => {
                println!(
                    "NEG-PROBE target={tr_cd}-negative HELD: pre-assert-flat refused — symbol not \
                     flat (resting ordnos=[{}]; a prior leg's stranded control? clear it) — no \
                     placement, no variants",
                    o.join(",")
                );
                return;
            }
            Err(NotClear::Fill(f)) => {
                println!(
                    "NEG-PROBE target={tr_cd}-negative HELD: pre-assert-flat refused — symbol has a \
                     fill (ordnos=[{}]); reset the paper book — no placement, no variants",
                    f.join(",")
                );
                return;
            }
        },
        Err(e) => {
            println!(
                "NEG-PROBE target={tr_cd}-negative HELD: pre-assert-flat scan failed [{e}] — no \
                 placement, no variants"
            );
            return;
        }
    }

    // Place the VALID CONTROL: a non-marketable resting buy at the band floor. It
    // stays RESTING while the variants fire (KTD1), so the referencing modify/cancel
    // legs exercise a LIVE control OrgOrdNo. An ambiguous/failed submit may have rested
    // — reconcile before returning (R3).
    let control_req =
        CSPAT00601Request::limit(symbol, "1", band.resting_buy_price().to_string(), "2", member);
    let placed_ordno = match sdk.orders().submit(&control_req).await {
        Ok(resp) => resp.order_no().to_string(),
        Err(e) => {
            println!(
                "NEG-PROBE target={tr_cd}-negative HELD: control submit failed/ambiguous [{}] — \
                 reconciling, no variants",
                safe_err(&e)
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
    // The valid control came back a success and is RESTING. `control_ok` is
    // `classify_probe`'s precondition (the valid control succeeded) — satisfied by the
    // successful placement; the cancel + flat-verify "cancel works" proof happens AFTER
    // the variants (KTD1).
    let control_ok = true;
    println!(
        "NEG-PROBE target={tr_cd}-negative control=[placed ok=true resting] — firing variants \
         against the live control"
    );

    // Build the variant seed off the RESTING control ordno (KTD1): the referencing
    // modify/cancel legs now exercise a live OrgOrdNo. Fill-vector bound (KTD1): only
    // type/required variants fire (a removed/wrong-typed OrdPrc is rejected, not coerced
    // to a marketable price), the modify seed is band-floor+1 tick, and the fill-inclusive
    // post-variant flat-verify (KTD2) catches any fill post-hoc.
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

    // Variants fired against the LIVE control. Now CANCEL the control and FLAT-VERIFY
    // (fill-inclusive, KTD2) — the "cancel works + no residue" proof, moved AFTER the
    // variants (KTD1). A fill surfaced here (the fill-vector KTD1 bounds) routes to the
    // unrecoverable UNEXPECTED-FILL HELD path, not a plain not-flat.
    let cancel = CSPAT00801Request::new(placed_ordno.trim(), symbol, "1");
    if let Err(e) = sdk.orders().cancel(&cancel).await {
        println!(
            "NEG-PROBE target={tr_cd}-negative control-cancel error [{}] — flat-verify decides",
            safe_err(&e)
        );
    }
    match scan_symbol_working_orders(&sdk, symbol).await {
        Ok(rows) => match require_flat_and_fill_free(&rows) {
            Ok(()) => println!(
                "NEG-PROBE target={tr_cd}-negative \
                 control=[placed+variants-fired+canceled ok=true flat=confirmed]"
            ),
            Err(NotClear::Fill(f)) => {
                println!(
                    "NEG-PROBE target={tr_cd}-negative HELD: UNEXPECTED-FILL ordnos=[{}] after \
                     variants — a fill cannot be canceled; reset the paper book — reconciling",
                    f.join(",")
                );
                order_reconcile_teardown(&sdk, symbol).await;
                return;
            }
            Err(NotClear::Resting(_)) => {
                println!(
                    "NEG-PROBE target={tr_cd}-negative HELD: control not positively flat after \
                     cancel — reconciling"
                );
                order_reconcile_teardown(&sdk, symbol).await;
                return;
            }
        },
        Err(e) => {
            println!(
                "NEG-PROBE target={tr_cd}-negative HELD: control flat-verify failed [{e}] — \
                 reconciling"
            );
            order_reconcile_teardown(&sdk, symbol).await;
            return;
        }
    }

    // Final flat-verify + cancel any residue — never leave a resting order. The
    // teardown cancel is UNCONDITIONAL (every resting row, no ownership set): sound
    // because pre-assert-flat proved the book clean at start, so every resting row is
    // the probe's — including an accepted WAVE-BLOCKED submit variant whose OrdNo
    // `fire_inblock` never surfaces (an owned-only teardown would strand it).
    order_reconcile_teardown(&sdk, symbol).await;
    println!(
        "NEG-PROBE target={tr_cd}-negative teardown=done \
         note=[variants fired against live control; control canceled+flat-verified post-variants; \
         residue reconciled]"
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
        ("t1102", serde_json::json!({ "shcode": "005930", "exchgubun": "K" })),
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

// ===========================================================================
// E. Pure-logic unit tests (RUN in CI) — the fail-closed autonomy gate, the
// credential-free contract, and the order-safety classifiers are the leg's
// safety spine; without these their first exercise would be a live attended run
// against real credentials.
// ===========================================================================

#[test]
fn validate_nonce_accepts_fresh_and_rejects_empty_nonnumeric_stale_and_future() {
    let now = 1_000_000i64;
    // A fresh unix-seconds nonce within TTL.
    assert!(validate_nonce("999950", now).is_ok(), "fresh nonce accepted");
    // Inclusive boundaries: exactly TTL old and exactly max forward skew.
    assert!(validate_nonce(&(now - NONCE_TTL_SECS).to_string(), now).is_ok(), "TTL edge ok");
    assert!(validate_nonce(&(now + NONCE_MAX_SKEW_SECS).to_string(), now).is_ok(), "skew edge ok");
    // Rejections: empty, non-numeric, stale past TTL, implausible future.
    assert!(validate_nonce("   ", now).is_err(), "empty");
    assert!(validate_nonce("not-a-timestamp", now).is_err(), "non-numeric");
    assert!(validate_nonce(&(now - NONCE_TTL_SECS - 1).to_string(), now).is_err(), "stale");
    assert!(validate_nonce(&(now + NONCE_MAX_SKEW_SECS + 1).to_string(), now).is_err(), "future");
}

#[test]
fn check_autonomy_refuses_unattended_or_missing_nonce_and_passes_when_clear() {
    let now = 1_000_000i64;
    // Unattended marker present → refuse regardless of nonce.
    assert!(check_autonomy(&AutonomyContext {
        unattended_marker: Some("CI env set".into()),
        nonce: Some(now.to_string()),
        now_unix: now,
    })
    .is_err());
    // Attended but no nonce → refuse.
    assert!(check_autonomy(&AutonomyContext {
        unattended_marker: None,
        nonce: None,
        now_unix: now,
    })
    .is_err());
    // Attended + fresh nonce → pass.
    assert!(check_autonomy(&AutonomyContext {
        unattended_marker: None,
        nonce: Some(now.to_string()),
        now_unix: now,
    })
    .is_ok());
}

#[test]
fn scrub_secrets_masks_credentials_and_keeps_short_order_numbers() {
    // The credential-free contract the probe depends on (shared scrubber): an
    // account number (with or without -NN suffix) and a long token mask; a short
    // order number survives so the agent can still read it.
    assert_eq!(scrub_secrets("acct=20187511401 ok"), "acct=*** ok");
    assert_eq!(scrub_secrets("acct=20187511401-01 ok"), "acct=*** ok");
    assert_eq!(scrub_secrets("ordno=12345 qty=10"), "ordno=12345 qty=10");
}

#[test]
fn validate_band_rejects_degenerate_and_accepts_up_over_dn() {
    assert!(validate_band("60000", "50000").is_ok(), "up>dn ok");
    assert!(validate_band("50000", "50000").is_err(), "up==dn degenerate");
    assert!(validate_band("40000", "50000").is_err(), "up<dn degenerate");
    assert!(validate_band("0", "0").is_err(), "zero degenerate");
    assert!(validate_band("abc", "50000").is_err(), "unparseable");
}

#[test]
fn tick_follows_the_krx_ladder_at_the_boundaries() {
    assert_eq!(tick(1_999), 1);
    assert_eq!(tick(2_000), 5);
    assert_eq!(tick(4_999), 5);
    assert_eq!(tick(5_000), 10);
    assert_eq!(tick(19_999), 10);
    assert_eq!(tick(20_000), 50);
    assert_eq!(tick(49_999), 50);
    assert_eq!(tick(50_000), 100);
    assert_eq!(tick(200_000), 500);
    assert_eq!(tick(500_000), 1_000);
}

#[test]
fn flat_verdict_keys_on_quantities_and_a_fill_outranks_a_resting_remainder() {
    let row = |ordno: &str, cheqty: &str, ordrem: &str| T0425OutBlock1 {
        ordno: ordno.into(),
        cheqty: cheqty.into(),
        ordrem: ordrem.into(),
        ..Default::default()
    };
    assert_eq!(flat_verdict(&[]), FlatVerdict::Flat);
    assert_eq!(flat_verdict(&[row("1", "0", "0")]), FlatVerdict::Flat);
    assert_eq!(flat_verdict(&[row("1", "0", "5")]), FlatVerdict::Resting(vec!["1".into()]));
    // A fill (cheqty>0) outranks a resting remainder, even in the same set.
    assert_eq!(
        flat_verdict(&[row("1", "0", "5"), row("2", "3", "0")]),
        FlatVerdict::Fill(vec!["2".into()])
    );
}

#[test]
fn working_orders_scan_request_is_fill_inclusive() {
    // Gap (b)/KTD2: the shared flat-verify scan request must carry the fill-inclusive
    // `chegb="0"` (all states incl. fully-filled), not the old unfilled-only `"2"` that
    // hid a fill from `flat_verdict`. Asserted on the request builder — no live call.
    let req = working_orders_request("005930");
    assert_eq!(req.inblock.chegb, "0", "scan must be fill-inclusive (chegb=0), not unfilled-only");
    assert_eq!(req.inblock.expcode, "005930", "scan stays symbol-scoped");
}

#[test]
fn require_flat_and_fill_free_gates_all_three_scan_consumers() {
    // Gap (b)+(c)/KTD2/KTD3: the SAME pure decision feeds all three fill-inclusive scan
    // consumers — pre-assert-flat (refuse to place), post-cancel flat-verify (HELD), and
    // teardown (UNEXPECTED-FILL alarm). A synthesized fill drives the unrecoverable
    // outcome, not just `flat_verdict` in isolation.
    let row = |ordno: &str, cheqty: &str, ordrem: &str| T0425OutBlock1 {
        ordno: ordno.into(),
        cheqty: cheqty.into(),
        ordrem: ordrem.into(),
        ..Default::default()
    };
    // Clear book → proceed (pre-assert places; post-cancel confirms flat; teardown quiet).
    assert_eq!(require_flat_and_fill_free(&[]), Ok(()));
    assert_eq!(require_flat_and_fill_free(&[row("1", "0", "0")]), Ok(()));
    // A resting probe row → NotClear::Resting (cancelable): pre-assert refuses, post-cancel
    // HELDs, teardown cancels it.
    assert_eq!(
        require_flat_and_fill_free(&[row("7", "0", "5")]),
        Err(NotClear::Resting(vec!["7".into()]))
    );
    // A FILL (cheqty>0), newly visible under chegb="0" → NotClear::Fill (uncancelable):
    // pre-assert refuses ("reset the paper book"), post-cancel routes to UNEXPECTED-FILL
    // HELD, teardown raises the UNEXPECTED-FILL alarm. This is the branch the old
    // unfilled-only scan could never reach.
    assert_eq!(
        require_flat_and_fill_free(&[row("9", "2", "0")]),
        Err(NotClear::Fill(vec!["9".into()]))
    );
    // A fill outranks a resting remainder in the same set → unrecoverable Fill.
    assert_eq!(
        require_flat_and_fill_free(&[row("1", "0", "5"), row("9", "2", "0")]),
        Err(NotClear::Fill(vec!["9".into()]))
    );
}

#[test]
fn form_with_replaces_a_field_or_removes_it() {
    let base = vec![("a".to_string(), "1".to_string()), ("b".to_string(), "2".to_string())];
    let replaced = form_with(&base, "a", Some("9"));
    assert!(replaced.contains(&("a".to_string(), "9".to_string())), "value replaced");
    assert_eq!(replaced.iter().filter(|(k, _)| k == "a").count(), 1, "no duplicate key");
    let removed = form_with(&base, "a", None);
    assert!(!removed.iter().any(|(k, _)| k == "a"), "field removed");
    assert!(removed.iter().any(|(k, _)| k == "b"), "other field kept");
}

#[test]
fn is_order_placement_success_recognizes_the_ls_core_ack_set() {
    // Locks the WAVE-BLOCKED tripwire against the full order-acceptance set
    // ls-core recognizes: submit 00039/00040, modify 00462, cancel 00463/00156,
    // F/O modify 00132 — plus the ambiguous 00000 (fail toward blocked). A
    // submit-only set would let an accepted malformed modify/cancel variant pass
    // as Clean.
    for code in ["00000", "00039", "00040", "00462", "00463", "00156", "00132"] {
        assert!(is_order_placement_success(200, code), "{code} must trip WAVE-BLOCKED");
    }
    // A business-rejection code is not a placement (the normal Clean path); a
    // non-2xx is never a placement regardless of code.
    assert!(!is_order_placement_success(200, "40510"), "a rejection is not a placement");
    assert!(!is_order_placement_success(500, "00040"), "a 5xx is never a placement");
}

#[test]
fn order_no_json_renders_numeric_ordno_as_a_json_number() {
    // OrgOrdNo must serialize as a JSON number (IGW40011 rule); a numeric order
    // number renders as a number, a non-numeric one falls back to a string seed.
    assert!(order_no_json("12345").is_number());
    assert!(order_no_json(" 12345 ").is_number(), "trimmed then numeric");
    assert!(order_no_json("O-1").is_string(), "non-numeric falls back to string");
}
