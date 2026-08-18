//! Strict validation and create-new import for bounded offline evidence.

use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::identity::canonicalize_strict_json;
use crate::schema::{
    ArtifactReference, AuditVerdict, BoundedComparisonEvidence, CapabilityContract,
    LegacyClassification, RepositoryPath, SchemaVersion, Sha256Digest, StableId,
};

const MAX_EVIDENCE_BYTES: u64 = 4 * 1024 * 1024;
const POLICY_PATH: &str =
    ".repository-engineering/scenarios/audit-carried-rows/comparison-policy.toml";
const EVIDENCE_DIRECTORY: &str = ".repository-engineering/evidence/bounded/audit-carried-rows";
const COMPARED_DIMENSIONS: &[&str] = &[
    "row_coverage",
    "verdict",
    "row_completion",
    "capability_blocking",
    "roll_up",
    "credential_rule",
    "path_rule",
];
const SUCCESSOR_ONLY_DIMENSIONS: &[&str] = &[
    "durability",
    "cancellation",
    "correlation",
    "freshness",
    "confinement",
];
const CONFORMANCE_CASES: &[(&str, &str)] = &[
    ("durability", "deterministic-digest-bound-roll-up"),
    ("cancellation", "cancelled-row-remains-incomplete"),
    ("correlation", "wrong-assignment-fails-closed"),
    ("freshness", "child-attempt-uses-fresh-invocation"),
    ("confinement", "roll-up-target-is-confined"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportError {
    UnsafeInput,
    InputTooLarge,
    InvalidJson,
    InvalidPolicy,
    IdentityMismatch,
    IncompleteCorpus,
    SemanticMismatch,
    OutputUnsafe,
    OutputExists,
    Io,
}

impl fmt::Display for ImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = match self {
            Self::UnsafeInput => "bounded_evidence.input_unsafe",
            Self::InputTooLarge => "bounded_evidence.input_too_large",
            Self::InvalidJson => "bounded_evidence.invalid_json",
            Self::InvalidPolicy => "bounded_evidence.policy_invalid",
            Self::IdentityMismatch => "bounded_evidence.identity_mismatch",
            Self::IncompleteCorpus => "bounded_evidence.corpus_incomplete",
            Self::SemanticMismatch => "bounded_evidence.semantic_mismatch",
            Self::OutputUnsafe => "bounded_evidence.output_unsafe",
            Self::OutputExists => "bounded_evidence.output_exists",
            Self::Io => "bounded_evidence.io",
        };
        formatter.write_str(code)
    }
}

impl std::error::Error for ImportError {}

#[derive(Debug, Deserialize)]
struct PolicyBinding {
    schema_version: String,
    policy_id: String,
    wave1_package_lock_id: Sha256Digest,
    implementation_subject: ArtifactReference,
    capability_contract: ArtifactReference,
    worker_role_contract: ArtifactReference,
    executor: ArtifactReference,
    successor_scenario: ArtifactReference,
    migration_source_manifest: ArtifactReference,
    legacy_ledger: ArtifactReference,
    legacy_report: ArtifactReference,
    legacy_oracle: ArtifactReference,
    legacy_artifact_set_digest: Sha256Digest,
    case_catalog: ArtifactReference,
    successor_conformance_basis: Vec<ArtifactReference>,
    expected_case_ids: Vec<crate::schema::StableId>,
    compared_dimensions: Vec<crate::schema::StableId>,
    successor_only_dimensions: Vec<crate::schema::StableId>,
    exclusions: Vec<String>,
    global_parity_eligible: bool,
}

#[derive(Debug, Deserialize)]
struct CaseCatalogBinding {
    schema_version: String,
    catalog_id: String,
    cases: Vec<CaseBinding>,
}

#[derive(Debug, Deserialize)]
struct CaseBinding {
    case_id: StableId,
    row_id: StableId,
    legacy_record: ArtifactReference,
    expected_classification: LegacyClassification,
    expected_verdict: AuditVerdict,
}

pub fn import_bounded_evidence(
    repository_root: &Path,
    external_path: &Path,
) -> Result<ArtifactReference, ImportError> {
    let root = repository_root
        .canonicalize()
        .map_err(|_| ImportError::Io)?;
    let metadata = fs::symlink_metadata(external_path).map_err(|_| ImportError::Io)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ImportError::UnsafeInput);
    }
    if metadata.len() > MAX_EVIDENCE_BYTES {
        return Err(ImportError::InputTooLarge);
    }
    let external = external_path.canonicalize().map_err(|_| ImportError::Io)?;
    if external.starts_with(&root) {
        return Err(ImportError::UnsafeInput);
    }
    let input = fs::read(&external).map_err(|_| ImportError::Io)?;
    let text = std::str::from_utf8(&input).map_err(|_| ImportError::InvalidJson)?;
    let canonical = canonicalize_strict_json(text).map_err(|_| ImportError::InvalidJson)?;
    let evidence: BoundedComparisonEvidence =
        serde_json::from_slice(&canonical).map_err(|_| ImportError::InvalidJson)?;
    validate_bounded_evidence(&root, &evidence)?;

    let digest = Sha256Digest(format!("sha256:{:x}", Sha256::digest(&canonical)));
    let directory = create_confined_directory(&root)?;
    let output = directory.join(format!("{}.json", &digest.0[7..]));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                ImportError::OutputExists
            } else {
                ImportError::Io
            }
        })?;
    file.write_all(&canonical).map_err(|_| ImportError::Io)?;
    file.sync_all().map_err(|_| ImportError::Io)?;
    Ok(ArtifactReference {
        schema_version: SchemaVersion::V0,
        path: RepositoryPath(format!("{EVIDENCE_DIRECTORY}/{}.json", &digest.0[7..])),
        sha256: digest,
        media_type: "application/json".to_owned(),
    })
}

pub fn validate_bounded_evidence(
    root: &Path,
    evidence: &BoundedComparisonEvidence,
) -> Result<(), ImportError> {
    let policy_bytes = read_confined(root, POLICY_PATH)?;
    let policy: PolicyBinding =
        toml::from_str(std::str::from_utf8(&policy_bytes).map_err(|_| ImportError::InvalidPolicy)?)
            .map_err(|_| ImportError::InvalidPolicy)?;
    let policy_reference = ArtifactReference {
        schema_version: SchemaVersion::V0,
        path: RepositoryPath(POLICY_PATH.to_owned()),
        sha256: Sha256Digest(format!("sha256:{:x}", Sha256::digest(&policy_bytes))),
        media_type: "application/toml".to_owned(),
    };
    if policy.schema_version != "v0"
        || policy.policy_id != "audit-carried-rows-bounded-v0"
        || policy.global_parity_eligible
        || evidence.evidence_kind.0 != "bounded_offline_comparison"
        || evidence.evidence_id.0 != "audit-carried-rows-bounded-v0"
        || evidence.comparator_policy != policy_reference
        || evidence.wave1_package_lock_id != policy.wave1_package_lock_id
        || evidence.implementation_subject != policy.implementation_subject
        || evidence.capability_contract != policy.capability_contract
        || evidence.worker_role_contract != policy.worker_role_contract
        || evidence.executor != policy.executor
        || evidence.successor_scenario != policy.successor_scenario
        || evidence.migration_source_manifest != policy.migration_source_manifest
        || evidence.legacy_ledger != policy.legacy_ledger
        || evidence.legacy_report != policy.legacy_report
        || evidence.legacy_oracle != policy.legacy_oracle
        || evidence.legacy_artifact_set_digest != policy.legacy_artifact_set_digest
        || evidence.case_catalog != policy.case_catalog
        || evidence.successor_conformance_basis != policy.successor_conformance_basis
        || evidence.compared_dimensions != policy.compared_dimensions
        || evidence.successor_only_dimensions != policy.successor_only_dimensions
        || evidence.exclusions != policy.exclusions
        || evidence.global_parity_eligible
    {
        return Err(ImportError::IdentityMismatch);
    }
    for reference in [
        &policy.implementation_subject,
        &policy.executor,
        &policy.successor_scenario,
        &policy.migration_source_manifest,
        &policy.legacy_ledger,
        &policy.legacy_report,
        &policy.legacy_oracle,
        &policy.case_catalog,
    ]
    .into_iter()
    .chain(policy.successor_conformance_basis.iter())
    {
        validate_reference(root, reference)?;
    }
    let catalog_bytes = read_confined(root, &policy.case_catalog.path.0)?;
    let catalog: CaseCatalogBinding = toml::from_str(
        std::str::from_utf8(&catalog_bytes).map_err(|_| ImportError::InvalidPolicy)?,
    )
    .map_err(|_| ImportError::InvalidPolicy)?;
    let legacy_corpus_digest = legacy_corpus_digest(root, &catalog.cases)?;
    validate_legacy_artifact_binding(root, &policy)?;
    validate_semantics(evidence, &policy, &catalog, &legacy_corpus_digest)
}

fn validate_semantics(
    evidence: &BoundedComparisonEvidence,
    policy: &PolicyBinding,
    catalog: &CaseCatalogBinding,
    legacy_corpus_digest: &Sha256Digest,
) -> Result<(), ImportError> {
    if policy.expected_case_ids.is_empty()
        || policy.exclusions.is_empty()
        || catalog.schema_version != "v0"
        || catalog.catalog_id != "audit-carried-rows-legacy-observed-v0"
        || evidence.expected_case_ids != policy.expected_case_ids
        || evidence.observed_legacy_case_ids != policy.expected_case_ids
        || evidence.observed_successor_case_ids != policy.expected_case_ids
        || evidence.results.len() != policy.expected_case_ids.len()
        || catalog.cases.len() != policy.expected_case_ids.len()
    {
        return Err(ImportError::IncompleteCorpus);
    }
    if evidence.legacy_corpus_digest != *legacy_corpus_digest {
        return Err(ImportError::IdentityMismatch);
    }
    let unique = policy.expected_case_ids.iter().collect::<BTreeSet<_>>();
    if unique.len() != policy.expected_case_ids.len() {
        return Err(ImportError::IncompleteCorpus);
    }
    for ((expected, result), case) in policy
        .expected_case_ids
        .iter()
        .zip(&evidence.results)
        .zip(&catalog.cases)
    {
        if &result.case_id != expected
            || &result.row_id != expected
            || &case.case_id != expected
            || &case.row_id != expected
            || result.classification != case.expected_classification
            || result.successor_verdict != case.expected_verdict
            || !result.agreement
            || result.legacy_verdict != result.successor_verdict
            || result.legacy_completed != result.successor_completed
            || !result.legacy_completed
            || result.legacy_blocking != result.successor_blocking
            || result.legacy_roll_up != result.successor_roll_up
            || result.legacy_credential_rule != result.successor_credential_rule
            || result.legacy_path_rule != result.successor_path_rule
            || !result.legacy_credential_rule
            || !result.successor_credential_rule
            || !result.legacy_path_rule
            || !result.successor_path_rule
        {
            return Err(ImportError::SemanticMismatch);
        }
    }
    let compared = evidence
        .compared_dimensions
        .iter()
        .map(|dimension| dimension.0.as_str())
        .collect::<Vec<_>>();
    let successor_only = evidence
        .successor_only_dimensions
        .iter()
        .map(|dimension| dimension.0.as_str())
        .collect::<Vec<_>>();
    let conformance = evidence
        .conformance
        .iter()
        .map(|result| {
            (
                result.dimension.0.as_str(),
                result.case_id.0.as_str(),
                result.passed,
            )
        })
        .collect::<Vec<_>>();
    let invocation_material = serde_json::to_vec(&(
        &evidence.comparator_policy,
        &evidence.case_catalog,
        &evidence.wave1_package_lock_id,
        &evidence.implementation_subject,
        &evidence.legacy_corpus_digest,
        &evidence.results,
        &evidence.conformance,
    ))
    .map_err(|_| ImportError::SemanticMismatch)?;
    let expected_invocation_id = format!(
        "bounded-audit-comparison-{:x}",
        Sha256::digest(invocation_material)
    );
    if compared != COMPARED_DIMENSIONS
        || successor_only != SUCCESSOR_ONLY_DIMENSIONS
        || conformance
            != CONFORMANCE_CASES
                .iter()
                .map(|(dimension, case_id)| (*dimension, *case_id, true))
                .collect::<Vec<_>>()
        || !evidence.failures.is_empty()
        || !evidence.cancellations.is_empty()
        || !evidence.bounded_agreement
        || evidence.adapter_facts.configured_global_limit != 8
        || evidence.adapter_facts.effective_global_limit != 2
        || evidence.adapter_facts.successor_host.0 != "none-pure-state-machine"
        || evidence.adapter_facts.legacy_normalizer.0 != "independent-frozen-yaml-projection-v0"
        || evidence.adapter_facts.successor_normalizer.0 != "sweep-machine-v0"
        || evidence.adapter_facts.output_mode.0 != "caller-owned-external-create-new"
        || !evidence
            .deterministic_invocation_id
            .0
            .starts_with("bounded-audit-comparison-")
        || evidence.deterministic_invocation_id.0 != expected_invocation_id
    {
        return Err(ImportError::SemanticMismatch);
    }
    Ok(())
}

fn legacy_corpus_digest(root: &Path, cases: &[CaseBinding]) -> Result<Sha256Digest, ImportError> {
    let mut hasher = Sha256::new();
    hasher.update(b"audit-bounded-comparison/corpus/v0\0");
    hasher.update((cases.len() as u64).to_be_bytes());
    for case in cases {
        let bytes = read_confined(root, &case.legacy_record.path.0)?;
        let actual = Sha256Digest(format!("sha256:{:x}", Sha256::digest(&bytes)));
        if actual != case.legacy_record.sha256 {
            return Err(ImportError::IdentityMismatch);
        }
        hasher.update(case.legacy_record.path.0.as_bytes());
        hasher.update([0]);
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(&bytes);
        hasher.update([0]);
    }
    Ok(Sha256Digest(format!("sha256:{:x}", hasher.finalize())))
}

fn validate_legacy_artifact_binding(
    root: &Path,
    policy: &PolicyBinding,
) -> Result<(), ImportError> {
    let bytes = read_confined(root, &policy.capability_contract.path.0)?;
    let contract: CapabilityContract =
        toml::from_str(std::str::from_utf8(&bytes).map_err(|_| ImportError::InvalidPolicy)?)
            .map_err(|_| ImportError::InvalidPolicy)?;
    if !contract.evidence_status.as_ref().is_some_and(|status| {
        status
            .legacy_artifact_sets
            .iter()
            .any(|set| set.aggregate_digest == policy.legacy_artifact_set_digest)
    }) {
        return Err(ImportError::IdentityMismatch);
    }
    Ok(())
}

fn validate_reference(root: &Path, reference: &ArtifactReference) -> Result<(), ImportError> {
    let bytes = read_confined(root, &reference.path.0)?;
    let actual = Sha256Digest(format!("sha256:{:x}", Sha256::digest(bytes)));
    if actual != reference.sha256 {
        return Err(ImportError::IdentityMismatch);
    }
    Ok(())
}

fn read_confined(root: &Path, relative: &str) -> Result<Vec<u8>, ImportError> {
    let candidate = root.join(relative);
    let metadata = fs::symlink_metadata(&candidate).map_err(|_| ImportError::Io)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_EVIDENCE_BYTES
    {
        return Err(ImportError::UnsafeInput);
    }
    let canonical = candidate.canonicalize().map_err(|_| ImportError::Io)?;
    if !canonical.starts_with(root) {
        return Err(ImportError::UnsafeInput);
    }
    fs::read(canonical).map_err(|_| ImportError::Io)
}

fn create_confined_directory(root: &Path) -> Result<PathBuf, ImportError> {
    let mut current = root.to_path_buf();
    for component in EVIDENCE_DIRECTORY.split('/') {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(ImportError::OutputUnsafe);
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|_| ImportError::Io)?;
            }
            Err(_) => return Err(ImportError::Io),
        }
    }
    let canonical = current.canonicalize().map_err(|_| ImportError::Io)?;
    if !canonical.starts_with(root) {
        return Err(ImportError::OutputUnsafe);
    }
    Ok(canonical)
}
