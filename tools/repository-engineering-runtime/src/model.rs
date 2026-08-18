use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactReference {
    pub schema_version: String,
    pub path: String,
    pub sha256: String,
    pub media_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditVerdict {
    Confirmed,
    Refuted,
    Unverifiable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditRecord {
    pub schema_version: String,
    pub row_id: String,
    pub verdict: AuditVerdict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditSuccessPayload {
    pub row_id: String,
    pub verdict: AuditVerdict,
    pub record: ArtifactReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerResult {
    Succeeded {
        schema_version: String,
        attempt_id: String,
        invocation_id: String,
        assignment_id: String,
        worker_instance_id: String,
        worker_instance_receipt: ArtifactReference,
        payload: AuditSuccessPayload,
    },
    Held {
        schema_version: String,
        attempt_id: String,
        invocation_id: String,
        assignment_id: String,
        worker_instance_id: String,
        worker_instance_receipt: ArtifactReference,
        reason: String,
    },
    Cancelled {
        schema_version: String,
        attempt_id: String,
        invocation_id: String,
        assignment_id: String,
        worker_instance_id: String,
        worker_instance_receipt: ArtifactReference,
        reason: String,
    },
    PolicyViolated {
        schema_version: String,
        attempt_id: String,
        invocation_id: String,
        assignment_id: String,
        worker_instance_id: String,
        worker_instance_receipt: ArtifactReference,
        policy_id: String,
    },
    Failed {
        schema_version: String,
        attempt_id: String,
        invocation_id: String,
        assignment_id: String,
        worker_instance_id: String,
        worker_instance_receipt: ArtifactReference,
        error_code: String,
    },
    RecoveryRequired {
        schema_version: String,
        attempt_id: String,
        invocation_id: String,
        assignment_id: String,
        worker_instance_id: String,
        worker_instance_receipt: ArtifactReference,
        checkpoint: ArtifactReference,
    },
}

impl WorkerResult {
    pub(crate) fn common(&self) -> (&str, &str, &str, &str, &ArtifactReference) {
        match self {
            Self::Succeeded {
                attempt_id,
                invocation_id,
                assignment_id,
                worker_instance_id,
                worker_instance_receipt,
                ..
            }
            | Self::Held {
                attempt_id,
                invocation_id,
                assignment_id,
                worker_instance_id,
                worker_instance_receipt,
                ..
            }
            | Self::Cancelled {
                attempt_id,
                invocation_id,
                assignment_id,
                worker_instance_id,
                worker_instance_receipt,
                ..
            }
            | Self::PolicyViolated {
                attempt_id,
                invocation_id,
                assignment_id,
                worker_instance_id,
                worker_instance_receipt,
                ..
            }
            | Self::Failed {
                attempt_id,
                invocation_id,
                assignment_id,
                worker_instance_id,
                worker_instance_receipt,
                ..
            }
            | Self::RecoveryRequired {
                attempt_id,
                invocation_id,
                assignment_id,
                worker_instance_id,
                worker_instance_receipt,
                ..
            } => (
                attempt_id,
                invocation_id,
                assignment_id,
                worker_instance_id,
                worker_instance_receipt,
            ),
        }
    }

    pub(crate) fn success_payload(&self) -> Option<&AuditSuccessPayload> {
        match self {
            Self::Succeeded { payload, .. } => Some(payload),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowInput {
    pub row_id: String,
    pub source_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRequest {
    pub schema_version: String,
    pub attempt_id: String,
    pub parent_attempt_id: Option<String>,
    pub idempotency_key: String,
    pub package_lock_digest: String,
    pub implementation_subject_digest: String,
    pub capability_contract_digest: String,
    pub executor_digest: String,
    pub scenario_digest: String,
    pub repository_snapshot_digest: String,
    pub row_manifest_digest: String,
    pub base_ledger_digest: String,
    pub rows: Vec<RowInput>,
    pub global_concurrency_limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Discovering,
    Dispatching,
    RollingUp,
    GateComputed,
    Complete,
    RecoveryRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchIntent {
    pub attempt_id: String,
    pub invocation_id: String,
    pub assignment_id: String,
    pub row_id: String,
    pub worker_instance_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointRow {
    pub row_id: String,
    pub source_available: bool,
    pub dispatch_intent: Option<DispatchIntent>,
    pub result_capsule: Option<ArtifactReference>,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectEntry {
    pub schema_version: String,
    pub effect_id: String,
    pub relative_target: String,
    pub expected_before_digest: Option<String>,
    pub after_bytes: Vec<u8>,
    pub after_digest: String,
    pub base_ledger_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointGeneration {
    pub schema_version: String,
    pub attempt_id: String,
    pub parent_attempt_id: Option<String>,
    pub sequence: u64,
    pub phase: Phase,
    pub parent_generation_digest: Option<String>,
    pub package_lock_digest: String,
    pub implementation_subject_digest: String,
    pub capability_contract_digest: String,
    pub executor_digest: String,
    pub scenario_digest: String,
    pub repository_snapshot_digest: String,
    pub row_manifest_digest: String,
    pub base_ledger_digest: String,
    pub rows: Vec<CheckpointRow>,
    pub cancellation_fence: Option<u64>,
    pub prepared_effects: Vec<EffectEntry>,
    pub applied_effect_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointHead {
    pub schema_version: String,
    pub attempt_id: String,
    pub sequence: u64,
    pub generation_digest: String,
    pub parent_generation_digest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalOutcome {
    Succeeded,
    Held,
    Cancelled,
    RecoveryRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalRow {
    pub row_id: String,
    pub source_available: bool,
    pub verdict: Option<AuditVerdict>,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalRecord {
    pub schema_version: String,
    pub attempt_id: String,
    pub outcome: TerminalOutcome,
    pub rows: Vec<TerminalRow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineError {
    InvalidRequest,
    InvalidLimit,
    DuplicateRow,
    InvalidPhase,
    CorrelationMismatch,
    RecordMissing,
    RecordInvalid,
    RecordTooLarge,
    RecordDigestMismatch,
    ReceiptReused,
    ConflictingReplay,
    BaseLedgerDrift,
}

impl std::fmt::Display for MachineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for MachineError {}

pub(crate) fn valid_id(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

pub(crate) fn valid_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
