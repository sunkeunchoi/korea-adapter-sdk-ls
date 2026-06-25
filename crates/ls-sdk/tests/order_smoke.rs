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
//! If the paper account cannot place an order in-window (not order-capable, paper
//! returns `01900`, or empty), the run records **Pending** — the TRs stay
//! callable-but-unconfirmed, no flip (AE5). Cleanup of any resting order is by
//! paper reset (the only verified teardown — cancel TRs are deferred); a missing
//! in-window clearing mechanism is a blocking Pending condition, not a silent gap.
//!
//! The offline `#[test]`s below prove the fail-closed contract and run in the
//! normal suite. The live matrix is `#[ignore]` and runs only via
//! `make live-smoke-order` with `.env` credentials.

#![allow(dead_code)] // helpers are exercised by offline tests + the ignored live run.

use ls_core::{LsConfig, LsError, LsResult};
use ls_sdk::market_session::T1102Request;
use ls_sdk::orders::{CSPAT00601Request, OrderIntent, OrderState};
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

/// The only order TR this harness can place. Selection is EXPLICIT.
const ORDER_TR: &str = "CSPAT00601";

/// Select the order TR from `LS_ORDER_SMOKE_TR`. NO default: unset or any value
/// other than the supported TR is a fail-closed "not certified" condition.
fn select_order_tr() -> Result<&'static str, String> {
    match std::env::var("LS_ORDER_SMOKE_TR") {
        Ok(v) if v == ORDER_TR => Ok(ORDER_TR),
        Ok(v) => Err(format!(
            "unsupported order TR selection '{v}' (only {ORDER_TR} is supported)"
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
                    let intent = OrderIntent {
                        account_no: sdk.orders().account_no().to_string(),
                        symbol: params.symbol.clone(),
                        side: req.inblock.bnstpcode.clone(),
                        qty: req.inblock.ordqty.clone(),
                        price: req.inblock.ordprc.clone(),
                        order_no: Some(resp.order_no().to_string()),
                    };
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
                } else if code == "01900" {
                    Certification::Pending // paper-incompatible → not order-capable
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
