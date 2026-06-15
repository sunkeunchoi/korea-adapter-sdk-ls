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
