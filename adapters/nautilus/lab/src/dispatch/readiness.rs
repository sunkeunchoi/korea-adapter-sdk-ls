//! U9 — the exceedance pass + the readiness reducer (R10, R11; KTD8).
//!
//! The trailing-sessions evidence loop. The **exceedance catalog** trends a pre-registered
//! set of conditions across the window from fields the artifacts already emit plus the ones
//! U5's finalize added (teardown retries, dedup hits) and U8's sidecar (twin-failed): each
//! per-session, plus `.tmp-` residue (R14(f)) and per-check deferral counts read from the
//! chain (R3). The **readiness reducer** computes a green/red verdict over the trailing K
//! **live-lane** sessions — the window admits only runs carrying `dispatch_id` + a live
//! `trading_env` (KTD3), excluding backtest/research runs, never tolerating them as absent.
//!
//! The verdict is **read-only** over the registry, the chain, and the report sidecar
//! (KTD8) — the reducer never writes. A red verdict does not deadlock the ladder: the gate
//! forces the session to **rung-1 probation** (effective_rung = 1) rather than refusing
//! (R11); the wiring into the gate's check list lives in [`crate::dispatch::checks`].
//!
//! Every "safe" verdict fails toward not-safe: a dedup hit on a real emission, a
//! teardown needing more than one retry, a twin-failed session, or `.tmp-` residue reds the
//! window regardless of the numeric thresholds.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::artifacts::data_quality::DataQualityReport;
use crate::artifacts::{aborted_runs, list_runs, DATA_QUALITY_FILE};
use crate::dispatch::chain::{ChainRecord, RecordKind};
use crate::dispatch::prereg::{ExceedanceThresholds, PreRegistration};
use crate::dispatch::tracking::{read_report, TwinStatus};
use crate::runner::research::read_manifest;

/// The readiness verdict the gate consumes as one of its checks (R11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessVerdict {
    /// The trailing window is healthy — the session may run at its authorized rung.
    Green,
    /// The trailing window tripped a threshold or a safety signal — the session runs at
    /// rung-1 probation (effective_rung = 1), never refused (R11).
    Red,
    /// No trailing window is frozen yet (no pre-registration / no K, or too few live-lane
    /// sessions) — readiness is not load-bearing; the session runs at its authorized rung.
    NotEvaluated,
}

impl ReadinessVerdict {
    /// Whether this verdict forces rung-1 probation.
    pub fn is_probation(self) -> bool {
        self == ReadinessVerdict::Red
    }
}

/// One trailing session's exceedance contributions (from its artifacts + the sidecar + the
/// chain). Every field is a count so the reducer can trend them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionExceedance {
    /// The run id.
    pub run_id: String,
    /// Reconcile-advised conditions in the session's data-quality report.
    pub reconcile_advised: u64,
    /// Coverage-gap conditions.
    pub coverage_gaps: u64,
    /// Approximated-fill count.
    pub approximated_fills: u64,
    /// The fail-closed teardown's cancel-retry count (0 if absent; >1 is a limit event).
    pub teardown_retries: u64,
    /// Order-dedup hits over the session (a real-emission hit is a limit event).
    pub dedup_hits: u64,
    /// Whether the session's tracking-error twin failed (from the sidecar).
    pub twin_failed: bool,
    /// Per-check deferral count recorded on the session's dispatch record (R3).
    pub deferrals: u64,
}

/// The exceedance catalog over the trailing K live-lane sessions plus registry-wide `.tmp-`
/// residue (R14(f)).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ExceedanceCatalog {
    /// One entry per trailing live-lane session (oldest → newest).
    pub sessions: Vec<SessionExceedance>,
    /// Aborted `.tmp-` staging directories in the registry (R14(f) residue).
    pub aborted_runs: u64,
}

impl ExceedanceCatalog {
    fn sum(&self, f: impl Fn(&SessionExceedance) -> u64) -> u64 {
        self.sessions.iter().map(f).sum()
    }
}

/// Whether a manifest is a live-lane session that may enter the readiness window (KTD3):
/// it must carry the dispatch linkage AND a live `trading_env`. Backtest/research runs and
/// paper pre-checks are excluded, never tolerated as absent.
fn is_live_lane(manifest: &crate::artifacts::manifest::Manifest) -> bool {
    manifest
        .dispatch
        .as_ref()
        .is_some_and(|d| d.trading_env.eq_ignore_ascii_case("live"))
}

/// The trailing K live-lane run ids (oldest → newest). Runs are ordered by run id (the UTC
/// stamp prefix sorts chronologically); only live-lane runs are admitted, then the last K
/// are taken.
pub fn qualifying_window(data_home: &Path, k: usize) -> Vec<String> {
    let mut live: Vec<String> = list_runs(data_home)
        .into_iter()
        .filter(|rid| read_manifest(data_home, rid).map(|m| is_live_lane(&m)).unwrap_or(false))
        .collect();
    live.sort();
    if live.len() > k {
        live.split_off(live.len() - k)
    } else {
        live
    }
}

/// Deferral counts keyed by session-dispatch record id, read from the chain (R3). A run's
/// manifest cites its `dispatch_id`, so the reducer joins a run to its recorded deferrals.
fn deferrals_by_dispatch(chain_records: &[ChainRecord]) -> std::collections::BTreeMap<String, u64> {
    let mut out = std::collections::BTreeMap::new();
    for r in chain_records {
        if let RecordKind::SessionDispatch(s) = &r.body.kind {
            out.insert(r.body.record_id.clone(), s.deferrals.len() as u64);
        }
    }
    out
}

/// Read a finalized run's data-quality report, if present and parseable (absent/corrupt →
/// `None`, tolerated — never a crash).
fn read_dq(data_home: &Path, run_id: &str) -> Option<DataQualityReport> {
    let path = data_home.join("runs").join(run_id).join(DATA_QUALITY_FILE);
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok())
}

/// Build the exceedance catalog over the trailing K live-lane sessions (read-only, KTD8).
pub fn build_catalog(data_home: &Path, chain_records: &[ChainRecord], k: usize) -> ExceedanceCatalog {
    let deferrals = deferrals_by_dispatch(chain_records);
    let mut sessions = Vec::new();
    for run_id in qualifying_window(data_home, k) {
        let Ok(manifest) = read_manifest(data_home, &run_id) else { continue };
        let dq = read_dq(data_home, &run_id);
        let twin_failed = read_report(data_home, &run_id)
            .ok()
            .flatten()
            .map(|r| matches!(r.status, TwinStatus::TwinFailed { .. }))
            .unwrap_or(false);
        let deferral_count = manifest
            .dispatch
            .as_ref()
            .and_then(|d| deferrals.get(&d.dispatch_id).copied())
            .unwrap_or(0);
        sessions.push(SessionExceedance {
            run_id: run_id.clone(),
            reconcile_advised: dq.as_ref().map(|d| d.reconcile_advised.len() as u64).unwrap_or(0),
            coverage_gaps: dq.as_ref().map(|d| d.coverage_gaps.len() as u64).unwrap_or(0),
            approximated_fills: dq.as_ref().map(|d| d.price_approximated_fills).unwrap_or(0),
            // Absent fields (a pre-U5 artifact) read as absent → 0, never crash.
            teardown_retries: dq.as_ref().and_then(|d| d.teardown_retries).unwrap_or(0),
            dedup_hits: dq.as_ref().and_then(|d| d.dedup_hits).unwrap_or(0),
            twin_failed,
            deferrals: deferral_count,
        });
    }
    ExceedanceCatalog { sessions, aborted_runs: aborted_runs(data_home).len() as u64 }
}

/// The reducer's verdict over an exceedance catalog (R11). Red when any pre-registered
/// numeric threshold is exceeded OR any safety signal is present (a dedup hit on a real
/// emission, a teardown needing more than one retry, a twin-failed session, or `.tmp-`
/// residue) — every safe verdict fails toward not-safe. Otherwise green.
pub fn readiness_verdict(catalog: &ExceedanceCatalog, thresholds: &ExceedanceThresholds) -> ReadinessVerdict {
    let over = |total: u64, limit: Option<u32>| limit.is_some_and(|l| total > l as u64);
    let threshold_tripped = over(catalog.sum(|s| s.reconcile_advised), thresholds.max_reconcile_advised)
        || over(catalog.sum(|s| s.deferrals), thresholds.max_deferrals)
        || over(catalog.sum(|s| s.coverage_gaps), thresholds.max_coverage_gaps);
    let safety_tripped = catalog.aborted_runs > 0
        || catalog.sessions.iter().any(|s| {
            s.dedup_hits > 0 || s.teardown_retries > 1 || s.twin_failed
        });
    if threshold_tripped || safety_tripped {
        ReadinessVerdict::Red
    } else {
        ReadinessVerdict::Green
    }
}

/// Compute the readiness verdict for a dispatch (R11, KTD8): read-only over the registry,
/// the chain, and the report sidecar. Returns [`ReadinessVerdict::NotEvaluated`] when no
/// pre-registration K is frozen or the window has fewer than K live-lane sessions (a window
/// that has not yet accumulated cannot be load-bearing). The exceedance catalog is returned
/// alongside for the record/report.
pub fn compute_readiness(
    data_home: &Path,
    chain_records: &[ChainRecord],
    prereg: Option<&PreRegistration>,
) -> (ReadinessVerdict, ExceedanceCatalog) {
    let empty = ExceedanceCatalog { sessions: Vec::new(), aborted_runs: 0 };
    let Some(values) = prereg else { return (ReadinessVerdict::NotEvaluated, empty) };
    let Ok(k) = values.k_window() else { return (ReadinessVerdict::NotEvaluated, empty) };
    let catalog = build_catalog(data_home, chain_records, k as usize);
    // A window that has not yet reached K live-lane sessions is not load-bearing.
    if (catalog.sessions.len() as u32) < k {
        return (ReadinessVerdict::NotEvaluated, catalog);
    }
    let verdict = readiness_verdict(&catalog, &values.exceedance);
    (verdict, catalog)
}

/// A compact, scrubbable one-line summary of the verdict + catalog for the dispatch record
/// (`readiness` field on the session-dispatch, KTD8). Structured counts only — no free text.
pub fn readiness_summary(verdict: ReadinessVerdict, catalog: &ExceedanceCatalog) -> String {
    format!(
        "{:?} over {} live sessions (reconcile={}, deferrals={}, gaps={}, dedup={}, retries>1={}, twin_failed={}, aborted={})",
        verdict,
        catalog.sessions.len(),
        catalog.sum(|s| s.reconcile_advised),
        catalog.sum(|s| s.deferrals),
        catalog.sum(|s| s.coverage_gaps),
        catalog.sessions.iter().filter(|s| s.dedup_hits > 0).count(),
        catalog.sessions.iter().filter(|s| s.teardown_retries > 1).count(),
        catalog.sessions.iter().filter(|s| s.twin_failed).count(),
        catalog.aborted_runs,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::data_quality::DataQualityReport;
    use crate::artifacts::manifest::{universe_hash, DataRange, DispatchLink, Manifest};
    use crate::artifacts::performance::PerformanceReport;
    use crate::artifacts::{RunSource, RunWriter};
    use crate::params::OrbParams;
    use tempfile::TempDir;

    fn stage_run(data_home: &Path, run_id: &str, trading_env: Option<&str>, dq: DataQualityReport) {
        let writer = RunWriter::new(data_home, run_id).unwrap();
        let params = OrbParams::default();
        let dispatch = trading_env.map(|env| DispatchLink {
            dispatch_id: format!("{run_id}-disp"),
            rung: 2,
            rung_fraction: 0.5,
            lane: "cafef00d".into(),
            trading_env: env.into(),
        });
        let manifest = Manifest {
            run_id: run_id.into(),
            source: RunSource::Live,
            strategy_id: params.strategy_id.clone(),
            strategy_version: params.strategy_version,
            params: params.clone(),
            data_range: DataRange { start: "20260716".into(), end: "20260716".into() },
            catalog_fingerprint: String::new(),
            universe_hash: universe_hash(&[]),
            strategy_code_hash: String::new(),
            checkpoint_hash: None,
            universe_metadata_hash: None,
            dispatch,
            created_utc: "2026-07-16T01:00:00Z".into(),
        };
        writer.write_manifest(&manifest).unwrap();
        writer.write_performance(&PerformanceReport::assemble(vec![], 1_000_000.0)).unwrap();
        writer.write_data_quality(&dq).unwrap();
        writer.finalize().unwrap();
    }

    fn clean_dq() -> DataQualityReport {
        let mut dq = DataQualityReport::backtest(vec![], vec![]);
        dq.teardown_retries = Some(0);
        dq.dedup_hits = Some(0);
        dq
    }

    fn prereg(k: u32, max_reconcile: Option<u32>) -> PreRegistration {
        serde_json::from_value(serde_json::json!({
            "version": 1,
            "k_window": k,
            "exceedance": { "max_reconcile_advised": max_reconcile }
        }))
        .unwrap()
    }

    #[test]
    fn only_live_lane_runs_enter_the_window() {
        let tmp = TempDir::new().unwrap();
        stage_run(tmp.path(), "20260716T010000Z-live-orb-v30", Some("live"), clean_dq());
        stage_run(tmp.path(), "20260716T020000Z-live-orb-v30", Some("paper"), clean_dq()); // paper — excluded
        stage_run(tmp.path(), "20260716T030000Z-backtest-orb-v30", None, clean_dq()); // no dispatch — excluded
        let window = qualifying_window(tmp.path(), 10);
        assert_eq!(window, vec!["20260716T010000Z-live-orb-v30".to_string()]);
    }

    #[test]
    fn verdict_flips_red_exactly_above_the_threshold() {
        let tmp = TempDir::new().unwrap();
        // Two live sessions, each with one reconcile-advised condition → total 2.
        for h in ["01", "02"] {
            let mut dq = clean_dq();
            dq.reconcile_advised.push(crate::artifacts::data_quality::ReconcileCondition {
                kind: crate::artifacts::data_quality::ReconcileConditionKind::PollInconclusive,
                symbol: "005930".into(),
            });
            stage_run(tmp.path(), &format!("20260716T{h}0000Z-live-orb-v30"), Some("live"), dq);
        }
        // Threshold 2: total 2 is NOT > 2 → green.
        let (v, cat) = compute_readiness(tmp.path(), &[], Some(&prereg(2, Some(2))));
        assert_eq!(v, ReadinessVerdict::Green, "{:?}", cat);
        // Threshold 1: total 2 > 1 → red.
        let (v, _) = compute_readiness(tmp.path(), &[], Some(&prereg(2, Some(1))));
        assert_eq!(v, ReadinessVerdict::Red);
    }

    #[test]
    fn an_aborted_tmp_run_pushes_the_verdict_red() {
        let tmp = TempDir::new().unwrap();
        stage_run(tmp.path(), "20260716T010000Z-live-orb-v30", Some("live"), clean_dq());
        stage_run(tmp.path(), "20260716T020000Z-live-orb-v30", Some("live"), clean_dq());
        // Leave a .tmp- staging dir (an aborted run, R14(f)).
        std::fs::create_dir_all(tmp.path().join("runs").join(".tmp-20260716T030000Z-live-orb-v30")).unwrap();
        let (v, _) = compute_readiness(tmp.path(), &[], Some(&prereg(2, None)));
        assert_eq!(v, ReadinessVerdict::Red, "an aborted .tmp- run reds the window");
    }

    #[test]
    fn a_twin_failed_sidecar_surfaces_and_reds() {
        use crate::dispatch::tracking::{write_report, TrackingErrorReport};
        let tmp = TempDir::new().unwrap();
        let rid = "20260716T010000Z-live-orb-v30";
        stage_run(tmp.path(), rid, Some("live"), clean_dq());
        stage_run(tmp.path(), "20260716T020000Z-live-orb-v30", Some("live"), clean_dq());
        write_report(
            tmp.path(),
            &TrackingErrorReport {
                run_id: rid.into(),
                rung: 2,
                status: TwinStatus::TwinFailed { reason: "missing catalog range".into() },
                entries: 0,
                mean_slippage_per_share: 0.0,
                max_abs_slippage_per_share: 0.0,
                approximated_fraction: 0.0,
                per_symbol: Vec::new(),
            },
        )
        .unwrap();
        let (v, cat) = compute_readiness(tmp.path(), &[], Some(&prereg(2, None)));
        assert!(cat.sessions.iter().any(|s| s.twin_failed), "twin-failed surfaces in the catalog");
        assert_eq!(v, ReadinessVerdict::Red);
    }

    #[test]
    fn below_k_live_sessions_is_not_evaluated_and_old_fieldless_runs_are_tolerated() {
        let tmp = TempDir::new().unwrap();
        // One live session with a pre-U5 data-quality report (no teardown_retries/dedup_hits).
        stage_run(tmp.path(), "20260716T010000Z-live-orb-v30", Some("live"), DataQualityReport::backtest(vec![], vec![]));
        // K = 3 but only 1 live session → not evaluated (window not accumulated).
        let (v, _) = compute_readiness(tmp.path(), &[], Some(&prereg(3, Some(1))));
        assert_eq!(v, ReadinessVerdict::NotEvaluated);
    }

    #[test]
    fn no_prereg_is_not_evaluated() {
        let tmp = TempDir::new().unwrap();
        stage_run(tmp.path(), "20260716T010000Z-live-orb-v30", Some("live"), clean_dq());
        let (v, _) = compute_readiness(tmp.path(), &[], None);
        assert_eq!(v, ReadinessVerdict::NotEvaluated, "no frozen window → readiness not load-bearing");
    }

    #[test]
    fn a_clean_full_window_is_green() {
        let tmp = TempDir::new().unwrap();
        for h in ["01", "02", "03"] {
            stage_run(tmp.path(), &format!("20260716T{h}0000Z-live-orb-v30"), Some("live"), clean_dq());
        }
        let (v, _) = compute_readiness(tmp.path(), &[], Some(&prereg(3, Some(5))));
        assert_eq!(v, ReadinessVerdict::Green);
    }
}
