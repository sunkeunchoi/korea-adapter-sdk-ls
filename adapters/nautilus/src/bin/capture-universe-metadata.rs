//! `capture-universe-metadata` — the attended live reference-data capture (U2,
//! plan 2026-07-10-003). Paper-only, operator-run, during an open KRX window
//! (`t1904` needs it; the five closure-certifiable TRs are pre-flighted under
//! closure first — see the plan's Verification Contract).
//!
//! Joins the six reference TRs by `shcode` into the `UniverseMetadata` artifact
//! (skeleton `t8430`/`t2522`/`t1904`×2, decorate `t1444`, gate `t1405`/`t1404`),
//! validates it fail-closed, and writes it with the content hash printed — the
//! identity the ingest pin and the backtest manifest both stamp (KTD2).
//!
//! Configuration (env vars):
//! - `LS_TRADING_ENV=paper` (required — refuses otherwise).
//! - `LS_CAPTURE_OUT`: output path (default `lab/config/universe-metadata.json`).
//! - `LS_CAPTURE_LANE_FILE`: optional lane env-file (else the process env).
//! - `LS_CAPTURE_KOSPI_UPCODE` / `LS_CAPTURE_KOSDAQ_UPCODE`: `t1444` boards
//!   (defaults `001` / `301`; confirm in the closed-window pre-flight).
//! - `LS_CAPTURE_CAP_ROWS`: board walk depth per market (default `400`).
//! - `LS_CAPTURE_T1405_CATEGORIES` / `LS_CAPTURE_T1404_CATEGORIES`: designation
//!   category specs, `gubun:jongchk:kind;...` with kind one of
//!   halt|managed|caution|warning|risk|overheated — the enum is confirmed live
//!   in the pre-flight; whatever is queried is recorded in provenance.
//! - `LS_CAPTURE_PACE_MS`: inter-call pacing (default `600`).
//! - `LS_CAPTURE_CATALOG`: optional ingest catalog path — enables the shared
//!   MarketData budget gate (refuses on `Defer`, KTD6) and records the capture's
//!   spend into the shared ledger so the minute ingest's planner sees it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use nautilus_ls::config::LsAdapterConfig;
use nautilus_ls::ingest::budget::{spend_ledger_path, BudgetModel, SpendLedger};
use nautilus_ls::reference::capture::{
    budget_gate, capture, estimated_capture_calls, CaptureConfig, DesignationQuery,
};
use nautilus_ls::reference::universe_metadata::{stratify, DesignationKind, Stratum};
use nautilus_ls::scrub;

const DEFAULT_OUT: &str = "lab/config/universe-metadata.json";

#[tokio::main]
async fn main() -> std::process::ExitCode {
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
    require_paper()?;

    let out = std::env::var("LS_CAPTURE_OUT").unwrap_or_else(|_| DEFAULT_OUT.into());
    let now = Utc::now();
    // The KST session the tags are as-of (tags are point-in-time, plan Dependencies).
    let session_date = (now + ChronoDuration::hours(9)).format("%Y%m%d").to_string();
    let mut cfg = CaptureConfig::new(now.to_rfc3339(), session_date);

    if let Ok(v) = std::env::var("LS_CAPTURE_KOSPI_UPCODE") {
        cfg.cap_boards[0].upcode = v;
    }
    if let Ok(v) = std::env::var("LS_CAPTURE_KOSDAQ_UPCODE") {
        cfg.cap_boards[1].upcode = v;
    }
    if let Ok(v) = std::env::var("LS_CAPTURE_CAP_ROWS") {
        let rows: usize = v.parse().map_err(|_| format!("LS_CAPTURE_CAP_ROWS must be an integer, got {v:?}"))?;
        for b in &mut cfg.cap_boards {
            b.max_rows = rows;
        }
    }
    if let Ok(v) = std::env::var("LS_CAPTURE_T1405_CATEGORIES") {
        cfg.t1405_categories = parse_categories(&v)?;
    }
    if let Ok(v) = std::env::var("LS_CAPTURE_T1404_CATEGORIES") {
        cfg.t1404_categories = parse_categories(&v)?;
    }
    if let Ok(v) = std::env::var("LS_CAPTURE_PACE_MS") {
        let ms: u64 = v.parse().map_err(|_| format!("LS_CAPTURE_PACE_MS must be an integer, got {v:?}"))?;
        cfg.pace = Duration::from_millis(ms);
    }

    let adapter_cfg = match std::env::var("LS_CAPTURE_LANE_FILE") {
        Ok(path) => LsAdapterConfig::from_lane_file(path),
        Err(_) => LsAdapterConfig::from_env(),
    };
    let sdk = adapter_cfg.build_sdk()?;

    // Budget gate (KTD6): consult the shared MarketData budget before spending
    // any of the attended window; record the capture's spend afterwards.
    let budget_wiring = match std::env::var("LS_CAPTURE_CATALOG") {
        Ok(catalog) => {
            let catalog = PathBuf::from(catalog);
            let model = BudgetModel::load_default();
            let ledger_path = spend_ledger_path(&catalog);
            let now_unix = Utc::now().timestamp();
            let ledger = SpendLedger::load_pruned(&ledger_path, now_unix - model.window_secs);
            let cred_hash = SpendLedger::hash_appkey(&sdk.inner().config.appkey);
            budget_gate(&model, &ledger, &cred_hash, now_unix, estimated_capture_calls(&cfg))?;
            Some((ledger_path, ledger, cred_hash))
        }
        Err(_) => None,
    };

    let outcome = capture(&sdk, &cfg).await?;
    let artifact = outcome.artifact;

    // Record the spend into the shared ledger (best-effort, advisory) so the
    // minute ingest's pre-dispatch planner sees the capture's cumulative cost.
    if let Some((ledger_path, mut ledger, cred_hash)) = budget_wiring {
        let at = Utc::now().timestamp();
        for _ in 0..outcome.calls_made {
            ledger.record_spend(&cred_hash, at);
        }
        if let Err(e) = ledger.save(&ledger_path) {
            eprintln!("warning: failed to persist spend ledger (advisory): {e}");
        }
    }

    let json = serde_json::to_string_pretty(&artifact)?;
    if let Some(parent) = Path::new(&out).parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }
    std::fs::write(&out, format!("{json}\n")).map_err(|e| format!("writing {out}: {e}"))?;

    // Summary: composition + resolution health + a stratification preview.
    let records = &artifact.records;
    let tradable = records.iter().filter(|r| r.tradable).count();
    let capped = records.iter().filter(|r| r.market_cap.resolved().is_some()).count();
    println!(
        "captured {} equity records ({} tradable, {} on the cap board, {} designated) → {out}",
        records.len(),
        tradable,
        capped,
        records.iter().filter(|r| r.designation.is_some()).count(),
    );
    println!("below-board (small-cap by exclusion): {}", records.len() - capped);
    if !artifact.provenance.paper_incompatible.is_empty() {
        for f in &artifact.provenance.paper_incompatible {
            println!("PAPER-INCOMPATIBLE: {} failed with {}", f.tr, f.code);
        }
    }
    let preview = stratify(records, usize::MAX);
    for stratum in Stratum::ALL {
        println!(
            "stratum {}: {} tradable candidates",
            stratum.label(),
            preview.get(&stratum).map(Vec::len).unwrap_or(0)
        );
    }
    println!("content hash: {}", artifact.content_hash());
    println!("gateway calls made: {}", outcome.calls_made);
    Ok(())
}

/// Parse a `gubun:jongchk:kind;...` category spec.
fn parse_categories(spec: &str) -> Result<Vec<DesignationQuery>, String> {
    let mut out = Vec::new();
    for part in spec.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let fields: Vec<&str> = part.split(':').collect();
        let [gubun, jongchk, kind] = fields.as_slice() else {
            return Err(format!("bad category spec {part:?} (want gubun:jongchk:kind)"));
        };
        let kind = match kind.trim().to_ascii_lowercase().as_str() {
            "halt" => DesignationKind::Halt,
            "managed" => DesignationKind::Managed,
            "caution" => DesignationKind::Caution,
            "warning" => DesignationKind::Warning,
            "risk" => DesignationKind::Risk,
            "overheated" => DesignationKind::Overheated,
            other => {
                return Err(format!(
                    "unknown designation kind {other:?} (want halt|managed|caution|warning|risk|overheated)"
                ))
            }
        };
        out.push(DesignationQuery {
            gubun: gubun.trim().to_string(),
            jongchk: jongchk.trim().to_string(),
            kind,
        });
    }
    if out.is_empty() {
        return Err("empty designation category spec".to_string());
    }
    Ok(out)
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
    fn category_spec_parses_and_rejects_bad_kinds() {
        let qs = parse_categories("0:1:warning; 0:2:halt").unwrap();
        assert_eq!(qs.len(), 2);
        assert_eq!(qs[0].gubun, "0");
        assert_eq!(qs[0].jongchk, "1");
        assert_eq!(qs[0].kind, DesignationKind::Warning);
        assert_eq!(qs[1].kind, DesignationKind::Halt);
        assert!(parse_categories("0:1:bogus").is_err());
        assert!(parse_categories("0:1").is_err());
        assert!(parse_categories("").is_err());
    }
}
