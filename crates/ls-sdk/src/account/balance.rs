//! Cash / F-O / overseas account deposit, margin, valuation & asset reads.
//!
//! Wave-2b split out of `account/mod.rs` (pure relocation; see
//! `docs/plans/notes/2026-06-29-003-refactor-baseline.md`). Re-exported via
//! `pub use balance::*;` so every `ls_sdk::account::*` path is unchanged.
use super::*;


/// Input block for `CSPAQ12200` — the balance-creation distinction.
///
/// The REAL `CSPAQ12200InBlock1` (generated source / spec) carries exactly one
/// field, `BalCreTp` (잔고생성구분, length 1). It holds NO account number: the
/// account is identified by the bearer token, and the account-scoped runtime
/// context is `ResolvedConfig.account_no`, sourced from config — never from a
/// caller-supplied identifier.
#[derive(Serialize, Debug, Clone)]
pub struct CSPAQ12200InBlock1 {
    /// Balance-creation distinction / 잔고생성구분 (e.g. `"1"`).
    #[serde(rename = "BalCreTp")]
    pub balcretp: String,
}

/// `CSPAQ12200` request — wraps the input block under the `CSPAQ12200InBlock1` key.
///
/// Serializes to `{"CSPAQ12200InBlock1":{"BalCreTp":...}}`. The `tr_cont`/
/// `tr_cont_key` fields are `#[serde(skip)]`, so they NEVER appear in the body;
/// they ride as HTTP headers via the [`ls_core::HasPagination`] impl below.
#[derive(Serialize, Debug, Clone)]
pub struct CSPAQ12200Request {
    #[serde(rename = "CSPAQ12200InBlock1")]
    pub inblock: CSPAQ12200InBlock1,
    /// Pagination continuation token (set by `collect_all`; injected as HTTP header).
    #[serde(skip)]
    pub tr_cont: String,
    /// Pagination continuation key (set by `collect_all`; injected as HTTP header).
    #[serde(skip)]
    pub tr_cont_key: String,
}

// Continuation tokens ride as HTTP headers via this trait; the macro is exported
// from `ls-core` (`#[macro_export]`) because the request struct lives in `ls-sdk`.
ls_core::impl_has_pagination!(CSPAQ12200Request);

impl CSPAQ12200Request {
    /// Build a `CSPAQ12200` balance inquiry for the given `BalCreTp`.
    ///
    /// The account number is NOT a parameter: it is established by the credentialed
    /// token and the config-supplied `ResolvedConfig.account_no`, never by the
    /// caller. Continuation fields start empty (first page).
    pub fn new(balcretp: impl Into<String>) -> Self {
        CSPAQ12200Request {
            inblock: CSPAQ12200InBlock1 {
                balcretp: balcretp.into(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `CSPAQ12200OutBlock1` — the account-identity summary block.
///
/// Echoes the request distinction plus account-identity fields. `AcntNo`/`Pwd` are
/// account-sensitive, so [`std::fmt::Debug`] is hand-written to redact them. Field
/// names mirror the spec (`specs/ls_openapi_specs.json` → `CSPAQ12200OutBlock1`)
/// verbatim; every value uses [`ls_core::string_or_number`].
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CSPAQ12200OutBlock1 {
    /// Record count / 레코드 카운트.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Managing branch number / 관리지점번호.
    #[serde(rename = "MgmtBrnNo", deserialize_with = "ls_core::string_or_number")]
    pub mgmtbrnno: String,
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

impl std::fmt::Debug for CSPAQ12200OutBlock1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CSPAQ12200OutBlock1")
            .field("reccnt", &self.reccnt)
            .field("mgmtbrnno", &self.mgmtbrnno)
            .field("acntno", &"<redacted>")
            .field("pwd", &"<redacted>")
            .field("balcretp", &self.balcretp)
            .finish()
    }
}

/// `CSPAQ12200OutBlock2` — the balance / orderable-amount / valuation block.
///
/// A representative, spec-grounded subset of the LS `CSPAQ12200OutBlock2`: the
/// headline orderable amounts and total-valuation figures. Every numeric-bearing
/// field uses [`ls_core::string_or_number`] (the gateway sends them as either JSON
/// numbers or strings); `#[serde(default)]` lets a sparse block deserialize.
/// Field names mirror the spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct CSPAQ12200OutBlock2 {
    /// Record count / 레코드 카운트.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Branch name / 지점명.
    #[serde(rename = "BrnNm", deserialize_with = "ls_core::string_or_number")]
    pub brnnm: String,
    /// Account name / 계좌명.
    #[serde(rename = "AcntNm", deserialize_with = "ls_core::string_or_number")]
    pub acntnm: String,
    /// Cash orderable amount / 현금주문가능금액.
    #[serde(
        rename = "MnyOrdAbleAmt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub mnyordableamt: String,
    /// Cash withdrawable amount / 현금출금가능금액.
    #[serde(
        rename = "MnyoutAbleAmt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub mnyoutableamt: String,
    /// KOSPI orderable amount / 유가증권주문가능금액.
    #[serde(
        rename = "SeOrdAbleAmt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub seordableamt: String,
    /// KOSDAQ orderable amount / 코스닥주문가능금액.
    #[serde(
        rename = "KdqOrdAbleAmt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub kdqordableamt: String,
    /// Total balance valuation / 잔고평가금액.
    #[serde(rename = "BalEvalAmt", deserialize_with = "ls_core::string_or_number")]
    pub balevalamt: String,
    /// Receivable amount / 미수금액.
    #[serde(rename = "RcvblAmt", deserialize_with = "ls_core::string_or_number")]
    pub rcvblamt: String,
    /// Deposit-asset total amount / 예탁자산총금액.
    #[serde(
        rename = "DpsastTotamt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub dpsasttotamt: String,
    /// Profit-and-loss rate / 손익율.
    #[serde(rename = "PnlRat", deserialize_with = "ls_core::string_or_number")]
    pub pnlrat: String,
    /// Deposit / 예수금.
    #[serde(rename = "Dps", deserialize_with = "ls_core::string_or_number")]
    pub dps: String,
    /// Substitute amount / 대용금액.
    #[serde(rename = "SubstAmt", deserialize_with = "ls_core::string_or_number")]
    pub substamt: String,
    /// D+1 deposit / D+1예수금.
    #[serde(rename = "D1Dps", deserialize_with = "ls_core::string_or_number")]
    pub d1dps: String,
    /// D+2 deposit / D+2예수금.
    #[serde(rename = "D2Dps", deserialize_with = "ls_core::string_or_number")]
    pub d2dps: String,
}

/// `CSPAQ12200` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields (classified in `ls-core`
/// dispatch before this struct is built). `outblock1` is the account-identity
/// summary under `CSPAQ12200OutBlock1`; `outblock2` is the balance block under
/// `CSPAQ12200OutBlock2`, tolerated as a single object OR an array via
/// [`ls_core::de_vec_or_single`].
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CSPAQ12200Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "CSPAQ12200OutBlock1", default)]
    pub outblock1: CSPAQ12200OutBlock1,
    #[serde(
        rename = "CSPAQ12200OutBlock2",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock2: Vec<CSPAQ12200OutBlock2>,
}

// ---------------------------------------------------------------------------
// CSPAQ22200 — 현물계좌예수금 주문가능금액 총평가2 (account orderable-amount /
// total-valuation inquiry, read-only).
//
// A third read-only account-state read, mirroring `CSPAQ12300`'s account-identity
// discipline: the account number comes from `ResolvedConfig.account_no` and the
// bearer token, NEVER a caller field. The in-block carries ONLY `BalCreTp`
// (잔고생성구분). This read is single-page (`facets.self_paginated: false`), so
// dispatch goes through plain `Inner::post` with no continuation tokens.
// ---------------------------------------------------------------------------

/// Input block for `CSPAQ22200` — the balance-creation distinction.
///
/// Per the normalized baseline, `CSPAQ22200InBlock1` carries exactly one field,
/// `BalCreTp` (잔고생성구분, length 1). It holds NO account number — the account
/// identity is the bearer token plus the config-supplied `ResolvedConfig.account_no`.
#[derive(Serialize, Debug, Clone)]
pub struct CSPAQ22200InBlock1 {
    /// Balance-creation distinction / 잔고생성구분.
    #[serde(rename = "BalCreTp")]
    pub balcretp: String,
}

/// `CSPAQ22200` request — wraps the input block under the `CSPAQ22200InBlock1` key.
///
/// Serializes to `{"CSPAQ22200InBlock1":{"BalCreTp":…}}`. No account number and no
/// continuation token ever appear in the body (this read is single-page).
#[derive(Serialize, Debug, Clone)]
pub struct CSPAQ22200Request {
    #[serde(rename = "CSPAQ22200InBlock1")]
    pub inblock: CSPAQ22200InBlock1,
}

impl CSPAQ22200Request {
    /// Build a `CSPAQ22200` orderable-amount/valuation inquiry for the given
    /// `BalCreTp`.
    ///
    /// The account number is NOT a parameter: it is established by the credentialed
    /// token and the config-supplied `ResolvedConfig.account_no`, never by the
    /// caller.
    pub fn new(balcretp: impl Into<String>) -> Self {
        CSPAQ22200Request {
            inblock: CSPAQ22200InBlock1 {
                balcretp: balcretp.into(),
            },
        }
    }
}

/// `CSPAQ22200OutBlock1` — the account-identity summary block.
///
/// Echoes the request distinction plus account-identity fields. There is no
/// `AcntNo`/`Pwd` in this block's spec (only `RecCnt`/`MgmtBrnNo`/`BalCreTp`), but
/// the managing-branch number is account-administrative, so [`std::fmt::Debug`] is
/// hand-written to redact it (mirrors the CSPAQ12200/CSPAQ12300 redaction
/// discipline for account-identifying fields).
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CSPAQ22200OutBlock1 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Managing branch number / 관리지점번호 (account-administrative; redacted in Debug).
    #[serde(rename = "MgmtBrnNo", deserialize_with = "ls_core::string_or_number")]
    pub mgmtbrnno: String,
    /// Balance-creation distinction / 잔고생성구분 (echoes the request).
    #[serde(rename = "BalCreTp", deserialize_with = "ls_core::string_or_number")]
    pub balcretp: String,
}

impl std::fmt::Debug for CSPAQ22200OutBlock1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CSPAQ22200OutBlock1")
            .field("reccnt", &self.reccnt)
            .field("mgmtbrnno", &"<redacted>")
            .field("balcretp", &self.balcretp)
            .finish()
    }
}

/// `CSPAQ22200OutBlock2` — the orderable-amount / valuation block.
///
/// A representative, spec-grounded subset of the LS `CSPAQ22200OutBlock2` (~38
/// fields). Every numeric-bearing field uses [`ls_core::string_or_number`] (the
/// gateway sends them as either JSON numbers or strings); `#[serde(default)]` lets a
/// sparse or empty block deserialize. Field names mirror the spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct CSPAQ22200OutBlock2 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Branch name / 지점명.
    #[serde(rename = "BrnNm", deserialize_with = "ls_core::string_or_number")]
    pub brnnm: String,
    /// Account name / 계좌명.
    #[serde(rename = "AcntNm", deserialize_with = "ls_core::string_or_number")]
    pub acntnm: String,
    /// Cash orderable amount / 현금주문가능금액 (canonical field, KTD4).
    #[serde(
        rename = "MnyOrdAbleAmt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub mnyordableamt: String,
    /// Substitute orderable amount / 대용주문가능금액.
    #[serde(
        rename = "SubstOrdAbleAmt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub substordableamt: String,
    /// Exchange (KOSPI) amount / 거래소금액.
    #[serde(
        rename = "SeOrdAbleAmt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub seordableamt: String,
    /// KOSDAQ amount / 코스닥금액.
    #[serde(
        rename = "KdqOrdAbleAmt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub kdqordableamt: String,
    /// Deposit / 예수금.
    #[serde(rename = "Dps", deserialize_with = "ls_core::string_or_number")]
    pub dps: String,
    /// Substitute amount / 대용금액.
    #[serde(rename = "SubstAmt", deserialize_with = "ls_core::string_or_number")]
    pub substamt: String,
    /// D+1 deposit / D1예수금.
    #[serde(rename = "D1Dps", deserialize_with = "ls_core::string_or_number")]
    pub d1dps: String,
    /// D+2 deposit / D2예수금.
    #[serde(rename = "D2Dps", deserialize_with = "ls_core::string_or_number")]
    pub d2dps: String,
    /// Receivable amount / 미수금액.
    #[serde(rename = "RcvblAmt", deserialize_with = "ls_core::string_or_number")]
    pub rcvblamt: String,
}

/// `CSPAQ22200` response envelope.
///
/// `outblock1` is the account-identity summary under `CSPAQ22200OutBlock1`;
/// `outblock2` is the orderable-amount/valuation block under `CSPAQ22200OutBlock2`,
/// tolerated as a single object OR an array via [`ls_core::de_vec_or_single`] (the
/// gateway collapses a one-row block to a bare object).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CSPAQ22200Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "CSPAQ22200OutBlock1", default)]
    pub outblock1: CSPAQ22200OutBlock1,
    #[serde(
        rename = "CSPAQ22200OutBlock2",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock2: Vec<CSPAQ22200OutBlock2>,
}

// ---------------------------------------------------------------------------
// CFOBQ10500 — 선물옵션 계좌예탁금증거금조회 (F/O account deposit / margin inquiry,
// read-only).
//
// A read-only account-state read on the futures/options account. Like the
// CSPAQ family it carries the same account-identity discipline: the account
// number comes from `ResolvedConfig.account_no` and the bearer token, NEVER a
// caller field. Unlike them this read has NO request-body fields at all — the
// `tr_cd` rides the request header, so the in-block is an empty object and
// `::new()` takes no arguments (mirrors `t8425`'s no-caller-input shape). This
// read is single-page (`facets.self_paginated: false`) → plain `Inner::post`.
//
// The deposit response may be empty on a position-less paper account (the
// `00707` empty case) — that is the PENDING outcome, not a defect.
// ---------------------------------------------------------------------------

/// Input block for `CFOBQ10500` — empty (no caller-supplied fields).
///
/// Per the normalized baseline, `CFOBQ10500` carries NO request-body fields: the
/// `tr_cd` is a request-header field, not a body field, and there is no account
/// number in the body (account identity is the bearer token plus the
/// config-supplied `ResolvedConfig.account_no`). The in-block serializes as an
/// empty object `{}`.
#[derive(Serialize, Debug, Clone, Default)]
pub struct CFOBQ10500InBlock {}

/// `CFOBQ10500` request — wraps the empty input block under the
/// `CFOBQ10500InBlock` key.
///
/// Serializes to `{"CFOBQ10500InBlock":{}}`. No account number and no caller
/// field ever appears in the body. The key is `CFOBQ10500InBlock` with NO `1`
/// suffix (unlike the sibling `CSPAQ*InBlock1` keys): the baseline carries no
/// numbered request in-block for this header-only read, so the suffix-less name
/// is the spec-accurate wire key — do NOT "normalize" it to `...InBlock1`.
#[derive(Serialize, Debug, Clone, Default)]
pub struct CFOBQ10500Request {
    #[serde(rename = "CFOBQ10500InBlock")]
    pub inblock: CFOBQ10500InBlock,
}

impl CFOBQ10500Request {
    /// Build a `CFOBQ10500` F/O deposit inquiry. Takes no caller input: the
    /// account is established by the credentialed token and the config-supplied
    /// `ResolvedConfig.account_no`, never by the caller.
    pub fn new() -> Self {
        CFOBQ10500Request {
            inblock: CFOBQ10500InBlock {},
        }
    }
}

/// `CFOBQ10500OutBlock1` — the account-identity summary block.
///
/// `AcntNo`/`Pwd` are account-sensitive, so [`std::fmt::Debug`] is hand-written
/// to redact them (mirrors the CSPAQ redaction discipline).
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CFOBQ10500OutBlock1 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Account number / 계좌번호 (account-sensitive; redacted in Debug).
    #[serde(rename = "AcntNo", deserialize_with = "ls_core::string_or_number")]
    pub acntno: String,
    /// Password / 비밀번호 (account-sensitive; redacted in Debug).
    #[serde(rename = "Pwd", deserialize_with = "ls_core::string_or_number")]
    pub pwd: String,
}

impl std::fmt::Debug for CFOBQ10500OutBlock1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CFOBQ10500OutBlock1")
            .field("reccnt", &self.reccnt)
            .field("acntno", &"<redacted>")
            .field("pwd", &"<redacted>")
            .finish()
    }
}

/// `CFOBQ10500OutBlock2` — the deposit / margin summary block.
///
/// A representative, spec-grounded subset of the LS `CFOBQ10500OutBlock2` (24
/// fields): the headline deposit, withdrawable, and margin figures. Every
/// numeric-bearing field uses [`ls_core::string_or_number`]; `#[serde(default)]`
/// lets a sparse or empty block deserialize. Field names mirror the spec verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct CFOBQ10500OutBlock2 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Account name / 계좌명.
    #[serde(rename = "AcntNm", deserialize_with = "ls_core::string_or_number")]
    pub acntnm: String,
    /// Total deposit / 예탁금총액 (canonical field, KTD4).
    #[serde(
        rename = "DpsamtTotamt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub dpsamttotamt: String,
    /// Deposit / 예수금.
    #[serde(rename = "Dps", deserialize_with = "ls_core::string_or_number")]
    pub dps: String,
    /// Substitute amount / 대용금액.
    #[serde(rename = "SubstAmt", deserialize_with = "ls_core::string_or_number")]
    pub substamt: String,
    /// Withdrawable amount / 인출가능금액.
    #[serde(
        rename = "WthdwAbleAmt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub wthdwableamt: String,
    /// Margin amount / 증거금액.
    #[serde(rename = "Mgn", deserialize_with = "ls_core::string_or_number")]
    pub mgn: String,
    /// Orderable amount / 주문가능금액.
    #[serde(rename = "OrdAbleAmt", deserialize_with = "ls_core::string_or_number")]
    pub ordableamt: String,
}

/// `CFOBQ10500OutBlock3` — the per-product-group margin breakdown block.
///
/// A representative, spec-grounded subset of the LS `CFOBQ10500OutBlock3` (18
/// fields). Every numeric-bearing field uses [`ls_core::string_or_number`];
/// `#[serde(default)]` lets a sparse or empty block deserialize.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct CFOBQ10500OutBlock3 {
    /// Product-group code name / 상품군코드명.
    #[serde(rename = "PdGrpCodeNm", deserialize_with = "ls_core::string_or_number")]
    pub pdgrpcodenm: String,
    /// Net-risk margin / 순위험증거금액.
    #[serde(rename = "NetRiskMgn", deserialize_with = "ls_core::string_or_number")]
    pub netriskmgn: String,
    /// Price margin / 가격증거금액.
    #[serde(rename = "PrcMgn", deserialize_with = "ls_core::string_or_number")]
    pub prcmgn: String,
    /// Order margin / 주문증거금액.
    #[serde(rename = "OrdMgn", deserialize_with = "ls_core::string_or_number")]
    pub ordmgn: String,
    /// Maintenance margin / 유지증거금액.
    #[serde(rename = "MaintMgn", deserialize_with = "ls_core::string_or_number")]
    pub maintmgn: String,
}

/// `CFOBQ10500` response envelope.
///
/// `outblock1` is the account-identity summary under `CFOBQ10500OutBlock1`;
/// `outblock2` (deposit/margin summary) and `outblock3` (per-product-group margin)
/// are each tolerated as a single object OR an array via
/// [`ls_core::de_vec_or_single`] (the gateway collapses a one-row block to a bare
/// object). An empty `00707` yields empty Vecs (the PENDING case).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CFOBQ10500Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "CFOBQ10500OutBlock1", default)]
    pub outblock1: CFOBQ10500OutBlock1,
    #[serde(
        rename = "CFOBQ10500OutBlock2",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock2: Vec<CFOBQ10500OutBlock2>,
    #[serde(
        rename = "CFOBQ10500OutBlock3",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock3: Vec<CFOBQ10500OutBlock3>,
}

// ---------------------------------------------------------------------------
// CFOEQ11100 — 선물옵션가정산예탁금상세 (F/O provisional-settlement deposit detail,
// read-only account-state read). The F/O account's deposit / margin figures;
// `RecCnt` is a numeric request slot (KTD4). The account-name field (`AcntNm`)
// is intentionally NOT modeled.
// ---------------------------------------------------------------------------

/// Input block for `CFOEQ11100` — settlement business date; `RecCnt` numeric (KTD4).
#[derive(Serialize, Debug, Clone)]
pub struct CFOEQ11100InBlock1 {
    /// Record count / 레코드갯수 (numeric slot).
    #[serde(rename = "RecCnt", serialize_with = "ls_core::string_as_number")]
    pub reccnt: String,
    /// Business date / 매매일자 (YYYYMMDD).
    #[serde(rename = "BnsDt")]
    pub bnsdt: String,
}

/// `CFOEQ11100` request — wraps the input block under `CFOEQ11100InBlock1`.
#[derive(Serialize, Debug, Clone)]
pub struct CFOEQ11100Request {
    #[serde(rename = "CFOEQ11100InBlock1")]
    pub inblock: CFOEQ11100InBlock1,
}

impl CFOEQ11100Request {
    /// Build a `CFOEQ11100` deposit-detail inquiry for a business date. `RecCnt` is
    /// fixed at `"1"`; the account number is NEVER a parameter (bearer token).
    pub fn new(bnsdt: impl Into<String>) -> Self {
        CFOEQ11100Request {
            inblock: CFOEQ11100InBlock1 {
                reccnt: "1".into(),
                bnsdt: bnsdt.into(),
            },
        }
    }
}

/// `CFOEQ11100OutBlock2` — the F/O deposit / margin detail block.
///
/// A representative numeric subset; the account-name field (`AcntNm`) is NOT
/// modeled (no PII). `Dps` (예수금) is the substantive deposit witness.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct CFOEQ11100OutBlock2 {
    /// Deposit / 예수금 (the substantive deposit witness).
    #[serde(rename = "Dps", deserialize_with = "ls_core::string_or_number")]
    pub dps: String,
    /// Market-open deposit total / 개장예탁금총액.
    #[serde(
        rename = "OpnmkDpsamtTotamt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub opnmkdpsamttotamt: String,
    /// Cash orderable amount / 현금주문가능금액.
    #[serde(rename = "MnyOrdAbleAmt", deserialize_with = "ls_core::string_or_number")]
    pub mnyordableamt: String,
    /// Consignment margin / 위탁증거금.
    #[serde(rename = "CsgnMgn", deserialize_with = "ls_core::string_or_number")]
    pub csgnmgn: String,
    /// Cash consignment margin / 현금위탁증거금.
    #[serde(rename = "MnyCsgnMgn", deserialize_with = "ls_core::string_or_number")]
    pub mnycsgnmgn: String,
}

/// `CFOEQ11100` response envelope (only the deposit-detail block is modeled).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CFOEQ11100Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "CFOEQ11100OutBlock2",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock2: Vec<CFOEQ11100OutBlock2>,
}

// ---------------------------------------------------------------------------
// CIDBQ03000 — 해외선물 예수금/잔고현황 (overseas-futures deposit / balance status,
// read-only account-state read). The per-currency balance block carries the
// deposit/valuation/orderable amounts. `RecCnt` is a numeric request slot (KTD4).
// The account-identity echo block (`CIDBQ03000OutBlock1`, which carries `AcntNo`/
// `AcntPwd`) is intentionally NOT modeled. Reachable only when authenticated as the
// overseas-futures account (overseas_option lane) — empty/wrong-account otherwise.
// ---------------------------------------------------------------------------

/// Input block for `CIDBQ03000` — account type + trade date; numeric slot (KTD4).
#[derive(Serialize, Debug, Clone)]
pub struct CIDBQ03000InBlock1 {
    /// Record count / 레코드갯수 (numeric slot).
    #[serde(rename = "RecCnt", serialize_with = "ls_core::string_as_number")]
    pub reccnt: String,
    /// Account-type code / 계좌구분 (`1`:위탁계좌, `2`:중개계좌).
    #[serde(rename = "AcntTpCode")]
    pub acnttpcode: String,
    /// Trade date / 거래일자 (`YYYYMMDD`).
    #[serde(rename = "TrdDt")]
    pub trddt: String,
}

/// `CIDBQ03000` request — wraps the input block under `CIDBQ03000InBlock1`.
#[derive(Serialize, Debug, Clone)]
pub struct CIDBQ03000Request {
    #[serde(rename = "CIDBQ03000InBlock1")]
    pub inblock: CIDBQ03000InBlock1,
}

impl CIDBQ03000Request {
    /// Build a `CIDBQ03000` deposit/balance inquiry for an account type + trade date.
    /// `RecCnt` is fixed at `"1"`; no account number (token-bound).
    pub fn new(acnttpcode: impl Into<String>, trddt: impl Into<String>) -> Self {
        CIDBQ03000Request {
            inblock: CIDBQ03000InBlock1 {
                reccnt: "1".into(),
                acnttpcode: acnttpcode.into(),
                trddt: trddt.into(),
            },
        }
    }
}

/// `CIDBQ03000OutBlock2` — the per-currency deposit/balance block (money fields
/// are decimal strings, tolerant via `string_or_number`).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct CIDBQ03000OutBlock2 {
    /// Currency-object code / 통화대상코드 (e.g. `TOT(USD)`).
    #[serde(rename = "CrcyObjCode")]
    pub crcyobjcode: String,
    /// Pre-exchange deposit / 환전전예수금.
    #[serde(rename = "PrexchDps", deserialize_with = "ls_core::string_or_number")]
    pub prexchdps: String,
    /// Evaluated asset amount / 평가자산금액 (the substantive witness).
    #[serde(rename = "EvalAssetAmt", deserialize_with = "ls_core::string_or_number")]
    pub evalassetamt: String,
    /// Abroad-futures orderable amount / 해외선물주문가능금액.
    #[serde(
        rename = "AbrdFutsOrdAbleAmt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub abrdfutsordableamt: String,
}

/// `CIDBQ03000` response envelope (only the per-currency balance block is modeled).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CIDBQ03000Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "CIDBQ03000OutBlock2",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock2: Vec<CIDBQ03000OutBlock2>,
}

// ---------------------------------------------------------------------------
// CIDBQ05300 — 해외선물 예탁자산 조회 (overseas-futures deposited-assets inquiry,
// read-only account-state read). The per-currency deposited-asset block carries
// the deposit/orderable amounts by currency. `RecCnt` is a numeric request slot
// (KTD4). The account-identity echo block (`CIDBQ05300OutBlock1`, `AcntNo`/`AcntPwd`)
// and the summary block (`CIDBQ05300OutBlock3`) are NOT modeled. Reachable only when
// authenticated as the overseas-futures account (overseas_option lane); the cash
// account returned `IGW40013` here (wrong-account artifact, §16).
// ---------------------------------------------------------------------------

/// Input block for `CIDBQ05300` — account type + currency; numeric slot (KTD4).
#[derive(Serialize, Debug, Clone)]
pub struct CIDBQ05300InBlock1 {
    /// Record count / 레코드갯수 (numeric slot).
    #[serde(rename = "RecCnt", serialize_with = "ls_core::string_as_number")]
    pub reccnt: String,
    /// Overseas account-type code / 해외계좌구분 (`1`:위탁).
    #[serde(rename = "OvrsAcntTpCode")]
    pub ovrsacnttpcode: String,
    /// FCM account number / 법인 FCM계좌번호 (corp-only; empty for individual accounts).
    #[serde(rename = "FcmAcntNo")]
    pub fcmacntno: String,
    /// Currency code / 통화코드 (`ALL`:전체, `USD`, `KRW`, …).
    #[serde(rename = "CrcyCode")]
    pub crcycode: String,
}

/// `CIDBQ05300` request — wraps the input block under `CIDBQ05300InBlock1`.
#[derive(Serialize, Debug, Clone)]
pub struct CIDBQ05300Request {
    #[serde(rename = "CIDBQ05300InBlock1")]
    pub inblock: CIDBQ05300InBlock1,
}

impl CIDBQ05300Request {
    /// Build a `CIDBQ05300` deposited-assets inquiry for an account type + currency.
    /// `RecCnt` is fixed at `"1"`, `FcmAcntNo` empty (individual account); no account
    /// number (token-bound).
    pub fn new(ovrsacnttpcode: impl Into<String>, crcycode: impl Into<String>) -> Self {
        CIDBQ05300Request {
            inblock: CIDBQ05300InBlock1 {
                reccnt: "1".into(),
                ovrsacnttpcode: ovrsacnttpcode.into(),
                fcmacntno: String::new(),
                crcycode: crcycode.into(),
            },
        }
    }
}

/// `CIDBQ05300OutBlock2` — the per-currency deposited-asset block (money fields
/// are decimal strings, tolerant via `string_or_number`).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct CIDBQ05300OutBlock2 {
    /// Currency code / 통화코드 (e.g. `USD`, `KRW`).
    #[serde(rename = "CrcyCode")]
    pub crcycode: String,
    /// Overseas-futures deposit / 해외선물예수금 (the substantive witness).
    #[serde(rename = "OvrsFutsDps", deserialize_with = "ls_core::string_or_number")]
    pub ovrsfutsdps: String,
    /// Abroad-futures orderable amount / 해외선물주문가능금액.
    #[serde(
        rename = "AbrdFutsOrdAbleAmt",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub abrdfutsordableamt: String,
}

/// `CIDBQ05300` response envelope (only the per-currency deposited-asset block is
/// modeled; the account echo + summary blocks are not).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CIDBQ05300Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(
        rename = "CIDBQ05300OutBlock2",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock2: Vec<CIDBQ05300OutBlock2>,
}
