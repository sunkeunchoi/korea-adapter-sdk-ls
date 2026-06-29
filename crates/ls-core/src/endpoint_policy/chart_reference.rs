//! Chart / price-band / static-reference / ranking-board REST policies (plan -004 batches A–C).
//!
//! Wave-3 split out of `endpoint_policy.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use chart_reference::*;` so every `endpoint_policy::FOO_POLICY` path is unchanged.
use super::*;


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

/// o3103 — 해외선물차트 분봉 조회 (overseas-futures minute chart; self-paginated on
/// the body `cts_date`/`cts_time` cursor). All-lane closed-window flip wave (plan -003).
pub const O3103_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "o3103",
    path: "/overseas-futureoption/chart",
    module: "overseas-futureoption",
    group: "[해외선물] 차트",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(10),
};

/// o3104 — 해외선물 일별체결 조회 (overseas-futures daily executions; non-paginated
/// market-data read; array out-block). Plan -003.
pub const O3104_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "o3104",
    path: "/overseas-futureoption/market-data",
    module: "overseas-futureoption",
    group: "[해외선물] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// o3108 — 해외선물차트(일주월) 조회 (overseas-futures D/W/M chart; self-paginated on
/// the body `cts_date` cursor). Plan -003.
pub const O3108_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "o3108",
    path: "/overseas-futureoption/chart",
    module: "overseas-futureoption",
    group: "[해외선물] 차트",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(10),
};

/// o3116 — 해외선물 시간대별(Tick)체결 조회 (overseas-futures tick; self-paginated on
/// the body `cts_seq` cursor). Plan -003.
pub const O3116_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "o3116",
    path: "/overseas-futureoption/market-data",
    module: "overseas-futureoption",
    group: "[해외선물] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// o3117 — 해외선물 차트 NTick 체결 조회 (overseas-futures NTick chart; self-paginated
/// on the body `cts_seq`/`cts_daygb` cursor). Plan -003.
pub const O3117_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "o3117",
    path: "/overseas-futureoption/chart",
    module: "overseas-futureoption",
    group: "[해외선물] 차트",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// o3123 — 해외선물옵션 차트 분봉 조회 (overseas-futopt minute chart; self-paginated on
/// the body `cts_date`/`cts_time` cursor). Plan -003.
pub const O3123_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "o3123",
    path: "/overseas-futureoption/market-data",
    module: "overseas-futureoption",
    group: "[해외선물] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// o3127 — 해외선물옵션 관심종목 조회 (overseas-futopt watchlist board; non-paginated
/// market-data read; array out-block). Plan -003.
pub const O3127_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "o3127",
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

/// t8427 — 선물옵션 N분주가 (F/O minute/day chart; non-paginated). Open-window flip
/// wave (plan -001). path /futureoption/market-data, group [선물/옵션] 시세.
pub const T8427_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8427",
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

/// t2210 — 선물옵션 특이거래량 (F/O unusual-volume conclusion counts; non-paginated).
/// Open-window flip wave (plan -001). path /futureoption/market-data, group [선물/옵션] 시세.
pub const T2210_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t2210",
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

/// t2424 — 선물옵션 N분봉 (F/O N-minute bars; non-paginated). Open-window flip wave
/// (plan -001). path /futureoption/market-data, group [선물/옵션] 시세.
pub const T2424_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t2424",
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

/// t2541 — 선물옵션 투자자별 매매추이 (F/O investor-by-time; self-paginated on the body
/// `cts_time`/`cts_idx` cursor). Open-window flip wave (plan -001).
/// path /futureoption/investor, group [선물/옵션] 투자자.
pub const T2541_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t2541",
    path: "/futureoption/investor",
    module: "futureoption",
    group: "[선물/옵션] 투자자",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(3),
};

/// t2214 — 선물옵션 기간별주가 (F/O daily OHLCV; self-paginated on the body `cts_code`
/// cursor). Open-window flip wave (plan -001).
/// path /futureoption/market-data, group [선물/옵션] 시세.
pub const T2214_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t2214",
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

/// t8428 — 투자자별 예탁금추이 (deposit-balance trend by investor; non-paginated).
/// Open-window flip wave (plan -001). path /stock/investinfo, group [주식] 투자정보.
pub const T8428_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8428",
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

/// o3128 — 해외선물옵션 차트 일주월 조회 (overseas-futopt D/W/M chart; self-paginated on
/// the body `cts_date` cursor). Plan -003.
pub const O3128_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "o3128",
    path: "/overseas-futureoption/market-data",
    module: "overseas-futureoption",
    group: "[해외선물] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// o3136 — 해외선물옵션 시간대별 Tick 체결 조회 (overseas-futopt tick; self-paginated on
/// the body `cts_seq` cursor). Plan -003.
pub const O3136_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "o3136",
    path: "/overseas-futureoption/market-data",
    module: "overseas-futureoption",
    group: "[해외선물] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// o3137 — 해외선물옵션 차트 NTick 체결 조회 (overseas-futopt NTick chart; self-paginated
/// on the body `cts_seq`/`cts_daygb` cursor). Plan -003.
pub const O3137_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "o3137",
    path: "/overseas-futureoption/market-data",
    module: "overseas-futureoption",
    group: "[해외선물] 시세",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(1),
};

/// o3139 — 해외선물옵션차트용NTick(고정형) (overseas-futopt NTick fixed chart;
/// self-paginated on the body `cts_seq`/`cts_daygb` cursor). Plan -003.
pub const O3139_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "o3139",
    path: "/overseas-futureoption/chart",
    module: "overseas-futureoption",
    group: "[해외선물] 차트",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: true,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(10),
};

/// t8462 — KRX야간파생 투자자기간별 (KRX night-derivatives investor-period table;
/// non-paginated market-data read; array out-block). Plan -003.
pub const T8462_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t8462",
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
