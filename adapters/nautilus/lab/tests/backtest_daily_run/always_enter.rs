//! The test-only always-enter strategy the suite drives the daily runner with: a full
//! [`DailyPathStrategy`] + [`DataActor`] pair, and the only large non-scenario item in
//! this suite. Split out of the crate root because the engine-phase, selection-phase and
//! entry-risk scenarios all share it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use nautilus_common::actor::{DataActor, DataActorNative};
use nautilus_ls_lab::artifacts::performance::{ClientOrderEntryRiskLedger, EntryRisk};
use nautilus_ls_lab::runner::backtest_daily::{DailyPathStrategy, MountedSymbol, OpenPositionBook};
use nautilus_model::data::Bar;
use nautilus_model::enums::{OrderSide, TimeInForce};
use nautilus_model::events::{PositionClosed, PositionOpened};
use nautilus_model::identifiers::{InstrumentId, PositionId, StrategyId};
use nautilus_model::orders::Order;
use nautilus_model::types::{Price, Quantity};
use nautilus_trading::nautilus_strategy;
use nautilus_trading::strategy::{Strategy, StrategyConfig, StrategyCore};

/// How the test strategy behaves. Deliberately trivial: batch membership IS the
/// instruction, because the runner only ever delivers a bar for a symbol that is
/// either already held or newly taken this session.
#[derive(Debug, Clone)]
pub(crate) struct AlwaysEnterConfig {
    /// Exit after this many session bars observed while the position is open.
    pub(crate) hold_sessions: usize,
    /// The entry-fixed stop, as KRW below the entry bar's close. `None` disables it.
    pub(crate) stop_below: Option<i64>,
    /// Whether a symbol may be entered again after a completed round trip.
    pub(crate) reenter: bool,
    /// The fixed order quantity.
    pub(crate) qty: i64,
    /// The first entry's recorded `risk_per_share`. Every subsequent entry records
    /// `risk_base + n · risk_step`, so **every entry in a run carries a distinct
    /// risk value** — a uniform-value fixture would hide a mis-ordered projection
    /// entirely (KTD3 assertion 3).
    pub(crate) risk_base: f64,
    /// The per-entry increment that makes the recorded risks distinct.
    pub(crate) risk_step: f64,
    /// A symbol entered **without** recording an entry risk — the position then has
    /// no ledger entry and must resolve to `None` (the legacy P&L path).
    pub(crate) skip_risk: Option<&'static str>,
    /// A symbol whose entry order is submitted at an off-precision price. KRX
    /// equities carry `price_precision = 0`, so the risk engine denies a
    /// precision-1 price: a recorded ledger entry that never opens a position.
    pub(crate) reject_entry: Option<&'static str>,
}

impl Default for AlwaysEnterConfig {
    fn default() -> Self {
        AlwaysEnterConfig {
            hold_sessions: 6,
            stop_below: None,
            reenter: true,
            qty: 10,
            risk_base: 1_000.0,
            risk_step: 250.0,
            skip_risk: None,
            reject_entry: None,
        }
    }
}

/// Every bar the strategy actually saw, in arrival order — the stream-side witness
/// the cache-read count is checked against.
#[derive(Debug, Clone, Default)]
pub(crate) struct BarWitness(Arc<Mutex<Vec<(InstrumentId, u64)>>>);

impl BarWitness {
    fn observe(&self, id: InstrumentId, ts: u64) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).push((id, ts));
    }
    pub(crate) fn snapshot(&self) -> Vec<(InstrumentId, u64)> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// The test-only strategy: enter every symbol whose bar arrives while it is not
/// held, hold it for a fixed number of session bars, and exit on the entry-fixed
/// stop or at hold expiry.
///
/// Every exit carries `Some(position.id)` via [`Strategy::close_position`]. Under
/// `OmsType::Hedging` a fill whose client order id has no cached position mints a
/// *fresh* position and the netting fallback is disabled, so an exit submitted
/// without a position id would open an opposite-side short instead of closing the
/// long — ORB's `reduce_only`-only exit is a Netting-only pattern (KTD12).
pub(crate) struct AlwaysEnter {
    core: StrategyCore,
    cfg: AlwaysEnterConfig,
    mounted: Vec<MountedSymbol>,
    book: OpenPositionBook,
    witness: BarWitness,
    /// Symbols with an entry order submitted but not yet opened.
    pending: std::collections::BTreeSet<InstrumentId>,
    /// Symbols that have completed at least one round trip.
    done: std::collections::BTreeSet<InstrumentId>,
    /// Session bars observed since each open position opened.
    held_sessions: HashMap<InstrumentId, usize>,
    /// The entry-fixed stop price per open position.
    stops: HashMap<InstrumentId, i64>,
    /// The pending stop for a symbol whose entry has not filled yet.
    pending_stop: HashMap<InstrumentId, i64>,
    /// The live position id per held symbol.
    position_of: HashMap<InstrumentId, PositionId>,
    /// The shared, client-order-keyed entry-risk ledger (KTD3). `ClientOrderId` is
    /// the only identity available here at submit time.
    entry_risk: ClientOrderEntryRiskLedger,
    /// How many entries have been recorded — the distinct-risk counter.
    entries_recorded: usize,
}

impl AlwaysEnter {
    fn new(mounted: Vec<MountedSymbol>, cfg: AlwaysEnterConfig, witness: BarWitness) -> Self {
        AlwaysEnter {
            core: StrategyCore::new(StrategyConfig {
                strategy_id: Some(StrategyId::from("always-enter-v1")),
                ..Default::default()
            }),
            cfg,
            mounted,
            book: OpenPositionBook::new(),
            witness,
            pending: Default::default(),
            done: Default::default(),
            held_sessions: HashMap::new(),
            stops: HashMap::new(),
            pending_stop: HashMap::new(),
            position_of: HashMap::new(),
            entry_risk: ClientOrderEntryRiskLedger::new(),
            entries_recorded: 0,
        }
    }

    fn exit(&mut self, id: InstrumentId) {
        let Some(pos_id) = self.position_of.get(&id).copied() else {
            return;
        };
        let position = {
            let cache = self.core.cache_rc();
            let cache = cache.borrow();
            cache.position(&pos_id).map(|p| p.cloned())
        };
        if let Some(position) = position {
            // The framework's close-position helper threads `Some(position.id)`
            // through submission — mandatory under Hedging (KTD12).
            self.close_position(&position, None, None, Some(TimeInForce::Gtc), None, None)
                .expect("close_position");
        }
    }
}

impl DailyPathStrategy for AlwaysEnter {
    fn open_position_book(&self) -> OpenPositionBook {
        self.book.clone()
    }

    fn entry_risk_ledger(&self) -> ClientOrderEntryRiskLedger {
        self.entry_risk.clone()
    }
}

impl std::fmt::Debug for AlwaysEnter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlwaysEnter").field("mounted", &self.mounted.len()).finish()
    }
}

nautilus_strategy!(AlwaysEnter, core, {
    fn on_position_opened(&mut self, event: PositionOpened) {
        self.pending.remove(&event.instrument_id);
        // The stream-side witness for the reconciliation (KTD3 assertion 2): which
        // recorded entries actually opened a position, known independently of the
        // cache read the assertion checks.
        self.entry_risk.record_opened(event.opening_order_id);
        self.book.record_opened(event.instrument_id, event.position_id);
        self.position_of.insert(event.instrument_id, event.position_id);
        self.held_sessions.insert(event.instrument_id, 0);
        if let Some(stop) = self.pending_stop.remove(&event.instrument_id) {
            self.stops.insert(event.instrument_id, stop);
        }
    }

    fn on_position_closed(&mut self, event: PositionClosed) {
        self.book.record_closed(&event.instrument_id);
        self.position_of.remove(&event.instrument_id);
        self.held_sessions.remove(&event.instrument_id);
        self.stops.remove(&event.instrument_id);
        self.done.insert(event.instrument_id);
    }
});

impl DataActor for AlwaysEnter {
    fn on_start(&mut self) -> anyhow::Result<()> {
        for m in self.mounted.clone() {
            self.subscribe_bars(m.bar_type, None, None);
        }
        Ok(())
    }

    fn on_bar(&mut self, bar: &Bar) -> anyhow::Result<()> {
        let id = bar.bar_type.instrument_id();
        self.witness.observe(id, bar.ts_event.as_u64());

        if self.book.is_held(&id) {
            let n = self.held_sessions.entry(id).or_insert(0);
            *n += 1;
            let elapsed = *n;
            let stopped = self
                .stops
                .get(&id)
                .is_some_and(|stop| (bar.low.as_f64() as i64) <= *stop);
            if stopped || elapsed >= self.cfg.hold_sessions {
                self.exit(id);
            }
            return Ok(());
        }
        if self.pending.contains(&id) || (!self.cfg.reenter && self.done.contains(&id)) {
            return Ok(());
        }

        // Enter: a marketable limit BUY a long way through the bar's close.
        let close = bar.close.as_f64() as i64;
        let symbol = id.to_string();
        // KRX equities carry `price_precision = 0`; a precision-1 price is denied by
        // the risk engine, so the entry is recorded but never opens a position.
        let price = if self.cfg.reject_entry == Some(symbol.as_str()) {
            Price::new((close + 5_000) as f64 + 0.5, 1)
        } else {
            Price::from((close + 5_000).to_string().as_str())
        };
        let order = self.order().limit(
            id,
            OrderSide::Buy,
            Quantity::from(self.cfg.qty),
            price,
            Some(TimeInForce::Gtc),
            None, None, Some(false),
            None, None, None, None, None, None, None, None,
        );
        // Capture the entry-fixed risk keyed by CLIENT ORDER ID (KTD3) — the only
        // identity available at submit time, and exactly the key the read side
        // carries as `Position.opening_order_id`. Each entry gets a DISTINCT
        // `risk_per_share` so a mis-ordered projection cannot hide.
        if self.cfg.skip_risk != Some(symbol.as_str()) {
            let n = self.entries_recorded;
            self.entries_recorded += 1;
            self.entry_risk.record(
                order.client_order_id(),
                EntryRisk {
                    risk_per_share: self.cfg.risk_base + (n as f64) * self.cfg.risk_step,
                    qty: self.cfg.qty as f64,
                },
            );
        }
        self.submit_order(order, None, None, None)?;
        self.pending.insert(id);
        if let Some(below) = self.cfg.stop_below {
            self.pending_stop.insert(id, close - below);
        }
        Ok(())
    }
}

/// A factory for the runner: one closure that builds a fresh strategy sharing the
/// supplied witness.
pub(crate) fn always_enter(
    cfg: AlwaysEnterConfig,
    witness: BarWitness,
) -> impl Fn(&[MountedSymbol]) -> AlwaysEnter + Send + 'static {
    move |mounted: &[MountedSymbol]| AlwaysEnter::new(mounted.to_vec(), cfg.clone(), witness.clone())
}

/// Like [`always_enter`] but sharing a **caller-owned** entry-risk ledger, so a test
/// can read exactly what the strategy recorded at submit time and check the runner's
/// projection against it position by position.
pub(crate) fn always_enter_sharing(
    cfg: AlwaysEnterConfig,
    ledger: ClientOrderEntryRiskLedger,
) -> impl Fn(&[MountedSymbol]) -> AlwaysEnter + Send + 'static {
    move |mounted: &[MountedSymbol]| {
        let mut s = AlwaysEnter::new(mounted.to_vec(), cfg.clone(), BarWitness::default());
        s.entry_risk = ledger.clone();
        s
    }
}
