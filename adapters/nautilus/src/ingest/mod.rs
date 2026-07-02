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

use std::collections::{HashSet, VecDeque};
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

use crate::error::{AdapterError, AdapterResult};
use crate::lock::{AdvisoryLock, LockKind};
use crate::rules::{KRX_REGULAR_CLOSE, KST_UTC_OFFSET_HOURS};
use self::checkpoint::{Checkpoint, GapReason};
use self::pacer::{Pacer, MARKET_DATA_CATEGORY_PER_SEC};

/// A defensive upper bound on daily-cursor pages per symbol (guards a gateway that
/// never terminates the cursor).
const MAX_DAILY_PAGES: usize = 500;

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
    let v = s.trim();
    let i = if v.is_empty() {
        0
    } else if let Ok(i) = v.parse::<i64>() {
        i
    } else if let Ok(f) = v.parse::<f64>() {
        f.trunc() as i64
    } else {
        return Err(AdapterError::FieldParse {
            field: field.to_string(),
            value: s.to_string(),
            reason: "expected integer KRW".to_string(),
        });
    };
    Ok(Price::from(i.max(0).to_string().as_str()))
}

fn qty_from_str(field: &str, s: &str) -> AdapterResult<Quantity> {
    let v = s.trim();
    let i = if v.is_empty() {
        0
    } else if let Ok(i) = v.parse::<i64>() {
        i
    } else if let Ok(f) = v.parse::<f64>() {
        f.trunc() as i64
    } else {
        return Err(AdapterError::FieldParse {
            field: field.to_string(),
            value: s.to_string(),
            reason: "expected integer volume".to_string(),
        });
    };
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

/// Fetches one daily-chart page for a symbol at a body cursor.
#[async_trait]
pub trait DailyFetcher {
    /// Fetch the t8410 page at `cts_date` (`""` = first page).
    async fn fetch_daily_page(&self, shcode: &str, cts_date: &str) -> AdapterResult<T8410Response>;
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

/// Production fetcher over the SDK, paced per-TR (KTD4).
pub struct SdkFetcher {
    sdk: LsSdk,
    daily_pacer: Pacer,
    minute_pacer: Pacer,
    daily_qrycnt: usize,
    minute_qrycnt: usize,
    sdate: String,
    edate: String,
}

impl SdkFetcher {
    fn new(sdk: LsSdk, sdate: String, edate: String) -> Self {
        SdkFetcher {
            sdk,
            daily_pacer: Pacer::for_policy(&T8410_POLICY, MARKET_DATA_CATEGORY_PER_SEC),
            minute_pacer: Pacer::for_policy(&T8412_POLICY, MARKET_DATA_CATEGORY_PER_SEC),
            daily_qrycnt: 900,
            minute_qrycnt: 900,
            sdate,
            edate,
        }
    }
}

#[async_trait]
impl DailyFetcher for SdkFetcher {
    async fn fetch_daily_page(&self, shcode: &str, cts_date: &str) -> AdapterResult<T8410Response> {
        self.daily_pacer.acquire().await;
        let mut req = T8410Request::new(
            shcode,
            "2", // daily
            self.daily_qrycnt.to_string(),
            self.sdate.clone(),
            self.edate.clone(),
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
) -> AdapterResult<TripleOutcome> {
    let mut bars = Vec::new();
    let mut cts_date = String::new();
    let mut seen = HashSet::new();
    let mut hit_cap = true;

    for _ in 0..MAX_DAILY_PAGES {
        let resp = match fetcher.fetch_daily_page(shcode, &cts_date).await {
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
        let fetcher = SdkFetcher::new(sdk, config.sdate.clone(), config.edate.clone());
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
                    BarKind::Daily => collect_daily(&self.fetcher, &shcode, bar_type).await?,
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
            budget,
        })
    }
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
async fn write_bars(catalog_path: &Path, bars: Vec<Bar>) -> AdapterResult<()> {
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
        async fn fetch_daily_page(&self, _shcode: &str, _cts: &str) -> AdapterResult<T8410Response> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.resp.clone())
        }
    }

    struct ErrDaily {
        code: String,
    }
    #[async_trait]
    impl DailyFetcher for ErrDaily {
        async fn fetch_daily_page(&self, _shcode: &str, _cts: &str) -> AdapterResult<T8410Response> {
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
        let outcome = collect_daily(&fetcher, "005930", bar_type).await.unwrap();
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
        let outcome = collect_daily(&fetcher, "005930", bar_type).await.unwrap();
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
        let outcome = collect_daily(&fetcher, "005930", bar_type).await.unwrap();
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
        let outcome = collect_daily(&fetcher, "005930", bar_type).await.unwrap();
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

    #[tokio::test]
    async fn minute_empty_history_is_a_gap_not_a_failure() {
        let bar_type = BarKind::Minute(1).bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let outcome = collect_minute(&EmptyMinute, "005930", 1, bar_type, "20240101", "20240105")
            .await
            .unwrap();
        assert!(matches!(outcome, TripleOutcome::Gap(GapReason::EmptyHistory)));
    }
}
