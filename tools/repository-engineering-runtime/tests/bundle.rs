use std::path::{Path, PathBuf};

use repository_engineering_runtime::bundle::{load_bundle, BundleError};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "repository-engineering-runtime-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}

fn copied_bundle() -> TestDirectory {
    let source = repository_root();
    let target = TestDirectory::new("bundle");
    let manifest_path = source.join(".repository-engineering/runtime-bundle.json");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    let destination_manifest = target.0.join(".repository-engineering/runtime-bundle.json");
    std::fs::create_dir_all(destination_manifest.parent().unwrap()).unwrap();
    std::fs::copy(manifest_path, destination_manifest).unwrap();
    for member in manifest["members"].as_array().unwrap() {
        let relative = member["path"].as_str().unwrap();
        let destination = target.0.join(relative);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::copy(source.join(relative), destination).unwrap();
    }
    target
}

#[test]
fn copied_bundle_loads_without_repository_discovery() {
    let root = copied_bundle();
    let loaded = load_bundle(&root.0).unwrap();
    assert!(loaded
        .member(".repository-engineering/schema-registry.json")
        .is_some());
    assert!(loaded.member(".agents/skills/audit-row/SKILL.md").is_some());
}

#[test]
fn missing_mutated_and_unlisted_members_fail_closed() {
    let root = copied_bundle();
    let member = root
        .0
        .join(".repository-engineering/executors/audit-carried-rows.toml");
    std::fs::write(&member, "mutated\n").unwrap();
    assert_eq!(
        load_bundle(&root.0).unwrap_err(),
        BundleError::DigestMismatch
    );

    let root = copied_bundle();
    std::fs::remove_file(
        root.0
            .join(".repository-engineering/executors/audit-carried-rows.toml"),
    )
    .unwrap();
    assert_eq!(
        load_bundle(&root.0).unwrap_err(),
        BundleError::MemberMissing
    );

    let root = copied_bundle();
    std::fs::write(root.0.join("unlisted.json"), "{}\n").unwrap();
    assert_eq!(
        load_bundle(&root.0).unwrap_err(),
        BundleError::UnlistedMember
    );
}

#[cfg(unix)]
#[test]
fn symlinked_member_is_rejected_without_following_target() {
    use std::os::unix::fs::symlink;

    let root = copied_bundle();
    let member = root
        .0
        .join(".repository-engineering/executors/audit-carried-rows.toml");
    std::fs::remove_file(&member).unwrap();
    let outside = root.0.join("outside");
    std::fs::write(&outside, "must not be read\n").unwrap();
    symlink(&outside, &member).unwrap();
    assert_eq!(load_bundle(&root.0).unwrap_err(), BundleError::UnsafePath);
}
