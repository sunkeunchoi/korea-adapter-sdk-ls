use ls_repository_engineering::schema::{
    schema_catalog, ArtifactSetReference, AttemptRecord, AuthorityState, CapabilityContract,
    CertificationState, ContractState, DeclarationState, ImplementationState, PackageManifest,
    RetirementState, WorkerResult, WorkerRoleContract,
};
use ls_repository_engineering::validator::{
    validate_attempt_record, validate_capability_contract_vocabulary,
    validate_first_slice_contract_state, validate_first_slice_package,
    validate_worker_result_correlation, validate_worker_role_contract_vocabulary,
};

const PACKAGE: &str = include_str!("fixtures/schema/package-manifest.valid.json");

#[test]
fn structural_catalog_is_closed_and_draft_2020_12() {
    let catalog = schema_catalog();
    let expected = [
        "artifact-reference",
        "attempt-checkpoint",
        "attempt-event",
        "attempt-record",
        "capability-contract",
        "discovery-policy",
        "exact-lock",
        "migration-ledger",
        "package-manifest",
        "runtime-installation-state",
        "state-migration-handoff",
        "version-set-fixture-input",
        "worker-result",
        "worker-role-contract",
    ];

    assert_eq!(
        catalog.keys().map(String::as_str).collect::<Vec<_>>(),
        expected
    );
    for schema in catalog.values() {
        assert_eq!(
            schema.get("$schema").and_then(serde_json::Value::as_str),
            Some("https://json-schema.org/draft/2020-12/schema")
        );
        jsonschema::meta::validate(schema).expect("generated schema must meta-validate");
    }
}

#[test]
fn inactive_contract_state_allows_only_unported_or_implemented() {
    let valid = ContractState {
        declaration: DeclarationState::Declared,
        implementation: ImplementationState::Unported,
        certification: CertificationState::Uncertified,
        authority: AuthorityState::Legacy,
        retirement: RetirementState::NotStarted,
    };
    assert!(validate_first_slice_contract_state(&valid, "fixture").is_empty());

    let implemented = ContractState {
        implementation: ImplementationState::Implemented,
        ..valid.clone()
    };
    assert!(validate_first_slice_contract_state(&implemented, "fixture").is_empty());

    let mutations = [
        ContractState {
            certification: CertificationState::Certified,
            ..valid.clone()
        },
        ContractState {
            authority: AuthorityState::Successor,
            ..valid.clone()
        },
        ContractState {
            retirement: RetirementState::Complete,
            ..valid
        },
    ];
    for mutation in mutations {
        assert!(!validate_first_slice_contract_state(&mutation, "fixture").is_empty());
    }
}

#[test]
fn implemented_contracts_require_closed_component_evidence() {
    let artifact = |path: &str| {
        serde_json::json!({
            "schema_version": "v0",
            "path": path,
            "sha256": format!("sha256:{}", "1".repeat(64)),
            "media_type": "application/json"
        })
    };

    let mut capability: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/fidelity/audit-carried-rows.capability.json"
    ))
    .unwrap();
    capability["state"]["implementation"] = serde_json::json!("implemented");
    capability["executor"] = artifact(".repository-engineering/executors/audit-carried-rows.json");
    capability["scenario_references"] = serde_json::json!([artifact(
        ".repository-engineering/scenarios/audit-carried-rows/implementation.json"
    )]);
    capability["implementation_evidence"] = serde_json::json!({
        "component_kind": "capability",
        "component_id": "audit-carried-rows",
        "subject_manifest": artifact(".repository-engineering/implementation-subjects/audit-carried-rows.json"),
        "evidence": artifact(".repository-engineering/evidence/implementation/audit-carried-rows.json"),
        "validation_basis": artifact(".repository-engineering/conformance/v0/runtime.json")
    });
    capability["evidence_status"]["successor_implementation"] =
        serde_json::json!("available_validated");
    let capability: CapabilityContract = serde_json::from_value(capability).unwrap();
    assert!(validate_capability_contract_vocabulary(&capability).is_empty());

    for missing in ["executor", "scenario_references", "implementation_evidence"] {
        let mut invalid = capability.clone();
        match missing {
            "executor" => invalid.executor = None,
            "scenario_references" => invalid.scenario_references.clear(),
            "implementation_evidence" => invalid.implementation_evidence = None,
            _ => unreachable!(),
        }
        assert!(validate_capability_contract_vocabulary(&invalid)
            .iter()
            .any(|finding| finding.code == "implementation.component_evidence.incomplete"));
    }

    let mut worker: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/fidelity/decommission-row-auditor.worker.json"
    ))
    .unwrap();
    worker["state"]["implementation"] = serde_json::json!("implemented");
    worker["role_bundle"] = artifact(".repository-engineering/roles/decommission-row-auditor.json");
    worker["scenario_references"] = serde_json::json!([artifact(
        ".repository-engineering/scenarios/audit-carried-rows/implementation.json"
    )]);
    worker["implementation_evidence"] = serde_json::json!({
        "component_kind": "worker_role",
        "component_id": "decommission-row-auditor",
        "subject_manifest": artifact(".repository-engineering/implementation-subjects/audit-carried-rows.json"),
        "evidence": artifact(".repository-engineering/evidence/implementation/decommission-row-auditor.json"),
        "validation_basis": artifact(".repository-engineering/conformance/v0/runtime.json")
    });
    let worker: WorkerRoleContract = serde_json::from_value(worker).unwrap();
    assert!(validate_worker_role_contract_vocabulary(&worker).is_empty());
}

#[test]
fn package_manifest_rejects_unknown_fields_and_versions() {
    let parsed: PackageManifest = serde_json::from_str(PACKAGE).expect("valid package fixture");
    assert!(validate_first_slice_package(&parsed).is_empty());

    let unknown = PACKAGE.replace(
        "\"schema_version\": \"v0\"",
        "\"schema_version\": \"v0\", \"payload\": \"secret\"",
    );
    assert!(serde_json::from_str::<PackageManifest>(&unknown).is_err());

    let newer = PACKAGE.replace("\"schema_version\": \"v0\"", "\"schema_version\": \"v1\"");
    assert!(serde_json::from_str::<PackageManifest>(&newer).is_err());

    let escaping = PACKAGE.replace(
        ".repository-engineering/discovery-policy.toml",
        "../discovery-policy.toml",
    );
    assert!(serde_json::from_str::<PackageManifest>(&escaping).is_err());

    let wrong_path = PACKAGE.replace(
        ".repository-engineering/discovery-policy.toml",
        ".repository-engineering/other-policy.toml",
    );
    let wrong_path: PackageManifest = serde_json::from_str(&wrong_path).unwrap();
    assert!(validate_first_slice_package(&wrong_path)
        .iter()
        .any(|finding| finding.code == "package.declaration.mismatch"));

    let mut omitted_component: serde_json::Value = serde_json::from_str(PACKAGE).unwrap();
    omitted_component["optional_components"]
        .as_array_mut()
        .unwrap()
        .pop();
    let omitted_component: PackageManifest = serde_json::from_value(omitted_component).unwrap();
    assert!(validate_first_slice_package(&omitted_component)
        .iter()
        .any(|finding| finding.code == "package.optional_component.incomplete"));
}

#[test]
fn lexical_contracts_reject_unsafe_ids_paths_and_digests() {
    use ls_repository_engineering::schema::{ArtifactReference, RepositoryPath, StableId};

    assert!(serde_json::from_str::<StableId>(r#""contains space""#).is_err());
    assert!(serde_json::from_str::<RepositoryPath>(r#""/absolute""#).is_err());
    assert!(serde_json::from_str::<RepositoryPath>(r#""safe/../escape""#).is_err());
    assert!(serde_json::from_str::<ArtifactReference>(
        r#"{"schema_version":"v0","path":"safe.json","sha256":"sha256:ABC","media_type":"application/json"}"#,
    )
    .is_err());
}

#[test]
fn worker_result_requires_an_explicit_tagged_outcome() {
    let held = serde_json::from_str::<WorkerResult>(
        r#"{"schema_version":"v0","result":"held","attempt_id":"attempt-1","invocation_id":"invocation-1","assignment_id":"L1","worker_instance_id":"worker-1","worker_instance_receipt":{"schema_version":"v0","path":"receipts/worker-1.json","sha256":"sha256:1111111111111111111111111111111111111111111111111111111111111111","media_type":"application/json"},"reason":"human_gate_required"}"#,
    )
    .expect("typed held result");
    assert_eq!(serde_json::to_value(held).unwrap()["result"], "held");

    assert!(serde_json::from_str::<WorkerResult>(
        r#"{"schema_version":"v0","reason":"human_gate_required"}"#
    )
    .is_err());
    assert!(serde_json::from_str::<WorkerResult>(
        r#"{"schema_version":"v0","result":"success","assignment_id":"L1","payload":{}}"#
    )
    .is_err());
}

#[test]
fn every_worker_result_variant_requires_assignment_correlation() {
    let artifact = r#"{"schema_version":"v0","path":"records/L1.yaml","sha256":"sha256:0000000000000000000000000000000000000000000000000000000000000000","media_type":"application/yaml"}"#;
    let common = r#""attempt_id":"attempt-1","invocation_id":"invocation-1","assignment_id":"L1","worker_instance_id":"worker-1","worker_instance_receipt":{"schema_version":"v0","path":"receipts/worker-1.json","sha256":"sha256:1111111111111111111111111111111111111111111111111111111111111111","media_type":"application/json"}"#;
    let variants = [
        format!(
            r#"{{"schema_version":"v0","result":"succeeded","payload":{{"row_id":"L1","verdict":"unverifiable","record":{artifact}}}}}"#
        ),
        r#"{"schema_version":"v0","result":"held","reason":"blocked"}"#.to_owned(),
        r#"{"schema_version":"v0","result":"cancelled","reason":"cancelled"}"#.to_owned(),
        r#"{"schema_version":"v0","result":"policy_violated","policy_id":"policy"}"#.to_owned(),
        r#"{"schema_version":"v0","result":"failed","error_code":"failed"}"#.to_owned(),
        format!(
            r#"{{"schema_version":"v0","result":"recovery_required","checkpoint":{artifact}}}"#
        ),
    ];
    for variant in &variants {
        assert!(serde_json::from_str::<WorkerResult>(&variant).is_err());
        let correlated = variant.replacen('{', &format!(r#"{{{common},"#), 1);
        assert!(serde_json::from_str::<WorkerResult>(&correlated).is_ok());
    }

    let success = variants[0].replacen('{', &format!(r#"{{{common},"#), 1);
    let success: WorkerResult = serde_json::from_str(&success).unwrap();
    let stable = |value: &str| ls_repository_engineering::schema::StableId(value.to_owned());
    assert!(validate_worker_result_correlation(
        &success,
        &stable("attempt-1"),
        &stable("invocation-1"),
        &stable("L1"),
    )
    .is_empty());

    let wrong_row = serde_json::to_string(&success)
        .unwrap()
        .replace("\"row_id\":\"L1\"", "\"row_id\":\"L2\"");
    let wrong_row: WorkerResult = serde_json::from_str(&wrong_row).unwrap();
    assert!(validate_worker_result_correlation(
        &wrong_row,
        &stable("attempt-1"),
        &stable("invocation-1"),
        &stable("L1"),
    )
    .iter()
    .any(|finding| finding.code == "worker_result.success_row.mismatch"));

    assert!(validate_worker_result_correlation(
        &success,
        &stable("stale-attempt"),
        &stable("invocation-1"),
        &stable("L1"),
    )
    .iter()
    .any(|finding| finding.code == "worker_result.correlation.mismatch"));

    let success = serde_json::to_string(&success).unwrap();
    assert!(serde_json::from_str::<WorkerResult>(
        &success.replace("\"payload\"", "\"payload\":{},\"payload_extra\"")
    )
    .is_err());
}

#[test]
fn declared_registries_and_semantic_vocabulary_are_closed() {
    let package: PackageManifest = serde_json::from_str(PACKAGE).unwrap();
    assert!(validate_first_slice_package(&package).is_empty());

    let capability: CapabilityContract = serde_json::from_str(include_str!(
        "fixtures/fidelity/audit-carried-rows.capability.json"
    ))
    .expect("audit capability fixture");
    assert!(validate_capability_contract_vocabulary(&capability).is_empty());
    let artifact_set: &ArtifactSetReference = &capability
        .evidence_status
        .as_ref()
        .unwrap()
        .legacy_artifact_sets[0];
    let mut expected = artifact_set.members.clone();
    expected.sort();
    assert_eq!(artifact_set.normalized_members(), expected);

    let worker: WorkerRoleContract = serde_json::from_str(include_str!(
        "fixtures/fidelity/decommission-row-auditor.worker.json"
    ))
    .expect("audit worker fixture");
    assert!(validate_worker_role_contract_vocabulary(&worker).is_empty());

    let mut unavailable = serde_json::to_value(&capability).unwrap();
    unavailable["external_source_requirements"][0]["locator"] = serde_json::json!("sibling");
    let unavailable: CapabilityContract = serde_json::from_value(unavailable).unwrap();
    assert!(validate_capability_contract_vocabulary(&unavailable)
        .iter()
        .any(|finding| finding.code == "external_source.unavailable_has_location"));

    let mut duplicate_allowed = serde_json::to_value(&worker).unwrap();
    duplicate_allowed["result_fields"][1]["allowed_values"] =
        serde_json::json!(["confirmed", "confirmed"]);
    let duplicate_allowed: WorkerRoleContract = serde_json::from_value(duplicate_allowed).unwrap();
    assert!(validate_worker_role_contract_vocabulary(&duplicate_allowed)
        .iter()
        .any(|finding| finding.code == "typed_field.allowed_values.duplicate"));

    let mut duplicate_registry = serde_json::to_value(package).unwrap();
    let duplicate = duplicate_registry["declared_capability_contracts"][0].clone();
    duplicate_registry["declared_capability_contracts"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    let duplicate_registry: PackageManifest = serde_json::from_value(duplicate_registry).unwrap();
    assert!(validate_first_slice_package(&duplicate_registry)
        .iter()
        .any(|finding| finding.code == "package.declared_registry.duplicate"));
}

#[test]
fn attempt_record_rejects_success_by_omission_and_invalid_transitions() {
    let invalid: AttemptRecord = serde_json::from_str(include_str!(
        "fixtures/schema/attempt-record.invalid-transition.json"
    ))
    .expect("structurally valid attempt record");
    let findings = validate_attempt_record(&invalid);
    assert!(findings
        .iter()
        .any(|finding| finding.code == "attempt.transition.invalid"));
    assert!(findings
        .iter()
        .any(|finding| finding.code == "attempt.initial_state.invalid"));

    let non_monotonic = r#"{
      "schema_version":"v0",
      "attempt_id":"attempt-fixture-2",
      "capability_id":"implement-tr",
      "events":[
        {"schema_version":"v0","sequence":"2","occurred_at_utc":"2026-08-17T00:00:00Z","state":"not_evaluated"},
        {"schema_version":"v0","sequence":"1","occurred_at_utc":"2026-08-17T00:00:01Z","state":"running"},
        {"schema_version":"v0","sequence":"3","occurred_at_utc":"2026-08-17T00:00:02Z","state":"succeeded"}
      ],
      "checkpoint":null,
      "outcome":"succeeded",
      "evidence":[]
    }"#;
    let non_monotonic: AttemptRecord = serde_json::from_str(non_monotonic).unwrap();
    assert!(validate_attempt_record(&non_monotonic)
        .iter()
        .any(|finding| finding.code == "attempt.sequence.not_monotonic"));
}

#[test]
fn public_surface_inventory_contains_no_runtime_authority() {
    let inventory = include_str!("fixtures/schema/public-surface.txt");
    for forbidden in [
        "install",
        "activate",
        "launch",
        "lease",
        "execute",
        "broker",
        "publish",
        "merge",
        "retire",
        "state_write",
    ] {
        assert!(!inventory.lines().any(|line| line == forbidden));
    }
}

#[test]
fn source_derived_fidelity_fixtures_are_inert_and_lossless() {
    let capability: CapabilityContract = serde_json::from_str(include_str!(
        "fixtures/fidelity/implement-tr.capability.json"
    ))
    .expect("capability fidelity fixture");
    assert!(
        validate_first_slice_contract_state(&capability.state, &capability.capability_id.0)
            .is_empty()
    );

    for fixture in [
        include_str!("fixtures/fidelity/decommission-row-auditor.worker.json"),
        include_str!("fixtures/fidelity/tr-promoter.worker.json"),
    ] {
        let worker: WorkerRoleContract = serde_json::from_str(fixture).expect("worker fixture");
        assert!(validate_first_slice_contract_state(&worker.state, &worker.role_id.0).is_empty());
        let value = serde_json::to_value(&worker).unwrap();
        let reparsed: WorkerRoleContract = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(reparsed).unwrap(), value);
    }
}

#[test]
fn crate_is_a_publish_disabled_tooling_leaf() {
    // This reads Cargo's graph; it does not infer the boundary from source layout.
    let output = std::process::Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .expect("cargo metadata runs");
    assert!(output.status.success());
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let packages = metadata["packages"].as_array().unwrap();
    let package = packages
        .iter()
        .find(|package| package["name"] == "ls-repository-engineering")
        .expect("workspace package present");
    assert_eq!(package["publish"], serde_json::json!([]));
    assert!(package["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .all(|dependency| !dependency["name"].as_str().unwrap().starts_with("ls-")));
    assert!(packages.iter().all(|candidate| {
        candidate["name"] == "ls-repository-engineering"
            || candidate["dependencies"]
                .as_array()
                .unwrap()
                .iter()
                .all(|dependency| dependency["name"] != "ls-repository-engineering")
    }));
}
