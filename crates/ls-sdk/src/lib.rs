//! `ls-sdk` — the maintained public SDK surface for the LS Securities Open API.
//!
//! Dependency classes are modules within this crate (standalone, market_session,
//! paginated, account, realtime), not separate crates. The Change-Scoped Gate
//! routes tests by metadata facet, not by module boundary.
//!
//! The public entry is [`LsSdk`], a thin wrapper over `ls_core`'s shared runtime
//! ([`ls_core::Inner`]). Each dependency class is reached through an accessor
//! that hands back a small, `Arc<Inner>`-backed handle (e.g. [`LsSdk::standalone`]
//! → [`standalone::Standalone`]). The accessors share one token cache and one
//! rate limiter, so acquiring a token in one class makes it available to all.

use std::sync::Arc;

use ls_core::{Inner, LsClient, LsConfig, LsResult};

pub mod market_session;
pub mod paginated;
pub mod standalone;

/// Public SDK client — the maintained entry point.
///
/// Holds `Arc<Inner>` (the same shared runtime `ls_core::LsClient` exposes), so
/// it is cheap to clone and every dependency-class handle it vends shares the
/// token cache and rate limiter.
#[derive(Clone)]
pub struct LsSdk {
    inner: Arc<Inner>,
}

impl LsSdk {
    /// Validate `config` and build the SDK client. Synchronous — no network I/O.
    ///
    /// Reuses `ls_core::LsClient::new` for credential + URL-scheme validation,
    /// then keeps the validated `Arc<Inner>`. The OAuth2 token is fetched lazily
    /// on first use.
    pub fn new(config: LsConfig) -> LsResult<Self> {
        let client = LsClient::new(config)?;
        Ok(LsSdk {
            inner: client.inner,
        })
    }

    /// Build directly from an already-constructed shared core.
    ///
    /// Useful in tests that build `Inner` from a mock config, and when sharing
    /// one runtime across several facades.
    pub fn from_inner(inner: Arc<Inner>) -> Self {
        LsSdk { inner }
    }

    /// Shared runtime core, for callers that need the transport primitives
    /// directly.
    pub fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    /// The standalone (OAuth-only) dependency class: `token` and `revoke`.
    pub fn standalone(&self) -> standalone::Standalone {
        standalone::Standalone::new(Arc::clone(&self.inner))
    }

    /// The market-session dependency class: the `t1102` current-price quote.
    pub fn market_session(&self) -> market_session::MarketSession {
        market_session::MarketSession::new(Arc::clone(&self.inner))
    }

    /// The paginated dependency class: the SELF-paginated `t8412` chart.
    pub fn paginated(&self) -> paginated::Paginated {
        paginated::Paginated::new(Arc::clone(&self.inner))
    }
}
