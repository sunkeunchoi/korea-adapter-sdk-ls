//! Adapter-owned KRX rule data: tick-size bands (both regimes) + session times.
//!
//! No LS TR carries the tick-size band table or the trading-session clock (R1,
//! verified absent from the t8430 baseline), so the adapter owns them as versioned
//! constants. KRX revised its stock tick-size ladder effective **2023-01-25**
//! ([`TICK_REFORM_DATE`]); because daily history spans both regimes (KTD7), both
//! tables ship and the effective-date switch ([`TickRegime::for_date`]) selects
//! between them per bar date.
//!
//! Band semantics: a band covers the half-open interval `[lower, upper)`. A price
//! exactly on a boundary belongs to the **higher** band (e.g. 50,000 KRW → the
//! 50,000–200,000 band). Prices are integer KRW (`price_precision = 0`, KTD7), so
//! ticks are integers.

use chrono::{NaiveDate, NaiveTime};

use crate::error::AdapterError;

/// The KRX stock tick-size reform effective date (2023-01-25). Bars dated on or
/// after this use [`TickRegime::Post2023`]; earlier bars use [`TickRegime::Pre2023`].
pub const TICK_REFORM_DATE: NaiveDate = match NaiveDate::from_ymd_opt(2023, 1, 25) {
    Some(d) => d,
    None => unreachable!(),
};

/// KST is UTC+09:00 with no daylight saving. The offset used to convert LS
/// wall-clock strings to UTC (KTD9).
pub const KST_UTC_OFFSET_HOURS: i32 = 9;

/// KRX regular-session open (09:00 KST).
pub const KRX_REGULAR_OPEN: NaiveTime = match NaiveTime::from_hms_opt(9, 0, 0) {
    Some(t) => t,
    None => unreachable!(),
};

/// KRX regular-session close (15:30 KST).
pub const KRX_REGULAR_CLOSE: NaiveTime = match NaiveTime::from_hms_opt(15, 30, 0) {
    Some(t) => t,
    None => unreachable!(),
};

/// A KRX market segment. Tick ladders differed between KOSPI and KOSDAQ **before**
/// the 2023 reform (KOSDAQ capped its tick at 100 KRW); the reform unified them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Market {
    /// KOSPI (유가증권시장) — t8430/t9945 `gubun == "1"`.
    Kospi,
    /// KOSDAQ (코스닥) — t8430/t9945 `gubun == "2"`.
    Kosdaq,
}

impl Market {
    /// Resolve a market from a t8430/t9945 `gubun` code (`"1"` KOSPI / `"2"` KOSDAQ).
    /// Anything else defaults to KOSPI (the reform table is identical across
    /// markets, so the fallback is only load-bearing for pre-2023 KOSDAQ history).
    pub fn from_gubun(gubun: &str) -> Self {
        match gubun.trim() {
            "2" => Market::Kosdaq,
            _ => Market::Kospi,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Market::Kospi => "KOSPI",
            Market::Kosdaq => "KOSDAQ",
        }
    }
}

/// Which tick-size regime applies to a given bar date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickRegime {
    /// Before 2023-01-25.
    Pre2023,
    /// On/after 2023-01-25 (the unified, finer ladder).
    Post2023,
}

impl TickRegime {
    /// Select the regime for a bar/quote date.
    pub fn for_date(date: NaiveDate) -> Self {
        if date >= TICK_REFORM_DATE {
            TickRegime::Post2023
        } else {
            TickRegime::Pre2023
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            TickRegime::Pre2023 => "pre_2023",
            TickRegime::Post2023 => "post_2023",
        }
    }
}

/// One tick band: prices in `[.., upper_exclusive)` step by `tick`. The final band
/// in a ladder has `upper_exclusive == None` (unbounded).
#[derive(Debug, Clone, Copy)]
struct Band {
    upper_exclusive: Option<i64>,
    tick: i64,
}

/// Post-2023 unified ladder (KOSPI == KOSDAQ), effective 2023-01-25.
const POST_2023: &[Band] = &[
    Band { upper_exclusive: Some(2_000), tick: 1 },
    Band { upper_exclusive: Some(5_000), tick: 5 },
    Band { upper_exclusive: Some(20_000), tick: 10 },
    Band { upper_exclusive: Some(50_000), tick: 50 },
    Band { upper_exclusive: Some(200_000), tick: 100 },
    Band { upper_exclusive: Some(500_000), tick: 500 },
    Band { upper_exclusive: None, tick: 1_000 },
];

/// Pre-2023 KOSPI ladder.
const PRE_2023_KOSPI: &[Band] = &[
    Band { upper_exclusive: Some(1_000), tick: 1 },
    Band { upper_exclusive: Some(5_000), tick: 5 },
    Band { upper_exclusive: Some(10_000), tick: 10 },
    Band { upper_exclusive: Some(50_000), tick: 50 },
    Band { upper_exclusive: Some(100_000), tick: 100 },
    Band { upper_exclusive: Some(500_000), tick: 500 },
    Band { upper_exclusive: None, tick: 1_000 },
];

/// Pre-2023 KOSDAQ ladder — capped at a 100 KRW tick above 50,000 (the historical
/// KOSDAQ divergence from KOSPI).
const PRE_2023_KOSDAQ: &[Band] = &[
    Band { upper_exclusive: Some(1_000), tick: 1 },
    Band { upper_exclusive: Some(5_000), tick: 5 },
    Band { upper_exclusive: Some(10_000), tick: 10 },
    Band { upper_exclusive: Some(50_000), tick: 50 },
    Band { upper_exclusive: None, tick: 100 },
];

fn ladder(market: Market, regime: TickRegime) -> &'static [Band] {
    match regime {
        TickRegime::Post2023 => POST_2023,
        TickRegime::Pre2023 => match market {
            Market::Kospi => PRE_2023_KOSPI,
            Market::Kosdaq => PRE_2023_KOSDAQ,
        },
    }
}

/// Return the KRX tick size (KRW) for `price` in the given market + regime.
///
/// `price` is integer KRW. A price on a band boundary belongs to the higher band.
///
/// # Errors
///
/// [`AdapterError::NoTickBand`] if `price` is negative (no band covers it). A
/// zero price resolves to the smallest tick (the first band).
pub fn tick_size(market: Market, regime: TickRegime, price: i64) -> Result<i64, AdapterError> {
    if price < 0 {
        return Err(AdapterError::NoTickBand {
            price,
            market: market.as_str().to_string(),
            regime: regime.as_str().to_string(),
        });
    }
    for band in ladder(market, regime) {
        match band.upper_exclusive {
            Some(upper) if price < upper => return Ok(band.tick),
            Some(_) => continue,
            None => return Ok(band.tick),
        }
    }
    // Unreachable: every ladder ends with an unbounded band.
    Err(AdapterError::NoTickBand {
        price,
        market: market.as_str().to_string(),
        regime: regime.as_str().to_string(),
    })
}

/// Round `price` DOWN to the nearest valid tick for its band (order-price
/// stepping, KTD7). Used where an order or backtest price must sit on the grid.
///
/// # Errors
///
/// Propagates [`tick_size`] errors.
pub fn round_down_to_tick(
    market: Market,
    regime: TickRegime,
    price: i64,
) -> Result<i64, AdapterError> {
    let tick = tick_size(market, regime, price)?;
    Ok(price - price.rem_euclid(tick))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_2023_bands_and_boundaries() {
        let r = TickRegime::Post2023;
        // Interior of each band.
        assert_eq!(tick_size(Market::Kospi, r, 1_500).unwrap(), 1);
        assert_eq!(tick_size(Market::Kospi, r, 3_000).unwrap(), 5);
        assert_eq!(tick_size(Market::Kospi, r, 12_000).unwrap(), 10);
        assert_eq!(tick_size(Market::Kospi, r, 30_000).unwrap(), 50);
        assert_eq!(tick_size(Market::Kospi, r, 60_000).unwrap(), 100);
        assert_eq!(tick_size(Market::Kospi, r, 300_000).unwrap(), 500);
        assert_eq!(tick_size(Market::Kospi, r, 1_000_000).unwrap(), 1_000);
        // Exact boundaries belong to the HIGHER band.
        assert_eq!(tick_size(Market::Kospi, r, 2_000).unwrap(), 5);
        assert_eq!(tick_size(Market::Kospi, r, 5_000).unwrap(), 10);
        assert_eq!(tick_size(Market::Kospi, r, 20_000).unwrap(), 50);
        assert_eq!(tick_size(Market::Kospi, r, 50_000).unwrap(), 100);
        assert_eq!(tick_size(Market::Kospi, r, 200_000).unwrap(), 500);
        assert_eq!(tick_size(Market::Kospi, r, 500_000).unwrap(), 1_000);
        // Post-2023 KOSPI == KOSDAQ.
        assert_eq!(tick_size(Market::Kosdaq, r, 60_000).unwrap(), 100);
    }

    #[test]
    fn pre_2023_kospi_vs_kosdaq_divergence() {
        let r = TickRegime::Pre2023;
        // KOSPI keeps stepping up above 50k; KOSDAQ caps at 100.
        assert_eq!(tick_size(Market::Kospi, r, 60_000).unwrap(), 100);
        assert_eq!(tick_size(Market::Kospi, r, 120_000).unwrap(), 500);
        assert_eq!(tick_size(Market::Kospi, r, 600_000).unwrap(), 1_000);
        assert_eq!(tick_size(Market::Kosdaq, r, 60_000).unwrap(), 100);
        assert_eq!(tick_size(Market::Kosdaq, r, 600_000).unwrap(), 100);
        // Boundary at 5,000 → 10 in both.
        assert_eq!(tick_size(Market::Kospi, r, 5_000).unwrap(), 10);
        assert_eq!(tick_size(Market::Kosdaq, r, 5_000).unwrap(), 10);
    }

    #[test]
    fn regime_switch_at_reform_date() {
        let before = NaiveDate::from_ymd_opt(2023, 1, 24).unwrap();
        let on = NaiveDate::from_ymd_opt(2023, 1, 25).unwrap();
        assert_eq!(TickRegime::for_date(before), TickRegime::Pre2023);
        assert_eq!(TickRegime::for_date(on), TickRegime::Post2023);
        // A 12,000 KRW KOSPI stock: pre-reform tick 50 (10k-50k band), post 10 (5k-20k).
        assert_eq!(tick_size(Market::Kospi, TickRegime::for_date(before), 12_000).unwrap(), 50);
        assert_eq!(tick_size(Market::Kospi, TickRegime::for_date(on), 12_000).unwrap(), 10);
    }

    #[test]
    fn negative_price_errors_not_panics() {
        let err = tick_size(Market::Kospi, TickRegime::Post2023, -1).unwrap_err();
        assert!(matches!(err, AdapterError::NoTickBand { .. }));
    }

    #[test]
    fn round_down_snaps_to_grid() {
        // 60,123 KRW KOSPI post-2023 → tick 100 → snap to 60,100.
        assert_eq!(
            round_down_to_tick(Market::Kospi, TickRegime::Post2023, 60_123).unwrap(),
            60_100
        );
        // Already on the grid stays put.
        assert_eq!(
            round_down_to_tick(Market::Kospi, TickRegime::Post2023, 60_100).unwrap(),
            60_100
        );
    }

    #[test]
    fn session_constants_are_regular_hours() {
        assert_eq!(KRX_REGULAR_OPEN, NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        assert_eq!(KRX_REGULAR_CLOSE, NaiveTime::from_hms_opt(15, 30, 0).unwrap());
        assert_eq!(KST_UTC_OFFSET_HOURS, 9);
    }
}
