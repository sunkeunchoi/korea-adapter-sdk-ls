//! Live paper runner (U6, F2) — the same ORB runs against the paper gateway and
//! emits the same artifact set into the same registry, fail-closed. **Operator-gated:
//! never run by the offline gate.** Proven offline by wiring tests + a direct-drive
//! test; the full `node.run` session is exercised only by the `lab-live` bin.
//!
//! Safety (KTD7): the runner takes the live advisory lock, honors the paper-only
//! interlock, and at exit/market-close runs a fail-closed teardown — stop the
//! strategy's order emission first, cancel all resting orders, run a quantity-keyed
//! t0425 flatness check (positive confirmation only), and engage the exec client's
//! kill switch only AFTER the closing cancels complete (the kill-switch-ordering
//! trap). Artifacts finalize on teardown; a crash leaves the `.tmp-` run directory as
//! the aborted-run marker.

use std::path::{Path, PathBuf};

use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_ls::orders::ledger::FillDelta;
use nautilus_ls::orders::poll::DrivenOutcome;

use crate::artifacts::data_quality::{DataQualityReport, ReconcileCondition, ReconcileConditionKind};
use crate::artifacts::RunWriter;
use crate::params::OrbParams;

/// Live run configuration.
#[derive(Debug, Clone)]
pub struct LiveConfig {
    /// The data home (registry + catalog live here).
    pub data_home: PathBuf,
    /// The ORB parameter set.
    pub params: OrbParams,
    /// The operator-resolved universe (live: from a t8407/daily read; the offline
    /// tests pass symbols directly).
    pub symbols: Vec<String>,
    /// Session duration (seconds) before an automatic time-flat teardown.
    pub session_secs: u64,
    /// Starting account balance (KRW) recorded for the equity curve.
    pub starting_balance: f64,
}

/// The fail-closed teardown seam (KTD7). Abstracted so the ordering invariant is
/// unit-testable with a fake; the live impl is backed by the exec client + SDK.
pub trait LiveSession {
    /// Stop the strategy's order emission (close its [`EmissionGate`]) — called
    /// FIRST so no new order races the cancels.
    fn stop_emission(&self);
    /// Cancel every resting order. Returns the count canceled; errors if a cancel
    /// could not be confirmed.
    fn cancel_all_resting(&self) -> impl std::future::Future<Output = anyhow::Result<usize>>;
    /// A quantity-keyed flatness check — POSITIVE confirmation only (a truncated or
    /// failed read returns `false`, never a false "flat").
    fn is_flat(&self) -> impl std::future::Future<Output = bool>;
    /// Engage the kill switch (blocks any further order placement). Called only
    /// AFTER the closing cancels.
    fn halt(&self);
}

/// The outcome of a fail-closed teardown (KTD7, R5). Carries the cancel-attempt count so
/// finalize can persist the retry metric (R14(d)) even when the teardown hard-failed —
/// [`run_teardown`] never errors, so the caller always gets the report and can finalize
/// abnormally before bailing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeardownReport {
    /// Number of cancel calls made (1..=`cancel_attempts`).
    pub cancel_attempts: u64,
    /// Whether cancels were confirmed.
    pub canceled: bool,
    /// Whether flatness was POSITIVELY confirmed.
    pub flat_confirmed: bool,
}

impl TeardownReport {
    /// Retries beyond the first cancel attempt (0 if the first succeeded). More than one
    /// retry is a limit event (R14(d)).
    pub fn retries(&self) -> u64 {
        self.cancel_attempts.saturating_sub(1)
    }

    /// Whether teardown could not positively confirm a flat account — the account must
    /// be treated as NOT flat (never conclude flat on ambiguity).
    pub fn hard_failed(&self) -> bool {
        !self.canceled || !self.flat_confirmed
    }
}

/// Run the fail-closed session teardown (KTD7). The ordering is the safety property:
/// stop emission → cancel resting (retried) → confirm flat (retried, positive-only) →
/// **halt after**. Returns a [`TeardownReport`] rather than erroring: a hard-failed
/// teardown must still leave scannable artifacts (R5), so the caller finalizes on the
/// report and bails afterward.
pub async fn run_teardown<S: LiveSession>(
    session: &S,
    cancel_attempts: usize,
    flat_attempts: usize,
) -> TeardownReport {
    // 1. Stop the strategy's order emission first.
    session.stop_emission();

    // 2. Cancel all resting orders, retrying. Count the attempts made (R14(d)).
    let mut canceled = false;
    let mut attempts = 0u64;
    for _ in 0..cancel_attempts.max(1) {
        attempts += 1;
        if session.cancel_all_resting().await.is_ok() {
            canceled = true;
            break;
        }
    }

    // 3. Quantity-keyed flatness check (positive confirmation only).
    let mut flat = false;
    for _ in 0..flat_attempts.max(1) {
        if session.is_flat().await {
            flat = true;
            break;
        }
    }

    // 4. Engage the kill switch AFTER the closing cancels — always, even on failure.
    session.halt();

    TeardownReport { cancel_attempts: attempts, canceled, flat_confirmed: flat }
}

/// Finalize a run's artifacts — ALWAYS, even after a hard-failed teardown (R5): a
/// session that carries limit events must still leave scannable artifacts. Stamps the
/// teardown retry count and dedup-hit count into the data-quality report (the fields
/// U9's exceedance scan reads, R10/R14(d)) and marks the run abnormal when teardown
/// could not confirm flat. Assumes the manifest/performance/decisions are already staged
/// into the writer's tmp dir. Consumes the writer.
pub fn finalize_session(
    writer: RunWriter,
    mut dq: DataQualityReport,
    report: &TeardownReport,
    dedup_hits: u64,
) -> anyhow::Result<PathBuf> {
    dq.teardown_retries = Some(report.retries());
    dq.dedup_hits = Some(dedup_hits);
    if report.hard_failed() {
        // The observation is scrubbed at write time; it is a fixed literal here.
        dq.observations.push(
            "ABNORMAL: teardown could not positively confirm a flat account — kill switch \
             engaged; operator must reconcile"
                .to_string(),
        );
    }
    writer.write_data_quality(&dq)?;
    writer.finalize()
}

/// Persist a safety-trip record to the dispatch chain at trip time (KTD4) — call this
/// BEFORE any finalize/bail. The runtime kill switch is a per-process in-memory
/// `AtomicBool`, so a fresh dispatch process would otherwise always read it disengaged
/// and the R1 kill-switch check would be a tautology; the persisted record is what the
/// gate reads.
pub fn record_safety_trip(
    chain: &DispatchChain,
    kind: SafetyTripKind,
    run_id: Option<&str>,
    detail: &str,
    now: chrono::DateTime<Utc>,
    chain_rung: u8,
) -> anyhow::Result<()> {
    chain.append(
        now,
        chain_rung,
        chain_rung,
        None,
        RecordKind::SafetyTrip(SafetyTrip {
            trip: kind,
            action: TripAction::Engage,
            run_id: run_id.map(str::to_string),
            detail: detail.to_string(),
        }),
    )?;
    Ok(())
}

/// Clear a persisted kill-switch trip — an explicit, nonce-gated operator action
/// recorded in the chain (KTD4). Re-enabling live dispatch after a safety trip is at
/// least as consequential as a deferral, so it is behind the same fresh-nonce +
/// no-TTY loud-refusal gate.
///
/// # Errors
///
/// Refuses (loudly) without a fresh operator nonce in an attended context; propagates a
/// chain-append failure.
pub fn clear_kill_switch(
    chain: &DispatchChain,
    gate: &OperatorGate,
    reason: &str,
    now: chrono::DateTime<Utc>,
    chain_rung: u8,
) -> anyhow::Result<()> {
    gate.authorize("kill-switch clear").map_err(|e| anyhow::anyhow!(e))?;
    chain.append(
        now,
        chain_rung,
        chain_rung,
        None,
        RecordKind::SafetyTrip(SafetyTrip {
            trip: SafetyTripKind::KillSwitch,
            action: TripAction::Clear,
            run_id: None,
            detail: reason.to_string(),
        }),
    )?;
    Ok(())
}

/// Acquire the live-session advisory lock on the catalog (KTD7). Refuses (errors) if
/// the ingest lock is held — a backfill and a live session cannot run concurrently.
/// Held for the session; released on drop.
pub fn live_guard(data_home: &Path) -> anyhow::Result<AdvisoryLock> {
    let catalog = data_home.join("catalog");
    std::fs::create_dir_all(&catalog)?;
    AdvisoryLock::acquire(&catalog, LockKind::Live)
        .map_err(|e| anyhow::anyhow!("live session refused — ingest in progress: {e}"))
}

/// Count the fills emitted at an approximated price (KTD4/R14) — the limit-price
/// fallbacks plus beyond-first poll partials. Feeds `data_quality.price_approximated_fills`.
pub fn count_approximated(deltas: &[FillDelta]) -> u64 {
    deltas.iter().filter(|d| d.price_approximated).count() as u64
}

/// Record a DRIVEN poll pass's reconcile-advised condition into the data-quality
/// report (R7/R9, AE3/AE4). The drive already self-heals transient
/// inconclusiveness with bounded re-polls, so only an **exhausted** drive — still
/// inconclusive after its budget — reaches the report. The drive collapses its
/// specific inconclusive reasons (truncation, unresolved row, cumulative
/// regression, request failure) into one terminal state, so this records the
/// honest [`ReconcileConditionKind::PollInconclusive`] rather than mislabeling a
/// specific cause — the agent treats the run's accounting as suspect either way.
pub fn record_reconcile(dq: &mut DataQualityReport, outcome: &DrivenOutcome, symbol: &str) {
    if outcome.exhausted() {
        dq.reconcile_advised.push(ReconcileCondition {
            kind: ReconcileConditionKind::PollInconclusive,
            symbol: symbol.to_string(),
        });
    }
}

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
    kst_trading_date, ChainStatus, DispatchChain, DispatchOutcome, RecordKind, SafetyTrip,
    SafetyTripKind, SessionDispatch, TripAction,
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

use nautilus_ls::calendar::{DivergenceObservation, StartupRecord};
use nautilus_ls_calendar::schema::DayStatus;
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
    /// The per-consumer calendar adoption posture (U12, KTD8). Composed default Shadow: the
    /// calendar date decision is computed + recorded to stderr while the weekday
    /// `window_open`/date-fact path stays authoritative. Enforced makes the calendar the
    /// authoritative date fact (no weekday fallback); Legacy leaves the weekday path.
    pub adoption: CalendarAdoption,
    /// The current dispatch run identity the attended Unknown override binds to (U12).
    /// Absent → an empty run id (no override can bind).
    pub run_id: Option<String>,
    /// Deterministic-test injection of the tri-state calendar DATE fact (U12) — the
    /// Enforced offline seam (mirrors `catalog_stub`/`readiness_stub`). Absent → resolve
    /// from `adoption` + the env-configured snapshot. The bin's env gather leaves this
    /// `None` (the real fact is always resolved).
    pub date_fact_stub: Option<CalendarDateFact>,
    /// A library-injected attended Unknown-date override (U12). Nonce/attendance-gated in
    /// [`run_dispatch`] before it can proceed an Unknown date. The bin's env gather leaves
    /// this `None` — the narrow override enters via an operator tool path, never a blunt
    /// env toggle (mirrors [`attended_override`](Self::attended_override)).
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
        // The per-consumer adoption posture (U12, KTD8): composed default Shadow.
        adoption: nautilus_ls::calendar::adoption_from_env(),
        run_id: std::env::var("LS_DISPATCH_RUN_ID").ok().filter(|s| !s.trim().is_empty()),
        // Never stubbed from the environment: the real date fact is always resolved.
        date_fact_stub: None,
        // Never sourced from the environment: the narrow attended Unknown override enters
        // via an operator tool path, not a blunt env toggle.
        unknown_override: None,
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
) -> DispatchContext {
    let now_utc = Utc.timestamp_opt(cfg.now_unix, 0).single().unwrap_or_else(Utc::now);
    let catalog = cfg.data_home.join("catalog");
    let (watermark_fresh, bars_present) = match cfg.catalog_stub.as_deref() {
        Some("ok") => (true, true),
        Some("empty") => (true, false),
        Some("stale") => (false, false),
        _ => (false, false), // not evaluated → deferrable red
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
) -> (CalendarDateFact, StartupRecord) {
    // The deterministic offline seam: a stubbed fact is authoritative; still emit a record
    // reflecting the stub + adoption so the composition-root diagnostic path is exercised
    // (no snapshot is loaded, so it renders `snapshot=not-configured`).
    if let Some(fact) = cfg.date_fact_stub {
        return (fact, stub_startup_record(cfg.adoption, fact));
    }
    let path = nautilus_ls::calendar::snapshot_path_from_env();
    let loaded = nautilus_ls::calendar::resolve_and_load(path.as_deref(), now_utc, cfg.adoption);
    let resolved = resolve_date_fact_and_record(cfg.adoption, &loaded, now_utc);
    // Shadow-divergence classification (U3): the classified weekday-vs-calendar date-fact
    // disagreement, on the same non-persisted channel (Shadow only). Non-fatal, recorded after
    // the fact is resolved; the weekday date fact stays authoritative.
    if let Some(obs) = ladder_divergence(cfg.adoption, &loaded, now_utc) {
        nautilus_ls::calendar::emit_divergence(&obs);
    }
    resolved
}

/// The classified Shadow-divergence for the Production Ladder date gate (U3, KTD6): the weekday
/// `date_fact` (Sat/Sun → Closed, else Trading Session — a KRX holiday still reads open) vs the
/// calendar's tri-state fact for the same KST date. `Some` only under Shadow (Legacy records no
/// divergence; Enforced acts on the calendar fact). Pure + assertable — env-free, so the tests
/// inject a fixture-built `LoadedCalendar` directly.
fn ladder_divergence(
    adoption: CalendarAdoption,
    loaded: &nautilus_ls::calendar::LoadedCalendar,
    now_utc: chrono::DateTime<Utc>,
) -> Option<DivergenceObservation> {
    if adoption != CalendarAdoption::Shadow {
        return None;
    }
    let kst_date = (now_utc + chrono::Duration::hours(9)).date_naive();
    let weekday_open = WeekdayKrxCalendar.date_fact(now_utc) == CalendarDateFact::TradingSession;
    let view = loaded.calendar().and_then(|cal| cal.as_of(now_utc).ok());
    let calendar = match date_fact_from_view(view.as_ref(), kst_date) {
        CalendarDateFact::TradingSession => Some(DayStatus::TradingSession),
        CalendarDateFact::Closed => Some(DayStatus::Closed),
        CalendarDateFact::Unknown => Some(DayStatus::Unknown),
        CalendarDateFact::Unavailable => None,
    };
    Some(DivergenceObservation::new(
        "lab-live-dispatch",
        kst_date,
        weekday_open,
        calendar,
    ))
}

/// Derive the authoritative [`CalendarDateFact`] and the dispatch-date-targeted
/// [`StartupRecord`] from ONE already-loaded calendar (KTD2, load-once-derive-twice). Pure
/// and env-free so the resolver tests inject a fixture-built `LoadedCalendar` directly.
///
/// - Legacy → the weekday date fact is authoritative; the calendar is loaded + recorded but
///   never authoritative (a load error is strictly non-fatal, `action=weekday-authoritative`).
/// - Shadow → the weekday date fact stays authoritative; the calendar fact is recorded in the
///   startup record only, so the dispatch outcome/chain is byte-identical to Legacy.
/// - Enforced → the `KrxCalendar` fact from the snapshot, or [`CalendarDateFact::Unavailable`]
///   on ANY load/use/query failure (no weekday fallback), never `Unknown`.
fn resolve_date_fact_and_record(
    adoption: CalendarAdoption,
    loaded: &nautilus_ls::calendar::LoadedCalendar,
    now_utc: chrono::DateTime<Utc>,
) -> (CalendarDateFact, StartupRecord) {
    // KST = UTC+9, no DST — the same civil-date shift `kst_trading_date` uses.
    let kst_date = (now_utc + chrono::Duration::hours(9)).date_naive();
    let record = nautilus_ls::calendar::build_startup_record_targeted(
        "lab-live-dispatch",
        adoption,
        loaded,
        now_utc,
        Some(kst_date),
    );
    let date_fact = match adoption {
        // The calendar is loaded + recorded but the weekday fact acts (Legacy/Shadow).
        CalendarAdoption::Legacy | CalendarAdoption::Shadow => WeekdayKrxCalendar.date_fact(now_utc),
        // Enforced: the snapshot fact is authoritative; any load/use/query failure →
        // Unavailable, never a weekday fallback.
        CalendarAdoption::Enforced => {
            let view = loaded.calendar().and_then(|cal| cal.as_of(now_utc).ok());
            date_fact_from_view(view.as_ref(), kst_date)
        }
    };
    (date_fact, record)
}

/// Build the startup record for the stubbed-fact offline seam (no snapshot is loaded, so the
/// diagnostic is `None`/`snapshot=not-configured`). The resulting action comes from the SAME
/// [`resulting_action`](nautilus_ls::calendar::resulting_action) mapping
/// [`build_startup_record_targeted`](nautilus_ls::calendar::build_startup_record_targeted) uses,
/// so a stub run's diagnostic cannot drift from a real one.
fn stub_startup_record(adoption: CalendarAdoption, fact: CalendarDateFact) -> StartupRecord {
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
    let (date_fact, startup_record) = resolve_calendar_for_dispatch(cfg, now_dt);
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
    );

    // Whether the attended override actually proceeded an Unknown date (audit): recorded on
    // the session-dispatch. Only meaningful when the date fact is Unknown and the override
    // binds to this exact KST date + run.
    let applied_override: Option<UnknownOverride> = if ctx.date_fact == CalendarDateFact::Unknown {
        ctx.unknown_override.clone().filter(|o| o.covers(&ctx.today_kst, &ctx.run_id))
    } else {
        None
    };

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
        lines.push(format!(
            "  attended Unknown override applied for {} (run {}, citation {}/{}) — calendar status unchanged",
            ov.kst_date, ov.run_id, ov.citation.issuer, ov.citation.reference
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

/// CLI entry point for the `lab-live` bin. `--dispatch` runs the phase-1 pre-flight gate;
/// `--genesis` registers the chain; a bare invocation points at the mount recipe (the
/// mounted session lands in U6). Installs the scrubber first; maps the verdict to an
/// exit code (`research.rs` shape).
pub fn main_cli() -> ExitCode {
    nautilus_ls::scrub::install();
    match dispatch_main() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {}", nautilus_ls::scrub::scrub_secrets(&e.to_string()));
            ExitCode::FAILURE
        }
    }
}

fn dispatch_main() -> anyhow::Result<ExitCode> {
    match std::env::args().nth(1).as_deref() {
        Some("--dispatch") => {
            // The `--dispatch` path emits its OWN deterministic, dispatch-date-targeted
            // startup record from a single load inside `run_dispatch` (KTD3), so the generic
            // `Utc::now()` `emit_startup_from_env` is suppressed here — exactly one
            // `calendar-startup` line fires per --dispatch run.
            let cfg = dispatch_gate_config_from_env()?;
            let out = run_dispatch(&cfg)?;
            for l in &out.lines {
                println!("{l}");
            }
            Ok(match out.result {
                GateResult::Green => ExitCode::SUCCESS,
                GateResult::Refused => ExitCode::FAILURE,
                // A throttle is a re-run, not success and not a plain failure — a
                // distinct exit code so an operator/agent shell never mistakes it for
                // either (never look-like-ran).
                GateResult::Throttled => ExitCode::from(75),
            })
        }
        Some("--genesis") => {
            // Non-dispatch subcommands keep the generic `consumer=lab-live` startup record
            // (KTD6, uniform composition root); only `--dispatch` owns the dispatch-targeted one.
            nautilus_ls::calendar::emit_startup_from_env("lab-live");
            let cfg = dispatch_gate_config_from_env()?;
            for l in &run_genesis(&cfg)? {
                println!("{l}");
            }
            Ok(ExitCode::SUCCESS)
        }
        _ => {
            nautilus_ls::calendar::emit_startup_from_env("lab-live");
            if std::env::var("LS_TRADING_ENV").as_deref() != Ok("paper") {
                anyhow::bail!("refusing to run: set LS_TRADING_ENV=paper (this adapter is paper-only)");
            }
            anyhow::bail!(
                "lab-live: run `lab-live --dispatch` for the pre-flight gate (`--genesis` to \
                 register the chain). The mounted LiveNode session lands in U6 — see \
                 adapters/nautilus/lab/README.md"
            )
        }
    }
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
use nautilus_live::node::LiveNode;
use nautilus_ls::config::LsAdapterConfig;
use nautilus_ls::factories::{LsDataClientFactory, LsExecutionClientFactory};
use nautilus_ls::ingest::budget::{spend_ledger_path, SpendLedger};
use nautilus_model::identifiers::TraderId;

use crate::agent::sink::DecisionSink;
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

/// Build a `LiveNode` with the ORB strategy mounted for a live session (U6). The mount
/// point the operator command drives after a green dispatch — offline-buildable (the repo
/// never drives `node.run` offline), so this is exactly the seam offline wiring tests
/// exercise. The data + exec clients resolve from the same lane config, so the session's
/// exec path and the gate's flat-start probe read one credential. The threaded
/// [`DecisionSink`] is the caller's handle to drain the session's decisions into the run
/// artifacts after `node.run`.
///
/// The `rung_fraction` is the authorized rung's pre-registered budget-numerator multiplier
/// (KTD6): the runner supplies it here and it reaches sizing via
/// [`OrbStrategy::with_rung_fraction`], composed with the equity factor and the ratio-ATR
/// tilt — never an `OrbParams`/manifest field, so a rung move produces zero head-identity
/// diff. `1.0` sizes exactly as v30.
///
/// # Errors
///
/// Any node-builder / client-registration / strategy-mount failure.
pub fn build_live_session_node(
    adapter_cfg: LsAdapterConfig,
    params: OrbParams,
    selected: Vec<SelectedSymbol>,
    sink: DecisionSink,
    rung_fraction: f64,
) -> anyhow::Result<LiveNode> {
    let mut node = LiveNode::builder(TraderId::from("LS-LAB-001"), Environment::Live)
        .map_err(|e| anyhow::anyhow!("live node builder: {e}"))?
        .with_name("ls-lab-live")
        .add_data_client(None, Box::new(LsDataClientFactory), Box::new(adapter_cfg.clone()))
        .map_err(|e| anyhow::anyhow!("data client: {e}"))?
        .add_exec_client(None, Box::new(LsExecutionClientFactory), Box::new(adapter_cfg))
        .map_err(|e| anyhow::anyhow!("exec client: {e}"))?
        .build()
        .map_err(|e| anyhow::anyhow!("node build: {e}"))?;
    // Off-identity equity multiplier 1.0; the ladder rung fraction scales the risk budget
    // numerator (KTD6).
    let strategy = OrbStrategy::new(params, selected, sink, 1.0).with_rung_fraction(rung_fraction);
    node.add_strategy(strategy).map_err(|e| anyhow::anyhow!("mount ORB strategy: {e}"))?;
    Ok(node)
}

/// Record one of the mounted session's gateway dispatches (an order call, a t0425 poll)
/// into the per-credential spend-ledger bucket (KTD5). Today only the ingest pacer and
/// universe capture write the ledger, so a live session's own gateway calls would be
/// invisible to the budget-headroom check; this closes that gap. The ledger is a lower
/// bound on true spend, so the headroom verdict stays advisory-deferrable.
///
/// # Errors
///
/// A ledger-save (write/rename) failure.
pub fn record_session_spend(data_home: &Path, lane_hash: &str, at_unix: i64) -> anyhow::Result<()> {
    let catalog = data_home.join("catalog");
    let path = spend_ledger_path(&catalog);
    let mut ledger = SpendLedger::load(&path);
    ledger.record_spend(lane_hash, at_unix);
    ledger.save(&path)?;
    Ok(())
}

/// Resolve the credential lane hash from the lane env file (the bin path). Offline tests
/// pass the hash directly; the bin reads the resolved appkey and hashes it (spend-ledger
/// precedent — never the raw key).
///
/// # Errors
///
/// If the lane env file cannot resolve credentials.
pub fn resolve_lane_hash(lane_env_path: &Path) -> anyhow::Result<String> {
    let adapter_cfg = LsAdapterConfig::from_lane_file(lane_env_path);
    let resolved = adapter_cfg.build_config().map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(SpendLedger::hash_appkey(&resolved.appkey))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_ls::calendar::ResultingAction;
    use std::cell::RefCell;

    /// A fake session recording the teardown call order + simulating still-resting /
    /// not-flat conditions. `cancel_fail_first` fails that many attempts before the
    /// `cancel_ok` verdict applies, so the retry count can be exercised.
    #[derive(Default)]
    struct FakeSession {
        log: RefCell<Vec<&'static str>>,
        cancel_ok: bool,
        flat: bool,
        cancel_fail_first: RefCell<usize>,
    }

    impl LiveSession for FakeSession {
        fn stop_emission(&self) {
            self.log.borrow_mut().push("stop_emission");
        }
        async fn cancel_all_resting(&self) -> anyhow::Result<usize> {
            self.log.borrow_mut().push("cancel");
            {
                let mut fails = self.cancel_fail_first.borrow_mut();
                if *fails > 0 {
                    *fails -= 1;
                    anyhow::bail!("cancel failed (transient)");
                }
            }
            if self.cancel_ok {
                Ok(1)
            } else {
                anyhow::bail!("cancel failed")
            }
        }
        async fn is_flat(&self) -> bool {
            self.log.borrow_mut().push("is_flat");
            self.flat
        }
        fn halt(&self) {
            self.log.borrow_mut().push("halt");
        }
    }

    #[tokio::test]
    async fn teardown_order_is_stop_cancel_flat_then_halt() {
        let s = FakeSession { cancel_ok: true, flat: true, ..Default::default() };
        let r = run_teardown(&s, 3, 3).await;
        assert!(!r.hard_failed());
        let log = s.log.borrow();
        assert_eq!(log[0], "stop_emission", "emission stopped FIRST");
        assert_eq!(*log.last().unwrap(), "halt", "kill switch engaged LAST");
        assert!(log.contains(&"cancel") && log.contains(&"is_flat"));
    }

    #[tokio::test]
    async fn teardown_hard_fails_when_not_flat_but_still_halts() {
        let s = FakeSession { cancel_ok: false, flat: false, ..Default::default() };
        let r = run_teardown(&s, 2, 2).await;
        assert!(r.hard_failed(), "not flat + cancels failed -> hard fail");
        assert_eq!(*s.log.borrow().last().unwrap(), "halt", "kill switch engaged even on failure");
    }

    #[tokio::test]
    async fn teardown_hard_fails_when_cancels_ok_but_not_flat() {
        // Cancels succeed but flatness never confirms: the account is NOT concluded flat
        // on ambiguity. Guards the flat term from silently regressing.
        let s = FakeSession { cancel_ok: true, flat: false, ..Default::default() };
        let r = run_teardown(&s, 2, 2).await;
        assert!(r.hard_failed(), "cancels ok but not flat -> still hard fail");
        assert!(r.canceled && !r.flat_confirmed);
        assert_eq!(*s.log.borrow().last().unwrap(), "halt", "kill switch engaged even when not flat");
    }

    #[tokio::test]
    async fn teardown_hard_fails_when_resting_order_remains() {
        let s = FakeSession { cancel_ok: false, flat: true, ..Default::default() };
        let r = run_teardown(&s, 3, 1).await;
        assert!(r.hard_failed(), "cancel failure hard-fails after retries");
        assert_eq!(r.cancel_attempts, 3, "cancel was retried the full 3 attempts");
    }

    #[tokio::test]
    async fn teardown_retry_count_is_recorded() {
        // First cancel fails, second succeeds → 2 attempts, 1 retry (R14(d) metric).
        let s = FakeSession {
            cancel_ok: true,
            flat: true,
            cancel_fail_first: RefCell::new(1),
            ..Default::default()
        };
        let r = run_teardown(&s, 3, 1).await;
        assert!(!r.hard_failed());
        assert_eq!(r.cancel_attempts, 2);
        assert_eq!(r.retries(), 1);
    }

    #[tokio::test]
    async fn finalize_runs_even_on_hard_fail_and_stamps_the_metrics() {
        use crate::artifacts::{data_quality::DataQualityReport, RunWriter};
        let tmp = tempfile::TempDir::new().unwrap();
        let writer = RunWriter::new(tmp.path(), "run-abnormal").unwrap();
        // A hard-failed teardown: finalize must STILL run and mark the run abnormal.
        let s = FakeSession { cancel_ok: false, flat: false, ..Default::default() };
        let report = run_teardown(&s, 2, 2).await;
        assert!(report.hard_failed());
        let dq = DataQualityReport::backtest(vec![], vec![]);
        let dir = finalize_session(writer, dq, &report, 2).unwrap();
        let text = std::fs::read_to_string(dir.join("data_quality.json")).unwrap();
        assert!(text.contains("ABNORMAL"), "abnormal note present: {text}");
        assert!(text.contains("teardown_retries"));
        assert!(text.contains("\"dedup_hits\": 2"), "{text}");
    }

    #[tokio::test]
    async fn safety_trip_is_recorded_before_finalize() {
        use crate::dispatch::chain::{DispatchChain, RecordKind, SafetyTripKind};
        let tmp = tempfile::TempDir::new().unwrap();
        let chain = DispatchChain::open(tmp.path()).unwrap();
        let now = Utc.timestamp_opt(1_752_600_000, 0).unwrap();
        chain.append(now, 1, 1, None, RecordKind::Genesis).unwrap();

        let s = FakeSession { cancel_ok: false, flat: false, ..Default::default() };
        let report = run_teardown(&s, 2, 2).await;
        assert!(report.hard_failed());
        // Trip record BEFORE finalize (KTD4): a fresh dispatch process must observe it.
        record_safety_trip(&chain, SafetyTripKind::KillSwitch, Some("run-x"), "teardown hard-fail", now, 1).unwrap();
        let writer = RunWriter::new(tmp.path(), "run-x").unwrap();
        finalize_session(writer, DataQualityReport::backtest(vec![], vec![]), &report, 0).unwrap();

        let state = chain.load();
        let trip = state.records.iter().any(|r| matches!(&r.body.kind,
            RecordKind::SafetyTrip(t) if t.trip == SafetyTripKind::KillSwitch));
        assert!(trip, "kill-switch trip persisted in the chain");
        assert!(tmp.path().join("runs").join("run-x").exists(), "run finalized despite hard-fail");
        assert!(state.kill_switch_engaged, "the gate would now read the kill switch engaged");
    }

    #[test]
    fn kill_switch_clear_needs_a_fresh_nonce_and_attendance() {
        use crate::dispatch::chain::{DispatchChain, RecordKind, TripAction, SafetyTripKind};
        use crate::dispatch::nonce::OperatorGate;
        let tmp = tempfile::TempDir::new().unwrap();
        let chain = DispatchChain::open(tmp.path()).unwrap();
        let now = Utc.timestamp_opt(1_752_600_000, 0).unwrap();
        chain.append(now, 1, 1, None, RecordKind::Genesis).unwrap();

        // Unattended (no-TTY / CI): refused, nothing appended.
        let unattended = OperatorGate { unattended_marker: Some("CI".into()), nonce: Some("1752600000".into()), now_unix: 1_752_600_000 };
        assert!(clear_kill_switch(&chain, &unattended, "clear", now, 1).is_err());
        let before = chain.load().records.len();

        // Attended + fresh nonce: appends a Clear record.
        let attended = OperatorGate { unattended_marker: None, nonce: Some("1752600000".into()), now_unix: 1_752_600_000 };
        clear_kill_switch(&chain, &attended, "operator cleared after reconcile", now, 1).unwrap();
        let state = chain.load();
        assert_eq!(state.records.len(), before + 1);
        assert!(state.records.iter().any(|r| matches!(&r.body.kind,
            RecordKind::SafetyTrip(t) if t.trip == SafetyTripKind::KillSwitch && t.action == TripAction::Clear)));
    }

    // -----------------------------------------------------------------------
    // U1 (#188) — the single-load dispatch composition root: resolver returns the
    // authoritative date fact AND the mandatory dispatch-date-targeted startup record.
    //
    // Single-load discipline is STRUCTURAL: `resolve_date_fact_and_record` takes an
    // already-loaded `&LoadedCalendar` (it CANNOT load), and `resolve_calendar_for_dispatch`
    // has exactly one `resolve_and_load` call site. The resolver tests inject a fixture-built
    // `LoadedCalendar` directly, so no env is read and the load count is one by construction.
    // -----------------------------------------------------------------------

    /// A short human-shaped authority the token heuristic would pass through — the redacted
    /// startup line must never leak it (mirrors `calendar_composition.rs` SECRET_AUTHORITY).
    const SECRET_AUTHORITY: &str = "Jane Doe / Agreement-7";

    /// The pinned dispatch instant: 2026-07-16 (Thu) 10:00 KST = 01:00 UTC — a KRX weekday,
    /// mid-session, matching the CLI suite's `weekday_ts()`.
    fn dispatch_now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 16, 1, 0, 0).unwrap()
    }

    /// Write a valid snapshot bracketing the pinned dispatch date whose 2026-07-16 row carries
    /// `mid_status`, then load it at `dispatch_now()`. `forward_through` sets the
    /// forward-readiness horizon (drives the `freshness=` token). The authority is
    /// `SECRET_AUTHORITY` so the redaction guard has something to catch.
    fn loaded_fixture(
        dir: &std::path::Path,
        mid_status: nautilus_ls_calendar::schema::DayStatus,
        forward_through: chrono::NaiveDate,
        adoption: CalendarAdoption,
    ) -> nautilus_ls::calendar::LoadedCalendar {
        use nautilus_ls_calendar::schema::{
            Authorization, CalendarScope, Coverage, DayRow, DayStatus, Freshness, Snapshot,
            SourceAvailabilityBound,
        };
        use nautilus_ls_calendar::{compute_artifact_id, compute_calendar_id};
        let d = |y, m, day| chrono::NaiveDate::from_ymd_opt(y, m, day).unwrap();
        let mut snap = Snapshot {
            schema_version: "1.0.0".to_string(),
            artifact_id: String::new(),
            calendar_id: String::new(),
            predecessor_artifact_id: None,
            scope: CalendarScope {
                calendar_name: "KRX domestic equity (SYNTHETIC)".to_string(),
                venue: "XKRX".to_string(),
                instrument_class: "domestic-equity".to_string(),
                timezone: "Asia/Seoul".to_string(),
                synthetic: true,
            },
            authorization: Authorization {
                authorized: true,
                authority: SECRET_AUTHORITY.to_string(),
                granted_at: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
                expires_at: Some(Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap()),
                terminated_at: None,
            },
            coverage: Coverage {
                materialized_from: d(2026, 7, 15),
                materialized_through: d(2026, 7, 17),
                retrospectively_checked_through: d(2026, 7, 17),
                scheduled_closure_evaluated_through: d(2026, 7, 17),
                source_availability: vec![SourceAvailabilityBound {
                    source_id: "s".to_string(),
                    available_from: None,
                    available_through: None,
                }],
            },
            freshness: Freshness {
                evidence_refreshed_at: Utc.with_ymd_and_hms(2026, 7, 16, 0, 0, 0).unwrap(),
                holiday_facts_checked_at: Some(Utc.with_ymd_and_hms(2026, 7, 15, 0, 0, 0).unwrap()),
                full_history_reconciled_at: Some(Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap()),
                forward_readiness_through: Some(forward_through),
                last_incremental_at: Some(Utc.with_ymd_and_hms(2026, 7, 16, 0, 0, 0).unwrap()),
            },
            sources: vec![],
            evidence: vec![],
            alerts: vec![],
            rows: vec![
                DayRow { date: d(2026, 7, 15), status: DayStatus::TradingSession, decisive_evidence: vec![], conflicting_evidence: vec![], alerts: vec![] },
                DayRow { date: d(2026, 7, 16), status: mid_status, decisive_evidence: vec![], conflicting_evidence: vec![], alerts: vec![] },
                DayRow { date: d(2026, 7, 17), status: DayStatus::TradingSession, decisive_evidence: vec![], conflicting_evidence: vec![], alerts: vec![] },
            ],
        };
        snap.artifact_id = compute_artifact_id(&snap);
        snap.calendar_id = compute_calendar_id(&snap);
        let path = dir.join("calendar.json");
        std::fs::write(&path, serde_json::to_vec_pretty(&snap).unwrap()).unwrap();
        nautilus_ls::calendar::resolve_and_load(Some(&path), dispatch_now(), adoption)
    }

    #[test]
    fn u188_shadow_over_fixture_records_but_weekday_stays_authoritative() {
        use nautilus_ls_calendar::schema::DayStatus;
        let dir = tempfile::TempDir::new().unwrap();
        // A Closed calendar row proves the weekday fact stays authoritative under Shadow:
        // the returned fact is the weekday Trading Session, NOT the calendar's Closed.
        let loaded = loaded_fixture(dir.path(), DayStatus::Closed, chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(), CalendarAdoption::Shadow);
        let (fact, rec) = resolve_date_fact_and_record(CalendarAdoption::Shadow, &loaded, dispatch_now());
        assert_eq!(fact, CalendarDateFact::TradingSession, "Shadow keeps the weekday fact authoritative");
        assert_eq!(rec.action, ResultingAction::ShadowRecorded);
        let diag = rec.diagnostic.as_ref().expect("a loaded snapshot carries a diagnostic");
        assert!(diag.artifact_id.is_some() && diag.calendar_id.is_some(), "snapshot identity recorded");
        assert!(diag.coverage.is_some(), "coverage recorded");
        let line = rec.render_line();
        assert!(line.contains("adoption=shadow"), "{line}");
        assert!(line.contains("action=shadow-recorded"), "{line}");
        assert!(line.contains("artifact_id="), "{line}");
    }

    /// U3: the Ladder Shadow arm records a CLASSIFIED, assertable, redacted divergence — a
    /// weekday (weekday-open) date the calendar proves Closed is `CalendarClosedWeekdayOpen`;
    /// Legacy/Enforced record no divergence (Legacy never records, Enforced acts on the fact).
    #[test]
    fn shadow_divergence_is_classified_and_redacted() {
        use nautilus_ls::calendar::DivergenceClass;
        use nautilus_ls_calendar::schema::DayStatus;
        let dir = tempfile::TempDir::new().unwrap();
        // dispatch_now()'s KST date (2026-07-16) is a weekday; the calendar proves it Closed.
        let loaded = loaded_fixture(
            dir.path(),
            DayStatus::Closed,
            chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
            CalendarAdoption::Shadow,
        );
        let obs = ladder_divergence(CalendarAdoption::Shadow, &loaded, dispatch_now())
            .expect("shadow emits a divergence");
        assert_eq!(obs.class, DivergenceClass::CalendarClosedWeekdayOpen);
        assert_eq!(obs.consumer, "lab-live-dispatch");
        assert!(ladder_divergence(CalendarAdoption::Legacy, &loaded, dispatch_now()).is_none());
        assert!(ladder_divergence(CalendarAdoption::Enforced, &loaded, dispatch_now()).is_none());

        let line = obs.render_line();
        assert!(line.contains("class=calendar-closed-weekday-open"), "{line}");
        assert!(!line.to_lowercase().contains("authority"), "{line}");
    }

    #[test]
    fn u188_enforced_trading_session_from_calendar_not_weekday() {
        use nautilus_ls_calendar::schema::DayStatus;
        let dir = tempfile::TempDir::new().unwrap();
        let loaded = loaded_fixture(dir.path(), DayStatus::TradingSession, chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(), CalendarAdoption::Enforced);
        let (fact, rec) = resolve_date_fact_and_record(CalendarAdoption::Enforced, &loaded, dispatch_now());
        assert_eq!(fact, CalendarDateFact::TradingSession);
        assert_eq!(rec.action, ResultingAction::EnforcedActive);
        assert!(rec.render_line().contains("action=enforced-active"));
    }

    #[test]
    fn u188_enforced_closed_from_calendar_fails_and_records_active() {
        use nautilus_ls_calendar::schema::DayStatus;
        let dir = tempfile::TempDir::new().unwrap();
        // 2026-07-16 is a weekday, but the calendar proves it Closed — Enforced returns Closed
        // (the calendar is authoritative), and the record shows the calendar is active.
        let loaded = loaded_fixture(dir.path(), DayStatus::Closed, chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(), CalendarAdoption::Enforced);
        let (fact, rec) = resolve_date_fact_and_record(CalendarAdoption::Enforced, &loaded, dispatch_now());
        assert_eq!(fact, CalendarDateFact::Closed, "Enforced reads the calendar, not the weekday");
        assert_eq!(rec.action, ResultingAction::EnforcedActive);
        let line = rec.render_line();
        assert!(line.contains("day=2026-07-16:Closed"), "{line}");
        assert!(line.contains("action=enforced-active"), "{line}");
    }

    #[test]
    fn u188_enforced_missing_snapshot_is_unavailable_and_fail_closed() {
        let dir = tempfile::TempDir::new().unwrap();
        let missing = dir.path().join("does-not-exist.json");
        let loaded = nautilus_ls::calendar::resolve_and_load(Some(&missing), dispatch_now(), CalendarAdoption::Enforced);
        let (fact, rec) = resolve_date_fact_and_record(CalendarAdoption::Enforced, &loaded, dispatch_now());
        assert_eq!(fact, CalendarDateFact::Unavailable, "no weekday fallback under Enforced");
        assert_eq!(rec.action, ResultingAction::EnforcedFailClosed);
        assert!(rec.render_line().contains("action=enforced-fail-closed"));
    }

    #[test]
    fn u188_legacy_over_fixture_is_weekday_authoritative_and_still_loads() {
        use nautilus_ls_calendar::schema::DayStatus;
        let dir = tempfile::TempDir::new().unwrap();
        let loaded = loaded_fixture(dir.path(), DayStatus::Closed, chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(), CalendarAdoption::Legacy);
        let (fact, rec) = resolve_date_fact_and_record(CalendarAdoption::Legacy, &loaded, dispatch_now());
        assert_eq!(fact, CalendarDateFact::TradingSession, "Legacy weekday fact is authoritative");
        assert_eq!(rec.action, ResultingAction::WeekdayAuthoritative);
        // The calendar is still loaded + recorded (KTD6 uniform composition root).
        assert!(rec.diagnostic.is_some(), "Legacy still loads + records the calendar");
        assert!(rec.render_line().contains("action=weekday-authoritative"));
    }

    #[test]
    fn u188_legacy_load_error_is_non_fatal_and_leaves_weekday_outcome() {
        let dir = tempfile::TempDir::new().unwrap();
        let missing = dir.path().join("does-not-exist.json");
        let loaded = nautilus_ls::calendar::resolve_and_load(Some(&missing), dispatch_now(), CalendarAdoption::Legacy);
        let (fact, rec) = resolve_date_fact_and_record(CalendarAdoption::Legacy, &loaded, dispatch_now());
        assert_eq!(fact, CalendarDateFact::TradingSession, "a Legacy load error cannot alter the weekday outcome");
        assert_eq!(rec.action, ResultingAction::WeekdayAuthoritative);
    }

    #[test]
    fn u188_startup_line_is_redacted_no_authority_leak() {
        use nautilus_ls_calendar::schema::DayStatus;
        let dir = tempfile::TempDir::new().unwrap();
        let loaded = loaded_fixture(dir.path(), DayStatus::TradingSession, chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(), CalendarAdoption::Enforced);
        let (_, rec) = resolve_date_fact_and_record(CalendarAdoption::Enforced, &loaded, dispatch_now());
        let line = rec.render_line();
        assert!(!line.contains(SECRET_AUTHORITY), "authority leaked into the startup line: {line}");
        assert!(!line.contains("Jane Doe"), "{line}");
    }

    #[test]
    fn u188_stub_seam_still_builds_a_record_reflecting_adoption() {
        // The offline Enforced seam (`date_fact_stub`) wins but still yields a record whose
        // action mirrors what an injected calendar would derive — no snapshot is loaded, so
        // the diagnostic renders `snapshot=not-configured`.
        let rec = stub_startup_record(CalendarAdoption::Enforced, CalendarDateFact::TradingSession);
        assert_eq!(rec.action, ResultingAction::EnforcedActive);
        assert!(rec.render_line().contains("snapshot=not-configured"), "no snapshot loaded in stub mode");

        assert_eq!(
            stub_startup_record(CalendarAdoption::Enforced, CalendarDateFact::Unavailable).action,
            ResultingAction::EnforcedFailClosed
        );
        assert_eq!(
            stub_startup_record(CalendarAdoption::Shadow, CalendarDateFact::TradingSession).action,
            ResultingAction::ShadowRecorded
        );
        assert_eq!(
            stub_startup_record(CalendarAdoption::Legacy, CalendarDateFact::Closed).action,
            ResultingAction::WeekdayAuthoritative
        );
    }
}
