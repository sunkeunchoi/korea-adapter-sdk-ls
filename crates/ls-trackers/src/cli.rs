//! The thin `ls-trackers` CLI (U5): `api-drift fetch`, `api-drift check`,
//! `api-drift promote --dry-run`, and `api-drift renormalize`, mapping findings
//! to the tiered exit contract (R17). Only `api-drift` subcommands are exposed
//! (R20).
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

use crate::api_drift::{
    compare, facts_degraded_finding, facts_outage_decision, normalize_run, DriftReport,
    FactsOutage, NormalizedRun,
};
use crate::fetch::{
    completeness_gate, FetchClient, GateOutcome, RawInventory, DEFAULT_TRUNCATION_PROPORTION,
};
use crate::spec_doc::{
    compare_examples, normalize_example_run, ExampleManifest, ExampleRun, SpecReport,
};
use crate::stages::promote_targets;
use crate::types::{CodeSet, DriftChange, ExampleShape, FetchReport, Manifest, TrShape};

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
    /// `renormalize` re-derives the committed normalized layout from the
    /// committed reviewed raw evidence, in place, without a live fetch (KTD-2) —
    /// the reviewed re-seed path for a normalizer-version bump.
    Renormalize,
    /// `spec-doc check` — compare a staged example projection against the
    /// committed example baseline, emitting advisory (non-gating) findings (U4).
    /// `--staged DIR` loads a pre-projected example run (the seam for a future
    /// staged snapshot); without it the staged side re-projects the shared raw.
    SpecCheck {
        staged: Option<PathBuf>,
    },
    /// `spec-doc renormalize` — re-project the example baseline from the shared
    /// committed raw snapshot, in place, network-free (the example re-seed path).
    SpecRenormalize,
}

/// Filesystem locations, injected so tests drive everything over a tempdir.
#[derive(Debug, Clone)]
pub struct Paths {
    /// Committed bounded baseline (`crates/ls-trackers/baselines/api-drift`). Also
    /// holds the shared raw snapshot the Specification Document Tracker re-projects
    /// examples from (`<baseline_dir>/raw/ls-openapi-full.json`, KTD2).
    pub baseline_dir: PathBuf,
    /// Staged-run root (`target/ls-trackers/api-drift`).
    pub run_root: PathBuf,
    /// Authored metadata directory (`metadata`).
    pub metadata_dir: PathBuf,
    /// Committed example baseline for the Specification Document Tracker
    /// (`crates/ls-trackers/baselines/spec-doc`) — its own tree under an
    /// independent `EXAMPLE_NORMALIZER_VERSION`, reusing the shared raw (KTD2).
    pub spec_baseline_dir: PathBuf,
}

impl Paths {
    /// Repo-root-relative defaults used by the installed binary.
    pub fn defaults() -> Self {
        Paths {
            baseline_dir: PathBuf::from("crates/ls-trackers/baselines/api-drift"),
            run_root: PathBuf::from("target/ls-trackers/api-drift"),
            metadata_dir: PathBuf::from("metadata"),
            spec_baseline_dir: PathBuf::from("crates/ls-trackers/baselines/spec-doc"),
        }
    }
}

/// Parse `<api-drift|spec-doc> <subcommand> [flags]`. Any other first token is a
/// usage error (mapped to exit `2`).
pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut it = args.into_iter();
    let family = it.next();
    let sub = it.next();
    let rest: Vec<String> = it.collect();
    match family.as_deref() {
        Some("api-drift") => parse_api_drift(sub.as_deref(), &rest),
        Some("spec-doc") => parse_spec_doc(sub.as_deref(), &rest),
        Some(other) => Err(format!(
            "unknown command `{other}` (expected `api-drift` or `spec-doc`)"
        )),
        None => Err(
            "usage: ls-trackers <api-drift|spec-doc> <subcommand> [--staged DIR] [--dry-run]"
                .to_string(),
        ),
    }
}

/// Parse an `api-drift` subcommand.
fn parse_api_drift(sub: Option<&str>, rest: &[String]) -> Result<Command, String> {
    match sub {
        Some("fetch") => {
            let mut seed = false;
            for arg in rest {
                match arg.as_str() {
                    "--seed" => seed = true,
                    other => return Err(format!("unexpected argument `{other}`")),
                }
            }
            Ok(Command::Fetch { seed })
        }
        Some("check") => Ok(Command::Check {
            staged: parse_staged(rest)?,
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
        Some("renormalize") => {
            if let Some(other) = rest.first() {
                return Err(format!("unexpected argument `{other}`"));
            }
            Ok(Command::Renormalize)
        }
        Some(other) => Err(format!("unknown api-drift subcommand `{other}`")),
        None => Err(
            "usage: ls-trackers api-drift <fetch|check|promote --dry-run|renormalize>".to_string(),
        ),
    }
}

/// Parse a `spec-doc` subcommand. The tracker adds no fetch path (R1) — it reuses
/// the shared raw the API Drift staging path produces — so only `check` and the
/// network-free `renormalize` re-seed are exposed.
fn parse_spec_doc(sub: Option<&str>, rest: &[String]) -> Result<Command, String> {
    match sub {
        Some("check") => Ok(Command::SpecCheck {
            staged: parse_staged(rest)?,
        }),
        Some("renormalize") => {
            if let Some(other) = rest.first() {
                return Err(format!("unexpected argument `{other}`"));
            }
            Ok(Command::SpecRenormalize)
        }
        Some(other) => Err(format!("unknown spec-doc subcommand `{other}`")),
        None => Err("usage: ls-trackers spec-doc <check [--staged DIR]|renormalize>".to_string()),
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

/// The aggregated example-shapes file. Unlike the API Drift per-TR layout
/// (`normalized/trs/{code}.json`), the **full-inventory** example baseline (KTD8)
/// is stored as one sorted `code → ExampleShape` map. Per-TR files are lossy
/// here: ~4 upstream codes collide case-insensitively (`S3_`/`s3_`, `K1_`/`k1_`,
/// `S2_`/`s2_`, `YS3`/`Ys3`), so on a case-insensitive filesystem the second
/// write of each pair would overwrite the first and silently drop a shape —
/// breaking the clean self-diff. A single map is collision-proof and reviews as
/// one sorted file.
const EXAMPLES_FILE: &str = "normalized/examples.json";

/// Write an example projection to `dir`: code-set, example manifest, and the
/// aggregated example-shapes map. Deterministic, pretty JSON. The map value type
/// is [`ExampleShape`], never a raw `serde_json::Value`, so no unprocessed
/// example payload can be written (KTD7).
pub fn write_example_baseline(dir: &Path, run: &ExampleRun) -> std::io::Result<()> {
    fs::create_dir_all(dir.join("normalized"))?;
    write_json(&dir.join(CODE_SET_FILE), &run.code_set)?;
    write_json(&dir.join(MANIFEST_FILE), &run.manifest)?;
    write_json(&dir.join(EXAMPLES_FILE), &run.shapes)?;
    Ok(())
}

/// Load an [`ExampleRun`] (code-set + example manifest + aggregated example
/// shapes) from a `spec-doc` baseline or staged-run directory. Missing or
/// malformed files are an error (exit `2`).
pub fn load_example_baseline(dir: &Path) -> Result<ExampleRun, String> {
    let code_set: CodeSet = read_json(&dir.join(CODE_SET_FILE))?;
    let manifest: ExampleManifest = read_json(&dir.join(MANIFEST_FILE))?;
    let shapes: BTreeMap<String, ExampleShape> = read_json(&dir.join(EXAMPLES_FILE))?;
    Ok(ExampleRun {
        code_set,
        manifest,
        shapes,
    })
}

/// Re-project the example shapes from the shared committed raw snapshot
/// (`<baseline_dir>/raw/ls-openapi-full.json`, KTD2) — the network-free staged
/// side. `provisional` is supplied by the caller (from the committed baseline it
/// already holds) so this does not re-read the committed baseline.
fn reproject_examples_from_raw(paths: &Paths, provisional: bool) -> Result<ExampleRun, String> {
    let raw: RawInventory = read_json(&paths.baseline_dir.join(RAW_FILE))?;
    Ok(normalize_example_run(&raw, provisional))
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
    let (staged_run, fetch_report) = match staged {
        Some(dir) => {
            let run = load_normalized(dir).map_err(|e| format!("staged run unavailable: {e}"))?;
            (run, load_fetch_report(dir)?)
        }
        None => {
            let (run_dir, run) = fetch_and_stage(paths, /* provisional */ false)?;
            let report = load_fetch_report(&run_dir)?;
            (run, report)
        }
    };

    // Support-aware facts-outage gate (U5, R3), applied *before* compare so
    // degraded facts never turn into spurious Structural API Shape changes
    // (KTD-4). The
    // decision is single-sourced in `facts_outage_decision` (KTD-3); a synthetic
    // staged run with no `fetch-report.json` carries no degradation context, so
    // the gate is a no-op there.
    let outage = match &fetch_report {
        Some(report) => {
            let maintained: BTreeSet<String> = trs.keys().cloned().collect();
            let maintained_present = staged_run
                .code_set
                .codes
                .iter()
                .any(|c| maintained.contains(c));
            facts_outage_decision(
                &report.degraded_tr_codes,
                &maintained,
                report.property_type_fallback_served,
                maintained_present,
            )
        }
        None => FactsOutage::None,
    };
    if let FactsOutage::MaintainedAffected(reason) = &outage {
        return Err(format!(
            "facts outage affects a maintained TR ({reason}); degraded facts could corrupt a \
             gated comparison — re-fetch before comparing"
        ));
    }

    // Refuse to compare across normalizer versions — the committed
    // description-hashes were computed under different rules, so a mismatch would
    // emit spurious findings instead of a clean diff. Re-baseline first (exit 2).
    if committed.manifest.normalizer_version != staged_run.manifest.normalizer_version {
        return Err(format!(
            "normalizer version mismatch: committed v{} vs staged v{} — re-baseline first",
            committed.manifest.normalizer_version, staged_run.manifest.normalizer_version
        ));
    }

    let mut report = compare(&committed, &staged_run, &trs);
    // An untracked-only facts degradation is surfaced as a visible, non-gating
    // finding at exit `0` (R3); re-sort so presentation stays highest-first.
    if let FactsOutage::UntrackedOnly(detail) = outage {
        report.findings.push(facts_degraded_finding(detail));
        report.findings.sort_by(|a, b| b.severity.cmp(&a.severity));
    }
    Ok(report)
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
    let outcome = client.fetch_full_inventory().map_err(|e| e.to_string())?;
    let raw = outcome.inventory;
    let property_type_fallback_served = outcome.property_type_fallback_served;
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
    // rate drift at compare time. Surface the degradation at fetch time and
    // record the affected TR codes so the gate (U5) can join on code (KTD-4a).
    let degraded_tr_codes = raw.facts_degraded_tr_codes();
    let facts_degraded_groups = raw.groups.iter().filter(|g| g.group_id.is_none()).count();
    if facts_degraded_groups > 0 {
        eprintln!(
            "warning: {facts_degraded_groups} of {} group(s) returned no protocol/rate facts; \
             endpoint/rate fields may be degraded for this run",
            raw.groups.len()
        );
    }
    if property_type_fallback_served {
        eprintln!(
            "warning: property-type mapping served the hardcoded fallback; field types may be \
             degraded for this run"
        );
    }

    let normalized = normalize_run(&raw, &maintained, provisional);
    let report = FetchReport {
        ok: true,
        fetched_count: fetched,
        committed_code_set_len: committed_len,
        facts_degraded_groups,
        degraded_tr_codes,
        property_type_fallback_served,
        failure: None,
    };
    write_staged_run(&run_dir, &raw, &normalized, &report).map_err(|e| e.to_string())?;
    update_latest(paths, &run_dir)?;
    Ok((run_dir, normalized))
}

/// Re-normalize the committed baseline's reviewed raw evidence into a fresh
/// normalized layout, in place, without a live fetch (KTD-2). Reads
/// `<baseline>/raw/ls-openapi-full.json`, normalizes the maintained TRs with the
/// current normalizer, preserves the committed code-set's `provisional` stance,
/// and rewrites the code-set / manifest / per-TR shapes. Deterministic and
/// network-free — it constructs no HTTP client. This is the reviewed re-seed path
/// after a [`NORMALIZER_VERSION`](crate::api_drift::NORMALIZER_VERSION) bump.
pub fn renormalize_committed(paths: &Paths) -> Result<NormalizedRun, String> {
    let maintained = maintained_codes(paths)?;
    let raw: RawInventory = read_json(&paths.baseline_dir.join(RAW_FILE))?;
    // Preserve the committed seed's provisional stance (KTD-6); default to
    // provisional when no committed code-set exists yet (bootstrap).
    let committed_prev = load_normalized(&paths.baseline_dir).ok();
    let provisional = committed_prev
        .as_ref()
        .map(|run| run.code_set.provisional)
        .unwrap_or(true);
    let normalized = normalize_run(&raw, &maintained, provisional);

    // Re-seeding re-derives the code-set from the committed raw evidence. If that
    // membership differs from the committed code-set (raw was refreshed, or codes
    // were admitted into code-set.json out of band), surface it rather than
    // silently overwriting — the re-seed is sold as a normalizer-only refresh.
    if let Some(prev) = &committed_prev {
        let added = normalized
            .code_set
            .codes
            .difference(&prev.code_set.codes)
            .count();
        let removed = prev
            .code_set
            .codes
            .difference(&normalized.code_set.codes)
            .count();
        if added > 0 || removed > 0 {
            eprintln!(
                "warning: re-normalized code-set differs from committed ({added} added, \
                 {removed} removed); review the code-set.json diff before committing"
            );
        }
    }

    write_normalized(&paths.baseline_dir, &normalized).map_err(|e| e.to_string())?;
    prune_stale_shapes(&paths.baseline_dir, &normalized)?;
    Ok(normalized)
}

/// Remove per-TR shape files in the committed layout whose code is no longer in
/// the maintained set. Without this, a maintained TR dropped from metadata would
/// leave a ghost `trs/<code>.json` that [`load_normalized`] keeps reading as
/// committed truth. Only the in-place re-seed needs this — a live staged run
/// writes into a fresh timestamped directory.
fn prune_stale_shapes(baseline_dir: &Path, normalized: &NormalizedRun) -> Result<(), String> {
    let trs_dir = baseline_dir.join(TRS_DIR);
    if !trs_dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(&trs_dir).map_err(|e| format!("reading {}: {e}", trs_dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_some_and(|x| x == "json") {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            if !normalized.shapes.contains_key(stem) {
                fs::remove_file(&path).map_err(|e| format!("removing {}: {e}", path.display()))?;
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Specification Document Tracker orchestration (U4)
// ---------------------------------------------------------------------------

/// Run `spec-doc check`: load the committed example baseline (the **Reviewed
/// Baseline**), obtain the staged example projection, and compare. Findings are
/// advisory and never gate (KTD4); only a load/parse/version error exits `2`.
///
/// The staged side is network-free: `--staged DIR` loads a pre-projected example
/// run (the seam for a future staged snapshot), and the default re-projects the
/// shared committed raw (`<baseline_dir>/raw`, KTD2), so the check is a self-diff
/// against the reviewed baseline.
pub fn run_spec_check(paths: &Paths, staged: Option<&Path>) -> Result<SpecReport, String> {
    let committed = load_example_baseline(&paths.spec_baseline_dir)
        .map_err(|e| format!("committed spec-doc baseline unavailable: {e}"))?;
    let trs = load_metadata(paths)?;
    let staged_run = match staged {
        Some(dir) => {
            load_example_baseline(dir).map_err(|e| format!("staged spec-doc run unavailable: {e}"))?
        }
        // Re-project from the shared raw, carrying the committed baseline's
        // provisional stance — `committed` is already loaded, so no re-read.
        None => reproject_examples_from_raw(paths, committed.code_set.provisional)?,
    };

    // Refuse to compare across example-normalizer versions — the committed shapes
    // were projected under different rules, so a mismatch would emit spurious
    // findings instead of a clean diff. Re-baseline first (exit 2). Mirrors the
    // API Drift normalizer-version guard.
    if committed.manifest.normalizer_version != staged_run.manifest.normalizer_version {
        return Err(format!(
            "example normalizer version mismatch: committed v{} vs staged v{} — re-baseline first",
            committed.manifest.normalizer_version, staged_run.manifest.normalizer_version
        ));
    }

    Ok(compare_examples(&committed, &staged_run, &trs))
}

/// Re-project the example baseline from the shared committed raw snapshot, in
/// place, network-free — the example re-seed path (mirrors
/// [`renormalize_committed`]). Rewrites the code-set, manifest, and aggregated
/// example-shapes map under `spec_baseline_dir`. No per-TR pruning is needed: the
/// single aggregated map is fully rewritten, so a TR that lost its example simply
/// disappears from the map.
pub fn renormalize_examples(paths: &Paths) -> Result<ExampleRun, String> {
    // Preserve the committed example baseline's provisional stance when one
    // exists (else default to provisional, bootstrap).
    let provisional = load_example_baseline(&paths.spec_baseline_dir)
        .ok()
        .map(|run| run.code_set.provisional)
        .unwrap_or(true);
    let run = reproject_examples_from_raw(paths, provisional)?;
    write_example_baseline(&paths.spec_baseline_dir, &run).map_err(|e| e.to_string())?;
    Ok(run)
}

/// Map a `spec-doc check` result to the tiered exit. Example findings are
/// advisory (KTD4), so a successful comparison is always exit `0`; only a
/// load/parse/version error exits `2`. `gates()` is consulted for parity with
/// [`exit_for`], though it is `false` by construction.
pub fn spec_exit_for(result: &Result<SpecReport, String>) -> Exit {
    match result {
        Ok(report) if report.gates() => Exit::Gated,
        Ok(_) => Exit::Ok,
        Err(_) => Exit::Error,
    }
}

fn failure_report(fetched: usize, committed_len: Option<usize>, gate: &GateOutcome) -> FetchReport {
    FetchReport {
        ok: false,
        fetched_count: fetched,
        committed_code_set_len: committed_len,
        facts_degraded_groups: 0,
        degraded_tr_codes: BTreeSet::new(),
        property_type_fallback_served: false,
        failure: Some(format!("{gate:?}")),
    }
}

/// Read a staged or live run's `fetch-report.json`. `None` means the file is
/// absent — a synthetic staged run (tests, hand-authored fixtures) carries no
/// degradation context and the facts gate is a no-op. A file that is *present
/// but unreadable* is an error (exit `2`): it must not silently degrade to `None`
/// and disable the facts gate (U5, R3), the same fail-loud stance as
/// [`committed_code_set_len`].
fn load_fetch_report(dir: &Path) -> Result<Option<FetchReport>, String> {
    let path = dir.join(FETCH_REPORT_FILE);
    if path.exists() {
        read_json(&path).map(Some)
    } else {
        Ok(None)
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
        Command::Renormalize => match renormalize_committed(paths) {
            Ok(run) => {
                println!(
                    "re-normalized {} maintained shape(s) at normalizer v{} into {}",
                    run.shapes.len(),
                    run.manifest.normalizer_version,
                    paths.baseline_dir.display()
                );
                Exit::Ok
            }
            Err(e) => {
                eprintln!("error: {e}");
                Exit::Error
            }
        },
        Command::PromoteDryRun { staged } => {
            let result = run_check(paths, staged.as_deref());
            match &result {
                Ok(report) => {
                    let affected = promote_affected_codes(report);
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
        Command::SpecCheck { staged } => {
            let result = run_spec_check(paths, staged.as_deref());
            match &result {
                Ok(report) => print_spec_report(report),
                Err(e) => eprintln!("error: {e}"),
            }
            spec_exit_for(&result)
        }
        Command::SpecRenormalize => match renormalize_examples(paths) {
            Ok(run) => {
                println!(
                    "re-projected {} example shape(s) at example-normalizer v{} into {}",
                    run.shapes.len(),
                    run.manifest.normalizer_version,
                    paths.spec_baseline_dir.display()
                );
                Exit::Ok
            }
            Err(e) => {
                eprintln!("error: {e}");
                Exit::Error
            }
        },
    }
}

/// Print an advisory example report: each finding with its support-aware severity
/// and any maintained-artifact review pointers (R5). No finding ever gates (KTD4),
/// so there is no `GATE` column — example changes are review candidates, not
/// failures.
fn print_spec_report(report: &SpecReport) {
    if report.findings.is_empty() {
        println!("no example findings.");
    } else {
        println!("{} example finding(s):", report.findings.len());
        for f in &report.findings {
            let pointers = if f.pointers.is_empty() {
                " (no artifact pointer — informational)".to_string()
            } else {
                let pointer_paths: Vec<&str> = f.pointers.iter().map(|p| p.path.as_str()).collect();
                format!(" → review: {}", pointer_paths.join(", "))
            };
            println!("  [{}] {} {:?}{pointers}", f.severity, f.tr_code, f.change);
        }
    }
    let c = &report.coverage;
    println!(
        "coverage: {} upstream, {} carrying examples",
        c.upstream_count, c.example_tr_count
    );
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

/// TR codes a real promote would touch, derived from a drift report. The
/// `FactsDegraded` finding carries the `(facts)` whole-inventory marker rather
/// than a real TR code, so it is excluded — otherwise a dry-run during a facts
/// outage would print a bogus `(facts)` promote target.
fn promote_affected_codes(report: &DriftReport) -> BTreeSet<&str> {
    report
        .findings
        .iter()
        .filter(|f| !matches!(f.change, DriftChange::FactsDegraded { .. }))
        .map(|f| f.tr_code.as_str())
        .collect()
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
    use crate::types::{CodeSet, DriftChange, Manifest};

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
        assert_eq!(
            parse_args(args(&["api-drift", "renormalize"])).unwrap(),
            Command::Renormalize
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
        assert!(
            parse_args(args(&["api-drift", "renormalize", "--seed"])).is_err(),
            "renormalize takes no arguments"
        );
    }

    /// The committed-raw root that ships with the crate — the reviewed evidence
    /// the re-normalize affordance reads (KTD-2).
    fn committed_baseline_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("baselines")
            .join("api-drift")
    }

    fn repo_metadata_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("metadata")
    }

    /// U2: re-normalizing the committed reviewed raw evidence is network-free,
    /// produces a v2 manifest with the 7 maintained shapes, and is byte-stable
    /// across runs. Writes into a scratch copy so the committed baseline is never
    /// mutated by the test.
    #[test]
    fn renormalize_from_committed_raw_is_deterministic_and_v2() {
        let scratch = scratch("renormalize");
        // Copy only the reviewed raw evidence into the scratch baseline; the
        // affordance regenerates the normalized layout from it.
        fs::create_dir_all(scratch.join("raw")).unwrap();
        fs::copy(
            committed_baseline_dir().join(RAW_FILE),
            scratch.join(RAW_FILE),
        )
        .unwrap();

        let paths = Paths {
            baseline_dir: scratch.clone(),
            run_root: scratch.join("runs"),
            metadata_dir: repo_metadata_dir(),
            spec_baseline_dir: scratch.join("spec-doc"),
        };

        let run = renormalize_committed(&paths).expect("re-normalize from committed raw");
        assert_eq!(
            run.manifest.normalizer_version,
            crate::api_drift::NORMALIZER_VERSION
        );
        assert_eq!(run.manifest.normalizer_version, 2);
        assert_eq!(run.shapes.len(), 7, "seven maintained shapes");
        assert_eq!(run.code_set.len(), 365, "full inventory code-set preserved");
        assert!(
            run.code_set.provisional,
            "the seed's provisional stance is preserved (KTD-6)"
        );
        // The token correction is present: `scope` is a real field now (U1).
        let token = &run.shapes["token"];
        assert!(
            token
                .response_blocks
                .iter()
                .any(|f| f.field_name == "scope" && f.length == Some(256)),
            "token exposes scope after re-normalization"
        );

        // Byte-stable: a second re-normalization writes identical token bytes.
        let first = fs::read(scratch.join(TRS_DIR).join("token.json")).unwrap();
        renormalize_committed(&paths).expect("second re-normalize");
        let second = fs::read(scratch.join(TRS_DIR).join("token.json")).unwrap();
        assert_eq!(first, second, "re-normalization is byte-stable");
    }

    /// U2 hardening: re-normalizing an already-attested baseline preserves a
    /// `provisional: false` stance (KTD-6, non-bootstrap), and prunes a stale
    /// shape left from a prior re-seed so it cannot linger as committed truth.
    #[test]
    fn renormalize_preserves_attested_flag_and_prunes_stale_shapes() {
        let scratch = scratch("renormalize-prune");
        // Seed a committed normalized layout that is already attested (provisional
        // false) and carries a ghost shape for a TR no longer maintained.
        write_normalized(&scratch, &empty_run(&["t1102"])).unwrap();
        assert!(!empty_run(&["t1102"]).code_set.provisional, "fixture is attested");
        fs::write(
            scratch.join(TRS_DIR).join("ghost.json"),
            br#"{"tr_code":"ghost","protocol":"rest","is_websocket":false}"#,
        )
        .unwrap();
        fs::create_dir_all(scratch.join("raw")).unwrap();
        fs::copy(
            committed_baseline_dir().join(RAW_FILE),
            scratch.join(RAW_FILE),
        )
        .unwrap();

        let paths = Paths {
            baseline_dir: scratch.clone(),
            run_root: scratch.join("runs"),
            metadata_dir: repo_metadata_dir(),
            spec_baseline_dir: scratch.join("spec-doc"),
        };
        let run = renormalize_committed(&paths).expect("re-normalize");

        assert!(
            !run.code_set.provisional,
            "an already-attested seed keeps provisional=false (KTD-6)"
        );
        assert!(
            !scratch.join(TRS_DIR).join("ghost.json").exists(),
            "a stale shape is pruned from the committed layout"
        );
        assert!(
            scratch.join(TRS_DIR).join("token.json").exists(),
            "maintained shapes remain"
        );
        assert_eq!(run.shapes.len(), 7);
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
            degraded_tr_codes: BTreeSet::new(),
            property_type_fallback_served: false,
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
            spec_baseline_dir: root.join("spec-doc"),
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
            spec_baseline_dir: root.join("spec-doc"),
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

    /// Write a `fetch-report.json` carrying degradation signals into a staged-run
    /// directory, so `run_check` exercises the facts gate on the `--staged` path
    /// with no network call.
    fn write_facts_report(dir: &Path, degraded: &[&str], fallback: bool) {
        let report = FetchReport {
            ok: true,
            fetched_count: 1,
            committed_code_set_len: Some(1),
            facts_degraded_groups: usize::from(!degraded.is_empty()),
            degraded_tr_codes: degraded.iter().map(|s| s.to_string()).collect(),
            property_type_fallback_served: fallback,
            failure: None,
        };
        write_json(&dir.join(FETCH_REPORT_FILE), &report).unwrap();
    }

    fn has_facts_finding(report: &DriftReport) -> bool {
        report
            .findings
            .iter()
            .any(|f| matches!(&f.change, DriftChange::FactsDegraded { .. }))
    }

    /// U5 (R3): the facts-outage gate is support-aware on the `--staged` path and
    /// network-free. The discriminating case — an untracked-only degradation
    /// exits `0` with a visible finding; a co-occurring maintained degradation
    /// exits `2` before compare. A property-type fallback with a maintained TR
    /// present exits `2`; a clean report and a report-less run are unaffected.
    #[test]
    fn facts_outage_gate_is_support_aware_on_the_staged_path() {
        let root = scratch("facts-gate");
        let baseline = root.join("baseline");
        let metadata = repo_metadata_dir();
        write_normalized(&baseline, &empty_run(&["t1102"])).unwrap();
        let paths = Paths {
            baseline_dir: baseline,
            run_root: root.join("runs"),
            metadata_dir: metadata,
            spec_baseline_dir: root.join("spec-doc"),
        };

        // Untracked-only degradation → exit 0 + a visible facts finding.
        let untracked = root.join("staged-untracked");
        write_normalized(&untracked, &empty_run(&["t1102"])).unwrap();
        write_facts_report(&untracked, &["UNTRACKED_X"], false);
        let result = run_check(&paths, Some(&untracked));
        assert_eq!(exit_for(&result), Exit::Ok);
        let report = result.unwrap();
        assert!(!report.gates(), "untracked-only facts outage does not gate");
        assert!(has_facts_finding(&report), "a visible finding is emitted");

        // A maintained TR co-occurs in the degraded set → exit 2 before compare.
        let maintained_deg = root.join("staged-maintained");
        write_normalized(&maintained_deg, &empty_run(&["t1102"])).unwrap();
        write_facts_report(&maintained_deg, &["t1102", "UNTRACKED_X"], false);
        let result = run_check(&paths, Some(&maintained_deg));
        assert_eq!(exit_for(&result), Exit::Error);
        assert!(result.unwrap_err().contains("maintained TR"));

        // Property-type mapping fallback + a maintained TR present → exit 2.
        let proptype = root.join("staged-proptype");
        write_normalized(&proptype, &empty_run(&["t1102"])).unwrap();
        write_facts_report(&proptype, &[], true);
        assert_eq!(exit_for(&run_check(&paths, Some(&proptype))), Exit::Error);

        // A clean fetch-report → no facts finding, unaffected (exit 0).
        let clean = root.join("staged-clean-facts");
        write_normalized(&clean, &empty_run(&["t1102"])).unwrap();
        write_facts_report(&clean, &[], false);
        let report = run_check(&paths, Some(&clean)).unwrap();
        assert!(!has_facts_finding(&report));
        assert!(!report.gates());

        // No fetch-report at all (synthetic run) → the gate is a no-op (exit 0).
        let no_report = root.join("staged-no-report");
        write_normalized(&no_report, &empty_run(&["t1102"])).unwrap();
        assert_eq!(exit_for(&run_check(&paths, Some(&no_report))), Exit::Ok);

        // A present-but-corrupt fetch-report.json must NOT silently disable the
        // gate — it is exit 2, not a no-op (distinct from the absent case above).
        let corrupt = root.join("staged-corrupt-report");
        write_normalized(&corrupt, &empty_run(&["t1102"])).unwrap();
        fs::write(corrupt.join(FETCH_REPORT_FILE), b"{ not json").unwrap();
        assert_eq!(
            exit_for(&run_check(&paths, Some(&corrupt))),
            Exit::Error,
            "a corrupt fetch-report is an error, not a silently-disabled gate"
        );
    }

    /// `promote --dry-run` excludes the `(facts)` whole-inventory marker so a
    /// dry-run during an untracked-only facts outage does not print a bogus
    /// `(facts)` promote target — only real TR codes are surfaced.
    #[test]
    fn promote_affected_codes_excludes_facts_marker() {
        use crate::api_drift::{facts_degraded_finding, DriftReport};
        use crate::types::{CoverageSummary, DriftChange, Severity, SupportState};

        let real = crate::types::DriftFinding {
            tr_code: "t1102".to_string(),
            change: DriftChange::TrRemoved,
            severity: Severity::Breaking,
            support_state: SupportState::Implemented,
            is_new_tr: false,
            gates: true,
            possible_rename: None,
        };
        let report = DriftReport {
            findings: vec![real, facts_degraded_finding("endpoint/rate degraded".to_string())],
            coverage: CoverageSummary::default(),
        };
        let affected = promote_affected_codes(&report);
        assert!(affected.contains("t1102"), "real TR codes are surfaced");
        assert!(
            !affected.contains("(facts)"),
            "the facts marker is not a promote target"
        );
    }

    // --- spec-doc subcommand (U4) -----------------------------------------

    fn example_run(trs: &[(&str, Value, Value)]) -> ExampleRun {
        use crate::fetch::{RawGroup, RawTr};
        let raw_trs: Vec<RawTr> = trs
            .iter()
            .map(|(code, req, res)| RawTr {
                code: code.to_string(),
                name: Some(code.to_string()),
                is_websocket: false,
                http_method: Some("POST".to_string()),
                url: Some(format!("/{code}")),
                protocol_type: Some("REST".to_string()),
                rate_limit_per_sec: Some(1),
                corp_rate_limit_per_sec: None,
                description: None,
                properties: vec![],
                req_example: req.clone(),
                res_example: res.clone(),
            })
            .collect();
        let inv = RawInventory {
            source_urls: vec![],
            property_types: BTreeMap::new(),
            groups: vec![RawGroup {
                category_name: "c".to_string(),
                group_id: Some("g".to_string()),
                group_name: "그룹".to_string(),
                is_websocket_group: false,
                trs: raw_trs,
            }],
        };
        normalize_example_run(&inv, false)
    }

    use serde_json::Value;

    fn jstr(s: &str) -> Value {
        Value::String(s.to_string())
    }

    /// `parse_args` parses `spec-doc check [--staged DIR]` and `renormalize`, and
    /// rejects unknown `spec-doc` subcommands. `spec-drift` stays an unknown first
    /// token (the API Drift rejection assertion is unaffected).
    #[test]
    fn parses_spec_doc_subcommands() {
        assert_eq!(
            parse_args(args(&["spec-doc", "check"])).unwrap(),
            Command::SpecCheck { staged: None }
        );
        assert_eq!(
            parse_args(args(&["spec-doc", "check", "--staged", "/tmp/run"])).unwrap(),
            Command::SpecCheck {
                staged: Some(PathBuf::from("/tmp/run"))
            }
        );
        assert_eq!(
            parse_args(args(&["spec-doc", "renormalize"])).unwrap(),
            Command::SpecRenormalize
        );
        assert!(parse_args(args(&["spec-doc", "bogus"])).is_err());
        assert!(
            parse_args(args(&["spec-doc", "renormalize", "--seed"])).is_err(),
            "renormalize takes no arguments"
        );
        // `spec-drift` (drift, not doc) is still an unknown first token.
        assert!(parse_args(args(&["spec-drift", "check"])).is_err());
    }

    /// `run_spec_check` against a clean Reviewed Baseline (compared to itself) →
    /// exit 0 with no findings.
    #[test]
    fn spec_check_clean_self_diff_exits_zero() {
        let root = scratch("spec-clean");
        let spec_dir = root.join("spec-doc");
        let run = example_run(&[("t1102", Value::Null, jstr(r#"{"blk":{"price":1}}"#))]);
        write_example_baseline(&spec_dir, &run).unwrap();
        let paths = Paths {
            baseline_dir: root.join("baseline"),
            run_root: root.join("runs"),
            metadata_dir: repo_metadata_dir(),
            spec_baseline_dir: spec_dir.clone(),
        };
        let result = run_spec_check(&paths, Some(&spec_dir));
        assert_eq!(spec_exit_for(&result), Exit::Ok);
        let report = result.unwrap();
        assert!(report.findings.is_empty(), "self-diff is clean: {:?}", report.findings);
        assert!(!report.gates());
    }

    /// A simulated example-normalizer-version mismatch between committed and
    /// staged refuses to compare (exit 2).
    #[test]
    fn spec_check_refuses_cross_version_compare() {
        let root = scratch("spec-version");
        let spec_dir = root.join("spec-doc");
        let mut committed = example_run(&[("t1102", Value::Null, jstr(r#"{"a":1}"#))]);
        committed.manifest.normalizer_version = 1;
        write_example_baseline(&spec_dir, &committed).unwrap();

        let staged_dir = root.join("staged");
        let mut staged = example_run(&[("t1102", Value::Null, jstr(r#"{"a":1}"#))]);
        staged.manifest.normalizer_version = 2;
        write_example_baseline(&staged_dir, &staged).unwrap();

        let paths = Paths {
            baseline_dir: root.join("baseline"),
            run_root: root.join("runs"),
            metadata_dir: repo_metadata_dir(),
            spec_baseline_dir: spec_dir,
        };
        let result = run_spec_check(&paths, Some(&staged_dir));
        assert_eq!(spec_exit_for(&result), Exit::Error);
        assert!(result.unwrap_err().contains("example normalizer version mismatch"));
    }

    /// AE1 + AE2 over the CLI: a Tracked-TR (implemented `t1102`) example change
    /// exits 0 with a visible advisory finding carrying its doc pointers; an
    /// untracked-only example change exits 0 with a visible finding, no pointer.
    #[test]
    fn spec_check_emits_advisory_findings_and_pointers_at_exit_zero() {
        let root = scratch("spec-findings");
        let spec_dir = root.join("spec-doc");
        let committed = example_run(&[
            ("t1102", Value::Null, jstr(r#"{"blk":{"price":1}}"#)),
            ("UNTRACKED", Value::Null, jstr(r#"{"x":1}"#)),
        ]);
        write_example_baseline(&spec_dir, &committed).unwrap();

        let staged_dir = root.join("staged");
        let staged = example_run(&[
            ("t1102", Value::Null, jstr(r#"{"blk":{"price":1,"qty":2}}"#)),
            ("UNTRACKED", Value::Null, jstr(r#"{"x":1,"y":2}"#)),
        ]);
        write_example_baseline(&staged_dir, &staged).unwrap();

        let paths = Paths {
            baseline_dir: root.join("baseline"),
            run_root: root.join("runs"),
            metadata_dir: repo_metadata_dir(),
            spec_baseline_dir: spec_dir,
        };
        let result = run_spec_check(&paths, Some(&staged_dir));
        assert_eq!(spec_exit_for(&result), Exit::Ok, "advisory findings never gate");
        let report = result.unwrap();
        assert!(!report.gates());

        let t1102 = report.findings.iter().find(|f| f.tr_code == "t1102").unwrap();
        assert!(!t1102.pointers.is_empty(), "implemented TR carries doc pointers");
        assert!(t1102
            .pointers
            .iter()
            .any(|p| p.path == "docs/reference/t1102.md"));

        let untracked = report.findings.iter().find(|f| f.tr_code == "UNTRACKED").unwrap();
        assert!(untracked.pointers.is_empty(), "untracked TR carries no pointer");
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
            spec_baseline_dir: root.join("spec-doc"),
        };
        let result = run_check(&paths, Some(&staged));
        assert_eq!(exit_for(&result), Exit::Error);
        assert!(result.unwrap_err().contains("normalizer version mismatch"));
    }
}
