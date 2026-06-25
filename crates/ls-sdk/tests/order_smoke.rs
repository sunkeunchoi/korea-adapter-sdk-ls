//! Guarded manual order-evidence harness (order-safety §4) — the order class's
//! Implemented gate. Operator-initiated, paper-only, and FAIL-CLOSED.
//!
//! Unlike a read smoke, placing an order is a real, irreversible market action,
//! so this harness guards harder than `paper_guard` alone:
//!
//! 1. `LS_TRADING_ENV` must be explicitly `paper` (the shared production guard).
//! 2. `LS_ORDER_SMOKE=1` must be set explicitly — a normal paper run never places
//!    an order by accident.
//! 3. The order TR is selected EXPLICITLY with NO default; an unset/unknown
//!    selection produces structured "not certified" evidence, never a submit.
//! 4. Operator parameters are validated BEFORE SDK construction; invalid params
//!    produce "not certified" evidence.
//! 5. The daily price band is fetched via `t1102` and validated; a degenerate
//!    band (halted / limit-locked / newly-listed symbol) records "not certified"
//!    rather than placing on a bad band (KTD8).
//!
//! The scenario matrix — a resting far-from-market limit buy and sell, one
//! marketable order, and one deliberate out-of-band rejection — pins the R4
//! order success predicate from OBSERVED `rsp_cd`/`rsp_msg` codes. Resting orders
//! are priced at the band's far edge (buy at `dnlmtprice`, sell at `uplmtprice`):
//! valid yet far from market, so they rest unfilled and are observable by `t0425`.
//!
//! If the paper account cannot place an order in-window (paper returns `01900`
//! service-not-provided, `01491` account-not-order-capable, or empty), the run
//! records **Pending** — the TRs stay callable-but-unconfirmed, no flip (AE5).
//!
//! Two live runs share these guards/helpers:
//! - `order_smoke_matrix` (`make live-smoke-order`) — the submit-only matrix
//!   (resting buy/sell, marketable, deliberate reject), gate 1's broad predicate
//!   evidence. Teardown is by paper reset.
//! - `order_chained_smoke` (`make live-smoke-order-chain`) — submit → modify →
//!   cancel against one real order number. Its FIRST leg is gate 1's evidence; the
//!   modify/cancel legs are gate 2's. **Cancel is now the primary teardown** (the
//!   wave that adds `CSPAT00801`); paper reset is the fallback when the cancel link
//!   fails or a resting order fills unexpectedly. A failure after gate 1 leaves
//!   only gate 2 Pending — gate 1 never waits on gate 2.
//!
//! The offline `#[test]`s below prove the fail-closed contract and run in the
//! normal suite. The live runs are `#[ignore]` and run only via the make targets
//! with `.env` credentials.

#![allow(dead_code)] // helpers are exercised by offline tests + the ignored live run.

use ls_core::{LsConfig, LsError, LsResult};
use ls_sdk::market_session::T1102Request;
use ls_sdk::orders::{
    CSPAT00601Request, CSPAT00701Request, CSPAT00801Request, OrderIntent, OrderState,
};
use ls_sdk::LsSdk;

// ---------------------------------------------------------------------------
// Guards (production + order opt-in)
// ---------------------------------------------------------------------------

/// Shared production guard — `LS_TRADING_ENV` must be explicitly `paper`.
fn paper_guard() -> LsResult<()> {
    match std::env::var("LS_TRADING_ENV") {
        Ok(v) if v.eq_ignore_ascii_case("paper") => Ok(()),
        Ok(v) => Err(LsError::Config(format!(
            "order smoke refuses to run: LS_TRADING_ENV must be explicitly 'paper', got '{v}'"
        ))),
        Err(_) => Err(LsError::Config(
            "order smoke refuses to run: LS_TRADING_ENV must be explicitly 'paper' (unset \
             is not allowed)"
                .into(),
        )),
    }
}

/// Order opt-in guard — placing an order requires an EXPLICIT second opt-in
/// beyond the paper guard, so no read-smoke run ever submits an order.
fn order_smoke_guard() -> LsResult<()> {
    paper_guard()?;
    match std::env::var("LS_ORDER_SMOKE") {
        Ok(v) if v == "1" || v.eq_ignore_ascii_case("true") => Ok(()),
        _ => Err(LsError::Config(
            "order smoke refuses to run: LS_ORDER_SMOKE must be explicitly '1' (placing a \
             live paper order is opt-in beyond the paper guard)"
                .into(),
        )),
    }
}

/// Build a real, gateway-pointed SDK after BOTH guards pass.
fn order_smoke_sdk() -> LsResult<LsSdk> {
    order_smoke_guard()?;
    let config = LsConfig::from_env()?;
    if !config.environment.is_paper() {
        return Err(LsError::Config(
            "resolved environment is not Paper after the guards passed — refusing".into(),
        ));
    }
    LsSdk::new(config)
}

// ---------------------------------------------------------------------------
// Explicit TR selection (no default)
// ---------------------------------------------------------------------------

/// The submit order TR (the matrix harness places only this one).
const ORDER_TR: &str = "CSPAT00601";

/// The order TRs this harness can dispatch. The submit matrix places `CSPAT00601`;
/// the chained smoke additionally drives the modify/cancel TRs against a real order
/// number. Selection is EXPLICIT — there is no default. (The non-place TRs the chain
/// uses are still allow-listed here so a misconfigured selection fails closed.)
const ORDER_TR_ALLOWLIST: [&str; 3] = ["CSPAT00601", "CSPAT00701", "CSPAT00801"];

/// Select the submit-matrix order TR from `LS_ORDER_SMOKE_TR`. NO default: unset or
/// any value not in the allowlist is a fail-closed "not certified" condition. The
/// submit matrix only places `CSPAT00601`.
fn select_order_tr() -> Result<&'static str, String> {
    match std::env::var("LS_ORDER_SMOKE_TR") {
        Ok(v) if v == ORDER_TR => Ok(ORDER_TR),
        Ok(v) if ORDER_TR_ALLOWLIST.contains(&v.as_str()) => Err(format!(
            "TR '{v}' is order-class but the submit matrix places only {ORDER_TR} \
             (the chained smoke drives modify/cancel)"
        )),
        Ok(v) => Err(format!(
            "unsupported order TR selection '{v}' (allowed: {ORDER_TR_ALLOWLIST:?})"
        )),
        Err(_) => Err(format!(
            "no order TR selected (set LS_ORDER_SMOKE_TR={ORDER_TR}); refusing to default"
        )),
    }
}

// ---------------------------------------------------------------------------
// Scenario matrix (explicit, no default)
// ---------------------------------------------------------------------------

/// The four evidence scenarios. Each uses DISTINCT order params so an identical
/// re-run regenerates fresh broker codes instead of a dedup cache hit (AE3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scenario {
    /// A resting limit buy at the band floor — valid, far below market.
    RestingBuy,
    /// A resting limit sell at the band ceiling — valid, far above market.
    RestingSell,
    /// A marketable/immediate order (the matrix's only fill-prone scenario).
    Marketable,
    /// A deliberate out-of-band-price order for a deterministic rejection.
    DeliberateReject,
}

impl Scenario {
    fn as_str(&self) -> &'static str {
        match self {
            Scenario::RestingBuy => "resting_buy",
            Scenario::RestingSell => "resting_sell",
            Scenario::Marketable => "marketable",
            Scenario::DeliberateReject => "deliberate_reject",
        }
    }

    /// Parse an explicit scenario — NO default.
    fn parse(s: &str) -> Result<Scenario, String> {
        match s {
            "resting_buy" => Ok(Scenario::RestingBuy),
            "resting_sell" => Ok(Scenario::RestingSell),
            "marketable" => Ok(Scenario::Marketable),
            "deliberate_reject" => Ok(Scenario::DeliberateReject),
            other => Err(format!("unknown scenario '{other}' (no default)")),
        }
    }

    fn all() -> [Scenario; 4] {
        [
            Scenario::RestingBuy,
            Scenario::RestingSell,
            Scenario::Marketable,
            Scenario::DeliberateReject,
        ]
    }
}

// ---------------------------------------------------------------------------
// Daily price band (KTD8)
// ---------------------------------------------------------------------------

/// A validated daily price band from `t1101`.
#[derive(Debug, Clone, Copy)]
struct Band {
    uplmt: u64,
    dnlmt: u64,
}

/// Validate a `t1101` band: both prices parse, are non-zero, and `up > dn`. A
/// degenerate band (halted / limit-locked / newly-listed symbol) is rejected so
/// the harness records "not certified" instead of placing on a bad band.
fn validate_band(uplmtprice: &str, dnlmtprice: &str) -> Result<Band, String> {
    let up: u64 = uplmtprice
        .trim()
        .parse()
        .map_err(|_| format!("unparseable uplmtprice '{uplmtprice}'"))?;
    let dn: u64 = dnlmtprice
        .trim()
        .parse()
        .map_err(|_| format!("unparseable dnlmtprice '{dnlmtprice}'"))?;
    if up == 0 || dn == 0 {
        return Err(format!("degenerate band (zero): up={up} dn={dn}"));
    }
    if up <= dn {
        return Err(format!("degenerate band (up<=dn): up={up} dn={dn}"));
    }
    Ok(Band { uplmt: up, dnlmt: dn })
}

/// KRX price tick ladder (2023+) — the on-tick increment for a given price.
fn tick(price: u64) -> u64 {
    match price {
        p if p < 2_000 => 1,
        p if p < 5_000 => 5,
        p if p < 20_000 => 10,
        p if p < 50_000 => 50,
        p if p < 200_000 => 100,
        p if p < 500_000 => 500,
        _ => 1_000,
    }
}

impl Band {
    /// Resting BUY price — at the floor (`dnlmtprice`): valid, far below market,
    /// so it rests unfilled.
    fn resting_buy_price(&self) -> u64 {
        self.dnlmt
    }
    /// Resting SELL price — at the ceiling (`uplmtprice`): valid, far above
    /// market, so it rests unfilled.
    fn resting_sell_price(&self) -> u64 {
        self.uplmt
    }
    /// An out-of-band BUY price — one tick BELOW the floor → a deterministic
    /// price-limit rejection.
    fn out_of_band_buy_price(&self) -> u64 {
        self.dnlmt.saturating_sub(tick(self.dnlmt)).max(1)
    }
}

// ---------------------------------------------------------------------------
// Credential-free evidence
// ---------------------------------------------------------------------------

/// The classification of an evidence record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Certification {
    Certified,
    NotCertified,
    Pending,
}

impl Certification {
    fn as_str(&self) -> &'static str {
        match self {
            Certification::Certified => "certified",
            Certification::NotCertified => "not-certified",
            Certification::Pending => "pending",
        }
    }
}

/// A credential-free evidence record for one scenario. Carries NO token, appkey,
/// secret, or account number — only the TR, scenario, classification, observed
/// `rsp_cd`/`rsp_msg`, order number/time, and reconciliation observation.
#[derive(Debug, Clone)]
struct OrderEvidence {
    tr_code: String,
    scenario: String,
    certification: Certification,
    rsp_cd: String,
    rsp_msg: String,
    order_no: Option<String>,
    reconciliation: Option<String>,
    /// Always true — production order testing is prohibited and was not run.
    production_not_run: bool,
}

impl OrderEvidence {
    fn not_certified(scenario: &str, reason: &str) -> Self {
        OrderEvidence {
            tr_code: ORDER_TR.into(),
            scenario: scenario.into(),
            certification: Certification::NotCertified,
            rsp_cd: String::new(),
            rsp_msg: reason.into(),
            order_no: None,
            reconciliation: None,
            production_not_run: true,
        }
    }

    fn pending(scenario: &str, reason: &str) -> Self {
        OrderEvidence {
            certification: Certification::Pending,
            ..Self::not_certified(scenario, reason)
        }
    }

    /// Print a credential-free evidence line. `rsp_msg` is gateway-controlled
    /// localized text that can embed an account number, so it is scrubbed of
    /// account-number-like digit runs before printing (§4/§5).
    fn record(&self) {
        println!(
            "ORDER-SMOKE tr={} scenario={} cert={} rsp_cd={} order_no={} recon={} \
             production_not_run={} msg=[{}]",
            self.tr_code,
            self.scenario,
            self.certification.as_str(),
            self.rsp_cd,
            self.order_no.as_deref().unwrap_or("-"),
            self.reconciliation.as_deref().unwrap_or("-"),
            self.production_not_run,
            scrub_digit_runs(&self.rsp_msg),
        );
    }
}

/// Mask any run of 6+ digits (account-number-like) with `***`, so a localized
/// broker `rsp_msg` cannot leak an account number into recorded evidence.
fn scrub_digit_runs(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut run = 0usize;
    let mut buf = String::new();
    let flush = |out: &mut String, buf: &mut String, run: usize| {
        if run >= 6 {
            out.push_str("***");
        } else {
            out.push_str(buf);
        }
        buf.clear();
    };
    for c in s.chars() {
        if c.is_ascii_digit() {
            run += 1;
            buf.push(c);
        } else {
            flush(&mut out, &mut buf, run);
            run = 0;
            out.push(c);
        }
    }
    flush(&mut out, &mut buf, run);
    out
}

/// Operator order parameters, validated BEFORE SDK construction.
struct OrderParams {
    symbol: String,
    member_no: String,
}

/// Validate operator params: symbol non-empty and a plausible domestic code,
/// member number non-empty. Runs before any SDK construction or dispatch.
fn validate_params(symbol: &str, member_no: &str) -> Result<OrderParams, String> {
    let symbol = symbol.trim();
    let member_no = member_no.trim();
    if symbol.is_empty() {
        return Err("empty symbol".into());
    }
    if !symbol.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(format!("implausible symbol '{symbol}'"));
    }
    if member_no.is_empty() {
        return Err("empty member number".into());
    }
    Ok(OrderParams {
        symbol: symbol.to_string(),
        member_no: member_no.to_string(),
    })
}

// ===========================================================================
// Offline fail-closed tests (run in the normal suite)
// ===========================================================================

#[test]
fn order_smoke_guard_requires_paper_and_explicit_optin() {
    let saved_env = std::env::var("LS_TRADING_ENV").ok();
    let saved_optin = std::env::var("LS_ORDER_SMOKE").ok();

    std::env::set_var("LS_TRADING_ENV", "paper");
    std::env::remove_var("LS_ORDER_SMOKE");
    assert!(
        order_smoke_guard().is_err(),
        "paper alone must NOT enable order placement"
    );
    std::env::set_var("LS_ORDER_SMOKE", "1");
    assert!(order_smoke_guard().is_ok(), "paper + explicit opt-in passes");

    std::env::set_var("LS_TRADING_ENV", "real");
    assert!(order_smoke_guard().is_err(), "real must be refused even with opt-in");

    match saved_env {
        Some(v) => std::env::set_var("LS_TRADING_ENV", v),
        None => std::env::remove_var("LS_TRADING_ENV"),
    }
    match saved_optin {
        Some(v) => std::env::set_var("LS_ORDER_SMOKE", v),
        None => std::env::remove_var("LS_ORDER_SMOKE"),
    }
}

#[test]
fn order_tr_selection_has_no_default() {
    let saved = std::env::var("LS_ORDER_SMOKE_TR").ok();

    std::env::remove_var("LS_ORDER_SMOKE_TR");
    assert!(select_order_tr().is_err(), "unset selection must not default");
    std::env::set_var("LS_ORDER_SMOKE_TR", "t0425");
    assert!(select_order_tr().is_err(), "a non-order TR must be refused");
    std::env::set_var("LS_ORDER_SMOKE_TR", ORDER_TR);
    assert_eq!(select_order_tr().unwrap(), ORDER_TR);

    match saved {
        Some(v) => std::env::set_var("LS_ORDER_SMOKE_TR", v),
        None => std::env::remove_var("LS_ORDER_SMOKE_TR"),
    }
}

#[test]
fn scenario_parse_has_no_default() {
    assert!(Scenario::parse("").is_err());
    assert!(Scenario::parse("buy").is_err());
    assert_eq!(Scenario::parse("resting_buy").unwrap(), Scenario::RestingBuy);
    assert_eq!(Scenario::all().len(), 4);
}

#[test]
fn invalid_params_are_rejected_before_construction() {
    assert!(validate_params("", "NXT").is_err(), "empty symbol");
    assert!(validate_params("00 59", "NXT").is_err(), "implausible symbol");
    assert!(validate_params("005930", "").is_err(), "empty member");
    let ok = validate_params("005930", "NXT").unwrap();
    assert_eq!(ok.symbol, "005930");
}

#[test]
fn degenerate_band_is_not_certified() {
    assert!(validate_band("0", "0").is_err(), "zero band");
    assert!(validate_band("42000", "42000").is_err(), "up==dn (limit-locked)");
    assert!(validate_band("100", "42000").is_err(), "up<dn inverted");
    assert!(validate_band("nan", "1").is_err(), "unparseable");
    let band = validate_band("54600", "29400").expect("a healthy band");
    assert_eq!(band.uplmt, 54_600);
    assert_eq!(band.dnlmt, 29_400);
}

#[test]
fn resting_prices_sit_inside_the_band_and_reject_price_is_outside() {
    let band = validate_band("54600", "29400").unwrap();
    // Resting buy at the floor, sell at the ceiling: valid, far from market.
    assert_eq!(band.resting_buy_price(), 29_400);
    assert_eq!(band.resting_sell_price(), 54_600);
    assert!(band.resting_buy_price() >= band.dnlmt && band.resting_buy_price() <= band.uplmt);
    assert!(band.resting_sell_price() >= band.dnlmt && band.resting_sell_price() <= band.uplmt);
    // The deliberate-reject price is strictly below the floor (out of band).
    assert!(
        band.out_of_band_buy_price() < band.dnlmt,
        "out-of-band price must be below the floor"
    );
}

#[test]
fn scrub_masks_account_number_like_digit_runs() {
    // A 6+ digit run (account-number-like) is masked; short numbers survive.
    assert_eq!(scrub_digit_runs("계좌 1234567890 거부"), "계좌 *** 거부");
    assert_eq!(scrub_digit_runs("qty 12 price 100"), "qty 12 price 100");
    assert_eq!(scrub_digit_runs("주문완료"), "주문완료");
    assert!(!scrub_digit_runs("acct 0000000001 done").contains("0000000001"));
}

#[test]
fn evidence_is_credential_free_and_states_production_not_run() {
    let ev = OrderEvidence {
        tr_code: ORDER_TR.into(),
        scenario: "resting_buy".into(),
        certification: Certification::Certified,
        rsp_cd: "00040".into(),
        rsp_msg: "ack".into(),
        order_no: Some("32004".into()),
        reconciliation: Some("accepted".into()),
        production_not_run: true,
    };
    // The structured record carries no credential/account fields by construction:
    // there is no field for a token, appkey, secret, or account number.
    assert!(ev.production_not_run);
    assert_eq!(ev.certification.as_str(), "certified");
    // not_certified / pending fail-closed constructors.
    let nc = OrderEvidence::not_certified("resting_buy", "degenerate band");
    assert_eq!(nc.certification, Certification::NotCertified);
    assert!(nc.production_not_run);
    let p = OrderEvidence::pending("resting_buy", "account not order-capable");
    assert_eq!(p.certification, Certification::Pending);
}

// ===========================================================================
// Live matrix (ignored; runs only via `make live-smoke-order`)
// ===========================================================================

/// Build a `CSPAT00601` request for a scenario against a validated band.
fn build_order(scenario: Scenario, params: &OrderParams, band: &Band) -> CSPAT00601Request {
    let (qty, price, side) = match scenario {
        // Distinct quantities per scenario so an identical re-run misses the dedup
        // cache and regenerates fresh broker codes (AE3).
        Scenario::RestingBuy => ("1", band.resting_buy_price(), "2"),
        Scenario::RestingSell => ("2", band.resting_sell_price(), "1"),
        Scenario::Marketable => ("3", band.uplmt, "2"), // marketable buy at the ceiling
        Scenario::DeliberateReject => ("4", band.out_of_band_buy_price(), "2"),
    };
    CSPAT00601Request::limit(&params.symbol, qty, price.to_string(), side, &params.member_no)
}

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

// ===========================================================================
// Chained live run (submit → modify → cancel) — gate 2 evidence; its FIRST leg
// is gate 1's. `#[ignore]`; runs only via `make live-smoke-order-chain`.
// ===========================================================================

/// A blank Pending evidence record for one chain leg (a specific order TR).
fn leg_evidence(tr_code: &str, scenario: &str) -> OrderEvidence {
    OrderEvidence {
        tr_code: tr_code.into(),
        scenario: scenario.into(),
        certification: Certification::Pending,
        rsp_cd: String::new(),
        rsp_msg: String::new(),
        order_no: None,
        reconciliation: None,
        production_not_run: true,
    }
}

/// Dump the credential-free `t0425` rows for the referenced order so the operator
/// can PIN the modify-replace shape (KTD4): whether the original `OrgOrdNo` row
/// moves to `정정`/`정정확인` or stays `접수`, and whether `t0425.orgordno` carries
/// the immediate parent. Prints only order numbers + status (no account, no creds);
/// `rsp_msg`-style free text is not emitted here.
async fn dump_t0425_rows(sdk: &LsSdk, symbol: &str, org_ordno: &str) {
    match sdk
        .orders()
        .inquiry(&ls_sdk::orders::T0425Request::for_symbol(symbol))
        .await
    {
        Ok(resp) => {
            for r in &resp.outblock1 {
                println!(
                    "ORDER-CHAIN t0425-row org_ref={org_ordno} ordno={} orgordno={} \
                     status=[{}] ordrem={} cheqty={}",
                    r.ordno, r.orgordno, r.status, r.ordrem, r.cheqty
                );
            }
        }
        Err(e) => println!("ORDER-CHAIN t0425-dump failed: {e}"),
    }
}

/// Guarded CHAINED paper-order evidence run: submit a resting far-from-market
/// order (gate 1 evidence), modify it, then cancel it as teardown — each observed
/// via `t0425`. Cancel is the PRIMARY teardown; paper-reset is the fallback when
/// the cancel link itself fails or a resting order fills unexpectedly (AE5). Records
/// Pending (never fails the build) when the paper account cannot place/modify/cancel
/// in-window. `#[ignore]` — runs only via `make live-smoke-order-chain`.
#[tokio::test]
#[ignore = "guarded chained paper order: needs credentials + LS_ORDER_SMOKE=1; run via `make live-smoke-order-chain`"]
async fn order_chained_smoke() {
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
            OrderEvidence::pending("band", &format!("t1102 band fetch failed: {e}")).record();
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
            sev.rsp_msg = format!("submit failed: {e}");
            sev.record();
            OrderEvidence::pending("chain", "submit leg did not place; chain not run").record();
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
            mev.rsp_msg = format!("modify failed: {e}");
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
            // The cancel link itself failed → paper-reset fallback teardown; gate 2
            // does not flip; gate 1 is unaffected (AE5).
            cev.rsp_msg = format!("cancel failed: {e}");
            cev.record();
            OrderEvidence::pending(
                "chain",
                "cancel link failed; paper-reset fallback teardown; gate 2 not flipped",
            )
            .record();
            println!(
                "ORDER-CHAIN teardown=paper-reset note=[cancel failed; operator must reset the \
                 paper book to clear order {live_ordno}]"
            );
            return;
        }
    }

    println!(
        "ORDER-CHAIN teardown=cancel note=[cancel is the primary teardown; if an unexpected fill \
         occurred mid-chain, operator unwinds out-of-band via paper reset]"
    );
}
