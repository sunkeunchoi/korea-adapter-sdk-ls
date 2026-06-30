//! No-input, schedule, member-firm, news and analytics reference reads.
//!
//! Wave-1 split out of `market_session/mod.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use reference::*;` so every `ls_sdk::market_session::*` path is unchanged.
use super::*;


// ---------------------------------------------------------------------------
// t8401 — 주식선물마스터조회 (stock-futures master). market_session, non-paginated.
// A no-caller-input read: the spec's `t8401InBlock` carries a single length-1
// `dummy` placeholder, so callers supply nothing. The response is a single
// out-block `t8401OutBlock` that is itself the data-bearing ROW ARRAY (the raw
// capture's `res_example` shows `"t8401OutBlock": [ {…}, … ]`, propertyType
// A0005 / propertyOrder 002.00x children) — one stock-futures contract per row.
// There is no separate count header. Modeled after `T8425` (single row-array
// out-block).
// ---------------------------------------------------------------------------

/// Input block for `t8401` — a no-caller-input read.
///
/// The spec's `t8401InBlock` carries a single length-1 `dummy` placeholder
/// (Dummy), so callers supply nothing. Modeled after `T8425InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T8401InBlock {
    /// Dummy placeholder / Dummy (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t8401` request — wraps the input block under the `t8401InBlock` key.
///
/// Serializes to `{"t8401InBlock":{"dummy":""}}`. `t8401` is not paginated and
/// takes no caller identifier, so there are no continuation fields and no
/// caller-supplied fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T8401Request {
    #[serde(rename = "t8401InBlock")]
    pub inblock: T8401InBlock,
}

impl T8401Request {
    /// Build a `t8401` stock-futures master request. Takes no caller input; the
    /// `dummy` placeholder serializes as an empty string.
    pub fn new() -> Self {
        T8401Request {
            inblock: T8401InBlock {
                dummy: String::new(),
            },
        }
    }
}

impl Default for T8401Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t8401OutBlock` — one stock-futures master row.
///
/// The data-bearing repeated block (`t8401OutBlock[]`). `hname` (종목명, the
/// stock-futures contract name) is the canonical identity field, resolved by its
/// `korean_name` from the baseline; the remaining fields are the contract codes.
/// `#[serde(default)]` lets a sparse row deserialize cleanly. Field names mirror
/// the LS spec verbatim. All fields are spec `String` types; no numeric coercion
/// is required here.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8401OutBlock {
    /// Contract name / 종목명 (the canonical identity field).
    pub hname: String,
    /// Short code / 단축코드.
    pub shcode: String,
    /// Expanded code / 확장코드.
    pub expcode: String,
    /// Underlying-asset code / 기초자산코드.
    pub basecode: String,
}

/// `t8401` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the
/// stock-futures master row array under the `t8401OutBlock` key, tolerated as a
/// single object OR an array via [`ls_core::de_vec_or_single`]. All
/// `#[serde(default)]` so a terse or empty envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8401Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8401OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8401OutBlock>,
}

// ---------------------------------------------------------------------------
// t8426 — 상품선물마스터조회 (commodity-futures master). market_session,
// non-paginated. A no-caller-input read: the spec's `t8426InBlock` carries a
// single length-1 `dummy` placeholder, so callers supply nothing. The response
// is a single out-block `t8426OutBlock` that is itself the data-bearing ROW
// ARRAY (the raw capture's `res_example` shows `"t8426OutBlock": [ {…}, … ]`) —
// one commodity-futures contract per row. There is no separate count header.
// Modeled after `T8401` (single row-array out-block).
// ---------------------------------------------------------------------------

/// Input block for `t8426` — a no-caller-input read.
///
/// The spec's `t8426InBlock` carries a single length-1 `dummy` placeholder
/// (Dummy), so callers supply nothing. Modeled after `T8401InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T8426InBlock {
    /// Dummy placeholder / Dummy (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t8426` request — wraps the input block under the `t8426InBlock` key.
///
/// Serializes to `{"t8426InBlock":{"dummy":""}}`. `t8426` is not paginated and
/// takes no caller identifier, so there are no continuation fields and no
/// caller-supplied fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T8426Request {
    #[serde(rename = "t8426InBlock")]
    pub inblock: T8426InBlock,
}

impl T8426Request {
    /// Build a `t8426` commodity-futures master request. Takes no caller input;
    /// the `dummy` placeholder serializes as an empty string.
    pub fn new() -> Self {
        T8426Request {
            inblock: T8426InBlock {
                dummy: String::new(),
            },
        }
    }
}

impl Default for T8426Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t8426OutBlock` — one commodity-futures master row.
///
/// The data-bearing repeated block (`t8426OutBlock[]`, confirmed from the raw
/// capture's `res_example` array). `hname` (종목명, the commodity-futures
/// contract name) is the canonical identity field, resolved by its `korean_name`
/// from the baseline; the remaining fields are the contract codes. `shcode`
/// (단축코드) uses [`ls_core::string_or_number`] for wire-type tolerance (the
/// gateway may send a numeric-looking code as a JSON number);
/// `#[serde(default)]` lets a sparse row deserialize cleanly. Field names mirror
/// the LS spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8426OutBlock {
    /// Contract name / 종목명 (the canonical identity field).
    pub hname: String,
    /// Short code / 단축코드 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Expanded code / 확장코드.
    pub expcode: String,
}

/// `t8426` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the
/// commodity-futures master row array under the `t8426OutBlock` key, tolerated
/// as a single object OR an array via [`ls_core::de_vec_or_single`]. All
/// `#[serde(default)]` so a terse or empty envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8426Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8426OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8426OutBlock>,
}

// ---------------------------------------------------------------------------
// t8433 — 지수옵션마스터조회API용 (index-option master). market_session,
// non-paginated. A no-caller-input read: the spec's `t8433InBlock` carries a
// single length-1 `dummy` placeholder, so callers supply nothing. The response
// is a single out-block `t8433OutBlock` that is itself the data-bearing ROW
// ARRAY (the raw capture's `res_example` shows `"t8433OutBlock": [ {…}, … ]`,
// rows direct under the key, no numbered sub-block) — one index-option contract
// per row. There is no separate count header. The row is modeled after the
// 9-field `T8435` row-array out-block (T8426 has only 3 fields; the index-option
// row carries the daily limit/close reference prices too).
// ---------------------------------------------------------------------------

/// Input block for `t8433` — a no-caller-input read.
///
/// The spec's `t8433InBlock` carries a single length-1 `dummy` placeholder
/// (Dummy), so callers supply nothing. Modeled after `T8426InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T8433InBlock {
    /// Dummy placeholder / Dummy (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t8433` request — wraps the input block under the `t8433InBlock` key.
///
/// Serializes to `{"t8433InBlock":{"dummy":""}}`. `t8433` is not paginated and
/// takes no caller identifier, so there are no continuation fields and no
/// caller-supplied fields in the body.
#[derive(Serialize, Debug, Clone)]
pub struct T8433Request {
    #[serde(rename = "t8433InBlock")]
    pub inblock: T8433InBlock,
}

impl T8433Request {
    /// Build a `t8433` index-option master request. Takes no caller input; the
    /// `dummy` placeholder serializes as an empty string.
    pub fn new() -> Self {
        T8433Request {
            inblock: T8433InBlock {
                dummy: String::new(),
            },
        }
    }
}

impl Default for T8433Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t8433OutBlock` — one index-option master row.
///
/// The data-bearing repeated block (`t8433OutBlock[]`, confirmed from the raw
/// capture's `res_example` array — rows are direct elements under the
/// `t8433OutBlock` key). A representative, spec-grounded subset modeled after the
/// 9-field [`T8435OutBlock`] row. `hname` (종목명, the index-option contract
/// name) is the canonical identity field, resolved by its `korean_name` from the
/// baseline; `shcode`/`expcode` are the contract codes, and the price fields are
/// the daily limit/close references (상한가/하한가/전일종가/전일고가/전일저가/
/// 기준가). `shcode` and the `Number`-typed price fields use
/// [`ls_core::string_or_number`] for wire-type tolerance (the gateway sends these
/// as JSON strings in the capture but may send numbers); `#[serde(default)]` lets
/// a sparse row deserialize cleanly. Field names mirror the LS spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8433OutBlock {
    /// Contract name / 종목명 (the canonical identity field).
    pub hname: String,
    /// Short code / 단축코드 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Expanded code / 확장코드.
    pub expcode: String,
    /// Upper limit price / 상한가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hprice: String,
    /// Lower limit price / 하한가 (tolerant of a string OR number wire value).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lprice: String,
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

/// `t8433` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields; `outblock` is the
/// index-option master row array under the `t8433OutBlock` key, tolerated as a
/// single object OR an array via [`ls_core::de_vec_or_single`]. All
/// `#[serde(default)]` so a terse or empty envelope still deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8433Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8433OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8433OutBlock>,
}

/// Input block for `t3102` — 뉴스본문 (news body). Keyed by `sNewsno`, a news
/// number sourced ONLY from the realtime `NWS` WebSocket title feed — there is
/// no REST producer of a news number, so the caller input is unresolved in this
/// (REST-only) wave (HELD).
#[derive(Serialize, Debug, Clone)]
pub struct T3102InBlock {
    /// News number / 뉴스번호.
    #[serde(rename = "sNewsno")]
    pub news_no: String,
}

/// `t3102` request — wraps the in-block under `t3102InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T3102Request {
    #[serde(rename = "t3102InBlock")]
    pub inblock: T3102InBlock,
}
impl T3102Request {
    /// Build a `t3102` news-body request for one news number.
    pub fn new(news_no: impl Into<String>) -> Self {
        T3102Request {
            inblock: T3102InBlock {
                news_no: news_no.into(),
            },
        }
    }
}

/// `t3102OutBlock2` — the news title block (single Object in the raw capture).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T3102OutBlock2 {
    /// News title / 뉴스타이틀.
    #[serde(rename = "sTitle", deserialize_with = "ls_core::string_or_number")]
    pub title: String,
}

/// `t3102` response — the title block under `t3102OutBlock2`. The body/issue
/// blocks (`t3102OutBlock`/`t3102OutBlock1`, Object Arrays in the raw capture)
/// are not modeled: this read ships HELD (input-unresolved), so only the title
/// block is pinned for the offline round-trip.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T3102Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t3102OutBlock2", default)]
    pub outblock2: T3102OutBlock2,
}

/// Input block for `t3320` — FNG_요약 (FnGuide company summary). Keyed by
/// `gicode`, a stock code (종목코드). The paper gateway accepts the bare 6-digit
/// ticker (e.g. `"005930"` for 삼성전자), confirmed on a live paper smoke.
#[derive(Serialize, Debug, Clone)]
pub struct T3320InBlock {
    /// Stock code / 종목코드 (bare 6-digit ticker, e.g. `"005930"`).
    pub gicode: String,
}

/// `t3320` request — wraps the in-block under `t3320InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T3320Request {
    #[serde(rename = "t3320InBlock")]
    pub inblock: T3320InBlock,
}
impl T3320Request {
    /// Build a `t3320` company-summary request for one FnGuide company code.
    pub fn new(gicode: impl Into<String>) -> Self {
        T3320Request {
            inblock: T3320InBlock {
                gicode: gicode.into(),
            },
        }
    }
}

/// `t3320OutBlock` — the company-summary header (single Object in the raw
/// capture). A representative, spec-grounded subset; numeric-bearing fields via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T3320OutBlock {
    /// Korean company name / 한글기업명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub company: String,
    /// Market segment name / 시장구분명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub marketnm: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Previous close / 전일종가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jnilclose: String,
    /// Market capitalization / 시가총액.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sigavalue: String,
}

/// `t3320OutBlock1` — the financial-ratios block (single Object in the raw
/// capture). A representative subset (PER/EPS/PBR/BPS); numerics via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T3320OutBlock1 {
    /// Company code / 기업코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gicode: String,
    /// Price-to-earnings ratio / PER.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub per: String,
    /// Earnings per share / EPS.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub eps: String,
    /// Price-to-book ratio / PBR.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pbr: String,
    /// Book value per share / BPS.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bps: String,
}

/// `t3320` response — the summary `t3320OutBlock` + ratios `t3320OutBlock1`
/// (both single Objects per the raw capture).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T3320Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t3320OutBlock", default)]
    pub outblock: T3320OutBlock,
    #[serde(rename = "t3320OutBlock1", default)]
    pub outblock1: T3320OutBlock1,
}

/// Input block for `t3202` — 종목별증시일정 (per-stock market schedule). `date`
/// is an optional filter (empty = the full schedule for the ticker).
#[derive(Serialize, Debug, Clone)]
pub struct T3202InBlock {
    /// Short code / 종목코드.
    pub shcode: String,
    /// Date filter / 일자 (empty = all).
    pub date: String,
}

/// `t3202` request — serializes to `{"t3202InBlock":{"shcode":"...","date":""}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T3202Request {
    #[serde(rename = "t3202InBlock")]
    pub inblock: T3202InBlock,
}
impl T3202Request {
    /// Build a `t3202` schedule request for one ticker (full schedule, no date filter).
    pub fn new(shcode: impl Into<String>) -> Self {
        T3202Request {
            inblock: T3202InBlock {
                shcode: shcode.into(),
                date: String::new(),
            },
        }
    }
}

/// `t3202OutBlock` — one schedule row: the corporate event for the ticker.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T3202OutBlock {
    /// Issuer number / 발행체번호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub custno: String,
    /// Issuer name / 발행회사명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub custnm: String,
    /// Reference date / 기준일 (YYYYMMDD).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub recdt: String,
    /// Short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Table id / 테이블아이디.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tableid: String,
    /// Event name / 업무명 (the canonical schedule label, e.g. 주주총회).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upunm: String,
    /// Event class / 업무구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upgu: String,
}

/// `t3202` response — the schedule array under `t3202OutBlock`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T3202Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t3202OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T3202OutBlock>,
}

/// Input block for `t1764`.
#[derive(Serialize, Debug, Clone)]
pub struct T1764InBlock {
    pub shcode: String,
    pub gubun1: String,
}

/// `t1764` request.
#[derive(Serialize, Debug, Clone)]
pub struct T1764Request {
    #[serde(rename = "t1764InBlock")]
    pub inblock: T1764InBlock,
}
impl T1764Request {
    /// Build a `t1764` request.
    pub fn new(shcode: impl Into<String>) -> Self {
        T1764Request {
            inblock: T1764InBlock {
                shcode: shcode.into(),
                gubun1: "0".to_string(),
            },
        }
    }
}

/// `t1764OutBlock` — one result row (repeated).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1764OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradno: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub tradname: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rank: String,
}

/// `t1764` response (single-or-array out-block).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1764Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1764OutBlock", default, deserialize_with = "ls_core::de_vec_or_single")]
    pub outblock: Vec<T1764OutBlock>,
}

// ---------------------------------------------------------------------------
// t0167 — 서버시간조회 (server-time inquiry, a stateless utility read).
//
// A closure-viable utility: the gateway always returns its own clock regardless
// of market hours. The in-block carries a single empty `id` slot; the out-block
// is the date + the millisecond-resolution server time.
// ---------------------------------------------------------------------------

/// Input block for `t0167` — a single (empty) `id` slot, no caller input.
#[derive(Serialize, Debug, Clone)]
pub struct T0167InBlock {
    /// Reserved id slot / 미사용 (empty).
    pub id: String,
}

/// `t0167` request — serializes to `{"t0167InBlock":{"id":""}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T0167Request {
    #[serde(rename = "t0167InBlock")]
    pub inblock: T0167InBlock,
}

impl Default for T0167Request {
    fn default() -> Self {
        Self::new()
    }
}

impl T0167Request {
    /// Build a `t0167` server-time request (no caller input).
    pub fn new() -> Self {
        T0167Request {
            inblock: T0167InBlock { id: String::new() },
        }
    }
}

/// `t0167OutBlock` — the server date + time.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T0167OutBlock {
    /// Server date / 일자 (YYYYMMDD).
    #[serde(rename = "dt", deserialize_with = "ls_core::string_or_number")]
    pub dt: String,
    /// Server time / 시간 (HHMMSS + millis; the substantive witness).
    #[serde(rename = "time", deserialize_with = "ls_core::string_or_number")]
    pub time: String,
}

/// `t0167` response envelope.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T0167Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t0167OutBlock", default)]
    pub outblock: T0167OutBlock,
}
