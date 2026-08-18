use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use repository_engineering_runtime::model::{
    AcceptedResultCapsule, ArtifactReference, AuditRecord, AuditSuccessPayload, AuditVerdict,
    DispatchIntent, WorkerResult,
};
use repository_engineering_runtime::worker_host::{HostRecovery, WorkerHost};
use sha2::{Digest, Sha256};
use tokio::sync::Notify;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptedHostError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryMode {
    NeverStarted,
    Running,
    Terminal,
    Unknown,
}

#[derive(Debug, Default)]
struct State {
    active: AtomicUsize,
    maximum_active: AtomicUsize,
    started: AtomicUsize,
    receipts: AtomicUsize,
    cancel_calls: AtomicUsize,
    cancelled: AtomicBool,
    changed: Notify,
}

#[derive(Debug, Clone)]
pub struct ScriptedHost {
    state: Arc<State>,
    delay: Duration,
    failing_assignment: Option<String>,
    recovery_mode: RecoveryMode,
    verdict: AuditVerdict,
    failing_cancel: bool,
}

impl ScriptedHost {
    pub fn new(delay: Duration) -> Self {
        Self {
            state: Arc::new(State::default()),
            delay,
            failing_assignment: None,
            recovery_mode: RecoveryMode::NeverStarted,
            verdict: AuditVerdict::Confirmed,
            failing_cancel: false,
        }
    }

    pub fn recovering_as(mut self, mode: RecoveryMode) -> Self {
        self.recovery_mode = mode;
        self
    }

    pub fn failing(mut self, assignment_id: &str) -> Self {
        self.failing_assignment = Some(assignment_id.to_owned());
        self
    }

    pub fn with_verdict(mut self, verdict: AuditVerdict) -> Self {
        self.verdict = verdict;
        self
    }

    pub fn failing_cancel(mut self) -> Self {
        self.failing_cancel = true;
        self
    }

    pub fn active(&self) -> usize {
        self.state.active.load(Ordering::SeqCst)
    }

    pub fn maximum_active(&self) -> usize {
        self.state.maximum_active.load(Ordering::SeqCst)
    }

    pub fn cancel_calls(&self) -> usize {
        self.state.cancel_calls.load(Ordering::SeqCst)
    }

    pub async fn wait_for_started(&self, count: usize) {
        while self.state.started.load(Ordering::SeqCst) < count {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
}

struct ActiveGuard(Arc<State>);

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
    }
}

impl WorkerHost for ScriptedHost {
    type Error = ScriptedHostError;

    async fn invoke(&self, intent: DispatchIntent) -> Result<AcceptedResultCapsule, Self::Error> {
        let active = self.state.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.state
            .maximum_active
            .fetch_max(active, Ordering::SeqCst);
        self.state.started.fetch_add(1, Ordering::SeqCst);
        let _guard = ActiveGuard(self.state.clone());

        tokio::select! {
            () = tokio::time::sleep(self.delay) => {}
            () = self.state.changed.notified() => {}
        }
        if self.failing_assignment.as_deref() == Some(intent.assignment_id.as_str()) {
            return Err(ScriptedHostError);
        }
        let receipt_number = self.state.receipts.fetch_add(1, Ordering::SeqCst);
        Ok(capsule(
            &intent,
            self.state.cancelled.load(Ordering::SeqCst),
            receipt_number,
            self.verdict,
        ))
    }

    async fn recover(&self, intent: DispatchIntent) -> Result<HostRecovery, Self::Error> {
        Ok(match self.recovery_mode {
            RecoveryMode::NeverStarted => HostRecovery::NeverStarted,
            RecoveryMode::Running => HostRecovery::Running,
            RecoveryMode::Terminal => HostRecovery::Terminal(Box::new(capsule(
                &intent,
                false,
                self.state.receipts.fetch_add(1, Ordering::SeqCst),
                self.verdict,
            ))),
            RecoveryMode::Unknown => HostRecovery::Unknown,
        })
    }

    async fn await_terminal(
        &self,
        intent: DispatchIntent,
    ) -> Result<AcceptedResultCapsule, Self::Error> {
        self.invoke(intent).await
    }

    async fn cancel_and_reap(&self, _invocation_id: String) -> Result<(), Self::Error> {
        self.state.cancel_calls.fetch_add(1, Ordering::SeqCst);
        self.state.cancelled.store(true, Ordering::SeqCst);
        self.state.changed.notify_one();
        if self.failing_cancel {
            Err(ScriptedHostError)
        } else {
            Ok(())
        }
    }
}

fn capsule(
    intent: &DispatchIntent,
    cancelled: bool,
    receipt_number: usize,
    verdict: AuditVerdict,
) -> AcceptedResultCapsule {
    let receipt_bytes = format!("{}:{receipt_number}", intent.invocation_id).into_bytes();
    let receipt = ArtifactReference {
        schema_version: "v0".to_owned(),
        path: format!("receipts/{}.json", intent.invocation_id),
        sha256: digest(&receipt_bytes),
        media_type: "application/json".to_owned(),
    };
    if cancelled {
        return AcceptedResultCapsule {
            schema_version: "v0".to_owned(),
            result: WorkerResult::Cancelled {
                schema_version: "v0".to_owned(),
                attempt_id: intent.attempt_id.clone(),
                invocation_id: intent.invocation_id.clone(),
                assignment_id: intent.assignment_id.clone(),
                worker_instance_id: intent.worker_instance_id.clone(),
                worker_instance_receipt: receipt,
                reason: "cancelled_by_fence".to_owned(),
            },
            record_bytes: None,
            worker_instance_receipt_bytes: receipt_bytes,
        };
    }

    let record_bytes = serde_json::to_vec(&AuditRecord {
        schema_version: "v0".to_owned(),
        row_id: intent.row_id.clone(),
        verdict,
    })
    .expect("record");
    let result = WorkerResult::Succeeded {
        schema_version: "v0".to_owned(),
        attempt_id: intent.attempt_id.clone(),
        invocation_id: intent.invocation_id.clone(),
        assignment_id: intent.assignment_id.clone(),
        worker_instance_id: intent.worker_instance_id.clone(),
        worker_instance_receipt: receipt,
        payload: AuditSuccessPayload {
            row_id: intent.row_id.clone(),
            verdict,
            record: ArtifactReference {
                schema_version: "v0".to_owned(),
                path: format!("records/{}.json", intent.row_id),
                sha256: digest(&record_bytes),
                media_type: "application/json".to_owned(),
            },
        },
    };
    AcceptedResultCapsule {
        schema_version: "v0".to_owned(),
        result,
        record_bytes: Some(record_bytes),
        worker_instance_receipt_bytes: receipt_bytes,
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
