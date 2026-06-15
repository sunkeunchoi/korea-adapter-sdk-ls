//! Standalone dependency class — OAuth-only `token` and `revoke`.
//!
//! This is the *standalone* class: operations that need credentials but no
//! account state, no market session, and — structurally — no pagination. It is
//! a thin wrapper over `ls-core`'s token lifecycle:
//!
//! - [`Standalone::token`] returns a valid bearer token, fetching and caching it
//!   via the shared [`ls_core::TokenManager`] (double-checked locking lives in
//!   core; this wrapper adds nothing to the acquisition path).
//! - [`Standalone::revoke`] revokes the cached token via
//!   [`ls_core::revoke_token_http`] and clears the cache **on success only**.
//!   Cache-clearing is the wrapper's job: `revoke_token_http` is a pure HTTP
//!   call and never touches the cache, so a failed revoke leaves a still-valid
//!   token in place for the caller to keep using.
//!
//! ## No pagination — by construction
//!
//! The standalone class is OAuth-only and has **no `HasPagination` impl** for
//! any of its types. There is no request struct here at all: `token`/`revoke`
//! take no body and dispatch through the OAuth form-POST paths, not the
//! `post_paginated`/`collect_all` machinery. Pagination is therefore structurally
//! unreachable. `tests/standalone_tests.rs` pins this with a trait-bound check
//! over [`Standalone`].

use std::sync::Arc;

use ls_core::{Inner, LsError, LsResult, RateLimitCategory};

/// OAuth-only standalone operations, backed by the shared runtime core.
///
/// Cheap to clone — shares `Arc<Inner>` (and therefore the token cache) with the
/// rest of the SDK.
#[derive(Clone)]
pub struct Standalone {
    inner: Arc<Inner>,
}

impl Standalone {
    /// Wrap a shared runtime core.
    pub fn new(inner: Arc<Inner>) -> Self {
        Standalone { inner }
    }

    /// Return a valid bearer token, fetching and caching it if absent or expiring.
    ///
    /// Delegates to the shared [`ls_core::TokenManager`], so concurrent callers
    /// fetch at most once (double-checked locking) and a cached, unexpired token
    /// is returned without a network call.
    pub async fn token(&self) -> LsResult<String> {
        self.inner
            .token_manager
            .get_or_refresh(&self.inner.client, &self.inner.config, &self.inner.rate_limiter)
            .await
    }

    /// Revoke the cached bearer token and clear the cache on success.
    ///
    /// Idempotent: with no cached token there is nothing to revoke, so this
    /// returns `Ok(())` without a network call.
    ///
    /// On a non-OK OAuth `{ code, message }` envelope the revoke fails and the
    /// error surfaces as [`LsError::Auth`] (the standalone class's auth-failure
    /// vocabulary), and the **still-valid** cache is left untouched — a failed
    /// revoke must not invalidate a token the caller can still use. The cache is
    /// cleared only after `revoke_token_http` reports success.
    pub async fn revoke(&self) -> LsResult<()> {
        // Nothing cached → nothing to revoke. Idempotent no-op.
        let Some(token) = self.inner.token_manager.snapshot_token().await else {
            return Ok(());
        };

        // Charge the Auth bucket for the revoke call, mirroring the token path.
        self.inner.rate_limiter.wait(RateLimitCategory::Auth).await;

        match ls_core::revoke_token_http(&self.inner.client, &self.inner.config, &token).await {
            Ok(()) => {
                // Success is the ONLY path that clears the cache — clearing is
                // the wrapper's responsibility, not the HTTP call's.
                self.inner.token_manager.clear().await;
                Ok(())
            }
            // A non-OK `{code,message}` envelope surfaces from core as
            // `ApiError`; the standalone surface re-frames it as `Auth` while
            // preserving the code/message. The cache is deliberately NOT cleared.
            Err(LsError::ApiError { code, message }) => {
                Err(LsError::Auth(format!("token revoke rejected ({code}): {message}")))
            }
            // Transport and other failures propagate unchanged; the cache is
            // likewise left intact (the token may still be valid).
            Err(other) => Err(other),
        }
    }
}
