//! Grounding gate (error-resilience gate U2/R4/KTD5): every authored per-TR
//! request field-constraint schema (`metadata/constraints/<tr>.yaml`) must have
//! its `type` + `required` declarations agree with the normalized baseline (the
//! wire-shape source of truth,
//! `crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json`). This is
//! the offline structural obligation that **blocks in CI** — enum/range/format
//! grading and behavioral confirmation are graded/probed elsewhere.
//!
//! Located in `ls-core` because this is where the baseline JSON is reachable and
//! `ls-metadata` is a dev-dependency; the grounding function itself lives in
//! `ls-metadata::constraints`.

use std::path::PathBuf;

use ls_metadata::{baseline_request_fields, ground_constraints, ConstraintSchema};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

#[test]
fn every_authored_constraint_schema_grounds_against_its_baseline() {
    let root = repo_root();
    let constraints_dir = root.join("metadata").join("constraints");
    let baselines_dir = root
        .join("crates")
        .join("ls-trackers")
        .join("baselines")
        .join("api-drift")
        .join("normalized")
        .join("trs");

    let mut checked = 0usize;
    for entry in std::fs::read_dir(&constraints_dir).expect("constraints dir exists") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        let tr = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("file stem")
            .to_string();

        let schema: ConstraintSchema = serde_yaml::from_str(
            &std::fs::read_to_string(&path).expect("read constraint schema"),
        )
        .unwrap_or_else(|e| panic!("constraints/{tr}.yaml must parse: {e}"));
        assert_eq!(schema.tr_code, tr, "constraints/{tr}.yaml tr_code mismatch");

        let baseline_path = baselines_dir.join(format!("{tr}.json"));
        let baseline_json: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&baseline_path)
                .unwrap_or_else(|e| panic!("baseline for {tr} must exist: {e}")),
        )
        .expect("baseline JSON parses");
        let baseline_fields = baseline_request_fields(&baseline_json);

        let errors = ground_constraints(&schema, &baseline_fields);
        assert!(
            errors.is_empty(),
            "constraints/{tr}.yaml does not ground against its baseline: {}",
            errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        );
        checked += 1;
    }

    assert!(
        checked >= 1,
        "expected at least the exemplar constraint schema to be grounded"
    );
}
