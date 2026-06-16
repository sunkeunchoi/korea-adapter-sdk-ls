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

    // t8430 is excluded (blocked upstream, R3) — it must not appear.
    assert!(
        !report.trs.contains_key("t8430"),
        "t8430 is blocked upstream and must not be in the slice metadata"
    );
}

/// `token` is the first Recommended TR (R1). Guards the promotion against an
/// accidental revert of `support.recommended`.
#[test]
fn token_is_the_first_recommended_tr() {
    let report = validate_dir(&metadata_root()).expect("slice metadata validates");
    let token = report.trs.get("token").expect("token present in metadata");
    assert!(
        token.support.recommended,
        "token must be recommended (the first Recommended TR)"
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
