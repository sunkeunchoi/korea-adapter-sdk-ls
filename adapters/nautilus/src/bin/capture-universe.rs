//! `capture-universe` — the one-time live `t1444` KOSPI top-market-cap capture (U1,
//! KTD-1). Paper-only, operator-run, attended. It resolves LS credentials from a
//! lane env-file (or the process env), calls the typed `T1444Request` scoped to a
//! KOSPI `upcode`, freezes the top-N shcodes (server-sorted by market cap) into the
//! provenance-stamped `turn3-universe.json`, and validates the file before writing
//! — a fail-closed materialization `ls-ingest` then consumes via `LS_INGEST_SYMBOLS`.
//!
//! This does **not** promote `t1444` (KTD-1). Selecting by *current* market cap for
//! a *past* backtest window is a mild look-ahead, disclosed in the frozen file's
//! provenance.
//!
//! Configuration (env vars):
//! - `LS_TRADING_ENV=paper` (required — refuses otherwise).
//! - `LS_CAPTURE_OUT`: output path (default `lab/config/turn3-universe.json`).
//! - `LS_CAPTURE_UPCODE`: the 업종코드 (default `001`, the KOSPI composite).
//! - `LS_CAPTURE_N`: how many top names to freeze (default `30`).
//! - `LS_CAPTURE_SENTINEL`: a shcode that MUST appear in the top-N as a wrong-market
//!   guard (default `005930`, Samsung Electronics — always #1 KOSPI market cap).
//! - `LS_CAPTURE_LANE_FILE`: optional lane env-file (else the process env is used).

use ls_sdk::paginated::T1444Request;
use nautilus_ls::config::LsAdapterConfig;
use nautilus_ls::scrub;
use nautilus_ls::universe::{Provenance, UniverseFile, SOURCE_TR};

/// Default output path (relative to the adapter crate root / CWD).
const DEFAULT_OUT: &str = "lab/config/turn3-universe.json";
/// Default `upcode` — `001` is the KOSPI composite (코스피종합).
const DEFAULT_UPCODE: &str = "001";
const DEFAULT_N: usize = 30;
/// Default wrong-market sentinel: Samsung Electronics leads KOSPI market cap.
const DEFAULT_SENTINEL: &str = "005930";

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
    let upcode = std::env::var("LS_CAPTURE_UPCODE").unwrap_or_else(|_| DEFAULT_UPCODE.into());
    let n: usize = match std::env::var("LS_CAPTURE_N") {
        Ok(v) => v.parse().map_err(|_| format!("LS_CAPTURE_N must be a positive integer, got {v:?}"))?,
        Err(_) => DEFAULT_N,
    };
    let sentinel = std::env::var("LS_CAPTURE_SENTINEL").unwrap_or_else(|_| DEFAULT_SENTINEL.into());

    let adapter_cfg = match std::env::var("LS_CAPTURE_LANE_FILE") {
        Ok(path) => LsAdapterConfig::from_lane_file(path),
        Err(_) => LsAdapterConfig::from_env(),
    };
    let sdk = adapter_cfg.build_sdk()?;

    // The one live call: KOSPI top-market-cap ranking (server-sorted). A single page
    // holds far more than the top-30 we freeze.
    let resp = sdk.paginated().market_cap_top(&T1444Request::new(upcode.clone())).await?;
    if resp.outblock1.is_empty() {
        return Err(format!(
            "t1444 returned no rows (rsp_cd={}, rsp_msg={:?}) — cannot freeze an empty universe",
            resp.rsp_cd, resp.rsp_msg
        )
        .into());
    }

    // Take the top-N shcodes in returned (market-cap-descending) order, skipping any
    // blank/short codes defensively.
    let shcodes: Vec<String> = resp
        .outblock1
        .iter()
        .map(|r| r.shcode.trim().to_string())
        .filter(|s| !s.is_empty())
        .take(n)
        .collect();

    // Wrong-market guard (KTD-1): the KOSPI top-cap board must contain the sentinel.
    // A wrong `upcode` would silently capture some other market — refuse it here.
    if !sentinel.trim().is_empty() && !shcodes.iter().any(|s| s == sentinel.trim()) {
        return Err(format!(
            "wrong-market guard tripped: sentinel {sentinel:?} not in the top-{n} for upcode {upcode:?} \
             — is this really the KOSPI composite? (top codes: {:?})",
            &shcodes.iter().take(5).collect::<Vec<_>>()
        )
        .into());
    }

    let file = UniverseFile {
        provenance: Provenance {
            source_tr: SOURCE_TR.to_string(),
            upcode: upcode.clone(),
            upcode_label: upcode_label(&upcode),
            captured_at: chrono::Utc::now().to_rfc3339(),
            count: shcodes.len(),
            look_ahead_caveat:
                "Symbols selected by CURRENT market cap (t1444) for a PAST backtest window — a mild \
                 look-ahead, disclosed and accepted for a first decisive read (KTD-1). t1444 is not promoted."
                    .to_string(),
        },
        shcodes,
    };

    // Fail closed BEFORE writing: a file that would not pass the offline validator
    // must never land on disk.
    if let Err(errs) = file.validate() {
        return Err(format!("captured universe failed validation:\n  - {}", errs.join("\n  - ")).into());
    }

    let json = serde_json::to_string_pretty(&file)?;
    if let Some(parent) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }
    std::fs::write(&out, format!("{json}\n"))
        .map_err(|e| format!("writing {out}: {e}"))?;

    println!(
        "captured {} KOSPI top-market-cap shcodes (upcode {}) → {out}",
        file.shcodes.len(),
        upcode
    );
    println!("top 5: {:?}", &file.shcodes.iter().take(5).collect::<Vec<_>>());
    println!("LS_INGEST_SYMBOLS={}", file.ingest_symbols());
    Ok(())
}

fn upcode_label(upcode: &str) -> String {
    match upcode {
        "001" => "코스피종합 (KOSPI composite)".to_string(),
        other => format!("upcode {other}"),
    }
}

fn require_paper() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::var("LS_TRADING_ENV").as_deref() {
        Ok("paper") => Ok(()),
        _ => Err("refusing to run: set LS_TRADING_ENV=paper (this adapter is paper-only in v1)".into()),
    }
}
