//! U5 — backtest runner tests. Offline: a fixture `ParquetDataCatalog` (built the
//! same way the adapter's own e2e test builds one — wiremock-ingested instruments +
//! directly-written bars) feeds a full ORB backtest that lands a finalized registry
//! run. No credentials, no network beyond the wiremock instrument masters.

use std::path::Path;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::ingest::checkpoint::{Checkpoint, GapReason, RebaseOrigin};
use nautilus_ls::ingest::{
    build_daily_bar, build_minute_bar, write_bars, write_instruments, BarKind,
};
use nautilus_ls::instruments::{InstrumentDomain, InstrumentProvider};
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_ls_lab::artifacts::data_quality::{DataQualityReport, GapReasonKind};
use nautilus_ls_lab::artifacts::manifest::Manifest;
use nautilus_ls_lab::artifacts::performance::PerformanceReport;
use nautilus_ls_lab::agent::envelope::{DecisionTrigger, SignalKind};
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
    // Session summaries fire at strategy stop, not on a bar — their trigger
    // records the stop-time state change (R5).
    let summaries: Vec<_> = envelopes
        .iter()
        .filter(|e| {
            e.decision_detail.as_ref().is_some_and(|d| d.kind == SignalKind::SessionSummary)
        })
        .collect();
    assert!(!summaries.is_empty(), "a session summary was recorded");
    assert!(
        summaries.iter().all(|e| matches!(e.trigger, DecisionTrigger::StateChange { .. })),
        "session summaries trigger on the stop-time state change, not a bar"
    );
    // AE3: the signal log is subsumed by decisions.jsonl — no signals.jsonl remains.
    assert!(!outcome.run_dir.join("signals.jsonl").exists(), "subsumed by decisions.jsonl");

    // R5: every exit envelope (Target / StopHit / TimeExit) carries per-trade MFE
    // alongside qty + price, so the next exit-tuning turn reads give-back directly.
    let exits: Vec<_> = envelopes
        .iter()
        .filter_map(|e| e.decision_detail.as_ref())
        .filter(|d| matches!(d.kind, SignalKind::Target | SignalKind::StopHit | SignalKind::TimeExit))
        .collect();
    assert!(!exits.is_empty(), "the completed trade recorded an exit");
    for d in &exits {
        for key in ["mfe_r", "qty", "price"] {
            assert!(d.values.contains_key(key), "exit {:?} carries {key}: {:?}", d.kind, d.values);
        }
    }

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
    // R7 inverted assertion: a clean catalog (no detected shift marks) reports an
    // EMPTY shift-symbol list — the blanket-discount era is over.
    assert!(dq.adjustment_basis_shift_symbols.is_empty(), "clean catalog → no shift symbols");
}

/// R7: a checkpoint shift mark on a symbol INSIDE the run's selected universe is
/// reported; a mark on a symbol outside it is not.
#[tokio::test]
async fn shift_marks_are_reported_per_symbol_intersected_with_the_universe() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path(), false).await;
    let cp_path = dir.path().join("catalog").join("ingest-checkpoint.json");
    let mut cp = Checkpoint::load(&cp_path).unwrap();
    // In-universe (the fixture selects 005930) + out-of-universe marks.
    cp.mark_shifted("005930.XKRX", "1-DAY", chrono::NaiveDate::from_ymd_opt(2024, 1, 5).unwrap(), RebaseOrigin::Heal);
    cp.mark_shifted("000660.XKRX", "1-DAY", chrono::NaiveDate::from_ymd_opt(2024, 1, 5).unwrap(), RebaseOrigin::Heal);
    cp.save(&cp_path).unwrap();

    let start = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
    let outcome = run(cfg(dir.path()), start).await.unwrap();
    let dq: DataQualityReport = serde_json::from_str(&std::fs::read_to_string(outcome.run_dir.join(DATA_QUALITY_FILE)).unwrap()).unwrap();
    assert_eq!(
        dq.adjustment_basis_shift_symbols,
        vec!["005930.XKRX".to_string()],
        "in-universe mark listed; out-of-universe mark not"
    );
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

// ---------------------------------------------------------------------------
// U7 — gap-report noise filter (R8, AE6, KTD5). Ingest writes the whole
// instrument universe while bars are bounded to the ingested few, so a
// never-ingested symbol must not flood the data-quality report with spurious
// missing-prior-daily gaps — while a symbol that HAS bars but lacks the prior
// session's daily still reports.
// ---------------------------------------------------------------------------

fn t8430_body_three_symbols() -> serde_json::Value {
    let sym = |hname: &str, shcode: &str, expcode: &str| {
        json!({ "hname": hname, "shcode": shcode, "expcode": expcode,
            "etfgubun": "0", "uplmtprice": "82000", "dnlmtprice": "44000",
            "jnilclose": "63000", "memedan": "1", "recprice": "63000", "gubun": "1" })
    };
    json!({
        "rsp_cd": "00000",
        "t8430OutBlock": [
            sym("삼성전자", "005930", "KR7005930003"),   // fully ingested (2 daily + minute)
            sym("에스케이하이닉스", "000660", "KR7000660001"), // never ingested (no bars)
            sym("기아", "000810", "KR7000810002"),        // has bars but lacks a prior daily
        ]
    })
}

/// Build a fixture whose instrument master carries three symbols but writes bars
/// for only two: 005930 (2 daily + a minute session) and 000810 (a single daily
/// bar, no prior-session daily). 000660 is never ingested.
async fn build_gap_noise_fixture(data_home: &Path) {
    let catalog = data_home.join("catalog");
    let server = MockServer::start().await;
    mount_token(&server).await;
    for (p, tr, body) in [
        ("/stock/etc", "t8430", t8430_body_three_symbols()),
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

    // 005930: two daily bars (a +5% gap) + a clean-breakout minute session.
    let id = InstrumentId::from("005930.XKRX");
    let daily_bt = BarKind::Daily.bar_type(id).unwrap();
    let daily: Vec<Bar> = [
        daily_json("20240104", "59000", "60500", "58500", "60000", "1000000"),
        daily_json("20240105", "63000", "64500", "62000", "64000", "1200000"),
    ]
    .iter()
    .map(|r| build_daily_bar(daily_bt, &serde_json::from_value(r.clone()).unwrap()).unwrap().unwrap())
    .collect();
    write_bars(&catalog, daily).await.unwrap();
    let minute_bt = BarKind::Minute(1).bar_type(id).unwrap();
    let minute: Vec<Bar> = [
        minute_json("20240105", "090000", "63000", "63500", "62500", "63200", "1000"),
        minute_json("20240105", "092000", "63300", "64000", "63300", "63900", "1000"),
        minute_json("20240105", "150000", "65000", "65300", "64900", "65100", "1000"),
        minute_json("20240105", "150100", "65100", "65300", "65000", "65200", "1000"),
    ]
    .iter()
    .map(|r| build_minute_bar(minute_bt, &serde_json::from_value(r.clone()).unwrap()).unwrap().unwrap())
    .collect();
    write_bars(&catalog, minute).await.unwrap();

    // 000810: a SINGLE in-range daily bar — has bars, but no prior-session daily.
    let kia = InstrumentId::from("000810.XKRX");
    let kia_bt = BarKind::Daily.bar_type(kia).unwrap();
    let kia_daily = build_daily_bar(
        kia_bt,
        &serde_json::from_value(daily_json("20240105", "10000", "10500", "9800", "10200", "500000")).unwrap(),
    )
    .unwrap()
    .unwrap();
    write_bars(&catalog, vec![kia_daily]).await.unwrap();

    let mut cp = Checkpoint::default();
    cp.adjusted_prices = true;
    cp.save(&catalog.join("ingest-checkpoint.json")).unwrap();
}

/// AE6: never-ingested symbols (no daily bars anywhere) contribute no gap
/// entries; a symbol that has bars but lacks its prior-session daily still does.
#[tokio::test]
async fn never_ingested_symbols_produce_no_gap_noise() {
    let dir = tempdir().unwrap();
    build_gap_noise_fixture(dir.path()).await;
    let start = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
    let outcome = run(cfg(dir.path()), start).await.unwrap();

    let dq: DataQualityReport =
        serde_json::from_str(&std::fs::read_to_string(outcome.run_dir.join(DATA_QUALITY_FILE)).unwrap())
            .unwrap();
    let missing: Vec<&str> = dq
        .coverage_gaps
        .iter()
        .filter(|g| g.reason == GapReasonKind::MissingPriorDaily)
        .map(|g| g.instrument.as_str())
        .collect();
    // 000660 is never ingested → filtered; 000810 has a bar but no prior daily → reported.
    assert!(!missing.contains(&"000660.XKRX"), "never-ingested symbol filtered: {missing:?}");
    assert!(missing.contains(&"000810.XKRX"), "has-bars-but-missing-prior-daily still reports: {missing:?}");
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

// ---------------------------------------------------------------------------
// Turn 5 (U1/U2/U4): multi-session drive, per-session universe reselection +
// per-day reset, one accumulated ledger, and the sequence-sensitive universe
// hash. A two-symbol fixture with a PRE-RANGE prior daily (so the earliest
// in-range session is tradeable, KTD-3) and day-specific gaps so the selected
// universe differs across sessions.
// ---------------------------------------------------------------------------

fn t8430_body_two() -> serde_json::Value {
    let sym = |hname: &str, shcode: &str, expcode: &str| {
        json!({ "hname": hname, "shcode": shcode, "expcode": expcode,
            "etfgubun": "0", "uplmtprice": "82000", "dnlmtprice": "44000",
            "jnilclose": "63000", "memedan": "1", "recprice": "63000", "gubun": "1" })
    };
    json!({
        "rsp_cd": "00000",
        "t8430OutBlock": [
            sym("삼성전자", "005930", "KR7005930003"),
            sym("에스케이하이닉스", "000660", "KR7000660001"),
        ]
    })
}

/// A clean winning opening-range-breakout minute session around `base` KRW:
/// range [base-500, base+500] over 09:00–09:10, a breakout to base+1000 at 09:20,
/// a fill + uptrend at 10:00, a 15:00 time-flat and its following-bar fill. The
/// range low is never breached, so it closes as a winner.
fn breakout_session(date: &str, base: i64) -> Vec<serde_json::Value> {
    let s = |n: i64| n.to_string();
    let rl = base - 500;
    let rh = base + 500;
    let bo = base + 1000;
    vec![
        minute_json(date, "090000", &s(base), &s(rh), &s(rl), &s(base), "1000"),
        minute_json(date, "091000", &s(base), &s(rh), &s(rl + 100), &s(base), "1000"),
        minute_json(date, "092000", &s(base + 200), &s(bo), &s(base + 100), &s(bo - 100), "1000"),
        minute_json(date, "100000", &s(bo - 100), &s(bo + 500), &s(rl + 600), &s(bo + 400), "1000"),
        minute_json(date, "150000", &s(bo + 400), &s(bo + 800), &s(bo + 300), &s(bo + 600), "1000"),
        minute_json(date, "150100", &s(bo + 600), &s(bo + 800), &s(bo + 500), &s(bo + 700), "1000"),
    ]
}

async fn write_daily_series(catalog: &Path, id: &str, rows: &[serde_json::Value]) {
    let bt = BarKind::Daily.bar_type(InstrumentId::from(id)).unwrap();
    let bars: Vec<Bar> = rows
        .iter()
        .map(|r| build_daily_bar(bt, &serde_json::from_value(r.clone()).unwrap()).unwrap().unwrap())
        .collect();
    write_bars(catalog, bars).await.unwrap();
}

async fn write_minute_session(catalog: &Path, id: &str, rows: &[serde_json::Value]) {
    let bt = BarKind::Minute(1).bar_type(InstrumentId::from(id)).unwrap();
    let bars: Vec<Bar> = rows
        .iter()
        .map(|r| build_minute_bar(bt, &serde_json::from_value(r.clone()).unwrap()).unwrap().unwrap())
        .collect();
    write_bars(catalog, bars).await.unwrap();
}

/// Build the multi-session fixture. Dailies (0104 is the PRE-RANGE prior):
///  - 005930 gaps +5% on 0105 and 0109, ~flat on 0108 → selected 0105 + 0109.
///  - 000660 gaps +5% on 0108 and 0109, ~flat on 0105 → selected 0108 + 0109.
/// Minute breakout sessions where each is selected, so 005930 trades 0105+0109 and
/// 000660 trades 0108+0109 (per-session isolation + multi-session drive).
async fn build_multi_session_fixture(data_home: &Path) {
    let catalog = data_home.join("catalog");
    let server = MockServer::start().await;
    mount_token(&server).await;
    for (p, tr, body) in [
        ("/stock/etc", "t8430", t8430_body_two()),
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

    // 005930 dailies: prior-close chain 60000 → 64000 → 64000 → 68000.
    write_daily_series(
        &catalog,
        "005930.XKRX",
        &[
            daily_json("20240104", "59000", "60500", "58500", "60000", "1000000"),
            daily_json("20240105", "63000", "64500", "62000", "64000", "1200000"), // +5% vs 60000
            daily_json("20240108", "64200", "65000", "63800", "64000", "1100000"), // +0.3% vs 64000
            daily_json("20240109", "67200", "68500", "67000", "68000", "1300000"), // +5% vs 64000
        ],
    )
    .await;
    // 000660 dailies: prior-close chain 50000 → 50000 → 53000 → 56000.
    write_daily_series(
        &catalog,
        "000660.XKRX",
        &[
            daily_json("20240104", "49000", "50500", "48800", "50000", "900000"),
            daily_json("20240105", "50100", "50600", "49800", "50000", "950000"), // +0.2% vs 50000
            daily_json("20240108", "52500", "53500", "52200", "53000", "1000000"), // +5% vs 50000
            daily_json("20240109", "55650", "56500", "55200", "56000", "1050000"), // +5% vs 53000
        ],
    )
    .await;

    // Minute breakout sessions on the days each symbol is selected.
    write_minute_session(&catalog, "005930.XKRX", &breakout_session("20240105", 63000)).await;
    write_minute_session(&catalog, "005930.XKRX", &breakout_session("20240109", 67000)).await;
    write_minute_session(&catalog, "000660.XKRX", &breakout_session("20240108", 52500)).await;
    write_minute_session(&catalog, "000660.XKRX", &breakout_session("20240109", 55000)).await;

    let mut cp = Checkpoint::default();
    cp.adjusted_prices = true;
    cp.save(&catalog.join("ingest-checkpoint.json")).unwrap();
}

/// The pinned range covering three in-range sessions (0105, 0108, 0109); 0104 is
/// the out-of-range prior for the earliest session.
fn multi_cfg(data_home: &Path) -> BacktestConfig {
    BacktestConfig::new(data_home, "20240105", "20240109")
}

/// The KST calendar date of a UTC-nanosecond timestamp (KST is UTC+9, no DST).
fn kst_date(ns: u64) -> NaiveDate {
    let dt = DateTime::<Utc>::from_timestamp_nanos(ns as i64);
    (dt + chrono::Duration::hours(9)).date_naive()
}

fn read_manifest_file(run_dir: &Path) -> Manifest {
    serde_json::from_str(&std::fs::read_to_string(run_dir.join(MANIFEST_FILE)).unwrap()).unwrap()
}

/// R1: the backtest drives EVERY in-range session — closed trades land on more than
/// one distinct session date (not just the last), and total trades accumulate into
/// one ledger.
#[tokio::test]
async fn multi_session_drive_trades_across_sessions() {
    let dir = tempdir().unwrap();
    build_multi_session_fixture(dir.path()).await;
    let start = Utc.with_ymd_and_hms(2024, 1, 10, 0, 0, 0).unwrap();
    let outcome = run(multi_cfg(dir.path()), start).await.unwrap();

    let perf = read_perf(&outcome.run_dir);
    // 005930 trades 0105 + 0109; 000660 trades 0108 + 0109 → 4 closed trades.
    assert_eq!(perf.summary["num_trades"], 4.0, "one accumulated ledger across sessions");
    let dates: std::collections::BTreeSet<NaiveDate> =
        perf.trades.iter().map(|t| kst_date(t.ts_opened)).collect();
    assert!(dates.len() > 1, "trades span >1 distinct session date: {dates:?}");
    assert_eq!(
        dates,
        [
            NaiveDate::from_ymd_opt(2024, 1, 5).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 8).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 9).unwrap(),
        ]
        .into_iter()
        .collect(),
        "every in-range session with a selected breakout trades"
    );
}

/// R3 (per-day reset, structural via KTD-2): a symbol that reached `Done` on an
/// earlier day trades AGAIN on a later day from a fresh state.
#[tokio::test]
async fn per_session_isolation_symbol_trades_again_on_a_later_day() {
    let dir = tempdir().unwrap();
    build_multi_session_fixture(dir.path()).await;
    let start = Utc.with_ymd_and_hms(2024, 1, 10, 0, 0, 0).unwrap();
    let outcome = run(multi_cfg(dir.path()), start).await.unwrap();

    let perf = read_perf(&outcome.run_dir);
    let samsung: Vec<NaiveDate> = perf
        .trades
        .iter()
        .filter(|t| t.symbol == "005930.XKRX")
        .map(|t| kst_date(t.ts_opened))
        .collect();
    assert_eq!(samsung.len(), 2, "005930 trades on two separate days: {samsung:?}");
    assert!(samsung.contains(&NaiveDate::from_ymd_opt(2024, 1, 5).unwrap()));
    assert!(
        samsung.contains(&NaiveDate::from_ymd_opt(2024, 1, 9).unwrap()),
        "Done on 0105, fresh state trades again on 0109: {samsung:?}"
    );
}

/// KTD-1 gate: two+ sessions driven sequentially on the same `spawn_blocking`
/// thread stay independent — no thread-local msgbus/handler leakage. Each session's
/// trade opens WITHIN its own session (no carried position, no duplicated fill), and
/// the ledger is exactly the per-session sum. A leak would surface as a trade opened
/// on the wrong day or a duplicated position.
#[tokio::test]
async fn same_thread_sessions_are_independent() {
    let dir = tempdir().unwrap();
    build_multi_session_fixture(dir.path()).await;
    let start = Utc.with_ymd_and_hms(2024, 1, 10, 0, 0, 0).unwrap();
    let outcome = run(multi_cfg(dir.path()), start).await.unwrap();

    let perf = read_perf(&outcome.run_dir);
    // Exactly one trade per (symbol, selected-session) — no duplication across the
    // sequential same-thread engines.
    let mut per_symbol_days: std::collections::BTreeMap<String, Vec<NaiveDate>> = Default::default();
    for t in &perf.trades {
        per_symbol_days.entry(t.symbol.clone()).or_default().push(kst_date(t.ts_opened));
    }
    assert_eq!(
        per_symbol_days["005930.XKRX"],
        vec![
            NaiveDate::from_ymd_opt(2024, 1, 5).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 9).unwrap()
        ],
        "005930 opens exactly once per selected session, in order"
    );
    assert_eq!(
        per_symbol_days["000660.XKRX"],
        vec![
            NaiveDate::from_ymd_opt(2024, 1, 8).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 9).unwrap()
        ],
        "000660 opens exactly once per selected session, in order"
    );
    // Each trade's open + close fall on the SAME session day (ORB flattens intraday;
    // a leaked cross-session position would close on a different day).
    for t in &perf.trades {
        if let Some(close) = t.ts_closed {
            assert_eq!(
                kst_date(t.ts_opened),
                kst_date(close),
                "trade opens and closes within one session: {}",
                t.symbol
            );
        }
    }
}

/// Degenerate range: a range containing exactly ONE session runs as a loop of one and
/// reproduces that session's trade — matching the same day's output inside the
/// multi-session run.
#[tokio::test]
async fn single_session_range_matches_that_session() {
    let dir = tempdir().unwrap();
    build_multi_session_fixture(dir.path()).await;
    let start = Utc.with_ymd_and_hms(2024, 1, 10, 0, 0, 0).unwrap();

    // The full multi-session run's 0109 trades.
    let full = run(multi_cfg(dir.path()), start).await.unwrap();
    let full_perf = read_perf(&full.run_dir);
    let jan9 = NaiveDate::from_ymd_opt(2024, 1, 9).unwrap();
    let mut full_0109: Vec<String> = full_perf
        .trades
        .iter()
        .filter(|t| kst_date(t.ts_opened) == jan9)
        .map(|t| t.symbol.clone())
        .collect();
    full_0109.sort();

    // A single-session range over just 0109 (0108 is the out-of-range prior).
    let mut one = BacktestConfig::new(dir.path(), "20240109", "20240109");
    one.params = multi_cfg(dir.path()).params;
    let s2 = Utc.with_ymd_and_hms(2024, 1, 11, 0, 0, 0).unwrap();
    let single = run(one, s2).await.unwrap();
    let single_perf = read_perf(&single.run_dir);
    let mut single_syms: Vec<String> = single_perf.trades.iter().map(|t| t.symbol.clone()).collect();
    single_syms.sort();

    assert_eq!(single_perf.summary["num_trades"], 2.0, "0109 selects both symbols → 2 trades");
    assert_eq!(single_syms, full_0109, "loop-of-one matches the same session inside the full run");
}

/// R2: per-session universe reselection — different sessions with different daily
/// gaps select DIFFERENT symbol sets, and the decision stream carries one universe
/// scan per session date.
#[tokio::test]
async fn per_session_universe_reselection_and_dated_envelopes() {
    let dir = tempdir().unwrap();
    build_multi_session_fixture(dir.path()).await;
    let start = Utc.with_ymd_and_hms(2024, 1, 10, 0, 0, 0).unwrap();
    let outcome = run(multi_cfg(dir.path()), start).await.unwrap();

    let decisions_path = outcome.run_dir.join(DECISIONS_FILE);
    let envelopes = read_envelopes(&decisions_path).unwrap();
    // The universe-scan envelopes (a StateChange "universe selection scan" trigger).
    let scan_dates: std::collections::BTreeSet<NaiveDate> = envelopes
        .iter()
        .filter(|e| matches!(&e.trigger,
            DecisionTrigger::StateChange { description } if description.contains("universe selection")))
        .map(|e| kst_date(e.ts_event))
        .collect();
    assert_eq!(
        scan_dates,
        [
            NaiveDate::from_ymd_opt(2024, 1, 5).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 8).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 9).unwrap(),
        ]
        .into_iter()
        .collect(),
        "one universe scan per in-range session date"
    );

    // Per-session universe ACCEPTs differ by day (005930 on 0105, 000660 on 0108).
    let accepts_on = |date: NaiveDate| -> std::collections::BTreeSet<String> {
        envelopes
            .iter()
            .filter(|e| kst_date(e.ts_event) == date)
            .filter_map(|e| e.decision_detail.as_ref())
            .filter(|d| d.kind == SignalKind::Universe && d.decision == Some(nautilus_ls_lab::agent::envelope::Decision::Accept))
            .map(|d| d.symbol.clone())
            .collect()
    };
    let jan5 = accepts_on(NaiveDate::from_ymd_opt(2024, 1, 5).unwrap());
    let jan8 = accepts_on(NaiveDate::from_ymd_opt(2024, 1, 8).unwrap());
    assert_eq!(jan5, ["005930.XKRX".to_string()].into_iter().collect(), "0105 selects 005930 only");
    assert_eq!(jan8, ["000660.XKRX".to_string()].into_iter().collect(), "0108 selects 000660 only");
    assert_ne!(jan5, jan8, "per-session universes differ when gaps differ");
}

/// R2 / KTD-3: the earliest in-range session is TRADEABLE — its prior daily comes
/// from the session immediately before the pinned range (0104), not a no-trade day.
/// And a symbol lacking any prior on any session is a real gap; a symbol tradeable on
/// some day is not (no spurious global gap).
#[tokio::test]
async fn first_in_range_session_is_tradeable_via_prior_lookback() {
    let dir = tempdir().unwrap();
    build_multi_session_fixture(dir.path()).await;
    let start = Utc.with_ymd_and_hms(2024, 1, 10, 0, 0, 0).unwrap();
    let outcome = run(multi_cfg(dir.path()), start).await.unwrap();

    let perf = read_perf(&outcome.run_dir);
    let jan5 = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
    assert!(
        perf.trades.iter().any(|t| kst_date(t.ts_opened) == jan5),
        "the earliest in-range session (0105) trades — prior daily from 0104 (pre-range)"
    );

    // No spurious global gap: both tradeable symbols are absent from missing-prior-daily.
    let dq: DataQualityReport = serde_json::from_str(
        &std::fs::read_to_string(outcome.run_dir.join(DATA_QUALITY_FILE)).unwrap(),
    )
    .unwrap();
    let missing: Vec<&str> = dq
        .coverage_gaps
        .iter()
        .filter(|g| g.reason == GapReasonKind::MissingPriorDaily)
        .map(|g| g.instrument.as_str())
        .collect();
    assert!(missing.is_empty(), "tradeable symbols are not spurious gaps: {missing:?}");
}

/// U4/KTD-5: the manifest stamps the operator-set version + held params, and
/// `universe_hash` is deterministic across identical multi-session runs yet
/// sequence-sensitive to a changed selection sequence.
#[tokio::test]
async fn manifest_version_held_params_and_sequence_sensitive_universe_hash() {
    let dir = tempdir().unwrap();
    build_multi_session_fixture(dir.path()).await;

    // A v6 run with the turn-5 held params.
    let mut v6 = multi_cfg(dir.path());
    v6.params.strategy_version = 6;
    v6.params.gap_min_pct = 0.6;
    v6.params.universe_top_n = 40;
    v6.params.max_concurrent = 5;
    v6.params.range_minutes = 15;
    let s1 = Utc.with_ymd_and_hms(2024, 1, 10, 0, 0, 0).unwrap();
    let o1 = run(v6.clone(), s1).await.unwrap();
    let m1 = read_manifest_file(&o1.run_dir);
    assert_eq!(m1.strategy_version, 6, "version stamped");
    assert_eq!(m1.params.gap_min_pct, 0.6);
    assert_eq!(m1.params.universe_top_n, 40);
    assert_eq!(m1.params.max_concurrent, 5);
    assert_eq!(m1.params.range_minutes, 15);

    // Identical run → identical sequence hash (determinism).
    let s2 = Utc.with_ymd_and_hms(2024, 1, 11, 0, 0, 0).unwrap();
    let o2 = run(v6, s2).await.unwrap();
    let m2 = read_manifest_file(&o2.run_dir);
    assert_eq!(m1.universe_hash, m2.universe_hash, "identical selection sequence → same hash");

    // A shorter range (fewer sessions → a different selection sequence) → different hash.
    let mut sub = BacktestConfig::new(dir.path(), "20240108", "20240109");
    sub.params.strategy_version = 6;
    sub.params.gap_min_pct = 0.6;
    sub.params.universe_top_n = 40;
    let s3 = Utc.with_ymd_and_hms(2024, 1, 12, 0, 0, 0).unwrap();
    let o3 = run(sub, s3).await.unwrap();
    let m3 = read_manifest_file(&o3.run_dir);
    assert_ne!(m1.universe_hash, m3.universe_hash, "a different session sequence → different hash");
}

// ---------------------------------------------------------------------------
// Turn 10 — breakout-strength band-pass (R2/R3, AE1/AE2/AE3, KTD3/KTD4/KTD6).
// The single-symbol `build_fixture` breaks out at strength 0.5: range
// [62500, 63500] (R = 1000), breakout bar high 64000 → (64000−63500)/1000 = 0.5.
// ---------------------------------------------------------------------------

/// Locate the first decision detail of a given kind in a run's decision stream.
fn find_detail_kind(
    envelopes: &[nautilus_ls_lab::agent::envelope::DecisionEnvelope],
    kind: SignalKind,
) -> Option<&nautilus_ls_lab::agent::envelope::DecisionDetail> {
    envelopes
        .iter()
        .filter_map(|e| e.decision_detail.as_ref())
        .find(|d| d.kind == kind)
}

/// AE2 / R3: a breakout whose strength (0.5) falls outside the configured band
/// [0.06, 0.13] is rejected done-for-day — the `Breakout` envelope AND a distinct
/// `breakout_strength_band` rejection are both recorded, and no order is placed.
#[tokio::test]
async fn band_pass_rejects_out_of_band_breakout() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path(), false).await;
    let mut c = cfg(dir.path());
    c.params.breakout_strength_min = 0.06;
    c.params.breakout_strength_max = 0.13;
    let start = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
    let outcome = run(c, start).await.unwrap();

    let decisions_path = outcome.run_dir.join(DECISIONS_FILE);
    let decisions = std::fs::read_to_string(&decisions_path).unwrap();
    assert!(decisions.contains("breakout_strength_band"), "the band filter rejected the entry");
    assert!(!decisions.contains("order_placed"), "no order placed on an out-of-band breakout");

    let envelopes = read_envelopes(&decisions_path).unwrap();
    // The Breakout envelope is still emitted, carrying strength ≈ 0.5 (R3: the
    // filtered population stays visible to strength-quartile reports).
    let breakout = find_detail_kind(&envelopes, SignalKind::Breakout).expect("a breakout envelope");
    let s = breakout.values.get("strength").copied().expect("strength recorded on the breakout");
    assert!((s - 0.5).abs() < 1e-9, "strength recorded: {s}");
    // The rejection reuses OrderRejectedSizing with the band filter string + edges.
    let reject = envelopes
        .iter()
        .filter_map(|e| e.decision_detail.as_ref())
        .find(|d| d.filter.as_deref() == Some("breakout_strength_band"))
        .expect("a band rejection envelope");
    assert_eq!(reject.kind, SignalKind::OrderRejectedSizing);
    assert_eq!(reject.values.get("strength").copied(), Some(0.5));
    assert_eq!(reject.values.get("breakout_strength_min").copied(), Some(0.06));
    assert_eq!(reject.values.get("breakout_strength_max").copied(), Some(0.13));

    // KTD3 one-shot-per-session: the rejected symbol is Done for the rest of the
    // session — the later 10:00/11:00 bars (both above range_high 63500) do NOT
    // re-arm a second breakout, so exactly one Breakout envelope is recorded.
    let breakouts = envelopes
        .iter()
        .filter_map(|e| e.decision_detail.as_ref())
        .filter(|d| d.kind == SignalKind::Breakout)
        .count();
    assert_eq!(breakouts, 1, "a strength-rejected symbol stays Done — no re-break re-entry");

    let perf = read_perf(&outcome.run_dir);
    assert_eq!(perf.summary["num_trades"], 0.0, "no trade on a rejected entry");
}

/// AE2 (below-floor direction) through the engine: a breakout at strength 0.5 is
/// rejected when it falls BELOW the band floor [0.6, 0.9] (the above-ceiling
/// direction is covered by `band_pass_rejects_out_of_band_breakout`). Reuses the
/// single-symbol fixture; no new fixture needed — only the floor moves.
#[tokio::test]
async fn band_pass_rejects_below_floor_breakout() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path(), false).await;
    let mut c = cfg(dir.path());
    c.params.breakout_strength_min = 0.6; // above the fixture's 0.5 strength
    c.params.breakout_strength_max = 0.9;
    let start = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
    let outcome = run(c, start).await.unwrap();

    let decisions = std::fs::read_to_string(outcome.run_dir.join(DECISIONS_FILE)).unwrap();
    assert!(decisions.contains("breakout_strength_band"), "below-floor strength is rejected");
    assert!(!decisions.contains("order_placed"), "no order on a below-floor breakout");
    let perf = read_perf(&outcome.run_dir);
    assert_eq!(perf.summary["num_trades"], 0.0);
}

/// AE1: a breakout at strength 0.5 inside the band [0.06, 0.6] enters exactly as
/// it would unfiltered — the order is placed, the trade completes, and no band
/// rejection is recorded.
#[tokio::test]
async fn band_pass_admits_in_band_breakout() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path(), false).await;
    let mut c = cfg(dir.path());
    c.params.breakout_strength_min = 0.06;
    c.params.breakout_strength_max = 0.6;
    let start = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
    let outcome = run(c, start).await.unwrap();

    let decisions = std::fs::read_to_string(outcome.run_dir.join(DECISIONS_FILE)).unwrap();
    assert!(decisions.contains("order_placed"), "an in-band breakout places the order");
    assert!(!decisions.contains("breakout_strength_band"), "no band rejection for an in-band breakout");
    let perf = read_perf(&outcome.run_dir);
    assert_eq!(perf.summary["num_trades"], 1.0, "the in-band trade completes");
}

/// R2 / KTD6: the band is inclusive on the ceiling — a breakout whose strength
/// equals the ceiling exactly (0.5) enters; narrowing the ceiling a hair below
/// rejects it.
#[tokio::test]
async fn band_pass_ceiling_is_inclusive() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path(), false).await;

    let mut inclusive = cfg(dir.path());
    inclusive.params.breakout_strength_max = 0.5; // == the fixture strength
    let s1 = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
    let o1 = run(inclusive, s1).await.unwrap();
    let d1 = std::fs::read_to_string(o1.run_dir.join(DECISIONS_FILE)).unwrap();
    assert!(d1.contains("order_placed"), "strength == ceiling enters (inclusive band)");
    assert!(!d1.contains("breakout_strength_band"), "the inclusive edge is not a rejection");

    let mut exclusive = cfg(dir.path());
    exclusive.params.breakout_strength_max = 0.49; // just below the fixture strength
    let s2 = Utc.with_ymd_and_hms(2024, 1, 7, 0, 0, 0).unwrap();
    let o2 = run(exclusive, s2).await.unwrap();
    let d2 = std::fs::read_to_string(o2.run_dir.join(DECISIONS_FILE)).unwrap();
    assert!(!d2.contains("order_placed"), "strength above the ceiling is rejected");
    assert!(d2.contains("breakout_strength_band"));
}

/// AE3: with the filter-off defaults, entry/exit behavior is unchanged from the
/// pre-filter binary — the order is placed and the winning trade completes — and
/// the only additive change to the decision stream is a `strength` value on the
/// Breakout envelope. (The params-summary band fields are covered by the
/// `numeric_summary_includes_band_fields` unit test.)
#[tokio::test]
async fn default_params_preserve_entry_and_add_strength() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path(), false).await;
    let start = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
    let outcome = run(cfg(dir.path()), start).await.unwrap();

    let decisions_path = outcome.run_dir.join(DECISIONS_FILE);
    let decisions = std::fs::read_to_string(&decisions_path).unwrap();
    assert!(decisions.contains("order_placed"), "default params place the order as before");
    assert!(!decisions.contains("breakout_strength_band"), "filter-off defaults never reject");
    let perf = read_perf(&outcome.run_dir);
    assert_eq!(perf.summary["num_trades"], 1.0, "the trade sequence is unchanged");
    assert!(perf.summary["pnl_total"] > 0.0, "the winning trade is unchanged");

    let envelopes = read_envelopes(&decisions_path).unwrap();
    let breakout = find_detail_kind(&envelopes, SignalKind::Breakout).expect("a breakout envelope");
    assert!(breakout.values.contains_key("strength"), "the Breakout envelope gains strength");
}

/// A single-session breakout where the opening range is [base−500, base+500] and
/// the breakout bar's high is `break_high` (> base+500) — so strength is
/// `(break_high − (base+500)) / 1000`. Mirrors [`breakout_session`]'s valid-OHLC
/// shape (a clean hold above the stop, below the 1.0R target, to the 15:00
/// time-flat), parametrizing only the breakout height to grade strength.
fn breakout_session_graded(date: &str, base: i64, break_high: i64) -> Vec<serde_json::Value> {
    let s = |n: i64| n.to_string();
    let rl = base - 500;
    let rh = base + 500;
    let bo = break_high;
    vec![
        minute_json(date, "090000", &s(base), &s(rh), &s(rl), &s(base), "1000"),
        minute_json(date, "091000", &s(base), &s(rh), &s(rl + 100), &s(base), "1000"),
        minute_json(date, "092000", &s(base + 200), &s(bo), &s(base + 100), &s(bo - 100), "1000"),
        minute_json(date, "100000", &s(bo - 100), &s(bo + 500), &s(rl + 600), &s(bo + 400), "1000"),
        minute_json(date, "150000", &s(bo + 400), &s(bo + 800), &s(bo + 300), &s(bo + 600), "1000"),
        minute_json(date, "150100", &s(bo + 600), &s(bo + 800), &s(bo + 500), &s(bo + 700), "1000"),
    ]
}

/// A 2-symbol single-session fixture (0105, prior 0104) where both symbols gap
/// +5% and break out, but at DIFFERENT strengths: 005930 breaks strongly
/// (strength 0.5), 000660 breaks mildly (strength 0.1). Proves a band-rejected
/// entry does not consume a `max_concurrent` slot.
async fn build_graded_strength_fixture(data_home: &Path) {
    let catalog = data_home.join("catalog");
    let server = MockServer::start().await;
    mount_token(&server).await;
    for (p, tr, body) in [
        ("/stock/etc", "t8430", t8430_body_two()),
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

    // Both gap +5% on 0105 (prior 0104), so both are selected.
    write_daily_series(
        &catalog,
        "005930.XKRX",
        &[
            daily_json("20240104", "59000", "60500", "58500", "60000", "1000000"),
            daily_json("20240105", "63000", "64500", "62000", "64000", "1200000"), // +5%
        ],
    )
    .await;
    write_daily_series(
        &catalog,
        "000660.XKRX",
        &[
            daily_json("20240104", "49000", "50500", "48800", "50000", "900000"),
            daily_json("20240105", "52500", "53500", "52200", "53000", "1000000"), // +5%
        ],
    )
    .await;

    // 005930 strong breakout: range [62500, 63500], high 64000 → strength 0.5.
    write_minute_session(&catalog, "005930.XKRX", &breakout_session_graded("20240105", 63000, 64000)).await;
    // 000660 mild breakout: range [52500, 53500], high 53600 → strength 0.1.
    write_minute_session(&catalog, "000660.XKRX", &breakout_session_graded("20240105", 53000, 53600)).await;

    let mut cp = Checkpoint::default();
    cp.adjusted_prices = true;
    cp.save(&catalog.join("ingest-checkpoint.json")).unwrap();
}

/// R3 / slot accounting: with `max_concurrent = 1` and the band [0.06, 0.13], the
/// strong 005930 break (strength 0.5) is band-rejected and the mild 000660 break
/// (strength 0.1) is admitted — the rejection did NOT consume the single slot.
#[tokio::test]
async fn band_rejected_entry_does_not_consume_a_concurrency_slot() {
    let dir = tempdir().unwrap();
    build_graded_strength_fixture(dir.path()).await;
    let mut c = cfg(dir.path());
    c.params.max_concurrent = 1;
    c.params.breakout_strength_min = 0.06;
    c.params.breakout_strength_max = 0.13;
    let start = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
    let outcome = run(c, start).await.unwrap();

    let decisions_path = outcome.run_dir.join(DECISIONS_FILE);
    let envelopes = read_envelopes(&decisions_path).unwrap();
    // 005930 is band-rejected...
    let rejected_5930 = envelopes.iter().filter_map(|e| e.decision_detail.as_ref()).any(|d| {
        d.filter.as_deref() == Some("breakout_strength_band") && d.symbol == "005930.XKRX"
    });
    assert!(rejected_5930, "the strong 005930 break is band-rejected");
    // ...and 000660 still places its order (the freed slot admits the in-band break).
    let placed_660 = envelopes.iter().filter_map(|e| e.decision_detail.as_ref()).any(|d| {
        d.kind == SignalKind::OrderPlaced && d.symbol == "000660.XKRX"
    });
    assert!(placed_660, "the in-band 000660 break enters despite max_concurrent = 1");
    // No entry was rejected by the concurrency gate — the band rejection never
    // leaked the single slot (max_concurrent appears only in the params summary).
    let concurrency_reject = envelopes
        .iter()
        .filter_map(|e| e.decision_detail.as_ref())
        .any(|d| d.filter.as_deref() == Some("max_concurrent"));
    assert!(!concurrency_reject, "no slot was consumed by the band rejection");
}

/// Build a single-symbol fixture whose breakout bar is a WHIPSAW: it clears the
/// range high (strength 0.5) AND breaches the range low on the same bar, so
/// `OrbState::on_bar` returns `[Enter, Exit{Stop}]` in one batch.
async fn build_whipsaw_fixture(data_home: &Path) {
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

    // Prior close 60000, today open 63000 → +5% gap (selected).
    write_daily_series(
        &catalog,
        "005930.XKRX",
        &[
            daily_json("20240104", "59000", "60500", "58500", "60000", "1000000"),
            daily_json("20240105", "63000", "64500", "62000", "64000", "1200000"),
        ],
    )
    .await;
    // Opening range 09:00–09:14 = [62500, 63500]; the 09:20 bar is a whipsaw:
    // high 64000 (> 63500, strength 0.5) AND low 62000 (<= the 62500 stop).
    write_minute_session(
        &catalog,
        "005930.XKRX",
        &[
            minute_json("20240105", "090000", "63000", "63500", "62500", "63200", "1000"),
            minute_json("20240105", "091000", "63200", "63400", "62800", "63000", "1000"),
            minute_json("20240105", "092000", "63000", "64000", "62000", "62500", "1000"),
            minute_json("20240105", "150000", "62500", "62800", "62000", "62400", "1000"),
            minute_json("20240105", "150100", "62400", "62600", "62000", "62300", "1000"),
        ],
    )
    .await;

    let mut cp = Checkpoint::default();
    cp.adjusted_prices = true;
    cp.save(&catalog.join("ingest-checkpoint.json")).unwrap();
}

/// R3 / KTD4: a whipsaw breakout whose entry is strength-rejected emits NO exit
/// envelope — the rejected Enter `continue`s before `entered_qty` is set, so the
/// same-bar Exit action hits the `qty <= 0` guard and places/records nothing.
/// (Guards against a future refactor moving the rejection after the qty insert,
/// which would emit a phantom exit for a position that never opened.)
#[tokio::test]
async fn whipsaw_entry_strength_rejected_emits_no_exit() {
    let dir = tempdir().unwrap();
    build_whipsaw_fixture(dir.path()).await;
    let mut c = cfg(dir.path());
    c.params.breakout_strength_min = 0.06;
    c.params.breakout_strength_max = 0.13; // the whipsaw's 0.5 strength is out of band
    let start = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
    let outcome = run(c, start).await.unwrap();

    let envelopes = read_envelopes(&outcome.run_dir.join(DECISIONS_FILE)).unwrap();
    // The band rejected the entry...
    assert!(
        find_detail_kind(&envelopes, SignalKind::Breakout).is_some(),
        "the whipsaw breakout is still recorded"
    );
    let band_rejected = envelopes
        .iter()
        .filter_map(|e| e.decision_detail.as_ref())
        .any(|d| d.filter.as_deref() == Some("breakout_strength_band"));
    assert!(band_rejected, "the whipsaw entry is band-rejected");
    // ...and NO exit envelope (Stop / Target / TimeExit) or order was emitted for
    // the position that never opened.
    let has_exit_or_order = envelopes.iter().filter_map(|e| e.decision_detail.as_ref()).any(|d| {
        matches!(
            d.kind,
            SignalKind::StopHit | SignalKind::Target | SignalKind::TimeExit | SignalKind::OrderPlaced
        )
    });
    assert!(!has_exit_or_order, "a band-rejected whipsaw emits no exit and places no order");
    let perf = read_perf(&outcome.run_dir);
    assert_eq!(perf.summary["num_trades"], 0.0, "no trade on a rejected whipsaw");
}

// ---------------------------------------------------------------------------
// Metadata-driven run (plan 2026-07-10-003, U4/U5/U6): gated, tagged selection
// over a metadata fixture; tier composition + the Turn-N count summary.
// ---------------------------------------------------------------------------

mod metadata_driven {
    use super::*;
    use nautilus_ls::reference::universe_metadata::{
        CapCutoff, CapTier, Designation, DesignationKind, IndexMembership, InstrumentMetadata,
        LiquidityTier, MarketClass, MetadataProvenance, Resolved, UniverseMetadata,
    };
    use nautilus_ls_lab::agent::envelope::Decision;

    /// Write a one-record artifact for 005930 (KOSPI × Top = blue-chip) and
    /// return its path + content hash. `tradable: false` carries a t1405 halt.
    fn write_artifact(data_home: &Path, tradable: bool) -> (std::path::PathBuf, String) {
        let designation = (!tradable)
            .then(|| Designation { kind: DesignationKind::Halt, source_tr: "t1405".to_string() });
        let artifact = UniverseMetadata {
            provenance: MetadataProvenance {
                captured_at: "2026-07-10T00:30:00Z".to_string(),
                session_date: "20240105".to_string(),
                source_trs: vec!["t8430".into(), "t1444".into(), "t1405".into()],
                instrument_type_filter: "equities-only (test fixture)".to_string(),
                tier_boundary_rule: "test: top half per market".to_string(),
                cap_cutoffs: vec![CapCutoff {
                    market_class: MarketClass::Kospi,
                    on_board: 1,
                    top_count: 1,
                    boundary_cap: Some(4_000_000.0),
                }],
                paper_incompatible: Vec::new(),
            },
            records: vec![InstrumentMetadata {
                shcode: "005930".to_string(),
                market_class: MarketClass::Kospi,
                market_cap: Resolved::Value(4_000_000.0),
                cap_tier: CapTier::Top,
                turnover: Resolved::Unavailable,
                liquidity_tier: LiquidityTier::Unknown,
                index_membership: Resolved::Proxy(IndexMembership::Kospi200),
                has_derivative: Resolved::Value(true),
                designation,
                tradable,
            }],
        };
        let path = data_home.join("universe-metadata.json");
        std::fs::write(&path, serde_json::to_string_pretty(&artifact).unwrap()).unwrap();
        (path, artifact.content_hash())
    }

    /// U4/U5: the metadata-driven run tags the accept, attributes the trade to
    /// its tier, pins the artifact hash in the manifest, records the typed tier
    /// composition, and emits a counts-only Turn-N summary (KTD5: no expectancy).
    #[tokio::test]
    async fn metadata_run_tags_selection_and_records_tier_composition() {
        let dir = tempdir().unwrap();
        build_fixture(dir.path(), false).await;
        let (artifact_path, hash) = write_artifact(dir.path(), true);

        let mut config = cfg(dir.path());
        config.metadata_path = Some(artifact_path);
        let start = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
        let outcome = run(config, start).await.unwrap();

        // KTD2: the artifact identity is pinned into the manifest.
        let manifest: Manifest = serde_json::from_str(
            &std::fs::read_to_string(outcome.run_dir.join(MANIFEST_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest.universe_metadata_hash.as_deref(), Some(hash.as_str()));

        // R9/KTD4: the accept envelope carries all five conditioner tags.
        let envelopes = read_envelopes(&outcome.run_dir.join(DECISIONS_FILE)).unwrap();
        let accept = envelopes
            .iter()
            .filter_map(|e| e.decision_detail.as_ref())
            .find(|d| d.decision == Some(Decision::Accept))
            .expect("the gapping symbol is accepted");
        let tags = accept.tags.as_ref().expect("accept carries the tag set");
        assert_eq!(tags.cap_tier, CapTier::Top);
        assert_eq!(tags.market_class, MarketClass::Kospi);
        assert_eq!(tags.index_membership, Resolved::Proxy(IndexMembership::Kospi200));
        assert_eq!(tags.has_derivative, Resolved::Value(true));
        // The daily-bar liquidity proxy resolved a tier (prior close 60000 ×
        // 1,000,000 shares = 60bn KRW ≥ the High floor).
        assert_eq!(tags.liquidity_tier, LiquidityTier::High);

        // U6: the typed tier composition attributes the symbol + its trade to
        // the blue-chip cell.
        let dq: DataQualityReport = serde_json::from_str(
            &std::fs::read_to_string(outcome.run_dir.join(DATA_QUALITY_FILE)).unwrap(),
        )
        .unwrap();
        let composition = dq.tier_composition.expect("metadata run records tier composition");
        let blue = composition.iter().find(|t| t.stratum == "kospi_blue_chip").unwrap();
        assert_eq!((blue.symbols, blue.trades), (1, 1), "one symbol, one joined trade");
        for other in composition.iter().filter(|t| t.stratum != "kospi_blue_chip") {
            assert_eq!((other.symbols, other.trades), (0, 0), "{}", other.stratum);
        }

        // KTD5 staging guard: the Turn-N summary reports counts + verdict only.
        let summary = outcome.tier_summary.expect("metadata run emits the Turn-N summary");
        let joined = summary.join("\n");
        assert!(joined.contains("kospi_blue_chip"), "{joined}");
        assert!(joined.contains("RED"), "1 trade cannot clear the 30-in-2 floor: {joined}");
        for banned in ["expectancy", "pnl", "P&L", "return"] {
            assert!(!joined.to_lowercase().contains(&banned.to_lowercase()), "no {banned}: {joined}");
        }
        // The standard performance artifact is still written unchanged.
        assert!(outcome.run_dir.join(PERFORMANCE_FILE).exists());
    }

    /// AE3 end-to-end: a designated symbol is gated out of selection, so it
    /// contributes no trade to any tier and the rejection names the gate.
    #[tokio::test]
    async fn designated_symbol_contributes_no_trade_to_any_tier() {
        let dir = tempdir().unwrap();
        build_fixture(dir.path(), false).await;
        let (artifact_path, _) = write_artifact(dir.path(), false);

        let mut config = cfg(dir.path());
        config.metadata_path = Some(artifact_path);
        let start = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
        let outcome = run(config, start).await.unwrap();

        let envelopes = read_envelopes(&outcome.run_dir.join(DECISIONS_FILE)).unwrap();
        assert!(
            envelopes.iter().filter_map(|e| e.decision_detail.as_ref()).any(|d| d
                .filter
                .as_deref()
                == Some("not_tradable")),
            "the halt gates the symbol out"
        );
        assert!(
            !envelopes
                .iter()
                .filter_map(|e| e.decision_detail.as_ref())
                .any(|d| d.kind == SignalKind::OrderPlaced),
            "no order for a designated symbol"
        );
        let dq: DataQualityReport = serde_json::from_str(
            &std::fs::read_to_string(outcome.run_dir.join(DATA_QUALITY_FILE)).unwrap(),
        )
        .unwrap();
        for tier in dq.tier_composition.expect("composition recorded") {
            assert_eq!(tier.trades, 0, "{}: no trade in any tier", tier.stratum);
        }
        let joined = outcome.tier_summary.unwrap().join("\n");
        assert!(joined.contains("RED"), "{joined}");
    }
}
