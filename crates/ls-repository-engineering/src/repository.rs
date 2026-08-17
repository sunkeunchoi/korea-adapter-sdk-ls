//! Composition of the authored package into its complete generated projection set.

use std::fmt;
use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::generate::{Projection, ProjectionSet};
use crate::identity::built_in_conformance_vector;
use crate::inventory::{discover_inventory, load_authored_package, reconcile_inventory};
use crate::lock::{build_lock, lock_bytes};
use crate::schema::{
    schema_catalog, ArtifactReference, BuildProvenance, NormativeLockClosure, RepositoryPath,
    SchemaVersion, Sha256Digest,
};
use crate::validator::validate_first_slice_package;

const PACKAGE_PATH: &str = ".repository-engineering/package.toml";
const DISCOVERY_PATH: &str = ".repository-engineering/discovery-policy.toml";
const LEDGER_PATH: &str = ".repository-engineering/migration-ledger.toml";
const REGISTRY_PATH: &str = ".repository-engineering/schema-registry.json";
const CONFORMANCE_MANIFEST_PATH: &str = ".repository-engineering/conformance/v0/manifest.json";
const LOCK_PATH: &str = ".repository-engineering/package.lock.json";
const REFERENCE_PATH: &str = "docs/reference/repository-engineering-package.md";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryError {
    pub code: &'static str,
}

impl RepositoryError {
    fn new(code: &'static str) -> Self {
        Self { code }
    }
}

impl fmt::Display for RepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for RepositoryError {}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct SchemaRegistry {
    schema_version: SchemaVersion,
    entries: Vec<SchemaRegistryEntry>,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct SchemaRegistryEntry {
    schema_id: String,
    artifact: ArtifactReference,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct ConformanceManifest {
    schema_version: SchemaVersion,
    artifacts: Vec<ArtifactReference>,
}

pub fn compose_repository(root: &Path) -> Result<ProjectionSet, RepositoryError> {
    let authored = load_authored_package(root).map_err(|error| RepositoryError::new(error.code))?;
    if !validate_first_slice_package(&authored.package).is_empty() {
        return Err(RepositoryError::new("repository.package.invalid"));
    }
    let inventory = discover_inventory(root, &authored.discovery_policy)
        .map_err(|error| RepositoryError::new(error.code))?;
    if !reconcile_inventory(&authored.ledger, &inventory).is_empty() {
        return Err(RepositoryError::new("repository.inventory.invalid"));
    }

    let mut projections = Vec::new();
    let mut registry_entries = Vec::new();
    for (name, mut schema) in schema_catalog() {
        let schema_id = format!("urn:ls:repository-engineering:schema:v0:{name}");
        schema
            .as_object_mut()
            .ok_or_else(|| RepositoryError::new("repository.schema.invalid"))?
            .insert("$id".to_owned(), Value::String(schema_id.clone()));
        let path = format!(".repository-engineering/schemas/v0/{name}.schema.json");
        let bytes = pretty_json(&schema)?;
        registry_entries.push(SchemaRegistryEntry {
            schema_id,
            artifact: bytes_reference(&path, &bytes, "application/schema+json"),
        });
        projections.push(Projection::new(path, bytes));
    }
    let registry_bytes = pretty_json(&SchemaRegistry {
        schema_version: SchemaVersion::V0,
        entries: registry_entries,
    })?;
    projections.push(Projection::new(REGISTRY_PATH, registry_bytes.clone()));

    let structural_path = ".repository-engineering/conformance/v0/structural.json";
    let structural_bytes = pretty_json(&json!({
        "schema_version": "v0",
        "validates": [
            PACKAGE_PATH,
            DISCOVERY_PATH,
            LEDGER_PATH,
            REGISTRY_PATH
        ],
        "unknown_fields": "reject",
        "unsupported_schema_versions": "reject"
    }))?;
    let cross_record_path = ".repository-engineering/conformance/v0/cross-record.json";
    let cross_record_bytes = pretty_json(&json!({
        "schema_version": "v0",
        "rules": [
            "every_discovered_obligation_has_exactly_one_ledger_row",
            "every_ledger_source_matches_its_discovered_source",
            "first_slice_authority_remains_legacy",
            "first_slice_migration_state_remains_unported",
            "first_slice_activation_eligibility_is_none"
        ]
    }))?;
    let vector_path = ".repository-engineering/conformance/v0/version-set-vector.json";
    let (vector_input, expected_version_set_id) = built_in_conformance_vector()
        .map_err(|_| RepositoryError::new("repository.conformance.vector_failed"))?;
    let vector_bytes = pretty_json(&json!({
        "schema_version": "v0",
        "fixture_only": true,
        "input": vector_input,
        "expected_version_set_id": expected_version_set_id
    }))?;
    let conformance_artifacts = vec![
        bytes_reference(structural_path, &structural_bytes, "application/json"),
        bytes_reference(cross_record_path, &cross_record_bytes, "application/json"),
        bytes_reference(vector_path, &vector_bytes, "application/json"),
    ];
    let conformance_manifest_bytes = pretty_json(&ConformanceManifest {
        schema_version: SchemaVersion::V0,
        artifacts: conformance_artifacts,
    })?;
    projections.extend([
        Projection::new(structural_path, structural_bytes),
        Projection::new(cross_record_path, cross_record_bytes),
        Projection::new(vector_path, vector_bytes),
        Projection::new(
            CONFORMANCE_MANIFEST_PATH,
            conformance_manifest_bytes.clone(),
        ),
    ]);

    let normative = NormativeLockClosure {
        package: semantic_reference(PACKAGE_PATH, &authored.package, "application/toml")?,
        discovery_policy: semantic_reference(
            DISCOVERY_PATH,
            &authored.discovery_policy,
            "application/toml",
        )?,
        migration_ledger: semantic_reference(LEDGER_PATH, &authored.ledger, "application/toml")?,
        schema_registry: bytes_reference(REGISTRY_PATH, &registry_bytes, "application/json"),
        conformance_corpus: bytes_reference(
            CONFORMANCE_MANIFEST_PATH,
            &conformance_manifest_bytes,
            "application/json",
        ),
        optional_components: authored.package.optional_components.clone(),
    };
    let build_provenance = BuildProvenance {
        generator: tree_reference(
            root,
            "crates/ls-repository-engineering",
            "application/vnd.rust.crate",
        )?,
        dependency_lock: file_reference(root, "Cargo.lock", "application/toml")?,
        workflow_pins: workflow_pins(root)?,
    };
    let exact_lock = build_lock(normative, build_provenance)
        .map_err(|_| RepositoryError::new("repository.lock.identity_failed"))?;
    projections.push(Projection::new(
        LOCK_PATH,
        lock_bytes(&exact_lock)
            .map_err(|_| RepositoryError::new("repository.lock.serialize_failed"))?,
    ));
    projections.push(Projection::new(
        REFERENCE_PATH,
        reference_document(&authored.ledger, &exact_lock.package_lock_id.0).into_bytes(),
    ));

    ProjectionSet::new(projections).map_err(|error| RepositoryError::new(error.code))
}

fn workflow_pins(root: &Path) -> Result<Vec<ArtifactReference>, RepositoryError> {
    let path = ".github/workflows/repository-engineering-check.yml";
    if root.join(path).exists() {
        Ok(vec![file_reference(root, path, "application/yaml")?])
    } else {
        Ok(Vec::new())
    }
}

fn semantic_reference<T: Serialize>(
    path: &str,
    value: &T,
    media_type: &str,
) -> Result<ArtifactReference, RepositoryError> {
    let canonical = serde_json_canonicalizer::to_vec(value)
        .map_err(|_| RepositoryError::new("repository.semantic_digest_failed"))?;
    Ok(bytes_reference(path, &canonical, media_type))
}

fn file_reference(
    root: &Path,
    path: &str,
    media_type: &str,
) -> Result<ArtifactReference, RepositoryError> {
    let bytes = fs::read(root.join(path))
        .map_err(|_| RepositoryError::new("repository.provenance.read_failed"))?;
    Ok(bytes_reference(path, &bytes, media_type))
}

fn tree_reference(
    root: &Path,
    relative: &str,
    media_type: &str,
) -> Result<ArtifactReference, RepositoryError> {
    let base = root.join(relative);
    let mut files = Vec::new();
    collect_regular_files(&base, &base, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = Sha256::new();
    hasher.update(b"ls-repository-engineering/tree/v0\0");
    for (path, bytes) in files {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(&bytes);
        hasher.update([0]);
    }
    Ok(ArtifactReference {
        schema_version: SchemaVersion::V0,
        path: RepositoryPath(relative.to_owned()),
        sha256: Sha256Digest(format!("sha256:{:x}", hasher.finalize())),
        media_type: media_type.to_owned(),
    })
}

fn collect_regular_files(
    base: &Path,
    directory: &Path,
    files: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), RepositoryError> {
    let mut entries: Vec<_> = fs::read_dir(directory)
        .map_err(|_| RepositoryError::new("repository.provenance.read_failed"))?
        .collect::<Result<_, _>>()
        .map_err(|_| RepositoryError::new("repository.provenance.read_failed"))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let metadata = entry
            .file_type()
            .map_err(|_| RepositoryError::new("repository.provenance.read_failed"))?;
        let path = entry.path();
        if metadata.is_symlink() {
            return Err(RepositoryError::new(
                "repository.provenance.symlink_forbidden",
            ));
        }
        if metadata.is_dir() {
            collect_regular_files(base, &path, files)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(base)
                .ok()
                .and_then(Path::to_str)
                .ok_or_else(|| RepositoryError::new("repository.provenance.path_invalid"))?
                .replace('\\', "/");
            let bytes = fs::read(path)
                .map_err(|_| RepositoryError::new("repository.provenance.read_failed"))?;
            files.push((relative, bytes));
        }
    }
    Ok(())
}

fn bytes_reference(path: &str, bytes: &[u8], media_type: &str) -> ArtifactReference {
    ArtifactReference {
        schema_version: SchemaVersion::V0,
        path: RepositoryPath(path.to_owned()),
        sha256: Sha256Digest(format!("sha256:{:x}", Sha256::digest(bytes))),
        media_type: media_type.to_owned(),
    }
}

fn pretty_json<T: Serialize>(value: &T) -> Result<Vec<u8>, RepositoryError> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| RepositoryError::new("repository.projection.serialize_failed"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn reference_document(ledger: &crate::schema::MigrationLedger, package_lock_id: &str) -> String {
    let mut rows: Vec<_> = ledger.rows.iter().collect();
    rows.sort_by(|left, right| left.logical_id.cmp(&right.logical_id));
    let mut document = format!(
        "# Repository Engineering Package\n\nThis page is generated from the inert, reviewed package declaration. It grants no runtime or publication authority.\n\n- Schema version: `v0`\n- Package lock identity: `{package_lock_id}`\n- Activation eligibility: `none`\n- Reviewed migration rows: `{}`\n\nDeclaration, implementation, certification, authority, and retirement are independent states. Every row below remains unported, uncertified, legacy-authoritative, and not retired.\n\n| Logical ID | Source kind | Source locator | Disposition | Declaration | Implementation | Certification | Authority | Retirement | Replacement |\n|---|---|---|---|---|---|---|---|---|---|\n",
        rows.len()
    );
    for row in rows {
        let replacement = row
            .replacement_contract
            .as_ref()
            .map(|value| value.0.as_str())
            .unwrap_or("absent");
        document.push_str(&format!(
            "| {} | {} | `{}` | {} | declared | unported | uncertified | legacy | not_started | {} |\n",
            escape_markdown(&row.logical_id.0),
            escape_markdown(&row.source_kind.0),
            escape_markdown(&row.source_locator.0),
            enum_token(&row.disposition),
            escape_markdown(replacement),
        ));
    }
    document
}

fn enum_token<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_owned())
}

fn escape_markdown(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace('\r', " ")
        .replace('\n', " ")
}
