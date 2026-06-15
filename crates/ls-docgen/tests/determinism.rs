//! Golden determinism and `--check` drift integration tests (U4).
//!
//! These read the real authored `metadata/` set and the committed docs, so they
//! double as the `make docs-check` green-path guard and the AE1 drift example.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ls_docgen::{check_docs, metadata_root, render_all, render_dependency_docs, repo_root};
use ls_metadata::{validate_dir, OwnerClass};

fn authored_files() -> BTreeMap<PathBuf, String> {
    let report = validate_dir(&metadata_root()).expect("authored metadata must validate clean");
    render_all(&report)
}

/// The committed docs must equal what the generator produces from current
/// metadata — the `make docs-check` green path.
#[test]
fn committed_docs_match_current_metadata() {
    let drifted = check_docs(&repo_root(), &authored_files());
    assert!(
        drifted.is_empty(),
        "committed docs drift from metadata (run `make docs`): {drifted:?}"
    );
}

/// Golden: every rendered file equals the committed file byte-for-byte, and
/// re-rendering is stable.
#[test]
fn regeneration_is_byte_identical_to_committed() {
    let root = repo_root();
    let first = authored_files();
    let second = authored_files();
    assert_eq!(first, second, "rendering is not deterministic across runs");

    for (rel, expected) in &first {
        let committed = std::fs::read_to_string(root.join(rel))
            .unwrap_or_else(|e| panic!("read committed {}: {e}", rel.display()));
        assert_eq!(&committed, expected, "byte drift in {}", rel.display());
    }
}

/// AE1: a TR's `owner_class` changes and `--check` runs without regenerating →
/// the stale doc is named.
///
/// Drive the low-level `render_dependency_docs(&BTreeMap, &TrIndex)` path with an
/// inline-mutated metadata map, NOT `validate_dir` on a mutated `metadata/` dir —
/// a per-TR `owner_class` mutation there trips the validator's routing
/// cross-check (`RoutingMismatch`) before any doc renders, which would assert the
/// wrong failure.
#[test]
fn check_names_stale_doc_when_owner_class_drifts() {
    let report = validate_dir(&metadata_root()).expect("metadata validates");
    let mut trs = report.trs.clone();
    trs.get_mut("t8412").expect("t8412 present").owner_class = OwnerClass::Account;

    let drifted_docs = render_dependency_docs(&trs, &report.index);
    let drifted = check_docs(&repo_root(), &drifted_docs);

    assert!(
        drifted.iter().any(|p| p.ends_with("t8412.md")),
        "check must name the stale t8412 page; got {drifted:?}"
    );
}
