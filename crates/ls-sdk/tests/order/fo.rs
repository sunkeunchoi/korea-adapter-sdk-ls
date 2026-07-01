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
    /// Marketable BUY price — the daily ceiling (`uplmtprice`): a limit priced at the
    /// top of the band crosses the book upward, so it fills against resting asks
    /// (KTD4). The manufacture smoke uses this to *fill* rather than rest — the exact
    /// opposite of [`resting_buy_price`]. Returned verbatim (a guaranteed valid tick).
    fn marketable_buy_price(&self) -> &str {
        &self.uplmt_str
    }
    /// Marketable SELL price — the daily floor (`dnlmtprice`): a limit priced at the
    /// bottom of the band crosses the book downward, so it fills against resting bids
    /// (KTD4). Used by the opposite-side close leg to flatten a manufactured long.
    fn marketable_sell_price(&self) -> &str {
        &self.dnlmt_str
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

// ---- F/O manufactured-fill witness over t0441 (Track A, KTD3) ----------------
//
// The manufacture smoke (`fo_position_manufacture_smoke`) INVERTS `fo_flat_verdict`'s
// meaning: where the flat chain treats a non-zero `jqty` as a fail-closed alarm, the
// manufacture smoke needs a non-zero `jqty` on the MANUFACTURED symbol equal to the
// submitted qty as its certification witness (R1, the non-default-position witness).
// `jqty` is the substantive balance-qty field (KTD3), corroborated at the call site by
// the row `appamt` / summary `tappamt` valuations.

/// SIGNED net position for `symbol` across every `t0441` row (F/O 잔고 can be SHORT).
/// The sign is load-bearing: it decides both the certification side (only a LONG of the
/// submitted qty certifies our manufactured buy) and the close side (sell a long, buy a
/// short). `Some(0)` = flat (or no row); `Some(n)` = net `n` lots (`n<0` short); `None` =
/// a row carries an UNPARSEABLE `jqty` — the position cannot be safely sized, so the
/// caller must fail closed rather than guess a close quantity (never silently flat).
fn fo_symbol_position_qty(rows: &[T0441OutBlock1], symbol: &str) -> Option<i64> {
    let sym = symbol.trim();
    let mut net = 0i64;
    let mut saw_row = false;
    for r in rows.iter().filter(|r| r.expcode.trim() == sym) {
        saw_row = true;
        let v: f64 = r.jqty.trim().parse().ok()?; // unparseable → cannot size a close
        net += v.round() as i64;
    }
    if !saw_row {
        return Some(0);
    }
    Some(net)
}

/// The manufactured-fill verdict for the buy leg's `t0441` read (R1/R3).
#[derive(Debug, Clone, PartialEq, Eq)]
enum FoFillWitness {
    /// No position on the manufactured symbol yet — the marketable buy has not filled
    /// (keep polling within the bound, then treat as no-fill / clean-cancel).
    NoFill,
    /// A position exists but it is NOT a LONG of exactly the submitted qty (a partial, an
    /// over-fill, a WRONG-SIDE short, or an unreadable `jqty`). Does NOT certify — flatten
    /// fail-closed (see U2 edges). A short here is an anomaly: the manufacture buy should
    /// produce a long, and a pre-existing short would be caught by the preflight flat gate.
    NotFull,
    /// A LONG of exactly the submitted qty on the manufactured symbol — the full-fill
    /// certification witness (R1). Carries the observed lot count.
    Full(u64),
}

/// Classify a `t0441` read for the MANUFACTURED symbol against the submitted qty
/// (R1/R3). Only the manufactured symbol's row is the certification witness; a LONG of
/// exactly the submitted qty is the sole certifying outcome — a partial/over-fill/
/// wrong-side-short/unreadable is `NotFull` (never certified). The always-full-long
/// certification bar is why U2 submits qty 1 (a 1-lot cannot partially fill) after
/// preflight-asserting the account is flat.
fn fo_fill_witness(rows: &[T0441OutBlock1], symbol: &str, submitted: u64) -> FoFillWitness {
    match fo_symbol_position_qty(rows, symbol) {
        Some(0) => FoFillWitness::NoFill,
        // A LONG (positive) of exactly the submitted qty is the only certifying witness.
        Some(n) if n == submitted as i64 => FoFillWitness::Full(submitted),
        // Partial, over-fill, or a WRONG-SIDE short — never certified.
        Some(_) => FoFillWitness::NotFull,
        None => FoFillWitness::NotFull, // unreadable jqty on the symbol row
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

/// Format a credential-free `t0441` row diagnostic line under `prefix` (so the chain and
/// the manufacture smoke each log under their own grep-able tag). Every field — including
/// the row's own `expcode`/`appamt` (NOT just `rsp_msg`) — is routed through
/// [`scrub_secrets`], because a balance row can carry account-shaped values (R6).
fn fo_t0441_row_line(prefix: &str, r: &T0441OutBlock1) -> String {
    format!(
        "{prefix} t0441-row expcode=[{}] jqty={} cqty={} appamt=[{}]",
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
                println!("{}", fo_t0441_row_line("ORDER-CHAIN-FO", r));
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
// Track A — fo_position_manufacture_smoke (plan 2026-07-01-003, U2).
//
// Manufacture a TRANSIENT domestic F/O position to certify a NON-EMPTY t0441 balance-
// valuation read (the t0441 flip witness, R1), then flatten fail-closed via an
// opposite-side marketable close (R2). This INVERTS the flat chain: it prices to FILL
// (marketable buy at the daily ceiling, KTD4) rather than to rest, and its teardown is
// a marketable close, not a clean-cancel. It is therefore a SEPARATE, one-use harness
// (KTD2) that never touches `fo_order_chained_smoke`'s clean-cancel invariant (which
// certifies CFOAT00300). Reuses `fo.rs`'s band sourcing + the autonomy/scrub/leak-
// suppressor guards verbatim. `#[ignore]`; runs only via `make live-smoke-fo-position`.
// ===========================================================================

/// U1-CALIBRATED marketable-fill poll bound. U1 (the in-window feasibility probe)
/// measures the real submit-ack → `jqty>0` latency; until that finding lands these are
/// conservative defaults (~a few seconds of ~600ms polls). The attempt count is
/// operator-overridable via `LS_FO_MANUFACTURE_POLL_ATTEMPTS` so U1's measurement can
/// tune the bound without a recompile.
const FO_MANUFACTURE_POLL_ATTEMPTS: u32 = 6;
const FO_MANUFACTURE_POLL_INTERVAL_MS: u64 = 600;
/// Hard upper bound on the operator override — a fill that has not landed within ~36s
/// (60 × 600ms) is a no-fill, not a slow fill. Caps `LS_FO_MANUFACTURE_POLL_ATTEMPTS` so
/// a fat-fingered value can never turn the poll into an effectively-unbounded live wait
/// holding a real position open.
const FO_MANUFACTURE_POLL_ATTEMPTS_MAX: u32 = 60;
/// Submit exactly one lot — a 1-lot cannot PARTIALLY fill, so the full-fill
/// certification bar (KTD3) is either met (`jqty==1`) or not met (`jqty==0`), and a
/// single-lot close cleanly offsets it (no partial-close hazard).
const FO_MANUFACTURE_QTY: &str = "1";

/// Pure resolver for the fill-poll attempt count (`raw` = the env var value, if set).
/// A valid override in `1..=FO_MANUFACTURE_POLL_ATTEMPTS_MAX` wins; anything else — unset,
/// zero, negative, non-numeric, or above the cap — falls back to the default. Pure so the
/// `n>0` / clamp logic is offline-tested without mutating a process-global env var.
fn fo_poll_attempts_from(raw: Option<&str>) -> u32 {
    match raw {
        Some(v) => v
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|&n| n >= 1 && n <= FO_MANUFACTURE_POLL_ATTEMPTS_MAX)
            .unwrap_or(FO_MANUFACTURE_POLL_ATTEMPTS),
        None => FO_MANUFACTURE_POLL_ATTEMPTS,
    }
}

/// Resolve the bounded fill-poll attempt count (U1-calibrated, env-overridable + clamped).
fn fo_manufacture_poll_attempts() -> u32 {
    fo_poll_attempts_from(std::env::var("LS_FO_MANUFACTURE_POLL_ATTEMPTS").ok().as_deref())
}

/// Read `t0441` once for the manufacture smoke, returning `(rows, tappamt)` or a
/// scrubbed error. `tappamt` (총평가금액, the summary block) corroborates the per-row
/// `jqty`/`appamt` witness (KTD3).
async fn fo_read_balance(sdk: &LsSdk) -> Result<(Vec<T0441OutBlock1>, String), String> {
    match sdk.account().fo_balance_eval(&T0441Request::new()).await {
        Ok(resp) => Ok((resp.outblock1, resp.outblock.tappamt)),
        Err(e) => Err(scrub_secrets(&e.to_string())),
    }
}

/// Flatten a manufactured F/O position fail-closed (R2, AE2). Reads the SIGNED held
/// position from `t0441` and closes it with an opposite-side marketable order — a SELL to
/// flatten a long, a BUY to flatten a short (the side is derived from the sign, so a
/// stray short is never "closed" by another sell). Makes AT MOST TWO close attempts: the
/// initial close, and — if that close RESTS instead of filling — exactly one
/// cancel-then-reflatten (cancel the resting close first so the two sells can never stack
/// into an over-close, then submit one fresh close). Any still-unflat outcome engages the
/// kill switch and `panic!`s. Never loops; never exits with a position open. IMPORTANT:
/// the caller must NOT engage the kill switch before calling this — it needs order
/// dispatch to place the close; it engages the kill switch itself only on its own
/// terminal failure arms.
async fn fo_flatten_fail_closed(sdk: &LsSdk, band: &FoBand, contract: &str, prior_ordnos: &[String]) {
    // At most 2 close attempts: initial + one reflatten after canceling a rested close.
    for attempt in 1..=2u32 {
        // Size the SIGNED position from t0441 — close EXACTLY what is held, deriving the
        // side from the sign (a partial/short/unexpected magnitude is closed correctly).
        let net = match fo_read_balance(sdk).await {
            Ok((rows, _)) => fo_symbol_position_qty(&rows, contract),
            Err(e) => {
                sdk.inner().set_orders_enabled(false);
                panic!("{}", loud_failure("fo-manufacture-flatten-read-failed", prior_ordnos, &e));
            }
        };
        let net = match net {
            Some(0) => {
                println!(
                    "ORDER-MANUFACTURE-FO flatten=confirmed attempt={attempt} \
                     note=[t0441 flat; nothing (else) to close]"
                );
                return;
            }
            Some(n) => n,
            None => {
                // A position row exists but its jqty is UNPARSEABLE — cannot size a safe
                // close. Fail closed loud; the operator flattens manually.
                sdk.inner().set_orders_enabled(false);
                panic!(
                    "{}",
                    loud_failure(
                        "fo-manufacture-unreadable-position",
                        prior_ordnos,
                        "t0441 shows a position on the manufactured symbol with an unparseable \
                         jqty — cannot size a close; MANUAL flatten required",
                    )
                );
            }
        };

        // Opposite-side MARKETABLE close (KTD4): SELL a long at the daily floor / BUY a
        // short at the daily ceiling — both cross the book to fill. Qty = |net|.
        let (side, price) = if net > 0 {
            ("1", band.marketable_sell_price()) // long → sell to close
        } else {
            ("2", band.marketable_buy_price()) // short → buy to close
        };
        let qty = net.unsigned_abs();
        let close_req = CFOAT00100Request::limit(contract, &qty.to_string(), price, side);
        let mut close_ev = leg_evidence("CFOAT00100", "fo_manufacture_close");
        let close_ordno = match sdk.fo_orders().submit(&close_req).await {
            Ok(resp) => {
                // The ack is informational — the AUTHORITATIVE flat check is the t0441
                // re-read below, never the ack alone (R2).
                close_ev.certification =
                    if fo_leg_certified(&FO_SUBMIT_OK, &resp.rsp_cd, resp.order_no()) {
                        Certification::Certified
                    } else {
                        Certification::Pending
                    };
                close_ev.rsp_cd = resp.rsp_cd.clone();
                close_ev.rsp_msg = resp.rsp_msg.clone();
                close_ev.order_no = Some(resp.order_no().to_string());
                close_ev.record();
                resp.order_no().to_string()
            }
            Err(e) => {
                // The close was rejected or errored — the position is still open. Fail closed.
                close_ev.rsp_msg = format!("close submit failed: {}", scrub_secrets(&e.to_string()));
                close_ev.record();
                sdk.inner().set_orders_enabled(false);
                panic!(
                    "{}",
                    loud_failure(
                        "fo-manufacture-close-failed",
                        prior_ordnos,
                        "opposite-side close SUBMIT failed — the manufactured position is still \
                         open; MANUAL flatten required",
                    )
                );
            }
        };

        // Confirm flat. A marketable close should fill; if it did, t0441 is now flat.
        match fo_read_balance(sdk).await {
            Ok((rows, _)) if fo_flat_verdict(&rows) == FoFlatVerdict::Flat => {
                println!(
                    "ORDER-MANUFACTURE-FO flatten=confirmed attempt={attempt} \
                     note=[opposite-side marketable close filled; t0441 flat]"
                );
                return;
            }
            Ok(_) => { /* still holding — the close RESTED or netted partially */ }
            Err(e) => {
                sdk.inner().set_orders_enabled(false);
                panic!("{}", loud_failure("fo-manufacture-postclose-read-failed", prior_ordnos, &e));
            }
        }

        // The position remains — the close likely RESTED (marketable but thin book).
        // Cancel the resting close BEFORE any reflatten so a second close can never stack
        // on the first into an over-close (which would flip us to the opposite side).
        if !close_ordno.trim().is_empty() && close_ordno.trim() != "0" {
            let cancel_req = CFOAT00300Request::new(&close_ordno, contract, &qty.to_string());
            match sdk.fo_orders().cancel(&cancel_req).await {
                Ok(resp) => println!(
                    "ORDER-MANUFACTURE-FO close-cancel attempt={attempt} ordno={} rsp_cd={}",
                    resp.order_no().trim(),
                    resp.rsp_cd.trim()
                ),
                Err(e) => println!(
                    "ORDER-MANUFACTURE-FO close-cancel attempt={attempt} failed: {}",
                    scrub_secrets(&e.to_string())
                ),
            }
        }
        // Loop for the ONE reflatten (attempt 2), which re-sizes from a fresh t0441 read
        // (so if the canceled close had actually netted flat, attempt 2 sees Some(0) and
        // returns). The `1..=2` bound guarantees no infinite loop.
    }

    // Both attempts exhausted with a position still open — the out-of-band-reset scenario
    // U1 exists to rule out. Fail closed loud.
    sdk.inner().set_orders_enabled(false);
    panic!(
        "{}",
        loud_failure(
            "fo-manufacture-not-flat",
            prior_ordnos,
            "the manufactured F/O position could not be flattened by two opposite-side \
             marketable closes (with a cancel between) — an out-of-band paper reset is \
             required; MANUAL flatten required",
        )
    );
}

/// Manufacture-and-flatten smoke (U2, R1/R2/R3): submit a MARKETABLE F/O buy, poll
/// `t0441` for the full-fill witness (the flip certification), then flatten fail-closed
/// via an opposite-side marketable close. Inherits every guard from
/// [`fo_order_chained_smoke`] verbatim (KTD2): autonomy precondition, resolved-paper
/// assertion, fail-closed dispatch-log suppressor, widened [`scrub_secrets`], and the
/// self-sourced (t8467 front-month) contract.
///
/// `#[ignore]` — runs only via `make live-smoke-fo-position`, OPERATOR-RUN in-window
/// (it places REAL marketable paper orders). Gated on U1's "flatten-in-session works"
/// verdict; the harness code below is offline-stageable and lands before U1/U2 run.
#[tokio::test]
#[ignore = "guarded F/O position MANUFACTURE (places real marketable paper orders): needs credentials + LS_ORDER_SMOKE=1 + a fresh LS_ORDER_SMOKE_NONCE (contract self-sourced from t8467; optional LS_FO_ORDER_SMOKE_SHCODE override); run via `make live-smoke-fo-position` in-window ONLY after U1 proves flatten-in-session works"]
async fn fo_position_manufacture_smoke() {
    // Fail-closed dispatch-log suppressor BEFORE any dispatch (KTD2, mirrors the chain).
    if let Err(e) = install_dispatch_log_suppressor() {
        panic!("{}", scrub_secrets(&e.to_string()));
    }
    // Autonomy precondition + paper-resolved SDK (refuses before any network I/O).
    let sdk = match autonomous_order_smoke_sdk() {
        Ok(s) => s,
        Err(e) => panic!("{}", scrub_secrets(&e.to_string())),
    };

    // Self-source the current F/O contract from the t8467 index-futures master
    // (front-month), the SAME source the chain uses; an explicit override still wins. A
    // missing/empty master certifies nothing and places NO order.
    let contract = match std::env::var("LS_FO_ORDER_SMOKE_SHCODE") {
        Ok(v)
            if !v.trim().is_empty() && v.trim().chars().all(|c| c.is_ascii_alphanumeric()) =>
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
                    &format!("t8467 returned no contract (rsp_cd={}); placed nothing", m.rsp_cd),
                )
                .record();
                OrderEvidence::pending("fo-manufacture", "no F/O contract; placed nothing").record();
                return;
            }
            Err(e) => {
                OrderEvidence::not_certified(
                    "preflight",
                    &format!("t8467 contract source failed: {}", scrub_secrets(&e.to_string())),
                )
                .record();
                OrderEvidence::pending("fo-manufacture", "contract source unavailable; placed nothing")
                    .record();
                return;
            }
        },
    };
    if contract.is_empty() || !contract.chars().all(|c| c.is_ascii_alphanumeric()) {
        OrderEvidence::not_certified("preflight", "resolved F/O contract invalid; placed nothing")
            .record();
        OrderEvidence::pending("fo-manufacture", "no valid F/O contract; placed nothing").record();
        return;
    }

    // Daily band anchor (t2111). FAIL-CLOSED — a degenerate/limit-locked band (KTD4)
    // places NO order (an unfillable-vs-fillable price mistake is unacceptable here).
    let band = match sdk.market_session().fo_quote(&T2111Request::new(&contract)).await {
        Ok(resp) => match validate_fo_band(&resp.outblock.uplmtprice, &resp.outblock.dnlmtprice) {
            Ok(b) => b,
            Err(e) => {
                OrderEvidence::not_certified("band", &e).record();
                OrderEvidence::pending("fo-manufacture", "degenerate band; placed nothing").record();
                return;
            }
        },
        Err(e) => {
            OrderEvidence::pending(
                "band",
                &format!("t2111 anchor fetch failed: {}", scrub_secrets(&e.to_string())),
            )
            .record();
            return;
        }
    };

    // ---- PREFLIGHT flat gate: the account MUST be flat before we manufacture. The
    //      witness/close logic assumes any post-buy position is OUR manufactured long; a
    //      pre-existing position (e.g. a short stranded by a prior aborted run) would
    //      corrupt both the fill witness and the close side. Refuse to manufacture on a
    //      non-flat book — place nothing, record Pending (R2 fail-closed). ----
    match fo_read_balance(&sdk).await {
        Ok((rows, _)) => {
            if fo_flat_verdict(&rows) != FoFlatVerdict::Flat {
                for r in rows.iter() {
                    println!("{}", fo_t0441_row_line("ORDER-MANUFACTURE-FO", r));
                }
                OrderEvidence::pending(
                    "fo-manufacture",
                    "account is NOT flat before manufacture (pre-existing F/O position); refusing \
                     to manufacture on an existing position; placed nothing",
                )
                .record();
                return;
            }
        }
        Err(e) => {
            OrderEvidence::pending(
                "fo-manufacture",
                &format!("preflight t0441 read failed: {}; placed nothing", scrub_secrets(&e)),
            )
            .record();
            return;
        }
    }

    // ---- MANUFACTURE leg: a MARKETABLE buy at the daily CEILING (KTD4) so it fills. ----
    let submit_qty: u64 = FO_MANUFACTURE_QTY.parse().expect("FO_MANUFACTURE_QTY is a literal 1");
    let buy_req =
        CFOAT00100Request::limit(&contract, FO_MANUFACTURE_QTY, band.marketable_buy_price(), "2");
    let mut buy_ev = leg_evidence("CFOAT00100", "fo_manufacture_marketable_buy");
    let buy_ordno = match sdk.fo_orders().submit(&buy_req).await {
        Ok(resp) => {
            let clean = fo_leg_certified(&FO_SUBMIT_OK, &resp.rsp_cd, resp.order_no());
            buy_ev.certification = if clean {
                Certification::Certified
            } else {
                Certification::Pending
            };
            buy_ev.rsp_cd = resp.rsp_cd.clone();
            buy_ev.rsp_msg = resp.rsp_msg.clone();
            buy_ev.order_no = Some(resp.order_no().to_string());
            buy_ev.record();
            if !clean {
                // A non-clean marketable buy ack is ambiguous — it MAY have filled. Flatten
                // with orders STILL ENABLED (the close needs dispatch; engaging the kill
                // switch first would block it), then panic loud. `fo_flatten_fail_closed`
                // engages the kill switch itself on its own terminal failure.
                fo_flatten_fail_closed(&sdk, &band, &contract, &[resp.order_no().to_string()]).await;
                panic!(
                    "{}",
                    loud_failure(
                        "fo-manufacture-buy-not-clean",
                        &[resp.order_no().to_string()],
                        "marketable buy ack was not clean — a position MAY exist; checked + \
                         flattened if present; MANUAL verify required",
                    )
                );
            }
            resp.order_no().to_string()
        }
        Err(LsError::ApiError { code, message }) if ls_core::is_paper_order_incapable(&code) => {
            buy_ev.rsp_cd = code.clone();
            buy_ev.rsp_msg = message;
            buy_ev.reconciliation = Some("classified:paper-order-incapable".into());
            buy_ev.record();
            OrderEvidence::pending(
                "fo-manufacture",
                &format!("paper account not F/O-order-capable ({code}); nothing placed"),
            )
            .record();
            return;
        }
        Err(LsError::ApiError { code, message }) if ls_core::is_paper_incompatible(&code) => {
            buy_ev.rsp_cd = code.clone();
            buy_ev.rsp_msg = message;
            buy_ev.record();
            OrderEvidence::pending(
                "fo-manufacture",
                &format!("F/O order service not in Paper ({code}); nothing placed"),
            )
            .record();
            return;
        }
        Err(LsError::ApiError { code, message }) => {
            buy_ev.rsp_cd = code.clone();
            buy_ev.rsp_msg = message;
            buy_ev.reconciliation = Some("unclassified-rejection".into());
            buy_ev.record();
            OrderEvidence::pending(
                "fo-manufacture",
                &format!("F/O buy rejected ({code}); recorded verbatim; nothing placed"),
            )
            .record();
            return;
        }
        Err(e) => {
            // Ambiguous/transport: a MARKETABLE order may have reached the gateway and
            // FILLED with no order number to reference. Flatten with orders STILL ENABLED
            // (so the close can dispatch), then panic loud.
            buy_ev.rsp_msg = format!("buy failed: {}", scrub_secrets(&e.to_string()));
            buy_ev.record();
            fo_flatten_fail_closed(&sdk, &band, &contract, &[]).await;
            panic!(
                "{}",
                loud_failure(
                    "fo-manufacture-buy-ambiguous",
                    &[],
                    "marketable buy failed AMBIGUOUSLY — an order may have filled uncancelable; \
                     checked + flattened if present; MANUAL verify required",
                )
            );
        }
    };

    // ---- FILL poll (KTD3): bounded wait for the full-fill witness on t0441. ----
    let attempts = fo_manufacture_poll_attempts();
    let mut witness: Option<(u64, String)> = None; // (jqty lots, tappamt) captured on Full
    for attempt in 1..=attempts {
        tokio::time::sleep(std::time::Duration::from_millis(FO_MANUFACTURE_POLL_INTERVAL_MS)).await;
        let (rows, tappamt) = match fo_read_balance(&sdk).await {
            Ok(v) => v,
            Err(e) => {
                // A t0441 read failure mid-poll with a possibly-open position is fail-closed.
                // Flatten with orders STILL ENABLED so the close can dispatch; the helper
                // engages the kill switch itself if it cannot flatten.
                fo_flatten_fail_closed(&sdk, &band, &contract, &[buy_ordno.clone()]).await;
                panic!(
                    "{}",
                    loud_failure("fo-manufacture-fill-poll-read-failed", &[buy_ordno.clone()], &e)
                );
            }
        };
        for r in rows.iter().filter(|r| r.expcode.trim() == contract.trim()) {
            println!("{}", fo_t0441_row_line("ORDER-MANUFACTURE-FO", r));
        }
        match fo_fill_witness(&rows, &contract, submit_qty) {
            FoFillWitness::Full(q) => {
                println!(
                    "ORDER-MANUFACTURE-FO fill=full attempt={attempt} jqty={q} \
                     note=[non-default t0441 position — CERTIFICATION WITNESS captured; \
                     tappamt corroborates]"
                );
                witness = Some((q, tappamt));
                break;
            }
            FoFillWitness::NotFull => {
                // Partial/over-fill (a 1-lot should not partial; U1 confirmed full fills,
                // so this is an anomaly). Do NOT certify — flatten fail-closed and exit.
                println!("ORDER-MANUFACTURE-FO fill=not-full attempt={attempt} action=flatten-fail-closed");
                fo_flatten_fail_closed(&sdk, &band, &contract, &[buy_ordno.clone()]).await;
                OrderEvidence::pending(
                    "fo-manufacture",
                    "non-full fill (partial/anomaly); flattened; t0441 NOT certified",
                )
                .record();
                return;
            }
            FoFillWitness::NoFill => {
                println!("ORDER-MANUFACTURE-FO fill=none attempt={attempt}/{attempts} note=[polling]");
            }
        }
    }

    let (jqty, tappamt) = match witness {
        Some(w) => w,
        None => {
            // Never filled within the bound: the marketable buy is RESTING (thin book).
            // Clean-cancel it (no fill to flatten), confirm no fill appeared, exit
            // uncertified — no stranded order.
            println!("ORDER-MANUFACTURE-FO fill=timeout note=[buy did not fill in bound; clean-cancel]");
            let cancel_req = CFOAT00300Request::new(&buy_ordno, &contract, FO_MANUFACTURE_QTY);
            let cancel_clean = match sdk.fo_orders().cancel(&cancel_req).await {
                Ok(resp) => fo_leg_certified(&FO_CANCEL_OK, &resp.rsp_cd, resp.order_no()),
                Err(e) => {
                    println!("ORDER-MANUFACTURE-FO cancel failed: {}", scrub_secrets(&e.to_string()));
                    false
                }
            };
            // Flatten any race-fill FIRST, orders still enabled (a marketable buy could have
            // filled just before the cancel); the helper closes it or fails loud.
            fo_flatten_fail_closed(&sdk, &band, &contract, &[buy_ordno.clone()]).await;
            // t0441 sees FILLS only, never a RESTING order — so a non-clean cancel leaves the
            // resting buy's removal UNCONFIRMED. Fail loud (mirrors the chain's
            // teardown-uncertain) rather than record a misleading "confirmed flat".
            if !cancel_clean {
                sdk.inner().set_orders_enabled(false);
                panic!(
                    "{}",
                    loud_failure(
                        "fo-manufacture-cancel-uncertain",
                        &[buy_ordno.clone()],
                        "the resting marketable buy's cancel was not clean — removal is \
                         unconfirmed (t0441 sees fills only, no F/O 미체결 read); MANUAL board \
                         check + flatten required",
                    )
                );
            }
            OrderEvidence::pending(
                "fo-manufacture",
                "marketable buy did not fill within the poll bound; cleanly canceled + confirmed \
                 flat; t0441 NOT certified",
            )
            .record();
            return;
        }
    };

    // ---- CERTIFIED: a non-empty t0441 read on a filled position (R1/R3). Record the
    //      certification evidence from t0441's OWN response, then flatten fail-closed. ----
    let mut cert = leg_evidence("t0441", "fo_manufacture_balance_witness");
    cert.certification = Certification::Certified;
    cert.rsp_cd = "00000".into();
    cert.rsp_msg = format!("t0441 non-default position witnessed: jqty={jqty} lots");
    cert.reconciliation = Some(format!("tappamt={}", scrub_secrets(&tappamt)));
    cert.record();

    // ---- FLATTEN fail-closed (R2, AE2): opposite-side marketable close + confirm flat. ----
    fo_flatten_fail_closed(&sdk, &band, &contract, &[buy_ordno.clone()]).await;

    println!(
        "ORDER-MANUFACTURE-FO result=certified jqty={jqty} \
         note=[non-empty t0441 read certified from its own response; position flattened + \
         confirmed flat; t0441 ready to flip Implemented (plan 2026-07-01-003 U3)]"
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

// ---- Track A (plan 2026-07-01-003): marketable pricing + fill witness --------

#[test]
fn fo_band_marketable_prices_cross_the_book() {
    // KTD4: the manufacture buy prices at the CEILING (crosses up → fills), the
    // opposite-side close prices at the FLOOR (crosses down → fills). These are the
    // exact inverse of the resting anchors, and both are returned verbatim (valid ticks).
    let band = validate_fo_band("1214.75", "1034.85").expect("a healthy F/O band");
    assert_eq!(band.marketable_buy_price(), "1214.75", "marketable buy = ceiling");
    assert_eq!(band.marketable_sell_price(), "1034.85", "marketable sell = floor");
    // Inverse of the resting anchors: a resting buy rests at the floor, a marketable buy
    // crosses at the ceiling.
    assert_eq!(band.resting_buy_price(), band.marketable_sell_price());
    assert_eq!(band.resting_sell_price(), band.marketable_buy_price());
}

#[test]
fn fo_symbol_position_qty_signs_sizes_and_fails_safe() {
    // No row for the symbol → Some(0) (nothing held).
    assert_eq!(fo_symbol_position_qty(&[], "101T9000"), Some(0));
    assert_eq!(
        fo_symbol_position_qty(&[fo_bal_row("105V3000", "3")], "101T9000"),
        Some(0),
        "a different symbol's position is not this symbol's"
    );
    // SIGN is preserved: a long is positive, a short is negative (the close side depends
    // on it — sell a long, buy a short).
    assert_eq!(fo_symbol_position_qty(&[fo_bal_row("101T9000", "2")], "101T9000"), Some(2));
    assert_eq!(fo_symbol_position_qty(&[fo_bal_row("101T9000", "-2")], "101T9000"), Some(-2));
    // Multiple rows for the SAME symbol sum SIGNED (a long + a short net): 2 + (-1) = 1.
    assert_eq!(
        fo_symbol_position_qty(
            &[fo_bal_row("101T9000", "2"), fo_bal_row("101T9000", "-1")],
            "101T9000"
        ),
        Some(1),
    );
    // An UNPARSEABLE jqty on the symbol row → None (cannot size a safe close).
    assert_eq!(fo_symbol_position_qty(&[fo_bal_row("101T9000", "??")], "101T9000"), None);
}

#[test]
fn fo_fill_witness_certifies_only_a_full_long() {
    // R1/R3: only a LONG of exactly the submitted qty on the manufactured symbol certifies.
    let sym = "101T9000";
    // Empty / zero / other-symbol → NoFill (keep polling).
    assert_eq!(fo_fill_witness(&[], sym, 1), FoFillWitness::NoFill);
    assert_eq!(fo_fill_witness(&[fo_bal_row(sym, "0")], sym, 1), FoFillWitness::NoFill);
    assert_eq!(
        fo_fill_witness(&[fo_bal_row("105V3000", "1")], sym, 1),
        FoFillWitness::NoFill,
        "a fill on a DIFFERENT symbol does not certify this one"
    );
    // Full LONG of the submitted qty → Full — the certification witness.
    assert_eq!(fo_fill_witness(&[fo_bal_row(sym, "1")], sym, 1), FoFillWitness::Full(1));
    // A SHORT is NOT our manufactured buy — never certifies (it would be "closed" by
    // another sell, deepening the short). This is the wrong-side guard.
    assert_eq!(
        fo_fill_witness(&[fo_bal_row(sym, "-1")], sym, 1),
        FoFillWitness::NotFull,
        "a short of matching magnitude is the WRONG side — must NOT certify"
    );
    // Partial / over-fill / unparseable → NotFull (never certified).
    assert_eq!(fo_fill_witness(&[fo_bal_row(sym, "1")], sym, 2), FoFillWitness::NotFull);
    assert_eq!(fo_fill_witness(&[fo_bal_row(sym, "3")], sym, 2), FoFillWitness::NotFull);
    assert_eq!(fo_fill_witness(&[fo_bal_row(sym, "??")], sym, 1), FoFillWitness::NotFull);
}

#[test]
fn fo_poll_attempts_overrides_clamps_and_fails_safe() {
    // Pure resolver (no env mutation): a valid override in 1..=MAX wins; everything else
    // (unset, zero, negative, non-numeric, above the cap) falls back to the default.
    assert_eq!(fo_poll_attempts_from(None), FO_MANUFACTURE_POLL_ATTEMPTS, "unset → default");
    assert_eq!(fo_poll_attempts_from(Some("3")), 3, "valid override wins");
    assert_eq!(fo_poll_attempts_from(Some(" 12 ")), 12, "trimmed override wins");
    assert_eq!(
        fo_poll_attempts_from(Some("0")),
        FO_MANUFACTURE_POLL_ATTEMPTS,
        "0 rejected (would be a zero-iteration poll)"
    );
    assert_eq!(fo_poll_attempts_from(Some("-4")), FO_MANUFACTURE_POLL_ATTEMPTS, "negative → default");
    assert_eq!(fo_poll_attempts_from(Some("nope")), FO_MANUFACTURE_POLL_ATTEMPTS, "non-numeric → default");
    assert_eq!(
        fo_poll_attempts_from(Some(&(FO_MANUFACTURE_POLL_ATTEMPTS_MAX + 1).to_string())),
        FO_MANUFACTURE_POLL_ATTEMPTS,
        "above the cap → default (never an unbounded live wait)"
    );
    assert_eq!(
        fo_poll_attempts_from(Some(&FO_MANUFACTURE_POLL_ATTEMPTS_MAX.to_string())),
        FO_MANUFACTURE_POLL_ATTEMPTS_MAX,
        "exactly the cap is allowed"
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
    let line = fo_t0441_row_line("ORDER-CHAIN-FO", &r);
    assert!(!line.contains("1234567890"), "account-shaped appamt leaked: {line}");
    assert!(!line.contains("9876543210"), "account-shaped expcode leaked: {line}");
    assert!(line.contains("jqty=1"), "short quantity must survive: {line}");
    assert!(line.starts_with("ORDER-CHAIN-FO"), "prefix is applied: {line}");
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
