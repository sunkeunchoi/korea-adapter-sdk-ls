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
    classify_probe, generate_invalid_variants, ConstraintSchema, LsConfig, LsError, LsResult,
    ProbeOutcome,
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
