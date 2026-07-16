//! The machine-readable candidate pre-register (U4, R1; KTD2/KTD3).
//!
//! A candidate is the frozen input to one merit-bearing turn: the gate
//! thresholds, the identities (path + content hash) of its diagnostic script and
//! independent twin, the flip parameter+value (or a sweep's enumerated leg set),
//! and the KEEP-rule anchor — all in ONE machine-readable place, so a tool never
//! reads a threshold from prose that a later edit could soften unnoticed. Frozen
//! inputs live in a git-tracked home (`candidates/<slug>/`), departing from the
//! gitignored `data/` convention, so commit history is the freeze evidence R2
//! relies on.
//!
//! **Freeze discipline (KTD2):** the frozen-input set is `candidate.json` plus the
//! declared diagnostic and twin files — never command-written outputs. The
//! `gate-verdict.json` the diagnose stage writes is explicitly excluded from both
//! the git-dirty check and the freeze-commit lookup, so a GO written earlier in
//! an invocation chain is reusable uncommitted. Git is shelled out to (a
//! dev-tooling-scoped precedent — no `git2`/`gix` dependency enters the pinned
//! workspace); every spawn pins `-C <repo-root>` derived from the candidate path.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::artifacts::manifest::hash_bytes;

/// The current candidate-schema version.
pub const CANDIDATE_SCHEMA_VERSION: u32 = 1;

/// The candidate.json file name in a candidate dir.
pub const CANDIDATE_FILE: &str = "candidate.json";

/// The gate-verdict file the diagnose stage writes into the candidate dir. It is
/// a command OUTPUT, so it is excluded from the freeze discipline (KTD2).
pub const GATE_VERDICT_FILE: &str = "gate-verdict.json";

/// The Phase-A class of a candidate (R4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PhaseAClass {
    /// A bespoke diagnostic script + independent twin (the normal path): both are
    /// required, and diagnose runs and bit-compares their canonical readings.
    Bespoke,
    /// An independent-signal lever declaring a minimal Phase-A
    /// (freshness/reconcile-only) in place of a bespoke diagnostic — no
    /// script/twin required.
    Minimal,
}

/// One declared reading key's tolerance + rounding precision (KTD3). Both fields
/// are required — a declared key without a tolerance is a schema-incomplete
/// candidate (the twin comparison would have no bound).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadingSpec {
    /// The per-reading absolute tolerance the twin comparison allows.
    pub tolerance: f64,
    /// The decimal precision the readings artifact is rounded to.
    pub precision: u32,
}

/// A diagnostic-script declaration (KTD3): the argv to run (interpreter-agnostic;
/// the wrapper appends the output path), the file whose content hash freezes it,
/// and its SHA-256 at freeze time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScriptDecl {
    /// The argv to run. The wrapper appends the readings output path as a final arg.
    pub argv: Vec<String>,
    /// The script file, relative to the candidate dir, whose bytes are frozen.
    pub file: String,
    /// SHA-256 hex of the script file's bytes at freeze time.
    pub content_hash: String,
}

/// A threshold comparator on an agreed reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Comparator {
    /// reading ≥ value.
    Ge,
    /// reading ≤ value.
    Le,
    /// reading > value.
    Gt,
    /// reading < value.
    Lt,
}

impl Comparator {
    /// Whether `reading` satisfies `self value`.
    pub fn passes(&self, reading: f64, value: f64) -> bool {
        match self {
            Comparator::Ge => reading >= value,
            Comparator::Le => reading <= value,
            Comparator::Gt => reading > value,
            Comparator::Lt => reading < value,
        }
    }
}

/// One frozen gate threshold: an agreed reading must satisfy `comparator value`
/// for the candidate to GO.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Threshold {
    /// The reading key this threshold gates.
    pub reading: String,
    /// The comparator.
    pub comparator: Comparator,
    /// The frozen bound.
    pub value: f64,
}

/// The machine-readable candidate pre-register (R1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    /// Schema version.
    pub schema_version: u32,
    /// The candidate slug (matches its dir name).
    pub slug: String,
    /// The lever family (e.g. `class-b`).
    pub family: String,
    /// The Phase-A class.
    pub phase_a: PhaseAClass,
    /// The flip parameter (single-flip candidate). Mutually exclusive with a
    /// sweep leg set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flip_param: Option<String>,
    /// The flip value (single-flip candidate).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flip_value: Option<f64>,
    /// The sweep parameter (sweep candidate). Mutually exclusive with a single flip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sweep_param: Option<String>,
    /// The enumerated sweep legs — each leg's flip matches the same GO (R4).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sweep_legs: Vec<f64>,
    /// The bespoke diagnostic script (required for `bespoke`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<ScriptDecl>,
    /// The independent twin script (required for `bespoke`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub twin: Option<ScriptDecl>,
    /// The declared reading keys, each with a per-reading tolerance + precision.
    #[serde(default)]
    pub readings: BTreeMap<String, ReadingSpec>,
    /// The frozen gate thresholds (all must pass for a GO).
    #[serde(default)]
    pub thresholds: Vec<Threshold>,
    /// The KEEP-rule anchor (mirrors the human pre-register's KEEP crux). Recorded
    /// in verdicts; the flip evaluation runs the existing KEEP logic, not this text.
    pub keep_anchor: String,
}

impl Candidate {
    /// Whether a proposed flip `(param, value)` matches this candidate's
    /// declaration: the single flip param/value, or a leg in the enumerated sweep
    /// set (float-tolerant on the value).
    pub fn flip_matches(&self, param: &str, value: f64) -> bool {
        if let (Some(p), Some(v)) = (&self.flip_param, self.flip_value) {
            return p == param && (v - value).abs() <= 1e-9;
        }
        if let Some(p) = &self.sweep_param {
            return p == param && self.sweep_legs.iter().any(|leg| (leg - value).abs() <= 1e-9);
        }
        false
    }
}

/// A loaded candidate: its parsed values, the pre-register content hash records
/// cite (hash of the raw `candidate.json` bytes, KTD3), and its dir.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedCandidate {
    /// The parsed candidate.
    pub values: Candidate,
    /// SHA-256 hex of the raw `candidate.json` bytes — the freeze citation.
    pub content_hash: String,
    /// The candidate directory.
    pub dir: PathBuf,
}

impl LoadedCandidate {
    /// The frozen-input files (absolute paths): `candidate.json` + the declared
    /// diagnostic and twin scripts. Never `gate-verdict.json` (a command output).
    pub fn frozen_inputs(&self) -> Vec<PathBuf> {
        let mut inputs = vec![self.dir.join(CANDIDATE_FILE)];
        for decl in [&self.values.diagnostic, &self.values.twin].into_iter().flatten() {
            inputs.push(self.dir.join(&decl.file));
        }
        inputs
    }
}

/// Load and validate a candidate from its directory (R1). Verifies the schema,
/// the flip-target exclusivity, the Phase-A class's script requirements, and each
/// declared script's content hash against the file on disk; computes the
/// pre-register content hash.
///
/// # Errors
///
/// On a missing/malformed `candidate.json`, a wrong schema version, a flip target
/// that is neither/both single+sweep, a bespoke candidate missing its scripts, a
/// threshold referencing an undeclared reading, or a script whose bytes no longer
/// match its declared hash.
pub fn load(candidate_dir: &Path) -> anyhow::Result<LoadedCandidate> {
    let path = candidate_dir.join(CANDIDATE_FILE);
    let bytes = std::fs::read(&path)
        .map_err(|e| anyhow::anyhow!("reading candidate {}: {e}", path.display()))?;
    let content_hash = hash_bytes(&bytes);
    let values: Candidate = serde_json::from_slice(&bytes)
        .map_err(|e| anyhow::anyhow!("parsing candidate {}: {e}", path.display()))?;

    if values.schema_version != CANDIDATE_SCHEMA_VERSION {
        anyhow::bail!(
            "candidate {}: unsupported schema version {} (this build reads {})",
            values.slug,
            values.schema_version,
            CANDIDATE_SCHEMA_VERSION
        );
    }

    // Flip-target exclusivity: exactly one of a single flip or a sweep leg set.
    let has_single = values.flip_param.is_some() || values.flip_value.is_some();
    let has_sweep = values.sweep_param.is_some() || !values.sweep_legs.is_empty();
    match (has_single, has_sweep) {
        (true, true) => anyhow::bail!(
            "candidate {}: declares BOTH a single flip and a sweep leg set — exactly one is allowed",
            values.slug
        ),
        (false, false) => anyhow::bail!(
            "candidate {}: declares neither a flip param+value nor a sweep leg set",
            values.slug
        ),
        (true, false) => {
            if values.flip_param.is_none() || values.flip_value.is_none() {
                anyhow::bail!(
                    "candidate {}: a single flip needs both flip_param and flip_value",
                    values.slug
                );
            }
        }
        (false, true) => {
            if values.sweep_param.is_none() || values.sweep_legs.is_empty() {
                anyhow::bail!(
                    "candidate {}: a sweep needs both sweep_param and a non-empty sweep_legs",
                    values.slug
                );
            }
        }
    }

    // Phase-A class script requirements.
    if values.phase_a == PhaseAClass::Bespoke
        && (values.diagnostic.is_none() || values.twin.is_none())
    {
        anyhow::bail!(
            "candidate {}: a bespoke Phase-A class requires both a diagnostic and a twin",
            values.slug
        );
    }

    // Thresholds must reference declared reading keys (schema completeness).
    for t in &values.thresholds {
        if !values.readings.contains_key(&t.reading) {
            anyhow::bail!(
                "candidate {}: threshold references undeclared reading key '{}'",
                values.slug,
                t.reading
            );
        }
    }

    // Verify each declared script's content hash against the file on disk.
    for (label, decl) in
        [("diagnostic", &values.diagnostic), ("twin", &values.twin)]
    {
        if let Some(decl) = decl {
            let script_path = candidate_dir.join(&decl.file);
            let script_bytes = std::fs::read(&script_path).map_err(|e| {
                anyhow::anyhow!("reading {label} script {}: {e}", script_path.display())
            })?;
            let actual = hash_bytes(&script_bytes);
            if actual != decl.content_hash {
                anyhow::bail!(
                    "candidate {}: {label} script {} content hash mismatch — the file changed \
                     after it was declared (frozen {}, on disk {})",
                    values.slug,
                    decl.file,
                    decl.content_hash,
                    actual
                );
            }
        }
    }

    Ok(LoadedCandidate { values, content_hash, dir: candidate_dir.to_path_buf() })
}

/// The result of a freeze check: the commit that last touched the frozen inputs.
#[derive(Debug, Clone, PartialEq)]
pub struct FreezeStatus {
    /// The commit hash R2 records as freeze evidence, or `None` if the frozen
    /// inputs are not yet committed (a fresh candidate committed with its turn).
    pub commit: Option<String>,
}

/// Refuse if any frozen input is git-dirty; otherwise return the freeze commit
/// (KTD2). Shells `git -C <repo-root>` with the repo root derived from the
/// candidate path. `gate-verdict.json` is never a frozen input, so a GO written
/// uncommitted does not trip the dirty check.
///
/// # Errors
///
/// When the repo root cannot be found, a `git` spawn fails, or a frozen input is
/// git-dirty.
pub fn freeze_check(loaded: &LoadedCandidate) -> anyhow::Result<FreezeStatus> {
    let root = repo_root_of(&loaded.dir)?;
    let inputs = loaded.frozen_inputs();

    // Dirty check: `git status --porcelain -- <inputs>`. Any output → dirty.
    let mut status = Command::new("git");
    status.arg("-C").arg(&root).args(["status", "--porcelain", "--"]);
    for p in &inputs {
        status.arg(p);
    }
    let status_out = status
        .output()
        .map_err(|e| anyhow::anyhow!("spawning `git status` for the freeze check: {e}"))?;
    if !status_out.status.success() {
        anyhow::bail!(
            "`git status` failed for the freeze check: {}",
            String::from_utf8_lossy(&status_out.stderr).trim()
        );
    }
    let dirty = String::from_utf8_lossy(&status_out.stdout);
    if !dirty.trim().is_empty() {
        anyhow::bail!(
            "candidate {} has git-dirty frozen inputs — commit them so history is the freeze \
             evidence (R2), then re-run:\n{}",
            loaded.values.slug,
            dirty.trim()
        );
    }

    // Freeze commit: `git log -1 --format=%H -- <inputs>`. Empty output → not yet
    // committed (a fresh candidate committed alongside its turn).
    let mut log = Command::new("git");
    log.arg("-C").arg(&root).args(["log", "-1", "--format=%H", "--"]);
    for p in &inputs {
        log.arg(p);
    }
    let log_out = log
        .output()
        .map_err(|e| anyhow::anyhow!("spawning `git log` for the freeze commit: {e}"))?;
    if !log_out.status.success() {
        anyhow::bail!(
            "`git log` failed for the freeze commit: {}",
            String::from_utf8_lossy(&log_out.stderr).trim()
        );
    }
    let commit = String::from_utf8_lossy(&log_out.stdout).trim().to_string();
    Ok(FreezeStatus { commit: if commit.is_empty() { None } else { Some(commit) } })
}

/// Walk up from `start` to the nearest directory containing `.git` (dir or file,
/// so worktrees resolve too).
fn repo_root_of(start: &Path) -> anyhow::Result<PathBuf> {
    let mut dir = start
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("resolving {}: {e}", start.display()))?;
    loop {
        if dir.join(".git").exists() {
            return Ok(dir);
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => anyhow::bail!("no git repo found above {}", start.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Author a candidate dir with a diagnostic + twin whose content hashes match.
    fn write_candidate(dir: &Path, mutate: impl FnOnce(&mut serde_json::Value)) {
        std::fs::create_dir_all(dir).unwrap();
        let diag = "print('diag')\n";
        let twin = "print('twin')\n";
        std::fs::write(dir.join("diag.py"), diag).unwrap();
        std::fs::write(dir.join("twin.py"), twin).unwrap();
        let mut v = serde_json::json!({
            "schema_version": 1,
            "slug": "example",
            "family": "class-b",
            "phase_a": "bespoke",
            "flip_param": "ratio_atr_alpha",
            "flip_value": 0.5,
            "diagnostic": { "argv": ["python3", "diag.py"], "file": "diag.py",
                "content_hash": hash_bytes(diag.as_bytes()) },
            "twin": { "argv": ["python3", "twin.py"], "file": "twin.py",
                "content_hash": hash_bytes(twin.as_bytes()) },
            "readings": { "collinearity_r": { "tolerance": 0.01, "precision": 4 } },
            "thresholds": [ { "reading": "collinearity_r", "comparator": "lt", "value": 0.7 } ],
            "keep_anchor": "return-on-risk strict flip PASS"
        });
        mutate(&mut v);
        std::fs::write(dir.join(CANDIDATE_FILE), serde_json::to_string_pretty(&v).unwrap()).unwrap();
    }

    #[test]
    fn valid_candidate_loads_and_hash_is_stable_across_reserialize() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("example");
        write_candidate(&dir, |_| {});
        let a = load(&dir).unwrap();
        let b = load(&dir).unwrap();
        assert_eq!(a.content_hash, b.content_hash, "content hash is stable");
        assert_eq!(a.values.slug, "example");
        assert!(a.values.flip_matches("ratio_atr_alpha", 0.5));
        assert!(!a.values.flip_matches("ratio_atr_alpha", 0.4));
    }

    #[test]
    fn edited_script_after_declaration_is_a_hash_mismatch_naming_the_file() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("example");
        write_candidate(&dir, |_| {});
        // Edit the twin after declaration.
        std::fs::write(dir.join("twin.py"), "print('twin edited')\n").unwrap();
        let err = load(&dir).unwrap_err();
        assert!(err.to_string().contains("twin.py"), "names the changed file: {err}");
        assert!(err.to_string().contains("content hash mismatch"), "{err}");
    }

    #[test]
    fn missing_tolerance_for_a_declared_reading_is_a_load_error() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("example");
        write_candidate(&dir, |v| {
            v["readings"]["collinearity_r"] = serde_json::json!({ "precision": 4 }); // no tolerance
        });
        assert!(load(&dir).is_err(), "a reading key without a tolerance fails to load");
    }

    #[test]
    fn both_flip_and_sweep_is_a_load_error_but_a_sweep_alone_loads() {
        let tmp = TempDir::new().unwrap();
        // Both → error.
        let dir = tmp.path().join("both");
        write_candidate(&dir, |v| {
            v["sweep_param"] = serde_json::json!("ratio_atr_alpha");
            v["sweep_legs"] = serde_json::json!([0.3, 0.5, 0.7]);
        });
        let err = load(&dir).unwrap_err();
        assert!(err.to_string().contains("BOTH"), "{err}");

        // Sweep alone → loads, and each leg flip-matches.
        let sdir = tmp.path().join("sweep");
        write_candidate(&sdir, |v| {
            let o = v.as_object_mut().unwrap();
            o.remove("flip_param");
            o.remove("flip_value");
            o.insert("sweep_param".into(), serde_json::json!("ratio_atr_alpha"));
            o.insert("sweep_legs".into(), serde_json::json!([0.3, 0.5, 0.7]));
        });
        let loaded = load(&sdir).unwrap();
        assert!(loaded.values.flip_matches("ratio_atr_alpha", 0.3));
        assert!(loaded.values.flip_matches("ratio_atr_alpha", 0.7));
        assert!(!loaded.values.flip_matches("ratio_atr_alpha", 0.4), "an undeclared leg does not match");
    }

    #[test]
    fn git_clean_returns_a_commit_and_git_dirty_refuses() {
        // A tempdir git repo: commit the candidate, freeze_check clean; then edit
        // a frozen input, freeze_check refuses.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let dir = root.join("candidates/example");
        write_candidate(&dir, |_| {});

        let git = |args: &[&str]| {
            let out = Command::new("git").arg("-C").arg(root).args(args).output().unwrap();
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "freeze"]);

        let loaded = load(&dir).unwrap();
        let clean = freeze_check(&loaded).unwrap();
        assert!(clean.commit.is_some(), "a committed candidate has a freeze commit");

        // Dirty a frozen input.
        std::fs::write(dir.join(CANDIDATE_FILE), {
            let mut s = std::fs::read_to_string(dir.join(CANDIDATE_FILE)).unwrap();
            s.push('\n');
            s
        })
        .unwrap();
        let err = freeze_check(&loaded).unwrap_err();
        assert!(err.to_string().contains("git-dirty"), "dirty frozen input refuses: {err}");
    }

    #[test]
    fn gate_verdict_output_is_not_a_frozen_input() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("example");
        write_candidate(&dir, |_| {});
        let loaded = load(&dir).unwrap();
        let inputs = loaded.frozen_inputs();
        assert!(
            !inputs.iter().any(|p| p.ends_with(GATE_VERDICT_FILE)),
            "gate-verdict.json is a command output, never frozen: {inputs:?}"
        );
        assert!(inputs.iter().any(|p| p.ends_with(CANDIDATE_FILE)));
        assert!(inputs.iter().any(|p| p.ends_with("diag.py")));
        assert!(inputs.iter().any(|p| p.ends_with("twin.py")));
    }
}
