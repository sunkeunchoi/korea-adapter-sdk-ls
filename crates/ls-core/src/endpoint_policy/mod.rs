//! Structured endpoint policy — the runtime descriptor for a single TR.
//!
//! Every REST TR has a static `EndpointPolicy` constant. Dispatch methods pass
//! `&EndpointPolicy` to `Inner::post` instead of loose `path` / `category` /
//! `tr_code` strings, so routing is a structured value rather than string
//! coupling.
//!
//! The `{TR}_POLICY` consts below are the runtime mirror of the `tr-index.yaml`
//! selector fields. They are hand-authored (not generated): the `ls-metadata`
//! validator cross-checks each const against the index so code and metadata
//! cannot drift.

use crate::{LsError, LsResult, RateLimitCategory};

/// Protocol for an endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// REST HTTP endpoint.
    Rest,
    /// WebSocket real-time feed.
    WebSocket,
}

/// Static metadata for a single TR endpoint.
///
/// All string fields are `&'static str` — zero allocation, zero clone cost —
/// and the whole struct is `Copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointPolicy {
    /// TR code, e.g. `"t1102"` or `"CSPAQ12200"`.
    pub tr_code: &'static str,
    /// REST path, e.g. `"/stock/market-data"`. WebSocket TRs use `"/websocket"`.
    pub path: &'static str,
    /// Rust module name, e.g. `"stock"`.
    pub module: &'static str,
    /// Group name from the spec, e.g. `"[주식] 시세"`.
    pub group: &'static str,
    /// `Rest` or `WebSocket`.
    pub protocol: Protocol,
    /// Rate-limit bucket charged for this endpoint.
    pub category: RateLimitCategory,
    /// `true` for order submission / cancel / modify endpoints.
    pub is_order: bool,
    /// `true` if the request supports `tr_cont`/`tr_cont_key` pagination.
    pub has_pagination: bool,
    /// Per-TR upstream rate limit (requests/sec), if declared in the LS spec.
    pub rate_limit_per_sec: Option<u32>,
    /// Corporate per-TR upstream rate limit (requests/sec), if declared in the LS spec.
    pub corp_rate_limit_per_sec: Option<u32>,
}

impl EndpointPolicy {
    /// Runtime guard: non-order dispatch methods must not be used for order
    /// endpoints — those must flow through the order dispatch path so
    /// deduplication and no-retry semantics are enforced.
    pub fn guard_non_order(&self) -> LsResult<()> {
        if self.is_order {
            return Err(LsError::ApiError {
                code: "order-dispatch".into(),
                message: format!(
                    "order endpoint '{}' must use the order dispatch path, not post/post_paginated",
                    self.tr_code
                ),
            });
        }
        Ok(())
    }

    /// Runtime guard: the order dispatch path (`Inner::post_order`) must not be
    /// used for non-order endpoints — the inverse of [`guard_non_order`]. This is
    /// defense-in-depth against routing a market-data or account inquiry through
    /// the no-retry/dedup order path (order-safety contract §1).
    ///
    /// [`guard_non_order`]: EndpointPolicy::guard_non_order
    pub fn guard_order(&self) -> LsResult<()> {
        if !self.is_order {
            return Err(LsError::ApiError {
                code: "non-order-dispatch".into(),
                message: format!(
                    "non-order endpoint '{}' must not use the order dispatch path",
                    self.tr_code
                ),
            });
        }
        Ok(())
    }

    /// `true` if this endpoint uses REST HTTP.
    pub const fn is_rest(&self) -> bool {
        matches!(self.protocol, Protocol::Rest)
    }

    /// `true` if this endpoint uses WebSocket.
    pub const fn is_websocket(&self) -> bool {
        matches!(self.protocol, Protocol::WebSocket)
    }
}

// ---------------------------------------------------------------------------
// Slice TR policy constants — runtime mirror of `tr-index.yaml`. Decomposed
// by domain (Wave-3); pure relocation, each re-exported so every
// `endpoint_policy::FOO_POLICY` path is unchanged.
// ---------------------------------------------------------------------------
mod auth;
pub use auth::*;
mod market_data;
pub use market_data::*;
mod order;
pub use order::*;
mod chart_reference;
pub use chart_reference::*;
mod realtime_md;
pub use realtime_md::*;
mod realtime_order;
pub use realtime_order::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn test_policy(is_order: bool) -> EndpointPolicy {
        EndpointPolicy {
            tr_code: "TEST",
            path: "/test/path",
            module: "test",
            group: "test",
            protocol: Protocol::Rest,
            category: RateLimitCategory::MarketData,
            is_order,
            has_pagination: false,
            rate_limit_per_sec: None,
            corp_rate_limit_per_sec: None,
        }
    }

    #[test]
    fn guard_non_order_passes_for_non_order() {
        let p = test_policy(false);
        assert!(p.guard_non_order().is_ok());
    }

    #[test]
    fn guard_non_order_fails_for_order() {
        let p = test_policy(true);
        let err = p.guard_non_order().unwrap_err().to_string();
        assert!(err.contains("order endpoint"));
        assert!(err.contains("post/post_paginated"));
    }

    #[test]
    fn slice_rest_policies_are_non_order_rest() {
        for p in [
            TOKEN_POLICY,
            REVOKE_POLICY,
            T1101_POLICY,
            T1102_POLICY,
            T8412_POLICY,
            T8425_POLICY,
            T8436_POLICY,
            T1531_POLICY,
            T1537_POLICY,
            T1452_POLICY,
            T1481_POLICY,
            T1482_POLICY,
            T1403_POLICY,
            T1441_POLICY,
            T1463_POLICY,
            T1466_POLICY,
            T1489_POLICY,
            T1492_POLICY,
            T1859_POLICY,
            T1866_POLICY,
            T1825_POLICY,
            T1826_POLICY,
            T9905_POLICY,
            T9907_POLICY,
            T8431_POLICY,
            T8430_POLICY,
            T9942_POLICY,
            T1958_POLICY,
            T1964_POLICY,
            T1601_POLICY,
            T1615_POLICY,
            T1640_POLICY,
            T1662_POLICY,
            T1664_POLICY,
            T3341_POLICY,
            T8424_POLICY,
            T1511_POLICY,
            T1485_POLICY,
            T1516_POLICY,
            T1514_POLICY,
            T1901_POLICY,
            T1105_POLICY,
            T1104_POLICY,
            T1305_POLICY,
            // Closed-window more-flips wave (plan -001): ETF LP order-book read.
            T1906_POLICY,
            // Closed-window more-flips wave (plan -001): integrated current-price/order-book read.
            T8450_POLICY,
            // Closed-window more-flips wave (plan -001): per-stock remaining-quantity/pre-disclosure read.
            T1638_POLICY,
            // Closed-window more-flips wave (plan -001): time-bucketed trade-chart read.
            T1308_POLICY,
            // Closed-window more-flips wave (plan -001): price-band trade-weight read.
            T1449_POLICY,
            // Closed-window more-flips wave (plan -001): by-time investor-trading read.
            T1621_POLICY,
            // Closed-window more-flips wave (plan -001): F/O by-time investor-trading read.
            T2545_POLICY,
            // Closed-window more-flips wave (plan -001): F/O by-tick conclusion-board read.
            T8406_POLICY,
            // Closed-window more-flips wave (plan -001): multi-symbol current-price read.
            T8407_POLICY,
            // Open-window domestic program-trade reads: 综合 / intraday-trend / daily-trend.
            T1631_POLICY,
            T1632_POLICY,
            T1633_POLICY,
            // Open-window domestic reads: foreign/institution by-issue trend, ETF
            // intraday-trend + constituents, short-sale daily trend, stock-loan trend.
            T1716_POLICY,
            T1902_POLICY,
            T1904_POLICY,
            T1927_POLICY,
            T1941_POLICY,
            // Open-window domestic reads: foreign/institution trends, sector-investor
            // chart, intraday quote-remainder trend, VP-relative ranking.
            T1702_POLICY,
            T1717_POLICY,
            T1665_POLICY,
            T1471_POLICY,
            T1475_POLICY,
            // Closed-window more-flips wave (plan -001): LP-target ELW issue list read.
            T1959_POLICY,
            // Closed-window more-flips wave (plan -001): ELW current-price/quote read.
            T1950_POLICY,
            // Open-window flip wave (plan -001, 2026-06-30): ELW daily-price read.
            T1954_POLICY,
            // Closed-window more-flips wave (plan -001): ELW current-price + quote-board read.
            T1971_POLICY,
            // Closed-window more-flips wave (plan -001): ELW current-price + trading-member board read.
            T1972_POLICY,
            // Closed-window more-flips wave (plan -001): ELWs sharing a base asset read.
            T1974_POLICY,
            // Closed-window more-flips wave (plan -001): ELW current-price / contracted-payout snapshot read.
            T1956_POLICY,
            // Closed-window more-flips wave (plan -001): ELW screener / indicator search.
            T1969_POLICY,
            // Closed-window flip wave (plan -003): self-paginated stock reads.
            T1310_POLICY,
            T1404_POLICY,
            // Closed-window more-flips wave (plan -001): self-paginated stock read.
            T1410_POLICY,
            // Closed-window more-flips wave (plan -001): self-paginated stock read (margin rates).
            T1411_POLICY,
            // Closed-window more-flips wave (plan -001): self-paginated stock read (expected-exec).
            T1488_POLICY,
            // Closed-window more-flips wave (plan -001): self-paginated stock read (program-trading).
            T1636_POLICY,
            // Closed-window more-flips wave (plan -001): self-paginated stock read (signal search).
            T1809_POLICY,
            // Open-window domestic reads (plan -001): self-paginated stock tick/conclusion
            // + program-flow reads.
            T1109_POLICY,
            T1301_POLICY,
            T1486_POLICY,
            T8454_POLICY,
            T1637_POLICY,
            // Open-window domestic reads (plan -001): self-paginated investor-flow
            // + exchange-broker reads.
            T1602_POLICY,
            T1603_POLICY,
            T1617_POLICY,
            T1752_POLICY,
            T1771_POLICY,
            CSPAQ12200_POLICY,
            CSPAQ12300_POLICY,
            CSPAQ22200_POLICY,
            // Closed-window account-lane flip wave (plan -001): non-order account read.
            T0424_POLICY,
            CSPBQ00200_POLICY,
            CLNAQ00100_POLICY,
            CFOEQ11100_POLICY,
            T0441_POLICY,
            CIDBQ01400_POLICY,
            // Paper account credential lanes (plan -002): overseas-F/O account reads.
            CIDBQ03000_POLICY,
            CIDBQ05300_POLICY,
            // Closed-window account-lane flip wave (plan -001): server-time utility read.
            T0167_POLICY,
            // t0425 IS a non-order REST read — it belongs here AND in the
            // crosscheck. CSPAT00601 (is_order: true) deliberately does NOT.
            T0425_POLICY,
            CFOBQ10500_POLICY,
            CCENQ90200_POLICY,
            CFOAQ10100_POLICY,
            CCENQ10100_POLICY,
            T2301_POLICY,
            T2522_POLICY,
            T8401_POLICY,
            T8426_POLICY,
            T8433_POLICY,
            T8435_POLICY,
            T8467_POLICY,
            T9943_POLICY,
            T9944_POLICY,
            T2111_POLICY,
            T2112_POLICY,
            T2106_POLICY,
            T8402_POLICY,
            T8403_POLICY,
            T8434_POLICY,
            T1988_POLICY,
            T3102_POLICY,
            T3320_POLICY,
            // Domestic stock master/reference breadth wave (plan -004): non-order
            // REST reads — registered in BOTH crosscheck lists.
            T9945_POLICY,
            T3202_POLICY,
            T3401_POLICY,
            T3518_POLICY,
            T3521_POLICY,
            O3103_POLICY,
            O3104_POLICY,
            O3108_POLICY,
            O3116_POLICY,
            O3117_POLICY,
            O3123_POLICY,
            O3127_POLICY,
            O3128_POLICY,
            O3136_POLICY,
            O3137_POLICY,
            O3139_POLICY,
            T8427_POLICY,
            T2210_POLICY,
            T2424_POLICY,
            T2541_POLICY,
            T2214_POLICY,
            T8428_POLICY,
            T8462_POLICY,
            T8410_POLICY,
            T8451_POLICY,
            T8419_POLICY,
            T4203_POLICY,
            T8417_POLICY,
            T8418_POLICY,
            T8411_POLICY,
            T8452_POLICY,
            T8453_POLICY,
            T1302_POLICY,
            T8464_POLICY,
            T8465_POLICY,
            T8466_POLICY,
            T8405_POLICY,
            T2216_POLICY,
            T1444_POLICY,
            T1422_POLICY,
            T1427_POLICY,
            T1442_POLICY,
            T1405_POLICY,
            T1960_POLICY,
            T1961_POLICY,
            T1966_POLICY,
            T1921_POLICY,
            T1532_POLICY,
            T1533_POLICY,
            T1926_POLICY,
            T1764_POLICY,
            T1903_POLICY,
            T8455_POLICY,
            T8460_POLICY,
            T8463_POLICY,
            G3101_POLICY,
            G3104_POLICY,
            G3106_POLICY,
            G3102_POLICY,
            G3103_POLICY,
            G3190_POLICY,
            O3101_POLICY,
            O3121_POLICY,
            O3105_POLICY,
            O3106_POLICY,
            O3125_POLICY,
            O3126_POLICY,
        ] {
            assert!(!p.is_order, "{} must not be an order endpoint", p.tr_code);
            assert!(p.is_rest(), "{} must be a REST endpoint", p.tr_code);
            assert!(p.guard_non_order().is_ok());
        }
    }

    #[test]
    fn s3_policy_is_websocket() {
        assert!(S3_POLICY.is_websocket());
        assert!(!S3_POLICY.is_rest());
        assert_eq!(S3_POLICY.path, "/websocket");
    }
}
