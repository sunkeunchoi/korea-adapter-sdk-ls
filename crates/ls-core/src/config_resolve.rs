//! Resolved runtime configuration — single source of truth for all defaults.
//!
//! `ResolvedConfig` materializes every `Option<T>` field from [`LsConfig`] into
//! a concrete value, so consumers (`Inner`, `WsManager`, `RateLimiterManager`)
//! never contain local `unwrap_or(fallback)` logic.
//!
//! All defaults are declared as named constants at the top of this module.
//! A reviewer can read one file to see the complete default table.

use std::time::Duration;

use crate::config::{Environment, LsConfig, RateLimitConfig, WsOverflowPolicy};
use crate::{LsError, LsResult};

// ---------------------------------------------------------------------------
// Default constants — the complete runtime default table
// ---------------------------------------------------------------------------

/// Default TCP connect timeout for HTTP requests.
pub const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;

/// Default full-request timeout (connect + send + read body).
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;

/// Default WebSocket connection timeout.
pub const DEFAULT_WS_CONNECT_TIMEOUT_SECS: u64 = 15;

/// Default per-subscriber dispatch channel capacity.
pub const DEFAULT_WS_CHANNEL_CAPACITY: usize = 64;

/// Default page-collection cap for `collect_all`.
pub const DEFAULT_MAX_PAGES: usize = 100;

// Rate-limit defaults (per second)

/// Default market-data rate limit.
pub const DEFAULT_MARKET_DATA_PER_SEC: u32 = 5;

/// Default order-submission rate limit.
pub const DEFAULT_ORDERS_PER_SEC: u32 = 3;

/// Default account-inquiry rate limit.
pub const DEFAULT_ACCOUNT_PER_SEC: u32 = 1;

/// Default auth rate limit.
pub const DEFAULT_AUTH_PER_SEC: u32 = 1;

// ---------------------------------------------------------------------------
// Resolved types
// ---------------------------------------------------------------------------

/// Rate-limit overrides with all `None` values materialized to concrete `u32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedRateLimits {
    /// Effective market-data rate limit (per second).
    pub market_data_per_sec: u32,
    /// Effective order-submission rate limit (per second).
    pub orders_per_sec: u32,
    /// Effective account-inquiry rate limit (per second).
    pub account_per_sec: u32,
    /// Effective auth rate limit (per second).
    pub auth_per_sec: u32,
}

impl ResolvedRateLimits {
    /// Materialize from an optional [`RateLimitConfig`].
    pub fn from_raw(raw: &Option<RateLimitConfig>) -> Self {
        match raw {
            Some(c) => Self {
                market_data_per_sec: c.market_data_per_sec.unwrap_or(DEFAULT_MARKET_DATA_PER_SEC),
                orders_per_sec: c.orders_per_sec.unwrap_or(DEFAULT_ORDERS_PER_SEC),
                account_per_sec: c.account_per_sec.unwrap_or(DEFAULT_ACCOUNT_PER_SEC),
                auth_per_sec: c.auth_per_sec.unwrap_or(DEFAULT_AUTH_PER_SEC),
            },
            None => Self {
                market_data_per_sec: DEFAULT_MARKET_DATA_PER_SEC,
                orders_per_sec: DEFAULT_ORDERS_PER_SEC,
                account_per_sec: DEFAULT_ACCOUNT_PER_SEC,
                auth_per_sec: DEFAULT_AUTH_PER_SEC,
            },
        }
    }
}

/// Fully-resolved runtime configuration — every field is a concrete value.
///
/// Construct via [`ResolvedConfig::from_raw`]. This performs validation
/// (e.g. `ws_channel_capacity >= 1`) and resolves URLs so that downstream
/// consumers need only read fields directly.
///
/// `Debug` is implemented manually and redacts the credential-bearing fields
/// (`appkey`, `appsecretkey`, `account_no`). Deriving `Debug` here would leak
/// credentials whenever a resolved config is logged.
#[derive(Clone, PartialEq, Eq)]
pub struct ResolvedConfig {
    // Credentials
    /// Customer app key.
    pub appkey: String,
    /// Customer app secret.
    pub appsecretkey: String,
    /// Account number (CANO).
    pub account_no: String,

    // Environment
    /// Target environment (Real / Paper).
    pub environment: Environment,
    /// Resolved REST base URL (override or environment default).
    pub base_url: String,
    /// Resolved WebSocket URL (override or environment default).
    pub ws_url: String,
    /// Whether loopback `http`/`ws` is permitted.
    pub allow_insecure_localhost: bool,

    // HTTP timeouts
    /// TCP connect timeout.
    pub connect_timeout: Duration,
    /// Full request timeout.
    pub request_timeout: Duration,

    // WebSocket config
    /// WS connect timeout.
    pub ws_connect_timeout: Duration,
    /// Per-subscriber channel capacity.
    pub ws_channel_capacity: usize,
    /// Overflow policy when channel is full.
    pub ws_overflow_policy: WsOverflowPolicy,

    // Pagination
    /// Maximum pages for `collect_all`.
    pub max_pages: usize,

    // Rate limits
    /// Resolved per-category rate limits.
    pub rate_limits: ResolvedRateLimits,
}

impl ResolvedConfig {
    /// Validate and resolve an [`LsConfig`] into a fully-materialized [`ResolvedConfig`].
    ///
    /// # Errors
    ///
    /// Returns [`LsError::Config`] if:
    /// - `ws_channel_capacity` is `Some(0)` (empty channel would block forever).
    /// - `max_pages` is `Some(0)` (`collect_all` would silently fetch nothing and
    ///   return an empty result, masking "never queried" as "no data").
    pub fn from_raw(raw: &LsConfig) -> LsResult<Self> {
        // capacity=0 is rejected immediately with a field-named error.
        if raw.ws_channel_capacity == Some(0) {
            return Err(LsError::Config(
                "ws_channel_capacity must be >= 1 (None uses the SDK default of 64)".into(),
            ));
        }
        // max_pages=0 makes collect_all return Ok(vec![]) with zero HTTP calls —
        // a silent failure where no-data is indistinguishable from never-queried.
        if raw.max_pages == Some(0) {
            return Err(LsError::Config(
                "max_pages must be >= 1 (None uses the SDK default of 100)".into(),
            ));
        }

        let connect_timeout = Duration::from_secs(
            raw.connect_timeout_secs
                .unwrap_or(DEFAULT_CONNECT_TIMEOUT_SECS),
        );
        let request_timeout = Duration::from_secs(
            raw.request_timeout_secs
                .unwrap_or(DEFAULT_REQUEST_TIMEOUT_SECS),
        );
        let ws_connect_timeout = Duration::from_secs(
            raw.ws_connect_timeout_secs
                .unwrap_or(DEFAULT_WS_CONNECT_TIMEOUT_SECS),
        );
        let ws_channel_capacity = raw
            .ws_channel_capacity
            .unwrap_or(DEFAULT_WS_CHANNEL_CAPACITY);
        let ws_overflow_policy = raw.ws_overflow_policy.clone().unwrap_or_default();
        let max_pages = raw.max_pages.unwrap_or(DEFAULT_MAX_PAGES);
        let rate_limits = ResolvedRateLimits::from_raw(&raw.rate_limits);
        let base_url = Environment::resolve_base_url(raw);
        let ws_url = Environment::resolve_ws_url(raw);

        Ok(Self {
            appkey: raw.appkey.clone(),
            appsecretkey: raw.appsecretkey.clone(),
            account_no: raw.account_no.clone(),
            environment: raw.environment.clone(),
            base_url,
            ws_url,
            allow_insecure_localhost: raw.allow_insecure_localhost,
            connect_timeout,
            request_timeout,
            ws_connect_timeout,
            ws_channel_capacity,
            ws_overflow_policy,
            max_pages,
            rate_limits,
        })
    }
}

impl std::fmt::Debug for ResolvedConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // ResolvedConfig carries credentials; redact them exactly as LsConfig does.
        f.debug_struct("ResolvedConfig")
            .field("appkey", &"<redacted>")
            .field("appsecretkey", &"<redacted>")
            .field("account_no", &"<redacted>")
            .field("environment", &self.environment)
            .field("base_url", &self.base_url)
            .field("ws_url", &self.ws_url)
            .field("allow_insecure_localhost", &self.allow_insecure_localhost)
            .field("connect_timeout", &self.connect_timeout)
            .field("request_timeout", &self.request_timeout)
            .field("ws_connect_timeout", &self.ws_connect_timeout)
            .field("ws_channel_capacity", &self.ws_channel_capacity)
            .field("ws_overflow_policy", &self.ws_overflow_policy)
            .field("max_pages", &self.max_pages)
            .field("rate_limits", &self.rate_limits)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolved_rate_limits_defaults() {
        let r = ResolvedRateLimits::from_raw(&None);
        assert_eq!(r.market_data_per_sec, DEFAULT_MARKET_DATA_PER_SEC);
        assert_eq!(r.orders_per_sec, DEFAULT_ORDERS_PER_SEC);
        assert_eq!(r.account_per_sec, DEFAULT_ACCOUNT_PER_SEC);
        assert_eq!(r.auth_per_sec, DEFAULT_AUTH_PER_SEC);
    }

    #[test]
    fn test_resolved_rate_limits_overrides() {
        let cfg = RateLimitConfig {
            market_data_per_sec: Some(10),
            orders_per_sec: Some(6),
            account_per_sec: Some(2),
            auth_per_sec: Some(3),
        };
        let r = ResolvedRateLimits::from_raw(&Some(cfg));
        assert_eq!(r.market_data_per_sec, 10);
        assert_eq!(r.orders_per_sec, 6);
        assert_eq!(r.account_per_sec, 2);
        assert_eq!(r.auth_per_sec, 3);
    }

    #[test]
    fn test_resolved_rate_limits_partial_override() {
        let cfg = RateLimitConfig {
            market_data_per_sec: Some(20),
            orders_per_sec: None,
            account_per_sec: None,
            auth_per_sec: None,
        };
        let r = ResolvedRateLimits::from_raw(&Some(cfg));
        assert_eq!(r.market_data_per_sec, 20);
        assert_eq!(r.orders_per_sec, DEFAULT_ORDERS_PER_SEC);
        assert_eq!(r.account_per_sec, DEFAULT_ACCOUNT_PER_SEC);
        assert_eq!(r.auth_per_sec, DEFAULT_AUTH_PER_SEC);
    }

    #[test]
    fn test_all_defaults() {
        let resolved = ResolvedConfig::from_raw(&crate::config::test_config()).expect("resolve");

        assert_eq!(resolved.connect_timeout.as_secs(), 10);
        assert_eq!(resolved.request_timeout.as_secs(), 30);
        assert_eq!(resolved.ws_connect_timeout.as_secs(), 15);
        assert_eq!(resolved.ws_channel_capacity, 64);
        assert_eq!(resolved.ws_overflow_policy, WsOverflowPolicy::DropNewest);
        assert_eq!(resolved.max_pages, 100);
        assert_eq!(resolved.rate_limits.market_data_per_sec, 5);
        assert_eq!(resolved.rate_limits.orders_per_sec, 3);
        assert_eq!(resolved.rate_limits.account_per_sec, 1);
        assert_eq!(resolved.rate_limits.auth_per_sec, 1);
    }

    #[test]
    fn test_url_resolution() {
        let resolved = ResolvedConfig::from_raw(&crate::config::test_config()).expect("resolve");
        assert_eq!(resolved.base_url, "https://openapi.ls-sec.co.kr:8080");
        assert_eq!(
            resolved.ws_url,
            "wss://openapi.ls-sec.co.kr:29443/websocket"
        );
    }

    #[test]
    fn test_url_override() {
        let mut cfg = crate::config::test_config();
        cfg.base_url = Some("https://example.com".into());
        cfg.ws_base_url = Some("wss://example.com/ws".into());
        let resolved = ResolvedConfig::from_raw(&cfg).expect("resolve");
        assert_eq!(resolved.base_url, "https://example.com");
        assert_eq!(resolved.ws_url, "wss://example.com/ws");
    }

    #[test]
    fn test_all_overrides() {
        let mut cfg = crate::config::test_config();
        cfg.connect_timeout_secs = Some(5);
        cfg.request_timeout_secs = Some(60);
        cfg.ws_connect_timeout_secs = Some(20);
        cfg.ws_channel_capacity = Some(128);
        cfg.max_pages = Some(50);
        cfg.rate_limits = Some(RateLimitConfig {
            market_data_per_sec: Some(10),
            orders_per_sec: Some(6),
            account_per_sec: Some(2),
            auth_per_sec: Some(3),
        });

        let resolved = ResolvedConfig::from_raw(&cfg).expect("resolve");

        assert_eq!(resolved.connect_timeout.as_secs(), 5);
        assert_eq!(resolved.request_timeout.as_secs(), 60);
        assert_eq!(resolved.ws_connect_timeout.as_secs(), 20);
        assert_eq!(resolved.ws_channel_capacity, 128);
        assert_eq!(resolved.max_pages, 50);
        assert_eq!(resolved.rate_limits.market_data_per_sec, 10);
        assert_eq!(resolved.rate_limits.orders_per_sec, 6);
        assert_eq!(resolved.rate_limits.account_per_sec, 2);
        assert_eq!(resolved.rate_limits.auth_per_sec, 3);
    }

    #[test]
    fn test_partial_overrides() {
        let mut cfg = crate::config::test_config();
        cfg.connect_timeout_secs = Some(7);
        cfg.rate_limits = Some(RateLimitConfig {
            market_data_per_sec: Some(20),
            orders_per_sec: None,
            account_per_sec: None,
            auth_per_sec: None,
        });

        let resolved = ResolvedConfig::from_raw(&cfg).expect("resolve");

        assert_eq!(resolved.connect_timeout.as_secs(), 7);
        assert_eq!(resolved.request_timeout.as_secs(), 30);
        assert_eq!(resolved.rate_limits.market_data_per_sec, 20);
        assert_eq!(resolved.rate_limits.orders_per_sec, 3);
    }

    #[test]
    fn test_capacity_zero_rejected() {
        let mut cfg = crate::config::test_config();
        cfg.ws_channel_capacity = Some(0);
        let err = ResolvedConfig::from_raw(&cfg).expect_err("should fail");
        match err {
            LsError::Config(msg) => {
                assert!(
                    msg.contains("ws_channel_capacity"),
                    "error should mention field: {msg}"
                );
            }
            other => panic!("expected LsError::Config, got {other:?}"),
        }
    }

    #[test]
    fn test_max_pages_zero_rejected() {
        let mut cfg = crate::config::test_config();
        cfg.max_pages = Some(0);
        let err = ResolvedConfig::from_raw(&cfg).expect_err("should fail");
        match err {
            LsError::Config(msg) => {
                assert!(
                    msg.contains("max_pages"),
                    "error should mention field: {msg}"
                );
            }
            other => panic!("expected LsError::Config, got {other:?}"),
        }
    }

    #[test]
    fn test_capacity_one_accepted() {
        let mut cfg = crate::config::test_config();
        cfg.ws_channel_capacity = Some(1);
        let resolved = ResolvedConfig::from_raw(&cfg).expect("resolve");
        assert_eq!(resolved.ws_channel_capacity, 1);
    }

    #[test]
    fn resolved_config_debug_redacts_credentials() {
        // CRITICAL credential-leak regression guard: the old repo derived Debug
        // on ResolvedConfig, leaking secrets. The known test appkey/secret/account
        // must be absent from the Debug output.
        let resolved = ResolvedConfig::from_raw(&crate::config::test_config()).expect("resolve");
        let dbg = format!("{resolved:?}");
        assert!(
            !dbg.contains("test-appkey"),
            "appkey leaked in Debug: {dbg}"
        );
        assert!(
            !dbg.contains("test-appsecretkey"),
            "secret leaked in Debug: {dbg}"
        );
        assert!(
            !dbg.contains("00000000-01"),
            "account leaked in Debug: {dbg}"
        );
        assert!(dbg.contains("<redacted>"));
    }
}
