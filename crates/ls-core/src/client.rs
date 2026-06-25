//! Public entry type `LsClient` — the thin, order-free transport facade.
//!
//! `LsClient` validates config (credentials + URL schemes), constructs the
//! shared `Arc<Inner>`, and exposes the non-order dispatch primitives
//! (`post`, `post_paginated`, `collect_all`) plus `revoke_token`. The typed
//! per-TR surface lives in `ls-sdk`, which builds on these primitives.

use std::sync::Arc;

use crate::endpoint_policy::EndpointPolicy;
use crate::inner::Inner;
use crate::pagination::HasPagination;
use crate::{LsConfig, LsError, LsResult};

/// Validate that `url` uses an allowed scheme.
///
/// Only `allowed_schemes` are permitted. When `allow_loopback` is true, loopback
/// hosts (`127.0.0.1`, `[::1]`, `localhost`) are also accepted for `http`/`ws`.
/// Synchronous — no I/O, no DNS. Returns `LsError::Config` on violation, naming
/// the offending field.
fn validate_url_scheme(
    url: &str,
    allowed_schemes: &[&str],
    allow_loopback: bool,
    field_name: &str,
) -> LsResult<()> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|e| LsError::Config(format!("{field_name}: invalid URL: {e}")))?;

    let scheme = parsed.scheme();
    if allowed_schemes.contains(&scheme) {
        return Ok(());
    }

    if allow_loopback && (scheme == "http" || scheme == "ws") {
        let host = parsed.host_str().unwrap_or("");
        // `host_str()` returns "[::1]" WITH brackets for IPv6 loopback.
        if host == "127.0.0.1" || host == "[::1]" || host == "localhost" {
            return Ok(());
        }
    }

    Err(LsError::Config(format!(
        "{field_name}: scheme '{scheme}://' is not in the allowed list: {}",
        allowed_schemes.join(", ")
    )))
}

/// Public client — constructed via `LsClient::new(config)`.
///
/// Holds `Arc<Inner>` so it (and any `ls-sdk` accessors built on it) can cheaply
/// share runtime state.
pub struct LsClient {
    /// Shared transport core.
    pub inner: Arc<Inner>,
}

impl LsClient {
    /// Validate config and build the client. Synchronous — no network I/O.
    ///
    /// Fails fast if any required credential is empty. The OAuth2 token is NOT
    /// fetched at construction; it is fetched lazily on the first HTTP call, so
    /// `new` is callable outside a Tokio runtime.
    pub fn new(config: LsConfig) -> LsResult<Self> {
        if config.appkey.is_empty() {
            return Err(LsError::Auth("appkey is required".into()));
        }
        if config.appsecretkey.is_empty() {
            return Err(LsError::Auth("appsecretkey is required".into()));
        }
        if config.account_no.is_empty() {
            return Err(LsError::Auth("account_no is required".into()));
        }

        // Allowlist URL validation: base_url must be https (or http loopback with
        // opt-in); ws_base_url must be wss (or ws loopback with opt-in).
        let rest_url = crate::config::Environment::resolve_base_url(&config);
        validate_url_scheme(
            &rest_url,
            &["https"],
            config.allow_insecure_localhost,
            "base_url",
        )?;
        let ws_url = crate::config::Environment::resolve_ws_url(&config);
        validate_url_scheme(
            &ws_url,
            &["wss"],
            config.allow_insecure_localhost,
            "ws_base_url",
        )?;

        let inner = Inner::new(config)?;
        Ok(LsClient { inner })
    }
}

impl Clone for LsClient {
    /// Cheap clone — shares `Arc<Inner>`.
    fn clone(&self) -> Self {
        LsClient {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl LsClient {
    /// Raw authenticated POST with retry + rate limiting — the primitive behind
    /// every non-order typed accessor in `ls-sdk`.
    pub async fn post<Req, Res>(&self, policy: &EndpointPolicy, req: &Req) -> LsResult<Res>
    where
        Req: serde::Serialize + Sync,
        Res: serde::de::DeserializeOwned + Send,
    {
        self.inner.post(policy, req).await
    }

    /// Paginated authenticated POST with retry + rate limiting — the primitive
    /// behind every paginated typed accessor in `ls-sdk`.
    pub async fn post_paginated<Req, Res>(
        &self,
        policy: &EndpointPolicy,
        req: &Req,
    ) -> LsResult<Res>
    where
        Req: HasPagination + serde::Serialize + Sync,
        Res: serde::de::DeserializeOwned + Send,
    {
        self.inner.post_paginated(policy, req).await
    }

    /// Collect all pages of a paginated TR — loops until the `tr_cont` response
    /// header is empty/`"N"` or `max_pages` is reached.
    pub async fn collect_all<Req, Res, F, Fut>(&self, req: Req, f: F) -> LsResult<Vec<Res>>
    where
        Req: HasPagination + Clone + Send + serde::Serialize,
        Res: HasPagination + serde::de::DeserializeOwned + Send,
        F: Fn(Req) -> Fut,
        Fut: std::future::Future<Output = LsResult<Res>> + Send,
    {
        self.inner.collect_all(req, f).await
    }

    /// Engage or release the global order kill switch (order-safety §1).
    ///
    /// `set_orders_enabled(false)` is the operator emergency halt: every
    /// subsequent order dispatch halts before dedup, rate limiting, or HTTP I/O.
    /// Non-order reads (`post`/`post_paginated`) are unaffected.
    pub fn set_orders_enabled(&self, enabled: bool) {
        self.inner.set_orders_enabled(enabled);
    }

    /// `true` if order dispatch is currently enabled (the default).
    pub fn orders_enabled(&self) -> bool {
        self.inner.orders_enabled()
    }

    /// Revoke the in-memory bearer token.
    ///
    /// Idempotent: if no token is cached, returns `Ok(())`. Otherwise POSTs to
    /// the revoke endpoint and clears the cache on success.
    pub async fn revoke_token(&self) -> LsResult<()> {
        let Some(token) = self.inner.token_manager.snapshot_token().await else {
            return Ok(());
        };
        self.inner
            .rate_limiter
            .wait(crate::RateLimitCategory::Auth)
            .await;
        crate::auth::revoke_token_http(&self.inner.client, &self.inner.config, &token).await?;
        self.inner.token_manager.clear().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Environment, LsError};

    #[test]
    fn new_rejects_empty_appkey() {
        let mut cfg = crate::config::test_config();
        cfg.appkey = String::new();
        assert!(matches!(LsClient::new(cfg), Err(LsError::Auth(ref m)) if m.contains("appkey")));
    }

    #[test]
    fn new_is_synchronous_and_does_no_io() {
        assert!(LsClient::new(crate::config::test_config()).is_ok());
    }

    #[test]
    fn url_validation_rejects_http_non_loopback() {
        let cfg = LsConfig {
            base_url: Some("http://example.com/api".into()),
            allow_insecure_localhost: false,
            ..crate::config::test_config()
        };
        assert!(matches!(LsClient::new(cfg), Err(LsError::Config(_))));
    }

    #[test]
    fn url_validation_loopback_with_flag_succeeds() {
        let cfg = LsConfig {
            base_url: Some("http://127.0.0.1:8080".into()),
            allow_insecure_localhost: true,
            ..crate::config::test_config()
        };
        assert!(LsClient::new(cfg).is_ok());
    }

    #[test]
    fn production_https_defaults_pass() {
        let cfg = LsConfig {
            environment: Environment::Real,
            ..crate::config::test_config()
        };
        assert!(LsClient::new(cfg).is_ok());
    }

    #[test]
    fn url_validation_rejects_ftp() {
        let cfg = LsConfig {
            base_url: Some("ftp://example.com/api".into()),
            ..crate::config::test_config()
        };
        assert!(matches!(LsClient::new(cfg), Err(LsError::Config(_))));
    }
}
