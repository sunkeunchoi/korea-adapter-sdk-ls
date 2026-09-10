//! The historical price-grid re-grid.
//!
//! Found by the R30 diagnostic probe (2026-09-08). nautilus's matching engine declines
//! a fill whose price is not a multiple of the instrument's `price_increment` — a WARN
//! on a run that exits 0 with a clean data-quality report. Over 2016-08-01..2019-12-31
//! that skipped 7,981 fills across 136 of 286 symbols and left 5,167 entry orders
//! unopened, so `target_m = 8` delivered 1.57 entries per session.
//!
//! Two things make the adapter's increment unusable for a historical run, and the
//! second is the one that decides the fix. It is derived from TODAY's reference price
//! under a hardcoded `TickRegime::Post2023`, so it is the wrong band for a 2016 price;
//! and the catalog is adjustment-adjusted, so its prices sit on no exchange tick grid
//! AT ALL — `005930`'s 2016 close reads 30,340, its pre-split price through the 2018
//! 50:1 split, and 30,340 is not a multiple of the 50 KRW tick that governed it. An
//! effective-dated ladder lookup would still refuse three of every five real prices.
//!
//! So the run mounts `gcd(price_gcd, adapter_increment)` per symbol: it divides every
//! in-range price by construction, is never coarser than the adapter's value, and
//! recovers the true exchange tick for a symbol no corporate action ever touched.

use std::collections::HashMap;

use chrono::NaiveDate;
use ls_sdk::paginated::T8410OutBlock1;
use nautilus_core::UnixNanos;
use nautilus_ls::ingest::{build_daily_bar, kst_to_unix_nanos, BarKind};
use nautilus_ls::instruments::map_equity;
use nautilus_ls_lab::runner::backtest_daily::regrid_instruments_for_range;
use nautilus_model::data::Bar;
use nautilus_model::identifiers::InstrumentId;
use nautilus_model::instruments::{Instrument, InstrumentAny};

use super::fixture::daily_json;

/// A KRX equity master row whose `recprice` puts it in whichever *2026* tick band the
/// caller wants — the input that produces today's static `price_increment`.
fn equity_at(shcode: &str, recprice: i64) -> InstrumentAny {
    let row: ls_sdk::market_session::T8430OutBlock = serde_json::from_value(serde_json::json!({
        "hname": "테스트", "shcode": shcode, "expcode": format!("KR7{shcode}003"),
        "etfgubun": "0", "uplmtprice": "0", "dnlmtprice": "0",
        "jnilclose": recprice.to_string(), "memedan": "1",
        "recprice": recprice.to_string(), "gubun": "1",
    }))
    .unwrap();
    InstrumentAny::Equity(map_equity(&row, None, UnixNanos::default()).unwrap())
}

/// One daily bar at a flat OHLC price on a KST session date.
fn flat_bar(shcode: &str, date: &str, price: i64) -> Bar {
    let id = InstrumentId::from(format!("{shcode}.XKRX").as_str());
    let bt = BarKind::Daily.bar_type(id).unwrap();
    let p = price.to_string();
    let row: T8410OutBlock1 =
        serde_json::from_value(daily_json(date, &p, &p, &p, &p, "1000000")).unwrap();
    build_daily_bar(bt, &row).unwrap().unwrap()
}

/// The whole-range window the re-grid is scoped to.
fn range(start: &str, end: &str) -> (u64, u64) {
    let d = |s: &str| NaiveDate::parse_from_str(s, "%Y%m%d").unwrap();
    let midnight = NaiveDate::MIN.and_hms_opt(0, 0, 0).unwrap().time();
    let eod = NaiveDate::MIN.and_hms_opt(23, 59, 59).unwrap().time();
    (
        kst_to_unix_nanos(d(start), midnight).unwrap().as_u64(),
        kst_to_unix_nanos(d(end), eod).unwrap().as_u64(),
    )
}

fn increment_of(instruments: &[InstrumentAny], shcode: &str) -> i64 {
    instruments
        .iter()
        .find(|i| i.id().symbol.as_str() == shcode)
        .expect("instrument present")
        .price_increment()
        .as_f64() as i64
}

/// Assert the property the matching engine actually checks, for every in-range bar.
/// This is the constructive guarantee the fix rests on, so every scenario re-asserts it
/// rather than trusting the increment it computed.
fn assert_no_fill_can_be_skipped(regridded: &[InstrumentAny], bars: &[Bar]) {
    let by_id: HashMap<_, _> = regridded
        .iter()
        .map(|i| (i.id(), i.price_increment().as_f64() as i64))
        .collect();
    for b in bars {
        let Some(&inc) = by_id.get(&b.bar_type.instrument_id()) else { continue };
        for p in [b.open, b.high, b.low, b.close] {
            let v = p.as_f64() as i64;
            assert_eq!(
                v % inc,
                0,
                "{} price {v} is off a {inc} grid — the engine would skip this fill",
                b.bar_type.instrument_id()
            );
        }
    }
}

/// The unadjusted half of the defect: a 2026 reference price of 630,000 gives `000660`
/// today's 1,000 KRW tick, but its 2016 bars traded at 33,550 on a 50 KRW grid. No
/// corporate action touched these prices, so the re-grid should recover the real
/// exchange tick rather than collapsing to 1.
#[test]
fn recovers_the_real_exchange_tick_for_a_symbol_no_action_touched() {
    let instruments = vec![equity_at("000660", 630_000)];
    let bars =
        vec![flat_bar("000660", "20160803", 33_550), flat_bar("000660", "20170103", 52_400)];
    let (start, end) = range("20160801", "20191231");

    let (regridded, rows) = regrid_instruments_for_range(&instruments, &bars, start, end);

    assert_eq!(increment_of(&instruments, "000660"), 1_000, "today's tick, unchanged input");
    assert_eq!(increment_of(&regridded, "000660"), 50, "the grid its own history printed on");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].from_increment, 1_000);
    assert_eq!(rows[0].to_increment, 50);
    assert_eq!(rows[0].instrument_id, InstrumentId::from("000660.XKRX"));
    assert_no_fill_can_be_skipped(&regridded, &bars);
}

/// The adjusted half, and the reason an effective-dated ladder lookup would not have
/// been enough: `005930`'s adjusted 2016 close of 30,340 is on NO KRX tick — the 50 KRW
/// tick that governed it does not divide it — because the 2018 50:1 split was applied
/// to the whole history. The re-grid admits it anyway.
#[test]
fn admits_an_adjusted_price_that_sits_on_no_exchange_tick() {
    // 30,340 % 50 == 40: the pre-2023 KOSPI tick for this band cannot describe it.
    assert_ne!(30_340 % 50, 0, "the premise: this adjusted price is off the exchange grid");

    let instruments = vec![equity_at("005930", 78_000)];
    let bars =
        vec![flat_bar("005930", "20160803", 30_340), flat_bar("005930", "20180102", 51_020)];
    let (start, end) = range("20160801", "20191231");

    let (regridded, rows) = regrid_instruments_for_range(&instruments, &bars, start, end);

    assert_eq!(increment_of(&instruments, "005930"), 100, "today's tick refuses 30,340");
    assert_eq!(increment_of(&regridded, "005930"), 20, "gcd(20, 100) — divides both prices");
    assert_eq!(rows.len(), 1);
    assert_no_fill_can_be_skipped(&regridded, &bars);
}

/// A mixed book — one adjusted symbol, one untouched — is handled per symbol, and the
/// no-skip property holds across the whole set.
#[test]
fn a_mixed_adjusted_and_untouched_book_holds_the_no_skip_property() {
    let instruments = vec![equity_at("005930", 78_000), equity_at("068270", 420_000)];
    let bars = vec![
        flat_bar("005930", "20160803", 30_340),
        flat_bar("005930", "20180102", 51_020),
        flat_bar("068270", "20160803", 92_700),
        flat_bar("068270", "20190102", 216_500),
    ];
    let (start, end) = range("20160801", "20191231");

    let (regridded, rows) = regrid_instruments_for_range(&instruments, &bars, start, end);

    assert_eq!(rows.len(), 2, "both symbols carry a 2026 tick their history is off");
    assert_eq!(increment_of(&regridded, "068270"), 100, "gcd(100, 500)");
    assert_no_fill_can_be_skipped(&regridded, &bars);
}

/// The re-grid only ever loosens. An adapter increment that already divides every
/// in-range price is kept exactly, and produces no diagnostic row — so the row count
/// means what it says and a sparse symbol cannot invent an absurdly coarse grid from
/// one bar (`gcd(1234, 1) == 1`, not 1,234).
#[test]
fn an_increment_that_already_divides_every_price_is_kept() {
    let instruments = vec![equity_at("000001", 1_500)];
    let bars = vec![flat_bar("000001", "20160803", 1_234)];
    let (start, end) = range("20160801", "20191231");

    let (regridded, rows) = regrid_instruments_for_range(&instruments, &bars, start, end);

    assert_eq!(increment_of(&instruments, "000001"), 1, "the 2026 tick is already 1 KRW");
    assert_eq!(increment_of(&regridded, "000001"), 1, "not widened to the single price");
    assert!(rows.is_empty(), "no move, no row");
    assert_no_fill_can_be_skipped(&regridded, &bars);
}

/// Out-of-range bars do not move the increment — the engine never sees them — but the
/// in-range bar still gets a grid that admits it.
#[test]
fn out_of_range_bars_do_not_move_the_grid() {
    let instruments = vec![equity_at("000003", 630_000)];
    let bars = vec![
        flat_bar("000003", "20150102", 1_237), // out of range: would force a 1 KRW grid
        flat_bar("000003", "20160803", 33_550), // in range
    ];
    let (start, end) = range("20160801", "20191231");

    let (regridded, _) = regrid_instruments_for_range(&instruments, &bars, start, end);

    assert_eq!(increment_of(&regridded, "000003"), 50, "gcd(33550, 1000); 1,237 ignored");
    assert_eq!(33_550 % 50, 0);
}

/// A symbol with no in-range bars is passed through untouched. There is no evidence to
/// re-grid it from and it trades nothing, so defaulting it to a 1 KRW grid would only
/// weaken the increment mounted for a symbol that never fills.
#[test]
fn a_symbol_with_no_in_range_bars_is_left_alone() {
    let instruments = vec![equity_at("000005", 630_000)];
    let bars = vec![flat_bar("000005", "20150102", 33_550)];
    let (start, end) = range("20160801", "20191231");

    let (regridded, rows) = regrid_instruments_for_range(&instruments, &bars, start, end);

    assert_eq!(increment_of(&regridded, "000005"), 1_000, "untouched");
    assert!(rows.is_empty());
}

/// Re-gridding is idempotent: feeding the output back in moves nothing and reports
/// nothing, so a caller cannot ratchet the grid finer by applying it twice.
#[test]
fn regridding_twice_changes_nothing() {
    let instruments = vec![equity_at("000660", 630_000), equity_at("005930", 78_000)];
    let bars = vec![
        flat_bar("000660", "20160803", 33_550),
        flat_bar("005930", "20160803", 30_340),
    ];
    let (start, end) = range("20160801", "20191231");

    let (once, first_rows) = regrid_instruments_for_range(&instruments, &bars, start, end);
    let (twice, second_rows) = regrid_instruments_for_range(&once, &bars, start, end);

    assert_eq!(first_rows.len(), 2);
    assert!(second_rows.is_empty(), "already on the grid");
    assert_eq!(increment_of(&once, "000660"), increment_of(&twice, "000660"));
    assert_eq!(increment_of(&once, "005930"), increment_of(&twice, "005930"));
}
