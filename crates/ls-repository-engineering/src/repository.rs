//! Composition of the authored package into its complete generated projection set.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::generate::{Projection, ProjectionSet};
use crate::identity::{
    built_in_conformance_vector, capability_contract_semantic_digest,
    package_manifest_semantic_digest, worker_role_contract_semantic_digest,
};
use crate::inventory::{
    discover_inventory, load_authored_package, reconcile_inventory, AuthoredPackage,
};
use crate::lock::{build_lock, lock_bytes};
use crate::schema::{
    schema_catalog, ArtifactReference, BuildProvenance, ContractState, ExecutorDescriptor,
    ImplementationSubjectManifest, NormativeLockClosure, RepositoryPath, RuntimeBundleManifest,
    ScenarioCatalog, SchemaVersion, Sha256Digest, WorkerRoleBundle,
};
use crate::validator::{validate_semantic_package, Finding};

const PACKAGE_PATH: &str = ".repository-engineering/package.toml";
const DISCOVERY_PATH: &str = ".repository-engineering/discovery-policy.toml";
const LEDGER_PATH: &str = ".repository-engineering/migration-ledger.toml";
const REGISTRY_PATH: &str = ".repository-engineering/schema-registry.json";
const CONFORMANCE_MANIFEST_PATH: &str = ".repository-engineering/conformance/v0/manifest.json";
const EXECUTOR_PATH: &str = ".repository-engineering/executors/audit-carried-rows.toml";
const ROLE_BUNDLE_PATH: &str = ".repository-engineering/roles/decommission-row-auditor.toml";
const SCENARIO_PATH: &str =
    ".repository-engineering/scenarios/audit-carried-rows/implementation.toml";
const RUNTIME_BUNDLE_PATH: &str = ".repository-engineering/runtime-bundle.json";
const IMPLEMENTATION_SUBJECT_PATH: &str =
    ".repository-engineering/implementation-subjects/audit-carried-rows.json";
const LOCK_PATH: &str = ".repository-engineering/package.lock.json";
const REFERENCE_PATH: &str = "docs/reference/repository-engineering-package.md";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryError {
    pub code: &'static str,
    pub findings: Vec<Finding>,
}

impl RepositoryError {
    fn new(code: &'static str) -> Self {
        Self {
            code,
            findings: Vec::new(),
        }
    }

    fn from_findings(code: &'static str, mut findings: Vec<Finding>) -> Self {
        findings.sort();
        findings.truncate(256);
        Self { code, findings }
    }
}

impl fmt::Display for RepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for RepositoryError {}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct SchemaRegistry {
    schema_version: SchemaVersion,
    entries: Vec<SchemaRegistryEntry>,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct SchemaRegistryEntry {
    schema_id: String,
    artifact: ArtifactReference,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct ConformanceManifest {
    schema_version: SchemaVersion,
    artifacts: Vec<ArtifactReference>,
}

pub fn compose_repository(root: &Path) -> Result<ProjectionSet, RepositoryError> {
    let authored = load_authored_package(root).map_err(|error| RepositoryError::new(error.code))?;
    let mut findings = validate_semantic_package(root, &authored);
    let inventory = discover_inventory(root, &authored.discovery_policy)
        .map_err(|error| RepositoryError::new(error.code))?;
    findings.extend(reconcile_inventory(&authored.ledger, &inventory));
    if !findings.is_empty() {
        return Err(RepositoryError::from_findings(
            "repository.validation.failed",
            findings,
        ));
    }
    let executor: ExecutorDescriptor = read_closed_toml(root, EXECUTOR_PATH)?;
    let role_bundle: WorkerRoleBundle = read_closed_toml(root, ROLE_BUNDLE_PATH)?;
    let scenario: ScenarioCatalog = read_closed_toml(root, SCENARIO_PATH)?;
    validate_portable_descriptors(&authored, &executor, &role_bundle, &scenario)?;

    let mut projections = Vec::new();
    let mut registry_entries = Vec::new();
    for (name, mut schema) in schema_catalog() {
        let schema_id = format!("urn:ls:repository-engineering:schema:v0:{name}");
        schema
            .as_object_mut()
            .ok_or_else(|| RepositoryError::new("repository.schema.invalid"))?
            .insert("$id".to_owned(), Value::String(schema_id.clone()));
        let path = format!(".repository-engineering/schemas/v0/{name}.schema.json");
        let bytes = pretty_json(&schema)?;
        registry_entries.push(SchemaRegistryEntry {
            schema_id,
            artifact: bytes_reference(&path, &bytes, "application/schema+json"),
        });
        projections.push(Projection::new(path, bytes));
    }
    let schema_artifacts: Vec<_> = registry_entries
        .iter()
        .map(|entry| entry.artifact.clone())
        .collect();
    let registry_bytes = pretty_json(&SchemaRegistry {
        schema_version: SchemaVersion::V0,
        entries: registry_entries,
    })?;
    let registry_reference = bytes_reference(REGISTRY_PATH, &registry_bytes, "application/json");
    projections.push(Projection::new(REGISTRY_PATH, registry_bytes.clone()));

    let structural_path = ".repository-engineering/conformance/v0/structural.json";
    let mut structurally_validated = vec![
        PACKAGE_PATH.to_owned(),
        DISCOVERY_PATH.to_owned(),
        LEDGER_PATH.to_owned(),
        REGISTRY_PATH.to_owned(),
    ];
    structurally_validated.extend(
        authored
            .package
            .declared_capability_contracts
            .iter()
            .chain(authored.package.declared_worker_roles.iter())
            .map(|registration| registration.path.0.clone()),
    );
    structurally_validated.sort();
    let structural_bytes = pretty_json(&json!({
        "schema_version": "v0",
        "validates": structurally_validated,
        "unknown_fields": "reject",
        "unsupported_schema_versions": "reject"
    }))?;
    let cross_record_path = ".repository-engineering/conformance/v0/cross-record.json";
    let cross_record_bytes = pretty_json(&json!({
        "schema_version": "v0",
        "rules": [
            "every_discovered_obligation_has_exactly_one_ledger_row",
            "every_ledger_source_matches_its_discovered_source",
            "declared_contract_registration_resolves_kind_id_and_path",
            "planned_replacement_is_declared_and_type_correct",
            "legacy_dependencies_remain_legacy_authoritative_below_parity",
            "semantic_claim_sources_resolve_and_field_groups_are_unique",
            "legacy_evidence_does_not_satisfy_successor_evidence",
            "unavailable_external_sources_remain_unproved_without_locator_or_digest",
            "terminal_results_preserve_assignment_row_correlation",
            "first_slice_authority_remains_legacy",
            "first_slice_activation_eligibility_is_none"
        ]
    }))?;
    let vector_path = ".repository-engineering/conformance/v0/version-set-vector.json";
    let (vector_input, expected_version_set_id) = built_in_conformance_vector()
        .map_err(|_| RepositoryError::new("repository.conformance.vector_failed"))?;
    let vector_bytes = pretty_json(&json!({
        "schema_version": "v0",
        "fixture_only": true,
        "input": vector_input,
        "expected_version_set_id": expected_version_set_id
    }))?;
    let runtime_vector_path = ".repository-engineering/conformance/v0/runtime-semantics.json";
    let runtime_vector_bytes = pretty_json(&json!({
        "schema_version": "v0",
        "cases": [
            {
                "case_id": "valid-audit-assignment",
                "kind": "audit_assignment",
                "expected": "accept",
                "input": {
                    "schema_version": "v0",
                    "attempt_id": "attempt-1",
                    "invocation_id": "invocation-1",
                    "assignment_id": "L1",
                    "row_id": "L1",
                    "idempotency_key": "attempt-1-L1",
                    "worker_instance_id": "worker-1"
                }
            },
            {
                "case_id": "valid-audit-success",
                "kind": "worker_result",
                "expected": "accept",
                "input": runtime_success_vector("L1")
            },
            {
                "case_id": "missing-attempt-id",
                "kind": "worker_result",
                "expected": "reject",
                "input": {
                    "schema_version": "v0",
                    "result": "held",
                    "invocation_id": "invocation-1",
                    "assignment_id": "L1",
                    "worker_instance_id": "worker-1",
                    "worker_instance_receipt": fixture_reference("receipts/worker-1.json", '1'),
                    "reason": "source-unavailable"
                }
            },
            {
                "case_id": "mismatched-success-row",
                "kind": "worker_result",
                "expected": "reject",
                "input": runtime_success_vector("L2")
            }
        ]
    }))?;
    let conformance_artifacts = vec![
        bytes_reference(structural_path, &structural_bytes, "application/json"),
        bytes_reference(cross_record_path, &cross_record_bytes, "application/json"),
        bytes_reference(vector_path, &vector_bytes, "application/json"),
        bytes_reference(
            runtime_vector_path,
            &runtime_vector_bytes,
            "application/json",
        ),
    ];
    let conformance_manifest_bytes = pretty_json(&ConformanceManifest {
        schema_version: SchemaVersion::V0,
        artifacts: conformance_artifacts.clone(),
    })?;
    let conformance_reference = bytes_reference(
        CONFORMANCE_MANIFEST_PATH,
        &conformance_manifest_bytes,
        "application/json",
    );
    projections.extend([
        Projection::new(structural_path, structural_bytes),
        Projection::new(cross_record_path, cross_record_bytes),
        Projection::new(vector_path, vector_bytes),
        Projection::new(runtime_vector_path, runtime_vector_bytes),
        Projection::new(
            CONFORMANCE_MANIFEST_PATH,
            conformance_manifest_bytes.clone(),
        ),
    ]);

    let executor_reference = file_reference(root, EXECUTOR_PATH, "application/toml")?;
    let role_bundle_reference = file_reference(root, ROLE_BUNDLE_PATH, "application/toml")?;
    let scenario_reference = file_reference(root, SCENARIO_PATH, "application/toml")?;
    let mut bundle_source_artifacts: Vec<_> = authored
        .capability_contracts
        .iter()
        .flat_map(|contract| contract.knowledge_references.iter().cloned())
        .chain(
            authored
                .worker_role_contracts
                .iter()
                .flat_map(|contract| contract.knowledge_references.iter().cloned()),
        )
        .collect();
    sort_and_dedup_references(&mut bundle_source_artifacts)?;

    let mut bundle_members = vec![
        executor_reference.clone(),
        role_bundle_reference.clone(),
        scenario_reference.clone(),
        registry_reference.clone(),
        conformance_reference.clone(),
    ];
    bundle_members.extend(schema_artifacts);
    bundle_members.extend(conformance_artifacts);
    bundle_members.extend(bundle_source_artifacts.iter().cloned());
    sort_and_dedup_references(&mut bundle_members)?;
    let runtime_bundle_bytes = pretty_json(&RuntimeBundleManifest {
        schema_version: SchemaVersion::V0,
        bundle_id: crate::schema::StableId("audit-carried-rows-runtime-v0".to_owned()),
        members: bundle_members,
    })?;
    let runtime_bundle_reference = bytes_reference(
        RUNTIME_BUNDLE_PATH,
        &runtime_bundle_bytes,
        "application/json",
    );
    projections.push(Projection::new(RUNTIME_BUNDLE_PATH, runtime_bundle_bytes));

    let mut subject_source_artifacts = bundle_source_artifacts;
    subject_source_artifacts.push(tree_reference(
        root,
        "tools/repository-engineering-runtime",
        "application/vnd.rust.crate",
    )?);
    sort_and_dedup_references(&mut subject_source_artifacts)?;
    let implementation_subject_bytes = pretty_json(&ImplementationSubjectManifest {
        schema_version: SchemaVersion::V0,
        subject_id: crate::schema::StableId("audit-carried-rows".to_owned()),
        executor: executor_reference,
        role_bundle: role_bundle_reference,
        runtime_bundle: runtime_bundle_reference.clone(),
        scenario_catalog: scenario_reference,
        source_artifacts: subject_source_artifacts,
        schema_registry: registry_reference.clone(),
        conformance_corpus: conformance_reference.clone(),
    })?;
    let implementation_subject_reference = bytes_reference(
        IMPLEMENTATION_SUBJECT_PATH,
        &implementation_subject_bytes,
        "application/json",
    );
    projections.push(Projection::new(
        IMPLEMENTATION_SUBJECT_PATH,
        implementation_subject_bytes,
    ));

    let capability_contracts = authored
        .package
        .declared_capability_contracts
        .iter()
        .zip(&authored.capability_contracts)
        .map(|(registration, contract)| {
            Ok(ArtifactReference {
                schema_version: SchemaVersion::V0,
                path: registration.path.clone(),
                sha256: capability_contract_semantic_digest(contract)
                    .map_err(|_| RepositoryError::new("repository.semantic_digest_failed"))?,
                media_type: "application/toml".to_owned(),
            })
        })
        .collect::<Result<Vec<_>, RepositoryError>>()?;
    let worker_role_contracts = authored
        .package
        .declared_worker_roles
        .iter()
        .zip(&authored.worker_role_contracts)
        .map(|(registration, contract)| {
            Ok(ArtifactReference {
                schema_version: SchemaVersion::V0,
                path: registration.path.clone(),
                sha256: worker_role_contract_semantic_digest(contract)
                    .map_err(|_| RepositoryError::new("repository.semantic_digest_failed"))?,
                media_type: "application/toml".to_owned(),
            })
        })
        .collect::<Result<Vec<_>, RepositoryError>>()?;
    let normative = NormativeLockClosure {
        package: ArtifactReference {
            schema_version: SchemaVersion::V0,
            path: RepositoryPath(PACKAGE_PATH.to_owned()),
            sha256: package_manifest_semantic_digest(&authored.package)
                .map_err(|_| RepositoryError::new("repository.semantic_digest_failed"))?,
            media_type: "application/toml".to_owned(),
        },
        discovery_policy: semantic_reference(
            DISCOVERY_PATH,
            &authored.discovery_policy,
            "application/toml",
        )?,
        migration_ledger: semantic_reference(LEDGER_PATH, &authored.ledger, "application/toml")?,
        schema_registry: registry_reference,
        conformance_corpus: conformance_reference,
        runtime_bundle: runtime_bundle_reference,
        implementation_subjects: vec![implementation_subject_reference],
        capability_contracts,
        worker_role_contracts,
        optional_components: authored.package.optional_components.clone(),
    };
    let build_provenance = BuildProvenance {
        generator: tree_reference(
            root,
            "crates/ls-repository-engineering",
            "application/vnd.rust.crate",
        )?,
        dependency_lock: file_reference(root, "Cargo.lock", "application/toml")?,
        workflow_pins: workflow_pins(root)?,
    };
    let exact_lock = build_lock(normative, build_provenance)
        .map_err(|_| RepositoryError::new("repository.lock.identity_failed"))?;
    projections.push(Projection::new(
        LOCK_PATH,
        lock_bytes(&exact_lock)
            .map_err(|_| RepositoryError::new("repository.lock.serialize_failed"))?,
    ));
    projections.push(Projection::new(
        REFERENCE_PATH,
        reference_document(&authored, &exact_lock.package_lock_id.0).into_bytes(),
    ));

    ProjectionSet::new(projections).map_err(|error| RepositoryError::new(error.code))
}

fn read_closed_toml<T: DeserializeOwned>(
    root: &Path,
    relative: &str,
) -> Result<T, RepositoryError> {
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| RepositoryError::new("repository.portable_artifact.read_failed"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(RepositoryError::new("repository.portable_artifact.unsafe"));
    }
    let bytes = fs::read(path)
        .map_err(|_| RepositoryError::new("repository.portable_artifact.read_failed"))?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| RepositoryError::new("repository.portable_artifact.invalid_utf8"))?;
    toml::from_str(text).map_err(|_| RepositoryError::new("repository.portable_artifact.invalid"))
}

fn validate_portable_descriptors(
    authored: &AuthoredPackage,
    executor: &ExecutorDescriptor,
    role_bundle: &WorkerRoleBundle,
    scenario: &ScenarioCatalog,
) -> Result<(), RepositoryError> {
    let capability = authored
        .capability_contracts
        .first()
        .ok_or_else(|| RepositoryError::new("repository.portable_artifact.contract_missing"))?;
    let worker = authored
        .worker_role_contracts
        .first()
        .ok_or_else(|| RepositoryError::new("repository.portable_artifact.contract_missing"))?;
    let expected_knowledge: BTreeSet<_> = worker
        .knowledge_references
        .iter()
        .map(|reference| reference.path.0.as_str())
        .collect();
    let actual_knowledge: BTreeSet<_> = role_bundle
        .knowledge_paths
        .iter()
        .map(|path| path.0.as_str())
        .collect();
    if executor.executor_id != capability.capability_id
        || executor.capability_id != capability.capability_id
        || executor.worker_role_id != worker.role_id
        || executor.effective_concurrency_cap != 2
        || executor.phases
            != [
                "discovering",
                "dispatching",
                "rolling_up",
                "gate_computed",
                "complete",
            ]
            .map(|value| crate::schema::StableId(value.to_owned()))
        || role_bundle.role_id != worker.role_id
        || role_bundle.assignment_schema.0 != "audit-assignment"
        || role_bundle.result_schema.0 != "worker-result"
        || role_bundle.record_format.0
            != ".agents/skills/audit-carried-rows/references/record-format.md"
        || expected_knowledge != actual_knowledge
        || scenario.capability_id != capability.capability_id
        || scenario.positive_cases.is_empty()
        || scenario.negative_cases.is_empty()
    {
        return Err(RepositoryError::new(
            "repository.portable_artifact.contract_mismatch",
        ));
    }
    Ok(())
}

fn sort_and_dedup_references(
    references: &mut Vec<ArtifactReference>,
) -> Result<(), RepositoryError> {
    references.sort_by(|left, right| left.path.cmp(&right.path));
    for pair in references.windows(2) {
        if pair[0].path == pair[1].path && pair[0] != pair[1] {
            return Err(RepositoryError::new(
                "repository.portable_artifact.reference_conflict",
            ));
        }
    }
    references.dedup_by(|left, right| left.path == right.path);
    Ok(())
}

fn workflow_pins(root: &Path) -> Result<Vec<ArtifactReference>, RepositoryError> {
    let path = ".github/workflows/repository-engineering-check.yml";
    match fs::read(root.join(path)) {
        Ok(bytes) => Ok(vec![bytes_reference(path, &bytes, "application/yaml")]),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(_) => Err(RepositoryError::new("repository.provenance.read_failed")),
    }
}

fn runtime_success_vector(row_id: &str) -> Value {
    json!({
        "schema_version": "v0",
        "result": "succeeded",
        "attempt_id": "attempt-1",
        "invocation_id": "invocation-1",
        "assignment_id": "L1",
        "worker_instance_id": "worker-1",
        "worker_instance_receipt": fixture_reference("receipts/worker-1.json", '1'),
        "payload": {
            "row_id": row_id,
            "verdict": "unverifiable",
            "record": fixture_reference("records/L1.yaml", '2')
        }
    })
}

fn fixture_reference(path: &str, digest_byte: char) -> Value {
    json!({
        "schema_version": "v0",
        "path": path,
        "sha256": format!("sha256:{}", digest_byte.to_string().repeat(64)),
        "media_type": "application/json"
    })
}

fn semantic_reference<T: Serialize>(
    path: &str,
    value: &T,
    media_type: &str,
) -> Result<ArtifactReference, RepositoryError> {
    let canonical = serde_json_canonicalizer::to_vec(value)
        .map_err(|_| RepositoryError::new("repository.semantic_digest_failed"))?;
    Ok(bytes_reference(path, &canonical, media_type))
}

fn file_reference(
    root: &Path,
    path: &str,
    media_type: &str,
) -> Result<ArtifactReference, RepositoryError> {
    let bytes = fs::read(root.join(path))
        .map_err(|_| RepositoryError::new("repository.provenance.read_failed"))?;
    Ok(bytes_reference(path, &bytes, media_type))
}

fn tree_reference(
    root: &Path,
    relative: &str,
    media_type: &str,
) -> Result<ArtifactReference, RepositoryError> {
    let base = root.join(relative);
    let mut files = Vec::new();
    collect_regular_files(&base, &base, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = Sha256::new();
    hasher.update(b"ls-repository-engineering/tree/v0\0");
    for (path, bytes) in files {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(&bytes);
        hasher.update([0]);
    }
    Ok(ArtifactReference {
        schema_version: SchemaVersion::V0,
        path: RepositoryPath(relative.to_owned()),
        sha256: Sha256Digest(format!("sha256:{:x}", hasher.finalize())),
        media_type: media_type.to_owned(),
    })
}

fn collect_regular_files(
    base: &Path,
    directory: &Path,
    files: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), RepositoryError> {
    let mut entries: Vec<_> = fs::read_dir(directory)
        .map_err(|_| RepositoryError::new("repository.provenance.read_failed"))?
        .collect::<Result<_, _>>()
        .map_err(|_| RepositoryError::new("repository.provenance.read_failed"))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let metadata = entry
            .file_type()
            .map_err(|_| RepositoryError::new("repository.provenance.read_failed"))?;
        let path = entry.path();
        if metadata.is_symlink() {
            return Err(RepositoryError::new(
                "repository.provenance.symlink_forbidden",
            ));
        }
        if metadata.is_dir() && entry.file_name() == "target" {
            continue;
        }
        if metadata.is_dir() {
            collect_regular_files(base, &path, files)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(base)
                .ok()
                .and_then(Path::to_str)
                .ok_or_else(|| RepositoryError::new("repository.provenance.path_invalid"))?
                .replace('\\', "/");
            let bytes = fs::read(path)
                .map_err(|_| RepositoryError::new("repository.provenance.read_failed"))?;
            files.push((relative, bytes));
        }
    }
    Ok(())
}

fn bytes_reference(path: &str, bytes: &[u8], media_type: &str) -> ArtifactReference {
    ArtifactReference {
        schema_version: SchemaVersion::V0,
        path: RepositoryPath(path.to_owned()),
        sha256: Sha256Digest(format!("sha256:{:x}", Sha256::digest(bytes))),
        media_type: media_type.to_owned(),
    }
}

fn pretty_json<T: Serialize>(value: &T) -> Result<Vec<u8>, RepositoryError> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| RepositoryError::new("repository.projection.serialize_failed"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn reference_document(authored: &AuthoredPackage, package_lock_id: &str) -> String {
    let mut rows: Vec<_> = authored.ledger.rows.iter().collect();
    rows.sort_by(|left, right| left.logical_id.cmp(&right.logical_id));
    let planned_rows = rows
        .iter()
        .filter(|row| row.migration_state == crate::schema::MigrationState::Planned)
        .count();
    let mut document = format!(
        "# Repository Engineering Package\n\nThis page is generated from the inert, reviewed package declaration. It grants no runtime, credential, activation, authority-transfer, retirement, or publication authority.\n\n- Schema version: `v0`\n- Package lock identity: `{package_lock_id}`\n- Activation eligibility: `{}`\n- Declared capability contracts: `{}`\n- Declared worker roles: `{}`\n- Active capability contracts: `{}`\n- Active worker roles: `{}`\n- Reviewed migration rows: `{}`\n- Planned migration rows: `{planned_rows}`\n\nDeclaration, migration planning, implementation, certification, parity, activation, authority, and retirement are independent states. Canonical lifecycle statements below come only from validated typed fields.\n\n## Declared contracts\n",
        enum_token(&authored.package.activation_eligibility),
        authored.package.declared_capability_contracts.len(),
        authored.package.declared_worker_roles.len(),
        authored.package.active_capability_contracts.len(),
        authored.package.active_worker_roles.len(),
        rows.len(),
    );
    for contract in &authored.capability_contracts {
        document.push_str(&format!(
            "\n### Capability `{}`\n\nNon-normative purpose text (not identity-bound and not lifecycle evidence): {}\n\nCanonical typed state: {}; activation: {}; executor: {}; scenarios: {}.\n",
            escape_markdown(&contract.capability_id.0),
            escape_markdown(contract.public_description.as_deref().unwrap_or("not provided")),
            state_summary(&contract.state),
            active_token(
                authored
                    .package
                    .active_capability_contracts
                    .contains(&contract.capability_id)
            ),
            if contract.executor.is_some() { "present" } else { "absent" },
            contract.scenario_references.len(),
        ));
        if let Some(evidence) = &contract.evidence_status {
            let artifact_set_members: usize = evidence
                .legacy_artifact_sets
                .iter()
                .map(|set| set.members.len())
                .sum();
            document.push_str(&format!(
                "\nEvidence boundary: legacy evidence `{}` ({} artifact(s), {} artifact set(s), {} set member(s)); successor implementation evidence `{}`; parity `{}`; certification `{}`; legacy evidence satisfies successor: `{}`.\n",
                enum_token(&evidence.legacy_status),
                evidence.legacy_artifacts.len(),
                evidence.legacy_artifact_sets.len(),
                artifact_set_members,
                enum_token(&evidence.successor_implementation),
                enum_token(&evidence.parity),
                enum_token(&evidence.certification),
                evidence.legacy_evidence_satisfies_successor,
            ));
        }
        if !contract.external_source_requirements.is_empty() {
            document.push_str("\nExternal source requirements:\n\n| Requirement | Status | Locator | Digest | Unavailable outcome | Worker verdict |\n|---|---|---|---|---|---|\n");
            for requirement in &contract.external_source_requirements {
                document.push_str(&format!(
                    "| {} | {} | {} | {} | {} | {} |\n",
                    escape_markdown(&requirement.requirement_id.0),
                    enum_token(&requirement.status),
                    requirement
                        .locator
                        .as_ref()
                        .map(|value| escape_markdown(&value.0))
                        .unwrap_or_else(|| "absent".to_owned()),
                    requirement
                        .digest
                        .as_ref()
                        .map(|value| escape_markdown(&value.0))
                        .unwrap_or_else(|| "absent".to_owned()),
                    enum_token(&requirement.unavailable_capability_outcome),
                    escape_markdown(&requirement.unavailable_worker_verdict.0),
                ));
            }
        }
        append_claims(&mut document, &contract.semantic_claims);
    }
    for contract in &authored.worker_role_contracts {
        document.push_str(&format!(
            "\n### Worker role `{}`\n\nNon-normative purpose text (not identity-bound and not lifecycle evidence): {}\n\nCanonical typed state: {}; activation: {}; terminal correlation: {}.\n",
            escape_markdown(&contract.role_id.0),
            escape_markdown(contract.public_description.as_deref().unwrap_or("not provided")),
            state_summary(&contract.state),
            active_token(authored.package.active_worker_roles.contains(&contract.role_id)),
            if contract.terminal_result_correlation.is_some() {
                "required"
            } else {
                "absent"
            },
        ));
        append_claims(&mut document, &contract.semantic_claims);
    }

    document.push_str("\n## Migration ledger\n\n| Logical ID | Source kind | Source locator | Disposition | Migration | Absence reason | Declaration | Implementation | Certification | Authority | Retirement | Replacement |\n|---|---|---|---|---|---|---|---|---|---|---|---|\n");
    for row in rows {
        let replacement = row
            .replacement_contract
            .as_ref()
            .map(|value| value.0.as_str())
            .unwrap_or("absent");
        let state = row
            .replacement_contract
            .as_ref()
            .and_then(|replacement| replacement_state(authored, replacement));
        document.push_str(&format!(
            "| {} | {} | `{}` | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            escape_markdown(&row.logical_id.0),
            escape_markdown(&row.source_kind.0),
            escape_markdown(&row.source_locator.0),
            enum_token(&row.disposition),
            enum_token(&row.migration_state),
            row.absence_reason
                .as_ref()
                .map(|reason| escape_markdown(&reason.0))
                .unwrap_or_else(|| "absent".to_owned()),
            state
                .map(|value| enum_token(&value.declaration))
                .unwrap_or_else(|| "absent".to_owned()),
            state
                .map(|value| enum_token(&value.implementation))
                .unwrap_or_else(|| "unported".to_owned()),
            state
                .map(|value| enum_token(&value.certification))
                .unwrap_or_else(|| "uncertified".to_owned()),
            enum_token(&row.current_authority),
            state
                .map(|value| enum_token(&value.retirement))
                .unwrap_or_else(|| "not_started".to_owned()),
            escape_markdown(replacement),
        ));
    }
    document
}

fn replacement_state<'a>(
    authored: &'a AuthoredPackage,
    replacement: &crate::schema::StableId,
) -> Option<&'a ContractState> {
    authored
        .capability_contracts
        .iter()
        .find(|contract| &contract.capability_id == replacement)
        .map(|contract| &contract.state)
        .or_else(|| {
            authored
                .worker_role_contracts
                .iter()
                .find(|contract| &contract.role_id == replacement)
                .map(|contract| &contract.state)
        })
}

fn state_summary(state: &ContractState) -> String {
    format!(
        "declaration `{}`, implementation `{}`, certification `{}`, authority `{}`, retirement `{}`",
        enum_token(&state.declaration),
        enum_token(&state.implementation),
        enum_token(&state.certification),
        enum_token(&state.authority),
        enum_token(&state.retirement),
    )
}

fn active_token(active: bool) -> &'static str {
    if active {
        "active"
    } else {
        "inactive"
    }
}

fn append_claims(document: &mut String, claims: &[crate::schema::SemanticClaim]) {
    if claims.is_empty() {
        return;
    }
    document.push_str("\nIdentity-bearing semantic provenance:\n\n| Field groups | Status | Source basis |\n|---|---|---|\n");
    for claim in claims {
        let fields = claim
            .field_groups
            .iter()
            .map(|field| field.0.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let sources = claim
            .sources
            .iter()
            .filter_map(|source| serde_json::to_string(source).ok())
            .map(|source| escape_markdown(&source))
            .collect::<Vec<_>>()
            .join("; ");
        document.push_str(&format!(
            "| {} | {} | `{}` |\n",
            escape_markdown(&fields),
            enum_token(&claim.status),
            sources,
        ));
    }
}

fn enum_token<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_owned())
}

fn escape_markdown(value: &str) -> String {
    value.replace('|', "\\|").replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::reference_document;
    use crate::inventory::load_authored_package;
    use std::path::Path;

    #[test]
    fn contradictory_presentation_stays_non_normative_and_adjacent_to_typed_state() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap();
        let mut authored = load_authored_package(root).unwrap();
        authored.capability_contracts[0].public_description =
            Some("Certified and successor-authoritative.".to_owned());
        let document = reference_document(&authored, "sha256:test");
        assert!(document.contains(
            "Non-normative purpose text (not identity-bound and not lifecycle evidence): Certified and successor-authoritative.\n\nCanonical typed state: declaration `declared`, implementation `implemented`, certification `uncertified`, authority `legacy`, retirement `not_started`; activation: inactive"
        ));
    }
}
