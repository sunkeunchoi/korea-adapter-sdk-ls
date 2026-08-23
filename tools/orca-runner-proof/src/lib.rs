use std::fmt;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const STATE_FILE: &str = "attempt.json";
const LOCK_FILE: &str = "attempt.lock";
const PINNED_ORCA_VERSION: &str = "1.4.188";
static NEXT_UNIQUE_ID: AtomicU64 = AtomicU64::new(0);

pub trait OrcaCommand {
    fn call(&mut self, args: &[String]) -> Result<Value, RunnerError>;
}

#[derive(Debug, Clone)]
pub struct ProcessOrca {
    executable: PathBuf,
}

impl ProcessOrca {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
        }
    }
}

impl OrcaCommand for ProcessOrca {
    fn call(&mut self, args: &[String]) -> Result<Value, RunnerError> {
        let output = Command::new(&self.executable)
            .args(args)
            .output()
            .map_err(|error| RunnerError::Orca(format!("could not start Orca: {error}")))?;
        let value: Value = serde_json::from_slice(&output.stdout).map_err(|error| {
            RunnerError::Orca(format!(
                "Orca returned non-JSON output (exit {:?}): {error}; stderr: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        })?;
        if !output.status.success() {
            return Err(RunnerError::OrcaReceipt {
                message: format!(
                    "Orca exited {:?}: {}",
                    output.status.code(),
                    compact_error(&value, &output.stderr)
                ),
                receipt: value,
            });
        }
        match value.get("ok").and_then(Value::as_bool) {
            Some(true) => Ok(value),
            Some(false) => Err(RunnerError::OrcaReceipt {
                message: format!(
                    "Orca command failed: {}",
                    compact_error(&value, &output.stderr)
                ),
                receipt: value,
            }),
            None => Err(RunnerError::Protocol(
                "Orca 1.4.188 receipt omitted its boolean `ok` field".into(),
            )),
        }
    }
}

fn compact_error(value: &Value, stderr: &[u8]) -> String {
    value
        .pointer("/error/message")
        .or_else(|| value.get("error"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| String::from_utf8_lossy(stderr).trim().to_owned())
}

#[derive(Debug)]
pub enum RunnerError {
    Io(String),
    InvalidArguments(String),
    Orca(String),
    OrcaReceipt { message: String, receipt: Value },
    OutcomeUnknown(String),
    PackageActive(String),
    Protocol(String),
    UnsafeStateRoot(String),
    InvalidTransition(String),
}

impl fmt::Display for RunnerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (kind, message) = match self {
            Self::Io(message) => ("I/O error", message),
            Self::InvalidArguments(message) => ("invalid arguments", message),
            Self::Orca(message) => ("Orca error", message),
            Self::OrcaReceipt { message, .. } => ("Orca error", message),
            Self::OutcomeUnknown(message) => ("mutation outcome unknown", message),
            Self::PackageActive(message) => ("plugin package is not inert", message),
            Self::Protocol(message) => ("Orca protocol error", message),
            Self::UnsafeStateRoot(message) => ("unsafe state root", message),
            Self::InvalidTransition(message) => ("invalid lifecycle transition", message),
        };
        write!(formatter, "{kind}: {message}")
    }
}

impl std::error::Error for RunnerError {}

impl RunnerError {
    fn receipt(&self) -> Option<&Value> {
        match self {
            Self::OrcaReceipt { receipt, .. } => Some(receipt),
            _ => None,
        }
    }
}

impl From<std::io::Error> for RunnerError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl From<serde_json::Error> for RunnerError {
    fn from(error: serde_json::Error) -> Self {
        Self::Protocol(error.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptPhase {
    Preparing,
    AwaitingApproval,
    Rejected,
    Running,
    Cancelled,
    Failed,
    OutcomeUnknown,
    Completed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrepareStage {
    #[default]
    Initialized,
    RunCreated,
    TaskCreated,
    GateCreated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    RunCreate,
    TaskCreate,
    GateCreate,
    WorkerStart,
    WorkerStop,
    WorkerRelease,
    WorkerRetain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    IntentPersisted,
    OutcomeUnknown,
    ReceiptCaptured,
    DefinitiveFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationIntent {
    pub kind: OperationKind,
    pub request_id: String,
    pub args: Vec<String>,
    pub status: OperationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptState {
    pub schema_version: String,
    pub repository_root: PathBuf,
    pub orca_version: String,
    pub worktree_id: String,
    #[serde(default)]
    pub prepare_stage: PrepareStage,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub gate_id: Option<String>,
    pub dispatch_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operation_history: Vec<OperationIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_operation: Option<OperationIntent>,
    pub phase: AttemptPhase,
}

impl AttemptState {
    fn new(repository_root: PathBuf, orca_version: String, worktree_id: String) -> Self {
        Self {
            schema_version: "v1".into(),
            repository_root,
            orca_version,
            worktree_id,
            prepare_stage: PrepareStage::Initialized,
            run_id: None,
            task_id: None,
            gate_id: None,
            dispatch_ids: Vec::new(),
            operation_history: Vec::new(),
            pending_operation: None,
            phase: AttemptPhase::Preparing,
        }
    }

    fn last_dispatch(&self) -> Result<&str, RunnerError> {
        self.dispatch_ids
            .last()
            .map(String::as_str)
            .ok_or_else(|| RunnerError::Protocol("attempt has no dispatch receipt".into()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ResumeOutcome {
    AwaitingApproval { gate_id: String },
    Rejected { gate_id: String },
    WorkerStarted { dispatch_id: String },
    Running { dispatch_id: String },
    Cancelled { dispatch_id: String },
    Failed { dispatch_id: String },
    Completed { dispatch_id: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusReport {
    pub state: AttemptState,
    pub observation: Value,
}

#[derive(Deserialize)]
struct OrcaReceipt<T> {
    ok: bool,
    result: T,
}

#[derive(Deserialize)]
struct StatusReceipt {
    runtime: RuntimeReceipt,
}

#[derive(Deserialize)]
struct RuntimeReceipt {
    #[serde(rename = "appVersion")]
    app_version: String,
}

#[derive(Deserialize)]
struct WorktreeCurrentReceipt {
    worktree: WorktreeReceipt,
}

#[derive(Deserialize)]
struct WorktreeReceipt {
    id: String,
    path: String,
}

#[derive(Deserialize)]
struct IdReceipt {
    id: String,
}

#[derive(Deserialize)]
struct RunCreateReceipt {
    run: IdReceipt,
}

#[derive(Deserialize)]
struct TaskCreateReceipt {
    task: IdReceipt,
}

#[derive(Deserialize)]
struct GateCreateReceipt {
    gate: IdReceipt,
}

#[derive(Deserialize)]
struct GateListReceipt {
    gates: Vec<GateReceipt>,
}

#[derive(Deserialize)]
struct GateReceipt {
    id: String,
    status: String,
    resolution: Option<String>,
}

#[derive(Deserialize)]
struct WorkerStartReceipt {
    #[serde(rename = "dispatchId")]
    dispatch_id: String,
    state: String,
}

#[derive(Deserialize)]
struct WorkerShowReceipt {
    dispatch: IdReceipt,
    worker: WorkerStateReceipt,
}

#[derive(Deserialize)]
struct WorkerStateReceipt {
    state: String,
}

#[derive(Deserialize)]
struct WorkerMutationReceipt {
    #[serde(rename = "dispatchId")]
    dispatch_id: String,
    state: String,
}

pub struct ProofRunner<C> {
    repository_root: PathBuf,
    state_root: PathBuf,
    orca: C,
}

impl<C: OrcaCommand> ProofRunner<C> {
    pub fn new(
        repository_root: impl AsRef<Path>,
        state_root: impl AsRef<Path>,
        orca: C,
    ) -> Result<Self, RunnerError> {
        let repository_root = fs::canonicalize(repository_root.as_ref()).map_err(|error| {
            RunnerError::InvalidArguments(format!("repository root is not readable: {error}"))
        })?;
        let state_root = resolve_path(state_root.as_ref())?;
        if state_root.starts_with(&repository_root) {
            return Err(RunnerError::UnsafeStateRoot(format!(
                "{} is inside repository {}; attempt records must be external",
                state_root.display(),
                repository_root.display()
            )));
        }
        Ok(Self {
            repository_root,
            state_root,
            orca,
        })
    }

    pub fn state_root(&self) -> &Path {
        &self.state_root
    }

    pub fn orca(&self) -> &C {
        &self.orca
    }

    pub fn orca_mut(&mut self) -> &mut C {
        &mut self.orca
    }

    pub fn prepare(&mut self) -> Result<AttemptState, RunnerError> {
        let _lock = self.acquire_mutation_lock()?;
        verify_package_is_inert(&self.repository_root)?;

        let status = self.call(["status", "--json"])?;
        let orca_version = status_receipt_version(&status)?;
        if orca_version != PINNED_ORCA_VERSION {
            return Err(RunnerError::Protocol(format!(
                "this receipt contract is pinned to Orca {PINNED_ORCA_VERSION}, found {orca_version}"
            )));
        }

        let worktree = self.call(["worktree", "current", "--json"])?;
        let (worktree_id, worktree_path) = worktree_current_receipt(&worktree)?;
        let worktree_path = fs::canonicalize(&worktree_path).map_err(|error| {
            RunnerError::Protocol(format!("Orca worktree path is not readable: {error}"))
        })?;
        if worktree_path != self.repository_root {
            return Err(RunnerError::Protocol(format!(
                "Orca resolved {}, expected {}",
                worktree_path.display(),
                self.repository_root.display()
            )));
        }

        let mut state = AttemptState::new(self.repository_root.clone(), orca_version, worktree_id);
        self.reserve_state(&state)?;
        self.continue_prepare(&mut state)?;
        Ok(state)
    }

    fn continue_prepare(&mut self, state: &mut AttemptState) -> Result<(), RunnerError> {
        self.normalize_prepare_stage(state)?;
        if state.prepare_stage == PrepareStage::Initialized {
            let receipt = self.mutate(
                state,
                OperationKind::RunCreate,
                vec![
                    "orchestration".into(),
                    "run-create".into(),
                    "--objective".into(),
                    "Attended external Runner proof; the repository plugin package remains inactive"
                        .into(),
                    "--json".into(),
                ],
            )?;
            state.run_id = Some(run_create_receipt_id(&receipt)?);
            state.prepare_stage = PrepareStage::RunCreated;
            self.finish_mutation(state)?;
        }

        if state.prepare_stage == PrepareStage::RunCreated {
            let receipt = self.mutate(
                state,
                OperationKind::TaskCreate,
                vec![
                    "orchestration".into(),
                    "task-create".into(),
                    "--run".into(),
                    required(&state.run_id, "run")?.into(),
                    "--task-title".into(),
                    "Read-only inactive-package proof".into(),
                    "--display-name".into(),
                    "Orca Runner proof worker".into(),
                    "--spec".into(),
                    worker_spec().into(),
                    "--json".into(),
                ],
            )?;
            state.task_id = Some(task_create_receipt_id(&receipt)?);
            state.prepare_stage = PrepareStage::TaskCreated;
            self.finish_mutation(state)?;
        }

        if state.prepare_stage == PrepareStage::TaskCreated {
            let receipt = self.mutate(
                state,
                OperationKind::GateCreate,
                vec![
                    "orchestration".into(),
                    "gate-create".into(),
                    "--task".into(),
                    required(&state.task_id, "task")?.into(),
                    "--question".into(),
                    "Approve one supervised Codex worker to perform the read-only inactive-package proof in the persisted exact worktree?"
                        .into(),
                    "--options".into(),
                    "[\"approved\",\"rejected\"]".into(),
                    "--json".into(),
                ],
            )?;
            state.gate_id = Some(gate_create_receipt_id(&receipt)?);
            state.prepare_stage = PrepareStage::GateCreated;
            state.phase = AttemptPhase::AwaitingApproval;
            self.finish_mutation(state)?;
        }
        if state.prepare_stage != PrepareStage::GateCreated {
            return Err(RunnerError::Protocol(
                "prepare did not reach its durable gate-created substage".into(),
            ));
        }
        if state.phase == AttemptPhase::Preparing {
            state.phase = AttemptPhase::AwaitingApproval;
            self.save_state(state)?;
        }
        Ok(())
    }

    fn normalize_prepare_stage(&self, state: &mut AttemptState) -> Result<(), RunnerError> {
        let inferred = match (
            state.run_id.is_some(),
            state.task_id.is_some(),
            state.gate_id.is_some(),
        ) {
            (false, false, false) => PrepareStage::Initialized,
            (true, false, false) => PrepareStage::RunCreated,
            (true, true, false) => PrepareStage::TaskCreated,
            (true, true, true) => PrepareStage::GateCreated,
            _ => {
                return Err(RunnerError::Protocol(
                    "prepare receipts are not a valid Run -> Task -> Gate prefix".into(),
                ))
            }
        };
        if state.prepare_stage == inferred {
            return Ok(());
        }
        if state.schema_version != "v0" {
            return Err(RunnerError::Protocol(format!(
                "prepare substage {:?} disagrees with persisted receipts ({inferred:?})",
                state.prepare_stage
            )));
        }
        state.prepare_stage = inferred;
        self.save_state(state)
    }

    pub fn resume(&mut self) -> Result<ResumeOutcome, RunnerError> {
        let _lock = self.acquire_mutation_lock()?;
        let mut state = self.load_state()?;
        match state.phase {
            AttemptPhase::AwaitingApproval => self.resume_after_gate(&mut state),
            AttemptPhase::Running => self.observe_worker(&mut state),
            AttemptPhase::Rejected => Ok(ResumeOutcome::Rejected {
                gate_id: required(&state.gate_id, "gate")?.into(),
            }),
            AttemptPhase::Cancelled => Ok(ResumeOutcome::Cancelled {
                dispatch_id: state.last_dispatch()?.into(),
            }),
            AttemptPhase::Failed => Ok(ResumeOutcome::Failed {
                dispatch_id: state.last_dispatch()?.into(),
            }),
            AttemptPhase::Completed => Ok(ResumeOutcome::Completed {
                dispatch_id: state.last_dispatch()?.into(),
            }),
            AttemptPhase::OutcomeUnknown => {
                let operation = state.pending_operation.as_ref().ok_or_else(|| {
                    RunnerError::Protocol(
                        "outcome_unknown attempt omitted its operation intent".into(),
                    )
                })?;
                Err(RunnerError::OutcomeUnknown(format!(
                    "{:?} request {} has an unreconciled captured receipt; inspect status and cancel the exact dispatch before retrying",
                    operation.kind, operation.request_id
                )))
            }
            AttemptPhase::Preparing => {
                self.continue_prepare(&mut state)?;
                Ok(ResumeOutcome::AwaitingApproval {
                    gate_id: required(&state.gate_id, "gate")?.into(),
                })
            }
        }
    }

    pub fn cancel(&mut self) -> Result<ResumeOutcome, RunnerError> {
        let _lock = self.acquire_mutation_lock()?;
        let mut state = self.load_state()?;
        if !matches!(
            state.phase,
            AttemptPhase::Running | AttemptPhase::OutcomeUnknown
        ) {
            return Err(RunnerError::InvalidTransition(format!(
                "cancel requires running or outcome_unknown, found {:?}",
                state.phase
            )));
        }
        let dispatch_id = state.last_dispatch()?.to_owned();
        if state.phase == AttemptPhase::OutcomeUnknown {
            let operation = state.pending_operation.take().ok_or_else(|| {
                RunnerError::Protocol(
                    "outcome_unknown attempt omitted its durable operation receipt".into(),
                )
            })?;
            state.operation_history.push(operation);
            self.save_state(&state)?;
        }
        let receipt = self.mutate(
            &mut state,
            OperationKind::WorkerStop,
            vec![
                "orchestration".into(),
                "worker-stop".into(),
                "--dispatch".into(),
                dispatch_id.clone(),
                "--json".into(),
            ],
        )?;
        parse_worker_stop_receipt(&receipt, &dispatch_id)?;
        state.phase = AttemptPhase::Cancelled;
        self.finish_mutation(&mut state)?;
        Ok(ResumeOutcome::Cancelled { dispatch_id })
    }

    pub fn retry(&mut self) -> Result<ResumeOutcome, RunnerError> {
        let _lock = self.acquire_mutation_lock()?;
        let mut state = self.load_state()?;
        if !matches!(state.phase, AttemptPhase::Cancelled | AttemptPhase::Failed) {
            return Err(RunnerError::InvalidTransition(format!(
                "retry requires cancelled or failed, found {:?}",
                state.phase
            )));
        }
        let prior_dispatch = state.last_dispatch()?.to_owned();
        self.start_worker(&mut state, Some(&prior_dispatch))
    }

    pub fn status(&mut self) -> Result<StatusReport, RunnerError> {
        let state = self.load_state()?;
        let observation = match state.phase {
            AttemptPhase::AwaitingApproval | AttemptPhase::Rejected => self.call_owned(vec![
                "orchestration".into(),
                "gate-list".into(),
                "--run".into(),
                required(&state.run_id, "run")?.into(),
                "--json".into(),
            ])?,
            AttemptPhase::Running
            | AttemptPhase::Cancelled
            | AttemptPhase::Failed
            | AttemptPhase::OutcomeUnknown
            | AttemptPhase::Completed => self.call_owned(vec![
                "orchestration".into(),
                "worker-show".into(),
                "--dispatch".into(),
                state.last_dispatch()?.into(),
                "--json".into(),
            ])?,
            AttemptPhase::Preparing => state
                .pending_operation
                .as_ref()
                .and_then(|operation| operation.receipt.clone())
                .unwrap_or_else(|| json!({"note": "prepare has no captured mutation receipt"})),
        };
        Ok(StatusReport { state, observation })
    }

    pub fn load_state(&self) -> Result<AttemptState, RunnerError> {
        let bytes = fs::read(self.state_path()).map_err(|error| {
            RunnerError::Io(format!(
                "could not read {}: {error}",
                self.state_path().display()
            ))
        })?;
        serde_json::from_slice(&bytes).map_err(Into::into)
    }

    pub fn save_state(&self, state: &AttemptState) -> Result<(), RunnerError> {
        fs::create_dir_all(&self.state_root)?;
        let temporary = self.state_root.join(format!(
            ".attempt.json.{}.{}.tmp",
            std::process::id(),
            NEXT_UNIQUE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            write_state(&mut file, state)?;
            fs::rename(&temporary, self.state_path())?;
            sync_directory(&self.state_root)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn acquire_mutation_lock(&self) -> Result<AttemptLock, RunnerError> {
        fs::create_dir_all(&self.state_root)?;
        let path = self.state_root.join(LOCK_FILE);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let token = new_request_id();
                if let Err(error) = writeln!(file, "{token}").and_then(|()| file.sync_all()) {
                    drop(file);
                    let _ = fs::remove_file(&path);
                    return Err(error.into());
                }
                Ok(AttemptLock { path, token })
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(RunnerError::InvalidTransition(format!(
                    "{} exists; another mutating command may be active or require operator reconciliation",
                    path.display()
                )))
            }
            Err(error) => Err(error.into()),
        }
    }

    fn reserve_state(&self, state: &AttemptState) -> Result<(), RunnerError> {
        fs::create_dir_all(&self.state_root)?;
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.state_path())
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(RunnerError::InvalidTransition(format!(
                    "{} already exists; use status or resume",
                    self.state_path().display()
                )))
            }
            Err(error) => return Err(error.into()),
        };
        write_state(&mut file, state)?;
        sync_directory(&self.state_root)
    }

    fn resume_after_gate(
        &mut self,
        state: &mut AttemptState,
    ) -> Result<ResumeOutcome, RunnerError> {
        let gates = self.call_owned(vec![
            "orchestration".into(),
            "gate-list".into(),
            "--run".into(),
            required(&state.run_id, "run")?.into(),
            "--json".into(),
        ])?;
        let gate_id = required(&state.gate_id, "gate")?.to_owned();
        let gate = gate_list_record(&gates, &gate_id)?;
        match (gate.status.as_str(), gate.resolution.as_deref()) {
            ("pending", None) => return Ok(ResumeOutcome::AwaitingApproval { gate_id }),
            ("resolved", Some("approved")) => {}
            ("resolved", Some("rejected")) => {
                state.phase = AttemptPhase::Rejected;
                self.save_state(state)?;
                return Ok(ResumeOutcome::Rejected { gate_id });
            }
            (status, resolution) => {
                return Err(RunnerError::Protocol(format!(
                    "Orca 1.4.188 gate-list returned unsupported status/resolution {status:?}/{resolution:?}"
                )))
            }
        }
        self.start_worker(state, None)
    }

    fn start_worker(
        &mut self,
        state: &mut AttemptState,
        retry_of: Option<&str>,
    ) -> Result<ResumeOutcome, RunnerError> {
        self.revalidate_launch_authority(state)?;
        let mut args = vec![
            "orchestration".into(),
            "worker-start".into(),
            "--task".into(),
            required(&state.task_id, "task")?.into(),
            "--run".into(),
            required(&state.run_id, "run")?.into(),
            "--worktree".into(),
            format!("id:{}", state.worktree_id),
            "--agent".into(),
            "codex".into(),
            "--json".into(),
        ];
        if let Some(dispatch_id) = retry_of {
            let json_position = args.len() - 1;
            args.splice(
                json_position..json_position,
                ["--retry-of".into(), dispatch_id.into()],
            );
        }
        let receipt = self.mutate(state, OperationKind::WorkerStart, args)?;
        let (dispatch_id, worker_state) = worker_start_receipt(&receipt)?;
        if !state.dispatch_ids.iter().any(|known| known == &dispatch_id) {
            state.dispatch_ids.push(dispatch_id.clone());
        }
        match worker_state.as_str() {
            "ready" => state.phase = AttemptPhase::Running,
            "failed" => state.phase = AttemptPhase::Failed,
            "outcome_unknown" => {
                state.phase = AttemptPhase::OutcomeUnknown;
                self.save_state(state)?;
                return Err(RunnerError::OutcomeUnknown(format!(
                    "worker-start returned outcome_unknown for {dispatch_id}; inspect status and cancel the exact dispatch before any retry"
                )));
            }
            state_name => {
                state.phase = AttemptPhase::OutcomeUnknown;
                self.save_state(state)?;
                return Err(RunnerError::Protocol(format!(
                    "worker-start returned unsupported state `{state_name}` for Orca 1.4.188"
                )));
            }
        }
        self.finish_mutation(state)?;
        if worker_state == "failed" {
            return Ok(ResumeOutcome::Failed { dispatch_id });
        }
        Ok(ResumeOutcome::WorkerStarted { dispatch_id })
    }

    fn revalidate_launch_authority(&mut self, state: &AttemptState) -> Result<(), RunnerError> {
        let persisted_root = fs::canonicalize(&state.repository_root).map_err(|error| {
            RunnerError::Protocol(format!(
                "persisted repository root is not readable: {error}"
            ))
        })?;
        if persisted_root != self.repository_root {
            return Err(RunnerError::InvalidTransition(format!(
                "attempt authorizes {}, runner was opened for {}",
                persisted_root.display(),
                self.repository_root.display()
            )));
        }
        verify_package_is_inert(&persisted_root)?;
        let status = self.call(["status", "--json"])?;
        let current_version = status_receipt_version(&status)?;
        if current_version != state.orca_version || current_version != PINNED_ORCA_VERSION {
            return Err(RunnerError::InvalidTransition(format!(
                "Orca version changed from approved {} to {current_version}; prepare a new attended proof for the new receipt contract",
                state.orca_version
            )));
        }
        let worktree = self.call(["worktree", "current", "--json"])?;
        let (current_id, current_path) = worktree_current_receipt(&worktree)?;
        let current_path = fs::canonicalize(current_path).map_err(|error| {
            RunnerError::Protocol(format!("current Orca worktree is not readable: {error}"))
        })?;
        if current_id != state.worktree_id || current_path != persisted_root {
            return Err(RunnerError::InvalidTransition(format!(
                "current Orca worktree `{current_id}` at {} does not match approved `{}` at {}",
                current_path.display(),
                state.worktree_id,
                persisted_root.display()
            )));
        }
        Ok(())
    }

    fn observe_worker(&mut self, state: &mut AttemptState) -> Result<ResumeOutcome, RunnerError> {
        let dispatch_id = state.last_dispatch()?.to_owned();
        let receipt = self.call_owned(vec![
            "orchestration".into(),
            "worker-show".into(),
            "--dispatch".into(),
            dispatch_id.clone(),
            "--json".into(),
        ])?;
        let (observed_dispatch, status) = worker_show_receipt(&receipt)?;
        if observed_dispatch != dispatch_id {
            return Err(RunnerError::Protocol(format!(
                "worker-show returned {observed_dispatch}, expected {dispatch_id}"
            )));
        }
        match status.as_str() {
            "succeeded" => {
                self.settle_worker(
                    state,
                    "worker-release",
                    &dispatch_id,
                    AttemptPhase::Completed,
                )?;
                Ok(ResumeOutcome::Completed { dispatch_id })
            }
            "failed" => {
                self.settle_worker(state, "worker-retain", &dispatch_id, AttemptPhase::Failed)?;
                Ok(ResumeOutcome::Failed { dispatch_id })
            }
            "stopped" | "abandoned" => {
                state.phase = AttemptPhase::Cancelled;
                self.save_state(state)?;
                Ok(ResumeOutcome::Cancelled { dispatch_id })
            }
            "starting" | "ready" | "stopping" => Ok(ResumeOutcome::Running { dispatch_id }),
            "start_unknown" | "stop_unknown" => Err(RunnerError::OutcomeUnknown(format!(
                "worker-show reports `{status}` for {dispatch_id}; inspect or cancel that exact dispatch before retrying"
            ))),
            other => Err(RunnerError::Protocol(format!(
                "worker-show returned unsupported Orca 1.4.188 worker state `{other}`"
            ))),
        }
    }

    fn settle_worker(
        &mut self,
        state: &mut AttemptState,
        action: &str,
        dispatch_id: &str,
        final_phase: AttemptPhase,
    ) -> Result<(), RunnerError> {
        let kind = match action {
            "worker-release" => OperationKind::WorkerRelease,
            "worker-retain" => OperationKind::WorkerRetain,
            _ => {
                return Err(RunnerError::Protocol(format!(
                    "unsupported settlement action `{action}`"
                )))
            }
        };
        let receipt = self.mutate(
            state,
            kind,
            vec![
                "orchestration".into(),
                action.into(),
                "--dispatch".into(),
                dispatch_id.into(),
                "--json".into(),
            ],
        )?;
        parse_worker_settlement_receipt(&receipt, dispatch_id, kind)?;
        state.phase = final_phase;
        self.finish_mutation(state)?;
        Ok(())
    }

    fn mutate(
        &mut self,
        state: &mut AttemptState,
        kind: OperationKind,
        mut args: Vec<String>,
    ) -> Result<Value, RunnerError> {
        if let Some(operation) = &state.pending_operation {
            if operation.kind != kind {
                return Err(RunnerError::OutcomeUnknown(format!(
                    "pending {:?} request {} must be reconciled before {:?}",
                    operation.kind, operation.request_id, kind
                )));
            }
            match operation.status {
                OperationStatus::ReceiptCaptured => {
                    return operation.receipt.clone().ok_or_else(|| {
                        RunnerError::Protocol(
                            "receipt_captured operation omitted its durable receipt".into(),
                        )
                    });
                }
                OperationStatus::DefinitiveFailure => {
                    return Err(RunnerError::InvalidTransition(format!(
                        "request {} failed definitively: {}",
                        operation.request_id,
                        operation.error.as_deref().unwrap_or("unknown Orca error")
                    )));
                }
                OperationStatus::IntentPersisted | OperationStatus::OutcomeUnknown => {
                    args = operation.args.clone();
                }
            }
        } else {
            let request_id = new_request_id();
            insert_retry_request(&mut args, &request_id)?;
            state.pending_operation = Some(OperationIntent {
                kind,
                request_id,
                args: args.clone(),
                status: OperationStatus::IntentPersisted,
                receipt: None,
                error: None,
            });
            self.save_state(state)?;
        }
        let expected_request_id = state
            .pending_operation
            .as_ref()
            .map(|operation| operation.request_id.clone())
            .ok_or_else(|| RunnerError::Protocol("mutation intent disappeared".into()))?;

        match self.call_owned(args) {
            Ok(receipt) => {
                let operation = state.pending_operation.as_mut().ok_or_else(|| {
                    RunnerError::Protocol("mutation returned without a persisted intent".into())
                })?;
                operation.status = OperationStatus::ReceiptCaptured;
                operation.receipt = Some(receipt.clone());
                operation.error = None;
                self.save_state(state)?;
                Ok(receipt)
            }
            Err(error) => {
                if error
                    .receipt()
                    .and_then(|receipt| receipt.get("ok"))
                    .and_then(Value::as_bool)
                    == Some(true)
                {
                    let receipt = error.receipt().cloned().ok_or_else(|| {
                        RunnerError::Protocol(
                            "structured nonzero Orca result lost its receipt".into(),
                        )
                    })?;
                    let operation = state.pending_operation.as_mut().ok_or_else(|| {
                        RunnerError::Protocol(
                            "structured nonzero mutation returned without a persisted intent"
                                .into(),
                        )
                    })?;
                    operation.status = OperationStatus::ReceiptCaptured;
                    operation.receipt = Some(receipt.clone());
                    operation.error = Some(error.to_string());
                    self.save_state(state)?;
                    return Ok(receipt);
                }
                let returned_request_id = error.receipt().and_then(|receipt| {
                    receipt
                        .pointer("/error/data/orchestrationRequestId")
                        .and_then(Value::as_str)
                });
                let mismatched_request =
                    returned_request_id.is_some_and(|request_id| request_id != expected_request_id);
                let definitive = mismatched_request
                    || error
                        .receipt()
                        .and_then(error_code)
                        .is_some_and(|code| !mutation_outcome_may_be_unknown(code));
                let message = if mismatched_request {
                    format!(
                        "{}; Orca returned recovery request {:?}, expected {}",
                        error, returned_request_id, expected_request_id
                    )
                } else {
                    error.to_string()
                };
                let receipt = error.receipt().cloned();
                let operation = state.pending_operation.as_mut().ok_or_else(|| {
                    RunnerError::Protocol("mutation failed without a persisted intent".into())
                })?;
                operation.status = if definitive {
                    OperationStatus::DefinitiveFailure
                } else {
                    OperationStatus::OutcomeUnknown
                };
                operation.receipt = receipt;
                operation.error = Some(message.clone());
                let request_id = operation.request_id.clone();
                self.save_state(state)?;
                if definitive {
                    Err(RunnerError::InvalidTransition(format!(
                        "request {} failed definitively and will not be reissued: {message}",
                        request_id
                    )))
                } else {
                    Err(RunnerError::OutcomeUnknown(format!(
                        "request {} may have taken effect; rerun the same lifecycle command to replay its persisted exact --retry-request, never issue a fresh mutation: {message}",
                        request_id
                    )))
                }
            }
        }
    }

    fn finish_mutation(&self, state: &mut AttemptState) -> Result<(), RunnerError> {
        let operation = state.pending_operation.as_ref().ok_or_else(|| {
            RunnerError::Protocol("cannot finish mutation without its durable intent".into())
        })?;
        if operation.status != OperationStatus::ReceiptCaptured {
            return Err(RunnerError::OutcomeUnknown(format!(
                "request {} has not produced a captured receipt",
                operation.request_id
            )));
        }
        let operation = state.pending_operation.take().ok_or_else(|| {
            RunnerError::Protocol("mutation receipt disappeared before archival".into())
        })?;
        state.operation_history.push(operation);
        self.save_state(state)
    }

    fn state_path(&self) -> PathBuf {
        self.state_root.join(STATE_FILE)
    }

    fn call<const N: usize>(&mut self, args: [&str; N]) -> Result<Value, RunnerError> {
        self.call_owned(args.into_iter().map(str::to_owned).collect())
    }

    fn call_owned(&mut self, args: Vec<String>) -> Result<Value, RunnerError> {
        self.orca.call(&args)
    }
}

struct AttemptLock {
    path: PathBuf,
    token: String,
}

impl Drop for AttemptLock {
    fn drop(&mut self) {
        if fs::read_to_string(&self.path).is_ok_and(|contents| contents.trim() == self.token) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn worker_spec() -> &'static str {
    "Read-only attended proof. Inspect the current worktree with `git status --short --branch` and read `.repository-engineering/package.toml`. Verify activation_eligibility is `none`, both active lists are empty, and `orca_ui` plus `worker_adapter` are disabled. Do not edit files, create commits, access credentials, or use network. Report the branch, changed-file count, and inert-package verdict to the coordinator, then follow the injected Orca worker instructions to mark the task completed."
}

fn verify_package_is_inert(repository_root: &Path) -> Result<(), RunnerError> {
    let path = repository_root.join(".repository-engineering/package.toml");
    let source = fs::read_to_string(&path).map_err(|error| {
        RunnerError::PackageActive(format!("could not read {}: {error}", path.display()))
    })?;
    let package: toml::Value = toml::from_str(&source)
        .map_err(|error| RunnerError::PackageActive(format!("invalid package.toml: {error}")))?;
    if package
        .get("activation_eligibility")
        .and_then(toml::Value::as_str)
        != Some("none")
    {
        return Err(RunnerError::PackageActive(
            "activation_eligibility must remain `none`".into(),
        ));
    }
    for key in ["active_capability_contracts", "active_worker_roles"] {
        if !matches!(package.get(key).and_then(toml::Value::as_array), Some(values) if values.is_empty())
        {
            return Err(RunnerError::PackageActive(format!(
                "{key} must remain empty"
            )));
        }
    }
    let optional = package
        .get("optional_components")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| RunnerError::PackageActive("optional_components missing".into()))?;
    for component in ["orca_ui", "worker_adapter"] {
        let disabled = optional.iter().any(|entry| {
            entry.get("component").and_then(toml::Value::as_str) == Some(component)
                && entry.get("selection").and_then(toml::Value::as_str) == Some("disabled")
        });
        if !disabled {
            return Err(RunnerError::PackageActive(format!(
                "{component} must remain disabled"
            )));
        }
    }
    Ok(())
}

fn resolve_path(path: &Path) -> Result<PathBuf, RunnerError> {
    if !path.is_absolute() {
        return Err(RunnerError::UnsafeStateRoot(
            "state root must be an absolute path".into(),
        ));
    }
    match fs::canonicalize(path) {
        Ok(path) => return Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let parent = path.parent().ok_or_else(|| {
        RunnerError::UnsafeStateRoot("state root must have an existing parent".into())
    })?;
    let parent = fs::canonicalize(parent).map_err(|error| {
        RunnerError::UnsafeStateRoot(format!(
            "state-root parent {} is not readable: {error}",
            parent.display()
        ))
    })?;
    Ok(parent.join(
        path.file_name()
            .ok_or_else(|| RunnerError::UnsafeStateRoot("state root has no name".into()))?,
    ))
}

fn write_state(file: &mut fs::File, state: &AttemptState) -> Result<(), RunnerError> {
    serde_json::to_writer_pretty(&mut *file, state)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), RunnerError> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

fn required<'a>(value: &'a Option<String>, noun: &str) -> Result<&'a str, RunnerError> {
    value
        .as_deref()
        .ok_or_else(|| RunnerError::Protocol(format!("attempt has no {noun} receipt")))
}

fn status_receipt_version(value: &Value) -> Result<String, RunnerError> {
    Ok(parse_receipt::<StatusReceipt>(value, "status")?
        .runtime
        .app_version)
}

fn worktree_current_receipt(value: &Value) -> Result<(String, String), RunnerError> {
    let receipt = parse_receipt::<WorktreeCurrentReceipt>(value, "worktree current")?.worktree;
    Ok((receipt.id, receipt.path))
}

fn run_create_receipt_id(value: &Value) -> Result<String, RunnerError> {
    Ok(parse_receipt::<RunCreateReceipt>(value, "run-create")?
        .run
        .id)
}

fn task_create_receipt_id(value: &Value) -> Result<String, RunnerError> {
    Ok(parse_receipt::<TaskCreateReceipt>(value, "task-create")?
        .task
        .id)
}

fn gate_create_receipt_id(value: &Value) -> Result<String, RunnerError> {
    Ok(parse_receipt::<GateCreateReceipt>(value, "gate-create")?
        .gate
        .id)
}

fn worker_start_receipt(value: &Value) -> Result<(String, String), RunnerError> {
    let receipt = parse_receipt::<WorkerStartReceipt>(value, "worker-start")?;
    Ok((receipt.dispatch_id, receipt.state))
}

fn worker_show_receipt(value: &Value) -> Result<(String, String), RunnerError> {
    let receipt = parse_receipt::<WorkerShowReceipt>(value, "worker-show")?;
    Ok((receipt.dispatch.id, receipt.worker.state))
}

fn parse_worker_stop_receipt(value: &Value, expected_dispatch: &str) -> Result<(), RunnerError> {
    let receipt = parse_receipt::<WorkerMutationReceipt>(value, "worker-stop")?;
    if receipt.dispatch_id != expected_dispatch {
        return Err(RunnerError::Protocol(format!(
            "worker-stop returned {}, expected {expected_dispatch}",
            receipt.dispatch_id
        )));
    }
    match receipt.state.as_str() {
        "stopped" => Ok(()),
        "stop_unknown" => Err(RunnerError::OutcomeUnknown(format!(
            "worker-stop could not prove {expected_dispatch} stopped"
        ))),
        other => Err(RunnerError::Protocol(format!(
            "worker-stop returned unsupported Orca 1.4.188 state `{other}`"
        ))),
    }
}

fn parse_worker_settlement_receipt(
    value: &Value,
    expected_dispatch: &str,
    kind: OperationKind,
) -> Result<(), RunnerError> {
    let receipt = parse_receipt::<WorkerMutationReceipt>(value, "worker settlement")?;
    if receipt.dispatch_id != expected_dispatch {
        return Err(RunnerError::Protocol(format!(
            "worker settlement returned {}, expected {expected_dispatch}",
            receipt.dispatch_id
        )));
    }
    let accepted = match kind {
        OperationKind::WorkerRelease => {
            matches!(
                receipt.state.as_str(),
                "released" | "already_released" | "retained"
            )
        }
        OperationKind::WorkerRetain => receipt.state == "retained",
        _ => false,
    };
    if accepted {
        Ok(())
    } else if matches!(
        receipt.state.as_str(),
        "release_pending" | "release_unknown"
    ) {
        Err(RunnerError::OutcomeUnknown(format!(
            "{:?} could not prove settlement for {expected_dispatch}",
            kind
        )))
    } else {
        Err(RunnerError::Protocol(format!(
            "{:?} returned unsupported Orca 1.4.188 state `{}`",
            kind, receipt.state
        )))
    }
}

fn gate_list_record(value: &Value, id: &str) -> Result<GateReceipt, RunnerError> {
    parse_receipt::<GateListReceipt>(value, "gate-list")?
        .gates
        .into_iter()
        .find(|record| record.id == id)
        .ok_or_else(|| RunnerError::Protocol(format!("gate-list receipt omitted gate {id}")))
}

fn parse_receipt<T: DeserializeOwned>(value: &Value, command: &str) -> Result<T, RunnerError> {
    let receipt: OrcaReceipt<T> = serde_json::from_value(value.clone()).map_err(|error| {
        RunnerError::Protocol(format!(
            "Orca 1.4.188 {command} receipt did not match its pinned schema: {error}"
        ))
    })?;
    if !receipt.ok {
        return Err(RunnerError::Protocol(format!(
            "Orca 1.4.188 {command} returned ok:false through the success boundary"
        )));
    }
    Ok(receipt.result)
}

fn new_request_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = NEXT_UNIQUE_ID.fetch_add(1, Ordering::Relaxed);
    format!(
        "orca_runner_proof_{}_{}_{}",
        std::process::id(),
        nanos,
        sequence
    )
}

fn insert_retry_request(args: &mut Vec<String>, request_id: &str) -> Result<(), RunnerError> {
    if args.iter().any(|arg| arg == "--retry-request") {
        return Err(RunnerError::Protocol(
            "fresh mutation arguments unexpectedly contained --retry-request".into(),
        ));
    }
    let json_position = args
        .iter()
        .position(|arg| arg == "--json")
        .ok_or_else(|| RunnerError::Protocol("mutation omitted --json".into()))?;
    args.splice(
        json_position..json_position,
        ["--retry-request".into(), request_id.into()],
    );
    Ok(())
}

fn error_code(value: &Value) -> Option<&str> {
    value.pointer("/error/code").and_then(Value::as_str)
}

fn mutation_outcome_may_be_unknown(code: &str) -> bool {
    matches!(
        code,
        "runtime_unavailable"
            | "remote_runtime_unavailable"
            | "runtime_timeout"
            | "invalid_runtime_response"
    )
}
