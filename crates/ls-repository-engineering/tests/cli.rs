use std::path::PathBuf;

use ls_repository_engineering::cli::{parse_command, Command};
use ls_repository_engineering::generate::{
    check_projection_set, generate_projection_set, generate_projection_set_with_stop, Projection,
    ProjectionSet,
};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "ls-repository-engineering-{label}-{}-{}",
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

fn projections(alpha: &[u8]) -> ProjectionSet {
    ProjectionSet::new(vec![
        Projection::new("schemas/v0/alpha.json", alpha.to_vec()),
        Projection::new("reference.md", b"reference\n".to_vec()),
    ])
    .unwrap()
}

#[test]
fn cli_surface_contains_only_generation_and_non_writing_check() {
    assert_eq!(parse_command(["generate"]).unwrap(), Command::Generate);
    assert_eq!(parse_command(["check"]).unwrap(), Command::Check);
    for forbidden in [
        "install", "activate", "execute", "publish", "retire", "identity",
    ] {
        assert!(parse_command([forbidden]).is_err());
    }
}

#[test]
fn generate_then_check_is_exact_and_check_never_writes() {
    let root = TestDirectory::new("exact");
    let expected = projections(b"alpha\n");
    generate_projection_set(&root.0, &expected).unwrap();
    assert!(check_projection_set(&root.0, &expected).is_empty());

    std::fs::write(root.0.join("schemas/v0/alpha.json"), b"stale\n").unwrap();
    let before = std::fs::read(root.0.join("schemas/v0/alpha.json")).unwrap();
    let findings = check_projection_set(&root.0, &expected);
    assert!(findings
        .iter()
        .any(|finding| finding.code == "generated.artifact.stale"));
    assert_eq!(
        std::fs::read(root.0.join("schemas/v0/alpha.json")).unwrap(),
        before
    );
}

#[test]
fn validation_failure_writes_nothing_and_interruption_is_repairable() {
    let root = TestDirectory::new("repair");
    std::fs::write(root.0.join("sentinel"), b"preserve").unwrap();
    assert!(ProjectionSet::new(vec![Projection::new("../escape", vec![])]).is_err());
    assert_eq!(std::fs::read(root.0.join("sentinel")).unwrap(), b"preserve");

    let expected = projections(b"alpha\n");
    let interrupted = generate_projection_set_with_stop(&root.0, &expected, 1);
    assert!(interrupted.is_err());
    assert!(check_projection_set(&root.0, &expected)
        .iter()
        .any(|finding| finding.code == "generated.set_manifest.stale"));

    generate_projection_set(&root.0, &expected).unwrap();
    assert!(check_projection_set(&root.0, &expected).is_empty());
}
