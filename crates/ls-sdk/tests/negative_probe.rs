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
    classify_probe, generate_invalid_variants, is_noneval_code, is_read_merits_reject,
    ConstraintSchema, CrossFieldRule, InvalidVariant, LsConfig, LsError, LsResult, ProbeOutcome,
    VariantVerdict,
};
use ls_sdk::account::T0424Request;
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
    outcome_label(outcome)
}

/// The verbatim render label for a raw `ProbeOutcome` (no tolerance/throttle
/// qualifier). Shared by `reported_outcome` (reads, which layer the
/// `expected-tolerant` downgrade on top) and the token leg (which has no schema
/// and renders verbatim), so the base three labels cannot drift between legs.
fn outcome_label(outcome: ProbeOutcome) -> &'static str {
    match outcome {
        ProbeOutcome::Clean => "Clean",
        ProbeOutcome::Held => "Held",
        ProbeOutcome::Divergent => "Divergent",
    }
}

/// The reason-qualified `Held-throttle` label for a non-EVALUATION (KTD1) — a
/// variant whose `rsp_cd` is a catalogued noneval code (`is_noneval_code`,
/// `{IGW00201}`) fired against a PASSING control. Returns `None` when the result
/// is not a throttle-`Held`, so the caller renders its normal label.
///
/// Deliberately keyed on `is_noneval_code`, NOT on `VariantVerdict::Inconclusive`
/// in general: an *unknown* reject code is also `Inconclusive` (strict inversion),
/// but it renders as a plain `Held` distinguished by its printed `rsp_cd` — only a
/// catalogued throttle earns the reason-qualified `Held-throttle` label. A
/// control-fail `Held` is likewise NOT a throttle-`Held` (the throttle detail is
/// moot when the control itself failed). Mirrors the order path's `Held-may-rest`
/// reason-qualified label; shared by the read and token legs so the throttle label
/// cannot drift between them (KTD5).
fn throttle_label(control_ok: bool, rsp_cd: &str) -> Option<&'static str> {
    (control_ok && is_noneval_code(rsp_cd)).then_some("Held-throttle")
}

/// Derive the READ-leg [`VariantVerdict`] from a gateway response (KTD2 read arm /
/// KTD5 shared derivation). Strict merits-allowlist inversion, in this order:
///
/// - `2xx && is_success(rsp_cd)` → `Accepted` (the gateway accepted the invalid
///   variant — a divergence).
/// - `is_read_merits_reject(rsp_cd)` (`{IGW40011, IGW40013}`) → `Rejected` (the
///   gateway evaluated it on merits and refused — `Clean`).
/// - everything else (an `IGW00201` throttle, a hard-gateway `IGW50008`, an
///   unknown code) → `Inconclusive` → `Held`, so a non-merits read surfaces loud
///   and re-probeable rather than a silent false-`Clean`.
///
/// The `rsp_cd` carries the merits signal independent of HTTP status — a genuine
/// `IGW40011` ingress reject arrives `http=500` (CONCEPTS "Ambiguous order
/// outcome") — so `Rejected` keys on `rsp_cd`, not the 2xx gate; only `Accepted`
/// requires 2xx. Transport failure (`None`) is handled at the call site as an
/// `Inconclusive` by construction (it never reaches this helper).
fn read_variant_verdict(http: u16, rsp_cd: &str) -> VariantVerdict {
    if (200..300).contains(&http) && is_success(rsp_cd) {
        VariantVerdict::Accepted
    } else if is_read_merits_reject(rsp_cd) {
        VariantVerdict::Rejected
    } else {
        VariantVerdict::Inconclusive
    }
}

/// The full READ-leg render label for one variant result (KTD5): derive the 3-way
/// verdict, classify it against the control, and render. A noneval `Inconclusive`
/// on a passing control renders `Held-throttle`; every other case falls through
/// to the tolerance layer (a `Divergent` on a `gateway_tolerant` (field, class)
/// downgrades to `expected-tolerant`, KTD4). Both the live loop and the offline
/// proxy call THIS function, so the branch ordering — not just the code sets —
/// cannot drift between them.
fn read_reported_label(
    schema: &ConstraintSchema,
    field: &str,
    class: &str,
    control_ok: bool,
    http: u16,
    rsp_cd: &str,
) -> &'static str {
    let verdict = read_variant_verdict(http, rsp_cd);
    throttle_label(control_ok, rsp_cd).unwrap_or_else(|| {
        reported_outcome(schema, field, class, classify_probe(control_ok, verdict))
    })
}

/// Derive the TOKEN-leg [`VariantVerdict`] from a token response (KTD2 token arm /
/// KTD5 shared derivation). The token endpoint fails *structurally differently*
/// from the InBlock reads (`auth.rs`): a genuine OAuth refusal is a non-2xx HTTP
/// status or a 2xx carrying a non-success `{code,message}` envelope — both already
/// collapsed into `ok` by `token_fire` (`ok = 2xx && has access_token`). So the
/// token guard is a **noneval carve-out**, not a merits allowlist:
///
/// - `is_noneval_code(rsp_cd)` (`{IGW00201}`) → `Inconclusive` → `Held` (a throttle
///   the gateway never evaluated).
/// - `ok` → `Accepted` (the gateway accepted an invalid variant — a divergence).
/// - otherwise → `Rejected` (a genuine OAuth refusal — `Clean`).
///
/// Weaker by design than the read allowlist (it catches only catalogued noneval
/// codes) because token's OAuth-refusal vocabulary cannot be allowlisted and is
/// not the throttle-masking motivating case — recorded, accepted (KTD2). Transport
/// failure (`None`) is handled at the call site as an `Inconclusive`.
fn token_variant_verdict(rsp_cd: &str, ok: bool) -> VariantVerdict {
    if is_noneval_code(rsp_cd) {
        VariantVerdict::Inconclusive
    } else if ok {
        VariantVerdict::Accepted
    } else {
        VariantVerdict::Rejected
    }
}

/// The full TOKEN-leg render label for one variant result (KTD5): derive the
/// verdict, classify it against the control, and render. A catalogued noneval code
/// on a passing control renders `Held-throttle` (via the shared `throttle_label`);
/// every other case renders the verbatim outcome label (token carries no schema,
/// so there is no `gateway_tolerant` downgrade). Both the live loop and the
/// offline proxy call THIS function so the branch ordering cannot drift.
fn token_reported_label(control_ok: bool, rsp_cd: &str, ok: bool) -> &'static str {
    let verdict = token_variant_verdict(rsp_cd, ok);
    throttle_label(control_ok, rsp_cd)
        .unwrap_or_else(|| outcome_label(classify_probe(control_ok, verdict)))
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

/// Inter-fire pace for the ORDER differential negative probe (`run_order_negative_probe`,
/// CSPAT006/007/008). The order fire loop dispatches the control submit + every
/// type/required variant against the Orders rate bucket; with no pace they collided
/// and self-inflicted an `IGW00201` throttle mid-run — observed 2026-07-15 on the
/// CSPAT00701 differential, where the 5th back-to-back order dispatch
/// (`OrdQty/required`) tripped `IGW00201` and halted (classified conservatively as
/// `Held-may-rest`) BEFORE the probe ever reached `OrdprcPtnCode` / `OrdCndiTpCode` /
/// `OrdPrc`. Same cumulative warm-budget code and the same fix as the t8412 read leg
/// (`T8412_PROBE_PACE` above): a 1000 ms inter-dispatch pace. Like there, the budget is
/// cumulative so no per-dispatch pace can *guarantee* it stays cool (the live re-probe
/// is the real proof, and the operator can bump this single named const in-window if a
/// warm Orders budget still trips); the Orders bucket is a distinct, tighter bucket than
/// market-data, so this stays its own const. Offline-inert: `run_order_negative_probe`
/// only runs under the attended `#[ignore]` order legs.
const ORDER_PROBE_PACE: Duration = Duration::from_millis(1000);

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
                // U2/KTD2/KTD5: merits-allowlist inversion via the shared
                // `read_reported_label` — a non-merits variant (IGW00201 throttle,
                // hard-gateway, unknown) renders Held-throttle, not a false-Clean.
                let label = read_reported_label(&schema, field, class, control_ok, http, &rsp_cd);
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
                // U3/KTD2/KTD5: token noneval carve-out via the shared
                // `token_reported_label` — an IGW00201 throttle renders Held-throttle;
                // a genuine OAuth refusal (non-2xx / non-success envelope) stays Clean.
                let label = token_reported_label(control_ok, &code, ok);
                println!(
                    "NEG-PROBE target=token-negative variant field={field} class={class} \
                     result=[http={http} rsp_cd={code}] outcome={label}"
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

    /// Marketable SELL price — AT the floor: an aggressive limit that crosses the
    /// book DOWNWARD and fills against resting bids. Mirrors `order_smoke.rs`
    /// `marketable_sell_price` (the paper-reset flatten pattern); used by the
    /// booking A/B sign-aware close-out to flatten a defaulted BUY fill.
    fn marketable_sell_price(&self) -> u64 {
        self.dnlmt
    }

    /// Marketable BUY price — AT the cap: the mirror aggressive limit that crosses
    /// the book UPWARD and fills against resting asks. Used by the booking A/B
    /// sign-aware close-out to buy back a defaulted SELL fill (close-only: the
    /// bought-back qty is exactly the observed delta, never beyond the pre state).
    fn marketable_buy_price(&self) -> u64 {
        self.uplmt
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

/// `true` if `schema` declares — for this exact `(field, class, rsp_cd)` triple —
/// that the fired variant **placed nothing** (Route B, plan 2026-07-14-001). The
/// order-path analogue of [`is_gateway_tolerant`]: a pure per-triple lookup over a
/// field's `placed_nothing_codes` map, so the scoped may-rest→placed-nothing
/// downgrade is offline-twinnable without a live fire.
///
/// Deliberately narrow — it fires ONLY for the exact triple a constraint file
/// declares (a different code, class, or field all miss). It intentionally breaks
/// the line-1206 "probe and live path can never disagree" invariant in the **safe**
/// direction: the probe reads a declared variant placed-nothing (lenient, after a
/// controlled seed+teardown A/B), while the runtime seam
/// (`ls_core::is_ingress_validation_reject`) is **unchanged** and still treats the
/// same code as may-rest (`AmbiguousOrder → reconcile via t0425`). The break — and
/// its bounded sensor-blinding cost (this variant reads CLEAN forever, so a future
/// gateway change to actually-rest this code is undetectable here) — is stated
/// explicitly per the plan's Route B section, not left implicit.
fn order_code_placed_nothing(
    schema: &ConstraintSchema,
    field: &str,
    class: &str,
    rsp_cd: &str,
) -> bool {
    schema
        .fields
        .iter()
        .find(|f| f.name == field)
        .and_then(|f| f.placed_nothing_codes.get(class))
        .is_some_and(|codes| codes.iter().any(|c| c == rsp_cd))
}

/// Resolve a fired order variant's disposition (Route B, plan 2026-07-14-001): the
/// pure composition of the untouched [`classify_fired_variant`] with the scoped
/// [`order_code_placed_nothing`] override. When `classify_fired_variant` yields
/// `MayHaveRested` **and** the schema declares this exact `(field, class, rsp_cd)`
/// a placed-nothing triple, the outcome is downgraded to `PlacedNothing`; every
/// other case defers verbatim to `classify_fired_variant` (Accepted / PlacedNothing
/// / an undeclared MayHaveRested all pass through). Keeping this a pure resolver
/// (rather than inlining the override in the async fire loop) makes the Route-B
/// routing offline-twinnable without a live gateway — resolving the fire-site
/// integration-test gap. `classify_fired_variant` stays pure and untouched (KTD1);
/// the runtime seam stays untouched (KTD4).
fn resolve_fired_outcome(
    schema: &ConstraintSchema,
    field: &str,
    class: &str,
    http: u16,
    rsp_cd: &str,
) -> FiredVariantOutcome {
    match classify_fired_variant(http, rsp_cd) {
        FiredVariantOutcome::MayHaveRested
            if order_code_placed_nothing(schema, field, class, rsp_cd) =>
        {
            FiredVariantOutcome::PlacedNothing
        }
        other => other,
    }
}

/// `true` if `schema` marks this exact `(field, class)` pair **booking-determining**
/// (Route C, §30): the class's violation, when fired live, changes WHAT gets booked
/// rather than whether the request is rejected (CSPAT00601 `BnsTpCode` omission →
/// a direction-defaulted REAL order, ledger §30 ordno=17093). The never-fire
/// analogue of [`is_gateway_tolerant`] / [`order_code_placed_nothing`]: a pure
/// per-pair lookup over a field's `booking_determining` list, so the skip decision
/// is offline-twinnable without a live fire. See
/// `docs/solutions/conventions/order-negative-probe-modify-vs-submit-policy.md`.
fn is_booking_determining(schema: &ConstraintSchema, field: &str, class: &str) -> bool {
    schema
        .fields
        .iter()
        .find(|f| f.name == field)
        .is_some_and(|f| f.booking_determining.iter().any(|c| c == class))
}

/// The Route-C fire-vs-skip decision for one order-probe variant (pure, §30): a
/// variant whose `(field, class)` is marked booking-determining is NEVER dispatched
/// — recorded, not sent — so an annotated variant is structurally unroutable at
/// the fire site, not merely filtered by class. Every other variant fires as
/// before. Extracted from the fire loop so the decision is testable against
/// `generate_invalid_variants` output without a live dispatch.
fn order_variant_may_fire(schema: &ConstraintSchema, v: &InvalidVariant) -> bool {
    !is_booking_determining(schema, &v.field, &v.class)
}

// ===========================================================================
// IGW00000 A/B characterization (plan 2026-07-14-001 U5). A one-shot attended
// seed → snapshot → fire → re-snapshot → cancel cycle that classifies the
// undocumented, success-SHAPED `IGW00000` on CSPAT00701's `OrdprcPtnCode`-omitted
// modify variant as placed-nothing vs may-rest — the U3 runbook encapsulated as a
// deterministic, re-runnable leg. Probe-only: the runtime seam is untouched.
// ===========================================================================

/// A snapshot of the seed/control order's mutable fields, taken from a **trap-free
/// `chegb="2"` working-orders scan** (NOT the fill-inclusive `chegb="0"` query,
/// which paginates on a traded symbol and false-HELDs — see
/// `docs/solutions/logic-errors/order-probe-fill-inclusive-scan-paginates-false-held.md`).
/// Fill-inclusiveness is realized instead by the post-fire cancel disposition: a
/// fully-filled seed vanishes from `chegb="2"` but its cancel is gateway-rejected
/// (`ControlDisposition::Filled`), the plan's named defense-in-depth.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SeedSnapshot {
    present: bool,
    price: String,
    qty: String,
    cheqty: String,
    ordrem: String,
}

/// Build the `SeedSnapshot` for `ordno` from a working-orders scan (pure). Matches
/// on `normalize_ordno` so a JSON-number/zero-padded-string representation mismatch
/// between submit and scan never mis-reads the seed as absent.
fn seed_snapshot_from(rows: &[T0425OutBlock1], ordno: &str) -> SeedSnapshot {
    let want = normalize_ordno(ordno);
    match rows.iter().find(|r| normalize_ordno(&r.ordno) == want) {
        Some(r) => SeedSnapshot {
            present: true,
            price: r.price.trim().to_string(),
            qty: r.qty.trim().to_string(),
            cheqty: r.cheqty.trim().to_string(),
            ordrem: r.ordrem.trim().to_string(),
        },
        None => SeedSnapshot::default(),
    }
}

/// `true` if the scan carries a RESTING order that is NOT the seed `ordno` — a
/// phantom the fired variant may have conjured (pure). Resting = `cheqty==0 &&
/// ordrem>0` (the same predicate teardown uses).
fn has_new_resting_order(rows: &[T0425OutBlock1], ordno: &str) -> bool {
    let want = normalize_ordno(ordno);
    rows.iter().any(|r| {
        normalize_ordno(&r.ordno) != want && parse_qty(&r.cheqty) == 0 && parse_qty(&r.ordrem) > 0
    })
}

/// The IGW00000 A/B verdict (plan 2026-07-14-001 U5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AbVerdict {
    /// fire=`500/IGW00000`, the seed is present & byte-identical, no new order, and it
    /// canceled cleanly → the omitted-pattern modify placed nothing.
    PlacedNothing,
    /// The seed filled, mutated, vanished, or a new order rested → the modify (may
    /// have) taken effect at the exchange.
    MayRest,
    /// An untrusted read (throttle/scan failure), an un-snapshottable pre-state, or a
    /// fire that is not the characterized `500/IGW00000` surface → stays HELD, re-run.
    /// Per #137 an untrusted read is NEVER read as placed-nothing.
    Inconclusive,
}

/// Classify the IGW00000 A/B from the fire result, the before/after seed snapshots,
/// whether a NEW order rested, and the post-fire cancel disposition (the
/// fill-inclusive defense). Pure so the bind signature is offline-twinnable. Tested
/// in the plan's bind-signature order: any decisive may-rest signal wins over a
/// placed-nothing read, and anything ambiguous fails safe to `Inconclusive` — a
/// false `PlacedNothing` (the only dangerous error) is structurally impossible.
fn classify_igw00000_ab(
    fire_http: u16,
    fire_rsp_cd: &str,
    reads_trusted: bool,
    s_pre: &SeedSnapshot,
    s_post: &SeedSnapshot,
    new_resting_order: bool,
    post_fire_disposition: &ControlDisposition,
) -> AbVerdict {
    // Untrusted read, or a control we could not even snapshot pre-fire → inconclusive
    // (never placed-nothing, #137).
    if !reads_trusted || !s_pre.present {
        return AbVerdict::Inconclusive;
    }
    // Decisive may-rest — the seed filled (partial in scan, or cancel gateway-rejected
    // on a flat book): the omitted-pattern modify became marketable and executed.
    if matches!(post_fire_disposition, ControlDisposition::Filled(_)) {
        return AbVerdict::MayRest;
    }
    // Decisive may-rest — a NEW order rested, the seed vanished from a trusted S_post
    // (a vanished seed is may-rest, never inconclusive — plan bind signature), or the
    // seed mutated (any of price/qty/cheqty/ordrem changed).
    if new_resting_order || !s_post.present || s_post != s_pre {
        return AbVerdict::MayRest;
    }
    // Positive placed-nothing: the characterized surface, seed byte-identical & still
    // resting, no new order, and it canceled cleanly (was genuinely just resting).
    if fire_http == 500
        && fire_rsp_cd == "IGW00000"
        && matches!(post_fire_disposition, ControlDisposition::CleanlyCanceled)
    {
        AbVerdict::PlacedNothing
    } else {
        AbVerdict::Inconclusive
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
///
/// Note this filter says only WHICH classes fire — not WHICH order TRs are safe to
/// fire a live `required`-omit variant against. That is the modify-vs-submit policy
/// (`docs/solutions/conventions/order-negative-probe-modify-vs-submit-policy.md`,
/// resolves pending.13 #5): a modify leg (seed + teardown → reversible/observable)
/// is probeable; a submit leg with a booking-determining field (the `BnsTpCode`
/// class → an uncontrolled resting order, no seed to snapshot) is permanently HELD.
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

    // Route B teardown-trust (plan 2026-07-14-001, review P2). A scoped
    // placed-nothing downgrade admits an undocumented, success-shaped code as
    // "placed nothing" on what `classify_fired_variant` alone calls MayHaveRested.
    // The may-rest arm reconciles with the untrusted cancel-EVERY-row fallback; a
    // downgrade skips that and continues to the owned-only post-loop teardown. If
    // the scoped tolerance is ever wrong and the variant rests a NEW (non-owned)
    // order, owned-only teardown would surface-but-not-cancel it. So a wave that
    // used any downgrade forces the post-loop residue teardown back to cancel-all —
    // preserving the may-rest halt's auto-reconcile net for exactly the wave that
    // leaned on the tolerance, while keeping the probe's Clean verdict lenient.
    let mut used_scoped_downgrade = false;

    for v in variants.iter().filter(|v| order_probe_classes(v)) {
        // Route C (§30): a booking-determining (field, class) variant is NEVER
        // dispatched — its live firing changes WHAT gets booked (CSPAT00601
        // `BnsTpCode` omission → a direction-defaulted REAL order), so it is
        // recorded, not sent. No request is constructed and no pace is consumed;
        // the skip is decided by the same pure `order_variant_may_fire` the
        // offline twin asserts, so the fire site and the twin cannot drift.
        if !order_variant_may_fire(&schema, v) {
            println!(
                "NEG-PROBE target={tr_cd}-negative variant field={} class={} \
                 outcome=booking-determining-skip (never fired by design)",
                v.field, v.class
            );
            continue;
        }
        // Pace every order dispatch (ORDER_PROBE_PACE) so the differential does not
        // self-inflict an `IGW00201` throttle and halt mid-run before reaching the
        // later required-omit variants. Paces after the control submit and between
        // every fire; the trailing pace on the final variant is negligible.
        tokio::time::sleep(ORDER_PROBE_PACE).await;
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
            Some((http, rsp_cd, ord_no)) => match resolve_fired_outcome(
                &schema, &v.field, &v.class, http, &rsp_cd,
            ) {
                // A non-IGW40011 5xx is MAY-REST — the gateway may have accepted before
                // failing. IGW40011-at-500 is exempt (an ingress input-validation reject
                // that placed nothing) and routes to `PlacedNothing` below, so the type
                // variant no longer false-halts the differential (KTD1/KTD2).
                //
                // Route B (plan 2026-07-14-001): `resolve_fired_outcome` additionally
                // downgrades a MayHaveRested to PlacedNothing when the schema declares
                // this exact `(field, class, rsp_cd)` a scoped placed-nothing triple
                // (`order_code_placed_nothing`). This DELIBERATELY breaks the line-1206
                // "probe and live path can never disagree" invariant in the SAFE
                // direction — the probe is lenient (after the attended seed+teardown A/B
                // proved placed-nothing) while the runtime seam is untouched (still
                // may-rest/reconcile, KTD4). No triple is declared today (dormant); the
                // CSPAT00701 `OrdprcPtnCode` annotation lands only post-verdict (U6).
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
                    // Detect a Route B scoped downgrade (review P2): `resolve_fired_outcome`
                    // ONLY ever rewrites MayHaveRested→PlacedNothing, so a raw MayHaveRested
                    // reaching this arm means the schema's `placed_nothing_codes` tolerance
                    // fired. Flag it so the post-loop teardown falls back to cancel-all.
                    if classify_fired_variant(http, &rsp_cd) == FiredVariantOutcome::MayHaveRested {
                        used_scoped_downgrade = true;
                        println!(
                            "NEG-PROBE target={tr_cd}-negative variant field={} class={} \
                             note=[Route B scoped placed-nothing downgrade — post-loop teardown \
                             will cancel-all (untrusted owned set) this wave]",
                            v.field, v.class
                        );
                    }
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

    // Post-loop residue teardown trust (review P2): owned-only normally, but
    // cancel-EVERY-row if any variant used a Route B scoped placed-nothing downgrade
    // this wave (the owned set can no longer be trusted to enumerate a stranded
    // order the tolerated variant might have rested).
    let owned_trustworthy = !used_scoped_downgrade;

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
                order_reconcile_teardown(&sdk, symbol, &owned, owned_trustworthy).await;
                return;
            }
            ControlDisposition::StillResting(_) => {
                println!(
                    "NEG-PROBE target={tr_cd}-negative HELD: control not positively flat after \
                     cancel — reconciling"
                );
                order_reconcile_teardown(&sdk, symbol, &owned, owned_trustworthy).await;
                return;
            }
        },
        Err(e) => {
            println!(
                "NEG-PROBE target={tr_cd}-negative HELD: control flat-verify failed [{e}] — \
                 reconciling"
            );
            order_reconcile_teardown(&sdk, symbol, &owned, owned_trustworthy).await;
            return;
        }
    }

    // Final flat-verify + cancel any residue — never leave a resting order. Owned-only
    // teardown (R4/AE4): the control (owned) was just canceled so it is gone; any
    // remaining owned row is canceled, and a FOREIGN row that arrived mid-probe is left
    // untouched. The owned set is fully constructed on this path — a WAVE-BLOCKED accept
    // or a may-rest outcome would have returned early with its own teardown. Exception
    // (review P2): a wave that used a Route B scoped downgrade drops to the cancel-all
    // fallback (`owned_trustworthy=false`) so a stranded order from the tolerated variant
    // is still auto-canceled, not merely surfaced.
    order_reconcile_teardown(&sdk, symbol, &owned, owned_trustworthy).await;
    println!(
        "NEG-PROBE target={tr_cd}-negative teardown=done \
         note=[variants fired against live control; control canceled+flat-verified post-variants; \
         residue reconciled {}]",
        if owned_trustworthy { "owned-only" } else { "cancel-all (Route B downgrade this wave)" }
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

/// The attended IGW00000 A/B leg (plan 2026-07-14-001 U5). Seeds a resting control,
/// snapshots it, fires the `OrdprcPtnCode`-omitted CSPAT00701 modify, re-snapshots,
/// cancels, and prints ONE credential-free `IGW00000-AB … verdict=…` line. Reuses
/// the negative-probe safety spine (autonomy/paper guards, pre-assert-flat, the
/// `chegb="2"` scan, `classify_control_disposition`, `order_reconcile_teardown`);
/// never leaves a resting order. The runtime seam is untouched (probe-only).
async fn run_igw00000_ab_probe() {
    let tag = "IGW00000-AB target=CSPAT00701";
    if let Err(e) = install_dispatch_log_suppressor() {
        panic!("{}", scrub_secrets(&e.to_string()));
    }
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

    // Daily band + three DISTINCT non-marketable prices near the floor:
    // P0 (control placed) < P1 (valid control modify) < P2 (violation OrdPrc). P2 != P1
    // is what makes a took-effect mutation observable as a price change in S_post.
    let band = match sdk
        .market_session()
        .quote(&T1102Request::new(symbol, "K"))
        .await
    {
        Ok(resp) => match validate_band(&resp.outblock.uplmtprice, &resp.outblock.dnlmtprice) {
            Ok(b) => b,
            Err(e) => {
                println!("{tag} verdict=inconclusive [band: {e}] — no placement");
                return;
            }
        },
        Err(e) => {
            println!(
                "{tag} verdict=inconclusive [band fetch failed: {}] — no placement",
                safe_err(&e)
            );
            return;
        }
    };
    let t = tick(band.dnlmt);
    let p0 = band.dnlmt;
    let p1 = band.dnlmt.saturating_add(t).min(band.uplmt);
    let p2 = band.dnlmt.saturating_add(t.saturating_mul(2)).min(band.uplmt);
    if p1 <= p0 || p2 <= p1 {
        println!("{tag} verdict=inconclusive [band too narrow for 3 distinct A/B prices]");
        return;
    }

    let token = match sdk.standalone().token().await {
        Ok(t) if !t.is_empty() => t,
        _ => {
            println!("{tag} verdict=inconclusive [token acquisition failed]");
            return;
        }
    };
    let base = ls_core::config::Environment::resolve_base_url(&config);
    let url = format!("{base}/stock/order");
    let client = probe_client();

    let mut owned: BTreeSet<String> = BTreeSet::new();

    // PRE-ASSERT-FLAT: refuse unless the symbol is flat & fill-free — a foreign or
    // stranded row would poison the owned-teardown soundness (do NOT teardown here).
    match scan_symbol_working_orders(&sdk, symbol).await {
        Ok(rows) => match require_flat_and_fill_free(&rows) {
            Ok(()) => {}
            Err(NotClear::Resting(o)) => {
                println!(
                    "{tag} verdict=inconclusive [pre-assert-flat: resting ordnos=[{}]; clear the \
                     book] — no placement",
                    o.join(",")
                );
                return;
            }
            Err(NotClear::Fill(f)) => {
                println!(
                    "{tag} verdict=inconclusive [pre-assert-flat: fill ordnos=[{}]; paper-reset] — \
                     no placement",
                    f.join(",")
                );
                return;
            }
        },
        Err(e) => {
            println!("{tag} verdict=inconclusive [pre-assert-flat scan failed: {e}] — no placement");
            return;
        }
    }

    // (1) SEED — place the resting control at P0 (band floor, non-marketable).
    let control_req = CSPAT00601Request::limit(symbol, "1", p0.to_string(), "2", member);
    let mut ordno = match sdk.orders().submit(&control_req).await {
        Ok(resp) => resp.order_no().trim().to_string(),
        Err(e) => {
            println!(
                "{tag} verdict=inconclusive [seed submit failed/ambiguous: {}] — reconciling",
                safe_err(&e)
            );
            order_reconcile_teardown(&sdk, symbol, &owned, false).await;
            return;
        }
    };
    if ordno.is_empty() || ordno == "0" {
        println!("{tag} verdict=inconclusive [seed returned no usable order number] — reconciling");
        order_reconcile_teardown(&sdk, symbol, &owned, false).await;
        return;
    }
    owned.insert(ordno.clone());

    // (2) CONTROL leg — a VALID modify (OrdprcPtnCode present) to P1 → expect an ack
    // (00462). Proves this session can modify at all, so an omitted-pattern IGW00000 is
    // the field's surface, not a session where every modify fails. A non-ack here is
    // itself inconclusive.
    let control_body = order_seed_00701(&ordno, p1);
    match fire_inblock(&client, &url, &token, "CSPAT00701", "CSPAT00701InBlock1", &control_body).await
    {
        Some((http, rsp_cd, child_no)) if is_order_placement_success(http, &rsp_cd) => {
            println!("{tag} control-modify=[ok http={http} rsp_cd={rsp_cd}]");
            // A modify is ABSOLUTE and creates a NEW order number (CSPAT00701
            // OutBlock2.OrdNo; modify-cancel plan KTD4): the seed submit's number is
            // now stale. Re-key the tracked order onto the modify child so S_pre/
            // S_post snapshot the LIVE resting order, the violation fires against it,
            // and the seed-cancel teardown targets it. Without this the harness holds
            // the vanished submit number: every snapshot reads the seed absent
            // (→ classify_igw00000_ab short-circuits to inconclusive) and the
            // seed-cancel hits 01433 (cancel of an already-replaced order number).
            // The child number comes from OUR modify response, so it is never a
            // foreign order.
            match child_no.filter(|n| !n.trim().is_empty() && n.trim() != "0") {
                Some(child) => {
                    // NO-STRAND INVARIANT: `owned` here is only a teardown HINT. Every
                    // teardown in this fn passes owned_fully_constructed=false (cancel-ALL),
                    // so a resting order is never stranded even though after this swap
                    // `owned` holds the child (not any took-effect fire grandchild) and the
                    // None branch below reconciles while `owned` still holds the STALE
                    // submit number. If any teardown here is ever switched to owned-only
                    // (true), this swap must instead track every resting order (child +
                    // grandchild) or a live order would be surfaced-but-not-cancelled.
                    owned.remove(&ordno);
                    ordno = child.trim().to_string();
                    owned.insert(ordno.clone());
                }
                None => {
                    println!(
                        "{tag} verdict=inconclusive [control modify acked but surfaced no child \
                         order number — cannot re-key onto the live order] — reconciling"
                    );
                    order_reconcile_teardown(&sdk, symbol, &owned, false).await;
                    return;
                }
            }
        }
        Some((http, rsp_cd, _)) => {
            println!(
                "{tag} verdict=inconclusive [control modify not acked: http={http} rsp_cd={rsp_cd} \
                 — session cannot modify] — reconciling"
            );
            order_reconcile_teardown(&sdk, symbol, &owned, false).await;
            return;
        }
        None => {
            println!(
                "{tag} verdict=inconclusive [control modify transport-failure — may-rest] — reconciling"
            );
            order_reconcile_teardown(&sdk, symbol, &owned, false).await;
            return;
        }
    }

    // (3) S_pre — snapshot the control after the valid modify (price should be P1).
    let s_pre = match scan_symbol_working_orders(&sdk, symbol).await {
        Ok(rows) => seed_snapshot_from(&rows, &ordno),
        Err(e) => {
            println!("{tag} verdict=inconclusive [S_pre scan failed: {e}] — reconciling");
            order_reconcile_teardown(&sdk, symbol, &owned, false).await;
            return;
        }
    };

    // (4) FIRE variant B — the SAME modify with OrdprcPtnCode OMITTED, at P2 (!= P1) so
    // a took-effect mutation shows as a price change (or a fill).
    let mut violation = order_seed_00701(&ordno, p2);
    if let Some(m) = violation.as_object_mut() {
        m.insert("OrdprcPtnCode".to_string(), serde_json::json!(""));
    }
    let (fire_http, fire_rsp_cd) =
        match fire_inblock(&client, &url, &token, "CSPAT00701", "CSPAT00701InBlock1", &violation).await
        {
            Some((http, rsp_cd, _)) => (http, rsp_cd),
            None => {
                // Transport failure on the fire is MAY-REST — the variant may have rested.
                println!(
                    "{tag} verdict=may-rest [fire transport-failure — cannot confirm placed-nothing] \
                     — reconciling"
                );
                order_reconcile_teardown(&sdk, symbol, &owned, false).await;
                return;
            }
        };
    println!("{tag} fire=[http={fire_http} rsp_cd={fire_rsp_cd}] (OrdprcPtnCode omitted)");

    // (5) S_post — paced re-snapshot + new-order check. The 1000ms pace keeps S_post
    // from self-throttling into a false untrusted read (§27 / #137).
    tokio::time::sleep(Duration::from_millis(1000)).await;
    let (s_post, new_order, reads_trusted) = match scan_symbol_working_orders(&sdk, symbol).await {
        Ok(rows) => (
            seed_snapshot_from(&rows, &ordno),
            has_new_resting_order(&rows, &ordno),
            true,
        ),
        Err(e) => {
            println!("{tag} S_post scan failed [{e}] — untrusted read");
            (SeedSnapshot::default(), false, false)
        }
    };

    // (6) TEARDOWN — cancel the seed + the bounded fill-check (the fill-inclusive
    // defense: a fully-filled seed vanished from S_post surfaces here as Filled).
    let cancel = CSPAT00801Request::new(&ordno, symbol, "1");
    let cancel_gateway_rejected = match sdk.orders().cancel(&cancel).await {
        Ok(_) => false,
        Err(e) => {
            let rejected = matches!(e, LsError::ApiError { .. });
            println!(
                "{tag} seed-cancel error [{}] (gateway_rejected={rejected}) — bounded fill-check decides",
                safe_err(&e)
            );
            rejected
        }
    };
    let disposition = match scan_symbol_working_orders(&sdk, symbol).await {
        Ok(rows) => classify_control_disposition(cancel_gateway_rejected, &rows),
        // An untrusted post-cancel scan is NOT CleanlyCanceled → the verdict cannot
        // reach placed-nothing (fails safe to inconclusive/reconcile).
        Err(_) => ControlDisposition::StillResting(vec![]),
    };

    let verdict = classify_igw00000_ab(
        fire_http,
        &fire_rsp_cd,
        reads_trusted,
        &s_pre,
        &s_post,
        new_order,
        &disposition,
    );
    let label = match verdict {
        AbVerdict::PlacedNothing => "placed-nothing",
        AbVerdict::MayRest => "may-rest",
        AbVerdict::Inconclusive => "inconclusive",
    };

    // Final reconcile — never leave a resting order. Cancel-all (untrusted owned set)
    // because this wave deliberately fired a possibly-resting variant.
    order_reconcile_teardown(&sdk, symbol, &owned, false).await;
    println!(
        "{tag} verdict={label} [fire http={fire_http} rsp_cd={fire_rsp_cd}] \
         (credential-free; U6 if placed-nothing, U7 if may-rest)"
    );
}

#[tokio::test]
#[ignore = "live probe: attended IGW00000 A/B; needs real LS paper ORDER-account + open KRX window + LS_ORDER_SMOKE=1 + a fresh LS_ORDER_SMOKE_NONCE (attended TTY); run via `make live-smoke-cspat00701-igw00000-ab`"]
async fn live_smoke_cspat00701_igw00000_ab() {
    run_igw00000_ab_probe().await;
}

// ===========================================================================
// Governed booking-determining A/B characterization (CSPAT00601, Route C §30).
// The ONLY sanctioned path to fire a booking-determining omission: a one-shot
// attended seed → S_pre → fire (the annotated field's required-omit submit) →
// paced S_post + t0424 position fill-check → sign-aware close/cancel →
// fail-closed teardown cycle. A `rejected` verdict RE-OPENS/LIFTS the
// annotation (plan R8; a provisional R11 annotation is lifted by nothing
// else). Probe-only: the runtime seam and the fire-loop skip are untouched.
// ===========================================================================

/// The env var naming the annotated CSPAT00601 field whose required-omission the
/// attended booking A/B fires. Empty/unset defaults to the §30-proven `BnsTpCode`.
const BOOKING_AB_FIELD_ENV: &str = "LS_AB_FIELD";
const BOOKING_AB_DEFAULT_FIELD: &str = "BnsTpCode";

/// The governed-field gate (pure, no dispatch): the harness fires ONLY a field the
/// embedded CSPAT00601 schema annotates `booking_determining: [required]` — the
/// SAME `is_booking_determining` lookup the fire-loop skip keys on, so "what the
/// differential refuses to fire" and "what the governed harness may fire" cannot
/// drift. An unknown field and an unannotated (reject-expected) field are both
/// refused with a clean message BEFORE any credential load or dispatch.
fn booking_ab_field_gate(schema: &ConstraintSchema, field: &str) -> Result<(), String> {
    if !schema.fields.iter().any(|f| f.name == field) {
        return Err(format!(
            "'{field}' is not a field of the embedded CSPAT00601 constraint schema — refusing \
             (no dispatch)"
        ));
    }
    if !is_booking_determining(schema, field, "required") {
        return Err(format!(
            "'{field}' is not annotated booking_determining[required] — the governed A/B fires \
             ONLY annotated omissions (its differential variant already fires normally); refusing \
             (no dispatch)"
        ));
    }
    Ok(())
}

/// The governed booking A/B verdict (plan U3). `Rejected` is the annotation
/// re-open/lift trigger (R8/R11); either `PlacesDefaultedOrder*` arm CONFIRMS the
/// annotation; `Inconclusive` changes nothing and fails closed to a cancel-all
/// teardown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BookingAbVerdict {
    /// The omitted-field submit was accepted and a NEW resting row appeared —
    /// the gateway defaulted the field and booked a real (resting) order.
    PlacesDefaultedOrderRested,
    /// A FILL was detected (position delta, a partial-fill row, or an acceptance
    /// ack whose child vanished from the book) — the defaulted order EXECUTED.
    PlacesDefaultedOrderFilled,
    /// A recognized merits reject with NOTHING observed — the gateway refused the
    /// omission and placed nothing. RE-OPENS/LIFTS the annotation (R8/R11).
    Rejected,
    /// Throttle / transport failure / untrusted reads / ambiguous ack with
    /// nothing observable — fail-closed cancel-all teardown, re-run.
    Inconclusive,
}

/// Classify the governed booking A/B (pure, offline-twinnable). Precedence, in
/// the fail-safe order: a transport failure or an untrusted post-fire read is
/// `Inconclusive` (#137: an untrusted read is NEVER evidence — `rejected`
/// asserts placed-nothing, so it needs trusted reads too); an observed FILL
/// outranks an observed resting row; any observation outranks the fired
/// `rsp_cd` (the book is the truth, not the code); with nothing observed, a
/// throttle/noneval code is not a merits answer, an acceptance ack is ambiguous
/// (never `rejected`), a non-ingress 5xx stays may-rest-shaped — only a positive
/// placed-nothing merits reject (via the shared [`classify_fired_variant`])
/// yields `Rejected`.
/// A booking-AB `Rejected` verdict LIFTS a booking-determining annotation (R8/R11),
/// so it must rest on a code proven to be an ON-MERITS placed-nothing reject of the
/// omitted submit field — never a bare `classify_fired_variant` "placed nothing"
/// inference. `classify_fired_variant`'s `else` arm returns `PlacedNothing` for ANY
/// non-success, non-5xx-may-rest answer, so a throttle degraded to an empty `rsp_cd`
/// (HTTP 429 / non-JSON body), or a business reject for a reason UNRELATED to the
/// injected omission (inventory, invalid-combination), would otherwise lift a field
/// that places real orders. The allowlist is the shared ingress-validation rejects
/// (`ls_core::is_ingress_validation_reject`, e.g. `IGW40011`) plus the catalogued
/// business reject observed for an omitted order field (`01407`, §30 IsuNo removal).
/// An empty/degraded or un-catalogued `rsp_cd` is NOT a merits answer → `Inconclusive`.
fn is_booking_ab_merits_reject(rsp_cd: &str) -> bool {
    ls_core::is_ingress_validation_reject(rsp_cd) || rsp_cd == "01407"
}

fn classify_booking_ab(
    fire: Option<(u16, &str)>,
    reads_trusted: bool,
    new_resting_order: bool,
    fill_detected: bool,
) -> BookingAbVerdict {
    let Some((http, rsp_cd)) = fire else {
        return BookingAbVerdict::Inconclusive;
    };
    if !reads_trusted {
        return BookingAbVerdict::Inconclusive;
    }
    if fill_detected {
        return BookingAbVerdict::PlacesDefaultedOrderFilled;
    }
    if new_resting_order {
        return BookingAbVerdict::PlacesDefaultedOrderRested;
    }
    if is_noneval_code(rsp_cd) {
        return BookingAbVerdict::Inconclusive;
    }
    match classify_fired_variant(http, rsp_cd) {
        // Placed nothing, AND on a proven omission-reject code: the gateway refused
        // the omission on its merits → safe to re-open/lift the annotation.
        FiredVariantOutcome::PlacedNothing if is_booking_ab_merits_reject(rsp_cd) => {
            BookingAbVerdict::Rejected
        }
        // Placed nothing but on an empty/degraded or un-catalogued code (429, non-JSON
        // body, unrelated business reject): NOT a merits answer — never lift.
        FiredVariantOutcome::PlacedNothing => BookingAbVerdict::Inconclusive,
        // An ack with nothing observable, or a non-ingress 5xx: ambiguous.
        FiredVariantOutcome::Accepted | FiredVariantOutcome::MayHaveRested => {
            BookingAbVerdict::Inconclusive
        }
    }
}

/// The sign-aware close-out side for a detected fill (close-only semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloseSide {
    /// The defaulted order BOUGHT `qty` — flatten with a marketable SELL.
    Sell(u64),
    /// The defaulted order SOLD `qty` — buy exactly `qty` back (returns to the
    /// pre state, never beyond flat).
    Buy(u64),
}

/// Plan the sign-aware close-out from the before/after `t0424` position (pure).
/// A positive `janqty` delta means the defaulted order BOUGHT → SELL the delta,
/// capped at the currently-sellable qty (`mdposqt`; an unsettled buy with zero
/// sellable yields NO close now — surfaced to the operator, never an oversell). A
/// negative delta means it SOLD → BUY exactly the delta back. No delta → no
/// close order (an absence-from-book fill with no measurable position change is
/// the operator's to reconcile).
fn plan_close_out(janqty_pre: i64, janqty_post: i64, sellable_post: u64) -> Option<CloseSide> {
    let delta = janqty_post - janqty_pre;
    if delta > 0 {
        let qty = (delta as u64).min(sellable_post);
        (qty > 0).then_some(CloseSide::Sell(qty))
    } else if delta < 0 {
        Some(CloseSide::Buy(delta.unsigned_abs()))
    } else {
        None
    }
}

/// Read the `t0424` position for `symbol`: `(janqty, mdposqt)` — `(0, 0)` when the
/// symbol carries no holdings row. Paced 1500ms (Account bucket) like the `t0425`
/// scan. A failed read is `Err` — the caller must treat it as UNTRUSTED (never
/// no-position). The expcode match tolerates an `A`-prefixed issue number.
async fn read_symbol_position(sdk: &LsSdk, symbol: &str) -> Result<(i64, u64), String> {
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    match sdk
        .account()
        .stock_balance(&T0424Request::new("1", "0", "0", "0"))
        .await
    {
        Ok(resp) => {
            let row = resp.outblock1.iter().find(|r| {
                let e = r.expcode.trim();
                e == symbol || e.trim_start_matches('A') == symbol
            });
            Ok((
                row.map(|r| parse_qty(&r.janqty) as i64).unwrap_or(0),
                row.map(|r| parse_qty(&r.mdposqt)).unwrap_or(0),
            ))
        }
        Err(e) => Err(format!("t0424 holdings read failed ({})", safe_err(&e))),
    }
}

/// The attended governed booking-determining A/B (plan U3). Re-characterizes the
/// omission behavior of ONE annotated CSPAT00601 field (`LS_AB_FIELD`, default
/// `BnsTpCode`) under the full negative-probe safety spine: guard chain →
/// pure field gate (refuse an unknown/unannotated field, no dispatch) → seed a
/// resting control → S_pre → fire the field-omitted submit → paced S_post +
/// `t0424` position fill-check → verdict → rested: cancel the child / filled:
/// sign-aware close-out to flat / rejected: print the R8/R11 re-open notice →
/// fail-closed cancel-all teardown in EVERY branch. Prints ONE credential-free
/// `BOOKING-AB field=… verdict=…` line; never leaves a resting order.
async fn run_booking_determining_ab_probe() {
    if let Err(e) = install_dispatch_log_suppressor() {
        panic!("{}", scrub_secrets(&e.to_string()));
    }
    if let Err(e) = autonomy_guard() {
        panic!("{}", scrub_secrets(&e.to_string()));
    }
    if let Err(e) = order_smoke_guard() {
        panic!("{}", scrub_secrets(&e.to_string()));
    }
    // The governed-field gate is PURE and runs before any credential load or
    // dispatch: an unannotated/unknown LS_AB_FIELD refuses cleanly.
    let field = std::env::var(BOOKING_AB_FIELD_ENV)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| BOOKING_AB_DEFAULT_FIELD.to_string());
    let tag = format!("BOOKING-AB target=CSPAT00601 field={field}");
    let schema = ls_core::schema_for("CSPAT00601")
        .expect("CSPAT00601 carries an embedded constraint schema");
    if let Err(e) = booking_ab_field_gate(schema, &field) {
        println!("{tag} verdict=refused [{e}]");
        return;
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

    // Daily band + two DISTINCT non-marketable prices near the floor: P0 (the
    // seed control) < P1 (the fired violation). P1 != P0 keeps the two orders
    // visually distinct in the scans; both are far below market, so a
    // direction-defaulted BUY rests while a defaulted SELL executes — which is
    // exactly what the fill-check exists to catch.
    let band = match sdk
        .market_session()
        .quote(&T1102Request::new(symbol, "K"))
        .await
    {
        Ok(resp) => match validate_band(&resp.outblock.uplmtprice, &resp.outblock.dnlmtprice) {
            Ok(b) => b,
            Err(e) => {
                println!("{tag} verdict=inconclusive [band: {e}] — no placement");
                return;
            }
        },
        Err(e) => {
            println!(
                "{tag} verdict=inconclusive [band fetch failed: {}] — no placement",
                safe_err(&e)
            );
            return;
        }
    };
    let p0 = band.resting_buy_price();
    let p1 = p0.saturating_add(tick(p0)).min(band.uplmt);
    if p1 <= p0 {
        println!("{tag} verdict=inconclusive [band too narrow for 2 distinct A/B prices]");
        return;
    }

    let token = match sdk.standalone().token().await {
        Ok(t) if !t.is_empty() => t,
        _ => {
            println!("{tag} verdict=inconclusive [token acquisition failed]");
            return;
        }
    };
    let base = ls_core::config::Environment::resolve_base_url(&config);
    let url = format!("{base}/stock/order");
    let client = probe_client();

    // `owned` is a teardown HINT only: every teardown in this fn passes
    // owned_fully_constructed=false (cancel-ALL) — this probe deliberately fires
    // a variant that can rest an order whose OrdNo may never surface, so the
    // owned set can never be trusted to enumerate the residue.
    let mut owned: BTreeSet<String> = BTreeSet::new();

    // PRE-ASSERT-FLAT: refuse unless the symbol is flat & fill-free — a foreign
    // or stranded row would poison both the new-order detection and the teardown
    // soundness (do NOT teardown here).
    match scan_symbol_working_orders(&sdk, symbol).await {
        Ok(rows) => match require_flat_and_fill_free(&rows) {
            Ok(()) => {}
            Err(NotClear::Resting(o)) => {
                println!(
                    "{tag} verdict=inconclusive [pre-assert-flat: resting ordnos=[{}]; clear the \
                     book] — no placement",
                    o.join(",")
                );
                return;
            }
            Err(NotClear::Fill(f)) => {
                println!(
                    "{tag} verdict=inconclusive [pre-assert-flat: fill ordnos=[{}]; paper-reset] — \
                     no placement",
                    f.join(",")
                );
                return;
            }
        },
        Err(e) => {
            println!("{tag} verdict=inconclusive [pre-assert-flat scan failed: {e}] — no placement");
            return;
        }
    }

    // POSITION BASELINE (t0424): the before leg of the fill-check. A defaulted
    // fire can EXECUTE (a direction-defaulted SELL at a below-market limit is
    // marketable), so the fill signal is a position delta — never assume it rests.
    let (janqty_pre, _) = match read_symbol_position(&sdk, symbol).await {
        Ok(p) => p,
        Err(e) => {
            println!("{tag} verdict=inconclusive [position baseline: {e}] — no placement");
            return;
        }
    };

    // (1) SEED — place the resting control at P0 (band floor, non-marketable buy)
    // and claim it into the owned set.
    let control_req = CSPAT00601Request::limit(symbol, "1", p0.to_string(), "2", member);
    let ordno = match sdk.orders().submit(&control_req).await {
        Ok(resp) => resp.order_no().trim().to_string(),
        Err(e) => {
            println!(
                "{tag} verdict=inconclusive [seed submit failed/ambiguous: {}] — reconciling",
                safe_err(&e)
            );
            order_reconcile_teardown(&sdk, symbol, &owned, false).await;
            return;
        }
    };
    if ordno.is_empty() || ordno == "0" {
        println!("{tag} verdict=inconclusive [seed returned no usable order number] — reconciling");
        order_reconcile_teardown(&sdk, symbol, &owned, false).await;
        return;
    }
    owned.insert(ordno.clone());

    // (2) S_pre — trusted snapshot of the resting seed (existing scan machinery).
    let s_pre = match scan_symbol_working_orders(&sdk, symbol).await {
        Ok(rows) => seed_snapshot_from(&rows, &ordno),
        Err(e) => {
            println!("{tag} verdict=inconclusive [S_pre scan failed: {e}] — reconciling");
            order_reconcile_teardown(&sdk, symbol, &owned, false).await;
            return;
        }
    };
    if !s_pre.present {
        println!("{tag} verdict=inconclusive [seed absent from S_pre — cannot anchor] — reconciling");
        order_reconcile_teardown(&sdk, symbol, &owned, false).await;
        return;
    }

    // (3) FIRE — the valid 1-lot submit with EXACTLY `field` blanked (the same
    // empty-string encoding `generate_invalid_variants` uses for required-omit),
    // at P1. Capture http/rsp_cd/ord_no; claim any surfaced child.
    let mut violation = order_seed_00601(p1);
    if let Some(m) = violation.as_object_mut() {
        m.insert(field.clone(), serde_json::json!(""));
    }
    let (fire_http, fire_rsp_cd, child) = match fire_inblock(
        &client, &url, &token, "CSPAT00601", "CSPAT00601InBlock1", &violation,
    )
    .await
    {
        Some((http, rsp_cd, child)) => (
            http,
            rsp_cd,
            child.filter(|c| !c.trim().is_empty() && c.trim() != "0"),
        ),
        None => {
            // Transport failure: the variant may have rested with an OrdNo we
            // never got — fail-closed cancel-all teardown, verdict inconclusive.
            println!("{tag} fire=[transport-failure] verdict=inconclusive — reconciling");
            order_reconcile_teardown(&sdk, symbol, &owned, false).await;
            return;
        }
    };
    println!("{tag} fire=[http={fire_http} rsp_cd={fire_rsp_cd}] ({field} omitted)");
    if let Some(c) = &child {
        owned.insert(c.trim().to_string());
    }

    // (4) Paced S_post — re-scan + new-resting detection (§27/#137 pace), plus
    // the t0424 position re-read. Any failed read renders the reads UNTRUSTED.
    tokio::time::sleep(Duration::from_millis(1000)).await;
    let (rows_post, scan_trusted) = match scan_symbol_working_orders(&sdk, symbol).await {
        Ok(rows) => (rows, true),
        Err(e) => {
            println!("{tag} S_post scan failed [{e}] — untrusted read");
            (Vec::new(), false)
        }
    };
    let (janqty_post, sellable_post, position_trusted) =
        match read_symbol_position(&sdk, symbol).await {
            Ok((j, s)) => (j, s, true),
            Err(e) => {
                println!("{tag} S_post position read failed [{e}] — untrusted read");
                (janqty_pre, 0, false)
            }
        };
    let reads_trusted = scan_trusted && position_trusted;

    // FILL DETECTION (never assume the defaulted order rests): a position delta,
    // a partial-fill row in the scan (`cheqty>0` — the pre-assert-flat baseline
    // was fill-free), or an acceptance ack whose surfaced child is ABSENT from a
    // trusted book (a fully-filled row vanishes from `chegb="2"` by construction).
    let new_resting = has_new_resting_order(&rows_post, &ordno);
    let child_resting = child.as_ref().is_some_and(|c| {
        let want = normalize_ordno(c);
        rows_post.iter().any(|r| {
            normalize_ordno(&r.ordno) == want && parse_qty(&r.cheqty) == 0 && parse_qty(&r.ordrem) > 0
        })
    });
    let partial_fill = rows_post.iter().any(|r| parse_qty(&r.cheqty) > 0);
    let accepted = is_order_placement_success(fire_http, &fire_rsp_cd);
    let fill_detected = reads_trusted
        && (janqty_post != janqty_pre
            || partial_fill
            || (accepted && child.is_some() && !child_resting));

    let verdict = classify_booking_ab(
        Some((fire_http, &fire_rsp_cd)),
        reads_trusted,
        new_resting,
        fill_detected,
    );

    // Per-verdict handling. The fail-closed cancel-all teardown + flat report
    // runs in EVERY branch after this match.
    let label = match verdict {
        BookingAbVerdict::PlacesDefaultedOrderRested => {
            // Cancel the defaulted child directly (teardown would also sweep it,
            // but the plan's contract is an explicit child cancel). Cancel every
            // resting non-seed row when the child OrdNo never surfaced.
            for r in rows_post.iter().filter(|r| {
                normalize_ordno(&r.ordno) != normalize_ordno(&ordno)
                    && parse_qty(&r.cheqty) == 0
                    && parse_qty(&r.ordrem) > 0
            }) {
                let cancel =
                    CSPAT00801Request::new(r.ordno.trim(), r.expcode.trim(), r.ordrem.trim());
                match sdk.orders().cancel(&cancel).await {
                    Ok(_) => println!(
                        "{tag} defaulted-child cancel ordno={} result=canceled",
                        r.ordno.trim()
                    ),
                    Err(e) => println!(
                        "{tag} defaulted-child cancel ordno={} result=[{}]",
                        r.ordno.trim(),
                        safe_err(&e)
                    ),
                }
            }
            "places-defaulted-order(rested)"
        }
        BookingAbVerdict::PlacesDefaultedOrderFilled => {
            // Sign-aware close-out (close-only semantics): sell the bought delta
            // (capped at sellable) or buy back the sold delta; never oversell,
            // never move beyond the pre-probe position.
            match plan_close_out(janqty_pre, janqty_post, sellable_post) {
                Some(CloseSide::Sell(qty)) => {
                    let close = CSPAT00601Request::limit(
                        symbol,
                        qty.to_string(),
                        band.marketable_sell_price().to_string(),
                        "1",
                        member,
                    );
                    match sdk.orders().submit(&close).await {
                        Ok(resp) => {
                            owned.insert(resp.order_no().trim().to_string());
                            println!("{tag} close-out=[sell qty={qty} marketable] result=acked");
                        }
                        Err(e) => println!(
                            "{tag} close-out=[sell qty={qty}] result=[{}] — operator must flatten",
                            safe_err(&e)
                        ),
                    }
                }
                Some(CloseSide::Buy(qty)) => {
                    let close = CSPAT00601Request::limit(
                        symbol,
                        qty.to_string(),
                        band.marketable_buy_price().to_string(),
                        "2",
                        member,
                    );
                    match sdk.orders().submit(&close).await {
                        Ok(resp) => {
                            owned.insert(resp.order_no().trim().to_string());
                            println!("{tag} close-out=[buy-back qty={qty} marketable] result=acked");
                        }
                        Err(e) => println!(
                            "{tag} close-out=[buy-back qty={qty}] result=[{}] — operator must \
                             flatten",
                            safe_err(&e)
                        ),
                    }
                }
                None => println!(
                    "{tag} close-out=none [fill detected but no closable janqty delta \
                     (unsettled/absence-only signal)] — operator must reconcile the position"
                ),
            }
            // Verify flat: the position must be back at the baseline.
            match read_symbol_position(&sdk, symbol).await {
                Ok((j, _)) if j == janqty_pre => {
                    println!("{tag} close-out flat=confirmed (janqty back at baseline)")
                }
                Ok((j, _)) => println!(
                    "{tag} close-out flat=NOT-confirmed (janqty delta {} remains) — operator must \
                     reconcile",
                    j - janqty_pre
                ),
                Err(e) => println!(
                    "{tag} close-out flat=UNVERIFIED [{e}] — operator must confirm the position"
                ),
            }
            "places-defaulted-order(filled)"
        }
        BookingAbVerdict::Rejected => {
            println!(
                "{tag} NOTE: verdict=rejected RE-OPENS/LIFTS the booking_determining annotation \
                 for {field} (plan R8/R11): remove `required` from the field's \
                 booking_determining list in metadata/constraints/CSPAT00601.yaml, flip its \
                 error-coverage status from booking_determining to confirmed, then re-run \
                 `make live-smoke-cspat00601-negative` to observe the omission on the normal \
                 differential path"
            );
            "rejected"
        }
        BookingAbVerdict::Inconclusive => "inconclusive",
    };

    // Fail-closed teardown in EVERY branch: cancel-all (owned untrusted by
    // design), loud alarms preserved, then the ONE credential-free verdict line.
    order_reconcile_teardown(&sdk, symbol, &owned, false).await;
    println!(
        "{tag} verdict={label} [fire http={fire_http} rsp_cd={fire_rsp_cd}] \
         (credential-free; rejected → re-open/lift the annotation per R8/R11)"
    );
}

#[tokio::test]
#[ignore = "live probe: attended governed booking-determining A/B; needs real LS paper ORDER-account + open KRX window + LS_ORDER_SMOKE=1 + a fresh LS_ORDER_SMOKE_NONCE (attended TTY); optional LS_AB_FIELD (default BnsTpCode); run via `make live-smoke-cspat00601-booking-ab`"]
async fn live_smoke_cspat00601_booking_determining_ab() {
    run_booking_determining_ab_probe().await;
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

/// A synthetic order-constraint fixture declaring the **intended CSPAT00701
/// annotation shape** (Route B, plan 2026-07-14-001): `OrdprcPtnCode`'s
/// `required`-class violation answering `IGW00000` placed nothing. Built through the
/// real `ConstraintSchema` deserialize path (same serde derive as the embedded
/// YAML), so these offline tests prove the live `InvalidVariant.field` / `v.class`
/// string binding matches the annotation key BEFORE the U6 metadata edit lands
/// (KTD3) — it must NOT depend on the not-yet-authored embedded CSPAT00701 schema.
fn placed_nothing_fixture_schema() -> ConstraintSchema {
    serde_json::from_value(serde_json::json!({
        "tr_code": "FIXTURE",
        "fields": [
            {
                "name": "OrdprcPtnCode",
                "type": "string",
                "required": true,
                "placed_nothing_codes": { "required": ["IGW00000"] },
                "enum": { "applicable": false },
                "range": { "applicable": false },
                "format": { "applicable": false }
            },
            {
                "name": "OrgOrdNo",
                "type": "integer",
                "required": true,
                "enum": { "applicable": false },
                "range": { "applicable": false },
                "format": { "applicable": false }
            }
        ]
    }))
    .expect("fixture schema deserializes")
}

#[test]
fn order_code_placed_nothing_is_scoped_to_the_exact_field_class_code_triple() {
    // Route B (plan 2026-07-14-001): the scoped placed-nothing predicate fires ONLY
    // for the exact declared `(field, class, code)` triple — a different code, class,
    // or field all miss, and an empty/absent declaration is never placed-nothing.
    let schema = placed_nothing_fixture_schema();
    assert!(order_code_placed_nothing(&schema, "OrdprcPtnCode", "required", "IGW00000"));
    // Same field+class, a DIFFERENT code → false (scoped to the declared code).
    assert!(!order_code_placed_nothing(&schema, "OrdprcPtnCode", "required", "IGW50008"));
    // Same field, a DIFFERENT class → false (scoped to the declared class).
    assert!(!order_code_placed_nothing(&schema, "OrdprcPtnCode", "type", "IGW00000"));
    // A DIFFERENT declared field with no annotation → false (scoped to the field).
    assert!(!order_code_placed_nothing(&schema, "OrgOrdNo", "required", "IGW00000"));
    // An unknown field → false.
    assert!(!order_code_placed_nothing(&schema, "Nonexistent", "required", "IGW00000"));
}

#[test]
fn resolve_fired_outcome_downgrades_only_a_declared_mayrest_triple() {
    // Resolver twin: the pure Route-B routing is offline-twinnable. Only a declared
    // (field, class, code) MayHaveRested downgrades to PlacedNothing; Accepted,
    // PlacedNothing, and an undeclared MayHaveRested all pass through unchanged.
    let schema = placed_nothing_fixture_schema();
    // The declared (field, class) with IGW00000-at-500 → downgraded to PlacedNothing.
    assert_eq!(
        resolve_fired_outcome(&schema, "OrdprcPtnCode", "required", 500, "IGW00000"),
        FiredVariantOutcome::PlacedNothing
    );
    // The SAME code on an UNDECLARED (field, class) → stays MayHaveRested (fail-closed).
    assert_eq!(
        resolve_fired_outcome(&schema, "OrgOrdNo", "required", 500, "IGW00000"),
        FiredVariantOutcome::MayHaveRested
    );
    // A 2xx order-acceptance ack is never downgraded → Accepted (WAVE-BLOCKED, unchanged).
    assert_eq!(
        resolve_fired_outcome(&schema, "OrdprcPtnCode", "required", 200, "00040"),
        FiredVariantOutcome::Accepted
    );
    // The pre-existing IGW40011-at-500 ingress exemption is unchanged by the resolver.
    assert_eq!(
        resolve_fired_outcome(&schema, "OrdprcPtnCode", "required", 500, "IGW40011"),
        FiredVariantOutcome::PlacedNothing
    );
    // A generic non-ack business reject stays PlacedNothing, unchanged.
    assert_eq!(
        resolve_fired_outcome(&schema, "OrgOrdNo", "required", 400, "40510"),
        FiredVariantOutcome::PlacedNothing
    );
}

#[test]
fn placed_nothing_binding_matches_the_live_invalidvariant_field_and_class_strings() {
    // Real-binding round-trip (de-risks the attended window, KTD3): the live fire
    // loop keys `order_code_placed_nothing` on `v.field` / `v.class` produced by
    // `generate_invalid_variants`. Generate the OrdprcPtnCode required-omit variant
    // from the fixture and assert its generated (field, class) strings resolve the
    // annotation — proving the offline binding before the U6 metadata edit.
    let schema = placed_nothing_fixture_schema();
    let seed = serde_json::json!({ "OrdprcPtnCode": "00", "OrgOrdNo": "12345" });
    let variant = generate_invalid_variants(&schema, &seed)
        .into_iter()
        .find(|v| v.field == "OrdprcPtnCode" && v.class == "required")
        .expect("OrdprcPtnCode required-omit variant is generated");
    assert!(
        order_code_placed_nothing(&schema, &variant.field, &variant.class, "IGW00000"),
        "the generated variant's field/class strings must resolve the annotation key"
    );
}

/// A synthetic order-constraint fixture declaring the **intended CSPAT00601
/// annotation shape** (Route C, §30): `BnsTpCode` marked
/// `booking_determining: [required]` next to an UNMARKED integer sibling
/// (`OrdQty`, which generates both type and required variants). Built through the
/// real `ConstraintSchema` deserialize path (same serde derive as the embedded
/// YAML), so these offline tests prove the live `InvalidVariant.field` /
/// `v.class` string binding matches the annotation key BEFORE the metadata edit
/// lands — it must NOT depend on the not-yet-annotated embedded CSPAT00601
/// schema.
fn booking_determining_fixture_schema() -> ConstraintSchema {
    serde_json::from_value(serde_json::json!({
        "tr_code": "FIXTURE",
        "fields": [
            {
                "name": "BnsTpCode",
                "type": "string",
                "required": true,
                "booking_determining": ["required"],
                "enum": { "applicable": false },
                "range": { "applicable": false },
                "format": { "applicable": false }
            },
            {
                "name": "OrdQty",
                "type": "integer",
                "required": true,
                "enum": { "applicable": false },
                "range": { "applicable": false },
                "format": { "applicable": false }
            }
        ]
    }))
    .expect("fixture schema deserializes")
}

#[test]
fn is_booking_determining_is_scoped_to_the_exact_marked_field_class_pair() {
    // Route C (§30): the never-fire predicate fires ONLY for the exact marked
    // (field, class) pair — a different class or field misses, and an
    // empty/absent declaration is never booking-determining.
    let schema = booking_determining_fixture_schema();
    assert!(is_booking_determining(&schema, "BnsTpCode", "required"));
    // Same field, a DIFFERENT class → false (the marked field's type variant
    // still fires).
    assert!(!is_booking_determining(&schema, "BnsTpCode", "type"));
    // An UNMARKED sibling field → false (its required variant still fires).
    assert!(!is_booking_determining(&schema, "OrdQty", "required"));
    // An unknown field → false.
    assert!(!is_booking_determining(&schema, "Nonexistent", "required"));
    // The real embedded CSPAT00601 schema now carries the U2 audit's annotations
    // (constraints/CSPAT00601.yaml) — the proven BnsTpCode pair resolves, and an
    // unannotated reject-expected sibling (IsuNo) still misses.
    let embedded = ls_core::schema_for("CSPAT00601").expect("CSPAT00601 schema");
    assert!(is_booking_determining(embedded, "BnsTpCode", "required"));
    assert!(!is_booking_determining(embedded, "IsuNo", "required"));
}

#[test]
fn order_variant_fire_decision_skips_only_annotated_generated_variants() {
    // Real-binding round-trip (Route C, §30): the live fire loop keys the skip on
    // `v.field` / `v.class` produced by `generate_invalid_variants`. Generate the
    // variants from the fixture and assert the SAME pure decision fn the loop
    // calls: the marked BnsTpCode required-omit variant is SKIP (never
    // dispatched); the unmarked sibling's required variant and a type-class
    // variant on the marked field both FIRE (negative anchors).
    let schema = booking_determining_fixture_schema();
    let seed = serde_json::json!({ "BnsTpCode": "2", "OrdQty": 1 });
    let variants = generate_invalid_variants(&schema, &seed);
    let find = |field: &str, class: &str| {
        variants
            .iter()
            .find(|v| v.field == field && v.class == class)
            .unwrap_or_else(|| panic!("{field}/{class} variant is generated"))
    };
    // The annotated (field, class) pair → skip: never fired by design.
    assert!(
        !order_variant_may_fire(&schema, find("BnsTpCode", "required")),
        "the marked BnsTpCode required variant must never be dispatched"
    );
    // The UNMARKED sibling's required variant → fires.
    assert!(
        order_variant_may_fire(&schema, find("OrdQty", "required")),
        "an unmarked sibling's required variant still fires"
    );
    // The unmarked sibling's type variant → fires.
    assert!(
        order_variant_may_fire(&schema, find("OrdQty", "type")),
        "an unmarked type variant still fires"
    );
    // The marked field's OTHER class → fires (a string field generates no type
    // variant, so anchor on a hand-built one with the same strings the loop sees).
    let bns_type = InvalidVariant {
        field: "BnsTpCode".into(),
        class: "type".into(),
        request: seed.clone(),
    };
    assert!(
        order_variant_may_fire(&schema, &bns_type),
        "only the marked class skips — the marked field's type variant still fires"
    );
}

#[test]
fn embedded_cspat00601_schema_skips_audited_booking_determining_variants_only() {
    // U2 pin against the REAL embedded metadata (constraints/CSPAT00601.yaml):
    // the audited booking-determining fields — BnsTpCode (PROVEN, §30
    // ordno=17093) and the three PROVISIONAL mode selectors (R11:
    // OrdprcPtnCode / OrdCndiTpCode / MgntrnCode) — are marked
    // `booking_determining: [required]`, so their generated required-omit
    // variants are structurally unroutable; the reject-expected PROVEN fields
    // (IsuNo → 01407, OrdQty → IGW40011) still fire.
    let schema = ls_core::schema_for("CSPAT00601").expect("CSPAT00601 schema");
    let seed = serde_json::json!({
        "IsuNo": "005930",
        "OrdQty": 1,
        "OrdPrc": 50000,
        "BnsTpCode": "2",
        "OrdprcPtnCode": "00",
        "MgntrnCode": "000",
        "OrdCndiTpCode": "0"
    });
    let variants = generate_invalid_variants(schema, &seed);
    let find = |field: &str, class: &str| {
        variants
            .iter()
            .find(|v| v.field == field && v.class == class)
            .unwrap_or_else(|| panic!("{field}/{class} variant is generated"))
    };
    for field in ["BnsTpCode", "OrdprcPtnCode", "OrdCndiTpCode", "MgntrnCode"] {
        assert!(
            is_booking_determining(schema, field, "required"),
            "{field}/required must be annotated booking-determining"
        );
        assert!(
            !order_variant_may_fire(schema, find(field, "required")),
            "{field}/required must never be dispatched"
        );
    }
    // Reject-expected, PROVEN fields (§30): omission is refused before
    // placement, so their variants still fire.
    assert!(
        order_variant_may_fire(schema, find("IsuNo", "required")),
        "IsuNo/required still fires (01407 reject observed, §30)"
    );
    assert!(
        order_variant_may_fire(schema, find("OrdQty", "required")),
        "OrdQty/required still fires (IGW40011 reject observed, §30)"
    );
}

// --- IGW00000 A/B offline twins (plan 2026-07-14-001 U5) -------------------

fn t0425_row(ordno: &str, price: &str, qty: &str, cheqty: &str, ordrem: &str) -> T0425OutBlock1 {
    T0425OutBlock1 {
        ordno: ordno.into(),
        price: price.into(),
        qty: qty.into(),
        cheqty: cheqty.into(),
        ordrem: ordrem.into(),
        ..Default::default()
    }
}

#[test]
fn seed_snapshot_from_finds_the_seed_by_normalized_ordno_else_absent() {
    // The snapshot matches the seed on `normalize_ordno` (a zero-padded scan ordno vs
    // a numeric submit ordno still resolves) and records its mutable fields; a seed
    // absent from the scan yields the default (present=false).
    let rows = vec![
        t0425_row("0000012345", "50000", "1", "0", "1"),
        t0425_row("999", "60000", "2", "0", "2"),
    ];
    let snap = seed_snapshot_from(&rows, "12345");
    assert!(snap.present && snap.price == "50000" && snap.qty == "1");
    assert!(!seed_snapshot_from(&rows, "77777").present, "absent seed → default");
}

#[test]
fn has_new_resting_order_ignores_the_seed_and_non_resting_rows() {
    // A foreign RESTING row (cheqty==0, ordrem>0) that is not the seed is a phantom;
    // the seed itself and any filled/partial row are not counted.
    let seed = "12345";
    assert!(!has_new_resting_order(&[t0425_row("12345", "50000", "1", "0", "1")], seed), "only the seed rests");
    assert!(
        has_new_resting_order(&[t0425_row("12345", "50000", "1", "0", "1"), t0425_row("99", "51000", "1", "0", "1")], seed),
        "a foreign resting row is a new order"
    );
    assert!(
        !has_new_resting_order(&[t0425_row("12345", "50000", "1", "0", "1"), t0425_row("99", "51000", "1", "1", "0")], seed),
        "a foreign FILLED row (ordrem==0) is not a new resting order"
    );
}

#[test]
fn classify_igw00000_ab_covers_every_bind_signature_arm() {
    let present = |price: &str| SeedSnapshot {
        present: true,
        price: price.into(),
        qty: "1".into(),
        cheqty: "0".into(),
        ordrem: "1".into(),
    };
    let s_pre = present("51000");
    let clean = ControlDisposition::CleanlyCanceled;

    // placed-nothing: fire=500/IGW00000, seed present & byte-identical, no new order,
    // clean cancel.
    assert_eq!(
        classify_igw00000_ab(500, "IGW00000", true, &s_pre, &present("51000"), false, &clean),
        AbVerdict::PlacedNothing
    );
    // may-rest — the seed MUTATED (price changed).
    assert_eq!(
        classify_igw00000_ab(500, "IGW00000", true, &s_pre, &present("52000"), false, &clean),
        AbVerdict::MayRest
    );
    // may-rest — the seed VANISHED from a trusted S_post (plan: vanished is may-rest).
    assert_eq!(
        classify_igw00000_ab(500, "IGW00000", true, &s_pre, &SeedSnapshot::default(), false, &clean),
        AbVerdict::MayRest
    );
    // may-rest — the seed FILLED (cancel gateway-rejected on a flat book).
    assert_eq!(
        classify_igw00000_ab(500, "IGW00000", true, &s_pre, &present("51000"), false, &ControlDisposition::Filled(vec![])),
        AbVerdict::MayRest
    );
    // may-rest — a NEW order rested.
    assert_eq!(
        classify_igw00000_ab(500, "IGW00000", true, &s_pre, &present("51000"), true, &clean),
        AbVerdict::MayRest
    );
    // inconclusive — untrusted read (throttle/scan failure) is NEVER placed-nothing (#137).
    assert_eq!(
        classify_igw00000_ab(500, "IGW00000", false, &s_pre, &present("51000"), false, &clean),
        AbVerdict::Inconclusive
    );
    // inconclusive — could not snapshot the control pre-fire.
    assert_eq!(
        classify_igw00000_ab(500, "IGW00000", true, &SeedSnapshot::default(), &present("51000"), false, &clean),
        AbVerdict::Inconclusive
    );
    // inconclusive — the fire is NOT the characterized 500/IGW00000 surface, even with
    // an otherwise-clean seed.
    assert_eq!(
        classify_igw00000_ab(500, "IGW50008", true, &s_pre, &present("51000"), false, &clean),
        AbVerdict::Inconclusive
    );
    // inconclusive — seed byte-identical but it did NOT cancel cleanly (StillResting):
    // cannot positively conclude placed-nothing.
    assert_eq!(
        classify_igw00000_ab(500, "IGW00000", true, &s_pre, &present("51000"), false, &ControlDisposition::StillResting(vec![])),
        AbVerdict::Inconclusive
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

    // A realistic CSPAT00701 MODIFY response: OutBlock1 echoes OrgOrdNo (the OLD/parent
    // number), OutBlock2 carries the NEW child OrdNo plus PrntOrdNo/aux. A modify is
    // absolute and reassigns the number (KTD4), so the A/B re-key MUST land on the child
    // OrdNo (OutBlock2.OrdNo) — never the echoed OrgOrdNo or the parent PrntOrdNo. This
    // locks in the "never a foreign/stale order" property the live-only re-key depends on;
    // an extractor that matched OrgOrdNo/PrntOrdNo would re-key onto a vanished order and
    // silently mis-verdict the A/B (see docs/solutions/logic-errors/
    // modify-reassigns-order-number-ab-harness-must-rekey-child.md).
    let modify = serde_json::json!({
        "rsp_cd": "00462",
        "CSPAT00701OutBlock1": { "RecCnt": 1, "OrgOrdNo": 22126, "IsuNo": "005930", "OrdQty": 1 },
        "CSPAT00701OutBlock2": { "RecCnt": 1, "OrdNo": 22127, "PrntOrdNo": 22126, "SpareOrdNo": 0, "RsvOrdNo": 0 }
    });
    assert_eq!(
        extract_ord_no(&modify),
        Some("22127".to_string()),
        "the modify re-key must select the child OrdNo (OutBlock2), not the echoed OrgOrdNo or the parent PrntOrdNo"
    );
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
fn read_throttle_inconclusive_reads_held_not_clean() {
    // U2/KTD2/KTD3: the read-leg merits-allowlist inversion, twinned offline through
    // the SAME `read_reported_label` the live loop calls (KTD5). A non-merits variant
    // fired against a PASSING control must NOT read a false `Clean`.
    let t1101 = ls_core::schema_for("t1101").expect("t1101 schema"); // unmarked → no tolerance noise
    let t0425 = ls_core::schema_for("t0425").expect("t0425 schema");
    let t8412 = ls_core::schema_for("t8412").expect("t8412 schema"); // carries the marked cross_field

    // The motivating §27 reason-A case: an IGW00201 throttle on a passing control was
    // false-`Clean` before U1/U2; it now reads `Held`, rendered `Held-throttle`.
    assert_eq!(
        read_variant_verdict(500, "IGW00201"),
        VariantVerdict::Inconclusive
    );
    assert_eq!(
        classify_probe(true, read_variant_verdict(500, "IGW00201")),
        ProbeOutcome::Held,
        "a throttle on a passing control is inconclusive, not Clean"
    );
    assert_eq!(
        read_reported_label(t1101, "shcode", "required", true, 500, "IGW00201"),
        "Held-throttle",
        "§27 reason A: the throttle is now visible as Held-throttle, not false-Clean"
    );

    // Merits rejects stay `Clean` — no certified disposition regresses.
    assert_eq!(
        read_reported_label(t1101, "shcode", "required", true, 500, "IGW40011"),
        "Clean",
        "IGW40011 is a merits reject — Clean preserved"
    );
    assert_eq!(
        read_reported_label(t0425, "sortgb", "required", true, 500, "IGW40013"),
        "Clean",
        "§30 t0425 sortgb/required → IGW40013 → Clean anchor preserved"
    );
    // t1101's certified CLEAN chain rejects `shcode/required` with the BUSINESS code
    // 00009 (error-coverage/t1101.yaml, attended 2026-07-06), not an IGW code — the
    // merits allowlist must carry it or this recommended TR regresses Clean → Held.
    assert_eq!(
        read_variant_verdict(200, "00009"),
        VariantVerdict::Rejected,
        "00009 is a business merits reject regardless of HTTP status"
    );
    assert_eq!(
        read_reported_label(t1101, "shcode", "required", true, 200, "00009"),
        "Clean",
        "t1101 shcode/required → 00009 → Clean anchor preserved"
    );

    // An accepted invalid variant (2xx success) is still a Divergent — unchanged.
    assert_eq!(
        read_reported_label(t1101, "shcode", "required", true, 200, "00000"),
        "Divergent",
        "an accepted invalid variant is a divergence"
    );

    // Accepted requires 2xx AND is_success: a control-success code (00000) arriving
    // at a NON-2xx status is NOT an acceptance — it is inconclusive (the gateway did
    // not deliver a 2xx), so it reads Held, never Divergent. Pins the http half of
    // the Accepted gate so dropping it would fail here.
    assert_eq!(
        read_variant_verdict(500, "00000"),
        VariantVerdict::Inconclusive,
        "a success code at non-2xx is not an acceptance"
    );
    assert_eq!(
        read_reported_label(t1101, "shcode", "required", true, 500, "00000"),
        "Held",
        "success code at non-2xx → inconclusive Held, not Divergent"
    );

    // The gateway_tolerant (KTD4) downgrade must still fire THROUGH the shared
    // `read_reported_label` (not only via `reported_outcome` in isolation): a marked
    // cross_field divergence downgrades to expected-tolerant even after the
    // throttle-label layer. (§30 t8412 sdate/edate.)
    assert_eq!(
        read_reported_label(t8412, "sdate/edate", "cross_field", true, 200, "00000"),
        "expected-tolerant",
        "a marked-tolerant divergence downgrades through the shared read helper"
    );

    // Strict inversion: an UNKNOWN read reject code is now inconclusive (`Held`), not
    // `Clean` — but it renders plain `Held` (distinguished by its printed rsp_cd),
    // NOT `Held-throttle`, which is reserved for a catalogued noneval code.
    assert_eq!(
        read_variant_verdict(500, "40510"),
        VariantVerdict::Inconclusive
    );
    assert_eq!(
        read_reported_label(t1101, "shcode", "required", true, 500, "40510"),
        "Held",
        "an unknown read reject is inconclusive (Held), not false-Clean and not Held-throttle"
    );

    // The `Held-throttle` label fires ONLY for a noneval Inconclusive on a passing
    // control — never for a control-fail Held (the throttle detail is moot then).
    assert_eq!(throttle_label(true, "IGW00201"), Some("Held-throttle"));
    assert_eq!(throttle_label(false, "IGW00201"), None, "control-fail is a plain Held");
    assert_eq!(throttle_label(true, "40510"), None, "unknown code is not a throttle");
    assert_eq!(throttle_label(true, "IGW40011"), None, "a merits reject is not a throttle");

    // The seed predicate itself.
    assert!(is_read_merits_reject("IGW40011"));
    assert!(is_read_merits_reject("IGW40013"));
    assert!(is_read_merits_reject("00009"));
    assert!(!is_read_merits_reject("IGW00201"));
    assert!(!is_read_merits_reject("00000"));
    assert!(!is_read_merits_reject("40510"));
}

#[test]
fn token_throttle_inconclusive_reads_held_not_clean() {
    // U3/KTD2: the token noneval carve-out, twinned offline through the SAME
    // `token_reported_label` the token loop calls (KTD5). Token's genuine-refusal
    // signal (non-2xx / non-success envelope) is collapsed into `ok` by
    // `token_fire`; the only diversion is a catalogued noneval code.

    // A throttle on a passing control is inconclusive → Held-throttle (was Clean).
    assert_eq!(
        token_variant_verdict("IGW00201", false),
        VariantVerdict::Inconclusive
    );
    // Noneval is checked BEFORE the ok arm: a catalogued throttle stays Inconclusive
    // even if the response somehow carried a token (ok=true). Pins that branch order.
    assert_eq!(
        token_variant_verdict("IGW00201", true),
        VariantVerdict::Inconclusive,
        "noneval short-circuits before the ok→Accepted arm"
    );
    assert_eq!(
        token_reported_label(true, "IGW00201", false),
        "Held-throttle",
        "an IGW00201 token throttle is now visible, not a false-Clean"
    );

    // A genuine OAuth refusal (ok=false, non-noneval code — e.g. the IGW00121 /
    // IGW00002 auth-reject candidates, OQ1) stays Clean — the certified token
    // disposition does not regress.
    assert_eq!(
        token_variant_verdict("IGW00121", false),
        VariantVerdict::Rejected
    );
    assert_eq!(
        token_reported_label(true, "IGW00121", false),
        "Clean",
        "a genuine OAuth refusal is unchanged Clean"
    );

    // An invalid token variant that was ACCEPTED (ok=true) is a divergence — unchanged.
    assert_eq!(token_variant_verdict("00000", true), VariantVerdict::Accepted);
    assert_eq!(
        token_reported_label(true, "00000", true),
        "Divergent",
        "an accepted invalid token variant is a divergence"
    );

    // A control-fail Held is a plain Held, never Held-throttle (throttle detail moot).
    assert_eq!(
        token_reported_label(false, "IGW00201", false),
        "Held",
        "control-fail dominates — a plain Held, not Held-throttle"
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

// --- Booking-determining A/B offline twins (governed harness, U3) -----------

#[test]
fn booking_ab_field_gate_accepts_only_annotated_fields() {
    // The governed harness REFUSES (pure, no dispatch) any field that is not
    // annotated `booking_determining: [required]` in the embedded CSPAT00601
    // schema — an unannotated reject-expected sibling and an unknown field are
    // both refused; the proven BnsTpCode and the three provisional (R11) mode
    // selectors are accepted.
    let schema = ls_core::schema_for("CSPAT00601").expect("CSPAT00601 schema");
    for field in ["BnsTpCode", "OrdprcPtnCode", "OrdCndiTpCode", "MgntrnCode"] {
        assert!(
            booking_ab_field_gate(schema, field).is_ok(),
            "{field} is annotated booking-determining — the gate must accept it"
        );
    }
    // An unannotated (reject-expected, §30-proven) sibling → refused, no dispatch.
    let err = booking_ab_field_gate(schema, "IsuNo").expect_err("IsuNo must be refused");
    assert!(err.contains("not annotated"), "msg names the missing annotation: {err}");
    // An unknown field → refused, no dispatch.
    let err = booking_ab_field_gate(schema, "NoSuchField").expect_err("unknown must be refused");
    assert!(err.contains("not a field"), "msg names the unknown field: {err}");
}

#[test]
fn classify_booking_ab_covers_every_verdict_arm() {
    // The governed booking A/B verdict (pure, offline-twinnable). Precedence:
    // transport/untrusted fail closed to inconclusive; an observed FILL outranks
    // an observed resting row; observations outrank the fired rsp_cd; `rejected`
    // requires a positive placed-nothing merits reject with NOTHING observed.
    //
    // places-defaulted-order(rested): trusted reads + a new resting row.
    assert_eq!(
        classify_booking_ab(Some((200, "00040")), true, true, false),
        BookingAbVerdict::PlacesDefaultedOrderRested
    );
    // places-defaulted-order(filled): a detected fill.
    assert_eq!(
        classify_booking_ab(Some((200, "00040")), true, false, true),
        BookingAbVerdict::PlacesDefaultedOrderFilled
    );
    // A fill OUTRANKS a resting row (both observed → filled).
    assert_eq!(
        classify_booking_ab(Some((200, "00040")), true, true, true),
        BookingAbVerdict::PlacesDefaultedOrderFilled
    );
    // An observation outranks a reject-shaped rsp_cd: a rested row with a reject
    // code is STILL places-defaulted-order (the book is the truth, not the code).
    assert_eq!(
        classify_booking_ab(Some((200, "01407")), true, true, false),
        BookingAbVerdict::PlacesDefaultedOrderRested
    );
    // rejected: a recognized merits reject (placed nothing) with nothing observed —
    // a 2xx business reject and the IGW40011-at-500 ingress reject both qualify.
    assert_eq!(
        classify_booking_ab(Some((200, "01407")), true, false, false),
        BookingAbVerdict::Rejected
    );
    assert_eq!(
        classify_booking_ab(Some((500, "IGW40011")), true, false, false),
        BookingAbVerdict::Rejected
    );
    // inconclusive: a throttle/noneval code is not a merits answer (any status).
    assert_eq!(
        classify_booking_ab(Some((200, "IGW00201")), true, false, false),
        BookingAbVerdict::Inconclusive
    );
    assert_eq!(
        classify_booking_ab(Some((503, "IGW00201")), true, false, false),
        BookingAbVerdict::Inconclusive
    );
    // inconclusive: transport failure — nothing answered, fail-closed teardown.
    assert_eq!(
        classify_booking_ab(None, true, false, false),
        BookingAbVerdict::Inconclusive
    );
    // inconclusive: untrusted post-fire reads are NEVER evidence (#137) — even a
    // reject-shaped code cannot conclude `rejected` (which asserts placed-nothing).
    assert_eq!(
        classify_booking_ab(Some((200, "01407")), false, false, false),
        BookingAbVerdict::Inconclusive
    );
    // inconclusive: an acceptance ack with NOTHING observable is ambiguous —
    // never `rejected`, never places-defaulted-order.
    assert_eq!(
        classify_booking_ab(Some((200, "00040")), true, false, false),
        BookingAbVerdict::Inconclusive
    );
    // inconclusive: any other 5xx stays may-rest-shaped (not a merits reject).
    assert_eq!(
        classify_booking_ab(Some((500, "IGW50008")), true, false, false),
        BookingAbVerdict::Inconclusive
    );
    // FALSE-LIFT GUARD (adversarial + cross-model): a `PlacedNothing`-shaped answer
    // is NOT a merits reject unless the code is allowlisted. Pre-fix these returned
    // `Rejected` and would have LIFTED a booking-determining annotation on a field
    // that places real orders.
    //   - HTTP 429 throttle degraded to an empty rsp_cd (4xx, so not may-rest-5xx):
    assert_eq!(
        classify_booking_ab(Some((429, "")), true, false, false),
        BookingAbVerdict::Inconclusive
    );
    //   - a placed-nothing business reject for a reason UNRELATED to the omission
    //     (e.g. an un-catalogued inventory/combination code):
    assert_eq!(
        classify_booking_ab(Some((200, "09999")), true, false, false),
        BookingAbVerdict::Inconclusive
    );
    // is_booking_ab_merits_reject allowlist is exactly {ingress-validation, 01407}.
    assert!(is_booking_ab_merits_reject("IGW40011"));
    assert!(is_booking_ab_merits_reject("01407"));
    assert!(!is_booking_ab_merits_reject(""));
    assert!(!is_booking_ab_merits_reject("09999"));
}

#[test]
fn plan_close_out_is_sign_aware_and_close_only() {
    // The sign-aware close-out plan (pure): a defaulted order that BOUGHT is
    // closed by a SELL of the janqty delta (capped at the sellable qty — never
    // oversell); one that SOLD is closed by a BUY back of the delta (returns to
    // the pre state, never beyond flat); no delta → no close order.
    assert_eq!(plan_close_out(0, 1, 1), Some(CloseSide::Sell(1)), "bought 1 → sell 1");
    assert_eq!(plan_close_out(3, 5, 5), Some(CloseSide::Sell(2)), "bought 2 on a base → sell 2");
    assert_eq!(plan_close_out(0, 3, 2), Some(CloseSide::Sell(2)), "sell capped at sellable");
    assert_eq!(
        plan_close_out(0, 1, 0),
        None,
        "bought but zero sellable (unsettled) → no close now; surfaced to the operator"
    );
    assert_eq!(plan_close_out(1, 0, 0), Some(CloseSide::Buy(1)), "sold 1 → buy 1 back");
    assert_eq!(plan_close_out(5, 3, 3), Some(CloseSide::Buy(2)), "sold 2 → buy 2 back");
    assert_eq!(plan_close_out(2, 2, 2), None, "no delta → no close order");
}
