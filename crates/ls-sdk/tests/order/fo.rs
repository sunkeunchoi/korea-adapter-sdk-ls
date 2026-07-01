use super::*;

// ===========================================================================
// F/O order chain (CFOAT00100 submit → CFOAT00200 modify → CFOAT00300 cancel).
// U3 (plan 2026-06-30-003): the domestic futures/options sibling of the stock
// chain. Reuses the autonomy / paper-resolved / leak-suppressor / scrub guards
// verbatim (KTD5). The F/O-specific pieces are:
//   - a daily-limit price anchor from t2111 (KTD2) — F/O prices are FRACTIONAL,
//   - a TWO-PART, fail-closed flatness check (KTD3): t0441 detects a FILL (잔고),
//     and the clean cancel response confirms the resting order's removal (no F/O
//     미체결 read exists to scan the working board).
// `#[ignore]`; runs only via `make live-smoke-fo-order`.
// ===========================================================================

// ---- F/O daily price band (KTD2) — fractional, sourced from t2111 -----------

/// A validated F/O daily price band from `t2111` (`uplmtprice` 상한가 / `dnlmtprice`
/// 하한가). Prices are FRACTIONAL (e.g. `342.25`), unlike the integer stock [`Band`].
/// The verbatim gateway strings are preserved as the resting-order anchors so the
/// limit price is a guaranteed valid tick (re-deriving an on-tick F/O price would need
/// the per-product tick ladder).
#[derive(Debug, Clone)]
struct FoBand {
    uplmt: f64,
    dnlmt: f64,
    uplmt_str: String,
    dnlmt_str: String,
}

/// Validate a `t2111` F/O band: both prices parse as `f64`, are strictly positive, and
/// `up > dn`. A degenerate band (halted / limit-locked / newly-listed / empty quote)
/// is rejected so the smoke records "not certified" and places NO order — a missing
/// anchor must NEVER fall back to a near-market (fillable) price (KTD2 fail-closed).
fn validate_fo_band(uplmtprice: &str, dnlmtprice: &str) -> Result<FoBand, String> {
    let up_s = uplmtprice.trim();
    let dn_s = dnlmtprice.trim();
    let up: f64 = up_s
        .parse()
        .map_err(|_| format!("unparseable F/O uplmtprice '{up_s}'"))?;
    let dn: f64 = dn_s
        .parse()
        .map_err(|_| format!("unparseable F/O dnlmtprice '{dn_s}'"))?;
    if !(up > 0.0) || !(dn > 0.0) {
        return Err(format!("degenerate F/O band (non-positive): up={up} dn={dn}"));
    }
    if up <= dn {
        return Err(format!("degenerate F/O band (up<=dn): up={up} dn={dn}"));
    }
    Ok(FoBand {
        uplmt: up,
        dnlmt: dn,
        uplmt_str: up_s.to_string(),
        dnlmt_str: dn_s.to_string(),
    })
}

impl FoBand {
    /// Resting BUY price — the daily floor (`dnlmtprice`): valid yet far below market,
    /// so it rests unfilled. Returned verbatim (a guaranteed valid tick).
    fn resting_buy_price(&self) -> &str {
        &self.dnlmt_str
    }
    /// Resting SELL price — the daily ceiling (`uplmtprice`): valid yet far above
    /// market.
    fn resting_sell_price(&self) -> &str {
        &self.uplmt_str
    }
}

// ---- F/O flatness over t0441 balance rows (KTD3, fill-detection only) --------

/// The F/O flatness verdict from a `t0441` (선물옵션잔고평가) balance read. `t0441`
/// sees a **filled** position (잔고), NOT a resting unfilled order — so this is only
/// the FILL half of the two-part flatness check (the other half is a clean cancel
/// response confirming the resting order's removal). A t0441 read that genuinely FAILS
/// is the caller's `Err` arm (panics `fo-flat-scan-failed`); an `Ok` read reaching this
/// verdict is a successful "your positions are: <rows>" answer, so an empty row set
/// means no position (R6, revised from the plan's AE3 "positions=0 row" assumption to
/// the observed empty-array reality — see [`fo_flat_verdict`]).
#[derive(Debug, Clone, PartialEq, Eq)]
enum FoFlatVerdict {
    /// No F/O position: either no balance rows at all (t0441 returns an empty array on a
    /// position-less account) or rows whose every quantity is zero. Positively confirms
    /// no fill — the caller trusts it only after a CLEAN cancel already removed the
    /// resting order.
    Flat,
    /// One or more FILLED positions (`jqty != 0`) — unrecoverable by cancel (paper
    /// reset is the sole remediation). Carries the position contract codes.
    Fill(Vec<String>),
}

/// `true` if a `t0441` quantity field is a non-zero (or unparseable) position. F/O
/// 잔고 can be SHORT (negative), so this keys on `!= 0` via `f64`, not a `u64` parse
/// that would silently drop a `-1`. An UNPARSEABLE value is treated as non-zero
/// (fail-safe: an unreadable quantity is assumed to be a position, never silently
/// zero).
fn fo_qty_is_position(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() || t == "0" {
        return false;
    }
    match t.parse::<f64>() {
        Ok(v) => v != 0.0,
        Err(_) => true,
    }
}

/// Classify a `t0441` balance-row set into an F/O flatness verdict (KTD3, R6).
fn fo_flat_verdict(rows: &[T0441OutBlock1]) -> FoFlatVerdict {
    // No row with a non-zero quantity = no F/O position = Flat. t0441 (선물/옵션 잔고평가)
    // returns an EMPTY array on a position-less account (confirmed live 2026-07-01, both
    // operator runs) — NOT a zero-qty row — so empty must read as Flat, or the
    // always-flat chain (unfillable daily-limit rest + clean cancel) could never certify.
    // This is NOT fail-OPEN: a genuinely failed/unreadable t0441 read is the caller's
    // `Err` arm (panics `fo-flat-scan-failed`), so an `Ok` empty read here is a real "no
    // positions" answer; a FILL (full or partial) still surfaces as a non-empty,
    // non-zero-`jqty` row → `Fill`; and the caller only consults this after a CLEAN
    // cancel, which itself proves the order was resting/unfilled (a filled order cannot
    // be canceled).
    let fills: Vec<String> = rows
        .iter()
        .filter(|r| fo_qty_is_position(&r.jqty))
        .map(|r| r.expcode.trim().to_string())
        .collect();
    if fills.is_empty() {
        FoFlatVerdict::Flat
    } else {
        FoFlatVerdict::Fill(fills)
    }
}

// ---- F/O per-leg certification predicate (R6, guards the U4 flip decision) ---

/// The F/O order-ack codes, PER LEG — all three CONFIRMED from the operator's 2026-07-01
/// in-window live runs: submit 00040 (buy) / 00039 (sell), modify 00462, cancel 00463. The
/// F/O family shares the domestic-stock chain's ack codes exactly. (The plan's raw-example
/// seeds 00132/00156 for modify/cancel were both disproven by the live runs — the first run
/// showed cancel 00463, and once the modify was fixed to a valid quantity reduction the
/// second and third runs both acked modify 00462.) Kept leg-specific so a submit that returns
/// a modify/cancel code (a gateway anomaly) is NOT certified as a clean submit. `ls_core`'s
/// runtime accept gate (`classify_order_rsp_cd`) is intentionally a coarser union across all
/// order TRs — it only decides retry/dedup safety; per-leg certification is the stricter
/// offline check the U4/U6 flip relies on.
const FO_SUBMIT_OK: [&str; 2] = ["00040", "00039"]; // buy / sell (confirmed live 2026-07-01)
const FO_MODIFY_OK: [&str; 1] = ["00462"]; // confirmed live 2026-07-01 (was seed 00132)
const FO_CANCEL_OK: [&str; 1] = ["00463"]; // confirmed live 2026-07-01 (was seed 00156)

/// Per-leg certification (R6): a leg certifies ONLY on a genuinely clean row — the
/// `rsp_cd` is in that LEG's accepted set AND a usable order number is present. A
/// generic-success envelope (`00000`/empty), a wrong-leg or unrecognized code, or a
/// missing order-number block is NOT certified. Keyed on the business code + order
/// number, never status text (PR #74 shipped a status-text cert bug that only review
/// caught). This is the offline-tested decision the operator's U4/U6 run exercises live.
fn fo_leg_certified(expected: &[&str], rsp_cd: &str, order_no: &str) -> bool {
    let o = order_no.trim();
    expected.contains(&rsp_cd.trim()) && !o.is_empty() && o != "0"
}

/// Format a credential-free `t0441` row diagnostic line. Every field — including the
/// row's own `expcode`/`appamt` (NOT just `rsp_msg`) — is routed through
/// [`scrub_secrets`], because a balance row can carry account-shaped values (R6).
fn fo_t0441_row_line(r: &T0441OutBlock1) -> String {
    format!(
        "ORDER-CHAIN-FO t0441-row expcode=[{}] jqty={} cqty={} appamt=[{}]",
        scrub_secrets(&r.expcode),
        scrub_secrets(&r.jqty),
        scrub_secrets(&r.cqty),
        scrub_secrets(&r.appamt),
    )
}

/// Two-part-flatness FILL half (KTD3 part 1): read `t0441` and assert NO filled F/O
/// position remains. A `Fill` verdict (a non-zero position) OR a genuinely failed t0441
/// read raises a LOUD operator-action-required signal (panic, the operator run's failure
/// channel) naming the order numbers; the kill switch is engaged as a no-new-orders
/// guard. An `Ok` read with no non-zero position (empty array or all-zero rows) records a
/// positive `Flat` pass (R6) — the resting order was already removed by the clean cancel.
async fn fo_assert_no_fill(sdk: &LsSdk, ordnos: &[String]) {
    match sdk.account().fo_balance_eval(&T0441Request::new()).await {
        Ok(resp) => {
            for r in &resp.outblock1 {
                println!("{}", fo_t0441_row_line(r));
            }
            match fo_flat_verdict(&resp.outblock1) {
                FoFlatVerdict::Flat => {
                    println!(
                        "ORDER-CHAIN-FO flat=confirmed scan=t0441 note=[zero-position; no fill]"
                    );
                }
                FoFlatVerdict::Fill(syms) => {
                    sdk.inner().set_orders_enabled(false);
                    panic!(
                        "{}",
                        loud_failure(
                            "fo-unexpected-fill",
                            ordnos,
                            &format!(
                                "t0441 shows a filled F/O position on [{}] — a fill cannot be \
                                 canceled; reset the paper book and flatten manually",
                                syms.join(",")
                            ),
                        )
                    );
                }
            }
        }
        Err(e) => {
            sdk.inner().set_orders_enabled(false);
            panic!(
                "{}",
                loud_failure(
                    "fo-flat-scan-failed",
                    ordnos,
                    &format!("t0441 read failed: {}", scrub_secrets(&e.to_string())),
                )
            );
        }
    }
}

/// AUTONOMOUS-guarded F/O chained paper-order run (U3/U4): submit a resting
/// far-from-market F/O order (gate 1 evidence), modify it, then cancel it as the
/// PRIMARY teardown — each leg certified from its OWN `rsp_cd` (R6/R7) — then run the
/// two-part flatness check (clean cancel = removal; `t0441` = no fill).
///
/// Inherits every guard from [`order_chained_smoke`] verbatim (KTD5): the U1 autonomy
/// precondition (CI/no-TTY refusal + fresh per-wave nonce), the U2 resolved-paper
/// assertion, the U4 fail-closed dispatch-log suppressor, and the widened
/// [`scrub_secrets`]. The F/O contract is self-sourced at runtime from the t8467
/// index-futures master (front-month) so it is never stale — an explicit
/// `LS_FO_ORDER_SMOKE_SHCODE` override still wins when a specific contract is wanted
/// (the current-contract gotcha, deps).
///
/// Capability outcomes (R8): `01491`/`01900` → Pending, classified by
/// `is_paper_order_incapable` only when `01491`; any OTHER rejection code is recorded
/// VERBATIM and explicitly NOT assumed to be paper-order-incapable. A clean run leaves
/// per-leg evidence the operator (U4/U6) flips from; the offline portions below prove
/// the decision logic without placing anything.
///
/// `#[ignore]` — runs only via `make live-smoke-fo-order`.
#[tokio::test]
#[ignore = "guarded F/O chained paper order: needs credentials + LS_ORDER_SMOKE=1 + a fresh LS_ORDER_SMOKE_NONCE (contract self-sourced from t8467; optional LS_FO_ORDER_SMOKE_SHCODE override); run via `make live-smoke-fo-order`"]
async fn fo_order_chained_smoke() {
    // U4: install the fail-closed dispatch-log suppressor BEFORE any dispatch (incl. the
    // t2111 price-band and t0441 reads, whose raw bodies carry account data).
    if let Err(e) = install_dispatch_log_suppressor() {
        panic!("{}", scrub_secrets(&e.to_string()));
    }

    // U1+U2: autonomy precondition + paper-resolved SDK. A refusal places nothing —
    // built FIRST so a no-TTY / unattended / non-paper run refuses before any network
    // I/O (including the contract-source read below).
    let sdk = match autonomous_order_smoke_sdk() {
        Ok(s) => s,
        Err(e) => panic!("{}", scrub_secrets(&e.to_string())),
    };
    // NB: the F/O surface has no t0425-style reconcile (no F/O 미체결 read), so unlike
    // the stock chain this run never builds a reconcile intent — flatness is t0441
    // fill-detection + clean-cancel removal. The account number is therefore not needed.

    // The current valid F/O contract. A hardcoded contract goes stale (the current-
    // contract gotcha), so it is NEVER defaulted — it is self-sourced at runtime from
    // the t8467 index-futures master (gubun "Q", front-month = first row), the SAME
    // current-contract source the F/O read/chart smokes use (mirrors
    // `fo_front_month_shcode` in live_smoke.rs). An explicit `LS_FO_ORDER_SMOKE_SHCODE`
    // override still wins when the operator wants to pin a specific contract. A missing /
    // empty master certifies nothing and places NO order (records Pending, R7).
    let contract = match std::env::var("LS_FO_ORDER_SMOKE_SHCODE") {
        Ok(v)
            if !v.trim().is_empty()
                && v.trim().chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            v.trim().to_string()
        }
        _ => match sdk
            .market_session()
            .index_futures_master(&T8467Request::new("Q"))
            .await
        {
            Ok(m) if !m.outblock.is_empty() => m.outblock[0].shcode.trim().to_string(),
            Ok(m) => {
                OrderEvidence::not_certified(
                    "preflight",
                    &format!(
                        "t8467 index-futures master returned no contract (rsp_cd={}); placed nothing",
                        m.rsp_cd
                    ),
                )
                .record();
                OrderEvidence::pending("fo-chain", "no F/O contract to key the chain; placed nothing")
                    .record();
                return;
            }
            Err(e) => {
                OrderEvidence::not_certified(
                    "preflight",
                    &format!("t8467 contract source failed: {}", scrub_secrets(&e.to_string())),
                )
                .record();
                OrderEvidence::pending("fo-chain", "F/O contract source unavailable; placed nothing")
                    .record();
                return;
            }
        },
    };
    // Defensive: a self-sourced contract must still be a plain alphanumeric code before
    // it keys any order (the master could in principle return an unexpected shape).
    if contract.is_empty() || !contract.chars().all(|c| c.is_ascii_alphanumeric()) {
        OrderEvidence::not_certified(
            "preflight",
            "resolved F/O contract is empty / non-alphanumeric; placed nothing",
        )
        .record();
        OrderEvidence::pending("fo-chain", "no valid F/O contract; placed nothing").record();
        return;
    }

    // Price anchor: the daily limits from t2111 (KTD2). FAIL-CLOSED — if no valid,
    // non-empty anchor can be sourced, place NO order.
    let band = match sdk
        .market_session()
        .fo_quote(&T2111Request::new(&contract))
        .await
    {
        Ok(resp) => match validate_fo_band(&resp.outblock.uplmtprice, &resp.outblock.dnlmtprice) {
            Ok(b) => b,
            Err(e) => {
                OrderEvidence::not_certified("band", &e).record();
                OrderEvidence::pending("fo-chain", "no valid F/O price anchor; placed nothing")
                    .record();
                return;
            }
        },
        Err(e) => {
            OrderEvidence::pending(
                "band",
                &format!("t2111 F/O anchor fetch failed: {}", scrub_secrets(&e.to_string())),
            )
            .record();
            return;
        }
    };

    // ---- SUBMIT leg (gate 1 evidence): a resting buy at the daily floor. Qty is 2 so the
    // MODIFY leg can exercise a valid quantity REDUCTION (2 → 1); F/O 정정 cannot INCREASE
    // qty. Args: (contract, ordqty="2", price, bnstpcode="2"=buy). ----
    let submit_req = CFOAT00100Request::limit(&contract, "2", band.resting_buy_price(), "2");
    let mut sev = leg_evidence("CFOAT00100", "fo_submit_resting_buy");
    let submit_ordno = match sdk.fo_orders().submit(&submit_req).await {
        Ok(resp) => {
            sev.certification = if fo_leg_certified(&FO_SUBMIT_OK, &resp.rsp_cd, resp.order_no()) {
                Certification::Certified
            } else {
                Certification::Pending
            };
            sev.rsp_cd = resp.rsp_cd.clone();
            sev.rsp_msg = resp.rsp_msg.clone();
            sev.order_no = Some(resp.order_no().to_string());
            sev.record();
            resp.order_no().to_string()
        }
        Err(LsError::ApiError { code, message })
            if ls_core::is_paper_order_incapable(&code) =>
        {
            // 01491 — the account is provisioned read/inquiry-only. Classified (R8).
            sev.rsp_cd = code.clone();
            sev.rsp_msg = message;
            sev.reconciliation = Some("classified:paper-order-incapable".into());
            sev.record();
            OrderEvidence::pending(
                "fo-chain",
                &format!("paper account not F/O-order-capable ({code}, classified); chain not run"),
            )
            .record();
            return;
        }
        Err(LsError::ApiError { code, message }) if ls_core::is_paper_incompatible(&code) => {
            // 01900 — service not provided in Paper.
            sev.rsp_cd = code.clone();
            sev.rsp_msg = message;
            sev.record();
            OrderEvidence::pending(
                "fo-chain",
                &format!("F/O order service not in Paper ({code}); chain not run"),
            )
            .record();
            return;
        }
        Err(LsError::ApiError { code, message }) => {
            // R8: ANY OTHER paper-order rejection (e.g. a venue-not-provisioned code).
            // Recorded VERBATIM and explicitly NOT assumed to be paper-order-incapable —
            // `is_paper_order_incapable` is not consulted for a non-01491 code.
            sev.rsp_cd = code.clone();
            sev.rsp_msg = message;
            sev.reconciliation = Some("unclassified-rejection".into());
            sev.record();
            OrderEvidence::pending(
                "fo-chain",
                &format!(
                    "F/O submit rejected ({code}); recorded verbatim, NOT classified as \
                     paper-order-incapable; chain not run"
                ),
            )
            .record();
            return;
        }
        Err(e) => {
            // Ambiguous / transport: the order MAY have reached the gateway and be RESTING
            // now, with NO order number to cancel it — and t0441 sees fills only, never a
            // resting order (KTD3). So this is fail-closed LOUD, not a silent Pending: since
            // `fo_flat_verdict` now reads an empty t0441 as Flat (no fill), an empty read here
            // must NOT be mistaken for "board is clear" — a resting order would pass silently.
            // Engage the kill switch, best-effort check for a fill, then hard-fail so the
            // operator does a manual board check + flatten. Mirrors the accepted-but-no-order-
            // number path below (never assume an ambiguous submit placed nothing).
            sev.rsp_msg = format!("submit failed: {}", scrub_secrets(&e.to_string()));
            sev.record();
            sdk.inner().set_orders_enabled(false);
            fo_assert_no_fill(&sdk, &[]).await;
            panic!(
                "{}",
                loud_failure(
                    "fo-submit-ambiguous",
                    &[],
                    "F/O submit failed AMBIGUOUSLY (transport/ambiguous error) — an order MAY \
                     rest uncancelable (no order number; t0441 sees fills only, and an empty \
                     t0441 no longer implies an empty order board); MANUAL board check + \
                     flatten required",
                )
            );
        }
    };
    if submit_ordno.trim().is_empty() || submit_ordno.trim() == "0" {
        // An ACCEPTED submit with no usable order number is AMBIGUOUS-PLACED: an order
        // may now be resting that the harness cannot reference to cancel. Do NOT silently
        // record Pending and return (fail-OPEN) — engage the kill switch, best-effort
        // check for a fill, then hard-fail loud so the operator does a manual board check
        // + flatten (fail-closed; t0441 sees fills only, never a resting order).
        sdk.inner().set_orders_enabled(false);
        fo_assert_no_fill(&sdk, &[]).await;
        panic!(
            "{}",
            loud_failure(
                "fo-submit-no-ordno",
                &[],
                "F/O submit was ACCEPTED but returned no usable order number — an order may rest \
                 uncancelable; MANUAL board check + flatten required",
            )
        );
    }

    // ---- MODIFY leg: REDUCE the resting order's QUANTITY from 2 → 1 (price stays at the
    // valid floor tick, so it stays far-from-market and cannot fill). The modify is
    // absolute (full target qty), KTD4, and MUST be a reduction: F/O 정정 cannot INCREASE
    // quantity above the resting qty (a larger target is rejected 01442 정정수량 초과 —
    // proven by the 2026-07-01 live run). Changing quantity rather than price avoids
    // re-deriving an on-tick F/O price near the floor. ----
    let modify_req = CFOAT00200Request::limit(&submit_ordno, &contract, "1", band.resting_buy_price());
    let mut mev = leg_evidence("CFOAT00200", "fo_modify_resting_qty");
    let (live_ordno, resting_qty, modify_uncertain) = match sdk.fo_orders().modify(&modify_req).await {
        Ok(resp) => {
            let certified = fo_leg_certified(&FO_MODIFY_OK, &resp.rsp_cd, resp.order_no());
            mev.certification = if certified {
                Certification::Certified
            } else {
                Certification::Pending
            };
            mev.rsp_cd = resp.rsp_cd.clone();
            mev.rsp_msg = resp.rsp_msg.clone();
            mev.order_no = Some(resp.order_no().to_string());
            // The F/O modify echoes the parent in OutBlock1.OrgOrdNo (no PrntOrdNo).
            mev.reconciliation = Some(format!("parent_ordno={}", resp.parent_order_no()));
            mev.record();
            let n = resp.order_no().to_string();
            if certified {
                // A CLEANLY certified modify transitioned the resting order to the NEW
                // order number at the modified quantity (now 1) — cancel that.
                (n, "1".to_string(), false)
            } else {
                // Ok but NOT cleanly certified (wrong-leg/unrecognized code, or empty/zero
                // new order number): it is ambiguous which order is now live. Best-effort
                // cancel the known order, but force a loud teardown-uncertain hard-fail at
                // the end — never conclude success from an ambiguous modify.
                let live = if n.trim().is_empty() || n.trim() == "0" {
                    submit_ordno.clone()
                } else {
                    n
                };
                (live, "1".to_string(), true)
            }
        }
        Err(e) => {
            // Gate 1 already flipped on the submit leg; gate 2 stays Pending. The original
            // submit order is untouched and still rests at the submit quantity (2).
            mev.rsp_msg = format!("modify failed: {}", scrub_secrets(&e.to_string()));
            mev.record();
            OrderEvidence::pending(
                "fo-chain",
                "F/O modify link failed; gate 2 not flipped (gate 1 stands)",
            )
            .record();
            (submit_ordno.clone(), "2".to_string(), false)
        }
    };

    // ---- CANCEL leg (PRIMARY teardown + gate 2 evidence). A CLEAN cancel response is
    // the only confirmation of the resting order's removal — no F/O 미체결 read exists
    // to scan the board (KTD3 part 2). ----
    let cancel_req = CFOAT00300Request::new(&live_ordno, &contract, &resting_qty);
    let mut cev = leg_evidence("CFOAT00300", "fo_cancel_teardown");
    let cancel_clean = match sdk.fo_orders().cancel(&cancel_req).await {
        Ok(resp) => {
            let ok = fo_leg_certified(&FO_CANCEL_OK, &resp.rsp_cd, resp.order_no());
            cev.certification = if ok {
                Certification::Certified
            } else {
                Certification::Pending
            };
            cev.rsp_cd = resp.rsp_cd.clone();
            cev.rsp_msg = resp.rsp_msg.clone();
            cev.order_no = Some(resp.order_no().to_string());
            cev.record();
            ok
        }
        Err(e) => {
            cev.rsp_msg = format!("cancel failed: {}", scrub_secrets(&e.to_string()));
            cev.record();
            OrderEvidence::pending("fo-chain", "F/O cancel link failed; gate 2 not flipped")
                .record();
            false
        }
    };

    // KTD3 teardown gate (fail-closed): the resting order's removal is confirmed ONLY by
    // a CLEAN cancel response — there is no F/O 미체결 read to scan the board, and t0441
    // sees FILLS only, never a resting order. So a non-clean cancel OR an ambiguous modify
    // (which leaves it unclear which order is live) means removal is UNCONFIRMABLE. In
    // either case do NOT proceed to the success line: engage the kill switch, best-effort
    // check for a fill, then hard-fail loud so the operator does a manual board check +
    // flatten. (A clean cancel of a cleanly-certified order is the only success path.)
    let teardown_uncertain = modify_uncertain || !cancel_clean;
    if teardown_uncertain {
        sdk.inner().set_orders_enabled(false);
        fo_assert_no_fill(&sdk, &[submit_ordno.clone(), live_ordno.clone()]).await;
        panic!(
            "{}",
            loud_failure(
                "fo-teardown-uncertain",
                &[submit_ordno.clone(), live_ordno.clone()],
                "F/O teardown could not confirm the resting order was removed (non-clean cancel \
                 or ambiguous modify) — t0441 sees fills only, no F/O 미체결 read exists; MANUAL \
                 board check + flatten required",
            )
        );
    }

    // Two-part flatness — FILL half (KTD3 part 1): t0441 must show no filled F/O position
    // (empty array or all-zero rows = Flat). Raises operator-action-required on a Fill or
    // a failed t0441 read.
    fo_assert_no_fill(&sdk, &[submit_ordno.clone(), live_ordno.clone()]).await;

    println!(
        "ORDER-CHAIN-FO teardown=cancel+t0441-fill-assert note=[clean-cancel confirms resting-\
         order removal; t0441 confirms no fill; F/O flatness is two-part and fail-closed]"
    );
}

// ===========================================================================
// F/O offline fail-closed tests (run in the normal suite)
// ===========================================================================

// ---- KTD2: fail-closed fractional price anchor ---------------------------

#[test]
fn fo_band_rejects_degenerate_and_keeps_verbatim_anchor() {
    assert!(validate_fo_band("0", "0").is_err(), "zero band");
    assert!(validate_fo_band("342.30", "342.30").is_err(), "up==dn (limit-locked)");
    assert!(validate_fo_band("100", "200").is_err(), "up<dn inverted");
    assert!(validate_fo_band("nan", "1").is_err(), "unparseable");
    assert!(validate_fo_band("", "1").is_err(), "empty anchor → no order");
    // A healthy fractional F/O band: the resting BUY anchor is the daily floor, verbatim
    // (a guaranteed valid tick), and sits strictly below the ceiling.
    let band = validate_fo_band("1214.75", "1034.85").expect("a healthy F/O band");
    assert_eq!(band.resting_buy_price(), "1034.85");
    assert_eq!(band.resting_sell_price(), "1214.75");
    assert!(band.dnlmt < band.uplmt);
}

// ---- KTD3/R6: two-part flatness — t0441 fill detection -------------------

/// A `t0441` balance-row helper for the F/O flat-verdict tests.
fn fo_bal_row(expcode: &str, jqty: &str) -> T0441OutBlock1 {
    T0441OutBlock1 {
        expcode: expcode.into(),
        jqty: jqty.into(),
        ..Default::default()
    }
}

#[test]
fn fo_flat_verdict_fill_blocks_and_empty_or_zero_is_flat() {
    // Covers R6. jqty>0 → Fill (block the flip); a SHORT position (negative) is also a Fill.
    assert_eq!(
        fo_flat_verdict(&[fo_bal_row("101T9000", "2")]),
        FoFlatVerdict::Fill(vec!["101T9000".into()]),
    );
    assert_eq!(
        fo_flat_verdict(&[fo_bal_row("101T9000", "-1")]),
        FoFlatVerdict::Fill(vec!["101T9000".into()]),
        "a short F/O position is still a fill"
    );
    // Empty array = no F/O position = Flat. t0441 returns an empty array on a
    // position-less account (confirmed live 2026-07-01); a genuinely failed READ is the
    // caller's Err arm, never an Ok-empty, so empty here positively confirms no fill.
    assert_eq!(fo_flat_verdict(&[]), FoFlatVerdict::Flat);
    // Zero-position rows present → positively Flat.
    assert_eq!(
        fo_flat_verdict(&[fo_bal_row("101T9000", "0"), fo_bal_row("105V3000", "0")]),
        FoFlatVerdict::Flat,
    );
    // An unparseable quantity is treated as a position (fail-safe), never silently zero.
    assert_eq!(
        fo_flat_verdict(&[fo_bal_row("101T9000", "??")]),
        FoFlatVerdict::Fill(vec!["101T9000".into()]),
    );
}

// ---- R6: per-leg certification predicate (guards the U4 flip decision) ----

#[test]
fn fo_leg_certifies_only_on_clean_rows() {
    // Covers R6. Clean success per leg (recognized ack for THAT leg + order number).
    assert!(fo_leg_certified(&FO_SUBMIT_OK, "00040", "69007"), "buy submit ack");
    assert!(fo_leg_certified(&FO_SUBMIT_OK, "00039", "69007"), "sell submit ack");
    assert!(fo_leg_certified(&FO_MODIFY_OK, "00462", "69041"), "F/O modify ack (confirmed live)");
    assert!(fo_leg_certified(&FO_CANCEL_OK, "00463", "69044"), "F/O cancel ack (confirmed live)");
    // Leg-specificity: a code valid for ANOTHER leg is NOT certified on this leg — a
    // submit returning a modify/cancel code is a gateway anomaly, never a clean submit.
    assert!(!fo_leg_certified(&FO_SUBMIT_OK, "00462", "69007"), "modify code on submit leg");
    assert!(!fo_leg_certified(&FO_SUBMIT_OK, "00463", "69007"), "cancel code on submit leg");
    assert!(!fo_leg_certified(&FO_MODIFY_OK, "00040", "69041"), "submit code on modify leg");
    assert!(!fo_leg_certified(&FO_CANCEL_OK, "00462", "69044"), "modify code on cancel leg");
    // Soft-reject in a success-shaped envelope (generic 00000 / empty) → NOT certified.
    assert!(!fo_leg_certified(&FO_SUBMIT_OK, "00000", "69007"), "generic success is not an order ack");
    assert!(!fo_leg_certified(&FO_SUBMIT_OK, "", "69007"), "empty rsp_cd is ambiguous");
    // Missing order-number block → NOT certified even with a good code.
    assert!(!fo_leg_certified(&FO_SUBMIT_OK, "00040", "0"), "no order number");
    assert!(!fo_leg_certified(&FO_SUBMIT_OK, "00040", ""), "empty order number");
    // An unrecognized broker code → NOT certified.
    assert!(!fo_leg_certified(&FO_SUBMIT_OK, "03181", "69007"), "a reject code is not certified");
}

// ---- R6/KTD4: credential-safe t0441 ROW diagnostic ------------------------

#[test]
fn fo_t0441_row_line_scrubs_account_shaped_row_fields() {
    // Synthetic account-number-shaped values embedded in t0441 ROW fields (here BOTH
    // `appamt` and `expcode`, NOT just rsp_msg) must be masked; short quantities survive
    // so the line stays useful. Two distinct fields prove the scrub is per-field, not
    // limited to one column.
    let mut r = fo_bal_row("9876543210", "1"); // account-shaped expcode (>=6 digit run)
    r.appamt = "1234567890".into(); // account-number-shaped
    let line = fo_t0441_row_line(&r);
    assert!(!line.contains("1234567890"), "account-shaped appamt leaked: {line}");
    assert!(!line.contains("9876543210"), "account-shaped expcode leaked: {line}");
    assert!(line.contains("jqty=1"), "short quantity must survive: {line}");
}

// ---- R8: 01491 vs an unclassified venue rejection -------------------------

#[test]
fn fo_rejection_classifies_only_01491_not_other_codes() {
    // Covers R8. A simulated 01491 is classified paper-order-incapable.
    assert!(ls_core::is_paper_order_incapable("01491"));
    // A simulated NON-01491 rejection (an unknown venue-not-provisioned code) is NOT
    // classified by is_paper_order_incapable — it is recorded verbatim instead.
    assert!(!ls_core::is_paper_order_incapable("09999"), "unknown code must not be classified");
    assert!(!ls_core::is_paper_incompatible("09999"), "unknown code is not 01900 either");
}

// ---- KTD3: a non-clean cancel raises operator-action-required (no auto-flat) ----

#[test]
fn fo_non_clean_cancel_signal_is_loud_and_account_free() {
    // A non-clean cancel (rejected/ambiguous rsp_cd, so fo_leg_certified is false) drives
    // the operator-action-required path. The loud message names the order yet leaks no
    // account, even when the broker detail embeds an account-shaped value.
    assert!(!fo_leg_certified(&FO_CANCEL_OK, "03181", "69044"), "a reject cancel is not clean");
    let msg = loud_failure(
        "fo-cancel-not-clean",
        &["69044".into()],
        "broker said 계좌 00000000-01 거부 for order 69044",
    );
    assert!(msg.contains("69044"), "the order must be named: {msg}");
    assert!(!msg.contains("00000000"), "account leaked: {msg}");
    assert!(!msg.contains("-01"), "account suffix leaked: {msg}");
}
