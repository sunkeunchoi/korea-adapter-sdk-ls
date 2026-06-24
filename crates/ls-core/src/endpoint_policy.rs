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
            CSPAQ12200_POLICY,
            CSPAQ12300_POLICY,
            CSPAQ22200_POLICY,
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
