//! Adapter-owned KRX rule data: tick-size bands (both regimes) + session times.
//!
//! No LS TR carries the tick-size band table or the trading-session clock (R1,
//! verified absent from the t8430 baseline), so the adapter owns them as versioned
//! constants. KRX revised its stock tick-size ladder effective **2023-01-25**
//! ([`TICK_REFORM_DATE`]); because daily history spans both regimes (KTD7), both
//! tables ship and the effective-date switch ([`TickRegime::for_date`]) selects
//! between them per bar date.
//!
//! KRX also extended the regular-session **close** by thirty minutes, effective
//! Monday **2016-08-01** ([`CLOSE_REFORM_DATE`]): 15:00 → 15:30. The same
//! template applies — a dated const, a regime enum ([`SessionRegime::for_date`]),
//! and the regime threaded to consumers as a parameter ([`regular_close`]), never
//! read ambiently. Measured against the committed calendar snapshot
//! (`state/krx.calendar.json`, coverage 2010-01-04..): **1,629** proven trading
//! sessions fall below the reform, and a flat 15:30 constant fabricates the close
//! for every one of them (R13/KTD15).
//!
//! R13's second clause is a SEPARATE surface and is deliberately not closed here:
//! the same snapshot carries **three** pre-reform `unknown` days —
//! **2010-06-02**, **2011-12-30**, **2015-08-14**. Accumulate-forward stops
//! before the first `unknown` and never crosses it, so a backfill from
//! 2010-01-04 halts at 2010-06-02 and reaches ~100 of those 1,629 sessions.
//! Resolving them needs a credentialed KRX witness probe per day, not a code
//! change, and `unknown` is never read as `closed`. Tracked in the work queue.
//!
//! The date-sensitive rule data in this module, and whether it is effective-dated:
//!
//! | Datum | Status |
//! |---|---|
//! | the three band tables | **dated** via [`TickRegime`] (2023-01-25) |
//! | the regular close | **dated** via [`SessionRegime`] (2016-08-01) |
//! | [`KRX_REGULAR_OPEN`] | 09:00, never moved — deliberately not dated |
//! | [`KST_UTC_OFFSET_HOURS`] | Korea ran DST in 1987–88 and UTC+08:30 earlier still, but is invariant across the 2010+ data range — deliberately not dated |
//! | [`Market::from_gubun`] | the `"1"`/`"2"` mapping and its KOSPI default would only become date-sensitive under a t8430 field-code semantics change — not dated |
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

/// The KRX regular-session close extension effective date (Monday 2016-08-01).
///
/// PROVENANCE, because this date is now load-bearing for catalog bytes: it rests
/// on agreeing SECONDARY sources, not a primary KRX notice — the 2016 press
/// release and the rulebook revision were not publicly reachable. Recorded as
/// PARTIALLY SETTLED in the effective-date convention under `docs/solutions/`
/// and in the Part A findings' in-window regime table. If a primary source ever
/// contradicts it, every bar stamped below the date is re-derived.
/// Sessions dated on or after this use [`SessionRegime::Post2016`] and close at
/// [`KRX_REGULAR_CLOSE`]; earlier sessions use [`SessionRegime::Pre2016`] and
/// close at [`KRX_REGULAR_CLOSE_PRE_2016`].
pub const CLOSE_REFORM_DATE: NaiveDate = match NaiveDate::from_ymd_opt(2016, 8, 1) {
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

/// KRX regular-session close (15:30 KST) — the [`SessionRegime::Post2016`] close,
/// in force since [`CLOSE_REFORM_DATE`].
///
/// This is **not** a session-independent constant: sessions dated before
/// [`CLOSE_REFORM_DATE`] closed at [`KRX_REGULAR_CLOSE_PRE_2016`]. Wherever a
/// session date is in scope, resolve through [`regular_close`] with
/// [`SessionRegime::for_date`] instead of reading this — a flat read stamps every
/// pre-2016 session at a close that did not exist yet (R13/KTD15). Reading it
/// directly is correct only where the date is known to be on/after the reform.
pub const KRX_REGULAR_CLOSE: NaiveTime = match NaiveTime::from_hms_opt(15, 30, 0) {
    Some(t) => t,
    None => unreachable!(),
};

/// KRX regular-session close before [`CLOSE_REFORM_DATE`] (15:00 KST) — the
/// [`SessionRegime::Pre2016`] close.
pub const KRX_REGULAR_CLOSE_PRE_2016: NaiveTime = match NaiveTime::from_hms_opt(15, 0, 0) {
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

/// Which regular-session clock applies to a given session date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRegime {
    /// Before 2016-08-01 — the regular session closed at 15:00 KST.
    Pre2016,
    /// On/after 2016-08-01 — the thirty-minute close extension (15:30 KST).
    Post2016,
}

impl SessionRegime {
    /// Select the regime for a session/bar date.
    pub fn for_date(date: NaiveDate) -> Self {
        if date >= CLOSE_REFORM_DATE {
            SessionRegime::Post2016
        } else {
            SessionRegime::Pre2016
        }
    }
}

/// Return the KRX regular-session close for the given session regime.
///
/// The regime is a **parameter**, exactly as [`TickRegime`] is to [`tick_size`]:
/// callers construct it at the call site from the session date already in scope
/// (`regular_close(SessionRegime::for_date(date))`) rather than reading a flat
/// constant that is only true for modern sessions (KTD15).
pub fn regular_close(regime: SessionRegime) -> NaiveTime {
    match regime {
        SessionRegime::Pre2016 => KRX_REGULAR_CLOSE_PRE_2016,
        SessionRegime::Post2016 => KRX_REGULAR_CLOSE,
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
    fn close_regime_switch_at_the_2016_close_extension() {
        // The last business day before the extension, the calendar day immediately
        // before it, and the day itself.
        let fri = NaiveDate::from_ymd_opt(2016, 7, 29).unwrap();
        let sun = NaiveDate::from_ymd_opt(2016, 7, 31).unwrap();
        let mon = NaiveDate::from_ymd_opt(2016, 8, 1).unwrap();
        assert_eq!(CLOSE_REFORM_DATE, mon);
        assert_eq!(SessionRegime::for_date(fri), SessionRegime::Pre2016);
        assert_eq!(SessionRegime::for_date(sun), SessionRegime::Pre2016);
        assert_eq!(SessionRegime::for_date(mon), SessionRegime::Post2016);
        // The observable consequence, not just the enum: the close instant moves.
        let at_15 = NaiveTime::from_hms_opt(15, 0, 0).unwrap();
        let at_1530 = NaiveTime::from_hms_opt(15, 30, 0).unwrap();
        assert_eq!(regular_close(SessionRegime::for_date(fri)), at_15);
        assert_eq!(regular_close(SessionRegime::for_date(sun)), at_15);
        assert_eq!(regular_close(SessionRegime::for_date(mon)), at_1530);
    }

    #[test]
    fn the_session_open_never_moved_across_the_close_extension() {
        // The 2016 reform extended the CLOSE only; the open is regime-independent
        // by construction (there is no regime-keyed open to read), so a pre-2016
        // session opens at exactly the same 09:00 a modern one does.
        assert_eq!(KRX_REGULAR_OPEN, NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        assert!(
            KRX_REGULAR_OPEN < regular_close(SessionRegime::Pre2016),
            "a pre-2016 session is still a full trading day"
        );
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
        // The post-extension close still equals the previously asserted constant —
        // the flat const was never wrong for modern sessions, only for old ones.
        assert_eq!(KRX_REGULAR_CLOSE, NaiveTime::from_hms_opt(15, 30, 0).unwrap());
        assert_eq!(KRX_REGULAR_CLOSE, regular_close(SessionRegime::Post2016));
        assert_eq!(KRX_REGULAR_CLOSE_PRE_2016, NaiveTime::from_hms_opt(15, 0, 0).unwrap());
        assert_eq!(KRX_REGULAR_CLOSE_PRE_2016, regular_close(SessionRegime::Pre2016));
        assert_eq!(KST_UTC_OFFSET_HOURS, 9);
    }
}
