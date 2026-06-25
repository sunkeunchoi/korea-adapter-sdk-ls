//! Paginated dependency class — two distinct continuation shapes, plus the facade.
//!
//! 1. **`t8412` — header-cursor pagination (multi-page).** Threads an LS
//!    continuation through the `tr_cont`/`tr_cont_key` HTTP headers and walks pages
//!    via `chart_all`. See [`chart`].
//! 2. **Body-`idx` rank/screen TRs — single-page only.** `t1452`, `t1403`,
//!    `t1441`, `t1463`, `t1466`, `t1489`, `t1492` carry a request-BODY `idx`
//!    continuation cursor, for which `ls-core` has no multi-page machinery; they
//!    are promoted at single-page scope. See [`rank_screen`].
//!
//! Both submodules' public types are re-exported here, so callers reach them as
//! `ls_sdk::paginated::T8412Request`, `ls_sdk::paginated::T1452Response`, etc.,
//! unchanged by the split. This module owns the [`Paginated`] facade.

use std::sync::Arc;

use ls_core::{Inner, LsResult};

mod chart;
mod invest_opinion;
mod rank_screen;
mod sector_index;

pub use chart::*;
pub use invest_opinion::*;
pub use rank_screen::*;
pub use sector_index::*;

/// Paginated operations, backed by the shared runtime core.
///
/// Cheap to clone — shares `Arc<Inner>` (and therefore the token cache and rate
/// limiter) with the rest of the SDK.
#[derive(Clone)]
pub struct Paginated {
    inner: Arc<Inner>,
}

impl Paginated {
    /// Wrap a shared runtime core.
    pub fn new(inner: Arc<Inner>) -> Self {
        Paginated { inner }
    }

    /// Fetch a SINGLE page of the `t8412` chart.
    ///
    /// Dispatches through [`ls_core::Inner::post_paginated`], which reads the
    /// request's `tr_cont`/`tr_cont_key` via [`ls_core::HasPagination`] and sends
    /// them as HTTP headers. The returned response carries the continuation from
    /// the response headers; the caller may thread it onto a follow-up request, or
    /// use [`Paginated::chart_all`] to walk every page.
    pub async fn chart_page(&self, req: &T8412Request) -> LsResult<T8412Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T8412_POLICY, req)
            .await
    }

    /// Fetch the FULL range of the `t8412` chart, walking every page.
    ///
    /// Drives [`ls_core::Inner::collect_all`], which loops until the response
    /// `tr_cont` header is empty/`"N"` or `max_pages` is hit (returning
    /// [`ls_core::LsError::PaginationLimit`] at the cap). Each page's `tr_cont`/
    /// `tr_cont_key` are copied onto the next request. Returns the accumulated
    /// pages in order; callers concatenate `outblock1` across them.
    pub async fn chart_all(&self, req: T8412Request) -> LsResult<Vec<T8412Response>> {
        let inner = Arc::clone(&self.inner);
        self.inner
            .collect_all(req, move |r| {
                let inner = Arc::clone(&inner);
                async move {
                    inner
                        .post_paginated::<T8412Request, T8412Response>(
                            &ls_core::endpoint_policy::T8412_POLICY,
                            &r,
                        )
                        .await
                }
            })
            .await
    }

    /// Fetch a SINGLE page of the `t1452` top-volume rank screen.
    ///
    /// Dispatches through [`ls_core::Inner::post_paginated`] with empty
    /// `tr_cont`/`tr_cont_key` headers; the body `idx` cursor carries the page
    /// position. Single-page scope only — no multi-page body-`idx` collection
    /// (deferred follow-up work).
    pub async fn top_volume(&self, req: &T1452Request) -> LsResult<T1452Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T1452_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of `t1403` newly-listed stocks (date-range screen).
    pub async fn new_listings(&self, req: &T1403Request) -> LsResult<T1403Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T1403_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of `t1441` top change-rate.
    pub async fn top_change_rate(&self, req: &T1441Request) -> LsResult<T1441Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T1441_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of `t1463` top trading value.
    pub async fn top_value(&self, req: &T1463Request) -> LsResult<T1463Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T1463_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of `t1466` volume-surge screen.
    pub async fn volume_surge(&self, req: &T1466Request) -> LsResult<T1466Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T1466_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of `t1489` top expected-execution volume.
    pub async fn top_expected_volume(&self, req: &T1489Request) -> LsResult<T1489Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T1489_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of `t1492` single-price expected change rate.
    pub async fn single_price_expected(&self, req: &T1492Request) -> LsResult<T1492Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T1492_POLICY, req)
            .await
    }

    /// List the account's server-saved screening conditions (`t1866`).
    ///
    /// Each returned `outblock1` row carries a `query_index` that keys a
    /// `t1859`/`t1860` condition search — the saved-condition spine producer.
    /// Single-page (body `cont`/`cont_key` cursor empty).
    pub async fn saved_conditions(&self, req: &T1866Request) -> LsResult<T1866Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T1866_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of the `t3341` financial ranking (재무순위종합).
    ///
    /// Dispatches through [`ls_core::Inner::post_paginated`] with empty header
    /// cursors; the body `idx` (first page `0`, serialized as a number) is the
    /// continuation. Single-page scope (Wave 2 / KTD-5); no multi-page collection.
    pub async fn financial_ranking(&self, req: &T3341Request) -> LsResult<T3341Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T3341_POLICY, req)
            .await
    }

    /// Read one sector's period trend (업종기간별추이) via `t1514`. Self-paginated
    /// on the body `cts_date` cursor (`cnt` serialized as a number); single-page
    /// scope — no multi-page collection.
    pub async fn sector_trend(&self, req: &T1514Request) -> LsResult<T1514Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T1514_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of `t1481` after-hours top change-rate (시간외등락율상위).
    ///
    /// Dispatches through [`ls_core::Inner::post_paginated`] with empty header
    /// cursors; the body `idx` (first page `0`, serialized as a number) is the
    /// continuation. Single-page scope — no multi-page body-`idx` collection.
    pub async fn after_hours_top_change_rate(
        &self,
        req: &T1481Request,
    ) -> LsResult<T1481Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T1481_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of `t1482` after-hours top volume (시간외거래량상위).
    ///
    /// Dispatches through [`ls_core::Inner::post_paginated`] with empty header
    /// cursors; the body `idx` (first page `0`, serialized as a number) is the
    /// continuation. Single-page scope — no multi-page body-`idx` collection.
    pub async fn after_hours_top_volume(&self, req: &T1482Request) -> LsResult<T1482Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T1482_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of `t8410` stock chart (일주월년). Self-paginated on the
    /// body `cts_date` cursor; single-page scope (plan -004).
    pub async fn stock_chart_period(&self, req: &T8410Request) -> LsResult<T8410Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T8410_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of `t8451` integrated stock chart (일주월년).
    /// Self-paginated on the body `cts_date` cursor; single-page scope (plan -004).
    pub async fn stock_chart_period_unified(
        &self,
        req: &T8451Request,
    ) -> LsResult<T8451Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T8451_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of `t8419` sector chart (일주월). Self-paginated on the
    /// body `cts_date` cursor; single-page scope (plan -004).
    pub async fn sector_chart_period(&self, req: &T8419Request) -> LsResult<T8419Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T8419_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of `t4203` composite sector chart (종합). Self-paginated
    /// on the body `cts_date`/`cts_time` cursors; single-page scope (plan -004).
    pub async fn sector_chart_composite(&self, req: &T4203Request) -> LsResult<T4203Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T4203_POLICY, req)
            .await
    }

    /// Fetch a SINGLE page of `t3401` investment-opinion history (투자의견).
    /// Self-paginated on the body `cts_date` cursor; single-page scope (plan -004).
    pub async fn investment_opinions(&self, req: &T3401Request) -> LsResult<T3401Response> {
        self.inner
            .post_paginated(&ls_core::endpoint_policy::T3401_POLICY, req)
            .await
    }
}
