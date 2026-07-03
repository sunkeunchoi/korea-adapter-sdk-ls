//! U5 — backtest runner tests. Offline: a fixture `ParquetDataCatalog` (built the
//! same way the adapter's own e2e test builds one — wiremock-ingested instruments +
//! directly-written bars) feeds a full ORB backtest that lands a finalized registry
//! run. No credentials, no network beyond the wiremock instrument masters.

use std::path::Path;

use chrono::{TimeZone, Utc};
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::ingest::checkpoint::{Checkpoint, GapReason};
use nautilus_ls::ingest::{
    build_daily_bar, build_minute_bar, write_bars, write_instruments, BarKind,
};
use nautilus_ls::instruments::{InstrumentDomain, InstrumentProvider};
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_ls_lab::artifacts::data_quality::{DataQualityReport, GapReasonKind};
use nautilus_ls_lab::artifacts::manifest::Manifest;
use nautilus_ls_lab::artifacts::performance::PerformanceReport;
use nautilus_ls_lab::agent::replay::read_envelopes;
use nautilus_ls_lab::artifacts::{aborted_runs, list_runs, MANIFEST_FILE, PERFORMANCE_FILE, DECISIONS_FILE, DATA_QUALITY_FILE};
use nautilus_ls_lab::runner::backtest::{run, run_inner, BacktestConfig};
use nautilus_model::data::Bar;
use nautilus_model::identifiers::InstrumentId;
use serde_json::json;
use tempfile::tempdir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn json_response(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_string(body.to_string())
        .insert_header("content-type", "application/json")
}

fn t8430_body() -> serde_json::Value {
    json!({
        "rsp_cd": "00000",
        "t8430OutBlock": [
            { "hname": "삼성전자", "shcode": "005930", "expcode": "KR7005930003",
              "etfgubun": "0", "uplmtprice": "82000", "dnlmtprice": "44000",
              "jnilclose": "63000", "memedan": "1", "recprice": "63000", "gubun": "1" }
        ]
    })
}

fn t9945_body() -> serde_json::Value {
    json!({
        "rsp_cd": "00000",
        "t9945OutBlock": [
            { "hname": "삼성전자", "shcode": "005930", "expcode": "KR7005930003",
              "etfchk": "0", "nxt_chk": "1", "filler": "" }
        ]
    })
}

fn daily_json(date: &str, o: &str, h: &str, l: &str, c: &str, v: &str) -> serde_json::Value {
    json!({ "date": date, "open": o, "high": h, "low": l, "close": c, "jdiff_vol": v,
        "value": "0", "jongchk": "0", "rate": "0", "pricechk": "0", "ratevalue": "0", "sign": "0" })
}

fn minute_json(date: &str, time: &str, o: &str, h: &str, l: &str, c: &str, v: &str) -> serde_json::Value {
    json!({ "date": date, "time": time, "open": o, "high": h, "low": l, "close": c,
        "jdiff_vol": v, "value": "0", "jongchk": "0", "rate": "0", "sign": "0" })
}

/// Build a fixture catalog with one gapping symbol (005930): two daily bars (a +5%
/// gap-up) for the universe scan, and a clean-breakout minute session for 20240105.
async fn build_fixture(data_home: &Path, with_checkpoint_gap: bool) {
    let catalog = data_home.join("catalog");
    let server = MockServer::start().await;
    mount_token(&server).await;
    for (p, tr, body) in [
        ("/stock/etc", "t8430", t8430_body()),
        ("/stock/market-data", "t9945", t9945_body()),
    ] {
        Mock::given(method("POST"))
            .and(path(p))
            .and(header("tr_cd", tr))
            .respond_with(json_response(body))
            .mount(&server)
            .await;
    }
    let sdk = LsSdk::new(mock_config(&server.uri())).unwrap();
    let mut provider = InstrumentProvider::new(sdk.clone());
    provider.load_domain(InstrumentDomain::DomesticEquity).await.unwrap();
    write_instruments(&catalog, provider.all_any()).await.unwrap();

    let id = InstrumentId::from("005930.XKRX");
    let daily_bt = BarKind::Daily.bar_type(id).unwrap();
    // Prior close 60000, today open 63000 → +5% gap.
    let daily: Vec<Bar> = [
        daily_json("20240104", "59000", "60500", "58500", "60000", "1000000"),
        daily_json("20240105", "63000", "64500", "62000", "64000", "1200000"),
    ]
    .iter()
    .map(|r| build_daily_bar(daily_bt, &serde_json::from_value(r.clone()).unwrap()).unwrap().unwrap())
    .collect();
    write_bars(&catalog, daily).await.unwrap();

    let minute_bt = BarKind::Minute(1).bar_type(id).unwrap();
    // Opening range 09:00–09:14 = [62500, 63500]; breakout at 09:20 (h 64000 > 63500);
    // a clean uptrend never breaches the 62500 stop; time-flat exit at 15:00 (sell at
    // the bar low 64900 > the ~64000 entry → a winning trade).
    let minute: Vec<Bar> = [
        minute_json("20240105", "090000", "63000", "63500", "62500", "63200", "1000"),
        minute_json("20240105", "091000", "63200", "63400", "63000", "63300", "1000"),
        minute_json("20240105", "092000", "63300", "64000", "63300", "63900", "1000"),
        minute_json("20240105", "100000", "64000", "64500", "63900", "64400", "1000"),
        minute_json("20240105", "110000", "64400", "65000", "64300", "64900", "1000"),
        minute_json("20240105", "150000", "65000", "65300", "64900", "65100", "1000"),
        // A trailing post-flat bar so the time-exit sell (submitted at 15:00) matches
        // against a following bar — the entry likewise matches on the bar after 09:20.
        minute_json("20240105", "150100", "65100", "65300", "65000", "65200", "1000"),
    ]
    .iter()
    .map(|r| build_minute_bar(minute_bt, &serde_json::from_value(r.clone()).unwrap()).unwrap().unwrap())
    .collect();
    write_bars(&catalog, minute).await.unwrap();

    let mut cp = Checkpoint::default();
    cp.adjusted_prices = true;
    if with_checkpoint_gap {
        cp.record_gap("000660.XKRX", "1-MINUTE", "20240102..20240105", GapReason::EmptyHistory);
    }
    cp.save(&catalog.join("ingest-checkpoint.json")).unwrap();
}

fn cfg(data_home: &Path) -> BacktestConfig {
    BacktestConfig::new(data_home, "20240102", "20240105")
}

fn read_perf(run_dir: &Path) -> PerformanceReport {
    serde_json::from_str(&std::fs::read_to_string(run_dir.join(PERFORMANCE_FILE)).unwrap()).unwrap()
}

/// Happy path: fixture catalog → full run → finalized run with a non-empty decision
/// stream and a performance report showing the completed ORB trade.
#[tokio::test]
async fn full_backtest_lands_a_registry_run() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path(), false).await;

    let start = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
    let outcome = run(cfg(dir.path()), start).await.unwrap();

    for f in [MANIFEST_FILE, PERFORMANCE_FILE, DECISIONS_FILE, DATA_QUALITY_FILE] {
        assert!(outcome.run_dir.join(f).exists(), "{f} present");
    }
    let decisions_path = outcome.run_dir.join(DECISIONS_FILE);
    let decisions = std::fs::read_to_string(&decisions_path).unwrap();
    assert!(decisions.lines().count() >= 3, "non-empty decision stream");
    assert!(decisions.contains("order_placed"), "an entry was recorded");

    // AE3: exactly one envelope per decision cycle — every line parses through the
    // replay loader, and every in-run envelope carries its telemetry detail.
    let envelopes = read_envelopes(&decisions_path).unwrap();
    assert_eq!(
        envelopes.len(),
        decisions.lines().count(),
        "one parseable envelope per decision-log line"
    );
    assert!(
        envelopes.iter().all(|e| e.decision_detail.is_some()),
        "every in-run envelope carries a decision detail"
    );
    // AE3: the signal log is subsumed by decisions.jsonl — no signals.jsonl remains.
    assert!(!outcome.run_dir.join("signals.jsonl").exists(), "subsumed by decisions.jsonl");

    let perf = read_perf(&outcome.run_dir);
    assert_eq!(perf.summary["num_trades"], 1.0, "one completed ORB trade");
    assert!(perf.summary["pnl_total"] > 0.0, "the winning trade is positive");

    // Registry: exactly one finalized run, no staging residue.
    assert_eq!(list_runs(dir.path()), vec![outcome.run_id]);
    assert!(aborted_runs(dir.path()).is_empty());
}

/// Two runs over the same pinned range are deterministic: performance parses equal
/// (run_id / timestamps excluded) and the catalog fingerprint + universe hash match.
#[tokio::test]
async fn repeat_runs_are_deterministic() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path(), false).await;

    let s1 = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
    let s2 = Utc.with_ymd_and_hms(2024, 1, 7, 0, 0, 0).unwrap();
    let o1 = run(cfg(dir.path()), s1).await.unwrap();
    let o2 = run(cfg(dir.path()), s2).await.unwrap();
    assert_ne!(o1.run_id, o2.run_id, "distinct run ids");

    let (p1, p2) = (read_perf(&o1.run_dir), read_perf(&o2.run_dir));
    assert_eq!(p1.trades, p2.trades, "trade ledger identical across runs");
    assert_eq!(p1.summary, p2.summary, "summary identical across runs");

    let m1: Manifest = serde_json::from_str(&std::fs::read_to_string(o1.run_dir.join(MANIFEST_FILE)).unwrap()).unwrap();
    let m2: Manifest = serde_json::from_str(&std::fs::read_to_string(o2.run_dir.join(MANIFEST_FILE)).unwrap()).unwrap();
    assert_eq!(m1.catalog_fingerprint, m2.catalog_fingerprint, "same in-range data → same fingerprint");
    assert_eq!(m1.universe_hash, m2.universe_hash);
}

/// AE4 (backtest half): a coverage gap in the catalog checkpoint is recorded in the
/// run's data-quality report; the run completes.
#[tokio::test]
async fn coverage_gap_is_recorded() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path(), true).await;
    let start = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
    let outcome = run(cfg(dir.path()), start).await.unwrap();

    let dq: DataQualityReport = serde_json::from_str(&std::fs::read_to_string(outcome.run_dir.join(DATA_QUALITY_FILE)).unwrap()).unwrap();
    assert!(!dq.coverage_gaps.is_empty(), "the checkpoint gap is recorded");
    assert_eq!(dq.coverage_gaps[0].reason, GapReasonKind::EmptyFeed);
    assert!(dq.adjustment_basis_splice, "adjusted-price basis surfaced from the checkpoint");
}

/// Error path: a missing catalog exits with a clear error and no registry residue.
#[tokio::test]
async fn missing_catalog_errors_with_no_residue() {
    let dir = tempdir().unwrap();
    let start = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
    let err = run(cfg(dir.path()), start).await.unwrap_err();
    assert!(err.to_string().contains("no catalog"), "err: {err}");
    assert!(list_runs(dir.path()).is_empty());
    assert!(aborted_runs(dir.path()).is_empty(), "no staging residue");
}

/// Error path: a mid-run catalog mutation (in-range) changes the fingerprint between
/// start and finalize → the run fails and leaves no registry residue.
#[tokio::test]
async fn mid_run_catalog_change_aborts_with_no_residue() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path(), false).await;
    let catalog = dir.path().join("catalog");
    let start = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();

    // The hook writes an extra in-range minute bar after the engine run, before the
    // finalize fingerprint re-check.
    let mutate = async {
        let id = InstrumentId::from("005930.XKRX");
        let bt = BarKind::Minute(1).bar_type(id).unwrap();
        let extra = build_minute_bar(bt, &serde_json::from_value(minute_json("20240105", "110000", "64000", "64100", "63900", "64050", "500")).unwrap()).unwrap().unwrap();
        write_bars(&catalog, vec![extra]).await.unwrap();
    };
    let err = run_inner(cfg(dir.path()), start, mutate).await.unwrap_err();
    assert!(err.to_string().contains("catalog changed in-range"), "err: {err}");
    assert!(list_runs(dir.path()).is_empty(), "no finalized run");
    assert!(aborted_runs(dir.path()).is_empty(), "no staging residue");
}

// ---------------------------------------------------------------------------
// U8 — loop-iteration proof: the loop turns once on fixture data (baseline run →
// committed analysis → a parameter change → re-run), the two runs are comparable via
// their manifests (AE1), and the R15 analysis co-location convention holds.
// ---------------------------------------------------------------------------

/// AE1 / Success Criterion 1: two runs whose only substantive change is a parameter
/// (a loop turn) — the manifests alone isolate the delta, no re-run or source diff.
#[tokio::test]
async fn loop_turn_manifest_comparison_isolates_param_delta() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path(), false).await;

    // Turn 1: ORB v0 baseline.
    let s1 = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
    let o1 = run(cfg(dir.path()), s1).await.unwrap();

    // Turn 2: the landed change — a parameter delta with a bumped strategy version
    // (005930's +5% gap still clears the lowered floor, so both runs trade).
    let mut cfg2 = cfg(dir.path());
    cfg2.params.strategy_version = 1;
    cfg2.params.gap_min_pct = 2.0;
    let s2 = Utc.with_ymd_and_hms(2024, 1, 7, 0, 0, 0).unwrap();
    let o2 = run(cfg2, s2).await.unwrap();

    let m1: Manifest = serde_json::from_str(&std::fs::read_to_string(o1.run_dir.join(MANIFEST_FILE)).unwrap()).unwrap();
    let m2: Manifest = serde_json::from_str(&std::fs::read_to_string(o2.run_dir.join(MANIFEST_FILE)).unwrap()).unwrap();

    let va = serde_json::to_value(&m1.params).unwrap();
    let vb = serde_json::to_value(&m2.params).unwrap();
    let (oa, ob) = (va.as_object().unwrap(), vb.as_object().unwrap());
    let diff: Vec<&String> = oa.keys().filter(|k| oa.get(*k) != ob.get(*k)).collect();
    assert_eq!(diff.len(), 2, "manifests isolate exactly the changed params: {diff:?}");
    assert!(diff.iter().any(|k| *k == "gap_min_pct"));
    assert!(diff.iter().any(|k| *k == "strategy_version"));
    // The pinned data range is identical → the range-scoped fingerprints match, so the
    // agent knows the delta is strategy-only, not data drift.
    assert_eq!(m1.catalog_fingerprint, m2.catalog_fingerprint);
    // Same strategy code (only params changed) → identical code hash; the manifest
    // would surface a logic change even without a version bump.
    assert_eq!(m1.strategy_code_hash, m2.strategy_code_hash);
    assert!(!m1.strategy_code_hash.is_empty(), "the strategy code is fingerprinted");
}

/// KTD8 drift test: a third run after a simulated OUT-OF-RANGE ingest keeps the
/// range-scoped fingerprint unchanged (accumulate-forward growth outside the pinned
/// range must not teach the agent to ignore the fingerprint).
#[tokio::test]
async fn out_of_range_ingest_keeps_range_fingerprint() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path(), false).await;
    let catalog = dir.path().join("catalog");

    let s1 = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
    let o1 = run(cfg(dir.path()), s1).await.unwrap();
    let m1: Manifest = serde_json::from_str(&std::fs::read_to_string(o1.run_dir.join(MANIFEST_FILE)).unwrap()).unwrap();

    // Simulate accumulate-forward: append a daily bar for 20240108, OUTSIDE the pinned
    // range (20240102..20240105).
    let id = InstrumentId::from("005930.XKRX");
    let daily_bt = BarKind::Daily.bar_type(id).unwrap();
    let extra = build_daily_bar(daily_bt, &serde_json::from_value(daily_json("20240108", "64000", "65000", "63500", "64800", "1500000")).unwrap()).unwrap().unwrap();
    write_bars(&catalog, vec![extra]).await.unwrap();

    // Re-run with the SAME pinned range → BOTH the range-scoped fingerprint AND the
    // universe hash are unchanged (the universe scan is range-scoped, so out-of-range
    // daily growth cannot silently drift the selection — the KTD8 comparability break
    // three reviewers flagged).
    let s3 = Utc.with_ymd_and_hms(2024, 1, 9, 0, 0, 0).unwrap();
    let o3 = run(cfg(dir.path()), s3).await.unwrap();
    let m3: Manifest = serde_json::from_str(&std::fs::read_to_string(o3.run_dir.join(MANIFEST_FILE)).unwrap()).unwrap();
    assert_eq!(m1.catalog_fingerprint, m3.catalog_fingerprint, "out-of-range growth does not change the range fingerprint");
    assert_eq!(m1.universe_hash, m3.universe_hash, "out-of-range growth does not change the selected universe");
}

/// The sizing/concurrency veto: when the fixed notional cannot afford a single share,
/// the entry is rejected (force_done + an OrderRejectedSizing envelope) and no trade
/// is placed — exercising handle_actions' veto branch end-to-end through the engine.
#[tokio::test]
async fn sizing_veto_rejects_and_records_the_decision() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path(), false).await;

    let mut c = cfg(dir.path());
    c.params.notional_per_position = 100.0; // < the ~63,500 breakout price → qty 0
    let start = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
    let outcome = run(c, start).await.unwrap();

    let decisions = std::fs::read_to_string(outcome.run_dir.join(DECISIONS_FILE)).unwrap();
    assert!(decisions.contains("order_rejected_sizing"), "the entry was vetoed by sizing");
    assert!(decisions.contains("notional_too_small"), "the veto names the notional filter");
    assert!(!decisions.contains("order_placed"), "no order was placed");
    let perf = read_perf(&outcome.run_dir);
    assert_eq!(perf.summary["num_trades"], 0.0, "no trade on a vetoed entry");
}

/// R15 co-location: the committed fixture `analysis.md` placed into a finalized run
/// dir is reported by the registry (the agent's analysis lives beside the runs it
/// analyzed).
#[tokio::test]
async fn analysis_md_co_locates_in_run_dir() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path(), false).await;
    let start = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
    let outcome = run(cfg(dir.path()), start).await.unwrap();

    // Before analysis: none reported.
    assert!(!nautilus_ls_lab::artifacts::run_has_analysis(dir.path(), &outcome.run_id));

    // The agent writes the committed fixture analysis into the finalized run dir.
    let fixture = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/analysis.md")).unwrap();
    std::fs::write(outcome.run_dir.join("analysis.md"), fixture).unwrap();

    assert!(nautilus_ls_lab::artifacts::run_has_analysis(dir.path(), &outcome.run_id), "analysis.md is co-located and reported");
    // The run is still a normal registry member.
    assert!(list_runs(dir.path()).contains(&outcome.run_id));
}

/// Error path: startup is refused while the ingest advisory lock is held.
#[tokio::test]
async fn refused_while_ingest_lock_held() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path(), false).await;
    let catalog = dir.path().join("catalog");
    let _held = AdvisoryLock::acquire(&catalog, LockKind::Ingest).unwrap();

    let start = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
    let err = run(cfg(dir.path()), start).await.unwrap_err();
    assert!(err.to_string().contains("refused"), "err: {err}");
    assert!(list_runs(dir.path()).is_empty());
}
