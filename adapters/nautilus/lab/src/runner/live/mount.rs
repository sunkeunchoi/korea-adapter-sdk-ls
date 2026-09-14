//! The LADDER lane: the dispatch pre-flight gate, the mount authorization that consumes a
//! green dispatch, the ORB node build, `--mount` itself, and the ladder/diagnostic CLI arms
//! (U9's split of `live.rs`).
//!
//! Everything here is dispatch-chain-bearing. The rehearsal lane
//! ([`super::rehearsal`]) reaches the same driver with no chain at all, which is why the
//! machinery both share lives in [`super::shared`] rather than here.
//!
//! Split out verbatim from the pre-U9 `live.rs`; the section comments below are the
//! original ones and still name the units that wrote them.

use super::shared::*;

// The pre-split file carried one flat import set; these are the entries the ladder half
// still needs (the driver half's live on `shared`).
use std::path::{Path, PathBuf};
use std::time::Duration;

use ls_sdk::LsSdk;
use nautilus_ls::ingest::checkpoint::Checkpoint;
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_ls::orders::ledger::FillLedger;

use crate::params::OrbParams;
use crate::runner::authority::mount_verdict;
use crate::runner::pnl::MarkPolicy;
use crate::runner::watchdog::WatchdogLimits;
use crate::strategy::hooks::MarkFeed;

// ===========================================================================
// U3 — the `lab-live --dispatch` pre-flight gate (R1–R4).
//
// A standalone, offline-runnable pre-check ahead of the manual operator recipe: it
// gathers a DispatchContext, runs the tiered checks, records the attempt in the
// dispatch chain, and reports. Everything a machine can check before a session runs is
// checked here; the LiveNode mount lands in U6 behind a green dispatch.
//
// Offline-first: the environmental probes normally read from live state are gathered
// through override seams so the whole gate is fixture-driven and testable without a
// gateway (the documented stubbed-binary pattern) — the gateway probes via
// `LS_DISPATCH_STUB_PROBES`, catalog freshness via `LS_DISPATCH_STUB_CATALOG`, the
// clock via `LS_DISPATCH_NOW_UNIX`. When no gateway stub is set the gate builds the
// resolved-lane paper client and does the real t0424/t0425 reads (the operator path;
// U6 threads this same client through the mounted session).
// ===========================================================================

use std::process::ExitCode;

use chrono::{TimeZone, Utc};

use nautilus_ls::lock::is_held;

use crate::dispatch::chain::{
    kst_trading_date, ChainStatus, DispatchChain, DispatchOutcome, RecordKind,
    SafetyTripKind, SessionDispatch,
};
use crate::dispatch::checks::{
    date_fact_from_view, decide, parse_deferrals, probe_flat_start, probe_stranded_orders,
    run_checks, BudgetHeadroom, CalendarDateFact, DispatchContext, GateResult, GatewayProbe,
    LanePosture, TradingCalendar, WeekdayKrxCalendar,
};
use crate::dispatch::nonce::{detect_unattended_marker, OperatorGate};
use crate::dispatch::ladder::apply_deescalation;
use crate::dispatch::readiness::{compute_readiness, readiness_summary, ReadinessVerdict};
use crate::dispatch::{UnknownOverride, RUNG_MIN};

use nautilus_ls::calendar::StartupRecord;
use nautilus_ls_calendar::CalendarAdoption;

/// The dispatch gate's resolved configuration (env-gathered, but constructible directly
/// so the library tests bypass the process environment).
#[derive(Debug, Clone)]
pub struct DispatchCliConfig {
    /// The data home (chain, catalog, spend ledger, registry live here).
    pub data_home: std::path::PathBuf,
    /// The rung this dispatch requests (guard rail, R15).
    pub requested_rung: u8,
    /// The lane posture (governs rung-auth tiering).
    pub lane: LanePosture,
    /// The lane env-file path (present-check for the interlock).
    pub lane_env_path: std::path::PathBuf,
    /// `LS_TRADING_ENV`.
    pub trading_env: Option<String>,
    /// Named deferral items (`LS_DISPATCH_DEFER`).
    pub deferrals: Vec<String>,
    /// The operator nonce (`LS_DISPATCH_NONCE`).
    pub nonce: Option<String>,
    /// Wall-clock unix seconds (injectable for deterministic tests).
    pub now_unix: i64,
    /// Catalog freshness stub (`ok` | `stale` | `empty`); absent → not evaluated (red).
    pub catalog_stub: Option<String>,
    /// Gateway-probe stub (`flat,stranded`, each `clear` | `blocked` | `throttled`);
    /// absent → real paper reads.
    pub probe_stub: Option<(GatewayProbe, GatewayProbe)>,
    /// Budget stub (`ok` | `low` | `unmeasured`); absent → `ok`.
    pub budget_stub: Option<String>,
    /// The per-session budget plan-ahead need (calls).
    pub budget_plan: i64,
    /// Library-only override of the attended/unattended detection: `Some(true)` forces
    /// attended, `Some(false)` forces unattended, `None` detects (CI env / TTY). The
    /// bin's env gather always leaves this `None` — it is not reachable from the
    /// environment, so the no-TTY refusal a real operator/agent shell sees can never be
    /// suppressed from the CLI; it exists only so a library test can exercise the
    /// applied-deferral path (which is unreachable in a no-TTY test harness by design).
    pub attended_override: Option<bool>,
    /// Readiness-verdict override (`green` | `red` | `na`) for deterministic gate tests;
    /// absent → compute the verdict from the registry + chain + sidecar (U9). The bin's
    /// env gather leaves this `None` (the real verdict is always computed).
    pub readiness_stub: Option<String>,
    /// The pre-registration values file (`preregistration.json`) the reducer + record
    /// citation load, when present (KTD9). Absent in phase 1.
    pub prereg_path: Option<std::path::PathBuf>,
    /// The per-consumer calendar adoption posture (U12, KTD8). Enforced-only after the #189
    /// weekday retirement: the calendar is the authoritative date fact with no weekday
    /// fallback. Retained as a startup-record field; the offline `date_fact_stub` seam still
    /// wins over env resolution for tests.
    pub adoption: CalendarAdoption,
    /// The current dispatch run identity the attended Unknown override binds to (U12).
    /// Absent → an empty run id (no override can bind).
    pub run_id: Option<String>,
    /// Deterministic-test injection of the tri-state calendar DATE fact (U12) — the
    /// Enforced offline seam (mirrors `catalog_stub`/`readiness_stub`). Absent → resolve
    /// from `adoption` + the env-configured snapshot. The bin's env gather leaves this
    /// `None` (the real fact is always resolved).
    pub date_fact_stub: Option<CalendarDateFact>,
    /// The attended Unknown-date override (U12). Nonce/attendance-gated in [`run_dispatch`]
    /// before it can proceed an Unknown date, and additionally bound to the snapshot
    /// identity actually in force. The bin's env gather loads it from the operator-authored
    /// file named by `LS_DISPATCH_UNKNOWN_OVERRIDE` — an authored artifact carrying a
    /// structured first-party citation, never a blunt env toggle (a bare env var could be
    /// set by reflex; a citation cannot be written by accident).
    pub unknown_override: Option<UnknownOverride>,
}

/// The gate's outcome: the verdict, the report lines, and whether a record was appended.
#[derive(Debug, Clone)]
pub struct DispatchGateOutcome {
    /// The gate verdict.
    pub result: GateResult,
    /// The report lines (verbatim, structured; free text is scrubbed at source).
    pub lines: Vec<String>,
    /// Whether a session-dispatch record was appended to the chain.
    pub appended: bool,
}

fn parse_probe(s: &str) -> GatewayProbe {
    match s.trim() {
        "clear" => GatewayProbe::Clear,
        "throttled" => GatewayProbe::Throttled,
        other => GatewayProbe::Blocked(format!("stub-blocked ({other})")),
    }
}

/// Gather the gate config from the process environment.
pub fn dispatch_gate_config_from_env() -> anyhow::Result<DispatchCliConfig> {
    let data_home = std::env::var("LS_DATA_HOME")
        .map_err(|_| anyhow::anyhow!("LS_DATA_HOME is required"))?
        .into();
    let trading_env = std::env::var("LS_TRADING_ENV").ok().filter(|s| !s.trim().is_empty());
    let lane = match std::env::var("LS_DISPATCH_LANE").as_deref() {
        Ok("live") => LanePosture::Live,
        Ok("paper") => LanePosture::Paper,
        // Default: a paper trading-env is a paper pre-check (rung informational); any
        // other resolved env is treated as a live-lane dispatch.
        _ => {
            if trading_env.as_deref().map(|e| e.eq_ignore_ascii_case("paper")).unwrap_or(false) {
                LanePosture::Paper
            } else {
                LanePosture::Live
            }
        }
    };
    let lane_name = std::env::var("LS_LANE").unwrap_or_else(|_| "domestic".to_string());
    let lane_env_path = std::env::var("LS_DISPATCH_LANE_ENV")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(format!(".env.{lane_name}")));
    let requested_rung = std::env::var("LS_DISPATCH_RUNG")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let now_unix = std::env::var("LS_DISPATCH_NOW_UNIX")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| Utc::now().timestamp());
    let probe_stub = std::env::var("LS_DISPATCH_STUB_PROBES").ok().map(|raw| {
        let mut it = raw.split(',');
        let flat = parse_probe(it.next().unwrap_or("clear"));
        let stranded = parse_probe(it.next().unwrap_or("clear"));
        (flat, stranded)
    });
    Ok(DispatchCliConfig {
        data_home,
        requested_rung,
        lane,
        lane_env_path,
        trading_env,
        deferrals: parse_deferrals(std::env::var("LS_DISPATCH_DEFER").ok().as_deref()),
        nonce: std::env::var("LS_DISPATCH_NONCE").ok().filter(|s| !s.trim().is_empty()),
        now_unix,
        catalog_stub: std::env::var("LS_DISPATCH_STUB_CATALOG").ok().filter(|s| !s.trim().is_empty()),
        probe_stub,
        budget_stub: std::env::var("LS_DISPATCH_STUB_BUDGET").ok().filter(|s| !s.trim().is_empty()),
        budget_plan: std::env::var("LS_DISPATCH_BUDGET_PLAN").ok().and_then(|v| v.parse().ok()).unwrap_or(5),
        // Never sourced from the environment: the no-TTY refusal cannot be suppressed
        // from the CLI.
        attended_override: None,
        // Never stubbed from the environment: the real verdict is always computed.
        readiness_stub: None,
        prereg_path: std::env::var("LS_DISPATCH_PREREG")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(std::path::PathBuf::from),
        // The per-consumer adoption posture (U12, KTD8): Enforced-only (#189).
        adoption: nautilus_ls::calendar::adoption_from_env(),
        run_id: std::env::var("LS_DISPATCH_RUN_ID").ok().filter(|s| !s.trim().is_empty()),
        // Never stubbed from the environment: the real date fact is always resolved.
        date_fact_stub: None,
        unknown_override: unknown_override_from_env()?,
    })
}

/// Load the narrow attended Unknown override from the operator-authored JSON file named by
/// `LS_DISPATCH_UNKNOWN_OVERRIDE` (U12). This is the operator *tool path*, not a blunt env
/// toggle: the file must carry a structured first-party citation, which cannot be written
/// by reflex the way an env var can be exported.
///
/// Fail-closed at every step — an unset variable is the only quiet outcome. A named file
/// that cannot be read, cannot be parsed, or is not well-formed is a hard error rather than
/// a silent `None`, so a typo'd path can never look like "the operator chose not to
/// override" while an Unknown date refuses for a reason the operator never sees.
///
/// # Errors
///
/// If the named file is unreadable, malformed, or missing a required audit field.
fn unknown_override_from_env() -> anyhow::Result<Option<UnknownOverride>> {
    let Some(path) =
        std::env::var("LS_DISPATCH_UNKNOWN_OVERRIDE").ok().filter(|s| !s.trim().is_empty())
    else {
        return Ok(None);
    };
    let bytes = std::fs::read(&path)
        .map_err(|e| anyhow::anyhow!("reading the Unknown-override file {path}: {e}"))?;
    parse_unknown_override(&bytes)
        .map(Some)
        .map_err(|e| anyhow::anyhow!("the Unknown-override file {path}: {e}"))
}

/// Parse an operator-authored Unknown-override blob (fail-closed on malformed or
/// audit-incomplete input). Split from the env/file read so it is testable without the
/// process environment, mirroring [`parse_mount_universe`].
///
/// # Errors
///
/// If the JSON is malformed, or any required audit field is blank.
pub fn parse_unknown_override(bytes: &[u8]) -> anyhow::Result<UnknownOverride> {
    let ov: UnknownOverride =
        serde_json::from_slice(bytes).map_err(|e| anyhow::anyhow!("parsing: {e}"))?;
    if !ov.is_well_formed() {
        anyhow::bail!(
            "not well-formed — kst_date, run_id, operator, reason, citation.reference and \
             citation.issuer are all required (a citation-less basis cannot authorize dispatch \
             on a date the calendar could not prove)"
        );
    }
    Ok(ov)
}

/// Whether an operator-authored override cites the snapshot identity **actually in force**.
///
/// The override's audit fields record which snapshot the operator reviewed. If that is not
/// the snapshot this run loaded, the operator reviewed a different calendar's alerts, so the
/// override cannot speak for this run. Fail-closed: an absent or partial in-force identity
/// (`snapshot=not-configured` / `snapshot=unavailable`) never matches.
pub fn override_matches_snapshot(ov: &UnknownOverride, record: &StartupRecord) -> bool {
    record.diagnostic.as_ref().is_some_and(|d| {
        d.artifact_id.as_deref() == Some(ov.snapshot_artifact_id.as_str())
            && d.calendar_id.as_deref() == Some(ov.snapshot_calendar_id.as_str())
    })
}

async fn resolve_real_probes(cfg: &DispatchCliConfig) -> anyhow::Result<(GatewayProbe, GatewayProbe)> {
    use nautilus_model::enums::AccountType;
    let adapter_cfg = nautilus_ls::config::LsAdapterConfig::from_lane_file(&cfg.lane_env_path);
    let resolved = adapter_cfg.build_config().map_err(|e| anyhow::anyhow!("{e}"))?;
    let account_no = resolved.account_no.clone();
    let sdk = ls_sdk::LsSdk::new(resolved).map_err(|e| anyhow::anyhow!("{e}"))?;
    let client = nautilus_ls::execution::LsExecClient::new(
        adapter_cfg.client_id.clone(),
        adapter_cfg.trader_id.clone(),
        account_no,
        sdk,
        AccountType::Cash,
    );
    let flat = probe_flat_start(&client).await;
    let stranded = probe_stranded_orders(&client).await;
    Ok((flat, stranded))
}

/// The daily bar-type component of a checkpoint watermark key. Confirmed against
/// [`Checkpoint::watermark_key`], whose format is `{instrument}|{bar_type}` — the runbook's
/// `endswith('1-DAY')` snippet is documentation, not the source of truth.
pub(crate) const DAILY_BAR_TYPE: &str = "1-DAY";

/// Derive `(watermark_fresh, bars_present)` from the real catalog (R5).
///
/// `watermark_fresh` requires BOTH that every daily watermark has reached the last closed
/// trading session AND that the recorded gap set is empty (KTD5). A gap means the watermark
/// is current while the coverage behind it is not trustworthy — a staleness fact, which is
/// why it lands here rather than on `bars_present`.
///
/// `bars_present` samples the catalog for actual bar files rather than trusting the
/// checkpoint, which is what makes the destructive-heal trap detectable at all: a heal that
/// wipes parquet while leaving the checkpoint intact would otherwise read as perfectly fresh.
///
/// Every failure to establish the baseline — no provable last session, an unreadable
/// checkpoint, no daily watermarks, an unparseable watermark value — fails closed to the
/// deferrable red this check has always produced when unevaluated.
pub(crate) fn evaluate_catalog(catalog: &Path, last_closed_session: Option<chrono::NaiveDate>) -> (bool, bool) {
    let Ok(checkpoint) = Checkpoint::load(&catalog.join("ingest-checkpoint.json")) else {
        // Without a checkpoint there is no instrument set to sample against, so the presence
        // question is unanswerable too — both halves fail closed together.
        return (false, false);
    };
    let dailies = checkpoint.watermarks_for(DAILY_BAR_TYPE);
    // Presence is asked PER WATERMARKED INSTRUMENT, not of the tree as a whole. A bare
    // "is there any parquet anywhere" sample is satisfied by the 1-MINUTE series that sit
    // beside the daily ones, so a heal that wiped every daily bar would still read as
    // present — and a heal that wiped just one symbol's would go entirely unnoticed.
    let bars_present = !dailies.is_empty()
        && dailies.iter().all(|(instrument, _)| daily_bars_present(catalog, instrument));
    let Some(expected) = last_closed_session else { return (false, bars_present) };
    if dailies.is_empty() {
        return (false, bars_present);
    }
    // `>=` not `==`: a catalog ingested past the last proven session (the calendar lags the
    // ingest) is ahead, not stale. An unparseable value is `None` and fails the test.
    let all_current = dailies.iter().all(|(_, d)| d.is_some_and(|d| d >= expected));
    // KTD5 scoped to the daily bar type, exactly like the watermark test above. The minute
    // series share this gap list and lag the daily set by design, so reading it unfiltered
    // lets a minute-series gap red a current daily catalog — permanently, since recorded
    // gaps are never cleared.
    let daily_gap = checkpoint.gaps().iter().any(|g| g.bar_type == DAILY_BAR_TYPE);
    (all_current && !daily_gap, bars_present)
}

/// Whether `instrument` has at least one daily parquet on disk. Series directories are named
/// `{instrument}-{bar_type}-...`, so the prefix pins both the instrument and the bar type and
/// cannot be satisfied by another symbol's or another resolution's files.
fn daily_bars_present(catalog: &Path, instrument: &str) -> bool {
    let bars = catalog.join("data").join("bars");
    let prefix = format!("{instrument}-{DAILY_BAR_TYPE}-");
    let Ok(series) = std::fs::read_dir(&bars) else { return false };
    series.flatten().any(|entry| {
        entry.file_name().to_string_lossy().starts_with(&prefix)
            && std::fs::read_dir(entry.path()).is_ok_and(|mut files| {
                files.any(|f| f.is_ok_and(|f| f.path().extension().is_some_and(|e| e == "parquet")))
            })
    })
}

/// Whether the catalog covers the session's OWN trading date for every traded
/// instrument — `produce_report`'s `catalog_has_range` gate (U8/R12).
///
/// This is the twin's prerequisite, not a freshness check: the paper twin replays
/// the session's decisions, so it is only meaningful once the post-session ingest
/// has landed that session's bars. Asked PER TRADED INSTRUMENT and against the
/// daily watermark, mirroring [`evaluate_catalog`]'s discipline — a whole-tree
/// sample would be satisfied by the minute series beside the daily ones.
///
/// Fails closed on every unestablished input (unreadable checkpoint, unparseable
/// trading date, an instrument with no watermark): a false here is
/// `TwinPending`, which is explicitly re-runnable per run id, so the conservative
/// answer costs nothing but a later re-production.
///
/// NOTE on ordering, because it decides what a finalize-time call can produce:
/// the KRX witness is retrospective, so at the moment a session finalizes its own
/// daily bar is normally NOT yet ingested and this returns false. That is the
/// honest answer — the report written at finalize records a re-runnable
/// `TwinPending` rather than a fabricated twin, which is already a strict
/// improvement on the previous state (no report at all, so `read_report` returned
/// `None` and the rung-2 refusal was structural rather than informative). A
/// *Computed* twin additionally requires re-production after that ingest lands.
pub(crate) fn session_range_in_catalog(catalog: &Path, trading_date: &str, symbols: &[String]) -> bool {
    let Ok(session) = chrono::NaiveDate::parse_from_str(trading_date, "%Y%m%d") else {
        return false;
    };
    let Ok(checkpoint) = Checkpoint::load(&catalog.join("ingest-checkpoint.json")) else {
        return false;
    };
    if symbols.is_empty() {
        return false;
    }
    symbols.iter().all(|instrument| {
        checkpoint
            .watermark(instrument, DAILY_BAR_TYPE)
            .is_some_and(|wm| wm >= session)
            && daily_bars_present(catalog, instrument)
    })
}

#[allow(clippy::too_many_arguments)]
fn build_context(
    cfg: &DispatchCliConfig,
    chain_authorized_rung: u8,
    kill_switch_engaged: bool,
    kill_switch_has_record: bool,
    probes: (GatewayProbe, GatewayProbe),
    readiness: ReadinessVerdict,
    date_fact: CalendarDateFact,
    unknown_override: Option<UnknownOverride>,
    last_closed_session: Option<chrono::NaiveDate>,
) -> DispatchContext {
    let now_utc = Utc.timestamp_opt(cfg.now_unix, 0).single().unwrap_or_else(Utc::now);
    let catalog = cfg.data_home.join("catalog");
    // The stub stays FIRST and test-only (KTD6): it is the only way the suite drives the
    // three outcomes deterministically. What changes is the fallthrough — an unset stub used
    // to mean "not evaluated → always red", so a clean ingest still burned one of the three
    // deferrals the pre-registration permits per 5-session window. Now it reads the catalog.
    let (watermark_fresh, bars_present) = match cfg.catalog_stub.as_deref() {
        Some("ok") => (true, true),
        Some("empty") => (true, false),
        Some("stale") => (false, false),
        Some(_) => (false, false), // an unrecognized stub value stays a deferrable red
        None => evaluate_catalog(&catalog, last_closed_session),
    };
    let budget = match cfg.budget_stub.as_deref() {
        Some("unmeasured") => BudgetHeadroom::Unmeasured,
        Some("low") => BudgetHeadroom::Measured { remaining: cfg.budget_plan - 1, plan: cfg.budget_plan },
        _ => BudgetHeadroom::Measured { remaining: cfg.budget_plan + 1000, plan: cfg.budget_plan },
    };
    DispatchContext {
        now_unix: cfg.now_unix,
        today_kst: kst_trading_date(now_utc),
        trading_env: cfg.trading_env.clone(),
        lane_env_present: cfg.lane_env_path.exists(),
        resolved_env_is_paper: cfg.trading_env.as_deref().map(|e| e.eq_ignore_ascii_case("paper")),
        live_lock_held: is_held(&catalog, LockKind::Live),
        date_fact,
        // The PRESERVED time-of-day window only (U12); the date decision is `date_fact`.
        window_open: WeekdayKrxCalendar.in_time_window(now_utc),
        run_id: cfg.run_id.clone().unwrap_or_default(),
        unknown_override,
        watermark_fresh,
        bars_present,
        flat_start: probes.0,
        stranded_orders: probes.1,
        kill_switch_engaged,
        kill_switch_has_record,
        budget,
        chain_authorized_rung,
        requested_rung: cfg.requested_rung,
        lane: cfg.lane,
        readiness,
    }
}

/// Resolve the AUTHORITATIVE calendar DATE fact for this dispatch AND build the mandatory
/// redacted, dispatch-date-targeted startup record from a SINGLE per-invocation load
/// (U12/#188, KTD1–KTD6). The composition root: a deterministic `date_fact_stub` wins (the
/// Enforced offline seam); otherwise it loads the env-configured snapshot ONCE and derives
/// both the record and the fact from that one `LoadedCalendar` (the #187 single-load
/// discipline — the diagnostic and the decision cannot disagree).
fn resolve_calendar_for_dispatch(
    cfg: &DispatchCliConfig,
    now_utc: chrono::DateTime<Utc>,
) -> (CalendarDateFact, StartupRecord, Option<chrono::NaiveDate>) {
    // Enforced-only after the Ladder Consumer Retirement Gate (#189 U9, KTD3): the date gate no
    // longer consults LS_CALENDAR_ADOPTION, and the startup record names the enforced posture.
    // The deterministic offline seam: a stubbed fact is authoritative; still emit a record so the
    // composition-root diagnostic path is exercised (no snapshot loaded → `snapshot=not-configured`).
    if let Some(fact) = cfg.date_fact_stub {
        // No snapshot is loaded on this seam, so no session date can be PROVEN. `None`
        // propagates as "unprovable", which the catalog check fails closed on.
        return (fact, stub_startup_record(CalendarAdoption::Enforced, fact), None);
    }
    let path = nautilus_ls::calendar::snapshot_path_from_env();
    let loaded = nautilus_ls::calendar::resolve_and_load(path.as_deref(), now_utc, cfg.adoption);
    resolve_date_fact_and_record(CalendarAdoption::Enforced, &loaded, now_utc)
}

/// How far back to scan for the last proven Trading Session. KRX has never closed for
/// anything near this long, so a window this wide either finds a session or proves the
/// calendar cannot answer — it never runs off the end of a normal holiday cluster.
const LAST_SESSION_LOOKBACK_DAYS: i64 = 30;

/// Derive the authoritative [`CalendarDateFact`] and the dispatch-date-targeted
/// [`StartupRecord`] from ONE already-loaded calendar (KTD2, load-once-derive-twice). Pure
/// and env-free so the resolver tests inject a fixture-built `LoadedCalendar` directly.
///
/// Enforced-only after the Ladder Consumer Retirement Gate (#189 U9, KTD3): the `KrxCalendar`
/// fact from the snapshot is authoritative, or [`CalendarDateFact::Unavailable`] on ANY
/// load/use/query failure (no weekday fallback), never `Unknown`. The weekday date decision is
/// retired; only the time-of-day window (`in_time_window`) survives (KTD7).
pub(crate) fn resolve_date_fact_and_record(
    adoption: CalendarAdoption,
    loaded: &nautilus_ls::calendar::LoadedCalendar,
    now_utc: chrono::DateTime<Utc>,
) -> (CalendarDateFact, StartupRecord, Option<chrono::NaiveDate>) {
    // KST = UTC+9, no DST — the same civil-date shift `kst_trading_date` uses.
    let kst_date = (now_utc + chrono::Duration::hours(9)).date_naive();
    let record = nautilus_ls::calendar::build_startup_record_targeted(
        "lab-live-dispatch",
        adoption,
        loaded,
        now_utc,
        Some(kst_date),
    );
    // The snapshot fact is authoritative; any load/use/query failure → Unavailable.
    let view = loaded.calendar().and_then(|cal| cal.as_of(now_utc).ok());
    let date_fact = date_fact_from_view(view.as_ref(), kst_date);
    // The last PROVEN Trading Session strictly before today, from this SAME load (KTD2's
    // load-once-derive-twice discipline extended to a third derivation). This is what the
    // catalog watermark is measured against — never the clock, which reads a weekend or a
    // KRX holiday as a stale watermark and reds a perfectly current catalog.
    //
    // `last_session` is proof-preserving: scanning backward it stops at an `Unknown` reached
    // before any proven session and yields `Indeterminate`. That is the correct morning
    // behaviour, not a defect — before the day's calendar refresh certifies yesterday, the
    // last session genuinely cannot be proven, and an unprovable baseline must not be used
    // to declare a catalog fresh.
    let last_closed_session = view.as_ref().and_then(|v| {
        let start = kst_date.checked_sub_signed(chrono::Duration::days(LAST_SESSION_LOOKBACK_DAYS))?;
        let range = nautilus_ls_calendar::DateRange::half_open(start, kst_date).ok()?;
        match v.last_session(&range).ok()? {
            nautilus_ls_calendar::SessionSearch::Found(d) => Some(d),
            _ => None,
        }
    });
    (date_fact, record, last_closed_session)
}

/// Build the startup record for the stubbed-fact offline seam (no snapshot is loaded, so the
/// diagnostic is `None`/`snapshot=not-configured`). The resulting action comes from the SAME
/// [`resulting_action`](nautilus_ls::calendar::resulting_action) mapping
/// [`build_startup_record_targeted`](nautilus_ls::calendar::build_startup_record_targeted) uses,
/// so a stub run's diagnostic cannot drift from a real one.
pub(crate) fn stub_startup_record(adoption: CalendarAdoption, fact: CalendarDateFact) -> StartupRecord {
    // A stub represents a successfully-resolved calendar fact except Unavailable (which
    // stands in for a load/use/query failure).
    let available = fact != CalendarDateFact::Unavailable;
    let action = nautilus_ls::calendar::resulting_action(adoption, available);
    StartupRecord { consumer: "lab-live-dispatch".to_string(), adoption, diagnostic: None, action }
}

/// Run the phase-1 dispatch gate: load the chain, gather the context, decide, record the
/// attempt (on a valid chain, unless throttled), and report. A refusal is chain history,
/// not a silent exit; a throttle is a re-run and is never written as a terminal record
/// (KTD5).
pub fn run_dispatch(cfg: &DispatchCliConfig) -> anyhow::Result<DispatchGateOutcome> {
    let chain = DispatchChain::open(&cfg.data_home)?;
    let mut state = chain.load();
    let now_dt = Utc.timestamp_opt(cfg.now_unix, 0).single().unwrap_or_else(Utc::now);

    // Resolve the authoritative calendar date fact + build the mandatory redacted startup
    // record from ONE per-invocation load (U12/#188, KTD1–KTD6), and emit it to the
    // non-persisted diagnostic channel (stderr) BEFORE any early-return refusal below — the
    // mandatory diagnostic must fire on EVERY --dispatch exit path (an absent/defective chain
    // still authorizes nothing, but the operator still gets the calendar posture). Shadow's
    // dispatch outcome/chain stay byte-identical to Legacy because this is stderr-only.
    let (date_fact, startup_record, last_closed_session) = resolve_calendar_for_dispatch(cfg, now_dt);
    nautilus_ls::calendar::emit_startup_record(&startup_record);

    // A record can only be appended onto a valid epoch. On no/defective chain, report
    // and direct to registration — never append a session-dispatch onto a broken or
    // unopened chain (it would violate the epoch-opens-with-a-registration invariant).
    match &state.status {
        ChainStatus::Valid => {}
        ChainStatus::NoChain => {
            return Ok(DispatchGateOutcome {
                result: GateResult::Refused,
                lines: vec![
                    "DISPATCH refused: no dispatch chain (rung 0, suspended)".to_string(),
                    "  run `lab-live --genesis` to register the chain at rung 1 first".to_string(),
                ],
                appended: false,
            });
        }
        ChainStatus::Defective(why) => {
            return Ok(DispatchGateOutcome {
                result: GateResult::Refused,
                lines: vec![
                    format!("DISPATCH refused: dispatch chain is defective ({why}) — rung 0"),
                    "  re-register the chain (epoch rollover) before any session".to_string(),
                ],
                appended: false,
            });
        }
    }

    // Load pre-registration once (optional in phase 1) — used by both the auto
    // de-escalation band checks and the readiness reducer.
    let prereg_loaded = cfg
        .prereg_path
        .as_ref()
        .and_then(|p| crate::dispatch::prereg::load_optional(p).ok().flatten());

    // F3: the next `--dispatch` auto-de-escalates for any unconsumed limit events BEFORE
    // authorizing, so the session runs at the corrected rung; the events are marked
    // consumed so they never double-fire.
    let mut deescalation_line: Option<String> = None;
    if let Some(rec) = apply_deescalation(&chain, &cfg.data_home, prereg_loaded.as_ref().map(|l| &l.values), now_dt)? {
        if let RecordKind::DeEscalation(d) = &rec.body.kind {
            deescalation_line = Some(format!(
                "  auto de-escalation: rung {} → {} on {} limit event(s)",
                d.from_rung, d.to_rung, d.events.len()
            ));
        }
        state = chain.load();
    }

    let kill_switch_has_record = state
        .records
        .iter()
        .any(|r| matches!(&r.body.kind, RecordKind::SafetyTrip(t) if t.trip == SafetyTripKind::KillSwitch));

    let probes = match &cfg.probe_stub {
        Some(p) => p.clone(),
        None => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(resolve_real_probes(cfg))?
        }
    };

    // The readiness verdict over the trailing K live-lane sessions (R11). A stub forces a
    // verdict for deterministic gate tests; otherwise it is computed from the registry +
    // chain + report sidecar (read-only). No frozen window (phase 1) → NotEvaluated.
    let (readiness, readiness_catalog) = match cfg.readiness_stub.as_deref() {
        Some("green") => (ReadinessVerdict::Green, Default::default()),
        Some("red") => (ReadinessVerdict::Red, Default::default()),
        Some(_) => (ReadinessVerdict::NotEvaluated, Default::default()),
        None => compute_readiness(
            &cfg.data_home,
            &state.records,
            prereg_loaded.as_ref().map(|l| &l.values),
        ),
    };

    // The attended Unknown override is a consequential operator action — gate it on the
    // same fresh-nonce + attendance rule as a deferral before it may proceed an Unknown
    // date (U12). A rejected override is dropped (leaving Unknown to refuse), noted below.
    let mut override_note: Option<String> = None;
    let effective_override: Option<UnknownOverride> = match cfg.unknown_override.clone() {
        None => None,
        // An override that cites a different snapshot reviewed different alerts — it cannot
        // authorize THIS run. Checked before the nonce gate: a stale-snapshot override is
        // rejected on its own terms, and a fresh nonce must never launder it.
        //
        // Skipped on the offline `date_fact_stub` seam, which loads no snapshot at all (its
        // startup record carries no diagnostic, so there is no in-force identity to bind
        // to). Real runs always resolve a snapshot, so the binding always applies to them.
        Some(ov)
            if cfg.date_fact_stub.is_none() && !override_matches_snapshot(&ov, &startup_record) =>
        {
            // The renderer already prefixes "attended Unknown override rejected:".
            override_note = Some(format!(
                "it cites snapshot artifact_id={} but that is not the snapshot in force — \
                 re-author the override against the snapshot whose alerts you reviewed",
                ov.snapshot_artifact_id
            ));
            None
        }
        Some(ov) => {
            let unattended_marker = match cfg.attended_override {
                Some(true) => None,
                Some(false) => Some("forced unattended".to_string()),
                None => detect_unattended_marker(),
            };
            let gate =
                OperatorGate { unattended_marker, nonce: cfg.nonce.clone(), now_unix: cfg.now_unix };
            match gate.authorize("attended Unknown calendar override") {
                Ok(()) => Some(ov),
                Err(e) => {
                    override_note = Some(e);
                    None
                }
            }
        }
    };

    let ctx = build_context(
        cfg,
        state.authorized_rung,
        state.kill_switch_engaged,
        kill_switch_has_record,
        probes,
        readiness,
        date_fact,
        effective_override,
        last_closed_session,
    );

    // Whether the attended override actually proceeded an Unknown date (audit): recorded on
    // the session-dispatch. Only meaningful when the date fact is Unknown and the override
    // binds to this exact KST date + run.
    let applied_override: Option<UnknownOverride> = if ctx.date_fact == CalendarDateFact::Unknown {
        ctx.unknown_override.clone().filter(|o| o.covers(&ctx.today_kst, &ctx.run_id))
    } else {
        None
    };
    // An override that survived the snapshot and nonce gates but does not BIND to this date
    // and run would otherwise refuse indistinguishably from having supplied no override at
    // all — the operator sees only the generic Unknown red and cannot tell that their file
    // was read, let alone that a date or run-id typo is the reason.
    if ctx.date_fact == CalendarDateFact::Unknown
        && applied_override.is_none()
        && override_note.is_none()
    {
        if let Some(ov) = &ctx.unknown_override {
            override_note = Some(format!(
                "it authorizes {}/run {} but this dispatch is {}/run {} — an override binds to \
                 the exact KST date and LS_DISPATCH_RUN_ID",
                ov.kst_date, ov.run_id, ctx.today_kst, ctx.run_id
            ));
        }
    }

    // The nonce authorizes the ACT of deferring. Without deferrals it is irrelevant.
    let mut nonce_note: Option<String> = None;
    let nonce_ok = if cfg.deferrals.is_empty() {
        false
    } else {
        let unattended_marker = match cfg.attended_override {
            Some(true) => None,
            Some(false) => Some("forced unattended".to_string()),
            None => detect_unattended_marker(),
        };
        let gate = OperatorGate {
            unattended_marker,
            nonce: cfg.nonce.clone(),
            now_unix: cfg.now_unix,
        };
        match gate.authorize("deferral") {
            Ok(()) => true,
            Err(e) => {
                nonce_note = Some(e);
                false
            }
        }
    };

    let outcomes = run_checks(&ctx);
    let decision = decide(&outcomes, &cfg.deferrals, nonce_ok);

    let mut lines = Vec::new();
    let header = match decision.result {
        GateResult::Green => "DISPATCH green — session authorized",
        GateResult::Refused => "DISPATCH refused",
        GateResult::Throttled => "DISPATCH throttled — re-run (not recorded)",
    };
    lines.push(format!("{header} (rung {} requested, chain rung {})", cfg.requested_rung, state.authorized_rung));
    if let Some(l) = &deescalation_line {
        lines.push(l.clone());
    }
    for r in &decision.records {
        let flag = if r.deferred { " [DEFERRED]" } else { "" };
        lines.push(format!("  {:<22} {:?} {:?}{flag} — {}", r.name, r.tier, r.status, r.detail));
    }
    if let Some(note) = &nonce_note {
        lines.push(format!("  deferral nonce rejected: {note}"));
    }
    if let Some(note) = &override_note {
        lines.push(format!("  attended Unknown override rejected: {note}"));
    }
    if let Some(ov) = &applied_override {
        // The citation is operator free text — scrub it on the way to stdout for the same
        // reason the chain record scrubs it on the way to disk.
        lines.push(format!(
            "  attended Unknown override applied for {} (run {}, citation {}/{}) — calendar status unchanged",
            ov.kst_date,
            ov.run_id,
            crate::artifacts::scrub(&ov.citation.issuer),
            crate::artifacts::scrub(&ov.citation.reference)
        ));
    }
    if !decision.refused_items.is_empty() {
        lines.push(format!("  red items: {}", decision.refused_items.join(", ")));
    }

    // A throttle is a re-run: never a terminal record (KTD5).
    if decision.result == GateResult::Throttled {
        return Ok(DispatchGateOutcome { result: decision.result, lines, appended: false });
    }

    let outcome = match decision.result {
        GateResult::Green => DispatchOutcome::Green,
        GateResult::Refused => DispatchOutcome::Refused,
        GateResult::Throttled => unreachable!(),
    };
    // A green dispatch under a red readiness runs at rung-1 probation: the effective rung
    // is forced to 1 while the record still carries the chain-authorized rung, so capital
    // history stays reconstructable from the chain alone (R11). Probation never refuses.
    let effective_rung = if decision.result == GateResult::Green {
        if readiness.is_probation() {
            RUNG_MIN
        } else {
            cfg.requested_rung
        }
    } else {
        state.authorized_rung
    };
    if decision.result == GateResult::Green && readiness.is_probation() {
        lines.push(format!(
            "  readiness RED → rung-1 probation (chain rung {}, effective rung {RUNG_MIN})",
            state.authorized_rung
        ));
    }
    chain.append(
        Utc.timestamp_opt(cfg.now_unix, 0).single().unwrap_or_else(Utc::now),
        state.authorized_rung,
        effective_rung,
        None,
        RecordKind::SessionDispatch(SessionDispatch {
            outcome,
            checks: decision.records.clone(),
            deferrals: decision.deferrals.clone(),
            readiness: Some(readiness_summary(readiness, &readiness_catalog)),
            unknown_override: applied_override,
        }),
    )?;

    Ok(DispatchGateOutcome { result: decision.result, lines, appended: true })
}

/// Register the dispatch chain genesis at rung 1 (an explicit, nonce-gated operator
/// action — the chain never genesis-es implicitly, KD2).
pub fn run_genesis(cfg: &DispatchCliConfig) -> anyhow::Result<Vec<String>> {
    OperatorGate {
        unattended_marker: detect_unattended_marker(),
        nonce: cfg.nonce.clone(),
        now_unix: cfg.now_unix,
    }
    .authorize("chain genesis registration")
    .map_err(|e| anyhow::anyhow!(e))?;

    let chain = DispatchChain::open(&cfg.data_home)?;
    match chain.load().status {
        ChainStatus::NoChain => {}
        ChainStatus::Valid => anyhow::bail!("chain already registered — refusing to re-genesis a live chain"),
        ChainStatus::Defective(why) => {
            anyhow::bail!("chain is defective ({why}) — repair via re-registration, not genesis")
        }
    }
    let now = Utc.timestamp_opt(cfg.now_unix, 0).single().unwrap_or_else(Utc::now);
    let rec = chain.append(now, 1, 1, None, RecordKind::Genesis)?;
    Ok(vec![format!(
        "GENESIS registered — chain authorizes rung 1 (record {}, {})",
        rec.body.record_id, rec.body.kst_trading_date
    )])
}


// ===========================================================================
// U6 — the LiveNode mounter behind a green dispatch (R5, R8; KTD2, KTD3; AE2).
//
// One operator-confirmed command takes a green, unconsumed, same-day dispatch through:
// operator confirm (fresh nonce; no-TTY loud refusal) → the Live advisory lock, held
// through the session so the check-then-mount TOCTOU gap is closed (KTD2) → a consumption
// marker recording the mounted run id AT MOUNT TIME (so a session that never finalizes
// leaves `.tmp-<run_id>` residue the de-escalation scan matches to this consumed
// dispatch, R14(f), chain-driven) → the LiveNode build → [`node.run`, live-only] →
// fail-closed teardown → finalize with the dispatch↔run linkage threaded into the
// manifest (KTD3). The session's exec path records its own gateway dispatches into the
// per-credential spend-ledger bucket so the budget-headroom check reads more than ingest
// spend (KTD5).
//
// Rung authorization is the phase-2 hardcoded rung-1 stub (R5): the mounter honors the
// chain's authorized rung, but the ladder machinery (evidence-verified escalation,
// automatic de-escalation, the rung fraction reaching sizing) lands in U10; the fraction
// is metadata here, not yet a sizing input. `node.run` is never driven offline (the
// documented invariant) — offline tests stop at node construction and drive the
// consumption/finalize/spend seams directly.
// ===========================================================================

use nautilus_common::enums::Environment;
use nautilus_common::factories::ExecutionClientFactory;
use nautilus_live::node::LiveNode;
use nautilus_ls::config::LsAdapterConfig;
use nautilus_ls::factories::{LsDataClientFactory, LsExecutionClientFactory};
use nautilus_model::identifiers::TraderId;

use crate::agent::sink::DecisionSink;
use crate::runner::watchdog::Heartbeats;
use crate::artifacts::manifest::DispatchLink;
use crate::artifacts::RunSource;
use crate::dispatch::chain::{Consumption, MountAuthz};
use crate::strategy::orb::{OrbStrategy, SelectedSymbol};

/// The resolved configuration for a live mount (env-gathered, but constructible directly
/// so offline tests bypass the process environment).
#[derive(Debug, Clone)]
pub struct MountConfig {
    /// The data home (chain, catalog, spend ledger, registry live here).
    pub data_home: std::path::PathBuf,
    /// The rung this mount requests (guard rail; must not exceed the authorized effective
    /// rung — R15). U6 is the rung-1 stub.
    pub requested_rung: u8,
    /// The credential lane hash (SHA-256 of the resolved appkey; spend-ledger precedent —
    /// never the raw key or account number).
    pub lane_hash: String,
    /// The resolved trading environment (`"paper"` | `"live"`), recorded in the manifest
    /// (closes the gap where `RunSource::Live` means paper-live today, KTD3).
    pub trading_env: String,
    /// The budget-numerator fraction recorded for this rung (KTD6). U6 stub: `1.0` — the
    /// prereg-driven fraction and its sizing threading land in U10; it is metadata here.
    pub rung_fraction: f64,
    /// The operator nonce authorizing the mount.
    pub nonce: Option<String>,
    /// Wall-clock unix seconds (injectable for deterministic tests).
    pub now_unix: i64,
    /// Library-only override of the attended/unattended detection (see
    /// [`DispatchCliConfig::attended_override`]). The bin's env gather always leaves this
    /// `None` — the no-TTY refusal can never be suppressed from the CLI.
    pub attended_override: Option<bool>,
}

/// A resolved authorization to mount a live session behind a green dispatch (U6). Carries
/// the identity a reviewer binds the run to (KTD3) and the run id the consumption marker
/// recorded at mount time.
#[derive(Debug, Clone, PartialEq)]
pub struct MountAuthorization {
    /// The run id the session finalizes under (recorded in the consumption marker so
    /// residue classification is chain-driven, R14(f)).
    pub run_id: String,
    /// The session-dispatch record id this mount consumes.
    pub dispatch_record_id: String,
    /// The chain-authorized rung.
    pub chain_rung: u8,
    /// The effective rung the session runs at (rung 1 under probation, R11).
    pub effective_rung: u8,
    /// The budget-numerator fraction recorded (KTD6).
    pub rung_fraction: f64,
    /// The credential lane hash.
    pub lane_hash: String,
    /// The resolved trading environment.
    pub trading_env: String,
}

impl MountAuthorization {
    /// The dispatch↔run linkage to thread into the mounted run's manifest (KTD3): binds
    /// the run to its authorization plus the rung metadata reducers key on.
    pub fn dispatch_link(&self) -> DispatchLink {
        DispatchLink {
            dispatch_id: self.dispatch_record_id.clone(),
            rung: self.effective_rung,
            rung_fraction: self.rung_fraction,
            lane: self.lane_hash.clone(),
            trading_env: self.trading_env.clone(),
        }
    }
}


/// Authorize and prepare a live mount behind a green dispatch (U6). In strict order:
///
/// 1. the operator nonce gate (fresh nonce, no-TTY loud refusal) — mounting a live session
///    is at least as consequential as a deferral;
/// 2. the chain must offer a green, unconsumed, same-day dispatch to mount
///    ([`MountAuthz::Ready`]); a consumed / expired / absent dispatch refuses;
/// 3. the requested rung must not exceed the authorized effective rung (R15);
/// 4. acquire the Live advisory lock and hold it through the session (returned to the
///    caller) — a lock held by another process between gate and mount refuses (the TOCTOU
///    arm, KTD2);
/// 5. append a consumption marker recording the mounted run id AT MOUNT TIME (R14(f)).
///
/// Returns the authorization plus the held Live lock; `node.run` and teardown are the
/// caller's (live-only). None of the refusal arms mounts or consumes.
///
/// # Errors
///
/// A loud, typed refusal string on any failing precondition; a chain-append failure.
pub fn authorize_mount(
    chain: &DispatchChain,
    cfg: &MountConfig,
    strategy_id: &str,
    strategy_version: u32,
) -> anyhow::Result<(MountAuthorization, AdvisoryLock)> {
    // 1. Operator confirm — a live mount is nonce-gated like a deferral (no-TTY loud).
    let unattended_marker = match cfg.attended_override {
        Some(true) => None,
        Some(false) => Some("forced unattended".to_string()),
        None => detect_unattended_marker(),
    };
    OperatorGate { unattended_marker, nonce: cfg.nonce.clone(), now_unix: cfg.now_unix }
        .authorize("live mount")
        .map_err(|e| anyhow::anyhow!(e))?;

    // 2. The chain must offer a green, unconsumed, same-day dispatch to mount.
    let now = Utc.timestamp_opt(cfg.now_unix, 0).single().unwrap_or_else(Utc::now);
    let today = kst_trading_date(now);
    let state = chain.load();
    let (record_id, chain_rung, effective_rung) = match state.mount_authz(&today) {
        MountAuthz::Ready { record_id, chain_rung, effective_rung } => {
            (record_id, chain_rung, effective_rung)
        }
        MountAuthz::Consumed => anyhow::bail!(
            "mount refused: the latest green dispatch is already consumed by a session — a green \
             dispatch is single-use; re-run `--dispatch` for a fresh authorization"
        ),
        MountAuthz::Expired => anyhow::bail!(
            "mount refused: the latest green dispatch is from a previous KST trading day (expired) \
             — re-run `--dispatch` today"
        ),
        MountAuthz::None => anyhow::bail!(
            "mount refused: no green dispatch available to mount (rung 0 / refused / no dispatch)"
        ),
    };

    // 3. Guard rail: never mount above the authorized rung (R15).
    if cfg.requested_rung > effective_rung {
        anyhow::bail!(
            "mount refused: requested rung {} exceeds the authorized effective rung {} — rung \
             selection is a guard rail, not an operator feature (R15)",
            cfg.requested_rung,
            effective_rung
        );
    }

    // 4. Acquire the Live lock and hold it through the session (TOCTOU close, KTD2).
    let lock = live_guard(&cfg.data_home)?;

    // 5. Consumption marker at mount time, recording the intended run id (R14(f)). The
    //    Dispatch-lock append is permitted while holding the Live lock (KTD2: Dispatch has
    //    no counterpart, so a lock-holding session may still record).
    let run_identifier = crate::artifacts::run_id(now, RunSource::Live, strategy_id, strategy_version);
    chain.append(
        now,
        chain_rung,
        effective_rung,
        state.last_prereg_hash.clone(),
        RecordKind::Consumption(Consumption {
            dispatch_record_id: record_id.clone(),
            run_id: Some(run_identifier.clone()),
        }),
    )?;

    Ok((
        MountAuthorization {
            run_id: run_identifier,
            dispatch_record_id: record_id,
            chain_rung,
            effective_rung,
            rung_fraction: cfg.rung_fraction,
            lane_hash: cfg.lane_hash.clone(),
            trading_env: cfg.trading_env.clone(),
        },
        lock,
    ))
}



/// Build a `LiveNode` with the ORB strategy mounted for a live session, returning the
/// [`LiveMount`] handle set (U6; live-session-driver U2, R3, KTD3/KTD4). The mount point
/// the operator command drives after a green dispatch — offline-buildable (the repo never
/// drives `node.run` offline), so this is exactly the seam offline wiring tests exercise.
///
/// **One SDK, one ledger (KTD3).** The exec client is built *here*, not inside the
/// factory: one [`LsSdk`] (hence one kill-switch `Arc<Inner>`) and one
/// `Arc<Mutex<FillLedger>>` are created, handed to the node through a stateful
/// [`LsExecutionClientFactory`], and retained on the returned [`LiveTeardownSession`]. A
/// teardown built from its own client would halt a *different* `AtomicBool` and read an
/// *empty* ledger — two silent no-ops. The data client still resolves from the same lane
/// config, so the session's exec path and the gate's flat-start probe read one credential.
///
/// The `rung_fraction` is the authorized rung's pre-registered budget-numerator multiplier
/// (KTD6): the runner supplies it here and it reaches sizing via
/// [`OrbStrategy::with_rung_fraction`], composed with the equity factor and the ratio-ATR
/// tilt — never an `OrbParams`/manifest field, so a rung move produces zero head-identity
/// diff. `1.0` sizes exactly as v30.
///
/// # Errors
///
/// Any credential-resolution / node-builder / client-registration / strategy-mount failure.
pub fn build_live_session_node(
    adapter_cfg: LsAdapterConfig,
    params: OrbParams,
    selected: Vec<SelectedSymbol>,
    sink: DecisionSink,
    rung_fraction: f64,
    now_unix: i64,
) -> anyhow::Result<LiveMount> {
    // 1. ONE SDK + ONE ledger, built outside the factory so both can be retained (KTD3).
    let resolved = adapter_cfg.build_config().map_err(|e| anyhow::anyhow!("lane credentials: {e}"))?;
    let account_no = resolved.account_no.clone();
    let sdk = LsSdk::new(resolved).map_err(|e| anyhow::anyhow!("sdk: {e}"))?;
    let ledger: std::sync::Arc<std::sync::Mutex<FillLedger>> =
        std::sync::Arc::new(std::sync::Mutex::new(FillLedger::new()));
    let exec = nautilus_ls::execution::LsExecClient::new_with_ledger(
        // The builder derives the client name from the factory when `None` is passed, so
        // pre-building under the factory's own name keeps the node's client identity
        // byte-identical to the stateless path.
        LsExecutionClientFactory::new().name().to_string(),
        adapter_cfg.trader_id.clone(),
        account_no,
        sdk.clone(),
        nautilus_model::enums::AccountType::Cash,
        std::sync::Arc::clone(&ledger),
    );
    let order_tasks = exec.order_tasks();
    let exec_factory = LsExecutionClientFactory::with_client(exec);

    // 2. The strategy, with the runtime dead-man feeder threaded in. Capture the emission
    //    gate BEFORE `add_strategy` moves the strategy into the trader (KTD4).
    let heartbeats = Heartbeats::new(now_unix);
    let marks = MarkFeed::new();
    let strategy = OrbStrategy::new(params, selected, sink.clone(), 1.0)
        .with_rung_fraction(rung_fraction)
        .with_heartbeats(heartbeats.clone())
        .with_mark_feed(marks.clone());
    let gate = strategy.emission_gate();

    let mut node = {
        // The logger-initializing build is serialized process-wide.
        let _guard = node_build_lock();
        LiveNode::builder(TraderId::from("LS-LAB-001"), Environment::Live)
            .map_err(|e| anyhow::anyhow!("live node builder: {e}"))?
            .with_name("ls-lab-live")
            .add_data_client(None, Box::new(LsDataClientFactory), Box::new(adapter_cfg.clone()))
            .map_err(|e| anyhow::anyhow!("data client: {e}"))?
            .add_exec_client(None, Box::new(exec_factory), Box::new(adapter_cfg))
            .map_err(|e| anyhow::anyhow!("exec client: {e}"))?
            .build()
            .map_err(|e| anyhow::anyhow!("node build: {e}"))?
    };
    node.add_strategy(strategy).map_err(|e| anyhow::anyhow!("mount ORB strategy: {e}"))?;
    // Grabbable before `run` and cloneable — the driver's stop path depends on it (KTD5).
    let handle = node.handle();

    Ok(LiveMount {
        node,
        handles: LiveSessionHandles {
            session: LiveTeardownSession::new(gate, sdk, ledger, order_tasks),
            heartbeats,
            handle,
            sink,
            marks,
        },
    })
}


// ---------------------------------------------------------------------------
// U2 (rung-1 readiness) — the `lab-live --mount` operator command (R3; KTD4/KTD5/KTD7).
//
// Wires the shipped mount machinery (authorize_mount / build_live_session_node) into a
// reachable CLI, sizing the live strategy at the pre-registered rung fraction from the head's REAL
// governed params. The attended live-session DRIVER (consume -> node.run -> fail-closed
// teardown -> finalize) shipped in the live-session-driver turn: `--mount` now RUNS an
// attended rung-1 session. Every fail-closed precheck still runs BEFORE the dispatch is
// consumed, so a recoverable config error costs an attempt (exit 71) rather than the whole
// `--dispatch` cycle — see [`run_mount`] for the ordering contract.
// ---------------------------------------------------------------------------

use serde::Deserialize;

use nautilus_ls::ingest::BarKind;
use nautilus_model::identifiers::InstrumentId;

use crate::strategy::orb::SessionGapPrices;



/// One symbol of the resolved live-mount universe. The operator materializes the dispatch
/// lane's daily/t8407 read into `LS_MOUNT_UNIVERSE_FILE` (a JSON array of these); `SelectedSymbol`
/// itself is not deserializable (it carries nautilus `InstrumentId`/`BarType`). Distinct from the
/// offline test path, which builds `SelectedSymbol`s directly.
#[derive(Debug, Clone, Deserialize)]
struct MountUniverseSymbol {
    /// The KST session date (`YYYY-MM-DD`) this row was resolved FOR. Required: `--mount`
    /// re-runs no selection, so without it a file resolved for another day is
    /// indistinguishable from a fresh one and its stale symbols and `today_open` prices
    /// would simply be traded.
    session_date: String,
    /// The 6-digit KRX short code (e.g. `005930`); mapped to `{shcode}.XKRX`.
    shcode: String,
    /// Canonical integer prior-close and today-open defining this symbol-session's opening gap.
    prior_close: i64,
    today_open: i64,
    #[serde(default)]
    prior_atr: Option<f64>,
    #[serde(default)]
    prior_open_vol_mean: Option<f64>,
    #[serde(default)]
    prior_illiq: Option<f64>,
}

impl MountUniverseSymbol {
    fn into_selected(self) -> anyhow::Result<SelectedSymbol> {
        let id = InstrumentId::from(format!("{}.XKRX", self.shcode).as_str());
        Ok(SelectedSymbol {
            instrument_id: id,
            bar_type: BarKind::Minute(1)
                .bar_type(id)
                .map_err(|e| anyhow::anyhow!("bar type for {}: {e}", self.shcode))?,
            gap_prices: SessionGapPrices::new(self.prior_close, self.today_open),
            prior_atr: self.prior_atr,
            prior_open_vol_mean: self.prior_open_vol_mean,
            prior_illiq: self.prior_illiq,
        })
    }
}

/// Resolve the live-mount universe from `LS_MOUNT_UNIVERSE_FILE` (fail-closed if absent/empty).
///
/// # Errors
///
/// If the env var is unset, the file is unreadable/malformed, or the universe is empty.
pub fn resolve_mount_universe() -> anyhow::Result<Vec<SelectedSymbol>> {
    let path = std::env::var("LS_MOUNT_UNIVERSE_FILE").map_err(|_| {
        anyhow::anyhow!(
            "mount refused: LS_MOUNT_UNIVERSE_FILE is required — the resolved daily/t8407 universe \
             (a JSON array of {{shcode, prior_close, today_open, prior_atr?, …}}) the live session trades"
        )
    })?;
    let bytes = std::fs::read(&path).map_err(|e| anyhow::anyhow!("reading mount universe {path}: {e}"))?;
    parse_mount_universe(&bytes)
}

/// Parse a mount-universe JSON blob into the live session's `Vec<SelectedSymbol>` (fail-closed on
/// malformed/empty). Split from the env/file read so it is testable without the process
/// environment.
///
/// # Errors
///
/// If the JSON is malformed or the universe is empty.
pub fn parse_mount_universe(bytes: &[u8]) -> anyhow::Result<Vec<SelectedSymbol>> {
    parse_mount_universe_for(bytes, None)
}

/// Parse a mount-universe blob, additionally binding every row to `expected_kst` when supplied.
///
/// The mount consumes an already-resolved universe and re-runs no selection, so a file left
/// over from a previous session is not detectable from its contents — yesterday's symbols and
/// yesterday's `today_open` parse exactly as cleanly as today's. Binding the session date is
/// what makes a stale file a refusal instead of a silently wrong session. Checked pre-consume,
/// so a mismatch costs the operator nothing but a re-run.
///
/// # Errors
///
/// If the JSON is malformed, the universe is empty, or any row was resolved for another date.
pub fn parse_mount_universe_for(
    bytes: &[u8],
    expected_kst: Option<&str>,
) -> anyhow::Result<Vec<SelectedSymbol>> {
    let rows: Vec<MountUniverseSymbol> =
        serde_json::from_slice(bytes).map_err(|e| anyhow::anyhow!("parsing mount universe: {e}"))?;
    if rows.is_empty() {
        anyhow::bail!("mount refused: the resolved universe (LS_MOUNT_UNIVERSE_FILE) is empty");
    }
    if let Some(want) = expected_kst {
        if let Some(bad) = rows.iter().find(|r| r.session_date.trim() != want) {
            anyhow::bail!(
                "mount refused: the resolved universe was built for {} but this session is {} \
                 — re-run `lab-mount-universe` for today (a stale universe would trade \
                 yesterday's symbols at yesterday's opening prices)",
                bad.session_date,
                want
            );
        }
    }
    rows.into_iter().map(MountUniverseSymbol::into_selected).collect()
}

/// Resolve the v34 head governed `OrbParams` for the mount, fail-closed against a zero-size
/// (all-levers-off `default()`) head (KTD7): a `default()` head has `risk_per_trade_krw == 0`,
/// which sizes every order to zero shares — the exact bug that would silently trade nothing.
///
/// # Errors
///
/// If the resolved head params size to zero.
pub fn resolve_mount_head_params(data_home: &Path) -> anyhow::Result<OrbParams> {
    // Pin the head to the expected version (LS_TURN_EXPECT_VERSION) when set, so the mount sizes
    // from the exact certified head even when older-version same-code runs share the data home;
    // a missing pinned head collapses to default() and is caught by the zero-size guard below.
    let params = crate::dispatch::ladder::head_governed_params_pinned(
        data_home,
        crate::dispatch::ladder::head_version_pin(),
    );
    if params.risk_per_trade_krw <= 0.0 {
        anyhow::bail!(
            "mount refused: the resolved head governed params size to ZERO (risk_per_trade_krw={:.0}) \
             — the data home's latest finalized run must be the v34 head (risk 299,340), never the \
             all-levers-off default; check LS_DATA_HOME points at the v34 epoch",
            params.risk_per_trade_krw
        );
    }
    Ok(params)
}

/// The `lab-live --mount` operator command — now the **live driver path**
/// (live-session-driver U5, R6; KTD3/KTD5/KTD7).
///
/// Order is the safety property. The paper interlock is first; the attendance/nonce gate
/// second; then **every fail-closed precheck runs BEFORE the dispatch is consumed** — a
/// green dispatch is single-use, and burning it on a recoverable config error (an
/// unarmable pre-registration, a missing fraction, a bad universe, a build failure) would
/// cost the operator a whole `--dispatch` cycle. Only once the session is guaranteed to
/// run does [`authorize_mount`] consume it and take the held Live lock, and only then does
/// the driver run.
///
/// `node.run` is driven here and ONLY here — the commit gate never reaches this function.
pub(crate) fn run_mount() -> anyhow::Result<ExitCode> {
    nautilus_ls::calendar::emit_startup_from_env("lab-live");
    // 1. Paper interlock FIRST — before any resolution, chain read, or gate (R3).
    if std::env::var("LS_TRADING_ENV").as_deref() != Ok("paper") {
        eprintln!(
            "mount refused: LS_TRADING_ENV must be `paper` (this adapter is paper-only; the \
             live-lane flip is a separate later step)"
        );
        return Ok(ExitCode::from(MOUNT_NOT_PAPER));
    }
    let data_home: PathBuf = std::env::var("LS_DATA_HOME")
        .map_err(|_| anyhow::anyhow!("mount refused: LS_DATA_HOME is required (absolute path)"))?
        .into();
    let requested_rung: u8 =
        std::env::var("LS_DISPATCH_RUNG").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let now_unix: i64 = std::env::var("LS_DISPATCH_NOW_UNIX")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| Utc::now().timestamp());
    let nonce = std::env::var("LS_DISPATCH_NONCE").ok().filter(|s| !s.trim().is_empty());
    let lane_name = std::env::var("LS_LANE").unwrap_or_else(|_| "domestic".to_string());
    let lane_env_path: PathBuf = std::env::var("LS_DISPATCH_LANE_ENV")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(format!(".env.{lane_name}")));

    // 2. Operator attendance/nonce gate — a live mount attempt is attended, no-TTY loud (R3).
    let gate = OperatorGate { unattended_marker: detect_unattended_marker(), nonce, now_unix };
    if let Err(e) = gate.authorize("live mount") {
        eprintln!("mount refused: {}", nautilus_ls::scrub::scrub_secrets(e.as_str()));
        return Ok(ExitCode::from(MOUNT_REFUSED_ATTEND));
    }

    // 3. Read-only mountability peek + the effective rung to size. NOTHING is consumed
    //    here: consumption is step 7, after every recoverable failure has been ruled out.
    let chain = DispatchChain::open(&data_home)?;
    let now = Utc.timestamp_opt(now_unix, 0).single().unwrap_or_else(Utc::now);
    let today = kst_trading_date(now);
    let effective_rung = match chain.load().mount_authz(&today) {
        MountAuthz::Ready { effective_rung, .. } => effective_rung,
        MountAuthz::Consumed => {
            eprintln!("mount refused: the latest green dispatch is already consumed — re-run --dispatch");
            return Ok(ExitCode::from(MOUNT_REFUSED_ATTEND));
        }
        MountAuthz::Expired => {
            eprintln!(
                "mount refused: the latest green dispatch is expired (previous KST day) — re-run --dispatch today"
            );
            return Ok(ExitCode::from(MOUNT_REFUSED_ATTEND));
        }
        MountAuthz::None => {
            eprintln!("mount refused: no green dispatch to mount (rung 0 / refused / none) — run --dispatch");
            return Ok(ExitCode::from(MOUNT_REFUSED_ATTEND));
        }
    };
    if requested_rung > effective_rung {
        eprintln!(
            "mount refused: requested rung {requested_rung} exceeds the authorized effective rung \
             {effective_rung} — rung selection is a guard rail, not an operator feature (R15)"
        );
        return Ok(ExitCode::from(MOUNT_REFUSED_ATTEND));
    }

    // 4-6. Every remaining fail-closed precheck, ALL of them pre-consume. A failure here
    //      leaves the green dispatch intact for a corrected re-run.
    let prepared = match mount_inputs_from_env(lane_env_path.clone())
        .and_then(|inputs| prepare_mount(&data_home, &inputs, effective_rung, now_unix))
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "mount refused (pre-consume): {} — the green dispatch is NOT consumed; fix and re-run --mount",
                nautilus_ls::scrub::scrub_secrets(&e.to_string())
            );
            return Ok(ExitCode::from(MOUNT_PRECHECK_FAILED));
        }
    };

    // 7. CONSUME — the last step before the session is driven (the Live lock is held
    //    through the session, closing the check-then-mount TOCTOU gap).
    let mount_cfg = MountConfig {
        data_home: data_home.clone(),
        requested_rung,
        lane_hash: prepared.lane_hash.clone(),
        trading_env: "paper".to_string(),
        rung_fraction: prepared.fraction,
        nonce: std::env::var("LS_DISPATCH_NONCE").ok().filter(|s| !s.trim().is_empty()),
        now_unix,
        attended_override: None,
    };
    let (auth, _live_lock) = match authorize_mount(
        &chain,
        &mount_cfg,
        &prepared.params.strategy_id,
        prepared.params.strategy_version,
    ) {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("mount refused: {}", nautilus_ls::scrub::scrub_secrets(&e.to_string()));
            return Ok(ExitCode::from(MOUNT_REFUSED_ATTEND));
        }
    };

    println!(
        "mount running: env=paper rung={} rung_fraction={} head_code_hash={} head_params_hash={} \
         universe={} session_secs={} run_id={}",
        auth.effective_rung,
        prepared.fraction,
        crate::artifacts::manifest::strategy_code_hash(),
        crate::dispatch::ladder::governed_params_hash(&prepared.params),
        prepared.symbols.len(),
        prepared.driver.session_secs,
        auth.run_id
    );

    // 8. Drive the session. `node.run` is live-only and reached ONLY from here.
    let PreparedMount { mount, driver, params, symbols, lane_hash, .. } = prepared;
    let LiveMount { mut node, handles } = mount;
    let session_probe = handles.session.clone();
    let authority = auth.session_authority(&data_home)?;
    let identity = SessionIdentity::Orb(params);
    let manifest = identity
        .live_manifest(LiveManifestParts {
            authority: &authority,
            symbols: &symbols,
            trading_date: &today,
            started_utc: now,
            // The ladder's ORB lane is not the daily lineage's paper stage (KTD2).
            paper_stage: false,
        })
        .map_err(|e| anyhow::anyhow!("mount refused: {e}"))?;
    let ctx = LiveSessionContext {
        data_home: data_home.clone(),
        authority,
        manifest,
        symbols,
        trading_date: today.clone(),
        // The ladder's runner records none of the rehearsal row types.
        observations: crate::runner::live::shared::SessionObservations::new(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let outcome = runtime.block_on(run_live_session(
        handles,
        &driver,
        &ctx,
        system_clock(),
        // THE live-only seam. Everything around it is offline-proven.
        move |_handle| async move { node.run().await },
    ))?;

    // 9. Record the session's own gateway dispatches into the per-credential bucket
    //    (ladder KTD5). A LOWER BOUND by construction — only the calls the session can
    //    account for after the fact (the teardown's flat legs + its cancel scans, plus one
    //    per observed fill) — which is exactly how the budget-headroom check treats it
    //    (advisory, deferrable), never an over-count that would refuse a valid dispatch.
    let observed = 2
        + outcome.report.cancel_attempts as usize
        + session_probe.ledger().lock().unwrap_or_else(|e| e.into_inner()).fills().len();
    for i in 0..observed {
        record_session_spend(&data_home, &lane_hash, now_unix + i as i64)?;
    }

    let verdict = if outcome.abnormal { "ABNORMAL" } else { "clean" };
    println!(
        "mount finalized ({verdict}): run_dir={} teardown_retries={} canceled={} flat_confirmed={} trip={:?} \
         gateway_dispatches_recorded={observed}",
        outcome.run_dir.display(),
        outcome.report.retries(),
        outcome.report.canceled,
        outcome.report.flat_confirmed,
        outcome.trip
    );
    let (code, messages) = mount_verdict(&outcome, ctx.authority.is_rehearsal());
    for m in messages {
        eprintln!("{m}");
    }
    Ok(ExitCode::from(code))
}

/// Everything resolved and built before the green dispatch is consumed (U5). Producing
/// this successfully is the guarantee that the session will run.
pub struct PreparedMount {
    /// The built node + its handle set.
    pub mount: LiveMount,
    /// The driver tunables, with the fail-closed-armed watchdog limits.
    pub driver: LiveDriverConfig,
    /// The head governed params the session trades.
    pub params: OrbParams,
    /// The traded universe, as instrument-id strings.
    pub symbols: Vec<String>,
    /// The pre-registered rung fraction the strategy was sized at.
    pub fraction: f64,
    /// The credential lane hash the session's spend is bucketed under.
    pub lane_hash: String,
}

/// The `--mount` file/tunable inputs, gathered from the environment by the bin but
/// **constructible directly**, so the pre-consume prechecks are testable without mutating
/// the process environment.
#[derive(Debug, Clone)]
pub struct MountInputs {
    /// The frozen pre-registration (`LS_DISPATCH_PREREG`).
    pub prereg_path: PathBuf,
    /// The operator keepalive file (`LS_MOUNT_KEEPALIVE`) whose mtime is the operator
    /// dead-man feeder.
    pub keepalive_path: PathBuf,
    /// The credential lane env file.
    pub lane_env_path: PathBuf,
    /// The resolved daily/t8407 universe (`LS_MOUNT_UNIVERSE_FILE`).
    pub universe_path: PathBuf,
    /// Attended session length before the driver's timer stops the node.
    pub session_secs: u64,
    /// Drain budget after a stop is requested, before the driver hard-stops the node
    /// (seconds, floored at 1 — the backstop cannot be disabled).
    pub stop_grace_secs: u64,
    /// Watchdog evaluation cadence (seconds, floored at 1).
    pub watchdog_tick_secs: u64,
    /// Starting account balance recorded on the equity curve.
    pub starting_balance: f64,
}

/// Gather the `--mount` inputs from the process environment (the bin path).
///
/// # Errors
///
/// If a required path variable is unset or empty.
pub fn mount_inputs_from_env(lane_env_path: PathBuf) -> anyhow::Result<MountInputs> {
    let required = |key: &str, what: &str| -> anyhow::Result<PathBuf> {
        std::env::var(key)
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("{key} is required ({what}; ABSOLUTE path)"))
    };
    Ok(MountInputs {
        prereg_path: required("LS_DISPATCH_PREREG", "the frozen fraction + band + envelope")?,
        keepalive_path: required(
            "LS_MOUNT_KEEPALIVE",
            "the operator keepalive file the attended operator refreshes; its mtime is the \
             operator dead-man feeder",
        )?,
        universe_path: required("LS_MOUNT_UNIVERSE_FILE", "the resolved daily/t8407 universe")?,
        lane_env_path,
        session_secs: env_u64("LS_MOUNT_SESSION_SECS", DEFAULT_SESSION_SECS),
        stop_grace_secs: env_u64("LS_MOUNT_STOP_GRACE_SECS", DEFAULT_STOP_GRACE_SECS),
        watchdog_tick_secs: env_u64("LS_MOUNT_WATCHDOG_TICK_SECS", DEFAULT_WATCHDOG_TICK_SECS),
        starting_balance: std::env::var("LS_MOUNT_STARTING_BALANCE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_STARTING_BALANCE),
    })
}

/// Run every fail-closed precheck and build the node — **all before any consumption**
/// (U5, R6). Each arm is a recoverable operator error, so failing here must leave the
/// green dispatch intact for a corrected re-run.
///
/// That property is structural, not just sequential: this function takes **no
/// [`DispatchChain`]**, so it has no way to consume a dispatch however it fails.
/// [`run_mount`] calls it before [`authorize_mount`], which is the only consumer.
///
/// # Errors
///
/// A missing/unloadable pre-registration, a missing rung fraction, an **unarmable
/// watchdog envelope**, a missing operator keepalive file, a zero-size head, an
/// empty/unreadable universe, or a credential/node-build failure.
pub fn prepare_mount(
    data_home: &Path,
    inputs: &MountInputs,
    effective_rung: u8,
    now_unix: i64,
) -> anyhow::Result<PreparedMount> {
    // (a) The pre-registered fraction for the effective rung (fail-closed, ladder KTD5).
    let prereg = crate::dispatch::prereg::load(&inputs.prereg_path)?;
    let fraction = prereg.values.rung_fraction(effective_rung)?;

    // (b) ARM the full watchdog envelope from the pre-registration — fail-closed (KTD8 /
    //     ladder KTD9). A missing heartbeat interval or max-loss threshold refuses the
    //     mount HERE, before consume: a half-armed envelope must never run a session.
    let limits = WatchdogLimits::from_prereg(&prereg.values).map_err(|e| {
        anyhow::anyhow!(
            "the watchdog envelope cannot be armed from the pre-registration ({e}) — refusing to \
             run a session on a half-envelope"
        )
    })?;

    // (c) The operator keepalive file. Its mtime is the operator dead-man feeder, and an
    //     absent file reads as stale — so a missing one would trip the envelope on the
    //     first tick. Require it up front rather than discovering it after the consume.
    if !inputs.keepalive_path.exists() {
        anyhow::bail!(
            "the operator keepalive file does not exist — create it before mounting, or the \
             operator dead-man trips on the first watchdog tick"
        );
    }

    // (d) v34's real governed params (fail-closed vs a zero-size default head) + the
    //     resolved universe.
    let params = resolve_mount_head_params(data_home)?;
    let today_kst = crate::dispatch::chain::kst_trading_date(
        Utc.timestamp_opt(now_unix, 0).single().unwrap_or_else(Utc::now),
    );
    let universe = parse_mount_universe_for(
        &std::fs::read(&inputs.universe_path).map_err(|e| {
            anyhow::anyhow!("reading the mount universe {}: {e}", inputs.universe_path.display())
        })?,
        Some(&today_kst),
    )?;
    let symbols: Vec<String> = universe.iter().map(|s| s.instrument_id.to_string()).collect();

    // (e) The credential lane hash + the node build itself. A build failure is the last
    //     thing that can go wrong recoverably.
    let lane_hash = resolve_lane_hash(&inputs.lane_env_path)?;
    let adapter_cfg = LsAdapterConfig::from_lane_file(&inputs.lane_env_path);
    let mount = build_live_session_node(
        adapter_cfg,
        params.clone(),
        universe,
        DecisionSink::new(),
        fraction,
        now_unix,
    )?;

    let driver = LiveDriverConfig {
        session_secs: inputs.session_secs,
        stop_grace: stop_grace(inputs.stop_grace_secs, limits.heartbeat_interval_secs),
        watchdog_tick: Duration::from_secs(inputs.watchdog_tick_secs.max(1)),
        limits,
        mark_policy: MarkPolicy::default(),
        keepalive_path: inputs.keepalive_path.clone(),
        cancel_attempts: TEARDOWN_CANCEL_ATTEMPTS,
        flat_attempts: TEARDOWN_FLAT_ATTEMPTS,
        starting_balance: inputs.starting_balance,
        // The ladder inherits no book, so there is nothing for a floor to cover.
        book_stop_floors: std::collections::HashMap::new(),
    };
    Ok(PreparedMount { mount, driver, params, symbols, fraction, lane_hash })
}


// ---------------------------------------------------------------------------
// U3 (rung-1 readiness) — the ladder + diagnostic operator CLI (R2/R4; KTD4).
// Thin argv arms over the already-tested library functions. `--head` is read-only (preflight);
// `--escalate`/`--reregister`/`--clear-killswitch` are nonce-gated, each with a DISTINCT exit
// code so a no-TTY shell never mistakes a refusal for a completed mutation.
// ---------------------------------------------------------------------------

const ESCALATE_REFUSED: u8 = 78;
const REREGISTER_REFUSED: u8 = 79;
const CLEAR_REFUSED: u8 = 80;

/// The absolute data home (`LS_DATA_HOME`).
fn env_data_home() -> anyhow::Result<PathBuf> {
    Ok(std::env::var("LS_DATA_HOME")
        .map_err(|_| anyhow::anyhow!("LS_DATA_HOME is required (absolute path)"))?
        .into())
}

/// Wall-clock unix seconds (`LS_DISPATCH_NOW_UNIX`, else now).
fn env_now_unix() -> i64 {
    std::env::var("LS_DISPATCH_NOW_UNIX").ok().and_then(|v| v.parse().ok()).unwrap_or_else(|| Utc::now().timestamp())
}

/// The operator gate from the environment (nonce + no-TTY detection + clock). The bin can never
/// suppress the no-TTY refusal — `detect_unattended_marker` is not env-overridable.
pub(crate) fn operator_gate_from_env(now_unix: i64) -> OperatorGate {
    OperatorGate {
        unattended_marker: detect_unattended_marker(),
        nonce: std::env::var("LS_DISPATCH_NONCE").ok().filter(|s| !s.trim().is_empty()),
        now_unix,
    }
}

/// `--head` (R2): print the running binary's head identity as verbatim fact lines. Read-only, no
/// nonce, no chain append. `strategy_code_hash()` is the SOLE head discriminator — the operator
/// confirms the binary embeds v35 by hash-equality against the documented `7571abef…`. The printed
/// `governed_params_hash(&OrbParams::default())` is a version-invariant constant (identical across
/// v9…v35, KTD7), so it does NOT confirm v35's governed values; it is labeled as such, never as a
/// version readout (the binary carries no hash→version map).
pub(crate) fn run_head_diagnostic() -> anyhow::Result<ExitCode> {
    nautilus_ls::calendar::emit_startup_from_env("lab-live");
    let code_hash = crate::artifacts::manifest::strategy_code_hash();
    let params_hash = crate::dispatch::ladder::governed_params_hash(&OrbParams::default());
    println!("head strategy_code_hash={code_hash}");
    println!(
        "head governed_params_hash(default)={params_hash} [version-invariant constant — NOT a v35 confirmation]"
    );
    println!(
        "head-check: the binary embeds v35 IFF strategy_code_hash == the documented head 7571abef… \
         (the sole discriminator; the binary carries no hash→version map)"
    );
    Ok(ExitCode::SUCCESS)
}

/// `--escalate` (R4): nonce-gated escalation over `run_escalation` — prints the appended
/// escalation evidence or the blocking reason (AE5 of the ladder plan); a refusal/block is a
/// distinct non-zero exit.
pub(crate) fn run_escalate_cli() -> anyhow::Result<ExitCode> {
    nautilus_ls::calendar::emit_startup_from_env("lab-live");
    let data_home = env_data_home()?;
    let now_unix = env_now_unix();
    let now = Utc.timestamp_opt(now_unix, 0).single().unwrap_or_else(Utc::now);
    let gate = operator_gate_from_env(now_unix);
    let prereg_path = std::env::var("LS_DISPATCH_PREREG")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("--escalate refused: LS_DISPATCH_PREREG is required (N + the expectation band)"))?;
    let prereg = crate::dispatch::prereg::load(Path::new(&prereg_path))?;
    let chain = DispatchChain::open(&data_home)?;
    let expected_version = crate::dispatch::ladder::head_version_pin();
    match crate::dispatch::ladder::run_escalation(&chain, &data_home, &gate, &prereg.values, expected_version, now) {
        Ok(rec) => {
            if let RecordKind::Escalation(e) = &rec.body.kind {
                println!(
                    "escalate: rung {} -> {} authorized; {} clean session(s) cited",
                    e.from_rung,
                    e.to_rung,
                    e.evidence_run_ids.len()
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("{}", nautilus_ls::scrub::scrub_secrets(&e.to_string()));
            Ok(ExitCode::from(ESCALATE_REFUSED))
        }
    }
}

/// `--reregister` (R4): nonce-gated re-registration over `run_reregistration`, bounded to rung-0
/// requalification or current-epoch repair — an out-of-bound `set_rung` ABOVE the chain-earned
/// rung is refused (an upward jump would bypass the earned-escalation gate, R15). The reason
/// (`LS_DISPATCH_REASON`) is scrubbed before it lands (also inside `chain.reregister`).
pub(crate) fn run_reregister_cli() -> anyhow::Result<ExitCode> {
    nautilus_ls::calendar::emit_startup_from_env("lab-live");
    let data_home = env_data_home()?;
    let now_unix = env_now_unix();
    let now = Utc.timestamp_opt(now_unix, 0).single().unwrap_or_else(Utc::now);
    let gate = operator_gate_from_env(now_unix);
    let set_rung: u8 = std::env::var("LS_DISPATCH_RUNG")
        .ok()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| anyhow::anyhow!(
            "--reregister refused: LS_DISPATCH_RUNG is required (target: 0 to suspend/requalify, or \
             ≤ the chain-earned rung to repair)"
        ))?;
    let raw_reason = std::env::var("LS_DISPATCH_REASON").unwrap_or_default();
    if raw_reason.trim().is_empty() {
        anyhow::bail!("--reregister refused: LS_DISPATCH_REASON is required (the audited who/why)");
    }
    let reason = nautilus_ls::scrub::scrub_secrets(&raw_reason);
    let chain = DispatchChain::open(&data_home)?;
    let state = chain.load();
    // The re-registration ceiling is the CURRENT authorized rung, floored at 1 — NOT the all-time
    // peak. A re-registration may requalify to rung 0, re-enter to rung 1 after a suspension, or
    // repair to the current epoch's rung — never restore a rung the ladder was de-escalated or
    // suspended OUT of. Using the historical peak would let an operator re-register straight back to
    // a de-escalated rung with only a nonce, bypassing the N-clean re-earn gate (R15).
    let ceiling = state.authorized_rung.max(1);
    if set_rung > ceiling {
        eprintln!(
            "--reregister refused: target rung {set_rung} exceeds the re-registration ceiling \
             {ceiling} (the current authorized rung, floored at 1) — a re-registration may only \
             requalify to rung 0/1 or repair the current epoch; restoring a de-escalated rung must \
             be re-earned through the escalation evidence gate (R15)"
        );
        return Ok(ExitCode::from(REREGISTER_REFUSED));
    }
    match crate::dispatch::ladder::run_reregistration(&chain, &gate, set_rung, &reason, state.last_prereg_hash.clone(), now) {
        Ok(_rec) => {
            println!("reregister: chain set to rung {set_rung} (reason recorded, scrubbed)");
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("{}", nautilus_ls::scrub::scrub_secrets(&e.to_string()));
            Ok(ExitCode::from(REREGISTER_REFUSED))
        }
    }
}

/// `--clear-killswitch` (R4): nonce + attendance gated clear of a persisted kill-switch trip over
/// `clear_kill_switch`, capturing a scrubbed operator reason (`LS_DISPATCH_REASON`) — re-arming
/// trading after an auto-halt must leave an audited who/why.
pub(crate) fn run_clear_killswitch_cli() -> anyhow::Result<ExitCode> {
    nautilus_ls::calendar::emit_startup_from_env("lab-live");
    let data_home = env_data_home()?;
    let now_unix = env_now_unix();
    let now = Utc.timestamp_opt(now_unix, 0).single().unwrap_or_else(Utc::now);
    let gate = operator_gate_from_env(now_unix);
    let raw_reason = std::env::var("LS_DISPATCH_REASON").unwrap_or_default();
    if raw_reason.trim().is_empty() {
        anyhow::bail!("--clear-killswitch refused: LS_DISPATCH_REASON is required (the audited who/why for re-arming trading)");
    }
    let chain = DispatchChain::open(&data_home)?;
    let chain_rung = chain.load().authorized_rung;
    match clear_kill_switch(&chain, &gate, &raw_reason, now, chain_rung) {
        Ok(()) => {
            println!("clear-killswitch: persisted kill-switch trip cleared at rung {chain_rung} (reason recorded, scrubbed)");
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("{}", nautilus_ls::scrub::scrub_secrets(&e.to_string()));
            Ok(ExitCode::from(CLEAR_REFUSED))
        }
    }
}

/// `--rung-report` (R5; KTD6): the agent's read-only post-session verification — clean/limit-event
/// classification of the trailing live-lane sessions, cumulative rung P&L against the v34 band,
/// N-progress toward escalation, and the readiness verdict. Appends nothing; no nonce. Prints the
/// head hash it evaluated under so a stale-binary reading is self-evident.
pub(crate) fn run_rung_report() -> anyhow::Result<ExitCode> {
    nautilus_ls::calendar::emit_startup_from_env("lab-live");
    let data_home = env_data_home()?;
    let prereg_path = std::env::var("LS_DISPATCH_PREREG")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("--rung-report refused: LS_DISPATCH_PREREG is required (the band + N)"))?;
    let prereg = crate::dispatch::prereg::load(Path::new(&prereg_path))?;
    let chain = DispatchChain::open(&data_home)?;
    let state = chain.load();
    let from_rung = state.authorized_rung.max(1);
    let expected_version = crate::dispatch::ladder::head_version_pin();
    let report =
        crate::dispatch::ladder::build_rung_report(&data_home, &state.records, from_rung, &prereg.values, expected_version);

    // The head hash the report evaluated under (KTD6) — a stale-binary reading is self-evident.
    println!(
        "rung-report head_code_hash={} (v35 IFF == 7571abef…) head_params_hash={}",
        report.head_code_hash, report.head_params_hash
    );
    println!(
        "rung-report rung={} clean={}/{} cum_pnl={:.0} band=[{:.0},{:.0}] in_band={}",
        report.from_rung, report.clean.len(), report.n_required, report.cum_pnl, report.band.0, report.band.1, report.in_band
    );
    for rid in &report.clean {
        println!("  clean {rid}");
    }
    for rid in &report.limit_event {
        println!("  limit-event {rid} (excluded from the clean count)");
    }
    for rid in &report.head_mismatched {
        println!("  head-mismatched {rid} (NOT counted — ran under a different head)");
    }
    match &report.escalation {
        crate::dispatch::ladder::EscalationCheck::Ready { to_rung, evidence } => {
            println!("rung-report escalation: READY -> rung {to_rung} ({} clean session(s) cited)", evidence.len());
        }
        crate::dispatch::ladder::EscalationCheck::Blocked(reason) => {
            println!("rung-report escalation: BLOCKED — {reason}");
        }
    }
    let verdict = match report.readiness {
        crate::dispatch::readiness::ReadinessVerdict::Green => "GREEN",
        crate::dispatch::readiness::ReadinessVerdict::Red => "RED (rung-1 probation)",
        crate::dispatch::readiness::ReadinessVerdict::NotEvaluated => "NOT-EVALUATED",
    };
    println!("rung-report readiness: {verdict} — {}", report.readiness_summary);
    Ok(ExitCode::SUCCESS)
}

