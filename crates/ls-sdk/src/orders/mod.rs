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

pub mod reconcile;

pub use reconcile::{reconcile, OrderIntent, OrderState, ReconcileOutcome, ReconciliationRecord};

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

// ---------------------------------------------------------------------------
// t0425 — 주식체결/미체결 (stock filled/unfilled order inquiry).
//
// The read-only reconciliation companion. is_order: false; dispatches through
// post_paginated. Self-paginates on the cts_ordno body cursor.
// ---------------------------------------------------------------------------

/// Input block for `t0425` — symbol filter, fill/side/sort flags, and the
/// `cts_ordno` cursor. Wire field names are lowercase, matching the raw capture.
#[derive(Serialize, Debug, Clone)]
pub struct T0425InBlock {
    /// Issue (symbol) number / 종목번호. Empty queries all symbols.
    pub expcode: String,
    /// Fill distinction / 체결구분 (`"0"` all, `"1"` filled, `"2"` unfilled).
    pub chegb: String,
    /// Buy/sell distinction / 매매구분 (`"0"` all, `"1"` sell, `"2"` buy).
    pub medosu: String,
    /// Sort order / 정렬순서.
    pub sortgb: String,
    /// Order-number cursor / 주문번호 — the `cts_ordno` continuation cursor
    /// (`" "` on the first page).
    pub cts_ordno: String,
}

/// `t0425` request — wraps the input block under the `t0425InBlock` key.
///
/// Self-paginates on the `cts_ordno` body cursor; the `tr_cont`/`tr_cont_key`
/// header tokens are threaded defensively via [`ls_core::HasPagination`] (they
/// ride as HTTP headers, never the body).
#[derive(Serialize, Debug, Clone)]
pub struct T0425Request {
    #[serde(rename = "t0425InBlock")]
    pub inblock: T0425InBlock,
    #[serde(skip)]
    pub tr_cont: String,
    #[serde(skip)]
    pub tr_cont_key: String,
}

ls_core::impl_has_pagination!(T0425Request);

impl T0425Request {
    /// A reconciliation query for one symbol: all fills, both sides, first page.
    pub fn for_symbol(expcode: impl Into<String>) -> Self {
        T0425Request {
            inblock: T0425InBlock {
                expcode: expcode.into(),
                chegb: "0".into(),
                medosu: "0".into(),
                sortgb: "2".into(),
                cts_ordno: " ".into(),
            },
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        }
    }
}

/// `t0425OutBlock` — the order-totals summary (single object).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T0425OutBlock {
    /// Total order quantity / 총주문수량.
    #[serde(rename = "tqty", deserialize_with = "ls_core::string_or_number")]
    pub tqty: String,
    /// Total filled quantity / 총체결수량.
    #[serde(rename = "tcheqty", deserialize_with = "ls_core::string_or_number")]
    pub tcheqty: String,
    /// Total unfilled quantity / 총미체결수량.
    #[serde(rename = "tordrem", deserialize_with = "ls_core::string_or_number")]
    pub tordrem: String,
    /// Order-number cursor / 주문번호 (the next-page `cts_ordno`).
    #[serde(rename = "cts_ordno", deserialize_with = "ls_core::string_or_number")]
    pub cts_ordno: String,
}

/// `t0425OutBlock1` — one order/execution row.
///
/// `ordno` is the order number (a `Number` on the wire), matched against
/// `CSPAT00601OutBlock2.OrdNo` after normalization. `medosu` is the side as
/// Korean text (`"매수"`/`"매도"`); `status` (상태) is the order state text
/// (`"접수"`/`"체결"`/`"취소"`/...).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T0425OutBlock1 {
    /// Order number / 주문번호.
    #[serde(rename = "ordno", deserialize_with = "ls_core::string_or_number")]
    pub ordno: String,
    /// Issue (symbol) number / 종목번호.
    #[serde(rename = "expcode", deserialize_with = "ls_core::string_or_number")]
    pub expcode: String,
    /// Side / 구분 (Korean text `"매수"`/`"매도"`).
    #[serde(rename = "medosu", deserialize_with = "ls_core::string_or_number")]
    pub medosu: String,
    /// Order quantity / 주문수량.
    #[serde(rename = "qty", deserialize_with = "ls_core::string_or_number")]
    pub qty: String,
    /// Order price / 주문가격.
    #[serde(rename = "price", deserialize_with = "ls_core::string_or_number")]
    pub price: String,
    /// Filled quantity / 체결수량.
    #[serde(rename = "cheqty", deserialize_with = "ls_core::string_or_number")]
    pub cheqty: String,
    /// Unfilled remaining / 미체결잔량.
    #[serde(rename = "ordrem", deserialize_with = "ls_core::string_or_number")]
    pub ordrem: String,
    /// Order state / 상태 (`"접수"`/`"체결"`/`"취소"`/`"정정"`/...).
    #[serde(rename = "status", deserialize_with = "ls_core::string_or_number")]
    pub status: String,
    /// Original order number / 원주문번호 (for a modify/cancel).
    #[serde(rename = "orgordno", deserialize_with = "ls_core::string_or_number")]
    pub orgordno: String,
    /// Order time / 주문시간.
    #[serde(rename = "ordtime", deserialize_with = "ls_core::string_or_number")]
    pub ordtime: String,
}

/// `t0425` response envelope.
///
/// `outblock1` is the row array (tolerated as a single object OR an array via
/// [`ls_core::de_vec_or_single`]); `outblock` is the totals summary. Implements
/// [`ls_core::HasPagination`] so `cts_ordno`-cursor continuation can be driven.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T0425Response {
    #[serde(default)]
    pub rsp_cd: String,
    #[serde(default)]
    pub rsp_msg: String,
    #[serde(rename = "t0425OutBlock", default)]
    pub outblock: T0425OutBlock,
    #[serde(
        rename = "t0425OutBlock1",
        default,
        deserialize_with = "ls_core::de_vec_or_single"
    )]
    pub outblock1: Vec<T0425OutBlock1>,
    /// Pagination continuation (injected by dispatch from the response headers).
    #[serde(default)]
    pub tr_cont: String,
    #[serde(default)]
    pub tr_cont_key: String,
}

ls_core::impl_has_pagination!(T0425Response);

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
    ///
    /// Dedup observability (known limitation): a within-window duplicate returns
    /// the cached response as `Ok` — indistinguishable at this return type from a
    /// fresh ack — with `dedup_hit=true` recorded only on the dispatch span. The
    /// safety property (no second exchange dispatch) holds, but a caller that
    /// needs the reconciliation `Duplicate` state must track its own submission
    /// identity and pass `dedup_hit` to [`Orders::reconcile`] (the evidence
    /// harness does this by varying scenario params). A first-class duplicate
    /// signal on the return value is a deliberate follow-up, not shipped here.
    pub async fn submit(&self, req: &CSPAT00601Request) -> LsResult<CSPAT00601Response> {
        self.inner
            .post_order(&ls_core::endpoint_policy::CSPAT00601_POLICY, req)
            .await
    }

    /// Query order/execution state via the `t0425` read (the reconciliation
    /// companion). A READ — dispatches through [`ls_core::Inner::post_paginated`]
    /// (`is_order: false`), never the order path.
    pub async fn inquiry(&self, req: &T0425Request) -> LsResult<T0425Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T0425_POLICY, req)
            .await
    }

    /// Reconcile a local order intent against live exchange state (order-safety
    /// §3). After an ambiguous send, query `t0425` for the intent's symbol and
    /// classify the outcome (Accepted / Rejected / Duplicate / Modified /
    /// Canceled / Unknown). A `dedup_hit` short-circuits to Duplicate without a
    /// query. A failed query fails toward Unknown (never silent Accepted); a
    /// clean query with no matching order proves absence and is safe to retry.
    pub async fn reconcile(&self, intent: &OrderIntent, dedup_hit: bool) -> ReconcileOutcome {
        if dedup_hit {
            return ReconcileOutcome::duplicate();
        }
        let req = T0425Request::for_symbol(&intent.symbol);
        let result = self.inquiry(&req).await;
        // `None` => the query itself failed: fail toward Unknown, not safe to
        // retry (we could not prove the order's absence).
        reconcile(intent, result.as_ref().ok(), dedup_hit)
    }
}
