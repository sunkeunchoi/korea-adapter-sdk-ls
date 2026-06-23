//! Account dependency class — `CSPAQ12200` 현물계좌예수금/주문가능금액/총평가 조회
//! (cash-account deposit / orderable-amount / total-valuation inquiry).
//!
//! This is the *account* class: a credentialed, account-scoped balance inquiry.
//! Its defining facet is `account_state: true` — the response reflects the live
//! state of the authenticated account, so meaningful evidence requires real
//! credentials. The Change-Scoped Gate therefore selects ONLY credential-free
//! request-construction tests for this TR (the `account_state` gate); credentialed
//! live evidence is scheduled separately and is NOT run in the unit suite.
//!
//! ## The account number comes from config, NOT from the caller
//!
//! `CSPAQ12200`'s `caller_supplied_identifiers` is empty. The real
//! `CSPAQ12200InBlock1` (per the generated source and `specs/ls_openapi_specs.json`)
//! carries exactly ONE field — `BalCreTp` (잔고생성구분) — and NO account number:
//! the account identity is established by the OAuth bearer token, and the
//! account-scoped runtime context is `ResolvedConfig.account_no`. The request is
//! built from a caller-chosen `BalCreTp` plus the CONFIG-supplied account
//! ([`Account::account_no`] reads it off `Inner::config`), never from a
//! caller-passed account identifier. A caller cannot inject an account number
//! through this surface.
//!
//! ## Pagination is a transport detail, not a body field
//!
//! `CSPAQ12200_POLICY.has_pagination` is `true`, so the request implements
//! [`ls_core::HasPagination`] and dispatch goes through
//! [`ls_core::Inner::post_paginated`]. The `tr_cont`/`tr_cont_key` continuation
//! tokens are `#[serde(skip)]` and ride as HTTP headers — they never appear in the
//! `{"CSPAQ12200InBlock1":{...}}` body.
//!
//! ## Wire-compat: string-or-number, single-or-array
//!
//! Balance amounts arrive as either JSON numbers or strings, so every numeric
//! field uses [`ls_core::string_or_number`]. `CSPAQ12200OutBlock2` is tolerated as
//! either a single object or an array via [`ls_core::de_vec_or_single`] (the
//! gateway collapses a one-row block to a bare object). Both are the load-bearing
//! behaviors R10 preserves.
//!
//! ## Error classification: `01715` vs `01900` are DISTINCT
//!
//! Business errors classify on the structured `rsp_cd`, never on `rsp_msg`
//! substrings. `01900` (paper-incompatible work) is the sole
//! paper-incompatible signal — [`ls_core::LsError::is_paper_incompatible`] is
//! `true` for it. `01715` (a date-related error, e.g. an empty date defaulting to a
//! non-trading day) is a normal [`ls_core::LsError::ApiError`] with code `"01715"`
//! and `is_paper_incompatible()` `false`. The two never collapse together.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use ls_core::{Inner, LsResult};

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

/// Account operations, backed by the shared runtime core.
///
/// Cheap to clone — shares `Arc<Inner>` (and therefore the token cache, rate
/// limiter, and `ResolvedConfig`) with the rest of the SDK.
#[derive(Clone)]
pub struct Account {
    inner: Arc<Inner>,
}

impl Account {
    /// Wrap a shared runtime core.
    pub fn new(inner: Arc<Inner>) -> Self {
        Account { inner }
    }

    /// The config-supplied account number this handle operates on.
    ///
    /// Sourced from `ResolvedConfig.account_no` — the account is established by
    /// config and the credentialed token, NEVER by a caller-supplied identifier.
    /// This is the account-scoped runtime context for the `account_state` class.
    pub fn account_no(&self) -> &str {
        &self.inner.config.account_no
    }

    /// Inquire the cash-account balance / orderable amounts via `CSPAQ12200`.
    ///
    /// Dispatches through [`ls_core::Inner::post_paginated`] (Account rate bucket).
    /// The account is the config-supplied [`Account::account_no`], identified by the
    /// bearer token — the caller passes only `BalCreTp`. A `01900` business code
    /// surfaces as [`ls_core::LsError::ApiError`] and classifies as
    /// paper-incompatible; `01715` surfaces as a normal `ApiError` that does not.
    pub async fn balance(&self, req: &CSPAQ12200Request) -> LsResult<CSPAQ12200Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::CSPAQ12200_POLICY, req)
            .await
    }

    /// Inquire the account BEP / balance via `CSPAQ12300`.
    ///
    /// Dispatches through plain [`ls_core::Inner::post`] (Account rate bucket,
    /// single-page). The account is the config-supplied [`Account::account_no`],
    /// identified by the bearer token — the caller passes only the four
    /// query-shape enums, never an account number.
    pub async fn bep(&self, req: &CSPAQ12300Request) -> LsResult<CSPAQ12300Response> {
        self.inner
            .post(&ls_core::endpoint_policy::CSPAQ12300_POLICY, req)
            .await
    }

    /// Inquire the account orderable-amount / total-valuation via `CSPAQ22200`.
    ///
    /// Dispatches through plain [`ls_core::Inner::post`] (Account rate bucket,
    /// single-page). The account is the config-supplied [`Account::account_no`],
    /// identified by the bearer token — the caller passes only `BalCreTp`, never an
    /// account number.
    pub async fn orderable(&self, req: &CSPAQ22200Request) -> LsResult<CSPAQ22200Response> {
        self.inner
            .post(&ls_core::endpoint_policy::CSPAQ22200_POLICY, req)
            .await
    }
}
