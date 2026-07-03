//! `ls-ingest` — the historical-bar backfill entry point (U3).
//!
//! Paper-only, operator-run. It resolves LS credentials from a lane env-file (or
//! the process env), loads the domestic-equity universe, writes the instrument
//! definitions + bars into a `ParquetDataCatalog`, and holds the R15 advisory lock
//! for the duration (refusing to start while a live session is running).
//!
//! Configuration (env vars):
//! - `LS_INGEST_CATALOG`: catalog directory (required).
//! - `LS_INGEST_MODE`: `range` (default) | `accumulate` (U5). In `accumulate` mode,
//!   `SDATE`/`EDATE` are ignored; coverage grows from each instrument's watermark to
//!   the last closed session.
//! - `LS_INGEST_SDATE` / `LS_INGEST_EDATE`: range bounds `YYYYMMDD` (required in
//!   `range` mode).
//! - `LS_INGEST_LOOKBACK`: accumulate-mode floor `YYYYMMDD` for an unseen/newly
//!   listed instrument (required in `accumulate` mode).
//! - `LS_INGEST_LANE_FILE`: optional lane env-file (else the process env is used).
//! - `LS_INGEST_SYMBOLS`: optional comma-separated shcodes to bound the universe
//!   (else the whole loaded universe; minute backfills MUST be bounded).
//! - `LS_INGEST_KIND`: `daily` (default) | `minute:<n>` | `daily,minute:<n>`.

use std::path::PathBuf;

use chrono::{Duration, NaiveDate, Utc};
use nautilus_ls::config::LsAdapterConfig;
use nautilus_ls::ingest::{
    last_closed_session, BarKind, IngestConfig, Ingestor, ACCUMULATE_CLOSE_BUFFER,
    DEFAULT_OVERLAP_DAYS,
};
use nautilus_ls::instruments::{InstrumentDomain, InstrumentProvider};
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_ls::scrub;
use nautilus_model::identifiers::{InstrumentId, Symbol, Venue};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    // Credential hygiene before any output (mirrors the repo's smoke convention).
    scrub::install();
    // Scrub the terminal error too — a `?`-propagated SDK error would otherwise be
    // printed unscrubbed by the runtime, leaking a raw broker message.
    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", scrub::scrub_secrets(&e.to_string()));
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    require_paper()?;

    let catalog: PathBuf = env_required("LS_INGEST_CATALOG")?.into();
    let mode = std::env::var("LS_INGEST_MODE").unwrap_or_else(|_| "range".into());
    let accumulate = match mode.as_str() {
        "range" => false,
        "accumulate" => true,
        "probe-lookback" => false, // handled early, below
        other => {
            return Err(format!(
                "unknown LS_INGEST_MODE {other:?} (want range | accumulate | probe-lookback)"
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
        return run_probe(&sdk, catalog).await;
    }

    // Load the domestic-equity universe.
    let mut provider = InstrumentProvider::new(sdk.clone());
    provider.load_domain(InstrumentDomain::DomesticEquity).await?;
    println!("loaded {} domestic-equity instruments", provider.len());

    // Bound the universe if requested (required for minute backfills).
    let universe: Vec<InstrumentId> = match std::env::var("LS_INGEST_SYMBOLS") {
        Ok(list) => list
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| InstrumentId::new(Symbol::from(s), Venue::from(nautilus_ls::KRX_VENUE)))
            .collect(),
        Err(_) => provider.all().map(|e| e.id).collect(),
    };

    // Persist the instrument definitions beside the bars (the universe re-snapshot,
    // R7 — newly-listed symbols enter coverage from this run forward).
    nautilus_ls::ingest::write_instruments(&catalog, provider.all_any()).await?;

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
        ingestor.run_accumulate(&universe, last_closed, floor).await?
    } else {
        ingestor.run(&universe).await?
    };

    println!(
        "ingest complete: {} bars across {} triples ({} skipped), {} coverage gaps",
        report.bars_written,
        report.triples_ingested,
        report.triples_skipped,
        report.gaps.len()
    );
    println!(
        "budget: {} symbols x {} bar-kinds, paced to {}/s (>= {:.0}s wall clock)",
        report.budget.symbols,
        report.budget.bar_kinds,
        report.budget.per_sec_cap,
        report.budget.min_seconds()
    );
    Ok(())
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
