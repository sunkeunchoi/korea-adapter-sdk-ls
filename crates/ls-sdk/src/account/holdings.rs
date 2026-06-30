//! Stock & F-O position-balance (잔고) and cost-basis reads.
//!
//! Wave-2b split out of `account/mod.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use holdings::*;` so every `ls_sdk::account::*` path is unchanged.
use super::*;


// ---------------------------------------------------------------------------
// CSPAQ12300 — BEP단가조회 (account BEP / balance inquiry, read-only).
//
// A second read-only account-state read, mirroring `CSPAQ12200`'s
// account-identity discipline: the account number comes from
// `ResolvedConfig.account_no` and the bearer token, NEVER a caller field, and the
// in-block carries only the four query-shape enums. Unlike `CSPAQ12200` this read
// is single-page (`facets.self_paginated: false`), so dispatch goes through plain
// `Inner::post` and the request carries no continuation tokens.
// ---------------------------------------------------------------------------

/// Input block for `CSPAQ12300` — the four query-shape enum selectors.
///
/// Per the normalized baseline, `CSPAQ12300InBlock1` carries exactly four fields
/// (each length-1 `String`): `BalCreTp` (잔고생성구분), `CmsnAppTpCode`
/// (수수료적용구분), `D2balBaseQryTp` (D2잔고기준조회구분), `UprcTpCode` (단가구분).
/// It holds NO account number — the account identity is the bearer token plus the
/// config-supplied `ResolvedConfig.account_no`.
#[derive(Serialize, Debug, Clone)]
pub struct CSPAQ12300InBlock1 {
    /// Balance-creation distinction / 잔고생성구분.
    #[serde(rename = "BalCreTp")]
    pub balcretp: String,
    /// Commission-application distinction / 수수료적용구분.
    #[serde(rename = "CmsnAppTpCode")]
    pub cmsnapptpcode: String,
    /// D2-balance-basis query distinction / D2잔고기준조회구분.
    #[serde(rename = "D2balBaseQryTp")]
    pub d2balbaseqrytp: String,
    /// Unit-price distinction / 단가구분.
    #[serde(rename = "UprcTpCode")]
    pub uprctpcode: String,
}

/// `CSPAQ12300` request — wraps the input block under the `CSPAQ12300InBlock1` key.
///
/// Serializes to `{"CSPAQ12300InBlock1":{"BalCreTp":…,"CmsnAppTpCode":…,…}}`. No
/// account number and no continuation token ever appear in the body (this read is
/// single-page).
#[derive(Serialize, Debug, Clone)]
pub struct CSPAQ12300Request {
    #[serde(rename = "CSPAQ12300InBlock1")]
    pub inblock: CSPAQ12300InBlock1,
}

impl CSPAQ12300Request {
    /// Build a `CSPAQ12300` BEP/balance inquiry from the four query-shape enums.
    ///
    /// The account number is NOT a parameter: it is established by the credentialed
    /// token and the config-supplied `ResolvedConfig.account_no`, never by the
    /// caller.
    pub fn new(
        balcretp: impl Into<String>,
        cmsnapptpcode: impl Into<String>,
        d2balbaseqrytp: impl Into<String>,
        uprctpcode: impl Into<String>,
    ) -> Self {
        CSPAQ12300Request {
            inblock: CSPAQ12300InBlock1 {
                balcretp: balcretp.into(),
                cmsnapptpcode: cmsnapptpcode.into(),
                d2balbaseqrytp: d2balbaseqrytp.into(),
                uprctpcode: uprctpcode.into(),
            },
        }
    }
}

/// `CSPAQ12300OutBlock1` — the account-identity summary block.
///
/// Echoes the request distinctions plus account-identity fields. `AcntNo`/`Pwd`
/// are account-sensitive, so [`std::fmt::Debug`] is hand-written to redact them.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CSPAQ12300OutBlock1 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Account number / 계좌번호 (account-sensitive; redacted in Debug).
    #[serde(rename = "AcntNo", deserialize_with = "ls_core::string_or_number")]
    pub acntno: String,
    /// Password / 비밀번호 (account-sensitive; redacted in Debug).
    #[serde(rename = "Pwd", deserialize_with = "ls_core::string_or_number")]
    pub pwd: String,
    /// Balance-creation distinction / 잔고생성구분 (echoes the request).
    #[serde(rename = "BalCreTp", deserialize_with = "ls_core::string_or_number")]
    pub balcretp: String,
}

impl std::fmt::Debug for CSPAQ12300OutBlock1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CSPAQ12300OutBlock1")
            .field("reccnt", &self.reccnt)
            .field("acntno", &"<redacted>")
            .field("pwd", &"<redacted>")
            .field("balcretp", &self.balcretp)
            .finish()
    }
}

/// `CSPAQ12300OutBlock2` — the BEP / balance / orderable-amount block.
///
/// A representative, spec-grounded subset of the LS `CSPAQ12300OutBlock2` (which
/// carries ~112 fields plus a nested `CSPAQ12300OutBlock3` object array that this
/// model intentionally SKIPS — only scalar fields are modeled). Every
/// numeric-bearing field uses [`ls_core::string_or_number`]; `#[serde(default)]`
/// lets a sparse or empty block deserialize.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct CSPAQ12300OutBlock2 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Cash orderable amount / 현금주문가능금액 (canonical field, KTD4).
    #[serde(
        rename = "MnyOrdAbleAmt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub mnyordableamt: String,
    /// Cash withdrawable amount / 출금가능금액.
    #[serde(
        rename = "MnyoutAbleAmt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub mnyoutableamt: String,
    /// Total balance valuation / 잔고평가금액.
    #[serde(rename = "BalEvalAmt", deserialize_with = "ls_core::string_or_number")]
    pub balevalamt: String,
    /// Deposit-asset total amount / 예탁자산총액.
    #[serde(
        rename = "DpsastTotamt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub dpsasttotamt: String,
    /// Deposit / 예수금.
    #[serde(rename = "Dps", deserialize_with = "ls_core::string_or_number")]
    pub dps: String,
    /// D+2 deposit / D2예수금.
    #[serde(rename = "D2Dps", deserialize_with = "ls_core::string_or_number")]
    pub d2dps: String,
    /// Orderable amount / 주문가능금액.
    #[serde(rename = "OrdAbleAmt", deserialize_with = "ls_core::string_or_number")]
    pub ordableamt: String,
    /// Purchase amount / 매입금액.
    #[serde(rename = "PchsAmt", deserialize_with = "ls_core::string_or_number")]
    pub pchsamt: String,
}

/// `CSPAQ12300` response envelope.
///
/// `outblock1` is the account-identity summary under `CSPAQ12300OutBlock1`;
/// `outblock2` is the BEP/balance block under `CSPAQ12300OutBlock2`, tolerated as a
/// single object OR an array via [`ls_core::de_vec_or_single`] (the gateway
/// collapses a one-row block to a bare object).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CSPAQ12300Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "CSPAQ12300OutBlock1", default)]
    pub outblock1: CSPAQ12300OutBlock1,
    #[serde(
        rename = "CSPAQ12300OutBlock2",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock2: Vec<CSPAQ12300OutBlock2>,
}

// ---------------------------------------------------------------------------
// CCENQ90200 — KRX야간파생 잔고조회 (KRX night-derivatives account balance inquiry,
// read-only). Account-gated, krx_extended (night) session.
//
// A read-only account-state read on the KRX night-derivatives (야간파생) account.
// Carries the same account-identity discipline as the CSPAQ/CFOBQ family: the
// account number comes from `ResolvedConfig.account_no` and the bearer token,
// NEVER a caller field. `caller_supplied_identifiers` is empty — the in-block
// carries only `RecCnt` + two evaluation-shape enums (`BalEvalTp`,
// `FutsPrcEvalTp`), and the `RecCnt` numeric field serializes as a JSON number
// (`string_as_number`, KTD4). This read is single-page
// (`facets.self_paginated: false`) → plain `Inner::post`.
//
// Out-block shape (from the RAW capture, KTD5): OutBlock1 (account-identity
// summary, single object), OutBlock2 (balance / margin summary, single object —
// modeled as a Vec via `de_vec_or_single` for one-row tolerance), OutBlock3 (the
// per-position breakdown, a true JSON ARRAY in the raw capture → `Vec` via
// `de_vec_or_single`). The balance read may be empty on a position-less paper
// account or off the night window — that is the `00707`/empty case (PENDING), not
// a defect.
// ---------------------------------------------------------------------------

/// Input block for `CCENQ90200` — record count + two evaluation-shape enums.
///
/// Per the normalized baseline + raw capture, `CCENQ90200InBlock1` carries exactly
/// three fields: `RecCnt` (레코드갯수, a Number serialized as a JSON number),
/// `BalEvalTp` (잔고평가구분, length 1), `FutsPrcEvalTp` (선물가격평가구분, length 1).
/// It holds NO account number — the account identity is the bearer token plus the
/// config-supplied `ResolvedConfig.account_no`.
#[derive(Serialize, Debug, Clone)]
pub struct CCENQ90200InBlock1 {
    /// Record count / 레코드갯수 (serializes as a JSON number, KTD4).
    #[serde(rename = "RecCnt", serialize_with = "ls_core::string_as_number")]
    pub reccnt: String,
    /// Balance-evaluation distinction / 잔고평가구분.
    #[serde(rename = "BalEvalTp")]
    pub balevaltp: String,
    /// Futures-price-evaluation distinction / 선물가격평가구분.
    #[serde(rename = "FutsPrcEvalTp")]
    pub futsprcevaltp: String,
}

/// `CCENQ90200` request — wraps the input block under the `CCENQ90200InBlock1` key.
///
/// Serializes to `{"CCENQ90200InBlock1":{"RecCnt":1,"BalEvalTp":…,"FutsPrcEvalTp":…}}`
/// — `RecCnt` as a JSON number. No account number ever appears in the body
/// (single-page read; no continuation token).
#[derive(Serialize, Debug, Clone)]
pub struct CCENQ90200Request {
    #[serde(rename = "CCENQ90200InBlock1")]
    pub inblock: CCENQ90200InBlock1,
}

impl CCENQ90200Request {
    /// Build a `CCENQ90200` night-derivatives balance inquiry from the record count
    /// and the two evaluation-shape enums.
    ///
    /// The account number is NOT a parameter: it is established by the credentialed
    /// token and the config-supplied `ResolvedConfig.account_no`, never by the
    /// caller.
    pub fn new(
        reccnt: impl Into<String>,
        balevaltp: impl Into<String>,
        futsprcevaltp: impl Into<String>,
    ) -> Self {
        CCENQ90200Request {
            inblock: CCENQ90200InBlock1 {
                reccnt: reccnt.into(),
                balevaltp: balevaltp.into(),
                futsprcevaltp: futsprcevaltp.into(),
            },
        }
    }
}

/// `CCENQ90200OutBlock1` — the account-identity summary block.
///
/// `AcntNo`/`InptPwd` are account-sensitive, so [`std::fmt::Debug`] is hand-written
/// to redact them (mirrors the CSPAQ redaction discipline).
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CCENQ90200OutBlock1 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Account number / 계좌번호 (account-sensitive; redacted in Debug).
    #[serde(rename = "AcntNo", deserialize_with = "ls_core::string_or_number")]
    pub acntno: String,
    /// Input password / 입력비밀번호 (account-sensitive; redacted in Debug).
    #[serde(rename = "InptPwd", deserialize_with = "ls_core::string_or_number")]
    pub inptpwd: String,
    /// Balance-evaluation distinction / 잔고평가구분 (echoes the request).
    #[serde(rename = "BalEvalTp", deserialize_with = "ls_core::string_or_number")]
    pub balevaltp: String,
}

impl std::fmt::Debug for CCENQ90200OutBlock1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CCENQ90200OutBlock1")
            .field("reccnt", &self.reccnt)
            .field("acntno", &"<redacted>")
            .field("inptpwd", &"<redacted>")
            .field("balevaltp", &self.balevaltp)
            .finish()
    }
}

/// `CCENQ90200OutBlock2` — the night-derivatives balance / margin summary block.
///
/// A representative, spec-grounded subset of the LS `CCENQ90200OutBlock2` (~25
/// fields). The canonical field (KTD6) is `EvalDpsamtTotamt` (평가예탁금총액).
/// Every numeric-bearing field uses [`ls_core::string_or_number`];
/// `#[serde(default)]` lets a sparse or empty block deserialize.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct CCENQ90200OutBlock2 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Account name / 계좌명.
    #[serde(rename = "AcntNm", deserialize_with = "ls_core::string_or_number")]
    pub acntnm: String,
    /// Evaluated-deposit total amount / 평가예탁금총액 (canonical field, KTD6).
    #[serde(
        rename = "EvalDpsamtTotamt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub evaldpsamttotamt: String,
    /// Deposit-amount total / 예탁금총액.
    #[serde(
        rename = "DpsamtTotamt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub dpsamttotamt: String,
    /// Withdrawable total amount / 인출가능총금액.
    #[serde(
        rename = "PsnOutAbleTotAmt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub psnoutabletotamt: String,
    /// Orderable total amount / 주문가능총금액.
    #[serde(
        rename = "OrdAbleTotAmt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub ordabletotamt: String,
    /// Cash orderable amount / 현금주문가능금액.
    #[serde(
        rename = "MnyOrdAbleAmt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub mnyordableamt: String,
    /// Consigned-margin total amount / 위탁증거금총액.
    #[serde(
        rename = "CsgnMgnTotamt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub csgnmgntotamt: String,
    /// Evaluation-P&L sum / 평가손익합계.
    #[serde(rename = "EvalPnlSum", deserialize_with = "ls_core::string_or_number")]
    pub evalpnlsum: String,
}

/// `CCENQ90200OutBlock3` — the per-position breakdown block (a true JSON array).
///
/// A representative, spec-grounded subset of the LS `CCENQ90200OutBlock3` (~14
/// fields). Per the raw capture this block is a JSON ARRAY, so it is modeled as a
/// `Vec` via [`ls_core::de_vec_or_single`]. Every numeric-bearing field uses
/// [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct CCENQ90200OutBlock3 {
    /// Futures/option issue number / 선물옵션종목번호.
    #[serde(rename = "FnoIsuNo", deserialize_with = "ls_core::string_or_number")]
    pub fnoisuno: String,
    /// Issue name / 종목명.
    #[serde(rename = "IsuNm", deserialize_with = "ls_core::string_or_number")]
    pub isunm: String,
    /// Unsettled quantity / 미결제수량.
    #[serde(rename = "UnsttQty", deserialize_with = "ls_core::string_or_number")]
    pub unsttqty: String,
    /// Evaluation P&L / 평가손익.
    #[serde(rename = "EvalPnl", deserialize_with = "ls_core::string_or_number")]
    pub evalpnl: String,
    /// Evaluation amount / 평가금액.
    #[serde(rename = "EvalAmt", deserialize_with = "ls_core::string_or_number")]
    pub evalamt: String,
}

/// `CCENQ90200` response envelope.
///
/// `outblock1` is the account-identity summary under `CCENQ90200OutBlock1`;
/// `outblock2` (balance/margin summary) and `outblock3` (per-position breakdown)
/// are each tolerated as a single object OR an array via
/// [`ls_core::de_vec_or_single`]. An empty `00707` yields empty Vecs (the
/// PENDING / off-window case).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CCENQ90200Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "CCENQ90200OutBlock1", default)]
    pub outblock1: CCENQ90200OutBlock1,
    #[serde(
        rename = "CCENQ90200OutBlock2",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock2: Vec<CCENQ90200OutBlock2>,
    #[serde(
        rename = "CCENQ90200OutBlock3",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock3: Vec<CCENQ90200OutBlock3>,
}

// ---------------------------------------------------------------------------
// t0424 — 주식잔고2 (stock balance v2, read-only account-state read).
//
// A cash-summary block (`t0424OutBlock`) plus a per-holding array
// (`t0424OutBlock1`). The in-block carries only query-shape gubun flags plus a
// `cts_expcode` continuation echo — NO account number (the account identity is
// the bearer token plus `ResolvedConfig.account_no`). Single-page dispatch
// (`facets.self_paginated: false`) through plain `Inner::post`.
//
// The holdings array is the wave's U2 holdings gate (KTD3): a populated
// `t0424OutBlock1` proves the account carries stock positions; an empty array on
// a non-default cash summary is the cash-only case (a cash-summary flip, NOT a
// positions-bearing one). The array shape comes from the RAW capture's
// `res_example` (`t0424OutBlock1` is a JSON array), deserialized tolerantly via
// [`ls_core::de_vec_or_single`].
// ---------------------------------------------------------------------------

/// Input block for `t0424` — query-shape gubun flags + a continuation echo.
///
/// All five fields are query-shape selectors (price/fill/loan/charge gubun) plus
/// the `cts_expcode` continuation token. None is an account number.
#[derive(Serialize, Debug, Clone)]
pub struct T0424InBlock {
    /// Price distinction / 단가구분.
    pub prcgb: String,
    /// Fill distinction / 체결구분.
    pub chegb: String,
    /// Loan distinction / 대출구분.
    pub dangb: String,
    /// Charge distinction / 비용구분.
    pub charge: String,
    /// Continuation issue code / 연속조회 종목코드 (empty on the first page).
    pub cts_expcode: String,
}

/// `t0424` request — wraps the input block under the `t0424InBlock` key.
#[derive(Serialize, Debug, Clone)]
pub struct T0424Request {
    #[serde(rename = "t0424InBlock")]
    pub inblock: T0424InBlock,
}

impl T0424Request {
    /// Build a `t0424` stock-balance inquiry from the four gubun flags. The
    /// continuation `cts_expcode` defaults to empty (first page); the account
    /// number is NEVER a parameter (bearer token + config).
    pub fn new(
        prcgb: impl Into<String>,
        chegb: impl Into<String>,
        dangb: impl Into<String>,
        charge: impl Into<String>,
    ) -> Self {
        T0424Request {
            inblock: T0424InBlock {
                prcgb: prcgb.into(),
                chegb: chegb.into(),
                dangb: dangb.into(),
                charge: charge.into(),
                cts_expcode: String::new(),
            },
        }
    }
}

/// `t0424OutBlock` — the account cash / valuation summary block.
///
/// A representative numeric subset; every numeric-bearing field uses
/// [`ls_core::string_or_number`] and `#[serde(default)]` tolerates a sparse block.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T0424OutBlock {
    /// Day P&L / 당일실현손익.
    #[serde(rename = "dtsunik", deserialize_with = "ls_core::string_or_number")]
    pub dtsunik: String,
    /// Loan amount / 대출금액.
    #[serde(rename = "mamt", deserialize_with = "ls_core::string_or_number")]
    pub mamt: String,
    /// Estimated deposit / 추정예수금.
    #[serde(rename = "sunamt1", deserialize_with = "ls_core::string_or_number")]
    pub sunamt1: String,
    /// Total valuation amount / 평가금액.
    #[serde(rename = "tappamt", deserialize_with = "ls_core::string_or_number")]
    pub tappamt: String,
    /// Estimated deposited assets / 추정순자산 (the substantive cash witness, KTD5).
    #[serde(rename = "sunamt", deserialize_with = "ls_core::string_or_number")]
    pub sunamt: String,
    /// Total day P&L / 총당일실현손익.
    #[serde(rename = "tdtsunik", deserialize_with = "ls_core::string_or_number")]
    pub tdtsunik: String,
}

/// `t0424OutBlock1` — one held stock position (repeated array block).
///
/// The U2 holdings gate reads the LENGTH of this array (KTD3); a non-empty array
/// proves the account holds positions. A representative field subset; the
/// account-name field is intentionally NOT modeled (no PII in the surface).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T0424OutBlock1 {
    /// Issue name / 종목명.
    #[serde(rename = "hname")]
    pub hname: String,
    /// Issue code / 종목번호.
    #[serde(rename = "expcode")]
    pub expcode: String,
    /// Balance quantity / 잔고수량 (the substantive holdings witness).
    #[serde(rename = "janqty", deserialize_with = "ls_core::string_or_number")]
    pub janqty: String,
    /// Sellable quantity / 매도가능수량.
    #[serde(rename = "mdposqt", deserialize_with = "ls_core::string_or_number")]
    pub mdposqt: String,
    /// Current price / 현재가.
    #[serde(rename = "price", deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Valuation amount / 평가금액.
    #[serde(rename = "appamt", deserialize_with = "ls_core::string_or_number")]
    pub appamt: String,
    /// Average unit price / 평균단가.
    #[serde(rename = "pamt", deserialize_with = "ls_core::string_or_number")]
    pub pamt: String,
    /// P&L rate / 수익율.
    #[serde(rename = "sunikrt", deserialize_with = "ls_core::string_or_number")]
    pub sunikrt: String,
}

/// `t0424` response envelope.
///
/// `outblock` is the cash/valuation summary; `outblock1` is the per-holding array
/// (tolerated as a single object OR an array via [`ls_core::de_vec_or_single`]).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T0424Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t0424OutBlock", default)]
    pub outblock: T0424OutBlock,
    #[serde(
        rename = "t0424OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T0424OutBlock1>,
}

// ---------------------------------------------------------------------------
// t0441 — 선물/옵션잔고평가(이동평균) (F/O balance valuation, read-only account-state
// read). A per-position array (`t0441OutBlock1`) plus a valuation summary
// (`t0441OutBlock`). The in-block carries only continuation echoes — no numeric
// slots, no account number. On a position-less paper account both blocks are
// empty/zero (the AE2 expected-empty case under the U2 holdings gate).
// ---------------------------------------------------------------------------

/// Input block for `t0441` — continuation echoes only (empty on the first page).
#[derive(Serialize, Debug, Clone)]
pub struct T0441InBlock {
    /// Continuation issue code / 연속조회 종목코드.
    pub cts_expcode: String,
    /// Continuation buy/sell code / 연속조회 매매구분.
    pub cts_medocd: String,
}

/// `t0441` request — serializes to `{"t0441InBlock":{...}}`.
#[derive(Serialize, Debug, Clone)]
pub struct T0441Request {
    #[serde(rename = "t0441InBlock")]
    pub inblock: T0441InBlock,
}

impl Default for T0441Request {
    fn default() -> Self {
        Self::new()
    }
}

impl T0441Request {
    /// Build a first-page `t0441` balance-valuation request (empty continuation).
    pub fn new() -> Self {
        T0441Request {
            inblock: T0441InBlock {
                cts_expcode: String::new(),
                cts_medocd: String::new(),
            },
        }
    }
}

/// `t0441OutBlock1` — one held F/O position (repeated array block).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T0441OutBlock1 {
    /// Issue code / 종목코드.
    #[serde(rename = "expcode")]
    pub expcode: String,
    /// Buy/sell name / 매매구분명.
    #[serde(rename = "medosu")]
    pub medosu: String,
    /// Balance quantity / 잔고수량 (the substantive position witness).
    #[serde(rename = "jqty", deserialize_with = "ls_core::string_or_number")]
    pub jqty: String,
    /// Current quantity / 청산가능수량.
    #[serde(rename = "cqty", deserialize_with = "ls_core::string_or_number")]
    pub cqty: String,
    /// Valuation amount / 평가금액.
    #[serde(rename = "appamt", deserialize_with = "ls_core::string_or_number")]
    pub appamt: String,
    /// P&L rate / 수익율.
    #[serde(rename = "sunikrt", deserialize_with = "ls_core::string_or_number")]
    pub sunikrt: String,
}

/// `t0441OutBlock` — the valuation summary block.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T0441OutBlock {
    /// Total valuation amount / 총평가금액 (the substantive summary witness).
    #[serde(rename = "tappamt", deserialize_with = "ls_core::string_or_number")]
    pub tappamt: String,
    /// Total P&L / 총손익.
    #[serde(rename = "tsunik", deserialize_with = "ls_core::string_or_number")]
    pub tsunik: String,
    /// Total day P&L / 총당일손익.
    #[serde(rename = "tdtsunik", deserialize_with = "ls_core::string_or_number")]
    pub tdtsunik: String,
}

/// `t0441` response envelope (per-position array + valuation summary).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T0441Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "t0441OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T0441OutBlock1>,
    #[serde(rename = "t0441OutBlock", default)]
    pub outblock: T0441OutBlock,
}
