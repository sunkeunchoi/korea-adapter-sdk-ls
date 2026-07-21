//! `budget-probe` — the attended, staged IGW00201 budget measurement (U4).
//!
//! Paper-only, operator-run, one gentle credential. It measures the IGW00201
//! budget model on a spare paper lane so the ingest can plan against real numbers
//! instead of the guessed 120s/day-ish model. Every call is counted against a hard
//! per-session ceiling (R5, blocking-risk protection enforced in code), pacing never
//! exceeds the published per-second caps, and each dispatch is recorded in the shared
//! spend ledger. The stage logic, classifier, ceiling, and report live in
//! [`nautilus_ls::ingest::budget`] (offline-tested); this binary wires them to live
//! SDK reads and writes the JSON report the operator promotes into
//! `lab/config/gateway-budget.json`.
//!
//! Stages (`LS_PROBE_STAGES`, default `0,1`):
//! - **0** scope: one spare-key call while the domestic key is exhausted → served =
//!   per-credential (AE1), throttled = broader-than-credential (AE2).
//! - **1** cold budget: a `t8412` minute-chart read (the TR the minute ingest
//!   drives) at a gentle pace until the first IGW00201.
//! - **2** refill: single calls at widening intervals until one serves again.
//! - **3** cross-class: one Account-bucket call post-exhaustion (does it span classes?).
//!
//! Configuration (env vars):
//! - `LS_TRADING_ENV=paper` (required — refuses otherwise).
//! - `LS_PROBE_LANE_FILE`: spare-lane env-file (else the process env is used).
//! - `LS_PROBE_STAGES`: comma-separated stage list (default `0,1`).
//! - `LS_PROBE_CEILING`: hard per-session call ceiling (default `40`, R5).
//! - `LS_PROBE_SYMBOL`: the MarketData probe shcode (default `005930`).
//! - `LS_PROBE_SDATE` / `LS_PROBE_EDATE`: the t8412 trading-day range (YYYYMMDD).
//!   Default = the most recent proven KRX Trading Session from the calendar (U13, KTD8); the
//!   probe refuses (no live call) when none can be proven and no explicit range is supplied.
//!   Setting either endpoint is an explicit BYPASS — a servable known-data day for
//!   reproducibility/recovery when automatic selection can prove no session; the probe only
//!   needs the call to serve. The snapshot is chosen by `LS_CALENDAR_SNAPSHOT` (composition
//!   root).
//! - `LS_PROBE_PACE_MS`: inter-call pace in ms for stage 1 (default `1000`; must stay
//!   at or under the published per-second cap — **t8412 is 1/s, so keep `>=1000`**;
//!   a faster pace would trip t8412's per-second cap and confound the measurement).
//! - `LS_PROBE_CATALOG`: catalog dir the spend ledger derives from (default `catalog`).
//! - `LS_PROBE_OUT`: report path (default `probes/budget-model-report.json`).
//! - `LS_PROBE_REFILL_SAMPLES`: comma-separated widening intervals for stage 2
//!   (seconds, default `30,60,120,240`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{Duration, NaiveDate};
use nautilus_ls_calendar::schema::DayStatus;
use nautilus_ls_calendar::{AsOfView, CalendarAdoption};

use nautilus_ls::config::LsAdapterConfig;
use nautilus_ls::ingest::budget::{
    self, spend_ledger_path, BucketScope, CallCeiling, CrossClassReport, ProbeCaller, ProbeReport,
    RefillReport, SdkProbeCaller, SpendLedger, Stage0Report, StageStop,
};
use nautilus_ls::scrub;

const DEFAULT_CEILING: usize = 40;
const DEFAULT_SYMBOL: &str = "005930";
const DEFAULT_PACE_MS: u64 = 1000;
const DEFAULT_OUT: &str = "probes/budget-model-report.json";
const DEFAULT_STAGES: &str = "0,1";
const DEFAULT_REFILL_SAMPLES: &str = "30,60,120,240";

#[tokio::main]
async fn main() -> std::process::ExitCode {
    // Mandatory credential hygiene before any output (repo pattern for
    // credential-touching binaries).
    scrub::install();
    // The mandatory startup calendar record is now emitted inside `run` from the SAME
    // single per-invocation load that drives the probe-date decision (U1/KTD1) — targeting
    // the probe anchor rather than today's KST date. It fires before the paper gate and the
    // fallible env parses, preserving the always-emit invariant on every path.
    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", scrub::scrub_secrets(&e.to_string()));
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    // U1 (KTD1): resolve the per-invocation calendar context ONCE — one explicit snapshot
    // path, one as-of instant, at most one immutable calendar — and emit the mandatory,
    // decision-relevant startup record from it, BEFORE the paper gate and the fallible env
    // parses. This preserves the always-emit invariant (a non-paper or parse-error
    // invocation still emits exactly one startup record) while collapsing the former
    // double load (main startup + resolve_probe_dates) into a single load and as-of.
    //
    // U13/U8 (KTD8/KTD3): budget-probe is Enforced-only after its Consumer Retirement Gate —
    // it selects the most recent proven Trading Session from the calendar and refuses (no live
    // call) when none can be proven and no explicit LS_PROBE_SDATE/EDATE range is supplied. An
    // explicit range is an auditable BYPASS (reproducibility/recovery).
    let explicit = ExplicitRange {
        sdate: std::env::var("LS_PROBE_SDATE").ok().filter(|s| !s.trim().is_empty()),
        edate: std::env::var("LS_PROBE_EDATE").ok().filter(|s| !s.trim().is_empty()),
    };
    let ctx = ProbeContext::resolve(&explicit);

    if !paper_ok(std::env::var("LS_TRADING_ENV").ok().as_deref()) {
        return Err("refusing to run: set LS_TRADING_ENV=paper (this probe is paper-only)".into());
    }

    let stages = parse_stages(&std::env::var("LS_PROBE_STAGES").unwrap_or_else(|_| DEFAULT_STAGES.into()))?;
    let ceiling_n: usize = parse_env("LS_PROBE_CEILING", DEFAULT_CEILING)?;
    let symbol = std::env::var("LS_PROBE_SYMBOL").unwrap_or_else(|_| DEFAULT_SYMBOL.into());
    let (sdate, edate) = ctx.resolved_range()?;
    let pace_ms: u64 = parse_env("LS_PROBE_PACE_MS", DEFAULT_PACE_MS)?;
    let pace = std::time::Duration::from_millis(pace_ms);
    let out = std::env::var("LS_PROBE_OUT").unwrap_or_else(|_| DEFAULT_OUT.into());
    let catalog: PathBuf = std::env::var("LS_PROBE_CATALOG").unwrap_or_else(|_| "catalog".into()).into();
    let refill_samples = parse_i64_list(
        &std::env::var("LS_PROBE_REFILL_SAMPLES").unwrap_or_else(|_| DEFAULT_REFILL_SAMPLES.into()),
    )?;

    let adapter_cfg = match std::env::var("LS_PROBE_LANE_FILE") {
        Ok(path) => LsAdapterConfig::from_lane_file(path),
        Err(_) => LsAdapterConfig::from_env(),
    };
    // The SDK build resolves the (paper-only) credentials; the appkey never leaves
    // this process except as its SHA-256 ledger key.
    let sdk = adapter_cfg.build_sdk()?;
    let cred_hash = SpendLedger::hash_appkey(&sdk.inner().config.appkey);

    let ledger_path = spend_ledger_path(&catalog);
    let ledger = Arc::new(Mutex::new(SpendLedger::load(&ledger_path)));
    let caller = SdkProbeCaller::new(
        sdk,
        Arc::clone(&ledger),
        cred_hash.clone(),
        symbol.clone(),
        sdate.clone(),
        edate.clone(),
    );

    let mut ceiling = CallCeiling::new(ceiling_n);
    let mut report = ProbeReport {
        probed_at: chrono::Utc::now().to_rfc3339(),
        ceiling: ceiling_n,
        total_calls: 0,
        stage0_scope: None,
        stage1_cold_budget: None,
        stage2_refill: None,
        stage3_cross_class: None,
        notes: Vec::new(),
    };

    for stage in &stages {
        match stage {
            0 => {
                if !ceiling.try_reserve() {
                    report.notes.push("stage 0 skipped: call ceiling reached (R5)".to_string());
                    continue;
                }
                let verdict = budget::classify_call(&caller.market_data_call().await);
                let scope = budget::scope_from_stage0(&verdict);
                println!(
                    "stage 0 (scope): {} → {:?}",
                    scrub::scrub_secrets(&format!("{verdict:?}")),
                    scope
                );
                if scope == BucketScope::BroaderThanCredential {
                    // AE2: stop same-day probing of the SHARED budget; later stages
                    // must re-plan onto cold windows. Do not keep burning.
                    report.notes.push(
                        "AE2: stage 0 tripped IGW00201 — bucket is broader than credential; \
                         STOPPING same-day probing. Re-plan stages 1–3 onto cold-window scheduling."
                            .to_string(),
                    );
                    report.stage0_scope = Some(Stage0Report { verdict, scope });
                    break;
                }
                report.stage0_scope = Some(Stage0Report { verdict, scope });
            }
            1 => {
                let cold = budget::measure_cold_budget(&caller, &mut ceiling, pace).await;
                println!(
                    "stage 1 (cold budget): {} calls served, stopped={:?}",
                    cold.calls_served, cold.stopped
                );
                if cold.stopped == StageStop::Ceiling {
                    report.notes.push(
                        "stage 1 stopped at the call ceiling before an IGW00201 — the cold budget \
                         is larger than the ceiling; raise LS_PROBE_CEILING on a fresh cold window \
                         to measure it fully (R5 left this axis a lower bound)."
                            .to_string(),
                    );
                }
                report.stage1_cold_budget = Some(cold);
            }
            2 => {
                let refill = measure_refill(&caller, &mut ceiling, &refill_samples).await;
                println!(
                    "stage 2 (refill): first success after {:?}s (samples {:?})",
                    refill.first_success_secs, refill.sample_intervals_secs
                );
                report.stage2_refill = Some(refill);
            }
            3 => {
                if !ceiling.try_reserve() {
                    report.notes.push("stage 3 skipped: call ceiling reached (R5)".to_string());
                    continue;
                }
                let verdict = budget::classify_call(&caller.other_class_call().await);
                let spans = matches!(verdict, budget::CallVerdict::Throttled);
                println!(
                    "stage 3 (cross-class): {} (spans classes: {spans})",
                    scrub::scrub_secrets(&format!("{verdict:?}"))
                );
                report.stage3_cross_class = Some(CrossClassReport { verdict, spans_classes: spans });
            }
            other => return Err(format!("unknown probe stage {other} (want 0..=3)").into()),
        }
    }

    report.total_calls = ceiling.made();

    // Persist the probe's spend so it counts against the shared budget window.
    if let Ok(l) = ledger.lock() {
        if let Err(e) = l.save(&ledger_path) {
            eprintln!("warning: {}", scrub::scrub_secrets(&format!("failed to persist spend ledger: {e}")));
        }
    }

    // Write the report (scrubbed, though it carries only codes/counts/timestamps).
    let json = scrub::scrub_secrets(&report.to_pretty_json());
    if let Some(parent) = Path::new(&out).parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }
    std::fs::write(&out, format!("{json}\n")).map_err(|e| format!("writing {out}: {e}"))?;
    println!(
        "probe complete: {} calls this session (ceiling {}) → {out}",
        report.total_calls, ceiling_n
    );
    println!("promote the measured numbers into lab/config/gateway-budget.json (U6).");
    Ok(())
}

/// Stage 2: single calls at widening intervals until one serves again, recording
/// the cumulative seconds to the first success (R3). Refuses to call past the
/// ceiling (R5); an all-throttle sweep records `None` (defer, don't re-burn).
async fn measure_refill<C: budget::ProbeCaller + ?Sized>(
    caller: &C,
    ceiling: &mut CallCeiling,
    samples: &[i64],
) -> RefillReport {
    let mut cumulative = 0i64;
    let mut tried: Vec<i64> = Vec::new();
    for &interval in samples {
        if !ceiling.try_reserve() {
            break;
        }
        tried.push(interval);
        cumulative += interval;
        if interval > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(interval as u64)).await;
        }
        if matches!(budget::classify_call(&caller.market_data_call().await), budget::CallVerdict::Served) {
            return RefillReport { first_success_secs: Some(cumulative), sample_intervals_secs: tried };
        }
    }
    RefillReport { first_success_secs: None, sample_intervals_secs: tried }
}

/// The paper-only interlock predicate (pure, testable).
fn paper_ok(env: Option<&str>) -> bool {
    env == Some("paper")
}

// ---------------------------------------------------------------------------
// U13 (KTD8) — calendar-backed default-date selection (Enforced-only, #189).
//
// The date selection is split into two side-effect-free, unit-testable pieces:
//   * `scan_recent_session` walks a loaded `KrxCalendar` view back from the anchor
//     over a bounded lookback and yields the most recent PROVEN Trading Session
//     (skipping Closed AND Unknown), or a `NoSession` / `Unavailable` outcome.
//   * `plan_probe_dates` is a pure decision over (scan, weekday anchor,
//     explicit override) → the selected range, warnings, bypass record, and WHETHER
//     a live request is attempted. No calendar, no gateway — fully testable.
// `ProbeContext` is the composition-root glue (U1): it reads env, builds the view from ONE
// per-invocation load, plans the range, emits the mandatory decision-relevant startup
// record and the calendar-decision diagnostics to the non-persisted channel, and hands the
// range (or a clean refusal when Enforced can prove nothing and has no bypass) to `run`.
// ---------------------------------------------------------------------------

/// How far back from the anchor `scan_recent_session` looks for a proven Trading Session
/// before giving up (a holiday cluster plus surrounding Unknown weekdays fits well inside
/// a month). Bounded so a far-future anchor (a real KST "today" past the fixture window)
/// never scans unboundedly.
const PROBE_LOOKBACK_DAYS: i64 = 30;

/// The most-recent-proven-Trading-Session scan outcome. `Copy` so a
/// caller can inspect it and still pass it to [`plan_probe_dates`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionScan {
    /// A proven Trading Session at/before the anchor within the lookback. `stale` = the
    /// calendar's evidence is stale at the as-of instant (usable, but warn).
    Session { date: NaiveDate, stale: bool },
    /// The calendar was consulted but no proven Trading Session sits in the bounded
    /// lookback (every date was Closed or Unknown).
    NoSession,
    /// No calendar view was injected (missing/failed snapshot, or an unauthorized/expired
    /// grant) — nothing could be proven.
    Unavailable,
}

/// Where the resolved probe range came from — recorded to the diagnostic channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DateSource {
    /// The calendar-selected most-recent proven Trading Session (Enforced).
    CalendarSession,
    /// An explicit `LS_PROBE_SDATE`/`EDATE` override — a bypass, not a calendar default.
    Bypass,
    /// The calendar could prove no session and no explicit range was supplied → no default.
    NoDefault,
}

impl DateSource {
    /// The stable token used in the diagnostic line.
    fn token(self) -> &'static str {
        match self {
            DateSource::CalendarSession => "calendar-session",
            DateSource::Bypass => "bypass",
            DateSource::NoDefault => "no-default",
        }
    }
}

/// The operator's explicit probe range override (`LS_PROBE_SDATE`/`LS_PROBE_EDATE`). Either
/// half present makes the range an explicit BYPASS of the calendar/weekday default.
#[derive(Debug, Clone, Default)]
struct ExplicitRange {
    sdate: Option<String>,
    edate: Option<String>,
}

impl ExplicitRange {
    /// `true` iff the operator supplied either endpoint — the bypass trigger.
    fn is_supplied(&self) -> bool {
        self.sdate.is_some() || self.edate.is_some()
    }
}

/// The resolved probe-date plan: the range to use (`None` when Enforced refuses), whether a
/// live request may be attempted, the range source, the recorded calendar default, any
/// non-fatal warnings, and — on an explicit-range bypass — the bypass audit — all
/// inspectable without a live gateway.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProbeDatePlan {
    sdate: Option<String>,
    edate: Option<String>,
    live_request: bool,
    source: DateSource,
    /// The calendar-selected session date, recorded for the diagnostic and used by the
    /// Enforced calendar. `None` when no session was proven.
    calendar_default: Option<String>,
    warnings: Vec<String>,
    /// The bypass audit — `Some` iff the operator supplied an explicit `LS_PROBE_SDATE`/
    /// `EDATE` range (U2, KTD4). A pure, inspectable record of WHO forced the range, the run
    /// context, and the calendar condition automatic selection skipped. Recorded on EVERY
    /// adoption; it never changes probe status or authorizes dispatch.
    bypass_audit: Option<BypassAudit>,
}

/// The calendar condition automatic selection SKIPPED when the operator forced an explicit
/// range (U2, KTD4). Derived purely from the recent-session scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BypassedCondition {
    /// The calendar proved no Trading Session in the bounded lookback.
    NoProvenSession,
    /// No usable calendar was injected (missing / failed / unauthorized / expired snapshot).
    Unavailable,
    /// A Trading Session WAS proven, but the operator's explicit range overrode it.
    ProvenSessionNotSelected,
}

impl BypassedCondition {
    /// The stable token used in the audit line.
    fn token(self) -> &'static str {
        match self {
            BypassedCondition::NoProvenSession => "no-proven-session",
            BypassedCondition::Unavailable => "unavailable",
            BypassedCondition::ProvenSessionNotSelected => "proven-session-not-selected",
        }
    }

    /// The condition automatic selection would have hit for `scan`.
    fn from_scan(scan: SessionScan) -> Self {
        match scan {
            SessionScan::Session { .. } => BypassedCondition::ProvenSessionNotSelected,
            SessionScan::NoSession => BypassedCondition::NoProvenSession,
            SessionScan::Unavailable => BypassedCondition::Unavailable,
        }
    }
}

/// The redacted run context threaded into the bypass audit at the composition root (U2,
/// KTD4): the operator identity and a run id, both SANITIZED before they reach the
/// non-persisted diagnostic channel so an operator-supplied value cannot forge a second
/// diagnostic/startup line. Pure data — carried into [`plan_probe_dates`], never read from
/// env there.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct RunContext {
    /// The sanitized operator identity (control chars stripped, length-bounded, scrubbed).
    operator: String,
    /// The sanitized, injectable/seeded run id (no wall-clock / random dependency).
    run_id: String,
}

impl RunContext {
    /// Resolve the run context at the composition root: operator from `LS_PROBE_OPERATOR`
    /// (else `USER`/`LOGNAME`, else `unknown`), run id from `LS_PROBE_RUN_ID` (else the
    /// process id — deterministic, no wall-clock/random) — each sanitized before use.
    fn from_env() -> Self {
        let operator = std::env::var("LS_PROBE_OPERATOR")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| std::env::var("USER").ok().filter(|s| !s.trim().is_empty()))
            .or_else(|| std::env::var("LOGNAME").ok().filter(|s| !s.trim().is_empty()))
            .unwrap_or_else(|| "unknown".to_string());
        let run_id = std::env::var("LS_PROBE_RUN_ID")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("pid-{}", std::process::id()));
        Self {
            operator: sanitize_audit_field(&operator),
            run_id: sanitize_audit_field(&run_id),
        }
    }
}

/// Sanitize a free-text audit field before it reaches the non-persisted diagnostic channel
/// (U2, KTD4/KTD8): strip control characters and newlines (so a value cannot inject a second
/// diagnostic or startup line), bound the length, and route the remainder through the
/// credential scrub. Redacted-by-construction, matching the diagnostic builders.
fn sanitize_audit_field(raw: &str) -> String {
    const MAX_LEN: usize = 96;
    // `char::is_control()` only covers the Unicode Cc category — it does NOT strip the
    // line/paragraph separators U+2028/U+2029 (Zl/Zp), which a Unicode-aware log consumer
    // renders as a line break. Strip those explicitly too so an operator-supplied value
    // cannot forge a second diagnostic/startup line on any consumer (NEL U+0085 is Cc, so
    // `is_control()` already catches it).
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control() && !matches!(*c, '\u{2028}' | '\u{2029}'))
        .take(MAX_LEN)
        .collect();
    let scrubbed = scrub::scrub_secrets(cleaned.trim());
    if scrubbed.is_empty() {
        "unknown".to_string()
    } else {
        scrubbed
    }
}

/// The bypass audit (U2, KTD4): a pure, inspectable record that an explicit range bypassed
/// automatic selection. It records WHO, the run context, and the calendar condition skipped —
/// and NOTHING that changes probe status or authorizes dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BypassAudit {
    operator: String,
    run_id: String,
    condition: BypassedCondition,
}

impl BypassAudit {
    /// Render the audit as ONE line for the non-persisted diagnostic channel. Every field is
    /// already sanitized; the line explicitly disclaims any status/dispatch effect.
    fn render_line(&self) -> String {
        format!(
            "explicit LS_PROBE_SDATE/EDATE range → BYPASS operator={} run_id={} bypassed={} \
             (audit only — probe status unchanged, no dispatch authorization)",
            self.operator,
            self.run_id,
            self.condition.token()
        )
    }
}

/// Format a civil date as the gateway's `YYYYMMDD`.
fn fmt_ymd(date: NaiveDate) -> String {
    date.format("%Y%m%d").to_string()
}

/// Walk `view` back from `anchor` up to [`PROBE_LOOKBACK_DAYS`] (clamped to the materialized
/// coverage) and return the most recent PROVEN Trading Session — skipping Closed AND Unknown
/// dates (an Unknown never manufactures a servable default). A missing view is
/// [`Unavailable`](SessionScan::Unavailable); an exhausted lookback with no proven session is
/// [`NoSession`](SessionScan::NoSession). The value the Enforced calendar acts on;
/// side-effect-free.
fn scan_recent_session(view: Option<&AsOfView<'_>>, anchor: NaiveDate) -> SessionScan {
    let view = match view {
        Some(v) => v,
        None => return SessionScan::Unavailable,
    };
    let stale = view.freshness().any_stale();
    let coverage = view.calendar().coverage();
    let floor = coverage.materialized_from;
    // Clamp the scan start into the window (an out-of-window anchor cannot itself be a
    // proven session); bound the walk to the lookback horizon computed from the raw anchor
    // so a far-future anchor simply proves nothing recent.
    let limit = anchor - Duration::days(PROBE_LOOKBACK_DAYS);
    let mut d = anchor.min(coverage.materialized_through);
    while d >= floor && d >= limit {
        // A materialized in-window date always resolves; a query error (out-of-range) is
        // treated as non-evidence and the walk continues — it never fabricates a session.
        if let Ok(fact) = view.day(d) {
            if fact.status == DayStatus::TradingSession {
                return SessionScan::Session { date: d, stale };
            }
        }
        d = match d.pred_opt() {
            Some(p) => p,
            None => break,
        };
    }
    SessionScan::NoSession
}

/// The pure default-date decision (U13/U2/U8, KTD8/KTD4/KTD3). Enforced-only after the
/// budget-probe Consumer Retirement Gate: automatic selection is the most recent proven Trading
/// Session, or a refusal (no live call) when nothing is proven and no explicit range is
/// supplied. An explicit range is always a BYPASS — recorded as a [`BypassAudit`] naming the
/// (pre-sanitized) operator + run context and the calendar condition automatic selection
/// skipped. The bypass NEVER changes probe status or authorizes dispatch. No calendar, no
/// I/O, no wall-clock — fully testable.
fn plan_probe_dates(scan: SessionScan, explicit: &ExplicitRange, run: &RunContext) -> ProbeDatePlan {
    // The calendar-selected session, recorded for the diagnostic (used when no explicit range
    // bypasses it).
    let calendar_default = match scan {
        SessionScan::Session { date, .. } => Some(fmt_ymd(date)),
        _ => None,
    };
    let bypass = explicit.is_supplied();
    // On a bypass, record WHICH calendar condition automatic selection skipped — derived purely
    // from the scan. Absent when no explicit range.
    let bypass_audit = if bypass {
        Some(BypassAudit {
            operator: run.operator.clone(),
            run_id: run.run_id.clone(),
            condition: BypassedCondition::from_scan(scan),
        })
    } else {
        None
    };

    if bypass {
        // An explicit range wins — it unblocks the live call automatic selection would refuse.
        // A partial range fills the missing endpoint from the supplied one (single-day probe);
        // the weekday anchor is gone (KTD3), so there is no weekday fallback.
        let sdate = explicit.sdate.clone().or_else(|| explicit.edate.clone()).unwrap_or_default();
        let edate = explicit.edate.clone().or_else(|| explicit.sdate.clone()).unwrap_or_default();
        return ProbeDatePlan {
            sdate: Some(sdate),
            edate: Some(edate),
            live_request: true,
            source: DateSource::Bypass,
            calendar_default,
            warnings: Vec::new(),
            bypass_audit,
        };
    }

    match scan {
        SessionScan::Session { date, stale } => {
            let mut warnings = Vec::new();
            if stale {
                warnings.push(format!(
                    "stale-but-established calendar evidence — using proven session {} anyway \
                     (refresh the snapshot)",
                    fmt_ymd(date)
                ));
            }
            let d = fmt_ymd(date);
            ProbeDatePlan {
                sdate: Some(d.clone()),
                edate: Some(d),
                live_request: true,
                source: DateSource::CalendarSession,
                calendar_default,
                warnings,
                bypass_audit: None,
            }
        }
        SessionScan::NoSession => ProbeDatePlan {
            sdate: None,
            edate: None,
            live_request: false,
            source: DateSource::NoDefault,
            calendar_default: None,
            warnings: vec![
                "no proven Trading Session in the calendar lookback — NO live call until an \
                 explicit LS_PROBE_SDATE/EDATE range is supplied"
                    .to_string(),
            ],
            bypass_audit: None,
        },
        SessionScan::Unavailable => ProbeDatePlan {
            sdate: None,
            edate: None,
            live_request: false,
            source: DateSource::NoDefault,
            calendar_default: None,
            warnings: vec![
                "calendar unavailable — NO live call until an explicit LS_PROBE_SDATE/EDATE \
                 range is supplied"
                    .to_string(),
            ],
            bypass_audit: None,
        },
    }
}

/// The per-invocation calendar context (U1, KTD1): ONE env read, ONE as-of instant, at most
/// ONE immutable calendar load, shared between the mandatory startup record and the
/// probe-date decision. Built by [`ProbeContext::resolve`] before the paper gate so the
/// always-emit startup invariant holds on every path; the resolved range (or the Enforced
/// refusal) is read back later via [`ProbeContext::resolved_range`].
struct ProbeContext {
    /// The planned probe-date decision (range, source, warnings, bypass audit, live-request
    /// flag) — inspectable without a live gateway.
    plan: ProbeDatePlan,
}

impl ProbeContext {
    /// Resolve the env-configured snapshot path + adoption, build a fixed-`now` as-of view
    /// from ONE load, scan for the recent proven session, plan the range, and emit BOTH the
    /// mandatory decision-relevant startup record and the calendar-decision diagnostics to
    /// the non-persisted channel (stderr, KTD8). The load is non-fatal (a missing/failed
    /// snapshot is a recorded unavailable state); the Enforced refusal is deferred to
    /// [`resolved_range`](Self::resolved_range) so the paper gate stays the primary refusal.
    fn resolve(explicit: &ExplicitRange) -> Self {
        // Enforced-only after the budget-probe Consumer Retirement Gate (#189 U8, KTD3): the
        // date decision no longer consults LS_CALENDAR_ADOPTION.
        let adoption = CalendarAdoption::Enforced;
        let as_of = chrono::Utc::now();
        let path = nautilus_ls::calendar::snapshot_path_from_env();
        let loaded = nautilus_ls::calendar::resolve_and_load(path.as_deref(), as_of, adoption);
        // A usable view requires a loaded calendar that authorizes at `as_of`.
        let view = loaded.calendar().and_then(|c| c.as_of(as_of).ok());
        // Scan back from today's KST civil date for the most recent proven Trading Session
        // (skipping Closed AND Unknown) over the bounded lookback — no weekday anchor.
        let anchor_date = (as_of + Duration::hours(9)).date_naive();
        let scan = scan_recent_session(view.as_ref(), anchor_date);
        // Resolve the (sanitized) operator + run context once at the composition root and
        // thread it into the pure plan (U2, KTD4) — it only lands in the audit on a bypass.
        let run = RunContext::from_env();
        let plan = plan_probe_dates(scan, explicit, &run);

        // Mandatory startup record (U1/KTD2, R2): one redacted line to the non-persisted
        // diagnostic channel, targeting the day the probe will ACTUALLY query — the resolved
        // probe date (`plan.sdate`) — or NO target when the plan refuses (no proven session /
        // unavailable). For a proven session it is the session (guaranteed in-coverage); for a
        // bypass it is the explicit date.
        let target = plan
            .sdate
            .as_deref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y%m%d").ok());
        let record = nautilus_ls::calendar::build_startup_record_targeted(
            "budget-probe",
            adoption,
            &loaded,
            as_of,
            target,
        );
        nautilus_ls::calendar::emit_startup_record(&record);

        // Record the calendar date decision to the non-persisted diagnostic channel only
        // (stderr) so the recording never touches stdout or a persisted artifact.
        Self::emit_probe_date_diagnostics(adoption, &plan);

        Self { plan }
    }

    /// Emit the calendar-date decision diagnostics (warnings, the recorded calendar default,
    /// and — for a bypass — the audit record) to the non-persisted channel (KTD8).
    fn emit_probe_date_diagnostics(adoption: CalendarAdoption, plan: &ProbeDatePlan) {
        for w in &plan.warnings {
            eprintln!("calendar-probe-date: {w}");
        }
        if let Some(def) = &plan.calendar_default {
            eprintln!(
                "calendar-probe-date: adoption={} source={} calendar_default={def}",
                adoption.as_str(),
                plan.source.token()
            );
        }
        if let Some(audit) = &plan.bypass_audit {
            eprintln!("calendar-probe-date: {}", audit.render_line());
        }
    }

    /// The resolved `(sdate, edate)` range, or a clean refusal when Enforced can prove
    /// nothing and no explicit range bypasses it.
    fn resolved_range(&self) -> Result<(String, String), Box<dyn std::error::Error>> {
        match (&self.plan.sdate, &self.plan.edate) {
            (Some(s), Some(e)) => Ok((s.clone(), e.clone())),
            _ => Err("calendar Enforced: no proven Trading Session in the lookback and no \
                      explicit LS_PROBE_SDATE/EDATE range — refusing to probe (supply an \
                      explicit range to bypass)"
                .into()),
        }
    }
}

/// Parse the comma-separated stage list (`"0,1"` → `[0, 1]`), rejecting junk.
fn parse_stages(spec: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let n: u8 = part.parse().map_err(|_| format!("bad stage {part:?} (want 0..=3)"))?;
        out.push(n);
    }
    if out.is_empty() {
        return Err("no stages selected (LS_PROBE_STAGES)".to_string());
    }
    Ok(out)
}

fn parse_i64_list(spec: &str) -> Result<Vec<i64>, String> {
    spec.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<i64>().map_err(|_| format!("bad interval {s:?}")))
        .collect()
}

fn parse_env<T: std::str::FromStr>(key: &str, default: T) -> Result<T, String> {
    match std::env::var(key) {
        Ok(v) => v.parse().map_err(|_| format!("{key} must parse, got {v:?}")),
        Err(_) => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_gate_refuses_non_paper() {
        assert!(paper_ok(Some("paper")));
        assert!(!paper_ok(Some("real")));
        assert!(!paper_ok(None));
    }

    #[test]
    fn stage_list_parses_and_rejects_junk() {
        assert_eq!(parse_stages("0,1").unwrap(), vec![0, 1]);
        assert_eq!(parse_stages(" 0 , 1 , 3 ").unwrap(), vec![0, 1, 3]);
        assert!(parse_stages("").is_err(), "empty selection is an error");
        assert!(parse_stages("x").is_err(), "junk is an error");
    }

    #[test]
    fn refill_sample_list_parses() {
        assert_eq!(parse_i64_list("30,60,120").unwrap(), vec![30, 60, 120]);
        assert!(parse_i64_list("30,abc").is_err());
    }

    // -----------------------------------------------------------------------
    // U13/U8 — budget-probe Enforced-only default-date selection. The pure
    // `plan_probe_dates` decides the selected default, warnings, bypass record, and WHETHER a
    // live request is attempted; `scan_recent_session` walks a fixture-loaded real
    // `KrxCalendar` (no live gateway). Selection is the most recent proven Trading Session.
    //
    // Fixture facts (nautilus-ls-calendar/fixtures/base_2010_2012.json):
    //   Trading Session : 2010-06-15, 2010-06-17, 2011-06-15
    //   Closed          : 2010-06-19, 2010-06-20, 2012-05-01, ...
    //   Unknown         : nearly every other weekday (e.g. 2010-06-18, 2010-01-05)
    //   Coverage        : 2010-01-01 .. 2012-12-31   Auth grant: 2013 .. 2099
    // -----------------------------------------------------------------------
    mod calendar_default_selection {
        use super::super::*;
        use chrono::{DateTime, NaiveDate, TimeZone, Utc};
        use nautilus_ls_calendar::KrxCalendar;

        fn fixture_calendar() -> KrxCalendar {
            let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("nautilus-ls-calendar/fixtures/base_2010_2012.json");
            KrxCalendar::load_from_path(&path, fresh_as_of()).expect("fixture calendar loads")
        }
        /// Every freshness anchor is fresh at this instant (all anchors == 2013-01-01).
        fn fresh_as_of() -> DateTime<Utc> {
            Utc.with_ymd_and_hms(2013, 1, 1, 0, 0, 0).unwrap()
        }
        /// > every freshness threshold (evidence from 2013-01-01) but still authorized.
        fn stale_as_of() -> DateTime<Utc> {
            Utc.with_ymd_and_hms(2013, 6, 1, 0, 0, 0).unwrap()
        }
        fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
            NaiveDate::from_ymd_opt(y, m, d).unwrap()
        }
        fn no_explicit() -> ExplicitRange {
            ExplicitRange { sdate: None, edate: None }
        }
        /// A fixed, pre-sanitized run context — asserted structurally, never by wall-clock.
        fn run_ctx() -> RunContext {
            RunContext { operator: "op-alice".to_string(), run_id: "run-42".to_string() }
        }

        /// Enforced Trading: the most recent proven session is selected, skipping a
        /// trailing Closed/Unknown run (2010-06-20 Closed, 06-19 Closed, 06-18 Unknown
        /// → 2010-06-17 Trading Session).
        #[test]
        fn enforced_selects_most_recent_proven_session_skipping_trailing_closed_unknown() {
            let cal = fixture_calendar();
            let view = cal.as_of(fresh_as_of()).unwrap();
            let scan = scan_recent_session(Some(&view), ymd(2010, 6, 20));
            assert!(
                matches!(scan, SessionScan::Session { date, stale: false } if date == ymd(2010, 6, 17)),
                "walk-back skips the trailing Closed/Unknown run to the proven session: {scan:?}"
            );
            let plan = plan_probe_dates(scan, &no_explicit(), &run_ctx());
            assert_eq!(plan.sdate.as_deref(), Some("20100617"));
            assert_eq!(plan.edate.as_deref(), Some("20100617"));
            assert!(plan.live_request, "a proven session is servable → live request");
            assert_eq!(plan.source, DateSource::CalendarSession);
            assert!(plan.warnings.is_empty(), "fresh evidence emits no warning: {:?}", plan.warnings);
        }

        /// Enforced Unknown/unavailable: NO live call is attempted until an explicit
        /// range is supplied.
        #[test]
        fn enforced_no_session_makes_no_live_call_until_explicit_range() {
            let cal = fixture_calendar();
            let view = cal.as_of(fresh_as_of()).unwrap();
            // Early-2010 anchor: the whole bounded lookback is Unknown/weekend-Closed
            // (first session is 2010-06-15) → no proven session.
            let scan = scan_recent_session(Some(&view), ymd(2010, 1, 20));
            assert!(matches!(scan, SessionScan::NoSession), "{scan:?}");
            let plan = plan_probe_dates(scan, &no_explicit(), &run_ctx());
            assert!(!plan.live_request, "no proven session → no live call");
            assert_eq!(plan.source, DateSource::NoDefault);
            assert!(plan.sdate.is_none() && plan.edate.is_none());
            assert!(!plan.warnings.is_empty(), "the refusal is recorded");

            // Unavailable calendar (no injected view) → the same refusal.
            let scan_u = scan_recent_session(None, ymd(2010, 1, 20));
            assert!(matches!(scan_u, SessionScan::Unavailable));
            let plan_u = plan_probe_dates(scan_u, &no_explicit(), &run_ctx());
            assert!(!plan_u.live_request);
            assert_eq!(plan_u.source, DateSource::NoDefault);

            // …until an explicit range is supplied → bypass → the live call proceeds.
            let explicit = ExplicitRange {
                sdate: Some("20100617".into()),
                edate: Some("20100617".into()),
            };
            let plan_b = plan_probe_dates(scan_u, &explicit, &run_ctx());
            assert!(plan_b.live_request, "an explicit range unblocks the live call");
            assert_eq!(plan_b.source, DateSource::Bypass);
            assert_eq!(plan_b.sdate.as_deref(), Some("20100617"));
        }

        /// A partial explicit range under Enforced fills the missing endpoint from the supplied
        /// one (single-day probe) — there is no weekday-anchor fallback after retirement.
        #[test]
        fn partial_explicit_range_fills_the_missing_endpoint() {
            let scan = scan_recent_session(None, ymd(2010, 1, 20)); // unavailable
            let only_sdate = ExplicitRange { sdate: Some("20100617".into()), edate: None };
            let plan = plan_probe_dates(scan, &only_sdate, &run_ctx());
            assert_eq!(plan.sdate.as_deref(), Some("20100617"));
            assert_eq!(plan.edate.as_deref(), Some("20100617"), "missing endpoint fills from sdate");
            assert_eq!(plan.source, DateSource::Bypass);
            assert!(plan.live_request);
        }

        /// Explicit range: recorded as a bypass, not a calendar override — even when a
        /// proven calendar session exists, the explicit range wins and the calendar
        /// default is only RECORDED.
        #[test]
        fn explicit_range_is_recorded_as_a_bypass_not_a_calendar_override() {
            let cal = fixture_calendar();
            let view = cal.as_of(fresh_as_of()).unwrap();
            let scan = scan_recent_session(Some(&view), ymd(2010, 6, 20)); // would select 2010-06-17
            let explicit = ExplicitRange {
                sdate: Some("20111231".into()),
                edate: Some("20111231".into()),
            };
            let plan = plan_probe_dates(scan, &explicit, &run_ctx());
            assert_eq!(plan.source, DateSource::Bypass);
            assert_eq!(plan.sdate.as_deref(), Some("20111231"));
            assert_eq!(plan.edate.as_deref(), Some("20111231"));
            assert!(plan.live_request);
            // The calendar default is recorded (diagnostic) but NOT used as the range.
            assert_eq!(plan.calendar_default.as_deref(), Some("20100617"));
        }

        /// Stale-but-established evidence: usable with a warning.
        #[test]
        fn stale_established_session_is_usable_with_a_warning() {
            let cal = fixture_calendar();
            let view = cal.as_of(stale_as_of()).unwrap();
            let scan = scan_recent_session(Some(&view), ymd(2010, 6, 20));
            assert!(matches!(scan, SessionScan::Session { stale: true, .. }), "{scan:?}");
            let plan = plan_probe_dates(scan, &no_explicit(), &run_ctx());
            assert_eq!(plan.sdate.as_deref(), Some("20100617"), "stale evidence is still usable");
            assert!(plan.live_request);
            assert_eq!(plan.source, DateSource::CalendarSession);
            assert!(
                plan.warnings.iter().any(|w| w.contains("stale")),
                "staleness surfaces a warning: {:?}",
                plan.warnings
            );
        }
    }

    // -----------------------------------------------------------------------
    // U2 (KTD4) — the explicit-range BYPASS audit. Every explicit `LS_PROBE_SDATE`/
    // `EDATE` range records WHO forced it, the run context, and the calendar condition
    // automatic selection skipped — a pure, inspectable value that changes neither probe
    // status nor any dispatch authorization, and whose operator/run fields are sanitized
    // before they can reach the non-persisted diagnostic channel.
    // -----------------------------------------------------------------------
    mod bypass_audit {
        use super::super::*;
        use chrono::{DateTime, TimeZone, Utc};
        use nautilus_ls_calendar::KrxCalendar;

        fn fixture_calendar() -> KrxCalendar {
            let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("nautilus-ls-calendar/fixtures/base_2010_2012.json");
            KrxCalendar::load_from_path(&path, fresh_as_of()).expect("fixture calendar loads")
        }
        fn fresh_as_of() -> DateTime<Utc> {
            Utc.with_ymd_and_hms(2013, 1, 1, 0, 0, 0).unwrap()
        }
        fn ymd(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
            chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
        }
        fn run_ctx() -> RunContext {
            RunContext { operator: "op-alice".to_string(), run_id: "run-42".to_string() }
        }
        fn explicit() -> ExplicitRange {
            ExplicitRange { sdate: Some("20111231".into()), edate: Some("20111231".into()) }
        }

        /// Scenario 1: explicit range under Enforced with an UNAVAILABLE calendar records a
        /// bypass audit naming the operator + run context + condition `unavailable`, with
        /// `live_request` true and the resolved range honored (probe status unchanged).
        #[test]
        fn enforced_unavailable_records_unavailable_condition_and_still_calls() {
            let scan = scan_recent_session(None, ymd(2010, 1, 20));
            assert!(matches!(scan, SessionScan::Unavailable));
            let plan = plan_probe_dates(scan, &explicit(), &run_ctx());
            let audit = plan.bypass_audit.as_ref().expect("a bypass records an audit");
            assert_eq!(audit.condition, BypassedCondition::Unavailable);
            assert_eq!(audit.operator, "op-alice");
            assert_eq!(audit.run_id, "run-42");
            assert!(plan.live_request, "the explicit range unblocks the live call");
            assert_eq!(plan.source, DateSource::Bypass);
            assert_eq!(plan.sdate.as_deref(), Some("20111231"));
        }

        /// Scenario 2: explicit range under Enforced with no proven session in the lookback
        /// records condition `no-proven-session`.
        #[test]
        fn enforced_no_proven_session_records_no_proven_session_condition() {
            let cal = fixture_calendar();
            let view = cal.as_of(fresh_as_of()).unwrap();
            // Early-2010 anchor: the whole bounded lookback is Unknown/weekend-Closed.
            let scan = scan_recent_session(Some(&view), ymd(2010, 1, 20));
            assert!(matches!(scan, SessionScan::NoSession), "{scan:?}");
            let plan = plan_probe_dates(scan, &explicit(), &run_ctx());
            let audit = plan.bypass_audit.as_ref().expect("a bypass records an audit");
            assert_eq!(audit.condition, BypassedCondition::NoProvenSession);
            assert!(plan.live_request);
        }

        /// Scenario 3: explicit range under Enforced when a session WAS proven records
        /// condition `proven-session-not-selected` and still records `calendar_default`.
        #[test]
        fn enforced_proven_but_overridden_records_proven_not_selected_and_default() {
            let cal = fixture_calendar();
            let view = cal.as_of(fresh_as_of()).unwrap();
            let scan = scan_recent_session(Some(&view), ymd(2010, 6, 20)); // would select 2010-06-17
            let plan = plan_probe_dates(scan, &explicit(), &run_ctx());
            let audit = plan.bypass_audit.as_ref().expect("a bypass records an audit");
            assert_eq!(audit.condition, BypassedCondition::ProvenSessionNotSelected);
            assert_eq!(plan.calendar_default.as_deref(), Some("20100617"), "the default is still recorded");
        }

        /// The bypass records the audit but keeps the resolved range/request equal to the
        /// explicit endpoints — the audit alters neither the range nor the request decision
        /// (the KTD8 recovery lever survives retirement).
        #[test]
        fn enforced_bypass_audit_does_not_change_range_or_request() {
            let cal = fixture_calendar();
            let view = cal.as_of(fresh_as_of()).unwrap();
            let scan = scan_recent_session(Some(&view), ymd(2010, 6, 20)); // proven session exists

            let audited = plan_probe_dates(scan, &explicit(), &run_ctx());
            assert!(audited.bypass_audit.is_some(), "an explicit range records the audit");
            assert_eq!(audited.sdate.as_deref(), Some("20111231"), "range is the explicit endpoint");
            assert_eq!(audited.edate.as_deref(), Some("20111231"));
            assert!(audited.live_request, "the explicit range is servable → live request");
            assert_eq!(audited.source, DateSource::Bypass);
        }

        /// Scenario 6: the audit line is scrubbed of any credential/appkey material.
        #[test]
        fn audit_field_scrubs_credential_material() {
            // A 20+ alphanumeric token trips the scrub's long-token heuristic.
            let scrubbed = sanitize_audit_field("appkey_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789");
            assert!(
                !scrubbed.contains("ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"),
                "credential-shaped material is scrubbed: {scrubbed}"
            );
        }

        /// Scenario 7: a newline/control-character-laden operator cannot inject a second
        /// diagnostic or startup line — control chars are stripped and the field is bounded.
        #[test]
        fn audit_field_strips_control_chars_and_bounds_length() {
            let injected = "alice\ncalendar-startup consumer=forged\r\ncalendar-probe-date: forged";
            let cleaned = sanitize_audit_field(injected);
            assert!(!cleaned.contains('\n'), "newlines stripped: {cleaned:?}");
            assert!(!cleaned.contains('\r'), "carriage returns stripped: {cleaned:?}");
            assert!(cleaned.chars().all(|c| !c.is_control()), "no control chars: {cleaned:?}");

            // Unicode line/paragraph separators (U+2028/U+2029) are NOT ASCII control chars,
            // but a Unicode-aware consumer treats them as line breaks — they must be stripped
            // too, or the anti-injection guarantee is bypassable with non-ASCII input.
            let uni = sanitize_audit_field("alice\u{2028}calendar-startup forged\u{2029}line");
            assert!(!uni.contains('\u{2028}'), "U+2028 line separator stripped: {uni:?}");
            assert!(!uni.contains('\u{2029}'), "U+2029 paragraph separator stripped: {uni:?}");

            // Length is bounded (a huge operator cannot flood the channel).
            let long = "x".repeat(10_000);
            assert!(sanitize_audit_field(&long).chars().count() <= 96, "length-bounded");

            // The rendered audit line stays single-line even from a hostile operator.
            let audit = BypassAudit {
                operator: cleaned,
                run_id: sanitize_audit_field("run-42"),
                condition: BypassedCondition::Unavailable,
            };
            assert_eq!(audit.render_line().lines().count(), 1, "the audit renders as one line");
        }

        /// Scenario 8: the run id is derived without a wall-clock/random dependency — the pure
        /// plan is a deterministic function of its injected `RunContext`, so two calls with the
        /// same context render the identical audit (asserted structurally, not by value).
        #[test]
        fn audit_is_deterministic_no_wall_clock() {
            let scan = scan_recent_session(None, ymd(2010, 1, 20));
            let a = plan_probe_dates(scan, &explicit(), &run_ctx());
            let b = plan_probe_dates(scan, &explicit(), &run_ctx());
            assert_eq!(a.bypass_audit, b.bypass_audit, "no hidden clock/random in the pure path");
            let line = a.bypass_audit.unwrap().render_line();
            assert!(line.contains("run_id=run-42"), "the injected run id renders: {line}");
            assert!(
                line.contains("audit only") && line.contains("no dispatch authorization"),
                "the audit disclaims any status/dispatch effect: {line}"
            );
        }

        /// Scenario 8 (derivation): `RunContext::from_env` — the REAL run-context derivation the
        /// pure plan only consumes — reads env + process id ONLY (no wall-clock/random), so two
        /// calls in one process yield the identical operator + run id, and both resolve
        /// non-empty. This exercises the derivation the injected-constant determinism test above
        /// cannot reach.
        #[test]
        fn run_context_from_env_is_deterministic_and_non_empty() {
            let a = RunContext::from_env();
            let b = RunContext::from_env();
            assert_eq!(a, b, "from_env is stable within a process — no clock/random dependency");
            assert!(!a.operator.is_empty(), "operator resolves non-empty (falls back to 'unknown')");
            assert!(!a.run_id.is_empty(), "run id resolves non-empty (falls back to pid)");
        }
    }
}
