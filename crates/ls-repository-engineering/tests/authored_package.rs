use std::path::Path;

use ls_repository_engineering::generate::{check_projection_set, generate_projection_set};
use ls_repository_engineering::inventory::load_authored_package;
use ls_repository_engineering::repository::compose_repository;
use ls_repository_engineering::schema::{
    ArtifactReference, AuthorityState, EvidenceAvailability, RepositoryPath,
};
use ls_repository_engineering::validator::validate_semantic_package;
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
    assert!(text.contains("Planned migration rows: `2`"));
    assert_eq!(
        text.matches("| planned | parity_not_proven | declared |")
            .count(),
        2
    );
    assert!(text.contains("successor implementation evidence `absent`"));
    assert!(text.contains("parity `unproved`"));
    assert!(text.contains("unavailable_unproved"));
    assert!(text.contains("Locator | Digest"));
    assert!(text.contains(
        "Canonical typed state: declaration `declared`, implementation `unported`, certification `uncertified`, authority `legacy`, retirement `not_started`; activation: inactive"
    ));
    assert!(!text.contains("successor-authoritative"));
}

#[test]
fn conformance_and_exact_lock_include_declared_contract_semantics() {
    let projections = compose_repository(repository_root()).unwrap();
    let artifact = |path: &str| {
        projections
            .artifacts()
            .iter()
            .find(|artifact| artifact.relative_path == path)
            .unwrap()
    };

    let structural: serde_json::Value = serde_json::from_slice(
        &artifact(".repository-engineering/conformance/v0/structural.json").bytes,
    )
    .unwrap();
    let validates = structural["validates"].as_array().unwrap();
    for path in [
        ".repository-engineering/contracts/capabilities/audit-carried-rows.toml",
        ".repository-engineering/contracts/workers/decommission-row-auditor.toml",
    ] {
        assert!(validates.iter().any(|value| value == path));
    }

    let cross_record: serde_json::Value = serde_json::from_slice(
        &artifact(".repository-engineering/conformance/v0/cross-record.json").bytes,
    )
    .unwrap();
    let rules = cross_record["rules"].as_array().unwrap();
    for rule in [
        "planned_replacement_is_declared_and_type_correct",
        "legacy_dependencies_remain_legacy_authoritative_below_parity",
        "semantic_claim_sources_resolve_and_field_groups_are_unique",
        "legacy_evidence_does_not_satisfy_successor_evidence",
        "terminal_results_preserve_assignment_row_correlation",
    ] {
        assert!(rules.iter().any(|value| value == rule));
    }

    let exact_lock: serde_json::Value =
        serde_json::from_slice(&artifact(".repository-engineering/package.lock.json").bytes)
            .unwrap();
    let normative = &exact_lock["normative"];
    assert_eq!(
        normative["capability_contracts"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        normative["worker_role_contracts"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        normative["capability_contracts"][0]["path"],
        ".repository-engineering/contracts/capabilities/audit-carried-rows.toml"
    );
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
fn semantic_cross_record_validation_rejects_false_readiness_and_broken_links() {
    let root = repository_root();
    let authored = load_authored_package(root).unwrap();
    assert!(validate_semantic_package(root, &authored).is_empty());

    let mut wrong_replacement = authored.clone();
    wrong_replacement
        .ledger
        .rows
        .iter_mut()
        .find(|row| row.logical_id.0 == "capability--audit-carried-rows")
        .unwrap()
        .replacement_contract = Some(ls_repository_engineering::schema::StableId(
        "decommission-row-auditor".to_owned(),
    ));
    assert_semantic_code(&wrong_replacement, "semantic.replacement.type_mismatch");

    let mut cross_kind_id_collision = authored.clone();
    cross_kind_id_collision.package.declared_worker_roles[0].id = cross_kind_id_collision
        .package
        .declared_capability_contracts[0]
        .id
        .clone();
    assert_semantic_code(
        &cross_kind_id_collision,
        "package.declared_registry.id_collision",
    );

    let mut unresolved_dependency = authored.clone();
    unresolved_dependency
        .ledger
        .rows
        .retain(|row| row.logical_id.0 != "capability--audit-row");
    assert_semantic_code(
        &unresolved_dependency,
        "semantic.legacy_dependency.unresolved",
    );

    let mut transferred_dependency = authored.clone();
    transferred_dependency
        .ledger
        .rows
        .iter_mut()
        .find(|row| row.logical_id.0 == "capability--audit-row")
        .unwrap()
        .current_authority = AuthorityState::Successor;
    assert_semantic_code(
        &transferred_dependency,
        "semantic.legacy_dependency.unresolved",
    );

    let artifact: ArtifactReference =
        authored.capability_contracts[0].knowledge_references[0].clone();
    let mut executor = authored.clone();
    executor.capability_contracts[0].executor = Some(artifact.clone());
    assert_semantic_code(&executor, "semantic.executor.forbidden");

    let mut scenario = authored.clone();
    scenario.capability_contracts[0]
        .scenario_references
        .push(artifact);
    assert_semantic_code(&scenario, "semantic.scenario_reference.forbidden");

    let mut successor_evidence = authored.clone();
    successor_evidence.capability_contracts[0]
        .evidence_status
        .as_mut()
        .unwrap()
        .successor_implementation = EvidenceAvailability::AvailableValidated;
    assert_semantic_code(
        &successor_evidence,
        "semantic.evidence.successor_claim_forbidden",
    );

    let mut duplicate_claim = authored.clone();
    let duplicate = duplicate_claim.capability_contracts[0].semantic_claims[0].clone();
    duplicate_claim.capability_contracts[0]
        .semantic_claims
        .push(duplicate);
    assert_semantic_code(&duplicate_claim, "semantic.claim.duplicate_field_group");

    let mut correlation = authored.clone();
    correlation.worker_role_contracts[0]
        .terminal_result_correlation
        .as_mut()
        .unwrap()
        .assignment_field
        .0 = "other-row".to_owned();
    assert_semantic_code(&correlation, "semantic.worker.correlation_invalid");

    let mut coordination = authored.clone();
    coordination.capability_contracts[0]
        .coordination_semantics
        .as_mut()
        .unwrap()
        .phases
        .clear();
    assert_semantic_code(&coordination, "semantic.coordination.incomplete");

    let mut contradictory_description = authored.clone();
    contradictory_description.capability_contracts[0].public_description =
        Some("This text says legacy, but typed state is authoritative.".to_owned());
    contradictory_description.capability_contracts[0]
        .state
        .authority = AuthorityState::Successor;
    assert_semantic_code(&contradictory_description, "authority.transfer.forbidden");

    let mut absent_declaration = authored.clone();
    absent_declaration.capability_contracts[0].state.declaration =
        ls_repository_engineering::schema::DeclarationState::Absent;
    assert_semantic_code(&absent_declaration, "declaration.absent.forbidden");

    let mut unresolved_claim_source = authored.clone();
    unresolved_claim_source.worker_role_contracts[0]
        .knowledge_references
        .remove(0);
    assert_semantic_code(&unresolved_claim_source, "semantic.claim.source_unresolved");

    let mut uncovered_claim = authored.clone();
    for claim in &mut uncovered_claim.capability_contracts[0].semantic_claims {
        claim
            .field_groups
            .retain(|field| field.0 != "evidence_obligations");
    }
    assert_semantic_code(&uncovered_claim, "semantic.claim.field_group_uncovered");

    let mut worker_reference = authored.clone();
    let worker = &mut worker_reference.worker_role_contracts[0];
    worker.semantic_claims[0].sources[0] =
        ls_repository_engineering::schema::SemanticClaimSource::WorkerKnowledgeReference {
            role_id: worker.role_id.clone(),
            path: worker.knowledge_references[0].path.clone(),
        };
    assert!(
        !validate_semantic_package(repository_root(), &worker_reference)
            .iter()
            .any(|finding| finding.code == "semantic.claim.source_unresolved")
    );

    let mut escaping_path = authored.clone();
    escaping_path.capability_contracts[0].touched_paths[0] = RepositoryPath("../escape".to_owned());
    assert_semantic_code(&escaping_path, "semantic.touched_path.invalid");
}

#[test]
fn successor_operational_fields_are_host_neutral_outside_legacy_only_fields() {
    let authored = load_authored_package(repository_root()).unwrap();
    let mut capability = serde_json::to_value(&authored.capability_contracts[0]).unwrap();
    let object = capability.as_object_mut().unwrap();
    for permitted_legacy_only in [
        "knowledge_references",
        "semantic_claims",
        "touched_paths",
        "external_source_requirements",
    ] {
        object.remove(permitted_legacy_only);
    }
    let mut worker = serde_json::to_value(&authored.worker_role_contracts[0]).unwrap();
    let object = worker.as_object_mut().unwrap();
    object.remove("knowledge_references");
    object.remove("semantic_claims");

    let operational = format!("{capability}{worker}").to_ascii_lowercase();
    for host_token in ["claude", "codex", "orca", "subagent", "bash", "/goal"] {
        assert!(!operational.contains(host_token), "{host_token}");
    }
}

#[cfg(unix)]
#[test]
fn touched_path_symlink_components_fail_without_opening_the_target() {
    use std::os::unix::fs::symlink;

    let mut authored = load_authored_package(repository_root()).unwrap();
    let root = std::env::temp_dir().join(format!(
        "ls-repository-engineering-touched-path-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let outside = root.join("outside");
    std::fs::create_dir_all(root.join("safe")).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("sentinel"), "do not open").unwrap();
    symlink(&outside, root.join("safe/link")).unwrap();
    authored.capability_contracts[0].touched_paths =
        vec![RepositoryPath("safe/link/sentinel".to_owned())];

    let findings = validate_semantic_package(&root, &authored);
    assert!(findings
        .iter()
        .any(|finding| finding.code == "semantic.touched_path.symlink"));
    assert_eq!(
        std::fs::read_to_string(outside.join("sentinel")).unwrap(),
        "do not open"
    );
    std::fs::remove_dir_all(root).unwrap();
}

fn assert_semantic_code(
    authored: &ls_repository_engineering::inventory::AuthoredPackage,
    code: &str,
) {
    let findings = validate_semantic_package(repository_root(), authored);
    assert!(
        findings.iter().any(|finding| finding.code == code),
        "missing {code}: {findings:?}"
    );
}

#[test]
fn real_repository_generate_and_check_round_trip() {
    let projections = compose_repository(repository_root()).unwrap();
    let temp = tempfile_directory();
    generate_projection_set(&temp, &projections).unwrap();
    assert!(check_projection_set(&temp, &projections).is_empty());
    let first_manifest =
        std::fs::read(temp.join(".repository-engineering/generated-set.json")).unwrap();
    generate_projection_set(&temp, &projections).unwrap();
    assert_eq!(
        std::fs::read(temp.join(".repository-engineering/generated-set.json")).unwrap(),
        first_manifest
    );
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
