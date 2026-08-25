//! U6 — the typed run observation and the per-session series.
//!
//! The unit-level behaviour (exit attribution, the R25 refusal, the fail-closed
//! placeholder accessor, the empty-run cases) is covered by `artifacts::observation`'s own
//! tests. What can only be checked here is that a REAL finalized run produces one, that it
//! agrees with the sibling artifacts it must be consistent with, and that a refused run
//! leaves none behind.

use std::collections::HashMap;
use std::path::Path;

use chrono::{TimeZone, Utc};
use nautilus_ls_lab::artifacts::observation::RunObservation;
use nautilus_ls_lab::artifacts::{aborted_runs, list_runs, OBSERVATION_FILE};
use nautilus_ls_lab::runner::backtest_daily::{run, run_inner};
use nautilus_ls_lab::strategy::daily::PLACEHOLDER_RANKING_SIGNAL;
use tempfile::tempdir;

use crate::fixture::{build_daily_fixture, cfg, daily_json, write_daily_series};

/// A finalized daily run writes the fifth artifact, and it agrees with the manifest and
/// the performance report it sits beside.
#[tokio::test]
async fn a_finalized_daily_run_writes_a_consistent_observation() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;

    let start = Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap();
    let result = run(cfg(dir.path(), 2), start).await.unwrap();

    // Written to disk, not merely returned in memory.
    let on_disk: RunObservation = serde_json::from_str(
        &std::fs::read_to_string(result.run_dir.join(OBSERVATION_FILE)).unwrap(),
    )
    .unwrap();

    // Everything except the statistic is bit-identical across the round trip.
    //
    // The statistic is compared to a tolerance because `serde_json`'s DEFAULT float parser
    // is not correctly rounded — it can land one ULP off — and the lab does not enable the
    // `float_roundtrip` feature that would fix it. Measured here: `1.0666666666666669` is
    // written and `1.0666666666666667` reads back. That is ~2e-16 relative, some fourteen
    // orders of magnitude below any hurdle this number is ever compared against, so it
    // cannot change a verdict. It is asserted rather than ignored because the alternative
    // is a future reader adding a bit-exact assertion here and getting a mystery.
    assert_eq!(
        RunObservation { observed_net_ror: result.observation.observed_net_ror, ..on_disk.clone() },
        result.observation,
        "every non-float field survives the round trip exactly"
    );
    assert!(
        (on_disk.observed_net_ror - result.observation.observed_net_ror).abs()
            < 1e-12 * result.observation.observed_net_ror.abs().max(1.0),
        "the statistic round-trips to within the JSON float parser's precision: {} vs {}",
        on_disk.observed_net_ror,
        result.observation.observed_net_ror
    );

    // R13: the observation carries its OWN range and fingerprint, so a consumer never has
    // to re-read the manifest to construct a judgment.
    assert_eq!(on_disk.data_range, result.manifest.data_range);
    assert_eq!(on_disk.catalog_fingerprint, result.manifest.catalog_fingerprint);
    assert_eq!(on_disk.run_id, result.run_id);

    // The closure check against performance.json: a dropped or double-counted session
    // shows up here and nowhere else.
    let edge = result.performance.edge_evaluation();
    assert_eq!(
        on_disk.series_risk_capital_total(),
        edge.risk_capital_total.expect("a daily run carries risk on every closed trade"),
        "Σ per-session risk_capital == performance.json's risk_capital_total"
    );
    // Compared in memory, because the on-disk float is one ULP off — see the round-trip
    // note above and `artifacts::observation`'s module docs.
    assert_eq!(result.observation.observed_net_ror, edge.return_on_risk.unwrap());

    // Every position is accounted for: closed on some session, or censored at range end.
    let closes: u32 = on_disk.sessions.iter().map(|s| s.closes).sum();
    assert_eq!(closes, on_disk.closed_positions);
    assert_eq!(
        (on_disk.closed_positions + on_disk.censored_positions) as usize,
        result.performance.trades.len()
    );

    // One row per in-range session, including the leading hold-length that exit
    // attribution necessarily leaves empty (KTD13).
    assert_eq!(on_disk.sessions.len(), result.outcome.selection.sessions.len());
}

/// R26/KTD6: the shipped run carries the placeholder marker, and the marker is enforced
/// rather than advisory — the run is unusable as a judgment.
#[tokio::test]
async fn a_placeholder_signal_run_is_marked_and_yields_no_judgment() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;

    let start = Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap();
    let result = run(cfg(dir.path(), 2), start).await.unwrap();

    assert!(
        result.observation.ranking_signal_is_placeholder,
        "the shipped signal is the placeholder — the signal carrying the hypothesis is \
         turn one's act"
    );
    assert_eq!(result.observation.ranking_signal, PLACEHOLDER_RANKING_SIGNAL.name);

    let err = result.observation.judgment_arguments().unwrap_err();
    assert!(
        err.to_string().contains("PLACEHOLDER"),
        "the only path to the judgment arguments refuses, naming why: {err}"
    );
}

/// A run aborted at the finalize fingerprint re-check leaves no observation anywhere —
/// not in a finalized run dir, and not in staging.
#[tokio::test]
async fn an_aborted_run_leaves_no_observation() {
    let dir = tempdir().unwrap();
    build_daily_fixture(dir.path(), &HashMap::new()).await;
    let catalog = dir.path().join("catalog");

    let mutate = async {
        write_daily_series(
            &catalog,
            "005930.XKRX",
            &[daily_json("20240130", "70000", "70500", "69500", "70000", "999")],
        )
        .await;
    };
    let start = Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap();
    run_inner(cfg(dir.path(), 2), start, mutate).await.unwrap_err();

    assert!(list_runs(dir.path()).is_empty());
    assert!(aborted_runs(dir.path()).is_empty());
    let found: Vec<_> = walk_files(dir.path())
        .into_iter()
        .filter(|p| p.file_name().and_then(|n| n.to_str()) == Some(OBSERVATION_FILE))
        .collect();
    assert!(found.is_empty(), "no observation survives the abort: {found:?}");
}

/// Recursively list every file under `root` — used to prove an artifact exists *nowhere*,
/// which a check against one expected directory cannot do.
fn walk_files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else { return out };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk_files(&p));
        } else {
            out.push(p);
        }
    }
    out
}

/// The frozen pre-registration is byte-identical: the observation is a NEW artifact beside
/// the run, never a field added to a governance file whose content hash is cited by its
/// own loader and by the judgment ledger (KTD8, R15).
#[test]
fn the_frozen_lineage_preregistration_is_untouched() {
    use sha2::{Digest, Sha256};
    let path = nautilus_ls_lab::lineage_prereg::frozen_lineage_prereg_path();
    let bytes = std::fs::read(&path).expect("the frozen artifact is committed");
    let digest = format!("{:x}", Sha256::digest(&bytes));
    assert_eq!(
        digest, "0ecd9d1163075edc28336035f511807e192b5d5c780e09340841ee81794b3dd4",
        "lineage-preregistration.json moved — its content hash is cited, so a new field \
         goes in a new artifact (R15)"
    );
}
