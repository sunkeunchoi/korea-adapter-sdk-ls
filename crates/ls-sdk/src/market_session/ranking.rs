//! Sector-index, VP, theme-screen and condition / ThinQ search reads.
//!
//! Wave-1 split out of `market_session/mod.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use ranking::*;` so every `ls_sdk::market_session::*` path is unchanged.
use super::*;


// ---------------------------------------------------------------------------
// t1638 — 종목별잔량/사전공시 (per-stock remaining-quantity / pre-disclosure ranking).
// market_session read; a repeated `t1638OutBlock` ranking array (tolerated
// single-or-array via `ls_core::de_vec_or_single`); path /stock/etc.
// 4-field request (gubun1/shcode/gubun2/exchgubun); shcode may be empty (full list).
// ---------------------------------------------------------------------------

/// Input block for `t1638` — division (`gubun1`), short code (`shcode`, may be
/// empty for the full list), sort (`gubun2`), exchange distinction (`exchgubun`).
#[derive(Serialize, Debug, Clone)]
pub struct T1638InBlock {
    /// Division / 구분 (e.g. `"1"`).
    pub gubun1: String,
    /// Short code / 종목코드 — empty string returns the full list.
    pub shcode: String,
    /// Sort / 정렬 (e.g. `"1"`).
    pub gubun2: String,
    /// Exchange distinction / 거래소구분코드 (e.g. `""` integrated).
    pub exchgubun: String,
}

/// `t1638` request — serializes to
/// `{"t1638InBlock":{"gubun1":...,"shcode":...,"gubun2":...,"exchgubun":...}}`.
/// Not paginated (`facets.self_paginated: false`).
#[derive(Serialize, Debug, Clone)]
pub struct T1638Request {
    #[serde(rename = "t1638InBlock")]
    pub inblock: T1638InBlock,
}

impl T1638Request {
    /// Build a `t1638` per-stock remaining-quantity / pre-disclosure ranking
    /// request. `shcode` may be empty (`""`) to return the full list.
    pub fn new(
        gubun1: impl Into<String>,
        shcode: impl Into<String>,
        gubun2: impl Into<String>,
        exchgubun: impl Into<String>,
    ) -> Self {
        T1638Request {
            inblock: T1638InBlock {
                gubun1: gubun1.into(),
                shcode: shcode.into(),
                gubun2: gubun2.into(),
                exchgubun: exchgubun.into(),
            },
        }
    }
}

/// `t1638OutBlock` — one ranking row (a representative, spec-grounded subset of the
/// LS `t1638OutBlock`): rank, Korean name, current price, change, remaining buy/sell
/// quantity, the pre-disclosure quantities, and the short code. Every numeric-bearing
/// field uses [`ls_core::string_or_number`] (the gateway sends numbers or strings);
/// `#[serde(default)]` lets a sparse row deserialize, and unknown fields are ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1638OutBlock {
    /// Rank / 순위.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rank: String,
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
    /// Net buy remaining quantity / 순매수잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub obuyvol: String,
    /// Buy remaining quantity / 매수잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub buyrem: String,
    /// Buy pre-disclosure quantity / 매수공시수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub psgvolume: String,
    /// Sell remaining quantity / 매도잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sellrem: String,
    /// Sell pre-disclosure quantity / 매도공시수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pdgvolume: String,
    /// Short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
}

/// `t1638` response envelope — the ranking rows under the `t1638OutBlock` key,
/// tolerated single-or-array via [`ls_core::de_vec_or_single`]. All
/// `#[serde(default)]` so a terse/empty envelope deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1638Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t1638OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T1638OutBlock>,
}

// ---------------------------------------------------------------------------
// t1475 — VP대비등락률상하위 (VP-relative rise/fall ranking). market_session
// domestic-stock 시세 read; path /stock/market-data, group [주식] 시세. 7-field
// request with NUMERIC slots — shcode (String), vptype (String), datacnt/date/time/
// rankcnt (NUMBERS — `#[serde(serialize_with = "ls_core::string_as_number")]` or the
// gateway returns IGW40011), gubun (String). Response: a single `t1475OutBlock` echo
// header (date/time/rankcnt) + a repeated `t1475OutBlock1` ARRAY (one ranked row per
// issue: price/change/volume + the VP moving averages) tolerated single-or-array via
// `ls_core::de_vec_or_single`.
// ---------------------------------------------------------------------------

/// Input block for `t1475` — the VP-relative ranking filters. `shcode` (종목코드),
/// `vptype` (상승하락), and `gubun` (조회구분) are ordinary request Strings; the
/// `datacnt` (데이터개수), `date` (기준일자), `time` (기준시간), and `rankcnt` (랭크카운터)
/// slots are spec **Numbers** and serialize as JSON numbers via
/// [`ls_core::string_as_number`] (else the gateway returns `IGW40011`). See
/// [`T1475Request::new`].
#[derive(Serialize, Debug, Clone)]
pub struct T1475InBlock {
    /// Issue code / 종목코드.
    pub shcode: String,
    /// Rise/fall type / 상승하락.
    pub vptype: String,
    /// Data count / 데이터개수 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub datacnt: String,
    /// Base date / 기준일자 (numeric request slot; `0` for the latest).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub date: String,
    /// Base time / 기준시간 (numeric request slot; `0` for the latest).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub time: String,
    /// Rank counter / 랭크카운터 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub rankcnt: String,
    /// Query division / 조회구분.
    pub gubun: String,
}

/// `t1475` request — serializes to `{"t1475InBlock":{...}}`. Not paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T1475Request {
    #[serde(rename = "t1475InBlock")]
    pub inblock: T1475InBlock,
}

impl T1475Request {
    /// Build a `t1475` VP-relative ranking request. The numeric slots (`datacnt`,
    /// `date`, `time`, `rankcnt`) are passed as Strings and serialized as JSON
    /// numbers; pass `"0"` for the date/time "latest" sentinels.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        shcode: impl Into<String>,
        vptype: impl Into<String>,
        datacnt: impl Into<String>,
        date: impl Into<String>,
        time: impl Into<String>,
        rankcnt: impl Into<String>,
        gubun: impl Into<String>,
    ) -> Self {
        T1475Request {
            inblock: T1475InBlock {
                shcode: shcode.into(),
                vptype: vptype.into(),
                datacnt: datacnt.into(),
                date: date.into(),
                time: time.into(),
                rankcnt: rankcnt.into(),
                gubun: gubun.into(),
            },
        }
    }
}

/// `t1475OutBlock` — the echo header (base date / time / rank counter). Every field a
/// spec `Number` via [`ls_core::string_or_number`]; `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1475OutBlock {
    /// Base date / 기준일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Base time / 기준시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Rank counter / 랭크카운터.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rankcnt: String,
}

/// `t1475OutBlock1` — one ranked row (a representative, spec-grounded subset): the
/// `datetime`, `price`, sign, `change`, `volume`, and the VP moving averages. Every
/// numeric field via [`ls_core::string_or_number`]; `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1475OutBlock1 {
    /// Date/time / 일자.
    pub datetime: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Vs-prior-day sign / 전일대비구분.
    pub sign: String,
    /// Vs-prior-day change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Today VP / 당일VP.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub todayvp: String,
    /// 5-day VP moving average / 5일MAVP.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ma5vp: String,
}

/// `t1475` response envelope — the single `t1475OutBlock` echo header + the repeated
/// `t1475OutBlock1` ranked ARRAY tolerated single-or-array via
/// [`ls_core::de_vec_or_single`]. All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1475Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1475OutBlock", default)]
    pub outblock: T1475OutBlock,
    #[serde(
        rename = "t1475OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1475OutBlock1>,
}

/// Input block for `t1859` — 서버저장조건 조건검색 (server-saved condition search).
///
/// Keyed by `query_index` (서버저장인덱스), the saved-condition index produced by
/// `t1866` (`t1866OutBlock1.query_index`) — the modeled cross-TR discovery edge.
/// The caller never fabricates it; it is self-sourced from a `t1866` list call.
#[derive(Serialize, Debug, Clone)]
pub struct T1859InBlock {
    /// Server-saved condition index / 서버저장인덱스 (from `t1866`).
    pub query_index: String,
}

/// `t1859` request — wraps the input block under the `t1859InBlock` key.
///
/// Serializes to `{"t1859InBlock":{"query_index":...}}`. `t1859` is not paginated,
/// so there are no `tr_cont`/`tr_cont_key` fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T1859Request {
    #[serde(rename = "t1859InBlock")]
    pub inblock: T1859InBlock,
}

impl T1859Request {
    /// Build a `t1859` condition-search request for one saved-condition
    /// `query_index` (source it from [`crate::paginated::T1866Response`]).
    pub fn new(query_index: impl Into<String>) -> Self {
        T1859Request {
            inblock: T1859InBlock {
                query_index: query_index.into(),
            },
        }
    }
}

/// `t1859OutBlock` — the condition-search summary block (single object).
///
/// `result_count` (검색종목수) is the modeled non-key signal proving a populated
/// response. Every field uses [`ls_core::string_or_number`] for wire-type
/// tolerance; `#[serde(default)]` lets a sparse/empty out-block deserialize.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1859OutBlock {
    /// Matched-issue count / 검색종목수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub result_count: String,
    /// Capture time / 포착시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub result_time: String,
    /// Strategy description / 전략설명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub text: String,
}

/// `t1859OutBlock1` — one matched-issue row.
///
/// The repeated row block (`t1859OutBlock1[]`); a representative subset of the
/// spec fields, every one via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1859OutBlock1 {
    /// Short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Korean name / 종목명.
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
    /// Rate of change / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t1859` response envelope.
///
/// `outblock` is the search summary; `outblock1` is the matched-issue array under
/// the `t1859OutBlock1` key, tolerated as single-or-array via
/// [`ls_core::de_vec_or_single`]. All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1859Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1859OutBlock", default)]
    pub outblock: T1859OutBlock,
    #[serde(
        rename = "t1859OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1859OutBlock1>,
}

/// Input block for `t1826` — 종목Q클릭검색리스트조회 (ThinQ Q-click search-list
/// inquiry; the Wave 3 producer).
///
/// `search_gb` selects which search catalog to list (검색구분):
/// `"0"` 핵심검색 / `"1"` 지표검색 / `"2"` 시세동향 / `"3"` 투자자동향. It is a
/// documented filter enum, not an instrument identifier. The response carries the
/// `search_cd` catalog keys that `t1825` consumes (the modeled discovery edge).
#[derive(Serialize, Debug, Clone)]
pub struct T1826InBlock {
    /// Search catalog / 검색구분 (`"0"`–`"3"`).
    pub search_gb: String,
}

/// `t1826` request — wraps the input block under the `t1826InBlock` key.
///
/// Serializes to `{"t1826InBlock":{"search_gb":...}}`. `t1826` is not paginated,
/// so there are no `tr_cont`/`tr_cont_key` fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T1826Request {
    #[serde(rename = "t1826InBlock")]
    pub inblock: T1826InBlock,
}

impl T1826Request {
    /// Build a `t1826` search-list request for one search catalog (`search_gb`,
    /// `"0"` 핵심검색 being the representative core-search catalog).
    pub fn new(search_gb: impl Into<String>) -> Self {
        T1826Request {
            inblock: T1826InBlock {
                search_gb: search_gb.into(),
            },
        }
    }
}

/// `t1826OutBlock` — one available-search row (`t1826OutBlock[]`).
///
/// `search_cd` (검색코드) is the catalog key fed to `t1825`; `search_nm` (검색명)
/// is its display name. Both via [`ls_core::string_or_number`] for wire-type
/// tolerance; `#[serde(default)]` lets a sparse/empty row deserialize.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1826OutBlock {
    /// Search code / 검색코드 (the `t1825` `search_cd` input).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub search_cd: String,
    /// Search name / 검색명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub search_nm: String,
}

/// `t1826` response envelope.
///
/// `outblock` is the available-search array under the `t1826OutBlock` key,
/// tolerated as single-or-array via [`ls_core::de_vec_or_single`]. All
/// `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1826Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t1826OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T1826OutBlock>,
}

/// Input block for `t1825` — 종목Q클릭검색 (ThinQ Q-click search; the Wave 3
/// consumer).
///
/// `search_cd` (검색코드) is the catalog key produced by `t1826`
/// (`t1826OutBlock.search_cd`) — the modeled cross-TR discovery edge; the caller
/// never fabricates it, it is self-sourced from a `t1826` list call. `gubun`
/// (구분) is a market filter: `"0"` 전체 / `"1"` 코스피 / `"2"` 코스닥.
#[derive(Serialize, Debug, Clone)]
pub struct T1825InBlock {
    /// Search code / 검색코드 (from `t1826`).
    pub search_cd: String,
    /// Market filter / 구분 (`"0"` all / `"1"` KOSPI / `"2"` KOSDAQ).
    pub gubun: String,
}

/// `t1825` request — wraps the input block under the `t1825InBlock` key.
///
/// Serializes to `{"t1825InBlock":{"search_cd":...,"gubun":...}}`. `t1825` is not
/// paginated, so there are no `tr_cont`/`tr_cont_key` fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T1825Request {
    #[serde(rename = "t1825InBlock")]
    pub inblock: T1825InBlock,
}

impl T1825Request {
    /// Build a `t1825` Q-click search request keyed by one `search_cd` (source it
    /// from [`T1826Response`]) and a `gubun` market filter (`"0"` 전체).
    pub fn new(search_cd: impl Into<String>, gubun: impl Into<String>) -> Self {
        T1825Request {
            inblock: T1825InBlock {
                search_cd: search_cd.into(),
                gubun: gubun.into(),
            },
        }
    }
}

/// `t1825OutBlock` — the Q-click search summary block (single object).
///
/// `jong_cnt` (검색종목수) is the modeled non-key signal proving a populated
/// response. Via [`ls_core::string_or_number`] for wire-type tolerance;
/// `#[serde(default)]` lets a sparse/empty out-block deserialize.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1825OutBlock {
    /// Matched-issue count / 검색종목수.
    #[serde(rename = "JongCnt", deserialize_with = "ls_core::string_or_number")]
    pub jong_cnt: String,
}

/// `t1825OutBlock1` — one matched-issue row (`t1825OutBlock1[]`).
///
/// A representative subset of the spec fields, every one via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1825OutBlock1 {
    /// Short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Rate of change / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t1825` response envelope.
///
/// `outblock` is the search summary; `outblock1` is the matched-issue array under
/// the `t1825OutBlock1` key, tolerated as single-or-array via
/// [`ls_core::de_vec_or_single`]. All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1825Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1825OutBlock", default)]
    pub outblock: T1825OutBlock,
    #[serde(
        rename = "t1825OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1825OutBlock1>,
}

// ---------------------------------------------------------------------------
// [업종] 시세 — sector/index cluster (Wave A). All on `/indtp/market-data`,
// instrument_domain `sector_index`. `upcode` (업종코드, e.g. "001"=코스피종합) is a
// fixed-width sector code → stays string-serialized; never `string_as_number`.
// ---------------------------------------------------------------------------

/// Input block for `t8424` — 전체업종 (all-sectors list). `gubun1` is an optional
/// filter; the all-sectors read sends it empty.
#[derive(Serialize, Debug, Clone)]
pub struct T8424InBlock {
    /// Filter / 구분 (empty = all sectors).
    pub gubun1: String,
}

/// `t8424` request — serializes to `{"t8424InBlock":{"gubun1":""}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T8424Request {
    #[serde(rename = "t8424InBlock")]
    pub inblock: T8424InBlock,
}
impl T8424Request {
    /// Build a `t8424` all-sectors request (no meaningful caller input).
    pub fn new() -> Self {
        T8424Request {
            inblock: T8424InBlock {
                gubun1: String::new(),
            },
        }
    }
}
impl Default for T8424Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t8424OutBlock` — one sector row: the `upcode` (업종코드) fed to the four
/// consumers (`t1511`/`t1514`/`t1516`/`t1485`) and its Korean name.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8424OutBlock {
    /// Sector name / 업종명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Sector code / 업종코드 (the `upcode` consumer key; string, never numeric).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upcode: String,
}

/// `t8424` response — the sector array under `t8424OutBlock`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8424Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8424OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8424OutBlock>,
}

/// Input block for `t1511` — 업종현재가 (index snapshot for one sector).
#[derive(Serialize, Debug, Clone)]
pub struct T1511InBlock {
    /// Sector code / 업종코드 (e.g. "001"; from `t8424` or a literal sector code).
    pub upcode: String,
}

/// `t1511` request — serializes to `{"t1511InBlock":{"upcode":"001"}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T1511Request {
    #[serde(rename = "t1511InBlock")]
    pub inblock: T1511InBlock,
}
impl T1511Request {
    /// Build a `t1511` index-snapshot request for one sector code.
    pub fn new(upcode: impl Into<String>) -> Self {
        T1511Request {
            inblock: T1511InBlock {
                upcode: upcode.into(),
            },
        }
    }
}

/// `t1511OutBlock` — the index snapshot. A representative, spec-grounded subset
/// of the ~65-field `t1511OutBlock`; every numeric-bearing field via
/// [`ls_core::string_or_number`] (the gateway mixes string and number forms).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1511OutBlock {
    /// Sector name / 업종명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current index / 현재지수 — the canonical composite index value.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pricejisu: String,
    /// First comparison sub-index / 첫번째지수 (distinct from `pricejisu`; for
    /// KOSPI composite the two coincide, but they diverge for other sectors).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub firstjisu: String,
    /// Previous-day index / 전일지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jniljisu: String,
    /// Open index / 시가지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub openjisu: String,
    /// High index / 고가지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub highjisu: String,
    /// Change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Value / 거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
}

/// `t1511` response — single snapshot under `t1511OutBlock`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1511Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1511OutBlock", default)]
    pub outblock: T1511OutBlock,
}

/// Input block for `t1485` — 예상지수 (expected/auction index for one sector).
#[derive(Serialize, Debug, Clone)]
pub struct T1485InBlock {
    /// Sector code / 업종코드.
    pub upcode: String,
    /// Mode / 구분.
    pub gubun: String,
}

/// `t1485` request — serializes to `{"t1485InBlock":{"upcode":"001","gubun":"1"}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T1485Request {
    #[serde(rename = "t1485InBlock")]
    pub inblock: T1485InBlock,
}
impl T1485Request {
    /// Build a `t1485` expected-index request for one sector and mode.
    pub fn new(upcode: impl Into<String>, gubun: impl Into<String>) -> Self {
        T1485Request {
            inblock: T1485InBlock {
                upcode: upcode.into(),
                gubun: gubun.into(),
            },
        }
    }
}

/// `t1485OutBlock` — expected-index summary. Numerics via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1485OutBlock {
    /// Expected index / 예상지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pricejisu: String,
    /// Change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t1485OutBlock1` — one expected-index time row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1485OutBlock1 {
    /// Index / 지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisu: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Time / 체결시간 (may be a label like "장 전").
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chetime: String,
}

/// `t1485` response — summary `t1485OutBlock` + the time array `t1485OutBlock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1485Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1485OutBlock", default)]
    pub outblock: T1485OutBlock,
    #[serde(
        rename = "t1485OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1485OutBlock1>,
}

/// Input block for `t1516` — 업종별종목시세 (per-sector stock board). Carries two
/// caller-supplied identifiers: the sector `upcode` and a `shcode` ticker.
#[derive(Serialize, Debug, Clone)]
pub struct T1516InBlock {
    /// Sector code / 업종코드.
    pub upcode: String,
    /// Mode / 구분.
    pub gubun: String,
    /// Stock short code / 종목코드 (a 6-char ticker; empty returns the full board).
    pub shcode: String,
}

/// `t1516` request — `{"t1516InBlock":{"upcode":"001","gubun":"1","shcode":"005930"}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T1516Request {
    #[serde(rename = "t1516InBlock")]
    pub inblock: T1516InBlock,
}
impl T1516Request {
    /// Build a `t1516` per-sector stock-board request.
    pub fn new(
        upcode: impl Into<String>,
        gubun: impl Into<String>,
        shcode: impl Into<String>,
    ) -> Self {
        T1516Request {
            inblock: T1516InBlock {
                upcode: upcode.into(),
                gubun: gubun.into(),
                shcode: shcode.into(),
            },
        }
    }
}

/// `t1516OutBlock` — sector-board summary header.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1516OutBlock {
    /// Echoed stock short code / 종목코드 (confirms which board was returned).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Sector index / 지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pricejisu: String,
    /// Change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Index change vs previous / 지수대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jdiff: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
}

/// `t1516OutBlock1` — one stock row within the sector board.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1516OutBlock1 {
    /// Stock short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Stock name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Value / 거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
}

/// `t1516` response — summary `t1516OutBlock` + per-stock array `t1516OutBlock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1516Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1516OutBlock", default)]
    pub outblock: T1516OutBlock,
    #[serde(
        rename = "t1516OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1516OutBlock1>,
}

/// Input block for `t3521` — 해외지수조회 (one overseas index's current snapshot,
/// e.g. Dow/NASDAQ). `kind`/`symbol` select the index (e.g. `kind="S"`,
/// `symbol="DJI@DJI"`). Non-paginated; no numeric request fields.
#[derive(Serialize, Debug, Clone)]
pub struct T3521InBlock {
    /// Symbol kind / 종목종류 (e.g. "S").
    pub kind: String,
    /// Index symbol / SYMBOL (e.g. "DJI@DJI").
    pub symbol: String,
}

/// `t3521` request — serializes to `{"t3521InBlock":{"kind":"...","symbol":"..."}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T3521Request {
    #[serde(rename = "t3521InBlock")]
    pub inblock: T3521InBlock,
}
impl T3521Request {
    /// Build a `t3521` overseas-index snapshot request (`kind`/`symbol`).
    pub fn new(kind: impl Into<String>, symbol: impl Into<String>) -> Self {
        T3521Request {
            inblock: T3521InBlock {
                kind: kind.into(),
                symbol: symbol.into(),
            },
        }
    }
}

/// `t3521OutBlock` — one overseas-index snapshot row. The raw capture documents no
/// `res_b` properties for this TR, so the field set is modeled from the gateway's
/// own `res_example` (date/symbol/change/sign/diff/close/hname).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T3521OutBlock {
    /// Trade date / 일자 (YYYYMMDD).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Index symbol / SYMBOL.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Change / 대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Change sign / 대비속성.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change rate / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Close / 현재지수 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    /// Index name / 지수명 (e.g. 다우 산업).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
}

/// `t3521` response — single snapshot under `t3521OutBlock`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T3521Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t3521OutBlock", default)]
    pub outblock: T3521OutBlock,
}
