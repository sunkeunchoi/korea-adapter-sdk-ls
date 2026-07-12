//! Backtest runner (U5, F1) — one command runs ORB vN over the catalog and lands a
//! registry run. Loads instruments + bars for the pinned range from the
//! `ParquetDataCatalog`, then drives **every** in-range trading session: per session it
//! reselects the universe from that day's prior/today daily bars, mounts a fresh ORB
//! `OrbStrategy` in a fresh `BacktestEngine` (a structural per-day reset) over that
//! day's minute bars, and folds the positions into one union ledger + one RunWriter run.
//!
//! Guards (KTD2/KTD8): the runner refuses to start while the ingest advisory lock is
//! held (and holds it for the run so ingest cannot mutate the catalog mid-run), and
//! re-reads the range-scoped catalog fingerprint at finalize — failing the run with no
//! registry residue if it changed since start.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use nautilus_backtest::config::{BacktestEngineConfig, SimulatedVenueConfig};
use nautilus_backtest::engine::BacktestEngine;
use nautilus_ls::ingest::checkpoint::Checkpoint;
use nautilus_ls::ingest::{kst_to_unix_nanos, read_all_bars, read_all_instruments, BarKind};
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_ls::rules::KRX_REGULAR_OPEN;
use nautilus_model::data::{Bar, Data};
use nautilus_model::enums::{AccountType, BarAggregation, BookType, OmsType};
use nautilus_model::identifiers::{InstrumentId, Venue};
use nautilus_model::instruments::{Instrument, InstrumentAny};
use nautilus_model::position::Position;
use nautilus_model::types::{Currency, Money};

use nautilus_ls::reference::universe_metadata::{
    assign_liquidity_tier, stratum_of, ConditionerTags, InstrumentMetadata, Resolved, Stratum,
    UniverseMetadata,
};

use crate::artifacts::data_quality::{
    CoverageGapRecord, DataQualityReport, GapReasonKind, TierCompositionEntry,
};
use crate::artifacts::manifest::{
    hash_bytes, range_fingerprint, universe_sequence_hash, DataRange, Manifest,
};
use crate::artifacts::performance::{EntryRisk, PerformanceReport};
use crate::artifacts::{run_id, RunSource, RunWriter};
use crate::agent::sink::DecisionSink;
use crate::params::OrbParams;
use crate::strategy::orb::{
    kst_time_from_nanos, select_universe, CandidateMeta, OrbStrategy, SelectedSymbol,
    UniverseCandidate,
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
    /// Optional `UniverseMetadata` artifact path (plan 2026-07-10-003, U4/R10):
    /// when set, `build_candidates` joins each candidate to its record, the
    /// selection gates on tradability + the liquidity floor, accepts carry the
    /// R9 conditioner tags, and the artifact's content hash is stamped into the
    /// manifest (KTD2). `None` preserves legacy (metadata-less) behavior.
    pub metadata_path: Option<PathBuf>,
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
            metadata_path: None,
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
    /// The Turn-N per-tier count summary for a metadata-driven run (U6):
    /// per-tier trade counts + the pre-check verdict, computed from the decision
    /// stream — **never** from `performance.json` (the KTD5 staging guard).
    /// `None` for a legacy run.
    pub tier_summary: Option<Vec<String>>,
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
    // Fail fast on an invalid gate configuration (KTD10) — before any catalog
    // work, so a misconfigured cutoff never lands a partial run.
    cfg.params
        .validate()
        .map_err(|e| anyhow::anyhow!("invalid ORB gate configuration: {e}"))?;

    let catalog_path = cfg.data_home.join("catalog");
    if !catalog_path.exists() {
        anyhow::bail!("no catalog at {} — ingest first", catalog_path.display());
    }

    // Own-guard: refuse if ingest is running, and hold the ingest lock so the catalog
    // cannot be mutated mid-run (KTD2). Released on drop / at end of the run.
    let _guard = AdvisoryLock::acquire(&catalog_path, LockKind::Ingest)
        .map_err(|e| anyhow::anyhow!("backtest refused — ingest/live in progress: {e}"))?;

    // Metadata-driven run (U4, plan 2026-07-10-003): load + validate the
    // artifact and index its records by shcode. The content hash pins the
    // manifest (KTD2) so the per-tier report can assert the ingest and the
    // backtest read the same artifact.
    let metadata: Option<(String, HashMap<String, InstrumentMetadata>)> = match &cfg.metadata_path {
        Some(path) => {
            let artifact = UniverseMetadata::load(path).map_err(|e| anyhow::anyhow!(e))?;
            artifact.validate().map_err(|errs| {
                anyhow::anyhow!("metadata artifact failed validation:\n  - {}", errs.join("\n  - "))
            })?;
            let hash = artifact.content_hash();
            let map = artifact.records.into_iter().map(|r| (r.shcode.clone(), r)).collect();
            Some((hash, map))
        }
        None => None,
    };

    // KTD2 at the runner, not only in `report tiers` (review finding): the
    // runner's own trailing summary prints the GREEN/RED pre-check, so it must
    // not green-light an artifact the ingest never pinned. A mismatched pin is
    // fatal; an absent pin (a catalog that never had a stratified ingest) is
    // surfaced as a loud warning on the summary instead.
    let mut pin_warning: Option<String> = None;
    if let Some((hash, _)) = &metadata {
        use nautilus_ls::reference::universe_metadata::MetadataPin;
        match MetadataPin::load(&catalog_path) {
            Ok(Some(pin)) if &pin.content_hash != hash => anyhow::bail!(
                "metadata artifact hash mismatch (KTD2): the catalog's ingest pin holds {} but \
                 LS_BT_METADATA resolves to {} — a re-capture between ingest and backtest \
                 re-tiers symbols; re-ingest and re-run against ONE artifact",
                pin.content_hash,
                hash
            ),
            Ok(Some(_)) => {}
            Ok(None) => {
                pin_warning = Some(
                    "(no ingest pin in the catalog — the artifact-to-ingest handshake is \
                     unverified; run `report tiers` before acting on these counts)"
                        .to_string(),
                )
            }
            Err(e) => anyhow::bail!("reading the catalog metadata pin: {e}"),
        }
    }

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

    // The one decision sink accumulates every session's universe + trade envelopes
    // into a single stream (KTD-6); it is drained after the loop for decisions.jsonl.
    let sink = DecisionSink::new();

    // Drive every in-range session (R1): for each session date the universe is
    // reselected from that day's prior/today daily bars (R2), a fresh strategy trades
    // that session's minute bars (R3, structural per-day reset), and positions fold
    // into one ledger. The whole loop runs on ONE blocking-pool thread (the engine
    // drives an internal `block_on` — the documented catalog/engine `spawn_blocking`
    // gotcha), so N sequential fresh engines are exercised same-thread (KTD-1).
    let instruments_for_loop = instruments.clone();
    let all_bars_for_loop = all_bars.clone();
    let params = cfg.params.clone();
    let sink_for_loop = sink.clone();
    let starting_balance = cfg.starting_balance;
    let minute_step = cfg.minute_step;
    let metadata_for_loop = metadata.as_ref().map(|(_, map)| map.clone());
    let loop_out: SessionLoop = tokio::task::spawn_blocking(move || {
        run_sessions(
            &instruments_for_loop,
            &all_bars_for_loop,
            &params,
            &sink_for_loop,
            starting_balance,
            minute_step,
            start_ns,
            end_ns,
            metadata_for_loop.as_ref(),
        )
    })
    .await??;

    // Test hook: simulate any mid-run catalog mutation before the finalize re-check.
    before_finalize.await;

    // Re-check the fingerprint at finalize: a mid-run catalog mutation invalidates the
    // run (no registry residue, KTD8). The guard spans the whole session loop.
    let all_bars_end = read_all_bars(&catalog_path).await?;
    let fingerprint_end = range_fingerprint(&all_bars_end, start_ns, end_ns);
    if fingerprint_end != fingerprint_start {
        anyhow::bail!("catalog changed in-range during the run — aborting with no registry residue");
    }

    // Assemble artifacts over the union ledger.
    let checkpoint = load_checkpoint(&catalog_path);
    let performance = PerformanceReport::from_positions_with_risk(
        &loop_out.positions,
        &loop_out.position_risks,
        cfg.starting_balance,
    );
    let selected_union = loop_out.selected_union.clone();
    // R7: report DETECTED per-symbol shifts — the checkpoint's unhealed shifted
    // marks intersected with this run's selected universe (the union across
    // sessions). A clean catalog reports an empty list; the agent discounts only
    // affected runs.
    let shift_symbols: Vec<String> = checkpoint
        .as_ref()
        .map(|c| {
            c.shifted_instruments("1-DAY")
                .into_iter()
                .filter(|s| selected_union.contains(s))
                .collect()
        })
        .unwrap_or_default();
    let mut data_quality = DataQualityReport::backtest(selected_union.clone(), shift_symbols);
    data_quality.coverage_gaps = collect_gaps(checkpoint.as_ref(), &loop_out.missing);

    // U6: per-tier composition + the Turn-N count summary for a metadata-driven
    // run — symbols attributed via the artifact, trades via the tagged accept
    // join over the decision stream. Counts only; `performance.json` is written
    // unchanged below but never read for this summary (the KTD5 staging guard).
    let (tier_composition, tier_summary) = match &metadata {
        Some((_, map)) => {
            let mut symbol_counts: std::collections::BTreeMap<Stratum, usize> =
                Stratum::ALL.iter().map(|s| (*s, 0)).collect();
            for sym in &selected_union {
                if let Some(rec) = map.get(shcode_of(sym)) {
                    *symbol_counts
                        .get_mut(&stratum_of(rec.market_class, rec.cap_tier))
                        .expect("all strata keyed") += 1;
                }
            }
            let envelopes = sink.snapshot();
            let (trade_counts, untagged) = crate::runner::report::tier_trade_counts(&envelopes);
            let entries: Vec<TierCompositionEntry> = Stratum::ALL
                .iter()
                .map(|s| TierCompositionEntry {
                    stratum: s.label().to_string(),
                    symbols: symbol_counts[s] as u64,
                    trades: trade_counts[s] as u64,
                })
                .collect();
            let mut lines = crate::runner::report::tier_summary_lines(
                "selected",
                &symbol_counts,
                &trade_counts,
                untagged,
            );
            if let Some(w) = &pin_warning {
                lines.push(w.clone());
            }
            (Some(entries), Some(lines))
        }
        None => (None, None),
    };
    data_quality.tier_composition = tier_composition;

    let rid = run_id(start, RunSource::Backtest, &cfg.params.strategy_id, cfg.params.strategy_version);
    let manifest = Manifest {
        run_id: rid.clone(),
        source: RunSource::Backtest,
        strategy_id: cfg.params.strategy_id.clone(),
        strategy_version: cfg.params.strategy_version,
        params: cfg.params.clone(),
        data_range: cfg.range.clone(),
        catalog_fingerprint: fingerprint_start,
        // KTD-5: sequence-sensitive hash over the per-session selection sequence.
        universe_hash: universe_sequence_hash(&loop_out.selection_sequence),
        strategy_code_hash: crate::artifacts::manifest::strategy_code_hash(),
        checkpoint_hash: checkpoint_hash(&catalog_path),
        universe_metadata_hash: metadata.as_ref().map(|(hash, _)| hash.clone()),
        created_utc: start.to_rfc3339(),
    };

    let writer = RunWriter::new(&cfg.data_home, &rid)?;
    writer.write_manifest(&manifest)?;
    writer.write_performance(&performance)?;
    writer.write_data_quality(&data_quality)?;
    writer.write_decisions(&sink.snapshot())?;
    let run_dir = writer.finalize()?;

    Ok(RunOutcome { run_dir, run_id: rid, tier_summary })
}

/// The union outcome of driving every in-range session (KTD-1/KTD-6): the folded
/// position ledger, the chronological per-session selection sequence (for the
/// sequence-sensitive `universe_hash`, KTD-5), the deduped selected-symbol union
/// (the universe snapshot + shift-mark intersection), and the coverage-gap symbols.
struct SessionLoop {
    positions: Vec<Position>,
    /// Entry-fixed risk per position, index-aligned with `positions` (U1) — the
    /// join input for `risk_capital`/`realized_r` at ledger assembly.
    position_risks: Vec<Option<EntryRisk>>,
    selection_sequence: Vec<(NaiveDate, Vec<String>)>,
    selected_union: Vec<String>,
    missing: Vec<String>,
}

/// Drive every in-range trading session, reselecting the universe and resetting
/// per-symbol state each day (R1/R2/R3). For each distinct in-range daily date:
/// build that session's candidates (KTD-3), select its universe into the shared
/// sink with a session-scoped `ts_event`, run a fresh engine + `OrbStrategy` over
/// that day's minute bars, and fold the positions forward. Runs on the blocking
/// pool (the engine's internal `block_on`), so all sessions execute sequentially on
/// one thread — the same-thread engine-independence case KTD-1 gates on.
#[allow(clippy::too_many_arguments)]
fn run_sessions(
    instruments: &[InstrumentAny],
    all_bars: &[Bar],
    params: &OrbParams,
    sink: &DecisionSink,
    starting_balance: f64,
    minute_step: u32,
    start_ns: u64,
    end_ns: u64,
    metadata: Option<&HashMap<String, InstrumentMetadata>>,
) -> anyhow::Result<SessionLoop> {
    // Index the catalog ONCE (one pass each), so the per-session loop does no repeated
    // full-catalog scans: daily bars bucketed per instrument (sorted by ts, for the
    // prior/today lookup + the noise filter) and in-range minute bars bucketed by KST
    // date (the per-bar KST conversion runs exactly once here, not once per session).
    let mut daily_by_inst: HashMap<InstrumentId, Vec<&Bar>> = HashMap::new();
    let mut minute_by_date: HashMap<NaiveDate, Vec<&Bar>> = HashMap::new();
    for b in all_bars {
        if is_daily(b) {
            daily_by_inst.entry(b.bar_type.instrument_id()).or_default().push(b);
        } else if is_minute(b) && in_range(b, start_ns, end_ns) {
            minute_by_date.entry(kst_date_of(b)).or_default().push(b);
        }
    }
    for bars in daily_by_inst.values_mut() {
        bars.sort_by_key(|b| b.ts_event.as_u64());
    }

    // Opening-window volume per (instrument, session date) — one pass over the
    // in-range minute index (KTD9). The window is the OR window itself
    // (`[range_open, range_end)`, the same predicate `OrbState` accumulates the
    // range over), so today's live opening-window volume the RVOL gate compares
    // against is measured identically. The RVOL baseline for a candidate is the
    // mean of these over its prior in-range sessions; building it here keeps the
    // per-session loop free of a re-scan (U2 execution note).
    let range_open = params.range_open;
    let range_end = params.range_end();
    let mut open_vol_by_inst: HashMap<InstrumentId, BTreeMap<NaiveDate, f64>> = HashMap::new();
    for (date, bars) in &minute_by_date {
        for b in bars {
            let t = kst_time_from_nanos(b.ts_event.as_u64());
            if t >= range_open && t < range_end {
                *open_vol_by_inst
                    .entry(b.bar_type.instrument_id())
                    .or_default()
                    .entry(*date)
                    .or_insert(0.0) += b.volume.as_f64();
            }
        }
    }

    // The distinct in-range daily session dates — the tradeable sessions, in order.
    // Each day's minute bars are pinned to the range; the prior-daily lookback may
    // reach one session BEFORE the range (KTD-3). That backward reach is safe against
    // the accumulate-forward comparability break the range-scoped scan guarded (forward
    // growth only appends dates AFTER the range, never a prior for any in-range
    // session); a heal/backfill of the pre-range prior daily instead surfaces through
    // `universe_hash` (the per-session selection sequence, KTD-5), which `runs compare`
    // keys on alongside the range-scoped `catalog_fingerprint`.
    let mut session_dates: Vec<NaiveDate> = daily_by_inst
        .values()
        .flat_map(|bars| bars.iter())
        .filter(|b| in_range(b, start_ns, end_ns))
        .map(|b| kst_date_of(b))
        .collect();
    session_dates.sort();
    session_dates.dedup();

    // The instruments that carry ANY daily bar (the gap-report noise filter, KTD5):
    // a never-ingested symbol (no daily bars anywhere) must never surface as a
    // spurious coverage gap. Derived from the daily bucket's keys.
    let daily_symbols: BTreeSet<String> = instruments
        .iter()
        .filter(|inst| daily_by_inst.contains_key(&inst.id()))
        .map(|inst| inst.id().to_string())
        .collect();

    let mut positions: Vec<Position> = Vec::new();
    let mut position_risks: Vec<Option<EntryRisk>> = Vec::new();
    let mut selection_sequence: Vec<(NaiveDate, Vec<String>)> = Vec::new();
    let mut selected_union: BTreeSet<String> = BTreeSet::new();
    let mut ever_candidate: BTreeSet<String> = BTreeSet::new();

    for date in &session_dates {
        // The universe scan is a session-open state change — stamp its envelopes at
        // this session's open so decisions.jsonl carries one scan per session date.
        let session_ts = kst_to_unix_nanos(*date, KRX_REGULAR_OPEN)?.as_u64();

        let candidates =
            build_candidates(instruments, &daily_by_inst, &open_vol_by_inst, params, *date, metadata);
        for c in &candidates {
            ever_candidate.insert(c.symbol.clone());
        }
        // The derived per-symbol ATR / RVOL baseline, keyed by symbol so the
        // selected-symbol seam can thread it to the strategy (U2). Selection
        // itself reads neither value (R4) — computed on every candidate,
        // consumed only for the selected ones.
        let derived: HashMap<String, (Option<f64>, Option<f64>)> = candidates
            .iter()
            .map(|c| (c.symbol.clone(), (c.prior_atr, c.prior_open_vol_mean)))
            .collect();
        let selected_symbols = select_universe(&candidates, params, sink, session_ts);
        for s in &selected_symbols {
            selected_union.insert(s.clone());
        }
        selection_sequence.push((*date, selected_symbols.clone()));

        // Resolve the selected symbols to instrument ids + minute bar types,
        // threading each symbol's ATR / RVOL baseline onto its SelectedSymbol.
        let selected: Vec<SelectedSymbol> = instruments
            .iter()
            .filter(|i| selected_symbols.iter().any(|s| s == &i.id().to_string()))
            .filter_map(|i| {
                BarKind::Minute(minute_step).bar_type(i.id()).ok().map(|bar_type| {
                    let (prior_atr, prior_open_vol_mean) =
                        derived.get(&i.id().to_string()).copied().unwrap_or((None, None));
                    SelectedSymbol { instrument_id: i.id(), bar_type, prior_atr, prior_open_vol_mean }
                })
            })
            .collect();

        // This session's minute bars only (from the pre-bucketed by-date index), kept
        // to the selected symbols.
        let minute_bars: Vec<Bar> = minute_by_date
            .get(date)
            .map(|bars| {
                bars.iter()
                    .filter(|b| selected.iter().any(|s| s.bar_type == b.bar_type))
                    .map(|b| (*b).clone())
                    .collect()
            })
            .unwrap_or_default();

        // Nothing selected or no minute bars → no trades this session; skip the engine
        // construction entirely (the universe scan is already recorded above).
        if selected.is_empty() || minute_bars.is_empty() {
            continue;
        }

        // A fresh engine + `OrbStrategy` (hence fresh `OrbState`s) per session is the
        // structural per-day reset (R3/KTD-2): a symbol that reached `Done` yesterday
        // starts clean today. Only the selected instruments are mounted — they are the
        // only ones with data/orders this session.
        let selected_instruments: Vec<InstrumentAny> = instruments
            .iter()
            .filter(|i| selected.iter().any(|s| s.instrument_id == i.id()))
            .cloned()
            .collect();
        let (session_positions, session_risks) = run_engine(
            selected_instruments,
            minute_bars,
            params.clone(),
            selected,
            sink.clone(),
            starting_balance,
        )?;
        positions.extend(session_positions);
        position_risks.extend(session_risks);
    }

    // Coverage-gap symbols (KTD5, R8; U2 "no spurious global gap"): a symbol with
    // daily bars that NEVER formed a valid (prior, today) candidate on any in-range
    // session. A symbol missing its prior daily on one day but selected on another is
    // NOT a gap — it was tradeable.
    let missing: Vec<String> = daily_symbols.difference(&ever_candidate).cloned().collect();

    Ok(SessionLoop {
        positions,
        position_risks,
        selection_sequence,
        selected_union: selected_union.into_iter().collect(),
        missing,
    })
}

/// Select `(prior, today)` for a target `session_date` from a `ts_event`-sorted
/// daily slice (KTD-3): `today` is the latest daily bar dated exactly on the
/// session, `prior` is the latest daily bar dated strictly before it — which may be
/// the session immediately before the pinned range (the first-session lookback).
/// `None` when either is absent. Keying on the KST date (not raw index) keeps the
/// scan robust to a same-session duplicate a re-ingest overlap can leave in the
/// catalog: `read_all_bars` drops byte-identical duplicates but deliberately keeps a
/// value-divergent same-session bar (an un-healed adjustment-basis overlap); because
/// `prior` is strictly an earlier date than `today`, the two can never collapse onto
/// one session (the nonsensical intraday self-gap the read-side dedup eliminates).
pub(crate) fn select_prior_today<'a>(
    daily_sorted: &[&'a Bar],
    session_date: NaiveDate,
) -> Option<(&'a Bar, &'a Bar)> {
    let today = *daily_sorted.iter().rev().find(|b| kst_date_of(b) == session_date)?;
    let prior = *daily_sorted.iter().rev().find(|b| kst_date_of(b) < session_date)?;
    Some((prior, today))
}

/// Build the universe candidates for one `session_date` (KTD-3): per instrument with
/// any catalog daily bar, `today_open` = the open of its daily on `session_date`,
/// `prior_close` = the close of its latest daily strictly before, `prior_turnover` =
/// the prior close × volume. An instrument lacking either is skipped for this session
/// (it may still be a candidate on another day). Reads each instrument's pre-sorted
/// daily slice from `daily_by_inst`; an instrument absent from the map has NO daily
/// bars anywhere (the never-ingested noise filter — not a candidate, not a gap).
fn build_candidates(
    instruments: &[InstrumentAny],
    daily_by_inst: &HashMap<InstrumentId, Vec<&Bar>>,
    open_vol_by_inst: &HashMap<InstrumentId, BTreeMap<NaiveDate, f64>>,
    params: &OrbParams,
    session_date: NaiveDate,
    metadata: Option<&HashMap<String, InstrumentMetadata>>,
) -> Vec<UniverseCandidate> {
    let atr_window = params.atr_window as usize;
    let rvol_window = params.rvol_window_sessions as usize;
    let rvol_min_history = params.rvol_min_history as usize;
    let mut candidates = Vec::new();
    for inst in instruments {
        let id = inst.id();
        let Some(daily) = daily_by_inst.get(&id) else {
            continue; // never-ingested → not a candidate, not a gap
        };
        let Some((prior, today)) = select_prior_today(daily, session_date) else {
            continue; // no daily today, or no prior before it — not a candidate today
        };
        let symbol = id.to_string();
        let prior_turnover = prior.close.as_f64() * prior.volume.as_f64();
        let meta = candidate_meta(metadata, &symbol, prior_turnover);
        // Derived gate inputs (U2) — computed for every candidate but read only
        // by the strategy's gates for the selected ones, never by selection (R4).
        let prior_atr = prior_atr(daily, session_date, atr_window);
        let prior_open_vol_mean = prior_open_window_vol_mean(
            open_vol_by_inst.get(&id),
            session_date,
            rvol_window,
            rvol_min_history,
        );
        candidates.push(UniverseCandidate {
            symbol,
            prior_close: prior.close.as_f64(),
            today_open: today.open.as_f64(),
            prior_turnover,
            meta,
            prior_atr,
            prior_open_vol_mean,
        });
    }
    candidates
}

/// ATR(`window`) over the daily bars strictly before `session_date` (KTD5),
/// deduped to one bar per KST session (the latest, matching
/// [`select_prior_today`], so a value-divergent re-ingest overlap never
/// double-counts a session). True range per session =
/// `max(high−low, |high−prev_close|, |low−prev_close|)`; ATR is the mean TR over
/// the most recent `window` sessions. `None` when fewer than `window`+1 prior
/// sessions exist (the fail-closed boundary) or `window` is zero.
pub(crate) fn prior_atr(daily_sorted: &[&Bar], session_date: NaiveDate, window: usize) -> Option<f64> {
    if window == 0 {
        return None;
    }
    // Dedup to one bar per prior session date; BTreeMap keeps them date-ascending
    // and a later ts (the sort order) overwrites an earlier same-date copy.
    let mut by_date: BTreeMap<NaiveDate, &Bar> = BTreeMap::new();
    for b in daily_sorted {
        let d = kst_date_of(b);
        if d < session_date {
            by_date.insert(d, *b);
        }
    }
    // Need window+1 sessions: each of the `window` true ranges needs a prior close.
    if by_date.len() < window + 1 {
        return None;
    }
    let sessions: Vec<&Bar> = by_date.values().copied().collect();
    let tail = &sessions[sessions.len() - (window + 1)..];
    let mut sum_tr = 0.0;
    for pair in tail.windows(2) {
        let prev_close = pair[0].close.as_f64();
        let high = pair[1].high.as_f64();
        let low = pair[1].low.as_f64();
        let tr = (high - low)
            .max((high - prev_close).abs())
            .max((low - prev_close).abs());
        sum_tr += tr;
    }
    Some(sum_tr / window as f64)
}

/// The mean opening-window volume over up to `window` prior in-range sessions
/// strictly before `session_date` (KTD9), or `None` below `min_history` samples
/// (the fail-closed thin-history boundary) or when no prior sample exists. The
/// per-date map is already range-filtered, so the first `min_history` sessions of
/// the range yield `None` — the accepted, recorded trade-count asymmetry.
pub(crate) fn prior_open_window_vol_mean(
    per_date: Option<&BTreeMap<NaiveDate, f64>>,
    session_date: NaiveDate,
    window: usize,
    min_history: usize,
) -> Option<f64> {
    let per_date = per_date?;
    let mut priors: Vec<f64> = per_date.range(..session_date).map(|(_, v)| *v).collect();
    if priors.len() > window {
        priors = priors.split_off(priors.len() - window); // keep the most recent `window`
    }
    if priors.is_empty() || priors.len() < min_history {
        return None;
    }
    Some(priors.iter().sum::<f64>() / priors.len() as f64)
}

/// Join one candidate to its metadata record (U4): `Untagged` for a legacy run
/// (no artifact), `Missing` when the artifact carries no record for the symbol
/// (non-selectable, recorded — never silently defaulted, R4), else `Tagged`
/// with the gate verdict and the R9 conditioner tags. The liquidity tier is
/// daily-bar derived (a close×volume **proxy**) because the capture-time
/// turnover attribute is `Unavailable` this turn (R2/R5).
fn candidate_meta(
    metadata: Option<&HashMap<String, InstrumentMetadata>>,
    symbol: &str,
    prior_turnover: f64,
) -> CandidateMeta {
    let Some(map) = metadata else {
        return CandidateMeta::Untagged;
    };
    match map.get(shcode_of(symbol)) {
        None => CandidateMeta::Missing,
        Some(rec) => CandidateMeta::Tagged {
            tradable: rec.tradable,
            tags: ConditionerTags {
                cap_tier: rec.cap_tier,
                liquidity_tier: assign_liquidity_tier(&Resolved::Proxy(prior_turnover)),
                market_class: rec.market_class,
                index_membership: rec.index_membership,
                has_derivative: rec.has_derivative,
            },
        },
    }
}

/// The shcode of a `{shcode}.XKRX` instrument-id string.
fn shcode_of(symbol: &str) -> &str {
    symbol.split('.').next().unwrap_or(symbol)
}

/// Build + run the engine, returning the finished positions (cloned) each paired
/// with the entry-fixed risk the strategy captured for that symbol this session (U1).
/// The pairing is by instrument id and unambiguous — ORB holds at most one open leg
/// per symbol per session — so `risks[i]` belongs to `positions[i]`.
fn run_engine(
    instruments: Vec<InstrumentAny>,
    bars: Vec<Bar>,
    params: OrbParams,
    selected: Vec<SelectedSymbol>,
    sink: DecisionSink,
    starting_balance: f64,
) -> anyhow::Result<(Vec<Position>, Vec<Option<EntryRisk>>)> {
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
    let strategy = OrbStrategy::new(params, selected, sink);
    // Clone the entry-risk ledger BEFORE the engine consumes the strategy (U1) —
    // the same shared-handle pattern as the decision sink.
    let risk_ledger = strategy.entry_risk_ledger();
    engine.add_strategy(strategy)?;

    if !bars.is_empty() {
        let mut sorted = bars;
        sorted.sort_by_key(|b| b.ts_event.as_u64());
        let data: Vec<Data> = sorted.into_iter().map(Data::Bar).collect();
        engine.add_data(data, None, true, true)?;
        engine.run(None, None, None, false)?;
    }

    let cache = engine.kernel().cache.borrow();
    let positions: Vec<Position> = cache
        .positions(None, None, None, None, None)
        .into_iter()
        .map(|p| p.cloned())
        .collect();
    // Join the captured entry risk to each position by instrument id (unambiguous
    // within a session). A position with no captured risk (should not occur — every
    // position starts from an OrderPlaced) resolves to `None` → the legacy P&L path.
    let risks_by_symbol = risk_ledger.snapshot();
    let risks: Vec<Option<EntryRisk>> =
        positions.iter().map(|p| risks_by_symbol.get(&p.instrument_id).copied()).collect();
    Ok((positions, risks))
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

pub(crate) fn is_daily(b: &Bar) -> bool {
    b.bar_type.spec().aggregation == BarAggregation::Day
}
fn is_minute(b: &Bar) -> bool {
    b.bar_type.spec().aggregation == BarAggregation::Minute
}
fn in_range(b: &Bar, start_ns: u64, end_ns: u64) -> bool {
    let ts = b.ts_event.as_u64();
    ts >= start_ns && ts <= end_ns
}

/// The KST calendar date of a bar (delegates to the adapter's single KST
/// conversion so session-slicing and ingest agree on date boundaries).
pub(crate) fn kst_date_of(b: &Bar) -> NaiveDate {
    nautilus_ls::ingest::kst_date_of(b.ts_event)
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
/// `LS_BT_BALANCE`, `LS_BT_METADATA` (optional — the `UniverseMetadata`
/// artifact path for a metadata-driven run, plan 2026-07-10-003),
/// `LS_BT_PARAMS_FROM_RUN` (optional — adopt the full parameter set from a
/// finalized run's manifest, so a count run reproduces the loop's current
/// identity instead of the v0 defaults; the pre-check counts are
/// threshold-conditioned on `gap_min_pct`, AE2), `LS_BT_VERSION` (optional
/// version override for the adopted set).
pub fn main_cli() -> anyhow::Result<()> {
    nautilus_ls::scrub::install();
    let data_home = std::env::var("LS_DATA_HOME").map_err(|_| anyhow::anyhow!("LS_DATA_HOME is required"))?;
    let sdate = std::env::var("LS_BT_SDATE").map_err(|_| anyhow::anyhow!("LS_BT_SDATE is required"))?;
    let edate = std::env::var("LS_BT_EDATE").map_err(|_| anyhow::anyhow!("LS_BT_EDATE is required"))?;
    let mut cfg = BacktestConfig::new(data_home.clone(), &sdate, &edate);
    if let Some(run_id) =
        std::env::var("LS_BT_PARAMS_FROM_RUN").ok().filter(|s| !s.trim().is_empty())
    {
        let manifest = crate::runner::research::read_manifest(Path::new(&data_home), run_id.trim())?;
        println!(
            "params adopted from run {} (v{}, gap_min_pct {})",
            run_id.trim(),
            manifest.strategy_version,
            manifest.params.gap_min_pct
        );
        cfg.params = manifest.params;
        // Review finding: an adopted-params run finalizing under the SOURCE
        // run's version would become the next `turn`'s latest-finalized
        // baseline indistinguishably — require a distinct version so the
        // EXPECT_VERSION seed assertion and `runs compare` can tell a count
        // run from the identity it borrowed.
        let v = std::env::var("LS_BT_VERSION").map_err(|_| {
            anyhow::anyhow!(
                "LS_BT_PARAMS_FROM_RUN requires LS_BT_VERSION: an adopted-params run must \
                 finalize under its own version, or the next turn resolves its baseline from \
                 this run without knowing it"
            )
        })?;
        cfg.params.strategy_version = v
            .parse()
            .map_err(|_| anyhow::anyhow!("LS_BT_VERSION must be an integer, got {v:?}"))?;
        println!(
            "count run finalizes as the LATEST run (v{}) — pin LS_TURN_EXPECT_VERSION on the \
             next turn accordingly",
            cfg.params.strategy_version
        );
    } else if let Ok(v) = std::env::var("LS_BT_VERSION") {
        cfg.params.strategy_version = v
            .parse()
            .map_err(|_| anyhow::anyhow!("LS_BT_VERSION must be an integer, got {v:?}"))?;
    }
    if let Ok(step) = std::env::var("LS_BT_MINUTE_STEP") {
        cfg.minute_step = step.parse().unwrap_or(1);
    }
    if let Ok(bal) = std::env::var("LS_BT_BALANCE") {
        cfg.starting_balance = bal.parse().unwrap_or(cfg.starting_balance);
    }
    cfg.metadata_path =
        std::env::var("LS_BT_METADATA").ok().filter(|s| !s.trim().is_empty()).map(PathBuf::from);
    let rt = tokio::runtime::Runtime::new()?;
    let outcome = rt.block_on(run(cfg, Utc::now()))?;
    // R10: a trailing summary block printed AFTER all engine logs, so the only
    // operator-relevant output never scrolls away under the engine's INFO noise.
    print!("{}", summary_block(&outcome.run_id, &outcome.run_dir));
    // The Turn-N per-tier count summary (U6) — counts + verdict only, computed
    // from the decision stream, never from performance.json (KTD5).
    if let Some(lines) = &outcome.tier_summary {
        for l in lines {
            println!("{l}");
        }
    }
    Ok(())
}

/// The `lab-backtest` trailing summary block (R10): run id, trade count, and the
/// finalized run dir, read from the run's `performance.json`. A missing/parse
/// failure degrades to a `?` trade count rather than hiding the block.
pub fn summary_block(run_id: &str, run_dir: &Path) -> String {
    let trades = std::fs::read_to_string(run_dir.join(crate::artifacts::PERFORMANCE_FILE))
        .ok()
        .and_then(|t| serde_json::from_str::<PerformanceReport>(&t).ok())
        .and_then(|p| p.summary.get("num_trades").copied())
        .map(|n| format!("{n:.0}"))
        .unwrap_or_else(|| "?".to_string());
    format!(
        "\n=== lab-backtest summary ===\nrun:    {run_id}\ntrades: {trades}\ndir:    {}\n",
        run_dir.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_ls::rules::KRX_REGULAR_CLOSE;
    use nautilus_model::data::BarType;
    use nautilus_model::identifiers::InstrumentId;
    use nautilus_model::types::{Price, Quantity};

    fn day(bt: BarType, ymd: (i32, u32, u32), open: i64, close: i64) -> Bar {
        let ts = kst_to_unix_nanos(
            NaiveDate::from_ymd_opt(ymd.0, ymd.1, ymd.2).unwrap(),
            KRX_REGULAR_CLOSE,
        )
        .unwrap();
        Bar::new(
            bt,
            Price::from(open.to_string().as_str()),
            Price::from((close + 10).to_string().as_str()),
            Price::from((open - 10).to_string().as_str()),
            Price::from(close.to_string().as_str()),
            Quantity::from(1000),
            ts,
            ts,
        )
    }

    #[test]
    fn select_prior_today_skips_a_same_session_duplicate() {
        let bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        // A value-divergent duplicate of the final session (an un-healed overlap the
        // read dedup keeps) sits last after the ts sort. prior/today must resolve to
        // two DISTINCT sessions (Jan4 -> Jan5), never both copies of Jan5 — which
        // would compute a self-gap.
        let jan4 = day(bt, (2024, 1, 4), 100, 110);
        let jan5_a = day(bt, (2024, 1, 5), 110, 120);
        let jan5_b = day(bt, (2024, 1, 5), 60, 65); // divergent same-session copy
        let daily = vec![&jan4, &jan5_a, &jan5_b];
        let jan5 = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        let (prior, today) = select_prior_today(&daily, jan5).expect("prior + today for the session");
        assert_eq!(prior.ts_event, jan4.ts_event, "prior is the earlier distinct session");
        assert_eq!(kst_date_of(today), jan5, "today is dated on the session");
        assert_ne!(prior.ts_event, today.ts_event, "prior and today are never the same session");
    }

    #[test]
    fn select_prior_today_needs_two_distinct_sessions() {
        let bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        // Only one real session (plus a divergent duplicate of it) is not enough for
        // a gap — no prior session exists.
        let a = day(bt, (2024, 1, 5), 110, 120);
        let b = day(bt, (2024, 1, 5), 60, 65);
        let jan5 = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        assert!(select_prior_today(&vec![&a, &b], jan5).is_none(), "one session has no prior");
        assert!(select_prior_today(&[], jan5).is_none(), "empty has no prior");
    }

    #[test]
    fn summary_block_names_run_trades_and_dir() {
        let dir = tempfile::tempdir().unwrap();
        let run_dir = dir.path().join("runs").join("r1");
        std::fs::create_dir_all(&run_dir).unwrap();
        let perf = PerformanceReport::assemble(Vec::new(), 1_000_000.0);
        std::fs::write(
            run_dir.join(crate::artifacts::PERFORMANCE_FILE),
            serde_json::to_string(&perf).unwrap(),
        )
        .unwrap();
        let block = summary_block("r1", &run_dir);
        assert!(block.contains("lab-backtest summary"));
        assert!(block.contains("run:    r1"));
        assert!(block.contains("trades: 0"), "trade count present: {block}");
        assert!(block.contains("dir:"));
    }

    // -- U2: candidate-seam ATR + opening-window RVOL derivation --------------

    /// A daily bar with explicit high/low/close (the `day` helper derives them;
    /// ATR needs them controlled).
    fn day_hlc(bt: BarType, ymd: (i32, u32, u32), high: i64, low: i64, close: i64) -> Bar {
        let ts = kst_to_unix_nanos(NaiveDate::from_ymd_opt(ymd.0, ymd.1, ymd.2).unwrap(), KRX_REGULAR_CLOSE)
            .unwrap();
        Bar::new(
            bt,
            Price::from(close.to_string().as_str()),
            Price::from(high.to_string().as_str()),
            Price::from(low.to_string().as_str()),
            Price::from(close.to_string().as_str()),
            Quantity::from(1000),
            ts,
            ts,
        )
    }

    #[test]
    fn prior_atr_matches_hand_computation_and_excludes_own_bar() {
        let bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        // jan1 close 100; jan2 TR = max(20, |110-100|, |90-100|) = 20;
        // jan3 TR = max(10, |108-105|, |98-105|) = 10. ATR(2) over jan1..jan3 = 15.
        let d1 = day_hlc(bt, (2024, 1, 1), 101, 99, 100);
        let d2 = day_hlc(bt, (2024, 1, 2), 110, 90, 105);
        let d3 = day_hlc(bt, (2024, 1, 3), 108, 98, 104);
        // The session's OWN bar (jan4) must be excluded from the lookback.
        let d4 = day_hlc(bt, (2024, 1, 4), 200, 10, 150);
        let daily = vec![&d1, &d2, &d3, &d4];
        let jan4 = NaiveDate::from_ymd_opt(2024, 1, 4).unwrap();
        assert_eq!(prior_atr(&daily, jan4, 2), Some(15.0));
    }

    #[test]
    fn prior_atr_fails_closed_below_window_plus_one() {
        let bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let d1 = day_hlc(bt, (2024, 1, 1), 101, 99, 100);
        let d2 = day_hlc(bt, (2024, 1, 2), 110, 90, 105);
        let d3 = day_hlc(bt, (2024, 1, 3), 108, 98, 104);
        let jan4 = NaiveDate::from_ymd_opt(2024, 1, 4).unwrap();
        // window 2 needs 3 priors: exactly 2 → None, exactly 3 → Some (boundary).
        assert_eq!(prior_atr(&vec![&d2, &d3], jan4, 2), None, "2 priors, window 2");
        assert!(prior_atr(&vec![&d1, &d2, &d3], jan4, 2).is_some(), "3 priors, window 2");
        assert_eq!(prior_atr(&vec![&d1, &d2, &d3], jan4, 0), None, "zero window");
    }

    #[test]
    fn prior_atr_dedups_a_same_session_divergent_copy() {
        let bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let d1 = day_hlc(bt, (2024, 1, 1), 101, 99, 100);
        let d2 = day_hlc(bt, (2024, 1, 2), 110, 90, 105);
        let d3 = day_hlc(bt, (2024, 1, 3), 108, 98, 104);
        // A value-divergent duplicate of jan3 sits last after the ts sort; the
        // latest per-date wins (matching select_prior_today), so it must not add
        // a phantom fourth session.
        let d3_dup = day_hlc(bt, (2024, 1, 3), 108, 98, 104);
        let jan4 = NaiveDate::from_ymd_opt(2024, 1, 4).unwrap();
        assert_eq!(
            prior_atr(&vec![&d1, &d2, &d3, &d3_dup], jan4, 2),
            Some(15.0),
            "the same-session copy is deduped, not counted as a 4th session"
        );
    }

    #[test]
    fn prior_open_window_vol_mean_respects_history_and_window() {
        let jan1 = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let jan2 = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        let jan3 = NaiveDate::from_ymd_opt(2024, 1, 3).unwrap();
        let session = NaiveDate::from_ymd_opt(2024, 1, 4).unwrap();
        let per_date: BTreeMap<NaiveDate, f64> =
            [(jan1, 100.0), (jan2, 200.0), (jan3, 300.0)].into_iter().collect();
        // Two priors (jan2, jan3 under a window of 2), min_history 2 → mean 250.
        assert_eq!(
            prior_open_window_vol_mean(Some(&per_date), session, 2, 2),
            Some(250.0),
            "most-recent 2 sessions averaged"
        );
        // Same window, min_history 3 → fewer samples than required → None.
        assert_eq!(prior_open_window_vol_mean(Some(&per_date), session, 2, 3), None);
        // All three within window 5, min_history 2 → mean 200.
        assert_eq!(prior_open_window_vol_mean(Some(&per_date), session, 5, 2), Some(200.0));
    }

    #[test]
    fn prior_open_window_vol_mean_none_without_prior_samples() {
        let jan1 = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let session = jan1; // no strictly-prior sample
        let per_date: BTreeMap<NaiveDate, f64> = [(jan1, 100.0)].into_iter().collect();
        assert_eq!(prior_open_window_vol_mean(Some(&per_date), session, 5, 1), None, "first session");
        assert_eq!(prior_open_window_vol_mean(None, session, 5, 1), None, "no minute index");
    }

    #[test]
    fn candidate_derived_fields_are_selection_neutral() {
        // R4: select_universe reads neither prior_atr nor prior_open_vol_mean —
        // the same candidates with the fields None vs Some select identically.
        let sink_a = DecisionSink::new();
        let sink_b = DecisionSink::new();
        let params = OrbParams::default(); // gap_min_pct 3.0
        let base = |atr: Option<f64>, rvol: Option<f64>| UniverseCandidate {
            symbol: "005930.XKRX".to_string(),
            prior_close: 60_000.0,
            today_open: 63_000.0, // +5% gap, passes
            prior_turnover: 1_000_000.0,
            meta: CandidateMeta::Untagged,
            prior_atr: atr,
            prior_open_vol_mean: rvol,
        };
        let none = vec![base(None, None)];
        let some = vec![base(Some(1234.0), Some(9999.0))];
        assert_eq!(
            select_universe(&none, &params, &sink_a, 0),
            select_universe(&some, &params, &sink_b, 0),
            "derived fields never change selection"
        );
    }
}
