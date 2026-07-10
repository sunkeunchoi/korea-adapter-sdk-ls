//! `lab-research` — the loop-turn CLI (U1–U6).
//!
//! Five subcommands, each composing an existing seam — the CLI adds
//! orchestration and machine verdicts, not new machinery:
//!
//! - `turn` (U2) — a governed parameter turn, or a no-override rerun.
//! - `runs compare` (U3) — the param-turn / data-turn manifest verdicts.
//! - `replay` (U4) — guardrail-swap replay with the zero-evaluated refusal.
//! - `catalog status` (U5) — the ingest→backtest go/no-go.
//! - `analyze --scaffold` (U6) — a pre-filled analysis file.
//!
//! Each command is a testable library function over an explicit config struct
//! (offline, no credentials); [`main_cli`] wires env → config → function →
//! stdout → exit code, following the `runner::backtest` precedent (KTD2). The
//! scrub discipline (KTD8): structured facts — symbols, params, run ids —
//! render as typed/verbatim values, never through the free-text scrub (a
//! 6-digit KRX shcode collides with the account-number heuristic); only prose
//! that could carry a credential routes through [`nautilus_ls::scrub`].

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chrono::{DateTime, Datelike, NaiveDate, Utc, Weekday};

use nautilus_ls::ingest::checkpoint::Checkpoint;
use nautilus_ls::ingest::{compact_catalog, kst_date_of, read_all_bars, CompactOutcome};
use nautilus_model::data::Bar;
use nautilus_model::enums::BarAggregation;

use crate::agent::capability::{ActionCapability, CapabilitySet};
use crate::agent::context::AgentContext;
use crate::agent::envelope::{
    CapabilityOutcome, DecisionTrigger, GuardrailResult, PolicyDecisionRecord,
};
use crate::agent::guardrails::proposal_bounds::ProposalBoundsGuardrail;
use crate::agent::intent::AgentIntent;
use crate::agent::pipeline::DecisionPipeline;
use crate::agent::policies::research::ResearchPolicy;
use crate::agent::policy::PolicyDecision;
use crate::agent::recording::DecisionRecorder;
use crate::agent::replay::{read_envelopes, replay};
use crate::artifacts::data_quality::DataQualityReport;
use crate::artifacts::manifest::{DataRange, Manifest};
use crate::artifacts::performance::PerformanceReport;
use crate::artifacts::{list_runs, ANALYSIS_FILE, DATA_QUALITY_FILE, MANIFEST_FILE, PERFORMANCE_FILE};
use crate::params::OrbParams;
use crate::runner::backtest::{run as run_backtest, BacktestConfig};

/// The committed proposal-bounds cap the CLI wires the decision pipeline with
/// (KTD3): 0.5 relative change, the value every committed instantiation uses —
/// pinned here as an explicit CLI decision, not inherited from a compiled
/// default.
pub const PROPOSAL_BOUNDS_CAP: f64 = 0.5;

/// The three verdict words a turn-2 analysis must reach (R15). Named in the
/// scaffold skeleton so no analysis invents its own vocabulary.
pub const VERDICT_WORDS: [&str; 3] = ["keep", "revert", "insufficient-evidence"];

// ===========================================================================
// Shared helpers
// ===========================================================================

/// Resolve `<data home>` from `LS_DATA_HOME` (the shared registry/catalog root).
fn data_home_from_env() -> anyhow::Result<PathBuf> {
    Ok(std::env::var("LS_DATA_HOME")
        .map_err(|_| anyhow::anyhow!("LS_DATA_HOME is required"))?
        .into())
}

/// Read a finalized run's manifest. Crate-visible: `runner::report` resolves
/// its source run through the same seam.
pub(crate) fn read_manifest(data_home: &Path, run_id: &str) -> anyhow::Result<Manifest> {
    let path = data_home.join("runs").join(run_id).join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| anyhow::anyhow!("parsing {}: {e}", path.display()))
}

/// The chronological sort key for a run id (KTD1). The fixed-width UTC stamp
/// prefix orders chronologically, but the trailing `-v<N>` version must compare
/// **numerically**, not lexically — otherwise two runs that tie on the
/// second-granularity stamp (a fast rerun + a governance bump) sort `-v10`
/// before `-v2`, and [`latest_finalized_run`] would read a stale manifest.
fn run_order_key(run_id: &str) -> (String, u32) {
    match run_id.rsplit_once("-v") {
        Some((head, ver)) => (head.to_string(), ver.parse().unwrap_or(0)),
        None => (run_id.to_string(), 0),
    }
}

/// The finalized run ids, ordered oldest → newest by [`run_order_key`] (numeric
/// version tiebreak, not the lexical order [`list_runs`] returns).
fn ordered_runs(data_home: &Path) -> Vec<String> {
    let mut runs = list_runs(data_home);
    runs.sort_by(|a, b| run_order_key(a).cmp(&run_order_key(b)));
    runs
}

/// The newest finalized run's id + manifest, or `None` on a fresh registry
/// (KTD1: current-params authority is the latest finalized manifest).
pub fn latest_finalized_run(data_home: &Path) -> anyhow::Result<Option<(String, Manifest)>> {
    match ordered_runs(data_home).last() {
        None => Ok(None),
        Some(run_id) => Ok(Some((run_id.clone(), read_manifest(data_home, run_id)?))),
    }
}

/// The checkpoint bar-series label for a bar (`1-DAY`, `n-MINUTE`), matching
/// [`nautilus_ls::ingest::BarKind::label`] so a span can be checked against the
/// checkpoint watermark keyed the same way.
fn bar_label(bar: &Bar) -> String {
    let spec = bar.bar_type.spec();
    match spec.aggregation {
        BarAggregation::Day => "1-DAY".to_string(),
        BarAggregation::Minute => format!("{}-MINUTE", spec.step),
        other => format!("{}-{other:?}", spec.step),
    }
}

/// The last weekday on or before `date`, walking back over Saturdays and
/// Sundays. The accumulate ingest advances the checkpoint watermark to the
/// calendar last-closed session even when that lands on a weekend (documented
/// `last_closed_session` behavior: a weekend "yields no new bars while the
/// watermark still advances"). A tail check comparing the last bar against the
/// raw watermark therefore false-flags a healthy Friday-closed catalog whenever
/// the most recent ingest ran over a weekend — comparing against the last
/// weekday instead flags only a genuine undershoot. (Holidays remain
/// undetectable: the repo carries no trading calendar.)
fn last_weekday_on_or_before(mut date: NaiveDate) -> NaiveDate {
    while matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
        date = date.pred_opt().expect("a date always has a predecessor");
    }
    date
}

// ===========================================================================
// U2 — the turn command
// ===========================================================================

/// The turn command's config. A turn with `override_param == None` is a
/// no-override **rerun** (KTD3): the resolved current params re-run with no
/// governance cycle and no version bump.
#[derive(Debug, Clone)]
pub struct TurnConfig {
    /// The data home.
    pub data_home: PathBuf,
    /// The parameter to change (a numeric `OrbParams` serde key). `None` runs a
    /// no-override rerun.
    pub override_param: Option<String>,
    /// The target value for `override_param`.
    pub override_target: Option<f64>,
    /// An explicit pinned range; `None` inherits the latest run's range (KTD1).
    /// Required on a fresh data home.
    pub range: Option<DataRange>,
    /// The proposal-bounds cap the pipeline is wired with (pinned
    /// [`PROPOSAL_BOUNDS_CAP`], KTD3).
    pub max_relative_change: f64,
    /// The minute-bar step the backtest trades.
    pub minute_step: u32,
    /// Starting account balance (KRW).
    pub starting_balance: f64,
    /// The run-id timestamp source (tests pin it).
    pub now: DateTime<Utc>,
    /// Test seam: the override set actually applied to the params. Defaults to
    /// `{override_param: override_target}`; a divergent map exercises the
    /// refuse-on-mismatch guard (R1).
    pub applied_overrides: Option<BTreeMap<String, f64>>,
    /// U4/KTD-5: the strategy version the resolved current params MUST carry. When
    /// set, a mismatch (e.g. a fresh home falling back to `OrbParams::default()`'s
    /// v0) is a hard stop before any backtest — not a silent default-param run.
    pub expect_version: Option<u32>,
    /// U4/KTD-5: the `gap_min_pct` the resolved current params MUST carry. Paired
    /// with [`Self::expect_version`] to pin the seeded v3 identity (0.6) before a
    /// fresh-home rerun.
    pub expect_gap_min_pct: Option<f64>,
}

impl TurnConfig {
    /// A turn config with the CLI defaults over `data_home`.
    pub fn new(data_home: impl Into<PathBuf>, now: DateTime<Utc>) -> Self {
        TurnConfig {
            data_home: data_home.into(),
            override_param: None,
            override_target: None,
            range: None,
            max_relative_change: PROPOSAL_BOUNDS_CAP,
            minute_step: 1,
            starting_balance: 100_000_000.0,
            now,
            applied_overrides: None,
            expect_version: None,
            expect_gap_min_pct: None,
        }
    }
}

/// The outcome of a turn.
#[derive(Debug, Clone)]
pub struct TurnOutcome {
    /// Whether the turn produced a finalized run.
    pub ran: bool,
    /// The finalized run id, when a backtest ran.
    pub run_id: Option<String>,
    /// The strategy version the produced run carries.
    pub version: Option<u32>,
    /// Whether governance approved the proposal (`None` for a rerun — no cycle).
    pub approved: Option<bool>,
    /// The denial/mismatch reason when the turn ran nothing.
    pub refusal: Option<String>,
    /// Human-facing result lines.
    pub lines: Vec<String>,
}

impl TurnOutcome {
    fn refused(reason: String, mut lines: Vec<String>, approved: Option<bool>) -> Self {
        lines.push(format!("turn ran no backtest: {reason}"));
        TurnOutcome { ran: false, run_id: None, version: None, approved, refusal: Some(reason), lines }
    }
}

/// Execute a parameter turn (or a no-override rerun) end-to-end (R1, R2, R3;
/// AE1, AE2, AE7). See [`TurnConfig`]/KTD1/KTD3 for the resolution rules.
pub async fn turn(cfg: TurnConfig) -> anyhow::Result<TurnOutcome> {
    let prior = latest_finalized_run(&cfg.data_home)?;
    let (current_params, current_version): (OrbParams, u32) = match &prior {
        Some((_, m)) => (m.params.clone(), m.strategy_version),
        None => (OrbParams::default(), OrbParams::default().strategy_version),
    };

    // U4 / KTD-5: assert the resolved v3 identity BEFORE running. On a fresh home
    // with no seeded manifest, `latest_finalized_run` is `None` and the params fall
    // back to `OrbParams::default()` (v0, gap 3.0) — a silent wrong-strategy run. If
    // the operator pinned the expected identity, refuse rather than run defaults.
    if let Some(expected) = cfg.expect_version {
        if current_version != expected {
            anyhow::bail!(
                "v3-param resolution failed: resolved strategy v{current_version} (gap_min_pct {:.4}), \
                 expected v{expected} — the fresh home is missing the seeded v3 manifest (KTD-5). Copy the \
                 turn-2b v3 run's manifest.json into runs/ before rerunning; refusing a silent default-param backtest",
                current_params.gap_min_pct
            );
        }
    }
    if let Some(expected_gap) = cfg.expect_gap_min_pct {
        if (current_params.gap_min_pct - expected_gap).abs() > 1e-9 {
            anyhow::bail!(
                "v3-param resolution failed: resolved gap_min_pct {:.4} (strategy v{current_version}), \
                 expected {expected_gap:.4} — the fresh home did not resolve the seeded v3 params (KTD-5). \
                 Refusing a wrong-param backtest",
                current_params.gap_min_pct
            );
        }
    }

    // Range inheritance (KTD1): a param verdict's range/fingerprint equality is
    // true by construction when the new run pins the prior run's range. A fresh
    // home carries no range and must be given one explicitly.
    let range = match (&cfg.range, &prior) {
        (Some(r), _) => r.clone(),
        (None, Some((_, m))) => m.data_range.clone(),
        (None, None) => anyhow::bail!(
            "fresh data home: an explicit range is required (LS_TURN_SDATE / LS_TURN_EDATE)"
        ),
    };

    let mut lines = Vec::new();

    // --- Rerun mode: no override → no governance, no version bump (KTD3). ---
    let Some(param) = cfg.override_param.clone() else {
        lines.push(format!(
            "rerun: current params (strategy v{current_version}), no governance cycle, no version bump"
        ));
        let outcome = run_one_backtest(&cfg, current_params.clone(), range).await?;
        lines.push(format!("finalized run {}", outcome.run_id));
        return Ok(TurnOutcome {
            ran: true,
            run_id: Some(outcome.run_id),
            version: Some(current_version),
            approved: None,
            refusal: None,
            lines,
        });
    };

    // --- Governed param turn. ---
    let target = cfg
        .override_target
        .ok_or_else(|| anyhow::anyhow!("override target is required for a param turn"))?;
    let current_numeric = current_params.numeric_summary();
    let current_value = *current_numeric.get(&param).ok_or_else(|| {
        anyhow::anyhow!("'{param}' is not a numeric OrbParams field — cannot turn it")
    })?;
    // A proposal that does not move the value is a no-op — govern nothing and give
    // a clear message rather than approving, bumping the version, then refusing
    // with the confusing "applied change touches {strategy_version}" mismatch.
    if target == current_value {
        anyhow::bail!(
            "'{param}' is already {current_value:.4} — proposing the same value is a no-op; \
             run with no LS_TURN_PARAM for a rerun (no version bump)"
        );
    }

    // Lower the operator's request into a manual-trigger envelope through the
    // pinned pipeline (KTD3): CapabilitySet limited to Research + the
    // proposal-bounds cap.
    let intent = AgentIntent::ProposeParameterChange {
        strategy_id: current_params.strategy_id.clone(),
        parameter: param.clone(),
        current_value,
        proposed_value: target,
        rationale: format!(
            "operator turn: {param} {current_value:.4} -> {target:.4} (strategy v{current_version} -> v{})",
            current_version + 1
        ),
    };
    let pipeline = DecisionPipeline::new(
        CapabilitySet {
            observations: BTreeSet::new(),
            actions: BTreeSet::from([ActionCapability::Research]),
            instrument_scope: BTreeSet::new(),
        },
        vec![Box::new(ProposalBoundsGuardrail { max_relative_change: cfg.max_relative_change })],
    );
    let context = turn_context(&cfg.data_home, prior.as_ref(), &current_params);
    let ts_event = cfg.now.timestamp_nanos_opt().unwrap_or(0).max(0) as u64;
    let envelope = pipeline.run(
        ts_event,
        DecisionTrigger::Manual {
            reason: format!("lab-research turn: {param} -> {target:.4}"),
        },
        context,
        PolicyDecision::execute(intent),
    );

    // Append the envelope regardless of outcome — approvals, denials, failures
    // are audit records, never state (KTD1).
    DecisionRecorder::new(&cfg.data_home)?.append(&envelope)?;

    // A capability denial or a guardrail rejection runs no backtest (R3, AE1).
    if let CapabilityOutcome::Denied { reason } = &envelope.capability {
        return Ok(TurnOutcome::refused(reason.clone(), lines, Some(false)));
    }
    if let GuardrailResult::Rejected { reason } = &envelope.guardrail {
        return Ok(TurnOutcome::refused(reason.clone(), lines, Some(false)));
    }

    // Refuse-on-mismatch (R1): the executed override key set must equal the
    // recorded envelope's parameter (plus the implicit strategy-version bump) —
    // the same exactly-two-key discipline as the compare verdict (KTD3).
    let applied = cfg
        .applied_overrides
        .clone()
        .unwrap_or_else(|| BTreeMap::from([(param.clone(), target)]));
    let governed_param = match &envelope.policy_decision {
        PolicyDecisionRecord::Execute {
            intent: AgentIntent::ProposeParameterChange { parameter, .. }, ..
        } => parameter.clone(),
        _ => param.clone(),
    };
    let applied_keys: BTreeSet<&String> = applied.keys().collect();
    if applied_keys.len() != 1 || !applied_keys.contains(&governed_param) {
        let got: Vec<&str> = applied.keys().map(String::as_str).collect();
        return Ok(TurnOutcome::refused(
            format!(
                "executed override set {got:?} differs from the governed parameter '{governed_param}' \
                 — refusing before backtest"
            ),
            lines,
            Some(true),
        ));
    }

    // Apply the override + bump the version (prior + 1), then run.
    let new_version = current_version + 1;
    let new_params = apply_overrides(&current_params, &applied, new_version)?;

    // The manifest diff the new run WILL show, isolated to {param, version}
    // (defence-in-depth against a broader applied set slipping the key check).
    let diff = param_diff(&current_params, &new_params);
    let expected: BTreeSet<String> =
        BTreeSet::from([governed_param.clone(), "strategy_version".to_string()]);
    let got: BTreeSet<String> = diff.iter().cloned().collect();
    if got != expected {
        return Ok(TurnOutcome::refused(
            format!("applied change touches {got:?}, not exactly {{'{governed_param}', 'strategy_version'}}"),
            lines,
            Some(true),
        ));
    }

    lines.push(format!(
        "approved: {param} {current_value:.4} -> {target:.4}, strategy v{current_version} -> v{new_version}"
    ));
    let outcome = run_one_backtest(&cfg, new_params, range).await?;
    lines.push(format!("finalized run {}", outcome.run_id));
    Ok(TurnOutcome {
        ran: true,
        run_id: Some(outcome.run_id),
        version: Some(new_version),
        approved: Some(true),
        refusal: None,
        lines,
    })
}

/// Build the captured context for the turn's envelope: the latest finalized
/// run's `RunState` when one exists (policy-sufficient for replay, R5→R7), else
/// a minimal `RunState` over the current params.
fn turn_context(
    data_home: &Path,
    prior: Option<&(String, Manifest)>,
    current_params: &OrbParams,
) -> AgentContext {
    if let Some((run_id, _)) = prior {
        if let Ok(ctx) = ResearchPolicy::context_from_run(data_home, run_id) {
            return ctx;
        }
    }
    AgentContext::run_state(0.0, Vec::new(), current_params.numeric_summary(), BTreeMap::new())
}

/// Apply a numeric override set to the params and bump the version, via a JSON
/// round-trip so any numeric serde field can be set generically. Integral
/// targets serialize as integers so integer-typed fields (`universe_top_n`, …)
/// still deserialize.
fn apply_overrides(
    current: &OrbParams,
    overrides: &BTreeMap<String, f64>,
    new_version: u32,
) -> anyhow::Result<OrbParams> {
    let mut value = serde_json::to_value(current)?;
    let obj = value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("params did not serialize to a JSON object"))?;
    for (k, v) in overrides {
        if !obj.contains_key(k) {
            anyhow::bail!("'{k}' is not an OrbParams field");
        }
        let n = if v.fract() == 0.0 && v.is_finite() {
            serde_json::json!(*v as i64)
        } else {
            serde_json::json!(*v)
        };
        obj.insert(k.clone(), n);
    }
    obj.insert("strategy_version".to_string(), serde_json::json!(new_version));
    serde_json::from_value(value)
        .map_err(|e| anyhow::anyhow!("override produced an invalid OrbParams: {e}"))
}

async fn run_one_backtest(
    cfg: &TurnConfig,
    params: OrbParams,
    range: DataRange,
) -> anyhow::Result<crate::runner::backtest::RunOutcome> {
    let mut bt = BacktestConfig::new(&cfg.data_home, &range.start, &range.end);
    bt.params = params;
    bt.minute_step = cfg.minute_step;
    bt.starting_balance = cfg.starting_balance;
    run_backtest(bt, cfg.now).await
}

// ===========================================================================
// U3 — runs compare
// ===========================================================================

/// Which verdict `runs compare` applies (KTD4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareMode {
    /// The param-turn verdict (corrected AE4): exactly-two-key param diff, code
    /// hash / fingerprint / range equal, universe equal-or-explained.
    Param,
    /// The data-turn verdict: zero-key param diff + code-hash equality; the
    /// fingerprint / range / universe deltas are reported and require a
    /// supplied explanation.
    Data,
}

/// `runs compare` config.
#[derive(Debug, Clone)]
pub struct CompareConfig {
    /// The data home.
    pub data_home: PathBuf,
    /// The two run ids; `None` defaults to the two newest finalized runs.
    pub run_a: Option<String>,
    pub run_b: Option<String>,
    /// The verdict mode.
    pub mode: CompareMode,
    /// The operator-supplied delta explanation (data mode; universe-delta clause
    /// in param mode).
    pub explanation: Option<String>,
}

/// A `runs compare` outcome.
#[derive(Debug, Clone)]
pub struct CompareOutcome {
    /// Whether the verdict passed.
    pub pass: bool,
    /// The verdict/report lines (printed by the bin).
    pub lines: Vec<String>,
}

/// The changed param serde keys between two manifests (KTD4/AE3).
pub fn param_diff(a: &OrbParams, b: &OrbParams) -> Vec<String> {
    let va = serde_json::to_value(a).unwrap_or(serde_json::Value::Null);
    let vb = serde_json::to_value(b).unwrap_or(serde_json::Value::Null);
    let (Some(oa), Some(ob)) = (va.as_object(), vb.as_object()) else {
        return Vec::new();
    };
    let mut keys: BTreeSet<&String> = oa.keys().collect();
    keys.extend(ob.keys());
    keys.into_iter()
        .filter(|k| oa.get(*k) != ob.get(*k))
        .cloned()
        .collect()
}

/// Resolve the two manifests to compare from the config (KTD4).
fn resolve_pair(cfg: &CompareConfig) -> anyhow::Result<(String, Manifest, String, Manifest)> {
    let (a_id, b_id) = match (&cfg.run_a, &cfg.run_b) {
        (Some(a), Some(b)) => (a.clone(), b.clone()),
        (None, None) => {
            // Default: the two newest finalized runs (numeric-version ordered).
            let runs = ordered_runs(&cfg.data_home);
            if runs.len() < 2 {
                anyhow::bail!(
                    "runs compare needs two runs; the registry holds {} (pass LS_COMPARE_A / LS_COMPARE_B)",
                    runs.len()
                );
            }
            (runs[runs.len() - 2].clone(), runs[runs.len() - 1].clone())
        }
        // Refuse a single-sided selection rather than silently defaulting to the
        // two newest — an operator who named one run must name both.
        _ => anyhow::bail!(
            "runs compare: set both LS_COMPARE_A and LS_COMPARE_B, or neither (defaults to the two newest)"
        ),
    };
    let a = read_manifest(&cfg.data_home, &a_id)?;
    let b = read_manifest(&cfg.data_home, &b_id)?;
    Ok((a_id, a, b_id, b))
}

/// Compare two run manifests under the configured verdict (R4; AE3; R13's
/// equal-or-explained clause).
pub fn compare(cfg: &CompareConfig) -> anyhow::Result<CompareOutcome> {
    let (a_id, a, b_id, b) = resolve_pair(cfg)?;
    let mut lines = vec![format!("comparing {a_id} -> {b_id}")];
    let diff = param_diff(&a.params, &b.params);
    lines.push(format!("param diff: {diff:?}"));

    let code_equal = a.strategy_code_hash == b.strategy_code_hash;
    let fp_equal = a.catalog_fingerprint == b.catalog_fingerprint;
    let range_equal = a.data_range == b.data_range;
    let universe_equal = a.universe_hash == b.universe_hash;
    // A metadata-gated run vs a legacy run (or two runs against different
    // re-captured artifacts) is a selection-identity difference even when the
    // selected sets happen to coincide (plan 2026-07-10-003 KTD2) — hard FAIL
    // in param mode, explanation-required delta in data mode.
    let metadata_equal = a.universe_metadata_hash == b.universe_metadata_hash;

    let pass = match cfg.mode {
        CompareMode::Param => {
            let mut ok = true;
            // Exactly {strategy_version, one param}.
            let has_version = diff.iter().any(|k| k == "strategy_version");
            let param_keys: Vec<&String> =
                diff.iter().filter(|k| *k != "strategy_version").collect();
            if diff.len() != 2 || !has_version || param_keys.len() != 1 {
                lines.push(format!(
                    "FAIL: param diff must be exactly {{strategy_version, one param}}, got {diff:?}"
                ));
                ok = false;
            } else {
                lines.push(format!("param-only delta: {} + strategy_version", param_keys[0]));
            }
            if !code_equal {
                lines.push("FAIL: strategy_code_hash differs".to_string());
                ok = false;
            }
            if !fp_equal {
                lines.push("FAIL: catalog_fingerprint differs (in-range data drift)".to_string());
                ok = false;
            }
            if !range_equal {
                lines.push("FAIL: data_range differs".to_string());
                ok = false;
            }
            if !universe_equal {
                match &cfg.explanation {
                    Some(exp) => lines.push(format!(
                        "universe_hash differs — explained: {}",
                        nautilus_ls::scrub::scrub_secrets(exp)
                    )),
                    None => {
                        lines.push(
                            "FAIL: universe_hash differs with no explanation (equal-or-explained)"
                                .to_string(),
                        );
                        ok = false;
                    }
                }
            }
            if !metadata_equal {
                lines.push(
                    "FAIL: universe_metadata_hash differs — the runs were selected under \
                     different (or gated vs ungated) reference-data artifacts, so the delta \
                     cannot be attributed to the governed parameter"
                        .to_string(),
                );
                ok = false;
            }
            ok
        }
        CompareMode::Data => {
            let mut ok = true;
            if !diff.is_empty() {
                lines.push(format!("FAIL: data turn requires a zero-key param diff, got {diff:?}"));
                ok = false;
            }
            if !code_equal {
                lines.push("FAIL: strategy_code_hash differs (data turn changes data, not code)".to_string());
                ok = false;
            }
            // The wider slice legitimately changes fingerprint/range/universe;
            // report each delta and require an explanation (KTD4).
            let deltas: Vec<&str> = [
                (!fp_equal).then_some("catalog_fingerprint"),
                (!range_equal).then_some("data_range"),
                (!universe_equal).then_some("universe_hash"),
                (!metadata_equal).then_some("universe_metadata_hash"),
            ]
            .into_iter()
            .flatten()
            .collect();
            if deltas.is_empty() {
                lines.push("no data deltas (fingerprint/range/universe all equal)".to_string());
            } else {
                lines.push(format!("data deltas: {deltas:?}"));
                match &cfg.explanation {
                    Some(exp) => lines.push(format!(
                        "delta explained: {}",
                        nautilus_ls::scrub::scrub_secrets(exp)
                    )),
                    None => {
                        lines.push(
                            "FAIL: data deltas require an explanation (LS_COMPARE_EXPLANATION)"
                                .to_string(),
                        );
                        ok = false;
                    }
                }
            }
            ok
        }
    };

    lines.push(format!("verdict: {}", if pass { "PASS" } else { "FAIL" }));
    Ok(CompareOutcome { pass, lines })
}

// ===========================================================================
// U4 — replay guard
// ===========================================================================

/// The replay command's config (KTD7).
#[derive(Debug, Clone)]
pub struct ReplayConfig {
    /// The recorded envelope stream to re-evaluate. Defaults (via
    /// [`ReplayConfig::from_env`]) to the cross-run registry.
    pub stream_path: PathBuf,
    /// The swapped guardrail's proposal-bounds cap.
    pub max_relative_change: f64,
}

/// A replay outcome.
#[derive(Debug, Clone)]
pub struct ReplayOutcome {
    /// Whether the command refused (zero evaluated cycles).
    pub refused: bool,
    /// Cycles actually re-evaluated under the swapped guardrail.
    pub evaluated_count: usize,
    /// Cycles whose guardrail outcome diverged.
    pub delta_count: usize,
    /// The first divergence index, when any.
    pub first_divergence: Option<usize>,
    /// The report lines.
    pub lines: Vec<String>,
}

/// Guardrail-swap replay with the zero-evaluated refusal (R5, AE4, KTD7). An
/// evaluated count of zero is a telemetry-only stream — refused, never read as
/// "no divergence".
pub fn replay_guard(cfg: &ReplayConfig) -> anyhow::Result<ReplayOutcome> {
    let envelopes = read_envelopes(&cfg.stream_path)?;
    let result = replay(
        &envelopes,
        &ProposalBoundsGuardrail { max_relative_change: cfg.max_relative_change },
    );
    let mut lines = vec![format!(
        "replayed {} envelopes under proposal_bounds cap {:.4}",
        envelopes.len(),
        cfg.max_relative_change
    )];
    if result.evaluated_count == 0 {
        lines.push(
            "REFUSED: zero cycles evaluated — telemetry-only stream (no guardrail agreement tested)"
                .to_string(),
        );
        return Ok(ReplayOutcome {
            refused: true,
            evaluated_count: 0,
            delta_count: result.delta_count,
            first_divergence: result.first_divergence,
            lines,
        });
    }
    lines.push(format!(
        "evaluated {} cycles, {} diverged",
        result.evaluated_count, result.delta_count
    ));
    match result.first_divergence {
        Some(i) => lines.push(format!("first divergence at cycle {i}")),
        None => lines.push("no divergence under the swapped guardrail".to_string()),
    }
    Ok(ReplayOutcome {
        refused: false,
        evaluated_count: result.evaluated_count,
        delta_count: result.delta_count,
        first_divergence: result.first_divergence,
        lines,
    })
}

// ===========================================================================
// U5 — catalog status
// ===========================================================================

/// `catalog status` config (KTD6).
#[derive(Debug, Clone)]
pub struct StatusConfig {
    /// The data home.
    pub data_home: PathBuf,
    /// An optional operator-supplied expected range, turning on both-direction
    /// span checks (front truncation is undetectable from the checkpoint alone).
    pub expected_range: Option<DataRange>,
}

/// Per-(instrument, bar-kind) facts.
#[derive(Debug, Clone)]
pub struct TripleStatus {
    /// Instrument id (`{shcode}.XKRX`).
    pub instrument: String,
    /// The bar-series label (`1-DAY`, `n-MINUTE`).
    pub bar_kind: String,
    /// Bar count in the catalog for the triple.
    pub count: usize,
    /// Earliest bar KST date.
    pub first: NaiveDate,
    /// Latest bar KST date.
    pub last: NaiveDate,
    /// Undershoot flags (tail vs watermark; front/tail vs expected range).
    pub flags: Vec<String>,
}

/// A `catalog status` outcome.
#[derive(Debug, Clone)]
pub struct StatusOutcome {
    /// Whether the catalog is a go (no undershoot, catalog + checkpoint present).
    pub go: bool,
    /// Per-triple facts, in `(instrument, bar-kind)` order.
    pub triples: Vec<TripleStatus>,
    /// The report lines.
    pub lines: Vec<String>,
}

/// The ingest→backtest go/no-go (R6; AE5; KTD6). Facts always; full undershoot
/// only against an operator-supplied expected range.
pub async fn catalog_status(cfg: &StatusConfig) -> anyhow::Result<StatusOutcome> {
    // The catalog dir must exist BEFORE catalog construction — ParquetDataCatalog
    // canonicalizes on `new` and fails on a missing dir (the block-on-from-async
    // trap). A missing catalog is an explicit no-go, never a panic.
    let catalog_path = cfg.data_home.join("catalog");
    if !catalog_path.exists() {
        anyhow::bail!("no catalog at {} — ingest first", catalog_path.display());
    }
    // A missing checkpoint is a no-go (KTD6): the tail check has no authority
    // without it.
    let checkpoint_path = catalog_path.join("ingest-checkpoint.json");
    if !checkpoint_path.exists() {
        anyhow::bail!("no ingest checkpoint at {} — ingest first", checkpoint_path.display());
    }
    let checkpoint = Checkpoint::load(&checkpoint_path)
        .map_err(|e| anyhow::anyhow!("checkpoint: {e}"))?;

    let bars = read_all_bars(&catalog_path).await?;

    // Group by (instrument, bar-kind).
    let mut groups: BTreeMap<(String, String), Vec<&Bar>> = BTreeMap::new();
    for b in &bars {
        let key = (b.bar_type.instrument_id().to_string(), bar_label(b));
        groups.entry(key).or_default().push(b);
    }
    if groups.is_empty() {
        anyhow::bail!("catalog holds no bars — nothing to report (no-go)");
    }

    let mut triples = Vec::new();
    let mut go = true;
    let mut lines = Vec::new();
    for ((instrument, bar_kind), group) in groups {
        let first = group.iter().map(|b| kst_date_of(b.ts_event)).min().expect("non-empty group");
        let last = group.iter().map(|b| kst_date_of(b.ts_event)).max().expect("non-empty group");
        let mut flags = Vec::new();

        // Tail check vs the checkpoint watermark (always). A span ending before
        // the covered watermark undershoots the completed range (AE5). The
        // watermark is compared at its last *session* (weekday), not the raw
        // date: accumulate advances the watermark onto weekends with no session,
        // so a healthy Friday-closed catalog must not false-flag when the last
        // ingest ran over a weekend.
        if let Some(wm) = checkpoint.watermark(&instrument, &bar_kind) {
            let last_session = last_weekday_on_or_before(wm);
            if last < last_session {
                flags.push(format!(
                    "tail undershoot: last {last} < last session {last_session} (watermark {wm})"
                ));
            }
        }
        // Both-direction checks vs an operator-supplied expected range.
        if let Some(exp) = &cfg.expected_range {
            if let Ok(exp_start) = NaiveDate::parse_from_str(exp.start.trim(), "%Y%m%d") {
                if first > exp_start {
                    flags.push(format!("front truncation: first {first} > expected start {exp_start}"));
                }
            }
            if let Ok(exp_end) = NaiveDate::parse_from_str(exp.end.trim(), "%Y%m%d") {
                if last < exp_end {
                    flags.push(format!("tail undershoot: last {last} < expected end {exp_end}"));
                }
            }
        }
        if !flags.is_empty() {
            go = false;
        }
        lines.push(format!(
            "{instrument} {bar_kind}: {} bars, {first}..{last}{}",
            group.len(),
            if flags.is_empty() { String::new() } else { format!("  [{}]", flags.join("; ")) }
        ));
        triples.push(TripleStatus {
            instrument,
            bar_kind,
            count: group.len(),
            first,
            last,
            flags,
        });
    }
    lines.push(format!("status: {}", if go { "GO" } else { "NO-GO" }));
    Ok(StatusOutcome { go, triples, lines })
}

// ===========================================================================
// catalog compact (U5 — write-side remediation)
// ===========================================================================

/// `catalog compact` config.
#[derive(Debug, Clone)]
pub struct CompactConfig {
    /// The data home.
    pub data_home: PathBuf,
}

/// A `catalog compact` outcome.
#[derive(Debug, Clone)]
pub struct CompactCliOutcome {
    /// Whether any series was refused for value divergence (drives a non-zero exit).
    pub refused: bool,
    /// The report lines (before/after file + bar counts, per-series outcome).
    pub lines: Vec<String>,
}

/// Collapse byte-identical duplicate bars per series into a clean file set,
/// reporting before/after file and bar counts (R8). A value-divergent series is
/// refused and left untouched (R9); the checkpoint is never touched (R10). Wraps
/// the adapter's [`compact_catalog`], which holds the ingest advisory lock.
pub async fn catalog_compact(cfg: &CompactConfig) -> anyhow::Result<CompactCliOutcome> {
    let catalog_path = cfg.data_home.join("catalog");
    if !catalog_path.exists() {
        anyhow::bail!("no catalog at {} — ingest first", catalog_path.display());
    }
    let report = compact_catalog(&catalog_path)
        .await
        .map_err(|e| anyhow::anyhow!("compact: {e}"))?;

    let mut lines = Vec::new();
    for s in &report.series {
        let outcome = match s.outcome {
            CompactOutcome::Compacted => "compacted",
            CompactOutcome::Clean => "clean",
            CompactOutcome::RefusedDivergent => {
                "REFUSED (value-divergent same-timestamp rows — left untouched, owned by the heal path)"
            }
        };
        lines.push(format!(
            "{}: {} files -> {} files, {} bars -> {} bars [{outcome}]",
            s.bar_type, s.files_before, s.files_after, s.bars_before, s.bars_after
        ));
    }
    let refused = report.any_refused();
    lines.push(format!(
        "compact: {}",
        if refused { "REFUSED (some series left untouched — see above)" } else { "OK" }
    ));
    Ok(CompactCliOutcome { refused, lines })
}

// ===========================================================================
// U6 — analyze --scaffold
// ===========================================================================

/// `analyze --scaffold` config.
#[derive(Debug, Clone)]
pub struct ScaffoldConfig {
    /// The data home.
    pub data_home: PathBuf,
    /// The run to scaffold an analysis for.
    pub run_id: String,
}

/// A scaffold outcome.
#[derive(Debug, Clone)]
pub struct ScaffoldOutcome {
    /// The written `analysis.md` path.
    pub path: PathBuf,
    /// The report lines.
    pub lines: Vec<String>,
}

/// Pre-fill a run's `analysis.md` with run facts (R7). Structured facts render
/// verbatim (symbols/ids never hit the free-text scrub, KTD8); the one prose
/// carrier (echoed data-quality observations) routes through the scrub. Refuses
/// if `analysis.md` already exists.
pub fn analyze_scaffold(cfg: &ScaffoldConfig) -> anyhow::Result<ScaffoldOutcome> {
    let run_dir = cfg.data_home.join("runs").join(&cfg.run_id);
    let analysis_path = run_dir.join(ANALYSIS_FILE);
    if analysis_path.exists() {
        anyhow::bail!(
            "{} already exists — refusing to overwrite an authored analysis",
            analysis_path.display()
        );
    }
    let manifest: Manifest = read_json(&run_dir.join(MANIFEST_FILE))?;
    let performance: PerformanceReport = read_json(&run_dir.join(PERFORMANCE_FILE))?;
    let data_quality: DataQualityReport = read_json(&run_dir.join(DATA_QUALITY_FILE))?;

    let num_trades = performance.summary.get("num_trades").copied().unwrap_or(0.0);
    let pnl_total = performance.summary.get("pnl_total").copied().unwrap_or(0.0);

    // The turn-5 edge-quality evaluation (R4, KTD-4): win-rate / expectancy / total
    // P&L read from the summary, single-symbol **dominance kept**, and the turn-3/4
    // trade-count + breadth floors **retired** (per-day trading clears those by
    // construction). The verdict is authored against these computed edge stats — not
    // eyeballed. Symbols render verbatim (structured, like the universe list — a
    // 6-digit shcode must not be masked).
    let edge = performance.edge_evaluation();
    let pf = |pass: bool| if pass { "PASS" } else { "FAIL" };
    let mut edge_rows = String::new();
    for s in &edge.per_symbol {
        edge_rows.push_str(&format!(
            "| `{}` | {} | {:.0} | {:.1}% |\n",
            s.symbol,
            s.trades,
            s.realized_pnl,
            s.abs_pnl_share * 100.0
        ));
    }
    if edge_rows.is_empty() {
        edge_rows.push_str("| _(no realized trades)_ | 0 | 0 | 0.0% |\n");
    }
    let edge_notes = if edge.failing_conditions.is_empty() {
        "_(none — positive expectancy with single-symbol dominance capped; the strategy advances)_"
            .to_string()
    } else {
        edge.failing_conditions.iter().map(|c| format!("- {c}")).collect::<Vec<_>>().join("\n")
    };
    // Degenerate all-zero P&L: the dominance share is undefined (denominator 0), so
    // rendering a bare "0.0%" against the ≤40% cap reads as a self-contradiction
    // ("0.0% → FAIL"). Show the fail reason instead, mirroring the named condition.
    let dom_display = if edge.degenerate_zero_pnl {
        "undefined (all-zero P&L)".to_string()
    } else {
        format!("{:.1}%", edge.max_abs_pnl_share * 100.0)
    };
    let fmt_opt = |v: Option<f64>| v.map(|x| format!("{x:.4}")).unwrap_or_else(|| "n/a".to_string());
    let win_rate_display = fmt_opt(edge.win_rate);
    let expectancy_display = fmt_opt(edge.expectancy);

    let mut params_rows = String::new();
    for (k, v) in manifest.params.numeric_summary() {
        params_rows.push_str(&format!("- `{k}`: {v:.4}\n"));
    }
    // Structured universe list — symbols render verbatim (a 6-digit shcode would
    // be masked by the free-text scrub, so it must not travel that route, KTD8).
    let mut universe = String::new();
    for sym in &data_quality.universe_snapshot {
        universe.push_str(&format!("- `{sym}`\n"));
    }
    if universe.is_empty() {
        universe.push_str("- (none selected)\n");
    }
    // The gap-noise summary (post-U7 counts): never-ingested instruments no
    // longer flood the report, so a gap here is real.
    let gap_count = data_quality.coverage_gaps.len();
    // The one free-text carrier — scrubbed, so an account-like token here is
    // masked while structured facts above stay intact (KTD8).
    let observations = if data_quality.observations.is_empty() {
        "_(none)_".to_string()
    } else {
        data_quality
            .observations
            .iter()
            .map(|o| format!("- {}", nautilus_ls::scrub::scrub_secrets(o)))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let content = format!(
        "# Loop analysis — {run_id}\n\
         \n\
         _Scaffolded by `lab-research analyze --scaffold`. Fill the verdict; the run\n\
         facts below are read from this run's four artifacts (co-located, R15)._\n\
         \n\
         ## Run under analysis\n\
         \n\
         - **Source:** {source}\n\
         - **Strategy:** `{strategy_id}` v{version}\n\
         - **Data range:** {start} – {end} (pinned)\n\
         \n\
         ## Parameters\n\
         \n\
         {params_rows}\n\
         ## What the artifacts show\n\
         \n\
         - **Performance:** `num_trades = {num_trades:.0}`, `pnl_total = {pnl_total:.4}` KRW.\n\
         - **Universe (`data_quality.json`):**\n\
         {universe}\n\
         - **Gap noise:** {gap_count} coverage-gap entries (never-ingested instruments\n\
           are filtered post-U7, so any entry here is a real gap).\n\
         - **Observations:**\n\
         {observations}\n\
         \n\
         ## Edge quality (R4) — computed\n\
         \n\
         Turn 5 judges the multi-session strategy on **edge quality** (expectancy /\n\
         win-rate) with single-symbol **dominance still capped**. The old trade-count\n\
         and symbol-breadth frequency bar is **retired** — per-day trading clears it by\n\
         construction, so a measurable edge implicitly proves the per-day reset fires.\n\
         The edge stats below are read from this run's `performance.json` summary (KTD-4).\n\
         \n\
         - **Win rate:** `{win_rate}`\n\
         - **Expectancy (KRW/trade):** `{expectancy}`\n\
         - **Total realized P&L (KRW):** `{pnl_total:.4}`\n\
         - **Closed trades:** `{num_trades:.0}`\n\
         - **(c) single-symbol dominance (≤ {dom_cap:.0}% of aggregate |P&L|):** `{dom_share}` → **{c_pf}**\n\
         \n\
         | Symbol | Trades | Realized P&L (KRW) | abs P&L share |\n\
         |---|---|---|---|\n\
         {edge_rows}\n\
         **Edge:** {is_edge}. **Notes:**\n\
         \n\
         {edge_notes}\n\
         \n\
         ## Verdict\n\
         \n\
         State one of **{keep}** / **{revert}** / **{insufficient}**, grounded in the edge\n\
         stats above. A **positive expectancy** with dominance capped is a real edge →\n\
         **{keep}** (the strategy advances). A flat / negative expectancy, or a tripped\n\
         dominance cap, → **{insufficient}** naming the next lever to try. Per R5 a\n\
         flat/negative edge is a **valid recorded outcome**, not a turn failure:\n\
         \n\
         > _verdict: TODO_\n",
        run_id = cfg.run_id,
        source = manifest.source.as_str(),
        strategy_id = manifest.strategy_id,
        version = manifest.strategy_version,
        start = manifest.data_range.start,
        end = manifest.data_range.end,
        win_rate = win_rate_display,
        expectancy = expectancy_display,
        dom_cap = crate::artifacts::performance::bar::DOMINANCE_CAP * 100.0,
        dom_share = dom_display,
        c_pf = pf(edge.dominance_pass),
        edge_rows = edge_rows.trim_end(),
        is_edge = if edge.is_edge { "yes" } else { "no" },
        edge_notes = edge_notes,
        keep = VERDICT_WORDS[0],
        revert = VERDICT_WORDS[1],
        insufficient = VERDICT_WORDS[2],
    );

    std::fs::write(&analysis_path, content)
        .map_err(|e| anyhow::anyhow!("writing {}: {e}", analysis_path.display()))?;
    Ok(ScaffoldOutcome {
        path: analysis_path.clone(),
        lines: vec![format!("scaffolded {}", analysis_path.display())],
    })
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| anyhow::anyhow!("parsing {}: {e}", path.display()))
}

// ===========================================================================
// Bin entry point + env config
// ===========================================================================

/// A usage string enumerating the valid subcommands (KTD2).
const USAGE: &str = "usage: lab-research <turn | runs compare | replay | catalog status | catalog compact | analyze --scaffold | report mfe | report tiers>";

/// Parse an optional `YYYYMMDD` range from a pair of env vars, returning `None`
/// when neither is set and erroring when only one is.
fn env_range(start_key: &str, end_key: &str) -> anyhow::Result<Option<DataRange>> {
    match (std::env::var(start_key).ok(), std::env::var(end_key).ok()) {
        (Some(start), Some(end)) => {
            // Validate the format here (not deep in a consumer): an unparseable
            // date must be a hard error, never a silently-skipped span check on a
            // go/no-go gate.
            for (key, val) in [(start_key, &start), (end_key, &end)] {
                NaiveDate::parse_from_str(val.trim(), "%Y%m%d")
                    .map_err(|_| anyhow::anyhow!("{key} must be YYYYMMDD, got {val:?}"))?;
            }
            Ok(Some(DataRange { start, end }))
        }
        (None, None) => Ok(None),
        _ => anyhow::bail!("{start_key} and {end_key} must be set together"),
    }
}

fn env_f64(key: &str) -> anyhow::Result<Option<f64>> {
    match std::env::var(key) {
        Ok(v) => Ok(Some(v.parse().map_err(|_| anyhow::anyhow!("{key} must be a number, got {v:?}"))?)),
        Err(_) => Ok(None),
    }
}

/// The CLI entry point: install scrub, dispatch the subcommand, print results,
/// and map the outcome to an exit code. A verdict failure (deny / FAIL / refuse
/// / no-go) is a non-zero exit, not an error; a genuine error is scrubbed and
/// exits non-zero too (KTD8).
pub fn main_cli() -> ExitCode {
    nautilus_ls::scrub::install();
    match dispatch() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {}", nautilus_ls::scrub::scrub_secrets(&e.to_string()));
            ExitCode::FAILURE
        }
    }
}

fn ok_fail(pass: bool) -> ExitCode {
    if pass {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn print_lines(lines: &[String]) {
    // Result lines are printed verbatim (KTD8): the commands render ids, symbols,
    // paths, and numbers as typed/structured values — a 6-digit KRX shcode or a
    // run-id timestamp would be masked by the account-number heuristic if routed
    // through the free-text scrub. The one external free-text field (a compare
    // explanation) is already scrubbed at embed time; the terminal-error path
    // (`main_cli`) still scrubs, catching any unexpected error text.
    for l in lines {
        println!("{l}");
    }
}

fn dispatch() -> anyhow::Result<ExitCode> {
    let sub = std::env::args().nth(1);
    match sub.as_deref() {
        Some("turn") => {
            let cfg = turn_config_from_env()?;
            let rt = tokio::runtime::Runtime::new()?;
            let out = rt.block_on(turn(cfg))?;
            print_lines(&out.lines);
            // A turn that ran a backtest succeeds; a governance refusal is a
            // non-zero exit.
            Ok(ok_fail(out.ran))
        }
        Some("runs") => match std::env::args().nth(2).as_deref() {
            Some("compare") => {
                let out = compare(&compare_config_from_env()?)?;
                print_lines(&out.lines);
                Ok(ok_fail(out.pass))
            }
            other => anyhow::bail!("unknown `runs` subcommand {other:?} — want `runs compare`\n{USAGE}"),
        },
        Some("replay") => {
            let out = replay_guard(&replay_config_from_env()?)?;
            print_lines(&out.lines);
            Ok(ok_fail(!out.refused))
        }
        Some("catalog") => match std::env::args().nth(2).as_deref() {
            Some("status") => {
                let rt = tokio::runtime::Runtime::new()?;
                let out = rt.block_on(catalog_status(&status_config_from_env()?))?;
                print_lines(&out.lines);
                Ok(ok_fail(out.go))
            }
            Some("compact") => {
                let rt = tokio::runtime::Runtime::new()?;
                let out = rt.block_on(catalog_compact(&compact_config_from_env()?))?;
                print_lines(&out.lines);
                Ok(ok_fail(!out.refused))
            }
            other => anyhow::bail!("unknown `catalog` subcommand {other:?} — want `catalog status` | `catalog compact`\n{USAGE}"),
        },
        Some("analyze") => match std::env::args().nth(2).as_deref() {
            Some("--scaffold") => {
                let out = analyze_scaffold(&scaffold_config_from_env()?)?;
                print_lines(&out.lines);
                Ok(ExitCode::SUCCESS)
            }
            other => anyhow::bail!("unknown `analyze` mode {other:?} — want `analyze --scaffold`\n{USAGE}"),
        },
        Some("report") => match std::env::args().nth(2).as_deref() {
            Some("mfe") => {
                let out = crate::runner::report::report_mfe(&report_config_from_env()?)?;
                print_lines(&out.lines);
                // The exit code reflects I/O success only — the distribution's
                // content (censored / out-of-band) is never a failure.
                Ok(ExitCode::SUCCESS)
            }
            Some("tiers") => {
                let rt = tokio::runtime::Runtime::new()?;
                let out = rt.block_on(crate::runner::report::report_tiers(&tiers_config_from_env()?))?;
                print_lines(&out.lines);
                // A red pre-check is a valid completion (AE2), not a failure —
                // the exit code reflects integrity + I/O only.
                Ok(ExitCode::SUCCESS)
            }
            other => anyhow::bail!("unknown `report` subcommand {other:?} — want `report mfe` | `report tiers`\n{USAGE}"),
        },
        other => anyhow::bail!("unknown subcommand {other:?}\n{USAGE}"),
    }
}

fn turn_config_from_env() -> anyhow::Result<TurnConfig> {
    let data_home = data_home_from_env()?;
    let mut cfg = TurnConfig::new(data_home, Utc::now());
    cfg.override_param = std::env::var("LS_TURN_PARAM").ok().filter(|s| !s.trim().is_empty());
    cfg.override_target = env_f64("LS_TURN_VALUE")?;
    if cfg.override_param.is_some() && cfg.override_target.is_none() {
        anyhow::bail!("LS_TURN_VALUE is required when LS_TURN_PARAM is set");
    }
    cfg.range = env_range("LS_TURN_SDATE", "LS_TURN_EDATE")?;
    if let Ok(step) = std::env::var("LS_TURN_MINUTE_STEP") {
        // Error loudly on a typo rather than silently defaulting to step 1 (the
        // same discipline as env_f64) — a wrong bar-kind run is worse than a stop.
        cfg.minute_step = step
            .parse()
            .map_err(|_| anyhow::anyhow!("LS_TURN_MINUTE_STEP must be an integer, got {step:?}"))?;
    }
    if let Some(bal) = env_f64("LS_TURN_BALANCE")? {
        cfg.starting_balance = bal;
    }
    // U4 / KTD-5: pin the expected resolved v3 identity so a fresh home missing the
    // seeded manifest stops instead of running defaults.
    cfg.expect_version = match std::env::var("LS_TURN_EXPECT_VERSION") {
        Ok(v) => Some(
            v.parse()
                .map_err(|_| anyhow::anyhow!("LS_TURN_EXPECT_VERSION must be an integer, got {v:?}"))?,
        ),
        Err(_) => None,
    };
    cfg.expect_gap_min_pct = env_f64("LS_TURN_EXPECT_GAP")?;
    Ok(cfg)
}

fn compare_config_from_env() -> anyhow::Result<CompareConfig> {
    let mode = match std::env::var("LS_COMPARE_MODE").as_deref() {
        Ok("data") => CompareMode::Data,
        Ok("param") | Err(_) => CompareMode::Param,
        Ok(other) => anyhow::bail!("LS_COMPARE_MODE must be param | data, got {other:?}"),
    };
    Ok(CompareConfig {
        data_home: data_home_from_env()?,
        run_a: std::env::var("LS_COMPARE_A").ok().filter(|s| !s.trim().is_empty()),
        run_b: std::env::var("LS_COMPARE_B").ok().filter(|s| !s.trim().is_empty()),
        mode,
        explanation: std::env::var("LS_COMPARE_EXPLANATION").ok().filter(|s| !s.trim().is_empty()),
    })
}

fn replay_config_from_env() -> anyhow::Result<ReplayConfig> {
    let data_home = data_home_from_env()?;
    // Default stream: the cross-run decision registry the turn command appends
    // to (KTD7 — the accumulated stream is what U9 replays).
    let stream_path = match std::env::var("LS_REPLAY_STREAM") {
        Ok(p) => PathBuf::from(p),
        Err(_) => data_home.join("decisions").join(crate::agent::recording::DECISIONS_FILE),
    };
    let max_relative_change = env_f64("LS_REPLAY_CAP")?.unwrap_or(0.25);
    Ok(ReplayConfig { stream_path, max_relative_change })
}

fn status_config_from_env() -> anyhow::Result<StatusConfig> {
    Ok(StatusConfig {
        data_home: data_home_from_env()?,
        expected_range: env_range("LS_STATUS_SDATE", "LS_STATUS_EDATE")?,
    })
}

fn compact_config_from_env() -> anyhow::Result<CompactConfig> {
    Ok(CompactConfig { data_home: data_home_from_env()? })
}

fn tiers_config_from_env() -> anyhow::Result<crate::runner::report::TiersConfig> {
    Ok(crate::runner::report::TiersConfig {
        data_home: data_home_from_env()?,
        run_id: std::env::var("LS_REPORT_RUN").ok().filter(|s| !s.trim().is_empty()),
        artifact_path: std::env::var("LS_REPORT_METADATA")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(std::path::PathBuf::from),
    })
}

fn report_config_from_env() -> anyhow::Result<crate::runner::report::ReportConfig> {
    Ok(crate::runner::report::ReportConfig {
        data_home: data_home_from_env()?,
        // Absent → default to the latest finalized run, marked as defaulted in
        // the report header. (Unlike `analyze --scaffold`, which hard-requires
        // LS_ANALYZE_RUN because it WRITES into the run dir — the report is
        // read-only, so a default is safe.)
        run_id: std::env::var("LS_REPORT_RUN").ok().filter(|s| !s.trim().is_empty()),
    })
}

fn scaffold_config_from_env() -> anyhow::Result<ScaffoldConfig> {
    Ok(ScaffoldConfig {
        data_home: data_home_from_env()?,
        run_id: std::env::var("LS_ANALYZE_RUN")
            .map_err(|_| anyhow::anyhow!("LS_ANALYZE_RUN is required (the run id to scaffold)"))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_order_key_sorts_the_version_suffix_numerically() {
        // Same second-granularity stamp, different versions: lexical order would
        // put -v10 before -v2; the numeric key must order it last.
        let mut ids = vec![
            "20260101T000000Z-backtest-orb-v2".to_string(),
            "20260101T000000Z-backtest-orb-v10".to_string(),
            "20260101T000000Z-backtest-orb-v9".to_string(),
        ];
        ids.sort_by(|a, b| run_order_key(a).cmp(&run_order_key(b)));
        assert_eq!(ids.last().unwrap(), "20260101T000000Z-backtest-orb-v10", "v10 is newest");
        assert_eq!(ids.first().unwrap(), "20260101T000000Z-backtest-orb-v2", "v2 is oldest");
        // A run id with no -v suffix degrades to version 0, never panics.
        assert_eq!(run_order_key("no-version-here").1, 0);
    }
}
