//! ETF NAV / PDF / daily-trend reads.
//!
//! Wave-1 split out of `market_session/mod.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use etf::*;` so every `ls_sdk::market_session::*` path is unchanged.
use super::*;


// ---------------------------------------------------------------------------
// t1901 — ETF현재가(시세)조회 (ETF current-price snapshot). market_session read,
// single OutBlock object; path /stock/etf. Mirrors t1102's single-object shape.
// ---------------------------------------------------------------------------

/// Input block for `t1901` — the ETF short code (단축코드). `shcode`-only.
#[derive(Serialize, Debug, Clone)]
pub struct T1901InBlock {
    /// Short code / 단축코드 (e.g. `"069500"` KODEX 200).
    pub shcode: String,
}

/// `t1901` request — serializes to `{"t1901InBlock":{"shcode":...}}`. Not paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T1901Request {
    #[serde(rename = "t1901InBlock")]
    pub inblock: T1901InBlock,
}

impl T1901Request {
    /// Build a `t1901` ETF quote request for one short code.
    pub fn new(shcode: impl Into<String>) -> Self {
        T1901Request {
            inblock: T1901InBlock {
                shcode: shcode.into(),
            },
        }
    }
}

/// `t1901OutBlock` — the ETF snapshot quote (a representative, spec-grounded subset
/// of the LS `t1901OutBlock`). Numeric-bearing fields use [`ls_core::string_or_number`]
/// (the gateway sends numbers or strings); `#[serde(default)]` lets a sparse out-block
/// deserialize, and unknown fields are ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1901OutBlock {
    /// Korean name / 한글 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비 구분.
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
    /// Reference (base) price / 기준가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub recprice: String,
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
    /// Trading value / 누적거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
}

/// `t1901` response envelope — the ETF snapshot under the `t1901OutBlock` key.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1901Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1901OutBlock", default)]
    pub outblock: T1901OutBlock,
}

// ---------------------------------------------------------------------------
// t1959 — LP대상종목정보조회 (LP target-issue info). market_session ELW read; one
// repeated `t1959OutBlock1` row per LP-target issue (tolerated single-or-array via
// `ls_core::de_vec_or_single`); path /stock/elw, group [주식] ELW. 1-field request
// — `shcode` (a six-digit short code); an EMPTY `shcode` returns the full LP-target
// list (this is a list/ranking read, not a single-instrument read). All-String
// request — no numeric request slot, so no `string_as_number` (cf. t8407's nrec).
// ---------------------------------------------------------------------------

/// Input block for `t1959` — the LP-target short code (`shcode`).
///
/// An empty `shcode` (`""`) returns the FULL LP-target issue list; a six-digit
/// code narrows to one issue. `shcode` is an ordinary request String (no numeric
/// serialize — this read carries no numeric request slot).
#[derive(Serialize, Debug, Clone)]
pub struct T1959InBlock {
    /// Short code / 종목코드 — empty (`""`) returns the full LP-target list.
    pub shcode: String,
}

/// `t1959` request — serializes to `{"t1959InBlock":{"shcode":""}}`. Not paginated
/// (`facets.self_paginated: false`).
#[derive(Serialize, Debug, Clone)]
pub struct T1959Request {
    #[serde(rename = "t1959InBlock")]
    pub inblock: T1959InBlock,
}

impl T1959Request {
    /// Build a `t1959` LP-target-issue request for one `shcode`. Pass `""` to fetch
    /// the full LP-target list. See [`T1959Request::new`] for the empty-default
    /// convenience constructor.
    pub fn for_shcode(shcode: impl Into<String>) -> Self {
        T1959Request {
            inblock: T1959InBlock {
                shcode: shcode.into(),
            },
        }
    }

    /// Build a `t1959` request defaulting `shcode` to `""` (the full LP-target
    /// list). The list/ranking entry point.
    pub fn new() -> Self {
        T1959Request::for_shcode("")
    }
}

impl Default for T1959Request {
    fn default() -> Self {
        T1959Request::new()
    }
}

/// `t1959OutBlock1` — one LP-target issue row (a representative, spec-grounded
/// subset): the short code / name keys, the current price, the prior-day change
/// sign / amount / rate, the cumulative volume / value, and the LP order-availability
/// flag. Every numeric-bearing field via [`ls_core::string_or_number`] (`rate` is a
/// spec `Number`); `#[serde(default)]` lets a sparse row deserialize and unknown
/// fields are ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1959OutBlock1 {
    /// Short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Issue name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Prior-day change sign / 부호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Prior-day change amount / 대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Prior-day change rate / 등락율 (spec `Number`).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub rate: String,
    /// Cumulative volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Cumulative value / 누적거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
    /// LP order-availability / LP주문가능여부.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lp_gb: String,
}

/// `t1959` response envelope — the LP-target issue rows under the `t1959OutBlock1`
/// key (tolerated single-or-array via [`ls_core::de_vec_or_single`]). All
/// `#[serde(default)]` so a terse/empty envelope deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1959Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t1959OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1959OutBlock1>,
}

// ---------------------------------------------------------------------------
// t1902 — ETF시간별추이 (ETF intraday NAV/price trend). market_session domestic-stock
// ETF read; path /stock/etf, group [주식] ETF. 2-field request — shcode, time
// (HHMMSS — `""` for the latest). All-String request — no numeric request slot.
// Response: a single `t1902OutBlock` header (time/hname/upname) + a repeated
// `t1902OutBlock1` time-series ARRAY (one row per timestamp: price/NAV/index)
// tolerated single-or-array via `ls_core::de_vec_or_single`.
// ---------------------------------------------------------------------------

/// Input block for `t1902` — the ETF short code (`shcode`) + a `time` (HHMMSS — `""`
/// for the latest). All ordinary request Strings.
#[derive(Serialize, Debug, Clone)]
pub struct T1902InBlock {
    /// ETF short code / 단축코드.
    pub shcode: String,
    /// Time / 시간 (HHMMSS — `""` for the latest).
    pub time: String,
}

/// `t1902` request — serializes to `{"t1902InBlock":{...}}`. Not paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T1902Request {
    #[serde(rename = "t1902InBlock")]
    pub inblock: T1902InBlock,
}

impl T1902Request {
    /// Build a `t1902` ETF intraday-trend request for one `shcode` + `time`.
    pub fn new(shcode: impl Into<String>, time: impl Into<String>) -> Self {
        T1902Request {
            inblock: T1902InBlock {
                shcode: shcode.into(),
                time: time.into(),
            },
        }
    }
}

/// `t1902OutBlock` — the ETF header (the snapshot time, the issue name, and the
/// sector-index name). String fields via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1902OutBlock {
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Issue name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Sector-index name / 업종지수명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upname: String,
}

/// `t1902OutBlock1` — one ETF intraday-trend row (representative subset): the
/// timestamp, the current price + change + cumulative volume, the NAV, and the
/// underlying index. Every numeric-bearing field via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1902OutBlock1 {
    /// Time / 시간.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub time: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Prior-day change sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Prior-day change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Cumulative volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// NAV / NAV.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub nav: String,
    /// Index / 지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisu: String,
}

/// `t1902` response envelope — the single `t1902OutBlock` header + the repeated
/// `t1902OutBlock1` time-series rows (tolerated single-or-array via
/// [`ls_core::de_vec_or_single`]). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1902Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1902OutBlock", default)]
    pub outblock: T1902OutBlock,
    #[serde(
        rename = "t1902OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1902OutBlock1>,
}

// ---------------------------------------------------------------------------
// t1904 — ETF구성종목조회 (ETF PDF / constituent basket). market_session
// domestic-stock ETF read; path /stock/etf, group [주식] ETF. 3-field request —
// shcode, date (PDF적용일자), sgb (정렬기준 — `1`:평가금액, `2`:증권수). All-String
// request — no numeric request slot. Response: a single `t1904OutBlock` header (the
// ETF quote + NAV + fund totals) + a repeated `t1904OutBlock1` constituent ARRAY
// (one row per basket issue) tolerated single-or-array via `ls_core::de_vec_or_single`.
// ---------------------------------------------------------------------------

/// Input block for `t1904` — the ETF short code (`shcode`), the PDF apply `date`
/// (YYYYMMDD), and the sort key `sgb` (`1`:평가금액, `2`:증권수). All ordinary
/// request Strings.
#[derive(Serialize, Debug, Clone)]
pub struct T1904InBlock {
    /// ETF short code / ETF단축코드.
    pub shcode: String,
    /// PDF apply date / PDF적용일자 (YYYYMMDD).
    pub date: String,
    /// Sort key / 정렬기준 (`1`:평가금액, `2`:증권수).
    pub sgb: String,
}

/// `t1904` request — serializes to `{"t1904InBlock":{...}}`. Not paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T1904Request {
    #[serde(rename = "t1904InBlock")]
    pub inblock: T1904InBlock,
}

impl T1904Request {
    /// Build a `t1904` ETF constituent request for one `shcode` + apply `date` +
    /// sort key `sgb`.
    pub fn new(
        shcode: impl Into<String>,
        date: impl Into<String>,
        sgb: impl Into<String>,
    ) -> Self {
        T1904Request {
            inblock: T1904InBlock {
                shcode: shcode.into(),
                date: date.into(),
                sgb: sgb.into(),
            },
        }
    }
}

/// `t1904OutBlock` — the ETF header (representative subset): the PDF apply date, the
/// ETF current price + cumulative volume, the NAV, the sector name, the net-asset
/// total, the constituent count, and the manager name. Every numeric-bearing field
/// via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1904OutBlock {
    /// PDF apply date / PDF적용일자.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// ETF current price / ETF현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// ETF cumulative volume / ETF누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// NAV / NAV.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub nav: String,
    /// Sector name / 업종명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upname: String,
    /// Net-asset total (단위:억) / 순자산총액.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub etftotcap: String,
    /// Constituent count / 구성종목수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub etfnum: String,
    /// Manager name / 운용사명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub opcom_nmk: String,
}

/// `t1904OutBlock1` — one ETF constituent row (representative subset): the issue
/// code + name, the current price + change + cumulative volume, the constituent
/// weight, and the evaluation amount. Every numeric-bearing field via
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1904OutBlock1 {
    /// Issue short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Issue name / 한글명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Prior-day change sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Prior-day change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Cumulative volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Evaluation amount / 평가금액.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pvalue: String,
    /// Weight (by evaluation amount) / 비중(평가금액).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub weight: String,
}

/// `t1904` response envelope — the single `t1904OutBlock` header + the repeated
/// `t1904OutBlock1` constituent rows (tolerated single-or-array via
/// [`ls_core::de_vec_or_single`]). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1904Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1904OutBlock", default)]
    pub outblock: T1904OutBlock,
    #[serde(
        rename = "t1904OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1904OutBlock1>,
}

// ---------------------------------------------------------------------------
// t1906 — ETFLP호가 (ETF LP order-book snapshot). market_session read, single
// OutBlock object; path /stock/etf. shcode-only request. Mirrors t1901's
// single-object shape (same /stock/etf family).
// ---------------------------------------------------------------------------

/// Input block for `t1906` — the ETF short code (단축코드). `shcode`-only.
#[derive(Serialize, Debug, Clone)]
pub struct T1906InBlock {
    /// Short code / 단축코드 (e.g. `"152100"` ARIRANG 200).
    pub shcode: String,
}

/// `t1906` request — serializes to `{"t1906InBlock":{"shcode":...}}`. Not paginated.
#[derive(Serialize, Debug, Clone)]
pub struct T1906Request {
    #[serde(rename = "t1906InBlock")]
    pub inblock: T1906InBlock,
}

impl T1906Request {
    /// Build a `t1906` ETF LP order-book request for one short code.
    pub fn new(shcode: impl Into<String>) -> Self {
        T1906Request {
            inblock: T1906InBlock {
                shcode: shcode.into(),
            },
        }
    }
}

/// `t1906OutBlock` — the ETF LP order-book snapshot (a representative, spec-grounded
/// subset of the LS `t1906OutBlock`): the current-price header, level-1 + level-2
/// offer/bid price+quantity, LP level-1 quantities, the day's OHLC, and the limit
/// prices. Every numeric-bearing field uses [`ls_core::string_or_number`] (the gateway
/// sends numbers or strings); `#[serde(default)]` lets a sparse out-block deserialize,
/// and unknown fields are ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1906OutBlock {
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
    /// LP offer quantity, level 1 / LP매도호가수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lp_offerrem1: String,
    /// LP bid quantity, level 1 / LP매수호가수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lp_bidrem1: String,
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

/// `t1906` response envelope — the ETF LP order-book snapshot under the
/// `t1906OutBlock` key. All `#[serde(default)]` so a terse/empty envelope deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1906Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1906OutBlock", default)]
    pub outblock: T1906OutBlock,
}

/// Input block for `t1903`.
#[derive(Serialize, Debug, Clone)]
pub struct T1903InBlock {
    pub shcode: String,
    pub date: String,
}

/// `t1903` request.
#[derive(Serialize, Debug, Clone)]
pub struct T1903Request {
    #[serde(rename = "t1903InBlock")]
    pub inblock: T1903InBlock,
}
impl T1903Request {
    /// Build a `t1903` request.
    pub fn new(shcode: impl Into<String>) -> Self {
        T1903Request {
            inblock: T1903InBlock {
                shcode: shcode.into(),
                date: "".to_string(),
            },
        }
    }
}

/// `t1903OutBlock` — summary block.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1903OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub upname: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
}

/// `t1903OutBlock1` — one result row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1903OutBlock1 {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jisu: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jichange: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub jirate: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub nav: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub navchange: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub navdiff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub grate: String,
    #[serde(rename = "crate")]
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub crate_: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t1903` response.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1903Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1903OutBlock", default)]
    pub outblock: T1903OutBlock,
    #[serde(rename = "t1903OutBlock1", default, deserialize_with = "ls_core::de_vec_or_single")]
    pub outblock1: Vec<T1903OutBlock1>,
}
