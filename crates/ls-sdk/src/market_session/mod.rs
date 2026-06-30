//! Market-session dependency class — `t1102` current-price (시세) quote.
//!
//! This is the *market_session* class: market-data queries scoped to a trading
//! session, credentialed but with no account state and — for `t1102` —
//! structurally **non-paginated**. The LS `t1102` TR (주식현재가(시세)조회)
//! returns a single snapshot quote for one symbol, so there is no continuation
//! to thread and no `HasPagination` impl: dispatch is a plain
//! [`ls_core::Inner::post`].
//!
//! ## Wire-compat: string-or-number coercion
//!
//! The LS gateway is inconsistent about whether numeric quote fields arrive as
//! JSON numbers (`"price": 4535`) or JSON strings (`"price": "4535"`) — the
//! captured spec example shows `price`/`volume` as bare numbers while `sign`
//! arrives as a string. Every numeric-bearing field therefore uses
//! [`ls_core::string_or_number`] so both shapes deserialize to the same `String`
//! without a panic. This is the load-bearing behavior R10 preserves; the
//! `market_session_tests` regression pins it against the spec-derived shape.
//!
//! ## No `tr_cont`/`tr_cont_key` in the body — by construction
//!
//! Because `t1102` is not paginated, the request carries NO continuation fields
//! at all. [`T1102Request`] serializes to exactly `{"t1102InBlock":{...}}`, so
//! the continuation tokens can never leak into the request body.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use ls_core::{Inner, LsResult};

// ---- Per-family struct modules (Wave-1 decomposition; pure relocation,
// re-exported so every public path is unchanged). ----
mod quote;
pub use quote::*;
mod quote_deriv;
pub use quote_deriv::*;
mod investor_flow;
pub use investor_flow::*;
mod charts;
pub use charts::*;
mod etf;
pub use etf::*;
mod elw;
pub use elw::*;
mod masters;
pub use masters::*;
mod reference;
pub use reference::*;
mod ranking;
pub use ranking::*;

/// Market-session operations, backed by the shared runtime core.
///
/// Cheap to clone — shares `Arc<Inner>` (and therefore the token cache and rate
/// limiter) with the rest of the SDK.
#[derive(Clone)]
pub struct MarketSession {
    inner: Arc<Inner>,
}

impl MarketSession {
    /// Wrap a shared runtime core.
    pub fn new(inner: Arc<Inner>) -> Self {
        MarketSession { inner }
    }

    /// Fetch the current-price (시세) snapshot for one symbol via `t1102`.
    ///
    /// Dispatches through [`ls_core::Inner::post`] (retry + rate limit on the
    /// MarketData bucket). `t1102` is not paginated, so this is a single,
    /// non-continuation POST. A `01900` business code surfaces as
    /// [`ls_core::LsError::ApiError`] and classifies as paper-incompatible.
    pub async fn quote(&self, req: &T1102Request) -> LsResult<T1102Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1102_POLICY, req)
            .await
    }

    /// Fetch minute-by-minute prices (분별주가) for one symbol via `t1302`.
    /// Non-paginated single call on the MarketData bucket (plan -004).
    pub async fn minute_prices(&self, req: &T1302Request) -> LsResult<T1302Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1302_POLICY, req)
            .await
    }

    /// Fetch the F/O tick/min trade chart (선물옵션틱분별체결) for one contract via
    /// `t2216`. Non-paginated single call on the MarketData bucket (plan -004 batch B).
    pub async fn fo_trade_chart(&self, req: &T2216Request) -> LsResult<T2216Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T2216_POLICY, req)
            .await
    }

    /// `t1532` stock themes ([주식] 섹터). Plan -004 batch C.
    pub async fn stock_themes(&self, req: &T1532Request) -> LsResult<T1532Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1532_POLICY, req)
            .await
    }

    /// `t1533` special themes ([주식] 섹터). Plan -004 batch C.
    pub async fn special_themes(&self, req: &T1533Request) -> LsResult<T1533Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1533_POLICY, req)
            .await
    }

    /// `t1926` credit info ([주식] 기타). Plan -004 batch C.
    pub async fn credit_info(&self, req: &T1926Request) -> LsResult<T1926Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1926_POLICY, req)
            .await
    }

    /// `t1764` member firms ([주식] 거래원). Plan -004 batch C.
    pub async fn member_firms(&self, req: &T1764Request) -> LsResult<T1764Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1764_POLICY, req)
            .await
    }

    /// `t1903` etf daily trend ([주식] ETF). Plan -004 batch C.
    pub async fn etf_daily_trend(&self, req: &T1903Request) -> LsResult<T1903Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1903_POLICY, req)
            .await
    }

    /// Fetch the ETF current-price (시세) snapshot for one short code via `t1901`.
    /// Non-paginated; dispatches through [`ls_core::Inner::post`] on the MarketData
    /// bucket.
    pub async fn etf_quote(&self, req: &T1901Request) -> LsResult<T1901Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1901_POLICY, req)
            .await
    }

    /// Fetch the integrated current-price + order-book (호가) level-2 snapshot for one
    /// short code via `t8450`. Non-paginated; dispatches through
    /// [`ls_core::Inner::post`] on the MarketData bucket.
    pub async fn current_price_orderbook(&self, req: &T8450Request) -> LsResult<T8450Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8450_POLICY, req)
            .await
    }

    /// Fetch the per-stock remaining-quantity / pre-disclosure ranking via `t1638`
    /// (`shcode` may be empty for the full list). Non-paginated; dispatches through
    /// [`ls_core::Inner::post`] on the MarketData bucket.
    pub async fn remaining_quantity_predisclosure(
        &self,
        req: &T1638Request,
    ) -> LsResult<T1638Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1638_POLICY, req)
            .await
    }

    /// Fetch the time-bucketed trade chart via `t1308` (`starttime`/`endtime` may
    /// be empty for the full session). Non-paginated; dispatches through
    /// [`ls_core::Inner::post`] on the MarketData bucket.
    pub async fn time_bucket_trade_chart(
        &self,
        req: &T1308Request,
    ) -> LsResult<T1308Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1308_POLICY, req)
            .await
    }

    /// Fetch the price-band trade-weight board via `t1449` (`dategb` must be
    /// non-empty, e.g. `"1"`). Non-paginated; dispatches through
    /// [`ls_core::Inner::post`] on the MarketData bucket.
    pub async fn price_band_trade_weight(
        &self,
        req: &T1449Request,
    ) -> LsResult<T1449Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1449_POLICY, req)
            .await
    }

    /// Fetch the by-time, by-sector investor-trading board via `t1621`
    /// (`nmin`/`cnt` wire-serialize as JSON numbers — KTD3). Non-paginated;
    /// dispatches through [`ls_core::Inner::post`] on the MarketData bucket.
    pub async fn investor_trading_by_time(
        &self,
        req: &T1621Request,
    ) -> LsResult<T1621Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1621_POLICY, req)
            .await
    }

    /// Fetch the F/O by-time, by-sector investor-trading board via `t2545`
    /// (`nmin`/`cnt` wire-serialize as JSON numbers — KTD3; use `bgubun="0"`).
    /// Non-paginated; dispatches through [`ls_core::Inner::post`] on the MarketData
    /// bucket.
    pub async fn fo_investor_trading_by_time(
        &self,
        req: &T2545Request,
    ) -> LsResult<T2545Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T2545_POLICY, req)
            .await
    }

    /// Fetch the F/O by-tick/by-minute conclusion board via `t8406`
    /// (`bgubun`/`cnt` wire-serialize as JSON numbers — KTD3). Non-paginated;
    /// dispatches through [`ls_core::Inner::post`] on the MarketData bucket
    /// (`/futureoption/market-data`).
    pub async fn fo_tick_conclusion(&self, req: &T8406Request) -> LsResult<T8406Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8406_POLICY, req)
            .await
    }

    /// Fetch the multi-symbol current-price board via `t8407` (`nrec` wire-serializes
    /// as a JSON number — KTD3; `shcode` packs N six-digit codes with no separators).
    /// Non-paginated; dispatches through [`ls_core::Inner::post`] on the MarketData
    /// bucket.
    pub async fn multi_symbol_current_price(
        &self,
        req: &T8407Request,
    ) -> LsResult<T8407Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8407_POLICY, req)
            .await
    }

    /// Fetch the LP-target ELW issue list via `t1959` (LP대상종목정보조회). An empty
    /// `shcode` returns the full LP-target list. Non-paginated; dispatches through
    /// [`ls_core::Inner::post`] on the MarketData bucket (`/stock/elw`).
    pub async fn lp_target_issues(&self, req: &T1959Request) -> LsResult<T1959Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1959_POLICY, req)
            .await
    }

    /// Fetch the program-trade综합 via `t1631` (프로그램매매종합조회) — the
    /// program-trade open-order remainders / order quantities (`t1631OutBlock`) plus
    /// the offer/bid volume + value totals (`t1631OutBlock1`). Non-paginated;
    /// dispatches through [`ls_core::Inner::post`] on the MarketData bucket
    /// (`/stock/program`).
    pub async fn program_trade_summary(&self, req: &T1631Request) -> LsResult<T1631Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1631_POLICY, req)
            .await
    }

    /// Fetch the program-trade intraday trend via `t1632` (프로그램매매추이(시간)) —
    /// a per-timestamp time series (`t1632OutBlock1`) of the KP200 index + the
    /// all/arbitrage/non-arbitrage buy/sell/net totals, with the cursor in
    /// `t1632OutBlock`. Non-paginated; dispatches through [`ls_core::Inner::post`] on
    /// the MarketData bucket (`/stock/program`).
    pub async fn program_trade_trend_intraday(
        &self,
        req: &T1632Request,
    ) -> LsResult<T1632Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1632_POLICY, req)
            .await
    }

    /// Fetch the program-trade daily trend via `t1633` (프로그램매매추이(일별)) — a
    /// per-date series (`t1633OutBlock1`) of the KP200 index + the program-trade
    /// net totals + volume, with the cursor in `t1633OutBlock`. Non-paginated;
    /// dispatches through [`ls_core::Inner::post`] on the MarketData bucket
    /// (`/stock/program`).
    pub async fn program_trade_trend_daily(&self, req: &T1633Request) -> LsResult<T1633Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1633_POLICY, req)
            .await
    }

    /// Fetch the foreign/institution by-issue trend via `t1716`
    /// (외인기관종목별동향) — a per-day series (`t1716OutBlock`) of the close + volume +
    /// the per-exchange individual/institution/foreign + program flows for one issue.
    /// `prapp` is a numeric request field. Non-paginated; dispatches through
    /// [`ls_core::Inner::post`] on the MarketData bucket (`/stock/frgr-itt`).
    pub async fn foreign_institution_issue_trend(
        &self,
        req: &T1716Request,
    ) -> LsResult<T1716Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1716_POLICY, req)
            .await
    }

    /// Fetch the ETF intraday NAV/price trend via `t1902` (ETF시간별추이) — the ETF
    /// header (`t1902OutBlock`: name + sector-index name) plus a per-timestamp series
    /// (`t1902OutBlock1`) of price/NAV/index for one ETF `shcode`. Non-paginated;
    /// dispatches through [`ls_core::Inner::post`] on the MarketData bucket
    /// (`/stock/etf`).
    pub async fn etf_intraday_trend(&self, req: &T1902Request) -> LsResult<T1902Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1902_POLICY, req)
            .await
    }

    /// Fetch the ETF PDF / constituent basket via `t1904` (ETF구성종목조회) — the ETF
    /// header (`t1904OutBlock`: quote + NAV + fund totals) plus the constituent rows
    /// (`t1904OutBlock1`: per-issue price/weight/evaluation amount) for one ETF
    /// `shcode` on a PDF apply `date`. Non-paginated; dispatches through
    /// [`ls_core::Inner::post`] on the MarketData bucket (`/stock/etf`).
    pub async fn etf_constituents(&self, req: &T1904Request) -> LsResult<T1904Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1904_POLICY, req)
            .await
    }

    /// Fetch the short-selling daily trend via `t1927` (공매도일별추이) — a per-date
    /// series (`t1927OutBlock1`) of close/volume/value + the short-sale volume / value
    /// / average price for one issue, with the cursor in `t1927OutBlock`.
    /// Non-paginated; dispatches through [`ls_core::Inner::post`] on the MarketData
    /// bucket (`/stock/etc`).
    pub async fn short_sale_daily_trend(&self, req: &T1927Request) -> LsResult<T1927Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1927_POLICY, req)
            .await
    }

    /// Fetch the per-issue stock-loan/대차 daily trend via `t1941`
    /// (종목별대차거래일간추이) — a per-date series (`t1941OutBlock1`) of close/volume +
    /// the loan execute/repay/balance flows + balance amount for one issue.
    /// Non-paginated; dispatches through [`ls_core::Inner::post`] on the MarketData
    /// bucket (`/stock/etc`).
    pub async fn stock_loan_daily_trend(&self, req: &T1941Request) -> LsResult<T1941Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1941_POLICY, req)
            .await
    }

    /// Fetch the foreign/institution by-issue trend via `t1702`
    /// (외국인/기관별 매매추이) — a per-day series (`t1702OutBlock1`) of close/volume +
    /// the per-investor net columns for one issue. Non-paginated; dispatches through
    /// [`ls_core::Inner::post`] on the MarketData bucket (`/stock/frgr-itt`).
    pub async fn foreign_institution_trend(
        &self,
        req: &T1702Request,
    ) -> LsResult<T1702Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1702_POLICY, req)
            .await
    }

    /// Fetch the foreign/institution net-buy trend via `t1717`
    /// (외국인/기관 순매수추이) — a per-day series (`t1717OutBlock`) of close/volume +
    /// the per-investor net-buy-quantity columns for one issue. Non-paginated;
    /// dispatches through [`ls_core::Inner::post`] on the MarketData bucket
    /// (`/stock/frgr-itt`).
    pub async fn foreign_institution_net_buy_trend(
        &self,
        req: &T1717Request,
    ) -> LsResult<T1717Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1717_POLICY, req)
            .await
    }

    /// Fetch the investor-by-sector trend chart via `t1665` (투자자별 매매추이(업종)) —
    /// the sector header (`t1665OutBlock`) + a per-date series (`t1665OutBlock1`) of
    /// the per-investor quantity/value columns + the market index `jisu`.
    /// Non-paginated; dispatches through [`ls_core::Inner::post`] on the MarketData
    /// bucket (`/stock/chart`).
    pub async fn sector_investor_trend(&self, req: &T1665Request) -> LsResult<T1665Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1665_POLICY, req)
            .await
    }

    /// Fetch the intraday best-quote-remainder trend via `t1471`
    /// (시간대별호가잔량추이) — the scalar quote header (`t1471OutBlock`) + a per-slot
    /// order-book series (`t1471OutBlock1`) of best bid/offer prices + remainders +
    /// totals for one issue. Non-paginated; dispatches through
    /// [`ls_core::Inner::post`] on the MarketData bucket (`/stock/market-data`).
    pub async fn intraday_quote_remainder_trend(
        &self,
        req: &T1471Request,
    ) -> LsResult<T1471Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1471_POLICY, req)
            .await
    }

    /// Fetch the VP-relative rise/fall ranking via `t1475` (VP대비등락률상하위) — the
    /// echo header (`t1475OutBlock`) + a ranked series (`t1475OutBlock1`) of
    /// price/change/volume + the VP moving averages for one issue. Non-paginated;
    /// dispatches through [`ls_core::Inner::post`] on the MarketData bucket
    /// (`/stock/market-data`).
    pub async fn vp_change_ranking(&self, req: &T1475Request) -> LsResult<T1475Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1475_POLICY, req)
            .await
    }

    /// Fetch the ELW current-price/quote via `t1950` (ELW현재가(시세)조회) for one
    /// `shcode` (a fresh, non-expired ELW issue code — e.g. the first `shcode` of
    /// `t8431`). Non-paginated; dispatches through [`ls_core::Inner::post`] on the
    /// MarketData bucket (`/stock/elw`).
    pub async fn elw_quote(&self, req: &T1950Request) -> LsResult<T1950Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1950_POLICY, req)
            .await
    }

    /// Fetch the ELW current-price + 10-level quote board via `t1971`
    /// (ELW현재가호가조회) for one `shcode` (a fresh, non-expired ELW issue code —
    /// e.g. the first `shcode` of `t8431`). Non-paginated; dispatches through
    /// [`ls_core::Inner::post`] on the MarketData bucket (`/stock/elw`).
    pub async fn elw_quote_board(&self, req: &T1971Request) -> LsResult<T1971Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1971_POLICY, req)
            .await
    }

    /// Fetch the ELW current-price + trading-member (거래원) board via `t1972`
    /// (ELW현재가(거래원)조회) for one `shcode` (a fresh, non-expired ELW issue code —
    /// e.g. the first `shcode` of `t8431`). Non-paginated; dispatches through
    /// [`ls_core::Inner::post`] on the MarketData bucket (`/stock/elw`).
    pub async fn elw_member_board(&self, req: &T1972Request) -> LsResult<T1972Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1972_POLICY, req)
            .await
    }

    /// Fetch the set of ELW issues sharing a base asset via `t1974`
    /// (ELW기초자산동일종목) for one `shcode` (a fresh, non-expired ELW issue code —
    /// e.g. the first `shcode` of `t8431`). Returns the `t1974OutBlock1` sibling-issue
    /// array (plus the `cnt` summary). Non-paginated; dispatches through
    /// [`ls_core::Inner::post`] on the MarketData bucket (`/stock/elw`).
    pub async fn elw_same_base_issues(&self, req: &T1974Request) -> LsResult<T1974Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1974_POLICY, req)
            .await
    }

    /// Fetch the ELW current-price / contracted-payout snapshot via `t1956`
    /// (ELW현재가(확정지급액)조회) for one `shcode` (a fresh, non-expired ELW issue code —
    /// e.g. the first `shcode` of `t8431`). Returns the `t1956OutBlock` snapshot (its
    /// name `hname`, current price, payout and ELW analytics) plus the
    /// `t1956OutBlock1` basket array. Non-paginated; dispatches through
    /// [`ls_core::Inner::post`] on the MarketData bucket (`/stock/elw`).
    pub async fn elw_current_price(&self, req: &T1956Request) -> LsResult<T1956Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1956_POLICY, req)
            .await
    }

    /// Fetch the ELW daily-price series via `t1954` (ELW일별주가) for one `shcode`
    /// (a fresh, non-expired ELW issue code — e.g. the first `shcode` of `t8431`).
    /// Returns the `t1954OutBlock1` daily OHLCV + ELW-analytics array (plus the
    /// `t1954OutBlock` base-asset header). `cnt` serializes as a JSON number.
    /// Non-paginated; dispatches through [`ls_core::Inner::post`] on the MarketData
    /// bucket (`/stock/elw`).
    pub async fn elw_daily(&self, req: &T1954Request) -> LsResult<T1954Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1954_POLICY, req)
            .await
    }

    /// Run the ELW screener via `t1969` (ELW지표검색). [`T1969Request::new`] builds
    /// the unfiltered "all ELWs" board. The numeric range bounds serialize as JSON
    /// numbers (`IGW40011` otherwise). Non-paginated; dispatches through
    /// [`ls_core::Inner::post`] on the MarketData bucket (`/stock/elw`).
    pub async fn elw_screener(&self, req: &T1969Request) -> LsResult<T1969Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1969_POLICY, req)
            .await
    }

    /// Fetch the ETF LP order-book (LP호가) snapshot for one short code via `t1906`.
    /// Non-paginated; dispatches through [`ls_core::Inner::post`] on the MarketData
    /// bucket.
    pub async fn etf_lp_order_book(&self, req: &T1906Request) -> LsResult<T1906Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1906_POLICY, req)
            .await
    }

    /// Fetch pivot / demark levels for one symbol via `t1105` (non-paginated).
    pub async fn pivot_demark(&self, req: &T1105Request) -> LsResult<T1105Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1105_POLICY, req)
            .await
    }

    /// Fetch the current-price memo rows for one symbol via `t1104` (non-paginated).
    pub async fn price_memo(&self, req: &T1104Request) -> LsResult<T1104Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1104_POLICY, req)
            .await
    }

    /// Fetch the current-price + order-book (호가) snapshot for one symbol via
    /// `t1101`.
    ///
    /// Dispatches through [`ls_core::Inner::post`] (retry + rate limit on the
    /// MarketData bucket). `t1101` is not paginated, so this is a single,
    /// non-continuation POST. A `01900` business code surfaces as
    /// [`ls_core::LsError::ApiError`] and classifies as paper-incompatible.
    pub async fn order_book(&self, req: &T1101Request) -> LsResult<T1101Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1101_POLICY, req)
            .await
    }

    /// Fetch the full theme list (전체테마) via `t8425`.
    ///
    /// Dispatches through [`ls_core::Inner::post`] (retry + rate limit on the
    /// MarketData bucket). `t8425` is not paginated and takes no caller input, so
    /// this is a single, non-continuation POST returning every theme's
    /// name + code. The returned `tmcode` values are the representative caller
    /// inputs for theme-keyed reads (`t1531`/`t1537`). A `01900` business code
    /// surfaces as [`ls_core::LsError::ApiError`] and classifies as
    /// paper-incompatible.
    pub async fn all_themes(&self, req: &T8425Request) -> LsResult<T8425Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8425_POLICY, req)
            .await
    }

    /// Fetch the stock master list (주식종목조회) for one market segment via
    /// `t8436`.
    ///
    /// Dispatches through [`ls_core::Inner::post`] (retry + rate limit on the
    /// MarketData bucket). `t8436` is not paginated; `gubun` is a market-segment
    /// filter (`"0"` all / `"1"` KOSPI / `"2"` KOSDAQ), not an instrument
    /// identifier. A `01900` business code surfaces as
    /// [`ls_core::LsError::ApiError`] and classifies as paper-incompatible.
    pub async fn stock_list(&self, req: &T8436Request) -> LsResult<T8436Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8436_POLICY, req)
            .await
    }

    /// Fetch the constituent stocks of one theme (테마별종목) via `t1531`.
    ///
    /// Dispatches through [`ls_core::Inner::post`] (non-paginated). The theme is
    /// identified by a matched `tmname`+`tmcode` pair (both required by the spec);
    /// source one from [`MarketSession::all_themes`].
    pub async fn theme_stocks(&self, req: &T1531Request) -> LsResult<T1531Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1531_POLICY, req)
            .await
    }

    /// Fetch per-stock quotes for one theme (테마종목별시세조회) via `t1537`.
    ///
    /// Dispatches through [`ls_core::Inner::post`] (non-paginated). Keyed by
    /// `tmcode`; the response carries a theme summary plus a per-stock quote array.
    pub async fn theme_quotes(&self, req: &T1537Request) -> LsResult<T1537Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1537_POLICY, req)
            .await
    }

    /// Run a server-saved condition search (서버저장조건 조건검색) via `t1859`.
    ///
    /// Dispatches through [`ls_core::Inner::post`] (non-paginated). Keyed by a
    /// `query_index` produced by `t1866` ([`crate::paginated::Paginated::saved_conditions`]);
    /// the response carries a search summary plus the matched-issue array. A
    /// `01900` business code surfaces as [`ls_core::LsError::ApiError`] and
    /// classifies as paper-incompatible.
    pub async fn condition_search(&self, req: &T1859Request) -> LsResult<T1859Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1859_POLICY, req)
            .await
    }

    /// List the available ThinQ Q-click searches (종목Q클릭검색리스트조회) via
    /// `t1826` — the Wave 3 producer.
    ///
    /// Dispatches through [`ls_core::Inner::post`] (non-paginated). `search_gb`
    /// selects the search catalog (`"0"` 핵심검색 being representative); the
    /// response carries the `search_cd` catalog keys consumed by `t1825`
    /// ([`MarketSession::qclick_search`]). A `01900` business code surfaces as
    /// [`ls_core::LsError::ApiError`] and classifies as paper-incompatible.
    pub async fn qclick_search_list(&self, req: &T1826Request) -> LsResult<T1826Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1826_POLICY, req)
            .await
    }

    /// Run one ThinQ Q-click search (종목Q클릭검색) via `t1825` — the Wave 3
    /// consumer.
    ///
    /// Dispatches through [`ls_core::Inner::post`] (non-paginated). Keyed by a
    /// `search_cd` produced by `t1826` ([`MarketSession::qclick_search_list`]);
    /// the response carries a search summary plus the matched-issue array. A
    /// `01900` business code surfaces as [`ls_core::LsError::ApiError`] and
    /// classifies as paper-incompatible.
    pub async fn qclick_search(&self, req: &T1825Request) -> LsResult<T1825Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1825_POLICY, req)
            .await
    }

    /// List the full underlying-asset universe (기초자산리스트조회) via `t9905`.
    ///
    /// Non-paginated, no caller input. The returned `shcode` values are the
    /// underlying-asset codes consumed by `t1964` (`item`).
    pub async fn underlying_list(&self, req: &T9905Request) -> LsResult<T9905Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T9905_POLICY, req)
            .await
    }

    /// List the ELW expiry months (만기월조회) via `t9907`. Non-paginated, no
    /// caller input.
    pub async fn elw_expiry_months(&self, req: &T9907Request) -> LsResult<T9907Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T9907_POLICY, req)
            .await
    }

    /// List the ELW symbol universe (ELW종목조회) via `t8431` — the Wave 1 spine
    /// producer. Non-paginated, no caller input; the returned `shcode` values are
    /// the ELW codes consumed by `t1958` ([`MarketSession::elw_compare`]).
    pub async fn elw_symbols(&self, req: &T8431Request) -> LsResult<T8431Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8431_POLICY, req)
            .await
    }

    /// List the ELW master universe (ELW마스터조회) via `t9942`. Non-paginated,
    /// no caller input.
    pub async fn elw_master(&self, req: &T9942Request) -> LsResult<T9942Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T9942_POLICY, req)
            .await
    }

    /// Compare two ELW symbols (ELW종목비교) via `t1958`. Non-paginated; keyed by
    /// two `shcode`s sourced from `t8431` ([`MarketSession::elw_symbols`]); the
    /// response carries each symbol's detail plus a comparison block.
    pub async fn elw_compare(&self, req: &T1958Request) -> LsResult<T1958Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1958_POLICY, req)
            .await
    }

    /// Read the ELW board (ELW전광판) for one underlying via `t1964`.
    /// Non-paginated; keyed by an `item` underlying-asset code sourced from
    /// `t9905` ([`MarketSession::underlying_list`]), with broad/default filters.
    pub async fn elw_board(&self, req: &T1964Request) -> LsResult<T1964Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1964_POLICY, req)
            .await
    }

    /// Read the investor-by-type aggregate (투자자별종합) via `t1601`. Non-paginated.
    pub async fn investor_aggregate(&self, req: &T1601Request) -> LsResult<T1601Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1601_POLICY, req)
            .await
    }

    /// Read the investor trading aggregate (투자자매매종합1) via `t1615`.
    /// Non-paginated.
    pub async fn investor_trading(&self, req: &T1615Request) -> LsResult<T1615Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1615_POLICY, req)
            .await
    }

    /// Read the program-trading aggregate (프로그램매매종합, mini) via `t1640`.
    /// Non-paginated.
    pub async fn program_aggregate(&self, req: &T1640Request) -> LsResult<T1640Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1640_POLICY, req)
            .await
    }

    /// Read the by-time program-trading chart (시간대별프로그램매매추이) via `t1662`.
    /// Non-paginated.
    pub async fn program_chart(&self, req: &T1662Request) -> LsResult<T1662Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1662_POLICY, req)
            .await
    }

    /// Read the investor trading chart (투자자매매종합 챠트) via `t1664`.
    /// Non-paginated.
    pub async fn investor_chart(&self, req: &T1664Request) -> LsResult<T1664Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1664_POLICY, req)
            .await
    }

    /// List every sector (전체업종) via `t8424`. Non-paginated; the anchor and
    /// `upcode` source for the sector cluster.
    pub async fn sectors(&self, req: &T8424Request) -> LsResult<T8424Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8424_POLICY, req)
            .await
    }

    /// List every stock issue (주식종목조회) via `t8430`. Non-paginated; returns the
    /// full KOSPI/KOSDAQ issue array (`shcode`/`hname`/price bounds per issue).
    pub async fn stock_issues(&self, req: &T8430Request) -> LsResult<T8430Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8430_POLICY, req)
            .await
    }

    /// Read one sector's index snapshot (업종현재가) via `t1511`. Non-paginated.
    pub async fn sector_quote(&self, req: &T1511Request) -> LsResult<T1511Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1511_POLICY, req)
            .await
    }

    /// Read one sector's expected/auction index (예상지수) via `t1485`.
    /// Non-paginated.
    pub async fn sector_expected_index(&self, req: &T1485Request) -> LsResult<T1485Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1485_POLICY, req)
            .await
    }

    /// Read the per-sector stock board (업종별종목시세) via `t1516`. Non-paginated;
    /// needs both `upcode` and a `shcode` ticker.
    pub async fn sector_stocks(&self, req: &T1516Request) -> LsResult<T1516Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1516_POLICY, req)
            .await
    }

    /// Read the option board (옵션전광판) via `t2301`. Non-paginated; keyed by a
    /// contract month `yyyymm` (월물) and a `gubun` mini/regular selector.
    pub async fn option_board(&self, req: &T2301Request) -> LsResult<T2301Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T2301_POLICY, req)
            .await
    }

    /// Read the stock-futures underlying-asset master (주식선물기초자산조회) via
    /// `t2522`. Non-paginated, no caller input; returns the underlying-asset
    /// header (name + codes).
    pub async fn stock_futures_underlying_master(
        &self,
        req: &T2522Request,
    ) -> LsResult<T2522Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T2522_POLICY, req)
            .await
    }

    /// Read the stock-futures master (주식선물마스터조회) via `t8401`.
    /// Non-paginated, no caller input; returns the stock-futures contract rows
    /// (name + codes).
    pub async fn stock_futures_master(&self, req: &T8401Request) -> LsResult<T8401Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8401_POLICY, req)
            .await
    }

    /// Read the commodity-futures master (상품선물마스터조회) via `t8426`.
    /// Non-paginated, no caller input; returns the commodity-futures contract
    /// rows (name + codes).
    pub async fn commodity_futures_master(
        &self,
        req: &T8426Request,
    ) -> LsResult<T8426Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8426_POLICY, req)
            .await
    }

    /// Read the price-bearing index-option master (지수옵션마스터조회) via `t8433`.
    ///
    /// Each row carries the contract name + codes PLUS the daily limit/close
    /// reference prices (상한가/하한가/전일종가/전일고가/전일저가/기준가) — the
    /// fuller variant. For the codes-only counterpart (3 identity fields, no
    /// price refs) use [`MarketSession::index_option_master_codes`] (`t9944`).
    /// Non-paginated, no caller input; returns the index-option contract rows.
    pub async fn index_option_master(
        &self,
        req: &T8433Request,
    ) -> LsResult<T8433Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8433_POLICY, req)
            .await
    }

    /// Read the derivatives master (파생종목마스터조회) via `t8435`.
    /// Non-paginated; keyed by a `gubun` segment selector — the MINI/weekly
    /// segments (`"MF"` 미니선물 / `"MO"` 미니옵션 / `"WK"` 코스피200위클리옵션 /
    /// `"SF"` 코스닥150선물 / `"QW"` 코스닥150위클리옵션). Returns the master
    /// snapshot (name + codes + daily limit/close reference prices).
    pub async fn derivatives_master(&self, req: &T8435Request) -> LsResult<T8435Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8435_POLICY, req)
            .await
    }

    /// Read the price-bearing index-futures master (지수선물마스터조회) via `t8467`.
    ///
    /// Each row carries the contract name + codes PLUS the daily limit/close
    /// reference prices (상한가/하한가/전일종가/전일고가/전일저가/기준가) — the
    /// fuller variant. For the codes-only counterpart (3 identity fields, no
    /// price refs) use [`MarketSession::index_futures_master_codes`] (`t9943`).
    /// Non-paginated; keyed by a `gubun` segment selector (`"V"` volatility /
    /// `"S"` sector / `"Q"` KOSDAQ150 / any other value → KOSPI200 index
    /// futures).
    pub async fn index_futures_master(&self, req: &T8467Request) -> LsResult<T8467Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8467_POLICY, req)
            .await
    }

    /// Read the codes-only index-futures master (지수선물마스터조회) via `t9943`.
    ///
    /// The lightweight index-futures master: each row carries only the 3 identity
    /// fields (contract name `hname` + short/expanded codes), with NO daily
    /// price references. This is the distinction from
    /// [`MarketSession::index_futures_master`] (`t8467`), whose rows additionally
    /// carry the daily limit/close reference prices (~9 fields). Both accept the
    /// same `gubun` segment selector (`"V"` volatility / `"S"` sector / any other
    /// value → KOSPI200 index futures); pick this one when only the contract
    /// codes are needed. Non-paginated; returns the master snapshot row array.
    pub async fn index_futures_master_codes(
        &self,
        req: &T9943Request,
    ) -> LsResult<T9943Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T9943_POLICY, req)
            .await
    }

    /// Read the codes-only index-option master (지수옵션마스터조회) via `t9944`.
    ///
    /// The lightweight index-option master: each row carries only the 3 identity
    /// fields (contract name `hname` + short/expanded codes), with NO daily
    /// price references. This is the distinction from
    /// [`MarketSession::index_option_master`] (`t8433`), whose rows additionally
    /// carry the daily limit/close reference prices. Pick this one when only the
    /// contract codes are needed. Non-paginated, no caller input; returns the
    /// master snapshot row array.
    pub async fn index_option_master_codes(
        &self,
        req: &T9944Request,
    ) -> LsResult<T9944Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T9944_POLICY, req)
            .await
    }

    /// Read the F/O current-price (시세) snapshot via `t2111`. Non-paginated;
    /// keyed by a futures/option contract `focode`. Single out-block.
    pub async fn fo_quote(&self, req: &T2111Request) -> LsResult<T2111Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T2111_POLICY, req)
            .await
    }

    /// Read the F/O current-price order book via `t2112`. Non-paginated; keyed by
    /// a contract `shcode`. Single out-block (5-level book).
    pub async fn fo_order_book(&self, req: &T2112Request) -> LsResult<T2112Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T2112_POLICY, req)
            .await
    }

    /// Read the F/O price-memo (시세메모) via `t2106`. Non-paginated; keyed by a
    /// contract `code`. Returns a summary block + a memo-row array.
    pub async fn fo_price_memo(&self, req: &T2106Request) -> LsResult<T2106Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T2106_POLICY, req)
            .await
    }

    /// Read the stock-futures current price via `t8402`. Non-paginated; keyed by
    /// a stock-futures contract `focode`. Single out-block.
    pub async fn stock_futures_quote(&self, req: &T8402Request) -> LsResult<T8402Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8402_POLICY, req)
            .await
    }

    /// Read the stock-futures order book via `t8403`. Non-paginated; keyed by a
    /// stock-futures contract `shcode`. Single out-block (10-level book).
    pub async fn stock_futures_order_book(
        &self,
        req: &T8403Request,
    ) -> LsResult<T8403Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8403_POLICY, req)
            .await
    }

    /// Read the F/O multi current-price via `t8434`. Non-paginated; keyed by a
    /// `qrycnt` count (a JSON number) + one or more `focode` contract codes.
    /// Returns a row array.
    pub async fn fo_multi_quote(&self, req: &T8434Request) -> LsResult<T8434Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8434_POLICY, req)
            .await
    }

    /// Read the ELW underlying-asset list (기초자산리스트조회) via `t1988`.
    /// Non-paginated; `mkt_gb` selects the market segment, all condition filters
    /// off. Routes through `market_session` (KTD3 — the placeholder
    /// `owner_class: standalone` is OAuth-only and cannot host a data read).
    pub async fn elw_underlying_list(&self, req: &T1988Request) -> LsResult<T1988Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T1988_POLICY, req)
            .await
    }

    /// Read a news body (뉴스본문) via `t3102`. Non-paginated; keyed by a news
    /// number (`sNewsno`) sourced only from the realtime `NWS` WebSocket feed.
    /// Routes through `market_session` (KTD3).
    pub async fn news_body(&self, req: &T3102Request) -> LsResult<T3102Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T3102_POLICY, req)
            .await
    }

    /// Read the FnGuide company summary (FNG_요약) via `t3320`. Non-paginated;
    /// keyed by a bare 6-digit ticker (`gicode`, e.g. `"005930"`), confirmed on a
    /// live paper smoke. Routes through `market_session` (KTD3).
    pub async fn company_summary(&self, req: &T3320Request) -> LsResult<T3320Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T3320_POLICY, req)
            .await
    }

    /// Read the KRX night-derivatives master (KRX야간파생 마스터조회) via `t8455`.
    /// Non-paginated; `gubun` selects the instrument class. Returns the master
    /// row array. `venue_session: krx_extended` — the data is only meaningful in
    /// the night session (~18:00–05:00 KST), not the regular clock (KTD7).
    pub async fn night_derivatives_master(&self, req: &T8455Request) -> LsResult<T8455Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8455_POLICY, req)
            .await
    }

    /// Read the KRX night-derivatives option board (KRX야간파생 옵션 전광판) via
    /// `t8460`. Non-paginated; keyed by a contract month `yyyymm` + an index
    /// `gubun`. Returns the near-month header + call/put option arrays.
    /// `venue_session: krx_extended` (KTD7).
    pub async fn night_option_board(&self, req: &T8460Request) -> LsResult<T8460Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8460_POLICY, req)
            .await
    }

    /// Read the KRX night-derivatives investor-by-timeslot (KRX야간파생
    /// 투자자시간대별) via `t8463`. Non-paginated; keyed by a timeslot `tm_rng`, an
    /// F/O distinction `fot_clsf_cd`, and an underlying `bsc_asts_id`; `cnt` is a
    /// numeric count (JSON number, KTD4). Returns the investor-code header + a
    /// time-series row array. `venue_session: krx_extended` (KTD7).
    pub async fn night_investor_timeslot(&self, req: &T8463Request) -> LsResult<T8463Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8463_POLICY, req)
            .await
    }

    /// Read the overseas current price (해외주식 현재가) via `g3101`. Non-paginated;
    /// keyed by an exchange code + symbol (e.g. `82`/`TSLA`). Single out-block.
    /// `instrument_domain: overseas_stock`, `venue_session: unspecified`.
    pub async fn overseas_quote(&self, req: &G3101Request) -> LsResult<G3101Response> {
        self.inner
            .post(&ls_core::endpoint_policy::G3101_POLICY, req)
            .await
    }

    /// Read the overseas stock-info master (해외주식 종목정보) via `g3104`.
    /// Non-paginated; keyed by an exchange code + symbol. Single out-block.
    pub async fn overseas_stock_info(&self, req: &G3104Request) -> LsResult<G3104Response> {
        self.inner
            .post(&ls_core::endpoint_policy::G3104_POLICY, req)
            .await
    }

    /// Read the overseas current price + order book (해외주식 현재가호가) via
    /// `g3106`. Non-paginated; keyed by an exchange code + symbol. Single
    /// out-block (level-1 book).
    pub async fn overseas_order_book(&self, req: &G3106Request) -> LsResult<G3106Response> {
        self.inner
            .post(&ls_core::endpoint_policy::G3106_POLICY, req)
            .await
    }

    /// Read the overseas time-series ticks (해외주식 시간대별) via `g3102`.
    /// Non-paginated; keyed by an exchange code + symbol; `readcnt`/`cts_seq` are
    /// numeric request fields (JSON numbers, KTD4). Returns a header + tick array.
    pub async fn overseas_time_series(&self, req: &G3102Request) -> LsResult<G3102Response> {
        self.inner
            .post(&ls_core::endpoint_policy::G3102_POLICY, req)
            .await
    }

    /// Read the overseas period chart (해외주식 일주월) via `g3103`. Non-paginated;
    /// keyed by an exchange code + symbol + period `gubun` + `date`. Returns a
    /// header + bar array.
    pub async fn overseas_period_chart(&self, req: &G3103Request) -> LsResult<G3103Response> {
        self.inner
            .post(&ls_core::endpoint_policy::G3103_POLICY, req)
            .await
    }

    /// Read the overseas master list (해외주식 마스터) via `g3190`. Non-paginated;
    /// keyed by a nation code + exchange distinction; `readcnt` is a numeric
    /// request field (JSON number, KTD4). Returns a header + master row array.
    pub async fn overseas_master(&self, req: &G3190Request) -> LsResult<G3190Response> {
        self.inner
            .post(&ls_core::endpoint_policy::G3190_POLICY, req)
            .await
    }

    /// Read the overseas-futures master list (해외선물마스터) via `o3101`.
    /// Non-paginated; `gubun` filters (`""` = all), no instrument identifier.
    /// Returns a master row array. `instrument_domain: overseas_futures`,
    /// `venue_session: unspecified`.
    pub async fn overseas_futures_master(&self, req: &O3101Request) -> LsResult<O3101Response> {
        self.inner
            .post(&ls_core::endpoint_policy::O3101_POLICY, req)
            .await
    }

    /// Read the overseas-future-option master list (해외선물옵션 마스터) via `o3121`.
    /// Non-paginated; keyed by a market distinction + base-product filter. Returns
    /// a master row array. `venue_session: unspecified`.
    pub async fn overseas_option_master(&self, req: &O3121Request) -> LsResult<O3121Response> {
        self.inner
            .post(&ls_core::endpoint_policy::O3121_POLICY, req)
            .await
    }

    /// Read the overseas-futures current price / symbol info (해외선물 현재가) via
    /// `o3105`. Non-paginated; keyed by one `symbol`. Single out-block.
    pub async fn overseas_futures_quote(&self, req: &O3105Request) -> LsResult<O3105Response> {
        self.inner
            .post(&ls_core::endpoint_policy::O3105_POLICY, req)
            .await
    }

    /// Read the overseas-futures current price + order book (해외선물 현재가호가) via
    /// `o3106`. Non-paginated; keyed by one `symbol`. Single out-block (level-1
    /// book).
    pub async fn overseas_futures_order_book(
        &self,
        req: &O3106Request,
    ) -> LsResult<O3106Response> {
        self.inner
            .post(&ls_core::endpoint_policy::O3106_POLICY, req)
            .await
    }

    /// Read the overseas-future-option current price / symbol info (해외선물옵션
    /// 현재가) via `o3125`. Non-paginated; keyed by a market distinction + symbol.
    /// Single out-block.
    pub async fn overseas_option_quote(&self, req: &O3125Request) -> LsResult<O3125Response> {
        self.inner
            .post(&ls_core::endpoint_policy::O3125_POLICY, req)
            .await
    }

    /// Read the overseas-future-option current price + order book (해외선물옵션
    /// 현재가호가) via `o3126`. Non-paginated; keyed by a market distinction +
    /// symbol. Single out-block (level-1 book).
    pub async fn overseas_option_order_book(
        &self,
        req: &O3126Request,
    ) -> LsResult<O3126Response> {
        self.inner
            .post(&ls_core::endpoint_policy::O3126_POLICY, req)
            .await
    }

    /// Read the stock master (주식마스터조회) via `t9945`. Non-paginated; one
    /// market per call (`"1"`=KOSPI, `"2"`=KOSDAQ). Returns the full ticker
    /// master (code/ISIN/name) array.
    pub async fn stock_master(&self, req: &T9945Request) -> LsResult<T9945Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T9945_POLICY, req)
            .await
    }

    /// Read a ticker's market schedule (종목별증시일정) via `t3202`. Non-paginated;
    /// keyed by `shcode`. Returns the corporate-event schedule rows.
    pub async fn stock_schedule(&self, req: &T3202Request) -> LsResult<T3202Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T3202_POLICY, req)
            .await
    }

    /// Read one overseas index's current snapshot (해외지수조회) via `t3521`.
    /// Non-paginated; keyed by `kind`/`symbol` (e.g. `"S"`/`"DJI@DJI"`).
    pub async fn overseas_index_quote(&self, req: &T3521Request) -> LsResult<T3521Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T3521_POLICY, req)
            .await
    }

    /// Read overseas-futures daily executions (해외선물 일별체결) via `o3104`.
    /// Non-paginated; keyed by `shcode`/`date`. A daily-row array.
    pub async fn overseas_futures_daily(&self, req: &O3104Request) -> LsResult<O3104Response> {
        self.inner
            .post(&ls_core::endpoint_policy::O3104_POLICY, req)
            .await
    }

    /// Read the overseas-futopt watchlist board (해외선물옵션 관심종목) via `o3127`.
    /// Non-paginated; keyed by `nrec`. A board-row array.
    pub async fn overseas_futopt_watchlist(&self, req: &O3127Request) -> LsResult<O3127Response> {
        self.inner
            .post(&ls_core::endpoint_policy::O3127_POLICY, req)
            .await
    }

    /// Read the F/O minute/day chart (선물옵션 N분주가) via `t8427`. Non-paginated;
    /// keyed by a contract `focode`. OHLCV rows under `t8427OutBlock1`.
    pub async fn fo_minute_day_chart(&self, req: &T8427Request) -> LsResult<T8427Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8427_POLICY, req)
            .await
    }

    /// Read the F/O unusual-volume conclusion counts (선물옵션 특이거래량) over a time
    /// window via `t2210`. Non-paginated; the buy/sell 체결수량 are the witnesses.
    pub async fn fo_unusual_volume(&self, req: &T2210Request) -> LsResult<T2210Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T2210_POLICY, req)
            .await
    }

    /// Read the F/O N-minute bars (선물옵션 N분봉) via `t2424`. Non-paginated; current
    /// price header + a bar array under `t2424OutBlock1`.
    pub async fn fo_minute_bars(&self, req: &T2424Request) -> LsResult<T2424Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T2424_POLICY, req)
            .await
    }

    /// Read the deposit-balance trend by investor (투자자별 예탁금추이) via `t8428`.
    /// Non-paginated; deposit-info rows under `t8428OutBlock1`.
    pub async fn deposit_balance_trend(&self, req: &T8428Request) -> LsResult<T8428Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8428_POLICY, req)
            .await
    }

    /// Read the KRX night-derivatives investor-period table (KRX야간파생 투자자기간별)
    /// via `t8462`. Non-paginated; keyed by basis asset + a `from_date`..`to_date`
    /// range. An investor-row array.
    pub async fn night_derivatives_investor_period(
        &self,
        req: &T8462Request,
    ) -> LsResult<T8462Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T8462_POLICY, req)
            .await
    }

    /// Read the gateway server time (서버시간조회) via `t0167`. A stateless utility,
    /// closure-viable (the clock is always populated). Non-paginated.
    pub async fn server_time(&self, req: &T0167Request) -> LsResult<T0167Response> {
        self.inner
            .post(&ls_core::endpoint_policy::T0167_POLICY, req)
            .await
    }
}

