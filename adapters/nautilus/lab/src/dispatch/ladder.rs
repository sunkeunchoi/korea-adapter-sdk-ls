//! U10 — capital-ladder enforcement (R12–R15, KD2, KD3; KTD1, KTD3, KTD6; AE3, AE5).
//!
//! The real rung machinery on top of the chain-authorized rung (U1): evidence-verified
//! escalation, automatic de-escalation, rung-0 suspension + re-registration, and the rung
//! fraction reaching sizing.
//!
//! - **Escalation** (R13, F2): an explicit, operator-nonce'd request that verifies N clean
//!   sessions at the current rung (finalized, zero limit events, required reports present,
//!   non-probation, live-lane, matching strategy-code + governed-params hashes) and that
//!   cumulative P&L sits inside the rung's expectation band; it appends an escalation record
//!   citing the qualifying run ids, or names the missing evidence.
//! - **De-escalation** (R14, F3): a scan over three sources — finalized artifacts,
//!   safety-trip records, and `.tmp-` residue — steps the rung down one level per
//!   session-with-events (all events listed) and stamps a consumed-through watermark so no
//!   event double-fires. Residue classification is **chain-driven**: `.tmp-` matching a
//!   consumed live dispatch's recorded run id is a limit event (R14(f)); residue matching no
//!   consumed dispatch is not.
//! - **Rung 0** refuses live dispatch; re-entry / chain repair go through a nonce-gated,
//!   reason-recorded re-registration (KTD1).
//! - **Rung fraction → sizing** (KTD6): the fraction is a runner-supplied, dimensionless
//!   budget-numerator multiplier composed with the equity factor and the ratio-ATR tilt —
//!   never an `OrbParams`/manifest field, so a rung move produces zero head-identity diff.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use chrono::{DateTime, Utc};

use crate::artifacts::manifest::{hash_bytes, Manifest};
use crate::artifacts::{aborted_runs, list_runs};
use crate::dispatch::chain::{
    ChainRecord, DeEscalation, DispatchChain, Escalation, RecordKind, SafetyTripKind, TripAction,
};
use crate::dispatch::nonce::OperatorGate;
use crate::dispatch::prereg::PreRegistration;
use crate::dispatch::tracking::read_report;
use crate::params::OrbParams;
use crate::runner::research::read_manifest;

/// The governed-params hash (KTD3): a content hash over the full ORB parameter set. Two
/// runs sharing this hash sized on the identical governed parameters; a params-only head
/// change flips it, so old-params sessions no longer qualify (R13 — N resets).
pub fn governed_params_hash(params: &OrbParams) -> String {
    hash_bytes(&serde_json::to_vec(params).expect("OrbParams is always serializable"))
}

/// One limit event (R14) attributed to a session (by run id). Kept as typed strings so the
/// de-escalation record can list every event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitEvent {
    /// The session (run id) the event belongs to.
    pub run_id: String,
    /// The event kind (`watchdog_trip`, `breaker_trip`, `kill_switch`, `dedup_hit`,
    /// `teardown_retries`, `reconcile_advised`, `tracking_band_breach`, `tmp_residue`).
    pub kind: String,
}

impl LimitEvent {
    fn label(&self) -> String {
        format!("{}:{}", self.run_id, self.kind)
    }
}

/// The run ids a consumption marker recorded (the live sessions a green dispatch mounted).
/// Residue matching one of these is a live session that never finalized (R14(f)).
fn consumed_run_ids(chain_records: &[ChainRecord]) -> BTreeSet<String> {
    chain_records
        .iter()
        .filter_map(|r| match &r.body.kind {
            RecordKind::Consumption(c) => c.run_id.clone(),
            _ => None,
        })
        .collect()
}

/// The consumed-through watermark of the latest de-escalation (empty before the first).
/// Events with a run id at or below the watermark are already consumed and never re-fire.
fn deescalation_watermark(chain_records: &[ChainRecord]) -> String {
    chain_records
        .iter()
        .rev()
        .find_map(|r| match &r.body.kind {
            RecordKind::DeEscalation(d) => Some(d.consumed_through.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn read_dq(data_home: &Path, run_id: &str) -> Option<crate::artifacts::data_quality::DataQualityReport> {
    let path = data_home.join("runs").join(run_id).join(crate::artifacts::DATA_QUALITY_FILE);
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok())
}

fn read_perf(data_home: &Path, run_id: &str) -> Option<crate::artifacts::performance::PerformanceReport> {
    let path = data_home.join("runs").join(run_id).join(crate::artifacts::PERFORMANCE_FILE);
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok())
}

/// Whether a manifest is a live-lane session (dispatch linkage + live `trading_env`).
fn is_live_lane(m: &Manifest) -> bool {
    m.dispatch.as_ref().is_some_and(|d| d.trading_env.eq_ignore_ascii_case("live"))
}

/// The limit events a single finalized run's artifacts carry (R14 (a)/(b)/(c)/(d)/(e)).
/// The tracking-band breach (c) and the expectation band (e) are load-bearing only when the
/// rung's band is pre-registered (rung 2+, KD6).
fn run_limit_events(
    data_home: &Path,
    run_id: &str,
    rung: u8,
    prereg: Option<&PreRegistration>,
) -> Vec<LimitEvent> {
    let mut out = Vec::new();
    let ev = |kind: &str| LimitEvent { run_id: run_id.to_string(), kind: kind.to_string() };
    if let Some(dq) = read_dq(data_home, run_id) {
        if !dq.reconcile_advised.is_empty() {
            out.push(ev("reconcile_advised")); // (b) reconcile-Unknown outcome
        }
        if dq.dedup_hits.unwrap_or(0) > 0 {
            out.push(ev("dedup_hit")); // (d)
        }
        if dq.teardown_retries.unwrap_or(0) > 1 {
            out.push(ev("teardown_retries")); // (d) more than one retry
        }
    }
    // (c) tracking-error band breach (rung 2+, load-bearing only with a frozen band).
    if let (Some(values), Ok(Some(band))) =
        (prereg, prereg.map(|v| v.tracking_band(rung)).unwrap_or(Ok(None)))
    {
        let _ = values;
        if let Ok(Some(report)) = read_report(data_home, run_id) {
            if report.status.produced()
                && (report.max_abs_slippage_per_share > band.max_slippage_per_share
                    || report.approximated_fraction > band.max_approximated_fraction)
            {
                out.push(ev("tracking_band_breach"));
            }
        }
    }
    out
}

/// Scan all three limit-event sources (F3), excluding events already consumed by a prior
/// de-escalation (the watermark). Residue classification is chain-driven (R14(f)).
pub fn scan_limit_events(
    data_home: &Path,
    chain_records: &[ChainRecord],
    prereg: Option<&PreRegistration>,
) -> Vec<LimitEvent> {
    let watermark = deescalation_watermark(chain_records);
    let consumed = consumed_run_ids(chain_records);
    let mut events: Vec<LimitEvent> = Vec::new();

    // 1. Finalized live-lane runs' artifacts.
    for run_id in list_runs(data_home) {
        if run_id.as_str() <= watermark.as_str() {
            continue;
        }
        let Ok(manifest) = read_manifest(data_home, &run_id) else { continue };
        if !is_live_lane(&manifest) {
            continue;
        }
        let rung = manifest.dispatch.as_ref().map(|d| d.rung).unwrap_or(1);
        events.extend(run_limit_events(data_home, &run_id, rung, prereg));
    }

    // 2. Safety-trip engage records (watchdog / breaker / kill-switch), by run id.
    for r in chain_records {
        if let RecordKind::SafetyTrip(t) = &r.body.kind {
            if t.action != TripAction::Engage {
                continue;
            }
            let Some(run_id) = t.run_id.clone() else { continue };
            if run_id.as_str() <= watermark.as_str() {
                continue;
            }
            let kind = match t.trip {
                SafetyTripKind::Watchdog => "watchdog_trip",
                SafetyTripKind::Breaker => "breaker_trip",
                SafetyTripKind::KillSwitch => "kill_switch",
            };
            events.push(LimitEvent { run_id, kind: kind.to_string() });
        }
    }

    // 3. `.tmp-` residue that matches a consumed live dispatch's run id (R14(f)). Residue
    //    matching no consumed dispatch is NOT a limit event (a backtest leftover).
    for run_id in aborted_runs(data_home) {
        if run_id.as_str() <= watermark.as_str() {
            continue;
        }
        if consumed.contains(&run_id) {
            events.push(LimitEvent { run_id, kind: "tmp_residue".to_string() });
        }
    }

    events
}

/// Group limit events by session (run id), newest last.
fn sessions_with_events(events: &[LimitEvent]) -> BTreeMap<String, Vec<LimitEvent>> {
    let mut by_run: BTreeMap<String, Vec<LimitEvent>> = BTreeMap::new();
    for e in events {
        by_run.entry(e.run_id.clone()).or_default().push(e.clone());
    }
    by_run
}

/// Auto-de-escalate for any unconsumed limit events (F3, R14). Steps the rung down one
/// level per session-with-events (floored at rung 0 — the stopping rule, R14 at rung 1),
/// lists every event, and stamps a consumed-through watermark so no event double-fires.
/// Returns the appended de-escalation record, or `None` when there is nothing to consume.
///
/// # Errors
///
/// A chain-append failure.
pub fn apply_deescalation(
    chain: &DispatchChain,
    data_home: &Path,
    prereg: Option<&PreRegistration>,
    now: DateTime<Utc>,
) -> anyhow::Result<Option<ChainRecord>> {
    let state = chain.load();
    if state.authorized_rung == 0 {
        return Ok(None); // already suspended — nothing to step down
    }
    let events = scan_limit_events(data_home, &state.records, prereg);
    if events.is_empty() {
        return Ok(None);
    }
    let by_run = sessions_with_events(&events);
    let steps = by_run.len() as u8;
    let from_rung = state.authorized_rung;
    let to_rung = from_rung.saturating_sub(steps); // rung 0 = suspended (the stopping rule)
    let consumed_through = by_run.keys().last().cloned().unwrap_or_default();
    let all_events: Vec<String> = events.iter().map(LimitEvent::label).collect();

    let rec = chain.append(
        now,
        to_rung,
        to_rung,
        state.last_prereg_hash.clone(),
        RecordKind::DeEscalation(DeEscalation { from_rung, to_rung, events: all_events, consumed_through }),
    )?;
    Ok(Some(rec))
}

/// Whether a finalized run is a *clean* qualifying session at `from_rung` for an escalation
/// citing the head `(code_hash, params_hash)` (R13). Clean iff: finalized + live-lane, ran
/// at `from_rung` non-probation (the chain record's chain_rung == effective_rung ==
/// from_rung), zero limit events, required reports present (a produced tracking-error twin
/// at rung 2+), and matching strategy-code + governed-params hashes.
pub fn is_clean_session(
    data_home: &Path,
    chain_records: &[ChainRecord],
    run_id: &str,
    from_rung: u8,
    code_hash: &str,
    params_hash: &str,
    prereg: Option<&PreRegistration>,
) -> bool {
    let Ok(manifest) = read_manifest(data_home, run_id) else { return false };
    let Some(link) = &manifest.dispatch else { return false };
    if !link.trading_env.eq_ignore_ascii_case("live") {
        return false;
    }
    // Ran at from_rung, and NOT a probation session (chain_rung == effective_rung).
    let dispatch = chain_records.iter().find_map(|r| match &r.body.kind {
        RecordKind::SessionDispatch(_) if r.body.record_id == link.dispatch_id => Some(&r.body),
        _ => None,
    });
    let Some(body) = dispatch else { return false };
    if body.chain_rung != from_rung || body.effective_rung != from_rung {
        return false; // wrong rung, or a probation session (effective forced to 1)
    }
    // Head identity: matching strategy-code + governed-params hashes.
    if manifest.strategy_code_hash != code_hash || governed_params_hash(&manifest.params) != params_hash {
        return false;
    }
    // Zero limit events for this run (artifact-sourced).
    if !run_limit_events(data_home, run_id, from_rung, prereg).is_empty() {
        return false;
    }
    // A safety-tripped session is NEVER clean evidence — at ANY rung, not just rung 2+.
    // `run_limit_events` reads only artifacts, so the chain safety-trip records are checked
    // here explicitly. A rung-1 trip suspends the ladder to rung 0 (the stopping rule), so
    // that same session must not later count toward re-escalation out of rung 1.
    if chain_records.iter().any(|r| matches!(&r.body.kind,
        RecordKind::SafetyTrip(t) if t.action == TripAction::Engage && t.run_id.as_deref() == Some(run_id)))
    {
        return false;
    }
    // Required reports (rung 2+): a produced (non-failed) tracking-error twin.
    if from_rung >= 2 {
        match read_report(data_home, run_id) {
            Ok(Some(r)) if r.status.produced() => {}
            _ => return false,
        }
    }
    true
}

/// The result of verifying an escalation request (R13, AE5).
#[derive(Debug, Clone, PartialEq)]
pub enum EscalationCheck {
    /// Enough clean evidence — escalate to `to_rung`, citing these run ids.
    Ready {
        /// The rung to authorize.
        to_rung: u8,
        /// The qualifying run ids cited as evidence.
        evidence: Vec<String>,
    },
    /// Not enough evidence — the human-readable reason names what is missing (AE5).
    Blocked(String),
}

/// Verify an escalation request FROM `from_rung` to `from_rung + 1` (R13, F2, AE5): N clean
/// sessions at `from_rung` matching the head `(code_hash, params_hash)`, and cumulative P&L
/// inside the rung's expectation band (R14(e) — never escalate against a bleeding edge).
/// Reads pre-registration for N and the expectation band (both fail-closed).
pub fn verify_escalation(
    data_home: &Path,
    chain_records: &[ChainRecord],
    from_rung: u8,
    code_hash: &str,
    params_hash: &str,
    prereg: &PreRegistration,
) -> EscalationCheck {
    if from_rung == 0 {
        return EscalationCheck::Blocked(
            "rung 0 is suspended — re-entry is a re-registration, not an escalation".to_string(),
        );
    }
    if from_rung >= crate::dispatch::RUNG_MAX {
        return EscalationCheck::Blocked(format!("already at the top rung ({})", crate::dispatch::RUNG_MAX));
    }
    let n = match prereg.n_for_rung(from_rung) {
        // Fail-closed on a degenerate N=0: never authorize escalation on zero clean evidence,
        // even if an operator pre-registered it (config-trust floor).
        Ok(0) => {
            return EscalationCheck::Blocked(format!(
                "pre-registered N for rung {from_rung} is 0 — refusing to escalate on zero clean \
                 evidence (fail-closed floor)"
            ))
        }
        Ok(n) => n as usize,
        Err(e) => return EscalationCheck::Blocked(e.to_string()),
    };
    let clean: Vec<String> = list_runs(data_home)
        .into_iter()
        .filter(|rid| is_clean_session(data_home, chain_records, rid, from_rung, code_hash, params_hash, Some(prereg)))
        .collect();
    if clean.len() < n {
        return EscalationCheck::Blocked(format!(
            "escalation from rung {from_rung} needs {n} clean session(s) at the current head; found {} \
             (missing {})",
            clean.len(),
            n - clean.len()
        ));
    }
    // Expectation band (R14(e)): cumulative realized P&L across the qualifying sessions.
    match prereg.expectation_band(from_rung) {
        Ok(band) => {
            let cum: f64 = clean
                .iter()
                .filter_map(|rid| read_perf(data_home, rid))
                .flat_map(|p| p.trades.into_iter().map(|t| t.realized_pnl))
                .sum();
            if cum < band.min_cum_pnl || cum > band.max_cum_pnl {
                return EscalationCheck::Blocked(format!(
                    "cumulative P&L {cum:.0} is outside the rung {from_rung} expectation band \
                     [{:.0}, {:.0}] — operational cleanliness never authorizes escalation against a \
                     bleeding edge (R14(e))",
                    band.min_cum_pnl, band.max_cum_pnl
                ));
            }
        }
        Err(e) => return EscalationCheck::Blocked(e.to_string()),
    }
    EscalationCheck::Ready { to_rung: from_rung + 1, evidence: clean }
}

/// Run an operator-requested escalation (F2): nonce-gated, verifies the evidence, and
/// appends an escalation record citing the qualifying run ids — or returns the blocking
/// reason (AE5).
///
/// # Errors
///
/// A nonce refusal, a blocked verification (named evidence gap), or a chain-append failure.
pub fn run_escalation(
    chain: &DispatchChain,
    data_home: &Path,
    gate: &OperatorGate,
    prereg: &PreRegistration,
    now: DateTime<Utc>,
) -> anyhow::Result<ChainRecord> {
    gate.authorize("rung escalation").map_err(|e| anyhow::anyhow!(e))?;
    let state = chain.load();
    let from_rung = state.authorized_rung;
    let code_hash = crate::artifacts::manifest::strategy_code_hash();
    let params_hash = governed_params_hash(&OrbParams::default());
    match verify_escalation(data_home, &state.records, from_rung, &code_hash, &params_hash, prereg) {
        EscalationCheck::Ready { to_rung, evidence } => {
            let rec = chain.append(
                now,
                to_rung,
                to_rung,
                state.last_prereg_hash.clone(),
                RecordKind::Escalation(Escalation { from_rung, to_rung, evidence_run_ids: evidence }),
            )?;
            Ok(rec)
        }
        EscalationCheck::Blocked(reason) => anyhow::bail!("escalation refused: {reason}"),
    }
}

/// Run a nonce-gated re-registration (KTD1): a rung-0 re-qualification or a chain-epoch
/// repair after a defect. Delegates to [`DispatchChain::reregister`] (which archives a
/// defective epoch content-hashed and opens a fresh one) with the operator reason scrubbed
/// before it lands.
///
/// # Errors
///
/// A nonce refusal or a chain-append/rollover failure.
pub fn run_reregistration(
    chain: &DispatchChain,
    gate: &OperatorGate,
    set_rung: u8,
    reason: &str,
    prereg_hash: Option<String>,
    now: DateTime<Utc>,
) -> anyhow::Result<ChainRecord> {
    gate.authorize("chain re-registration").map_err(|e| anyhow::anyhow!(e))?;
    chain.reregister(now, set_rung, prereg_hash, reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::data_quality::DataQualityReport;
    use crate::artifacts::manifest::{universe_hash, DataRange, DispatchLink};
    use crate::artifacts::performance::{PerformanceReport, TradeRecord};
    use crate::artifacts::{RunSource, RunWriter};
    use crate::dispatch::chain::{SafetyTrip, SessionDispatch, DispatchOutcome};
    use crate::dispatch::tracking::{write_report, TrackingErrorReport, TwinStatus};
    use tempfile::TempDir;

    fn now() -> DateTime<Utc> {
        Utc.timestamp_opt(1_752_600_000, 0).unwrap()
    }
    use chrono::TimeZone;

    fn attended(nonce: i64) -> OperatorGate {
        OperatorGate { unattended_marker: None, nonce: Some(nonce.to_string()), now_unix: nonce }
    }

    fn prereg(json: serde_json::Value) -> PreRegistration {
        serde_json::from_value(json).unwrap()
    }

    /// Append a green session-dispatch at `chain_rung`/`effective_rung` and return its id.
    fn dispatch_record(chain: &DispatchChain, chain_rung: u8, effective_rung: u8) -> String {
        let rec = chain
            .append(
                now(),
                chain_rung,
                effective_rung,
                None,
                RecordKind::SessionDispatch(SessionDispatch {
                    outcome: DispatchOutcome::Green,
                    checks: Vec::new(),
                    deferrals: Vec::new(),
                    readiness: None,
                }),
            )
            .unwrap();
        rec.body.record_id
    }

    /// Stage a finalized live-lane run at `rung` bound to `dispatch_id`, with `realized_pnl`
    /// and optional dedup hits; writes a produced tracking twin for rung 2+.
    fn stage_clean_run(data_home: &Path, run_id: &str, rung: u8, dispatch_id: &str, realized_pnl: f64) {
        let params = OrbParams::default();
        let writer = RunWriter::new(data_home, run_id).unwrap();
        let manifest = Manifest {
            run_id: run_id.into(),
            source: RunSource::Live,
            strategy_id: params.strategy_id.clone(),
            strategy_version: params.strategy_version,
            params: params.clone(),
            data_range: DataRange { start: "20260716".into(), end: "20260716".into() },
            catalog_fingerprint: String::new(),
            universe_hash: universe_hash(&[]),
            strategy_code_hash: crate::artifacts::manifest::strategy_code_hash(),
            checkpoint_hash: None,
            universe_metadata_hash: None,
            dispatch: Some(DispatchLink {
                dispatch_id: dispatch_id.into(),
                rung,
                rung_fraction: 0.5,
                lane: "cafef00d".into(),
                trading_env: "live".into(),
            }),
            created_utc: "2026-07-16T01:00:00Z".into(),
        };
        writer.write_manifest(&manifest).unwrap();
        let trade = TradeRecord {
            symbol: "005930.XKRX".into(),
            entry_side: "BUY".into(),
            quantity: 10.0,
            avg_px_open: 60_000.0,
            avg_px_close: Some(60_000.0 + realized_pnl / 10.0),
            realized_pnl,
            ts_opened: 1,
            ts_closed: Some(2),
            fills: Vec::new(),
            risk_capital: None,
            realized_r: None,
        };
        writer.write_performance(&PerformanceReport::assemble(vec![trade], 1_000_000.0)).unwrap();
        let mut dq = DataQualityReport::backtest(vec![], vec![]);
        dq.teardown_retries = Some(0);
        dq.dedup_hits = Some(0);
        writer.write_data_quality(&dq).unwrap();
        writer.finalize().unwrap();
        if rung >= 2 {
            write_report(
                data_home,
                &TrackingErrorReport {
                    run_id: run_id.into(),
                    rung,
                    status: TwinStatus::Computed,
                    entries: 1,
                    mean_slippage_per_share: 0.0,
                    max_abs_slippage_per_share: 0.0,
                    approximated_fraction: 0.0,
                    per_symbol: Vec::new(),
                },
            )
            .unwrap();
        }
    }

    fn rung1_prereg() -> PreRegistration {
        prereg(serde_json::json!({
            "version": 1,
            "rungs": [
                { "rung": 1, "fraction": 0.1, "n_clean_sessions": 2,
                  "expectation_band": { "min_cum_pnl": -100000.0, "max_cum_pnl": 1000000.0 } },
                { "rung": 2, "fraction": 0.25, "n_clean_sessions": 2,
                  "tracking_band": { "max_slippage_per_share": 10.0, "max_approximated_fraction": 0.2 },
                  "expectation_band": { "min_cum_pnl": 0.0, "max_cum_pnl": 1000000.0 } }
            ]
        }))
    }

    #[test]
    fn governed_params_hash_is_stable_and_flips_on_a_param_change() {
        let a = governed_params_hash(&OrbParams::default());
        let b = governed_params_hash(&OrbParams::default());
        assert_eq!(a, b, "stable for identical params");
        let mut p = OrbParams::default();
        p.risk_per_trade_krw += 1.0;
        assert_ne!(a, governed_params_hash(&p), "a params change flips the hash");
    }

    #[test]
    fn escalation_refuses_below_n_and_names_the_gap() {
        // Covers AE5: N-1 clean sessions → refused, output names the missing evidence.
        let tmp = TempDir::new().unwrap();
        let chain = DispatchChain::open(tmp.path()).unwrap();
        chain.append(now(), 1, 1, None, RecordKind::Genesis).unwrap();
        // One clean session at rung 1 (N=2 required).
        let d = dispatch_record(&chain, 1, 1);
        stage_clean_run(tmp.path(), "20260716T010000Z-live-orb-v30", 1, &d, 500.0);
        let check = verify_escalation(
            tmp.path(),
            &chain.load().records,
            1,
            &crate::artifacts::manifest::strategy_code_hash(),
            &governed_params_hash(&OrbParams::default()),
            &rung1_prereg(),
        );
        match check {
            EscalationCheck::Blocked(reason) => assert!(reason.contains("needs 2"), "{reason}"),
            other => panic!("expected blocked, got {other:?}"),
        }
    }

    #[test]
    fn escalation_ready_with_n_clean_sessions_in_band() {
        let tmp = TempDir::new().unwrap();
        let chain = DispatchChain::open(tmp.path()).unwrap();
        chain.append(now(), 1, 1, None, RecordKind::Genesis).unwrap();
        for h in ["01", "02"] {
            let d = dispatch_record(&chain, 1, 1);
            stage_clean_run(tmp.path(), &format!("20260716T{h}0000Z-live-orb-v30"), 1, &d, 500.0);
        }
        let check = verify_escalation(
            tmp.path(),
            &chain.load().records,
            1,
            &crate::artifacts::manifest::strategy_code_hash(),
            &governed_params_hash(&OrbParams::default()),
            &rung1_prereg(),
        );
        assert!(matches!(check, EscalationCheck::Ready { to_rung: 2, .. }), "{check:?}");
    }

    #[test]
    fn a_probation_session_never_counts_as_qualifying() {
        let tmp = TempDir::new().unwrap();
        let chain = DispatchChain::open(tmp.path()).unwrap();
        chain.append(now(), 1, 1, None, RecordKind::Genesis).unwrap();
        // Escalate to rung 2 so a rung-2 chain exists.
        chain.append(now(), 2, 2, None, RecordKind::Escalation(Escalation { from_rung: 1, to_rung: 2, evidence_run_ids: vec![] })).unwrap();
        // A probation session at rung 2: chain_rung 2 but effective_rung 1.
        let d = dispatch_record(&chain, 2, 1);
        stage_clean_run(tmp.path(), "20260716T010000Z-live-orb-v30", 1, &d, 500.0);
        assert!(
            !is_clean_session(tmp.path(), &chain.load().records, "20260716T010000Z-live-orb-v30", 2,
                &crate::artifacts::manifest::strategy_code_hash(), &governed_params_hash(&OrbParams::default()), Some(&rung1_prereg())),
            "a probation session is never qualifying evidence"
        );
    }

    #[test]
    fn a_params_change_disqualifies_old_sessions() {
        let tmp = TempDir::new().unwrap();
        let chain = DispatchChain::open(tmp.path()).unwrap();
        chain.append(now(), 1, 1, None, RecordKind::Genesis).unwrap();
        let d = dispatch_record(&chain, 1, 1);
        stage_clean_run(tmp.path(), "20260716T010000Z-live-orb-v30", 1, &d, 500.0);
        // A different governed-params head → the old session no longer qualifies (N resets).
        let mut changed = OrbParams::default();
        changed.risk_per_trade_krw += 1.0;
        assert!(!is_clean_session(tmp.path(), &chain.load().records, "20260716T010000Z-live-orb-v30", 1,
            &crate::artifacts::manifest::strategy_code_hash(), &governed_params_hash(&changed), Some(&rung1_prereg())));
    }

    #[test]
    fn a_safety_tripped_session_is_not_clean_evidence_even_at_rung_1() {
        // Regression: the safety-trip exclusion must apply at ALL rungs, not just rung 2+.
        // A rung-1 trip suspends to rung 0 (stopping rule), so that session must never later
        // count toward re-escalation out of rung 1.
        let tmp = TempDir::new().unwrap();
        let chain = DispatchChain::open(tmp.path()).unwrap();
        chain.append(now(), 1, 1, None, RecordKind::Genesis).unwrap();
        let d = dispatch_record(&chain, 1, 1);
        let run = "20260716T010000Z-live-orb-v30";
        stage_clean_run(tmp.path(), run, 1, &d, 500.0); // artifact-clean
        // A benign watchdog trip on that run lands in the chain (clean artifacts, tripped).
        chain
            .append(now(), 1, 1, None, RecordKind::SafetyTrip(SafetyTrip {
                trip: SafetyTripKind::Watchdog,
                action: TripAction::Engage,
                run_id: Some(run.into()),
                detail: "network blip".into(),
            }))
            .unwrap();
        assert!(
            !is_clean_session(tmp.path(), &chain.load().records, run, 1,
                &crate::artifacts::manifest::strategy_code_hash(), &governed_params_hash(&OrbParams::default()), None),
            "a safety-tripped rung-1 session is never clean escalation evidence"
        );
    }

    #[test]
    fn a_strategy_code_hash_change_disqualifies_old_sessions() {
        // R13 head-change: a strategy-code-hash change returns the ladder to rung 1 — old
        // sessions at the prior code head no longer qualify.
        let tmp = TempDir::new().unwrap();
        let chain = DispatchChain::open(tmp.path()).unwrap();
        chain.append(now(), 1, 1, None, RecordKind::Genesis).unwrap();
        let d = dispatch_record(&chain, 1, 1);
        let run = "20260716T010000Z-live-orb-v30";
        stage_clean_run(tmp.path(), run, 1, &d, 500.0);
        assert!(
            !is_clean_session(tmp.path(), &chain.load().records, run, 1,
                "a-different-code-hash", &governed_params_hash(&OrbParams::default()), None),
            "an off-code-head session is not qualifying"
        );
    }

    #[test]
    fn escalation_refuses_a_degenerate_zero_n() {
        let tmp = TempDir::new().unwrap();
        let chain = DispatchChain::open(tmp.path()).unwrap();
        chain.append(now(), 1, 1, None, RecordKind::Genesis).unwrap();
        let pre = prereg(serde_json::json!({
            "version": 1,
            "rungs": [{ "rung": 1, "n_clean_sessions": 0,
                        "expectation_band": { "min_cum_pnl": -1.0, "max_cum_pnl": 1.0 } }]
        }));
        let check = verify_escalation(
            tmp.path(), &chain.load().records, 1,
            &crate::artifacts::manifest::strategy_code_hash(), &governed_params_hash(&OrbParams::default()), &pre,
        );
        assert!(matches!(check, EscalationCheck::Blocked(_)), "N=0 is fail-closed, not auto-escalate: {check:?}");
    }

    #[test]
    fn a_benign_watchdog_trip_de_escalates_one_rung() {
        // Covers AE3: a benign watchdog firing → next dispatch authorizes one rung lower.
        let tmp = TempDir::new().unwrap();
        let chain = DispatchChain::open(tmp.path()).unwrap();
        chain.append(now(), 1, 1, None, RecordKind::Genesis).unwrap();
        chain.append(now(), 2, 2, None, RecordKind::Escalation(Escalation { from_rung: 1, to_rung: 2, evidence_run_ids: vec![] })).unwrap();
        // A safety-trip (watchdog) at run-X.
        chain.append(now(), 2, 2, None, RecordKind::SafetyTrip(SafetyTrip {
            trip: SafetyTripKind::Watchdog, action: TripAction::Engage,
            run_id: Some("20260716T090000Z-live-orb-v30".into()), detail: "network blip".into(),
        })).unwrap();
        let rec = apply_deescalation(&chain, tmp.path(), None, now()).unwrap().unwrap();
        assert!(matches!(rec.body.kind, RecordKind::DeEscalation(_)));
        assert_eq!(chain.load().authorized_rung, 1, "stepped down one rung");
        // A following dispatch does not step down again (watermark consumed).
        assert!(apply_deescalation(&chain, tmp.path(), None, now()).unwrap().is_none());
    }

    #[test]
    fn two_events_in_one_session_step_one_rung_and_list_both() {
        let tmp = TempDir::new().unwrap();
        let chain = DispatchChain::open(tmp.path()).unwrap();
        chain.append(now(), 1, 1, None, RecordKind::Genesis).unwrap();
        chain.append(now(), 3, 3, None, RecordKind::Escalation(Escalation { from_rung: 1, to_rung: 3, evidence_run_ids: vec![] })).unwrap();
        let run = "20260716T090000Z-live-orb-v30";
        // Two safety trips on the SAME run → one session-with-events.
        for kind in [SafetyTripKind::Watchdog, SafetyTripKind::Breaker] {
            chain.append(now(), 3, 3, None, RecordKind::SafetyTrip(SafetyTrip {
                trip: kind, action: TripAction::Engage, run_id: Some(run.into()), detail: "x".into(),
            })).unwrap();
        }
        let rec = apply_deescalation(&chain, tmp.path(), None, now()).unwrap().unwrap();
        if let RecordKind::DeEscalation(d) = &rec.body.kind {
            assert_eq!(d.from_rung, 3);
            assert_eq!(d.to_rung, 2, "one rung step for one session, not two");
            assert_eq!(d.events.len(), 2, "both events listed");
        } else {
            panic!("expected de-escalation");
        }
    }

    #[test]
    fn tmp_residue_de_escalates_only_when_it_matches_a_consumed_dispatch() {
        let tmp = TempDir::new().unwrap();
        let chain = DispatchChain::open(tmp.path()).unwrap();
        chain.append(now(), 1, 1, None, RecordKind::Genesis).unwrap();
        chain.append(now(), 2, 2, None, RecordKind::Escalation(Escalation { from_rung: 1, to_rung: 2, evidence_run_ids: vec![] })).unwrap();
        std::fs::create_dir_all(tmp.path().join("runs")).unwrap();
        // A backtest .tmp- leftover matching no consumed dispatch → NOT a limit event.
        std::fs::create_dir_all(tmp.path().join("runs").join(".tmp-20260716T080000Z-backtest-orb-v30")).unwrap();
        assert!(apply_deescalation(&chain, tmp.path(), None, now()).unwrap().is_none(), "backtest residue is not a limit event");

        // A consumed live dispatch whose run never finalized → its .tmp- residue IS R14(f).
        let d = dispatch_record(&chain, 2, 2);
        let live_run = "20260716T090000Z-live-orb-v30";
        chain.append(now(), 2, 2, None, RecordKind::Consumption(crate::dispatch::chain::Consumption {
            dispatch_record_id: d, run_id: Some(live_run.into()),
        })).unwrap();
        std::fs::create_dir_all(tmp.path().join("runs").join(format!(".tmp-{live_run}"))).unwrap();
        let rec = apply_deescalation(&chain, tmp.path(), None, now()).unwrap();
        assert!(rec.is_some(), "consumed-dispatch residue de-escalates (R14(f))");
        assert_eq!(chain.load().authorized_rung, 1);
    }

    #[test]
    fn a_limit_event_at_rung_1_suspends_to_rung_0() {
        let tmp = TempDir::new().unwrap();
        let chain = DispatchChain::open(tmp.path()).unwrap();
        chain.append(now(), 1, 1, None, RecordKind::Genesis).unwrap();
        chain.append(now(), 1, 1, None, RecordKind::SafetyTrip(SafetyTrip {
            trip: SafetyTripKind::Breaker, action: TripAction::Engage,
            run_id: Some("20260716T090000Z-live-orb-v30".into()), detail: "max loss".into(),
        })).unwrap();
        apply_deescalation(&chain, tmp.path(), None, now()).unwrap().unwrap();
        assert_eq!(chain.load().authorized_rung, 0, "rung-1 limit event suspends to rung 0 (stopping rule)");
    }

    #[test]
    fn reregistration_is_nonce_gated_and_scrubs_the_reason() {
        let tmp = TempDir::new().unwrap();
        let chain = DispatchChain::open(tmp.path()).unwrap();
        chain.append(now(), 1, 1, None, RecordKind::Genesis).unwrap();
        // Suspend to rung 0.
        chain.append(now(), 0, 0, None, RecordKind::DeEscalation(DeEscalation {
            from_rung: 1, to_rung: 0, events: vec!["x".into()], consumed_through: "z".into(),
        })).unwrap();
        assert_eq!(chain.load().authorized_rung, 0);
        // Unattended → refused.
        let unattended = OperatorGate { unattended_marker: Some("CI".into()), nonce: Some("1752600000".into()), now_unix: 1_752_600_000 };
        assert!(run_reregistration(&chain, &unattended, 1, "requalified acct 20187511401", None, now()).is_err());
        // Attended + fresh nonce → re-registers at rung 1; the secret is scrubbed.
        run_reregistration(&chain, &attended(1_752_600_000), 1, "requalified acct 20187511401", None, now()).unwrap();
        assert_eq!(chain.load().authorized_rung, 1);
        let bytes = std::fs::read_to_string(chain.chain_path()).unwrap();
        assert!(!bytes.contains("20187511401"), "re-registration reason scrubbed: {bytes}");
    }

    #[test]
    fn full_ladder_walk_genesis_up_and_down_and_repair() {
        let tmp = TempDir::new().unwrap();
        let chain = DispatchChain::open(tmp.path()).unwrap();
        chain.append(now(), 1, 1, None, RecordKind::Genesis).unwrap();
        let pre = rung1_prereg();

        // Rung 1 → 2: two clean sessions in band, escalate.
        for h in ["01", "02"] {
            let d = dispatch_record(&chain, 1, 1);
            stage_clean_run(tmp.path(), &format!("20260716T{h}0000Z-live-orb-v30"), 1, &d, 500.0);
        }
        run_escalation(&chain, tmp.path(), &attended(1_752_600_000), &pre, now()).unwrap();
        assert_eq!(chain.load().authorized_rung, 2, "escalated to rung 2");

        // A limit event at rung 2 de-escalates to rung 1.
        chain.append(now(), 2, 2, None, RecordKind::SafetyTrip(SafetyTrip {
            trip: SafetyTripKind::Watchdog, action: TripAction::Engage,
            run_id: Some("20260716T120000Z-live-orb-v30".into()), detail: "blip".into(),
        })).unwrap();
        apply_deescalation(&chain, tmp.path(), Some(&pre), now()).unwrap().unwrap();
        assert_eq!(chain.load().authorized_rung, 1);
    }
}
