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
//!   Default = the last weekday on/before today (KST) under the composed-default Shadow
//!   adoption, which also records the calendar-selected proven Trading Session to the
//!   diagnostic channel (U13, KTD8); under Enforced the default IS the most recent proven
//!   Trading Session, and the probe refuses (no live call) when none can be proven. Setting
//!   either endpoint is an explicit BYPASS — a servable known-data day when the gateway
//!   errors (e.g. holiday); the probe only needs the call to serve. Adoption is chosen by
//!   `LS_CALENDAR_ADOPTION` and the snapshot by `LS_CALENDAR_SNAPSHOT` (composition root).
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
    // Mandatory startup calendar record (U8): one redacted line to the non-persisted
    // diagnostic channel (stderr). Default adoption = Shadow; a missing snapshot is
    // non-fatal (KTD8). Startup record ONLY — the budget-probe date migration is U13.
    nautilus_ls::calendar::emit_startup_from_env("budget-probe");
    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", scrub::scrub_secrets(&e.to_string()));
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    if !paper_ok(std::env::var("LS_TRADING_ENV").ok().as_deref()) {
        return Err("refusing to run: set LS_TRADING_ENV=paper (this probe is paper-only)".into());
    }

    let stages = parse_stages(&std::env::var("LS_PROBE_STAGES").unwrap_or_else(|_| DEFAULT_STAGES.into()))?;
    let ceiling_n: usize = parse_env("LS_PROBE_CEILING", DEFAULT_CEILING)?;
    let symbol = std::env::var("LS_PROBE_SYMBOL").unwrap_or_else(|_| DEFAULT_SYMBOL.into());
    // U13 (KTD8): resolve the t8412 probe range through the calendar adoption seam.
    // Shadow (the composed default) keeps the weekday `recent_trading_day` default
    // authoritative — byte-identical to Legacy — while recording the calendar default to
    // the non-persisted diagnostic channel; Enforced (offline-tested) selects the most
    // recent proven Trading Session and refuses (no live call) when none can be proven and
    // no explicit LS_PROBE_SDATE/EDATE range is supplied. An explicit range is a BYPASS.
    let weekday_anchor = recent_trading_day();
    let explicit = ExplicitRange {
        sdate: std::env::var("LS_PROBE_SDATE").ok().filter(|s| !s.trim().is_empty()),
        edate: std::env::var("LS_PROBE_EDATE").ok().filter(|s| !s.trim().is_empty()),
    };
    let (sdate, edate) = resolve_probe_dates(&weekday_anchor, &explicit)?;
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

/// The last weekday on/before today in KST (`YYYYMMDD`) — the default t8412 probe
/// day. A weekend rolls back to Friday so the gateway does not `01715`; holidays
/// still need an `LS_PROBE_SDATE`/`EDATE` override (the probe only needs a servable
/// day, not real data).
fn recent_trading_day() -> String {
    use chrono::{Datelike, Duration, Weekday};
    let mut day = (chrono::Utc::now() + Duration::hours(9)).date_naive();
    while matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
        day -= Duration::days(1);
    }
    day.format("%Y%m%d").to_string()
}

// ---------------------------------------------------------------------------
// U13 (KTD8) — calendar-backed default-date selection behind the adoption seam.
//
// The date selection is split into two side-effect-free, unit-testable pieces:
//   * `scan_recent_session` walks a loaded `KrxCalendar` view back from the anchor
//     over a bounded lookback and yields the most recent PROVEN Trading Session
//     (skipping Closed AND Unknown), or a `NoSession` / `Unavailable` outcome.
//   * `plan_probe_dates` is a pure decision over (adoption, scan, weekday anchor,
//     explicit override) → the selected range, warnings, bypass record, and WHETHER
//     a live request is attempted. No calendar, no gateway — fully testable.
// `resolve_probe_dates` is the composition-root glue that reads env, builds the view,
// records the calendar decision to the non-persisted diagnostic channel, and returns
// the range (or a clean refusal when Enforced can prove nothing and has no bypass).
// ---------------------------------------------------------------------------

/// How far back from the anchor `scan_recent_session` looks for a proven Trading Session
/// before giving up (a holiday cluster plus surrounding Unknown weekdays fits well inside
/// a month). Bounded so a far-future anchor (a real KST "today" past the fixture window)
/// never scans unboundedly.
const PROBE_LOOKBACK_DAYS: i64 = 30;

/// The most-recent-proven-Trading-Session scan outcome (adoption-independent). `Copy` so a
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
    /// The weekday `recent_trading_day` default (Legacy / Shadow authoritative).
    WeekdayDefault,
    /// The calendar-selected most-recent proven Trading Session (Enforced).
    CalendarSession,
    /// An explicit `LS_PROBE_SDATE`/`EDATE` override — a bypass, not a calendar default.
    Bypass,
    /// Enforced could prove no session and no explicit range was supplied → no default.
    NoDefault,
}

impl DateSource {
    /// The stable token used in the diagnostic line.
    fn token(self) -> &'static str {
        match self {
            DateSource::WeekdayDefault => "weekday-default",
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
/// live request may be attempted, the range source, the recorded calendar default, and any
/// non-fatal warnings — all inspectable without a live gateway.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProbeDatePlan {
    sdate: Option<String>,
    edate: Option<String>,
    live_request: bool,
    source: DateSource,
    /// The calendar-selected session date, recorded for the diagnostic (Shadow records it;
    /// Enforced uses it). `None` under Legacy (never consults the calendar), or when no
    /// session was proven.
    calendar_default: Option<String>,
    warnings: Vec<String>,
}

/// Format a civil date as the gateway's `YYYYMMDD`.
fn fmt_ymd(date: NaiveDate) -> String {
    date.format("%Y%m%d").to_string()
}

/// Walk `view` back from `anchor` up to [`PROBE_LOOKBACK_DAYS`] (clamped to the materialized
/// coverage) and return the most recent PROVEN Trading Session — skipping Closed AND Unknown
/// dates (an Unknown never manufactures a servable default). A missing view is
/// [`Unavailable`](SessionScan::Unavailable); an exhausted lookback with no proven session is
/// [`NoSession`](SessionScan::NoSession). Adoption-independent (the value Shadow records and
/// Enforced acts on); side-effect-free.
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

/// The pure default-date decision (U13, KTD8). Legacy/Shadow keep the weekday anchor
/// authoritative (Shadow additionally RECORDS the calendar default); Enforced selects the
/// proven session, or refuses (no live call) when nothing is proven and no explicit range is
/// supplied. An explicit range is always a BYPASS. No calendar, no I/O — fully testable.
fn plan_probe_dates(
    adoption: CalendarAdoption,
    scan: SessionScan,
    weekday_anchor: &str,
    explicit: &ExplicitRange,
) -> ProbeDatePlan {
    // The weekday-authoritative range, byte-identical to the pre-migration behavior: each
    // endpoint is the explicit override if present, else the weekday `recent_trading_day`.
    let weekday_range = || {
        (
            explicit.sdate.clone().unwrap_or_else(|| weekday_anchor.to_string()),
            explicit.edate.clone().unwrap_or_else(|| weekday_anchor.to_string()),
        )
    };
    // The calendar-selected session (recorded in Shadow, used in Enforced). Legacy never
    // consults the calendar, so it records nothing.
    let calendar_default = match (adoption, scan) {
        (CalendarAdoption::Legacy, _) => None,
        (_, SessionScan::Session { date, .. }) => Some(fmt_ymd(date)),
        _ => None,
    };
    let bypass = explicit.is_supplied();

    match adoption {
        // Legacy / Shadow: the weekday path acts. Shadow differs only by recording the
        // calendar default (the `calendar_default` field above); the range + request
        // decision are identical, so byte-identical-to-Legacy holds.
        CalendarAdoption::Legacy | CalendarAdoption::Shadow => {
            let (sdate, edate) = weekday_range();
            ProbeDatePlan {
                sdate: Some(sdate),
                edate: Some(edate),
                live_request: true,
                source: if bypass { DateSource::Bypass } else { DateSource::WeekdayDefault },
                calendar_default,
                warnings: Vec::new(),
            }
        }
        CalendarAdoption::Enforced => {
            if bypass {
                // An explicit override wins even under Enforced — recorded as a bypass, and
                // it unblocks the live call the (unavailable/Unknown) calendar would refuse.
                let (sdate, edate) = weekday_range();
                return ProbeDatePlan {
                    sdate: Some(sdate),
                    edate: Some(edate),
                    live_request: true,
                    source: DateSource::Bypass,
                    calendar_default,
                    warnings: Vec::new(),
                };
            }
            match scan {
                SessionScan::Session { date, stale } => {
                    let mut warnings = Vec::new();
                    if stale {
                        warnings.push(format!(
                            "stale-but-established calendar evidence — using proven session {} \
                             anyway (refresh the snapshot)",
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
                    }
                }
                SessionScan::NoSession => ProbeDatePlan {
                    sdate: None,
                    edate: None,
                    live_request: false,
                    source: DateSource::NoDefault,
                    calendar_default: None,
                    warnings: vec![
                        "no proven Trading Session in the calendar lookback — NO live call \
                         until an explicit LS_PROBE_SDATE/EDATE range is supplied"
                            .to_string(),
                    ],
                },
                SessionScan::Unavailable => ProbeDatePlan {
                    sdate: None,
                    edate: None,
                    live_request: false,
                    source: DateSource::NoDefault,
                    calendar_default: None,
                    warnings: vec![
                        "calendar unavailable — NO live call until an explicit \
                         LS_PROBE_SDATE/EDATE range is supplied"
                            .to_string(),
                    ],
                },
            }
        }
    }
}

/// Composition-root glue (U13): resolve the env-configured snapshot path + adoption, build a
/// fixed-`now` as-of view, scan for the recent proven session, plan the range, record the
/// calendar decision to the non-persisted diagnostic channel (stderr, KTD8), and return the
/// `(sdate, edate)` range — or a clean refusal when Enforced can prove nothing and no explicit
/// range bypasses it. The pure functions above carry the logic; this only wires I/O.
fn resolve_probe_dates(
    weekday_anchor: &str,
    explicit: &ExplicitRange,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let adoption = nautilus_ls::calendar::adoption_from_env();
    let as_of = chrono::Utc::now();
    let path = nautilus_ls::calendar::snapshot_path_from_env();
    let loaded = nautilus_ls::calendar::resolve_and_load(path.as_deref(), as_of, adoption);
    // A usable view requires a loaded calendar that authorizes at `as_of`.
    let view = loaded.calendar().and_then(|c| c.as_of(as_of).ok());
    let anchor_date = NaiveDate::parse_from_str(weekday_anchor, "%Y%m%d")
        .map_err(|e| format!("weekday anchor {weekday_anchor:?} parse: {e}"))?;
    let scan = scan_recent_session(view.as_ref(), anchor_date);
    let plan = plan_probe_dates(adoption, scan, weekday_anchor, explicit);

    // Record the calendar decision to the non-persisted diagnostic channel only (stderr) so
    // a Shadow/Legacy recording never touches stdout or a persisted artifact (KTD8).
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
    if matches!(plan.source, DateSource::Bypass) {
        eprintln!(
            "calendar-probe-date: explicit LS_PROBE_SDATE/EDATE range → BYPASS \
             (not a calendar-backed default)"
        );
    }

    match (plan.sdate, plan.edate) {
        (Some(s), Some(e)) => Ok((s, e)),
        _ => Err("calendar Enforced: no proven Trading Session in the lookback and no explicit \
                  LS_PROBE_SDATE/EDATE range — refusing to probe (supply an explicit range to \
                  bypass)"
            .into()),
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

    #[test]
    fn recent_trading_day_is_a_weekday_yyyymmdd() {
        use chrono::{Datelike, NaiveDate, Weekday};
        let day = recent_trading_day();
        assert_eq!(day.len(), 8, "YYYYMMDD");
        let parsed = NaiveDate::parse_from_str(&day, "%Y%m%d").expect("parses");
        assert!(!matches!(parsed.weekday(), Weekday::Sat | Weekday::Sun));
    }

    // -----------------------------------------------------------------------
    // U13 (KTD8) — budget-probe default-date selection migration behind the
    // calendar adoption seam. The pure `plan_probe_dates` decides the selected
    // default, warnings, bypass record, and WHETHER a live request is attempted;
    // `scan_recent_session` walks a fixture-loaded real `KrxCalendar` (no live
    // gateway). Shadow stays byte-identical to Legacy (weekday authoritative);
    // Enforced (offline-tested) selects the most recent proven Trading Session.
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
        use nautilus_ls_calendar::{CalendarAdoption, KrxCalendar};

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
            let plan = plan_probe_dates(CalendarAdoption::Enforced, scan, "20100620", &no_explicit());
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
            let plan = plan_probe_dates(CalendarAdoption::Enforced, scan, "20100120", &no_explicit());
            assert!(!plan.live_request, "no proven session → no live call");
            assert_eq!(plan.source, DateSource::NoDefault);
            assert!(plan.sdate.is_none() && plan.edate.is_none());
            assert!(!plan.warnings.is_empty(), "the refusal is recorded");

            // Unavailable calendar (no injected view) → the same refusal.
            let scan_u = scan_recent_session(None, ymd(2010, 1, 20));
            assert!(matches!(scan_u, SessionScan::Unavailable));
            let plan_u =
                plan_probe_dates(CalendarAdoption::Enforced, scan_u, "20100120", &no_explicit());
            assert!(!plan_u.live_request);
            assert_eq!(plan_u.source, DateSource::NoDefault);

            // …until an explicit range is supplied → bypass → the live call proceeds.
            let explicit = ExplicitRange {
                sdate: Some("20100617".into()),
                edate: Some("20100617".into()),
            };
            let plan_b =
                plan_probe_dates(CalendarAdoption::Enforced, scan_u, "20100120", &explicit);
            assert!(plan_b.live_request, "an explicit range unblocks the live call");
            assert_eq!(plan_b.source, DateSource::Bypass);
            assert_eq!(plan_b.sdate.as_deref(), Some("20100617"));
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
            let plan = plan_probe_dates(CalendarAdoption::Enforced, scan, "20100620", &explicit);
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
            let plan = plan_probe_dates(CalendarAdoption::Enforced, scan, "20100620", &no_explicit());
            assert_eq!(plan.sdate.as_deref(), Some("20100617"), "stale evidence is still usable");
            assert!(plan.live_request);
            assert_eq!(plan.source, DateSource::CalendarSession);
            assert!(
                plan.warnings.iter().any(|w| w.contains("stale")),
                "staleness surfaces a warning: {:?}",
                plan.warnings
            );
        }

        /// Shadow: default + request behavior byte-identical to Legacy while the calendar
        /// default is recorded — asserted both where the calendar DISAGREES and where it
        /// finds a session the weekday anchor ignores.
        #[test]
        fn shadow_default_and_request_are_byte_identical_to_legacy() {
            let cal = fixture_calendar();
            let view = cal.as_of(fresh_as_of()).unwrap();

            // Disagreement: weekday anchor 2010-01-20 is Unknown to the calendar (no
            // session in the lookback) — Shadow keeps the weekday anchor authoritative.
            let scan = scan_recent_session(Some(&view), ymd(2010, 1, 20));
            let legacy = plan_probe_dates(CalendarAdoption::Legacy, scan, "20100120", &no_explicit());
            let shadow = plan_probe_dates(CalendarAdoption::Shadow, scan, "20100120", &no_explicit());
            assert_eq!(legacy.sdate, shadow.sdate, "byte-identical sdate");
            assert_eq!(legacy.edate, shadow.edate, "byte-identical edate");
            assert_eq!(legacy.live_request, shadow.live_request, "same request decision");
            assert_eq!(shadow.sdate.as_deref(), Some("20100120"));
            assert!(shadow.live_request);
            assert_eq!(legacy.calendar_default, None, "Legacy never consults the calendar");

            // Session-bearing anchor: Shadow records the calendar default (2010-06-17) but
            // still uses the weekday anchor (2010-06-20) authoritatively.
            let scan2 = scan_recent_session(Some(&view), ymd(2010, 6, 20));
            let shadow2 =
                plan_probe_dates(CalendarAdoption::Shadow, scan2, "20100620", &no_explicit());
            assert_eq!(shadow2.sdate.as_deref(), Some("20100620"), "weekday anchor authoritative");
            assert_eq!(shadow2.source, DateSource::WeekdayDefault);
            assert_eq!(
                shadow2.calendar_default.as_deref(),
                Some("20100617"),
                "the calendar default is recorded in Shadow"
            );
        }
    }
}
