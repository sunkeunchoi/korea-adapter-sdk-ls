use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

use crate::model::{
    valid_digest, valid_id, AuditRecord, AuditVerdict, DispatchIntent, MachineError, Phase,
    RunRequest, TerminalOutcome, TerminalRecord, TerminalRow, WorkerResult,
};

const MAX_RECORD_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
enum RowStatus {
    Pending,
    Running(DispatchIntent),
    Terminal(AcceptedTerminal),
}

#[derive(Debug, Clone)]
struct AcceptedTerminal {
    result_bytes: Vec<u8>,
    record_digest: Option<String>,
    verdict: Option<AuditVerdict>,
    completed: bool,
}

#[derive(Debug, Clone)]
struct RuntimeRow {
    row_id: String,
    source_available: bool,
    status: RowStatus,
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
            let intent = DispatchIntent {
                attempt_id: self.request.attempt_id.clone(),
                invocation_id: format!("{}-{}", self.request.attempt_id, row.row_id),
                assignment_id: row.row_id.clone(),
                row_id: row.row_id.clone(),
                worker_instance_id: format!("worker-{}-{}", self.request.attempt_id, row.row_id),
            };
            row.status = RowStatus::Running(intent.clone());
            intents.push(intent);
        }
        Ok(intents)
    }

    pub fn accept_terminal(
        &mut self,
        result: WorkerResult,
        record_bytes: Option<&[u8]>,
    ) -> Result<(), MachineError> {
        let result_bytes = serde_json::to_vec(&result).map_err(|_| MachineError::RecordInvalid)?;
        let (attempt_id, invocation_id, assignment_id, worker_instance_id, receipt) =
            result.common();
        let row_index = self
            .rows
            .iter()
            .position(|row| row.row_id == assignment_id)
            .ok_or_else(|| self.recovery(MachineError::CorrelationMismatch))?;

        if let RowStatus::Terminal(accepted) = &self.rows[row_index].status {
            let replay_digest = record_bytes.map(digest_bytes);
            if accepted.result_bytes == result_bytes && accepted.record_digest == replay_digest {
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
            || receipt.schema_version != "v0"
            || !valid_digest(&receipt.sha256)
        {
            return Err(self.recovery(MachineError::CorrelationMismatch));
        }
        if self.accepted_receipts.contains(&receipt.sha256) {
            return Err(self.recovery(MachineError::ReceiptReused));
        }

        let (verdict, completed, record_digest) = if let Some(payload) = result.success_payload() {
            let bytes = record_bytes.ok_or_else(|| self.recovery(MachineError::RecordMissing))?;
            if bytes.len() > MAX_RECORD_BYTES {
                return Err(self.recovery(MachineError::RecordTooLarge));
            }
            if payload.row_id != intent.row_id {
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
            (Some(payload.verdict), true, Some(digest))
        } else {
            if record_bytes.is_some() {
                return Err(self.recovery(MachineError::RecordInvalid));
            }
            (None, false, None)
        };

        self.accepted_receipts.insert(receipt.sha256.clone());
        self.rows[row_index].status = RowStatus::Terminal(AcceptedTerminal {
            result_bytes,
            record_digest,
            verdict,
            completed,
        });
        if self.all_invocations_terminal() {
            self.phase = Phase::RollingUp;
        }
        Ok(())
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

    pub fn complete(&mut self) -> Result<TerminalRecord, MachineError> {
        if self.phase != Phase::GateComputed {
            return Err(MachineError::InvalidPhase);
        }
        let rows: Vec<_> = self
            .rows
            .iter()
            .map(|row| match &row.status {
                RowStatus::Terminal(terminal) => TerminalRow {
                    row_id: row.row_id.clone(),
                    source_available: row.source_available,
                    verdict: terminal.verdict,
                    completed: terminal.completed,
                },
                RowStatus::Pending | RowStatus::Running(_) => unreachable!("roll-up is terminal"),
            })
            .collect();
        let succeeded = rows.iter().all(|row| {
            row.completed
                && row.source_available
                && !matches!(row.verdict, Some(AuditVerdict::Refuted))
        });
        self.phase = Phase::Complete;
        Ok(TerminalRecord {
            schema_version: "v0".to_owned(),
            attempt_id: self.request.attempt_id.clone(),
            outcome: if succeeded {
                TerminalOutcome::Succeeded
            } else {
                TerminalOutcome::Held
            },
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
        || request
            .parent_attempt_id
            .as_deref()
            .is_some_and(|value| !valid_id(value))
        || [
            &request.package_lock_digest,
            &request.implementation_subject_digest,
            &request.capability_contract_digest,
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
