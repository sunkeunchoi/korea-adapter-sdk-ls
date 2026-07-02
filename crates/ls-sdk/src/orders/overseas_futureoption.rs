//! Overseas futures/options order surface — the `CIDBT` order chain.
//!
//! The overseas-futures sibling of the domestic F/O `CFOAT` chain in
//! [`super::futureoption`]. Like every order it is a credentialed, irreversible
//! market action (`owner_class: orders`, `is_order: true`), so
//! [`OverseasFoOrders::submit`]/[`OverseasFoOrders::modify`]/[`OverseasFoOrders::cancel`]
//! route EXCLUSIVELY through [`ls_core::Inner::post_order`] — the no-retry / dedup /
//! kill-switch path — never `post`/`post_paginated`.
//!
//! The legs reuse the domestic order runtime verbatim (the dedup window, the kill
//! switch, and the [`ls_core::is_paper_order_incapable`] `01491` classifier). What
//! differs is the wire shape, read from the normalized baselines
//! (`crates/ls-trackers/baselines/api-drift/normalized/trs/CIDBT001{00,900,000}.json`).
//!
//! ## Numeric request fields (IGW40011)
//!
//! The price fields `OvrsDrvtOrdPrc` / `CndiOrdPrc` are `Number` on the wire and are
//! FRACTIONAL (overseas prices carry ticks like `4213.25`), so they serialize as JSON
//! numbers via [`ls_core::string_as_decimal`] (KTD1). `OrdQty` is an integer `Number`
//! → [`ls_core::string_as_number`]. A quoted numeric request field makes the gateway
//! return `IGW40011` (see AGENTS.md Gotchas). Every other request field
//! (`IsuCodeVal`, `BnsTpCode`, `FutsOrdTpCode`, `AbrdFutsOrdPtnCode`, `CrcyCode`,
//! `PrdtCode`, `DueYymm`, `ExchCode`, `OrdDt`, `OvrsFutsOrgOrdNo`, …) is a plain wire
//! string (`#[serde(rename)]`, no serializer).
//!
//! ## Parent order number is caller-supplied, not read back (KTD2)
//!
//! Unlike the domestic F/O modify/cancel — which echo the parent in
//! `OutBlock1.OrgOrdNo` and expose it via `parent_order_no()` — the overseas
//! `CIDBT00900`/`CIDBT01000` take the parent (`OvrsFutsOrgOrdNo`) as a plain `String`
//! REQUEST field supplied by the caller. It flows in directly from the submit
//! response's [`CIDBT00100Response::order_no`] accessor (`OutBlock2.OvrsFutsOrdNo`),
//! so there is no read-back and no reconcile round-trip. The overseas surface has no
//! transient-position read (the CIDBQ account reads are per-currency summaries, not
//! per-contract 잔고 rows), so unlike the domestic F/O chain there is no `t0441`-style
//! fill-detection — flatness rests on clean-cancel confirmation (the smoke harness
//! documents this).
//!
//! ## Response shape and order number
//!
//! For all three TRs both out-blocks are SINGLE objects in the normalized baseline.
//! `OutBlock2.OvrsFutsOrdNo` is the live order number. The modify/cancel `OutBlock2`
//! additionally carries `InnerMsgCnts`, the gateway ack text, surfaced via `ack()`.
//! Account-sensitive response fields (`AcntNo`, `Pwd`) are redacted in `Debug` and
//! never serialized into the dedup cache.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use ls_core::{Inner, LsResult};

// ===========================================================================
// CIDBT00100 — 해외선물 신규주문 (overseas-futures order SUBMIT).
// ===========================================================================

/// Input block for `CIDBT00100` — the twelve required overseas-futures submit fields
/// (baseline `CIDBT00100InBlock1`).
///
/// `OvrsDrvtOrdPrc`/`CndiOrdPrc` carry [`ls_core::string_as_decimal`] (fractional JSON
/// numbers); `OrdQty` carries [`ls_core::string_as_number`] (JSON int). All are IGW40011
/// guardrails. Every other field is a plain wire string.
#[derive(Serialize, Debug, Clone)]
pub struct CIDBT00100InBlock1 {
    /// Order date / 주문일자 (`YYYYMMDD`).
    #[serde(rename = "OrdDt")]
    pub orddt: String,
    /// Issue (contract) code value / 종목코드값.
    #[serde(rename = "IsuCodeVal")]
    pub isucodeval: String,
    /// Futures order-type code / 선물주문구분코드.
    #[serde(rename = "FutsOrdTpCode")]
    pub futsordtpcode: String,
    /// Buy/sell distinction / 매매구분코드 (`"1"` sell, `"2"` buy).
    #[serde(rename = "BnsTpCode")]
    pub bnstpcode: String,
    /// Overseas-futures order-pattern code / 해외선물주문유형코드 (e.g. limit vs market).
    #[serde(rename = "AbrdFutsOrdPtnCode")]
    pub abrdfutsordptncode: String,
    /// Currency code / 통화코드 (e.g. `"USD"`).
    #[serde(rename = "CrcyCode")]
    pub crcycode: String,
    /// Overseas-derivative order price / 해외파생주문가격 (fractional; JSON number).
    #[serde(rename = "OvrsDrvtOrdPrc", serialize_with = "ls_core::string_as_decimal")]
    pub ovrsdrvtordprc: String,
    /// Conditional order price / 조건주문가격 (fractional; JSON number — `"0"` when unused).
    #[serde(rename = "CndiOrdPrc", serialize_with = "ls_core::string_as_decimal")]
    pub cndiordprc: String,
    /// Order quantity / 주문수량 (JSON int).
    #[serde(rename = "OrdQty", serialize_with = "ls_core::string_as_number")]
    pub ordqty: String,
    /// Product code / 상품코드.
    #[serde(rename = "PrdtCode")]
    pub prdtcode: String,
    /// Maturity year-month / 만기년월 (`YYYYMM`).
    #[serde(rename = "DueYymm")]
    pub dueyymm: String,
    /// Exchange code / 거래소코드 (e.g. `"CME"`).
    #[serde(rename = "ExchCode")]
    pub exchcode: String,
}

/// `CIDBT00100` request — wraps the input block under the `CIDBT00100InBlock1` key,
/// serializing to `{"CIDBT00100InBlock1":{...}}`. Dispatches once via
/// [`ls_core::Inner::post_order`] with no continuation.
#[derive(Serialize, Debug, Clone)]
pub struct CIDBT00100Request {
    #[serde(rename = "CIDBT00100InBlock1")]
    pub inblock: CIDBT00100InBlock1,
}

impl CIDBT00100Request {
    /// Build an overseas-futures order submit. All twelve wire fields are explicit —
    /// the overseas order-type/pattern codes (`futsordtpcode`, `abrdfutsordptncode`)
    /// and the contract descriptor (`isucodeval`, `prdtcode`, `dueyymm`, `exchcode`,
    /// `crcycode`) are venue-specific and sourced by the caller (the smoke harness
    /// derives the contract descriptor from the `o3101` master). `ordqty`/`ovrsdrvtordprc`
    /// are decimal strings (e.g. `"1"` / `"4213.25"`); `cndiordprc` is `"0"` for a plain
    /// order.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        orddt: impl Into<String>,
        isucodeval: impl Into<String>,
        futsordtpcode: impl Into<String>,
        bnstpcode: impl Into<String>,
        abrdfutsordptncode: impl Into<String>,
        crcycode: impl Into<String>,
        ovrsdrvtordprc: impl Into<String>,
        cndiordprc: impl Into<String>,
        ordqty: impl Into<String>,
        prdtcode: impl Into<String>,
        dueyymm: impl Into<String>,
        exchcode: impl Into<String>,
    ) -> Self {
        CIDBT00100Request {
            inblock: CIDBT00100InBlock1 {
                orddt: orddt.into(),
                isucodeval: isucodeval.into(),
                futsordtpcode: futsordtpcode.into(),
                bnstpcode: bnstpcode.into(),
                abrdfutsordptncode: abrdfutsordptncode.into(),
                crcycode: crcycode.into(),
                ovrsdrvtordprc: ovrsdrvtordprc.into(),
                cndiordprc: cndiordprc.into(),
                ordqty: ordqty.into(),
                prdtcode: prdtcode.into(),
                dueyymm: dueyymm.into(),
                exchcode: exchcode.into(),
            },
        }
    }
}

/// `CIDBT00100OutBlock1` — the request-echo block (single object).
///
/// Account-sensitive `AcntNo`/`Pwd` are redacted in `Debug` and never serialized.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CIDBT00100OutBlock1 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Order date / 주문일자.
    #[serde(rename = "OrdDt", deserialize_with = "ls_core::string_or_number")]
    pub orddt: String,
    /// Branch code / 지점코드.
    #[serde(rename = "BrnCode", deserialize_with = "ls_core::string_or_number")]
    pub brncode: String,
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
    /// Issue (contract) code value / 종목코드값.
    #[serde(rename = "IsuCodeVal", deserialize_with = "ls_core::string_or_number")]
    pub isucodeval: String,
    /// Futures order-type code / 선물주문구분코드.
    #[serde(rename = "FutsOrdTpCode", deserialize_with = "ls_core::string_or_number")]
    pub futsordtpcode: String,
    /// Buy/sell distinction / 매매구분코드.
    #[serde(rename = "BnsTpCode", deserialize_with = "ls_core::string_or_number")]
    pub bnstpcode: String,
    /// Overseas-futures order-pattern code / 해외선물주문유형코드.
    #[serde(
        rename = "AbrdFutsOrdPtnCode",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub abrdfutsordptncode: String,
    /// Currency code / 통화코드.
    #[serde(rename = "CrcyCode", deserialize_with = "ls_core::string_or_number")]
    pub crcycode: String,
    /// Overseas-derivative order price / 해외파생주문가격.
    #[serde(rename = "OvrsDrvtOrdPrc", deserialize_with = "ls_core::string_or_number")]
    pub ovrsdrvtordprc: String,
    /// Conditional order price / 조건주문가격.
    #[serde(rename = "CndiOrdPrc", deserialize_with = "ls_core::string_or_number")]
    pub cndiordprc: String,
    /// Order quantity / 주문수량.
    #[serde(rename = "OrdQty", deserialize_with = "ls_core::string_or_number")]
    pub ordqty: String,
    /// Product code / 상품코드.
    #[serde(rename = "PrdtCode", deserialize_with = "ls_core::string_or_number")]
    pub prdtcode: String,
    /// Maturity year-month / 만기년월.
    #[serde(rename = "DueYymm", deserialize_with = "ls_core::string_or_number")]
    pub dueyymm: String,
    /// Exchange code / 거래소코드.
    #[serde(rename = "ExchCode", deserialize_with = "ls_core::string_or_number")]
    pub exchcode: String,
}

impl std::fmt::Debug for CIDBT00100OutBlock1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CIDBT00100OutBlock1")
            .field("reccnt", &self.reccnt)
            .field("orddt", &self.orddt)
            .field("brncode", &self.brncode)
            .field("acntno", &"<redacted>")
            .field("pwd", &"<redacted>")
            .field("isucodeval", &self.isucodeval)
            .field("futsordtpcode", &self.futsordtpcode)
            .field("bnstpcode", &self.bnstpcode)
            .field("abrdfutsordptncode", &self.abrdfutsordptncode)
            .field("crcycode", &self.crcycode)
            .field("ovrsdrvtordprc", &self.ovrsdrvtordprc)
            .field("cndiordprc", &self.cndiordprc)
            .field("ordqty", &self.ordqty)
            .field("prdtcode", &self.prdtcode)
            .field("dueyymm", &self.dueyymm)
            .field("exchcode", &self.exchcode)
            .finish()
    }
}

/// `CIDBT00100OutBlock2` — the order-result block (single object).
///
/// `OvrsFutsOrdNo` is the live submit's order number (fed into modify/cancel as their
/// caller-supplied `OvrsFutsOrgOrdNo`). Account-sensitive `AcntNo` is redacted.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CIDBT00100OutBlock2 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Account number / 계좌번호 (account-sensitive; redacted + never serialized).
    #[serde(
        rename = "AcntNo",
        deserialize_with = "ls_core::string_or_number",
        skip_serializing
    )]
    pub acntno: String,
    /// Overseas-futures order number / 해외선물주문번호 — the live submit's order number.
    #[serde(rename = "OvrsFutsOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub ovrsfutsordno: String,
}

impl std::fmt::Debug for CIDBT00100OutBlock2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CIDBT00100OutBlock2")
            .field("reccnt", &self.reccnt)
            .field("acntno", &"<redacted>")
            .field("ovrsfutsordno", &self.ovrsfutsordno)
            .finish()
    }
}

/// `CIDBT00100` response envelope.
///
/// `rsp_cd`/`rsp_msg` are classified by the order predicate in `ls-core` dispatch
/// before this struct is built. Derives `Serialize` so the dedup cache can round-trip it.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CIDBT00100Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "CIDBT00100OutBlock1", default)]
    pub outblock1: CIDBT00100OutBlock1,
    #[serde(rename = "CIDBT00100OutBlock2", default)]
    pub outblock2: CIDBT00100OutBlock2,
}

impl CIDBT00100Response {
    /// The live submit's order number (`OutBlock2.OvrsFutsOrdNo`) — the value fed into
    /// modify/cancel as their caller-supplied `OvrsFutsOrgOrdNo` (KTD2).
    pub fn order_no(&self) -> &str {
        &self.outblock2.ovrsfutsordno
    }
}

// ===========================================================================
// CIDBT00900 — 해외선물 정정주문 (overseas-futures order MODIFY).
//
// References the parent via the caller-supplied `OvrsFutsOrgOrdNo` REQUEST field
// (KTD2) — no read-back. `OutBlock2.OvrsFutsOrdNo` is a NEW order number; `InnerMsgCnts`
// carries the gateway ack text.
// ===========================================================================

/// Input block for `CIDBT00900` — the thirteen required overseas-futures modify fields
/// (baseline `CIDBT00900InBlock1`).
///
/// `OvrsFutsOrgOrdNo` is a plain `String` REQUEST field (the caller-supplied parent,
/// KTD2 — NOT a JSON number). `OvrsDrvtOrdPrc`/`CndiOrdPrc` carry
/// [`ls_core::string_as_decimal`]; `OrdQty` carries [`ls_core::string_as_number`].
#[derive(Serialize, Debug, Clone)]
pub struct CIDBT00900InBlock1 {
    /// Order date / 주문일자 (`YYYYMMDD`).
    #[serde(rename = "OrdDt")]
    pub orddt: String,
    /// Original (parent) overseas-futures order number / 해외선물원주문번호 (caller-supplied
    /// `String`, KTD2 — flows in from the submit's `order_no()`).
    #[serde(rename = "OvrsFutsOrgOrdNo")]
    pub ovrsfutsorgordno: String,
    /// Issue (contract) code value / 종목코드값.
    #[serde(rename = "IsuCodeVal")]
    pub isucodeval: String,
    /// Futures order-type code / 선물주문구분코드.
    #[serde(rename = "FutsOrdTpCode")]
    pub futsordtpcode: String,
    /// Buy/sell distinction / 매매구분코드.
    #[serde(rename = "BnsTpCode")]
    pub bnstpcode: String,
    /// Futures order-pattern code / 선물주문유형코드.
    #[serde(rename = "FutsOrdPtnCode")]
    pub futsordptncode: String,
    /// Currency code value / 통화코드값.
    #[serde(rename = "CrcyCodeVal")]
    pub crcycodeval: String,
    /// Overseas-derivative order price / 해외파생주문가격 (fractional; JSON number).
    #[serde(rename = "OvrsDrvtOrdPrc", serialize_with = "ls_core::string_as_decimal")]
    pub ovrsdrvtordprc: String,
    /// Conditional order price / 조건주문가격 (fractional; JSON number).
    #[serde(rename = "CndiOrdPrc", serialize_with = "ls_core::string_as_decimal")]
    pub cndiordprc: String,
    /// Order quantity / 주문수량 (absolute target; JSON int).
    #[serde(rename = "OrdQty", serialize_with = "ls_core::string_as_number")]
    pub ordqty: String,
    /// Overseas-derivative product code / 해외파생상품코드.
    #[serde(rename = "OvrsDrvtPrdtCode")]
    pub ovrsdrvtprdtcode: String,
    /// Maturity year-month / 만기년월 (`YYYYMM`).
    #[serde(rename = "DueYymm")]
    pub dueyymm: String,
    /// Exchange code / 거래소코드.
    #[serde(rename = "ExchCode")]
    pub exchcode: String,
}

/// `CIDBT00900` request — wraps the input block under the `CIDBT00900InBlock1` key.
#[derive(Serialize, Debug, Clone)]
pub struct CIDBT00900Request {
    #[serde(rename = "CIDBT00900InBlock1")]
    pub inblock: CIDBT00900InBlock1,
}

impl CIDBT00900Request {
    /// Build an overseas-futures order modify against an existing order number. The parent
    /// (`ovrsfutsorgordno`) is the submit's [`CIDBT00100Response::order_no`]. The modify is
    /// absolute (it carries the full target `OvrsDrvtOrdPrc`/`OrdQty`).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        orddt: impl Into<String>,
        ovrsfutsorgordno: impl Into<String>,
        isucodeval: impl Into<String>,
        futsordtpcode: impl Into<String>,
        bnstpcode: impl Into<String>,
        futsordptncode: impl Into<String>,
        crcycodeval: impl Into<String>,
        ovrsdrvtordprc: impl Into<String>,
        cndiordprc: impl Into<String>,
        ordqty: impl Into<String>,
        ovrsdrvtprdtcode: impl Into<String>,
        dueyymm: impl Into<String>,
        exchcode: impl Into<String>,
    ) -> Self {
        CIDBT00900Request {
            inblock: CIDBT00900InBlock1 {
                orddt: orddt.into(),
                ovrsfutsorgordno: ovrsfutsorgordno.into(),
                isucodeval: isucodeval.into(),
                futsordtpcode: futsordtpcode.into(),
                bnstpcode: bnstpcode.into(),
                futsordptncode: futsordptncode.into(),
                crcycodeval: crcycodeval.into(),
                ovrsdrvtordprc: ovrsdrvtordprc.into(),
                cndiordprc: cndiordprc.into(),
                ordqty: ordqty.into(),
                ovrsdrvtprdtcode: ovrsdrvtprdtcode.into(),
                dueyymm: dueyymm.into(),
                exchcode: exchcode.into(),
            },
        }
    }
}

/// `CIDBT00900OutBlock1` — the request-echo block (single object). Echoes the parent
/// (`OvrsFutsOrgOrdNo`). Account-sensitive `AcntNo`/`Pwd` redacted + never serialized.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CIDBT00900OutBlock1 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Order date / 주문일자.
    #[serde(rename = "OrdDt", deserialize_with = "ls_core::string_or_number")]
    pub orddt: String,
    /// Registration branch number / 등록지점번호.
    #[serde(rename = "RegBrnNo", deserialize_with = "ls_core::string_or_number")]
    pub regbrnno: String,
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
    /// Original (parent) overseas-futures order number / 해외선물원주문번호.
    #[serde(
        rename = "OvrsFutsOrgOrdNo",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub ovrsfutsorgordno: String,
    /// Issue (contract) code value / 종목코드값.
    #[serde(rename = "IsuCodeVal", deserialize_with = "ls_core::string_or_number")]
    pub isucodeval: String,
    /// Futures order-type code / 선물주문구분코드.
    #[serde(rename = "FutsOrdTpCode", deserialize_with = "ls_core::string_or_number")]
    pub futsordtpcode: String,
    /// Buy/sell distinction / 매매구분코드.
    #[serde(rename = "BnsTpCode", deserialize_with = "ls_core::string_or_number")]
    pub bnstpcode: String,
    /// Futures order-pattern code / 선물주문유형코드.
    #[serde(rename = "FutsOrdPtnCode", deserialize_with = "ls_core::string_or_number")]
    pub futsordptncode: String,
    /// Currency code value / 통화코드값.
    #[serde(rename = "CrcyCodeVal", deserialize_with = "ls_core::string_or_number")]
    pub crcycodeval: String,
    /// Overseas-derivative order price / 해외파생주문가격.
    #[serde(rename = "OvrsDrvtOrdPrc", deserialize_with = "ls_core::string_or_number")]
    pub ovrsdrvtordprc: String,
    /// Conditional order price / 조건주문가격.
    #[serde(rename = "CndiOrdPrc", deserialize_with = "ls_core::string_or_number")]
    pub cndiordprc: String,
    /// Order quantity / 주문수량.
    #[serde(rename = "OrdQty", deserialize_with = "ls_core::string_or_number")]
    pub ordqty: String,
    /// Overseas-derivative product code / 해외파생상품코드.
    #[serde(
        rename = "OvrsDrvtPrdtCode",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub ovrsdrvtprdtcode: String,
    /// Maturity year-month / 만기년월.
    #[serde(rename = "DueYymm", deserialize_with = "ls_core::string_or_number")]
    pub dueyymm: String,
    /// Exchange code / 거래소코드.
    #[serde(rename = "ExchCode", deserialize_with = "ls_core::string_or_number")]
    pub exchcode: String,
}

impl std::fmt::Debug for CIDBT00900OutBlock1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CIDBT00900OutBlock1")
            .field("reccnt", &self.reccnt)
            .field("orddt", &self.orddt)
            .field("regbrnno", &self.regbrnno)
            .field("acntno", &"<redacted>")
            .field("pwd", &"<redacted>")
            .field("ovrsfutsorgordno", &self.ovrsfutsorgordno)
            .field("isucodeval", &self.isucodeval)
            .field("futsordtpcode", &self.futsordtpcode)
            .field("bnstpcode", &self.bnstpcode)
            .field("futsordptncode", &self.futsordptncode)
            .field("crcycodeval", &self.crcycodeval)
            .field("ovrsdrvtordprc", &self.ovrsdrvtordprc)
            .field("cndiordprc", &self.cndiordprc)
            .field("ordqty", &self.ordqty)
            .field("ovrsdrvtprdtcode", &self.ovrsdrvtprdtcode)
            .field("dueyymm", &self.dueyymm)
            .field("exchcode", &self.exchcode)
            .finish()
    }
}

/// `CIDBT00900OutBlock2` — the order-result block (single object). `OvrsFutsOrdNo` is
/// the NEW order number; `InnerMsgCnts` is the gateway ack text. `AcntNo` redacted.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CIDBT00900OutBlock2 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Account number / 계좌번호 (account-sensitive; redacted + never serialized).
    #[serde(
        rename = "AcntNo",
        deserialize_with = "ls_core::string_or_number",
        skip_serializing
    )]
    pub acntno: String,
    /// New overseas-futures order number / 해외선물주문번호.
    #[serde(rename = "OvrsFutsOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub ovrsfutsordno: String,
    /// Inner message content / 내부메시지내용 — the gateway ack text.
    #[serde(rename = "InnerMsgCnts", deserialize_with = "ls_core::string_or_number")]
    pub innermsgcnts: String,
}

impl std::fmt::Debug for CIDBT00900OutBlock2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CIDBT00900OutBlock2")
            .field("reccnt", &self.reccnt)
            .field("acntno", &"<redacted>")
            .field("ovrsfutsordno", &self.ovrsfutsordno)
            .field("innermsgcnts", &self.innermsgcnts)
            .finish()
    }
}

/// `CIDBT00900` response envelope.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CIDBT00900Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "CIDBT00900OutBlock1", default)]
    pub outblock1: CIDBT00900OutBlock1,
    #[serde(rename = "CIDBT00900OutBlock2", default)]
    pub outblock2: CIDBT00900OutBlock2,
}

impl CIDBT00900Response {
    /// The modify's NEW order number (`OutBlock2.OvrsFutsOrdNo`).
    pub fn order_no(&self) -> &str {
        &self.outblock2.ovrsfutsordno
    }
    /// The gateway ack text (`OutBlock2.InnerMsgCnts`).
    pub fn ack(&self) -> &str {
        &self.outblock2.innermsgcnts
    }
}

// ===========================================================================
// CIDBT01000 — 해외선물 취소주문 (overseas-futures order CANCEL).
//
// References the parent via the caller-supplied `OvrsFutsOrgOrdNo` REQUEST field
// (KTD2). Cancel idempotency comes free from the dedup key (the full body — incl.
// `OvrsFutsOrgOrdNo` — is hashed). `InnerMsgCnts` carries the ack text.
// ===========================================================================

/// Input block for `CIDBT01000` — the six required overseas-futures cancel fields
/// (baseline `CIDBT01000InBlock1`).
///
/// `OvrsFutsOrgOrdNo` is a plain `String` REQUEST field (the caller-supplied parent,
/// KTD2). Cancel carries no numeric fields.
#[derive(Serialize, Debug, Clone)]
pub struct CIDBT01000InBlock1 {
    /// Order date / 주문일자 (`YYYYMMDD`).
    #[serde(rename = "OrdDt")]
    pub orddt: String,
    /// Issue (contract) code value / 종목코드값.
    #[serde(rename = "IsuCodeVal")]
    pub isucodeval: String,
    /// Original (parent) overseas-futures order number / 해외선물원주문번호 (caller-supplied
    /// `String`, KTD2 — the order being canceled).
    #[serde(rename = "OvrsFutsOrgOrdNo")]
    pub ovrsfutsorgordno: String,
    /// Futures order-type code / 선물주문구분코드.
    #[serde(rename = "FutsOrdTpCode")]
    pub futsordtpcode: String,
    /// Product-type code / 상품구분코드.
    #[serde(rename = "PrdtTpCode")]
    pub prdttpcode: String,
    /// Exchange code / 거래소코드.
    #[serde(rename = "ExchCode")]
    pub exchcode: String,
}

/// `CIDBT01000` request — wraps the input block under the `CIDBT01000InBlock1` key.
#[derive(Serialize, Debug, Clone)]
pub struct CIDBT01000Request {
    #[serde(rename = "CIDBT01000InBlock1")]
    pub inblock: CIDBT01000InBlock1,
}

impl CIDBT01000Request {
    /// Build an overseas-futures order cancel against an existing order number. The parent
    /// (`ovrsfutsorgordno`) is the submit's (or modify's) `order_no()`.
    pub fn new(
        orddt: impl Into<String>,
        isucodeval: impl Into<String>,
        ovrsfutsorgordno: impl Into<String>,
        futsordtpcode: impl Into<String>,
        prdttpcode: impl Into<String>,
        exchcode: impl Into<String>,
    ) -> Self {
        CIDBT01000Request {
            inblock: CIDBT01000InBlock1 {
                orddt: orddt.into(),
                isucodeval: isucodeval.into(),
                ovrsfutsorgordno: ovrsfutsorgordno.into(),
                futsordtpcode: futsordtpcode.into(),
                prdttpcode: prdttpcode.into(),
                exchcode: exchcode.into(),
            },
        }
    }
}

/// `CIDBT01000OutBlock1` — the request-echo block (single object). Echoes the parent
/// (`OvrsFutsOrgOrdNo`). Account-sensitive `AcntNo`/`Pwd` redacted + never serialized.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CIDBT01000OutBlock1 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Order date / 주문일자.
    #[serde(rename = "OrdDt", deserialize_with = "ls_core::string_or_number")]
    pub orddt: String,
    /// Branch number / 지점번호.
    #[serde(rename = "BrnNo", deserialize_with = "ls_core::string_or_number")]
    pub brnno: String,
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
    /// Issue (contract) code value / 종목코드값.
    #[serde(rename = "IsuCodeVal", deserialize_with = "ls_core::string_or_number")]
    pub isucodeval: String,
    /// Original (parent) overseas-futures order number / 해외선물원주문번호.
    #[serde(
        rename = "OvrsFutsOrgOrdNo",
        deserialize_with = "ls_core::string_or_number"
    )]
    pub ovrsfutsorgordno: String,
    /// Futures order-type code / 선물주문구분코드.
    #[serde(rename = "FutsOrdTpCode", deserialize_with = "ls_core::string_or_number")]
    pub futsordtpcode: String,
    /// Product-type code / 상품구분코드.
    #[serde(rename = "PrdtTpCode", deserialize_with = "ls_core::string_or_number")]
    pub prdttpcode: String,
    /// Exchange code / 거래소코드.
    #[serde(rename = "ExchCode", deserialize_with = "ls_core::string_or_number")]
    pub exchcode: String,
}

impl std::fmt::Debug for CIDBT01000OutBlock1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CIDBT01000OutBlock1")
            .field("reccnt", &self.reccnt)
            .field("orddt", &self.orddt)
            .field("brnno", &self.brnno)
            .field("acntno", &"<redacted>")
            .field("pwd", &"<redacted>")
            .field("isucodeval", &self.isucodeval)
            .field("ovrsfutsorgordno", &self.ovrsfutsorgordno)
            .field("futsordtpcode", &self.futsordtpcode)
            .field("prdttpcode", &self.prdttpcode)
            .field("exchcode", &self.exchcode)
            .finish()
    }
}

/// `CIDBT01000OutBlock2` — the order-result block (single object). `OvrsFutsOrdNo` is
/// the NEW cancel order number; `InnerMsgCnts` is the gateway ack text. `AcntNo` redacted.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct CIDBT01000OutBlock2 {
    /// Record count / 레코드갯수.
    #[serde(rename = "RecCnt", deserialize_with = "ls_core::string_or_number")]
    pub reccnt: String,
    /// Account number / 계좌번호 (account-sensitive; redacted + never serialized).
    #[serde(
        rename = "AcntNo",
        deserialize_with = "ls_core::string_or_number",
        skip_serializing
    )]
    pub acntno: String,
    /// New overseas-futures order number / 해외선물주문번호.
    #[serde(rename = "OvrsFutsOrdNo", deserialize_with = "ls_core::string_or_number")]
    pub ovrsfutsordno: String,
    /// Inner message content / 내부메시지내용 — the gateway ack text.
    #[serde(rename = "InnerMsgCnts", deserialize_with = "ls_core::string_or_number")]
    pub innermsgcnts: String,
}

impl std::fmt::Debug for CIDBT01000OutBlock2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CIDBT01000OutBlock2")
            .field("reccnt", &self.reccnt)
            .field("acntno", &"<redacted>")
            .field("ovrsfutsordno", &self.ovrsfutsordno)
            .field("innermsgcnts", &self.innermsgcnts)
            .finish()
    }
}

/// `CIDBT01000` response envelope.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CIDBT01000Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "CIDBT01000OutBlock1", default)]
    pub outblock1: CIDBT01000OutBlock1,
    #[serde(rename = "CIDBT01000OutBlock2", default)]
    pub outblock2: CIDBT01000OutBlock2,
}

impl CIDBT01000Response {
    /// The cancel's NEW order number (`OutBlock2.OvrsFutsOrdNo`).
    pub fn order_no(&self) -> &str {
        &self.outblock2.ovrsfutsordno
    }
    /// The gateway ack text (`OutBlock2.InnerMsgCnts`).
    pub fn ack(&self) -> &str {
        &self.outblock2.innermsgcnts
    }
}

// ===========================================================================
// OverseasFoOrders handle
// ===========================================================================

/// The overseas futures/options order handle — the overseas sibling of
/// [`super::FoOrders`].
///
/// Every method routes EXCLUSIVELY through [`ls_core::Inner::post_order`] (no-retry /
/// dedup / kill-switch), never `post`/`post_paginated`.
pub struct OverseasFoOrders {
    inner: Arc<Inner>,
}

impl OverseasFoOrders {
    /// Wrap a shared runtime core.
    pub fn new(inner: Arc<Inner>) -> Self {
        OverseasFoOrders { inner }
    }

    /// The config-supplied account number this handle operates on.
    pub fn account_no(&self) -> &str {
        &self.inner.config.account_no
    }

    /// Submit an overseas-futures order via `CIDBT00100`.
    ///
    /// Single HTTP attempt (no retry — an ambiguous timeout is reconciled, never
    /// resubmitted), gated by the kill switch and the dedup window, charging the
    /// `Orders` rate bucket. A rejection surfaces as [`ls_core::LsError::ApiError`]; an
    /// ambiguous outcome (incl. an HTTP-500 `IGW40011`) surfaces as
    /// [`ls_core::LsError::AmbiguousOrder`].
    pub async fn submit(&self, req: &CIDBT00100Request) -> LsResult<CIDBT00100Response> {
        self.inner
            .post_order(&ls_core::endpoint_policy::CIDBT00100_POLICY, req)
            .await
    }

    /// Modify an existing overseas-futures order via `CIDBT00900`.
    ///
    /// Same no-retry / dedup / kill-switch path as [`OverseasFoOrders::submit`]. The
    /// parent order number flows in as the caller-supplied `OvrsFutsOrgOrdNo` (KTD2) — no
    /// read-back.
    pub async fn modify(&self, req: &CIDBT00900Request) -> LsResult<CIDBT00900Response> {
        self.inner
            .post_order(&ls_core::endpoint_policy::CIDBT00900_POLICY, req)
            .await
    }

    /// Cancel an existing overseas-futures order via `CIDBT01000`.
    ///
    /// Cancel is idempotent within the dedup TTL for free (the full body incl.
    /// `OvrsFutsOrgOrdNo` is the dedup key).
    pub async fn cancel(&self, req: &CIDBT01000Request) -> LsResult<CIDBT01000Response> {
        self.inner
            .post_order(&ls_core::endpoint_policy::CIDBT01000_POLICY, req)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- CIDBT00100 submit -------------------------------------------------

    /// Edge (R4/KTD1, IGW40011): the fractional price fields OvrsDrvtOrdPrc/CndiOrdPrc
    /// serialize as JSON numbers (not strings) and OrdQty as a JSON int; the string
    /// fields (IsuCodeVal/BnsTpCode/ExchCode) stay strings.
    #[test]
    fn cidbt00100_request_serializes_numeric_fields_as_json_numbers() {
        let req = CIDBT00100Request::new(
            "20260702", "ESU26", "1", "2", "1", "USD", "4213.25", "0", "1", "ES", "202609", "CME",
        );
        let v: serde_json::Value = serde_json::to_value(&req).unwrap();
        let inb = &v["CIDBT00100InBlock1"];
        assert!(
            inb["OvrsDrvtOrdPrc"].is_number(),
            "OvrsDrvtOrdPrc must be a JSON number, got {}",
            inb["OvrsDrvtOrdPrc"]
        );
        assert!(inb["CndiOrdPrc"].is_number(), "CndiOrdPrc must be a JSON number");
        assert!(inb["OrdQty"].is_number(), "OrdQty must be a JSON int");
        assert_eq!(inb["OvrsDrvtOrdPrc"].as_f64().unwrap(), 4213.25);
        assert_eq!(inb["OrdQty"].as_i64().unwrap(), 1);
        // Non-numeric wire fields stay strings.
        assert!(inb["IsuCodeVal"].is_string());
        assert!(inb["BnsTpCode"].is_string());
        assert!(inb["ExchCode"].is_string());
        assert_eq!(inb["ExchCode"], "CME");
    }

    /// Edge (KTD1): an INTEGER-valued price serializes as a bare JSON integer number
    /// with no gratuitous trailing `.0` — some overseas contracts trade integer ticks,
    /// and the gateway's strict numeric contract must see `4200`, not `4200.0`.
    #[test]
    fn cidbt00100_integer_price_has_no_trailing_dot_zero() {
        let req = CIDBT00100Request::new(
            "20260702", "ESU26", "1", "2", "1", "USD", "4200", "0", "1", "ES", "202609", "CME",
        );
        let s = serde_json::to_string(&req).unwrap();
        // The JSON text must carry `"OvrsDrvtOrdPrc":4200` (a bare int), never `4200.0`.
        assert!(
            s.contains("\"OvrsDrvtOrdPrc\":4200")
                && !s.contains("\"OvrsDrvtOrdPrc\":4200.0"),
            "integer price must serialize as a bare int (no trailing .0): {s}"
        );
        let v: serde_json::Value = serde_json::to_value(&req).unwrap();
        assert_eq!(v["CIDBT00100InBlock1"]["OvrsDrvtOrdPrc"].as_i64(), Some(4200));
    }

    /// Happy path (R4): round-trip decode of a CIDBT00100 success response (fixture
    /// synthesized from the baseline `response_blocks` field list — the normalized
    /// baseline has no `res_example`). Surfaces the order number via `order_no()`.
    #[test]
    fn cidbt00100_response_decodes_echo_and_order_number() {
        let raw = serde_json::json!({
            "rsp_cd": "00040",
            "rsp_msg": "정상처리 되었습니다.",
            "CIDBT00100OutBlock1": {
                "RecCnt": 1, "OrdDt": "20260702", "BrnCode": "0001",
                "AcntNo": "20001652603", "Pwd": "********", "IsuCodeVal": "ESU26",
                "FutsOrdTpCode": "1", "BnsTpCode": "2", "AbrdFutsOrdPtnCode": "1",
                "CrcyCode": "USD", "OvrsDrvtOrdPrc": "4213.25000000", "CndiOrdPrc": 0,
                "OrdQty": 1, "PrdtCode": "ES", "DueYymm": "202609", "ExchCode": "CME"
            },
            "CIDBT00100OutBlock2": {
                "RecCnt": 1, "AcntNo": "20001652603", "OvrsFutsOrdNo": 90007
            }
        });
        let resp: CIDBT00100Response = serde_json::from_value(raw).unwrap();
        assert_eq!(resp.rsp_cd, "00040");
        assert_eq!(resp.order_no(), "90007");
        assert_eq!(resp.outblock1.isucodeval, "ESU26");
        assert_eq!(resp.outblock1.ordqty, "1");
        // Account-sensitive fields decode but are never serialized into the dedup cache.
        assert_eq!(resp.outblock1.acntno, "20001652603");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.contains("20001652603"), "AcntNo must not serialize: {json}");
        // Debug redacts account-sensitive fields.
        let dbg = format!("{:?}", resp.outblock2);
        assert!(dbg.contains("<redacted>") && !dbg.contains("20001652603"));
    }

    // ---- CIDBT00900 modify -------------------------------------------------

    /// Covers KTD1/KTD2: a modify request carries the caller-supplied parent
    /// (OvrsFutsOrgOrdNo) as a plain STRING (not a JSON number), and the numeric fields
    /// as JSON numbers.
    #[test]
    fn cidbt00900_request_parent_is_string_and_numerics_are_numbers() {
        let req = CIDBT00900Request::new(
            "20260702", "90007", "ESU26", "1", "2", "1", "USD", "4213.5", "0", "1", "ES",
            "202609", "CME",
        );
        let v: serde_json::Value = serde_json::to_value(&req).unwrap();
        let inb = &v["CIDBT00900InBlock1"];
        // KTD2: parent order number is a caller-supplied STRING request field.
        assert!(
            inb["OvrsFutsOrgOrdNo"].is_string(),
            "OvrsFutsOrgOrdNo must be a plain string (KTD2), got {}",
            inb["OvrsFutsOrgOrdNo"]
        );
        assert_eq!(inb["OvrsFutsOrgOrdNo"], "90007");
        // KTD1: prices/qty are JSON numbers.
        assert!(inb["OvrsDrvtOrdPrc"].is_number());
        assert!(inb["OrdQty"].is_number());
        assert_eq!(inb["OvrsDrvtOrdPrc"].as_f64().unwrap(), 4213.5);
        assert_eq!(inb["OrdQty"].as_i64().unwrap(), 1);
    }

    /// Happy path (R4): decode a CIDBT00900 modify response — the NEW order number and
    /// the InnerMsgCnts ack are surfaced.
    #[test]
    fn cidbt00900_response_decodes_new_order_number_and_ack() {
        let raw = serde_json::json!({
            "rsp_cd": "00132",
            "rsp_msg": "정정주문이 완료되었습니다.",
            "CIDBT00900OutBlock1": {
                "RecCnt": 1, "OrdDt": "20260702", "RegBrnNo": "0001",
                "AcntNo": "20277932702", "Pwd": "********", "OvrsFutsOrgOrdNo": "90007",
                "IsuCodeVal": "ESU26", "FutsOrdTpCode": "1", "BnsTpCode": "2",
                "FutsOrdPtnCode": "1", "CrcyCodeVal": "USD", "OvrsDrvtOrdPrc": "4213.50000000",
                "CndiOrdPrc": 0, "OrdQty": 1, "OvrsDrvtPrdtCode": "ES",
                "DueYymm": "202609", "ExchCode": "CME"
            },
            "CIDBT00900OutBlock2": {
                "RecCnt": 1, "AcntNo": "20277932702", "OvrsFutsOrdNo": 90009,
                "InnerMsgCnts": "정정 접수"
            }
        });
        let resp: CIDBT00900Response = serde_json::from_value(raw).unwrap();
        assert_eq!(resp.rsp_cd, "00132");
        assert_eq!(resp.order_no(), "90009", "new order number from OutBlock2");
        assert_eq!(resp.ack(), "정정 접수");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.contains("20277932702"), "AcntNo must not serialize");
    }

    // ---- CIDBT01000 cancel -------------------------------------------------

    /// Covers KTD2: a cancel request built from a submit's order number carries that
    /// number in the plain-string OvrsFutsOrgOrdNo field.
    #[test]
    fn cidbt01000_cancel_request_carries_parent_as_string() {
        let submit_ordno = "90007"; // the OvrsFutsOrdNo a CIDBT00100 submit returned
        let req = CIDBT01000Request::new("20260702", "ESU26", submit_ordno, "1", "F", "CME");
        let v: serde_json::Value = serde_json::to_value(&req).unwrap();
        let inb = &v["CIDBT01000InBlock1"];
        assert!(inb["OvrsFutsOrgOrdNo"].is_string(), "parent is a plain string (KTD2)");
        assert_eq!(inb["OvrsFutsOrgOrdNo"], "90007");
        assert_eq!(inb["ExchCode"], "CME");
    }

    /// Happy path (R4): decode a CIDBT01000 cancel response — the new order number and
    /// the InnerMsgCnts ack are surfaced.
    #[test]
    fn cidbt01000_response_decodes_order_number_and_ack() {
        let raw = serde_json::json!({
            "rsp_cd": "00156",
            "rsp_msg": "취소주문이 완료되었습니다.",
            "CIDBT01000OutBlock1": {
                "RecCnt": 1, "OrdDt": "20260702", "BrnNo": "0001",
                "AcntNo": "20277932702", "Pwd": "********", "IsuCodeVal": "ESU26",
                "OvrsFutsOrgOrdNo": "90007", "FutsOrdTpCode": "1", "PrdtTpCode": "F",
                "ExchCode": "CME"
            },
            "CIDBT01000OutBlock2": {
                "RecCnt": 1, "AcntNo": "20277932702", "OvrsFutsOrdNo": 90011,
                "InnerMsgCnts": "취소 접수"
            }
        });
        let resp: CIDBT01000Response = serde_json::from_value(raw).unwrap();
        assert_eq!(resp.rsp_cd, "00156");
        assert_eq!(resp.order_no(), "90011");
        assert_eq!(resp.ack(), "취소 접수");
        // Debug redacts the account number on the echo block.
        let dbg = format!("{:?}", resp.outblock1);
        assert!(dbg.contains("<redacted>") && !dbg.contains("20277932702"));
    }
}
