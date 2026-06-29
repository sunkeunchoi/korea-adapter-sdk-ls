//! Orderable-quantity, margin-rate and loanable-collateral capacity reads.
//!
//! Wave-2b split out of `account/mod.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use capacity::*;` so every `ls_sdk::account::*` path is unchanged.
use super::*;


// ---------------------------------------------------------------------------
// CFOAQ10100 — 선물옵션 주문가능수량조회 (F/O orderable-quantity inquiry, read-only).
//
// A read-only account-state read returning the orderable quantity for a given
// F/O instrument + order parameters. This is an INQUIRY (조회), NOT an order
// mutation — it places nothing. Account-identity discipline as the CSPAQ family:
// the account number comes from `ResolvedConfig.account_no` and the bearer token,
// NEVER a caller field. The in-block carries caller-supplied order-shape inputs
// (incl. `FnoIsuNo`) — the numeric fields `RecCnt`/`OrdAmt`/`RatVal`/`FnoOrdPrc`
// serialize as JSON numbers (`string_as_number`, KTD4). Single-page → `Inner::post`.
//
// Out-block shape (raw capture, KTD5): OutBlock1 (echo / identity, single),
// OutBlock2 (orderable-quantity result, single object → Vec via
// `de_vec_or_single`). Canonical field (KTD6): `OrdAbleQty` (주문가능수량).
// ---------------------------------------------------------------------------

/// Input block for `CFOAQ10100` — the order-shape inputs for an orderable-quantity
/// inquiry.
///
/// Per the normalized baseline + raw capture, `CFOAQ10100InBlock1` carries seven
/// fields. The numeric ones (`RecCnt`, `OrdAmt`, `RatVal`, `FnoOrdPrc`) serialize
/// as JSON numbers (KTD4); the rest are short enum/code strings. It holds NO
/// account number — the account is the bearer token plus
/// `ResolvedConfig.account_no`.
#[derive(Serialize, Debug, Clone)]
pub struct CFOAQ10100InBlock1 {
    /// Record count / 레코드갯수 (JSON number).
    #[serde(rename = "RecCnt", serialize_with = "ls_core::string_as_number")]
    pub reccnt: String,
    /// Query distinction / 조회구분.
    #[serde(rename = "QryTp")]
    pub qrytp: String,
    /// Order amount / 주문금액 (JSON number).
    #[serde(rename = "OrdAmt", serialize_with = "ls_core::string_as_number")]
    pub ordamt: String,
    /// Ratio value / 비율값 (JSON number).
    #[serde(rename = "RatVal", serialize_with = "ls_core::string_as_number")]
    pub ratval: String,
    /// Futures/option issue number / 선물옵션종목번호.
    #[serde(rename = "FnoIsuNo")]
    pub fnoisuno: String,
    /// Buy/sell distinction / 매매구분.
    #[serde(rename = "BnsTpCode")]
    pub bnstpcode: String,
    /// Futures/option order price / 선물옵션주문가격 (JSON number).
    #[serde(rename = "FnoOrdPrc", serialize_with = "ls_core::string_as_number")]
    pub fnoordprc: String,
    /// Futures/option order-price-pattern code / 선물옵션호가유형코드.
    #[serde(rename = "FnoOrdprcPtnCode")]
    pub fnoordprcptncode: String,
}

/// `CFOAQ10100` request — wraps the input block under the `CFOAQ10100InBlock1` key.
///
/// Serializes the four numeric fields as JSON numbers (KTD4). No account number
/// ever appears in the body (single-page read; no continuation token).
#[derive(Serialize, Debug, Clone)]
pub struct CFOAQ10100Request {
    #[serde(rename = "CFOAQ10100InBlock1")]
    pub inblock: CFOAQ10100InBlock1,
}

impl CFOAQ10100Request {
    /// Build a `CFOAQ10100` orderable-quantity inquiry from the order-shape inputs.
    ///
    /// This is a read-only inquiry (조회), not an order. The account number is NOT a
    /// parameter: it is established by the credentialed token and the
    /// config-supplied `ResolvedConfig.account_no`, never by the caller.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        reccnt: impl Into<String>,
        qrytp: impl Into<String>,
        ordamt: impl Into<String>,
        ratval: impl Into<String>,
        fnoisuno: impl Into<String>,
        bnstpcode: impl Into<String>,
        fnoordprc: impl Into<String>,
        fnoordprcptncode: impl Into<String>,
    ) -> Self {
        CFOAQ10100Request {
            inblock: CFOAQ10100InBlock1 {
                reccnt: reccnt.into(),
                qrytp: qrytp.into(),
                ordamt: ordamt.into(),
                ratval: ratval.into(),
                fnoisuno: fnoisuno.into(),
                bnstpcode: bnstpcode.into(),
                fnoordprc: fnoordprc.into(),
                fnoordprcptncode: fnoordprcptncode.into(),
            },
        }
    }
}

/// `CFOAQ10100OutBlock1` — the echo / account-identity summary block.
///
/// `AcntNo`/`Pwd` are account-sensitive, so [`std::fmt::Debug`] is hand-written to
/// redact them.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CFOAQ10100OutBlock1 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Account number / 계좌번호 (account-sensitive; redacted in Debug).
    #[serde(rename = "AcntNo", deserialize_with = "ls_core::string_or_number")]
    pub acntno: String,
    /// Password / 비밀번호 (account-sensitive; redacted in Debug).
    #[serde(rename = "Pwd", deserialize_with = "ls_core::string_or_number")]
    pub pwd: String,
    /// Futures/option issue number / 선물옵션종목번호 (echoes the request).
    #[serde(rename = "FnoIsuNo", deserialize_with = "ls_core::string_or_number")]
    pub fnoisuno: String,
}

impl std::fmt::Debug for CFOAQ10100OutBlock1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CFOAQ10100OutBlock1")
            .field("reccnt", &self.reccnt)
            .field("acntno", &"<redacted>")
            .field("pwd", &"<redacted>")
            .field("fnoisuno", &self.fnoisuno)
            .finish()
    }
}

/// `CFOAQ10100OutBlock2` — the orderable-quantity result block.
///
/// A representative, spec-grounded subset of the LS `CFOAQ10100OutBlock2`. The
/// canonical field (KTD6) is `OrdAbleQty` (주문가능수량). Every numeric-bearing
/// field uses [`ls_core::string_or_number`]; `#[serde(default)]` lets a sparse or
/// empty block deserialize.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct CFOAQ10100OutBlock2 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Account name / 계좌명.
    #[serde(rename = "AcntNm", deserialize_with = "ls_core::string_or_number")]
    pub acntnm: String,
    /// Orderable quantity / 주문가능수량 (canonical field, KTD6).
    #[serde(rename = "OrdAbleQty", deserialize_with = "ls_core::string_or_number")]
    pub ordableqty: String,
    /// New-order orderable quantity / 신규주문가능수량.
    #[serde(
        rename = "NewOrdAbleQty",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub newordableqty: String,
    /// Liquidation orderable quantity / 청산주문가능수량.
    #[serde(
        rename = "LqdtOrdAbleQty",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub lqdtordableqty: String,
    /// Orderable amount / 주문가능금액.
    #[serde(rename = "OrdAbleAmt", deserialize_with = "ls_core::string_or_number")]
    pub ordableamt: String,
    /// Cash orderable amount / 현금주문가능금액.
    #[serde(
        rename = "MnyOrdAbleAmt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub mnyordableamt: String,
}

/// `CFOAQ10100` response envelope.
///
/// `outblock1` is the echo/identity summary under `CFOAQ10100OutBlock1`;
/// `outblock2` is the orderable-quantity result under `CFOAQ10100OutBlock2`,
/// tolerated as a single object OR an array via [`ls_core::de_vec_or_single`]. An
/// empty `00707` yields an empty Vec (the PENDING case).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CFOAQ10100Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "CFOAQ10100OutBlock1", default)]
    pub outblock1: CFOAQ10100OutBlock1,
    #[serde(
        rename = "CFOAQ10100OutBlock2",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock2: Vec<CFOAQ10100OutBlock2>,
}

// ---------------------------------------------------------------------------
// CCENQ10100 — KRX야간파생 주문가능수량 조회 (KRX night-derivatives orderable-quantity
// inquiry, read-only). The night (krx_extended) sibling of CFOAQ10100.
//
// Identical shape and discipline to CFOAQ10100 but on the KRX night-derivatives
// account: a read-only orderable-quantity INQUIRY (조회), NOT an order. The
// account number comes from config, never a caller field. The in-block carries an
// extra `BnsTpCode`-adjacent ordering; numeric fields
// `RecCnt`/`OrdAmt`/`RatVal`/`FnoOrdPrc` serialize as JSON numbers (KTD4).
// Single-page → `Inner::post`. Canonical field (KTD6): `OrdAbleQty` (주문가능수량).
// ---------------------------------------------------------------------------

/// Input block for `CCENQ10100` — the order-shape inputs for a night-derivatives
/// orderable-quantity inquiry.
///
/// Per the normalized baseline + raw capture, `CCENQ10100InBlock1` carries eight
/// fields. The numeric ones (`RecCnt`, `OrdAmt`, `RatVal`, `FnoOrdPrc`) serialize
/// as JSON numbers (KTD4). It holds NO account number.
#[derive(Serialize, Debug, Clone)]
pub struct CCENQ10100InBlock1 {
    /// Record count / 레코드갯수 (JSON number).
    #[serde(rename = "RecCnt", serialize_with = "ls_core::string_as_number")]
    pub reccnt: String,
    /// Query distinction / 조회구분.
    #[serde(rename = "QryTp")]
    pub qrytp: String,
    /// Order amount / 주문금액 (JSON number).
    #[serde(rename = "OrdAmt", serialize_with = "ls_core::string_as_number")]
    pub ordamt: String,
    /// Ratio value / 비율값 (JSON number).
    #[serde(rename = "RatVal", serialize_with = "ls_core::string_as_number")]
    pub ratval: String,
    /// Futures/option issue number / 선물옵션종목번호.
    #[serde(rename = "FnoIsuNo")]
    pub fnoisuno: String,
    /// Buy/sell distinction / 매매구분.
    #[serde(rename = "BnsTpCode")]
    pub bnstpcode: String,
    /// Futures/option order price / 선물옵션주문가격 (JSON number).
    #[serde(rename = "FnoOrdPrc", serialize_with = "ls_core::string_as_number")]
    pub fnoordprc: String,
    /// Futures/option order-price-pattern code / 선물옵션호가유형코드.
    #[serde(rename = "FnoOrdprcPtnCode")]
    pub fnoordprcptncode: String,
}

/// `CCENQ10100` request — wraps the input block under the `CCENQ10100InBlock1` key.
///
/// Serializes the four numeric fields as JSON numbers (KTD4). No account number
/// ever appears in the body (single-page read; no continuation token).
#[derive(Serialize, Debug, Clone)]
pub struct CCENQ10100Request {
    #[serde(rename = "CCENQ10100InBlock1")]
    pub inblock: CCENQ10100InBlock1,
}

impl CCENQ10100Request {
    /// Build a `CCENQ10100` night-derivatives orderable-quantity inquiry from the
    /// order-shape inputs.
    ///
    /// This is a read-only inquiry (조회), not an order. The account number is NOT a
    /// parameter: it is established by the credentialed token and the
    /// config-supplied `ResolvedConfig.account_no`, never by the caller.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        reccnt: impl Into<String>,
        qrytp: impl Into<String>,
        ordamt: impl Into<String>,
        ratval: impl Into<String>,
        fnoisuno: impl Into<String>,
        bnstpcode: impl Into<String>,
        fnoordprc: impl Into<String>,
        fnoordprcptncode: impl Into<String>,
    ) -> Self {
        CCENQ10100Request {
            inblock: CCENQ10100InBlock1 {
                reccnt: reccnt.into(),
                qrytp: qrytp.into(),
                ordamt: ordamt.into(),
                ratval: ratval.into(),
                fnoisuno: fnoisuno.into(),
                bnstpcode: bnstpcode.into(),
                fnoordprc: fnoordprc.into(),
                fnoordprcptncode: fnoordprcptncode.into(),
            },
        }
    }
}

/// `CCENQ10100OutBlock1` — the echo / account-identity summary block.
///
/// `AcntNo`/`Pwd` are account-sensitive, so [`std::fmt::Debug`] is hand-written to
/// redact them.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CCENQ10100OutBlock1 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Account number / 계좌번호 (account-sensitive; redacted in Debug).
    #[serde(rename = "AcntNo", deserialize_with = "ls_core::string_or_number")]
    pub acntno: String,
    /// Password / 비밀번호 (account-sensitive; redacted in Debug).
    #[serde(rename = "Pwd", deserialize_with = "ls_core::string_or_number")]
    pub pwd: String,
    /// Futures/option issue number / 선물옵션종목번호 (echoes the request).
    #[serde(rename = "FnoIsuNo", deserialize_with = "ls_core::string_or_number")]
    pub fnoisuno: String,
}

impl std::fmt::Debug for CCENQ10100OutBlock1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CCENQ10100OutBlock1")
            .field("reccnt", &self.reccnt)
            .field("acntno", &"<redacted>")
            .field("pwd", &"<redacted>")
            .field("fnoisuno", &self.fnoisuno)
            .finish()
    }
}

/// `CCENQ10100OutBlock2` — the orderable-quantity result block.
///
/// A representative, spec-grounded subset of the LS `CCENQ10100OutBlock2`. The
/// canonical field (KTD6) is `OrdAbleQty` (주문가능수량). Every numeric-bearing
/// field uses [`ls_core::string_or_number`]; `#[serde(default)]` lets a sparse or
/// empty block deserialize.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct CCENQ10100OutBlock2 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Account name / 계좌명.
    #[serde(rename = "AcntNm", deserialize_with = "ls_core::string_or_number")]
    pub acntnm: String,
    /// Orderable quantity / 주문가능수량 (canonical field, KTD6).
    #[serde(rename = "OrdAbleQty", deserialize_with = "ls_core::string_or_number")]
    pub ordableqty: String,
    /// New-order orderable quantity / 신규주문가능수량.
    #[serde(
        rename = "NewOrdAbleQty",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub newordableqty: String,
    /// Liquidation orderable quantity / 청산주문가능수량.
    #[serde(
        rename = "LqdtOrdAbleQty",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub lqdtordableqty: String,
    /// Orderable amount / 주문가능금액.
    #[serde(rename = "OrdAbleAmt", deserialize_with = "ls_core::string_or_number")]
    pub ordableamt: String,
    /// Cash orderable amount / 현금주문가능금액.
    #[serde(
        rename = "MnyOrdAbleAmt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub mnyordableamt: String,
}

/// `CCENQ10100` response envelope.
///
/// `outblock1` is the echo/identity summary under `CCENQ10100OutBlock1`;
/// `outblock2` is the orderable-quantity result under `CCENQ10100OutBlock2`,
/// tolerated as a single object OR an array via [`ls_core::de_vec_or_single`]. An
/// empty `00707` yields an empty Vec (the PENDING / off-window case).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CCENQ10100Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "CCENQ10100OutBlock1", default)]
    pub outblock1: CCENQ10100OutBlock1,
    #[serde(
        rename = "CCENQ10100OutBlock2",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock2: Vec<CCENQ10100OutBlock2>,
}

// ---------------------------------------------------------------------------
// CSPBQ00200 — 현물계좌증거금률별주문가능수량조회 (orderable-quantity / capacity by
// margin rate, read-only account-state read).
//
// Returns the account's order-capacity amounts (예수금 / 거래소·코스닥 주문가능금액)
// for a given issue and side. The request carries numeric slots (`RecCnt`,
// `OrdPrc`) that MUST serialize as JSON numbers (KTD4) or the gateway returns
// IGW40011. The account-identity echo block (`CSPBQ00200OutBlock1`, which carries
// `AcntNo`/`InptPwd`) is intentionally NOT modeled — only the capacity block.
// ---------------------------------------------------------------------------

/// Input block for `CSPBQ00200` — issue + side + price; `RecCnt`/`OrdPrc` are
/// numeric slots (KTD4).
#[derive(Serialize, Debug, Clone)]
pub struct CSPBQ00200InBlock1 {
    /// Record count / 레코드갯수 (numeric slot).
    #[serde(rename = "RecCnt", serialize_with = "ls_core::string_as_number")]
    pub reccnt: String,
    /// Buy/sell distinction / 매매구분.
    #[serde(rename = "BnsTpCode")]
    pub bnstpcode: String,
    /// Issue number / 종목번호 (ISIN, e.g. `KR7005930003`).
    #[serde(rename = "IsuNo")]
    pub isuno: String,
    /// Order price / 주문가격 (numeric slot; `0` = broad capacity).
    #[serde(rename = "OrdPrc", serialize_with = "ls_core::string_as_number")]
    pub ordprc: String,
    /// Registered media code / 등록매체.
    #[serde(rename = "RegCommdaCode")]
    pub regcommdacode: String,
}

/// `CSPBQ00200` request — wraps the input block under `CSPBQ00200InBlock1`.
#[derive(Serialize, Debug, Clone)]
pub struct CSPBQ00200Request {
    #[serde(rename = "CSPBQ00200InBlock1")]
    pub inblock: CSPBQ00200InBlock1,
}

impl CSPBQ00200Request {
    /// Build a `CSPBQ00200` capacity inquiry for one issue + side at a price
    /// (`"0"` for broad capacity). `RecCnt` is fixed at `"1"`. The account number
    /// is NEVER a parameter (bearer token + config).
    pub fn new(
        bnstpcode: impl Into<String>,
        isuno: impl Into<String>,
        ordprc: impl Into<String>,
        regcommdacode: impl Into<String>,
    ) -> Self {
        CSPBQ00200Request {
            inblock: CSPBQ00200InBlock1 {
                reccnt: "1".into(),
                bnstpcode: bnstpcode.into(),
                isuno: isuno.into(),
                ordprc: ordprc.into(),
                regcommdacode: regcommdacode.into(),
            },
        }
    }
}

/// `CSPBQ00200OutBlock2` — the order-capacity block (cash + per-market amounts).
///
/// A representative numeric subset; the account-name field (`AcntNm`) is
/// intentionally NOT modeled (no PII in the surface).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct CSPBQ00200OutBlock2 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Deposit / 예수금.
    #[serde(rename = "Dps", deserialize_with = "ls_core::string_or_number")]
    pub dps: String,
    /// Estimated D+1 deposit / 추정예수금 D1 (pure cash, order-price-independent —
    /// the substantive cash witness on a cash-only account).
    #[serde(rename = "PrsmptDpsD1", deserialize_with = "ls_core::string_or_number")]
    pub prsmptdpsd1: String,
    /// Estimated D+2 deposit / 추정예수금 D2.
    #[serde(rename = "PrsmptDpsD2", deserialize_with = "ls_core::string_or_number")]
    pub prsmptdpsd2: String,
    /// KRX orderable amount / 거래소주문가능금액 (the substantive capacity witness).
    #[serde(rename = "SeOrdAbleAmt", deserialize_with = "ls_core::string_or_number")]
    pub seordableamt: String,
    /// KOSDAQ orderable amount / 코스닥주문가능금액.
    #[serde(rename = "KdqOrdAbleAmt", deserialize_with = "ls_core::string_or_number")]
    pub kdqordableamt: String,
    /// Cash orderable amount / 현금주문가능금액.
    #[serde(rename = "MnyOrdAbleAmt", deserialize_with = "ls_core::string_or_number")]
    pub mnyordableamt: String,
    /// Cash withdrawable amount / 출금가능금액.
    #[serde(rename = "MnyoutAbleAmt", deserialize_with = "ls_core::string_or_number")]
    pub mnyoutableamt: String,
    /// Orderable quantity / 주문가능수량 (0 when no price is supplied).
    #[serde(rename = "OrdAbleQty", deserialize_with = "ls_core::string_or_number")]
    pub ordableqty: String,
    /// Consignment margin rate / 위탁증거금률.
    #[serde(rename = "TrdMgnrt", deserialize_with = "ls_core::string_or_number")]
    pub trdmgnrt: String,
}

/// `CSPBQ00200` response envelope (only the capacity block is modeled).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CSPBQ00200Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "CSPBQ00200OutBlock2",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock2: Vec<CSPBQ00200OutBlock2>,
}

// ---------------------------------------------------------------------------
// CLNAQ00100 — 예탁담보융자가능종목현황조회 (loanable-collateral stock list,
// read-only). Account-aware reference data: a list of stocks eligible for
// deposit-collateral loans, with per-account limit fields. Persistent reference
// data (the loanable universe holds regardless of market hours), so it is
// closure-viable. `RecCnt` is a numeric request slot (KTD4). On `/stock/etc`.
// ---------------------------------------------------------------------------

/// Input block for `CLNAQ00100` — full-list query shape; `RecCnt` is numeric (KTD4).
#[derive(Serialize, Debug, Clone)]
pub struct CLNAQ00100InBlock1 {
    /// Record count / 레코드갯수 (numeric slot).
    #[serde(rename = "RecCnt", serialize_with = "ls_core::string_as_number")]
    pub reccnt: String,
    /// Query distinction / 조회구분 (`"0"` = full list).
    #[serde(rename = "QryTp")]
    pub qrytp: String,
    /// Issue number / 종목번호 (empty for the full list).
    #[serde(rename = "IsuNo")]
    pub isuno: String,
    /// Security-type code / 유가증권구분.
    #[serde(rename = "SecTpCode")]
    pub sectpcode: String,
    /// Loan-interest-grade code / 융자이자등급.
    #[serde(rename = "LoanIntrstGrdCode")]
    pub loanintrstgrdcode: String,
    /// Loan type / 융자구분.
    #[serde(rename = "LoanTp")]
    pub loantp: String,
}

/// `CLNAQ00100` request — wraps the input block under `CLNAQ00100InBlock1`.
#[derive(Serialize, Debug, Clone)]
pub struct CLNAQ00100Request {
    #[serde(rename = "CLNAQ00100InBlock1")]
    pub inblock: CLNAQ00100InBlock1,
}

impl Default for CLNAQ00100Request {
    fn default() -> Self {
        Self::full_list()
    }
}

impl CLNAQ00100Request {
    /// Build a full-list `CLNAQ00100` query (the whole loanable-collateral universe;
    /// `QryTp="0"`, empty `IsuNo`). Broad/default filters for the rest.
    pub fn full_list() -> Self {
        CLNAQ00100Request {
            inblock: CLNAQ00100InBlock1 {
                reccnt: "1".into(),
                qrytp: "0".into(),
                isuno: String::new(),
                sectpcode: "0".into(),
                loanintrstgrdcode: "00".into(),
                loantp: "1".into(),
            },
        }
    }
}

/// `CLNAQ00100OutBlock2` — one loanable-collateral stock (repeated array block).
///
/// A representative subset (the registration-person id is intentionally NOT
/// modeled). `IsuNm` + `LoanAbleRat` are the substantive list witnesses.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct CLNAQ00100OutBlock2 {
    /// Issue number / 종목번호.
    #[serde(rename = "IsuNo")]
    pub isuno: String,
    /// Issue name / 종목명 (the substantive list witness).
    #[serde(rename = "IsuNm")]
    pub isunm: String,
    /// Loanable rate / 융자가능비율.
    #[serde(rename = "LoanAbleRat", deserialize_with = "ls_core::string_or_number")]
    pub loanablerat: String,
    /// Registration-type name / 등록구분명 (가능/불가능).
    #[serde(rename = "RegTpNm")]
    pub regtpnm: String,
    /// Market-type name / 시장구분명.
    #[serde(rename = "MktTpNm")]
    pub mkttpnm: String,
    /// Limit value / 한도금액.
    #[serde(rename = "LmtVal", deserialize_with = "ls_core::string_or_number")]
    pub lmtval: String,
}

/// `CLNAQ00100OutBlock3` — the list summary block.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct CLNAQ00100OutBlock3 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Large-withdrawal sum amount / 대량출금합계금액.
    #[serde(
        rename = "LrgMnyoutSumAmt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub lrgmnyoutsumamt: String,
}

/// `CLNAQ00100` response envelope (the loanable-stock array + summary).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CLNAQ00100Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "CLNAQ00100OutBlock2",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock2: Vec<CLNAQ00100OutBlock2>,
    #[serde(rename = "CLNAQ00100OutBlock3", default)]
    pub outblock3: CLNAQ00100OutBlock3,
}

// ---------------------------------------------------------------------------
// CIDBQ01400 — 해외선물 체결내역개별 조회(주문가능수량) (overseas-futures orderable-
// quantity inquiry, read-only account-state read). The orderable quantity for an
// overseas-futures contract + side at a price; `RecCnt`/`OvrsDrvtOrdPrc` are
// numeric request slots (KTD4). The account-identity echo block
// (`CIDBQ01400OutBlock1`, which carries `AcntNo`) is intentionally NOT modeled.
// ---------------------------------------------------------------------------

/// Input block for `CIDBQ01400` — contract + side + price; numeric slots (KTD4).
#[derive(Serialize, Debug, Clone)]
pub struct CIDBQ01400InBlock1 {
    /// Record count / 레코드갯수 (numeric slot).
    #[serde(rename = "RecCnt", serialize_with = "ls_core::string_as_number")]
    pub reccnt: String,
    /// Query distinction / 조회구분.
    #[serde(rename = "QryTpCode")]
    pub qrytpcode: String,
    /// Issue code value / 종목코드 (an overseas-futures contract, e.g. `ADM23`).
    #[serde(rename = "IsuCodeVal")]
    pub isucodeval: String,
    /// Buy/sell distinction / 매매구분.
    #[serde(rename = "BnsTpCode")]
    pub bnstpcode: String,
    /// Overseas-derivative order price / 해외파생주문가격 (numeric slot; an overseas-
    /// futures price can be fractional, so it serializes via `string_as_decimal`,
    /// not the i64-only `string_as_number` which would quote a decimal → IGW40011).
    #[serde(
        rename = "OvrsDrvtOrdPrc",
        serialize_with = "ls_core::string_as_decimal"
    )]
    pub ovrsdrvtordprc: String,
    /// Abroad-futures order-pattern code / 해외선물주문유형.
    #[serde(rename = "AbrdFutsOrdPtnCode")]
    pub abrdfutsordptncode: String,
}

/// `CIDBQ01400` request — wraps the input block under `CIDBQ01400InBlock1`.
#[derive(Serialize, Debug, Clone)]
pub struct CIDBQ01400Request {
    #[serde(rename = "CIDBQ01400InBlock1")]
    pub inblock: CIDBQ01400InBlock1,
}

impl CIDBQ01400Request {
    /// Build a `CIDBQ01400` orderable-quantity inquiry for an overseas-futures
    /// contract + side at a price. `RecCnt` is fixed at `"1"`; no account number.
    pub fn new(
        qrytpcode: impl Into<String>,
        isucodeval: impl Into<String>,
        bnstpcode: impl Into<String>,
        ovrsdrvtordprc: impl Into<String>,
        abrdfutsordptncode: impl Into<String>,
    ) -> Self {
        CIDBQ01400Request {
            inblock: CIDBQ01400InBlock1 {
                reccnt: "1".into(),
                qrytpcode: qrytpcode.into(),
                isucodeval: isucodeval.into(),
                bnstpcode: bnstpcode.into(),
                ovrsdrvtordprc: ovrsdrvtordprc.into(),
                abrdfutsordptncode: abrdfutsordptncode.into(),
            },
        }
    }
}

/// `CIDBQ01400OutBlock2` — the orderable-quantity block.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct CIDBQ01400OutBlock2 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Orderable quantity / 주문가능수량 (the substantive witness).
    #[serde(rename = "OrdAbleQty", deserialize_with = "ls_core::string_or_number")]
    pub ordableqty: String,
}

/// `CIDBQ01400` response envelope (only the orderable-quantity block is modeled).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CIDBQ01400Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "CIDBQ01400OutBlock2",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock2: Vec<CIDBQ01400OutBlock2>,
}
