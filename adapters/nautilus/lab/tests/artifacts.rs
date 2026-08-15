//! U4 — artifact writer + run registry tests. Offline, no network: a RunWriter
//! stages the four artifacts and finalizes atomically into an append-only registry.

use chrono::{TimeZone, Utc};
use nautilus_ls_lab::agent::envelope::{
    self as envelope, Decision, DecisionDetail, DecisionEnvelope, DecisionTrigger,
};
use nautilus_ls_lab::artifacts::data_quality::DataQualityReport;
use nautilus_ls_lab::artifacts::manifest::{range_fingerprint, universe_hash, DataRange, Manifest};
use nautilus_ls_lab::artifacts::performance::{FillRecord, PerformanceReport, TradeRecord};
use nautilus_ls_lab::artifacts::{
    self, aborted_runs, list_runs, run_id, RunSource, RunWriter, DATA_QUALITY_FILE, DECISIONS_FILE,
    MANIFEST_FILE, PERFORMANCE_FILE,
};
use nautilus_ls_lab::params::OrbParams;
use std::collections::BTreeMap;
use tempfile::tempdir;

/// One telemetry decision envelope (the universe-accept decision the retired signal
/// log used to record).
fn telemetry_envelope() -> DecisionEnvelope {
    DecisionEnvelope::telemetry(
        1,
        DecisionTrigger::StateChange { description: "universe selection scan".to_string() },
        DecisionDetail::universe("005930.XKRX", Decision::Accept, None, BTreeMap::new()),
        OrbParams::default().telemetry_context(BTreeMap::new()),
    )
}

fn fixed_run_id(source: RunSource, version: u32) -> String {
    let start = Utc.with_ymd_and_hms(2024, 1, 5, 9, 0, 0).unwrap();
    run_id(start, source, "orb", version)
}

fn manifest(run_id: &str, source: RunSource, params: OrbParams) -> Manifest {
    Manifest {
        run_id: run_id.to_string(),
        source,
        strategy_id: params.strategy_id.clone(),
        strategy_version: params.strategy_version,
        params,
        data_range: DataRange { start: "20240102".into(), end: "20240105".into() },
        catalog_fingerprint: range_fingerprint(&[], 0, u64::MAX),
        universe_hash: universe_hash(&["005930.XKRX".to_string()]),
        strategy_code_hash: String::new(),
        lab_src_fingerprint: None,
        checkpoint_hash: None,
        universe_metadata_hash: None,
        dispatch: None,
        daily_params: None,
        created_utc: "2024-01-05T09:00:00Z".into(),
    }
}

fn trade(symbol: &str, pnl: f64, exec_px: f64) -> TradeRecord {
    TradeRecord {
        symbol: symbol.to_string(),
        entry_side: "BUY".into(),
        quantity: 10.0,
        avg_px_open: exec_px,
        avg_px_close: Some(exec_px + pnl / 10.0),
        realized_pnl: pnl,
        ts_opened: 1,
        ts_closed: Some(2),
        fills: vec![FillRecord {
            ts_event: 1,
            side: "BUY".into(),
            qty: 10.0,
            price: exec_px,
            trade_id: "POLL-1001-10".into(),
            commission: 0.0,
        }],
        risk_capital: None,
        realized_r: None,
    }
}

/// Happy path: a scripted run produces all four parseable files; the registry gains
/// exactly one immutable run dir.
#[test]
fn writes_four_artifacts_and_finalizes() {
    let dir = tempdir().unwrap();
    let data = dir.path();
    let id = fixed_run_id(RunSource::Backtest, 0);
    let writer = RunWriter::new(data, &id).unwrap();

    let params = OrbParams::default();
    writer.write_manifest(&manifest(&id, RunSource::Backtest, params.clone())).unwrap();
    writer.write_performance(&PerformanceReport::assemble(vec![trade("005930.XKRX", 100.0, 60_000.0)], 1_000_000.0)).unwrap();
    writer.write_data_quality(&DataQualityReport::backtest(vec!["005930.XKRX".into()], vec!["005930.XKRX".into()])).unwrap();
    writer.write_decisions(&[telemetry_envelope()]).unwrap();
    let run_dir = writer.finalize().unwrap();

    for f in [MANIFEST_FILE, PERFORMANCE_FILE, DATA_QUALITY_FILE, DECISIONS_FILE] {
        assert!(run_dir.join(f).exists(), "{f} written");
    }
    // Each JSON artifact round-trips through its serde type.
    let m: Manifest = serde_json::from_str(&std::fs::read_to_string(run_dir.join(MANIFEST_FILE)).unwrap()).unwrap();
    assert_eq!(m.run_id, id);
    let _p: PerformanceReport = serde_json::from_str(&std::fs::read_to_string(run_dir.join(PERFORMANCE_FILE)).unwrap()).unwrap();
    let _d: DataQualityReport = serde_json::from_str(&std::fs::read_to_string(run_dir.join(DATA_QUALITY_FILE)).unwrap()).unwrap();
    // The decision stream parses back envelope-for-envelope.
    let back = envelope::from_jsonl(&std::fs::read_to_string(run_dir.join(DECISIONS_FILE)).unwrap()).unwrap();
    assert_eq!(back.len(), 1);
    assert!(back[0].decision_detail.is_some(), "the telemetry detail round-trips");

    // Exactly one finalized run, and no staging directory remains.
    assert_eq!(list_runs(data), vec![id]);
    assert!(aborted_runs(data).is_empty(), "no aborted staging dir after finalize");
}

/// AE1: two runs' manifests differing only in one parameter — the manifests alone
/// identify the delta (no re-run or source diff).
#[test]
fn manifests_isolate_a_single_param_delta() {
    let dir = tempdir().unwrap();
    let data = dir.path();

    let base = OrbParams::default();
    let mut changed = base.clone();
    changed.strategy_version = 1;
    changed.gap_min_pct = 5.0; // the only substantive change

    let id_a = fixed_run_id(RunSource::Backtest, 0);
    let id_b = fixed_run_id(RunSource::Backtest, 1);
    for (id, params) in [(&id_a, base.clone()), (&id_b, changed.clone())] {
        let w = RunWriter::new(data, id).unwrap();
        w.write_manifest(&manifest(id, RunSource::Backtest, params)).unwrap();
        w.finalize().unwrap();
    }

    let ma: Manifest = serde_json::from_str(&std::fs::read_to_string(data.join("runs").join(&id_a).join(MANIFEST_FILE)).unwrap()).unwrap();
    let mb: Manifest = serde_json::from_str(&std::fs::read_to_string(data.join("runs").join(&id_b).join(MANIFEST_FILE)).unwrap()).unwrap();

    // Field-level diff over the parameter sets isolates exactly the changed keys.
    let va = serde_json::to_value(&ma.params).unwrap();
    let vb = serde_json::to_value(&mb.params).unwrap();
    let (oa, ob) = (va.as_object().unwrap(), vb.as_object().unwrap());
    let diff: Vec<&String> = oa.keys().filter(|k| oa.get(*k) != ob.get(*k)).collect();
    assert_eq!(diff.len(), 2, "only strategy_version + gap_min_pct differ: {diff:?}");
    assert!(diff.iter().any(|k| *k == "gap_min_pct"));
    assert!(diff.iter().any(|k| *k == "strategy_version"));
}

/// Error path: a writer dropped mid-run leaves no finalized dir; the next writer
/// reports the aborted staging dir and does not reuse it.
#[test]
fn dropped_writer_leaves_reported_aborted_run() {
    let dir = tempdir().unwrap();
    let data = dir.path();
    let id = fixed_run_id(RunSource::Backtest, 0);

    {
        let w = RunWriter::new(data, &id).unwrap();
        w.write_manifest(&manifest(&id, RunSource::Backtest, OrbParams::default())).unwrap();
        // Dropped here WITHOUT finalize → staging dir remains.
    }
    assert!(list_runs(data).is_empty(), "no finalized run from an aborted writer");
    assert_eq!(aborted_runs(data), vec![id.clone()], "the aborted staging dir is reported");

    // A new writer for the SAME id refuses to silently reuse the staging dir.
    let err = RunWriter::new(data, &id).unwrap_err();
    assert!(err.to_string().contains("never silently reused"), "err: {err}");
}

/// Append-only: a second run never touches the first.
#[test]
fn second_run_is_append_only() {
    let dir = tempdir().unwrap();
    let data = dir.path();

    let id_a = fixed_run_id(RunSource::Backtest, 0);
    let w = RunWriter::new(data, &id_a).unwrap();
    w.write_manifest(&manifest(&id_a, RunSource::Backtest, OrbParams::default())).unwrap();
    w.finalize().unwrap();
    let a_mtime = std::fs::metadata(data.join("runs").join(&id_a).join(MANIFEST_FILE)).unwrap().modified().unwrap();

    let id_b = fixed_run_id(RunSource::Live, 0);
    let w = RunWriter::new(data, &id_b).unwrap();
    w.write_manifest(&manifest(&id_b, RunSource::Live, OrbParams::default())).unwrap();
    w.finalize().unwrap();

    // The first run's file is byte-untouched.
    let a_mtime2 = std::fs::metadata(data.join("runs").join(&id_a).join(MANIFEST_FILE)).unwrap().modified().unwrap();
    assert_eq!(a_mtime, a_mtime2);
    assert_eq!(list_runs(data).len(), 2);

    // Re-finalizing over an existing run id is refused.
    let err = RunWriter::new(data, &id_a).unwrap_err();
    assert!(err.to_string().contains("append-only"), "err: {err}");
}

/// AE5 (report half): a fill whose execution price differs from the order limit lands
/// in performance.json at the execution price.
#[test]
fn performance_report_carries_exec_price() {
    let dir = tempdir().unwrap();
    let data = dir.path();
    let id = fixed_run_id(RunSource::Live, 0);
    let w = RunWriter::new(data, &id).unwrap();
    // Order limit was 60_000; the fill executed at 60_050.
    let report = PerformanceReport::assemble(vec![trade("005930.XKRX", 500.0, 60_050.0)], 1_000_000.0);
    w.write_performance(&report).unwrap();
    let run_dir = w.finalize().unwrap();

    let p: PerformanceReport = serde_json::from_str(&std::fs::read_to_string(run_dir.join(PERFORMANCE_FILE)).unwrap()).unwrap();
    assert_eq!(p.trades[0].fills[0].price, 60_050.0, "the fill's exec price is reported");
    assert_eq!(p.trades[0].avg_px_open, 60_050.0);
}

/// The universe snapshot list round-trips through data_quality.json (R7/KTD8: the
/// composition lives in the data-quality report).
#[test]
fn universe_snapshot_round_trips() {
    let dir = tempdir().unwrap();
    let data = dir.path();
    let id = fixed_run_id(RunSource::Backtest, 0);
    let w = RunWriter::new(data, &id).unwrap();
    let snapshot = vec!["005930.XKRX".to_string(), "000660.XKRX".to_string()];
    let shifted = vec!["000660.XKRX".to_string()];
    w.write_data_quality(&DataQualityReport::backtest(snapshot.clone(), shifted.clone())).unwrap();
    let run_dir = w.finalize().unwrap();

    let d: DataQualityReport = serde_json::from_str(&std::fs::read_to_string(run_dir.join(DATA_QUALITY_FILE)).unwrap()).unwrap();
    assert_eq!(d.universe_snapshot, snapshot);
    assert_eq!(d.adjustment_basis_shift_symbols, shifted, "per-symbol shift list round-trips (R7)");
}

/// Security: a run seeded with an account-number-bearing error string yields
/// artifacts with the account token scrubbed (the free-text carrier passes the
/// adapter's scrub at write time).
#[test]
fn free_text_observations_are_scrubbed() {
    let dir = tempdir().unwrap();
    let data = dir.path();
    let id = fixed_run_id(RunSource::Live, 0);
    let w = RunWriter::new(data, &id).unwrap();

    let mut report = DataQualityReport::backtest(vec!["005930.XKRX".into()], Vec::new());
    report.observations = vec!["poll error on account 20187511401: rejected".to_string()];
    w.write_data_quality(&report).unwrap();
    let run_dir = w.finalize().unwrap();

    let contents = std::fs::read_to_string(run_dir.join(DATA_QUALITY_FILE)).unwrap();
    assert!(!contents.contains("20187511401"), "the account number is scrubbed: {contents}");
    assert!(contents.contains("***"), "the scrub redaction is present");
    // Controlled identifiers (symbols) survive — the agent needs them.
    assert!(contents.contains("005930.XKRX"));
}

/// The scrub applied at write time is the adapter's own predicate (no re-implementation).
#[test]
fn scrub_delegates_to_adapter() {
    assert_eq!(artifacts::scrub("acct=20187511401 ok"), "acct=*** ok");
    assert_eq!(artifacts::scrub("ordno=12345 qty=10"), "ordno=12345 qty=10");
}

/// write_decisions applies the same free-text scrub the cross-run recorder does:
/// an account-like token in a free-text field is masked on disk and the line
/// still parses (UUIDs untouched).
#[test]
fn write_decisions_scrubs_free_text_on_disk() {
    let dir = tempdir().unwrap();
    let data = dir.path();
    let id = fixed_run_id(RunSource::Backtest, 7);
    let w = RunWriter::new(data, &id).unwrap();

    let mut e = telemetry_envelope();
    e.trigger = DecisionTrigger::Manual {
        reason: "operator probe on account 20187511401".to_string(),
    };
    w.write_decisions(&[e]).unwrap();
    let run_dir = w.finalize().unwrap();

    let contents = std::fs::read_to_string(run_dir.join(DECISIONS_FILE)).unwrap();
    assert!(!contents.contains("20187511401"), "account token masked: {contents}");
    assert!(contents.contains("***"), "redaction present");
    let back = envelope::from_jsonl(&contents).unwrap();
    assert_eq!(back.len(), 1, "the scrubbed line still parses");
}

/// U5 back-compat: a data-quality file written before the teardown-retries / dedup-hits
/// fields existed still deserializes, and the absent fields read as absent (None), not
/// zero — a real zero-retry live teardown records Some(0), distinct from "never ran".
#[test]
fn pre_u5_data_quality_deserializes_with_absent_fields() {
    let json = serde_json::json!({
        "coverage_gaps": [],
        "shallow_history_symbols": [],
        "adjustment_basis_shift_symbols": [],
        "price_approximated_fills": 0,
        "reconcile_advised": [],
        "universe_snapshot": []
    });
    let dq: DataQualityReport = serde_json::from_value(json).unwrap();
    assert_eq!(dq.teardown_retries, None, "absent -> None, not zero");
    assert_eq!(dq.dedup_hits, None);
}

/// U5 back-compat: a manifest written before the dispatch-linkage field existed still
/// deserializes with `dispatch = None`.
#[test]
fn pre_u5_manifest_deserializes_without_dispatch_link() {
    let json = serde_json::json!({
        "run_id": "20260101T000000Z-live-orb-v1",
        "source": "live",
        "strategy_id": "orb",
        "strategy_version": 1,
        "params": OrbParams::default(),
        "data_range": { "start": "20260101", "end": "20260101" },
        "catalog_fingerprint": "abc",
        "universe_hash": "def",
        "strategy_code_hash": "ghi",
        "created_utc": "2026-01-01T00:00:00Z"
    });
    let m: Manifest = serde_json::from_value(json).unwrap();
    assert!(m.dispatch.is_none(), "absent dispatch link -> None");
}
