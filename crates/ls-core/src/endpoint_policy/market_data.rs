//! Domestic & overseas market-data read (non-order REST) policies.
//!
//! Wave-3 split out of `endpoint_policy.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use market_data::*;` so every `endpoint_policy::FOO_POLICY` path is unchanged.
use super::*;


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

/// t1716 — 외인기관종목별동향 (foreign/institution by-issue trend; non-paginated).
/// path /stock/frgr-itt, group [주식] 외인/기관.
pub const T1716_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1716",
    path: "/stock/frgr-itt",
    module: "stock",
    group: "[주식] 외인/기관",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1902 — ETF시간별추이 (ETF intraday NAV/price trend; non-paginated).
/// path /stock/etf, group [주식] ETF.
pub const T1902_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1902",
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

/// t1904 — ETF구성종목조회 (ETF PDF / constituent basket; non-paginated).
/// path /stock/etf, group [주식] ETF.
pub const T1904_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1904",
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

/// t1927 — 공매도일별추이 (short-selling daily trend; non-paginated).
/// path /stock/etc, group [주식] 기타.
pub const T1927_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1927",
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

/// t1941 — 종목별대차거래일간추이 (per-issue stock-loan/대차 daily trend; non-paginated).
/// path /stock/etc, group [주식] 기타.
pub const T1941_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1941",
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

/// t1631 — 프로그램매매종합조회 (program-trade综合; non-paginated).
/// path /stock/program, group [주식] 프로그램.
pub const T1631_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1631",
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

/// t1632 — 프로그램매매추이(시간) (program-trade intraday trend; non-paginated).
/// path /stock/program, group [주식] 프로그램.
pub const T1632_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1632",
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

/// t1633 — 프로그램매매추이(일별) (program-trade daily trend; non-paginated).
/// path /stock/program, group [주식] 프로그램.
pub const T1633_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1633",
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

/// t1702 — 외국인/기관별 매매추이 (foreign/institution by-issue trend; non-paginated).
/// path /stock/frgr-itt, group [주식] 외인/기관.
pub const T1702_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1702",
    path: "/stock/frgr-itt",
    module: "stock",
    group: "[주식] 외인/기관",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1717 — 외국인/기관 순매수추이 (foreign/institution net-buy trend; non-paginated).
/// path /stock/frgr-itt, group [주식] 외인/기관.
pub const T1717_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1717",
    path: "/stock/frgr-itt",
    module: "stock",
    group: "[주식] 외인/기관",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1665 — 투자자별 매매추이(업종) (investor-by-sector trend chart; non-paginated).
/// path /stock/chart, group [주식] 차트.
pub const T1665_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1665",
    path: "/stock/chart",
    module: "stock",
    group: "[주식] 차트",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1471 — 시간대별호가잔량추이 (intraday best-quote-remainder trend; non-paginated).
/// path /stock/market-data, group [주식] 시세.
pub const T1471_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1471",
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

/// t1475 — VP대비등락률상하위 (VP-relative rise/fall ranking; non-paginated).
/// path /stock/market-data, group [주식] 시세. corp rate is 2/s (per baseline).
pub const T1475_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1475",
    path: "/stock/market-data",
    module: "stock",
    group: "[주식] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(2),
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

/// t1109 — 시간외체결량 (after-hours tick conclusion; self-paginated on the body
/// `dan_chetime`/`idx` cursor, `idx` serialized as a JSON number). Open-window
/// domestic reads (plan -001).
pub const T1109_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1109",
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

/// t1301 — 시간대별체결조회 (time-band tick conclusion; self-paginated on the body
/// `cts_time` cursor; `cvolume` filter serialized as a JSON number). Open-window
/// domestic reads (plan -001).
pub const T1301_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1301",
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

/// t1486 — 예상체결가등락율 (expected-conclusion; self-paginated on the body
/// `cts_time` cursor; `cnt` count serialized as a JSON number). Open-window
/// domestic reads (plan -001).
pub const T1486_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1486",
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

/// t8454 — 시간대별체결조회 (exchange-qualified time-band tick conclusion;
/// self-paginated on the body `cts_time` cursor; `cvolume` filter serialized as a
/// JSON number). Open-window domestic reads (plan -001).
pub const T8454_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8454",
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

/// t1637 — 프로그램매매추이(종목별) (per-stock program-trade flow; self-paginated on
/// the body `cts_idx` cursor serialized as a JSON number). Open-window domestic
/// reads (plan -001).
pub const T1637_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1637",
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

/// t1602 — 시간대별투자자매매추이 (time-band investor flow by sector; self-paginated
/// on the body `cts_time` cursor; `cts_idx`/`cnt` serialized as JSON numbers).
/// Open-window domestic reads (plan -001).
pub const T1602_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1602",
    path: "/stock/investor",
    module: "stock",
    group: "[주식] 투자자",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1603 — 투자자별매매종목 (investor detail by issue; self-paginated on the body
/// `cts_time` cursor; `cts_idx`/`cnt` serialized as JSON numbers). Open-window
/// domestic reads (plan -001).
pub const T1603_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1603",
    path: "/stock/investor",
    module: "stock",
    group: "[주식] 투자자",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1617 — 투자자별일별매매추이 (investor time/daily flow; self-paginated on the body
/// `cts_date`/`cts_time` cursors; all request slots String). Open-window domestic
/// reads (plan -001).
pub const T1617_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1617",
    path: "/stock/investor",
    module: "stock",
    group: "[주식] 투자자",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1752 — 거래원별종목별동향 (broker-by-issue; self-paginated on the body `cts_idx`
/// cursor serialized as a JSON number). Open-window domestic reads (plan -001).
pub const T1752_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1752",
    path: "/stock/exchange",
    module: "stock",
    group: "[주식] 거래원",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t1771 — 거래원별시간대별추이 (broker time-series by issue; self-paginated on the
/// body `cts_idx` cursor; `cts_idx`/`cnt` serialized as JSON numbers; row array
/// under `t1771OutBlock2`). Open-window domestic reads (plan -001).
pub const T1771_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t1771",
    path: "/stock/exchange",
    module: "stock",
    group: "[주식] 거래원",
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
