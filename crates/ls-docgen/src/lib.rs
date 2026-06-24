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
    validate_dir, CertificationPath, EvidenceRecord, InstrumentDomain, OwnerClass, Protocol,
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

/// Render a single implemented TR's Reference page. A Recommended TR carries its
/// full recommendation contract (R9); an implemented-but-not-recommended TR keeps
/// the not-recommended banner and the schemas/examples-deferred caveat.
fn render_reference_page(meta: &TrMetadata, evidence: Option<&EvidenceRecord>) -> String {
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
/// such as `CSPAT00601` is excluded from Reference while still appearing in the
/// Dependency Docs (R3). Each entry carries the "not yet recommended" banner
/// whenever `support.recommended == false` (R4).
pub fn render_reference_docs(
    trs: &BTreeMap<String, TrMetadata>,
    evidence: &BTreeMap<String, EvidenceRecord>,
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
            render_reference_page(meta, evidence.get(*tr_code)),
        );
    }
    files
}

/// High-level: render the full generated file set from a validated
/// [`ValidationReport`], keyed by repo-relative path.
pub fn render_all(report: &ValidationReport) -> BTreeMap<PathBuf, String> {
    let mut files = render_dependency_docs(&report.trs, &report.index);
    files.extend(render_reference_docs(&report.trs, &report.evidence));
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

    /// The tracked TRs in the slice. The original eight (`token`, `revoke`,
    /// `t1101`, `t1102`, `t8412`, `CSPAQ12200`, `S3_`, `CSPAT00601`) plus the 41
    /// read-only stock/sector TRs brought into tracked-only maintenance ownership
    /// (incl. the Wave A sector cluster t8424/t1511/t1514/t1516/t1485).
    const TRACKED_TRS: [&str; 85] = [
        "CCENQ10100",
        "CCENQ90200",
        "CFOAQ10100",
        "CFOBQ10500",
        "CSPAQ12200",
        "CSPAQ12300",
        "CSPAQ22200",
        "CSPAT00601",
        "S3_",
        "g3101",
        "g3102",
        "g3103",
        "g3104",
        "g3106",
        "g3190",
        "o3101",
        "o3105",
        "o3106",
        "o3121",
        "o3125",
        "o3126",
        "revoke",
        "t1101",
        "t1102",
        "t1403",
        "t1441",
        "t1452",
        "t1463",
        "t1466",
        "t1481",
        "t1482",
        "t1485",
        "t1489",
        "t1492",
        "t1511",
        "t1514",
        "t1516",
        "t1531",
        "t1537",
        "t1601",
        "t1615",
        "t1640",
        "t1662",
        "t1664",
        "t1825",
        "t1826",
        "t1852",
        "t1856",
        "t1859",
        "t1860",
        "t1866",
        "t1958",
        "t1964",
        "t1988",
        "t2106",
        "t2111",
        "t2112",
        "t2301",
        "t2522",
        "t3102",
        "t3320",
        "t3341",
        "t8401",
        "t8402",
        "t8403",
        "t8412",
        "t8424",
        "t8425",
        "t8426",
        "t8430",
        "t8431",
        "t8433",
        "t8434",
        "t8435",
        "t8436",
        "t8455",
        "t8460",
        "t8463",
        "t8467",
        "t9905",
        "t9907",
        "t9942",
        "t9943",
        "t9944",
        "token",
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
        assert!(page.contains("- Recommended: yes"));
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
        }
    }

    #[test]
    fn reference_covers_implemented_with_banner_and_omits_unimplemented() {
        let report = authored_report();
        let reference = render_reference_docs(&report.trs, &report.evidence);
        let dependency = render_dependency_docs(&report.trs, &report.index);

        // The still-unrecommended implemented TRs each carry the banner.
        let banner_trs = [
            "CFOAQ10100", "CFOBQ10500", "CSPAQ12300", "CSPAQ22200", "revoke", "t1403", "t1441",
            "t1452", "t1463",
            "t1466", "t1481", "t1482", "t1485", "t1489", "t1492", "t1511", "t1514", "t1516", "t1531",
            "t1537", "t1601",
            "t1615", "t1640", "t1662", "t1664", "t1825", "t1826", "t1859", "t1866", "t1958",
            "t2301", "t2522", "t3341", "t8401", "t8424", "t8425", "t8426", "t8433", "t8435",
            "t8467", "t9943", "t9944", "t8431", "t8436", "t9905", "t9907", "t9942",
            "t2111", "t2112", "t8402", "t8403", "t8434",
            "t1988", "t3320",
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

        // token, t1101, t1102, t8412, S3_, and CSPAQ12200 are Recommended TRs: each
        // still renders a Reference page (all stay implemented), but the banner is gone
        // now that they are promoted.
        for rec in ["token", "t1101", "t1102", "t8412", "S3_", "CSPAQ12200"] {
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
        assert_eq!(
            reference.len(),
            61,
            "index + the implemented reference pages"
        );

        // The tracked-but-unimplemented order TR is excluded from Reference …
        assert!(!reference.contains_key(Path::new("docs/reference/CSPAT00601.md")));
        let ref_index = &reference[Path::new("docs/reference/index.md")];
        assert!(!ref_index.contains("CSPAT00601"));

        // … but still appears in the Dependency Docs.
        assert!(dependency.contains_key(Path::new("docs/tr-dependencies/CSPAT00601.md")));
    }

    #[test]
    fn reference_banner_is_keyed_on_recommended_flag() {
        let mut trs: BTreeMap<String, TrMetadata> = BTreeMap::new();
        trs.insert("rec".to_string(), sample_meta("rec", true, true));
        trs.insert("notrec".to_string(), sample_meta("notrec", true, false));

        let reference = render_reference_docs(&trs, &BTreeMap::new());

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

        let reference = render_reference_docs(&trs, &BTreeMap::new());
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
        let reference = render_reference_docs(&trs, &evidence);
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
        let reference = render_reference_docs(&trs, &evidence);
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
        let reference = render_reference_docs(&trs, &evidence);
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
        let reference = render_reference_docs(&trs, &BTreeMap::new());
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
        let a = render_reference_docs(&trs, &evidence);
        let b = render_reference_docs(&trs, &evidence);
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
