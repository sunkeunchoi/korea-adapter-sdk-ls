//! Domestic futures/options (F/O) order surface — the `CFOAT` order chain.
//!
//! The F/O sibling of the domestic-stock `CSPAT` chain in [`super`]. Like every
//! order it is a credentialed, irreversible market action (`owner_class: orders`,
//! `is_order: true`), so [`FoOrders::submit`]/[`FoOrders::modify`]/[`FoOrders::cancel`]
//! route EXCLUSIVELY through [`ls_core::Inner::post_order`] — the no-retry / dedup /
//! kill-switch path — never `post`/`post_paginated`.
//!
//! The F/O legs reuse the domestic-stock order runtime verbatim (R5): the
//! [`OrderAction`]/`org_order_no` reconcile seam (via the `reconcile_intent`
//! helpers), the dedup window, the kill switch, and the
//! [`ls_core::is_paper_order_incapable`] `01491` classifier. What differs is the
//! F/O request shape — `FnoIsuNo`/`MdfyQty`/`CancQty` and a `FnoOrdPrc` that can be
//! fractional — read from the normalized baseline
//! (`crates/ls-trackers/baselines/api-drift/normalized/trs/CFOAT00100.json`).
//!
//! ## Numeric request fields (IGW40011)
//!
//! `FnoOrdPrc`, `OrdQty`, `OrgOrdNo`, `MdfyQty`, and `CancQty` are numbers on the
//! wire (raw `propertyType` `A0004`) and serialize as JSON **numbers**; a quoted
//! numeric request field makes the gateway return `IGW40011` (see AGENTS.md Gotchas).
//! The integer fields (`OrdQty`/`OrgOrdNo`/`MdfyQty`/`CancQty`) use
//! [`ls_core::string_as_number`]; `FnoOrdPrc` is fractional (e.g. `"342.25"`) so it
//! uses [`ls_core::string_as_decimal`], which emits a decimal as a JSON `f64` rather
//! than quoting it.
//!
//! ## Response shape and order number
//!
//! For all three TRs both out-blocks are SINGLE objects in the raw capture (`A0003`).
//! `OutBlock2.OrdNo` is the live order number (the reconciliation key). Unlike the
//! `CSPAT` modify/cancel, the F/O modify/cancel `OutBlock2` carries **no** `PrntOrdNo`
//! — the parent order number is echoed in `OutBlock1.OrgOrdNo` instead, surfaced via
//! `parent_order_no()`. Account-sensitive response fields (`AcntNo`, `AcntNm`, `Pwd`)
//! are redacted in `Debug` and never serialized into the dedup cache.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use ls_core::{Inner, LsResult};

use super::{normalize_symbol, OrderIntent};

// ===========================================================================
// CFOAT00100 — 선물옵션 정상주문 (domestic F/O order SUBMIT).
// ===========================================================================

/// Input block for `CFOAT00100` — the five required F/O submit fields.
///
/// `OrdQty` carries [`ls_core::string_as_number`]; `FnoOrdPrc` is fractional so it
/// carries [`ls_core::string_as_decimal`] (both JSON numbers, IGW40011).
#[derive(Serialize, Debug, Clone)]
pub struct CFOAT00100InBlock1 {
    /// F/O issue (contract) number / 선물옵션종목번호.
    #[serde(rename = "FnoIsuNo")]
    pub fnoisuno: String,
    /// Buy/sell distinction / 매매구분 (`"1"` sell, `"2"` buy).
    #[serde(rename = "BnsTpCode")]
    pub bnstpcode: String,
    /// F/O order-price pattern code / 선물옵션호가유형코드 (e.g. `"00"` limit).
    #[serde(rename = "FnoOrdprcPtnCode")]
    pub fnoordprcptncode: String,
    /// F/O order price / 선물옵션주문가격 (fractional; serialized as a JSON number).
    #[serde(rename = "FnoOrdPrc", serialize_with = "ls_core::string_as_decimal")]
    pub fnoordprc: String,
    /// Order quantity / 주문수량 (serialized as a JSON number).
    #[serde(rename = "OrdQty", serialize_with = "ls_core::string_as_number")]
    pub ordqty: String,
}

/// `CFOAT00100` request — wraps the input block under the `CFOAT00100InBlock1` key,
/// serializing to `{"CFOAT00100InBlock1":{...}}`. Dispatches once via
/// [`ls_core::Inner::post_order`] with no continuation.
#[derive(Serialize, Debug, Clone)]
pub struct CFOAT00100Request {
    #[serde(rename = "CFOAT00100InBlock1")]
    pub inblock: CFOAT00100InBlock1,
}

impl CFOAT00100Request {
    /// Build a domestic F/O order submit. `ordqty` is a decimal string (e.g. `"1"`);
    /// `fnoordprc` is a decimal string (e.g. `"342.25"`). `bnstpcode` is `"1"` sell /
    /// `"2"` buy.
    pub fn new(
        fnoisuno: impl Into<String>,
        ordqty: impl Into<String>,
        fnoordprc: impl Into<String>,
        bnstpcode: impl Into<String>,
        fnoordprcptncode: impl Into<String>,
    ) -> Self {
        CFOAT00100Request {
            inblock: CFOAT00100InBlock1 {
                fnoisuno: fnoisuno.into(),
                bnstpcode: bnstpcode.into(),
                fnoordprcptncode: fnoordprcptncode.into(),
                fnoordprc: fnoordprc.into(),
                ordqty: ordqty.into(),
            },
        }
    }

    /// A plain F/O limit order with the conventional `FnoOrdprcPtnCode="00"` (limit).
    /// `bnstpcode` is `"1"` sell / `"2"` buy.
    pub fn limit(
        fnoisuno: impl Into<String>,
        ordqty: impl Into<String>,
        fnoordprc: impl Into<String>,
        bnstpcode: impl Into<String>,
    ) -> Self {
        Self::new(fnoisuno, ordqty, fnoordprc, bnstpcode, "00")
    }
}

/// `CFOAT00100OutBlock1` — the request-echo block (single object).
///
/// A representative subset; account-sensitive `AcntNo`/`Pwd` are redacted in `Debug`
/// and never serialized.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CFOAT00100OutBlock1 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Order market code / 주문시장코드.
    #[serde(rename = "OrdMktCode", deserialize_with = "ls_core::string_or_number")]
    pub ordmktcode: String,
    /// Account number / 계좌번호 (account-sensitive; redacted + never serialized).
    #[serde(
        rename = "AcntNo",
        deserialize_with = "ls_core::string_or_number",
        skip_serializing
    )]
    pub acntno: String,
    /// Password / 비밀번호 (gateway-masked, still sensitive; redacted + not serialized).
    #[serde(
        rename = "Pwd",
        deserialize_with = "ls_core::string_or_number",
        skip_serializing
    )]
    pub pwd: String,
    /// F/O issue (contract) number / 선물옵션종목번호.
    #[serde(rename = "FnoIsuNo", deserialize_with = "ls_core::string_or_number")]
    pub fnoisuno: String,
    /// Buy/sell distinction / 매매구분.
    #[serde(rename = "BnsTpCode", deserialize_with = "ls_core::string_or_number")]
    pub bnstpcode: String,
    /// F/O order price / 선물옵션주문가격.
    #[serde(rename = "FnoOrdPrc", deserialize_with = "ls_core::string_or_number")]
    pub fnoordprc: String,
    /// Order quantity / 주문수량.
    #[serde(rename = "OrdQty", deserialize_with = "ls_core::string_or_number")]
    pub ordqty: String,
}

impl std::fmt::Debug for CFOAT00100OutBlock1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CFOAT00100OutBlock1")
            .field("reccnt", &self.reccnt)
            .field("ordmktcode", &self.ordmktcode)
            .field("acntno", &"<redacted>")
            .field("pwd", &"<redacted>")
            .field("fnoisuno", &self.fnoisuno)
            .field("bnstpcode", &self.bnstpcode)
            .field("fnoordprc", &self.fnoordprc)
            .field("ordqty", &self.ordqty)
            .finish()
    }
}

/// `CFOAT00100OutBlock2` — the order-result block (single object).
///
/// `OrdNo` is the live submit's order number (the reconciliation key).
/// Account-sensitive `AcntNm` is redacted in `Debug` and never serialized.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CFOAT00100OutBlock2 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Order number / 주문번호 — the live submit's order number.
    #[serde(rename = "OrdNo", deserialize_with = "ls_core::string_or_number")]
    pub ordno: String,
    /// Issue name / 종목명.
    #[serde(rename = "IsuNm", deserialize_with = "ls_core::string_or_number")]
    pub isunm: String,
    /// Account name / 계좌명 (account-sensitive; redacted + never serialized).
    #[serde(
        rename = "AcntNm",
        deserialize_with = "ls_core::string_or_number",
        skip_serializing
    )]
    pub acntnm: String,
    /// Order margin / 주문증거금.
    #[serde(rename = "OrdMgn", deserialize_with = "ls_core::string_or_number")]
    pub ordmgn: String,
    /// Orderable quantity / 주문가능수량.
    #[serde(rename = "OrdAbleQty", deserialize_with = "ls_core::string_or_number")]
    pub ordableqty: String,
    /// Orderable amount / 주문가능금액.
    #[serde(rename = "OrdAbleAmt", deserialize_with = "ls_core::string_or_number")]
    pub ordableamt: String,
    /// Branch name / 지점명.
    #[serde(rename = "BrnNm", deserialize_with = "ls_core::string_or_number")]
    pub brnnm: String,
}

impl std::fmt::Debug for CFOAT00100OutBlock2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CFOAT00100OutBlock2")
            .field("reccnt", &self.reccnt)
            .field("ordno", &self.ordno)
            .field("isunm", &self.isunm)
            .field("acntnm", &"<redacted>")
            .field("ordmgn", &self.ordmgn)
            .field("ordableqty", &self.ordableqty)
            .field("ordableamt", &self.ordableamt)
            .field("brnnm", &self.brnnm)
            .finish()
    }
}

/// `CFOAT00100` response envelope.
///
/// `rsp_cd`/`rsp_msg` are classified by the order predicate in `ls-core` dispatch
/// before this struct is built (`00040` buy-ack / `00039` sell-ack are Accepted).
/// Derives `Serialize` so the dedup cache can round-trip it.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CFOAT00100Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "CFOAT00100OutBlock1", default)]
    pub outblock1: CFOAT00100OutBlock1,
    #[serde(rename = "CFOAT00100OutBlock2", default)]
    pub outblock2: CFOAT00100OutBlock2,
}

impl CFOAT00100Response {
    /// The live submit's order number (`OutBlock2.OrdNo`), the reconciliation key.
    pub fn order_no(&self) -> &str {
        &self.outblock2.ordno
    }
}

// ===========================================================================
// CFOAT00200 — 선물옵션 정정주문 (domestic F/O order MODIFY).
//
// References an existing order via `OrgOrdNo` and carries the full target
// `FnoOrdPrc`/`MdfyQty` (absolute, no delta — like the CSPAT modify), so a blind
// re-send re-applies the same target. `OutBlock2.OrdNo` is a NEW order number; the
// parent (`OrgOrdNo`) is echoed in `OutBlock1`, not `OutBlock2`. Success rsp_cd is
// `00132` (정정주문 완료) per the raw capture.
// ===========================================================================

/// Input block for `CFOAT00200` — the five required F/O modify fields.
///
/// `OrgOrdNo`/`MdfyQty` carry [`ls_core::string_as_number`]; `FnoOrdPrc` carries
/// [`ls_core::string_as_decimal`] (all JSON numbers, IGW40011).
#[derive(Serialize, Debug, Clone)]
pub struct CFOAT00200InBlock1 {
    /// F/O issue (contract) number / 선물옵션종목번호.
    #[serde(rename = "FnoIsuNo")]
    pub fnoisuno: String,
    /// Original order number / 원주문번호 — the order being modified (JSON number).
    #[serde(rename = "OrgOrdNo", serialize_with = "ls_core::string_as_number")]
    pub orgordno: String,
    /// F/O order-price pattern code / 선물옵션호가유형코드 (e.g. `"00"` limit).
    #[serde(rename = "FnoOrdprcPtnCode")]
    pub fnoordprcptncode: String,
    /// New F/O order price / 선물옵션주문가격 (absolute target; JSON number).
    #[serde(rename = "FnoOrdPrc", serialize_with = "ls_core::string_as_decimal")]
    pub fnoordprc: String,
    /// Modify quantity / 정정수량 (absolute target; JSON number).
    #[serde(rename = "MdfyQty", serialize_with = "ls_core::string_as_number")]
    pub mdfyqty: String,
}

/// `CFOAT00200` request — wraps the input block under the `CFOAT00200InBlock1` key.
#[derive(Serialize, Debug, Clone)]
pub struct CFOAT00200Request {
    #[serde(rename = "CFOAT00200InBlock1")]
    pub inblock: CFOAT00200InBlock1,
}

impl CFOAT00200Request {
    /// Build a domestic F/O order modify against an existing order number. The modify
    /// is absolute (it carries the full target `FnoOrdPrc`/`MdfyQty`).
    pub fn new(
        orgordno: impl Into<String>,
        fnoisuno: impl Into<String>,
        mdfyqty: impl Into<String>,
        fnoordprc: impl Into<String>,
        fnoordprcptncode: impl Into<String>,
    ) -> Self {
        CFOAT00200Request {
            inblock: CFOAT00200InBlock1 {
                fnoisuno: fnoisuno.into(),
                orgordno: orgordno.into(),
                fnoordprcptncode: fnoordprcptncode.into(),
                fnoordprc: fnoordprc.into(),
                mdfyqty: mdfyqty.into(),
            },
        }
    }

    /// A plain F/O limit modify with the conventional `FnoOrdprcPtnCode="00"` (limit).
    pub fn limit(
        orgordno: impl Into<String>,
        fnoisuno: impl Into<String>,
        mdfyqty: impl Into<String>,
        fnoordprc: impl Into<String>,
    ) -> Self {
        Self::new(orgordno, fnoisuno, mdfyqty, fnoordprc, "00")
    }

    /// Build the reconciliation intent for this modify, keyed off the referenced
    /// `OrgOrdNo` (R5). Reuses the domestic-stock [`OrderIntent::modify`] seam; the
    /// matcher keys off `org_order_no`, not the new order number.
    pub fn reconcile_intent(&self, account_no: impl Into<String>) -> OrderIntent {
        OrderIntent::modify(
            account_no,
            normalize_symbol(&self.inblock.fnoisuno),
            "", // a modify request carries no side; reconciliation keys off OrgOrdNo
            self.inblock.mdfyqty.clone(),
            self.inblock.fnoordprc.clone(),
            self.inblock.orgordno.clone(),
        )
    }
}

/// `CFOAT00200OutBlock1` — the request-echo block (single object). Carries the parent
/// (`OrgOrdNo`). Account-sensitive `AcntNo`/`Pwd` redacted + never serialized.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CFOAT00200OutBlock1 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Order market code / 주문시장코드.
    #[serde(rename = "OrdMktCode", deserialize_with = "ls_core::string_or_number")]
    pub ordmktcode: String,
    /// Account number / 계좌번호 (account-sensitive; redacted + never serialized).
    #[serde(
        rename = "AcntNo",
        deserialize_with = "ls_core::string_or_number",
        skip_serializing
    )]
    pub acntno: String,
    /// Password / 비밀번호 (redacted + not serialized).
    #[serde(
        rename = "Pwd",
        deserialize_with = "ls_core::string_or_number",
        skip_serializing
    )]
    pub pwd: String,
    /// F/O issue (contract) number / 선물옵션종목번호.
    #[serde(rename = "FnoIsuNo", deserialize_with = "ls_core::string_or_number")]
    pub fnoisuno: String,
    /// Original (parent) order number / 원주문번호.
    #[serde(rename = "OrgOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub orgordno: String,
    /// New F/O order price / 선물옵션주문가격.
    #[serde(rename = "FnoOrdPrc", deserialize_with = "ls_core::string_or_number")]
    pub fnoordprc: String,
    /// Modify quantity / 정정수량.
    #[serde(rename = "MdfyQty", deserialize_with = "ls_core::string_or_number")]
    pub mdfyqty: String,
}

impl std::fmt::Debug for CFOAT00200OutBlock1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CFOAT00200OutBlock1")
            .field("reccnt", &self.reccnt)
            .field("ordmktcode", &self.ordmktcode)
            .field("acntno", &"<redacted>")
            .field("pwd", &"<redacted>")
            .field("fnoisuno", &self.fnoisuno)
            .field("orgordno", &self.orgordno)
            .field("fnoordprc", &self.fnoordprc)
            .field("mdfyqty", &self.mdfyqty)
            .finish()
    }
}

/// `CFOAT00200OutBlock2` — the order-result block (single object). `OrdNo` is the
/// NEW order number; the parent is in `OutBlock1.OrgOrdNo`. `AcntNm` redacted.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CFOAT00200OutBlock2 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// New order number / 주문번호 — the modify's new order number.
    #[serde(rename = "OrdNo", deserialize_with = "ls_core::string_or_number")]
    pub ordno: String,
    /// Issue name / 종목명.
    #[serde(rename = "IsuNm", deserialize_with = "ls_core::string_or_number")]
    pub isunm: String,
    /// Account name / 계좌명 (account-sensitive; redacted + never serialized).
    #[serde(
        rename = "AcntNm",
        deserialize_with = "ls_core::string_or_number",
        skip_serializing
    )]
    pub acntnm: String,
    /// Order margin / 주문증거금.
    #[serde(rename = "OrdMgn", deserialize_with = "ls_core::string_or_number")]
    pub ordmgn: String,
    /// Orderable quantity / 주문가능수량.
    #[serde(rename = "OrdAbleQty", deserialize_with = "ls_core::string_or_number")]
    pub ordableqty: String,
}

impl std::fmt::Debug for CFOAT00200OutBlock2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CFOAT00200OutBlock2")
            .field("reccnt", &self.reccnt)
            .field("ordno", &self.ordno)
            .field("isunm", &self.isunm)
            .field("acntnm", &"<redacted>")
            .field("ordmgn", &self.ordmgn)
            .field("ordableqty", &self.ordableqty)
            .finish()
    }
}

/// `CFOAT00200` response envelope. `rsp_cd` `00132` (정정주문 완료) is Accepted.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CFOAT00200Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "CFOAT00200OutBlock1", default)]
    pub outblock1: CFOAT00200OutBlock1,
    #[serde(rename = "CFOAT00200OutBlock2", default)]
    pub outblock2: CFOAT00200OutBlock2,
}

impl CFOAT00200Response {
    /// The modify's NEW order number (`OutBlock2.OrdNo`).
    pub fn order_no(&self) -> &str {
        &self.outblock2.ordno
    }
    /// The parent (original) order number (`OutBlock1.OrgOrdNo` — the F/O modify
    /// echoes the parent in the echo block, NOT in a `PrntOrdNo` result field).
    pub fn parent_order_no(&self) -> &str {
        &self.outblock1.orgordno
    }
}

// ===========================================================================
// CFOAT00300 — 선물옵션 취소주문 (domestic F/O order CANCEL).
//
// References an existing order via `OrgOrdNo`. Cancel idempotency comes free from
// the dedup key (the full body — incl. `OrgOrdNo` — is hashed). Success rsp_cd is
// `00156` (취소주문 완료) per the raw capture.
// ===========================================================================

/// Input block for `CFOAT00300` — the three required F/O cancel fields.
///
/// `OrgOrdNo`/`CancQty` carry [`ls_core::string_as_number`] (JSON numbers, IGW40011).
#[derive(Serialize, Debug, Clone)]
pub struct CFOAT00300InBlock1 {
    /// F/O issue (contract) number / 선물옵션종목번호.
    #[serde(rename = "FnoIsuNo")]
    pub fnoisuno: String,
    /// Original order number / 원주문번호 — the order being canceled (JSON number).
    #[serde(rename = "OrgOrdNo", serialize_with = "ls_core::string_as_number")]
    pub orgordno: String,
    /// Cancel quantity / 취소수량 (JSON number).
    #[serde(rename = "CancQty", serialize_with = "ls_core::string_as_number")]
    pub cancqty: String,
}

/// `CFOAT00300` request — wraps the input block under the `CFOAT00300InBlock1` key.
#[derive(Serialize, Debug, Clone)]
pub struct CFOAT00300Request {
    #[serde(rename = "CFOAT00300InBlock1")]
    pub inblock: CFOAT00300InBlock1,
}

impl CFOAT00300Request {
    /// Build a domestic F/O order cancel against an existing order number.
    pub fn new(
        orgordno: impl Into<String>,
        fnoisuno: impl Into<String>,
        cancqty: impl Into<String>,
    ) -> Self {
        CFOAT00300Request {
            inblock: CFOAT00300InBlock1 {
                fnoisuno: fnoisuno.into(),
                orgordno: orgordno.into(),
                cancqty: cancqty.into(),
            },
        }
    }

    /// Build the reconciliation intent for this cancel, keyed off the referenced
    /// `OrgOrdNo` (R5). Reuses the domestic-stock [`OrderIntent::cancel`] seam; the
    /// cancel-aware matcher fails toward still-live — a matched non-`취소` row is
    /// never read as success.
    pub fn reconcile_intent(&self, account_no: impl Into<String>) -> OrderIntent {
        OrderIntent::cancel(
            account_no,
            normalize_symbol(&self.inblock.fnoisuno),
            "", // a cancel request carries no side; reconciliation keys off OrgOrdNo
            self.inblock.cancqty.clone(),
            self.inblock.orgordno.clone(),
        )
    }
}

/// `CFOAT00300OutBlock1` — the request-echo block (single object). Carries the parent
/// (`OrgOrdNo`). Account-sensitive `AcntNo`/`Pwd` redacted + never serialized.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CFOAT00300OutBlock1 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Order market code / 주문시장코드.
    #[serde(rename = "OrdMktCode", deserialize_with = "ls_core::string_or_number")]
    pub ordmktcode: String,
    /// Account number / 계좌번호 (account-sensitive; redacted + never serialized).
    #[serde(
        rename = "AcntNo",
        deserialize_with = "ls_core::string_or_number",
        skip_serializing
    )]
    pub acntno: String,
    /// Password / 비밀번호 (redacted + not serialized).
    #[serde(
        rename = "Pwd",
        deserialize_with = "ls_core::string_or_number",
        skip_serializing
    )]
    pub pwd: String,
    /// F/O issue (contract) number / 선물옵션종목번호.
    #[serde(rename = "FnoIsuNo", deserialize_with = "ls_core::string_or_number")]
    pub fnoisuno: String,
    /// Original (parent) order number / 원주문번호.
    #[serde(rename = "OrgOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub orgordno: String,
    /// Cancel quantity / 취소수량.
    #[serde(rename = "CancQty", deserialize_with = "ls_core::string_or_number")]
    pub cancqty: String,
}

impl std::fmt::Debug for CFOAT00300OutBlock1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CFOAT00300OutBlock1")
            .field("reccnt", &self.reccnt)
            .field("ordmktcode", &self.ordmktcode)
            .field("acntno", &"<redacted>")
            .field("pwd", &"<redacted>")
            .field("fnoisuno", &self.fnoisuno)
            .field("orgordno", &self.orgordno)
            .field("cancqty", &self.cancqty)
            .finish()
    }
}

/// `CFOAT00300OutBlock2` — the order-result block (single object). `OrdNo` is the
/// NEW cancel order number; the parent is in `OutBlock1.OrgOrdNo`. `AcntNm` redacted.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CFOAT00300OutBlock2 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// New cancel order number / 주문번호.
    #[serde(rename = "OrdNo", deserialize_with = "ls_core::string_or_number")]
    pub ordno: String,
    /// Issue name / 종목명.
    #[serde(rename = "IsuNm", deserialize_with = "ls_core::string_or_number")]
    pub isunm: String,
    /// Account name / 계좌명 (account-sensitive; redacted + never serialized).
    #[serde(
        rename = "AcntNm",
        deserialize_with = "ls_core::string_or_number",
        skip_serializing
    )]
    pub acntnm: String,
    /// Order margin / 주문증거금.
    #[serde(rename = "OrdMgn", deserialize_with = "ls_core::string_or_number")]
    pub ordmgn: String,
    /// Orderable quantity / 주문가능수량.
    #[serde(rename = "OrdAbleQty", deserialize_with = "ls_core::string_or_number")]
    pub ordableqty: String,
}

impl std::fmt::Debug for CFOAT00300OutBlock2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CFOAT00300OutBlock2")
            .field("reccnt", &self.reccnt)
            .field("ordno", &self.ordno)
            .field("isunm", &self.isunm)
            .field("acntnm", &"<redacted>")
            .field("ordmgn", &self.ordmgn)
            .field("ordableqty", &self.ordableqty)
            .finish()
    }
}

/// `CFOAT00300` response envelope. `rsp_cd` `00156` (취소주문 완료) is Accepted.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CFOAT00300Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "CFOAT00300OutBlock1", default)]
    pub outblock1: CFOAT00300OutBlock1,
    #[serde(rename = "CFOAT00300OutBlock2", default)]
    pub outblock2: CFOAT00300OutBlock2,
}

impl CFOAT00300Response {
    /// The cancel's NEW order number (`OutBlock2.OrdNo`).
    pub fn order_no(&self) -> &str {
        &self.outblock2.ordno
    }
    /// The parent (original) order number (`OutBlock1.OrgOrdNo`).
    pub fn parent_order_no(&self) -> &str {
        &self.outblock1.orgordno
    }
}

// ===========================================================================
// FoOrders handle
// ===========================================================================

/// The domestic futures/options order handle — the F/O sibling of [`super::Orders`].
///
/// Every method routes EXCLUSIVELY through [`ls_core::Inner::post_order`] (no-retry /
/// dedup / kill-switch), never `post`/`post_paginated`.
pub struct FoOrders {
    inner: Arc<Inner>,
}

impl FoOrders {
    /// Wrap a shared runtime core.
    pub fn new(inner: Arc<Inner>) -> Self {
        FoOrders { inner }
    }

    /// The config-supplied account number this handle operates on.
    pub fn account_no(&self) -> &str {
        &self.inner.config.account_no
    }

    /// Submit a domestic F/O order via `CFOAT00100`.
    ///
    /// Single HTTP attempt (no retry — an ambiguous timeout is reconciled, never
    /// resubmitted), gated by the kill switch and the dedup window, charging the
    /// `Orders` rate bucket. A `00040`/`00039` ack returns Accepted; a rejection
    /// surfaces as [`ls_core::LsError::ApiError`]; an ambiguous outcome surfaces as
    /// [`ls_core::LsError::AmbiguousOrder`].
    pub async fn submit(&self, req: &CFOAT00100Request) -> LsResult<CFOAT00100Response> {
        self.inner
            .post_order(&ls_core::endpoint_policy::CFOAT00100_POLICY, req)
            .await
    }

    /// Modify an existing domestic F/O order via `CFOAT00200`.
    ///
    /// Same no-retry / dedup / kill-switch path as [`FoOrders::submit`]. The modify is
    /// absolute (full target `FnoOrdPrc`/`MdfyQty`): a within-window identical re-send
    /// hits the dedup cache; an ambiguous outcome is reconciled by referenced order
    /// number via [`CFOAT00200Request::reconcile_intent`], never blindly resubmitted.
    pub async fn modify(&self, req: &CFOAT00200Request) -> LsResult<CFOAT00200Response> {
        self.inner
            .post_order(&ls_core::endpoint_policy::CFOAT00200_POLICY, req)
            .await
    }

    /// Cancel an existing domestic F/O order via `CFOAT00300`.
    ///
    /// Cancel is idempotent within the dedup TTL for free (the full body incl.
    /// `OrgOrdNo` is the dedup key). An ambiguous cancel is reconciled by referenced
    /// order number via [`CFOAT00300Request::reconcile_intent`] and INVERTS the risk:
    /// never assumed successful unless reconciliation proves a `취소`.
    pub async fn cancel(&self, req: &CFOAT00300Request) -> LsResult<CFOAT00300Response> {
        self.inner
            .post_order(&ls_core::endpoint_policy::CFOAT00300_POLICY, req)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orders::OrderAction;

    // ---- CFOAT00100 submit -------------------------------------------------

    /// Happy path (R4): round-trip decode of a CFOAT00100 success response (echo
    /// OutBlock1 + order-number OutBlock2, from the raw success example) deserializes
    /// without loss and surfaces the order number.
    #[test]
    fn cfoat00100_response_decodes_echo_and_order_number() {
        let raw = serde_json::json!({
            "rsp_cd": "00040",
            "rsp_msg": "매수 주문이 완료되었습니다.",
            "CFOAT00100OutBlock1": {
                "RecCnt": 1, "OrdMktCode": "40", "AcntNo": "20001652603",
                "Pwd": "********", "FnoIsuNo": "KR4301T63220", "BnsTpCode": "2",
                "FnoOrdPrc": "2.40000000", "OrdQty": 1
            },
            "CFOAT00100OutBlock2": {
                "RecCnt": 1, "OrdNo": 69007, "IsuNm": "P 202306 322.5",
                "AcntNm": "임동무", "OrdMgn": 600000, "OrdAbleQty": 0,
                "OrdAbleAmt": 9978355752i64, "BrnNm": ""
            }
        });
        let resp: CFOAT00100Response = serde_json::from_value(raw).unwrap();
        assert_eq!(resp.rsp_cd, "00040");
        assert_eq!(resp.order_no(), "69007");
        assert_eq!(resp.outblock1.fnoisuno, "KR4301T63220");
        assert_eq!(resp.outblock1.ordqty, "1");
        // Account-sensitive fields decode but are never serialized into the dedup cache.
        assert_eq!(resp.outblock1.acntno, "20001652603");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.contains("20001652603"), "AcntNo must not serialize: {json}");
        assert!(!json.contains("임동무"), "AcntNm must not serialize: {json}");
        // Debug redacts account-sensitive fields.
        let dbg = format!("{:?}", resp.outblock1);
        assert!(dbg.contains("<redacted>") && !dbg.contains("20001652603"));
    }

    /// Edge (R4, IGW40011): the numeric request fields FnoOrdPrc/OrdQty serialize as
    /// JSON numbers (not strings); FnoIsuNo/BnsTpCode stay strings.
    #[test]
    fn cfoat00100_request_serializes_numeric_fields_as_json_numbers() {
        let req = CFOAT00100Request::limit("101T9000", "5", "342.25", "2");
        let v: serde_json::Value = serde_json::to_value(&req).unwrap();
        let inb = &v["CFOAT00100InBlock1"];
        assert!(inb["FnoOrdPrc"].is_number(), "FnoOrdPrc must be a JSON number");
        assert!(inb["OrdQty"].is_number(), "OrdQty must be a JSON number");
        assert_eq!(inb["FnoOrdPrc"].as_f64().unwrap(), 342.25);
        assert_eq!(inb["OrdQty"].as_i64().unwrap(), 5);
        assert!(inb["FnoIsuNo"].is_string());
        assert_eq!(inb["FnoOrdprcPtnCode"], "00");
    }

    // ---- CFOAT00200 modify -------------------------------------------------

    /// Happy path (R4): round-trip decode of a CFOAT00200 modify response — the NEW
    /// order number (OutBlock2.OrdNo) is distinct from the parent (OutBlock1.OrgOrdNo).
    #[test]
    fn cfoat00200_response_decodes_new_and_parent_order_numbers() {
        let raw = serde_json::json!({
            "rsp_cd": "00132",
            "rsp_msg": "정정주문이 완료되었습니다.",
            "CFOAT00200OutBlock1": {
                "RecCnt": 1, "OrdMktCode": "40", "AcntNo": "20277932702",
                "Pwd": "********", "FnoIsuNo": "KR4101T60006", "OrgOrdNo": 69039,
                "FnoOrdPrc": "342.30000000", "MdfyQty": 1
            },
            "CFOAT00200OutBlock2": {
                "RecCnt": 1, "OrdNo": 69041, "IsuNm": "F 202306", "AcntNm": "충조감",
                "OrdMgn": 50748360, "OrdAbleQty": 0
            }
        });
        let resp: CFOAT00200Response = serde_json::from_value(raw).unwrap();
        assert_eq!(resp.rsp_cd, "00132");
        assert_eq!(resp.order_no(), "69041", "new order number from OutBlock2");
        assert_eq!(resp.parent_order_no(), "69039", "parent from OutBlock1.OrgOrdNo");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.contains("20277932702"), "AcntNo must not serialize");
    }

    /// Covers R5: a modify request carries OrgOrdNo from the submit and the revised
    /// price as a JSON number; the reconcile intent keys off the referenced OrgOrdNo.
    #[test]
    fn cfoat00200_request_carries_orgordno_numeric_and_reconcile_intent() {
        let req = CFOAT00200Request::limit("67005", "101T9000", "1", "342.3");
        let v: serde_json::Value = serde_json::to_value(&req).unwrap();
        let inb = &v["CFOAT00200InBlock1"];
        assert!(inb["OrgOrdNo"].is_number(), "OrgOrdNo must be a JSON number");
        assert!(inb["FnoOrdPrc"].is_number(), "FnoOrdPrc must be a JSON number");
        assert!(inb["MdfyQty"].is_number(), "MdfyQty must be a JSON number");
        assert_eq!(inb["OrgOrdNo"].as_i64().unwrap(), 67005);
        let intent = req.reconcile_intent("ACCT");
        assert_eq!(intent.action, OrderAction::Modify);
        assert_eq!(intent.org_order_no.as_deref(), Some("67005"));
        assert_eq!(intent.symbol, "101T9000");
    }

    // ---- CFOAT00300 cancel -------------------------------------------------

    /// Happy path (R4): round-trip decode of a CFOAT00300 cancel response.
    #[test]
    fn cfoat00300_response_decodes_order_number() {
        let raw = serde_json::json!({
            "rsp_cd": "00156",
            "rsp_msg": "취소주문이 완료되었습니다.",
            "CFOAT00300OutBlock1": {
                "RecCnt": 1, "OrdMktCode": "40", "AcntNo": "20277932702",
                "Pwd": "********", "FnoIsuNo": "101T6000", "OrgOrdNo": 69043,
                "CancQty": 2
            },
            "CFOAT00300OutBlock2": {
                "RecCnt": 1, "OrdNo": 69044, "IsuNm": "F 202306", "AcntNm": "충조감",
                "OrdMgn": 0, "OrdAbleQty": 0
            }
        });
        let resp: CFOAT00300Response = serde_json::from_value(raw).unwrap();
        assert_eq!(resp.rsp_cd, "00156");
        assert_eq!(resp.order_no(), "69044");
        assert_eq!(resp.parent_order_no(), "69043");
    }

    /// Covers R5: a cancel request built from a submit's order number carries that
    /// original order number in the F/O cancel `OrgOrdNo` field (as a JSON number),
    /// and the reconcile intent is a cancel keyed off it.
    #[test]
    fn cfoat00300_cancel_request_carries_submit_order_number() {
        let submit_ordno = "69043"; // the OrdNo a CFOAT00100 submit returned
        let req = CFOAT00300Request::new(submit_ordno, "101T6000", "2");
        let v: serde_json::Value = serde_json::to_value(&req).unwrap();
        let inb = &v["CFOAT00300InBlock1"];
        assert!(inb["OrgOrdNo"].is_number(), "OrgOrdNo must be a JSON number");
        assert!(inb["CancQty"].is_number(), "CancQty must be a JSON number");
        assert_eq!(inb["OrgOrdNo"].as_i64().unwrap(), 69043);
        let intent = req.reconcile_intent("ACCT");
        assert_eq!(intent.action, OrderAction::Cancel);
        assert_eq!(intent.org_order_no.as_deref(), Some("69043"));
    }
}
