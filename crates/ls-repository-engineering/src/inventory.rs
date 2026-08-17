//! Exact tracked-tree discovery and Migration Ledger reconciliation.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::schema::{
    DiscoveryPolicy, MigrationLedger, PackageManifest, RepositoryPath, Sha256Digest, StableId,
};
use crate::validator::Finding;

const PACKAGE_ROOT: &str = ".repository-engineering";
const MAX_AUTHORED_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_TRACKED_PATHS: usize = 100_000;
const MAX_PATH_BYTES: usize = 1_024;

#[derive(Debug, Clone)]
pub struct AuthoredPackage {
    pub package: PackageManifest,
    pub discovery_policy: DiscoveryPolicy,
    pub ledger: MigrationLedger,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Obligation {
    pub logical_id: StableId,
    pub source_kind: StableId,
    pub source_locator: RepositoryPath,
    pub source_digest: Option<Sha256Digest>,
}

#[derive(Debug, Clone)]
pub struct Inventory {
    pub obligations: Vec<Obligation>,
    pub unclassified_paths: Vec<RepositoryPath>,
}

impl Inventory {
    pub fn count_kind(&self, source_kind: &str) -> usize {
        self.obligations
            .iter()
            .filter(|obligation| obligation.source_kind.0 == source_kind)
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoredError {
    pub path: PathBuf,
    pub code: &'static str,
}

impl fmt::Display for AuthoredError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path.display(), self.code)
    }
}

impl std::error::Error for AuthoredError {}

pub fn load_authored_package(root: &Path) -> Result<AuthoredPackage, AuthoredError> {
    let package_root = root.join(PACKAGE_ROOT);
    Ok(AuthoredPackage {
        package: read_toml(&package_root.join("package.toml"))?,
        discovery_policy: read_toml(&package_root.join("discovery-policy.toml"))?,
        ledger: read_toml(&package_root.join("migration-ledger.toml"))?,
    })
}

pub fn discover_inventory(
    root: &Path,
    policy: &DiscoveryPolicy,
) -> Result<Inventory, AuthoredError> {
    let tracked = tracked_entries(root)?;
    let mut claimed = BTreeSet::new();
    let mut obligations = Vec::new();

    validate_discoverer_ownership(policy)?;
    for discoverer in &policy.discoverers {
        match discoverer.source_kind.0.as_str() {
            "capability" => discover_capabilities(
                root,
                &tracked,
                &discoverer.prefix.0,
                discoverer.marker.as_deref().unwrap_or("SKILL.md"),
                &mut claimed,
                &mut obligations,
            )?,
            "claude_alias" => discover_single_files(
                root,
                &tracked,
                &discoverer.prefix.0,
                "claude-alias",
                "claude_alias",
                &mut claimed,
                &mut obligations,
            )?,
            "worker_role" => discover_single_files(
                root,
                &tracked,
                &discoverer.prefix.0,
                "worker-role",
                "worker_role",
                &mut claimed,
                &mut obligations,
            )?,
            _ => {
                return Err(AuthoredError {
                    path: root.join(PACKAGE_ROOT).join("discovery-policy.toml"),
                    code: "discovery.unknown_source_kind",
                });
            }
        }
    }

    for path in &policy.exact_instruction_paths {
        if tracked.contains_key(&path.0) {
            claimed.insert(path.0.clone());
            obligations.push(Obligation {
                logical_id: StableId(format!("instruction--{}", slug(&path.0))),
                source_kind: StableId("instruction_config".to_owned()),
                source_locator: path.clone(),
                source_digest: Some(digest_source(root, &path.0, std::slice::from_ref(&path.0))?),
            });
        }
    }

    obligations.extend(
        policy
            .declared_obligations
            .iter()
            .map(|declared| Obligation {
                logical_id: declared.logical_id.clone(),
                source_kind: declared.source_kind.clone(),
                source_locator: declared.locator.clone(),
                source_digest: None,
            }),
    );

    let mut unclassified_paths = Vec::new();
    for path in tracked.keys() {
        if claimed.contains(path)
            || policy
                .exclusions
                .iter()
                .any(|rule| matches_prefix(path, &rule.prefix.0))
        {
            continue;
        }
        unclassified_paths.push(RepositoryPath(path.clone()));
    }

    obligations.sort_by(|left, right| left.logical_id.cmp(&right.logical_id));
    unclassified_paths.sort();
    Ok(Inventory {
        obligations,
        unclassified_paths,
    })
}

pub fn reconcile_inventory(ledger: &MigrationLedger, inventory: &Inventory) -> Vec<Finding> {
    let mut discovered = BTreeMap::new();
    let mut ledger_ids = BTreeSet::new();
    let mut folded_ledger_ids = BTreeSet::new();
    let mut findings = Vec::new();

    for obligation in &inventory.obligations {
        if discovered
            .insert(obligation.logical_id.0.as_str(), obligation)
            .is_some()
        {
            findings.push(finding(
                Some(obligation.logical_id.0.clone()),
                "logical_id",
                "inventory.discovery.duplicate",
                "rename_colliding_source",
            ));
        }
    }

    for row in &ledger.rows {
        if !ledger_ids.insert(row.logical_id.0.as_str()) {
            findings.push(finding(
                Some(row.logical_id.0.clone()),
                "logical_id",
                "inventory.ledger.duplicate",
                "remove_duplicate_row",
            ));
            continue;
        }
        if !folded_ledger_ids.insert(row.logical_id.0.to_ascii_lowercase()) {
            findings.push(finding(
                Some(row.logical_id.0.clone()),
                "logical_id",
                "inventory.ledger.case_collision",
                "rename_colliding_row",
            ));
        }
        match discovered.get(row.logical_id.0.as_str()) {
            None => findings.push(finding(
                Some(row.logical_id.0.clone()),
                "logical_id",
                "inventory.ledger.extra",
                "remove_or_reclassify_row",
            )),
            Some(obligation) => {
                if row.source_kind != obligation.source_kind
                    || row.source_locator != obligation.source_locator
                    || row.source_digest != obligation.source_digest
                {
                    findings.push(finding(
                        Some(row.logical_id.0.clone()),
                        "source",
                        "inventory.ledger.source_mismatch",
                        "refresh_reviewed_source",
                    ));
                }
            }
        }
        if row.current_authority != crate::schema::AuthorityState::Legacy {
            findings.push(finding(
                Some(row.logical_id.0.clone()),
                "current_authority",
                "inventory.authority.transfer_forbidden",
                "restore_legacy_authority",
            ));
        }
        if row.migration_state != crate::schema::MigrationState::Unported {
            findings.push(finding(
                Some(row.logical_id.0.clone()),
                "migration_state",
                "inventory.migration_state.forbidden",
                "restore_unported_state",
            ));
        }
        if row.absence_reason.is_none() {
            findings.push(finding(
                Some(row.logical_id.0.clone()),
                "absence_reason",
                "inventory.absence_reason.required",
                "record_absence_reason",
            ));
        }
        if row.replacement_contract.is_some() || row.parity_reference.is_some() {
            findings.push(finding(
                Some(row.logical_id.0.clone()),
                "successor",
                "inventory.successor_reference.forbidden",
                "remove_unported_successor_reference",
            ));
        }
    }

    for logical_id in discovered.keys() {
        if !ledger_ids.contains(logical_id) {
            findings.push(finding(
                Some((*logical_id).to_owned()),
                "logical_id",
                "inventory.ledger.missing",
                "author_reviewed_disposition",
            ));
        }
    }
    for path in &inventory.unclassified_paths {
        findings.push(finding(
            None,
            "path",
            "inventory.path.unclassified",
            "classify_or_review_exclusion",
        ));
        if findings.len() == 256 {
            break;
        }
        let _ = path;
    }
    findings.sort();
    findings.truncate(256);
    findings
}

fn read_toml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, AuthoredError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| AuthoredError {
        path: path.to_path_buf(),
        code: "authored.read_failed",
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AuthoredError {
            path: path.to_path_buf(),
            code: "authored.input_unsafe",
        });
    }
    if metadata.len() > MAX_AUTHORED_FILE_BYTES {
        return Err(AuthoredError {
            path: path.to_path_buf(),
            code: "authored.file_too_large",
        });
    }
    let bytes = fs::read(path).map_err(|_| AuthoredError {
        path: path.to_path_buf(),
        code: "authored.read_failed",
    })?;
    let text = std::str::from_utf8(&bytes).map_err(|_| AuthoredError {
        path: path.to_path_buf(),
        code: "authored.invalid_utf8",
    })?;
    toml::from_str(text).map_err(|_| AuthoredError {
        path: path.to_path_buf(),
        code: "authored.parse_failed",
    })
}

fn tracked_entries(root: &Path) -> Result<BTreeMap<String, String>, AuthoredError> {
    let output = Command::new("git")
        .args(["-C", root.to_str().unwrap_or("."), "ls-files", "-s", "-z"])
        .output()
        .map_err(|_| AuthoredError {
            path: root.to_path_buf(),
            code: "inventory.git_unavailable",
        })?;
    if !output.status.success() {
        return Err(AuthoredError {
            path: root.to_path_buf(),
            code: "inventory.git_failed",
        });
    }

    let mut entries = BTreeMap::new();
    for record in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let record = std::str::from_utf8(record).map_err(|_| AuthoredError {
            path: root.to_path_buf(),
            code: "inventory.path.invalid_utf8",
        })?;
        let (metadata, path) = record.split_once('\t').ok_or_else(|| AuthoredError {
            path: root.to_path_buf(),
            code: "inventory.git_record_invalid",
        })?;
        if path.len() > MAX_PATH_BYTES || !safe_repository_path(path) {
            return Err(AuthoredError {
                path: root.to_path_buf(),
                code: "inventory.path.unsafe",
            });
        }
        let mode = metadata.split_whitespace().next().unwrap_or_default();
        entries.insert(path.to_owned(), mode.to_owned());
        if entries.len() > MAX_TRACKED_PATHS {
            return Err(AuthoredError {
                path: root.to_path_buf(),
                code: "inventory.path_limit_exceeded",
            });
        }
    }
    Ok(entries)
}

fn validate_discoverer_ownership(policy: &DiscoveryPolicy) -> Result<(), AuthoredError> {
    let mut kinds = BTreeSet::new();
    if policy
        .discoverers
        .iter()
        .any(|discoverer| !kinds.insert(discoverer.source_kind.0.to_ascii_lowercase()))
    {
        return Err(AuthoredError {
            path: PathBuf::from(PACKAGE_ROOT).join("discovery-policy.toml"),
            code: "discovery.duplicate_source_kind",
        });
    }
    for discoverer in &policy.discoverers {
        let marker_valid = discoverer.marker.as_ref().is_none_or(|marker| {
            !marker.is_empty()
                && marker.len() <= 128
                && !marker.contains(['/', '\\'])
                && marker.is_ascii()
        });
        let shape_valid = match discoverer.source_kind.0.as_str() {
            "capability" => discoverer.include_descendants && discoverer.marker.is_some(),
            "claude_alias" | "worker_role" => !discoverer.include_descendants,
            _ => true,
        };
        if !marker_valid || !shape_valid {
            return Err(AuthoredError {
                path: PathBuf::from(PACKAGE_ROOT).join("discovery-policy.toml"),
                code: "discovery.discoverer_invalid",
            });
        }
    }
    Ok(())
}

fn discover_capabilities(
    root: &Path,
    tracked: &BTreeMap<String, String>,
    prefix: &str,
    marker: &str,
    claimed: &mut BTreeSet<String>,
    obligations: &mut Vec<Obligation>,
) -> Result<(), AuthoredError> {
    let suffix = format!("/{marker}");
    for path in tracked
        .keys()
        .filter(|path| path.starts_with(prefix) && path.ends_with(&suffix))
    {
        let directory = path.strip_suffix(&suffix).unwrap();
        let id = directory.rsplit('/').next().unwrap_or_default();
        let closure: Vec<_> = tracked
            .keys()
            .filter(|candidate| matches_prefix(candidate, &format!("{directory}/")))
            .cloned()
            .collect();
        claimed.extend(closure.iter().cloned());
        obligations.push(Obligation {
            logical_id: StableId(format!("capability--{id}")),
            source_kind: StableId("capability".to_owned()),
            source_locator: RepositoryPath(directory.to_owned()),
            source_digest: Some(digest_source(root, directory, &closure)?),
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn discover_single_files(
    root: &Path,
    tracked: &BTreeMap<String, String>,
    prefix: &str,
    id_prefix: &str,
    source_kind: &str,
    claimed: &mut BTreeSet<String>,
    obligations: &mut Vec<Obligation>,
) -> Result<(), AuthoredError> {
    for path in tracked.keys().filter(|path| path.starts_with(prefix)) {
        let remainder = path.trim_start_matches(prefix).trim_start_matches('/');
        if remainder.contains('/') {
            continue;
        }
        let id = remainder.strip_suffix(".md").unwrap_or(remainder);
        if source_kind == "claude_alias" {
            let expected = PathBuf::from(format!("../../.agents/skills/{id}"));
            let actual = fs::read_link(root.join(path)).map_err(|_| AuthoredError {
                path: PathBuf::from(path),
                code: "inventory.alias_target_unreadable",
            })?;
            if actual != expected {
                return Err(AuthoredError {
                    path: PathBuf::from(path),
                    code: "inventory.alias_target_invalid",
                });
            }
        }
        claimed.insert(path.clone());
        obligations.push(Obligation {
            logical_id: StableId(format!("{id_prefix}--{id}")),
            source_kind: StableId(source_kind.to_owned()),
            source_locator: RepositoryPath(path.clone()),
            source_digest: Some(digest_source(root, path, std::slice::from_ref(path))?),
        });
    }
    Ok(())
}

fn digest_source(
    root: &Path,
    label: &str,
    paths: &[String],
) -> Result<Sha256Digest, AuthoredError> {
    let mut hasher = Sha256::new();
    hasher.update(b"ls-repository-engineering/source-closure/v0\0");
    hasher.update(label.as_bytes());
    hasher.update([0]);
    for path in paths {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        let absolute = root.join(path);
        let metadata = fs::symlink_metadata(&absolute).map_err(|_| AuthoredError {
            path: PathBuf::from(path),
            code: "inventory.source_read_failed",
        })?;
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&absolute).map_err(|_| AuthoredError {
                path: PathBuf::from(path),
                code: "inventory.symlink_read_failed",
            })?;
            hasher.update(target.to_string_lossy().as_bytes());
        } else {
            hasher.update(fs::read(&absolute).map_err(|_| AuthoredError {
                path: PathBuf::from(path),
                code: "inventory.source_read_failed",
            })?);
        }
        hasher.update([0]);
    }
    Ok(Sha256Digest(format!("sha256:{:x}", hasher.finalize())))
}

fn safe_repository_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        && !path.split('/').any(|segment| {
            segment.ends_with('.') || segment.ends_with(' ') || segment.contains(':')
        })
        && !path.chars().any(|character| {
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
}

fn matches_prefix(path: &str, prefix: &str) -> bool {
    path == prefix || (prefix.ends_with('/') && path.starts_with(prefix))
}

fn slug(path: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for character in path.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    slug.trim_matches('-').to_owned()
}

fn finding(
    logical_id: Option<String>,
    field: &'static str,
    code: &'static str,
    remediation: &'static str,
) -> Finding {
    Finding {
        path: ".repository-engineering/migration-ledger.toml".to_owned(),
        logical_id,
        field,
        code,
        remediation,
    }
}
