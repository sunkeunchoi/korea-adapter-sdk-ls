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

use crate::agent::envelope::SignalKind;
use crate::agent::replay::read_envelopes;
use crate::artifacts::manifest::Manifest;
use crate::artifacts::DECISIONS_FILE;
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
        return LegTwoCandidate { value: None, verdict: CandidateVerdict::NoSample, band };
    };
    let candidate = (p70 / CANDIDATE_STEP).round() * CANDIDATE_STEP;
    let verdict = if (candidate - profit_target_r).abs() <= CANDIDATE_STEP + 1e-9 {
        CandidateVerdict::RightCensored
    } else if candidate < band.0 - 1e-9 || candidate > band.1 + 1e-9 {
        CandidateVerdict::OutOfBand
    } else {
        CandidateVerdict::Runnable
    };
    LegTwoCandidate { value: Some(candidate), verdict, band }
}

/// Build the MFE-distribution report for one run (R5, R7). Fails cleanly on a
/// missing/empty decision stream or an unknown run id; the distribution's
/// *content* never fails the command (the exit code reflects I/O only).
pub fn report_mfe(cfg: &ReportConfig) -> anyhow::Result<ReportOutcome> {
    let (run_id, manifest): (String, Manifest) = match &cfg.run_id {
        Some(id) => (id.clone(), read_manifest(&cfg.data_home, id)?),
        None => latest_finalized_run(&cfg.data_home)?.ok_or_else(|| {
            anyhow::anyhow!(
                "no finalized runs under {} — set LS_REPORT_RUN or run a backtest first",
                cfg.data_home.display()
            )
        })?,
    };
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

    let mut all_mfe: Vec<f64> = rows.iter().map(|r| r.mfe_r).collect();
    all_mfe.sort_by(|a, b| a.partial_cmp(b).expect("mfe_r is never NaN"));
    let mut positive: Vec<f64> = all_mfe.iter().copied().filter(|m| *m > 0.0).collect();
    positive.sort_by(|a, b| a.partial_cmp(b).expect("mfe_r is never NaN"));
    let candidate = leg_two_candidate(&positive, profit_target_r);

    let mut lines = Vec::new();
    lines.push(format!(
        "report mfe: run {run_id} (strategy v{}, profit_target_r {profit_target_r:.2})",
        manifest.strategy_version
    ));
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
        let mut sample: Vec<f64> =
            rows.iter().filter(|r| r.kind == kind).map(|r| r.mfe_r).collect();
        sample.sort_by(|a, b| a.partial_cmp(b).expect("mfe_r is never NaN"));
        let median = nearest_rank(&sample, 50.0)
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "n/a".to_string());
        lines.push(format!(
            "  {:<9} n={:<4} median {median}  mean {:.2}",
            exit_kind_label(kind),
            sample.len(),
            mean(&sample)
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
    if n == 0 {
        lines.push("  (no joined trades with a non-degenerate range)".to_string());
    }
    for q in 0..4usize {
        let (start, end) = (q * n / 4, (q + 1) * n / 4);
        if start >= end {
            continue;
        }
        let bucket = &by_strength[start..end];
        let lo = bucket.first().and_then(|r| r.strength).unwrap_or(0.0);
        let hi = bucket.last().and_then(|r| r.strength).unwrap_or(0.0);
        let mut sample: Vec<f64> = bucket.iter().map(|r| r.mfe_r).collect();
        sample.sort_by(|a, b| a.partial_cmp(b).expect("mfe_r is never NaN"));
        let wins: Vec<bool> = bucket.iter().filter_map(|r| r.win).collect();
        let win_share = if wins.is_empty() {
            "n/a".to_string()
        } else {
            format!("{:.1}%", wins.iter().filter(|w| **w).count() as f64 / wins.len() as f64 * 100.0)
        };
        let median = nearest_rank(&sample, 50.0)
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "n/a".to_string());
        lines.push(format!(
            "  q{} strength [{lo:.3}..{hi:.3}]  n={:<4} win {win_share}  median {median}  mean {:.2}",
            q + 1,
            bucket.len(),
            mean(&sample)
        ));
    }

    // Leg-2 candidate (KTD6).
    match candidate.value {
        Some(v) => lines.push(format!(
            "leg-2 candidate: p70(mfe_r > 0, n={}) = {:.4} -> {v:.2} (rounded to {CANDIDATE_STEP})",
            positive.len(),
            nearest_rank(&positive, 70.0).unwrap_or(0.0)
        )),
        None => lines.push("leg-2 candidate: no positive-MFE trades".to_string()),
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

    use chrono::{NaiveDate, TimeZone, Utc};

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
            created_utc: "2026-07-10T00:00:00+00:00".to_string(),
        };
        std::fs::write(dir.join(MANIFEST_FILE), serde_json::to_string(&manifest).unwrap())
            .unwrap();
        std::fs::write(dir.join(DECISIONS_FILE), to_jsonl(envelopes).unwrap()).unwrap();
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
