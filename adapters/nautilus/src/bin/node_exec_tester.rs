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
//!
//! **SC certification (U3/U6, KTD-5).** `LS_NODE_SC_CERTIFY=1` runs a marketable 1-lot
//! buy and witnesses the SAME fill through **both** the SC1 frame and the t0425 poll via
//! one production ledger — proving the exactly-once dedup collapse + capturing `cheprice`
//! and `execprc`. A `CERTIFIED` verdict authorizes `LS_NODE_SC_PRIMARY=1` (U6); this is
//! the leg that produces U6's activation evidence, which the bare `LS_NODE_SC_MARKETABLE`
//! frame count cannot.
//!
//! **Flatten-only recovery.** `LS_NODE_CLOSE_ONLY=1` sells out a residual holding on
//! `LS_NODE_SYMBOL` (a probe whose auto-close did not net) and confirms flat. Runs before
//! the flat-start gate, re-reads the sellable qty before every submit (never oversells),
//! and never buys. Use it to clear a stuck position after a marketable-probe run.
//!
//! **SC-primary mode (U4/U6, KTD-5).** `LS_NODE_SC_PRIMARY=1` (off by default) relaxes
//! the t0425 poll to the fail-closed backstop cadence
//! ([`nautilus_ls::execution::SC_PRIMARY_BACKSTOP_CADENCE`]) so SC push-fills carry the
//! fill path and the poll is a slow reconcile safety net. **Only** set it after the U3
//! live probe files a *certifying* verdict (SC1 frames observed, 2nd WS session
//! tolerated, live cross-source dedup + positive `execprc` witnessed) — otherwise poll
//! stays authoritative. Off = byte-identical to today.

use std::time::Duration;

use ls_sdk::account::T0424Request;
use ls_sdk::market_session::T8450Request;
use ls_sdk::orders::{CSPAT00601Request, CSPAT00801Request, T0425Request};
use nautilus_ls::config::LsAdapterConfig;
use nautilus_ls::execution::{resolve_poll_cadence, LsExecClient};
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
    // SC-primary selector (U4/U6, KTD-5): OFF by default → identical to today (poll
    // authoritative). Set `LS_NODE_SC_PRIMARY=1` — only AFTER the U3 live probe files a
    // certifying verdict (SC1 frames + 2nd-WS tolerated + live cross-source dedup) — to
    // relax the t0425 poll to the fail-closed backstop cadence and let SC carry fills.
    // Must be the exact value "1" so an accidental `=0`/`=false` cannot arm it.
    let sc_primary = std::env::var("LS_NODE_SC_PRIMARY").as_deref() == Ok("1");
    let client = LsExecClient::new("LS-KRX", cfg.trader_id.clone(), account_no, sdk.clone(), AccountType::Cash)
        .with_poll_cadence(resolve_poll_cadence(sc_primary));

    let symbol = std::env::var("LS_NODE_SYMBOL").unwrap_or_else(|_| "005930".into());
    let member = std::env::var("LS_NODE_MEMBER").unwrap_or_else(|_| "NXT".into());
    // Safety opt-ins require the exact value "1" — an accidental `=0`/`=false` must
    // NOT arm a live order path (a bare `.is_ok()` would treat any value as enabled).
    let flag_on = |k: &str| std::env::var(k).as_deref() == Ok("1");

    // Flatten-only recovery (R14 recovery): sell out a residual holding the account is
    // carrying — e.g. a marketable-probe run whose auto-close did not net. Runs BEFORE
    // the flat-start gate precisely because the account is NOT flat here; the gate would
    // otherwise abort. Fail-closed and it never buys.
    if flag_on("LS_NODE_CLOSE_ONLY") {
        return run_close_only(&sdk, &client, &symbol, &member).await;
    }

    // R14: refuse unless the account starts flat.
    client.verify_flat().await?;
    println!("flat-start gate passed");

    let sc_probe = flag_on("LS_NODE_SC_PROBE");

    // SC CERTIFY leg (U3/U6, KTD-5): a marketable buy whose SAME fill is witnessed via
    // BOTH the SC1 frame and the t0425 poll through one production ledger, so the
    // exactly-once dedup collapse + cheprice + execprc are observed live. A CERTIFIED
    // line authorizes flipping `LS_NODE_SC_PRIMARY=1` (U6).
    if flag_on("LS_NODE_SC_CERTIFY") {
        return run_certify_probe(&sdk, &client, &symbol, &member).await;
    }

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

/// SC certification witness (U3/U6, KTD-5): drive a marketable 1-lot buy and observe the
/// SAME execution through **both** the SC1 push frame **and** the t0425 poll, feeding one
/// production [`FillLedger`] so the exactly-once dedup collapse is witnessed live — the
/// evidence a bare frame count (the `LS_NODE_SC_MARKETABLE` leg) cannot provide. Reports,
/// for the fill: SC1 frame seen + `execprc` positive (KTD-5); poll row seen + whether
/// `cheprice` populated positive or fell back (U3 #3); and that the two sources collapsed
/// to exactly ONE `FillDelta` (KTD-5 dedup). Then sign-aware closes and confirms flat. A
/// `CERTIFIED` verdict here is what authorizes flipping `LS_NODE_SC_PRIMARY=1` (U6).
///
/// The poll leg uses **ord_no-targeted presence detection** (scan the returned page for
/// the known order's fill row) rather than the production poll's absence-proving
/// truncation fail-close, so it can still corroborate a fill on a heavily-traded symbol
/// whose `chegb="0"` history paginates — and if the row is genuinely not on the page it
/// reports `poll_saw_fill=false` (NOT certified) honestly rather than hanging.
async fn run_certify_probe(
    sdk: &ls_sdk::LsSdk,
    client: &LsExecClient,
    symbol: &str,
    member: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use nautilus_ls::orders::ledger::{FillLedger, FillObservation};
    use nautilus_ls::ws::rows::OrderEventMsg;
    use nautilus_model::enums::{OrderSide, OrderType, TimeInForce};
    use nautilus_model::identifiers::{ClientOrderId, InstrumentId};
    use nautilus_model::orders::OrderTestBuilder;
    use nautilus_model::types::{Price, Quantity};
    use std::sync::{Arc, Mutex};

    let quote = sdk
        .market_session()
        .current_price_orderbook(&T8450Request::new(symbol, "N"))
        .await?;
    let upper = quote.outblock.uplmtprice.trim().to_string();
    let lower = quote.outblock.dnlmtprice.trim().to_string();
    if upper.is_empty() || lower.is_empty() {
        return Err("certify probe: t8450 daily band unavailable — refusing".into());
    }
    let limit_price: i64 = upper.parse().unwrap_or(0);
    let isuno = format!("A{symbol}");

    let (sup, mut rx, ws_ok) = subscribe_sc(sdk).await;
    let ledger = Arc::new(Mutex::new(FillLedger::new()));
    // Emission-identity order (the ledger keys dedup on the OrdNo chain, not this).
    let order = OrderTestBuilder::new(OrderType::Limit)
        .instrument_id(InstrumentId::from(format!("{symbol}.XKRX").as_str()))
        .client_order_id(ClientOrderId::from("SC-CERT-1"))
        .side(OrderSide::Buy)
        .quantity(Quantity::from(1))
        .price(Price::from(upper.as_str()))
        .time_in_force(TimeInForce::Day)
        .build();

    // Marketable BUY at 상한가 → fills. Do NOT `?` — an ambiguous submit MAY have filled,
    // so fall through to the always-run close + halt.
    let ord_no = match sdk
        .orders()
        .submit(&CSPAT00601Request::limit(&isuno, "1", &upper, "2", member))
        .await
    {
        Ok(buy) => {
            let n = buy.order_no().to_string();
            println!("certify buy submitted: {}", scrub::scrub_secrets(&format!("ordno={n}")));
            Some(n)
        }
        Err(e) => {
            eprintln!(
                "certify buy failed/ambiguous (MAY have filled) — proceeding to close: {}",
                scrub::scrub_secrets(&e.to_string())
            );
            None
        }
    };

    let mut sc_frames = 0usize;
    let mut sc_deltas = 0usize;
    let mut sc_execprc_positive = false;
    let mut poll_saw_fill = false;
    let mut poll_deltas = 0usize;
    let mut cheprice_populated = false;

    if let Some(ord_no) = ord_no.clone() {
        ledger.lock().expect("ledger mutex poisoned").register(order, ord_no.clone());

        // (a) SC witness: apply this order's SC fill frames to the shared ledger (~4s).
        let drain = async {
            while let Some(msg) = rx.recv().await {
                if let OrderEventMsg::Fill(obs) = msg {
                    if obs.ord_no.trim() == ord_no.trim() {
                        sc_frames += 1;
                        if obs.price > 0 {
                            sc_execprc_positive = true;
                        }
                        let out = ledger.lock().expect("ledger mutex poisoned").apply(obs);
                        sc_deltas += out.deltas.len();
                    }
                }
            }
        };
        let _ = tokio::time::timeout(Duration::from_secs(4), drain).await;

        // (b) Poll witness: one paced t0425 read; ord_no-targeted (tolerates truncation).
        tokio::time::sleep(Duration::from_millis(1500)).await;
        match sdk.orders().inquiry(&T0425Request::for_symbol(symbol)).await {
            Ok(scan) => {
                if let Some(row) = scan.outblock1.iter().find(|r| r.ordno.trim() == ord_no.trim()) {
                    let cheqty = row.cheqty.trim().parse::<i64>().unwrap_or(0);
                    let cheprice = row.cheprice.trim().parse::<i64>().unwrap_or(0);
                    if cheqty > 0 {
                        poll_saw_fill = true;
                        cheprice_populated = cheprice > 0;
                        // Mirror the production poll seam (poll.rs KTD4): cheprice when
                        // positive, else the limit price with price_approximated set.
                        let (price, approx) =
                            if cheprice > 0 { (cheprice, false) } else { (limit_price, true) };
                        let out = ledger.lock().expect("ledger mutex poisoned")
                            .apply(FillObservation::poll(&ord_no, cheqty, price, approx));
                        poll_deltas += out.deltas.len();
                    }
                }
            }
            Err(e) => eprintln!("certify poll read failed: {}", scrub::scrub_secrets(&e.to_string())),
        }
    }

    // Sign-aware close (SELL 1 at 하한가) + confirm flat + always halt.
    let mut closed = false;
    for attempt in 1..=3 {
        match sdk
            .orders()
            .submit(&CSPAT00601Request::limit(&isuno, "1", &lower, "1", member))
            .await
        {
            Ok(sell) => {
                println!("certify close submitted: {}", scrub::scrub_secrets(&format!("ordno={}", sell.order_no())));
                closed = true;
                break;
            }
            Err(e) => eprintln!("certify close attempt {attempt} failed: {}", scrub::scrub_secrets(&e.to_string())),
        }
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    let flat = client.verify_flat().await;
    match &flat {
        Ok(()) => println!("certify: flat confirmed after close"),
        Err(e) => eprintln!("WARNING: not flat after close: {}", scrub::scrub_secrets(&e.to_string())),
    }
    client.halt();
    sup.shutdown();

    // Verdict: the KTD-5 bar is BOTH sources witnessing the fill AND collapsing to one delta.
    let total_deltas = sc_deltas + poll_deltas;
    let dedup_ok = sc_frames > 0 && poll_saw_fill && total_deltas == 1;
    let certified = dedup_ok && sc_execprc_positive && ws_ok;
    println!(
        "SC CERTIFY: sc1_frames={sc_frames} sc_execprc_positive={sc_execprc_positive} \
         poll_saw_fill={poll_saw_fill} cheprice_populated={cheprice_populated} \
         total_fill_deltas={total_deltas} dedup_collapsed_to_one={dedup_ok} \
         2nd_ws_tolerated={ws_ok} => {}",
        if certified {
            "CERTIFIED — safe to set LS_NODE_SC_PRIMARY=1 (U6)"
        } else {
            "NOT CERTIFIED — poll stays authoritative"
        }
    );

    // Fail-closed (mirrors run_marketable_probe): this leg placed a REAL marketable buy, so
    // a failed close or a not-flat post-close state must exit NON-ZERO — never `Ok(())` with
    // a leaked live position hidden behind the verdict/WARNING line, whatever the SC verdict.
    if !closed || flat.is_err() {
        return Err("certify: account may not be flat after close (a position may remain) — \
                    kill switch engaged, operator must reconcile the paper account"
            .into());
    }
    Ok(())
}

/// Flatten-only recovery: sell out a residual holding on `symbol` (a probe whose
/// auto-close did not net — R14 recovery). Fail-closed and it **never buys**:
///
/// - Re-reads the sellable quantity from `t0424` before **every** submit, so it can
///   never oversell/flip short — a flat book (`janqty == 0`) stops it immediately, and a
///   held-but-not-yet-sellable lot (`mdposqt == 0`, pending settlement) waits rather than
///   forcing an order.
/// - Sells the sellable balance at 하한가 (`dnlmtprice`, marketable into the bid).
/// - Loops until the holding nets away, then confirms with `verify_flat`; if it is still
///   not flat after the attempts, it bails **loudly** with the kill switch engaged so an
///   operator finishes by hand — never a silent partial close.
async fn run_close_only(
    sdk: &ls_sdk::LsSdk,
    client: &LsExecClient,
    symbol: &str,
    member: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let isuno = format!("A{symbol}");
    // Cumulative sell cap (the hard no-oversell invariant): never let the total submitted
    // sell quantity across the run exceed the FIRST-seen held balance. Re-reading `mdposqt`
    // each pass is the primary guard, but if an accepted sell has not yet reserved sellable
    // qty within the 2s settle window a bare re-read could re-submit the full size; this cap
    // bounds cumulative submissions regardless of settlement lag, so the run can never flip
    // short even if the gateway's sellable accounting lags.
    let mut initial_balance: Option<i64> = None;
    let mut sold_submitted: i64 = 0;
    for attempt in 1..=4 {
        // Re-read holdings BEFORE each submit — the no-oversell invariant.
        let holdings = sdk
            .account()
            .stock_balance(&T0424Request::new("1", "0", "0", "0"))
            .await?;
        // Diagnostic (symbol + share qty only — not secrets): show exactly what the
        // account is carrying so a residual that doesn't match `symbol` is visible.
        for r in &holdings.outblock1 {
            println!(
                "close-only holding: expcode={} janqty={} mdposqt={}",
                r.expcode.trim(),
                r.janqty.trim(),
                r.mdposqt.trim()
            );
        }
        // Match the holding row tolerant of the market-prefix ("005930" or "A005930").
        let row = holdings
            .outblock1
            .iter()
            .find(|r| r.expcode.trim().trim_start_matches('A') == symbol);
        let balance = row.and_then(|r| r.janqty.trim().parse::<i64>().ok()).unwrap_or(0);
        let sellable = row.and_then(|r| r.mdposqt.trim().parse::<i64>().ok()).unwrap_or(0);
        // First-seen held balance is the cumulative cap for the whole run.
        let cap = *initial_balance.get_or_insert(balance);
        if balance <= 0 {
            println!("close-only: {symbol} already flat (no balance) — nothing to sell");
            break;
        }
        // Never submit more than (cap - already-submitted): bounds the run against an
        // accepted-but-not-yet-reserved sell that a bare `mdposqt` re-read would double.
        let qty = sellable.min((cap - sold_submitted).max(0));
        if qty <= 0 {
            eprintln!(
                "close-only attempt {attempt}: {symbol} holds {balance} but 0 sellable within \
                 the cap ({sold_submitted}/{cap} already submitted) — waiting for fills to settle"
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        }
        let quote = sdk
            .market_session()
            .current_price_orderbook(&T8450Request::new(symbol, "N"))
            .await?;
        let lower = quote.outblock.dnlmtprice.trim().to_string();
        if lower.is_empty() {
            return Err("close-only: t8450 lower band (하한가) unavailable — refusing".into());
        }
        // Count `qty` against the cap the moment we submit — an ambiguous (Err) submit MAY
        // have sold, so it must consume the cap too (fail-closed: under-selling and leaving a
        // loudly-surfaced residual is safe; re-selling an ambiguous fill and flipping short is not).
        sold_submitted += qty;
        // Marketable SELL (bnstpcode "1") of the capped sellable balance at 하한가.
        match sdk
            .orders()
            .submit(&CSPAT00601Request::limit(&isuno, qty.to_string(), &lower, "1", member))
            .await
        {
            Ok(sell) => println!(
                "close-only sell submitted: {}",
                scrub::scrub_secrets(&format!("ordno={} qty={qty}", sell.order_no()))
            ),
            Err(e) => eprintln!(
                "close-only sell attempt {attempt} failed/ambiguous (MAY have sold): {}",
                scrub::scrub_secrets(&e.to_string())
            ),
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // Fail-closed final verdict, then always halt.
    let result: Result<(), Box<dyn std::error::Error>> = match client.verify_flat().await {
        Ok(()) => {
            println!("close-only: flat confirmed");
            Ok(())
        }
        Err(e) => Err(format!(
            "close-only: account NOT flat after retries — manual intervention required: {}",
            scrub::scrub_secrets(&e.to_string())
        )
        .into()),
    };
    client.halt();
    println!("kill switch engaged (post-close)");
    result
}

fn require_paper() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::var("LS_TRADING_ENV").as_deref() {
        Ok("paper") => Ok(()),
        _ => Err("refusing to run: set LS_TRADING_ENV=paper (this adapter is paper-only in v1)".into()),
    }
}
