//! U3 acceptance: synthetic closed-trade ledgers and isolated judgment ledgers.
//! No catalog ingestion, sockets, credentials, strategy source reads or frozen writes.

use std::path::{Path, PathBuf};
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use nautilus_ls_calendar::{compute_artifact_id, compute_calendar_id, schema::Snapshot};
use nautilus_ls_lab::artifacts::manifest::{DataRange, Manifest};
use nautilus_ls_lab::artifacts::observation::{RunObservation, SessionRow};
use nautilus_ls_lab::artifacts::performance::{PerformanceReport, TradeRecord};
use nautilus_ls_lab::lineage_prereg::{self, JudgmentAttempt, JudgmentLedger};
use nautilus_ls_lab::params::OrbParams;
use nautilus_ls_lab::params_daily::{DailyParams, RankingSignalKind};
use nautilus_ls_lab::runner::lineage::{self, LineageExit, RecheckReport};
use nautilus_ls_lab::stats;
use serde_json::json;

fn write(path: &Path, value: &impl serde::Serialize) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn date(s: &str) -> NaiveDate { NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap() }
fn stamp(d: NaiveDate) -> u64 {
    Utc.from_utc_datetime(&d.and_hms_opt(3, 0, 0).unwrap()).timestamp_nanos_opt().unwrap() as u64
}

pub(crate) struct Fixture {
    _temp: tempfile::TempDir,
    pub(crate) spec: PathBuf,
    pub(crate) hold: PathBuf,
    pub(crate) calendar: PathBuf,
    pub(crate) catalog: PathBuf,
    pub(crate) ledger: JudgmentLedger,
}

impl Fixture {
    pub(crate) fn new(icc: f64, warmup: usize) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let spec = root.join("spec");
        let hold = root.join("hold");
        let calendar = root.join("calendar.json");
        let catalog = root.join("catalog-record.json");
        let ledger = JudgmentLedger::new(root.join("ledger.jsonl"));
        let start = date("2017-01-02");
        let sessions: Vec<_> = (0..64 + warmup).map(|n| start + Duration::days(n as i64)).collect();
        let range = DataRange { start: start.format("%Y%m%d").to_string(),
            end: sessions.last().unwrap().format("%Y%m%d").to_string() };
        let trades = synthetic(&sessions[warmup..], icc);
        stage(&spec, "spec", range, &sessions, trades, &sessions[..warmup]);
        calendar_fixture(&calendar, &sessions);
        stage(&hold, "hold", DataRange { start: "20200102".into(), end: "20260520".into() },
            &[date("2020-01-02"), date("2020-01-03")], synthetic(&[date("2020-01-02"), date("2020-01-03")], 0.30), &[]);
        // The real committed catalog record currently lacks this field. This
        // explicit synthetic pin exercises judgment without inventing a production pin.
        write(&catalog, &json!({"catalog_fingerprint": "fixture-catalog"}));
        Self { _temp: temp, spec, hold, calendar, catalog, ledger }
    }

    fn admit(&self) -> RecheckReport {
        let out = lineage::recheck(&self.spec, &self.calendar).unwrap();
        assert_eq!(out.exit, LineageExit::Clear, "{:?}", out.lines);
        serde_json::from_slice(&std::fs::read(self.spec.join(lineage::RECHECK_FILE)).unwrap()).unwrap()
    }

    fn judge(&self) -> lineage::LineageOutcome {
        lineage::judge(&self.hold, &self.spec, &self.ledger, &self.catalog).unwrap()
    }

    pub(crate) fn claim(&self) -> JudgmentAttempt {
        JudgmentAttempt {
            schema_version: 1, run_id: "hold".into(), catalog_fingerprint: "fixture-catalog".into(),
            strategy_code_hash: Some(lineage::PINNED_DAILY_CODE_HASH.into()),
            params_hash: Some(lineage::PINNED_DAILY_PARAMS_HASH.into()),
            prereg_content_hash: lineage_prereg::load(&lineage_prereg::frozen_lineage_prereg_path()).unwrap().content_hash,
            claimed_utc: "2026-08-15T00:00:00+00:00".into(), observed_net_ror: None, cleared: None,
        }
    }
}

fn synthetic(sessions: &[NaiveDate], rho: f64) -> Vec<TradeRecord> {
    // Balanced ANOVA: within-session offsets sum to zero. Solve MSB/MSW
    // algebraically for rho, then normalize total sample sd to the frozen ORB sd.
    let k = sessions.len() as f64;
    let a = ((k - 1.0) / k / 7.0 * (1.0 + 7.0 * rho) / (1.0 - rho)).sqrt();
    let raw: Vec<_> = sessions.iter().enumerate().flat_map(|(i, _)|
        (0..8).map(move |j| (if i % 2 == 0 { a } else { -a }) + if j < 4 { 1.0 } else { -1.0 })).collect();
    let scale = 0.6415229271189272 / stats::sample_sd(&raw).unwrap();
    sessions.iter().enumerate().flat_map(|(i, d)| {
        let raw = &raw;
        (0..8).map(move |j| {
            let net_r = 0.08 + raw[i * 8 + j] * scale;
            TradeRecord {
                symbol: format!("{j:06}.XKRX"), entry_side: "BUY".into(), quantity: 1.0,
                avg_px_open: 100.0, avg_px_close: Some(100.0 + net_r),
                realized_pnl: net_r * 100.0, risk_capital: Some(100.0), realized_r: Some(net_r),
                ts_opened: stamp(*d), ts_closed: Some(stamp(*d)), fills: vec![],
            }
        })
    }).collect()
}

fn stage(dir: &Path, id: &str, range: DataRange, sessions: &[NaiveDate], trades: Vec<TradeRecord>, warmup: &[NaiveDate]) {
    std::fs::create_dir_all(dir).unwrap();
    let manifest: Manifest = serde_json::from_value(json!({
        "run_id": id, "source": "backtest", "strategy_id": "daily-ms", "strategy_version": 0,
        "params": OrbParams::default(), "daily_params": DailyParams::default(),
        "data_range": range, "catalog_fingerprint": "fixture-catalog", "universe_hash": "fixture-universe",
        "strategy_code_hash": lineage::PINNED_DAILY_CODE_HASH, "created_utc": "2026-08-15T00:00:00Z"
    })).unwrap();
    let rows = sessions.iter().map(|d| {
        let closed: Vec<_> = trades.iter().filter(|t| t.ts_closed == Some(stamp(*d))).collect();
        SessionRow { session_date: *d, realized_pnl: closed.iter().map(|t| t.realized_pnl).sum(),
            risk_capital: closed.iter().map(|t| t.risk_capital.unwrap()).sum(),
            closes: closed.len() as u32, entries: trades.iter().filter(|t| t.ts_opened == stamp(*d)).count() as u32 }
    }).collect();
    let observation = RunObservation {
        schema_version: 1, run_id: id.into(), data_range: range, catalog_fingerprint: "fixture-catalog".into(),
        observed_net_ror: trades.iter().map(|t| t.realized_pnl).sum::<f64>() / (trades.len() as f64 * 100.0),
        ranking_signal: "fixture-signal".into(), ranking_signal_is_placeholder: false,
        censored_positions: 0, closed_positions: trades.len() as u32, sessions: rows,
        warmup_sessions: warmup.to_vec(),
    };
    write(&dir.join("manifest.json"), &manifest);
    write(&dir.join("observation.json"), &observation);
    write(&dir.join("performance.json"), &PerformanceReport::assemble(trades, 100_000_000.0));
}

fn calendar_fixture(path: &Path, sessions: &[NaiveDate]) {
    let start = sessions[0];
    let end = *sessions.last().unwrap();
    let mut snapshot: Snapshot = serde_json::from_value(json!({
        "schema_version": "1.0.0", "artifact_id": "", "calendar_id": "", "predecessor_artifact_id": null,
        "scope": {"calendar_name": "SYNTHETIC", "venue": "XKRX", "instrument_class": "domestic-equity",
            "timezone": "Asia/Seoul", "synthetic": true},
        "authorization": {"authorized": true, "authority": "SYNTHETIC", "granted_at": "2000-01-01T00:00:00Z",
            "expires_at": "2099-01-01T00:00:00Z", "terminated_at": null},
        "coverage": {"materialized_from": start, "materialized_through": end,
            "retrospectively_checked_through": end, "scheduled_closure_evaluated_through": end,
            "source_availability": []},
        "freshness": {"evidence_refreshed_at": "2026-08-15T00:00:00Z", "holiday_facts_checked_at": null,
            "full_history_reconciled_at": null, "forward_readiness_through": null, "last_incremental_at": null},
        "sources": [], "evidence": [], "alerts": [],
        "rows": sessions.iter().map(|d| json!({"date": d, "status": "trading_session",
            "decisive_evidence": [], "conflicting_evidence": [], "alerts": []})).collect::<Vec<_>>()
    })).unwrap();
    snapshot.calendar_id = compute_calendar_id(&snapshot);
    snapshot.artifact_id = compute_artifact_id(&snapshot);
    write(path, &snapshot);
}

fn change(path: &Path, mutate: impl FnOnce(&mut serde_json::Value)) {
    let mut value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    mutate(&mut value);
    write(path, &value);
}

/// AE1: ICC≈0.30, p=1 and m=8 CLEAR; the no-lowering rule reproduces the
/// registered hurdle to five decimal places, at registered 80% power.
#[test]
fn ae1_projected_case_clears_at_the_registered_hurdle() {
    let f = Fixture::new(0.30, 0);
    let report = f.admit();
    assert!((report.measured.icc - 0.30).abs() < 1e-10);
    assert_eq!(report.measured.participation, 1.0);
    assert_eq!(report.measured.trades_per_active_session, 8.0);
    assert_eq!(format!("{:.5}", report.recomputed.hurdle), "0.03613");
    assert_eq!(report.recomputed.required_sessions, 1566.0);
    assert!((report.recomputed.effect_required_at_power - 0.04854555488459612).abs() < 1e-12);
    assert_eq!(report.strategy_code_hash, lineage::PINNED_DAILY_CODE_HASH);
    assert_eq!(report.params_hash, lineage::PINNED_DAILY_PARAMS_HASH);
    assert_eq!(report.measured.blocks, 4);
}

/// AE2: ICC≈0.60 refuses and prints measured and projected ICC together.
#[test]
fn ae2_measured_clustering_can_refuse_the_lineage() {
    let f = Fixture::new(0.60, 0);
    let out = lineage::recheck(&f.spec, &f.calendar).unwrap();
    assert_eq!(out.exit, LineageExit::Refuse);
    assert!(out.lines[0].contains("0.600000") && out.lines[0].contains("0.327334"));
    let report: RecheckReport = serde_json::from_slice(&std::fs::read(f.spec.join(lineage::RECHECK_FILE)).unwrap()).unwrap();
    assert!(report.recomputed.required_sessions > 1566.0);
    assert!(report.recomputed.hurdle > report.projected.hurdle);
    assert!(report.recomputed.effect_required_at_power > report.registered_effect);
}

#[test]
fn only_explicit_warmup_is_excluded_from_calendar_participation() {
    let f = Fixture::new(0.30, 16);
    let marked = f.admit();
    change(&f.spec.join("observation.json"), |v| { v["warmup_sessions"] = json!([]); });
    let unmarked = lineage::compute_recheck(&f.spec, &f.calendar).unwrap();
    assert_eq!(marked.measured.participation, 1.0);
    assert_eq!(unmarked.measured.participation, 0.8);
    assert_eq!(marked.measured.eligible_sessions, 64);
    assert_eq!(unmarked.measured.eligible_sessions, 80);
    assert_eq!(marked.measured.icc, unmarked.measured.icc);
}

#[test]
fn specification_range_refuses_before_reading_computation_inputs() {
    let f = Fixture::new(0.30, 0);
    for filename in ["manifest.json", "observation.json"] {
        change(&f.spec.join(filename), |v| { v["data_range"]["start"] = json!("20160731"); });
    }
    std::fs::remove_file(f.spec.join("performance.json")).unwrap();
    let out = lineage::recheck(&f.spec, Path::new("absent-calendar")).unwrap();
    assert_eq!(out.exit, LineageExit::Refuse);
    assert!(out.lines.join(" ").contains("specification"));
    assert!(!f.spec.join(lineage::RECHECK_FILE).exists());
}

/// AE11: a holdout ending one day short refuses before any claim or lock write.
#[test]
fn ae11_holdout_end_must_match_exactly() {
    let f = Fixture::new(0.30, 0);
    for filename in ["manifest.json", "observation.json"] {
        change(&f.hold.join(filename), |v| { v["data_range"]["end"] = json!("20260519"); });
    }
    let out = f.judge();
    assert_eq!(out.exit, LineageExit::Refuse);
    assert!(out.lines.join(" ").contains("exactly"));
    assert!(!f.ledger.path().exists());
    assert!(!f.ledger.path().with_extension("lock").exists());
}

#[test]
fn catalog_code_params_and_recheck_mismatches_refuse_before_claim() {
    for mismatch in ["catalog", "code", "params", "recheck"] {
        let f = Fixture::new(0.30, 0);
        f.admit();
        match mismatch {
            "catalog" => write(&f.catalog, &json!({"catalog_fingerprint": "wrong"})),
            "code" => change(&f.hold.join("manifest.json"), |v| { v["strategy_code_hash"] = json!("wrong"); }),
            "params" => change(&f.hold.join("manifest.json"), |v| { v["daily_params"]["strategy_version"] = json!(99); }),
            _ => change(&f.spec.join(lineage::RECHECK_FILE), |v| { v["params_hash"] = json!("wrong"); }),
        }
        assert_eq!(f.judge().exit, LineageExit::Refuse, "{mismatch}");
        assert!(!f.ledger.path().exists(), "{mismatch}");
        assert!(!f.hold.join(lineage::JUDGMENT_FILE).exists());
    }
}

/// The mismatch loop above mutates only the holdout side, so the downstream
/// recheck-vs-holdout cross-check would still refuse even with the two literal pin
/// comparisons deleted. Move both runs off the pin together: now nothing but the pin
/// itself can catch it, which is the property the pin exists to provide.
#[test]
fn identity_pins_refuse_a_run_that_is_self_consistent_but_off_the_pin() {
    for field in ["code", "params"] {
        let f = Fixture::new(0.30, 0);
        for dir in [&f.spec, &f.hold] {
            change(&dir.join("manifest.json"), |v| {
                if field == "code" {
                    v["strategy_code_hash"] = json!("0".repeat(64));
                } else {
                    v["daily_params"]["strategy_version"] = json!(99);
                }
            });
        }
        f.admit();
        let out = f.judge();
        assert_eq!(out.exit, LineageExit::Refuse, "{field}");
        assert!(out.lines.join(" ").contains("identity_guards pin"), "{field}: {:?}", out.lines);
        assert!(!f.ledger.path().exists(), "{field}");
        assert!(!f.hold.join(lineage::JUDGMENT_FILE).exists(), "{field}");
    }
}

/// The consuming path must be no weaker than the non-consuming one: `measure` already
/// re-folds the statistic from `performance.json`, so `judge` must too. Editing the
/// observation alone is the cheapest way to spend the one holdout on a fabricated number.
#[test]
fn judge_refolds_the_holdout_statistic_from_its_own_trade_ledger() {
    for run in ["hold", "spec"] {
        let f = Fixture::new(0.30, 0);
        f.admit();
        let dir = if run == "hold" { &f.hold } else { &f.spec };
        change(&dir.join("observation.json"), |v| { v["observed_net_ror"] = json!(9.0); });
        let out = f.judge();
        assert_eq!(out.exit, LineageExit::Refuse, "{run}");
        assert!(out.lines.join(" ").contains("net RoR mismatch"), "{run}: {:?}", out.lines);
        assert!(!f.ledger.path().exists(), "{run}");
        assert!(!f.hold.join(lineage::JUDGMENT_FILE).exists(), "{run}");
    }
}

/// Every identity field on a `recheck.json` survives an in-place edit of the run it was
/// computed from, so identity alone cannot prove the report is current. Bind it to the
/// statistic: a spec run that moved after its CLEAR must re-run the re-check.
#[test]
fn judge_refuses_a_recheck_that_no_longer_matches_its_source_run() {
    let f = Fixture::new(0.30, 0);
    f.admit();
    // Move the spec run consistently -- observation and trade ledger agree with each other,
    // so the re-fold passes and only the stale-report check can catch this.
    change(&f.spec.join("observation.json"), |v| { v["observed_net_ror"] = json!(0.09); });
    change(&f.spec.join("performance.json"), |v| {
        for trade in v["trades"].as_array_mut().unwrap() {
            trade["realized_pnl"] = json!(9.0);
            trade["realized_r"] = json!(0.09);
        }
    });
    let out = f.judge();
    assert_eq!(out.exit, LineageExit::Refuse);
    assert!(out.lines.join(" ").contains("stale"), "{:?}", out.lines);
    assert!(!f.ledger.path().exists());
}

/// The advisory lock is the only thing serializing the resume-and-back-fill sequence, and
/// its contention branch was never exercised. Hold the lock and prove `judge` refuses
/// without touching the ledger.
#[test]
fn a_held_judgment_lock_refuses_a_concurrent_judge() {
    let f = Fixture::new(0.30, 0);
    f.admit();
    let held = f.ledger.lock_judgment().unwrap();
    let out = f.judge();
    assert_eq!(out.exit, LineageExit::Refuse);
    assert!(out.lines.join(" ").contains("already running"), "{:?}", out.lines);
    assert!(!f.ledger.path().exists());
    assert!(!f.hold.join(lineage::JUDGMENT_FILE).exists());
    // Released, the same judgment proceeds -- the lock gates, it does not poison.
    drop(held);
    assert_eq!(f.judge().exit, LineageExit::Clear);
}

/// Until U4's warmup loader lands, a signal needing prior bars warms up *inside* its own
/// window and no producer marks it. Scoring that run silently shrinks participation and
/// raises the hurdle, so a re-check that is spent exactly once must refuse the ambiguity.
#[test]
fn unmarked_warmup_refuses_rather_than_scoring_participation_it_cannot_trust() {
    let f = Fixture::new(0.30, 16);
    change(&f.spec.join("manifest.json"), |v| {
        v["daily_params"]["ranking_signal"] =
            serde_json::to_value(RankingSignalKind::Momentum12x1).unwrap();
    });
    // Warmup recorded: scored normally.
    assert_eq!(lineage::recheck(&f.spec, &f.calendar).unwrap().exit, LineageExit::Clear);
    // Same run, marker dropped: refuses instead of measuring participation over sessions
    // the signal could not score.
    change(&f.spec.join("observation.json"), |v| { v["warmup_sessions"] = json!([]); });
    let err = lineage::compute_recheck(&f.spec, &f.calendar).unwrap_err();
    assert!(format!("{err:#}").contains("marks no warmup"), "{err:#}");
    // A signal that needs only the current bar has no such prefix and is unaffected.
    change(&f.spec.join("manifest.json"), |v| {
        v["daily_params"]["ranking_signal"] =
            serde_json::to_value(RankingSignalKind::PriorTurnoverDesc).unwrap();
    });
    assert!(lineage::compute_recheck(&f.spec, &f.calendar).is_ok());
}

#[test]
fn judgment_claims_backfills_and_refuses_a_second_judgment() {
    let f = Fixture::new(0.30, 0);
    f.admit();
    let out = f.judge();
    assert_eq!(out.exit, LineageExit::Clear, "{:?}", out.lines);
    let rows = f.ledger.read_all().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].cleared.is_none() && rows[0].observed_net_ror.is_none());
    assert_eq!(rows[1].cleared, Some(true));
    assert_eq!(rows[0].strategy_code_hash, rows[1].strategy_code_hash);
    assert_eq!(rows[0].params_hash, rows[1].params_hash);
    assert!(f.hold.join(lineage::JUDGMENT_FILE).exists());
    let bytes = std::fs::read(f.ledger.path()).unwrap();
    assert_eq!(f.judge().exit, LineageExit::Refuse);
    assert_eq!(std::fs::read(f.ledger.path()).unwrap(), bytes);
}

/// AE11: resume only the same complete unfinished claim, including both hashes.
#[test]
fn ae11_same_claim_resumes_but_other_identity_cannot() {
    for mismatch in ["run", "catalog", "code", "params", "prereg", "none"] {
        let f = Fixture::new(0.30, 0);
        f.admit();
        let mut claim = f.claim();
        match mismatch {
            "run" => claim.run_id = "different-run".into(),
            "catalog" => claim.catalog_fingerprint = "different-catalog".into(),
            "code" => claim.strategy_code_hash = Some("a".repeat(64)),
            "params" => claim.params_hash = Some("b".repeat(64)),
            "prereg" => claim.prereg_content_hash = "different-freeze".into(),
            _ => {},
        }
        f.ledger.append(&claim, true).unwrap();
        let out = f.judge();
        let rows = f.ledger.read_all().unwrap();
        if mismatch == "none" {
            assert_eq!(out.exit, LineageExit::Clear, "{:?}", out.lines);
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].claimed_utc, rows[1].claimed_utc);
        } else {
            assert_eq!(out.exit, LineageExit::Refuse, "{mismatch}");
            assert_eq!(rows.len(), 1);
        }
    }
}

#[test]
fn placeholder_refuses_both_verbs() {
    let f = Fixture::new(0.30, 0);
    f.admit();
    for run in [&f.spec, &f.hold] {
        change(&run.join("observation.json"), |v| { v["ranking_signal_is_placeholder"] = json!(true); });
    }
    for out in [lineage::recheck(&f.spec, &f.calendar).unwrap(), f.judge()] {
        assert_eq!(out.exit, LineageExit::Refuse);
        assert!(out.lines.join(" ").contains("PLACEHOLDER"));
    }
    assert!(!f.ledger.path().exists());
}

#[test]
fn orb_refuses_both_verbs_even_without_an_observation() {
    let f = Fixture::new(0.30, 0);
    for run in [&f.spec, &f.hold] {
        change(&run.join("manifest.json"), |v| { v["strategy_id"] = json!("orb"); });
        std::fs::remove_file(run.join("observation.json")).unwrap();
    }
    for out in [lineage::recheck(&f.spec, &f.calendar).unwrap(), f.judge()] {
        assert_eq!(out.exit, LineageExit::Refuse);
        assert!(out.lines.join(" ").contains("strategy-id mismatch"));
    }
    assert!(!f.ledger.path().exists());
}

#[test]
fn empty_torn_partial_legacy_and_verdict_rows_never_resume() {
    let f = Fixture::new(0.30, 0);
    f.admit();
    let full = serde_json::to_string(&f.claim()).unwrap();
    let mut verdict = serde_json::to_value(f.claim()).unwrap();
    verdict["cleared"] = json!(null);
    let mut legacy = serde_json::to_value(f.claim()).unwrap();
    legacy.as_object_mut().unwrap().remove("params_hash");
    for text in [String::new(), "\n".into(), "{torn\n".into(), "{\"run_id\":\"hold\"}\n".into(),
        full.clone(), format!("{full}\n{{torn\n"), format!("{legacy}\n"), format!("{verdict}\n"),
        format!("{full}\n{full}\n")]
    {
        std::fs::write(f.ledger.path(), &text).unwrap();
        assert_eq!(f.judge().exit, LineageExit::Refuse, "accepted {text:?}");
        assert_eq!(std::fs::read_to_string(f.ledger.path()).unwrap(), text);
    }
}

#[test]
fn losing_judgment_still_backfills_and_spends_the_holdout() {
    let f = Fixture::new(0.30, 0);
    f.admit();
    // The loss has to be real in the trade ledger, not merely asserted in the observation:
    // `judge` re-folds the statistic, so a run that only *claims* to have lost is refused
    // before the claim. Editing the observation alone was this test's original lever, and
    // that lever was the defect.
    change(&f.hold.join("performance.json"), |v| {
        for trade in v["trades"].as_array_mut().unwrap() {
            trade["realized_pnl"] = json!(-20.0);
            trade["risk_capital"] = json!(100.0);
            trade["realized_r"] = json!(-0.2);
        }
    });
    change(&f.hold.join("observation.json"), |v| { v["observed_net_ror"] = json!(-0.2); });
    assert_eq!(f.judge().exit, LineageExit::Refuse);
    assert_eq!(f.ledger.read_all().unwrap()[1].cleared, Some(false));
    assert!(f.hold.join(lineage::JUDGMENT_FILE).exists());
}

#[test]
fn exit_blocks_use_exit_dates_keep_gaps_and_preserve_ratio_of_sums() {
    let sessions: Vec<_> = (0..33).map(|n| date("2017-01-02") + Duration::days(n)).collect();
    let mut trades = synthetic(&sessions[..2], 0.3);
    trades.truncate(2);
    trades[0].ts_closed = Some(stamp(sessions[15]));
    trades[1].ts_closed = Some(stamp(sessions[32]));
    trades[0].realized_pnl = 20.0; trades[0].risk_capital = Some(100.0);
    trades[1].realized_pnl = 90.0; trades[1].risk_capital = Some(300.0);
    let mut censored = trades[0].clone(); censored.ts_closed = None; censored.risk_capital = None;
    trades.push(censored);
    let blocks = stats::exit_session_blocks(&trades, &sessions, 16).unwrap();
    assert_eq!(blocks, vec![vec![(20.0, 100.0)], vec![], vec![(90.0, 300.0)]]);
    assert_eq!(stats::ratio_statistic(&blocks).unwrap(), 0.275);
    assert!(stats::exit_session_blocks(&trades, &sessions, 0).is_err());
    assert!(stats::exit_session_blocks(&trades, &sessions[..32], 16).is_err());
}

#[test]
fn missing_frozen_catalog_pin_is_not_replaced_by_universe_manifest_hash() {
    let f = Fixture::new(0.30, 0);
    f.admit();
    write(&f.catalog, &json!({"provenance": {"manifest_hash": "fixture-catalog"}}));
    let out = f.judge();
    assert_eq!(out.exit, LineageExit::Refuse);
    assert!(out.lines.join(" ").contains("no catalog_fingerprint"));
    assert!(!f.ledger.path().exists());
}
