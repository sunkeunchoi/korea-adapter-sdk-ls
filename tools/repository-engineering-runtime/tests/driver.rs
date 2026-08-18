#[path = "support/effect_store.rs"]
mod effect_store;
#[allow(dead_code)]
#[path = "support/scripted_host.rs"]
mod scripted_host;
#[path = "support/subprocess_host.rs"]
mod subprocess_host;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use effect_store::MemoryEffects;
use repository_engineering_runtime::adapters::artifact_fs::ArtifactFs;
use repository_engineering_runtime::adapters::checkpoint_fs::{
    CheckpointFs, LocalFsTrust, NoFault,
};
use repository_engineering_runtime::driver::{Driver, DriverError};
use repository_engineering_runtime::machine::SweepMachine;
use repository_engineering_runtime::model::{
    CheckpointGeneration, Phase, RowInput, RunRequest, TerminalOutcome,
};
use scripted_host::{RecoveryMode, ScriptedHost};
use subprocess_host::{unique_pids, SubprocessHost};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "repository-engineering-runtime-{label}-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create test directory");
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn digest(byte: char) -> String {
    format!("sha256:{}", byte.to_string().repeat(64))
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repository root")
        .to_path_buf()
}

fn copied_bundle() -> TestDirectory {
    let source = repository_root();
    let target = TestDirectory::new("bundle");
    let manifest_path = source.join(".repository-engineering/runtime-bundle.json");
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("bundle manifest"))
            .expect("bundle JSON");
    let destination_manifest = target.0.join(".repository-engineering/runtime-bundle.json");
    fs::create_dir_all(destination_manifest.parent().expect("manifest parent"))
        .expect("manifest parent directories");
    fs::copy(manifest_path, destination_manifest).expect("copy manifest");
    for member in manifest["members"].as_array().expect("bundle members") {
        let relative = member["path"].as_str().expect("member path");
        let destination = target.0.join(relative);
        fs::create_dir_all(destination.parent().expect("member parent"))
            .expect("member parent directories");
        fs::copy(source.join(relative), destination).expect("copy member");
    }
    target
}

fn request(limit: usize, row_count: usize) -> RunRequest {
    RunRequest {
        schema_version: "v0".to_owned(),
        attempt_id: format!("attempt-{limit}-{row_count}"),
        parent_attempt_id: None,
        idempotency_key: format!("idempotency-{limit}-{row_count}"),
        package_lock_digest: digest('1'),
        implementation_subject_digest: digest('2'),
        capability_contract_digest: digest('3'),
        worker_role_digest: digest('9'),
        executor_digest: digest('4'),
        scenario_digest: digest('5'),
        repository_snapshot_digest: digest('6'),
        row_manifest_digest: digest('7'),
        base_ledger_digest: digest('8'),
        output_root_id: "test-output-root".to_owned(),
        rows: (1..=row_count)
            .map(|index| RowInput {
                row_id: format!("L{index}"),
                source_available: true,
            })
            .collect(),
        global_concurrency_limit: limit,
    }
}

fn driver(
    host: ScriptedHost,
    state: &TestDirectory,
    artifacts: &TestDirectory,
) -> Driver<ScriptedHost, ArtifactFs, CheckpointFs, MemoryEffects> {
    Driver::new(
        host,
        ArtifactFs::new(&artifacts.0).expect("artifact store"),
        CheckpointFs::new(&state.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
            .expect("checkpoint store"),
        MemoryEffects::new(digest('8')),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn configured_limit_bounds_the_owned_task_set() {
    for (limit, expected_maximum) in [(1, 1), (99, 2)] {
        let state = TestDirectory::new("driver-state");
        let artifacts = TestDirectory::new("driver-artifacts");
        let host = ScriptedHost::new(Duration::from_millis(2));
        let mut driver = driver(host.clone(), &state, &artifacts);
        let (_cancel, receiver) = tokio::sync::watch::channel(false);

        let result = driver.run(request(limit, 8), receiver).await.expect("run");
        assert_eq!(result.terminal.outcome, TerminalOutcome::Succeeded);
        assert_eq!(result.terminal.rows.len(), 8);
        assert_eq!(host.maximum_active(), expected_maximum);
        assert_eq!(host.active(), 0);

        let mut checkpoints =
            CheckpointFs::new(&state.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
                .expect("reopen checkpoints");
        let recovered = checkpoints
            .recover(&result.head.generation_digest)
            .expect("recover final checkpoint");
        assert_eq!(recovered.generation.phase, Phase::Complete);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_fences_late_results_and_drains_every_task() {
    let state = TestDirectory::new("cancel-state");
    let artifacts = TestDirectory::new("cancel-artifacts");
    let host = ScriptedHost::new(Duration::from_secs(30));
    let mut driver = driver(host.clone(), &state, &artifacts);
    let (cancel, receiver) = tokio::sync::watch::channel(false);
    let run = tokio::spawn(async move { driver.run(request(8, 6), receiver).await });
    host.wait_for_started(2).await;
    cancel.send(true).expect("cancel signal");

    let result = run.await.expect("driver task").expect("cancelled run");
    assert_eq!(result.terminal.outcome, TerminalOutcome::Cancelled);
    assert!(result.terminal.rows.iter().all(|row| !row.completed));
    assert_eq!(host.active(), 0);
    assert_eq!(host.maximum_active(), 2);

    let mut checkpoints = CheckpointFs::new(&state.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
        .expect("reopen checkpoints");
    let recovered = checkpoints
        .recover(&result.head.generation_digest)
        .expect("recover cancellation");
    assert_eq!(recovered.generation.phase, Phase::Cancelled);
    assert!(recovered.generation.cancellation_fence.is_some());
    assert!(!artifacts.0.join("capsules").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_errors_still_signal_and_drain_every_task() {
    let state = TestDirectory::new("cancel-error-state");
    let artifacts = TestDirectory::new("cancel-error-artifacts");
    let host = ScriptedHost::new(Duration::from_secs(60)).failing_cancel();
    let mut driver = driver(host.clone(), &state, &artifacts);
    let (cancel, receiver) = tokio::sync::watch::channel(false);
    let run = tokio::spawn(async move { driver.run(request(8, 2), receiver).await });
    host.wait_for_started(2).await;
    cancel.send(true).expect("cancel");
    assert_eq!(run.await.expect("driver join"), Err(DriverError::Host));
    assert_eq!(host.cancel_calls(), 2);
    assert_eq!(host.active(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_failure_requires_recovery_without_fabricating_a_verdict() {
    let state = TestDirectory::new("failure-state");
    let artifacts = TestDirectory::new("failure-artifacts");
    let host = ScriptedHost::new(Duration::from_millis(2)).failing("L1");
    let mut driver = driver(host.clone(), &state, &artifacts);
    let (_cancel, receiver) = tokio::sync::watch::channel(false);

    assert_eq!(
        driver.run(request(2, 4), receiver).await,
        Err(DriverError::Host)
    );
    assert_eq!(host.active(), 0);
    let head_bytes = fs::read(state.0.join("head.json")).expect("recovery head");
    assert!(String::from_utf8(head_bytes)
        .expect("utf8 head")
        .contains("generation_digest"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_revalidates_every_referenced_result_capsule() {
    let state = TestDirectory::new("capsule-state");
    let artifacts = TestDirectory::new("capsule-artifacts");
    let request = request(2, 2);
    let host = ScriptedHost::new(Duration::from_millis(2));
    let mut first_driver = driver(host.clone(), &state, &artifacts);
    let (_cancel, receiver) = tokio::sync::watch::channel(false);
    let result = first_driver
        .run(request.clone(), receiver)
        .await
        .expect("initial run");
    drop(first_driver);

    fs::write(
        artifacts
            .0
            .join(format!("capsules/{}/L1.json", request.attempt_id)),
        b"changed",
    )
    .expect("corrupt capsule");
    let mut resumed = driver(host, &state, &artifacts);
    let (_cancel, receiver) = tokio::sync::watch::channel(false);
    assert_eq!(
        resumed
            .resume(request, &result.head.generation_digest, receiver)
            .await,
        Err(DriverError::RecoveryRequired)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_retains_invocation_identity_and_unknown_never_redispatches() {
    for mode in [
        RecoveryMode::NeverStarted,
        RecoveryMode::Running,
        RecoveryMode::Terminal,
    ] {
        let state = TestDirectory::new("resume-state");
        let artifacts = TestDirectory::new("resume-artifacts");
        let request = request(2, 2);
        let head = seed_dispatched_checkpoint(&state, &request);
        let host = ScriptedHost::new(Duration::from_millis(2)).recovering_as(mode);
        let mut driver = driver(host.clone(), &state, &artifacts);
        let (_cancel, receiver) = tokio::sync::watch::channel(false);

        let result = driver
            .resume(request, &head.generation_digest, receiver)
            .await
            .expect("resume");
        assert_eq!(result.terminal.outcome, TerminalOutcome::Succeeded);
        assert_eq!(host.active(), 0);
    }

    let state = TestDirectory::new("unknown-state");
    let artifacts = TestDirectory::new("unknown-artifacts");
    let request = request(2, 2);
    let head = seed_dispatched_checkpoint(&state, &request);
    let host = ScriptedHost::new(Duration::from_millis(2)).recovering_as(RecoveryMode::Unknown);
    let mut driver = driver(host.clone(), &state, &artifacts);
    let (_cancel, receiver) = tokio::sync::watch::channel(false);
    assert_eq!(
        driver
            .resume(request, &head.generation_digest, receiver)
            .await,
        Err(DriverError::RecoveryRequired)
    );
    assert_eq!(host.maximum_active(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn subprocess_workers_are_fresh_confined_bounded_and_reaped() {
    let state = TestDirectory::new("subprocess-state");
    let artifacts = TestDirectory::new("subprocess-artifacts");
    let work = TestDirectory::new("subprocess-work");
    let bundle = copied_bundle();
    let host = SubprocessHost::new(
        env!("CARGO_BIN_EXE_fixture-worker"),
        &work.0,
        &bundle.0,
        Duration::from_secs(120),
    )
    .expect("subprocess host");
    let mut driver = Driver::new(
        host.clone(),
        ArtifactFs::new(&artifacts.0).expect("artifacts"),
        CheckpointFs::new(&state.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
            .expect("checkpoints"),
        MemoryEffects::new(digest('8')),
    );
    let (_cancel, receiver) = tokio::sync::watch::channel(false);
    let result = driver.run(request(8, 4), receiver).await.expect("run");
    assert_eq!(result.terminal.outcome, TerminalOutcome::Succeeded);
    assert_eq!(host.maximum_active(), 2);
    assert_eq!(host.active(), 0);
    let observations = host.observations();
    assert_eq!(observations.len(), 4);
    assert_eq!(unique_pids(&observations), 4);
    assert!(observations.iter().all(|observation| observation.cwd_empty));
    assert!(observations
        .iter()
        .all(|observation| observation.environment == ["FIXTURE_ALLOWED"]));

    for mode in ["hang", "oversized"] {
        let state = TestDirectory::new("hostile-state");
        let artifacts = TestDirectory::new("hostile-artifacts");
        let work = TestDirectory::new("hostile-work");
        let bundle = copied_bundle();
        let hostile = SubprocessHost::new(
            env!("CARGO_BIN_EXE_fixture-worker"),
            &work.0,
            &bundle.0,
            Duration::from_millis(100),
        )
        .expect("hostile host")
        .with_mode(mode);
        let mut driver = Driver::new(
            hostile.clone(),
            ArtifactFs::new(&artifacts.0).expect("artifacts"),
            CheckpointFs::new(&state.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
                .expect("checkpoints"),
            MemoryEffects::new(digest('8')),
        );
        let (_cancel, receiver) = tokio::sync::watch::channel(false);
        assert_eq!(
            driver.run(request(1, 1), receiver).await,
            Err(DriverError::Host)
        );
        assert_eq!(hostile.active(), 0);
    }

    let state = TestDirectory::new("subprocess-cancel-state");
    let artifacts = TestDirectory::new("subprocess-cancel-artifacts");
    let work = TestDirectory::new("subprocess-cancel-work");
    let bundle = copied_bundle();
    let cancellable = SubprocessHost::new(
        env!("CARGO_BIN_EXE_fixture-worker"),
        &work.0,
        &bundle.0,
        Duration::from_secs(120),
    )
    .expect("cancellable host")
    .with_mode("hang");
    let mut driver = Driver::new(
        cancellable.clone(),
        ArtifactFs::new(&artifacts.0).expect("artifacts"),
        CheckpointFs::new(&state.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
            .expect("checkpoints"),
        MemoryEffects::new(digest('8')),
    );
    let (cancel, receiver) = tokio::sync::watch::channel(false);
    let run = tokio::spawn(async move { driver.run(request(8, 2), receiver).await });
    for _ in 0..5000 {
        if cancellable.active() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert_eq!(cancellable.active(), 2);
    cancel.send(true).expect("cancel subprocesses");
    let result = run.await.expect("driver task").expect("cancelled run");
    assert_eq!(result.terminal.outcome, TerminalOutcome::Cancelled);
    assert_eq!(cancellable.active(), 0);
}

fn seed_dispatched_checkpoint(
    state: &TestDirectory,
    request: &RunRequest,
) -> repository_engineering_runtime::model::PublishedHead {
    let mut checkpoints = CheckpointFs::new(&state.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
        .expect("checkpoint store");
    let mut machine = SweepMachine::new(request.clone()).expect("machine");
    let first = checkpoints
        .create(generation(&machine, 0, None))
        .expect("discovering checkpoint");
    machine.begin_dispatch().expect("dispatch phase");
    machine.request_dispatches().expect("dispatch intents");
    checkpoints
        .publish(
            &first.generation_digest,
            generation(&machine, 1, Some(first.generation_digest.clone())),
        )
        .expect("dispatch checkpoint")
}

fn generation(
    machine: &SweepMachine,
    sequence: u64,
    parent_generation_digest: Option<String>,
) -> CheckpointGeneration {
    let request = machine.request();
    CheckpointGeneration {
        schema_version: "v0".to_owned(),
        attempt_id: request.attempt_id.clone(),
        parent_attempt_id: request.parent_attempt_id.clone(),
        sequence,
        phase: machine.phase(),
        parent_generation_digest,
        package_lock_digest: request.package_lock_digest.clone(),
        implementation_subject_digest: request.implementation_subject_digest.clone(),
        capability_contract_digest: request.capability_contract_digest.clone(),
        worker_role_digest: request.worker_role_digest.clone(),
        executor_digest: request.executor_digest.clone(),
        scenario_digest: request.scenario_digest.clone(),
        repository_snapshot_digest: request.repository_snapshot_digest.clone(),
        row_manifest_digest: request.row_manifest_digest.clone(),
        base_ledger_digest: request.base_ledger_digest.clone(),
        output_root_id: request.output_root_id.clone(),
        rows: machine
            .checkpoint_rows(&BTreeMap::new())
            .expect("checkpoint rows"),
        cancellation_fence: None,
        prepared_effects: Vec::new(),
        applied_effect_ids: Vec::new(),
    }
}
