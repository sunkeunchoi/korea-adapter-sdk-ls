//! Reproducible specification recheck and one-shot daily holdout judgment.
//! Frozen inputs are read only. Each command writes solely to its run and, for
//! judgment, the append-only claim ledger after all provenance checks pass.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{ensure, Context};
use chrono::{NaiveDate, Utc};
use nautilus_ls_calendar::{schema::DayStatus, KrxCalendar};
use serde::{Deserialize, Serialize};

use crate::artifacts::manifest::{DataRange, Manifest};
use crate::artifacts::observation::RunObservation;
use crate::artifacts::performance::PerformanceReport;
use crate::dispatch::ladder::daily_governed_params_hash;
use crate::lineage_prereg::{self, JudgmentAttempt, JudgmentLedger, LoadedLineagePreReg};
use crate::stats;

/// The same literal pins guarded by tests/identity_guards.rs. Never derive these
/// from the running source: that would silently accept a changed head.
pub const PINNED_DAILY_CODE_HASH: &str =
    "fb78cc5502a023939a8341c53cd071cbc2b89ff93d0e9826d952347cddb5a8b4";
/// The daily governed-params pin in tests/identity_guards.rs.
pub const PINNED_DAILY_PARAMS_HASH: &str =
    "c7980e8b24625a2d0773b0c07dfb7bdaddd38eb3033a0c6b4a9d5043e04b68f0";
/// Run-local recheck artifact.
pub const RECHECK_FILE: &str = "recheck.json";
/// Run-local judgment artifact.
pub const JUDGMENT_FILE: &str = "judgment.json";

/// A refusal is an ordinary command outcome with a nonzero exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LineageExit { Clear, Refuse }

impl LineageExit {
    /// Process status for the thin research dispatch arm.
    pub fn exit_code(self) -> ExitCode {
        match self { Self::Clear => ExitCode::SUCCESS, Self::Refuse => ExitCode::FAILURE }
    }
}

/// Human output plus a typed exit, as in runner::governed.
#[derive(Debug)]
pub struct LineageOutcome {
    pub exit: LineageExit,
    pub lines: Vec<String>,
}

impl LineageOutcome {
    fn refuse(error: impl std::fmt::Display) -> Self {
        Self { exit: LineageExit::Refuse, lines: vec![format!("REFUSE {error}")] }
    }
}

/// Measurements from this specification run, on an explicitly named calendar basis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Measured {
    pub closed_trades: usize,
    pub exit_clusters: usize,
    pub icc: f64,
    pub design_effect: f64,
    pub effective_n: f64,
    pub net_r_sd: f64,
    pub calendar_sessions: usize,
    pub warmup_sessions: usize,
    pub eligible_sessions: usize,
    pub active_entry_sessions: usize,
    pub participation: f64,
    pub trades_per_active_session: f64,
    pub trades_per_calendar_session: f64,
    pub closed_trades_per_calendar_session: f64,
    pub observed_net_ror: f64,
    pub block_length_sessions: usize,
    pub blocks: usize,
}

/// The frozen projection alongside the lineage's own measurements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Projected {
    pub icc: f64,
    pub net_r_sd: f64,
    pub design_effect: f64,
    pub trades_per_calendar_session: f64,
    pub holdout_sessions: usize,
    pub holdout_se: f64,
    pub bar: f64,
    pub haircut: f64,
    pub hurdle: f64,
}

/// Re-derived terms, retaining the freeze's no-lowering rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recomputed {
    pub variance_ratio: f64,
    pub unfloored_holdout_se: f64,
    pub holdout_se: f64,
    pub bar: f64,
    pub haircut: f64,
    pub hurdle: f64,
    pub effect_required_at_power: f64,
    pub required_sessions: f64,
    pub trade_model_required_sessions: f64,
}

/// Complete reproducible admission evidence. No fields are added to the frozen schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecheckReport {
    pub schema_version: u32,
    pub run_id: String,
    pub data_range: DataRange,
    pub catalog_fingerprint: String,
    pub strategy_code_hash: String,
    pub params_hash: String,
    pub prereg_content_hash: String,
    pub margin_content_hash: String,
    pub calendar_artifact_id: String,
    pub calendar_id: String,
    pub confidence: f64,
    pub power: f64,
    pub registered_effect: f64,
    pub derivation: String,
    pub measured: Measured,
    pub projected: Projected,
    pub recomputed: Recomputed,
    pub verdict: LineageExit,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    serde_json::from_slice(&std::fs::read(path).with_context(|| format!("reading {}", path.display()))?)
        .with_context(|| format!("parsing {}", path.display()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    // Atomic replacement avoids leaving apparently usable partial evidence after a crash.
    use std::io::Write;
    let mut file = tempfile::NamedTempFile::new_in(path.parent().context("artifact parent")?)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    Ok(())
}

fn range(range: &DataRange) -> anyhow::Result<(NaiveDate, NaiveDate)> {
    Ok((NaiveDate::parse_from_str(&range.start, "%Y%m%d")?,
        NaiveDate::parse_from_str(&range.end, "%Y%m%d")?))
}

fn params_hash(manifest: &Manifest) -> anyhow::Result<String> {
    Ok(daily_governed_params_hash(manifest.daily_params.as_ref().context("missing daily_params")?))
}

/// Re-fold the verdict statistic from the run's own closed trades.
///
/// `measure` already does this for the re-check, but it needs a calendar to bucket exits
/// into blocks. The judging path has no calendar and does not need one: the statistic is a
/// ratio of sums, so the blocking that matters to the bootstrap cannot move the total.
/// Without this the one irreversible verdict is decided by a float read verbatim out of
/// `observation.json` that no other artifact corroborates — leaving the consuming path
/// strictly weaker than the non-consuming dry run.
fn refold_observed_net_ror(run_dir: &Path, observation: &RunObservation) -> anyhow::Result<()> {
    let performance: PerformanceReport = read_json(&run_dir.join(crate::artifacts::PERFORMANCE_FILE))?;
    let mut block: stats::Block = Vec::new();
    for trade in &performance.trades {
        let Some(ts) = trade.ts_closed else { continue };
        ensure!(ts >= trade.ts_opened, "exit precedes entry");
        let risk = trade.risk_capital.context("closed trade missing risk_capital")?;
        ensure!(risk.is_finite() && risk > 0.0, "closed trade has non-positive risk_capital");
        ensure!(trade.realized_pnl.is_finite(), "closed trade has non-finite realized_pnl");
        block.push((trade.realized_pnl, risk));
    }
    ensure!(block.len() == observation.closed_positions as usize
        && performance.trades.len() - block.len() == observation.censored_positions as usize,
        "observation/performance position count mismatch");
    ensure!(nearly_equal(stats::ratio_statistic(&[block])?, observation.observed_net_ror),
        "observation/performance net RoR mismatch");
    Ok(())
}

fn session(ts: u64) -> NaiveDate {
    nautilus_ls::ingest::kst_date_of(nautilus_core::UnixNanos::from(ts))
}

fn nearly_equal(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-10 * a.abs().max(b.abs()).max(1.0)
}

/// Compute a recheck with no writes. Range and placeholder gates run before the
/// calendar or performance artifact is opened.
pub fn compute_recheck(run_dir: &Path, calendar_path: &Path) -> anyhow::Result<RecheckReport> {
    let prereg = lineage_prereg::load(&lineage_prereg::frozen_lineage_prereg_path())?;
    let (observation, manifest) = RunObservation::read(run_dir)?;
    let (from, to) = range(&observation.data_range)?;
    lineage_prereg::specification_dry_run(&prereg.values, from, to, observation.observed_net_ror)?;
    observation.judgment_arguments()?;
    let calendar = KrxCalendar::load_from_path(calendar_path, Utc::now())?;
    let view = calendar.as_of(Utc::now())?;
    let mut sessions = Vec::new();
    for date in from.iter_days().take_while(|d| *d <= to) {
        match view.day(date)?.status {
            DayStatus::TradingSession => sessions.push(date),
            DayStatus::Closed => {},
            DayStatus::Unknown => anyhow::bail!("calendar cannot prove {date} open or closed"),
        }
    }
    let performance: PerformanceReport = read_json(&run_dir.join(crate::artifacts::PERFORMANCE_FILE))?;
    measure(&prereg, &observation, &manifest, &performance, &sessions,
        calendar.artifact_id(), calendar.calendar_id())
}

fn measure(
    prereg: &LoadedLineagePreReg, observation: &RunObservation, manifest: &Manifest,
    performance: &PerformanceReport, sessions: &[NaiveDate], calendar_artifact_id: &str,
    calendar_id: &str,
) -> anyhow::Result<RecheckReport> {
    let frozen = &prereg.values;
    ensure!(frozen.search.n_max == 1 && frozen.power.confidence == 0.95,
        "unsupported frozen derivation: requires n_max=1 and the registered 0.95 bar");
    let margin = crate::margin::load(&crate::margin::frozen_margin_path())?;
    let baseline = &margin.values.provenance;
    let blocks = stats::exit_session_blocks(&performance.trades, sessions,
        frozen.verdict.bootstrap_block_length_sessions)?;
    let ratio = stats::ratio_statistic(&blocks)?;
    ensure!(nearly_equal(ratio, observation.observed_net_ror), "observation/performance net RoR mismatch");
    let rows: BTreeMap<_, _> = observation.sessions.iter().map(|s| (s.session_date, s)).collect();
    ensure!(rows.len() == observation.sessions.len(), "duplicate observation sessions");
    ensure!(rows.keys().all(|d| sessions.binary_search(d).is_ok()), "observation session outside calendar");
    let warmup: BTreeSet<_> = observation.warmup_sessions.iter().copied().collect();
    ensure!(warmup.len() == observation.warmup_sessions.len()
        && warmup.iter().all(|d| sessions.binary_search(d).is_ok()), "invalid warmup session markers");
    // Warmup is a leading prefix, never an arbitrary way to remove quiet dates.
    ensure!(sessions.iter().take(warmup.len()).all(|d| warmup.contains(d)),
        "warmup sessions must be a leading calendar prefix");
    let mut entries: BTreeMap<NaiveDate, usize> = BTreeMap::new();
    let mut exits: BTreeMap<NaiveDate, (usize, f64, f64)> = BTreeMap::new();
    let mut values = Vec::new();
    let mut cluster_ids = Vec::new();
    for trade in &performance.trades {
        let opened = session(trade.ts_opened);
        ensure!(sessions.binary_search(&opened).is_ok(), "entry session outside calendar: {opened}");
        ensure!(!warmup.contains(&opened), "entry during marked warmup: {opened}");
        *entries.entry(opened).or_default() += 1;
        if let Some(ts) = trade.ts_closed {
            ensure!(ts >= trade.ts_opened, "exit precedes entry");
            let closed = session(ts);
            let risk = trade.risk_capital.context("closed trade missing risk_capital")?;
            let net_r = trade.realized_r.context("closed trade missing realized_r")?;
            ensure!(nearly_equal(net_r, trade.realized_pnl / risk), "inconsistent per-trade net r");
            values.push(net_r);
            cluster_ids.push(sessions.binary_search(&closed)
                .map_err(|_| anyhow::anyhow!("exit outside calendar: {closed}"))?);
            let e = exits.entry(closed).or_default();
            e.0 += 1; e.1 += trade.realized_pnl; e.2 += risk;
        }
    }
    ensure!(values.len() == observation.closed_positions as usize
        && performance.trades.len() - values.len() == observation.censored_positions as usize,
        "observation/performance position count mismatch");
    for date in sessions {
        let entry_count = entries.get(date).copied().unwrap_or(0);
        let (closes, pnl, risk) = exits.get(date).copied().unwrap_or_default();
        if let Some(row) = rows.get(date) {
            ensure!(row.entries as usize == entry_count && row.closes as usize == closes
                && nearly_equal(row.realized_pnl, pnl) && nearly_equal(row.risk_capital, risk),
                "observation/performance session mismatch on {date}");
        } else {
            ensure!(entry_count == 0 && closes == 0, "missing active observation session {date}");
        }
    }
    // A signal that needs prior bars cannot score the opening sessions of its own window
    // unless the runner loaded warmup from before it. An unmarked leading prefix with no
    // entries is therefore ambiguous: either warmup came from outside the window (correct,
    // nothing to mark) or it ran inside the window and nobody recorded it. The second case
    // shrinks `participation`, inflates `projected_rate / rate`, and raises the hurdle --
    // it pushes this gate toward REFUSE on arithmetic rather than on the strategy. The
    // re-check is spent exactly once, so refuse the ambiguity instead of scoring it.
    let lookback = manifest.daily_params.as_ref().context("missing daily_params")?
        .ranking_signal.warmup_bars();
    if warmup.is_empty() && lookback > 1 {
        let prefix = lookback - 1;
        ensure!(sessions.len() <= prefix
            || !sessions.iter().take(prefix).all(|d| entries.get(d).copied().unwrap_or(0) == 0),
            "ranking signal declares {lookback} prior bars and the first {prefix} sessions carry \
             no entry, but the observation marks no warmup: participation would be measured over \
             sessions the signal could not score");
    }
    let eligible = sessions.len().checked_sub(warmup.len()).filter(|n| *n > 0)
        .context("no calendar sessions after warmup")?;
    let active = entries.len();
    ensure!(active > 0, "no participating sessions");
    let participation = active as f64 / eligible as f64;
    let m = performance.trades.len() as f64 / active as f64;
    let rate = m * participation;
    let clustering = stats::clustering(&values, &cluster_ids)?;
    let sd = stats::sample_sd(&values)?;
    let projected_rate = frozen.hypothesis.target_m as f64 * frozen.hypothesis.target_session_participation;
    let projected_de = stats::design_effect(frozen.hypothesis.target_m as f64, baseline.icc);
    let variance_ratio = (sd / baseline.net_r_sd).powi(2)
        * clustering.design_effect / projected_de * projected_rate / rate;
    ensure!(variance_ratio.is_finite(), "non-finite variance projection");
    // Reproduce the freeze's rounded z=1.96 and SE-root scaling. Transport the
    // root variance by measured sd², DE and calendar entry supply relative to
    // target_m/target_p at projected ICC. Never lower the frozen bar.
    let projected_se = nautilus_ls::reference::pit_walk::SE_AT_ROOT
        * (nautilus_ls::reference::pit_walk::SE_ROOT_SESSIONS / frozen.holdout().sessions as f64).sqrt();
    let unfloored_se = projected_se * variance_ratio.sqrt();
    let se = unfloored_se.max(projected_se);
    let bar = nautilus_ls::reference::pit_walk::Z_95 * se;
    let haircut = frozen.verdict.haircut_fraction * bar;
    let hurdle = bar + haircut;
    let required_effect = hurdle + stats::power_z(frozen.power.power)? * se;
    let target = frozen.hypothesis.effect_size_net_ror;
    let raw_sessions = frozen.holdout().sessions as f64 * (required_effect / target).powi(2);
    // Only absorb floating point roundoff at an exact integer boundary.
    let required_sessions = (raw_sessions - 1e-9).ceil();
    let trade_sessions = stats::required_trades(target, sd, clustering.design_effect,
        frozen.power.confidence, frozen.power.power)? / rate;
    let verdict = if target >= required_effect || nearly_equal(target, required_effect) {
        LineageExit::Clear
    } else { LineageExit::Refuse };
    Ok(RecheckReport {
        schema_version: 1, run_id: observation.run_id.clone(), data_range: observation.data_range.clone(),
        catalog_fingerprint: observation.catalog_fingerprint.clone(),
        strategy_code_hash: manifest.strategy_code_hash.clone(), params_hash: params_hash(manifest)?,
        prereg_content_hash: prereg.content_hash.clone(), margin_content_hash: margin.content_hash,
        calendar_artifact_id: calendar_artifact_id.into(), calendar_id: calendar_id.into(),
        confidence: frozen.power.confidence, power: frozen.power.power, registered_effect: target,
        derivation: "variance ratio = (measured sd / ORB sd)^2 * measured exit DE / DE(target_m, projected ICC) * target_m * target_p / calendar entry rate; SE = max(frozen SE, frozen SE * sqrt(variance ratio)); bar = 1.96 * SE; haircut = registered fraction * bar; power target = hurdle + power_z * SE; required sessions = ceil(holdout sessions * (power target / registered effect)^2)".into(),
        measured: Measured {
            closed_trades: values.len(), exit_clusters: clustering.clusters, icc: clustering.icc,
            design_effect: clustering.design_effect, effective_n: clustering.effective_n, net_r_sd: sd,
            calendar_sessions: sessions.len(), warmup_sessions: warmup.len(), eligible_sessions: eligible,
            active_entry_sessions: active, participation, trades_per_active_session: m,
            trades_per_calendar_session: rate, closed_trades_per_calendar_session: values.len() as f64 / eligible as f64,
            observed_net_ror: ratio, block_length_sessions: frozen.verdict.bootstrap_block_length_sessions,
            blocks: blocks.len(),
        },
        projected: Projected {
            icc: baseline.icc, net_r_sd: baseline.net_r_sd, design_effect: projected_de,
            trades_per_calendar_session: projected_rate, holdout_sessions: frozen.holdout().sessions,
            holdout_se: projected_se, bar: frozen.bar(), haircut: frozen.haircut(), hurdle: frozen.hurdle(),
        },
        recomputed: Recomputed {
            variance_ratio, unfloored_holdout_se: unfloored_se, holdout_se: se, bar, haircut, hurdle,
            effect_required_at_power: required_effect, required_sessions,
            trade_model_required_sessions: trade_sessions.ceil(),
        }, verdict,
    })
}

/// Persist measured admission evidence; invalid inputs return a typed refusal.
pub fn recheck(run_dir: &Path, calendar_path: &Path) -> anyhow::Result<LineageOutcome> {
    let report = match compute_recheck(run_dir, calendar_path) {
        Ok(report) => report, Err(e) => return Ok(LineageOutcome::refuse(format!("{e:#}"))),
    };
    write_json(&run_dir.join(RECHECK_FILE), &report)?;
    Ok(LineageOutcome { exit: report.verdict, lines: vec![
        format!("measured ICC {:.6}; projected ICC {:.6}; calendar participation {:.6}; entries/session {:.6}",
            report.measured.icc, report.projected.icc, report.measured.participation,
            report.measured.trades_per_calendar_session),
        format!("{:?}: hurdle {:.8}; registered effect {:.8}; effect required at power {:.2}: {:.8}; required sessions {:.0}",
            report.verdict, report.recomputed.hurdle, report.registered_effect, report.power,
            report.recomputed.effect_required_at_power, report.recomputed.required_sessions).replace("Clear:", "CLEAR:").replace("Refuse:", "REFUSE:"),
    ] })
}

/// Persisted holdout verdict and the identities checked before the claim.
#[derive(Debug, Serialize, Deserialize)]
pub struct JudgmentReport {
    pub schema_version: u32,
    pub run_id: String,
    pub recheck_run_id: String,
    pub catalog_fingerprint: String,
    pub strategy_code_hash: String,
    pub params_hash: String,
    pub prereg_content_hash: String,
    pub observed_net_ror: f64,
    pub hurdle: f64,
    pub cleared: bool,
}

fn validate_judgment(run_dir: &Path, recheck_dir: &Path, catalog_record: &Path)
    -> anyhow::Result<(LoadedLineagePreReg, JudgmentAttempt, RecheckReport, f64)>
{
    let prereg = lineage_prereg::load(&lineage_prereg::frozen_lineage_prereg_path())?;
    let (observation, manifest) = RunObservation::read(run_dir)?;
    let args = observation.judgment_arguments()?;
    let (from, to) = range(&observation.data_range)?;
    ensure!(from == prereg.values.holdout().from && to == prereg.values.holdout().to,
        "data_range must equal the holdout window exactly");
    let params = params_hash(&manifest)?;
    ensure!(manifest.strategy_code_hash == PINNED_DAILY_CODE_HASH, "code hash differs from identity_guards pin");
    ensure!(params == PINNED_DAILY_PARAMS_HASH, "params hash differs from identity_guards pin");
    refold_observed_net_ror(run_dir, &observation)?;
    let catalog: serde_json::Value = read_json(catalog_record)?;
    let fingerprint = catalog.get("catalog_fingerprint").and_then(|v| v.as_str())
        .context("frozen daily catalog record has no catalog_fingerprint; provenance.manifest_hash is not a catalog fingerprint")?;
    ensure!(!fingerprint.is_empty() && args.catalog_fingerprint == fingerprint,
        "catalog fingerprint differs from frozen value");
    let recheck: RecheckReport = read_json(&recheck_dir.join(RECHECK_FILE))?;
    let (spec, spec_manifest) = RunObservation::read(recheck_dir)?;
    spec.judgment_arguments()?;
    let (spec_from, spec_to) = range(&spec.data_range)?;
    lineage_prereg::specification_dry_run(&prereg.values, spec_from, spec_to, spec.observed_net_ror)?;
    ensure!(recheck.schema_version == 1 && recheck.verdict == LineageExit::Clear
        && recheck.prereg_content_hash == prereg.content_hash, "recheck is not a current CLEAR");
    refold_observed_net_ror(recheck_dir, &spec)?;
    ensure!(recheck.run_id == spec.run_id && recheck.data_range == spec.data_range
        && recheck.catalog_fingerprint == spec.catalog_fingerprint,
        "recheck provenance differs from its source run");
    // Bind the report to the run as it stands NOW, not merely to a run of the same identity.
    // Every field above survives an in-place edit of the spec run's artifacts, so without
    // this a CLEAR computed against an earlier state of the same directory still reads as
    // current and the holdout is spent on superseded evidence.
    ensure!(nearly_equal(recheck.measured.observed_net_ror, spec.observed_net_ror),
        "recheck statistic differs from its source run: the report is stale, re-run recheck");
    ensure!(recheck.strategy_code_hash == manifest.strategy_code_hash
        && recheck.params_hash == params && recheck.strategy_code_hash == spec_manifest.strategy_code_hash
        && recheck.params_hash == params_hash(&spec_manifest)?, "holdout/recheck code or params hash mismatch");
    ensure!(nearly_equal(recheck.registered_effect, prereg.values.hypothesis.effect_size_net_ror)
        && recheck.recomputed.hurdle >= prereg.values.hurdle() - 1e-12
        && (recheck.registered_effect >= recheck.recomputed.effect_required_at_power
            || nearly_equal(recheck.registered_effect, recheck.recomputed.effect_required_at_power)),
        "recheck values do not support CLEAR");
    let claim = JudgmentAttempt {
        schema_version: lineage_prereg::JUDGMENT_SCHEMA_VERSION, run_id: args.run_id,
        catalog_fingerprint: args.catalog_fingerprint, strategy_code_hash: Some(manifest.strategy_code_hash),
        params_hash: Some(params), claimed_utc: Utc::now().to_rfc3339(),
        prereg_content_hash: prereg.content_hash.clone(), observed_net_ror: None, cleared: None,
    };
    Ok((prereg, claim, recheck, args.observed_net_ror))
}

/// Judge only after all observation, catalog and identity gates pass. The paths are
/// explicit for offline tests; the CLI fixes the catalog and ledger in production.
pub fn judge(run_dir: &Path, recheck_dir: &Path, ledger: &JudgmentLedger, catalog_record: &Path)
    -> anyhow::Result<LineageOutcome>
{
    let (prereg, claim, recheck, observed) = match validate_judgment(run_dir, recheck_dir, catalog_record) {
        Ok(inputs) => inputs, Err(e) => return Ok(LineageOutcome::refuse(format!("{e:#}"))),
    };
    let _lock = match ledger.lock_judgment() {
        Ok(lock) => lock, Err(e) => return Ok(LineageOutcome::refuse(e)),
    };
    let verdict = match lineage_prereg::judge_holdout_resuming(&prereg, ledger, &claim, observed) {
        Ok(verdict) => verdict, Err(e) => return Ok(LineageOutcome::refuse(e)),
    };
    let report = JudgmentReport {
        schema_version: 1, run_id: verdict.run_id, recheck_run_id: recheck.run_id,
        catalog_fingerprint: claim.catalog_fingerprint,
        strategy_code_hash: claim.strategy_code_hash.context("missing claim code hash")?,
        params_hash: claim.params_hash.context("missing claim params hash")?,
        prereg_content_hash: verdict.prereg_content_hash, observed_net_ror: verdict.observed_net_ror,
        hurdle: verdict.hurdle, cleared: verdict.cleared,
    };
    // Write the run-local artifact BEFORE the ledger back-fill, so every crash point stays
    // recoverable. The verdict is re-derived deterministically from inputs that are already
    // gated, so a death here leaves exactly the single unfinished claim `claim_or_resume`
    // accepts and the retry rewrites this file. The reverse order strands it permanently:
    // two ledger rows read as judged, the resume branch refuses them, and the CLI can never
    // produce `judgment.json` again even though the verdict is recorded.
    write_json(&run_dir.join(JUDGMENT_FILE), &report)?;
    // Copy the original claim, including its timestamp, for the audit back-fill.
    let mut backfill = ledger.read_all()?.into_iter().next().context("missing claimed row")?;
    backfill.observed_net_ror = Some(report.observed_net_ror);
    backfill.cleared = Some(report.cleared);
    ledger.append(&backfill, false)?;
    Ok(LineageOutcome {
        exit: if report.cleared { LineageExit::Clear } else { LineageExit::Refuse },
        lines: vec![format!("{} {}", if report.cleared { "CLEAR" } else { "REFUSE" }, serde_json::to_string(&report)?)],
    })
}

fn cli_args(judging: bool) -> anyhow::Result<(PathBuf, Option<PathBuf>)> {
    let mut args = std::env::args().skip(3);
    let mut run = None;
    let mut recheck = None;
    while let Some(flag) = args.next() {
        let slot = match flag.as_str() {
            "--run" => &mut run,
            "--recheck" if judging => &mut recheck,
            _ => anyhow::bail!("unknown lineage argument {flag:?}"),
        };
        ensure!(slot.is_none(), "duplicate lineage argument {flag}");
        let value = args.next().filter(|v| !v.is_empty() && !v.starts_with("--"))
            .with_context(|| format!("{flag} needs a run directory"))?;
        *slot = Some(PathBuf::from(value));
    }
    let run = run.context("lineage requires --run <run-directory>")?;
    ensure!(!judging || recheck.is_some(), "lineage judge requires --recheck <recheck-run-directory>");
    Ok((run, recheck))
}

/// `lab-research lineage recheck --run <dir>`; calendar from LS_CALENDAR_SNAPSHOT.
pub fn run_recheck_cli() -> anyhow::Result<LineageOutcome> {
    let (run, _) = cli_args(false)?;
    let calendar = nautilus_ls::calendar::snapshot_path_from_env().unwrap_or_default();
    recheck(&run, &calendar)
}

fn test_path(key: &str, default: PathBuf) -> anyhow::Result<PathBuf> {
    match std::env::var_os(key) {
        Some(value) => {
            ensure!(cfg!(debug_assertions), "{key} is only available in test/debug builds");
            ensure!(!value.is_empty(), "{key} must not be empty");
            Ok(PathBuf::from(value))
        }
        None => Ok(default),
    }
}

/// `lab-research lineage judge --run <dir> --recheck <dir>`.
/// LS_LINEAGE_TEST_LEDGER and LS_LINEAGE_TEST_CATALOG_RECORD are debug-only test
/// seams. The latter requires an isolated test ledger and never changes a frozen file.
pub fn run_judge_cli() -> anyhow::Result<LineageOutcome> {
    let (run, recheck) = cli_args(true)?;
    let ledger = test_path("LS_LINEAGE_TEST_LEDGER", lineage_prereg::judgment_ledger_path())?;
    if std::env::var_os("LS_LINEAGE_TEST_CATALOG_RECORD").is_some() {
        ensure!(std::env::var_os("LS_LINEAGE_TEST_LEDGER").is_some(), "test catalog requires a test ledger");
    }
    let catalog = test_path("LS_LINEAGE_TEST_CATALOG_RECORD",
        Path::new(env!("CARGO_MANIFEST_DIR")).join("config/daily-catalog-20160801-20260812.json"))?;
    judge(&run, &recheck.context("missing --recheck")?, &JudgmentLedger::new(ledger), &catalog)
}
