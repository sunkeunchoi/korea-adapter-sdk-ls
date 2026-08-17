use ls_repository_engineering::identity::{canonicalize_strict_json, package_lock_id};
use ls_repository_engineering::lock::{build_lock, lock_bytes};
use ls_repository_engineering::schema::{
    ArtifactReference, BuildProvenance, NormativeLockClosure, OptionalComponent,
    OptionalComponentKind, RepositoryPath, SchemaVersion, Sha256Digest,
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
    assert_eq!(package_lock_id(&shuffled).unwrap(), baseline_id);

    let mut changed = baseline.clone();
    changed.discovery_policy.sha256 = Sha256Digest(format!("sha256:{}", "a".repeat(64)));
    assert_ne!(package_lock_id(&changed).unwrap(), baseline_id);
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
