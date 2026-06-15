//! Mock HTTP support — `mock_config` plus wiremock OAuth2 endpoint mounts.
//!
//! These helpers exercise the REAL `ls-core` dispatch code paths: `mock_config`
//! injects `base_url`, so every request resolves to the wiremock server through
//! the single `ResolvedConfig.base_url` choke point. The token/revoke mounts
//! return realistic LS OAuth2 envelopes.
//!
//! Reused across the dependency-class suites (U11–U14): keep these general and
//! ergonomic. Free functions take `&MockServer` so callers compose them with
//! their own TR-response mounts on the same server.

use ls_core::{Environment, LsConfig};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Canonical mock credentials, shared so credential-asserting mounts agree.
pub const TEST_APPKEY: &str = "test-appkey";
pub const TEST_APPSECRETKEY: &str = "test-appsecretkey";
pub const TEST_ACCOUNT_NO: &str = "00000000-01";
/// Token string issued by [`mount_token`].
pub const TEST_TOKEN: &str = "tok_test";
/// Default token TTL in seconds (24h) issued by [`mount_token`].
pub const DEFAULT_TOKEN_TTL_SECS: u64 = 86_400;

/// Build a mock `LsConfig` pointed at `base_url`.
///
/// - `environment`: `Simulation`
/// - `base_url`: `Some(base_url)` so dispatch hits the mock server (the single
///   test-injection seam)
/// - `rate_limits`: generous 1000/s on every category so the limiter never
///   throttles a test
/// - `allow_insecure_localhost`: `true` (the mock server is plain `http`)
/// - `max_pages`: `Some(10)`
///
/// Reused by every dependency-class suite; override fields with struct-update
/// syntax (`LsConfig { max_pages: Some(2), ..mock_config(url) }`).
pub fn mock_config(base_url: &str) -> LsConfig {
    LsConfig {
        appkey: TEST_APPKEY.into(),
        appsecretkey: TEST_APPSECRETKEY.into(),
        account_no: TEST_ACCOUNT_NO.into(),
        environment: Environment::Simulation,
        rate_limits: Some(ls_core::RateLimitConfig {
            auth_per_sec: Some(1000),
            market_data_per_sec: Some(1000),
            orders_per_sec: Some(1000),
            account_per_sec: Some(1000),
        }),
        base_url: Some(base_url.to_string()),
        ws_base_url: None,
        max_pages: Some(10),
        connect_timeout_secs: None,
        request_timeout_secs: None,
        ws_connect_timeout_secs: None,
        allow_insecure_localhost: true,
        ws_channel_capacity: None,
        ws_overflow_policy: None,
    }
}

/// Mount the OAuth2 token endpoint (`POST /oauth2/token`) issuing [`TEST_TOKEN`]
/// with a 24h TTL on every call.
///
/// Returns no handle; use [`mount_token_expect`] when a hit-count assertion is
/// needed (e.g. proving the cache was cleared and a re-fetch occurred).
pub async fn mount_token(server: &MockServer) {
    token_mock().mount(server).await;
}

/// Mount the token endpoint with an exact expected hit count, asserted when the
/// server is dropped.
///
/// `expected == 1` proves the token is fetched once and cached; `expected == 2`
/// across two `token` calls proves a `revoke` cleared the cache and forced a
/// re-fetch.
pub async fn mount_token_expect(server: &MockServer, expected: u64) {
    token_mock().expect(expected).mount(server).await;
}

/// Mount the OAuth2 revoke endpoint (`POST /oauth2/revoke`) returning a success
/// envelope (`{ code: "0000", message: "OK" }`).
pub async fn mount_revoke(server: &MockServer) {
    revoke_mock(ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "code": "0000",
        "message": "OK",
    })))
    .mount(server)
    .await;
}

/// Mount the revoke endpoint returning a NON-OK `{ code, message }` envelope on
/// HTTP 200 — the failure shape `revoke_token_http` surfaces as
/// `LsError::ApiError`.
pub async fn mount_revoke_non_ok(server: &MockServer, code: &str, message: &str) {
    revoke_mock(ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "code": code,
        "message": message,
    })))
    .mount(server)
    .await;
}

/// The token-endpoint `Mock` (unmounted) so callers can chain `.expect(n)`.
fn token_mock() -> Mock {
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": TEST_TOKEN,
            "expire_in": DEFAULT_TOKEN_TTL_SECS.to_string(),
            "scope": "oob",
            "token_type": "Bearer",
        })))
}

/// The revoke-endpoint `Mock` (unmounted) with the given response.
fn revoke_mock(response: ResponseTemplate) -> Mock {
    Mock::given(method("POST"))
        .and(path("/oauth2/revoke"))
        .respond_with(response)
}
