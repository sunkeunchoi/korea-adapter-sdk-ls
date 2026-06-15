//! Per-category token-bucket rate limiting.
//!
//! Uses `governor::DefaultDirectRateLimiter` — the GCRA algorithm with correct
//! burst-token handling. Each category is an independent bucket. `wait(category)`
//! blocks until a token is available and NEVER returns an error — the
//! caller-transparent-wait contract.
//!
//! `DefaultDirectRateLimiter` is `Send + Sync`, so the manager needs no Mutex.

use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use std::num::NonZeroU32;

use crate::config_resolve::ResolvedRateLimits;
use crate::{LsError, LsResult};

/// Rate-limit bucket a dispatch charges against.
///
/// This is also the `rate_bucket` metadata vocabulary mirrored by per-TR
/// metadata, so every variant is load-bearing even when no runtime path
/// currently charges it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitCategory {
    /// Market data (quotes, orderbooks, indices) — default 5/s.
    MarketData,
    /// Order submission / cancel / modify — default 3/s.
    Orders,
    /// Account balances, positions, order history — default 1/s.
    Account,
    /// OAuth2 token / revoke — default 1/s.
    Auth,
}

impl RateLimitCategory {
    /// Variant name as a `&'static str`, byte-identical to the `Debug`
    /// rendering — lets `wait` record the tracing `category` field without a
    /// per-call `format!` allocation. The equivalence is locked by
    /// `as_str_matches_debug_rendering` below.
    pub const fn as_str(&self) -> &'static str {
        match self {
            RateLimitCategory::MarketData => "MarketData",
            RateLimitCategory::Orders => "Orders",
            RateLimitCategory::Account => "Account",
            RateLimitCategory::Auth => "Auth",
        }
    }
}

/// Four independent rate limiters — one per category.
pub struct RateLimiterManager {
    market_data: DefaultDirectRateLimiter,
    orders: DefaultDirectRateLimiter,
    account: DefaultDirectRateLimiter,
    auth: DefaultDirectRateLimiter,
}

impl RateLimiterManager {
    /// Build the four limiters from resolved rate limits.
    ///
    /// # Errors
    ///
    /// Returns [`LsError::Config`] naming the offending field if any category's
    /// configured rate is zero — a zero rate is a bucket that never replenishes.
    pub fn new(limits: &ResolvedRateLimits) -> LsResult<Self> {
        let mk = |n: u32, field: &str| -> LsResult<DefaultDirectRateLimiter> {
            let nz = NonZeroU32::new(n)
                .ok_or_else(|| LsError::Config(format!("{field} must be non-zero")))?;
            Ok(RateLimiter::direct(Quota::per_second(nz)))
        };
        Ok(Self {
            market_data: mk(limits.market_data_per_sec, "market_data_per_sec")?,
            orders: mk(limits.orders_per_sec, "orders_per_sec")?,
            account: mk(limits.account_per_sec, "account_per_sec")?,
            auth: mk(limits.auth_per_sec, "auth_per_sec")?,
        })
    }

    /// Block until a token is available for the given category.
    /// Never returns an error — caller-transparent wait.
    #[tracing::instrument(skip_all, fields(category))]
    pub async fn wait(&self, category: RateLimitCategory) {
        let category_str = category.as_str();
        tracing::Span::current().record("category", category_str);
        let start = std::time::Instant::now();
        match category {
            RateLimitCategory::MarketData => self.market_data.until_ready().await,
            RateLimitCategory::Orders => self.orders.until_ready().await,
            RateLimitCategory::Account => self.account.until_ready().await,
            RateLimitCategory::Auth => self.auth.until_ready().await,
        }
        let elapsed = start.elapsed();
        if elapsed > std::time::Duration::from_millis(10) {
            tracing::info!(
                wait_ms = elapsed.as_millis() as u64,
                category = category_str,
                "rate limiter wait"
            );
        }
    }
}

impl std::fmt::Debug for RateLimiterManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimiterManager").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_resolve::ResolvedRateLimits;
    use crate::RateLimitConfig;

    /// Buckets must be `Send + Sync` without a Mutex.
    fn _assert_send_sync<T: Send + Sync>() {}
    #[test]
    fn rate_limiter_manager_is_send_sync() {
        _assert_send_sync::<RateLimiterManager>();
    }

    #[test]
    fn as_str_matches_debug_rendering() {
        for category in [
            RateLimitCategory::MarketData,
            RateLimitCategory::Orders,
            RateLimitCategory::Account,
            RateLimitCategory::Auth,
        ] {
            assert_eq!(
                category.as_str(),
                format!("{:?}", category),
                "as_str must stay byte-identical to the Debug rendering"
            );
        }
    }

    #[tokio::test]
    async fn wait_market_data_returns_promptly_under_rate() {
        // The first cell in a fresh GCRA bucket is available immediately, so a
        // single wait under the configured rate must not block meaningfully.
        let resolved = ResolvedRateLimits::from_raw(&None);
        let mgr = RateLimiterManager::new(&resolved).unwrap();
        let start = std::time::Instant::now();
        mgr.wait(RateLimitCategory::MarketData).await;
        assert!(
            start.elapsed() < std::time::Duration::from_millis(50),
            "first wait under the configured rate should return promptly"
        );
    }

    #[test]
    fn test_rate_limiter_manager_constructs_with_defaults() {
        let defaults = ResolvedRateLimits::from_raw(&None);
        let _m = RateLimiterManager::new(&defaults).unwrap();
    }

    #[test]
    fn test_rate_limiter_manager_honors_config_overrides() {
        let cfg = RateLimitConfig {
            market_data_per_sec: Some(10),
            orders_per_sec: Some(5),
            account_per_sec: Some(2),
            auth_per_sec: Some(2),
        };
        let resolved = ResolvedRateLimits::from_raw(&Some(cfg));
        let _m = RateLimiterManager::new(&resolved).unwrap();
    }

    /// A zero rate limit is a non-functional limiter (a bucket that never
    /// replenishes). The `NonZeroU32` guard in `RateLimiterManager::new` rejects
    /// it per category with an `LsError::Config` that names the offending field.
    ///
    /// `ResolvedRateLimits::from_raw` preserves `Some(0)` verbatim — `unwrap_or`
    /// only substitutes the default for `None` — so the guard is reachable
    /// through the normal resolution path.
    #[test]
    fn zero_rate_limit_rejected_for_each_category() {
        let cases = [
            (
                RateLimitConfig {
                    market_data_per_sec: Some(0),
                    ..Default::default()
                },
                "market_data_per_sec",
            ),
            (
                RateLimitConfig {
                    orders_per_sec: Some(0),
                    ..Default::default()
                },
                "orders_per_sec",
            ),
            (
                RateLimitConfig {
                    account_per_sec: Some(0),
                    ..Default::default()
                },
                "account_per_sec",
            ),
            (
                RateLimitConfig {
                    auth_per_sec: Some(0),
                    ..Default::default()
                },
                "auth_per_sec",
            ),
        ];

        for (cfg, field) in cases {
            let resolved = ResolvedRateLimits::from_raw(&Some(cfg));
            let err = RateLimiterManager::new(&resolved)
                .expect_err(&format!("zero {field} must be rejected"));
            match err {
                LsError::Config(msg) => assert!(
                    msg.contains(field),
                    "Config error must name the offending field {field}, got: {msg}"
                ),
                other => panic!("expected LsError::Config for zero {field}, got {other:?}"),
            }
        }
    }

    /// Sanity: `Some(1)` for every category constructs successfully — the guard
    /// rejects only zero, not all overrides.
    #[test]
    fn nonzero_rate_limits_construct_for_all_categories() {
        let cfg = RateLimitConfig {
            market_data_per_sec: Some(1),
            orders_per_sec: Some(1),
            account_per_sec: Some(1),
            auth_per_sec: Some(1),
        };
        let resolved = ResolvedRateLimits::from_raw(&Some(cfg));
        assert!(
            RateLimiterManager::new(&resolved).is_ok(),
            "Some(1) for every category must construct — the guard rejects only zero"
        );
    }
}
