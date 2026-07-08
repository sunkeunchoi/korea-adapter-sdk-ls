//! `ls-ingest` — the historical-bar backfill entry point (U3).
//!
//! Paper-only, operator-run. It resolves LS credentials from a lane env-file (or
//! the process env), loads the domestic-equity universe, writes the instrument
//! definitions + bars into a `ParquetDataCatalog`, and holds the R15 advisory lock
//! for the duration (refusing to start while a live session is running).
//!
//! Configuration (env vars):
//! - `LS_INGEST_CATALOG`: catalog directory (required).
//! - `LS_INGEST_MODE`: `range` (default) | `accumulate` (U5) | `rebase` (epoch
//!   re-base — see the README runbook) | `probe-lookback`. In `accumulate`/`rebase`
//!   modes, `SDATE`/`EDATE` are ignored; coverage grows from each instrument's
//!   watermark to the last closed session. `rebase` first marks every daily triple
//!   shifted (one atomic checkpoint save), then heals each through the same path.
//! - `LS_INGEST_SDATE` / `LS_INGEST_EDATE`: range bounds `YYYYMMDD` (required in
//!   `range` mode).
//! - `LS_INGEST_LOOKBACK`: accumulate/rebase-mode floor `YYYYMMDD` for an
//!   unseen/newly listed instrument — and, in `rebase` mode, the re-pull floor
//!   for every symbol (required; pin at or before the original backfill start).
//! - `LS_INGEST_LANE_FILE`: optional lane env-file (else the process env is used).
//! - `LS_INGEST_SYMBOLS`: optional comma-separated shcodes to bound the universe
//!   (else the whole loaded universe; minute backfills MUST be bounded).
//! - `LS_INGEST_KIND`: `daily` (default) | `minute:<n>` | `daily,minute:<n>`.
//! - `LS_INGEST_SKIP_UNIVERSE_LOAD`: `1`/`true` to skip the per-invocation universe
//!   load (`t8430` + 2× `t9945`) and the `write_instruments` re-snapshot — the
//!   dominant avoidable IGW00201 cost in a per-symbol drip loop. REQUIRES an
//!   explicit `LS_INGEST_SYMBOLS` list and a non-empty catalog (a prior
//!   full-universe pass must have persisted the instrument defs); refuses otherwise.

use std::path::PathBuf;

use chrono::{Duration, NaiveDate, Utc};
use nautilus_ls::config::LsAdapterConfig;
use nautilus_ls::ingest::{
    last_closed_session, BarKind, CoverageReport, IngestConfig, Ingestor, ACCUMULATE_CLOSE_BUFFER,
    DEFAULT_OVERLAP_DAYS,
};
use nautilus_ls::instruments::{InstrumentDomain, InstrumentProvider};
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_ls::scrub;
use nautilus_model::identifiers::{InstrumentId, Symbol, Venue};

/// The exit code for a run that carried a genuine per-triple refusal (#104): a
/// range/heal/append refusal means a triple stalled and an operator must act.
/// Distinct from the hard-error `1` the `Err` path returns (`ExitCode::FAILURE`),
/// so CI can tell "the run itself failed" from "the run completed but N triples
/// were refused".
const EXIT_REFUSALS: u8 = 2;

/// The process exit code implied by a completed run's [`CoverageReport`] (#104,
/// R8/R9): nonzero when any *genuine refusal* vec (range, heal, or append overlap)
/// is non-empty, zero otherwise. Backward-widen warnings are informational and
/// never consulted (R9) — a late-listed symbol warns forever without reddening CI.
fn exit_code_for(report: &CoverageReport) -> u8 {
    let refused = !report.range_refusals.is_empty()
        || !report.heal_refusals.is_empty()
        || !report.append_refusals.is_empty();
    if refused {
        EXIT_REFUSALS
    } else {
        0
    }
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    // Credential hygiene before any output (mirrors the repo's smoke convention).
    scrub::install();
    // Scrub the terminal error too — a `?`-propagated SDK error would otherwise be
    // printed unscrubbed by the runtime, leaking a raw broker message.
    match run().await {
        // Probe mode carries no coverage report — nothing to refuse, exit 0.
        Ok(None) => std::process::ExitCode::SUCCESS,
        // A completed run: exit nonzero iff it carried a genuine refusal (#104).
        Ok(Some(report)) => std::process::ExitCode::from(exit_code_for(&report)),
        Err(e) => {
            eprintln!("error: {}", scrub::scrub_secrets(&e.to_string()));
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<Option<CoverageReport>, Box<dyn std::error::Error>> {
    require_paper()?;

    let catalog: PathBuf = env_required("LS_INGEST_CATALOG")?.into();
    let mode = std::env::var("LS_INGEST_MODE").unwrap_or_else(|_| "range".into());
    // `accumulate` and `rebase` share the watermark/floor arithmetic; rebase
    // additionally marks every daily triple shifted first (the epoch re-base,
    // KTD-4 — see the README runbook before running it).
    let accumulate = match mode.as_str() {
        "range" => false,
        "accumulate" | "rebase" => true,
        "probe-lookback" => false, // handled early, below
        other => {
            return Err(format!(
                "unknown LS_INGEST_MODE {other:?} (want range | accumulate | rebase | probe-lookback)"
            )
            .into())
        }
    };
    let bar_kinds = parse_kinds(&std::env::var("LS_INGEST_KIND").unwrap_or_else(|_| "daily".into()))?;

    // Take the R15 ingest lock FIRST — before any gateway request — so a live
    // session holding the counterpart lock blocks us before we issue the universe
    // load (t8430 + 2x t9945) against the shared per-process rate buckets.
    let _lock = AdvisoryLock::acquire(&catalog, LockKind::Ingest)?;

    let adapter_cfg = match std::env::var("LS_INGEST_LANE_FILE") {
        Ok(path) => LsAdapterConfig::from_lane_file(path),
        Err(_) => LsAdapterConfig::from_env(),
    };
    let sdk = adapter_cfg.build_sdk()?;

    // Staged max-lookback probe (KTD10, R10): locate the earliest served minute date
    // for a pilot symbol and write <data>/probes/minute-lookback.json. No universe
    // load — the probe walks a single pilot symbol. Operator-gated.
    if mode == "probe-lookback" {
        run_probe(&sdk, catalog).await?;
        return Ok(None);
    }

    // Resolve the universe. A per-symbol drip loop re-invokes `ls-ingest` many times;
    // the universe load (t8430 + 2× t9945) is identical every time and charges the
    // shared IGW00201 budget, so `LS_INGEST_SKIP_UNIVERSE_LOAD` (with explicit
    // symbols + instruments already persisted by the drip's daily pass) skips it —
    // the dominant avoidable per-invocation cost (KTD5 budget).
    let symbols_env = std::env::var("LS_INGEST_SYMBOLS").ok().filter(|s| !s.trim().is_empty());
    let skip_load =
        should_skip_universe_load(env_flag("LS_INGEST_SKIP_UNIVERSE_LOAD"), symbols_env.is_some())?;

    let universe: Vec<InstrumentId> = if skip_load {
        // Skipping the load also skips `write_instruments`, which is only safe when a
        // prior full-universe pass persisted instrument defs. Refuse on an empty/
        // missing catalog rather than writing bars with no instruments (a silent
        // failure that only surfaces when the backtest can't resolve the instrument).
        let catalog_populated = catalog.exists()
            && std::fs::read_dir(&catalog).map(|mut d| d.next().is_some()).unwrap_or(false);
        if !catalog_populated {
            return Err(format!(
                "LS_INGEST_SKIP_UNIVERSE_LOAD is set but catalog {} is empty/missing — run a \
                 full-universe pass (without the flag) first so instrument definitions are persisted",
                catalog.display()
            )
            .into());
        }
        let syms = symbols_env.as_deref().unwrap_or_default();
        let u = parse_symbol_ids(syms);
        println!(
            "skipping universe load (LS_INGEST_SKIP_UNIVERSE_LOAD): {} explicit symbols \
             (catalog already populated)",
            u.len()
        );
        u
    } else {
        // Load the domestic-equity universe (t8430 + 2× t9945).
        let mut provider = InstrumentProvider::new(sdk.clone());
        provider.load_domain(InstrumentDomain::DomesticEquity).await?;
        println!("loaded {} domestic-equity instruments", provider.len());
        // Bound the universe if requested (required for minute backfills).
        let u: Vec<InstrumentId> = match &symbols_env {
            Some(list) => parse_symbol_ids(list),
            None => provider.all().map(|e| e.id).collect(),
        };
        // Persist the instrument definitions beside the bars (the universe re-snapshot,
        // R7 — newly-listed symbols enter coverage from this run forward).
        nautilus_ls::ingest::write_instruments(&catalog, provider.all_any()).await?;
        u
    };

    // Resolve the per-mode date range.
    let (sdate, edate) = if accumulate {
        let floor = env_required("LS_INGEST_LOOKBACK")?;
        let now_kst = (Utc::now() + Duration::hours(9)).naive_utc();
        let last_closed = last_closed_session(now_kst, ACCUMULATE_CLOSE_BUFFER);
        (floor, last_closed.format("%Y%m%d").to_string())
    } else {
        (env_required("LS_INGEST_SDATE")?, env_required("LS_INGEST_EDATE")?)
    };

    let config = IngestConfig {
        catalog_path: catalog,
        bar_kinds,
        sdate: sdate.clone(),
        edate: edate.clone(),
        adjusted_prices: true,
        overlap_days: DEFAULT_OVERLAP_DAYS,
    };
    // The ingest lock is already held (`_lock`), so run without re-acquiring it.
    let mut ingestor = Ingestor::new(sdk, config);
    let report = if accumulate {
        let floor = parse_yyyymmdd(&sdate)?;
        let last_closed = parse_yyyymmdd(&edate)?;
        if mode == "rebase" {
            ingestor.run_rebase(&universe, last_closed, floor).await?
        } else {
            ingestor.run_accumulate(&universe, last_closed, floor).await?
        }
    } else {
        ingestor.run(&universe).await?
    };

    println!(
        "ingest complete: {} bars across {} triples ({} skipped), {} coverage gaps, {} refused pending heal",
        report.bars_written,
        report.triples_ingested,
        report.triples_skipped,
        report.gaps.len(),
        report.range_refusals.len()
    );
    if !report.range_refusals.is_empty() {
        for r in &report.range_refusals {
            println!(
                "REFUSED PENDING HEAL: {} {} carries an unhealed basis-shift mark (detected {}); range mode will not serve it on a stale basis — run accumulate/rebase to heal",
                r.instrument, r.bar_type, r.detected
            );
        }
    }
    if !report.heal_refusals.is_empty() {
        for r in &report.heal_refusals {
            println!(
                "HEAL REFUSED: {} {} — run floor {} is later than earliest stored bar {}; re-run with LS_INGEST_LOOKBACK at or before it (symbol stays marked)",
                r.instrument, r.bar_type, r.floor, r.earliest_stored
            );
        }
    }
    if !report.append_refusals.is_empty() {
        for r in &report.append_refusals {
            println!(
                "APPEND REFUSED (overlap): {} {} — attempted {} overlaps stored coverage [{}]; run `lab-research catalog compact` (duplicate pollution) or wipe + full re-pull / fresh catalog (disjoint coverage). Watermark not advanced.",
                r.instrument, r.bar_type, r.attempted, r.stored
            );
        }
    }
    if !report.backward_widen_warnings.is_empty() {
        for w in &report.backward_widen_warnings {
            println!(
                "BACKWARD WIDEN NO-OP: {} {} — lookback floor {} precedes earliest stored coverage {}; accumulate never fetches below the watermark. Recover the pre-coverage region with a fresh catalog at the wider lookback, or wipe + full re-pull.",
                w.instrument, w.bar_type, w.floor, w.earliest_stored
            );
        }
    }
    if !report.budget_deferrals.is_empty() {
        for d in &report.budget_deferrals {
            println!(
                "SCHEDULED REMAINDER (budget): {} {} — estimated {} pages exceeds the remaining budget window ({} calls); stopped before the cliff, no bars fetched. Re-run on a cold budget window to resume (per-symbol idempotent).",
                d.instrument, d.bar_type, d.estimated_pages, d.remaining_budget
            );
        }
    }
    println!(
        "budget: {} symbols x {} bar-kinds, paced to {}/s (>= {:.0}s wall clock)",
        report.budget.symbols,
        report.budget.bar_kinds,
        report.budget.per_sec_cap,
        report.budget.min_seconds()
    );
    Ok(Some(report))
}

/// Staged max-lookback probe (KTD10). Uses a single liquid pilot symbol (default
/// `005930`) and a windowed backward search anchored at the last closed session,
/// writing the result to `<data>/probes/minute-lookback.json`.
async fn run_probe(sdk: &ls_sdk::LsSdk, catalog: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let pilot = std::env::var("LS_PROBE_SYMBOL").unwrap_or_else(|_| "005930".into());
    let ncnt: u32 = std::env::var("LS_PROBE_NCNT").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let now_kst = (Utc::now() + Duration::hours(9)).naive_utc();
    let anchor = last_closed_session(now_kst, ACCUMULATE_CLOSE_BUFFER);
    let probed_at = Utc::now().to_rfc3339();

    // A dummy config carrying the catalog path (the probe uses the fetcher, not the
    // range fields).
    let config = IngestConfig {
        catalog_path: catalog,
        bar_kinds: vec![BarKind::Minute(ncnt)],
        sdate: String::new(),
        edate: String::new(),
        adjusted_prices: true,
        overlap_days: DEFAULT_OVERLAP_DAYS,
    };
    let ingestor = Ingestor::new(sdk.clone(), config);
    match ingestor.run_probe_lookback(&pilot, ncnt, anchor, probed_at).await? {
        Some(lb) => {
            println!(
                "probe: pilot {pilot} earliest minute date {} (depth {} days) — recorded to <data>/probes/minute-lookback.json",
                lb.earliest_date, lb.depth_days
            );
            println!("derive the backfill floor: LS_INGEST_LOOKBACK={} (or anchor − {} days)", lb.earliest_date, lb.depth_days);
        }
        None => {
            println!("probe: pilot {pilot} served no minute history — nothing recorded");
        }
    }
    Ok(())
}

fn env_required(key: &str) -> Result<String, String> {
    std::env::var(key).map_err(|_| format!("missing required env var {key}"))
}

fn parse_yyyymmdd(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s.trim(), "%Y%m%d").map_err(|e| format!("bad date {s:?}: {e}"))
}

fn require_paper() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::var("LS_TRADING_ENV").as_deref() {
        Ok("paper") => Ok(()),
        _ => Err("refusing to run: set LS_TRADING_ENV=paper (this adapter is paper-only in v1)".into()),
    }
}

fn parse_kinds(spec: &str) -> Result<Vec<BarKind>, String> {
    let mut kinds = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if part == "daily" {
            kinds.push(BarKind::Daily);
        } else if let Some(n) = part.strip_prefix("minute:") {
            let n: u32 = n.parse().map_err(|_| format!("bad minute spec: {part}"))?;
            kinds.push(BarKind::Minute(n));
        } else {
            return Err(format!("unknown bar kind: {part} (want daily | minute:<n>)"));
        }
    }
    if kinds.is_empty() {
        kinds.push(BarKind::Daily);
    }
    Ok(kinds)
}

/// Read a boolean-ish env flag: present and `"1"`/`"true"` (case-insensitive) → true.
fn env_flag(key: &str) -> bool {
    std::env::var(key)
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true"))
        .unwrap_or(false)
}

/// Whether to skip the 3-call universe load (`t8430` + 2× `t9945`) and its
/// `write_instruments` re-snapshot for this invocation.
///
/// Skipping is the dominant avoidable IGW00201 saving in a per-symbol drip loop
/// (KTD5 budget): the universe load runs on *every* `ls-ingest` invocation but the
/// masters don't change minute-to-minute, so a 20-symbol minute drip re-fetches the
/// identical universe ~20× (3 calls each) on top of the bar fetches. Skipping is
/// only valid with an explicit `LS_INGEST_SYMBOLS` list (the load is otherwise the
/// only way to enumerate the universe), and assumes a prior full-universe
/// invocation already persisted the instrument definitions (the drip daily pass).
fn should_skip_universe_load(skip_requested: bool, has_symbols: bool) -> Result<bool, String> {
    match (skip_requested, has_symbols) {
        (true, true) => Ok(true),
        (true, false) => Err(
            "LS_INGEST_SKIP_UNIVERSE_LOAD requires an explicit LS_INGEST_SYMBOLS list \
             (the universe load is the only way to enumerate the full universe)"
                .to_string(),
        ),
        (false, _) => Ok(false),
    }
}

/// Parse a comma-separated shcode list into KRX-venue instrument ids.
fn parse_symbol_ids(list: &str) -> Vec<InstrumentId> {
    list.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| InstrumentId::new(Symbol::from(s), Venue::from(nautilus_ls::KRX_VENUE)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_ls::ingest::{AppendRefusal, BackwardWidenWarning, BudgetEstimate, HealRefusal, RangeRefusal};

    /// A zero-refusal, zero-warning coverage report — the base each case mutates.
    fn empty_report() -> CoverageReport {
        CoverageReport {
            bars_written: 0,
            triples_ingested: 0,
            triples_skipped: 0,
            gaps: Vec::new(),
            heal_refusals: Vec::new(),
            range_refusals: Vec::new(),
            append_refusals: Vec::new(),
            backward_widen_warnings: Vec::new(),
            budget_deferrals: Vec::new(),
            budget: BudgetEstimate { symbols: 0, bar_kinds: 0, per_sec_cap: 1, min_requests: 0 },
        }
    }

    #[test]
    fn exit_zero_for_empty_report() {
        assert_eq!(exit_code_for(&empty_report()), 0);
    }

    /// R9: a report carrying only backward-widen warnings is still exit 0 —
    /// warnings never redden CI (a late-listed symbol warns every run forever).
    #[test]
    fn exit_zero_for_warning_only_report() {
        let mut report = empty_report();
        report.backward_widen_warnings.push(BackwardWidenWarning {
            instrument: "005930.XKRX".to_string(),
            bar_type: "1-DAY".to_string(),
            floor: "20240101".to_string(),
            earliest_stored: "20240618".to_string(),
        });
        assert_eq!(exit_code_for(&report), 0, "backward-widen warnings never affect the exit code");
    }

    /// R8: each genuine refusal vec independently forces a nonzero exit — and it is
    /// the distinct refusal code (2), separate from the hard-error FAILURE (1).
    #[test]
    fn exit_nonzero_for_each_genuine_refusal() {
        let mut append = empty_report();
        append.append_refusals.push(AppendRefusal {
            instrument: "005930.XKRX".to_string(),
            bar_type: "1-DAY".to_string(),
            attempted: "20240103..20240105".to_string(),
            stored: "20240103..20240105".to_string(),
        });
        assert_eq!(exit_code_for(&append), EXIT_REFUSALS, "append refusal → nonzero");

        let mut heal = empty_report();
        heal.heal_refusals.push(HealRefusal {
            instrument: "005930.XKRX".to_string(),
            bar_type: "1-DAY".to_string(),
            floor: "20240104".to_string(),
            earliest_stored: "20240103".to_string(),
        });
        assert_eq!(exit_code_for(&heal), EXIT_REFUSALS, "heal refusal → nonzero");

        let mut range = empty_report();
        range.range_refusals.push(RangeRefusal {
            instrument: "005930.XKRX".to_string(),
            bar_type: "1-DAY".to_string(),
            detected: "20240105".to_string(),
        });
        assert_eq!(exit_code_for(&range), EXIT_REFUSALS, "range refusal → nonzero");

        assert_ne!(EXIT_REFUSALS, 1, "the refusal code stays distinct from the hard-error FAILURE");
    }

    #[test]
    fn skip_universe_load_decision() {
        // Flag + explicit symbols → skip the 3-call load (the drip-loop saving).
        assert_eq!(should_skip_universe_load(true, true), Ok(true));
        // No flag → always load (default, backward-compatible).
        assert_eq!(should_skip_universe_load(false, true), Ok(false));
        assert_eq!(should_skip_universe_load(false, false), Ok(false));
        // Flag WITHOUT explicit symbols is an error: the load is the only way to
        // enumerate the universe, so skipping it would leave nothing to ingest.
        assert!(
            should_skip_universe_load(true, false).is_err(),
            "skip without an explicit symbol list must be refused, not silently skipped"
        );
    }

    #[test]
    fn env_flag_parses_truthy_values() {
        // (env mutation is process-global; use a key unique to this test.)
        std::env::remove_var("LS_TEST_FLAG_XYZ");
        assert!(!env_flag("LS_TEST_FLAG_XYZ"), "unset → false");
        std::env::set_var("LS_TEST_FLAG_XYZ", "1");
        assert!(env_flag("LS_TEST_FLAG_XYZ"), "1 → true");
        std::env::set_var("LS_TEST_FLAG_XYZ", "TRUE");
        assert!(env_flag("LS_TEST_FLAG_XYZ"), "TRUE (case-insensitive) → true");
        std::env::set_var("LS_TEST_FLAG_XYZ", "0");
        assert!(!env_flag("LS_TEST_FLAG_XYZ"), "0 → false");
        std::env::remove_var("LS_TEST_FLAG_XYZ");
    }

    #[test]
    fn parse_symbol_ids_builds_krx_venue_ids() {
        let ids = parse_symbol_ids(" 005930, 000660 ,, 402340 ");
        assert_eq!(ids.len(), 3, "blank/whitespace entries skipped");
        assert_eq!(ids[0].symbol.as_str(), "005930");
        assert_eq!(ids[0].venue.as_str(), nautilus_ls::KRX_VENUE);
        assert_eq!(ids[2].symbol.as_str(), "402340");
    }
}
