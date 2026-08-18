use std::path::Path;

use ls_repository_engineering::identity::{
    canonicalize_strict_json, capability_contract_semantic_digest, package_lock_id,
    package_manifest_semantic_digest, worker_role_contract_semantic_digest,
};
use ls_repository_engineering::inventory::load_authored_package;
use ls_repository_engineering::lock::{build_lock, lock_bytes};
use ls_repository_engineering::schema::{
    ArtifactReference, BuildProvenance, NormativeLockClosure, OptionalComponent,
    OptionalComponentKind, RepositoryPath, SchemaVersion, Sha256Digest, WorkerRoleContract,
};

fn artifact(path: &str, byte: char) -> ArtifactReference {
    ArtifactReference {
        schema_version: SchemaVersion::V0,
        path: RepositoryPath(path.to_owned()),
        sha256: Sha256Digest(format!("sha256:{}", byte.to_string().repeat(64))),
        media_type: "application/json".to_owned(),
    }
}

fn normative() -> NormativeLockClosure {
    NormativeLockClosure {
        package: artifact(".repository-engineering/package.toml", '1'),
        discovery_policy: artifact(".repository-engineering/discovery-policy.toml", '2'),
        migration_ledger: artifact(".repository-engineering/migration-ledger.toml", '3'),
        schema_registry: artifact(".repository-engineering/schemas/v0/registry.json", '4'),
        conformance_corpus: artifact(".repository-engineering/conformance/v0/manifest.json", '5'),
        runtime_bundle: artifact(".repository-engineering/runtime-bundle.json", 'a'),
        implementation_subjects: vec![artifact(
            ".repository-engineering/implementation-subjects/audit-carried-rows.json",
            'b',
        )],
        capability_contracts: vec![artifact(
            ".repository-engineering/contracts/capabilities/audit-carried-rows.toml",
            '8',
        )],
        worker_role_contracts: vec![artifact(
            ".repository-engineering/contracts/workers/decommission-row-auditor.toml",
            '9',
        )],
        optional_components: vec![
            OptionalComponent::Disabled {
                component: OptionalComponentKind::WorkerAdapter,
            },
            OptionalComponent::Disabled {
                component: OptionalComponentKind::OrcaUi,
            },
        ],
    }
}

fn provenance(byte: char) -> BuildProvenance {
    BuildProvenance {
        generator: artifact("crates/ls-repository-engineering", byte),
        dependency_lock: artifact("Cargo.lock", '7'),
        workflow_pins: vec![],
    }
}

#[test]
fn identity_normalizes_sets_but_changes_for_normative_mutations() {
    let baseline = normative();
    let baseline_id = package_lock_id(&baseline).unwrap();

    let mut shuffled = baseline.clone();
    shuffled.optional_components.reverse();
    shuffled.capability_contracts.reverse();
    shuffled.worker_role_contracts.reverse();
    assert_eq!(package_lock_id(&shuffled).unwrap(), baseline_id);

    let mut changed = baseline.clone();
    changed.discovery_policy.sha256 = Sha256Digest(format!("sha256:{}", "a".repeat(64)));
    assert_ne!(package_lock_id(&changed).unwrap(), baseline_id);

    let mutations: [fn(&mut NormativeLockClosure); 5] = [
        |value: &mut NormativeLockClosure| value.migration_ledger.sha256.0.replace_range(7..8, "a"),
        |value: &mut NormativeLockClosure| {
            value.conformance_corpus.sha256.0.replace_range(7..8, "a")
        },
        |value: &mut NormativeLockClosure| {
            value.capability_contracts[0]
                .sha256
                .0
                .replace_range(7..8, "a")
        },
        |value: &mut NormativeLockClosure| {
            value.worker_role_contracts[0]
                .sha256
                .0
                .replace_range(7..8, "a")
        },
        |value: &mut NormativeLockClosure| value.schema_registry.sha256.0.replace_range(7..8, "a"),
    ];
    for mutate in mutations {
        let mut changed = baseline.clone();
        mutate(&mut changed);
        assert_ne!(package_lock_id(&changed).unwrap(), baseline_id);
    }
}

#[test]
fn contract_identity_excludes_presentation_and_normalizes_set_like_fields() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap();
    let authored = load_authored_package(root).unwrap();
    let capability = &authored.capability_contracts[0];
    let worker = &authored.worker_role_contracts[0];
    let capability_id = capability_contract_semantic_digest(capability).unwrap();
    let worker_id = worker_role_contract_semantic_digest(worker).unwrap();
    let package_id = package_manifest_semantic_digest(&authored.package).unwrap();

    let mut presentation = capability.clone();
    presentation.public_description = Some(
        "Contradictory presentation: active, successor-authoritative, and certified.".to_owned(),
    );
    assert_eq!(
        capability_contract_semantic_digest(&presentation).unwrap(),
        capability_id
    );

    let mut reordered = capability.clone();
    reordered.safety_overlays.reverse();
    reordered.touched_paths.reverse();
    reordered.knowledge_references.reverse();
    reordered.semantic_claims.reverse();
    reordered.semantic_claims[0].sources.reverse();
    reordered
        .coordination_semantics
        .as_mut()
        .unwrap()
        .dispatch_cohorts
        .reverse();
    reordered
        .evidence_status
        .as_mut()
        .unwrap()
        .legacy_artifact_sets[0]
        .members
        .reverse();
    assert_eq!(
        capability_contract_semantic_digest(&reordered).unwrap(),
        capability_id
    );

    let mut reordered_worker = worker.clone();
    reordered_worker.knowledge_references.reverse();
    reordered_worker.semantic_claims.reverse();
    assert_eq!(
        worker_role_contract_semantic_digest(&reordered_worker).unwrap(),
        worker_id
    );

    let mut reordered_package = authored.package.clone();
    let mut extra_capability = reordered_package.declared_capability_contracts[0].clone();
    extra_capability.id.0 = "second-capability".to_owned();
    extra_capability.path.0 =
        ".repository-engineering/contracts/capabilities/second-capability.toml".to_owned();
    reordered_package
        .declared_capability_contracts
        .push(extra_capability);
    let mut extra_worker = reordered_package.declared_worker_roles[0].clone();
    extra_worker.id.0 = "second-worker".to_owned();
    extra_worker.path.0 = ".repository-engineering/contracts/workers/second-worker.toml".to_owned();
    reordered_package.declared_worker_roles.push(extra_worker);
    let ordered_package_id = package_manifest_semantic_digest(&reordered_package).unwrap();
    reordered_package.declared_capability_contracts.reverse();
    reordered_package.declared_worker_roles.reverse();
    reordered_package.optional_components.reverse();
    assert_eq!(
        package_manifest_semantic_digest(&reordered_package).unwrap(),
        ordered_package_id
    );
    assert_ne!(ordered_package_id, package_id);

    let worker_toml = std::fs::read_to_string(
        root.join(".repository-engineering/contracts/workers/decommission-row-auditor.toml"),
    )
    .unwrap();
    let reordered_toml = worker_toml.replacen(
        "schema_version = \"v0\"\nrole_id = \"decommission-row-auditor\"",
        "role_id = \"decommission-row-auditor\"\nschema_version = \"v0\"",
        1,
    );
    let parsed: WorkerRoleContract = toml::from_str(&reordered_toml).unwrap();
    assert_eq!(
        worker_role_contract_semantic_digest(&parsed).unwrap(),
        worker_id
    );
}

#[test]
fn operational_contract_mutations_change_identity() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap();
    let authored = load_authored_package(root).unwrap();
    let capability = &authored.capability_contracts[0];
    let worker = &authored.worker_role_contracts[0];
    let capability_id = capability_contract_semantic_digest(capability).unwrap();
    let worker_id = worker_role_contract_semantic_digest(worker).unwrap();

    let mut mutations = Vec::new();
    let mut autonomy = capability.clone();
    autonomy.autonomy = ls_repository_engineering::schema::AutonomyClass::A2;
    mutations.push(autonomy);
    let mut overlay = capability.clone();
    overlay.safety_overlays.pop();
    mutations.push(overlay);
    let mut outcome = capability.clone();
    outcome.outcomes.pop();
    mutations.push(outcome);
    let mut touched_path = capability.clone();
    touched_path.touched_paths.pop();
    mutations.push(touched_path);
    let mut worker_role = capability.clone();
    worker_role.worker_roles[0].0 = "changed-worker".to_owned();
    mutations.push(worker_role);
    let mut phases = capability.clone();
    phases
        .coordination_semantics
        .as_mut()
        .unwrap()
        .phases
        .reverse();
    mutations.push(phases);
    let mut coordination = capability.clone();
    coordination
        .coordination_semantics
        .as_mut()
        .unwrap()
        .roll_up
        .0 = "changed-roll-up".to_owned();
    mutations.push(coordination);
    let mut credential = capability.clone();
    credential
        .credential_boundary
        .as_mut()
        .unwrap()
        .future_executor
        .attendance
        .0 = "changed-attendance".to_owned();
    mutations.push(credential);
    let mut external = capability.clone();
    external.external_source_requirements[0].purpose.0 = "changed-purpose".to_owned();
    mutations.push(external);
    let mut evidence = capability.clone();
    evidence
        .evidence_status
        .as_mut()
        .unwrap()
        .legacy_evidence_satisfies_successor = true;
    mutations.push(evidence);
    let mut provenance = capability.clone();
    provenance.semantic_claims[0].field_groups[0].0 = "changed_field".to_owned();
    mutations.push(provenance);
    let mut dependency = capability.clone();
    dependency.legacy_authority_dependencies[0].0 = "changed-dependency".to_owned();
    mutations.push(dependency);

    for mutation in mutations {
        assert_ne!(
            capability_contract_semantic_digest(&mutation).unwrap(),
            capability_id
        );
    }

    let mut correlation = worker.clone();
    correlation
        .terminal_result_correlation
        .as_mut()
        .unwrap()
        .assignment_field
        .0 = "changed-field".to_owned();
    assert_ne!(
        worker_role_contract_semantic_digest(&correlation).unwrap(),
        worker_id
    );
    let mut allowed_verdict = worker.clone();
    allowed_verdict.result_fields[1].allowed_values.pop();
    assert_ne!(
        worker_role_contract_semantic_digest(&allowed_verdict).unwrap(),
        worker_id
    );
    let mut result_order = worker.clone();
    result_order.result_fields.reverse();
    assert_ne!(
        worker_role_contract_semantic_digest(&result_order).unwrap(),
        worker_id
    );
}

#[test]
fn build_provenance_is_outside_identity_and_lock_bytes_are_exact() {
    let normative = normative();
    let first = build_lock(normative.clone(), provenance('6')).unwrap();
    let second = build_lock(normative, provenance('8')).unwrap();
    assert_eq!(first.package_lock_id, second.package_lock_id);
    assert_ne!(lock_bytes(&first).unwrap(), lock_bytes(&second).unwrap());
    let bytes = lock_bytes(&first).unwrap();
    assert!(bytes.ends_with(b"\n"));
    assert!(!bytes.ends_with(b"\n\n"));
    let reparsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(reparsed["package_lock_id"], first.package_lock_id.0);
}

#[test]
fn strict_jcs_matches_rfc_ordering_and_rejects_ambiguous_inputs() {
    let input = include_str!("fixtures/identity/rfc8785.input.json");
    let expected = include_bytes!("fixtures/identity/rfc8785.expected.json")
        .strip_suffix(b"\n")
        .unwrap();
    assert_eq!(canonicalize_strict_json(input).unwrap(), expected);

    assert!(canonicalize_strict_json(r#"{"a":1,"a":2}"#).is_err());
    assert!(canonicalize_strict_json(r#"{"negative_zero":-0}"#).is_err());

    let distinct = canonicalize_strict_json(r#"{"é":1,"é":2}"#).unwrap();
    assert_eq!(String::from_utf8(distinct).unwrap(), r#"{"é":2,"é":1}"#);
}
