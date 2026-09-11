//! U4's code half (plan 2026-09-08-1215): the ranking-signal and transaction-cost knobs
//! on the daily CLI, and the warmup marking the re-check requires of a signal that needs
//! prior bars.
//!
//! The judgment home's catalog floor IS the specification window's first session, so a
//! lookback signal cannot score the window's opening sessions and warms up inside it.
//! Those sessions must be RECORDED on the observation — `lineage recheck` refuses an
//! unmarked no-entry prefix rather than measure participation over sessions the signal
//! could not score, and the re-check is spent exactly once.

use std::collections::HashMap;

use chrono::{NaiveDate, TimeZone};
use nautilus_ls_lab::params_daily::RankingSignalKind;
use nautilus_ls_lab::runner::backtest_daily::{
    apply_signal_and_cost_env, run, COST_CONFIG_ENV, SIGNAL_ENV,
};
use tempfile::tempdir;

use super::fixture::{
    build_daily_fixture, cfg, kst_date, IN_RANGE_SESSIONS, RANGE_START, SESSION_DAYS,
};

fn ymd(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y%m%d").unwrap()
}

/// A distinct `started` per run in one home: the registry is append-only and keys the
/// run id off this instant, so two runs at the same second collide.
fn started(minute: u32) -> chrono::DateTime<chrono::Utc> {
    chrono::Utc.with_ymd_and_hms(2024, 2, 1, 0, minute, 0).unwrap()
}

fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |key| pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| (*v).to_string())
}

/// `LS_BTD_SIGNAL` names a signal by its manifest spelling; a misspelling refuses rather
/// than silently running something else.
///
/// Unset now means the **frozen** signal, not the placeholder — U4 closed the selection
/// window, so there is no longer a pre-freeze default to fall back to. `DailyParams::frozen`
/// is where that comes from, and `resolve_ranking_signal` resolves an absent mount-universe
/// override the same way, so the two entry points agree on what "unset" means.
#[test]
fn the_signal_knob_selects_by_manifest_spelling_and_refuses_garbage() {
    let home = tempdir().unwrap();
    let mut c = cfg(home.path(), 2);
    let notes = apply_signal_and_cost_env(&mut c, lookup(&[])).unwrap();
    assert!(notes.is_empty());
    assert_eq!(c.daily.ranking_signal, RankingSignalKind::Momentum12x1);

    let notes = apply_signal_and_cost_env(&mut c, lookup(&[(SIGNAL_ENV, "momentum12x1")])).unwrap();
    assert_eq!(c.daily.ranking_signal, RankingSignalKind::Momentum12x1);
    assert!(notes[0].contains("momentum_12x1") && notes[0].contains("warmup 13"), "{notes:?}");

    let err = apply_signal_and_cost_env(&mut c, lookup(&[(SIGNAL_ENV, "Momentum12x1")]))
        .unwrap_err()
        .to_string();
    assert!(err.contains(SIGNAL_ENV) && err.contains("momentum12x1"), "{err}");
    assert_eq!(c.daily.ranking_signal, RankingSignalKind::Momentum12x1, "a refusal changes nothing");
}

/// The cost loader is the ORB path's, by name and by artifact: a rate-zero artifact leaves
/// the performance bytes identical to the unarmed run, and the committed rates change them
/// — proof the rates reach `finalize_daily_run`'s cost model, not just the config.
#[tokio::test]
async fn the_cost_knob_reaches_performance_and_rate_zero_is_byte_identical() {
    let home = tempdir().unwrap();
    build_daily_fixture(home.path(), &HashMap::new()).await;
    let zero = home.path().join("zero-costs.json");
    std::fs::write(
        &zero,
        r#"{"schema_version":1,"commission_rate_per_side":0.0,"sell_tax_rate":0.0,"effective":"2026-01-01","retrieved":"2026-07-31","sources":[],"notes":[]}"#,
    )
    .unwrap();
    let committed = concat!(env!("CARGO_MANIFEST_DIR"), "/config/transaction-costs.json");

    let unarmed = run(cfg(home.path(), 2), started(0)).await.unwrap();

    let mut c = cfg(home.path(), 2);
    let notes = apply_signal_and_cost_env(&mut c, lookup(&[(COST_CONFIG_ENV, zero.to_str().unwrap())])).unwrap();
    assert!(notes[0].contains("transaction costs armed"), "{notes:?}");
    let rate_zero = run(c, started(1)).await.unwrap();
    assert_eq!(
        serde_json::to_vec(&unarmed.performance).unwrap(),
        serde_json::to_vec(&rate_zero.performance).unwrap(),
        "a rate-zero artifact is the zero-cost reproduction path"
    );

    let mut c = cfg(home.path(), 2);
    apply_signal_and_cost_env(&mut c, lookup(&[(COST_CONFIG_ENV, committed)])).unwrap();
    assert_eq!(c.params.cost_sell_tax_rate, 0.0020);
    assert_eq!(c.params.cost_commission_rate_per_side, 0.00015);
    let armed = run(c, started(2)).await.unwrap();
    assert!(!unarmed.performance.trades.is_empty(), "the fixture trades");
    assert_ne!(
        serde_json::to_vec(&unarmed.performance).unwrap(),
        serde_json::to_vec(&armed.performance).unwrap(),
        "the committed rates must change the performance artifact"
    );
    let net = |p: &nautilus_ls_lab::artifacts::performance::PerformanceReport| {
        p.trades.iter().map(|t| t.realized_pnl).sum::<f64>()
    };
    assert!(net(&armed.performance) < net(&unarmed.performance), "costs reduce net P&L");
    assert_eq!(armed.manifest.params.cost_sell_tax_rate, 0.0020, "the manifest records the rates");
}

/// The warmup contract. The fixture holds exactly ONE bar before the window, so a
/// 13-bar signal cannot score in-range session k until k >= 13: the first twelve in-range
/// sessions are warmup, recorded on the observation as a leading prefix; no trade opens
/// inside it; `data_range` still starts at `LS_BTD_SDATE`. The placeholder marks nothing.
#[tokio::test]
async fn a_lookback_signal_marks_its_unscoreable_leading_sessions_as_warmup() {
    let home = tempdir().unwrap();
    // A crash three sessions after the first scoreable session stops 005930 out: the
    // frozen 16-session hold cannot expire inside a 21-session window from session 13,
    // and the verdict statistic (and therefore the observation) needs a closed trade.
    let crash = HashMap::from([("005930.XKRX", HashMap::from([(16usize, 40_000i64)]))]);
    build_daily_fixture(home.path(), &crash).await;

    let mut c = cfg(home.path(), 2);
    c.daily.ranking_signal = RankingSignalKind::Momentum12x1;
    let momentum = run(c, started(0)).await.unwrap();

    let expected: Vec<NaiveDate> = SESSION_DAYS[1..=12].iter().map(|d| ymd(d)).collect();
    assert_eq!(momentum.observation.warmup_sessions, expected, "the first 12 in-range sessions");
    let first_scoreable = ymd(SESSION_DAYS[13]);
    assert!(
        !momentum.performance.trades.is_empty(),
        "the signal scores from session 13 and the fixture enters"
    );
    for t in &momentum.performance.trades {
        assert!(
            kst_date(t.ts_opened) >= first_scoreable,
            "no entry inside the marked warmup: opened {}",
            kst_date(t.ts_opened)
        );
    }
    assert_eq!(momentum.manifest.data_range.start, RANGE_START, "data_range stays the window");
    assert_eq!(
        momentum.observation.sessions.len(),
        IN_RANGE_SESSIONS,
        "every in-range session is still a row"
    );
    assert!(!momentum.observation.ranking_signal_is_placeholder);

    // The pre-freeze contrast — a one-bar signal marks no warmup and enters before the
    // lookback signal could — can no longer be RUN: U4 froze the signal, so a placeholder
    // run is refused at manifest construction. That refusal is the stronger property, and
    // it is what this arm now pins. (`Placeholder.warmup_bars() == 1` is still asserted
    // directly, in `params_daily`'s unit tests.)
    let mut c = cfg(home.path(), 2);
    c.daily.ranking_signal = RankingSignalKind::Placeholder;
    let err = run(c, started(1)).await.expect_err("a placeholder run is not admissible").to_string();
    assert!(err.contains("ranking_signal"), "{err}");
    assert!(err.contains("momentum_12x1"), "and names the frozen signal: {err}");
}

/// Gate 2's instrument: `lab-research catalog fingerprint` reproduces a run's
/// `manifest.catalog_fingerprint` from the bars alone — same bars, same bounds, same hash,
/// no strategy — so the holdout pin can be authored without a holdout run on disk. A
/// different range is a different value, because the fingerprint is range-scoped.
#[tokio::test]
async fn the_fingerprint_verb_reproduces_a_runs_catalog_fingerprint_without_running_it() {
    use nautilus_ls_lab::artifacts::manifest::DataRange;
    use nautilus_ls_lab::runner::research::{catalog_fingerprint, FingerprintConfig};
    use super::fixture::RANGE_END;

    let home = tempdir().unwrap();
    build_daily_fixture(home.path(), &HashMap::new()).await;
    let ran = run(cfg(home.path(), 2), started(0)).await.unwrap();

    let same = catalog_fingerprint(&FingerprintConfig {
        data_home: home.path().to_path_buf(),
        range: DataRange { start: RANGE_START.into(), end: RANGE_END.into() },
    })
    .await
    .unwrap();
    assert_eq!(same.fingerprint, ran.manifest.catalog_fingerprint, "same bars, same bounds, same hash");
    assert_eq!(same.series_in_range, 2, "two symbols' daily series");
    assert_eq!(
        same.bars_in_range,
        IN_RANGE_SESSIONS * 2,
        "{IN_RANGE_SESSIONS} in-range sessions x 2 symbols"
    );
    assert!(same.lines[0].contains(&format!("fingerprint={}", ran.manifest.catalog_fingerprint)));

    let narrower = catalog_fingerprint(&FingerprintConfig {
        data_home: home.path().to_path_buf(),
        range: DataRange { start: RANGE_START.into(), end: SESSION_DAYS[10].into() },
    })
    .await
    .unwrap();
    assert_ne!(narrower.fingerprint, same.fingerprint, "range-scoped");
    assert_eq!(narrower.bars_in_range, 20);

    let err = catalog_fingerprint(&FingerprintConfig {
        data_home: home.path().to_path_buf(),
        range: DataRange { start: RANGE_END.into(), end: RANGE_START.into() },
    })
    .await
    .unwrap_err()
    .to_string();
    assert!(err.contains("is after"), "an inverted range refuses: {err}");
}
