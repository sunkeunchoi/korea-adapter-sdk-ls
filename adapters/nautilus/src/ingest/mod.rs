//! Historical bar ingestion into a `ParquetDataCatalog` (U3).
//!
//! Per KTD4/KTD5/KTD9: an adapter-side per-TR [`pacer`] meters t8410/t8412 to the
//! stricter of their per-TR and category caps; **daily** bars (t8410) are walked on
//! the body `cts_date` cursor (which is exactly the checkpointing seam R5 needs);
//! **minute** bars (t8412) are pulled with `chart_all` per conservative date chunk,
//! halving the chunk and requeueing on `PaginationLimit` (the SDK discards fetched
//! pages on that error, so chunk sizing is the cost control). LS returns KST
//! wall-clock strings; the adapter converts to UTC `UnixNanos` with `ts_event` =
//! **bar close** (Nautilus convention). Runs are resumable via [`checkpoint`].

pub mod checkpoint;
pub mod pacer;

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use ls_core::endpoint_policy::{T8410_POLICY, T8412_POLICY};
use ls_core::LsError;
use ls_sdk::paginated::{
    T8410OutBlock1, T8410Request, T8410Response, T8412OutBlock1, T8412Request, T8412Response,
};
use ls_sdk::LsSdk;
use nautilus_core::UnixNanos;
use nautilus_model::data::{Bar, BarSpecification, BarType};
use nautilus_model::enums::{AggregationSource, BarAggregation, PriceType};
use nautilus_model::identifiers::InstrumentId;
use nautilus_model::types::{Price, Quantity};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use serde::{Deserialize, Serialize};

use crate::error::{AdapterError, AdapterResult};
use crate::lock::{AdvisoryLock, LockKind};
use crate::parse::strict_i64;
use crate::rules::{KRX_REGULAR_CLOSE, KST_UTC_OFFSET_HOURS};
use self::checkpoint::{Checkpoint, CoverageGap, GapReason, RebaseEvent};
use self::pacer::{Pacer, MARKET_DATA_CATEGORY_PER_SEC};

/// A defensive upper bound on daily-cursor pages per symbol (guards a gateway that
/// never terminates the cursor).
const MAX_DAILY_PAGES: usize = 500;

/// The default post-close safety buffer for the accumulate session-clock rule
/// (16:30 KST): today counts as a *closed* session only after this time, so a
/// post-close cron delivers the just-closed session rather than lagging a day, and
/// the watermark never advances into an in-session day (U5, KTD7).
pub const ACCUMULATE_CLOSE_BUFFER: NaiveTime = match NaiveTime::from_hms_opt(16, 30, 0) {
    Some(t) => t,
    None => unreachable!(),
};

/// The last **closed** trading session date for the current KST wall-clock (U5,
/// KTD7). Today counts as closed once now-KST is past the regular close plus a
/// safety buffer (`close_buffer`); otherwise the last closed session is
/// yesterday-or-earlier. Weekends/holidays with no session simply yield no new
/// bars (the gateway returns a coverage gap) while the watermark still advances.
pub fn last_closed_session(now_kst: NaiveDateTime, close_buffer: NaiveTime) -> NaiveDate {
    if now_kst.time() >= close_buffer {
        now_kst.date()
    } else {
        now_kst
            .date()
            .pred_opt()
            .expect("a date always has a predecessor")
    }
}

/// Which bar series to ingest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarKind {
    /// Daily bars via t8410 (`gubun="2"`).
    Daily,
    /// N-minute bars via t8412.
    Minute(u32),
}

impl BarKind {
    /// A short label used in checkpoint keys + coverage (`1-DAY`, `1-MINUTE`).
    pub fn label(self) -> String {
        match self {
            BarKind::Daily => "1-DAY".to_string(),
            BarKind::Minute(n) => format!("{n}-MINUTE"),
        }
    }

    /// The nautilus [`BarType`] for this kind on `instrument_id` (External source —
    /// required by the backtest engine's `add_data(validate=true)`).
    pub fn bar_type(self, instrument_id: InstrumentId) -> AdapterResult<BarType> {
        let (step, agg) = match self {
            BarKind::Daily => (1usize, BarAggregation::Day),
            BarKind::Minute(n) => (n as usize, BarAggregation::Minute),
        };
        let spec = BarSpecification::new_checked(step, agg, PriceType::Last)
            .map_err(|e| AdapterError::Ingest(format!("bad bar spec {self:?}: {e}")))?;
        Ok(BarType::new(instrument_id, spec, AggregationSource::External))
    }
}

/// Convert a KST wall-clock date + time to a UTC [`UnixNanos`] (KTD9).
///
/// # Errors
///
/// [`AdapterError::FieldParse`] if the date/time cannot be resolved to a unique
/// instant.
pub fn kst_to_unix_nanos(date: NaiveDate, time: NaiveTime) -> AdapterResult<UnixNanos> {
    let naive = NaiveDateTime::new(date, time);
    let kst = FixedOffset::east_opt(KST_UTC_OFFSET_HOURS * 3600)
        .expect("KST offset is valid");
    let dt = match kst.from_local_datetime(&naive).single() {
        Some(dt) => dt,
        None => {
            return Err(AdapterError::FieldParse {
                field: "ts_event".to_string(),
                value: format!("{naive}"),
                reason: "ambiguous KST instant".to_string(),
            })
        }
    };
    let nanos = dt.timestamp_nanos_opt().ok_or_else(|| AdapterError::FieldParse {
        field: "ts_event".to_string(),
        value: format!("{naive}"),
        reason: "timestamp out of range".to_string(),
    })?;
    // `UnixNanos` is a u64; a pre-1970 instant (negative nanos) would wrap to a
    // far-future timestamp. Reject it rather than silently corrupting the bar.
    if nanos < 0 {
        return Err(AdapterError::FieldParse {
            field: "ts_event".to_string(),
            value: format!("{naive}"),
            reason: "pre-epoch timestamp (negative nanos)".to_string(),
        });
    }
    Ok(UnixNanos::from(nanos as u64))
}

fn parse_yyyymmdd(field: &str, s: &str) -> AdapterResult<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y%m%d").map_err(|e| AdapterError::FieldParse {
        field: field.to_string(),
        value: s.to_string(),
        reason: format!("expected YYYYMMDD: {e}"),
    })
}

/// Parse an LS intraday time field (`HHMMSS` or `HHMM`) to a [`NaiveTime`].
fn parse_hms(field: &str, s: &str) -> AdapterResult<NaiveTime> {
    let t = s.trim();
    let fmt = match t.len() {
        6 => "%H%M%S",
        4 => "%H%M",
        _ => {
            return Err(AdapterError::FieldParse {
                field: field.to_string(),
                value: s.to_string(),
                reason: "expected HHMM or HHMMSS".to_string(),
            })
        }
    };
    NaiveTime::parse_from_str(t, fmt).map_err(|e| AdapterError::FieldParse {
        field: field.to_string(),
        value: s.to_string(),
        reason: format!("bad time: {e}"),
    })
}

fn price_from_krw(field: &str, s: &str) -> AdapterResult<Price> {
    let i = strict_i64(field, s)?;
    Ok(Price::from(i.max(0).to_string().as_str()))
}

fn qty_from_str(field: &str, s: &str) -> AdapterResult<Quantity> {
    let i = strict_i64(field, s)?;
    Ok(Quantity::from(i.max(0)))
}

/// Build a daily [`Bar`] from a t8410 row. `ts_event` = the session close
/// (15:30 KST) of the candle date (KTD9).
pub fn build_daily_bar(bar_type: BarType, row: &T8410OutBlock1) -> AdapterResult<Option<Bar>> {
    if row.date.trim().is_empty() {
        return Ok(None);
    }
    let date = parse_yyyymmdd("date", &row.date)?;
    let ts = kst_to_unix_nanos(date, KRX_REGULAR_CLOSE)?;
    build_bar(bar_type, &row.open, &row.high, &row.low, &row.close, &row.jdiff_vol, ts)
}

/// Build a minute [`Bar`] from a t8412 row. `ts_event` = the candle's own KST
/// timestamp (its close), converted to UTC (KTD9).
pub fn build_minute_bar(bar_type: BarType, row: &T8412OutBlock1) -> AdapterResult<Option<Bar>> {
    if row.date.trim().is_empty() || row.time.trim().is_empty() {
        return Ok(None);
    }
    let date = parse_yyyymmdd("date", &row.date)?;
    let time = parse_hms("time", &row.time)?;
    let ts = kst_to_unix_nanos(date, time)?;
    build_bar(bar_type, &row.open, &row.high, &row.low, &row.close, &row.jdiff_vol, ts)
}

#[allow(clippy::too_many_arguments)]
fn build_bar(
    bar_type: BarType,
    open: &str,
    high: &str,
    low: &str,
    close: &str,
    volume: &str,
    ts: UnixNanos,
) -> AdapterResult<Option<Bar>> {
    let bar = Bar::new_checked(
        bar_type,
        price_from_krw("open", open)?,
        price_from_krw("high", high)?,
        price_from_krw("low", low)?,
        price_from_krw("close", close)?,
        qty_from_str("volume", volume)?,
        ts,
        ts,
    );
    match bar {
        Ok(b) => Ok(Some(b)),
        // A row whose OHLC violates high≥open≥low etc. is skipped rather than
        // failing the whole run (real feeds occasionally emit a degenerate row).
        Err(e) => {
            tracing::warn!(error = %e, "skipping malformed OHLC bar");
            Ok(None)
        }
    }
}

// ---------------------------------------------------------------------------
// Fetcher seams — the cursor/narrowing loops are generic over these so their
// failure modes (cursor non-termination, page-discarding cap) are unit-testable
// with fakes, while production fetches route through the SDK + pacer.
// ---------------------------------------------------------------------------

/// Fetches one daily-chart page for a symbol at a body cursor over `[sdate, edate]`.
#[async_trait]
pub trait DailyFetcher {
    /// Fetch the t8410 page for `[sdate, edate]` at `cts_date` (`""` = first page).
    /// Per-call dates (matching [`MinuteFetcher`]) so accumulate-forward can request
    /// a different range per instrument (U5).
    async fn fetch_daily_page(
        &self,
        shcode: &str,
        sdate: &str,
        edate: &str,
        cts_date: &str,
    ) -> AdapterResult<T8410Response>;
}

/// Fetches all minute-chart pages for a symbol over a date chunk.
#[async_trait]
pub trait MinuteFetcher {
    /// Fetch every t8412 page for `[sdate, edate]`. Returns
    /// [`LsError::PaginationLimit`] (wrapped) when the chunk exceeds the page cap.
    async fn fetch_minute_chunk(
        &self,
        shcode: &str,
        ncnt: u32,
        sdate: &str,
        edate: &str,
    ) -> AdapterResult<Vec<T8412Response>>;
}

/// Production fetcher over the SDK, paced per-TR (KTD4). Dates ride per call (U5),
/// so one fetcher serves both the range-global backfill and per-instrument
/// accumulate-forward ranges.
pub struct SdkFetcher {
    sdk: LsSdk,
    daily_pacer: Pacer,
    minute_pacer: Pacer,
    daily_qrycnt: usize,
    minute_qrycnt: usize,
}

impl SdkFetcher {
    fn new(sdk: LsSdk) -> Self {
        SdkFetcher {
            sdk,
            daily_pacer: Pacer::for_policy(&T8410_POLICY, MARKET_DATA_CATEGORY_PER_SEC),
            minute_pacer: Pacer::for_policy(&T8412_POLICY, MARKET_DATA_CATEGORY_PER_SEC),
            daily_qrycnt: 900,
            minute_qrycnt: 900,
        }
    }
}

#[async_trait]
impl DailyFetcher for SdkFetcher {
    async fn fetch_daily_page(
        &self,
        shcode: &str,
        sdate: &str,
        edate: &str,
        cts_date: &str,
    ) -> AdapterResult<T8410Response> {
        self.daily_pacer.acquire().await;
        let mut req = T8410Request::new(
            shcode,
            "2", // daily
            self.daily_qrycnt.to_string(),
            sdate.to_string(),
            edate.to_string(),
        );
        req.inblock.cts_date = cts_date.to_string();
        Ok(self.sdk.paginated().stock_chart_period(&req).await?)
    }
}

#[async_trait]
impl MinuteFetcher for SdkFetcher {
    async fn fetch_minute_chunk(
        &self,
        shcode: &str,
        ncnt: u32,
        sdate: &str,
        edate: &str,
    ) -> AdapterResult<Vec<T8412Response>> {
        self.minute_pacer.acquire().await;
        let req = T8412Request::new(
            shcode,
            ncnt.to_string(),
            self.minute_qrycnt.to_string(),
            "0",
            sdate,
            edate,
            "N",
        );
        Ok(self.sdk.paginated().chart_all(req).await?)
    }
}

/// The outcome of ingesting one `(instrument, bar-kind)` triple.
enum TripleOutcome {
    Bars(Vec<Bar>),
    Gap(GapReason),
}

/// Walk the daily cursor for one symbol, collecting bars. Terminates on an empty
/// next-cursor, a repeated cursor (defensive), an empty page, the page cap, or an
/// `01715` (non-trading-day) error → coverage gap.
async fn collect_daily<F: DailyFetcher>(
    fetcher: &F,
    shcode: &str,
    bar_type: BarType,
    sdate: &str,
    edate: &str,
) -> AdapterResult<TripleOutcome> {
    let mut bars = Vec::new();
    let mut cts_date = String::new();
    let mut seen = HashSet::new();
    let mut hit_cap = true;

    for _ in 0..MAX_DAILY_PAGES {
        let resp = match fetcher.fetch_daily_page(shcode, sdate, edate, &cts_date).await {
            Ok(r) => r,
            Err(AdapterError::Sdk(LsError::ApiError { code, .. })) if code == "01715" => {
                return Ok(TripleOutcome::Gap(GapReason::NonTradingDay));
            }
            Err(e) => return Err(e),
        };
        for row in &resp.outblock1 {
            if let Some(b) = build_daily_bar(bar_type, row)? {
                bars.push(b);
            }
        }
        let next = resp.outblock.cts_date.trim().to_string();
        if next.is_empty() || resp.outblock1.is_empty() || !seen.insert(next.clone()) {
            hit_cap = false;
            break;
        }
        cts_date = next;
    }

    if bars.is_empty() {
        return Ok(TripleOutcome::Gap(GapReason::EmptyHistory));
    }
    // LS daily charts return newest-first and the cursor walks recent→older, so
    // bars accumulate DESCENDING across pages. The catalog requires ascending
    // `ts_init` (the disjoint check is skipped on write), so sort before returning
    // — exactly as `collect_minute` does.
    bars.sort_by_key(|b| b.ts_init.as_u64());
    if hit_cap {
        // The page cap was reached without the cursor terminating — the returned
        // history is truncated, not complete. Surface it as paper-thin/uncertain
        // rather than claiming a full ingest.
        tracing::warn!(shcode, "daily cursor hit the {MAX_DAILY_PAGES}-page cap; history truncated");
        return Ok(TripleOutcome::Gap(GapReason::PaperThin));
    }
    Ok(TripleOutcome::Bars(bars))
}

/// Ingest minute bars for one symbol over `[sdate, edate]`, halving the chunk and
/// requeueing on `PaginationLimit` (KTD5). A single-day chunk that still overflows
/// is recorded as a paper-thin/uningestable gap and skipped.
async fn collect_minute<F: MinuteFetcher>(
    fetcher: &F,
    shcode: &str,
    ncnt: u32,
    bar_type: BarType,
    sdate: &str,
    edate: &str,
) -> AdapterResult<TripleOutcome> {
    let start = parse_yyyymmdd("sdate", sdate)?;
    let end = parse_yyyymmdd("edate", edate)?;
    let mut bars = Vec::new();
    let mut overflowed_single_day = false;
    let mut queue: VecDeque<(NaiveDate, NaiveDate)> = VecDeque::new();
    queue.push_back((start, end));

    while let Some((s, e)) = queue.pop_front() {
        let s_str = s.format("%Y%m%d").to_string();
        let e_str = e.format("%Y%m%d").to_string();
        match fetcher.fetch_minute_chunk(shcode, ncnt, &s_str, &e_str).await {
            Ok(pages) => {
                for page in &pages {
                    for row in &page.outblock1 {
                        if let Some(b) = build_minute_bar(bar_type, row)? {
                            bars.push(b);
                        }
                    }
                }
            }
            Err(AdapterError::Sdk(LsError::PaginationLimit(_))) => {
                if let Some((left, right)) = split_range(s, e) {
                    // Requeue narrower halves at the FRONT so we finish this range
                    // before moving on (keeps memory bounded).
                    queue.push_front(right);
                    queue.push_front(left);
                } else {
                    // Can't narrow below a single day — record and skip.
                    overflowed_single_day = true;
                }
            }
            Err(AdapterError::Sdk(LsError::ApiError { code, .. })) if code == "01715" => {
                // Non-trading sub-range — skip it, keep the rest.
            }
            Err(e) => return Err(e),
        }
    }

    if !bars.is_empty() {
        // Bars may arrive out of order across chunks; sort by ts_event ascending
        // (the catalog requires ascending ts_init).
        bars.sort_by_key(|b| b.ts_init.as_u64());
        Ok(TripleOutcome::Bars(bars))
    } else if overflowed_single_day {
        Ok(TripleOutcome::Gap(GapReason::PaperThin))
    } else {
        Ok(TripleOutcome::Gap(GapReason::EmptyHistory))
    }
}

/// Split a `[s, e]` date range into two halves. Returns `None` if `s == e` (a
/// single day cannot be narrowed).
fn split_range(s: NaiveDate, e: NaiveDate) -> Option<((NaiveDate, NaiveDate), (NaiveDate, NaiveDate))> {
    if s >= e {
        return None;
    }
    let span = (e - s).num_days();
    let mid = s + ChronoDuration::days(span / 2);
    if mid >= e {
        // Adjacent days — split into [s,s] and [e,e].
        Some(((s, s), (e, e)))
    } else {
        Some(((s, mid), (mid + ChronoDuration::days(1), e)))
    }
}

// ---------------------------------------------------------------------------
// Basis-shift detection (KTD-3) — the adjusted daily series is rewritten
// server-side by every split/dividend, so accumulate-forward re-fetches a bounded
// overlap window and exact-compares it against stored bars before appending.
// ---------------------------------------------------------------------------

/// Default overlap-window size: the last N stored trading days ending at the
/// watermark (the `IngestConfig::overlap_days` knob).
pub const DEFAULT_OVERLAP_DAYS: usize = 5;

/// Minimum mutually-present dates for an overlap comparison to be meaningful
/// (KTD-3): fewer — including the no-watermark first-ever accumulate — skips
/// detection entirely rather than marking.
const MIN_OVERLAP_DATES: usize = 3;

/// The calendar start of the overlap window: wide enough that `overlap_days`
/// *trading* days ending at the watermark fit despite weekends/holiday clusters
/// (Seollal/Chuseok). A symbol suspended longer than this simply yields an
/// insufficient overlap and skips detection.
fn overlap_window_start(watermark: NaiveDate, overlap_days: usize) -> NaiveDate {
    watermark - ChronoDuration::days(overlap_days as i64 * 3 + 10)
}

/// The verdict of an overlap comparison (KTD-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverlapVerdict {
    /// Enough mutually-present dates and every OHLC matches exactly.
    Match,
    /// At least one mutually-present date differs — the basis shifted.
    Shifted,
    /// Fewer than [`MIN_OVERLAP_DATES`] mutually-present dates (short history,
    /// long suspension, truncated fetch) — detection is skipped, never marked.
    Insufficient,
}

fn ohlc_by_ts(bars: &[Bar]) -> BTreeMap<u64, (Price, Price, Price, Price)> {
    bars.iter()
        .map(|b| (b.ts_event.as_u64(), (b.open, b.high, b.low, b.close)))
        .collect()
}

/// Exact-compare stored vs freshly-fetched bars on mutually-present dates only
/// (KTD-3). One-sided dates — gap/holiday days with no stored bar, a server-side
/// gap-fill, a dropped bar — are excluded: they are not a basis shift, and
/// including them would re-detect forever. Daily closes are integer KRW on the
/// wire, so the comparison is exact-match, not tolerance-based.
fn compare_overlap(stored: &[Bar], fetched: &[Bar]) -> OverlapVerdict {
    let stored = ohlc_by_ts(stored);
    let fetched = ohlc_by_ts(fetched);
    let mut mutual = 0usize;
    let mut shifted = false;
    for (ts, s) in &stored {
        if let Some(f) = fetched.get(ts) {
            mutual += 1;
            if s != f {
                shifted = true;
            }
        }
    }
    if mutual < MIN_OVERLAP_DATES {
        OverlapVerdict::Insufficient
    } else if shifted {
        OverlapVerdict::Shifted
    } else {
        OverlapVerdict::Match
    }
}

/// Fetch the server's overlap window `[wstart, wend]` for comparison. A gap
/// outcome (empty history, non-trading range, truncated fetch) yields `None` —
/// nothing trustworthy to compare, so detection is skipped (KTD-3).
async fn fetch_overlap<F: DailyFetcher>(
    fetcher: &F,
    shcode: &str,
    bar_type: BarType,
    wstart: NaiveDate,
    wend: NaiveDate,
) -> AdapterResult<Option<Vec<Bar>>> {
    let sdate = fmt_ymd(wstart);
    let edate = fmt_ymd(wend);
    match collect_daily(fetcher, shcode, bar_type, &sdate, &edate).await? {
        TripleOutcome::Bars(bars) => Ok(Some(bars)),
        TripleOutcome::Gap(_) => Ok(None),
    }
}

/// A refused heal wipe (KTD-2 precondition): the run's backfill floor is later
/// than the symbol's earliest stored bar, so wiping would silently truncate
/// stored history. The symbol stays marked until a run with an adequate floor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealRefusal {
    /// The instrument id (`{shcode}.XKRX`).
    pub instrument: String,
    /// The bar-type label (e.g. `1-DAY`).
    pub bar_type: String,
    /// The run's backfill floor (`YYYYMMDD`).
    pub floor: String,
    /// The symbol's earliest stored bar date (`YYYYMMDD`).
    pub earliest_stored: String,
}

/// The outcome of one per-symbol heal attempt.
enum HealOutcome {
    /// Wipe → re-pull → re-verify completed; the mark is cleared and the event
    /// recorded. Carries the number of bars re-written.
    Healed(usize),
    /// The wipe precondition refused (floor later than earliest stored bar).
    Refused(HealRefusal),
    /// The re-pull was truncated or the re-verify mismatched — the mark stays
    /// and the next run re-enters at the wipe.
    Incomplete,
}

/// A per-run request-budget estimate (R4/KTD5).
#[derive(Debug, Clone)]
pub struct BudgetEstimate {
    /// Symbols in the universe.
    pub symbols: usize,
    /// Bar kinds requested per symbol.
    pub bar_kinds: usize,
    /// Requests-per-second cap the run pace to (the stricter per-TR cap).
    pub per_sec_cap: u32,
    /// A conservative lower bound on total requests (one page per triple).
    pub min_requests: usize,
}

impl BudgetEstimate {
    /// A lower-bound wall-clock estimate at the per-second cap.
    pub fn min_seconds(&self) -> f64 {
        if self.per_sec_cap == 0 {
            return f64::INFINITY;
        }
        self.min_requests as f64 / self.per_sec_cap as f64
    }
}

/// The result of an ingest run.
#[derive(Debug, Clone)]
pub struct CoverageReport {
    /// Total bars written across all triples.
    pub bars_written: usize,
    /// Triples that produced bars.
    pub triples_ingested: usize,
    /// Triples skipped because the checkpoint already had them.
    pub triples_skipped: usize,
    /// Coverage gaps recorded this run.
    pub gaps: Vec<checkpoint::CoverageGap>,
    /// Heal wipes refused this run (KTD-2 precondition) — surfaced, never silent.
    pub heal_refusals: Vec<HealRefusal>,
    /// The request-budget estimate for the run.
    pub budget: BudgetEstimate,
}

/// Ingestion configuration.
#[derive(Debug, Clone)]
pub struct IngestConfig {
    /// Directory the `ParquetDataCatalog` + checkpoint + lockfile live in.
    pub catalog_path: PathBuf,
    /// Which bar series to ingest per symbol.
    pub bar_kinds: Vec<BarKind>,
    /// Range start (`YYYYMMDD`, a trading day) for minute chunks.
    pub sdate: String,
    /// Range end (`YYYYMMDD`, a trading day) for minute chunks.
    pub edate: String,
    /// Whether daily bars used adjusted prices (`sujung="Y"`, recorded in the
    /// checkpoint as the catalog price basis).
    pub adjusted_prices: bool,
    /// Basis-shift detection overlap: the last N stored trading days ending at
    /// the watermark are re-fetched and exact-compared before appending (KTD-3).
    /// Use [`DEFAULT_OVERLAP_DAYS`] unless a test needs otherwise.
    pub overlap_days: usize,
}

impl IngestConfig {
    fn checkpoint_path(&self) -> PathBuf {
        self.catalog_path.join("ingest-checkpoint.json")
    }
}

/// The historical-bar ingestor. Holds the SDK-backed fetcher, the catalog path,
/// and the resumable checkpoint. `ls-ingest` is the only entry point (it also
/// takes the R15 advisory lock — see [`Ingestor::run_locked`]).
pub struct Ingestor {
    fetcher: SdkFetcher,
    config: IngestConfig,
}

impl Ingestor {
    /// Build an ingestor over an SDK handle and config.
    pub fn new(sdk: LsSdk, config: IngestConfig) -> Self {
        let fetcher = SdkFetcher::new(sdk);
        Ingestor { fetcher, config }
    }

    /// Run ingestion while holding the R15 ingest lock (refuses if a live session
    /// is running). Releases the lock on return.
    pub async fn run_locked(&mut self, universe: &[InstrumentId]) -> AdapterResult<CoverageReport> {
        let _lock = AdvisoryLock::acquire(&self.config.catalog_path, LockKind::Ingest)?;
        self.run(universe).await
    }

    /// Run ingestion over `universe` into the catalog, resuming from the
    /// checkpoint. (Does not take the lock — use [`Self::run_locked`] for the
    /// entry-point path; this is exposed for tests that drive it directly.)
    pub async fn run(&mut self, universe: &[InstrumentId]) -> AdapterResult<CoverageReport> {
        std::fs::create_dir_all(&self.config.catalog_path).map_err(|e| {
            AdapterError::Ingest(format!("mkdir catalog {}: {e}", self.config.catalog_path.display()))
        })?;
        let checkpoint_path = self.config.checkpoint_path();
        let mut checkpoint = Checkpoint::load(&checkpoint_path)?;
        checkpoint.adjusted_prices = self.config.adjusted_prices;

        let range = format!("{}..{}", self.config.sdate, self.config.edate);
        let mut bars_written = 0usize;
        let mut ingested = 0usize;
        let mut skipped = 0usize;
        let mut gaps_this_run = Vec::new();

        for id in universe {
            let shcode = id.symbol.as_str().to_string();
            for &kind in &self.config.bar_kinds {
                let label = kind.label();
                if checkpoint.is_done(&id.to_string(), &label, &range) {
                    skipped += 1;
                    continue;
                }
                let bar_type = kind.bar_type(*id)?;
                let outcome = match kind {
                    BarKind::Daily => {
                        collect_daily(&self.fetcher, &shcode, bar_type, &self.config.sdate, &self.config.edate).await?
                    }
                    BarKind::Minute(n) => {
                        collect_minute(
                            &self.fetcher,
                            &shcode,
                            n,
                            bar_type,
                            &self.config.sdate,
                            &self.config.edate,
                        )
                        .await?
                    }
                };
                match outcome {
                    // The collectors only ever return non-empty `Bars` (empty maps
                    // to a gap), but branch on is_empty defensively rather than
                    // relying on that with an unreachable arm.
                    TripleOutcome::Bars(bars) if !bars.is_empty() => {
                        let n = bars.len();
                        write_bars(&self.config.catalog_path, bars).await?;
                        bars_written += n;
                        ingested += 1;
                        checkpoint.mark_done(&id.to_string(), &label, &range);
                    }
                    TripleOutcome::Bars(_) => {
                        checkpoint.record_gap(&id.to_string(), &label, &range, GapReason::EmptyHistory);
                        gaps_this_run.push(last_gap(&checkpoint));
                    }
                    TripleOutcome::Gap(reason) => {
                        checkpoint.record_gap(&id.to_string(), &label, &range, reason);
                        gaps_this_run.push(last_gap(&checkpoint));
                    }
                }
                // Persist after every triple so a crash loses at most one triple.
                checkpoint.save(&checkpoint_path)?;
            }
        }

        let budget = BudgetEstimate {
            symbols: universe.len(),
            bar_kinds: self.config.bar_kinds.len(),
            per_sec_cap: self.fetcher.daily_pacer_cap(),
            min_requests: universe.len() * self.config.bar_kinds.len(),
        };
        tracing::info!(
            symbols = budget.symbols,
            bar_kinds = budget.bar_kinds,
            per_sec_cap = budget.per_sec_cap,
            min_requests = budget.min_requests,
            min_seconds = budget.min_seconds(),
            "ingest budget estimate"
        );

        Ok(CoverageReport {
            bars_written,
            triples_ingested: ingested,
            triples_skipped: skipped,
            gaps: gaps_this_run,
            heal_refusals: Vec::new(),
            budget,
        })
    }

    /// Accumulate-forward run under the R15 lock (refuses while a live session runs).
    /// The caller re-snapshots the universe first (`write_instruments`).
    pub async fn run_accumulate_locked(
        &mut self,
        universe: &[InstrumentId],
        last_closed: NaiveDate,
        lookback_floor: NaiveDate,
    ) -> AdapterResult<CoverageReport> {
        let _lock = AdvisoryLock::acquire(&self.config.catalog_path, LockKind::Ingest)?;
        self.run_accumulate(universe, last_closed, lookback_floor).await
    }

    /// Accumulate-forward: grow whole-universe coverage from each instrument's
    /// watermark to `last_closed`, reusing the proven per-triple fetch loop over a
    /// per-instrument range (U5, KTD7). The **watermark map is the sole skip
    /// authority**: a triple already current makes zero bar fetches (R6/AE4). An
    /// instrument with no watermark starts at `lookback_floor` (the initial bounded
    /// backfill, R8; a newly-listed symbol begins here too, R7/AE5). The watermark
    /// advances to `last_closed` even for a gap day, so an empty history is reported
    /// once and never retried forever (R10). Does not take the lock or re-snapshot
    /// the universe — use [`Self::run_accumulate_locked`] and pre-write instruments.
    pub async fn run_accumulate(
        &mut self,
        universe: &[InstrumentId],
        last_closed: NaiveDate,
        lookback_floor: NaiveDate,
    ) -> AdapterResult<CoverageReport> {
        std::fs::create_dir_all(&self.config.catalog_path).map_err(|e| {
            AdapterError::Ingest(format!("mkdir catalog {}: {e}", self.config.catalog_path.display()))
        })?;
        let checkpoint_path = self.config.checkpoint_path();
        let mut checkpoint = Checkpoint::load(&checkpoint_path)?;
        checkpoint.adjusted_prices = self.config.adjusted_prices;

        let mut bars_written = 0usize;
        let mut ingested = 0usize;
        let mut skipped = 0usize;
        let mut gaps_this_run: Vec<CoverageGap> = Vec::new();
        let mut heal_refusals: Vec<HealRefusal> = Vec::new();

        for id in universe {
            let shcode = id.symbol.as_str().to_string();
            let instrument = id.to_string();
            for &kind in &self.config.bar_kinds {
                let label = kind.label();
                let bar_type = kind.bar_type(*id)?;
                // The shifted mark outranks the watermark as authority (KTD-2): a
                // marked symbol heals regardless of watermark state, BEFORE the
                // already-current skip below.
                if matches!(kind, BarKind::Daily) && checkpoint.is_shifted(&instrument, &label) {
                    match self
                        .heal_daily(&mut checkpoint, &checkpoint_path, &shcode, &instrument, &label, bar_type, last_closed, lookback_floor)
                        .await?
                    {
                        HealOutcome::Healed(n) => {
                            bars_written += n;
                            ingested += 1;
                        }
                        HealOutcome::Refused(r) => heal_refusals.push(r),
                        HealOutcome::Incomplete => gaps_this_run.push(CoverageGap {
                            instrument: instrument.clone(),
                            bar_type: label.clone(),
                            range: format!("{}..{}", fmt_ymd(lookback_floor), fmt_ymd(last_closed)),
                            reason: GapReason::PaperThin,
                        }),
                    }
                    continue;
                }
                // Range = watermark+1 .. last closed session (or floor if unseen).
                let start = match checkpoint.watermark(&instrument, &label) {
                    Some(d) => d.succ_opt().expect("a date always has a successor"),
                    None => lookback_floor,
                };
                if start > last_closed {
                    // Already current — the sole skip authority makes this a no-op
                    // (no bar fetch), even though the universe re-snapshot still ran.
                    skipped += 1;
                    continue;
                }
                // Basis-shift detection (KTD-3): before appending new daily bars,
                // re-fetch the overlap window ending at the watermark and compare
                // against stored bars. No watermark (first-ever accumulate) or an
                // insufficient overlap skips detection entirely.
                if matches!(kind, BarKind::Daily) {
                    if let Some(wm) = checkpoint.watermark(&instrument, &label) {
                        if self.detect_shift(&shcode, bar_type, wm).await? {
                            // Save the mark atomically BEFORE any delete (KTD-2:
                            // mark-before-wipe is load-bearing — the reverse order
                            // plus a crash would leave a high watermark over an
                            // empty store and silently truncate history forever).
                            checkpoint.mark_shifted(&instrument, &label, last_closed);
                            checkpoint.save(&checkpoint_path)?;
                            tracing::warn!(instrument = %instrument, "adjustment-basis shift detected; healing");
                            match self
                                .heal_daily(&mut checkpoint, &checkpoint_path, &shcode, &instrument, &label, bar_type, last_closed, lookback_floor)
                                .await?
                            {
                                HealOutcome::Healed(n) => {
                                    bars_written += n;
                                    ingested += 1;
                                }
                                HealOutcome::Refused(r) => heal_refusals.push(r),
                                HealOutcome::Incomplete => gaps_this_run.push(CoverageGap {
                                    instrument: instrument.clone(),
                                    bar_type: label.clone(),
                                    range: format!("{}..{}", fmt_ymd(lookback_floor), fmt_ymd(last_closed)),
                                    reason: GapReason::PaperThin,
                                }),
                            }
                            continue;
                        }
                    }
                }
                let sdate = start.format("%Y%m%d").to_string();
                let edate = last_closed.format("%Y%m%d").to_string();
                let range = format!("{sdate}..{edate}");
                let outcome = match kind {
                    BarKind::Daily => {
                        collect_daily(&self.fetcher, &shcode, bar_type, &sdate, &edate).await?
                    }
                    BarKind::Minute(n) => {
                        collect_minute(&self.fetcher, &shcode, n, bar_type, &sdate, &edate).await?
                    }
                };
                // A PaperThin outcome means the fetch was TRUNCATED (page cap hit /
                // single-day chunk still overflowed) — the range is only partially
                // retrieved, so the watermark must NOT advance past it or the
                // un-fetched older history is skipped forever. Every other outcome
                // (bars, empty history, non-trading day) is complete-for-the-range
                // and advances (R10 — a gap day is covered-but-empty, never retried).
                let mut advance = true;
                match outcome {
                    TripleOutcome::Bars(bars) if !bars.is_empty() => {
                        let n = bars.len();
                        write_bars(&self.config.catalog_path, bars).await?;
                        bars_written += n;
                        ingested += 1;
                    }
                    TripleOutcome::Bars(_) => gaps_this_run.push(CoverageGap {
                        instrument: instrument.clone(),
                        bar_type: label.clone(),
                        range: range.clone(),
                        reason: GapReason::EmptyHistory,
                    }),
                    TripleOutcome::Gap(reason) => {
                        if reason == GapReason::PaperThin {
                            advance = false;
                        }
                        gaps_this_run.push(CoverageGap {
                            instrument: instrument.clone(),
                            bar_type: label.clone(),
                            range: range.clone(),
                            reason,
                        });
                    }
                }
                if advance {
                    checkpoint.set_watermark(&instrument, &label, last_closed);
                }
                // Persist after each triple for crash safety.
                checkpoint.save(&checkpoint_path)?;
            }
        }

        // Prune legacy completed/gap rows below the watermarks so daily runs stay
        // bounded (KTD7); the run's own gaps report comes from memory.
        checkpoint.prune_below_watermarks();
        checkpoint.save(&checkpoint_path)?;

        let budget = BudgetEstimate {
            symbols: universe.len(),
            bar_kinds: self.config.bar_kinds.len(),
            per_sec_cap: self.fetcher.daily_pacer_cap(),
            min_requests: universe.len() * self.config.bar_kinds.len(),
        };
        tracing::info!(
            symbols = budget.symbols,
            ingested,
            skipped,
            gaps = gaps_this_run.len(),
            "accumulate-forward run complete"
        );
        Ok(CoverageReport {
            bars_written,
            triples_ingested: ingested,
            triples_skipped: skipped,
            gaps: gaps_this_run,
            heal_refusals,
            budget,
        })
    }

    /// Detect a basis shift for one daily triple (KTD-3): fetch the overlap
    /// window ending at the watermark, read the stored side through the scoped
    /// read (never `read_all_bars`), exact-compare mutually-present dates.
    async fn detect_shift(
        &self,
        shcode: &str,
        bar_type: BarType,
        watermark: NaiveDate,
    ) -> AdapterResult<bool> {
        let wstart = overlap_window_start(watermark, self.config.overlap_days);
        // Stored side: the last `overlap_days` stored trading days in the window.
        let ws_ns = kst_to_unix_nanos(wstart, NaiveTime::MIN)?;
        let we_ns = kst_to_unix_nanos(watermark, KRX_REGULAR_CLOSE)?;
        let mut stored =
            read_bars_scoped(&self.config.catalog_path, bar_type, Some(ws_ns), Some(we_ns)).await?;
        if stored.len() > self.config.overlap_days {
            stored.drain(..stored.len() - self.config.overlap_days);
        }
        if stored.len() < MIN_OVERLAP_DATES {
            return Ok(false);
        }
        let fetched = match fetch_overlap(&self.fetcher, shcode, bar_type, wstart, watermark).await? {
            Some(bars) => bars,
            None => return Ok(false),
        };
        Ok(compare_overlap(&stored, &fetched) == OverlapVerdict::Shifted)
    }

    /// Heal one marked daily triple (KTD-2): one idempotent re-entrant sequence
    /// that always restarts at the wipe. The caller has already durably saved the
    /// shifted mark. Wipe precondition first: refuse (and stay marked) if the
    /// run's floor is later than the earliest stored bar — a heal must never
    /// silently shrink stored history.
    #[allow(clippy::too_many_arguments)]
    async fn heal_daily(
        &self,
        checkpoint: &mut Checkpoint,
        checkpoint_path: &Path,
        shcode: &str,
        instrument: &str,
        label: &str,
        bar_type: BarType,
        last_closed: NaiveDate,
        floor: NaiveDate,
    ) -> AdapterResult<HealOutcome> {
        // Wipe precondition (KTD-2). An already-wiped re-entry has no stored bars
        // and passes trivially (there is no history left to truncate).
        let stored = read_bars_scoped(&self.config.catalog_path, bar_type, None, None).await?;
        if let Some(earliest) = stored.first() {
            let earliest_date = kst_date_of(earliest.ts_event);
            if floor > earliest_date {
                tracing::warn!(
                    instrument,
                    floor = %fmt_ymd(floor),
                    earliest = %fmt_ymd(earliest_date),
                    "heal refused: run floor is later than the earliest stored bar; symbol stays marked"
                );
                return Ok(HealOutcome::Refused(HealRefusal {
                    instrument: instrument.to_string(),
                    bar_type: label.to_string(),
                    floor: fmt_ymd(floor),
                    earliest_stored: fmt_ymd(earliest_date),
                }));
            }
        }

        // Wipe: true-delete the series, then drop the watermark so the accumulate
        // arithmetic re-pulls from the floor. Persist the wiped state — the mark
        // (already saved) makes any crash from here converge back to this wipe.
        delete_bar_series(&self.config.catalog_path, bar_type).await?;
        checkpoint.clear_watermark(instrument, label);
        checkpoint.save(checkpoint_path)?;

        // Re-pull the full history from the floor. Completion keys on the fetch
        // cursor completing, never on bar count (KTD-3) — a shallow-history
        // symbol (listed after the floor) still clears its mark. A truncated
        // fetch (page cap) is NOT complete: keep the mark for the next run.
        let sdate = fmt_ymd(floor);
        let edate = fmt_ymd(last_closed);
        let pulled = match collect_daily(&self.fetcher, shcode, bar_type, &sdate, &edate).await? {
            TripleOutcome::Bars(bars) => bars,
            TripleOutcome::Gap(GapReason::PaperThin) => {
                tracing::warn!(instrument, "heal re-pull truncated; symbol stays marked");
                return Ok(HealOutcome::Incomplete);
            }
            // Cursor completed with zero bars — the server serves nothing for
            // this symbol anymore; the (empty) series is on a single basis.
            TripleOutcome::Gap(_) => Vec::new(),
        };
        if !pulled.is_empty() {
            write_bars(&self.config.catalog_path, pulled.clone()).await?;
        }

        // Re-verify (the gateway may rewrite the series again while the heal is
        // in flight): one more overlap fetch against the just-pulled tail. Only a
        // positive mismatch keeps the mark — an insufficient overlap (shallow
        // history) must not pin a symbol shifted forever.
        if !pulled.is_empty() {
            let wstart = overlap_window_start(last_closed, self.config.overlap_days);
            let ws_ns = kst_to_unix_nanos(wstart, NaiveTime::MIN)?.as_u64();
            let mut tail: Vec<Bar> = pulled
                .iter()
                .filter(|b| b.ts_event.as_u64() >= ws_ns)
                .cloned()
                .collect();
            if tail.len() > self.config.overlap_days {
                tail.drain(..tail.len() - self.config.overlap_days);
            }
            if let Some(fetched) =
                fetch_overlap(&self.fetcher, shcode, bar_type, wstart, last_closed).await?
            {
                if compare_overlap(&tail, &fetched) == OverlapVerdict::Shifted {
                    tracing::warn!(instrument, "heal re-verify mismatched; symbol stays marked");
                    return Ok(HealOutcome::Incomplete);
                }
            }
        }

        // Completion: clear the mark, record the re-base event, and set the
        // watermark in one save (KTD-2).
        let detected = checkpoint
            .shifted_detected(instrument, label)
            .unwrap_or(&fmt_ymd(last_closed))
            .to_string();
        checkpoint.clear_shifted(instrument, label);
        checkpoint.record_rebase_event(RebaseEvent {
            instrument: instrument.to_string(),
            bar_type: label.to_string(),
            detected,
            healed: fmt_ymd(last_closed),
        });
        checkpoint.set_watermark(instrument, label, last_closed);
        checkpoint.save(checkpoint_path)?;
        tracing::info!(instrument, bars = pulled.len(), "basis-shift heal complete");
        Ok(HealOutcome::Healed(pulled.len()))
    }
}

/// The KST calendar date of a bar timestamp (inverse of [`kst_to_unix_nanos`]'s
/// date component).
fn kst_date_of(ts: UnixNanos) -> NaiveDate {
    let kst = FixedOffset::east_opt(KST_UTC_OFFSET_HOURS * 3600).expect("KST offset is valid");
    chrono::DateTime::<chrono::Utc>::from_timestamp_nanos(ts.as_u64() as i64)
        .with_timezone(&kst)
        .date_naive()
}

impl SdkFetcher {
    fn daily_pacer_cap(&self) -> u32 {
        // 1s / interval, rounded.
        let secs = self.daily_pacer.min_interval().as_secs_f64();
        if secs <= 0.0 {
            0
        } else {
            (1.0 / secs).round() as u32
        }
    }
}

fn last_gap(cp: &Checkpoint) -> checkpoint::CoverageGap {
    cp.gaps().last().cloned().expect("a gap was just recorded")
}

/// Write bars to the catalog on a blocking thread.
///
/// `ParquetDataCatalog` drives an internal runtime via `block_on`, which panics if
/// called on a thread already running a tokio reactor — so every catalog
/// interaction is moved to the blocking pool (`spawn_blocking`). The catalog is
/// constructed, used, and dropped entirely inside the closure. Ascending `ts_init`
/// is guaranteed by the callers; the disjoint check is skipped (re-ingesting a
/// symbol overwrites its range; dedup is by checkpoint).
///
/// Public so the lab (and tooling) can stage a fixture catalog symmetrically with
/// [`read_all_bars`] / [`write_instruments`]. **This is a low-level primitive that
/// bypasses the ingest checkpoint** — it advances no watermark and records no coverage,
/// so production coverage growth must go through [`Ingestor`] (which owns the
/// checkpoint), never this. Reserve direct use for test fixtures / one-off staging.
pub async fn write_bars(catalog_path: &Path, bars: Vec<Bar>) -> AdapterResult<()> {
    let path = catalog_path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&path)
            .map_err(|e| AdapterError::Ingest(format!("mkdir catalog {}: {e}", path.display())))?;
        let catalog = ParquetDataCatalog::new(&path, None, None, None, None);
        catalog
            .write_to_parquet(&bars, None, None, Some(true))
            .map(|_| ())
            .map_err(|e| AdapterError::Ingest(format!("catalog write: {e}")))
    })
    .await
    .map_err(|e| AdapterError::Ingest(format!("catalog write task panicked: {e}")))?
}

/// Write instrument definitions to the catalog (so a backtest can load them). Runs
/// on the blocking pool for the same reason as [`write_bars`].
pub async fn write_instruments(
    catalog_path: &Path,
    instruments: Vec<nautilus_model::instruments::InstrumentAny>,
) -> AdapterResult<()> {
    let path = catalog_path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&path)
            .map_err(|e| AdapterError::Ingest(format!("mkdir catalog {}: {e}", path.display())))?;
        let catalog = ParquetDataCatalog::new(&path, None, None, None, None);
        catalog
            .write_instruments(instruments)
            .map(|_| ())
            .map_err(|e| AdapterError::Ingest(format!("catalog write_instruments: {e}")))
    })
    .await
    .map_err(|e| AdapterError::Ingest(format!("catalog write_instruments task panicked: {e}")))?
}

/// Read all bars back from the catalog on a blocking thread (round-trip helper for
/// tests + the backtest loader).
pub async fn read_all_bars(catalog_path: &Path) -> AdapterResult<Vec<Bar>> {
    let path = catalog_path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut catalog = ParquetDataCatalog::new(&path, None, None, None, None);
        catalog
            .bars(None, None, None)
            .map_err(|e| AdapterError::Ingest(format!("catalog read: {e}")))
    })
    .await
    .map_err(|e| AdapterError::Ingest(format!("catalog read task panicked: {e}")))?
}

/// True-delete one bar-type series (e.g. one symbol's daily bars) from the
/// catalog, on a blocking thread (same `spawn_blocking` rationale as
/// [`write_bars`]). The heal's wipe step (KTD-2): overwrite-and-tolerate is not
/// enough — `write_to_parquet` with the disjoint check skipped leaves stale
/// old-basis files readable wherever date ranges don't exactly coincide, so the
/// wipe must remove the files. Deleting a series with no stored bars is a no-op
/// `Ok`. Scoped to ONE bar type: a daily wipe never touches minute bars (KTD-8).
pub async fn delete_bar_series(catalog_path: &Path, bar_type: BarType) -> AdapterResult<()> {
    let path = catalog_path.to_path_buf();
    let identifier = bar_type.to_string();
    tokio::task::spawn_blocking(move || {
        let mut catalog = ParquetDataCatalog::new(&path, None, None, None, None);
        catalog
            .delete_data_range("bars", Some(&identifier), None, None)
            .map_err(|e| AdapterError::Ingest(format!("catalog delete {identifier}: {e}")))
    })
    .await
    .map_err(|e| AdapterError::Ingest(format!("catalog delete task panicked: {e}")))?
}

/// Read one bar-type series over a bounded `[start, end]` window, on a blocking
/// thread. This is the per-triple read primitive: [`read_all_bars`] loads the
/// entire catalog and must not be used per symbol (an accumulate run would
/// re-read the full multi-year catalog once per symbol — a cost small offline
/// fixtures never expose).
pub async fn read_bars_scoped(
    catalog_path: &Path,
    bar_type: BarType,
    start: Option<UnixNanos>,
    end: Option<UnixNanos>,
) -> AdapterResult<Vec<Bar>> {
    let path = catalog_path.to_path_buf();
    let identifier = bar_type.to_string();
    tokio::task::spawn_blocking(move || {
        let mut catalog = ParquetDataCatalog::new(&path, None, None, None, None);
        catalog
            .bars(Some(vec![identifier.clone()]), start, end)
            .map_err(|e| AdapterError::Ingest(format!("catalog scoped read {identifier}: {e}")))
    })
    .await
    .map_err(|e| AdapterError::Ingest(format!("catalog scoped read task panicked: {e}")))?
}

/// Read instrument definitions back from the catalog on a blocking thread (the
/// backtest loader, which loads instruments + bars from the catalog per F1).
pub async fn read_all_instruments(
    catalog_path: &Path,
) -> AdapterResult<Vec<nautilus_model::instruments::InstrumentAny>> {
    let path = catalog_path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let catalog = ParquetDataCatalog::new(&path, None, None, None, None);
        catalog
            .instruments(None, None, None)
            .map_err(|e| AdapterError::Ingest(format!("catalog read instruments: {e}")))
    })
    .await
    .map_err(|e| AdapterError::Ingest(format!("catalog read instruments task panicked: {e}")))?
}

// ---------------------------------------------------------------------------
// Max-lookback probe (U7, KTD10, R10) — the staged operator mode that locates the
// earliest served minute date for a pilot symbol and records it so the backfill can
// be sized. Both the probe and the backfill are operator-gated.
// ---------------------------------------------------------------------------

/// The recorded result of the minute-lookback probe (KTD10), persisted to
/// `<data>/probes/minute-lookback.json`. The backfill derives `LS_INGEST_LOOKBACK`
/// from either form — the explicit `earliest_date` or the rolling `depth_days`
/// (which keeps a lookback honest when the probe and the backfill run days apart).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinuteLookback {
    /// The earliest minute date the server served for the pilot (`YYYYMMDD`).
    pub earliest_date: String,
    /// Calendar-day depth from the earliest served date to the probe anchor.
    pub depth_days: i64,
    /// When the probe ran (RFC3339 — a stale probe should be re-run).
    pub probed_at: String,
}

fn fmt_ymd(d: NaiveDate) -> String {
    d.format("%Y%m%d").to_string()
}

/// The probes directory beside the catalog (`<data>/probes/`, KTD2).
pub fn probes_dir_for(catalog_path: &Path) -> PathBuf {
    catalog_path
        .parent()
        .map(|p| p.join("probes"))
        .unwrap_or_else(|| catalog_path.join("probes"))
}

/// Search backward in ≥7-calendar-day windows for the earliest served minute date on
/// `pilot` (KTD10). Each window spans at least a week so it always contains trading
/// days; only an **all-empty** window reads as beyond-lookback (a single-date probe
/// would converge wrongly on KRX weekends/holidays). Returns the earliest served date,
/// or `None` if the pilot serves nothing at all.
///
/// # Errors
///
/// Propagates a fetcher error (e.g. a pagination-limit on an over-wide window).
pub async fn probe_minute_lookback<F: MinuteFetcher>(
    fetcher: &F,
    pilot: &str,
    ncnt: u32,
    anchor: NaiveDate,
    window_days: i64,
    max_windows: usize,
) -> AdapterResult<Option<NaiveDate>> {
    let window_days = window_days.max(7);
    let mut earliest: Option<NaiveDate> = None;
    let mut wend = anchor;
    for _ in 0..max_windows.max(1) {
        let wstart = wend - ChronoDuration::days(window_days - 1);
        let resp = fetcher
            .fetch_minute_chunk(pilot, ncnt, &fmt_ymd(wstart), &fmt_ymd(wend))
            .await?;
        let mut wmin: Option<NaiveDate> = None;
        for row in resp.iter().flat_map(|r| &r.outblock1) {
            if let Ok(d) = NaiveDate::parse_from_str(row.date.trim(), "%Y%m%d") {
                wmin = Some(wmin.map_or(d, |m| m.min(d)));
            }
        }
        match wmin {
            // A non-empty window: extend the earliest and step the window back.
            Some(m) => {
                earliest = Some(earliest.map_or(m, |e| e.min(m)));
                wend = wstart - ChronoDuration::days(1);
            }
            // An all-empty window is beyond lookback — stop.
            None => break,
        }
    }
    Ok(earliest)
}

/// Persist a [`MinuteLookback`] to `<probes_dir>/minute-lookback.json` atomically
/// (temp file + rename, mirroring the checkpoint save).
pub fn write_minute_lookback(probes_dir: &Path, lb: &MinuteLookback) -> AdapterResult<()> {
    std::fs::create_dir_all(probes_dir)
        .map_err(|e| AdapterError::Ingest(format!("mkdir {}: {e}", probes_dir.display())))?;
    let json = serde_json::to_string_pretty(lb)
        .map_err(|e| AdapterError::Ingest(format!("serialize probe: {e}")))?;
    let path = probes_dir.join("minute-lookback.json");
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)
        .map_err(|e| AdapterError::Ingest(format!("write probe tmp {}: {e}", tmp.display())))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| AdapterError::Ingest(format!("commit probe {}: {e}", path.display())))
}

/// Read the recorded [`MinuteLookback`] from `<probes_dir>/minute-lookback.json`.
///
/// # Errors
///
/// [`AdapterError::Ingest`] if the file is missing or unparseable.
pub fn read_minute_lookback(probes_dir: &Path) -> AdapterResult<MinuteLookback> {
    let path = probes_dir.join("minute-lookback.json");
    let s = std::fs::read_to_string(&path)
        .map_err(|e| AdapterError::Ingest(format!("read probe {}: {e}", path.display())))?;
    serde_json::from_str(&s)
        .map_err(|e| AdapterError::Ingest(format!("corrupt probe {}: {e}", path.display())))
}

impl Ingestor {
    /// Run the staged max-lookback probe for a pilot symbol (KTD10): locate the
    /// earliest served minute date via a windowed backward search and, on success,
    /// write `<data>/probes/minute-lookback.json`. Returns the recorded result, or
    /// `None` when the pilot serves nothing (nothing is written).
    pub async fn run_probe_lookback(
        &self,
        pilot: &str,
        ncnt: u32,
        anchor: NaiveDate,
        probed_at: String,
    ) -> AdapterResult<Option<MinuteLookback>> {
        let earliest = probe_minute_lookback(&self.fetcher, pilot, ncnt, anchor, 7, 400).await?;
        match earliest {
            Some(d) => {
                let lb = MinuteLookback {
                    earliest_date: fmt_ymd(d),
                    depth_days: anchor.signed_duration_since(d).num_days(),
                    probed_at,
                };
                write_minute_lookback(&probes_dir_for(&self.config.catalog_path), &lb)?;
                Ok(Some(lb))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kst_converts_with_date_rollover_at_midnight() {
        // 00:30 KST on 2024-01-05 = 15:30 UTC on 2024-01-04 (rolls back a day).
        let date = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        let time = NaiveTime::from_hms_opt(0, 30, 0).unwrap();
        let ns = kst_to_unix_nanos(date, time).unwrap();
        // Expected UTC: 2024-01-04 15:30:00.
        let expect = FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(2024, 1, 4, 15, 30, 0)
            .single()
            .unwrap()
            .timestamp_nanos_opt()
            .unwrap() as u64;
        assert_eq!(ns.as_u64(), expect);
    }

    #[test]
    fn daily_bar_close_is_1530_kst() {
        // 2024-01-05 daily close 15:30 KST = 06:30 UTC.
        let bar_type = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let row = T8410OutBlock1 {
            date: "20240105".to_string(),
            open: "60000".to_string(),
            high: "61000".to_string(),
            low: "59000".to_string(),
            close: "60500".to_string(),
            jdiff_vol: "1000000".to_string(),
            ..Default::default()
        };
        let bar = build_daily_bar(bar_type, &row).unwrap().unwrap();
        let expect = FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(2024, 1, 5, 6, 30, 0)
            .single()
            .unwrap()
            .timestamp_nanos_opt()
            .unwrap() as u64;
        assert_eq!(bar.ts_event.as_u64(), expect);
        assert_eq!(bar.close, Price::from("60500"));
    }

    #[test]
    fn last_closed_session_respects_the_post_close_buffer() {
        let day = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        // 18:00 KST (past the 16:30 buffer) → today is the last closed session.
        let evening = NaiveDateTime::new(day, NaiveTime::from_hms_opt(18, 0, 0).unwrap());
        assert_eq!(last_closed_session(evening, ACCUMULATE_CLOSE_BUFFER), day);
        // 10:00 KST (mid-session, before the buffer) → yesterday, never today (the
        // watermark must not advance into an in-session day, KTD7).
        let morning = NaiveDateTime::new(day, NaiveTime::from_hms_opt(10, 0, 0).unwrap());
        assert_eq!(
            last_closed_session(morning, ACCUMULATE_CLOSE_BUFFER),
            NaiveDate::from_ymd_opt(2024, 1, 4).unwrap()
        );
    }

    #[test]
    fn split_range_narrows_and_bottoms_out() {
        let s = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let e = NaiveDate::from_ymd_opt(2024, 1, 10).unwrap();
        let (left, right) = split_range(s, e).unwrap();
        assert!(left.1 < right.0); // disjoint halves
        assert_eq!(left.0, s);
        assert_eq!(right.1, e);
        // A single day cannot be narrowed.
        assert!(split_range(s, s).is_none());
    }

    // --- fetcher-loop fakes: cursor termination + PaginationLimit narrowing ---

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    fn daily_row(date: &str) -> T8410OutBlock1 {
        T8410OutBlock1 {
            date: date.to_string(),
            open: "100".to_string(),
            high: "110".to_string(),
            low: "90".to_string(),
            close: "105".to_string(),
            jdiff_vol: "1000".to_string(),
            ..Default::default()
        }
    }

    fn daily_resp(next_cursor: &str, row_date: &str) -> T8410Response {
        let mut resp = T8410Response {
            rsp_cd: "00000".to_string(),
            outblock1: vec![daily_row(row_date)],
            ..Default::default()
        };
        resp.outblock.cts_date = next_cursor.to_string();
        resp
    }

    struct FixedDaily {
        resp: T8410Response,
        calls: AtomicUsize,
    }
    #[async_trait]
    impl DailyFetcher for FixedDaily {
        async fn fetch_daily_page(&self, _shcode: &str, _sd: &str, _ed: &str, _cts: &str) -> AdapterResult<T8410Response> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.resp.clone())
        }
    }

    struct ErrDaily {
        code: String,
    }
    #[async_trait]
    impl DailyFetcher for ErrDaily {
        async fn fetch_daily_page(&self, _shcode: &str, _sd: &str, _ed: &str, _cts: &str) -> AdapterResult<T8410Response> {
            Err(AdapterError::Sdk(LsError::ApiError {
                code: self.code.clone(),
                message: "non-trading day".to_string(),
            }))
        }
    }

    #[tokio::test]
    async fn daily_bars_are_sorted_ascending_even_when_pages_are_newest_first() {
        // LS daily charts return newest-first; a single page here carries rows in
        // DESCENDING date order. The collector must sort ascending for the catalog.
        let bar_type = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let mut resp = T8410Response {
            rsp_cd: "00000".to_string(),
            outblock1: vec![daily_row("20240105"), daily_row("20240104"), daily_row("20240103")],
            ..Default::default()
        };
        resp.outblock.cts_date = String::new(); // single page
        let fetcher = FixedDaily { resp, calls: AtomicUsize::new(0) };
        let outcome = collect_daily(&fetcher, "005930", bar_type, "20240101", "20240131").await.unwrap();
        let bars = match outcome {
            TripleOutcome::Bars(b) => b,
            _ => panic!("expected bars"),
        };
        assert_eq!(bars.len(), 3);
        for w in bars.windows(2) {
            assert!(
                w[0].ts_init.as_u64() <= w[1].ts_init.as_u64(),
                "daily bars must be ascending for the catalog"
            );
        }
    }

    #[tokio::test]
    async fn daily_cursor_terminates_on_empty_next_cursor() {
        let bar_type = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let fetcher = FixedDaily {
            resp: daily_resp("", "20240105"),
            calls: AtomicUsize::new(0),
        };
        let outcome = collect_daily(&fetcher, "005930", bar_type, "20240101", "20240131").await.unwrap();
        assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1, "empty cursor stops after one page");
        assert!(matches!(outcome, TripleOutcome::Bars(ref b) if b.len() == 1));
    }

    #[tokio::test]
    async fn daily_cursor_defensive_stop_on_repeated_cursor() {
        let bar_type = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        // A gateway that echoes the same non-empty cursor forever must not loop.
        let fetcher = FixedDaily {
            resp: daily_resp("SAME", "20240105"),
            calls: AtomicUsize::new(0),
        };
        let outcome = collect_daily(&fetcher, "005930", bar_type, "20240101", "20240131").await.unwrap();
        assert_eq!(
            fetcher.calls.load(Ordering::SeqCst),
            2,
            "repeated cursor stops after the repeat is detected"
        );
        assert!(matches!(outcome, TripleOutcome::Bars(_)));
    }

    #[tokio::test]
    async fn daily_01715_becomes_non_trading_day_gap() {
        let bar_type = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let fetcher = ErrDaily { code: "01715".to_string() };
        let outcome = collect_daily(&fetcher, "005930", bar_type, "20240101", "20240131").await.unwrap();
        assert!(matches!(outcome, TripleOutcome::Gap(GapReason::NonTradingDay)));
    }

    fn minute_row(date: &str, time: &str) -> T8412OutBlock1 {
        T8412OutBlock1 {
            date: date.to_string(),
            time: time.to_string(),
            open: "100".to_string(),
            high: "110".to_string(),
            low: "90".to_string(),
            close: "105".to_string(),
            jdiff_vol: "10".to_string(),
            ..Default::default()
        }
    }

    fn minute_page(date: &str) -> T8412Response {
        T8412Response {
            rsp_cd: "00000".to_string(),
            outblock1: vec![minute_row(date, "0900")],
            ..Default::default()
        }
    }

    /// A fetcher that overflows (PaginationLimit) for any chunk spanning >2 days,
    /// and returns one row per narrow chunk otherwise. Records every requested
    /// range so the test can assert narrowing happened.
    struct NarrowingMinute {
        ranges: Mutex<Vec<(String, String)>>,
    }
    #[async_trait]
    impl MinuteFetcher for NarrowingMinute {
        async fn fetch_minute_chunk(
            &self,
            _shcode: &str,
            _ncnt: u32,
            sdate: &str,
            edate: &str,
        ) -> AdapterResult<Vec<T8412Response>> {
            self.ranges.lock().unwrap().push((sdate.to_string(), edate.to_string()));
            let s = NaiveDate::parse_from_str(sdate, "%Y%m%d").unwrap();
            let e = NaiveDate::parse_from_str(edate, "%Y%m%d").unwrap();
            if (e - s).num_days() > 2 {
                Err(AdapterError::Sdk(LsError::PaginationLimit(10)))
            } else {
                Ok(vec![minute_page(sdate)])
            }
        }
    }

    #[tokio::test]
    async fn minute_pagination_limit_narrows_and_ingests_all() {
        let bar_type = BarKind::Minute(1).bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let fetcher = NarrowingMinute { ranges: Mutex::new(Vec::new()) };
        let outcome = collect_minute(&fetcher, "005930", 1, bar_type, "20240101", "20240110")
            .await
            .unwrap();
        // Narrowing must have bottomed out into ≤2-day chunks that each returned a row.
        let bars = match outcome {
            TripleOutcome::Bars(b) => b,
            other => panic!("expected bars, got a gap: {:?}", std::mem::discriminant(&other)),
        };
        assert!(!bars.is_empty(), "narrowing should ingest rows");
        // ts_event ascending after the sort.
        for w in bars.windows(2) {
            assert!(w[0].ts_init.as_u64() <= w[1].ts_init.as_u64());
        }
        // The widest range was retried narrower (more than one distinct request).
        assert!(fetcher.ranges.lock().unwrap().len() > 1);
    }

    struct EmptyMinute;
    #[async_trait]
    impl MinuteFetcher for EmptyMinute {
        async fn fetch_minute_chunk(
            &self,
            _s: &str,
            _n: u32,
            _sd: &str,
            _ed: &str,
        ) -> AdapterResult<Vec<T8412Response>> {
            Ok(vec![]) // empty history
        }
    }

    // --- basis-shift overlap compare (KTD-3) — pure compare semantics ---

    fn ohlc_bar(date: &str, close: i64) -> Bar {
        let bar_type = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let row = T8410OutBlock1 {
            date: date.to_string(),
            open: (close - 5).to_string(),
            high: (close + 10).to_string(),
            low: (close - 10).to_string(),
            close: close.to_string(),
            jdiff_vol: "1000".to_string(),
            ..Default::default()
        };
        build_daily_bar(bar_type, &row).unwrap().unwrap()
    }

    #[test]
    fn overlap_matches_when_mutual_dates_agree() {
        let stored = vec![ohlc_bar("20240103", 100), ohlc_bar("20240104", 110), ohlc_bar("20240105", 120)];
        assert_eq!(compare_overlap(&stored, &stored.clone()), OverlapVerdict::Match);
    }

    #[test]
    fn overlap_shifts_on_any_mutual_date_mismatch() {
        let stored = vec![ohlc_bar("20240103", 100), ohlc_bar("20240104", 110), ohlc_bar("20240105", 120)];
        // A post-split basis rewrites the whole series; one differing close is enough.
        let fetched = vec![ohlc_bar("20240103", 100), ohlc_bar("20240104", 110), ohlc_bar("20240105", 60)];
        assert_eq!(compare_overlap(&stored, &fetched), OverlapVerdict::Shifted);
    }

    #[test]
    fn overlap_excludes_one_sided_dates() {
        // Stored has a bar the server dropped; the server gap-filled a day with no
        // stored bar. Neither is a basis shift (KTD-3) — mutual dates agree.
        let stored = vec![
            ohlc_bar("20240102", 90),
            ohlc_bar("20240103", 100),
            ohlc_bar("20240105", 120),
            ohlc_bar("20240108", 130),
        ];
        let fetched = vec![
            ohlc_bar("20240102", 90),
            ohlc_bar("20240103", 100),
            ohlc_bar("20240104", 999), // server-side gap-fill, no stored counterpart
            ohlc_bar("20240105", 120),
            ohlc_bar("20240108", 130),
        ];
        assert_eq!(compare_overlap(&stored, &fetched), OverlapVerdict::Match);
    }

    #[test]
    fn overlap_insufficient_below_minimum_mutual_dates() {
        // Two mutual dates (< MIN_OVERLAP_DATES) — even a disagreement must skip
        // detection rather than mark.
        let stored = vec![ohlc_bar("20240104", 110), ohlc_bar("20240105", 120)];
        let fetched = vec![ohlc_bar("20240104", 55), ohlc_bar("20240105", 60)];
        assert_eq!(compare_overlap(&stored, &fetched), OverlapVerdict::Insufficient);
        // Disjoint dates: zero mutual.
        let other = vec![ohlc_bar("20240110", 1), ohlc_bar("20240111", 2), ohlc_bar("20240112", 3)];
        assert_eq!(compare_overlap(&stored, &other), OverlapVerdict::Insufficient);
    }

    #[test]
    fn overlap_window_start_spans_holiday_clusters() {
        let wm = NaiveDate::from_ymd_opt(2024, 10, 2).unwrap();
        let start = overlap_window_start(wm, DEFAULT_OVERLAP_DAYS);
        // 5 trading days ending at the watermark must fit even across a Chuseok
        // cluster + weekends (25 calendar days for the default knob).
        assert_eq!((wm - start).num_days(), 25);
    }

    #[tokio::test]
    async fn minute_empty_history_is_a_gap_not_a_failure() {
        let bar_type = BarKind::Minute(1).bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let outcome = collect_minute(&EmptyMinute, "005930", 1, bar_type, "20240101", "20240105")
            .await
            .unwrap();
        assert!(matches!(outcome, TripleOutcome::Gap(GapReason::EmptyHistory)));
    }
}
