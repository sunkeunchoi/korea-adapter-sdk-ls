//! The dual-source fill ledger — the single exactly-once emission seam (U1, KTD1).
//!
//! Both fill sources (the t0425 poll loop, U3; the SC0/SC1 order-event lane, U2)
//! feed [`FillObservation`]s into one [`FillLedger`], which returns the
//! [`FillDelta`]s to emit. No lane emits `OrderFilled` directly — dual-source
//! exactly-once (R2, AE1) lives here so it is proven in one tested component.
//!
//! **Accounting is per-OrdNo, not per-chain (KTD1).** t0425 `cheqty` is cumulative
//! *per order-number row* and **restarts** on a modify's new OrdNo, so each chained
//! OrdNo carries its own cumulative watermark and the chain total is their sum. A
//! per-chain watermark would under-emit fills landing on a post-modify OrdNo.
//!
//! **Cross-source merge rule (KTD1).** Each observation carries a *source
//! cumulative* filled quantity for its OrdNo: the poll lane reports t0425 `cheqty`
//! directly (already cumulative); the SC lane reports one execution's `execqty`,
//! which the ledger accumulates per OrdNo behind an execution-number dedup. Every
//! observation then emits `max(0, source_cumulative − watermark)` against its
//! OrdNo's watermark and advances it — so a fill observed by both lanes emits
//! exactly once (AE1), regardless of arrival order.
//!
//! The ledger also **owns the [`OrderChain`]** (so an observation on any chained
//! OrdNo resolves to the originating order) and **retains the `OrderAny` emission
//! context** registered at submit, because every nautilus-live emit method takes
//! `&OrderAny` and nothing else in the adapter holds one after the submit task ends.

use std::collections::{BTreeSet, HashMap, HashSet};

use nautilus_model::enums::OrderSide;
use nautilus_model::identifiers::{ClientOrderId, TradeId};
use nautilus_model::orders::{Order, OrderAny};

use crate::orders::chain::OrderChain;

/// Which lane observed a fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillSource {
    /// The authoritative t0425 poll loop (U3). Carries a cumulative `cheqty`.
    Poll,
    /// The SC0/SC1 order-event lane (U2). Carries one execution's `execqty` + execno.
    Sc,
}

/// A single fill observation from one source, keyed on a specific OrdNo.
#[derive(Debug, Clone)]
pub struct FillObservation {
    /// Which lane produced it.
    pub source: FillSource,
    /// The OrdNo this observation is keyed on (may be any chained OrdNo).
    pub ord_no: String,
    /// The observed filled quantity. **Poll:** cumulative `cheqty` for this OrdNo.
    /// **Sc:** this frame's per-execution `execqty`.
    pub qty: i64,
    /// Fill price (integer KRW): the t0425 row's `cheprice` execution price for
    /// [`FillSource::Poll`] (KTD4) when it parsed positive, else the order's limit
    /// price; `execprc` for [`FillSource::Sc`].
    pub price: i64,
    /// The price is an approximation, not an exact per-fill execution price (KTD4):
    /// set by the poll seam when `cheprice` was absent/garbage and the limit price
    /// was used as a fallback. The ledger additionally flags any beyond-first poll
    /// partial (a row carries one `cheprice` per order, so a second partial's price
    /// is the row's current value — approximate). Always `false` for the SC lane,
    /// whose `execprc` is an exact per-execution price.
    pub price_approximated: bool,
    /// The SC execution number (the dedup key). `None` for a poll observation
    /// (t0425 carries no execution number).
    pub exec_no: Option<String>,
    /// The traded symbol (bare short code, U1/KTD3), if the source carried one.
    /// `None` for a blank/absent symbol at the `ToEvent` seam — the ledger then
    /// records nothing pending for an unknown order (the KTD3 empty-symbol guard).
    pub symbol: Option<String>,
}

impl FillObservation {
    /// A poll (t0425) observation: cumulative `cheqty` at the resolved fill `price`
    /// (KTD4: the row's `cheprice` when positive, else the limit price with
    /// `price_approximated` set by the caller).
    pub fn poll(
        ord_no: impl Into<String>,
        cumulative_qty: i64,
        price: i64,
        price_approximated: bool,
    ) -> Self {
        FillObservation {
            source: FillSource::Poll,
            ord_no: ord_no.into(),
            qty: cumulative_qty,
            price,
            price_approximated,
            exec_no: None,
            symbol: None,
        }
    }

    /// An SC1 observation: one execution's `execqty` at `execprc`, keyed by `execno`.
    /// The SC lane's price is exact per-execution, so it is never approximated.
    pub fn sc(
        ord_no: impl Into<String>,
        exec_qty: i64,
        exec_price: i64,
        exec_no: impl Into<String>,
    ) -> Self {
        FillObservation {
            source: FillSource::Sc,
            ord_no: ord_no.into(),
            qty: exec_qty,
            price: exec_price,
            price_approximated: false,
            exec_no: Some(exec_no.into()),
            symbol: None,
        }
    }

    /// Attach the traded symbol (U1, KTD3). A blank/absent symbol stays `None` so
    /// the ledger's pending-reconcile set never admits an empty string.
    pub fn with_symbol(mut self, symbol: Option<String>) -> Self {
        self.symbol = symbol;
        self
    }
}

/// An execution to emit to nautilus (the caller pairs it with the retained
/// [`OrderAny`] via [`FillLedger::order`] and calls `emit_order_filled`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillDelta {
    /// The originating order.
    pub client_order_id: ClientOrderId,
    /// The OrdNo the fill landed on (the venue order id to stamp on the event).
    pub ord_no: String,
    /// The incremental filled quantity to emit (always > 0).
    pub qty: i64,
    /// The fill price (integer KRW): the row's `cheprice` for a poll fill that
    /// carried one, else the limit-price fallback; `execprc` for an SC fill.
    pub price: i64,
    /// The price is an approximation, not an exact per-fill execution price (KTD4).
    /// True when the poll seam fell back to the limit price, OR this is a
    /// beyond-first poll partial on the OrdNo (its price is the row's current
    /// `cheprice`). The lab's data-quality collector counts these so the agent never
    /// reads an approximated price as exact (R14). Always `false` for SC fills.
    pub price_approximated: bool,
    /// A globally-unique trade id: real `execno` for an SC fill, a deterministic
    /// synthetic (`POLL-{ordno}-{watermark}`) for a poll-derived fill — the two
    /// schemes cannot collide (KTD5).
    pub trade_id: TradeId,
    /// Whether this delta completes the order (chain total ≥ order quantity).
    pub terminal: bool,
}

/// One recorded execution, in arrival order — the input to a realized-P&L accounting
/// seam (live-session-driver KTD8(a)).
///
/// The ledger's per-`OrdNo` watermarks are exactly-once *emission* accounting: they carry
/// no cost basis and no realized P&L, so a live max-loss breaker cannot "sum the
/// FillLedger". This journal is the additive record it can match offsetting fills over.
/// Written on the one seam every fill flows through ([`FillLedger::apply`]), so the SC and
/// poll lanes are both covered exactly once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerFill {
    /// The bare shcode the fill was on.
    pub symbol: String,
    /// The order side that produced the fill.
    pub side: OrderSide,
    /// The incremental filled quantity (always > 0).
    pub qty: i64,
    /// The fill price (integer KRW). May be an approximation — see
    /// [`FillDelta::price_approximated`].
    pub price: i64,
    /// Whether `price` is approximated rather than an exact execution price.
    pub price_approximated: bool,
    /// The globally-unique trade id (mirrors the emitted [`FillDelta`]).
    pub trade_id: TradeId,
    /// When the ledger recorded the execution (realtime unix nanos, the same clock the
    /// emitter stamps events with). The observation itself carries no timestamp, so this
    /// is *observation* time, not exchange time — good enough to order a session's trades
    /// and stamp the run's performance report, and never used for a safety decision.
    pub observed_ns: u64,
}

/// The outcome of applying one observation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApplyOutcome {
    /// The executions to emit (empty for a dedup no-op or an unknown OrdNo).
    pub deltas: Vec<FillDelta>,
    /// The observation could not be trusted (unknown OrdNo, or a poll cumulative
    /// regression) — the caller must drive a reconcile (R4). Adoption of an unknown
    /// OrdNo is the poll loop's job (U3), not the ledger's.
    pub reconcile_needed: bool,
}

impl ApplyOutcome {
    fn none() -> Self {
        ApplyOutcome::default()
    }
    fn reconcile() -> Self {
        ApplyOutcome {
            deltas: Vec::new(),
            reconcile_needed: true,
        }
    }
}

/// Per-OrdNo cumulative accounting.
#[derive(Debug, Default)]
struct OrdNoState {
    /// Max cumulative quantity already emitted for this OrdNo (the watermark).
    watermark: i64,
    /// Running sum of SC `execqty` over unique execnos on this OrdNo (the SC lane's
    /// view of cumulative filled).
    sc_cumulative: i64,
}

/// A tracked order: its emission context + per-OrdNo accounting + terminal state.
struct Entry {
    order: OrderAny,
    symbol: String,
    side: OrderSide,
    order_qty: i64,
    limit_price: i64,
    per_ordno: HashMap<String, OrdNoState>,
    seen_execnos: HashSet<String>,
    terminal: bool,
}

impl Entry {
    fn chain_total(&self) -> i64 {
        self.per_ordno.values().map(|s| s.watermark).sum()
    }
}

/// An open order still known only by a synthetic `RECON-` venue id (its real OrdNo
/// was never learned — the ambiguous-submit path). The poll loop (U3)
/// intent-corroborates an unknown t0425 row against these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconCandidate {
    /// The originating order.
    pub client_order_id: ClientOrderId,
    /// Order symbol (shcode).
    pub symbol: String,
    /// Order side.
    pub side: OrderSide,
    /// Order quantity.
    pub qty: i64,
    /// Order limit price (integer KRW).
    pub price: i64,
}

/// The single fill-emission seam. Owns the [`OrderChain`] and the per-order entries.
#[derive(Default)]
pub struct FillLedger {
    chain: OrderChain,
    entries: HashMap<ClientOrderId, Entry>,
    /// Symbols the ledger owes a reconcile scan (U2, KTD1/KTD2): recorded when an
    /// SC-sourced fill names an order the ledger does not know, or a cancel is
    /// skipped ("nothing resting" / unknown order). The poll drive unions this
    /// (drained under the lock at pass start) with [`Self::open_symbols`] so a
    /// flat ledger still scans the affected symbol. Sorted + deduped; never admits
    /// an empty string (KTD3). Insertion is deliberately restricted to those
    /// sources — a poll-sourced unknown row must NOT insert, or a foreign resting
    /// order (an operator's manual HTS order on the same account) would make the
    /// drive self-sustaining forever.
    pending_reconcile: BTreeSet<String>,
    /// Every emitted execution, in arrival order (live-session-driver KTD8(a)) — the
    /// realized-P&L accounting seam's input. Append-only; never read by the emission path.
    fills: Vec<LedgerFill>,
}

impl FillLedger {
    /// A fresh, empty ledger.
    pub fn new() -> Self {
        FillLedger::default()
    }

    /// Register a newly-accepted order and its initial OrdNo. Stores the emission
    /// context (`OrderAny`) and opens the OrdNo in the chain.
    pub fn register(&mut self, order: OrderAny, ord_no: impl Into<String>) {
        let ord_no = ord_no.into();
        let client_order_id = order.client_order_id();
        let symbol = order.instrument_id().symbol.as_str().to_string();
        let side = order.order_side();
        let order_qty = order.quantity().as_f64() as i64;
        let limit_price = order.price().map(|p| p.as_f64() as i64).unwrap_or(0);
        self.chain.register(client_order_id, ord_no.clone());
        self.entries.insert(
            client_order_id,
            Entry {
                order,
                symbol,
                side,
                order_qty,
                limit_price,
                per_ordno: {
                    let mut m = HashMap::new();
                    m.insert(ord_no, OrdNoState::default());
                    m
                },
                seen_execnos: HashSet::new(),
                terminal: false,
            },
        );
    }

    /// Append a modify/cancel's new OrdNo to an order's chain (delegates to
    /// [`OrderChain::append_child`]). Returns `false` if the parent is unknown.
    pub fn append_child(&mut self, parent_ord_no: &str, new_ord_no: impl Into<String>) -> bool {
        let new_ord_no = new_ord_no.into();
        if self.chain.append_child(parent_ord_no, new_ord_no.clone()) {
            if let Some(client) = self.chain.resolve(&new_ord_no) {
                if let Some(entry) = self.entries.get_mut(&client) {
                    entry.per_ordno.entry(new_ord_no).or_default();
                }
            }
            true
        } else {
            false
        }
    }

    /// Adopt a real OrdNo into an order's chain (chain repair, U3): registers
    /// `ord_no` for the order that currently owns `known_ord_no`. Used when an
    /// unknown t0425 row is corroborated to a `RECON-`-only order. Returns `false`
    /// if `known_ord_no` is not in any chain.
    pub fn adopt(&mut self, ord_no: impl Into<String>, known_ord_no: &str) -> bool {
        let ord_no = ord_no.into();
        match self.chain.resolve(known_ord_no) {
            Some(client) => {
                self.chain.register(client, ord_no.clone());
                if let Some(entry) = self.entries.get_mut(&client) {
                    entry.per_ordno.entry(ord_no).or_default();
                }
                true
            }
            None => false,
        }
    }

    /// Adopt a real OrdNo directly onto a known order (intent-corroboration, U3):
    /// registers `ord_no` for `client`. Returns `false` if `client` is untracked.
    pub fn adopt_for_client(&mut self, ord_no: impl Into<String>, client: ClientOrderId) -> bool {
        if !self.entries.contains_key(&client) {
            return false;
        }
        let ord_no = ord_no.into();
        self.chain.register(client, ord_no.clone());
        self.entries
            .get_mut(&client)
            .expect("entry present")
            .per_ordno
            .entry(ord_no)
            .or_default();
        true
    }

    /// Resolve any chained OrdNo to its originating order.
    pub fn resolve(&self, ord_no: &str) -> Option<ClientOrderId> {
        self.chain.resolve(ord_no)
    }

    /// The latest (most-recent) OrdNo in an order's chain — the modify/cancel target
    /// (KRX modify/cancel reference the current live order number, U4).
    pub fn latest_ord_no(&self, client: &ClientOrderId) -> Option<String> {
        self.chain.chain_of(client).and_then(|c| c.last().cloned())
    }

    /// Record a modify's new quantity/price on the retained accounting (U4), so
    /// terminal detection tracks the modified quantity. The retained `OrderAny`
    /// (emission identity) is unchanged.
    pub fn note_modify(&mut self, client: &ClientOrderId, new_qty: i64, new_price: i64) {
        if let Some(e) = self.entries.get_mut(client) {
            e.order_qty = new_qty;
            e.limit_price = new_price;
        }
    }

    /// Close an order out of the open set (a confirmed cancel, U4): marks it terminal
    /// and forgets its chain so its OrdNos free up.
    pub fn close(&mut self, client: &ClientOrderId) {
        if let Some(e) = self.entries.get_mut(client) {
            e.terminal = true;
        }
        self.chain.forget(client);
    }

    /// The retained emission context for an order (for `emit_order_filled` et al.).
    pub fn order(&self, client_order_id: &ClientOrderId) -> Option<&OrderAny> {
        self.entries.get(client_order_id).map(|e| &e.order)
    }

    /// Whether any non-terminal order is open (drives the U3 poll loop's idle).
    pub fn has_open_orders(&self) -> bool {
        self.entries.values().any(|e| !e.terminal)
    }

    /// Distinct symbols with at least one open order (the U3 poll set).
    pub fn open_symbols(&self) -> Vec<String> {
        let mut syms: Vec<String> = self
            .entries
            .values()
            .filter(|e| !e.terminal)
            .map(|e| e.symbol.clone())
            .collect();
        syms.sort();
        syms.dedup();
        syms
    }

    /// Record a symbol the ledger owes a reconcile scan (U2, KTD1/KTD2). A blank
    /// or whitespace symbol is dropped (the KTD3 empty-symbol guard — an empty
    /// `expcode` would be the banned flat scan).
    pub fn record_pending_symbol(&mut self, symbol: &str) {
        let s = symbol.trim();
        if !s.is_empty() {
            self.pending_reconcile.insert(s.to_string());
        }
    }

    /// Whether any symbol is pending a reconcile scan (the poll loop's cadence
    /// gate consults this so a flat ledger with a consumed arm still wakes, KTD2).
    pub fn has_pending(&self) -> bool {
        !self.pending_reconcile.is_empty()
    }

    /// Drain (take) the pending-reconcile set as a snapshot under the ledger lock
    /// at a poll pass's start (KTD2). Symbols the pass does not conclusively scan
    /// are re-inserted via [`Self::reinsert_pending_symbols`].
    pub fn take_pending_symbols(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_reconcile).into_iter().collect()
    }

    /// Re-insert pending symbols an exhausted/errored drive did not conclusively
    /// scan (R2). Blank entries are still guarded (KTD3).
    pub fn reinsert_pending_symbols(&mut self, symbols: impl IntoIterator<Item = String>) {
        for s in symbols {
            self.record_pending_symbol(&s);
        }
    }

    /// Record the symbol of an SC-sourced observation for an order the ledger does
    /// not know (KTD1). Poll-sourced unknown rows do NOT insert — the poll scanning
    /// the symbol is itself the reconcile, and re-inserting from the scan would make
    /// the set self-sustaining under a foreign resting order.
    fn note_unknown_symbol(&mut self, obs: &FillObservation) {
        if obs.source == FillSource::Sc {
            if let Some(sym) = obs.symbol.as_deref() {
                self.record_pending_symbol(sym);
            }
        }
    }

    /// The order's limit price (the poll-derived fill basis, KTD5), if tracked.
    /// Maintained by [`Self::note_modify`], so it reflects the *current* order state
    /// after a modify — unlike the retained `OrderAny`, which is emission-identity
    /// only and is not rewritten on a modify.
    pub fn limit_price(&self, client_order_id: &ClientOrderId) -> Option<i64> {
        self.entries.get(client_order_id).map(|e| e.limit_price)
    }

    /// The order's current quantity (maintained by [`Self::note_modify`]), if
    /// tracked. Reflects a modify's new quantity; the retained `OrderAny` does not.
    pub fn order_qty(&self, client_order_id: &ClientOrderId) -> Option<i64> {
        self.entries.get(client_order_id).map(|e| e.order_qty)
    }

    /// The order's remaining (unfilled) quantity: the maintained `order_qty`
    /// minus the per-OrdNo fill-watermark sum (the chain total), clamped at zero
    /// (a modify can reduce quantity below the already-filled total). Reads only
    /// ledger-maintained fields — never the retained `OrderAny`, which is frozen
    /// emission identity. `None` for an untracked order; the cancel path then
    /// falls back to full quantity, because refusing to cancel is the one
    /// unacceptable failure mode.
    pub fn remaining_qty(&self, client_order_id: &ClientOrderId) -> Option<i64> {
        self.entries
            .get(client_order_id)
            .map(|e| (e.order_qty - e.chain_total()).max(0))
    }

    /// Open orders known only by a `RECON-` venue id — U3 intent-corroboration
    /// candidates. An order qualifies when every OrdNo in its chain is a `RECON-`
    /// placeholder (its real OrdNo was never learned).
    pub fn open_recon_candidates(&self) -> Vec<ReconCandidate> {
        self.entries
            .iter()
            .filter(|(client, e)| {
                !e.terminal
                    && self
                        .chain
                        .chain_of(client)
                        .map(|c| !c.is_empty() && c.iter().all(|o| o.starts_with("RECON-")))
                        .unwrap_or(false)
            })
            .map(|(client, e)| ReconCandidate {
                client_order_id: *client,
                symbol: e.symbol.clone(),
                side: e.side,
                qty: e.order_qty,
                price: e.limit_price,
            })
            .collect()
    }

    /// Every execution recorded on this ledger, in arrival order (KTD8(a)) — the input to
    /// the live session's realized-P&L accounting. Exactly the executions that were
    /// emitted; the exactly-once discipline in [`Self::apply`] is what makes it safe to
    /// match offsetting fills over.
    pub fn fills(&self) -> &[LedgerFill] {
        &self.fills
    }

    /// Apply one fill observation, returning the executions to emit (KTD1). The
    /// heart of the exactly-once seam.
    pub fn apply(&mut self, obs: FillObservation) -> ApplyOutcome {
        // Resolve the OrdNo to an order. An unknown OrdNo never emits — it signals a
        // reconcile (adoption is the poll loop's job, U3).
        let Some(client) = self.chain.resolve(&obs.ord_no) else {
            // Unknown OrdNo: record the symbol pending (SC-sourced only, KTD1) so
            // the next armed drive scans it even on a flat ledger (R1).
            self.note_unknown_symbol(&obs);
            return ApplyOutcome::reconcile();
        };
        let Some(entry) = self.entries.get_mut(&client) else {
            self.note_unknown_symbol(&obs);
            return ApplyOutcome::reconcile();
        };
        if entry.terminal {
            // Late frame for a completed order — nothing to emit.
            return ApplyOutcome::none();
        }

        // Ensure this OrdNo has accounting state, then compute its source cumulative.
        entry.per_ordno.entry(obs.ord_no.clone()).or_default();
        let source_cumulative = match obs.source {
            FillSource::Poll => obs.qty,
            FillSource::Sc => {
                if let Some(execno) = &obs.exec_no {
                    // Dedup on execution number first: a replayed/out-of-order frame
                    // for a seen execno never advances the SC accumulator.
                    if !entry.seen_execnos.insert(execno.clone()) {
                        return ApplyOutcome::none();
                    }
                }
                // Re-borrow the state mutably (the seen-execno insert borrowed entry).
                let state = entry.per_ordno.entry(obs.ord_no.clone()).or_default();
                state.sc_cumulative += obs.qty.max(0);
                state.sc_cumulative
            }
        };

        let state = entry.per_ordno.get_mut(&obs.ord_no).expect("state present");

        // A poll cumulative below this OrdNo's watermark is a regression (a
        // truncated/stale read) — never a negative delta; fail toward reconcile.
        if obs.source == FillSource::Poll && source_cumulative < state.watermark {
            return ApplyOutcome::reconcile();
        }

        let delta_qty = (source_cumulative - state.watermark).max(0);
        if delta_qty == 0 {
            return ApplyOutcome::none();
        }
        // A beyond-first poll partial on this OrdNo is approximate by construction:
        // the row carries one `cheprice` per order, so a later partial's price is the
        // row's current value, not this increment's exact fill price (KTD4).
        let beyond_first_poll_partial = obs.source == FillSource::Poll && state.watermark > 0;
        let price_approximated = obs.price_approximated || beyond_first_poll_partial;
        state.watermark = source_cumulative;

        let chain_total = entry.chain_total();
        let terminal = chain_total >= entry.order_qty;
        if terminal {
            entry.terminal = true;
        }

        let symbol = entry.symbol.clone();
        let side = entry.side;
        let trade_id = match (obs.source, &obs.exec_no) {
            (FillSource::Sc, Some(execno)) => TradeId::from(format!("SC-{execno}").as_str()),
            _ => TradeId::from(format!("POLL-{}-{}", obs.ord_no, source_cumulative).as_str()),
        };
        let delta = FillDelta {
            client_order_id: client,
            ord_no: obs.ord_no.clone(),
            qty: delta_qty,
            price: obs.price,
            price_approximated,
            trade_id,
            terminal,
        };

        // Journal the execution for the realized-P&L accounting seam (KTD8(a)). Append
        // only — nothing on the emission path reads it.
        self.fills.push(LedgerFill {
            symbol,
            side,
            qty: delta_qty,
            price: obs.price,
            price_approximated,
            trade_id,
            observed_ns: nautilus_core::time::get_atomic_clock_realtime().get_time_ns().as_u64(),
        });

        // On terminal, forget the chain so the OrdNos free up (KTD1). The entry
        // stays (terminal=true) so `order()` still resolves the emission context.
        if terminal {
            self.chain.forget(&client);
        }
        ApplyOutcome {
            deltas: vec![delta],
            reconcile_needed: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::enums::{OrderSide, OrderType, TimeInForce};
    use nautilus_model::identifiers::InstrumentId;
    use nautilus_model::orders::OrderTestBuilder;
    use nautilus_model::types::{Price, Quantity};

    fn order(client_id: &str, qty: i64, price: i64, side: OrderSide) -> OrderAny {
        OrderTestBuilder::new(OrderType::Limit)
            .instrument_id(InstrumentId::from("005930.XKRX"))
            .client_order_id(ClientOrderId::from(client_id))
            .side(side)
            .quantity(Quantity::from(qty))
            .price(Price::from(price.to_string().as_str()))
            .time_in_force(TimeInForce::Day)
            .build()
    }

    fn ledger_with(client_id: &str, qty: i64, price: i64, ord_no: &str) -> (FillLedger, ClientOrderId) {
        let mut led = FillLedger::new();
        let o = order(client_id, qty, price, OrderSide::Buy);
        let cid = o.client_order_id();
        led.register(o, ord_no);
        (led, cid)
    }

    /// AE1: poll observes a fill, then the same execution arrives as SC1 → exactly
    /// one delta.
    #[test]
    fn poll_then_sc_of_same_execution_emits_once() {
        let (mut led, cid) = ledger_with("O-1", 100, 60_000, "1001");
        // Poll sees cheqty=30.
        let out = led.apply(FillObservation::poll("1001", 30, 60_000, false));
        assert_eq!(out.deltas.len(), 1);
        assert_eq!(out.deltas[0].qty, 30);
        assert_eq!(out.deltas[0].client_order_id, cid);
        assert!(!out.deltas[0].terminal);
        // The same 30-qty execution now arrives via SC1 (execno E1) → no new delta.
        let out = led.apply(FillObservation::sc("1001", 30, 60_100, "E1"));
        assert!(out.deltas.is_empty(), "the already-polled fill must not re-emit");
        assert!(!out.reconcile_needed);
    }

    /// SC1 partial fills accumulate (30 + 70 on qty 100 → two deltas, then terminal).
    #[test]
    fn sc_partial_fills_accumulate_then_terminal() {
        let (mut led, _cid) = ledger_with("O-2", 100, 60_000, "1001");
        let out = led.apply(FillObservation::sc("1001", 30, 60_000, "E1"));
        assert_eq!(out.deltas.len(), 1);
        assert_eq!(out.deltas[0].qty, 30);
        assert!(!out.deltas[0].terminal);
        let out = led.apply(FillObservation::sc("1001", 70, 60_050, "E2"));
        assert_eq!(out.deltas.len(), 1);
        assert_eq!(out.deltas[0].qty, 70);
        assert_eq!(out.deltas[0].price, 60_050);
        assert!(out.deltas[0].terminal, "reaching order qty is terminal");
    }

    /// A replayed SC execno produces no delta.
    #[test]
    fn replayed_sc_execno_is_deduped() {
        let (mut led, _cid) = ledger_with("O-3", 100, 60_000, "1001");
        let out = led.apply(FillObservation::sc("1001", 30, 60_000, "E1"));
        assert_eq!(out.deltas.len(), 1);
        // Replay of E1 (same execno) → no delta and no reconcile.
        let out = led.apply(FillObservation::sc("1001", 30, 60_000, "E1"));
        assert!(out.deltas.is_empty());
        assert!(!out.reconcile_needed);
    }

    /// Post-modify accounting (KTD1): a 30-fill on OrdNo₀, modify chains OrdNo₁, then
    /// a poll row for OrdNo₁ with cheqty=40 → emits 40 (per-OrdNo watermarks; a naive
    /// per-chain watermark would emit only 10).
    #[test]
    fn post_modify_poll_uses_per_ordno_watermark() {
        let (mut led, _cid) = ledger_with("O-4", 100, 60_000, "1001");
        let out = led.apply(FillObservation::poll("1001", 30, 60_000, false));
        assert_eq!(out.deltas[0].qty, 30);
        // A modify chains a new OrdNo 1002 (cheqty restarts at 0 on it).
        assert!(led.append_child("1001", "1002"));
        let out = led.apply(FillObservation::poll("1002", 40, 60_000, false));
        assert_eq!(out.deltas.len(), 1);
        assert_eq!(out.deltas[0].qty, 40, "per-OrdNo watermark emits the full 40");
        assert_eq!(out.deltas[0].ord_no, "1002");
    }

    /// A poll cumulative below the OrdNo watermark is a regression → no delta,
    /// reconcile-flagged.
    #[test]
    fn poll_regression_flags_reconcile() {
        let (mut led, _cid) = ledger_with("O-5", 100, 60_000, "1001");
        led.apply(FillObservation::poll("1001", 50, 60_000, false));
        let out = led.apply(FillObservation::poll("1001", 20, 60_000, false));
        assert!(out.deltas.is_empty());
        assert!(out.reconcile_needed, "a cheqty regression must flag reconcile");
    }

    /// An observation on a chained (modified) OrdNo resolves to the original order.
    #[test]
    fn fill_on_modified_ordno_resolves_to_original() {
        let (mut led, cid) = ledger_with("O-6", 100, 60_000, "1001");
        assert!(led.append_child("1001", "1002"));
        let out = led.apply(FillObservation::sc("1002", 100, 60_000, "E9"));
        assert_eq!(out.deltas.len(), 1);
        assert_eq!(out.deltas[0].client_order_id, cid);
        assert!(out.deltas[0].terminal);
    }

    /// An unknown OrdNo emits nothing and flags a reconcile (adoption is U3's job).
    #[test]
    fn unknown_ordno_flags_reconcile_no_emission() {
        let (mut led, _cid) = ledger_with("O-7", 100, 60_000, "1001");
        let out = led.apply(FillObservation::poll("9999", 10, 60_000, false));
        assert!(out.deltas.is_empty());
        assert!(out.reconcile_needed);
    }

    /// Poll-derived fills carry a deterministic synthetic trade id distinct from SC.
    #[test]
    fn poll_and_sc_trade_ids_do_not_collide() {
        let (mut led, _cid) = ledger_with("O-8", 100, 60_000, "1001");
        let poll = led.apply(FillObservation::poll("1001", 30, 60_000, false));
        assert_eq!(poll.deltas[0].trade_id, TradeId::from("POLL-1001-30"));
        let sc = led.apply(FillObservation::sc("1001", 70, 60_050, "E2"));
        assert_eq!(sc.deltas[0].trade_id, TradeId::from("SC-E2"));
    }

    /// RECON- adoption: a RECON--only order is an intent-corroboration candidate;
    /// adopting a real OrdNo lets a fill on it emit.
    #[test]
    fn recon_candidate_adoption_enables_fill() {
        let mut led = FillLedger::new();
        let o = order("O-9", 100, 60_000, OrderSide::Buy);
        let cid = o.client_order_id();
        led.register(o, "RECON-O-9");
        let cands = led.open_recon_candidates();
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].client_order_id, cid);
        assert_eq!(cands[0].qty, 100);
        // Adopt the real OrdNo 5001 into the RECON- order's chain.
        assert!(led.adopt("5001", "RECON-O-9"));
        let out = led.apply(FillObservation::poll("5001", 100, 60_000, false));
        assert_eq!(out.deltas.len(), 1);
        assert_eq!(out.deltas[0].client_order_id, cid);
        assert!(out.deltas[0].terminal);
    }

    /// U2: a single full poll fill carrying a positive `cheprice` (approximated=false)
    /// emits at that price, unflagged.
    #[test]
    fn single_poll_fill_with_cheprice_is_exact() {
        let (mut led, _cid) = ledger_with("O-EX", 100, 60_000, "1001");
        let out = led.apply(FillObservation::poll("1001", 100, 60_050, false));
        assert_eq!(out.deltas.len(), 1);
        assert_eq!(out.deltas[0].price, 60_050);
        assert!(!out.deltas[0].price_approximated, "a first full fill at cheprice is exact");
    }

    /// U2: a poll fill whose seam fell back to the limit price (approximated=true)
    /// carries the flag through to the delta.
    #[test]
    fn poll_fallback_price_is_flagged_approximated() {
        let (mut led, _cid) = ledger_with("O-FB", 100, 60_000, "1001");
        let out = led.apply(FillObservation::poll("1001", 40, 60_000, true));
        assert_eq!(out.deltas.len(), 1);
        assert!(out.deltas[0].price_approximated, "a limit-price fallback is approximate");
    }

    /// U2: a beyond-first poll partial on the same OrdNo is flagged approximate even
    /// when its `cheprice` parsed positive — a row carries one price per order (KTD4).
    #[test]
    fn beyond_first_poll_partial_is_flagged() {
        let (mut led, _cid) = ledger_with("O-MP", 100, 60_000, "1001");
        let first = led.apply(FillObservation::poll("1001", 30, 60_050, false));
        assert_eq!(first.deltas[0].qty, 30);
        assert!(!first.deltas[0].price_approximated, "the first partial's cheprice is exact");
        let second = led.apply(FillObservation::poll("1001", 100, 60_070, false));
        assert_eq!(second.deltas[0].qty, 70);
        assert!(
            second.deltas[0].price_approximated,
            "a beyond-first partial is approximate — one cheprice per row"
        );
    }

    /// Remaining quantity reads ledger-maintained fields: order_qty − chain
    /// total, tracking fills across chained OrdNos and note_modify reductions,
    /// clamped at zero when a modify drops quantity below the filled total.
    #[test]
    fn remaining_qty_tracks_fills_modifies_and_clamps() {
        let (mut led, cid) = ledger_with("O-REM", 10, 60_000, "1001");
        assert_eq!(led.remaining_qty(&cid), Some(10), "unfilled → full quantity");
        led.apply(FillObservation::poll("1001", 4, 60_000, false));
        assert_eq!(led.remaining_qty(&cid), Some(6), "10 − 4 filled");
        // Fills on a chained OrdNo count toward the chain total.
        assert!(led.append_child("1001", "1002"));
        led.apply(FillObservation::poll("1002", 2, 60_000, false));
        assert_eq!(led.remaining_qty(&cid), Some(4), "per-OrdNo watermarks sum");
        // A modify below the filled total clamps at zero, never negative.
        led.note_modify(&cid, 3, 60_000);
        assert_eq!(led.remaining_qty(&cid), Some(0), "qty 3 < 6 filled → clamp at 0");
        // Untracked order → None (the caller falls back to full quantity).
        assert_eq!(led.remaining_qty(&ClientOrderId::from("O-NOPE")), None);
    }

    /// U2/AE1: an SC-sourced fill for an unknown OrdNo records the observation's
    /// symbol pending (so a flat ledger's next drive scans it); a poll-sourced
    /// unknown row records NOTHING pending (KTD1 — the scan is itself the reconcile).
    #[test]
    fn sc_unknown_fill_records_pending_poll_does_not() {
        let (mut led, _cid) = ledger_with("O-P1", 100, 60_000, "1001");
        assert!(!led.has_pending());
        // SC unknown OrdNo with a symbol → pending recorded, reconcile flagged.
        let out = led.apply(FillObservation::sc("9999", 10, 60_000, "E9").with_symbol(Some("000660".to_string())));
        assert!(out.reconcile_needed);
        assert!(led.has_pending());
        assert_eq!(led.take_pending_symbols(), vec!["000660".to_string()]);
        assert!(!led.has_pending(), "take drains the set");
        // Poll unknown OrdNo carrying a symbol → reconcile flagged but NOTHING pending.
        let out = led.apply(FillObservation::poll("8888", 10, 60_000, false).with_symbol(Some("000660".to_string())));
        assert!(out.reconcile_needed);
        assert!(!led.has_pending(), "a poll-sourced unknown row must not self-sustain the pending set");
    }

    /// U2 (KTD3 guard): an SC unknown fill with no symbol records nothing pending.
    #[test]
    fn sc_unknown_fill_with_blank_symbol_records_nothing() {
        let mut led = FillLedger::new();
        let out = led.apply(FillObservation::sc("9999", 10, 60_000, "E9").with_symbol(None));
        assert!(out.reconcile_needed);
        assert!(!led.has_pending(), "a blank symbol never admits to the pending set");
        // An empty string is also refused at the record seam.
        led.record_pending_symbol("   ");
        assert!(!led.has_pending());
    }

    /// U2/R2: take drains a snapshot; reinsert restores un-scanned symbols; the set
    /// dedups repeated records for one symbol.
    #[test]
    fn pending_set_dedups_and_reinserts() {
        let mut led = FillLedger::new();
        led.record_pending_symbol("005930");
        led.record_pending_symbol("005930"); // duplicate → deduped
        led.record_pending_symbol("000660");
        assert_eq!(led.take_pending_symbols(), vec!["000660".to_string(), "005930".to_string()]);
        assert!(!led.has_pending());
        // Reinsert un-scanned symbols (e.g. an errored fetch).
        led.reinsert_pending_symbols(vec!["005930".to_string()]);
        assert!(led.has_pending());
        assert_eq!(led.take_pending_symbols(), vec!["005930".to_string()]);
    }

    /// Open-order queries drive the poll loop's idle + poll set.
    #[test]
    fn open_queries_track_terminal_state() {
        let (mut led, _cid) = ledger_with("O-10", 100, 60_000, "1001");
        assert!(led.has_open_orders());
        assert_eq!(led.open_symbols(), vec!["005930".to_string()]);
        // Fill it fully → no longer open.
        led.apply(FillObservation::poll("1001", 100, 60_000, false));
        assert!(!led.has_open_orders());
        assert!(led.open_symbols().is_empty());
    }
}
