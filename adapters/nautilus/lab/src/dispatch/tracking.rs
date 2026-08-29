//! U8 — the tracking-error report via a decision-replay twin (R9, KD6; KTD7, KTD8; AE4).
//!
//! The paper twin is a **decision replay, not a parallel session** (KTD7): it replays the
//! live session's recorded entry decisions (`decisions.jsonl`, decisions held fixed)
//! against the session's catalog bars to produce counterfactual paper fills, and compares
//! them to the live fills in the performance report. This measures *execution* divergence
//! (price deltas, slippage, approximated-fill fraction) at zero extra gateway budget — a
//! parallel paper session would double IGW00201 spend and need a second credential lane.
//!
//! **Size-normalized** (KD6): slippage is per-share, so a rung change (which scales qty,
//! not per-share price) never reads as divergence.
//!
//! **Rung 1 is calibration** (KD6): the report is produced and written from rung 1 but is
//! *reported-not-load-bearing* — no de-escalation input comes from it until the band
//! freezes for rung 2.
//!
//! **Fail-soft** (KTD7): a twin that cannot be produced (missing catalog range, unreadable
//! decisions) yields a *twin-failed* status — the session is not clean (R13) and the
//! failure is an exceedance entry (U9), but the pass never panics.
//!
//! **Sidecar, idempotent** (KTD8): reports live under `<data_home>/dispatch/reports/`,
//! keyed by run id — never inside the immutable finalized run dir. Re-running overwrites
//! the same keyed report, so a twin can be produced later when same-day bars lag.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::agent::envelope::{from_jsonl, SignalKind};
use crate::artifacts::data_quality::DataQualityReport;
use crate::artifacts::performance::PerformanceReport;
use crate::artifacts::scrub;
use crate::artifacts::{DATA_QUALITY_FILE, DECISIONS_FILE, PERFORMANCE_FILE};
use crate::dispatch::chain::DISPATCH_DIR;

/// The report sidecar directory name under `<data_home>/dispatch/`.
pub const REPORTS_DIR: &str = "reports";

/// The twin's status (KTD7, KD6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TwinStatus {
    /// Computed and load-bearing (rung 2+): the band governs de-escalation (R14(c)).
    Computed,
    /// Computed but NOT load-bearing (rung 1 calibration, KD6/AE4): reported, no
    /// de-escalation input.
    ReportedNotLoadBearing,
    /// The twin could not be produced — the session is not clean (R13) and this is an
    /// exceedance entry (U9), but never a limit event and never a panic.
    TwinFailed {
        /// Why the twin failed (scrubbed free text).
        reason: String,
    },
    /// The twin is not yet ATTEMPTABLE: its prerequisite (the post-session ingest
    /// landing the session's own bars) has not happened yet. Distinct from
    /// [`Self::TwinFailed`] on purpose — a failed twin is evidence the session is
    /// not clean and reds the readiness reducer, whereas a pending one is the
    /// ordinary state of every session at the moment it finalizes (the KRX witness
    /// is retrospective). Collapsing the two would red readiness on every live
    /// session forever, which is a self-inflicted de-escalation on a condition no
    /// production path can clear.
    ///
    /// Like `TwinFailed` this is NOT `produced()`, so it never satisfies the
    /// rung-2 cleanliness gate; it says "ask again after the ingest", not "this
    /// session failed".
    TwinPending {
        /// What is still missing (scrubbed free text).
        reason: String,
    },
}

impl TwinStatus {
    /// Whether the twin was produced (either load-bearing or calibration).
    /// Neither a failed nor a PENDING twin counts, so the rung-2 gate stays
    /// fail-closed on both.
    pub fn produced(&self) -> bool {
        matches!(self, TwinStatus::Computed | TwinStatus::ReportedNotLoadBearing)
    }

    /// Whether the twin FAILED, as opposed to merely not being attemptable yet.
    /// This is the readiness reducer's question: a failed twin is evidence about
    /// the session, a pending one is evidence about the clock.
    pub fn failed(&self) -> bool {
        matches!(self, TwinStatus::TwinFailed { .. })
    }
}

/// One entry's paper-vs-live per-share comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolSlippage {
    /// The instrument (`{shcode}.XKRX`).
    pub symbol: String,
    /// The live entry fill price (per share).
    pub live_price: f64,
    /// The counterfactual paper fill price (the decision's intended price, per share).
    pub paper_price: f64,
    /// The entry quantity (for the report's context; slippage is per-share, size-invariant).
    pub qty: f64,
    /// `live_price − paper_price` (per share): positive = paid up vs the decision price.
    pub slippage_per_share: f64,
}

/// The per-session tracking-error report (KTD7, KTD8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackingErrorReport {
    /// The run id this report keys on.
    pub run_id: String,
    /// The rung the session ran at (governs load-bearing status, KD6).
    pub rung: u8,
    /// The twin status.
    pub status: TwinStatus,
    /// The number of matched entries compared.
    pub entries: u64,
    /// Mean per-share slippage across entries (size-normalized).
    pub mean_slippage_per_share: f64,
    /// Max absolute per-share slippage (the worst single divergence).
    pub max_abs_slippage_per_share: f64,
    /// The approximated-fill fraction over the session's live fills (KTD7).
    pub approximated_fraction: f64,
    /// The per-symbol comparisons (empty on twin-failed or a no-entry session).
    pub per_symbol: Vec<SymbolSlippage>,
}

/// The report sidecar directory: `<data_home>/dispatch/reports/`.
pub fn reports_dir(data_home: &Path) -> PathBuf {
    data_home.join(DISPATCH_DIR).join(REPORTS_DIR)
}

/// The sidecar path for a run id.
pub fn report_path(data_home: &Path, run_id: &str) -> PathBuf {
    reports_dir(data_home).join(format!("{run_id}.json"))
}

fn twin_report(run_id: &str, rung: u8, status: TwinStatus) -> TrackingErrorReport {
    TrackingErrorReport {
        run_id: run_id.to_string(),
        rung,
        status,
        entries: 0,
        mean_slippage_per_share: 0.0,
        max_abs_slippage_per_share: 0.0,
        approximated_fraction: 0.0,
        per_symbol: Vec::new(),
    }
}

fn twin_failed(run_id: &str, rung: u8, reason: impl Into<String>) -> TrackingErrorReport {
    twin_report(run_id, rung, TwinStatus::TwinFailed { reason: scrub(&reason.into()) })
}

fn twin_pending(run_id: &str, rung: u8, reason: impl Into<String>) -> TrackingErrorReport {
    twin_report(run_id, rung, TwinStatus::TwinPending { reason: scrub(&reason.into()) })
}

/// Extract the paper-twin fills (decisions held fixed, KTD7): the intended entry price +
/// qty per symbol from each `OrderPlaced` decision. The last OrderPlaced for a symbol wins
/// (ORB enters a symbol once per session; a defensive last-write is harmless).
fn paper_fills(decisions_jsonl: &str) -> anyhow::Result<BTreeMap<String, (f64, f64)>> {
    let envelopes = from_jsonl(decisions_jsonl)?;
    let mut out = BTreeMap::new();
    for e in envelopes {
        if let Some(detail) = &e.decision_detail {
            if detail.kind == SignalKind::OrderPlaced {
                let price = detail.values.get("price").copied();
                let qty = detail.values.get("qty").copied();
                if let (Some(price), Some(qty)) = (price, qty) {
                    out.insert(detail.symbol.clone(), (price, qty));
                }
            }
        }
    }
    Ok(out)
}

/// Produce the tracking-error report for a finalized run (KTD7). Replays the run's entry
/// decisions (the paper-twin fills, decisions held fixed) against the live fills in the
/// performance report; `catalog_has_range` gates twin validity — a missing range yields a
/// twin-PENDING status (never a panic, and never twin-FAILED: see below). Size-normalized
/// (per-share) so rung changes never read as divergence. Idempotent per run id: the caller
/// writes the returned report to the sidecar, overwriting any prior one.
///
/// Rung ≤ 1 → *reported-not-load-bearing* (calibration, KD6/AE4); rung ≥ 2 → *computed*
/// (load-bearing).
pub fn produce_report(
    run_dir: &Path,
    run_id: &str,
    rung: u8,
    catalog_has_range: bool,
) -> TrackingErrorReport {
    // Twin prerequisite: the session's bars must be present in the catalog (F1's
    // post-session ingest). That has NOT happened at the moment a session finalizes
    // — the KRX witness is retrospective — so this arm is the ordinary state of
    // every fresh run, not a failure of it. It returns PENDING rather than FAILED
    // precisely because the readiness reducer treats a failed twin as a safety
    // signal: reporting "failed" here would red readiness on every live session and
    // pin the ladder to probation on a condition no production path can clear.
    if !catalog_has_range {
        return twin_pending(
            run_id,
            rung,
            "the session's catalog range has not landed yet — the twin becomes producible \
             after the post-session ingest, and is re-runnable per run id",
        );
    }

    // The paper twin: intended entry prices from decisions.jsonl (decisions held fixed).
    let decisions_text = match std::fs::read_to_string(run_dir.join(DECISIONS_FILE)) {
        Ok(t) => t,
        Err(e) => return twin_failed(run_id, rung, format!("decisions.jsonl unreadable: {e}")),
    };
    let paper = match paper_fills(&decisions_text) {
        Ok(p) => p,
        Err(e) => return twin_failed(run_id, rung, format!("decisions.jsonl unparseable: {e}")),
    };

    // The live fills: entry prices from the performance report.
    let perf: PerformanceReport = match std::fs::read_to_string(run_dir.join(PERFORMANCE_FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
    {
        Some(p) => p,
        None => return twin_failed(run_id, rung, "performance.json unreadable/unparseable"),
    };

    // The approximated-fill fraction over the session's live fills (KTD7).
    let dq: Option<DataQualityReport> = std::fs::read_to_string(run_dir.join(DATA_QUALITY_FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    let total_fills: u64 = perf.trades.iter().map(|t| t.fills.len() as u64).sum();
    let approximated = dq.as_ref().map(|d| d.price_approximated_fills).unwrap_or(0);
    let approximated_fraction = if total_fills > 0 {
        approximated as f64 / total_fills as f64
    } else {
        0.0
    };

    // Match live entries to their paper-twin fills by symbol (ORB enters once per symbol).
    let mut per_symbol = Vec::new();
    for trade in &perf.trades {
        if let Some((paper_price, _paper_qty)) = paper.get(&trade.symbol) {
            let slippage = trade.avg_px_open - paper_price;
            per_symbol.push(SymbolSlippage {
                symbol: trade.symbol.clone(),
                live_price: trade.avg_px_open,
                paper_price: *paper_price,
                qty: trade.quantity,
                slippage_per_share: slippage,
            });
        }
    }

    let entries = per_symbol.len() as u64;
    let mean = if entries > 0 {
        per_symbol.iter().map(|s| s.slippage_per_share).sum::<f64>() / entries as f64
    } else {
        0.0
    };
    let max_abs = per_symbol
        .iter()
        .map(|s| s.slippage_per_share.abs())
        .fold(0.0_f64, f64::max);

    // Rung 1 is calibration; the band is load-bearing only from rung 2 (KD6, AE4).
    let status = if rung <= 1 {
        TwinStatus::ReportedNotLoadBearing
    } else {
        TwinStatus::Computed
    };

    TrackingErrorReport {
        run_id: run_id.to_string(),
        rung,
        status,
        entries,
        mean_slippage_per_share: mean,
        max_abs_slippage_per_share: max_abs,
        approximated_fraction,
        per_symbol,
    }
}

/// Write a tracking-error report to the sidecar (idempotent overwrite, KTD8), scrubbing
/// the one free-text carrier (the twin-failed `reason`) at write time so a planted secret
/// never lands (the data-quality write-time-scrub discipline — structured facts like
/// symbols and run ids print verbatim, exactly as `universe_snapshot` does). Never writes
/// inside a run dir.
///
/// # Errors
///
/// A directory-create, file-write or rename failure.
pub fn write_report(data_home: &Path, report: &TrackingErrorReport) -> anyhow::Result<PathBuf> {
    let dir = reports_dir(data_home);
    std::fs::create_dir_all(&dir)?;
    let mut scrubbed = report.clone();
    match &mut scrubbed.status {
        TwinStatus::TwinFailed { reason } | TwinStatus::TwinPending { reason } => {
            *reason = scrub(reason);
        }
        TwinStatus::Computed | TwinStatus::ReportedNotLoadBearing => {}
    }
    let path = report_path(data_home, &report.run_id);
    // Atomic tmp+rename, the same idiom `Queue::save` and the ingest checkpoint use.
    // A torn write here is not a benign lost update: `readiness::build_catalog` maps an
    // unreadable report to "no twin failure", so a half-written file would read as an
    // absent one and fail OPEN on a signal the reducer treats as load-bearing.
    let tmp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    std::fs::write(&tmp, serde_json::to_string_pretty(&scrubbed)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Read a run's tracking-error report from the sidecar, if present.
///
/// # Errors
///
/// A read/parse failure of an existing report.
pub fn read_report(data_home: &Path, run_id: &str) -> anyhow::Result<Option<TrackingErrorReport>> {
    let path = report_path(data_home, run_id);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(Some(serde_json::from_str(&text)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::context::AgentContext;
    use crate::agent::envelope::{DecisionDetail, DecisionEnvelope, DecisionTrigger};
    use crate::artifacts::performance::{FillRecord, PerformanceReport, TradeRecord};
    use crate::artifacts::RunWriter;
    use tempfile::TempDir;

    fn vals(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    /// An OrderPlaced telemetry envelope with an intended `price`/`qty` (the paper fill).
    fn order_placed(symbol: &str, price: f64, qty: f64) -> DecisionEnvelope {
        DecisionEnvelope::telemetry(
            1,
            DecisionTrigger::StateChange { description: "breakout".into() },
            DecisionDetail::transition(symbol, SignalKind::OrderPlaced, vals(&[("price", price), ("qty", qty)])),
            AgentContext::telemetry("orb", 30, BTreeMap::new(), BTreeMap::new()),
        )
    }

    fn trade(symbol: &str, avg_px_open: f64, qty: f64) -> TradeRecord {
        TradeRecord {
            symbol: symbol.into(),
            entry_side: "BUY".into(),
            quantity: qty,
            avg_px_open,
            avg_px_close: Some(avg_px_open),
            realized_pnl: 0.0,
            ts_opened: 1,
            ts_closed: Some(2),
            fills: vec![FillRecord {
                ts_event: 1,
                side: "BUY".into(),
                qty,
                price: avg_px_open,
                trade_id: format!("T-{symbol}"),
                commission: 0.0,
            }],
            risk_capital: None,
            realized_r: None,
        }
    }

    /// Stage a finalized run dir with the given trades + OrderPlaced decisions.
    fn stage_run(
        data_home: &Path,
        run_id: &str,
        trades: Vec<TradeRecord>,
        decisions: Vec<DecisionEnvelope>,
        approximated_fills: u64,
    ) -> PathBuf {
        let writer = RunWriter::new(data_home, run_id).unwrap();
        writer.write_performance(&PerformanceReport::assemble(trades, 1_000_000.0)).unwrap();
        writer.write_decisions(&decisions).unwrap();
        let mut dq = DataQualityReport::backtest(vec![], vec![]);
        dq.price_approximated_fills = approximated_fills;
        writer.write_data_quality(&dq).unwrap();
        writer.finalize().unwrap()
    }

    #[test]
    fn deterministic_report_with_zero_deltas_when_live_matches_replay() {
        let tmp = TempDir::new().unwrap();
        let run_dir = stage_run(
            tmp.path(),
            "run-a",
            vec![trade("005930.XKRX", 60_000.0, 10.0)],
            vec![order_placed("005930.XKRX", 60_000.0, 10.0)],
            0,
        );
        // Rung 2 → load-bearing.
        let report = produce_report(&run_dir, "run-a", 2, true);
        assert_eq!(report.status, TwinStatus::Computed);
        assert_eq!(report.entries, 1);
        assert_eq!(report.mean_slippage_per_share, 0.0, "live == replay → zero deltas");
        assert_eq!(report.approximated_fraction, 0.0);
    }

    #[test]
    fn slippage_and_approximated_fraction_are_computed() {
        let tmp = TempDir::new().unwrap();
        // Live filled 50 above the decision price; 1 of 1 fill approximated.
        let run_dir = stage_run(
            tmp.path(),
            "run-b",
            vec![trade("005930.XKRX", 60_050.0, 10.0)],
            vec![order_placed("005930.XKRX", 60_000.0, 10.0)],
            1,
        );
        let report = produce_report(&run_dir, "run-b", 2, true);
        assert_eq!(report.entries, 1);
        assert_eq!(report.mean_slippage_per_share, 50.0);
        assert_eq!(report.max_abs_slippage_per_share, 50.0);
        assert_eq!(report.approximated_fraction, 1.0, "1 of 1 live fill approximated");
    }

    #[test]
    fn missing_catalog_range_is_twin_pending_and_writes_nothing_in_the_run_dir() {
        let tmp = TempDir::new().unwrap();
        let run_dir = stage_run(
            tmp.path(),
            "run-c",
            vec![trade("005930.XKRX", 60_000.0, 10.0)],
            vec![order_placed("005930.XKRX", 60_000.0, 10.0)],
            0,
        );
        let before = std::fs::read(run_dir.join(DATA_QUALITY_FILE)).unwrap();
        let report = produce_report(&run_dir, "run-c", 2, false);
        assert!(
            matches!(report.status, TwinStatus::TwinPending { .. }),
            "a missing range is PENDING, not failed: the post-session ingest has simply not \
             run, and reporting a failure reds the readiness reducer on every live session"
        );
        assert!(!report.status.produced(), "pending still never satisfies the rung-2 gate");
        assert!(!report.status.failed(), "and is not a safety signal");
        // The finalized run dir is byte-identical after the pass (immutable).
        assert_eq!(std::fs::read(run_dir.join(DATA_QUALITY_FILE)).unwrap(), before);
    }

    #[test]
    fn rung_1_report_is_reported_not_load_bearing() {
        // Covers AE4: a rung-1 report exceeding any provisional figure produces no
        // de-escalation input (the status is not-load-bearing).
        let tmp = TempDir::new().unwrap();
        let run_dir = stage_run(
            tmp.path(),
            "run-d",
            vec![trade("005930.XKRX", 70_000.0, 10.0)], // huge slippage vs the decision
            vec![order_placed("005930.XKRX", 60_000.0, 10.0)],
            0,
        );
        let report = produce_report(&run_dir, "run-d", 1, true);
        assert_eq!(report.status, TwinStatus::ReportedNotLoadBearing);
        assert_eq!(report.mean_slippage_per_share, 10_000.0, "computed and reported");
    }

    #[test]
    fn report_lands_in_the_sidecar_and_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let run_dir = stage_run(
            tmp.path(),
            "run-e",
            vec![trade("005930.XKRX", 60_000.0, 10.0)],
            vec![order_placed("005930.XKRX", 60_000.0, 10.0)],
            0,
        );
        let report = produce_report(&run_dir, "run-e", 2, true);
        let p1 = write_report(tmp.path(), &report).unwrap();
        assert!(p1.starts_with(tmp.path().join("dispatch").join("reports")), "sidecar, not the run dir");
        let bytes1 = std::fs::read(&p1).unwrap();
        // Re-running the pass overwrites the same keyed report — no duplicate.
        let report2 = produce_report(&run_dir, "run-e", 2, true);
        let p2 = write_report(tmp.path(), &report2).unwrap();
        assert_eq!(p1, p2);
        assert_eq!(std::fs::read(&p2).unwrap(), bytes1, "idempotent — identical bytes");
        assert_eq!(read_report(tmp.path(), "run-e").unwrap().unwrap(), report);
    }

    #[test]
    fn a_planted_secret_never_reaches_the_report_bytes() {
        let tmp = TempDir::new().unwrap();
        // A planted account-number-like token in the free-text reason is scrubbed at write.
        let report = TrackingErrorReport {
            run_id: "run-f".into(),
            rung: 2,
            status: TwinStatus::TwinFailed { reason: "twin failed: acct 20187511401 unreadable".into() },
            entries: 0,
            mean_slippage_per_share: 0.0,
            max_abs_slippage_per_share: 0.0,
            approximated_fraction: 0.0,
            per_symbol: Vec::new(),
        };
        let path = write_report(tmp.path(), &report).unwrap();
        let bytes = std::fs::read_to_string(&path).unwrap();
        assert!(!bytes.contains("20187511401"), "the planted secret is scrubbed: {bytes}");
    }

    #[test]
    fn unreadable_decisions_is_twin_failed_not_a_panic() {
        let tmp = TempDir::new().unwrap();
        // A run dir with a performance report but a corrupt decisions file.
        let writer = RunWriter::new(tmp.path(), "run-g").unwrap();
        writer.write_performance(&PerformanceReport::assemble(vec![], 1_000_000.0)).unwrap();
        let run_dir = writer.finalize().unwrap();
        // A torn decisions line written into the finalized dir → the twin fails soft.
        std::fs::write(run_dir.join(DECISIONS_FILE), "{not valid json").unwrap();
        let report = produce_report(&run_dir, "run-g", 2, true);
        assert!(matches!(report.status, TwinStatus::TwinFailed { .. }));
    }
}
