//! Integration gate: the U7 validator must pass over the real authored slice
//! metadata under `<workspace>/metadata/` (U8). This is the "validator passes
//! over the authored set" verification for U8 — if a per-TR file drifts from
//! the routing index, or carries an unknown enum value, this test fails.

use std::path::PathBuf;

use ls_metadata::validate_dir;

fn metadata_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <workspace>/crates/ls-metadata
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("metadata")
}

/// Raw-parse the machine-emitted top-level `date:` field from a TR's Focused
/// Evidence file (`metadata/evidence/<tr>.yaml`). Line-based on purpose: no
/// schema field links a TR to its evidence record yet, so this reads the
/// convention file directly rather than through `ls-metadata`.
fn evidence_date(tr: &str) -> String {
    let path = metadata_root()
        .join("evidence")
        .join(format!("{tr}.yaml"));
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read evidence file {}: {e}", path.display()));
    for line in content.lines() {
        if let Some(rest) = line.trim().strip_prefix("date:") {
            return rest.trim().trim_matches('"').to_string();
        }
    }
    panic!("no top-level `date:` field in {}", path.display());
}

#[test]
fn authored_slice_metadata_validates_clean() {
    let root = metadata_root();
    let report = match validate_dir(&root) {
        Ok(report) => report,
        Err(errors) => {
            let rendered: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
            panic!(
                "slice metadata under {} failed validation:\n  - {}",
                root.display(),
                rendered.join("\n  - ")
            );
        }
    };

    // Every slice TR must be present and agree with the index.
    for tr in ["token", "revoke", "t1102", "t8412", "CSPAQ12200", "S3_"] {
        assert!(
            report.trs.contains_key(tr),
            "expected slice TR `{tr}` in validated metadata"
        );
    }

    // t8430 is implemented (tracked-and-raw flip wave): the supposed array-shape
    // blocker was just a header-less Object Array response, modeled as
    // Vec<T8430OutBlock> via `de_vec_or_single` and confirmed on a clean paper
    // smoke (4291 issues). Implemented, not yet recommended.
    let t8430 = report
        .trs
        .get("t8430")
        .expect("t8430 must be present in the slice metadata");
    assert!(t8430.support.tracked, "t8430 is tracked");
    assert!(
        t8430.support.implemented,
        "t8430 is implemented (array-shape resolved via de_vec_or_single)"
    );
    assert!(
        !t8430.support.recommended,
        "t8430 is not recommended (Implemented only)"
    );
}

/// The error-resilience gate (plan 2026-07-01-004, R12) demoted the 10 TRs
/// promoted under the old happy-path gate back to Implemented; each re-promotes
/// only after passing the new differential-probe gate (U8, operator-run across
/// live windows). Until then the Recommended set is EMPTY, so the badge carries a
/// single consistent meaning ("this call fails gracefully"). This test guards
/// against an accidental re-promotion that skips the new gate.
#[test]
fn recommended_set_is_empty_pending_error_resilience_recert() {
    let report = validate_dir(&metadata_root()).expect("slice metadata validates");
    let recommended: Vec<&String> = report
        .trs
        .iter()
        .filter(|(_, m)| m.support.recommended)
        .map(|(code, _)| code)
        .collect();
    assert!(
        recommended.is_empty(),
        "no TR may be Recommended until re-certified through the error-resilience \
         gate (U8); found: {recommended:?}"
    );
}

/// Consistency guard: `token`'s `maintenance.last_reviewed` must equal the
/// machine-emitted `date:` in its Focused Evidence file. No schema field links
/// them yet, so this test is the only thing keeping the review anchor and the
/// run that justifies it from silently drifting.
#[test]
fn token_last_reviewed_matches_its_evidence_date() {
    let report = validate_dir(&metadata_root()).expect("slice metadata validates");
    let token = report.trs.get("token").expect("token present in metadata");
    let reviewed = &token.maintenance.last_reviewed;
    let evidence = evidence_date("token");
    assert_eq!(
        *reviewed, evidence,
        "token's last_reviewed ({reviewed}) must equal its evidence date ({evidence}) \
         — they cannot drift before an `evidence_ref` schema link exists"
    );
}

// --- U2: attested-shape fields on the evidence record ----------------------

use ls_metadata::shape::{BlockField, Direction};
use ls_metadata::{EvidenceRecord, Protocol, TrShape};

/// An evidence YAML authored before the attested-shape fields existed
/// deserializes with both new fields `None` (serde-default forward compat) — the
/// six real records (pre-U8 backfill) parse exactly this way.
#[test]
fn evidence_without_attested_shape_defaults_to_none() {
    let yaml = "\
tr_code: token
date: 2026-06-16
env: paper
target: live-smoke
line: \"LIVE-SMOKE result=[token_len=380 rsp_cd=00000]\"
";
    let record: EvidenceRecord = serde_yaml::from_str(yaml).expect("legacy evidence parses");
    assert_eq!(record.attested_shape, None);
    assert_eq!(record.attested_normalizer_version, None);
}

/// An evidence record carrying a full attested shape + normalizer version
/// round-trips through YAML and re-serializes equal (the captured-at-attestation
/// contract).
#[test]
fn evidence_with_attested_shape_round_trips() {
    let record = EvidenceRecord {
        tr_code: "token".to_string(),
        date: "2026-06-16".to_string(),
        env: "paper".to_string(),
        target: Some("live-smoke".to_string()),
        line: Some("LIVE-SMOKE result=[ok]".to_string()),
        attested_shape: Some(TrShape {
            tr_code: "token".to_string(),
            tr_name: Some("접근토큰 발급".to_string()),
            protocol: Protocol::Rest,
            is_websocket: false,
            endpoint_path: Some("/oauth2/token".to_string()),
            api_group_id: None,
            source_group_name: None,
            request_blocks: vec![BlockField {
                direction: Direction::Request,
                block_name: "request_body".to_string(),
                field_index: 0,
                field_name: "grant_type".to_string(),
                korean_name: None,
                r#type: Some("String".to_string()),
                length: Some(100),
                required: true,
                description_hash: Some("a739607c5d7c01a1".to_string()),
            }],
            response_blocks: vec![],
            rate_limit_per_sec: None,
            corp_rate_limit_per_sec: None,
            rate_source_group: None,
            description_hash: None,
        }),
        attested_normalizer_version: Some(2),
    };
    let yaml = serde_yaml::to_string(&record).expect("serialize");
    let back: EvidenceRecord = serde_yaml::from_str(&yaml).expect("round-trip parses");
    assert_eq!(back, record, "attested-shape evidence round-trips");
    assert_eq!(back.attested_normalizer_version, Some(2));
}

/// Unknown/extra fields in an evidence YAML are still ignored (existing behavior),
/// so a forward-compat field added later does not break older readers.
#[test]
fn evidence_ignores_unknown_fields() {
    let yaml = "\
tr_code: token
date: 2026-06-16
env: paper
some_future_field: ignored
attested_normalizer_version: 2
";
    let record: EvidenceRecord = serde_yaml::from_str(yaml).expect("parses with unknown field");
    assert_eq!(record.attested_normalizer_version, Some(2));
    assert_eq!(record.attested_shape, None);
}
