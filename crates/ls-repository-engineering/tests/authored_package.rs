use std::path::Path;

use ls_repository_engineering::generate::{check_projection_set, generate_projection_set};
use ls_repository_engineering::repository::compose_repository;

fn repository_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
}

#[test]
fn real_package_projects_a_complete_deterministic_closed_set() {
    let first = compose_repository(repository_root()).unwrap();
    let second = compose_repository(repository_root()).unwrap();
    assert_eq!(first.artifacts(), second.artifacts());

    let paths: Vec<_> = first
        .artifacts()
        .iter()
        .map(|artifact| artifact.relative_path.as_str())
        .collect();
    assert!(paths.contains(&".repository-engineering/package.lock.json"));
    assert!(paths.contains(&".repository-engineering/schema-registry.json"));
    assert!(paths.contains(&".repository-engineering/conformance/v0/manifest.json"));
    assert!(paths.contains(&"docs/reference/repository-engineering-package.md"));
    assert_eq!(
        paths
            .iter()
            .filter(|path| path.starts_with(".repository-engineering/schemas/v0/"))
            .count(),
        14
    );

    for artifact in first
        .artifacts()
        .iter()
        .filter(|artifact| artifact.relative_path.contains("/schemas/v0/"))
    {
        let schema: serde_json::Value = serde_json::from_slice(&artifact.bytes).unwrap();
        assert!(schema["$id"].as_str().unwrap().contains(":v0:"));
        assert!(all_references_are_local(&schema));
    }
}

#[test]
fn generated_reference_names_every_reviewed_row_and_separates_states() {
    let projections = compose_repository(repository_root()).unwrap();
    let reference = projections
        .artifacts()
        .iter()
        .find(|artifact| {
            artifact.relative_path == "docs/reference/repository-engineering-package.md"
        })
        .unwrap();
    let text = std::str::from_utf8(&reference.bytes).unwrap();
    let authored =
        ls_repository_engineering::inventory::load_authored_package(repository_root()).unwrap();
    for row in authored.ledger.rows {
        assert!(text.contains(&row.logical_id.0));
    }
    assert!(text.contains("Declaration"));
    assert!(text.contains("Implementation"));
    assert!(text.contains("Certification"));
    assert!(text.contains("Authority"));
    assert!(text.contains("unported"));
    assert!(text.contains("uncertified"));
    assert!(text.contains("legacy"));
}

#[test]
fn real_repository_generate_and_check_round_trip() {
    let projections = compose_repository(repository_root()).unwrap();
    let temp = tempfile_directory();
    generate_projection_set(&temp, &projections).unwrap();
    assert!(check_projection_set(&temp, &projections).is_empty());
    std::fs::remove_dir_all(temp).unwrap();
}

fn all_references_are_local(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => map.iter().all(|(key, value)| {
            (key != "$ref"
                || value
                    .as_str()
                    .is_some_and(|reference| reference.starts_with('#')))
                && all_references_are_local(value)
        }),
        serde_json::Value::Array(values) => values.iter().all(all_references_are_local),
        _ => true,
    }
}

fn tempfile_directory() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "ls-repository-engineering-authored-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&path).unwrap();
    path
}
