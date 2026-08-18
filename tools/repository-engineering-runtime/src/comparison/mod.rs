mod legacy;
mod successor;

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::model::{ArtifactReference, AuditVerdict, TerminalOutcome};

const MAX_INPUT_BYTES: u64 = 4 * 1024 * 1024;
const OUTPUT_NAME: &str = "audit-carried-rows-bounded-v0.json";
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonError {
    InvalidInput,
    IdentityMismatch,
    IncompleteCorpus,
    SemanticDifference,
    OutputConfined,
    OutputExists,
    Io,
}

impl std::fmt::Display for ComparisonError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ComparisonError {}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonPolicy {
    pub schema_version: String,
    pub policy_id: String,
    pub wave1_package_lock_id: String,
    pub implementation_subject: ArtifactReference,
    pub capability_contract: ArtifactReference,
    pub worker_role_contract: ArtifactReference,
    pub executor: ArtifactReference,
    pub successor_scenario: ArtifactReference,
    pub migration_source_manifest: ArtifactReference,
    pub legacy_ledger: ArtifactReference,
    pub legacy_report: ArtifactReference,
    pub legacy_oracle: ArtifactReference,
    pub legacy_artifact_set_digest: String,
    pub case_catalog: ArtifactReference,
    pub successor_conformance_basis: Vec<ArtifactReference>,
    pub expected_case_ids: Vec<String>,
    pub compared_dimensions: Vec<String>,
    pub successor_only_dimensions: Vec<String>,
    pub exclusions: Vec<String>,
    pub global_parity_eligible: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonCatalog {
    pub schema_version: String,
    pub catalog_id: String,
    #[serde(default)]
    pub cases: Vec<ComparisonCase>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonCase {
    pub case_id: String,
    pub row_id: String,
    pub legacy_record: ArtifactReference,
    pub expected_classification: String,
    pub expected_verdict: AuditVerdict,
    pub expected_disposition: String,
    pub source_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedCaseResult {
    pub case_id: String,
    pub row_id: String,
    pub classification: String,
    pub legacy_verdict: AuditVerdict,
    pub successor_verdict: AuditVerdict,
    pub legacy_completed: bool,
    pub successor_completed: bool,
    pub legacy_blocking: bool,
    pub successor_blocking: bool,
    pub legacy_roll_up: String,
    pub successor_roll_up: String,
    pub legacy_credential_rule: bool,
    pub successor_credential_rule: bool,
    pub legacy_path_rule: bool,
    pub successor_path_rule: bool,
    pub agreement: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConformanceResult {
    pub dimension: String,
    pub case_id: String,
    pub passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterFacts {
    pub legacy_normalizer: String,
    pub successor_normalizer: String,
    pub successor_host: String,
    pub configured_global_limit: usize,
    pub effective_global_limit: usize,
    pub output_mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundedComparisonEvidence {
    pub schema_version: String,
    pub evidence_kind: String,
    pub evidence_id: String,
    pub comparator_policy: ArtifactReference,
    pub case_catalog: ArtifactReference,
    pub deterministic_invocation_id: String,
    pub wave1_package_lock_id: String,
    pub implementation_subject: ArtifactReference,
    pub capability_contract: ArtifactReference,
    pub worker_role_contract: ArtifactReference,
    pub executor: ArtifactReference,
    pub successor_scenario: ArtifactReference,
    pub migration_source_manifest: ArtifactReference,
    pub legacy_ledger: ArtifactReference,
    pub legacy_report: ArtifactReference,
    pub legacy_oracle: ArtifactReference,
    pub legacy_artifact_set_digest: String,
    pub legacy_corpus_digest: String,
    pub successor_conformance_basis: Vec<ArtifactReference>,
    pub adapter_facts: AdapterFacts,
    pub expected_case_ids: Vec<String>,
    pub observed_legacy_case_ids: Vec<String>,
    pub observed_successor_case_ids: Vec<String>,
    pub results: Vec<NormalizedCaseResult>,
    pub conformance: Vec<ConformanceResult>,
    pub compared_dimensions: Vec<String>,
    pub successor_only_dimensions: Vec<String>,
    pub exclusions: Vec<String>,
    pub failures: Vec<String>,
    pub cancellations: Vec<String>,
    pub bounded_agreement: bool,
    pub global_parity_eligible: bool,
}

pub fn run_comparison(
    repository_root: &Path,
    policy_relative: &str,
) -> Result<BoundedComparisonEvidence, ComparisonError> {
    let repository_root = repository_root
        .canonicalize()
        .map_err(|_| ComparisonError::Io)?;
    let policy_path = confined_existing(&repository_root, policy_relative)?;
    let policy_bytes = read_bounded(&policy_path)?;
    let policy: ComparisonPolicy = parse_toml(&policy_bytes)?;
    validate_policy(&repository_root, &policy)?;

    let catalog_path = confined_existing(&repository_root, &policy.case_catalog.path)?;
    let catalog_bytes = read_bounded(&catalog_path)?;
    verify_digest(&catalog_bytes, &policy.case_catalog.sha256)?;
    let catalog: ComparisonCatalog = parse_toml(&catalog_bytes)?;
    validate_catalog(&policy, &catalog)?;

    let legacy = legacy::normalize(&repository_root, &catalog.cases)?;
    let successor = successor::normalize(&policy, &catalog.cases)?;
    let legacy_ids = legacy
        .rows
        .iter()
        .map(|row| row.case_id.clone())
        .collect::<Vec<_>>();
    let successor_ids = successor
        .rows
        .iter()
        .map(|row| row.case_id.clone())
        .collect::<Vec<_>>();
    if legacy_ids != policy.expected_case_ids || successor_ids != policy.expected_case_ids {
        return Err(ComparisonError::IncompleteCorpus);
    }

    let mut failures = Vec::new();
    let mut results = Vec::with_capacity(catalog.cases.len());
    for ((case, legacy), successor) in catalog.cases.iter().zip(&legacy.rows).zip(&successor.rows) {
        let agreement = legacy.verdict == successor.verdict
            && legacy.completed == successor.completed
            && legacy.blocking == successor.blocking
            && legacy.roll_up == successor.roll_up
            && legacy.credential_rule == successor.credential_rule
            && legacy.path_rule == successor.path_rule;
        if !agreement {
            failures.push(case.case_id.clone());
        }
        results.push(NormalizedCaseResult {
            case_id: case.case_id.clone(),
            row_id: case.row_id.clone(),
            classification: legacy.classification.clone(),
            legacy_verdict: legacy.verdict,
            successor_verdict: successor.verdict,
            legacy_completed: legacy.completed,
            successor_completed: successor.completed,
            legacy_blocking: legacy.blocking,
            successor_blocking: successor.blocking,
            legacy_roll_up: legacy.roll_up.clone(),
            successor_roll_up: successor.roll_up.clone(),
            legacy_credential_rule: legacy.credential_rule,
            successor_credential_rule: successor.credential_rule,
            legacy_path_rule: legacy.path_rule,
            successor_path_rule: successor.path_rule,
            agreement,
        });
    }
    let expected_outcome = if legacy.rows.iter().any(|row| row.blocking) {
        TerminalOutcome::Held
    } else {
        TerminalOutcome::Succeeded
    };
    if successor.outcome != expected_outcome {
        failures.push("capability-outcome".to_owned());
    }

    let conformance = successor::conformance(&policy)?;
    failures.extend(
        conformance
            .iter()
            .filter(|result| !result.passed)
            .map(|result| result.case_id.clone()),
    );
    failures.sort();
    failures.dedup();

    let policy_reference = ArtifactReference {
        schema_version: "v0".to_owned(),
        path: policy_relative.to_owned(),
        sha256: digest_bytes(&policy_bytes),
        media_type: "application/toml".to_owned(),
    };
    let legacy_corpus_digest = legacy::corpus_digest(&legacy.artifacts);
    let invocation_material = serde_json::to_vec(&(
        &policy_reference,
        &policy.case_catalog,
        &policy.wave1_package_lock_id,
        &policy.implementation_subject,
        &legacy_corpus_digest,
        &results,
        &conformance,
    ))
    .map_err(|_| ComparisonError::InvalidInput)?;
    let deterministic_invocation_id = format!(
        "bounded-audit-comparison-{:x}",
        Sha256::digest(invocation_material)
    );
    let bounded_agreement = failures.is_empty();

    Ok(BoundedComparisonEvidence {
        schema_version: "v0".to_owned(),
        evidence_kind: "bounded_offline_comparison".to_owned(),
        evidence_id: "audit-carried-rows-bounded-v0".to_owned(),
        comparator_policy: policy_reference,
        case_catalog: policy.case_catalog.clone(),
        deterministic_invocation_id,
        wave1_package_lock_id: policy.wave1_package_lock_id.clone(),
        implementation_subject: policy.implementation_subject.clone(),
        capability_contract: policy.capability_contract.clone(),
        worker_role_contract: policy.worker_role_contract.clone(),
        executor: policy.executor.clone(),
        successor_scenario: policy.successor_scenario.clone(),
        migration_source_manifest: policy.migration_source_manifest.clone(),
        legacy_ledger: policy.legacy_ledger.clone(),
        legacy_report: policy.legacy_report.clone(),
        legacy_oracle: policy.legacy_oracle.clone(),
        legacy_artifact_set_digest: policy.legacy_artifact_set_digest.clone(),
        legacy_corpus_digest,
        successor_conformance_basis: policy.successor_conformance_basis.clone(),
        adapter_facts: AdapterFacts {
            legacy_normalizer: "independent-frozen-yaml-projection-v0".to_owned(),
            successor_normalizer: "sweep-machine-v0".to_owned(),
            successor_host: "none-pure-state-machine".to_owned(),
            configured_global_limit: 8,
            effective_global_limit: 2,
            output_mode: "caller-owned-external-create-new".to_owned(),
        },
        expected_case_ids: policy.expected_case_ids.clone(),
        observed_legacy_case_ids: legacy_ids,
        observed_successor_case_ids: successor_ids,
        results,
        conformance,
        compared_dimensions: policy.compared_dimensions.clone(),
        successor_only_dimensions: policy.successor_only_dimensions.clone(),
        exclusions: policy.exclusions.clone(),
        failures,
        cancellations: Vec::new(),
        bounded_agreement,
        global_parity_eligible: false,
    })
}

pub fn write_external_evidence(
    repository_root: &Path,
    output_root: &Path,
    evidence: &BoundedComparisonEvidence,
) -> Result<PathBuf, ComparisonError> {
    let repository_root = repository_root
        .canonicalize()
        .map_err(|_| ComparisonError::Io)?;
    let output_root = output_root
        .canonicalize()
        .map_err(|_| ComparisonError::Io)?;
    if output_root.starts_with(&repository_root) {
        return Err(ComparisonError::OutputConfined);
    }
    let output = output_root.join(OUTPUT_NAME);
    let bytes = serde_json::to_vec_pretty(evidence).map_err(|_| ComparisonError::InvalidInput)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                ComparisonError::OutputExists
            } else {
                ComparisonError::Io
            }
        })?;
    file.write_all(&bytes).map_err(|_| ComparisonError::Io)?;
    file.write_all(b"\n").map_err(|_| ComparisonError::Io)?;
    file.sync_all().map_err(|_| ComparisonError::Io)?;
    Ok(output)
}

fn validate_policy(root: &Path, policy: &ComparisonPolicy) -> Result<(), ComparisonError> {
    if policy.schema_version != "v0"
        || policy.policy_id != "audit-carried-rows-bounded-v0"
        || policy.expected_case_ids.is_empty()
        || policy.global_parity_eligible
        || policy.exclusions.is_empty()
        || policy
            .compared_dimensions
            .iter()
            .map(String::as_str)
            .ne(COMPARED_DIMENSIONS.iter().copied())
        || policy
            .successor_only_dimensions
            .iter()
            .map(String::as_str)
            .ne(SUCCESSOR_ONLY_DIMENSIONS.iter().copied())
        || !valid_digest(&policy.wave1_package_lock_id)
        || !valid_digest(&policy.legacy_artifact_set_digest)
    {
        return Err(ComparisonError::InvalidInput);
    }
    let unique = policy.expected_case_ids.iter().collect::<BTreeSet<_>>();
    if unique.len() != policy.expected_case_ids.len() {
        return Err(ComparisonError::IncompleteCorpus);
    }
    for reference in [
        &policy.implementation_subject,
        &policy.executor,
        &policy.successor_scenario,
        &policy.migration_source_manifest,
        &policy.legacy_ledger,
        &policy.legacy_report,
        &policy.legacy_oracle,
    ]
    .into_iter()
    .chain(policy.successor_conformance_basis.iter())
    {
        validate_reference(root, reference)?;
    }
    validate_legacy_artifact_binding(root, policy)?;
    Ok(())
}

fn validate_legacy_artifact_binding(
    root: &Path,
    policy: &ComparisonPolicy,
) -> Result<(), ComparisonError> {
    let path = confined_existing(root, &policy.capability_contract.path)?;
    let bytes = read_bounded(&path)?;
    let contract: toml::Value = parse_toml(&bytes)?;
    let artifact_sets = contract
        .get("evidence_status")
        .and_then(|status| status.get("legacy_artifact_sets"))
        .and_then(toml::Value::as_array)
        .ok_or(ComparisonError::InvalidInput)?;
    if !artifact_sets.iter().any(|artifact_set| {
        artifact_set
            .get("aggregate_digest")
            .and_then(toml::Value::as_str)
            == Some(policy.legacy_artifact_set_digest.as_str())
    }) {
        return Err(ComparisonError::IdentityMismatch);
    }
    Ok(())
}

fn validate_catalog(
    policy: &ComparisonPolicy,
    catalog: &ComparisonCatalog,
) -> Result<(), ComparisonError> {
    if catalog.schema_version != "v0"
        || catalog.catalog_id != "audit-carried-rows-legacy-observed-v0"
        || catalog.cases.is_empty()
        || catalog.cases.len() != policy.expected_case_ids.len()
    {
        return Err(ComparisonError::IncompleteCorpus);
    }
    let ids = catalog
        .cases
        .iter()
        .map(|case| case.case_id.clone())
        .collect::<Vec<_>>();
    if ids != policy.expected_case_ids {
        return Err(ComparisonError::IncompleteCorpus);
    }
    let mut rows = BTreeSet::new();
    for case in &catalog.cases {
        if case.case_id != case.row_id
            || !rows.insert(case.row_id.clone())
            || !matches!(
                case.expected_classification.as_str(),
                "behavioral" | "knowledge" | "discard"
            )
            || !matches!(case.expected_disposition.as_str(), "carried" | "discard")
        {
            return Err(ComparisonError::InvalidInput);
        }
    }
    Ok(())
}

fn validate_reference(root: &Path, reference: &ArtifactReference) -> Result<(), ComparisonError> {
    if reference.schema_version != "v0"
        || reference.media_type.is_empty()
        || !valid_digest(&reference.sha256)
    {
        return Err(ComparisonError::InvalidInput);
    }
    let path = confined_existing(root, &reference.path)?;
    verify_digest(&read_bounded(&path)?, &reference.sha256)
}

fn confined_existing(root: &Path, relative: &str) -> Result<PathBuf, ComparisonError> {
    if relative.is_empty()
        || relative.starts_with('/')
        || relative.contains('\\')
        || relative
            .split('/')
            .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
    {
        return Err(ComparisonError::InvalidInput);
    }
    let candidate = root.join(relative);
    let metadata = fs::symlink_metadata(&candidate).map_err(|_| ComparisonError::Io)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ComparisonError::InvalidInput);
    }
    let canonical = candidate.canonicalize().map_err(|_| ComparisonError::Io)?;
    if !canonical.starts_with(root) {
        return Err(ComparisonError::InvalidInput);
    }
    Ok(canonical)
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, ComparisonError> {
    let metadata = fs::metadata(path).map_err(|_| ComparisonError::Io)?;
    if !metadata.is_file() || metadata.len() > MAX_INPUT_BYTES {
        return Err(ComparisonError::InvalidInput);
    }
    fs::read(path).map_err(|_| ComparisonError::Io)
}

fn parse_toml<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, ComparisonError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ComparisonError::InvalidInput)?;
    toml::from_str(text).map_err(|_| ComparisonError::InvalidInput)
}

fn verify_digest(bytes: &[u8], expected: &str) -> Result<(), ComparisonError> {
    if digest_bytes(bytes) != expected {
        return Err(ComparisonError::IdentityMismatch);
    }
    Ok(())
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn valid_digest(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
}
