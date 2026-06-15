//! OAuth2 bearer-token lifecycle.
//!
//! Responsibilities:
//! - Hold the in-memory token (`TokenData`) under a `tokio::sync::RwLock`
//! - Fetch a new token from `/oauth2/token` when cache is empty or expiring
//! - Hand callers a valid Bearer string on demand
//! - Support explicit revocation (`/oauth2/revoke`) with cache clear
//!
//! Refresh strategy is lazy: before returning a token, the cache is checked;
//! if it expires within 5 minutes it is refreshed inline.
//!
//! The LS spec field name is `expire_in` (no trailing 's'). The example JSON
//! in the spec mistakenly shows `expires_in`; we accept both via a serde alias
//! and fall back to a 24-hour default when the field is absent.
//!
//! URL resolution flows through `ResolvedConfig.base_url`, which is produced by
//! `Environment::resolve_base_url` — the single choke point that honors
//! `LsConfig.base_url` for test injection.

use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::config_resolve::ResolvedConfig;
use crate::rate_limiter::{RateLimitCategory, RateLimiterManager};
use crate::{LsError, LsResult};

/// In-memory token snapshot. `expires_at` is computed at fetch time by adding
/// the server-reported `expire_in` seconds to `Utc::now()`.
#[derive(Clone)]
pub struct TokenData {
    /// Bearer token string returned by LS.
    pub access_token: String,
    /// Absolute expiry instant (UTC). Computed from the `expire_in` TTL.
    pub expires_at: DateTime<Utc>,
}

impl TokenData {
    /// True if the token expires within the next 5 minutes (300s). Drives the
    /// refresh decision in `TokenManager::get_or_refresh`.
    pub fn is_expiring_soon(&self) -> bool {
        Utc::now() + Duration::seconds(300) >= self.expires_at
    }
}

impl std::fmt::Debug for TokenData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenData")
            .field("access_token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Thread-safe token cache. Construct once and share via `&`.
pub struct TokenManager {
    token: RwLock<Option<TokenData>>,
}

impl TokenManager {
    /// Construct an empty cache. Tokens are fetched lazily on first use.
    pub fn new() -> Self {
        TokenManager {
            token: RwLock::new(None),
        }
    }

    /// Return a valid access token, fetching or refreshing if needed.
    ///
    /// Double-checked-lock pattern:
    /// 1. Acquire the read lock — if a token is present and not expiring soon,
    ///    return it.
    /// 2. Drop the read lock, acquire the write lock.
    /// 3. Re-check (another task may have refreshed while we waited).
    /// 4. If still stale, charge the `Auth` bucket, call `fetch_token`, store it.
    ///
    /// The re-check on the write lock is MANDATORY — without it, two
    /// simultaneously-expired callers would both issue a network request.
    pub async fn get_or_refresh(
        &self,
        client: &reqwest::Client,
        config: &ResolvedConfig,
        rate_limiter: &RateLimiterManager,
    ) -> LsResult<String> {
        // Fast path: read-lock check.
        {
            let guard = self.token.read().await;
            if let Some(ref td) = *guard {
                if !td.is_expiring_soon() {
                    return Ok(td.access_token.clone());
                }
            }
        } // read guard dropped here

        // Slow path: write-lock check + fetch.
        let mut guard = self.token.write().await;
        // Re-check — another task may have refreshed while we awaited the lock.
        if let Some(ref td) = *guard {
            if !td.is_expiring_soon() {
                return Ok(td.access_token.clone());
            }
        }
        // Charge the Auth bucket only when an actual fetch is about to happen.
        rate_limiter.wait(RateLimitCategory::Auth).await;
        let td = fetch_token(client, config).await?;
        let token_str = td.access_token.clone();
        *guard = Some(td);
        Ok(token_str)
    }

    /// Clear the in-memory token (called after a successful revoke).
    pub async fn clear(&self) {
        let mut guard = self.token.write().await;
        *guard = None;
    }

    /// Seed the cache with a known [`TokenData`] directly, bypassing the network.
    ///
    /// This is the cross-crate test seam: `ls-sdk`'s WebSocket ordering proof
    /// (`subscribe_records_subscription_before_outbound_send`) seeds a
    /// far-future token so `get_or_refresh` takes the cache fast path — no HTTP,
    /// no rate-limiter wait — which is what lets the test inject a *synchronous*
    /// send failure at the exact step after the ordering point. In the old
    /// single-crate runtime the test wrote the `pub(crate)` `token` field
    /// directly; the maintained layout puts the WS manager in a separate crate,
    /// so the seam is exposed as this explicit, narrowly-documented method
    /// rather than as field visibility.
    pub async fn seed_token(&self, token: TokenData) {
        let mut guard = self.token.write().await;
        *guard = Some(token);
    }

    /// Return the current access token if any. Uses a read lock and does NOT
    /// trigger a refresh.
    pub async fn snapshot_token(&self) -> Option<String> {
        let guard = self.token.read().await;
        guard.as_ref().map(|td| td.access_token.clone())
    }
}

impl Default for TokenManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Fetch a fresh bearer token from `<base_url>/oauth2/token`.
///
/// Form-encoded POST per LS spec. Computes absolute expiry as
/// `Utc::now() + expire_in`. A 24-hour default is used when the field is
/// absent; a zero or negative TTL fails closed with [`LsError::Auth`].
///
/// `#[tracing::instrument(skip_all)]` ensures the `client`/`config` arguments
/// (which carry the appkey/secret) never land in span fields.
#[tracing::instrument(skip_all)]
pub(crate) async fn fetch_token(
    client: &reqwest::Client,
    config: &ResolvedConfig,
) -> LsResult<TokenData> {
    let url = format!("{}/oauth2/token", config.base_url);
    let resp = client
        .post(&url)
        .form(&[
            ("grant_type", "client_credentials"),
            ("appkey", config.appkey.as_str()),
            ("appsecretkey", config.appsecretkey.as_str()),
            ("scope", "oob"),
        ])
        .send()
        .await
        .map_err(LsError::Http)?;

    if !resp.status().is_success() {
        return Err(LsError::Http(resp.error_for_status().unwrap_err()));
    }

    // A 200 can still carry a `{ code, message }` envelope on business failure
    // (e.g. invalid credentials). Inspect it before deserializing the token.
    let val: serde_json::Value = resp.json().await.map_err(LsError::Http)?;
    check_envelope(&val)?;

    #[derive(Deserialize)]
    struct TokenResponse {
        #[serde(deserialize_with = "crate::string_or_number")]
        access_token: String,
        #[serde(
            default,
            alias = "expires_in",
            deserialize_with = "crate::option_string_or_number"
        )]
        expire_in: Option<String>,
    }

    let body: TokenResponse = serde_json::from_value(val).map_err(LsError::Decode)?;
    // `expire_in` is a stringified integer (e.g. "86400"). Reject non-numeric,
    // zero, or negative TTLs — fail closed.
    let ttl_seconds: i64 = match body.expire_in {
        Some(v) => v.parse().map_err(|_| {
            LsError::Auth(format!("token response contained invalid expire_in: {v}"))
        })?,
        None => 86_400, // 24-hour default when the API omits expiry
    };
    if ttl_seconds <= 0 {
        return Err(LsError::Auth(format!(
            "token response contained non-positive expire_in: {ttl_seconds}"
        )));
    }
    let expires_at = Utc::now() + Duration::seconds(ttl_seconds);

    Ok(TokenData {
        access_token: body.access_token,
        expires_at,
    })
}

/// Revoke an access token at `<base_url>/oauth2/revoke`.
///
/// `#[tracing::instrument(skip_all)]` ensures the `token`/`config` arguments
/// never land in span fields.
#[tracing::instrument(skip_all)]
pub async fn revoke_token_http(
    client: &reqwest::Client,
    config: &ResolvedConfig,
    token: &str,
) -> LsResult<()> {
    let url = format!("{}/oauth2/revoke", config.base_url);
    let resp = client
        .post(url)
        .form(&[
            ("appkey", config.appkey.as_str()),
            ("appsecretkey", config.appsecretkey.as_str()),
            ("token_type_hint", "access_token"),
            ("token", token),
        ])
        .send()
        .await
        .map_err(LsError::Http)?;
    if !resp.status().is_success() {
        return Err(LsError::Http(resp.error_for_status().unwrap_err()));
    }
    // Parse the body even on 2xx — LS OAuth can return `{ code, message }` with
    // a non-success code while the HTTP status is 200.
    let val: serde_json::Value = resp.json().await.map_err(LsError::Http)?;
    check_envelope(&val)?;
    Ok(())
}

/// Treat a `{ code, message }` envelope as success when `code` is empty,
/// `"0000"`, or `"00000"`; otherwise surface [`LsError::ApiError`].
fn check_envelope(val: &serde_json::Value) -> LsResult<()> {
    if let Some(code) = val.get("code").and_then(|v| v.as_str()) {
        if !code.is_empty() && code != "0000" && code != "00000" {
            let message = val
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            return Err(LsError::ApiError {
                code: code.to_string(),
                message,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LsConfig;
    use crate::config_resolve::ResolvedConfig;
    use crate::rate_limiter::RateLimiterManager;
    use crate::{Environment, RateLimitConfig};
    use std::sync::Arc;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Build a `ResolvedConfig` pointed at the given wiremock URI, with a high
    /// auth rate limit so the rate limiter never throttles the test.
    fn resolved_for(base_url: &str) -> ResolvedConfig {
        let cfg = LsConfig {
            appkey: "test-appkey".into(),
            appsecretkey: "test-appsecretkey".into(),
            account_no: "00000000-01".into(),
            environment: Environment::Simulation,
            rate_limits: None,
            base_url: Some(base_url.to_string()),
            ws_base_url: None,
            max_pages: None,
            connect_timeout_secs: None,
            request_timeout_secs: None,
            ws_connect_timeout_secs: None,
            allow_insecure_localhost: true,
            ws_channel_capacity: None,
            ws_overflow_policy: None,
        };
        ResolvedConfig::from_raw(&cfg).expect("resolve config")
    }

    /// Rate limiter with a generous auth quota so tests are not slowed by the
    /// 1/s default.
    fn fast_rate_limiter() -> RateLimiterManager {
        let limits = crate::config_resolve::ResolvedRateLimits::from_raw(&Some(RateLimitConfig {
            market_data_per_sec: Some(1000),
            orders_per_sec: Some(1000),
            account_per_sec: Some(1000),
            auth_per_sec: Some(1000),
        }));
        RateLimiterManager::new(&limits).expect("build rate limiter")
    }

    async fn mount_token(server: &MockServer, token: &str, expire_in: Option<&str>) -> Mock {
        let mut body = serde_json::json!({
            "access_token": token,
            "scope": "oob",
            "token_type": "Bearer",
        });
        if let Some(e) = expire_in {
            body["expire_in"] = serde_json::json!(e);
        }
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
    }

    // -----------------------------------------------------------------------
    // EXECUTION NOTE: this concurrency contract test is written first.
    // -----------------------------------------------------------------------

    /// Two concurrent `get_or_refresh` callers against a fresh manager trigger
    /// the OAuth2 token fetch EXACTLY ONCE (the double-checked-lock contract).
    #[tokio::test]
    async fn two_concurrent_get_or_refresh_fetch_token_exactly_once() {
        let server = MockServer::start().await;
        mount_token(&server, "tok_concurrent", Some("86400"))
            .await
            .expect(1) // <- the contract: the token endpoint is hit exactly once
            .mount(&server)
            .await;

        let manager = Arc::new(TokenManager::new());
        let config = Arc::new(resolved_for(&server.uri()));
        let rate_limiter = Arc::new(fast_rate_limiter());
        let client = reqwest::Client::new();
        let client_a = client.clone();
        let client_b = client.clone();

        let m1 = manager.clone();
        let c1 = config.clone();
        let r1 = rate_limiter.clone();
        let m2 = manager.clone();
        let c2 = config.clone();
        let r2 = rate_limiter.clone();

        let (a, b) = tokio::join!(
            tokio::spawn(async move { m1.get_or_refresh(&client_a, &c1, &r1).await }),
            tokio::spawn(async move { m2.get_or_refresh(&client_b, &c2, &r2).await }),
        );

        let tok_a = a.unwrap().expect("call A ok");
        let tok_b = b.unwrap().expect("call B ok");
        assert_eq!(tok_a, "tok_concurrent");
        assert_eq!(tok_b, "tok_concurrent");
        // wiremock asserts `.expect(1)` on server drop.
    }

    /// Happy path: `get_or_refresh` fetches once, caches, and returns the
    /// cached token on a second call within expiry.
    #[tokio::test]
    async fn get_or_refresh_caches_token_within_expiry() {
        let server = MockServer::start().await;
        mount_token(&server, "tok_cached", Some("86400"))
            .await
            .expect(1)
            .mount(&server)
            .await;

        let manager = TokenManager::new();
        let config = resolved_for(&server.uri());
        let rate_limiter = fast_rate_limiter();
        let client = reqwest::Client::new();

        let first = manager
            .get_or_refresh(&client, &config, &rate_limiter)
            .await
            .expect("first fetch");
        let second = manager
            .get_or_refresh(&client, &config, &rate_limiter)
            .await
            .expect("cached");
        assert_eq!(first, "tok_cached");
        assert_eq!(second, "tok_cached");
        assert_eq!(
            manager.snapshot_token().await,
            Some("tok_cached".to_string())
        );
    }

    /// Edge: `expire_in` absent → 24-hour default (token is not expiring soon).
    #[tokio::test]
    async fn fetch_token_defaults_to_24h_when_expire_in_absent() {
        let server = MockServer::start().await;
        mount_token(&server, "tok_default", None)
            .await
            .mount(&server)
            .await;

        let config = resolved_for(&server.uri());
        let client = reqwest::Client::new();
        let td = fetch_token(&client, &config).await.expect("fetch");
        assert!(!td.is_expiring_soon());
        // ~24h out: comfortably more than 23 hours from now.
        assert!(td.expires_at > Utc::now() + Duration::hours(23));
    }

    /// Edge: zero `expire_in` fails closed with `LsError::Auth`.
    #[tokio::test]
    async fn fetch_token_zero_expire_in_fails_closed() {
        let server = MockServer::start().await;
        mount_token(&server, "tok_zero", Some("0"))
            .await
            .mount(&server)
            .await;

        let config = resolved_for(&server.uri());
        let client = reqwest::Client::new();
        let err = fetch_token(&client, &config).await.unwrap_err();
        assert!(
            matches!(err, LsError::Auth(_)),
            "expected Auth, got {err:?}"
        );
    }

    /// Edge: negative `expire_in` fails closed with `LsError::Auth`.
    #[tokio::test]
    async fn fetch_token_negative_expire_in_fails_closed() {
        let server = MockServer::start().await;
        mount_token(&server, "tok_neg", Some("-5"))
            .await
            .mount(&server)
            .await;

        let config = resolved_for(&server.uri());
        let client = reqwest::Client::new();
        let err = fetch_token(&client, &config).await.unwrap_err();
        assert!(
            matches!(err, LsError::Auth(_)),
            "expected Auth, got {err:?}"
        );
    }

    /// `expires_in` (standard OAuth2 spelling) is accepted via the serde alias.
    #[tokio::test]
    async fn fetch_token_accepts_expires_in_alias() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "access_token": "tok_alias",
            "expires_in": "86400",
        });
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let config = resolved_for(&server.uri());
        let client = reqwest::Client::new();
        let td = fetch_token(&client, &config).await.expect("fetch");
        assert_eq!(td.access_token, "tok_alias");
        assert!(!td.is_expiring_soon());
    }

    /// Error: a 200 with a non-OK `{code,message}` envelope yields
    /// `LsError::ApiError` and caches NO bad token.
    #[tokio::test]
    async fn get_or_refresh_non_ok_envelope_caches_nothing() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "code": "IGW00002",
            "message": "invalid credentials",
        });
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let manager = TokenManager::new();
        let config = resolved_for(&server.uri());
        let rate_limiter = fast_rate_limiter();
        let client = reqwest::Client::new();

        let err = manager
            .get_or_refresh(&client, &config, &rate_limiter)
            .await
            .unwrap_err();
        assert!(
            matches!(err, LsError::ApiError { .. }),
            "expected ApiError, got {err:?}"
        );
        // No bad token cached.
        assert_eq!(manager.snapshot_token().await, None);
    }

    /// `revoke_token_http` posts `client_credentials`-shaped form with the
    /// `token_type_hint=access_token` and the token, succeeding on an OK
    /// envelope.
    #[tokio::test]
    async fn revoke_token_http_succeeds_on_ok_envelope() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/revoke"))
            .and(body_string_contains("token_type_hint=access_token"))
            .and(body_string_contains("token=tok_revoke"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": "0000",
                "message": "OK",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let config = resolved_for(&server.uri());
        let client = reqwest::Client::new();
        revoke_token_http(&client, &config, "tok_revoke")
            .await
            .expect("revoke ok");
    }

    /// `revoke_token_http` surfaces a non-OK envelope as `LsError::ApiError`.
    #[tokio::test]
    async fn revoke_token_http_non_ok_envelope_is_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/revoke"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": "IGW00121",
                "message": "already revoked",
            })))
            .mount(&server)
            .await;

        let config = resolved_for(&server.uri());
        let client = reqwest::Client::new();
        let err = revoke_token_http(&client, &config, "tok_revoke")
            .await
            .unwrap_err();
        assert!(
            matches!(err, LsError::ApiError { .. }),
            "expected ApiError, got {err:?}"
        );
    }

    /// `clear` empties the cache.
    #[tokio::test]
    async fn clear_empties_cache() {
        let server = MockServer::start().await;
        mount_token(&server, "tok_clear", Some("86400"))
            .await
            .mount(&server)
            .await;

        let manager = TokenManager::new();
        let config = resolved_for(&server.uri());
        let rate_limiter = fast_rate_limiter();
        let client = reqwest::Client::new();

        manager
            .get_or_refresh(&client, &config, &rate_limiter)
            .await
            .expect("fetch");
        assert!(manager.snapshot_token().await.is_some());
        manager.clear().await;
        assert_eq!(manager.snapshot_token().await, None);
    }

    /// The redacting `Debug` never prints the token.
    #[test]
    fn token_data_debug_redacts_access_token() {
        let td = TokenData {
            access_token: "super-secret-token".into(),
            expires_at: Utc::now(),
        };
        let rendered = format!("{td:?}");
        assert!(!rendered.contains("super-secret-token"));
        assert!(rendered.contains("<redacted>"));
    }
}
