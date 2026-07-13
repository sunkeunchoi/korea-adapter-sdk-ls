use super::*;


/// Guarded paper-order evidence matrix. `#[ignore]` — runs only via
/// `make live-smoke-order` with `.env` credentials and the explicit opt-ins.
///
/// Records Pending (not a failure) when the paper account cannot place an order
/// in-window, so the machinery still ships honestly (AE5). It never fails the
/// build for an environmental Pending.
#[tokio::test]
#[ignore = "guarded live paper order: needs credentials + LS_ORDER_SMOKE=1; run via `make live-smoke-order`"]
async fn order_smoke_matrix() {
    // Fail-closed selection BEFORE any SDK construction.
    let tr = select_order_tr().expect("explicit LS_ORDER_SMOKE_TR=CSPAT00601 required");
    assert_eq!(tr, ORDER_TR);
    let symbol = std::env::var("LS_ORDER_SMOKE_SHCODE").unwrap_or_else(|_| "005930".into());
    let member_no = std::env::var("LS_ORDER_SMOKE_MBRNO").unwrap_or_else(|_| "NXT".into());
    let params = match validate_params(&symbol, &member_no) {
        Ok(p) => p,
        Err(e) => {
            OrderEvidence::not_certified("preflight", &e).record();
            panic!("invalid operator params: {e}");
        }
    };

    let sdk = order_smoke_sdk().expect("both guards + paper config must succeed");

    // Fetch + validate the daily price band (KTD8). The band (uplmtprice /
    // dnlmtprice) is surfaced by t1102's out-block in this SDK (the plan named
    // t1101; t1101's order-book out-block does not carry the band).
    let band = match sdk
        .market_session()
        .quote(&T1102Request::new(&params.symbol, "K"))
        .await
    {
        Ok(resp) => match validate_band(&resp.outblock.uplmtprice, &resp.outblock.dnlmtprice) {
            Ok(b) => b,
            Err(e) => {
                // Degenerate band — record not-certified and stop (do not place).
                OrderEvidence::not_certified("band", &e).record();
                OrderEvidence::pending("matrix", "degenerate band; matrix not run").record();
                return;
            }
        },
        Err(e) => {
            OrderEvidence::pending("band", &format!("t1101 band fetch failed: {e}")).record();
            return;
        }
    };

    // Place each scenario, capturing the observed rsp_cd/rsp_msg for the predicate.
    let mut placed_any = false;
    for scenario in Scenario::all() {
        let req = build_order(scenario, &params, &band);
        let mut ev = OrderEvidence {
            tr_code: ORDER_TR.into(),
            scenario: scenario.as_str().into(),
            certification: Certification::Pending,
            rsp_cd: String::new(),
            rsp_msg: String::new(),
            order_no: None,
            reconciliation: None,
            production_not_run: true,
        };
        match sdk.orders().submit(&req).await {
            Ok(resp) => {
                placed_any = true;
                ev.certification = Certification::Certified;
                ev.rsp_cd = resp.rsp_cd.clone();
                ev.rsp_msg = resp.rsp_msg.clone();
                ev.order_no = Some(resp.order_no().to_string());
                // Reconcile resting orders via t0425.
                if matches!(scenario, Scenario::RestingBuy | Scenario::RestingSell) {
                    let intent = OrderIntent::submit(
                        sdk.orders().account_no().to_string(),
                        params.symbol.clone(),
                        req.inblock.bnstpcode.clone(),
                        req.inblock.ordqty.clone(),
                        req.inblock.ordprc.clone(),
                        Some(resp.order_no().to_string()),
                    );
                    let outcome = sdk.orders().reconcile(&intent, false).await;
                    ev.reconciliation = Some(outcome.state.as_str().to_string());
                    if outcome.state == OrderState::Accepted {
                        // Resting order rests as expected.
                    }
                }
            }
            Err(LsError::ApiError { code, message }) => {
                ev.rsp_cd = code.clone();
                ev.rsp_msg = message;
                // A deliberate rejection is the EXPECTED outcome here.
                ev.certification = if scenario == Scenario::DeliberateReject {
                    Certification::Certified
                } else if ls_core::is_paper_incompatible(&code)
                    || ls_core::is_paper_order_incapable(&code)
                {
                    // 01900 (service not in Paper) or 01491 (account not
                    // order-capable) → cannot prove order-capability; not evidence.
                    Certification::Pending
                } else {
                    Certification::Certified // a real broker code is valid evidence
                };
            }
            Err(LsError::AmbiguousOrder { code, message }) => {
                ev.rsp_cd = code;
                ev.rsp_msg = format!("ambiguous: {message}");
                ev.certification = Certification::Pending;
            }
            Err(e) => {
                ev.rsp_msg = format!("transport/other: {e}");
                ev.certification = Certification::Pending;
            }
        }
        ev.record();
    }

    if !placed_any {
        OrderEvidence::pending(
            "matrix",
            "paper account placed no order in-window (not order-capable / empty)",
        )
        .record();
    }

    // Teardown: resting orders are cleared by paper reset (the only verified
    // teardown — cancel TRs are deferred). An unexpected fill on a resting/
    // marketable order must be unwound out-of-band by the operator. The harness
    // records the observation; the operator owns the paper reset.
    println!(
        "ORDER-SMOKE teardown=paper-reset note=[operator must reset the paper book; \
         any unexpected fill is unwound out-of-band]"
    );
}

#[test]
fn chain_tr_allowlist_recognizes_modify_cancel_but_matrix_places_only_submit() {
    let saved = std::env::var("LS_ORDER_SMOKE_TR").ok();
    // The chain's order-class TRs are recognized (distinct message), but the
    // submit MATRIX still places only CSPAT00601.
    for tr in ["CSPAT00701", "CSPAT00801"] {
        std::env::set_var("LS_ORDER_SMOKE_TR", tr);
        let err = select_order_tr().expect_err("the matrix places only the submit TR");
        assert!(err.contains("modify/cancel"), "unexpected message: {err}");
    }
    std::env::set_var("LS_ORDER_SMOKE_TR", "NOTATR");
    assert!(select_order_tr().unwrap_err().contains("unsupported"));
    match saved {
        Some(v) => std::env::set_var("LS_ORDER_SMOKE_TR", v),
        None => std::env::remove_var("LS_ORDER_SMOKE_TR"),
    }
}

/// AUTONOMOUS chained paper-order run (U5): the agent invokes this directly during a
/// human-present wave — there is NO operator handoff. Submit a resting far-from-market
/// order (gate 1 evidence), modify it, then cancel it as teardown — each observed via
/// `t0425` — then assert the traded symbol is FLAT (U3). Cancel is the PRIMARY
/// teardown; the flat assertion + retry-cancel is the autonomous fallback when the
/// cancel link fails or a resting order remains; paper reset is the last resort.
///
/// Pending vs hard-fail (the autonomy trade, R3/AE3): when NOTHING is placed (out of
/// window, not order-capable / `01900` / `01491`, degenerate band) the run records
/// Pending and passes — no order is left resting. But ONCE an order is placed, a
/// still-resting order after retry-cancel, an unexpected fill, or a failed/ambiguous
/// flat scan HARD-FAILS the build (there is no operator to clean up) — autonomy trades
/// the human pre-placement checkpoint for loud post-run detection.
/// `#[ignore]` — runs only via `make live-smoke-order-chain`.
///
/// Autonomy invariants (all fail-closed):
/// - U1: refuses unless a CI/no-TTY marker is ABSENT and a fresh per-wave human nonce
///   (`LS_ORDER_SMOKE_NONCE=$(date +%s)`) is present and within TTL.
/// - U2: asserts the RESOLVED environment is paper after credential load.
/// - U3: after teardown, the traded-symbol working-order `t0425` scan must positively
///   confirm zero live rows; a resting remainder triggers retry-cancel then a loud
///   hard-fail, a
///   fill hard-fails immediately, and a failed/ambiguous scan is treated as NOT flat.
/// - U4: installs a fail-closed dispatch-log suppressor and scrubs all output.
///
/// OPERATIONAL NOTE: because U1 refuses without a TTY, the live run must be invoked in
/// an attended terminal context (a PTY) with a freshly-minted nonce — the autonomy
/// delivered is removal of the operator-handoff *protocol*, not unattended placement
/// (KTD1 / R1).
#[tokio::test]
#[ignore = "guarded chained paper order: needs credentials + LS_ORDER_SMOKE=1 + a fresh LS_ORDER_SMOKE_NONCE; run via `make live-smoke-order-chain`"]
async fn order_chained_smoke() {
    // U4: install the fail-closed dispatch-log suppressor BEFORE any dispatch, so the
    // unscrubbed ls_core whole-body/`rsp_msg` debug events are dropped for this run.
    if let Err(e) = install_dispatch_log_suppressor() {
        panic!("{}", scrub_secrets(&e.to_string()));
    }

    let symbol = std::env::var("LS_ORDER_SMOKE_SHCODE").unwrap_or_else(|_| "005930".into());
    let member_no = std::env::var("LS_ORDER_SMOKE_MBRNO").unwrap_or_else(|_| "NXT".into());
    let params = match validate_params(&symbol, &member_no) {
        Ok(p) => p,
        Err(e) => {
            OrderEvidence::not_certified("preflight", &e).record();
            panic!("invalid operator params: {}", scrub_secrets(&e));
        }
    };

    // U1+U2: autonomy precondition (CI/no-TTY + fresh nonce) and paper-resolved SDK.
    // A refusal places nothing and emits no order evidence — only the scrubbed reason.
    let sdk = match autonomous_order_smoke_sdk() {
        Ok(s) => s,
        Err(e) => panic!("{}", scrub_secrets(&e.to_string())),
    };
    let account = sdk.orders().account_no().to_string();

    // Fetch + validate the daily band (KTD8); a degenerate band records not-certified.
    let band = match sdk
        .market_session()
        .quote(&T1102Request::new(&params.symbol, "K"))
        .await
    {
        Ok(resp) => match validate_band(&resp.outblock.uplmtprice, &resp.outblock.dnlmtprice) {
            Ok(b) => b,
            Err(e) => {
                OrderEvidence::not_certified("band", &e).record();
                OrderEvidence::pending("chain", "degenerate band; chain not run").record();
                return;
            }
        },
        Err(e) => {
            OrderEvidence::pending("band", &format!("t1102 band fetch failed: {}", scrub_secrets(&e.to_string())))
                .record();
            return;
        }
    };

    // ---- SUBMIT leg (gate 1 evidence): a resting buy at the band floor. ----
    let resting_price = band.resting_buy_price();
    let submit_req =
        CSPAT00601Request::limit(&params.symbol, "1", resting_price.to_string(), "2", &params.member_no);
    let mut sev = leg_evidence("CSPAT00601", "submit_resting_buy");
    let submit_ordno = match sdk.orders().submit(&submit_req).await {
        Ok(resp) => {
            sev.certification = Certification::Certified;
            sev.rsp_cd = resp.rsp_cd.clone();
            sev.rsp_msg = resp.rsp_msg.clone();
            sev.order_no = Some(resp.order_no().to_string());
            let intent = OrderIntent::submit(
                &account,
                params.symbol.clone(),
                submit_req.inblock.bnstpcode.clone(),
                submit_req.inblock.ordqty.clone(),
                submit_req.inblock.ordprc.clone(),
                Some(resp.order_no().to_string()),
            );
            sev.reconciliation = Some(sdk.orders().reconcile(&intent, false).await.state.as_str().into());
            sev.record();
            resp.order_no().to_string()
        }
        Err(LsError::ApiError { code, message })
            if ls_core::is_paper_incompatible(&code)
                || ls_core::is_paper_order_incapable(&code) =>
        {
            // 01900 (service not in Paper) or 01491 (account provisioned
            // read/inquiry-only) — the request reached the gateway and was
            // cleanly rejected; nothing placed, so the chain cannot run.
            let reason = format!("paper account not order-capable ({code}); chain not run");
            sev.rsp_cd = code;
            sev.rsp_msg = message;
            sev.record();
            OrderEvidence::pending("chain", &reason).record();
            return;
        }
        Err(e) => {
            // The catch-all submit error — including `LsError::AmbiguousOrder`, which
            // means the order MAY have reached the gateway (an ambiguous send is
            // reconciled, never assumed not-placed). With no operator to clean up, run
            // the traded-symbol flat assertion BEFORE recording Pending: a resting order
            // is retry-canceled then hard-failed naming it; a clean transport failure
            // (nothing placed) positively confirms flat and falls through to Pending
            // (R3/R5). The proven-not-placed arm above (01900/01491) is the only
            // post-submit path that may skip this.
            sev.rsp_msg = format!("submit failed: {}", scrub_secrets(&e.to_string()));
            sev.record();
            assert_account_flat(&sdk, &params.symbol).await;
            OrderEvidence::pending(
                "chain",
                "submit leg did not cleanly place; account confirmed flat; chain not run",
            )
            .record();
            return;
        }
    };
    if submit_ordno.trim().is_empty() || submit_ordno.trim() == "0" {
        OrderEvidence::pending("chain", "submit returned no usable order number; chain not run")
            .record();
        return;
    }

    // ---- MODIFY leg: amend the resting order to a new (still far-from-market)
    // price. The modify is absolute (full target), KTD4. ----
    let modify_price = band.dnlmt.saturating_add(tick(band.dnlmt)).min(band.uplmt);
    let modify_req = CSPAT00701Request::limit(&submit_ordno, &params.symbol, "1", modify_price.to_string());
    let mut mev = leg_evidence("CSPAT00701", "modify_resting");
    let live_ordno = match sdk.orders().modify(&modify_req).await {
        Ok(resp) => {
            mev.certification = Certification::Certified;
            mev.rsp_cd = resp.rsp_cd.clone();
            mev.rsp_msg = resp.rsp_msg.clone();
            mev.order_no = Some(resp.order_no().to_string());
            let intent = modify_req.reconcile_intent(&account);
            mev.reconciliation = Some(sdk.orders().reconcile(&intent, false).await.state.as_str().into());
            mev.record();
            // PIN the replace shape (KTD4) from this NON-ambiguous read.
            dump_t0425_rows(&sdk, &params.symbol, &submit_ordno).await;
            // The live order to cancel: the modify's NEW order number if present,
            // else the original (an in-place modify).
            let n = resp.order_no().to_string();
            if n.trim().is_empty() || n.trim() == "0" { submit_ordno.clone() } else { n }
        }
        Err(e) => {
            // Gate 1 already flipped on the submit leg; gate 2 stays Pending.
            mev.rsp_msg = format!("modify failed: {}", scrub_secrets(&e.to_string()));
            mev.record();
            OrderEvidence::pending("chain", "modify link failed; gate 2 not flipped (gate 1 stands)")
                .record();
            // Still tear down the resting submit order via cancel below.
            submit_ordno.clone()
        }
    };

    // ---- CANCEL leg (PRIMARY teardown + gate 2 evidence). ----
    let cancel_req = CSPAT00801Request::new(&live_ordno, &params.symbol, "1");
    let mut cev = leg_evidence("CSPAT00801", "cancel_teardown");
    match sdk.orders().cancel(&cancel_req).await {
        Ok(resp) => {
            cev.certification = Certification::Certified;
            cev.rsp_cd = resp.rsp_cd.clone();
            cev.rsp_msg = resp.rsp_msg.clone();
            cev.order_no = Some(resp.order_no().to_string());
            let intent = cancel_req.reconcile_intent(&account);
            let outcome = sdk.orders().reconcile(&intent, false).await;
            cev.reconciliation = Some(outcome.state.as_str().into());
            cev.record();
            if outcome.state != OrderState::Canceled {
                // The cancel acked but the book is not provably clean — fall back to
                // paper reset and flag for review (inverted-risk guard, R7).
                println!(
                    "ORDER-CHAIN warning=[cancel acked but t0425 not provably 취소: {}] \
                     teardown=paper-reset",
                    outcome.state.as_str()
                );
            }
        }
        Err(e) => {
            // The cancel link itself failed → gate 2 does not flip; gate 1 is
            // unaffected (AE5). Do NOT return — fall through to the traded-symbol flat
            // assertion (U3), which retry-cancels the still-resting order and either
            // clears the book or hard-fails loudly naming it.
            cev.rsp_msg = format!("cancel failed: {}", scrub_secrets(&e.to_string()));
            cev.record();
            OrderEvidence::pending(
                "chain",
                "cancel link failed; gate 2 not flipped; flat assertion will clean up or hard-fail",
            )
            .record();
        }
    }

    // U3 (R3/R4, KTD2/KTD3): assert the traded symbol is FLAT after the run. This is
    // the autonomous replacement for the operator's out-of-band paper reset — a
    // resting remainder is retry-canceled then hard-failed, a fill hard-fails
    // immediately, and a failed scan is treated as NOT flat. Scoped to the traded
    // symbol (the only symbol the chain places on) so the scan completes on a
    // heavily-used paper account instead of overrunning the history-page cap.
    assert_account_flat(&sdk, &params.symbol).await;

    println!(
        "ORDER-CHAIN teardown=cancel+flat-assert note=[cancel is the primary teardown; the \
         traded-symbol working-order flat assertion confirms no order remains resting]"
    );
}

/// PAPER-RESET (attended, PAPER-ONLY): flatten the domestic paper book — cancel every
/// resting order and close every long stock position — then positively confirm flat.
///
/// This exists because the order smokes can leave a resting order and, when a marketable
/// scenario fills, a real long position that teardown cannot unwind; without this the
/// operator resets the paper book by hand. It places REAL paper orders (marketable
/// sells), so it reuses the EXACT order-smoke fail-closed chain — NOT a looser one:
/// `autonomous_order_smoke_sdk` layers U1 (no CI/no-TTY marker + a FRESH per-wave human
/// nonce within TTL) and U2 (paper-resolved after credential load) ahead of the standing
/// `LS_TRADING_ENV=paper` + `LS_ORDER_SMOKE=1` double opt-in; U4 installs the fail-closed
/// dispatch-log suppressor and every output line is scrubbed. A refusal on any gate
/// places nothing.
///
/// Order of operations (fail-closed, positive confirmation only):
///  a. CANCEL: account-wide `t0425` (chegb="2", unfilled) → `CSPAT00801` each resting
///     remainder (`ordrem > 0`). A cancel that fails is logged (scrubbed) and the flat
///     re-scan catches whatever remains.
///  b. CLOSE: `t0424` holdings → for each sellable position (`mdposqt > 0`) fetch its
///     `t1102` band and place a MARKETABLE `CSPAT00601` SELL at the floor.
///  c. VERIFY: bounded re-scan of `t0425` + `t0424`; `PAPER-RESET ... flat=confirmed`
///     when both are clean, else `flat=not-yet remaining=[...]` naming what is left
///     (never loops forever). A failed scan is treated as NOT flat.
///
/// `#[ignore]` — runs only via `make paper-reset` in an attended PTY with a fresh nonce.
#[tokio::test]
#[ignore = "guarded PAPER-ONLY book reset: places real marketable sells; needs credentials + LS_ORDER_SMOKE=1 + a fresh LS_ORDER_SMOKE_NONCE; run via `make paper-reset`"]
async fn paper_reset() {
    // U4: suppress the unscrubbed ls_core dispatch debug events BEFORE any dispatch.
    if let Err(e) = install_dispatch_log_suppressor() {
        panic!("{}", scrub_secrets(&e.to_string()));
    }

    // The member-firm number for the flatten SELL (same default as the order smokes).
    let member_no = std::env::var("LS_ORDER_SMOKE_MBRNO").unwrap_or_else(|_| "NXT".into());
    if member_no.trim().is_empty() {
        panic!("paper-reset refuses: empty member number (set LS_ORDER_SMOKE_MBRNO)");
    }

    // U1 + U2: the SAME fail-closed autonomy chain as the order legs (nonce + no-TTY +
    // paper-resolved). A refusal places nothing and emits only the scrubbed reason.
    let sdk = match autonomous_order_smoke_sdk() {
        Ok(s) => s,
        Err(e) => panic!("{}", scrub_secrets(&e.to_string())),
    };

    // ---- (a) Cancel every resting working order (account-wide, unfilled). ----
    // Reuse the order-chain working-order scan with an EMPTY symbol filter → all symbols;
    // chegb="2" keeps the set to the currently-unfilled (resting) orders, and the scan
    // fails CLOSED on a paginated cursor. A failed scan is fatal (positive confirmation
    // only — we must know the resting set before placing any sell).
    let working = match scan_symbol_working_orders(&sdk, "").await {
        Ok(rows) => rows,
        Err(e) => panic!("{}", loud_failure("paper-reset-scan-failed", &[], &e)),
    };
    let mut canceled = 0usize;
    for r in working
        .iter()
        .filter(|r| parse_qty(&r.ordrem) > 0)
    {
        let cancel = CSPAT00801Request::new(r.ordno.trim(), r.expcode.trim(), r.ordrem.trim());
        match sdk.orders().cancel(&cancel).await {
            Ok(_) => {
                canceled += 1;
                println!("PAPER-RESET cancel ordno={} result=acked", r.ordno.trim());
            }
            Err(e) => println!(
                "PAPER-RESET cancel ordno={} result=[{}]",
                r.ordno.trim(),
                scrub_secrets(&e.to_string())
            ),
        }
    }

    // ---- (b) Close every sellable long via a marketable SELL. ----
    let holdings = match sdk
        .account()
        .stock_balance(&T0424Request::new("1", "0", "0", "0"))
        .await
    {
        Ok(resp) => resp.outblock1,
        Err(e) => panic!(
            "{}",
            loud_failure("paper-reset-holdings-failed", &[], &e.to_string())
        ),
    };
    let mut closed = 0usize;
    // §30 follow-up: set when a close is rejected `01458` (모의투자 장종료 / paper session
    // closed) — the reset is well-formed but KRX is closed, so the operator should retry
    // at the next open. Surfaced in the witness so "market closed" is distinguishable from
    // a real remediation failure. Not a hard-fail: the flat re-scan still runs best-effort.
    let mut market_closed = false;
    for close in holdings_to_sells(&holdings) {
        // Price the marketable sell off THIS symbol's daily band (floor). A degenerate
        // band (halted / limit-locked / newly-listed) cannot be priced safely, so skip
        // it — the flat re-scan will report it as still-remaining rather than place on a
        // bad band.
        let band = match sdk
            .market_session()
            .quote(&T1102Request::new(&close.symbol, "K"))
            .await
        {
            Ok(resp) => match validate_band(&resp.outblock.uplmtprice, &resp.outblock.dnlmtprice) {
                Ok(b) => b,
                Err(e) => {
                    println!(
                        "PAPER-RESET close symbol={} skipped=[degenerate band: {}]",
                        close.symbol,
                        scrub_secrets(&e)
                    );
                    continue;
                }
            },
            Err(e) => {
                println!(
                    "PAPER-RESET close symbol={} skipped=[band fetch failed: {}]",
                    close.symbol,
                    scrub_secrets(&e.to_string())
                );
                continue;
            }
        };
        let req = build_marketable_sell(&close.symbol, close.qty, &band, &member_no);
        match sdk.orders().submit(&req).await {
            Ok(resp) => {
                closed += 1;
                println!(
                    "PAPER-RESET close symbol={} qty={} ordno={} result=acked",
                    close.symbol,
                    close.qty,
                    resp.order_no()
                );
            }
            Err(e) if is_market_closed(&e) => {
                market_closed = true;
                println!(
                    "PAPER-RESET close symbol={} qty={} skipped=[market-closed 01458 — retry at next open]",
                    close.symbol, close.qty
                );
            }
            Err(e) => println!(
                "PAPER-RESET close symbol={} qty={} result=[{}]",
                close.symbol,
                close.qty,
                scrub_secrets(&e.to_string())
            ),
        }
    }

    // ---- (c) Verify flat: bounded re-scan of t0425 + t0424 (never loops forever). ----
    const MAX_VERIFY_ATTEMPTS: usize = 3;
    let mut flat = false;
    let mut remaining: Vec<String> = Vec::new();
    for attempt in 1..=MAX_VERIFY_ATTEMPTS {
        // A short settle so a just-placed marketable sell can fill / a cancel can clear
        // before the confirming read (the scan itself also paces the t0425 budget).
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        let w = scan_symbol_working_orders(&sdk, "").await;
        let h = sdk
            .account()
            .stock_balance(&T0424Request::new("1", "0", "0", "0"))
            .await;
        match (w, h) {
            (Ok(w), Ok(h)) => {
                if book_is_flat(&w, &h.outblock1) {
                    flat = true;
                    break;
                }
                remaining = describe_remaining(&w, &h.outblock1);
                println!(
                    "PAPER-RESET verify attempt={attempt} flat=not-yet remaining=[{}]",
                    remaining.join(",")
                );
            }
            (w, h) => {
                // A failed scan is NOT flat (positive confirmation only). Record a
                // scrubbed reason and keep the remaining note honest.
                let why = match (&w, &h) {
                    (Err(e), _) => format!("t0425 scan failed: {}", scrub_secrets(&e.to_string())),
                    (_, Err(e)) => format!("t0424 read failed: {}", scrub_secrets(&e.to_string())),
                    _ => "unknown".into(),
                };
                remaining = vec![format!("scan:{why}")];
                println!("PAPER-RESET verify attempt={attempt} flat=not-yet {why}");
            }
        }
    }

    // ONE credential-free witness line (no token/appkey/secret/account/rsp_msg): counts +
    // flat status, and on not-yet the bare remaining identifiers (public, not secrets).
    if flat {
        println!("PAPER-RESET canceled={canceled} closed={closed} flat=confirmed");
    } else {
        // A market-closed close is a session-timing condition, not a failure — name it
        // so the operator reads "retry at next open" rather than a broken reset. not-yet
        // stays a pass (best-effort, no hard-fail).
        let reason = if market_closed { " reason=market-closed" } else { "" };
        println!(
            "PAPER-RESET canceled={canceled} closed={closed} flat=not-yet{reason} remaining=[{}]",
            remaining.join(",")
        );
    }
}
