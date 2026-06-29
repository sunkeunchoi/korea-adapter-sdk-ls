//! The thin `ls-trackers` CLI: three command families — `api-drift`
//! (`fetch` / `check` / `promote --dry-run` / `renormalize`), `spec-doc`
//! (`check` / `renormalize`), and `freshness` (`check`) — each mapping findings
//! to the tiered exit contract (R17).
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
    compare, facts_degraded_finding, facts_outage_decision, normalize_run, type_only_gate,
    DriftReport, FactsOutage, NormalizedRun, TypeOnlyDecision,
};
use crate::fetch::{
    completeness_gate, FetchClient, GateOutcome, RawInventory, DEFAULT_TRUNCATION_PROPORTION,
};
use crate::spec_doc::{
    compare_examples, normalize_example_run, ExampleManifest, ExampleRun, SpecReport,
};
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
        /// Preview the opt-in type-only promotion gate (R1) alongside the drift
        /// review — reports whether a `--type-only --attest` promote would be
        /// admitted or blocked, writing nothing.
        type_only: bool,
    },
    /// `promote --attest <operator-or-issue>` — the mutating Baseline Promotion
    /// (R1, R4). Replaces the committed raw with the pinned staged run's raw,
    /// re-derives the normalized baselines, and appends one promotion-log record.
    /// `--attest` is the only path that writes; its value is the free-form, non-empty
    /// attested-by string (KTD6). `--staged DIR` pins an explicit run; otherwise the
    /// `latest.txt` pointer selects the most recent (R2).
    Promote {
        staged: Option<PathBuf>,
        attest: String,
        /// Opt-in type-only promotion gate (R1, R3): refuse with exit 2 (zero
        /// mutation) unless the maintained-shape drift is a pure field-type wave
        /// plus `DescriptionChanged`. Independent of `--attest` — attesting cannot
        /// satisfy it (R3).
        type_only: bool,
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
    /// `freshness check` — evaluate the 90-day evidence backstop over Recommended
    /// TRs, emitting advisory (non-gating) `Severity::Evidence` findings for any
    /// past the window. Operator-invoked, mutates nothing (R7). `--json` swaps the
    /// human printer for the pinned machine-readable contract the scheduled-cadence
    /// workflow consumes; exit semantics are unchanged.
    FreshnessCheck {
        json: bool,
    },
    /// `freshness re-pin <tr> [--force]` — capture the current committed baseline
    /// shape into the named Recommended TR's evidence record as its attested
    /// shape, the permanent R11 re-attestation interface. **Populate-if-absent**:
    /// refuses to overwrite an existing attested shape (which would silently clear
    /// a standing stale-by-change signal) unless `--force` is passed during a real
    /// re-attestation. Mutates exactly one evidence file; no network, no metadata
    /// flip.
    FreshnessRePin {
        tr_code: String,
        force: bool,
    },
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
        Some("freshness") => parse_freshness(sub.as_deref(), &rest),
        Some(other) => Err(format!(
            "unknown command `{other}` (expected `api-drift`, `spec-doc`, or `freshness`)"
        )),
        None => Err(
            "usage: ls-trackers <api-drift|spec-doc|freshness> <subcommand> [--staged DIR] [--dry-run]"
                .to_string(),
        ),
    }
}

/// Parse a `freshness` subcommand. Only `check` is exposed — the evaluator reads
/// metadata and reports; there is no staged-run or fetch path. `check` accepts an
/// optional `--json` flag selecting the machine-readable contract output.
fn parse_freshness(sub: Option<&str>, rest: &[String]) -> Result<Command, String> {
    match sub {
        Some("check") => {
            let mut json = false;
            for arg in rest {
                match arg.as_str() {
                    "--json" => json = true,
                    other => return Err(format!("unexpected argument `{other}`")),
                }
            }
            Ok(Command::FreshnessCheck { json })
        }
        Some("re-pin") => {
            let mut tr_code: Option<String> = None;
            let mut force = false;
            for arg in rest {
                match arg.as_str() {
                    "--force" => force = true,
                    other if other.starts_with('-') => {
                        return Err(format!("unexpected argument `{other}`"))
                    }
                    other if tr_code.is_none() => tr_code = Some(other.to_string()),
                    other => return Err(format!("unexpected argument `{other}`")),
                }
            }
            let tr_code =
                tr_code.ok_or_else(|| "`re-pin` requires a TR code argument".to_string())?;
            Ok(Command::FreshnessRePin { tr_code, force })
        }
        Some(other) => Err(format!("unknown freshness subcommand `{other}`")),
        None => Err("usage: ls-trackers freshness <check [--json] | re-pin <tr> [--force]>".to_string()),
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
        Some("promote") => parse_promote(rest),
        Some("renormalize") => {
            if let Some(other) = rest.first() {
                return Err(format!("unexpected argument `{other}`"));
            }
            Ok(Command::Renormalize)
        }
        Some(other) => Err(format!("unknown api-drift subcommand `{other}`")),
        None => Err(
            "usage: ls-trackers api-drift <fetch|check|promote (--dry-run | --attest <operator-or-issue>) [--type-only] [--staged DIR]|renormalize>".to_string(),
        ),
    }
}

/// Parse `promote [--dry-run] [--attest <operator-or-issue>] [--staged DIR]`.
///
/// `--dry-run` is the non-mutating preview; `--attest <value>` is the only path
/// that mutates (R4). Invoking promote with **neither** flag is a usage error that
/// writes nothing (AE6). `--dry-run` takes precedence when both are present, so a
/// cautious `promote --dry-run --attest X` still only previews. The attest value
/// must be a non-empty operator/issue string (KTD6); a missing value, or one that
/// looks like the next flag, is rejected.
fn parse_promote(rest: &[String]) -> Result<Command, String> {
    let mut dry_run = false;
    let mut attest: Option<String> = None;
    let mut staged: Option<PathBuf> = None;
    let mut type_only = false;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            "--type-only" => {
                type_only = true;
                i += 1;
            }
            "--attest" => {
                let value = rest.get(i + 1).ok_or_else(|| {
                    "`--attest` requires an operator-or-issue value".to_string()
                })?;
                if value.is_empty() || value.starts_with("--") {
                    return Err(
                        "`--attest` requires a non-empty operator-or-issue value".to_string()
                    );
                }
                attest = Some(value.clone());
                i += 2;
            }
            "--staged" => {
                let dir = rest
                    .get(i + 1)
                    .filter(|d| !d.starts_with("--"))
                    .ok_or_else(|| "`--staged` requires a directory argument".to_string())?;
                staged = Some(PathBuf::from(dir));
                i += 2;
            }
            other => return Err(format!("unexpected argument `{other}`")),
        }
    }
    if dry_run {
        Ok(Command::PromoteDryRun { staged, type_only })
    } else if let Some(attest) = attest {
        Ok(Command::Promote {
            staged,
            attest,
            type_only,
        })
    } else {
        Err(
            "`promote` requires `--dry-run` (preview) or `--attest <operator-or-issue>` (mutate); neither was given"
                .to_string(),
        )
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

/// The committed append-only promotion log (R14), beside `SEED-ATTESTATION.md`.
const PROMOTION_LOG_FILE: &str = "promotion-log.jsonl";

/// Append exactly one [`PromotionRecord`](crate::types::PromotionRecord) as a
/// single JSONL line (R14), preserving every prior record. This is the crate's
/// **first append-mode writer** — every other write is whole-file [`fs::write`].
/// The whole record is serialized through `serde_json::to_string`, so any newline
/// or JSON metacharacter in an operator-supplied field is escaped and cannot
/// inject a second line. The append is the *last* step of a promote (after the
/// baseline files are durable, KTD2), so a missing record on an advanced baseline
/// is a detectable, re-appendable inconsistency rather than a silent loss.
fn append_promotion_record(
    paths: &Paths,
    record: &crate::types::PromotionRecord,
) -> Result<(), String> {
    use std::io::Write;
    let path = paths.baseline_dir.join(PROMOTION_LOG_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }
    let mut line =
        serde_json::to_string(record).map_err(|e| format!("serializing promotion record: {e}"))?;
    line.push('\n');
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("opening {}: {e}", path.display()))?;
    file.write_all(line.as_bytes())
        .map_err(|e| format!("appending {}: {e}", path.display()))
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

    let mut normalized = normalize_run(&raw, &maintained, provisional);
    // Stamp the staged manifest's refresh date with today's UTC date (R9a). This
    // is the live network path (not unit-tested), so the clock read lives here,
    // not inside the pure projection.
    normalized.manifest.refreshed = chrono::Utc::now().date_naive().to_string();
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
///
/// `refreshed` is the baseline-refresh date stamped into the manifest (R9a) — an
/// injected `as_of` seam, never a wall-clock read inside this network-free layer.
/// The operator path passes today's UTC date; tests pass a fixed date.
pub fn renormalize_committed(paths: &Paths, refreshed: &str) -> Result<NormalizedRun, String> {
    let maintained = maintained_codes(paths)?;
    let raw: RawInventory = read_json(&paths.baseline_dir.join(RAW_FILE))?;
    // Preserve the committed seed's provisional stance (KTD-6); default to
    // provisional when no committed code-set exists yet (bootstrap).
    let committed_prev = load_normalized(&paths.baseline_dir).ok();
    let provisional = committed_prev
        .as_ref()
        .map(|run| run.code_set.provisional)
        .unwrap_or(true);
    let mut normalized = normalize_run(&raw, &maintained, provisional);
    // Stamp the injected refresh date into the manifest (R9a) before writing.
    normalized.manifest.refreshed = refreshed.to_string();

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

/// The outcome of a mutating `api-drift promote`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromoteOutcome {
    /// The committed baseline was advanced and one promotion record appended.
    Promoted {
        /// Maintained shapes re-derived into the committed baseline.
        maintained_shapes: usize,
        /// Whether the promoted run carried gated findings (accepted via attest).
        gated: bool,
    },
    /// The staged run gates and no attestation was given — the mutation was refused
    /// and nothing was written (R6, AE2). Maps to exit `1`. Unreachable through the
    /// CLI (parse guarantees a non-empty attest in the `Promote` arm); the
    /// defensive guard is reached only by a direct call with an empty attest.
    RefusedGated,
    /// `--type-only` was set and the maintained-shape drift carried a non-(pure-type
    /// `FieldChanged` | `DescriptionChanged`) change (U3, R1–R3). The mutation was
    /// refused with **zero mutation** before any write; the string is the gate's
    /// block reason. Maps to exit `2` (a non-attestable refusal, not `Exit::Gated`),
    /// independent of `--attest` (R3).
    RefusedTypeOnly(String),
}

/// Run a mutating `api-drift promote` (R1, R5). Pins the staged run, runs the drift
/// gate, and — on a clean diff or an attested gated diff — replaces the committed
/// raw with the staged raw, re-derives the normalized baselines, prunes stale
/// shapes, stamps `manifest.refreshed`, and appends one promotion record **last**
/// (KTD2 ordering).
///
/// **Derive-then-write:** the re-derivation is computed and validated *before* any
/// committed file is written, so a derive/validation failure aborts with zero
/// mutation (exit 2). The multi-file baseline write that follows is per-file
/// [`fs::write`] (not crash-atomic); a crash mid-write is recovered via the git
/// working tree, not in-process rollback. `as_of` is an injected clock seam (the
/// manifest refresh date and the record timestamp; operator passes today, tests
/// pass a fixed date). Promote is assumed single-operator-serial.
pub fn promote_committed(
    paths: &Paths,
    staged: Option<&Path>,
    attest: &str,
    type_only: bool,
    as_of: &str,
) -> Result<PromoteOutcome, String> {
    // 1. Pin the staged run and gate against it (facts-outage gate, version guard,
    //    compare — KTD1). Never a live fetch.
    let run_dir = resolve_staged_run(paths, staged)?;
    let report = run_check(paths, Some(&run_dir))?;

    // 1b. Opt-in type-only gate (U3, R1–R3): admit only a pure field-type wave
    //     (+ DescriptionChanged) on maintained TRs. Refuses with zero mutation
    //     before step 2, independently of --attest (R3 — attesting cannot satisfy
    //     it). The clean-fetch precondition (R4) is already enforced by run_check's
    //     facts-outage gate above, so it is not re-checked here.
    if type_only {
        if let TypeOnlyDecision::Block(reason) = type_only_gate(&report.findings) {
            return Ok(PromoteOutcome::RefusedTypeOnly(reason));
        }
        // The findings-based gate cannot see a maintained shape present in the
        // staged run but absent from the committed baseline (compare() diffs only
        // the shared set), so a newly-maintained-but-unbaselined TR would otherwise
        // ride in un-evaluated. Block any maintained shape-set change so the promote
        // stays scoped to field-type changes within the existing shapes.
        let committed_baseline = load_normalized(&paths.baseline_dir)
            .map_err(|e| format!("committed baseline unavailable: {e}"))?;
        let staged_normalized = load_normalized(&run_dir)
            .map_err(|e| format!("staged run normalized layout unavailable: {e}"))?;
        if let Some(reason) =
            crate::api_drift::type_only_shape_set_block(&committed_baseline, &staged_normalized)
        {
            return Ok(PromoteOutcome::RefusedTypeOnly(reason));
        }
    }

    // 2. Attestation is the only mutation go-ahead (R4/R7), and an empty attest
    //    never writes: on a gated run it is the documented refusal (R6, AE2); on a
    //    clean run it is a misuse. The CLI parse guarantees a non-empty attest in
    //    the Promote arm, so both empty-attest paths are reachable only by a direct
    //    call — but neither is allowed to advance the baseline.
    if attest.is_empty() {
        return if report.gates() {
            Ok(PromoteOutcome::RefusedGated)
        } else {
            Err("promote requires a non-empty `--attest <operator-or-issue>` value".to_string())
        };
    }

    // 3. Whole-raw hash over the on-disk staged raw bytes, captured at resolve time
    //    (the value recorded in the log; KTD3/KTD4). The gate reads normalized
    //    shapes, not the raw, so promote computes this digest itself.
    let raw_path = run_dir.join(RAW_FILE);
    let raw_bytes =
        fs::read(&raw_path).map_err(|e| format!("reading {}: {e}", raw_path.display()))?;
    let raw_hash = crate::api_drift::whole_raw_hash(&raw_bytes);

    // 4. Derive + validate the re-derivation in memory — no committed write yet.
    let staged_raw: RawInventory = serde_json::from_slice(&raw_bytes)
        .map_err(|e| format!("parsing staged raw {}: {e}", raw_path.display()))?;
    let maintained = maintained_codes(paths)?;
    // Preserve the committed code-set's provisional stance (KTD-6); bootstrap=true.
    let provisional = load_normalized(&paths.baseline_dir)
        .ok()
        .map(|run| run.code_set.provisional)
        .unwrap_or(true);
    let mut normalized = normalize_run(&staged_raw, &maintained, provisional);
    normalized.manifest.refreshed = as_of.to_string();
    // Validate: the in-memory re-derivation reproduces the staged run's *persisted*
    // normalized layout — both the code-set and the shapes the gate evaluated and
    // the operator reviewed. Comparing the code-set (not just shapes) is what
    // catches a staged run whose `code-set.json` diverges from its raw (a stale or
    // hand-edited staged run): the gate decided on the persisted code-set, so
    // committing a different raw-derived code-set would advance a baseline the
    // operator never reviewed and record findings that do not match it. A staged
    // run normalized under a different normalizer version also fails here, because
    // the current re-derivation's shapes will not match the older projection.
    // (This is *not* "diff against the old baseline is clean" — that is false by
    // construction for an attested breaking promote, whose new baseline differs.)
    let staged_run = load_normalized(&run_dir)
        .map_err(|e| format!("staged run normalized layout unavailable: {e}"))?;
    // Compare the reviewed *codes* and *shapes* — what the gate evaluated and the
    // operator reviewed. The `provisional` stance is intentionally re-derived from
    // the committed baseline above (KTD-6), so it is expected to differ from an
    // ordinary (non-provisional) staged run and must NOT be compared here; doing
    // so made every attested promote onto a still-provisional baseline diverge.
    if !normalized.code_set.codes_match(&staged_run.code_set)
        || normalized.shapes != staged_run.shapes
    {
        return Err(
            "re-derived code-set/shapes differ from the staged run's reviewed normalized layout — \
             refusing to promote a baseline that diverges from what the gate evaluated"
                .to_string(),
        );
    }

    // 5. Re-read the pinned raw and confirm it is unchanged since resolve (guards
    //    an in-process TOCTOU on the pinned file). Mismatch → zero mutation.
    let raw_bytes_now =
        fs::read(&raw_path).map_err(|e| format!("re-reading {}: {e}", raw_path.display()))?;
    if crate::api_drift::whole_raw_hash(&raw_bytes_now) != raw_hash {
        return Err(
            "staged raw changed between gating and write — aborting with zero mutation".to_string(),
        );
    }

    // 6. Write the committed baseline: raw, then re-derived normalized + prune.
    //    Per-file writes; not crash-atomic — recovery is git + re-run (KTD2).
    fs::create_dir_all(paths.baseline_dir.join("raw")).map_err(|e| e.to_string())?;
    fs::write(paths.baseline_dir.join(RAW_FILE), &raw_bytes_now)
        .map_err(|e| format!("writing committed raw: {e}"))?;
    write_normalized(&paths.baseline_dir, &normalized).map_err(|e| e.to_string())?;
    prune_stale_shapes(&paths.baseline_dir, &normalized)?;

    // 7. Append exactly one promotion record — last, after the baseline is durable.
    let accepted: Vec<crate::types::AcceptedFinding> = report
        .findings
        .iter()
        .filter(|f| f.gates)
        .map(|f| crate::types::AcceptedFinding {
            tr_code: f.tr_code.clone(),
            kind: crate::types::change_kind(&f.change).to_string(),
            severity: f.severity,
        })
        .collect();
    let affected_codes: Vec<String> = promote_affected_codes(&report)
        .into_iter()
        .map(|c| c.to_string())
        .collect();
    let source_run = match staged {
        Some(p) => p.display().to_string(),
        None => run_dir
            .strip_prefix(&paths.run_root)
            .map(|r| r.display().to_string())
            .unwrap_or_else(|_| run_dir.display().to_string()),
    };
    let gated = report.gates();
    let record = crate::types::PromotionRecord {
        promoted_at: as_of.to_string(),
        source_run,
        raw_hash,
        attested_by: attest.to_string(),
        accepted_findings: accepted,
        affected_codes,
        note: None,
    };
    append_promotion_record(paths, &record)?;

    Ok(PromoteOutcome::Promoted {
        maintained_shapes: normalized.shapes.len(),
        gated,
    })
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

/// Run `freshness check`: evaluate the age backstop **and** change-driven staling
/// over Recommended TRs against `as_of`. Loads the validated metadata (for the
/// recommendation + attested-shape evidence map) and the committed baseline (for
/// the per-TR shapes + manifest normalizer version), then runs both rules and
/// merges them (KTD8). Production passes today's UTC date; tests inject a fixed
/// date. Reads only — mutates nothing (R7).
///
/// A whole-run baseline that is absent or unreadable is a loud error (exit 2),
/// never a silent fresh-by-change (KTD8); a missing *per-TR* shape surfaces as a
/// re-attestation advisory inside the evaluator.
pub fn run_freshness_check(
    paths: &Paths,
    as_of: chrono::NaiveDate,
) -> Result<crate::freshness::FreshnessReport, String> {
    let report = validate_dir(&paths.metadata_dir).map_err(|e| format!("metadata error: {e:?}"))?;
    let baseline = load_normalized(&paths.baseline_dir)
        .map_err(|e| format!("committed baseline unavailable: {e}"))?;
    Ok(crate::freshness::evaluate_freshness(
        &report.trs,
        &baseline,
        &report.evidence,
        as_of,
    ))
}

/// Map a `freshness check` result to the tiered exit. Stale evidence is advisory
/// (`Severity::Evidence` never gates), so a run with only stale findings is exit
/// `0`. A metadata load error, or a Recommended TR whose `last_reviewed` could not
/// be parsed (freshness genuinely could not be evaluated), exits `2`.
pub fn freshness_exit_for(result: &Result<crate::freshness::FreshnessReport, String>) -> Exit {
    match result {
        Ok(report) if report.has_errors() => Exit::Error,
        Ok(_) => Exit::Ok,
        Err(_) => Exit::Error,
    }
}

/// The outcome of a `freshness re-pin` run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RePinOutcome {
    /// The attested shape was (re)written from the current committed baseline.
    Pinned { tr_code: String },
    /// The TR already carried an attested shape and `--force` was not given —
    /// left untouched so a standing stale-by-change signal is not silently cleared.
    AlreadyPinned { tr_code: String },
}

/// Run `freshness re-pin <tr>`: capture the current committed baseline shape +
/// manifest `normalizer_version` into the named Recommended TR's Focused Evidence
/// record as its attested shape (R11). Reads only the committed baseline and the
/// metadata tree — no network. **Populate-if-absent**: refuses to overwrite an
/// existing attested shape unless `force` is set, so a routine re-run never clears
/// a genuine, intentionally-standing stale-by-change signal.
pub fn run_freshness_repin(
    paths: &Paths,
    tr_code: &str,
    force: bool,
) -> Result<RePinOutcome, String> {
    let baseline = load_normalized(&paths.baseline_dir)
        .map_err(|e| format!("committed baseline unavailable: {e}"))?;
    let shape = baseline.shapes.get(tr_code).ok_or_else(|| {
        format!(
            "no committed baseline shape for `{tr_code}` — re-fetch/re-seed the baseline before re-pinning"
        )
    })?;
    let version = baseline.manifest.normalizer_version;

    // Resolve the evidence path by reading just this TR's file (not the whole
    // metadata tree). Re-pin must NOT depend on `validate_dir`: the U7 validator
    // requires a recommended TR to already carry an attested shape, and re-pin is
    // the tool that *populates* it — routing through validation would make pinning
    // a never-pinned TR (backfill, recovery) impossible.
    let tr_path = paths.metadata_dir.join("trs").join(format!("{tr_code}.yaml"));
    let tr_yaml = fs::read_to_string(&tr_path)
        .map_err(|e| format!("reading {}: {e}", tr_path.display()))?;
    let meta = ls_metadata::parse_tr_metadata(tr_code, &tr_path, &tr_yaml)
        .map_err(|e| format!("parsing {}: {e}", tr_path.display()))?;
    if !meta.support.recommended {
        return Err(format!(
            "TR `{tr_code}` is not Recommended — re-pin applies to Recommended TRs only"
        ));
    }
    let rec = meta
        .recommendation
        .as_ref()
        .ok_or_else(|| format!("TR `{tr_code}` has no recommendation block"))?;
    let evidence_path = paths.metadata_dir.join(&rec.evidence_ref);

    let original = fs::read_to_string(&evidence_path)
        .map_err(|e| format!("reading {}: {e}", evidence_path.display()))?;
    let record: ls_metadata::EvidenceRecord = serde_yaml::from_str(&original)
        .map_err(|e| format!("parsing {}: {e}", evidence_path.display()))?;

    if record.attested_shape.is_some() && !force {
        return Ok(RePinOutcome::AlreadyPinned {
            tr_code: tr_code.to_string(),
        });
    }

    let updated = upsert_attested_fields(&original, shape, version)?;
    fs::write(&evidence_path, updated)
        .map_err(|e| format!("writing {}: {e}", evidence_path.display()))?;
    Ok(RePinOutcome::Pinned {
        tr_code: tr_code.to_string(),
    })
}

/// Rewrite an evidence YAML's `attested_shape` + `attested_normalizer_version`
/// fields, preserving every other line (including the secret-safety comment
/// header) verbatim. Any pre-existing attested fields are stripped first (the
/// force-replace path), then the fresh pair is appended — so a re-pin is
/// idempotent in structure and never accumulates stale blocks.
fn upsert_attested_fields(
    original: &str,
    shape: &TrShape,
    version: u32,
) -> Result<String, String> {
    // Drop any existing attested fields, keeping all other lines untouched.
    let mut kept = String::new();
    let mut in_shape_block = false;
    for line in original.lines() {
        if in_shape_block {
            // The `attested_shape:` value is an indented block; consume its
            // indented (and blank) lines until a column-0 line ends it.
            if line.is_empty() || line.starts_with(char::is_whitespace) {
                continue;
            }
            in_shape_block = false; // fall through to handle this column-0 line
        }
        if line.starts_with("attested_shape:") {
            in_shape_block = true;
            continue;
        }
        if line.starts_with("attested_normalizer_version:") {
            continue;
        }
        kept.push_str(line);
        kept.push('\n');
    }
    let kept = kept.trim_end();

    // Serialize the shape and indent it two spaces under `attested_shape:`.
    let shape_yaml =
        serde_yaml::to_string(shape).map_err(|e| format!("serializing attested shape: {e}"))?;
    let shape_yaml = shape_yaml.strip_prefix("---\n").unwrap_or(&shape_yaml);
    let mut indented = String::new();
    for line in shape_yaml.lines() {
        if line.is_empty() {
            indented.push('\n');
        } else {
            indented.push_str("  ");
            indented.push_str(line);
            indented.push('\n');
        }
    }

    Ok(format!(
        "{kept}\n\nattested_normalizer_version: {version}\nattested_shape:\n{indented}"
    ))
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

/// The portable pointer `fetch` writes (and `promote` now reads) to select the
/// most recent staged run by default (R2).
const LATEST_FILE: &str = "latest.txt";

/// Resolve the staged run a promote (or its dry-run) pins and acts on (R2). An
/// explicit `--staged DIR` wins; otherwise the net-new reader follows the
/// `latest.txt` pointer (`runs/{name}`) under the run root. Promote **pins** — it
/// never live-fetches — so a missing/empty pointer or a run lacking its raw
/// evidence is a hard error, not a silent fall-back to the network. The resolved
/// directory is handed to [`run_check`] so the gate and the write act on the same
/// bytes.
fn resolve_staged_run(paths: &Paths, staged: Option<&Path>) -> Result<PathBuf, String> {
    let dir = match staged {
        Some(d) => d.to_path_buf(),
        None => {
            let latest = paths.run_root.join(LATEST_FILE);
            let pointer = fs::read_to_string(&latest).map_err(|e| {
                format!(
                    "no staged run selected: reading {}: {e} — run `api-drift fetch` first or pass `--staged DIR`",
                    latest.display()
                )
            })?;
            let rel = pointer.trim();
            if rel.is_empty() {
                return Err(format!(
                    "{} is empty — no staged run to promote",
                    latest.display()
                ));
            }
            paths.run_root.join(rel)
        }
    };
    if !dir.join(RAW_FILE).is_file() {
        return Err(format!(
            "staged run `{}` has no `{}` — not a complete staged run",
            dir.display(),
            RAW_FILE
        ));
    }
    Ok(dir)
}

/// Write `latest.txt` pointing at the most recent run (a portable relative path).
fn update_latest(paths: &Paths, run_dir: &Path) -> Result<(), String> {
    let name = run_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let latest = paths.run_root.join(LATEST_FILE);
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
        Command::Renormalize => match renormalize_committed(
            paths,
            &crate::freshness::today().to_string(),
        ) {
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
        Command::PromoteDryRun { staged, type_only } => {
            // Pin the same run a real promote would (R3) — never a live fetch — and
            // apply the gate before reporting, so dry-run mirrors `check`/`fetch`.
            match resolve_staged_run(paths, staged.as_deref()) {
                Ok(run_dir) => {
                    let result = run_check(paths, Some(&run_dir));
                    match &result {
                        Ok(report) => {
                            let raw_hash = fs::read(run_dir.join(RAW_FILE))
                                .ok()
                                .map(|b| crate::api_drift::whole_raw_hash(&b));
                            print_promote_dry_run(&run_dir, raw_hash.as_deref(), report);
                        }
                        Err(e) => eprintln!("error: {e}"),
                    }
                    // With --type-only, also preview the type-only gate decision
                    // (U3): a block would refuse a real promote with exit 2, so the
                    // dry-run surfaces it as exit 2 too; otherwise the exit follows
                    // the drift gate (the type wave itself gates → exit 1, the
                    // operator's signal to pass --attest). Writes nothing either way.
                    if type_only {
                        if let Ok(report) = &result {
                            match type_only_gate(&report.findings) {
                                TypeOnlyDecision::Admit => {
                                    println!(
                                        "type-only gate: ADMIT — maintained drift is a pure \
                                         field-type wave (+ description changes); a \
                                         `--type-only --attest` promote would proceed."
                                    );
                                }
                                TypeOnlyDecision::Block(reason) => {
                                    eprintln!("type-only gate: BLOCKED — {reason}");
                                    return Exit::Error;
                                }
                            }
                        }
                    }
                    exit_for(&result)
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    Exit::Error
                }
            }
        }
        Command::Promote {
            staged,
            attest,
            type_only,
        } => {
            match promote_committed(
                paths,
                staged.as_deref(),
                &attest,
                type_only,
                &crate::freshness::today().to_string(),
            ) {
                Ok(PromoteOutcome::Promoted {
                    maintained_shapes,
                    gated,
                }) => {
                    println!(
                        "promoted: committed baseline advanced ({maintained_shapes} maintained \
                         shape(s){}); 1 promotion record appended.",
                        if gated {
                            ", gated findings attested"
                        } else {
                            ""
                        }
                    );
                    Exit::Ok
                }
                Ok(PromoteOutcome::RefusedGated) => {
                    eprintln!(
                        "refused: staged run gates on Tracker Findings; pass \
                         `--attest <operator-or-issue>` to acknowledge and promote"
                    );
                    Exit::Gated
                }
                Ok(PromoteOutcome::RefusedTypeOnly(reason)) => {
                    eprintln!(
                        "refused: --type-only promotion blocked ({reason}); the drift is not a \
                         pure field-type wave — route the non-type drift to a separate \
                         Maintenance Review Decision (attesting cannot satisfy this gate)"
                    );
                    Exit::Error
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
        Command::FreshnessCheck { json } => {
            let as_of = crate::freshness::today();
            let result = run_freshness_check(paths, as_of);
            match &result {
                Ok(report) if json => print!(
                    "{}",
                    crate::freshness::report_to_json(
                        report,
                        as_of,
                        ls_metadata::DEFAULT_WINDOW_DAYS
                    )
                ),
                Ok(report) => print_freshness_report(report),
                // A metadata load error has no report to serialize; the workflow
                // reads the non-zero exit, not stdout, as the failure signal.
                Err(e) => eprintln!("error: {e}"),
            }
            freshness_exit_for(&result)
        }
        Command::FreshnessRePin { tr_code, force } => match run_freshness_repin(paths, &tr_code, force) {
            Ok(RePinOutcome::Pinned { tr_code }) => {
                println!(
                    "re-pinned attested shape for `{tr_code}` to the current committed baseline."
                );
                Exit::Ok
            }
            Ok(RePinOutcome::AlreadyPinned { tr_code }) => {
                println!(
                    "`{tr_code}` already has an attested shape; left unchanged (pass --force to \
                     re-pin during a real re-attestation)."
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

/// Print an advisory freshness report: each stale Recommended TR with its age past
/// the 90-day backstop, plus an "N of M stale" summary. Like the spec-doc report,
/// there is no `GATE` column — stale evidence is a re-attestation candidate, not a
/// failure (`Severity::Evidence` never gates).
fn print_freshness_report(report: &crate::freshness::FreshnessReport) {
    if report.findings.is_empty() {
        println!(
            "no stale evidence: {} of {} Recommended TR(s) within the 90-day backstop.",
            report.recommended_count, report.recommended_count
        );
    } else {
        println!(
            "{} of {} Recommended TR(s) stale (re-attest: rerun smoke, update evidence + last_reviewed, regenerate docs):",
            report.findings.len(),
            report.recommended_count
        );
        for f in &report.findings {
            println!("  {f}");
        }
    }
    if report.baseline.stale {
        let detail = match report.baseline.age_days {
            Some(age) => format!("{age} days old (refreshed {})", report.baseline.refreshed),
            None if report.baseline.refreshed.is_empty() => "never stamped".to_string(),
            None => format!("unparseable refresh date `{}`", report.baseline.refreshed),
        };
        println!(
            "advisory: committed baseline is stale ({detail}); change-detection compares against \
             possibly-outdated structural truth — re-fetch/re-seed the baseline."
        );
    }
    if report.has_reattest() {
        println!(
            "advisory: {} Recommended TR(s) need re-attestation (normalizer-version mismatch or \
             missing baseline shape; re-pin against the current baseline): {}",
            report.reattest.len(),
            report.reattest.join(", ")
        );
    }
    if report.has_errors() {
        println!(
            "error: {} Recommended TR(s) have an unparseable last_reviewed (freshness not evaluated): {}",
            report.unparseable.len(),
            report.unparseable.join(", ")
        );
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

/// Print the `promote --dry-run` preview (R3): the pinned staged run, its whole-raw
/// digest, the gated findings a real promote would attest, and the TR codes that
/// carry drift. Writes nothing; the caller's exit code follows the gate. This
/// reports what a real (narrowed) promote actually changes — raw + normalized — not
/// the wider advisory `promote_targets` superset (metadata/docs are human
/// follow-up, KTD7).
fn print_promote_dry_run(run_dir: &Path, raw_hash: Option<&str>, report: &DriftReport) {
    println!(
        "promote --dry-run (writes nothing). Pinned staged run: {}",
        run_dir.display()
    );
    if let Some(h) = raw_hash {
        println!("  raw hash: {h}");
    }
    let gated: Vec<&crate::types::DriftFinding> =
        report.findings.iter().filter(|f| f.gates).collect();
    if gated.is_empty() {
        println!("  gated findings: none — a real promote would proceed cleanly under --attest.");
    } else {
        println!("  gated findings ({}):", gated.len());
        for f in &gated {
            println!("    [{}] {} {:?}", f.severity, f.tr_code, f.change);
        }
    }
    let affected = promote_affected_codes(report);
    if affected.is_empty() {
        println!("  TR codes with drift: none");
    } else {
        let codes: Vec<&str> = affected.into_iter().collect();
        println!("  TR codes with drift: {}", codes.join(", "));
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
            Command::PromoteDryRun {
                staged: None,
                type_only: false
            }
        );
        assert_eq!(
            parse_args(args(&["api-drift", "renormalize"])).unwrap(),
            Command::Renormalize
        );
    }

    /// U1 (R4, AE6): promote parses `--attest <value>` into the mutating variant,
    /// threads `--staged`, lets `--dry-run` win when both are present, and treats
    /// "neither flag" / an empty-or-flag-shaped attest value as a usage error.
    #[test]
    fn parses_promote_attest_and_rejects_bare_invocation() {
        assert_eq!(
            parse_args(args(&["api-drift", "promote", "--attest", "ENG-123"])).unwrap(),
            Command::Promote {
                staged: None,
                attest: "ENG-123".to_string(),
                type_only: false
            }
        );
        assert_eq!(
            parse_args(args(&[
                "api-drift", "promote", "--attest", "ENG-123", "--staged", "/tmp/run"
            ]))
            .unwrap(),
            Command::Promote {
                staged: Some(PathBuf::from("/tmp/run")),
                attest: "ENG-123".to_string(),
                type_only: false
            }
        );
        // --dry-run wins over --attest (cautious preview).
        assert_eq!(
            parse_args(args(&["api-drift", "promote", "--dry-run", "--attest", "ENG-1"])).unwrap(),
            Command::PromoteDryRun {
                staged: None,
                type_only: false
            }
        );
        // AE6: neither flag → usage error, no command produced.
        assert!(
            parse_args(args(&["api-drift", "promote"])).is_err(),
            "neither --dry-run nor --attest is a usage error"
        );
        // --attest with no value, or a flag-shaped value, is rejected.
        assert!(parse_args(args(&["api-drift", "promote", "--attest"])).is_err());
        assert!(
            parse_args(args(&["api-drift", "promote", "--attest", "--staged", "/x"])).is_err(),
            "a flag-shaped attest value is rejected"
        );
        // --staged must not swallow a following flag (e.g. the --dry-run safety flag).
        assert!(
            parse_args(args(&["api-drift", "promote", "--attest", "E", "--staged", "--dry-run"]))
                .is_err(),
            "a flag-shaped --staged value is rejected, not swallowed"
        );
    }

    /// U3 (R1): `--type-only` is a value-less opt-in flag that composes with both
    /// `--attest` (mutate) and `--dry-run` (preview), and defaults off.
    #[test]
    fn parses_promote_type_only_flag() {
        assert_eq!(
            parse_args(args(&["api-drift", "promote", "--type-only", "--attest", "ENG-1"])).unwrap(),
            Command::Promote {
                staged: None,
                attest: "ENG-1".to_string(),
                type_only: true
            }
        );
        assert_eq!(
            parse_args(args(&["api-drift", "promote", "--type-only", "--dry-run"])).unwrap(),
            Command::PromoteDryRun {
                staged: None,
                type_only: true
            }
        );
        // Flag order is irrelevant; --type-only composes with --staged too.
        assert_eq!(
            parse_args(args(&[
                "api-drift", "promote", "--attest", "ENG-1", "--type-only", "--staged", "/tmp/run"
            ]))
            .unwrap(),
            Command::Promote {
                staged: Some(PathBuf::from("/tmp/run")),
                attest: "ENG-1".to_string(),
                type_only: true
            }
        );
        // Defaults off when absent.
        assert_eq!(
            parse_args(args(&["api-drift", "promote", "--attest", "ENG-1"])).unwrap(),
            Command::Promote {
                staged: None,
                attest: "ENG-1".to_string(),
                type_only: false
            }
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

        let run = renormalize_committed(&paths, "2026-06-20").expect("re-normalize from committed raw");
        assert_eq!(
            run.manifest.normalizer_version,
            crate::api_drift::NORMALIZER_VERSION
        );
        assert_eq!(run.manifest.normalizer_version, 2);
        // R9a: the injected refresh date is stamped into the manifest, and the
        // written manifest carries it (read back from disk).
        assert_eq!(run.manifest.refreshed, "2026-06-20");
        let written: Manifest =
            read_json(&scratch.join(MANIFEST_FILE)).expect("written manifest reloads");
        assert_eq!(written.refreshed, "2026-06-20");
        assert_eq!(run.shapes.len(), 307, "three-hundred-seven maintained shapes");
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
        renormalize_committed(&paths, "2026-06-20").expect("second re-normalize");
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
        let run = renormalize_committed(&paths, "2026-06-20").expect("re-normalize");

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
        assert_eq!(run.shapes.len(), 307);
    }

    fn empty_run(codes: &[&str]) -> NormalizedRun {
        NormalizedRun {
            code_set: CodeSet::new(codes.iter().map(|c| c.to_string()), false),
            manifest: Manifest {
                upstream_tr_count: codes.len(),
                maintained_tr_count: 0,
                source_urls: vec![],
                normalizer_version: 1,
                refreshed: "2026-06-20".to_string(),
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

    // --- freshness check (U3) ---

    fn real_metadata_paths(root: &std::path::Path) -> Paths {
        let metadata = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("metadata");
        Paths {
            // The real committed baseline: change-driven detection diffs the
            // backfilled attested shapes against it (attested == baseline, so the
            // six start fresh-by-change; only the age rule varies by as_of).
            baseline_dir: committed_baseline_dir(),
            run_root: root.join("runs"),
            metadata_dir: metadata,
            spec_baseline_dir: root.join("spec-doc"),
        }
    }

    #[test]
    fn freshness_parse_accepts_check_rejects_others() {
        assert_eq!(
            parse_args(["freshness".to_string(), "check".to_string()]).unwrap(),
            Command::FreshnessCheck { json: false }
        );
        // `--json` is accepted and threaded through (the workflow contract).
        assert_eq!(
            parse_args([
                "freshness".to_string(),
                "check".to_string(),
                "--json".to_string()
            ])
            .unwrap(),
            Command::FreshnessCheck { json: true }
        );
        assert!(parse_args(["freshness".to_string()]).is_err());
        assert!(parse_args(["freshness".to_string(), "wat".to_string()]).is_err());
        assert!(parse_args([
            "freshness".to_string(),
            "check".to_string(),
            "--nope".to_string()
        ])
        .is_err());
    }

    #[test]
    fn freshness_check_stale_lists_findings_and_exits_zero() {
        let root = scratch("freshness-stale");
        let paths = real_metadata_paths(&root);
        // Inject an as-of well past the 90-day window for every Recommended TR.
        let result = run_freshness_check(&paths, chrono::NaiveDate::from_ymd_opt(2026, 10, 1).unwrap());
        let report = result.as_ref().unwrap();
        assert_eq!(report.findings.len(), 6);
        assert_eq!(report.recommended_count, 6);
        // Advisory — stale evidence never gates.
        assert_eq!(freshness_exit_for(&result), Exit::Ok);
    }

    #[test]
    fn freshness_check_all_fresh_exits_zero_with_no_findings() {
        let root = scratch("freshness-fresh");
        let paths = real_metadata_paths(&root);
        let result =
            run_freshness_check(&paths, chrono::NaiveDate::from_ymd_opt(2026, 6, 18).unwrap());
        assert!(result.as_ref().unwrap().findings.is_empty());
        assert_eq!(freshness_exit_for(&result), Exit::Ok);
    }

    #[test]
    fn freshness_check_metadata_error_exits_two() {
        let root = scratch("freshness-err");
        let mut paths = real_metadata_paths(&root);
        paths.metadata_dir = root.join("does-not-exist");
        let result =
            run_freshness_check(&paths, chrono::NaiveDate::from_ymd_opt(2026, 10, 1).unwrap());
        assert!(result.is_err());
        assert_eq!(freshness_exit_for(&result), Exit::Error);
    }

    #[test]
    fn freshness_check_mutates_nothing() {
        // AE6: the authored metadata + evidence are byte-identical after a stale run.
        let root = scratch("freshness-nomutate");
        let paths = real_metadata_paths(&root);
        let token_tr = paths.metadata_dir.join("trs").join("token.yaml");
        let token_ev = paths.metadata_dir.join("evidence").join("token.yaml");
        let before_tr = fs::read(&token_tr).unwrap();
        let before_ev = fs::read(&token_ev).unwrap();
        let _ =
            run_freshness_check(&paths, chrono::NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()).unwrap();
        assert_eq!(fs::read(&token_tr).unwrap(), before_tr, "trs/token.yaml unchanged");
        assert_eq!(
            fs::read(&token_ev).unwrap(),
            before_ev,
            "evidence/token.yaml unchanged"
        );
    }

    // --- U8: re-pin mechanism -----------------------------------------------

    #[test]
    fn parses_freshness_re_pin() {
        assert_eq!(
            parse_args(args(&["freshness", "re-pin", "token"])).unwrap(),
            Command::FreshnessRePin {
                tr_code: "token".to_string(),
                force: false
            }
        );
        assert_eq!(
            parse_args(args(&["freshness", "re-pin", "token", "--force"])).unwrap(),
            Command::FreshnessRePin {
                tr_code: "token".to_string(),
                force: true
            }
        );
        assert!(
            parse_args(args(&["freshness", "re-pin"])).is_err(),
            "re-pin needs a TR code"
        );
    }

    /// A recommended `token` TR fixture whose `last_reviewed` matches the minimal
    /// evidence date below, so the validator passes over the scratch metadata.
    const REPIN_TOKEN_TR: &str = r#"
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

    /// U8: re-pin captures the committed baseline shape + manifest version into the
    /// evidence record (attested == baseline at backfill), is populate-if-absent on
    /// a re-run, overwrites under `--force`, and never accumulates attested blocks.
    /// The secret-safety comment header survives every rewrite.
    #[test]
    fn re_pin_captures_baseline_shape_populate_if_absent_and_force_overwrites() {
        let root = scratch("repin");
        let baseline = root.join("baseline");
        let metadata = root.join("metadata");

        // Minimal committed baseline with one recommended TR shape.
        let shape = TrShape {
            tr_code: "token".to_string(),
            tr_name: None,
            protocol: crate::types::Protocol::Rest,
            is_websocket: false,
            endpoint_path: Some("/oauth2/token".to_string()),
            api_group_id: None,
            source_group_name: None,
            request_blocks: vec![crate::types::BlockField {
                direction: crate::types::Direction::Request,
                block_name: "request_body".to_string(),
                field_index: 0,
                field_name: "grant_type".to_string(),
                korean_name: None,
                r#type: Some("String".to_string()),
                length: Some(100),
                required: true,
                description_hash: Some("abc".to_string()),
            }],
            response_blocks: vec![],
            rate_limit_per_sec: None,
            corp_rate_limit_per_sec: None,
            rate_source_group: None,
            description_hash: None,
        };
        let mut shapes = BTreeMap::new();
        shapes.insert("token".to_string(), shape.clone());
        let run = NormalizedRun {
            code_set: CodeSet::new(["token".to_string()], false),
            manifest: Manifest {
                upstream_tr_count: 1,
                maintained_tr_count: 1,
                source_urls: vec![],
                normalizer_version: 2,
                refreshed: "2026-06-20".to_string(),
            },
            shapes,
        };
        write_normalized(&baseline, &run).unwrap();

        // Minimal metadata tree: index + recommended token + commented evidence.
        fs::create_dir_all(metadata.join("trs")).unwrap();
        fs::create_dir_all(metadata.join("evidence")).unwrap();
        fs::write(
            metadata.join("tr-index.yaml"),
            "version: 1\ntrs:\n  token:\n    file: trs/token.yaml\n    owner_class: standalone\n    protocol: rest\n    instrument_domain: misc\n    venue_session: unspecified\n",
        )
        .unwrap();
        fs::write(metadata.join("trs/token.yaml"), REPIN_TOKEN_TR).unwrap();
        fs::write(
            metadata.join("evidence/token.yaml"),
            "# secret-safety header — must survive re-pin\ntr_code: token\ndate: 2026-06-16\nenv: paper\n",
        )
        .unwrap();

        let paths = Paths {
            baseline_dir: baseline,
            run_root: root.join("runs"),
            metadata_dir: metadata.clone(),
            spec_baseline_dir: root.join("spec"),
        };
        let ev_path = metadata.join("evidence/token.yaml");

        // First re-pin populates from the baseline.
        assert_eq!(
            run_freshness_repin(&paths, "token", false).unwrap(),
            RePinOutcome::Pinned { tr_code: "token".to_string() }
        );
        let ev: ls_metadata::EvidenceRecord =
            serde_yaml::from_str(&fs::read_to_string(&ev_path).unwrap()).unwrap();
        assert_eq!(ev.attested_normalizer_version, Some(2));
        assert_eq!(ev.attested_shape.as_ref(), Some(&shape), "attested == baseline");
        assert!(
            fs::read_to_string(&ev_path).unwrap().contains("secret-safety header"),
            "comment header preserved"
        );

        // Populate-if-absent: a re-run without --force is a no-op.
        assert_eq!(
            run_freshness_repin(&paths, "token", false).unwrap(),
            RePinOutcome::AlreadyPinned { tr_code: "token".to_string() }
        );

        // --force overwrites and does not accumulate a second attested block.
        assert_eq!(
            run_freshness_repin(&paths, "token", true).unwrap(),
            RePinOutcome::Pinned { tr_code: "token".to_string() }
        );
        let text = fs::read_to_string(&ev_path).unwrap();
        assert_eq!(
            text.matches("attested_shape:").count(),
            1,
            "force re-pin replaces, never accumulates"
        );
        assert_eq!(
            text.matches("attested_normalizer_version:").count(),
            1,
            "single normalizer-version line after force re-pin"
        );

        // A non-recommended / unknown TR is rejected loudly.
        assert!(run_freshness_repin(&paths, "nope", false).is_err());
    }

    // --- U4: promotion log writer -------------------------------------------

    fn promo_paths(scratch: &Path) -> Paths {
        Paths {
            baseline_dir: scratch.to_path_buf(),
            run_root: scratch.join("runs"),
            metadata_dir: repo_metadata_dir(),
            spec_baseline_dir: scratch.join("spec-doc"),
        }
    }

    fn sample_record(attested_by: &str) -> crate::types::PromotionRecord {
        crate::types::PromotionRecord {
            promoted_at: "2026-06-21".to_string(),
            source_run: "runs/2026-06-21T00-00-00Z".to_string(),
            raw_hash: "0123456789abcdef".to_string(),
            attested_by: attested_by.to_string(),
            accepted_findings: vec![crate::types::AcceptedFinding {
                tr_code: "t1102".to_string(),
                kind: "field_removed".to_string(),
                severity: crate::types::Severity::Breaking,
            }],
            affected_codes: vec!["t1102".to_string()],
            note: None,
        }
    }

    /// U4 (R14): the writer creates the log on first append, preserves prior lines
    /// on subsequent appends (no accumulation into existing records), every line
    /// round-trips back into a `PromotionRecord`, and a clean record carries an
    /// empty accepted-findings list rather than a fabricated entry.
    #[test]
    fn promotion_log_appends_one_line_and_preserves_prior() {
        let scratch = scratch("promo-log");
        let paths = promo_paths(&scratch);
        let log = scratch.join(PROMOTION_LOG_FILE);

        append_promotion_record(&paths, &sample_record("ENG-1")).unwrap();
        let first = fs::read_to_string(&log).unwrap();
        assert_eq!(first.lines().count(), 1, "first append creates exactly one line");

        append_promotion_record(&paths, &sample_record("ENG-2")).unwrap();
        let second = fs::read_to_string(&log).unwrap();
        assert_eq!(second.lines().count(), 2, "second append yields two lines");
        assert!(
            second.starts_with(&first),
            "the first record's bytes are preserved verbatim by the second append"
        );

        // Every line round-trips into a PromotionRecord (the JSONL contract).
        for line in second.lines() {
            let _: crate::types::PromotionRecord =
                serde_json::from_str(line).expect("each JSONL line round-trips");
        }

        // A clean promote's record serializes an empty accepted-findings list, not
        // a fabricated entry.
        let mut clean = sample_record("ENG-3");
        clean.accepted_findings.clear();
        append_promotion_record(&paths, &clean).unwrap();
        let last: crate::types::PromotionRecord =
            serde_json::from_str(fs::read_to_string(&log).unwrap().lines().last().unwrap()).unwrap();
        assert!(last.accepted_findings.is_empty());
    }

    /// U4 (injection resistance + secret-safety): an `attested_by` value carrying
    /// an embedded newline and JSON metacharacters serializes as exactly one valid
    /// JSONL line that round-trips cleanly — serde escaping is the one-line-per-record
    /// guarantee — and a value that looks like a credential lands only inside the
    /// escaped `attested_by` field, never as a structural value that could leak
    /// elsewhere.
    #[test]
    fn promotion_log_escapes_metacharacters_into_one_line() {
        let scratch = scratch("promo-inject");
        let paths = promo_paths(&scratch);
        let log = scratch.join(PROMOTION_LOG_FILE);

        let nasty = "ENG-9\n{\"injected\":true}\t\"quoted\"";
        append_promotion_record(&paths, &sample_record(nasty)).unwrap();

        let text = fs::read_to_string(&log).unwrap();
        assert_eq!(
            text.lines().count(),
            1,
            "an embedded newline does not split the record into two log lines"
        );
        let back: crate::types::PromotionRecord =
            serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!(back.attested_by, nasty, "the value round-trips exactly");
    }

    // --- U5 / U6: mutating promote + dry-run --------------------------------

    /// The maintained TR set from the real repo metadata (the keys promote
    /// re-derives shapes for).
    fn maintained_set() -> BTreeSet<String> {
        ls_metadata::validate_dir(&repo_metadata_dir())
            .unwrap()
            .trs
            .keys()
            .cloned()
            .collect()
    }

    /// Write a self-consistent staged run (`raw` + its `normalize_run` projection +
    /// an ok fetch-report) so promote's re-derivation matches the staged normalized.
    /// Returns the on-disk staged raw bytes (the bytes a promote should copy).
    fn write_staged_from_raw(dir: &Path, raw: &RawInventory) -> Vec<u8> {
        let maintained = maintained_set();
        let mut normalized = normalize_run(raw, &maintained, true);
        normalized.manifest.refreshed = "2026-06-21".to_string();
        let report = FetchReport {
            ok: true,
            fetched_count: normalized.code_set.len(),
            committed_code_set_len: Some(normalized.code_set.len()),
            facts_degraded_groups: 0,
            degraded_tr_codes: BTreeSet::new(),
            property_type_fallback_served: false,
            failure: None,
        };
        write_staged_run(dir, raw, &normalized, &report).unwrap();
        fs::read(dir.join(RAW_FILE)).unwrap()
    }

    /// The committed baseline's real raw inventory.
    fn committed_raw() -> RawInventory {
        serde_json::from_slice(&fs::read(committed_baseline_dir().join(RAW_FILE)).unwrap()).unwrap()
    }

    /// A minimal untracked `RawTr` used to inject a new upstream TR (gates).
    fn new_raw_tr(code: &str) -> crate::fetch::RawTr {
        crate::fetch::RawTr {
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
            req_example: Value::Null,
            res_example: Value::Null,
        }
    }

    /// Covers AE1 / R1 / R6. A clean staged run + `--attest` advances the committed
    /// baseline: the gate runs before any write, the committed raw is replaced
    /// byte-for-byte by the staged raw, the normalized layout is re-derived (with
    /// `refreshed` stamped) and a stale shape is pruned, and exactly one promotion
    /// record (no accepted findings) is appended. The post-promote self-diff is
    /// clean.
    #[test]
    fn promote_clean_run_advances_baseline_and_appends_record() {
        let scratch = scratch("promote-clean");
        let staged = scratch.join("staged");
        let raw_bytes = write_staged_from_raw(&staged, &committed_raw());

        let paths = Paths {
            baseline_dir: scratch.join("baseline"),
            run_root: scratch.join("runs"),
            metadata_dir: repo_metadata_dir(),
            spec_baseline_dir: scratch.join("spec"),
        };
        // Seed committed normalized from the staged run (clean self-diff), plus a
        // sentinel committed raw (different) to prove promote replaces it, and a
        // ghost shape to prove the prune pass runs.
        let staged_run = load_normalized(&staged).unwrap();
        write_normalized(&paths.baseline_dir, &staged_run).unwrap();
        fs::create_dir_all(paths.baseline_dir.join("raw")).unwrap();
        fs::write(paths.baseline_dir.join(RAW_FILE), b"{\"sentinel\":true}\n").unwrap();
        fs::write(
            paths.baseline_dir.join(TRS_DIR).join("ghost.json"),
            br#"{"tr_code":"ghost","protocol":"rest","is_websocket":false}"#,
        )
        .unwrap();

        // An empty attest on a clean run is a misuse: it errors and writes nothing
        // (no empty-`attested_by` record can ever be committed, even off the CLI).
        assert!(promote_committed(&paths, Some(&staged), "", false, "2026-06-21").is_err());
        assert!(!paths.baseline_dir.join(PROMOTION_LOG_FILE).exists());

        let outcome = promote_committed(&paths, Some(&staged), "ENG-1", false, "2026-06-21").unwrap();
        assert!(
            matches!(outcome, PromoteOutcome::Promoted { gated: false, maintained_shapes } if maintained_shapes == 307)
        );

        // Raw replaced byte-for-byte by the staged raw (R1, R9).
        assert_eq!(fs::read(paths.baseline_dir.join(RAW_FILE)).unwrap(), raw_bytes);
        // Normalized re-derived with the injected refresh date (KTD8); ghost pruned.
        let committed = load_normalized(&paths.baseline_dir).unwrap();
        assert_eq!(committed.manifest.refreshed, "2026-06-21");
        assert_eq!(committed.shapes.len(), 307);
        assert!(
            !paths.baseline_dir.join(TRS_DIR).join("ghost.json").exists(),
            "a stale shape is pruned by promote (R10)"
        );
        // Exactly one promotion record, no accepted findings on a clean promote.
        let log = fs::read_to_string(paths.baseline_dir.join(PROMOTION_LOG_FILE)).unwrap();
        assert_eq!(log.lines().count(), 1);
        let rec: crate::types::PromotionRecord =
            serde_json::from_str(log.lines().next().unwrap()).unwrap();
        assert_eq!(rec.attested_by, "ENG-1");
        assert!(rec.accepted_findings.is_empty());
        assert_eq!(rec.raw_hash, crate::api_drift::whole_raw_hash(&raw_bytes));

        // Self-diff invariant (clean path): re-checking the staged run is clean.
        assert!(!run_check(&paths, Some(&staged)).unwrap().gates());

        // No-accumulation: a second identical promote appends a second line and
        // leaves one raw/manifest, not duplicates.
        promote_committed(&paths, Some(&staged), "ENG-2", false, "2026-06-21").unwrap();
        let log2 = fs::read_to_string(paths.baseline_dir.join(PROMOTION_LOG_FILE)).unwrap();
        assert_eq!(log2.lines().count(), 2);
        assert_eq!(fs::read(paths.baseline_dir.join(RAW_FILE)).unwrap(), raw_bytes);
    }

    /// Regression (KTD-6): an attested promote from an ordinary *non-provisional*
    /// staged run onto a still-*provisional* committed baseline must succeed. The
    /// re-derivation integrity check compares the reviewed codes + shapes, not the
    /// `provisional` stance — promote intentionally re-derives that stance from the
    /// committed baseline, so it legitimately differs from the staged run's flag.
    /// Before the fix the whole `code_set` (flag included) was compared, so this
    /// shape diverged unconditionally and refused every such promote. The live
    /// field-type re-pin is exactly this shape: a provisional seed baseline and a
    /// clean, non-provisional fetch. Every prior promote test stages and seeds from
    /// the same `provisional=true` run, so the asymmetry was never exercised.
    #[test]
    fn promote_succeeds_when_staged_nonprovisional_but_committed_provisional() {
        let scratch = scratch("promote-provisional-asymmetry");
        let staged = scratch.join("staged");
        let maintained = maintained_set();
        let raw = committed_raw();

        // Staged run: an ordinary fetch normalizes to provisional=false.
        let mut staged_norm = normalize_run(&raw, &maintained, false);
        staged_norm.manifest.refreshed = "2026-06-21".to_string();
        assert!(!staged_norm.code_set.provisional, "staged run is non-provisional");
        let report = FetchReport {
            ok: true,
            fetched_count: staged_norm.code_set.len(),
            committed_code_set_len: Some(staged_norm.code_set.len()),
            facts_degraded_groups: 0,
            degraded_tr_codes: BTreeSet::new(),
            property_type_fallback_served: false,
            failure: None,
        };
        write_staged_run(&staged, &raw, &staged_norm, &report).unwrap();
        let raw_bytes = fs::read(staged.join(RAW_FILE)).unwrap();

        let paths = Paths {
            baseline_dir: scratch.join("baseline"),
            run_root: scratch.join("runs"),
            metadata_dir: repo_metadata_dir(),
            spec_baseline_dir: scratch.join("spec"),
        };
        // Committed baseline: identical raw + shapes, but a provisional seed stance.
        let committed_norm = normalize_run(&raw, &maintained, true);
        assert!(committed_norm.code_set.provisional, "committed seed is provisional");
        assert_eq!(
            committed_norm.shapes, staged_norm.shapes,
            "codes + shapes are identical; only the provisional flag differs"
        );
        write_normalized(&paths.baseline_dir, &committed_norm).unwrap();
        fs::create_dir_all(paths.baseline_dir.join("raw")).unwrap();
        fs::write(paths.baseline_dir.join(RAW_FILE), &raw_bytes).unwrap();

        let outcome =
            promote_committed(&paths, Some(&staged), "ENG-repin", false, "2026-06-21").unwrap();
        assert!(
            matches!(outcome, PromoteOutcome::Promoted { gated: false, .. }),
            "a non-provisional staged run promotes onto a provisional baseline"
        );
        // The committed baseline preserves its provisional stance (KTD-6).
        assert!(
            load_normalized(&paths.baseline_dir).unwrap().code_set.provisional,
            "committed provisional stance is preserved across promote"
        );
    }

    /// Negative companion to the narrowed `codes_match` check: a staged run whose
    /// persisted `code-set.json` membership diverges from what its raw re-derives
    /// (a stale or hand-edited staged run) must still be REFUSED with zero mutation.
    /// The narrowing dropped only the `provisional` flag — set divergence still trips
    /// the guard via `code_set.codes`.
    #[test]
    fn promote_refuses_when_staged_codes_diverge_from_re_derivation() {
        let scratch = scratch("promote-codes-diverge");
        let staged = scratch.join("staged");
        write_staged_from_raw(&staged, &committed_raw());
        // Corrupt the staged run's persisted code-set with a code its raw lacks.
        let mut cs: CodeSet =
            serde_json::from_slice(&fs::read(staged.join(CODE_SET_FILE)).unwrap()).unwrap();
        cs.codes.insert("zzzz_phantom".to_string());
        fs::write(
            staged.join(CODE_SET_FILE),
            serde_json::to_vec_pretty(&cs).unwrap(),
        )
        .unwrap();

        let paths = Paths {
            baseline_dir: scratch.join("baseline"),
            run_root: scratch.join("runs"),
            metadata_dir: repo_metadata_dir(),
            spec_baseline_dir: scratch.join("spec"),
        };
        // Seed a committed baseline + a sentinel raw to prove zero mutation on refusal.
        let staged_run = load_normalized(&staged).unwrap();
        write_normalized(&paths.baseline_dir, &staged_run).unwrap();
        fs::create_dir_all(paths.baseline_dir.join("raw")).unwrap();
        fs::write(paths.baseline_dir.join(RAW_FILE), b"{\"sentinel\":true}\n").unwrap();

        let err = promote_committed(&paths, Some(&staged), "ENG-1", false, "2026-06-21").unwrap_err();
        assert!(
            err.contains("re-derived code-set/shapes differ"),
            "a staged code-set diverging from its raw must be refused: {err}"
        );
        // Zero mutation: committed raw untouched.
        assert_eq!(
            fs::read(paths.baseline_dir.join(RAW_FILE)).unwrap(),
            b"{\"sentinel\":true}\n"
        );
    }

    /// Covers AE2 / R6. A staged run that gates on a new TR, with no attestation,
    /// is refused: nothing is written and the committed baseline is byte-identical.
    #[test]
    fn promote_gated_without_attest_refuses_and_writes_nothing() {
        let scratch = scratch("promote-refuse");
        let paths = Paths {
            baseline_dir: scratch.join("baseline"),
            run_root: scratch.join("runs"),
            metadata_dir: repo_metadata_dir(),
            spec_baseline_dir: scratch.join("spec"),
        };
        write_normalized(&paths.baseline_dir, &empty_run(&["t1102"])).unwrap();
        // Staged run discovers a new TR → gates.
        let staged = scratch.join("staged");
        write_normalized(&staged, &empty_run(&["t1102", "BRANDNEW"])).unwrap();
        fs::create_dir_all(staged.join("raw")).unwrap();
        fs::write(
            staged.join(RAW_FILE),
            b"{\"source_urls\":[],\"property_types\":{},\"groups\":[]}\n",
        )
        .unwrap();

        let manifest_before = fs::read(paths.baseline_dir.join(MANIFEST_FILE)).unwrap();
        let codeset_before = fs::read(paths.baseline_dir.join(CODE_SET_FILE)).unwrap();

        let outcome = promote_committed(&paths, Some(&staged), "", false, "2026-06-21").unwrap();
        assert_eq!(outcome, PromoteOutcome::RefusedGated);

        assert_eq!(
            fs::read(paths.baseline_dir.join(MANIFEST_FILE)).unwrap(),
            manifest_before
        );
        assert_eq!(
            fs::read(paths.baseline_dir.join(CODE_SET_FILE)).unwrap(),
            codeset_before
        );
        assert!(
            !paths.baseline_dir.join(PROMOTION_LOG_FILE).exists(),
            "no log line is written on a refusal"
        );
    }

    /// Covers AE3 / AE4 / R8 / R11 / R12. A gated run (new upstream TR) promoted
    /// with `--attest` proceeds: the record's accepted-findings names the gated
    /// TR alongside the attested-by value, the new TR lands in the committed raw +
    /// code-set but is not admitted to the maintained shapes, and the real metadata
    /// + evidence files are left byte-identical (promote touches neither).
    #[test]
    fn promote_gated_with_attest_proceeds_and_records_accepted_findings() {
        let scratch = scratch("promote-attest");
        let staged = scratch.join("staged");
        let mut raw = committed_raw();
        raw.groups[0].trs.push(new_raw_tr("BRANDNEW"));
        let raw_bytes = write_staged_from_raw(&staged, &raw);

        let paths = Paths {
            baseline_dir: scratch.join("baseline"),
            run_root: scratch.join("runs"),
            metadata_dir: repo_metadata_dir(),
            spec_baseline_dir: scratch.join("spec"),
        };
        // Committed baseline = the real raw re-derived (no BRANDNEW), so the staged
        // run gates on the new TR.
        let committed_norm = normalize_run(&committed_raw(), &maintained_set(), true);
        write_normalized(&paths.baseline_dir, &committed_norm).unwrap();

        // Snapshot real metadata + evidence to prove promote touches neither (R12).
        let tr_token = repo_metadata_dir().join("trs").join("token.yaml");
        let ev_token = repo_metadata_dir().join("evidence").join("token.yaml");
        let tr_before = fs::read(&tr_token).unwrap();
        let ev_before = fs::read(&ev_token).unwrap();

        let outcome = promote_committed(&paths, Some(&staged), "ops@team", false, "2026-06-21").unwrap();
        assert!(matches!(outcome, PromoteOutcome::Promoted { gated: true, .. }));

        // The record names the gated new TR and the attested-by value (R8).
        let log = fs::read_to_string(paths.baseline_dir.join(PROMOTION_LOG_FILE)).unwrap();
        let rec: crate::types::PromotionRecord =
            serde_json::from_str(log.lines().next().unwrap()).unwrap();
        assert_eq!(rec.attested_by, "ops@team");
        assert!(
            rec.accepted_findings.iter().any(|f| f.tr_code == "BRANDNEW"),
            "the accepted-findings field names the gated new TR: {:?}",
            rec.accepted_findings
        );
        assert_eq!(rec.raw_hash, crate::api_drift::whole_raw_hash(&raw_bytes));

        // AE4: BRANDNEW is in the committed raw + code-set but not a maintained
        // shape, and still surfaces as a finding on the next check.
        let committed = load_normalized(&paths.baseline_dir).unwrap();
        assert!(committed.code_set.contains("BRANDNEW"));
        assert!(!committed.shapes.contains_key("BRANDNEW"));
        assert!(fs::read_to_string(paths.baseline_dir.join(RAW_FILE))
            .unwrap()
            .contains("BRANDNEW"));

        // R12 / AE5: promote touched no TR metadata or evidence.
        assert_eq!(fs::read(&tr_token).unwrap(), tr_before, "trs/token.yaml unchanged");
        assert_eq!(fs::read(&ev_token).unwrap(), ev_before, "evidence/token.yaml unchanged");
    }

    /// Covers R5. A staged run whose raw is malformed JSON passes the gate (which
    /// reads normalized, not raw) but fails the in-memory re-derivation, so promote
    /// aborts with the committed baseline byte-identical and no log line.
    #[test]
    fn promote_derive_failure_aborts_with_zero_mutation() {
        let scratch = scratch("promote-derive-fail");
        let paths = Paths {
            baseline_dir: scratch.join("baseline"),
            run_root: scratch.join("runs"),
            metadata_dir: repo_metadata_dir(),
            spec_baseline_dir: scratch.join("spec"),
        };
        write_normalized(&paths.baseline_dir, &empty_run(&["t1102"])).unwrap();
        let manifest_before = fs::read(paths.baseline_dir.join(MANIFEST_FILE)).unwrap();

        // Staged: clean normalized (no gate) but a malformed raw.
        let staged = scratch.join("staged");
        write_normalized(&staged, &empty_run(&["t1102"])).unwrap();
        fs::create_dir_all(staged.join("raw")).unwrap();
        fs::write(staged.join(RAW_FILE), b"{ not json").unwrap();

        let result = promote_committed(&paths, Some(&staged), "ENG-1", false, "2026-06-21");
        assert!(result.is_err(), "a malformed staged raw aborts the promote");
        assert_eq!(
            fs::read(paths.baseline_dir.join(MANIFEST_FILE)).unwrap(),
            manifest_before,
            "committed baseline is byte-identical after an aborted promote"
        );
        assert!(!paths.baseline_dir.join(PROMOTION_LOG_FILE).exists());
    }

    /// Seed a committed baseline equal to the staged run, then mutate one
    /// maintained field so the staged run shows exactly one drift on a maintained
    /// TR. `mutate` receives the first field of the first non-empty block of the
    /// first maintained shape (request blocks first, then response). Returns the
    /// staged dir and the prepared `Paths`.
    fn stage_with_one_maintained_field_change(
        name: &str,
        mutate: impl FnOnce(&mut ls_metadata::BlockField),
    ) -> (PathBuf, Paths) {
        let scratch = scratch(name);
        let staged = scratch.join("staged");
        write_staged_from_raw(&staged, &committed_raw());

        let paths = Paths {
            baseline_dir: scratch.join("baseline"),
            run_root: scratch.join("runs"),
            metadata_dir: repo_metadata_dir(),
            spec_baseline_dir: scratch.join("spec"),
        };
        // Committed baseline starts byte-equal to the staged run (clean self-diff),
        // then one maintained field is mutated so the staged run drifts against it.
        let mut committed = load_normalized(&staged).unwrap();
        let shape = committed
            .shapes
            .values_mut()
            .find(|s| !s.request_blocks.is_empty() || !s.response_blocks.is_empty())
            .expect("a maintained shape with at least one field");
        let field = shape
            .request_blocks
            .iter_mut()
            .chain(shape.response_blocks.iter_mut())
            .next()
            .expect("a field to mutate");
        mutate(field);
        write_normalized(&paths.baseline_dir, &committed).unwrap();
        (staged, paths)
    }

    /// Covers R1 / R6 (admit-path). A staged run whose only maintained-shape drift
    /// is a pure field-`type` change, promoted with `--type-only --attest`, passes
    /// the type-only gate and performs the normal whole-raw promote: raw replaced,
    /// one record appended.
    #[test]
    fn type_only_promote_admits_pure_type_change() {
        let (staged, paths) = stage_with_one_maintained_field_change(
            "type-only-admit",
            // Committed carries an old type; the staged run's real type differs →
            // a pure-type FieldChanged on a maintained TR.
            |f| f.r#type = Some("__OLD_TYPE__".to_string()),
        );
        let raw_bytes = fs::read(staged.join(RAW_FILE)).unwrap();

        let outcome =
            promote_committed(&paths, Some(&staged), "ENG-1", true, "2026-06-21").unwrap();
        assert!(
            matches!(outcome, PromoteOutcome::Promoted { gated: true, .. }),
            "a pure-type wave gates (Breaking/Maintenance) and is attested through: {outcome:?}"
        );
        // Whole-raw promote happened: committed raw replaced by the staged raw.
        assert_eq!(fs::read(paths.baseline_dir.join(RAW_FILE)).unwrap(), raw_bytes);
        let log = fs::read_to_string(paths.baseline_dir.join(PROMOTION_LOG_FILE)).unwrap();
        assert_eq!(log.lines().count(), 1, "exactly one promotion record");
        // Post-promote self-diff is clean.
        assert!(!run_check(&paths, Some(&staged)).unwrap().gates());
    }

    /// Covers R1 / R3 (block-path). A staged run that changes a field's identity on
    /// a maintained TR (surfacing non-type structural drift) is refused by the
    /// type-only gate with zero mutation — and `--attest` cannot satisfy it (R3).
    #[test]
    fn type_only_promote_blocks_non_type_drift_with_zero_mutation() {
        let (staged, paths) = stage_with_one_maintained_field_change(
            "type-only-block",
            // Rename the committed field so the staged run's real field has no
            // identity match → it surfaces as added/removed (non-type drift).
            |f| f.field_name = format!("{}__renamed_so_staged_adds", f.field_name),
        );
        // Seed a sentinel committed raw to prove the blocked promote never replaces it.
        fs::create_dir_all(paths.baseline_dir.join("raw")).unwrap();
        fs::write(paths.baseline_dir.join(RAW_FILE), b"{\"sentinel\":true}\n").unwrap();
        let raw_before = fs::read(paths.baseline_dir.join(RAW_FILE)).unwrap();
        let manifest_before = fs::read(paths.baseline_dir.join(MANIFEST_FILE)).unwrap();

        let outcome =
            promote_committed(&paths, Some(&staged), "ops@team", true, "2026-06-21").unwrap();
        assert!(
            matches!(&outcome, PromoteOutcome::RefusedTypeOnly(_)),
            "non-type drift on a maintained TR is refused: {outcome:?}"
        );
        // Zero mutation: committed raw + manifest byte-identical, no log line.
        assert_eq!(
            fs::read(paths.baseline_dir.join(RAW_FILE)).unwrap(),
            raw_before
        );
        assert_eq!(
            fs::read(paths.baseline_dir.join(MANIFEST_FILE)).unwrap(),
            manifest_before
        );
        assert!(!paths.baseline_dir.join(PROMOTION_LOG_FILE).exists());
    }

    /// Covers AE1. A fallback-served staged run with a maintained TR is rejected by
    /// the existing facts-outage gate (exit 2 / Err) *before* the type-only gate is
    /// reached — `--type-only` adds no new fallback path. Zero mutation.
    #[test]
    fn type_only_promote_fallback_served_rejected_by_facts_gate() {
        let scratch = scratch("type-only-fallback");
        let staged = scratch.join("staged");
        write_staged_from_raw(&staged, &committed_raw());
        // Re-stamp the fetch report as fallback-served (maintained TRs are present
        // in the committed raw, so the facts-outage gate fires MaintainedAffected).
        write_facts_report(&staged, &[], true);

        let paths = Paths {
            baseline_dir: scratch.join("baseline"),
            run_root: scratch.join("runs"),
            metadata_dir: repo_metadata_dir(),
            spec_baseline_dir: scratch.join("spec"),
        };
        let committed = load_normalized(&staged).unwrap();
        write_normalized(&paths.baseline_dir, &committed).unwrap();
        let manifest_before = fs::read(paths.baseline_dir.join(MANIFEST_FILE)).unwrap();

        let result = promote_committed(&paths, Some(&staged), "ENG-1", true, "2026-06-21");
        assert!(
            result.is_err(),
            "a fallback-served run is rejected by the facts-outage gate before the type-only gate"
        );
        assert_eq!(
            fs::read(paths.baseline_dir.join(MANIFEST_FILE)).unwrap(),
            manifest_before,
            "zero mutation on the fallback-rejected path"
        );
        assert!(!paths.baseline_dir.join(PROMOTION_LOG_FILE).exists());
    }

    /// Covers the shape-set guard (U3). A maintained TR present in the staged run
    /// but absent from the committed baseline (newly-maintained, not-yet-baselined)
    /// produces no `compare` finding the findings-gate could catch — so the
    /// companion shape-set guard must refuse with zero mutation rather than
    /// silently writing its never-evaluated shape.
    #[test]
    fn type_only_promote_blocks_newly_maintained_shape_with_zero_mutation() {
        let scratch = scratch("type-only-new-shape");
        let staged = scratch.join("staged");
        write_staged_from_raw(&staged, &committed_raw());

        let paths = Paths {
            baseline_dir: scratch.join("baseline"),
            run_root: scratch.join("runs"),
            metadata_dir: repo_metadata_dir(),
            spec_baseline_dir: scratch.join("spec"),
        };
        // Committed baseline = staged minus one maintained shape, so the staged run
        // re-introduces that shape with no finding (compare only diffs the shared
        // set). The findings-gate admits; the shape-set guard must block.
        let mut committed = load_normalized(&staged).unwrap();
        let dropped = committed
            .shapes
            .keys()
            .next()
            .expect("at least one maintained shape")
            .clone();
        committed.shapes.remove(&dropped);
        write_normalized(&paths.baseline_dir, &committed).unwrap();
        fs::create_dir_all(paths.baseline_dir.join("raw")).unwrap();
        fs::write(paths.baseline_dir.join(RAW_FILE), b"{\"sentinel\":true}\n").unwrap();
        let raw_before = fs::read(paths.baseline_dir.join(RAW_FILE)).unwrap();

        // Sanity: the findings-gate alone does NOT catch the re-introduced shape
        // (the gap the companion guard closes).
        let report = run_check(&paths, Some(&staged)).unwrap();
        assert_eq!(
            crate::api_drift::type_only_gate(&report.findings),
            crate::api_drift::TypeOnlyDecision::Admit,
            "findings-gate cannot see the un-diffed newly-maintained shape"
        );

        let outcome =
            promote_committed(&paths, Some(&staged), "ENG-1", true, "2026-06-21").unwrap();
        assert!(
            matches!(&outcome, PromoteOutcome::RefusedTypeOnly(r) if r.contains(&dropped)),
            "the shape-set guard refuses the re-introduced maintained shape: {outcome:?}"
        );
        assert_eq!(
            fs::read(paths.baseline_dir.join(RAW_FILE)).unwrap(),
            raw_before,
            "zero mutation: committed raw byte-identical"
        );
        assert!(!paths.baseline_dir.join(PROMOTION_LOG_FILE).exists());
    }

    /// Dispatch (U3): `promote --type-only --dry-run` over a blocking staged run
    /// surfaces the gate block as exit 2 and writes nothing; an admitted type wave
    /// follows the drift gate (exit 1, the signal to pass `--attest`).
    #[test]
    fn dispatch_type_only_dry_run_block_exits_error_writes_nothing() {
        // Blocking: a renamed maintained field surfaces as non-type structural drift.
        let (staged, paths) = stage_with_one_maintained_field_change(
            "type-only-dry-block",
            |f| f.field_name = format!("{}__renamed", f.field_name),
        );
        let manifest_before = fs::read(paths.baseline_dir.join(MANIFEST_FILE)).unwrap();
        assert_eq!(
            dispatch(
                &paths,
                Command::PromoteDryRun {
                    staged: Some(staged),
                    type_only: true
                }
            ),
            Exit::Error,
            "a blocked type-only dry-run exits 2"
        );
        assert_eq!(
            fs::read(paths.baseline_dir.join(MANIFEST_FILE)).unwrap(),
            manifest_before,
            "dry-run writes nothing even on a block"
        );
        assert!(!paths.baseline_dir.join(PROMOTION_LOG_FILE).exists());
    }

    /// U2 / R2: resolution prefers an explicit `--staged`, falls back to the
    /// `latest.txt` pointer, and errors loudly with neither (no live-fetch
    /// fallback).
    #[test]
    fn resolve_staged_run_uses_explicit_then_latest_then_errors() {
        let scratch = scratch("resolve-staged");
        let paths = Paths {
            baseline_dir: scratch.join("baseline"),
            run_root: scratch.join("runs"),
            metadata_dir: repo_metadata_dir(),
            spec_baseline_dir: scratch.join("spec"),
        };
        // No latest.txt and no --staged → error.
        assert!(resolve_staged_run(&paths, None).is_err());

        // Stage a run and point latest.txt at it (relative `runs/{name}`).
        let run = paths.run_root.join("runs").join("2026-06-21T00-00-00Z");
        write_staged_from_raw(&run, &committed_raw());
        update_latest(&paths, &run).unwrap();
        let resolved = resolve_staged_run(&paths, None).unwrap();
        assert_eq!(resolved, run, "default resolution follows latest.txt");

        // Explicit --staged overrides.
        let explicit = scratch.join("explicit");
        write_staged_from_raw(&explicit, &committed_raw());
        assert_eq!(
            resolve_staged_run(&paths, Some(&explicit)).unwrap(),
            explicit
        );

        // A --staged path without a raw is rejected.
        let bare = scratch.join("bare");
        fs::create_dir_all(&bare).unwrap();
        assert!(resolve_staged_run(&paths, Some(&bare)).is_err());
    }

    /// U6 / F4 / R3: `promote --dry-run` pins the run, reports without writing, and
    /// maps the exit to the gate — exit 0 on a clean run (default-selected via
    /// latest.txt), exit 1 on a gated run.
    #[test]
    fn promote_dry_run_reports_and_follows_the_gate_without_writing() {
        let scratch = scratch("promote-dry");
        let paths = Paths {
            baseline_dir: scratch.join("baseline"),
            run_root: scratch.join("runs"),
            metadata_dir: repo_metadata_dir(),
            spec_baseline_dir: scratch.join("spec"),
        };
        // Clean staged run (default-selected via latest.txt); committed seeded to
        // match so the diff is clean.
        let run = paths.run_root.join("runs").join("2026-06-21T00-00-00Z");
        write_staged_from_raw(&run, &committed_raw());
        update_latest(&paths, &run).unwrap();
        write_normalized(&paths.baseline_dir, &load_normalized(&run).unwrap()).unwrap();
        let baseline_before = fs::read(paths.baseline_dir.join(MANIFEST_FILE)).unwrap();

        assert_eq!(
            dispatch(
                &paths,
                Command::PromoteDryRun {
                    staged: None,
                    type_only: false
                }
            ),
            Exit::Ok,
            "clean dry-run via latest.txt exits 0"
        );
        assert_eq!(
            fs::read(paths.baseline_dir.join(MANIFEST_FILE)).unwrap(),
            baseline_before,
            "dry-run writes nothing"
        );
        assert!(!paths.baseline_dir.join(PROMOTION_LOG_FILE).exists());

        // Gated staged run → exit 1, still writes nothing.
        let gated = scratch.join("gated");
        let mut raw = committed_raw();
        raw.groups[0].trs.push(new_raw_tr("BRANDNEW"));
        write_staged_from_raw(&gated, &raw);
        assert_eq!(
            dispatch(
                &paths,
                Command::PromoteDryRun {
                    staged: Some(gated),
                    type_only: false
                }
            ),
            Exit::Gated,
            "gated dry-run exits 1"
        );
        assert_eq!(
            fs::read(paths.baseline_dir.join(MANIFEST_FILE)).unwrap(),
            baseline_before
        );
    }
}
