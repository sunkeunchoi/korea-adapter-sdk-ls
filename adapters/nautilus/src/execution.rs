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
use crate::orders::poll::{poll_open_orders, poll_pacer};
use crate::ws::rows::OrderEventMsg;
use crate::ws::supervisor::{RowKind, SubSpec, WsSupervisor};
use crate::KRX_VENUE;

/// Default cadence between t0425 poll passes (KTD2 — relaxed after SC certification).
const DEFAULT_POLL_CADENCE: Duration = Duration::from_secs(2);

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
}

impl LsExecClient {
    /// Build an execution client.
    pub fn new(
        client_id: impl Into<String>,
        trader_id: impl Into<String>,
        account_id: impl Into<String>,
        sdk: LsSdk,
        account_type: AccountType,
    ) -> Self {
        let clock = get_atomic_clock_realtime();
        let trader_id = TraderId::from(trader_id.into().as_str());
        let account_id = AccountId::from(account_id.into().as_str());
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
            ledger: Arc::new(Mutex::new(FillLedger::new())),
            poll_cadence: DEFAULT_POLL_CADENCE,
            sc_supervisor: None,
            tasks: Vec::new(),
            last_drop_count: Arc::new(AtomicU64::new(0)),
        }
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
    /// # Errors
    ///
    /// [`AdapterError::Config`] with a reason if not flat (AE5).
    pub async fn verify_flat(&self) -> AdapterResult<()> {
        // Open-order check: single-page t0425 (fail-closed on truncation).
        let orders = self
            .sdk
            .orders()
            .inquiry(&T0425Request::for_symbol(""))
            .await?;
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
        let open: Vec<&str> = orders
            .outblock1
            .iter()
            .filter(|r| {
                r.ordrem.trim().parse::<i64>().map_or(true, |n| n > 0)
            })
            .map(|r| r.ordno.trim())
            .collect();
        if !open.is_empty() {
            return Err(AdapterError::Config(format!(
                "flat-start gate: {} open (or unparseable-remaining) order(s) present — refusing \
                 to start (v1 is flat-start-only, R14)",
                open.len()
            )));
        }

        // Holdings check: t0424 per-holding array must be empty.
        let holdings = self
            .sdk
            .account()
            .stock_balance(&T0424Request::new("1", "0", "0", "0"))
            .await?;
        if !holdings.outblock1.is_empty() {
            return Err(AdapterError::Config(format!(
                "flat-start gate: {} holding position(s) present — refusing to start (R14)",
                holdings.outblock1.len()
            )));
        }
        Ok(())
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

    /// Run a single t0425 poll pass and emit any derived fills (KTD5). A
    /// deterministic seam for tests + operator diagnostics; the live poll loop (U3)
    /// calls the same primitives on its cadence. Returns the pass outcome.
    pub async fn poll_once(&self) -> crate::orders::poll::PollOutcome {
        let pacer = poll_pacer();
        let outcome = poll_open_orders(&self.sdk, &self.ledger, &pacer).await;
        emit_fill_deltas(&self.ledger, &self.emitter, self.clock, outcome.deltas.clone());
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
fn emit_fill_deltas(
    ledger: &Mutex<FillLedger>,
    emitter: &ExecutionEventEmitter,
    clock: &'static AtomicTime,
    deltas: Vec<FillDelta>,
) {
    for delta in deltas {
        let led = lock_ledger(ledger);
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
) {
    while let Some(msg) = rx.recv().await {
        match msg {
            OrderEventMsg::Fill(obs) => {
                let outcome = lock_ledger(&ledger).apply(obs);
                if outcome.reconcile_needed {
                    tracing::warn!(
                        "SC fill for an unknown order — no emission; the poll lane reconciles"
                    );
                }
                emit_fill_deltas(&ledger, &emitter, clock, outcome.deltas);
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

/// The authoritative poll loop (U3): while any order is open, run a paced t0425 pass
/// and emit the derived fills. Idles when flat. Fills emit even when the SC lane is
/// silent (AE3).
async fn run_poll_loop(
    sdk: LsSdk,
    ledger: Arc<Mutex<FillLedger>>,
    emitter: ExecutionEventEmitter,
    clock: &'static AtomicTime,
    cadence: Duration,
) {
    let pacer = poll_pacer();
    loop {
        if lock_ledger(&ledger).has_open_orders() {
            let outcome = poll_open_orders(&sdk, &ledger, &pacer).await;
            if outcome.reconcile_needed {
                tracing::warn!("t0425 poll pass was inconclusive (truncation/unresolved) — reconcile advised");
            }
            emit_fill_deltas(&ledger, &emitter, clock, outcome.deltas);
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
}

fn snapshot(ledger: &Mutex<FillLedger>, client_order_id: &nautilus_model::identifiers::ClientOrderId) -> Option<OrderSnapshot> {
    let led = lock_ledger(ledger);
    let order = led.order(client_order_id)?.clone();
    let latest_ord_no = led.latest_ord_no(client_order_id)?;
    Some(OrderSnapshot {
        symbol: order.instrument_id().symbol.as_str().to_string(),
        side: order.order_side(),
        // Read qty/price from the ledger's note_modify-maintained fields, NOT the
        // retained OrderAny (which is emission-identity only and is never rewritten
        // on a modify): a price-only re-modify (`cmd.quantity == None`) would
        // otherwise fall back to the ORIGINAL quantity and silently resurrect a
        // prior size reduction, increasing live exposure.
        qty: led.order_qty(client_order_id).unwrap_or_else(|| order.quantity().as_f64() as i64),
        price: led
            .limit_price(client_order_id)
            .unwrap_or_else(|| order.price().map(|p| p.as_f64() as i64).unwrap_or(0)),
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

/// The spawned cancel worker (U4): target the order's latest OrdNo; on ack emit
/// OrderCanceled + forget the chain; a business rejection emits cancel-rejected
/// (order stays open, KTD6).
async fn run_cancel(
    sdk: LsSdk,
    emitter: ExecutionEventEmitter,
    ledger: Arc<Mutex<FillLedger>>,
    clock: &'static AtomicTime,
    cmd: CancelOrder,
) {
    let client_order_id = cmd.client_order_id;
    let Some(snap) = snapshot(&ledger, &client_order_id) else {
        tracing::warn!(%client_order_id, "cancel of an unknown order — ignored (nothing resting)");
        return;
    };
    let isuno = format!("A{}", snap.symbol);
    let req = CSPAT00801Request::new(&snap.latest_ord_no, &isuno, snap.qty.to_string());

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
            handle_action_error(OrderAction::Cancel, &sdk, &emitter, &ledger, clock, &snap, snap.qty, snap.price, &err).await;
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
        ));
        let poll = tokio::spawn(run_poll_loop(
            self.sdk.clone(),
            Arc::clone(&self.ledger),
            self.emitter.clone(),
            self.clock,
            self.poll_cadence,
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
        tokio::spawn(run_submit(sdk, emitter, ledger, clock, cmd.order_init));
        Ok(())
    }

    fn modify_order(&self, cmd: ModifyOrder) -> anyhow::Result<()> {
        let sdk = self.sdk.clone();
        let emitter = self.emitter.clone();
        let ledger = Arc::clone(&self.ledger);
        let clock = self.clock;
        tokio::spawn(run_modify(sdk, emitter, ledger, clock, cmd));
        Ok(())
    }

    fn cancel_order(&self, cmd: CancelOrder) -> anyhow::Result<()> {
        let sdk = self.sdk.clone();
        let emitter = self.emitter.clone();
        let ledger = Arc::clone(&self.ledger);
        let clock = self.clock;
        tokio::spawn(run_cancel(sdk, emitter, ledger, clock, cmd));
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
}
