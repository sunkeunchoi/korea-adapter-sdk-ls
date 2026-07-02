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
//!
//! **Staged SC live probe (U8, R12).** Set `LS_NODE_SC_PROBE=1` to concurrently
//! subscribe the SC0/SC1 order-event lane during the guarded resting chain and print
//! a verdict (this leg certifies SC0 only — a resting order never fills). Set
//! `LS_NODE_SC_MARKETABLE=1` for the SC1 leg: a 1-lot marketable buy + sign-aware
//! close-out, **bypassing the U6 band guard** (the only way an SC1 fill frame is
//! observable). The verdict also records whether the gateway tolerated the exec
//! client's second concurrent WS session (KTD3).

use std::time::Duration;

use ls_sdk::market_session::T8450Request;
use ls_sdk::orders::{CSPAT00601Request, CSPAT00801Request};
use nautilus_ls::config::LsAdapterConfig;
use nautilus_ls::execution::LsExecClient;
use nautilus_ls::guard::check_resting_price;
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_ls::probe::{drain_observations, format_verdict, ProbeLeg, ScObservation};
use nautilus_ls::scrub;
use nautilus_ls::ws::rows::OrderEventMsg;
use nautilus_ls::ws::supervisor::{RowKind, SubSpec, WsSupervisor};
use nautilus_ls::KRX_VENUE;
use nautilus_model::enums::AccountType;
use nautilus_model::identifiers::{InstrumentId, Symbol, Venue};
use tokio::sync::mpsc;

/// Subscribe SC0/SC1 on the exec client's own order-event supervisor and report the
/// receiver + whether the second WS session established (KTD3).
async fn subscribe_sc(sdk: &ls_sdk::LsSdk) -> (WsSupervisor, mpsc::UnboundedReceiver<OrderEventMsg>, bool) {
    let (tx, rx) = mpsc::unbounded_channel::<OrderEventMsg>();
    let sup = WsSupervisor::spawn_order_events(sdk.clone(), tx);
    let placeholder = InstrumentId::new(Symbol::from("SC"), Venue::from(KRX_VENUE));
    for (tr_cd, kind) in [("SC0", RowKind::OrderAccept), ("SC1", RowKind::OrderFill)] {
        sup.subscribe(SubSpec {
            tr_cd: tr_cd.to_string(),
            tr_key: String::new(),
            instrument_id: placeholder,
            kind,
        });
    }
    // Give the two subscriptions a moment to register, then read connectivity.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let ok = sup.is_connected();
    (sup, rx, ok)
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    scrub::install();
    // Route the terminal error through the scrubber: `main() -> Result` would
    // otherwise let the runtime print an SDK error (raw broker `rsp_msg`, auth
    // strings) to stderr UNSCRUBBED, defeating the credential hygiene the bin
    // installs for every other output path.
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
    let member = std::env::var("LS_NODE_MEMBER").unwrap_or_else(|_| "NXT".into());
    // Safety opt-ins require the exact value "1" — an accidental `=0`/`=false` must
    // NOT arm a live order path (a bare `.is_ok()` would treat any value as enabled).
    let flag_on = |k: &str| std::env::var(k).as_deref() == Ok("1");
    let sc_probe = flag_on("LS_NODE_SC_PROBE");

    // SC1 leg (U8): a marketable buy + sign-aware close, bypassing the U6 guard —
    // the ONLY way to witness an SC1 fill frame. Separately env-gated (must be "1").
    if flag_on("LS_NODE_SC_MARKETABLE") {
        return run_marketable_probe(&sdk, &client, &symbol, &member).await;
    }

    let price = std::env::var("LS_NODE_PRICE")
        .map_err(|_| "LS_NODE_PRICE is required (a safe resting buy price below market)")?;

    // R14/U6 band guard: fetch t8450 and REFUSE before placing anything if the
    // operator price is marketable (≥ best ask) or outside the daily band. Fetch it
    // through the "N" (integrated) exchange view.
    let quote = sdk
        .market_session()
        .current_price_orderbook(&T8450Request::new(&symbol, "N"))
        .await?;
    let ob = &quote.outblock;
    match check_resting_price(&price, &ob.offerho1, &ob.dnlmtprice, &ob.uplmtprice) {
        Ok(p) => println!("band guard passed: resting price {p} is below the best ask and in-band"),
        Err(reason) => {
            return Err(format!("band guard refused (no order placed): {reason}").into());
        }
    }

    // SC0 leg (U8): subscribe the order-event lane before submitting so an accept
    // frame can be observed during the resting chain.
    let sc = if sc_probe { Some(subscribe_sc(&sdk).await) } else { None };

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

    // SC0 probe verdict (U8): drain any accept frames observed during the chain.
    if let Some((sup, mut rx, ws_ok)) = sc {
        let (accepts, fills) = drain_observations(&mut rx, Duration::from_secs(2)).await;
        let obs = ScObservation { sc0_accepts: accepts, sc1_fills: fills, second_ws_session_ok: ws_ok };
        println!("{}", format_verdict(ProbeLeg::Resting, &obs));
        sup.shutdown();
    }

    if !canceled {
        return Err("cancel failed after retries — resting order may remain; kill switch engaged, \
                    operator must reconcile the paper account"
            .into());
    }
    Ok(())
}

/// U8 SC1 leg: a 1-lot marketable buy (limited at 상한가 `uplmtprice`, so it fills),
/// an SC observation window, then a sign-aware marketable close (sell at 하한가
/// `dnlmtprice`). Bypasses the U6 band guard by design. Fail-closed teardown: the
/// close + kill switch always run.
async fn run_marketable_probe(
    sdk: &ls_sdk::LsSdk,
    client: &LsExecClient,
    symbol: &str,
    member: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let quote = sdk
        .market_session()
        .current_price_orderbook(&T8450Request::new(symbol, "N"))
        .await?;
    let ob = &quote.outblock;
    let upper = ob.uplmtprice.trim().to_string();
    let lower = ob.dnlmtprice.trim().to_string();
    if upper.is_empty() || lower.is_empty() {
        return Err("marketable probe: t8450 daily band unavailable — refusing".into());
    }
    let isuno = format!("A{symbol}");

    let (sup, mut rx, ws_ok) = subscribe_sc(sdk).await;

    // Marketable BUY (bnstpcode "2") priced at 상한가 → fills immediately. Do NOT
    // `?` this: an ambiguous/errored submit MAY have filled, so bailing here would
    // skip the close + kill switch (the exact fail-open the repo's kill-switch
    // learning warns about). Log and fall through to the always-run close teardown.
    match sdk
        .orders()
        .submit(&CSPAT00601Request::limit(&isuno, "1", &upper, "2", member))
        .await
    {
        Ok(buy) => println!("marketable buy submitted: {}", scrub::scrub_secrets(&format!("ordno={}", buy.order_no()))),
        Err(e) => eprintln!(
            "marketable buy failed/ambiguous (MAY have filled) — proceeding to close + halt: {}",
            scrub::scrub_secrets(&e.to_string())
        ),
    }

    // Observe the SC lane while the fill lands.
    let (accepts, fills) = drain_observations(&mut rx, Duration::from_secs(3)).await;

    // Sign-aware close: SELL 1 (bnstpcode "1") priced at 하한가 → fills immediately.
    // Fail-closed: retry the close, then always halt.
    let mut closed = false;
    for attempt in 1..=3 {
        match sdk
            .orders()
            .submit(&CSPAT00601Request::limit(&isuno, "1", &lower, "1", member))
            .await
        {
            Ok(sell) => {
                println!("marketable close submitted: {}", scrub::scrub_secrets(&format!("ordno={}", sell.order_no())));
                closed = true;
                break;
            }
            Err(e) => eprintln!("close attempt {attempt} failed: {}", scrub::scrub_secrets(&e.to_string())),
        }
    }

    match client.verify_flat().await {
        Ok(()) => println!("flat confirmed after close"),
        Err(e) => eprintln!("WARNING: not flat after close: {}", scrub::scrub_secrets(&e.to_string())),
    }
    client.halt();
    println!("kill switch engaged (post-close)");

    let obs = ScObservation { sc0_accepts: accepts, sc1_fills: fills, second_ws_session_ok: ws_ok };
    println!("{}", format_verdict(ProbeLeg::Marketable, &obs));
    sup.shutdown();

    if !closed {
        return Err("marketable close failed after retries — a position may remain; kill switch \
                    engaged, operator must reconcile the paper account"
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
