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

    /// Runtime guard: the order dispatch path (`Inner::post_order`) must not be
    /// used for non-order endpoints — the inverse of [`guard_non_order`]. This is
    /// defense-in-depth against routing a market-data or account inquiry through
    /// the no-retry/dedup order path (order-safety contract §1).
    ///
    /// [`guard_non_order`]: EndpointPolicy::guard_non_order
    pub fn guard_order(&self) -> LsResult<()> {
        if !self.is_order {
            return Err(LsError::ApiError {
                code: "non-order-dispatch".into(),
                message: format!(
                    "non-order endpoint '{}' must not use the order dispatch path",
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

/// t1901 — ETF현재가(시세)조회 (ETF current-price snapshot; non-paginated).
pub const T1901_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1901",
    path: "/stock/etf",
    module: "stock",
    group: "[주식] ETF",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1906 — ETFLP호가 (ETF LP order-book snapshot; non-paginated).
pub const T1906_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1906",
    path: "/stock/etf",
    module: "stock",
    group: "[주식] ETF",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(5),
};

/// t8450 — (통합)주식현재가호가조회2 (integrated current-price + order-book level-2
/// snapshot; non-paginated).
pub const T8450_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8450",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(3),
};

/// t1638 — 종목별잔량/사전공시 (per-stock remaining-quantity / pre-disclosure ranking;
/// non-paginated). path /stock/etc, group [주식] 기타.
pub const T1638_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1638",
    path: "/stock/etc",
    module: "stock",
    group: "[주식] 기타",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// t1308 — 주식시간대별체결조회챠트 (time-bucketed trade chart; non-paginated).
/// path /stock/market-data, group [주식] 시세.
pub const T1308_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1308",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1449 — 가격대별매매비중조회 (price-band trade-weight; non-paginated).
/// path /stock/market-data, group [주식] 시세.
pub const T1449_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1449",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1621 — 업종별분별투자자매매동향(챠트용) (by-time investor trading; non-paginated).
/// path /stock/investor, group [주식] 투자자.
pub const T1621_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1621",
    path: "/stock/investor",
    module: "stock",
    group: "[주식] 투자자",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t2545 — 상품선물투자자매매동향(챠트용) (F/O by-time investor trading; non-paginated).
/// path /futureoption/investor, group [선물/옵션] 투자자.
pub const T2545_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t2545",
    path: "/futureoption/investor",
    module: "futureoption",
    group: "[선물/옵션] 투자자",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t8406 — 주식선물틱분별체결조회(API용) (F/O by-tick conclusion board; non-paginated).
/// path /futureoption/market-data, group [선물/옵션] 시세.
pub const T8406_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8406",
    path: "/futureoption/market-data",
    module: "futureoption",
    group: "[선물/옵션] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t8407 — API용주식멀티현재가조회 (multi-symbol current price; non-paginated).
/// path /stock/market-data, group [주식] 시세.
pub const T8407_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8407",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(5),
    corp_rate_limit_per_sec: Some(3),
};

/// t1959 — LP대상종목정보조회 (LP-target ELW issue list; non-paginated).
/// path /stock/elw, group [주식] ELW.
pub const T1959_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1959",
    path: "/stock/elw",
    module: "stock",
    group: "[주식] ELW",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(5),
};

/// t1950 — ELW현재가(시세)조회 (ELW current-price/quote; non-paginated).
/// path /stock/elw, group [주식] ELW.
pub const T1950_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1950",
    path: "/stock/elw",
    module: "stock",
    group: "[주식] ELW",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(5),
};

/// t1971 — ELW현재가호가조회 (ELW current-price + 10-level quote board;
/// non-paginated). path /stock/elw, group [주식] ELW.
pub const T1971_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1971",
    path: "/stock/elw",
    module: "stock",
    group: "[주식] ELW",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(5),
};

/// t1972 — ELW현재가(거래원)조회 (ELW current-price + trading-member (거래원) board;
/// non-paginated). path /stock/elw, group [주식] ELW.
pub const T1972_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1972",
    path: "/stock/elw",
    module: "stock",
    group: "[주식] ELW",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t1974 — ELW기초자산동일종목 (ELWs sharing a base asset; non-paginated —
/// metadata self_paginated:false). path /stock/elw, group [주식] ELW.
pub const T1974_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1974",
    path: "/stock/elw",
    module: "stock",
    group: "[주식] ELW",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t1956 — ELW현재가(확정지급액)조회 (ELW current-price / contracted-payout snapshot;
/// non-paginated — metadata self_paginated:false). path /stock/elw, group [주식] ELW.
pub const T1956_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1956",
    path: "/stock/elw",
    module: "stock",
    group: "[주식] ELW",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(5),
};

/// t1969 — ELW지표검색 (ELW screener / indicator search; non-paginated).
/// path /stock/elw, group [주식] ELW.
pub const T1969_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1969",
    path: "/stock/elw",
    module: "stock",
    group: "[주식] ELW",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t1305 — 기간별주가 (period/historical OHLC; self-paginated on `date`).
pub const T1305_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1305",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1105 — 주식피봇/디마크조회 (pivot/demark levels; non-paginated).
pub const T1105_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1105",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(3),
    corp_rate_limit_per_sec: Some(5),
};

/// t1104 — 주식현재가시세메모 (current-price memo; non-paginated).
pub const T1104_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1104",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(3),
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

/// `t1859` — 서버저장조건 조건검색 (server-saved condition search; the spine
/// consumer). Non-paginated `market_session` read keyed by a `t1866`-produced
/// `query_index`.
pub const T1859_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1859",
    path: "/stock/item-search",
    module: "stock",
    group: "[주식] 종목검색",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// `t1825` — 종목Q클릭검색 (ThinQ Q-click search; the Wave 3 spine consumer).
/// Non-paginated `market_session` read keyed by a `t1826`-produced `search_cd`.
pub const T1825_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1825",
    path: "/stock/item-search",
    module: "stock",
    group: "[주식] 종목검색",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// `t1826` — 종목Q클릭검색리스트조회 (ThinQ Q-click search-list; the Wave 3 spine
/// producer). Non-paginated `market_session` read whose `search_cd` output keys
/// the `t1825` consumer.
pub const T1826_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1826",
    path: "/stock/item-search",
    module: "stock",
    group: "[주식] 종목검색",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// `t9905` — 기초자산리스트조회 (full underlying-asset list; Wave 1). Non-paginated
/// `market_session` ELW read; its `shcode` output keys `t1964`'s `item`.
pub const T9905_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t9905",
    path: "/stock/elw",
    module: "stock",
    group: "[주식] ELW",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(5),
};

/// `t9907` — 만기월조회 (ELW expiry-month list; Wave 1). Non-paginated
/// `market_session` ELW read.
pub const T9907_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t9907",
    path: "/stock/elw",
    module: "stock",
    group: "[주식] ELW",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(5),
};

/// `t8431` — ELW종목조회 (ELW symbol list; the Wave 1 spine producer). Non-paginated
/// `market_session` ELW read; its `shcode` output keys `t1958`'s comparison pair.
pub const T8431_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8431",
    path: "/stock/elw",
    module: "stock",
    group: "[주식] ELW",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t8430 — 주식종목조회 (full KOSPI/KOSDAQ stock-issue list; non-paginated). The
/// issue-universe read; `gubun` selects market. owner_class market_session.
pub const T8430_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8430",
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

/// `t9942` — ELW마스터조회API용 (ELW master list; Wave 1). Non-paginated
/// `market_session` ELW read.
pub const T9942_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t9942",
    path: "/stock/elw",
    module: "stock",
    group: "[주식] ELW",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// `t1958` — ELW종목비교 (ELW symbol comparison; Wave 1 comparison member).
/// Non-paginated `market_session` ELW read keyed by two `t8431`-sourced `shcode`s.
pub const T1958_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1958",
    path: "/stock/elw",
    module: "stock",
    group: "[주식] ELW",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(5),
};

/// `t1964` — ELW전광판 (ELW board; Wave 1 board member). Non-paginated ELW read
/// keyed by a `t9905`-sourced `item` underlying code. Ships **PENDING** (callable
/// but shape-unconfirmed: broad filter defaults return an empty board), so its
/// `owner_class` stays the `standalone` placeholder until a confirming flip.
pub const T1964_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1964",
    path: "/stock/elw",
    module: "stock",
    group: "[주식] ELW",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(5),
};

/// `t1601` — 투자자별종합 (investor-by-type aggregate; Wave 2). Non-paginated
/// `market_session` investor read.
pub const T1601_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1601",
    path: "/stock/investor",
    module: "stock",
    group: "[주식] 투자자",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(5),
};

/// `t1615` — 투자자매매종합1 (investor trading aggregate; Wave 2). Non-paginated.
pub const T1615_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1615",
    path: "/stock/investor",
    module: "stock",
    group: "[주식] 투자자",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// `t1640` — 프로그램매매종합조회(미니) (program-trading aggregate; Wave 2).
/// Non-paginated.
pub const T1640_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1640",
    path: "/stock/program",
    module: "stock",
    group: "[주식] 프로그램",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// `t1662` — 시간대별프로그램매매추이(차트) (by-time program-trading chart; Wave 2).
/// Non-paginated.
pub const T1662_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1662",
    path: "/stock/program",
    module: "stock",
    group: "[주식] 프로그램",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// `t1664` — 투자자매매종합(챠트) (investor trading chart; Wave 2). Non-paginated.
pub const T1664_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1664",
    path: "/stock/investor",
    module: "stock",
    group: "[주식] 투자자",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// `t3341` — 재무순위종합 (financial ranking; Wave 2; single-page body-`idx`
/// paginated). `has_pagination: true` mirrors `facets.self_paginated`.
pub const T3341_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t3341",
    path: "/stock/investinfo",
    module: "stock",
    group: "[주식] 투자정보",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t8424 — 전체업종 (all-sectors list; non-paginated sector/index read). The
/// anchor and `upcode` source for the [업종] 시세 cluster (Wave A).
pub const T8424_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8424",
    path: "/indtp/market-data",
    module: "indtp",
    group: "[업종] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1511 — 업종현재가 (one sector's index snapshot; non-paginated). The 10/5
/// rate limit is higher than its sibling sector TRs (1/3) — this is the limit the
/// upstream spec publishes for the snapshot endpoint, mirrored from the baseline.
pub const T1511_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1511",
    path: "/indtp/market-data",
    module: "indtp",
    group: "[업종] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(5),
};

/// t1485 — 예상지수 (one sector's expected/auction index; non-paginated).
pub const T1485_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1485",
    path: "/indtp/market-data",
    module: "indtp",
    group: "[업종] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1516 — 업종별종목시세 (per-sector stock board; non-paginated; two inputs).
pub const T1516_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1516",
    path: "/indtp/market-data",
    module: "indtp",
    group: "[업종] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1514 — 업종기간별추이 (one sector's period trend; self-paginated on
/// `cts_date`). `has_pagination: true` mirrors `facets.self_paginated`.
pub const T1514_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1514",
    path: "/indtp/market-data",
    module: "indtp",
    group: "[업종] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// t1310 — 주식당일전일분틱조회 (today/prev tick-or-min chart; self-paginated on the
/// body `cts_time` cursor). `has_pagination: true` mirrors `facets.self_paginated`
/// (plan -003).
pub const T1310_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1310",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t1404 — 관리/불성실/투자유의조회 (administrative-designation board; self-paginated
/// on the body `cts_shcode` cursor). `has_pagination: true` mirrors
/// `facets.self_paginated` (plan -003).
pub const T1404_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1404",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1410 — 초저유동성조회 (ultra-low-liquidity board; self-paginated on the body
/// `cts_shcode` cursor). Plan -001, closed-window more-flips.
pub const T1410_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1410",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// t1411 — 증거금율별종목조회 (stocks by margin rate; single-page body-`idx`
/// paginated; `idx` serialized as a number). Plan -001, closed-window more-flips.
pub const T1411_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1411",
    path: "/stock/etc",
    module: "stock",
    group: "[주식] 기타",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1488 — 예상체결가등락율상위조회 (expected-execution price top change rate;
/// single-page body-`idx` paginated; numeric `idx`/`yesprice`/`yeeprice`/`yevolume`
/// serialized as JSON numbers). Plan -001, closed-window more-flips.
pub const T1488_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1488",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t1636 — 종목별프로그램매매동향 (per-stock program-trading trend; single-page
/// body-`cts_idx` paginated; numeric `cts_idx` cursor serialized as a JSON number
/// via `string_as_number`). Plan -001, closed-window more-flips.
pub const T1636_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1636",
    path: "/stock/program",
    module: "stock",
    group: "[주식] 프로그램",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1809 — 신호조회 (signal search; self-paginated on the body `cts` string
/// cursor — an ordinary in-block field at first-page `"1"`, NOT skipped). All
/// request fields are strings (no `string_as_number`). Plan -001, closed-window
/// more-flips.
pub const T1809_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1809",
    path: "/stock/item-search",
    module: "stock",
    group: "[주식] 종목검색",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
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

/// t1481 — 시간외등락율상위 (after-hours top change rate; single-page body-`idx`
/// paginated). `has_pagination: true` mirrors `facets.self_paginated`.
pub const T1481_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1481",
    path: "/stock/high-item",
    module: "stock",
    group: "[주식] 상위종목",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// t1482 — 시간외거래량상위 (after-hours top volume; single-page body-`idx`
/// paginated). `has_pagination: true` mirrors `facets.self_paginated`.
pub const T1482_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1482",
    path: "/stock/high-item",
    module: "stock",
    group: "[주식] 상위종목",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// t1403 — 신규상장종목조회 (newly-listed stocks; single-page body-`idx` paginated).
pub const T1403_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1403",
    path: "/stock/etc",
    module: "stock",
    group: "[주식] 기타",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// t1441 — 등락율상위 (top change rate; single-page body-`idx` paginated).
pub const T1441_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1441",
    path: "/stock/high-item",
    module: "stock",
    group: "[주식] 상위종목",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// t1463 — 거래대금상위 (top trading value; single-page body-`idx` paginated).
pub const T1463_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1463",
    path: "/stock/high-item",
    module: "stock",
    group: "[주식] 상위종목",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// t1466 — 전일동시간대비거래급증 (volume surge; single-page body-`idx` paginated).
pub const T1466_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1466",
    path: "/stock/high-item",
    module: "stock",
    group: "[주식] 상위종목",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// t1489 — 예상체결량상위조회 (top expected-execution volume; single-page paginated).
pub const T1489_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1489",
    path: "/stock/high-item",
    module: "stock",
    group: "[주식] 상위종목",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// t1492 — 단일가예상등락율상위 (single-price expected change rate; single-page).
pub const T1492_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1492",
    path: "/stock/high-item",
    module: "stock",
    group: "[주식] 상위종목",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// `t1866` — 서버저장조건 리스트조회 (server-saved condition list; the saved-condition
/// spine producer). Body-cursor single-page; `has_pagination` mirrors
/// `facets.self_paginated: true`.
pub const T1866_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1866",
    path: "/stock/item-search",
    module: "stock",
    group: "[주식] 종목검색",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
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

// ---------------------------------------------------------------------------
// Order class — the first `is_order: true` policy.
// ---------------------------------------------------------------------------

/// CSPAT00601 — 현물 정규주문 (domestic cash-equity order submission).
///
/// The FIRST `is_order: true` policy in the repo. It MUST route through
/// [`Inner::post_order`](crate::Inner::post_order) — the no-retry / dedup / kill
/// switch path — never `post`/`post_paginated`; `guard_order` enforces that.
/// Charges the `Orders` rate bucket. Registered in the policy-index crosscheck
/// ONLY — it must NOT appear in `slice_rest_policies_are_non_order_rest`, which
/// asserts every member is a non-order endpoint (R12).
pub const CSPAT00601_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CSPAT00601",
    path: "/stock/order",
    module: "stock",
    group: "[주식] 주문",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Orders,
    is_order: true,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(10),
};

/// CSPAT00701 — 현물정정주문 (domestic cash-equity order MODIFY).
///
/// An `is_order: true` policy: routes through [`Inner::post_order`](crate::Inner::post_order)
/// — the no-retry / dedup / kill switch path — never `post`/`post_paginated`.
/// Charges the `Orders` rate bucket. Registered in the policy-index crosscheck
/// ONLY — it must NOT appear in `slice_rest_policies_are_non_order_rest` (R12).
pub const CSPAT00701_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CSPAT00701",
    path: "/stock/order",
    module: "stock",
    group: "[주식] 주문",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Orders,
    is_order: true,
    has_pagination: false,
    rate_limit_per_sec: Some(3),
    corp_rate_limit_per_sec: Some(3),
};

/// CSPAT00801 — 현물취소주문 (domestic cash-equity order CANCEL).
///
/// An `is_order: true` policy, same dispatch contract as `CSPAT00701`. A cancel
/// re-sent identically within the dedup TTL is idempotent-for-free (the full body,
/// incl. `OrgOrdNo`, is the dedup key — KTD5). Registered in the policy-index
/// crosscheck ONLY (R12).
pub const CSPAT00801_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CSPAT00801",
    path: "/stock/order",
    module: "stock",
    group: "[주식] 주문",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Orders,
    is_order: true,
    has_pagination: false,
    rate_limit_per_sec: Some(3),
    corp_rate_limit_per_sec: Some(3),
};

/// t0425 — 주식체결/미체결 (stock filled/unfilled order inquiry).
///
/// The reconciliation companion to `CSPAT00601` — a READ (`is_order: false`),
/// NOT an order, so it dispatches through `post_paginated`. Self-paginates on the
/// `cts_ordno` body cursor (`has_pagination: true`) and charges the `MarketData`
/// bucket (KTD5). Registered in BOTH the policy-index crosscheck AND
/// `slice_rest_policies_are_non_order_rest` (it is a non-order REST read, R12).
pub const T0425_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t0425",
    path: "/stock/accno",
    module: "stock",
    group: "[주식] 계좌",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(10),
};

/// CSPAQ12300 — BEP단가조회 (account BEP / balance inquiry, read-only).
///
/// Dispatches through plain `Inner::post` (non-paginated): the result is
/// single-page (`facets.self_paginated: false`), so `has_pagination: false`.
pub const CSPAQ12300_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CSPAQ12300",
    path: "/stock/accno",
    module: "stock",
    group: "[주식] 계좌",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Account,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(10),
};

/// CSPAQ22200 — 현물계좌예수금 주문가능금액 총평가2 (account orderable-amount /
/// valuation inquiry, read-only).
///
/// Dispatches through plain `Inner::post` (non-paginated): the result is
/// single-page (`facets.self_paginated: false`), so `has_pagination: false`.
pub const CSPAQ22200_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CSPAQ22200",
    path: "/stock/accno",
    module: "stock",
    group: "[주식] 계좌",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Account,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(10),
};

/// t0424 — 주식잔고2 (stock balance v2, read-only account-state read).
///
/// Dispatches through plain `Inner::post` (non-paginated): the result is
/// single-page (`facets.self_paginated: false`), so `has_pagination: false`.
/// The cash summary persists regardless of market hours, so the read is
/// closure-viable; the per-holding array is empty on a cash-only account.
pub const T0424_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t0424",
    path: "/stock/accno",
    module: "stock",
    group: "[주식] 계좌",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Account,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(10),
};

/// CFOBQ10500 — 선물옵션 계좌예탁금증거금조회 (F/O account deposit / margin inquiry,
/// read-only).
///
/// Dispatches through plain `Inner::post` (non-paginated): the result is
/// single-page (`facets.self_paginated: false`), so `has_pagination: false`.
pub const CFOBQ10500_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CFOBQ10500",
    path: "/futureoption/accno",
    module: "futureoption",
    group: "[선물/옵션] 계좌",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Account,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(10),
};

/// CCENQ90200 — KRX야간파생 잔고조회 (KRX night-derivatives account balance inquiry,
/// read-only). Account-gated, krx_extended session.
///
/// Dispatches through plain `Inner::post` (non-paginated): the result is
/// single-page (`facets.self_paginated: false`), so `has_pagination: false`.
pub const CCENQ90200_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CCENQ90200",
    path: "/futureoption/accno",
    module: "futureoption",
    group: "[선물/옵션] 계좌",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Account,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// CFOAQ10100 — 선물옵션 주문가능수량조회 (F/O orderable-quantity inquiry, read-only —
/// an inquiry, NOT an order).
///
/// Dispatches through plain `Inner::post` (non-paginated): the result is
/// single-page (`facets.self_paginated: false`), so `has_pagination: false`.
pub const CFOAQ10100_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CFOAQ10100",
    path: "/futureoption/accno",
    module: "futureoption",
    group: "[선물/옵션] 계좌",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Account,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(10),
};

/// CCENQ10100 — KRX야간파생 주문가능수량 조회 (KRX night-derivatives orderable-quantity
/// inquiry, read-only — an inquiry, NOT an order). krx_extended session.
///
/// Dispatches through plain `Inner::post` (non-paginated): the result is
/// single-page (`facets.self_paginated: false`), so `has_pagination: false`.
pub const CCENQ10100_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CCENQ10100",
    path: "/futureoption/accno",
    module: "futureoption",
    group: "[선물/옵션] 계좌",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Account,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(10),
};

/// t2301 — 옵션전광판 (F/O option board; non-paginated market read). Keyed by a
/// contract month + mini/regular selector.
pub const T2301_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t2301",
    path: "/futureoption/market-data",
    module: "futureoption",
    group: "[선물/옵션] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t2522 — 주식선물기초자산조회 (stock-futures underlying-asset master;
/// non-paginated market read). No caller input (a single `dummy` placeholder).
pub const T2522_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t2522",
    path: "/futureoption/market-data",
    module: "futureoption",
    group: "[선물/옵션] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t8401 — 주식선물마스터조회 (stock-futures master; non-paginated market read).
/// No caller input (a single `dummy` placeholder).
pub const T8401_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8401",
    path: "/futureoption/market-data",
    module: "futureoption",
    group: "[선물/옵션] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t8426 — 상품선물마스터조회 (commodity-futures master; non-paginated market
/// read). No caller input (a single `dummy` placeholder).
pub const T8426_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8426",
    path: "/futureoption/market-data",
    module: "futureoption",
    group: "[선물/옵션] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t8433 — 지수옵션마스터조회API용 (index-option master; non-paginated market
/// read). No caller input (a single `dummy` placeholder).
pub const T8433_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8433",
    path: "/futureoption/market-data",
    module: "futureoption",
    group: "[선물/옵션] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t8435 — 파생종목마스터조회API용 (derivatives master; non-paginated market
/// read). Caller supplies a `gubun` segment selector — the LS spec defines these
/// as the MINI/weekly segments (`"MF"` 미니선물 / `"MO"` 미니옵션 / `"WK"`
/// 코스피200위클리옵션 / `"SF"` 코스닥150선물 / `"QW"` 코스닥150위클리옵션).
pub const T8435_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8435",
    path: "/futureoption/market-data",
    module: "futureoption",
    group: "[선물/옵션] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t8467 — 지수선물마스터조회API용 (index-futures master; F/O market-data read).
///
/// Non-paginated `market_session` read keyed by a `gubun` segment selector.
/// MarketData rate bucket; not an order.
pub const T8467_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8467",
    path: "/futureoption/market-data",
    module: "futureoption",
    group: "[선물/옵션] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t9943 — 지수선물마스터조회API용 (index-futures master; F/O market-data read).
///
/// Non-paginated `market_session` read keyed by a `gubun` segment selector.
/// MarketData rate bucket; not an order.
pub const T9943_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t9943",
    path: "/futureoption/market-data",
    module: "futureoption",
    group: "[선물/옵션] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t9944 — 지수옵션마스터조회API용 (index-option master, F/O). Non-paginated
/// market-data read; no caller input (a single `dummy` placeholder).
pub const T9944_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t9944",
    path: "/futureoption/market-data",
    module: "futureoption",
    group: "[선물/옵션] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t2111 — 선물/옵션현재가(시세)조회 (F/O current-price quote; non-paginated F/O
/// market-data read). Keyed by a contract `focode`.
pub const T2111_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t2111",
    path: "/futureoption/market-data",
    module: "futureoption",
    group: "[선물/옵션] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(5),
};

/// t2112 — 선물/옵션현재가호가조회 (F/O current-price order book; non-paginated F/O
/// market-data read). Keyed by a contract `shcode`.
pub const T2112_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t2112",
    path: "/futureoption/market-data",
    module: "futureoption",
    group: "[선물/옵션] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(5),
};

/// t2106 — 선물/옵션현재가시세메모 (F/O price memo; non-paginated F/O market-data
/// read). Keyed by a contract `code`.
pub const T2106_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t2106",
    path: "/futureoption/market-data",
    module: "futureoption",
    group: "[선물/옵션] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t8402 — 주식선물현재가조회(API용) (stock-futures current price; non-paginated F/O
/// market-data read). Keyed by a stock-futures contract `focode`.
pub const T8402_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8402",
    path: "/futureoption/market-data",
    module: "futureoption",
    group: "[선물/옵션] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(2),
};

/// t8403 — 주식선물호가조회(API용) (stock-futures order book; non-paginated F/O
/// market-data read). Keyed by a stock-futures contract `shcode`.
pub const T8403_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8403",
    path: "/futureoption/market-data",
    module: "futureoption",
    group: "[선물/옵션] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(2),
};

/// t8434 — 선물/옵션멀티현재가조회 (F/O multi current-price; non-paginated F/O
/// market-data read). Keyed by a numeric `qrycnt` + one or more `focode` codes.
pub const T8434_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8434",
    path: "/futureoption/market-data",
    module: "futureoption",
    group: "[선물/옵션] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(3),
    corp_rate_limit_per_sec: Some(5),
};

/// t1988 — 기초자산리스트조회 (ELW underlying-asset list; non-paginated market-data
/// read). Routes through `market_session` (KTD3 — placeholder `standalone`
/// owner_class is OAuth-only). `from_rate`/`to_rate` are numeric request slots.
pub const T1988_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1988",
    path: "/stock/elw",
    module: "stock",
    group: "[주식] ELW",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t3102 — 뉴스본문 (news body; non-paginated market-data read). Keyed by a news
/// number (`sNewsno`). Routes through `market_session` (KTD3).
pub const T3102_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t3102",
    path: "/stock/investinfo",
    module: "stock",
    group: "[주식] 투자정보",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t3320 — FNG_요약 (FnGuide company summary; non-paginated market-data read).
/// Keyed by a 7-char FnGuide company code (`gicode`). Routes through
/// `market_session` (KTD3).
pub const T3320_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t3320",
    path: "/stock/investinfo",
    module: "stock",
    group: "[주식] 투자정보",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t9945 — 주식마스터조회 (stock master; non-paginated `market_session` read).
/// One market per call; returns the full ticker master array (plan -004).
pub const T9945_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t9945",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t3202 — 종목별증시일정 (per-stock market schedule; non-paginated
/// `market_session` read). Keyed by `shcode` (plan -004).
pub const T3202_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t3202",
    path: "/stock/investinfo",
    module: "stock",
    group: "[주식] 투자정보",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t3401 — 투자의견 (per-broker investment-opinion history; self-paginated on the
/// body `cts_date` cursor). `has_pagination: true` mirrors `facets.self_paginated`
/// (plan -004).
pub const T3401_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t3401",
    path: "/stock/investinfo",
    module: "stock",
    group: "[주식] 투자정보",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t8410 — API전용주식차트(일주월년) (stock D/W/M/Y chart; self-paginated on the
/// body `cts_date` cursor) (plan -004).
pub const T8410_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8410",
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

/// t8451 — (통합)주식챠트(일주월년) (integrated stock D/W/M/Y chart; self-paginated
/// on the body `cts_date` cursor) (plan -004).
pub const T8451_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8451",
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

/// t8419 — 업종차트(일주월) (sector D/W/M chart; self-paginated on the body
/// `cts_date` cursor). `module: indtp` (plan -004).
pub const T8419_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8419",
    path: "/indtp/chart",
    module: "indtp",
    group: "[업종] 차트",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t4203 — 업종차트(종합) (composite sector chart; self-paginated on the body
/// `cts_date`/`cts_time` cursors). `module: indtp` (plan -004).
pub const T4203_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t4203",
    path: "/indtp/chart",
    module: "indtp",
    group: "[업종] 차트",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

// --- plan -004 batch A: chart/price family. Rate limits pinned from each TR's
//     own normalized baseline (not the mirror exemplar — see
//     docs/solutions/conventions/endpoint-policy-rate-limits-from-own-baseline.md).

/// t8417 — 업종차트(틱/n틱) (sector tick chart; self-paginated).
pub const T8417_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8417",
    path: "/indtp/chart",
    module: "indtp",
    group: "[업종] 차트",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t8418 — 업종차트(N분) (sector N-minute chart; self-paginated).
pub const T8418_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8418",
    path: "/indtp/chart",
    module: "indtp",
    group: "[업종] 차트",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t8411 — 주식차트(틱/n틱) (stock tick chart; self-paginated).
pub const T8411_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8411",
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

/// t8452 — (통합)주식챠트(N분) (integrated stock N-minute chart; self-paginated).
pub const T8452_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8452",
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

/// t8453 — (통합)주식챠트(틱/N틱) (integrated stock tick chart; self-paginated).
pub const T8453_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8453",
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

/// t1302 — 주식분별주가조회 (minute-by-minute price; non-paginated).
pub const T1302_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1302",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

// --- plan -004 batch B: domestic F/O chart/period family. Rate limits pinned
//     from each TR's own normalized baseline.

/// t8464 — 선물옵션차트(틱/n틱) (F/O tick chart; self-paginated).
pub const T8464_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8464",
    path: "/futureoption/chart",
    module: "futureoption",
    group: "[선물/옵션] 차트",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t8465 — 선물/옵션차트(N분) (F/O N-minute chart; self-paginated).
pub const T8465_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8465",
    path: "/futureoption/chart",
    module: "futureoption",
    group: "[선물/옵션] 차트",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t8466 — 선물/옵션차트(일주월) (F/O day/week/month chart; self-paginated).
pub const T8466_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8466",
    path: "/futureoption/chart",
    module: "futureoption",
    group: "[선물/옵션] 차트",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t8405 — 주식선물기간별주가(API용) (stock-futures period price; self-paginated).
pub const T8405_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8405",
    path: "/futureoption/market-data",
    module: "futureoption",
    group: "[선물/옵션] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t2216 — 선물옵션틱분별체결조회차트 (F/O tick/min trade chart; non-paginated).
pub const T2216_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t2216",
    path: "/futureoption/chart",
    module: "futureoption",
    group: "[선물/옵션] 차트",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};


// --- plan -004 batch C: static reference / designation / ranking boards.
//     Rate limits pinned from each TR's own normalized baseline.

/// t1444 — market_cap_top ([주식] 상위종목).
pub const T1444_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1444",
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

/// t1422 — price_limit ([주식] 시세).
pub const T1422_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1422",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t1427 — price_limit_imminent ([주식] 시세).
pub const T1427_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1427",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t1442 — new_high_low ([주식] 시세).
pub const T1442_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1442",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t1405 — trade_suspension ([주식] 시세).
pub const T1405_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1405",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1960 — elw_change_rank ([주식] ELW).
pub const T1960_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1960",
    path: "/stock/elw",
    module: "stock",
    group: "[주식] ELW",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(5),
};

/// t1961 — elw_volume_rank ([주식] ELW).
pub const T1961_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1961",
    path: "/stock/elw",
    module: "stock",
    group: "[주식] ELW",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(5),
};

/// t1966 — elw_value_rank ([주식] ELW).
pub const T1966_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1966",
    path: "/stock/elw",
    module: "stock",
    group: "[주식] ELW",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(5),
};

/// t1921 — credit_trend ([주식] 기타).
pub const T1921_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1921",
    path: "/stock/etc",
    module: "stock",
    group: "[주식] 기타",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1532 — stock_themes ([주식] 섹터).
pub const T1532_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1532",
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

/// t1533 — special_themes ([주식] 섹터).
pub const T1533_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1533",
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

/// t1926 — credit_info ([주식] 기타).
pub const T1926_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1926",
    path: "/stock/etc",
    module: "stock",
    group: "[주식] 기타",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1764 — member_firms ([주식] 거래원).
pub const T1764_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1764",
    path: "/stock/exchange",
    module: "stock",
    group: "[주식] 거래원",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1903 — etf_daily_trend ([주식] ETF).
pub const T1903_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1903",
    path: "/stock/etf",
    module: "stock",
    group: "[주식] ETF",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t8455 — KRX야간파생 마스터조회(API용) (night-derivatives master; non-paginated
/// F/O market-data read). `venue_session: krx_extended` (KTD7). Keyed by a
/// `gubun` class selector.
pub const T8455_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8455",
    path: "/futureoption/market-data",
    module: "futureoption",
    group: "[선물/옵션] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t8460 — KRX야간파생 옵션 전광판 (night-derivatives option board; non-paginated
/// F/O market-data read). `venue_session: krx_extended` (KTD7). Keyed by a
/// contract month `yyyymm` + an index `gubun`.
pub const T8460_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8460",
    path: "/futureoption/market-data",
    module: "futureoption",
    group: "[선물/옵션] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(3),
};

/// t8463 — KRX야간파생 투자자시간대별(API용) (night-derivatives investor-by-timeslot;
/// non-paginated F/O investor read). `venue_session: krx_extended` (KTD7). Keyed
/// by `tm_rng`/`fot_clsf_cd`/`bsc_asts_id`; `cnt` is a numeric request slot.
pub const T8463_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8463",
    path: "/futureoption/investor",
    module: "futureoption",
    group: "[선물/옵션] 투자자",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// g3101 — 해외주식 현재가 조회 (overseas current-price; non-paginated market-data
/// read). Domain `overseas_stock`; routes through `market_session`.
pub const G3101_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "g3101",
    path: "/overseas-stock/market-data",
    module: "overseas-stock",
    group: "[해외주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(50),
};

/// g3104 — 해외주식 종목정보 조회 (overseas stock-info master; non-paginated
/// market-data read).
pub const G3104_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "g3104",
    path: "/overseas-stock/market-data",
    module: "overseas-stock",
    group: "[해외주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(50),
};

/// g3106 — 해외주식 현재가호가 조회 (overseas current-price + order book;
/// non-paginated market-data read).
pub const G3106_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "g3106",
    path: "/overseas-stock/market-data",
    module: "overseas-stock",
    group: "[해외주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(50),
};

/// g3102 — 해외주식 시간대별 (overseas time-series ticks; non-paginated market-data
/// read). `readcnt`/`cts_seq` are numeric request slots (KTD4).
pub const G3102_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "g3102",
    path: "/overseas-stock/market-data",
    module: "overseas-stock",
    group: "[해외주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(50),
};

/// g3103 — 해외주식 일주월 조회 (overseas daily/weekly/monthly chart; non-paginated
/// market-data read; lower-rate chart bucket).
pub const G3103_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "g3103",
    path: "/overseas-stock/chart",
    module: "overseas-stock",
    group: "[해외주식] 차트",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(10),
};

/// g3190 — 해외주식 마스터 조회 (overseas master list; non-paginated market-data
/// read). `readcnt` is a numeric request slot (KTD4).
pub const G3190_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "g3190",
    path: "/overseas-stock/market-data",
    module: "overseas-stock",
    group: "[해외주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(50),
};

/// o3101 — 해외선물마스터조회 (overseas-futures master list; non-paginated
/// market-data read; ARRAY out-block). `gubun` filters, no instrument id.
pub const O3101_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "o3101",
    path: "/overseas-futureoption/market-data",
    module: "overseas-futureoption",
    group: "[해외선물] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(10),
};

/// o3121 — 해외선물옵션 마스터 조회 (overseas-future-option master list;
/// non-paginated market-data read; ARRAY out-block).
pub const O3121_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "o3121",
    path: "/overseas-futureoption/market-data",
    module: "overseas-futureoption",
    group: "[해외선물] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(10),
};

/// o3105 — 해외선물 현재가(종목정보) 조회 (overseas-futures current price /
/// symbol info; non-paginated market-data read; single out-block).
pub const O3105_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "o3105",
    path: "/overseas-futureoption/market-data",
    module: "overseas-futureoption",
    group: "[해외선물] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(50),
};

/// o3106 — 해외선물 현재가호가 조회 (overseas-futures current price + order book;
/// non-paginated market-data read; single out-block).
pub const O3106_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "o3106",
    path: "/overseas-futureoption/market-data",
    module: "overseas-futureoption",
    group: "[해외선물] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(50),
};

/// o3125 — 해외선물옵션 현재가(종목정보) 조회 (overseas-future-option current price /
/// symbol info; non-paginated market-data read; single out-block).
pub const O3125_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "o3125",
    path: "/overseas-futureoption/market-data",
    module: "overseas-futureoption",
    group: "[해외선물] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(50),
};

/// o3126 — 해외선물옵션 현재가호가 조회 (overseas-future-option current price +
/// order book; non-paginated market-data read; single out-block).
pub const O3126_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "o3126",
    path: "/overseas-futureoption/market-data",
    module: "overseas-futureoption",
    group: "[해외선물] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(2),
    corp_rate_limit_per_sec: Some(50),
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
            T1481_POLICY,
            T1482_POLICY,
            T1403_POLICY,
            T1441_POLICY,
            T1463_POLICY,
            T1466_POLICY,
            T1489_POLICY,
            T1492_POLICY,
            T1859_POLICY,
            T1866_POLICY,
            T1825_POLICY,
            T1826_POLICY,
            T9905_POLICY,
            T9907_POLICY,
            T8431_POLICY,
            T8430_POLICY,
            T9942_POLICY,
            T1958_POLICY,
            T1964_POLICY,
            T1601_POLICY,
            T1615_POLICY,
            T1640_POLICY,
            T1662_POLICY,
            T1664_POLICY,
            T3341_POLICY,
            T8424_POLICY,
            T1511_POLICY,
            T1485_POLICY,
            T1516_POLICY,
            T1514_POLICY,
            T1901_POLICY,
            T1105_POLICY,
            T1104_POLICY,
            T1305_POLICY,
            // Closed-window more-flips wave (plan -001): ETF LP order-book read.
            T1906_POLICY,
            // Closed-window more-flips wave (plan -001): integrated current-price/order-book read.
            T8450_POLICY,
            // Closed-window more-flips wave (plan -001): per-stock remaining-quantity/pre-disclosure read.
            T1638_POLICY,
            // Closed-window more-flips wave (plan -001): time-bucketed trade-chart read.
            T1308_POLICY,
            // Closed-window more-flips wave (plan -001): price-band trade-weight read.
            T1449_POLICY,
            // Closed-window more-flips wave (plan -001): by-time investor-trading read.
            T1621_POLICY,
            // Closed-window more-flips wave (plan -001): F/O by-time investor-trading read.
            T2545_POLICY,
            // Closed-window more-flips wave (plan -001): F/O by-tick conclusion-board read.
            T8406_POLICY,
            // Closed-window more-flips wave (plan -001): multi-symbol current-price read.
            T8407_POLICY,
            // Closed-window more-flips wave (plan -001): LP-target ELW issue list read.
            T1959_POLICY,
            // Closed-window more-flips wave (plan -001): ELW current-price/quote read.
            T1950_POLICY,
            // Closed-window more-flips wave (plan -001): ELW current-price + quote-board read.
            T1971_POLICY,
            // Closed-window more-flips wave (plan -001): ELW current-price + trading-member board read.
            T1972_POLICY,
            // Closed-window more-flips wave (plan -001): ELWs sharing a base asset read.
            T1974_POLICY,
            // Closed-window more-flips wave (plan -001): ELW current-price / contracted-payout snapshot read.
            T1956_POLICY,
            // Closed-window more-flips wave (plan -001): ELW screener / indicator search.
            T1969_POLICY,
            // Closed-window flip wave (plan -003): self-paginated stock reads.
            T1310_POLICY,
            T1404_POLICY,
            // Closed-window more-flips wave (plan -001): self-paginated stock read.
            T1410_POLICY,
            // Closed-window more-flips wave (plan -001): self-paginated stock read (margin rates).
            T1411_POLICY,
            // Closed-window more-flips wave (plan -001): self-paginated stock read (expected-exec).
            T1488_POLICY,
            // Closed-window more-flips wave (plan -001): self-paginated stock read (program-trading).
            T1636_POLICY,
            // Closed-window more-flips wave (plan -001): self-paginated stock read (signal search).
            T1809_POLICY,
            CSPAQ12200_POLICY,
            CSPAQ12300_POLICY,
            CSPAQ22200_POLICY,
            // Closed-window account-lane flip wave (plan -001): non-order account read.
            T0424_POLICY,
            // t0425 IS a non-order REST read — it belongs here AND in the
            // crosscheck. CSPAT00601 (is_order: true) deliberately does NOT.
            T0425_POLICY,
            CFOBQ10500_POLICY,
            CCENQ90200_POLICY,
            CFOAQ10100_POLICY,
            CCENQ10100_POLICY,
            T2301_POLICY,
            T2522_POLICY,
            T8401_POLICY,
            T8426_POLICY,
            T8433_POLICY,
            T8435_POLICY,
            T8467_POLICY,
            T9943_POLICY,
            T9944_POLICY,
            T2111_POLICY,
            T2112_POLICY,
            T2106_POLICY,
            T8402_POLICY,
            T8403_POLICY,
            T8434_POLICY,
            T1988_POLICY,
            T3102_POLICY,
            T3320_POLICY,
            // Domestic stock master/reference breadth wave (plan -004): non-order
            // REST reads — registered in BOTH crosscheck lists.
            T9945_POLICY,
            T3202_POLICY,
            T3401_POLICY,
            T8410_POLICY,
            T8451_POLICY,
            T8419_POLICY,
            T4203_POLICY,
            T8417_POLICY,
            T8418_POLICY,
            T8411_POLICY,
            T8452_POLICY,
            T8453_POLICY,
            T1302_POLICY,
            T8464_POLICY,
            T8465_POLICY,
            T8466_POLICY,
            T8405_POLICY,
            T2216_POLICY,
            T1444_POLICY,
            T1422_POLICY,
            T1427_POLICY,
            T1442_POLICY,
            T1405_POLICY,
            T1960_POLICY,
            T1961_POLICY,
            T1966_POLICY,
            T1921_POLICY,
            T1532_POLICY,
            T1533_POLICY,
            T1926_POLICY,
            T1764_POLICY,
            T1903_POLICY,
            T8455_POLICY,
            T8460_POLICY,
            T8463_POLICY,
            G3101_POLICY,
            G3104_POLICY,
            G3106_POLICY,
            G3102_POLICY,
            G3103_POLICY,
            G3190_POLICY,
            O3101_POLICY,
            O3121_POLICY,
            O3105_POLICY,
            O3106_POLICY,
            O3125_POLICY,
            O3126_POLICY,
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
