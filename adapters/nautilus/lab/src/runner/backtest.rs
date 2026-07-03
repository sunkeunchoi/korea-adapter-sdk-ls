//! Backtest runner (U5, F1) — one command runs ORB vN over the catalog and lands a
//! registry run. Loads instruments + bars for the pinned range from the
//! `ParquetDataCatalog`, scans the universe from prior-session daily bars, mounts ORB
//! in a `BacktestEngine`, and wires the resulting fills into the RunWriter.
//!
//! Guards (KTD2/KTD8): the runner refuses to start while the ingest advisory lock is
//! held (and holds it for the run so ingest cannot mutate the catalog mid-run), and
//! re-reads the range-scoped catalog fingerprint at finalize — failing the run with no
//! registry residue if it changed since start.

use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use nautilus_backtest::config::{BacktestEngineConfig, SimulatedVenueConfig};
use nautilus_backtest::engine::BacktestEngine;
use nautilus_ls::ingest::checkpoint::Checkpoint;
use nautilus_ls::ingest::{kst_to_unix_nanos, read_all_bars, read_all_instruments, BarKind};
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_model::data::{Bar, Data};
use nautilus_model::enums::{AccountType, BarAggregation, BookType, OmsType};
use nautilus_model::identifiers::Venue;
use nautilus_model::instruments::{Instrument, InstrumentAny};
use nautilus_model::position::Position;
use nautilus_model::types::{Currency, Money};

use crate::artifacts::data_quality::{CoverageGapRecord, DataQualityReport, GapReasonKind};
use crate::artifacts::manifest::{hash_bytes, range_fingerprint, universe_hash, DataRange, Manifest};
use crate::artifacts::performance::PerformanceReport;
use crate::artifacts::{run_id, RunSource, RunWriter};
use crate::agent::sink::DecisionSink;
use crate::params::OrbParams;
use crate::strategy::orb::{
    select_universe, OrbStrategy, SelectedSymbol, UniverseCandidate,
};

/// Backtest run configuration.
#[derive(Debug, Clone)]
pub struct BacktestConfig {
    /// The data home (`<data>/catalog`, `<data>/runs`, …).
    pub data_home: PathBuf,
    /// The explicit pinned bar-data range (KTD8).
    pub range: DataRange,
    /// The ORB parameter set (defaults to KTD6).
    pub params: OrbParams,
    /// Starting account balance (KRW).
    pub starting_balance: f64,
    /// The minute-bar step the strategy trades (default 1).
    pub minute_step: u32,
}

impl BacktestConfig {
    /// A config with KTD6 defaults over `data_home` for `[start, end]` (YYYYMMDD).
    pub fn new(data_home: impl Into<PathBuf>, start: &str, end: &str) -> Self {
        BacktestConfig {
            data_home: data_home.into(),
            range: DataRange { start: start.to_string(), end: end.to_string() },
            params: OrbParams::default(),
            starting_balance: 100_000_000.0,
            minute_step: 1,
        }
    }
}

/// The outcome of a finalized backtest run.
#[derive(Debug, Clone)]
pub struct RunOutcome {
    /// The finalized run directory.
    pub run_dir: PathBuf,
    /// The run id.
    pub run_id: String,
}

/// Run a backtest to a finalized registry run.
pub async fn run(cfg: BacktestConfig, start: DateTime<Utc>) -> anyhow::Result<RunOutcome> {
    run_inner(cfg, start, std::future::ready(())).await
}

/// Run a backtest with a test hook awaited between the engine run and the finalize
/// fingerprint re-check — used to simulate a mid-run catalog mutation (KTD8). The
/// public [`run`] passes a no-op.
pub async fn run_inner<F: std::future::Future<Output = ()>>(
    cfg: BacktestConfig,
    start: DateTime<Utc>,
    before_finalize: F,
) -> anyhow::Result<RunOutcome> {
    let catalog_path = cfg.data_home.join("catalog");
    if !catalog_path.exists() {
        anyhow::bail!("no catalog at {} — ingest first", catalog_path.display());
    }

    // Own-guard: refuse if ingest is running, and hold the ingest lock so the catalog
    // cannot be mutated mid-run (KTD2). Released on drop / at end of the run.
    let _guard = AdvisoryLock::acquire(&catalog_path, LockKind::Ingest)
        .map_err(|e| anyhow::anyhow!("backtest refused — ingest/live in progress: {e}"))?;

    let start_date = parse_date(&cfg.range.start)?;
    let end_date = parse_date(&cfg.range.end)?;
    let start_ns = kst_to_unix_nanos(start_date, midnight())?.as_u64();
    let end_ns = kst_to_unix_nanos(end_date, end_of_day())?.as_u64();

    let instruments = read_all_instruments(&catalog_path).await?;
    let all_bars = read_all_bars(&catalog_path).await?;
    if all_bars.is_empty() {
        anyhow::bail!("catalog holds no bars for the pinned range");
    }

    // The range-scoped catalog fingerprint at start (KTD8).
    let fingerprint_start = range_fingerprint(&all_bars, start_ns, end_ns);

    // Universe scan from prior-session daily bars WITHIN the pinned range (KTD8): the
    // scan must key only on data inside the fingerprinted window, or accumulate-forward
    // growth outside the range would silently change the selected universe while the
    // range-scoped fingerprint stayed identical — breaking run comparability.
    let sink = DecisionSink::new();
    let (candidates, missing) = build_candidates(&instruments, &all_bars, start_ns, end_ns);
    let selected_symbols = select_universe(&candidates, &cfg.params, &sink, start_ns);

    // A backtest run trades a SINGLE session — the last trading day whose daily bar
    // falls in the pinned range (the session the universe scan selected "today" from).
    // Pinning one session keeps the run reproducible and matches the universe scan; a
    // multi-day range grows coverage but a run always trades its last in-range session.
    let session_date = all_bars
        .iter()
        .filter(|b| is_daily(b) && in_range(b, start_ns, end_ns))
        .map(kst_date_of)
        .max();

    // Resolve the selected symbols to instrument ids + minute bar types.
    let selected: Vec<SelectedSymbol> = instruments
        .iter()
        .filter(|i| selected_symbols.iter().any(|s| s == &i.id().to_string()))
        .filter_map(|i| {
            BarKind::Minute(cfg.minute_step)
                .bar_type(i.id())
                .ok()
                .map(|bar_type| SelectedSymbol { instrument_id: i.id(), bar_type })
        })
        .collect();

    // The selected symbols' minute bars for the pinned session feed the engine.
    let minute_bars: Vec<Bar> = all_bars
        .iter()
        .filter(|b| is_minute(b) && in_range(b, start_ns, end_ns))
        .filter(|b| session_date.is_none_or(|d| kst_date_of(b) == d))
        .filter(|b| selected.iter().any(|s| s.bar_type == b.bar_type))
        .cloned()
        .collect();

    let engine_instruments: Vec<InstrumentAny> = instruments.clone();
    let params = cfg.params.clone();
    let sink_for_engine = sink.clone();
    let selected_for_engine = selected.clone();
    let starting_balance = cfg.starting_balance;

    // The engine drives an internal runtime (`block_on`) → run on the blocking pool
    // (the documented catalog/engine `spawn_blocking` gotcha). Extract positions
    // inside the closure before the engine drops.
    let positions: Vec<Position> = tokio::task::spawn_blocking(move || {
        run_engine(
            engine_instruments,
            minute_bars,
            params,
            selected_for_engine,
            sink_for_engine,
            starting_balance,
        )
    })
    .await??;

    // Test hook: simulate any mid-run catalog mutation before the finalize re-check.
    before_finalize.await;

    // Re-check the fingerprint at finalize: a mid-run catalog mutation invalidates the
    // run (no registry residue, KTD8).
    let all_bars_end = read_all_bars(&catalog_path).await?;
    let fingerprint_end = range_fingerprint(&all_bars_end, start_ns, end_ns);
    if fingerprint_end != fingerprint_start {
        anyhow::bail!("catalog changed in-range during the run — aborting with no registry residue");
    }

    // Assemble artifacts.
    let checkpoint = load_checkpoint(&catalog_path);
    let performance = PerformanceReport::from_positions(&positions, cfg.starting_balance);
    let mut data_quality = DataQualityReport::backtest(
        selected_symbols.clone(),
        checkpoint.as_ref().map(|c| c.adjusted_prices).unwrap_or(false),
    );
    data_quality.coverage_gaps = collect_gaps(checkpoint.as_ref(), &missing);

    let rid = run_id(start, RunSource::Backtest, &cfg.params.strategy_id, cfg.params.strategy_version);
    let manifest = Manifest {
        run_id: rid.clone(),
        source: RunSource::Backtest,
        strategy_id: cfg.params.strategy_id.clone(),
        strategy_version: cfg.params.strategy_version,
        params: cfg.params.clone(),
        data_range: cfg.range.clone(),
        catalog_fingerprint: fingerprint_start,
        universe_hash: universe_hash(&selected_symbols),
        strategy_code_hash: crate::artifacts::manifest::strategy_code_hash(),
        checkpoint_hash: checkpoint_hash(&catalog_path),
        created_utc: start.to_rfc3339(),
    };

    let writer = RunWriter::new(&cfg.data_home, &rid)?;
    writer.write_manifest(&manifest)?;
    writer.write_performance(&performance)?;
    writer.write_data_quality(&data_quality)?;
    writer.write_decisions(&sink.snapshot())?;
    let run_dir = writer.finalize()?;

    Ok(RunOutcome { run_dir, run_id: rid })
}

/// Build universe candidates from prior-session daily bars: per instrument with ≥2
/// in-catalog daily bars, `prior_close` = the second-to-last close, `today_open` =
/// the last open, `prior_turnover` = the prior bar's close × volume. Instruments
/// without a prior daily bar are returned as `missing` (a data-quality gap).
fn build_candidates(
    instruments: &[InstrumentAny],
    all_bars: &[Bar],
    start_ns: u64,
    end_ns: u64,
) -> (Vec<UniverseCandidate>, Vec<String>) {
    let mut candidates = Vec::new();
    let mut missing = Vec::new();
    for inst in instruments {
        let id = inst.id();
        // Only daily bars INSIDE the pinned range drive the scan (KTD8 comparability).
        let mut daily: Vec<&Bar> = all_bars
            .iter()
            .filter(|b| is_daily(b) && b.bar_type.instrument_id() == id && in_range(b, start_ns, end_ns))
            .collect();
        daily.sort_by_key(|b| b.ts_event.as_u64());
        if daily.len() < 2 {
            missing.push(id.to_string());
            continue;
        }
        let prior = daily[daily.len() - 2];
        let today = daily[daily.len() - 1];
        candidates.push(UniverseCandidate {
            symbol: id.to_string(),
            prior_close: prior.close.as_f64(),
            today_open: today.open.as_f64(),
            prior_turnover: prior.close.as_f64() * prior.volume.as_f64(),
        });
    }
    (candidates, missing)
}

/// Build + run the engine, returning the finished positions (cloned).
fn run_engine(
    instruments: Vec<InstrumentAny>,
    bars: Vec<Bar>,
    params: OrbParams,
    selected: Vec<SelectedSymbol>,
    sink: DecisionSink,
    starting_balance: f64,
) -> anyhow::Result<Vec<Position>> {
    let mut engine = BacktestEngine::new(BacktestEngineConfig {
        bypass_logging: true,
        ..Default::default()
    })?;
    engine.add_venue(
        SimulatedVenueConfig::builder()
            .venue(Venue::from(nautilus_ls::KRX_VENUE))
            .oms_type(OmsType::Netting)
            .account_type(AccountType::Margin)
            .base_currency(Currency::KRW())
            .book_type(BookType::L1_MBP)
            .starting_balances(vec![Money::new(starting_balance, Currency::KRW())])
            .build()
            .map_err(|e| anyhow::anyhow!("venue build: {e}"))?,
    )?;
    for inst in &instruments {
        engine.add_instrument(inst)?;
    }
    engine.add_strategy(OrbStrategy::new(params, selected, sink))?;

    if !bars.is_empty() {
        let mut sorted = bars;
        sorted.sort_by_key(|b| b.ts_event.as_u64());
        let data: Vec<Data> = sorted.into_iter().map(Data::Bar).collect();
        engine.add_data(data, None, true, true)?;
        engine.run(None, None, None, false)?;
    }

    let cache = engine.kernel().cache.borrow();
    let positions = cache
        .positions(None, None, None, None, None)
        .into_iter()
        .map(|p| p.cloned())
        .collect();
    Ok(positions)
}

fn collect_gaps(checkpoint: Option<&Checkpoint>, missing: &[String]) -> Vec<CoverageGapRecord> {
    let mut gaps: Vec<CoverageGapRecord> = missing
        .iter()
        .map(|sym| CoverageGapRecord {
            instrument: sym.clone(),
            bar_type: String::new(),
            range: String::new(),
            reason: GapReasonKind::MissingPriorDaily,
        })
        .collect();
    if let Some(cp) = checkpoint {
        for g in cp.gaps() {
            gaps.push(CoverageGapRecord {
                instrument: g.instrument.clone(),
                bar_type: g.bar_type.clone(),
                range: g.range.clone(),
                reason: GapReasonKind::EmptyFeed,
            });
        }
    }
    gaps
}

fn load_checkpoint(catalog_path: &Path) -> Option<Checkpoint> {
    Checkpoint::load(&catalog_path.join("ingest-checkpoint.json")).ok()
}

fn checkpoint_hash(catalog_path: &Path) -> Option<String> {
    std::fs::read(catalog_path.join("ingest-checkpoint.json"))
        .ok()
        .map(|bytes| hash_bytes(&bytes))
}

fn is_daily(b: &Bar) -> bool {
    b.bar_type.spec().aggregation == BarAggregation::Day
}
fn is_minute(b: &Bar) -> bool {
    b.bar_type.spec().aggregation == BarAggregation::Minute
}
fn in_range(b: &Bar, start_ns: u64, end_ns: u64) -> bool {
    let ts = b.ts_event.as_u64();
    ts >= start_ns && ts <= end_ns
}

/// The KST calendar date of a bar (its UTC `ts_event` shifted to KST, +09:00).
fn kst_date_of(b: &Bar) -> NaiveDate {
    let dt = chrono::DateTime::<Utc>::from_timestamp_nanos(b.ts_event.as_u64() as i64);
    (dt + chrono::Duration::hours(nautilus_ls::rules::KST_UTC_OFFSET_HOURS as i64)).date_naive()
}

fn parse_date(s: &str) -> anyhow::Result<NaiveDate> {
    Ok(NaiveDate::parse_from_str(s.trim(), "%Y%m%d")?)
}
fn midnight() -> NaiveTime {
    NaiveTime::from_hms_opt(0, 0, 0).unwrap()
}
fn end_of_day() -> NaiveTime {
    NaiveTime::from_hms_opt(23, 59, 59).unwrap()
}

/// CLI entry point for the `lab-backtest` bin. Reads config from env:
/// `LS_DATA_HOME`, `LS_BT_SDATE`, `LS_BT_EDATE` (required); `LS_BT_MINUTE_STEP`,
/// `LS_BT_BALANCE` (optional).
pub fn main_cli() -> anyhow::Result<()> {
    nautilus_ls::scrub::install();
    let data_home = std::env::var("LS_DATA_HOME").map_err(|_| anyhow::anyhow!("LS_DATA_HOME is required"))?;
    let sdate = std::env::var("LS_BT_SDATE").map_err(|_| anyhow::anyhow!("LS_BT_SDATE is required"))?;
    let edate = std::env::var("LS_BT_EDATE").map_err(|_| anyhow::anyhow!("LS_BT_EDATE is required"))?;
    let mut cfg = BacktestConfig::new(data_home, &sdate, &edate);
    if let Ok(step) = std::env::var("LS_BT_MINUTE_STEP") {
        cfg.minute_step = step.parse().unwrap_or(1);
    }
    if let Ok(bal) = std::env::var("LS_BT_BALANCE") {
        cfg.starting_balance = bal.parse().unwrap_or(cfg.starting_balance);
    }
    let rt = tokio::runtime::Runtime::new()?;
    let outcome = rt.block_on(run(cfg, Utc::now()))?;
    println!("finalized run {} at {}", outcome.run_id, outcome.run_dir.display());
    Ok(())
}
