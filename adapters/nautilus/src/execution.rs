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

use async_trait::async_trait;
use ls_sdk::account::T0424Request;
use ls_sdk::orders::{CSPAT00601Request, OrderIntent, ReconcileOutcome, T0425Request};
use ls_sdk::LsSdk;
use nautilus_common::clients::ExecutionClient;
use nautilus_common::messages::execution::SubmitOrder;
use nautilus_core::time::{get_atomic_clock_realtime, AtomicTime};
use nautilus_core::UnixNanos;
use nautilus_model::accounts::AccountAny;
use nautilus_model::enums::{AccountType, OmsType, OrderSide};
use nautilus_model::events::{OrderEventAny, OrderInitialized};
use nautilus_model::identifiers::{AccountId, ClientId, TraderId, Venue, VenueOrderId};
use nautilus_model::orders::OrderAny;
use nautilus_model::types::{AccountBalance, Currency, MarginBalance};
use nautilus_live::execution::emitter::ExecutionEventEmitter;

use crate::error::{AdapterError, AdapterResult};
use crate::orders::chain::OrderChain;
use crate::orders::map::{classify_reconcile, classify_submit_error, ReconcileEvent, SubmitAction};
use crate::KRX_VENUE;

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
    chain: Arc<Mutex<OrderChain>>,
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
            chain: Arc::new(Mutex::new(OrderChain::new())),
            last_drop_count: Arc::new(AtomicU64::new(0)),
        }
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

/// Lock the order chain, recovering from a poisoned mutex (a panic in one submit
/// task must not cascade to lose every subsequent order — fail-open on data loss
/// is the trap).
fn lock_chain(chain: &Mutex<OrderChain>) -> std::sync::MutexGuard<'_, OrderChain> {
    chain.lock().unwrap_or_else(|e| e.into_inner())
}

/// The spawned submit worker: run the Orders facade, classify per KTD6, update the
/// chain, and emit the matching nautilus event.
async fn run_submit(
    sdk: LsSdk,
    emitter: ExecutionEventEmitter,
    chain: Arc<Mutex<OrderChain>>,
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
            lock_chain(&chain).register(client_order_id, ord_no.clone());
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
                        lock_chain(&chain).register(client_order_id, venue_id.clone());
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
        self.connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn disconnect(&mut self) -> anyhow::Result<()> {
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn submit_order(&self, cmd: SubmitOrder) -> anyhow::Result<()> {
        let sdk = self.sdk.clone();
        let emitter = self.emitter.clone();
        let chain = Arc::clone(&self.chain);
        let clock = self.clock;
        tokio::spawn(run_submit(sdk, emitter, chain, clock, cmd.order_init));
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
