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

use crate::schema::{
    ConstraintSchema, ErrorCatalog, ErrorCoverage, EvidenceRecord, IndexEntry, TrIndex, TrMetadata,
};

/// The conventional index filename under a metadata root.
pub const INDEX_FILE_NAME: &str = "tr-index.yaml";

/// The shared error-explanation catalog filename under a metadata root.
pub const CATALOG_FILE_NAME: &str = "error-catalog.yaml";

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
    /// A TR is `recommended` but carries no `recommendation` contract block.
    RecommendationMissing { tr_code: String },
    /// A TR carries a `recommendation` block but is not `recommended`.
    RecommendationOnUnrecommended { tr_code: String },
    /// A recommended TR's `evidence_ref` does not resolve to a file on disk.
    EvidenceFileMissing { tr_code: String, path: PathBuf },
    /// A recommended TR's evidence record could not be parsed.
    EvidenceParse {
        tr_code: String,
        path: PathBuf,
        message: String,
    },
    /// A recommended TR's evidence `date` disagrees with its
    /// `maintenance.last_reviewed` — they must match until enforcement wires the
    /// link more richly (the convention guard, now a hard check).
    EvidenceDateMismatch {
        tr_code: String,
        last_reviewed: String,
        evidence_date: String,
    },
    /// A recommended TR's evidence record is missing its attested structural shape
    /// or the normalizer version it was captured under (R11). Change-driven staling
    /// cannot evaluate a TR with no frozen attested shape; this presence backstop
    /// catches "never captured" intra-metadata (the version-coupling check lives in
    /// the freshness path, which loads the manifest — KTD7). Clear it with
    /// `ls-trackers freshness re-pin <tr>`.
    AttestedShapeMissing { tr_code: String },
    /// A TR's `constraints_ref` does not resolve to a file on disk.
    ConstraintFileMissing { tr_code: String, path: PathBuf },
    /// A TR's constraint schema could not be parsed (covers a missing per-class
    /// N/A marker — the exhaustiveness audit, R5 — surfaced as a serde
    /// missing-field error, and unknown keys).
    ConstraintParse {
        tr_code: String,
        path: PathBuf,
        message: String,
    },
    /// A TR's `error_coverage_ref` does not resolve to a file on disk.
    ErrorCoverageFileMissing { tr_code: String, path: PathBuf },
    /// A TR's error-coverage artifact could not be parsed.
    ErrorCoverageParse {
        tr_code: String,
        path: PathBuf,
        message: String,
    },
    /// A `recommended` TR carries no `error_coverage_ref` — the error-resilience
    /// gate (R1) requires one before promotion. Distinct from a present-but-
    /// unparseable coverage file: this is the presence condition, independent of
    /// the coverage's input-class content (R2).
    RecommendedMissingErrorCoverage { tr_code: String },
    /// The shared error catalog (`error-catalog.yaml`) could not be read/parsed.
    CatalogRead { path: PathBuf, message: String },
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
            ValidationError::RecommendationMissing { tr_code } => write!(
                f,
                "TR `{tr_code}`: support.recommended is true but no `recommendation` contract block is present"
            ),
            ValidationError::RecommendationOnUnrecommended { tr_code } => write!(
                f,
                "TR `{tr_code}`: a `recommendation` block is present but support.recommended is false"
            ),
            ValidationError::EvidenceFileMissing { tr_code, path } => write!(
                f,
                "TR `{tr_code}`: recommendation.evidence_ref does not resolve — no file at {}",
                path.display()
            ),
            ValidationError::EvidenceParse {
                tr_code,
                path,
                message,
            } => write!(
                f,
                "TR `{tr_code}`: failed to parse evidence record {}: {message}",
                path.display()
            ),
            ValidationError::EvidenceDateMismatch {
                tr_code,
                last_reviewed,
                evidence_date,
            } => write!(
                f,
                "TR `{tr_code}`: maintenance.last_reviewed `{last_reviewed}` disagrees with evidence date `{evidence_date}`"
            ),
            ValidationError::AttestedShapeMissing { tr_code } => write!(
                f,
                "TR `{tr_code}`: recommended evidence record is missing `attested_shape` / `attested_normalizer_version` — re-pin with `ls-trackers freshness re-pin {tr_code}`"
            ),
            ValidationError::ConstraintFileMissing { tr_code, path } => write!(
                f,
                "TR `{tr_code}`: constraints_ref does not resolve — no file at {}",
                path.display()
            ),
            ValidationError::ConstraintParse {
                tr_code,
                path,
                message,
            } => write!(
                f,
                "TR `{tr_code}`: failed to parse constraint schema {}: {message}",
                path.display()
            ),
            ValidationError::ErrorCoverageFileMissing { tr_code, path } => write!(
                f,
                "TR `{tr_code}`: error_coverage_ref does not resolve — no file at {}",
                path.display()
            ),
            ValidationError::ErrorCoverageParse {
                tr_code,
                path,
                message,
            } => write!(
                f,
                "TR `{tr_code}`: failed to parse error-coverage artifact {}: {message}",
                path.display()
            ),
            ValidationError::RecommendedMissingErrorCoverage { tr_code } => write!(
                f,
                "TR `{tr_code}`: support.recommended is true but no `error_coverage_ref` is present — the error-resilience gate requires error coverage before promotion"
            ),
            ValidationError::CatalogRead { path, message } => write!(
                f,
                "failed to read/parse error catalog {}: {message}",
                path.display()
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
    /// Parsed Focused Evidence records for recommended TRs, keyed by TR code.
    /// Populated only for TRs that pass the recommendation/evidence checks, so a
    /// consumer (e.g. `ls-docgen`) can render the evidence environment level.
    pub evidence: BTreeMap<String, EvidenceRecord>,
    /// Parsed request field-constraint schemas, keyed by TR code — for any TR that
    /// declares a `constraints_ref` and parses cleanly (error-resilience gate).
    /// `ls-docgen` projects the preflight rules from these.
    pub constraints: BTreeMap<String, ConstraintSchema>,
    /// Parsed error-coverage artifacts, keyed by TR code — for any TR that declares
    /// an `error_coverage_ref` and parses cleanly. `ls-docgen` projects the
    /// reachable-code table from these.
    pub error_coverage: BTreeMap<String, ErrorCoverage>,
    /// The shared gateway error-explanation catalog (`error-catalog.yaml`), parsed
    /// once. `ls-docgen` projects each reachable code's explanation from it (R11).
    pub error_catalog: ErrorCatalog,
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
    fn check<T: PartialEq + std::fmt::Debug>(
        code: &str,
        field: &'static str,
        index: T,
        file: T,
        errors: &mut Vec<ValidationError>,
    ) {
        if index != file {
            errors.push(ValidationError::RoutingMismatch {
                tr_code: code.to_string(),
                field,
                index_value: format!("{index:?}"),
                file_value: format!("{file:?}"),
            });
        }
    }

    check(tr_code, "owner_class", entry.owner_class, meta.owner_class, errors);
    check(
        tr_code,
        "protocol",
        entry.protocol,
        meta.facets.protocol,
        errors,
    );
    check(
        tr_code,
        "instrument_domain",
        entry.instrument_domain,
        meta.facets.instrument_domain,
        errors,
    );
    check(
        tr_code,
        "venue_session",
        entry.venue_session,
        meta.facets.venue_session,
        errors,
    );
}

/// Check a parsed TR's recommendation contract and (when recommended) its linked
/// Focused Evidence record, accumulating located errors. On a clean recommended
/// TR the parsed evidence record is inserted into `evidence` so downstream
/// consumers can render its environment level.
///
/// Rules: a recommended TR must carry a `recommendation` block; a non-recommended
/// TR must not; a recommended TR's `evidence_ref` must resolve to a parseable file
/// whose `date` equals `maintenance.last_reviewed`.
pub fn check_recommendation(
    metadata_root: &Path,
    tr_code: &str,
    meta: &TrMetadata,
    evidence: &mut BTreeMap<String, EvidenceRecord>,
    errors: &mut Vec<ValidationError>,
) {
    match (meta.support.recommended, &meta.recommendation) {
        (true, None) => errors.push(ValidationError::RecommendationMissing {
            tr_code: tr_code.to_string(),
        }),
        (false, Some(_)) => errors.push(ValidationError::RecommendationOnUnrecommended {
            tr_code: tr_code.to_string(),
        }),
        (false, None) => {}
        (true, Some(rec)) => {
            let evidence_path = metadata_root.join(&rec.evidence_ref);
            let yaml = match std::fs::read_to_string(&evidence_path) {
                Ok(s) => s,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    errors.push(ValidationError::EvidenceFileMissing {
                        tr_code: tr_code.to_string(),
                        path: evidence_path,
                    });
                    return;
                }
                Err(e) => {
                    errors.push(ValidationError::EvidenceParse {
                        tr_code: tr_code.to_string(),
                        path: evidence_path,
                        message: e.to_string(),
                    });
                    return;
                }
            };
            let record: EvidenceRecord = match serde_yaml::from_str(&yaml) {
                Ok(r) => r,
                Err(e) => {
                    errors.push(ValidationError::EvidenceParse {
                        tr_code: tr_code.to_string(),
                        path: evidence_path,
                        message: e.to_string(),
                    });
                    return;
                }
            };
            if record.date != meta.maintenance.last_reviewed {
                errors.push(ValidationError::EvidenceDateMismatch {
                    tr_code: tr_code.to_string(),
                    last_reviewed: meta.maintenance.last_reviewed.clone(),
                    evidence_date: record.date.clone(),
                });
                return;
            }
            // Presence backstop (R11): a recommended TR's evidence must carry the
            // attested shape + the version it was captured under, or change-driven
            // staling has no frozen baseline to diff. Presence-only here; the
            // version-coupling check lives in the freshness path (KTD7).
            if record.attested_shape.is_none() || record.attested_normalizer_version.is_none() {
                errors.push(ValidationError::AttestedShapeMissing {
                    tr_code: tr_code.to_string(),
                });
                return;
            }
            evidence.insert(tr_code.to_string(), record);
        }
    }
}

/// Check a parsed TR's error-resilience artifacts (constraint schema + error
/// coverage) and enforce the promotion gate (R1). Accumulates located errors and,
/// on a clean parse, inserts the parsed artifacts into `constraints` /
/// `error_coverage` for downstream projection.
///
/// Rules, encoded as independent conditions so neither implies the other (R2):
/// - Any TR that declares a `constraints_ref` must resolve+parse it (the R5
///   exhaustiveness audit rides on the parse: a field missing an `enum`/`range`/
///   `format` class is a serde error).
/// - Any TR that declares an `error_coverage_ref` must resolve+parse it.
/// - A `recommended` TR must *declare* an `error_coverage_ref` (presence
///   condition, independent of the coverage's input-class content).
pub fn check_artifacts(
    metadata_root: &Path,
    tr_code: &str,
    meta: &TrMetadata,
    constraints: &mut BTreeMap<String, ConstraintSchema>,
    error_coverage: &mut BTreeMap<String, ErrorCoverage>,
    errors: &mut Vec<ValidationError>,
) {
    if let Some(ref rel) = meta.constraints_ref {
        let path = metadata_root.join(rel);
        match std::fs::read_to_string(&path) {
            Ok(yaml) => match serde_yaml::from_str::<ConstraintSchema>(&yaml) {
                Ok(schema) => {
                    constraints.insert(tr_code.to_string(), schema);
                }
                Err(e) => errors.push(ValidationError::ConstraintParse {
                    tr_code: tr_code.to_string(),
                    path,
                    message: e.to_string(),
                }),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                errors.push(ValidationError::ConstraintFileMissing {
                    tr_code: tr_code.to_string(),
                    path,
                });
            }
            Err(e) => errors.push(ValidationError::ConstraintParse {
                tr_code: tr_code.to_string(),
                path,
                message: e.to_string(),
            }),
        }
    }

    if let Some(ref rel) = meta.error_coverage_ref {
        let path = metadata_root.join(rel);
        match std::fs::read_to_string(&path) {
            Ok(yaml) => match serde_yaml::from_str::<ErrorCoverage>(&yaml) {
                Ok(coverage) => {
                    error_coverage.insert(tr_code.to_string(), coverage);
                }
                Err(e) => errors.push(ValidationError::ErrorCoverageParse {
                    tr_code: tr_code.to_string(),
                    path,
                    message: e.to_string(),
                }),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                errors.push(ValidationError::ErrorCoverageFileMissing {
                    tr_code: tr_code.to_string(),
                    path,
                });
            }
            Err(e) => errors.push(ValidationError::ErrorCoverageParse {
                tr_code: tr_code.to_string(),
                path,
                message: e.to_string(),
            }),
        }
    }

    // Promotion gate (R1): a recommended TR must declare error coverage.
    if meta.support.recommended && meta.error_coverage_ref.is_none() {
        errors.push(ValidationError::RecommendedMissingErrorCoverage {
            tr_code: tr_code.to_string(),
        });
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
    let mut evidence: BTreeMap<String, EvidenceRecord> = BTreeMap::new();
    let mut constraints: BTreeMap<String, ConstraintSchema> = BTreeMap::new();
    let mut error_coverage: BTreeMap<String, ErrorCoverage> = BTreeMap::new();

    // The shared error catalog is a single committed data file (KTD2). Parse it
    // once for the docgen projection. An ABSENT catalog is treated as empty (the
    // catalog is a projection input, not a per-TR validation gate — its existence
    // and coverage are enforced by `ls-core`'s build.rs + tests); a PRESENT but
    // malformed catalog is a located error, so drift in the committed file is
    // caught without forcing every temp-root validator test to author one.
    let catalog_path = metadata_root.join(CATALOG_FILE_NAME);
    let empty_catalog = ErrorCatalog {
        version: 0,
        codes: BTreeMap::new(),
    };
    let error_catalog = match std::fs::read_to_string(&catalog_path) {
        Ok(s) => match serde_yaml::from_str::<ErrorCatalog>(&s) {
            Ok(c) => c,
            Err(e) => {
                errors.push(ValidationError::CatalogRead {
                    path: catalog_path,
                    message: e.to_string(),
                });
                empty_catalog
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => empty_catalog,
        Err(e) => {
            errors.push(ValidationError::CatalogRead {
                path: catalog_path,
                message: e.to_string(),
            });
            empty_catalog
        }
    };

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
                check_recommendation(metadata_root, tr_code, &meta, &mut evidence, &mut errors);
                check_artifacts(
                    metadata_root,
                    tr_code,
                    &meta,
                    &mut constraints,
                    &mut error_coverage,
                    &mut errors,
                );
                trs.insert(tr_code.clone(), meta);
            }
            Err(e) => errors.push(e),
        }
    }

    if errors.is_empty() {
        Ok(ValidationReport {
            index,
            trs,
            evidence,
            constraints,
            error_coverage,
            error_catalog,
        })
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

    /// A recommended `token` per-TR file carrying a full `recommendation` block.
    const RECOMMENDED_TOKEN: &str = r#"
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
support:
  tracked: true
  implemented: true
  recommended: true
maintenance:
  source_spec_hash: aaaa1111bbbb
  last_reviewed: 2026-06-16
recommendation:
  behavior: Paper OAuth access-token issuance
  evidence_ref: evidence/token.yaml
  excludes:
    - Production-credential token issuance
"#;

    const TOKEN_EVIDENCE: &str = r#"
tr_code: token
date: 2026-06-16
env: paper
target: live-smoke
line: "LIVE-SMOKE target=live-smoke result=[token_len=380 rsp_cd=00000]"
attested_normalizer_version: 2
attested_shape:
  tr_code: token
  protocol: rest
  is_websocket: false
"#;

    fn meta(tr_code: &str, yaml: &str) -> TrMetadata {
        parse_tr_metadata(tr_code, Path::new("inline"), yaml).expect("fixture parses")
    }

    #[test]
    fn recommended_tr_with_matching_evidence_validates_and_records_env() {
        let root = temp_metadata_root();
        write(&root, "evidence/token.yaml", TOKEN_EVIDENCE);
        let m = meta("token", RECOMMENDED_TOKEN);

        let mut evidence = BTreeMap::new();
        let mut errors = Vec::new();
        check_recommendation(&root, "token", &m, &mut evidence, &mut errors);

        assert!(errors.is_empty(), "clean recommended TR: {errors:?}");
        assert_eq!(evidence["token"].env, "paper", "evidence env surfaced");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn recommended_without_block_is_located_error() {
        let root = temp_metadata_root();
        // VALID_TOKEN is recommended:false; flip just that flag, no block added.
        let recommended_no_block = VALID_TOKEN.replace("recommended: false", "recommended: true");
        let m = meta("token", &recommended_no_block);

        let mut errors = Vec::new();
        check_recommendation(&root, "token", &m, &mut BTreeMap::new(), &mut errors);

        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::RecommendationMissing { tr_code } if tr_code == "token")));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn recommendation_block_on_unrecommended_tr_is_error() {
        let root = temp_metadata_root();
        // RECOMMENDED_TOKEN has a block; flip recommended back to false.
        let block_but_unrec = RECOMMENDED_TOKEN.replace("recommended: true", "recommended: false");
        let m = meta("token", &block_but_unrec);

        let mut errors = Vec::new();
        check_recommendation(&root, "token", &m, &mut BTreeMap::new(), &mut errors);

        assert!(errors.iter().any(|e| matches!(
            e,
            ValidationError::RecommendationOnUnrecommended { tr_code } if tr_code == "token"
        )));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn recommended_with_missing_evidence_file_is_located_error() {
        let root = temp_metadata_root(); // no evidence/ file written
        let m = meta("token", RECOMMENDED_TOKEN);

        let mut errors = Vec::new();
        check_recommendation(&root, "token", &m, &mut BTreeMap::new(), &mut errors);

        let located = errors
            .iter()
            .find(|e| matches!(e, ValidationError::EvidenceFileMissing { tr_code, .. } if tr_code == "token"))
            .expect("missing-evidence error");
        assert!(located.to_string().contains("evidence/token.yaml"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn evidence_date_mismatch_is_located_error() {
        let root = temp_metadata_root();
        // Evidence dated a day off from last_reviewed (2026-06-16).
        let stale = TOKEN_EVIDENCE.replace("date: 2026-06-16", "date: 2026-06-15");
        write(&root, "evidence/token.yaml", &stale);
        let m = meta("token", RECOMMENDED_TOKEN);

        let mut evidence = BTreeMap::new();
        let mut errors = Vec::new();
        check_recommendation(&root, "token", &m, &mut evidence, &mut errors);

        assert!(errors.iter().any(|e| matches!(
            e,
            ValidationError::EvidenceDateMismatch { tr_code, .. } if tr_code == "token"
        )));
        assert!(evidence.is_empty(), "no env recorded on a mismatch");
        std::fs::remove_dir_all(&root).ok();
    }

    /// U7: a recommended TR whose evidence record lacks `attested_shape` fails with
    /// a located `AttestedShapeMissing` and is not recorded in the evidence map.
    #[test]
    fn recommended_without_attested_shape_is_located_error() {
        let root = temp_metadata_root();
        // Date matches last_reviewed (2026-06-16) but no attested_shape fields.
        let no_attested = "\
tr_code: token
date: 2026-06-16
env: paper
";
        write(&root, "evidence/token.yaml", no_attested);
        let m = meta("token", RECOMMENDED_TOKEN);

        let mut evidence = BTreeMap::new();
        let mut errors = Vec::new();
        check_recommendation(&root, "token", &m, &mut evidence, &mut errors);

        let located = errors
            .iter()
            .find(|e| matches!(e, ValidationError::AttestedShapeMissing { tr_code } if tr_code == "token"))
            .expect("attested-shape-missing error");
        assert!(located.to_string().contains("re-pin"));
        assert!(evidence.is_empty(), "no env recorded when attested shape missing");
        std::fs::remove_dir_all(&root).ok();
    }

    /// U7: a recommended TR carrying `attested_shape` but no
    /// `attested_normalizer_version` is also an error (both fields are required).
    #[test]
    fn recommended_without_attested_version_is_located_error() {
        let root = temp_metadata_root();
        let shape_no_version = "\
tr_code: token
date: 2026-06-16
env: paper
attested_shape:
  tr_code: token
  protocol: rest
  is_websocket: false
";
        write(&root, "evidence/token.yaml", shape_no_version);
        let m = meta("token", RECOMMENDED_TOKEN);

        let mut errors = Vec::new();
        check_recommendation(&root, "token", &m, &mut BTreeMap::new(), &mut errors);
        assert!(errors.iter().any(|e| matches!(
            e,
            ValidationError::AttestedShapeMissing { tr_code } if tr_code == "token"
        )));
        std::fs::remove_dir_all(&root).ok();
    }

    // --- error-resilience gate artifacts (U2/U6) ---------------------------

    /// A minimal implemented TR YAML, optionally recommended, with optional
    /// constraint/coverage refs — for the artifact checks.
    fn tr_yaml(recommended: bool, constraints_ref: Option<&str>, coverage_ref: Option<&str>) -> String {
        let mut y = format!(
            "\
tr_code: t8412
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
support:
  tracked: true
  implemented: true
  recommended: {recommended}
maintenance:
  source_spec_hash: deadbeef
  last_reviewed: 2026-06-22
"
        );
        if let Some(c) = constraints_ref {
            y.push_str(&format!("constraints_ref: {c}\n"));
        }
        if let Some(c) = coverage_ref {
            y.push_str(&format!("error_coverage_ref: {c}\n"));
        }
        y
    }

    const EXEMPLAR_CONSTRAINTS: &str = r#"
tr_code: t8412
fields:
  - name: shcode
    type: string
    required: true
    enum: { applicable: false }
    range: { applicable: false }
    format: { applicable: true, kind: symbol, confirmed: false }
"#;

    const EXEMPLAR_COVERAGE: &str = r#"
tr_code: t8412
probe_status: not_probed
input_classes: []
gateway_codes: ["01900"]
"#;

    #[test]
    fn recommended_without_error_coverage_is_located_error() {
        // U6: a recommended TR with no error_coverage_ref fails the promotion gate.
        let root = temp_metadata_root();
        let m = meta("t8412", &tr_yaml(true, None, None));
        let mut errors = Vec::new();
        check_artifacts(
            &root,
            "t8412",
            &m,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut errors,
        );
        assert!(errors.iter().any(|e| matches!(
            e,
            ValidationError::RecommendedMissingErrorCoverage { tr_code } if tr_code == "t8412"
        )));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn recommended_with_empty_input_class_coverage_still_validates() {
        // U6/R2 independence: coverage present (even with an empty input-class set)
        // satisfies the presence gate — the two conditions don't collapse.
        let root = temp_metadata_root();
        write(&root, "error-coverage/t8412.yaml", EXEMPLAR_COVERAGE);
        let m = meta("t8412", &tr_yaml(true, None, Some("error-coverage/t8412.yaml")));
        let mut coverage = BTreeMap::new();
        let mut errors = Vec::new();
        check_artifacts(&root, "t8412", &m, &mut BTreeMap::new(), &mut coverage, &mut errors);
        assert!(errors.is_empty(), "coverage present is enough: {errors:?}");
        assert!(coverage.contains_key("t8412"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn implemented_non_recommended_without_coverage_validates_clean() {
        // U6: an implemented, non-recommended TR needs no coverage.
        let root = temp_metadata_root();
        let m = meta("t8412", &tr_yaml(false, None, None));
        let mut errors = Vec::new();
        check_artifacts(
            &root,
            "t8412",
            &m,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut errors,
        );
        assert!(errors.is_empty(), "no coverage required: {errors:?}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn constraints_ref_missing_file_is_located_error() {
        let root = temp_metadata_root();
        let m = meta("t8412", &tr_yaml(false, Some("constraints/t8412.yaml"), None));
        let mut errors = Vec::new();
        check_artifacts(
            &root,
            "t8412",
            &m,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut errors,
        );
        assert!(errors.iter().any(|e| matches!(
            e,
            ValidationError::ConstraintFileMissing { tr_code, .. } if tr_code == "t8412"
        )));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn constraint_schema_missing_class_marker_is_parse_error() {
        // R5 exhaustiveness: a field omitting the `range` class marker is a parse
        // error (the non-optional class field is missing) — not a silent gap.
        let root = temp_metadata_root();
        let bad = r#"
tr_code: t8412
fields:
  - name: shcode
    type: string
    required: true
    enum: { applicable: false }
    format: { applicable: false }
"#;
        write(&root, "constraints/t8412.yaml", bad);
        let m = meta("t8412", &tr_yaml(false, Some("constraints/t8412.yaml"), None));
        let mut errors = Vec::new();
        check_artifacts(
            &root,
            "t8412",
            &m,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut errors,
        );
        assert!(errors.iter().any(|e| matches!(
            e,
            ValidationError::ConstraintParse { tr_code, .. } if tr_code == "t8412"
        )));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn well_formed_constraints_parse_and_are_recorded() {
        let root = temp_metadata_root();
        write(&root, "constraints/t8412.yaml", EXEMPLAR_CONSTRAINTS);
        let m = meta("t8412", &tr_yaml(false, Some("constraints/t8412.yaml"), None));
        let mut constraints = BTreeMap::new();
        let mut errors = Vec::new();
        check_artifacts(&root, "t8412", &m, &mut constraints, &mut BTreeMap::new(), &mut errors);
        assert!(errors.is_empty(), "clean schema: {errors:?}");
        assert_eq!(constraints["t8412"].fields.len(), 1);
        std::fs::remove_dir_all(&root).ok();
    }

    /// U7: a fully-populated recommended TR (attested shape + version present, post
    /// backfill) validates clean and records its env.
    #[test]
    fn recommended_with_attested_shape_validates_clean() {
        let root = temp_metadata_root();
        write(&root, "evidence/token.yaml", TOKEN_EVIDENCE);
        let m = meta("token", RECOMMENDED_TOKEN);

        let mut evidence = BTreeMap::new();
        let mut errors = Vec::new();
        check_recommendation(&root, "token", &m, &mut evidence, &mut errors);
        assert!(errors.is_empty(), "fully-attested recommended TR: {errors:?}");
        assert_eq!(evidence["token"].attested_normalizer_version, Some(2));
        std::fs::remove_dir_all(&root).ok();
    }
}
