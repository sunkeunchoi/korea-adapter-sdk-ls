//! `lab-research` CLI tests (U1–U6). Offline: an in-memory wiremock-ingested
//! fixture catalog feeds real backtest runs into the registry, then the CLI's
//! turn / compare / replay / catalog-status / analyze commands run against them.
//! No credentials, no network beyond the wiremock instrument masters. Dispatch +
//! scrub (U1) are exercised through the compiled bin (`CARGO_BIN_EXE_*`); the
//! command verdicts (U2–U6) through the library functions for structured
//! assertions.

use std::path::Path;
use std::process::Command;

use chrono::{DateTime, TimeZone, Utc};
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::ingest::checkpoint::Checkpoint;
use nautilus_ls::ingest::{build_daily_bar, build_minute_bar, write_bars, write_instruments, BarKind};
use nautilus_ls::instruments::{InstrumentDomain, InstrumentProvider};
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_ls_lab::agent::recording::DecisionRecorder;
use nautilus_ls_lab::artifacts::manifest::{DataRange, Manifest};
use nautilus_ls_lab::artifacts::{list_runs, MANIFEST_FILE};
use nautilus_ls_lab::runner::backtest::{run as backtest_run, BacktestConfig};
use nautilus_ls_lab::runner::research::{
    analyze_scaffold, catalog_compact, catalog_status, compare, replay_guard, turn, CompactConfig,
    CompareConfig, CompareMode, ReplayConfig, ScaffoldConfig, StatusConfig, TurnConfig,
};
use nautilus_model::data::Bar;
use nautilus_model::identifiers::InstrumentId;
use serde_json::json;
use tempfile::tempdir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// --------------------------------------------------------------------------
// Fixture: a catalog with one +5% gapping symbol (005930) — two daily bars for
// the universe scan and a clean-breakout minute session on 20240105.
// --------------------------------------------------------------------------

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

async fn build_fixture(data_home: &Path) {
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
        minute_json("20240105", "091000", "63200", "63400", "63000", "63300", "1000"),
        minute_json("20240105", "092000", "63300", "64000", "63300", "63900", "1000"),
        minute_json("20240105", "100000", "64000", "64500", "63900", "64400", "1000"),
        minute_json("20240105", "110000", "64400", "65000", "64300", "64900", "1000"),
        minute_json("20240105", "150000", "65000", "65300", "64900", "65100", "1000"),
        minute_json("20240105", "150100", "65100", "65300", "65000", "65200", "1000"),
    ]
    .iter()
    .map(|r| build_minute_bar(minute_bt, &serde_json::from_value(r.clone()).unwrap()).unwrap().unwrap())
    .collect();
    write_bars(&catalog, minute).await.unwrap();

    let mut cp = Checkpoint::default();
    cp.adjusted_prices = true;
    cp.save(&catalog.join("ingest-checkpoint.json")).unwrap();
}

fn stamp(secs: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_704_500_000 + secs, 0).unwrap()
}

/// Seed a finalized backtest run at a given gap floor + version, returning its id.
async fn seed_run(data_home: &Path, gap: f64, version: u32, at: DateTime<Utc>) -> String {
    let mut cfg = BacktestConfig::new(data_home, "20240102", "20240105");
    cfg.params.gap_min_pct = gap;
    cfg.params.strategy_version = version;
    backtest_run(cfg, at).await.unwrap().run_id
}

fn read_manifest(data_home: &Path, run_id: &str) -> Manifest {
    serde_json::from_str(
        &std::fs::read_to_string(data_home.join("runs").join(run_id).join(MANIFEST_FILE)).unwrap(),
    )
    .unwrap()
}

fn turn_cfg(data_home: &Path, param: &str, target: f64, at: DateTime<Utc>) -> TurnConfig {
    let mut cfg = TurnConfig::new(data_home, at);
    cfg.override_param = Some(param.to_string());
    cfg.override_target = Some(target);
    cfg
}

// ===========================================================================
// U1 — dispatch + scrub (through the compiled bin)
// ===========================================================================

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lab-research"))
}

#[test]
fn unknown_subcommand_enumerates_valid_ones() {
    let out = bin().arg("bogus").output().unwrap();
    assert!(!out.status.success(), "unknown subcommand is a non-zero exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown subcommand"), "{stderr}");
    for expected in
        ["turn", "runs compare", "replay", "catalog status", "analyze --scaffold", "report mfe"]
    {
        assert!(stderr.contains(expected), "usage enumerates {expected:?}: {stderr}");
    }
}

#[test]
fn report_unknown_mode_names_report_mfe() {
    let out = bin().args(["report", "bogus"]).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("report mfe"), "{stderr}");
}

#[test]
fn report_mfe_through_the_bin_prints_the_distribution() {
    // Dispatch + env wiring end-to-end: a synthetic finalized run (manifest +
    // decisions.jsonl) under LS_DATA_HOME, selected by LS_REPORT_RUN, prints the
    // MFE distribution with the shcode verbatim (KTD8) and exits zero.
    use std::collections::BTreeMap;

    use nautilus_ls_lab::agent::context::AgentContext;
    use nautilus_ls_lab::agent::envelope::{
        to_jsonl, DecisionDetail, DecisionEnvelope, DecisionTrigger, SignalKind,
    };
    use nautilus_ls_lab::artifacts::{RunSource, DECISIONS_FILE};
    use nautilus_ls_lab::params::OrbParams;

    let dir = tempdir().unwrap();
    let run_id = "20260601T000000Z-backtest-orb-v9";
    let run_dir = dir.path().join("runs").join(run_id);
    std::fs::create_dir_all(&run_dir).unwrap();

    let mut params = OrbParams::default();
    params.strategy_version = 9;
    params.profit_target_r = 1.0;
    let manifest = Manifest {
        run_id: run_id.to_string(),
        source: RunSource::Backtest,
        strategy_id: "orb".to_string(),
        strategy_version: 9,
        params,
        data_range: DataRange { start: "20260601".to_string(), end: "20260630".to_string() },
        catalog_fingerprint: "fp".to_string(),
        universe_hash: "uh".to_string(),
        strategy_code_hash: "ch".to_string(),
        checkpoint_hash: None,
        universe_metadata_hash: None,
        created_utc: "2026-07-10T00:00:00+00:00".to_string(),
    };
    std::fs::write(run_dir.join(MANIFEST_FILE), serde_json::to_string(&manifest).unwrap())
        .unwrap();

    let vals = |pairs: &[(&str, f64)]| -> BTreeMap<String, f64> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    };
    let env = |kind: SignalKind, values: BTreeMap<String, f64>| {
        DecisionEnvelope::telemetry(
            // 10:00 KST on 2026-06-01 (01:00Z).
            1_780_275_600_000_000_000,
            DecisionTrigger::Manual { reason: "test".to_string() },
            DecisionDetail::transition("005930.XKRX", kind, values),
            AgentContext::telemetry("orb", 9, BTreeMap::new(), BTreeMap::new()),
        )
    };
    let envelopes = vec![
        env(
            SignalKind::Breakout,
            vals(&[("range_high", 100.0), ("range_low", 90.0), ("breakout_price", 101.0)]),
        ),
        env(SignalKind::Target, vals(&[("mfe_r", 1.05), ("price", 111.0), ("qty", 10.0)])),
    ];
    std::fs::write(run_dir.join(DECISIONS_FILE), to_jsonl(&envelopes).unwrap()).unwrap();

    let out = bin()
        .args(["report", "mfe"])
        .env("LS_DATA_HOME", dir.path())
        .env("LS_REPORT_RUN", run_id)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("mfe_r percentiles"), "{stdout}");
    assert!(stdout.contains("target-exit share: 1/1"), "{stdout}");
    assert!(!stdout.contains("***"), "structured output is not masked: {stdout}");
    assert!(stdout.contains("candidate verdict:"), "{stdout}");
}

#[test]
fn missing_env_var_names_the_variable() {
    // `catalog status` needs LS_DATA_HOME first — the error names it.
    let out = bin().args(["catalog", "status"]).env_remove("LS_DATA_HOME").output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("LS_DATA_HOME"), "names the missing variable: {stderr}");
}

#[test]
fn terminal_error_with_an_account_like_token_is_scrubbed() {
    // An error carrying a 6+-digit run is masked before it reaches stderr (KTD8):
    // a bogus run id with an embedded account-like token surfaces via the
    // missing-manifest error, scrubbed.
    let dir = tempdir().unwrap();
    let out = bin()
        .args(["analyze", "--scaffold"])
        .env("LS_DATA_HOME", dir.path())
        .env("LS_ANALYZE_RUN", "acct-20187511401-run")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("20187511401"), "account token masked: {stderr}");
    assert!(stderr.contains("***"), "masking marker present: {stderr}");
}

#[tokio::test]
async fn structured_stdout_prints_symbols_unmasked() {
    // KTD8: stdout prints typed values verbatim — a 6-digit KRX shcode must not be
    // masked by the free-text account-number heuristic. `catalog status` through
    // the compiled bin is the end-to-end check.
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let out = bin().args(["catalog", "status"]).env("LS_DATA_HOME", dir.path()).output().unwrap();
    assert!(out.status.success(), "healthy catalog is a go");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("005930.XKRX"), "shcode printed verbatim: {stdout}");
    assert!(!stdout.contains("***"), "structured output is not masked: {stdout}");
}

#[tokio::test]
async fn malformed_expected_range_is_a_hard_error_not_a_silent_go() {
    // A bad LS_STATUS_SDATE must error, never silently skip the span check and
    // report GO (fail-open on a go/no-go gate).
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let out = bin()
        .args(["catalog", "status"])
        .env("LS_DATA_HOME", dir.path())
        .env("LS_STATUS_SDATE", "2024-01-01") // dashes, not YYYYMMDD
        .env("LS_STATUS_EDATE", "20240105")
        .output()
        .unwrap();
    assert!(!out.status.success(), "malformed date is a hard error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("LS_STATUS_SDATE") && stderr.contains("YYYYMMDD"), "{stderr}");
}

#[tokio::test]
async fn malformed_minute_step_errors_rather_than_defaulting() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    seed_run(dir.path(), 2.4, 0, stamp(0)).await;
    let out = bin()
        .args(["turn"])
        .env("LS_DATA_HOME", dir.path())
        .env("LS_TURN_MINUTE_STEP", "5m") // typo
        .output()
        .unwrap();
    assert!(!out.status.success(), "a bad minute step is a hard error, not a silent step-1 run");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("LS_TURN_MINUTE_STEP"), "names the variable: {stderr}");
}

#[test]
fn no_args_prints_usage_and_exits_non_zero() {
    let out = bin().output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("usage") || stderr.contains("unknown subcommand"), "{stderr}");
}

// ===========================================================================
// U2 — the turn command
// ===========================================================================

#[tokio::test]
async fn ae2_bound_inclusive_turn_is_approved_and_bumps_the_version() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    seed_run(dir.path(), 2.4, 0, stamp(0)).await;

    // 2.4 -> 1.2 is a relative change of exactly 0.50 — approved (bound inclusive).
    let out = turn(turn_cfg(dir.path(), "gap_min_pct", 1.2, stamp(10))).await.unwrap();
    assert!(out.ran, "backtest ran: {:?}", out.refusal);
    assert_eq!(out.approved, Some(true));
    assert_eq!(out.version, Some(1), "version is prior + 1");
    let m = read_manifest(dir.path(), out.run_id.as_ref().unwrap());
    assert_eq!(m.strategy_version, 1);
    assert_eq!(m.params.gap_min_pct, 1.2);
}

#[tokio::test]
async fn ae1_over_bound_turn_is_denied_and_runs_nothing() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let seed = seed_run(dir.path(), 2.4, 0, stamp(0)).await;

    // 2.4 -> 0.6 is a relative change of 0.75 — denied.
    let out = turn(turn_cfg(dir.path(), "gap_min_pct", 0.6, stamp(10))).await.unwrap();
    assert!(!out.ran, "no backtest on a denial");
    assert_eq!(out.approved, Some(false));
    assert!(out.refusal.unwrap().contains("proposal_bounds"), "guardrail reason recorded");
    // No new run: only the seed remains.
    assert_eq!(list_runs(dir.path()), vec![seed]);
    // The denial WAS appended to the cross-run registry (audit, KTD1).
    let recorded = DecisionRecorder::new(dir.path()).unwrap().read_all().unwrap();
    assert_eq!(recorded.len(), 1, "the denied cycle is an audit record");
}

#[tokio::test]
async fn ae7_fresh_home_resolves_to_the_committed_default() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    // No seed run → a fresh registry. Current params resolve to OrbParams::default
    // (gap 3.0). 3.0 -> 1.5 is exactly 0.50 — approved. A fresh home needs a range.
    let mut cfg = turn_cfg(dir.path(), "gap_min_pct", 1.5, stamp(10));
    cfg.range = Some(DataRange { start: "20240102".into(), end: "20240105".into() });
    let out = turn(cfg).await.unwrap();
    assert!(out.ran, "approved from the 3.0 default: {:?}", out.refusal);
    let m = read_manifest(dir.path(), out.run_id.as_ref().unwrap());
    assert_eq!(m.params.gap_min_pct, 1.5);
    assert_eq!(m.strategy_version, 1, "default version 0 -> 1");
}

#[tokio::test]
async fn fresh_home_without_a_range_errors() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let err = turn(turn_cfg(dir.path(), "gap_min_pct", 1.5, stamp(10))).await.unwrap_err();
    assert!(err.to_string().contains("range is required"), "{err}");
}

#[tokio::test]
async fn a_no_op_target_errors_clearly_not_with_a_mismatch_message() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    seed_run(dir.path(), 2.4, 0, stamp(0)).await;
    // Propose the current value (2.4) — a no-op. It must error with a clear
    // message, not approve-bump-then-refuse with a confusing mismatch.
    let err = turn(turn_cfg(dir.path(), "gap_min_pct", 2.4, stamp(10))).await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("no-op"), "clear no-op message: {msg}");
    assert!(!msg.contains("touches"), "not the mismatch message: {msg}");
    assert_eq!(list_runs(dir.path()).len(), 1, "no new run");
}

#[tokio::test]
async fn mismatched_override_set_is_refused_before_backtest() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let seed = seed_run(dir.path(), 2.4, 0, stamp(0)).await;

    // The envelope authorizes gap_min_pct, but the executed override set touches a
    // different parameter — refuse before the backtest (R1).
    let mut cfg = turn_cfg(dir.path(), "gap_min_pct", 1.2, stamp(10));
    cfg.applied_overrides =
        Some(std::collections::BTreeMap::from([("max_concurrent".to_string(), 3.0)]));
    let out = turn(cfg).await.unwrap();
    assert!(!out.ran, "no backtest on a mismatch");
    assert!(out.refusal.unwrap().contains("differs"), "names the mismatch");
    assert_eq!(list_runs(dir.path()), vec![seed], "no new run");
}

#[tokio::test]
async fn a_denied_turn_consumes_no_version() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    seed_run(dir.path(), 2.4, 5, stamp(0)).await;

    // Deny first (0.75 change), then approve (0.50 change).
    let denied = turn(turn_cfg(dir.path(), "gap_min_pct", 0.6, stamp(10))).await.unwrap();
    assert!(!denied.ran);
    let approved = turn(turn_cfg(dir.path(), "gap_min_pct", 1.2, stamp(20))).await.unwrap();
    assert!(approved.ran);
    assert_eq!(approved.version, Some(6), "prior 5 + 1 — the denial consumed no version");
}

#[tokio::test]
async fn a_failed_backtest_self_heals_the_next_turn() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let seed = seed_run(dir.path(), 2.4, 0, stamp(0)).await;

    // Hold the ingest lock so an approved turn's backtest refuses to start —
    // the turn errors after appending its (approved) envelope, leaving no run.
    {
        let _held = AdvisoryLock::acquire(&dir.path().join("catalog"), LockKind::Ingest).unwrap();
        let err = turn(turn_cfg(dir.path(), "gap_min_pct", 1.2, stamp(10))).await.unwrap_err();
        assert!(err.to_string().contains("refused"), "{err}");
    }
    assert_eq!(list_runs(dir.path()), vec![seed.clone()], "no finalized run from the failure");

    // The next turn still resolves current params from the prior finalized
    // manifest — version prior + 1, not prior + 2.
    let healed = turn(turn_cfg(dir.path(), "gap_min_pct", 1.2, stamp(20))).await.unwrap();
    assert!(healed.ran);
    assert_eq!(healed.version, Some(1), "self-heal: current is still the seed at v0");
}

#[tokio::test]
async fn range_is_inherited_without_an_env_override() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let seed = seed_run(dir.path(), 2.4, 0, stamp(0)).await;
    let seed_range = read_manifest(dir.path(), &seed).data_range;

    let out = turn(turn_cfg(dir.path(), "gap_min_pct", 1.2, stamp(10))).await.unwrap();
    let m = read_manifest(dir.path(), out.run_id.as_ref().unwrap());
    assert_eq!(m.data_range, seed_range, "range inherited from the prior run");
}

#[tokio::test]
async fn rerun_mode_produces_a_data_turn_comparable_pair() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let seed = seed_run(dir.path(), 2.4, 0, stamp(0)).await;

    // No override → rerun: same params, no governance cycle, no version bump.
    let mut cfg = TurnConfig::new(dir.path(), stamp(10));
    cfg.override_param = None;
    let out = turn(cfg).await.unwrap();
    assert!(out.ran);
    assert_eq!(out.approved, None, "a rerun runs no governance cycle");
    assert_eq!(out.version, Some(0), "no version bump");

    // The pair passes the data-turn verdict: zero-key param diff, equal code hash
    // (identical catalog + range → no data deltas either).
    let verdict = compare(&CompareConfig {
        data_home: dir.path().to_path_buf(),
        run_a: Some(seed),
        run_b: out.run_id,
        mode: CompareMode::Data,
        explanation: None,
    })
    .unwrap();
    assert!(verdict.pass, "data-turn verdict: {:?}", verdict.lines);
}

// ---------------------------------------------------------------------------
// U4 / KTD-5 — the v3-param resolution assertion (fresh-home seed guard).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fresh_home_rerun_without_the_seeded_v3_manifest_is_a_stop_condition() {
    // A fresh home with no seeded manifest resolves OrbParams::default (v0, gap 3.0).
    // With the expected v3 identity pinned, the turn must REFUSE rather than run a
    // silent default-param backtest.
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let mut cfg = TurnConfig::new(dir.path(), stamp(10));
    cfg.override_param = None; // rerun
    cfg.range = Some(DataRange { start: "20240102".into(), end: "20240105".into() });
    cfg.expect_version = Some(3);
    cfg.expect_gap_min_pct = Some(0.6);
    let err = turn(cfg).await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("v3-param resolution failed"), "names the stop condition: {msg}");
    assert!(msg.contains("KTD-5"), "points at the seed remedy: {msg}");
    // Nothing ran.
    assert!(list_runs(dir.path()).is_empty(), "no backtest on the stop condition");
}

#[tokio::test]
async fn seeded_v3_manifest_satisfies_the_resolution_assertion_and_reruns() {
    // Seed a v3 / gap 0.6 finalized run (stands in for the copied turn-2b v3
    // manifest). The expected-identity rerun now resolves it and runs.
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    seed_run(dir.path(), 0.6, 3, stamp(0)).await;

    let mut cfg = TurnConfig::new(dir.path(), stamp(10));
    cfg.override_param = None; // rerun
    cfg.expect_version = Some(3);
    cfg.expect_gap_min_pct = Some(0.6);
    let out = turn(cfg).await.unwrap();
    assert!(out.ran, "seeded v3 resolves and reruns: {:?}", out.refusal);
    assert_eq!(out.version, Some(3), "no version bump on a rerun");
    let m = read_manifest(dir.path(), out.run_id.as_ref().unwrap());
    assert_eq!(m.params.gap_min_pct, 0.6);
    assert_eq!(m.strategy_version, 3);
}

#[tokio::test]
async fn a_matching_version_with_a_wrong_gap_trips_the_gap_assertion() {
    // Version matches (v3) so the version guard passes, but the resolved gap (0.9)
    // differs from the expected 0.6 → the gap guard bails. Exercises the gap branch
    // + its float-tolerance compare, which the version-mismatch cases never reach.
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    seed_run(dir.path(), 0.9, 3, stamp(0)).await;
    let mut cfg = TurnConfig::new(dir.path(), stamp(10));
    cfg.override_param = None;
    cfg.expect_version = Some(3); // passes
    cfg.expect_gap_min_pct = Some(0.6); // resolved 0.9 ≠ 0.6 → bail
    let err = turn(cfg).await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("v3-param resolution failed"), "names the stop condition: {msg}");
    assert!(msg.contains("0.6000"), "names the expected gap: {msg}");
    assert_eq!(list_runs(dir.path()).len(), 1, "no new run — only the seed remains");
}

#[tokio::test]
async fn a_wrong_seeded_version_trips_the_resolution_assertion() {
    // A home whose latest run is v0 (gap 3.0) while the operator expects v3 → refuse.
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    seed_run(dir.path(), 3.0, 0, stamp(0)).await;
    let mut cfg = TurnConfig::new(dir.path(), stamp(10));
    cfg.override_param = None;
    cfg.expect_version = Some(3);
    let err = turn(cfg).await.unwrap_err();
    assert!(err.to_string().contains("v3-param resolution failed"), "{err}");
}

// ===========================================================================
// U3 — runs compare
// ===========================================================================

#[tokio::test]
async fn ae3_param_turn_pair_passes_the_param_verdict() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    seed_run(dir.path(), 2.4, 0, stamp(0)).await;
    let out = turn(turn_cfg(dir.path(), "gap_min_pct", 1.2, stamp(10))).await.unwrap();
    assert!(out.ran);

    // Default selection: the two newest finalized runs.
    let verdict = compare(&CompareConfig {
        data_home: dir.path().to_path_buf(),
        run_a: None,
        run_b: None,
        mode: CompareMode::Param,
        explanation: None,
    })
    .unwrap();
    assert!(verdict.pass, "param verdict: {:?}", verdict.lines);
    assert!(
        verdict.lines.iter().any(|l| l.contains("gap_min_pct") && l.contains("strategy_version")),
        "isolates {{gap_min_pct, strategy_version}}: {:?}",
        verdict.lines
    );
}

#[tokio::test]
async fn param_verdict_fails_on_a_three_key_diff() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let seed = seed_run(dir.path(), 2.4, 0, stamp(0)).await;

    // A run that changes TWO params plus the version (built directly).
    let mut cfg2 = BacktestConfig::new(dir.path(), "20240102", "20240105");
    cfg2.params.gap_min_pct = 1.2;
    cfg2.params.max_concurrent = 9; // the extra key
    cfg2.params.strategy_version = 1;
    let other = backtest_run(cfg2, stamp(10)).await.unwrap().run_id;

    let verdict = compare(&CompareConfig {
        data_home: dir.path().to_path_buf(),
        run_a: Some(seed),
        run_b: Some(other),
        mode: CompareMode::Param,
        explanation: None,
    })
    .unwrap();
    assert!(!verdict.pass, "three-key diff must FAIL: {:?}", verdict.lines);
    assert!(
        verdict.lines.iter().any(|l| l.contains("max_concurrent")),
        "names the extra key: {:?}",
        verdict.lines
    );
}

/// Turn-8 / KTD5: a **code turn** (v9) bumps only `strategy_version` — its fixed
/// profit target rides `orb.rs`, not a swept param (`profit_target_r`'s default
/// keeps it out of `param_diff`). The param-mode verdict therefore FAILs on the
/// version-only diff: no `runs compare` mode PASSes a code turn, so the turn is
/// judged on the edge bar, not a green compare.
#[tokio::test]
async fn param_verdict_fails_on_a_version_only_code_turn_diff() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    // v8 baseline then a v9 run with identical params (a code-turn shape).
    let v8 = seed_run(dir.path(), 2.4, 8, stamp(0)).await;
    let v9 = seed_run(dir.path(), 2.4, 9, stamp(10)).await;

    let verdict = compare(&CompareConfig {
        data_home: dir.path().to_path_buf(),
        run_a: Some(v8),
        run_b: Some(v9),
        mode: CompareMode::Param,
        explanation: None,
    })
    .unwrap();
    assert!(!verdict.pass, "a version-only (code-turn) diff FAILs param mode: {:?}", verdict.lines);
    assert!(
        verdict.lines.iter().any(|l| l.contains("must be exactly")),
        "the FAIL names the not-exactly-{{version, one param}} diff: {:?}",
        verdict.lines
    );
}

#[tokio::test]
async fn data_verdict_requires_an_explanation_for_deltas() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let seed = seed_run(dir.path(), 2.4, 0, stamp(0)).await;

    // A rerun over a WIDER range: same params/code, but the range (and so the
    // range-scoped fingerprint) differs — a data-turn shape.
    let mut wide = BacktestConfig::new(dir.path(), "20240101", "20240105");
    wide.params.gap_min_pct = 2.4;
    let wide_run = backtest_run(wide, stamp(10)).await.unwrap().run_id;

    let base = CompareConfig {
        data_home: dir.path().to_path_buf(),
        run_a: Some(seed.clone()),
        run_b: Some(wide_run.clone()),
        mode: CompareMode::Data,
        explanation: None,
    };
    // No explanation → FAIL.
    assert!(!compare(&base).unwrap().pass, "unexplained data delta FAILs");
    // With an explanation → PASS, and the explanation appears in the output.
    let explained = CompareConfig {
        explanation: Some("widened the ingest slice by one prior session".into()),
        ..base
    };
    let verdict = compare(&explained).unwrap();
    assert!(verdict.pass, "explained data delta PASSes: {:?}", verdict.lines);
    assert!(verdict.lines.iter().any(|l| l.contains("widened the ingest slice")));
}

#[tokio::test]
async fn compare_refuses_a_single_sided_run_selection() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let seed = seed_run(dir.path(), 2.4, 0, stamp(0)).await;
    // Only run_a set → refuse rather than silently defaulting to the two newest.
    let err = compare(&CompareConfig {
        data_home: dir.path().to_path_buf(),
        run_a: Some(seed),
        run_b: None,
        mode: CompareMode::Param,
        explanation: None,
    })
    .unwrap_err();
    assert!(err.to_string().contains("both LS_COMPARE_A and LS_COMPARE_B"), "{err}");
}

#[tokio::test]
async fn data_verdict_fails_on_a_nonzero_param_diff() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let seed = seed_run(dir.path(), 2.4, 0, stamp(0)).await;
    let param_turn = turn(turn_cfg(dir.path(), "gap_min_pct", 1.2, stamp(10))).await.unwrap();

    let verdict = compare(&CompareConfig {
        data_home: dir.path().to_path_buf(),
        run_a: Some(seed),
        run_b: param_turn.run_id,
        mode: CompareMode::Data,
        explanation: Some("irrelevant".into()),
    })
    .unwrap();
    assert!(!verdict.pass, "a param diff is not a data turn: {:?}", verdict.lines);
}

// ===========================================================================
// U4 — replay guard
// ===========================================================================

#[tokio::test]
async fn ae4_telemetry_only_stream_is_refused() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let seed = seed_run(dir.path(), 2.4, 0, stamp(0)).await;

    // A run dir's decisions.jsonl is all telemetry (NotEvaluated stages).
    let stream = dir.path().join("runs").join(&seed).join("decisions.jsonl");
    assert!(stream.exists(), "the run has a telemetry stream");
    let out = replay_guard(&ReplayConfig { stream_path: stream, max_relative_change: 0.25 }).unwrap();
    assert!(out.refused, "telemetry-only stream refused");
    assert_eq!(out.evaluated_count, 0);
    assert!(
        out.lines.iter().any(|l| l.contains("telemetry-only")),
        "explicit refusal message: {:?}",
        out.lines
    );
}

#[tokio::test]
async fn evaluated_stream_reports_a_divergence_under_a_tighter_cap() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    seed_run(dir.path(), 2.4, 0, stamp(0)).await;
    // An approved 2.4 -> 1.2 turn appends an intent-bearing envelope to the
    // cross-run registry (evaluated under the 0.5 pin).
    let approved = turn(turn_cfg(dir.path(), "gap_min_pct", 1.2, stamp(10))).await.unwrap();
    assert!(approved.ran);

    let stream = dir.path().join("decisions").join("decisions.jsonl");
    // Replay under a cap of 0.1: the recorded 0.5 approval now diverges (rejected).
    let out = replay_guard(&ReplayConfig { stream_path: stream, max_relative_change: 0.1 }).unwrap();
    assert!(!out.refused, "the stream has an evaluated cycle");
    assert_eq!(out.evaluated_count, 1);
    assert_eq!(out.delta_count, 1, "the 0.5 approval diverges under the 0.1 cap");
    assert_eq!(out.first_divergence, Some(0));
}

// ===========================================================================
// U5 — catalog status
// ===========================================================================

#[tokio::test]
async fn healthy_catalog_prints_per_triple_facts_and_is_a_go() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let out = catalog_status(&StatusConfig {
        data_home: dir.path().to_path_buf(),
        expected_range: None,
    })
    .await
    .unwrap();
    assert!(out.go, "healthy fixture is a go: {:?}", out.lines);
    // One daily + one minute triple for 005930.
    assert_eq!(out.triples.len(), 2, "one triple per (instrument, bar-kind): {:?}", out.triples);
    assert!(out.triples.iter().all(|t| t.count > 0 && t.flags.is_empty()));
}

#[tokio::test]
async fn ae5_tail_undershoot_vs_the_watermark_is_flagged() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    // Advance the daily watermark PAST the last bar (20240105) → the span
    // undershoots the completed range.
    let cp_path = dir.path().join("catalog").join("ingest-checkpoint.json");
    let mut cp = Checkpoint::load(&cp_path).unwrap();
    cp.set_watermark("005930.XKRX", "1-DAY", chrono::NaiveDate::from_ymd_opt(2024, 1, 31).unwrap());
    cp.save(&cp_path).unwrap();

    let out = catalog_status(&StatusConfig {
        data_home: dir.path().to_path_buf(),
        expected_range: None,
    })
    .await
    .unwrap();
    assert!(!out.go, "undershoot is a no-go");
    assert!(
        out.triples.iter().any(|t| t.bar_kind == "1-DAY" && !t.flags.is_empty()),
        "the daily triple is flagged: {:?}",
        out.triples
    );
}

#[tokio::test]
async fn weekend_watermark_does_not_false_flag_a_friday_closed_catalog() {
    // Accumulate advances the checkpoint watermark to the calendar last-closed
    // session even when that lands on a weekend (documented `last_closed_session`
    // behavior). A catalog whose last bar is the immediately preceding Friday is
    // healthy — the raw watermark comparison used to false-flag it as a tail
    // undershoot, turning a fine catalog into a NO-GO. (Turn-2b certification.)
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let cp_path = dir.path().join("catalog").join("ingest-checkpoint.json");
    let mut cp = Checkpoint::load(&cp_path).unwrap();
    // Last bar in the fixture is 20240105 (Friday); watermark advances to
    // 20240106 (Saturday) and 20240107 (Sunday) — both non-sessions.
    for wm in ["20240106", "20240107"] {
        cp.set_watermark(
            "005930.XKRX",
            "1-DAY",
            chrono::NaiveDate::parse_from_str(wm, "%Y%m%d").unwrap(),
        );
        cp.save(&cp_path).unwrap();
        let out = catalog_status(&StatusConfig {
            data_home: dir.path().to_path_buf(),
            expected_range: None,
        })
        .await
        .unwrap();
        assert!(
            out.go,
            "a Friday-closed catalog under a {wm} watermark is a go, not a false undershoot: {:?}",
            out.lines
        );
        assert!(
            out.triples.iter().all(|t| t.flags.is_empty()),
            "no tail flag when the watermark is a weekend ({wm}): {:?}",
            out.triples
        );
    }
}

#[tokio::test]
async fn genuine_undershoot_across_a_weekend_still_flags() {
    // The walk-back must not OVER-suppress: a Monday watermark (20240108) with the
    // last bar on the prior Friday (20240105) is a real tail undershoot — Monday is
    // a weekday, so last_weekday_on_or_before(Mon) = Mon, and Fri < Mon flags. This
    // guards against the walk-back ever being widened to skip a session day.
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let cp_path = dir.path().join("catalog").join("ingest-checkpoint.json");
    let mut cp = Checkpoint::load(&cp_path).unwrap();
    cp.set_watermark(
        "005930.XKRX",
        "1-DAY",
        chrono::NaiveDate::from_ymd_opt(2024, 1, 8).unwrap(), // Monday
    );
    cp.save(&cp_path).unwrap();
    let out = catalog_status(&StatusConfig {
        data_home: dir.path().to_path_buf(),
        expected_range: None,
    })
    .await
    .unwrap();
    assert!(!out.go, "a Friday last bar under a Monday watermark is a genuine undershoot");
    assert!(
        out.triples.iter().any(|t| t.bar_kind == "1-DAY" && !t.flags.is_empty()),
        "the daily triple is flagged: {:?}",
        out.triples
    );
}

#[tokio::test]
async fn front_truncation_is_flagged_only_with_an_expected_range() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    // Bars start 20240104; an expected range starting earlier reveals front
    // truncation — undetectable from the checkpoint alone.
    let with_expected = catalog_status(&StatusConfig {
        data_home: dir.path().to_path_buf(),
        expected_range: Some(DataRange { start: "20240101".into(), end: "20240105".into() }),
    })
    .await
    .unwrap();
    assert!(!with_expected.go, "front truncation is a no-go");
    assert!(with_expected
        .triples
        .iter()
        .any(|t| t.flags.iter().any(|f| f.contains("front truncation"))));

    // Without the expected range, the same catalog is a go (checkpoint watermark
    // is unset in the fixture).
    let without = catalog_status(&StatusConfig {
        data_home: dir.path().to_path_buf(),
        expected_range: None,
    })
    .await
    .unwrap();
    assert!(without.go, "no front check without an expected range: {:?}", without.lines);
}

#[tokio::test]
async fn missing_catalog_dir_is_a_clean_no_go_not_a_panic() {
    let dir = tempdir().unwrap();
    // No fixture — the catalog dir does not exist.
    let err = catalog_status(&StatusConfig {
        data_home: dir.path().to_path_buf(),
        expected_range: None,
    })
    .await
    .unwrap_err();
    assert!(err.to_string().contains("no catalog"), "clean error: {err}");
}

// ===========================================================================
// U6 — analyze --scaffold
// ===========================================================================

#[tokio::test]
async fn scaffold_prefills_run_facts_and_the_verdict_skeleton() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let run_id = seed_run(dir.path(), 2.4, 0, stamp(0)).await;

    let out = analyze_scaffold(&ScaffoldConfig {
        data_home: dir.path().to_path_buf(),
        run_id: run_id.clone(),
    })
    .unwrap();
    let content = std::fs::read_to_string(&out.path).unwrap();
    // Params, trade count, gap summary, and the three verdict words.
    assert!(content.contains("gap_min_pct"), "params present: {content}");
    assert!(content.contains("num_trades"), "trade count present");
    assert!(content.contains("coverage-gap"), "gap-noise summary present");
    for word in ["keep", "revert", "insufficient-evidence"] {
        assert!(content.contains(word), "verdict skeleton names {word:?}");
    }
    // A structured symbol renders VERBATIM (a 6-digit shcode must not be masked).
    assert!(content.contains("005930.XKRX"), "symbol unmasked in the structured list: {content}");
    // U3: the computed turn-5 edge-quality section renders (frequency/breadth bar
    // retired, dominance kept). On this single-symbol fixture the lone winner carries
    // 100% of |P&L| → dominance trips → not an edge, even though expectancy is positive.
    assert!(content.contains("Edge quality (R4)"), "edge section present: {content}");
    assert!(content.contains("Win rate"), "win-rate stat surfaced: {content}");
    assert!(content.contains("Expectancy"), "expectancy stat surfaced: {content}");
    assert!(
        content.contains("single-symbol dominance"),
        "dominance retained and named: {content}"
    );
    assert!(content.contains("**Edge:** no"), "single-symbol dominance trips the edge: {content}");
    // The retired frequency bar leaves no trace.
    assert!(!content.contains("trade-count floor"), "frequency bar retired: {content}");
    assert!(!content.contains("Decisiveness bar"), "decisiveness bar retired: {content}");
    // The per-symbol table must be well-formed GFM: the header and delimiter rows
    // carry the same cell count (a literal `|P&L|` in the header would split it into
    // more cells than the delimiter and GFM would not render a table at all).
    let header = content.lines().find(|l| l.trim_start().starts_with("| Symbol |")).expect("header row");
    let delim = content.lines().find(|l| l.trim_start().starts_with("|---|")).expect("delimiter row");
    assert_eq!(
        header.matches('|').count(),
        delim.matches('|').count(),
        "table header/delimiter pipe counts must match:\n  header: {header}\n  delim:  {delim}"
    );
}

#[tokio::test]
async fn scaffold_refuses_to_overwrite_an_existing_analysis() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let run_id = seed_run(dir.path(), 2.4, 0, stamp(0)).await;
    let cfg = ScaffoldConfig { data_home: dir.path().to_path_buf(), run_id };
    analyze_scaffold(&cfg).unwrap();
    let err = analyze_scaffold(&cfg).unwrap_err();
    assert!(err.to_string().contains("already exists"), "refuses overwrite: {err}");
}

#[tokio::test]
async fn scaffold_masks_an_account_like_token_in_free_text() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let run_id = seed_run(dir.path(), 2.4, 0, stamp(0)).await;

    // Inject a free-text observation carrying an account-like token into the
    // finalized data-quality report, then scaffold.
    let dq_path = dir.path().join("runs").join(&run_id).join("data_quality.json");
    let mut dq: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&dq_path).unwrap()).unwrap();
    dq["observations"] = json!(["operator note: acct 20187511401 flagged"]);
    std::fs::write(&dq_path, serde_json::to_string_pretty(&dq).unwrap()).unwrap();

    let out = analyze_scaffold(&ScaffoldConfig {
        data_home: dir.path().to_path_buf(),
        run_id,
    })
    .unwrap();
    let content = std::fs::read_to_string(&out.path).unwrap();
    assert!(!content.contains("20187511401"), "account token masked in free text: {content}");
    assert!(content.contains("***"), "masking marker present");
    // The structured symbol is still unmasked.
    assert!(content.contains("005930.XKRX"), "structured symbol survives");
}

// ===========================================================================
// catalog compact (U5 — write-side remediation CLI)
// ===========================================================================

#[test]
fn unknown_catalog_subcommand_lists_compact() {
    let out = bin().args(["catalog", "bogus"]).output().unwrap();
    assert!(!out.status.success(), "unknown catalog subcommand is a non-zero exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("compact"), "the catalog usage lists compact: {stderr}");
}

#[tokio::test]
async fn compact_cli_exits_zero_on_a_clean_catalog_and_status_stays_go() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let out = bin().args(["catalog", "compact"]).env("LS_DATA_HOME", dir.path()).output().unwrap();
    assert!(
        out.status.success(),
        "a clean catalog compacts with exit 0: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("compact: OK"), "reports OK");
    // The compacted fixture is still a go.
    let status = catalog_status(&StatusConfig {
        data_home: dir.path().to_path_buf(),
        expected_range: None,
    })
    .await
    .unwrap();
    assert!(status.go, "the compacted fixture is still a go: {:?}", status.lines);
}

#[tokio::test]
async fn compact_cli_exits_nonzero_on_a_value_divergent_series() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    // Inject a value-divergent same-timestamp daily row (Jan 4, close 999 vs the
    // fixture's 60000) into 005930's series as a second file.
    let catalog = dir.path().join("catalog");
    let bt = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
    let divergent = build_daily_bar(
        bt,
        &serde_json::from_value(daily_json("20240104", "999", "1000", "998", "999", "1")).unwrap(),
    )
    .unwrap()
    .unwrap();
    write_bars(&catalog, vec![divergent]).await.unwrap();

    let out = bin().args(["catalog", "compact"]).env("LS_DATA_HOME", dir.path()).output().unwrap();
    assert!(!out.status.success(), "a refused divergent series exits non-zero");
    assert!(String::from_utf8_lossy(&out.stdout).contains("REFUSED"), "{}", String::from_utf8_lossy(&out.stdout));
}

#[tokio::test]
async fn catalog_compact_library_reports_clean_on_the_fixture() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let out = catalog_compact(&CompactConfig { data_home: dir.path().to_path_buf() }).await.unwrap();
    assert!(!out.refused, "the clean fixture is not refused");
    assert!(out.lines.iter().any(|l| l.contains("compact: OK")));
}

// ===========================================================================
// `report tiers` — the Turn-N per-tier count report (plan 2026-07-10-003, U6)
// ===========================================================================

mod report_tiers {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use chrono::{TimeZone, Utc};
    use nautilus_ls::reference::universe_metadata::{
        CapTier, ConditionerTags, IndexMembership, InstrumentMetadata, LiquidityTier, MarketClass,
        MetadataPin, MetadataProvenance, Resolved, UniverseMetadata,
    };
    use nautilus_ls_lab::agent::context::AgentContext;
    use nautilus_ls_lab::agent::envelope::{
        to_jsonl, Decision, DecisionDetail, DecisionEnvelope, DecisionTrigger, SignalKind,
    };
    use nautilus_ls_lab::artifacts::{RunSource, DECISIONS_FILE};
    use nautilus_ls_lab::params::OrbParams;
    use nautilus_ls_lab::runner::report::{report_tiers, TiersConfig};
    use tempfile::tempdir;

    use super::*;

    fn tags(market: MarketClass, cap: CapTier) -> ConditionerTags {
        ConditionerTags {
            cap_tier: cap,
            liquidity_tier: LiquidityTier::Unknown,
            market_class: market,
            index_membership: Resolved::Proxy(IndexMembership::NotMember),
            has_derivative: Resolved::Value(false),
        }
    }

    fn record(shcode: &str, market: MarketClass, cap_tier: CapTier) -> InstrumentMetadata {
        let market_cap = match cap_tier {
            CapTier::BelowBoard => Resolved::Unavailable,
            _ => Resolved::Value(1_000_000.0),
        };
        InstrumentMetadata {
            shcode: shcode.to_string(),
            market_class: market,
            market_cap,
            cap_tier,
            turnover: Resolved::Unavailable,
            liquidity_tier: LiquidityTier::Unknown,
            index_membership: Resolved::Proxy(IndexMembership::NotMember),
            has_derivative: Resolved::Value(false),
            designation: None,
            tradable: true,
        }
    }

    /// 10:00 KST on 2026-06-`day` as UTC unix ns.
    fn ts_kst(day: u32) -> u64 {
        Utc.with_ymd_and_hms(2026, 6, day, 1, 0, 0).unwrap().timestamp_nanos_opt().unwrap() as u64
    }

    /// One tagged accept + one exit for `symbol` on 2026-06-`day`.
    fn trade(symbol: &str, day: u32, t: ConditionerTags) -> Vec<DecisionEnvelope> {
        let env = |ts: u64, detail: DecisionDetail| {
            DecisionEnvelope::telemetry(
                ts,
                DecisionTrigger::Manual { reason: "test".to_string() },
                detail,
                AgentContext::telemetry("orb", 13, BTreeMap::new(), BTreeMap::new()),
            )
        };
        vec![
            env(
                ts_kst(day),
                DecisionDetail::universe(symbol, Decision::Accept, None, BTreeMap::new())
                    .with_tags(Some(t)),
            ),
            env(
                ts_kst(day) + 3_600_000_000_000,
                DecisionDetail::transition(
                    symbol,
                    SignalKind::TimeExit,
                    [("mfe_r".to_string(), 0.5), ("price".to_string(), 100.0)].into(),
                ),
            ),
        ]
    }

    /// Write a full synthetic metadata-driven run: artifact + catalog daily
    /// bars + ingest pin + manifest (hash-stamped) + decisions. Returns the
    /// data home and the run id.
    async fn build_tiers_fixture(
        blue_chip_trades: u32,
        exclusion_trades: u32,
    ) -> (tempfile::TempDir, String) {
        let dir = tempdir().unwrap();
        let data_home = dir.path().to_path_buf();

        // The artifact: one blue-chip KOSPI name, one below-board KOSDAQ name.
        let artifact = UniverseMetadata {
            provenance: MetadataProvenance {
                captured_at: "2026-06-01T00:30:00Z".to_string(),
                session_date: "20260601".to_string(),
                source_trs: vec!["t8430".into()],
                instrument_type_filter: "equities-only (test fixture)".to_string(),
                tier_boundary_rule: "test".to_string(),
                cap_cutoffs: Vec::new(),
                paper_incompatible: Vec::new(),
            },
            records: vec![
                record("005930", MarketClass::Kospi, CapTier::Top),
                record("300001", MarketClass::Kosdaq, CapTier::BelowBoard),
            ],
        };
        let artifact_path = data_home.join("universe-metadata.json");
        std::fs::write(&artifact_path, serde_json::to_string_pretty(&artifact).unwrap()).unwrap();
        let hash = artifact.content_hash();

        // Catalog daily bars for the gap distribution: 005930 gaps +5%
        // (60000 → 63000 open), 300001 gaps +1% (10000 → 10100 open).
        let catalog = data_home.join("catalog");
        for (sym, rows) in [
            (
                "005930.XKRX",
                [
                    daily_json("20260601", "59000", "60500", "58500", "60000", "1000"),
                    daily_json("20260602", "63000", "64500", "62000", "64000", "1000"),
                ],
            ),
            (
                "300001.XKRX",
                [
                    daily_json("20260601", "9900", "10100", "9800", "10000", "1000"),
                    daily_json("20260602", "10100", "10300", "10000", "10200", "1000"),
                ],
            ),
        ] {
            let bt = BarKind::Daily.bar_type(InstrumentId::from(sym)).unwrap();
            let bars: Vec<Bar> = rows
                .iter()
                .map(|r| {
                    build_daily_bar(bt, &serde_json::from_value(r.clone()).unwrap())
                        .unwrap()
                        .unwrap()
                })
                .collect();
            write_bars(&catalog, bars).await.unwrap();
        }

        // The ingest pin (KTD2's ingest-side half).
        MetadataPin {
            artifact_path: artifact_path.display().to_string(),
            content_hash: hash.clone(),
            per_stratum: BTreeMap::from([
                ("kospi_blue_chip".to_string(), 1usize),
                ("small_cap_exclusion".to_string(), 1usize),
            ]),
            symbols: vec!["005930".to_string(), "300001".to_string()],
            pinned_at: "2026-06-01T01:00:00Z".to_string(),
        }
        .write(&catalog)
        .unwrap();

        // The finalized run: manifest (hash-stamped) + tagged decisions.
        let run_id = "20260630T000000Z-backtest-orb-v13".to_string();
        let run_dir = data_home.join("runs").join(&run_id);
        std::fs::create_dir_all(&run_dir).unwrap();
        let mut params = OrbParams::default();
        params.strategy_version = 13;
        let manifest = Manifest {
            run_id: run_id.clone(),
            source: RunSource::Backtest,
            strategy_id: "orb".to_string(),
            strategy_version: 13,
            params,
            data_range: DataRange { start: "20260601".to_string(), end: "20260630".to_string() },
            catalog_fingerprint: "fp".to_string(),
            universe_hash: "uh".to_string(),
            strategy_code_hash: "ch".to_string(),
            checkpoint_hash: None,
            universe_metadata_hash: Some(hash),
            created_utc: "2026-06-30T07:00:00Z".to_string(),
        };
        std::fs::write(run_dir.join(MANIFEST_FILE), serde_json::to_string(&manifest).unwrap())
            .unwrap();

        let mut envelopes = Vec::new();
        for day in 0..blue_chip_trades {
            envelopes.extend(trade(
                "005930.XKRX",
                1 + (day % 30),
                tags(MarketClass::Kospi, CapTier::Top),
            ));
        }
        for day in 0..exclusion_trades {
            envelopes.extend(trade(
                "300001.XKRX",
                1 + (day % 30),
                tags(MarketClass::Kosdaq, CapTier::BelowBoard),
            ));
        }
        std::fs::write(run_dir.join(DECISIONS_FILE), to_jsonl(&envelopes).unwrap()).unwrap();
        (dir, run_id)
    }

    fn tiers_cfg(data_home: &Path, run_id: &str) -> TiersConfig {
        TiersConfig {
            data_home: data_home.to_path_buf(),
            run_id: Some(run_id.to_string()),
            artifact_path: None,
        }
    }

    /// Covers AE1: ≥30 trades in ≥2 tiers → GREEN, with the per-tier gap-%
    /// distribution alongside the counts and no expectancy anywhere.
    #[tokio::test]
    async fn thirty_trades_in_two_tiers_is_a_green_verdict() {
        let (dir, run_id) = build_tiers_fixture(30, 31).await;
        let out = report_tiers(&tiers_cfg(dir.path(), &run_id)).await.unwrap();
        assert!(out.green);
        let joined = out.lines.join("\n");
        assert!(joined.contains("GREEN"), "{joined}");
        assert!(joined.contains("trades 30"), "{joined}");
        assert!(joined.contains("trades 31"), "{joined}");
        // AE2 diagnosability: the gap-% distribution rides every report.
        assert!(joined.contains("opening-gap%"), "{joined}");
        assert!(joined.contains("p50 5.00"), "005930 gapped +5%: {joined}");
        assert!(joined.contains("p50 1.00"), "300001 gapped +1%: {joined}");
        assert!(joined.contains(">= 3%: 1/1"), "blue-chip session clears gap_min 3.0: {joined}");
        assert!(joined.contains(">= 3%: 0/1"), "exclusion session misses gap_min 3.0: {joined}");
        // KTD5 staging guard: counts only.
        for banned in ["expectancy", "pnl", "p&l"] {
            assert!(!joined.to_lowercase().contains(banned), "no {banned}: {joined}");
        }
    }

    /// Covers AE2: fewer than 2 tiers clearing 30 → RED, a valid completion
    /// (Ok, not Err) that calls off Turn N+1.
    #[tokio::test]
    async fn a_thin_run_is_a_red_verdict_not_a_failure() {
        let (dir, run_id) = build_tiers_fixture(30, 4).await;
        let out = report_tiers(&tiers_cfg(dir.path(), &run_id)).await.unwrap();
        assert!(!out.green, "only one tier clears the floor");
        let joined = out.lines.join("\n");
        assert!(joined.contains("RED"), "{joined}");
        assert!(joined.contains("called off"), "{joined}");
    }

    /// Covers KTD2: a pin whose hash differs from the run manifest fails the
    /// report outright — a re-capture between ingest and backtest re-tiers
    /// symbols and corrupts the counts.
    #[tokio::test]
    async fn mismatched_artifact_hashes_fail_the_report() {
        let (dir, run_id) = build_tiers_fixture(5, 5).await;
        let catalog = dir.path().join("catalog");
        let mut pin = MetadataPin::load(&catalog).unwrap().unwrap();
        pin.content_hash = "recaptured-differently".to_string();
        pin.write(&catalog).unwrap();
        let err = report_tiers(&tiers_cfg(dir.path(), &run_id)).await.unwrap_err();
        assert!(err.to_string().contains("hash mismatch"), "{err}");
        assert!(err.to_string().contains("KTD2"), "{err}");
    }

    /// A legacy (metadata-less) run cannot be tier-reported: clean failure
    /// naming the fix, never a silently empty report.
    #[tokio::test]
    async fn a_run_without_metadata_hash_is_a_clean_failure() {
        let (dir, run_id) = build_tiers_fixture(1, 1).await;
        // Strip the hash from the manifest.
        let manifest_path = dir.path().join("runs").join(&run_id).join(MANIFEST_FILE);
        let mut m: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
        m.as_object_mut().unwrap().remove("universe_metadata_hash");
        std::fs::write(&manifest_path, serde_json::to_string(&m).unwrap()).unwrap();
        let err = report_tiers(&tiers_cfg(dir.path(), &run_id)).await.unwrap_err();
        assert!(err.to_string().contains("LS_BT_METADATA"), "{err}");
    }

    /// Dispatch: `report tiers` is reachable through the compiled bin and the
    /// usage line names it.
    #[test]
    fn report_tiers_dispatches_through_the_bin() {
        let out = bin().args(["report", "bogus-tiers"]).output().unwrap();
        assert!(!out.status.success());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("report tiers"), "{stderr}");
        let _ = PathBuf::new(); // keep the import used on all platforms
    }
}
