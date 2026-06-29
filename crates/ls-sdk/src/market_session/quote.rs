//! Domestic equity current-price / order-book / multi-symbol quote reads.
//!
//! Wave-1 split out of `market_session/mod.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use quote::*;` so every `ls_sdk::market_session::*` path is unchanged.
use super::*;


/// Input block for `t1102` — the symbol to quote.
///
/// `shcode` is the 6-digit short code (단축코드). `exchgubun` is the exchange
/// distinction (거래소 구분). Both are caller-supplied identifiers.
#[derive(Serialize, Debug, Clone)]
pub struct T1102InBlock {
    /// Short code / 단축코드 (e.g. `"078020"`).
    pub shcode: String,
    /// Exchange distinction / 거래소 구분.
    pub exchgubun: String,
}

/// `t1102` request — wraps the input block under the `t1102InBlock` key.
///
/// Serializes to `{"t1102InBlock":{"shcode":...,"exchgubun":...}}`. There are no
/// `tr_cont`/`tr_cont_key` fields: `t1102` is not paginated, so the continuation
/// tokens are structurally absent from the body.
#[derive(Serialize, Debug, Clone)]
pub struct T1102Request {
    #[serde(rename = "t1102InBlock")]
    pub inblock: T1102InBlock,
}

impl T1102Request {
    /// Build a `t1102` request for one symbol on one exchange.
    pub fn new(shcode: impl Into<String>, exchgubun: impl Into<String>) -> Self {
        T1102Request {
            inblock: T1102InBlock {
                shcode: shcode.into(),
                exchgubun: exchgubun.into(),
            },
        }
    }
}

/// `t1102OutBlock` — the snapshot quote.
///
/// A representative, spec-grounded subset of the LS `t1102OutBlock`: the core
/// quote fields plus the level-1 bid/offer aggregates. Every numeric-bearing
/// field uses [`ls_core::string_or_number`] because the gateway sends them as
/// either JSON numbers or JSON strings; `#[serde(default)]` on the struct lets a
/// sparse/empty out-block deserialize cleanly. Field names mirror the LS spec
/// (`specs/ls_openapi_specs.json` → `t1102OutBlock`) verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1102OutBlock {
    /// Korean name / 한글 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비 구분 (e.g. `"2"` = up). Arrives as a string in the spec.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Rate of change (%) / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Open / 시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
    /// High / 고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// Low / 저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// Upper limit price / 상한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub uplmtprice: String,
    /// Lower limit price / 하한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dnlmtprice: String,
    /// Previous day's volume / 전일거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilvolume: String,
    /// Volume difference vs. previous day / 거래량 대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volumediff: String,
}

/// `t1102` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields (classified in
/// `ls-core` dispatch before this struct is built); `outblock` is the snapshot
/// quote under the `t1102OutBlock` key. All three are `#[serde(default)]` so a
/// terse or partial envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1102Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1102OutBlock", default)]
    pub outblock: T1102OutBlock,
}

// ---------------------------------------------------------------------------
// t8450 — (통합)주식현재가호가조회2 (integrated current-price + order-book level-2
// snapshot). market_session read, single OutBlock object; path /stock/market-data.
// shcode + exchgubun request. Mirrors t1105's two-field InBlock shape.
// ---------------------------------------------------------------------------

/// Input block for `t8450` — short code (단축코드) + exchange distinction (거래소구분코드).
#[derive(Serialize, Debug, Clone)]
pub struct T8450InBlock {
    /// Short code / 단축코드 (e.g. `"005930"` Samsung Electronics).
    pub shcode: String,
    /// Exchange distinction / 거래소구분코드 (e.g. `"N"` integrated, `"K"` KRX).
    pub exchgubun: String,
}

/// `t8450` request — serializes to `{"t8450InBlock":{"shcode":...,"exchgubun":...}}`.
/// Not paginated (`facets.self_paginated: false`).
#[derive(Serialize, Debug, Clone)]
pub struct T8450Request {
    #[serde(rename = "t8450InBlock")]
    pub inblock: T8450InBlock,
}

impl T8450Request {
    /// Build a `t8450` current-price/order-book request for one short code + exchange.
    pub fn new(shcode: impl Into<String>, exchgubun: impl Into<String>) -> Self {
        T8450Request {
            inblock: T8450InBlock {
                shcode: shcode.into(),
                exchgubun: exchgubun.into(),
            },
        }
    }
}

/// `t8450OutBlock` — the integrated current-price + order-book snapshot (a
/// representative, spec-grounded subset of the LS `t8450OutBlock`): the current-price
/// header, level-1 + level-2 offer/bid price+quantity, the order-book totals, the day's
/// OHLC, and the limit prices. Every numeric-bearing field uses
/// [`ls_core::string_or_number`] (the gateway sends numbers or strings);
/// `#[serde(default)]` lets a sparse out-block deserialize, and unknown fields are
/// ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8450OutBlock {
    /// Korean name / 한글명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Rate of change (%) / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Previous close / 전일종가(기준가).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilclose: String,
    /// Offer (ask) price, level 1 / 매도호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Bid price, level 1 / 매수호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    /// Offer quantity, level 1 / 매도호가수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem1: String,
    /// Bid quantity, level 1 / 매수호가수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem1: String,
    /// Offer (ask) price, level 2 / 매도호가2.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho2: String,
    /// Bid price, level 2 / 매수호가2.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho2: String,
    /// Total offer quantity / 매도호가수량합.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offer: String,
    /// Total bid quantity / 매수호가수량합.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bid: String,
    /// Open / 시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
    /// High / 고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// Low / 저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// Upper limit price / 상한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub uplmtprice: String,
    /// Lower limit price / 하한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dnlmtprice: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
}

/// `t8450` response envelope — the integrated current-price/order-book snapshot under
/// the `t8450OutBlock` key. All `#[serde(default)]` so a terse/empty envelope
/// deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8450Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t8450OutBlock", default)]
    pub outblock: T8450OutBlock,
}

// ---------------------------------------------------------------------------
// t8407 — API용주식멀티현재가조회 (복수종목 현재가, multi-symbol current price).
// market_session read; one repeated `t8407OutBlock1` row per requested symbol
// (tolerated single-or-array via `ls_core::de_vec_or_single`); path
// /stock/market-data. 2-field request — `shcode` is a CONCATENATION of N
// six-digit short codes with no separators; `nrec` is the count of those codes
// and MUST serialize as a JSON NUMBER via `ls_core::string_as_number` (KTD3 — the
// string form returns IGW40011).
// ---------------------------------------------------------------------------

/// Input block for `t8407` — the requested record count (`nrec`) and the
/// concatenated short codes (`shcode`).
///
/// `shcode` is a single String holding N six-digit codes back-to-back with no
/// separators (e.g. `"005930000660001200"` for three symbols). `nrec` is held as
/// a `String` but serializes as a JSON NUMBER via [`ls_core::string_as_number`]
/// (KTD3 — the string form returns `IGW40011`).
#[derive(Serialize, Debug, Clone)]
pub struct T8407InBlock {
    /// Record count / 건수 (numeric request slot, KTD3) — the number of
    /// six-digit codes packed into `shcode`.
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub nrec: String,
    /// Concatenated short codes / 종목코드 — N six-digit codes back-to-back, no
    /// separators (e.g. `"005930000660001200"`).
    pub shcode: String,
}

/// `t8407` request — serializes to
/// `{"t8407InBlock":{"nrec":3,"shcode":"005930000660001200"}}` (`nrec` as a JSON
/// number). Not paginated (`facets.self_paginated: false`).
#[derive(Serialize, Debug, Clone)]
pub struct T8407Request {
    #[serde(rename = "t8407InBlock")]
    pub inblock: T8407InBlock,
}

impl T8407Request {
    /// Build a `t8407` multi-symbol current-price request. `nrec` is passed as a
    /// String but wire-serializes as a JSON number (KTD3); `shcode` is the N
    /// six-digit codes concatenated with no separators.
    pub fn new(nrec: impl Into<String>, shcode: impl Into<String>) -> Self {
        T8407Request {
            inblock: T8407InBlock {
                nrec: nrec.into(),
                shcode: shcode.into(),
            },
        }
    }
}

/// `t8407OutBlock1` — one current-price row per requested symbol (a
/// representative, spec-grounded subset): the short code / name keys, the current
/// price, the prior-day change sign / amount / rate, the cumulative volume, and
/// the day's open/high/low. Every numeric-bearing field via
/// [`ls_core::string_or_number`]; `#[serde(default)]` lets a sparse row
/// deserialize and unknown fields are ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8407OutBlock1 {
    /// Short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Issue name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Prior-day change sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Prior-day change amount / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Prior-day change rate / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Cumulative volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Open / 시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
    /// High / 고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// Low / 저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
}

/// `t8407` response envelope — the per-symbol rows under the `t8407OutBlock1` key
/// (tolerated single-or-array via [`ls_core::de_vec_or_single`]). All
/// `#[serde(default)]` so a terse/empty envelope deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8407Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8407OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T8407OutBlock1>,
}

// ---------------------------------------------------------------------------
// t1471 — 시간대별호가잔량추이 (intraday best-quote-remainder trend). market_session
// domestic-stock 시세 read; path /stock/market-data, group [주식] 시세. 5-field
// all-String request — shcode, gubun (분구분), time, cnt (자료개수 — a String here),
// exchgubun. Response: a single `t1471OutBlock` scalar quote header (time CTS / price
// / change / volume) + a repeated `t1471OutBlock1` ARRAY (one order-book/trend row
// per slot: best bid/offer remainders, totals, close) tolerated single-or-array via
// `ls_core::de_vec_or_single`.
// ---------------------------------------------------------------------------

/// Input block for `t1471` — the intraday best-quote-remainder trend filters. All
/// ordinary request Strings: `shcode` (종목코드), `gubun` (분구분), `time` (시간 — `""`
/// for the latest), `cnt` (자료개수 — a String, e.g. `"20"`), `exchgubun` (`"1"` =
/// KRX). See [`T1471Request::new`].
#[derive(Serialize, Debug, Clone)]
pub struct T1471InBlock {
    /// Issue code / 종목코드.
    pub shcode: String,
    /// Minute division / 분구분.
    pub gubun: String,
    /// Time / 시간 (`""` for the latest).
    pub time: String,
    /// Record count / 자료개수 (a String — e.g. `"20"`).
    pub cnt: String,
    /// Exchange division / 거래소구분코드 (`"1"` = KRX).
    pub exchgubun: String,
}

/// `t1471` request — serializes to `{"t1471InBlock":{...}}`. Not paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T1471Request {
    #[serde(rename = "t1471InBlock")]
    pub inblock: T1471InBlock,
}

impl T1471Request {
    /// Build a `t1471` intraday best-quote-remainder trend request from the filters.
    pub fn new(
        shcode: impl Into<String>,
        gubun: impl Into<String>,
        time: impl Into<String>,
        cnt: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1471Request {
            inblock: T1471InBlock {
                shcode: shcode.into(),
                gubun: gubun.into(),
                time: time.into(),
                cnt: cnt.into(),
                exchgubun: exchgubun.into(),
            },
        }
    }
}

/// `t1471OutBlock` — the scalar quote header: the CTS `time`, current `price`, sign,
/// `change`, and `volume`. Every numeric field via [`ls_core::string_or_number`];
/// `#[serde(default)]` lets a terse header deserialize.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1471OutBlock {
    /// Time CTS / 시간CTS.
    pub time: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Vs-prior-day sign / 전일대비구분.
    pub sign: String,
    /// Vs-prior-day change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Cumulative volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t1471OutBlock1` — one order-book/trend row (a representative, spec-grounded
/// subset): the execution `time`, best offer/bid prices + remainders, the buy/sell
/// totals, and the `close`. Every numeric field via [`ls_core::string_or_number`];
/// `#[serde(default)]` lets a sparse row deserialize.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1471OutBlock1 {
    /// Execution time / 체결시간.
    pub time: String,
    /// Best-offer remainder / 매도우선잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem1: String,
    /// Best-offer price / 매도우선호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Best-bid price / 매수우선호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    /// Best-bid remainder / 매수우선잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem1: String,
    /// Total offer / 총매도.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub totofferrem: String,
    /// Total bid / 총매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub totbidrem: String,
    /// Close price / 종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
}

/// `t1471` response envelope — the single `t1471OutBlock` scalar header + the repeated
/// `t1471OutBlock1` ARRAY tolerated single-or-array via [`ls_core::de_vec_or_single`].
/// All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1471Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1471OutBlock", default)]
    pub outblock: T1471OutBlock,
    #[serde(
        rename = "t1471OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1471OutBlock1>,
}

// ---------------------------------------------------------------------------
// t1105 — 주식피봇/디마크조회 (pivot / demark levels). market_session, single
// OutBlock; path /stock/market-data. shcode + exchgubun request.
// ---------------------------------------------------------------------------

/// Input block for `t1105` — short code + exchange distinction.
#[derive(Serialize, Debug, Clone)]
pub struct T1105InBlock {
    pub shcode: String,
    pub exchgubun: String,
}

/// `t1105` request — `{"t1105InBlock":{"shcode":...,"exchgubun":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T1105Request {
    #[serde(rename = "t1105InBlock")]
    pub inblock: T1105InBlock,
}

impl T1105Request {
    /// Build a `t1105` pivot/demark request for one symbol on one exchange.
    pub fn new(shcode: impl Into<String>, exchgubun: impl Into<String>) -> Self {
        T1105Request {
            inblock: T1105InBlock {
                shcode: shcode.into(),
                exchgubun: exchgubun.into(),
            },
        }
    }
}

/// `t1105OutBlock` — pivot + demark levels (single object). Numeric-bearing fields
/// via [`ls_core::string_or_number`]; `#[serde(default)]` tolerates a sparse block.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1105OutBlock {
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Pivot / 피봇.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pbot: String,
    /// Pivot 1st resistance / 1차 매도.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offer1: String,
    /// Pivot 1st support / 1차 매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub supp1: String,
    /// Pivot 2nd resistance / 2차 매도.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offer2: String,
    /// Pivot 2nd support / 2차 매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub supp2: String,
    /// Demark standard price / 디마크 기준가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub stdprc: String,
    /// Demark resistance / 디마크 매도.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerd: String,
    /// Demark support / 디마크 매수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub suppd: String,
}

/// `t1105` response envelope — the pivot/demark block under `t1105OutBlock`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1105Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1105OutBlock", default)]
    pub outblock: T1105OutBlock,
}

// ---------------------------------------------------------------------------
// t1104 — 주식현재가시세메모 (current-price memo). market_session; a summary
// OutBlock plus a memo-row array OutBlock1; path /stock/market-data.
// ---------------------------------------------------------------------------

/// Input block for `t1104` — short code (`code`), row count (`nrec`), exchange.
#[derive(Serialize, Debug, Clone)]
pub struct T1104InBlock {
    pub code: String,
    pub nrec: String,
    pub exchgubun: String,
}

/// `t1104` request — `{"t1104InBlock":{"code":...,"nrec":...,"exchgubun":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T1104Request {
    #[serde(rename = "t1104InBlock")]
    pub inblock: T1104InBlock,
}

impl T1104Request {
    /// Build a `t1104` price-memo request for one symbol on one exchange.
    pub fn new(
        code: impl Into<String>,
        nrec: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1104Request {
            inblock: T1104InBlock {
                code: code.into(),
                nrec: nrec.into(),
                exchgubun: exchgubun.into(),
            },
        }
    }
}

/// `t1104OutBlock` — the summary block (record count).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1104OutBlock {
    /// Record count / 레코드 수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub nrec: String,
}

/// `t1104OutBlock1` — one memo row (index / kind / value).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1104OutBlock1 {
    /// Index / 인덱스.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub indx: String,
    /// Kind / 구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gubn: String,
    /// Value / 값.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub vals: String,
}

/// `t1104` response envelope — summary `t1104OutBlock` + memo-row array
/// `t1104OutBlock1` (tolerated single-or-array via [`ls_core::de_vec_or_single`]).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1104Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1104OutBlock", default)]
    pub outblock: T1104OutBlock,
    #[serde(
        rename = "t1104OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1104OutBlock1>,
}

/// Input block for `t1101` — the symbol to look up.
///
/// `shcode` is the 6-digit short code (단축코드). Unlike `t1102`, the `t1101`
/// request carries no `exchgubun`: the spec's `t1101InBlock` is `shcode`-only.
#[derive(Serialize, Debug, Clone)]
pub struct T1101InBlock {
    /// Short code / 단축코드 (e.g. `"078020"`).
    pub shcode: String,
}

/// `t1101` request — wraps the input block under the `t1101InBlock` key.
///
/// Serializes to `{"t1101InBlock":{"shcode":...}}`. `t1101` is a single snapshot
/// (current price + order book), not paginated, so there are no
/// `tr_cont`/`tr_cont_key` fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T1101Request {
    #[serde(rename = "t1101InBlock")]
    pub inblock: T1101InBlock,
}

impl T1101Request {
    /// Build a `t1101` request for one symbol.
    pub fn new(shcode: impl Into<String>) -> Self {
        T1101Request {
            inblock: T1101InBlock {
                shcode: shcode.into(),
            },
        }
    }
}

/// `t1101OutBlock` — current-price header plus the 10-level order book.
///
/// A representative, spec-grounded subset of the LS `t1101OutBlock`: the
/// current-price header fields plus all ten offer/bid price+quantity levels.
/// Every numeric-bearing field uses [`ls_core::string_or_number`] because the
/// gateway sends them as either JSON numbers or JSON strings; `#[serde(default)]`
/// on the struct lets a sparse/empty out-block deserialize cleanly. Field names
/// mirror the LS spec (`t1101OutBlock`) verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1101OutBlock {
    /// Korean name / 한글명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Rate of change (%) / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Previous close / 전일종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilclose: String,
    /// Offer (ask) prices, levels 1–10 / 매도호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho2: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho3: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho4: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho5: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho6: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho7: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho8: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho9: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho10: String,
    /// Bid prices, levels 1–10 / 매수호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho2: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho3: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho4: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho5: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho6: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho7: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho8: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho9: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho10: String,
    /// Offer (ask) quantities, levels 1–10 / 매도호가수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem1: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem2: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem3: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem4: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem5: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem6: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem7: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem8: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem9: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem10: String,
    /// Bid quantities, levels 1–10 / 매수호가수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem1: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem2: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem3: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem4: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem5: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem6: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem7: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem8: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem9: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem10: String,
    /// Total offer quantity / 총매도호가수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offer: String,
    /// Total bid quantity / 총매수호가수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bid: String,
}

/// `t1101` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields (classified in `ls-core`
/// dispatch before this struct is built); `outblock` is the snapshot under the
/// `t1101OutBlock` key. All three are `#[serde(default)]` so a terse or partial
/// envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1101Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1101OutBlock", default)]
    pub outblock: T1101OutBlock,
}

/// Input block for `t1537` — 테마종목별시세조회 (quotes for a theme's stocks).
///
/// Keyed by `tmcode` (테마코드) alone.
#[derive(Serialize, Debug, Clone)]
pub struct T1537InBlock {
    /// Theme code / 테마코드 (4-digit).
    pub tmcode: String,
}

/// `t1537` request — wraps the input block under the `t1537InBlock` key.
#[derive(Serialize, Debug, Clone)]
pub struct T1537Request {
    #[serde(rename = "t1537InBlock")]
    pub inblock: T1537InBlock,
}

impl T1537Request {
    /// Build a `t1537` request for one theme code.
    pub fn new(tmcode: impl Into<String>) -> Self {
        T1537Request {
            inblock: T1537InBlock {
                tmcode: tmcode.into(),
            },
        }
    }
}

/// `t1537OutBlock` — the theme summary block (single object).
///
/// Carries the theme-level aggregates; the per-stock rows are in
/// [`T1537OutBlock1`]. Every field uses [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1537OutBlock {
    /// Advancing-issue count / 상승종목수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upcnt: String,
    /// Theme issue count / 테마종목수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmcnt: String,
    /// Advancing-issue ratio / 상승종목비율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub uprate: String,
    /// Theme name / 테마명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmname: String,
}

/// `t1537OutBlock1` — one per-stock quote row within the theme.
///
/// The repeated row block (`t1537OutBlock1[]`); a representative subset of the
/// spec fields, every one via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1537OutBlock1 {
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Rate of change / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t1537` response envelope.
///
/// `outblock` is the theme summary; `outblock1` is the per-stock quote array
/// under the `t1537OutBlock1` key, tolerated as single-or-array via
/// [`ls_core::de_vec_or_single`]. All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1537Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1537OutBlock", default)]
    pub outblock: T1537OutBlock,
    #[serde(
        rename = "t1537OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1537OutBlock1>,
}

// ---------------------------------------------------------------------------
// t2301 — 옵션전광판 (option board). market_session, non-paginated. Keyed by a
// contract month `yyyymm` (월물) + a `gubun` mini/regular selector
// (미니구분 — `"M"` mini / `"G"` regular). The out-block is a single struct: the
// board header carries the near-month futures snapshot (`gm*` fields) plus the
// representative call-option leg; the deeper per-strike legs are nested object
// arrays the recipe models as a representative subset, not in full.
// ---------------------------------------------------------------------------

/// Input block for `t2301` — the contract month + mini/regular selector.
///
/// `yyyymm` (월물) is the contract month, `YYYYMM` (e.g. `"202609"`); the spec
/// types it `String` (length 6). `gubun` (미니구분) selects mini vs regular:
/// `"M"` 미니 / `"G"` 정규. Both are caller-supplied.
#[derive(Serialize, Debug, Clone)]
pub struct T2301InBlock {
    /// Contract month / 월물 (`YYYYMM`, e.g. `"202609"`).
    pub yyyymm: String,
    /// Mini/regular selector / 미니구분 (`"M"` mini / `"G"` regular).
    pub gubun: String,
}

/// `t2301` request — wraps the input block under the `t2301InBlock` key.
///
/// Serializes to `{"t2301InBlock":{"yyyymm":...,"gubun":...}}`. `t2301` is not
/// paginated, so there are no `tr_cont`/`tr_cont_key` fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T2301Request {
    #[serde(rename = "t2301InBlock")]
    pub inblock: T2301InBlock,
}

impl T2301Request {
    /// Build a `t2301` option-board request for one contract month + selector.
    pub fn new(yyyymm: impl Into<String>, gubun: impl Into<String>) -> Self {
        T2301Request {
            inblock: T2301InBlock {
                yyyymm: yyyymm.into(),
                gubun: gubun.into(),
            },
        }
    }
}

/// `t2301OutBlock` — the option-board header (single object).
///
/// A representative, spec-grounded subset of the 76-field `t2301OutBlock`: the
/// near-month futures snapshot (`gm*` — the board's headline current value) and
/// the representative call-option leg. `gmprice` (근월물현재가, near-month
/// futures current price) is the canonical current-value field, resolved by its
/// `korean_name` from the baseline. Every numeric-bearing field uses
/// [`ls_core::string_or_number`] for wire-type tolerance; `#[serde(default)]`
/// lets a sparse/empty out-block deserialize cleanly. Field names mirror the LS
/// spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2301OutBlock {
    /// Historical volatility / 역사적변동성.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub histimpv: String,
    /// Option days-to-expiry / 옵션잔존일.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jandatecnt: String,
    /// Near-month futures current price / 근월물현재가 (the canonical current value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmprice: String,
    /// Near-month sign vs. previous close / 근월물전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmsign: String,
    /// Near-month change vs. previous close / 근월물전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmchange: String,
    /// Near-month rate of change / 근월물등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmdiff: String,
    /// Near-month volume / 근월물거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmvolume: String,
    /// Near-month futures code / 근월물선물코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmshcode: String,
    /// Call-option representative IV / 콜옵션대표IV.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cimpv: String,
    /// Put-option representative IV / 풋옵션대표IV.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pimpv: String,
}

/// `t2301` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the board
/// header under the `t2301OutBlock` key. All `#[serde(default)]` so a terse or
/// empty envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T2301Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t2301OutBlock", default)]
    pub outblock: T2301OutBlock,
}
