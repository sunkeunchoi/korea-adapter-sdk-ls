//! Structured endpoint policy — the runtime descriptor for a single TR.
//!
//! Every REST TR has a static `EndpointPolicy` constant. Dispatch methods pass
//! `&EndpointPolicy` to `Inner::post` instead of loose `path` / `category` /
//! `tr_code` strings, so routing is a structured value rather than string
//! coupling.
//!
//! The `{TR}_POLICY` consts below are the runtime mirror of the `tr-index.yaml`
//! selector fields. They are hand-authored (not generated): the `ls-metadata`
//! validator cross-checks each const against the index so code and metadata
//! cannot drift.

use crate::{LsError, LsResult, RateLimitCategory};

/// Protocol for an endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// REST HTTP endpoint.
    Rest,
    /// WebSocket real-time feed.
    WebSocket,
}

/// Static metadata for a single TR endpoint.
///
/// All string fields are `&'static str` — zero allocation, zero clone cost —
/// and the whole struct is `Copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointPolicy {
    /// TR code, e.g. `"t1102"` or `"CSPAQ12200"`.
    pub tr_code: &'static str,
    /// REST path, e.g. `"/stock/market-data"`. WebSocket TRs use `"/websocket"`.
    pub path: &'static str,
    /// Rust module name, e.g. `"stock"`.
    pub module: &'static str,
    /// Group name from the spec, e.g. `"[주식] 시세"`.
    pub group: &'static str,
    /// `Rest` or `WebSocket`.
    pub protocol: Protocol,
    /// Rate-limit bucket charged for this endpoint.
    pub category: RateLimitCategory,
    /// `true` for order submission / cancel / modify endpoints.
    pub is_order: bool,
    /// `true` if the request supports `tr_cont`/`tr_cont_key` pagination.
    pub has_pagination: bool,
    /// Per-TR upstream rate limit (requests/sec), if declared in the LS spec.
    pub rate_limit_per_sec: Option<u32>,
    /// Corporate per-TR upstream rate limit (requests/sec), if declared in the LS spec.
    pub corp_rate_limit_per_sec: Option<u32>,
}

impl EndpointPolicy {
    /// Runtime guard: non-order dispatch methods must not be used for order
    /// endpoints — those must flow through the order dispatch path so
    /// deduplication and no-retry semantics are enforced.
    pub fn guard_non_order(&self) -> LsResult<()> {
        if self.is_order {
            return Err(LsError::ApiError {
                code: "order-dispatch".into(),
                message: format!(
                    "order endpoint '{}' must use the order dispatch path, not post/post_paginated",
                    self.tr_code
                ),
            });
        }
        Ok(())
    }

    /// `true` if this endpoint uses REST HTTP.
    pub const fn is_rest(&self) -> bool {
        matches!(self.protocol, Protocol::Rest)
    }

    /// `true` if this endpoint uses WebSocket.
    pub const fn is_websocket(&self) -> bool {
        matches!(self.protocol, Protocol::WebSocket)
    }
}

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

/// t1102 — 주식 현재가(시세) 조회 (market-data quote).
pub const T1102_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1102",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    // t1102 is structurally non-paginated (no `HasPagination` impl; dispatches via
    // plain `post`). The flag is the runtime mirror of `facets.self_paginated:
    // false`, so it must be false too — the prior `true` was stale.
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(5),
};

/// t1101 — 주식 현재가호가 조회 (market-data current-price + order book).
pub const T1101_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1101",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(5),
};

/// t8412 — 주식 차트(N분봉) 조회 (SELF-paginated chart).
pub const T8412_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8412",
    path: "/stock/chart",
    module: "stock",
    group: "[주식] 차트",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t8425 — 전체테마 조회 (all-themes list; non-paginated market read).
pub const T8425_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8425",
    path: "/stock/sector",
    module: "stock",
    group: "[주식] 섹터",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1531 — 테마별종목 (stocks in a theme; non-paginated market read).
pub const T1531_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1531",
    path: "/stock/sector",
    module: "stock",
    group: "[주식] 섹터",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1537 — 테마종목별시세조회 (per-stock quotes for a theme; non-paginated).
pub const T1537_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1537",
    path: "/stock/sector",
    module: "stock",
    group: "[주식] 섹터",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1452 — 거래량상위 (top trading volume; single-page body-`idx` paginated).
pub const T1452_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1452",
    path: "/stock/high-item",
    module: "stock",
    group: "[주식] 상위종목",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t8436 — 주식종목조회 (stock master list; non-paginated market read).
pub const T8436_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8436",
    path: "/stock/etc",
    module: "stock",
    group: "[주식] 기타",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(5),
};

/// CSPAQ12200 — 계좌 잔고/예수금 조회 (account balance inquiry).
pub const CSPAQ12200_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CSPAQ12200",
    path: "/stock/accno",
    module: "stock",
    group: "[주식] 계좌",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Account,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// S3_ — KOSPI체결 실시간 시세 (real-time KOSPI trade feed, WebSocket).
///
/// WebSocket TR: there is no REST dispatch, but the policy const is retained as
/// the runtime mirror of the metadata index for cross-checking.
pub const S3_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "S3_",
    path: "/websocket",
    module: "stock",
    group: "[주식] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn test_policy(is_order: bool) -> EndpointPolicy {
        EndpointPolicy {
            tr_code: "TEST",
            path: "/test/path",
            module: "test",
            group: "test",
            protocol: Protocol::Rest,
            category: RateLimitCategory::MarketData,
            is_order,
            has_pagination: false,
            rate_limit_per_sec: None,
            corp_rate_limit_per_sec: None,
        }
    }

    #[test]
    fn guard_non_order_passes_for_non_order() {
        let p = test_policy(false);
        assert!(p.guard_non_order().is_ok());
    }

    #[test]
    fn guard_non_order_fails_for_order() {
        let p = test_policy(true);
        let err = p.guard_non_order().unwrap_err().to_string();
        assert!(err.contains("order endpoint"));
        assert!(err.contains("post/post_paginated"));
    }

    #[test]
    fn slice_rest_policies_are_non_order_rest() {
        for p in [
            TOKEN_POLICY,
            REVOKE_POLICY,
            T1101_POLICY,
            T1102_POLICY,
            T8412_POLICY,
            T8425_POLICY,
            T8436_POLICY,
            T1531_POLICY,
            T1537_POLICY,
            T1452_POLICY,
            CSPAQ12200_POLICY,
        ] {
            assert!(!p.is_order, "{} must not be an order endpoint", p.tr_code);
            assert!(p.is_rest(), "{} must be a REST endpoint", p.tr_code);
            assert!(p.guard_non_order().is_ok());
        }
    }

    #[test]
    fn s3_policy_is_websocket() {
        assert!(S3_POLICY.is_websocket());
        assert!(!S3_POLICY.is_rest());
        assert_eq!(S3_POLICY.path, "/websocket");
    }
}
