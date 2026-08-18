//! Closed, versioned vocabulary for repository-engineering artifacts.

use std::collections::BTreeMap;

use schemars::generate::{Contract, SchemaSettings};
use schemars::JsonSchema;
use serde::de;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SchemaVersion {
    V0,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct StableId(#[schemars(regex(pattern = r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$"))] pub String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct RepositoryPath(#[schemars(regex(pattern = r"^[A-Za-z0-9._/-]{1,1024}$"))] pub String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct Sha256Digest(#[schemars(regex(pattern = r"^sha256:[0-9a-f]{64}$"))] pub String);

impl<'de> Deserialize<'de> for StableId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        is_stable_id(&value)
            .then_some(Self(value))
            .ok_or_else(|| de::Error::custom("invalid stable identifier"))
    }
}

impl<'de> Deserialize<'de> for RepositoryPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        is_repository_path(&value)
            .then_some(Self(value))
            .ok_or_else(|| de::Error::custom("invalid repository path"))
    }
}

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        is_sha256_digest(&value)
            .then_some(Self(value))
            .ok_or_else(|| de::Error::custom("invalid sha256 digest"))
    }
}

fn is_stable_id(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn is_repository_path(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 1_024
        || value.starts_with('/')
        || value.contains('\\')
        || !value.is_ascii()
    {
        return false;
    }
    let trimmed = value.strip_suffix('/').unwrap_or(value);
    !trimmed.is_empty()
        && trimmed.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && !segment.ends_with('.')
                && !segment.ends_with(' ')
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        })
}

fn is_sha256_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactReference {
    pub schema_version: SchemaVersion,
    pub path: RepositoryPath,
    pub sha256: Sha256Digest,
    pub media_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActivationEligibility {
    None,
    Shadow,
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeclarationState {
    Declared,
    Absent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImplementationState {
    Unported,
    Implemented,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CertificationState {
    Uncertified,
    Certified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityState {
    Legacy,
    Successor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RetirementState {
    NotStarted,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContractState {
    pub declaration: DeclarationState,
    pub implementation: ImplementationState,
    pub certification: CertificationState,
    pub authority: AuthorityState,
    pub retirement: RetirementState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Compatibility {
    pub contract_family: StableId,
    pub minimum_reader: SchemaVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackagePaths {
    pub discovery_policy: RepositoryPath,
    pub migration_ledger: RepositoryPath,
    pub schema_registry: RepositoryPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeclaredContractRegistration {
    pub id: StableId,
    pub path: RepositoryPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OptionalComponentKind {
    OrcaUi,
    WorkerAdapter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "selection", rename_all = "snake_case", deny_unknown_fields)]
pub enum OptionalComponent {
    Disabled {
        component: OptionalComponentKind,
    },
    Selected {
        component: OptionalComponentKind,
        artifact: ArtifactReference,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub schema_version: SchemaVersion,
    pub repository_id: StableId,
    pub package_id: StableId,
    pub compatibility: Compatibility,
    pub activation_eligibility: ActivationEligibility,
    pub paths: PackagePaths,
    #[serde(default)]
    pub declared_capability_contracts: Vec<DeclaredContractRegistration>,
    #[serde(default)]
    pub declared_worker_roles: Vec<DeclaredContractRegistration>,
    pub active_capability_contracts: Vec<StableId>,
    pub active_worker_roles: Vec<StableId>,
    pub optional_components: Vec<OptionalComponent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscovererPolicy {
    pub source_kind: StableId,
    pub prefix: RepositoryPath,
    pub marker: Option<String>,
    pub include_descendants: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewedExclusion {
    pub prefix: RepositoryPath,
    pub reason: StableId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeclaredObligation {
    pub logical_id: StableId,
    pub source_kind: StableId,
    pub locator: RepositoryPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryPolicy {
    pub schema_version: SchemaVersion,
    pub discoverers: Vec<DiscovererPolicy>,
    pub exact_instruction_paths: Vec<RepositoryPath>,
    pub exclusions: Vec<ReviewedExclusion>,
    pub declared_obligations: Vec<DeclaredObligation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum AutonomyClass {
    A0,
    A1,
    A2,
    A3,
    A4,
    AX,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SafetyOverlay {
    CredentialFree,
    NoExternalMutation,
    HumanGate,
    PathConfined,
    RedactedDiagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    String,
    Boolean,
    IntegerString,
    ArtifactReference,
    StableId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TypedField {
    pub name: StableId,
    pub field_type: FieldType,
    pub required: bool,
    #[serde(default)]
    pub allowed_values: Vec<StableId>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeKind {
    Succeeded,
    Held,
    Cancelled,
    PolicyViolated,
    Failed,
    RecoveryRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DispatchConcurrency {
    Serial,
    Parallel,
    BoundedParallel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DispatchCohort {
    pub cohort_id: StableId,
    pub candidate_classes: Vec<StableId>,
    pub concurrency: DispatchConcurrency,
    pub max_concurrency: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResumeSemantics {
    pub completed_assignment_policy: StableId,
    pub orphan_record_policy: StableId,
    pub mismatch_outcome: OutcomeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TerminalCondition {
    pub outcome: OutcomeKind,
    pub condition: StableId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoordinationSemantics {
    pub coverage: StableId,
    pub assignment: StableId,
    pub dispatch_cohorts: Vec<DispatchCohort>,
    pub checkpoint_owner: StableId,
    pub phases: Vec<StableId>,
    pub resume: ResumeSemantics,
    pub roll_up: StableId,
    pub terminal_conditions: Vec<TerminalCondition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FutureExecutorBoundary {
    pub attendance: StableId,
    pub credential_scope: StableId,
    pub environment: StableId,
    pub review_requirements: Vec<StableId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CredentialBoundary {
    pub credential_free_scopes: Vec<StableId>,
    pub future_executor: FutureExecutorBoundary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceAvailability {
    Absent,
    AvailableValidated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ParityStatus {
    Unproved,
    Proved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactSetReference {
    pub artifact_set_id: StableId,
    pub members: Vec<RepositoryPath>,
    pub aggregate_digest: Sha256Digest,
    pub validation_basis: ArtifactReference,
}

impl ArtifactSetReference {
    pub fn normalized_members(&self) -> Vec<RepositoryPath> {
        let mut members = self.members.clone();
        members.sort();
        members
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvidenceStatus {
    pub legacy_status: EvidenceAvailability,
    pub legacy_artifacts: Vec<ArtifactReference>,
    pub legacy_artifact_sets: Vec<ArtifactSetReference>,
    pub successor_implementation: EvidenceAvailability,
    pub parity: ParityStatus,
    pub certification: CertificationState,
    pub legacy_evidence_satisfies_successor: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImplementationComponentKind {
    Capability,
    WorkerRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImplementationEvidenceReference {
    pub component_kind: ImplementationComponentKind,
    pub component_id: StableId,
    pub subject_manifest: ArtifactReference,
    pub evidence: ArtifactReference,
    pub validation_basis: ArtifactReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImplementationEvidence {
    pub schema_version: SchemaVersion,
    pub evidence_id: StableId,
    pub component_kind: ImplementationComponentKind,
    pub component_id: StableId,
    pub subject_manifest: ArtifactReference,
    pub scenario_catalog: ArtifactReference,
    pub validation_basis: ArtifactReference,
    pub runtime_hosts: Vec<StableId>,
    pub validated_scenarios: Vec<StableId>,
    pub row_count: u16,
    pub closed_bundle_validated: bool,
    pub closed_result_validator_used: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoundedEvidenceReference {
    pub component_kind: ImplementationComponentKind,
    pub component_id: StableId,
    pub evidence: ArtifactReference,
    pub comparator_policy: ArtifactReference,
    pub wave1_package_lock_id: Sha256Digest,
    pub global_parity_eligible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LegacyClassification {
    Behavioral,
    Knowledge,
    Discard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BoundedRollUp {
    Unchanged,
    RedispositionRequired,
    UnchangedBlocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoundedCaseResult {
    pub case_id: StableId,
    pub row_id: StableId,
    pub classification: LegacyClassification,
    pub legacy_verdict: AuditVerdict,
    pub successor_verdict: AuditVerdict,
    pub legacy_completed: bool,
    pub successor_completed: bool,
    pub legacy_blocking: bool,
    pub successor_blocking: bool,
    pub legacy_roll_up: BoundedRollUp,
    pub successor_roll_up: BoundedRollUp,
    pub legacy_credential_rule: bool,
    pub successor_credential_rule: bool,
    pub legacy_path_rule: bool,
    pub successor_path_rule: bool,
    pub agreement: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoundedConformanceResult {
    pub dimension: StableId,
    pub case_id: StableId,
    pub passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoundedAdapterFacts {
    pub legacy_normalizer: StableId,
    pub successor_normalizer: StableId,
    pub successor_host: StableId,
    pub configured_global_limit: usize,
    pub effective_global_limit: usize,
    pub output_mode: StableId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoundedComparisonEvidence {
    pub schema_version: SchemaVersion,
    pub evidence_kind: StableId,
    pub evidence_id: StableId,
    pub comparator_policy: ArtifactReference,
    pub case_catalog: ArtifactReference,
    pub deterministic_invocation_id: StableId,
    pub wave1_package_lock_id: Sha256Digest,
    pub implementation_subject: ArtifactReference,
    pub capability_contract: ArtifactReference,
    pub worker_role_contract: ArtifactReference,
    pub executor: ArtifactReference,
    pub successor_scenario: ArtifactReference,
    pub migration_source_manifest: ArtifactReference,
    pub legacy_ledger: ArtifactReference,
    pub legacy_report: ArtifactReference,
    pub legacy_oracle: ArtifactReference,
    pub legacy_artifact_set_digest: Sha256Digest,
    pub legacy_corpus_digest: Sha256Digest,
    pub successor_conformance_basis: Vec<ArtifactReference>,
    pub adapter_facts: BoundedAdapterFacts,
    pub expected_case_ids: Vec<StableId>,
    pub observed_legacy_case_ids: Vec<StableId>,
    pub observed_successor_case_ids: Vec<StableId>,
    pub results: Vec<BoundedCaseResult>,
    pub conformance: Vec<BoundedConformanceResult>,
    pub compared_dimensions: Vec<StableId>,
    pub successor_only_dimensions: Vec<StableId>,
    pub exclusions: Vec<String>,
    pub failures: Vec<StableId>,
    pub cancellations: Vec<StableId>,
    pub bounded_agreement: bool,
    pub global_parity_eligible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalSourceStatus {
    UnavailableUnproved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExternalSourceRequirement {
    pub requirement_id: StableId,
    pub purpose: StableId,
    pub required_phase: StableId,
    pub status: ExternalSourceStatus,
    pub locator: Option<RepositoryPath>,
    pub digest: Option<Sha256Digest>,
    pub unavailable_capability_outcome: OutcomeKind,
    pub unavailable_worker_verdict: StableId,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct SemanticFieldPath(
    #[schemars(regex(pattern = r"^[a-z][a-z0-9_.*\[\]]{0,255}$"))] pub String,
);

impl<'de> Deserialize<'de> for SemanticFieldPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        is_semantic_field_path(&value)
            .then_some(Self(value))
            .ok_or_else(|| de::Error::custom("invalid semantic field path"))
    }
}

fn is_semantic_field_path(value: &str) -> bool {
    (1..=256).contains(&value.len())
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'_' | b'.' | b'[' | b']' | b'*')
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SemanticClaimStatus {
    LegacyObserved,
    SuccessorRequirement,
    UnavailableUnproved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "source_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SemanticClaimSource {
    KnowledgeReference {
        path: RepositoryPath,
    },
    WorkerKnowledgeReference {
        role_id: StableId,
        path: RepositoryPath,
    },
    MigrationLedgerRows {
        logical_ids: Vec<StableId>,
    },
    LegacyArtifactSet {
        artifact_set_id: StableId,
    },
    SuccessorDecision {
        decision_id: StableId,
    },
    ExternalSourceRequirement {
        requirement_id: StableId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SemanticClaim {
    pub field_groups: Vec<SemanticFieldPath>,
    pub status: SemanticClaimStatus,
    pub sources: Vec<SemanticClaimSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CapabilityContract {
    pub schema_version: SchemaVersion,
    pub capability_id: StableId,
    pub public_description: Option<String>,
    pub state: ContractState,
    pub autonomy: AutonomyClass,
    pub safety_overlays: Vec<SafetyOverlay>,
    pub credential_boundary: Option<CredentialBoundary>,
    pub inputs: Vec<TypedField>,
    pub outcomes: Vec<OutcomeKind>,
    pub coordination_semantics: Option<CoordinationSemantics>,
    pub touched_paths: Vec<RepositoryPath>,
    pub evidence_obligations: Vec<StableId>,
    pub evidence_status: Option<EvidenceStatus>,
    pub human_gates: Vec<StableId>,
    pub executor: Option<ArtifactReference>,
    #[serde(default)]
    pub implementation_evidence: Option<ImplementationEvidenceReference>,
    #[serde(default)]
    pub bounded_evidence: Vec<BoundedEvidenceReference>,
    pub knowledge_references: Vec<ArtifactReference>,
    #[serde(default)]
    pub external_source_requirements: Vec<ExternalSourceRequirement>,
    #[serde(default)]
    pub legacy_authority_dependencies: Vec<StableId>,
    pub worker_roles: Vec<StableId>,
    pub scenario_references: Vec<ArtifactReference>,
    #[serde(default)]
    pub semantic_claims: Vec<SemanticClaim>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConcurrencyClass {
    Serial,
    BoundedParallel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CorrelationRule {
    AllEqual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TerminalResultCorrelation {
    pub assignment_field: StableId,
    pub envelope_field: StableId,
    pub success_payload_field: StableId,
    pub required_variants: Vec<OutcomeKind>,
    pub correlation: CorrelationRule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditAssignment {
    pub schema_version: SchemaVersion,
    pub attempt_id: StableId,
    pub invocation_id: StableId,
    pub assignment_id: StableId,
    pub row_id: StableId,
    pub idempotency_key: StableId,
    pub worker_instance_id: StableId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutorDescriptor {
    pub schema_version: SchemaVersion,
    pub executor_id: StableId,
    pub capability_id: StableId,
    pub worker_role_id: StableId,
    pub phases: Vec<StableId>,
    pub effective_concurrency_cap: u16,
    pub state_owner: StableId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkerRoleBundle {
    pub schema_version: SchemaVersion,
    pub role_id: StableId,
    pub assignment_schema: StableId,
    pub result_schema: StableId,
    pub knowledge_paths: Vec<RepositoryPath>,
    pub record_format: RepositoryPath,
    pub safety_rules: Vec<StableId>,
    pub verdicts: Vec<AuditVerdict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScenarioCatalog {
    pub schema_version: SchemaVersion,
    pub catalog_id: StableId,
    pub capability_id: StableId,
    pub positive_cases: Vec<StableId>,
    pub negative_cases: Vec<StableId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkerRoleContract {
    pub schema_version: SchemaVersion,
    pub role_id: StableId,
    pub public_description: Option<String>,
    pub state: ContractState,
    pub assignment_fields: Vec<TypedField>,
    pub result_fields: Vec<TypedField>,
    pub fresh_context_required: bool,
    pub concurrency: ConcurrencyClass,
    pub cancellation_supported: bool,
    pub idempotency_key_required: bool,
    pub result_validation_required: bool,
    pub terminal_result_correlation: Option<TerminalResultCorrelation>,
    #[serde(default)]
    pub role_bundle: Option<ArtifactReference>,
    #[serde(default)]
    pub scenario_references: Vec<ArtifactReference>,
    #[serde(default)]
    pub implementation_evidence: Option<ImplementationEvidenceReference>,
    #[serde(default)]
    pub bounded_evidence: Vec<BoundedEvidenceReference>,
    #[serde(default)]
    pub knowledge_references: Vec<ArtifactReference>,
    #[serde(default)]
    pub semantic_claims: Vec<SemanticClaim>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerResult {
    Succeeded {
        schema_version: SchemaVersion,
        attempt_id: StableId,
        invocation_id: StableId,
        assignment_id: StableId,
        worker_instance_id: StableId,
        worker_instance_receipt: ArtifactReference,
        payload: AuditSuccessPayload,
    },
    Held {
        schema_version: SchemaVersion,
        attempt_id: StableId,
        invocation_id: StableId,
        assignment_id: StableId,
        worker_instance_id: StableId,
        worker_instance_receipt: ArtifactReference,
        reason: StableId,
    },
    Cancelled {
        schema_version: SchemaVersion,
        attempt_id: StableId,
        invocation_id: StableId,
        assignment_id: StableId,
        worker_instance_id: StableId,
        worker_instance_receipt: ArtifactReference,
        reason: StableId,
    },
    PolicyViolated {
        schema_version: SchemaVersion,
        attempt_id: StableId,
        invocation_id: StableId,
        assignment_id: StableId,
        worker_instance_id: StableId,
        worker_instance_receipt: ArtifactReference,
        policy_id: StableId,
    },
    Failed {
        schema_version: SchemaVersion,
        attempt_id: StableId,
        invocation_id: StableId,
        assignment_id: StableId,
        worker_instance_id: StableId,
        worker_instance_receipt: ArtifactReference,
        error_code: StableId,
    },
    RecoveryRequired {
        schema_version: SchemaVersion,
        attempt_id: StableId,
        invocation_id: StableId,
        assignment_id: StableId,
        worker_instance_id: StableId,
        worker_instance_receipt: ArtifactReference,
        checkpoint: ArtifactReference,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuditVerdict {
    Confirmed,
    Refuted,
    Unverifiable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditSuccessPayload {
    pub row_id: StableId,
    pub verdict: AuditVerdict,
    pub record: ArtifactReference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MigrationDisposition {
    Port,
    Merge,
    ReplaceWithExecutor,
    Retire,
    GlobalCleanup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MigrationState {
    Unported,
    Planned,
    ParityProven,
    AuthorityTransferred,
    RetirementComplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MigrationRow {
    pub logical_id: StableId,
    pub source_kind: StableId,
    pub source_locator: RepositoryPath,
    pub source_digest: Option<Sha256Digest>,
    pub current_authority: AuthorityState,
    pub disposition: MigrationDisposition,
    pub migration_state: MigrationState,
    pub replacement_contract: Option<StableId>,
    pub parity_reference: Option<ArtifactReference>,
    pub absence_reason: Option<StableId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MigrationLedger {
    pub schema_version: SchemaVersion,
    pub rows: Vec<MigrationRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NormativeLockClosure {
    pub package: ArtifactReference,
    pub discovery_policy: ArtifactReference,
    pub migration_ledger: ArtifactReference,
    pub schema_registry: ArtifactReference,
    pub conformance_corpus: ArtifactReference,
    pub runtime_bundle: ArtifactReference,
    pub implementation_subjects: Vec<ArtifactReference>,
    pub capability_contracts: Vec<ArtifactReference>,
    pub worker_role_contracts: Vec<ArtifactReference>,
    pub optional_components: Vec<OptionalComponent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeBundleManifest {
    pub schema_version: SchemaVersion,
    pub bundle_id: StableId,
    pub members: Vec<ArtifactReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImplementationSubjectManifest {
    pub schema_version: SchemaVersion,
    pub subject_id: StableId,
    pub executor: ArtifactReference,
    pub role_bundle: ArtifactReference,
    pub runtime_bundle: ArtifactReference,
    pub scenario_catalog: ArtifactReference,
    pub source_artifacts: Vec<ArtifactReference>,
    pub schema_registry: ArtifactReference,
    pub conformance_corpus: ArtifactReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BuildProvenance {
    pub generator: ArtifactReference,
    pub dependency_lock: ArtifactReference,
    pub workflow_pins: Vec<ArtifactReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExactLock {
    pub schema_version: SchemaVersion,
    pub package_lock_id: Sha256Digest,
    pub normative: NormativeLockClosure,
    pub build_provenance: BuildProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AttemptState {
    NotEvaluated,
    Running,
    Held,
    Cancelled,
    PolicyViolated,
    Failed,
    RecoveryRequired,
    Succeeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AttemptEvent {
    pub schema_version: SchemaVersion,
    pub sequence: String,
    pub occurred_at_utc: String,
    pub state: AttemptState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AttemptCheckpoint {
    pub schema_version: SchemaVersion,
    pub sequence: String,
    pub state: AttemptState,
    pub artifact: ArtifactReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AttemptRecord {
    pub schema_version: SchemaVersion,
    pub attempt_id: StableId,
    pub capability_id: StableId,
    pub events: Vec<AttemptEvent>,
    pub checkpoint: Option<AttemptCheckpoint>,
    pub outcome: AttemptState,
    pub evidence: Vec<ArtifactReference>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeLifecycleState {
    Disabled,
    Shadow,
    Active,
    Draining,
    Quarantined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeInstallationState {
    pub schema_version: SchemaVersion,
    pub installation_id: StableId,
    pub state: RuntimeLifecycleState,
    pub package_lock_id: Sha256Digest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StateMigrationHandoff {
    pub schema_version: SchemaVersion,
    pub handoff_id: StableId,
    pub from: ArtifactReference,
    pub to: ArtifactReference,
    pub evidence: Vec<ArtifactReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VersionSetComponent {
    pub component_id: StableId,
    pub required: bool,
    pub artifact: Option<ArtifactReference>,
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VersionSetFixtureInput {
    pub schema_version: SchemaVersion,
    pub fixture_only: bool,
    pub package_lock_id: Sha256Digest,
    pub package: ArtifactReference,
    pub components: Vec<VersionSetComponent>,
}

pub fn schema_catalog() -> BTreeMap<String, Value> {
    let mut schemas = BTreeMap::new();
    insert_schema::<ArtifactReference>(&mut schemas, "artifact-reference");
    insert_schema::<AuditAssignment>(&mut schemas, "audit-assignment");
    insert_schema::<AuditSuccessPayload>(&mut schemas, "audit-success-payload");
    insert_schema::<AttemptCheckpoint>(&mut schemas, "attempt-checkpoint");
    insert_schema::<AttemptEvent>(&mut schemas, "attempt-event");
    insert_schema::<AttemptRecord>(&mut schemas, "attempt-record");
    insert_schema::<CapabilityContract>(&mut schemas, "capability-contract");
    insert_schema::<DiscoveryPolicy>(&mut schemas, "discovery-policy");
    insert_schema::<ExactLock>(&mut schemas, "exact-lock");
    insert_schema::<ExecutorDescriptor>(&mut schemas, "executor-descriptor");
    insert_schema::<ImplementationEvidence>(&mut schemas, "implementation-evidence");
    insert_schema::<ImplementationSubjectManifest>(&mut schemas, "implementation-subject-manifest");
    insert_schema::<MigrationLedger>(&mut schemas, "migration-ledger");
    insert_schema::<PackageManifest>(&mut schemas, "package-manifest");
    insert_schema::<RuntimeInstallationState>(&mut schemas, "runtime-installation-state");
    insert_schema::<RuntimeBundleManifest>(&mut schemas, "runtime-bundle-manifest");
    insert_schema::<ScenarioCatalog>(&mut schemas, "scenario-catalog");
    insert_schema::<StateMigrationHandoff>(&mut schemas, "state-migration-handoff");
    insert_schema::<VersionSetFixtureInput>(&mut schemas, "version-set-fixture-input");
    insert_schema::<WorkerResult>(&mut schemas, "worker-result");
    insert_schema::<WorkerRoleBundle>(&mut schemas, "worker-role-bundle");
    insert_schema::<WorkerRoleContract>(&mut schemas, "worker-role-contract");
    for name in ["capability-contract", "worker-role-contract"] {
        strip_bounded_evidence_extension(
            schemas
                .get_mut(name)
                .expect("implementation schema is present"),
        );
    }
    schemas
}

pub fn bounded_evidence_schema_catalog() -> BTreeMap<String, Value> {
    let mut schemas = BTreeMap::new();
    insert_schema::<BoundedComparisonEvidence>(&mut schemas, "bounded-comparison-evidence");
    insert_schema::<BoundedEvidenceReference>(&mut schemas, "bounded-evidence-reference");
    insert_schema::<CapabilityContract>(&mut schemas, "capability-contract-with-bounded-evidence");
    insert_schema::<WorkerRoleContract>(&mut schemas, "worker-role-contract-with-bounded-evidence");
    schemas
}

fn strip_bounded_evidence_extension(schema: &mut Value) {
    schema
        .get_mut("properties")
        .and_then(Value::as_object_mut)
        .expect("contract schema properties")
        .remove("bounded_evidence");
    if let Some(definitions) = schema.get_mut("$defs").and_then(Value::as_object_mut) {
        definitions.remove("BoundedEvidenceReference");
    }
}

fn insert_schema<T: JsonSchema>(schemas: &mut BTreeMap<String, Value>, name: &str) {
    let settings = SchemaSettings::draft2020_12().with(|settings| {
        settings.contract = Contract::Deserialize;
    });
    let schema = settings.into_generator().into_root_schema_for::<T>();
    schemas.insert(
        name.to_owned(),
        serde_json::to_value(schema).expect("schema serialization is infallible"),
    );
}
