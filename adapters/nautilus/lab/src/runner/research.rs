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

use chrono::{DateTime, NaiveDate, Utc};

use nautilus_ls::ingest::checkpoint::Checkpoint;
use nautilus_ls::ingest::{compact_catalog, kst_date_of, read_all_bars, CompactOutcome};
use nautilus_ls_calendar::{AsOfView, CalendarAdoption, DateRange, SessionSearch};
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
use crate::runner::diagnose::{read_gate_verdict, GateExit};
use crate::trials::{LookKind, SampleLineage, TrialRecord, TrialsLedger};

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

/// The newest finalized **ORB** run's id + manifest, or `None` on a fresh registry
/// (KTD1: current-params authority is the latest finalized manifest).
///
/// # The strategy partition (P7/U8, KTD14, R24)
///
/// This used to be a bare newest-by-run-id lookup, which was correct while `"orb"` was the
/// only strategy in the registry. It no longer is: the daily multi-session path writes into
/// the same `<data>/runs` tree, and nothing structurally separates the two homes — the
/// separation is one `LS_DATA_HOME` slip deep.
///
/// The filter is applied **here**, in the default, rather than threaded as an argument
/// through the seven consumers that trust this function — `turn()`'s params adoption
/// (`:392`), range inheritance (`:426`), `decide_keep_or_revert`
/// (`governed.rs:196`), the diagnose trial anchor (`:2133`), and the three reporting
/// commands (`report.rs:328`, `:491`, `:1015`). That is the same reasoning KTD5 applies to
/// `strategy_code_hash()`: a parameter every call site passes the literal `"orb"` to is
/// seven chances to forget, on lookups whose failure mode is *silent* — a daily run
/// finalized after the newest ORB run would become the ORB turn's adopted params, its
/// inherited range, its KEEP/REVERT baseline, and its trial anchor, all without an error.
/// A caller that genuinely wants another strategy asks for it by name via
/// [`latest_finalized_run_for`].
///
/// Every filter here is a **no-op** against the existing registry, where all eight
/// committed manifests carry `strategy_id: "orb"`.
pub fn latest_finalized_run(data_home: &Path) -> anyhow::Result<Option<(String, Manifest)>> {
    latest_finalized_run_for(data_home, crate::params::STRATEGY_ID)
}

/// The newest finalized run of `strategy_id`, or `None` when the registry holds none.
///
/// Reads **every** manifest newest-first rather than only the newest run, because the
/// newest run may now belong to the other strategy.
///
/// # The newest manifest is read strictly; only older ones are skipped
///
/// Tolerating an unreadable manifest is what lets the partition scan past legacy artifacts
/// deeper in the registry — a strict read of all of them would turn a previously-succeeding
/// lookup into a hard error the first time an *old* manifest failed to parse. But that
/// justification covers exactly the manifests the pre-partition lookup never opened. The
/// old code read `ordered_runs().last()` and propagated its parse error, so extending the
/// same silence to the newest run would be a **new** silence, not a preserved one — and the
/// worst one available: with a valid older ORB run present, a corrupt newest manifest would
/// resolve the older run as the apparent head, and every consumer would adopt stale params,
/// a stale range, and a stale KEEP/REVERT baseline with no error. With no older run it
/// returns `None`, which `decide_keep_or_revert` cannot tell apart from a fresh registry and
/// treats as licence to skip the comparison entirely.
///
/// So: the newest run is read strictly (whatever strategy it belongs to — that is what the
/// old lookup did), and the skip-on-unreadable scan applies only to the remainder.
///
/// # Errors
///
/// Propagates the read/parse error when the **newest** finalized run's manifest is
/// unreadable.
pub fn latest_finalized_run_for(
    data_home: &Path,
    strategy_id: &str,
) -> anyhow::Result<Option<(String, Manifest)>> {
    let ordered = ordered_runs(data_home);
    let Some((newest, older)) = ordered.split_last() else {
        return Ok(None);
    };
    let newest_manifest = read_manifest(data_home, newest)?;
    if newest_manifest.strategy_id == strategy_id {
        return Ok(Some((newest.clone(), newest_manifest)));
    }
    Ok(older
        .iter()
        .rev()
        .filter_map(|rid| read_manifest(data_home, rid).ok().map(|m| (rid.clone(), m)))
        .find(|(_rid, m)| m.strategy_id == strategy_id))
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

/// A proven Trading-Session boundary for a catalog readiness check (U11, KTD8). Computed
/// purely from the injected calendar view; the Enforced calendar ACTS on it, with no
/// weekday fallback (#189). The proof-preserving
/// tri-state distinction is intact — an `Unknown` at the boundary is never collapsed into
/// a session or a proven absence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionBoundary {
    /// A positively proven Trading Session marks the boundary date.
    Session(NaiveDate),
    /// The relevant span is positively proven all-`Closed` — no session boundary to
    /// compare a span endpoint against (vacuously satisfied, never an undershoot).
    NoSession,
    /// An `Unknown` sits where it could be the boundary — the fact cannot be proven, so a
    /// boundary-relevant Unknown fails closed under Enforced (`NO-GO — calendar indeterminate`).
    Indeterminate,
    /// No calendar was injected, or the boundary date is outside the materialized coverage
    /// window (a `QueryError::OutOfRange`) — unavailable (`NO-GO — calendar unavailable`).
    Unavailable,
}

/// The per-consumer calendar seam for `catalog status` (U11, KTD8). Carries, when a snapshot
/// loaded and authorized, an [`AsOfView`]. `Copy` (the view is a borrow + an instant).
/// Construct [`new`](Self::new) at the composition root. Enforced-only after the #189 weekday
/// retirement (U7/U10): the calendar is authoritative with no weekday fallback.
#[derive(Debug, Clone, Copy)]
pub struct CatalogCalendarGate<'c> {
    view: Option<AsOfView<'c>>,
}

impl<'c> CatalogCalendarGate<'c> {
    /// Build a gate with an optional as-of view (`None` = calendar unavailable — a
    /// missing/failed snapshot, which fails closed under the sole surviving Enforced posture,
    /// #189 U7/U10).
    pub fn new(view: Option<AsOfView<'c>>) -> Self {
        Self { view }
    }

    /// The LAST proven Trading Session on or before `date` (proof-preserving). The real
    /// tail boundary a span must reach — replaces the weekday [`last_weekday_on_or_before`]
    /// walk-back under Enforced. A boundary date outside the materialized window is
    /// [`Unavailable`](SessionBoundary::Unavailable) (fail closed), never clamped.
    pub fn last_session_on_or_before(&self, date: NaiveDate) -> SessionBoundary {
        let Some(view) = self.view else {
            return SessionBoundary::Unavailable;
        };
        let coverage = view.calendar().coverage();
        if date < coverage.materialized_from || date > coverage.materialized_through {
            return SessionBoundary::Unavailable;
        }
        let range = match DateRange::inclusive(coverage.materialized_from, date) {
            Ok(range) => range,
            Err(_) => return SessionBoundary::Unavailable,
        };
        match view.last_session(&range) {
            Ok(SessionSearch::Found(d)) => SessionBoundary::Session(d),
            Ok(SessionSearch::None) => SessionBoundary::NoSession,
            Ok(SessionSearch::Indeterminate) => SessionBoundary::Indeterminate,
            Err(_) => SessionBoundary::Unavailable,
        }
    }

    /// The FIRST proven Trading Session on or after `date` (proof-preserving). The real
    /// front boundary an expected range's first bar must not fall behind. A boundary date
    /// outside the materialized window is [`Unavailable`](SessionBoundary::Unavailable).
    pub fn first_session_on_or_after(&self, date: NaiveDate) -> SessionBoundary {
        let Some(view) = self.view else {
            return SessionBoundary::Unavailable;
        };
        let coverage = view.calendar().coverage();
        if date < coverage.materialized_from || date > coverage.materialized_through {
            return SessionBoundary::Unavailable;
        }
        let range = match DateRange::inclusive(date, coverage.materialized_through) {
            Ok(range) => range,
            Err(_) => return SessionBoundary::Unavailable,
        };
        match view.first_session(&range) {
            Ok(SessionSearch::Found(d)) => SessionBoundary::Session(d),
            Ok(SessionSearch::None) => SessionBoundary::NoSession,
            Ok(SessionSearch::Indeterminate) => SessionBoundary::Indeterminate,
            Err(_) => SessionBoundary::Unavailable,
        }
    }

    /// Whether the injected calendar's freshness is stale at the view's as-of instant.
    /// `false` when no view is injected (there is nothing to be stale). Advisory only — a
    /// stale-but-established boundary is a GO with a prominent warning, never a status flip.
    pub fn is_stale(&self) -> bool {
        self.view.map(|v| v.freshness().any_stale()).unwrap_or(false)
    }

    /// The freshness dimension(s) that BOUND a queried boundary date and are STALE at the
    /// as-of instant (U11, KTD5). Consumer-owned relevance: the calendar core exposes only
    /// snapshot-global dimensions, so the catalog decides which one bounds a given date —
    /// a boundary already retrospectively re-checked (`<= retrospectively_checked_through`)
    /// keys on the historical dimensions (`incremental`, `full_history`, `kasi_holiday_facts`);
    /// a boundary in the forward/unverified zone (past `retrospectively_checked_through`)
    /// keys on `forward_readiness`. A snapshot stale only in a dimension that does NOT bound
    /// the date returns empty here (no spurious warning). Empty when no view is injected.
    pub fn stale_bounding_dimensions(&self, boundary: NaiveDate) -> Vec<&'static str> {
        let Some(view) = self.view else {
            return Vec::new();
        };
        let fresh = view.freshness();
        let coverage = view.calendar().coverage();
        let mut dims = Vec::new();
        if boundary > coverage.retrospectively_checked_through {
            // Forward/unverified zone — forward readiness is the dimension that bounds it.
            if fresh.forward_readiness.is_stale() {
                dims.push("forward_readiness");
            }
        } else {
            // Retrospectively re-checked historical zone — the historical dimensions bound it.
            if fresh.incremental.is_stale() {
                dims.push("incremental");
            }
            if fresh.full_history.is_stale() {
                dims.push("full_history");
            }
            if fresh.kasi_holiday_facts.is_stale() {
                dims.push("kasi_holiday_facts");
            }
        }
        dims
    }
}

// ===========================================================================
// U2 — the turn command
// ===========================================================================

/// The governed-flip binding (U6, R4/KTD1): the candidate whose recorded GO
/// authorizes a param flip, plus the trials ledger the flip look is appended to.
/// Populated from `LS_TURN_CANDIDATE` at the CLI seam; a param flip without one
/// refuses ([`GateExit::UngovernedFlip`]).
#[derive(Debug, Clone)]
pub struct GovernedFlip {
    /// The candidate directory (holds `candidate.json` + `gate-verdict.json`).
    pub candidate_dir: PathBuf,
    /// The trials ledger the flip look is recorded to (and the GO reading is
    /// looked up in).
    pub ledger: TrialsLedger,
}

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
    /// U2/KTD7: a code-turn native version bump. When true (and `override_param`
    /// is `None`), the resolved current params re-run at `version + 1` with a
    /// zero param diff — the native path that subsumes the manual
    /// seed-manifest-and-rerun workaround. Companion-field seeding is automatic:
    /// the resolved params already carry any newer `#[serde(default)]` field at
    /// its default (applied at manifest-read time), and the JSON round-trip
    /// re-serializes them into the bumped manifest.
    pub code_bump: bool,
    /// U6/KTD1: the governed-flip binding. A param flip (`override_param` set)
    /// refuses without one (the guard is default-on, not opt-in). `None` for a
    /// rerun or code-bump (which the guard does not bind).
    pub candidate: Option<GovernedFlip>,
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
            code_bump: false,
            candidate: None,
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
    /// U6: the typed gate exit for a flip-guard refusal, mapped to a distinct
    /// process exit code. `None` for a run, a rerun, or a pipeline denial (which
    /// exits the generic non-zero).
    pub gate_exit: Option<GateExit>,
    /// Human-facing result lines.
    pub lines: Vec<String>,
}

impl TurnOutcome {
    fn refused(reason: String, mut lines: Vec<String>, approved: Option<bool>) -> Self {
        lines.push(format!("turn ran no backtest: {reason}"));
        TurnOutcome {
            ran: false,
            run_id: None,
            version: None,
            approved,
            refusal: Some(reason),
            gate_exit: None,
            lines,
        }
    }

    /// A flip-guard refusal carrying a typed gate exit (U6). Runs nothing.
    fn refused_gate(reason: String, exit: GateExit, mut lines: Vec<String>) -> Self {
        lines.push(format!("flip refused [{exit:?}]: {reason}"));
        TurnOutcome {
            ran: false,
            run_id: None,
            version: None,
            approved: None,
            refusal: Some(reason),
            gate_exit: Some(exit),
            lines,
        }
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

    // --- Code-turn native bump (U2/KTD7): version+1, zero param diff, no
    // governance cycle. Subsumes the manual seed-manifest-and-rerun workaround.
    // The compiled-in strategy source (a changed orb.rs) moves strategy_code_hash;
    // the version label bumps so the run is `runs compare` Code-mode comparable
    // against the prior head. ---
    if cfg.code_bump {
        if cfg.override_param.is_some() {
            anyhow::bail!(
                "LS_TURN_CODE_BUMP is a version-only bump — it cannot be combined with LS_TURN_PARAM \
                 (a governed param turn); run one or the other"
            );
        }
        let new_version = current_version + 1;
        // Zero overrides: the round-trip re-serializes the resolved current params
        // (companion-field seeding — any newer #[serde(default)] param the prior
        // head predates is already at its default in current_params) and bumps
        // only the version.
        let new_params = apply_overrides(&current_params, &BTreeMap::new(), new_version)?;
        // Defence-in-depth: the manifest param delta must be exactly
        // strategy_version — nothing else may have moved.
        let diff = param_diff(&current_params, &new_params);
        if diff != ["strategy_version".to_string()] {
            anyhow::bail!(
                "code bump changed {diff:?}, expected only strategy_version — refusing before backtest"
            );
        }
        lines.push(format!(
            "code turn: strategy v{current_version} -> v{new_version}, params unchanged \
             (new strategy_code_hash rides the compiled strategy source)"
        ));
        let outcome = run_one_backtest(&cfg, new_params, range).await?;
        lines.push(format!("finalized run {}", outcome.run_id));
        return Ok(TurnOutcome {
            ran: true,
            run_id: Some(outcome.run_id),
            version: Some(new_version),
            approved: None,
            refusal: None,
            gate_exit: None,
            lines,
        });
    }

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
            gate_exit: None,
            lines,
        });
    };

    // --- Governed param turn. ---
    let target = cfg
        .override_target
        .ok_or_else(|| anyhow::anyhow!("override target is required for a param turn"))?;

    // U6/KTD1: the flip guard — a pre-flight bail beside the expect-version
    // assertion. A param flip is structurally impossible without a matching,
    // unedited GO verdict for the exact candidate being flipped (R4, R5). Runs
    // before the pipeline, bounds cap, and seed assertion; refusals exit through
    // the typed gate-exit registry.
    if let Some((reason, exit)) = flip_guard(&cfg, &param, target, prior.as_ref())? {
        return Ok(TurnOutcome::refused_gate(reason, exit, lines));
    }

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

    // U6: the flip look is a trial. The turn (running in the fresh child under the
    // governed orchestrator) is the single writer for the flip look, appended
    // after the backtest finalizes.
    if let Some(flip) = &cfg.candidate {
        append_flip_trial(flip, prior.as_ref(), new_version, &mut lines)?;
    }

    Ok(TurnOutcome {
        ran: true,
        run_id: Some(outcome.run_id),
        version: Some(new_version),
        approved: Some(true),
        refusal: None,
        gate_exit: None,
        lines,
    })
}

/// The Phase-B flip guard (U6, R4/R5; KTD1). Returns `Some((reason, exit))` to
/// refuse, `None` to proceed. Refuses a param flip that lacks a candidate, whose
/// candidate has no GO verdict (or a STOP), whose pre-register was edited after
/// its GO, whose flip target does not match the candidate, whose GO ran against a
/// different sample than the anchor run, or whose GO has no matching gate-reading
/// ledger record.
fn flip_guard(
    cfg: &TurnConfig,
    param: &str,
    target: f64,
    prior: Option<&(String, Manifest)>,
) -> anyhow::Result<Option<(String, GateExit)>> {
    let Some(flip) = &cfg.candidate else {
        return Ok(Some((
            format!(
                "param flip '{param}' -> {target:.4} without a candidate (LS_TURN_CANDIDATE) — a \
                 governed flip requires a candidate with a recorded GO (R4)"
            ),
            GateExit::UngovernedFlip,
        )));
    };
    let loaded = crate::candidates::load(&flip.candidate_dir)?;
    let slug = loaded.values.slug.clone();

    let Some(verdict) = read_gate_verdict(&flip.candidate_dir)? else {
        return Ok(Some((
            format!("candidate '{slug}' has no gate verdict — run `turn diagnose` first (R4)"),
            GateExit::NoGoVerdict,
        )));
    };
    if verdict.decision != "GO" {
        return Ok(Some((
            format!("candidate '{slug}' gate verdict is {} (not GO) — the flip refuses", verdict.decision),
            GateExit::NoGoVerdict,
        )));
    }
    // R5 / AE1: an edited pre-register no longer matches its verdict's recorded hash.
    if verdict.pre_register_hash != loaded.content_hash {
        return Ok(Some((
            format!(
                "candidate '{slug}' pre-register was edited after its GO (current hash {} != verdict {}) \
                 — the flip refuses (R5)",
                loaded.content_hash, verdict.pre_register_hash
            ),
            GateExit::PreRegisterHashMismatch,
        )));
    }
    if !loaded.values.flip_matches(param, target) {
        return Ok(Some((
            format!("flip '{param}' -> {target:.4} does not match candidate '{slug}' declaration"),
            GateExit::FlipMismatch,
        )));
    }
    // The GO must have run against the same sample the flip anchors on.
    let anchor_fp = prior.map(|(_, m)| m.catalog_fingerprint.clone()).unwrap_or_default();
    if verdict.catalog_fingerprint != anchor_fp {
        return Ok(Some((
            format!(
                "candidate '{slug}' GO ran against sample {} but the anchor run's sample is {} — \
                 fingerprint drift, the flip refuses",
                verdict.catalog_fingerprint, anchor_fp
            ),
            GateExit::FingerprintDrift,
        )));
    }
    // A GO with no matching gate-reading ledger record is a re-registration that
    // never actually diagnosed — refuse (R5).
    let has_record = flip.ledger.read_all()?.iter().any(|r| {
        r.candidate == slug
            && matches!(r.look, LookKind::GateReading)
            && r.verdict.starts_with("GO")
            && r.lineage.catalog_fingerprint == anchor_fp
    });
    if !has_record {
        return Ok(Some((
            format!("candidate '{slug}' GO has no matching gate-reading ledger record — the flip refuses (R5)"),
            GateExit::MissingLedgerRecord,
        )));
    }
    Ok(None)
}

/// Append the flip look's trial record (U6). Called after a guarded flip's
/// backtest finalizes.
fn append_flip_trial(
    flip: &GovernedFlip,
    prior: Option<&(String, Manifest)>,
    new_version: u32,
    lines: &mut Vec<String>,
) -> anyhow::Result<()> {
    let loaded = crate::candidates::load(&flip.candidate_dir)?;
    let anchor_fp = prior.map(|(_, m)| m.catalog_fingerprint.clone()).unwrap_or_default();
    let trial = TrialRecord::new(
        loaded.values.slug.clone(),
        loaded.values.family.clone(),
        LookKind::Flip,
        SampleLineage { catalog_fingerprint: anchor_fp, parent_fingerprint: None },
        BTreeMap::new(),
        format!("flip approved v{new_version}"),
        Utc::now().to_rfc3339(),
    );
    flip.ledger.append(&trial)?;
    lines.push(format!("appended flip trial for candidate '{}'", loaded.values.slug));
    Ok(())
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
    /// The code-turn verdict (U2/KTD7): a version-only param diff with an
    /// **expected** code-hash delta reported (not a failure), every other
    /// identity field (fingerprint / range / universe / metadata) still
    /// hard-checked. This is the mode that PASSes a native code-turn re-baseline,
    /// retiring the "no `runs compare` mode PASSes a code turn" workaround where
    /// the operator had to read a param-mode FAIL as the re-baseline evidence.
    Code,
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

    // A cross-strategy pair is refused in EVERY mode, before any comparison runs (R24).
    // The incidental guards would catch most such pairs — the code hashes differ, and the
    // param diff is wide — but not reliably: `Manifest.params` is a non-optional
    // `OrbParams`, so a daily run asserts a full ORB parameter set it never ran under, and
    // a daily run whose recorded assembly params happen to match an ORB run's would produce
    // an *empty* param diff and read as a clean reproduction of it. Refusing on the
    // discriminator is the only check that does not depend on the values lining up.
    if a.strategy_id != b.strategy_id {
        anyhow::bail!(
            "refusing to compare runs of different strategies: {a_id} is {:?} and {b_id} is \
             {:?}. `Manifest.params` cannot express \"this run has no OrbParams\", so a daily \
             run carries a fictitious ORB parameter set — a param diff across the two \
             compares values neither run was selected under (KTD14, R24)",
            a.strategy_id,
            b.strategy_id
        );
    }

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
        CompareMode::Code => {
            // A code turn bumps only the version and moves the strategy_code_hash;
            // every other identity field must hold, so the delta is attributable
            // to the code change alone (the v27/v29 re-baseline shape, KTD7).
            let mut ok = true;
            if diff != ["strategy_version".to_string()] {
                lines.push(format!(
                    "FAIL: code turn requires a version-only param diff, got {diff:?}"
                ));
                ok = false;
            } else {
                lines.push("version-only delta: strategy_version (code turn)".to_string());
            }
            // The code-hash delta is the whole point — its ABSENCE is the failure.
            if code_equal {
                lines.push(
                    "FAIL: strategy_code_hash is unchanged — a code turn must move it (use param/data mode)"
                        .to_string(),
                );
                ok = false;
            } else {
                lines.push("strategy_code_hash delta: expected (code-turn re-baseline)".to_string());
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
                lines.push("FAIL: universe_hash differs".to_string());
                ok = false;
            }
            if !metadata_equal {
                lines.push("FAIL: universe_metadata_hash differs".to_string());
                ok = false;
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

/// The ingest→backtest go/no-go behind the calendar seam (R6; AE5; KTD6). Enforced-only after
/// the catalog Consumer Retirement Gate (#189 U7): the watermark and expected-range boundary
/// checks resolve against PROVEN first/last Trading Sessions from the injected calendar `gate`
/// — a real holiday closure no longer false-flags, a boundary the calendar cannot prove is
/// `NO-GO — calendar indeterminate`, an out-of-coverage boundary is `NO-GO — calendar
/// unavailable`, and a stale-but-established boundary is a GO with a prominent warning. There is
/// no weekday fallback; the un-gated `catalog_status` wrapper and `last_weekday_on_or_before`
/// walk-back are retired.
pub async fn catalog_status_gated(
    cfg: &StatusConfig,
    gate: CatalogCalendarGate<'_>,
) -> anyhow::Result<StatusOutcome> {
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
    // Distinct Enforced-only messaging for the boundary-relevant Unknown / unavailable
    // cases (U11). Collected separately so the per-triple facts stay stable and these
    // NO-GO lines render just before the final status verdict.
    let mut calendar_notes: Vec<String> = Vec::new();
    // The civil boundary dates the Enforced GO evaluation actually keyed on (watermark +
    // expected-range endpoints). The dimension-relevant stale warning (U11, KTD5) names only
    // the freshness dimensions that bound THESE dates — not a blanket `any_stale()`.
    let mut enforced_boundaries: Vec<NaiveDate> = Vec::new();
    for ((instrument, bar_kind), group) in groups {
        let first = group.iter().map(|b| kst_date_of(b.ts_event)).min().expect("non-empty group");
        let last = group.iter().map(|b| kst_date_of(b.ts_event)).max().expect("non-empty group");
        let mut flags = Vec::new();

        // Tail check vs the checkpoint watermark (always). A span ending before
        // the covered watermark undershoots the completed range (AE5). Under Legacy/Shadow
        // the watermark is compared at its last *weekday* (accumulate advances the watermark
        // onto weekends with no session, so a healthy Friday-closed catalog must not
        // false-flag when the last ingest ran over a weekend). Under Enforced the boundary is
        // the PROVEN last Trading Session on or before the watermark — a real holiday closure
        // no longer false-flags, and an Unknown/unavailable boundary fails closed.
        if let Some(wm) = checkpoint.watermark(&instrument, &bar_kind) {
            enforced_boundaries.push(wm);
            match gate.last_session_on_or_before(wm) {
                SessionBoundary::Session(sess) => {
                    if last < sess {
                        flags.push(format!(
                            "tail undershoot: last {last} < last session {sess} (watermark {wm})"
                        ));
                    }
                }
                SessionBoundary::NoSession => {}
                SessionBoundary::Indeterminate => {
                    go = false;
                    calendar_notes.push(format!(
                        "NO-GO — calendar indeterminate: {instrument} {bar_kind} last session at/before watermark {wm} cannot be proven (Unknown at the boundary)"
                    ));
                }
                SessionBoundary::Unavailable => {
                    go = false;
                    calendar_notes.push(format!(
                        "NO-GO — calendar unavailable: {instrument} {bar_kind} watermark {wm} is outside calendar coverage"
                    ));
                }
            }
        }
        // Both-direction checks vs an operator-supplied expected range.
        if let Some(exp) = &cfg.expected_range {
            if let Ok(exp_start) = NaiveDate::parse_from_str(exp.start.trim(), "%Y%m%d") {
                enforced_boundaries.push(exp_start);
                match gate.first_session_on_or_after(exp_start) {
                    SessionBoundary::Session(sess) => {
                        if first > sess {
                            flags.push(format!(
                                "front truncation: first {first} > first session {sess} (expected start {exp_start})"
                            ));
                        }
                    }
                    SessionBoundary::NoSession => {}
                    SessionBoundary::Indeterminate => {
                        go = false;
                        calendar_notes.push(format!(
                            "NO-GO — calendar indeterminate: {instrument} {bar_kind} first session at/after expected start {exp_start} cannot be proven (Unknown at the boundary)"
                        ));
                    }
                    SessionBoundary::Unavailable => {
                        go = false;
                        calendar_notes.push(format!(
                            "NO-GO — calendar unavailable: {instrument} {bar_kind} expected start {exp_start} is outside calendar coverage"
                        ));
                    }
                }
            }
            if let Ok(exp_end) = NaiveDate::parse_from_str(exp.end.trim(), "%Y%m%d") {
                enforced_boundaries.push(exp_end);
                match gate.last_session_on_or_before(exp_end) {
                    SessionBoundary::Session(sess) => {
                        if last < sess {
                            flags.push(format!(
                                "tail undershoot: last {last} < last session {sess} (expected end {exp_end})"
                            ));
                        }
                    }
                    SessionBoundary::NoSession => {}
                    SessionBoundary::Indeterminate => {
                        go = false;
                        calendar_notes.push(format!(
                            "NO-GO — calendar indeterminate: {instrument} {bar_kind} last session at/before expected end {exp_end} cannot be proven (Unknown at the boundary)"
                        ));
                    }
                    SessionBoundary::Unavailable => {
                        go = false;
                        calendar_notes.push(format!(
                            "NO-GO — calendar unavailable: {instrument} {bar_kind} expected end {exp_end} is outside calendar coverage"
                        ));
                    }
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
    // Stale-but-established (Enforced): the boundary facts are proven (no calendar note,
    // still a GO) but the calendar's freshness is stale at the as-of instant — surface a
    // PROMINENT warning without flipping the verdict. The warning names ONLY the freshness
    // dimension(s) that actually bound the queried boundary dates (U11, KTD5): a snapshot
    // stale only in an unrelated dimension raises no spurious catalog warning. Where there is
    // no boundary date to key on, fall back to the snapshot-global `any_stale()`.
    if go {
        let mut stale_dims: BTreeSet<&'static str> = BTreeSet::new();
        for boundary in &enforced_boundaries {
            for dim in gate.stale_bounding_dimensions(*boundary) {
                stale_dims.insert(dim);
            }
        }
        if !stale_dims.is_empty() {
            lines.push(format!(
                "WARNING: calendar evidence is STALE in the bounding dimension(s) [{}] — \
                 boundary facts established, proceeding (GO)",
                stale_dims.into_iter().collect::<Vec<_>>().join(", ")
            ));
        } else if enforced_boundaries.is_empty() && gate.is_stale() {
            // No boundary date cleanly bounds a dimension (no watermark, no expected range) —
            // fall back to the snapshot-global staleness signal (KTD5).
            lines.push(
                "WARNING: calendar evidence is STALE — boundary facts established, proceeding (GO)"
                    .to_string(),
            );
        }
    }
    // The Enforced-only NO-GO calendar lines render just before the verdict.
    lines.extend(calendar_notes);
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

    // Size-invariant edge metrics (CLASS B, R1/R2/R3): return-on-risk is the KEEP
    // crux under variable sizing, mean-R the size-invariant diagnostic invariant, and
    // risk-capital share the decisional dominance gate. `n/a` on a legacy / pre-risk
    // run whose closed trades carry no `risk_capital` (the verdict then reads the
    // P&L-share dominance above). RoR/expectancy share sign, so a positive RoR is the
    // size-honest restatement of a positive expectancy (KRW/trade is size-contaminated
    // once the sizing lever is on).
    let ror_display = fmt_opt(edge.return_on_risk);
    let mean_r_display = fmt_opt(edge.mean_realized_r);
    let risk_dom_display = if edge.degenerate_zero_risk {
        "undefined (zero deployed risk)".to_string()
    } else {
        match edge.max_risk_capital_share {
            Some(s) => format!("{:.1}%", s * 100.0),
            None => "n/a (no risk info — verdict on P&L share)".to_string(),
        }
    };
    let risk_dom_pf = match edge.risk_dominance_pass {
        Some(true) => "PASS",
        Some(false) => "FAIL",
        None => "n/a",
    };

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
         - **Expectancy (KRW/trade, size-contaminated once sizing is on — diagnostic):** `{expectancy}`\n\
         - **Total realized P&L (KRW):** `{pnl_total:.4}`\n\
         - **Closed trades:** `{num_trades:.0}`\n\
         - **(c) single-symbol dominance (≤ {dom_cap:.0}% of aggregate |P&L|) — diagnostic:** `{dom_share}` → **{c_pf}**\n\
         \n\
         ### Size-invariant edge (CLASS B, R1/R2/R3)\n\
         \n\
         Once a position-sizing lever varies per-trade notional, KRW/trade expectancy stops\n\
         being a size-invariant edge measure. **Return-on-risk** = `Σrealized_pnl /\n\
         Σrisk_capital` is the KEEP crux (R6): flat under a uniform size-up, responsive to\n\
         risk reallocation. **Risk-capital share** is the decisional dominance gate (R3).\n\
         `n/a` means this run carried no per-trade `risk_capital` (a legacy / pre-CLASS-B\n\
         run — the P&L-share dominance above is then the gate).\n\
         \n\
         - **Return-on-risk (Σpnl / Σrisk_capital) — KEEP crux:** `{ror}`\n\
         - **Equal-weight mean-R (size-invariant diagnostic invariant):** `{mean_r}`\n\
         - **Total deployed risk_capital (KRW):** `{risk_total}`\n\
         - **Risk-capital dominance (≤ {dom_cap:.0}% of Σrisk_capital) — decisional:** `{risk_dom_share}` → **{risk_dom_pf}**\n\
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
        ror = ror_display,
        mean_r = mean_r_display,
        risk_total = fmt_opt(edge.risk_capital_total),
        risk_dom_share = risk_dom_display,
        risk_dom_pf = risk_dom_pf,
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
const USAGE: &str = "usage: lab-research <turn | turn diagnose | turn governed | runs compare | replay | catalog status | catalog compact | analyze --scaffold | report mfe | report tiers | report sample | report paired | fingerprint | trials count | trials record>";

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
    // Mandatory startup calendar record (U8): one redacted line to the non-persisted
    // diagnostic channel (stderr). adoption = Enforced (the sole posture, #189); a missing
    // snapshot fails closed (KTD8).
    //
    // The `catalog status` path (U1) emits its OWN decision-relevant startup record from a
    // single shared load inside its CLI branch — so it is skipped here to keep exactly one
    // load and one startup record per invocation. Every other subcommand emits the generic
    // record here.
    if !is_catalog_status_invocation() {
        nautilus_ls::calendar::emit_startup_from_env("lab-research");
    }
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

/// Whether this invocation is `catalog status` — which owns its decision-relevant startup
/// record from a single shared load (U1), so `main_cli` must not also emit the generic one.
fn is_catalog_status_invocation() -> bool {
    let mut args = std::env::args().skip(1);
    args.next().as_deref() == Some("catalog") && args.next().as_deref() == Some("status")
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
        Some("turn") => match std::env::args().nth(2).as_deref() {
            // U5: the Phase-A diagnose stage as a standalone subcommand.
            Some("diagnose") => {
                let out = run_diagnose_cli()?;
                print_lines(&out.lines);
                Ok(out.exit.exit_code())
            }
            // U7: the one-shot governed orchestrator (parent).
            Some("governed") => {
                let out = crate::runner::governed::run_governed_cli()?;
                print_lines(&out.lines);
                Ok(out.exit.exit_code())
            }
            _ => {
                // U7: the fresh child running the flip stage (the decider).
                if std::env::var("LS_GOVERNED_CHILD").map(|v| v == "1").unwrap_or(false) {
                    return crate::runner::governed::run_governed_child_cli();
                }
                let cfg = turn_config_from_env()?;
                let rt = tokio::runtime::Runtime::new()?;
                let out = rt.block_on(turn(cfg))?;
                print_lines(&out.lines);
                // A flip-guard refusal exits through its typed gate code (U6); a
                // pipeline denial is the generic non-zero; a run succeeds.
                match out.gate_exit {
                    Some(exit) => Ok(exit.exit_code()),
                    None => Ok(ok_fail(out.ran)),
                }
            }
        },
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
                // Composition root (KTD5/KTD8, U1): resolve the EXPLICIT snapshot path and load
                // ONCE, then emit the mandatory startup record BEFORE the fallible config parse /
                // runtime build — a malformed LS_STATUS_* or LS_DATA_HOME must NOT drop the
                // always-emit startup invariant. The single loaded calendar is shared with the
                // catalog gate below. Enforced-only after the catalog Consumer Retirement Gate
                // (#189 U7, KTD3): the date decision no longer consults LS_CALENDAR_ADOPTION —
                // the catalog resolves boundaries against proven Trading Sessions and fails
                // closed when the calendar is unavailable.
                let as_of = Utc::now();
                let adoption = CalendarAdoption::Enforced;
                let path = nautilus_ls::calendar::snapshot_path_from_env();
                let loaded = nautilus_ls::calendar::resolve_and_load(path.as_deref(), as_of, adoption);
                // The decision-relevant startup target (KTD2): catalog has no single per-triple
                // decision date at the startup emit point, so it reports posture plus a defined
                // representative target — the operator-supplied expected-range END when present,
                // else the coverage watermark `materialized_through`. Read the endpoint straight
                // from env so the emit never depends on the fallible full-config parse below.
                let target = std::env::var("LS_STATUS_EDATE")
                    .ok()
                    .and_then(|s| NaiveDate::parse_from_str(s.trim(), "%Y%m%d").ok())
                    .or_else(|| loaded.calendar().map(|cal| cal.coverage().materialized_through));
                let record = nautilus_ls::calendar::build_startup_record_targeted(
                    "lab-research",
                    adoption,
                    &loaded,
                    as_of,
                    target,
                );
                nautilus_ls::calendar::emit_startup_record(&record);

                // The fallible config parse + go/no-go run AFTER the record is already emitted.
                let rt = tokio::runtime::Runtime::new()?;
                let cfg = status_config_from_env()?;
                let view = loaded.calendar().and_then(|cal| cal.as_of(as_of).ok());
                let gate = CatalogCalendarGate::new(view);
                let out = rt.block_on(catalog_status_gated(&cfg, gate))?;
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
            Some("sample") => {
                let rt = tokio::runtime::Runtime::new()?;
                let out = rt.block_on(crate::runner::report::report_sample(&sample_config_from_env()?))?;
                print_lines(&out.lines);
                // An insufficient sample and a refused margin are both valid
                // completions (R9 — a stand-down is a verdict, not a failure);
                // the exit code reflects I/O and input integrity only.
                Ok(ExitCode::SUCCESS)
            }
            Some("paired") => {
                let out = crate::runner::report::report_paired(&paired_config_from_env()?)?;
                print_lines(&out.lines);
                // Attributable, unattributable, or a mixture are all valid
                // completions (R9 — a stand-down is a verdict, not a failure);
                // the exit code reflects I/O and input integrity only.
                Ok(ExitCode::SUCCESS)
            }
            other => anyhow::bail!("unknown `report` subcommand {other:?} — want `report mfe` | `report tiers` | `report sample` | `report paired`\n{USAGE}"),
        },
        Some("trials") => match std::env::args().nth(2).as_deref() {
            Some("count") => {
                let out = crate::trials::count_trials(&trials_ledger_from_env())?;
                print_lines(&out.lines);
                Ok(ExitCode::SUCCESS)
            }
            Some("record") => {
                // Utc::now at the CLI seam; the library takes the stamp as a
                // parameter so tests stay deterministic.
                let out = crate::trials::record_from_env(
                    &trials_ledger_from_env(),
                    Utc::now().to_rfc3339(),
                )?;
                print_lines(&out.lines);
                Ok(ExitCode::SUCCESS)
            }
            other => anyhow::bail!(
                "unknown `trials` subcommand {other:?} — want `trials count` | `trials record`\n{USAGE}"
            ),
        },
        Some("fingerprint") => {
            // U1/KTD5: print the binary's embedded lab-source fingerprint. A
            // structured line (not free-text), so it renders verbatim; the
            // orchestrator (U7) parses `fingerprint: <hex>` from a freshly built
            // binary and requires it to match the recomputed tree hash.
            print_lines(&[format!("fingerprint: {}", crate::fingerprint::EMBEDDED)]);
            Ok(ExitCode::SUCCESS)
        }
        other => anyhow::bail!("unknown subcommand {other:?}\n{USAGE}"),
    }
}

pub(crate) fn turn_config_from_env() -> anyhow::Result<TurnConfig> {
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
    // U2/KTD7: a code-turn native version bump (any non-empty value enables it).
    cfg.code_bump = std::env::var("LS_TURN_CODE_BUMP")
        .map(|v| !v.trim().is_empty() && v.trim() != "0")
        .unwrap_or(false);
    if cfg.code_bump && cfg.override_param.is_some() {
        anyhow::bail!("LS_TURN_CODE_BUMP cannot be combined with LS_TURN_PARAM");
    }
    // U6: the governed-flip binding from LS_TURN_CANDIDATE (the guard refuses a
    // param flip without it).
    cfg.candidate = std::env::var("LS_TURN_CANDIDATE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|slug| GovernedFlip {
            candidate_dir: candidates_home().join(&slug),
            ledger: trials_ledger_from_env(),
        });
    Ok(cfg)
}

fn compare_config_from_env() -> anyhow::Result<CompareConfig> {
    let mode = match std::env::var("LS_COMPARE_MODE").as_deref() {
        Ok("data") => CompareMode::Data,
        Ok("code") => CompareMode::Code,
        Ok("param") | Err(_) => CompareMode::Param,
        Ok(other) => anyhow::bail!("LS_COMPARE_MODE must be param | data | code, got {other:?}"),
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

/// A numeric override that is present but unparseable is a loud refusal:
/// silently falling back to the default would report a seed the run did not
/// actually use, and the whole point of the seed is re-derivability.
fn env_parsed<T: std::str::FromStr>(var: &str, default: T) -> anyhow::Result<T>
where
    T::Err: std::fmt::Display,
{
    match std::env::var(var).ok().filter(|s| !s.trim().is_empty()) {
        None => Ok(default),
        Some(s) => s.trim().parse::<T>().map_err(|e| anyhow::anyhow!("{var}={s:?}: {e}")),
    }
}

/// `report paired` config. Two data homes: the head lives under `turn4-fresh`
/// and the cost-aware off-flip arms under `turn4-cost-scratch`, so
/// `sample_config_from_env`'s single home cannot be copied.
fn paired_config_from_env() -> anyhow::Result<crate::runner::report::PairedConfig> {
    let head_home = data_home_from_env()?;
    // The head run is REQUIRED. `report sample` may default to the latest
    // finalized run because it names what it did in the header; here that
    // default would silently pair against whatever ran last under the head home
    // — which under `turn4-fresh` is not v35.
    let head_run = std::env::var("LS_REPORT_RUN")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "LS_REPORT_RUN is required for `report paired` — it names the HEAD run. Unlike \
                 `report sample` this verb never defaults to the latest finalized run: pairing \
                 against an unintended head would report a difference nobody asked for"
            )
        })?;
    let arm_runs: Vec<String> = std::env::var("LS_PAIRED_ARMS")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "LS_PAIRED_ARMS is required for `report paired` — a comma-separated list of the \
                 off-flip arm run ids to pair the head against"
            )
        })?;
    Ok(crate::runner::report::PairedConfig {
        // Absent → the arms share the head's home. Stated in the header either
        // way, so a single-home run is never ambiguous.
        arm_home: std::env::var("LS_PAIRED_ARM_HOME")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map_or_else(|| head_home.clone(), PathBuf::from),
        head_home,
        head_run,
        arm_runs,
        margin_path: std::env::var("LS_SAMPLE_MARGIN")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from),
        replicates: env_parsed("LS_SAMPLE_REPLICATES", crate::runner::report::SAMPLE_REPLICATES)?,
        seed: env_parsed("LS_SAMPLE_SEED", crate::runner::report::SAMPLE_SEED)?,
    })
}

fn sample_config_from_env() -> anyhow::Result<crate::runner::report::SampleConfig> {
    Ok(crate::runner::report::SampleConfig {
        data_home: data_home_from_env()?,
        // Absent → the latest finalized run, marked as defaulted in the header.
        // Safe only because this report writes nothing.
        run_id: std::env::var("LS_REPORT_RUN").ok().filter(|s| !s.trim().is_empty()),
        margin_path: std::env::var("LS_SAMPLE_MARGIN")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from),
        replicates: env_parsed("LS_SAMPLE_REPLICATES", crate::runner::report::SAMPLE_REPLICATES)?,
        seed: env_parsed("LS_SAMPLE_SEED", crate::runner::report::SAMPLE_SEED)?,
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

/// Resolve the git-tracked candidates home (U4/KTD2). `LS_CANDIDATES_HOME`
/// overrides (tests point it at a fixture); otherwise the fixed
/// `<crate>/candidates` dir baked from `CARGO_MANIFEST_DIR`.
pub(crate) fn candidates_home() -> PathBuf {
    std::env::var("LS_CANDIDATES_HOME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("candidates"))
}

/// Run the `turn diagnose` stage from env (U5). Resolves the candidate by slug,
/// runs the freeze check (a git-dirty frozen input is the `FrozenInputDirty` gate),
/// resolves the anchor run's catalog fingerprint, and diagnoses.
pub(crate) fn run_diagnose_cli() -> anyhow::Result<crate::runner::diagnose::DiagnoseOutcome> {
    use crate::runner::diagnose::{diagnose, DiagnoseConfig, DiagnoseOutcome, GateExit};

    let data_home = data_home_from_env()?;
    let slug = std::env::var("LS_TURN_CANDIDATE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("LS_TURN_CANDIDATE (the candidate slug) is required for `turn diagnose`"))?;
    let candidate_dir = candidates_home().join(&slug);

    // Freeze check (KTD2): refuse a git-dirty frozen input, capture the commit.
    let loaded = crate::candidates::load(&candidate_dir)?;
    let freeze = match crate::candidates::freeze_check(&loaded) {
        Ok(f) => f,
        Err(e) => {
            return Ok(DiagnoseOutcome {
                go: false,
                exit: GateExit::FrozenInputDirty,
                gate_verdict_path: None,
                lines: vec![
                    nautilus_ls::scrub::scrub_secrets(&e.to_string()),
                    "STOP frozen-input-dirty".to_string(),
                ],
            });
        }
    };

    let anchor = latest_finalized_run(&data_home)?
        .ok_or_else(|| {
            anyhow::anyhow!("no finalized anchor run in {} — run a baseline first", data_home.display())
        })?
        .1
        .catalog_fingerprint;

    let cfg = DiagnoseConfig {
        candidate_dir,
        ledger: trials_ledger_from_env(),
        anchor_fingerprint: anchor,
        parent_fingerprint: std::env::var("LS_DIAGNOSE_PARENT_FP").ok().filter(|s| !s.trim().is_empty()),
        freeze_commit: freeze.commit,
        recorded_utc: Utc::now().to_rfc3339(),
    };
    diagnose(&cfg)
}

/// Resolve the TRIALS ledger (U3/KTD2). `LS_TRIALS_LEDGER` overrides (bin-level
/// tests point it at a tempdir); otherwise the fixed tracked home under the lab
/// crate root (`<crate>/ledger/trials.jsonl`), baked from `CARGO_MANIFEST_DIR` so
/// the path is stable regardless of the invoking cwd.
pub(crate) fn trials_ledger_from_env() -> crate::trials::TrialsLedger {
    let path = std::env::var("LS_TRIALS_LEDGER")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join(crate::trials::LEDGER_RELPATH)
        });
    crate::trials::TrialsLedger::new(path)
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
