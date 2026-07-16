//! Precondition checks + the deferral surface (U2, R1, R3, KTD5, AE1).
//!
//! A [`DispatchContext`] is gathered once (env, chain state, lock probe, spend ledger,
//! catalog checkpoint, gateway reads) and then each check is a **pure, offline-testable
//! function** returning a tiered outcome — the `check_autonomy`/`AutonomyContext` shape.
//! Non-deferrable reds (the trading-env interlock, kill-switch state, account flat-start,
//! rung authorization) abort with no override; deferrable reds abort unless an explicit,
//! named, nonce-authorized deferral overrides them (R3). A gateway throttle (IGW00201)
//! during any live-touching read is a **re-run**, never a terminal outcome (KTD5).
//!
//! Each check implementation incorporates its domain's documented false-reading mode: the
//! t0424 same-day-round-trip flat-start trap and the t0425 body-cursor truncation signal
//! (via the split `execution.rs` legs), the warm-IGW00201 throttle trap, and the
//! empty-store-with-current-watermark trap (a fresh watermark paired with a bar-presence
//! sample).

use chrono::{DateTime, Datelike, Timelike, Utc};
use chrono_tz::Asia::Seoul;

use nautilus_ls::error::AdapterResult;
use nautilus_ls::execution::LsExecClient;

use crate::artifacts::scrub;
use crate::dispatch::{CheckRecord, CheckStatus, Deferral, Tier};

/// Stable check names (used in records, deferral lists, and exceedance counts).
pub const CHECK_ADVISORY_LOCK: &str = "advisory_lock";
pub const CHECK_TRADING_ENV: &str = "trading_env_interlock";
pub const CHECK_SESSION_WINDOW: &str = "session_window";
pub const CHECK_WATERMARK: &str = "catalog_watermark";
pub const CHECK_FLAT_START: &str = "flat_start";
pub const CHECK_STRANDED: &str = "stranded_orders";
pub const CHECK_KILL_SWITCH: &str = "kill_switch";
pub const CHECK_BUDGET: &str = "budget_headroom";
pub const CHECK_RUNG_AUTH: &str = "rung_authorization";

/// A "is this a trading session" seam (Dependencies). Today's implementation is
/// weekday-only; a KRX holiday calendar upgrade drops in without redesign.
pub trait TradingCalendar {
    /// Whether `now` (UTC) falls inside an open KRX trading session.
    fn is_trading_session(&self, now: DateTime<Utc>) -> bool;
}

/// The weekday-only KRX session window (09:00–15:30 KST, Mon–Fri). Inherits today's
/// logic until the separate session-truth calendar lands (Dependencies) — a KRX holiday
/// passes this check until then.
#[derive(Debug, Clone, Copy, Default)]
pub struct WeekdayKrxCalendar;

impl TradingCalendar for WeekdayKrxCalendar {
    fn is_trading_session(&self, now: DateTime<Utc>) -> bool {
        let kst = now.with_timezone(&Seoul);
        let weekday = kst.weekday();
        if weekday == chrono::Weekday::Sat || weekday == chrono::Weekday::Sun {
            return false;
        }
        // 09:00–15:30 KST inclusive of the open, exclusive of the close minute.
        let minutes = kst.hour() * 60 + kst.minute();
        (9 * 60..=15 * 60 + 30).contains(&minutes)
    }
}

/// A gateway-read probe result (KTD5): a definite clear/blocked verdict, or a throttle
/// that means "re-run", never a terminal outcome.
#[derive(Debug, Clone)]
pub enum GatewayProbe {
    /// The read completed and the precondition is satisfied.
    Clear,
    /// The read completed and the precondition failed (scrubbed reason).
    Blocked(String),
    /// An IGW00201 throttle during the read — the gate re-runs; never terminal.
    Throttled,
}

/// Budget headroom (KTD5). `Unmeasured` (`budget_calls: None`) is a deferrable red named
/// "unmeasured", never a silent green.
#[derive(Debug, Clone, Copy)]
pub enum BudgetHeadroom {
    /// Measured spend vs the plan-ahead need in the rolling window.
    Measured {
        /// Calls remaining in the window.
        remaining: i64,
        /// The plan-ahead need for the session.
        plan: i64,
    },
    /// No measured budget — the plan-ahead model carries `budget_calls: None`.
    Unmeasured,
}

/// The rung-authorization posture: live-lane dispatches are gated non-deferrably;
/// paper-lane pre-checks report the rung informationally (paper sessions do not consume
/// rungs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanePosture {
    /// A live-lane dispatch — rung authorization is non-deferrable.
    Live,
    /// A paper-lane pre-check — rung authorization is informational.
    Paper,
}

/// The once-gathered decision inputs. Every field is plain data so each check is a pure
/// function; the only IO is the two gateway probes, gathered by [`probe_flat_start`] /
/// [`probe_stranded_orders`].
#[derive(Debug, Clone)]
pub struct DispatchContext {
    /// Wall-clock unix seconds (nonce TTL, budget window).
    pub now_unix: i64,
    /// The KST trading date this attempt keys on.
    pub today_kst: String,

    /// `LS_TRADING_ENV`, if set.
    pub trading_env: Option<String>,
    /// Whether the resolved lane env file is present.
    pub lane_env_present: bool,
    /// Whether the resolved credential env is paper (`Some(true)`), live (`Some(false)`),
    /// or unresolvable (`None`).
    pub resolved_env_is_paper: Option<bool>,

    /// Whether a live session currently holds the Live advisory lock.
    pub live_lock_held: bool,

    /// Whether the session window is open (calendar seam).
    pub window_open: bool,

    /// Whether the catalog watermark is fresh.
    pub watermark_fresh: bool,
    /// Whether the bar-presence sample found bars (an empty store with a current
    /// watermark still reds — the destructive-heal trap).
    pub bars_present: bool,

    /// The flat-start (t0424 holdings) probe — non-deferrable.
    pub flat_start: GatewayProbe,
    /// The stranded-orders (t0425 open orders) probe — deferrable.
    pub stranded_orders: GatewayProbe,

    /// Whether a persisted kill-switch trip is engaged (from the chain, KTD4).
    pub kill_switch_engaged: bool,
    /// Whether any kill-switch record exists (an absent store with no live history reads
    /// green-with-note).
    pub kill_switch_has_record: bool,

    /// Budget headroom.
    pub budget: BudgetHeadroom,

    /// The rung the chain currently authorizes.
    pub chain_authorized_rung: u8,
    /// The rung this dispatch requests (guard rail, R15).
    pub requested_rung: u8,
    /// The lane posture governing rung-auth tiering.
    pub lane: LanePosture,
}

/// One check's tiered outcome (pure). Converted to a [`CheckRecord`] with a `deferred`
/// flag by [`decide`].
#[derive(Debug, Clone)]
pub struct CheckOutcome {
    /// The check's stable name.
    pub name: &'static str,
    /// Its enforcement tier.
    pub tier: Tier,
    /// The status it resolved to.
    pub status: CheckStatus,
    /// Free-text detail (scrubbed at construction).
    pub detail: String,
}

impl CheckOutcome {
    fn new(name: &'static str, tier: Tier, status: CheckStatus, detail: impl Into<String>) -> Self {
        CheckOutcome { name, tier, status, detail: scrub(&detail.into()) }
    }
}

/// The advisory-lock check (deferrable): red if a live session holds the Live lock.
pub fn check_advisory_lock(ctx: &DispatchContext) -> CheckOutcome {
    if ctx.live_lock_held {
        CheckOutcome::new(
            CHECK_ADVISORY_LOCK,
            Tier::Deferrable,
            CheckStatus::Red,
            "a live session holds the Live advisory lock — refusing a second concurrent session",
        )
    } else {
        CheckOutcome::new(CHECK_ADVISORY_LOCK, Tier::Deferrable, CheckStatus::Green, "lock free")
    }
}

/// The trading-env interlock (non-deferrable): `LS_TRADING_ENV` set, the lane env file
/// present with no fallback, and the shell env matching the resolved credential env.
pub fn check_trading_env(ctx: &DispatchContext) -> CheckOutcome {
    let red = |d: &str| CheckOutcome::new(CHECK_TRADING_ENV, Tier::NonDeferrable, CheckStatus::Red, d);
    let env = match ctx.trading_env.as_deref() {
        Some(e) if !e.trim().is_empty() => e.trim(),
        _ => return red("LS_TRADING_ENV is unset — the trading-env interlock cannot resolve"),
    };
    if !ctx.lane_env_present {
        return red("lane env file missing (.env.<lane>) — no fallback lane");
    }
    match ctx.resolved_env_is_paper {
        None => red("cannot resolve lane credentials from the lane env file"),
        Some(resolved_paper) => {
            let shell_paper = env.eq_ignore_ascii_case("paper");
            if shell_paper != resolved_paper {
                red(&format!(
                    "resolved-env mismatch: shell LS_TRADING_ENV={env} but resolved credentials are \
                     {}",
                    if resolved_paper { "paper" } else { "live" }
                ))
            } else {
                CheckOutcome::new(
                    CHECK_TRADING_ENV,
                    Tier::NonDeferrable,
                    CheckStatus::Green,
                    format!("interlock ok (env={env})"),
                )
            }
        }
    }
}

/// The session-window check (deferrable): red outside the KRX session window / on a
/// weekend.
pub fn check_session_window(ctx: &DispatchContext) -> CheckOutcome {
    if ctx.window_open {
        CheckOutcome::new(CHECK_SESSION_WINDOW, Tier::Deferrable, CheckStatus::Green, "session window open")
    } else {
        CheckOutcome::new(
            CHECK_SESSION_WINDOW,
            Tier::Deferrable,
            CheckStatus::Red,
            "outside the KRX session window (weekday-only until the holiday calendar lands)",
        )
    }
}

/// The catalog-watermark check (deferrable): a stale watermark reds; a current watermark
/// over an empty bar sample also reds (a current watermark does not imply data presence).
pub fn check_watermark(ctx: &DispatchContext) -> CheckOutcome {
    if !ctx.watermark_fresh {
        CheckOutcome::new(CHECK_WATERMARK, Tier::Deferrable, CheckStatus::Red, "catalog watermark is stale")
    } else if !ctx.bars_present {
        CheckOutcome::new(
            CHECK_WATERMARK,
            Tier::Deferrable,
            CheckStatus::Red,
            "watermark is current but the bar-presence sample is empty (destructive-heal trap)",
        )
    } else {
        CheckOutcome::new(CHECK_WATERMARK, Tier::Deferrable, CheckStatus::Green, "watermark fresh, bars present")
    }
}

fn probe_outcome(name: &'static str, tier: Tier, probe: &GatewayProbe, clear_detail: &str) -> CheckOutcome {
    match probe {
        GatewayProbe::Clear => CheckOutcome::new(name, tier, CheckStatus::Green, clear_detail),
        GatewayProbe::Blocked(why) => CheckOutcome::new(name, tier, CheckStatus::Red, why.clone()),
        GatewayProbe::Throttled => CheckOutcome::new(
            name,
            tier,
            CheckStatus::Throttled,
            "IGW00201 throttle during the read — re-run (never a terminal outcome)",
        ),
    }
}

/// The flat-start (holdings) check (non-deferrable).
pub fn check_flat_start(ctx: &DispatchContext) -> CheckOutcome {
    probe_outcome(CHECK_FLAT_START, Tier::NonDeferrable, &ctx.flat_start, "account flat (no holdings)")
}

/// The stranded-orders check (deferrable).
pub fn check_stranded_orders(ctx: &DispatchContext) -> CheckOutcome {
    probe_outcome(CHECK_STRANDED, Tier::Deferrable, &ctx.stranded_orders, "no resting orders")
}

/// The kill-switch check (non-deferrable): a persisted engaged trip reds; an absent
/// store with no live history reads green-with-note.
pub fn check_kill_switch(ctx: &DispatchContext) -> CheckOutcome {
    if ctx.kill_switch_engaged {
        CheckOutcome::new(
            CHECK_KILL_SWITCH,
            Tier::NonDeferrable,
            CheckStatus::Red,
            "a persisted kill-switch trip is engaged — clear it explicitly (nonce + no-TTY gate)",
        )
    } else if !ctx.kill_switch_has_record {
        CheckOutcome::new(
            CHECK_KILL_SWITCH,
            Tier::NonDeferrable,
            CheckStatus::GreenWithNote,
            "no kill-switch record yet (no live-session history)",
        )
    } else {
        CheckOutcome::new(CHECK_KILL_SWITCH, Tier::NonDeferrable, CheckStatus::Green, "kill switch clear")
    }
}

/// The budget-headroom check (deferrable): measured shortfall reds; an unmeasured budget
/// is a deferrable red named "unmeasured", never a silent green.
pub fn check_budget(ctx: &DispatchContext) -> CheckOutcome {
    match ctx.budget {
        BudgetHeadroom::Measured { remaining, plan } if remaining >= plan => CheckOutcome::new(
            CHECK_BUDGET,
            Tier::Deferrable,
            CheckStatus::Green,
            format!("headroom ok ({remaining} remaining >= {plan} planned)"),
        ),
        BudgetHeadroom::Measured { remaining, plan } => CheckOutcome::new(
            CHECK_BUDGET,
            Tier::Deferrable,
            CheckStatus::Red,
            format!("measured headroom below plan ({remaining} remaining < {plan} planned)"),
        ),
        BudgetHeadroom::Unmeasured => CheckOutcome::new(
            CHECK_BUDGET,
            Tier::Deferrable,
            CheckStatus::Red,
            "unmeasured budget (budget_calls: None) — deferrable, never a silent green",
        ),
    }
}

/// The rung-authorization check (R15). Non-deferrable on a live lane; informational on a
/// paper pre-check (paper sessions do not consume rungs).
pub fn check_rung_authorization(ctx: &DispatchContext) -> CheckOutcome {
    let tier = match ctx.lane {
        LanePosture::Live => Tier::NonDeferrable,
        LanePosture::Paper => Tier::Informational,
    };
    if ctx.chain_authorized_rung == 0 {
        CheckOutcome::new(
            CHECK_RUNG_AUTH,
            tier,
            CheckStatus::Red,
            "the dispatch chain authorizes rung 0 (suspended) — no live session",
        )
    } else if ctx.requested_rung > ctx.chain_authorized_rung {
        CheckOutcome::new(
            CHECK_RUNG_AUTH,
            tier,
            CheckStatus::Red,
            format!(
                "requested rung {} exceeds the chain-authorized rung {} — rung selection is a guard \
                 rail, not an operator feature",
                ctx.requested_rung, ctx.chain_authorized_rung
            ),
        )
    } else {
        CheckOutcome::new(
            CHECK_RUNG_AUTH,
            tier,
            CheckStatus::Green,
            format!("rung {} authorized (chain rung {})", ctx.requested_rung, ctx.chain_authorized_rung),
        )
    }
}

/// Run every check over the gathered context, in a stable order.
pub fn run_checks(ctx: &DispatchContext) -> Vec<CheckOutcome> {
    vec![
        check_trading_env(ctx),
        check_advisory_lock(ctx),
        check_session_window(ctx),
        check_watermark(ctx),
        check_flat_start(ctx),
        check_stranded_orders(ctx),
        check_kill_switch(ctx),
        check_budget(ctx),
        check_rung_authorization(ctx),
    ]
}

/// The overall gate verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateResult {
    /// All checks green or explicitly deferred — a session may mount.
    Green,
    /// A non-deferrable red, or an undeferred deferrable red — no session.
    Refused,
    /// A live-touching check throttled — re-run; the attempt is NOT recorded as terminal.
    Throttled,
}

/// The gate decision: the verdict, the per-check records (with deferral flags), the
/// applied deferrals, and the red items that blocked.
#[derive(Debug, Clone)]
pub struct GateDecision {
    /// The verdict.
    pub result: GateResult,
    /// Per-check records for the chain.
    pub records: Vec<CheckRecord>,
    /// The deferrals that were applied.
    pub deferrals: Vec<Deferral>,
    /// The named red items that blocked (non-deferrable, or deferrable-but-undeferred).
    pub refused_items: Vec<String>,
}

/// Parse `LS_DISPATCH_DEFER` (comma-separated check names) into a deferral item list.
pub fn parse_deferrals(raw: Option<&str>) -> Vec<String> {
    raw.map(|s| s.split(',').map(|i| i.trim().to_string()).filter(|i| !i.is_empty()).collect())
        .unwrap_or_default()
}

/// Apply deferrals to the check outcomes and reach a verdict (R3, KTD5). A deferrable
/// red is overridden only when it is explicitly named in `deferrals` AND the operator
/// nonce authorized deferrals (`nonce_ok`). A non-deferrable red never yields to a
/// deferral. A throttle short-circuits to [`GateResult::Throttled`] — a re-run — so the
/// attempt is never written as a terminal record (KTD5).
pub fn decide(outcomes: &[CheckOutcome], deferrals: &[String], nonce_ok: bool) -> GateDecision {
    let mut records = Vec::with_capacity(outcomes.len());
    let mut applied = Vec::new();
    let mut refused_items = Vec::new();
    let mut throttled = false;

    for o in outcomes {
        let mut deferred = false;
        match o.status {
            CheckStatus::Throttled => throttled = true,
            CheckStatus::Green | CheckStatus::GreenWithNote => {}
            CheckStatus::Red => {
                let named = deferrals.iter().any(|d| d == o.name);
                if o.tier == Tier::Deferrable && named && nonce_ok {
                    deferred = true;
                    applied.push(Deferral {
                        item: o.name.to_string(),
                        reason: "operator deferral via LS_DISPATCH_DEFER".to_string(),
                    });
                } else if o.tier == Tier::Informational {
                    // Advisory — never blocks.
                } else {
                    refused_items.push(o.name.to_string());
                }
            }
        }
        records.push(CheckRecord {
            name: o.name.to_string(),
            tier: o.tier,
            status: o.status,
            detail: o.detail.clone(),
            deferred,
        });
    }

    let result = if throttled {
        GateResult::Throttled
    } else if refused_items.is_empty() {
        GateResult::Green
    } else {
        GateResult::Refused
    };
    GateDecision { result, records, deferrals: applied, refused_items }
}

/// Probe the flat-start (holdings) leg, classifying an IGW00201 throttle as a re-run
/// rather than a terminal red (KTD5).
pub async fn probe_flat_start(client: &LsExecClient) -> GatewayProbe {
    classify(client.check_flat_start().await)
}

/// Probe the stranded-orders leg, classifying an IGW00201 throttle as a re-run.
pub async fn probe_stranded_orders(client: &LsExecClient) -> GatewayProbe {
    classify(client.check_stranded_orders().await)
}

fn classify(r: AdapterResult<()>) -> GatewayProbe {
    match r {
        Ok(()) => GatewayProbe::Clear,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("IGW00201") {
                GatewayProbe::Throttled
            } else {
                GatewayProbe::Blocked(scrub(&msg))
            }
        }
    }
}
