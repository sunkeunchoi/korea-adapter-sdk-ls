//! Validator that gates per-TR YAML against the routing index.
//!
//! The validator is the Rust-owned authority (ADR 0012). It enforces:
//!
//! 1. Every TR named in `tr-index.yaml` has a per-TR file present on disk.
//! 2. The index routing fields (`owner_class`, `protocol`, `instrument_domain`,
//!    `venue_session`) equal the per-TR file's values.
//! 3. Exactly one `owner_class` per TR — structurally enforced by the schema's
//!    single `owner_class` field; the parse step rejects unknown values.
//! 4. `owner_class` and all facet enum values are known/parseable (enforced at
//!    deserialize time by the closed-set enums in [`crate::schema`]).
//!
//! Every failure is a [`ValidationError`] that names the TR and, where
//! applicable, the field — errors are located, never anonymous.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::schema::{IndexEntry, TrIndex, TrMetadata};

/// The conventional index filename under a metadata root.
pub const INDEX_FILE_NAME: &str = "tr-index.yaml";

/// A located validation failure. Every variant carries enough context to point
/// a maintainer at the offending TR (and field, when the failure is field-level).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// The index file itself could not be read.
    IndexRead { path: PathBuf, message: String },
    /// The index file could not be parsed as `tr-index.yaml`.
    IndexParse { path: PathBuf, message: String },
    /// A per-TR file referenced by the index is missing on disk.
    MissingTrFile { tr_code: String, path: PathBuf },
    /// A per-TR file could not be read.
    TrFileRead {
        tr_code: String,
        path: PathBuf,
        message: String,
    },
    /// A per-TR file could not be parsed (covers unknown enum values, which
    /// serde rejects at deserialize time — e.g. an unknown `owner_class`).
    TrFileParse {
        tr_code: String,
        path: PathBuf,
        message: String,
    },
    /// The `tr_code` inside a per-TR file disagrees with its index key.
    TrCodeMismatch {
        index_key: String,
        file_tr_code: String,
        path: PathBuf,
    },
    /// An index routing field does not match the per-TR file's value.
    RoutingMismatch {
        tr_code: String,
        field: &'static str,
        index_value: String,
        file_value: String,
    },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationError::IndexRead { path, message } => {
                write!(f, "failed to read index {}: {message}", path.display())
            }
            ValidationError::IndexParse { path, message } => {
                write!(f, "failed to parse index {}: {message}", path.display())
            }
            ValidationError::MissingTrFile { tr_code, path } => {
                write!(
                    f,
                    "TR `{tr_code}`: per-TR file missing at {}",
                    path.display()
                )
            }
            ValidationError::TrFileRead {
                tr_code,
                path,
                message,
            } => write!(
                f,
                "TR `{tr_code}`: failed to read {}: {message}",
                path.display()
            ),
            ValidationError::TrFileParse {
                tr_code,
                path,
                message,
            } => write!(
                f,
                "TR `{tr_code}`: failed to parse {}: {message}",
                path.display()
            ),
            ValidationError::TrCodeMismatch {
                index_key,
                file_tr_code,
                path,
            } => write!(
                f,
                "TR `{index_key}`: per-TR file {} declares tr_code `{file_tr_code}` (index key disagrees)",
                path.display()
            ),
            ValidationError::RoutingMismatch {
                tr_code,
                field,
                index_value,
                file_value,
            } => write!(
                f,
                "TR `{tr_code}`: index field `{field}` is `{index_value}` but the per-TR file says `{file_value}`"
            ),
        }
    }
}

impl std::error::Error for ValidationError {}

/// A clean validation run: the parsed index plus every parsed per-TR record,
/// keyed by TR code. Returned so callers (e.g. a future `ls-core` dev-test) can
/// cross-check runtime constants against the validated metadata.
#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub index: TrIndex,
    pub trs: BTreeMap<String, TrMetadata>,
}

impl ValidationReport {
    /// Number of TRs validated.
    pub fn len(&self) -> usize {
        self.trs.len()
    }

    /// Whether the report contains no TRs.
    pub fn is_empty(&self) -> bool {
        self.trs.is_empty()
    }
}

/// Parse and validate a single per-TR YAML document already in memory.
///
/// `tr_code` is the expected code (e.g. the index key); a mismatch is reported.
/// `path` is used only for locating errors. Unknown enum values surface here as
/// [`ValidationError::TrFileParse`] because the closed-set enums reject them at
/// deserialize time.
pub fn parse_tr_metadata(
    tr_code: &str,
    path: &Path,
    yaml: &str,
) -> Result<TrMetadata, ValidationError> {
    let meta: TrMetadata =
        serde_yaml::from_str(yaml).map_err(|e| ValidationError::TrFileParse {
            tr_code: tr_code.to_string(),
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
    if meta.tr_code != tr_code {
        return Err(ValidationError::TrCodeMismatch {
            index_key: tr_code.to_string(),
            file_tr_code: meta.tr_code.clone(),
            path: path.to_path_buf(),
        });
    }
    Ok(meta)
}

/// Check one index entry's routing fields against a parsed per-TR record,
/// accumulating a located [`ValidationError::RoutingMismatch`] per disagreeing
/// field.
pub fn check_routing(
    tr_code: &str,
    entry: &IndexEntry,
    meta: &TrMetadata,
    errors: &mut Vec<ValidationError>,
) {
    let checks: [(&'static str, String, String); 4] = [
        (
            "owner_class",
            format!("{:?}", entry.owner_class),
            format!("{:?}", meta.owner_class),
        ),
        (
            "protocol",
            format!("{:?}", entry.protocol),
            format!("{:?}", meta.facets.protocol),
        ),
        (
            "instrument_domain",
            format!("{:?}", entry.instrument_domain),
            format!("{:?}", meta.facets.instrument_domain),
        ),
        (
            "venue_session",
            format!("{:?}", entry.venue_session),
            format!("{:?}", meta.facets.venue_session),
        ),
    ];
    for (field, index_value, file_value) in checks {
        if index_value != file_value {
            errors.push(ValidationError::RoutingMismatch {
                tr_code: tr_code.to_string(),
                field,
                index_value,
                file_value,
            });
        }
    }
}

/// Validate a metadata directory: load `tr-index.yaml`, then load and check
/// every per-TR file it references.
///
/// On success returns a [`ValidationReport`] carrying the parsed index and
/// per-TR records. On failure returns every located [`ValidationError`] found
/// (the validator does not stop at the first error, so a maintainer sees the
/// full picture). An index that cannot be read or parsed is a single fatal
/// error returned on its own.
pub fn validate_dir(metadata_root: &Path) -> Result<ValidationReport, Vec<ValidationError>> {
    let index_path = metadata_root.join(INDEX_FILE_NAME);

    let index_yaml = match std::fs::read_to_string(&index_path) {
        Ok(s) => s,
        Err(e) => {
            return Err(vec![ValidationError::IndexRead {
                path: index_path,
                message: e.to_string(),
            }])
        }
    };
    let index: TrIndex = match serde_yaml::from_str(&index_yaml) {
        Ok(i) => i,
        Err(e) => {
            return Err(vec![ValidationError::IndexParse {
                path: index_path,
                message: e.to_string(),
            }])
        }
    };

    let mut errors: Vec<ValidationError> = Vec::new();
    let mut trs: BTreeMap<String, TrMetadata> = BTreeMap::new();

    for (tr_code, entry) in &index.trs {
        // Per-TR `file` paths are recorded relative to the metadata root
        // (e.g. `trs/t8412.yaml`), matching the migration-plan example.
        let tr_path = metadata_root.join(&entry.file);

        let tr_yaml = match std::fs::read_to_string(&tr_path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                errors.push(ValidationError::MissingTrFile {
                    tr_code: tr_code.clone(),
                    path: tr_path,
                });
                continue;
            }
            Err(e) => {
                errors.push(ValidationError::TrFileRead {
                    tr_code: tr_code.clone(),
                    path: tr_path,
                    message: e.to_string(),
                });
                continue;
            }
        };

        match parse_tr_metadata(tr_code, &tr_path, &tr_yaml) {
            Ok(meta) => {
                check_routing(tr_code, entry, &meta, &mut errors);
                trs.insert(tr_code.clone(), meta);
            }
            Err(e) => errors.push(e),
        }
    }

    if errors.is_empty() {
        Ok(ValidationReport { index, trs })
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    const VALID_T8412: &str = r#"
tr_code: t8412
name: 주식차트(N분)
owner_class: paginated
facets:
  protocol: rest
  instrument_domain: stock
  venue_session: krx_regular
  date_sensitive: true
  self_paginated: true
  account_state: false
  paper_incompatible: false
  certification_path: automated
  rate_bucket: market_data
  caller_supplied_identifiers: [shcode]
dependencies:
  self_continuation_fields: [cts_date, cts_time]
  strong_order_fields: []
support:
  tracked: true
  implemented: true
  recommended: false
maintenance:
  source_spec_hash: 238beb842b1a
  last_reviewed: 2026-06-14
"#;

    const VALID_TOKEN: &str = r#"
tr_code: token
owner_class: standalone
facets:
  protocol: rest
  instrument_domain: misc
  venue_session: unspecified
  date_sensitive: false
  self_paginated: false
  account_state: false
  paper_incompatible: false
  certification_path: automated
  rate_bucket: auth
  caller_supplied_identifiers: []
dependencies:
  self_continuation_fields: []
  strong_order_fields: []
support:
  tracked: true
  implemented: true
  recommended: false
maintenance:
  source_spec_hash: aaaa1111bbbb
  last_reviewed: 2026-06-14
"#;

    /// Build a unique tempdir under the OS temp root (no external crate).
    fn temp_metadata_root() -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("ls-metadata-test-{pid}-{n}"));
        std::fs::create_dir_all(dir.join("trs")).expect("create trs dir");
        dir
    }

    fn write(root: &Path, rel: &str, contents: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, contents).expect("write fixture");
    }

    #[test]
    fn happy_path_valid_index_and_trs_validates_clean() {
        let root = temp_metadata_root();
        write(&root, "trs/t8412.yaml", VALID_T8412);
        write(&root, "trs/token.yaml", VALID_TOKEN);
        write(
            &root,
            INDEX_FILE_NAME,
            r#"
version: 1
trs:
  t8412:
    file: trs/t8412.yaml
    owner_class: paginated
    protocol: rest
    instrument_domain: stock
    venue_session: krx_regular
  token:
    file: trs/token.yaml
    owner_class: standalone
    protocol: rest
    instrument_domain: misc
    venue_session: unspecified
"#,
        );

        let report = validate_dir(&root).expect("valid metadata should pass");
        assert_eq!(report.len(), 2);
        assert!(report.trs.contains_key("t8412"));
        assert!(report.index.trs.contains_key("token"));
        // Parsed types are pub and accessible for a future cross-check.
        assert!(report.trs["t8412"].facets.self_paginated);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn routing_field_mismatch_fails_with_located_error() {
        let root = temp_metadata_root();
        write(&root, "trs/t8412.yaml", VALID_T8412);
        // Index claims `account` but the per-TR file says `paginated`.
        write(
            &root,
            INDEX_FILE_NAME,
            r#"
version: 1
trs:
  t8412:
    file: trs/t8412.yaml
    owner_class: account
    protocol: rest
    instrument_domain: stock
    venue_session: krx_regular
"#,
        );

        let errors = validate_dir(&root).expect_err("mismatch must fail");
        let located = errors
            .iter()
            .find(|e| matches!(e, ValidationError::RoutingMismatch { field, .. } if *field == "owner_class"))
            .expect("an owner_class routing mismatch");
        let msg = located.to_string();
        assert!(msg.contains("t8412"), "names the TR: {msg}");
        assert!(msg.contains("owner_class"), "names the field: {msg}");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_per_tr_file_fails_located() {
        let root = temp_metadata_root();
        // t8412 present, but `token` is indexed without a file on disk.
        write(&root, "trs/t8412.yaml", VALID_T8412);
        write(
            &root,
            INDEX_FILE_NAME,
            r#"
version: 1
trs:
  t8412:
    file: trs/t8412.yaml
    owner_class: paginated
    protocol: rest
    instrument_domain: stock
    venue_session: krx_regular
  token:
    file: trs/token.yaml
    owner_class: standalone
    protocol: rest
    instrument_domain: misc
    venue_session: unspecified
"#,
        );

        let errors = validate_dir(&root).expect_err("missing file must fail");
        let located = errors
            .iter()
            .find(|e| matches!(e, ValidationError::MissingTrFile { tr_code, .. } if tr_code == "token"))
            .expect("a missing-file error for token");
        assert!(located.to_string().contains("token"));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn unknown_owner_class_is_rejected() {
        let root = temp_metadata_root();
        let bad = VALID_T8412.replace("owner_class: paginated", "owner_class: not_a_class");
        write(&root, "trs/t8412.yaml", &bad);
        write(
            &root,
            INDEX_FILE_NAME,
            r#"
version: 1
trs:
  t8412:
    file: trs/t8412.yaml
    owner_class: paginated
    protocol: rest
    instrument_domain: stock
    venue_session: krx_regular
"#,
        );

        let errors = validate_dir(&root).expect_err("unknown owner_class must fail");
        assert!(errors.iter().any(|e| matches!(
            e,
            ValidationError::TrFileParse { tr_code, .. } if tr_code == "t8412"
        )));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn unknown_facet_enum_value_is_rejected() {
        let root = temp_metadata_root();
        let bad = VALID_T8412.replace("rate_bucket: market_data", "rate_bucket: futures_bucket");
        write(&root, "trs/t8412.yaml", &bad);
        write(
            &root,
            INDEX_FILE_NAME,
            r#"
version: 1
trs:
  t8412:
    file: trs/t8412.yaml
    owner_class: paginated
    protocol: rest
    instrument_domain: stock
    venue_session: krx_regular
"#,
        );

        let errors = validate_dir(&root).expect_err("unknown rate_bucket must fail");
        assert!(errors.iter().any(|e| matches!(
            e,
            ValidationError::TrFileParse { tr_code, .. } if tr_code == "t8412"
        )));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn single_file_parse_rejects_tr_code_mismatch() {
        let err = parse_tr_metadata("wrong_code", Path::new("trs/t8412.yaml"), VALID_T8412)
            .expect_err("tr_code mismatch must fail");
        assert!(matches!(err, ValidationError::TrCodeMismatch { .. }));
        assert!(err.to_string().contains("wrong_code"));
    }

    #[test]
    fn single_file_parse_happy_path() {
        let meta = parse_tr_metadata("t8412", Path::new("trs/t8412.yaml"), VALID_T8412)
            .expect("valid single file parses");
        assert_eq!(meta.tr_code, "t8412");
        assert!(meta.facets.date_sensitive);
        assert_eq!(
            meta.dependencies.self_continuation_fields,
            ["cts_date", "cts_time"]
        );
    }
}
