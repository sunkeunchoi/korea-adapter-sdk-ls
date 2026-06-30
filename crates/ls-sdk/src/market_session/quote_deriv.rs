//! Futures-option and overseas current-price / order-book quote reads.
//!
//! Wave-1 split out of `market_session/mod.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use quote_deriv::*;` so every `ls_sdk::market_session::*` path is unchanged.
use super::*;


// ---------------------------------------------------------------------------
// U5 (reach wave) — F/O quote/master reads. All `/futureoption/market-data`,
// `[선물/옵션] 시세`, non-paginated market_session. Out-block keys + array-ness
// read from the RAW capture (KTD5): t2111/t2112/t8402/t8403 carry a SINGLE
// out-block; t2106 carries a single summary + an ARRAY detail block; t8434
// carries an ARRAY out-block (`t8434OutBlock1`). t8434's `qrycnt` is a numeric
// REQUEST field serialized as a JSON number (`string_as_number`, KTD4).
// ---------------------------------------------------------------------------

/// Input block for `t2111` — 선물/옵션현재가(시세)조회 (F/O current-price quote).
///
/// `focode` is the futures/option contract short code (단축코드), a
/// caller-supplied identifier sourced from an F/O master (e.g.
/// [`MarketSession::index_futures_master`]'s `shcode`).
#[derive(Serialize, Debug, Clone)]
pub struct T2111InBlock {
    /// Short code / 단축코드 (F/O contract code).
    pub focode: String,
}

/// `t2111` request — serializes to `{"t2111InBlock":{"focode":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T2111Request {
    #[serde(rename = "t2111InBlock")]
    pub inblock: T2111InBlock,
}
impl T2111Request {
    /// Build a `t2111` F/O current-price request for one contract code.
    pub fn new(focode: impl Into<String>) -> Self {
        T2111Request {
            inblock: T2111InBlock {
                focode: focode.into(),
            },
        }
    }
}

/// `t2111OutBlock` — the F/O current-price snapshot (single object).
///
/// A representative, spec-grounded subset of the `t2111OutBlock`; every
/// numeric-bearing field uses [`ls_core::string_or_number`]. `pricejisu`
/// (종합지수) and `kospijisu` (KOSPI200지수) are modeled as DISTINCT index fields
/// (not collapsed) so a fixture can pin each separately (KTD6). All
/// `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2111OutBlock {
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
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Open interest / 미결제량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mgjv: String,
    /// Composite index / 종합지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub pricejisu: String,
    /// KOSPI200 index / KOSPI200지수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub kospijisu: String,
    /// Contract code / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub focode: String,
}

/// `t2111` response envelope. `outblock` is the snapshot under the
/// `t2111OutBlock` key (single object). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T2111Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t2111OutBlock", default)]
    pub outblock: T2111OutBlock,
}

/// Input block for `t2112` — 선물/옵션현재가호가조회 (F/O current-price order book).
///
/// `shcode` is the F/O contract short code (단축코드), a caller-supplied
/// identifier sourced from an F/O master.
#[derive(Serialize, Debug, Clone)]
pub struct T2112InBlock {
    /// Short code / 단축코드 (F/O contract code).
    pub shcode: String,
}

/// `t2112` request — serializes to `{"t2112InBlock":{"shcode":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T2112Request {
    #[serde(rename = "t2112InBlock")]
    pub inblock: T2112InBlock,
}
impl T2112Request {
    /// Build a `t2112` F/O order-book request for one contract code.
    pub fn new(shcode: impl Into<String>) -> Self {
        T2112Request {
            inblock: T2112InBlock {
                shcode: shcode.into(),
            },
        }
    }
}

/// `t2112OutBlock` — the F/O current-price + 5-level order book (single object).
///
/// A representative subset of the `t2112OutBlock`: the price header plus the
/// level-1 bid/offer aggregates. Every numeric-bearing field uses
/// [`ls_core::string_or_number`]; all `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2112OutBlock {
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Best offer (ask) / 매도호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Best bid / 매수호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    /// Best offer quantity / 매도호가수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem1: String,
    /// Best bid quantity / 매수호가수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem1: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
}

/// `t2112` response envelope. `outblock` is the order book under the
/// `t2112OutBlock` key (single object). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T2112Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t2112OutBlock", default)]
    pub outblock: T2112OutBlock,
}

/// Input block for `t2106` — 선물/옵션현재가시세메모 (F/O price-memo read).
///
/// `code` is the F/O contract code (종목코드); `nrec` is the requested memo
/// count (건수). The spec's `t2106InBlock` carries `code` + `nrec`; the optional
/// `t2106InBlock1` condition array is not modeled (the read is keyed by `code`).
#[derive(Serialize, Debug, Clone)]
pub struct T2106InBlock {
    /// Contract code / 종목코드 (F/O contract code).
    pub code: String,
    /// Requested count / 건수 (empty = default).
    pub nrec: String,
}

/// `t2106` request — serializes to `{"t2106InBlock":{"code":...,"nrec":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T2106Request {
    #[serde(rename = "t2106InBlock")]
    pub inblock: T2106InBlock,
}
impl T2106Request {
    /// Build a `t2106` price-memo request for one contract code (`nrec` defaults
    /// to empty — the gateway returns the default memo set).
    pub fn new(code: impl Into<String>) -> Self {
        T2106Request {
            inblock: T2106InBlock {
                code: code.into(),
                nrec: String::new(),
            },
        }
    }
}

/// `t2106OutBlock` — the price-memo summary block (single object).
///
/// `nrec` (출력건수) is the modeled non-key signal. Via
/// [`ls_core::string_or_number`]; `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2106OutBlock {
    /// Output count / 출력건수.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub nrec: String,
}

/// `t2106OutBlock1` — one price-memo row (`t2106OutBlock1[]`, an ARRAY block).
///
/// The repeated detail block (the spec marks `t2106OutBlock1` an array); each
/// row is index/condition/value. Every field via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T2106OutBlock1 {
    /// Index / 인덱스.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub indx: String,
    /// Condition distinction / 조건구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub gubn: String,
    /// Output value / 출력값.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub vals: String,
}

/// `t2106` response envelope.
///
/// `outblock` is the memo summary; `outblock1` is the memo-row array under the
/// `t2106OutBlock1` key, tolerated as single-or-array via
/// [`ls_core::de_vec_or_single`]. All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T2106Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t2106OutBlock", default)]
    pub outblock: T2106OutBlock,
    #[serde(
        rename = "t2106OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T2106OutBlock1>,
}

/// Input block for `t8402` — 주식선물현재가조회(API용) (stock-futures current price).
///
/// `focode` is the stock-futures contract short code (단축코드), a
/// caller-supplied identifier sourced from the stock-futures master
/// ([`MarketSession::stock_futures_master`]'s `shcode`).
#[derive(Serialize, Debug, Clone)]
pub struct T8402InBlock {
    /// Short code / 단축코드 (stock-futures contract code).
    pub focode: String,
}

/// `t8402` request — serializes to `{"t8402InBlock":{"focode":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T8402Request {
    #[serde(rename = "t8402InBlock")]
    pub inblock: T8402InBlock,
}
impl T8402Request {
    /// Build a `t8402` stock-futures current-price request for one contract code.
    pub fn new(focode: impl Into<String>) -> Self {
        T8402Request {
            inblock: T8402InBlock {
                focode: focode.into(),
            },
        }
    }
}

/// `t8402OutBlock` — the stock-futures current-price snapshot (single object).
///
/// A representative subset; every numeric field via
/// [`ls_core::string_or_number`]. `basehname` (기초자산한글명) is a DISTINCT
/// underlying-name string modeled separately from the futures `hname` so a
/// fixture can pin each (KTD6). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8402OutBlock {
    /// Korean name / 한글명 (the stock-futures contract name).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Open interest / 미결제량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub mgjv: String,
    /// Underlying short code / 기초자산단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
    /// Underlying Korean name / 기초자산한글명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub basehname: String,
    /// Underlying current price / 기초자산현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub baseprice: String,
}

/// `t8402` response envelope. `outblock` is the snapshot under the
/// `t8402OutBlock` key (single object). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8402Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t8402OutBlock", default)]
    pub outblock: T8402OutBlock,
}

/// Input block for `t8403` — 주식선물호가조회(API용) (stock-futures order book).
///
/// `shcode` is the stock-futures contract short code (단축코드), a
/// caller-supplied identifier sourced from the stock-futures master.
#[derive(Serialize, Debug, Clone)]
pub struct T8403InBlock {
    /// Short code / 단축코드 (stock-futures contract code).
    pub shcode: String,
}

/// `t8403` request — serializes to `{"t8403InBlock":{"shcode":...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T8403Request {
    #[serde(rename = "t8403InBlock")]
    pub inblock: T8403InBlock,
}
impl T8403Request {
    /// Build a `t8403` stock-futures order-book request for one contract code.
    pub fn new(shcode: impl Into<String>) -> Self {
        T8403Request {
            inblock: T8403InBlock {
                shcode: shcode.into(),
            },
        }
    }
}

/// `t8403OutBlock` — the stock-futures current-price + 10-level order book
/// (single object).
///
/// A representative subset: the price header plus the level-1 bid/offer
/// aggregates. Every numeric-bearing field via [`ls_core::string_or_number`];
/// all `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8403OutBlock {
    /// Korean name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub hname: String,
    /// Current price / 현재가.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Volume / 거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Best offer (ask) / 매도호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Best bid / 매수호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    /// Best offer quantity / 매도호가수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerrem1: String,
    /// Best bid quantity / 매수호가수량1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidrem1: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub shcode: String,
}

/// `t8403` response envelope. `outblock` is the order book under the
/// `t8403OutBlock` key (single object). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8403Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t8403OutBlock", default)]
    pub outblock: T8403OutBlock,
}

/// Input block for `t8434` — 선물/옵션멀티현재가조회 (F/O multi current-price).
///
/// `qrycnt` is the requested contract COUNT (건수), a numeric REQUEST field
/// serialized as a JSON number via [`ls_core::string_as_number`] (KTD4 — the
/// string form risks `IGW40011`). `focode` is a comma-joined list of F/O
/// contract codes (단축코드, up to length 400).
#[derive(Serialize, Debug, Clone)]
pub struct T8434InBlock {
    /// Requested count / 건수 (serialized as a JSON number).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub qrycnt: String,
    /// Short code(s) / 단축코드 (one or more F/O contract codes).
    pub focode: String,
}

/// `t8434` request — serializes to `{"t8434InBlock":{"qrycnt":1,"focode":...}}`
/// (`qrycnt` as a JSON number).
#[derive(Serialize, Debug, Clone)]
pub struct T8434Request {
    #[serde(rename = "t8434InBlock")]
    pub inblock: T8434InBlock,
}
impl T8434Request {
    /// Build a `t8434` multi current-price request for `qrycnt` contracts keyed
    /// by `focode` (a single code or a comma-joined list).
    pub fn new(qrycnt: impl Into<String>, focode: impl Into<String>) -> Self {
        T8434Request {
            inblock: T8434InBlock {
                qrycnt: qrycnt.into(),
                focode: focode.into(),
            },
        }
    }
}

/// `t8434OutBlock1` — one F/O current-price row (`t8434OutBlock1[]`, an ARRAY
/// block).
///
/// The multi-quote response is a repeated row array (the spec marks
/// `t8434OutBlock1` an array). Every numeric-bearing field via
/// [`ls_core::string_or_number`]; `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8434OutBlock1 {
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
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Short code / 단축코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub focode: String,
}

/// `t8434` response envelope.
///
/// `outblock1` is the multi-quote row array under the `t8434OutBlock1` key,
/// tolerated as single-or-array via [`ls_core::de_vec_or_single`]. All
/// `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8434Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t8434OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T8434OutBlock1>,
}

// ---------------------------------------------------------------------------
// Overseas-stock reads (reach wave U7). Domain `overseas_stock` (`g`-prefix),
// path `/overseas-stock/{market-data,chart}`. Non-paginated market-data reads
// keyed by an exchange code + symbol (e.g. `82`/`TSLA`). `venue_session:
// unspecified` (uncharted). Out-block keys/array-ness from the raw capture
// (KTD5); canonical price/name field by `korean_name` from non-collapsing
// fixtures (KTD6). Numeric request counts serialize as JSON numbers (KTD4).
// ---------------------------------------------------------------------------

/// Input block for `g3101` — 해외주식 현재가 조회 (overseas current-price). Keyed by
/// an exchange code (`exchcd`, e.g. `"82"` = NASDAQ) + `symbol` plus the
/// composite `keysymbol` (= exchcd+symbol). `delaygb` is the realtime/delayed
/// distinction (`"R"` = realtime).
#[derive(Serialize, Debug, Clone)]
pub struct G3101InBlock {
    /// Realtime/delayed distinction / 지연구분 (`"R"` = realtime).
    pub delaygb: String,
    /// Composite key / KEY종목코드 (`exchcd` + `symbol`).
    pub keysymbol: String,
    /// Exchange code / 거래소코드.
    pub exchcd: String,
    /// Symbol / 종목코드.
    pub symbol: String,
}

/// `g3101` request — serializes to `{"g3101InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct G3101Request {
    #[serde(rename = "g3101InBlock")]
    pub inblock: G3101InBlock,
}
impl G3101Request {
    /// Build a `g3101` current-price request for one overseas symbol.
    pub fn new(
        delaygb: impl Into<String>,
        keysymbol: impl Into<String>,
        exchcd: impl Into<String>,
        symbol: impl Into<String>,
    ) -> Self {
        G3101Request {
            inblock: G3101InBlock {
                delaygb: delaygb.into(),
                keysymbol: keysymbol.into(),
                exchcd: exchcd.into(),
                symbol: symbol.into(),
            },
        }
    }
}

/// `g3101OutBlock` — the overseas current-price snapshot (single object).
///
/// A representative subset; every numeric-bearing field via
/// [`ls_core::string_or_number`]. `price` (현재가) is the canonical price field
/// (KTD6).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct G3101OutBlock {
    /// Korean name / 한글종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub korname: String,
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Current price / 현재가 (canonical field, KTD6).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
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
    /// Currency / 통화.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub currency: String,
}

/// `g3101` response envelope. `outblock` is the snapshot under the
/// `g3101OutBlock` key (single object). All `#[serde(default)]`.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct G3101Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "g3101OutBlock", default)]
    pub outblock: G3101OutBlock,
}

/// Input block for `g3106` — 해외주식 현재가호가 조회 (overseas current-price +
/// order book). Same key shape as `g3101`.
#[derive(Serialize, Debug, Clone)]
pub struct G3106InBlock {
    /// Realtime/delayed distinction / 지연구분.
    pub delaygb: String,
    /// Composite key / KEY종목코드.
    pub keysymbol: String,
    /// Exchange code / 거래소코드.
    pub exchcd: String,
    /// Symbol / 종목코드.
    pub symbol: String,
}

/// `g3106` request — serializes to `{"g3106InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct G3106Request {
    #[serde(rename = "g3106InBlock")]
    pub inblock: G3106InBlock,
}
impl G3106Request {
    /// Build a `g3106` current-price+order-book request for one overseas symbol.
    pub fn new(
        delaygb: impl Into<String>,
        keysymbol: impl Into<String>,
        exchcd: impl Into<String>,
        symbol: impl Into<String>,
    ) -> Self {
        G3106Request {
            inblock: G3106InBlock {
                delaygb: delaygb.into(),
                keysymbol: keysymbol.into(),
                exchcd: exchcd.into(),
                symbol: symbol.into(),
            },
        }
    }
}

/// `g3106OutBlock` — the overseas current-price + level-1 order book (single
/// object).
///
/// `price` (현재가) is the canonical price field (KTD6). Every numeric-bearing
/// field via [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct G3106OutBlock {
    /// Korean name / 한글종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub korname: String,
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Current price / 현재가 (canonical field, KTD6).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Sign / 전일대비구분.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Best offer (ask) price / 매도호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Best bid price / 매수호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
}

/// `g3106` response envelope (single out-block).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct G3106Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "g3106OutBlock", default)]
    pub outblock: G3106OutBlock,
}

/// Input block for `o3105` — 해외선물 현재가(종목정보) 조회 (overseas-futures
/// current price / symbol info). Keyed by one `symbol`.
#[derive(Serialize, Debug, Clone)]
pub struct O3105InBlock {
    /// Symbol / 종목심볼.
    pub symbol: String,
}

/// `o3105` request — serializes to `{"o3105InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct O3105Request {
    #[serde(rename = "o3105InBlock")]
    pub inblock: O3105InBlock,
}
impl O3105Request {
    /// Build an `o3105` symbol-info request for one overseas-futures symbol.
    pub fn new(symbol: impl Into<String>) -> Self {
        O3105Request {
            inblock: O3105InBlock {
                symbol: symbol.into(),
            },
        }
    }
}

/// `o3105OutBlock` — the overseas-futures current-price snapshot (single object
/// per the raw capture, KTD5). `trd_p` (체결가격) is the canonical price field
/// (KTD6); `tot_q`/`trd_q`/`seq_no`/`dot_gb` are numeric. Rust fields are
/// snake_case with `#[serde(rename)]` to the PascalCase wire keys.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3105OutBlock {
    /// Symbol / 종목코드.
    #[serde(rename = "Symbol", deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Symbol name / 종목명.
    #[serde(rename = "SymbolNm", deserialize_with = "ls_core::string_or_number")]
    pub symbol_nm: String,
    /// Trade price / 체결가격 (canonical field, KTD6).
    #[serde(rename = "TrdP", deserialize_with = "ls_core::string_or_number")]
    pub trd_p: String,
    /// Open / 시가.
    #[serde(rename = "OpenP", deserialize_with = "ls_core::string_or_number")]
    pub open_p: String,
    /// High / 고가.
    #[serde(rename = "HighP", deserialize_with = "ls_core::string_or_number")]
    pub high_p: String,
    /// Low / 저가.
    #[serde(rename = "LowP", deserialize_with = "ls_core::string_or_number")]
    pub low_p: String,
    /// Total volume / 누적거래량 (numeric out field).
    #[serde(rename = "TotQ", deserialize_with = "ls_core::string_or_number")]
    pub tot_q: String,
    /// Trade quantity / 체결수량 (numeric out field).
    #[serde(rename = "TrdQ", deserialize_with = "ls_core::string_or_number")]
    pub trd_q: String,
    /// Sequence number / 수신순번 (numeric out field).
    #[serde(rename = "SeqNo", deserialize_with = "ls_core::string_or_number")]
    pub seq_no: String,
    /// Currency / 통화코드.
    #[serde(rename = "CrncyCd", deserialize_with = "ls_core::string_or_number")]
    pub crncy_cd: String,
}

/// `o3105` response envelope. Single out-block under the `o3105OutBlock` key.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3105Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "o3105OutBlock", default)]
    pub outblock: O3105OutBlock,
}

/// Input block for `o3106` — 해외선물 현재가호가 조회 (overseas-futures current
/// price + order book). Keyed by one `symbol`.
#[derive(Serialize, Debug, Clone)]
pub struct O3106InBlock {
    /// Symbol / 종목심볼.
    pub symbol: String,
}

/// `o3106` request — serializes to `{"o3106InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct O3106Request {
    #[serde(rename = "o3106InBlock")]
    pub inblock: O3106InBlock,
}
impl O3106Request {
    /// Build an `o3106` order-book request for one overseas-futures symbol.
    pub fn new(symbol: impl Into<String>) -> Self {
        O3106Request {
            inblock: O3106InBlock {
                symbol: symbol.into(),
            },
        }
    }
}

/// `o3106OutBlock` — the overseas-futures current-price + order-book snapshot
/// (single object per the raw capture, KTD5). `price` (현재가) is the canonical
/// price field (KTD6); the level-1 book + counts are numeric.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3106OutBlock {
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Symbol name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbolname: String,
    /// Current price / 현재가 (canonical field, KTD6).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Best ask price / 매도호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Best bid price / 매수호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    /// Total ask volume / 매도호가총잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offer: String,
    /// Total bid volume / 매수호가총잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bid: String,
}

/// `o3106` response envelope. Single out-block under the `o3106OutBlock` key.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3106Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "o3106OutBlock", default)]
    pub outblock: O3106OutBlock,
}

/// Input block for `o3125` — 해외선물옵션 현재가(종목정보) 조회 (overseas
/// future-option current price / symbol info). Keyed by `mktgb` + `symbol`.
#[derive(Serialize, Debug, Clone)]
pub struct O3125InBlock {
    /// Market distinction / 시장구분 (`"F"` = future, `"O"` = option).
    pub mktgb: String,
    /// Symbol / 종목심볼.
    pub symbol: String,
}

/// `o3125` request — serializes to `{"o3125InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct O3125Request {
    #[serde(rename = "o3125InBlock")]
    pub inblock: O3125InBlock,
}
impl O3125Request {
    /// Build an `o3125` symbol-info request for one market + symbol.
    pub fn new(mktgb: impl Into<String>, symbol: impl Into<String>) -> Self {
        O3125Request {
            inblock: O3125InBlock {
                mktgb: mktgb.into(),
                symbol: symbol.into(),
            },
        }
    }
}

/// `o3125OutBlock` — the overseas-future-option current-price snapshot (single
/// object per the raw capture, KTD5). `trd_p` (체결가격) is the canonical price
/// field (KTD6); `tot_q`/`trd_q`/`seq_no`/`dot_gb` are numeric. Rust fields are
/// snake_case with `#[serde(rename)]` to the PascalCase wire keys.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3125OutBlock {
    /// Symbol / 종목코드.
    #[serde(rename = "Symbol", deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Symbol name / 종목명.
    #[serde(rename = "SymbolNm", deserialize_with = "ls_core::string_or_number")]
    pub symbol_nm: String,
    /// Trade price / 체결가격 (canonical field, KTD6).
    #[serde(rename = "TrdP", deserialize_with = "ls_core::string_or_number")]
    pub trd_p: String,
    /// Open / 시가.
    #[serde(rename = "OpenP", deserialize_with = "ls_core::string_or_number")]
    pub open_p: String,
    /// High / 고가.
    #[serde(rename = "HighP", deserialize_with = "ls_core::string_or_number")]
    pub high_p: String,
    /// Low / 저가.
    #[serde(rename = "LowP", deserialize_with = "ls_core::string_or_number")]
    pub low_p: String,
    /// Total volume / 누적거래량 (numeric out field).
    #[serde(rename = "TotQ", deserialize_with = "ls_core::string_or_number")]
    pub tot_q: String,
    /// Trade quantity / 체결수량 (numeric out field).
    #[serde(rename = "TrdQ", deserialize_with = "ls_core::string_or_number")]
    pub trd_q: String,
    /// Sequence number / 수신순번 (numeric out field).
    #[serde(rename = "SeqNo", deserialize_with = "ls_core::string_or_number")]
    pub seq_no: String,
    /// Currency / 통화코드.
    #[serde(rename = "CrncyCd", deserialize_with = "ls_core::string_or_number")]
    pub crncy_cd: String,
}

/// `o3125` response envelope. Single out-block under the `o3125OutBlock` key.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3125Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "o3125OutBlock", default)]
    pub outblock: O3125OutBlock,
}

/// Input block for `o3126` — 해외선물옵션 현재가호가 조회 (overseas future-option
/// current price + order book). Keyed by `mktgb` + `symbol`.
#[derive(Serialize, Debug, Clone)]
pub struct O3126InBlock {
    /// Market distinction / 시장구분 (`"F"` = future, `"O"` = option).
    pub mktgb: String,
    /// Symbol / 종목심볼.
    pub symbol: String,
}

/// `o3126` request — serializes to `{"o3126InBlock":{...}}`. Non-paginated.
#[derive(Serialize, Debug, Clone)]
pub struct O3126Request {
    #[serde(rename = "o3126InBlock")]
    pub inblock: O3126InBlock,
}
impl O3126Request {
    /// Build an `o3126` order-book request for one market + symbol.
    pub fn new(mktgb: impl Into<String>, symbol: impl Into<String>) -> Self {
        O3126Request {
            inblock: O3126InBlock {
                mktgb: mktgb.into(),
                symbol: symbol.into(),
            },
        }
    }
}

/// `o3126OutBlock` — the overseas-future-option current-price + order-book
/// snapshot (single object per the raw capture, KTD5). `price` (현재가) is the
/// canonical price field (KTD6); the level-1 book + counts are numeric.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3126OutBlock {
    /// Symbol / 종목코드.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    /// Symbol name / 종목명.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbolname: String,
    /// Current price / 현재가 (canonical field, KTD6).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Change vs. previous close / 전일대비.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    /// Accumulated volume / 누적거래량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    /// Best ask price / 매도호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offerho1: String,
    /// Best bid price / 매수호가1.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bidho1: String,
    /// Total ask volume / 매도호가총잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub offer: String,
    /// Total bid volume / 매수호가총잔량.
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub bid: String,
}

/// `o3126` response envelope. Single out-block under the `o3126OutBlock` key.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3126Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "o3126OutBlock", default)]
    pub outblock: O3126OutBlock,
}

// ---------------------------------------------------------------------------
// o3127 — 해외선물옵션 관심종목 조회 (overseas-futopt watchlist board). Non-paginated
// market-data read keyed by `nrec` (genuinely-numeric record count); an
// `o3127OutBlock[]` board array (de_vec_or_single). Plan -003.
// ---------------------------------------------------------------------------

/// Input block for `o3127` — overseas-futopt watchlist board. `nrec` is the
/// genuinely-numeric record count (JSON number; IGW40011 guard).
#[derive(Serialize, Debug, Clone)]
pub struct O3127InBlock {
    /// Record count / 건수 (genuinely numeric).
    #[serde(serialize_with = "ls_core::string_as_number")]
    pub nrec: String,
}

/// Repeated request sub-block `o3127InBlock1` (Occurs) — one watch entry per
/// requested symbol. The gateway returns a per-symbol quote row only when the
/// symbol is supplied here; an `nrec`-only request returns placeholder rows with
/// a zero `price` (not a real quote).
#[derive(Serialize, Debug, Clone)]
pub struct O3127InBlock1 {
    /// Market distinction / 기본입력 (e.g. `"0"`).
    pub mktgb: String,
    /// Symbol / 종목심볼 (e.g. `"CUSN26"`).
    pub symbol: String,
}

/// `o3127` request — serializes to `{"o3127InBlock":{...},"o3127InBlock1":[...]}`.
/// Non-paginated. The repeated `o3127InBlock1` carries the watched symbols.
#[derive(Serialize, Debug, Clone)]
pub struct O3127Request {
    #[serde(rename = "o3127InBlock")]
    pub inblock: O3127InBlock,
    #[serde(rename = "o3127InBlock1")]
    pub inblock1: Vec<O3127InBlock1>,
}
impl O3127Request {
    /// Build an `o3127` watchlist-board request for one watched `symbol` under
    /// `mktgb`; `nrec` is set to match the single supplied entry.
    pub fn new(mktgb: impl Into<String>, symbol: impl Into<String>) -> Self {
        O3127Request {
            inblock: O3127InBlock {
                nrec: "1".to_string(),
            },
            inblock1: vec![O3127InBlock1 {
                mktgb: mktgb.into(),
                symbol: symbol.into(),
            }],
        }
    }
}

/// `o3127OutBlock` — one watchlist-board row.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct O3127OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbol: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub symbolname: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub change: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub sign: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub diff: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub volume: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub high: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub low: String,
    /// Price / 현재가 (the substantive witness).
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    #[serde(deserialize_with = "ls_core::string_or_number")]
    pub open: String,
}

/// `o3127` response — board rows under `o3127OutBlock` (single-or-array).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct O3127Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "o3127OutBlock",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock: Vec<O3127OutBlock>,
}
