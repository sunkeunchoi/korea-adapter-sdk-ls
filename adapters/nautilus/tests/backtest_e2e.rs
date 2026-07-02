//! U4 — the backtest end-to-end proof (Success Criterion 1).
//!
//! One offline test: wiremock serves fixture masters + a few symbols' daily bars →
//! the real ingest core writes them to a temp `ParquetDataCatalog` → a minimal
//! placeholder opening-range-breakout strategy runs in a `BacktestEngine` loading
//! instruments and bars **from that catalog** → we assert the data path (instrument
//! count, bar count, `ts_event` monotonic, ≥1 simulated order). The strategy is
//! throwaway test scaffolding, not a deliverable (scope boundary).

use std::fmt::Debug;

use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_backtest::config::{BacktestEngineConfig, SimulatedVenueConfig};
use nautilus_backtest::engine::BacktestEngine;
use nautilus_backtest::result::BacktestResult;
use nautilus_common::actor::DataActor;
use nautilus_ls::ingest::{BarKind, IngestConfig, Ingestor};
use nautilus_ls::instruments::{InstrumentDomain, InstrumentProvider};
use nautilus_model::data::{Bar, BarType, Data};
use nautilus_model::enums::{AccountType, BookType, OmsType, OrderSide};
use nautilus_model::identifiers::{InstrumentId, StrategyId, Venue};
use nautilus_model::instruments::{Instrument, InstrumentAny};
use nautilus_model::types::{Currency, Money, Price, Quantity};
use nautilus_trading::nautilus_strategy;
use nautilus_trading::strategy::{Strategy, StrategyConfig, StrategyCore};
use tempfile::tempdir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ---------------------------------------------------------------------------
// Placeholder opening-range-breakout strategy (throwaway test scaffolding). It
// records the first bar's high as the opening range and submits a single market
// buy on the first subsequent bar that breaks it — enough to prove an order is
// simulated on LS-ingested bars.
// ---------------------------------------------------------------------------

struct OpeningRangeBreakout {
    core: StrategyCore,
    bar_type: BarType,
    instrument_id: InstrumentId,
    trade_size: Quantity,
    opening_high: Option<Price>,
    entered: bool,
}

impl OpeningRangeBreakout {
    fn new(bar_type: BarType, trade_size: Quantity) -> Self {
        let instrument_id = bar_type.instrument_id();
        let base = StrategyConfig {
            strategy_id: Some(StrategyId::from("ORB-001")),
            ..Default::default()
        };
        OpeningRangeBreakout {
            core: StrategyCore::new(base),
            bar_type,
            instrument_id,
            trade_size,
            opening_high: None,
            entered: false,
        }
    }

    fn enter(&mut self) -> anyhow::Result<()> {
        let order = self.order().market(
            self.instrument_id,
            OrderSide::Buy,
            self.trade_size,
            None, None, None, None, None, None, None,
        );
        self.submit_order(order, None, None, None)
    }
}

nautilus_strategy!(OpeningRangeBreakout);

impl Debug for OpeningRangeBreakout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpeningRangeBreakout")
            .field("instrument_id", &self.instrument_id)
            .field("entered", &self.entered)
            .finish()
    }
}

impl DataActor for OpeningRangeBreakout {
    fn on_start(&mut self) -> anyhow::Result<()> {
        self.subscribe_bars(self.bar_type, None, None);
        Ok(())
    }

    fn on_bar(&mut self, bar: &Bar) -> anyhow::Result<()> {
        match self.opening_high {
            None => self.opening_high = Some(bar.high),
            Some(range_high) => {
                if !self.entered && bar.high > range_high {
                    self.entered = true;
                    self.enter()?;
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Fixtures + ingest
// ---------------------------------------------------------------------------

fn json_response(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_string(body.to_string())
        .insert_header("content-type", "application/json")
}

fn t8430_body() -> serde_json::Value {
    serde_json::json!({
        "rsp_cd": "00000",
        "t8430OutBlock": [
            { "hname": "삼성전자", "shcode": "005930", "expcode": "KR7005930003",
              "etfgubun": "0", "uplmtprice": "78000", "dnlmtprice": "42000",
              "jnilclose": "60000", "memedan": "1", "recprice": "60000", "gubun": "1" }
        ]
    })
}

fn t9945_body() -> serde_json::Value {
    serde_json::json!({
        "rsp_cd": "00000",
        "t9945OutBlock": [
            { "hname": "삼성전자", "shcode": "005930", "expcode": "KR7005930003",
              "etfchk": "0", "nxt_chk": "1", "filler": "" }
        ]
    })
}

/// Daily bars with strictly rising highs so the ORB breakout triggers.
fn t8410_body() -> serde_json::Value {
    serde_json::json!({
        "rsp_cd": "00000",
        "t8410OutBlock": { "shcode": "005930", "cts_date": "", "rec_count": "4" },
        "t8410OutBlock1": [
            { "date": "20240102", "open": "60000", "high": "61000", "low": "59500", "close": "60800", "jdiff_vol": "1000000" },
            { "date": "20240103", "open": "60800", "high": "62000", "low": "60500", "close": "61900", "jdiff_vol": "1100000" },
            { "date": "20240104", "open": "61900", "high": "63000", "low": "61500", "close": "62800", "jdiff_vol": "1200000" },
            { "date": "20240105", "open": "62800", "high": "64000", "low": "62500", "close": "63500", "jdiff_vol": "1300000" }
        ]
    })
}

async fn build_catalog() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempdir().unwrap();
    let catalog_path = dir.path().join("catalog");
    let server = MockServer::start().await;
    mount_token(&server).await;
    for (p, tr, body) in [
        ("/stock/etc", "t8430", t8430_body()),
        ("/stock/market-data", "t9945", t9945_body()),
        ("/stock/chart", "t8410", t8410_body()),
    ] {
        Mock::given(method("POST"))
            .and(path(p))
            .and(header("tr_cd", tr))
            .respond_with(json_response(body))
            .mount(&server)
            .await;
    }

    let sdk = LsSdk::new(mock_config(&server.uri())).unwrap();
    let mut provider = InstrumentProvider::new(sdk.clone());
    provider
        .load_domain(InstrumentDomain::DomesticEquity)
        .await
        .unwrap();
    nautilus_ls::ingest::write_instruments(&catalog_path, provider.all_any())
        .await
        .unwrap();

    let config = IngestConfig {
        catalog_path: catalog_path.clone(),
        bar_kinds: vec![BarKind::Daily],
        sdate: "20240102".to_string(),
        edate: "20240105".to_string(),
        adjusted_prices: true,
    };
    let mut ingestor = Ingestor::new(sdk, config);
    ingestor
        .run(&[InstrumentId::from("005930.XKRX")])
        .await
        .unwrap();

    (dir, catalog_path)
}

/// Build + run a backtest over catalog-loaded instruments and bars, returning the
/// result. Runs inside `spawn_blocking` because the engine drives an internal
/// runtime (`block_on`).
fn run_backtest(instruments: Vec<InstrumentAny>, bars: Vec<Bar>) -> BacktestResult {
    let mut engine = BacktestEngine::new(BacktestEngineConfig {
        bypass_logging: true,
        ..Default::default()
    })
    .unwrap();

    engine
        .add_venue(
            SimulatedVenueConfig::builder()
                .venue(Venue::from(nautilus_ls::KRX_VENUE))
                .oms_type(OmsType::Netting)
                .account_type(AccountType::Margin)
                .base_currency(Currency::KRW())
                .book_type(BookType::L1_MBP)
                .starting_balances(vec![Money::new(100_000_000.0, Currency::KRW())])
                .build()
                .unwrap(),
        )
        .unwrap();

    for inst in &instruments {
        engine.add_instrument(inst).unwrap();
    }

    let bar_type = bars[0].bar_type;
    engine
        .add_strategy(OpeningRangeBreakout::new(bar_type, Quantity::from(10)))
        .unwrap();

    let data: Vec<Data> = bars.into_iter().map(Data::Bar).collect();
    engine.add_data(data, None, true, true).unwrap();
    engine.run(None, None, None, false).unwrap();
    engine.get_result()
}

#[tokio::test]
async fn backtest_runs_end_to_end_on_ls_ingested_data() {
    let (_dir, catalog_path) = build_catalog().await;

    // Load instruments + bars FROM the catalog (F1: the backtest sources both from
    // LS-ingested data only).
    let instruments = nautilus_ls::ingest::read_all_instruments(&catalog_path)
        .await
        .unwrap();
    let bars = nautilus_ls::ingest::read_all_bars(&catalog_path).await.unwrap();
    assert_eq!(instruments.len(), 1, "one instrument ingested");
    assert_eq!(bars.len(), 4, "four daily candles ingested");
    for w in bars.windows(2) {
        assert!(
            w[0].ts_event.as_u64() <= w[1].ts_event.as_u64(),
            "bars monotonic by ts_event"
        );
    }

    let result = tokio::task::spawn_blocking(move || run_backtest(instruments, bars))
        .await
        .unwrap();

    assert!(
        result.total_orders >= 1,
        "the ORB strategy simulated at least one order (got {})",
        result.total_orders
    );
    assert_eq!(result.iterations, 4, "the engine processed all four bars");
}

#[tokio::test]
async fn re_running_the_backtest_is_deterministic() {
    let (_dir, catalog_path) = build_catalog().await;
    let instruments = nautilus_ls::ingest::read_all_instruments(&catalog_path)
        .await
        .unwrap();
    let bars = nautilus_ls::ingest::read_all_bars(&catalog_path).await.unwrap();

    let (i1, b1) = (instruments.clone(), bars.clone());
    let first = tokio::task::spawn_blocking(move || run_backtest(i1, b1))
        .await
        .unwrap();
    let second = tokio::task::spawn_blocking(move || run_backtest(instruments, bars))
        .await
        .unwrap();

    assert_eq!(
        first.total_orders, second.total_orders,
        "same catalog ⇒ same simulated order count"
    );
    assert_eq!(first.iterations, second.iterations);
}

/// KTD7: daily price limits are NOT baked into the instrument, so an order priced
/// outside today's band is representable, and price stepping routes through the
/// rules band lookup across a tick-band boundary.
#[tokio::test]
async fn instrument_omits_daily_limits_and_steps_via_rules() {
    let (_dir, catalog_path) = build_catalog().await;
    let instruments = nautilus_ls::ingest::read_all_instruments(&catalog_path)
        .await
        .unwrap();
    // The loaded instrument carries no frozen daily band.
    let inst = &instruments[0];
    assert!(inst.max_price().is_none(), "no max_price baked in (KTD7)");
    assert!(inst.min_price().is_none(), "no min_price baked in (KTD7)");

    // A price well above today's upper limit (78,000) steps per the rules band
    // lookup (crossing the 50k–200k / 200k–500k boundary), not the instrument.
    use nautilus_ls::rules::{round_down_to_tick, Market, TickRegime};
    let stepped = round_down_to_tick(Market::Kospi, TickRegime::Post2023, 201_234).unwrap();
    assert_eq!(stepped, 201_000, "200k+ band steps by 500 KRW");
}
