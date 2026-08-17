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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeKind {
    Succeeded,
    Held,
    Cancelled,
    PolicyViolated,
    Failed,
    RecoveryRequired,
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
    pub inputs: Vec<TypedField>,
    pub outcomes: Vec<OutcomeKind>,
    pub touched_paths: Vec<RepositoryPath>,
    pub evidence_obligations: Vec<StableId>,
    pub human_gates: Vec<StableId>,
    pub executor: Option<ArtifactReference>,
    pub knowledge_references: Vec<ArtifactReference>,
    pub worker_roles: Vec<StableId>,
    pub scenario_references: Vec<ArtifactReference>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConcurrencyClass {
    Serial,
    BoundedParallel,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerResult {
    Succeeded {
        schema_version: SchemaVersion,
        artifacts: Vec<ArtifactReference>,
    },
    Held {
        schema_version: SchemaVersion,
        reason: StableId,
    },
    Cancelled {
        schema_version: SchemaVersion,
        reason: StableId,
    },
    PolicyViolated {
        schema_version: SchemaVersion,
        policy_id: StableId,
    },
    Failed {
        schema_version: SchemaVersion,
        error_code: StableId,
    },
    RecoveryRequired {
        schema_version: SchemaVersion,
        checkpoint: ArtifactReference,
    },
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
    pub optional_components: Vec<OptionalComponent>,
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
    insert_schema::<AttemptCheckpoint>(&mut schemas, "attempt-checkpoint");
    insert_schema::<AttemptEvent>(&mut schemas, "attempt-event");
    insert_schema::<AttemptRecord>(&mut schemas, "attempt-record");
    insert_schema::<CapabilityContract>(&mut schemas, "capability-contract");
    insert_schema::<DiscoveryPolicy>(&mut schemas, "discovery-policy");
    insert_schema::<ExactLock>(&mut schemas, "exact-lock");
    insert_schema::<MigrationLedger>(&mut schemas, "migration-ledger");
    insert_schema::<PackageManifest>(&mut schemas, "package-manifest");
    insert_schema::<RuntimeInstallationState>(&mut schemas, "runtime-installation-state");
    insert_schema::<StateMigrationHandoff>(&mut schemas, "state-migration-handoff");
    insert_schema::<VersionSetFixtureInput>(&mut schemas, "version-set-fixture-input");
    insert_schema::<WorkerResult>(&mut schemas, "worker-result");
    insert_schema::<WorkerRoleContract>(&mut schemas, "worker-role-contract");
    schemas
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
