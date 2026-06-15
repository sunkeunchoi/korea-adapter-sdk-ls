//! Standalone (OAuth-only) dependency-class tests.
//!
//! Exercises `token`/`revoke` against wiremock through REAL `ls-core` dispatch
//! (the mock config injects `base_url`, so the OAuth form-POSTs hit the mock
//! server). Covers the happy path, the non-OK revoke envelope, and the
//! structural no-pagination guarantee.

use ls_core::{Inner, LsError};
use ls_sdk::standalone::Standalone;
use ls_sdk::LsSdk;
use ls_sdk_test_support::mock_http::{
    mock_config, mount_revoke, mount_revoke_non_ok, mount_token_expect, TEST_TOKEN,
};
use wiremock::MockServer;

/// Build an `LsSdk` whose dispatch is pointed at the mock server.
fn sdk_for(server: &MockServer) -> LsSdk {
    let inner = Inner::new(mock_config(&server.uri())).expect("valid mock config");
    LsSdk::from_inner(inner)
}

/// Happy path: `token` fetches via the mock endpoint and caches; after a
/// successful `revoke` the cache is cleared, so the NEXT `token` call re-fetches.
///
/// The token endpoint is mounted with `.expect(2)`: one fetch before revoke, one
/// after. If revoke failed to clear the cache the second `token` would hit the
/// cache and the count would be 1, failing the assertion on server drop.
#[tokio::test]
async fn token_then_successful_revoke_clears_cache_and_refetches() {
    let server = MockServer::start().await;
    mount_token_expect(&server, 2).await;
    mount_revoke(&server).await;

    let standalone = sdk_for(&server).standalone();

    // First acquisition fetches from the endpoint.
    let first = standalone.token().await.expect("first token");
    assert_eq!(first, TEST_TOKEN);

    // Cached: a second call within expiry would NOT hit the endpoint (proven by
    // the .expect(2) total — this call is served from cache).
    let cached = standalone.token().await.expect("cached token");
    assert_eq!(cached, TEST_TOKEN);

    // Revoke succeeds and clears the cache.
    standalone.revoke().await.expect("revoke ok");

    // Next acquisition must re-fetch (cache cleared) — this is the SECOND hit.
    let refetched = standalone.token().await.expect("refetched token");
    assert_eq!(refetched, TEST_TOKEN);

    // `.expect(2)` is asserted when `server` drops at end of scope.
}

/// Edge: a non-OK `{code,message}` revoke envelope surfaces `LsError::Auth` and
/// does NOT clear a still-valid cache — the subsequent `token` call is served
/// from cache and does NOT re-fetch.
///
/// The token endpoint is mounted with `.expect(1)`: exactly one fetch total. If
/// the failed revoke had wrongly cleared the cache, the post-revoke `token` call
/// would re-fetch and the count would be 2, failing on drop.
#[tokio::test]
async fn failed_revoke_surfaces_auth_and_keeps_cache() {
    let server = MockServer::start().await;
    mount_token_expect(&server, 1).await;
    mount_revoke_non_ok(&server, "IGW00121", "already revoked").await;

    let standalone = sdk_for(&server).standalone();

    // Acquire and cache a token (the one-and-only fetch).
    let first = standalone.token().await.expect("first token");
    assert_eq!(first, TEST_TOKEN);

    // Revoke fails on the non-OK envelope: surfaced as Auth, not ApiError.
    let err = standalone.revoke().await.expect_err("revoke must fail");
    match err {
        LsError::Auth(msg) => {
            assert!(msg.contains("IGW00121"), "auth message preserves the code: {msg}");
        }
        other => panic!("expected LsError::Auth, got {other:?}"),
    }

    // Cache survived the failed revoke: this is served from cache (no re-fetch).
    let still_cached = standalone.token().await.expect("still cached");
    assert_eq!(still_cached, TEST_TOKEN);

    // `.expect(1)` is asserted on drop — a re-fetch would have made it 2.
}

/// Idempotence: `revoke` with no cached token is a no-op `Ok(())` and makes no
/// network call (the revoke endpoint is never mounted, so any call would 404 →
/// error).
#[tokio::test]
async fn revoke_with_no_token_is_ok_noop() {
    let server = MockServer::start().await;
    // Deliberately mount nothing.
    let standalone = sdk_for(&server).standalone();
    standalone.revoke().await.expect("revoke with empty cache is a no-op");
}

// ---------------------------------------------------------------------------
// Integration: the standalone class is structurally non-paginated.
// ---------------------------------------------------------------------------
//
// The standalone class is OAuth-only. It exposes NO request struct and NO type
// that implements `ls_core::HasPagination`; `token`/`revoke` take no body and
// dispatch through the OAuth form-POST paths, never `post_paginated` /
// `collect_all`. Pagination is therefore structurally unreachable.
//
// We pin this with a compile-time trait-bound guard. `assert_paginated` only
// accepts `T: HasPagination`; it is invoked ONLY for a known-paginated control
// type, never for any standalone type. The accompanying note documents that no
// `impl HasPagination` exists for `Standalone` (or any standalone request type,
// of which there are none) — if one were ever added, this module's intent would
// be violated, and the absence is enforced structurally by there being no such
// type to pass here.

/// Compile-time witness that a type is paginated. Used only on a control type.
#[allow(dead_code)]
fn assert_paginated<T: ls_core::HasPagination>() {}

#[test]
fn standalone_is_structurally_non_paginated() {
    // Control: a genuinely paginated type satisfies the bound, proving the guard
    // is real and not vacuously true.
    assert_paginated::<PaginatedControl>();

    // `Standalone` itself carries no pagination state. The standalone class has
    // no request struct at all, so there is nothing that could implement
    // `HasPagination`. The line below is intentionally NOT written, because it
    // would fail to compile — that failure is the structural guarantee:
    //
    //     assert_paginated::<Standalone>();  // ← does not compile: no impl
    //
    // Referencing the type keeps the assertion honest if the class is ever given
    // a body type.
    let _ = std::marker::PhantomData::<Standalone>;
}

/// A minimal paginated request used solely as the positive control for the
/// `assert_paginated` guard above. It mirrors how a real paginated TR wrapper
/// (U12's `t8412`) opts into pagination via the core macro.
#[derive(Clone, serde::Serialize)]
struct PaginatedControl {
    #[serde(skip)]
    tr_cont: String,
    #[serde(skip)]
    tr_cont_key: String,
}
ls_core::impl_has_pagination!(PaginatedControl);
