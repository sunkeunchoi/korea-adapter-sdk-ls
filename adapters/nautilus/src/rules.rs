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
use nautilus_model::enums::OrderSide;

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

/// Round `price` UP to the nearest valid tick for its band — the buy-side mirror of
/// [`round_down_to_tick`].
///
/// The band is resolved at the ROUNDED-UP value, not at `price`, so a round-up that
/// crosses a band boundary lands on the coarser band's grid rather than on the finer
/// one it started in (every KRX boundary is itself a multiple of the higher band's
/// tick, so one re-resolution is enough — asserted by
/// `round_up_across_a_band_boundary_lands_on_the_higher_bands_grid`).
///
/// # Errors
///
/// Propagates [`tick_size`] errors.
pub fn round_up_to_tick(
    market: Market,
    regime: TickRegime,
    price: i64,
) -> Result<i64, AdapterError> {
    let tick = tick_size(market, regime, price)?;
    let rem = price.rem_euclid(tick);
    let up = if rem == 0 { price } else { price + (tick - rem) };
    // Re-resolve at the result: crossing into a coarser band can leave `up` off the
    // NEW band's grid, and an off-grid limit price is rejected by the exchange.
    let tick_up = tick_size(market, regime, up)?;
    let rem_up = up.rem_euclid(tick_up);
    Ok(if rem_up == 0 { up } else { up + (tick_up - rem_up) })
}

/// A session's daily price limits (상한가 / 하한가), integer KRW.
///
/// Session-scoped, never an instrument constant (KTD7) — the values ride on the
/// cached `Equity`'s `info` and are read out by
/// [`crate::instruments::daily_price_band`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceBand {
    /// 하한가 — the lowest price the exchange accepts this session.
    pub lower: i64,
    /// 상한가 — the highest price the exchange accepts this session.
    pub upper: i64,
}

/// Price a **marketable limit** order for the closing single-price auction (KTD5, R16).
///
/// The daily strategy emits the same MARKET order live that it emits in the backtest —
/// byte-identical strategy source is the whole point of the identity freeze — but the
/// LS order surface (`CSPAT00601`) is limit-only. This is the translation: take the
/// 15:20 decision-time price and step `k` ticks in the direction that CROSSES, so the
/// order is guaranteed to participate in the 15:30 auction.
///
/// **`k` is fill insurance, not a price decision.** In a single-price auction every
/// crossing limit executes at the one clearing price, so a buy limit 3 ticks above the
/// last trade fills at the close, not at the limit. Widening `k` buys protection
/// against the close moving away from 15:20; it does not make the fill worse.
///
/// - **Buy**: `last + k` ticks, rounded UP onto the grid (a rounded-DOWN buy could
///   land below the clearing price and miss the auction).
/// - **Sell**: `last - k` ticks, rounded DOWN onto the grid, for the mirror reason.
/// - Both are then clamped into `band`, and snapped back INWARD so a band edge that is
///   itself off-grid can never produce an off-grid order.
///
/// The tick step is resolved at `last` (the price the order is anchored to); the
/// rounding helpers re-resolve at their own result, so a `k`-tick step that crosses a
/// band boundary still lands on the grid.
///
/// # Errors
///
/// - [`AdapterError::Config`] if `side` is not a clean Buy/Sell (never defaulted — the
///   same fail-closed rule the submit path applies), if `last` is not positive, if
///   `k` is negative, or if `band` is inverted/non-positive.
/// - Propagates [`tick_size`] errors.
pub fn marketable_limit(
    market: Market,
    regime: TickRegime,
    last: i64,
    side: OrderSide,
    k: i64,
    band: PriceBand,
) -> Result<i64, AdapterError> {
    if last <= 0 {
        return Err(AdapterError::Config(format!(
            "marketable limit: last price must be positive, got {last} — refusing to price an \
             order off a missing/garbage mark"
        )));
    }
    if k < 0 {
        return Err(AdapterError::Config(format!(
            "marketable limit: tick offset k must be >= 0, got {k}"
        )));
    }
    if band.lower <= 0 || band.upper < band.lower {
        return Err(AdapterError::Config(format!(
            "marketable limit: invalid daily price band [{}, {}]",
            band.lower, band.upper
        )));
    }
    let tick = tick_size(market, regime, last)?;
    let stepped = match side {
        OrderSide::Buy => last.saturating_add(k.saturating_mul(tick)),
        OrderSide::Sell => last.saturating_sub(k.saturating_mul(tick)).max(1),
        other => {
            return Err(AdapterError::Config(format!(
                "marketable limit: unsupported order side {other:?} (Buy/Sell only) — refusing \
                 rather than defaulting a live side"
            )))
        }
    };
    let snapped = match side {
        OrderSide::Buy => round_up_to_tick(market, regime, stepped)?,
        _ => round_down_to_tick(market, regime, stepped)?,
    };
    let clamped = snapped.clamp(band.lower, band.upper);
    // Snap INWARD after the clamp: KRX publishes on-grid limits, so this is the
    // identity in practice, but an off-grid band would otherwise become an off-grid
    // order the gateway rejects.
    let priced = match side {
        OrderSide::Buy => round_down_to_tick(market, regime, clamped)?,
        _ => round_up_to_tick(market, regime, clamped)?,
    };
    if priced <= 0 {
        return Err(AdapterError::Config(format!(
            "marketable limit: priced to {priced} for last={last} k={k} band=[{}, {}] — refusing \
             a non-positive limit",
            band.lower, band.upper
        )));
    }
    Ok(priced)
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
    fn round_up_snaps_to_grid() {
        // 60,123 KRW KOSPI post-2023 → tick 100 → snap UP to 60,200 (down was 60,100).
        assert_eq!(
            round_up_to_tick(Market::Kospi, TickRegime::Post2023, 60_123).unwrap(),
            60_200
        );
        // Already on the grid stays put — round-up is not "always step".
        assert_eq!(
            round_up_to_tick(Market::Kospi, TickRegime::Post2023, 60_100).unwrap(),
            60_100
        );
    }

    #[test]
    fn round_up_across_a_band_boundary_lands_on_the_higher_bands_grid() {
        // 19,995 sits in the 10-tick band; rounding up crosses into the 50-tick band
        // at 20,000. The result must be on the NEW band's grid, not the old one's.
        let up = round_up_to_tick(Market::Kospi, TickRegime::Post2023, 19_995).unwrap();
        assert_eq!(up, 20_000);
        assert_eq!(up % tick_size(Market::Kospi, TickRegime::Post2023, up).unwrap(), 0);
        // Pre-2023 KOSPI: 9,998 (tick 10) → 10,000, where the tick becomes 50.
        let up = round_up_to_tick(Market::Kospi, TickRegime::Pre2023, 9_998).unwrap();
        assert_eq!(up, 10_000);
        assert_eq!(up % tick_size(Market::Kospi, TickRegime::Pre2023, up).unwrap(), 0);
    }

    #[test]
    fn marketable_buy_steps_up_k_ticks_onto_the_grid() {
        // 60,000 KOSPI post-2023 → tick 100. k=3 → 60,300, already on the grid.
        let band = PriceBand { lower: 42_000, upper: 78_000 };
        assert_eq!(
            marketable_limit(Market::Kospi, TickRegime::Post2023, 60_000, OrderSide::Buy, 3, band)
                .unwrap(),
            60_300
        );
        // An off-grid last price rounds UP after the step, never down (a rounded-down
        // buy can sit below the auction's clearing price and miss the fill).
        assert_eq!(
            marketable_limit(Market::Kospi, TickRegime::Post2023, 60_123, OrderSide::Buy, 3, band)
                .unwrap(),
            60_500
        );
    }

    #[test]
    fn marketable_sell_steps_down_k_ticks_onto_the_grid() {
        let band = PriceBand { lower: 42_000, upper: 78_000 };
        assert_eq!(
            marketable_limit(Market::Kospi, TickRegime::Post2023, 60_000, OrderSide::Sell, 3, band)
                .unwrap(),
            59_700
        );
        // 60,123 − 300 = 59,823 → rounded DOWN to 59,800.
        assert_eq!(
            marketable_limit(Market::Kospi, TickRegime::Post2023, 60_123, OrderSide::Sell, 3, band)
                .unwrap(),
            59_800
        );
    }

    #[test]
    fn marketable_limit_clamps_to_the_daily_band() {
        // A buy stepping past the 상한가 is clamped to it, not sent above it.
        let band = PriceBand { lower: 42_000, upper: 60_200 };
        assert_eq!(
            marketable_limit(Market::Kospi, TickRegime::Post2023, 60_000, OrderSide::Buy, 5, band)
                .unwrap(),
            60_200,
            "a buy never prices above the 상한가"
        );
        // The mirror: a sell stepping below the 하한가 is clamped to it.
        let band = PriceBand { lower: 59_900, upper: 78_000 };
        assert_eq!(
            marketable_limit(Market::Kospi, TickRegime::Post2023, 60_000, OrderSide::Sell, 5, band)
                .unwrap(),
            59_900,
            "a sell never prices below the 하한가"
        );
    }

    #[test]
    fn marketable_limit_k_zero_is_the_last_price_snapped() {
        // k=0 is a legal (if unprotected) policy: the limit is just the mark on the
        // grid, rounded in the crossing direction.
        let band = PriceBand { lower: 42_000, upper: 78_000 };
        assert_eq!(
            marketable_limit(Market::Kospi, TickRegime::Post2023, 60_123, OrderSide::Buy, 0, band)
                .unwrap(),
            60_200
        );
        assert_eq!(
            marketable_limit(Market::Kospi, TickRegime::Post2023, 60_123, OrderSide::Sell, 0, band)
                .unwrap(),
            60_100
        );
    }

    #[test]
    fn marketable_limit_refuses_garbage_inputs_rather_than_pricing_them() {
        let band = PriceBand { lower: 42_000, upper: 78_000 };
        // An ambiguous side is refused, never defaulted to a live sell.
        assert!(marketable_limit(
            Market::Kospi,
            TickRegime::Post2023,
            60_000,
            OrderSide::NoOrderSide,
            3,
            band
        )
        .is_err());
        // A missing/garbage mark (0 or negative) is refused.
        assert!(
            marketable_limit(Market::Kospi, TickRegime::Post2023, 0, OrderSide::Buy, 3, band)
                .is_err()
        );
        // An inverted band is refused.
        let inverted = PriceBand { lower: 78_000, upper: 42_000 };
        assert!(marketable_limit(
            Market::Kospi,
            TickRegime::Post2023,
            60_000,
            OrderSide::Buy,
            3,
            inverted
        )
        .is_err());
    }

    #[test]
    fn marketable_limit_uses_the_kosdaq_ladder_when_the_market_says_so() {
        // Pre-2023 KOSDAQ caps its tick at 100 above 50,000 while KOSPI steps to 500,
        // so the same k=2 step prices differently. The market is a PARAMETER here for
        // exactly this reason.
        let band = PriceBand { lower: 84_000, upper: 156_000 };
        let kosdaq =
            marketable_limit(Market::Kosdaq, TickRegime::Pre2023, 120_000, OrderSide::Buy, 2, band)
                .unwrap();
        let kospi =
            marketable_limit(Market::Kospi, TickRegime::Pre2023, 120_000, OrderSide::Buy, 2, band)
                .unwrap();
        assert_eq!(kosdaq, 120_200, "KOSDAQ tick 100 × 2");
        assert_eq!(kospi, 121_000, "KOSPI tick 500 × 2");
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
