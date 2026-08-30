//! The live-mount universe producer (`lab-mount-universe`).
//!
//! `--mount` re-runs no selection, so this producer's output IS what a live session trades.
//! These tests cover the fail-closed guards on `resolve()` — the ones that decide whether a
//! session runs against the certified head's universe or a silently different one.
//!
//! Its own test binary, deliberately: `head_version_pin()` reads the process-wide
//! `LS_TURN_EXPECT_VERSION`, so sharing a binary with tests that set it would make these
//! order-dependent.

use std::path::Path;

use nautilus_ls_lab::artifacts::manifest::{DataRange, Manifest};
use nautilus_ls_lab::artifacts::{RunSource, MANIFEST_FILE};
use nautilus_ls_lab::params::OrbParams;
use nautilus_ls_lab::runner::mount_universe::{resolve, MountUniverseConfig, TodayOpenSource};
use tempfile::TempDir;

/// Write a finalized run whose manifest is the head the producer will resolve: the running
/// binary's own `strategy_code_hash` (so the pin matches) and a real, non-zero risk size (so
/// the zero-size head guard does not fire first).
fn write_head_run(data_home: &Path, run_id: &str, universe_metadata_hash: Option<&str>) {
    let dir = data_home.join("runs").join(run_id);
    std::fs::create_dir_all(&dir).unwrap();
    let mut params = OrbParams::default();
    params.risk_per_trade_krw = 299_340.0;
    let manifest = Manifest {
        run_id: run_id.to_string(),
        source: RunSource::Backtest,
        strategy_id: "orb".to_string(),
        strategy_version: params.strategy_version,
        params,
        data_range: DataRange { start: "20260601".to_string(), end: "20260630".to_string() },
        catalog_fingerprint: "fp".to_string(),
        universe_hash: "uh".to_string(),
        strategy_code_hash: nautilus_ls_lab::artifacts::manifest::strategy_code_hash(),
        lab_src_fingerprint: None,
        checkpoint_hash: None,
        universe_metadata_hash: universe_metadata_hash.map(str::to_string),
        dispatch: None,
        daily_params: None,
        created_utc: "2026-07-26T00:00:00+00:00".to_string(),
    };
    std::fs::write(dir.join(MANIFEST_FILE), serde_json::to_string(&manifest).unwrap()).unwrap();
}

fn cfg(home: &Path, metadata: Option<&Path>) -> MountUniverseConfig {
    MountUniverseConfig {
        data_home: home.to_path_buf(),
        session_date: chrono::NaiveDate::from_ymd_opt(2026, 7, 27).unwrap(),
        metadata_path: metadata.map(Path::to_path_buf),
        // Pinned, never derived from the clock: the source lives on the config precisely so
        // these stay offline and deterministic no matter what date the suite runs on. Only
        // `config_from_env` consults the wall clock.
        today_open_source: TodayOpenSource::Catalog,
    }
}

/// An absent catalog refuses before anything else — the producer never invents an open.
#[tokio::test]
async fn a_missing_catalog_refuses() {
    let tmp = TempDir::new().unwrap();
    let err = resolve(&cfg(tmp.path(), None)).await.unwrap_err().to_string();
    assert!(err.contains("no catalog at"), "names the missing catalog: {err}");
}

/// The head-fidelity guard: when the head run is metadata-driven, producing a universe
/// WITHOUT that artifact silently drops the tradability gate — every candidate becomes
/// `Untagged` and symbols the certified backtest excluded enter the live universe, with
/// nothing in the emitted file to show it. That must be a refusal, not a warning.
#[tokio::test]
async fn a_metadata_driven_head_refuses_a_universe_built_without_the_artifact() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("catalog")).unwrap();
    write_head_run(tmp.path(), "20260725T000000Z-backtest-orb-v34", Some("abc123"));

    let err = resolve(&cfg(tmp.path(), None)).await.unwrap_err().to_string();
    assert!(err.contains("METADATA-DRIVEN"), "names the cause: {err}");
    assert!(err.contains("abc123"), "names the head's expected artifact hash: {err}");
    assert!(
        err.contains("LS_MOUNT_UNIVERSE_METADATA"),
        "tells the operator which variable to set: {err}"
    );
}

/// The mismatch half of the same guard: a metadata artifact that is not the one the head was
/// built from re-tiers symbols, so it is refused rather than silently applied.
#[tokio::test]
async fn a_metadata_artifact_that_is_not_the_heads_is_refused() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("catalog")).unwrap();
    write_head_run(tmp.path(), "20260725T000000Z-backtest-orb-v34", Some("expected-hash"));

    // A COMPLETE, well-formed artifact — so the refusal below is the hash binding doing its
    // job, not a parse error incidentally covering for it.
    let art = tmp.path().join("universe-metadata.json");
    std::fs::write(&art, valid_artifact_json()).unwrap();

    let err = resolve(&cfg(tmp.path(), Some(&art))).await.unwrap_err().to_string();
    assert!(
        err.contains("hash mismatch"),
        "the refusal is the head-identity binding, not an incidental parse failure: {err}"
    );
    assert!(err.contains("expected-hash"), "names the head's hash: {err}");
}

/// AE13/R15. Neither an artifact nor a head metadata hash: this arm used to
/// PROCEED, silently — absent metadata maps every candidate to
/// `CandidateMeta::Untagged`, the tradability gate disappears, and the live
/// session trades symbols on no eligibility evidence at all, with nothing in the
/// emitted artifact to show it. The module's own comment stated that harm while
/// the code did it anyway. It must refuse.
///
/// This test previously asserted the opposite (that the guard must NOT fire
/// here); the contract moved, so the assertion moved with it.
#[tokio::test]
async fn neither_an_artifact_nor_a_head_metadata_hash_is_refused() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("catalog")).unwrap();
    write_head_run(tmp.path(), "20260725T000000Z-backtest-orb-v34", None);

    let err = resolve(&cfg(tmp.path(), None)).await.unwrap_err().to_string();
    assert!(
        err.contains("mount-universe refused"),
        "the bare-Option arm must refuse, not proceed: {err}"
    );
    assert!(
        err.contains("tradability gate"),
        "the refusal names the behaviour that would be dropped: {err}"
    );
    assert!(
        err.contains("LS_MOUNT_UNIVERSE_METADATA"),
        "and the knob that fixes it: {err}"
    );
    // The refusal is its OWN case, not the metadata-driven-head guard borrowed:
    // that one fires when the head HAS a hash, and this head has none.
    assert!(
        !err.contains("METADATA-DRIVEN"),
        "a head with no hash is not a metadata-driven head: {err}"
    );
}

/// R15 step 2 — the narrower case must stay DISTINGUISHABLE from the refusal.
/// An artifact supplied against a head carrying no `universe_metadata_hash`
/// applies the gate where the head's did not: narrower than a mismatch, so it
/// warns and proceeds. Hardening this arm would have closed nothing, which is
/// why KTD14 points at the silent one instead.
#[tokio::test]
async fn an_artifact_against_an_untagged_head_still_proceeds_past_the_binding() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("catalog")).unwrap();
    write_head_run(tmp.path(), "20260725T000000Z-backtest-orb-v34", None);
    let art = tmp.path().join("universe-metadata.json");
    std::fs::write(&art, valid_artifact_json()).unwrap();

    let err = resolve(&cfg(tmp.path(), Some(&art))).await.unwrap_err().to_string();
    assert!(
        !err.contains("mount-universe refused"),
        "supplying the artifact clears the binding; the run fails later on its empty \
         catalog, which is a different question: {err}"
    );
}

/// A complete, schema-valid `UniverseMetadata` artifact with no records. Its content hash is
/// whatever it is — the point is only that it is never the head's `"expected-hash"`.
fn valid_artifact_json() -> String {
    serde_json::json!({
        "provenance": {
            "captured_at": "2026-07-23T01:33:35.837612+00:00",
            "session_date": "20260723",
            "source_trs": ["t8430"],
            "instrument_type_filter": "equities-only; ETF/ETN rows dropped",
            "tier_boundary_rule": "pre-registered cap-tier boundaries",
            "cap_cutoffs": []
        },
        "records": [{
            "shcode": "005930",
            "market_class": "kospi",
            "market_cap": { "resolution": "unavailable" },
            "cap_tier": "below_board",
            "turnover": { "resolution": "unavailable" },
            "liquidity_tier": "unknown",
            "index_membership": { "resolution": "proxy", "value": "not_member" },
            "has_derivative": { "resolution": "value", "value": false },
            "designation": null,
            "tradable": true
        }]
    })
    .to_string()
}
