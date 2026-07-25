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

use ls_sdk::LsSdk;
use nautilus_ls::execution::OrderDispatchTasks;
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_ls::orders::ledger::{FillDelta, FillLedger};
use nautilus_ls::orders::poll::DrivenOutcome;

use crate::artifacts::data_quality::{DataQualityReport, ReconcileCondition, ReconcileConditionKind};
use crate::artifacts::RunWriter;
use crate::params::OrbParams;
use crate::strategy::orb::EmissionGate;

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

/// The **production** [`LiveSession`] — the concrete, `Send + Sync` teardown handle the
/// live driver and the watchdog share (live-session-driver U1; R1, KTD1/KTD2/KTD4/KTD6).
///
/// Deliberately **not** an [`LsExecClient`](nautilus_ls::execution::LsExecClient): that
/// type is not `Clone` (it owns `JoinHandle`s) and cannot be retrieved after
/// `LiveNode::build()` type-erases it, and its `Send + Sync` would ride on the
/// `ExecutionEventEmitter`/`WsSupervisor` bounds. Instead every field here is
/// `Arc`-shared state captured **before** the builder:
///
/// - `gate` — a clone of the strategy's [`EmissionGate`] (`Arc<AtomicBool>`), taken before
///   `add_strategy` moves the strategy into the trader;
/// - `sdk` — a clone of the **node's** [`LsSdk`], so `halt()` flips the very
///   `Arc<Inner>::orders_enabled` the node's order path checks (KTD3: a separately-built
///   client would halt a *different* `AtomicBool` — a silent no-op on exactly the orders
///   that matter);
/// - `ledger` — the **node's** `Arc<Mutex<FillLedger>>` (a separate `Arc` from the SDK, so
///   `sdk.clone()` does not carry it), which the max-loss breaker feeder reads;
/// - `order_tasks` — the node client's retained order-dispatch tasks, quiesced before the
///   cancel scan (KTD2).
///
/// Ordering is **not** this type's job: [`run_teardown`] owns `stop → cancel → flat →
/// halt` and is reused unchanged.
/// (No `Debug`: neither `LsSdk` nor `FillLedger` implements it, and a derived one would
/// risk printing resolved credential state anyway — this handle is never logged.)
#[derive(Clone)]
pub struct LiveTeardownSession {
    gate: EmissionGate,
    sdk: LsSdk,
    ledger: std::sync::Arc<std::sync::Mutex<FillLedger>>,
    order_tasks: OrderDispatchTasks,
    quiesce_budget: std::time::Duration,
}

impl LiveTeardownSession {
    /// Build the teardown handle from the pieces captured before `LiveNode::build()`.
    pub fn new(
        gate: EmissionGate,
        sdk: LsSdk,
        ledger: std::sync::Arc<std::sync::Mutex<FillLedger>>,
        order_tasks: OrderDispatchTasks,
    ) -> Self {
        LiveTeardownSession {
            gate,
            sdk,
            ledger,
            order_tasks,
            quiesce_budget: nautilus_ls::execution::QUIESCE_BUDGET,
        }
    }

    /// Override the in-flight order-dispatch drain budget (tests drive it to zero so the
    /// quiesce path is exercised without waiting).
    pub fn with_quiesce_budget(mut self, budget: std::time::Duration) -> Self {
        self.quiesce_budget = budget;
        self
    }

    /// The shared fill ledger — what the max-loss breaker feeder reads (KTD3).
    pub fn ledger(&self) -> std::sync::Arc<std::sync::Mutex<FillLedger>> {
        std::sync::Arc::clone(&self.ledger)
    }

    /// A clone of the mounted strategy's emission gate — the same `Arc<AtomicBool>` the
    /// strategy reads before every order it would emit (KTD4).
    pub fn emission_gate(&self) -> EmissionGate {
        self.gate.clone()
    }

    /// The lifetime order-dedup hit count on the shared SDK — a within-TTL identical
    /// re-send or a concurrent-duplicate rejection. A non-zero count on a real emission is
    /// a limit event (ladder R14(d)); the run's data-quality report persists it.
    pub fn dedup_hits(&self) -> u64 {
        self.sdk.inner().order_dedup.hit_count()
    }

    /// Whether the shared kill switch still permits order dispatch. `false` once
    /// [`LiveSession::halt`] has run — read by the wiring tests that prove the node's
    /// in-trader client shares this switch.
    pub fn orders_enabled(&self) -> bool {
        self.sdk.inner().orders_enabled()
    }
}

impl LiveSession for LiveTeardownSession {
    fn stop_emission(&self) {
        self.gate.stop();
    }

    async fn cancel_all_resting(&self) -> anyhow::Result<usize> {
        // KTD2: drain the detached submit/modify/cancel workers FIRST. `stop_emission`
        // only closes the strategy's gate — a submission already in flight would
        // otherwise reach the gateway after the scan below, pass the kill-switch check,
        // and rest an order the scan never saw.
        self.order_tasks.quiesce(self.quiesce_budget).await;
        nautilus_ls::execution::cancel_all_resting_on(&self.sdk)
            .await
            // `LsError` Display carries the broker's `rsp_msg`; scrub before it can reach
            // any record or output line.
            .map_err(|e| anyhow::anyhow!("{}", nautilus_ls::scrub::scrub_secrets(&e.to_string())))
    }

    async fn is_flat(&self) -> bool {
        // KTD1: positive confirmation only. `verify_flat_on` composes t0425 (resting
        // orders) + t0424 (`janqty` holdings), failing closed on truncation/garbage — so a
        // truncated, failed, or ambiguous read is `Err` and reads here as NOT flat.
        nautilus_ls::execution::verify_flat_on(&self.sdk).await.is_ok()
    }

    fn halt(&self) {
        // The shared `Arc<Inner>` kill switch — the same `AtomicBool` `post_order` checks
        // first for every order the node dispatches (KTD3).
        self.sdk.inner().set_orders_enabled(false);
    }
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
    // Scrub the operator reason before it lands (KTD4 clear-reason capture) — clearing an
    // auto-halt kill switch is the CLI's most safety-sensitive mutation and must leave an
    // audited who/why record with no secret in it (mirrors `chain.reregister`'s reason scrub).
    chain.append(
        now,
        chain_rung,
        chain_rung,
        None,
        RecordKind::SafetyTrip(SafetyTrip {
            trip: SafetyTripKind::KillSwitch,
            action: TripAction::Clear,
            run_id: None,
            detail: nautilus_ls::scrub::scrub_secrets(reason),
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
        // The per-consumer adoption posture (U12, KTD8): Enforced-only (#189).
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
    // Enforced-only after the Ladder Consumer Retirement Gate (#189 U9, KTD3): the date gate no
    // longer consults LS_CALENDAR_ADOPTION, and the startup record names the enforced posture.
    // The deterministic offline seam: a stubbed fact is authoritative; still emit a record so the
    // composition-root diagnostic path is exercised (no snapshot loaded → `snapshot=not-configured`).
    if let Some(fact) = cfg.date_fact_stub {
        return (fact, stub_startup_record(CalendarAdoption::Enforced, fact));
    }
    let path = nautilus_ls::calendar::snapshot_path_from_env();
    let loaded = nautilus_ls::calendar::resolve_and_load(path.as_deref(), now_utc, cfg.adoption);
    resolve_date_fact_and_record(CalendarAdoption::Enforced, &loaded, now_utc)
}

/// Derive the authoritative [`CalendarDateFact`] and the dispatch-date-targeted
/// [`StartupRecord`] from ONE already-loaded calendar (KTD2, load-once-derive-twice). Pure
/// and env-free so the resolver tests inject a fixture-built `LoadedCalendar` directly.
///
/// Enforced-only after the Ladder Consumer Retirement Gate (#189 U9, KTD3): the `KrxCalendar`
/// fact from the snapshot is authoritative, or [`CalendarDateFact::Unavailable`] on ANY
/// load/use/query failure (no weekday fallback), never `Unknown`. The weekday date decision is
/// retired; only the time-of-day window (`in_time_window`) survives (KTD7).
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
    // The snapshot fact is authoritative; any load/use/query failure → Unavailable.
    let view = loaded.calendar().and_then(|cal| cal.as_of(now_utc).ok());
    let date_fact = date_fact_from_view(view.as_ref(), kst_date);
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
        Some("--mount") => run_mount(),
        Some("--head") => run_head_diagnostic(),
        Some("--escalate") => run_escalate_cli(),
        Some("--reregister") => run_reregister_cli(),
        Some("--clear-killswitch") => run_clear_killswitch_cli(),
        Some("--rung-report") => run_rung_report(),
        _ => {
            nautilus_ls::calendar::emit_startup_from_env("lab-live");
            if std::env::var("LS_TRADING_ENV").as_deref() != Ok("paper") {
                anyhow::bail!("refusing to run: set LS_TRADING_ENV=paper (this adapter is paper-only)");
            }
            anyhow::bail!(
                "lab-live: run `lab-live --dispatch` for the pre-flight gate (`--genesis` to \
                 register the chain, `--mount` to RUN an attended rung-1 live session — it \
                 consumes the green dispatch, drives the session, and finalizes) — see \
                 adapters/nautilus/lab/RUNG1-PREFLIGHT.md"
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
use nautilus_common::factories::ExecutionClientFactory;
use nautilus_live::node::{LiveNode, LiveNodeHandle};
use nautilus_ls::config::LsAdapterConfig;
use nautilus_ls::factories::{LsDataClientFactory, LsExecutionClientFactory};
use nautilus_ls::ingest::budget::{spend_ledger_path, SpendLedger};
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

/// Serializes `LiveNode::build()` across threads. Nautilus initializes the process-global
/// logger with a non-atomic check-then-set, so two concurrent builds intermittently trip
/// "a non-Nautilus logger is already registered"
/// (`docs/solutions/test-failures/nautilus-livenode-tests-race-on-the-global-logger-init.md`).
/// Poison-tolerant: a panicking build must not wedge every later one.
static NODE_BUILD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Hold the [`NODE_BUILD_LOCK`] across a `LiveNode::build()`. Public so the wiring tests
/// that build their own nodes serialize against the runner's builds.
pub fn node_build_lock() -> std::sync::MutexGuard<'static, ()> {
    NODE_BUILD_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A built live session: the node plus every handle the driver, the watchdog, and the
/// finalize path need — all captured **before** the builder (live-session-driver KTD4).
///
/// After `LiveNode::build()` the exec client is type-erased in `Vec<LiveExecutionClient>`
/// with no downcast, and `add_strategy` moves the strategy into the trader. Neither handle
/// can be retrieved afterwards, so they are taken here or not at all.
pub struct LiveMount {
    /// The built node. `node.run()` is live-only and is never driven by the gate — it is
    /// deliberately kept OUT of [`LiveSessionHandles`] so the driver can be exercised
    /// offline without one.
    pub node: LiveNode,
    /// Everything the driver, the watchdog, and the finalize path need.
    pub handles: LiveSessionHandles,
}

/// The handle set a live session is driven through — captured before the builder (KTD4)
/// and deliberately node-free, so [`run_live_session`] is fully offline-testable.
#[derive(Clone)]
pub struct LiveSessionHandles {
    /// The fail-closed teardown handle, sharing the node's kill switch + fill ledger.
    pub session: LiveTeardownSession,
    /// The dead-man feeders: the strategy touches `runtime`, the watchdog `supervisor`.
    pub heartbeats: Heartbeats,
    /// The node's stop handle, grabbed BEFORE `run` (the session timer and a watchdog
    /// trip both use it to unblock the run loop).
    pub handle: LiveNodeHandle,
    /// The session's decision sink — drained into the run artifacts at finalize.
    pub sink: DecisionSink,
    /// The strategy's published per-symbol market view — the breaker's mark source
    /// (KTD8(b)); the watchdog thread has no market-data access of its own.
    pub marks: MarkFeed,
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
// live-session-driver U3/U4 — the driver that owns `node.run`'s lifecycle (R4, R5;
// KTD5, KTD7, KTD8).
//
// `node.run(&mut self)` blocks the current thread and runs INDEFINITELY: it has no
// session timer and no market-close stop. The caller owns the stop. So the driver:
//
//   1. grabs `node.handle()` BEFORE `run` (KTD4/KTD5 — after `run` there is no way in);
//   2. spawns a session timer that calls `handle.stop()` at the session duration;
//   3. spins the full watchdog envelope on a DEDICATED OS thread with its own
//      current-thread runtime, so a stalled session runtime cannot stall its own
//      remediation (ladder KTD10), and runs the session-side mutual-liveness check on
//      the node runtime so a dead watchdog thread never degrades the envelope silently;
//   4. drives `node.run()` — the single seam never exercised offline, injected as a
//      substitutable closure so every surrounding seam IS;
//   5. runs the fail-closed `run_teardown` afterwards **only if it wins the atomic
//      `TripLatch::try_claim`** — the same compare-exchange the watchdog uses. A
//      non-atomic `is_tripped()` read would race a concurrent watchdog claim and let
//      BOTH paths tear down;
//   6. stages the manifest (with the DispatchLink), the performance from the shared fill
//      ledger, and the drained decisions, then finalizes — abnormally when the teardown
//      hard-failed.
//
// The teardown runs AFTER `node.run`'s own graceful shutdown by design (KTD7): the node
// cancels and drains on stop, but that is not the sticky-kill-switch + positive
// t0424/t0425 confirmation the gate requires. Re-asserting the safety invariant at the
// driver altitude is the point.
// ---------------------------------------------------------------------------

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::runner::pnl::{self, MarkPolicy};
use crate::runner::watchdog::{
    operator_keepalive_unix, session_liveness_tick_reporting, watchdog_tick_reporting,
    TripCause, TripLatch, WatchdogLimits, WatchdogObservation,
};
use crate::strategy::orb::MarkFeed;

/// An injectable wall clock (unix seconds) — tests drive it rather than sleeping.
pub type SessionClock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// The real wall clock.
pub fn system_clock() -> SessionClock {
    Arc::new(|| Utc::now().timestamp())
}

/// The driver's tunables. Everything safety-bearing (`limits`) comes from the frozen
/// pre-registration; the cadences are operational.
#[derive(Debug, Clone)]
pub struct LiveDriverConfig {
    /// Session duration before the timer stops the node.
    pub session_secs: u64,
    /// Watchdog evaluation cadence. Well under the heartbeat interval so a stale feeder
    /// is caught promptly.
    pub watchdog_tick: Duration,
    /// The pre-registered dead-man interval + max-loss threshold (fail-closed armed).
    pub limits: WatchdogLimits,
    /// How the breaker marks open positions when the feed is stale (KTD8(b)).
    pub mark_policy: MarkPolicy,
    /// The operator keepalive file the attended operator refreshes (absent = stale).
    pub keepalive_path: PathBuf,
    /// Cancel/flat retry budgets for the session-end teardown.
    pub cancel_attempts: usize,
    /// Flat-confirmation attempts for the session-end teardown.
    pub flat_attempts: usize,
    /// Starting account balance (KRW) recorded on the equity curve.
    pub starting_balance: f64,
}

/// The identity + artifact context a driven session finalizes under.
#[derive(Debug, Clone)]
pub struct LiveSessionContext {
    /// The data home (registry + chain live here).
    pub data_home: PathBuf,
    /// The run id the consumption marker already recorded (R14(f)).
    pub run_id: String,
    /// The chain-authorized rung safety-trip records are appended at.
    pub chain_rung: u8,
    /// The dispatch↔run linkage threaded into the manifest (KTD3).
    pub dispatch: Option<DispatchLink>,
    /// The governed params the session traded.
    pub params: OrbParams,
    /// The traded universe (instrument-id strings), for the manifest's universe hash.
    pub symbols: Vec<String>,
    /// The KST trading date the run covers (`YYYYMMDD`).
    pub trading_date: String,
    /// The manifest's `created_utc` stamp (RFC-3339). Supplied rather than read from the
    /// clock so a driven session's artifacts are reproducible in tests.
    pub created_utc: String,
}

/// What a driven session finalized as.
#[derive(Debug, Clone)]
pub struct LiveSessionOutcome {
    /// The one fail-closed teardown's report (whichever path claimed it).
    pub report: TeardownReport,
    /// The watchdog/mutual-liveness cause, when a trip (not the session timer) ended it.
    pub trip: Option<TripCause>,
    /// The finalized run directory.
    pub run_dir: PathBuf,
    /// Whether the run finalized ABNORMAL — the teardown could not positively confirm a
    /// flat account, so the kill switch is engaged and an operator must reconcile.
    pub abnormal: bool,
}

/// Assemble one watchdog observation from the live feeders (R5; KTD8). Pure given its
/// inputs, so the breaker's arithmetic is provable offline against scripted fixtures.
pub fn assemble_observation(
    now_unix: i64,
    heartbeats: &Heartbeats,
    keepalive_path: &Path,
    ledger: &std::sync::Mutex<FillLedger>,
    marks: &MarkFeed,
    policy: &MarkPolicy,
) -> WatchdogObservation {
    // KTD8(a): realized P&L is ACCOUNTING over the shared ledger's fill journal, not a
    // sum — the ledger carries no cost basis. KTD8(b): open positions are marked at the
    // adverse edge with a stale-feed floor, never a last-seen favorable price.
    let session = pnl::account_shared(ledger);
    let open_marked =
        pnl::mark_open_pnl(&session.open, &marks.snapshot(), now_unix, policy);
    WatchdogObservation {
        now_unix,
        runtime_heartbeat_unix: heartbeats.runtime_unix(),
        operator_keepalive_unix: operator_keepalive_unix(keepalive_path),
        realized_pnl_krw: session.realized_krw,
        open_marked_pnl_krw: open_marked,
    }
}

/// Everything the watchdog OS thread owns. All `Arc`-shared or owned outright, so the
/// thread needs nothing from the session runtime (ladder KTD10).
struct WatchdogArming {
    session: LiveTeardownSession,
    heartbeats: Heartbeats,
    marks: MarkFeed,
    latch: Arc<TripLatch>,
    node_handle: LiveNodeHandle,
    stop: Arc<AtomicBool>,
    clock: SessionClock,
    data_home: PathBuf,
    keepalive_path: PathBuf,
    limits: WatchdogLimits,
    mark_policy: MarkPolicy,
    tick: Duration,
    run_id: String,
    chain_rung: u8,
}

/// Spin the watchdog on its own OS thread + current-thread runtime (ladder KTD10). On a
/// claimed trip it drives the fail-closed teardown **there** — a stalled session runtime
/// cannot stall its own remediation — and then calls `handle.stop()` to unblock
/// `node.run`. Returns the joinable thread; its value is the trip (cause + report), or
/// `None` if the session ended first.
fn spawn_watchdog(
    arming: WatchdogArming,
) -> std::thread::JoinHandle<anyhow::Result<Option<(TripCause, TeardownReport)>>> {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        // The chain is a directory handle; opening it on this thread keeps the watchdog
        // independent of anything the session runtime owns.
        let chain = DispatchChain::open(&arming.data_home)?;
        rt.block_on(async move {
            loop {
                if arming.stop.load(Ordering::SeqCst) {
                    return Ok(None);
                }
                let now = (arming.clock)();
                // Mutual liveness: the session side reads this to detect a dead watchdog.
                arming.heartbeats.touch_supervisor(now);
                let obs = assemble_observation(
                    now,
                    &arming.heartbeats,
                    &arming.keepalive_path,
                    &arming.session.ledger(),
                    &arming.marks,
                    &arming.mark_policy,
                );
                let tripped = watchdog_tick_reporting(
                    &arming.session,
                    &chain,
                    &arming.latch,
                    &obs,
                    &arming.limits,
                    Some(arming.run_id.as_str()),
                    arming.chain_rung,
                )
                .await;
                match tripped {
                    Ok(Some((cause, report))) => {
                        // The teardown already ran HERE, on this runtime (halt last), and
                        // the cause + kill-switch records are persisted. Unblock
                        // `node.run` so the driver can finalize on this very report.
                        arming.node_handle.stop();
                        return Ok(Some((cause, report)));
                    }
                    Ok(None) => {}
                    Err(e) => {
                        // A chain-append failure must never silently disarm the envelope:
                        // the teardown inside `execute_trip` always runs first, so the
                        // remediation happened. Surface and stop watching.
                        return Err(e);
                    }
                }
                tokio::time::sleep(arming.tick).await;
            }
        })
    })
}

/// Drive one attended live session end-to-end (R4, R5).
///
/// `run_node` is the **only** seam not exercised offline: the live call site passes
/// `move |_| async move { node.run().await }`; tests pass a scripted future. Everything
/// around it — the timer, the watchdog arming, the exactly-one teardown, the staging and
/// the finalize — runs in both.
///
/// # Errors
///
/// A staging/finalize failure, or a watchdog chain-append failure. Note that a *failed
/// teardown* is not an error: it finalizes the run ABNORMAL and is reported on the
/// outcome, because a hard-failed teardown must still leave scannable artifacts (R5).
pub async fn run_live_session<F, Fut>(
    handles: LiveSessionHandles,
    cfg: &LiveDriverConfig,
    ctx: &LiveSessionContext,
    clock: SessionClock,
    run_node: F,
) -> anyhow::Result<LiveSessionOutcome>
where
    F: FnOnce(LiveNodeHandle) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<()>>,
{
    let LiveSessionHandles { session, heartbeats, handle, sink, marks } = handles;
    let latch = Arc::new(TripLatch::new());
    let watchdog_stop = Arc::new(AtomicBool::new(false));

    // (3) The watchdog envelope, on its own thread + runtime.
    let watchdog = spawn_watchdog(WatchdogArming {
        session: session.clone(),
        heartbeats: heartbeats.clone(),
        marks: marks.clone(),
        latch: Arc::clone(&latch),
        node_handle: handle.clone(),
        stop: Arc::clone(&watchdog_stop),
        clock: Arc::clone(&clock),
        data_home: ctx.data_home.clone(),
        keepalive_path: cfg.keepalive_path.clone(),
        limits: cfg.limits,
        mark_policy: cfg.mark_policy,
        tick: cfg.watchdog_tick,
        run_id: ctx.run_id.clone(),
        chain_rung: ctx.chain_rung,
    });

    // (3b) Mutual liveness on the SESSION side: a dead watchdog thread must never
    // silently degrade the envelope to attended-operator-only. Shares the one latch.
    let liveness = tokio::spawn(session_liveness_loop(
        session.clone(),
        heartbeats.clone(),
        Arc::clone(&latch),
        handle.clone(),
        Arc::clone(&clock),
        Arc::clone(&watchdog_stop),
        ctx.data_home.clone(),
        cfg.limits.heartbeat_interval_secs,
        cfg.watchdog_tick,
        ctx.run_id.clone(),
        ctx.chain_rung,
    ));

    // (2) The session timer — `node.run` has none of its own.
    let timer_handle = handle.clone();
    let session_secs = cfg.session_secs;
    let timer = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(session_secs)).await;
        timer_handle.stop();
    });

    // (4) The live-only seam.
    let run_result = run_node(handle.clone()).await;
    timer.abort();

    // Signal both supervisors to stand down BEFORE the driver contends for the claim, then
    // wait for the session-side loop to finish. It is deliberately NOT aborted: aborting a
    // task that had just won the claim would strand a claimed latch with no teardown —
    // the one state this whole design exists to prevent.
    watchdog_stop.store(true, Ordering::SeqCst);
    let liveness_trip = liveness.await.unwrap_or(None);

    // (5) EXACTLY ONE teardown. The atomic claim is the arbiter — a non-atomic
    // `is_tripped()` read here would race the watchdog and let both paths tear down.
    let driver_report = if latch.try_claim() {
        Some(run_teardown(&session, cfg.cancel_attempts, cfg.flat_attempts).await)
    } else {
        None
    };

    // Collect the watchdog's verdict (it returns immediately after a trip; otherwise it
    // observes the stop flag within one tick).
    let trip = tokio::task::spawn_blocking(move || watchdog.join())
        .await
        .map_err(|e| anyhow::anyhow!("watchdog join task: {e}"))?
        .map_err(|_| anyhow::anyhow!("the watchdog thread panicked"))??;

    let (report, cause) = match (driver_report, trip, liveness_trip) {
        // The driver won the claim: the session ended on its own terms.
        (Some(r), _, _) => (r, None),
        // The watchdog won: its teardown is the one that ran.
        (None, Some((cause, r)), _) => (r, Some(cause)),
        // The session-side mutual-liveness check won (a dead watchdog thread).
        (None, None, Some((cause, r))) => (r, Some(cause)),
        // Nobody produced a report yet somebody holds the claim. Fail closed rather than
        // finalize a session with no recorded teardown.
        (None, None, None) => anyhow::bail!(
            "internal: the teardown latch was claimed but no teardown report was produced — \
             refusing to finalize a session with no recorded teardown"
        ),
    };

    // (6) Stage + finalize. ALWAYS — a hard-failed teardown must still leave scannable
    // artifacts (R5); `finalize_session` marks it abnormal.
    let run_dir = stage_and_finalize(&session, &sink, cfg, ctx, &report, run_result)?;

    Ok(LiveSessionOutcome { report, trip: cause, run_dir, abnormal: report.hard_failed() })
}

/// The session-side mutual-liveness loop (ladder KTD10). Runs on the NODE runtime and
/// shares the one [`TripLatch`], so a watchdog trip and this trip together still tear down
/// exactly once.
#[allow(clippy::too_many_arguments)]
async fn session_liveness_loop(
    session: LiveTeardownSession,
    heartbeats: Heartbeats,
    latch: Arc<TripLatch>,
    node_handle: LiveNodeHandle,
    clock: SessionClock,
    stop: Arc<AtomicBool>,
    data_home: PathBuf,
    interval_secs: i64,
    tick: Duration,
    run_id: String,
    chain_rung: u8,
) -> Option<(TripCause, TeardownReport)> {
    let chain = DispatchChain::open(&data_home).ok()?;
    loop {
        if stop.load(Ordering::SeqCst) {
            return None;
        }
        tokio::time::sleep(tick).await;
        // Re-check after the sleep so a stand-down signal is never followed by one more
        // claim attempt.
        if stop.load(Ordering::SeqCst) {
            return None;
        }
        let tripped = session_liveness_tick_reporting(
            &session,
            &chain,
            &latch,
            clock(),
            heartbeats.supervisor_unix(),
            interval_secs,
            Some(run_id.as_str()),
            chain_rung,
        )
        .await;
        if let Ok(Some(trip)) = tripped {
            node_handle.stop();
            return Some(trip);
        }
    }
}

/// Stage the run's manifest (with the dispatch link), the performance assembled from the
/// **shared** fill ledger, and the drained decisions, then finalize (abnormally when the
/// teardown hard-failed).
fn stage_and_finalize(
    session: &LiveTeardownSession,
    sink: &DecisionSink,
    cfg: &LiveDriverConfig,
    ctx: &LiveSessionContext,
    report: &TeardownReport,
    run_result: anyhow::Result<()>,
) -> anyhow::Result<PathBuf> {
    use crate::artifacts::manifest::{universe_hash, DataRange, Manifest};
    use crate::artifacts::performance::PerformanceReport;

    let writer = RunWriter::new(&ctx.data_home, &ctx.run_id)?;
    let ledger = session.ledger();
    let dedup_hits = session.dedup_hits();
    let (trades, approximated) = {
        let guard = ledger.lock().unwrap_or_else(|e| e.into_inner());
        let fills = guard.fills();
        let approximated = fills.iter().filter(|f| f.price_approximated).count() as u64;
        (pnl::session_trades(fills), approximated)
    };
    writer.write_performance(&PerformanceReport::assemble(trades, cfg.starting_balance))?;

    let manifest = Manifest {
        run_id: ctx.run_id.clone(),
        source: RunSource::Live,
        strategy_id: ctx.params.strategy_id.clone(),
        strategy_version: ctx.params.strategy_version,
        params: ctx.params.clone(),
        data_range: DataRange { start: ctx.trading_date.clone(), end: ctx.trading_date.clone() },
        catalog_fingerprint: String::new(),
        universe_hash: universe_hash(&ctx.symbols),
        strategy_code_hash: crate::artifacts::manifest::strategy_code_hash(),
        lab_src_fingerprint: None,
        checkpoint_hash: None,
        universe_metadata_hash: None,
        dispatch: ctx.dispatch.clone(),
        created_utc: ctx.created_utc.clone(),
    };
    writer.write_manifest(&manifest)?;
    writer.write_decisions(&sink.snapshot())?;

    let mut dq = DataQualityReport::backtest(ctx.symbols.clone(), Vec::new());
    dq.price_approximated_fills = approximated;
    if let Err(e) = run_result {
        // The node's own run error is a data-quality observation, not a reason to skip
        // finalize: the teardown already ran and the artifacts must stay scannable.
        dq.observations
            .push(format!("node.run returned an error: {}", nautilus_ls::scrub::scrub_secrets(&e.to_string())));
    }
    finalize_session(writer, dq, report, dedup_hits)
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

// ---------------------------------------------------------------------------
// U2 (rung-1 readiness) — the `lab-live --mount` operator command (R3; KTD4/KTD5/KTD7).
//
// Wires the shipped mount machinery (authorize_mount / build_live_session_node) into a
// reachable CLI, sizing the live strategy at the pre-registered rung fraction from v34's REAL
// governed params. The attended live-session DRIVER (node.run -> fail-closed teardown ->
// finalize) is DEFERRED: no live `LiveSession` adapter (real resting-order cancel / t0425
// flatness / kill-switch halt) is shipped, and authoring one is safety-critical runtime logic
// beyond this plan's wiring scope (the plan's teardown-sequence stop condition). So `--mount`
// resolves every input, BUILDS the node at the real v34 rung-fraction size (proving
// mountability), does a READ-ONLY mountability check, and stops at the driver seam WITHOUT
// consuming the green dispatch — never leaving a consumed-but-unrun dispatch in the chain.
// ---------------------------------------------------------------------------

use serde::Deserialize;

use nautilus_ls::ingest::BarKind;
use nautilus_model::identifiers::InstrumentId;

use crate::strategy::orb::SessionGapPrices;

/// Distinct `--mount` exit codes — never `0`, so a no-TTY shell never mistakes a prepared-but-
/// unrun mount for a completed session (the "never look-like-ran" discipline).
const MOUNT_NOT_PAPER: u8 = 66; // the paper interlock refused (env != paper)
const MOUNT_REFUSED_ATTEND: u8 = 77; // no fresh nonce / no-TTY / no mountable dispatch
/// A fail-closed PRE-CONSUME precheck failed (prereg / fraction / watchdog arming /
/// keepalive / head params / universe / node build). Distinct because it is the one
/// refusal class that is recoverable **and** leaves the green dispatch unconsumed — the
/// operator fixes the input and re-runs `--mount` without a fresh `--dispatch` cycle.
const MOUNT_PRECHECK_FAILED: u8 = 71;
/// The session RAN but finalized ABNORMAL: the fail-closed teardown could not positively
/// confirm a flat account. Never `0` — the operator must reconcile the account and clear
/// the persisted kill switch before the next dispatch.
const MOUNT_ABNORMAL: u8 = 72;

/// One symbol of the resolved live-mount universe. The operator materializes the dispatch
/// lane's daily/t8407 read into `LS_MOUNT_UNIVERSE_FILE` (a JSON array of these); `SelectedSymbol`
/// itself is not deserializable (it carries nautilus `InstrumentId`/`BarType`). Distinct from the
/// offline test path, which builds `SelectedSymbol`s directly.
#[derive(Debug, Clone, Deserialize)]
struct MountUniverseSymbol {
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
    let rows: Vec<MountUniverseSymbol> =
        serde_json::from_slice(bytes).map_err(|e| anyhow::anyhow!("parsing mount universe: {e}"))?;
    if rows.is_empty() {
        anyhow::bail!("mount refused: the resolved universe (LS_MOUNT_UNIVERSE_FILE) is empty");
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
fn run_mount() -> anyhow::Result<ExitCode> {
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
    let ctx = LiveSessionContext {
        data_home: data_home.clone(),
        run_id: auth.run_id.clone(),
        chain_rung: auth.chain_rung,
        dispatch: Some(auth.dispatch_link()),
        params,
        symbols,
        trading_date: today.clone(),
        created_utc: now.to_rfc3339(),
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
    if outcome.abnormal {
        eprintln!(
            "mount ABNORMAL: the teardown could not positively confirm a flat account — the kill \
             switch is engaged and its record reds the next --dispatch. Reconcile the account, then \
             clear it with `lab-live --clear-killswitch` (nonce-gated). See lab/RUNBOOK-rung1.md."
        );
        return Ok(ExitCode::from(MOUNT_ABNORMAL));
    }
    Ok(ExitCode::SUCCESS)
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
    let universe = parse_mount_universe(&std::fs::read(&inputs.universe_path).map_err(|e| {
        anyhow::anyhow!("reading the mount universe {}: {e}", inputs.universe_path.display())
    })?)?;
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
        watchdog_tick: Duration::from_secs(inputs.watchdog_tick_secs.max(1)),
        limits,
        mark_policy: MarkPolicy::default(),
        keepalive_path: inputs.keepalive_path.clone(),
        cancel_attempts: TEARDOWN_CANCEL_ATTEMPTS,
        flat_attempts: TEARDOWN_FLAT_ATTEMPTS,
        starting_balance: inputs.starting_balance,
    };
    Ok(PreparedMount { mount, driver, params, symbols, fraction, lane_hash })
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Default attended session length — one KRX continuous session with headroom. Overridden
/// by `LS_MOUNT_SESSION_SECS`.
const DEFAULT_SESSION_SECS: u64 = 6 * 60 * 60;
/// Default watchdog cadence — well under any sane pre-registered heartbeat interval, so a
/// stale feeder is caught promptly rather than one interval late.
const DEFAULT_WATCHDOG_TICK_SECS: u64 = 5;
/// Retry budgets for the session-end teardown (the watchdog path has its own).
const TEARDOWN_CANCEL_ATTEMPTS: usize = 3;
const TEARDOWN_FLAT_ATTEMPTS: usize = 3;
/// Recorded on the equity curve when the operator does not supply the account balance.
const DEFAULT_STARTING_BALANCE: f64 = 10_000_000.0;

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
fn operator_gate_from_env(now_unix: i64) -> OperatorGate {
    OperatorGate {
        unattended_marker: detect_unattended_marker(),
        nonce: std::env::var("LS_DISPATCH_NONCE").ok().filter(|s| !s.trim().is_empty()),
        now_unix,
    }
}

/// `--head` (R2): print the running binary's head identity as verbatim fact lines. Read-only, no
/// nonce, no chain append. `strategy_code_hash()` is the SOLE head discriminator — the operator
/// confirms the binary embeds v34 by hash-equality against the documented `d7a9820b…`. The printed
/// `governed_params_hash(&OrbParams::default())` is a version-invariant constant (identical across
/// v9…v34, KTD7), so it does NOT confirm v34's governed values; it is labeled as such, never as a
/// version readout (the binary carries no hash→version map).
fn run_head_diagnostic() -> anyhow::Result<ExitCode> {
    nautilus_ls::calendar::emit_startup_from_env("lab-live");
    let code_hash = crate::artifacts::manifest::strategy_code_hash();
    let params_hash = crate::dispatch::ladder::governed_params_hash(&OrbParams::default());
    println!("head strategy_code_hash={code_hash}");
    println!(
        "head governed_params_hash(default)={params_hash} [version-invariant constant — NOT a v34 confirmation]"
    );
    println!(
        "head-check: the binary embeds v34 IFF strategy_code_hash == the documented head d7a9820b… \
         (the sole discriminator; the binary carries no hash→version map)"
    );
    Ok(ExitCode::SUCCESS)
}

/// `--escalate` (R4): nonce-gated escalation over `run_escalation` — prints the appended
/// escalation evidence or the blocking reason (AE5 of the ladder plan); a refusal/block is a
/// distinct non-zero exit.
fn run_escalate_cli() -> anyhow::Result<ExitCode> {
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
fn run_reregister_cli() -> anyhow::Result<ExitCode> {
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
fn run_clear_killswitch_cli() -> anyhow::Result<ExitCode> {
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
fn run_rung_report() -> anyhow::Result<ExitCode> {
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
        "rung-report head_code_hash={} (v34 IFF == d7a9820b…) head_params_hash={}",
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

    #[test]
    fn kill_switch_clear_scrubs_the_reason(){
        // KTD4 clear-reason capture: a planted secret in the reason never lands in the chain.
        use crate::dispatch::chain::{DispatchChain, RecordKind};
        use crate::dispatch::nonce::OperatorGate;
        let tmp = tempfile::TempDir::new().unwrap();
        let chain = DispatchChain::open(tmp.path()).unwrap();
        let now = Utc.timestamp_opt(1_752_600_000, 0).unwrap();
        chain.append(now, 1, 1, None, RecordKind::Genesis).unwrap();
        let attended = OperatorGate { unattended_marker: None, nonce: Some("1752600000".into()), now_unix: 1_752_600_000 };
        clear_kill_switch(&chain, &attended, "cleared after reconcile on acct 20187511401", now, 1).unwrap();
        let bytes = std::fs::read_to_string(chain.chain_path()).unwrap();
        assert!(!bytes.contains("20187511401"), "the kill-switch clear reason is scrubbed: {bytes}");
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

    // (The Shadow byte-identical + Shadow-divergence-classification tests were retired with the
    //  Ladder Enforced-only cutover — the date gate no longer has a Legacy/Shadow path.)

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

    // (The Legacy weekday-authoritative tests were retired with the Ladder Enforced-only cutover.)

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
    }
}
