//! Resource-bounded projection checking and manifest-last replacement.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::validator::Finding;

pub const SET_MANIFEST: &str = ".repository-engineering/generated-set.json";
const MAX_PATH_BYTES: usize = 1_024;
const MAX_ARTIFACT_BYTES: usize = 8 * 1024 * 1024;
const MAX_SET_BYTES: usize = 64 * 1024 * 1024;
const MAX_ARTIFACTS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Projection {
    pub relative_path: String,
    pub bytes: Vec<u8>,
}

impl Projection {
    pub fn new(relative_path: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            relative_path: relative_path.into(),
            bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionSet {
    artifacts: Vec<Projection>,
}

impl ProjectionSet {
    pub fn new(mut artifacts: Vec<Projection>) -> Result<Self, PipelineError> {
        if artifacts.len() > MAX_ARTIFACTS {
            return Err(PipelineError::new("generated.artifact_count_exceeded"));
        }
        let mut total = 0usize;
        let mut paths = BTreeSet::new();
        for artifact in &artifacts {
            validate_relative_path(&artifact.relative_path)?;
            if artifact.relative_path == SET_MANIFEST {
                return Err(PipelineError::new("generated.set_manifest.reserved"));
            }
            if artifact.bytes.len() > MAX_ARTIFACT_BYTES {
                return Err(PipelineError::new("generated.artifact_too_large"));
            }
            total = total
                .checked_add(artifact.bytes.len())
                .ok_or_else(|| PipelineError::new("generated.set_too_large"))?;
            if total > MAX_SET_BYTES {
                return Err(PipelineError::new("generated.set_too_large"));
            }
            if !paths.insert(artifact.relative_path.as_str()) {
                return Err(PipelineError::new("generated.artifact_duplicate"));
            }
        }
        artifacts.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        Ok(Self { artifacts })
    }

    pub fn artifacts(&self) -> &[Projection] {
        &self.artifacts
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineError {
    pub code: &'static str,
}

impl PipelineError {
    fn new(code: &'static str) -> Self {
        Self { code }
    }
}

impl fmt::Display for PipelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for PipelineError {}

pub fn generate_projection_set(
    output_root: &Path,
    expected: &ProjectionSet,
) -> Result<(), PipelineError> {
    generate_projection_set_with_stop(output_root, expected, usize::MAX)
}

#[doc(hidden)]
pub fn generate_projection_set_with_stop(
    output_root: &Path,
    expected: &ProjectionSet,
    stop_after: usize,
) -> Result<(), PipelineError> {
    validate_output_root(output_root, expected)?;
    let manifest = manifest_bytes(expected)?;

    remove_obsolete_artifacts(output_root, expected)?;

    for (index, artifact) in expected.artifacts.iter().enumerate() {
        if index == stop_after {
            return Err(PipelineError::new("generated.replacement_interrupted"));
        }
        replace_file(output_root, &artifact.relative_path, &artifact.bytes)?;
    }
    if expected.artifacts.len() == stop_after {
        return Err(PipelineError::new("generated.replacement_interrupted"));
    }
    replace_file(output_root, SET_MANIFEST, &manifest)?;
    Ok(())
}

pub fn check_projection_set(output_root: &Path, expected: &ProjectionSet) -> Vec<Finding> {
    let mut findings = Vec::new();
    let expected_manifest = match manifest_bytes(expected) {
        Ok(bytes) => bytes,
        Err(error) => {
            findings.push(generated_finding(
                SET_MANIFEST,
                error.code,
                "repair_projection",
            ));
            return findings;
        }
    };

    for artifact in &expected.artifacts {
        let path = output_root.join(&artifact.relative_path);
        match read_regular_file(&path) {
            Ok(actual) if actual == artifact.bytes => {}
            Ok(_) => findings.push(generated_finding(
                &artifact.relative_path,
                "generated.artifact.stale",
                "run_generate",
            )),
            Err(_) => findings.push(generated_finding(
                &artifact.relative_path,
                "generated.artifact.missing",
                "run_generate",
            )),
        }
    }

    let manifest_path = output_root.join(SET_MANIFEST);
    match read_regular_file(&manifest_path) {
        Ok(actual) if actual == expected_manifest => {}
        _ => findings.push(generated_finding(
            SET_MANIFEST,
            "generated.set_manifest.stale",
            "run_generate",
        )),
    }
    if let Ok(actual) = read_regular_file(&manifest_path) {
        if let Ok(previous) = serde_json::from_slice::<GeneratedSetManifest>(&actual) {
            let expected_paths: BTreeSet<_> = expected
                .artifacts
                .iter()
                .map(|artifact| artifact.relative_path.as_str())
                .collect();
            for artifact in previous.artifacts {
                if !expected_paths.contains(artifact.path.as_str()) {
                    findings.push(generated_finding(
                        &artifact.path,
                        "generated.artifact.extra",
                        "run_generate",
                    ));
                }
            }
        }
    }
    findings.sort();
    findings.truncate(256);
    findings
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GeneratedSetManifest {
    schema_version: String,
    artifacts: Vec<GeneratedArtifact>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GeneratedArtifact {
    path: String,
    sha256: String,
    byte_length: String,
}

fn manifest_bytes(expected: &ProjectionSet) -> Result<Vec<u8>, PipelineError> {
    let artifacts = expected
        .artifacts
        .iter()
        .map(|artifact| GeneratedArtifact {
            path: artifact.relative_path.clone(),
            sha256: format!("sha256:{:x}", Sha256::digest(&artifact.bytes)),
            byte_length: artifact.bytes.len().to_string(),
        })
        .collect();
    let mut bytes = serde_json::to_vec_pretty(&GeneratedSetManifest {
        schema_version: "v0".to_owned(),
        artifacts,
    })
    .map_err(|_| PipelineError::new("generated.set_manifest.serialize_failed"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn remove_obsolete_artifacts(
    output_root: &Path,
    expected: &ProjectionSet,
) -> Result<(), PipelineError> {
    let manifest_path = output_root.join(SET_MANIFEST);
    let Ok(bytes) = read_regular_file(&manifest_path) else {
        return Ok(());
    };
    if bytes.len() > MAX_ARTIFACT_BYTES {
        return Err(PipelineError::new("generated.set_manifest.invalid"));
    }
    let previous: GeneratedSetManifest = serde_json::from_slice(&bytes)
        .map_err(|_| PipelineError::new("generated.set_manifest.invalid"))?;
    if previous.schema_version != "v0" || previous.artifacts.len() > MAX_ARTIFACTS {
        return Err(PipelineError::new("generated.set_manifest.invalid"));
    }

    let expected_paths: BTreeSet<_> = expected
        .artifacts
        .iter()
        .map(|artifact| artifact.relative_path.as_str())
        .collect();
    let managed_parents: BTreeSet<_> = expected
        .artifacts
        .iter()
        .filter_map(|artifact| Path::new(&artifact.relative_path).parent())
        .collect();
    for artifact in previous.artifacts {
        validate_relative_path(&artifact.path)?;
        if expected_paths.contains(artifact.path.as_str()) {
            continue;
        }
        let path = Path::new(artifact.path.as_str());
        let parent = path
            .parent()
            .ok_or_else(|| PipelineError::new("generated.obsolete_path_unsafe"))?;
        if !managed_parents.contains(parent) {
            return Err(PipelineError::new("generated.obsolete_path_unsafe"));
        }
        let destination = output_root.join(path);
        match fs::symlink_metadata(&destination) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(PipelineError::new("generated.output_leaf_unsafe"));
            }
            Ok(_) => fs::remove_file(&destination)
                .map_err(|_| PipelineError::new("generated.obsolete_remove_failed"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(PipelineError::new("generated.obsolete_read_failed")),
        }
    }
    Ok(())
}

fn validate_output_root(output_root: &Path, expected: &ProjectionSet) -> Result<(), PipelineError> {
    let metadata = fs::symlink_metadata(output_root)
        .map_err(|_| PipelineError::new("generated.output_root_missing"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(PipelineError::new("generated.output_root_unsafe"));
    }
    for artifact in &expected.artifacts {
        let mut current = output_root.to_path_buf();
        let path = Path::new(&artifact.relative_path);
        if let Some(parent) = path.parent() {
            for component in parent.components() {
                if let Component::Normal(segment) = component {
                    current.push(segment);
                    if let Ok(metadata) = fs::symlink_metadata(&current) {
                        if metadata.file_type().is_symlink() || !metadata.is_dir() {
                            return Err(PipelineError::new("generated.output_parent_unsafe"));
                        }
                    }
                }
            }
        }
        let destination = output_root.join(&artifact.relative_path);
        if let Ok(metadata) = fs::symlink_metadata(destination) {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(PipelineError::new("generated.output_leaf_unsafe"));
            }
        }
    }
    Ok(())
}

fn replace_file(output_root: &Path, relative: &str, bytes: &[u8]) -> Result<(), PipelineError> {
    let destination = output_root.join(relative);
    let parent = destination
        .parent()
        .ok_or_else(|| PipelineError::new("generated.output_parent_missing"))?;
    fs::create_dir_all(parent)
        .map_err(|_| PipelineError::new("generated.output_parent_create_failed"))?;
    if fs::symlink_metadata(parent)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(true)
    {
        return Err(PipelineError::new("generated.output_parent_unsafe"));
    }

    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| PipelineError::new("generated.output_name_invalid"))?;
    let temporary = parent.join(format!(".{file_name}.repository-engineering.tmp"));
    if let Ok(metadata) = fs::symlink_metadata(&temporary) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(PipelineError::new("generated.temporary_unsafe"));
        }
        fs::remove_file(&temporary)
            .map_err(|_| PipelineError::new("generated.temporary_remove_failed"))?;
    }
    fs::write(&temporary, bytes)
        .map_err(|_| PipelineError::new("generated.temporary_write_failed"))?;
    fs::rename(&temporary, &destination)
        .map_err(|_| PipelineError::new("generated.replace_failed"))?;
    Ok(())
}

fn read_regular_file(path: &Path) -> Result<Vec<u8>, std::io::Error> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not a regular file",
        ));
    }
    fs::read(path)
}

fn validate_relative_path(path: &str) -> Result<(), PipelineError> {
    if path.is_empty()
        || path.len() > MAX_PATH_BYTES
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains(':')
        || Path::new(path)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || path
            .split('/')
            .any(|segment| segment.is_empty() || segment.ends_with('.') || segment.ends_with(' '))
        || path.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '\u{061c}'
                        | '\u{200e}'
                        | '\u{200f}'
                        | '\u{202a}'..='\u{202e}'
                        | '\u{2066}'..='\u{2069}'
                )
        })
    {
        return Err(PipelineError::new("generated.path_invalid"));
    }
    Ok(())
}

fn generated_finding(path: &str, code: &'static str, remediation: &'static str) -> Finding {
    Finding {
        path: path.to_owned(),
        logical_id: None,
        field: "artifact",
        code,
        remediation,
    }
}
