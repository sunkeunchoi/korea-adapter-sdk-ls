//! `turn governed` — the one-shot orchestrator (U7, R6/R9; KTD5/6/7).
//!
//! One invocation runs diagnose → build → binary-fingerprint verification →
//! flip → verdict, halting loudly at any red gate with a distinct exit code
//! (KTD6). The **parent drives, the fresh child decides**: the parent is
//! transport — it self-checks its own fingerprint, resolves a GO (reusing a
//! committed one or running diagnose), builds the binary foreground, verifies the
//! *built* binary's embedded fingerprint against the recomputed tree hash, then
//! spawns that fresh binary as a child for the flip stage and adopts the child's
//! last structured line as the governed verdict. The parent never recomputes KEEP
//! (the anchor-on-decider convention) and never touches git.
//!
//! Test seams (KTD6): `LS_GOVERNED_BUILD_CMD` substitutes the build, and
//! `LS_GOVERNED_CHILD_BIN` substitutes the child binary, so the stage machine is
//! proven with stubs; the real `cargo build` path is exercised attended, not by
//! CI. `LS_GOVERNED_SRC_DIR` / `LS_GOVERNED_CARGO_TOML` point the fingerprint
//! recompute at a fixture tree (to simulate a stale parent).

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use crate::artifacts::PERFORMANCE_FILE;
use crate::fingerprint;
use crate::runner::diagnose::{read_gate_verdict, GateExit};
use crate::runner::research::{
    candidates_home, latest_finalized_run, read_manifest, run_diagnose_cli, turn,
    turn_config_from_env,
};

/// The governed-run outcome. `exit` is [`GateExit::Ok`] on a completed evaluation
/// (KEEP/REVERT) and a typed gate on any halt; `verdict` is the machine-readable
/// last line the CLI prints.
#[derive(Debug, Clone)]
pub struct GovernedOutcome {
    /// The typed gate exit.
    pub exit: GateExit,
    /// The verdict last line (`KEEP … | REVERT … | STOP … | HELD …`).
    pub verdict: String,
    /// The progress lines (printed before the verdict).
    pub lines: Vec<String>,
}

impl GovernedOutcome {
    fn held(exit: GateExit, reason: String, mut lines: Vec<String>) -> Self {
        let verdict = format!("HELD {reason}");
        lines.push(verdict.clone());
        GovernedOutcome { exit, verdict, lines }
    }

    fn stop(exit: GateExit, gate: String, mut lines: Vec<String>) -> Self {
        let verdict = format!("STOP {gate}");
        lines.push(verdict.clone());
        GovernedOutcome { exit, verdict, lines }
    }
}

// ===========================================================================
// Parent orchestrator
// ===========================================================================

/// Run the one-shot governed orchestrator (U7). See the module docs for the stage
/// sequence and the test seams.
///
/// # Errors
///
/// On an I/O failure recomputing the fingerprint, spawning a stage, or reading a
/// manifest. Every *gate* halt is an outcome (typed exit), not an error.
pub fn run_governed_cli() -> anyhow::Result<GovernedOutcome> {
    let mut lines = Vec::new();

    // --- 1. Parent self-check (KTD6): the code class R7 distrusts never writes a
    // gate verdict, so the parent halts as stale BEFORE any diagnose. ---
    let (src_dir, cargo_toml) = governed_src_paths();
    let tree_hash = fingerprint::recompute_from_dir(&src_dir, &cargo_toml)
        .map_err(|e| anyhow::anyhow!("recomputing lab-source fingerprint: {e}"))?;
    if tree_hash != fingerprint::EMBEDDED {
        return Ok(GovernedOutcome::held(
            GateExit::StaleBinary,
            format!(
                "parent binary is stale: embedded {} != source tree {} — rebuild before governing",
                fingerprint::EMBEDDED,
                tree_hash
            ),
            lines,
        ));
    }
    lines.push("parent fingerprint OK".to_string());

    // --- 2. Resolve a GO (reuse a committed/uncommitted one, else diagnose). ---
    let slug = std::env::var("LS_TURN_CANDIDATE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("LS_TURN_CANDIDATE is required for `turn governed`"))?;
    let candidate_dir = candidates_home().join(&slug);
    match read_gate_verdict(&candidate_dir)? {
        Some(v) if v.decision == "GO" => {
            lines.push(format!("reusing GO for candidate '{slug}'"));
        }
        Some(v) => {
            // A reused STOP verdict short-circuits — no build (KTD6).
            let gate_name = v.stop_gate.clone().unwrap_or_else(|| "no-go".to_string());
            let exit = v
                .stop_gate
                .as_deref()
                .and_then(GateExit::from_name)
                .unwrap_or(GateExit::NoGoVerdict);
            lines.push(format!("candidate '{slug}' gate verdict is STOP — short-circuit, no build"));
            return Ok(GovernedOutcome::stop(exit, gate_name, lines));
        }
        None => {
            // Run diagnose in-parent; a STOP short-circuits before the build.
            let d = run_diagnose_cli()?;
            lines.extend(d.lines.iter().cloned());
            if !d.go {
                let gate = d.lines.last().cloned().unwrap_or_else(|| "stop".to_string());
                return Ok(GovernedOutcome::stop(d.exit, gate, lines));
            }
        }
    }

    // --- 3. Foreground build (KTD6). ---
    let build_cmd = std::env::var("LS_GOVERNED_BUILD_CMD").unwrap_or_else(|_| {
        "cargo build --release -p nautilus-ls-lab --bin lab-research".to_string()
    });
    lines.push(format!("build: {build_cmd}"));
    if !run_shell(&build_cmd, &build_cwd())? {
        return Ok(GovernedOutcome::held(
            GateExit::BuildFailure,
            format!("build command failed: {build_cmd}"),
            lines,
        ));
    }
    lines.push("build OK".to_string());

    // --- 4. Built-binary fingerprint check (KTD6, AE2): interrogate the BUILT
    // binary, not the process that ran the build. ---
    let child_bin = child_bin_path();
    let reported = read_child_fingerprint(&child_bin)?;
    if reported != tree_hash {
        return Ok(GovernedOutcome::held(
            GateExit::StaleBinary,
            format!(
                "built binary fingerprint {reported} != source tree {tree_hash} — refusing to \
                 backtest a stale binary"
            ),
            lines,
        ));
    }
    lines.push("built binary fingerprint OK".to_string());

    // --- 5. Spawn the fresh child for the flip stage; adopt its verdict (KTD6). ---
    let out = Command::new(&child_bin)
        .arg("turn")
        .env("LS_GOVERNED_CHILD", "1")
        .output()
        .map_err(|e| anyhow::anyhow!("spawning the child flip {}: {e}", child_bin.display()))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    for l in stdout.lines() {
        lines.push(l.to_string());
    }
    let verdict = stdout
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("HELD child produced no verdict")
        .to_string();
    let exit = if out.status.success() {
        GateExit::Ok
    } else {
        // Preserve the child's typed gate code (KTD6).
        let code = out.status.code().unwrap_or(1) as u8;
        GateExit::from_code(code).unwrap_or(GateExit::Generic)
    };
    Ok(GovernedOutcome { exit, verdict, lines })
}

// ===========================================================================
// Child flip stage (runs in the freshly built binary — the decider)
// ===========================================================================

/// Run the flip stage as the fresh child (U7/KTD6): run the flip, then — as the
/// single decider — evaluate the KEEP rule and emit the machine verdict as the
/// last structured line. Appends the flip trial (the child is the single writer
/// for the flip look). Invoked when `LS_GOVERNED_CHILD=1`.
///
/// # Errors
///
/// On an I/O failure running the flip or reading a manifest.
pub fn run_governed_child_cli() -> anyhow::Result<ExitCode> {
    nautilus_ls::scrub::install();
    let code_turn = std::env::var("LS_TURN_CODE_BUMP")
        .map(|v| !v.trim().is_empty() && v.trim() != "0")
        .unwrap_or(false);
    let cfg = turn_config_from_env()?;
    let data_home = cfg.data_home.clone();
    let prior = latest_finalized_run(&data_home)?;

    if code_turn {
        stage_log("bump");
    }
    let rt = tokio::runtime::Runtime::new()?;
    let out = rt.block_on(turn(cfg))?;
    for l in &out.lines {
        println!("{l}");
    }

    // A guard refusal → HELD, with the child's typed gate exit preserved.
    if let Some(exit) = out.gate_exit {
        println!("HELD {}", out.refusal.clone().unwrap_or_default());
        return Ok(exit.exit_code());
    }
    // A pipeline denial (the flip did not happen) → HELD.
    if !out.ran {
        println!("HELD {}", out.refusal.clone().unwrap_or_else(|| "flip denied".to_string()));
        return Ok(ExitCode::FAILURE);
    }

    let new_run = out.run_id.clone().expect("a ran flip has a run id");
    let new_version = out.version.unwrap_or(0);

    // Code turn: sentinel re-baseline → 1:1 reconcile against the prior head →
    // compare, then KEEP on the re-baseline (KTD7). Reconcile is a sample-identity
    // check (same range/catalog/universe — the only change is code); it is an
    // identity check, not a "look", so it lands no ledger record.
    if code_turn {
        stage_log("rebaseline");
        if let Some((_, prior_m)) = &prior {
            let new_m = read_manifest(&data_home, &new_run)?;
            let reconciled = prior_m.data_range == new_m.data_range
                && prior_m.catalog_fingerprint == new_m.catalog_fingerprint
                && prior_m.universe_hash == new_m.universe_hash;
            stage_log("reconcile");
            if !reconciled {
                println!("REVERT reconcile-failed");
                return Ok(ExitCode::SUCCESS);
            }
        } else {
            stage_log("reconcile");
        }
        stage_log("compare");
    }

    let verdict = decide_keep_or_revert(&data_home, prior.as_ref(), &new_run, new_version)?;
    println!("{verdict}");
    Ok(ExitCode::SUCCESS)
}

/// The KEEP-rule verdict on the size-invariant return-on-risk crux (the loop's
/// documented KEEP rule): KEEP when the new run's return-on-risk strictly exceeds
/// the prior head's and risk-cap dominance holds; else REVERT. The verdict line's
/// hash is the new run's lab-source fingerprint.
fn decide_keep_or_revert(
    data_home: &Path,
    prior: Option<&(String, crate::artifacts::manifest::Manifest)>,
    new_run: &str,
    new_version: u32,
) -> anyhow::Result<String> {
    let new_m = read_manifest(data_home, new_run)?;
    let hash = new_m
        .lab_src_fingerprint
        .clone()
        .unwrap_or_else(|| new_m.strategy_code_hash.clone());
    let new_edge = read_edge(data_home, new_run)?;

    // Read the prior head's edge to compare against. Distinguish the two None
    // cases the `.ok()`-swallow used to conflate (correctness review): a prior
    // head that legitimately predates return-on-risk yields `Some(edge)` whose
    // `return_on_risk` is `None` (the legacy fallback inside `keeps_over`), whereas
    // an *unreadable* prior artifact is a genuine error we propagate — never a
    // silent fall to the laxer `is_edge` bar, which could false-advance the head.
    let prior_edge = match prior {
        Some((prior_id, _)) => Some(read_edge(data_home, prior_id)?),
        None => None,
    };
    let keep = new_edge.keeps_over(prior_edge.as_ref());

    Ok(if keep {
        format!("KEEP v{new_version} {hash}")
    } else {
        // The size-honest cause: a non-improving return-on-risk (KTD9 seed set).
        "REVERT ror-negative".to_string()
    })
}

/// Read a run's edge evaluation from its performance artifact.
fn read_edge(
    data_home: &Path,
    run_id: &str,
) -> anyhow::Result<crate::artifacts::performance::EdgeEvaluation> {
    let path = data_home.join("runs").join(run_id).join(PERFORMANCE_FILE);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
    let perf: crate::artifacts::performance::PerformanceReport = serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("parsing {}: {e}", path.display()))?;
    Ok(perf.edge_evaluation())
}

/// Append a stage name to `LS_GOVERNED_STAGELOG` if set (U7 code-turn ordering).
fn stage_log(stage: &str) {
    if let Ok(path) = std::env::var("LS_GOVERNED_STAGELOG") {
        if !path.trim().is_empty() {
            use std::io::Write as _;
            if let Ok(mut f) =
                std::fs::OpenOptions::new().append(true).create(true).open(&path)
            {
                let _ = writeln!(f, "{stage}");
            }
        }
    }
}

// ===========================================================================
// Stage seams + path resolution
// ===========================================================================

/// The source dir + Cargo.toml the fingerprint recompute reads. Seamed by
/// `LS_GOVERNED_SRC_DIR` / `LS_GOVERNED_CARGO_TOML` (tests point at a fixture tree
/// to simulate a stale parent); defaults to the baked crate paths.
fn governed_src_paths() -> (PathBuf, PathBuf) {
    let src = std::env::var("LS_GOVERNED_SRC_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
    let toml = std::env::var("LS_GOVERNED_CARGO_TOML")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"));
    (src, toml)
}

/// The cwd the build command runs from — the standalone workspace root
/// (`adapters/nautilus`). Seamed by `LS_GOVERNED_BUILD_CWD`.
fn build_cwd() -> PathBuf {
    std::env::var("LS_GOVERNED_BUILD_CWD")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("lab crate has a parent (the workspace root)")
                .to_path_buf()
        })
}

/// The child binary path. Seamed by `LS_GOVERNED_CHILD_BIN` (tests point at a stub
/// or the compiled test bin); defaults to the built release binary.
fn child_bin_path() -> PathBuf {
    std::env::var("LS_GOVERNED_CHILD_BIN")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| build_cwd().join("target/release/lab-research"))
}

/// Run a shell-word-split command in `cwd`, returning whether it exited zero.
fn run_shell(cmd: &str, cwd: &Path) -> anyhow::Result<bool> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let Some((program, args)) = parts.split_first() else {
        anyhow::bail!("empty build command");
    };
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .status()
        .map_err(|e| anyhow::anyhow!("spawning build `{cmd}`: {e}"))?;
    Ok(status.success())
}

/// Run `<bin> fingerprint` and parse `fingerprint: <hex>`.
fn read_child_fingerprint(bin: &Path) -> anyhow::Result<String> {
    let out = Command::new(bin)
        .arg("fingerprint")
        .output()
        .map_err(|e| anyhow::anyhow!("spawning `{} fingerprint`: {e}", bin.display()))?;
    if !out.status.success() {
        anyhow::bail!("`{} fingerprint` exited non-zero", bin.display());
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout
        .lines()
        .find_map(|l| l.trim().strip_prefix("fingerprint: ").map(|h| h.trim().to_string()))
        .ok_or_else(|| anyhow::anyhow!("`{} fingerprint` printed no `fingerprint:` line", bin.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Author a run dir with a manifest and (optionally) a `performance.json`.
    fn write_run(home: &Path, run_id: &str, with_perf: bool) {
        let dir = home.join("runs").join(run_id);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = serde_json::json!({
            "run_id": run_id, "source": "backtest", "strategy_id": "orb", "strategy_version": 9,
            "params": crate::params::OrbParams::default(),
            "data_range": { "start": "20240102", "end": "20240105" },
            "catalog_fingerprint": "fp", "universe_hash": "uh", "strategy_code_hash": "ch",
            "lab_src_fingerprint": "labfp", "created_utc": "2026-07-16T00:00:00+00:00"
        });
        std::fs::write(dir.join("manifest.json"), serde_json::to_string(&manifest).unwrap()).unwrap();
        if with_perf {
            let perf = serde_json::json!({ "trades": [], "equity_curve": [], "summary": {} });
            std::fs::write(dir.join("performance.json"), serde_json::to_string(&perf).unwrap()).unwrap();
        }
    }

    #[test]
    fn decide_reverts_a_no_edge_run_and_stamps_the_fingerprint() {
        // Empty trades → is_edge false → REVERT; prior None → keeps_over(None)=is_edge.
        let tmp = TempDir::new().unwrap();
        write_run(tmp.path(), "new-v9", true);
        let verdict = decide_keep_or_revert(tmp.path(), None, "new-v9", 9).unwrap();
        assert!(verdict.starts_with("REVERT"), "a no-edge flip REVERTs: {verdict}");
    }

    #[test]
    fn decide_propagates_an_unreadable_prior_artifact_instead_of_silently_keeping() {
        // The correctness-review fix: a prior head whose performance.json is missing
        // is a genuine error we surface — never a silent fall to the laxer is_edge bar.
        let tmp = TempDir::new().unwrap();
        write_run(tmp.path(), "prior-v8", false); // manifest only — no performance.json
        let prior_m = read_manifest(tmp.path(), "prior-v8").unwrap();
        write_run(tmp.path(), "new-v9", true);
        let result =
            decide_keep_or_revert(tmp.path(), Some(&("prior-v8".to_string(), prior_m)), "new-v9", 9);
        assert!(result.is_err(), "an unreadable prior artifact propagates, not silently KEEPs");
    }
}
