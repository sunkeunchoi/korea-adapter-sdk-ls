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
//!   Default = the last weekday on/before today (KST). Override to a known-data day
//!   if the gateway errors (e.g. holiday) — the probe only needs the call to serve.
//! - `LS_PROBE_PACE_MS`: inter-call pace in ms for stage 1 (default `1000`; must stay
//!   at or under the published per-second cap — **t8412 is 1/s, so keep `>=1000`**;
//!   a faster pace would trip t8412's per-second cap and confound the measurement).
//! - `LS_PROBE_CATALOG`: catalog dir the spend ledger derives from (default `catalog`).
//! - `LS_PROBE_OUT`: report path (default `probes/budget-model-report.json`).
//! - `LS_PROBE_REFILL_SAMPLES`: comma-separated widening intervals for stage 2
//!   (seconds, default `30,60,120,240`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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
    let default_day = recent_trading_day();
    let sdate = std::env::var("LS_PROBE_SDATE").unwrap_or_else(|_| default_day.clone());
    let edate = std::env::var("LS_PROBE_EDATE").unwrap_or_else(|_| default_day.clone());
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
}
