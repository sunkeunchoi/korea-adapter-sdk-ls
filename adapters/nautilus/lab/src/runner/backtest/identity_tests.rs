use super::*;

use nautilus_common::actor::{DataActor, DataActorNative};
use nautilus_ls::rules::KRX_REGULAR_CLOSE;
use nautilus_model::data::BarType;
use nautilus_model::enums::{OrderSide, TimeInForce};
use nautilus_model::events::{PositionClosed, PositionOpened};
use nautilus_model::identifiers::{InstrumentId, PositionId, StrategyId};
use nautilus_model::types::{Price, Quantity};
use nautilus_trading::nautilus_strategy;
use nautilus_trading::strategy::{Strategy, StrategyConfig, StrategyCore};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A minimal KRX-shaped equity for the candidate-builder tests. `build_candidates` reads
/// only the id off an instrument, so the remaining fields just have to be valid.
fn sample_instrument(id: InstrumentId) -> nautilus_model::instruments::InstrumentAny {
    use nautilus_model::instruments::Equity;
    use nautilus_model::types::Currency;
    let equity = Equity::new(
        id,
        nautilus_model::identifiers::Symbol::from(shcode_of(&id.to_string())),
        None,
        Currency::KRW(),
        0,
        Price::from("1"),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        nautilus_core::UnixNanos::default(),
        nautilus_core::UnixNanos::default(),
    );
    nautilus_model::instruments::InstrumentAny::Equity(equity)
}

fn day(bt: BarType, ymd: (i32, u32, u32), open: i64, close: i64) -> Bar {
    let ts = kst_to_unix_nanos(
        NaiveDate::from_ymd_opt(ymd.0, ymd.1, ymd.2).unwrap(),
        KRX_REGULAR_CLOSE,
    )
    .unwrap();
    Bar::new(
        bt,
        Price::from(open.to_string().as_str()),
        Price::from((close + 10).to_string().as_str()),
        Price::from((open - 10).to_string().as_str()),
        Price::from(close.to_string().as_str()),
        Quantity::from(1000),
        ts,
        ts,
    )
}

/// A deliberately tiny strategy for the ORB venue identity guard: enter, close,
/// then enter and close the same symbol again. Netting reopens the symbol's one
/// stable position id; Hedging mints a distinct position for each round trip.
struct SameSymbolReentry {
    core: StrategyCore,
    bar_type: BarType,
    position_id: Option<PositionId>,
    entry_pending: bool,
    exit_pending: bool,
    entries_submitted: usize,
    completed_round_trips: Arc<AtomicUsize>,
}

impl SameSymbolReentry {
    fn new(bar_type: BarType, completed_round_trips: Arc<AtomicUsize>) -> Self {
        Self {
            core: StrategyCore::new(StrategyConfig {
                strategy_id: Some(StrategyId::from("orb-venue-identity")),
                ..Default::default()
            }),
            bar_type,
            position_id: None,
            entry_pending: false,
            exit_pending: false,
            entries_submitted: 0,
            completed_round_trips,
        }
    }
}

impl std::fmt::Debug for SameSymbolReentry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SameSymbolReentry")
            .field("entries_submitted", &self.entries_submitted)
            .finish()
    }
}

nautilus_strategy!(SameSymbolReentry, core, {
    fn on_position_opened(&mut self, event: PositionOpened) {
        self.entry_pending = false;
        self.position_id = Some(event.position_id);
    }

    fn on_position_closed(&mut self, _event: PositionClosed) {
        self.exit_pending = false;
        self.position_id = None;
        self.completed_round_trips.fetch_add(1, Ordering::Relaxed);
    }
});

impl DataActor for SameSymbolReentry {
    fn on_start(&mut self) -> anyhow::Result<()> {
        self.subscribe_bars(self.bar_type, None, None);
        Ok(())
    }

    fn on_bar(&mut self, _bar: &Bar) -> anyhow::Result<()> {
        if self.entry_pending || self.exit_pending {
            return Ok(());
        }

        if let Some(position_id) = self.position_id {
            let position = {
                let cache = self.core.cache_rc();
                let cache = cache.borrow();
                cache
                    .position(&position_id)
                    .map(|position| position.cloned())
            };
            if let Some(position) = position {
                self.close_position(&position, None, None, Some(TimeInForce::Gtc), None, None)?;
                self.exit_pending = true;
            }
        } else if self.entries_submitted < 2 {
            let order = self.order().market(
                self.bar_type.instrument_id(),
                OrderSide::Buy,
                Quantity::from(10),
                Some(TimeInForce::Gtc),
                Some(false),
                None,
                None,
                None,
                None,
                None,
            );
            self.submit_order(order, None, None, None)?;
            self.entry_pending = true;
            self.entries_submitted += 1;
        }
        Ok(())
    }
}

struct ReentryOutcome {
    position_count: usize,
    completed_round_trips: usize,
}

fn same_symbol_reentry_outcome(mut engine: BacktestEngine) -> ReentryOutcome {
    let id = InstrumentId::from("005930.XKRX");
    let instrument = sample_instrument(id);
    let bar_type = BarKind::Daily.bar_type(id).unwrap();
    engine.add_instrument(&instrument).unwrap();
    let completed_round_trips = Arc::new(AtomicUsize::new(0));
    engine
        .add_strategy(SameSymbolReentry::new(
            bar_type,
            Arc::clone(&completed_round_trips),
        ))
        .unwrap();
    let bars = (1..=7)
        .map(|day_of_month| day(bar_type, (2024, 1, day_of_month), 100, 100))
        .map(Data::Bar)
        .collect();
    engine.add_data(bars, None, true, true).unwrap();
    engine.run(None, None, None, false).unwrap();
    let position_count = engine
        .kernel()
        .cache
        .borrow()
        .positions(None, None, None, None, None)
        .len();
    ReentryOutcome {
        position_count,
        completed_round_trips: completed_round_trips.load(Ordering::Relaxed),
    }
}

#[test]
fn the_orb_venue_is_executably_netting_across_same_symbol_reentry() {
    let netting = same_symbol_reentry_outcome(orb_engine(100_000_000.0).unwrap());

    let hedging = simulated_venue_config(OmsType::Hedging, 100_000_000.0).unwrap();
    let hedging = same_symbol_reentry_outcome(engine_with_venue(hedging).unwrap());

    assert_eq!(netting.completed_round_trips, 2);
    assert_eq!(hedging.completed_round_trips, 2);
    assert_eq!(
        netting.position_count, 1,
        "Netting reopens one stable symbol position"
    );
    assert_eq!(
        hedging.position_count, 2,
        "Hedging keeps both same-symbol round trips"
    );
}

#[test]
fn catalog_candidate_assembly_is_the_explicit_no_override_path() {
    let id = InstrumentId::from("005930.XKRX");
    let bar_type = BarKind::Daily.bar_type(id).unwrap();
    let session_day = 20u32;
    let bars: Vec<Bar> = (1..=session_day)
        .map(|day_of_month| {
            day(
                bar_type,
                (2024, 1, day_of_month),
                100 + day_of_month as i64,
                110 + day_of_month as i64,
            )
        })
        .collect();
    let mut daily_by_inst = HashMap::new();
    daily_by_inst.insert(id, bars.iter().collect());
    let mut opening_volumes = BTreeMap::new();
    for day_of_month in 1..session_day {
        opening_volumes.insert(
            NaiveDate::from_ymd_opt(2024, 1, day_of_month).unwrap(),
            1_000.0 + day_of_month as f64,
        );
    }
    let mut open_vol = HashMap::new();
    open_vol.insert(id, opening_volumes);
    let params = OrbParams::default();
    let instruments = vec![sample_instrument(id)];
    let session = NaiveDate::from_ymd_opt(2024, 1, session_day).unwrap();

    let catalog = build_candidates(
        &instruments,
        &daily_by_inst,
        &open_vol,
        &params,
        session,
        None,
    );
    let explicit = build_candidates_with_today_open(
        &instruments,
        &daily_by_inst,
        &open_vol,
        &params,
        session,
        None,
        None,
    );

    assert_eq!(
        catalog, explicit,
        "both ORB consumers execute one candidate assembly path"
    );
    assert_eq!(catalog.len(), 1);
    assert!(catalog[0].prior_atr.is_some());
    assert!(catalog[0].prior_illiq.is_some());
    assert!(catalog[0].prior_open_vol_mean.is_some());
    assert_eq!(catalog[0].gap_prices, SessionGapPrices::new(129, 120));
}
