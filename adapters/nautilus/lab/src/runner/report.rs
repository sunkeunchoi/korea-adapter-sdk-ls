//! `report mfe` — the MFE-distribution report over a run's `decisions.jsonl`
//! (turn-9 U1; R5–R7).
//!
//! Reads a finalized run's decision stream through the replay reader (KTD2 —
//! never hand-parsed JSONL), joins each exit envelope to its breakout envelope
//! on (symbol, KST session date) (KTD3 — one trade per symbol per session makes
//! the join unambiguous), and prints:
//!
//! - per-trade `mfe_r` percentiles (nearest-rank, KTD5),
//! - `mfe_r` by exit reason,
//! - `mfe_r` by breakout-strength quartile — the entry-filter spec input (KTD4),
//! - the leg-2 target candidate with its censoring/band verdict (KTD6), printed
//!   alongside the source run's `profit_target_r` and target-exit share so the
//!   right-censoring is visible in every future use.
//!
//! The report reads artifacts only — no strategy code is touched, so the
//! strategy code hash is unchanged and no re-baseline occurs (R6).

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::NaiveDate;
use nautilus_core::UnixNanos;
use nautilus_ls::ingest::kst_date_of;

use nautilus_ls::reference::universe_metadata::Stratum;

use crate::agent::envelope::{Decision, DecisionEnvelope, SignalKind};
use crate::agent::replay::read_envelopes;
use crate::artifacts::manifest::Manifest;
use crate::artifacts::performance::PerformanceReport;
use crate::artifacts::{DECISIONS_FILE, PERFORMANCE_FILE};
use crate::margin::{self, LoadedMargin};
use crate::params::StopMode;
use crate::runner::research::{latest_finalized_run, read_manifest, PROPOSAL_BOUNDS_CAP};
use crate::stats::{
    self, block_bootstrap_ratio, clustering, interval_normal, interval_t_few_clusters,
    minimum_detectable_edge, required_trades, wild_cluster_interval, Block, BootstrapOutcome,
    Clustering, Interval, MarginArm, MarginVerdict,
};

/// The candidate rounding step (KTD6): the empirical p70 is rounded to the
/// nearest 0.05, and a candidate at or within one step of the source run's own
/// `profit_target_r` is declared right-censored.
pub const CANDIDATE_STEP: f64 = 0.05;

/// `report mfe` config.
#[derive(Debug, Clone)]
pub struct ReportConfig {
    /// The data home.
    pub data_home: PathBuf,
    /// The run to report on (`LS_REPORT_RUN`); `None` means the latest
    /// finalized run (the `analyze --scaffold` / `LS_ANALYZE_RUN` precedent).
    pub run_id: Option<String>,
}

/// The leg-2 candidate's disposition (KTD6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateVerdict {
    /// In-band and not censored — a governed leg may run at this value.
    Runnable,
    /// At or within one rounding step of the source run's own target: the
    /// distribution is truncated there and yields no informative candidate;
    /// a further leg proceeds only via operator override.
    RightCensored,
    /// Outside the proposal-bounds band off the source run's target: record
    /// the recommendation, run nothing (AE3).
    OutOfBand,
    /// No trades with `mfe_r > 0` — no candidate at all.
    NoSample,
}

impl CandidateVerdict {
    fn label(self) -> &'static str {
        match self {
            CandidateVerdict::Runnable => "RUNNABLE",
            CandidateVerdict::RightCensored => "RIGHT-CENSORED (no informative candidate)",
            CandidateVerdict::OutOfBand => "OUT-OF-BAND (record recommendation, no run)",
            CandidateVerdict::NoSample => "NO-SAMPLE (no positive-MFE trades)",
        }
    }
}

/// The leg-2 target candidate the report derives (KTD6).
#[derive(Debug, Clone)]
pub struct LegTwoCandidate {
    /// The rounded candidate value (`None` when there is no positive-MFE sample).
    pub value: Option<f64>,
    /// The raw nearest-rank p70 the candidate was rounded from (`None` with no
    /// positive-MFE sample) — carried so display never recomputes it.
    pub p70: Option<f64>,
    /// The disposition.
    pub verdict: CandidateVerdict,
    /// The proposal-bounds band off the source run's `profit_target_r`.
    pub band: (f64, f64),
}

/// A `report mfe` outcome. Structured facts for tests + the printed lines.
#[derive(Debug, Clone)]
pub struct ReportOutcome {
    /// The reported run.
    pub run_id: String,
    /// The source run's `profit_target_r` — every distribution is right-censored
    /// at this value (KTD6).
    pub profit_target_r: f64,
    /// MFE-bearing exit records (= trades).
    pub trades: usize,
    /// Share of trades that exited at the fixed target.
    pub target_exit_share: f64,
    /// Breakout envelopes with no exit (sizing-rejected / teardown) — tolerated
    /// and counted, never a panic (KTD3).
    pub orphan_breakouts: usize,
    /// Exit envelopes with no joinable breakout — excluded from strength buckets.
    pub orphan_exits: usize,
    /// Breakouts with a degenerate range (`R <= 0`) — excluded from strength
    /// buckets (KTD4).
    pub degenerate_ranges: usize,
    /// The leg-2 candidate (KTD6).
    pub candidate: LegTwoCandidate,
    /// The printed report lines.
    pub lines: Vec<String>,
}

/// One joined per-trade row.
struct TradeRow {
    kind: SignalKind,
    mfe_r: f64,
    /// `(breakout_price − range_high) / R` when the breakout joined and its
    /// range was non-degenerate (KTD4).
    strength: Option<f64>,
    /// Exit limit price > entry (breakout) limit price, when both are known.
    win: Option<bool>,
}

/// Nearest-rank percentile over a **sorted** slice (KTD5): the smallest sample
/// value with at least `pct`% of the sample at or below it.
///
/// One implementation, in [`crate::stats::percentile`]. This alias keeps the
/// six existing call sites and `nearest_rank_at_odd_and_even_counts` reading as
/// they did, while the statistics core owns the arithmetic — a second copy of
/// the same formula is a place for the two to drift.
use crate::stats::percentile as nearest_rank;

/// Sort an f64 sample ascending. The unwrap is safe for every sample this
/// report builds: `mfe_r` arrives via serde_json (which cannot carry NaN) and
/// strength is only computed under an `r > 0` guard.
fn sorted(mut values: Vec<f64>) -> Vec<f64> {
    values.sort_by(|a, b| a.partial_cmp(b).expect("sample is never NaN"));
    values
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

fn exit_kind_label(kind: SignalKind) -> &'static str {
    match kind {
        SignalKind::StopHit => "stop_hit",
        SignalKind::Target => "target",
        SignalKind::TimeExit => "time_exit",
        _ => "other",
    }
}

/// Derive the leg-2 candidate from the positive-MFE sample (KTD6): nearest-rank
/// p70 rounded to [`CANDIDATE_STEP`], censoring checked before the band — a
/// candidate pinned at the source target is uninformative regardless of band.
fn leg_two_candidate(positive_sorted: &[f64], profit_target_r: f64) -> LegTwoCandidate {
    let band = (
        profit_target_r * (1.0 - PROPOSAL_BOUNDS_CAP),
        profit_target_r * (1.0 + PROPOSAL_BOUNDS_CAP),
    );
    let Some(p70) = nearest_rank(positive_sorted, 70.0) else {
        return LegTwoCandidate { value: None, p70: None, verdict: CandidateVerdict::NoSample, band };
    };
    let candidate = (p70 / CANDIDATE_STEP).round() * CANDIDATE_STEP;
    let verdict = if (candidate - profit_target_r).abs() <= CANDIDATE_STEP + 1e-9 {
        CandidateVerdict::RightCensored
    } else if candidate < band.0 - 1e-9 || candidate > band.1 + 1e-9 {
        CandidateVerdict::OutOfBand
    } else {
        CandidateVerdict::Runnable
    };
    LegTwoCandidate { value: Some(candidate), p70: Some(p70), verdict, band }
}

// ---------------------------------------------------------------------------
// Per-tier trade counts + the power pre-check (plan 2026-07-10-003, U6/R8).
// ---------------------------------------------------------------------------

/// The pre-registered per-tier trade floor (Goal Capsule): a tier "clears"
/// with at least this many joined trades.
pub const PRECHECK_TRADE_FLOOR: usize = 30;
/// The pre-registered tier quorum (Goal Capsule): the pre-check is green when
/// at least this many tiers clear [`PRECHECK_TRADE_FLOOR`].
pub const PRECHECK_MIN_TIERS: usize = 2;

/// Count joined trades per stratum from a decision stream (U5's join): exit
/// envelopes (`stop_hit`/`target`/`time_exit`) join to their tagged
/// universe-accept envelope on `(symbol, KST session date)`; the stratum comes
/// from the accept envelope's conditioner tags (KTD4). Returns the per-stratum
/// counts (every stratum keyed, zeros included) plus the exits that joined no
/// tagged accept (counted, never silently dropped).
pub fn tier_trade_counts(envelopes: &[DecisionEnvelope]) -> (BTreeMap<Stratum, usize>, usize) {
    let mut accept_stratum: BTreeMap<(String, NaiveDate), Stratum> = BTreeMap::new();
    for e in envelopes {
        let Some(d) = &e.decision_detail else { continue };
        if d.kind == SignalKind::Universe && d.decision == Some(Decision::Accept) {
            if let Some(tags) = &d.tags {
                accept_stratum
                    .entry((d.symbol.clone(), kst_date_of(UnixNanos::from(e.ts_event))))
                    .or_insert(tags.stratum());
            }
        }
    }
    let mut counts: BTreeMap<Stratum, usize> = Stratum::ALL.iter().map(|s| (*s, 0)).collect();
    let mut untagged_exits = 0usize;
    for e in envelopes {
        let Some(d) = &e.decision_detail else { continue };
        if !matches!(d.kind, SignalKind::StopHit | SignalKind::Target | SignalKind::TimeExit) {
            continue;
        }
        let key = (d.symbol.clone(), kst_date_of(UnixNanos::from(e.ts_event)));
        match accept_stratum.get(&key) {
            Some(stratum) => *counts.get_mut(stratum).expect("all strata keyed") += 1,
            None => untagged_exits += 1,
        }
    }
    (counts, untagged_exits)
}

/// The pre-registered power pre-check (R8): green iff at least
/// [`PRECHECK_MIN_TIERS`] tiers carry at least [`PRECHECK_TRADE_FLOOR`] trades.
pub fn power_precheck(counts: &BTreeMap<Stratum, usize>) -> bool {
    counts.values().filter(|c| **c >= PRECHECK_TRADE_FLOOR).count() >= PRECHECK_MIN_TIERS
}

/// Render the Turn-N per-tier count summary (U6): per-tier counts against the
/// floor plus the green/red verdict line. **Counts only — no expectancy, no
/// P&L** (the KTD5 staging guard; the caller never reads `performance.json`).
/// `symbols_label` names the symbol population the caller supplies — the
/// runner passes per-tier **selected**-union counts, `report tiers` passes the
/// ingest pin's **ingested** counts; the label keeps the two surfaces from
/// printing identical-looking columns with different meanings.
pub fn tier_summary_lines(
    symbols_label: &str,
    symbol_counts: &BTreeMap<Stratum, usize>,
    trade_counts: &BTreeMap<Stratum, usize>,
    untagged_exits: usize,
) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("per-tier trade counts (power pre-check; counts only, KTD5 staging guard):".to_string());
    for stratum in Stratum::ALL {
        let trades = trade_counts.get(&stratum).copied().unwrap_or(0);
        let symbols = symbol_counts.get(&stratum).copied().unwrap_or(0);
        lines.push(format!(
            "  {:<20} {symbols_label} symbols {:<4} trades {:<4} floor {} -> {}",
            stratum.label(),
            symbols,
            trades,
            PRECHECK_TRADE_FLOOR,
            if trades >= PRECHECK_TRADE_FLOOR { "clears" } else { "short" }
        ));
    }
    if untagged_exits > 0 {
        lines.push(format!(
            "  (untagged: {untagged_exits} exits joined no tagged accept — excluded from every tier)"
        ));
    }
    let green = power_precheck(trade_counts);
    lines.push(format!(
        "power pre-check: {} (>= {} trades in >= {} tiers) — {}",
        if green { "GREEN" } else { "RED" },
        PRECHECK_TRADE_FLOOR,
        PRECHECK_MIN_TIERS,
        if green {
            "Turn N+1 verdict run is green-lit"
        } else {
            "Turn N+1 is called off; a red pre-check is a valid completion (AE2)"
        }
    ));
    lines
}

// ---------------------------------------------------------------------------
// `report tiers` — the Turn-N per-tier count report (plan 2026-07-10-003, U6).
// ---------------------------------------------------------------------------

/// `report tiers` config.
#[derive(Debug, Clone)]
pub struct TiersConfig {
    /// The data home.
    pub data_home: PathBuf,
    /// The run to report on (`LS_REPORT_RUN`); `None` = latest finalized.
    pub run_id: Option<String>,
    /// Artifact-path override (`LS_REPORT_METADATA`); `None` reads the path the
    /// ingest pin recorded. The content hash is asserted either way.
    pub artifact_path: Option<PathBuf>,
}

/// A `report tiers` outcome: structured counts for tests + the printed lines.
#[derive(Debug, Clone)]
pub struct TiersOutcome {
    /// The reported run.
    pub run_id: String,
    /// Joined trades per stratum.
    pub trade_counts: BTreeMap<Stratum, usize>,
    /// The pre-registered power pre-check verdict (R8).
    pub green: bool,
    /// The printed report lines — counts + distribution only, **no expectancy**
    /// (the KTD5 staging guard: `performance.json` is never read here).
    pub lines: Vec<String>,
}

/// Build the per-tier trade-count report + power pre-check for one run (U6,
/// R7/R8). Fails on I/O, on a run that carries no metadata hash, and on any
/// artifact-hash mismatch between the ingest pin, the run manifest, and the
/// artifact on disk (KTD2 — a re-capture between ingest and backtest would
/// silently re-tier symbols). A **red** verdict is a valid completion, not a
/// failure (AE2): the exit code reflects integrity + I/O only.
pub async fn report_tiers(cfg: &TiersConfig) -> anyhow::Result<TiersOutcome> {
    let (run_id, manifest): (String, Manifest) = match &cfg.run_id {
        Some(id) => (id.clone(), read_manifest(&cfg.data_home, id)?),
        None => latest_finalized_run(&cfg.data_home)?.ok_or_else(|| {
            anyhow::anyhow!(
                "no finalized runs under {} — set LS_REPORT_RUN or run a backtest first",
                cfg.data_home.display()
            )
        })?,
    };
    let Some(expected_hash) = manifest.universe_metadata_hash.clone() else {
        anyhow::bail!(
            "run {run_id} carries no universe_metadata_hash — not a metadata-driven run \
             (re-run the backtest with LS_BT_METADATA)"
        );
    };

    // KTD2 hash handshake: ingest pin ↔ run manifest ↔ artifact on disk.
    let catalog_path = cfg.data_home.join("catalog");
    let pin = nautilus_ls::reference::universe_metadata::MetadataPin::load(&catalog_path)
        .map_err(|e| anyhow::anyhow!(e))?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no ingest pin at {}/universe-metadata-pin.json — run the tier-stratified \
                 ingest (LS_INGEST_METADATA) before reporting (KTD2)",
                catalog_path.display()
            )
        })?;
    if pin.content_hash != expected_hash {
        anyhow::bail!(
            "artifact hash mismatch (KTD2): ingest pinned {} but run {run_id} was backtested \
             against {} — a re-capture between ingest and backtest re-tiers symbols and \
             corrupts the per-tier counts; re-ingest and re-run against ONE artifact",
            pin.content_hash,
            expected_hash
        );
    }
    let artifact_path =
        cfg.artifact_path.clone().unwrap_or_else(|| PathBuf::from(&pin.artifact_path));
    let artifact = nautilus_ls::reference::universe_metadata::UniverseMetadata::load(&artifact_path)
        .map_err(|e| anyhow::anyhow!(e))?;
    if artifact.content_hash() != expected_hash {
        anyhow::bail!(
            "artifact {} no longer matches the run: its content hash differs from the pinned \
             {expected_hash} (KTD2)",
            artifact_path.display()
        );
    }

    // Per-tier trade counts via the tagged accept join (U5).
    let decisions_path = cfg.data_home.join("runs").join(&run_id).join(DECISIONS_FILE);
    let envelopes = read_envelopes(&decisions_path)
        .map_err(|e| anyhow::anyhow!("reading {}: {e:?}", decisions_path.display()))?;
    let (trade_counts, untagged_exits) = tier_trade_counts(&envelopes);

    // Ingested-symbol counts per tier, from the pin's recorded composition.
    let mut symbol_counts: BTreeMap<Stratum, usize> =
        Stratum::ALL.iter().map(|s| (*s, 0)).collect();
    for stratum in Stratum::ALL {
        if let Some(n) = pin.per_stratum.get(stratum.label()) {
            symbol_counts.insert(stratum, *n);
        }
    }

    let mut lines = Vec::new();
    lines.push(format!(
        "report tiers: run {run_id} (strategy v{}, artifact hash {})",
        manifest.strategy_version, expected_hash
    ));
    lines.extend(tier_summary_lines("ingested", &symbol_counts, &trade_counts, untagged_exits));

    // Per-tier opening-gap-% distribution from catalog daily bars (AE2's
    // diagnosability): separates a genuinely thin tier (no qualifying gaps)
    // from a blue-chip-calibrated `gap_min_pct` sitting just above another
    // tier's gap sizes. Descriptive counts only — no P&L (KTD5).
    let gap_min = manifest.params.gap_min_pct;
    let all_bars = nautilus_ls::ingest::read_all_bars(&catalog_path).await?;
    let start_ns = kst_bound_ns(&manifest.data_range.start, false)?;
    let end_ns = kst_bound_ns(&manifest.data_range.end, true)?;
    let mut gap_samples: BTreeMap<Stratum, Vec<f64>> =
        Stratum::ALL.iter().map(|s| (*s, Vec::new())).collect();
    {
        use crate::runner::backtest::{is_daily, kst_date_of as bar_date, select_prior_today};
        let mut daily_by_inst: std::collections::HashMap<String, Vec<&nautilus_model::data::Bar>> =
            std::collections::HashMap::new();
        for b in &all_bars {
            if is_daily(b) {
                daily_by_inst.entry(b.bar_type.instrument_id().to_string()).or_default().push(b);
            }
        }
        let stratum_by_shcode: std::collections::HashMap<&str, Stratum> = artifact
            .records
            .iter()
            .map(|r| {
                (r.shcode.as_str(), nautilus_ls::reference::universe_metadata::stratum_of(r.market_class, r.cap_tier))
            })
            .collect();
        for (symbol, daily) in daily_by_inst.iter_mut() {
            let shcode = symbol.split('.').next().unwrap_or(symbol);
            let Some(stratum) = stratum_by_shcode.get(shcode) else { continue };
            daily.sort_by_key(|b| b.ts_event.as_u64());
            let mut session_dates: Vec<NaiveDate> = daily
                .iter()
                .filter(|b| {
                    let ts = b.ts_event.as_u64();
                    ts >= start_ns && ts <= end_ns
                })
                .map(|b| bar_date(b))
                .collect();
            session_dates.sort();
            session_dates.dedup();
            for date in session_dates {
                if let Some((prior, today)) = select_prior_today(daily, date) {
                    let prior_close = prior.close.as_f64();
                    if prior_close > 0.0 {
                        let gap = (today.open.as_f64() - prior_close) / prior_close * 100.0;
                        gap_samples.get_mut(stratum).expect("keyed").push(gap);
                    }
                }
            }
        }
    }
    lines.push(format!("per-tier opening-gap% distribution (gap_min_pct {gap_min}):"));
    for stratum in Stratum::ALL {
        let sample = sorted(gap_samples.remove(&stratum).unwrap_or_default());
        if sample.is_empty() {
            lines.push(format!("  {:<20} (no daily-bar symbol-sessions)", stratum.label()));
            continue;
        }
        let clearing = sample.iter().filter(|g| **g >= gap_min).count();
        let p = |pct: f64| {
            nearest_rank(&sample, pct).map(|v| format!("{v:.2}")).unwrap_or_else(|| "n/a".into())
        };
        lines.push(format!(
            "  {:<20} n={:<5} p25 {} | p50 {} | p75 {} | p90 {} | >= {gap_min}%: {}/{} ({:.1}%)",
            stratum.label(),
            sample.len(),
            p(25.0),
            p(50.0),
            p(75.0),
            p(90.0),
            clearing,
            sample.len(),
            clearing as f64 / sample.len() as f64 * 100.0
        ));
    }

    let green = power_precheck(&trade_counts);
    Ok(TiersOutcome { run_id, trade_counts, green, lines })
}

/// A KST day bound (`YYYYMMDD` → UTC unix ns), start-of-day or end-of-day.
fn kst_bound_ns(date: &str, end_of_day: bool) -> anyhow::Result<u64> {
    let d = NaiveDate::parse_from_str(date.trim(), "%Y%m%d")?;
    let t = if end_of_day {
        chrono::NaiveTime::from_hms_opt(23, 59, 59).expect("valid time")
    } else {
        chrono::NaiveTime::from_hms_opt(0, 0, 0).expect("valid time")
    };
    Ok(nautilus_ls::ingest::kst_to_unix_nanos(d, t)?.as_u64())
}

/// Build the MFE-distribution report for one run (R5, R7). Fails cleanly on a
/// missing/empty decision stream or an unknown run id; the distribution's
/// *content* never fails the command (the exit code reflects I/O only).
pub fn report_mfe(cfg: &ReportConfig) -> anyhow::Result<ReportOutcome> {
    let latest = latest_finalized_run(&cfg.data_home)?;
    let (run_id, manifest): (String, Manifest) = match &cfg.run_id {
        Some(id) => (id.clone(), read_manifest(&cfg.data_home, id)?),
        None => latest.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "no finalized runs under {} — set LS_REPORT_RUN or run a backtest first",
                cfg.data_home.display()
            )
        })?,
    };
    let defaulted = cfg.run_id.is_none();
    let profit_target_r = manifest.params.profit_target_r;

    let decisions_path = cfg.data_home.join("runs").join(&run_id).join(DECISIONS_FILE);
    let envelopes = read_envelopes(&decisions_path)
        .map_err(|e| anyhow::anyhow!("reading {}: {e:?}", decisions_path.display()))?;

    // Partition the stream (KTD2): breakout envelopes keyed for the join,
    // exit-kind envelopes as trade candidates.
    struct BreakoutRec {
        range_high: f64,
        range_low: f64,
        breakout_price: f64,
    }
    struct ExitRec {
        key: (String, NaiveDate),
        kind: SignalKind,
        mfe_r: f64,
        price: Option<f64>,
    }
    let mut breakouts: BTreeMap<(String, NaiveDate), BreakoutRec> = BTreeMap::new();
    let mut exits: Vec<ExitRec> = Vec::new();
    let mut exits_without_mfe = 0usize;
    for e in &envelopes {
        let Some(d) = &e.decision_detail else { continue };
        let key = (d.symbol.clone(), kst_date_of(UnixNanos::from(e.ts_event)));
        match d.kind {
            SignalKind::Breakout => {
                let get = |k: &str| d.values.get(k).copied();
                if let (Some(range_high), Some(range_low), Some(breakout_price)) =
                    (get("range_high"), get("range_low"), get("breakout_price"))
                {
                    breakouts.entry(key).or_insert(BreakoutRec {
                        range_high,
                        range_low,
                        breakout_price,
                    });
                }
            }
            SignalKind::StopHit | SignalKind::Target | SignalKind::TimeExit => {
                match d.values.get("mfe_r").copied() {
                    Some(mfe_r) => exits.push(ExitRec {
                        key,
                        kind: d.kind,
                        mfe_r,
                        price: d.values.get("price").copied(),
                    }),
                    // An exit without mfe_r predates the turn-8 telemetry —
                    // counted, never silently read as 0.
                    None => exits_without_mfe += 1,
                }
            }
            _ => {}
        }
    }
    if exits.is_empty() {
        anyhow::bail!(
            "{}: no mfe_r-bearing exit records ({} exits without mfe_r) — \
             the run predates the turn-8 MFE telemetry or traded nothing",
            decisions_path.display(),
            exits_without_mfe
        );
    }

    // Join exits to breakouts on (symbol, KST session date) (KTD3).
    let mut rows: Vec<TradeRow> = Vec::new();
    let mut joined_keys: std::collections::BTreeSet<(String, NaiveDate)> =
        std::collections::BTreeSet::new();
    let mut orphan_exits = 0usize;
    let mut degenerate_ranges = 0usize;
    for exit in &exits {
        let (strength, win) = match breakouts.get(&exit.key) {
            Some(b) => {
                joined_keys.insert(exit.key.clone());
                let r = b.range_high - b.range_low;
                let strength = if r > 0.0 {
                    Some((b.breakout_price - b.range_high) / r)
                } else {
                    degenerate_ranges += 1;
                    None
                };
                (strength, exit.price.map(|p| p > b.breakout_price))
            }
            None => {
                orphan_exits += 1;
                (None, None)
            }
        };
        rows.push(TradeRow { kind: exit.kind, mfe_r: exit.mfe_r, strength, win });
    }
    let orphan_breakouts = breakouts.keys().filter(|k| !joined_keys.contains(*k)).count();

    // --- Aggregates. ---
    let trades = rows.len();
    let target_exits = rows.iter().filter(|r| r.kind == SignalKind::Target).count();
    let target_exit_share = target_exits as f64 / trades as f64;

    let all_mfe = sorted(rows.iter().map(|r| r.mfe_r).collect());
    // Filtering a sorted sample preserves order — no second sort.
    let positive: Vec<f64> = all_mfe.iter().copied().filter(|m| *m > 0.0).collect();
    let candidate = leg_two_candidate(&positive, profit_target_r);

    let mut lines = Vec::new();
    lines.push(format!(
        "report mfe: run {run_id} (strategy v{}, profit_target_r {profit_target_r:.2}){}",
        manifest.strategy_version,
        if defaulted { " [defaulted: latest finalized]" } else { "" }
    ));
    // R8 / KTD4 / AE3: state the run's stop mode and the MFE denominator it
    // implies, so R-denominated percentiles are never compared across modes
    // unlabeled. Old manifests (no stop_mode key) deserialize to range-low (v9).
    let (stop_mode_label, mfe_denom_label) = match manifest.params.stop_placement() {
        StopMode::RangeLow => ("range-low (v9)", "range-R"),
        StopMode::OrMidpoint => ("or-midpoint", "trade-R (entry − stop)"),
        StopMode::Atr => ("atr", "trade-R (entry − stop)"),
    };
    lines.push(format!(
        "stop mode: {stop_mode_label} — MFE denominated by {mfe_denom_label} (KTD4); \
         compare R-metrics only within one stop mode"
    ));
    // The governance band below is anchored on THIS run's target, but a next
    // `turn` proposes off the LATEST finalized run's params — when they differ,
    // say so rather than letting the band read as the guardrail's answer.
    if let Some((latest_id, latest_m)) = &latest {
        if *latest_id != run_id {
            lines.push(format!(
                "note: latest finalized run is {latest_id} (profit_target_r {:.2}) — the turn \
                 guardrail bands off that value, not this run's",
                latest_m.params.profit_target_r
            ));
        }
    }
    lines.push(format!(
        "records: {trades} mfe-bearing exits, {} breakouts ({orphan_breakouts} breakouts without exit, \
         {orphan_exits} exits without breakout, {degenerate_ranges} degenerate ranges, \
         {exits_without_mfe} exits without mfe_r)",
        breakouts.len()
    ));
    lines.push(format!(
        "target-exit share: {target_exits}/{trades} ({:.1}%) — MFE right-censored at {profit_target_r:.2}R for target exits",
        target_exit_share * 100.0
    ));
    let p = |pct: f64| {
        nearest_rank(&all_mfe, pct).map(|v| format!("{v:.2}")).unwrap_or_else(|| "n/a".to_string())
    };
    lines.push(format!(
        "mfe_r percentiles (n={trades}): p25 {} | p50 {} | p70 {} | p75 {} | p90 {}",
        p(25.0),
        p(50.0),
        p(70.0),
        p(75.0),
        p(90.0)
    ));

    // By exit reason (median = nearest-rank p50, KTD5).
    lines.push("mfe_r by exit reason:".to_string());
    for kind in [SignalKind::StopHit, SignalKind::Target, SignalKind::TimeExit] {
        let sample = sorted(rows.iter().filter(|r| r.kind == kind).map(|r| r.mfe_r).collect());
        let median = nearest_rank(&sample, 50.0)
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "n/a".to_string());
        // An empty sample has no mean — "0.00" would read as a real value.
        let mean_s = if sample.is_empty() {
            "n/a".to_string()
        } else {
            format!("{:.2}", mean(&sample))
        };
        lines.push(format!(
            "  {:<9} n={:<4} median {median}  mean {mean_s}",
            exit_kind_label(kind),
            sample.len(),
        ));
    }

    // By breakout-strength quartile (KTD4) — the entry-filter spec input (R8).
    let mut by_strength: Vec<&TradeRow> = rows.iter().filter(|r| r.strength.is_some()).collect();
    by_strength.sort_by(|a, b| {
        a.strength.partial_cmp(&b.strength).expect("strength is never NaN")
    });
    lines.push(
        "mfe_r by breakout strength quartile (strength = (breakout_price - range_high)/R):"
            .to_string(),
    );
    let n = by_strength.len();
    let bucket_line = |label: &str, bucket: &[&TradeRow]| -> String {
        let lo = bucket.first().and_then(|r| r.strength).unwrap_or(0.0);
        let hi = bucket.last().and_then(|r| r.strength).unwrap_or(0.0);
        let sample = sorted(bucket.iter().map(|r| r.mfe_r).collect());
        let wins: Vec<bool> = bucket.iter().filter_map(|r| r.win).collect();
        let win_share = if wins.is_empty() {
            "n/a".to_string()
        } else {
            format!("{:.1}%", wins.iter().filter(|w| **w).count() as f64 / wins.len() as f64 * 100.0)
        };
        let median = nearest_rank(&sample, 50.0)
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "n/a".to_string());
        format!(
            "  {label} strength [{lo:.3}..{hi:.3}]  n={:<4} win {win_share}  median {median}  mean {:.2}",
            bucket.len(),
            mean(&sample)
        )
    };
    if n == 0 {
        lines.push("  (no joined trades with a non-degenerate range)".to_string());
    } else if n < 4 {
        // Too thin for a quartile cut — one honest bucket instead of a rank
        // formula that would label a single trade "q4".
        lines.push(bucket_line("all (n<4, no quartile cut)", &by_strength));
    } else {
        for q in 0..4usize {
            let (start, end) = (q * n / 4, (q + 1) * n / 4);
            let label = format!("q{}", q + 1);
            lines.push(bucket_line(&label, &by_strength[start..end]));
        }
    }

    // Leg-2 candidate (KTD6).
    match (candidate.value, candidate.p70) {
        (Some(v), Some(p70)) => lines.push(format!(
            "leg-2 candidate: p70(mfe_r > 0, n={}) = {p70:.4} -> {v:.2} (rounded to {CANDIDATE_STEP})",
            positive.len(),
        )),
        _ => lines.push("leg-2 candidate: no positive-MFE trades".to_string()),
    }
    lines.push(format!(
        "governance band off {profit_target_r:.2}: [{:.2}, {:.2}] (proposal-bounds cap {PROPOSAL_BOUNDS_CAP})",
        candidate.band.0, candidate.band.1
    ));
    lines.push(format!(
        "candidate verdict: {}{}",
        candidate.verdict.label(),
        candidate.value.map(|v| format!(" at {v:.2}")).unwrap_or_default()
    ));

    Ok(ReportOutcome {
        run_id,
        profit_target_r,
        trades,
        target_exit_share,
        orphan_breakouts,
        orphan_exits,
        degenerate_ranges,
        candidate,
        lines,
    })
}

// ===========================================================================
// `report sample` — the sample-sufficiency verdict (plan 2026-08-05-001, U2)
// ===========================================================================

/// Two-sided confidence, pinned by KTD11 **before any reading**. Named here and
/// nowhere else so the multiplier cannot be retuned once an answer is visible.
pub const SAMPLE_CONFIDENCE: f64 = 0.95;

/// Statistical power, pinned by KTD11 before any reading.
pub const SAMPLE_POWER: f64 = 0.80;

/// Default bootstrap replicates (`LS_SAMPLE_REPLICATES` overrides).
pub const SAMPLE_REPLICATES: usize = 10_000;

/// Default resampler seed (`LS_SAMPLE_SEED` overrides). Fixed so a recorded
/// interval is re-derivable.
pub const SAMPLE_SEED: u64 = 20_260_805;

/// `report sample` config.
#[derive(Debug, Clone)]
pub struct SampleConfig {
    /// The data home.
    pub data_home: PathBuf,
    /// The run to report on (`LS_REPORT_RUN`); `None` = latest finalized.
    pub run_id: Option<String>,
    /// The frozen margin record (`LS_SAMPLE_MARGIN`); `None` = the committed
    /// `config/sample-margin.json`.
    pub margin_path: Option<PathBuf>,
    /// Bootstrap replicates.
    pub replicates: usize,
    /// Resampler seed.
    pub seed: u64,
}

/// The margin adjudication for the reported run.
#[derive(Debug, Clone, PartialEq)]
pub struct SampleMarginOutcome {
    /// SHA-256 of the margin file the verdict cites.
    pub content_hash: String,
    /// `E[max]` at the frozen trial count and dispersion.
    pub expected_max_null: f64,
    /// The threshold at this run's own bootstrap standard error.
    pub threshold: f64,
    /// The adjudication.
    pub verdict: MarginVerdict,
    /// Whether the margin must be re-derived before it binds (the run's catalog
    /// fingerprint differs from the frozen one).
    pub requires_rederivation: bool,
}

/// One row of the target-effect band (U5): required n is proportional to the
/// inverse square of the target, so a single point estimate hides how sharply
/// the answer moves. The band spans the gross edge's own confidence interval.
#[derive(Debug, Clone, PartialEq)]
pub struct TargetEffectRow {
    /// Where in the interval this target sits.
    pub label: String,
    /// The per-trade effect the row is computed at.
    pub target_effect: f64,
    /// Closed trades required there (design-effect inflated).
    pub required_trades: f64,
    /// Sessions required there, at the observed trades-per-session rate.
    pub required_sessions: f64,
    /// Whether the catalog's coverage supplies them.
    pub reachable: Option<bool>,
}

/// Which unit the trades-per-session rate is denominated in. The rate converts
/// a required TRADE count into a required SESSION count, and that count is
/// compared against calendar coverage — so the two must share a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateBasis {
    /// Calendar sessions inside the source run's own data range. The correct
    /// basis, used whenever the catalog's coverage is readable.
    CalendarSessions {
        /// Sessions the run's data range actually covers.
        in_range_sessions: usize,
    },
    /// Trade-producing sessions only — the optimistic fallback used when the
    /// catalog cannot be read. It overstates the rate (sessions that produced
    /// no trade are excluded from the denominator), so every session count
    /// derived from it is a **lower bound** on the true requirement.
    TradeProducingSessionsFallback,
}

/// The catalog supply probe and its acquisition verdict (U5; R4, R5, R9).
#[derive(Debug, Clone, PartialEq)]
pub struct SupplyOutcome {
    /// The catalog the coverage was read from.
    pub catalog_path: PathBuf,
    /// Distinct KST sessions the catalog's daily bars cover. `None` when the
    /// catalog is absent or unreadable — supply is then UNESTABLISHED, which
    /// fails closed to a stand-down rather than assuming a number.
    pub available_sessions: Option<usize>,
    /// Of those, the ones inside the source run's own data range — the
    /// denominator of [`Self::trades_per_session`].
    pub in_range_sessions: Option<usize>,
    /// Why the catalog could not be read, when it exists but failed. `None`
    /// when the catalog read succeeded or the catalog is simply absent.
    pub catalog_error: Option<String>,
    /// Earliest covered KST session.
    pub first_session: Option<NaiveDate>,
    /// Latest covered KST session.
    pub last_session: Option<NaiveDate>,
    /// Closed trades per **calendar** session in the source run's data range —
    /// the rate that drives every session count and the reachability verdict.
    pub trades_per_session: f64,
    /// Closed trades per **trade-producing** session. Reported for continuity
    /// with the clustering figures; never used for a verdict.
    pub trades_per_trade_session: f64,
    /// Which unit [`Self::trades_per_session`] is denominated in.
    pub rate_basis: RateBasis,
    /// The run's `max_concurrent` — the hard ceiling on trades per session,
    /// whatever the universe width.
    pub max_concurrent: f64,
    /// Sessions required at the pinned target effect.
    pub required_sessions: f64,
    /// Whether the required sessions fit inside the coverage.
    pub reachable: bool,
    /// Sessions short, when they do not.
    pub shortfall_sessions: f64,
}

/// A `report sample` outcome: structured figures for tests + the printed lines.
#[derive(Debug, Clone, PartialEq)]
pub struct SampleOutcome {
    /// The reported run.
    pub run_id: String,
    /// Whether the run was defaulted to the latest finalized one.
    pub defaulted_run: bool,
    /// The catalog fingerprint the figures were read at.
    pub catalog_fingerprint: String,
    /// Closed trades in the run.
    pub closed_trades: usize,
    /// Trade records in the run (closed + open).
    pub trade_records: usize,
    /// Measured session clustering.
    pub clustering: Clustering,
    /// Per-trade **net** r mean and sample sd (KTD4 — the cost-aware series).
    pub net_r_mean: f64,
    /// Per-trade net r sample sd.
    pub net_r_sd: f64,
    /// Per-trade **gross** r mean — the target effect (KTD11: the largest edge
    /// this strategy has demonstrated; targeting more assumes an edge never
    /// observed).
    pub gross_r_mean: f64,
    /// The smallest per-trade edge this sample can distinguish from zero.
    pub minimum_detectable_edge: f64,
    /// The target effect the required count was computed at.
    pub target_effect: f64,
    /// Closed trades required at the target effect, design-effect inflated.
    pub required_trades: f64,
    /// Whether the sample already meets the requirement.
    pub sufficient: bool,
    /// Session-block bootstrap of net RoR.
    pub net_ror: BootstrapOutcome,
    /// Naive, few-cluster (t), and wild-cluster intervals — diagnostics (KTD5).
    pub intervals: Vec<Interval>,
    /// Required trades and sessions across a band of target effects (U5).
    pub band: Vec<TargetEffectRow>,
    /// The catalog supply probe and its acquisition verdict (U5).
    pub supply: SupplyOutcome,
    /// The margin adjudication.
    pub margin: SampleMarginOutcome,
    /// The printed report lines.
    pub lines: Vec<String>,
}

/// One trade, reduced to what the derivation needs.
struct SampleTrade {
    session: NaiveDate,
    net_r: f64,
    gross_r: f64,
    realized_pnl: f64,
    risk_capital: f64,
}

/// Reduce a run's performance artifact to its closed, risk-joined trades, or
/// refuse with the reason. A refusal here is a **missing input**, not a
/// verdict — the verdict itself never fails the command (KTD1).
fn sample_trades(report: &PerformanceReport, run_id: &str) -> anyhow::Result<Vec<SampleTrade>> {
    let records = report.trades.len();
    let closed: Vec<_> = report.trades.iter().filter(|t| t.ts_closed.is_some()).collect();
    if closed.is_empty() {
        anyhow::bail!(
            "run {run_id} has no CLOSED trades ({records} trade record(s) in {PERFORMANCE_FILE}) \
             — the sample-sufficiency derivation needs a realized per-trade distribution, and \
             an empty one is a refusal, not a sample of size zero"
        );
    }
    let mut out = Vec::with_capacity(closed.len());
    for t in &closed {
        let (Some(risk_capital), Some(net_r)) = (t.risk_capital, t.realized_r) else {
            anyhow::bail!(
                "run {run_id} carries closed trades with a null `risk_capital`/`realized_r` \
                 (first: {} closed at {:?}) — this is a PRE-FIELD vintage artifact written \
                 before the entry-risk join (R4) landed, so it has no size-invariant per-trade \
                 series to derive from. Re-run the backtest on the current tree rather than \
                 deriving from the legacy P&L path",
                t.symbol,
                t.ts_closed
            );
        };
        if !(risk_capital > 0.0) || !net_r.is_finite() {
            anyhow::bail!(
                "run {run_id}: closed trade {} carries a degenerate risk_capital ({risk_capital}) \
                 or realized_r ({net_r})",
                t.symbol
            );
        }
        let commission: f64 = t.fills.iter().map(|f| f.commission).sum();
        out.push(SampleTrade {
            session: kst_date_of(UnixNanos::from(t.ts_opened)),
            net_r,
            gross_r: (t.realized_pnl + commission) / risk_capital,
            realized_pnl: t.realized_pnl,
            risk_capital,
        });
    }
    Ok(out)
}

/// Build the sample-sufficiency report for one run (U2; R1, R3, R6, R8).
///
/// Read-only in the strongest sense: it opens the run's `performance.json` and
/// `manifest.json`, writes no run-dir artifact, touches no strategy code and no
/// governed param, and therefore moves neither `strategy_code_hash` nor head
/// identity. Every verdict — insufficient sample, refused margin — is a
/// **successful** completion; only a missing or unusable input fails.
///
/// The staging guard, mirroring the tier report's: the run's KRW P&L builds the
/// distribution, but **no KRW-denominated P&L or expectancy figure reaches the
/// output**. A power question must not be decided by a profitability number.
///
/// The guard is deliberately narrower than "no profitability number at all":
/// net RoR *is* printed, because it is the statistic the margin adjudicates and
/// suppressing it would leave the margin verdict unauditable. What the guard
/// keeps out is the KRW-scale P&L that would invite reading this report as a
/// profitability report. `tests/research_cli.rs` enforces exactly that.
///
/// # Errors
///
/// On an unknown/absent run, an unreadable performance artifact, a run with no
/// closed trades, a pre-field vintage carrying null risk fields, a single-session
/// sample, or an unreadable margin record.
pub async fn report_sample(cfg: &SampleConfig) -> anyhow::Result<SampleOutcome> {
    let defaulted_run = cfg.run_id.is_none();
    let (run_id, manifest): (String, Manifest) = match &cfg.run_id {
        Some(id) => (id.clone(), read_manifest(&cfg.data_home, id)?),
        None => latest_finalized_run(&cfg.data_home)?.ok_or_else(|| {
            anyhow::anyhow!(
                "no finalized runs under {} — set LS_REPORT_RUN or run a backtest first",
                cfg.data_home.display()
            )
        })?,
    };

    let perf_path = cfg.data_home.join("runs").join(&run_id).join(PERFORMANCE_FILE);
    let perf_text = std::fs::read_to_string(&perf_path)
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", perf_path.display()))?;
    let report: PerformanceReport = serde_json::from_str(&perf_text)
        .map_err(|e| anyhow::anyhow!("parsing {}: {e}", perf_path.display()))?;
    let trade_records = report.trades.len();
    let trades = sample_trades(&report, &run_id)?;

    // Session index: the clustering unit, and the resampling block (Q1).
    let mut session_ids: BTreeMap<NaiveDate, usize> = BTreeMap::new();
    for t in &trades {
        let next = session_ids.len();
        session_ids.entry(t.session).or_insert(next);
    }
    let cluster_ids: Vec<usize> = trades.iter().map(|t| session_ids[&t.session]).collect();
    let net_r: Vec<f64> = trades.iter().map(|t| t.net_r).collect();
    let gross_r: Vec<f64> = trades.iter().map(|t| t.gross_r).collect();

    let clus = clustering(&net_r, &cluster_ids)
        .map_err(|e| anyhow::anyhow!("run {run_id}: {e} (the derivation needs ≥2 KST sessions)"))?;
    let net_r_mean = stats::mean(&net_r).map_err(|e| anyhow::anyhow!("run {run_id}: {e}"))?;
    let net_r_sd = stats::sample_sd(&net_r).map_err(|e| anyhow::anyhow!("run {run_id}: {e}"))?;
    let gross_r_mean = stats::mean(&gross_r).map_err(|e| anyhow::anyhow!("run {run_id}: {e}"))?;

    let mde =
        minimum_detectable_edge(net_r_sd, clus.effective_n, SAMPLE_CONFIDENCE, SAMPLE_POWER)
            .map_err(|e| anyhow::anyhow!("run {run_id}: {e}"))?;
    // KTD11's target: the measured gross per-trade edge — the largest this
    // strategy has demonstrated. A non-positive gross edge is not a detectable
    // target at any sample size; that is a VERDICT ("undetectable"), not a
    // failure, so it is reported as an infinite requirement rather than
    // aborting the whole report — the same handling the band rows use.
    let target_effect = gross_r_mean;
    let required = required_trades(
        target_effect,
        net_r_sd,
        clus.design_effect,
        SAMPLE_CONFIDENCE,
        SAMPLE_POWER,
    )
    .unwrap_or(f64::INFINITY);
    let sufficient = (trades.len() as f64) >= required;

    // Session-block bootstrap of net RoR (Σ realized / Σ risk capital).
    let mut by_session: BTreeMap<usize, Block> = BTreeMap::new();
    for (t, id) in trades.iter().zip(&cluster_ids) {
        by_session.entry(*id).or_default().push((t.realized_pnl, t.risk_capital));
    }
    let blocks: Vec<Block> = by_session.into_values().collect();
    let net_ror = block_bootstrap_ratio(&blocks, cfg.replicates, cfg.seed, SAMPLE_CONFIDENCE)
        .map_err(|e| anyhow::anyhow!("run {run_id}: {e}"))?;

    let intervals = vec![
        interval_normal(net_ror.point, net_ror.standard_error, SAMPLE_CONFIDENCE)
            .map_err(|e| anyhow::anyhow!("run {run_id}: {e}"))?,
        interval_t_few_clusters(
            net_ror.point,
            net_ror.standard_error,
            SAMPLE_CONFIDENCE,
            clus.clusters,
        )
        .map_err(|e| anyhow::anyhow!("run {run_id}: {e}"))?,
        wild_cluster_interval(
            &blocks,
            net_ror.standard_error,
            SAMPLE_CONFIDENCE,
            cfg.replicates,
            cfg.seed.wrapping_add(1),
        )
        .map_err(|e| anyhow::anyhow!("run {run_id}: {e}"))?,
    ];

    // ---- U5: the target-effect band, the supply probe, the verdict ---------
    //
    // Required n scales as the inverse square of the target effect, and the
    // pinned +0.0284 R is itself a noisy estimate — so the answer is reported
    // across the gross edge's own confidence interval, not at one point.
    let gross_sd = stats::sample_sd(&gross_r).map_err(|e| anyhow::anyhow!("run {run_id}: {e}"))?;
    let z = stats::two_sided_z(SAMPLE_CONFIDENCE).map_err(|e| anyhow::anyhow!("{e}"))?;
    let se_naive = gross_sd / (trades.len() as f64).sqrt();
    let se_clustered = gross_sd / clus.effective_n.sqrt();

    let mut band_targets: Vec<(String, f64)> = vec![
        (
            format!("gross CI upper, design-effect corrected (SE {se_clustered:.6})"),
            gross_r_mean + z * se_clustered,
        ),
        (format!("gross CI upper, naive (SE {se_naive:.6})"), gross_r_mean + z * se_naive),
        ("gross point estimate — the KTD11 target".to_string(), gross_r_mean),
        (
            format!("gross CI lower, naive (SE {se_naive:.6})"),
            gross_r_mean - z * se_naive,
        ),
        (
            format!("gross CI lower, design-effect corrected (SE {se_clustered:.6})"),
            gross_r_mean - z * se_clustered,
        ),
    ];
    band_targets.sort_by(|a, b| b.1.partial_cmp(&a.1).expect("finite targets"));

    // Supply: read the catalog's coverage rather than assuming it, and read it
    // WITHOUT an operator-supplied expected range — an asserted span across all
    // bar kinds forces a false NO-GO on a healthy catalog. Coverage here is the
    // distinct KST sessions the catalog's DAILY bars actually hold.
    let catalog_path = cfg.data_home.join("catalog");
    let mut catalog_error: Option<String> = None;
    let (available_sessions, in_range_sessions, first_session, last_session) = if catalog_path
        .exists()
    {
        match nautilus_ls::ingest::read_all_bars(&catalog_path).await {
            Ok(bars) => {
                use crate::runner::backtest::{is_daily, kst_date_of as bar_date};
                let mut dates: Vec<NaiveDate> =
                    bars.iter().filter(|b| is_daily(b)).map(bar_date).collect();
                dates.sort_unstable();
                dates.dedup();
                // Sessions inside the SOURCE RUN's own data range — the
                // denominator of the trades-per-session rate below.
                let start = NaiveDate::parse_from_str(manifest.data_range.start.trim(), "%Y%m%d");
                let end = NaiveDate::parse_from_str(manifest.data_range.end.trim(), "%Y%m%d");
                let in_range = match (start, end) {
                    (Ok(s), Ok(e)) => Some(dates.iter().filter(|d| **d >= s && **d <= e).count()),
                    _ => None,
                };
                (Some(dates.len()), in_range, dates.first().copied(), dates.last().copied())
            }
            Err(e) => {
                // Fail closed, but never silently: a corrupt or unreadable
                // catalog must not render the same line as an absent one.
                catalog_error = Some(format!("{e}"));
                (None, None, None, None)
            }
        }
    } else {
        (None, None, None, None)
    };

    // The rate that converts a required TRADE count into a required SESSION
    // count has to be denominated in the same unit as the coverage it is later
    // compared against: **calendar** sessions, not the subset that happened to
    // produce a trade. The head trades on 4.6 of its *trading* sessions but on
    // far fewer of the catalog's *calendar* sessions, and using the former
    // against the latter understates the requirement — the exact unit slip that
    // makes a shortfall look smaller than it is. Both rates are reported; only
    // the calendar rate drives a verdict.
    let trades_per_trade_session = trades.len() as f64 / clus.clusters as f64;
    let (trades_per_session, rate_basis) = match in_range_sessions {
        Some(n) if n > 0 => (
            trades.len() as f64 / n as f64,
            RateBasis::CalendarSessions { in_range_sessions: n },
        ),
        // No readable coverage to denominate against. Fall back to the
        // trade-producing rate and SAY SO — it is the optimistic end, so the
        // shortfall it produces is a lower bound, never a reassurance.
        _ => (trades_per_trade_session, RateBasis::TradeProducingSessionsFallback),
    };

    let required_sessions = (required / trades_per_session).ceil();
    let reachable = available_sessions.is_some_and(|n| required_sessions <= n as f64);
    let shortfall_sessions =
        available_sessions.map_or(required_sessions, |n| (required_sessions - n as f64).max(0.0));
    let max_concurrent = manifest.params.max_concurrent as f64;
    let supply = SupplyOutcome {
        catalog_path: catalog_path.clone(),
        available_sessions,
        in_range_sessions,
        catalog_error: catalog_error.clone(),
        first_session,
        last_session,
        trades_per_session,
        trades_per_trade_session,
        rate_basis,
        max_concurrent,
        required_sessions,
        reachable,
        shortfall_sessions,
    };

    let band: Vec<TargetEffectRow> = band_targets
        .into_iter()
        .map(|(label, target)| {
            let (req_trades, req_sessions, row_reachable) = match required_trades(
                target,
                net_r_sd,
                clus.design_effect,
                SAMPLE_CONFIDENCE,
                SAMPLE_POWER,
            ) {
                Ok(t) => {
                    let s = (t / trades_per_session).ceil();
                    (t, s, available_sessions.map(|n| s <= n as f64))
                }
                // A non-positive target is not detectable at any sample size —
                // reported as infinite rather than silently dropped.
                Err(_) => (f64::INFINITY, f64::INFINITY, Some(false)),
            };
            TargetEffectRow {
                label,
                target_effect: target,
                required_trades: req_trades,
                required_sessions: req_sessions,
                reachable: row_reachable,
            }
        })
        .collect();

    // The frozen margin (U3): loaded, adjudicated, and printed as an explicit
    // pass/fail line, so the bar has a carrier in running code rather than
    // existing only as a document claim.
    let margin_path = cfg.margin_path.clone().unwrap_or_else(margin::frozen_margin_path);
    let LoadedMargin { values: frozen, content_hash } = margin::load(&margin_path)?;
    let expected_max = frozen
        .derived_expected_max_null()
        .map_err(|e| anyhow::anyhow!("margin {}: {e}", margin_path.display()))?;
    let verdict = frozen
        .adjudicate(net_ror.point, net_ror.standard_error, MarginArm::Armed)
        .map_err(|e| anyhow::anyhow!("margin {}: {e}", margin_path.display()))?;
    let requires_rederivation =
        margin::requires_rederivation(&frozen, &manifest.catalog_fingerprint);
    let margin_outcome = SampleMarginOutcome {
        content_hash: content_hash.clone(),
        expected_max_null: expected_max,
        threshold: verdict.threshold,
        verdict,
        requires_rederivation,
    };

    let mut lines = Vec::new();
    lines.push(format!(
        "report sample: run {run_id} (strategy v{}, catalog {})",
        manifest.strategy_version, manifest.catalog_fingerprint
    ));
    lines.push(format!(
        "  run resolution: {}",
        if defaulted_run {
            "DEFAULTED to the latest finalized run (LS_REPORT_RUN unset)"
        } else {
            "LS_REPORT_RUN"
        }
    ));
    lines.push(format!(
        "  data range {}..{} | {} closed trades of {trade_records} records over {} KST sessions",
        manifest.data_range.start,
        manifest.data_range.end,
        trades.len(),
        clus.clusters
    ));

    lines.push("observed dispersion — net, cost-aware (KTD4):".to_string());
    lines.push(format!("  per-trade net r:   mean {net_r_mean:+.6}  sd {net_r_sd:.6}"));
    lines.push(format!("  per-trade gross r: mean {gross_r_mean:+.6}"));

    lines.push("measured clustering (R1):".to_string());
    lines.push(format!(
        "  sessions {} | mean cluster size {:.4} | Kish cluster size {:.4} | ICC {:.6}",
        clus.clusters, clus.mean_cluster_size, clus.kish_cluster_size, clus.icc
    ));
    lines.push(format!(
        "  design effect {:.4} -> effective n {:.2} of {}",
        clus.design_effect,
        clus.effective_n,
        trades.len()
    ));
    lines.push(format!(
        "  FRAGILITY (R3): the design effect is itself estimated from {} clusters; below ~30 \
         clusters it and every interval below inherit that instability",
        clus.clusters
    ));

    lines.push(format!(
        "detectability at {:.0}% confidence / {:.0}% power (KTD11 — pinned before any reading):",
        SAMPLE_CONFIDENCE * 100.0,
        SAMPLE_POWER * 100.0
    ));
    lines.push(format!("  minimum detectable edge: {mde:+.4} R"));
    lines.push(format!(
        "  target effect (measured gross per-trade edge): {target_effect:+.6} R"
    ));
    if required.is_finite() {
        lines.push(format!(
            "  required closed trades: {:.0} (naive {:.0} x design effect {:.4})",
            required.ceil(),
            (required / clus.design_effect).ceil(),
            clus.design_effect
        ));
        lines.push(format!(
            "  VERDICT: sample {} — {} closed trades against {:.0} required at \
             {target_effect:+.6} R",
            if sufficient { "SUFFICIENT" } else { "INSUFFICIENT" },
            trades.len(),
            required.ceil()
        ));
    } else {
        lines.push(
            "  required closed trades: UNDETECTABLE — the measured gross per-trade edge is not \
             positive, so no sample size can distinguish it from zero"
                .to_string(),
        );
        lines.push(format!(
            "  VERDICT: sample INSUFFICIENT — the target effect itself is {target_effect:+.6} R; \
             this is not a sample-size problem and more data cannot fix it"
        ));
    }

    lines.push(format!(
        "net RoR, session-block bootstrap (block = 1 KST session — Q1; {} replicates, seed {}):",
        net_ror.replicates, net_ror.seed
    ));
    lines.push(format!(
        "  point {:+.6} | {:.0}% [{:+.4}, {:+.4}] | SE {:.6} | share of replicates above zero {:.4}",
        net_ror.point,
        SAMPLE_CONFIDENCE * 100.0,
        net_ror.lo,
        net_ror.hi,
        net_ror.standard_error,
        net_ror.p_positive
    ));
    lines.push("few-cluster corrections (KTD5 — diagnostics, NOT the gate):".to_string());
    for i in &intervals {
        lines.push(format!(
            "  {:<40} crit {:.4}  [{:+.4}, {:+.4}]",
            i.label, i.critical_value, i.lo, i.hi
        ));
    }

    // ---- U5 output: band, supply, acquisition verdict ----------------------
    lines.push(format!(
        "required sample across the gross edge's own {:.0}% interval (required n scales as the \
         inverse square of the target, so one point estimate hides the sensitivity):",
        SAMPLE_CONFIDENCE * 100.0
    ));
    lines.push(format!(
        "  {:<13} {:>12} {:>9} {:>7}  {:<16} {}",
        "target", "required n", "sessions", "years*", "within coverage", "where in the interval"
    ));
    for row in &band {
        let fits = match row.reachable {
            Some(true) => "yes",
            Some(false) => "NO",
            None => "coverage unread",
        };
        if row.required_trades.is_finite() {
            lines.push(format!(
                "  {:+.6} R {:>12.0} {:>9.0} {:>7.1}  {fits:<16} {}",
                row.target_effect,
                row.required_trades.ceil(),
                row.required_sessions,
                row.required_sessions / 250.0,
                row.label
            ));
        } else {
            lines.push(format!(
                "  {:+.6} R {:>12} {:>9} {:>7}  {:<16} {} — NON-POSITIVE, undetectable at any \
                 sample size",
                row.target_effect, "n/a", "n/a", "n/a", "n/a", row.label
            ));
        }
    }
    lines.push("  * years at ~250 KRX trading sessions.".to_string());

    lines.push("supply — read from the catalog's own coverage (U5, R5):".to_string());
    match supply.available_sessions {
        Some(n) => lines.push(format!(
            "  catalog {} holds {n} distinct KST daily-bar sessions ({} .. {}) — coverage read, \
             not asserted (no expected range supplied)",
            supply.catalog_path.display(),
            supply.first_session.map_or_else(|| "?".into(), |d| d.format("%Y%m%d").to_string()),
            supply.last_session.map_or_else(|| "?".into(), |d| d.format("%Y%m%d").to_string()),
        )),
        None => lines.push(match &supply.catalog_error {
            Some(e) => format!(
                "  catalog {} EXISTS but could not be read ({e}) — supply UNESTABLISHED",
                supply.catalog_path.display()
            ),
            None => format!(
                "  catalog {} is absent — supply UNESTABLISHED",
                supply.catalog_path.display()
            ),
        }),
    }
    match supply.rate_basis {
        RateBasis::CalendarSessions { in_range_sessions } => lines.push(format!(
            "  rate {trades_per_session:.4} closed trades per CALENDAR session ({} closed trades \
             over {in_range_sessions} sessions the run's range {}..{} covers). The head trades on \
             only {} of them, at {:.4} per trade-producing session — that higher rate is the \
             clustering unit and is NOT what a session requirement may be divided by.",
            trades.len(),
            manifest.data_range.start,
            manifest.data_range.end,
            clus.clusters,
            supply.trades_per_trade_session
        )),
        RateBasis::TradeProducingSessionsFallback => lines.push(format!(
            "  rate {trades_per_session:.4} closed trades per TRADE-PRODUCING session — the \
             coverage needed to denominate calendar sessions is unreadable, so this is the \
             OPTIMISTIC fallback and every session count below is a LOWER BOUND on the true \
             requirement"
        )),
    }
    lines.push(format!(
        "  run max_concurrent {max_concurrent:.0} caps the per-session trade count at \
         {max_concurrent:.0} regardless of universe width"
    ));
    lines.push(format!(
        "  required sessions at the target effect {target_effect:+.6} R: {:.0}",
        supply.required_sessions
    ));

    if supply.reachable {
        lines.push(format!(
            "ACQUISITION VERDICT: REACHABLE at {target_effect:+.6} R — the recommended range is \
             {:.0} KST sessions (~{:.1} years), against {} covered.",
            supply.required_sessions,
            supply.required_sessions / 250.0,
            supply.available_sessions.unwrap_or(0)
        ));
    } else {
        lines.push(format!(
            "ACQUISITION VERDICT: STAND DOWN at {target_effect:+.6} R — {:.0} sessions required, \
             {} covered, SHORTFALL {:.0} sessions (~{:.1} years).",
            supply.required_sessions,
            supply
                .available_sessions
                .map_or_else(|| "UNESTABLISHED".to_string(), |n| n.to_string()),
            supply.shortfall_sessions,
            supply.shortfall_sessions / 250.0
        ));
    }
    lines.push(
        "  The acquisition path is a FRESH CATALOG at a wider lookback, not an incremental \
         extension: `accumulate` never fetches below the watermark. Its cost therefore includes \
         a moved catalog fingerprint, a moved universe hash and a moved data range — and with \
         them the loss of comparability with every prior measurement, this one included (AE3)."
            .to_string(),
    );
    lines.push(format!(
        "  Lengthen HISTORY, not breadth: at ICC {:.4} and cluster size {:.4}, added sessions \
         raise effective n roughly in proportion while added breadth adds trades inside blocks \
         already held — and max_concurrent {max_concurrent:.0} caps how much breadth converts at \
         all.",
        clus.icc, clus.kish_cluster_size
    ));
    lines.push(
        "  This turn stops at the verdict and executes NO acquisition and NO ingest (KTD7, R8)."
            .to_string(),
    );

    lines.push(format!(
        "margin — frozen {} (sha256 {}):",
        frozen.frozen_utc, margin_outcome.content_hash
    ));
    lines.push(format!("  rule: {}", frozen.rule));
    lines.push(format!(
        "  E[max | N={} trials, sigma={:.8}] = {:.8}",
        frozen.trial_count, frozen.cross_trial_sd, margin_outcome.expected_max_null
    ));
    lines.push(format!(
        "  threshold at this run's SE {:.6} = {:+.6}; candidate net RoR {:+.6}",
        net_ror.standard_error, margin_outcome.threshold, net_ror.point
    ));
    lines.push(format!(
        "  MARGIN VERDICT: {}",
        if margin_outcome.verdict.clears {
            "CLEARED — evidence exceeds the trials-corrected threshold"
        } else {
            "REFUSED — evidence does not exceed the trials-corrected threshold"
        }
    ));
    lines.push(if margin_outcome.requires_rederivation {
        format!(
            "  RE-DERIVATION REQUIRED: this run's catalog {} differs from the frozen {} — the \
             margin must be re-derived before it binds (AE3)",
            manifest.catalog_fingerprint, frozen.provenance.catalog_fingerprint
        )
    } else {
        "  catalog fingerprint matches the frozen one — the margin binds as recorded".to_string()
    });

    Ok(SampleOutcome {
        run_id,
        defaulted_run,
        catalog_fingerprint: manifest.catalog_fingerprint.clone(),
        closed_trades: trades.len(),
        trade_records,
        clustering: clus,
        net_r_mean,
        net_r_sd,
        gross_r_mean,
        minimum_detectable_edge: mde,
        target_effect,
        required_trades: required,
        sufficient,
        net_ror,
        intervals,
        band,
        supply,
        margin: margin_outcome,
        lines,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::agent::context::AgentContext;
    use crate::agent::envelope::{
        to_jsonl, DecisionDetail, DecisionEnvelope, DecisionTrigger, SignalKind,
    };
    use crate::artifacts::{RunSource, MANIFEST_FILE};
    use crate::params::OrbParams;

    fn vals(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    /// 10:00 KST on the given date, as UTC unix ns (KST = UTC+9 → 01:00Z).
    fn ts_kst(y: i32, m: u32, d: u32) -> u64 {
        Utc.with_ymd_and_hms(y, m, d, 1, 0, 0).unwrap().timestamp_nanos_opt().unwrap() as u64
    }

    fn envelope(symbol: &str, ts: u64, kind: SignalKind, values: BTreeMap<String, f64>) -> DecisionEnvelope {
        DecisionEnvelope::telemetry(
            ts,
            DecisionTrigger::Manual { reason: "test".to_string() },
            DecisionDetail::transition(symbol, kind, values),
            AgentContext::telemetry("orb", 9, BTreeMap::new(), BTreeMap::new()),
        )
    }

    fn breakout(symbol: &str, ts: u64, high: f64, low: f64, price: f64) -> DecisionEnvelope {
        envelope(
            symbol,
            ts,
            SignalKind::Breakout,
            vals(&[("range_high", high), ("range_low", low), ("breakout_price", price)]),
        )
    }

    fn exit(symbol: &str, ts: u64, kind: SignalKind, mfe_r: f64, price: f64) -> DecisionEnvelope {
        envelope(symbol, ts, kind, vals(&[("mfe_r", mfe_r), ("price", price), ("qty", 10.0)]))
    }

    /// Write a synthetic finalized run (manifest + decisions.jsonl) into
    /// `<data_home>/runs/<run_id>/`.
    fn write_run(
        data_home: &Path,
        run_id: &str,
        profit_target_r: f64,
        envelopes: &[DecisionEnvelope],
    ) {
        let dir = data_home.join("runs").join(run_id);
        std::fs::create_dir_all(&dir).unwrap();
        let mut params = OrbParams::default();
        params.strategy_version = 9;
        params.profit_target_r = profit_target_r;
        let manifest = Manifest {
            run_id: run_id.to_string(),
            source: RunSource::Backtest,
            strategy_id: "orb".to_string(),
            strategy_version: 9,
            params,
            data_range: crate::artifacts::manifest::DataRange {
                start: "20260601".to_string(),
                end: "20260630".to_string(),
            },
            catalog_fingerprint: "fp".to_string(),
            universe_hash: "uh".to_string(),
            strategy_code_hash: "ch".to_string(),
            lab_src_fingerprint: None,
            checkpoint_hash: None,
            universe_metadata_hash: None,
            dispatch: None,
            created_utc: "2026-07-10T00:00:00+00:00".to_string(),
        };
        std::fs::write(dir.join(MANIFEST_FILE), serde_json::to_string(&manifest).unwrap())
            .unwrap();
        std::fs::write(dir.join(DECISIONS_FILE), to_jsonl(envelopes).unwrap()).unwrap();
    }

    /// Write a synthetic finalized run whose manifest carries a non-default
    /// `stop_mode` (U5), for the report's stop-mode labeling tests.
    fn write_run_stop_mode(
        data_home: &Path,
        run_id: &str,
        stop_mode: f64,
        envelopes: &[DecisionEnvelope],
    ) {
        write_run(data_home, run_id, 1.0, envelopes);
        // Rewrite the manifest with the stop_mode set — the only field the
        // labeling reads; the decisions are untouched.
        let dir = data_home.join("runs").join(run_id);
        let mut manifest: Manifest =
            serde_json::from_str(&std::fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap()).unwrap();
        manifest.params.stop_mode = stop_mode;
        std::fs::write(dir.join(MANIFEST_FILE), serde_json::to_string(&manifest).unwrap()).unwrap();
    }

    fn cfg(data_home: &Path, run_id: &str) -> ReportConfig {
        ReportConfig {
            data_home: data_home.to_path_buf(),
            run_id: Some(run_id.to_string()),
        }
    }

    #[test]
    fn nearest_rank_at_odd_and_even_counts() {
        // Odd: n=5 → p50 rank ceil(2.5)=3 → third value.
        let odd = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(nearest_rank(&odd, 50.0), Some(3.0));
        assert_eq!(nearest_rank(&odd, 70.0), Some(4.0), "rank ceil(3.5)=4");
        assert_eq!(nearest_rank(&odd, 90.0), Some(5.0));
        // Even: n=4 → p50 rank ceil(2.0)=2 → second value (nearest-rank, not
        // interpolated).
        let even = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(nearest_rank(&even, 50.0), Some(2.0));
        assert_eq!(nearest_rank(&even, 75.0), Some(3.0));
        // Single element: every percentile is that element.
        assert_eq!(nearest_rank(&[7.0], 25.0), Some(7.0));
        assert_eq!(nearest_rank(&[7.0], 90.0), Some(7.0));
        // Empty: no value, never a panic.
        assert_eq!(nearest_rank(&[], 50.0), None);
        // p0 clamps to the first element rather than underflowing rank 0.
        assert_eq!(nearest_rank(&even, 0.0), Some(1.0));
    }

    #[test]
    fn happy_path_joins_two_symbols_across_two_sessions() {
        let tmp = tempfile::tempdir().unwrap();
        let d1 = ts_kst(2026, 6, 1);
        let d2 = ts_kst(2026, 6, 2);
        let envs = vec![
            // Session 1: A wins at target, B stops out.
            breakout("A.XKRX", d1, 100.0, 90.0, 101.0),
            exit("A.XKRX", d1 + 3_600_000_000_000, SignalKind::Target, 1.1, 111.0),
            breakout("B.XKRX", d1, 200.0, 180.0, 202.0),
            exit("B.XKRX", d1 + 3_600_000_000_000, SignalKind::StopHit, 0.2, 180.0),
            // Session 2: A time-exits, B time-exits.
            breakout("A.XKRX", d2, 105.0, 95.0, 106.0),
            exit("A.XKRX", d2 + 3_600_000_000_000, SignalKind::TimeExit, 0.5, 108.0),
            breakout("B.XKRX", d2, 210.0, 190.0, 211.0),
            exit("B.XKRX", d2 + 3_600_000_000_000, SignalKind::TimeExit, 0.7, 214.0),
        ];
        write_run(tmp.path(), "run-v9", 1.0, &envs);
        let out = report_mfe(&cfg(tmp.path(), "run-v9")).unwrap();

        assert_eq!(out.trades, 4);
        assert_eq!(out.orphan_breakouts, 0);
        assert_eq!(out.orphan_exits, 0);
        assert_eq!(out.degenerate_ranges, 0);
        assert!((out.target_exit_share - 0.25).abs() < 1e-9);
        // Percentiles over sorted [0.2, 0.5, 0.7, 1.1]: nearest-rank p50 = 0.5.
        let pct_line = out.lines.iter().find(|l| l.contains("percentiles")).unwrap();
        assert!(pct_line.contains("p50 0.50"), "{pct_line}");
        assert!(pct_line.contains("p90 1.10"), "{pct_line}");
        // By-exit-reason medians: single-sample kinds report their own value.
        let stop_line = out.lines.iter().find(|l| l.trim_start().starts_with("stop_hit")).unwrap();
        assert!(stop_line.contains("median 0.20"), "{stop_line}");
        let te_line = out.lines.iter().find(|l| l.trim_start().starts_with("time_exit")).unwrap();
        assert!(te_line.contains("n=2"), "{te_line}");
        assert!(te_line.contains("median 0.50"), "nearest-rank p50 of [0.5, 0.7]: {te_line}");
    }

    #[test]
    fn report_labels_range_low_default_for_old_manifest() {
        // U5 / AE3: a v9-era manifest has no stop_mode key → serde default 0.0 →
        // the report prints the range-low label and a range-R MFE denominator.
        let tmp = tempfile::tempdir().unwrap();
        let d = ts_kst(2026, 6, 1);
        let envs = vec![
            breakout("A.XKRX", d, 100.0, 90.0, 101.0),
            exit("A.XKRX", d + 1, SignalKind::TimeExit, 0.5, 100.0),
        ];
        write_run(tmp.path(), "run-v9", 1.0, &envs);
        let out = report_mfe(&cfg(tmp.path(), "run-v9")).unwrap();
        let mode_line = out.lines.iter().find(|l| l.starts_with("stop mode:")).unwrap();
        assert!(mode_line.contains("range-low (v9)"), "{mode_line}");
        assert!(mode_line.contains("range-R"), "range-R denominator: {mode_line}");
    }

    #[test]
    fn report_labels_midpoint_stop_mode() {
        // U5 / AE3: stop_mode 1.0 → the report prints the or-midpoint label and a
        // trade-R MFE denominator, so cross-mode R-metrics can't be compared blind.
        let tmp = tempfile::tempdir().unwrap();
        let d = ts_kst(2026, 6, 1);
        let envs = vec![
            breakout("A.XKRX", d, 100.0, 90.0, 101.0),
            exit("A.XKRX", d + 1, SignalKind::TimeExit, 0.5, 100.0),
        ];
        write_run_stop_mode(tmp.path(), "run-mid", 1.0, &envs);
        let out = report_mfe(&cfg(tmp.path(), "run-mid")).unwrap();
        let mode_line = out.lines.iter().find(|l| l.starts_with("stop mode:")).unwrap();
        assert!(mode_line.contains("or-midpoint"), "{mode_line}");
        assert!(mode_line.contains("trade-R"), "trade-R denominator: {mode_line}");
    }

    #[test]
    fn report_labels_atr_stop_mode() {
        // U5: stop_mode 2.0 → the report prints the atr label and a trade-R denominator.
        let tmp = tempfile::tempdir().unwrap();
        let d = ts_kst(2026, 6, 1);
        let envs = vec![
            breakout("A.XKRX", d, 100.0, 90.0, 101.0),
            exit("A.XKRX", d + 1, SignalKind::TimeExit, 0.5, 100.0),
        ];
        write_run_stop_mode(tmp.path(), "run-atr", 2.0, &envs);
        let out = report_mfe(&cfg(tmp.path(), "run-atr")).unwrap();
        let mode_line = out.lines.iter().find(|l| l.starts_with("stop mode:")).unwrap();
        assert!(mode_line.contains("atr"), "{mode_line}");
        assert!(mode_line.contains("trade-R"), "trade-R denominator: {mode_line}");
    }

    #[test]
    fn candidate_in_band_is_runnable() {
        // Source target 1.5 (v10-like): band [0.75, 2.25]. Positive MFE sample
        // with p70 = 1.23 → rounds to 1.25, in-band, not within 0.05 of 1.5.
        let tmp = tempfile::tempdir().unwrap();
        let mfes = [0.3, 0.6, 0.9, 1.23, 1.4];
        let envs: Vec<DecisionEnvelope> = mfes
            .iter()
            .enumerate()
            .flat_map(|(i, m)| {
                let sym = format!("S{i}.XKRX");
                let ts = ts_kst(2026, 6, 1 + i as u32);
                vec![
                    breakout(&sym, ts, 100.0, 90.0, 101.0),
                    exit(&sym, ts + 1, SignalKind::TimeExit, *m, 100.0),
                ]
            })
            .collect();
        write_run(tmp.path(), "run-v10", 1.5, &envs);
        let out = report_mfe(&cfg(tmp.path(), "run-v10")).unwrap();
        // n=5 positives → p70 rank ceil(3.5)=4 → 1.23 → 1.25.
        assert_eq!(out.candidate.verdict, CandidateVerdict::Runnable);
        assert!((out.candidate.value.unwrap() - 1.25).abs() < 1e-9);
        assert!(out.lines.iter().any(|l| l.contains("RUNNABLE")), "{:?}", out.lines);
    }

    #[test]
    fn candidate_out_of_band_prints_no_run_recommendation() {
        // Source target 1.5: band [0.75, 2.25]. p70 (nearest-rank ceil(2.1)=3)
        // = 2.6 → out of band (AE3).
        let tmp = tempfile::tempdir().unwrap();
        let mfes = [2.4, 2.5, 2.6];
        let envs: Vec<DecisionEnvelope> = mfes
            .iter()
            .enumerate()
            .flat_map(|(i, m)| {
                let sym = format!("S{i}.XKRX");
                let ts = ts_kst(2026, 6, 1 + i as u32);
                vec![
                    breakout(&sym, ts, 100.0, 90.0, 101.0),
                    exit(&sym, ts + 1, SignalKind::TimeExit, *m, 100.0),
                ]
            })
            .collect();
        write_run(tmp.path(), "run-v10", 1.5, &envs);
        let out = report_mfe(&cfg(tmp.path(), "run-v10")).unwrap();
        assert_eq!(out.candidate.verdict, CandidateVerdict::OutOfBand);
        assert!((out.candidate.value.unwrap() - 2.6).abs() < 1e-9);
        assert!(out.lines.iter().any(|l| l.contains("OUT-OF-BAND")), "{:?}", out.lines);
    }

    #[test]
    fn candidate_pinned_at_source_target_is_right_censored() {
        // Source target 1.0; the distribution truncates there: p70 rounds to
        // 1.0 (within one 0.05 step) → right-censored, no informative candidate.
        let tmp = tempfile::tempdir().unwrap();
        let mfes = [0.4, 0.98, 1.0, 1.02, 1.03];
        let envs: Vec<DecisionEnvelope> = mfes
            .iter()
            .enumerate()
            .flat_map(|(i, m)| {
                let sym = format!("S{i}.XKRX");
                let ts = ts_kst(2026, 6, 1 + i as u32);
                vec![
                    breakout(&sym, ts, 100.0, 90.0, 101.0),
                    exit(&sym, ts + 1, SignalKind::Target, *m, 110.0),
                ]
            })
            .collect();
        write_run(tmp.path(), "run-v9", 1.0, &envs);
        let out = report_mfe(&cfg(tmp.path(), "run-v9")).unwrap();
        assert_eq!(out.candidate.verdict, CandidateVerdict::RightCensored);
        assert!(out.lines.iter().any(|l| l.contains("RIGHT-CENSORED")), "{:?}", out.lines);
        // The censoring evidence is printed: target-exit share on its own line.
        assert!(
            out.lines.iter().any(|l| l.contains("target-exit share: 5/5")),
            "{:?}",
            out.lines
        );
    }

    #[test]
    fn orphan_breakout_is_counted_and_excluded() {
        let tmp = tempfile::tempdir().unwrap();
        let d = ts_kst(2026, 6, 1);
        let envs = vec![
            breakout("A.XKRX", d, 100.0, 90.0, 101.0),
            exit("A.XKRX", d + 1, SignalKind::TimeExit, 0.5, 102.0),
            // B broke out but was sizing-rejected: no exit envelope.
            breakout("B.XKRX", d, 200.0, 180.0, 202.0),
        ];
        write_run(tmp.path(), "run-v9", 1.0, &envs);
        let out = report_mfe(&cfg(tmp.path(), "run-v9")).unwrap();
        assert_eq!(out.trades, 1);
        assert_eq!(out.orphan_breakouts, 1);
        assert!(
            out.lines.iter().any(|l| l.contains("1 breakouts without exit")),
            "{:?}",
            out.lines
        );
    }

    #[test]
    fn orphan_exit_keeps_mfe_stats_but_no_strength() {
        // An exit with no joinable breakout stays in the percentile sample but
        // is excluded from the strength buckets.
        let tmp = tempfile::tempdir().unwrap();
        let d = ts_kst(2026, 6, 1);
        let envs = vec![exit("A.XKRX", d, SignalKind::TimeExit, 0.5, 102.0)];
        write_run(tmp.path(), "run-v9", 1.0, &envs);
        let out = report_mfe(&cfg(tmp.path(), "run-v9")).unwrap();
        assert_eq!(out.trades, 1);
        assert_eq!(out.orphan_exits, 1);
        let pct_line = out.lines.iter().find(|l| l.contains("percentiles")).unwrap();
        assert!(pct_line.contains("n=1"), "{pct_line}");
    }

    #[test]
    fn degenerate_range_is_excluded_from_strength_buckets() {
        let tmp = tempfile::tempdir().unwrap();
        let d = ts_kst(2026, 6, 1);
        let envs = vec![
            // range_high == range_low → R = 0 → degenerate.
            breakout("A.XKRX", d, 100.0, 100.0, 101.0),
            exit("A.XKRX", d + 1, SignalKind::TimeExit, 0.5, 102.0),
        ];
        write_run(tmp.path(), "run-v9", 1.0, &envs);
        let out = report_mfe(&cfg(tmp.path(), "run-v9")).unwrap();
        assert_eq!(out.degenerate_ranges, 1);
        assert!(
            out.lines.iter().any(|l| l.contains("1 degenerate ranges")),
            "{:?}",
            out.lines
        );
        assert!(
            out.lines
                .iter()
                .any(|l| l.contains("no joined trades with a non-degenerate range")),
            "{:?}",
            out.lines
        );
    }

    #[test]
    fn single_trade_run_reports_without_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let d = ts_kst(2026, 6, 1);
        let envs = vec![
            breakout("A.XKRX", d, 100.0, 90.0, 101.0),
            exit("A.XKRX", d + 1, SignalKind::Target, 1.02, 110.0),
        ];
        write_run(tmp.path(), "run-v9", 1.0, &envs);
        let out = report_mfe(&cfg(tmp.path(), "run-v9")).unwrap();
        assert_eq!(out.trades, 1);
        // p70 of [1.02] = 1.02 → rounds to 1.0 → censored at the 1.0 target.
        assert_eq!(out.candidate.verdict, CandidateVerdict::RightCensored);
    }

    #[test]
    fn missing_decisions_file_is_a_clean_failure() {
        let tmp = tempfile::tempdir().unwrap();
        // Manifest exists, decisions.jsonl does not.
        write_run(tmp.path(), "run-v9", 1.0, &[]);
        std::fs::remove_file(tmp.path().join("runs/run-v9").join(DECISIONS_FILE)).unwrap();
        let err = report_mfe(&cfg(tmp.path(), "run-v9")).unwrap_err();
        assert!(err.to_string().contains("decisions.jsonl"), "{err}");
    }

    #[test]
    fn empty_decisions_file_is_a_clean_failure() {
        let tmp = tempfile::tempdir().unwrap();
        write_run(tmp.path(), "run-v9", 1.0, &[]);
        let err = report_mfe(&cfg(tmp.path(), "run-v9")).unwrap_err();
        assert!(err.to_string().contains("no mfe_r-bearing exit records"), "{err}");
    }

    #[test]
    fn unknown_run_id_is_a_clean_failure() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("runs")).unwrap();
        let err = report_mfe(&cfg(tmp.path(), "nope-v1")).unwrap_err();
        assert!(err.to_string().contains("manifest.json"), "{err}");
    }

    #[test]
    fn absent_run_id_defaults_to_latest_finalized_run() {
        let tmp = tempfile::tempdir().unwrap();
        let d = ts_kst(2026, 6, 1);
        let old = vec![
            breakout("A.XKRX", d, 100.0, 90.0, 101.0),
            exit("A.XKRX", d + 1, SignalKind::TimeExit, 0.3, 102.0),
        ];
        let new = vec![
            breakout("A.XKRX", d, 100.0, 90.0, 101.0),
            exit("A.XKRX", d + 1, SignalKind::TimeExit, 0.9, 102.0),
        ];
        write_run(tmp.path(), "20260101T000000Z-backtest-orb-v2", 1.0, &old);
        write_run(tmp.path(), "20260101T000000Z-backtest-orb-v10", 1.0, &new);
        let out = report_mfe(&ReportConfig {
            data_home: tmp.path().to_path_buf(),
            run_id: None,
        })
        .unwrap();
        // Numeric version ordering: -v10 is the latest, not -v2.
        assert_eq!(out.run_id, "20260101T000000Z-backtest-orb-v10");
    }

    #[test]
    fn strength_quartiles_report_ranges_win_share_and_medians() {
        // Eight trades with distinct strengths 0.1..0.8 (breakout_price 101..108
        // over range 100/90 → strength k/10) and alternating win/loss exits:
        // each rank-quartile holds two trades, one win one loss.
        let tmp = tempfile::tempdir().unwrap();
        let d = ts_kst(2026, 6, 1);
        let mut envs = Vec::new();
        for k in 1..=8u32 {
            let sym = format!("S{k}.XKRX");
            let mfe = k as f64 / 10.0;
            let exit_price = if k % 2 == 0 { 200.0 } else { 50.0 };
            envs.push(breakout(&sym, d, 100.0, 90.0, 100.0 + k as f64));
            envs.push(exit(&sym, d + 1, SignalKind::TimeExit, mfe, exit_price));
        }
        write_run(tmp.path(), "run-v9", 1.0, &envs);
        let out = report_mfe(&cfg(tmp.path(), "run-v9")).unwrap();

        let q1 = out.lines.iter().find(|l| l.contains("q1 strength")).unwrap();
        assert!(q1.contains("[0.100..0.200]"), "{q1}");
        assert!(q1.contains("win 50.0%"), "one win, one loss per bucket: {q1}");
        assert!(q1.contains("median 0.10"), "nearest-rank p50 of [0.1, 0.2]: {q1}");
        assert!(q1.contains("mean 0.15"), "{q1}");
        let q4 = out.lines.iter().find(|l| l.contains("q4 strength")).unwrap();
        assert!(q4.contains("[0.700..0.800]"), "highest-strength bucket last: {q4}");
        assert!(q4.contains("median 0.70") && q4.contains("mean 0.75"), "{q4}");
        for q in ["q1", "q2", "q3", "q4"] {
            assert!(
                out.lines.iter().any(|l| l.contains(&format!("{q} strength")) && l.contains("n=2")),
                "{q} holds exactly 2 of 8 trades: {:?}",
                out.lines
            );
        }
    }

    #[test]
    fn thin_sample_prints_one_bucket_not_a_mislabeled_q4() {
        // Fewer than 4 joined trades cannot fill a quartile cut; the rank
        // formula would label a single trade "q4" — print one honest bucket.
        let tmp = tempfile::tempdir().unwrap();
        let d = ts_kst(2026, 6, 1);
        let envs = vec![
            breakout("A.XKRX", d, 100.0, 90.0, 101.0),
            exit("A.XKRX", d + 1, SignalKind::TimeExit, 0.5, 102.0),
        ];
        write_run(tmp.path(), "run-v9", 1.0, &envs);
        let out = report_mfe(&cfg(tmp.path(), "run-v9")).unwrap();
        assert!(
            out.lines.iter().any(|l| l.contains("all (n<4, no quartile cut) strength")),
            "{:?}",
            out.lines
        );
        assert!(!out.lines.iter().any(|l| l.contains("q4 strength")), "{:?}", out.lines);
    }

    #[test]
    fn no_positive_mfe_yields_no_sample_verdict() {
        // Every trade stopped out without favorable excursion: no candidate.
        let tmp = tempfile::tempdir().unwrap();
        let d1 = ts_kst(2026, 6, 1);
        let d2 = ts_kst(2026, 6, 2);
        let envs = vec![
            breakout("A.XKRX", d1, 100.0, 90.0, 101.0),
            exit("A.XKRX", d1 + 1, SignalKind::StopHit, 0.0, 90.0),
            breakout("A.XKRX", d2, 100.0, 90.0, 101.0),
            exit("A.XKRX", d2 + 1, SignalKind::StopHit, -0.2, 90.0),
        ];
        write_run(tmp.path(), "run-v9", 1.0, &envs);
        let out = report_mfe(&cfg(tmp.path(), "run-v9")).unwrap();
        assert_eq!(out.candidate.verdict, CandidateVerdict::NoSample);
        assert!(out.candidate.value.is_none());
        assert!(
            out.lines.iter().any(|l| l.contains("no positive-MFE trades")),
            "{:?}",
            out.lines
        );
        assert!(out.lines.iter().any(|l| l.contains("NO-SAMPLE")), "{:?}", out.lines);
    }

    #[test]
    fn empty_registry_without_run_id_is_a_clean_failure() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("runs")).unwrap();
        let err = report_mfe(&ReportConfig {
            data_home: tmp.path().to_path_buf(),
            run_id: None,
        })
        .unwrap_err();
        assert!(err.to_string().contains("no finalized runs"), "{err}");
        assert!(err.to_string().contains("LS_REPORT_RUN"), "names the pin var: {err}");
    }

    #[test]
    fn empty_exit_kind_prints_na_mean_not_a_fake_zero() {
        let tmp = tempfile::tempdir().unwrap();
        let d = ts_kst(2026, 6, 1);
        let envs = vec![
            breakout("A.XKRX", d, 100.0, 90.0, 101.0),
            exit("A.XKRX", d + 1, SignalKind::TimeExit, 0.5, 102.0),
        ];
        write_run(tmp.path(), "run-v9", 1.0, &envs);
        let out = report_mfe(&cfg(tmp.path(), "run-v9")).unwrap();
        let stop = out.lines.iter().find(|l| l.trim_start().starts_with("stop_hit")).unwrap();
        assert!(stop.contains("median n/a") && stop.contains("mean n/a"), "{stop}");
    }

    #[test]
    fn pinned_non_latest_run_notes_the_latest_guardrail_anchor() {
        // The band is anchored on the REPORTED run's target, but a next `turn`
        // proposes off the LATEST finalized run's params — reporting an older
        // run must say so, or the band reads as the guardrail's answer.
        let tmp = tempfile::tempdir().unwrap();
        let d = ts_kst(2026, 6, 1);
        let envs = vec![
            breakout("A.XKRX", d, 100.0, 90.0, 101.0),
            exit("A.XKRX", d + 1, SignalKind::TimeExit, 0.5, 102.0),
        ];
        write_run(tmp.path(), "20260101T000000Z-backtest-orb-v9", 1.0, &envs);
        write_run(tmp.path(), "20260101T000000Z-backtest-orb-v10", 1.5, &envs);
        let out = report_mfe(&cfg(tmp.path(), "20260101T000000Z-backtest-orb-v9")).unwrap();
        let note = out.lines.iter().find(|l| l.starts_with("note: latest finalized")).unwrap();
        assert!(note.contains("20260101T000000Z-backtest-orb-v10"), "{note}");
        assert!(note.contains("1.50"), "names the latest run's target: {note}");
        // Reporting the latest run itself carries no note and no defaulted marker.
        let out = report_mfe(&cfg(tmp.path(), "20260101T000000Z-backtest-orb-v10")).unwrap();
        assert!(!out.lines.iter().any(|l| l.starts_with("note: latest")), "{:?}", out.lines);
        assert!(!out.lines[0].contains("defaulted"), "{}", out.lines[0]);
    }

    #[test]
    fn defaulted_run_selection_is_marked_in_the_header() {
        let tmp = tempfile::tempdir().unwrap();
        let d = ts_kst(2026, 6, 1);
        let envs = vec![
            breakout("A.XKRX", d, 100.0, 90.0, 101.0),
            exit("A.XKRX", d + 1, SignalKind::TimeExit, 0.5, 102.0),
        ];
        write_run(tmp.path(), "run-v9", 1.0, &envs);
        let out = report_mfe(&ReportConfig {
            data_home: tmp.path().to_path_buf(),
            run_id: None,
        })
        .unwrap();
        assert!(out.lines[0].contains("[defaulted: latest finalized]"), "{}", out.lines[0]);
    }

    #[test]
    fn exits_without_mfe_are_counted_never_read_as_zero() {
        let tmp = tempfile::tempdir().unwrap();
        let d = ts_kst(2026, 6, 1);
        let envs = vec![
            breakout("A.XKRX", d, 100.0, 90.0, 101.0),
            // A pre-turn-8 exit envelope: no mfe_r in values.
            envelope("A.XKRX", d + 1, SignalKind::TimeExit, vals(&[("price", 102.0)])),
        ];
        write_run(tmp.path(), "run-v8", 1.0, &envs);
        let err = report_mfe(&cfg(tmp.path(), "run-v8")).unwrap_err();
        assert!(err.to_string().contains("1 exits without mfe_r"), "{err}");
    }
}
