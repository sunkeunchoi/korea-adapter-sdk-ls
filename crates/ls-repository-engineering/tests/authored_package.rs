use std::path::Path;

use ls_repository_engineering::generate::{check_projection_set, generate_projection_set};
use ls_repository_engineering::inventory::load_authored_package;
use ls_repository_engineering::repository::compose_repository;
use sha2::{Digest, Sha256};

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
fn real_authored_pair_is_loaded_and_digest_bound_without_mutable_ledger_knowledge() {
    let authored = load_authored_package(repository_root()).unwrap();
    let capability = &authored.capability_contracts[0];
    let worker = &authored.worker_role_contracts[0];

    let expected_capability = serde_json::from_str(include_str!(
        "fixtures/fidelity/audit-carried-rows.capability.json"
    ))
    .unwrap();
    let expected_worker = serde_json::from_str(include_str!(
        "fixtures/fidelity/decommission-row-auditor.worker.json"
    ))
    .unwrap();
    assert_eq!(capability, &expected_capability);
    assert_eq!(worker, &expected_worker);

    assert_eq!(capability.capability_id.0, "audit-carried-rows");
    assert_eq!(worker.role_id.0, "decommission-row-auditor");
    assert!(capability
        .knowledge_references
        .iter()
        .all(|reference| { reference.path.0 != "docs/migration-source-extraction-ledger.md" }));
    assert!(capability
        .touched_paths
        .iter()
        .any(|path| path.0 == "docs/migration-source-extraction-ledger.md"));

    let evidence = capability.evidence_status.as_ref().unwrap();
    assert_eq!(evidence.legacy_artifact_sets.len(), 1);
    assert_eq!(evidence.legacy_artifact_sets[0].members.len(), 26);
    assert!(worker
        .terminal_result_correlation
        .as_ref()
        .is_some_and(|correlation| correlation.envelope_field.0 == "assignment_id"));
}

#[test]
fn exactly_two_planned_rows_changed_and_every_other_row_matches_the_pre_wave_hash() {
    let authored = load_authored_package(repository_root()).unwrap();
    let planned: Vec<_> = authored
        .ledger
        .rows
        .iter()
        .filter(|row| {
            row.migration_state == ls_repository_engineering::schema::MigrationState::Planned
        })
        .map(|row| row.logical_id.0.as_str())
        .collect();
    assert_eq!(
        planned,
        [
            "capability--audit-carried-rows",
            "worker-role--decommission-row-auditor"
        ]
    );

    let capability = authored
        .ledger
        .rows
        .iter()
        .find(|row| row.logical_id.0 == "capability--audit-carried-rows")
        .unwrap();
    assert_eq!(
        capability.source_locator.0,
        ".agents/skills/audit-carried-rows"
    );
    assert_eq!(
        capability.source_digest.as_ref().unwrap().0,
        "sha256:1dd707f6481a2cb4e71c95f12041d8b1c2c6aee38f45554ff3e5ff8a05a19949"
    );
    assert_eq!(
        capability.replacement_contract.as_ref().unwrap().0,
        "audit-carried-rows"
    );
    assert!(capability.parity_reference.is_none());
    assert_eq!(
        capability.current_authority,
        ls_repository_engineering::schema::AuthorityState::Legacy
    );

    let worker = authored
        .ledger
        .rows
        .iter()
        .find(|row| row.logical_id.0 == "worker-role--decommission-row-auditor")
        .unwrap();
    assert_eq!(
        worker.source_locator.0,
        ".claude/agents/decommission-row-auditor.md"
    );
    assert_eq!(
        worker.source_digest.as_ref().unwrap().0,
        "sha256:67e28c21b90e82703efbd129dda3fe10d5ecaf8ee198f2a3dcb754f518fd9f72"
    );
    assert_eq!(
        worker.replacement_contract.as_ref().unwrap().0,
        "decommission-row-auditor"
    );
    assert!(worker.parity_reference.is_none());
    assert_eq!(
        worker.current_authority,
        ls_repository_engineering::schema::AuthorityState::Legacy
    );

    let mut protected = authored.ledger.clone();
    protected.rows.retain(|row| {
        !matches!(
            row.logical_id.0.as_str(),
            "capability--audit-carried-rows" | "worker-role--decommission-row-auditor"
        )
    });
    let canonical = serde_json_canonicalizer::to_vec(&protected).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(canonical)),
        "1c23da96f17416abfa8c78f4ae2155108ef2d68d4cacdcc970e1526d5e4279ed"
    );
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
