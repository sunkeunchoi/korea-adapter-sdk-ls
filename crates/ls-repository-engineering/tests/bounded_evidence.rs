use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use ls_repository_engineering::bounded_evidence::{
    import_bounded_evidence, validate_bounded_evidence, ImportError,
};
use ls_repository_engineering::identity::canonicalize_strict_json;
use ls_repository_engineering::schema::BoundedComparisonEvidence;
use sha2::{Digest, Sha256};

const EVIDENCE: &str = ".repository-engineering/evidence/bounded/audit-carried-rows/44883d95756f89e196caf39f17dda969a67cd5b0e2a79a8ed57284112316ff9c.json";
const POLICY: &str = ".repository-engineering/scenarios/audit-carried-rows/comparison-policy.toml";

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "ls-repository-engineering-bounded-{label}-{}-{}",
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

fn copy_file(source: &Path, target: &Path, relative: &str) {
    let destination = target.join(relative);
    fs::create_dir_all(destination.parent().expect("copy parent")).expect("create copy parent");
    fs::copy(source.join(relative), destination).expect("copy bounded input");
}

fn validation_root() -> TestDirectory {
    let source = repository_root();
    let target = TestDirectory::new("root");
    copy_file(&source, &target.0, POLICY);
    let bytes = fs::read(source.join(EVIDENCE)).expect("bounded evidence");
    let evidence: BoundedComparisonEvidence =
        serde_json::from_slice(&bytes).expect("typed bounded evidence");
    for reference in [
        &evidence.implementation_subject,
        &evidence.capability_contract,
        &evidence.executor,
        &evidence.successor_scenario,
        &evidence.migration_source_manifest,
        &evidence.legacy_ledger,
        &evidence.legacy_report,
        &evidence.legacy_oracle,
        &evidence.case_catalog,
    ]
    .into_iter()
    .chain(evidence.successor_conformance_basis.iter())
    {
        copy_file(&source, &target.0, &reference.path.0);
    }
    let catalog: toml::Value = toml::from_str(
        &fs::read_to_string(source.join(&evidence.case_catalog.path.0))
            .expect("comparison catalog"),
    )
    .expect("parse comparison catalog");
    for case in catalog["cases"].as_array().expect("catalog cases") {
        let record_path = case["legacy_record"]["path"]
            .as_str()
            .expect("legacy record path");
        copy_file(&source, &target.0, record_path);
    }
    target
}

#[test]
fn committed_payload_is_strict_complete_and_lifecycle_neutral() {
    let root = repository_root();
    let bytes = fs::read(root.join(EVIDENCE)).expect("bounded evidence");
    let evidence: BoundedComparisonEvidence =
        serde_json::from_slice(&bytes).expect("typed bounded evidence");
    validate_bounded_evidence(&root, &evidence).expect("valid bounded evidence");
    assert_eq!(
        canonicalize_strict_json(std::str::from_utf8(&bytes).expect("UTF-8 evidence"))
            .expect("canonical bounded evidence"),
        bytes
    );
    assert_eq!(evidence.expected_case_ids.len(), 26);
    assert!(evidence.bounded_agreement);
    assert!(!evidence.global_parity_eligible);
    assert!(evidence.failures.is_empty());
    assert!(evidence.cancellations.is_empty());
}

#[test]
fn wave1_subject_and_deferred_lifecycle_inputs_are_byte_identical() {
    let root = repository_root();
    for (relative, expected) in [
        (
            ".repository-engineering/implementation-subjects/audit-carried-rows.json",
            "3359cdc5805dca3eeafff939652c8173be52557a0b509619e19e69ac103d0c19",
        ),
        (
            ".repository-engineering/package.toml",
            "11b7bbc0da007eb76e0bb14d41b07d11eb289930475aa8858d53cf38af98c448",
        ),
        (
            ".repository-engineering/migration-ledger.toml",
            "539b8e988bd2d6a6fadeff59acd98ab07a4b40d7378c41bfffb92117ced7314a",
        ),
        (
            "docs/migration-source-extraction-ledger.md",
            "f3ed6793d3ca6c7bf6e33bd761b7c0b7050e7d9f256a246ad906e59a49d80f1f",
        ),
    ] {
        let actual = format!(
            "{:x}",
            Sha256::digest(fs::read(root.join(relative)).expect("lifecycle input"))
        );
        assert_eq!(actual, expected, "{relative}");
    }
    let authored = ls_repository_engineering::inventory::load_authored_package(&root)
        .expect("authored package");
    assert!(authored.package.active_capability_contracts.is_empty());
    assert!(authored.package.active_worker_roles.is_empty());
    assert_eq!(
        authored.package.activation_eligibility,
        ls_repository_engineering::schema::ActivationEligibility::None
    );
    assert!(authored.ledger.rows.iter().all(|row| {
        row.current_authority == ls_repository_engineering::schema::AuthorityState::Legacy
            && row.parity_reference.is_none()
    }));
}

#[test]
fn importer_is_canonical_create_new_and_rejects_repository_inputs() {
    let source = repository_root();
    let root = validation_root();
    let reference = import_bounded_evidence(&root.0, &source.join(EVIDENCE))
        .expect("import external bounded evidence");
    assert_eq!(
        reference.path.0,
        ".repository-engineering/evidence/bounded/audit-carried-rows/44883d95756f89e196caf39f17dda969a67cd5b0e2a79a8ed57284112316ff9c.json"
    );
    assert_eq!(
        fs::read(root.0.join(&reference.path.0)).expect("imported evidence"),
        fs::read(source.join(EVIDENCE)).expect("source evidence")
    );
    assert_eq!(
        import_bounded_evidence(&root.0, &source.join(EVIDENCE)),
        Err(ImportError::OutputExists)
    );
    assert_eq!(
        import_bounded_evidence(&source, &source.join(EVIDENCE)),
        Err(ImportError::UnsafeInput)
    );
}

#[test]
fn semantic_or_identity_mutation_never_imports() {
    let source = repository_root();
    let root = validation_root();
    let mutation = TestDirectory::new("mutation");
    let original = fs::read_to_string(source.join(EVIDENCE)).expect("bounded evidence");

    let semantic = mutation.0.join("semantic.json");
    fs::write(
        &semantic,
        original.replacen(
            "\"bounded_agreement\":true",
            "\"bounded_agreement\":false",
            1,
        ),
    )
    .expect("semantic mutation");
    assert_eq!(
        import_bounded_evidence(&root.0, &semantic),
        Err(ImportError::SemanticMismatch)
    );

    let identity = mutation.0.join("identity.json");
    fs::write(
        &identity,
        original.replacen(
            "sha256:82f8661328e0551c328114f4b7f3e5669fcccb805535ea7fddb3c27c516e5f7c",
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            1,
        ),
    )
    .expect("identity mutation");
    assert_eq!(
        import_bounded_evidence(&root.0, &identity),
        Err(ImportError::IdentityMismatch)
    );

    for (label, old, new, expected_error) in [
        (
            "credential-rule",
            "\"legacy_credential_rule\":true",
            "\"legacy_credential_rule\":false",
            ImportError::SemanticMismatch,
        ),
        (
            "invocation",
            "bounded-audit-comparison-eb8121c0b90c3e2c43ec06261c1b66072eeced4811b5d2ea3708e00c0230eb6c",
            "bounded-audit-comparison-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ImportError::SemanticMismatch,
        ),
        (
            "corpus",
            "sha256:1bc32015e176f274a4fb898d7a341c5a15bd6b9d9de27825a6f6c05e6087b93c",
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            ImportError::IdentityMismatch,
        ),
    ] {
        let path = mutation.0.join(format!("{label}.json"));
        fs::write(&path, original.replacen(old, new, 1)).expect("bounded mutation");
        assert_eq!(
            import_bounded_evidence(&root.0, &path),
            Err(expected_error),
            "{label}"
        );
    }
}
