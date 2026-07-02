//! `node_data_tester` — operator-gated live data smoke (U7).
//!
//! Paper-only, session-windowed. Subscribes a liquid domestic symbol on the S3_
//! trade lane and prints scrubbed ticks for a bounded window, exercising the real
//! WS supervisor against the paper gateway. Holds the R15 live-session lock
//! (refuses while an ingest run is active). **Operator's call to run** — this is the
//! adapter's live certification path, not run by the offline gate.
//!
//! Env: `LS_TRADING_ENV=paper` (required), `LS_NODE_LANE_FILE` (optional lane file,
//! else process env), `LS_NODE_SYMBOL` (default `005930`), `LS_NODE_SECONDS`
//! (default `20`), `LS_NODE_LOCK_DIR` (default `.`).

use std::time::Duration;

use nautilus_ls::config::LsAdapterConfig;
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_ls::scrub;
use nautilus_ls::ws::supervisor::{RowKind, SubSpec, WsSupervisor};
use nautilus_model::identifiers::{InstrumentId, Symbol, Venue};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    scrub::install();
    require_paper()?;

    let lock_dir = std::env::var("LS_NODE_LOCK_DIR").unwrap_or_else(|_| ".".into());
    let _lock = AdvisoryLock::acquire(std::path::Path::new(&lock_dir), LockKind::Live)?;

    let cfg = match std::env::var("LS_NODE_LANE_FILE") {
        Ok(p) => LsAdapterConfig::from_lane_file(p),
        Err(_) => LsAdapterConfig::from_env(),
    };
    let sdk = cfg.build_sdk()?;

    let symbol = std::env::var("LS_NODE_SYMBOL").unwrap_or_else(|_| "005930".into());
    let seconds: u64 = std::env::var("LS_NODE_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    let (tx, mut rx) = mpsc::unbounded_channel();
    let sup = WsSupervisor::spawn(sdk, tx);
    let instrument_id =
        InstrumentId::new(Symbol::from(symbol.as_str()), Venue::from(nautilus_ls::KRX_VENUE));
    sup.subscribe(SubSpec {
        tr_cd: "S3_".to_string(),
        tr_key: symbol.clone(),
        instrument_id,
        kind: RowKind::Trade,
    });
    println!("subscribed S3_:{symbol}; printing ticks for {seconds}s (paper)");

    let deadline = tokio::time::sleep(Duration::from_secs(seconds));
    tokio::pin!(deadline);
    let mut ticks = 0usize;
    loop {
        tokio::select! {
            _ = &mut deadline => break,
            ev = rx.recv() => match ev {
                Some(ev) => {
                    ticks += 1;
                    // Structured events carry no credentials; scrub defensively anyway.
                    println!("tick {ticks}: {}", scrub::scrub_secrets(&format!("{ev:?}")));
                }
                None => break,
            }
        }
    }

    for stale in sup.never_delivered(Duration::from_secs(seconds.min(5))) {
        eprintln!("WARNING: never-delivered subscription: {stale}");
    }
    sup.shutdown();
    println!("done: {ticks} tick(s)");
    Ok(())
}

fn require_paper() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::var("LS_TRADING_ENV").as_deref() {
        Ok("paper") => Ok(()),
        _ => Err("refusing to run: set LS_TRADING_ENV=paper (this adapter is paper-only in v1)".into()),
    }
}
