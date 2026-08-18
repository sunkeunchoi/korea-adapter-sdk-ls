#[path = "support/fault_store.rs"]
mod fault_store;

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use fault_store::FailOnce;
use repository_engineering_runtime::adapters::checkpoint_fs::{
    CheckpointError, CheckpointFault, CheckpointFs, LocalFsTrust, NoFault,
};
use repository_engineering_runtime::model::{CheckpointGeneration, CheckpointRow, Phase};

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

fn generation(sequence: u64, parent: Option<String>, phase: Phase) -> CheckpointGeneration {
    CheckpointGeneration {
        schema_version: "v0".to_owned(),
        attempt_id: "attempt-1".to_owned(),
        parent_attempt_id: None,
        sequence,
        phase,
        parent_generation_digest: parent,
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
        rows: vec![CheckpointRow {
            row_id: "L1".to_owned(),
            source_available: true,
            dispatch_intent: None,
            result_capsule: None,
            completed: false,
        }],
        cancellation_fence: None,
        prepared_effects: Vec::new(),
        applied_effect_ids: Vec::new(),
    }
}

#[test]
fn publishes_immutable_generations_and_recovers_from_a_pinned_ancestor() {
    let directory = TestDirectory::new("checkpoint-chain");
    let mut store = CheckpointFs::new(&directory.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
        .expect("store");
    let first = store
        .create(generation(0, None, Phase::Discovering))
        .expect("initial publish");
    let second = store
        .publish(
            &first.generation_digest,
            generation(1, Some(first.generation_digest.clone()), Phase::Dispatching),
        )
        .expect("next publish");

    let recovered = store
        .recover(&first.generation_digest)
        .expect("ancestor pin is accepted");
    assert_eq!(recovered.head, second);
    assert_eq!(recovered.generation.sequence, 1);
    assert!(directory.0.join("checkpoint.lock").is_file());
    assert_eq!(
        fs::read_dir(directory.0.join("generations"))
            .expect("generations")
            .count(),
        2
    );
}

#[test]
fn caller_pin_mismatch_and_corrupt_generation_fail_closed() {
    let directory = TestDirectory::new("checkpoint-corrupt");
    let mut store = CheckpointFs::new(&directory.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
        .expect("store");
    let head = store
        .create(generation(0, None, Phase::Discovering))
        .expect("initial publish");
    assert_eq!(
        store.recover(&digest('f')).expect_err("stale pin"),
        CheckpointError::CallerPinMismatch
    );

    fs::write(store.generation_path(&head), b"{\"schema_version\":\"v0\"}")
        .expect("corrupt generation");
    assert_eq!(
        store
            .recover(&head.generation_digest)
            .expect_err("corruption must fail"),
        CheckpointError::RecoveryRequired
    );
}

#[test]
fn strict_head_decode_phase_regression_and_concurrent_publish_fail_closed() {
    let directory = TestDirectory::new("checkpoint-strict");
    let mut first_store =
        CheckpointFs::new(&directory.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
            .expect("store");
    let initial = first_store
        .create(generation(0, None, Phase::Discovering))
        .expect("initial publish");
    let dispatching = generation(
        1,
        Some(initial.generation_digest.clone()),
        Phase::Dispatching,
    );
    let current = first_store
        .publish(&initial.generation_digest, dispatching.clone())
        .expect("dispatching publish");
    let regressed = generation(
        2,
        Some(current.generation_digest.clone()),
        Phase::Discovering,
    );
    assert_eq!(
        first_store.publish(&current.generation_digest, regressed),
        Err(CheckpointError::InvalidGeneration)
    );

    let mut second_store =
        CheckpointFs::new(&directory.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
            .expect("second orchestrator");
    assert_eq!(
        second_store.publish(&initial.generation_digest, dispatching),
        Err(CheckpointError::ConcurrentUpdate)
    );

    let head_path = directory.0.join("head.json");
    let mut head: serde_json::Value =
        serde_json::from_slice(&fs::read(&head_path).expect("head bytes")).expect("head json");
    head.as_object_mut()
        .expect("head object")
        .insert("unknown".to_owned(), serde_json::Value::Bool(true));
    fs::write(&head_path, serde_json::to_vec(&head).expect("mutated head"))
        .expect("write mutated head");
    assert_eq!(
        second_store.recover(&current.generation_digest),
        Err(CheckpointError::RecoveryRequired)
    );
}

#[cfg(unix)]
#[test]
fn generation_directory_symlink_is_rejected_without_traversal() {
    use std::os::unix::fs::symlink;

    let directory = TestDirectory::new("checkpoint-parent-symlink");
    let outside = TestDirectory::new("checkpoint-parent-outside");
    let mut store = CheckpointFs::new(&directory.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
        .expect("store");
    let head = store
        .create(generation(0, None, Phase::Discovering))
        .expect("initial publish");
    fs::rename(
        directory.0.join("generations"),
        directory.0.join("real-generations"),
    )
    .expect("move real generations");
    symlink(&outside.0, directory.0.join("generations")).expect("symlink generations");

    assert_eq!(
        store.recover(&head.generation_digest),
        Err(CheckpointError::RecoveryRequired)
    );
    assert_eq!(fs::read_dir(&outside.0).expect("outside").count(), 0);
}

#[test]
fn publication_faults_leave_an_exact_old_or_recoverable_new_head() {
    for point in [
        CheckpointFault::BeforeGenerationCreate,
        CheckpointFault::AfterPartialGenerationWrite,
        CheckpointFault::BeforeGenerationSync,
        CheckpointFault::BeforeHeadReplace,
        CheckpointFault::AfterHeadReplace,
        CheckpointFault::BeforeDirectorySync,
        CheckpointFault::BeforeCanonicalReopen,
    ] {
        let directory = TestDirectory::new("checkpoint-fault");
        let mut healthy =
            CheckpointFs::new(&directory.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
                .expect("store");
        let first = healthy
            .create(generation(0, None, Phase::Discovering))
            .expect("initial publish");
        drop(healthy);

        let mut faulty = CheckpointFs::new(
            &directory.0,
            LocalFsTrust::TrustedSingleHostUnix,
            FailOnce::at(point),
        )
        .expect("store");
        let result = faulty.publish(
            &first.generation_digest,
            generation(1, Some(first.generation_digest.clone()), Phase::Dispatching),
        );
        assert!(result.is_err(), "{point:?} must be observable");
        drop(faulty);

        let mut recovered =
            CheckpointFs::new(&directory.0, LocalFsTrust::TrustedSingleHostUnix, NoFault)
                .expect("reopen");
        let state = recovered
            .recover(&first.generation_digest)
            .expect("old or new chain remains recoverable");
        assert!(state.generation.sequence <= 1);
    }
}
