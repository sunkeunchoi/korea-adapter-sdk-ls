//! Client configuration types.
//!
//! `LsConfig` is constructed as a plain struct literal — no builder pattern.
//! All fields are `pub` so downstream crates construct it directly.
//!
//! For ergonomic env-var loading, use [`LsConfig::from_env`] which reads
//! paper / real credentials from `LS_PAPER_*` / `LS_REAL_*` (or
//! legacy `LS_APPKEY` / `LS_SECRET` / `LS_ACCOUNT`) based on `LS_TRADING_ENV`.
//!
//! `Environment` is a 2-variant enum — no runtime inference; users choose
//! Real or Paper explicitly. String parsing via [`FromStr`] accepts only the
//! two canonical names (`paper`, `real`) — no aliases.
//!
//! `base_url: Option<String>` is the test-injection escape hatch. When
//! `Some(url)`, runtime code uses that URL verbatim. When `None`, runtime
//! code falls back to `environment.base_url()`. Production callers set `None`;
//! tests set `Some(mock_server.uri())`. This enables tests to exercise real
//! production code paths instead of replicating pipeline logic in "mirror" tests.

use crate::{LsError, LsResult};
use std::str::FromStr;

/// Which LS API environment to target.
///
/// Both variants currently route to the same public REST gateway
/// (`https://openapi.ls-sec.co.kr:8080`). Paper is distinguished by
/// the appkey/appsecretkey credentials, not by domain — LS has no separate
/// sandbox host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Environment {
    /// Production (real-money) environment.
    Real,
    /// Paper-trading environment.
    Paper,
}

impl Environment {
    /// Return the base URL for this environment.
    ///
    /// Both Real and Paper currently return the same URL — LS routes
    /// paper traffic by credential, not by domain.
    pub fn base_url(&self) -> &'static str {
        match self {
            Environment::Real => "https://openapi.ls-sec.co.kr:8080",
            Environment::Paper => "https://openapi.ls-sec.co.kr:8080",
        }
    }

    /// Resolve the effective base URL for a given config.
    ///
    /// Returns `config.base_url` if `Some(url)`; otherwise falls back to
    /// `config.environment.base_url()`. All runtime HTTP dispatch points MUST
    /// call this helper instead of reading `environment.base_url()` directly —
    /// otherwise tests cannot inject a wiremock URL.
    pub fn resolve_base_url(config: &LsConfig) -> String {
        config
            .base_url
            .clone()
            .unwrap_or_else(|| config.environment.base_url().to_string())
    }

    /// Return the WebSocket URL for this environment.
    ///
    /// Unlike `base_url`, these URLs DO differ per environment (different ports).
    pub fn ws_url(&self) -> &'static str {
        match self {
            Environment::Real => "wss://openapi.ls-sec.co.kr:9443/websocket",
            Environment::Paper => "wss://openapi.ls-sec.co.kr:29443/websocket",
        }
    }

    /// Resolve the effective WebSocket URL for a given config.
    /// Mirrors `resolve_base_url` exactly — override wins; environment default otherwise.
    pub fn resolve_ws_url(config: &LsConfig) -> String {
        config
            .ws_base_url
            .clone()
            .unwrap_or_else(|| config.environment.ws_url().to_string())
    }

    /// Convenience constructor for the paper-trading environment.
    pub fn paper() -> Self {
        Environment::Paper
    }

    /// Convenience constructor for the production (real-money) environment.
    pub fn production() -> Self {
        Environment::Real
    }

    /// Returns `true` if this is the paper-trading environment.
    pub fn is_paper(&self) -> bool {
        matches!(self, Environment::Paper)
    }

    /// Returns `true` if this is the production environment.
    pub fn is_production(&self) -> bool {
        matches!(self, Environment::Real)
    }
}

impl std::fmt::Display for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Environment::Real => write!(f, "real"),
            Environment::Paper => write!(f, "paper"),
        }
    }
}

impl FromStr for Environment {
    type Err = LsError;

    fn from_str(s: &str) -> LsResult<Self> {
        match s.to_ascii_lowercase().as_str() {
            "paper" => Ok(Environment::Paper),
            "real" => Ok(Environment::Real),
            other => Err(LsError::Config(format!(
                "unknown environment '{other}'. Expected one of: paper, real"
            ))),
        }
    }
}

/// What to do when a WebSocket subscriber's channel is full.
///
/// No overflow policy guarantees complete state delivery; operators requiring
/// complete state must rebuild from REST TR snapshots after drops.
///
/// Configured via [`LsConfig::ws_overflow_policy`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum WsOverflowPolicy {
    /// Drop the newest (incoming) frame when the channel is full.
    ///
    /// The bounded `mpsc` channel keeps the oldest unread frames intact.
    /// Frames that cannot be enqueued are silently discarded and counted.
    /// This is the default policy.
    #[default]
    DropNewest,
    /// Keep only the latest frame; previous unread frame is overwritten.
    ///
    /// Uses a shared `Mutex<Option<T>>` + `Notify` slot instead of a
    /// channel. The channel capacity field is ignored for this variant.
    LatestOnly,
}

/// Per-category rate limit overrides. Any `None` falls back to the
/// defaults (market 5/s, orders 3/s, account 1/s, auth 1/s).
#[derive(Debug, Clone, Default)]
pub struct RateLimitConfig {
    /// Override for MarketData category (default 5/s).
    pub market_data_per_sec: Option<u32>,
    /// Override for Orders category (default 3/s).
    pub orders_per_sec: Option<u32>,
    /// Override for Account category (default 1/s).
    pub account_per_sec: Option<u32>,
    /// Override for Auth category (default 1/s).
    pub auth_per_sec: Option<u32>,
}

/// Public client configuration.
///
/// ## Ergonomic construction
///
/// Use [`LsConfig::from_env`] to load credentials from environment variables.
///
/// ## Manual construction
///
/// For test injection or advanced use, construct via struct literal with all
/// optional fields set to `None` / `false`.
///
/// `Debug` is implemented manually and redacts credential-bearing fields.
/// `Display` is deliberately NOT implemented — printing a config risks
/// leaking credentials.
#[derive(Clone)]
pub struct LsConfig {
    /// Customer app key from LS Open API console.
    pub appkey: String,
    /// Customer app secret from LS Open API console.
    pub appsecretkey: String,
    /// Account number (CANO) — required for order and account TRs.
    pub account_no: String,
    /// Which environment to target (Real or Paper).
    pub environment: Environment,
    /// Optional per-bucket rate limit config.
    pub rate_limits: Option<RateLimitConfig>,
    /// Optional base-URL override for testing.
    ///
    /// - `None` (production default): runtime uses `environment.base_url()`.
    /// - `Some(url)`: runtime uses `url` verbatim (tests inject a wiremock
    ///   server URL here; production callers never set this).
    ///
    /// All runtime HTTP dispatch points MUST resolve via
    /// `Environment::resolve_base_url(&config)` — this keeps the override
    /// in effect everywhere without scattering the fallback logic.
    pub base_url: Option<String>,
    /// Optional WebSocket base-URL override for testing.
    ///
    /// - `None` (production default): runtime uses `environment.ws_url()`.
    /// - `Some(url)`: runtime uses `url` verbatim (WS integration tests inject
    ///   a local `ws://127.0.0.1:<port>` mock-server URL here).
    ///
    /// All WS connection points MUST resolve via `resolve_ws_url(&config)`.
    pub ws_base_url: Option<String>,
    /// Maximum number of pages to fetch in `collect_all`.
    ///
    /// - `None`: use the SDK default cap of 100 pages.
    /// - `Some(n)`: stop after `n` pages even if the server signals more.
    pub max_pages: Option<usize>,
    /// Connection timeout in seconds for the TCP connect phase of HTTP requests.
    /// Applied via `reqwest::ClientBuilder::connect_timeout`.
    /// - `None`: use SDK default of 10 s
    /// - `Some(n)`: override with caller-supplied value
    pub connect_timeout_secs: Option<u64>,
    /// Total request timeout in seconds (connect + send + read body).
    /// Applied via `reqwest::ClientBuilder::timeout`.
    /// - `None`: use SDK default of 30 s
    /// - `Some(n)`: override with caller-supplied value
    pub request_timeout_secs: Option<u64>,
    /// WebSocket connection timeout in seconds, applied via `tokio::time::timeout`
    /// wrapping `tokio_tungstenite::connect_async`.
    /// - `None`: use SDK default of 15 s
    /// - `Some(n)`: override with caller-supplied value
    pub ws_connect_timeout_secs: Option<u64>,
    /// When `true`, permits `http://` REST base URLs and `ws://` WebSocket base
    /// URLs that point to loopback addresses (`127.0.0.1`, `::1`, `localhost`).
    /// MUST be `false` (the default) in production — setting `true` enables test
    /// injection via local mock servers.
    /// Does NOT affect non-loopback hosts — those always require `https://`/`wss://`.
    pub allow_insecure_localhost: bool,
    /// Bounded channel depth per WebSocket subscription.
    ///
    /// - `None`: use SDK default of 64 frames per subscription.
    /// - `Some(n)`: override with caller-supplied value. Must be >= 1;
    ///   resolution returns `LsError::Config` if `Some(0)`.
    pub ws_channel_capacity: Option<usize>,
    /// Overflow policy when a subscriber's channel is full.
    ///
    /// - `None`: use SDK default of [`WsOverflowPolicy::DropNewest`].
    /// - `Some(policy)`: apply the given policy.
    pub ws_overflow_policy: Option<WsOverflowPolicy>,
}

impl LsConfig {
    /// Load configuration from environment variables.
    ///
    /// Reads `LS_TRADING_ENV` to determine which credential set to load:
    /// - `"paper"` → reads `LS_PAPER_APPKEY`, `LS_PAPER_SECRET`, `LS_PAPER_ACCOUNT`
    /// - `"real"` → reads `LS_REAL_APPKEY`, `LS_REAL_SECRET`, `LS_REAL_ACCOUNT`
    ///
    /// `LS_TRADING_ENV` accepts only `paper` and `real`; any other value is an
    /// error. If `LS_TRADING_ENV` is unset, defaults to `"paper"`.
    ///
    /// Each credential falls back to the legacy name if the env-specific one
    /// is missing:
    /// - `LS_APPKEY`  (if `LS_PAPER_APPKEY` / `LS_REAL_APPKEY` missing)
    /// - `LS_SECRET`  (if `LS_PAPER_SECRET` / `LS_REAL_SECRET` missing)
    /// - `LS_ACCOUNT` (if `LS_PAPER_ACCOUNT` / `LS_REAL_ACCOUNT` missing)
    ///
    /// All optional fields (`rate_limits`, `base_url`, `ws_base_url`, etc.)
    /// use their defaults (`None` / `false`).
    ///
    /// # Errors
    ///
    /// Returns [`LsError::Config`] if a required environment variable is missing
    /// or if `LS_TRADING_ENV` contains an unrecognised value.
    pub fn from_env() -> LsResult<Self> {
        let env_raw = std::env::var("LS_TRADING_ENV").unwrap_or_else(|_| "paper".into());
        let environment = env_raw.parse::<Environment>()?;

        let (appkey_var, secret_var, account_var) = if environment.is_paper() {
            ("LS_PAPER_APPKEY", "LS_PAPER_SECRET", "LS_PAPER_ACCOUNT")
        } else {
            ("LS_REAL_APPKEY", "LS_REAL_SECRET", "LS_REAL_ACCOUNT")
        };

        let appkey = env_with_fallback(appkey_var, "LS_APPKEY")?;
        let appsecretkey = env_with_fallback(secret_var, "LS_SECRET")?;
        let account_no = env_with_fallback(account_var, "LS_ACCOUNT")?;

        Ok(LsConfig {
            appkey,
            appsecretkey,
            account_no,
            environment,
            rate_limits: None,
            base_url: None,
            ws_base_url: None,
            max_pages: None,
            connect_timeout_secs: None,
            request_timeout_secs: None,
            ws_connect_timeout_secs: None,
            allow_insecure_localhost: false,
            ws_channel_capacity: None,
            ws_overflow_policy: None,
        })
    }
}

/// Resolve a credential through the primary-then-legacy env-var fallback chain.
/// Reads the environment at call time — no caching.
///
/// The error-message format `missing env var: {primary} (or {legacy})` is
/// pinned byte-for-byte by `env_with_fallback_missing_both_exact_message`.
fn env_with_fallback(primary: &str, legacy: &str) -> LsResult<String> {
    std::env::var(primary)
        .or_else(|_| std::env::var(legacy))
        .map_err(|_| LsError::Config(format!("missing env var: {primary} (or {legacy})")))
}

impl std::fmt::Debug for LsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LsConfig")
            .field("appkey", &"<redacted>")
            .field("appsecretkey", &"<redacted>")
            .field("account_no", &"<redacted>")
            .field("environment", &self.environment)
            .field("rate_limits", &self.rate_limits)
            .field("base_url", &self.base_url)
            .field("ws_base_url", &self.ws_base_url)
            .field("max_pages", &self.max_pages)
            .field("connect_timeout_secs", &self.connect_timeout_secs)
            .field("request_timeout_secs", &self.request_timeout_secs)
            .field("ws_connect_timeout_secs", &self.ws_connect_timeout_secs)
            .field("allow_insecure_localhost", &self.allow_insecure_localhost)
            .field("ws_channel_capacity", &self.ws_channel_capacity)
            .field("ws_overflow_policy", &self.ws_overflow_policy)
            .finish()
    }
}

#[cfg(test)]
pub(crate) fn test_config() -> LsConfig {
    LsConfig {
        appkey: "test-appkey".into(),
        appsecretkey: "test-appsecretkey".into(),
        account_no: "00000000-01".into(),
        environment: Environment::Paper,
        rate_limits: None,
        base_url: None,
        ws_base_url: None,
        max_pages: None,
        connect_timeout_secs: None,
        request_timeout_secs: None,
        ws_connect_timeout_secs: None,
        allow_insecure_localhost: false,
        ws_channel_capacity: None,
        ws_overflow_policy: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Env vars are process-global; the env-touching tests serialize on this lock
    // and use unique var names, saving/restoring shared LS_* vars so they never
    // race each other.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn clear(vars: &[&str]) {
        for v in vars {
            std::env::remove_var(v);
        }
    }

    #[test]
    fn ws_overflow_policy_default_is_drop_newest() {
        assert_eq!(WsOverflowPolicy::default(), WsOverflowPolicy::DropNewest);
    }

    #[test]
    fn environment_from_str_canonical_only() {
        // Only the two canonical names parse — case-insensitively.
        assert_eq!("paper".parse::<Environment>().unwrap(), Environment::Paper);
        assert_eq!("Paper".parse::<Environment>().unwrap(), Environment::Paper);
        assert_eq!("real".parse::<Environment>().unwrap(), Environment::Real);
        assert_eq!("REAL".parse::<Environment>().unwrap(), Environment::Real);
        // Former aliases are now hard errors.
        for alias in ["simulation", "sim", "production", "prod", "bogus"] {
            assert!(
                alias.parse::<Environment>().is_err(),
                "{alias} should no longer parse"
            );
        }
    }

    #[test]
    fn environment_display_is_symmetric() {
        assert_eq!(Environment::Paper.to_string(), "paper");
        assert_eq!(Environment::Real.to_string(), "real");
    }

    /// Snapshot the LS_* env vars this test family touches, returning a restorer.
    fn save_ls_env() -> Vec<(&'static str, Option<String>)> {
        const VARS: &[&str] = &[
            "LS_TRADING_ENV",
            "LS_PAPER_APPKEY",
            "LS_PAPER_SECRET",
            "LS_PAPER_ACCOUNT",
            "LS_REAL_APPKEY",
            "LS_REAL_SECRET",
            "LS_REAL_ACCOUNT",
            "LS_APPKEY",
            "LS_SECRET",
            "LS_ACCOUNT",
        ];
        VARS.iter().map(|v| (*v, std::env::var(v).ok())).collect()
    }

    fn restore_ls_env(saved: Vec<(&'static str, Option<String>)>) {
        for (v, val) in saved {
            match val {
                Some(s) => std::env::set_var(v, s),
                None => std::env::remove_var(v),
            }
        }
    }

    #[test]
    fn from_env_paper_resolves_paper() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved = save_ls_env();

        clear(&[
            "LS_TRADING_ENV",
            "LS_PAPER_APPKEY",
            "LS_PAPER_SECRET",
            "LS_PAPER_ACCOUNT",
            "LS_APPKEY",
            "LS_SECRET",
            "LS_ACCOUNT",
        ]);
        std::env::set_var("LS_TRADING_ENV", "paper");
        std::env::set_var("LS_PAPER_APPKEY", "paper-key");
        std::env::set_var("LS_PAPER_SECRET", "paper-secret");
        std::env::set_var("LS_PAPER_ACCOUNT", "paper-account");

        let cfg = LsConfig::from_env().expect("from_env should succeed");
        assert_eq!(cfg.environment, Environment::Paper);
        assert_eq!(cfg.appkey, "paper-key");
        assert_eq!(cfg.appsecretkey, "paper-secret");
        assert_eq!(cfg.account_no, "paper-account");

        restore_ls_env(saved);
    }

    #[test]
    fn from_env_legacy_fallback_resolves() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved = save_ls_env();

        clear(&[
            "LS_TRADING_ENV",
            "LS_PAPER_APPKEY",
            "LS_PAPER_SECRET",
            "LS_PAPER_ACCOUNT",
            "LS_APPKEY",
            "LS_SECRET",
            "LS_ACCOUNT",
        ]);
        // No LS_PAPER_APPKEY — legacy LS_APPKEY must be used instead.
        std::env::set_var("LS_TRADING_ENV", "paper");
        std::env::set_var("LS_APPKEY", "legacy-key");
        std::env::set_var("LS_PAPER_SECRET", "paper-secret");
        std::env::set_var("LS_PAPER_ACCOUNT", "paper-account");

        let cfg = LsConfig::from_env().expect("from_env should succeed via legacy fallback");
        assert_eq!(cfg.appkey, "legacy-key");
        assert_eq!(cfg.appsecretkey, "paper-secret");
        assert_eq!(cfg.account_no, "paper-account");

        restore_ls_env(saved);
    }

    #[test]
    fn from_env_missing_required_var_errors() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved = save_ls_env();

        clear(&[
            "LS_TRADING_ENV",
            "LS_PAPER_APPKEY",
            "LS_PAPER_SECRET",
            "LS_PAPER_ACCOUNT",
            "LS_APPKEY",
            "LS_SECRET",
            "LS_ACCOUNT",
        ]);
        std::env::set_var("LS_TRADING_ENV", "paper");
        // Deliberately set nothing else — appkey resolution must fail.

        let err = LsConfig::from_env().expect_err("missing creds should fail");
        match err {
            LsError::Config(msg) => {
                assert!(msg.contains("LS_PAPER_APPKEY"), "got: {msg}");
            }
            other => panic!("expected LsError::Config, got {other:?}"),
        }

        restore_ls_env(saved);
    }

    #[test]
    fn env_with_fallback_primary_set_returns_primary() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear(&["U3_TEST_PRIMARY_A", "U3_TEST_LEGACY_A"]);
        std::env::set_var("U3_TEST_PRIMARY_A", "primary-value");

        let got = env_with_fallback("U3_TEST_PRIMARY_A", "U3_TEST_LEGACY_A").unwrap();
        assert_eq!(got, "primary-value");
        clear(&["U3_TEST_PRIMARY_A"]);
    }

    #[test]
    fn env_with_fallback_primary_unset_returns_legacy() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear(&["U3_TEST_PRIMARY_B", "U3_TEST_LEGACY_B"]);
        std::env::set_var("U3_TEST_LEGACY_B", "legacy-value");

        let got = env_with_fallback("U3_TEST_PRIMARY_B", "U3_TEST_LEGACY_B").unwrap();
        assert_eq!(got, "legacy-value");
        clear(&["U3_TEST_LEGACY_B"]);
    }

    #[test]
    fn env_with_fallback_both_set_primary_wins() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear(&["U3_TEST_PRIMARY_C", "U3_TEST_LEGACY_C"]);
        std::env::set_var("U3_TEST_PRIMARY_C", "primary-wins");
        std::env::set_var("U3_TEST_LEGACY_C", "legacy-loses");

        let got = env_with_fallback("U3_TEST_PRIMARY_C", "U3_TEST_LEGACY_C").unwrap();
        assert_eq!(got, "primary-wins");
        clear(&["U3_TEST_PRIMARY_C", "U3_TEST_LEGACY_C"]);
    }

    #[test]
    fn env_with_fallback_missing_both_exact_message() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear(&["U3_TEST_PRIMARY_D", "U3_TEST_LEGACY_D"]);

        let err = env_with_fallback("U3_TEST_PRIMARY_D", "U3_TEST_LEGACY_D").unwrap_err();
        match err {
            LsError::Config(msg) => {
                // Exact-match pin on the full error format.
                assert_eq!(
                    msg,
                    "missing env var: U3_TEST_PRIMARY_D (or U3_TEST_LEGACY_D)"
                );
            }
            other => panic!("expected LsError::Config, got {other:?}"),
        }
    }

    #[test]
    fn debug_redacts_credentials() {
        let cfg = test_config();
        let dbg = format!("{cfg:?}");
        // The known test appkey must never appear in Debug output.
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
