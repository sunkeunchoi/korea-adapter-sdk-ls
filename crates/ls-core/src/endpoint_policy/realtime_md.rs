//! Market-data WebSocket channel policies (closure-flip + open-window waves).
//!
//! Wave-3 split out of `endpoint_policy.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use realtime_md::*;` so every `endpoint_policy::FOO_POLICY` path is unchanged.
use super::*;


// === Closure-flip WS batch (plan -004): 31 connection-reachable-only WebSocket
// market-data policies. Registered in policy_index_crosscheck ONLY (KTD1). ===

/// `NS3` — (NXT)체결 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const NS3_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "NS3",
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

/// `NH1` — (NXT)호가잔량 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const NH1_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "NH1",
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

/// `NS2` — (NXT)우선호가 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const NS2_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "NS2",
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

/// `NK1` — (NXT)거래원 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const NK1_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "NK1",
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

/// `NBT` — (NXT)시간대별투자자매매추이 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const NBT_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "NBT",
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

/// `KS_` — KOSDAQ우선호가 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const KS_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "KS_",
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

/// `OK_` — KOSDAQ거래원 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const OK_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "OK_",
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

/// `KH_` — KOSDAQ프로그램매매종목별 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const KH_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "KH_",
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

/// `KM_` — KOSDAQ프로그램매매전체집계 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const KM_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "KM_",
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

/// `PH_` — KOSPI프로그램매매종목별 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const PH_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "PH_",
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

/// `K1_` — KOSPI거래원 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const K1_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "K1_",
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

/// `IJ_` — 지수 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const IJ_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "IJ_",
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

/// `YS3` — KOSPI예상체결 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const YS3_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "YS3",
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

/// `YK3` — KOSDAQ예상체결 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const YK3_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "YK3",
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

/// `VI_` — VI발동해제 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const VI_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "VI_",
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

/// `JC0` — 주식선물체결 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const JC0_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "JC0",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `JH0` — 주식선물호가 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const JH0_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "JH0",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `JD0` — 주식선물실시간상하한가 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const JD0_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "JD0",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `FD0` — KOSPI200선물실시간상하한가 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const FD0_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "FD0",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `OD0` — KOSPI200옵션실시간상하한가 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const OD0_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "OD0",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `OMG` — KOSPI200옵션민감도 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const OMG_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "OMG",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `YF9` — 지수선물예상체결 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const YF9_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "YF9",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `YOC` — 지수옵션예상체결 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const YOC_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "YOC",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `BM_` — 업종별투자자별매매현황 ([업종] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const BM_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "BM_",
    path: "/websocket",
    module: "indtp",
    group: "[업종] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `WOC` — 해외옵션 체결 ([해외선물] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const WOC_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "WOC",
    path: "/websocket",
    module: "overseas-futureoption",
    group: "[해외선물] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `WOH` — 해외옵션 호가 ([해외선물] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const WOH_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "WOH",
    path: "/websocket",
    module: "overseas-futureoption",
    group: "[해외선물] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `JIF` — 장운영정보 ([기타] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const JIF_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "JIF",
    path: "/websocket",
    module: "etc",
    group: "[기타] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `NWS` — 실시간뉴스제목패킷 ([기타] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const NWS_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "NWS",
    path: "/websocket",
    module: "etc",
    group: "[기타] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `BMT` — 시간대별투자자매매추이 ([실시간 시세 투자정보] 투자정보, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const BMT_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "BMT",
    path: "/websocket",
    module: "investinfo",
    group: "[실시간 시세 투자정보] 투자정보",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `CUR` — 현물정보USD실시간 ([실시간 시세 투자정보] 투자정보, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const CUR_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CUR",
    path: "/websocket",
    module: "investinfo",
    group: "[실시간 시세 투자정보] 투자정보",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `MK2` — US지수 ([실시간 시세 투자정보] 투자정보, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1, plan -004). No REST rate
/// limits. Registered in the policy_index crosscheck array ONLY.
pub const MK2_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "MK2",
    path: "/websocket",
    module: "investinfo",
    group: "[실시간 시세 투자정보] 투자정보",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

// === Open-window WS track-flip wave (plan 2026-06-29-001): 39
// connection-reachable-only WebSocket market-data policies. Registered in
// policy_index_crosscheck ONLY (KTD1). Metadata stays implemented:false. ===

/// `AFR` — API사용자조건검색실시간 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const AFR_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "AFR",
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

/// `B7_` — ETF호가잔량 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const B7_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "B7_",
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

/// `C02` — KRX야간파생 선물체결 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const C02_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "C02",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `CD0` — 상품선물실시간상하한가 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const CD0_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CD0",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `DBM` — KRX야간파생 투자자매매현황 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const DBM_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "DBM",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `DBT` — KRX야간파생 투자자별현황 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const DBT_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "DBT",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `DC0` — KRX야간파생 체결 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const DC0_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "DC0",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `DD0` — KRX야간파생 실시간상하한가 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const DD0_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "DD0",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `DH0` — KRX야간파생 호가 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const DH0_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "DH0",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `DH1` — KOSPI시간외단일가호가잔량 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const DH1_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "DH1",
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

/// `DHA` — KOSDAQ시간외단일가호가잔량 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const DHA_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "DHA",
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

/// `DK3` — KOSDAQ시간외단일가체결 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const DK3_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "DK3",
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

/// `DS3` — KOSPI시간외단일가체결 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const DS3_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "DS3",
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

/// `DVI` — 시간외단일가VI발동해제 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const DVI_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "DVI",
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

/// `ESN` — 뉴ELW투자지표민감도 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const ESN_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "ESN",
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

/// `FX9` — KOSPI200선물가격제한폭확대 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const FX9_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "FX9",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `H02` — KRX야간파생 선물정정취소 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const H02_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "H02",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `H2_` — KOSPI장전시간외호가잔량 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const H2_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "H2_",
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

/// `HB_` — KOSDAQ장전시간외호가잔량 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const HB_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "HB_",
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

/// `I5_` — 코스피ETF종목실시간NAV ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const I5_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "I5_",
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

/// `JX0` — 주식선물가격제한폭확대 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const JX0_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "JX0",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `NBM` — (NXT)업종별투자자별매매현황 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const NBM_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "NBM",
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

/// `NPM` — (NXT)프로그램매매전체집계 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const NPM_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "NPM",
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

/// `NVI` — (NXT)VI 발동 해제 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const NVI_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "NVI",
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

/// `O02` — KRX야간파생 선물접수 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const O02_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "O02",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `OX0` — KOSPI200옵션가격제한폭확대 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const OX0_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "OX0",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `SHC` — 상/하한가근접진입 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const SHC_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "SHC",
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

/// `SHD` — 상/하한가근접이탈 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const SHD_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "SHD",
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

/// `SHI` — 상/하한가진입 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const SHI_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "SHI",
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

/// `SHO` — 상/하한가이탈 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const SHO_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "SHO",
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

/// `UBM` — (통합) 업종별투자자별매매현황 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const UBM_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "UBM",
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

/// `UBT` — (통합)시간대별투자자매매추이 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const UBT_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "UBT",
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

/// `UK1` — (통합)거래원 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const UK1_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "UK1",
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

/// `UVI` — (통합)VI발동해제 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const UVI_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "UVI",
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

/// `UYS` — (통합)예상체결 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const UYS_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "UYS",
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

/// `YC3` — 상품선물예상체결 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const YC3_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "YC3",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `YJC` — 주식선물예상체결 ([선물/옵션] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const YJC_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "YJC",
    path: "/websocket",
    module: "futureoption",
    group: "[선물/옵션] 실시간 시세",
    protocol: Protocol::WebSocket,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: None,
    corp_rate_limit_per_sec: None,
};

/// `YJ_` — 예상지수 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const YJ_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "YJ_",
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

/// `h3_` — ELW호가잔량 ([주식] 실시간 시세, WebSocket).
///
/// Connection-reachable-only WS market-data feed (KTD1). No REST rate limits.
/// Registered in the policy_index crosscheck array ONLY.
pub const H3_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "h3_",
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


/// K3_ — KOSDAQ체결 실시간 시세 (real-time KOSDAQ trade feed, WebSocket).
///
/// WebSocket TR: no REST dispatch; the policy const mirrors the metadata index
/// for cross-checking. Ported verbatim from the migration source's `K3_POLICY`.
pub const K3_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "K3_",
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

/// H1_ — KOSPI호가잔량 실시간 시세 (real-time KOSPI order-book feed, WebSocket).
///
/// WebSocket TR: no REST dispatch; the policy const mirrors the metadata index
/// for cross-checking. Ported verbatim from the migration source's `H1_POLICY`.
pub const H1_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "H1_",
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

/// HA_ — KOSDAQ호가잔량 실시간 시세 (real-time KOSDAQ order-book feed, WebSocket).
///
/// WebSocket TR: no REST dispatch; the policy const mirrors the metadata index
/// for cross-checking. Ported verbatim from the migration source's `HA_POLICY`.
pub const HA_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "HA_",
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

/// S2_ — KOSPI우선호가 실시간 시세 (real-time KOSPI best-quote feed, WebSocket).
///
/// WebSocket TR: no REST dispatch; the policy const mirrors the metadata index
/// for cross-checking. Ported verbatim from the migration source's `S2_POLICY`.
pub const S2_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "S2_",
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

/// US3 — (통합)체결 실시간 시세 (real-time integrated trade feed, WebSocket).
///
/// WebSocket TR: no REST dispatch; the policy const mirrors the metadata index
/// for cross-checking. Ported verbatim from the migration source's `US3_POLICY`.
pub const US3_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "US3",
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

/// UH1 — (통합)호가잔량 실시간 시세 (real-time integrated order-book feed, WebSocket).
///
/// WebSocket TR: no REST dispatch; the policy const mirrors the metadata index
/// for cross-checking. Ported verbatim from the migration source's `UH1_POLICY`.
pub const UH1_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "UH1",
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

/// US2 — (통합)우선호가 실시간 시세 (real-time integrated best-quote feed, WebSocket).
///
/// WebSocket TR: no REST dispatch; the policy const mirrors the metadata index
/// for cross-checking. Ported verbatim from the migration source's `US2_POLICY`.
pub const US2_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "US2",
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

/// GSC — 해외주식 체결 실시간 시세 (real-time overseas-stock trade feed, WebSocket).
///
/// WebSocket TR: no REST dispatch; the policy const mirrors the metadata index
/// for cross-checking. Ported verbatim from the migration source's `GSC_POLICY`.
pub const GSC_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "GSC",
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

/// GSH — 해외주식 호가 실시간 시세 (real-time overseas-stock order-book feed, WebSocket).
///
/// WebSocket TR: no REST dispatch; the policy const mirrors the metadata index
/// for cross-checking. Ported verbatim from the migration source's `GSH_POLICY`.
pub const GSH_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "GSH",
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

/// OVC — 해외선물 체결 실시간 시세 (real-time overseas-futures trade feed, WebSocket).
///
/// WebSocket TR: no REST dispatch; the policy const mirrors the metadata index
/// for cross-checking. Ported verbatim from the migration source's `OVC_POLICY`.
pub const OVC_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "OVC",
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

/// OVH — 해외선물 호가 실시간 시세 (real-time overseas-futures order-book feed, WebSocket).
///
/// WebSocket TR: no REST dispatch; the policy const mirrors the metadata index
/// for cross-checking. Ported verbatim from the migration source's `OVH_POLICY`.
pub const OVH_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "OVH",
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

/// OC0 — KOSPI200옵션체결 실시간 시세 (real-time option trade feed, WebSocket).
///
/// WebSocket TR: no REST dispatch; the policy const mirrors the metadata index
/// for cross-checking. Ported verbatim from the migration source's `OC0_POLICY`.
pub const OC0_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "OC0",
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

/// OH0 — KOSPI200옵션호가 실시간 시세 (real-time option order-book feed, WebSocket).
///
/// WebSocket TR: no REST dispatch; the policy const mirrors the metadata index
/// for cross-checking. Ported verbatim from the migration source's `OH0_POLICY`.
pub const OH0_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "OH0",
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

/// FC9 — KOSPI200선물체결 실시간 시세 (real-time futures trade feed, WebSocket).
///
/// WebSocket TR: no REST dispatch; the policy const mirrors the metadata index
/// for cross-checking. Ported verbatim from the migration source's `FC9_POLICY`.
pub const FC9_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "FC9",
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

/// FH9 — KOSPI200선물호가 실시간 시세 (real-time futures order-book feed, WebSocket).
///
/// WebSocket TR: no REST dispatch; the policy const mirrors the metadata index
/// for cross-checking. Ported verbatim from the migration source's `FH9_POLICY`.
pub const FH9_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "FH9",
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
