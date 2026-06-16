//! The thin `ls-trackers` CLI (U5): `api-drift fetch`, `api-drift check`, and
//! `api-drift promote --dry-run`, mapping findings to the tiered exit contract
//! (R17). Only `api-drift` subcommands are exposed (R20).
//!
//! All logic lives here so it is unit-testable; the binary
//! (`src/main.rs`) only maps the resolved exit code. Arg parsing, staged-run
//! storage, exit-code mapping, and `check --staged` are network-free; the live
//! fetch path is exercised only by the operator seed (U6).

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use ls_metadata::validate_dir;

use crate::api_drift::{compare, normalize_run, DriftReport, NormalizedRun};
use crate::fetch::{
    completeness_gate, FetchClient, GateOutcome, RawInventory, DEFAULT_TRUNCATION_PROPORTION,
};
use crate::stages::promote_targets;
use crate::types::{CodeSet, FetchReport, Manifest, TrShape};

/// The live LS Open API base URL (`api-drift fetch` / default `check`).
pub const LS_BASE_URL: &str = "https://openapi.ls-sec.co.kr";

/// The tiered exit contract (R17). The binary maps these to process exit codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// `0` — comparison completed; no finding crossed the gate threshold.
    Ok,
    /// `1` — at least one finding crossed the gate threshold (R17b).
    Gated,
    /// `2` — fetch, parse, baseline, staged-run, or internal error.
    Error,
}

impl Exit {
    pub fn code(self) -> u8 {
        match self {
            Exit::Ok => 0,
            Exit::Gated => 1,
            Exit::Error => 2,
        }
    }
}

/// A parsed `api-drift` subcommand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `seed` marks the staged code-set `provisional: true` for the one-time
    /// bootstrap seed (KTD-5); ordinary fetches stage a non-provisional run.
    Fetch {
        seed: bool,
    },
    Check {
        staged: Option<PathBuf>,
    },
    PromoteDryRun {
        staged: Option<PathBuf>,
    },
}

/// Filesystem locations, injected so tests drive everything over a tempdir.
#[derive(Debug, Clone)]
pub struct Paths {
    /// Committed bounded baseline (`crates/ls-trackers/baselines/api-drift`).
    pub baseline_dir: PathBuf,
    /// Staged-run root (`target/ls-trackers/api-drift`).
    pub run_root: PathBuf,
    /// Authored metadata directory (`metadata`).
    pub metadata_dir: PathBuf,
}

impl Paths {
    /// Repo-root-relative defaults used by the installed binary.
    pub fn defaults() -> Self {
        Paths {
            baseline_dir: PathBuf::from("crates/ls-trackers/baselines/api-drift"),
            run_root: PathBuf::from("target/ls-trackers/api-drift"),
            metadata_dir: PathBuf::from("metadata"),
        }
    }
}

/// Parse `api-drift <subcommand> [flags]`. Only `api-drift` is exposed (R20);
/// anything else is a usage error (mapped to exit `2`).
pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut it = args.into_iter();
    match it.next().as_deref() {
        Some("api-drift") => {}
        Some(other) => return Err(format!("unknown command `{other}` (expected `api-drift`)")),
        None => {
            return Err(
                "usage: ls-trackers api-drift <fetch|check|promote> [--staged DIR] [--dry-run]"
                    .to_string(),
            )
        }
    }
    let sub = it.next();
    let rest: Vec<String> = it.collect();
    match sub.as_deref() {
        Some("fetch") => {
            let mut seed = false;
            for arg in &rest {
                match arg.as_str() {
                    "--seed" => seed = true,
                    other => return Err(format!("unexpected argument `{other}`")),
                }
            }
            Ok(Command::Fetch { seed })
        }
        Some("check") => Ok(Command::Check {
            staged: parse_staged(&rest)?,
        }),
        Some("promote") => {
            if !rest.iter().any(|a| a == "--dry-run") {
                return Err(
                    "`promote` requires `--dry-run` (mutating promote is out of scope)".to_string(),
                );
            }
            let staged = parse_staged(
                &rest
                    .iter()
                    .filter(|a| *a != "--dry-run")
                    .cloned()
                    .collect::<Vec<_>>(),
            )?;
            Ok(Command::PromoteDryRun { staged })
        }
        Some(other) => Err(format!("unknown api-drift subcommand `{other}`")),
        None => Err("usage: ls-trackers api-drift <fetch|check|promote --dry-run>".to_string()),
    }
}

fn parse_staged(rest: &[String]) -> Result<Option<PathBuf>, String> {
    let mut staged = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--staged" => {
                let dir = rest
                    .get(i + 1)
                    .ok_or_else(|| "`--staged` requires a directory argument".to_string())?;
                staged = Some(PathBuf::from(dir));
                i += 2;
            }
            other => return Err(format!("unexpected argument `{other}`")),
        }
    }
    Ok(staged)
}

// ---------------------------------------------------------------------------
// Staged-run + baseline storage
// ---------------------------------------------------------------------------

const RAW_FILE: &str = "raw/ls-openapi-full.json";
const CODE_SET_FILE: &str = "code-set.json";
const MANIFEST_FILE: &str = "normalized/manifest.json";
const TRS_DIR: &str = "normalized/trs";
const FETCH_REPORT_FILE: &str = "fetch-report.json";

/// Write a full staged run to `run_dir` (AE1): raw evidence, code-set, manifest,
/// per-TR shapes, and the fetch-report. Deterministic, pretty JSON.
pub fn write_staged_run(
    run_dir: &Path,
    raw: &RawInventory,
    normalized: &NormalizedRun,
    report: &FetchReport,
) -> std::io::Result<()> {
    write_normalized(run_dir, normalized)?;
    fs::create_dir_all(run_dir.join("raw"))?;
    write_json(&run_dir.join(RAW_FILE), raw)?;
    write_json(&run_dir.join(FETCH_REPORT_FILE), report)?;
    Ok(())
}

/// Write the code-set + manifest + per-TR shapes (the layout shared by a staged
/// run and the committed baseline).
pub fn write_normalized(dir: &Path, normalized: &NormalizedRun) -> std::io::Result<()> {
    fs::create_dir_all(dir.join(TRS_DIR))?;
    write_json(&dir.join(CODE_SET_FILE), &normalized.code_set)?;
    write_json(&dir.join(MANIFEST_FILE), &normalized.manifest)?;
    for (code, shape) in &normalized.shapes {
        write_json(&dir.join(TRS_DIR).join(format!("{code}.json")), shape)?;
    }
    Ok(())
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)
}

/// Load a normalized run (code-set + manifest + per-TR shapes) from a staged-run
/// or committed-baseline directory. Missing or malformed files are an error
/// (exit `2`: baseline/staged-run error).
pub fn load_normalized(dir: &Path) -> Result<NormalizedRun, String> {
    let code_set: CodeSet = read_json(&dir.join(CODE_SET_FILE))?;
    let manifest: Manifest = read_json(&dir.join(MANIFEST_FILE))?;
    let mut shapes = BTreeMap::new();
    let trs_dir = dir.join(TRS_DIR);
    if trs_dir.is_dir() {
        let mut entries: Vec<PathBuf> = fs::read_dir(&trs_dir)
            .map_err(|e| format!("reading {}: {e}", trs_dir.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        entries.sort();
        for path in entries {
            let shape: TrShape = read_json(&path)?;
            shapes.insert(shape.tr_code.clone(), shape);
        }
    }
    Ok(NormalizedRun {
        code_set,
        manifest,
        shapes,
    })
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parsing {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// Orchestration
// ---------------------------------------------------------------------------

/// The maintained-TR code set (the keys of the authored metadata).
fn maintained_codes(paths: &Paths) -> Result<BTreeSet<String>, String> {
    Ok(load_metadata(paths)?.keys().cloned().collect())
}

/// The committed code-set size for the truncation gate's denominator, or `None`
/// on bootstrap (no committed baseline). A baseline that is *present but
/// unreadable* is a hard error — not silently `None`, which would disable the
/// truncation guard and let a mass-truncated fetch stage as complete.
fn committed_code_set_len(baseline_dir: &Path) -> Result<Option<usize>, String> {
    if baseline_dir.join(CODE_SET_FILE).exists() {
        let run = load_normalized(baseline_dir)
            .map_err(|e| format!("committed baseline unreadable: {e}"))?;
        Ok(Some(run.code_set.len()))
    } else {
        Ok(None)
    }
}

fn load_metadata(paths: &Paths) -> Result<BTreeMap<String, ls_metadata::TrMetadata>, String> {
    validate_dir(&paths.metadata_dir)
        .map(|r| r.trs)
        .map_err(|e| format!("metadata error: {e:?}"))
}

/// Run `api-drift check`: load the committed baseline, obtain the staged run
/// (from `--staged DIR` or a live fetch), compare, and return the report. The
/// caller maps the report's gate flag to the exit code.
pub fn run_check(paths: &Paths, staged: Option<&Path>) -> Result<DriftReport, String> {
    let committed = load_normalized(&paths.baseline_dir)
        .map_err(|e| format!("committed baseline unavailable: {e}"))?;
    let trs = load_metadata(paths)?;
    let staged_run = match staged {
        Some(dir) => load_normalized(dir).map_err(|e| format!("staged run unavailable: {e}"))?,
        None => fetch_and_stage(paths, /* provisional */ false)?.1,
    };
    // Refuse to compare across normalizer versions — the committed
    // description-hashes were computed under different rules, so a mismatch would
    // emit spurious findings instead of a clean diff. Re-baseline first (exit 2).
    if committed.manifest.normalizer_version != staged_run.manifest.normalizer_version {
        return Err(format!(
            "normalizer version mismatch: committed v{} vs staged v{} — re-baseline first",
            committed.manifest.normalizer_version, staged_run.manifest.normalizer_version
        ));
    }
    Ok(compare(&committed, &staged_run, &trs))
}

/// Live-fetch the inventory, apply the split completeness gate, and write a
/// timestamped staged run. Returns the run directory and its normalized run.
/// This is the only network path; it is not exercised under `cargo test`.
pub fn fetch_and_stage(
    paths: &Paths,
    provisional: bool,
) -> Result<(PathBuf, NormalizedRun), String> {
    let maintained = maintained_codes(paths)?;
    let committed_len = committed_code_set_len(&paths.baseline_dir)?;

    let client = FetchClient::new(LS_BASE_URL).map_err(|e| e.to_string())?;
    let raw = client.fetch_full_inventory().map_err(|e| e.to_string())?;
    let fetched = raw.code_set(false).len();

    // Menu parsed (fetch succeeded); apply the truncation guard (KTD-3).
    let gate = completeness_gate(true, fetched, committed_len, DEFAULT_TRUNCATION_PROPORTION);
    let run_dir = paths.run_root.join("runs").join(timestamp());
    if !gate.passed() {
        let report = failure_report(fetched, committed_len, &gate);
        // Record the failure report even on the abort path so the operator sees
        // why the fetch was rejected; nothing else is staged.
        let _ = write_json(&run_dir.join(FETCH_REPORT_FILE), &report);
        return Err(format!("fetch incomplete: {gate:?}"));
    }

    // A group whose protocol endpoint failed has no `group_id`; a wholesale
    // outage would otherwise stage `ok: true` and surface as spurious endpoint/
    // rate drift at compare time. Surface the degradation at fetch time.
    let facts_degraded_groups = raw.groups.iter().filter(|g| g.group_id.is_none()).count();
    if facts_degraded_groups > 0 {
        eprintln!(
            "warning: {facts_degraded_groups} of {} group(s) returned no protocol/rate facts; \
             endpoint/rate fields may be degraded for this run",
            raw.groups.len()
        );
    }

    let normalized = normalize_run(&raw, &maintained, provisional);
    let report = FetchReport {
        ok: true,
        fetched_count: fetched,
        committed_code_set_len: committed_len,
        facts_degraded_groups,
        failure: None,
    };
    write_staged_run(&run_dir, &raw, &normalized, &report).map_err(|e| e.to_string())?;
    update_latest(paths, &run_dir)?;
    Ok((run_dir, normalized))
}

fn failure_report(fetched: usize, committed_len: Option<usize>, gate: &GateOutcome) -> FetchReport {
    FetchReport {
        ok: false,
        fetched_count: fetched,
        committed_code_set_len: committed_len,
        facts_degraded_groups: 0,
        failure: Some(format!("{gate:?}")),
    }
}

/// Write `latest.txt` pointing at the most recent run (a portable relative path).
fn update_latest(paths: &Paths, run_dir: &Path) -> Result<(), String> {
    let name = run_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let latest = paths.run_root.join("latest.txt");
    if let Some(parent) = latest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&latest, format!("runs/{name}\n")).map_err(|e| e.to_string())
}

fn timestamp() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H-%M-%SZ").to_string()
}

/// Map a `check`/`promote` result to the tiered exit (R17).
pub fn exit_for(result: &Result<DriftReport, String>) -> Exit {
    match result {
        Ok(report) if report.gates() => Exit::Gated,
        Ok(_) => Exit::Ok,
        Err(_) => Exit::Error,
    }
}

/// Drive a parsed command, printing human output to stdout/stderr and returning
/// the tiered exit. The binary calls this and maps [`Exit::code`].
pub fn dispatch(paths: &Paths, command: Command) -> Exit {
    match command {
        Command::Fetch { seed } => match fetch_and_stage(paths, seed) {
            Ok((dir, _)) => {
                println!("staged run written to {}", dir.display());
                Exit::Ok
            }
            Err(e) => {
                eprintln!("error: {e}");
                Exit::Error
            }
        },
        Command::Check { staged } => {
            let result = run_check(paths, staged.as_deref());
            match &result {
                Ok(report) => print_report(report),
                Err(e) => eprintln!("error: {e}"),
            }
            exit_for(&result)
        }
        Command::PromoteDryRun { staged } => {
            let result = run_check(paths, staged.as_deref());
            match &result {
                Ok(report) => {
                    let affected: BTreeSet<&str> =
                        report.findings.iter().map(|f| f.tr_code.as_str()).collect();
                    let targets = promote_targets(affected.iter().copied());
                    print_promote(&targets);
                    // A dry-run reports and writes nothing; it does not gate.
                    Exit::Ok
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    Exit::Error
                }
            }
        }
    }
}

fn print_report(report: &DriftReport) {
    if report.findings.is_empty() {
        println!("no drift findings.");
    } else {
        println!("{} finding(s):", report.findings.len());
        for f in &report.findings {
            let gate = if f.gates { "GATE" } else { "    " };
            let rename = f
                .possible_rename
                .as_deref()
                .map(|r| format!(" (possible rename? {r})"))
                .unwrap_or_default();
            println!(
                "  [{gate}] {} {} {:?}{rename}",
                f.severity, f.tr_code, f.change
            );
        }
    }
    let c = &report.coverage;
    println!(
        "coverage: {} upstream, {} metadata ({} implemented, {} tracked-only); \
         {} metadata-missing-upstream, {} upstream-missing-metadata",
        c.upstream_count,
        c.metadata_count,
        c.implemented_count,
        c.tracked_only_count,
        c.metadata_missing_upstream.len(),
        c.upstream_missing_metadata.len(),
    );
}

fn print_promote(targets: &crate::types::PromoteReport) {
    println!("promote --dry-run (writes nothing). A real promote would touch:");
    for f in &targets.baseline_files {
        println!("  baseline: {f}");
    }
    for f in &targets.metadata_fields {
        println!("  metadata: {f}");
    }
    for d in &targets.generated_docs {
        println!("  docs:     {d}");
    }
}

/// Entry point used by the binary: parse, dispatch, and resolve the exit code.
pub fn run_cli(args: impl IntoIterator<Item = String>) -> Exit {
    match parse_args(args) {
        Ok(command) => dispatch(&Paths::defaults(), command),
        Err(e) => {
            eprintln!("error: {e}");
            Exit::Error
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CodeSet, Manifest};

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_all_subcommands() {
        assert_eq!(
            parse_args(args(&["api-drift", "fetch"])).unwrap(),
            Command::Fetch { seed: false }
        );
        assert_eq!(
            parse_args(args(&["api-drift", "fetch", "--seed"])).unwrap(),
            Command::Fetch { seed: true }
        );
        assert_eq!(
            parse_args(args(&["api-drift", "check"])).unwrap(),
            Command::Check { staged: None }
        );
        assert_eq!(
            parse_args(args(&["api-drift", "check", "--staged", "/tmp/run"])).unwrap(),
            Command::Check {
                staged: Some(PathBuf::from("/tmp/run"))
            }
        );
        assert_eq!(
            parse_args(args(&["api-drift", "promote", "--dry-run"])).unwrap(),
            Command::PromoteDryRun { staged: None }
        );
    }

    #[test]
    fn rejects_unknown_and_non_api_drift_commands() {
        assert!(parse_args(args(&["spec-drift", "check"])).is_err());
        assert!(parse_args(args(&["api-drift", "bogus"])).is_err());
        assert!(
            parse_args(args(&["api-drift", "promote"])).is_err(),
            "promote needs --dry-run"
        );
        assert!(
            parse_args(args(&["api-drift", "check", "--staged"])).is_err(),
            "needs a dir"
        );
    }

    fn empty_run(codes: &[&str]) -> NormalizedRun {
        NormalizedRun {
            code_set: CodeSet::new(codes.iter().map(|c| c.to_string()), false),
            manifest: Manifest {
                upstream_tr_count: codes.len(),
                maintained_tr_count: 0,
                source_urls: vec![],
                normalizer_version: 1,
            },
            shapes: BTreeMap::new(),
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ls-trackers-cli-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// AE1 surface: a staged run round-trips — every artifact is written and the
    /// normalized layout reloads identically; `latest.txt` is updated.
    #[test]
    fn staged_run_writes_all_artifacts_and_reloads() {
        let root = scratch("write-run");
        let run_dir = root.join("runs").join("2026-06-16T00-00-00Z");
        let raw = RawInventory {
            source_urls: vec!["https://example/apiservice".to_string()],
            property_types: BTreeMap::new(),
            groups: vec![],
        };
        let normalized = empty_run(&["t1102", "t8412"]);
        let report = FetchReport {
            ok: true,
            fetched_count: 2,
            committed_code_set_len: Some(2),
            facts_degraded_groups: 0,
            failure: None,
        };
        write_staged_run(&run_dir, &raw, &normalized, &report).unwrap();

        assert!(run_dir.join(RAW_FILE).is_file());
        assert!(run_dir.join(CODE_SET_FILE).is_file());
        assert!(run_dir.join(MANIFEST_FILE).is_file());

        // The fetch-report is not just present — its content round-trips (AE1).
        let report_back: FetchReport =
            serde_json::from_slice(&fs::read(run_dir.join(FETCH_REPORT_FILE)).unwrap()).unwrap();
        assert_eq!(report_back, report);

        let reloaded = load_normalized(&run_dir).unwrap();
        assert_eq!(reloaded, normalized, "normalized layout round-trips");

        // latest.txt update is a separate step (mirrors fetch_and_stage).
        let paths = Paths {
            baseline_dir: root.join("baseline"),
            run_root: root.clone(),
            metadata_dir: root.join("metadata"),
        };
        update_latest(&paths, &run_dir).unwrap();
        let latest = fs::read_to_string(root.join("latest.txt")).unwrap();
        assert_eq!(latest.trim(), "runs/2026-06-16T00-00-00Z");
    }

    /// `check --staged` over committed-vs-staged maps to the tiered exit: a clean
    /// run is exit 0; a new-TR discovery gates to exit 1; a missing baseline is
    /// exit 2.
    #[test]
    fn check_staged_maps_to_tiered_exit_codes() {
        let root = scratch("check-exit");
        let baseline = root.join("baseline");
        let metadata = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("metadata");

        // Commit a baseline code-set of {t1102}.
        write_normalized(&baseline, &empty_run(&["t1102"])).unwrap();
        let paths = Paths {
            baseline_dir: baseline.clone(),
            run_root: root.join("runs"),
            metadata_dir: metadata.clone(),
        };

        // Clean staged run (identical inventory) → exit 0.
        let clean = root.join("staged-clean");
        write_normalized(&clean, &empty_run(&["t1102"])).unwrap();
        let result = run_check(&paths, Some(&clean));
        assert_eq!(exit_for(&result), Exit::Ok);
        assert!(!result.unwrap().gates());

        // Staged run discovers a new TR → exit 1.
        let drift = root.join("staged-drift");
        write_normalized(&drift, &empty_run(&["t1102", "BRANDNEW"])).unwrap();
        let result = run_check(&paths, Some(&drift));
        assert_eq!(exit_for(&result), Exit::Gated);

        // Missing committed baseline → exit 2.
        let bad_paths = Paths {
            baseline_dir: root.join("nonexistent"),
            ..paths.clone()
        };
        let result = run_check(&bad_paths, Some(&clean));
        assert_eq!(exit_for(&result), Exit::Error);
    }

    #[test]
    fn exit_code_values_match_the_contract() {
        assert_eq!(Exit::Ok.code(), 0);
        assert_eq!(Exit::Gated.code(), 1);
        assert_eq!(Exit::Error.code(), 2);
    }

    /// Bootstrap (absent baseline) yields `None`; a valid baseline yields its
    /// size; a present-but-corrupt baseline is a hard error (never silent `None`,
    /// which would disable the truncation guard).
    #[test]
    fn committed_code_set_len_distinguishes_bootstrap_from_corruption() {
        let root = scratch("committed-len");
        // Bootstrap: no baseline directory.
        assert_eq!(committed_code_set_len(&root.join("absent")).unwrap(), None);

        // Valid baseline → Some(len).
        let good = root.join("good");
        write_normalized(&good, &empty_run(&["a", "b", "c"])).unwrap();
        assert_eq!(committed_code_set_len(&good).unwrap(), Some(3));

        // Present but corrupt code-set.json → hard error.
        let bad = root.join("bad");
        write_normalized(&bad, &empty_run(&["a"])).unwrap();
        fs::write(bad.join(CODE_SET_FILE), b"{ not json").unwrap();
        assert!(
            committed_code_set_len(&bad).is_err(),
            "a corrupt baseline must not silently disable the truncation guard"
        );
    }

    /// A normalizer-version mismatch between committed and staged refuses to
    /// compare (exit 2) rather than emitting spurious cross-version findings.
    #[test]
    fn check_refuses_to_compare_across_normalizer_versions() {
        let root = scratch("version-guard");
        let baseline = root.join("baseline");
        let metadata = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("metadata");
        let mut committed = empty_run(&["t1102"]);
        committed.manifest.normalizer_version = 1;
        write_normalized(&baseline, &committed).unwrap();

        let staged = root.join("staged");
        let mut staged_run = empty_run(&["t1102"]);
        staged_run.manifest.normalizer_version = 2;
        write_normalized(&staged, &staged_run).unwrap();

        let paths = Paths {
            baseline_dir: baseline,
            run_root: root.join("runs"),
            metadata_dir: metadata,
        };
        let result = run_check(&paths, Some(&staged));
        assert_eq!(exit_for(&result), Exit::Error);
        assert!(result.unwrap_err().contains("normalizer version mismatch"));
    }
}
