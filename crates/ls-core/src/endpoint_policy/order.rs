//! Order-class REST policies (`is_order: true`) and their inquiry siblings.
//!
//! Wave-3 split out of `endpoint_policy.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use order::*;` so every `endpoint_policy::FOO_POLICY` path is unchanged.
use super::*;


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

/// CFOAT00100 — 선물옵션 정상주문 (domestic F/O order SUBMIT).
///
/// The F/O sibling of `CSPAT00601`: an `is_order: true` policy routing through
/// [`Inner::post_order`](crate::Inner::post_order) (no-retry / dedup / kill switch),
/// charging the `Orders` bucket. Registered in the policy-index crosscheck ONLY — it
/// must NOT appear in `slice_rest_policies_are_non_order_rest` (R12/KTD4).
pub const CFOAT00100_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CFOAT00100",
    path: "/futureoption/order",
    module: "futureoption",
    group: "[선물/옵션] 주문",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Orders,
    is_order: true,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(10),
};

/// CFOAT00200 — 선물옵션 정정주문 (domestic F/O order MODIFY).
///
/// An `is_order: true` policy, same dispatch contract as `CFOAT00100`. Registered in
/// the policy-index crosscheck ONLY (R12/KTD4).
pub const CFOAT00200_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CFOAT00200",
    path: "/futureoption/order",
    module: "futureoption",
    group: "[선물/옵션] 주문",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Orders,
    is_order: true,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(10),
};

/// CFOAT00300 — 선물옵션 취소주문 (domestic F/O order CANCEL).
///
/// An `is_order: true` policy, same dispatch contract as `CFOAT00100`. A cancel
/// re-sent identically within the dedup TTL is idempotent-for-free (the full body,
/// incl. `OrgOrdNo`, is the dedup key). Registered in the policy-index crosscheck
/// ONLY (R12/KTD4).
pub const CFOAT00300_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CFOAT00300",
    path: "/futureoption/order",
    module: "futureoption",
    group: "[선물/옵션] 주문",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Orders,
    is_order: true,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(10),
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

/// CSPBQ00200 — 현물계좌증거금률별주문가능수량조회 (orderable-quantity / capacity by
/// margin rate, read-only account-state read).
///
/// Dispatches through plain `Inner::post` (non-paginated): the result is
/// single-page (`facets.self_paginated: false`), so `has_pagination: false`.
pub const CSPBQ00200_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CSPBQ00200",
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

/// CLNAQ00100 — 예탁담보융자가능종목현황조회 (loanable-collateral stock list,
/// read-only). Account-aware reference data on `/stock/etc`.
///
/// Dispatches through plain `Inner::post` (non-paginated): the result is
/// single-page (`facets.self_paginated: false`), so `has_pagination: false`.
pub const CLNAQ00100_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CLNAQ00100",
    path: "/stock/etc",
    module: "stock",
    group: "[주식] 기타",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Account,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(30),
};

/// CFOEQ11100 — 선물옵션가정산예탁금상세 (F/O provisional-settlement deposit detail,
/// read-only account-state read).
///
/// Dispatches through plain `Inner::post` (non-paginated): the result is
/// single-page (`facets.self_paginated: false`), so `has_pagination: false`.
pub const CFOEQ11100_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CFOEQ11100",
    path: "/futureoption/accno",
    module: "futureoption",
    group: "[선물/옵션] 계좌",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Account,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(5),
};

/// CIDBQ01400 — 해외선물 체결내역개별 조회(주문가능수량) (overseas-futures orderable-
/// quantity inquiry, read-only account-state read).
///
/// Dispatches through plain `Inner::post` (non-paginated): the result is
/// single-page (`facets.self_paginated: false`), so `has_pagination: false`.
pub const CIDBQ01400_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CIDBQ01400",
    path: "/overseas-futureoption/accno",
    module: "overseas-futureoption",
    group: "[해외선물] 계좌",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Account,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(10),
};

/// CIDBQ03000 — 해외선물 예수금/잔고현황 (overseas-futures deposit/balance status,
/// read-only account-state read).
///
/// Dispatches through plain `Inner::post` (non-paginated): the result is
/// single-page (`facets.self_paginated: false`), so `has_pagination: false`.
pub const CIDBQ03000_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CIDBQ03000",
    path: "/overseas-futureoption/accno",
    module: "overseas-futureoption",
    group: "[해외선물] 계좌",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Account,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(10),
};

/// CIDBQ05300 — 해외선물 예탁자산 조회 (overseas-futures deposited-assets inquiry,
/// read-only account-state read).
///
/// Dispatches through plain `Inner::post` (non-paginated): the result is
/// single-page (`facets.self_paginated: false`), so `has_pagination: false`.
pub const CIDBQ05300_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "CIDBQ05300",
    path: "/overseas-futureoption/accno",
    module: "overseas-futureoption",
    group: "[해외선물] 계좌",
    protocol: Protocol::Rest,
    category: RateLimitCategory::Account,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(1),
    corp_rate_limit_per_sec: Some(10),
};

/// t0441 — 선물/옵션잔고평가(이동평균) (F/O balance valuation, read-only account-state
/// read).
///
/// Dispatches through plain `Inner::post` (non-paginated): the result is
/// single-page (`facets.self_paginated: false`), so `has_pagination: false`.
pub const T0441_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t0441",
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

/// t0167 — 서버시간조회 (server-time utility read, read-only, stateless).
///
/// Dispatches through plain `Inner::post` (non-paginated). Not account-scoped —
/// `market_data` bucket, `/etc/time-search`.
pub const T0167_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t0167",
    path: "/etc/time-search",
    module: "etc",
    group: "[기타] 시간조회",
    protocol: Protocol::Rest,
    category: RateLimitCategory::MarketData,
    is_order: false,
    has_pagination: false,
    rate_limit_per_sec: Some(10),
    corp_rate_limit_per_sec: Some(50),
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

/// t3518 — 해외실시간지수 (overseas equity-index time-series via /stock/investinfo;
/// self-paginated on the body `cts_date`/`cts_time` cursor). All-lane closed-window
/// flip wave (plan -003).
pub const T3518_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t3518",
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

/// t3521 — 해외지수조회 (one overseas index's current snapshot via /stock/investinfo;
/// non-paginated). All-lane closed-window flip wave (plan -003).
pub const T3521_POLICY: EndpointPolicy = EndpointPolicy {
    tr_code: "t3521",
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
