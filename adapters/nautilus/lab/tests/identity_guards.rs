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

/// `runner/backtest.rs`'s source, for the two structural guards below. Neither the venue
/// config nor the shared candidate assembly sits behind a public API that a behavioural
/// test could pin without also pinning ORB's whole result, and both are edits that break
/// R3 *invisibly* — a changed OMS type or a changed assembly reorders ORB's fills without
/// failing anything that exists.
const BACKTEST_RS: &str = include_str!("../src/runner/backtest.rs");

/// `strategy/orb.rs`'s source. Its bytes *are* [`PINNED_ORB_CODE_HASH`].
const ORB_RS: &str = include_str!("../src/strategy/orb.rs");

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

/// The shared candidate assembly is unchanged (R3).
///
/// `build_candidates` and `build_candidates_with_today_open` live in `backtest.rs`, outside
/// `strategy_code_hash`'s file scope and outside `governed_params_hash`'s parameter scope —
/// so an edit here changes ORB's selected universe with **no** digest moving and no
/// existing test objecting. KTD15 reuses them at their current signatures for exactly this
/// reason; the daily path duplicates the *selection rule* instead.
#[test]
fn the_shared_candidate_assembly_is_unchanged() {
    for sig in [
        "pub(crate) fn build_candidates(",
        "pub(crate) fn build_candidates_with_today_open(",
    ] {
        assert!(
            BACKTEST_RS.contains(sig),
            "{sig} — the shared assembly's signature changed; the daily path reuses it at \
             its current signature (KTD15) and ORB's universe depends on its behaviour"
        );
    }
    // The delegation is the repo's extend-and-delegate shape: the two-arg form must still
    // be the one-arg form's caller, or the two can drift apart silently.
    assert!(
        BACKTEST_RS.contains("build_candidates_with_today_open(")
            && BACKTEST_RS.matches("fn build_candidates").count() == 2,
        "exactly the two assembly entry points exist"
    );
}

/// ORB's venue config is unchanged, `OmsType::Netting` included (R3).
///
/// The daily path uses `OmsType::Hedging` (KTD12) because Netting collapses position
/// identity — `determine_netting_position_id` mints one constant id per symbol, and a
/// re-entry takes the `reopen_position` path that snapshots the earlier round trip out of
/// the live index with no diagnostic. That is the right call for a multi-session hold and
/// the *wrong* call for ORB, whose fills would change. The two venue configs are
/// per-path; this asserts ORB's was not "helpfully" migrated along with the new one.
#[test]
fn the_orb_venue_config_is_unchanged() {
    assert!(
        BACKTEST_RS.contains("OmsType::Netting"),
        "ORB's venue must still be Netting — changing it changes ORB's fills (R3)"
    );
    assert!(
        !BACKTEST_RS.contains("OmsType::Hedging"),
        "the daily path's Hedging venue belongs in backtest_daily.rs, not here"
    );
    // ORB's exit submits with no position id and relies on `reduce_only`. That pattern is
    // Netting-only: under Hedging it opens an opposite-side position instead of closing the
    // long, and the account type does not reject the accidental short (KTD12).
    assert!(
        ORB_RS.contains("reduce_only"),
        "ORB's Netting-only exit pattern is unchanged"
    );
}

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
