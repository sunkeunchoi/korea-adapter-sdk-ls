//! OAuth token issuance & revocation endpoint policies.
//!
//! Wave-3 split out of `endpoint_policy.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use auth::*;` so every `endpoint_policy::FOO_POLICY` path is unchanged.
use super::*;


// ---------------------------------------------------------------------------
// Slice TR policy constants — runtime mirror of `tr-index.yaml`.
// ---------------------------------------------------------------------------

/// 접근토큰 발급 (OAuth2 token issue).
pub const TOKEN_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "token",
    path: "/oauth2/token",
    module: "auth",
    group: "접근토큰 발급",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Auth,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// 접근토큰 폐기 (OAuth2 token revoke).
pub const REVOKE_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "revoke",
    path: "/oauth2/revoke",
    module: "auth",
    group: "접근토큰 폐기",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Auth,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};
