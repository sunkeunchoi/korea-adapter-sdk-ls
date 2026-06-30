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

// ---- Per-account-domain struct modules (Wave-2b decomposition; pure
// relocation, re-exported so every public path is unchanged). ----
mod balance;
pub use balance::*;
mod holdings;
pub use holdings::*;
mod capacity;
pub use capacity::*;

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

    /// Inquire the futures/options account deposit / margin via `CFOBQ10500`.
    ///
    /// Dispatches through plain [`ls_core::Inner::post`] (Account rate bucket,
    /// single-page). The account is the config-supplied [`Account::account_no`],
    /// identified by the bearer token — the caller passes no input at all (the
    /// request body is an empty in-block). A position-less paper account may
    /// return an empty `00707` deposit (the PENDING case), not a defect.
    pub async fn fo_deposit(&self, req: &CFOBQ10500Request) -> LsResult<CFOBQ10500Response> {
        self.inner
            .post(&ls_core::endpoint_policy::CFOBQ10500_POLICY, req)
            .await
    }

    /// Inquire the KRX night-derivatives account balance via `CCENQ90200`.
    ///
    /// Dispatches through plain [`ls_core::Inner::post`] (Account rate bucket,
    /// single-page). This is a read-only account-state read on the krx_extended
    /// (night) session. The account is the config-supplied [`Account::account_no`],
    /// identified by the bearer token — the caller passes only the record count and
    /// two evaluation-shape enums, never an account number. A position-less paper
    /// account or an off-window run may return an empty `00707` (the PENDING case),
    /// not a defect.
    pub async fn night_balance(
        &self,
        req: &CCENQ90200Request,
    ) -> LsResult<CCENQ90200Response> {
        self.inner
            .post(&ls_core::endpoint_policy::CCENQ90200_POLICY, req)
            .await
    }

    /// Inquire the F/O orderable quantity via `CFOAQ10100`.
    ///
    /// Dispatches through plain [`ls_core::Inner::post`] (Account rate bucket,
    /// single-page). This is a read-only orderable-quantity INQUIRY (조회), NOT an
    /// order — it places nothing. The account is the config-supplied
    /// [`Account::account_no`], identified by the bearer token — the caller passes
    /// the order-shape inputs (incl. `FnoIsuNo`), never an account number.
    pub async fn fo_orderable_qty(
        &self,
        req: &CFOAQ10100Request,
    ) -> LsResult<CFOAQ10100Response> {
        self.inner
            .post(&ls_core::endpoint_policy::CFOAQ10100_POLICY, req)
            .await
    }

    /// Inquire the KRX night-derivatives orderable quantity via `CCENQ10100`.
    ///
    /// Dispatches through plain [`ls_core::Inner::post`] (Account rate bucket,
    /// single-page). This is a read-only orderable-quantity INQUIRY (조회) on the
    /// krx_extended (night) session, NOT an order. The account is the
    /// config-supplied [`Account::account_no`], identified by the bearer token —
    /// the caller passes the order-shape inputs, never an account number.
    pub async fn night_orderable_qty(
        &self,
        req: &CCENQ10100Request,
    ) -> LsResult<CCENQ10100Response> {
        self.inner
            .post(&ls_core::endpoint_policy::CCENQ10100_POLICY, req)
            .await
    }

    /// Inquire the stock balance (positions + cash summary) via `t0424`.
    ///
    /// Dispatches through plain [`ls_core::Inner::post`] (Account rate bucket,
    /// single-page). The account is the config-supplied [`Account::account_no`],
    /// identified by the bearer token — the caller passes only the gubun flags,
    /// never an account number. A position-less paper account returns a populated
    /// cash summary (`outblock`) with an empty holdings array (`outblock1`) — the
    /// cash-only case (KTD3), not a defect.
    pub async fn stock_balance(&self, req: &T0424Request) -> LsResult<T0424Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T0424_POLICY, req)
            .await
    }

    /// Inquire the order-capacity by margin rate (증거금률별주문가능수량) via
    /// `CSPBQ00200`.
    ///
    /// Dispatches through plain [`ls_core::Inner::post`] (Account rate bucket,
    /// single-page). The account is the config-supplied [`Account::account_no`],
    /// identified by the bearer token — the caller passes the issue + side + price,
    /// never an account number. With `OrdPrc = "0"` the orderable quantity is 0 but
    /// the capacity amounts (예수금 / 주문가능금액) are populated from account cash.
    pub async fn order_capacity(
        &self,
        req: &CSPBQ00200Request,
    ) -> LsResult<CSPBQ00200Response> {
        self.inner
            .post(&ls_core::endpoint_policy::CSPBQ00200_POLICY, req)
            .await
    }

    /// Inquire the loanable-collateral stock list (예탁담보융자가능종목) via
    /// `CLNAQ00100`.
    ///
    /// Dispatches through plain [`ls_core::Inner::post`] (Account rate bucket,
    /// single-page) on `/stock/etc`. Persistent reference data — the loanable
    /// universe is populated regardless of market hours. The account context (per-
    /// account limit fields) comes from the bearer token, never a caller identifier.
    pub async fn loanable_stocks(
        &self,
        req: &CLNAQ00100Request,
    ) -> LsResult<CLNAQ00100Response> {
        self.inner
            .post(&ls_core::endpoint_policy::CLNAQ00100_POLICY, req)
            .await
    }

    /// Inquire the F/O provisional-settlement deposit detail (가정산예탁금상세) via
    /// `CFOEQ11100`.
    ///
    /// Dispatches through plain [`ls_core::Inner::post`] (Account rate bucket,
    /// single-page) on `/futureoption/accno`. The account is the config-supplied
    /// [`Account::account_no`], identified by the bearer token — the caller passes
    /// only the settlement business date. A position-less paper account may return an
    /// empty `00707` (the PENDING case), not a defect.
    pub async fn fo_deposit_detail(
        &self,
        req: &CFOEQ11100Request,
    ) -> LsResult<CFOEQ11100Response> {
        self.inner
            .post(&ls_core::endpoint_policy::CFOEQ11100_POLICY, req)
            .await
    }

    /// Inquire the F/O balance valuation (잔고평가, moving-average) via `t0441`.
    ///
    /// Dispatches through plain [`ls_core::Inner::post`] (Account rate bucket,
    /// single-page) on `/futureoption/accno`. The account is the config-supplied
    /// [`Account::account_no`], identified by the bearer token. A position-less paper
    /// account returns an empty position array + zero valuation summary (the PENDING
    /// case), not a defect.
    pub async fn fo_balance_eval(&self, req: &T0441Request) -> LsResult<T0441Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T0441_POLICY, req)
            .await
    }

    /// Inquire the overseas-futures orderable quantity (해외선물 주문가능수량) via
    /// `CIDBQ01400`.
    ///
    /// Dispatches through plain [`ls_core::Inner::post`] (Account rate bucket,
    /// single-page) on `/overseas-futureoption/accno`. The account is the config-
    /// supplied [`Account::account_no`], identified by the bearer token — the caller
    /// passes the contract + side + price. An overseas paper account without
    /// overseas-futures eligibility may return an empty/zero quantity (the PENDING
    /// case), not a defect.
    pub async fn overseas_fo_order_qty(
        &self,
        req: &CIDBQ01400Request,
    ) -> LsResult<CIDBQ01400Response> {
        self.inner
            .post(&ls_core::endpoint_policy::CIDBQ01400_POLICY, req)
            .await
    }

    /// Inquire the overseas-futures deposit / balance status (해외선물 예수금/잔고현황)
    /// via `CIDBQ03000`.
    ///
    /// Dispatches through plain [`ls_core::Inner::post`] (Account rate bucket,
    /// single-page) on `/overseas-futureoption/accno`. The account is the config-
    /// supplied [`Account::account_no`], identified by the bearer token — the caller
    /// passes only the account-type flag and a trade date, never an account number.
    /// Reachable only when the token authenticates as the overseas-futures account;
    /// otherwise an empty/zero balance (the PENDING case), not a defect.
    pub async fn overseas_fo_balance(
        &self,
        req: &CIDBQ03000Request,
    ) -> LsResult<CIDBQ03000Response> {
        self.inner
            .post(&ls_core::endpoint_policy::CIDBQ03000_POLICY, req)
            .await
    }

    /// Inquire the overseas-futures deposited assets (해외선물 예탁자산) via `CIDBQ05300`.
    ///
    /// Dispatches through plain [`ls_core::Inner::post`] (Account rate bucket,
    /// single-page) on `/overseas-futureoption/accno`. The account is the config-
    /// supplied [`Account::account_no`], identified by the bearer token — the caller
    /// passes only the account-type and currency flags, never an account number.
    /// Reachable only when the token authenticates as the overseas-futures account
    /// (the cash account returned `IGW40013` here, a wrong-account artifact).
    pub async fn overseas_fo_deposited_assets(
        &self,
        req: &CIDBQ05300Request,
    ) -> LsResult<CIDBQ05300Response> {
        self.inner
            .post(&ls_core::endpoint_policy::CIDBQ05300_POLICY, req)
            .await
    }
}
