//! `lab-research` CLI tests (U1–U6). Offline: an in-memory wiremock-ingested
//! fixture catalog feeds real backtest runs into the registry, then the CLI's
//! turn / compare / replay / catalog-status / analyze commands run against them.
//! No credentials, no network beyond the wiremock instrument masters. Dispatch +
//! scrub (U1) are exercised through the compiled bin (`CARGO_BIN_EXE_*`); the
//! command verdicts (U2–U6) through the library functions for structured
//! assertions.

use std::path::Path;
use std::process::Command;

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc, Weekday};
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::ingest::checkpoint::Checkpoint;
use nautilus_ls::ingest::{build_daily_bar, build_minute_bar, write_bars, write_instruments, BarKind};
use nautilus_ls::instruments::{InstrumentDomain, InstrumentProvider};
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use std::collections::BTreeMap;

use nautilus_ls_lab::agent::recording::DecisionRecorder;
use nautilus_ls_lab::artifacts::manifest::{hash_bytes, DataRange, Manifest};
use nautilus_ls_lab::artifacts::{list_runs, MANIFEST_FILE};
use nautilus_ls_lab::runner::backtest::{run as backtest_run, BacktestConfig};
use nautilus_ls_lab::runner::research::{
    analyze_scaffold, catalog_compact, catalog_status_gated, compare,
    latest_finalized_run, replay_guard, turn, CatalogCalendarGate, CompactConfig, CompareConfig,
    CompareMode, GovernedFlip, ReplayConfig, ScaffoldConfig, SessionBoundary, StatusConfig,
    TurnConfig,
};
use nautilus_ls_calendar::schema::{
    Authorization, CalendarScope, Coverage, DayRow, DayStatus, Freshness, Snapshot,
};
use nautilus_ls_calendar::{compute_artifact_id, compute_calendar_id, KrxCalendar};
use nautilus_ls_lab::runner::diagnose::GateExit;
use nautilus_ls_lab::trials::{LookKind, SampleLineage, TrialRecord, TrialsLedger};
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

// --------------------------------------------------------------------------
// U6 — governed-flip fixtures. `write_go` authors a minimal-class candidate with
// a GO gate-verdict + a matching gate-reading ledger record keyed to
// (param, value, anchor_fp); `govern` wraps `turn_cfg` with it, resolving the
// anchor fingerprint from the current head so the guard's fingerprint check
// passes. (The guard's REFUSAL paths get their own bespoke fixtures below.)
// --------------------------------------------------------------------------

fn trials_ledger(home: &Path) -> TrialsLedger {
    TrialsLedger::new(home.join("trials").join("trials.jsonl"))
}

fn candidate_json(slug: &str, param: &str, value: f64) -> Vec<u8> {
    serde_json::to_vec_pretty(&json!({
        "schema_version": 1,
        "slug": slug,
        "family": "class-b",
        "phase_a": "minimal",
        "flip_param": param,
        "flip_value": value,
        "keep_anchor": "size-invariant return-on-risk strict flip PASS"
    }))
    .unwrap()
}

/// Author a GO candidate (minimal Phase-A) + gate-verdict + ledger record.
fn write_go(home: &Path, slug: &str, param: &str, value: f64, anchor_fp: &str) -> GovernedFlip {
    let dir = home.join("candidates").join(slug);
    std::fs::create_dir_all(&dir).unwrap();
    let bytes = candidate_json(slug, param, value);
    std::fs::write(dir.join("candidate.json"), &bytes).unwrap();
    let content_hash = hash_bytes(&bytes);
    let verdict = json!({
        "schema_version": 1,
        "slug": slug,
        "family": "class-b",
        "decision": "GO",
        "diagnostic_readings": {},
        "twin_readings": {},
        "agreed_readings": {},
        "pre_register_hash": content_hash,
        "catalog_fingerprint": anchor_fp,
        "freeze_commit": "commit-fixture",
        "flip_param": param,
        "flip_value": value,
        "recorded_utc": "2026-07-16T00:00:00+00:00"
    });
    std::fs::write(dir.join("gate-verdict.json"), serde_json::to_string_pretty(&verdict).unwrap())
        .unwrap();
    let ledger = trials_ledger(home);
    ledger
        .append(&TrialRecord::new(
            slug,
            "class-b",
            LookKind::GateReading,
            SampleLineage { catalog_fingerprint: anchor_fp.to_string(), parent_fingerprint: None },
            BTreeMap::new(),
            "GO",
            "2026-07-16T00:00:00+00:00",
        ))
        .unwrap();
    GovernedFlip { candidate_dir: dir, ledger }
}

/// The current head's catalog fingerprint (`""` on a fresh home).
fn anchor_fp(home: &Path) -> String {
    latest_finalized_run(home)
        .unwrap()
        .map(|(_, m)| m.catalog_fingerprint)
        .unwrap_or_default()
}

/// A governed param-turn config: `turn_cfg` plus a matching GO candidate.
fn govern(home: &Path, slug: &str, param: &str, target: f64, at: DateTime<Utc>) -> TurnConfig {
    let fp = anchor_fp(home);
    let flip = write_go(home, slug, param, target, &fp);
    let mut cfg = turn_cfg(home, param, target, at);
    cfg.candidate = Some(flip);
    cfg
}

/// Write only `candidate.json` (single-flip, minimal Phase-A); return dir + hash.
fn write_candidate(home: &Path, slug: &str, param: &str, value: f64) -> (std::path::PathBuf, String) {
    let dir = home.join("candidates").join(slug);
    std::fs::create_dir_all(&dir).unwrap();
    let bytes = candidate_json(slug, param, value);
    std::fs::write(dir.join("candidate.json"), &bytes).unwrap();
    (dir, hash_bytes(&bytes))
}

/// Write a gate-verdict into a candidate dir with the given decision + recorded hash.
fn write_verdict(dir: &Path, slug: &str, param: &str, value: f64, anchor: &str, decision: &str, hash: &str) {
    let verdict = json!({
        "schema_version": 1, "slug": slug, "family": "class-b", "decision": decision,
        "diagnostic_readings": {}, "twin_readings": {}, "agreed_readings": {},
        "pre_register_hash": hash, "catalog_fingerprint": anchor,
        "flip_param": param, "flip_value": value, "recorded_utc": "2026-07-16T00:00:00+00:00"
    });
    std::fs::write(dir.join("gate-verdict.json"), serde_json::to_string_pretty(&verdict).unwrap())
        .unwrap();
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
        lab_src_fingerprint: None,
        checkpoint_hash: None,
        universe_metadata_hash: None,
        dispatch: None,
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
fn fingerprint_subcommand_prints_a_64_hex_line_and_exits_zero() {
    // U1/KTD5: the compiled bin reports its embedded lab-source fingerprint as a
    // structured `fingerprint: <hex>` line (verbatim, unmasked), exit 0.
    let out = bin().arg("fingerprint").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().find(|l| l.starts_with("fingerprint: ")).expect("fingerprint line");
    let hex = line.trim_start_matches("fingerprint: ").trim();
    assert_eq!(hex.len(), 64, "SHA-256 hex is 64 chars: {hex}");
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit()), "{hex}");
    assert!(!stdout.contains("***"), "structured fingerprint is not masked: {stdout}");
}

#[test]
fn fingerprint_is_enumerated_in_usage() {
    let out = bin().arg("bogus").output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("fingerprint"), "usage enumerates fingerprint: {stderr}");
}

#[test]
fn a_pre_fingerprint_manifest_still_deserializes_and_round_trips() {
    // U1: the optional lab_src_fingerprint field is serde(default,
    // skip_serializing_if) — a manifest JSON written before the field existed
    // must still parse (as None), and a manifest carrying it must round-trip.
    let legacy = json!({
        "run_id": "20260101T000000Z-backtest-orb-v0",
        "source": "backtest",
        "strategy_id": "orb",
        "strategy_version": 0,
        "params": nautilus_ls_lab::params::OrbParams::default(),
        "data_range": { "start": "20240102", "end": "20240105" },
        "catalog_fingerprint": "fp",
        "universe_hash": "uh",
        "strategy_code_hash": "ch",
        "created_utc": "2026-01-01T00:00:00+00:00"
    });
    let m: Manifest = serde_json::from_value(legacy).unwrap();
    assert!(m.lab_src_fingerprint.is_none(), "absent field deserializes to None");
    // A round-trip of a manifest WITH the field preserves it.
    let mut with = m.clone();
    with.lab_src_fingerprint = Some("deadbeef".repeat(8));
    let back: Manifest = serde_json::from_str(&serde_json::to_string(&with).unwrap()).unwrap();
    assert_eq!(back.lab_src_fingerprint, with.lab_src_fingerprint);
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
    let out = turn(govern(dir.path(), "c", "gap_min_pct", 1.2, stamp(10))).await.unwrap();
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

    // 2.4 -> 0.6 is a relative change of 0.75 — denied (the guard passes on the
    // matching GO; the proposal-bounds cap is what refuses).
    let out = turn(govern(dir.path(), "c", "gap_min_pct", 0.6, stamp(10))).await.unwrap();
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
    let mut cfg = govern(dir.path(), "c", "gap_min_pct", 1.5, stamp(10));
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
    // message, not approve-bump-then-refuse with a confusing mismatch. (The guard
    // passes on the matching GO; the no-op check is what errors.)
    let err = turn(govern(dir.path(), "c", "gap_min_pct", 2.4, stamp(10))).await.unwrap_err();
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
    // different parameter — refuse before the backtest (R1). The flip guard passes
    // (GO for gap_min_pct 1.2); the applied-set mismatch is what refuses.
    let mut cfg = govern(dir.path(), "c", "gap_min_pct", 1.2, stamp(10));
    cfg.applied_overrides = Some(BTreeMap::from([("max_concurrent".to_string(), 3.0)]));
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
    let denied = turn(govern(dir.path(), "c1", "gap_min_pct", 0.6, stamp(10))).await.unwrap();
    assert!(!denied.ran);
    let approved = turn(govern(dir.path(), "c2", "gap_min_pct", 1.2, stamp(20))).await.unwrap();
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
        let err = turn(govern(dir.path(), "c1", "gap_min_pct", 1.2, stamp(10))).await.unwrap_err();
        assert!(err.to_string().contains("refused"), "{err}");
    }
    assert_eq!(list_runs(dir.path()), vec![seed.clone()], "no finalized run from the failure");

    // The next turn still resolves current params from the prior finalized
    // manifest — version prior + 1, not prior + 2.
    let healed = turn(govern(dir.path(), "c2", "gap_min_pct", 1.2, stamp(20))).await.unwrap();
    assert!(healed.ran);
    assert_eq!(healed.version, Some(1), "self-heal: current is still the seed at v0");
}

#[tokio::test]
async fn range_is_inherited_without_an_env_override() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let seed = seed_run(dir.path(), 2.4, 0, stamp(0)).await;
    let seed_range = read_manifest(dir.path(), &seed).data_range;

    let out = turn(govern(dir.path(), "c", "gap_min_pct", 1.2, stamp(10))).await.unwrap();
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

// ===========================================================================
// U6 — the flip guard (R4, R5; AE1). Refusals assert on the typed gate exit, not
// message text.
// ===========================================================================

#[tokio::test]
async fn ae1_editing_the_pre_register_after_a_go_refuses_with_hash_mismatch() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    seed_run(dir.path(), 2.4, 0, stamp(0)).await;
    let fp = anchor_fp(dir.path());
    let flip = write_go(dir.path(), "c", "gap_min_pct", 1.2, &fp);

    // Edit the pre-register (any content change) AFTER its GO.
    let cand = flip.candidate_dir.join("candidate.json");
    let mut s = std::fs::read_to_string(&cand).unwrap();
    s.push('\n');
    std::fs::write(&cand, s).unwrap();

    let mut cfg = turn_cfg(dir.path(), "gap_min_pct", 1.2, stamp(10));
    cfg.candidate = Some(flip);
    let out = turn(cfg).await.unwrap();
    assert_eq!(out.gate_exit, Some(GateExit::PreRegisterHashMismatch), "hash-mismatch gate");
    assert!(!out.ran, "the softened freeze cannot flip");
}

#[tokio::test]
async fn a_param_flip_without_a_candidate_refuses_but_a_rerun_proceeds() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    seed_run(dir.path(), 2.4, 0, stamp(0)).await;

    // Override param, no candidate → refuse (the guard is not opt-in).
    let out = turn(turn_cfg(dir.path(), "gap_min_pct", 1.2, stamp(10))).await.unwrap();
    assert_eq!(out.gate_exit, Some(GateExit::UngovernedFlip));
    assert!(!out.ran);

    // A rerun flips nothing → exempt, proceeds.
    let mut rr = TurnConfig::new(dir.path(), stamp(20));
    rr.override_param = None;
    assert!(turn(rr).await.unwrap().ran, "rerun stays exempt from the flip guard");
}

#[tokio::test]
async fn an_absent_or_stop_verdict_refuses() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    seed_run(dir.path(), 2.4, 0, stamp(0)).await;
    let fp = anchor_fp(dir.path());

    // No verdict written.
    let (cdir, _hash) = write_candidate(dir.path(), "c1", "gap_min_pct", 1.2);
    let mut cfg = turn_cfg(dir.path(), "gap_min_pct", 1.2, stamp(10));
    cfg.candidate = Some(GovernedFlip { candidate_dir: cdir, ledger: trials_ledger(dir.path()) });
    assert_eq!(turn(cfg).await.unwrap().gate_exit, Some(GateExit::NoGoVerdict), "no verdict");

    // A STOP verdict.
    let (cdir2, hash2) = write_candidate(dir.path(), "c2", "gap_min_pct", 1.2);
    write_verdict(&cdir2, "c2", "gap_min_pct", 1.2, &fp, "STOP", &hash2);
    let mut cfg2 = turn_cfg(dir.path(), "gap_min_pct", 1.2, stamp(20));
    cfg2.candidate = Some(GovernedFlip { candidate_dir: cdir2, ledger: trials_ledger(dir.path()) });
    assert_eq!(turn(cfg2).await.unwrap().gate_exit, Some(GateExit::NoGoVerdict), "STOP verdict");
}

#[tokio::test]
async fn a_go_without_a_matching_ledger_record_refuses() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    seed_run(dir.path(), 2.4, 0, stamp(0)).await;
    let fp = anchor_fp(dir.path());

    // GO verdict with the right hash + anchor, but the ledger has no gate-reading.
    let (cdir, hash) = write_candidate(dir.path(), "c", "gap_min_pct", 1.2);
    write_verdict(&cdir, "c", "gap_min_pct", 1.2, &fp, "GO", &hash);
    let empty = TrialsLedger::new(dir.path().join("empty/trials.jsonl"));
    let mut cfg = turn_cfg(dir.path(), "gap_min_pct", 1.2, stamp(10));
    cfg.candidate = Some(GovernedFlip { candidate_dir: cdir, ledger: empty });
    assert_eq!(turn(cfg).await.unwrap().gate_exit, Some(GateExit::MissingLedgerRecord));
}

#[tokio::test]
async fn a_flip_that_does_not_match_the_candidate_declaration_refuses() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    seed_run(dir.path(), 2.4, 0, stamp(0)).await;
    let fp = anchor_fp(dir.path());
    // Candidate GO for gap_min_pct 1.2; the turn proposes 0.9 → mismatch.
    let flip = write_go(dir.path(), "c", "gap_min_pct", 1.2, &fp);
    let mut cfg = turn_cfg(dir.path(), "gap_min_pct", 0.9, stamp(10));
    cfg.candidate = Some(flip);
    assert_eq!(turn(cfg).await.unwrap().gate_exit, Some(GateExit::FlipMismatch));
}

#[tokio::test]
async fn an_undeclared_sweep_leg_refuses() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    seed_run(dir.path(), 2.4, 0, stamp(0)).await;
    let fp = anchor_fp(dir.path());

    // A sweep candidate with legs {0.3, 0.5, 0.7}; a matching GO record.
    let cdir = dir.path().join("candidates").join("sweep");
    std::fs::create_dir_all(&cdir).unwrap();
    let bytes = serde_json::to_vec_pretty(&json!({
        "schema_version": 1, "slug": "sweep", "family": "class-b", "phase_a": "minimal",
        "sweep_param": "gap_min_pct", "sweep_legs": [0.3, 0.5, 0.7],
        "keep_anchor": "RoR PASS"
    }))
    .unwrap();
    std::fs::write(cdir.join("candidate.json"), &bytes).unwrap();
    let hash = hash_bytes(&bytes);
    write_verdict(&cdir, "sweep", "gap_min_pct", 0.5, &fp, "GO", &hash);
    let ledger = trials_ledger(dir.path());
    ledger
        .append(&TrialRecord::new(
            "sweep", "class-b", LookKind::GateReading,
            SampleLineage { catalog_fingerprint: fp.clone(), parent_fingerprint: None },
            BTreeMap::new(), "GO", "2026-07-16T00:00:00+00:00",
        ))
        .unwrap();

    // 0.4 is not a declared leg → refuse.
    let mut cfg = turn_cfg(dir.path(), "gap_min_pct", 0.4, stamp(10));
    cfg.candidate = Some(GovernedFlip { candidate_dir: cdir, ledger });
    assert_eq!(turn(cfg).await.unwrap().gate_exit, Some(GateExit::FlipMismatch), "undeclared leg");
}

#[tokio::test]
async fn catalog_fingerprint_drift_between_verdict_and_anchor_refuses() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    seed_run(dir.path(), 2.4, 0, stamp(0)).await;
    // The GO ran against a different sample than the anchor run.
    let flip = write_go(dir.path(), "c", "gap_min_pct", 1.2, "WRONG_FINGERPRINT");
    let mut cfg = turn_cfg(dir.path(), "gap_min_pct", 1.2, stamp(10));
    cfg.candidate = Some(flip);
    assert_eq!(turn(cfg).await.unwrap().gate_exit, Some(GateExit::FingerprintDrift));
}

#[tokio::test]
async fn happy_path_flips_and_appends_exactly_one_flip_trial() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    seed_run(dir.path(), 2.4, 0, stamp(0)).await;
    let fp = anchor_fp(dir.path());
    let flip = write_go(dir.path(), "c", "gap_min_pct", 1.2, &fp);
    let ledger = flip.ledger.clone();
    let mut cfg = turn_cfg(dir.path(), "gap_min_pct", 1.2, stamp(10));
    cfg.candidate = Some(flip);
    let out = turn(cfg).await.unwrap();
    assert!(out.ran, "GO + clean hashes flips: {:?}", out.refusal);
    assert!(out.gate_exit.is_none(), "no gate refusal on the happy path");
    // The ledger now carries the gate-reading + exactly one flip trial.
    let records = ledger.read_all().unwrap();
    assert_eq!(records.len(), 2, "gate-reading + one flip: {records:?}");
    assert_eq!(records.iter().filter(|r| matches!(r.look, LookKind::Flip)).count(), 1);
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
    let out = turn(govern(dir.path(), "c", "gap_min_pct", 1.2, stamp(10))).await.unwrap();
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

// ---------------------------------------------------------------------------
// U2 — code-turn native path (LS_TURN_CODE_BUMP + CompareMode::Code)
// ---------------------------------------------------------------------------

/// Write a manifest-only run dir (no performance/decisions) and return its id.
/// `mutate` can drop or edit params keys to model a prior head that predates a
/// newer defaulted field, or a divergent code hash.
fn write_manifest_run(data_home: &Path, run_id: &str, mut manifest: Manifest, mutate: impl FnOnce(&mut serde_json::Value)) {
    manifest.run_id = run_id.to_string();
    let mut v = serde_json::to_value(&manifest).unwrap();
    mutate(&mut v);
    let run_dir = data_home.join("runs").join(run_id);
    std::fs::create_dir_all(&run_dir).unwrap();
    std::fs::write(run_dir.join(MANIFEST_FILE), serde_json::to_string_pretty(&v).unwrap()).unwrap();
}

#[tokio::test]
async fn ae6_code_bump_turn_bumps_the_version_with_params_unchanged_no_seed_dir() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let v8 = seed_run(dir.path(), 2.4, 8, stamp(0)).await;
    let prior = read_manifest(dir.path(), &v8);

    // Native code bump: no LS_TURN_PARAM, no manual seed dir — just code_bump.
    let mut cfg = TurnConfig::new(dir.path(), stamp(10));
    cfg.code_bump = true;
    let out = turn(cfg).await.unwrap();
    assert!(out.ran, "code bump ran: {:?}", out.refusal);
    assert_eq!(out.approved, None, "a code turn runs no governance cycle");
    assert_eq!(out.version, Some(9), "version is prior + 1");

    let bumped = read_manifest(dir.path(), out.run_id.as_ref().unwrap());
    assert_eq!(bumped.strategy_version, 9);
    // Params byte-equal to the prior head modulo the version field.
    let mut a = serde_json::to_value(&prior.params).unwrap();
    let mut b = serde_json::to_value(&bumped.params).unwrap();
    a["strategy_version"] = json!(0);
    b["strategy_version"] = json!(0);
    assert_eq!(a, b, "params unchanged across the code bump");
    // Only the real run exists — no lingering seed dir.
    assert_eq!(list_runs(dir.path()).len(), 2, "just the v8 seed + the v9 run");
}

#[tokio::test]
async fn code_bump_rejects_a_combined_param_turn() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    seed_run(dir.path(), 2.4, 8, stamp(0)).await;
    let mut cfg = turn_cfg(dir.path(), "gap_min_pct", 1.2, stamp(10));
    cfg.code_bump = true;
    let err = turn(cfg).await.unwrap_err();
    assert!(err.to_string().contains("cannot be combined with LS_TURN_PARAM"), "{err}");
}

#[tokio::test]
async fn code_bump_seeds_a_newer_defaulted_param_the_prior_head_predates() {
    // Companion-field regression: a prior head whose manifest literally lacks a
    // newer #[serde(default)] param (profit_target_r) must yield a bumped manifest
    // carrying that param at its default — the native path subsumes the manual
    // companion-field seeding step.
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let v8 = seed_run(dir.path(), 2.4, 8, stamp(0)).await;
    let prior = read_manifest(dir.path(), &v8);

    // Re-seed v8 as a manifest that predates profit_target_r (drop the key).
    write_manifest_run(dir.path(), "20240601T000000Z-backtest-orb-v8", prior, |v| {
        v["params"].as_object_mut().unwrap().remove("profit_target_r");
    });

    let mut cfg = TurnConfig::new(dir.path(), stamp(20));
    cfg.code_bump = true;
    let out = turn(cfg).await.unwrap();
    assert!(out.ran, "code bump over a companion-predating head ran: {:?}", out.refusal);
    let bumped = read_manifest(dir.path(), out.run_id.as_ref().unwrap());
    assert_eq!(bumped.strategy_version, 9);
    // The bumped manifest carries the newer param at its default (1.0).
    assert_eq!(bumped.params.profit_target_r, 1.0, "companion field seeded at default");
}

#[tokio::test]
async fn code_mode_passes_a_version_only_diff_with_a_code_hash_delta() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    // Two manifest-only runs: identical params, version differs, code hash differs.
    let base = read_manifest(dir.path(), &seed_run(dir.path(), 2.4, 8, stamp(0)).await);
    write_manifest_run(dir.path(), "20240601T000000Z-backtest-orb-v8b", base.clone(), |v| {
        v["strategy_version"] = json!(8);
        v["params"]["strategy_version"] = json!(8);
        v["strategy_code_hash"] = json!("aaaa_old_code");
    });
    write_manifest_run(dir.path(), "20240601T000001Z-backtest-orb-v9", base.clone(), |v| {
        v["strategy_version"] = json!(9);
        v["params"]["strategy_version"] = json!(9);
        v["strategy_code_hash"] = json!("bbbb_new_code");
    });

    let pass = compare(&CompareConfig {
        data_home: dir.path().to_path_buf(),
        run_a: Some("20240601T000000Z-backtest-orb-v8b".into()),
        run_b: Some("20240601T000001Z-backtest-orb-v9".into()),
        mode: CompareMode::Code,
        explanation: None,
    })
    .unwrap();
    assert!(pass.pass, "version-only diff + code-hash delta PASSes code mode: {:?}", pass.lines);
    assert!(pass.lines.iter().any(|l| l.contains("strategy_code_hash delta: expected")), "{:?}", pass.lines);
}

#[tokio::test]
async fn code_mode_fails_when_a_param_also_changed_or_the_code_hash_is_equal() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    let base = read_manifest(dir.path(), &seed_run(dir.path(), 2.4, 8, stamp(0)).await);

    // (a) a param ALSO changed → FAIL (not a version-only diff).
    write_manifest_run(dir.path(), "20240601T000000Z-backtest-orb-v8c", base.clone(), |v| {
        v["strategy_version"] = json!(8);
        v["params"]["strategy_version"] = json!(8);
        v["strategy_code_hash"] = json!("aaaa");
    });
    write_manifest_run(dir.path(), "20240601T000001Z-backtest-orb-v9c", base.clone(), |v| {
        v["strategy_version"] = json!(9);
        v["params"]["strategy_version"] = json!(9);
        v["params"]["gap_min_pct"] = json!(1.2); // extra param delta
        v["strategy_code_hash"] = json!("bbbb");
    });
    let param_also = compare(&CompareConfig {
        data_home: dir.path().to_path_buf(),
        run_a: Some("20240601T000000Z-backtest-orb-v8c".into()),
        run_b: Some("20240601T000001Z-backtest-orb-v9c".into()),
        mode: CompareMode::Code,
        explanation: None,
    })
    .unwrap();
    assert!(!param_also.pass, "a param delta breaks the version-only rule: {:?}", param_also.lines);

    // (b) code hash UNCHANGED → FAIL (a code turn must move it).
    write_manifest_run(dir.path(), "20240601T000002Z-backtest-orb-v9d", base.clone(), |v| {
        v["strategy_version"] = json!(9);
        v["params"]["strategy_version"] = json!(9);
        // same strategy_code_hash as v8c ("aaaa")
        v["strategy_code_hash"] = json!("aaaa");
    });
    let no_delta = compare(&CompareConfig {
        data_home: dir.path().to_path_buf(),
        run_a: Some("20240601T000000Z-backtest-orb-v8c".into()),
        run_b: Some("20240601T000002Z-backtest-orb-v9d".into()),
        mode: CompareMode::Code,
        explanation: None,
    })
    .unwrap();
    assert!(!no_delta.pass, "an unchanged code hash fails code mode: {:?}", no_delta.lines);

    // (c) catalog fingerprint differs → FAIL even with a version-only diff.
    write_manifest_run(dir.path(), "20240601T000003Z-backtest-orb-v9e", base.clone(), |v| {
        v["strategy_version"] = json!(9);
        v["params"]["strategy_version"] = json!(9);
        v["strategy_code_hash"] = json!("bbbb");
        v["catalog_fingerprint"] = json!("drifted");
    });
    let fp_drift = compare(&CompareConfig {
        data_home: dir.path().to_path_buf(),
        run_a: Some("20240601T000000Z-backtest-orb-v8c".into()),
        run_b: Some("20240601T000003Z-backtest-orb-v9e".into()),
        mode: CompareMode::Code,
        explanation: None,
    })
    .unwrap();
    assert!(!fp_drift.pass, "a fingerprint drift fails code mode: {:?}", fp_drift.lines);
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
    let param_turn = turn(govern(dir.path(), "c", "gap_min_pct", 1.2, stamp(10))).await.unwrap();

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
    let approved = turn(govern(dir.path(), "c", "gap_min_pct", 1.2, stamp(10))).await.unwrap();
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
    let cal = build_calendar(&[], false);
    let view = cal.as_of(cal_as_of()).unwrap();
    let gate = CatalogCalendarGate::new(Some(view));
    let out = catalog_status_gated(
        &StatusConfig {
            data_home: dir.path().to_path_buf(),
            expected_range: None,
        },
        gate,
    )
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

    // Enforced: 2024-01-31 (Wed) is a proven session, so last session (01-31) > last bar
    // (01-05) → genuine tail undershoot flags.
    let cal = build_calendar(&[], false);
    let view = cal.as_of(cal_as_of()).unwrap();
    let gate = CatalogCalendarGate::new(Some(view));
    let out = catalog_status_gated(
        &StatusConfig {
            data_home: dir.path().to_path_buf(),
            expected_range: None,
        },
        gate,
    )
    .await
    .unwrap();
    assert!(!out.go, "undershoot is a no-go");
    assert!(
        out.triples.iter().any(|t| t.bar_kind == "1-DAY" && !t.flags.is_empty()),
        "the daily triple is flagged: {:?}",
        out.triples
    );
}

// (Legacy weekday walk-back tests `weekend_watermark_does_not_false_flag_a_friday_closed_catalog`
//  and `genuine_undershoot_across_a_weekend_still_flags` were retired with the catalog cutover;
//  their Enforced equivalent is `enforced_closed_watermark_boundary_does_not_false_flag`.)

#[tokio::test]
async fn front_truncation_is_flagged_only_with_an_expected_range() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    // Bars start 20240104; an expected range starting earlier reveals front truncation —
    // undetectable from the checkpoint alone. Enforced: 20240101 (Mon) is a proven session, so
    // first session (01-01) < first bar (01-04) → front truncation flags.
    let cal = build_calendar(&[], false);
    let view = cal.as_of(cal_as_of()).unwrap();
    let gate = CatalogCalendarGate::new(Some(view));
    let with_expected = catalog_status_gated(
        &StatusConfig {
            data_home: dir.path().to_path_buf(),
            expected_range: Some(DataRange { start: "20240101".into(), end: "20240105".into() }),
        },
        gate,
    )
    .await
    .unwrap();
    assert!(!with_expected.go, "front truncation is a no-go");
    assert!(with_expected
        .triples
        .iter()
        .any(|t| t.flags.iter().any(|f| f.contains("front truncation"))));

    // Without the expected range, the same catalog is a go (checkpoint watermark
    // is unset in the fixture).
    let cal2 = build_calendar(&[], false);
    let view2 = cal2.as_of(cal_as_of()).unwrap();
    let gate2 = CatalogCalendarGate::new(Some(view2));
    let without = catalog_status_gated(
        &StatusConfig {
            data_home: dir.path().to_path_buf(),
            expected_range: None,
        },
        gate2,
    )
    .await
    .unwrap();
    assert!(without.go, "no front check without an expected range: {:?}", without.lines);
}

#[tokio::test]
async fn missing_catalog_dir_is_a_clean_no_go_not_a_panic() {
    let dir = tempdir().unwrap();
    // No fixture — the catalog dir does not exist (bails before any calendar query).
    let gate = CatalogCalendarGate::new(None);
    let err = catalog_status_gated(
        &StatusConfig {
            data_home: dir.path().to_path_buf(),
            expected_range: None,
        },
        gate,
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("no catalog"), "clean error: {err}");
}

// ===========================================================================
// U11 (KTD8) — catalog watermark + expected-range migration under the Enforced calendar
// (#189). The tail/expected-range boundary checks are based on PROVEN first/last Trading
// Sessions (a real holiday closure no longer false-flags, an Unknown/unavailable boundary
// fails closed with distinct messaging); there is no weekday fallback. PROOF-FIRST: assert
// the OBSERVABLE GO/NO-GO, boundary dates, and operator messages — never the weekday helper.
//
// The 2024 catalog bars (last daily = 20240105 Friday) are covered by a small
// in-memory calendar over 2024-01-01..2024-01-31 (default: weekends Closed, weekdays
// Trading Session) with per-date overrides for the precise boundary scenario.
// ===========================================================================

/// The as-of instant every U11 calendar view is evaluated at — inside the injected
/// calendar's authorization grant and after its (fresh) freshness anchors.
fn cal_as_of() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 2, 15, 0, 0, 0).unwrap()
}

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

/// Build a small, contiguous, loadable in-memory calendar over 2024-01-01..2024-01-31.
/// Each date defaults to `Closed` on weekends and `TradingSession` on weekdays; every
/// `(date, status)` in `overrides` replaces that default. `stale` drives the freshness
/// block: fresh (all dimensions current at [`cal_as_of`]) or stale (holiday facts far in
/// the past) — staleness never rewrites a day status.
fn build_calendar(overrides: &[(NaiveDate, DayStatus)], stale: bool) -> KrxCalendar {
    let as_of = cal_as_of();
    let holiday_anchor = if stale {
        // 14-day KASI threshold: far in the past → stale at cal_as_of.
        Some(Utc.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap())
    } else {
        Some(as_of)
    };
    let freshness = Freshness {
        evidence_refreshed_at: as_of,
        holiday_facts_checked_at: holiday_anchor,
        full_history_reconciled_at: Some(as_of),
        forward_readiness_through: Some(as_of.date_naive() + Duration::days(365)),
        last_incremental_at: Some(as_of),
    };
    build_calendar_with_freshness(overrides, freshness)
}

/// A fresh-in-every-dimension `Freshness` block at [`cal_as_of`], for tests that then flip
/// exactly one dimension stale to prove dimension-relevant warnings (U11, KTD5).
fn fresh_freshness() -> Freshness {
    let as_of = cal_as_of();
    Freshness {
        evidence_refreshed_at: as_of,
        holiday_facts_checked_at: Some(as_of),
        full_history_reconciled_at: Some(as_of),
        forward_readiness_through: Some(as_of.date_naive() + Duration::days(365)),
        last_incremental_at: Some(as_of),
    }
}

/// The 14-day-plus-past instant that makes an instant-anchored dimension stale at
/// [`cal_as_of`] (well past every instant threshold: KASI 14d, incremental 2d, full 120d).
fn stale_anchor() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap()
}

/// Build the 2024-01 calendar with an explicit [`Freshness`] block — the freshness-varying
/// twin of [`build_calendar`] (staleness never rewrites a day status; U11, KTD5).
/// `retrospectively_checked_through` == `materialized_through`.
fn build_calendar_with_freshness(
    overrides: &[(NaiveDate, DayStatus)],
    freshness: Freshness,
) -> KrxCalendar {
    build_calendar_full(overrides, freshness, ymd(2024, 1, 31))
}

/// Build the 2024-01 calendar with an explicit freshness block AND an explicit
/// `retrospectively_checked_through`. When `retro_through` < `materialized_through`
/// (2024-01-31), the dates in `(retro_through, materialized_through]` form the forward/
/// unverified zone — the only zone that exercises the `forward_readiness` bounding
/// dimension in [`CatalogCalendarGate::stale_bounding_dimensions`] (U11, KTD5).
fn build_calendar_full(
    overrides: &[(NaiveDate, DayStatus)],
    freshness: Freshness,
    retro_through: NaiveDate,
) -> KrxCalendar {
    let from = ymd(2024, 1, 1);
    let through = ymd(2024, 1, 31);
    let mut rows = Vec::new();
    let mut d = from;
    while d <= through {
        let status = overrides
            .iter()
            .find(|(dd, _)| *dd == d)
            .map(|(_, s)| *s)
            .unwrap_or(if matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
                DayStatus::Closed
            } else {
                DayStatus::TradingSession
            });
        rows.push(DayRow {
            date: d,
            status,
            decisive_evidence: vec![],
            conflicting_evidence: vec![],
            alerts: vec![],
        });
        d = d.succ_opt().unwrap();
    }
    let as_of = cal_as_of();
    let authorization = Authorization {
        authorized: true,
        authority: "SYNTHETIC-MAINTAINER".to_string(),
        granted_at: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
        expires_at: Some(Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap()),
        terminated_at: None,
    };
    let mut snap = Snapshot {
        schema_version: "1.0.0".to_string(),
        artifact_id: String::new(),
        calendar_id: String::new(),
        predecessor_artifact_id: None,
        scope: CalendarScope {
            calendar_name: "KRX domestic equity regular session (SYNTHETIC)".to_string(),
            venue: "XKRX".to_string(),
            instrument_class: "domestic-equity".to_string(),
            timezone: "Asia/Seoul".to_string(),
            synthetic: true,
        },
        authorization,
        coverage: Coverage {
            materialized_from: from,
            materialized_through: through,
            retrospectively_checked_through: retro_through,
            scheduled_closure_evaluated_through: through,
            source_availability: vec![],
        },
        freshness,
        sources: vec![],
        evidence: vec![],
        alerts: vec![],
        rows,
    };
    snap.artifact_id = compute_artifact_id(&snap);
    snap.calendar_id = compute_calendar_id(&snap);
    KrxCalendar::from_snapshot(snap, as_of).expect("hand-built calendar must load")
}

/// Set the daily watermark for 005930 to `wm` (mirrors accumulate's checkpoint advance).
fn set_daily_watermark(dir: &Path, wm: NaiveDate) {
    let cp_path = dir.join("catalog").join("ingest-checkpoint.json");
    let mut cp = Checkpoint::load(&cp_path).unwrap();
    cp.set_watermark("005930.XKRX", "1-DAY", wm);
    cp.save(&cp_path).unwrap();
}

/// Closed boundary: a Monday-holiday watermark that Legacy flags as a tail undershoot does
/// NOT flag under the calendar — the PROVEN last Trading Session is the prior Friday, which
/// the catalog reaches. (This is the exact scenario `genuine_undershoot_across_a_weekend`
/// flags under the weekday walk-back.)
#[tokio::test]
async fn enforced_closed_watermark_boundary_does_not_false_flag() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    // Watermark = Monday 20240108 (a holiday). Last daily bar = Friday 20240105.
    set_daily_watermark(dir.path(), ymd(2024, 1, 8));
    let cfg = StatusConfig { data_home: dir.path().to_path_buf(), expected_range: None };

    // Enforced with 20240108 proven Closed: last session = Friday 20240105 → no undershoot.
    // (The weekday walk-back would have flagged Monday-as-weekday; retired with the cutover.)
    let cal = build_calendar(&[(ymd(2024, 1, 8), DayStatus::Closed)], false);
    let view = cal.as_of(cal_as_of()).unwrap();
    let gate = CatalogCalendarGate::new(Some(view));
    let enforced = catalog_status_gated(&cfg, gate).await.unwrap();
    assert!(enforced.go, "the calendar clears the false undershoot: {:?}", enforced.lines);
    assert!(
        enforced.triples.iter().all(|t| t.flags.is_empty()),
        "no tail flag once the real last session (Friday) is proven: {:?}",
        enforced.triples
    );
}

/// A boundary-relevant Unknown (Unknown at the watermark, before any proven session
/// scanning back) fails closed with the distinct `NO-GO — calendar indeterminate` line.
#[tokio::test]
async fn enforced_boundary_relevant_unknown_is_a_no_go_indeterminate() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    set_daily_watermark(dir.path(), ymd(2024, 1, 8));
    let cfg = StatusConfig { data_home: dir.path().to_path_buf(), expected_range: None };

    let cal = build_calendar(&[(ymd(2024, 1, 8), DayStatus::Unknown)], false);
    let view = cal.as_of(cal_as_of()).unwrap();
    let gate = CatalogCalendarGate::new(Some(view));
    let out = catalog_status_gated(&cfg, gate).await.unwrap();
    assert!(!out.go, "an Unknown boundary is a no-go: {:?}", out.lines);
    assert!(
        out.lines.iter().any(|l| l.contains("NO-GO — calendar indeterminate")),
        "the indeterminate message is present: {:?}",
        out.lines
    );
}

/// A watermark outside the materialized coverage window is unavailable — the calendar
/// cannot prove the boundary, so Enforced fails closed with `NO-GO — calendar unavailable`.
#[tokio::test]
async fn enforced_out_of_coverage_watermark_is_a_no_go_unavailable() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    // 20240201 is past the calendar's 2024-01-31 materialized_through.
    set_daily_watermark(dir.path(), ymd(2024, 2, 1));
    let cfg = StatusConfig { data_home: dir.path().to_path_buf(), expected_range: None };

    let cal = build_calendar(&[], false);
    let view = cal.as_of(cal_as_of()).unwrap();
    let gate = CatalogCalendarGate::new(Some(view));
    let out = catalog_status_gated(&cfg, gate).await.unwrap();
    assert!(!out.go, "an out-of-coverage watermark is a no-go: {:?}", out.lines);
    assert!(
        out.lines.iter().any(|l| l.contains("NO-GO — calendar unavailable")),
        "the unavailable message is present: {:?}",
        out.lines
    );

    // A missing calendar (no view injected) is equally unavailable under Enforced.
    let blind = CatalogCalendarGate::new(None);
    let out_blind = catalog_status_gated(&cfg, blind).await.unwrap();
    assert!(!out_blind.go, "no calendar → unavailable no-go: {:?}", out_blind.lines);
    assert!(out_blind.lines.iter().any(|l| l.contains("NO-GO — calendar unavailable")));
}

/// Stale-but-established: the boundary facts are proven (a GO) but the calendar's freshness
/// is stale at the as-of instant — a GO WITH a prominent warning, never a status flip.
#[tokio::test]
async fn enforced_stale_but_established_is_a_go_with_a_prominent_warning() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    set_daily_watermark(dir.path(), ymd(2024, 1, 8));
    let cfg = StatusConfig { data_home: dir.path().to_path_buf(), expected_range: None };

    // 20240108 proven Closed (established boundary → GO) but freshness is stale.
    let cal = build_calendar(&[(ymd(2024, 1, 8), DayStatus::Closed)], true);
    let view = cal.as_of(cal_as_of()).unwrap();
    let gate = CatalogCalendarGate::new(Some(view));
    let out = catalog_status_gated(&cfg, gate).await.unwrap();
    assert!(out.go, "an established boundary stays a GO even when stale: {:?}", out.lines);
    assert!(
        out.lines.iter().any(|l| l.contains("WARNING") && l.contains("STALE")),
        "a prominent stale warning is present on the GO: {:?}",
        out.lines
    );
}

// (The Shadow byte-identical + Shadow-divergence-classification tests were retired with the
//  catalog Enforced-only cutover — catalog no longer has a Legacy/Shadow path.)

/// AE3 (U3): a watermark after a multi-day holiday cluster whose last proven session
/// precedes the cluster is NOT a false tail undershoot (the proven last session is the
/// pre-cluster Friday the catalog reaches). Flipping that pre-cluster boundary to Unknown
/// flips only between GO and `NO-GO — calendar indeterminate`.
#[tokio::test]
async fn enforced_holiday_cluster_watermark_does_not_false_flag() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    // Watermark = Wed 20240110, after a 3-day holiday cluster (Mon–Wed 20240108/09/10).
    // Last daily bar = Friday 20240105 (the proven session before the cluster).
    set_daily_watermark(dir.path(), ymd(2024, 1, 10));
    let cfg = StatusConfig { data_home: dir.path().to_path_buf(), expected_range: None };

    let cluster = [
        (ymd(2024, 1, 8), DayStatus::Closed),
        (ymd(2024, 1, 9), DayStatus::Closed),
        (ymd(2024, 1, 10), DayStatus::Closed),
    ];
    let cal = build_calendar(&cluster, false);
    let view = cal.as_of(cal_as_of()).unwrap();
    let gate = CatalogCalendarGate::new(Some(view));
    let out = catalog_status_gated(&cfg, gate).await.unwrap();
    assert!(out.go, "the cluster walk-back reaches the pre-cluster Friday: {:?}", out.lines);
    assert!(
        out.triples.iter().all(|t| t.flags.is_empty()),
        "no tail flag once the pre-cluster session (Friday) is proven: {:?}",
        out.triples
    );
    // Sanity: the boundary genuinely walked back across the whole cluster.
    assert_eq!(
        gate.last_session_on_or_before(ymd(2024, 1, 10)),
        SessionBoundary::Session(ymd(2024, 1, 5)),
    );

    // Flip the pre-cluster boundary (Friday 20240105) to Unknown: the walk-back now hits an
    // Unknown before any proven session → indeterminate NO-GO (never collapses to Closed).
    let mut with_unknown = cluster.to_vec();
    with_unknown.push((ymd(2024, 1, 5), DayStatus::Unknown));
    let cal_u = build_calendar(&with_unknown, false);
    let view_u = cal_u.as_of(cal_as_of()).unwrap();
    let gate_u = CatalogCalendarGate::new(Some(view_u));
    let out_u = catalog_status_gated(&cfg, gate_u).await.unwrap();
    assert!(!out_u.go, "an Unknown pre-cluster boundary is indeterminate: {:?}", out_u.lines);
    assert!(
        out_u.lines.iter().any(|l| l.contains("NO-GO — calendar indeterminate")),
        "the indeterminate message is present: {:?}",
        out_u.lines
    );
}

/// AE4 (U3): an expected range whose civil endpoints are weekend dates is validated against
/// the interval's first/last PROVEN sessions — clearing a Legacy raw-civil-date false flag —
/// while an out-of-coverage endpoint yields `NO-GO — calendar unavailable`.
#[tokio::test]
async fn enforced_expected_range_weekend_endpoints_use_proven_sessions() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    // No watermark: isolate the expected-range check (front + tail vs the range).
    let cfg = StatusConfig {
        data_home: dir.path().to_path_buf(),
        // Sat 20240106 .. Sun 20240107 — both weekend/closed civil endpoints.
        expected_range: Some(DataRange { start: "20240106".into(), end: "20240107".into() }),
    };

    // Enforced resolves the weekend end to the proven Friday session 20240105 the catalog
    // reaches (and the weekend start to the Monday session 20240108, which the earlier
    // catalog start does not undershoot) → GO. (The retired weekday path compared raw civil
    // endpoints and false-flagged the weekend end.)
    let cal = build_calendar(&[], false);
    let view = cal.as_of(cal_as_of()).unwrap();
    let gate = CatalogCalendarGate::new(Some(view));
    let enforced = catalog_status_gated(&cfg, gate).await.unwrap();
    assert!(enforced.go, "weekend endpoints resolve to proven sessions → GO: {:?}", enforced.lines);

    // An out-of-coverage expected end (past materialized_through 20240131) is unavailable.
    let cfg_oob = StatusConfig {
        data_home: dir.path().to_path_buf(),
        expected_range: Some(DataRange { start: "20240106".into(), end: "20240215".into() }),
    };
    let cal2 = build_calendar(&[], false);
    let view2 = cal2.as_of(cal_as_of()).unwrap();
    let gate2 = CatalogCalendarGate::new(Some(view2));
    let out_oob = catalog_status_gated(&cfg_oob, gate2).await.unwrap();
    assert!(!out_oob.go, "an out-of-coverage endpoint is a no-go: {:?}", out_oob.lines);
    assert!(
        out_oob.lines.iter().any(|l| l.contains("NO-GO — calendar unavailable")),
        "the unavailable message is present: {:?}",
        out_oob.lines
    );
}

/// AE5 (U3, KTD5): a boundary fact stale in the dimension that BOUNDS it yields GO with a
/// prominent warning that NAMES that dimension.
#[tokio::test]
async fn enforced_stale_warning_names_the_bounding_dimension() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    set_daily_watermark(dir.path(), ymd(2024, 1, 8));
    let cfg = StatusConfig { data_home: dir.path().to_path_buf(), expected_range: None };

    // Watermark boundary 20240108 is retrospectively re-checked (historical zone), so it is
    // bounded by the historical dimensions. Make `incremental` stale (a bounding dimension)
    // AND `forward_readiness` stale (NOT a bounding dimension for a historical boundary) — so
    // the exclusion assertion below is meaningful, not vacuous.
    let mut freshness = fresh_freshness();
    freshness.last_incremental_at = Some(stale_anchor());
    freshness.forward_readiness_through = Some(ymd(2024, 2, 20)); // 5 days remaining < 45 → stale
    let cal = build_calendar_with_freshness(&[(ymd(2024, 1, 8), DayStatus::Closed)], freshness);
    let view = cal.as_of(cal_as_of()).unwrap();
    assert!(view.freshness().forward_readiness.is_stale(), "forward_readiness IS stale here");
    let gate = CatalogCalendarGate::new(Some(view));
    let out = catalog_status_gated(&cfg, gate).await.unwrap();
    assert!(out.go, "an established boundary stays a GO even when stale: {:?}", out.lines);
    let warning = out
        .lines
        .iter()
        .find(|l| l.contains("WARNING") && l.contains("STALE"))
        .expect("a prominent stale warning is present");
    assert!(warning.contains("incremental"), "the warning names the bounding dimension: {warning}");
    // forward_readiness is stale but does NOT bound a historical boundary → excluded.
    assert!(
        !warning.contains("forward_readiness"),
        "a stale-but-non-bounding dimension is excluded: {warning}"
    );
}

/// U3 (KTD5): a snapshot stale ONLY in a dimension that does not bound the queried boundary
/// raises NO catalog warning; staleness in the bounding dimension does warn. Proven by
/// varying `forward_readiness` (does not bound a retrospectively-checked historical boundary)
/// against `incremental` (does) independently.
#[tokio::test]
async fn enforced_unrelated_dimension_staleness_does_not_warn() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    set_daily_watermark(dir.path(), ymd(2024, 1, 8));
    let cfg = StatusConfig { data_home: dir.path().to_path_buf(), expected_range: None };

    // Stale ONLY in forward_readiness (horizon nearly exhausted at cal_as_of 2024-02-15).
    // The watermark boundary 20240108 is historical, so forward_readiness does NOT bound it.
    let mut unrelated = fresh_freshness();
    unrelated.forward_readiness_through = Some(ymd(2024, 2, 20)); // 5 days remaining < 45 → stale
    let cal = build_calendar_with_freshness(&[(ymd(2024, 1, 8), DayStatus::Closed)], unrelated);
    let view = cal.as_of(cal_as_of()).unwrap();
    assert!(view.freshness().any_stale(), "the snapshot IS stale (forward_readiness)");
    let gate = CatalogCalendarGate::new(Some(view));
    let out = catalog_status_gated(&cfg, gate).await.unwrap();
    assert!(out.go, "GO: {:?}", out.lines);
    assert!(
        !out.lines.iter().any(|l| l.contains("WARNING") && l.contains("STALE")),
        "an unrelated stale dimension raises NO catalog warning: {:?}",
        out.lines
    );

    // The bounding dimension (incremental) stale on the SAME boundary DOES warn.
    let mut bounding = fresh_freshness();
    bounding.last_incremental_at = Some(stale_anchor());
    let cal2 = build_calendar_with_freshness(&[(ymd(2024, 1, 8), DayStatus::Closed)], bounding);
    let view2 = cal2.as_of(cal_as_of()).unwrap();
    let gate2 = CatalogCalendarGate::new(Some(view2));
    let out2 = catalog_status_gated(&cfg, gate2).await.unwrap();
    assert!(
        out2.lines.iter().any(|l| l.contains("WARNING") && l.contains("incremental")),
        "the bounding dimension does warn: {:?}",
        out2.lines
    );
}

/// U3 (KTD5): the FORWARD/unverified zone exercises the `forward_readiness` bounding
/// dimension — the mirror of the historical-zone tests above (which all key on boundaries
/// at/below `retrospectively_checked_through`). With retrospectively_checked_through
/// (20240103) < materialized_through (20240131), the watermark boundary 20240105 (the
/// catalog's last daily bar, a proven Friday session → no undershoot, go stays true) sits in
/// the forward zone. A stale `forward_readiness` warns and names it; a stale HISTORICAL
/// dimension does NOT bound a forward boundary, so it raises no warning.
#[tokio::test]
async fn enforced_forward_zone_boundary_keys_on_forward_readiness() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;
    // Watermark 20240105 == the catalog's last daily bar (proven Trading Session) → no
    // undershoot; and > retrospectively_checked_through 20240103 → the forward zone.
    set_daily_watermark(dir.path(), ymd(2024, 1, 5));
    let cfg = StatusConfig { data_home: dir.path().to_path_buf(), expected_range: None };
    let retro = ymd(2024, 1, 3);

    // forward_readiness stale (bounds the forward boundary); every historical dimension fresh.
    let mut fwd = fresh_freshness();
    fwd.forward_readiness_through = Some(ymd(2024, 2, 20)); // < 45 days remaining → stale
    let cal = build_calendar_full(&[], fwd, retro);
    let view = cal.as_of(cal_as_of()).unwrap();
    assert!(view.freshness().forward_readiness.is_stale(), "forward_readiness IS stale");
    let gate = CatalogCalendarGate::new(Some(view));
    let out = catalog_status_gated(&cfg, gate).await.unwrap();
    assert!(out.go, "the forward boundary is a proven session the catalog reaches: {:?}", out.lines);
    let warning = out
        .lines
        .iter()
        .find(|l| l.contains("WARNING") && l.contains("STALE"))
        .expect("a forward-zone stale warning is present");
    assert!(warning.contains("forward_readiness"), "names the forward dimension: {warning}");
    assert!(!warning.contains("incremental"), "no historical dimension named: {warning}");

    // Mirror: a stale HISTORICAL dimension (incremental) does NOT bound a forward-zone
    // boundary → the snapshot is stale, yet the forward boundary raises no warning.
    let mut hist = fresh_freshness();
    hist.last_incremental_at = Some(stale_anchor());
    let cal2 = build_calendar_full(&[], hist, retro);
    let view2 = cal2.as_of(cal_as_of()).unwrap();
    assert!(view2.freshness().any_stale(), "the snapshot IS stale (incremental)");
    let gate2 = CatalogCalendarGate::new(Some(view2));
    let out2 = catalog_status_gated(&cfg, gate2).await.unwrap();
    assert!(out2.go, "GO: {:?}", out2.lines);
    assert!(
        !out2.lines.iter().any(|l| l.contains("WARNING") && l.contains("STALE")),
        "a stale historical dimension does not warn for a forward-zone boundary: {:?}",
        out2.lines
    );
}

/// AE1 (U4, scenario 2): the `lab-research catalog status` composition-root smoke — an
/// explicit temporary synthetic snapshot resolves once via `LS_CALENDAR_SNAPSHOT`, the
/// single startup record names the adoption and stays redacted, and the verdict is produced
/// with no production snapshot, credentials, or network.
#[tokio::test]
async fn catalog_status_composition_root_smoke() {
    let dir = tempdir().unwrap();
    build_fixture(dir.path()).await;

    // A deletable, explicitly synthetic snapshot at a temporary path (never a production one).
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../nautilus-ls-calendar/fixtures/base_2010_2012.json");
    let snap = dir.path().join("calendar.json");
    std::fs::copy(&fixture, &snap).expect("fixture copies");

    let out = bin()
        .args(["catalog", "status"])
        .env("LS_DATA_HOME", dir.path())
        .env("LS_CALENDAR_ADOPTION", "shadow")
        .env("LS_CALENDAR_SNAPSHOT", &snap)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    // The verdict is produced offline (no watermark set + no expected range → GO).
    assert!(out.status.success(), "catalog status is a GO offline: {stdout}\n{stderr}");
    assert!(stdout.contains("status: GO"), "the verdict line is present: {stdout}");

    // Exactly one startup record, naming the adoption, redacted — and only ONE load (the
    // generic `main_cli` startup emit is suppressed for `catalog status`).
    assert_eq!(
        stderr.matches("calendar-startup").count(),
        1,
        "exactly one startup record (single load): {stderr}"
    );
    // Enforced-only after the catalog cutover: LS_CALENDAR_ADOPTION=shadow is IGNORED — the
    // startup record names the enforced posture the consumer now always runs under.
    assert!(stderr.contains("adoption=enforced"), "startup names the enforced adoption: {stderr}");
    assert!(
        !stderr.contains("SYNTHETIC-MAINTAINER"),
        "the granting authority must never leak into the startup line: {stderr}"
    );
    assert!(stderr.contains("artifact_id="), "the redacted snapshot identity is reported: {stderr}");
}

/// U1 always-emit invariant (catalog side): `catalog status` emits its mandatory startup
/// record BEFORE the fallible config parse, so a config error (here: missing LS_DATA_HOME)
/// still emits exactly one record before it fails — the generic `main_cli` emit is
/// suppressed for this subcommand, so a late emit would have dropped the invariant.
#[tokio::test]
async fn catalog_status_emits_startup_record_even_on_config_error() {
    let dir = tempdir().unwrap();
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../nautilus-ls-calendar/fixtures/base_2010_2012.json");
    let snap = dir.path().join("calendar.json");
    std::fs::copy(&fixture, &snap).expect("fixture copies");

    let out = bin()
        .args(["catalog", "status"])
        // LS_DATA_HOME intentionally unset → status_config_from_env() fails.
        .env_remove("LS_DATA_HOME")
        .env("LS_CALENDAR_ADOPTION", "shadow")
        .env("LS_CALENDAR_SNAPSHOT", &snap)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "a config error exits non-zero: {stderr}");
    assert_eq!(
        stderr.matches("calendar-startup").count(),
        1,
        "the startup record still fires exactly once before the config error: {stderr}"
    );
    assert!(stderr.contains("adoption=enforced"), "the record names the enforced adoption: {stderr}");
    assert!(stderr.contains("LS_DATA_HOME"), "the config error is the failure cause: {stderr}");
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
    // The compacted fixture is still a go (Enforced, no watermark/expected range → no boundary
    // check → GO).
    let cal = build_calendar(&[], false);
    let view = cal.as_of(cal_as_of()).unwrap();
    let gate = CatalogCalendarGate::new(Some(view));
    let status = catalog_status_gated(
        &StatusConfig {
            data_home: dir.path().to_path_buf(),
            expected_range: None,
        },
        gate,
    )
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
                dropped_preferred: Vec::new(),
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
            lab_src_fingerprint: None,
            checkpoint_hash: None,
            universe_metadata_hash: Some(hash),
            dispatch: None,
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

// ===========================================================================
// `report sample` — the sample-sufficiency verdict (plan 2026-08-05-001, U2)
// ===========================================================================

mod report_sample {
    use std::path::{Path, PathBuf};

    use nautilus_ls_lab::artifacts::performance::{FillRecord, PerformanceReport, TradeRecord};
    use nautilus_ls_lab::artifacts::{RunSource, PERFORMANCE_FILE};
    use nautilus_ls_lab::margin;
    use nautilus_ls_lab::params::OrbParams;
    use nautilus_ls_lab::runner::report::{
        report_sample, RateBasis, SampleConfig, SAMPLE_CONFIDENCE, SAMPLE_POWER,
    };
    use tempfile::tempdir;

    use super::*;

    /// The v35 catalog fingerprint the committed margin is frozen against, so a
    /// fixture run does not spuriously trip the re-derivation trigger.
    pub(super) fn frozen_fingerprint() -> String {
        margin::load(&margin::frozen_margin_path())
            .unwrap()
            .values
            .provenance
            .catalog_fingerprint
    }

    /// 10:00 KST on 2026-06-`day`, as UTC unix ns.
    fn opened(day: u32) -> u64 {
        Utc.with_ymd_and_hms(2026, 6, day, 1, 0, 0).unwrap().timestamp_nanos_opt().unwrap() as u64
    }

    /// One closed trade on session `day` with per-trade R-multiple `r`,
    /// risk capital 100 and a single 5-unit commission — so its **gross** R is
    /// exactly `r + 0.05`.
    pub(super) fn trade(day: u32, r: f64) -> TradeRecord {
        TradeRecord {
            symbol: format!("00{day:04}.XKRX"),
            entry_side: "BUY".to_string(),
            quantity: 1.0,
            avg_px_open: 1_000.0,
            avg_px_close: Some(1_000.0 + r * 100.0),
            realized_pnl: r * 100.0,
            ts_opened: opened(day),
            ts_closed: Some(opened(day) + 3_600_000_000_000),
            fills: vec![FillRecord {
                ts_event: opened(day),
                side: "BUY".to_string(),
                qty: 1.0,
                price: 1_000.0,
                trade_id: format!("T-{day}-{r}"),
                commission: 5.0,
            }],
            risk_capital: Some(100.0),
            realized_r: Some(r),
        }
    }

    /// Write a finalized run carrying `trades`, and return `(data home, run id)`.
    fn write_run(trades: Vec<TradeRecord>, fingerprint: &str) -> (tempfile::TempDir, String) {
        let dir = tempdir().unwrap();
        let run_id = "20260601T000000Z-backtest-orb-v35".to_string();
        let mut params = OrbParams::default();
        params.strategy_version = 35;
        write_run_into(dir.path(), &run_id, trades, fingerprint, "uh", "ch", params);
        (dir, run_id)
    }

    /// Write a finalized run into an EXISTING data home, with every field the
    /// comparability gate reads under the caller's control.
    ///
    /// [`write_run`] hardcodes the run id and all three hashes, which is right
    /// for the single-run sample report but cannot express what `report paired`
    /// needs: two runs in two homes, several arms in one home, and a manifest
    /// that diverges from the head on exactly one of the three hashes. Both
    /// helpers share this one body, so the 22 existing `report_sample` call
    /// sites are untouched.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn write_run_into(
        data_home: &Path,
        run_id: &str,
        trades: Vec<TradeRecord>,
        fingerprint: &str,
        universe_hash: &str,
        strategy_code_hash: &str,
        params: OrbParams,
    ) {
        let run_dir = data_home.join("runs").join(run_id);
        std::fs::create_dir_all(&run_dir).unwrap();

        let strategy_version = params.strategy_version;
        let manifest = Manifest {
            run_id: run_id.to_string(),
            source: RunSource::Backtest,
            strategy_id: "orb".to_string(),
            strategy_version,
            params,
            data_range: DataRange { start: "20260601".to_string(), end: "20260630".to_string() },
            catalog_fingerprint: fingerprint.to_string(),
            universe_hash: universe_hash.to_string(),
            strategy_code_hash: strategy_code_hash.to_string(),
            lab_src_fingerprint: None,
            checkpoint_hash: None,
            universe_metadata_hash: None,
            dispatch: None,
            created_utc: "2026-06-01T00:00:00+00:00".to_string(),
        };
        std::fs::write(run_dir.join(MANIFEST_FILE), serde_json::to_string(&manifest).unwrap())
            .unwrap();
        let perf = PerformanceReport {
            trades,
            equity_curve: Vec::new(),
            summary: BTreeMap::new(),
        };
        std::fs::write(run_dir.join(PERFORMANCE_FILE), serde_json::to_string(&perf).unwrap())
            .unwrap();
    }

    fn cfg(data_home: &Path, run_id: Option<&str>) -> SampleConfig {
        SampleConfig {
            data_home: data_home.to_path_buf(),
            run_id: run_id.map(str::to_string),
            margin_path: None,
            // A small replicate count keeps the CLI suite fast; the seed is
            // fixed either way, so the figures are reproducible.
            replicates: 2_000,
            seed: 20_260_805,
        }
    }

    // --- The fixture, and every figure it implies, computed by hand ---------
    //
    // Four KST sessions of two trades each (n = 8, k = 4, every cluster size 2):
    //
    //   session 1: +1.00, +0.80   mean +0.900
    //   session 2: -0.60, -0.40   mean -0.500
    //   session 3: +0.30, +0.10   mean +0.200
    //   session 4: -0.20,  0.00   mean -0.100
    //
    //   grand mean = 1.0 / 8                                   = 0.125
    //   Kish m0    = (8 - Σm²/8) / (k-1) = (8 - 16/8) / 3       = 2.0
    //   MSB        = 2·[0.775² + 0.625² + 0.075² + 0.225²] / 3  = 2.095 / 3
    //   MSW        = (4 × 2 × 0.1²) / (8 - 4)                   = 0.02
    //   Σ(r - mean)²                                            = 2.175
    const FIXTURE_MSB: f64 = 2.095 / 3.0;
    const FIXTURE_MSW: f64 = 0.02;
    const FIXTURE_KISH: f64 = 2.0;
    const FIXTURE_SS: f64 = 2.175;
    /// Φ⁻¹(0.975) + Φ⁻¹(0.80) — the published quantiles at the pinned levels.
    const Z_SUM: f64 = 1.959_963_984_540_054 + 0.841_621_233_572_914_4;

    fn fixture_trades() -> Vec<TradeRecord> {
        vec![
            trade(1, 1.0),
            trade(1, 0.8),
            trade(2, -0.6),
            trade(2, -0.4),
            trade(3, 0.3),
            trade(3, 0.1),
            trade(4, -0.2),
            trade(4, 0.0),
        ]
    }

    fn fixture_icc() -> f64 {
        (FIXTURE_MSB - FIXTURE_MSW) / (FIXTURE_MSB + (FIXTURE_KISH - 1.0) * FIXTURE_MSW)
    }

    fn fixture_deff() -> f64 {
        1.0 + (FIXTURE_KISH - 1.0) * fixture_icc()
    }

    fn fixture_sd() -> f64 {
        (FIXTURE_SS / 7.0).sqrt()
    }

    #[tokio::test]
    async fn a_known_trade_set_produces_the_hand_computed_derivation() {
        let (dir, run_id) = write_run(fixture_trades(), &frozen_fingerprint());
        let out = report_sample(&cfg(dir.path(), Some(&run_id))).await.unwrap();

        assert_eq!(out.closed_trades, 8, "eight closed trades");
        assert_eq!(out.clustering.clusters, 4, "four KST sessions");
        assert!(
            (out.clustering.kish_cluster_size - FIXTURE_KISH).abs() < 1e-12,
            "Kish cluster size: {}",
            out.clustering.kish_cluster_size
        );
        assert!(
            (out.clustering.icc - fixture_icc()).abs() < 1e-12,
            "ICC: got {}, want {}",
            out.clustering.icc,
            fixture_icc()
        );
        assert!(
            (out.clustering.design_effect - fixture_deff()).abs() < 1e-12,
            "design effect: got {}, want {}",
            out.clustering.design_effect,
            fixture_deff()
        );
        assert!((out.net_r_sd - fixture_sd()).abs() < 1e-12, "net r sd: {}", out.net_r_sd);
        // The target effect is the measured GROSS per-trade edge: every fixture
        // trade's gross R is its net R plus 0.05, so the gross mean is +0.175.
        assert!((out.target_effect - 0.175).abs() < 1e-12, "target effect: {}", out.target_effect);

        let want_mde = Z_SUM * fixture_sd() / (8.0 / fixture_deff()).sqrt();
        assert!(
            (out.minimum_detectable_edge - want_mde).abs() < 1e-12,
            "MDE: got {}, want {want_mde}",
            out.minimum_detectable_edge
        );
        let want_required = (Z_SUM * fixture_sd() / 0.175).powi(2) * fixture_deff();
        assert!(
            (out.required_trades - want_required).abs() < 1e-9,
            "required trades: got {}, want {want_required}",
            out.required_trades
        );
        // …and the derived figures reach the printed lines, not just the struct.
        let joined = out.lines.join("\n");
        assert!(
            joined.contains(&format!("design effect {:.4}", fixture_deff())),
            "the design effect is printed: {joined}"
        );
        assert!(
            joined.contains(&format!("required closed trades: {:.0}", want_required.ceil())),
            "the required trade count is printed: {joined}"
        );
        assert!(
            joined.contains(&format!("{:.0}% confidence / {:.0}% power", SAMPLE_CONFIDENCE * 100.0, SAMPLE_POWER * 100.0)),
            "the pinned confidence and power are printed: {joined}"
        );
    }

    #[tokio::test]
    async fn the_verdict_reads_insufficient_below_the_requirement_and_sufficient_above_it() {
        let (dir, run_id) = write_run(fixture_trades(), &frozen_fingerprint());
        let thin = report_sample(&cfg(dir.path(), Some(&run_id))).await.unwrap();
        assert!(!thin.sufficient, "8 trades against ~{:.0} required", thin.required_trades);
        let joined = thin.lines.join("\n");
        assert!(joined.contains("sample INSUFFICIENT"), "{joined}");
        assert!(!joined.contains("sample SUFFICIENT"), "{joined}");

        // A head whose edge is enormous relative to its dispersion needs only a
        // handful of trades, so the same code path reports the other verdict.
        let big = vec![
            trade(1, 1.00),
            trade(1, 1.02),
            trade(2, 0.98),
            trade(2, 1.00),
            trade(3, 1.01),
            trade(3, 0.99),
            trade(4, 1.00),
            trade(4, 1.00),
        ];
        let (dir2, run2) = write_run(big, &frozen_fingerprint());
        let fat = report_sample(&cfg(dir2.path(), Some(&run2))).await.unwrap();
        assert!(
            fat.sufficient,
            "8 trades against {:.4} required at a +1.05 R target",
            fat.required_trades
        );
        let joined = fat.lines.join("\n");
        assert!(joined.contains("sample SUFFICIENT"), "{joined}");
    }

    #[tokio::test]
    async fn the_margin_line_refuses_a_head_whose_evidence_does_not_exceed_the_threshold() {
        let (dir, run_id) = write_run(fixture_trades(), &frozen_fingerprint());
        let out = report_sample(&cfg(dir.path(), Some(&run_id))).await.unwrap();
        assert!(!out.margin.verdict.clears, "the fixture head does not clear the frozen margin");
        assert!(
            out.margin.verdict.statistic <= out.margin.threshold,
            "evidence {} vs threshold {}",
            out.margin.verdict.statistic,
            out.margin.threshold
        );
        let joined = out.lines.join("\n");
        assert!(joined.contains("MARGIN VERDICT: REFUSED"), "{joined}");
        assert!(joined.contains("E[max | N="), "the threshold's inputs are printed: {joined}");
        assert!(
            !out.margin.requires_rederivation
                && joined.contains("catalog fingerprint matches the frozen one"),
            "a run on the frozen catalog binds as recorded: {joined}"
        );
    }

    #[tokio::test]
    async fn a_run_on_a_different_catalog_triggers_re_derivation_rather_than_binding_silently() {
        let (dir, run_id) = write_run(fixture_trades(), "a-different-catalog-fingerprint");
        let out = report_sample(&cfg(dir.path(), Some(&run_id))).await.unwrap();
        assert!(out.margin.requires_rederivation, "a moved fingerprint invalidates the margin");
        assert!(
            out.lines.join("\n").contains("RE-DERIVATION REQUIRED"),
            "{:?}",
            out.lines
        );
    }

    #[tokio::test]
    async fn the_staging_guard_holds_no_profitability_number_reaches_the_verdict() {
        // A power question must not be decided by a profitability number: the
        // run's P&L builds the distribution, but none of it is printed.
        let (dir, run_id) = write_run(fixture_trades(), &frozen_fingerprint());
        let out = report_sample(&cfg(dir.path(), Some(&run_id))).await.unwrap();
        let joined = out.lines.join("\n").to_lowercase();
        for banned in ["expectancy", "pnl", "p&l"] {
            assert!(!joined.contains(banned), "staging guard: {banned:?} reached the output");
        }
    }

    #[tokio::test]
    async fn the_header_names_the_catalog_fingerprint_and_the_resolved_run() {
        let fp = frozen_fingerprint();
        let (dir, run_id) = write_run(fixture_trades(), &fp);
        let out = report_sample(&cfg(dir.path(), Some(&run_id))).await.unwrap();
        assert!(!out.defaulted_run, "the run was named explicitly");
        let header = &out.lines[0];
        assert!(header.contains(&run_id), "header names the run: {header}");
        assert!(header.contains(&fp), "header names the catalog fingerprint: {header}");
        assert!(
            out.lines[1].contains("LS_REPORT_RUN") && !out.lines[1].contains("DEFAULTED"),
            "{:?}",
            out.lines[1]
        );
    }

    #[tokio::test]
    async fn an_unset_run_var_defaults_to_the_latest_finalized_run_and_says_so() {
        let (dir, run_id) = write_run(fixture_trades(), &frozen_fingerprint());
        let out = report_sample(&cfg(dir.path(), None)).await.unwrap();
        assert!(out.defaulted_run, "no run id supplied");
        assert_eq!(out.run_id, run_id, "resolved to the only finalized run");
        assert!(
            out.lines[1].contains("DEFAULTED to the latest finalized run"),
            "the header marks it: {:?}",
            out.lines[1]
        );
    }

    #[tokio::test]
    async fn zero_closed_trades_is_an_explicit_refusal_naming_what_was_missing() {
        let mut open_only = trade(1, 0.5);
        open_only.ts_closed = None;
        open_only.realized_r = None;
        let (dir, run_id) = write_run(vec![open_only], &frozen_fingerprint());
        let err = report_sample(&cfg(dir.path(), Some(&run_id))).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no CLOSED trades"), "{msg}");
        assert!(msg.contains("1 trade record"), "names what was there: {msg}");
        assert!(msg.contains("not a sample of size zero"), "not a silent zero: {msg}");
    }

    #[tokio::test]
    async fn a_pre_field_vintage_refuses_by_naming_the_missing_field_and_the_vintage() {
        // A legacy artifact predating the entry-risk join carries closed trades
        // with null risk_capital / realized_r. Without the guard those fall
        // through to an empty-series error that names nothing actionable.
        let mut trades = fixture_trades();
        trades[2].risk_capital = None;
        trades[2].realized_r = None;
        let (dir, run_id) = write_run(trades, &frozen_fingerprint());
        let err = report_sample(&cfg(dir.path(), Some(&run_id))).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("risk_capital"), "names the field: {msg}");
        assert!(msg.contains("realized_r"), "names the field: {msg}");
        assert!(msg.contains("PRE-FIELD vintage"), "names the vintage: {msg}");
        assert!(!msg.contains("empty series"), "not the downstream empty-series error: {msg}");
    }

    #[tokio::test]
    async fn a_single_session_sample_refuses_rather_than_reporting_a_design_effect() {
        let (dir, run_id) = write_run(vec![trade(1, 0.5), trade(1, -0.5)], &frozen_fingerprint());
        let err = report_sample(&cfg(dir.path(), Some(&run_id))).await.unwrap_err();
        assert!(err.to_string().contains("≥2 KST sessions"), "{err}");
    }

    // =======================================================================
    // U5 — catalog supply probe and the acquisition verdict (R4, R5, R9)
    // =======================================================================

    /// Write `sessions` consecutive weekday daily bars for one symbol into the
    /// run's catalog, starting at `start` and refusing to run past `end`, so the
    /// supply probe has real coverage to read *inside the run's own data range*
    /// — which is what the trades-per-calendar-session rate is denominated over.
    async fn write_coverage_over(data_home: &Path, start: &str, end: &str, sessions: usize) {
        let catalog = data_home.join("catalog");
        let id = InstrumentId::from("005930.XKRX");
        let bt = BarKind::Daily.bar_type(id).unwrap();
        let last = NaiveDate::parse_from_str(end, "%Y%m%d").unwrap();
        let mut day = NaiveDate::parse_from_str(start, "%Y%m%d").unwrap();
        let mut bars: Vec<Bar> = Vec::with_capacity(sessions);
        while bars.len() < sessions {
            assert!(day <= last, "the requested {sessions} sessions do not fit in {start}..{end}");
            if !matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
                let row = daily_json(
                    &day.format("%Y%m%d").to_string(),
                    "59000",
                    "60500",
                    "58500",
                    "60000",
                    "1000000",
                );
                bars.push(
                    build_daily_bar(bt, &serde_json::from_value(row).unwrap()).unwrap().unwrap(),
                );
            }
            day = day.succ_opt().unwrap();
        }
        write_bars(&catalog, bars).await.unwrap();
    }

    /// `write_coverage_over` across the fixture run's own data range.
    async fn write_coverage(data_home: &Path, sessions: usize) {
        write_coverage_over(data_home, "20260601", "20260630", sessions).await;
    }

    #[tokio::test]
    async fn required_sessions_is_required_trades_over_the_observed_rate_at_every_band_target() {
        let (dir, run_id) = write_run(fixture_trades(), &frozen_fingerprint());
        let out = report_sample(&cfg(dir.path(), Some(&run_id))).await.unwrap();

        // Eight closed trades over four sessions.
        assert!(
            (out.supply.trades_per_session - 2.0).abs() < 1e-12,
            "observed rate: {}",
            out.supply.trades_per_session
        );
        assert!(
            (out.supply.required_sessions - (out.required_trades / 2.0).ceil()).abs() < 1e-12,
            "required sessions is required trades / rate, rounded up"
        );
        assert!(out.band.len() >= 3, "the band spans the interval: {:?}", out.band);
        for row in &out.band {
            if row.required_trades.is_finite() {
                assert!(
                    (row.required_sessions - (row.required_trades / 2.0).ceil()).abs() < 1e-12,
                    "row {} maps trades to sessions the same way",
                    row.label
                );
            }
        }
        // The band brackets the point estimate: a larger target needs fewer
        // trades, a smaller one more.
        let point = out
            .band
            .iter()
            .find(|r| (r.target_effect - out.target_effect).abs() < 1e-12)
            .expect("the pinned target is a band row");
        let upper = out.band.first().expect("non-empty band");
        assert!(upper.target_effect > point.target_effect, "the band reaches above the point");
        assert!(
            upper.required_trades < point.required_trades,
            "a larger target needs fewer trades: {} vs {}",
            upper.required_trades,
            point.required_trades
        );
        assert!(
            out.band.iter().any(|r| !r.required_trades.is_finite()),
            "the interval's non-positive end is reported, not silently dropped"
        );
    }

    #[tokio::test]
    async fn the_output_names_max_concurrent_and_the_per_session_ceiling_it_implies() {
        let (dir, run_id) = write_run(fixture_trades(), &frozen_fingerprint());
        let out = report_sample(&cfg(dir.path(), Some(&run_id))).await.unwrap();
        let want = OrbParams::default().max_concurrent as f64;
        assert!((out.supply.max_concurrent - want).abs() < 1e-12);
        let joined = out.lines.join("\n");
        assert!(
            joined.contains(&format!(
                "max_concurrent {want:.0} caps the per-session trade count at {want:.0}"
            )),
            "the ceiling is stated: {joined}"
        );
        assert!(
            joined.contains("regardless of universe width"),
            "and stated as a hard cap on what breadth can convert: {joined}"
        );
    }

    #[tokio::test]
    async fn the_session_rate_is_denominated_in_calendar_sessions_not_trade_producing_ones() {
        // The unit that converts required TRADES into required SESSIONS must
        // match the unit of the coverage it is compared against. The fixture
        // trades on 4 sessions but the catalog covers 10 in the run's range, so
        // a trade-producing denominator would understate the requirement by
        // 2.5x and could turn an unreachable target into a reachable one.
        let (dir, run_id) = write_run(fixture_trades(), &frozen_fingerprint());
        write_coverage_over(dir.path(), "20260601", "20260630", 10).await;
        let out = report_sample(&cfg(dir.path(), Some(&run_id))).await.unwrap();

        assert_eq!(out.supply.in_range_sessions, Some(10), "10 calendar sessions in range");
        assert!(
            matches!(out.supply.rate_basis, RateBasis::CalendarSessions { in_range_sessions: 10 }),
            "{:?}",
            out.supply.rate_basis
        );
        assert!(
            (out.supply.trades_per_session - 0.8).abs() < 1e-12,
            "8 closed trades over 10 calendar sessions: {}",
            out.supply.trades_per_session
        );
        assert!(
            (out.supply.trades_per_trade_session - 2.0).abs() < 1e-12,
            "the trade-producing rate is still reported, at 8 over 4: {}",
            out.supply.trades_per_trade_session
        );
        assert!(
            out.supply.trades_per_session < out.supply.trades_per_trade_session,
            "the calendar rate is the lower, honest one"
        );
        assert!(
            (out.supply.required_sessions - (out.required_trades / 0.8).ceil()).abs() < 1e-12,
            "required sessions divides by the CALENDAR rate"
        );
        let joined = out.lines.join("\n");
        assert!(joined.contains("closed trades per CALENDAR session"), "{joined}");
        assert!(
            joined.contains("is NOT what a session requirement may be divided by"),
            "the trap is named for the next reader: {joined}"
        );
    }

    #[tokio::test]
    async fn an_unreadable_coverage_falls_back_to_the_trade_rate_and_says_it_is_a_lower_bound() {
        // No catalog at all: there is nothing to denominate calendar sessions
        // with, so the optimistic trade-producing rate is used -- and labelled,
        // because every session count it produces is a lower bound.
        let (dir, run_id) = write_run(fixture_trades(), &frozen_fingerprint());
        let out = report_sample(&cfg(dir.path(), Some(&run_id))).await.unwrap();
        assert_eq!(out.supply.in_range_sessions, None);
        assert!(matches!(out.supply.rate_basis, RateBasis::TradeProducingSessionsFallback));
        assert!((out.supply.trades_per_session - 2.0).abs() < 1e-12);
        let joined = out.lines.join("\n");
        assert!(joined.contains("OPTIMISTIC fallback"), "{joined}");
        assert!(joined.contains("LOWER BOUND on the true requirement"), "{joined}");
    }

    #[tokio::test]
    async fn a_non_positive_gross_edge_is_a_verdict_not_a_crash() {
        // The target effect is the MEASURED gross per-trade edge. A head whose
        // gross edge is negative has no detectable target at any sample size --
        // which is a verdict the report must print, not an error that aborts it
        // before the operator sees anything.
        let losers = vec![
            trade(1, -0.40),
            trade(1, -0.20),
            trade(2, -0.30),
            trade(2, -0.10),
            trade(3, -0.25),
            trade(3, -0.15),
            trade(4, -0.35),
            trade(4, -0.05),
        ];
        let (dir, run_id) = write_run(losers, &frozen_fingerprint());
        let out = report_sample(&cfg(dir.path(), Some(&run_id))).await.unwrap();
        assert!(out.target_effect < 0.0, "gross edge is negative: {}", out.target_effect);
        assert!(!out.required_trades.is_finite(), "no sample size resolves it");
        assert!(!out.sufficient);
        let joined = out.lines.join("\n");
        assert!(joined.contains("required closed trades: UNDETECTABLE"), "{joined}");
        assert!(
            joined.contains("more data cannot fix it"),
            "and it says why more data is not the answer: {joined}"
        );
        // The rest of the report still renders -- the margin verdict included.
        assert!(joined.contains("MARGIN VERDICT:"), "{joined}");
        assert!(joined.contains("ACQUISITION VERDICT:"), "{joined}");
    }

    #[tokio::test]
    async fn the_verdict_line_names_the_target_effect_it_was_computed_at() {
        let (dir, run_id) = write_run(fixture_trades(), &frozen_fingerprint());
        let out = report_sample(&cfg(dir.path(), Some(&run_id))).await.unwrap();
        let verdict = out
            .lines
            .iter()
            .find(|l| l.starts_with("ACQUISITION VERDICT"))
            .expect("an acquisition verdict line");
        assert!(
            verdict.contains(&format!("{:+.6} R", out.target_effect)),
            "the verdict names its target: {verdict}"
        );
    }

    #[tokio::test]
    async fn a_shortfall_stands_down_and_names_it() {
        // Four sessions of coverage against a head needing thousands.
        let (dir, run_id) = write_run(fixture_trades(), &frozen_fingerprint());
        write_coverage(dir.path(), 4).await;
        let out = report_sample(&cfg(dir.path(), Some(&run_id))).await.unwrap();
        assert_eq!(out.supply.available_sessions, Some(4));
        assert_eq!(out.supply.in_range_sessions, Some(4), "all four sit in the run's range");
        assert!(!out.supply.reachable);
        assert!(
            (out.supply.shortfall_sessions - (out.supply.required_sessions - 4.0)).abs() < 1e-12,
            "shortfall is required minus covered"
        );
        let joined = out.lines.join("\n");
        assert!(joined.contains("ACQUISITION VERDICT: STAND DOWN"), "{joined}");
        assert!(
            joined.contains(&format!("SHORTFALL {:.0} sessions", out.supply.shortfall_sessions)),
            "the shortfall is named: {joined}"
        );
        assert!(joined.contains("executes NO acquisition and NO ingest"), "{joined}");
    }

    #[tokio::test]
    async fn a_reachable_requirement_names_the_range_as_a_fresh_catalog_build_this_turn_wont_run() {
        // The large-edge head needs a single session; twenty are covered.
        let big = vec![
            trade(1, 1.00),
            trade(1, 1.02),
            trade(2, 0.98),
            trade(2, 1.00),
            trade(3, 1.01),
            trade(3, 0.99),
            trade(4, 1.00),
            trade(4, 1.00),
        ];
        let (dir, run_id) = write_run(big, &frozen_fingerprint());
        write_coverage(dir.path(), 20).await;
        let out = report_sample(&cfg(dir.path(), Some(&run_id))).await.unwrap();
        assert_eq!(out.supply.available_sessions, Some(20));
        assert!(out.supply.reachable, "required {:.0} sessions", out.supply.required_sessions);
        let joined = out.lines.join("\n");
        assert!(joined.contains("ACQUISITION VERDICT: REACHABLE"), "{joined}");
        assert!(
            joined.contains(&format!("{:.0} KST sessions", out.supply.required_sessions)),
            "the recommended range is named: {joined}"
        );
        assert!(joined.contains("FRESH CATALOG at a wider lookback"), "{joined}");
        assert!(
            joined.contains("`accumulate` never fetches below the watermark"),
            "and why it cannot be incremental: {joined}"
        );
        assert!(joined.contains("executes NO acquisition and NO ingest"), "{joined}");
    }

    #[tokio::test]
    async fn an_unreadable_catalog_leaves_supply_unestablished_and_fails_closed() {
        // No catalog at all: supply is not assumed to be zero OR infinite — it
        // is UNESTABLISHED, which fails closed to a stand-down.
        let (dir, run_id) = write_run(fixture_trades(), &frozen_fingerprint());
        let out = report_sample(&cfg(dir.path(), Some(&run_id))).await.unwrap();
        assert_eq!(out.supply.available_sessions, None);
        assert!(!out.supply.reachable, "unestablished supply is not a reachable one");
        let joined = out.lines.join("\n");
        assert!(joined.contains("supply UNESTABLISHED"), "{joined}");
        assert!(joined.contains("ACQUISITION VERDICT: STAND DOWN"), "{joined}");
    }

    #[tokio::test]
    async fn the_recommendation_names_history_over_breadth_with_the_effective_n_reason() {
        let (dir, run_id) = write_run(fixture_trades(), &frozen_fingerprint());
        let out = report_sample(&cfg(dir.path(), Some(&run_id))).await.unwrap();
        let joined = out.lines.join("\n");
        assert!(joined.contains("Lengthen HISTORY, not breadth"), "{joined}");
        assert!(
            joined.contains("raise effective n roughly in proportion"),
            "the effective-n reason is stated, not asserted: {joined}"
        );
        assert!(
            joined.contains("adds trades inside blocks already held"),
            "and why breadth does not: {joined}"
        );
    }

    /// No branch of the report reaches an acquisition path. Asserted on the
    /// source rather than the output: an ingest entry point could be called
    /// without printing anything, so the output is not evidence about it.
    #[test]
    fn no_code_path_in_the_sample_report_reaches_an_ingest_entry_point() {
        let src = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("runner").join("report.rs"),
        )
        .unwrap();
        let body = src
            .split_once("pub async fn report_sample(")
            .expect("report_sample exists")
            .1
            .split_once("\n#[cfg(test)]")
            .expect("the test module terminates the non-test source")
            .0;
        // Call-shaped needles, so the prose explaining WHY the acquisition
        // cannot be incremental does not trip its own guard.
        for banned in [
            "accumulate(",
            "run_accumulate",
            "write_bars(",
            "write_instruments(",
            "delete_bar_series(",
            "heal(",
            "fetch(",
        ] {
            assert!(
                !body.contains(banned),
                "report_sample must not reach {banned:?} — the turn stops at the verdict (KTD7)"
            );
        }
        // The one catalog call it DOES make is a read.
        assert!(body.contains("read_all_bars"), "the supply probe reads coverage");
    }

    /// Dispatch: `report sample` is reachable through the compiled bin, and the
    /// unknown-mode bail enumerates it among the valid report modes.
    #[test]
    fn report_sample_is_enumerated_by_the_compiled_bins_unknown_mode_bail() {
        let out = bin().args(["report", "bogus-sample"]).output().unwrap();
        assert!(!out.status.success());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("report sample"), "{stderr}");
        assert!(stderr.contains("report mfe") && stderr.contains("report tiers"), "{stderr}");
        let _ = PathBuf::new();
    }
}

// ===========================================================================
// `report paired` — the paired-power measurement (plan 2026-08-07-001, U4)
// ===========================================================================

mod report_paired {
    use std::path::Path;

    use nautilus_ls_lab::artifacts::performance::TradeRecord;
    use nautilus_ls_lab::params::OrbParams;
    use nautilus_ls_lab::runner::report::{
        report_paired, PairedConfig, PairedOutcome, PAIRED_HEAD_CALENDAR_SESSIONS,
        PAIRED_HEAD_CALENDAR_SESSIONS_RUN, PAIRED_REACHABLE_CALENDAR_SESSIONS, SAMPLE_CONFIDENCE,
        SAMPLE_POWER,
    };
    use nautilus_ls_lab::stats::{power_z, two_sided_z};
    use tempfile::tempdir;

    use super::report_sample::{frozen_fingerprint, trade, write_run_into};
    use super::*;

    const HEAD_RUN: &str = "20260601T000000Z-backtest-orb-v35";
    const ARM_RUN: &str = "20260601T000100Z-backtest-orb-v92";

    fn params(version: u32) -> OrbParams {
        let mut p = OrbParams::default();
        p.strategy_version = version;
        p
    }

    /// A config over `head_home`/`arm_home` with the CLI's own seed and a small
    /// replicate count.
    fn cfg(head_home: &Path, arm_home: &Path, arms: &[&str]) -> PairedConfig {
        PairedConfig {
            head_home: head_home.to_path_buf(),
            head_run: HEAD_RUN.to_string(),
            arm_home: arm_home.to_path_buf(),
            arm_runs: arms.iter().map(|s| (*s).to_string()).collect(),
            margin_path: None,
            replicates: 2_000,
            seed: 20_260_805,
        }
    }

    /// `trade(day, r)` carries `realized_pnl = r × 100` on `risk_capital = 100`,
    /// so a run's net RoR — `Σ realized_pnl / Σ risk_capital` — is exactly the
    /// MEAN of its `r` values. Every figure below is hand-computed from that.
    ///
    /// ```text
    /// head  session 1: +0.4, −0.2     session 2: +0.6, +0.4
    ///       net RoR = 1.2 / 4 = +0.3
    /// arm   session 1: +0.1, +0.1     session 2: +0.3, −0.1     session 3: +0.5, +0.3
    ///       net RoR over the UNION        = 1.2 / 6 = +0.2   → delta +0.1
    ///       net RoR over the INTERSECTION = 0.4 / 4 = +0.1   → delta +0.2
    /// ```
    ///
    /// The two disagree by construction: session 3 is the arm's own, and its
    /// trades are chosen so dropping it MOVES the arm's ratio. An
    /// intersection-only build therefore fails AE5 rather than passing it by
    /// coincidence — which is the whole reason KTD4 pins the union.
    fn head_trades() -> Vec<TradeRecord> {
        vec![trade(1, 0.4), trade(1, -0.2), trade(2, 0.6), trade(2, 0.4)]
    }
    fn arm_trades() -> Vec<TradeRecord> {
        vec![
            trade(1, 0.1),
            trade(1, 0.1),
            trade(2, 0.3),
            trade(2, -0.1),
            trade(3, 0.5),
            trade(3, 0.3),
        ]
    }
    const HEAD_NET_ROR: f64 = 0.3;
    const ARM_NET_ROR: f64 = 0.2;
    /// What the arm's ratio would be if the blocks were built over the
    /// intersection instead of the union. Named so the assertion below can say
    /// explicitly which quantity it is refusing.
    const ARM_NET_ROR_INTERSECTION_ONLY: f64 = 0.1;

    fn two_homes() -> (tempfile::TempDir, tempfile::TempDir) {
        let head = tempdir().unwrap();
        let arm = tempdir().unwrap();
        let fp = frozen_fingerprint();
        write_run_into(head.path(), HEAD_RUN, head_trades(), &fp, "uh", "ch", params(35));
        write_run_into(arm.path(), ARM_RUN, arm_trades(), &fp, "uh", "ch", params(92));
        (head, arm)
    }

    /// **AE5.** The point estimate is the head's whole-run net RoR minus the
    /// arm's — the quantity TURN-LOG recorded — and not a differently-scoped one.
    ///
    /// This is the assertion the union-block choice (KTD4) exists to satisfy,
    /// and it is written to FAIL under an intersection build rather than to pass
    /// under both: see the fixture's doc comment for why the two disagree here.
    #[test]
    fn ae5_the_point_estimate_is_the_recorded_whole_run_delta() {
        let (head, arm) = two_homes();
        let out = report_paired(&cfg(head.path(), arm.path(), &[ARM_RUN])).unwrap();
        assert_eq!(out.arms.len(), 1);
        let a = &out.arms[0];
        assert!(
            (a.head_net_ror - HEAD_NET_ROR).abs() < 1e-12,
            "head net RoR {} != {HEAD_NET_ROR}",
            a.head_net_ror
        );
        assert!(
            (a.arm_net_ror - ARM_NET_ROR).abs() < 1e-12,
            "arm net RoR {} != {ARM_NET_ROR} — the arm's ratio is taken over the UNION",
            a.arm_net_ror
        );
        assert!(
            (a.arm_net_ror - ARM_NET_ROR_INTERSECTION_ONLY).abs() > 1e-9,
            "the fixture must discriminate: the union and intersection ratios coincide, so this \
             assertion proves nothing"
        );
        assert!(
            (a.bootstrap.point - (HEAD_NET_ROR - ARM_NET_ROR)).abs() < 1e-12,
            "point {} != head − arm = {}",
            a.bootstrap.point,
            HEAD_NET_ROR - ARM_NET_ROR
        );
        assert_eq!(a.union_blocks, 3, "the union counts the session only the arm traded");
        assert_eq!(a.intersection_blocks, 2, "the head traded two of the three");
    }

    #[test]
    fn an_arm_sharing_no_session_with_the_head_is_still_measurable_over_the_union() {
        // Disjoint sessions: the union is every session either traded, the
        // intersection is empty, and the head contributes nothing at all to the
        // sessions it did not trade. The point estimate is still the whole-run
        // delta, because that is a property of the union and not of any overlap.
        let head = tempdir().unwrap();
        let arm = tempdir().unwrap();
        let fp = frozen_fingerprint();
        write_run_into(head.path(), HEAD_RUN, head_trades(), &fp, "uh", "ch", params(35));
        write_run_into(
            arm.path(),
            ARM_RUN,
            // sessions 7 and 8 — the head trades neither. mean r = 0.4/4 = +0.1
            vec![trade(7, 0.3), trade(7, -0.1), trade(8, 0.1), trade(8, 0.1)],
            &fp,
            "uh",
            "ch",
            params(92),
        );
        let out = report_paired(&cfg(head.path(), arm.path(), &[ARM_RUN])).unwrap();
        let a = &out.arms[0];
        assert_eq!(a.union_blocks, 4, "two head sessions + two arm sessions");
        assert_eq!(a.intersection_blocks, 0, "they share none");
        assert_eq!(a.head_only_blocks, 2);
        assert_eq!(a.arm_only_blocks, 2);
        assert!((a.head_net_ror - HEAD_NET_ROR).abs() < 1e-12);
        assert!((a.arm_net_ror - 0.1).abs() < 1e-12);
        assert!(
            (a.bootstrap.point - (HEAD_NET_ROR - 0.1)).abs() < 1e-12,
            "still the whole-run delta: {}",
            a.bootstrap.point
        );
        // With no shared session there is no common shock to cancel, so the
        // shared component is UNAVAILABLE — reported as such rather than as 0.0
        // or as a NaN that would propagate into a printed governance figure.
        assert_eq!(a.shared_component, None, "an empty intersection has no shared component");
        assert_eq!(a.unshared_residual, None, "and no residual to take against it");
        assert!(
            out.lines.iter().any(|l| l.contains("shared-session component UNAVAILABLE")),
            "the output says so rather than printing NaN: {:?}",
            out.lines
        );
        assert!(
            !out.lines.iter().any(|l| l.contains("NaN")),
            "no NaN reaches the output: {:?}",
            out.lines
        );
    }

    // --- The three-hash comparability gate (KTD7) ---------------------------
    //
    // A fingerprint is range-scoped over hashed bars only, so an identical
    // fingerprint does not prove an identical derived universe. Each hash is
    // asserted independently, and the universe-hash case is specifically the one
    // a fingerprint-only gate would wave through.

    fn refusal_with(fp: &str, universe: &str, code: &str) -> String {
        let head = tempdir().unwrap();
        let arm = tempdir().unwrap();
        write_run_into(
            head.path(),
            HEAD_RUN,
            head_trades(),
            &frozen_fingerprint(),
            "uh",
            "ch",
            params(35),
        );
        write_run_into(arm.path(), ARM_RUN, arm_trades(), fp, universe, code, params(92));
        let err = report_paired(&cfg(head.path(), arm.path(), &[ARM_RUN]))
            .expect_err("a non-comparable pair is refused");
        format!("{err:#}")
    }

    #[test]
    fn an_arm_on_a_different_catalog_fingerprint_is_refused_by_name() {
        let msg = refusal_with("a-different-catalog-fingerprint", "uh", "ch");
        assert!(msg.contains("catalog_fingerprint"), "{msg}");
        assert!(msg.contains("NOT comparable"), "{msg}");
        assert!(msg.contains("MISSING INPUT"), "a refusal here is an input problem: {msg}");
    }

    #[test]
    fn an_arm_on_a_different_universe_hash_is_refused_even_when_the_fingerprint_matches() {
        let msg = refusal_with(&frozen_fingerprint(), "a-different-universe-hash", "ch");
        assert!(
            msg.contains("universe_hash"),
            "the case a fingerprint-only gate would pass: {msg}"
        );
        assert!(!msg.contains("`catalog_fingerprint` diverges"), "{msg}");
    }

    #[test]
    fn an_arm_on_a_different_strategy_code_hash_is_refused_by_name() {
        let msg = refusal_with(&frozen_fingerprint(), "uh", "a-different-code-hash");
        assert!(msg.contains("strategy_code_hash"), "{msg}");
    }

    // --- Missing and unusable inputs ----------------------------------------

    #[test]
    fn an_arm_with_no_closed_trades_is_refused_as_a_missing_input() {
        let head = tempdir().unwrap();
        let arm = tempdir().unwrap();
        let fp = frozen_fingerprint();
        write_run_into(head.path(), HEAD_RUN, head_trades(), &fp, "uh", "ch", params(35));
        let mut open_only = trade(1, 0.5);
        open_only.ts_closed = None;
        write_run_into(arm.path(), ARM_RUN, vec![open_only], &fp, "uh", "ch", params(92));
        let err = report_paired(&cfg(head.path(), arm.path(), &[ARM_RUN])).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("no CLOSED trades"), "{msg}");
    }

    #[test]
    fn an_arm_carrying_a_null_risk_capital_is_refused_as_a_pre_field_vintage() {
        let head = tempdir().unwrap();
        let arm = tempdir().unwrap();
        let fp = frozen_fingerprint();
        write_run_into(head.path(), HEAD_RUN, head_trades(), &fp, "uh", "ch", params(35));
        let mut legacy = trade(1, 0.5);
        legacy.risk_capital = None;
        write_run_into(
            arm.path(),
            ARM_RUN,
            vec![legacy, trade(2, 0.1)],
            &fp,
            "uh",
            "ch",
            params(92),
        );
        let err = report_paired(&cfg(head.path(), arm.path(), &[ARM_RUN])).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("PRE-FIELD vintage"), "{msg}");
    }

    #[test]
    fn an_empty_arm_list_is_refused_rather_than_reported_as_nothing_to_see() {
        let (head, arm) = two_homes();
        let err = report_paired(&cfg(head.path(), arm.path(), &[])).unwrap_err();
        assert!(format!("{err:#}").contains("LS_PAIRED_ARMS"), "{err:#}");
    }

    // --- Labels (KTD6) ------------------------------------------------------

    /// The head with both risk levers ARMED, mirroring the real v35 head.
    /// `OrbParams::default()` leaves both at `0.0`, which is the OFF state — so
    /// an arm built from the default differs from this head by exactly the
    /// levers it leaves off.
    fn armed_head_params() -> OrbParams {
        let mut p = params(35);
        p.risk_per_trade_krw = 299_340.0;
        p.ratio_atr_alpha = 1.0;
        p
    }

    /// Pair an armed head against an arm whose params are `arm_params`, and
    /// return the outcome.
    fn paired_with_params(arm_params: OrbParams) -> (tempfile::TempDir, tempfile::TempDir, PairedOutcome) {
        let head = tempdir().unwrap();
        let arm = tempdir().unwrap();
        let fp = frozen_fingerprint();
        write_run_into(head.path(), HEAD_RUN, head_trades(), &fp, "uh", "ch", armed_head_params());
        write_run_into(arm.path(), ARM_RUN, arm_trades(), &fp, "uh", "ch", arm_params);
        let out = report_paired(&cfg(head.path(), arm.path(), &[ARM_RUN])).unwrap();
        (head, arm, out)
    }

    #[test]
    fn an_arm_flipping_two_params_prints_both_and_is_marked_confounded() {
        // The real v95 arm: `risk_per_trade_krw` 299,340 -> 0 AND
        // `ratio_atr_alpha` 1.0 -> 0, while the frozen record labels it by the
        // first alone. KTD6 — print the confound, do not drop the arm.
        let mut two = armed_head_params();
        two.strategy_version = 92;
        two.risk_per_trade_krw = 0.0;
        two.ratio_atr_alpha = 0.0;
        let (_h, _a, out) = paired_with_params(two);
        let a = &out.arms[0];
        assert_eq!(a.param_diff.len(), 2, "both flips are recorded: {:?}", a.param_diff);
        assert!(a.label.contains("risk_per_trade_krw"), "{}", a.label);
        assert!(a.label.contains("ratio_atr_alpha"), "{}", a.label);
        assert!(
            !a.label.contains("strategy_version"),
            "strategy_version is the run's identity, not a lever — without the exclusion every \
             arm prints as multi-param and the confound signal is destroyed: {}",
            a.label
        );
        assert!(out.lines.iter().any(|l| l.contains("CONFOUNDED")), "the confound is printed");
    }

    #[test]
    fn a_single_param_arm_prints_one_lever_and_is_not_marked_confounded() {
        // The falsifier for the test above, on data that differs from the head:
        // one lever off, and the marker stays silent. If `strategy_version`
        // leaked into the diff this would print CONFOUNDED too and the marker
        // would carry no information at all.
        let mut one = armed_head_params();
        one.strategy_version = 92;
        one.ratio_atr_alpha = 0.0;
        let (_h, _a, out) = paired_with_params(one);
        let arm = &out.arms[0];
        assert_eq!(arm.param_diff.len(), 1, "exactly one lever moved: {:?}", arm.param_diff);
        assert!(arm.label.contains("ratio_atr_alpha"), "{}", arm.label);
        assert!(!arm.label.contains("risk_per_trade_krw"), "{}", arm.label);
        assert!(!out.lines.iter().any(|l| l.contains("CONFOUNDED")), "{:?}", out.lines);
    }

    // --- The staging guard and the exit contract ----------------------------

    #[test]
    fn the_staging_guard_holds_no_krw_profitability_number_reaches_the_output() {
        // A power question must not be decided by a profitability number. Net
        // RoR IS printed — it is the statistic being adjudicated — but the KRW
        // sums it is a ratio of are not.
        let (head, arm) = two_homes();
        let out = report_paired(&cfg(head.path(), arm.path(), &[ARM_RUN])).unwrap();
        let joined = out.lines.join("\n").to_lowercase();
        for banned in ["expectancy", "pnl", "p&l"] {
            assert!(!joined.contains(banned), "staging guard: {banned:?} reached the output");
        }
        assert!(joined.contains("net ror"), "the adjudicated statistic is still printed");
    }

    #[test]
    fn the_verb_completes_when_no_arm_is_attributable_because_a_stand_down_is_a_verdict() {
        let (head, arm) = two_homes();
        let out = report_paired(&cfg(head.path(), arm.path(), &[ARM_RUN])).unwrap();
        assert!(
            !out.arms[0].attributable,
            "this two-block fixture cannot resolve a +0.1 difference"
        );
        assert!(
            out.lines.iter().any(|l| l.starts_with("SUMMARY at the sample held: 0 of 1")),
            "{:?}",
            out.lines
        );
    }

    // --- The reachable-supply projection and the multiplicity line ----------
    //
    // Both reach the printed governance summary, so both are asserted THROUGH
    // `report_paired` rather than only against the formula in the hermetic
    // guard — a wiring bug between the two would otherwise ship green.

    /// The head the KTD10 projection root was measured against. Writing the
    /// pinned run id — rather than this module's synthetic `HEAD_RUN` — is what
    /// makes the projection apply.
    fn two_homes_with_the_pinned_head() -> (tempfile::TempDir, tempfile::TempDir) {
        let head = tempdir().unwrap();
        let arm = tempdir().unwrap();
        let fp = frozen_fingerprint();
        write_run_into(
            head.path(),
            PAIRED_HEAD_CALENDAR_SESSIONS_RUN,
            head_trades(),
            &fp,
            "uh",
            "ch",
            params(35),
        );
        write_run_into(arm.path(), ARM_RUN, arm_trades(), &fp, "uh", "ch", params(92));
        (head, arm)
    }

    #[test]
    fn the_reachable_supply_projection_is_wired_through_the_report() {
        let (head, arm) = two_homes_with_the_pinned_head();
        let mut c = cfg(head.path(), arm.path(), &[ARM_RUN]);
        c.head_run = PAIRED_HEAD_CALENDAR_SESSIONS_RUN.to_string();
        let out = report_paired(&c).unwrap();
        let a = &out.arms[0];
        let (z, zp) = (two_sided_z(SAMPLE_CONFIDENCE).unwrap(), power_z(SAMPLE_POWER).unwrap());

        let factor = (PAIRED_HEAD_CALENDAR_SESSIONS / PAIRED_REACHABLE_CALENDAR_SESSIONS).sqrt();
        let got_factor = out.projection_factor.expect("the pinned head projects");
        assert!((got_factor - factor).abs() < 1e-12, "{got_factor}");

        let se = a.projected_standard_error.expect("the pinned head projects");
        assert!(
            (se - a.bootstrap.standard_error * factor).abs() < 1e-12,
            "projected SE {se} is not the measured SE scaled by {factor}"
        );
        // Variance x sessions is conserved. An INVERTED ratio still satisfies
        // "projected = SE x some factor" but fails this, so this is the
        // assertion that actually pins the direction.
        assert!(
            (se.powi(2) * PAIRED_REACHABLE_CALENDAR_SESSIONS
                - a.bootstrap.standard_error.powi(2) * PAIRED_HEAD_CALENDAR_SESSIONS)
                .abs()
                < 1e-12,
            "variance x sessions is not conserved"
        );
        assert!(se < a.bootstrap.standard_error);

        // KTD11's minimum detectable paired difference at each supply level.
        assert!(
            (a.minimum_detectable_difference - (z + zp) * a.bootstrap.standard_error).abs() < 1e-12
        );
        assert!(
            (a.projected_minimum_detectable_difference.unwrap() - (z + zp) * se).abs() < 1e-12
        );

        // R9's second question: the verdict at the reachable supply is the SAME
        // observed difference against the PROJECTED bar.
        assert_eq!(a.attributable_at_reachable_supply, Some(a.bootstrap.point.abs() > z * se));
        // The projected bar is strictly lower, so an arm attributable now can
        // never read as unattributable there. A swapped comparison would.
        assert!(
            !(a.attributable && a.attributable_at_reachable_supply == Some(false)),
            "the projected verdict must be at least as permissive as the current one"
        );

        assert!(
            out.lines.iter().any(|l| l.contains("projected to the reachable supply")),
            "the per-arm projection line reaches the output"
        );
        assert!(
            out.lines.iter().any(|l| l.starts_with("SUMMARY projected to the reachable supply:")
                && !l.contains("WITHHELD")),
            "and so does the summary count: {:?}",
            out.lines
        );
    }

    #[test]
    fn the_projection_is_withheld_for_a_head_it_was_not_measured_against() {
        // The projection root is 45 CALENDAR sessions measured on ONE run, but
        // `head_run` is a free-form required parameter. Scaling a different
        // head's SE by that root would print an authoritative-looking figure
        // that is simply wrong — so it is withheld, not guessed.
        let (head, arm) = two_homes();
        let out = report_paired(&cfg(head.path(), arm.path(), &[ARM_RUN])).unwrap();
        assert_ne!(HEAD_RUN, PAIRED_HEAD_CALENDAR_SESSIONS_RUN, "the fixture head is a different run");
        assert_eq!(out.projection_factor, None);
        let a = &out.arms[0];
        assert_eq!(a.projected_standard_error, None);
        assert_eq!(a.projected_minimum_detectable_difference, None);
        assert_eq!(a.attributable_at_reachable_supply, None);

        // The measurement at the sample held is unaffected — only the
        // projection is withheld.
        assert!(a.bootstrap.standard_error > 0.0);
        assert!(a.minimum_detectable_difference > 0.0);

        let joined = out.lines.join("\n");
        assert!(joined.contains("WITHHELD"), "{joined}");
        assert!(
            joined.contains(PAIRED_HEAD_CALENDAR_SESSIONS_RUN),
            "the refusal names the run the root WAS measured for: {joined}"
        );
        assert!(
            joined.contains("unanswered for this head, not answered negatively"),
            "a withheld projection must not read as a negative verdict: {joined}"
        );
        // And no stale projected figure leaks into the output.
        assert!(
            !out.lines.iter().any(|l| l.contains("projection (KTD10): the paired SE scales")),
            "the scaling line must not print when the root does not apply"
        );
    }

    /// Write `n` arms into one arm home, each differing from the head by a
    /// different lever so their standard errors are not identical.
    fn write_n_arms(arm_home: &Path, n: usize) -> Vec<String> {
        let fp = frozen_fingerprint();
        (0..n)
            .map(|i| {
                let run_id = format!("20260601T00{:02}00Z-backtest-orb-v{}", i + 1, 92 + i);
                let mut p = armed_head_params();
                p.strategy_version = 92 + i as u32;
                match i {
                    0 => p.ratio_atr_alpha = 0.0,
                    1 => p.risk_per_trade_krw = 0.0,
                    _ => p.entry_confirm = 0.0,
                }
                // Vary the trades so the arms do not collapse to one SE.
                let trades = vec![
                    trade(1, 0.1 + i as f64 * 0.05),
                    trade(1, 0.1),
                    trade(2, 0.3 - i as f64 * 0.05),
                    trade(2, -0.1),
                    trade(3, 0.5),
                    trade(3, 0.3),
                ];
                write_run_into(arm_home, &run_id, trades, &fp, "uh", "ch", p);
                run_id
            })
            .collect()
    }

    #[test]
    fn the_family_wide_critical_value_is_bonferroni_over_the_supplied_arms() {
        let z = two_sided_z(SAMPLE_CONFIDENCE).unwrap();

        // One arm: no multiplicity to correct for, so the family value IS the
        // per-arm value. A hard-coded six-arm divisor would fail here.
        let (h1, a1) = two_homes();
        let one = report_paired(&cfg(h1.path(), a1.path(), &[ARM_RUN])).unwrap();
        assert!((one.critical_value - z).abs() < 1e-12);
        assert!(
            (one.family_critical_value - z).abs() < 1e-12,
            "at one arm the family bar is the per-arm bar, got {}",
            one.family_critical_value
        );

        // Three arms: two-sided alpha 0.05 split three ways.
        let head = tempdir().unwrap();
        let arms_home = tempdir().unwrap();
        write_run_into(
            head.path(),
            HEAD_RUN,
            head_trades(),
            &frozen_fingerprint(),
            "uh",
            "ch",
            armed_head_params(),
        );
        let ids = write_n_arms(arms_home.path(), 3);
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let three = report_paired(&cfg(head.path(), arms_home.path(), &refs)).unwrap();
        assert_eq!(three.arms.len(), 3);

        let want = two_sided_z(1.0 - (1.0 - SAMPLE_CONFIDENCE) / 3.0).unwrap();
        assert!(
            (three.family_critical_value - want).abs() < 1e-12,
            "family critical value {} != Bonferroni z at alpha/3 = {want}",
            three.family_critical_value
        );
        assert!(
            three.family_critical_value > three.critical_value,
            "correcting for multiplicity can only RAISE the bar"
        );
        // And the family verdict is never more permissive than the per-arm one.
        for a in &three.arms {
            assert!(
                !(a.attributable_family_wide && !a.attributable),
                "arm {} clears the family bar but not its own",
                a.run_id
            );
        }
        assert!(
            three.lines.iter().any(|l| l.contains("family-wide Bonferroni over 3 arms")),
            "the arm count reaches the printed critical-value line: {:?}",
            three.lines
        );
    }

    #[test]
    fn an_arm_within_monte_carlo_error_of_its_bar_is_reported_as_marginal() {
        // A bootstrap SD carries its own sampling error, SE / sqrt(2(B-1)) —
        // about 0.7% of the SE at 10,000 replicates. An arm inside that band of
        // its own threshold flips its verdict on a different seed with
        // identical data, so a bare boolean would publish a coin flip. The
        // report must say so.
        let (head, arm) = two_homes();
        let out = report_paired(&cfg(head.path(), arm.path(), &[ARM_RUN])).unwrap();
        let a = &out.arms[0];
        let z = two_sided_z(SAMPLE_CONFIDENCE).unwrap();

        // `bar_ratio` is the distance to the bar, and it decides the verdict.
        assert!(
            (a.bar_ratio - a.bootstrap.point.abs() / (z * a.bootstrap.standard_error)).abs()
                < 1e-12
        );
        assert_eq!(
            a.attributable,
            a.bar_ratio > 1.0,
            "attributable is exactly `|point| exceeds the bar`"
        );

        // The marginality band, recomputed from the formula.
        let band = (z * a.bootstrap.standard_error)
            / (2.0 * (a.bootstrap.replicates - 1) as f64).sqrt();
        let distance = (a.bootstrap.point.abs() - z * a.bootstrap.standard_error).abs();
        assert_eq!(a.marginal, distance <= band, "marginal is the Monte-Carlo band test");
        assert!(band > 0.0 && band < z * a.bootstrap.standard_error, "the band is a small slice");

        // Whatever this fixture happens to be, marginality must reach the
        // output whenever it is true, and never claim it when it is false.
        let joined = out.lines.join("\n");
        assert_eq!(
            a.marginal,
            joined.contains("MARGINAL"),
            "marginality must be printed iff it holds: {joined}"
        );
        // And the per-arm distance is always readable, marginal or not.
        assert!(
            joined.contains(&format!("{:.4} of the bar", a.bar_ratio)),
            "the distance to the bar is printed: {joined}"
        );
    }

    #[test]
    fn a_seed_change_cannot_silently_move_a_non_marginal_verdict() {
        // The falsifier for the marginality machinery: on an arm that is NOT
        // marginal, the verdict must be stable across seeds. If this ever fails,
        // the band is too narrow to be doing its job.
        let (head, arm) = two_homes();
        let mut a_cfg = cfg(head.path(), arm.path(), &[ARM_RUN]);
        let base = report_paired(&a_cfg).unwrap();
        if base.arms[0].marginal {
            return; // this fixture is marginal; the assertion above covers it
        }
        for seed in [1_u64, 42, 999, 20_260_806] {
            a_cfg.seed = seed;
            let out = report_paired(&a_cfg).unwrap();
            assert_eq!(
                out.arms[0].attributable, base.arms[0].attributable,
                "a non-marginal verdict moved at seed {seed} — the band is too narrow"
            );
        }
    }

    // --- CLI wiring ---------------------------------------------------------

    #[test]
    fn a_present_but_unparseable_replicate_or_seed_override_is_a_loud_refusal() {
        for (var, bad) in [("LS_SAMPLE_REPLICATES", "many"), ("LS_SAMPLE_SEED", "-1")] {
            let out = bin()
                .args(["report", "paired"])
                .env("LS_DATA_HOME", "/nonexistent-head-home")
                .env("LS_REPORT_RUN", HEAD_RUN)
                .env("LS_PAIRED_ARMS", ARM_RUN)
                .env(var, bad)
                .output()
                .unwrap();
            assert!(!out.status.success(), "{var}={bad} must not silently default");
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(stderr.contains(var), "the refusal names the variable: {stderr}");
        }
    }

    #[test]
    fn an_absent_head_run_is_refused_rather_than_defaulted_to_the_latest_finalized_run() {
        let out = bin()
            .args(["report", "paired"])
            .env("LS_DATA_HOME", "/nonexistent-head-home")
            .env("LS_PAIRED_ARMS", ARM_RUN)
            .env_remove("LS_REPORT_RUN")
            .output()
            .unwrap();
        assert!(!out.status.success());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("LS_REPORT_RUN is required"), "{stderr}");
        assert!(stderr.contains("never defaults"), "{stderr}");
    }

    #[test]
    fn the_verb_is_enumerated_by_the_compiled_bins_usage_and_unknown_mode_bail() {
        let out = bin().args(["report", "bogus-paired"]).output().unwrap();
        assert!(!out.status.success());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("report paired"), "the bail enumerates it: {stderr}");
        assert!(stderr.contains("report sample"), "beside the existing verbs: {stderr}");
        // And in USAGE, which the same bail appends.
        assert!(stderr.contains("usage: lab-research"), "{stderr}");
        let usage_line =
            stderr.lines().find(|l| l.contains("usage: lab-research")).expect("a usage line");
        assert!(usage_line.contains("report paired"), "{usage_line}");
    }

    /// No branch of the paired report reaches an acquisition path. Asserted on
    /// the source rather than the output: an ingest entry point could be called
    /// without printing anything, so the output is not evidence about it.
    #[test]
    fn no_code_path_in_the_paired_report_reaches_an_ingest_entry_point() {
        let src = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("runner").join("report.rs"),
        )
        .unwrap();
        let body = src
            .split_once("pub fn report_paired(")
            .expect("report_paired exists")
            .1
            .split_once("\n#[cfg(test)]")
            .expect("the test module terminates the non-test source")
            .0;
        for banned in [
            "accumulate(",
            "run_accumulate",
            "write_bars(",
            "write_instruments(",
            "delete_bar_series(",
            "heal(",
            "fetch(",
            // Unlike `report sample`, the paired report reads NO catalog at all.
            "read_all_bars",
        ] {
            assert!(
                !body.contains(banned),
                "report_paired must not reach {banned:?} — the measurement is a read-only fold \
                 from two run directories to one verdict"
            );
        }
    }
}
