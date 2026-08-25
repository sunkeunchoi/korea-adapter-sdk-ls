//! U5 — the `lab-backtest-daily` entry point.
//!
//! The dead-code hazard is the point of this unit: a daily path reachable only from
//! `#[test]` bodies is dead code with a green coverage report
//! (docs/solutions/architecture-patterns/a-safety-escape-hatch-wired-to-none-at-the-
//! composition-root-is-dead-code-its-unit-tests-still-pass.md). Each scenario below is
//! therefore marked *(binary)* — driving the compiled bin through `CARGO_BIN_EXE_*`, which
//! exercises the real composition root — or *(library)*, for the two seams that are
//! deliberately unreachable from `main_cli`.

use std::collections::HashMap;
use std::path::Path;

use chrono::{NaiveDate, TimeZone, Utc};
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_ls_lab::agent::envelope::{self as envelope, DecisionEnvelope};
use nautilus_ls_lab::agent::sink::DecisionSink;
use nautilus_ls_lab::artifacts::manifest::Manifest;
use nautilus_ls_lab::artifacts::performance::PerformanceReport;
use nautilus_ls_lab::artifacts::{
    aborted_runs, list_runs, run_id, RunSource, DECISIONS_FILE, MANIFEST_FILE, PERFORMANCE_FILE,
};
use nautilus_ls_lab::params_daily::{DAILY_STRATEGY_ID, FROZEN_ATR_WINDOW_SESSIONS};
use nautilus_ls_lab::runner::backtest_daily::{run, run_inner, select_daily_sessions};
use tempfile::tempdir;

use crate::fixture::{
    build_daily_fixture, cfg, daily_json, kst_date, rank_all, write_daily_series, RANGE_END,
    RANGE_START,
};

/// A `lab-backtest-daily` invocation over `data_home` with no `LS_BTD_*` set beyond the
/// three the caller supplies. The environment is cleared rather than inherited: this
/// shell exports a dozen `LS_*` variables, and an inherited `LS_DATA_HOME` would make the
/// missing-variable scenario pass for the wrong reason.
fn daily_bin(data_home: Option<&Path>) -> std::process::Command {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_lab-backtest-daily"));
    cmd.env_clear();
    if let Some(home) = data_home {
        cmd.env("LS_DATA_HOME", home);
    }
    cmd.env("LS_BTD_SDATE", RANGE_START).env("LS_BTD_EDATE", RANGE_END);
    cmd
}

/// *(binary)* The compiled bin lands a finalized run in the registry, and that run holds a
/// position across session boundaries — the whole point of the path, proven through the
/// composition root rather than through `run_daily` directly.
#[tokio::test]
async fn compiled_bin_lands_a_finalized_run_holding_across_sessions() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;

    let out = daily_bin(Some(dir.path())).env("LS_BTD_TARGET_M", "2").output().unwrap();
    assert!(
        out.status.success(),
        "bin failed: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let runs = list_runs(dir.path());
    assert_eq!(runs.len(), 1, "exactly one finalized run: {runs:?}");
    assert!(aborted_runs(dir.path()).is_empty(), "no staging residue");

    let run_dir = dir.path().join("runs").join(&runs[0]);
    let manifest: Manifest =
        serde_json::from_str(&std::fs::read_to_string(run_dir.join(MANIFEST_FILE)).unwrap())
            .unwrap();
    // The registry discriminator, written by the real binary (KTD14). U8's filters key on
    // exactly this field; if the bin wrote "orb" here they would all be no-ops.
    assert_eq!(manifest.strategy_id, DAILY_STRATEGY_ID);
    assert!(runs[0].contains(DAILY_STRATEGY_ID), "the run id carries the discriminator: {}", runs[0]);
    assert!(manifest.daily_params.is_some(), "the daily terms are carried");
    let decision_text = std::fs::read_to_string(run_dir.join(DECISIONS_FILE)).unwrap();
    let decisions = envelope::from_jsonl(&decision_text).unwrap();
    assert!(!decisions.is_empty(), "the finalized decision stream is non-empty");
    let line_order: Vec<_> = decision_text
        .lines()
        .map(|line| serde_json::from_str::<DecisionEnvelope>(line).unwrap().envelope_id)
        .collect();
    let parsed_order: Vec<_> = decisions.iter().map(|decision| decision.envelope_id).collect();
    assert_eq!(parsed_order, line_order, "the JSONL parser preserves append order");
    // The ATR bridge reached the manifest: `OrbParams::atr_window` defaults to 14, and an
    // unbridged run would record that and refuse every entry as `atr_unavailable`.
    assert_eq!(
        manifest.params.atr_window,
        FROZEN_ATR_WINDOW_SESSIONS,
        "assembly ran with the frozen daily ATR window, not OrbParams' default 14"
    );

    // A position held across session boundaries. The fixture is 21 in-range sessions
    // against a 16-session hold, so a session-1 entry is still open on session 16.
    let report: PerformanceReport =
        serde_json::from_str(&std::fs::read_to_string(run_dir.join(PERFORMANCE_FILE)).unwrap())
            .unwrap();
    let spans_sessions = report.trades.iter().any(|t| match t.ts_closed {
        // Closed on a later KST session than it opened on — a genuine overnight hold,
        // which the per-session ORB path cannot produce at all.
        Some(closed) => kst_date(closed) > kst_date(t.ts_opened),
        // Still open at range end, having opened before the last session: censored, and
        // equally proof the position outlived an engine batch.
        None => kst_date(t.ts_opened) < NaiveDate::from_ymd_opt(2024, 1, 31).unwrap(),
    });
    assert!(
        spans_sessions,
        "at least one position outlives its opening session: {:?}",
        report.trades
    );
    assert!(
        out.stdout.windows(b"lab-backtest-daily summary".len()).any(|w| w == b"lab-backtest-daily summary"),
        "the trailing summary block prints"
    );
}

/// *(binary)* A missing `LS_DATA_HOME` names that variable rather than failing obscurely
/// downstream on a path that does not exist.
#[tokio::test]
async fn compiled_bin_names_the_missing_data_home() {
    let out = daily_bin(None).output().unwrap();
    assert!(!out.status.success(), "the bin must not succeed without LS_DATA_HOME");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("LS_DATA_HOME"), "the error names the variable: {err}");
}

/// *(binary)* A malformed numeric variable errors rather than silently defaulting.
///
/// This is the `backtest.rs:1049` `unwrap_or(1)` anti-pattern, pinned by
/// `research_cli.rs:430`. Silently defaulting here would finalize a run at a concurrency
/// nobody chose and record the substituted value in the manifest as though intended.
#[tokio::test]
async fn compiled_bin_refuses_a_malformed_numeric_variable() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;

    let out = daily_bin(Some(dir.path())).env("LS_BTD_TARGET_M", "2x").output().unwrap();
    assert!(!out.status.success(), "a malformed LS_BTD_TARGET_M must not default");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("LS_BTD_TARGET_M"), "the error names the variable: {err}");
    assert!(err.contains("2x"), "the error quotes the offending value: {err}");
    assert!(list_runs(dir.path()).is_empty(), "no run was finalized");
}

/// *(library)* A catalog mutated in-range between the engine run and the finalize
/// fingerprint re-check aborts with no registry residue. The `before_finalize` seam is a
/// library-only surface — deliberately not reachable through `main_cli`.
#[tokio::test]
async fn mid_run_catalog_change_aborts_with_no_residue() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    let catalog = dir.path().join("catalog");
    let start = Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap();
    let config = cfg(dir.path(), 2);
    let staged_run_id = run_id(
        start,
        RunSource::Backtest,
        DAILY_STRATEGY_ID,
        config.daily.strategy_version,
    );
    let staged_decisions = dir
        .path()
        .join("runs")
        .join(format!(".tmp-{staged_run_id}"))
        .join(DECISIONS_FILE);

    // Append an extra in-range daily bar after the engine run, before the re-check.
    let mutate = async {
        let streamed = std::fs::read_to_string(&staged_decisions).unwrap();
        assert!(
            !envelope::from_jsonl(&streamed).unwrap().is_empty(),
            "decision envelopes reach staging before finalization"
        );
        write_daily_series(
            &catalog,
            "005930.XKRX",
            &[daily_json("20240130", "70000", "70500", "69500", "70000", "999")],
        )
        .await;
    };

    let err = run_inner(config, start, mutate).await.unwrap_err();
    assert!(err.to_string().contains("catalog changed in-range"), "err: {err}");
    assert!(list_runs(dir.path()).is_empty(), "no finalized run");
    assert!(aborted_runs(dir.path()).is_empty(), "no staging residue");
}

/// *(library)* A malformed range is discovered after the run writer opens staging but
/// before the blocking engine starts, so it is a graceful refusal rather than an abort.
#[tokio::test]
async fn pre_engine_parse_refusal_removes_staging() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    let mut config = cfg(dir.path(), 2);
    config.range.start = "not-a-date".to_string();

    let start = Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap();
    let err = run(config, start).await.unwrap_err();
    assert!(err.to_string().contains("input contains invalid characters"), "err: {err}");
    assert!(list_runs(dir.path()).is_empty(), "no finalized run");
    assert!(aborted_runs(dir.path()).is_empty(), "refusal removes staging");
}

/// *(library)* The run refuses to start while the ingest advisory lock is held, and the
/// single guard spans the engine phase and the finalize re-check.
#[tokio::test]
async fn refused_while_ingest_lock_held() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    let catalog = dir.path().join("catalog");
    let _held = AdvisoryLock::acquire(&catalog, LockKind::Ingest).unwrap();

    let start = Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap();
    let err = run(cfg(dir.path(), 2), start).await.unwrap_err();
    assert!(err.to_string().contains("refused"), "err: {err}");
    assert!(list_runs(dir.path()).is_empty());
}

/// An invalid daily parameter set is refused **before** the engine runs, not at manifest
/// assembly hours later. `Manifest::new_daily` remains the construction-point gate; this
/// is the fail-fast one.
#[tokio::test]
async fn an_off_freeze_parameter_set_is_refused_before_the_engine_runs() {
    let dir = tempdir().unwrap();
    // No catalog is built: reaching the "no catalog" error would prove validation ran
    // AFTER the catalog check, and reaching the engine would prove it ran after that.
    let mut c = cfg(dir.path(), 2);
    c.daily.holding_period_sessions = 5;

    let start = Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap();
    let err = run(c, start).await.unwrap_err();
    assert!(err.to_string().contains("invalid daily parameter set"), "err: {err}");
    assert!(
        !err.to_string().contains("no catalog"),
        "validation runs before the catalog check, so a bad set never reaches the engine: {err}"
    );
}

/// The ATR bridge, pinned directly: `DailyParams::atr_window_sessions` reaches the shared
/// candidate assembly, and `OrbParams::atr_window`'s default does not.
///
/// This is the unit's sharpest silent failure. The frozen daily stop is ATR(1); the
/// `OrbParams` the assembly reads defaults `atr_window` to 14, needing 15 prior sessions.
/// Nothing but `assembly_params()` connects the two. Unbridged, the first 14 in-range
/// sessions of every symbol derive **no** prior ATR — and because the stop fails closed
/// (KTD9), that is not a visible misconfiguration: every entry is refused
/// `atr_unavailable`, the run finalizes green with zero positions, and `return_on_risk`
/// is vacuous. The assertion below is the difference between those two worlds.
#[tokio::test]
async fn the_frozen_atr_window_reaches_candidate_assembly() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    let c = cfg(dir.path(), 2);

    assert_eq!(
        c.params.atr_window,
        nautilus_ls_lab::params::OrbParams::default().atr_window,
        "the raw config still carries ORB's window — the bridge is not a mutation of it"
    );
    assert_eq!(c.assembly_params().atr_window, FROZEN_ATR_WINDOW_SESSIONS);
    assert_ne!(
        c.assembly_params().atr_window,
        c.params.atr_window,
        "the two genuinely differ, so this test is not tautological"
    );

    let catalog = dir.path().join("catalog");
    let instruments = nautilus_ls::ingest::read_all_instruments(&catalog).await.unwrap();
    let all_bars = nautilus_ls::ingest::read_all_bars(&catalog).await.unwrap();
    let bounds = |d: &str, t: chrono::NaiveTime| {
        nautilus_ls::ingest::kst_to_unix_nanos(
            NaiveDate::parse_from_str(d, "%Y%m%d").unwrap(),
            t,
        )
        .unwrap()
        .as_u64()
    };
    let start_ns = bounds(RANGE_START, chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    let end_ns = bounds(RANGE_END, chrono::NaiveTime::from_hms_opt(23, 59, 59).unwrap());

    let derivable = |params: &nautilus_ls_lab::params::OrbParams| -> usize {
        select_daily_sessions(
            &instruments,
            &all_bars,
            params,
            &DecisionSink::new(),
            start_ns,
            end_ns,
            &rank_all,
        )
        .unwrap()
        .sessions
        .iter()
        .filter(|s| s.prior_atr.values().any(Option::is_some))
        .count()
    };

    let bridged = derivable(&c.assembly_params());
    let unbridged = derivable(&c.params);
    assert_eq!(bridged, 20, "ATR(1) is derivable from the second in-range session onward");
    assert_eq!(
        unbridged, 7,
        "ATR(14) leaves only the last 7 of 21 sessions with a derivable ATR — the other 13 \
         would refuse every entry, and the run would still finalize green"
    );
}
