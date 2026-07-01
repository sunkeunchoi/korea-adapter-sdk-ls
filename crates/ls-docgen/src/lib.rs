//! `ls-docgen` — deterministic documentation generated from `ls-metadata`.
//!
//! Metadata is the single source of truth. These docs are a *projection* of the
//! validated `ls-metadata` records, never a mirror of upstream LS docs or of
//! tracker output: the generator calls [`ls_metadata::validate_dir`] and renders
//! markdown directly from the parsed [`TrMetadata`] / [`TrIndex`] types. It emits
//! no wall-clock or run timestamp, and renders stored `last_reviewed` /
//! `source_spec_hash` fields verbatim, so identical metadata yields byte-identical
//! output across runs and platforms (R5). A `--check` mode (R6) compares the
//! rendered set against the committed files and fails, naming any drift, so the
//! committed docs cannot silently fall out of sync with metadata.
//!
//! Library-first split (mirroring `ls_metadata::planner`): the low-level
//! `render_*` functions take a `&BTreeMap<String, TrMetadata>` (and the index)
//! so tests drive them from inline fixtures; [`render_all`] takes a validated
//! [`ValidationReport`]. `main.rs` is a thin CLI shell over [`run_cli`].

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use ls_metadata::{
    validate_dir, CertificationPath, ConstraintSchema, CrossFieldRule, ErrorCatalog, ErrorCoverage,
    EvidenceRecord, FieldConstraint, FieldType, FormatKind, InstrumentDomain, OwnerClass, Protocol,
    RateBucket, Support, TrIndex, TrMetadata, ValidationError, ValidationReport, VenueSession,
};

/// Generated TR Dependency Docs live here, relative to the repo root.
pub const DEPENDENCY_DOCS_DIR: &str = "docs/tr-dependencies";
/// Generated SDK Reference Docs live here, relative to the repo root.
pub const REFERENCE_DOCS_DIR: &str = "docs/reference";

/// CLI mode: write the docs (default) or check committed docs against metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Render and write the docs to disk (default, no flag).
    Write,
    /// Render in memory and compare against committed files; drift is an error.
    Check,
}

/// A located docgen failure. Every variant carries enough context to point a
/// maintainer at the cause (mirrors the `ls-metadata` located-error convention).
#[derive(Debug)]
pub enum DocgenError {
    /// An unrecognized CLI argument was passed.
    UnknownArg(String),
    /// The metadata directory failed to validate; carries the located errors.
    MetadataInvalid(Vec<ValidationError>),
    /// A filesystem read/write failed for a specific path.
    Io { path: PathBuf, message: String },
    /// `--check` found committed docs that no longer match metadata. Carries the
    /// drifted paths (repo-relative), each named in the message.
    Drift(Vec<PathBuf>),
}

impl fmt::Display for DocgenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocgenError::UnknownArg(arg) => {
                write!(
                    f,
                    "unrecognized argument `{arg}` (expected no flag, or `--check`)"
                )
            }
            DocgenError::MetadataInvalid(errors) => {
                writeln!(
                    f,
                    "metadata failed to validate ({} error(s)):",
                    errors.len()
                )?;
                for e in errors {
                    writeln!(f, "  - {e}")?;
                }
                Ok(())
            }
            DocgenError::Io { path, message } => {
                write!(f, "I/O error at {}: {message}", path.display())
            }
            DocgenError::Drift(paths) => {
                writeln!(
                    f,
                    "docs drift: {} file(s) differ from current metadata (run `make docs` to regenerate):",
                    paths.len()
                )?;
                for p in paths {
                    writeln!(f, "  - {}", p.display())?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for DocgenError {}

/// Parse CLI args (already past the binary name) into a [`Mode`].
///
/// No args → [`Mode::Write`]; `--check` → [`Mode::Check`]; anything else is a
/// located [`DocgenError::UnknownArg`].
pub fn parse_mode<I, S>(args: I) -> Result<Mode, DocgenError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut mode = Mode::Write;
    for arg in args {
        match arg.as_ref() {
            "--check" => mode = Mode::Check,
            other => return Err(DocgenError::UnknownArg(other.to_string())),
        }
    }
    Ok(mode)
}

/// The repository root, resolved from this crate's manifest dir at compile time
/// (`crates/ls-docgen` → repo). Mirrors the `policy_index_crosscheck` precedent
/// of anchoring to `CARGO_MANIFEST_DIR` rather than the process cwd.
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// The authored metadata root (`<repo>/metadata`).
pub fn metadata_root() -> PathBuf {
    repo_root().join("metadata")
}

/// Shared do-not-edit banner placed at the top of every generated file.
const GENERATED_BANNER: &str =
    "> Generated from `ls-metadata` — do not edit by hand. Run `make docs` to regenerate.";

/// Canonical snake_case form of each closed-set enum, matching the YAML the
/// metadata is authored in. The authoritative vocabulary lives on the enums via
/// `#[serde(rename_all = "snake_case")]`; these helpers deliberately mirror it so
/// rendering stays a pure, dependency-free string build (the plan's "markdown by
/// hand" decision) rather than a serde round-trip. The exhaustive `match` makes a
/// newly *added* variant a compile error; a *renamed* serde value would need
/// these kept in step by hand. If a third consumer appears, lift `as_str` onto
/// the enums in `ls-metadata` instead.
fn owner_class_str(c: OwnerClass) -> &'static str {
    match c {
        OwnerClass::Standalone => "standalone",
        OwnerClass::MarketSession => "market_session",
        OwnerClass::Paginated => "paginated",
        OwnerClass::Account => "account",
        OwnerClass::Orders => "orders",
        OwnerClass::Realtime => "realtime",
        OwnerClass::PaperIncompatible => "paper_incompatible",
    }
}

fn protocol_str(p: Protocol) -> &'static str {
    match p {
        Protocol::Rest => "rest",
        Protocol::Websocket => "websocket",
    }
}

fn instrument_domain_str(d: InstrumentDomain) -> &'static str {
    match d {
        InstrumentDomain::Stock => "stock",
        InstrumentDomain::FuturesOptions => "futures_options",
        InstrumentDomain::OverseasStock => "overseas_stock",
        InstrumentDomain::OverseasFutures => "overseas_futures",
        InstrumentDomain::SectorIndex => "sector_index",
        InstrumentDomain::RealtimeInvest => "realtime_invest",
        InstrumentDomain::Misc => "misc",
    }
}

fn venue_session_str(v: VenueSession) -> &'static str {
    match v {
        VenueSession::KrxRegular => "krx_regular",
        VenueSession::KrxExtended => "krx_extended",
        VenueSession::Unspecified => "unspecified",
    }
}

fn certification_path_str(c: CertificationPath) -> &'static str {
    match c {
        CertificationPath::Automated => "automated",
        CertificationPath::Manual => "manual",
        CertificationPath::None => "none",
    }
}

fn rate_bucket_str(b: RateBucket) -> &'static str {
    match b {
        RateBucket::MarketData => "market_data",
        RateBucket::Orders => "orders",
        RateBucket::Account => "account",
        RateBucket::Auth => "auth",
    }
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

/// Render a list of field names as backtick-quoted, comma-joined, or `none` when
/// empty — so an empty dependency list renders as a clear `none` rather than a
/// dangling, empty section.
fn field_list(fields: &[String]) -> String {
    if fields.is_empty() {
        "none".to_string()
    } else {
        fields
            .iter()
            .map(|f| format!("`{f}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The highest support tier a TR has reached, as a single compact label for the
/// index routing table (recommended ⊃ implemented ⊃ tracked).
fn support_label(support: &Support) -> &'static str {
    if support.recommended {
        "recommended"
    } else if support.implemented {
        "implemented"
    } else if support.tracked {
        "tracked"
    } else {
        "untracked"
    }
}

/// Render a single TR's Dependency Doc page.
fn render_dependency_page(meta: &TrMetadata) -> String {
    let facets = &meta.facets;
    let mut out = String::new();

    out.push_str(&format!("# TR Dependency: {}\n\n", meta.tr_code));
    if let Some(name) = &meta.name {
        out.push_str(&format!("{name}\n\n"));
    }
    out.push_str(GENERATED_BANNER);
    out.push_str("\n\n");

    out.push_str("## Support\n\n");
    out.push_str(&format!("- Tracked: {}\n", yes_no(meta.support.tracked)));
    out.push_str(&format!(
        "- Implemented: {}\n",
        yes_no(meta.support.implemented)
    ));
    out.push_str(&format!(
        "- Recommended: {}\n\n",
        yes_no(meta.support.recommended)
    ));

    out.push_str("## Ownership\n\n");
    out.push_str(&format!(
        "- Owner class: `{}`\n\n",
        owner_class_str(meta.owner_class)
    ));

    out.push_str("## Facets\n\n");
    out.push_str(&format!(
        "- Protocol: `{}`\n",
        protocol_str(facets.protocol)
    ));
    out.push_str(&format!(
        "- Instrument domain: `{}`\n",
        instrument_domain_str(facets.instrument_domain)
    ));
    out.push_str(&format!(
        "- Venue / session: `{}`\n",
        venue_session_str(facets.venue_session)
    ));
    out.push_str(&format!(
        "- Date sensitive: {}\n",
        yes_no(facets.date_sensitive)
    ));
    out.push_str(&format!(
        "- Self-paginated: {}\n",
        yes_no(facets.self_paginated)
    ));
    out.push_str(&format!(
        "- Account state: {}\n",
        yes_no(facets.account_state)
    ));
    out.push_str(&format!(
        "- Paper incompatible: {}\n",
        yes_no(facets.paper_incompatible)
    ));
    out.push_str(&format!(
        "- Certification path: `{}`\n",
        certification_path_str(facets.certification_path)
    ));
    out.push_str(&format!(
        "- Rate bucket: `{}`\n",
        rate_bucket_str(facets.rate_bucket)
    ));
    out.push_str(&format!(
        "- Caller-supplied identifiers: {}\n\n",
        field_list(&facets.caller_supplied_identifiers)
    ));

    out.push_str("## Dependencies\n\n");
    out.push_str(&format!(
        "- Self-continuation fields: {}\n",
        field_list(&meta.dependencies.self_continuation_fields)
    ));
    out.push_str(&format!(
        "- Strong-order fields: {}\n\n",
        field_list(&meta.dependencies.strong_order_fields)
    ));

    out.push_str("## Maintenance\n\n");
    out.push_str(&format!(
        "- Source spec hash: `{}`\n",
        meta.maintenance.source_spec_hash
    ));
    out.push_str(&format!(
        "- Last reviewed: `{}`\n",
        meta.maintenance.last_reviewed
    ));

    out
}

/// Render the Dependency Docs index page: a routing table over every tracked TR.
fn render_dependency_index(trs: &BTreeMap<String, TrMetadata>) -> String {
    let mut out = String::new();
    out.push_str("# TR Dependency Docs\n\n");
    out.push_str(GENERATED_BANNER);
    out.push_str("\n\n");
    out.push_str(
        "Maintainer- and operator-facing projection of TR maintenance metadata: owner class, \
         support state, prerequisite coupling, and venue/session constraints for every tracked TR.\n\n",
    );
    out.push_str("| TR | Name | Owner class | Support | Page |\n");
    out.push_str("|----|------|-------------|---------|------|\n");
    for (tr_code, meta) in trs {
        let name = meta.name.as_deref().unwrap_or("");
        out.push_str(&format!(
            "| `{tr_code}` | {name} | `{}` | {} | [{tr_code}](./{tr_code}.md) |\n",
            owner_class_str(meta.owner_class),
            support_label(&meta.support),
        ));
    }
    out
}

/// Low-level: render the TR Dependency Docs file set (index + per-TR pages),
/// keyed by repo-relative path. Takes the raw metadata map (and index) so tests
/// drive it from inline fixtures without touching disk.
///
/// The index defines the canonical TR set and page ordering (its `BTreeMap` is
/// sorted for free); per-page content is projected from the matching metadata
/// record. A TR present in the index but absent from `trs` is skipped — the
/// validator already rejects that case before this runs.
pub fn render_dependency_docs(
    trs: &BTreeMap<String, TrMetadata>,
    index: &TrIndex,
) -> BTreeMap<PathBuf, String> {
    let dir = Path::new(DEPENDENCY_DOCS_DIR);
    let mut files = BTreeMap::new();

    files.insert(dir.join("index.md"), render_dependency_index(trs));

    for tr_code in index.trs.keys() {
        if let Some(meta) = trs.get(tr_code) {
            files.insert(
                dir.join(format!("{tr_code}.md")),
                render_dependency_page(meta),
            );
        }
    }

    files
}

/// The status banner an implemented-but-not-recommended TR carries (R4). Keyed
/// purely on `support.recommended == false`, so the day a TR is promoted the
/// banner drops automatically.
const NOT_RECOMMENDED_BANNER: &str =
    "> ⚠️ **Implemented, not yet recommended.** This TR is wired and tested but has not been \
     promoted to recommended status; its surface and guidance may still change.";

/// The stable revocation-policy text rendered into every Recommended TR's
/// contract. **Per-clause candor** (R10), mirroring `metadata/EVIDENCE-FRESHNESS.md`:
/// both the 90-day backstop and change-driven staling *detection* are enforced by
/// the freshness evaluator; only the *auto-revoke* arm (flipping
/// `support.recommended`) stays deferred. Keep each clause's enforcement status
/// accurate.
const REVOCATION_POLICY: &str =
    "What would revoke this claim: the **90-day backstop is enforced** — `make freshness-check` \
     flags this TR's Focused Evidence as stale once 90 days elapse from the freshness date (the \
     review-by date above), and the recommendation must then be re-attested. **Change-driven \
     staling is also enforced** — a qualifying Structural API Shape change (field add/remove/change \
     or endpoint/protocol change) diverging from the attested shape stales the evidence (advisory, \
     surfaced by the same check); only *auto-revoke* of the recommendation is deferred (a human \
     re-attests or demotes). Description / `korean_name` / rate-limit / reorder changes are \
     non-qualifying and do not stale it. See `metadata/EVIDENCE-FRESHNESS.md`.";

/// Render the user-facing recommendation contract for a Recommended TR (R9): the
/// recommended behavior, the backing evidence and its environment level, the
/// freshness date, the revocation policy, and what the claim does *not* cover.
/// `evidence` is the parsed Focused Evidence record surfaced by the validator;
/// `None` only when a caller drives rendering from a map without the record (the
/// real `render_all` path always supplies it for a recommended TR).
fn render_recommendation(meta: &TrMetadata, evidence: Option<&EvidenceRecord>) -> String {
    let Some(rec) = &meta.recommendation else {
        // Defensive: a recommended TR without a block cannot reach the validated
        // report, but keep rendering total rather than panicking.
        return String::new();
    };
    let env = evidence.map(|e| e.env.as_str()).unwrap_or("unspecified");

    let mut out = String::new();
    out.push_str("## Recommendation\n\n");
    out.push_str(&format!("**Recommended behavior:** {}\n\n", rec.behavior));
    out.push_str(&format!(
        "- Evidence: `{}` (environment: `{}`)\n",
        rec.evidence_ref, env
    ));
    out.push_str(&format!(
        "- Freshness date: `{}` (`maintenance.last_reviewed`)\n",
        meta.maintenance.last_reviewed
    ));
    // Deterministic review-by date: freshness date + the 90-day backstop. A pure
    // derivation of `last_reviewed` (no clock), so committed docs stay
    // byte-identical across runs. The live stale verdict is the freshness
    // evaluator's job (`make freshness-check`), not the committed page. Skipped
    // only if `last_reviewed` is unparseable — unreachable for the authored ISO
    // dates, but a malformed date degrades to omitting the line, not a panic.
    if let Ok(review_by) = ls_metadata::review_by(
        &meta.maintenance.last_reviewed,
        ls_metadata::DEFAULT_WINDOW_DAYS,
    ) {
        out.push_str(&format!(
            "- Review by: `{}` (freshness date + {}-day backstop)\n",
            review_by.format("%Y-%m-%d"),
            ls_metadata::DEFAULT_WINDOW_DAYS
        ));
    }
    out.push_str(&format!("- {REVOCATION_POLICY}\n\n"));

    out.push_str("This recommendation does not claim:\n\n");
    if rec.excludes.is_empty() {
        out.push_str("- (no explicit exclusions recorded)\n");
    } else {
        for exclude in &rec.excludes {
            out.push_str(&format!("- {exclude}\n"));
        }
    }
    out
}

/// Describe one field-type for the preflight rules list.
fn field_type_str(t: FieldType) -> &'static str {
    match t {
        FieldType::String => "string",
        FieldType::Integer => "integer",
        FieldType::Number => "number",
    }
}

/// Render one field's preflight rules line from its declared constraints. Type +
/// required always enforce; enum/range/format note "permissive until confirmed"
/// when their bound is unconfirmed (R6), so users understand a declared-but-
/// unconfirmed bound does not (yet) reject locally.
fn render_field_rule(field: &FieldConstraint) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!("type `{}`", field_type_str(field.field_type)));
    parts.push(if field.required {
        "required".to_string()
    } else {
        "optional".to_string()
    });
    if field.enum_rule.applicable {
        let suffix = if field.enum_rule.confirmed {
            "enforced"
        } else {
            "permissive until confirmed"
        };
        parts.push(format!(
            "one of [{}] ({suffix})",
            field.enum_rule.values.join(", ")
        ));
    }
    if field.range.applicable {
        let suffix = if field.range.confirmed {
            "enforced"
        } else {
            "permissive until confirmed"
        };
        let bound = match (field.range.min, field.range.max) {
            (Some(min), Some(max)) => format!("{min}..={max}"),
            (Some(min), None) => format!(">= {min}"),
            (None, Some(max)) => format!("<= {max}"),
            (None, None) => "range".to_string(),
        };
        parts.push(format!("{bound} ({suffix})"));
    }
    if field.format.applicable {
        let suffix = if field.format.confirmed {
            "enforced"
        } else {
            "permissive until confirmed"
        };
        let kind = match field.format.kind {
            Some(FormatKind::Symbol) => "alphanumeric symbol",
            Some(FormatKind::Date) => "YYYYMMDD date",
            None => "format",
        };
        parts.push(format!("{kind} ({suffix})"));
    }
    format!("- `{}` — {}\n", field.name, parts.join("; "))
}

/// Render the per-TR "Errors & validation" section (R11) from the constraint
/// schema, the error-coverage evidence, and the shared catalog. Deterministic
/// (no clock, stable ordering). Returns an empty string when the TR has neither a
/// constraint schema nor error coverage, so pages without artifacts are
/// byte-unchanged.
fn render_errors_and_validation(
    constraint: Option<&ConstraintSchema>,
    coverage: Option<&ErrorCoverage>,
    catalog: &ErrorCatalog,
) -> String {
    if constraint.is_none() && coverage.is_none() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("\n## Errors & validation\n\n");

    if let Some(schema) = constraint {
        out.push_str(
            "Preflight validation runs before any network call; an invalid request is \
             rejected locally (`LsError::Invalid`) with no HTTP call. Type and required-ness \
             always enforce; a value-class bound (enum/range/format) is permissive until the \
             differential probe confirms it, so a valid request is never falsely rejected.\n\n",
        );
        out.push_str("**Request field rules:**\n\n");
        for field in &schema.fields {
            out.push_str(&render_field_rule(field));
        }
        for rule in &schema.cross_field {
            match rule {
                CrossFieldRule::DateOrder {
                    start,
                    end,
                    confirmed,
                } => {
                    let suffix = if *confirmed {
                        "enforced"
                    } else {
                        "permissive until confirmed"
                    };
                    out.push_str(&format!(
                        "- cross-field: `{start}` must not be after `{end}` ({suffix})\n"
                    ));
                }
            }
        }
        out.push('\n');
    }

    if let Some(cov) = coverage {
        if !cov.gateway_codes.is_empty() {
            out.push_str(
                "**Reachable gateway errors** (explained once from the shared catalog; \
                 environment/entitlement codes are not reproduced per TR):\n\n",
            );
            for code in &cov.gateway_codes {
                let explanation = catalog
                    .codes
                    .get(code)
                    .map(|e| e.explanation.as_str())
                    .unwrap_or("(no catalog entry)");
                out.push_str(&format!("- `{code}` — {explanation}\n"));
            }
            out.push('\n');
        }
    }

    out
}

/// Render a single implemented TR's Reference page. A Recommended TR carries its
/// full recommendation contract (R9); an implemented-but-not-recommended TR keeps
/// the not-recommended banner and the schemas/examples-deferred caveat. A TR with
/// an error-resilience constraint schema / coverage additionally carries the
/// "Errors & validation" section (R11).
fn render_reference_page(
    meta: &TrMetadata,
    evidence: Option<&EvidenceRecord>,
    constraint: Option<&ConstraintSchema>,
    coverage: Option<&ErrorCoverage>,
    catalog: &ErrorCatalog,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("# SDK Reference: {}\n\n", meta.tr_code));
    if let Some(name) = &meta.name {
        out.push_str(&format!("{name}\n\n"));
    }
    out.push_str(GENERATED_BANNER);
    out.push_str("\n\n");

    // The not-recommended warning stays prominent, above the owner class, exactly
    // where it was before the contract surface existed — so non-recommended pages
    // remain byte-identical and only a promoted TR's page changes.
    if !meta.support.recommended {
        out.push_str(NOT_RECOMMENDED_BANNER);
        out.push_str("\n\n");
    }

    out.push_str(&format!(
        "- Owner class: `{}`\n\n",
        owner_class_str(meta.owner_class)
    ));

    if meta.support.recommended {
        out.push_str(&render_recommendation(meta, evidence));
    } else {
        out.push_str(
            "_Request/response schemas and verified examples are deferred until this TR reaches \
             recommended status or a real consumer exists._\n",
        );
    }
    out.push_str(&render_errors_and_validation(constraint, coverage, catalog));
    out
}

/// Render the Reference Docs index page over the implemented TRs.
fn render_reference_index(implemented: &BTreeMap<&String, &TrMetadata>) -> String {
    let mut out = String::new();
    out.push_str("# SDK Reference Docs\n\n");
    out.push_str(GENERATED_BANNER);
    out.push_str("\n\n");
    out.push_str(
        "Minimal user-facing reference for the implemented TRs. Tracked-but-unimplemented TRs are \
         intentionally excluded; see the TR Dependency Docs for the full tracked set.\n\n",
    );
    out.push_str("| TR | Name | Owner class | Status |\n");
    out.push_str("|----|------|-------------|--------|\n");
    for (tr_code, meta) in implemented {
        let name = meta.name.as_deref().unwrap_or("");
        let status = if meta.support.recommended {
            "recommended"
        } else {
            "implemented, not yet recommended"
        };
        out.push_str(&format!(
            "| `{tr_code}` | {name} | `{}` | {status} |\n",
            owner_class_str(meta.owner_class),
        ));
    }
    out
}

/// Low-level: render the SDK Reference Docs file set (implemented TRs only),
/// keyed by repo-relative path.
///
/// Filters to `support.implemented == true`, so a tracked-but-unimplemented TR
/// such as `t1964` is excluded from Reference while still appearing in the
/// Dependency Docs (R3). Each entry carries the "not yet recommended" banner
/// whenever `support.recommended == false` (R4).
pub fn render_reference_docs(
    trs: &BTreeMap<String, TrMetadata>,
    evidence: &BTreeMap<String, EvidenceRecord>,
    constraints: &BTreeMap<String, ConstraintSchema>,
    error_coverage: &BTreeMap<String, ErrorCoverage>,
    catalog: &ErrorCatalog,
) -> BTreeMap<PathBuf, String> {
    let dir = Path::new(REFERENCE_DOCS_DIR);
    let implemented: BTreeMap<&String, &TrMetadata> = trs
        .iter()
        .filter(|(_, meta)| meta.support.implemented)
        .collect();

    let mut files = BTreeMap::new();
    files.insert(dir.join("index.md"), render_reference_index(&implemented));
    for (tr_code, meta) in &implemented {
        files.insert(
            dir.join(format!("{tr_code}.md")),
            render_reference_page(
                meta,
                evidence.get(*tr_code),
                constraints.get(*tr_code),
                error_coverage.get(*tr_code),
                catalog,
            ),
        );
    }
    files
}

/// High-level: render the full generated file set from a validated
/// [`ValidationReport`], keyed by repo-relative path.
pub fn render_all(report: &ValidationReport) -> BTreeMap<PathBuf, String> {
    let mut files = render_dependency_docs(&report.trs, &report.index);
    files.extend(render_reference_docs(
        &report.trs,
        &report.evidence,
        &report.constraints,
        &report.error_coverage,
        &report.error_catalog,
    ));
    files
}

/// Write the rendered file set under `root`, creating parent directories.
pub fn write_docs(root: &Path, files: &BTreeMap<PathBuf, String>) -> Result<(), DocgenError> {
    for (rel, contents) in files {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| DocgenError::Io {
                path: parent.to_path_buf(),
                message: e.to_string(),
            })?;
        }
        std::fs::write(&path, contents).map_err(|e| DocgenError::Io {
            path: rel.clone(),
            message: e.to_string(),
        })?;
    }
    Ok(())
}

/// Compare the rendered file set against the committed files under `root`,
/// **bidirectionally**.
///
/// Reports every drifted repo-relative path, sorted: a rendered file that is
/// missing or differs on disk, *and* an orphaned committed `*.md` under the
/// managed doc dirs that the generator no longer produces (e.g. a TR removed
/// from metadata, or demoted out of Reference). Without the orphan sweep a stale
/// file would pass `--check` silently, leaving the R6 guarantee one-directional.
/// An empty vec means the committed docs match the current metadata exactly.
pub fn check_docs(root: &Path, files: &BTreeMap<PathBuf, String>) -> Vec<PathBuf> {
    let mut drifted: Vec<PathBuf> = Vec::new();

    // Forward: each rendered file must be present and identical on disk.
    for (rel, expected) in files {
        let path = root.join(rel);
        match std::fs::read_to_string(&path) {
            Ok(actual) if &actual == expected => {}
            _ => drifted.push(rel.clone()),
        }
    }

    // Reverse: any committed `*.md` under a managed dir that we no longer render
    // is an orphan and counts as drift.
    for dir in [DEPENDENCY_DOCS_DIR, REFERENCE_DOCS_DIR] {
        let managed = Path::new(dir);
        let Ok(entries) = std::fs::read_dir(root.join(managed)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            if let Some(name) = path.file_name() {
                let rel = managed.join(name);
                if !files.contains_key(&rel) {
                    drifted.push(rel);
                }
            }
        }
    }

    drifted.sort();
    drifted.dedup();
    drifted
}

/// Run the full CLI flow: parse args, validate metadata, render, then write or
/// check. The single entry point `main.rs` delegates to.
pub fn run_cli<I, S>(args: I) -> Result<(), DocgenError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mode = parse_mode(args)?;
    let report = validate_dir(&metadata_root()).map_err(DocgenError::MetadataInvalid)?;
    let files = render_all(&report);
    let root = repo_root();
    match mode {
        Mode::Write => write_docs(&root, &files),
        Mode::Check => {
            let drifted = check_docs(&root, &files);
            if drifted.is_empty() {
                Ok(())
            } else {
                Err(DocgenError::Drift(drifted))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_args_resolves_to_write_mode() {
        let empty: [&str; 0] = [];
        assert_eq!(parse_mode(empty).unwrap(), Mode::Write);
    }

    #[test]
    fn check_flag_resolves_to_check_mode() {
        assert_eq!(parse_mode(["--check"]).unwrap(), Mode::Check);
    }

    #[test]
    fn unknown_flag_is_a_located_error() {
        let err = parse_mode(["--nope"]).expect_err("unknown flag must error");
        assert!(matches!(err, DocgenError::UnknownArg(ref a) if a == "--nope"));
        assert!(err.to_string().contains("--nope"));
    }

    /// The authored metadata under `<repo>/metadata`, validated. Doubles as an
    /// integration check that the real set renders.
    fn authored_report() -> ValidationReport {
        validate_dir(&metadata_root()).expect("authored metadata must validate clean")
    }

    /// An empty error catalog for tests that render pages without exercising the
    /// error-resilience projection.
    fn empty_catalog() -> ErrorCatalog {
        ErrorCatalog {
            version: 1,
            codes: BTreeMap::new(),
        }
    }

    /// Render the reference docs from `trs` + `evidence` with no constraint /
    /// coverage / catalog inputs (the pre-error-resilience shape). Keeps the many
    /// projection tests that predate the "Errors & validation" section terse.
    fn render_ref_basic(
        trs: &BTreeMap<String, TrMetadata>,
        evidence: &BTreeMap<String, EvidenceRecord>,
    ) -> BTreeMap<PathBuf, String> {
        render_reference_docs(
            trs,
            evidence,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &empty_catalog(),
        )
    }

    /// The tracked TRs in the slice. The original eight (`token`, `revoke`,
    /// `t1101`, `t1102`, `t8412`, `CSPAQ12200`, `S3_`, `CSPAT00601`) plus the 41
    /// read-only stock/sector TRs brought into tracked-only maintenance ownership
    /// (incl. the Wave A sector cluster t8424/t1511/t1514/t1516/t1485).
    const TRACKED_TRS: [&str; 320] = [
        "AS0",
        "AS1",
        "AS2",
        "AS3",
        "AS4",
        "C01",
        "CCENQ10100",
        "CCENQ90200",
        "CCENT00100",
        "CCENT00200",
        "CCENT00300",
        "CFOAQ10100",
        "CFOAT00100",
        "CFOAT00200",
        "CFOAT00300",
        "CFOBQ10500",
        "CFOEQ11100",
        "CIDBQ01400",
        "CIDBQ03000",
        "CIDBQ05300",
        "CIDBT00100",
        "CIDBT00900",
        "CIDBT01000",
        "CLNAQ00100",
        "COSAT00301",
        "COSAT00311",
        "COSAT00400",
        "COSMT00300",
        "CSPAQ12200",
        "CSPAQ12300",
        "CSPAQ22200",
        "CSPAT00601",
        "CSPAT00701",
        "CSPAT00801",
        "CSPBQ00200",
        "FC9",
        "FH9",
        "GSC",
        "GSH",
        "H01",
        "H1_",
        "HA_",
        "K3_",
        "O01",
        "OC0",
        "OH0",
        "OVC",
        "OVH",
        "S2_",
        "S3_",
        "SC0",
        "SC1",
        "SC2",
        "SC3",
        "SC4",
        "TC1",
        "TC2",
        "TC3",
        "UH1",
        "US2",
        "US3",
        "g3101",
        "g3102",
        "g3103",
        "g3104",
        "g3106",
        "g3190",
        "o3101",
        "o3103",
        "o3104",
        "o3105",
        "o3106",
        "o3107",
        "o3108",
        "o3116",
        "o3117",
        "o3121",
        "o3123",
        "o3125",
        "o3126",
        "o3127",
        "o3128",
        "o3136",
        "o3137",
        "o3139",
        "revoke",
        "t0425",
        "t0167",
        "t0424",
        "t0441",
        "t1101",
        "t1102",
        "t1104",
        "t1105",
        "t1109",
        "t1301",
        "t1302",
        "t1305",
        "t1308",
        "t1310",
        "t1403",
        "t1404",
        "t1405",
        "t1410",
        "t1411",
        "t1422",
        "t1427",
        "t1441",
        "t1442",
        "t1444",
        "t1449",
        "t1452",
        "t1463",
        "t1466",
        "t1471",
        "t1475",
        "t1481",
        "t1482",
        "t1485",
        "t1486",
        "t1488",
        "t1489",
        "t1492",
        "t1511",
        "t1514",
        "t1516",
        "t1531",
        "t1532",
        "t1533",
        "t1537",
        "t1601",
        "t1602",
        "t1603",
        "t1615",
        "t1617",
        "t1621",
        "t1631",
        "t1632",
        "t1633",
        "t1636",
        "t1637",
        "t1638",
        "t1640",
        "t1662",
        "t1664",
        "t1665",
        "t1702",
        "t1716",
        "t1717",
        "t1752",
        "t1764",
        "t1771",
        "t1809",
        "t1825",
        "t1826",
        "t1852",
        "t1856",
        "t1859",
        "t1860",
        "t1866",
        "t1901",
        "t1902",
        "t1903",
        "t1904",
        "t1906",
        "t1921",
        "t1926",
        "t1927",
        "t1941",
        "t1950",
        "t1951",
        "t1954",
        "t1956",
        "t1958",
        "t1959",
        "t1960",
        "t1961",
        "t1964",
        "t1966",
        "t1969",
        "t1971",
        "t1972",
        "t1973",
        "t1974",
        "t1988",
        "t2106",
        "t2111",
        "t2112",
        "t2210",
        "t2212",
        "t2214",
        "t2216",
        "t2301",
        "t2407",
        "t2424",
        "t2522",
        "t2541",
        "t2545",
        "t3102",
        "t3202",
        "t3320",
        "t3341",
        "t3401",
        "t3518",
        "t3521",
        "t4203",
        "t8401",
        "t8402",
        "t8403",
        "t8404",
        "t8405",
        "t8406",
        "t8407",
        "t8410",
        "t8411",
        "t8412",
        "t8417",
        "t8418",
        "t8419",
        "t8424",
        "t8425",
        "t8426",
        "t8427",
        "t8428",
        "t8430",
        "t8431",
        "t8433",
        "t8434",
        "t8435",
        "t8436",
        "t8450",
        "t8451",
        "t8452",
        "t8453",
        "t8454",
        "t8455",
        "t8460",
        "t8462",
        "t8463",
        "t8464",
        "t8465",
        "t8466",
        "t8467",
        "t9905",
        "t9907",
        "t9942",
        "t9943",
        "t9944",
        "t9945",
        "token",
        // Closure flip WS batch (plan -004): 31 tracked-only realtime channels.
        "BMT",
        "BM_",
        "CUR",
        "FD0",
        "IJ_",
        "JC0",
        "JD0",
        "JH0",
        "JIF",
        "K1_",
        "KH_",
        "KM_",
        "KS_",
        "MK2",
        "NBT",
        "NH1",
        "NK1",
        "NS2",
        "NS3",
        "NWS",
        "OD0",
        "OK_",
        "OMG",
        "PH_",
        "VI_",
        "WOC",
        "WOH",
        "YF9",
        "YK3",
        "YOC",
        "YS3",
        "AFR",
        "B7_",
        "C02",
        "CD0",
        "DBM",
        "DBT",
        "DC0",
        "DD0",
        "DH0",
        "DH1",
        "DHA",
        "DK3",
        "DS3",
        "DVI",
        "ESN",
        "FX9",
        "H02",
        "H2_",
        "HB_",
        "I5_",
        "JX0",
        "NBM",
        "NPM",
        "NVI",
        "O02",
        "OX0",
        "SHC",
        "SHD",
        "SHI",
        "SHO",
        "UBM",
        "UBT",
        "UK1",
        "UVI",
        "UYS",
        "YC3",
        "YJC",
        "YJ_",
        "h3_",
    ];

    #[test]
    fn dependency_page_for_t8412_renders_all_metadata_facts() {
        let report = authored_report();
        let files = render_dependency_docs(&report.trs, &report.index);
        let page = files
            .get(Path::new("docs/tr-dependencies/t8412.md"))
            .expect("t8412 page exists");

        // Owner class, support flags, facets, dependency fields, venue/session.
        assert!(page.contains("Owner class: `paginated`"));
        assert!(page.contains("- Tracked: yes"));
        assert!(page.contains("- Implemented: yes"));
        // Demoted to Implemented by the error-resilience gate (plan 2026-07-01-004,
        // R12); re-promotes only after passing the new differential-probe gate (U8).
        assert!(page.contains("- Recommended: no"));
        assert!(page.contains("Venue / session: `krx_regular`"));
        assert!(page.contains("Date sensitive: yes"));
        assert!(page.contains("Self-paginated: yes"));
        assert!(page.contains("Self-continuation fields: `cts_date`, `cts_time`"));
    }

    #[test]
    fn every_tracked_tr_gets_a_page_and_the_index_lists_all_tracked() {
        let report = authored_report();
        let files = render_dependency_docs(&report.trs, &report.index);

        // index + one page per TR.
        assert!(files.contains_key(Path::new("docs/tr-dependencies/index.md")));
        for tr in TRACKED_TRS {
            let path = format!("docs/tr-dependencies/{tr}.md");
            assert!(
                files.contains_key(Path::new(&path)),
                "missing page for {tr}"
            );
        }
        assert_eq!(
            files.len(),
            TRACKED_TRS.len() + 1,
            "index + one page per tracked TR"
        );

        let index = files
            .get(Path::new("docs/tr-dependencies/index.md"))
            .expect("index exists");
        for tr in TRACKED_TRS {
            assert!(index.contains(&format!("`{tr}`")), "index omits {tr}");
        }
    }

    #[test]
    fn tr_with_empty_dependency_fields_renders_none_not_dangling() {
        let report = authored_report();
        let files = render_dependency_docs(&report.trs, &report.index);
        let page = files
            .get(Path::new("docs/tr-dependencies/token.md"))
            .expect("token page exists");

        // token has no dependency coupling — sections render `none`, not empty.
        assert!(page.contains("Self-continuation fields: none"));
        assert!(page.contains("Strong-order fields: none"));
        assert!(page.contains("Caller-supplied identifiers: none"));
        // No empty trailing bullet (a "- \n" would be a dangling list item).
        assert!(!page.contains("- \n"), "no dangling empty bullets");
    }

    #[test]
    fn dependency_rendering_is_deterministic() {
        let report = authored_report();
        let a = render_dependency_docs(&report.trs, &report.index);
        let b = render_dependency_docs(&report.trs, &report.index);
        assert_eq!(a, b, "identical metadata yields byte-identical output");
    }

    use ls_metadata::{
        Dependencies, Facets, InstrumentDomain, Maintenance, Protocol, RateBucket, Recommendation,
        Support, VenueSession,
    };

    /// Build a minimal TR record for inline reference-rendering tests.
    fn sample_meta(tr_code: &str, implemented: bool, recommended: bool) -> TrMetadata {
        TrMetadata {
            tr_code: tr_code.to_string(),
            name: Some(format!("name of {tr_code}")),
            owner_class: OwnerClass::Standalone,
            facets: Facets {
                protocol: Protocol::Rest,
                instrument_domain: InstrumentDomain::Misc,
                venue_session: VenueSession::Unspecified,
                date_sensitive: false,
                self_paginated: false,
                account_state: false,
                paper_incompatible: false,
                certification_path: CertificationPath::Automated,
                rate_bucket: RateBucket::Auth,
                caller_supplied_identifiers: vec![],
            },
            dependencies: Dependencies::default(),
            support: Support {
                tracked: true,
                implemented,
                recommended,
            },
            maintenance: Maintenance {
                source_spec_hash: "deadbeef".to_string(),
                last_reviewed: "2026-06-15".to_string(),
            },
            recommendation: None,
            constraints_ref: None,
            error_coverage_ref: None,
        }
    }

    #[test]
    fn reference_covers_implemented_with_banner_and_omits_unimplemented() {
        let report = authored_report();
        let reference = render_reference_docs(&report.trs, &report.evidence, &report.constraints, &report.error_coverage, &report.error_catalog);
        let dependency = render_dependency_docs(&report.trs, &report.index);

        // The still-unrecommended implemented TRs each carry the banner.
        let banner_trs = [
            "CFOAQ10100", "CFOBQ10500", "CSPAQ12300", "CSPAQ22200", "revoke", "t1403", "t1441",
            "t1452", "t1463",
            "t1466", "t1481", "t1482", "t1485", "t1489", "t1492", "t1511", "t1514", "t1516", "t1531",
            "t1537", "t1601",
            "t1615", "t1640", "t1662", "t1664", "t1825", "t1826", "t1859", "t1866", "t1958",
            "t2301", "t2522", "t3341", "t8401", "t8424", "t8425", "t8426", "t8433", "t8435",
            "t8467", "t9943", "t9944", "t8430", "t8431", "t8436", "t9905", "t9907", "t9942",
            "t2111", "t2112", "t8402", "t8403", "t8434",
            "t1988", "t3320",
            "t9945", "t3202", "t3401", "t8410", "t8451", "t8419", "t4203",
            // All-lane closed-window flip wave (plan -003) — domestic REST lane
            // (overseas-index reads via /stock/investinfo, populated under closure).
            "t3518", "t3521",
            // All-lane closed-window flip wave (plan -003) — overseas-futures(-option)
            // chart/market-data reads (front-month CUSN26 persists under closure) +
            // KRX night-derivatives investor table. o3107/o3127 stayed PENDING
            // (account-state watchlist boards return empty/zero rows).
            "o3103", "o3104", "o3108", "o3116", "o3117", "o3123", "o3128", "o3136", "o3137", "o3139",
            "t8462",
            "t1901", "t1906", "t8450", "t1638", "t1308", "t1449", "t1621", "t2545", "t8406", "t8407", "t1959", "t1950", "t1971", "t1972", "t1974", "t1956", "t1969", "t1105", "t1104", "t1305",
            // Open-window flip wave (plan -001, 2026-06-30): ELW daily-price read.
            "t1954",
            "t1310", "t1404", "t1410", "t1411", "t1488", "t1636", "t1809",
            "t8417", "t8418", "t8411", "t8452", "t8453", "t1302",
            "t8464", "t8465", "t8466", "t2216", "t8405",
            "t1444", "t1422", "t1427", "t1442", "t1405", "t1960", "t1961", "t1966", "t1921", "t1532", "t1533", "t1926", "t1764", "t1903",
            // CSPAT00601/00701/00801 + t0425 promoted to Recommended (plan
            // 2026-06-30-002) — moved to the recommended-no-banner loop below.
            // Closed-window account-lane flip wave (plan -001).
            "t0424", "t0167", "CLNAQ00100",
            // Paper account credential lanes (plan -002): F/O + overseas-F/O account
            // reads that flipped once authenticated as their own account's lane.
            "CFOEQ11100", "CIDBQ01400", "CIDBQ03000", "CIDBQ05300",
            "o3101", "o3121",
            "K3_",
            "H1_", "HA_", "S2_", "US3", "UH1", "US2", "GSC", "GSH", "OVC", "OVH", "OC0", "OH0",
            "FC9", "FH9",
            "SC0", "SC1", "SC2", "SC3", "SC4", "C01", "O01", "H01", "AS0", "AS1", "AS2", "AS3",
            "AS4", "TC1", "TC2", "TC3",
            // Closure flip WS batch (plan -004): 31 connection-reachable-only realtime
            // channels flipped on a clean paper lifecycle sweep (make live-smoke-ws-p3).
            "NS3", "NH1", "NS2", "NK1", "NBT", "KS_", "OK_", "KH_", "KM_", "PH_", "K1_", "IJ_",
            "YS3", "YK3", "VI_", "JC0", "JH0", "JD0", "FD0", "OD0", "OMG", "YF9", "YOC", "BM_",
            "WOC", "WOH", "JIF", "NWS", "BMT", "CUR", "MK2",
            // Open-window domestic program-trade reads: intraday-trend t1632 +
            // daily-trend t1633 certified non-empty (t1631 PENDING — gateway IGW40014).
            "t1632", "t1633",
            // Open-window domestic reads: foreign/institution by-issue trend t1702 +
            // net-buy trend t1717, investor-by-sector chart t1665, intraday
            // quote-remainder trend t1471, VP-relative ranking t1475 — all certified
            // non-empty on in-window paper smokes (KRX open 2026-06-29).
            "t1702", "t1717", "t1665", "t1471", "t1475",
            // Open-window domestic reads: foreign/institution by-issue trend t1716,
            // ETF intraday-trend t1902 + constituents t1904, short-sale daily trend
            // t1927, stock-loan/대차 daily trend t1941 — all certified non-empty on
            // in-window paper smokes (KRX open 2026-06-29).
            "t1716", "t1902", "t1904", "t1927", "t1941",
            // Open-window domestic paginated reads (plan -001): time-band tick
            // conclusion t1301 + t8454, expected-conclusion t1486, per-stock
            // program-trade flow t1637 — all certified non-empty on in-window paper
            // smokes (KRX open 2026-06-29). t1109 (시간외체결량) PENDS: empty 00707
            // in the regular continuous session (after-hours data does not populate).
            "t1301", "t1486", "t8454", "t1637",
            // Open-window domestic paginated reads (plan -001): investor-flow reads
            // t1602 (time-band by sector) + t1603 (detail by issue) + t1617
            // (time/daily) and exchange-broker reads t1752 (broker-by-issue) +
            // t1771 (broker time-series) — all certified non-empty on in-window
            // paper smokes (KRX open 2026-06-29).
            "t1602", "t1603", "t1617", "t1752", "t1771",
            // Open-window F-O + domestic reads (plan -001): F/O investor-by-time
            // t2541, F/O daily OHLCV t2214, F/O N-minute bars t2424, F/O unusual-
            // volume conclusion counts t2210 (front-month code self-sourced from
            // t8467), deposit-balance trend t8428, and KRX night-derivatives
            // investor-timeslot t8463 (day session, paper_incompatible retracted) —
            // all certified non-empty on in-window paper smokes (KRX open
            // 2026-06-29). t8455/g3190 stayed PENDING (night-master/overseas-stock
            // carry no day-session/paper feed, flags kept); t8427 PENDS (empty
            // chart); o3127 PENDS (zero price/empty symbolname).
            "t2541", "t2214", "t2424", "t2210", "t8428", "t8463",
            // Open-window WS track/flip wave (plan 2026-06-29-001): 39
            // connection-reachable-only realtime channels flipped on a clean paper
            // lifecycle sweep (make live-smoke-ws-p4; KTD6 NOT-OBSERVABLE, so the
            // claim is connection reachability only). All 39 connected cleanly.
            "AFR", "B7_", "C02", "CD0", "DBM", "DBT", "DC0", "DD0", "DH0", "DH1", "DHA", "DK3",
            "DS3", "DVI", "ESN", "FX9", "H02", "H2_", "HB_", "I5_", "JX0", "NBM", "NPM", "NVI",
            "O02", "OX0", "SHC", "SHD", "SHI", "SHO", "UBM", "UBT", "UK1", "UVI", "UYS", "YC3",
            "YJC", "YJ_", "h3_",
            // KRX-open domestic F/O order certify-flip (plan 2026-07-01-001): the domestic
            // F/O order chain certified in-window (submit 00040 / modify 00462 / cancel
            // 00463, flat confirmed; make live-smoke-fo-order). Implemented-not-recommended.
            "CFOAT00100", "CFOAT00200", "CFOAT00300",
            // Error-resilience gate (plan 2026-07-01-004, R12): the 10 TRs promoted
            // under the old happy-path gate demoted to Implemented so the badge means
            // "fails gracefully". Each carries the not-recommended banner again until
            // re-certified through the new differential-probe gate (U8, operator-run).
            "token", "t1101", "t1102", "t8412", "S3_", "CSPAQ12200", "CSPAT00601",
            "CSPAT00701", "CSPAT00801", "t0425",
        ];
        for tr in banner_trs {
            let page = reference
                .get(Path::new(&format!("docs/reference/{tr}.md")))
                .unwrap_or_else(|| panic!("reference page for {tr}"));
            assert!(
                page.contains("Implemented, not yet recommended"),
                "{tr} reference must carry the not-recommended banner"
            );
        }

        // The Recommended set is EMPTY after the error-resilience demotion (R12):
        // no reference page omits the banner. Re-promotion (U8) is operator-gated and
        // proceeds independently across live windows.
        let recommended_no_banner: [&str; 0] = [];
        for rec in recommended_no_banner {
            let page = reference
                .get(Path::new(&format!("docs/reference/{rec}.md")))
                .unwrap_or_else(|| {
                    panic!("{rec} reference page still renders (still implemented)")
                });
            assert!(
                !page.contains("Implemented, not yet recommended"),
                "{rec} is recommended — its reference page must not carry the banner"
            );
        }

        // index + the implemented reference pages (banner-carrying still-implemented
        // TRs + the promoted-but-still-implemented token/t1101/t1102/t8412/S3_/
        // CSPAQ12200). Count only grows as TRs implement; CSPAQ12300 (PR-A U1),
        // CSPAQ22200 (PR-A U2), and CFOBQ10500 (PR-A U3) each add one
        // banner-carrying page. t2301 (PR-B U4) adds one more (F/O option board);
        // t2522 (PR-B U5) adds one more (F/O stock-futures underlying master);
        // t8401 (PR-B U6) adds one more (F/O stock-futures master);
        // t8426 (PR-B U7) adds one more (F/O commodity-futures master);
        // t8433 (PR-B U8) adds one more (F/O index-option master);
        // t8435 (PR-B U9) adds one more (F/O derivatives master);
        // t8467 (PR-B U10) adds one more (F/O index-futures master);
        // t9943 (PR-B U11) adds one more (F/O index-futures master, array out-block).
        // t9944 (PR-B U12) adds one more (F/O index-option master, array out-block).
        // (Wave 1 t1988 + t1964 ship PENDING — not implemented, not counted.)
        // t1481 + t1482 (U2 reach wave, paginated body-`idx`) add one each.
        // t2111 (U5 reach wave, F/O current-price quote) adds one more.
        // t2112 (U5 reach wave, F/O current-price order book) adds one more.
        // t8402 (U5 reach wave, stock-futures current price) adds one more.
        // t8403 (U5 reach wave, stock-futures order book) adds one more.
        // t8434 (U5 reach wave, F/O multi current-price, array out-block) adds one more.
        // (t2106 ships PENDING — empty memo array off-session — not counted.)
        // CFOAQ10100 (U4 reach wave, F/O orderable-quantity inquiry) adds one more.
        // (CCENQ90200 + CCENQ10100 ship Tracked/paper-incompatible (gateway 01900) —
        // not implemented, not counted.)
        // t1988 (U3 reach wave, ELW underlying-asset list; standalone→market_session,
        // IGW40011 cleared by the `from_rate`/`to_rate` wire-type fix) adds one more.
        // t3320 (U3 reach wave, FnGuide company summary; standalone→market_session,
        // bare-6-digit gicode found via raw-probe A/B) adds one more.
        // (t3102 ships HELD input-unresolved (sNewsno only from realtime NWS) —
        // not counted.)
        // o3101 (U8 reach wave, overseas-futures master; market_session, ARRAY
        // out-block; non-empty 85-row paper smoke) adds one more.
        // o3121 (U8 reach wave, overseas-future-option master; market_session,
        // ARRAY out-block; non-empty 2-row paper smoke) adds one more.
        // K3_ (realtime flip wave, KOSDAQ 체결 WebSocket; clean paper lifecycle smoke,
        // connection-reachable-only per KTD6=NOT-OBSERVABLE) adds one more.
        // P1 market-data WS lane (realtime flip wave): H1_/HA_/S2_/US3/UH1/US2 (stock),
        // GSC/GSH (overseas stock), OVC/OVH (overseas futures), OC0/OH0/FC9/FH9 (F/O) —
        // 14 TRs, each a clean paper lifecycle smoke via the live-smoke-ws-p1 sweep,
        // connection-reachable-only per KTD6 — add 14 more.
        // P2 order-event WS lane (realtime flip wave): SC0-SC4 (stock), C01/O01/H01 (F/O),
        // AS0-AS4 (overseas stock), TC1-TC3 (overseas futures) — 16 TRs, each a clean
        // order-event (tr_type "1"/"2") lifecycle via the live-smoke-ws-p2 sweep,
        // observation-only + connection-reachable-only per KTD6 — add 16 more.
        // Night/overseas/ELW wave: the four overseas-futures/-option reads
        // o3105/o3106/o3125/o3126 flipped on clean non-empty paper smokes once their
        // smoke symbols were refreshed to current front-month contracts (the stale
        // 2023-expiry symbols had masked a provisioned feed as empty) — add 4 more.
        // Domestic stock master/reference breadth wave (plan -004): the seven reads
        // t9945/t3202/t3401/t8410/t8451/t8419/t4203 each flipped on a clean non-empty
        // paper smoke — add 7 more.
        // Order flip-certify wave (plan -005): the three order TRs CSPAT00601 (submit)/
        // CSPAT00701 (modify)/CSPAT00801 (cancel) + t0425 (reconcile read) each certified
        // on a clean guarded paper order-chain smoke (`make live-smoke-order-chain`) —
        // add 4 more. They stop at Implemented; Recommended is gated on ADR 0008.
        // t1901 ETF현재가 + t1105 피봇/디마크 + t1104 현재가시세메모 + t1305 기간별주가
        // each flipped Implemented on clean typed paper smokes (rsp_cd 00000) — add 4
        // (plan -002 Track 2).
        // Closed-window flip wave (plan -003): t1310 주식당일전일분틱 (20-tick paper
        // smoke) + t1404 관리/불성실/투자유의 designation board (100-row paper smoke)
        // each certified non-empty UNDER closure — add 2.
        // Closed-window breadth flip wave (plan -004) batch A: the six chart/price reads
        // t8417/t8418 (sector tick/N분) + t8411 (stock tick) + t8452/t8453 (integrated
        // stock N분/tick) + t1302 (분별주가) each certified non-empty (20 rows) UNDER
        // closure — add 6.
        // Plan -004 batch B: five domestic F/O chart/period reads t8464/t8465/t8466
        // (선물옵션 tick/N분/일주월) + t2216 (F/O tick trade chart) + t8405 (주식선물
        // 기간별주가) each certified non-empty (20 rows) UNDER closure, contract sourced
        // from a derivatives master at smoke time — add 5.
        // Plan -004 batch C: 14 static reference/designation/ranking reads (시가총액상위
        // t1444, 상·하한 t1422/t1427, 신고저가 t1442, 매매정지 t1405, ELW rankings
        // t1960/t1961/t1966, 신용 t1921/t1926, 테마 t1532/t1533, 회원사 t1764, ETF일별
        // t1903) each certified non-empty UNDER closure — add 14.
        // Plan -001 closed-window more-flips: ETF LP order-book t1906 certified
        // non-empty UNDER closure (shcode=152100) — add 1.
        // Plan -001 closed-window more-flips: integrated current-price/order-book
        // t8450 certified non-empty UNDER closure (shcode=005930, exchgubun=N) — add 1.
        // Plan -001 closed-window more-flips: per-stock remaining-quantity/pre-disclosure
        // t1638 certified non-empty UNDER closure (gubun1=1, shcode="" full list, 780 rows) — add 1.
        // Plan -001 closed-window more-flips: time-bucketed trade-chart t1308 certified
        // non-empty UNDER closure (shcode=005930, full session, bun_term=1, 381 rows) — add 1.
        // Plan -001 closed-window more-flips: price-band trade-weight t1449 certified
        // non-empty UNDER closure (shcode=005930, dategb=1, 141 price-band rows) — add 1.
        // Plan -001 closed-window more-flips: by-time investor-trading t1621 certified
        // non-empty UNDER closure (upcode=001, nmin=0, cnt=20, 20 by-time rows;
        // nmin/cnt serialize as JSON numbers or IGW40011, KTD3) — add 1.
        // Plan -001 closed-window more-flips: multi-symbol current-price t8407 certified
        // non-empty UNDER closure (nrec=3, shcode 3 concatenated codes, 3 per-symbol
        // rows; nrec serializes as a JSON number or IGW40011, KTD3) — add 1.
        // Plan -001 closed-window more-flips: LP-target ELW issue-list t1959 certified
        // non-empty UNDER closure (shcode="" full LP-target list, 420 per-issue rows) —
        // add 1.
        // Plan -001 closed-window more-flips: ELW screener t1969 certified non-empty
        // UNDER closure (all-ELWs default screen, summary cnt + 2883 screened rows) —
        // add 1.
        // Plan -001 closed-window more-flips: ELW current-price/quote t1950 certified
        // non-empty UNDER closure (t8431-sourced fresh shcode, populated single-instrument
        // quote + 1 basket row) — add 1.
        // Plan -001 closed-window more-flips: ELW current-price + quote-board t1971
        // certified non-empty UNDER closure (t8431-sourced fresh shcode, populated
        // single-instrument quote-board, top bid/offer present) — add 1.
        // Plan -001 closed-window more-flips: ELW current-price + trading-member board
        // t1972 certified non-empty UNDER closure (t8431-sourced fresh shcode, populated
        // single-instrument member board, modeled member-volume field present) — add 1.
        // Plan -001 closed-window more-flips: ELWs-sharing-a-base-asset t1974 certified
        // non-empty UNDER closure (t8431-sourced fresh shcode, populated same-base sibling
        // list t1974OutBlock1 + cnt summary, modeled hname present) — add 1.
        // Plan -001 closed-window more-flips: ELW current-price/payout snapshot t1956
        // certified non-empty UNDER closure (t8431-sourced fresh shcode, rsp_cd=00000,
        // populated single t1956OutBlock hname NAME witness + basket array t1956OutBlock1)
        // — add 1.
        // Plan -001 closed-window more-flips: F/O by-time investor-trading t2545 certified
        // non-empty UNDER closure (eitem=01, sgubun=0, upcode=001, nmin=0, cnt=10, bgubun=0,
        // rsp_cd=00000, 10 by-time rows, modeled date/indmsvol; nmin/cnt serialize as JSON
        // numbers and bgubun="0" or IGW40011/IGW50008, KTD3) — add 1.
        // Plan -001 closed-window more-flips: F/O by-tick conclusion t8406 certified non-empty
        // UNDER closure (t8467-sourced live front-month focode, cgubun=1, bgubun=0, cnt=10,
        // rsp_cd=00000, 10 conclusion rows, modeled chetime/price; bgubun/cnt serialize as
        // JSON numbers or IGW40011, KTD3) — add 1.
        // Closed-window account-lane flip wave (plan -001): t0424 (주식잔고2) certified
        // a non-default cash summary UNDER closure (holdings=0 cash-only account, the
        // U2 holdings gate; cash witness sunamt non-default) — add 1.
        // t0167 (서버시간조회) certified a non-default server time (utility) — add 1.
        // CLNAQ00100 (예탁담보융자가능종목) certified a non-empty loanable-stock list
        // (20 stocks, non-default IsuNm) UNDER closure (persistent reference) — add 1.
        // Paper account credential lanes (plan -002): CFOEQ11100 (선물옵션가정산예탁금상세)
        // certified a non-default deposit (Dps) once authenticated as the F/O account
        // via the domestic_option lane (…51) — the §16 PENDING was a wrong-account
        // artifact (all-zero on the cash-only …01 account) — add 1.
        // CIDBQ01400 (해외선물 주문가능수량) certified a non-default OrdAbleQty on the
        // overseas_option lane (…71) — likewise a §16 wrong-account artifact — add 1.
        // U5 track-and-flip: CIDBQ03000 (해외선물 예수금/잔고현황) certified a non-default
        // EvalAssetAmt on the overseas_option lane (…71) — was 00707 on …01 (§16); TrdDt
        // must be a trading day (weekend → 01715) — add 1.
        // CIDBQ05300 (해외선물 예탁자산) certified a non-default OvrsFutsDps on the
        // overseas_option lane (…71) — the cash account returned IGW40013 (§16) — add 1.
        // All-lane closed-window flip wave (plan -003), domestic REST lane: t3518
        // (해외실시간지수 time-series) + t3521 (해외지수조회 snapshot) certified non-empty
        // index data via /stock/investinfo under closure — add 2.
        // All-lane closed-window flip wave (plan -003), overseas-futures(-option) +
        // night-deriv REST lanes: o3103/o3104/o3108/o3116/o3117/o3123/o3128/o3136/
        // o3137/o3139 (front-month CUSN26 persists under closure) + t8462 (KRX
        // night-derivatives investor table) certified non-empty — add 11.
        // (o3107/o3127 stayed PENDING: account-state watchlist boards empty/zero.)
        // Open-window domestic program-trade reads: intraday-trend t1632 +
        // daily-trend t1633 certified non-empty (20-row series each) — add 2.
        // (t1631 stayed PENDING: gateway-side IGW40014 on its own response payload —
        //  live-confirmed 2026-06-29, garbage bytes in bidvolume; not flippable.)
        // Open-window domestic reads: foreign/institution by-issue trend t1702 +
        // net-buy trend t1717, investor-by-sector chart t1665, intraday
        // quote-remainder trend t1471, VP-relative ranking t1475 — all certified
        // non-empty on in-window paper smokes (KRX open 2026-06-29) — add 5.
        // Open-window domestic reads: foreign/institution by-issue trend t1716, ETF
        // intraday-trend t1902 + constituents t1904, short-sale daily trend t1927,
        // stock-loan/대차 daily trend t1941 — all certified non-empty on in-window
        // paper smokes (KRX open 2026-06-29) — add 5.
        // Open-window domestic paginated reads (plan -001): time-band tick conclusion
        // t1301 + t8454, expected-conclusion t1486, per-stock program-trade flow t1637
        // — all certified non-empty (20/20/20/21-row series) on in-window paper smokes
        // (KRX open 2026-06-29) — add 4. (t1109 stayed PENDING: empty 00707 in the
        // regular continuous session.)
        // Open-window domestic paginated reads (plan -001): investor-flow t1602/t1603/
        // t1617 + exchange-broker t1752/t1771 — all certified non-empty (20/20/20/15/1-row
        // series) on in-window paper smokes (KRX open 2026-06-29) — add 5.
        // Open-window F-O + domestic reads (plan -001): t2541/t2214/t2424/t2210/t8428/
        // t8463 — all certified non-empty on in-window paper smokes (KRX open
        // 2026-06-29) — add 6.
        // Open-window WS track/flip wave (plan 2026-06-29-001): 39 realtime channels
        // (AFR/B7_/C02/.../h3_) flipped connection-reachable-only on a clean paper WS
        // lifecycle sweep (make live-smoke-ws-p4; KTD6 NOT-OBSERVABLE) — add 39.
        // Open-window flip wave (plan 2026-06-30-001): t1954 ELW daily-price read
        // flipped on a non-empty open-window paper smoke (rows=20, close witness) — add 1.
        // KRX-open domestic F/O order certify-flip (plan 2026-07-01-001): the domestic
        // F/O order chain CFOAT00100/00200/00300 (submit/modify/cancel) certified on a
        // clean in-window guarded paper order-chain smoke (rsp_cd 00040/00462/00463, flat
        // confirmed; make live-smoke-fo-order, KRX open 2026-07-01) — add 3.
        assert_eq!(
            reference.len(),
            283,
            "index + the implemented reference pages"
        );

        // A tracked-but-unimplemented TR (t1964, empty-board ELW, still Tracked) is
        // excluded from Reference …
        assert!(!reference.contains_key(Path::new("docs/reference/t1964.md")));
        let ref_index = &reference[Path::new("docs/reference/index.md")];
        assert!(!ref_index.contains("t1964"));

        // … but still appears in the Dependency Docs.
        assert!(dependency.contains_key(Path::new("docs/tr-dependencies/t1964.md")));

        // The now-implemented order TRs DO appear in Reference (banner-carrying).
        assert!(reference.contains_key(Path::new("docs/reference/CSPAT00601.md")));
    }

    #[test]
    fn reference_banner_is_keyed_on_recommended_flag() {
        let mut trs: BTreeMap<String, TrMetadata> = BTreeMap::new();
        trs.insert("rec".to_string(), sample_meta("rec", true, true));
        trs.insert("notrec".to_string(), sample_meta("notrec", true, false));

        let reference = render_ref_basic(&trs, &BTreeMap::new());

        let rec = &reference[Path::new("docs/reference/rec.md")];
        let notrec = &reference[Path::new("docs/reference/notrec.md")];
        assert!(
            !rec.contains("Implemented, not yet recommended"),
            "a recommended TR drops the banner"
        );
        assert!(
            notrec.contains("Implemented, not yet recommended"),
            "a not-recommended TR keeps the banner"
        );
    }

    #[test]
    fn reference_excludes_unimplemented_tr() {
        let mut trs: BTreeMap<String, TrMetadata> = BTreeMap::new();
        trs.insert("done".to_string(), sample_meta("done", true, false));
        trs.insert(
            "tracked_only".to_string(),
            sample_meta("tracked_only", false, false),
        );

        let reference = render_ref_basic(&trs, &BTreeMap::new());
        assert!(reference.contains_key(Path::new("docs/reference/done.md")));
        assert!(!reference.contains_key(Path::new("docs/reference/tracked_only.md")));
    }

    /// Build a recommended TR (with a contract block) plus its evidence record.
    fn recommended_with_evidence() -> (
        BTreeMap<String, TrMetadata>,
        BTreeMap<String, EvidenceRecord>,
    ) {
        let mut meta = sample_meta("token", true, true);
        meta.recommendation = Some(Recommendation {
            behavior: "Paper OAuth access-token issuance".to_string(),
            excludes: vec!["Production-credential token issuance".to_string()],
            evidence_ref: "evidence/token.yaml".to_string(),
        });
        let mut trs = BTreeMap::new();
        trs.insert("token".to_string(), meta);

        let mut evidence = BTreeMap::new();
        evidence.insert(
            "token".to_string(),
            EvidenceRecord {
                tr_code: "token".to_string(),
                date: "2026-06-15".to_string(),
                env: "paper".to_string(),
                target: Some("live-smoke".to_string()),
                line: None,
                attested_shape: None,
                attested_normalizer_version: None,
            },
        );
        (trs, evidence)
    }

    #[test]
    fn recommended_page_renders_full_contract_and_no_deferred_line() {
        // Covers AE3: behavior, evidence + env, freshness date, and excludes are
        // all present; the schemas-deferred caveat is gone for a recommended TR.
        let (trs, evidence) = recommended_with_evidence();
        let reference = render_ref_basic(&trs, &evidence);
        let page = &reference[Path::new("docs/reference/token.md")];

        assert!(page.contains("## Recommendation"));
        assert!(page.contains("Paper OAuth access-token issuance"));
        assert!(page.contains("environment: `paper`"));
        assert!(page.contains("Freshness date: `2026-06-15`"));
        assert!(page.contains("Production-credential token issuance"));
        assert!(
            !page.contains("deferred until this TR reaches"),
            "a recommended TR must not carry the schemas-deferred caveat"
        );
        assert!(
            !page.contains("Implemented, not yet recommended"),
            "a recommended TR drops the not-recommended banner"
        );
    }

    #[test]
    fn recommended_page_renders_deterministic_review_by_date() {
        // The review-by date is `last_reviewed` + 90 days — a pure derivation
        // (no clock), the docgen surface for R8/R9 as refined. token's freshness
        // date 2026-06-15 + 90 days = 2026-09-13.
        let (trs, evidence) = recommended_with_evidence();
        let reference = render_ref_basic(&trs, &evidence);
        let page = &reference[Path::new("docs/reference/token.md")];
        assert!(
            page.contains("Review by: `2026-09-13` (freshness date + 90-day backstop)"),
            "recommended page must render the deterministic review-by date"
        );
    }

    #[test]
    fn recommended_page_states_policy_per_clause_candor() {
        // Covers R10: per-clause candor — both the backstop and change-driven
        // staling detection read as enforced, while only the auto-revoke arm reads
        // as deferred. Guards against both over-claiming auto-revoke and
        // under-claiming the enforced detection.
        let (trs, evidence) = recommended_with_evidence();
        let reference = render_ref_basic(&trs, &evidence);
        let page = &reference[Path::new("docs/reference/token.md")];

        assert!(
            page.contains("90-day backstop is enforced"),
            "the backstop clause must read as enforced"
        );
        assert!(
            page.contains("Change-driven staling is also enforced"),
            "the change-driven detection clause must read as enforced"
        );
        assert!(
            page.contains("auto-revoke") && page.contains("deferred"),
            "only the auto-revoke arm stays deferred"
        );
        assert!(page.contains("EVIDENCE-FRESHNESS.md"));
    }

    #[test]
    fn not_recommended_implemented_tr_keeps_deferred_caveat_and_banner() {
        // Covers AE5 (negative side): the five banner TRs are unregressed —
        // implemented-but-not-recommended pages still defer schemas and warn.
        let mut trs = BTreeMap::new();
        trs.insert("notrec".to_string(), sample_meta("notrec", true, false));
        let reference = render_ref_basic(&trs, &BTreeMap::new());
        let page = &reference[Path::new("docs/reference/notrec.md")];

        assert!(page.contains("Implemented, not yet recommended"));
        assert!(page.contains("deferred until this TR reaches"));
        assert!(!page.contains("## Recommendation"));
        assert!(
            !page.contains("Review by:"),
            "a non-recommended TR carries no review-by line"
        );
    }

    #[test]
    fn recommended_rendering_is_deterministic() {
        let (trs, evidence) = recommended_with_evidence();
        let a = render_ref_basic(&trs, &evidence);
        let b = render_ref_basic(&trs, &evidence);
        assert_eq!(
            a, b,
            "identical metadata + evidence yields identical output"
        );
    }

    /// A unique tempdir under the OS temp root (no external crate), mirroring the
    /// `ls-metadata` validator test helper.
    fn temp_root() -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ls-docgen-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(dir.join(DEPENDENCY_DOCS_DIR)).expect("create managed dir");
        dir
    }

    #[test]
    fn check_detects_orphaned_committed_doc() {
        let root = temp_root();
        let rel = Path::new(DEPENDENCY_DOCS_DIR).join("index.md");

        // One rendered file, written to disk so it matches …
        let mut files: BTreeMap<PathBuf, String> = BTreeMap::new();
        files.insert(rel.clone(), "rendered\n".to_string());
        write_docs(&root, &files).expect("write");

        // … plus a stale orphan the generator no longer produces.
        std::fs::write(
            root.join(DEPENDENCY_DOCS_DIR).join("removed_tr.md"),
            "stale",
        )
        .expect("orphan");

        let drifted = check_docs(&root, &files);
        assert_eq!(
            drifted,
            vec![Path::new(DEPENDENCY_DOCS_DIR).join("removed_tr.md")],
            "the orphan must be reported as drift; the matching rendered file must not"
        );

        std::fs::remove_dir_all(&root).ok();
    }
}
