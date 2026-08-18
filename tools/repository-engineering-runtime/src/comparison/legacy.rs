use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::model::AuditVerdict;

use super::{confined_existing, read_bounded, verify_digest, ComparisonCase, ComparisonError};

const CREDENTIAL_FIELDS: &[&str] = &[
    "line",
    "reason",
    "claim_text",
    "target_location",
    "coherence_note",
    "acceptance_reason",
    "unverifiable_reason",
    "gap",
];
const CREDENTIAL_PATTERNS: &[&str] = &[
    "rsp_msg",
    "appkey",
    "app_key",
    "apikey",
    "api_key",
    "secret",
    "password",
    "passwd",
    "bearer",
    "authorization",
    "account_no",
    "acnt_no",
    "accountno",
    "account_number",
    "token=",
];

#[derive(Debug)]
pub(super) struct LegacyRow {
    pub case_id: String,
    pub classification: String,
    pub verdict: AuditVerdict,
    pub completed: bool,
    pub blocking: bool,
    pub roll_up: String,
    pub credential_rule: bool,
    pub path_rule: bool,
}

#[derive(Debug)]
pub(super) struct LegacyArtifact {
    path: String,
    bytes: Vec<u8>,
}

#[derive(Debug)]
pub(super) struct LegacyNormalization {
    pub rows: Vec<LegacyRow>,
    pub artifacts: Vec<LegacyArtifact>,
}

pub(super) fn normalize(
    root: &Path,
    cases: &[ComparisonCase],
) -> Result<LegacyNormalization, ComparisonError> {
    let mut rows = Vec::with_capacity(cases.len());
    let mut artifacts = Vec::with_capacity(cases.len());
    for case in cases {
        let path = confined_existing(root, &case.legacy_record.path)?;
        let bytes = read_bounded(&path)?;
        verify_digest(&bytes, &case.legacy_record.sha256)?;
        let fields = top_level_fields(&bytes)?;
        let row_id = required(&fields, "row_id")?;
        let classification = required(&fields, "classification")?;
        let verdict = parse_verdict(required(&fields, "verdict")?)?;
        let evidence_pointer = required(&fields, "evidence_pointer")?;
        if row_id != case.row_id || classification != case.expected_classification {
            return Err(ComparisonError::SemanticDifference);
        }
        let (blocking, roll_up) = match verdict {
            AuditVerdict::Confirmed => (false, "unchanged"),
            AuditVerdict::Refuted => (true, "redisposition_required"),
            AuditVerdict::Unverifiable => (true, "unchanged_blocked"),
        };
        rows.push(LegacyRow {
            case_id: case.case_id.clone(),
            classification: classification.to_owned(),
            verdict,
            completed: true,
            blocking,
            roll_up: roll_up.to_owned(),
            credential_rule: credential_fields_are_safe(&bytes)?,
            path_rule: valid_evidence_pointer(root, evidence_pointer),
        });
        artifacts.push(LegacyArtifact {
            path: case.legacy_record.path.clone(),
            bytes,
        });
    }
    Ok(LegacyNormalization { rows, artifacts })
}

pub(super) fn corpus_digest(artifacts: &[LegacyArtifact]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"audit-bounded-comparison/corpus/v0\0");
    hasher.update((artifacts.len() as u64).to_be_bytes());
    for artifact in artifacts {
        hasher.update(artifact.path.as_bytes());
        hasher.update([0]);
        hasher.update((artifact.bytes.len() as u64).to_be_bytes());
        hasher.update(&artifact.bytes);
        hasher.update([0]);
    }
    format!("sha256:{:x}", hasher.finalize())
}

fn top_level_fields(bytes: &[u8]) -> Result<BTreeMap<String, String>, ComparisonError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ComparisonError::InvalidInput)?;
    let mut fields = BTreeMap::new();
    for line in text.lines() {
        if line.is_empty()
            || line.starts_with(char::is_whitespace)
            || line.trim_start().starts_with('#')
            || line == "---"
        {
            continue;
        }
        let Some((key, raw)) = line.split_once(':') else {
            return Err(ComparisonError::InvalidInput);
        };
        if !matches!(
            key,
            "row_id" | "classification" | "verdict" | "evidence_pointer"
        ) {
            continue;
        }
        let value = unquote(raw.trim())?;
        if value.is_empty() || fields.insert(key.to_owned(), value).is_some() {
            return Err(ComparisonError::InvalidInput);
        }
    }
    Ok(fields)
}

fn unquote(value: &str) -> Result<String, ComparisonError> {
    if let Some(inner) = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    {
        if inner.contains('"') || inner.contains('\\') {
            return Err(ComparisonError::InvalidInput);
        }
        return Ok(inner.to_owned());
    }
    if let Some(inner) = value
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
    {
        if inner.contains('\'') {
            return Err(ComparisonError::InvalidInput);
        }
        return Ok(inner.to_owned());
    }
    if value.contains(['#', '"', '\'']) {
        return Err(ComparisonError::InvalidInput);
    }
    Ok(value.to_owned())
}

fn required<'a>(
    fields: &'a BTreeMap<String, String>,
    key: &str,
) -> Result<&'a str, ComparisonError> {
    fields
        .get(key)
        .map(String::as_str)
        .ok_or(ComparisonError::InvalidInput)
}

fn parse_verdict(value: &str) -> Result<AuditVerdict, ComparisonError> {
    match value {
        "confirmed" => Ok(AuditVerdict::Confirmed),
        "refuted" => Ok(AuditVerdict::Refuted),
        "unverifiable" => Ok(AuditVerdict::Unverifiable),
        _ => Err(ComparisonError::InvalidInput),
    }
}

fn credential_fields_are_safe(bytes: &[u8]) -> Result<bool, ComparisonError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ComparisonError::InvalidInput)?;
    let mut block_indent = None;
    for line in text.lines() {
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim_start();
        if let Some(required_indent) = block_indent {
            if indent > required_indent {
                if contains_credential(trimmed) {
                    return Ok(false);
                }
                continue;
            }
            block_indent = None;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let field = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        let Some((key, value)) = field.split_once(':') else {
            continue;
        };
        if !CREDENTIAL_FIELDS.contains(&key) {
            continue;
        }
        let value = value.trim();
        if matches!(value, "|" | "|-" | ">" | ">-") {
            block_indent = Some(indent);
        } else if contains_credential(value) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn contains_credential(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    CREDENTIAL_PATTERNS
        .iter()
        .any(|pattern| lower.contains(pattern))
}

fn valid_evidence_pointer(root: &Path, value: &str) -> bool {
    value == "inline"
        || (!value.is_empty()
            && !value.starts_with('/')
            && !value.starts_with("target/")
            && !value.contains('\\')
            && value
                .split('/')
                .all(|segment| !segment.is_empty() && !matches!(segment, "." | ".."))
            && confined_existing(root, value).is_ok())
}
