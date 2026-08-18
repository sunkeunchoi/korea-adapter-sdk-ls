#[allow(dead_code)]
#[path = "support/scripted_host.rs"]
mod scripted_host;
#[path = "support/subprocess_host.rs"]
mod subprocess_host;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use repository_engineering_runtime::adapters::artifact_fs::ArtifactFs;
use repository_engineering_runtime::adapters::checkpoint_fs::{
    CheckpointFs, LocalFsTrust, NoFault,
};
use repository_engineering_runtime::adapters::effect_fs::EffectFs;
use repository_engineering_runtime::bundle::load_bundle;
use repository_engineering_runtime::contract::validate_portable_contract;
use repository_engineering_runtime::driver::{Driver, DriverError};
use repository_engineering_runtime::machine::SweepMachine;
use repository_engineering_runtime::model::{
    AcceptedResultCapsule, ArtifactReference, AuditRecord, AuditSuccessPayload, AuditVerdict,
    MachineError, RowInput, RunRequest, TerminalOutcome, WorkerResult,
};
use scripted_host::ScriptedHost;
use sha2::{Digest, Sha256};
use subprocess_host::{unique_pids, SubprocessHost};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "repository-engineering-runtime-e2e-{label}-{}-{}",
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

fn effect_root(label: &str) -> TestDirectory {
    let target = TestDirectory::new(label);
    let ledger = target
        .0
        .join(".repository-engineering/migration-ledger.toml");
    fs::create_dir_all(ledger.parent().expect("ledger parent")).expect("effect directories");
    fs::copy(
        repository_root().join(".repository-engineering/migration-ledger.toml"),
        ledger,
    )
    .expect("copy base ledger");
    target
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn file_digest(relative: &str) -> String {
    digest(&fs::read(repository_root().join(relative)).expect("identity input"))
}

fn request(attempt: &str, source_available: bool) -> RunRequest {
    let lock: serde_json::Value = serde_json::from_slice(
        &fs::read(repository_root().join(".repository-engineering/package.lock.json"))
            .expect("package lock"),
    )
    .expect("package lock JSON");
    RunRequest {
        schema_version: "v0".to_owned(),
        attempt_id: attempt.to_owned(),
        parent_attempt_id: None,
        idempotency_key: format!("{attempt}-idempotency"),
        package_lock_digest: lock["package_lock_id"]
            .as_str()
            .expect("package lock identity")
            .to_owned(),
        implementation_subject_digest: file_digest(
            ".repository-engineering/implementation-subjects/audit-carried-rows.json",
        ),
        capability_contract_digest: file_digest(
            ".repository-engineering/contracts/capabilities/audit-carried-rows.toml",
        ),
        worker_role_digest: file_digest(
            ".repository-engineering/contracts/workers/decommission-row-auditor.toml",
        ),
        executor_digest: file_digest(".repository-engineering/executors/audit-carried-rows.toml"),
        scenario_digest: file_digest(
            ".repository-engineering/scenarios/audit-carried-rows/implementation.toml",
        ),
        repository_snapshot_digest: digest(b"wave-1-fixture-repository-snapshot-v0"),
        row_manifest_digest: file_digest("docs/migration-source/audit/manifest.yaml"),
        base_ledger_digest: file_digest(".repository-engineering/migration-ledger.toml"),
        output_root_id: format!("{attempt}-output-root"),
        rows: (1..=26)
            .map(|index| RowInput {
                row_id: format!("L{index}"),
                source_available,
            })
            .collect(),
        global_concurrency_limit: 8,
    }
}

fn capsule(
    intent: &repository_engineering_runtime::model::DispatchIntent,
    attempt_id: &str,
    invocation_id: &str,
    assignment_id: &str,
    success_row_id: &str,
    verdict: AuditVerdict,
    stale_record_digest: bool,
) -> AcceptedResultCapsule {
    let record_bytes = serde_json::to_vec(&AuditRecord {
        schema_version: "v0".to_owned(),
        row_id: success_row_id.to_owned(),
        verdict,
    })
    .expect("record bytes");
    let receipt_bytes = format!("receipt:{invocation_id}").into_bytes();
    AcceptedResultCapsule {
        schema_version: "v0".to_owned(),
        result: WorkerResult::Succeeded {
            schema_version: "v0".to_owned(),
            attempt_id: attempt_id.to_owned(),
            invocation_id: invocation_id.to_owned(),
            assignment_id: assignment_id.to_owned(),
            worker_instance_id: intent.worker_instance_id.clone(),
            worker_instance_receipt: ArtifactReference {
                schema_version: "v0".to_owned(),
                path: format!("receipts/{invocation_id}.json"),
                sha256: digest(&receipt_bytes),
                media_type: "application/json".to_owned(),
            },
            payload: AuditSuccessPayload {
                row_id: success_row_id.to_owned(),
                verdict,
                record: ArtifactReference {
                    schema_version: "v0".to_owned(),
                    path: format!("records/{success_row_id}.json"),
                    sha256: if stale_record_digest {
                        digest(b"stale")
                    } else {
                        digest(&record_bytes)
                    },
                    media_type: "application/json".to_owned(),
                },
            },
        },
        record_bytes: Some(record_bytes),
        worker_instance_receipt_bytes: receipt_bytes,
    }
}

fn run_machine_case(source_available: bool, verdict: AuditVerdict) -> TerminalOutcome {
    let mut run = request("scenario-case", source_available);
    run.rows.truncate(1);
    let base = run.base_ledger_digest.clone();
    let mut machine = SweepMachine::new(run).expect("scenario machine");
    machine.begin_dispatch().expect("begin dispatch");
    let intent = machine.request_dispatches().expect("dispatch").remove(0);
    let result = capsule(
        &intent,
        &intent.attempt_id,
        &intent.invocation_id,
        &intent.assignment_id,
        &intent.row_id,
        verdict,
        false,
    );
    machine.accept_capsule(&result).expect("accept result");
    machine.finish_roll_up(&base).expect("finish roll-up");
    machine.complete().expect("complete").outcome
}

#[test]
fn scenario_catalog_is_exhaustively_bound_to_runtime_cases() {
    let copied = copied_bundle();
    let loaded = load_bundle(&copied.0).expect("closed bundle");
    let contract = validate_portable_contract(&loaded).expect("portable contract");
    for case_id in &contract.scenario_ids {
        match case_id.as_str() {
            "complete-confirmed-corpus" => {
                assert_eq!(
                    run_machine_case(true, AuditVerdict::Confirmed),
                    TerminalOutcome::Succeeded
                )
            }
            "complete-unverifiable-corpus" | "unavailable-required-source" => {
                assert_eq!(
                    run_machine_case(false, AuditVerdict::Unverifiable),
                    TerminalOutcome::Held
                )
            }
            "empty-row-manifest" => {
                let mut run = request("empty-case", true);
                run.rows.clear();
                assert_eq!(
                    SweepMachine::new(run).expect_err("empty must fail"),
                    MachineError::InvalidRequest
                );
            }
            "duplicate-row-id" => {
                let mut run = request("duplicate-case", true);
                run.rows = vec![run.rows[0].clone(), run.rows[0].clone()];
                assert_eq!(
                    SweepMachine::new(run).expect_err("duplicate must fail"),
                    MachineError::DuplicateRow
                );
            }
            "identical-terminal-replay"
            | "conflicting-terminal-replay"
            | "wrong-attempt-id"
            | "wrong-invocation-id"
            | "wrong-assignment-id"
            | "wrong-success-row-id"
            | "stale-record-digest" => {
                let mut run = request(&format!("{case_id}-case"), true);
                run.rows.truncate(1);
                let mut machine = SweepMachine::new(run).expect("scenario machine");
                machine.begin_dispatch().expect("begin dispatch");
                let intent = machine.request_dispatches().expect("dispatch").remove(0);
                let baseline = capsule(
                    &intent,
                    &intent.attempt_id,
                    &intent.invocation_id,
                    &intent.assignment_id,
                    &intent.row_id,
                    AuditVerdict::Confirmed,
                    false,
                );
                match case_id.as_str() {
                    "identical-terminal-replay" => {
                        machine.accept_capsule(&baseline).expect("first result");
                        machine.accept_capsule(&baseline).expect("identical replay");
                    }
                    "conflicting-terminal-replay" => {
                        machine.accept_capsule(&baseline).expect("first result");
                        let conflict = capsule(
                            &intent,
                            &intent.attempt_id,
                            &intent.invocation_id,
                            &intent.assignment_id,
                            &intent.row_id,
                            AuditVerdict::Refuted,
                            false,
                        );
                        assert_eq!(
                            machine.accept_capsule(&conflict),
                            Err(MachineError::ConflictingReplay)
                        );
                    }
                    other => {
                        let changed = capsule(
                            &intent,
                            if other == "wrong-attempt-id" {
                                "wrong-attempt"
                            } else {
                                &intent.attempt_id
                            },
                            if other == "wrong-invocation-id" {
                                "wrong-invocation"
                            } else {
                                &intent.invocation_id
                            },
                            if other == "wrong-assignment-id" {
                                "L2"
                            } else {
                                &intent.assignment_id
                            },
                            if other == "wrong-success-row-id" {
                                "L2"
                            } else {
                                &intent.row_id
                            },
                            AuditVerdict::Confirmed,
                            other == "stale-record-digest",
                        );
                        assert!(machine.accept_capsule(&changed).is_err());
                    }
                }
            }
            unknown => panic!("unbound implementation scenario: {unknown}"),
        }
    }
    assert_eq!(contract.scenario_ids.len(), 12);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closed_bundle_drives_all_26_rows_through_both_fixture_hosts() {
    let copied = copied_bundle();
    let loaded = load_bundle(&copied.0).expect("closed bundle");
    let contract = validate_portable_contract(&loaded).expect("portable contract");
    assert_eq!(contract.scenario_ids.len(), 12);

    let state = TestDirectory::new("scripted-state");
    let artifacts = TestDirectory::new("scripted-artifacts");
    let effects = effect_root("scripted-effects");
    let scripted = ScriptedHost::new(Duration::from_millis(1));
    let mut driver = Driver::new(
        scripted.clone(),
        ArtifactFs::new(&artifacts.0).expect("artifact store"),
        CheckpointFs::new(&state.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
            .expect("checkpoint store"),
        EffectFs::new(&effects.0).expect("effect store"),
    );
    let (_cancel, receiver) = tokio::sync::watch::channel(false);
    let result = driver
        .run(request("wave1-scripted", true), receiver)
        .await
        .expect("scripted sweep");
    assert_eq!(result.terminal.outcome, TerminalOutcome::Succeeded);
    assert_eq!(result.terminal.rows.len(), 26);
    assert!(result.terminal.rows.iter().all(|row| row.completed));
    assert_eq!(scripted.maximum_active(), 2);
    assert_eq!(scripted.active(), 0);
    let report_path = effects.0.join("reports/wave1-scripted.json");
    assert!(!fs::read(&report_path).expect("roll-up report").is_empty());
    let mut checkpoints = CheckpointFs::new(&state.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
        .expect("reopen checkpoints");
    let recovered = checkpoints
        .recover(&result.head.generation_digest)
        .expect("recover effect checkpoint");
    assert_eq!(recovered.generation.prepared_effects.len(), 1);
    assert_eq!(recovered.generation.applied_effect_ids.len(), 1);
    assert_eq!(
        recovered.generation.prepared_effects[0].effect_id,
        recovered.generation.applied_effect_ids[0]
    );

    let state = TestDirectory::new("process-state");
    let artifacts = TestDirectory::new("process-artifacts");
    let effects = effect_root("process-effects");
    let work = TestDirectory::new("process-work");
    let process = SubprocessHost::new(
        env!("CARGO_BIN_EXE_fixture-worker"),
        &work.0,
        &copied.0,
        Duration::from_secs(120),
    )
    .expect("process host");
    let mut driver = Driver::new(
        process.clone(),
        ArtifactFs::new(&artifacts.0).expect("artifact store"),
        CheckpointFs::new(&state.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
            .expect("checkpoint store"),
        EffectFs::new(&effects.0).expect("effect store"),
    );
    let (_cancel, receiver) = tokio::sync::watch::channel(false);
    let result = driver
        .run(request("wave1-process", true), receiver)
        .await
        .expect("process sweep");
    assert_eq!(result.terminal.outcome, TerminalOutcome::Succeeded);
    assert_eq!(result.terminal.rows.len(), 26);
    assert_eq!(process.maximum_active(), 2);
    assert_eq!(process.active(), 0);
    assert_eq!(process.observations().len(), 26);
    assert_eq!(unique_pids(&process.observations()), 26);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unavailable_source_produces_complete_unverifiable_rows_and_a_held_capability() {
    let state = TestDirectory::new("unavailable-state");
    let artifacts = TestDirectory::new("unavailable-artifacts");
    let effects = effect_root("unavailable-effects");
    let host = ScriptedHost::new(Duration::from_millis(1)).with_verdict(AuditVerdict::Unverifiable);
    let mut driver = Driver::new(
        host,
        ArtifactFs::new(&artifacts.0).expect("artifact store"),
        CheckpointFs::new(&state.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
            .expect("checkpoint store"),
        EffectFs::new(&effects.0).expect("effect store"),
    );
    let (_cancel, receiver) = tokio::sync::watch::channel(false);
    let result = driver
        .run(request("wave1-unavailable", false), receiver)
        .await
        .expect("unavailable-source sweep");
    assert_eq!(result.terminal.outcome, TerminalOutcome::Held);
    assert_eq!(result.terminal.rows.len(), 26);
    assert!(result
        .terminal
        .rows
        .iter()
        .all(|row| { row.completed && row.verdict == Some(AuditVerdict::Unverifiable) }));

    let copied = copied_bundle();
    let state = TestDirectory::new("unavailable-process-state");
    let artifacts = TestDirectory::new("unavailable-process-artifacts");
    let effects = effect_root("unavailable-process-effects");
    let work = TestDirectory::new("unavailable-process-work");
    let host = SubprocessHost::new(
        env!("CARGO_BIN_EXE_fixture-worker"),
        &work.0,
        &copied.0,
        Duration::from_secs(120),
    )
    .expect("process host")
    .with_mode("unverifiable");
    let mut driver = Driver::new(
        host.clone(),
        ArtifactFs::new(&artifacts.0).expect("artifact store"),
        CheckpointFs::new(&state.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
            .expect("checkpoint store"),
        EffectFs::new(&effects.0).expect("effect store"),
    );
    let (_cancel, receiver) = tokio::sync::watch::channel(false);
    let result = driver
        .run(request("wave1-unavailable-process", false), receiver)
        .await
        .expect("unavailable subprocess sweep");
    assert_eq!(result.terminal.outcome, TerminalOutcome::Held);
    assert!(result
        .terminal
        .rows
        .iter()
        .all(|row| row.completed && row.verdict == Some(AuditVerdict::Unverifiable)));
    assert_eq!(host.observations().len(), 26);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn filesystem_resume_and_cancellation_preserve_the_26_row_boundary() {
    let state = TestDirectory::new("resume-state");
    let artifacts = TestDirectory::new("resume-artifacts");
    let effects = effect_root("resume-effects");
    let resume_request = request("wave1-resume", true);
    let host = ScriptedHost::new(Duration::from_millis(1));
    let mut driver = Driver::new(
        host.clone(),
        ArtifactFs::new(&artifacts.0).expect("artifact store"),
        CheckpointFs::new(&state.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
            .expect("checkpoint store"),
        EffectFs::new(&effects.0).expect("effect store"),
    );
    let (_cancel, receiver) = tokio::sync::watch::channel(false);
    let first = driver
        .run(resume_request.clone(), receiver)
        .await
        .expect("initial sweep");
    drop(driver);

    let mut resumed = Driver::new(
        host,
        ArtifactFs::new(&artifacts.0).expect("artifact store"),
        CheckpointFs::new(&state.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
            .expect("checkpoint store"),
        EffectFs::new(&effects.0).expect("effect store"),
    );
    let (_cancel, receiver) = tokio::sync::watch::channel(false);
    let resumed = resumed
        .resume(
            resume_request.clone(),
            &first.head.generation_digest,
            receiver,
        )
        .await
        .expect("resume final checkpoint");
    assert_eq!(resumed.terminal, first.terminal);

    fs::remove_file(effects.0.join("reports/wave1-resume.json"))
        .expect("simulate applied-effect rollback");
    let mut rolled_back = Driver::new(
        ScriptedHost::new(Duration::from_millis(1)),
        ArtifactFs::new(&artifacts.0).expect("artifact store"),
        CheckpointFs::new(&state.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
            .expect("checkpoint store"),
        EffectFs::new(&effects.0).expect("effect store"),
    );
    let (_cancel, receiver) = tokio::sync::watch::channel(false);
    assert_eq!(
        rolled_back
            .resume(resume_request, &first.head.generation_digest, receiver)
            .await,
        Err(DriverError::RecoveryRequired)
    );

    let state = TestDirectory::new("cancel-state");
    let artifacts = TestDirectory::new("cancel-artifacts");
    let effects = effect_root("cancel-effects");
    let work = TestDirectory::new("cancel-work");
    let copied = copied_bundle();
    let host = SubprocessHost::new(
        env!("CARGO_BIN_EXE_fixture-worker"),
        &work.0,
        &copied.0,
        Duration::from_secs(120),
    )
    .expect("process host")
    .with_mode("hang");
    let mut driver = Driver::new(
        host.clone(),
        ArtifactFs::new(&artifacts.0).expect("artifact store"),
        CheckpointFs::new(&state.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
            .expect("checkpoint store"),
        EffectFs::new(&effects.0).expect("effect store"),
    );
    let (cancel, receiver) = tokio::sync::watch::channel(false);
    let run =
        tokio::spawn(async move { driver.run(request("wave1-cancel", true), receiver).await });
    for _ in 0..5000 {
        if host.active() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert_eq!(host.active(), 2);
    cancel.send(true).expect("cancellation fence");
    let cancelled = run.await.expect("driver task").expect("cancelled sweep");
    assert_eq!(cancelled.terminal.outcome, TerminalOutcome::Cancelled);
    assert_eq!(cancelled.terminal.rows.len(), 26);
    assert!(cancelled.terminal.rows.iter().all(|row| !row.completed));
    assert_eq!(host.active(), 0);
}
