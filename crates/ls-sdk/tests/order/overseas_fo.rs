use super::*;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use ls_core::Inner;
use ls_sdk::market_session::{O3101OutBlock, O3101Request, O3105Request};
use ls_sdk::orders::{CIDBT00100Request, CIDBT00900Request, CIDBT01000Request};
use ls_sdk_test_support::mock_http::{mock_config, mount_token};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

// ===========================================================================
// Overseas F/O order chain (CIDBT00100 submit → CIDBT00900 modify → CIDBT01000
// cancel). The overseas-futures sibling of the domestic F/O chain (`fo.rs`).
// Reuses the autonomy / paper-resolved / leak-suppressor / scrub guards verbatim
// (KTD6). Two overseas-specific design points are resolved here (both were left to
// the implementer by the plan, U3):
//
//   1. CONTRACT SOURCING (front-month rule). o3101 (해외선물마스터조회, Implemented)
//      carries NO reliable expiry-ordering key (its rows are `symbol`/`exch_cd`/
//      `crncy_cd`/`bsc_gds_cd`, no maturity date), so "front-month = row[0]" would be
//      a guess. The rule here is therefore: the OPERATOR pins the exact front-month
//      `symbol` via `LS_OVERSEAS_FO_ORDER_SMOKE_SYMBOL` (they know the current
//      contract in-window), and the harness VALIDATES it against the o3101 master
//      universe and extracts `ExchCode`/`CrcyCode`/`PrdtCode` from that row — using
//      o3101 as the plan intends (descriptor source + existence check) while
//      honoring "do not assume row[0]". No pin / not-in-universe ⇒ fail-closed
//      (place nothing).
//
//   2. FILL DETECTION. The domestic template's no-fill assertion rests on `t0441`
//      (선물옵션잔고 fill-detect). The overseas account reads CIDBQ01400/03000/05300
//      are per-currency capacity/deposit summaries — NONE surfaces a per-contract
//      transient 잔고 row (confirmed U3). So the CIDBT teardown rests on
//      CLEAN-CANCEL-CONFIRMATION ALONE: a clean CIDBT01000 response is the sole
//      proof the resting order was removed (a filled order cannot be canceled).
//      This is the plan's documented fallback, made explicit here rather than
//      silently dropping a fill-detect leg.
//
// A third point falls out of (1): overseas-futures exposes NO daily price-band read
// (o3105/o3106 carry only session high/low + last trade — NOT 상한가/하한가, unlike
// domestic t2111), so a provably-valid-tick far-from-market price cannot be derived.
// The resting price is operator-pinned via `LS_OVERSEAS_FO_ORDER_SMOKE_PRICE` (a
// known valid far-from-market tick), with an o3105 last-trade deep-discount as a
// best-effort fallback. A tick/band rejection of the fallback surfaces as
// `OtherRejection` (recorded verbatim), NEVER a capability PENDING (see the KTD5
// classifier below) — so a mispriced fallback can never poison the capability ledger.
//
// `#[ignore]`; the live legs run only via `make live-smoke-overseas-fo-order`.
// ===========================================================================

// ---- Provisional venue codes (operator-tunable; unconfirmed until the live run) --
//
// The overseas-futures order-type / pattern / product-type codes are NOT yet
// confirmed against the live paper gateway (no CIDBT live run has cleared). They are
// operator-overridable via env so the certification run can amend them without a
// recompile; the defaults are the plausible seeds. A wrong code surfaces as an
// `OtherRejection` (recorded verbatim), never a capability PENDING.
const OVERSEAS_FO_BUY: &str = "2"; // 매매구분코드 buy (mirrors domestic F/O)

fn env_or(key: &str, default: &str) -> String {
    match std::env::var(key) {
        Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => default.to_string(),
    }
}

// ---- Contract descriptor sourced from the o3101 master (front-month rule) --------

/// The order descriptor for one overseas-futures contract, assembled from the o3101
/// master row for an operator-pinned `symbol` plus an operator-supplied maturity.
/// `isucodeval` is the wire symbol; `exchcode`/`crcycode`/`prdtcode` come from the
/// master row; `dueyymm` is operator-supplied (`YYYYMM`) — it is NOT in the o3101 row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OverseasContract {
    isucodeval: String,
    exchcode: String,
    crcycode: String,
    prdtcode: String,
    dueyymm: String,
}

/// Resolve the contract descriptor for the operator-pinned `symbol` against the o3101
/// master universe (the front-month rule, U3). FAIL-CLOSED: an empty pin, a symbol not
/// present in the master, a master row missing its exch/currency/product descriptor, or
/// a malformed `dueyymm` all return `Err` so the harness places NO order. Pure, so the
/// rule is offline-tested without a live master read.
fn resolve_contract(
    rows: &[O3101OutBlock],
    symbol_pin: &str,
    dueyymm: &str,
) -> Result<OverseasContract, String> {
    let sym = symbol_pin.trim();
    if sym.is_empty() {
        return Err("no overseas-futures symbol pinned \
                    (set LS_OVERSEAS_FO_ORDER_SMOKE_SYMBOL); placed nothing"
            .into());
    }
    // Front-month rule: the operator-pinned symbol must EXIST in the master universe —
    // never row[0]. This validates the pin and sources the descriptor in one step.
    let row = rows
        .iter()
        .find(|r| r.symbol.trim() == sym)
        .ok_or_else(|| {
            format!(
                "pinned symbol not found among the {} o3101 master rows; placed nothing",
                rows.len()
            )
        })?;
    let exch = row.exch_cd.trim();
    let crcy = row.crncy_cd.trim();
    let prdt = row.bsc_gds_cd.trim();
    if exch.is_empty() || crcy.is_empty() || prdt.is_empty() {
        return Err("o3101 master row is missing an exch/currency/product descriptor; \
                    placed nothing"
            .into());
    }
    let due = dueyymm.trim();
    if due.len() != 6 || !due.chars().all(|c| c.is_ascii_digit()) {
        return Err("no valid DueYymm (YYYYMM) supplied \
                    (set LS_OVERSEAS_FO_ORDER_SMOKE_DUEYYMM); placed nothing"
            .into());
    }
    Ok(OverseasContract {
        isucodeval: sym.into(),
        exchcode: exch.into(),
        crcycode: crcy.into(),
        prdtcode: prdt.into(),
        dueyymm: due.into(),
    })
}

// ---- Provably-unfillable resting price (no daily band exists) --------------------

/// Derive a far-below-market resting BUY price from a reference (last-trade) price by a
/// deep discount `factor` (in `0<f<1`). A limit BUY priced this far below market rests
/// unfilled. Returns `None` (fail-closed) for a non-finite / non-positive reference or an
/// out-of-range factor — a missing price must NEVER fall back to a near-market fillable
/// price. Trailing zeros are trimmed so an integer-tick contract emits a bare int (no
/// gratuitous `.0`). This is a BEST-EFFORT fallback: overseas-futures has no daily
/// price-band read (o3105/o3106 carry only session high/low), so a valid tick is not
/// guaranteed — the operator pins `LS_OVERSEAS_FO_ORDER_SMOKE_PRICE` for the clean-
/// certification path, and a tick rejection of this fallback is `OtherRejection`, never a
/// capability PENDING.
fn unfillable_buy_price(reference: f64, factor: f64) -> Option<String> {
    if !reference.is_finite() || reference <= 0.0 || !(factor > 0.0 && factor < 1.0) {
        return None;
    }
    let p = reference * factor;
    if !(p.is_finite() && p > 0.0) {
        return None;
    }
    let s = format!("{p:.4}");
    let trimmed = s.trim_end_matches('0').trim_end_matches('.').to_string();
    // Fail closed if the 4-dp rounding collapsed a sub-tick reference to zero (e.g.
    // reference 0.00009 × 0.5 → "0.0000" → "0"): a "0" price is not a valid resting order.
    match trimmed.parse::<f64>() {
        Ok(v) if v > 0.0 => Some(trimmed),
        _ => None,
    }
}

/// The margin an operator-pinned resting BUY price must clear below the last-trade
/// reference to be accepted as provably unfillable: the pin must be at most 90% of the
/// reference (i.e. ≥10% below market). A buy limit fills when priced at/above the market
/// ask, so a pin near or above the reference could FILL — this is the guard against a
/// fat-fingered near/above-market pin manufacturing a real position.
const UNFILLABLE_PIN_MARGIN: f64 = 0.9;

/// `true` iff an operator-pinned resting BUY price is a valid, positive number that sits
/// safely below market (≤ [`UNFILLABLE_PIN_MARGIN`] × the last-trade `reference`). A
/// non-numeric, non-positive, or near/above-market pin returns `false` so the harness
/// fails closed and places nothing rather than risk a fill. Pure, offline-testable.
fn pinned_price_is_unfillable(pin: &str, reference: f64) -> bool {
    if !(reference.is_finite() && reference > 0.0) {
        return false;
    }
    match pin.trim().parse::<f64>() {
        Ok(p) if p.is_finite() && p > 0.0 => p <= UNFILLABLE_PIN_MARGIN * reference,
        _ => false,
    }
}

// ---- Per-leg certification predicate (mirrors fo_leg_certified) -------------------

/// The overseas-futures order-ack codes, PER LEG. SEED-ONLY / UNCONFIRMED: no CIDBT
/// live run has cleared, so these mirror the domestic F/O family's ack codes (the
/// gateway shares its order-success seed set) and the operator's live run CONFIRMS or
/// AMENDS them. Kept leg-specific so a submit that returns a modify/cancel code (a
/// gateway anomaly) is NOT certified as a clean submit.
const OVERSEAS_FO_SUBMIT_OK: [&str; 2] = ["00040", "00039"]; // buy / sell (seed)
const OVERSEAS_FO_MODIFY_OK: [&str; 2] = ["00462", "00132"]; // seed
const OVERSEAS_FO_CANCEL_OK: [&str; 2] = ["00463", "00156"]; // seed

/// Per-leg certification: a leg certifies ONLY on a genuinely clean row — the `rsp_cd`
/// is in that LEG's accepted set AND a usable order number is present. Keyed on the
/// business code + order number, never status text (mirrors `fo_leg_certified`).
fn overseas_fo_leg_certified(expected: &[&str], rsp_cd: &str, order_no: &str) -> bool {
    let o = order_no.trim();
    expected.contains(&rsp_cd.trim()) && !o.is_empty() && o != "0"
}

// ---- KTD5 capability-verdict classifier (IGW40011 ≠ 01491), U4 -------------------

/// Venue-closed rejection codes (`01458`-class 장종료). Extensible; an UNKNOWN
/// venue-closed code (e.g. a CME maintenance-break / weekend variant) falls through to
/// `OtherRejection`, which is ALSO never a capability PENDING — so the safety property
/// (never falsely `01491`) holds regardless of set completeness.
const VENUE_CLOSED_CODES: &[&str] = &["01458"];

/// The capability verdict for a CIDBT submit outcome (KTD5). Every non-`01491` outcome
/// is a DISTINCT variant so no serde defect, venue closure, or benign band/tick
/// rejection can ever collapse into the `01491` capability record (Req §4).
#[derive(Debug, Clone, PartialEq, Eq)]
enum CapabilityVerdict {
    /// Order cleared (accepted + rests) → certify.
    Clears,
    /// `01491` — account order-incapable → record account PENDING, stop.
    PaperOrderIncapable,
    /// `01900` — paper-incompatible feed → recorded, NOT a capability verdict.
    PaperIncompatible,
    /// `01458`-class venue-closed → "retry in-window", NOT a capability PENDING.
    VenueClosed(String),
    /// `IGW40011` — request-shape / serde defect → fail loudly, NEVER a capability
    /// verdict. The gateway rejects the request shape PRE-execution (placed nothing). On
    /// the ORDER path this surfaces as an HTTP-500 carried by
    /// `LsError::AmbiguousOrder { code: "IGW40011", .. }` (a non-2xx on an order is
    /// ambiguous), NOT `LsError::ApiError` and NOT the client-side `LsError::Invalid` —
    /// so it MUST be caught before the may-rest variant check below.
    RequestShapeDefect,
    /// An AMBIGUOUS or transport-level outcome — a non-2xx HTTP with no `rsp_cd`
    /// (`AmbiguousOrder{code:""}`), a transport error (`Http`), or a 2xx whose response
    /// failed to decode (`Decode`) — where the order MAY have reached the exchange and be
    /// resting NOW with no order number to cancel it. NOT placed-nothing and NOT a
    /// capability verdict: the harness fails CLOSED (kill switch + loud panic), because
    /// overseas-futures has no transient-position read to detect a stray fill.
    AmbiguousPlacement,
    /// Any other code from a CLEAN 2xx business rejection (`ApiError`) that placed nothing —
    /// recorded VERBATIM, explicitly NOT binned as `01491`.
    OtherRejection(String),
}

/// Extract the gateway `rsp_cd` an order error carries, if any. Both `ApiError` (a 2xx
/// business rejection: `01491`/`01900`/`01458`) and `AmbiguousOrder` (a non-2xx, incl.
/// the HTTP-500 that carries `rsp_cd=IGW40011`, or an ambiguous ack) carry a `code`.
/// `Invalid` (client-side preflight) never carries a gateway code, and transport errors
/// carry none — both yield `None`.
fn order_error_code(err: &LsError) -> Option<&str> {
    match err {
        LsError::ApiError { code, .. } | LsError::AmbiguousOrder { code, .. } => Some(code.trim()),
        _ => None,
    }
}

/// Classify a submit-leg ERROR into a capability verdict (KTD5 + fail-closed teardown).
/// The load-bearing distinction is PLACED-NOTHING vs MAY-REST, keyed off the `LsError`
/// variant, not just the `rsp_cd`:
///   - `IGW40011` (any variant) — a request-shape defect rejected PRE-execution (placed
///     nothing); a loud serde-defect verdict, checked FIRST because it arrives as an
///     HTTP-500 `AmbiguousOrder` that the may-rest arm would otherwise swallow.
///   - `ApiError` — a CLEAN 2xx business rejection (the gateway received and rejected the
///     order, placing nothing); capability-classified by `rsp_cd`.
///   - `AmbiguousOrder` (non-IGW40011) / `Http` / `Decode` — non-2xx, transport, or an
///     undecodable 2xx response: the order MAY be resting now → `AmbiguousPlacement`
///     (fail closed). This is the domestic `fo.rs` catch-all made explicit — without it a
///     `503`/dropped connection would fall through to `OtherRejection` and silently
///     record a PENDING while an order rests uncancelable (the fail-OPEN this guards).
///   - anything else (client-side `Invalid`/`Config`) — never reached the wire, placed
///     nothing → `OtherRejection`.
/// The `Ok` (clears) arm is handled by the caller from the response's per-leg certification.
fn classify_submit_error(err: &LsError) -> CapabilityVerdict {
    let code = order_error_code(err).unwrap_or("");
    // PRE-execution serde defect — placed nothing, but a loud verdict, never a capability
    // PENDING. First, because it rides an HTTP-500 AmbiguousOrder the may-rest arm catches.
    if code == "IGW40011" {
        return CapabilityVerdict::RequestShapeDefect;
    }
    match err {
        // A clean 2xx business rejection: the gateway rejected the order, placing nothing.
        LsError::ApiError { .. } => {
            if ls_core::is_paper_order_incapable(code) {
                CapabilityVerdict::PaperOrderIncapable
            } else if ls_core::is_paper_incompatible(code) {
                CapabilityVerdict::PaperIncompatible
            } else if VENUE_CLOSED_CODES.contains(&code) {
                CapabilityVerdict::VenueClosed(code.to_string())
            } else {
                CapabilityVerdict::OtherRejection(code.to_string())
            }
        }
        // Non-2xx / transport / undecodable-2xx: the order MAY be resting → fail closed.
        LsError::AmbiguousOrder { .. } | LsError::Http(_) | LsError::Decode(_) => {
            CapabilityVerdict::AmbiguousPlacement
        }
        // Client-side preflight / config: never reached the wire — placed nothing.
        _ => CapabilityVerdict::OtherRejection(code.to_string()),
    }
}

// ===========================================================================
// overseas_fo_chained_smoke — the live capability gate (#[ignore], operator-run).
// ===========================================================================

/// AUTONOMOUS-guarded overseas-F/O chained paper-order run: submit a resting
/// far-from-market order (gate-1 evidence), modify it (reduce qty 2→1), then cancel it
/// as the PRIMARY (and only) teardown — each leg certified from its OWN `rsp_cd`. Since
/// overseas-futures has no transient-position read (design point 2), flatness rests on
/// the CLEAN CANCEL alone; a non-clean cancel or an ambiguous outcome engages the kill
/// switch (AFTER the cancel attempt, never before) and hard-fails loud for a manual
/// board check.
///
/// Inherits every guard from the domestic chain verbatim (KTD6): the autonomy
/// precondition, the paper-resolved SDK, the fail-closed dispatch-log suppressor, and
/// the widened `scrub_secrets`. Capability outcomes are classified by
/// [`classify_submit_error`] (KTD5): only `01491` records account PENDING; `IGW40011`
/// is a loud serde-defect failure; venue-closed / any-other code is recorded but never a
/// capability PENDING.
///
/// `#[ignore]` — runs only via `make live-smoke-overseas-fo-order`.
#[tokio::test]
#[ignore = "guarded overseas-F/O chained paper order: needs .env.overseas_option credentials + LS_TRADING_ENV=paper + LS_ORDER_SMOKE=1 + a fresh LS_ORDER_SMOKE_NONCE + an operator-pinned LS_OVERSEAS_FO_ORDER_SMOKE_SYMBOL/DUEYYMM (optional LS_OVERSEAS_FO_ORDER_SMOKE_PRICE); run via `make live-smoke-overseas-fo-order`"]
async fn overseas_fo_chained_smoke() {
    // Install the fail-closed dispatch-log suppressor BEFORE any dispatch (the o3101/
    // o3105 reads carry account-shaped data in their raw bodies).
    if let Err(e) = install_dispatch_log_suppressor() {
        panic!("{}", scrub_secrets(&e.to_string()));
    }

    // Autonomy precondition + paper-resolved SDK. A refusal places nothing.
    let sdk = match autonomous_order_smoke_sdk() {
        Ok(s) => s,
        Err(e) => panic!("{}", scrub_secrets(&e.to_string())),
    };

    // ---- Contract descriptor (design point 1): operator-pinned symbol validated
    // against the o3101 master universe; fail-closed on any gap. ----
    let symbol_pin = env_or("LS_OVERSEAS_FO_ORDER_SMOKE_SYMBOL", "");
    let dueyymm = env_or("LS_OVERSEAS_FO_ORDER_SMOKE_DUEYYMM", "");
    let contract = match sdk
        .market_session()
        .overseas_futures_master(&O3101Request::new(""))
        .await
    {
        Ok(m) => match resolve_contract(&m.outblock, &symbol_pin, &dueyymm) {
            Ok(c) => c,
            Err(e) => {
                OrderEvidence::not_certified("preflight", &e).record();
                OrderEvidence::pending("overseas-fo-chain", &e).record();
                return;
            }
        },
        Err(e) => {
            OrderEvidence::pending(
                "preflight",
                &format!(
                    "o3101 master source failed: {}",
                    scrub_secrets(&e.to_string())
                ),
            )
            .record();
            return;
        }
    };

    // ---- Resting price. Overseas-futures exposes no daily price-band read, so the
    // reference is the o3105 last trade. ALWAYS fetch it (even for a pin) so a pinned price
    // can be VALIDATED as provably below market — a fat-fingered near/above-market pin would
    // FILL and manufacture a real position. Fail-closed: no valid reference ⇒ place nothing.
    let reference = match sdk
        .market_session()
        .overseas_futures_quote(&O3105Request::new(&contract.isucodeval))
        .await
    {
        Ok(q) => match q.outblock.trd_p.trim().parse::<f64>() {
            Ok(r) if r.is_finite() && r > 0.0 => r,
            _ => {
                OrderEvidence::not_certified(
                    "band",
                    "o3105 returned no valid last-trade reference; placed nothing",
                )
                .record();
                OrderEvidence::pending("overseas-fo-chain", "no resting-price anchor").record();
                return;
            }
        },
        Err(e) => {
            OrderEvidence::pending(
                "band",
                &format!("o3105 price fetch failed: {}", scrub_secrets(&e.to_string())),
            )
            .record();
            return;
        }
    };
    let price = match env_or("LS_OVERSEAS_FO_ORDER_SMOKE_PRICE", "") {
        // Operator pin (clean-cert path) — accepted ONLY if provably below market.
        pin if !pin.is_empty() => {
            if !pinned_price_is_unfillable(&pin, reference) {
                OrderEvidence::not_certified(
                    "band",
                    "LS_OVERSEAS_FO_ORDER_SMOKE_PRICE pin is not provably below market \
                     (must be ≤ 90% of the o3105 last trade) — refusing to risk a fill; \
                     placed nothing",
                )
                .record();
                OrderEvidence::pending("overseas-fo-chain", "pinned price not unfillable")
                    .record();
                return;
            }
            pin
        }
        // Unpinned: a 0.5× last-trade deep discount — a limit buy this far below market rests
        // unfilled. A tick rejection of it is OtherRejection, never a capability PENDING.
        _ => match unfillable_buy_price(reference, 0.5) {
            Some(p) => p,
            None => {
                OrderEvidence::not_certified(
                    "band",
                    "could not derive an unfillable resting price from the o3105 reference; \
                     placed nothing",
                )
                .record();
                OrderEvidence::pending("overseas-fo-chain", "no resting-price anchor").record();
                return;
            }
        },
    };

    // Operator-tunable venue codes (provisional seeds; the live run confirms).
    let orddt = env_or("LS_OVERSEAS_FO_ORDER_SMOKE_ORDDT", ""); // "" ⇒ gateway defaults today
    let futs_ord_tp = env_or("LS_OVERSEAS_FO_ORDER_TP", "1");
    let ord_ptn = env_or("LS_OVERSEAS_FO_ORD_PTN", "1");
    let prdt_tp = env_or("LS_OVERSEAS_FO_PRDT_TP", "F");

    // ---- SUBMIT leg (gate-1 evidence): a resting buy far below market, qty 2 so the
    // MODIFY can exercise a valid quantity reduction (2 → 1). ----
    let submit_req = CIDBT00100Request::new(
        &orddt,
        &contract.isucodeval,
        &futs_ord_tp,
        OVERSEAS_FO_BUY,
        &ord_ptn,
        &contract.crcycode,
        &price,
        "0", // no conditional price
        "2", // qty
        &contract.prdtcode,
        &contract.dueyymm,
        &contract.exchcode,
    );
    let mut sev = leg_evidence("CIDBT00100", "overseas_fo_submit_resting_buy");
    let submit_ordno = match sdk.overseas_fo_orders().submit(&submit_req).await {
        Ok(resp) => {
            sev.certification =
                if overseas_fo_leg_certified(&OVERSEAS_FO_SUBMIT_OK, &resp.rsp_cd, resp.order_no()) {
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
        Err(e) => {
            // KTD5: classify the capability verdict from the gateway rsp_cd (NOT the
            // transport variant). Only 01491 is a capability PENDING; IGW40011 is a loud
            // serde defect; venue-closed / other codes are recorded, never a PENDING.
            let verdict = classify_submit_error(&e);
            sev.rsp_cd = order_error_code(&e).unwrap_or("").to_string();
            sev.rsp_msg = scrub_secrets(&e.to_string());
            match verdict {
                CapabilityVerdict::PaperOrderIncapable => {
                    sev.reconciliation = Some("classified:paper-order-incapable".into());
                    sev.record();
                    OrderEvidence::pending(
                        "overseas-fo-chain",
                        "paper account not overseas-F/O-order-capable (01491, classified); \
                         chain not run",
                    )
                    .record();
                    return;
                }
                CapabilityVerdict::PaperIncompatible => {
                    sev.record();
                    OrderEvidence::pending(
                        "overseas-fo-chain",
                        "overseas-F/O order service not in Paper (01900); chain not run",
                    )
                    .record();
                    return;
                }
                CapabilityVerdict::VenueClosed(code) => {
                    sev.reconciliation = Some("venue-closed:retry-in-window".into());
                    sev.record();
                    OrderEvidence::pending(
                        "overseas-fo-chain",
                        &format!(
                            "venue closed ({code}, 장종료/maintenance) — retry in an open \
                             window; NOT a capability PENDING; chain not run"
                        ),
                    )
                    .record();
                    return;
                }
                CapabilityVerdict::RequestShapeDefect => {
                    // A serde/wire-shape defect (IGW40011) — NEVER a capability verdict.
                    // Fail loudly so it is fixed, not recorded as a PENDING.
                    sev.reconciliation = Some("request-shape-defect:IGW40011".into());
                    sev.record();
                    panic!(
                        "{}",
                        loud_failure(
                            "overseas-fo-igw40011",
                            &[],
                            "CIDBT submit returned IGW40011 (request-shape/serde defect) — a \
                             numeric request field serialized as a string; FIX the serde and \
                             re-run. This is NOT a capability verdict and was NOT recorded as \
                             a PENDING.",
                        )
                    );
                }
                CapabilityVerdict::AmbiguousPlacement => {
                    // The order MAY have reached the exchange and be RESTING now with no
                    // order number to cancel it, and overseas-futures has no
                    // transient-position read to detect it (design point 2). This is
                    // fail-closed LOUD, never a silent PENDING: engage the kill switch
                    // (nothing to cancel — this only halts further orders) and hard-fail
                    // so the operator does a MANUAL board check + flatten. Mirrors the
                    // domestic `fo.rs` ambiguous-submit catch-all.
                    sev.reconciliation = Some("ambiguous-placement".into());
                    sev.record();
                    sdk.inner().set_orders_enabled(false);
                    panic!(
                        "{}",
                        loud_failure(
                            "overseas-fo-submit-ambiguous",
                            &[],
                            "CIDBT submit failed AMBIGUOUSLY (non-2xx / transport / undecodable \
                             response) — an order MAY rest uncancelable (no order number; \
                             overseas has no transient-position read); MANUAL board check + \
                             flatten required",
                        )
                    );
                }
                CapabilityVerdict::OtherRejection(code) => {
                    sev.reconciliation = Some("unclassified-rejection".into());
                    sev.record();
                    OrderEvidence::pending(
                        "overseas-fo-chain",
                        &format!(
                            "overseas-F/O submit rejected ({code}); recorded verbatim, NOT \
                             classified as paper-order-incapable; chain not run"
                        ),
                    )
                    .record();
                    return;
                }
                CapabilityVerdict::Clears => unreachable!("Clears is the Ok arm, not an error"),
            }
        }
    };
    if submit_ordno.trim().is_empty() || submit_ordno.trim() == "0" {
        // An accepted submit with no usable order number is AMBIGUOUS-PLACED: an order
        // may now rest that the harness cannot reference to cancel. There is no overseas
        // transient-position read to check for a fill, so engage the kill switch and
        // hard-fail loud for a manual board check (fail-closed).
        sdk.inner().set_orders_enabled(false);
        panic!(
            "{}",
            loud_failure(
                "overseas-fo-submit-no-ordno",
                &[],
                "CIDBT submit was ACCEPTED but returned no usable order number — an order may \
                 rest uncancelable; MANUAL board check + flatten required",
            )
        );
    }

    // ---- MODIFY leg: reduce the resting order's quantity 2 → 1 (price stays far below
    // market so it stays unfilled). The parent flows in as the caller-supplied
    // OvrsFutsOrgOrdNo (KTD2). ----
    let modify_req = CIDBT00900Request::new(
        &orddt,
        &submit_ordno,
        &contract.isucodeval,
        &futs_ord_tp,
        OVERSEAS_FO_BUY,
        &ord_ptn,
        &contract.crcycode,
        &price,
        "0",
        "1", // reduced qty
        &contract.prdtcode,
        &contract.dueyymm,
        &contract.exchcode,
    );
    let mut mev = leg_evidence("CIDBT00900", "overseas_fo_modify_reduce_qty");
    // CIDBT01000 cancel keys off the order number only (it carries no quantity field), so
    // the modify leg yields just the live order number + whether the outcome was ambiguous.
    let (live_ordno, modify_uncertain) = match sdk.overseas_fo_orders().modify(&modify_req).await {
        Ok(resp) => {
            let certified =
                overseas_fo_leg_certified(&OVERSEAS_FO_MODIFY_OK, &resp.rsp_cd, resp.order_no());
            mev.certification = if certified {
                Certification::Certified
            } else {
                Certification::Pending
            };
            mev.rsp_cd = resp.rsp_cd.clone();
            mev.rsp_msg = resp.rsp_msg.clone();
            mev.order_no = Some(resp.order_no().to_string());
            mev.reconciliation = Some(format!("ack=[{}]", scrub_secrets(resp.ack())));
            mev.record();
            let n = resp.order_no().to_string();
            if certified {
                (n, false)
            } else {
                // Ok but not cleanly certified: ambiguous which order is now live.
                let live = if n.trim().is_empty() || n.trim() == "0" {
                    submit_ordno.clone()
                } else {
                    n
                };
                (live, true)
            }
        }
        Err(e) => {
            // Gate 1 stands (submit); gate 2 stays Pending. The original submit order is
            // untouched and still rests at qty 2.
            mev.rsp_cd = order_error_code(&e).unwrap_or("").to_string();
            mev.rsp_msg = scrub_secrets(&e.to_string());
            mev.record();
            OrderEvidence::pending(
                "overseas-fo-chain",
                "overseas-F/O modify link failed; gate 2 not flipped (gate 1 stands)",
            )
            .record();
            (submit_ordno.clone(), false)
        }
    };

    // ---- CANCEL leg (PRIMARY + ONLY teardown, gate-2 evidence). A CLEAN cancel is the
    // sole confirmation the resting order was removed — there is no overseas
    // transient-position read (design point 2). ----
    let cancel_req = CIDBT01000Request::new(
        &orddt,
        &contract.isucodeval,
        &live_ordno,
        &futs_ord_tp,
        &prdt_tp,
        &contract.exchcode,
    );
    let mut cev = leg_evidence("CIDBT01000", "overseas_fo_cancel_teardown");
    let cancel_clean = match sdk.overseas_fo_orders().cancel(&cancel_req).await {
        Ok(resp) => {
            let ok =
                overseas_fo_leg_certified(&OVERSEAS_FO_CANCEL_OK, &resp.rsp_cd, resp.order_no());
            cev.certification = if ok {
                Certification::Certified
            } else {
                Certification::Pending
            };
            cev.rsp_cd = resp.rsp_cd.clone();
            cev.rsp_msg = resp.rsp_msg.clone();
            cev.order_no = Some(resp.order_no().to_string());
            cev.reconciliation = Some(format!("ack=[{}]", scrub_secrets(resp.ack())));
            cev.record();
            ok
        }
        Err(e) => {
            cev.rsp_cd = order_error_code(&e).unwrap_or("").to_string();
            cev.rsp_msg = scrub_secrets(&e.to_string());
            cev.record();
            OrderEvidence::pending(
                "overseas-fo-chain",
                "overseas-F/O cancel link failed; gate 2 not flipped",
            )
            .record();
            false
        }
    };

    // Teardown gate (fail-closed): removal is confirmed ONLY by a CLEAN cancel of a
    // cleanly-certified order — there is no overseas transient-position read to fall back
    // on. A non-clean cancel OR an ambiguous modify means removal is UNCONFIRMABLE:
    // engage the kill switch (AFTER the cancel attempt, never before — KTD7) and
    // hard-fail loud for a manual board check.
    let teardown_uncertain = modify_uncertain || !cancel_clean;
    if teardown_uncertain {
        sdk.inner().set_orders_enabled(false);
        panic!(
            "{}",
            loud_failure(
                "overseas-fo-teardown-uncertain",
                &[submit_ordno.clone(), live_ordno.clone()],
                "overseas-F/O teardown could not confirm the resting order was removed \
                 (non-clean cancel or ambiguous modify) — there is no overseas \
                 transient-position read; MANUAL board check + flatten required",
            )
        );
    }

    println!(
        "ORDER-CHAIN-OVERSEAS-FO teardown=clean-cancel note=[clean-cancel confirms resting-order \
         removal; overseas-futures has no transient-position read, so flatness is clean-cancel \
         only and fail-closed]"
    );
}

// ===========================================================================
// Offline tests (run in the normal suite — never place a live order).
// ===========================================================================

// ---- U3: contract sourcing (front-month rule), fail-closed -----------------------

fn o3101_row(symbol: &str, exch: &str, crncy: &str, prdt: &str) -> O3101OutBlock {
    O3101OutBlock {
        symbol: symbol.into(),
        exch_cd: exch.into(),
        crncy_cd: crncy.into(),
        bsc_gds_cd: prdt.into(),
        ..Default::default()
    }
}

#[test]
fn resolve_contract_validates_pin_against_master_and_fails_closed() {
    let rows = vec![
        o3101_row("ESU26", "CME", "USD", "ES"),
        o3101_row("NQU26", "CME", "USD", "NQ"),
    ];
    // Happy path: the pinned symbol is found and its descriptor extracted from the row
    // (NOT row[0] — the pin selects NQU26, the second row).
    let c = resolve_contract(&rows, "NQU26", "202609").expect("pinned symbol resolves");
    assert_eq!(
        c,
        OverseasContract {
            isucodeval: "NQU26".into(),
            exchcode: "CME".into(),
            crcycode: "USD".into(),
            prdtcode: "NQ".into(),
            dueyymm: "202609".into(),
        }
    );
    // Fail-closed: empty pin, symbol not in the universe, malformed dueyymm.
    assert!(resolve_contract(&rows, "", "202609").is_err(), "empty pin ⇒ err");
    assert!(
        resolve_contract(&rows, "CLZ26", "202609").is_err(),
        "symbol not in master universe ⇒ err (never row[0])"
    );
    assert!(
        resolve_contract(&rows, "ESU26", "2609").is_err(),
        "malformed dueyymm ⇒ err"
    );
    assert!(
        resolve_contract(&rows, "ESU26", "").is_err(),
        "missing dueyymm ⇒ err"
    );
    // Fail-closed: a master row missing its descriptor.
    let bad = vec![o3101_row("ESU26", "", "USD", "ES")];
    assert!(
        resolve_contract(&bad, "ESU26", "202609").is_err(),
        "row missing exch ⇒ err"
    );
}

// ---- U3: unfillable resting price, fail-closed (no daily band read exists) --------

#[test]
fn unfillable_buy_price_deep_discounts_and_fails_closed() {
    // 0.5× of a fractional last trade rests far below market; trailing zeros trimmed.
    assert_eq!(unfillable_buy_price(4213.0, 0.5).as_deref(), Some("2106.5"));
    // An integer-tick result emits a bare int (no gratuitous ".0").
    assert_eq!(unfillable_buy_price(4200.0, 0.5).as_deref(), Some("2100"));
    // Fail-closed: a non-positive / non-finite reference or an out-of-range factor must
    // NEVER produce a near-market fillable price.
    assert_eq!(unfillable_buy_price(0.0, 0.5), None);
    assert_eq!(unfillable_buy_price(-1.0, 0.5), None);
    assert_eq!(unfillable_buy_price(f64::NAN, 0.5), None);
    assert_eq!(unfillable_buy_price(f64::INFINITY, 0.5), None);
    assert_eq!(unfillable_buy_price(4200.0, 0.0), None, "factor 0 ⇒ none");
    assert_eq!(unfillable_buy_price(4200.0, 1.0), None, "factor 1 (at market) ⇒ none");
    assert_eq!(unfillable_buy_price(4200.0, 1.5), None, "factor >1 ⇒ none");
    // Fail-closed: a sub-tick reference whose discounted price rounds to zero at 4dp must
    // NOT emit "0" as a price (adversarial edge).
    assert_eq!(unfillable_buy_price(0.00009, 0.5), None, "sub-tick ⇒ none, never \"0\"");
}

#[test]
fn pinned_price_is_unfillable_rejects_near_or_above_market_pins() {
    // A pin well below market (≤90% of the reference) is accepted.
    assert!(pinned_price_is_unfillable("2000", 4200.0), "deep pin is unfillable");
    assert!(pinned_price_is_unfillable("3780", 4200.0), "exactly 90% (boundary) is ok");
    // A near-market or above-market pin is REJECTED (would risk a fill) — the codex P1 guard.
    assert!(!pinned_price_is_unfillable("4100", 4200.0), "near-market pin rejected");
    assert!(!pinned_price_is_unfillable("4200", 4200.0), "at-market pin rejected");
    assert!(!pinned_price_is_unfillable("5000", 4200.0), "above-market pin rejected");
    // Malformed / non-positive pins and a degenerate reference all fail closed.
    assert!(!pinned_price_is_unfillable("", 4200.0));
    assert!(!pinned_price_is_unfillable("abc", 4200.0));
    assert!(!pinned_price_is_unfillable("0", 4200.0));
    assert!(!pinned_price_is_unfillable("-100", 4200.0));
    assert!(!pinned_price_is_unfillable("2000", 0.0), "no reference ⇒ reject");
    assert!(!pinned_price_is_unfillable("2000", f64::NAN), "nan reference ⇒ reject");
}

#[test]
fn overseas_fo_leg_certified_needs_code_and_order_number() {
    assert!(overseas_fo_leg_certified(&OVERSEAS_FO_SUBMIT_OK, "00040", "90007"));
    assert!(!overseas_fo_leg_certified(&OVERSEAS_FO_SUBMIT_OK, "00040", ""), "no order no");
    assert!(!overseas_fo_leg_certified(&OVERSEAS_FO_SUBMIT_OK, "00040", "0"), "zero order no");
    assert!(!overseas_fo_leg_certified(&OVERSEAS_FO_SUBMIT_OK, "00000", "90007"), "generic code");
    assert!(!overseas_fo_leg_certified(&OVERSEAS_FO_SUBMIT_OK, "00462", "90007"), "wrong-leg code");
}

// ---- U4: capability-verdict classifier — every bucket distinct --------------------

#[test]
fn classify_submit_error_maps_01491_to_capability_pending() {
    let err = LsError::ApiError {
        code: "01491".into(),
        message: "모의투자 주문이 불가한 계좌입니다.".into(),
    };
    assert_eq!(classify_submit_error(&err), CapabilityVerdict::PaperOrderIncapable);
}

#[test]
fn classify_submit_error_igw40011_is_request_shape_not_capability() {
    // On the ORDER path IGW40011 arrives as an HTTP-500 carried by AmbiguousOrder — NOT
    // ApiError, NOT the client-side Invalid. It must classify as a serde defect, never a
    // capability PENDING.
    let ambiguous = LsError::AmbiguousOrder {
        code: "IGW40011".into(),
        message: "numeric field serialized as string".into(),
    };
    assert_eq!(classify_submit_error(&ambiguous), CapabilityVerdict::RequestShapeDefect);
    // Belt-and-suspenders: even if a future gateway surfaced IGW40011 as an ApiError,
    // the rsp_cd match still catches it (we match the code, not the variant).
    let as_api = LsError::ApiError {
        code: "IGW40011".into(),
        message: "x".into(),
    };
    assert_eq!(classify_submit_error(&as_api), CapabilityVerdict::RequestShapeDefect);
    // And the client-side preflight Invalid (which never reached the wire, so placed
    // nothing) is NOT an IGW40011, NOT a capability verdict, and NOT an ambiguous
    // placement — it is an OtherRejection with no code.
    let invalid = LsError::Invalid {
        field: "OrdQty".into(),
        reason: "empty".into(),
    };
    assert_eq!(
        classify_submit_error(&invalid),
        CapabilityVerdict::OtherRejection(String::new())
    );
}

/// REGRESSION (reliability P1 + cross-model adversarial): an AMBIGUOUS or transport-level
/// submit outcome — where the order MAY have reached the exchange and be resting now — must
/// classify as `AmbiguousPlacement` (which the harness fails CLOSED on), NEVER a silent
/// `OtherRejection` PENDING that returns and leaves an uncancelable resting order. This is
/// the fail-OPEN both the in-process reliability reviewer and the cross-model Codex pass
/// independently caught.
#[test]
fn classify_submit_error_ambiguous_or_transport_is_may_rest_not_pending() {
    // A non-2xx HTTP with no parseable rsp_cd (the 503 / gateway-5xx transport case;
    // inner.rs surfaces it as AmbiguousOrder{code:""} on the order path).
    let transport_5xx = LsError::AmbiguousOrder {
        code: String::new(),
        message: "502 Bad Gateway".into(),
    };
    assert_eq!(
        classify_submit_error(&transport_5xx),
        CapabilityVerdict::AmbiguousPlacement,
        "an empty-code AmbiguousOrder means the order MAY rest — must be AmbiguousPlacement"
    );
    assert_ne!(
        classify_submit_error(&transport_5xx),
        CapabilityVerdict::OtherRejection(String::new()),
        "must NOT collapse into the placed-nothing OtherRejection bucket (fail-OPEN guard)"
    );
    // An AmbiguousOrder carrying a non-IGW40011 code (e.g. an ambiguous ack) is still
    // may-rest — the code is not IGW40011 and the variant is ambiguous.
    let ambiguous_ack = LsError::AmbiguousOrder {
        code: "00000".into(),
        message: "ambiguous ack".into(),
    };
    assert_eq!(classify_submit_error(&ambiguous_ack), CapabilityVerdict::AmbiguousPlacement);
    // A 2xx response that failed to decode (order landed, no order number extractable) is
    // also may-rest.
    let decode_err = LsError::Decode(serde_json::from_str::<i32>("not-an-int").unwrap_err());
    assert_eq!(classify_submit_error(&decode_err), CapabilityVerdict::AmbiguousPlacement);
    // Guardrail: an ambiguous placement is NEVER a capability PENDING.
    assert_ne!(
        classify_submit_error(&transport_5xx),
        CapabilityVerdict::PaperOrderIncapable
    );
}

#[test]
fn classify_submit_error_venue_closed_is_not_capability_pending() {
    let err = LsError::ApiError {
        code: "01458".into(),
        message: "장종료".into(),
    };
    assert_eq!(
        classify_submit_error(&err),
        CapabilityVerdict::VenueClosed("01458".into())
    );
}

#[test]
fn classify_submit_error_unknown_code_is_recorded_verbatim_never_01491() {
    let err = LsError::ApiError {
        code: "01442".into(), // a band/tick-style rejection, e.g.
        message: "some other rejection".into(),
    };
    let v = classify_submit_error(&err);
    assert_eq!(v, CapabilityVerdict::OtherRejection("01442".into()));
    assert_ne!(v, CapabilityVerdict::PaperOrderIncapable, "must NEVER bin as 01491");
    // 01900 is its own bucket, distinct from 01491.
    let paper = LsError::ApiError {
        code: "01900".into(),
        message: "x".into(),
    };
    assert_eq!(classify_submit_error(&paper), CapabilityVerdict::PaperIncompatible);
}

#[test]
fn capability_verdict_buckets_are_all_distinct() {
    // No two buckets collapse (the Req §4 anti-conflation invariant).
    let all = [
        CapabilityVerdict::Clears,
        CapabilityVerdict::PaperOrderIncapable,
        CapabilityVerdict::PaperIncompatible,
        CapabilityVerdict::VenueClosed("01458".into()),
        CapabilityVerdict::RequestShapeDefect,
        CapabilityVerdict::AmbiguousPlacement,
        CapabilityVerdict::OtherRejection("01442".into()),
    ];
    for (i, a) in all.iter().enumerate() {
        for (j, b) in all.iter().enumerate() {
            assert_eq!(i == j, a == b, "bucket {i} vs {j} distinctness");
        }
    }
}

/// U4 scrub guard: the IGW40011 loud-failure/verdict-emit path must be credential-free —
/// an account-number-shaped token in the gateway message is masked before it reaches any
/// recorded evidence (mirrors `loud_failure_message_is_account_free_but_names_the_order`).
#[test]
fn igw40011_emitted_evidence_is_account_free() {
    let err = LsError::AmbiguousOrder {
        code: "IGW40011".into(),
        message: "rejected for account 20001652603 due to numeric wire type".into(),
    };
    assert_eq!(classify_submit_error(&err), CapabilityVerdict::RequestShapeDefect);
    // The harness routes err.to_string() through scrub_secrets before recording it.
    let scrubbed = scrub_secrets(&err.to_string());
    assert!(
        !scrubbed.contains("20001652603"),
        "account number must be masked in emitted IGW40011 evidence: {scrubbed}"
    );
    // A loud-failure line for this defect is likewise account-free.
    let line = loud_failure("overseas-fo-igw40011", &[], &scrubbed);
    assert!(!line.contains("20001652603"), "loud failure must be account-free: {line}");
}

// ---- U2/U3: offline wiremock chain — the facade drives all three legs -------------

fn sdk_for(server: &MockServer) -> LsSdk {
    let inner = Inner::new(mock_config(&server.uri())).expect("valid mock config");
    LsSdk::from_inner(inner)
}

const OVERSEAS_ORDER_PATH: &str = "/overseas-futureoption/order";

fn submit_req() -> CIDBT00100Request {
    CIDBT00100Request::new(
        "20260702", "ESU26", "1", "2", "1", "USD", "2106.5", "0", "2", "ES", "202609", "CME",
    )
}

/// U2/U3: the `overseas_fo_orders()` facade posts every leg to
/// `/overseas-futureoption/order` via the order path, and the submit→modify→cancel chain
/// surfaces each leg's order number + ack. Fully offline (wiremock), no live credentials.
#[tokio::test]
async fn overseas_fo_facade_drives_three_legs_on_order_path() {
    let server = MockServer::start().await;
    mount_token(&server).await;

    let submit_ack = r#"{ "rsp_cd": "00040", "rsp_msg": "ack",
        "CIDBT00100OutBlock2": { "OvrsFutsOrdNo": 90007 } }"#;
    let modify_ack = r#"{ "rsp_cd": "00462", "rsp_msg": "modify ack",
        "CIDBT00900OutBlock2": { "OvrsFutsOrdNo": 90009, "InnerMsgCnts": "정정 접수" } }"#;
    let cancel_ack = r#"{ "rsp_cd": "00463", "rsp_msg": "cancel ack",
        "CIDBT01000OutBlock2": { "OvrsFutsOrdNo": 90011, "InnerMsgCnts": "취소 접수" } }"#;

    // A single mock for the order path that returns the right ack per tr_cd header.
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_inner = hits.clone();
    Mock::given(method("POST"))
        .and(path(OVERSEAS_ORDER_PATH))
        .respond_with(move |req: &Request| {
            hits_inner.fetch_add(1, Ordering::SeqCst);
            let tr = req
                .headers
                .get("tr_cd")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            let body = match tr {
                "CIDBT00900" => modify_ack,
                "CIDBT01000" => cancel_ack,
                _ => submit_ack,
            };
            ResponseTemplate::new(200).set_body_string(body)
        })
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let submit = sdk
        .overseas_fo_orders()
        .submit(&submit_req())
        .await
        .expect("submit dispatches");
    assert_eq!(submit.order_no(), "90007");

    let modify = sdk
        .overseas_fo_orders()
        .modify(&CIDBT00900Request::new(
            "20260702",
            submit.order_no(),
            "ESU26",
            "1",
            "2",
            "1",
            "USD",
            "2106.5",
            "0",
            "1",
            "ES",
            "202609",
            "CME",
        ))
        .await
        .expect("modify dispatches");
    assert_eq!(modify.order_no(), "90009");
    assert_eq!(modify.ack(), "정정 접수");

    let cancel = sdk
        .overseas_fo_orders()
        .cancel(&CIDBT01000Request::new(
            "20260702",
            "ESU26",
            modify.order_no(),
            "1",
            "F",
            "CME",
        ))
        .await
        .expect("cancel dispatches");
    assert_eq!(cancel.order_no(), "90011");
    assert_eq!(cancel.ack(), "취소 접수");

    assert_eq!(hits.load(Ordering::SeqCst), 3, "all three legs hit the order path");
}

/// A 503-forever responder that counts every hit.
struct Counting(Arc<AtomicUsize>, u16);
impl wiremock::Respond for Counting {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        self.0.fetch_add(1, Ordering::SeqCst);
        ResponseTemplate::new(self.1)
    }
}

/// No-retry: an overseas-F/O order 5xx is dispatched exactly ONCE (a blind order retry
/// risks a double fill) and surfaces as an ambiguous outcome — never a clean rejection.
#[tokio::test]
async fn overseas_fo_submit_5xx_is_single_attempt_no_retry() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    let hits = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path(OVERSEAS_ORDER_PATH))
        .respond_with(Counting(hits.clone(), 503))
        .mount(&server)
        .await;

    let err = sdk_for(&server)
        .overseas_fo_orders()
        .submit(&submit_req())
        .await
        .unwrap_err();
    assert!(
        matches!(err, LsError::AmbiguousOrder { .. } | LsError::Http(_)),
        "a 5xx order is ambiguous, got {err:?}"
    );
    assert_eq!(hits.load(Ordering::SeqCst), 1, "order dispatch must not retry");
}

/// Dedup: an identical overseas-F/O submit within the window returns the cached response
/// and never hits HTTP a second time.
#[tokio::test]
async fn overseas_fo_identical_submit_is_a_dedup_hit() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_inner = hits.clone();
    Mock::given(method("POST"))
        .and(path(OVERSEAS_ORDER_PATH))
        .respond_with(move |_: &Request| {
            hits_inner.fetch_add(1, Ordering::SeqCst);
            ResponseTemplate::new(200).set_body_string(
                r#"{ "rsp_cd": "00040", "rsp_msg": "ack",
                    "CIDBT00100OutBlock2": { "OvrsFutsOrdNo": 90007 } }"#,
            )
        })
        .mount(&server)
        .await;

    let sdk = sdk_for(&server);
    let a = sdk.overseas_fo_orders().submit(&submit_req()).await.expect("first dispatches");
    let b = sdk.overseas_fo_orders().submit(&submit_req()).await.expect("second is cached");
    assert_eq!(a.order_no(), b.order_no());
    assert_eq!(hits.load(Ordering::SeqCst), 1, "dedup hit must bypass HTTP");
}
