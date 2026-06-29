//! Order-event (주문/체결) WebSocket lane policies (P2, observation-only).
//!
//! Wave-3 split out of `endpoint_policy.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use realtime_order::*;` so every `endpoint_policy::FOO_POLICY` path is unchanged.
use super::*;


// =============================================================================
// P2 order-event realtime lane (WebSocket; observation-only 주문/체결 feeds).
//
// Ported verbatim from the migration source's `<tr>_POLICY`. NOTE: in the source
// every one of these 16 order-event channels carries `category: MarketData` and
// `is_order: false` (they sit under the module's "실시간 시세" group), so they need
// no MarketData re-pin to match the U4 `market_data` rate_bucket in metadata —
// the crosscheck passes as-is. WebSocket policies carry no rate limits.
// =============================================================================

/// SC0 — 주식주문접수 실시간 (real-time stock order-accept feed, WebSocket).
pub const SC0_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "SC0",
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

/// SC1 — 주식주문체결 실시간 (real-time stock order-fill feed, WebSocket).
pub const SC1_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "SC1",
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

/// SC2 — 주식주문정정 실시간 (real-time stock order-amend feed, WebSocket).
pub const SC2_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "SC2",
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

/// SC3 — 주식주문취소 실시간 (real-time stock order-cancel feed, WebSocket).
pub const SC3_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "SC3",
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

/// SC4 — 주식주문거부 실시간 (real-time stock order-reject feed, WebSocket).
pub const SC4_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "SC4",
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

/// C01 — 선물주문체결 실시간 (real-time F-O order-fill feed, WebSocket).
pub const C01_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "C01",
    path: "/websocket",
    module: "futures_options",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// O01 — 선물접수 실시간 (real-time F-O order-accept feed, WebSocket).
pub const O01_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "O01",
    path: "/websocket",
    module: "futures_options",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// H01 — 선물주문정정취소 실시간 (real-time F-O order-amend-cancel feed, WebSocket).
pub const H01_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "H01",
    path: "/websocket",
    module: "futures_options",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// AS0 — 해외주식주문접수(미국) 실시간 (real-time overseas-stock order-accept, WebSocket).
pub const AS0_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "AS0",
    path: "/websocket",
    module: "overseas_stock",
    group: "[해외주식] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// AS1 — 해외주식주문체결(미국) 실시간 (real-time overseas-stock order-fill, WebSocket).
pub const AS1_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "AS1",
    path: "/websocket",
    module: "overseas_stock",
    group: "[해외주식] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// AS2 — 해외주식주문정정(미국) 실시간 (real-time overseas-stock order-amend, WebSocket).
pub const AS2_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "AS2",
    path: "/websocket",
    module: "overseas_stock",
    group: "[해외주식] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// AS3 — 해외주식주문취소(미국) 실시간 (real-time overseas-stock order-cancel, WebSocket).
pub const AS3_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "AS3",
    path: "/websocket",
    module: "overseas_stock",
    group: "[해외주식] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// AS4 — 해외주식주문거부(미국) 실시간 (real-time overseas-stock order-reject, WebSocket).
pub const AS4_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "AS4",
    path: "/websocket",
    module: "overseas_stock",
    group: "[해외주식] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// TC1 — 해외선물 주문접수 실시간 (real-time overseas-futures order-accept, WebSocket).
pub const TC1_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "TC1",
    path: "/websocket",
    module: "overseas_futures",
    group: "[해외선물] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// TC2 — 해외선물 주문응답 실시간 (real-time overseas-futures order-response, WebSocket).
pub const TC2_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "TC2",
    path: "/websocket",
    module: "overseas_futures",
    group: "[해외선물] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// TC3 — 해외선물 주문체결 실시간 (real-time overseas-futures order-fill, WebSocket).
pub const TC3_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "TC3",
    path: "/websocket",
    module: "overseas_futures",
    group: "[해외선물] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};
