use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use orca_runner_proof::{
    AttemptPhase, OperationStatus, OrcaCommand, ProcessOrca, ProofRunner, ResumeOutcome,
    RunnerError,
};
use serde_json::{json, Value};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "orca-runner-proof-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

enum ScriptedResponse {
    Value(Value),
    Error(RunnerError),
    BlockStatePersistence { state_root: PathBuf, value: Value },
}

impl From<Value> for ScriptedResponse {
    fn from(value: Value) -> Self {
        Self::Value(value)
    }
}

#[derive(Default)]
struct ScriptedOrca {
    responses: VecDeque<ScriptedResponse>,
    calls: Vec<Vec<String>>,
}

impl ScriptedOrca {
    fn with_responses(responses: impl IntoIterator<Item = Value>) -> Self {
        Self {
            responses: responses.into_iter().map(Into::into).collect(),
            calls: Vec::new(),
        }
    }

    fn push_values(&mut self, responses: impl IntoIterator<Item = Value>) {
        self.responses.extend(responses.into_iter().map(Into::into));
    }
}

impl OrcaCommand for ScriptedOrca {
    fn call(&mut self, args: &[String]) -> Result<Value, RunnerError> {
        self.calls.push(args.to_vec());
        match self
            .responses
            .pop_front()
            .ok_or_else(|| RunnerError::Protocol("unexpected Orca call".into()))?
        {
            ScriptedResponse::Value(value) => Ok(value),
            ScriptedResponse::Error(error) => Err(error),
            ScriptedResponse::BlockStatePersistence { state_root, value } => {
                let blocked = state_root.with_extension("blocked");
                fs::rename(&state_root, &blocked).unwrap();
                fs::write(&state_root, b"blocks state directory recreation").unwrap();
                Ok(value)
            }
        }
    }
}

fn status_receipt() -> Value {
    json!({"ok": true, "result": {"runtime": {"state": "ready", "appVersion": "1.4.188"}}})
}

fn worktree_receipt(repository: &Path, id: &str) -> Value {
    json!({"ok": true, "result": {"worktree": {"id": id, "path": repository}}})
}

fn gate_receipt(status: &str, resolution: Option<&str>) -> Value {
    json!({
        "ok": true,
        "result": {
            "gates": [{"id": "gate-1", "status": status, "resolution": resolution}]
        }
    })
}

fn worker_start_receipt(dispatch_id: &str, state: &str) -> Value {
    json!({
        "ok": true,
        "result": {
            "runId": "run-1",
            "taskId": "task-1",
            "dispatchId": dispatch_id,
            "state": state
        }
    })
}

fn worker_show_receipt(dispatch_id: &str, state: &str) -> Value {
    json!({
        "ok": true,
        "result": {"dispatch": {"id": dispatch_id}, "worker": {"state": state}}
    })
}

fn worker_mutation_receipt(dispatch_id: &str, state: &str) -> Value {
    json!({"ok": true, "result": {"dispatchId": dispatch_id, "state": state}})
}

fn inactive_repository(root: &Path) {
    let package = root.join(".repository-engineering");
    fs::create_dir_all(&package).unwrap();
    fs::write(
        package.join("package.toml"),
        r#"
activation_eligibility = "none"
active_capability_contracts = []
active_worker_roles = []

[[optional_components]]
component = "orca_ui"
selection = "disabled"

[[optional_components]]
component = "worker_adapter"
selection = "disabled"
"#,
    )
    .unwrap();
}

fn prepared_attempt() -> (TempDir, TempDir, ProofRunner<ScriptedOrca>) {
    let repository = TempDir::new("repository");
    let state_parent = TempDir::new("external-state");
    inactive_repository(repository.path());
    let state_root = state_parent.path().join("attempt");
    let orca = ScriptedOrca::with_responses([
        status_receipt(),
        worktree_receipt(repository.path(), "repo-1::worktree"),
        json!({"ok": true, "result": {"run": {"id": "run-1"}}}),
        json!({"ok": true, "result": {"task": {"id": "task-1"}}}),
        json!({"ok": true, "result": {"gate": {"id": "gate-1"}}}),
    ]);
    let mut runner = ProofRunner::new(repository.path(), &state_root, orca).unwrap();
    runner.prepare().unwrap();
    (repository, state_parent, runner)
}

fn command_count(runner: &ProofRunner<ScriptedOrca>, command: &str) -> usize {
    runner
        .orca()
        .calls
        .iter()
        .filter(|call| call.get(1).map(String::as_str) == Some(command))
        .count()
}

fn assert_retry_request(call: &[String]) {
    let position = call
        .iter()
        .position(|arg| arg == "--retry-request")
        .expect("every mutation must carry a persisted retry request");
    assert!(!call[position + 1].is_empty());
}

#[test]
fn prepare_creates_attended_gate_and_persists_receipts_outside_repository() {
    let (repository, _state_parent, runner) = prepared_attempt();
    let state = runner.load_state().unwrap();
    assert_eq!(state.phase, AttemptPhase::AwaitingApproval);
    assert_eq!(state.run_id.as_deref(), Some("run-1"));
    assert_eq!(state.task_id.as_deref(), Some("task-1"));
    assert_eq!(state.gate_id.as_deref(), Some("gate-1"));
    assert!(!runner.state_root().starts_with(repository.path()));

    let calls = &runner.orca().calls;
    assert_eq!(calls[0], ["status", "--json"]);
    assert_eq!(calls[1], ["worktree", "current", "--json"]);
    assert_eq!(calls[2][..2], ["orchestration", "run-create"]);
    assert_eq!(calls[3][..2], ["orchestration", "task-create"]);
    assert_eq!(calls[4][..2], ["orchestration", "gate-create"]);
    for call in &calls[2..=4] {
        assert_retry_request(call);
    }
}

#[test]
fn resume_waits_for_operator_and_starts_only_after_approval_in_exact_worktree() {
    let (repository, _state_parent, mut runner) = prepared_attempt();
    runner.orca_mut().push_values([
        gate_receipt("pending", None),
        gate_receipt("resolved", Some("approved")),
        status_receipt(),
        worktree_receipt(repository.path(), "repo-1::worktree"),
        worker_start_receipt("dispatch-1", "ready"),
    ]);

    assert_eq!(
        runner.resume().unwrap(),
        ResumeOutcome::AwaitingApproval {
            gate_id: "gate-1".into()
        }
    );
    assert_eq!(
        runner.resume().unwrap(),
        ResumeOutcome::WorkerStarted {
            dispatch_id: "dispatch-1".into()
        }
    );
    assert_eq!(runner.load_state().unwrap().phase, AttemptPhase::Running);

    let worker_start = runner
        .orca()
        .calls
        .iter()
        .find(|call| call.get(1).map(String::as_str) == Some("worker-start"))
        .unwrap();
    assert!(worker_start
        .windows(2)
        .any(|pair| pair == ["--worktree", "id:repo-1::worktree"]));
    assert!(worker_start
        .windows(2)
        .any(|pair| pair == ["--agent", "codex"]));
    assert_retry_request(worker_start);
}

#[test]
fn partial_prepare_resumes_from_last_receipt_with_same_request_identity() {
    let repository = TempDir::new("partial-repository");
    let state_parent = TempDir::new("partial-state");
    inactive_repository(repository.path());
    let mut orca = ScriptedOrca::with_responses([
        status_receipt(),
        worktree_receipt(repository.path(), "repo-1::worktree"),
        json!({"ok": true, "result": {"run": {"id": "run-1"}}}),
    ]);
    orca.responses
        .push_back(ScriptedResponse::Error(RunnerError::Orca(
            "transport disappeared after task-create".into(),
        )));
    orca.push_values([
        json!({"ok": true, "result": {"task": {"id": "task-1"}}}),
        json!({"ok": true, "result": {"gate": {"id": "gate-1"}}}),
    ]);
    let mut runner =
        ProofRunner::new(repository.path(), state_parent.path().join("attempt"), orca).unwrap();

    assert!(matches!(
        runner.prepare(),
        Err(RunnerError::OutcomeUnknown(_))
    ));
    let partial = runner.load_state().unwrap();
    assert_eq!(partial.phase, AttemptPhase::Preparing);
    assert_eq!(partial.run_id.as_deref(), Some("run-1"));
    assert_eq!(
        partial.pending_operation.as_ref().unwrap().status,
        OperationStatus::OutcomeUnknown
    );
    assert_eq!(
        runner.resume().unwrap(),
        ResumeOutcome::AwaitingApproval {
            gate_id: "gate-1".into()
        }
    );
    assert_eq!(command_count(&runner, "run-create"), 1);
    let task_calls: Vec<_> = runner
        .orca()
        .calls
        .iter()
        .filter(|call| call.get(1).map(String::as_str) == Some("task-create"))
        .collect();
    assert_eq!(task_calls.len(), 2);
    assert_eq!(task_calls[0], task_calls[1]);
}

#[test]
fn mutation_success_before_persistence_replays_the_same_idempotent_request() {
    let repository = TempDir::new("persistence-fault-repository");
    let state_parent = TempDir::new("persistence-fault-state");
    inactive_repository(repository.path());
    let state_root = state_parent.path().join("attempt");
    let blocked_root = state_root.with_extension("blocked");
    let mut orca = ScriptedOrca::with_responses([
        status_receipt(),
        worktree_receipt(repository.path(), "repo-1::worktree"),
    ]);
    orca.responses
        .push_back(ScriptedResponse::BlockStatePersistence {
            state_root: state_root.clone(),
            value: json!({"ok": true, "result": {"run": {"id": "run-1"}}}),
        });
    orca.push_values([
        json!({"ok": true, "result": {"run": {"id": "run-1"}}}),
        json!({"ok": true, "result": {"task": {"id": "task-1"}}}),
        json!({"ok": true, "result": {"gate": {"id": "gate-1"}}}),
    ]);
    let mut runner = ProofRunner::new(repository.path(), &state_root, orca).unwrap();

    assert!(matches!(runner.prepare(), Err(RunnerError::Io(_))));
    fs::remove_file(&state_root).unwrap();
    fs::rename(&blocked_root, &state_root).unwrap();
    let _ = fs::remove_file(state_root.join("attempt.lock"));

    assert_eq!(
        runner.resume().unwrap(),
        ResumeOutcome::AwaitingApproval {
            gate_id: "gate-1".into()
        }
    );
    let run_calls: Vec<_> = runner
        .orca()
        .calls
        .iter()
        .filter(|call| call.get(1).map(String::as_str) == Some("run-create"))
        .collect();
    assert_eq!(run_calls.len(), 2);
    assert_eq!(run_calls[0], run_calls[1]);
}

#[test]
fn definitive_mutation_failure_is_persisted_and_never_reissued() {
    let repository = TempDir::new("definitive-repository");
    let state_parent = TempDir::new("definitive-state");
    inactive_repository(repository.path());
    let mut orca = ScriptedOrca::with_responses([
        status_receipt(),
        worktree_receipt(repository.path(), "repo-1::worktree"),
    ]);
    orca.responses
        .push_back(ScriptedResponse::Error(RunnerError::OrcaReceipt {
            message: "invalid objective".into(),
            receipt: json!({
                "ok": false,
                "error": {"code": "invalid_argument", "message": "invalid objective"}
            }),
        }));
    let mut runner =
        ProofRunner::new(repository.path(), state_parent.path().join("attempt"), orca).unwrap();

    assert!(matches!(
        runner.prepare(),
        Err(RunnerError::InvalidTransition(_))
    ));
    assert!(matches!(
        runner.resume(),
        Err(RunnerError::InvalidTransition(_))
    ));
    assert_eq!(command_count(&runner, "run-create"), 1);
    assert_eq!(
        runner
            .load_state()
            .unwrap()
            .pending_operation
            .unwrap()
            .status,
        OperationStatus::DefinitiveFailure
    );
}

#[test]
fn unknown_success_receipt_is_durable_and_does_not_duplicate_worker_start() {
    let (repository, _state_parent, mut runner) = prepared_attempt();
    runner.orca_mut().push_values([
        gate_receipt("resolved", Some("approved")),
        status_receipt(),
        worktree_receipt(repository.path(), "repo-1::worktree"),
        json!({"ok": true, "result": {"dispatch": {"id": "dispatch-1", "status": "ready"}}}),
        gate_receipt("resolved", Some("approved")),
        status_receipt(),
        worktree_receipt(repository.path(), "repo-1::worktree"),
    ]);

    assert!(matches!(runner.resume(), Err(RunnerError::Protocol(_))));
    assert_eq!(
        runner
            .load_state()
            .unwrap()
            .pending_operation
            .as_ref()
            .unwrap()
            .status,
        OperationStatus::ReceiptCaptured
    );
    assert!(matches!(runner.resume(), Err(RunnerError::Protocol(_))));
    assert_eq!(command_count(&runner, "worker-start"), 1);
}

#[test]
fn successful_worker_is_released_but_failed_worker_is_retained() {
    let (_repository, _state_parent, mut runner) = prepared_attempt();
    let mut state = runner.load_state().unwrap();
    state.phase = AttemptPhase::Running;
    state.dispatch_ids.push("dispatch-1".into());
    runner.save_state(&state).unwrap();
    runner.orca_mut().push_values([
        worker_show_receipt("dispatch-1", "succeeded"),
        worker_mutation_receipt("dispatch-1", "released"),
    ]);

    assert_eq!(
        runner.resume().unwrap(),
        ResumeOutcome::Completed {
            dispatch_id: "dispatch-1".into()
        }
    );
    let release = runner
        .orca()
        .calls
        .iter()
        .find(|call| call.get(1).map(String::as_str) == Some("worker-release"))
        .unwrap();
    assert_retry_request(release);

    let (_repository, _state_parent, mut runner) = prepared_attempt();
    let mut state = runner.load_state().unwrap();
    state.phase = AttemptPhase::Running;
    state.dispatch_ids.push("dispatch-2".into());
    runner.save_state(&state).unwrap();
    runner.orca_mut().push_values([
        worker_show_receipt("dispatch-2", "failed"),
        worker_mutation_receipt("dispatch-2", "retained"),
    ]);

    assert_eq!(
        runner.resume().unwrap(),
        ResumeOutcome::Failed {
            dispatch_id: "dispatch-2".into()
        }
    );
    let retain = runner
        .orca()
        .calls
        .iter()
        .find(|call| call.get(1).map(String::as_str) == Some("worker-retain"))
        .unwrap();
    assert_retry_request(retain);
}

#[test]
fn cancel_fences_the_dispatch_and_retry_names_the_exact_attempt() {
    let (repository, _state_parent, mut runner) = prepared_attempt();
    let mut state = runner.load_state().unwrap();
    state.phase = AttemptPhase::Running;
    state.dispatch_ids.push("dispatch-1".into());
    runner.save_state(&state).unwrap();
    runner.orca_mut().push_values([
        worker_mutation_receipt("dispatch-1", "stopped"),
        status_receipt(),
        worktree_receipt(repository.path(), "repo-1::worktree"),
        worker_start_receipt("dispatch-2", "ready"),
    ]);

    assert_eq!(
        runner.cancel().unwrap(),
        ResumeOutcome::Cancelled {
            dispatch_id: "dispatch-1".into()
        }
    );
    assert_eq!(
        runner.retry().unwrap(),
        ResumeOutcome::WorkerStarted {
            dispatch_id: "dispatch-2".into()
        }
    );

    let retry = runner
        .orca()
        .calls
        .iter()
        .find(|call| {
            call.windows(2)
                .any(|pair| pair == ["--retry-of", "dispatch-1"])
        })
        .expect("retry must carry prior dispatch provenance");
    assert_eq!(retry[0..2], ["orchestration", "worker-start"]);
    assert_retry_request(retry);
}

#[test]
fn rejected_gate_never_starts_a_worker() {
    let (_repository, _state_parent, mut runner) = prepared_attempt();
    runner
        .orca_mut()
        .push_values([gate_receipt("resolved", Some("rejected"))]);

    assert_eq!(
        runner.resume().unwrap(),
        ResumeOutcome::Rejected {
            gate_id: "gate-1".into()
        }
    );
    assert_eq!(runner.load_state().unwrap().phase, AttemptPhase::Rejected);
    assert_eq!(command_count(&runner, "worker-start"), 0);
}

#[test]
fn remote_worker_cancellation_is_observed_without_a_second_stop() {
    let (_repository, _state_parent, mut runner) = prepared_attempt();
    let mut state = runner.load_state().unwrap();
    state.phase = AttemptPhase::Running;
    state.dispatch_ids.push("dispatch-1".into());
    runner.save_state(&state).unwrap();
    runner
        .orca_mut()
        .push_values([worker_show_receipt("dispatch-1", "stopped")]);

    assert_eq!(
        runner.resume().unwrap(),
        ResumeOutcome::Cancelled {
            dispatch_id: "dispatch-1".into()
        }
    );
    assert_eq!(command_count(&runner, "worker-stop"), 0);
}

#[test]
fn launch_rejects_post_approval_package_or_worktree_drift() {
    let (repository, _state_parent, mut runner) = prepared_attempt();
    fs::write(
        repository
            .path()
            .join(".repository-engineering/package.toml"),
        "activation_eligibility = \"active\"\n",
    )
    .unwrap();
    runner
        .orca_mut()
        .push_values([gate_receipt("resolved", Some("approved"))]);
    assert!(matches!(
        runner.resume(),
        Err(RunnerError::PackageActive(_))
    ));
    assert_eq!(command_count(&runner, "worker-start"), 0);

    let (repository, _state_parent, mut runner) = prepared_attempt();
    runner.orca_mut().push_values([
        gate_receipt("resolved", Some("approved")),
        status_receipt(),
        worktree_receipt(repository.path(), "repo-1::different-worktree"),
    ]);
    assert!(matches!(
        runner.resume(),
        Err(RunnerError::InvalidTransition(_))
    ));
    assert_eq!(command_count(&runner, "worker-start"), 0);
}

#[test]
fn status_is_read_only_for_gate_and_dispatch_phases() {
    let (_repository, _state_parent, mut runner) = prepared_attempt();
    runner
        .orca_mut()
        .push_values([gate_receipt("pending", None)]);
    let before = fs::read(runner.state_root().join("attempt.json")).unwrap();
    let report = runner.status().unwrap();
    let after = fs::read(runner.state_root().join("attempt.json")).unwrap();
    assert_eq!(report.state.phase, AttemptPhase::AwaitingApproval);
    assert_eq!(before, after);
    assert_eq!(
        runner.orca().calls.last().unwrap(),
        &["orchestration", "gate-list", "--run", "run-1", "--json"]
    );

    let mut state = runner.load_state().unwrap();
    state.phase = AttemptPhase::Running;
    state.dispatch_ids.push("dispatch-1".into());
    runner.save_state(&state).unwrap();
    runner
        .orca_mut()
        .push_values([worker_show_receipt("dispatch-1", "ready")]);
    let before = fs::read(runner.state_root().join("attempt.json")).unwrap();
    let report = runner.status().unwrap();
    let after = fs::read(runner.state_root().join("attempt.json")).unwrap();
    assert_eq!(report.state.phase, AttemptPhase::Running);
    assert_eq!(before, after);
    assert_eq!(
        runner.orca().calls.last().unwrap(),
        &[
            "orchestration",
            "worker-show",
            "--dispatch",
            "dispatch-1",
            "--json"
        ]
    );
}

#[test]
fn mutation_lock_prevents_a_second_writer_from_reaching_orca() {
    let (_repository, _state_parent, mut runner) = prepared_attempt();
    fs::write(
        runner.state_root().join("attempt.lock"),
        "pid=someone-else\n",
    )
    .unwrap();
    let calls_before = runner.orca().calls.len();

    assert!(matches!(
        runner.resume(),
        Err(RunnerError::InvalidTransition(_))
    ));
    assert_eq!(runner.orca().calls.len(), calls_before);
}

#[test]
fn prepare_rejects_state_inside_repository_and_non_inert_package() {
    let repository = TempDir::new("repository");
    inactive_repository(repository.path());
    let result = ProofRunner::new(
        repository.path(),
        repository.path().join("state"),
        ScriptedOrca::default(),
    );
    assert!(matches!(result, Err(RunnerError::UnsafeStateRoot(_))));

    fs::write(
        repository
            .path()
            .join(".repository-engineering/package.toml"),
        "activation_eligibility = \"active\"\n",
    )
    .unwrap();
    let external = TempDir::new("external-state");
    let mut runner = ProofRunner::new(
        repository.path(),
        external.path().join("attempt"),
        ScriptedOrca::default(),
    )
    .unwrap();
    assert!(matches!(
        runner.prepare(),
        Err(RunnerError::PackageActive(_))
    ));
}

#[test]
fn second_prepare_cannot_create_duplicate_remote_resources() {
    let (repository, _state_parent, mut runner) = prepared_attempt();
    runner.orca_mut().push_values([
        status_receipt(),
        worktree_receipt(repository.path(), "repo-1::worktree"),
    ]);
    assert!(matches!(
        runner.prepare(),
        Err(RunnerError::InvalidTransition(_))
    ));
    assert_eq!(command_count(&runner, "run-create"), 1);
}

#[cfg(unix)]
fn executable_script(directory: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = directory.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}

#[cfg(unix)]
#[test]
fn process_orca_rejects_nonzero_exit_even_with_valid_success_json() {
    let directory = TempDir::new("process-nonzero");
    let executable = executable_script(
        directory.path(),
        "orca",
        "printf '%s\\n' '{\"ok\":true,\"result\":{}}'; exit 9",
    );
    let mut orca = ProcessOrca::new(executable);

    assert!(orca.call(&["status".into(), "--json".into()]).is_err());
}

#[cfg(unix)]
#[test]
fn process_orca_preserves_ok_false_json_receipt() {
    let directory = TempDir::new("process-false");
    let executable = executable_script(
        directory.path(),
        "orca",
        "printf '%s\\n' '{\"ok\":false,\"error\":{\"code\":\"runtime_timeout\",\"message\":\"timed out\"}}'",
    );
    let mut orca = ProcessOrca::new(executable);

    match orca.call(&["status".into(), "--json".into()]) {
        Err(RunnerError::OrcaReceipt { receipt, .. }) => {
            assert_eq!(
                receipt.pointer("/error/code").and_then(Value::as_str),
                Some("runtime_timeout")
            );
        }
        other => panic!("expected preserved Orca receipt, got {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn process_orca_rejects_malformed_json() {
    let directory = TempDir::new("process-malformed");
    let executable = executable_script(
        directory.path(),
        "orca",
        "printf '%s\\n' 'not-json'; exit 0",
    );
    let mut orca = ProcessOrca::new(executable);

    assert!(matches!(
        orca.call(&["status".into(), "--json".into()]),
        Err(RunnerError::Orca(_))
    ));
}
