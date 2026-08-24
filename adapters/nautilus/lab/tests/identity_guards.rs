//! U7 — identity-preservation guards for the daily path (P7).
//!
//! The failure this file exists to convert is the highest-consequence one in the plan and
//! it is **not** a test failure by default: an accidental move of `strategy_code_hash()`
//! shows up as `lab-live --mount` exiting 71, later, with the head silently resolving to
//! `OrbParams::default()`. Nothing about that points back at the commit that caused it.
//!
//! **The existing head assertions do not pin the hash.** `dispatch_cli.rs:798` and `:877`
//! assert `stdout.contains("7571abef")`, but the live-mount diagnostic prints that short
//! digest as *fixed prose* on every invocation (`live.rs:2823`, `:2963` — "the documented
//! head 7571abef…"), so both pass unchanged after any `orb.rs` edit. `paired_power.rs:238`
//! compares fixture JSON to the literal, not the binary. `rung_report.rs` compares a report
//! field to the function's own return value. None of them observes the computed digest.
//!
//! `research_cli.rs:679` (landed with U3) is the one place that does, and this file is the
//! named home the plan asks for — the guards live together, and U8 appends its registry
//! partition assertions here. The overlap with `research_cli.rs` is two lines and is
//! deliberate: a guard that only exists inside the suite for a *different* unit is a guard
//! one refactor away from disappearing.
//!
//! Offline throughout; no credentials, no network beyond the wiremock instrument masters.

use std::path::Path;

use chrono::{TimeZone, Utc};
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::ingest::checkpoint::Checkpoint;
use nautilus_ls::ingest::{
    build_daily_bar, build_minute_bar, write_bars, write_instruments, BarKind,
};
use nautilus_ls::instruments::{InstrumentDomain, InstrumentProvider};
use nautilus_ls_lab::artifacts::manifest::{strategy_code_hash, Manifest};
use nautilus_ls_lab::artifacts::performance::PerformanceReport;
use nautilus_ls_lab::artifacts::{MANIFEST_FILE, PERFORMANCE_FILE};
use nautilus_ls_lab::dispatch::ladder::governed_params_hash;
use nautilus_ls_lab::params::OrbParams;
use nautilus_ls_lab::runner::backtest::{run, BacktestConfig};
use nautilus_model::data::Bar;
use nautilus_model::identifiers::InstrumentId;
use serde_json::json;
use tempfile::tempdir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ---------------------------------------------------------------------------
// The pinned identity values
// ---------------------------------------------------------------------------

/// `strategy_code_hash()` over `strategy/orb.rs` — head identity.
///
/// Recorded at `tests/fixtures/paired-arms-closed-trades.json:10`, where it appears seven
/// times. KTD5 leaves the function alone entirely, so this holds by construction rather
/// than by care; the assertion is what turns "by construction" into something observed.
const PINNED_ORB_CODE_HASH: &str =
    "7571abefd715cfa0095ac04ba566f165b6e536cfcb7f86f4a8b88dcf2240133c";

/// `governed_params_hash(&OrbParams::default())` — head-**params** identity, a hash over
/// the *serialized* `OrbParams`. KTD4 keeps `Manifest.params` a concrete non-optional
/// `OrbParams` precisely so this cannot move: retyping it into an enum would change the
/// serialized JSON this hashes and detach every existing run from the running binary.
const PINNED_DEFAULT_GOVERNED_PARAMS_HASH: &str =
    "6a09279cb3182c90b0c2ec6d2b0ff0ba69ccbb94b69f184caf70098d5ecc0f3e";

// ---------------------------------------------------------------------------
// Scenario 1-2: the two pinned digests, by direct equality against the binary
// ---------------------------------------------------------------------------

/// `strategy_code_hash()` equals the full digest, and its signature is unchanged.
///
/// The zero-argument signature is asserted structurally: the call below takes no argument
/// and binds a `String`. KTD5's whole argument is that adding a strategy-id parameter would
/// buy eight edits at the crate's most identity-critical function, all passing the literal
/// `"orb"` — so the signature is as load-bearing as the value.
#[test]
fn the_orb_code_hash_and_its_signature_are_unchanged() {
    let computed: String = strategy_code_hash();
    assert_eq!(
        computed, PINNED_ORB_CODE_HASH,
        "strategy_code_hash() moved — this is head identity (R6). The symptom in production \
         is `lab-live --mount` exiting 71 with the head resolving to OrbParams::default(), \
         not a test failure. If an orb.rs edit was intended, it is a re-baseline, not a fix \
         to this literal."
    );
}

/// `governed_params_hash(&OrbParams::default())` equals its pre-change value (R7).
#[test]
fn the_default_governed_params_hash_is_unchanged() {
    assert_eq!(
        governed_params_hash(&OrbParams::default()),
        PINNED_DEFAULT_GOVERNED_PARAMS_HASH,
        "the head-params digest moved — either OrbParams gained/lost/renamed a field, or a \
         default changed. Both detach every existing run from the running binary."
    );
}

/// A daily run's code hash is the *sibling* digest, never the ORB one (R6, KTD5).
#[test]
fn a_daily_code_hash_differs_from_the_orb_value() {
    let daily = nautilus_ls_lab::artifacts::manifest::daily_strategy_code_hash(
        nautilus_ls_lab::strategy::DAILY_SOURCE,
    );
    assert_ne!(
        daily, PINNED_ORB_CODE_HASH,
        "the daily path must not advertise the ORB head digest"
    );
    assert_eq!(
        strategy_code_hash(),
        PINNED_ORB_CODE_HASH,
        "and computing the sibling cannot have moved the original"
    );
}

// ---------------------------------------------------------------------------
// Scenario 4-6: the surfaces outside every pinned hash
// ---------------------------------------------------------------------------

/// The per-day-reset gate is intact and was not co-opted for the daily path (R3).
///
/// `same_thread_sessions_are_independent` is the committed empirical result that a fresh
/// engine per session is independent. It explicitly does **not** cover the streaming
/// workflow KTD1 rests on, so the daily path needed its own carry-over gate rather than a
/// relaxation of this one.
#[test]
fn the_per_session_reset_gate_is_not_co_opted() {
    let backtest_run_rs = include_str!("backtest_run.rs");
    assert!(
        backtest_run_rs.contains("async fn same_thread_sessions_are_independent()"),
        "the ORB per-day-reset gate still exists under its own name"
    );
    assert!(
        !backtest_run_rs.contains("backtest_daily"),
        "and the ORB suite was not repointed at the daily runner"
    );
}

// ---------------------------------------------------------------------------
// Scenario 3: the ORB reproduction, run rather than argued
// ---------------------------------------------------------------------------

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

fn minute_json(
    date: &str,
    time: &str,
    o: &str,
    h: &str,
    l: &str,
    c: &str,
    v: &str,
) -> serde_json::Value {
    json!({ "date": date, "time": time, "open": o, "high": h, "low": l, "close": c,
        "jdiff_vol": v, "value": "0", "jongchk": "0", "rate": "0", "sign": "0" })
}

/// The ORB reproduction fixture — deliberately byte-for-byte the same catalog
/// `backtest_run.rs::build_fixture` builds (one +5% gapping symbol, a clean-breakout
/// minute session, a time-flat exit).
///
/// It is duplicated rather than imported because each `lab/tests/*.rs` is its own test
/// binary with no shared test-support module, so the helper is genuinely unreachable from
/// here. Keeping it identical is the point: the run below has to be the *same* run.
async fn build_orb_fixture(data_home: &Path) {
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
    .map(|r| {
        build_daily_bar(daily_bt, &serde_json::from_value(r.clone()).unwrap()).unwrap().unwrap()
    })
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
    .map(|r| {
        build_minute_bar(minute_bt, &serde_json::from_value(r.clone()).unwrap()).unwrap().unwrap()
    })
    .collect();
    write_bars(&catalog, minute).await.unwrap();

    let mut cp = Checkpoint::default();
    cp.adjusted_prices = true;
    cp.save(&catalog.join("ingest-checkpoint.json")).unwrap();
}

/// An ORB backtest over the fixed fixture produces the same position set and the same
/// `performance.json` after this work as before it (R3, AE2).
///
/// The plan's execution note is the reason this runs an engine rather than arguing from
/// inspection: R3 is the requirement most likely to be *believed* rather than verified.
///
/// **On what makes these literals a pre-change baseline.** A run captured on this branch
/// and asserted on this branch would be self-confirming. What licenses them is that the
/// branch's entire ORB-reachable delta against the merge-base `ea0c076` is three things,
/// and all three are provably behaviour-neutral:
///
/// 1. `runner/backtest.rs` gained the single line `daily_params: None` in the ORB manifest
///    literal. The field is `Option<DailyParams>` under
///    `skip_serializing_if = "Option::is_none"`, so the serialized ORB manifest is
///    byte-identical, and it is read by nothing on the engine path.
/// 2. `runner/backtest.rs` had two private helpers (`load_checkpoint`, `checkpoint_hash`)
///    widened to `pub(crate)` so the daily tail can reuse them. Visibility keywords.
/// 3. `artifacts/performance.rs` is **pure addition** — the only removed line in its whole
///    diff is a `use` statement widened to cover the new type. `from_positions_with_risk`,
///    `joined_risk`, and `dominance_fold` are untouched.
///
/// `strategy/orb.rs` is byte-identical, which the digest guard above observes directly. So
/// the values below are what `ea0c076` produced, and this test is what keeps them that way.
/// Re-derive that claim (do not trust this comment) with:
/// `git diff ea0c076..HEAD -- adapters/nautilus/lab/src/`.
#[tokio::test]
async fn an_orb_run_reproduces_its_pre_change_positions_and_performance() {
    let dir = tempdir().unwrap();
    build_orb_fixture(dir.path()).await;

    let start = Utc.with_ymd_and_hms(2024, 1, 6, 0, 0, 0).unwrap();
    let outcome = run(BacktestConfig::new(dir.path(), "20240102", "20240105"), start)
        .await
        .unwrap();

    let manifest: Manifest = serde_json::from_str(
        &std::fs::read_to_string(outcome.run_dir.join(MANIFEST_FILE)).unwrap(),
    )
    .unwrap();
    let perf: PerformanceReport = serde_json::from_str(
        &std::fs::read_to_string(outcome.run_dir.join(PERFORMANCE_FILE)).unwrap(),
    )
    .unwrap();

    // Identity first: an ORB run still records the ORB digests and the ORB discriminator.
    assert_eq!(manifest.strategy_code_hash, PINNED_ORB_CODE_HASH);
    assert_eq!(manifest.strategy_id, nautilus_ls_lab::params::STRATEGY_ID);
    assert_eq!(governed_params_hash(&manifest.params), PINNED_DEFAULT_GOVERNED_PARAMS_HASH);
    assert!(
        manifest.daily_params.is_none(),
        "an ORB run carries no daily params — `validate_strategy_identity` refuses that \
         combination outright, but a run that never produces it is the real guard"
    );
    // U6's fifth artifact is the daily path's alone. Asserted POSITIVELY here because the
    // existing artifact test only checks that four expected files are present — it never
    // asserts the set is exactly four, so an ORB run that started emitting an observation
    // would pass it.
    assert!(
        !outcome.run_dir.join(nautilus_ls_lab::artifacts::OBSERVATION_FILE).exists(),
        "an ORB run writes no observation: the artifact carries the DAILY lineage's frozen \
         verdict statistic, and an ORB run emitting one would be a false claim to it"
    );

    // The position set, field by field. A count alone would pass against a run that
    // entered the right number of positions at the wrong prices.
    assert_eq!(perf.trades.len(), 1, "one round trip: {:?}", perf.trades);
    let t = &perf.trades[0];
    assert_eq!(t.symbol, "005930.XKRX");
    assert_eq!(t.entry_side, "BUY");
    assert_eq!(t.quantity, PINNED_ORB_QTY);
    assert_eq!(t.avg_px_open, PINNED_ORB_ENTRY_PX);
    assert_eq!(t.avg_px_close, Some(PINNED_ORB_EXIT_PX));
    assert_eq!(t.realized_pnl, PINNED_ORB_REALIZED_PNL);
    assert_eq!(t.risk_capital, Some(PINNED_ORB_RISK_CAPITAL));

    // …and the whole summary map, so a new or dropped key is caught too.
    let mut summary: Vec<(String, f64)> =
        perf.summary.iter().map(|(k, v)| (k.clone(), *v)).collect();
    summary.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(summary, pinned_orb_summary(), "performance.json summary moved");
}

// The fixture's single round trip. Breakout entry at 63,900 against the 62,400 stop
// (1,500/share × 156 = 234,000 risk capital), time-flat exit at 65,000
// (1,100/share × 156 = 171,600 realized). The arithmetic is stated so a future reader can
// tell a *changed* number from a *wrong* one.
const PINNED_ORB_QTY: f64 = 156.0;
const PINNED_ORB_ENTRY_PX: f64 = 63_900.0;
const PINNED_ORB_EXIT_PX: f64 = 65_000.0;
const PINNED_ORB_REALIZED_PNL: f64 = 171_600.0;
const PINNED_ORB_RISK_CAPITAL: f64 = 234_000.0;

/// The full `performance.json` summary map, sorted by key.
///
/// Pinned whole rather than key-by-key so a *new* or *dropped* key fails too — the
/// existing ORB suite only asserts `num_trades` and `pnl_total > 0`, which a run that
/// silently stopped emitting half the map would still satisfy. `PnL (total)` is nautilus's
/// own zero-valued key and `pnl_total` is the lab's; both are recorded as they are.
fn pinned_orb_summary() -> Vec<(String, f64)> {
    [
        ("Average (Return)", 0.001_716),
        ("Average Win (Return)", 0.001_716),
        ("Avg Winner", 171_600.0),
        ("Expectancy", 171_600.0),
        ("Max Winner", 171_600.0),
        ("Min Winner", 171_600.0),
        ("PnL (total)", 0.0),
        ("PnL% (total)", 0.0),
        ("Win Rate", 1.0),
        ("max_drawdown", 0.0),
        ("num_trades", 1.0),
        ("pnl_total", 171_600.0),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

// ---------------------------------------------------------------------------
// U8 — the registry strategy partition (KTD14, R24)
//
// Nothing structurally separates the two strategies' data homes: the separation is one
// `LS_DATA_HOME` slip deep, and every failure this partition guards is silent. A daily run
// finalized after the newest ORB run would otherwise become the ORB turn's adopted params,
// its inherited range, its KEEP/REVERT baseline, and its trial anchor — with no error
// anywhere, because `Manifest.params` is a non-optional `OrbParams` and so a daily run
// asserts a complete, fictitious ORB parameter set.
// ---------------------------------------------------------------------------

use nautilus_ls_lab::artifacts::manifest::{
    range_fingerprint, universe_hash, DailyManifestParts, DataRange,
};
use nautilus_ls_lab::artifacts::{list_runs, run_id, RunSource, RunWriter};
use nautilus_ls_lab::params_daily::{DailyParams, DAILY_STRATEGY_ID};
use nautilus_ls_lab::runner::research::{latest_finalized_run, latest_finalized_run_for};

/// Read a staged run's manifest off disk. `research::read_manifest` is crate-private, and
/// deliberately so — the partition's whole point is that a consumer resolves runs through
/// the filtered lookup rather than reaching for individual manifests.
fn manifest_of(data: &Path, run_id: &str) -> Manifest {
    let text =
        std::fs::read_to_string(data.join("runs").join(run_id).join(MANIFEST_FILE)).unwrap();
    serde_json::from_str(&text).unwrap()
}

/// An ORB manifest stamped at `hour`, staged into `data`.
fn stage_orb_run(data: &Path, hour: u32, version: u32) -> String {
    let started = Utc.with_ymd_and_hms(2024, 1, 5, hour, 0, 0).unwrap();
    let params = OrbParams { strategy_version: version, ..OrbParams::default() };
    let id = run_id(started, RunSource::Backtest, &params.strategy_id, version);
    let m = Manifest {
        run_id: id.clone(),
        source: RunSource::Backtest,
        strategy_id: params.strategy_id.clone(),
        strategy_version: version,
        params,
        data_range: DataRange { start: "20240102".into(), end: "20240105".into() },
        catalog_fingerprint: range_fingerprint(&[], 0, u64::MAX),
        universe_hash: universe_hash(&["005930.XKRX".to_string()]),
        strategy_code_hash: strategy_code_hash(),
        lab_src_fingerprint: None,
        checkpoint_hash: None,
        universe_metadata_hash: None,
        dispatch: None,
        daily_params: None,
        created_utc: started.to_rfc3339(),
    };
    let w = RunWriter::new(data, &id).unwrap();
    w.write_manifest(&m).unwrap();
    w.finalize().unwrap();
    id
}

/// A daily manifest stamped at `hour`, staged into `data`. Built through
/// `Manifest::new_daily` — the only production path — so the test cannot accidentally
/// hand-write a discriminator the real runner would never produce.
fn stage_daily_run(data: &Path, hour: u32) -> String {
    let started = Utc.with_ymd_and_hms(2024, 1, 5, hour, 0, 0).unwrap();
    let m = Manifest::new_daily(DailyManifestParts {
        daily: DailyParams::default(),
        assembly_params: OrbParams::default(),
        daily_source: nautilus_ls_lab::strategy::DAILY_SOURCE,
        started_utc: started,
        data_range: DataRange { start: "20240102".into(), end: "20240105".into() },
        catalog_fingerprint: range_fingerprint(&[], 0, u64::MAX),
        universe_hash: universe_hash(&["005930.XKRX".to_string()]),
        lab_src_fingerprint: None,
        checkpoint_hash: None,
        universe_metadata_hash: None,
    })
    .unwrap();
    let id = m.run_id.clone();
    let w = RunWriter::new(data, &id).unwrap();
    w.write_manifest(&m).unwrap();
    w.finalize().unwrap();
    id
}

/// The core partition: a daily run finalized AFTER the newest ORB run is not what "the
/// current run" resolves to.
///
/// All seven consumers that trust `latest_finalized_run` — `turn()`'s params adoption, the
/// inherited range, `decide_keep_or_revert`, the diagnose trial anchor, and the three
/// reporting commands — go through this one function, so this is the partition for every
/// one of them at once. `the_seven_consumers_all_resolve_through_the_filtered_lookup`
/// below is what keeps that true.
#[test]
fn a_newer_daily_run_is_not_the_current_orb_run() {
    let dir = tempdir().unwrap();
    let data = dir.path();
    let orb = stage_orb_run(data, 9, 35);
    let daily = stage_daily_run(data, 17);

    assert_eq!(list_runs(data).len(), 2, "both runs are in the registry");
    let (resolved, m) = latest_finalized_run(data).unwrap().expect("an ORB run resolves");
    assert_eq!(resolved, orb, "the newer daily run did not displace the ORB head");
    assert_eq!(m.strategy_id, nautilus_ls_lab::params::STRATEGY_ID);
    assert!(m.daily_params.is_none());

    // The daily run is still reachable — by name, which is the point. The partition is a
    // partition, not a deletion.
    let (dresolved, dm) =
        latest_finalized_run_for(data, DAILY_STRATEGY_ID).unwrap().expect("the daily run resolves");
    assert_eq!(dresolved, daily);
    assert_eq!(dm.strategy_id, DAILY_STRATEGY_ID);
}

/// Without this, every filter above would pass for the WRONG reason.
///
/// Both `Manifest.strategy_id` and the run id derive from the parameter set's
/// `strategy_id`, whose `OrbParams` default is `"orb"`. Had the daily runner written
/// `OrbParams::default()` into the non-optional `params` field and derived the
/// discriminator from it, every strategy filter would pass the daily run through and the
/// partition would be vacuous — the exact silent head-reversion it exists to prevent.
#[test]
fn a_daily_manifest_is_not_identified_as_orb() {
    let dir = tempdir().unwrap();
    let daily = stage_daily_run(dir.path(), 17);
    let m = manifest_of(dir.path(), &daily);

    assert_ne!(m.strategy_id, nautilus_ls_lab::params::STRATEGY_ID);
    assert_eq!(m.strategy_id, DAILY_STRATEGY_ID);
    assert!(!daily.contains("-orb-"), "nor does the run id read as an ORB run: {daily}");
    // …while the assembly params it records still carry ORB's id, deliberately ignored.
    assert_eq!(m.params.strategy_id, nautilus_ls_lab::params::STRATEGY_ID);
}

/// Head selection does not resolve a daily run either — belt-and-braces beside the
/// code-hash filter, and the two filter chains stay verbatim-identical.
#[test]
fn head_selection_does_not_resolve_a_daily_run() {
    use nautilus_ls_lab::dispatch::ladder::{head_governed_params_pinned, head_manifest_pinned};
    let dir = tempdir().unwrap();
    let data = dir.path();
    let orb = stage_orb_run(data, 9, 35);
    stage_daily_run(data, 17);

    let (head_id, head) = head_manifest_pinned(data, Some(35)).expect("the ORB head resolves");
    assert_eq!(head_id, orb);
    assert_eq!(head.strategy_id, nautilus_ls_lab::params::STRATEGY_ID);
    assert_eq!(head_governed_params_pinned(data, Some(35)).strategy_id, "orb");
}

/// An unreadable **older** manifest is skipped, not fatal.
///
/// A filtered lookup has to read *every* manifest rather than only the newest, so it now
/// touches old runs a bare newest-by-id lookup never opened. A strict read of those would
/// turn a previously-succeeding lookup into a hard error the first time one failed to parse
/// — a regression introduced by the fix rather than by the bug.
#[test]
fn an_unreadable_older_manifest_is_skipped() {
    let dir = tempdir().unwrap();
    let data = dir.path();
    let old = stage_orb_run(data, 8, 34);
    let newer = stage_orb_run(data, 9, 35);
    // The daily run is newest, so the ORB lookup falls through to the scan over the rest.
    stage_daily_run(data, 17);

    std::fs::write(data.join("runs").join(&old).join(MANIFEST_FILE), "{ not json").unwrap();

    let (resolved, _) = latest_finalized_run(data).unwrap().expect("the readable ORB run resolves");
    assert_eq!(resolved, newer);
}

/// An unreadable **newest** manifest is a hard error, not a silent fallback.
///
/// This is the boundary of the skip-on-unreadable tolerance above, and getting it wrong is
/// the worst available failure: the pre-partition lookup read `ordered_runs().last()` and
/// propagated its parse error, so swallowing it here would be a NEW silence. With a valid
/// older ORB run present, a corrupt newest manifest would resolve the older run as the
/// apparent head and every consumer would adopt stale params, a stale range, and a stale
/// KEEP/REVERT baseline. With no older run it would return `None`, which
/// `decide_keep_or_revert` cannot tell apart from a fresh registry.
#[test]
fn an_unreadable_newest_manifest_is_an_error_not_a_stale_head() {
    // (a) A corrupt newest ORB run with a valid older one behind it must NOT resolve the
    //     older run — that is the silent stale head.
    let dir = tempdir().unwrap();
    let data = dir.path();
    let older = stage_orb_run(data, 8, 34);
    let newest = stage_orb_run(data, 9, 35);
    std::fs::write(data.join("runs").join(&newest).join(MANIFEST_FILE), "{ not json").unwrap();

    let err = latest_finalized_run(data).unwrap_err();
    assert!(
        !err.to_string().contains(&older),
        "the error names the unreadable run, not the one it would have fallen back to: {err}"
    );

    // (b) A corrupt sole run errors rather than reading as a fresh registry.
    let dir2 = tempdir().unwrap();
    let solo = stage_orb_run(dir2.path(), 8, 34);
    std::fs::write(dir2.path().join("runs").join(&solo).join(MANIFEST_FILE), "{ not json").unwrap();
    assert!(
        latest_finalized_run(dir2.path()).is_err(),
        "a corrupt sole manifest must not read as `None` — decide_keep_or_revert treats \
         `None` as licence to skip the RoR comparison"
    );

    // (c) An EMPTY registry is still `None`, not an error — the distinction the fix preserves.
    let dir3 = tempdir().unwrap();
    assert!(latest_finalized_run(dir3.path()).unwrap().is_none());
}

/// With only ORB runs present, every consumer resolves exactly as it did before this unit
/// — the filters are asserted no-ops on a single-strategy registry.
#[test]
fn a_single_strategy_registry_resolves_exactly_as_before() {
    let dir = tempdir().unwrap();
    let data = dir.path();
    stage_orb_run(data, 8, 34);
    let newest = stage_orb_run(data, 9, 35);

    let (resolved, m) = latest_finalized_run(data).unwrap().unwrap();
    assert_eq!(resolved, newest, "still the newest by run-order key");
    assert_eq!(m.strategy_version, 35);
    // A fresh registry is still `None`, not an error.
    let empty = tempdir().unwrap();
    assert!(latest_finalized_run(empty.path()).unwrap().is_none());
}

/// `compare` refuses a cross-strategy pair in EVERY mode, naming the mismatch.
///
/// The incidental guards (code-hash equality, a wide param diff) would catch most such
/// pairs but not reliably: a daily run records a full `OrbParams` it never ran under, so a
/// daily run whose recorded assembly params match an ORB run's produces an *empty* param
/// diff and reads as a clean reproduction of it.
#[test]
fn compare_refuses_a_cross_strategy_pair_in_every_mode() {
    use nautilus_ls_lab::runner::research::{compare, CompareConfig, CompareMode};
    let dir = tempdir().unwrap();
    let data = dir.path();
    let orb = stage_orb_run(data, 9, 35);
    let daily = stage_daily_run(data, 17);

    for mode in [CompareMode::Param, CompareMode::Data, CompareMode::Code] {
        let err = compare(&CompareConfig {
            data_home: data.to_path_buf(),
            run_a: Some(orb.clone()),
            run_b: Some(daily.clone()),
            mode,
            explanation: Some("explained".to_string()),
        })
        .expect_err("a cross-strategy pair is refused, not merely failed");
        let msg = err.to_string();
        assert!(msg.contains("different strategies"), "{mode:?}: {msg}");
        assert!(msg.contains(DAILY_STRATEGY_ID), "{mode:?} names the mismatch: {msg}");
    }

    // A same-strategy pair still compares (the refusal is not a blanket one).
    let orb_b = stage_orb_run(data, 10, 36);
    let out = compare(&CompareConfig {
        data_home: data.to_path_buf(),
        run_a: Some(orb),
        run_b: Some(orb_b),
        mode: CompareMode::Param,
        explanation: None,
    });
    assert!(out.is_ok(), "two ORB runs still compare: {:?}", out.err());
}

/// The seven consumers all resolve "the current run" through the filtered lookup.
///
/// The partition lives in `latest_finalized_run`'s default rather than in a parameter
/// threaded through each site — the same reasoning KTD5 applies to `strategy_code_hash()`,
/// where a parameter every site passes `"orb"` to is just seven chances to forget. What
/// that trades away is the compiler's help: a consumer could reintroduce a bare
/// newest-by-id scan of its own and nothing would object. This guard is that objection.
#[test]
fn the_seven_consumers_all_resolve_through_the_filtered_lookup() {
    let research = include_str!("../src/runner/research.rs");
    let governed = include_str!("../src/runner/governed.rs");
    let report = include_str!("../src/runner/report.rs");

    // The seven consumers map onto SIX call sites, because `turn()`'s params adoption and
    // its range inheritance share one `prior` binding. Counted exactly, so that a consumer
    // quietly dropping the lookup — or a new one appearing that never went through it —
    // shows up as an arithmetic mismatch rather than being absorbed.
    //
    //   research.rs  `:429`  turn params adoption AND range inheritance (one binding)
    //   research.rs  `:2189` the diagnose trial anchor
    //   governed.rs  `:196`  the KEEP/REVERT baseline
    //   report.rs    `:328`, `:491`, `:1015`  the three reporting commands
    //
    // The `+ 1`s are non-call occurrences of the same token: research.rs's own `pub fn`
    // definition, and report.rs's `absent_run_id_defaults_to_latest_finalized_run()` test
    // name.
    assert_eq!(
        research.matches("latest_finalized_run(").count(),
        2 + 1,
        "research.rs: two call sites covering three consumers, plus the definition"
    );
    assert_eq!(governed.matches("latest_finalized_run(").count(), 1, "the KEEP/REVERT baseline");
    assert_eq!(
        report.matches("latest_finalized_run(").count(),
        3 + 1,
        "report.rs: the three reporting commands, plus a test name"
    );

    // No consumer resolves "the newest run" by scanning ids itself. `ordered_runs` is
    // private to research.rs by design; the guard is that the other two files never grow
    // their own equivalent.
    for (name, src) in [("governed.rs", governed), ("report.rs", report)] {
        assert!(
            !src.contains("list_runs(") && !src.contains("ordered_runs("),
            "{name} must resolve the current run through the filtered lookup, not by \
             scanning run ids — an unfiltered scan is exactly the silent path R24 closes"
        );
    }
}
