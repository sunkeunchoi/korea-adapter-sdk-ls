//! `ls-sdk-test-support` — dev-only test mocks for `ls-sdk`.
//!
//! `publish = false`: this is not one of the shippable target crates. Provides
//! mock config, wiremock HTTP endpoints (token/revoke + TR responses), and a
//! mock WebSocket server, reused across the dependency-class test suites.

pub mod mock_http;
pub mod mock_ws;

pub use mock_http::{
    mock_config, mount_revoke, mount_revoke_non_ok, mount_token, mount_token_expect,
    DEFAULT_TOKEN_TTL_SECS, TEST_ACCOUNT_NO, TEST_APPKEY, TEST_APPSECRETKEY, TEST_TOKEN,
};
pub use mock_ws::{MockWsServer, MOCK_REJECTION_RSP_CD};
