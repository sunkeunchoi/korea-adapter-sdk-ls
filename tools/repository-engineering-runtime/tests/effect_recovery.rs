use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use repository_engineering_runtime::adapters::artifact_fs::ArtifactFs;
use repository_engineering_runtime::adapters::effect_fs::{ApplyOutcome, EffectError, EffectFs};
use repository_engineering_runtime::model::EffectEntry;
use repository_engineering_runtime::ports::ArtifactStore;
use sha2::{Digest, Sha256};

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

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn effect(before: Option<&[u8]>, after: &[u8]) -> EffectEntry {
    EffectEntry {
        schema_version: "v0".to_owned(),
        effect_id: "ledger-roll-up".to_owned(),
        relative_target: "docs/migration-source/EXTRACTION-LEDGER.md".to_owned(),
        expected_before_digest: before.map(digest),
        after_bytes: after.to_vec(),
        after_digest: digest(after),
        base_ledger_digest: digest(before.unwrap_or_default()),
    }
}

#[test]
fn immutable_artifacts_are_confined_and_create_new() {
    let directory = TestDirectory::new("artifacts");
    let mut store = ArtifactFs::new(&directory.0).expect("store");
    store
        .create("capsules/row-1.json", b"accepted")
        .expect("create capsule");
    assert_eq!(
        store.read("capsules/row-1.json").expect("read capsule"),
        b"accepted"
    );
    assert!(store.create("capsules/row-1.json", b"changed").is_err());
    assert!(store.create("../escape", b"no").is_err());
}

#[test]
fn effects_apply_only_from_before_and_checkpoint_an_existing_after() {
    let directory = TestDirectory::new("effects");
    let target = directory
        .0
        .join("docs/migration-source/EXTRACTION-LEDGER.md");
    fs::create_dir_all(target.parent().expect("parent")).expect("parents");
    fs::write(&target, b"before").expect("seed target");
    let entry = effect(Some(b"before"), b"after");
    let mut effects = EffectFs::new(&directory.0).expect("effects");

    assert_eq!(effects.apply(&entry).expect("apply"), ApplyOutcome::Applied);
    assert_eq!(
        effects.apply(&entry).expect("resume after"),
        ApplyOutcome::AlreadyApplied
    );
    assert_eq!(fs::read(target).expect("target"), b"after");
}

#[test]
fn effect_conflicts_and_duplicate_targets_fail_closed() {
    let directory = TestDirectory::new("effect-conflict");
    let target = directory.0.join("ledger.md");
    fs::write(&target, b"other").expect("seed target");
    let mut effects = EffectFs::new(&directory.0).expect("effects");
    let entry = EffectEntry {
        relative_target: "ledger.md".to_owned(),
        ..effect(Some(b"before"), b"after")
    };
    assert_eq!(effects.apply(&entry), Err(EffectError::StateConflict));
    let duplicate_target = EffectEntry {
        effect_id: "second-effect".to_owned(),
        ..entry.clone()
    };
    assert_eq!(
        effects.validate_plan(&[entry, duplicate_target]),
        Err(EffectError::DuplicateTarget)
    );
}

#[cfg(unix)]
#[test]
fn symlink_targets_are_rejected_without_following_them() {
    use std::os::unix::fs::symlink;

    let directory = TestDirectory::new("effect-symlink");
    let outside = TestDirectory::new("effect-outside");
    fs::write(outside.0.join("target"), b"outside").expect("outside");
    symlink(outside.0.join("target"), directory.0.join("ledger.md")).expect("symlink");
    let mut effects = EffectFs::new(&directory.0).expect("effects");
    let entry = EffectEntry {
        relative_target: "ledger.md".to_owned(),
        ..effect(Some(b"outside"), b"after")
    };
    assert!(effects.apply(&entry).is_err());
    assert_eq!(
        fs::read(outside.0.join("target")).expect("outside"),
        b"outside"
    );

    let artifact_root = TestDirectory::new("artifact-parent-symlink");
    fs::create_dir(outside.0.join("capsules")).expect("outside capsules");
    fs::write(outside.0.join("capsules/row.json"), b"outside").expect("outside capsule");
    symlink(outside.0.join("capsules"), artifact_root.0.join("capsules"))
        .expect("artifact parent symlink");
    let mut artifacts = ArtifactFs::new(&artifact_root.0).expect("artifact store");
    assert!(artifacts.read("capsules/row.json").is_err());
    assert!(artifacts.create("capsules/new.json", b"new").is_err());
    assert!(!outside.0.join("capsules/new.json").exists());
}
