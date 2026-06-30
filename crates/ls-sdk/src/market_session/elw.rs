//! ELW instrument, screener and board reads.
//!
//! Wave-1 split out of `market_session/mod.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use elw::*;` so every `ls_sdk::market_session::*` path is unchanged.
use super::*;


// ---------------------------------------------------------------------------
// t1950 — ELW현재가(시세)조회 (ELW current-price/quote). market_session ELW read;
// a single-instrument quote: the main `t1950OutBlock` is ONE object (the quote +
// ELW analytics), with a secondary `t1950OutBlock1` basket-asset array (tolerated
// single-or-array via `ls_core::de_vec_or_single`). path /stock/elw, group [주식]
// ELW. 1-field request — `shcode` (a six-digit ELW issue code; these EXPIRE, so a
// live caller sources a fresh one, e.g. from t8431). All-String request — no
// numeric request slot, so no `string_as_number`.
// ---------------------------------------------------------------------------

/// Input block for `t1950` — the ELW short code (`shcode`).
///
/// `shcode` is a six-digit ELW issue code; ELW codes EXPIRE, so a live caller
/// should source a fresh one at runtime (e.g. the first `shcode` of `t8431`).
/// Ordinary request String (no numeric serialize — no numeric request slot).
#[derive(Serialize, Debug, Clone)]
pub struct T1950InBlock {
    /// ELW short code / ELW단축코드 — a six-digit, expiring issue code.
    pub shcode: String,
}

/// `t1950` request — serializes to `{"t1950InBlock":{"shcode":"52XXXX"}}`. Not
/// paginated (`facets.self_paginated: false`).
#[derive(Serialize, Debug, Clone)]
pub struct T1950Request {
    #[serde(rename = "t1950InBlock")]
    pub inblock: T1950InBlock,
}

impl T1950Request {
    /// Build a `t1950` ELW current-price request for one `shcode` (a fresh,
    /// non-expired ELW issue code).
    pub fn for_shcode(shcode: impl Into<String>) -> Self {
        T1950Request {
            inblock: T1950InBlock {
                shcode: shcode.into(),
            },
        }
    }

    /// Build a `t1950` request for one `shcode`. Alias of [`T1950Request::for_shcode`]
    /// — there is no list/default form (a quote read needs a real issue code).
    pub fn new(shcode: impl Into<String>) -> Self {
        T1950Request::for_shcode(shcode)
    }
}

/// `t1950OutBlock` — the ELW quote (a representative, spec-grounded subset): the
/// issue name, the current price + prior-day sign / change / rate, the cumulative
/// volume / value, and the underlying-asset code + price. Every numeric-bearing
/// field via [`ls_core::string_or_number`]; `#[serde(default)]` lets a sparse
/// quote deserialize and unknown fields are ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1950OutBlock {
    /// Issue name / 한글명.
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
    /// Cumulative value / 누적거래대금.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub value: String,
    /// Underlying-asset code / 기초자산코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bcode: String,
    /// Underlying-asset current price / 기초자산현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bprice: String,
}

/// `t1950OutBlock1` — one basket-asset row (representative subset): the asset code,
/// its ratio, and its current price. Tolerated single-or-array on the response.
/// Every numeric-bearing field via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1950OutBlock1 {
    /// Basket-asset code / 기초자산코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bskcode: String,
    /// Basket-asset ratio / 기초자산비율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bskbno: String,
    /// Basket-asset current price / 기초자산현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bskprice: String,
}

/// `t1950` response envelope — the single-instrument quote under `t1950OutBlock`
/// (one object) plus the basket-asset rows under `t1950OutBlock1` (tolerated
/// single-or-array via [`ls_core::de_vec_or_single`]). All `#[serde(default)]` so
/// a terse/empty envelope deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1950Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1950OutBlock", default)]
    pub outblock: T1950OutBlock,
    #[serde(
        rename = "t1950OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1950OutBlock1>,
}

// ---------------------------------------------------------------------------
// t1954 — ELW일별주가 (ELW daily prices). A market_session ELW read; path
// /stock/elw, group [주식] ELW. Given one ELW `shcode`, a row count `cnt` and an
// optional anchor `date`, returns the daily OHLCV + ELW-analytics series under the
// `t1954OutBlock1` array (date/open/high/low/close + change/volume + parity/
// gearing/premium/...), plus a `t1954OutBlock` header (base-asset codes). `cnt` is a
// spec Number and MUST serialize as a JSON NUMBER via `ls_core::string_as_number`
// (the string form risks IGW40011). `shcode` is a six-digit ELW issue code (these
// EXPIRE — a live caller sources a fresh one, e.g. the first `shcode` of t8431). The
// gateway sends OHLC as JSON numbers and the analytics as strings, so every
// numeric-bearing response field uses `ls_core::string_or_number`. Confirmed
// non-empty on an open-window paper smoke (plan -001 open-window flip wave 2026-06-30).
// ---------------------------------------------------------------------------

/// Input block for `t1954` — the ELW short code (`shcode`), an anchor `date`
/// (`YYYYMMDD`, empty for "latest") and the row count `cnt`.
///
/// `shcode` is a six-digit ELW issue code; ELW codes EXPIRE, so a live caller should
/// source a fresh one at runtime (e.g. the first `shcode` of `t8431`). `cnt` is the
/// numeric request slot — wire-serialized as a JSON NUMBER via
/// [`ls_core::string_as_number`] (the string form risks `IGW40011`).
#[derive(Serialize, Debug, Clone)]
pub struct T1954InBlock {
    /// ELW short code / 단축코드 — a six-digit, expiring issue code.
    pub shcode: String,
    /// Anchor date / 날짜 (`YYYYMMDD`; empty for the latest session).
    pub date: String,
    /// Row count / 조회갯수 — the numeric request slot.
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub cnt: String,
}

/// `t1954` request — serializes to
/// `{"t1954InBlock":{"shcode":"52XXXX","date":"","cnt":20}}` (with `cnt` a JSON
/// number). Not paginated (`facets.self_paginated: false`).
#[derive(Serialize, Debug, Clone)]
pub struct T1954Request {
    #[serde(rename = "t1954InBlock")]
    pub inblock: T1954InBlock,
}

impl T1954Request {
    /// Build a `t1954` ELW daily-price request for one `shcode`, anchor `date`
    /// (`YYYYMMDD`, empty for the latest session) and row count `cnt`.
    pub fn new(shcode: impl Into<String>, date: impl Into<String>, cnt: u32) -> Self {
        T1954Request {
            inblock: T1954InBlock {
                shcode: shcode.into(),
                date: date.into(),
                cnt: cnt.to_string(),
            },
        }
    }

    /// Build a `t1954` request for one `shcode` with the default window (latest
    /// `date`, 20 rows).
    pub fn for_shcode(shcode: impl Into<String>) -> Self {
        T1954Request::new(shcode, "", 20)
    }
}

/// `t1954OutBlock` — the daily-series header: the anchor date plus the base-asset
/// keys (현물/지수 codes). Numeric-bearing fields via [`ls_core::string_or_number`];
/// `#[serde(default)]` so a sparse/empty header deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1954OutBlock {
    /// Anchor date / 날짜.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Base-asset kind / 기초자산구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bsjgubun: String,
    /// Base-asset code (현물) / 기초자산코드(현물).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bscode: String,
    /// Base-asset code (지수) / 기초자산코드(지수).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bjcode: String,
}

/// `t1954OutBlock1` — one daily-price row (a representative, spec-grounded subset):
/// the date, the day OHLC (`open`/`high`/`low`/`close`), the prior-day change
/// (`sign`/`change`/`diff`), the `volume`, and the ELW analytics
/// (`parity`/`egearing`/`premium`/`gearing`/`mness`). The gateway sends OHLC as JSON
/// numbers and the analytics as strings, so every numeric-bearing field via
/// [`ls_core::string_or_number`]; `#[serde(default)]` lets a sparse row deserialize
/// and unknown fields are ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1954OutBlock1 {
    /// Date / 날짜.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub date: String,
    /// Day open / 시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
    /// Day high / 고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// Day low / 저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// Day close / 종가 — the NAMED market-data witness.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub close: String,
    /// Prior-day change sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Prior-day change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Change rate / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Parity / 패리티.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub parity: String,
    /// Effective gearing / e.기어링.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub egearing: String,
    /// Premium / 프리미엄.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub premium: String,
    /// Gearing / 기어링.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gearing: String,
    /// Moneyness / Moneyness.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mness: String,
}

/// `t1954` response — the `t1954OutBlock` header plus the `t1954OutBlock1`
/// daily-price array (tolerated single-or-array via [`ls_core::de_vec_or_single`]).
/// All `#[serde(default)]` so an empty `00707` envelope deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1954Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1954OutBlock", default)]
    pub outblock: T1954OutBlock,
    #[serde(
        rename = "t1954OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1954OutBlock1>,
}

// ---------------------------------------------------------------------------
// t1969 — ELW지표검색 (ELW screener / indicator search). market_session ELW read;
// path /stock/elw, group [주식] ELW. A MANY-field screen request (`t1969InBlock`):
// chk*/cb*/duedate*/lp_code/cbkoba are filter Strings; the numeric range fields
// (elwexec[se]/volume[se]/rate[se]/premium[se]/parity[se]/berate[se]/capt[se]/
// egearing[se]/gearing[se]/delta[se]/theta[se]) are spec `Number`s and MUST
// serialize as JSON NUMBERS via `ls_core::string_as_number` (cf. t1621's nmin/cnt
// — the string form returns IGW40011). The response NESTS: a `t1969OutBlock`
// summary (the row count `cnt`) plus a repeated `t1969OutBlock1` array (the
// screened ELW issues, tolerated single-or-array via `ls_core::de_vec_or_single`)
// — same shape as t1621. `::new()` builds the "all ELWs" screen (every chk*/cb*
// at its widest, numeric ranges 0/0, dates 000000..999999) so a no-input smoke
// returns the full board.
// ---------------------------------------------------------------------------

/// Input block for `t1969` — the ELW screener filters. The `chk*` toggles and
/// `cb*`/`duedate*`/`lp_code`/`cbkoba` codes serialize as ordinary Strings; the
/// numeric range bounds (`elwexecs`/`elwexece`/`volumes`/... ) are held as
/// `String` but wire-serialize as JSON NUMBERS via [`ls_core::string_as_number`]
/// (the string form returns `IGW40011`). See [`T1969Request::new`] for the
/// all-ELWs default screen.
#[derive(Serialize, Debug, Clone)]
pub struct T1969InBlock {
    /// Underlying-asset filter toggle / 기초자산chk구분.
    pub chkitem: String,
    /// Underlying-asset code / 기초자산코드.
    pub cbitem: String,
    /// Issuer filter toggle / 발행사chk구분.
    pub chkissuer: String,
    /// Issuer / 발행사.
    pub cbissuer: String,
    /// Call/put filter toggle / 권리chk구분.
    pub chkcallput: String,
    /// Call/put (call:01, put:02) / 권리.
    pub cbcallput: String,
    /// Strike filter toggle / 행사가chk구분.
    pub chkexec: String,
    /// Strike comparator (>=:1, <=:2) / 행사가.
    pub cbexec: String,
    /// Exercise-style filter toggle / 행사방식chk구분.
    pub chktype: String,
    /// Exercise style / 행사방식.
    pub cbtype: String,
    /// Settlement filter toggle / 결제방법chk구분.
    pub chksettle: String,
    /// Settlement method / 결제방법.
    pub cbsettle: String,
    /// Maturity filter toggle / 만기chk구분.
    pub chklast: String,
    /// Maturity month / 만기월별.
    pub cblast: String,
    /// Strike-range filter toggle / 행사가격chk구분.
    pub chkelwexec: String,
    /// Strike lower bound / 행사가이상 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub elwexecs: String,
    /// Strike upper bound / 행사가이하 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub elwexece: String,
    /// Volume filter toggle / 거래량chk구분.
    pub chkvolume: String,
    /// Volume lower bound / 거래량이상 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub volumes: String,
    /// Volume upper bound / 거래량이하 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub volumee: String,
    /// Change-rate filter toggle / 등락율chk구분.
    pub chkrate: String,
    /// Change-rate lower bound / 등락율이상 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub rates: String,
    /// Change-rate upper bound / 등락율이하 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub ratee: String,
    /// Premium filter toggle / 프리미엄chk구분.
    pub chkpremium: String,
    /// Premium lower bound / 프리미엄이상 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub premiums: String,
    /// Premium upper bound / 프리미엄이하 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub premiume: String,
    /// Parity filter toggle / 패리티chk구분.
    pub chkparity: String,
    /// Parity lower bound / 패리티이상 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub paritys: String,
    /// Parity upper bound / 패리티이하 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub paritye: String,
    /// Break-even filter toggle / 손익분기chk구분.
    pub chkberate: String,
    /// Break-even lower bound / 손익분기이상 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub berates: String,
    /// Break-even upper bound / 손익분기이하 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub beratee: String,
    /// Capital-support filter toggle / 자본지지chk구분.
    pub chkcapt: String,
    /// Capital-support lower bound / 자본지지이상 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub capts: String,
    /// Capital-support upper bound / 자본지지이하 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub capte: String,
    /// Effective-gearing filter toggle / e.기어링chk구분.
    pub chkegearing: String,
    /// Effective-gearing lower bound / e.기어링이상 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub egearings: String,
    /// Effective-gearing upper bound / e.기어링이하 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub egearinge: String,
    /// Gearing filter toggle / 기어링chk구분.
    pub chkgearing: String,
    /// Gearing lower bound / 기어링이상 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub gearings: String,
    /// Gearing upper bound / 기어링이하 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub gearinge: String,
    /// Delta filter toggle / 델타chk구분.
    pub chkdelta: String,
    /// Delta lower bound / 델타이상 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub deltas: String,
    /// Delta upper bound / 델타이하 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub deltae: String,
    /// Theta filter toggle / 쎄타chk구분.
    pub chktheta: String,
    /// Theta lower bound / 쎄타이상 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub thetas: String,
    /// Theta upper bound / 쎄타이하 (numeric request slot).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub thetae: String,
    /// Last-trade-date filter toggle / 최종거래일chk구분.
    pub chkduedate: String,
    /// Last-trade-date lower bound / 최종거래일이상 (date string, e.g. `"000000"`).
    pub duedates: String,
    /// Last-trade-date upper bound / 최종거래일이하 (date string, e.g. `"999999"`).
    pub duedatee: String,
    /// LP one-tick-gap flag / LP갭1틱.
    pub onetickgubun: String,
    /// LP liquidity-supply flag / LP유동성공급.
    pub lp_liquidity: String,
    /// LP filter toggle / LPchk구분.
    pub chklp_code: String,
    /// LP member-firm code / LP회원사코드.
    pub lp_code: String,
    /// Early-termination filter toggle / 조기종료chk구분.
    pub chkkoba: String,
    /// Early-termination (0:all, 1:KOBA, 2:non-KOBA) / 조기종료.
    pub cbkoba: String,
}

/// `t1969` request — serializes to `{"t1969InBlock":{...}}` with the numeric range
/// bounds as JSON numbers. Not paginated (`facets.self_paginated: false`).
#[derive(Serialize, Debug, Clone)]
pub struct T1969Request {
    #[serde(rename = "t1969InBlock")]
    pub inblock: T1969InBlock,
}

impl T1969Request {
    /// Build a `t1969` ELW screener request from a fully-specified filter block.
    /// For the unfiltered "all ELWs" board use [`T1969Request::new`].
    pub fn from_inblock(inblock: T1969InBlock) -> Self {
        T1969Request { inblock }
    }

    /// Build the "all ELWs" screen — every `chk*` toggle off (`"0"`), every code
    /// filter at its widest, the numeric ranges at `0`/`0`, and the date window
    /// `000000`..`999999`. No caller input; the smoke entry point.
    pub fn new() -> Self {
        T1969Request {
            inblock: T1969InBlock {
                chkitem: "0".into(),
                cbitem: "000000000000".into(),
                chkissuer: "0".into(),
                cbissuer: "000000000000".into(),
                chkcallput: "0".into(),
                cbcallput: "00".into(),
                chkexec: "0".into(),
                cbexec: "1".into(),
                chktype: "0".into(),
                cbtype: "00".into(),
                chksettle: "0".into(),
                cbsettle: "00".into(),
                chklast: "0".into(),
                cblast: "000000".into(),
                chkelwexec: "0".into(),
                elwexecs: "0".into(),
                elwexece: "0".into(),
                chkvolume: "0".into(),
                volumes: "0".into(),
                volumee: "0".into(),
                chkrate: "0".into(),
                rates: "0".into(),
                ratee: "0".into(),
                chkpremium: "0".into(),
                premiums: "0".into(),
                premiume: "0".into(),
                chkparity: "0".into(),
                paritys: "0".into(),
                paritye: "0".into(),
                chkberate: "0".into(),
                berates: "0".into(),
                beratee: "0".into(),
                chkcapt: "0".into(),
                capts: "0".into(),
                capte: "0".into(),
                chkegearing: "0".into(),
                egearings: "0".into(),
                egearinge: "0".into(),
                chkgearing: "0".into(),
                gearings: "0".into(),
                gearinge: "0".into(),
                chkdelta: "0".into(),
                deltas: "0".into(),
                deltae: "0".into(),
                chktheta: "0".into(),
                thetas: "0".into(),
                thetae: "0".into(),
                chkduedate: "0".into(),
                duedates: "000000".into(),
                duedatee: "999999".into(),
                onetickgubun: "0".into(),
                lp_liquidity: "0".into(),
                chklp_code: "0".into(),
                lp_code: "".into(),
                chkkoba: "0".into(),
                cbkoba: "".into(),
            },
        }
    }
}

impl Default for T1969Request {
    fn default() -> Self {
        T1969Request::new()
    }
}

/// `t1969OutBlock` — the screener summary header: the matched-issue count (`cnt`).
/// Modeled via [`ls_core::string_or_number`]; `#[serde(default)]` so a sparse/absent
/// header deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1969OutBlock {
    /// Matched-issue count / 종목갯수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cnt: String,
}

/// `t1969OutBlock1` — one screened ELW issue row (a representative, spec-grounded
/// subset): the issue/underlying keys, the current price + prior-day change, the
/// volume, the strike, and a few indicator columns. Every numeric-bearing field via
/// [`ls_core::string_or_number`]; `#[serde(default)]` lets a sparse row deserialize
/// and unknown fields are ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1969OutBlock1 {
    /// Issue name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Short code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Issuer / 발행사.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub issuernmk: String,
    /// Underlying-asset code / 기초자산코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemcode: String,
    /// Call/put / 콜/풋구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cpgubun: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Prior-day change sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Prior-day change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Change rate / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Strike / 행사가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub elwexec: String,
    /// Underlying-asset name / 기초자산명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub item: String,
    /// Last-trade date / 최종거래일.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lastdate: String,
    /// LP member-firm / LP회원사.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lpname: String,
}

/// `t1969` response envelope — the summary header under the `t1969OutBlock` key,
/// plus the screened ELW rows under the `t1969OutBlock1` key (tolerated
/// single-or-array via [`ls_core::de_vec_or_single`]). All `#[serde(default)]` so a
/// terse/empty envelope deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1969Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1969OutBlock", default)]
    pub outblock: T1969OutBlock,
    #[serde(
        rename = "t1969OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1969OutBlock1>,
}

// ---------------------------------------------------------------------------
// t1971 — ELW현재가호가조회 (ELW current-price + 10-level quote board). A
// market_session ELW read: ONE `t1971OutBlock` object carrying the quote (issue
// name, current price + prior-day sign/change/rate, cumulative volume), the top
// bid/offer level (호가1) + its resting quantity, the day's open/high/low, plus
// the ELW knock-out analytics (권리형태/KO베리어/접근도/발생여부). path /stock/elw,
// group [주식] ELW. 1-field request — `shcode` (a six-digit ELW issue code; these
// EXPIRE, so a live caller sources a fresh one, e.g. from t8431). All-String
// request — no numeric request slot, so no `string_as_number`.
// ---------------------------------------------------------------------------

/// Input block for `t1971` — the ELW short code (`shcode`).
///
/// `shcode` is a six-digit ELW issue code; ELW codes EXPIRE, so a live caller
/// should source a fresh one at runtime (e.g. the first `shcode` of `t8431`).
/// Ordinary request String (no numeric serialize — no numeric request slot).
#[derive(Serialize, Debug, Clone)]
pub struct T1971InBlock {
    /// ELW short code / 단축코드 — a six-digit, expiring issue code.
    pub shcode: String,
}

/// `t1971` request — serializes to `{"t1971InBlock":{"shcode":"52XXXX"}}`. Not
/// paginated (`facets.self_paginated: false`).
#[derive(Serialize, Debug, Clone)]
pub struct T1971Request {
    #[serde(rename = "t1971InBlock")]
    pub inblock: T1971InBlock,
}

impl T1971Request {
    /// Build a `t1971` ELW quote-board request for one `shcode` (a fresh,
    /// non-expired ELW issue code).
    pub fn for_shcode(shcode: impl Into<String>) -> Self {
        T1971Request {
            inblock: T1971InBlock {
                shcode: shcode.into(),
            },
        }
    }

    /// Build a `t1971` request for one `shcode`. Alias of [`T1971Request::for_shcode`]
    /// — there is no list/default form (a quote-board read needs a real issue code).
    pub fn new(shcode: impl Into<String>) -> Self {
        T1971Request::for_shcode(shcode)
    }
}

/// `t1971OutBlock` — the ELW current-price + quote-board (a representative,
/// spec-grounded subset): the issue name, the current price + prior-day
/// sign/change/rate, the cumulative volume, the top bid/offer level (호가1) and
/// its resting quantity, the day's open/high/low, and the ELW knock-out analytics
/// (권리형태/KO베리어/접근도/발생여부). Every numeric-bearing field via
/// [`ls_core::string_or_number`]; `#[serde(default)]` lets a sparse quote
/// deserialize and unknown fields are ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1971OutBlock {
    /// Issue name / 한글명.
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
    /// Top offer (ask) price / 매도호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Top bid price / 매수호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    /// Top offer resting quantity / 매도호가수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem1: String,
    /// Top bid resting quantity / 매수호가수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem1: String,
    /// Day open / 시가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
    /// Day high / 고가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    /// Day low / 저가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// ELW right type / ELW권리형태 (1:표준 2:디지털 3:조기종료).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub invidx: String,
    /// Knock-out barrier / KO베리어.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub koba_stdprc: String,
    /// Knock-out approach ratio / KO접근도.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub koba_acc_rt: String,
    /// Knock-out occurred flag / KO발생여부 (Y/N).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub koba_yn: String,
}

/// `t1971` response envelope — the single-instrument quote-board under
/// `t1971OutBlock` (ONE object; no secondary array block per the normalized
/// baseline). All `#[serde(default)]` so a terse/empty envelope deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1971Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1971OutBlock", default)]
    pub outblock: T1971OutBlock,
}

// ---------------------------------------------------------------------------
// t1972 — ELW현재가(거래원)조회 (ELW current-price + trading-member (거래원) board).
// A market_session ELW read: ONE `t1972OutBlock` object carrying the issue name +
// codes (한글명/표준코드/단축코드) and the per-member-firm sell/buy board — the
// top trading-member firm codes (매도/매수증권사코드1) with their cumulative
// volumes (총매도/총매수수량1), increments (매도/매수증감1) and ratios (매도/매수비율1),
// plus the foreign-member aggregates (외국계 매도/매수 합계 수량·비율). path
// /stock/elw, group [주식] ELW. 1-field request — `shcode` (a six-digit ELW issue
// code; these EXPIRE, so a live caller sources a fresh one, e.g. from t8431).
// All-String request — no numeric request slot, so no `string_as_number`.
// ---------------------------------------------------------------------------

/// Input block for `t1972` — the ELW short code (`shcode`).
///
/// `shcode` is a six-digit ELW issue code; ELW codes EXPIRE, so a live caller
/// should source a fresh one at runtime (e.g. the first `shcode` of `t8431`).
/// Ordinary request String (no numeric serialize — no numeric request slot).
#[derive(Serialize, Debug, Clone)]
pub struct T1972InBlock {
    /// ELW short code / 단축코드 — a six-digit, expiring issue code.
    pub shcode: String,
}

/// `t1972` request — serializes to `{"t1972InBlock":{"shcode":"52XXXX"}}`. Not
/// paginated (`facets.self_paginated: false`).
#[derive(Serialize, Debug, Clone)]
pub struct T1972Request {
    #[serde(rename = "t1972InBlock")]
    pub inblock: T1972InBlock,
}

impl T1972Request {
    /// Build a `t1972` ELW trading-member board request for one `shcode` (a fresh,
    /// non-expired ELW issue code).
    pub fn for_shcode(shcode: impl Into<String>) -> Self {
        T1972Request {
            inblock: T1972InBlock {
                shcode: shcode.into(),
            },
        }
    }

    /// Build a `t1972` request for one `shcode`. Alias of [`T1972Request::for_shcode`]
    /// — there is no list/default form (a member-board read needs a real issue code).
    pub fn new(shcode: impl Into<String>) -> Self {
        T1972Request::for_shcode(shcode)
    }
}

/// `t1972OutBlock` — the ELW current-price + trading-member (거래원) board (a
/// representative, spec-grounded subset): the issue name + codes, the top
/// trading-member firm codes (매도/매수증권사코드1), their cumulative sell/buy
/// volumes (총매도/총매수수량1), increments (매도/매수증감1) and ratios
/// (매도/매수비율1), and the foreign-member aggregates (외국계 매도/매수 합계 수량·비율).
/// Every numeric-bearing field via [`ls_core::string_or_number`] (the gateway sends
/// the ratios as strings and the volumes/increments as JSON numbers);
/// `#[serde(default)]` lets a sparse board deserialize and unknown fields are
/// ignored. Single object — no array secondary block per the normalized baseline.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1972OutBlock {
    /// Issue name / 한글명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Standard code / 표준코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Top sell trading-member firm code / 매도증권사코드1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerno1: String,
    /// Top buy trading-member firm code / 매수증권사코드1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidno1: String,
    /// Top sell-member cumulative volume / 총매도수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dvol1: String,
    /// Top buy-member cumulative volume / 총매수수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub svol1: String,
    /// Top sell-member increment / 매도증감1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub dcha1: String,
    /// Top buy-member increment / 매수증감1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub scha1: String,
    /// Top sell-member ratio / 매도비율1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ddiff1: String,
    /// Top buy-member ratio / 매수비율1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sdiff1: String,
    /// Foreign-member total sell volume / 외국계매도합계수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub fwdvl: String,
    /// Foreign-member total buy volume / 외국계매수합계수량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub fwsvl: String,
    /// Foreign-member total sell ratio / 외국계매도합계비율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub fwddiff: String,
    /// Foreign-member total buy ratio / 외국계매수합계비율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub fwsdiff: String,
}

/// `t1972` response envelope — the single trading-member board under
/// `t1972OutBlock` (ONE object; no secondary array block per the normalized
/// baseline). All `#[serde(default)]` so a terse/empty envelope deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1972Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1972OutBlock", default)]
    pub outblock: T1972OutBlock,
}

// ---------------------------------------------------------------------------
// t1974 — ELW기초자산동일종목 (ELWs sharing a base asset / 동일 기초자산 ELW 목록).
// A market_session ELW read (metadata owner_class market_session,
// self_paginated:false — NOT paginated, despite the old SDK's tr_cont scaffolding;
// metadata is the routing source of truth). Given one ELW `shcode`, it returns the
// SET of ELW issues sharing that issue's underlying base asset: a `t1974OutBlock`
// summary (the row count `cnt`) plus the `t1974OutBlock1` array — one row per sibling
// ELW carrying its short code (단축코드), name (종목명), call/put flag (콜/풋구분),
// current price (현재가), prev-day-change sign/amount (전일대비구분/전일대비), percent
// change (등락율) and volume (거래량). path /stock/elw, group [주식] ELW. 1-field
// request — `shcode` (a six-digit ELW issue code; these EXPIRE, so a live caller
// sources a fresh one, e.g. the first `shcode` of t8431). All-String request — no
// numeric request slot, so no `string_as_number`.
// ---------------------------------------------------------------------------

/// Input block for `t1974` — the ELW short code (`shcode`).
///
/// `shcode` is a six-digit ELW issue code; ELW codes EXPIRE, so a live caller should
/// source a fresh one at runtime (e.g. the first `shcode` of `t8431`). Ordinary
/// request String (no numeric serialize — no numeric request slot).
#[derive(Serialize, Debug, Clone)]
pub struct T1974InBlock {
    /// ELW short code / 단축코드 — a six-digit, expiring issue code.
    pub shcode: String,
}

/// `t1974` request — serializes to `{"t1974InBlock":{"shcode":"52XXXX"}}`. Not
/// paginated (`facets.self_paginated: false`).
#[derive(Serialize, Debug, Clone)]
pub struct T1974Request {
    #[serde(rename = "t1974InBlock")]
    pub inblock: T1974InBlock,
}

impl T1974Request {
    /// Build a `t1974` same-base-asset ELW request for one `shcode` (a fresh,
    /// non-expired ELW issue code).
    pub fn for_shcode(shcode: impl Into<String>) -> Self {
        T1974Request {
            inblock: T1974InBlock {
                shcode: shcode.into(),
            },
        }
    }

    /// Build a `t1974` request for one `shcode`. Alias of [`T1974Request::for_shcode`]
    /// — there is no list/default form (a same-base read needs a real issue code).
    pub fn new(shcode: impl Into<String>) -> Self {
        T1974Request::for_shcode(shcode)
    }
}

/// `t1974OutBlock` — the summary header: the count of sibling ELW issues (`cnt`,
/// 종목갯수). Numeric-bearing via [`ls_core::string_or_number`]; `#[serde(default)]`
/// so a sparse/empty header deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1974OutBlock {
    /// Sibling-issue count / 종목갯수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cnt: String,
}

/// `t1974OutBlock1` — one sibling ELW issue (a representative, spec-grounded subset):
/// short code (단축코드), name (종목명), call/put flag (콜/풋구분), current price
/// (현재가), prev-day-change sign/amount (전일대비구분/전일대비), percent change (등락율)
/// and volume (거래량). Every numeric-bearing field via [`ls_core::string_or_number`]
/// (the gateway sends `price`/`change` as JSON numbers and `volume`/`diff` as
/// strings); `#[serde(default)]` lets a sparse row deserialize and unknown fields are
/// ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1974OutBlock1 {
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Issue name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Call/put flag / 콜·풋구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub cpgubun: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Prev-day-change sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Prev-day change / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Percent change / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t1974` response — the `t1974OutBlock` summary header plus the `t1974OutBlock1`
/// sibling-issue array. The array tolerates a lone object or a list via
/// [`ls_core::de_vec_or_single`]; every block defaults so an empty `00707` envelope
/// deserializes.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1974Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1974OutBlock", default)]
    pub outblock: T1974OutBlock,
    #[serde(rename = "t1974OutBlock1", default, deserialize_with = "ls_core::de_vec_or_single")]
    pub outblock1: Vec<T1974OutBlock1>,
}

// ---------------------------------------------------------------------------
// t1956 — ELW현재가(확정지급액)조회 (ELW current-price / contracted-payout snapshot).
// A market_session ELW read (metadata owner_class market_session,
// self_paginated:false — NOT paginated, despite the old SDK's tr_cont scaffolding;
// metadata is the routing source of truth). Given one ELW `shcode`, it returns the
// single `t1956OutBlock` snapshot — the issue name (한글명), current price (현재가),
// percent change (등락율), accumulated volume (누적거래량), the ELW analytics
// (행사가/내재변동성/델타) and the contracted payout (확정지급액) — plus a
// `t1956OutBlock1` basket array (one row per underlying basket constituent). path
// /stock/elw, group [주식] ELW. 1-field request — `shcode` (a six-digit ELW issue
// code; these EXPIRE, so a live caller sources a fresh one, e.g. the first `shcode`
// of t8431). All-String request — no numeric request slot, so no `string_as_number`.
// ---------------------------------------------------------------------------

/// Input block for `t1956` — the ELW short code (`shcode`).
///
/// `shcode` is a six-digit ELW issue code; ELW codes EXPIRE, so a live caller should
/// source a fresh one at runtime (e.g. the first `shcode` of `t8431`). Ordinary
/// request String (no numeric serialize — no numeric request slot).
#[derive(Serialize, Debug, Clone)]
pub struct T1956InBlock {
    /// ELW short code / 단축코드 — a six-digit, expiring issue code.
    pub shcode: String,
}

/// `t1956` request — serializes to `{"t1956InBlock":{"shcode":"52XXXX"}}`. Not
/// paginated (`facets.self_paginated: false`).
#[derive(Serialize, Debug, Clone)]
pub struct T1956Request {
    #[serde(rename = "t1956InBlock")]
    pub inblock: T1956InBlock,
}

impl T1956Request {
    /// Build a `t1956` ELW current-price request for one `shcode` (a fresh,
    /// non-expired ELW issue code).
    pub fn for_shcode(shcode: impl Into<String>) -> Self {
        T1956Request {
            inblock: T1956InBlock {
                shcode: shcode.into(),
            },
        }
    }

    /// Build a `t1956` request for one `shcode`. Alias of [`T1956Request::for_shcode`]
    /// — there is no list/default form (a snapshot read needs a real issue code).
    pub fn new(shcode: impl Into<String>) -> Self {
        T1956Request::for_shcode(shcode)
    }
}

/// `t1956OutBlock` — the ELW current-price / contracted-payout snapshot (a
/// representative, spec-grounded subset): the issue name (한글명, the NAME witness),
/// current price (현재가), percent change (등락율), accumulated volume (누적거래량),
/// the strike (행사가), implied volatility (내재변동성), delta (델타), the underlying
/// base-asset code (기초자산코드) and the contracted payout (확정지급액). Every
/// numeric-bearing field via [`ls_core::string_or_number`] (the gateway sends some as
/// JSON numbers, some as strings); `#[serde(default)]` so a sparse/empty snapshot
/// deserializes and unknown fields are ignored.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1956OutBlock {
    /// Issue name / 한글명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Percent change / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Strike / 행사가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub elwexec: String,
    /// Implied volatility / 내재변동성.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub impv: String,
    /// Delta / 델타.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub delt: String,
    /// Underlying base-asset code / 기초자산코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bcode: String,
    /// Contracted payout / 확정지급액.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub givemoney: String,
}

/// `t1956OutBlock1` — one underlying basket constituent (a representative subset):
/// base-asset code (기초자산코드), basket ratio (기초자산비율), current price
/// (기초자산현재가) and volume (기초자산거래량). Numeric-bearing fields via
/// [`ls_core::string_or_number`]; `#[serde(default)]` lets a sparse row deserialize.
/// For a single-constituent basket the gateway may send a lone object, so the array
/// uses `de_vec_or_single`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1956OutBlock1 {
    /// Base-asset code / 기초자산코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bskcode: String,
    /// Basket ratio / 기초자산비율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bskbno: String,
    /// Base-asset current price / 기초자산현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bskprice: String,
    /// Base-asset volume / 기초자산거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bskvolume: String,
}

/// `t1956` response — the single `t1956OutBlock` snapshot plus the `t1956OutBlock1`
/// basket array (`de_vec_or_single`, since a single-constituent basket may arrive as
/// a lone object). `#[serde(default)]` on each block so an empty/sparse envelope
/// deserializes cleanly.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1956Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1956OutBlock", default)]
    pub outblock: T1956OutBlock,
    #[serde(rename = "t1956OutBlock1", default, deserialize_with = "ls_core::de_vec_or_single")]
    pub outblock1: Vec<T1956OutBlock1>,
}

/// Input block for `t9907` — 만기월조회 (ELW expiry-month list). No caller input.
#[derive(Serialize, Debug, Clone)]
pub struct T9907InBlock {
    /// Dummy placeholder / DUMMY (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t9907` request — serializes to `{"t9907InBlock":{"dummy":""}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T9907Request {
    #[serde(rename = "t9907InBlock")]
    pub inblock: T9907InBlock,
}
impl T9907Request {
    /// Build a `t9907` expiry-month request (no caller input).
    pub fn new() -> Self {
        T9907Request {
            inblock: T9907InBlock {
                dummy: String::new(),
            },
        }
    }
}
impl Default for T9907Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t9907OutBlock1` — one expiry-month row. All via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T9907OutBlock1 {
    /// Expiry month / 만기월 (`YYYYMM`).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lastym: String,
    /// Expiry-month name / 만기월명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub lastnm: String,
}

/// `t9907` response — expiry-month array under `t9907OutBlock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T9907Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t9907OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T9907OutBlock1>,
}

/// Input block for `t8431` — ELW종목조회 (ELW symbol list; the Wave 1 spine
/// producer for `t1958`). No caller input.
#[derive(Serialize, Debug, Clone)]
pub struct T8431InBlock {
    /// Dummy placeholder / Dummy (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t8431` request — serializes to `{"t8431InBlock":{"dummy":""}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T8431Request {
    #[serde(rename = "t8431InBlock")]
    pub inblock: T8431InBlock,
}
impl T8431Request {
    /// Build a `t8431` ELW-symbol-list request (no caller input).
    pub fn new() -> Self {
        T8431Request {
            inblock: T8431InBlock {
                dummy: String::new(),
            },
        }
    }
}
impl Default for T8431Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t8431OutBlock` — one ELW symbol row. `shcode` (단축코드) is the ELW code fed
/// to `t1958` (the comparison pair). All via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8431OutBlock {
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Short code / 단축코드 (the ELW code; `t1958` `shcode1`/`shcode2` input).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Extended code / 확장코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
    /// Reference price / 기준가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub recprice: String,
}

/// `t8431` response — ELW symbol array under `t8431OutBlock`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8431Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8431OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T8431OutBlock>,
}

/// Input block for `t9942` — ELW마스터조회API용 (ELW master list). No caller input.
#[derive(Serialize, Debug, Clone)]
pub struct T9942InBlock {
    /// Dummy placeholder / Dummy (length-1; the read takes no caller input).
    pub dummy: String,
}

/// `t9942` request — serializes to `{"t9942InBlock":{"dummy":""}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T9942Request {
    #[serde(rename = "t9942InBlock")]
    pub inblock: T9942InBlock,
}
impl T9942Request {
    /// Build a `t9942` ELW-master request (no caller input).
    pub fn new() -> Self {
        T9942Request {
            inblock: T9942InBlock {
                dummy: String::new(),
            },
        }
    }
}
impl Default for T9942Request {
    fn default() -> Self {
        Self::new()
    }
}

/// `t9942OutBlock` — one ELW master row. All via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T9942OutBlock {
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Extended code / 확장코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
}

/// `t9942` response — ELW master array under `t9942OutBlock`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T9942Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t9942OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<T9942OutBlock>,
}

/// Input block for `t1958` — ELW종목비교 (ELW symbol comparison; the Wave 1
/// comparison member). Keyed by two ELW codes (`shcode1`/`shcode2`) self-sourced
/// from `t8431` (`t8431OutBlock.shcode`) — the modeled discovery edge; never
/// fabricated.
#[derive(Serialize, Debug, Clone)]
pub struct T1958InBlock {
    /// First ELW code / 종목코드1 (from `t8431`).
    pub shcode1: String,
    /// Second ELW code / 종목코드2 (from `t8431`).
    pub shcode2: String,
}

/// `t1958` request — serializes to `{"t1958InBlock":{"shcode1":...,"shcode2":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T1958Request {
    #[serde(rename = "t1958InBlock")]
    pub inblock: T1958InBlock,
}
impl T1958Request {
    /// Build a `t1958` comparison request for two ELW codes (source both from
    /// [`T8431Response`]).
    pub fn new(shcode1: impl Into<String>, shcode2: impl Into<String>) -> Self {
        T1958Request {
            inblock: T1958InBlock {
                shcode1: shcode1.into(),
                shcode2: shcode2.into(),
            },
        }
    }
}

/// `t1958OutBlock` / `t1958OutBlock1` — one ELW symbol's detail (single object;
/// the two compared symbols). A representative subset, every field via
/// [`ls_core::string_or_number`]; `hname` is the modeled non-key signal of a
/// populated comparison.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1958Detail {
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Underlying asset / 기초자산.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub item1: String,
    /// Issuer / 발행사.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub issuernmk: String,
    /// Call/put / 콜풋구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub elwopt: String,
    /// Price / 가격.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Rate of change / 등락율.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
}

/// `t1958OutBlock2` — the comparison block (the `…cmp` fields; single object). A
/// representative subset via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1958Compare {
    /// Compared name / 종목명비교.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hnamecmp: String,
    /// Compared underlying / 기초자산비교.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub item1cmp: String,
    /// Compared price / 가격비교.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pricecmp: String,
    /// Compared volume / 거래량비교.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volumecmp: String,
    /// Compared rate of change / 등락율비교.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diffcmp: String,
}

/// `t1958` response — the first symbol (`outblock`), the second (`outblock1`),
/// and the comparison block (`outblock2`); all single objects, all
/// `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1958Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1958OutBlock", default)]
    pub outblock: T1958Detail,
    #[serde(rename = "t1958OutBlock1", default)]
    pub outblock1: T1958Detail,
    #[serde(rename = "t1958OutBlock2", default)]
    pub outblock2: T1958Compare,
}

/// Input block for `t1964` — ELW전광판 (ELW board; the Wave 1 board member).
/// `item` (기초자산코드) is the underlying-asset code self-sourced from `t9905`
/// (`t9905OutBlock1.shcode`) — the modeled discovery edge; the remaining fields
/// are broad/default filters.
#[derive(Serialize, Debug, Clone)]
pub struct T1964InBlock {
    /// Underlying-asset code / 기초자산코드 (from `t9905`).
    pub item: String,
    /// Issuer / 발행사 (broad: empty = all).
    pub issuercd: String,
    /// Expiry month / 만기월물 (broad: empty = all).
    pub lastmonth: String,
    /// Call/put / 콜풋구분 (broad: `"0"`).
    pub elwopt: String,
    /// Moneyness / 머니구분 (broad: `"0"`).
    pub atmgubun: String,
    /// Exercise type / 권리행사방식 (broad: `"0"`).
    pub elwtype: String,
    /// Settlement / 결제방법 (broad: `"0"`).
    pub settletype: String,
    /// Exercise underlying class / 행사기초자산구분 (broad: `"0"`).
    pub elwexecgubun: String,
    /// Ratio range start / 시작비율 (broad: `"0"`).
    pub fromrat: String,
    /// Ratio range end / 종료비율 (broad: `"0"`).
    pub torat: String,
    /// Volume filter / 거래량 (broad: `"0"`).
    pub volume: String,
}

/// `t1964` request — serializes to `{"t1964InBlock":{...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T1964Request {
    #[serde(rename = "t1964InBlock")]
    pub inblock: T1964InBlock,
}
impl T1964Request {
    /// Build a `t1964` board request for one underlying-asset code (source it from
    /// [`T9905Response`]) with broad/default filters for the remaining fields.
    pub fn new(item: impl Into<String>) -> Self {
        T1964Request {
            inblock: T1964InBlock {
                item: item.into(),
                issuercd: String::new(),
                lastmonth: String::new(),
                elwopt: "0".into(),
                atmgubun: "0".into(),
                elwtype: "0".into(),
                settletype: "0".into(),
                elwexecgubun: "0".into(),
                fromrat: "0".into(),
                torat: "0".into(),
                volume: "0".into(),
            },
        }
    }
}

/// `t1964OutBlock1` — one ELW board row. `shcode` (ELW코드) and `item1`
/// (기초자산코드) via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1964OutBlock1 {
    /// ELW code / ELW코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Underlying-asset code / 기초자산코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub item1: String,
    /// Underlying-asset name / 기초자산명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub itemnm: String,
    /// Issuer / 발행사.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub issuernmk: String,
}

/// `t1964` response — ELW board array under `t1964OutBlock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1964Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t1964OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1964OutBlock1>,
}

// ---------------------------------------------------------------------------
// Standalone-lane reads (reach wave U3). These carry a placeholder
// `owner_class: standalone`, but the `standalone` module is OAuth-only
// (token/revoke) and cannot host a data read — they route through
// `market_session` (non-paginated, MarketData). KTD3.
// ---------------------------------------------------------------------------

/// Input block for `t1988` — 기초자산리스트조회 (ELW underlying-asset list). A
/// filter screen: `mkt_gb` selects the market and the `chk_*` flags toggle the
/// price/volume/amount/rate conditions (`"0"` = all). `from_rate`/`to_rate` are
/// the only Number-typed request fields — they MUST serialize as JSON numbers
/// (`string_as_number`, KTD4) or the gateway rejects the call with `IGW40011`.
#[derive(Serialize, Debug, Clone)]
pub struct T1988InBlock {
    /// Market / 시장구분 (`"0"` all / `"1"` KOSPI / `"2"` KOSDAQ).
    pub mkt_gb: String,
    /// Price filter / 가격설정 (`"0"` all).
    pub chk_price: String,
    /// Price lower bound / 가격1.
    pub from_price: String,
    /// Price upper bound / 가격2.
    pub to_price: String,
    /// Volume filter / 거래량설정 (`"0"` all).
    pub chk_vol: String,
    /// Volume lower bound / 거래량1.
    pub from_vol: String,
    /// Volume upper bound / 거래량2.
    pub to_vol: String,
    /// Rate filter / 등락율설정 (`"0"` all).
    pub chk_rate: String,
    /// Rate lower bound / 등락율1 (numeric request slot, KTD4).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub from_rate: String,
    /// Rate upper bound / 등락율2 (numeric request slot, KTD4).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub to_rate: String,
    /// Amount filter / 거래대금설정 (`"0"` all).
    pub chk_amt: String,
    /// Amount lower bound / 거래대금1.
    pub from_amt: String,
    /// Amount upper bound / 거래대금2.
    pub to_amt: String,
    /// Bullish-candle filter / 양봉설정 (`"0"` all).
    pub chk_up: String,
    /// Bearish-candle filter / 음봉설정 (`"0"` all).
    pub chk_down: String,
}

/// `t1988` request — wraps the in-block under `t1988InBlock`.
#[derive(Serialize, Debug, Clone)]
pub struct T1988Request {
    #[serde(rename = "t1988InBlock")]
    pub inblock: T1988InBlock,
}
impl T1988Request {
    /// Build a `t1988` all-underlyings request: every filter off (`"0"`),
    /// numeric rate bounds `0`, blank string bounds. Returns the unfiltered
    /// underlying-asset universe for one market segment.
    pub fn new(mkt_gb: impl Into<String>) -> Self {
        T1988Request {
            inblock: T1988InBlock {
                mkt_gb: mkt_gb.into(),
                chk_price: "0".into(),
                from_price: String::new(),
                to_price: String::new(),
                chk_vol: "0".into(),
                from_vol: String::new(),
                to_vol: String::new(),
                chk_rate: "0".into(),
                from_rate: "0".into(),
                to_rate: "0".into(),
                chk_amt: "0".into(),
                from_amt: String::new(),
                to_amt: String::new(),
                chk_up: "0".into(),
                chk_down: "0".into(),
            },
        }
    }
}
impl Default for T1988Request {
    fn default() -> Self {
        Self::new("0")
    }
}

/// `t1988OutBlock1` — one underlying-asset row (the Object-Array detail block).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1988OutBlock1 {
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Standard code / 표준코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
    /// Issue name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 부호.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Volume / 누적거래량(주).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
}

/// `t1988OutBlock` — summary header: KOSPI/KOSDAQ counts plus the per-asset row
/// array under `t1988OutBlock1` (single-or-array via [`ls_core::de_vec_or_single`]).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T1988OutBlock {
    /// KOSPI issue count / 코스피종목건수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ksp_cnt: String,
    /// KOSDAQ issue count / 코스닥종목건수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub ksd_cnt: String,
}

/// `t1988` response — summary `t1988OutBlock` + the per-asset array
/// `t1988OutBlock1`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T1988Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t1988OutBlock", default)]
    pub outblock: T1988OutBlock,
    #[serde(
        rename = "t1988OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T1988OutBlock1>,
}
