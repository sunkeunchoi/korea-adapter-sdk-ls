use serde::Deserialize;

use crate::bundle::LoadedBundle;
use crate::model::{valid_digest, valid_id, ArtifactReference, DispatchIntent, WorkerResult};

const EXECUTOR_PATH: &str = ".repository-engineering/executors/audit-carried-rows.toml";
const ROLE_PATH: &str = ".repository-engineering/roles/decommission-row-auditor.toml";
const SCENARIO_PATH: &str =
    ".repository-engineering/scenarios/audit-carried-rows/implementation.toml";
const RUNTIME_VECTORS_PATH: &str = ".repository-engineering/conformance/v0/runtime-semantics.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedContract {
    pub scenario_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractError {
    MissingMember,
    InvalidDescriptor,
    InvalidVectorCorpus,
    VectorExpectationMismatch,
}

impl std::fmt::Display for ContractError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ContractError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecutorDescriptor {
    schema_version: String,
    executor_id: String,
    capability_id: String,
    worker_role_id: String,
    phases: Vec<String>,
    effective_concurrency_cap: u16,
    state_owner: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerRoleBundle {
    schema_version: String,
    role_id: String,
    assignment_schema: String,
    result_schema: String,
    knowledge_paths: Vec<String>,
    record_format: String,
    safety_rules: Vec<String>,
    verdicts: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioCatalog {
    schema_version: String,
    catalog_id: String,
    capability_id: String,
    positive_cases: Vec<String>,
    negative_cases: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeVectorCorpus {
    schema_version: String,
    cases: Vec<RuntimeVector>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeVector {
    case_id: String,
    kind: String,
    expected: String,
    input: serde_json::Value,
}

pub fn validate_portable_contract(
    bundle: &LoadedBundle,
) -> Result<ValidatedContract, ContractError> {
    let executor: ExecutorDescriptor = parse_toml_member(bundle, EXECUTOR_PATH)?;
    if executor.schema_version != "v0"
        || executor.executor_id != "audit-carried-rows"
        || executor.capability_id != "audit-carried-rows"
        || executor.worker_role_id != "decommission-row-auditor"
        || executor.phases
            != [
                "discovering",
                "dispatching",
                "rolling_up",
                "gate_computed",
                "complete",
            ]
        || executor.effective_concurrency_cap != 2
        || executor.state_owner != "orchestrator_only"
    {
        return Err(ContractError::InvalidDescriptor);
    }

    let role: WorkerRoleBundle = parse_toml_member(bundle, ROLE_PATH)?;
    if role.schema_version != "v0"
        || role.role_id != "decommission-row-auditor"
        || role.assignment_schema != "audit-assignment"
        || role.result_schema != "worker-result"
        || role.knowledge_paths.is_empty()
        || role.record_format != ".agents/skills/audit-carried-rows/references/record-format.md"
        || role.safety_rules.is_empty()
        || role.verdicts != ["confirmed", "refuted", "unverifiable"]
        || role
            .knowledge_paths
            .iter()
            .any(|path| bundle.member(path).is_none())
    {
        return Err(ContractError::InvalidDescriptor);
    }

    let scenario: ScenarioCatalog = parse_toml_member(bundle, SCENARIO_PATH)?;
    if scenario.schema_version != "v0"
        || scenario.catalog_id != "audit-carried-rows-implementation"
        || scenario.capability_id != "audit-carried-rows"
        || scenario.positive_cases.is_empty()
        || scenario.negative_cases.is_empty()
    {
        return Err(ContractError::InvalidDescriptor);
    }
    let mut scenario_ids = scenario.positive_cases;
    scenario_ids.extend(scenario.negative_cases);
    scenario_ids.sort();
    scenario_ids.dedup();

    let vectors: RuntimeVectorCorpus = parse_json_member(bundle, RUNTIME_VECTORS_PATH)
        .map_err(|_| ContractError::InvalidVectorCorpus)?;
    if vectors.schema_version != "v0" || vectors.cases.is_empty() {
        return Err(ContractError::InvalidVectorCorpus);
    }
    for vector in vectors.cases {
        if !valid_id(&vector.case_id) || !matches!(vector.expected.as_str(), "accept" | "reject") {
            return Err(ContractError::InvalidVectorCorpus);
        }
        let accepted = match vector.kind.as_str() {
            "audit_assignment" => validate_assignment_value(&vector.input),
            "worker_result" => validate_worker_result_value(&vector.input),
            _ => return Err(ContractError::InvalidVectorCorpus),
        };
        if accepted != (vector.expected == "accept") {
            return Err(ContractError::VectorExpectationMismatch);
        }
    }

    Ok(ValidatedContract { scenario_ids })
}

fn parse_toml_member<T: serde::de::DeserializeOwned>(
    bundle: &LoadedBundle,
    path: &str,
) -> Result<T, ContractError> {
    let bytes = bundle.member(path).ok_or(ContractError::MissingMember)?;
    let text = std::str::from_utf8(bytes).map_err(|_| ContractError::InvalidDescriptor)?;
    toml::from_str(text).map_err(|_| ContractError::InvalidDescriptor)
}

fn parse_json_member<T: serde::de::DeserializeOwned>(
    bundle: &LoadedBundle,
    path: &str,
) -> Result<T, ContractError> {
    let bytes = bundle.member(path).ok_or(ContractError::MissingMember)?;
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = T::deserialize(&mut deserializer).map_err(|_| ContractError::InvalidDescriptor)?;
    deserializer
        .end()
        .map_err(|_| ContractError::InvalidDescriptor)?;
    Ok(value)
}

fn validate_assignment_value(input: &serde_json::Value) -> bool {
    let Ok(assignment) = serde_json::from_value::<DispatchIntent>(input.clone()) else {
        return false;
    };
    validate_assignment(&assignment)
}

pub fn validate_assignment(assignment: &DispatchIntent) -> bool {
    assignment.schema_version == "v0"
        && valid_id(&assignment.attempt_id)
        && valid_id(&assignment.invocation_id)
        && valid_id(&assignment.assignment_id)
        && assignment.assignment_id == assignment.row_id
        && assignment.idempotency_key == format!("{}-{}", assignment.attempt_id, assignment.row_id)
        && valid_id(&assignment.worker_instance_id)
}

fn validate_worker_result_value(input: &serde_json::Value) -> bool {
    let Ok(result) = serde_json::from_value::<WorkerResult>(input.clone()) else {
        return false;
    };
    validate_worker_result(&result)
}

pub fn validate_worker_result(result: &WorkerResult) -> bool {
    let (attempt_id, invocation_id, assignment_id, worker_instance_id, receipt) = result.common();
    if result.schema_version() != "v0"
        || [attempt_id, invocation_id, assignment_id, worker_instance_id]
            .into_iter()
            .any(|value| !valid_id(value))
        || !valid_reference(receipt)
    {
        return false;
    }
    result
        .success_payload()
        .is_none_or(|payload| payload.row_id == assignment_id && valid_reference(&payload.record))
}

fn valid_reference(reference: &ArtifactReference) -> bool {
    reference.schema_version == "v0"
        && valid_digest(&reference.sha256)
        && !reference.media_type.is_empty()
        && !reference.path.is_empty()
        && !reference.path.starts_with('/')
        && !reference.path.contains("..")
}
