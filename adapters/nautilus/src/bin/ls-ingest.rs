//! `ls-ingest` — the historical-bar backfill entry point (U3).
//!
//! Paper-only, operator-run. It resolves LS credentials from a lane env-file (or
//! the process env), loads the domestic-equity universe, writes the instrument
//! definitions + bars into a `ParquetDataCatalog`, and holds the R15 advisory lock
//! for the duration (refusing to start while a live session is running).
//!
//! Configuration (env vars):
//! - `LS_INGEST_CATALOG`   — catalog directory (required)
//! - `LS_INGEST_SDATE`     — range start `YYYYMMDD`, a trading day (required)
//! - `LS_INGEST_EDATE`     — range end `YYYYMMDD`, a trading day (required)
//! - `LS_INGEST_LANE_FILE` — optional lane env-file (else the process env is used)
//! - `LS_INGEST_SYMBOLS`   — optional comma-separated shcodes to bound the universe
//!                           (else the whole loaded universe; minute backfills MUST
//!                           be bounded — see the README budget note)
//! - `LS_INGEST_KIND`      — `daily` (default) | `minute:<n>` | `daily,minute:<n>`

use std::path::PathBuf;

use nautilus_ls::config::LsAdapterConfig;
use nautilus_ls::ingest::{BarKind, IngestConfig, Ingestor};
use nautilus_ls::instruments::{InstrumentDomain, InstrumentProvider};
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_ls::scrub;
use nautilus_model::identifiers::{InstrumentId, Symbol, Venue};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Credential hygiene before any output (mirrors the repo's smoke convention).
    scrub::install();
    require_paper()?;

    let catalog: PathBuf = env_required("LS_INGEST_CATALOG")?.into();
    let sdate = env_required("LS_INGEST_SDATE")?;
    let edate = env_required("LS_INGEST_EDATE")?;
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

    // Persist the instrument definitions beside the bars.
    nautilus_ls::ingest::write_instruments(&catalog, provider.all_any()).await?;

    let config = IngestConfig {
        catalog_path: catalog,
        bar_kinds,
        sdate,
        edate,
        adjusted_prices: true,
    };
    // The ingest lock is already held (`_lock`), so run without re-acquiring it.
    let mut ingestor = Ingestor::new(sdk, config);
    let report = ingestor.run(&universe).await?;

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

fn env_required(key: &str) -> Result<String, String> {
    std::env::var(key).map_err(|_| format!("missing required env var {key}"))
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
