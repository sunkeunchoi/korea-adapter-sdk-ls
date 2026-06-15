//! `ls-sdk-test-support` — dev-only test mocks for `ls-sdk`.
//!
//! `publish = false`: this is not one of the shippable target crates. Provides
//! mock config, wiremock HTTP endpoints (token/revoke + TR responses), and a
//! mock WebSocket server, reused across the dependency-class test suites.
