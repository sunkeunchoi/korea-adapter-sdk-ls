//! The Specification Document Tracker — example drift and advisory artifact
//! pointers, built parallel to [`crate::api_drift`] over the **same** staged raw
//! snapshot.
//!
//! The API Drift normalizer never reads the `req_example`/`res_example` fields
//! carried on every [`RawTr`](crate::fetch::RawTr); this tracker projects exactly
//! that latent facet. One staged snapshot feeds two non-overlapping lenses: API
//! Drift owns structural shape plus description/`korean_name`, and this tracker
//! owns request/response examples.
//!
//! Three layers live here:
//!
//! * [`normalize_example_run`] (U1) projects every TR carrying an example into a
//!   per-payload-class [`ExampleShape`] under a dedicated
//!   [`EXAMPLE_NORMALIZER_VERSION`]. The projection is **full-inventory** (KTD8)
//!   so an untracked TR's in-place example change is detectable, but it stores
//!   only structural descriptors — never a raw example value (KTD7).
//! * [`compare_examples`] (U2) diffs a staged projection against the Reviewed
//!   Baseline into support-aware, **advisory (never-gating)** [`SpecFinding`]s.
//! * [`spec_targets`] (U3) resolves a Tracked TR to the maintained SDK artifacts
//!   (reference + dependency docs) a changed example should prompt review of.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::fetch::RawInventory;
use crate::types::{ExampleFacet, ExampleShape, FieldShape};

/// The example-normalizer version recorded in the [`ExampleManifest`] (R8/KTD2).
/// It starts independent of [`NORMALIZER_VERSION`](crate::api_drift::NORMALIZER_VERSION)
/// so the example projection has its own re-baseline cadence; bump it when the
/// example normalization rules change so an example-hash shift is auditable.
pub const EXAMPLE_NORMALIZER_VERSION: u32 = 1;

/// The normalized projection of one fetched run's **examples** (U1): the
/// full-inventory code-set, an [`ExampleManifest`], and per-TR [`ExampleShape`]s
/// for every TR carrying a non-empty example (KTD8). Mirrors
/// [`NormalizedRun`](crate::api_drift::NormalizedRun) but for the example facet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExampleRun {
    pub code_set: crate::types::CodeSet,
    pub manifest: ExampleManifest,
    /// Example shapes keyed by `tr_code` (sorted, deterministic).
    pub shapes: BTreeMap<String, ExampleShape>,
}

/// Inventory facts for an example projection (R8): full code-set size, the count
/// of TRs carrying an example, the source URLs, and the example-normalizer
/// version (so a normalizer change is auditable and a cross-version compare is
/// refused, U4). Distinct from the API Drift [`Manifest`](crate::types::Manifest)
/// so the two baselines version independently (KTD2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExampleManifest {
    /// Total upstream TR count for this snapshot (full inventory).
    pub upstream_tr_count: usize,
    /// Number of TRs carrying a non-empty request or response example.
    pub example_tr_count: usize,
    /// Source URLs the snapshot was scraped from.
    #[serde(default)]
    pub source_urls: Vec<String>,
    /// The example-normalizer version that produced these shapes.
    pub normalizer_version: u32,
}

/// Project a fetched [`RawInventory`] into an [`ExampleRun`]: the full-inventory
/// code-set plus an [`ExampleShape`] for **every** TR carrying a non-empty
/// request or response example (KTD8). A TR with no example contributes only its
/// code to the code-set, never a shape. `provisional` flags the code-set as a
/// not-yet-attested seed (KTD-5), mirroring
/// [`normalize_run`](crate::api_drift::normalize_run).
///
/// Support state is **not** used to filter the projection — the example baseline
/// is full-inventory so an untracked TR's in-place example change is detectable
/// (R3, R6/AE2). Severity weighting by support happens later, at compare time.
pub fn normalize_example_run(inventory: &RawInventory, provisional: bool) -> ExampleRun {
    let code_set = inventory.code_set(provisional);

    let mut shapes = BTreeMap::new();
    for group in &inventory.groups {
        for raw in &group.trs {
            let code = raw.code.trim().to_string();
            if code.is_empty() {
                continue;
            }
            let shape = ExampleShape {
                tr_code: code.clone(),
                req: classify_example(&raw.req_example),
                res: classify_example(&raw.res_example),
            };
            // Skip TRs whose examples are both empty — they carry no facet to
            // baseline. Key by TR code, last-writer-wins, with the same same-code
            // dedup caution as the API Drift normalizer (carried R-3).
            if shape.is_absent() {
                continue;
            }
            shapes.insert(code, shape);
        }
    }

    let manifest = ExampleManifest {
        upstream_tr_count: code_set.len(),
        example_tr_count: shapes.len(),
        source_urls: inventory.source_urls.clone(),
        normalizer_version: EXAMPLE_NORMALIZER_VERSION,
    };

    ExampleRun {
        code_set,
        manifest,
        shapes,
    }
}

/// Classify one raw `req_example`/`res_example` [`Value`] into a per-class
/// [`ExampleFacet`] (KTD3). LS stores examples as opaque strings of mixed
/// encoding (JSON-as-text, form-encoded, or free text), with `null`/empty for
/// TRs that carry none.
///
/// * `null` / empty / whitespace-only → [`ExampleFacet::Absent`].
/// * JSON-parseable (a JSON string, or an already-structured value) → reduce to a
///   field-path → leaf [`FieldShape`] map, discarding scalar values (R2, AE4).
/// * form-encoded (`k=v&k=v`, no JSON braces) → reduce to its key set, discarding
///   values so a secret-only change is invisible (R2, AE5).
/// * anything else (single-quoted pseudo-JSON, truncated payloads, free text) →
///   [`ExampleFacet::Opaque`], carrying no structure (R2, R9).
///
/// No raw value or scalar is ever returned — only paths, keys, and shapes (KTD7).
fn classify_example(value: &Value) -> ExampleFacet {
    match value {
        Value::Null => ExampleFacet::Absent,
        // An example stored as an already-structured JSON value (object/array):
        // project its shape directly.
        Value::Object(_) | Value::Array(_) | Value::Bool(_) | Value::Number(_) => {
            ExampleFacet::Json {
                shape: json_shape(value),
            }
        }
        Value::String(s) => classify_example_str(s),
    }
}

/// Classify a string example by payload class. Trims leading/trailing whitespace
/// first (LS examples are wrapped in `\r\n` noise) before attempting each class.
fn classify_example_str(raw: &str) -> ExampleFacet {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return ExampleFacet::Absent;
    }
    // JSON-as-text: the dominant class. `from_str` skips leading/trailing
    // whitespace, so the `\r\n`-wrapped examples parse cleanly.
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return ExampleFacet::Json {
            shape: json_shape(&value),
        };
    }
    // Form-encoded: `k=v(&k=v)*` with no JSON braces. Only `token`/`revoke`
    // requests take this path upstream.
    if let Some(keys) = form_keys(trimmed) {
        return ExampleFacet::Form { keys };
    }
    // Free text, single-quoted pseudo-JSON, or truncated payloads: opaque. Never
    // silently dropped — marked explicitly so a class transition is observable
    // (the R-1-class trap), but carries no structure to compare.
    ExampleFacet::Opaque
}

/// Reduce a parsed JSON example to a sorted field-path → leaf [`FieldShape`] map,
/// reusing the API Drift leaf walker (KTD3) so example shape-diffing is identical
/// to the structural leaf model. Scalar sample values are discarded — only the
/// leaf shape per collapsed path survives, so value churn is not drift (AE4).
fn json_shape(value: &Value) -> BTreeMap<String, FieldShape> {
    let mut fields = BTreeMap::new();
    crate::stages::collect_paths(value, String::new(), &mut fields);
    fields
}

/// Parse a form-encoded example into its sorted key set, or `None` when the
/// string is not form-encoded. A form example is `k=v` pairs joined by `&`, with
/// no JSON object brace; each pair's key is the text before its first `=`, so a
/// value containing `=` (e.g. base64 padding in a JWT) does not corrupt the key.
/// Values are discarded entirely — only keys are retained (KTD7, AE5).
fn form_keys(trimmed: &str) -> Option<BTreeSet<String>> {
    if trimmed.contains('{') || !trimmed.contains('=') {
        return None;
    }
    let mut keys = BTreeSet::new();
    for pair in trimmed.split('&') {
        let (key, _value) = pair.split_once('=')?;
        let key = key.trim();
        if key.is_empty() {
            return None;
        }
        keys.insert(key.to_string());
    }
    Some(keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::{RawGroup, RawTr};

    fn raw_tr(code: &str, req: Value, res: Value) -> RawTr {
        RawTr {
            code: code.to_string(),
            name: Some(code.to_string()),
            is_websocket: false,
            http_method: Some("POST".to_string()),
            url: Some(format!("/{code}")),
            protocol_type: Some("REST".to_string()),
            rate_limit_per_sec: Some(1),
            corp_rate_limit_per_sec: None,
            description: None,
            properties: vec![],
            req_example: req,
            res_example: res,
        }
    }

    fn inventory(trs: Vec<RawTr>) -> RawInventory {
        RawInventory {
            source_urls: vec!["https://openapi.ls-sec.co.kr/apiservice".to_string()],
            property_types: BTreeMap::new(),
            groups: vec![RawGroup {
                category_name: "c".to_string(),
                group_id: Some("g".to_string()),
                group_name: "그룹".to_string(),
                is_websocket_group: false,
                trs,
            }],
        }
    }

    fn json_str(s: &str) -> Value {
        Value::String(s.to_string())
    }

    /// AE4: a JSON example whose structure is unchanged but a scalar value differs
    /// (timestamp, account number) normalizes to the identical `ExampleShape` —
    /// value churn is discarded, only the leaf-path shape survives.
    #[test]
    fn json_sample_value_churn_normalizes_identically() {
        let a = classify_example(&json_str(r#"{"blk":{"date":"20240906","price":4535}}"#));
        let b = classify_example(&json_str(r#"{"blk":{"date":"20231201","price":99}}"#));
        assert_eq!(a, b, "scalar value churn must not change the shape");
        match a {
            ExampleFacet::Json { shape } => {
                assert_eq!(shape.get("blk.date"), Some(&FieldShape::String));
                assert_eq!(shape.get("blk.price"), Some(&FieldShape::Number));
            }
            other => panic!("expected Json, got {other:?}"),
        }
    }

    /// A JSON example with an added field normalizes to a shape carrying the new
    /// path — a structural change is visible.
    #[test]
    fn json_added_field_changes_the_shape() {
        let base = classify_example(&json_str(r#"{"blk":{"price":1}}"#));
        let with_field = classify_example(&json_str(r#"{"blk":{"price":1,"qty":2}}"#));
        assert_ne!(base, with_field);
        if let ExampleFacet::Json { shape } = with_field {
            assert!(shape.contains_key("blk.qty"));
        } else {
            panic!("expected Json");
        }
    }

    /// AE5: the `token` form-encoded request normalizes to a key set; adding a key
    /// changes it; changing only a secret value does not.
    #[test]
    fn form_encoded_normalizes_to_key_set_ignoring_secret_values() {
        let base = classify_example(&json_str(
            "appkey=BSrTOOZNoXtx&appsecretkey=d3HloL6T&grant_type=client_credentials&scope=oob",
        ));
        match &base {
            ExampleFacet::Form { keys } => {
                assert_eq!(
                    keys.iter().cloned().collect::<Vec<_>>(),
                    vec!["appkey", "appsecretkey", "grant_type", "scope"]
                );
            }
            other => panic!("expected Form, got {other:?}"),
        }

        // Changing only a secret value (the appkey/appsecretkey) → identical keys.
        let rotated = classify_example(&json_str(
            "appkey=ROTATED9999&appsecretkey=NEWSECRET&grant_type=client_credentials&scope=oob",
        ));
        assert_eq!(base, rotated, "a secret-only rotation is not a key-set change");

        // Adding a key → a different key set.
        let extra = classify_example(&json_str(
            "appkey=X&appsecretkey=Y&grant_type=client_credentials&scope=oob&extra=1",
        ));
        assert_ne!(base, extra);
    }

    /// A form value containing `=` (base64 padding in a JWT) does not corrupt the
    /// key — only the text before the first `=` is the key.
    #[test]
    fn form_key_extraction_is_robust_to_equals_in_values() {
        let facet = classify_example(&json_str("appkey=X&token=eyJ0eXA==.payload=="));
        if let ExampleFacet::Form { keys } = facet {
            assert_eq!(keys.iter().cloned().collect::<Vec<_>>(), vec!["appkey", "token"]);
        } else {
            panic!("expected Form");
        }
    }

    /// A free-text / non-parseable example normalizes to `Opaque` with no
    /// structure — never silently dropped, but never shape-diffed.
    #[test]
    fn non_parseable_example_is_opaque() {
        let py = classify_example(&json_str(
            "Example Language : Python\r\n----------------------------------",
        ));
        assert_eq!(py, ExampleFacet::Opaque);
        // Single-quoted pseudo-JSON (invalid JSON) is opaque too.
        let single = classify_example(&json_str("{\r\n\t'sysflag': 'U',\r\n\t'flag': 'E'\r\n}"));
        assert_eq!(single, ExampleFacet::Opaque);
    }

    /// `null` / empty / whitespace-only examples are `Absent`.
    #[test]
    fn empty_examples_are_absent() {
        assert_eq!(classify_example(&Value::Null), ExampleFacet::Absent);
        assert_eq!(classify_example(&json_str("")), ExampleFacet::Absent);
        assert_eq!(classify_example(&json_str("   \r\n  ")), ExampleFacet::Absent);
    }

    /// A TR with both examples empty produces no shape; a TR with any example is
    /// projected. The projection is full-inventory: it does not filter by support.
    #[test]
    fn projection_skips_empty_and_keeps_any_example() {
        let inv = inventory(vec![
            raw_tr("withjson", json_str(r#"{"a":1}"#), Value::Null),
            raw_tr("withform", json_str("k=v"), Value::Null),
            raw_tr("noexample", Value::Null, Value::Null),
        ]);
        let run = normalize_example_run(&inv, false);
        assert_eq!(run.code_set.len(), 3, "all codes in the code-set");
        assert_eq!(run.shapes.len(), 2, "only TRs carrying an example get a shape");
        assert!(run.shapes.contains_key("withjson"));
        assert!(run.shapes.contains_key("withform"));
        assert!(!run.shapes.contains_key("noexample"));
        assert_eq!(run.manifest.upstream_tr_count, 3);
        assert_eq!(run.manifest.example_tr_count, 2);
        assert_eq!(run.manifest.normalizer_version, EXAMPLE_NORMALIZER_VERSION);
    }

    /// The example run serializes deterministically and round-trips, and the
    /// committed example values never appear in the serialized shape (KTD7).
    #[test]
    fn example_run_serializes_deterministically_without_values() {
        let inv = inventory(vec![raw_tr(
            "token",
            json_str("appkey=SECRET123&scope=oob"),
            json_str(r#"{"access_token":"eyJSECRETJWT"}"#),
        )]);
        let run = normalize_example_run(&inv, true);
        let a = serde_json::to_vec(&run).unwrap();
        let b = serde_json::to_vec(&run).unwrap();
        assert_eq!(a, b, "serialization is byte-stable");
        let back: ExampleRun = serde_json::from_slice(&a).unwrap();
        assert_eq!(back, run, "example run round-trips");

        let json = String::from_utf8(a).unwrap();
        assert!(!json.contains("SECRET123"), "the form secret must not be stored");
        assert!(!json.contains("eyJSECRETJWT"), "the JWT value must not be stored");
        assert!(json.contains("access_token"), "but the structural key is stored");
        assert!(json.contains("appkey"), "and the form key is stored");
    }

    /// Projection over the committed shared raw snapshot is deterministic and
    /// full-inventory: it yields an example shape for every TR carrying an example
    /// (~355 req / 344 res), including the 7 Tracked TRs with their expected
    /// payload classes (`token`/`revoke` requests are form-encoded; the rest are
    /// JSON). No real example value lands in the projection (KTD7).
    #[test]
    fn projects_committed_raw_full_inventory_without_values() {
        let raw_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("baselines/api-drift/raw/ls-openapi-full.json");
        let raw: RawInventory =
            serde_json::from_slice(&std::fs::read(&raw_path).expect("committed raw present"))
                .expect("committed raw parses");

        let run = normalize_example_run(&raw, true);
        assert_eq!(run.code_set.len(), 365, "full inventory code-set");
        // ~355 TRs carry at least one example (req or res); assert a healthy lower
        // bound rather than an exact count so a benign upstream re-scrape doesn't
        // brittle-fail this guard.
        assert!(
            run.shapes.len() >= 350,
            "most TRs carry an example, got {}",
            run.shapes.len()
        );

        // The 7 Tracked TRs all project.
        for code in ["token", "revoke", "t1102", "t8412", "CSPAQ12200", "S3_", "CSPAT00601"] {
            assert!(run.shapes.contains_key(code), "{code} projects an example");
        }

        // `token`/`revoke` requests are form-encoded; their responses are JSON.
        assert!(matches!(run.shapes["token"].req, ExampleFacet::Form { .. }));
        assert!(matches!(run.shapes["token"].res, ExampleFacet::Json { .. }));
        assert!(matches!(run.shapes["revoke"].req, ExampleFacet::Form { .. }));
        // `t1102` request + response are JSON.
        assert!(matches!(run.shapes["t1102"].req, ExampleFacet::Json { .. }));
        assert!(matches!(run.shapes["t1102"].res, ExampleFacet::Json { .. }));

        // The projection is deterministic (byte-stable across two runs).
        let again = normalize_example_run(&raw, true);
        assert_eq!(run, again, "projection over committed raw is deterministic");

        // No embedded credential reaches the projection: the real `appkey` secret
        // and the JWT prefix from the token examples are absent from the bytes.
        let bytes = serde_json::to_vec(&run).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(
            !text.contains("appsecretkey=") && !text.contains("eyJ0eXAiOiJKV1Q"),
            "no raw form pair or JWT value is serialized"
        );
    }
}
