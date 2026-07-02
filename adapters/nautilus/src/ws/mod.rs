//! WebSocket bridging: adapter-owned frame rows + a reconnect supervisor (KTD8).

pub mod rows;
pub mod supervisor;

use std::time::{SystemTime, UNIX_EPOCH};

use nautilus_core::UnixNanos;

use crate::rules::Market;

/// The real-time trade `tr_cd` for a market segment (S3_ KOSPI / K3_ KOSDAQ).
pub fn trade_tr_cd(market: Market) -> &'static str {
    match market {
        Market::Kospi => "S3_",
        Market::Kosdaq => "K3_",
    }
}

/// The real-time top-of-book `tr_cd` for a market segment (H1_ KOSPI / HA_ KOSDAQ).
pub fn quote_tr_cd(market: Market) -> &'static str {
    match market {
        Market::Kospi => "H1_",
        Market::Kosdaq => "HA_",
    }
}

/// Current wall-clock time as [`UnixNanos`] (receipt timestamp for live ticks).
pub fn now_nanos() -> UnixNanos {
    let ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    UnixNanos::from(ns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_by_market_segment() {
        assert_eq!(trade_tr_cd(Market::Kospi), "S3_");
        assert_eq!(trade_tr_cd(Market::Kosdaq), "K3_");
        assert_eq!(quote_tr_cd(Market::Kospi), "H1_");
        assert_eq!(quote_tr_cd(Market::Kosdaq), "HA_");
    }
}
