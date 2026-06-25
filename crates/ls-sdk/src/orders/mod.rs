//! Orders dependency class — the first order-execution surface.
//!
//! This is the *order* class: a credentialed, irreversible market action, not a
//! read. Its defining facets are `owner_class: orders` and `is_order: true`, so
//! [`CSPAT00601`](Orders::submit) routes EXCLUSIVELY through
//! [`ls_core::Inner::post_order`] — the no-retry / dedup / kill-switch dispatch
//! path — never `post`/`post_paginated`. `guard_order` rejects any attempt to
//! route a non-order policy here, and the runtime rejects routing this order
//! policy through a non-order path.
//!
//! Earns Implemented through a guarded **live paper order** plus a `t0425`
//! reconciliation read (order-safety §4), NOT the automated Paper Live Smoke that
//! every read-only TR uses. The automated gate proves order *logic* against mocks
//! and never submits a live order.
//!
//! ## CSPAT00601 — 현물 정규주문 (domestic cash-equity order submit)
//!
//! The request carries the nine required `CSPAT00601InBlock1` fields read from the
//! raw capture. The numeric request fields `OrdQty` and `OrdPrc` serialize as JSON
//! **numbers** via [`ls_core::string_as_number`]; a quoted numeric request field
//! makes the gateway return `IGW40011` (KTD6). The four `strong_order_fields`
//! (`IsuNo`, `BnsTpCode`, `OrdQty`, `OrdPrc`) are the dedup-identity subset — not
//! the full body, which is what the dedup key actually hashes.
//!
//! ## Response shape and order number
//!
//! Both `CSPAT00601OutBlock1` (the request echo) and `CSPAT00601OutBlock2` (the
//! order-result block) are SINGLE objects in the raw capture (`A0003`), confirmed
//! by the response example — so they are modeled as single structs, not arrays.
//! `OutBlock2.OrdNo` is the live submit's order number (the `t0425` reconciliation
//! key); `SpareOrdNo`/`RsvOrdNo` are auxiliary order numbers, not the live one.
//! Account-sensitive response fields (`AcntNo`, `AcntNm`) are redacted in `Debug`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use ls_core::{Inner, LsResult};

/// Input block for `CSPAT00601` — the nine required order fields.
///
/// `OrdQty`/`OrdPrc` carry [`ls_core::string_as_number`]: the gateway requires
/// them as JSON numbers (KTD6). The remaining fields are wire strings.
#[derive(Serialize, Debug, Clone)]
pub struct CSPAT00601InBlock1 {
    /// Issue (symbol) number / 종목번호.
    #[serde(rename = "IsuNo")]
    pub isuno: String,
    /// Order quantity / 주문수량 (serialized as a JSON number).
    #[serde(rename = "OrdQty", serialize_with = "ls_core::string_as_number")]
    pub ordqty: String,
    /// Order price / 주문가 (serialized as a JSON number).
    #[serde(rename = "OrdPrc", serialize_with = "ls_core::string_as_number")]
    pub ordprc: String,
    /// Buy/sell distinction / 매매구분 (`"1"` sell, `"2"` buy).
    #[serde(rename = "BnsTpCode")]
    pub bnstpcode: String,
    /// Order-price pattern code / 호가유형코드 (e.g. `"00"` limit).
    #[serde(rename = "OrdprcPtnCode")]
    pub ordprcptncode: String,
    /// Credit-transaction code / 신용거래코드 (e.g. `"000"`).
    #[serde(rename = "MgntrnCode")]
    pub mgntrncode: String,
    /// Loan date / 대출일 (empty for a cash order).
    #[serde(rename = "LoanDt")]
    pub loandt: String,
    /// Order-condition distinction / 주문조건구분 (e.g. `"0"`).
    #[serde(rename = "OrdCndiTpCode")]
    pub ordcnditpcode: String,
    /// Member-firm number / 회원사번호 (e.g. `"NXT"`).
    #[serde(rename = "MbrNo")]
    pub mbrno: String,
}

/// `CSPAT00601` request — wraps the input block under the `CSPAT00601InBlock1`
/// key, serializing to `{"CSPAT00601InBlock1":{...}}`. Not paginated: it
/// dispatches once via [`ls_core::Inner::post_order`] with no continuation.
#[derive(Serialize, Debug, Clone)]
pub struct CSPAT00601Request {
    #[serde(rename = "CSPAT00601InBlock1")]
    pub inblock: CSPAT00601InBlock1,
}

impl CSPAT00601Request {
    /// Build a domestic cash-equity order submit.
    ///
    /// `ordqty`/`ordprc` are passed as decimal strings (e.g. `"1"`, `"60000"`)
    /// and serialize as JSON numbers. `bnstpcode` is `"1"` sell / `"2"` buy.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        isuno: impl Into<String>,
        ordqty: impl Into<String>,
        ordprc: impl Into<String>,
        bnstpcode: impl Into<String>,
        ordprcptncode: impl Into<String>,
        mgntrncode: impl Into<String>,
        loandt: impl Into<String>,
        ordcnditpcode: impl Into<String>,
        mbrno: impl Into<String>,
    ) -> Self {
        CSPAT00601Request {
            inblock: CSPAT00601InBlock1 {
                isuno: isuno.into(),
                ordqty: ordqty.into(),
                ordprc: ordprc.into(),
                bnstpcode: bnstpcode.into(),
                ordprcptncode: ordprcptncode.into(),
                mgntrncode: mgntrncode.into(),
                loandt: loandt.into(),
                ordcnditpcode: ordcnditpcode.into(),
                mbrno: mbrno.into(),
            },
        }
    }

    /// A plain limit order with conventional defaults: `OrdprcPtnCode="00"`
    /// (limit), `MgntrnCode="000"` (cash), empty `LoanDt`, `OrdCndiTpCode="0"`,
    /// and the given member number. `bnstpcode` is `"1"` sell / `"2"` buy.
    pub fn limit(
        isuno: impl Into<String>,
        ordqty: impl Into<String>,
        ordprc: impl Into<String>,
        bnstpcode: impl Into<String>,
        mbrno: impl Into<String>,
    ) -> Self {
        Self::new(
            isuno, ordqty, ordprc, bnstpcode, "00", "000", "", "0", mbrno,
        )
    }
}

/// `CSPAT00601OutBlock1` — the request-echo block (single object).
///
/// Every value uses [`ls_core::string_or_number`]; account-sensitive fields are
/// redacted in `Debug`.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CSPAT00601OutBlock1 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Account number / 계좌번호 (account-sensitive; redacted in Debug).
    #[serde(rename = "AcntNo", deserialize_with = "ls_core::string_or_number")]
    pub acntno: String,
    /// Issue (symbol) number / 종목번호.
    #[serde(rename = "IsuNo", deserialize_with = "ls_core::string_or_number")]
    pub isuno: String,
    /// Order quantity / 주문수량.
    #[serde(rename = "OrdQty", deserialize_with = "ls_core::string_or_number")]
    pub ordqty: String,
    /// Order price / 주문가.
    #[serde(rename = "OrdPrc", deserialize_with = "ls_core::string_or_number")]
    pub ordprc: String,
    /// Buy/sell distinction / 매매구분.
    #[serde(rename = "BnsTpCode", deserialize_with = "ls_core::string_or_number")]
    pub bnstpcode: String,
}

impl std::fmt::Debug for CSPAT00601OutBlock1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CSPAT00601OutBlock1")
            .field("reccnt", &self.reccnt)
            .field("acntno", &"<redacted>")
            .field("isuno", &self.isuno)
            .field("ordqty", &self.ordqty)
            .field("ordprc", &self.ordprc)
            .field("bnstpcode", &self.bnstpcode)
            .finish()
    }
}

/// `CSPAT00601OutBlock2` — the order-result block (single object).
///
/// `OrdNo` is the live submit's order number — the reconciliation key matched
/// against `t0425`. `SpareOrdNo`/`RsvOrdNo` are auxiliary order numbers, NOT the
/// live order number. Account-sensitive `AcntNm` is redacted in `Debug`.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CSPAT00601OutBlock2 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Order number / 주문번호 — the live submit's order number.
    #[serde(rename = "OrdNo", deserialize_with = "ls_core::string_or_number")]
    pub ordno: String,
    /// Order time / 주문시각.
    #[serde(rename = "OrdTime", deserialize_with = "ls_core::string_or_number")]
    pub ordtime: String,
    /// Order market code / 주문시장코드.
    #[serde(rename = "OrdMktCode", deserialize_with = "ls_core::string_or_number")]
    pub ordmktcode: String,
    /// Short issue number / 단축종목번호.
    #[serde(rename = "ShtnIsuNo", deserialize_with = "ls_core::string_or_number")]
    pub shtnisuno: String,
    /// Order amount / 주문금액.
    #[serde(rename = "OrdAmt", deserialize_with = "ls_core::string_or_number")]
    pub ordamt: String,
    /// Spare order number / 예비주문번호 (auxiliary, not the live order number).
    #[serde(rename = "SpareOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub spareordno: String,
    /// Reserved order number / 예약주문번호 (auxiliary, not the live order number).
    #[serde(rename = "RsvOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub rsvordno: String,
    /// Account name / 계좌명 (account-sensitive; redacted in Debug).
    #[serde(rename = "AcntNm", deserialize_with = "ls_core::string_or_number")]
    pub acntnm: String,
    /// Issue name / 종목명.
    #[serde(rename = "IsuNm", deserialize_with = "ls_core::string_or_number")]
    pub isunm: String,
}

impl std::fmt::Debug for CSPAT00601OutBlock2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CSPAT00601OutBlock2")
            .field("reccnt", &self.reccnt)
            .field("ordno", &self.ordno)
            .field("ordtime", &self.ordtime)
            .field("ordmktcode", &self.ordmktcode)
            .field("shtnisuno", &self.shtnisuno)
            .field("ordamt", &self.ordamt)
            .field("spareordno", &self.spareordno)
            .field("rsvordno", &self.rsvordno)
            .field("acntnm", &"<redacted>")
            .field("isunm", &self.isunm)
            .finish()
    }
}

/// `CSPAT00601` response envelope.
///
/// `rsp_cd`/`rsp_msg` are the LS business-status fields (classified by the order
/// predicate in `ls-core` dispatch before this struct is built — `00040` buy-ack
/// / `00039` sell-ack are Accepted). `outblock1` echoes the request; `outblock2`
/// carries the order number. Derives `Serialize` so the dedup cache can round-trip
/// it.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CSPAT00601Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "CSPAT00601OutBlock1", default)]
    pub outblock1: CSPAT00601OutBlock1,
    #[serde(rename = "CSPAT00601OutBlock2", default)]
    pub outblock2: CSPAT00601OutBlock2,
}

impl CSPAT00601Response {
    /// The live submit's order number (`OutBlock2.OrdNo`), the reconciliation key.
    pub fn order_no(&self) -> &str {
        &self.outblock2.ordno
    }
}

/// The orders dependency class handle.
pub struct Orders {
    inner: Arc<Inner>,
}

impl Orders {
    /// Wrap a shared runtime core.
    pub fn new(inner: Arc<Inner>) -> Self {
        Orders { inner }
    }

    /// The config-supplied account number this handle operates on. The account is
    /// established by config and the credentialed token, never a caller field.
    pub fn account_no(&self) -> &str {
        &self.inner.config.account_no
    }

    /// Submit a domestic cash-equity order via `CSPAT00601`.
    ///
    /// Routes EXCLUSIVELY through [`ls_core::Inner::post_order`]: a single HTTP
    /// attempt (no retry — an ambiguous timeout is reconciled, never resubmitted),
    /// gated by the kill switch and the deduplication window, charging the
    /// `Orders` rate bucket. A `00040`/`00039` ack returns Accepted; a rejection
    /// surfaces as [`ls_core::LsError::ApiError`] with the broker code/message; an
    /// ambiguous outcome surfaces as [`ls_core::LsError::AmbiguousOrder`] for the
    /// caller to reconcile via `t0425`.
    pub async fn submit(&self, req: &CSPAT00601Request) -> LsResult<CSPAT00601Response> {
        self.inner
            .post_order(&ls_core::endpoint_policy::CSPAT00601_POLICY, req)
            .await
    }
}
