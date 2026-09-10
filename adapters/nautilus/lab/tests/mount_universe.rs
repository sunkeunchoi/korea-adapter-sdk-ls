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

// ===========================================================================
// `--daily` — the rehearsal universe producer (plan 2026-09-08-1215 U10, R24)
// ===========================================================================
//
// These run against a real on-disk catalog (instrument masters via a wiremock gateway,
// daily bars via the ingest builders) so the assembly path exercised is the backtest's own.

mod daily {
    use std::collections::HashMap;
    use std::path::Path;

    use ls_sdk::LsSdk;
    use ls_sdk_test_support::{mock_config, mount_token};
    use nautilus_ls::ingest::{
        build_daily_bar, build_minute_bar, write_bars, write_instruments, BarKind,
    };
    use nautilus_ls::instruments::{InstrumentDomain, InstrumentProvider};
    use nautilus_ls_lab::params_daily::{DailyParams, RankingSignalKind};
    use nautilus_ls_lab::runner::mount_universe::{
        daily_to_json, resolve_daily, DailyUniverseConfig, DailyUniverseFile, DailyUniverseRow,
    };
    use nautilus_model::data::Bar;
    use nautilus_model::identifiers::InstrumentId;
    use serde_json::json;
    use tempfile::TempDir;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// The three fixture symbols, in the order their masters are served.
    const SYMBOLS: [(&str, &str, &str); 3] = [
        ("삼성전자", "005930", "KR7005930003"),
        ("에스케이하이닉스", "000660", "KR7000660001"),
        ("NAVER", "035420", "KR7035420009"),
    ];

    /// The session the universe is resolved FOR. Its own bar is never in the catalog —
    /// exactly a rehearsal morning.
    const SESSION: &str = "2024-02-01";

    /// Twenty consecutive weekdays strictly before `SESSION`, oldest first.
    const PRIOR_DAYS: [&str; 20] = [
        "20240104", "20240105", "20240108", "20240109", "20240110", "20240111", "20240112",
        "20240115", "20240116", "20240117", "20240118", "20240119", "20240122", "20240123",
        "20240124", "20240125", "20240126", "20240129", "20240130", "20240131",
    ];

    fn json_response(body: serde_json::Value) -> ResponseTemplate {
        ResponseTemplate::new(200)
            .set_body_string(body.to_string())
            .insert_header("content-type", "application/json")
    }

    async fn write_masters(catalog: &Path) {
        let server = MockServer::start().await;
        mount_token(&server).await;
        let t8430 = json!({ "rsp_cd": "00000", "t8430OutBlock": SYMBOLS.iter().map(|(h, s, e)| json!({
            "hname": h, "shcode": s, "expcode": e, "etfgubun": "0", "uplmtprice": "82000",
            "dnlmtprice": "44000", "jnilclose": "63000", "memedan": "1", "recprice": "63000",
            "gubun": "1" })).collect::<Vec<_>>() });
        let t9945 = json!({ "rsp_cd": "00000", "t9945OutBlock": SYMBOLS.iter().map(|(h, s, e)| json!({
            "hname": h, "shcode": s, "expcode": e, "etfchk": "0", "nxt_chk": "1", "filler": "" }))
            .collect::<Vec<_>>() });
        for (p, tr, body) in [("/stock/etc", "t8430", t8430), ("/stock/market-data", "t9945", t9945)] {
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
        write_instruments(catalog, provider.all_any()).await.unwrap();
    }

    fn daily_json(date: &str, o: i64, h: i64, l: i64, c: i64, v: i64) -> serde_json::Value {
        json!({ "date": date, "open": o.to_string(), "high": h.to_string(), "low": l.to_string(),
            "close": c.to_string(), "jdiff_vol": v.to_string(), "value": "0", "jongchk": "0",
            "rate": "0", "pricechk": "0", "ratevalue": "0", "sign": "0" })
    }

    async fn write_daily(catalog: &Path, shcode: &str, rows: &[serde_json::Value]) {
        let bt = BarKind::Daily.bar_type(InstrumentId::from(format!("{shcode}.XKRX").as_str())).unwrap();
        let bars: Vec<Bar> = rows
            .iter()
            .map(|r| build_daily_bar(bt, &serde_json::from_value(r.clone()).unwrap()).unwrap().unwrap())
            .collect();
        write_bars(catalog, bars).await.unwrap();
    }

    async fn write_minute(catalog: &Path, shcode: &str, date: &str) {
        let bt = BarKind::Minute(1).bar_type(InstrumentId::from(format!("{shcode}.XKRX").as_str())).unwrap();
        let rows = [("090000", 63_000), ("090100", 63_100)];
        let bars: Vec<Bar> = rows
            .iter()
            .map(|(t, px)| {
                let r = json!({ "date": date, "time": t, "open": px.to_string(), "high": (px + 100).to_string(),
                    "low": (px - 100).to_string(), "close": px.to_string(), "jdiff_vol": "1000",
                    "value": "0", "jongchk": "0", "rate": "0", "sign": "0" });
                build_minute_bar(bt, &serde_json::from_value(r).unwrap()).unwrap().unwrap()
            })
            .collect();
        write_bars(catalog, bars).await.unwrap();
    }

    /// A flat series over the last `n` prior days: constant close `base`, high `+500`,
    /// low `-500`, constant volume `vol`. The final bar's true range (prior close = the
    /// same `base`) is therefore `high - low = 1000` — the ATR(1) every row should carry.
    fn flat_series(n: usize, base: i64, vol: i64) -> Vec<serde_json::Value> {
        PRIOR_DAYS[PRIOR_DAYS.len() - n..]
            .iter()
            .map(|d| daily_json(d, base, base + 500, base - 500, base, vol))
            .collect()
    }

    /// A complete artifact for the three symbols. `designated` symbols carry a halt
    /// designation (and therefore `tradable: false`, the value `validate()` checks).
    fn artifact_json(designated: &[&str]) -> String {
        let records: Vec<serde_json::Value> = SYMBOLS
            .iter()
            .map(|(_, s, _)| {
                let d = designated.contains(s);
                json!({
                    "shcode": s, "market_class": "kospi",
                    "market_cap": { "resolution": "unavailable" }, "cap_tier": "below_board",
                    "turnover": { "resolution": "unavailable" }, "liquidity_tier": "unknown",
                    "index_membership": { "resolution": "proxy", "value": "not_member" },
                    "has_derivative": { "resolution": "value", "value": false },
                    "designation": if d { json!({ "kind": "halt", "source_tr": "t1405" }) } else { json!(null) },
                    "tradable": !d
                })
            })
            .collect();
        json!({
            "provenance": {
                "captured_at": "2024-01-31T07:00:00+00:00", "session_date": "20240131",
                "source_trs": ["t8430"], "instrument_type_filter": "equities-only; ETF/ETN rows dropped",
                "tier_boundary_rule": "pre-registered cap-tier boundaries", "cap_cutoffs": []
            },
            "records": records
        })
        .to_string()
    }

    fn cfg(home: &Path, artifact: &Path, signal: RankingSignalKind) -> DailyUniverseConfig {
        DailyUniverseConfig {
            data_home: home.to_path_buf(),
            session_date: chrono::NaiveDate::parse_from_str(SESSION, "%Y-%m-%d").unwrap(),
            metadata_path: artifact.to_path_buf(),
            daily_params: DailyParams { ranking_signal: signal, ..DailyParams::default() },
        }
    }

    /// Three symbols with full history and distinct turnover; `designated` names which
    /// the artifact marks.
    async fn three_symbol_home(designated: &[&str]) -> (TempDir, std::path::PathBuf) {
        let tmp = TempDir::new().unwrap();
        let catalog = tmp.path().join("catalog");
        std::fs::create_dir_all(&catalog).unwrap();
        write_masters(&catalog).await;
        // turnover = close × volume: 000660 > 035420 > 005930 by construction.
        write_daily(&catalog, "005930", &flat_series(20, 70_000, 1_000)).await;
        write_daily(&catalog, "000660", &flat_series(20, 130_000, 5_000)).await;
        write_daily(&catalog, "035420", &flat_series(20, 200_000, 2_000)).await;
        let art = tmp.path().join("universe-metadata.json");
        std::fs::write(&art, artifact_json(designated)).unwrap();
        (tmp, art)
    }

    fn by_shcode(file: &DailyUniverseFile) -> HashMap<&str, &DailyUniverseRow> {
        file.rows.iter().map(|r| (r.shcode.as_str(), r)).collect()
    }

    /// Scenario 1: rank is the signal's descending order and ATR(1) is the prior bar's
    /// true range — derived from bars strictly before the session, whose own bar is absent.
    #[tokio::test]
    async fn ranks_descend_by_signal_and_atr1_is_the_prior_bars_true_range() {
        let (tmp, art) = three_symbol_home(&[]).await;
        let file = resolve_daily(&cfg(tmp.path(), &art, RankingSignalKind::Placeholder)).await.unwrap();

        assert_eq!(file.session_date, SESSION);
        assert_eq!(file.ranking_signal, "prior_turnover_desc");
        assert!(file.ranking_signal_is_placeholder);
        assert_eq!(file.warmup_bars, 1);
        assert!(!file.universe_metadata_hash.is_empty());
        let order: Vec<&str> = file.rows.iter().map(|r| r.shcode.as_str()).collect();
        assert_eq!(order, ["000660", "035420", "005930"], "turnover descending");
        assert_eq!(file.rows.iter().map(|r| r.rank).collect::<Vec<_>>(), [0, 1, 2]);
        for r in &file.rows {
            assert!((r.prior_atr1 - 1_000.0).abs() < 1e-9, "{}: ATR(1) = high-low of the prior bar", r.shcode);
            assert!(r.tradable, "{}: no designation → tradable", r.shcode);
        }
        let rows = by_shcode(&file);
        assert_eq!(rows["005930"].prior_close, 70_000);
        assert_eq!(rows["000660"].signal_value, Some(130_000.0 * 5_000.0), "turnover IS the score");
    }

    /// Scenario 2: a symbol with fewer prior bars than the signal's warmup is absent from
    /// the file — not emitted unscored, not defaulted (the prior-ATR trap, applied to the
    /// signal). Momentum12x1 needs 13; give one symbol 5.
    #[tokio::test]
    async fn a_symbol_short_of_the_signals_warmup_is_dropped() {
        let tmp = TempDir::new().unwrap();
        let catalog = tmp.path().join("catalog");
        std::fs::create_dir_all(&catalog).unwrap();
        write_masters(&catalog).await;
        write_daily(&catalog, "005930", &flat_series(20, 70_000, 1_000)).await;
        write_daily(&catalog, "000660", &flat_series(5, 130_000, 5_000)).await;
        write_daily(&catalog, "035420", &flat_series(13, 200_000, 2_000)).await;
        let art = tmp.path().join("universe-metadata.json");
        std::fs::write(&art, artifact_json(&[])).unwrap();

        let file = resolve_daily(&cfg(tmp.path(), &art, RankingSignalKind::Momentum12x1)).await.unwrap();
        assert_eq!(file.warmup_bars, 13);
        let order: Vec<&str> = file.rows.iter().map(|r| r.shcode.as_str()).collect();
        assert_eq!(order.len(), 2, "the 5-bar symbol is gone: {order:?}");
        assert!(!order.contains(&"000660"));
        assert!(order.contains(&"035420"), "exactly 13 bars is enough");
        for r in &file.rows {
            assert_eq!(r.signal_value, None, "the momentum score is not exposed (daily.rs is closed)");
        }
    }

    /// Scenario 3: a designated symbol STAYS in the file as `tradable: false` — the runner
    /// must still be able to exit it — and keeps its rank.
    #[tokio::test]
    async fn a_designated_symbol_is_kept_as_not_tradable() {
        let (tmp, art) = three_symbol_home(&["035420"]).await;
        let file = resolve_daily(&cfg(tmp.path(), &art, RankingSignalKind::Placeholder)).await.unwrap();
        let rows = by_shcode(&file);
        assert_eq!(file.rows.len(), 3, "kept, not dropped");
        assert!(!rows["035420"].tradable);
        assert_eq!(rows["035420"].rank, 1, "the gate does not re-rank");
        assert!(rows["000660"].tradable && rows["005930"].tradable);
    }

    /// Scenario 4: an ORB home — minute bars only — is refused by name, not resolved to an
    /// empty file or a stale universe.
    #[tokio::test]
    async fn an_orb_minute_only_home_is_refused_for_lacking_daily_bars() {
        let tmp = TempDir::new().unwrap();
        let catalog = tmp.path().join("catalog");
        std::fs::create_dir_all(&catalog).unwrap();
        write_masters(&catalog).await;
        write_minute(&catalog, "005930", "20240131").await;
        let art = tmp.path().join("universe-metadata.json");
        std::fs::write(&art, artifact_json(&[])).unwrap();

        let err = resolve_daily(&cfg(tmp.path(), &art, RankingSignalKind::Placeholder))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("NO daily (1-DAY) bar"), "names the cause: {err}");
        assert!(err.contains("2 minute bar(s)"), "and what it found instead: {err}");
        assert!(err.contains("KTD10"), "and which home to point at: {err}");
    }

    /// Scenario 5 (the U10 verification contract): the same catalog and date twice give
    /// byte-identical output.
    #[tokio::test]
    async fn the_same_inputs_produce_byte_identical_output() {
        let (tmp, art) = three_symbol_home(&["000660"]).await;
        let c = cfg(tmp.path(), &art, RankingSignalKind::Placeholder);
        let a = daily_to_json(&resolve_daily(&c).await.unwrap()).unwrap();
        let b = daily_to_json(&resolve_daily(&c).await.unwrap()).unwrap();
        assert_eq!(a, b);
        assert!(a.contains("\"universe_metadata_hash\""), "the binding travels in the file");
    }

    /// The artifact is not optional on this path: a missing one refuses before any
    /// catalog read decides anything.
    #[tokio::test]
    async fn a_missing_artifact_refuses() {
        let (tmp, _art) = three_symbol_home(&[]).await;
        let missing = tmp.path().join("nope.json");
        let err = resolve_daily(&cfg(tmp.path(), &missing, RankingSignalKind::Placeholder))
            .await
            .unwrap_err()
            .to_string();
        assert!(!err.is_empty());
        assert!(!err.contains("no symbol resolved"), "refused at the artifact, not after: {err}");
    }
}
