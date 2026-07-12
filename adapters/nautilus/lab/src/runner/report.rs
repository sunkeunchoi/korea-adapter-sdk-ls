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
use crate::artifacts::DECISIONS_FILE;
use crate::params::StopMode;
use crate::runner::research::{latest_finalized_run, read_manifest, PROPOSAL_BOUNDS_CAP};

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
fn nearest_rank(sorted: &[f64], pct: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let n = sorted.len();
    let rank = ((pct / 100.0) * n as f64).ceil() as usize;
    Some(sorted[rank.clamp(1, n) - 1])
}

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
            checkpoint_hash: None,
            universe_metadata_hash: None,
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
