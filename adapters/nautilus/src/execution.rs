//! The execution client — a nautilus [`ExecutionClient`] over the SDK order path
//! (U6).
//!
//! Order-event mapping is keyed on the [`ls_core::LsError`] variant (KTD6, see
//! [`crate::orders::map`]); order identity is chained across KRX modify's
//! new-order-number chaining ([`crate::orders::chain`]). At connect the client runs
//! a **flat-start gate** (R14): the account must have no open orders and no
//! holdings, else it refuses to start. Ambiguous/transport submit outcomes hold a
//! pending state and drive `Orders::reconcile`; `Unknown` stays pending and never
//! retries. The kill switch (`set_orders_enabled(false)`) is the halt hook and is
//! engaged only **after** any closing action.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use ls_sdk::account::T0424Request;
use ls_sdk::orders::{
    CSPAT00601Request, CSPAT00701Request, CSPAT00801Request, OrderIntent, ReconcileOutcome,
    T0425Request,
};
use ls_sdk::LsSdk;
use nautilus_common::clients::ExecutionClient;
use nautilus_common::messages::execution::{CancelOrder, ModifyOrder, SubmitOrder};
use nautilus_core::time::{get_atomic_clock_realtime, AtomicTime};
use nautilus_core::UnixNanos;
use nautilus_model::accounts::AccountAny;
use nautilus_model::enums::{AccountType, LiquiditySide, OmsType, OrderSide};
use nautilus_model::events::{OrderEventAny, OrderInitialized};
use nautilus_model::identifiers::{AccountId, ClientId, InstrumentId, Symbol, TraderId, Venue, VenueOrderId};
use nautilus_model::orders::{Order, OrderAny};
use nautilus_model::types::{AccountBalance, Currency, MarginBalance, Price, Quantity};
use nautilus_live::execution::emitter::ExecutionEventEmitter;
use tokio::sync::mpsc;

use crate::error::{AdapterError, AdapterResult};
use crate::orders::ledger::{FillDelta, FillLedger};
use crate::orders::map::{classify_reconcile, classify_submit_error, ReconcileEvent, SubmitAction};
use crate::orders::poll::{drive_poll_pass, poll_pacer, DrivenOutcome};
use crate::ws::rows::OrderEventMsg;
use crate::ws::supervisor::{RowKind, SubSpec, WsSupervisor};
use crate::KRX_VENUE;

/// Default cadence between t0425 poll passes (KTD2 — relaxed after SC certification).
/// This is the poll-authoritative steady state and the shipped default: the exec
/// client polls every 2s and the SC lane is a corroborating source.
pub const DEFAULT_POLL_CADENCE: Duration = Duration::from_secs(2);

/// The relaxed backstop cadence for **SC-primary** mode (KTD-3/KTD-4): once the live
/// probe certifies SC push-fills (U3) the operator flips SC to the primary fill source
/// and demotes the t0425 poll to a slow fail-closed reconcile backstop. SC frames carry
/// fills in real time; the poll only catches a *dropped/missed* SC frame.
///
/// **This value IS the worst-case time-to-detect a dropped SC fill (KTD-4):** the poll
/// loop consumes `reconcile_armed` and re-scans only *after* `sleep(cadence)` (no
/// event-driven wakeup, see [`run_poll_loop`]), so a missed fill stays invisible for at
/// most one cadence. It is therefore bounded below [`SC_FILL_DETECTION_CEILING`] — the
/// maximum stale-state window the bar strategy can tolerate — while still being an order
/// of magnitude slower than [`DEFAULT_POLL_CADENCE`] so it is a genuine backstop, not a
/// fill path (and stays clear of the t0425 2/s Account-bucket throttle, `IGW00201`).
pub const SC_PRIMARY_BACKSTOP_CADENCE: Duration = Duration::from_secs(15);

/// The maximum acceptable time-to-detect a dropped/missed SC fill (KTD-4). Because the
/// poll loop's steady-state cadence *is* the detection latency (it consumes the arm only
/// after its sleep), the SC-primary backstop cadence must not exceed this ceiling.
/// Chosen below one 1-minute bar so the bar-driven strategy can never act on stale
/// flat/position state for a whole bar. Enforced by an offline invariant test.
pub const SC_FILL_DETECTION_CEILING: Duration = Duration::from_secs(30);

/// Resolve the t0425 poll cadence from the off-by-default SC-primary selector (KTD-5).
///
/// - `false` (the shipped default) → [`DEFAULT_POLL_CADENCE`]: poll authoritative,
///   byte-identical to today. Shipping the mechanism is a no-op until an operator flips
///   the selector on (U6) under a certifying live verdict.
/// - `true` → [`SC_PRIMARY_BACKSTOP_CADENCE`]: SC carries fills, poll relaxes to the
///   fail-closed backstop. The poll loop is **never disabled** — only slowed — so it can
///   always reconcile a dropped SC frame within the KTD-4 detection ceiling.
///
/// Pure so the selector's off = no-op branch and the ceiling invariant are provable
/// offline, before the scarce attended open-KRX window (KTD-5).
pub fn resolve_poll_cadence(sc_primary_selected: bool) -> Duration {
    if sc_primary_selected {
        SC_PRIMARY_BACKSTOP_CADENCE
    } else {
        DEFAULT_POLL_CADENCE
    }
}

/// Shape a raw broker account number into a nautilus [`AccountId`] string.
///
/// Nautilus requires an `ISSUER-ID` form (at least one `-`), but LS paper account
/// numbers arrive with no issuer segment (the live gateway accepts the bare number —
/// U2's t0425 probe returned `rsp_cd=00000` with it). Passing the bare value straight
/// to `AccountId::from` panics (`did not contain '-'`), which blocked the first real
/// `node_exec_tester` run. Prefix a synthetic `LS` issuer when none is present; a value
/// that already carries a `-` (e.g. the mock `00000000-01`) passes through unchanged.
/// This only shapes nautilus's *internal* account identity — the gateway-facing account
/// number (`sdk.orders().account_no()`, used for `OrderIntent`) is never rewritten.
fn normalize_account_id(raw: &str) -> String {
    if raw.contains('-') {
        raw.to_string()
    } else {
        format!("LS-{raw}")
    }
}

/// How long a teardown waits for the in-flight order-dispatch tasks to drain before it
/// gives up on them and proceeds to the cancel scan (KTD2). Bounded so a wedged dispatch
/// can never stall the halt: an aborted task's order (if it dispatched at all) is caught
/// by the scan that follows, and an unconfirmed cancel fails the teardown closed.
pub const QUIESCE_BUDGET: Duration = Duration::from_secs(5);

/// Per-order cancel attempts [`cancel_all_resting_on`] makes before failing closed.
/// Small: the caller ([`crate::execution::cancel_all_resting_on`]'s teardown) is itself
/// retried by `run_teardown`, so a large inner budget only delays the halt.
const CANCEL_ALL_ATTEMPTS: usize = 3;

/// The shared t0425 pacer for the teardown flat/cancel scans. A **process-static** pacer
/// (not a per-call one) so repeated teardown scans — `run_teardown` retries the cancel up
/// to three times, and the watchdog may drive its own teardown — stay inside the t0425
/// Account-bucket cap instead of bursting into `IGW00201`
/// (`docs/solutions/integration-issues/ls-gateway-t0425-rate-limit-and-pagination-flat-scan.md`).
fn teardown_pacer() -> &'static crate::ingest::pacer::Pacer {
    static PACER: std::sync::OnceLock<crate::ingest::pacer::Pacer> = std::sync::OnceLock::new();
    PACER.get_or_init(poll_pacer)
}

/// The retained handles of the exec client's **detached order-dispatch tasks** (KTD2).
///
/// `submit_order`/`modify_order`/`cancel_order` spawn their worker and used to drop the
/// [`tokio::task::JoinHandle`], so a submission already in flight could reach
/// `sdk.orders().submit()` *after* a teardown's cancel scan and *before* `halt` — passing
/// the kill-switch check (checked first in `post_order`) and resting an order the scan
/// never saw, while `is_flat` raced ahead of the fill and finalized the run NORMAL. That
/// is a fail-open in a fail-closed system.
///
/// Retaining the handles here makes those tasks awaitable/abortable, so the teardown can
/// **quiesce** them before it enumerates: after [`quiesce`](Self::quiesce) returns, no
/// further submission can land at the gateway from a task that started before it.
/// Cheaply cloneable (`Arc`-shared) and `Send + Sync`, so the teardown handle can hold it.
#[derive(Debug, Clone, Default)]
pub struct OrderDispatchTasks(Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>);

/// What a [`OrderDispatchTasks::quiesce`] pass did — surfaced so a teardown can record
/// that it had to abort (rather than drain) an in-flight dispatch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QuiesceReport {
    /// Order-dispatch tasks that completed on their own within the budget.
    pub drained: usize,
    /// Order-dispatch tasks aborted after the budget elapsed (they may have already
    /// dispatched — the cancel scan that follows is what catches those).
    pub aborted: usize,
}

impl OrderDispatchTasks {
    /// A fresh, empty task set.
    pub fn new() -> Self {
        OrderDispatchTasks::default()
    }

    /// Retain a spawned order-dispatch task, first pruning the handles that already
    /// finished so a long session's set stays bounded.
    pub fn track(&self, handle: tokio::task::JoinHandle<()>) {
        let mut guard = self.0.lock().unwrap_or_else(|e| e.into_inner());
        guard.retain(|h| !h.is_finished());
        guard.push(handle);
    }

    /// Outstanding (not yet finished) order-dispatch tasks.
    pub fn pending(&self) -> usize {
        let guard = self.0.lock().unwrap_or_else(|e| e.into_inner());
        guard.iter().filter(|h| !h.is_finished()).count()
    }

    /// Drain every outstanding order-dispatch task, aborting any that outlives `budget`
    /// (KTD2). Takes the handles out of the set, so a second call is a no-op unless new
    /// dispatches were tracked in between. Never errors: a task that panicked is counted
    /// as drained — the point is only that it can no longer dispatch.
    pub async fn quiesce(&self, budget: Duration) -> QuiesceReport {
        let handles: Vec<tokio::task::JoinHandle<()>> = {
            let mut guard = self.0.lock().unwrap_or_else(|e| e.into_inner());
            std::mem::take(&mut *guard)
        };
        let mut report = QuiesceReport::default();
        let deadline = tokio::time::Instant::now() + budget;
        for handle in handles {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(remaining, handle).await {
                // Completed (or panicked) — either way it can no longer dispatch.
                Ok(_) => report.drained += 1,
                Err(_elapsed) => {
                    // `timeout` dropped the JoinHandle, which detaches rather than
                    // cancels; the task is already past its budget, so the following
                    // cancel scan is what catches anything it rested.
                    report.aborted += 1;
                }
            }
        }
        report
    }
}

/// The stranded-resting-order leg of the flat check over a bare SDK handle — the body
/// [`LsExecClient::check_stranded_orders`] delegates to, so the teardown handle (which
/// holds the shared [`LsSdk`], never a non-`Clone` [`LsExecClient`]) runs the identical
/// fail-closed check.
///
/// # Errors
///
/// [`AdapterError::Config`] if the inquiry is truncated or any resting order remains.
pub async fn check_stranded_orders_on(sdk: &LsSdk) -> AdapterResult<()> {
    // Open-order check: single-page t0425 (fail-closed on truncation).
    let orders = sdk.orders().inquiry(&T0425Request::for_symbol("")).await?;
    let next_cursor = orders.outblock.cts_ordno.trim();
    if !next_cursor.is_empty() {
        return Err(AdapterError::Config(
            "flat-start gate: order inquiry was truncated (more pages) — cannot prove the \
             account is flat; refusing to start"
                .to_string(),
        ));
    }
    // Fail CLOSED: a row is "open" if its unfilled remaining is > 0 OR its
    // `ordrem` is unparseable — the gate must never treat a garbage/unexpected
    // remaining-qty as "0 = filled" and slip a resting order through (R14).
    let open = orders
        .outblock1
        .iter()
        .filter(|r| r.ordrem.trim().parse::<i64>().map_or(true, |n| n > 0))
        .count();
    if open > 0 {
        return Err(AdapterError::Config(format!(
            "flat-start gate: {open} open (or unparseable-remaining) order(s) present — refusing \
             to start (v1 is flat-start-only, R14)"
        )));
    }
    Ok(())
}

/// The holdings/flat-start leg of the flat check over a bare SDK handle — the body
/// [`LsExecClient::check_flat_start`] delegates to.
///
/// # Errors
///
/// [`AdapterError::Config`] if any holding row carries an open (or unparseable) balance.
pub async fn check_flat_start_on(sdk: &LsSdk) -> AdapterResult<()> {
    // Holdings check: no t0424 row may carry an OPEN balance. A same-day buy+sell
    // round-trip leaves a lingering `janqty=0` row for the symbol (net-zero position,
    // still listed for the session) — that is NOT an open holding, so gating on a
    // bare `!is_empty()` false-fails "not flat". Mirror the order check above and
    // fail CLOSED: a row is open if its `janqty` parses > 0 OR is unparseable (never
    // treat a garbage balance as "0 = flat").
    let holdings = sdk
        .account()
        .stock_balance(&T0424Request::new("1", "0", "0", "0"))
        .await?;
    let open_holdings = holdings
        .outblock1
        .iter()
        .filter(|r| r.janqty.trim().parse::<i64>().map_or(true, |n| n > 0))
        .count();
    if open_holdings > 0 {
        return Err(AdapterError::Config(format!(
            "flat-start gate: {open_holdings} holding position(s) present — refusing to \
             start (R14)"
        )));
    }
    Ok(())
}

/// The composed flat check over a bare SDK handle (t0425 resting orders, then t0424
/// holdings) — the body [`LsExecClient::verify_flat`] delegates to, and the exact
/// positive-confirmation-only predicate the live teardown's `is_flat` reads through
/// `.is_ok()` (KTD1): a truncated/failed/ambiguous read returns `Err`, never a false
/// "flat".
///
/// # Errors
///
/// [`AdapterError::Config`] with a reason if not flat (AE5).
pub async fn verify_flat_on(sdk: &LsSdk) -> AdapterResult<()> {
    check_stranded_orders_on(sdk).await?;
    check_flat_start_on(sdk).await?;
    Ok(())
}

/// Cancel EVERY resting order on the account, returning the count actually canceled
/// (R2, KTD2). The primitive the fail-closed teardown needs and which had no
/// implementation anywhere — there is no cancel-all on the SDK.
///
/// Fail-closed by construction:
///
/// - a **single-page** t0425 `inquiry` (never `collect_all`, which can walk a
///   non-terminating `cts_ordno` on a polluted account); a truncated read (non-empty
///   `cts_ordno`) is an error, never a partial "flat";
/// - a row is resting if `ordrem` parses `> 0` **or** is unparseable — garbage is never
///   read as "0 = filled";
/// - each cancel is retried a small number of times and classified by the [`LsError`]
///   **variant** (`docs/solutions/conventions/order-error-classifier-placed-nothing-vs-may-rest.md`):
///   a clean `ApiError` business rejection (already filled / gone — the gateway placed
///   nothing and there is nothing resting) counts as not-resting and moves on; anything
///   that **may have rested or may still rest** (`AmbiguousOrder`/`Http`/`Decode`, and the
///   pre-network denials, which mean the cancel never reached the venue) exhausts the
///   retries and then returns `Err` — "not safe", so the teardown records the account as
///   not flat rather than concluding a false all-clear;
/// - it **never places a flattening order** (v1 is flat-start-only): halt-last stays safe
///   precisely because the teardown is read-or-cancel only
///   (`docs/solutions/conventions/kill-switch-ordering-in-order-placing-teardown.md`).
///
/// The read is paced against the t0425 Account-bucket cap. Order numbers reach the error
/// text through a structured, scrubbed value; broker free text (`LsError` Display carries
/// `rsp_msg`) is always scrubbed before it lands.
///
/// **Callers must quiesce the in-flight order-dispatch tasks first**
/// ([`OrderDispatchTasks::quiesce`]) — otherwise a submission already in flight can rest
/// an order *after* this scan (KTD2).
///
/// # Errors
///
/// [`AdapterError::Config`] on a truncated read or any un-acked cancel; the SDK error on
/// a failed inquiry.
pub async fn cancel_all_resting_on(sdk: &LsSdk) -> AdapterResult<usize> {
    let pacer = teardown_pacer();
    pacer.acquire().await;
    let orders = sdk.orders().inquiry(&T0425Request::for_symbol("")).await?;
    if !orders.outblock.cts_ordno.trim().is_empty() {
        return Err(AdapterError::Config(
            "cancel-all: the t0425 order inquiry was truncated (more pages) — cannot enumerate \
             every resting order; refusing to report the account cancel-clean"
                .to_string(),
        ));
    }
    // Fail CLOSED, exactly like the flat gate: a row rests if its unfilled remaining
    // parses > 0 OR is unparseable. A cancel sends the REMAINING quantity (R8); an
    // unparseable remaining falls back to the full order quantity, and to 1 when even
    // that is garbage — refusing to send a cancel is the one unacceptable failure mode.
    let resting: Vec<(String, String, i64)> = orders
        .outblock1
        .iter()
        .filter(|r| r.ordrem.trim().parse::<i64>().map_or(true, |n| n > 0))
        .map(|r| {
            let qty = r
                .ordrem
                .trim()
                .parse::<i64>()
                .ok()
                .filter(|n| *n > 0)
                .or_else(|| r.qty.trim().parse::<i64>().ok().filter(|n| *n > 0))
                .unwrap_or(1);
            (
                r.ordno.trim().to_string(),
                format!("A{}", r.expcode.trim()),
                qty,
            )
        })
        .collect();

    let mut canceled = 0usize;
    for (ordno, isuno, qty) in resting {
        let mut settled = false;
        let mut last_reason: Option<String> = None;
        for _ in 0..CANCEL_ALL_ATTEMPTS {
            pacer.acquire().await;
            let req = CSPAT00801Request::new(&ordno, &isuno, qty.to_string());
            match sdk.orders().cancel(&req).await {
                Ok(_) => {
                    canceled += 1;
                    settled = true;
                    break;
                }
                Err(err) => match classify_submit_error(&err) {
                    // A clean 2xx business rejection: the gateway placed nothing and the
                    // order is not resting (already filled / already gone). Not an error.
                    SubmitAction::Reject => {
                        settled = true;
                        break;
                    }
                    // A dedup reservation hit — an identical cancel is already in flight;
                    // retry so the outcome is observed rather than assumed.
                    SubmitAction::DropDuplicate => {
                        last_reason = Some("a concurrent identical cancel held the dedup reservation".to_string());
                    }
                    // May have rested / never reached the venue — either way this order is
                    // NOT proven canceled. Retry, then fail closed below.
                    SubmitAction::Pending | SubmitAction::Deny | SubmitAction::Accept => {
                        last_reason = Some(crate::scrub::scrub_secrets(&err.to_string()));
                    }
                },
            }
        }
        if !settled {
            let reason = last_reason.unwrap_or_else(|| "unknown cancel failure".to_string());
            return Err(AdapterError::Config(format!(
                "cancel-all: order {} could not be confirmed canceled after {CANCEL_ALL_ATTEMPTS} \
                 attempt(s) — treating the account as NOT flat (fail-closed): {reason}",
                crate::scrub::scrub_secrets(&ordno)
            )));
        }
    }
    Ok(canceled)
}

/// The LS domestic cash-equity execution client.
pub struct LsExecClient {
    client_id: ClientId,
    account_id: AccountId,
    venue: Venue,
    oms_type: OmsType,
    sdk: LsSdk,
    clock: &'static AtomicTime,
    emitter: ExecutionEventEmitter,
    connected: Arc<AtomicBool>,
    /// The single dual-source fill-emission seam (U1); owns the order chain.
    ledger: Arc<Mutex<FillLedger>>,
    /// Cadence between t0425 poll passes (U3).
    poll_cadence: Duration,
    /// The order-event (SC0/SC1) lane supervisor, spawned at connect (U2, KTD3).
    sc_supervisor: Option<WsSupervisor>,
    /// Background tasks (SC consumer + poll loop), aborted at disconnect.
    tasks: Vec<tokio::task::JoinHandle<()>>,
    /// Last observed order-lane WS drop count (AE6 reconcile trigger).
    last_drop_count: Arc<AtomicU64>,
    /// SC-lane unknown-fill trigger (KTD-7): set by the SC consumer, consumed by
    /// the poll loop — arms the reconcile drive on the next pass.
    reconcile_armed: Arc<AtomicBool>,
    /// Retained handles of the spawned submit/modify/cancel workers (KTD2) — the
    /// teardown drains them before its cancel scan so no submission lands after it.
    order_tasks: OrderDispatchTasks,
}

impl LsExecClient {
    /// Build an execution client with its own fresh [`FillLedger`].
    pub fn new(
        client_id: impl Into<String>,
        trader_id: impl Into<String>,
        account_id: impl Into<String>,
        sdk: LsSdk,
        account_type: AccountType,
    ) -> Self {
        Self::new_with_ledger(
            client_id,
            trader_id,
            account_id,
            sdk,
            account_type,
            Arc::new(Mutex::new(FillLedger::new())),
        )
    }

    /// Build an execution client over a **caller-supplied** [`FillLedger`] (KTD3).
    ///
    /// The ledger is a `sdk.clone()`-independent `Arc`: cloning the SDK shares the
    /// kill-switch `Arc<Inner>` but *not* the ledger, which `new` creates fresh inside.
    /// A live session's max-loss breaker must read the **node's** fills, so the runner
    /// builds one ledger, hands it here, and retains the same `Arc` for the feeder —
    /// otherwise the breaker reads an empty ledger and never trips (a silent no-op).
    pub fn new_with_ledger(
        client_id: impl Into<String>,
        trader_id: impl Into<String>,
        account_id: impl Into<String>,
        sdk: LsSdk,
        account_type: AccountType,
        ledger: Arc<Mutex<FillLedger>>,
    ) -> Self {
        let clock = get_atomic_clock_realtime();
        let trader_id = TraderId::from(trader_id.into().as_str());
        let account_id = AccountId::from(normalize_account_id(&account_id.into()).as_str());
        let emitter = ExecutionEventEmitter::new(
            clock,
            trader_id,
            account_id,
            account_type,
            Some(Currency::KRW()),
        );
        LsExecClient {
            client_id: ClientId::from(client_id.into().as_str()),
            account_id,
            venue: Venue::from(KRX_VENUE),
            oms_type: OmsType::Netting,
            sdk,
            clock,
            emitter,
            connected: Arc::new(AtomicBool::new(false)),
            ledger,
            poll_cadence: DEFAULT_POLL_CADENCE,
            sc_supervisor: None,
            tasks: Vec::new(),
            last_drop_count: Arc::new(AtomicU64::new(0)),
            reconcile_armed: Arc::new(AtomicBool::new(false)),
            order_tasks: OrderDispatchTasks::new(),
        }
    }

    /// A clone of this client's SDK handle — the same `Arc<Inner>`, so the kill switch
    /// engaged through it is the one gating **this** client's order dispatch (KTD3).
    pub fn sdk(&self) -> LsSdk {
        self.sdk.clone()
    }

    /// A clone of this client's shared fill-ledger `Arc` (KTD3) — what the live session's
    /// max-loss breaker feeder reads so it sees the node's real fills.
    pub fn ledger_handle(&self) -> Arc<Mutex<FillLedger>> {
        Arc::clone(&self.ledger)
    }

    /// A clone of this client's retained order-dispatch task set (KTD2) — what a teardown
    /// quiesces before its cancel scan.
    pub fn order_tasks(&self) -> OrderDispatchTasks {
        self.order_tasks.clone()
    }

    /// Override the t0425 poll cadence (KTD2 — the post-SC-certification primacy
    /// flip is a cadence relaxation, not a code-path change).
    pub fn with_poll_cadence(mut self, cadence: Duration) -> Self {
        self.poll_cadence = cadence;
        self
    }

    /// The R14 flat-start gate: the account must have **no open orders** and **no
    /// holdings**, and the order inquiry must not be truncated (fail-closed).
    ///
    /// Composes the two independently-callable legs
    /// ([`check_stranded_orders`](Self::check_stranded_orders) then
    /// [`check_flat_start`](Self::check_flat_start)) in the original order, so existing
    /// callers keep the exact single-verdict behavior. The dispatch gate calls the two
    /// legs directly instead: the composed function short-circuits on the open-orders
    /// leg, so one call cannot yield the two differently-tiered outcomes the gate needs
    /// — the stranded-orders leg is deferrable, the flat-start (holdings) leg is not
    /// (R3, AE1).
    ///
    /// # Errors
    ///
    /// [`AdapterError::Config`] with a reason if not flat (AE5).
    pub async fn verify_flat(&self) -> AdapterResult<()> {
        verify_flat_on(&self.sdk).await
    }

    /// Cancel every resting order on the account (R2, KTD2), returning the count canceled.
    ///
    /// Quiesces this client's retained order-dispatch tasks **first** so no submission
    /// already in flight can rest an order after the enumeration, then runs the
    /// [`cancel_all_resting_on`] primitive over the shared SDK.
    ///
    /// # Errors
    ///
    /// [`AdapterError::Config`] on a truncated read or any un-acked cancel.
    pub async fn cancel_all_resting(&self) -> AdapterResult<usize> {
        self.order_tasks.quiesce(QUIESCE_BUDGET).await;
        cancel_all_resting_on(&self.sdk).await
    }

    /// The stranded-resting-order leg of the flat check (single-page t0425), split out
    /// so the dispatch gate can tier it as **deferrable** (R3, AE1). Fail-closed on
    /// truncation and on an unparseable remaining quantity — identical semantics to the
    /// leg `verify_flat` used to inline.
    ///
    /// # Errors
    ///
    /// [`AdapterError::Config`] if the inquiry is truncated or any resting order remains.
    pub async fn check_stranded_orders(&self) -> AdapterResult<()> {
        check_stranded_orders_on(&self.sdk).await
    }

    /// The holdings/flat-start leg of the flat check (t0424), split out so the dispatch
    /// gate can tier it as **non-deferrable** (R3). Fail-closed on an unparseable
    /// balance — identical semantics to the leg `verify_flat` used to inline.
    ///
    /// # Errors
    ///
    /// [`AdapterError::Config`] if any holding row carries an open (or unparseable) balance.
    pub async fn check_flat_start(&self) -> AdapterResult<()> {
        check_flat_start_on(&self.sdk).await
    }

    /// Run `Orders::reconcile` for an intent (used on ambiguous submit + on a
    /// drop-count advance).
    pub async fn reconcile(&self, intent: &OrderIntent) -> ReconcileOutcome {
        self.sdk.orders().reconcile(intent, false).await
    }

    /// AE6: if the order-lane WS drop count advanced past the last seen value,
    /// treat fill accounting as suspect and drive an order-inquiry reconcile for
    /// `intent`. Returns the outcome if a reconcile ran.
    pub async fn on_drop_count(&self, count: u64, intent: &OrderIntent) -> Option<ReconcileOutcome> {
        let prev = self.last_drop_count.swap(count, Ordering::SeqCst);
        if count > prev {
            Some(self.reconcile(intent).await)
        } else {
            None
        }
    }

    /// Run a single DRIVEN t0425 poll pass and emit any derived fills (KTD5,
    /// KTD-7): an inconclusive pass re-polls with bounded backoff inside the
    /// shared primitive, so transient flakiness self-heals here too. A
    /// deterministic seam for tests + operator diagnostics; the live poll loop
    /// (U3) calls the same primitive on its cadence. Returns the driven outcome
    /// — the live runner records a reconcile condition only on `Exhausted`.
    pub async fn poll_once(&self) -> DrivenOutcome {
        let pacer = poll_pacer();
        let outcome = drive_poll_pass(&self.sdk, &self.ledger, &pacer).await;
        emit_fill_deltas(&self.ledger, &self.emitter, self.clock, &outcome.deltas);
        outcome
    }

    /// The kill-switch halt hook (`set_orders_enabled(false)`). Engage only AFTER
    /// any closing action, never before (a halt before a closing teardown defeats
    /// the close).
    pub fn halt(&self) {
        self.sdk.inner().set_orders_enabled(false);
    }

    /// Whether the order kill switch is currently armed (orders enabled).
    pub fn orders_enabled(&self) -> bool {
        self.sdk.inner().orders_enabled()
    }

    /// The lifetime count of order-dedup hits on this client's SDK (a within-TTL
    /// identical re-send served from cache, or a concurrent-duplicate rejection). A
    /// per-session safety metric the live runner persists into the run's data-quality
    /// report; a non-zero count on a real emission is a limit event (production-ladder
    /// R14(d)).
    pub fn dedup_hits(&self) -> u64 {
        self.sdk.inner().order_dedup.hit_count()
    }
}

/// The LS `BnsTpCode` for a nautilus order side. Returns `None` for anything but a
/// clean Buy/Sell — an ambiguous side must be **refused**, never defaulted to a
/// live sell (fail-closed).
fn side_code(order_side: OrderSide) -> Option<&'static str> {
    match order_side {
        OrderSide::Buy => Some("2"),
        OrderSide::Sell => Some("1"),
        _ => None,
    }
}

/// Build a domestic cash-equity **limit** submit request from a nautilus order, or
/// a deny reason. v1 supports Buy/Sell LIMIT orders only: an ambiguous side or a
/// market order (no price) is refused rather than silently sent as a limit-at-0 or
/// a wrong-side order.
fn submit_request(order_init: &OrderInitialized) -> Result<(CSPAT00601Request, &'static str), String> {
    let side = side_code(order_init.order_side)
        .ok_or_else(|| format!("unsupported order side {:?} (v1 accepts Buy/Sell only)", order_init.order_side))?;
    let price = order_init.price.ok_or_else(|| {
        "market orders are not supported in v1 (limit-only) — refusing rather than sending a \
         price-0 limit"
            .to_string()
    })?;
    let shcode = order_init.instrument_id.symbol.as_str();
    let isuno = format!("A{shcode}");
    let qty = order_init.quantity.as_f64() as i64;
    let price = price.as_f64() as i64;
    let req = CSPAT00601Request::limit(isuno, qty.to_string(), price.to_string(), side, "");
    Ok((req, side))
}

/// Build the reconcile intent for a submit (keyed for the t0425 query). `side` is
/// the already-validated `BnsTpCode`.
fn submit_intent(sdk: &LsSdk, order_init: &OrderInitialized, side: &str) -> OrderIntent {
    let shcode = order_init.instrument_id.symbol.as_str();
    let qty = (order_init.quantity.as_f64() as i64).to_string();
    let price = order_init.price.map(|p| (p.as_f64() as i64).to_string()).unwrap_or_default();
    OrderIntent::submit(
        sdk.orders().account_no().to_string(),
        shcode.to_string(),
        side.to_string(),
        qty,
        price,
        None,
    )
}

/// Lock the fill ledger, recovering from a poisoned mutex (a panic in one worker
/// task must not cascade to lose every subsequent order — fail-open on data loss
/// is the trap).
fn lock_ledger(ledger: &Mutex<FillLedger>) -> std::sync::MutexGuard<'_, FillLedger> {
    ledger.lock().unwrap_or_else(|e| e.into_inner())
}

/// Emit each fill delta through the ledger's retained emission context (the only
/// component alive to hold an `&OrderAny`, KTD1). Poll-derived fills carry no
/// execution price (KTD5), so they emit at the delta's `price` (the order limit).
/// Borrows the deltas (emission only reads them) and holds the ledger lock once
/// across the loop — the body never mutates the ledger.
fn emit_fill_deltas(
    ledger: &Mutex<FillLedger>,
    emitter: &ExecutionEventEmitter,
    clock: &'static AtomicTime,
    deltas: &[FillDelta],
) {
    let led = lock_ledger(ledger);
    for delta in deltas {
        if let Some(order) = led.order(&delta.client_order_id) {
            emitter.emit_order_filled(
                order,
                VenueOrderId::from(delta.ord_no.as_str()),
                None, // venue_position_id — netting OMS, no per-fill position id
                delta.trade_id,
                Quantity::from(delta.qty),
                Price::from(delta.price.to_string().as_str()),
                Currency::KRW(),
                None,                          // commission — not modeled on paper
                LiquiditySide::NoLiquiditySide, // poll cannot tell maker/taker (KTD5)
                clock.get_time_ns(),
            );
        }
    }
}

/// The SC0/SC1 order-event consumer (U2): fills feed the ledger (exactly-once via
/// KTD1); accepts cross-check the modify chain. Both lanes share the emit seam.
async fn run_sc_consumer(
    mut rx: mpsc::UnboundedReceiver<OrderEventMsg>,
    ledger: Arc<Mutex<FillLedger>>,
    emitter: ExecutionEventEmitter,
    clock: &'static AtomicTime,
    reconcile_armed: Arc<AtomicBool>,
) {
    while let Some(msg) = rx.recv().await {
        match msg {
            OrderEventMsg::Fill(obs) => {
                let outcome = lock_ledger(&ledger).apply(obs);
                if outcome.reconcile_needed {
                    // Arm the reconcile drive on the poll loop's next pass
                    // (KTD-7) — the SC lane itself never emits for an unknown
                    // order.
                    reconcile_armed.store(true, Ordering::SeqCst);
                    tracing::warn!(
                        "SC fill for an unknown order — no emission; the poll drive reconciles"
                    );
                }
                emit_fill_deltas(&ledger, &emitter, clock, &outcome.deltas);
            }
            OrderEventMsg::Accept { ord_no, org_ord_no } => {
                // A modify/cancel acceptance the REST ack may not have chained yet:
                // if the parent is known and the child isn't, chain it (KTD4 — REST
                // acks are authoritative; this is a belt-and-braces cross-check).
                let mut led = lock_ledger(&ledger);
                if !org_ord_no.is_empty()
                    && led.resolve(&ord_no).is_none()
                    && led.resolve(&org_ord_no).is_some()
                {
                    led.append_child(&org_ord_no, ord_no);
                }
            }
        }
    }
}

/// The authoritative poll loop (U3): while any order is open (or the SC lane
/// armed a reconcile), run a paced, DRIVEN t0425 pass and emit the derived
/// fills. The drive (KTD-7) self-heals transient inconclusiveness inside the
/// pass; only exhaustion is left standing for the live runner's report. Idles
/// when flat. Fills emit even when the SC lane is silent (AE3).
async fn run_poll_loop(
    sdk: LsSdk,
    ledger: Arc<Mutex<FillLedger>>,
    emitter: ExecutionEventEmitter,
    clock: &'static AtomicTime,
    cadence: Duration,
    reconcile_armed: Arc<AtomicBool>,
) {
    let pacer = poll_pacer();
    loop {
        let armed = reconcile_armed.swap(false, Ordering::SeqCst);
        // Run when the SC lane armed a wakeup, an order is open, OR a symbol is
        // pending a reconcile scan (U2, KTD2) — a flat ledger with a consumed arm
        // would otherwise sleep on pending symbols forever.
        let has_work = {
            let led = lock_ledger(&ledger);
            led.has_open_orders() || led.has_pending()
        };
        if armed || has_work {
            let outcome = drive_poll_pass(&sdk, &ledger, &pacer).await;
            if outcome.exhausted() {
                tracing::warn!("t0425 reconcile drive exhausted still inconclusive — reconcile advised");
            }
            emit_fill_deltas(&ledger, &emitter, clock, &outcome.deltas);
        }
        tokio::time::sleep(cadence).await;
    }
}

/// The spawned submit worker: run the Orders facade, classify per KTD6, register in
/// the ledger, and emit the matching nautilus event.
async fn run_submit(
    sdk: LsSdk,
    emitter: ExecutionEventEmitter,
    ledger: Arc<Mutex<FillLedger>>,
    clock: &'static AtomicTime,
    order_init: OrderInitialized,
) {
    let order: OrderAny =
        match OrderAny::from_events(vec![OrderEventAny::Initialized(order_init.clone())]) {
            Ok(o) => o,
            Err(e) => {
                tracing::error!(error = %e, "could not rebuild order from init event; not submitting");
                return;
            }
        };
    let client_order_id = order_init.client_order_id;
    // Build the request, refusing (denying) unsupported order shapes fail-closed
    // rather than sending a wrong-side or price-0 order.
    let (req, side) = match submit_request(&order_init) {
        Ok(pair) => pair,
        Err(reason) => {
            emitter.emit_order_denied(&order, &reason);
            return;
        }
    };

    match sdk.orders().submit(&req).await {
        Ok(resp) => {
            let ord_no = resp.order_no().to_string();
            lock_ledger(&ledger).register(order.clone(), ord_no.clone());
            emitter.emit_order_submitted(&order);
            emitter.emit_order_accepted(&order, VenueOrderId::from(ord_no.as_str()), clock.get_time_ns());
        }
        Err(err) => match classify_submit_error(&err) {
            // `Accept` is only produced for the `Ok` arm above; on the error path
            // fail closed to a denial rather than panicking in a detached task.
            SubmitAction::Accept => {
                tracing::error!("classify_submit_error returned Accept on an error — denying");
                emitter.emit_order_denied(&order, "internal: unexpected accept classification");
            }
            SubmitAction::Reject => {
                emitter.emit_order_rejected(&order, &err.to_string(), clock.get_time_ns(), false);
            }
            SubmitAction::Deny => {
                emitter.emit_order_denied(&order, &err.to_string());
            }
            SubmitAction::DropDuplicate => {
                tracing::info!("duplicate submit dropped (dedup reservation hit)");
            }
            SubmitAction::Pending => {
                // May have rested — reconcile before deciding (AE1).
                let intent = submit_intent(&sdk, &order_init, side);
                let outcome = sdk.orders().reconcile(&intent, false).await;
                match classify_reconcile(outcome) {
                    ReconcileEvent::Accepted => {
                        // The order rested at the venue but `ReconcileOutcome` does
                        // not carry the OrdNo, so we adopt a synthetic-but-UNIQUE
                        // venue id keyed on the client order id (never a shared
                        // constant that would collide across orders), and register
                        // it so a later SC/reconcile keyed on it resolves.
                        let venue_id = format!("RECON-{client_order_id}");
                        lock_ledger(&ledger).register(order.clone(), venue_id.clone());
                        emitter.emit_order_submitted(&order);
                        emitter.emit_order_accepted(
                            &order,
                            VenueOrderId::from(venue_id.as_str()),
                            clock.get_time_ns(),
                        );
                    }
                    ReconcileEvent::Rejected => {
                        emitter.emit_order_rejected(&order, "reconciled: rejected", clock.get_time_ns(), false);
                    }
                    ReconcileEvent::Canceled | ReconcileEvent::Modified => {
                        // Uncommon on a submit; leave to the periodic reconcile.
                        tracing::warn!("submit reconcile returned modified/canceled");
                    }
                    ReconcileEvent::StayPending => {
                        // Unknown: never retry; stay pending + alert.
                        tracing::error!(
                            "AMBIGUOUS submit could not be reconciled (Unknown) — order held \
                             pending, NOT retried"
                        );
                    }
                }
            }
        },
    }
}

/// The action a reconcile/rejection classification applies to (U4). Modify and
/// cancel emit **action-specific** rejections: a rejected cancel is
/// cancel-rejected (order stays open), never a canceled event (KTD6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OrderAction {
    Modify,
    Cancel,
}

/// Snapshot of a tracked order needed to build a modify/cancel request off the
/// spawned worker (cloned under the ledger lock so the async call holds no lock).
struct OrderSnapshot {
    order: OrderAny,
    latest_ord_no: String,
    symbol: String,
    side: OrderSide,
    qty: i64,
    price: i64,
    /// The unfilled remainder from the ledger's maintained accounting (R8): what
    /// a cancel sends. Falls back to `qty` when the ledger has no fill info —
    /// refusing to cancel is the one unacceptable failure mode.
    remaining: i64,
}

fn snapshot(ledger: &Mutex<FillLedger>, client_order_id: &nautilus_model::identifiers::ClientOrderId) -> Option<OrderSnapshot> {
    let led = lock_ledger(ledger);
    let order = led.order(client_order_id)?.clone();
    let latest_ord_no = led.latest_ord_no(client_order_id)?;
    // Read qty/price from the ledger's note_modify-maintained fields, NOT the
    // retained OrderAny (which is emission-identity only and is never rewritten
    // on a modify): a price-only re-modify (`cmd.quantity == None`) would
    // otherwise fall back to the ORIGINAL quantity and silently resurrect a
    // prior size reduction, increasing live exposure.
    let qty = led
        .order_qty(client_order_id)
        .unwrap_or_else(|| order.quantity().as_f64() as i64);
    Some(OrderSnapshot {
        symbol: order.instrument_id().symbol.as_str().to_string(),
        side: order.order_side(),
        price: led
            .limit_price(client_order_id)
            .unwrap_or_else(|| order.price().map(|p| p.as_f64() as i64).unwrap_or(0)),
        remaining: led.remaining_qty(client_order_id).unwrap_or(qty),
        qty,
        latest_ord_no,
        order,
    })
}

/// The spawned modify worker (U4): target the order's latest OrdNo, chain the new
/// OrdNo on ack, and classify errors action-aware (KTD6).
async fn run_modify(
    sdk: LsSdk,
    emitter: ExecutionEventEmitter,
    ledger: Arc<Mutex<FillLedger>>,
    clock: &'static AtomicTime,
    cmd: ModifyOrder,
) {
    let client_order_id = cmd.client_order_id;
    let Some(snap) = snapshot(&ledger, &client_order_id) else {
        // A modify of an unknown/never-accepted order is denied (never guessed).
        tracing::warn!(%client_order_id, "modify of an unknown order — denied");
        return;
    };
    let new_qty = cmd.quantity.map(|q| q.as_f64() as i64).unwrap_or(snap.qty);
    let new_price = cmd.price.map(|p| p.as_f64() as i64).unwrap_or(snap.price);
    let isuno = format!("A{}", snap.symbol);
    let req = CSPAT00701Request::limit(&snap.latest_ord_no, &isuno, new_qty.to_string(), new_price.to_string());

    match sdk.orders().modify(&req).await {
        Ok(resp) => {
            let new_ord_no = resp.order_no().to_string();
            {
                let mut led = lock_ledger(&ledger);
                led.append_child(&snap.latest_ord_no, new_ord_no.clone());
                led.note_modify(&client_order_id, new_qty, new_price);
            }
            emitter.emit_order_updated(
                &snap.order,
                VenueOrderId::from(new_ord_no.as_str()),
                Quantity::from(new_qty),
                Some(Price::from(new_price.to_string().as_str())),
                None,
                None,
                clock.get_time_ns(),
            );
        }
        Err(err) => {
            handle_action_error(OrderAction::Modify, &sdk, &emitter, &ledger, clock, &snap, new_qty, new_price, &err).await;
        }
    }
}

/// The spawned cancel worker (U4): target the order's latest OrdNo with the
/// ledger-derived REMAINING quantity (R8 — never the original order quantity);
/// on ack emit OrderCanceled + forget the chain; a business rejection emits
/// cancel-rejected (order stays open, KTD6).
async fn run_cancel(
    sdk: LsSdk,
    emitter: ExecutionEventEmitter,
    ledger: Arc<Mutex<FillLedger>>,
    clock: &'static AtomicTime,
    cmd: CancelOrder,
) {
    let client_order_id = cmd.client_order_id;
    let Some(snap) = snapshot(&ledger, &client_order_id) else {
        // The ledger never tracked this order — the adapter sent no cancel, so a
        // truthful non-terminal cancel-rejection returns nautilus-core's FSM from
        // PENDING_CANCEL (never a synthetic terminal event, KTD4). We have no
        // retained `OrderAny`, so emit from the command's ids, and arm a reconcile
        // for the command's instrument so venue truth is re-verified (R4).
        tracing::warn!(%client_order_id, "cancel of an unknown order — nothing resting; emitting cancel-rejection + arming reconcile");
        // Arm the reconcile before emitting so an observer that keys off the event
        // sees the pending symbol already recorded. The command's instrument symbol
        // is already the bare short code (no `A` prefix to invert, unlike the SC
        // issue-code seam), so record it directly — `record_pending_symbol` trims
        // and drops a blank (the KTD3 empty-symbol guard).
        lock_ledger(&ledger).record_pending_symbol(cmd.instrument_id.symbol.as_str());
        emitter.emit_order_cancel_rejected_event(
            cmd.strategy_id,
            cmd.instrument_id,
            client_order_id,
            None,
            "cancel skipped: order unknown to the ledger (nothing resting)",
            clock.get_time_ns(),
        );
        return;
    };
    if snap.remaining == 0 {
        // Fully filled (or modified below the filled total) just before the cancel:
        // nothing rests, so skip the send. A synthetic terminal event could mask a
        // still-resting order (inverted-cancel risk), so emit a truthful
        // non-terminal cancel-rejection instead — it returns the FSM from
        // PENDING_CANCEL (the `handle_action_error` pattern) — then close the ledger
        // entry (its venue-done state follows from the acked modify plus observed
        // fills, reusing the terminal condition) so the open set clears and the poll
        // loop can idle, and arm a reconcile for the symbol (R4, KTD4).
        tracing::info!(%client_order_id, "cancel skipped: remaining quantity is 0 (nothing resting); emitting cancel-rejection + closing ledger entry");
        // Close the venue-done entry + arm the reconcile before emitting, so an
        // observer that keys off the event sees a flat open set and the pending
        // symbol already recorded.
        {
            let mut led = lock_ledger(&ledger);
            led.close(&client_order_id);
            led.record_pending_symbol(&snap.symbol);
        }
        emitter.emit_order_cancel_rejected(
            &snap.order,
            Some(VenueOrderId::from(snap.latest_ord_no.as_str())),
            "cancel skipped: remaining quantity is 0 (nothing resting)",
            clock.get_time_ns(),
        );
        return;
    }
    let isuno = format!("A{}", snap.symbol);
    let req = CSPAT00801Request::new(&snap.latest_ord_no, &isuno, snap.remaining.to_string());

    match sdk.orders().cancel(&req).await {
        Ok(resp) => {
            let new_ord_no = resp.order_no().to_string();
            lock_ledger(&ledger).close(&client_order_id);
            emitter.emit_order_canceled(
                &snap.order,
                Some(VenueOrderId::from(new_ord_no.as_str())),
                clock.get_time_ns(),
            );
        }
        Err(err) => {
            // The ambiguous-action reconcile intent carries the same remaining
            // quantity the cancel sent.
            handle_action_error(OrderAction::Cancel, &sdk, &emitter, &ledger, clock, &snap, snap.remaining, snap.price, &err).await;
        }
    }
}

/// Classify a modify/cancel error per KTD6 and emit the action-appropriate event.
/// A clean rejection emits modify/cancel-rejected (order stays open); an ambiguous
/// outcome holds pending and drives an action-aware reconcile keyed on the ORIGINAL
/// OrdNo; `Unknown` never authorizes a retry.
#[allow(clippy::too_many_arguments)]
async fn handle_action_error(
    action: OrderAction,
    sdk: &LsSdk,
    emitter: &ExecutionEventEmitter,
    ledger: &Arc<Mutex<FillLedger>>,
    clock: &'static AtomicTime,
    snap: &OrderSnapshot,
    qty: i64,
    price: i64,
    err: &ls_core::LsError,
) {
    let venue = Some(VenueOrderId::from(snap.latest_ord_no.as_str()));
    let reject = |reason: &str| match action {
        OrderAction::Modify => {
            emitter.emit_order_modify_rejected(&snap.order, venue, reason, clock.get_time_ns())
        }
        OrderAction::Cancel => {
            emitter.emit_order_cancel_rejected(&snap.order, venue, reason, clock.get_time_ns())
        }
    };

    match classify_submit_error(err) {
        // A clean business rejection or a pre-network denial leaves the order
        // resting: emit the action-specific rejection, never a canceled/updated.
        SubmitAction::Reject | SubmitAction::Deny => reject(&err.to_string()),
        SubmitAction::DropDuplicate => {
            tracing::info!("duplicate modify/cancel dropped (dedup reservation hit)");
        }
        SubmitAction::Accept => {
            tracing::error!("classify returned Accept on an error path — rejecting the action");
            reject("internal: unexpected accept classification");
        }
        SubmitAction::Pending => {
            // May have taken effect — reconcile with the ORIGINAL OrdNo (action-aware).
            let side = side_code(snap.side).unwrap_or("2");
            let account = sdk.orders().account_no().to_string();
            let intent = match action {
                OrderAction::Modify => OrderIntent::modify(
                    account, snap.symbol.clone(), side.to_string(), qty.to_string(), price.to_string(), snap.latest_ord_no.clone(),
                ),
                OrderAction::Cancel => OrderIntent::cancel(
                    account, snap.symbol.clone(), side.to_string(), qty.to_string(), snap.latest_ord_no.clone(),
                ),
            };
            let outcome = sdk.orders().reconcile(&intent, false).await;
            match (action, classify_reconcile(outcome)) {
                // Cancel confirmed, or a modify that reconciled as canceled → the
                // order is gone: emit canceled + forget.
                (_, ReconcileEvent::Canceled) => {
                    lock_ledger(ledger).close(&snap.order.client_order_id());
                    emitter.emit_order_canceled(&snap.order, venue, clock.get_time_ns());
                }
                (OrderAction::Cancel, ReconcileEvent::Rejected) => reject("reconciled: cancel rejected"),
                (OrderAction::Modify, ReconcileEvent::Rejected) => reject("reconciled: modify rejected"),
                // Modify confirmed but the reconcile carries no new OrdNo — the order
                // is live under a number we cannot chain; hold rather than emit a
                // wrong id. The poll lane keeps accounting on the known OrdNos.
                (OrderAction::Modify, ReconcileEvent::Modified | ReconcileEvent::Accepted) => {
                    tracing::warn!("modify reconciled as applied but the new OrdNo is unknown — held, not chained");
                }
                // Cancel did not take (still resting/modified) → leave open.
                (OrderAction::Cancel, ReconcileEvent::Accepted | ReconcileEvent::Modified) => {
                    tracing::warn!("cancel reconciled as still-open — order remains resting");
                }
                // Unknown → never retry; stay pending + alert (KTD6).
                (_, ReconcileEvent::StayPending) => {
                    tracing::error!("AMBIGUOUS {action:?} could not be reconciled (Unknown) — held pending, NOT retried");
                }
            }
        }
    }
}

#[async_trait(?Send)]
impl ExecutionClient for LsExecClient {
    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn client_id(&self) -> ClientId {
        self.client_id
    }

    fn account_id(&self) -> AccountId {
        self.account_id
    }

    fn venue(&self) -> Venue {
        self.venue
    }

    fn oms_type(&self) -> OmsType {
        self.oms_type
    }

    fn get_account(&self) -> Option<AccountAny> {
        None // v1 does not materialize a nautilus account object
    }

    fn generate_account_state(
        &self,
        balances: Vec<AccountBalance>,
        margins: Vec<MarginBalance>,
        reported: bool,
        ts_event: UnixNanos,
    ) -> anyhow::Result<()> {
        self.emitter.emit_account_state(balances, margins, reported, ts_event);
        Ok(())
    }

    fn start(&mut self) -> anyhow::Result<()> {
        // Capture the runner's execution-event sender (panics outside an
        // initialized runner — the live node initializes it before start()).
        self.emitter
            .set_sender(nautilus_common::live::runner::get_exec_event_sender());
        Ok(())
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn connect(&mut self) -> anyhow::Result<()> {
        // R14 flat-start gate: refuse to start unless the account is flat.
        self.verify_flat().await.map_err(|e| anyhow::anyhow!("{e}"))?;

        // Spawn the SC0/SC1 order-event lane over the exec client's OWN supervisor
        // instance / WsManager (KTD3 — isolated failure domain from market data).
        let (sc_tx, sc_rx) = mpsc::unbounded_channel::<OrderEventMsg>();
        let sc_sup = WsSupervisor::spawn_order_events(self.sdk.clone(), sc_tx);
        let placeholder = InstrumentId::new(Symbol::from("SC"), self.venue);
        for tr_cd in ["SC0", "SC1"] {
            let kind = if tr_cd == "SC0" { RowKind::OrderAccept } else { RowKind::OrderFill };
            sc_sup.subscribe(SubSpec {
                tr_cd: tr_cd.to_string(),
                tr_key: String::new(), // account-wide
                instrument_id: placeholder,
                kind,
            });
        }

        // The SC consumer + the authoritative poll loop both feed the one ledger.
        let consumer = tokio::spawn(run_sc_consumer(
            sc_rx,
            Arc::clone(&self.ledger),
            self.emitter.clone(),
            self.clock,
            Arc::clone(&self.reconcile_armed),
        ));
        let poll = tokio::spawn(run_poll_loop(
            self.sdk.clone(),
            Arc::clone(&self.ledger),
            self.emitter.clone(),
            self.clock,
            self.poll_cadence,
            Arc::clone(&self.reconcile_armed),
        ));

        self.sc_supervisor = Some(sc_sup);
        self.tasks = vec![consumer, poll];
        self.connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn disconnect(&mut self) -> anyhow::Result<()> {
        if let Some(sup) = self.sc_supervisor.take() {
            sup.shutdown();
        }
        for task in self.tasks.drain(..) {
            task.abort();
        }
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn submit_order(&self, cmd: SubmitOrder) -> anyhow::Result<()> {
        let sdk = self.sdk.clone();
        let emitter = self.emitter.clone();
        let ledger = Arc::clone(&self.ledger);
        let clock = self.clock;
        // Retain the handle (KTD2) so a teardown can quiesce this dispatch before its
        // cancel scan — a dropped handle would let the submit land after the scan.
        self.order_tasks
            .track(tokio::spawn(run_submit(sdk, emitter, ledger, clock, cmd.order_init)));
        Ok(())
    }

    fn modify_order(&self, cmd: ModifyOrder) -> anyhow::Result<()> {
        let sdk = self.sdk.clone();
        let emitter = self.emitter.clone();
        let ledger = Arc::clone(&self.ledger);
        let clock = self.clock;
        self.order_tasks
            .track(tokio::spawn(run_modify(sdk, emitter, ledger, clock, cmd)));
        Ok(())
    }

    fn cancel_order(&self, cmd: CancelOrder) -> anyhow::Result<()> {
        let sdk = self.sdk.clone();
        let emitter = self.emitter.clone();
        let ledger = Arc::clone(&self.ledger);
        let clock = self.clock;
        self.order_tasks
            .track(tokio::spawn(run_cancel(sdk, emitter, ledger, clock, cmd)));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_code_maps_buy_sell_and_rejects_ambiguous() {
        assert_eq!(side_code(OrderSide::Buy), Some("2"));
        assert_eq!(side_code(OrderSide::Sell), Some("1"));
        // An ambiguous/no side must be refused (None) — never defaulted to a live
        // SELL. This is the fail-closed guard `submit_request` relies on.
        assert_eq!(side_code(OrderSide::NoOrderSide), None);
    }

    #[test]
    fn normalize_account_id_adds_a_synthetic_issuer_only_when_absent() {
        // A bare LS paper account number (no issuer segment) gets the `LS-` prefix so
        // nautilus `AccountId::from` (which requires a `-`) does not panic — the defect
        // the first live `node_exec_tester` run surfaced.
        assert_eq!(normalize_account_id("1234567890"), "LS-1234567890");
        // An already-issuer-qualified value (e.g. the mock) is untouched — no double prefix.
        assert_eq!(normalize_account_id("00000000-01"), "00000000-01");
        // The result is a valid nautilus AccountId (constructing it must not panic).
        let _ = AccountId::from(normalize_account_id("1234567890").as_str());
    }
}
