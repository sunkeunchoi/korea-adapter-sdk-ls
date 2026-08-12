//! `pit-universe-walk` — the P4 pit-universe depth walk over `t8410` daily
//! bars (plan `2026-08-12-001-feat-pit-universe-depth-walk-t8410`).
//!
//! Subcommands:
//! - `walk`: the attended live walk — screens the frozen board-ranked set for
//!   per-symbol listing evidence (oldest-first calendar-snapped windows) and
//!   runs the row-level measurement walks, then derives and writes the
//!   committed artifact. Paced, budget-gated, paper-only.
//! - `derive`: offline re-derivation over an existing artifact (no gateway).
//!
//! Configuration (env vars):
//! - `LS_TRADING_ENV=paper` (required for `walk` — refuses otherwise).
//! - `LS_CALENDAR_SNAPSHOT`: the calendar snapshot (enforced; fail-closed).
//! - `LS_PIT_UNIVERSE`: source capture artifact (default
//!   `lab/config/universe-metadata-20260723.json`).
//! - `LS_PIT_OUT`: artifact path (default `lab/config/pit-universe-<anchor>.json`;
//!   `derive` requires it or an explicit path argument).
//! - `LS_PIT_FLOOR`: `YYYYMMDD` (default `20160801`, the pre-registered floor).
//! - `LS_PIT_ANCHOR`: explicit proven-session anchor `YYYYMMDD` (default: the
//!   last proven session at/before today KST — refuses, proof-preserving, when
//!   an Unknown blocks the backward scan).
//! - `LS_PIT_LANE_FILE`: lane env-file (else the process env).
//! - `LS_PIT_PACE_MS`: inter-call pacing (default `1000`, the settled figure).
//! - `LS_PIT_BACKOFF_MS`: `IGW00201` backoff (default `120000`).
//! - `LS_PIT_QRYCNT`: requested rows/page (default `900`, the certified ingest
//!   figure — deliberately above the inferred 500-row server cap so the served
//!   page size *measures* the cap).
//! - `LS_PIT_SYMBOLS`: comma list restricting the screen to these frozen-set
//!   members (re-runs after a partial failure).
//! - `LS_PIT_MEASURE`: comma list of in-set `[floor, anchor]` full measurement
//!   walks (default empty; chosen at run time per plan Q1).
//! - `LS_PIT_MEASURE_DEEP`: comma list of unbounded deep walks from 1980-01-04
//!   (default `005930` — measures the true per-symbol vendor floor).
//! - `LS_PIT_THRESHOLDS`: comma list of concurrency floors (default `70,140`).
//! - `LS_PIT_CATALOG`: optional ingest catalog path — enables the shared
//!   MarketData budget gate and records the walk's spend into the shared
//!   ledger so the minute ingest's planner sees it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
use ls_sdk::paginated::{T8410Request, T8410Response};
use ls_sdk::LsSdk;
use nautilus_ls::calendar::IngestCalendarContext;
use nautilus_ls::config::LsAdapterConfig;
use nautilus_ls::error::AdapterResult;
use nautilus_ls::ingest::budget::{spend_ledger_path, BudgetModel, SpendLedger};
use nautilus_ls::ingest::DailyFetcher;
use nautilus_ls::reference::capture::budget_gate;
use nautilus_ls::reference::pit_walk::{
    default_floor, derive, freeze_walk_set, partition_windows, resolve_anchor, screen_symbol,
    walk_window, FailedSymbol, FrozenSet, ListingOutcome, MeasurementRecord, PitUniverseArtifact,
    SymbolRecord, WalkProvenance, ARTIFACT_SCHEMA_VERSION, MAX_SESSIONS_PER_WINDOW,
    MAX_WALK_PAGES,
};
use nautilus_ls::reference::universe_metadata::UniverseMetadata;
use nautilus_ls::scrub;

const DEFAULT_UNIVERSE: &str = "lab/config/universe-metadata-20260723.json";
const WALKED_TR: &str = "t8410";

/// The deep measurement walk's start (a weekday, far below any KRX listing the
/// vendor could serve) — the walk terminates at the symbol's true vendor floor.
fn deep_floor() -> NaiveDate {
    NaiveDate::from_ymd_opt(1980, 1, 4).expect("static date")
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    scrub::install();
    match run().await {
        Ok(clean) => {
            if clean {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("error: {}", scrub::scrub_secrets(&e.to_string()));
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<bool, Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).unwrap_or_default();
    match mode.as_str() {
        "walk" => run_walk().await,
        "derive" => run_derive().map(|()| true),
        other => Err(format!(
            "usage: pit-universe-walk <walk|derive> (got {other:?})"
        )
        .into()),
    }
}

// ---------------------------------------------------------------------------
// walk
// ---------------------------------------------------------------------------

/// The production fetcher: the certified single-page t8410 facade with the
/// body `cts_date` cursor threaded (mirrors the ingest's `SdkFetcher` — the
/// `tr_cont` header is not the continuation signal). Pacing lives in the walk
/// loop; the backoff is the measured `IGW00201` refill window.
struct PitFetcher {
    sdk: LsSdk,
    qrycnt: usize,
    backoff: Duration,
}

#[async_trait]
impl DailyFetcher for PitFetcher {
    async fn fetch_daily_page(
        &self,
        shcode: &str,
        sdate: &str,
        edate: &str,
        cts_date: &str,
    ) -> AdapterResult<T8410Response> {
        let mut req = T8410Request::new(
            shcode,
            "2", // daily
            self.qrycnt.to_string(),
            sdate.to_string(),
            edate.to_string(),
        );
        req.inblock.cts_date = cts_date.to_string();
        Ok(self.sdk.paginated().stock_chart_period(&req).await?)
    }

    fn throttle_backoff(&self) -> Duration {
        self.backoff
    }
}

async fn run_walk() -> Result<bool, Box<dyn std::error::Error>> {
    require_paper()?;

    // Calendar: enforced, fail-closed (the composition-root pattern).
    let now = Utc::now();
    let ctx = IngestCalendarContext::from_env(now);
    let today_kst = (now + ChronoDuration::hours(9)).date_naive();
    nautilus_ls::calendar::emit_startup_record(&ctx.startup_record("pit-universe-walk", today_kst));
    let view = ctx
        .view()
        .ok_or("calendar unavailable — enforced fail-closed (set LS_CALENDAR_SNAPSHOT)")?;

    let floor = match std::env::var("LS_PIT_FLOOR") {
        Ok(v) => parse_ymd("LS_PIT_FLOOR", &v)?,
        Err(_) => default_floor(),
    };
    let explicit_anchor = match std::env::var("LS_PIT_ANCHOR") {
        Ok(v) => Some(parse_ymd("LS_PIT_ANCHOR", &v)?),
        Err(_) => None,
    };
    let anchor = resolve_anchor(&view, floor, today_kst, explicit_anchor)?;

    // The frozen walk set (KTD1).
    let universe_path =
        std::env::var("LS_PIT_UNIVERSE").unwrap_or_else(|_| DEFAULT_UNIVERSE.into());
    let meta: UniverseMetadata = serde_json::from_str(
        &std::fs::read_to_string(&universe_path)
            .map_err(|e| format!("reading {universe_path}: {e}"))?,
    )
    .map_err(|e| format!("parsing {universe_path}: {e}"))?;
    let set = freeze_walk_set(&meta);
    // A restricted run (LS_PIT_SYMBOLS) is a re-run/repair tool: its artifact
    // must never read as the frozen universe, so it writes to a `-partial`
    // path and never carries a derived block.
    let restricted = std::env::var("LS_PIT_SYMBOLS").is_ok();
    let members = restrict_members(&set)?;

    let range = partition_windows(&view, floor, anchor, MAX_SESSIONS_PER_WINDOW)?;
    let pace = Duration::from_millis(env_u64("LS_PIT_PACE_MS", 1000)?);
    let backoff = Duration::from_millis(env_u64("LS_PIT_BACKOFF_MS", 120_000)?);
    let qrycnt = env_u64("LS_PIT_QRYCNT", 900)? as usize;
    let measure = env_list("LS_PIT_MEASURE");
    let measure_deep = match std::env::var("LS_PIT_MEASURE_DEEP") {
        Ok(v) => split_list(&v),
        Err(_) => vec!["005930".to_string()],
    };
    let thresholds = env_thresholds()?;

    println!(
        "pit-walk: floor {floor}, anchor {anchor} — {} proven sessions ({} unknown days), \
         {} windows; set {} members ({} preferred dropped, {} malformed dropped){}, \
         measure {:?} + deep {:?}",
        range.sessions.len(),
        range.unknown_days,
        range.windows.len(),
        members.len(),
        set.dropped_preferred.len(),
        set.dropped_malformed.len(),
        if restricted { " [RESTRICTED]" } else { "" },
        measure,
        measure_deep,
    );

    let adapter_cfg = match std::env::var("LS_PIT_LANE_FILE") {
        Ok(path) => LsAdapterConfig::from_lane_file(path),
        Err(_) => LsAdapterConfig::from_env(),
    };
    let sdk = adapter_cfg.build_sdk()?;

    // Budget gate (shared MarketData window). The screening estimate is one
    // single-page probe per (member, window) — the worst REALISTIC case: a
    // post-floor or no-served-rows symbol probes every window, and a
    // pre-floor symbol resolves in one. (Pathological multi-page windows are
    // bounded by the fail-closed walk arms, not priced here.) Measurement
    // walks add their page bounds.
    let estimated = (members.len() * range.windows.len()) as u32
        + (measure_deep.len() as u32) * 30
        + (measure.len() as u32) * 10;
    let budget_wiring = match std::env::var("LS_PIT_CATALOG") {
        Ok(catalog) => {
            let catalog = PathBuf::from(catalog);
            let model = BudgetModel::load_default();
            let ledger_path = spend_ledger_path(&catalog);
            let now_unix = Utc::now().timestamp();
            let ledger = SpendLedger::load_pruned(&ledger_path, now_unix - model.window_secs);
            let cred_hash = SpendLedger::hash_appkey(&sdk.inner().config.appkey);
            budget_gate(&model, &ledger, &cred_hash, now_unix, estimated)?;
            Some((ledger_path, ledger, cred_hash))
        }
        Err(_) => None,
    };

    let fetcher = PitFetcher {
        sdk,
        qrycnt,
        backoff,
    };

    // Screen every member; a failed symbol degrades to a FailedSymbol row
    // (surfaced, non-zero exit), never aborts the multi-symbol run.
    let mut symbols: Vec<SymbolRecord> = Vec::new();
    let mut failed: Vec<FailedSymbol> = Vec::new();
    let mut calls_made: u32 = 0;
    for m in &members {
        match screen_symbol(&fetcher, &m.shcode, &range.windows, pace).await {
            Ok(sw) => {
                calls_made += sw.calls;
                println!("  {} → {:?} ({} calls)", m.shcode, sw.outcome, sw.calls);
                symbols.push(SymbolRecord {
                    shcode: m.shcode.clone(),
                    market_class: m.market_class,
                    cap_tier: m.cap_tier,
                    outcome: sw.outcome,
                    calls: sw.calls,
                    pages: sw.pages,
                });
            }
            Err(we) => {
                // A failed walk spent real budget: its calls still count
                // toward the ledger (the CaptureError pattern).
                calls_made += we.calls;
                let msg = scrub::scrub_secrets(&we.to_string());
                eprintln!("  {} FAILED after {} calls: {msg}", m.shcode, we.calls);
                failed.push(FailedSymbol {
                    shcode: m.shcode.clone(),
                    error: msg,
                });
            }
        }
    }

    // Measurement walks (KTD3): row-level page evidence.
    let mut measurements: Vec<MeasurementRecord> = Vec::new();
    for (shcode, sdate) in measure_deep
        .iter()
        .map(|s| (s, deep_floor()))
        .chain(measure.iter().map(|s| (s, floor)))
    {
        match walk_window(&fetcher, shcode, sdate, anchor, pace, MAX_WALK_PAGES).await {
            Ok(ww) => {
                calls_made += ww.calls;
                println!(
                    "  measure {shcode} [{sdate}..{anchor}]: {} pages, max {} rows/page",
                    ww.pages.len(),
                    ww.pages.iter().map(|p| p.rows).max().unwrap_or(0)
                );
                measurements.push(MeasurementRecord {
                    shcode: shcode.clone(),
                    sdate,
                    edate: anchor,
                    pages: ww.pages,
                    calls: ww.calls,
                });
            }
            Err(we) => {
                calls_made += we.calls;
                let msg = scrub::scrub_secrets(&we.to_string());
                eprintln!("  measure {shcode} FAILED after {} calls: {msg}", we.calls);
                failed.push(FailedSymbol {
                    shcode: shcode.clone(),
                    error: msg,
                });
            }
        }
    }

    // Record the spend on BOTH outcomes — a failed walk spends real budget.
    if let Some((ledger_path, mut ledger, cred_hash)) = budget_wiring {
        let at = Utc::now().timestamp();
        for _ in 0..calls_made {
            ledger.record_spend(&cred_hash, at);
        }
        if let Err(e) = ledger.save(&ledger_path) {
            eprintln!("warning: failed to persist spend ledger (advisory): {e}");
        }
    }

    // Derive only on a complete, unrestricted run; an incomplete or restricted
    // artifact carries `None`. A derive error must NOT discard the walk — the
    // budget is already spent, so the artifact is written either way and the
    // non-zero exit says what is missing.
    let mut run_clean = failed.is_empty();
    let derived = if !failed.is_empty() {
        eprintln!(
            "run incomplete ({} failures) — derived block withheld; re-run the failures via \
             LS_PIT_SYMBOLS / LS_PIT_MEASURE and merge",
            failed.len()
        );
        None
    } else if restricted {
        eprintln!(
            "restricted run (LS_PIT_SYMBOLS) — derived block withheld: N(s) over a subset must \
             not read as the frozen universe"
        );
        None
    } else {
        match derive(&symbols, &measurements, &range.sessions, &thresholds) {
            Ok(d) => Some(d),
            Err(e) => {
                run_clean = false;
                eprintln!(
                    "warning: derive failed ({}) — artifact written with derived: null; fix and \
                     run `pit-universe-walk derive`",
                    scrub::scrub_secrets(&e.to_string())
                );
                None
            }
        }
    };

    let artifact = PitUniverseArtifact {
        schema_version: ARTIFACT_SCHEMA_VERSION,
        provenance: WalkProvenance {
            tr: WALKED_TR.into(),
            probed_at: now.to_rfc3339(),
            anchor,
            floor,
            source_artifact: universe_path.clone(),
            source_content_hash: set.source_content_hash.clone(),
            pace_ms: pace.as_millis() as u64,
            qrycnt,
            windows: range.windows.clone(),
            proven_sessions: range.sessions.len(),
            unknown_days: range.unknown_days,
            calls_made,
            dropped_preferred: set.dropped_preferred.clone(),
            dropped_malformed: set.dropped_malformed.clone(),
            restricted,
        },
        symbols,
        measurements,
        failed: failed.clone(),
        derived,
    };

    let out = match std::env::var("LS_PIT_OUT") {
        Ok(v) => v,
        Err(_) => {
            let suffix = if restricted { "-partial" } else { "" };
            format!(
                "lab/config/pit-universe-{}{suffix}.json",
                anchor.format("%Y%m%d")
            )
        }
    };
    write_artifact(&out, &artifact)?;
    print_summary(&artifact, &out);
    Ok(run_clean)
}

// ---------------------------------------------------------------------------
// derive
// ---------------------------------------------------------------------------

fn run_derive() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(2)
        .or_else(|| std::env::var("LS_PIT_OUT").ok())
        .ok_or("derive: pass the artifact path (arg or LS_PIT_OUT)")?;
    let mut artifact: PitUniverseArtifact = serde_json::from_str(
        &std::fs::read_to_string(&path).map_err(|e| format!("reading {path}: {e}"))?,
    )
    .map_err(|e| format!("parsing {path}: {e}"))?;
    if !artifact.failed.is_empty() {
        return Err(format!(
            "derive: artifact records {} failed walks — an incomplete run has no derived block",
            artifact.failed.len()
        )
        .into());
    }
    if artifact.provenance.restricted {
        return Err(
            "derive: artifact is from a restricted run (LS_PIT_SYMBOLS) — N(s) over a subset \
             must not read as the frozen universe; re-walk the full set"
                .into(),
        );
    }
    // Composition-root convention: emit the startup record BEFORE any
    // fallible calendar use (docs/solutions/conventions/
    // composition-root-always-emit-before-fallible-parse.md).
    let now = Utc::now();
    let ctx = IngestCalendarContext::from_env(now);
    let today_kst = (now + ChronoDuration::hours(9)).date_naive();
    nautilus_ls::calendar::emit_startup_record(
        &ctx.startup_record("pit-universe-walk-derive", today_kst),
    );
    let view = ctx
        .view()
        .ok_or("calendar unavailable — enforced fail-closed (set LS_CALENDAR_SNAPSHOT)")?;
    let range = partition_windows(
        &view,
        artifact.provenance.floor,
        artifact.provenance.anchor,
        MAX_SESSIONS_PER_WINDOW,
    )?;
    // Compare the session STRUCTURE, not just its cardinality: a same-size
    // snapshot with shifted holidays would silently move every
    // partition_point mapping. The calendar-snapped windows pin the exact
    // boundaries and per-window session counts; unknown_days pins the rest.
    if range.sessions.len() != artifact.provenance.proven_sessions
        || range.windows != artifact.provenance.windows
        || range.unknown_days != artifact.provenance.unknown_days
    {
        return Err(format!(
            "derive: calendar disagrees with the artifact's provenance ({} proven sessions / {} \
             windows now vs {} / {} at walk time) — the snapshot changed; re-walk or pin the \
             walk-time snapshot",
            range.sessions.len(),
            range.windows.len(),
            artifact.provenance.proven_sessions,
            artifact.provenance.windows.len(),
        )
        .into());
    }
    let thresholds = env_thresholds()?;
    artifact.derived = Some(derive(
        &artifact.symbols,
        &artifact.measurements,
        &range.sessions,
        &thresholds,
    )?);
    write_artifact(&path, &artifact)?;
    print_summary(&artifact, &path);
    Ok(())
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn print_summary(artifact: &PitUniverseArtifact, out: &str) {
    let (mut pre, mut listed, mut none) = (0usize, 0usize, 0usize);
    for s in &artifact.symbols {
        match s.outcome {
            ListingOutcome::PreFloor => pre += 1,
            ListingOutcome::Listed { .. } => listed += 1,
            ListingOutcome::NoServedRows { .. } => none += 1,
        }
    }
    println!(
        "walked {} symbols: {pre} pre-floor, {listed} post-floor listings, {none} no-served-rows \
         (anomalies); {} measurement walks; {} gateway calls → {out}",
        artifact.symbols.len(),
        artifact.measurements.len(),
        artifact.provenance.calls_made,
    );
    if let Some(d) = &artifact.derived {
        println!(
            "derived: {} proven sessions, N(s) min/median/max {}/{}/{}, mean participation \
             {:.4}, max observed rows/page {} (a lower bound on the server cap; measured only \
             if strictly below the requested qrycnt)",
            d.proven_sessions,
            d.listed_count_min,
            d.listed_count_median,
            d.listed_count_max,
            d.mean_participation,
            d.max_observed_rows_per_page,
        );
        for t in &d.thresholds {
            println!(
                "  concurrency ≥{}: effective S_max {} (first {}), bar(N=1) {:+.6}",
                t.concurrency,
                t.effective_s_max,
                t.first_session_at_or_above
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "never".into()),
                t.margin_bar_n1,
            );
        }
    }
    if !artifact.failed.is_empty() {
        println!("FAILED walks ({}):", artifact.failed.len());
        for f in &artifact.failed {
            println!("  {}: {}", f.shcode, f.error);
        }
    }
}

fn write_artifact(path: &str, artifact: &PitUniverseArtifact) -> Result<(), String> {
    let json = serde_json::to_string_pretty(artifact).map_err(|e| format!("serialize: {e}"))?;
    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }
    std::fs::write(path, format!("{json}\n")).map_err(|e| format!("writing {path}: {e}"))
}

fn restrict_members(
    set: &FrozenSet,
) -> Result<Vec<nautilus_ls::reference::pit_walk::FrozenMember>, String> {
    match std::env::var("LS_PIT_SYMBOLS") {
        Err(_) => Ok(set.members.clone()),
        Ok(v) => {
            let want = split_list(&v);
            let mut seen = std::collections::HashSet::new();
            let mut out = Vec::new();
            for w in &want {
                // Dedup: a repeated entry would double-spend and double-count
                // the symbol in every downstream tally.
                if !seen.insert(w.clone()) {
                    continue;
                }
                match set.members.iter().find(|m| &m.shcode == w) {
                    Some(m) => out.push(m.clone()),
                    None => {
                        return Err(format!(
                            "LS_PIT_SYMBOLS: {w:?} is not in the frozen walk set (board tiers \
                             minus the preferred rule)"
                        ))
                    }
                }
            }
            Ok(out)
        }
    }
}

fn env_list(name: &str) -> Vec<String> {
    std::env::var(name).map(|v| split_list(&v)).unwrap_or_default()
}

fn split_list(v: &str) -> Vec<String> {
    v.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn env_u64(name: &str, default: u64) -> Result<u64, String> {
    match std::env::var(name) {
        Err(_) => Ok(default),
        Ok(v) => v
            .parse()
            .map_err(|_| format!("{name} must be an integer, got {v:?}")),
    }
}

fn env_thresholds() -> Result<Vec<usize>, String> {
    match std::env::var("LS_PIT_THRESHOLDS") {
        Err(_) => Ok(vec![70, 140]),
        Ok(v) => split_list(&v)
            .iter()
            .map(|s| {
                s.parse()
                    .map_err(|_| format!("LS_PIT_THRESHOLDS entry {s:?} must be an integer"))
            })
            .collect(),
    }
}

fn parse_ymd(name: &str, v: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(v.trim(), "%Y%m%d")
        .map_err(|e| format!("{name} must be YYYYMMDD, got {v:?}: {e}"))
}

fn require_paper() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::var("LS_TRADING_ENV").as_deref() {
        Ok("paper") => Ok(()),
        _ => Err("refusing to run: set LS_TRADING_ENV=paper (this adapter is paper-only in v1)".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_and_threshold_parsing() {
        assert_eq!(split_list(" 005930, 000660 ,"), vec!["005930", "000660"]);
        assert!(split_list("").is_empty());
        assert_eq!(env_u64("LS_PIT_TEST_UNSET_VAR", 7).unwrap(), 7);
        assert_eq!(parse_ymd("x", "20160801").unwrap(), default_floor());
        assert!(parse_ymd("x", "2016-08-01").is_err());
    }
}
