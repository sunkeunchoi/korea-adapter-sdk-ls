use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub use repository_engineering_runtime::{machine, model};
use sha2::{Digest, Sha256};

#[path = "../src/comparison/mod.rs"]
mod comparison;

use comparison::{
    run_comparison, write_external_evidence, ComparisonCatalog, ComparisonError, ComparisonPolicy,
};

const POLICY: &str = ".repository-engineering/scenarios/audit-carried-rows/comparison-policy.toml";
const CATALOG: &str = ".repository-engineering/scenarios/audit-carried-rows/comparison-cases.toml";
const LEGACY_SENTINEL: &str = include_str!("fixtures/comparison/legacy-credential-sentinel.txt");
const SUCCESSOR_VERDICT: &str = include_str!("fixtures/comparison/successor-verdict-mutation.txt");

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "repository-engineering-comparison-{label}-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create test directory");
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repository root")
        .to_path_buf()
}

fn copy_file(source_root: &Path, target_root: &Path, relative: &str) {
    let target = target_root.join(relative);
    fs::create_dir_all(target.parent().expect("copied file parent"))
        .expect("create copied file parent");
    fs::copy(source_root.join(relative), target).expect("copy comparison input");
}

fn copied_repository() -> TestDirectory {
    let source = repository_root();
    let target = TestDirectory::new("repository");
    copy_file(&source, &target.0, POLICY);
    copy_file(&source, &target.0, CATALOG);

    let policy: ComparisonPolicy =
        toml::from_str(&fs::read_to_string(source.join(POLICY)).expect("comparison policy"))
            .expect("parse comparison policy");
    let references = [
        &policy.implementation_subject,
        &policy.capability_contract,
        &policy.worker_role_contract,
        &policy.executor,
        &policy.successor_scenario,
        &policy.migration_source_manifest,
        &policy.legacy_ledger,
        &policy.legacy_report,
        &policy.legacy_oracle,
    ];
    for reference in references
        .into_iter()
        .chain(policy.successor_conformance_basis.iter())
    {
        copy_file(&source, &target.0, &reference.path);
    }

    let catalog: ComparisonCatalog =
        toml::from_str(&fs::read_to_string(source.join(CATALOG)).expect("comparison catalog"))
            .expect("parse comparison catalog");
    for case in &catalog.cases {
        copy_file(&source, &target.0, &case.legacy_record.path);
        let text =
            fs::read_to_string(source.join(&case.legacy_record.path)).expect("legacy record");
        let pointer = text
            .lines()
            .find_map(|line| line.strip_prefix("evidence_pointer: "))
            .expect("evidence pointer");
        if pointer != "inline" && !target.0.join(pointer).exists() {
            copy_file(&source, &target.0, pointer);
        }
    }
    target
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn replace_once(path: &Path, old: &str, new: &str) {
    let text = fs::read_to_string(path).expect("mutation input");
    assert_eq!(
        text.matches(old).count(),
        1,
        "mutation target must be unique"
    );
    fs::write(path, text.replacen(old, new, 1)).expect("write mutation");
}

fn replace_first(path: &Path, old: &str, new: &str) {
    let text = fs::read_to_string(path).expect("mutation input");
    assert!(text.contains(old), "mutation target must exist");
    fs::write(path, text.replacen(old, new, 1)).expect("write mutation");
}

fn rebind_catalog(root: &Path) {
    let policy_path = root.join(POLICY);
    let policy: ComparisonPolicy =
        toml::from_str(&fs::read_to_string(&policy_path).expect("policy")).expect("parse policy");
    let new_digest = digest(&fs::read(root.join(CATALOG)).expect("catalog"));
    replace_once(&policy_path, &policy.case_catalog.sha256, &new_digest);
}

fn rebind_record(root: &Path, case_id: &str) {
    let catalog_path = root.join(CATALOG);
    let catalog: ComparisonCatalog =
        toml::from_str(&fs::read_to_string(&catalog_path).expect("catalog"))
            .expect("parse catalog");
    let case = catalog
        .cases
        .iter()
        .find(|case| case.case_id == case_id)
        .expect("catalog case");
    let new_digest = digest(&fs::read(root.join(&case.legacy_record.path)).expect("record"));
    replace_once(&catalog_path, &case.legacy_record.sha256, &new_digest);
    rebind_catalog(root);
}

fn snapshot(root: &Path) -> BTreeMap<String, String> {
    fn visit(root: &Path, path: &Path, result: &mut BTreeMap<String, String>) {
        let mut entries = fs::read_dir(path)
            .expect("snapshot directory")
            .map(|entry| entry.expect("snapshot entry").path())
            .collect::<Vec<_>>();
        entries.sort();
        for entry in entries {
            if entry.is_dir() {
                visit(root, &entry, result);
            } else {
                let relative = entry
                    .strip_prefix(root)
                    .expect("snapshot relative path")
                    .to_string_lossy()
                    .into_owned();
                result.insert(relative, digest(&fs::read(entry).expect("snapshot file")));
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

#[test]
fn complete_corpus_produces_deterministic_read_only_bounded_evidence() {
    let repository = copied_repository();
    let before = snapshot(&repository.0);
    let first = run_comparison(&repository.0, POLICY).expect("first comparison");
    let second = run_comparison(&repository.0, POLICY).expect("second comparison");
    assert_eq!(first, second);
    assert!(first.bounded_agreement);
    assert!(!first.global_parity_eligible);
    assert_eq!(first.expected_case_ids.len(), 26);
    assert_eq!(first.observed_legacy_case_ids, first.expected_case_ids);
    assert_eq!(first.observed_successor_case_ids, first.expected_case_ids);
    assert!(first.results.iter().all(|result| result.agreement));
    assert!(first.conformance.iter().all(|result| result.passed));
    assert_eq!(snapshot(&repository.0), before);

    let output_a = TestDirectory::new("output-a");
    let output_b = TestDirectory::new("output-b");
    let first_path = write_external_evidence(&repository.0, &output_a.0, &first)
        .expect("first external evidence");
    let second_path = write_external_evidence(&repository.0, &output_b.0, &second)
        .expect("second external evidence");
    assert_eq!(
        fs::read(first_path).expect("first bytes"),
        fs::read(second_path).expect("second bytes")
    );
    assert_eq!(
        write_external_evidence(&repository.0, &output_a.0, &first),
        Err(ComparisonError::OutputExists)
    );
    assert_eq!(
        write_external_evidence(&repository.0, &repository.0, &first),
        Err(ComparisonError::OutputConfined)
    );
    assert_eq!(snapshot(&repository.0), before);
}

#[test]
fn malformed_stale_and_drifted_inputs_fail_closed() {
    let stale = copied_repository();
    replace_once(
        &stale.0.join("docs/migration-source/audit/records/L1.yaml"),
        "row_id: L1",
        "row_id: L999",
    );
    assert_eq!(
        run_comparison(&stale.0, POLICY),
        Err(ComparisonError::IdentityMismatch)
    );

    let wrong_id = copied_repository();
    replace_once(
        &wrong_id
            .0
            .join("docs/migration-source/audit/records/L1.yaml"),
        "row_id: L1",
        "row_id: L999",
    );
    rebind_record(&wrong_id.0, "L1");
    assert_eq!(
        run_comparison(&wrong_id.0, POLICY),
        Err(ComparisonError::SemanticDifference)
    );

    let malformed = copied_repository();
    replace_once(
        &malformed.0.join(CATALOG),
        "catalog_id = \"audit-carried-rows-legacy-observed-v0\"",
        "catalog_id = [",
    );
    rebind_catalog(&malformed.0);
    assert_eq!(
        run_comparison(&malformed.0, POLICY),
        Err(ComparisonError::InvalidInput)
    );

    let ledger_drift = copied_repository();
    let ledger_path = ledger_drift
        .0
        .join("docs/migration-source-extraction-ledger.md");
    fs::write(&ledger_path, b"drifted legacy ledger\n").expect("ledger mutation");
    assert_eq!(
        run_comparison(&ledger_drift.0, POLICY),
        Err(ComparisonError::IdentityMismatch)
    );

    let artifact_set_drift = copied_repository();
    let policy_path = artifact_set_drift.0.join(POLICY);
    let policy: ComparisonPolicy =
        toml::from_str(&fs::read_to_string(&policy_path).expect("artifact-set policy"))
            .expect("parse artifact-set policy");
    replace_once(
        &policy_path,
        &policy.legacy_artifact_set_digest,
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    assert_eq!(
        run_comparison(&artifact_set_drift.0, POLICY),
        Err(ComparisonError::IdentityMismatch)
    );
}

#[test]
fn exact_case_set_rejects_empty_partial_extra_duplicate_and_reordering() {
    let empty = copied_repository();
    let catalog_path = empty.0.join(CATALOG);
    let text = fs::read_to_string(&catalog_path).expect("catalog");
    let cases_at = text.find("[[cases]]").expect("first case");
    fs::write(&catalog_path, &text[..cases_at]).expect("empty catalog");
    rebind_catalog(&empty.0);
    assert_eq!(
        run_comparison(&empty.0, POLICY),
        Err(ComparisonError::IncompleteCorpus)
    );

    let partial = copied_repository();
    let catalog_path = partial.0.join(CATALOG);
    let text = fs::read_to_string(&catalog_path).expect("catalog");
    let last_at = text.rfind("[[cases]]").expect("last case");
    fs::write(&catalog_path, &text[..last_at]).expect("partial catalog");
    rebind_catalog(&partial.0);
    assert_eq!(
        run_comparison(&partial.0, POLICY),
        Err(ComparisonError::IncompleteCorpus)
    );

    for label in ["extra", "duplicate"] {
        let repository = copied_repository();
        let catalog_path = repository.0.join(CATALOG);
        let mut text = fs::read_to_string(&catalog_path).expect("catalog");
        let last_at = text.rfind("[[cases]]").expect("last case");
        let mut last = text[last_at..].to_owned();
        if label == "extra" {
            last = last
                .replacen("case_id = \"L26\"", "case_id = \"L27\"", 1)
                .replacen("row_id = \"L26\"", "row_id = \"L27\"", 1);
        }
        text.push_str(&last);
        fs::write(&catalog_path, text).expect("append case");
        rebind_catalog(&repository.0);
        assert_eq!(
            run_comparison(&repository.0, POLICY),
            Err(ComparisonError::IncompleteCorpus),
            "{label}"
        );
    }

    let reordered = copied_repository();
    replace_once(
        &reordered.0.join(CATALOG),
        "case_id = \"L1\"",
        "case_id = \"L2\"",
    );
    rebind_catalog(&reordered.0);
    assert_eq!(
        run_comparison(&reordered.0, POLICY),
        Err(ComparisonError::IncompleteCorpus)
    );
}

#[test]
fn independent_normalizers_expose_side_specific_semantic_mutations() {
    let legacy = copied_repository();
    let record_path = legacy.0.join("docs/migration-source/audit/records/L1.yaml");
    replace_once(&record_path, "verdict: confirmed", "verdict: refuted");
    rebind_record(&legacy.0, "L1");
    let evidence = run_comparison(&legacy.0, POLICY).expect("legacy mutation comparison");
    assert!(!evidence.bounded_agreement);
    assert!(evidence.failures.contains(&"L1".to_owned()));
    assert!(evidence.failures.contains(&"capability-outcome".to_owned()));

    let successor = copied_repository();
    replace_first(
        &successor.0.join(CATALOG),
        "expected_verdict = \"confirmed\"",
        &format!("expected_verdict = \"{}\"", SUCCESSOR_VERDICT.trim()),
    );
    rebind_catalog(&successor.0);
    let evidence = run_comparison(&successor.0, POLICY).expect("successor mutation comparison");
    assert!(!evidence.bounded_agreement);
    assert!(evidence.failures.contains(&"L1".to_owned()));
}

#[test]
fn credential_path_source_and_verdict_edge_cases_are_not_normalized_away() {
    let credential = copied_repository();
    let record_path = credential
        .0
        .join("docs/migration-source/audit/records/L5.yaml");
    replace_once(&record_path, "rsp_cd 00136", LEGACY_SENTINEL.trim());
    rebind_record(&credential.0, "L5");
    let evidence = run_comparison(&credential.0, POLICY).expect("credential comparison");
    assert!(!evidence.bounded_agreement);
    assert!(!evidence.results[4].legacy_credential_rule);

    let path = copied_repository();
    replace_once(
        &path.0.join("docs/migration-source/audit/records/L1.yaml"),
        "evidence_pointer: docs/plans/maintained-sdk-migration-plan.md",
        "evidence_pointer: missing/evidence.md",
    );
    rebind_record(&path.0, "L1");
    let evidence = run_comparison(&path.0, POLICY).expect("path comparison");
    assert!(!evidence.bounded_agreement);
    assert!(!evidence.results[0].legacy_path_rule);

    let source_gap = copied_repository();
    replace_first(
        &source_gap.0.join(CATALOG),
        "source_available = true",
        "source_available = false",
    );
    rebind_catalog(&source_gap.0);
    let evidence = run_comparison(&source_gap.0, POLICY).expect("source gap comparison");
    assert!(!evidence.bounded_agreement);
    assert!(evidence.results[0].successor_blocking);

    for verdict in ["unverifiable", "assumption-accepted"] {
        let repository = copied_repository();
        replace_once(
            &repository
                .0
                .join("docs/migration-source/audit/records/L1.yaml"),
            "verdict: confirmed",
            &format!("verdict: {verdict}"),
        );
        rebind_record(&repository.0, "L1");
        if verdict == "unverifiable" {
            let evidence = run_comparison(&repository.0, POLICY).expect("unverifiable comparison");
            assert!(!evidence.bounded_agreement);
            assert!(evidence.failures.contains(&"L1".to_owned()));
        } else {
            assert_eq!(
                run_comparison(&repository.0, POLICY),
                Err(ComparisonError::InvalidInput)
            );
        }
    }
}

#[test]
fn irrelevant_legacy_text_changes_results_but_not_semantics() {
    let baseline = copied_repository();
    let baseline_evidence = run_comparison(&baseline.0, POLICY).expect("baseline");
    let annotated = copied_repository();
    let record_path = annotated
        .0
        .join("docs/migration-source/audit/records/L1.yaml");
    replace_once(
        &record_path,
        "# Decommission audit record",
        "# observed_at: 2026-08-18T12:17:00+09:00\n# temp_path: /ignored/comparison\n# Decommission audit record",
    );
    rebind_record(&annotated.0, "L1");
    let annotated_evidence = run_comparison(&annotated.0, POLICY).expect("annotated");
    assert!(annotated_evidence.bounded_agreement);
    assert_eq!(annotated_evidence.results, baseline_evidence.results);
    assert_eq!(
        annotated_evidence.conformance,
        baseline_evidence.conformance
    );
    assert_ne!(
        annotated_evidence.legacy_corpus_digest,
        baseline_evidence.legacy_corpus_digest
    );
    assert_ne!(
        annotated_evidence.deterministic_invocation_id,
        baseline_evidence.deterministic_invocation_id
    );
}
