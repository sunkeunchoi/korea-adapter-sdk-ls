//! `node_exec_tester` — operator-gated live execution smoke (U7).
//!
//! Paper-only, session-windowed. Runs a guarded submit → cancel round-trip on a
//! **resting, far-from-market** limit order, honouring the repo's order-safety
//! conventions (flat-start gate first, `post_order` dedup/no-retry/kill-switch
//! path, kill switch engaged only AFTER the closing cancel). Holds the R15
//! live-session lock. **Operator's call to run** — not run by the offline gate.
//!
//! Env: `LS_TRADING_ENV=paper` (required), `LS_NODE_LANE_FILE` (optional),
//! `LS_NODE_SYMBOL` (default `005930`), `LS_NODE_PRICE` (required — a safe resting
//! buy price BELOW market but within the daily band; the operator picks it),
//! `LS_NODE_MEMBER` (default `NXT`), `LS_NODE_LOCK_DIR` (default `.`).

use ls_sdk::orders::{CSPAT00601Request, CSPAT00801Request};
use nautilus_ls::config::LsAdapterConfig;
use nautilus_ls::execution::LsExecClient;
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_ls::scrub;
use nautilus_model::enums::AccountType;

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
    let account_no = sdk.orders().account_no().to_string();
    let client = LsExecClient::new("LS-KRX", cfg.trader_id.clone(), account_no, sdk.clone(), AccountType::Cash);

    // R14: refuse unless the account starts flat.
    client.verify_flat().await?;
    println!("flat-start gate passed");

    let symbol = std::env::var("LS_NODE_SYMBOL").unwrap_or_else(|_| "005930".into());
    let price = std::env::var("LS_NODE_PRICE")
        .map_err(|_| "LS_NODE_PRICE is required (a safe resting buy price below market)")?;
    let member = std::env::var("LS_NODE_MEMBER").unwrap_or_else(|_| "NXT".into());

    // The `IsuNo` must match between submit and cancel (both `A{symbol}`, the
    // production `submit_request` form) or the cancel cannot reference the order.
    let isuno = format!("A{symbol}");

    // Submit a resting BUY limit (bnstpcode "2") that should NOT fill.
    let submit = sdk
        .orders()
        .submit(&CSPAT00601Request::limit(&isuno, "1", &price, "2", &member))
        .await?;
    let ord_no = submit.order_no().to_string();
    println!("submitted: {}", scrub::scrub_secrets(&format!("ordno={ord_no}")));

    // Fail-CLOSED teardown: once an order is live, the cancel + kill switch must run
    // even if the cancel errors (a `?` early-return here would orphan the resting
    // order with the kill switch never engaged — the exact trap the repo's
    // kill-switch-ordering learning warns about). Retry the cancel, then always halt.
    let mut canceled = false;
    for attempt in 1..=3 {
        match sdk
            .orders()
            .cancel(&CSPAT00801Request::new(&ord_no, &isuno, "1"))
            .await
        {
            Ok(cancel) => {
                println!(
                    "canceled: {}",
                    scrub::scrub_secrets(&format!(
                        "parent={} new={}",
                        cancel.parent_order_no(),
                        cancel.order_no()
                    ))
                );
                canceled = true;
                break;
            }
            Err(e) => eprintln!(
                "cancel attempt {attempt} failed: {}",
                scrub::scrub_secrets(&e.to_string())
            ),
        }
    }

    // Confirm flat, then engage the kill switch AFTER the closing action — always,
    // regardless of whether the cancel succeeded.
    match client.verify_flat().await {
        Ok(()) => println!("flat confirmed after cancel"),
        Err(e) => eprintln!("WARNING: not flat after cancel: {}", scrub::scrub_secrets(&e.to_string())),
    }
    client.halt();
    println!("kill switch engaged (post-close)");

    if !canceled {
        return Err("cancel failed after retries — resting order may remain; kill switch engaged, \
                    operator must reconcile the paper account"
            .into());
    }
    Ok(())
}

fn require_paper() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::var("LS_TRADING_ENV").as_deref() {
        Ok("paper") => Ok(()),
        _ => Err("refusing to run: set LS_TRADING_ENV=paper (this adapter is paper-only in v1)".into()),
    }
}
