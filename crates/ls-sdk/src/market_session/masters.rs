//! Stock / sector / theme / derivatives / overseas instrument master & list reads.
//!
//! Wave-1 split out of `market_session/mod.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use masters::*;` so every `ls_sdk::market_session::*` path is unchanged.
use super::*;


/// Input block for `t8425` — 전체테마 (all themes).
///
/// `t8425` is a no-caller-input read: the spec's `t8425InBlock` carries a single
/// length-1 `dummy` placeholder (단축코드-style filler), so callers supply
/// nothing. Modeled after `T1102InBlock` *minus* every caller identifier.
#[derive(Serialize, Debug, Clone)]
pub struct T8425InBlock {
    /// Dummy placeholder / Dummy (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t8425` request — wraps the input block under the `t8425InBlock` key.
///
/// Serializes to `{"t8425InBlock":{"dummy":""}}`. `t8425` is not paginated and
/// takes no caller identifier, so there are no continuation fields and no
/// caller-supplied fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T8425Request {
    #[serde(rename = "t8425InBlock")]
    pub inblock: T8425InBlock,
}

impl T8425Request {
    /// Build a `t8425` all-themes request. Takes no caller input; the `dummy`
    /// placeholder serializes as an empty string.
    pub fn new() -> Self {
        T8425Request {
            inblock: T8425InBlock {
                dummy: String::new(),
            },
        }
    }
}

impl Default for T8425Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t8425OutBlock` — one theme row.
///
/// The `t8425OutBlock` response block is a repeated array of theme rows (the spec
/// marks the block itself `Binary`, the array marker), so [`T8425Response`] holds
/// a `Vec` of these tolerated as single-or-array via [`ls_core::de_vec_or_single`].
/// Both fields use [`ls_core::string_or_number`] for wire-type tolerance and
/// `#[serde(default)]` lets a sparse row deserialize cleanly. Field names mirror
/// the LS spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8425OutBlock {
    /// Theme name / 테마명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmname: String,
    /// Theme code / 테마코드 (the representative caller input for `t1531`/`t1537`).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmcode: String,
}

/// `t8425` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields (classified in `ls-core`
/// dispatch before this struct is built); `outblock` is the all-themes array
/// under the `t8425OutBlock` key, tolerated as a single object OR an array via
/// [`ls_core::de_vec_or_single`]. All three are `#[serde(default)]` so a terse or
/// empty envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8425Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8425OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8425OutBlock>,
}

/// Input block for `t8436` — 주식종목조회 (stock master list).
///
/// `gubun` is a market-segment FILTER (구분: `"0"` 전체 / `"1"` 코스피 /
/// `"2"` 코스닥), not an instrument identifier — the read returns the whole list
/// for the chosen segment.
#[derive(Serialize, Debug, Clone)]
pub struct T8436InBlock {
    /// Market-segment filter / 구분 (`"0"` all / `"1"` KOSPI / `"2"` KOSDAQ).
    pub gubun: String,
}

/// `t8436` request — wraps the input block under the `t8436InBlock` key.
///
/// Serializes to `{"t8436InBlock":{"gubun":"0"}}`. `t8436` is not paginated, so
/// there are no continuation fields in the body; `gubun` is a filter selector.
#[derive(Serialize, Debug, Clone)]
pub struct T8436Request {
    #[serde(rename = "t8436InBlock")]
    pub inblock: T8436InBlock,
}

impl T8436Request {
    /// Build a `t8436` stock-list request for one market segment (`gubun`).
    pub fn new(gubun: impl Into<String>) -> Self {
        T8436Request {
            inblock: T8436InBlock {
                gubun: gubun.into(),
            },
        }
    }
}

/// `t8436OutBlock` — one stock-master row.
///
/// The `t8436OutBlock` response block is a repeated array (the spec marks the
/// block `Binary`), so [`T8436Response`] holds a `Vec` tolerated as single-or-
/// array via [`ls_core::de_vec_or_single`]. A representative, spec-grounded
/// subset; every field uses [`ls_core::string_or_number`] for wire-type
/// tolerance and `#[serde(default)]` lets a sparse row deserialize cleanly.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8436OutBlock {
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Short code / 단축코드 (6-digit).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Extended code / 확장코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
    /// ETF distinction / ETF구분 (`"1"` ETF / `"2"` ETN).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub etfgubun: String,
    /// Upper limit price / 상한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub uplmtprice: String,
    /// Lower limit price / 하한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dnlmtprice: String,
    /// Previous close / 전일가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilclose: String,
    /// Market segment / 구분 (`"1"` KOSPI / `"2"` KOSDAQ).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gubun: String,
}

/// `t8436` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the
/// stock-master array under the `t8436OutBlock` key, tolerated as single-or-array
/// via [`ls_core::de_vec_or_single`]. All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8436Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8436OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8436OutBlock>,
}

/// Input block for `t1531` — 테마별종목 (stocks in a theme).
///
/// The spec marks BOTH `tmname` (테마명) and `tmcode` (테마코드) required, so
/// callers pass a matched name+code pair (e.g. a row from [`MarketSession::all_themes`]).
#[derive(Serialize, Debug, Clone)]
pub struct T1531InBlock {
    /// Theme name / 테마명.
    pub tmname: String,
    /// Theme code / 테마코드 (4-digit).
    pub tmcode: String,
}

/// `t1531` request — wraps the input block under the `t1531InBlock` key.
#[derive(Serialize, Debug, Clone)]
pub struct T1531Request {
    #[serde(rename = "t1531InBlock")]
    pub inblock: T1531InBlock,
}

impl T1531Request {
    /// Build a `t1531` request for one theme (name + code, both required).
    pub fn new(tmname: impl Into<String>, tmcode: impl Into<String>) -> Self {
        T1531Request {
            inblock: T1531InBlock {
                tmname: tmname.into(),
                tmcode: tmcode.into(),
            },
        }
    }
}

/// `t1531OutBlock` — one theme-constituent row.
///
/// The `t1531OutBlock` response block is a repeated array (the spec marks it
/// `Binary`), so [`T1531Response`] holds a `Vec` tolerated as single-or-array via
/// [`ls_core::de_vec_or_single`]. Every field uses [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1531OutBlock {
    /// Theme name / 테마명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmname: String,
    /// Average rate of change / 평균등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub avgdiff: String,
    /// Theme code / 테마코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmcode: String,
}

/// `t1531` response envelope. `outblock` is the theme-row array under the
/// `t1531OutBlock` key, tolerated as single-or-array. All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1531Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t1531OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T1531OutBlock>,
}

// ---------------------------------------------------------------------------
// Wave 1 — ELW universe & instrument surface. No-caller-input `dummy` reads
// (t9905, t9907, t8431, t9942) modeled after `t8425`; each returns a list keyed
// by a code field. All `/stock/elw`, `[주식] ELW`, non-paginated market_session.
// ---------------------------------------------------------------------------

/// Input block for `t9905` — 기초자산리스트조회 (full underlying-asset list). A
/// no-caller-input read: a single length-1 `dummy` placeholder.
#[derive(Serialize, Debug, Clone)]
pub struct T9905InBlock {
    /// Dummy placeholder / DUMMY (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t9905` request — serializes to `{"t9905InBlock":{"dummy":""}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T9905Request {
    #[serde(rename = "t9905InBlock")]
    pub inblock: T9905InBlock,
}
impl T9905Request {
    /// Build a `t9905` underlying-list request (no caller input).
    pub fn new() -> Self {
        T9905Request {
            inblock: T9905InBlock {
                dummy: String::new(),
            },
        }
    }
}
impl Default for T9905Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t9905OutBlock1` — one underlying-asset row. `shcode` (단축코드) is the
/// underlying-asset code consumed by `t1964` (`item`). All via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T9905OutBlock1 {
    /// Short code / 단축코드 (the underlying-asset code; `t1964` `item` input).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Standard code / 표준코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
}

/// `t9905` response — underlying-asset array under `t9905OutBlock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T9905Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t9905OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T9905OutBlock1>,
}

/// Input block for `t8430` — 주식종목조회 (full stock-issue list). `gubun` selects
/// the market: "0" all, "1" KOSPI, "2" KOSDAQ. The full-list read sends "0".
/// `gubun` is a code string ("0"/"1"/"2"), not numeric — no `string_as_number`.
#[derive(Serialize, Debug, Clone)]
pub struct T8430InBlock {
    /// Market filter / 구분 ("0":전체 "1":코스피 "2":코스닥).
    pub gubun: String,
}

/// `t8430` request — serializes to `{"t8430InBlock":{"gubun":"0"}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T8430Request {
    #[serde(rename = "t8430InBlock")]
    pub inblock: T8430InBlock,
}
impl T8430Request {
    /// Build a `t8430` stock-issue-list request for a market filter
    /// ("0":전체 "1":코스피 "2":코스닥).
    pub fn new(gubun: impl Into<String>) -> Self {
        T8430Request {
            inblock: T8430InBlock {
                gubun: gubun.into(),
            },
        }
    }
    /// Build a `t8430` request for every market ("0":전체).
    pub fn all() -> Self {
        Self::new("0")
    }
}
impl Default for T8430Request {
    fn default() -> Self {
        Self::all()
    }
}

/// `t8430OutBlock` — one stock-issue row. Numeric-bearing fields via
/// [`ls_core::string_or_number`] (the gateway mixes string and number forms).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8430OutBlock {
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Extended code / 확장코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
    /// ETF flag / ETF구분 ("1":ETF).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub etfgubun: String,
    /// Upper-limit price / 상한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub uplmtprice: String,
    /// Lower-limit price / 하한가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dnlmtprice: String,
    /// Previous-day close / 전일가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilclose: String,
    /// Order-quantity unit / 주문수량단위.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub memedan: String,
    /// Reference price / 기준가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub recprice: String,
    /// Market flag / 구분 ("1":코스피 "2":코스닥).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gubun: String,
}

/// `t8430` response — the stock-issue array under `t8430OutBlock`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8430Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8430OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8430OutBlock>,
}

// ---------------------------------------------------------------------------
// t2522 — 주식선물기초자산조회 (stock-futures underlying-asset master). market_session,
// non-paginated. A no-caller-input read: the spec's `t2522InBlock` carries a single
// length-1 `dummy` placeholder, so callers supply nothing. The response is a count
// header (`t2522OutBlock`, single) plus the underlying-asset rows
// (`t2522OutBlock1`, an object array) — the data-bearing block where each row's
// 기초자산명 lives.
// ---------------------------------------------------------------------------

/// Input block for `t2522` — a no-caller-input read.
///
/// The spec's `t2522InBlock` carries a single length-1 `dummy` placeholder
/// (Dummy), so callers supply nothing. Modeled after `T8425InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T2522InBlock {
    /// Dummy placeholder / Dummy (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t2522` request — wraps the input block under the `t2522InBlock` key.
///
/// Serializes to `{"t2522InBlock":{"dummy":""}}`. `t2522` is not paginated and
/// takes no caller identifier, so there are no continuation fields and no
/// caller-supplied fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T2522Request {
    #[serde(rename = "t2522InBlock")]
    pub inblock: T2522InBlock,
}

impl T2522Request {
    /// Build a `t2522` stock-futures underlying-asset request. Takes no caller
    /// input; the `dummy` placeholder serializes as an empty string.
    pub fn new() -> Self {
        T2522Request {
            inblock: T2522InBlock {
                dummy: String::new(),
            },
        }
    }
}

impl Default for T2522Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t2522OutBlock` — the count header (single object).
///
/// Carries the row count (`cnt` / 건수); the underlying-asset rows themselves are
/// in [`T2522OutBlock1`]. `cnt` uses [`ls_core::string_or_number`] (the gateway
/// sends it as a JSON number); `#[serde(default)]` lets a sparse/empty header
/// deserialize cleanly.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2522OutBlock {
    /// Row count / 건수 (arrives as a JSON number).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cnt: String,
}

/// `t2522OutBlock1` — one stock-futures underlying-asset row.
///
/// The data-bearing repeated block (`t2522OutBlock1[]`). `bsc_asts_nm`
/// (기초자산명, the underlying-asset name) is the canonical identity field,
/// resolved by its `korean_name` from the baseline; the remaining fields are the
/// underlying codes. Every field uses [`ls_core::string_or_number`] for wire-type
/// tolerance; `#[serde(default)]` lets a sparse row deserialize cleanly. Field
/// names mirror the LS spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2522OutBlock1 {
    /// Underlying-asset name / 기초자산명 (the canonical identity field).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bsc_asts_nm: String,
    /// Underlying-asset issue code / 기초자산종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bsc_asts_is_cd: String,
    /// Underlying-asset ID / 기초자산ID.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bsc_asts_id: String,
    /// Near-month issue code / 최근월물종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub nmc_is_shrt_cd: String,
}

/// `t2522` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the count
/// header under the `t2522OutBlock` key; `outblock1` is the underlying-asset row
/// array under the `t2522OutBlock1` key, tolerated as a single object OR an array
/// via [`ls_core::de_vec_or_single`]. All `#[serde(default)]` so a terse or empty
/// envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T2522Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t2522OutBlock", default)]
    pub outblock: T2522OutBlock,
    #[serde(
        rename = "t2522OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T2522OutBlock1>,
}

// ---------------------------------------------------------------------------
// t8435 — 파생종목마스터조회API용 (derivatives master). market_session,
// non-paginated. Keyed by a `gubun` (구분) selector — the LS spec defines these
// as the MINI/weekly segments: `"MF"` 미니선물 (mini futures) / `"MO"` 미니옵션
// (mini options) / `"WK"` 코스피200위클리옵션 / `"SF"` 코스닥150선물 / `"QW"`
// 코스닥150위클리옵션. The response out-block `t8435OutBlock` is itself a ROW
// ARRAY (the raw capture's `res_example` shows `"t8435OutBlock": [ {…}, … ]`,
// one derivatives contract per row, no numbered sub-block — the normalized
// baseline collapses the block, so the true wire shape is read from the raw
// capture per KTD3) — each row carries the contract name + codes plus the daily
// limit/close reference prices. Modeled after `T8433` (single row-array
// out-block) but with a caller `gubun` selector.
// ---------------------------------------------------------------------------

/// Input block for `t8435` — the derivatives-segment selector.
///
/// `gubun` (구분) selects the derivatives segment. The LS spec defines these as
/// the MINI/weekly segments: `"MF"` 미니선물 (mini futures) / `"MO"` 미니옵션
/// (mini options) / `"WK"` 코스피200위클리옵션 (KOSPI200 weekly options) /
/// `"SF"` 코스닥150선물 (KOSDAQ150 futures) / `"QW"` 코스닥150위클리옵션
/// (KOSDAQ150 weekly options). The spec types it `String` (length 2).
/// Caller-supplied.
#[derive(Serialize, Debug, Clone)]
pub struct T8435InBlock {
    /// Segment selector / 구분 (`"MF"` mini futures / `"MO"` mini options /
    /// `"WK"`/`"SF"`/`"QW"` weekly/KOSDAQ150 segments).
    pub gubun: String,
}

/// `t8435` request — wraps the input block under the `t8435InBlock` key.
///
/// Serializes to `{"t8435InBlock":{"gubun":"MF"}}`. `t8435` is not paginated, so
/// there are no `tr_cont`/`tr_cont_key` fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T8435Request {
    #[serde(rename = "t8435InBlock")]
    pub inblock: T8435InBlock,
}

impl T8435Request {
    /// Build a `t8435` derivatives-master request for one segment (`gubun`:
    /// `"MF"` mini futures / `"MO"` mini options / `"WK"`/`"SF"`/`"QW"` weekly/
    /// KOSDAQ150 segments).
    pub fn new(gubun: impl Into<String>) -> Self {
        T8435Request {
            inblock: T8435InBlock {
                gubun: gubun.into(),
            },
        }
    }
}

/// `t8435OutBlock` — one derivatives-master row.
///
/// The data-bearing repeated block (`t8435OutBlock[]`, confirmed from the raw
/// capture's `res_example` array — rows are direct elements under the
/// `t8435OutBlock` key). The full 9 fields. `hname` (종목명, the derivatives
/// contract name) is the canonical identity field, resolved by its `korean_name`
/// from the baseline; `shcode`/`expcode` are the contract codes, and the
/// `Number`-typed `uplmtprice`/`dnlmtprice`/`jnilclose`/`jnilhigh`/`jnillow`/
/// `recprice` fields are the daily limit/close reference prices. The
/// numeric-bearing fields use [`ls_core::string_or_number`] for wire-type
/// tolerance (the gateway may send a `Number` field as a JSON number);
/// `#[serde(default)]` lets a sparse row deserialize cleanly. Field names mirror
/// the LS spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8435OutBlock {
    /// Contract name / 종목명 (the canonical identity field).
    pub hname: String,
    /// Short code / 단축코드 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Expanded code / 확장코드.
    pub expcode: String,
    /// Upper limit price / 상한가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub uplmtprice: String,
    /// Lower limit price / 하한가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dnlmtprice: String,
    /// Previous-day close / 전일종가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilclose: String,
    /// Previous-day high / 전일고가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilhigh: String,
    /// Previous-day low / 전일저가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnillow: String,
    /// Reference price / 기준가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub recprice: String,
}

/// `t8435` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the
/// derivatives-master row array under the `t8435OutBlock` key, tolerated as a
/// single object OR an array via [`ls_core::de_vec_or_single`]. All
/// `#[serde(default)]` so a terse or empty envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8435Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8435OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8435OutBlock>,
}

// ---------------------------------------------------------------------------
// t8467 — 지수선물마스터조회API용 (index-futures master). market_session,
// non-paginated. Keyed by a `gubun` (구분) segment selector — `"V"` 변동성지수선물
// (volatility-index futures) / `"S"` 섹터지수선물 (sector-index futures) / `"Q"`
// 코스닥150지수선물 (KOSDAQ150-index futures) / any other value → 코스피200지수선물
// (KOSPI200-index futures, the default). The response out-block `t8467OutBlock`
// is itself a ROW ARRAY (the raw capture's `res_example` shows
// `"t8467OutBlock": [ {…}, … ]`, propertyType `A0005`/Object Array, one
// index-futures contract per row — the normalized baseline lists the row fields
// flat under the block name, so the true wire shape is read from the raw capture
// per KTD3). Each row carries the contract name + codes plus the daily
// limit/close reference prices. Modeled identically to `T8435` (single row-array
// out-block, the same 9 fields) but with the index-futures `gubun` selector.
// ---------------------------------------------------------------------------

/// Input block for `t8467` — the index-futures segment selector.
///
/// `gubun` (구분) selects the index-futures segment: `"V"` 변동성지수선물 / `"S"`
/// 섹터지수선물 / `"Q"` 코스닥150지수선물 / any other value → 코스피200지수선물
/// (the default). The spec types it `String` (length 1). Caller-supplied.
#[derive(Serialize, Debug, Clone)]
pub struct T8467InBlock {
    /// Segment selector / 구분 (`"V"`/`"S"`/`"Q"` or default → KOSPI200).
    pub gubun: String,
}

/// `t8467` request — wraps the input block under the `t8467InBlock` key.
///
/// Serializes to `{"t8467InBlock":{"gubun":"Q"}}`. `t8467` is not paginated, so
/// there are no `tr_cont`/`tr_cont_key` fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T8467Request {
    #[serde(rename = "t8467InBlock")]
    pub inblock: T8467InBlock,
}

impl T8467Request {
    /// Build a `t8467` index-futures-master request for one segment (`gubun`:
    /// `"V"`/`"S"`/`"Q"` or any other value → KOSPI200-index futures).
    pub fn new(gubun: impl Into<String>) -> Self {
        T8467Request {
            inblock: T8467InBlock {
                gubun: gubun.into(),
            },
        }
    }
}

/// `t8467OutBlock` — one index-futures-master row.
///
/// The data-bearing repeated block (`t8467OutBlock[]`, confirmed from the raw
/// capture's `res_example` array — rows are direct elements under the
/// `t8467OutBlock` key). The full 9 fields. `hname` (종목명, the index-futures
/// contract name) is the canonical identity field, resolved by its `korean_name`
/// from the baseline; `shcode`/`expcode` are the contract codes, and the
/// `Number`-typed `uplmtprice`/`dnlmtprice`/`jnilclose`/`jnilhigh`/`jnillow`/
/// `recprice` fields are the daily limit/close reference prices. The
/// numeric-bearing fields use [`ls_core::string_or_number`] for wire-type
/// tolerance (the gateway may send a `Number` field as a JSON number);
/// `#[serde(default)]` lets a sparse row deserialize cleanly. Field names mirror
/// the LS spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8467OutBlock {
    /// Contract name / 종목명 (the canonical identity field).
    pub hname: String,
    /// Short code / 단축코드 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Expanded code / 확장코드.
    pub expcode: String,
    /// Upper limit price / 상한가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub uplmtprice: String,
    /// Lower limit price / 하한가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dnlmtprice: String,
    /// Previous-day close / 전일종가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilclose: String,
    /// Previous-day high / 전일고가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilhigh: String,
    /// Previous-day low / 전일저가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnillow: String,
    /// Reference price / 기준가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub recprice: String,
}

/// `t8467` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the
/// index-futures-master row array under the `t8467OutBlock` key, tolerated as a
/// single object OR an array via [`ls_core::de_vec_or_single`]. All
/// `#[serde(default)]` so a terse or empty envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8467Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8467OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8467OutBlock>,
}

// ---------------------------------------------------------------------------
// t9943 — 지수선물마스터조회API용 (index-futures master). market_session,
// non-paginated. Keyed by a `gubun` (구분) segment selector — `"V"` 변동성지수선물
// (volatility-index futures) / `"S"` 섹터지수선물 (sector-index futures) / any
// other value → 코스피200지수선물 (KOSPI200-index futures, the default). The
// response out-block `t9943OutBlock` is itself a ROW ARRAY: the raw capture's
// `res_example` shows `"t9943OutBlock": [ {…}, … ]` (propertyType `A0005`/Object
// Array), each row a direct element carrying the contract name + codes — the
// normalized baseline collapses the block name to `response_body`, so the true
// wire out-block key `t9943OutBlock` is read from the raw capture per KTD3.
// Modeled after `T8467` (same 지수선물마스터 read, the same `gubun` selector) but
// the spec lists only the 3 identity fields (`hname`/`shcode`/`expcode`), no
// daily limit/close reference prices.
// ---------------------------------------------------------------------------

/// Input block for `t9943` — the index-futures segment selector.
///
/// `gubun` (구분) selects the index-futures segment: `"V"` 변동성지수선물 / `"S"`
/// 섹터지수선물 / any other value → 코스피200지수선물 (the default). The spec types
/// it `String` (length 1). Caller-supplied.
#[derive(Serialize, Debug, Clone)]
pub struct T9943InBlock {
    /// Segment selector / 구분 (`"V"`/`"S"` or default → KOSPI200).
    pub gubun: String,
}

/// `t9943` request — wraps the input block under the `t9943InBlock` key.
///
/// Serializes to `{"t9943InBlock":{"gubun":"V"}}`. `t9943` is not paginated, so
/// there are no `tr_cont`/`tr_cont_key` fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T9943Request {
    #[serde(rename = "t9943InBlock")]
    pub inblock: T9943InBlock,
}

impl T9943Request {
    /// Build a `t9943` index-futures-master request for one segment (`gubun`:
    /// `"V"`/`"S"` or any other value → KOSPI200-index futures).
    pub fn new(gubun: impl Into<String>) -> Self {
        T9943Request {
            inblock: T9943InBlock {
                gubun: gubun.into(),
            },
        }
    }
}

/// `t9943OutBlock` — one index-futures-master row.
///
/// The data-bearing repeated block (`t9943OutBlock[]`, confirmed from the raw
/// capture's `res_example` array — rows are direct elements under the
/// `t9943OutBlock` key). The 3 spec fields. `hname` (종목명, the index-futures
/// contract name) is the canonical identity field, resolved by its `korean_name`
/// from the baseline; `shcode` (단축코드) / `expcode` (확장코드) are the contract
/// codes. `shcode` uses [`ls_core::string_or_number`] for wire-type tolerance
/// (the gateway may send a code field as a JSON number); `#[serde(default)]` lets
/// a sparse row deserialize cleanly. Field names mirror the LS spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T9943OutBlock {
    /// Contract name / 종목명 (the canonical identity field).
    pub hname: String,
    /// Short code / 단축코드 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Expanded code / 확장코드.
    pub expcode: String,
}

/// `t9943` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the
/// index-futures-master row array under the `t9943OutBlock` key, tolerated as a
/// single object OR an array via [`ls_core::de_vec_or_single`]. All
/// `#[serde(default)]` so a terse or empty envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T9943Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t9943OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T9943OutBlock>,
}

// ---------------------------------------------------------------------------
// t9944 — 지수옵션마스터조회API용 (index-option master). market_session,
// non-paginated. A no-caller-input read: the spec's `t9944InBlock` carries a
// single length-1 `dummy` placeholder, so callers supply nothing. The response
// out-block `t9944OutBlock` is itself a ROW ARRAY: the raw capture's
// `res_example` shows `"t9944OutBlock": [ {…}, … ]` (propertyType Object Array),
// each row a direct element carrying the contract name + codes — the normalized
// baseline collapses the block name to `response_body`, so the true wire
// out-block key `t9944OutBlock` is read from the raw capture per KTD3. Modeled
// after `T8426`/`T9943` (same dummy-input row-array master read); the spec lists
// only the 3 identity fields (`hname`/`shcode`/`expcode`).
// ---------------------------------------------------------------------------

/// Input block for `t9944` — a no-caller-input read.
///
/// The spec's `t9944InBlock` carries a single length-1 `dummy` placeholder
/// (Dummy), so callers supply nothing. Modeled after `T8426InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T9944InBlock {
    /// Dummy placeholder / Dummy (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t9944` request — wraps the input block under the `t9944InBlock` key.
///
/// Serializes to `{"t9944InBlock":{"dummy":""}}`. `t9944` is not paginated and
/// takes no caller identifier, so there are no continuation fields and no
/// caller-supplied fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T9944Request {
    #[serde(rename = "t9944InBlock")]
    pub inblock: T9944InBlock,
}

impl T9944Request {
    /// Build a `t9944` index-option master request. Takes no caller input; the
    /// `dummy` placeholder serializes as an empty string.
    pub fn new() -> Self {
        T9944Request {
            inblock: T9944InBlock {
                dummy: String::new(),
            },
        }
    }
}

impl Default for T9944Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t9944OutBlock` — one index-option master row.
///
/// The data-bearing repeated block (`t9944OutBlock[]`, confirmed from the raw
/// capture's `res_example` array — rows are direct elements under the
/// `t9944OutBlock` key). The 3 spec fields. `hname` (종목명, the index-option
/// contract name) is the canonical identity field, resolved by its `korean_name`
/// from the baseline; `shcode` (단축코드) / `expcode` (확장코드) are the contract
/// codes. `shcode` uses [`ls_core::string_or_number`] for wire-type tolerance
/// (the gateway may send a code field as a JSON number); `#[serde(default)]` lets
/// a sparse row deserialize cleanly. Field names mirror the LS spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T9944OutBlock {
    /// Contract name / 종목명 (the canonical identity field).
    pub hname: String,
    /// Short code / 단축코드 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Expanded code / 확장코드.
    pub expcode: String,
}

/// `t9944` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the
/// index-option master row array under the `t9944OutBlock` key, tolerated as a
/// single object OR an array via [`ls_core::de_vec_or_single`]. All
/// `#[serde(default)]` so a terse or empty envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T9944Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t9944OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T9944OutBlock>,
}

// ---------------------------------------------------------------------------
// Night-derivatives lane (reach wave U6) — KRX야간파생 market-data reads, routed
// through `market_session` (KTD3). These are `venue_session: krx_extended`: the
// data is only meaningful in the night session (~18:00–05:00 KST), NOT the
// regular ~09:00–15:30 clock (KTD7). Members flip Implemented venue-provisional
// on a reachable in-window probe; an off-window empty result is not a valid
// attempt. Out-block shape from the raw capture (KTD5): t8455 master is an array
// (A0005); t8460 carries a single near-month header (A0003) + call/put option
// arrays (A0005); t8463 carries a single investor-code header (A0003) + a
// time-series row array (A0005). Canonical field by baseline `korean_name`
// (KTD6); t8463's `cnt` request field serializes as a JSON number (KTD4).
// ---------------------------------------------------------------------------

/// Input block for `t8455` — KRX야간파생 마스터조회(API용) (night-derivatives master).
///
/// `gubun` selects the instrument class (구분: `"NF"` 야간선물 / `"NC"` 야간콜옵션 /
/// `"NM"` 야간미니 / `"NO"` 야간풋옵션), a caller-supplied selector — not an
/// instrument identifier.
#[derive(Serialize, Debug, Clone)]
pub struct T8455InBlock {
    /// Class selector / 구분 (`"NF"`/`"NC"`/`"NM"`/`"NO"`).
    pub gubun: String,
}

/// `t8455` request — serializes to `{"t8455InBlock":{"gubun":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T8455Request {
    #[serde(rename = "t8455InBlock")]
    pub inblock: T8455InBlock,
}
impl T8455Request {
    /// Build a `t8455` night-derivatives master request for one instrument class.
    pub fn new(gubun: impl Into<String>) -> Self {
        T8455Request {
            inblock: T8455InBlock {
                gubun: gubun.into(),
            },
        }
    }
}

/// `t8455OutBlock` — one night-derivatives master row (`t8455OutBlock[]`, an
/// ARRAY block in the raw capture). A representative subset; numeric `tradeunit`
/// (거래승수) via [`ls_core::string_or_number`]. `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8455OutBlock {
    /// Issue name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Standard code / 표준코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
    /// Trade multiplier / 거래승수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradeunit: String,
}

/// `t8455` response — the master row array under the `t8455OutBlock` key,
/// tolerated as single-or-array via [`ls_core::de_vec_or_single`] (KTD5). All
/// `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8455Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8455OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8455OutBlock>,
}

/// Input block for `t8460` — KRX야간파생 옵션 전광판 (night-derivatives option board).
///
/// `yyyymm` is the contract month (월물, or `"WN"` for a weekly); `gubun` selects
/// the index variant (`"G"` 원지수 / `"W"` 위클리). Both caller-supplied.
#[derive(Serialize, Debug, Clone)]
pub struct T8460InBlock {
    /// Contract month / 월물 (혹은 주물 `"WN"`).
    pub yyyymm: String,
    /// Index variant / 구분 (`"G"` 원지수 / `"W"` 위클리).
    pub gubun: String,
}

/// `t8460` request — serializes to `{"t8460InBlock":{"yyyymm":...,"gubun":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T8460Request {
    #[serde(rename = "t8460InBlock")]
    pub inblock: T8460InBlock,
}
impl T8460Request {
    /// Build a `t8460` night-option-board request for one contract month + variant.
    pub fn new(yyyymm: impl Into<String>, gubun: impl Into<String>) -> Self {
        T8460Request {
            inblock: T8460InBlock {
                yyyymm: yyyymm.into(),
                gubun: gubun.into(),
            },
        }
    }
}

/// `t8460OutBlock` — the near-month futures header (single Object, A0003 in the
/// raw capture). A representative subset; numeric-bearing fields via
/// [`ls_core::string_or_number`]. `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8460OutBlock {
    /// Near-month current price / 근월물현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmprice: String,
    /// Near-month change vs. previous / 근월물전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmchange: String,
    /// Near-month volume / 근월물거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmvolume: String,
    /// Near-month futures code / 근월물선물코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gmshcode: String,
}

/// `t8460OutBlock1` — one CALL-option board row (`t8460OutBlock1[]`, an ARRAY
/// block, A0005). A representative subset; numerics via
/// [`ls_core::string_or_number`]. `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8460OutBlock1 {
    /// Strike price / 행사가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub actprice: String,
    /// Call option code / 콜옵션코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub optcode: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Best offer / 매도호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Best bid / 매수호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
}

/// `t8460OutBlock2` — one PUT-option board row (`t8460OutBlock2[]`, an ARRAY
/// block, A0005). A representative subset; numerics via
/// [`ls_core::string_or_number`]. `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8460OutBlock2 {
    /// Strike price / 행사가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub actprice: String,
    /// Put option code / 풋옵션코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub optcode: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Best offer / 매도호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Best bid / 매수호가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
}

/// `t8460` response — the near-month header `t8460OutBlock` + the call-option
/// array `t8460OutBlock1` + the put-option array `t8460OutBlock2` (each tolerated
/// single-or-array via [`ls_core::de_vec_or_single`]). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8460Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t8460OutBlock", default)]
    pub outblock: T8460OutBlock,
    #[serde(
        rename = "t8460OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T8460OutBlock1>,
    #[serde(
        rename = "t8460OutBlock2",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock2: Vec<T8460OutBlock2>,
}

/// Input block for `g3104` — 해외주식 종목정보 조회 (overseas stock-info master).
/// Same key shape as `g3101`.
#[derive(Serialize, Debug, Clone)]
pub struct G3104InBlock {
    /// Realtime/delayed distinction / 지연구분.
    pub delaygb: String,
    /// Composite key / KEY종목코드.
    pub keysymbol: String,
    /// Exchange code / 거래소코드.
    pub exchcd: String,
    /// Symbol / 종목코드.
    pub symbol: String,
}

/// `g3104` request — serializes to `{"g3104InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct G3104Request {
    #[serde(rename = "g3104InBlock")]
    pub inblock: G3104InBlock,
}
impl G3104Request {
    /// Build a `g3104` stock-info request for one overseas symbol.
    pub fn new(
        delaygb: impl Into<String>,
        keysymbol: impl Into<String>,
        exchcd: impl Into<String>,
        symbol: impl Into<String>,
    ) -> Self {
        G3104Request {
            inblock: G3104InBlock {
                delaygb: delaygb.into(),
                keysymbol: keysymbol.into(),
                exchcd: exchcd.into(),
                symbol: symbol.into(),
            },
        }
    }
}

/// `g3104OutBlock` — the overseas stock-info master (single object).
///
/// `korname` (한글종목명) is the canonical name field (KTD6). Every
/// numeric-bearing field via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct G3104OutBlock {
    /// Korean name / 한글종목명 (canonical field, KTD6).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub korname: String,
    /// English name / 영문종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub engname: String,
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Exchange name / 거래소명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub exchange_name: String,
    /// Nation name / 국가명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub nation_name: String,
    /// Currency / 통화.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub currency: String,
    /// Listed shares / 상장주식수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub share: String,
    /// Previous close / 전일종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pcls: String,
}

/// `g3104` response envelope (single out-block).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct G3104Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "g3104OutBlock", default)]
    pub outblock: G3104OutBlock,
}

/// Input block for `g3190` — 해외주식 마스터 조회 (overseas master list). Keyed by a
/// nation code (`natcode`, e.g. `"US"`) + exchange distinction (`exgubun`).
/// `readcnt` is the requested row COUNT, a numeric REQUEST field serialized as a
/// JSON number (`string_as_number`, KTD4). `cts_value` is the (string)
/// continuation token (`""` first page).
#[derive(Serialize, Debug, Clone)]
pub struct G3190InBlock {
    /// Realtime/delayed distinction / 지연구분.
    pub delaygb: String,
    /// Nation code / 국가코드 (`"US"`).
    pub natcode: String,
    /// Exchange distinction / 거래소구분.
    pub exgubun: String,
    /// Requested row count / 요청건수 (serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub readcnt: String,
    /// Continuation token / 연속조회키 (`""` first page).
    pub cts_value: String,
}

/// `g3190` request — serializes to `{"g3190InBlock":{...}}` with `readcnt` as a
/// JSON number.
#[derive(Serialize, Debug, Clone)]
pub struct G3190Request {
    #[serde(rename = "g3190InBlock")]
    pub inblock: G3190InBlock,
}
impl G3190Request {
    /// Build a `g3190` master-list request for one nation/exchange.
    pub fn new(
        delaygb: impl Into<String>,
        natcode: impl Into<String>,
        exgubun: impl Into<String>,
        readcnt: impl Into<String>,
        cts_value: impl Into<String>,
    ) -> Self {
        G3190Request {
            inblock: G3190InBlock {
                delaygb: delaygb.into(),
                natcode: natcode.into(),
                exgubun: exgubun.into(),
                readcnt: readcnt.into(),
                cts_value: cts_value.into(),
            },
        }
    }
}

/// `g3190OutBlock` — the master-list header (single object): the echo + the
/// continuation token + the row count.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct G3190OutBlock {
    /// Nation code / 국가코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub natcode: String,
    /// Continuation token / 연속조회키.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cts_value: String,
    /// Returned row count / 레코드카운트.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rec_count: String,
}

/// `g3190OutBlock1` — one master row (`g3190OutBlock1[]`, an ARRAY block).
/// `korname` (한글종목명) is the canonical name field (KTD6).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct G3190OutBlock1 {
    /// Composite key / KEY종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub keysymbol: String,
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Korean name / 한글종목명 (canonical field, KTD6).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub korname: String,
    /// English name / 영문종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub engname: String,
    /// Currency / 통화.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub currency: String,
    /// Previous close / 전일종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pcls: String,
}

/// `g3190` response envelope: header out-block + the master row array under the
/// `g3190OutBlock1` key, tolerated as single-or-array via
/// [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct G3190Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "g3190OutBlock", default)]
    pub outblock: G3190OutBlock,
    #[serde(
        rename = "g3190OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<G3190OutBlock1>,
}

// ── Overseas-futures (`o`-prefix) reads — U8 reach wave ─────────────────────
//
// Surface: `/overseas-futureoption/market-data`, group `[해외선물] 시세`,
// instrument_domain overseas_futures, venue_session unspecified (uncharted). One
// `o`-probe + KTD9 A/B (wrong path → http=404, wrong tr_cd → http=500 IGW00215,
// intended → http=200; NO 01900) confirms the domain REACHABLE and our contract
// CORRECT. The two MASTER reads (o3101 futures, o3121 option) return non-empty
// data on paper; the four live quote/order-book reads (o3105/o3106/o3125/o3126)
// answer http=200 rsp_cd=00000 with an empty body (the live overseas-futures feed
// is not provisioned on paper) → PENDING per the disposition state machine. All
// request fields are strings (no numeric REQUEST field → no `string_as_number`).

/// Input block for `o3101` — 해외선물마스터조회 (overseas-futures master list). A
/// single `gubun` selector (`""` = all); no instrument identifier.
#[derive(Serialize, Debug, Clone)]
pub struct O3101InBlock {
    /// Distinction / 구분 (`""` = all products).
    pub gubun: String,
}

/// `o3101` request — serializes to `{"o3101InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct O3101Request {
    #[serde(rename = "o3101InBlock")]
    pub inblock: O3101InBlock,
}
impl O3101Request {
    /// Build an `o3101` futures-master request for one `gubun` selector.
    pub fn new(gubun: impl Into<String>) -> Self {
        O3101Request {
            inblock: O3101InBlock {
                gubun: gubun.into(),
            },
        }
    }
}

/// `o3101OutBlock` — one overseas-futures master row (`o3101OutBlock[]`, an
/// ARRAY block per the raw capture, KTD5). `symbol_nm` (종목명) is the canonical
/// name field (KTD6); `dot_gb` is a numeric out-block field (소수점자리수). Rust
/// fields are snake_case with `#[serde(rename)]` to the PascalCase wire keys.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3101OutBlock {
    /// Symbol / 종목코드.
    #[serde(rename = "Symbol", deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Symbol name / 종목명 (canonical field, KTD6).
    #[serde(rename = "SymbolNm", deserialize_with = "ls_core::string_or_number")]
    pub symbol_nm: String,
    /// Base-product code / 기초상품코드.
    #[serde(rename = "BscGdsCd", deserialize_with = "ls_core::string_or_number")]
    pub bsc_gds_cd: String,
    /// Base-product name / 기초상품명.
    #[serde(rename = "BscGdsNm", deserialize_with = "ls_core::string_or_number")]
    pub bsc_gds_nm: String,
    /// Exchange code / 거래소코드.
    #[serde(rename = "ExchCd", deserialize_with = "ls_core::string_or_number")]
    pub exch_cd: String,
    /// Currency / 통화코드.
    #[serde(rename = "CrncyCd", deserialize_with = "ls_core::string_or_number")]
    pub crncy_cd: String,
    /// Unit price / 호가단위.
    #[serde(rename = "UntPrc", deserialize_with = "ls_core::string_or_number")]
    pub unt_prc: String,
    /// Decimal places / 소수점자리수 (numeric out field).
    #[serde(rename = "DotGb", deserialize_with = "ls_core::string_or_number")]
    pub dot_gb: String,
}

/// `o3101` response envelope: the master row array under the `o3101OutBlock` key,
/// tolerated as single-or-array via [`ls_core::de_vec_or_single`] (KTD5).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3101Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "o3101OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<O3101OutBlock>,
}

/// Input block for `o3121` — 해외선물옵션 마스터 조회 (overseas-future-option master).
/// `MktGb` selects the market (`"O"` = option) and `BscGdsCd` filters by base
/// product (`""` = all).
#[derive(Serialize, Debug, Clone)]
pub struct O3121InBlock {
    /// Market distinction / 시장구분 (`"O"` = option).
    #[serde(rename = "MktGb")]
    pub mkt_gb: String,
    /// Option base-product code / 옵션기초상품코드 (`""` = all).
    #[serde(rename = "BscGdsCd")]
    pub bsc_gds_cd: String,
}

/// `o3121` request — serializes to `{"o3121InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct O3121Request {
    #[serde(rename = "o3121InBlock")]
    pub inblock: O3121InBlock,
}
impl O3121Request {
    /// Build an `o3121` option-master request for one market + base product.
    pub fn new(mkt_gb: impl Into<String>, bsc_gds_cd: impl Into<String>) -> Self {
        O3121Request {
            inblock: O3121InBlock {
                mkt_gb: mkt_gb.into(),
                bsc_gds_cd: bsc_gds_cd.into(),
            },
        }
    }
}

/// `o3121OutBlock` — one overseas-future-option master row (`o3121OutBlock[]`,
/// an ARRAY block per the raw capture, KTD5). `bsc_gds_nm` (기초상품명) is the
/// canonical name field (KTD6); `dot_gb` is a numeric out-block field. Rust
/// fields are snake_case with `#[serde(rename)]` to the PascalCase wire keys.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3121OutBlock {
    /// Symbol / 종목코드.
    #[serde(rename = "Symbol", deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Base-product code / 옵션기초상품코드.
    #[serde(rename = "BscGdsCd", deserialize_with = "ls_core::string_or_number")]
    pub bsc_gds_cd: String,
    /// Base-product name / 기초상품명 (canonical field, KTD6).
    #[serde(rename = "BscGdsNm", deserialize_with = "ls_core::string_or_number")]
    pub bsc_gds_nm: String,
    /// Exchange code / 거래소코드.
    #[serde(rename = "ExchCd", deserialize_with = "ls_core::string_or_number")]
    pub exch_cd: String,
    /// Strike price / 행사가.
    #[serde(rename = "XrcPrc", deserialize_with = "ls_core::string_or_number")]
    pub xrc_prc: String,
    /// Option type code / 콜풋구분.
    #[serde(rename = "OptTpCode", deserialize_with = "ls_core::string_or_number")]
    pub opt_tp_code: String,
    /// Decimal places / 소수점자리수 (numeric out field).
    #[serde(rename = "DotGb", deserialize_with = "ls_core::string_or_number")]
    pub dot_gb: String,
}

/// `o3121` response envelope: the master row array under the `o3121OutBlock` key,
/// tolerated as single-or-array via [`ls_core::de_vec_or_single`] (KTD5).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3121Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "o3121OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<O3121OutBlock>,
}

// ---------------------------------------------------------------------------
// Domestic stock master/reference breadth wave (plan -004). Non-paginated
// `market_session` reads; each out-block is a single Object-Array modeled as a
// `Vec<...>` via `de_vec_or_single` with the literal `<tr>OutBlock` key read from
// the raw `res_example` (KTD3). No numeric request fields here, so no
// `string_as_number`.
// ---------------------------------------------------------------------------

/// Input block for `t9945` — 주식마스터조회 (stock master). `gubun` selects the
/// market: `"1"` = KOSPI (KSP), `"2"` = KOSDAQ (KSD).
#[derive(Serialize, Debug, Clone)]
pub struct T9945InBlock {
    /// Market selector / 구분 (KSP:1 KSD:2).
    pub gubun: String,
}

/// `t9945` request — serializes to `{"t9945InBlock":{"gubun":"1"}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T9945Request {
    #[serde(rename = "t9945InBlock")]
    pub inblock: T9945InBlock,
}
impl T9945Request {
    /// Build a `t9945` stock-master request for one market (`"1"`=KOSPI, `"2"`=KOSDAQ).
    pub fn new(gubun: impl Into<String>) -> Self {
        T9945Request {
            inblock: T9945InBlock {
                gubun: gubun.into(),
            },
        }
    }
}

/// `t9945OutBlock` — one stock-master row: the ticker, its codes, and Korean name.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T9945OutBlock {
    /// Stock name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Short code / 단축코드 (the canonical 6-digit ticker).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Expanded code / 확장코드 (ISIN).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
    /// ETF flag / ETF구분 (`"1"` = ETF).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub etfchk: String,
    /// NXT-listing flag / NXT상장구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub nxt_chk: String,
    /// Reserved filler / filler.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub filler: String,
}

/// `t9945` response — the stock-master array under `t9945OutBlock`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T9945Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t9945OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T9945OutBlock>,
}

// === plan -004 batch C — market_session reference reads ====================

/// Input block for `t1532`.
#[derive(Serialize, Debug, Clone)]
pub struct T1532InBlock {
    pub shcode: String,
}

/// `t1532` request.
#[derive(Serialize, Debug, Clone)]
pub struct T1532Request {
    #[serde(rename = "t1532InBlock")]
    pub inblock: T1532InBlock,
}
impl T1532Request {
    /// Build a `t1532` request.
    pub fn new(shcode: impl Into<String>) -> Self {
        T1532Request {
            inblock: T1532InBlock {
                shcode: shcode.into(),
            },
        }
    }
}

/// `t1532OutBlock` — one result row (repeated).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1532OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmname: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub avgdiff: String,
}

/// `t1532` response (single-or-array out-block).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1532Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1532OutBlock", default, deserialize_with = "ls_core::de_vec_or_single")]
    pub outblock: Vec<T1532OutBlock>,
}

/// Input block for `t1533`.
#[derive(Serialize, Debug, Clone)]
pub struct T1533InBlock {
    pub gubun: String,
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub chgdate: String,
}

/// `t1533` request.
#[derive(Serialize, Debug, Clone)]
pub struct T1533Request {
    #[serde(rename = "t1533InBlock")]
    pub inblock: T1533InBlock,
}
impl T1533Request {
    /// Build a `t1533` request.
    pub fn new(gubun: impl Into<String>) -> Self {
        T1533Request {
            inblock: T1533InBlock {
                gubun: gubun.into(),
                chgdate: "0".to_string(),
            },
        }
    }
}

/// `t1533OutBlock` — summary block.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1533OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bdate: String,
}

/// `t1533OutBlock1` — one result row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1533OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmname: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tmcode: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub avgdiff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub totcnt: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upcnt: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dncnt: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub chgdiff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub uprate: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff_vol: String,
}

/// `t1533` response.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1533Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1533OutBlock", default)]
    pub outblock: T1533OutBlock,
    #[serde(rename = "t1533OutBlock1", default, deserialize_with = "ls_core::de_vec_or_single")]
    pub outblock1: Vec<T1533OutBlock1>,
}
