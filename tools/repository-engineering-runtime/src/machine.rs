use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};

use serde::Serialize;

use crate::model::{
    valid_digest, valid_id, AcceptedResultCapsule, ArtifactReference, AuditRecord, AuditVerdict,
    CheckpointGeneration, CheckpointRow, DispatchIntent, EffectEntry, MachineError, Phase,
    RunRequest, TerminalOutcome, TerminalRecord, TerminalRow,
};

const MAX_RECORD_BYTES: usize = 64 * 1024;
const MAX_RECEIPT_BYTES: usize = 64 * 1024;
const MAX_CAPSULE_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone)]
enum RowStatus {
    Pending,
    Running(DispatchIntent),
    Terminal(AcceptedTerminal),
}

#[derive(Debug, Clone)]
struct AcceptedTerminal {
    capsule_bytes: Vec<u8>,
    verdict: Option<AuditVerdict>,
    completed: bool,
}

#[derive(Debug, Clone)]
struct RuntimeRow {
    row_id: String,
    source_available: bool,
    status: RowStatus,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct RollUpReport {
    schema_version: String,
    attempt_id: String,
    implementation_subject_digest: String,
    base_ledger_digest: String,
    rows: Vec<TerminalRow>,
}

#[derive(Debug, Clone)]
pub struct SweepMachine {
    request: RunRequest,
    phase: Phase,
    rows: Vec<RuntimeRow>,
    accepted_receipts: BTreeSet<String>,
}

impl SweepMachine {
    pub fn new(request: RunRequest) -> Result<Self, MachineError> {
        validate_request(&request)?;
        let mut seen = BTreeSet::new();
        let mut rows = Vec::with_capacity(request.rows.len());
        for row in &request.rows {
            if !valid_id(&row.row_id) || !seen.insert(row.row_id.clone()) {
                return Err(if valid_id(&row.row_id) {
                    MachineError::DuplicateRow
                } else {
                    MachineError::InvalidRequest
                });
            }
            rows.push(RuntimeRow {
                row_id: row.row_id.clone(),
                source_available: row.source_available,
                status: RowStatus::Pending,
            });
        }
        Ok(Self {
            request,
            phase: Phase::Discovering,
            rows,
            accepted_receipts: BTreeSet::new(),
        })
    }

    pub fn phase(&self) -> Phase {
        self.phase.clone()
    }

    pub fn begin_dispatch(&mut self) -> Result<(), MachineError> {
        if self.phase != Phase::Discovering {
            return Err(MachineError::InvalidPhase);
        }
        self.phase = Phase::Dispatching;
        Ok(())
    }

    pub fn request_dispatches(&mut self) -> Result<Vec<DispatchIntent>, MachineError> {
        if self.phase != Phase::Dispatching {
            return Err(MachineError::InvalidPhase);
        }
        let effective_limit = self.request.global_concurrency_limit.min(2);
        let running = self
            .rows
            .iter()
            .filter(|row| matches!(row.status, RowStatus::Running(_)))
            .count();
        let available = effective_limit.saturating_sub(running);
        let mut intents = Vec::with_capacity(available);
        for row in self
            .rows
            .iter_mut()
            .filter(|row| matches!(row.status, RowStatus::Pending))
            .take(available)
        {
            let intent = dispatch_intent(&self.request, &row.row_id);
            row.status = RowStatus::Running(intent.clone());
            intents.push(intent);
        }
        Ok(intents)
    }

    pub fn accept_capsule(&mut self, capsule: &AcceptedResultCapsule) -> Result<(), MachineError> {
        if capsule.schema_version != "v0"
            || capsule.worker_instance_receipt_bytes.is_empty()
            || capsule.worker_instance_receipt_bytes.len() > MAX_RECEIPT_BYTES
        {
            return Err(self.recovery(MachineError::RecordInvalid));
        }
        let capsule_bytes =
            serde_json::to_vec(capsule).map_err(|_| self.recovery(MachineError::RecordInvalid))?;
        if capsule_bytes.len() > MAX_CAPSULE_BYTES {
            return Err(self.recovery(MachineError::RecordTooLarge));
        }
        let result = &capsule.result;
        let (attempt_id, invocation_id, assignment_id, worker_instance_id, receipt) =
            result.common();
        if result.schema_version() != "v0" || !valid_artifact_reference(receipt) {
            return Err(self.recovery(MachineError::RecordInvalid));
        }
        if digest_bytes(&capsule.worker_instance_receipt_bytes) != receipt.sha256 {
            return Err(self.recovery(MachineError::RecordDigestMismatch));
        }
        let row_index = self
            .rows
            .iter()
            .position(|row| row.row_id == assignment_id)
            .ok_or_else(|| self.recovery(MachineError::CorrelationMismatch))?;

        if let RowStatus::Terminal(accepted) = &self.rows[row_index].status {
            if accepted.capsule_bytes == capsule_bytes {
                return Ok(());
            }
            return Err(self.recovery(MachineError::ConflictingReplay));
        }
        if self.phase != Phase::Dispatching {
            return Err(self.recovery(MachineError::InvalidPhase));
        }
        let intent = match &self.rows[row_index].status {
            RowStatus::Running(intent) => intent.clone(),
            RowStatus::Pending | RowStatus::Terminal(_) => {
                return Err(self.recovery(MachineError::CorrelationMismatch));
            }
        };
        if attempt_id != intent.attempt_id
            || invocation_id != intent.invocation_id
            || assignment_id != intent.assignment_id
            || worker_instance_id != intent.worker_instance_id
        {
            return Err(self.recovery(MachineError::CorrelationMismatch));
        }
        if self.accepted_receipts.contains(&receipt.sha256) {
            return Err(self.recovery(MachineError::ReceiptReused));
        }

        let (verdict, completed) = if let Some(payload) = result.success_payload() {
            let bytes = capsule
                .record_bytes
                .as_deref()
                .ok_or_else(|| self.recovery(MachineError::RecordMissing))?;
            if bytes.len() > MAX_RECORD_BYTES {
                return Err(self.recovery(MachineError::RecordTooLarge));
            }
            if payload.row_id != intent.row_id || !valid_artifact_reference(&payload.record) {
                return Err(self.recovery(MachineError::CorrelationMismatch));
            }
            let record: AuditRecord = serde_json::from_slice(bytes)
                .map_err(|_| self.recovery(MachineError::RecordInvalid))?;
            if record.schema_version != "v0"
                || record.row_id != intent.row_id
                || record.verdict != payload.verdict
            {
                return Err(self.recovery(MachineError::CorrelationMismatch));
            }
            let digest = digest_bytes(bytes);
            if payload.record.sha256 != digest {
                return Err(self.recovery(MachineError::RecordDigestMismatch));
            }
            (Some(payload.verdict), true)
        } else {
            if capsule.record_bytes.is_some() {
                return Err(self.recovery(MachineError::RecordInvalid));
            }
            (None, false)
        };

        self.accepted_receipts.insert(receipt.sha256.clone());
        self.rows[row_index].status = RowStatus::Terminal(AcceptedTerminal {
            capsule_bytes,
            verdict,
            completed,
        });
        if self.all_invocations_terminal() {
            self.phase = Phase::RollingUp;
        }
        Ok(())
    }

    pub fn checkpoint_rows(
        &self,
        capsules: &BTreeMap<String, ArtifactReference>,
    ) -> Result<Vec<CheckpointRow>, MachineError> {
        self.rows
            .iter()
            .map(|row| {
                let (dispatch_intent, completed, needs_capsule) = match &row.status {
                    RowStatus::Pending => (None, false, false),
                    RowStatus::Running(intent) => (Some(intent.clone()), false, false),
                    RowStatus::Terminal(terminal) => {
                        let intent = dispatch_intent(&self.request, &row.row_id);
                        (Some(intent), terminal.completed, true)
                    }
                };
                let result_capsule = capsules.get(&row.row_id).cloned();
                if needs_capsule && result_capsule.is_none() {
                    return Err(MachineError::RecordMissing);
                }
                Ok(CheckpointRow {
                    row_id: row.row_id.clone(),
                    source_available: row.source_available,
                    dispatch_intent,
                    result_capsule,
                    completed,
                })
            })
            .collect()
    }

    pub fn request(&self) -> &RunRequest {
        &self.request
    }

    pub fn restore(
        request: RunRequest,
        checkpoint: &CheckpointGeneration,
        capsules: &BTreeMap<String, AcceptedResultCapsule>,
    ) -> Result<Self, MachineError> {
        if checkpoint.attempt_id != request.attempt_id
            || checkpoint.parent_attempt_id != request.parent_attempt_id
            || checkpoint.package_lock_digest != request.package_lock_digest
            || checkpoint.implementation_subject_digest != request.implementation_subject_digest
            || checkpoint.capability_contract_digest != request.capability_contract_digest
            || checkpoint.worker_role_digest != request.worker_role_digest
            || checkpoint.executor_digest != request.executor_digest
            || checkpoint.scenario_digest != request.scenario_digest
            || checkpoint.repository_snapshot_digest != request.repository_snapshot_digest
            || checkpoint.row_manifest_digest != request.row_manifest_digest
            || checkpoint.base_ledger_digest != request.base_ledger_digest
            || checkpoint.output_root_id != request.output_root_id
            || checkpoint.rows.len() != request.rows.len()
        {
            return Err(MachineError::InvalidRequest);
        }
        let mut machine = Self::new(request)?;
        for (index, persisted) in checkpoint.rows.iter().enumerate() {
            let runtime = &machine.rows[index];
            if persisted.row_id != runtime.row_id
                || persisted.source_available != runtime.source_available
            {
                return Err(MachineError::InvalidRequest);
            }
        }
        if checkpoint.phase == Phase::Discovering {
            if checkpoint.rows.iter().any(|row| {
                row.dispatch_intent.is_some() || row.result_capsule.is_some() || row.completed
            }) || !capsules.is_empty()
            {
                return Err(MachineError::InvalidRequest);
            }
            return Ok(machine);
        }

        machine.phase = Phase::Dispatching;
        for (index, persisted) in checkpoint.rows.iter().enumerate() {
            let Some(intent) = &persisted.dispatch_intent else {
                if persisted.result_capsule.is_some() || persisted.completed {
                    return Err(MachineError::InvalidRequest);
                }
                continue;
            };
            if *intent != dispatch_intent(&machine.request, &persisted.row_id) {
                return Err(MachineError::CorrelationMismatch);
            }
            machine.rows[index].status = RowStatus::Running(intent.clone());
            if persisted.result_capsule.is_some() {
                let capsule = capsules
                    .get(&persisted.row_id)
                    .ok_or(MachineError::RecordMissing)?;
                machine.accept_capsule(capsule)?;
                if !matches!(
                    &machine.rows[index].status,
                    RowStatus::Terminal(terminal) if terminal.completed == persisted.completed
                ) {
                    return Err(MachineError::RecordInvalid);
                }
            }
        }
        let referenced_capsules = checkpoint
            .rows
            .iter()
            .filter_map(|row| row.result_capsule.as_ref().map(|_| row.row_id.as_str()))
            .collect::<BTreeSet<_>>();
        if capsules
            .keys()
            .any(|row_id| !referenced_capsules.contains(row_id.as_str()))
        {
            return Err(MachineError::RecordInvalid);
        }

        match checkpoint.phase {
            Phase::Dispatching if machine.phase == Phase::Dispatching => {}
            Phase::RollingUp if machine.phase == Phase::RollingUp => {}
            Phase::GateComputed if machine.phase == Phase::RollingUp => {
                machine.phase = Phase::GateComputed;
            }
            Phase::Complete if machine.phase == Phase::RollingUp => machine.phase = Phase::Complete,
            Phase::Cancelling | Phase::Cancelled | Phase::RecoveryRequired => {
                machine.phase = checkpoint.phase.clone();
            }
            _ => return Err(MachineError::InvalidPhase),
        }
        Ok(machine)
    }

    pub fn running_intents(&self) -> Vec<DispatchIntent> {
        self.rows
            .iter()
            .filter_map(|row| match &row.status {
                RowStatus::Running(intent) => Some(intent.clone()),
                RowStatus::Pending | RowStatus::Terminal(_) => None,
            })
            .collect()
    }

    pub fn cancel(&mut self) -> Result<(), MachineError> {
        if !matches!(
            self.phase,
            Phase::Discovering | Phase::Dispatching | Phase::RollingUp
        ) {
            return Err(MachineError::InvalidPhase);
        }
        self.phase = Phase::Cancelling;
        Ok(())
    }

    pub fn finish_cancel(&mut self) -> Result<TerminalRecord, MachineError> {
        if self.phase != Phase::Cancelling {
            return Err(MachineError::InvalidPhase);
        }
        self.phase = Phase::Cancelled;
        Ok(TerminalRecord {
            schema_version: "v0".to_owned(),
            attempt_id: self.request.attempt_id.clone(),
            outcome: TerminalOutcome::Cancelled,
            rows: self.terminal_rows(),
        })
    }

    pub fn terminal_rows(&self) -> Vec<TerminalRow> {
        self.rows
            .iter()
            .map(|row| match &row.status {
                RowStatus::Terminal(terminal) => TerminalRow {
                    row_id: row.row_id.clone(),
                    source_available: row.source_available,
                    verdict: terminal.verdict,
                    completed: terminal.completed,
                },
                RowStatus::Pending | RowStatus::Running(_) => TerminalRow {
                    row_id: row.row_id.clone(),
                    source_available: row.source_available,
                    verdict: None,
                    completed: false,
                },
            })
            .collect()
    }

    pub fn current_terminal_record(&self) -> Option<TerminalRecord> {
        let rows = self.terminal_rows();
        let outcome = match self.phase {
            Phase::Complete => capability_outcome(&rows),
            Phase::Cancelled => TerminalOutcome::Cancelled,
            _ => return None,
        };
        Some(TerminalRecord {
            schema_version: "v0".to_owned(),
            attempt_id: self.request.attempt_id.clone(),
            outcome,
            rows,
        })
    }

    pub fn require_recovery(&mut self) {
        self.phase = Phase::RecoveryRequired;
    }

    pub fn all_invocations_terminal(&self) -> bool {
        self.rows
            .iter()
            .all(|row| matches!(row.status, RowStatus::Terminal(_)))
    }

    pub fn finish_roll_up(&mut self, observed_base_digest: &str) -> Result<(), MachineError> {
        if self.phase != Phase::RollingUp {
            return Err(MachineError::InvalidPhase);
        }
        if observed_base_digest != self.request.base_ledger_digest {
            return Err(self.recovery(MachineError::BaseLedgerDrift));
        }
        self.phase = Phase::GateComputed;
        Ok(())
    }

    pub fn prepare_roll_up_effects(&self) -> Result<Vec<EffectEntry>, MachineError> {
        if self.phase != Phase::RollingUp {
            return Err(MachineError::InvalidPhase);
        }
        let report = RollUpReport {
            schema_version: "v0".to_owned(),
            attempt_id: self.request.attempt_id.clone(),
            implementation_subject_digest: self.request.implementation_subject_digest.clone(),
            base_ledger_digest: self.request.base_ledger_digest.clone(),
            rows: self.terminal_rows(),
        };
        let after_bytes = serde_json::to_vec(&report).map_err(|_| MachineError::RecordInvalid)?;
        let effect_key = format!("{}:roll-up-report", self.request.attempt_id);
        Ok(vec![EffectEntry {
            schema_version: "v0".to_owned(),
            effect_id: format!("roll-up-report-{:x}", Sha256::digest(effect_key.as_bytes())),
            relative_target: format!("reports/{}.json", self.request.attempt_id),
            expected_before_digest: None,
            after_digest: digest_bytes(&after_bytes),
            after_bytes,
            base_ledger_digest: self.request.base_ledger_digest.clone(),
        }])
    }

    pub fn complete(&mut self) -> Result<TerminalRecord, MachineError> {
        if self.phase != Phase::GateComputed {
            return Err(MachineError::InvalidPhase);
        }
        let rows = self.terminal_rows();
        self.phase = Phase::Complete;
        Ok(TerminalRecord {
            schema_version: "v0".to_owned(),
            attempt_id: self.request.attempt_id.clone(),
            outcome: capability_outcome(&rows),
            rows,
        })
    }

    fn recovery(&mut self, error: MachineError) -> MachineError {
        self.phase = Phase::RecoveryRequired;
        error
    }
}

fn validate_request(request: &RunRequest) -> Result<(), MachineError> {
    if request.global_concurrency_limit == 0 {
        return Err(MachineError::InvalidLimit);
    }
    if request.schema_version != "v0"
        || request.rows.is_empty()
        || !valid_id(&request.attempt_id)
        || !valid_id(&request.idempotency_key)
        || !valid_id(&request.output_root_id)
        || request
            .parent_attempt_id
            .as_deref()
            .is_some_and(|value| !valid_id(value))
        || [
            &request.package_lock_digest,
            &request.implementation_subject_digest,
            &request.capability_contract_digest,
            &request.worker_role_digest,
            &request.executor_digest,
            &request.scenario_digest,
            &request.repository_snapshot_digest,
            &request.row_manifest_digest,
            &request.base_ledger_digest,
        ]
        .into_iter()
        .any(|digest| !valid_digest(digest))
    {
        return Err(MachineError::InvalidRequest);
    }
    Ok(())
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn dispatch_intent(request: &RunRequest, row_id: &str) -> DispatchIntent {
    DispatchIntent {
        schema_version: "v0".to_owned(),
        attempt_id: request.attempt_id.clone(),
        invocation_id: format!("{}-{row_id}", request.attempt_id),
        assignment_id: row_id.to_owned(),
        row_id: row_id.to_owned(),
        idempotency_key: format!("{}-{row_id}", request.attempt_id),
        worker_instance_id: format!("worker-{}-{row_id}", request.attempt_id),
    }
}

fn capability_outcome(rows: &[TerminalRow]) -> TerminalOutcome {
    if rows.iter().all(|row| {
        row.completed && row.source_available && !matches!(row.verdict, Some(AuditVerdict::Refuted))
    }) {
        TerminalOutcome::Succeeded
    } else {
        TerminalOutcome::Held
    }
}

fn valid_artifact_reference(reference: &ArtifactReference) -> bool {
    reference.schema_version == "v0"
        && valid_digest(&reference.sha256)
        && !reference.media_type.is_empty()
        && reference.media_type.len() <= 128
        && !reference.path.is_empty()
        && reference.path.len() <= 1024
        && !reference.path.starts_with('/')
        && !reference.path.contains('\\')
        && reference
            .path
            .split('/')
            .all(|segment| !segment.is_empty() && !matches!(segment, "." | ".."))
}
