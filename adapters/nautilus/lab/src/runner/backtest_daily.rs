//! Daily-resolution, multi-session-hold backtest path (P7, U1) — the **second**,
//! additive runner beside [`crate::runner::backtest`] (KTD2). Where the ORB path
//! resets structurally every session (a fresh `BacktestEngine` per day over that
//! day's minute bars), this path drives **one** engine over a *streaming* daily bar
//! stream so an open position survives session boundaries (KTD1, R1).
//!
//! The loop splits into two phases:
//!
//! 1. **Selection (pure, no engine — KTD11).** Index the catalog once, derive the
//!    in-range session dates, build each session's candidates with the *shared*
//!    [`build_candidates`] assembly, and rank them with a caller-supplied rule.
//!    `select_universe` is deliberately NOT called: it encodes ORB's gap/top-N
//!    hypothesis, not this one (KTD15). Ranking must happen here rather than in a
//!    bar callback because `run_impl` delivers one datum at a time, so a
//!    cross-sectional rank computed on bar *k* of *N* is silently partial.
//! 2. **Engine (one engine, streaming).** `clear_data()` → `add_data(batch, sort)` →
//!    `run(.., streaming = true)` per session, `end()` once at the end, then a single
//!    cache read (R4). The venue is `OmsType::Hedging` (KTD12) so a symbol re-entered
//!    after a completed hold mints a *distinct* position instead of silently
//!    snapshotting the earlier round trip out of the live index (R19).
//!
//! # The missing-data policy: fail closed, never carry and never synthesize
//!
//! A held position **must** receive its symbol's daily bar on every session of its
//! hold. Both of this path's exits fire from `DataActor::on_bar`, so a session that
//! delivers no bar for a held symbol hands the strategy no callback for that position
//! at all — its stop cannot evaluate and its hold-expiry exit cannot fire. The hold
//! then runs past the **pre-registered** `holding_period_sessions` and the run still
//! finalizes green, moving the frozen verdict statistic under a term that is not
//! supposed to be able to move.
//!
//! The policy is therefore to abort the run with
//! [`DailyEngineError::HeldSymbolMissingBar`], enforced in [`engine_phase`] against
//! [`build_batch`]'s output on the **held** set. The two alternatives were considered
//! and rejected: carrying the position silently *is* the defect, and flattening at a
//! last-known price invents an execution a daily-resolution observer never saw, with
//! its own lineage consequences. Neither may be adopted implicitly, so a catalog gap
//! under a live hold is surfaced to the operator to heal rather than absorbed into a
//! number. Note the empty-batch skip is not the same guard: a batch can be non-empty
//! from other names while the held one is absent.
//!
//! The already-held set is engine state, and R4 forbids a per-session position-report
//! read, so the runner clones an [`OpenPositionBook`] handle off the strategy *before*
//! mounting it — the same shared-handle pattern `run_engine` uses for the entry-risk
//! ledger — and reads it between batches (KTD16). Because
//! `BacktestEngine::add_strategy` is a **sized generic**, the runner is generic over
//! the strategy type rather than taking a boxed factory; that is also what lets this
//! unit land and be tested before the daily strategy exists.
//!
//! # Where the pieces live
//!
//! This module owns the **engine phase** and the entry points that drive it. The
//! parts that are separable from the engine live in `backtest_daily/` and are
//! re-exported here, so `runner::backtest_daily::<Item>` keeps working:
//!
//! - `selection` — the pure selection phase (KTD11), engine-free by construction.
//! - `handles` — the shared handles ([`OpenPositionBook`], [`DailySessionSignals`])
//!   and the [`DailyPathStrategy`] contract that exposes them.
//! - `entry_risk` — the KTD3/R12 projection seam and its three assertions.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use nautilus_backtest::config::{BacktestEngineConfig, SimulatedVenueConfig};
use nautilus_backtest::engine::BacktestEngine;
use nautilus_common::actor::DataActorNative;
use nautilus_common::component::Component;
use nautilus_ls::ingest::{kst_to_unix_nanos, read_all_bars, read_all_instruments, BarKind};
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_model::data::{Bar, BarType, Data};
use nautilus_model::enums::{AccountType, BookType, OmsType};
use nautilus_model::identifiers::{ClientOrderId, InstrumentId, PositionId, Venue};
use nautilus_model::instruments::{Instrument, InstrumentAny};
use nautilus_model::position::Position;
use nautilus_model::types::{Currency, Money, Price};
use nautilus_trading::strategy::{Strategy, StrategyNative};

use crate::agent::sink::DecisionSink;
use crate::artifacts::data_quality::DataQualityReport;
use crate::artifacts::manifest::{
    range_fingerprint, universe_sequence_hash, DailyManifestParts, DataRange, Manifest,
};
use crate::artifacts::observation::{ObservationParts, RunObservation};
use crate::artifacts::performance::{EntryRisk, PerformanceReport};
use crate::artifacts::{run_id, RunSource, RunWriter};
use crate::params::OrbParams;
use crate::params_daily::{DailyParams, RankingSignalKind};
use crate::strategy::daily::{
    rank_by_placeholder_signal, rank_by_signal, record_signal_decisions, AdjustmentBasisShifts,
    DailyStrategy,
};
use crate::strategy::orb::UniverseCandidate;

mod entry_risk;
mod handles;
mod selection;

pub use entry_risk::{project_entry_risks, EntryRiskProjection};
pub use handles::{DailyPathStrategy, DailySessionContext, DailySessionSignals, OpenPositionBook};
pub use selection::{
    select_daily_sessions, DailySelection, DailySessionPlan, DAILY_EQUITY_MULTIPLIER,
};

use selection::{index_daily, select_from_index, session_dates_of};

/// One instrument mounted into the engine up front, with the daily bar series it is
/// driven on. Every instrument is mounted before the first batch (KTD11) so the loop
/// never adds an instrument mid-stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountedSymbol {
    /// The instrument id (`{shcode}.XKRX`).
    pub instrument_id: InstrumentId,
    /// The daily bar type — `BarKind::Daily`, never `BarKind::Minute` (R2).
    pub bar_type: BarType,
}

// ---------------------------------------------------------------------------
// Engine phase (one engine, streaming — KTD1)
// ---------------------------------------------------------------------------

/// The failure modes of the streaming loop that must abort the run rather than
/// finish green over a silently truncated position set.
#[derive(Debug, thiserror::Error)]
pub enum DailyEngineError {
    /// The first batch processed zero items. If the first-iteration block does not
    /// run, it re-runs on the next `run()` — minting a new run id, a new
    /// `backtest_start`, and calling `initialize_account()` mid-stream.
    #[error(
        "the first daily batch on {date} processed zero items (engine.iteration() == 0): the \
         engine's first-iteration block would re-run on the next batch, re-initializing the \
         account mid-stream"
    )]
    FirstBatchNoIteration {
        /// The session whose batch was submitted first.
        date: NaiveDate,
    },
    /// The engine finished mid-stream. A `force_stop` calls `end()` even under
    /// `streaming = true`, and because `iteration != 0` no later `run()` restarts the
    /// trader — the loop would then route bars into a stopped trader and finish green
    /// with a truncated position set.
    #[error(
        "the backtest engine finished mid-stream after session {date} (run_finished = \
         {finished_ns}): a force-stop ends the run even under streaming, and no later run() \
         restarts the trader — later sessions would be routed into a stopped trader"
    )]
    RunFinishedMidStream {
        /// The session after which the engine reported finished.
        date: NaiveDate,
        /// The engine's `run_finished` timestamp.
        finished_ns: u64,
    },
    /// A held position's symbol contributed no daily bar to a session of its hold.
    /// Both exits fire from `on_bar`, so the position would silently outlive the
    /// pre-registered hold — see the check in [`engine_phase`] for why this fails
    /// the run closed rather than carrying or synthesizing an exit.
    #[error(
        "session {date} delivered no daily bar for {} held position(s) [{}]: the stop and the \
         hold-expiry exit both fire from on_bar, so a held symbol absent from the session's \
         batch receives no callback — its exit is postponed to whichever later session does \
         deliver a bar, or never fires at all, and the position outlives the PRE-REGISTERED \
         holding_period_sessions while the run finalizes green. Heal the catalog gap on these \
         symbols over this session and re-run",
        .missing.len(),
        format_instrument_ids(.missing)
    )]
    HeldSymbolMissingBar {
        /// The session whose batch was missing the held symbols' bars.
        date: NaiveDate,
        /// The held symbols with no bar on that session, in id order.
        missing: Vec<InstrumentId>,
    },
}

/// Render instrument ids for an error message, comma-separated in the given order.
fn format_instrument_ids(ids: &[InstrumentId]) -> String {
    ids.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
}

/// A duplicate daily bar dropped from a session batch (R23). A surviving
/// value-divergent duplicate would deliver two callbacks for one session, which
/// both shortens the frozen hold and fires the stop check twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateBarDrop {
    /// The session the duplicate was dropped from.
    pub date: NaiveDate,
    /// The instrument the duplicate belonged to.
    pub instrument_id: InstrumentId,
    /// The `ts_event` both copies shared.
    pub ts_event: u64,
    /// Whether the dropped copy's OHLCV differed from the kept one (a genuine
    /// adjustment-basis conflict, not a benign re-ingest overlap).
    pub divergent: bool,
}

/// What one session's pre-batch step resolved and submitted.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionBatch {
    /// The session date.
    pub date: NaiveDate,
    /// The symbols already holding an open position at the session's pre-batch step.
    pub held: Vec<InstrumentId>,
    /// The newly taken symbols: the top `target_m` of the ranked list from those not
    /// already held (R10).
    pub taken: Vec<InstrumentId>,
    /// The number of bars actually submitted after the R23 dedupe.
    pub bars: usize,
    /// Whether the whole `clear_data` / `add_data` / `run` cycle was skipped because
    /// the batch was empty (`add_data` errors on an empty slice).
    pub skipped: bool,
}

/// The outcome of a daily multi-session run.
#[derive(Debug, Clone)]
pub struct DailyRunOutcome {
    /// The single post-`end()` cache read (R4), in cache order.
    pub positions: Vec<Position>,
    /// The pure selection phase's output.
    pub selection: DailySelection,
    /// What each session's pre-batch step resolved.
    pub batches: Vec<SessionBatch>,
    /// Every duplicate bar dropped by the R23 dedupe.
    pub duplicate_drops: Vec<DuplicateBarDrop>,
    /// Every position id observed opening across the stream, in observation order.
    pub observed_position_ids: Vec<PositionId>,
    /// The entry-risk ledger projected into [`Self::positions`]' order (KTD3, R12),
    /// ready to hand straight to `from_positions_with_risk`. Already reconciled by
    /// the three seam assertions.
    pub entry_risks: Vec<Option<EntryRisk>>,
    /// Recorded entry orders that never opened a position — a venue or risk-engine
    /// rejection. A named diagnostic, not a failure (KTD3).
    pub unopened_entry_orders: Vec<ClientOrderId>,
}

/// The daily path's run configuration.
#[derive(Debug, Clone)]
pub struct DailyBacktestConfig {
    /// The data home (`<data>/catalog`, `<data>/runs`, …).
    pub data_home: PathBuf,
    /// The explicit pinned bar-data range.
    pub range: DataRange,
    /// The candidate-assembly parameter set — the knobs the *shared*
    /// [`build_candidates`] reads. The daily *selection rule* is the caller's `rank`
    /// (KTD15), not anything in here, and the sizing term is
    /// [`DailyParams::notional_per_position`], never anything in here (R27).
    ///
    /// Its `atr_window` is **not** authoritative on this path: [`Self::assembly_params`]
    /// forces it from [`DailyParams::atr_window_sessions`]. See that method for why.
    pub params: OrbParams,
    /// The daily parameter set — the frozen terms, the per-session take
    /// ([`DailyParams::target_m`], R10), and the source of the manifest's registry
    /// discriminator (KTD14). This is the *single* home for `target_m`: carrying a second
    /// copy on this struct bought only a runtime check that the two agreed.
    pub daily: DailyParams,
    /// Starting account balance (KRW).
    pub starting_balance: f64,
}

impl DailyBacktestConfig {
    /// A config over `data_home` for `[start, end]` (YYYYMMDD), taking the frozen daily
    /// terms with `target_m` overridden — a fixture may run *fewer* than the frozen 8,
    /// never more, and [`DailyParams::validate`] enforces that ceiling.
    pub fn new(data_home: impl Into<PathBuf>, start: &str, end: &str, target_m: usize) -> Self {
        DailyBacktestConfig {
            data_home: data_home.into(),
            range: DataRange { start: start.to_string(), end: end.to_string() },
            params: OrbParams::default(),
            daily: DailyParams { target_m, ..DailyParams::default() },
            starting_balance: 100_000_000.0,
        }
    }

    /// The candidate-assembly parameters actually handed to [`build_candidates`], with
    /// `atr_window` **forced** from [`DailyParams::atr_window_sessions`].
    ///
    /// This bridge is not a convenience. `OrbParams::atr_window` defaults to 14 — ORB's
    /// term, needing 15 prior sessions — while the frozen daily stop is `ATR(1)`. The two
    /// live in different structs and nothing else connects them, so an unbridged config
    /// derives a prior ATR that is absent for the first 14 in-range sessions of every
    /// symbol. Under the fail-closed stop (KTD9) that is not a visible misconfiguration:
    /// every entry is refused `atr_unavailable`, the run finalizes green with zero
    /// positions, and `return_on_risk` is vacuous. Forcing it here means the value is
    /// wrong in exactly one place or none.
    ///
    /// The forced value is what the manifest records as `params`, because it is what
    /// assembly ran with.
    #[must_use]
    pub fn assembly_params(&self) -> OrbParams {
        OrbParams { atr_window: self.daily.atr_window_sessions, ..self.params.clone() }
    }
}

/// Run the daily multi-session path to a position set.
///
/// The catalog read is async; **everything else** — selection and the whole engine
/// lifecycle — runs inside one [`tokio::task::spawn_blocking`] closure with owned
/// data moved in (R5), because the catalog and the engine both drive an internal
/// `block_on` and panic from an async context.
///
/// `make_strategy` receives every mounted symbol and returns the strategy; the runner
/// clones its [`OpenPositionBook`] before `add_strategy` consumes it (KTD16).
pub async fn run_daily<S, R, F>(
    cfg: DailyBacktestConfig,
    sink: DecisionSink,
    rank: R,
    make_strategy: F,
) -> anyhow::Result<DailyRunOutcome>
where
    S: DailyPathStrategy
        + Strategy
        + StrategyNative
        + DataActorNative
        + Component
        + std::fmt::Debug
        + 'static,
    R: Fn(&[UniverseCandidate]) -> Vec<String> + Send + 'static,
    F: FnOnce(&[MountedSymbol]) -> S + Send + 'static,
{
    let (_catalog_path, _guard) = acquire_catalog_guard(&cfg.data_home)?;
    run_daily_locked(cfg, sink, rank, make_strategy, None)
        .await
        .map(|locked| locked.outcome)
        .map_err(DailyRunFailure::into_error)
}

/// Check the catalog exists and take the ingest advisory lock over it.
///
/// Shared by the two entry points so they cannot drift on the refusal message, but
/// deliberately returning the guard rather than holding it: the whole difference between
/// [`run_daily`] and [`run_inner`] is the guard's *scope*. `run_daily` needs it for the
/// engine phase; `run_inner` must hold one continuous guard across the engine phase and
/// the finalize re-check, or it opens the very window that re-check exists to detect.
fn acquire_catalog_guard(data_home: &Path) -> anyhow::Result<(PathBuf, AdvisoryLock)> {
    let catalog_path = data_home.join("catalog");
    if !catalog_path.exists() {
        anyhow::bail!("no catalog at {} — ingest first", catalog_path.display());
    }
    // Own-guard: refuse if ingest is running, and hold the ingest lock so the catalog
    // cannot be mutated mid-run. Released on drop / at end of the run.
    let guard = AdvisoryLock::acquire(&catalog_path, LockKind::Ingest)
        .map_err(|e| anyhow::anyhow!("daily backtest refused — ingest/live in progress: {e}"))?;
    Ok((catalog_path, guard))
}

/// [`run_daily`]'s body with the advisory lock **already held by the caller**, plus the
/// range-scoped catalog fingerprint the finalize re-check compares against.
///
/// Split out because the lock's scope differs between the two entry points. [`run_daily`]
/// only needs it for the engine phase, but [`run_inner`] must hold one continuous guard
/// across the engine phase *and* the finalize re-check — re-acquiring it in a nested call
/// would either deadlock or, worse, open a window in which the catalog can be mutated
/// between the run and the re-check that exists to detect exactly that.
async fn run_daily_locked<S, R, F>(
    cfg: DailyBacktestConfig,
    sink: DecisionSink,
    rank: R,
    make_strategy: F,
    ranking_signal: Option<RankingSignalKind>,
) -> Result<LockedDailyRun, DailyRunFailure>
where
    S: DailyPathStrategy
        + Strategy
        + StrategyNative
        + DataActorNative
        + Component
        + std::fmt::Debug
        + 'static,
    R: Fn(&[UniverseCandidate]) -> Vec<String> + Send + 'static,
    F: FnOnce(&[MountedSymbol]) -> S + Send + 'static,
{
    let catalog_path = cfg.data_home.join("catalog");
    let start_date = parse_date(&cfg.range.start).map_err(DailyRunFailure::Refused)?;
    let end_date = parse_date(&cfg.range.end).map_err(DailyRunFailure::Refused)?;
    let start_ns = kst_to_unix_nanos(start_date, midnight())
        .map_err(|error| DailyRunFailure::Refused(error.into()))?
        .as_u64();
    let end_ns = kst_to_unix_nanos(end_date, end_of_day())
        .map_err(|error| DailyRunFailure::Refused(error.into()))?
        .as_u64();

    let instruments = read_all_instruments(&catalog_path)
        .await
        .map_err(|error| DailyRunFailure::Refused(error.into()))?;
    let all_bars = read_all_bars(&catalog_path)
        .await
        .map_err(|error| DailyRunFailure::Refused(error.into()))?;

    // The range-scoped catalog fingerprint at start. Taken here, before any engine work,
    // so the finalize re-check compares against the catalog the run actually read.
    let fingerprint_start = range_fingerprint(&all_bars, start_ns, end_ns);

    // Mount a price grid the catalog's own in-range prices sit on, so the matching
    // engine cannot silently decline a fill (R30 probe, 2026-09-08). The adapter's
    // live-derived increment stays untouched — see `regrid_instruments_for_range`.
    let (instruments, tick_regrids) =
        regrid_instruments_for_range(&instruments, &all_bars, start_ns, end_ns);
    if !tick_regrids.is_empty() {
        tracing::info!(
            "re-gridded price_increment on {} of {} instruments for the historical range \
             (the adapter's increment is derived from today's reference price and this \
             catalog carries adjusted prices); e.g. {}",
            tick_regrids.len(),
            instruments.len(),
            tick_regrids
                .iter()
                .take(3)
                .map(|m| format!("{} {}->{}", m.instrument_id, m.from_increment, m.to_increment))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    let tick_regrid_count = tick_regrids.len();

    // The ATR bridge (see `DailyBacktestConfig::assembly_params`) — the frozen daily ATR
    // window reaches the shared assembly here, and nowhere else.
    let params = cfg.assembly_params();
    let starting_balance = cfg.starting_balance;
    let target_m = cfg.daily.target_m;
    let daily_params = cfg.daily.clone();
    let blocking = tokio::task::spawn_blocking(move || {
        run_daily_blocking(
            DailyBlockingRun {
                instruments: &instruments,
                all_bars: &all_bars,
                params: &params,
                sink: &sink,
                starting_balance,
                target_m,
                start_ns,
                end_ns,
                rank: &rank,
                ranking_signal,
                daily_params: &daily_params,
            },
            make_strategy,
        )
    })
    .await;
    let outcome = match blocking {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(error)) => return Err(DailyRunFailure::Aborted(error)),
        Err(error) => {
            return Err(DailyRunFailure::Aborted(anyhow::anyhow!(
                "daily blocking task aborted: {error}"
            )));
        }
    };

    Ok(LockedDailyRun { outcome, fingerprint_start, start_ns, end_ns, tick_regrids: tick_regrid_count })
}

/// Whether a failure occurred before the blocking engine started (a refusal) or after
/// ownership crossed that boundary (an abort). Only refusals remove run staging.
enum DailyRunFailure {
    Refused(anyhow::Error),
    Aborted(anyhow::Error),
}

impl DailyRunFailure {
    fn into_error(self) -> anyhow::Error {
        match self {
            Self::Refused(error) | Self::Aborted(error) => error,
        }
    }
}

/// [`run_daily_locked`]'s output: the run plus the finalize re-check's inputs.
struct LockedDailyRun {
    outcome: DailyRunOutcome,
    fingerprint_start: String,
    start_ns: u64,
    end_ns: u64,
    /// How many instruments were mounted on a re-gridded increment (R30 probe).
    tick_regrids: usize,
}

// ---------------------------------------------------------------------------
// The finalized entry point (U5)
// ---------------------------------------------------------------------------

/// A finalized daily run: the registry artifacts plus the in-memory objects they were
/// written from, so a caller need not read the run back off disk to inspect it.
#[derive(Debug, Clone)]
pub struct DailyRunResult {
    /// The finalized run directory.
    pub run_dir: PathBuf,
    /// The run id — derived from the *daily* discriminator, never from
    /// `assembly_params.strategy_id` (KTD14).
    pub run_id: String,
    /// The manifest as written.
    pub manifest: Manifest,
    /// The performance report as written.
    pub performance: PerformanceReport,
    /// The typed run observation as written (U6).
    pub observation: RunObservation,
    /// The engine phase's full outcome.
    pub outcome: DailyRunOutcome,
    /// How many instruments this run mounted on a re-gridded `price_increment` (R30
    /// probe, 2026-09-08). Reported in the summary block rather than only logged: a
    /// log line scrolls away under the engine's per-session INFO volume, and a zero
    /// here on a deep-history catalog is itself worth noticing.
    pub tick_regrids: usize,
}

/// Run the daily multi-session path to a **finalized registry run** (R18).
///
/// This is the daily sibling of [`crate::runner::backtest::run`], and it deliberately
/// duplicates that path's preamble and tail rather than generalizing it (KTD2): the
/// advisory lock, the range-fingerprint assert-and-re-check, and the artifact-writing
/// tail. The shared *candidate assembly* is reused at its current signature; the
/// selection rule, the venue, the OMS, and the hold semantics are this path's own.
pub async fn run(cfg: DailyBacktestConfig, start: DateTime<Utc>) -> anyhow::Result<DailyRunResult> {
    run_inner(cfg, start, std::future::ready(())).await
}

/// [`run`] with a hook awaited between the engine run and the finalize fingerprint
/// re-check — the library seam a test uses to simulate a mid-run catalog mutation. It is
/// deliberately **not** reachable through [`main_cli`]; the public [`run`] passes a no-op.
pub async fn run_inner<F: std::future::Future<Output = ()>>(
    cfg: DailyBacktestConfig,
    start: DateTime<Utc>,
    before_finalize: F,
) -> anyhow::Result<DailyRunResult> {
    // Fail fast on a parameter set off a frozen term, before any catalog or engine work.
    //
    // `Manifest::new_daily` is the *construction-point* gate and calls `validate()` again
    // — that one is what makes an invalid set unable to reach the registry through any
    // caller. This call is what keeps it from reaching the **engine**: without it a bad
    // config burns the whole run and only errors at manifest assembly, which for the
    // 837-session window is hours. Two named call sites, two different jobs.
    cfg.daily
        .validate()
        .map_err(|e| anyhow::anyhow!("invalid daily parameter set: {e}"))?;

    // ONE guard spanning the engine phase and the finalize re-check (see
    // `run_daily_locked`). Released on drop, at end of the run.
    let (catalog_path, _guard) = acquire_catalog_guard(&cfg.data_home)?;

    let assembly_params = cfg.assembly_params();
    let data_home = cfg.data_home.clone();
    let data_range = cfg.range.clone();
    let daily_params = cfg.daily.clone();
    let ranking_signal = daily_params.ranking_signal;
    let starting_balance = cfg.starting_balance;
    let expected_run_id = run_id(
        start,
        RunSource::Backtest,
        &daily_params.strategy_id,
        daily_params.strategy_version,
    );
    let writer = RunWriter::new(&data_home, &expected_run_id)?;
    let sink = match writer.stream_decisions() {
        Ok(sink) => sink,
        Err(error) => return Err(discard_staging(writer, error)),
    };

    // The R22 gate's input. A catalog with no checkpoint genuinely has no recorded shift
    // ledger, which `AdjustmentBasisShifts::none` states explicitly rather than implying.
    let shifts = match crate::runner::backtest::load_checkpoint(&catalog_path) {
        Some(cp) => AdjustmentBasisShifts::from_checkpoint(&cp),
        None => AdjustmentBasisShifts::none(),
    };
    let make_strategy =
        DailyStrategy::factory(daily_params.clone(), sink.clone(), shifts);

    // The placeholder ranking signal (KTD6). It is named and marked, not scaffolding: the
    // signal that carries the hypothesis is turn one's act, and U6's observation refuses to
    // yield judgment arguments while the marker is set.
    let locked = run_daily_locked(
        cfg,
        sink.clone(),
        rank_by_placeholder_signal,
        make_strategy,
        Some(ranking_signal),
    )
    .await;
    let (writer, locked) = resolve_locked_run(writer, locked)?;

    // Test hook: simulate any mid-run catalog mutation before the finalize re-check.
    before_finalize.await;

    // Re-check the fingerprint at finalize: a mid-run catalog mutation invalidates the run
    // and leaves NO registry residue. The decision stream opened staging before selection,
    // so this graceful refusal explicitly removes it; a crash still leaves the normal
    // `.tmp-` aborted-run marker.
    let all_bars_end = match read_all_bars(&catalog_path).await {
        Ok(bars) => bars,
        Err(error) => return Err(discard_staging(writer, error.into())),
    };
    let fingerprint_end = range_fingerprint(&all_bars_end, locked.start_ns, locked.end_ns);
    if fingerprint_end != locked.fingerprint_start {
        return Err(discard_staging(
            writer,
            anyhow::anyhow!(
                "catalog changed in-range during the daily run — aborting with no registry residue"
            ),
        ));
    }

    finalize_daily_run(FinalizeDaily {
        writer,
        started_utc: start,
        outcome: locked.outcome,
        fingerprint_start: locked.fingerprint_start,
        assembly_params,
        daily_params,
        data_range,
        starting_balance,
        catalog_path: &catalog_path,
        tick_regrids: locked.tick_regrids,
    })
}

fn resolve_locked_run(
    writer: RunWriter,
    result: Result<LockedDailyRun, DailyRunFailure>,
) -> anyhow::Result<(RunWriter, LockedDailyRun)> {
    match result {
        Ok(locked) => Ok((writer, locked)),
        Err(DailyRunFailure::Refused(error)) => Err(discard_staging(writer, error)),
        Err(DailyRunFailure::Aborted(error)) => Err(error),
    }
}

fn discard_staging(writer: RunWriter, error: anyhow::Error) -> anyhow::Error {
    match writer.discard() {
        Ok(()) => error,
        Err(discard_error) => anyhow::anyhow!(
            "{error}; additionally failed to remove the refused run's staging directory: {discard_error}"
        ),
    }
}

/// Everything [`finalize_daily_run`] needs. A struct rather than eleven positional
/// arguments, matching [`crate::artifacts::manifest::DailyManifestParts`]'s reasoning.
struct FinalizeDaily<'a> {
    writer: RunWriter,
    started_utc: DateTime<Utc>,
    outcome: DailyRunOutcome,
    fingerprint_start: String,
    assembly_params: OrbParams,
    daily_params: DailyParams,
    data_range: DataRange,
    starting_balance: f64,
    catalog_path: &'a Path,
    /// How many instruments were mounted on a re-gridded increment (R30 probe).
    tick_regrids: usize,
}

/// Assemble and write the run's artifacts, then finalize the run directory.
fn finalize_daily_run(p: FinalizeDaily<'_>) -> anyhow::Result<DailyRunResult> {
    let checkpoint = crate::runner::backtest::load_checkpoint(p.catalog_path);

    // Zero-rate assembly params yield `None` — the pre-model path. A daily-path
    // transaction-cost model is not this plan's scope; an explicitly-rated config is
    // still honoured rather than silently zeroed.
    let cost_model = crate::strategy::orb::TransactionCostModel::from_params(&p.assembly_params);
    let performance = PerformanceReport::from_positions_with_risk(
        &p.outcome.positions,
        &p.outcome.entry_risks,
        p.starting_balance,
        cost_model.as_ref(),
    );

    // The candidate union is this path's universe snapshot. Intersecting it with the
    // checkpoint's unhealed shift marks is what makes an adjustment-basis rewrite inside a
    // hold *visible* on the run — the risk R22 refuses per position, reported here per run.
    let candidate_union = p.outcome.selection.candidate_union.clone();
    // The in-range session calendar, from the selection phase rather than from the trades:
    // a session with no activity must still appear as a zero row in the series, or a
    // session-block bootstrap resamples a shortened series and understates the error.
    let session_dates: Vec<chrono::NaiveDate> =
        p.outcome.selection.sessions.iter().map(|s| s.date).collect();
    let shift_symbols: Vec<String> = checkpoint
        .as_ref()
        .map(|c| {
            c.shifted_instruments(crate::strategy::daily::DAILY_BAR_TYPE_LABEL)
                .into_iter()
                .filter(|s| candidate_union.contains(s))
                .collect()
        })
        .unwrap_or_default();
    let mut data_quality = DataQualityReport::backtest(candidate_union, shift_symbols);
    // R23's dropped duplicates have to reach a FINALIZED artifact, not just the in-memory
    // outcome. A value-divergent duplicate is the adjustment-basis conflict this path is
    // most exposed to — two bars for one instrument at one `ts_event` whose OHLCV disagree,
    // where the kept copy is simply whichever the catalog yielded first. That choice can
    // move an entry, a stop, and therefore the run's statistic. Recorded here so a run
    // carrying one cannot read as clean; `dedup_hits` is deliberately NOT reused, because it
    // is documented as the live path's ORDER-dedup counter and overloading it would make two
    // different quantities indistinguishable to a reader.
    if !p.outcome.duplicate_drops.is_empty() {
        let divergent: Vec<&DuplicateBarDrop> =
            p.outcome.duplicate_drops.iter().filter(|d| d.divergent).collect();
        data_quality.observations.push(format!(
            "R23 duplicate daily bars dropped: {} total, {} value-divergent",
            p.outcome.duplicate_drops.len(),
            divergent.len()
        ));
        // Name the divergent ones individually — they are the ones that change prices.
        const MAX_LISTED: usize = 20;
        for d in divergent.iter().take(MAX_LISTED) {
            data_quality.observations.push(format!(
                "value-divergent duplicate bar: {} on {} (ts_event {}) — the kept copy is the \
                 first in catalog order; re-ingest may have spliced two adjustment bases",
                d.instrument_id, d.date, d.ts_event
            ));
        }
        if divergent.len() > MAX_LISTED {
            data_quality.observations.push(format!(
                "... and {} further value-divergent duplicates not listed",
                divergent.len() - MAX_LISTED
            ));
        }
    }

    // Every identity-bearing field is derived by the constructor, not passed: `strategy_id`,
    // `strategy_version`, `run_id`, and `strategy_code_hash` all come off `daily_params` and
    // `DAILY_SOURCE`. `assembly_params` is recorded verbatim in the non-optional
    // `Manifest.params` because that field cannot express "this run has no OrbParams" —
    // it is the parameter set the *shared candidate assembly* ran with, and nothing else.
    // Its `strategy_id` still reads "orb" and is deliberately ignored here; U8's filters key
    // on `Manifest.strategy_id`, which `new_daily` takes from the daily discriminator, so
    // this recorded set can never be selected as an ORB baseline.
    let ranking_signal = p.daily_params.ranking_signal;

    // Warmup (U4). Every catalog daily bar feeds the signal — `index_daily` buckets ALL
    // daily bars per instrument, in-range or not — so a signal's lookback is loaded from
    // before `LS_BTD_SDATE` whenever the catalog holds it, and nothing is marked. When the
    // catalog does NOT (the judgment home's floor IS the specification window's first
    // session, KTD10), the signal cannot score the window's opening sessions and warms up
    // inside it. Those are the leading in-range sessions whose ranked list came back
    // EMPTY — recorded here from the selection phase, never inferred from inactivity —
    // so the re-check measures participation over the sessions the signal could score and
    // refuses an entry inside the marked prefix. Only a signal that needs prior bars has a
    // warmup; the one-bar signals are scoreable from the first session that has a prior.
    let warmup_session_dates: Vec<NaiveDate> = if ranking_signal.warmup_bars() > 1 {
        p.outcome
            .selection
            .sessions
            .iter()
            .take_while(|s| s.ranked.is_empty())
            .map(|s| s.date)
            .collect()
    } else {
        Vec::new()
    };
    let manifest = match Manifest::new_daily(DailyManifestParts {
        daily: p.daily_params,
        assembly_params: p.assembly_params,
        daily_source: crate::strategy::DAILY_SOURCE,
        started_utc: p.started_utc,
        data_range: p.data_range,
        catalog_fingerprint: p.fingerprint_start,
        universe_hash: universe_sequence_hash(&p.outcome.selection.selection_sequence()),
        lab_src_fingerprint: Some(crate::fingerprint::EMBEDDED.to_string()),
        checkpoint_hash: crate::runner::backtest::checkpoint_hash(p.catalog_path),
        universe_metadata_hash: None,
    }) {
        Ok(manifest) => manifest,
        Err(error) => {
            return Err(discard_staging(
                p.writer,
                anyhow::anyhow!("daily manifest refused: {error}"),
            ));
        }
    };

    // The fifth artifact (U6). An R25 refusal removes the streaming staging directory:
    // a refusal is not an aborted run, while an actual crash still leaves the marker.
    let observation = match RunObservation::build(ObservationParts {
        run_id: &manifest.run_id,
        data_range: &manifest.data_range,
        catalog_fingerprint: &manifest.catalog_fingerprint,
        performance: &performance,
        session_dates: &session_dates,
        warmup_session_dates: &warmup_session_dates,
        ranking_signal: ranking_signal.name(),
        ranking_signal_is_placeholder: ranking_signal.is_placeholder(),
    }) {
        Ok(observation) => observation,
        Err(error) => {
            return Err(discard_staging(p.writer, anyhow::anyhow!("{error}")));
        }
    };

    debug_assert_eq!(p.writer.run_id(), manifest.run_id);
    p.writer.write_manifest(&manifest)?;
    p.writer.write_performance(&performance)?;
    p.writer.write_data_quality(&data_quality)?;
    p.writer.write_observation(&observation)?;
    let run_dir = p.writer.finalize()?;

    Ok(DailyRunResult {
        run_dir,
        run_id: manifest.run_id.clone(),
        manifest,
        performance,
        observation,
        outcome: p.outcome,
        tick_regrids: p.tick_regrids,
    })
}

/// Parse a required environment variable, naming it in both failure modes.
///
/// The silent-default anti-pattern this refuses is `parse().unwrap_or(1)` at
/// `backtest.rs:1049`, pinned as an anti-pattern by `research_cli.rs:430`: a typo'd
/// `LS_BTD_TARGET_M=8x` would there become a *valid* run at a concurrency nobody chose,
/// and the manifest would record the substituted value as though it were intended.
fn env_parsed<T: std::str::FromStr>(key: &str, default: T) -> anyhow::Result<T> {
    match std::env::var(key) {
        Err(_) => Ok(default),
        Ok(raw) => raw.trim().parse::<T>().map_err(|_| {
            anyhow::anyhow!(
                "{key} must parse as {}, got {raw:?} — refusing rather than defaulting",
                std::any::type_name::<T>()
            )
        }),
    }
}

/// CLI entry point for the `lab-backtest-daily` bin (R18). Reads config from env:
/// `LS_DATA_HOME`, `LS_BTD_SDATE`, `LS_BTD_EDATE` (required); `LS_BTD_TARGET_M`,
/// `LS_BTD_BALANCE`, `LS_BTD_NOTIONAL`, `LS_BTD_VERSION` (optional); `LS_BTD_SIGNAL` (the
/// candidate under test, manifest spelling; unset = placeholder) and `LS_BT_COST_CONFIG`
/// (the committed rate artifact; unset = zero-cost) — see [`apply_signal_and_cost_env`].
///
/// Every optional numeric variable hard-errors on a malformed value rather than
/// defaulting. The frozen terms — hold, directionality, stop multiple, ATR window — take
/// **no** environment override at all: they are frozen, and a run that could move one from
/// the shell is a run that can drift off the pre-registration without a code change.
///
/// The `before_finalize` seam of [`run_inner`] is deliberately unreachable from here.
pub fn main_cli() -> anyhow::Result<()> {
    nautilus_ls::scrub::install();
    let data_home =
        std::env::var("LS_DATA_HOME").map_err(|_| anyhow::anyhow!("LS_DATA_HOME is required"))?;
    let sdate =
        std::env::var("LS_BTD_SDATE").map_err(|_| anyhow::anyhow!("LS_BTD_SDATE is required"))?;
    let edate =
        std::env::var("LS_BTD_EDATE").map_err(|_| anyhow::anyhow!("LS_BTD_EDATE is required"))?;

    let target_m = env_parsed("LS_BTD_TARGET_M", crate::params_daily::FROZEN_TARGET_M)?;
    let mut cfg = DailyBacktestConfig::new(&data_home, &sdate, &edate, target_m);
    cfg.starting_balance = env_parsed("LS_BTD_BALANCE", cfg.starting_balance)?;
    cfg.daily.notional_per_position =
        env_parsed("LS_BTD_NOTIONAL", cfg.daily.notional_per_position)?;
    cfg.daily.strategy_version = env_parsed("LS_BTD_VERSION", cfg.daily.strategy_version)?;
    for line in apply_signal_and_cost_env(&mut cfg, |key| std::env::var(key).ok())? {
        println!("{line}");
    }

    let rt = tokio::runtime::Runtime::new()?;
    let result = rt.block_on(run(cfg, Utc::now()))?;
    // A trailing summary printed AFTER the engine logs, so the only operator-relevant
    // output never scrolls away under nautilus's ~8,900 INFO lines per session.
    print!("{}", daily_summary_block(&result));
    Ok(())
}

/// The candidate-selection knob: which ranking signal the run ranks under
/// (`RankingSignalKind`'s serde spelling — `placeholder`, `prior_turnover_desc`,
/// `momentum12x1`). Unset keeps [`DailyParams::default`]'s placeholder.
pub const SIGNAL_ENV: &str = "LS_BTD_SIGNAL";

/// The transaction-cost knob, shared with the ORB path by name and by loader: the
/// committed rate artifact (`lab/config/transaction-costs.json`) whose rates land in the
/// assembly params, which is where `finalize_daily_run` builds the cost model from and
/// what the manifest records as provenance. Unset keeps the zero-cost reproduction path.
pub const COST_CONFIG_ENV: &str = "LS_BT_COST_CONFIG";

/// Apply the two U4 knobs from an environment lookup (plan 2026-09-08-1215 U4): the
/// ranking signal and the transaction-cost artifact. Returns the operator lines to print.
///
/// Takes the lookup rather than reading the process environment so the wiring is testable
/// without mutating global state across a parallel test binary. A signal other than
/// [`FROZEN_RANKING_SIGNAL`] is not refused HERE — `DailyParams::validate` refuses it at
/// the same fail-fast point every other frozen term is checked, and a second copy of that
/// rule would be a second place to get it wrong.
///
/// # Errors
///
/// If the signal name is not one of the serde spellings, or the cost artifact fails to load.
pub fn apply_signal_and_cost_env(
    cfg: &mut DailyBacktestConfig,
    lookup: impl Fn(&str) -> Option<String>,
) -> anyhow::Result<Vec<String>> {
    let mut notes = Vec::new();
    if let Some(raw) = lookup(SIGNAL_ENV).filter(|s| !s.trim().is_empty()) {
        let kind = crate::runner::mount_universe::ranking_signal_from_name(&raw).ok_or_else(|| {
            anyhow::anyhow!(
                "{SIGNAL_ENV}={raw:?} is not a ranking signal; expected one of placeholder, \
                 prior_turnover_desc, momentum12x1 (the manifest spelling) — refusing rather \
                 than defaulting to the placeholder"
            )
        })?;
        cfg.daily.ranking_signal = kind;
        notes.push(format!(
            "ranking signal: {} (warmup {} prior bar(s); placeholder={})",
            kind.name(),
            kind.warmup_bars(),
            kind.is_placeholder()
        ));
    }
    if let Some(path) = lookup(COST_CONFIG_ENV).filter(|s| !s.trim().is_empty()) {
        let cost_cfg =
            crate::strategy::orb::TransactionCostConfig::load(Path::new(path.trim()))
                .map_err(|e| anyhow::anyhow!(e))?;
        cfg.params.cost_commission_rate_per_side = cost_cfg.commission_rate_per_side;
        cfg.params.cost_sell_tax_rate = cost_cfg.sell_tax_rate;
        notes.push(format!(
            "transaction costs armed: commission {}/side, sell tax {} (from {})",
            cost_cfg.commission_rate_per_side,
            cost_cfg.sell_tax_rate,
            path.trim()
        ));
    }
    Ok(notes)
}

/// The `lab-backtest-daily` trailing summary block.
#[must_use]
pub fn daily_summary_block(result: &DailyRunResult) -> String {
    let censored = result
        .outcome
        .positions
        .iter()
        .filter(|p| p.is_open())
        .count();
    format!(
        "\n=== lab-backtest-daily summary ===\nrun:       {}\nstrategy:  {} v{}\nsessions:  {}\n\
         positions: {} ({censored} open at range end)\nunopened:  {}\nre-gridded: {}\n\
         dir:       {}\n",
        result.run_id,
        result.manifest.strategy_id,
        result.manifest.strategy_version,
        result.outcome.selection.sessions.len(),
        result.outcome.positions.len(),
        result.outcome.unopened_entry_orders.len(),
        result.tick_regrids,
        result.run_dir.display(),
    )
}

/// Everything [`run_daily_blocking`] reads. A struct rather than nine positional
/// arguments behind a `clippy::too_many_arguments` allow, matching [`FinalizeDaily`]'s
/// reasoning: four of them are bare `f64`/`usize`/`u64` scalars in a row, which is a
/// call the compiler cannot tell apart from a transposed one.
struct DailyBlockingRun<'a, R: ?Sized> {
    instruments: &'a [InstrumentAny],
    all_bars: &'a [Bar],
    params: &'a OrbParams,
    sink: &'a DecisionSink,
    starting_balance: f64,
    target_m: usize,
    start_ns: u64,
    end_ns: u64,
    rank: &'a R,
    ranking_signal: Option<RankingSignalKind>,
    daily_params: &'a DailyParams,
}

/// The whole daily lifecycle on one blocking thread: index once, run the pure
/// selection phase, then drive the streaming engine phase.
fn run_daily_blocking<S, R, F>(
    run: DailyBlockingRun<'_, R>,
    make_strategy: F,
) -> anyhow::Result<DailyRunOutcome>
where
    S: DailyPathStrategy
        + Strategy
        + StrategyNative
        + DataActorNative
        + Component
        + std::fmt::Debug
        + 'static,
    R: Fn(&[UniverseCandidate]) -> Vec<String> + ?Sized,
    F: FnOnce(&[MountedSymbol]) -> S,
{
    let DailyBlockingRun {
        instruments,
        all_bars,
        params,
        sink,
        starting_balance,
        target_m,
        start_ns,
        end_ns,
        rank,
        ranking_signal,
        daily_params,
    } = run;
    let (daily_by_inst, daily_by_date) = index_daily(all_bars, start_ns, end_ns);
    let session_dates = session_dates_of(&daily_by_date);
    let selection = if let Some(kind) = ranking_signal {
        let next_session = std::cell::Cell::new(0usize);
        let governed_rank = |candidates: &[UniverseCandidate]| {
            let index = next_session.get();
            next_session.set(index + 1);
            let date = session_dates[index];
            let prior_closes: BTreeMap<String, Vec<f64>> = candidates
                .iter()
                .map(|candidate| {
                    let instrument_id = InstrumentId::from(candidate.symbol.as_str());
                    let closes = daily_by_inst
                        .get(&instrument_id)
                        .into_iter()
                        .flatten()
                        .filter(|bar| crate::runner::backtest::kst_date_of(bar) < date)
                        .map(|bar| bar.close.as_f64())
                        .collect::<Vec<_>>();
                    (candidate.symbol.clone(), closes)
                })
                .collect();
            let ranking = rank_by_signal(kind, candidates, &prior_closes);
            let session_ts = kst_to_unix_nanos(date, nautilus_ls::rules::KRX_REGULAR_OPEN)
                .expect("a selected session date converts to its KRX open")
                .as_u64();
            record_signal_decisions(
                sink,
                daily_params,
                session_ts,
                candidates,
                &prior_closes,
                &ranking,
            );
            ranking.ranked
        };
        // The governed ranker emitted the signal-aware records above. The generic
        // selection module receives a detached sink so it can remain byte-identical
        // for existing custom-ranker callers without duplicating those records.
        let detached_selection_sink = DecisionSink::new();
        select_from_index(
            instruments,
            &daily_by_inst,
            &session_dates,
            params,
            &detached_selection_sink,
            &governed_rank,
        )?
    } else {
        select_from_index(instruments, &daily_by_inst, &session_dates, params, sink, rank)?
    };

    let engine_out = engine_phase(
        instruments,
        &daily_by_date,
        &selection,
        target_m,
        starting_balance,
        make_strategy,
    )?;

    Ok(DailyRunOutcome {
        positions: engine_out.positions,
        selection,
        batches: engine_out.batches,
        duplicate_drops: engine_out.duplicate_drops,
        observed_position_ids: engine_out.observed_position_ids,
        entry_risks: engine_out.entry_risk_projection.risks(),
        unopened_entry_orders: engine_out.entry_risk_projection.unopened_entries().to_vec(),
    })
}

struct EnginePhase {
    positions: Vec<Position>,
    batches: Vec<SessionBatch>,
    duplicate_drops: Vec<DuplicateBarDrop>,
    observed_position_ids: Vec<PositionId>,
    entry_risk_projection: EntryRiskProjection,
}

/// Drive every in-range session through the engine.
fn engine_phase<S, F>(
    instruments: &[InstrumentAny],
    daily_by_date: &HashMap<NaiveDate, Vec<&Bar>>,
    selection: &DailySelection,
    target_m: usize,
    starting_balance: f64,
    make_strategy: F,
) -> anyhow::Result<EnginePhase>
where
    S: DailyPathStrategy
        + Strategy
        + StrategyNative
        + DataActorNative
        + Component
        + std::fmt::Debug
        + 'static,
    F: FnOnce(&[MountedSymbol]) -> S,
{
    let by_symbol: HashMap<String, InstrumentId> =
        instruments.iter().map(|i| (i.id().to_string(), i.id())).collect();

    // ONE engine for the whole stream (KTD1). `clear_data` resets only data-iteration
    // state and leaves the kernel cache intact, so positions survive across batches.
    // A hoisted engine with `reset()` between sessions was rejected: `reset` clears
    // the cache and defeats the purpose.
    let (mut engine, mounted) = build_engine(instruments, starting_balance)?;
    let strategy = make_strategy(&mounted);
    // Clone the open-position handle BEFORE `add_strategy` consumes the strategy
    // (KTD16) — the same shared-handle pattern `run_engine` uses for the entry-risk
    // ledger.
    let book = strategy.open_position_book();
    // Likewise the client-order-keyed entry-risk ledger (KTD3): the runner projects
    // it into cache-read order after the single post-`end()` read.
    let risk_ledger = strategy.entry_risk_ledger();
    // And the per-session signal handle (U4): the runner *writes* this one. The
    // ordered session calendar is published once, before the loop — a prospective
    // hold window is measured on it (R22).
    let signals = strategy.session_signals();
    signals.publish_sessions(selection.sessions.iter().map(|s| s.date).collect());
    engine.add_strategy(strategy)?;

    let mut batches: Vec<SessionBatch> = Vec::new();
    let mut duplicate_drops: Vec<DuplicateBarDrop> = Vec::new();
    let mut ran = 0usize;

    for (index, plan) in selection.sessions.iter().enumerate() {
        // The held set is engine state, read from the shared handle between batches —
        // never a per-session position-report read (R4).
        let held = book.held();
        let taken = resolve_take(&plan.ranked, &held, &by_symbol, target_m);
        let mut wanted: BTreeSet<InstrumentId> = held.clone();
        wanted.extend(taken.iter().copied());

        // Publish what this session resolved BEFORE its batch runs, so the strategy's
        // first bar callback of the session already sees the session ordinal, the
        // take, and the prior ATRs its stop gate needs (KTD9).
        signals.publish_session(DailySessionContext {
            index,
            date: plan.date,
            ranked: plan
                .ranked
                .iter()
                .filter_map(|s| by_symbol.get(s.as_str()).copied())
                .collect(),
            taken: taken.clone(),
            held: held.iter().copied().collect(),
            prior_atr: plan
                .prior_atr
                .iter()
                .filter_map(|(s, a)| by_symbol.get(s.as_str()).map(|id| (*id, *a)))
                .collect(),
        });

        let batch = build_batch(
            daily_by_date.get(&plan.date).map(|v| v.as_slice()).unwrap_or_default(),
            &wanted,
            plan.date,
            &mut duplicate_drops,
        );

        // Fail closed on a data gap inside a HELD position's series. Both the stop and
        // the hold-expiry exit fire from `DataActor::on_bar`, so a held symbol that
        // contributes no bar to this session gets no callback for that position: its
        // exit slides to whichever later session does deliver a bar, or never fires and
        // the position ends censored. Either way the hold runs past the PRE-REGISTERED
        // `holding_period_sessions` and the run still finalizes green, which moves the
        // frozen verdict statistic (`Σ realized_pnl / Σ risk_capital`) under a term that
        // is not supposed to be able to move.
        //
        // The alternative policies were considered and rejected: carrying the position
        // silently is the current bias, and flattening at a last-known price invents an
        // execution the daily-resolution observer never saw. Neither may be chosen
        // implicitly, so the gap becomes the operator's problem here rather than a
        // number in a green run.
        //
        // This runs over the HELD set, before the empty-batch skip below: the empty
        // batch is not the only path, because a batch can be non-empty from other names
        // while the held one is absent (`build_batch` emits only what `session_bars`
        // contains). Entry-session absence is deliberately not covered — a taken symbol
        // with no bar simply never opens a position, so no frozen term is in flight.
        let present: BTreeSet<InstrumentId> =
            batch.iter().map(|b| b.bar_type.instrument_id()).collect();
        let missing: Vec<InstrumentId> =
            held.iter().copied().filter(|id| !present.contains(id)).collect();
        if !missing.is_empty() {
            return Err(
                DailyEngineError::HeldSymbolMissingBar { date: plan.date, missing }.into()
            );
        }

        let bars = batch.len();
        let mut record = SessionBatch {
            date: plan.date,
            held: held.into_iter().collect(),
            taken,
            bars,
            skipped: true,
        };

        // Skip the whole cycle on an empty batch — `add_data` errors on an empty slice.
        if batch.is_empty() {
            batches.push(record);
            continue;
        }
        record.skipped = false;

        // `clear_data` is the documented streaming step and is what makes the `None`
        // bounds below resolve to *this* batch's own range. Note for future readers:
        // it is NOT what carries positions across sessions — `add_data` opens a fresh
        // named stream with its own cursor each call, so an exhausted batch is never
        // re-delivered even without it. Its load-bearing effects are the batch-scoped
        // `ts_first`/`ts_last_data` and not retaining every batch's `Vec<Data>` for the
        // whole 837-session run.
        engine.clear_data();
        // `sort = true` on EVERY call: `self.sorted = sort` is an assignment, not an
        // OR, so the last `add_data` of a batch decides whether `run` accepts it.
        engine.add_data(batch.into_iter().map(Data::Bar).collect(), None, true, true)?;
        // `None` bounds, NOT the pinned range: `run_impl` sets `last_ns = start_ns` and
        // calls `set_all_clocks_time` unconditionally *before* the `iteration == 0`
        // gate, so passing the range on every batch would rewind every component clock
        // each time. `clear_data` nulls `ts_first`/`ts_last_data` and `add_data`
        // recomputes them per batch, so `None` resolves to this batch's own bounds.
        // Range pinning lives in which bars are added, not in these arguments.
        engine.run(None, None, None, true)?;
        ran += 1;

        // If the first batch processes zero items the first-iteration block re-runs on
        // the next call — a new run id, a new `backtest_start`, and an
        // `initialize_account()` mid-stream.
        if ran == 1 && engine.iteration() == 0 {
            return Err(DailyEngineError::FirstBatchNoIteration { date: plan.date }.into());
        }
        // A `force_stop` calls `end()` even under `streaming = true`, and because
        // `iteration != 0` no later `run()` restarts the trader: the loop would route
        // later sessions' bars into a stopped trader and finish green over a truncated
        // position set. Abort with a typed error instead.
        if let Some(finished) = engine.run_finished() {
            return Err(DailyEngineError::RunFinishedMidStream {
                date: plan.date,
                finished_ns: finished.as_u64(),
            }
            .into());
        }

        batches.push(record);
    }

    // Finalize the streaming run, then read the cache EXACTLY ONCE (R4). Guarded on at
    // least one batch: with zero batches the trader was never started (the
    // first-iteration block never ran), so there is nothing to end.
    if ran > 0 {
        engine.end();
    }
    let positions: Vec<Position> = engine
        .kernel()
        .cache
        .borrow()
        .positions(None, None, None, None, None)
        .into_iter()
        .map(|p| p.cloned())
        .collect();

    // Project the client-order-keyed ledger into THAT read's order and reconcile it
    // (KTD3). The projection is built here, against the one cache read that defines
    // position order — never in ledger order, which passes both count checks while
    // attaching every risk to the wrong position.
    let entry_risk_projection = project_entry_risks(&positions, &risk_ledger);
    entry_risk_projection.assert_aligned(&positions);

    Ok(EnginePhase {
        positions,
        batches,
        duplicate_drops,
        observed_position_ids: book.opened_position_ids(),
        entry_risk_projection,
    })
}

/// Build the engine and mount every instrument up front (KTD11), returning the
/// mounted universe paired with each instrument's daily bar type (R2).
///
/// The venue is `OmsType::Hedging` (KTD12). Under `Netting` the position id is
// ---------------------------------------------------------------------------
// Historical price grid (found by the R30 probe, 2026-09-08)
// ---------------------------------------------------------------------------

/// One instrument whose `price_increment` moved for a historical run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickRegrid {
    /// The instrument re-gridded.
    pub instrument_id: InstrumentId,
    /// The increment the adapter's master mapping produced (today's exchange tick).
    pub from_increment: i64,
    /// The increment this run mounts instead.
    pub to_increment: i64,
}

/// Re-grid each instrument's `price_increment` so every in-range bar price the engine
/// will see sits ON that grid.
///
/// # The defect this closes
///
/// nautilus's matching engine runs every fill price through
/// `normalize_price_for_current_instrument` and, when the price is not a multiple of
/// the instrument's `price_increment`, logs `Skipping fill …` at WARN and leaves the
/// order **unfilled**. The run exits 0 with a clean data-quality report and a healthy
/// summary. The R30 probe measured it over 2016-08-01..2019-12-31: 7,981 fills skipped
/// across 136 of 286 symbols, 5,167 entry orders never opened, `target_m = 8`
/// delivering 1.57 entries per session — and the fills that DID land were the subset
/// whose price happened to divide the mounted increment, a price-correlated subsample
/// rather than a random one.
///
/// # Why the adapter's increment cannot serve a historical run
///
/// Two independent reasons, and the second is the one that decides the fix.
///
/// 1. `nautilus_ls::instruments::map_equity` derives `price_increment` ONCE, from the
///    master row's *current* reference price under a hardcoded `TickRegime::Post2023`.
///    That is correct for live order placement — today's order is priced in today's
///    band under today's ladder — and this function deliberately leaves it alone. It is
///    simply the wrong band for a 2016 price: `000660` closed at 33,550 on a 50 KRW
///    grid and carries today's 1,000 KRW tick.
/// 2. **The catalog is adjustment-adjusted** (`checkpoint.adjusted_prices == true`), so
///    its prices lie on no exchange tick grid at all. `005930`'s 2016 close reads 30,340
///    — its pre-split price divided through by the 2018 50:1 split — and 30,340 is not a
///    multiple of 50, the tick that actually governed it. An effective-dated ladder
///    lookup (`TickRegime::for_date`, which exists and is correct) therefore does NOT
///    fix this: it would still refuse three of every five real prices. Adjustment
///    destroys the exchange grid, and no ladder can describe the series afterwards.
///
/// # What it mounts instead
///
/// Per symbol, `gcd(g, f)` where `g` is the GCD of every in-range OHLC price and `f` is
/// the adapter's increment. That divides every price the engine will see, so **no fill
/// can be skipped, by construction** rather than by a runtime check; it is never coarser
/// than `f` (when `f` already divides everything, `gcd(g, f) == f` and nothing moves);
/// and for a symbol untouched by a corporate action it recovers the real exchange tick
/// rather than collapsing to 1. Nothing is rounded and no price is invented: the daily
/// path submits market orders and fills at real catalog bar prices, so the increment is
/// only a fill-price *validator* here.
///
/// # Why run-scoped rather than per session
///
/// nautilus 0.60 cannot swap an instrument mid-run:
/// `SimulatedExchange::add_instrument` on an existing id constructs a **new**
/// `OrderMatchingEngine` and inserts it over the old one, discarding that engine's book
/// and state, and derives `raw_id` from `self.instruments.len()`, which does not grow on
/// a replace — so every symbol swapped in one session would collide on one raw id. One
/// run-scoped increment per symbol needs none of that.
///
/// Out-of-range bars are ignored: the engine never sees them, so letting a pre-range
/// lookback bar pull the grid finer would weaken the increment for no gain. A symbol
/// with no in-range bars trades nothing and is passed through untouched.
#[must_use]
pub fn regrid_instruments_for_range(
    instruments: &[InstrumentAny],
    bars: &[Bar],
    start_ns: u64,
    end_ns: u64,
) -> (Vec<InstrumentAny>, Vec<TickRegrid>) {
    let mut price_gcd: HashMap<InstrumentId, i64> = HashMap::new();
    for b in bars {
        if !crate::runner::backtest::is_daily(b) {
            continue;
        }
        let ts = b.ts_event.as_u64();
        if ts < start_ns || ts > end_ns {
            continue;
        }
        let slot = price_gcd.entry(b.bar_type.instrument_id()).or_insert(0);
        for p in [b.open, b.high, b.low, b.close] {
            *slot = gcd(*slot, p.as_f64() as i64);
        }
    }

    let mut regridded = Vec::with_capacity(instruments.len());
    let mut moves = Vec::new();
    for inst in instruments {
        let id = inst.id();
        let from = inst.price_increment().as_f64() as i64;
        let to = match price_gcd.get(&id) {
            Some(&g) if g > 0 && from > 0 => gcd(g, from),
            // No in-range bars, or a degenerate price/increment: leave it alone.
            _ => from,
        };
        match inst {
            InstrumentAny::Equity(e) if to != from && to > 0 => {
                let mut e = e.clone();
                e.price_increment = Price::from(to.to_string().as_str());
                regridded.push(InstrumentAny::Equity(e));
                moves.push(TickRegrid { instrument_id: id, from_increment: from, to_increment: to });
            }
            _ => regridded.push(inst.clone()),
        }
    }

    moves.sort_by_key(|m| m.instrument_id);
    (regridded, moves)
}

/// Greatest common divisor, with `gcd(0, n) == n` so it folds over a price stream.
fn gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// `{instrument_id}-{strategy_id}` — one constant id per symbol for the whole run —
/// so re-entering a symbol takes the `reopen_position` path, which snapshots the
/// closed position out of the live index that `cache.positions()` reads. Every
/// earlier round trip would disappear from the run with no diagnostic, and
/// concurrent legs would merge, destroying the entry-fixed stop.
fn build_engine(
    instruments: &[InstrumentAny],
    starting_balance: f64,
) -> anyhow::Result<(BacktestEngine, Vec<MountedSymbol>)> {
    let mut engine = BacktestEngine::new(BacktestEngineConfig {
        bypass_logging: true,
        ..Default::default()
    })?;
    engine.add_venue(
        SimulatedVenueConfig::builder()
            .venue(Venue::from(nautilus_ls::KRX_VENUE))
            .oms_type(OmsType::Hedging)
            .account_type(AccountType::Margin)
            .base_currency(Currency::KRW())
            .book_type(BookType::L1_MBP)
            .starting_balances(vec![Money::new(starting_balance, Currency::KRW())])
            .build()
            .map_err(|e| anyhow::anyhow!("venue build: {e}"))?,
    )?;
    let mut mounted = Vec::with_capacity(instruments.len());
    for inst in instruments {
        engine.add_instrument(inst)?;
        mounted.push(MountedSymbol {
            instrument_id: inst.id(),
            bar_type: BarKind::Daily.bar_type(inst.id()).map_err(|e| anyhow::anyhow!(e))?,
        });
    }
    Ok((engine, mounted))
}

/// R10: take the top `target_m` of the ranked list **from those not already held**.
/// The already-held exclusion runs first — a held symbol is `Excluded`, and the
/// remaining ranked symbols past `target_m` are `Passed`.
fn resolve_take(
    ranked: &[String],
    held: &BTreeSet<InstrumentId>,
    by_symbol: &HashMap<String, InstrumentId>,
    target_m: usize,
) -> Vec<InstrumentId> {
    ranked
        .iter()
        .filter_map(|s| by_symbol.get(s.as_str()).copied())
        .filter(|id| !held.contains(id))
        .take(target_m)
        .collect()
}

/// Build one session's batch: **every symbol with an open position plus that
/// session's newly taken symbols** (step 4). A held position must receive its daily
/// bar on every session of its hold or the venue cannot price it and the stop never
/// evaluates — this function can only emit what `session_bars` actually holds, so
/// that requirement is *enforced* by the [`DailyEngineError::HeldSymbolMissingBar`]
/// check in [`engine_phase`], against this function's output.
///
/// Deduped to one bar per instrument per distinct `ts_event` (R23), recording each
/// drop and whether it was value-divergent. The first copy in catalog order wins.
fn build_batch(
    session_bars: &[&Bar],
    wanted: &BTreeSet<InstrumentId>,
    date: NaiveDate,
    drops: &mut Vec<DuplicateBarDrop>,
) -> Vec<Bar> {
    let mut kept: BTreeMap<(InstrumentId, u64), Bar> = BTreeMap::new();
    for b in session_bars {
        let id = b.bar_type.instrument_id();
        if !wanted.contains(&id) {
            continue;
        }
        let key = (id, b.ts_event.as_u64());
        match kept.get(&key) {
            None => {
                kept.insert(key, (*b).clone());
            }
            Some(existing) => drops.push(DuplicateBarDrop {
                date,
                instrument_id: id,
                ts_event: b.ts_event.as_u64(),
                divergent: !bars_equal(existing, b),
            }),
        }
    }
    kept.into_values().collect()
}

/// Whether two bars carry identical OHLCV (the divergence test for a dropped
/// duplicate — a differing copy is an adjustment-basis conflict, not a benign
/// re-ingest overlap).
fn bars_equal(a: &Bar, b: &Bar) -> bool {
    a.open == b.open
        && a.high == b.high
        && a.low == b.low
        && a.close == b.close
        && a.volume == b.volume
}

fn in_range(b: &Bar, start_ns: u64, end_ns: u64) -> bool {
    let ts = b.ts_event.as_u64();
    ts >= start_ns && ts <= end_ns
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

#[cfg(test)]
mod failure_classification_tests {
    use super::*;
    use crate::artifacts::aborted_runs;

    #[test]
    fn abnormal_engine_failure_keeps_the_aborted_run_marker() {
        let data_home = tempfile::tempdir().unwrap();
        let run_id = "classified-engine-abort";
        let writer = RunWriter::new(data_home.path(), run_id).unwrap();
        let _emitter = writer.stream_decisions().unwrap();
        let failure = Err(DailyRunFailure::Aborted(anyhow::anyhow!(
            "engine failed after blocking work began"
        )));

        assert!(resolve_locked_run(writer, failure).is_err());
        assert_eq!(aborted_runs(data_home.path()), vec![run_id.to_string()]);
    }
}
