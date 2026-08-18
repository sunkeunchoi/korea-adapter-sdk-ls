use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::Deserialize;
use sha2::{Digest, Sha256};

const MANIFEST_PATH: &str = ".repository-engineering/runtime-bundle.json";
const MAX_MEMBER_BYTES: u64 = 2 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 32 * 1024 * 1024;
const MAX_MEMBERS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleManifest {
    schema_version: String,
    bundle_id: String,
    members: Vec<ArtifactReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactReference {
    schema_version: String,
    path: String,
    sha256: String,
    media_type: String,
}

#[derive(Debug, Clone)]
pub struct LoadedBundle {
    members: BTreeMap<String, Vec<u8>>,
}

impl LoadedBundle {
    pub fn member(&self, path: &str) -> Option<&[u8]> {
        self.members.get(path).map(Vec::as_slice)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleError {
    RootUnavailable,
    ManifestInvalid,
    MemberMissing,
    DigestMismatch,
    UnsafePath,
    LimitExceeded,
    DuplicateMember,
    UnlistedMember,
}

impl std::fmt::Display for BundleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for BundleError {}

pub fn load_bundle(root: &Path) -> Result<LoadedBundle, BundleError> {
    let root = fs::canonicalize(root).map_err(|_| BundleError::RootUnavailable)?;
    if !root.is_dir() {
        return Err(BundleError::RootUnavailable);
    }
    let manifest_bytes = read_member(&root, MANIFEST_PATH, BundleError::ManifestInvalid)?;
    let mut deserializer = serde_json::Deserializer::from_slice(&manifest_bytes);
    let manifest =
        BundleManifest::deserialize(&mut deserializer).map_err(|_| BundleError::ManifestInvalid)?;
    deserializer
        .end()
        .map_err(|_| BundleError::ManifestInvalid)?;
    if manifest.schema_version != "v0"
        || !valid_id(&manifest.bundle_id)
        || manifest.members.is_empty()
        || manifest.members.len() > MAX_MEMBERS
    {
        return Err(BundleError::ManifestInvalid);
    }

    let mut members = BTreeMap::new();
    let mut expected_paths = BTreeSet::new();
    let mut total_bytes = 0usize;
    for reference in manifest.members {
        if reference.schema_version != "v0"
            || !valid_path(&reference.path)
            || !valid_digest(&reference.sha256)
            || reference.media_type.is_empty()
        {
            return Err(BundleError::ManifestInvalid);
        }
        if !expected_paths.insert(reference.path.clone()) {
            return Err(BundleError::DuplicateMember);
        }
        let bytes = read_member(&root, &reference.path, BundleError::MemberMissing)?;
        total_bytes = total_bytes
            .checked_add(bytes.len())
            .ok_or(BundleError::LimitExceeded)?;
        if total_bytes > MAX_TOTAL_BYTES {
            return Err(BundleError::LimitExceeded);
        }
        let actual = format!("sha256:{:x}", Sha256::digest(&bytes));
        if actual != reference.sha256 {
            return Err(BundleError::DigestMismatch);
        }
        members.insert(reference.path, bytes);
    }

    let mut actual_paths = BTreeSet::new();
    collect_files(&root, &root, &mut actual_paths)?;
    actual_paths.remove(MANIFEST_PATH);
    if actual_paths != expected_paths {
        return Err(BundleError::UnlistedMember);
    }
    Ok(LoadedBundle { members })
}

fn read_member(root: &Path, relative: &str, missing: BundleError) -> Result<Vec<u8>, BundleError> {
    if !valid_path(relative) {
        return Err(BundleError::UnsafePath);
    }
    let mut current = root.to_path_buf();
    let mut components = relative.split('/').peekable();
    while let Some(component) = components.next() {
        current.push(component);
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Err(missing),
            Err(_) => return Err(missing),
        };
        if metadata.file_type().is_symlink() {
            return Err(BundleError::UnsafePath);
        }
        if components.peek().is_some() && !metadata.is_dir() {
            return Err(BundleError::UnsafePath);
        }
        if components.peek().is_none() && (!metadata.is_file() || metadata.len() > MAX_MEMBER_BYTES)
        {
            return Err(BundleError::LimitExceeded);
        }
    }
    fs::read(current).map_err(|_| missing)
}

fn collect_files(
    root: &Path,
    directory: &Path,
    paths: &mut BTreeSet<String>,
) -> Result<(), BundleError> {
    let mut entries: Vec<_> = fs::read_dir(directory)
        .map_err(|_| BundleError::RootUnavailable)?
        .collect::<Result<_, _>>()
        .map_err(|_| BundleError::RootUnavailable)?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let file_type = entry.file_type().map_err(|_| BundleError::UnsafePath)?;
        if file_type.is_symlink() {
            return Err(BundleError::UnsafePath);
        }
        if file_type.is_dir() {
            collect_files(root, &entry.path(), paths)?;
        } else if file_type.is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)
                .ok()
                .and_then(Path::to_str)
                .ok_or(BundleError::UnsafePath)?
                .replace('\\', "/");
            paths.insert(relative);
        } else {
            return Err(BundleError::UnsafePath);
        }
    }
    Ok(())
}

fn valid_id(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1024
        && !value.starts_with('/')
        && !value.contains('\\')
        && value
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

fn valid_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
