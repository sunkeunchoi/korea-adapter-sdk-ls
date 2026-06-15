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

use ls_metadata::{validate_dir, TrIndex, TrMetadata, ValidationError, ValidationReport};

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
                write!(f, "unrecognized argument `{arg}` (expected no flag, or `--check`)")
            }
            DocgenError::MetadataInvalid(errors) => {
                writeln!(f, "metadata failed to validate ({} error(s)):", errors.len())?;
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

/// Low-level: render the TR Dependency Docs file set (index + per-TR pages),
/// keyed by repo-relative path. Takes the raw metadata map (and index) so tests
/// drive it from inline fixtures without touching disk.
///
/// Filled in U2; this scaffold returns an empty set so the crate compiles and
/// the binary runs without panicking.
pub fn render_dependency_docs(
    _trs: &BTreeMap<String, TrMetadata>,
    _index: &TrIndex,
) -> BTreeMap<PathBuf, String> {
    BTreeMap::new()
}

/// Low-level: render the SDK Reference Docs file set (implemented TRs only),
/// keyed by repo-relative path.
///
/// Filled in U3; this scaffold returns an empty set.
pub fn render_reference_docs(_trs: &BTreeMap<String, TrMetadata>) -> BTreeMap<PathBuf, String> {
    BTreeMap::new()
}

/// High-level: render the full generated file set from a validated
/// [`ValidationReport`], keyed by repo-relative path.
pub fn render_all(report: &ValidationReport) -> BTreeMap<PathBuf, String> {
    let mut files = render_dependency_docs(&report.trs, &report.index);
    files.extend(render_reference_docs(&report.trs));
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

/// Compare the rendered file set against the committed files under `root`.
///
/// Returns the drifted repo-relative paths (missing or differing), sorted. An
/// empty vec means the committed docs match the current metadata.
pub fn check_docs(root: &Path, files: &BTreeMap<PathBuf, String>) -> Vec<PathBuf> {
    let mut drifted: Vec<PathBuf> = Vec::new();
    for (rel, expected) in files {
        let path = root.join(rel);
        match std::fs::read_to_string(&path) {
            Ok(actual) if &actual == expected => {}
            _ => drifted.push(rel.clone()),
        }
    }
    drifted.sort();
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
}
