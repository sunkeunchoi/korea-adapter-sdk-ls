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
    T0425InBlock, T0425OutBlock1, T0425Request, T0425Response,
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
    // U2 (R2/AE2): the resolved environment — not the shell env var — is the
    // enforceable runtime invariant. `LS_TRADING_ENV=paper` is the first gate
    // (order_smoke_guard), but credentials could still resolve a non-paper
    // environment; refuse on the resolved value before any placement.
    assert_resolved_paper(&config.environment)?;
    LsSdk::new(config)
}

/// Build a real, gateway-pointed SDK for an AUTONOMOUS (agent-invoked) run. Layers
/// the U1 autonomy precondition (CI/no-TTY + per-wave nonce) ahead of every existing
/// guard, then the standard paper-resolved `order_smoke_sdk`. Used by the chained
/// smoke so an agent can drive it during a human-present wave without an operator
/// handoff — never authorizing an unattended order (R1).
fn autonomous_order_smoke_sdk() -> LsResult<LsSdk> {
    autonomy_guard()?;
    order_smoke_sdk()
}

// ---------------------------------------------------------------------------
// U2 — post-credential-load paper assertion (R2/AE2)
// ---------------------------------------------------------------------------

/// Assert the RESOLVED environment is paper after credential load. The shell
/// `LS_TRADING_ENV=paper` check (`paper_guard`) is necessary but not sufficient —
/// `from_env` resolves the real environment, and that resolved value is the
/// enforceable invariant. Pure so the fail-closed contract is offline-testable.
fn assert_resolved_paper(env: &ls_core::Environment) -> LsResult<()> {
    if env.is_paper() {
        Ok(())
    } else {
        Err(LsError::Config(format!(
            "order smoke refuses to place: resolved environment is '{env}', not paper, after the \
             guards passed — refusing (LS_TRADING_ENV alone is not trusted)"
        )))
    }
}

// ---------------------------------------------------------------------------
// U1 — fail-closed autonomy precondition (R1/AE1)
// ---------------------------------------------------------------------------

/// The TTL for a per-wave human-issued nonce (seconds). A fresh nonce is a unix
/// timestamp the human mints each wave (`export LS_ORDER_SMOKE_NONCE=$(date +%s)`);
/// once older than this it is rejected, so a static reusable constant degrades to an
/// expired timestamp within minutes and can never re-authorize placement.
const NONCE_TTL_SECS: i64 = 600;

/// Forward-skew tolerance (seconds) for a nonce timestamp, so a small clock
/// difference between the human's shell and the runner does not reject a fresh nonce.
const NONCE_MAX_SKEW_SECS: i64 = 60;

/// The decision inputs for the autonomy precondition, gathered from the environment
/// by [`autonomy_guard`]. Separated so the fail-closed decision is a PURE function
/// ([`check_autonomy`]) that offline tests can exercise across every scenario —
/// including no-TTY, which cannot be forced in-process.
struct AutonomyContext {
    /// `Some(reason)` when an unattended/CI marker is detected (CI env var or no TTY).
    unattended_marker: Option<String>,
    /// The raw `LS_ORDER_SMOKE_NONCE` value, if set.
    nonce: Option<String>,
    /// The current unix time (seconds) for TTL validation.
    now_unix: i64,
}

/// The fail-closed autonomy decision (R1/KTD1). Refuses unless BOTH hold:
///   1. no unattended/CI marker is present, AND
///   2. a per-wave human nonce is present AND fresh (within TTL).
/// Either failing refuses — passive CI detection alone cannot tell a human-present
/// agent wave from an unmarked headless runner, and the standing `LS_ORDER_SMOKE`
/// opt-in cannot either; the active fresh nonce is the human-present signal.
fn check_autonomy(ctx: &AutonomyContext) -> Result<(), String> {
    if let Some(reason) = &ctx.unattended_marker {
        return Err(format!(
            "refusing autonomous order placement: detected unattended context ({reason}); \
             the autonomous order smoke is bounded to interactive, human-present waves"
        ));
    }
    let Some(nonce) = ctx.nonce.as_deref() else {
        return Err(
            "refusing autonomous order placement: per-wave human nonce absent (mint a fresh one: \
             `export LS_ORDER_SMOKE_NONCE=$(date +%s)`) — the standing LS_ORDER_SMOKE opt-in \
             cannot distinguish an agent wave from CI"
            .to_string(),
        );
    };
    validate_nonce(nonce, ctx.now_unix)
}

/// Validate a per-wave nonce: it MUST be a fresh unix-seconds timestamp within TTL.
/// A non-numeric value (a static well-known constant) fails to parse; an old value
/// (a replayed / hardcoded constant) is expired; a far-future value is rejected as
/// implausible skew. So "valid nonce" can never degenerate to "env var present".
fn validate_nonce(nonce: &str, now_unix: i64) -> Result<(), String> {
    let nonce = nonce.trim();
    if nonce.is_empty() {
        return Err("refusing: LS_ORDER_SMOKE_NONCE is empty".into());
    }
    let issued: i64 = nonce.parse().map_err(|_| {
        "refusing: LS_ORDER_SMOKE_NONCE must be a fresh unix-seconds timestamp minted this wave \
         (`date +%s`), not a static constant"
            .to_string()
    })?;
    let age = now_unix - issued;
    if age > NONCE_TTL_SECS {
        return Err(format!(
            "refusing: LS_ORDER_SMOKE_NONCE is stale ({age}s old > {NONCE_TTL_SECS}s TTL) — a \
             replayed or hardcoded nonce cannot re-authorize placement; mint a fresh one this wave"
        ));
    }
    if age < -NONCE_MAX_SKEW_SECS {
        return Err(format!(
            "refusing: LS_ORDER_SMOKE_NONCE is from the future (skew {}s) — implausible, rejecting",
            -age
        ));
    }
    Ok(())
}

/// Gather the live autonomy context from the process environment and decide.
/// The CI/unattended marker is `CI`/`GITHUB_ACTIONS` being set, or stdin not being
/// a TTY; the nonce comes from `LS_ORDER_SMOKE_NONCE`; the clock is the wall clock.
fn autonomy_guard() -> LsResult<()> {
    let ctx = AutonomyContext {
        unattended_marker: detect_unattended_marker(),
        nonce: std::env::var("LS_ORDER_SMOKE_NONCE").ok(),
        now_unix: now_unix(),
    };
    check_autonomy(&ctx).map_err(LsError::Config)
}

/// Detect an unattended/CI context: a known CI env var set to a non-empty value, or
/// no TTY on stdin. `Some(reason)` means refuse.
fn detect_unattended_marker() -> Option<String> {
    for var in ["CI", "GITHUB_ACTIONS"] {
        if std::env::var_os(var).is_some_and(|v| !v.is_empty()) {
            return Some(format!("{var} env set"));
        }
    }
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        return Some("no TTY on stdin".into());
    }
    None
}

/// Current wall-clock unix time (seconds). The runtime core is clock-free, but the
/// test binary may read the wall clock to validate the nonce TTL.
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
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
    /// localized text that can embed an account number or other secret material, so
    /// it is routed through the widened [`scrub_secrets`] (account numbers + `-NN`
    /// suffix + bearer tokens/appkeys) before printing (R5/§4/§5).
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
            scrub_secrets(&self.rsp_msg),
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

// ---------------------------------------------------------------------------
// U4 — autonomous-run output safety (R5/AE5, KTD4)
// ---------------------------------------------------------------------------

/// Widened secret scrubbing for autonomous-run output (R5): the superset of
/// [`scrub_digit_runs`]. Masks any maximal `[A-Za-z0-9-]` token that either
/// (a) contains a 6+ consecutive-digit substring — an account number, with or
/// without a `-NN` product suffix (the suffix is inside the same token, so it is
/// masked too), or (b) is 20+ alphanumeric chars long — a bearer token / appkey.
/// Short numbers (quantities, prices) and order numbers (<6 digits, no suffix)
/// SURVIVE, so a loud failure can still name the order it is reporting.
fn scrub_secrets(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut run = String::new();
    let flush = |out: &mut String, run: &mut String| {
        if run_is_sensitive(run) {
            out.push_str("***");
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '-' {
            run.push(c);
        } else {
            flush(&mut out, &mut run);
            out.push(c);
        }
    }
    flush(&mut out, &mut run);
    out
}

/// `true` if a `[A-Za-z0-9-]` token is account- or secret-like: a 6+ consecutive
/// digit run (account number) or a 20+ alphanumeric token (bearer token / appkey).
fn run_is_sensitive(run: &str) -> bool {
    let mut digits = 0usize;
    for c in run.chars() {
        if c.is_ascii_digit() {
            digits += 1;
            if digits >= 6 {
                return true;
            }
        } else {
            digits = 0;
        }
    }
    run.chars().filter(|c| c.is_ascii_alphanumeric()).count() >= 20
}

/// Install a process-global tracing subscriber that DROPS the `ls_core` dispatch
/// debug events. Those events (`inner.rs` ~343 `rsp_msg`, ~353 raw `body`) log whole
/// broker text the digit-run scrubber never sees, so for an autonomous run they are
/// suppressed entirely (KTD4). FAIL-CLOSED: `tracing` allows exactly one global
/// default per process, so if a foreign subscriber is already installed we cannot
/// guarantee suppression — we refuse the run rather than fail OPEN on a known leak.
fn install_dispatch_log_suppressor() -> LsResult<()> {
    use tracing_subscriber::EnvFilter;
    // Drop everything below error globally and ls_core entirely — the dispatch leak
    // events are `debug!` on the `ls_core` target.
    let filter = EnvFilter::new("error,ls_core=off");
    let subscriber = tracing_subscriber::fmt().with_env_filter(filter).finish();
    tracing::subscriber::set_global_default(subscriber).map_err(|_| {
        LsError::Config(
            "refusing autonomous order run: a foreign global tracing subscriber is already \
             installed, so the unscrubbed ls_core dispatch debug log cannot be guaranteed \
             suppressed (KTD4) — failing closed rather than risking a secret leak"
                .into(),
        )
    })
}

/// Build a loud, account-free hard-failure message. The free-text `detail` is treated
/// as UNTRUSTED broker text and run through [`scrub_secrets`]; structured order
/// numbers are passed via `ordnos` (an order number is not a secret, so it is named
/// verbatim so the failure is actionable). Used for every NOT-flat / unexpected-fill /
/// cleanup-failure panic so no `rsp_msg` or `LsError` text is ever interpolated raw.
fn loud_failure(kind: &str, ordnos: &[String], detail: &str) -> String {
    format!(
        "ORDER-CHAIN HARD-FAIL kind={kind} ordnos=[{}] detail=[{}]",
        ordnos.join(","),
        scrub_secrets(detail),
    )
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

// ---------------------------------------------------------------------------
// U3 — post-run flat-account assertion (R3/R4, KTD2/KTD3)
// ---------------------------------------------------------------------------

/// The account-flatness verdict from an ACCOUNT-WIDE `t0425` working-orders scan.
/// "Flat" is concluded ONLY from positive confirmation — a completed scan with zero
/// live rows. A failed / timed-out / truncated scan is treated as NOT flat at the
/// call site and never produces a `Flat` here (R3).
#[derive(Debug, Clone, PartialEq, Eq)]
enum FlatVerdict {
    /// Positively confirmed flat — no live (resting or filled) order remains.
    Flat,
    /// One or more cancelable resting remainders (`ordrem > 0`, no fill). Carries
    /// their order numbers for the loud failure and best-effort retry-cancel.
    Resting(Vec<String>),
    /// One or more unexpected fills (`cheqty > 0`). A fill cannot be canceled — paper
    /// reset is the sole remediation, so this routes straight to a hard-fail.
    Fill(Vec<String>),
}

/// Parse a `t0425` quantity field (already string-normalized via `string_or_number`).
fn parse_qty(s: &str) -> u64 {
    s.trim().parse().unwrap_or(0)
}

/// Classify an account-wide `t0425` row set into a flatness verdict (KTD2/KTD3).
///
/// Keys on QUANTITIES, never the status TEXT (KTD2: "the flat check keys on
/// `ordrem`"). A FILL (`cheqty > 0`) outranks a resting remainder: a fill is
/// unrecoverable, so even a partial fill (`cheqty > 0` AND `ordrem > 0`) routes to
/// `Fill`. A cancelable remainder (`ordrem > 0`, no fill) is `Resting`. A row with
/// neither (`cheqty == 0` AND `ordrem == 0`) contributes nothing. Zero rows → `Flat`.
///
/// Why NOT a status-text "terminal" filter: a genuinely canceled order RELEASES its
/// remainder (`ordrem == 0`), so it is already flat by quantity. Crucially, a
/// cancel-REJECTED (`취소거부`) or modify-rejected (`정정거부`) order is STILL RESTING —
/// its status text contains 취소/거부 but the order was not removed, so a text filter
/// would wrongly skip it and conclude flat while a paper order rests. The production
/// `reconcile::classify_status` likewise treats 거부 as still-live, never terminal.
///
/// NOTE this counts EVERY live row, not just the smoke's own order — that is the
/// whole point of the account-wide scan vs the per-intent `reconcile_rows`: a
/// leftover resting order from a prior aborted run must surface as NOT flat.
fn flat_verdict(rows: &[T0425OutBlock1]) -> FlatVerdict {
    let mut fills = Vec::new();
    let mut resting = Vec::new();
    for r in rows {
        let cheqty = parse_qty(&r.cheqty);
        let ordrem = parse_qty(&r.ordrem);
        if cheqty > 0 {
            fills.push(r.ordno.trim().to_string());
        } else if ordrem > 0 {
            resting.push(r.ordno.trim().to_string());
        }
        // cheqty == 0 && ordrem == 0: nothing filled, nothing resting — a genuinely
        // canceled / fully-terminal order contributes nothing to flatness.
    }
    if !fills.is_empty() {
        FlatVerdict::Fill(fills)
    } else if !resting.is_empty() {
        FlatVerdict::Resting(resting)
    } else {
        FlatVerdict::Flat
    }
}

/// Run the ACCOUNT-WIDE `t0425` working-orders scan, exhausting every page (KTD2).
/// `expcode` empty queries all symbols; `collect_all` follows the `cts_ordno`/
/// `tr_cont` continuation to the terminal page. Returns `Err` on any failure OR a
/// truncated scan — the caller treats that as NOT flat (positive confirmation only).
async fn scan_account_wide_working_orders(sdk: &LsSdk) -> Result<Vec<T0425OutBlock1>, String> {
    use std::sync::Arc;
    let base = T0425Request {
        inblock: T0425InBlock {
            expcode: String::new(), // all symbols — catch leftovers from prior runs
            chegb: "0".into(),      // filled + unfilled
            medosu: "0".into(),     // both sides
            sortgb: "2".into(),
            cts_ordno: " ".into(),
        },
        tr_cont: String::new(),
        tr_cont_key: String::new(),
    };
    let inner = Arc::clone(sdk.inner());
    let pages = sdk
        .inner()
        .collect_all(base, move |req| {
            let inner = Arc::clone(&inner);
            async move {
                inner
                    .post_paginated::<T0425Request, T0425Response>(
                        &ls_core::endpoint_policy::T0425_POLICY,
                        &req,
                    )
                    .await
            }
        })
        .await;
    match pages {
        Ok(pages) => Ok(pages.into_iter().flat_map(|p| p.outblock1).collect()),
        Err(e) => Err(format!(
            "account-wide t0425 scan did not complete ({}) — cannot positively confirm flat",
            scrub_secrets(&e.to_string())
        )),
    }
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

// ---- U1: autonomy precondition (R1/AE1) ---------------------------------

/// A fresh, in-window nonce + no unattended marker passes the precondition.
fn fresh_ctx() -> AutonomyContext {
    let now = 1_700_000_000;
    AutonomyContext {
        unattended_marker: None,
        nonce: Some(now.to_string()),
        now_unix: now,
    }
}

#[test]
fn autonomy_passes_only_with_fresh_nonce_and_attended_context() {
    // Covers AE1: the precondition passes only when both gates hold.
    assert!(check_autonomy(&fresh_ctx()).is_ok(), "fresh nonce + TTY must pass");
}

#[test]
fn autonomy_refuses_ci_marker_even_with_valid_nonce() {
    // Covers AE1: CI marker present + valid nonce → refuses, places nothing.
    let mut ctx = fresh_ctx();
    ctx.unattended_marker = Some("CI env set".into());
    let err = check_autonomy(&ctx).expect_err("CI marker must refuse");
    assert!(err.contains("unattended context"), "msg: {err}");
}

#[test]
fn autonomy_refuses_no_tty_even_with_valid_nonce() {
    // No TTY detected + valid nonce → refuses.
    let mut ctx = fresh_ctx();
    ctx.unattended_marker = Some("no TTY on stdin".into());
    assert!(check_autonomy(&ctx).is_err(), "no TTY must refuse even with a valid nonce");
}

#[test]
fn autonomy_refuses_absent_nonce_in_attended_context() {
    // Nonce absent, no CI marker, TTY present → refuses (active human gate missing).
    let mut ctx = fresh_ctx();
    ctx.nonce = None;
    let err = check_autonomy(&ctx).expect_err("absent nonce must refuse");
    assert!(err.contains("nonce absent"), "msg: {err}");
}

#[test]
fn autonomy_refuses_expired_and_replayed_and_static_nonces() {
    let now = 1_700_000_000;
    // Expired-TTL nonce (minted > TTL ago) → refuses.
    assert!(validate_nonce(&(now - NONCE_TTL_SECS - 1).to_string(), now).is_err(), "expired");
    // A replayed nonce from a prior wave is just an old timestamp → expired → refuses.
    assert!(validate_nonce(&(now - 86_400).to_string(), now).is_err(), "day-old replay");
    // A static well-known constant (non-numeric) → refuses (env-var-present is not enough).
    assert!(validate_nonce("REPLAY", now).is_err(), "non-numeric constant");
    assert!(validate_nonce("yes", now).is_err(), "static constant");
    // A numeric static constant is a stale/implausible timestamp → refuses.
    assert!(validate_nonce("1", now).is_err(), "epoch-era constant is expired");
    assert!(validate_nonce("9999999999", now).is_err(), "far-future constant");
    // Empty → refuses.
    assert!(validate_nonce("   ", now).is_err(), "empty");
    // A fresh, in-window nonce passes (incl. small forward skew).
    assert!(validate_nonce(&now.to_string(), now).is_ok(), "fresh");
    assert!(validate_nonce(&(now + NONCE_MAX_SKEW_SECS).to_string(), now).is_ok(), "small skew ok");
    assert!(validate_nonce(&(now + NONCE_MAX_SKEW_SECS + 5).to_string(), now).is_err(), "over-skew");
}

// ---- U2: post-credential-load paper assertion (R2/AE2) ------------------

#[test]
fn resolved_non_paper_is_refused_even_when_env_var_says_paper() {
    // Covers AE2: a non-paper RESOLVED environment refuses regardless of the shell
    // LS_TRADING_ENV value (the resolved value is the enforceable invariant).
    assert!(
        assert_resolved_paper(&ls_core::Environment::Real).is_err(),
        "a resolved Real environment must be refused"
    );
    assert!(
        assert_resolved_paper(&ls_core::Environment::Paper).is_ok(),
        "a resolved Paper environment proceeds"
    );
}

// ---- U3: account-wide flat verdict (R3/R4, KTD2/KTD3) -------------------

/// A `t0425` row helper for the flat-verdict tests.
fn flat_row(ordno: &str, status: &str, cheqty: &str, ordrem: &str) -> T0425OutBlock1 {
    T0425OutBlock1 {
        ordno: ordno.into(),
        expcode: "005930".into(),
        status: status.into(),
        cheqty: cheqty.into(),
        ordrem: ordrem.into(),
        ..Default::default()
    }
}

#[test]
fn flat_verdict_genuinely_canceled_rows_are_flat() {
    // Zero rows → flat.
    assert_eq!(flat_verdict(&[]), FlatVerdict::Flat);
    // A genuinely canceled order (the chain's own teardown) releases its remainder
    // (ordrem==0, no fill), and a rejected SUBMIT never rested — both contribute
    // nothing to flatness.
    let rows = [
        flat_row("84005", "취소", "0", "0"),
        flat_row("84007", "거부", "0", "0"),
    ];
    assert_eq!(flat_verdict(&rows), FlatVerdict::Flat);
}

/// REGRESSION GUARD (review-flagged P0): a cancel-REJECTED (`취소거부`) or modify-
/// rejected (`정정거부`) order is STILL RESTING — its status text contains 취소/거부
/// but the order was NOT removed. flat_verdict must surface it via `ordrem > 0`, never
/// skip it as "terminal" (which would conclude flat while a real paper order rests —
/// the exact false-negative the account-flat assertion exists to prevent).
#[test]
fn flat_verdict_rejected_cancel_or_modify_with_remainder_is_resting_not_flat() {
    assert_eq!(
        flat_verdict(&[flat_row("84005", "취소거부", "0", "2")]),
        FlatVerdict::Resting(vec!["84005".into()]),
        "a cancel-rejected order still rests — must NOT read as flat"
    );
    assert_eq!(
        flat_verdict(&[flat_row("84006", "정정거부", "0", "3")]),
        FlatVerdict::Resting(vec!["84006".into()]),
        "a modify-rejected order still rests — must NOT read as flat"
    );
}

#[test]
fn flat_verdict_resting_remainder_is_not_flat() {
    // A 접수 row with an unfilled remainder is a cancelable resting order.
    let rows = [flat_row("84005", "접수", "0", "2")];
    assert_eq!(flat_verdict(&rows), FlatVerdict::Resting(vec!["84005".into()]));
}

#[test]
fn flat_verdict_account_wide_catches_leftover_other_symbol() {
    // A live resting row for a DIFFERENT symbol (a leftover from a prior aborted
    // run) still counts — the account-wide scan is the point (a per-intent reconcile
    // would have missed it).
    let mut leftover = flat_row("90001", "접수", "0", "5");
    leftover.expcode = "000660".into();
    assert_eq!(
        flat_verdict(&[leftover]),
        FlatVerdict::Resting(vec!["90001".into()])
    );
}

#[test]
fn flat_verdict_fill_outranks_resting() {
    // A fully-filled row (체결, cheqty>0, ordrem==0) → Fill.
    assert_eq!(
        flat_verdict(&[flat_row("84005", "체결", "1", "0")]),
        FlatVerdict::Fill(vec!["84005".into()])
    );
    // A PARTIAL fill (cheqty>0 AND ordrem>0) routes to Fill, not Resting — the fill
    // is unrecoverable even though a remainder rests.
    assert_eq!(
        flat_verdict(&[flat_row("84005", "체결", "1", "1")]),
        FlatVerdict::Fill(vec!["84005".into()])
    );
    // With BOTH a fill and a separate resting row present, Fill wins (terminal hard-fail).
    let rows = [
        flat_row("84005", "접수", "0", "2"), // resting
        flat_row("84006", "체결", "3", "0"), // fill
    ];
    assert_eq!(flat_verdict(&rows), FlatVerdict::Fill(vec!["84006".into()]));
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

// ---- U4: widened scrubbing + log suppression (R5/AE5, KTD4) -------------

#[test]
fn scrub_secrets_masks_account_with_suffix_and_tokens_keeps_order_numbers() {
    // The account number AND its `-NN` product suffix are masked as one token.
    assert_eq!(scrub_secrets("계좌 00000000-01 거부"), "계좌 *** 거부");
    assert!(!scrub_secrets("acct 12345678-99 done").contains("99"), "suffix must not leak");
    // A bearer-token / appkey-shaped string (20+ alnum) is masked.
    let token = "eyJhbGciOiJIUzI1Niabcdef012345";
    assert_eq!(scrub_secrets(&format!("Bearer {token}")), "Bearer ***");
    // A plain 6+ digit account run is masked (superset of scrub_digit_runs).
    assert_eq!(scrub_secrets("계좌 1234567890 거부"), "계좌 *** 거부");
    // Short numbers (qty/price) survive.
    assert_eq!(scrub_secrets("qty 12 price 100"), "qty 12 price 100");
    // Order numbers (<6 digits, no suffix) SURVIVE so a loud failure names the order.
    assert_eq!(scrub_secrets("resting ordno 84005 remains"), "resting ordno 84005 remains");
    // Korean status text is untouched.
    assert_eq!(scrub_secrets("정정거부"), "정정거부");
}

#[test]
fn loud_failure_message_is_account_free_but_names_the_order() {
    // A hard-fail built under a synthetic account-bearing rsp_msg leaks no account
    // digit run or suffix, yet still names the resting order number.
    let detail = "broker said 계좌 00000000-01 주문 12345678 거부 token=abcdefghij0123456789X";
    let msg = loud_failure("not-flat", &["84005".into()], detail);
    assert!(msg.contains("84005"), "the order must be named: {msg}");
    assert!(!msg.contains("00000000"), "account leaked: {msg}");
    assert!(!msg.contains("-01"), "account suffix leaked: {msg}");
    assert!(!msg.contains("12345678"), "an 8-digit run leaked: {msg}");
    assert!(!msg.contains("abcdefghij0123456789X"), "token leaked: {msg}");
}

#[test]
fn lserror_text_is_scrubbed_before_output() {
    // An emitted LsError carrying an account-bearing rsp_msg is account-free once
    // routed through scrub_secrets (KTD4: error text is untrusted broker text).
    let err = LsError::ApiError {
        code: "00123".into(),
        message: "계좌 98765432-01 거부".into(),
    };
    let scrubbed = scrub_secrets(&err.to_string());
    assert!(!scrubbed.contains("98765432"), "account leaked from LsError: {scrubbed}");
    assert!(!scrubbed.contains("-01"), "suffix leaked from LsError: {scrubbed}");
}

#[test]
fn dispatch_log_suppressor_refuses_when_a_subscriber_is_already_installed() {
    // Once ANY global default subscriber exists (set by this call or another test in
    // the binary), a second install MUST refuse — tracing allows one global default
    // per process, so a silent install-failure would be fail-OPEN on a known leak.
    //
    // This test permanently claims the process-global default for the order_smoke test
    // binary. That is harmless because (a) no other offline test relies on tracing and
    // (b) the live `order_chained_smoke` is run ALONE via `make live-smoke-order-chain`
    // (`cargo test -- --ignored --exact order_chained_smoke`), so the offline tests do
    // not co-run with it and its own install_dispatch_log_suppressor() succeeds first.
    let _ = install_dispatch_log_suppressor();
    assert!(
        install_dispatch_log_suppressor().is_err(),
        "a second install must fail closed (foreign subscriber already present)"
    );
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

/// Post-run account-flat assertion + best-effort cleanup (U3, R3/R4, KTD2/KTD3).
///
/// Runs the ACCOUNT-WIDE `t0425` scan and acts on the verdict:
/// - `Flat` → record a positively-confirmed clean pass.
/// - `Resting` → retry-cancel each still-resting order (while dispatch is enabled),
///   re-scan; if now flat record a pass with a cleanup note, else engage the
///   no-new-orders kill-switch and HARD-FAIL naming the order.
/// - `Fill` → engage the kill-switch and HARD-FAIL immediately (a fill cannot be
///   canceled; paper reset is the sole remediation).
/// - scan failure / truncation → NOT flat: engage the kill-switch and HARD-FAIL
///   (positive confirmation only — flat is never concluded from a failed read).
///
/// The kill-switch (`set_orders_enabled(false)`) is a "no new orders" guard engaged
/// on a wedged terminal run — it HALTS dispatch and is NOT a teardown (it cannot
/// remove a resting order); retry-cancel is the only removal path, and it runs
/// BEFORE the switch (KTD3). Every loud failure routes free text through
/// [`scrub_secrets`] and names order numbers verbatim.
async fn assert_account_flat(sdk: &LsSdk) {
    let rows = match scan_account_wide_working_orders(sdk).await {
        Ok(rows) => rows,
        Err(e) => {
            sdk.inner().set_orders_enabled(false);
            panic!("{}", loud_failure("flat-scan-failed", &[], &e));
        }
    };
    match flat_verdict(&rows) {
        FlatVerdict::Flat => {
            println!("ORDER-CHAIN flat=confirmed scan=account-wide note=[zero live rows]");
        }
        FlatVerdict::Fill(ordnos) => {
            sdk.inner().set_orders_enabled(false);
            panic!(
                "{}",
                loud_failure(
                    "unexpected-fill",
                    &ordnos,
                    "an order filled before teardown; a fill cannot be canceled — reset the paper book",
                )
            );
        }
        FlatVerdict::Resting(ordnos) => {
            println!(
                "ORDER-CHAIN flat=not-yet resting=[{}] action=retry-cancel",
                ordnos.join(",")
            );
            // Best-effort cleanup: retry-cancel each still-resting order while
            // dispatch is still enabled (the kill-switch would block these), THEN
            // re-scan. A resting row is `cheqty == 0 && ordrem > 0` (a partial fill is
            // a Fill, not retry-cancelable). Cancel qty is the remaining `ordrem`.
            for r in rows
                .iter()
                .filter(|r| parse_qty(&r.cheqty) == 0 && parse_qty(&r.ordrem) > 0)
            {
                let cancel =
                    CSPAT00801Request::new(r.ordno.trim(), r.expcode.trim(), r.ordrem.trim());
                match sdk.orders().cancel(&cancel).await {
                    Ok(_) => println!(
                        "ORDER-CHAIN retry-cancel ordno={} result=acked",
                        r.ordno.trim()
                    ),
                    Err(e) => println!(
                        "ORDER-CHAIN retry-cancel ordno={} result=[{}]",
                        r.ordno.trim(),
                        scrub_secrets(&e.to_string())
                    ),
                }
            }
            // Re-scan: positive confirmation only.
            let still = match scan_account_wide_working_orders(sdk).await {
                Ok(rows) => flat_verdict(&rows),
                Err(e) => {
                    sdk.inner().set_orders_enabled(false);
                    panic!("{}", loud_failure("flat-rescan-failed", &ordnos, &e));
                }
            };
            match still {
                FlatVerdict::Flat => println!(
                    "ORDER-CHAIN flat=confirmed-after-cleanup note=[retry-cancel cleared the book]"
                ),
                FlatVerdict::Fill(f) => {
                    sdk.inner().set_orders_enabled(false);
                    panic!(
                        "{}",
                        loud_failure(
                            "unexpected-fill",
                            &f,
                            "a resting order filled during cleanup; paper reset required",
                        )
                    );
                }
                FlatVerdict::Resting(s) => {
                    sdk.inner().set_orders_enabled(false);
                    panic!(
                        "{}",
                        loud_failure(
                            "still-resting",
                            &s,
                            "retry-cancel did not clear the order; it may remain resting — reset the paper book",
                        )
                    );
                }
            }
        }
    }
}

/// AUTONOMOUS chained paper-order run (U5): the agent invokes this directly during a
/// human-present wave — there is NO operator handoff. Submit a resting far-from-market
/// order (gate 1 evidence), modify it, then cancel it as teardown — each observed via
/// `t0425` — then assert the account is account-wide FLAT (U3). Cancel is the PRIMARY
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
/// - U3: after teardown, an account-wide `t0425` scan must positively confirm zero
///   live rows; a resting remainder triggers retry-cancel then a loud hard-fail, a
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
            sev.rsp_msg = format!("submit failed: {}", scrub_secrets(&e.to_string()));
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
            // unaffected (AE5). Do NOT return — fall through to the account-wide flat
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

    // U3 (R3/R4, KTD2/KTD3): assert the account is account-wide FLAT after the run.
    // This is the autonomous replacement for the operator's out-of-band paper reset —
    // a resting remainder is retry-canceled then hard-failed, a fill hard-fails
    // immediately, and a failed scan is treated as NOT flat.
    assert_account_flat(&sdk).await;

    println!(
        "ORDER-CHAIN teardown=cancel+flat-assert note=[cancel is the primary teardown; the \
         account-wide flat assertion confirms no order remains resting]"
    );
}
