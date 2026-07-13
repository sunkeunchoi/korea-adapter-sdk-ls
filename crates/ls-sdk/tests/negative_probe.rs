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

use std::collections::BTreeSet;
use std::time::Duration;

use ls_core::{
    classify_probe, generate_invalid_variants, ConstraintSchema, CrossFieldRule, InvalidVariant,
    LsConfig, LsError, LsResult, ProbeOutcome, VariantVerdict,
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

    // Differential comparator (AE2 / KTD1, 3-way verdict).
    assert_eq!(
        classify_probe(false, VariantVerdict::Rejected),
        ProbeOutcome::Held
    );
    assert_eq!(
        classify_probe(true, VariantVerdict::Rejected),
        ProbeOutcome::Clean
    );
    assert_eq!(
        classify_probe(true, VariantVerdict::Accepted),
        ProbeOutcome::Divergent
    );
}

#[test]
fn t8412_probe_is_paced() {
    // §27 reason A / #117 item 3: the t8412 live differential probe must carry a
    // NON-ZERO inter-dispatch pace so it does not self-inflict an `IGW00201`
    // throttle that masks every variant as a false `Clean`. This offline proxy
    // asserts the structural property (the pace is present and market-data-sized);
    // the true anti-throttle behavior is only observable in the Monday in-window
    // re-probe (the leg is `#[ignore]`).
    assert!(
        !T8412_PROBE_PACE.is_zero(),
        "t8412 probe must be paced (non-zero) so it does not self-throttle (§27 reason A)"
    );
    // At least the 10/s market-data bucket period (100 ms) — a meaningfully sized
    // pace, not a token sub-millisecond value.
    assert!(
        T8412_PROBE_PACE >= Duration::from_millis(100),
        "t8412 pace should be at least the 10/s market-data bucket period (100 ms)"
    );
}

/// `true` if a gateway response classifies as a read success (control passes).
fn is_success(rsp_cd: &str) -> bool {
    matches!(rsp_cd, "" | "00000" | "00136" | "00707")
}

/// `true` if the gateway is known to tolerate an accepted violation of `class` on
/// `field` for this schema — the `gateway_tolerant` facet (U2/KTD4). A pure lookup
/// so the per-class downgrade is offline-twinnable without a live call.
///
/// Two tolerance sources, both marked in the schema: a per-field `(field, class)`
/// pair (a field's `gateway_tolerant` list), and — for the `cross_field` class — a
/// combination rule flagged `gateway_tolerant` (§30, t8412 `sdate/edate`). The
/// cross_field variant is generated with the pseudo-field `"<start>/<end>"`, which
/// the per-field lookup never matches, so it is resolved against the schema's
/// `cross_field` rules instead. A pseudo-field with no marked rule stays
/// non-tolerant.
fn is_gateway_tolerant(schema: &ConstraintSchema, field: &str, class: &str) -> bool {
    // Per-field (field, class) tolerance.
    if let Some(f) = schema.fields.iter().find(|f| f.name == field) {
        return f.gateway_tolerant.iter().any(|c| c == class);
    }
    // Cross-field tolerance: a `cross_field` variant's pseudo-field is
    // `"<start>/<end>"`; a rule flagged `gateway_tolerant` downgrades its accepted
    // violation just like a per-field pair.
    if class == "cross_field" {
        return schema.cross_field.iter().any(|rule| match rule {
            CrossFieldRule::DateOrder {
                start,
                end,
                gateway_tolerant,
                ..
            } => *gateway_tolerant && format!("{start}/{end}") == field,
        });
    }
    false
}

/// The reported outcome LABEL for one differential probe result (U2/KTD4). Pure
/// tolerance layer over `classify_probe`: a `Divergent` result whose `(field,
/// class)` is marked `gateway_tolerant` is downgraded to `expected-tolerant`
/// (non-blocking); every other outcome renders verbatim. `classify_probe` itself
/// is untouched — this is the single decision function both live probe loops and
/// the offline twin call, so the two cannot drift.
fn reported_outcome(
    schema: &ConstraintSchema,
    field: &str,
    class: &str,
    outcome: ProbeOutcome,
) -> &'static str {
    if outcome == ProbeOutcome::Divergent && is_gateway_tolerant(schema, field, class) {
        return "expected-tolerant";
    }
    match outcome {
        ProbeOutcome::Clean => "Clean",
        ProbeOutcome::Held => "Held",
        ProbeOutcome::Divergent => "Divergent",
    }
}

/// Inter-dispatch pace for the t8412 live differential probe (§27 reason A / #117
/// item 3). t8412 fires ~12 calls (control + 11 variants); with no pace they
/// collided in the market-data bucket and every variant read a self-inflicted
/// `IGW00201` throttle — which classifies as a rejection → false `Clean`, so the
/// differential was never evaluated on merits. `IGW00201` is a warm-sensitive
/// *cumulative* budget, NOT a pure rate (see the `igw00201-budget-characterization`
/// learning), so t8412's higher call count trips it where the lower-count sibling
/// market-data legs (t1101 / t1102, `Duration::ZERO`) do not — and, because the
/// budget is cumulative, no per-dispatch pace can *guarantee* it stays cool. The
/// 2026-07-13 in-window re-cert (ledger §30) confirmed this empirically: at 250 ms two
/// variants tripped `IGW00201`, and at 500 ms two *different* variants tripped — the
/// throttle MOVED, not cleared. Only **1000 ms** (10× the 10/s bucket period, ~12 s
/// total, still well below the 1500 ms account-lane pace) produced a zero-throttle
/// authoritative read. Offline only the non-zero property is asserted
/// (`t8412_probe_is_paced`); a residual throttle would still read as `Clean`, which
/// is why the true anti-throttle proof is the live re-probe, not this const (the
/// operator can still bump this single named const further in-window if a warm
/// budget ever needs it — KTD-2 / risk R-2).
const T8412_PROBE_PACE: Duration = Duration::from_millis(1000);

#[tokio::test]
#[ignore = "live probe: needs real LS paper credentials + in-window session; run via `make live-smoke-t8412-negative`"]
async fn live_smoke_t8412_negative() {
    // The last read leg to leave its bespoke inline loop for the shared U6-paced
    // helper (§27 reason A / #117 item 3, KTD-1). Delegating inherits the
    // inter-dispatch pace (`T8412_PROBE_PACE`, above) AND the `reported_outcome`
    // gateway_tolerant downgrade the inline loop used to re-wire by hand — so the
    // duplicated `fire` + classification are gone (R2). Header parity with the old
    // inline `fire` holds: `fire_inblock` posts the same `{"t8412InBlock": …}` body
    // with `tr_cd` / `tr_cont: "N"` / `tr_cont_key: ""` (KTD-3).
    run_inblock_negative_probe(
        "t8412",
        "/stock/chart",
        "t8412InBlock",
        valid_seed(),
        T8412_PROBE_PACE,
    )
    .await;
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

/// Recursively surface the live order number (`"OrdNo"`) from a response body
/// (U5/R3). The order number lives under a per-TR OutBlock key (e.g.
/// `CSPAT00601OutBlock2.OrdNo`), NOT top-level like `rsp_cd`, so the lookup
/// descends into nested objects/arrays. Only the exact `OrdNo` key matches — the
/// auxiliary `SpareOrdNo`/`RsvOrdNo` are deliberately excluded. Returns `None`
/// when absent, empty, `"0"`, or non-scalar (no parseable order number surfaced).
fn extract_ord_no(body: &serde_json::Value) -> Option<String> {
    fn scalar(v: &serde_json::Value) -> Option<String> {
        let s = match v {
            serde_json::Value::String(s) => s.trim().to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            _ => return None,
        };
        (!s.is_empty() && s != "0").then_some(s)
    }
    match body {
        serde_json::Value::Object(map) => {
            if let Some(s) = map.get("OrdNo").and_then(scalar) {
                return Some(s);
            }
            map.values().find_map(extract_ord_no)
        }
        serde_json::Value::Array(arr) => arr.iter().find_map(extract_ord_no),
        _ => None,
    }
}

/// Fire one raw InBlock request for `tr_cd` at `url`, wrapping `inblock` under
/// `inblock_key` (`{"<TR>InBlock…": inblock}`) — the parametrized generalization
/// of the t8412 exemplar's inline `fire`. Returns `Some((http_status, rsp_cd,
/// ord_no))` when the gateway ANSWERED (`ord_no` is the surfaced order number, R3,
/// `None` for a read TR or an unparseable body), or `None` on a transport failure
/// (timeout / connection / body-read error). A transport failure is NOT a
/// rejection — collapsing it would let a network blip print a false CLEAN. Never
/// emits `rsp_msg` or body content.
async fn fire_inblock(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    tr_cd: &str,
    inblock_key: &str,
    inblock: &serde_json::Value,
) -> Option<(u16, String, Option<String>)> {
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
    let parsed: Option<serde_json::Value> = serde_json::from_str(&text).ok();
    let rsp_cd = parsed
        .as_ref()
        .and_then(|v| v.get("rsp_cd").and_then(|c| c.as_str()).map(String::from))
        .unwrap_or_default();
    let ord_no = parsed.as_ref().and_then(extract_ord_no);
    Some((status, rsp_cd, ord_no))
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
    pace: Duration,
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

    // U6/R12: pace each dispatch so an account-lane (1/s bucket) TR — CSPAQ12200,
    // t0425 — does not collide the control and its variants in the bucket and read
    // a self-inflicted IGW00201 throttle instead of the merits response. Market-data
    // reads pass `Duration::ZERO`.
    let pace_dispatch = || async {
        if !pace.is_zero() {
            tokio::time::sleep(pace).await;
        }
    };

    // Valid control, same session.
    pace_dispatch().await;
    let control = fire_inblock(&client, &url, &token, tr_cd, inblock_key, &seed).await;
    let control_ok =
        matches!(&control, Some((http, cd, _)) if *http >= 200 && *http < 300 && is_success(cd));
    match &control {
        Some((http, cd, _)) => println!(
            "NEG-PROBE target={tr_cd}-negative control=[http={http} rsp_cd={cd} ok={control_ok}]"
        ),
        None => {
            println!("NEG-PROBE target={tr_cd}-negative control=[transport-failure ok=false]")
        }
    }

    for variant in generate_invalid_variants(&schema, &seed) {
        let field = &variant.field;
        let class = &variant.class;
        pace_dispatch().await;
        match fire_inblock(&client, &url, &token, tr_cd, inblock_key, &variant.request).await {
            Some((http, rsp_cd, _)) => {
                let variant_rejected = !(http >= 200 && http < 300 && is_success(&rsp_cd));
                let verdict = if variant_rejected {
                    VariantVerdict::Rejected
                } else {
                    VariantVerdict::Accepted
                };
                let outcome = classify_probe(control_ok, verdict);
                // U2/KTD4: downgrade a Divergent on a gateway_tolerant (field, class)
                // to the non-blocking expected-tolerant outcome.
                let label = reported_outcome(&schema, field, class, outcome);
                println!(
                    "NEG-PROBE target={tr_cd}-negative variant field={field} class={class} \
                     result=[http={http} rsp_cd={rsp_cd}] outcome={label}"
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
        Duration::ZERO,
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
        Duration::ZERO,
    )
    .await;
}

#[tokio::test]
#[ignore = "live probe: needs real LS paper credentials + in-window session; run via `make live-smoke-cspaq12200-negative`"]
async fn live_smoke_cspaq12200_negative() {
    // BalCreTp "0" is the well-formed control; a live HELD on this value (a funding-
    // dependent account) is reconciled by the operator — offline this never runs.
    // U6/R12: CSPAQ12200 sits on the Account 1/s bucket; its sole `BalCreTp/required`
    // variant only ever returned IGW00201 (a self-inflicted throttle) when the control
    // and variant collided in the bucket. Pace 1500ms so the bucket is cool for each
    // dispatch and the variant is evaluated on merits (AE5), not throttled.
    run_inblock_negative_probe(
        "CSPAQ12200",
        "/stock/accno",
        "CSPAQ12200InBlock1",
        serde_json::json!({ "BalCreTp": "0" }),
        Duration::from_millis(1500),
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
        // U6/R12: t0425 is also on the Account 1/s bucket — pace so its many variants
        // do not throttle each other.
        Duration::from_millis(1500),
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
                let verdict = if ok {
                    VariantVerdict::Accepted
                } else {
                    VariantVerdict::Rejected
                };
                let outcome = classify_probe(control_ok, verdict);
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

/// Why a scanned book is not clear-to-proceed. Shared by the `chegb="2"` scan
/// consumers (KTD1): pre-assert-flat, post-cancel flat-verify, teardown.
#[derive(Debug, PartialEq, Eq)]
enum NotClear {
    /// Cancelable resting probe rows remain.
    Resting(Vec<String>),
    /// An uncancelable fill surfaced — unrecoverable (reset the paper book).
    Fill(Vec<String>),
}

/// Require the scanned book to be **Flat and fill-free** (KTD1). `Ok(())` = clear;
/// `Err(NotClear::Resting)` = cancelable resting rows; `Err(NotClear::Fill)` = an
/// uncancelable fill. Pure over a scanned row set (reuses `flat_verdict`) so every
/// consumer's guard — pre-assert-flat (refuse to place), post-cancel flat-verify
/// (HELD), teardown (UNEXPECTED-FILL alarm) — is unit-testable without a live scan.
/// Under `chegb="2"` the `Fill` arm still fires on a *partial* fill (`cheqty>0`,
/// `ordrem>0`, which stays on the working set) — it is only a *fully-filled*
/// (`ordrem==0`) control that `chegb="2"` hides, which the bounded post-cancel
/// `classify_control_disposition` fill-check covers instead (KTD1).
fn require_flat_and_fill_free(rows: &[T0425OutBlock1]) -> Result<(), NotClear> {
    match flat_verdict(rows) {
        FlatVerdict::Flat => Ok(()),
        FlatVerdict::Resting(o) => Err(NotClear::Resting(o)),
        FlatVerdict::Fill(f) => Err(NotClear::Fill(f)),
    }
}

/// The post-cancel disposition of the placed control (KTD1 bounded fill-check).
/// Combines the cancel outcome (`cancel_ok`) with the `chegb="2"` scan to
/// disambiguate a cleanly-canceled control from one that filled — NOT mere absence
/// from the resting set. Because `chegb="2"` excludes a *fully-filled* (`ordrem==0`)
/// row, an absent control is ambiguous on its own; a cancel that **failed** while
/// the scan reads flat means the control left the book as a fill (it could not be
/// canceled because there was no remaining quantity), so it is routed to the
/// unrecoverable `Filled` outcome rather than a clean pass.
#[derive(Debug, PartialEq, Eq)]
enum ControlDisposition {
    /// Nothing resting and no positive fill signal — the control canceled cleanly.
    CleanlyCanceled,
    /// The control filled: either a partial-fill row is visible (`chegb="2"` shows
    /// it) or the gateway POSITIVELY REJECTED the cancel while the book scans flat
    /// (a `cannot-cancel-because-filled` signal — the control left the book, KTD1).
    Filled(Vec<String>),
    /// The control (or residue) is still resting — not flat, reconcile.
    StillResting(Vec<String>),
}

/// Classify the control's post-cancel disposition (KTD1 bounded fill-check). Pure
/// over the scan + `cancel_gateway_rejected` (the specific `cannot-cancel-because-
/// filled` signal — the gateway ANSWERED and refused the cancel, an `ApiError`, NOT
/// a transport blip / throttle / session-purge). Only a positive fill signal yields
/// `Filled`: a scanned partial-fill row, or a gateway-rejected cancel on an
/// otherwise-flat book. A cancel that merely errored in transport while the book
/// reads flat is `CleanlyCanceled` — inferring a fill from ANY cancel failure would
/// raise a spurious unrecoverable UNEXPECTED-FILL on a benign blip. Offline-twinnable.
fn classify_control_disposition(
    cancel_gateway_rejected: bool,
    scan: &[T0425OutBlock1],
) -> ControlDisposition {
    match require_flat_and_fill_free(scan) {
        Err(NotClear::Fill(f)) => ControlDisposition::Filled(f),
        Err(NotClear::Resting(o)) => ControlDisposition::StillResting(o),
        Ok(()) => {
            if cancel_gateway_rejected {
                // The gateway positively refused to cancel a control that is NOT
                // resting → it left the book as a fill (cannot-cancel-because-filled).
                ControlDisposition::Filled(vec![])
            } else {
                ControlDisposition::CleanlyCanceled
            }
        }
    }
}

/// Canonicalize an order number for OWNERSHIP MATCHING only (never for the cancel
/// call, which must use the gateway's exact string). The submit response and the
/// `t0425` scan both deserialize `OrdNo` via `string_or_number`, so the same order
/// can surface as a JSON number `12345` on one and a zero-padded string
/// `"0000012345"` on the other; a raw string-equality owned-set match would then
/// treat the control as foreign and strand it. Numeric ordnos compare by value;
/// a non-numeric ordno falls back to its trimmed form.
fn normalize_ordno(ordno: &str) -> String {
    let t = ordno.trim();
    match t.parse::<u64>() {
        Ok(n) => n.to_string(),
        Err(_) => t.to_string(),
    }
}

/// Select which resting ordnos teardown cancels (R4/KTD, AE4). When the owned set
/// was **fully constructed** (every accepted WAVE-BLOCKED variant surfaced a
/// parseable OrdNo), cancel only owned rows — a foreign row that arrived mid-probe
/// is left untouched. When the owned set is **incomplete** (an accepted variant
/// whose body yielded no OrdNo, or a may-rest transport/5xx/ambiguous outcome),
/// fall back to cancel-every-resting-row so no live order is ever stranded.
/// Ownership is matched on `normalize_ordno` so a representation mismatch between
/// the submit response and the scan cannot mis-classify the control as foreign;
/// the ORIGINAL resting strings are returned (the cancel call needs the exact
/// ordno). Returns `(ordnos_to_cancel, used_fallback)`; pure and offline-twinnable.
fn select_teardown_cancels(
    resting: &[String],
    owned: &BTreeSet<String>,
    owned_fully_constructed: bool,
) -> (Vec<String>, bool) {
    if owned_fully_constructed {
        let owned_norm: BTreeSet<String> = owned.iter().map(|o| normalize_ordno(o)).collect();
        (
            resting
                .iter()
                .filter(|o| owned_norm.contains(&normalize_ordno(o)))
                .cloned()
                .collect(),
            false,
        )
    } else {
        (resting.to_vec(), true)
    }
}

/// Build the single-symbol working-orders (flatness) request, **unfilled-only**
/// (`chegb="2"`, KTD1). Reverted from the §26 fill-inclusive `chegb="0"`: on a
/// heavily-traded paper symbol `chegb="0"` returns the entire accumulated order
/// history, sets a continuation, and the single-page guard (below) fail-closed at
/// pre-assert-flat *before any control was placed*
/// (`docs/solutions/logic-errors/order-probe-fill-inclusive-scan-paginates-false-held.md`).
/// `chegb="2"` keeps the working set to a single page (resting + partial-fill rows;
/// it excludes a fully-filled `ordrem==0` row by construction). That reduction is
/// accepted: the scan positively confirms only *not-resting*, not *no-fill* — the
/// non-marketable band-floor control price + the WAVE-BLOCKED tripwire carry
/// fill-safety, and the bounded post-cancel ordno fill-check
/// (`classify_control_disposition`) is the defense-in-depth for a fully-filled
/// control. `chegb` value semantics per
/// `docs/solutions/integration-issues/ls-gateway-t0425-rate-limit-and-pagination-flat-scan.md`
/// (the LS API-doc reference; the normalized baseline records `chegb` only as a
/// 1-char String with no value meanings). Pure so the offline twin can assert the
/// reverted class without a live call.
fn working_orders_request(symbol: &str) -> T0425Request {
    T0425Request {
        inblock: T0425InBlock {
            expcode: symbol.into(),
            chegb: "2".into(),
            medosu: "0".into(),
            sortgb: "2".into(),
            cts_ordno: " ".into(),
        },
        tr_cont: String::new(),
        tr_cont_key: String::new(),
    }
}

/// `true` if a `t0425` working-orders response is the **terminal** page — the
/// body cursor carries no real continuation. t0425 self-paginates on the
/// `cts_ordno` body cursor, NOT the `tr_cont` header: the gateway sets
/// `tr_cont="0"` on *any* non-empty page, so gating single-page-ness on the header
/// fail-closes the instant the probe places its own control row (KTD-1; see
/// `docs/solutions/logic-errors/order-probe-fill-inclusive-scan-paginates-false-held.md`).
/// Terminal when the cursor is empty / `" "` / the numeric-default `"0"`; paginated only
/// when it carries a real order-number continuation cursor. The nautilus runtime's own
/// terminality checks (`adapters/nautilus/src/execution.rs` `verify_flat` and
/// `orders/poll.rs`) use the stricter `cts_ordno.trim().is_empty()`; this probe helper
/// **additionally** treats `"0"` as terminal per KTD-1's "all-default" — safe because no
/// real order number is `0`, so `"0"` can never be a genuine continuation cursor. That is
/// a deliberate difference, not a mirror: empirically the gateway emits an *empty* terminal
/// cursor (the live SC-certify run's poll concluded on 005930 without perpetual reconcile),
/// so the `"0"` branch is defensive belt-and-braces. Pure so the offline twin can assert it
/// without a live t0425 call.
fn scan_page_is_terminal(cts_ordno: &str) -> bool {
    let cursor = cts_ordno.trim();
    cursor.is_empty() || cursor == "0"
}

/// Run the `t0425` working-orders scan for the traded symbol (KTD1), **unfilled-only**
/// (`chegb="2"`, see `working_orders_request`), single page, with a 1500ms pre-pace so
/// the per-TR budget refills. Returns `Err` on any failure (treated as NOT flat —
/// positive confirmation only). Sound only paired with the pre-assert-flat, which
/// proves no foreign resting row is present to misattribute.
async fn scan_symbol_working_orders(
    sdk: &LsSdk,
    symbol: &str,
) -> Result<Vec<T0425OutBlock1>, String> {
    let req = working_orders_request(symbol);
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    match sdk.orders().inquiry(&req).await {
        Ok(resp) => {
            // Single-page-ness is decided on the `cts_ordno` body cursor (KTD-1),
            // NOT the `tr_cont` header (which reads `"0"` on any non-empty page and
            // would false-HELD the instant this probe's own control row is present).
            if !scan_page_is_terminal(&resp.outblock.cts_ordno) {
                return Err(
                    "traded-symbol t0425 working-order scan is paginated (cts_ordno cursor \
                     carries a continuation) — a single page cannot positively confirm flat"
                        .to_string(),
                );
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

/// The disposition of a fired variant whose gateway ANSWERED (`fire_inblock` →
/// `Some((http, rsp_cd, ..))`). The transport-failure (`None`) arm is separate and
/// always may-rest.
#[derive(Debug, PartialEq, Eq)]
enum FiredVariantOutcome {
    /// The request placed nothing — a non-ack business rejection (any non-success
    /// 2xx/4xx) OR a gateway INGRESS input-validation reject (`IGW40011`-at-500).
    /// Classify Clean and CONTINUE firing variants.
    PlacedNothing,
    /// The gateway may have accepted before failing (a non-`IGW40011` 5xx) — HALT,
    /// reconcile, fail-closed (KTD3).
    MayHaveRested,
    /// A 2xx order-acceptance ack — a malformed variant was ACCEPTED → WAVE BLOCKED.
    Accepted,
}

/// Classify a fired variant's answered `(http, rsp_cd)` (KTD3). Pure so the offline
/// twin can assert the exemption without a live fire.
///
/// The subtlety this encodes: order endpoints CAN place, so the order probe treats a
/// `5xx` as may-have-rested by default (unlike the read probe, which has no may-rest
/// arm). But `IGW40011` — a numeric request field sent as a string — is a gateway
/// **ingress** input-validation reject that arrives as `http=500`; it is refused BEFORE
/// routing to the exchange, so it structurally placed nothing and the differential can
/// CONTINUE. That "which code is a placed-nothing ingress reject" decision is the shared
/// `ls_core::is_ingress_validation_reject` source of truth, so this probe and the live
/// order path (`ls_core::inner::dispatch_once`) can never disagree. Every OTHER `5xx`
/// stays may-rest/halt; a 2xx ack trips the WAVE-BLOCKED tripwire, unchanged.
fn classify_fired_variant(http: u16, rsp_cd: &str) -> FiredVariantOutcome {
    if is_order_placement_success(http, rsp_cd) {
        FiredVariantOutcome::Accepted
    } else if http >= 500 && !ls_core::is_ingress_validation_reject(rsp_cd) {
        FiredVariantOutcome::MayHaveRested
    } else {
        FiredVariantOutcome::PlacedNothing
    }
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

/// Best-effort symbol-scoped reconcile + cancel of resting residue (R4/KTD): the
/// may-rest / final teardown. Cancels the resting rows `select_teardown_cancels`
/// picks — **owned rows only** when `owned_fully_constructed` (leaving a foreign
/// mid-probe row untouched, AE4), or **every resting row** as a loud fallback when
/// the owned set is incomplete (never strand a live order). A scan that fails or
/// paginates is surfaced loudly (`reconcile-scan failed`) rather than silently
/// trusted, so teardown is best-effort — an operator must confirm the book is flat
/// on a scan failure, and a partial fill is reported as unrecoverable.
async fn order_reconcile_teardown(
    sdk: &LsSdk,
    symbol: &str,
    owned: &BTreeSet<String>,
    owned_fully_constructed: bool,
) {
    match scan_symbol_working_orders(sdk, symbol).await {
        Ok(rows) => {
            let is_resting = |r: &T0425OutBlock1| parse_qty(&r.cheqty) == 0 && parse_qty(&r.ordrem) > 0;
            let resting: Vec<String> = rows
                .iter()
                .filter(|r| is_resting(r))
                .map(|r| r.ordno.trim().to_string())
                .collect();
            let (to_cancel, used_fallback) =
                select_teardown_cancels(&resting, owned, owned_fully_constructed);
            if used_fallback {
                println!(
                    "NEG-PROBE reconcile FALLBACK: owned set incomplete — canceling EVERY resting \
                     row (may include a foreign order) to guarantee no stranded order"
                );
            }
            for r in rows
                .iter()
                .filter(|r| is_resting(r) && to_cancel.contains(&r.ordno.trim().to_string()))
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
            // R4 stranded-order backstop: on the owned-only path a resting row NOT
            // selected for cancel is either a foreign mid-probe row (left untouched by
            // design, AE4) OR a probe order the ack-set classifier failed to recognize.
            // The two are indistinguishable here, so surface every uncanceled resting
            // row LOUDLY — never silently strand one. (On the fallback path every
            // resting row is already being canceled.)
            if !used_fallback {
                let unowned: Vec<String> = resting
                    .iter()
                    .filter(|o| !to_cancel.contains(*o))
                    .cloned()
                    .collect();
                if !unowned.is_empty() {
                    println!(
                        "NEG-PROBE reconcile UNOWNED-RESTING ordnos=[{}] left uncanceled — foreign \
                         mid-probe row (spared, AE4) OR an unrecognized probe order; operator must \
                         reconcile the book",
                        unowned.join(",")
                    );
                }
            }
            // Teardown fill alarm (KTD1): a *partial* fill still surfaces under
            // `chegb="2"` (cheqty>0, ordrem>0) and is uncancelable. Scope it to the
            // rows this teardown is responsible for — owned rows on the owned-only
            // path (a foreign partial fill is not the probe's to alarm on), every row
            // on the fallback path — so a foreign fill is not mis-attributed to the probe.
            let owned_norm: BTreeSet<String> = owned.iter().map(|o| normalize_ordno(o)).collect();
            let fill_scope: Vec<T0425OutBlock1> = if used_fallback {
                rows.clone()
            } else {
                rows.iter()
                    .filter(|r| owned_norm.contains(&normalize_ordno(r.ordno.trim())))
                    .cloned()
                    .collect()
            };
            if let Err(NotClear::Fill(f)) = require_flat_and_fill_free(&fill_scope) {
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

    // R4/AE4: the OWNED set — every order number this probe placed or had accepted.
    // Teardown cancels owned rows only (leaving a foreign mid-probe row untouched)
    // when the set is fully constructed; a may-rest / unsurfaced-accept path passes
    // `owned_fully_constructed=false` to force the cancel-every-resting-row fallback.
    let mut owned: BTreeSet<String> = BTreeSet::new();

    // PRE-ASSERT-FLAT (KTD1): the symbol must be Flat AND fill-free BEFORE we place
    // the control. Scan `chegb="2"` (unfilled-only, single page); if any resting
    // `005930` row (or a still-visible partial fill) exists, HELD — refuse to place.
    // Do NOT teardown here: a non-flat pre-state means a FOREIGN row is present (or a
    // stranded control from a prior leg — the operator clears it between legs), which
    // the probe must not cancel. This proven-clean baseline is what makes the owned
    // teardown sound: every resting row the probe later owns is the probe's by
    // construction.
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
            // Ambiguous submit: a phantom order may rest with an OrdNo we never got —
            // owned set is NOT trustworthy → fall back to unconditional teardown.
            order_reconcile_teardown(&sdk, symbol, &owned, false).await;
            return;
        }
    };
    if placed_ordno.trim().is_empty() || placed_ordno.trim() == "0" {
        println!(
            "NEG-PROBE target={tr_cd}-negative HELD: control returned no usable order number"
        );
        // No usable control OrdNo — same as an ambiguous submit: force the fallback.
        order_reconcile_teardown(&sdk, symbol, &owned, false).await;
        return;
    }
    // The control is placed and RESTING — record it as owned (its OrdNo is known).
    owned.insert(placed_ordno.trim().to_string());
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
    // to a marketable price), the modify seed is band-floor+1 tick, and the bounded
    // post-cancel fill-check (classify_control_disposition) covers a fully-filled control.
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
            // Transport failure / timeout = MAY-REST: stop, reconcile, halt (KTD3). The
            // variant may have rested with an OrdNo we never got — force the fallback.
            None => {
                println!(
                    "NEG-PROBE target={tr_cd}-negative variant field={} class={} \
                     result=[transport-failure] outcome=Held-may-rest halt=true",
                    v.field, v.class
                );
                order_reconcile_teardown(&sdk, symbol, &owned, false).await;
                return;
            }
            Some((http, rsp_cd, ord_no)) => match classify_fired_variant(http, &rsp_cd) {
                // A non-IGW40011 5xx is MAY-REST — the gateway may have accepted before
                // failing. IGW40011-at-500 is exempt (an ingress input-validation reject
                // that placed nothing) and routes to `PlacedNothing` below, so the type
                // variant no longer false-halts the differential (KTD1/KTD2).
                FiredVariantOutcome::MayHaveRested => {
                    println!(
                        "NEG-PROBE target={tr_cd}-negative variant field={} class={} \
                         result=[http={http} rsp_cd={rsp_cd}] outcome=Held-may-rest halt=true",
                        v.field, v.class
                    );
                    order_reconcile_teardown(&sdk, symbol, &owned, false).await;
                    return;
                }
                // A malformed variant was ACCEPTED — do NOT classify; teardown + block.
                // R3/R4: surface the accepted OrdNo into the owned set so teardown
                // cancels exactly it (AE4). If the body yielded no OrdNo the owned set
                // is incomplete → force the cancel-every-resting-row fallback so the
                // accepted-but-unsurfaced order is never stranded.
                FiredVariantOutcome::Accepted => {
                    let owned_fully_constructed = match &ord_no {
                        Some(o) => {
                            owned.insert(o.clone());
                            true
                        }
                        None => false,
                    };
                    println!(
                        "NEG-PROBE target={tr_cd}-negative WAVE BLOCKED pending investigation: \
                         variant field={} class={} was ACCEPTED [http={http} rsp_cd={rsp_cd} \
                         ordno={}]",
                        v.field,
                        v.class,
                        ord_no.as_deref().unwrap_or("<unsurfaced>")
                    );
                    order_reconcile_teardown(&sdk, symbol, &owned, owned_fully_constructed).await;
                    return;
                }
                // Placed nothing (a non-success rsp_cd, incl. IGW40011-at-500) = Clean.
                FiredVariantOutcome::PlacedNothing => {
                    let outcome = classify_probe(control_ok, VariantVerdict::Rejected);
                    println!(
                        "NEG-PROBE target={tr_cd}-negative variant field={} class={} \
                         result=[http={http} rsp_cd={rsp_cd}] outcome={outcome:?}",
                        v.field, v.class
                    );
                }
            },
        }
    }

    // Variants fired against the LIVE control. Now CANCEL the control and run the
    // bounded post-cancel fill-check (KTD1): `classify_control_disposition` combines
    // the `chegb="2"` scan with whether the gateway POSITIVELY REJECTED the cancel.
    // Only a positive fill signal (a scanned partial fill, or a gateway-rejected
    // cancel on an otherwise-flat book) routes to the unrecoverable UNEXPECTED-FILL
    // HELD path — a cancel that merely errored in transport is not read as a fill.
    let cancel = CSPAT00801Request::new(placed_ordno.trim(), symbol, "1");
    let cancel_gateway_rejected = match sdk.orders().cancel(&cancel).await {
        Ok(_) => false,
        Err(e) => {
            // A gateway ApiError = the broker ANSWERED and refused (a genuine
            // cannot-cancel signal); any other error is transport/ambiguous and must
            // NOT be read as a fill.
            let rejected = matches!(e, LsError::ApiError { .. });
            println!(
                "NEG-PROBE target={tr_cd}-negative control-cancel error [{}] \
                 (gateway_rejected={rejected}) — bounded fill-check decides",
                safe_err(&e)
            );
            rejected
        }
    };
    match scan_symbol_working_orders(&sdk, symbol).await {
        Ok(rows) => match classify_control_disposition(cancel_gateway_rejected, &rows) {
            ControlDisposition::CleanlyCanceled => println!(
                "NEG-PROBE target={tr_cd}-negative \
                 control=[placed+variants-fired+canceled ok=true flat=confirmed]"
            ),
            ControlDisposition::Filled(f) => {
                println!(
                    "NEG-PROBE target={tr_cd}-negative HELD: UNEXPECTED-FILL ordnos=[{}] after \
                     variants (gateway_rejected={cancel_gateway_rejected}) — a fill cannot be \
                     canceled; reset the paper book — reconciling",
                    f.join(",")
                );
                order_reconcile_teardown(&sdk, symbol, &owned, true).await;
                return;
            }
            ControlDisposition::StillResting(_) => {
                println!(
                    "NEG-PROBE target={tr_cd}-negative HELD: control not positively flat after \
                     cancel — reconciling"
                );
                order_reconcile_teardown(&sdk, symbol, &owned, true).await;
                return;
            }
        },
        Err(e) => {
            println!(
                "NEG-PROBE target={tr_cd}-negative HELD: control flat-verify failed [{e}] — \
                 reconciling"
            );
            order_reconcile_teardown(&sdk, symbol, &owned, true).await;
            return;
        }
    }

    // Final flat-verify + cancel any residue — never leave a resting order. Owned-only
    // teardown (R4/AE4): the control (owned) was just canceled so it is gone; any
    // remaining owned row is canceled, and a FOREIGN row that arrived mid-probe is left
    // untouched. The owned set is fully constructed on this path — a WAVE-BLOCKED accept
    // or a may-rest outcome would have returned early with its own teardown.
    order_reconcile_teardown(&sdk, symbol, &owned, true).await;
    println!(
        "NEG-PROBE target={tr_cd}-negative teardown=done \
         note=[variants fired against live control; control canceled+flat-verified post-variants; \
         residue reconciled owned-only]"
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
fn working_orders_scan_request_is_unfilled_only_single_page() {
    // R1/R5/KTD1: the flatness scan request is reverted to unfilled-only `chegb="2"`
    // (single page on a traded-history symbol), NOT the §26 fill-inclusive `chegb="0"`
    // that paginated and false-HELD before placing. Fill-detection is a SEPARATE bounded
    // path (`classify_control_disposition`), not this scan. Asserted on the request
    // builder — no live call.
    let req = working_orders_request("005930");
    assert_eq!(
        req.inblock.chegb, "2",
        "flatness scan must be unfilled-only (chegb=2, single page), not fill-inclusive chegb=0"
    );
    assert_eq!(req.inblock.expcode, "005930", "scan stays symbol-scoped");
    assert_eq!(req.inblock.cts_ordno, " ", "first-page cursor keeps the scan single-page");
}

#[test]
fn scan_page_terminality_keys_on_the_cts_ordno_body_cursor_not_tr_cont() {
    // KTD-1: single-page-ness is decided on the `cts_ordno` body cursor, NOT the
    // `tr_cont` header. The §27 root cause is that the gateway sets `tr_cont="0"` on
    // ANY non-empty page, so the header-gated guard false-HELD the moment the probe's
    // own control row was resting. Terminal on empty / `" "` / the numeric-default
    // `"0"`; paginated only on a real order-number continuation cursor.
    // Terminal (single page → confirms flat/working-set):
    assert!(scan_page_is_terminal(""), "empty cursor is terminal (all-default/omitted block)");
    assert!(scan_page_is_terminal(" "), "the first-page sentinel cursor is terminal");
    assert!(scan_page_is_terminal("   "), "whitespace-only cursor is terminal");
    assert!(scan_page_is_terminal("0"), "numeric-default `0` cursor is terminal (no order is #0)");
    // Paginated (a genuine continuation cursor → fail-closed, cannot confirm flat):
    assert!(
        !scan_page_is_terminal("20240001"),
        "a real order-number continuation cursor must be treated as paginated (fail-closed)"
    );
    assert!(!scan_page_is_terminal(" 12345 "), "a trimmed real cursor is still paginated");
}

#[test]
fn classify_fired_variant_exempts_igw40011_at_500_but_holds_other_5xx() {
    // R1/R2/R3/KTD2: the order type-variant differential must CONTINUE past an
    // IGW40011-at-500 (a gateway ingress input-validation reject that placed nothing),
    // while every OTHER 5xx stays may-rest/halt and a 2xx ack still trips WAVE-BLOCKED.
    // IGW40011-at-500 → placed nothing → Clean, continue (the fix).
    assert_eq!(
        classify_fired_variant(500, "IGW40011"),
        FiredVariantOutcome::PlacedNothing
    );
    // Any OTHER 5xx → may-rest → halt (fail-closed default preserved).
    assert_eq!(
        classify_fired_variant(500, "IGW50008"),
        FiredVariantOutcome::MayHaveRested
    );
    assert_eq!(
        classify_fired_variant(503, "IGW00201"),
        FiredVariantOutcome::MayHaveRested
    );
    // Adversarial precedence: a 5xx that happens to carry an order-ack code is NOT an
    // acceptance (ack requires a 2xx) — it stays may-rest/halt, fail-closed.
    assert_eq!(
        classify_fired_variant(500, "00040"),
        FiredVariantOutcome::MayHaveRested
    );
    // A 2xx order-acceptance ack → a malformed variant was ACCEPTED → WAVE BLOCKED.
    assert_eq!(
        classify_fired_variant(200, "00040"),
        FiredVariantOutcome::Accepted
    );
    // A non-ack business rejection (2xx or 4xx) → placed nothing → Clean, continue.
    assert_eq!(
        classify_fired_variant(200, "40510"),
        FiredVariantOutcome::PlacedNothing
    );
    assert_eq!(
        classify_fired_variant(400, "40510"),
        FiredVariantOutcome::PlacedNothing
    );
}

#[test]
fn classify_control_disposition_keys_on_a_positive_fill_signal_only() {
    // R2/KTD1: the bounded post-cancel fill-check reports Filled ONLY on a positive
    // signal — a scanned partial fill, or a gateway-REJECTED cancel on a flat book.
    // The arg is `cancel_gateway_rejected` (the gateway answered+refused), NOT any
    // cancel failure: a transport blip must not raise a spurious UNEXPECTED-FILL.
    let row = |ordno: &str, cheqty: &str, ordrem: &str| T0425OutBlock1 {
        ordno: ordno.into(),
        cheqty: cheqty.into(),
        ordrem: ordrem.into(),
        ..Default::default()
    };
    // Cancel not gateway-rejected (acked, OR merely transport-failed) + nothing
    // resting → cleanly canceled. This is the fix: a benign cancel failure is NOT a fill.
    assert_eq!(
        classify_control_disposition(false, &[]),
        ControlDisposition::CleanlyCanceled
    );
    // Gateway POSITIVELY rejected the cancel + nothing resting → the control left the
    // book as a fill (cannot-cancel-because-filled), not a clean pass.
    assert_eq!(
        classify_control_disposition(true, &[]),
        ControlDisposition::Filled(vec![])
    );
    // A partial fill still surfaces under chegb="2" (cheqty>0) → Filled regardless of cancel.
    assert_eq!(
        classify_control_disposition(false, &[row("9", "2", "0")]),
        ControlDisposition::Filled(vec!["9".into()])
    );
    // A resting remainder → still resting (reconcile).
    assert_eq!(
        classify_control_disposition(false, &[row("7", "0", "5")]),
        ControlDisposition::StillResting(vec!["7".into()])
    );
}

#[test]
fn extract_ord_no_finds_nested_outblock_key_and_skips_aux_keys() {
    // R3: the live OrdNo lives under a per-TR OutBlock key (CSPAT00601OutBlock2.OrdNo),
    // not top-level; the extractor descends and matches only the exact `OrdNo` key.
    let body = serde_json::json!({
        "rsp_cd": "00040",
        "CSPAT00601OutBlock2": { "OrdNo": 123456, "SpareOrdNo": 999, "RsvOrdNo": 888 }
    });
    assert_eq!(extract_ord_no(&body), Some("123456".to_string()));
    // A string OrdNo is trimmed; SpareOrdNo/RsvOrdNo are never matched.
    let body_str = serde_json::json!({ "X": { "OrdNo": " 42 ", "SpareOrdNo": "7" } });
    assert_eq!(extract_ord_no(&body_str), Some("42".to_string()));
    // No OrdNo (or zero / empty) → None: the owned set cannot be constructed.
    assert_eq!(extract_ord_no(&serde_json::json!({ "rsp_cd": "40510" })), None);
    assert_eq!(extract_ord_no(&serde_json::json!({ "B": { "OrdNo": 0 } })), None);
    assert_eq!(extract_ord_no(&serde_json::json!({ "B": { "OrdNo": "" } })), None);
}

#[test]
fn select_teardown_cancels_owns_only_when_fully_constructed_else_falls_back() {
    // R4/AE4: a fully-constructed owned set cancels ONLY owned rows (a foreign row is
    // left untouched); an incomplete owned set falls back to cancel-every-resting-row so
    // no live order is stranded.
    let resting = vec!["OWNED1".to_string(), "FOREIGN".to_string(), "OWNED2".to_string()];
    let owned: BTreeSet<String> = ["OWNED1".to_string(), "OWNED2".to_string()].into_iter().collect();
    // Fully constructed: only the owned rows are selected; the foreign row is spared.
    let (to_cancel, fallback) = select_teardown_cancels(&resting, &owned, true);
    assert!(!fallback, "owned-only, not fallback");
    assert_eq!(to_cancel, vec!["OWNED1".to_string(), "OWNED2".to_string()]);
    assert!(!to_cancel.contains(&"FOREIGN".to_string()), "foreign row left untouched (AE4)");
    // Incomplete owned set: fall back to every resting row (never strand a live order).
    let (to_cancel_fb, fallback_fb) = select_teardown_cancels(&resting, &owned, false);
    assert!(fallback_fb, "incomplete owned set forces the fallback");
    assert_eq!(to_cancel_fb, resting, "fallback cancels every resting row");
}

#[test]
fn owned_matching_survives_ordno_representation_mismatch() {
    // The control OrdNo can surface zero-padded on one response and bare on another
    // (string_or_number). Ownership must match on the normalized value, and the
    // ORIGINAL scan string is returned for the cancel call.
    assert_eq!(normalize_ordno("0000012345"), "12345");
    assert_eq!(normalize_ordno(" 12345 "), "12345");
    assert_eq!(normalize_ordno("O-7"), "O-7", "non-numeric falls back to trimmed");
    // owned holds the zero-padded submit-response form; the scan returns the bare form.
    let owned: BTreeSet<String> = ["0000012345".to_string()].into_iter().collect();
    let resting = vec!["12345".to_string(), "0000067890".to_string()];
    let (to_cancel, fallback) = select_teardown_cancels(&resting, &owned, true);
    assert!(!fallback);
    assert_eq!(
        to_cancel,
        vec!["12345".to_string()],
        "the control is recognized as owned despite the padding mismatch, and its \
         original scan string is returned for the cancel"
    );
}

#[test]
fn gateway_tolerant_downgrade_fires_only_on_marked_class() {
    // U2/R9/R10/R11/AE1/AE2: the per-class `gateway_tolerant` downgrade is offline-twinnable.
    // t8412 carries the most pairs; each split-facet TR is exercised at its real schema.
    let t8412 = ls_core::schema_for("t8412").expect("t8412 schema");
    // shcode marked [required] → required downgrades, format does NOT (AE2).
    assert!(is_gateway_tolerant(t8412, "shcode", "required"));
    assert!(!is_gateway_tolerant(t8412, "shcode", "format"), "AE2: only the marked class");
    // sdate/edate marked [format] (R11 spans required + format).
    assert!(is_gateway_tolerant(t8412, "sdate", "format"));
    assert!(is_gateway_tolerant(t8412, "edate", "format"));
    // ncnt carries no facet → never downgrades.
    assert!(!is_gateway_tolerant(t8412, "ncnt", "required"));
    // nday marked [required] (§30 live re-probe unmasked it once paced — the gateway
    // accepts its removal; preflight still enforces as a caller contract).
    assert!(is_gateway_tolerant(t8412, "nday", "required"));
    // sdate/edate cross_field marked gateway_tolerant (§30: gateway accepts start>end).
    // The pseudo-field the per-field lookup never carries resolves via the cross_field
    // rule instead.
    assert!(
        is_gateway_tolerant(t8412, "sdate/edate", "cross_field"),
        "§30: the marked sdate/edate date_order rule downgrades its accepted violation"
    );
    // A per-field date class on sdate is unmarked → not tolerant (only the joint
    // ordering is, not either endpoint's own format/required).
    assert!(!is_gateway_tolerant(t8412, "sdate/edate", "required"), "only the cross_field class");
    // t1102 shcode + exchgubun required; t0425 chegb required (R10).
    let t1102 = ls_core::schema_for("t1102").expect("t1102 schema");
    assert!(is_gateway_tolerant(t1102, "shcode", "required"));
    assert!(is_gateway_tolerant(t1102, "exchgubun", "required"));
    let t0425 = ls_core::schema_for("t0425").expect("t0425 schema");
    assert!(is_gateway_tolerant(t0425, "chegb", "required"));
    // medosu marked [required] (§30 live re-probe: sibling of chegb, gateway accepts removal).
    assert!(is_gateway_tolerant(t0425, "medosu", "required"));
    // sortgb stays unmarked — the gateway genuinely enforces it (IGW40013 on removal).
    assert!(!is_gateway_tolerant(t0425, "sortgb", "required"), "sortgb is gateway-enforced");
    // CSPAQ12200 BalCreTp marked [required] (§30 live re-probe: merits 00136 on removal).
    let cspaq12200 = ls_core::schema_for("CSPAQ12200").expect("CSPAQ12200 schema");
    assert!(is_gateway_tolerant(cspaq12200, "BalCreTp", "required"));
    // An unmarked TR never downgrades (unchanged behavior for every other TR).
    let t1101 = ls_core::schema_for("t1101").expect("t1101 schema");
    assert!(!is_gateway_tolerant(t1101, "shcode", "required"));

    // The reported-outcome layer: only a Divergent on a marked (field, class) becomes
    // expected-tolerant; Clean/Held/an unmarked Divergent render verbatim.
    assert_eq!(
        reported_outcome(t8412, "shcode", "required", ProbeOutcome::Divergent),
        "expected-tolerant"
    );
    assert_eq!(
        reported_outcome(t8412, "shcode", "format", ProbeOutcome::Divergent),
        "Divergent",
        "AE2: an accepted malformed shcode still reports a divergence"
    );
    assert_eq!(
        reported_outcome(t8412, "shcode", "required", ProbeOutcome::Clean),
        "Clean",
        "a Clean is never downgraded"
    );
    assert_eq!(
        reported_outcome(t1101, "shcode", "required", ProbeOutcome::Divergent),
        "Divergent",
        "an unmarked TR's divergence is never downgraded"
    );

    // Cross-field tolerance (§30): the marked t8412 sdate/edate ordering downgrades a
    // Divergent to expected-tolerant, exactly like a per-field pair.
    assert_eq!(
        reported_outcome(t8412, "sdate/edate", "cross_field", ProbeOutcome::Divergent),
        "expected-tolerant"
    );
    // Negative anchor: the SAME date_order rule with the flag OFF still reports
    // Divergent — the downgrade fires only on the marked rule, never on cross_field
    // as a class.
    let unmarked = ConstraintSchema {
        tr_code: "TEST".into(),
        fields: vec![],
        cross_field: vec![CrossFieldRule::DateOrder {
            start: "sdate".into(),
            end: "edate".into(),
            confirmed: false,
            gateway_tolerant: false,
        }],
    };
    assert!(!is_gateway_tolerant(&unmarked, "sdate/edate", "cross_field"));
    assert_eq!(
        reported_outcome(&unmarked, "sdate/edate", "cross_field", ProbeOutcome::Divergent),
        "Divergent",
        "an unmarked cross_field rule's divergence is never downgraded"
    );
}

#[test]
fn require_flat_and_fill_free_gates_all_three_scan_consumers() {
    // KTD1: the SAME pure decision feeds all three `chegb="2"` scan consumers —
    // pre-assert-flat (refuse to place), post-cancel flat-verify (HELD), and teardown
    // (UNEXPECTED-FILL alarm). Under chegb="2" the Fill arm still fires on a *partial*
    // fill (cheqty>0, ordrem>0, which stays on the working set); a fully-filled control
    // is caught by the bounded post-cancel `classify_control_disposition` instead.
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
    // A FILL (cheqty>0) → NotClear::Fill (uncancelable): pre-assert refuses ("reset the
    // paper book"), post-cancel routes to UNEXPECTED-FILL HELD, teardown raises the alarm.
    // Under the reverted chegb="2" scan this arm fires on a *partial* fill (which stays on
    // the working set); a fully-filled control (ordrem==0) is hidden from chegb="2" and is
    // caught by the bounded post-cancel classify_control_disposition instead (KTD1). The
    // pure function still classifies any cheqty>0 row as Fill regardless.
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
