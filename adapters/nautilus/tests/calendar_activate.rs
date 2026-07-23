//! U15 activation: revalidate candidate + predecessor, record approval, ATOMIC install
//! (owner-readable 0o600), refusing stale-base / invalid / unreviewed / absence-driven
//! destructive candidates. Plus the publication boundary + expired-authorization reference.
//! All synthetic, offline, fixed-clock.

use std::os::unix::fs::PermissionsExt;

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc, Weekday};
use nautilus_ls::calendar_refresh::{
    activate, build_genesis, describe_genesis, first_install, genesis_description_path_for, refresh,
    required_acknowledgments, rollback, write_candidate, write_genesis, ActivationApproval,
    ActivationError, DateRange, FirstInstallError, GenesisActivationRecord, GenesisDescription,
    GenesisParams, RefreshInputs, RefreshMode, RefreshScope, RollbackError, SourceOutcome,
    StaticEvidencePort, GENESIS_ACK_KEY,
};
use nautilus_ls_calendar::schema::{
    Authorization, CalendarScope, Citation, Coverage, DayRow, DayStatus, EvidenceKind,
    EvidenceRecord, Freshness, Snapshot, Source, SourceAvailabilityBound, SourceKind,
};
use nautilus_ls_calendar::{compute_artifact_id, compute_calendar_id, CalendarLoadError, KrxCalendar};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn t(y: i32, m: u32, day: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, day, 0, 0, 0).unwrap()
}

fn ev(id: &str, source_id: &str, date: NaiveDate, kind: EvidenceKind, cited: bool) -> EvidenceRecord {
    EvidenceRecord {
        id: id.to_string(),
        source_id: source_id.to_string(),
        date,
        kind,
        valid: true,
        superseded_by: None,
        citation: cited.then(|| Citation {
            reference: "NOTICE-1".to_string(),
            issuer: "KRX".to_string(),
            note: None,
        }),
        recorded_at: t(2012, 5, 1),
    }
}

fn src(id: &str, kind: SourceKind) -> Source {
    Source {
        id: id.to_string(),
        kind,
        label: format!("{id} (SYNTHETIC)"),
        synthetic: true,
    }
}

fn stamp(mut snap: Snapshot) -> Snapshot {
    snap.artifact_id = compute_artifact_id(&snap);
    snap.calendar_id = compute_calendar_id(&snap);
    snap
}

/// A prior (active predecessor) over 2012-06-01..2012-06-05: 06-01 Closed (holiday+rule),
/// 06-04 TradingSession (witness), rest Unknown.
fn prior_snapshot() -> Snapshot {
    let from = d(2012, 6, 1);
    let through = d(2012, 6, 5);
    let sources = vec![
        src("krx-daily", SourceKind::KrxDailyMarket),
        src("kasi", SourceKind::KasiHoliday),
        src("krx-rule", SourceKind::KrxRule),
    ];
    let evidence = vec![
        ev("kasi-0601", "kasi", d(2012, 6, 1), EvidenceKind::HolidayFact, false),
        ev("rule-0601", "krx-rule", d(2012, 6, 1), EvidenceKind::DeterministicRule, false),
        ev("witness-0604", "krx-daily", d(2012, 6, 4), EvidenceKind::PositiveWitness, false),
    ];
    let mut rows = Vec::new();
    let mut cursor = from;
    while cursor <= through {
        let (status, decisive) = match cursor {
            x if x == d(2012, 6, 1) => (
                DayStatus::Closed,
                vec!["kasi-0601".to_string(), "rule-0601".to_string()],
            ),
            x if x == d(2012, 6, 4) => (DayStatus::TradingSession, vec!["witness-0604".to_string()]),
            _ => (DayStatus::Unknown, vec![]),
        };
        rows.push(DayRow {
            date: cursor,
            status,
            decisive_evidence: decisive,
            conflicting_evidence: vec![],
            alerts: vec![],
        });
        cursor = cursor.succ_opt().unwrap();
    }
    stamp(base_snapshot(from, through, sources, evidence, rows, Some(t(2099, 1, 1))))
}

#[allow(clippy::too_many_arguments)]
fn base_snapshot(
    from: NaiveDate,
    through: NaiveDate,
    sources: Vec<Source>,
    evidence: Vec<EvidenceRecord>,
    rows: Vec<DayRow>,
    expires_at: Option<DateTime<Utc>>,
) -> Snapshot {
    Snapshot {
        schema_version: "1.0.0".to_string(),
        artifact_id: String::new(),
        calendar_id: String::new(),
        predecessor_artifact_id: None,
        scope: CalendarScope {
            calendar_name: "KRX domestic equity (SYNTHETIC)".to_string(),
            venue: "XKRX".to_string(),
            instrument_class: "domestic-equity".to_string(),
            timezone: "Asia/Seoul".to_string(),
            synthetic: true,
        },
        authorization: Authorization {
            authorized: true,
            authority: "Synthetic Authority".to_string(),
            granted_at: t(2010, 1, 1),
            expires_at,
            terminated_at: None,
        },
        coverage: Coverage {
            materialized_from: from,
            materialized_through: through,
            retrospectively_checked_through: through,
            scheduled_closure_evaluated_through: through,
            source_availability: vec![SourceAvailabilityBound {
                source_id: "krx-daily".to_string(),
                available_from: Some(d(2010, 1, 4)),
                available_through: None,
            }],
        },
        freshness: Freshness {
            evidence_refreshed_at: t(2012, 5, 20),
            holiday_facts_checked_at: Some(t(2012, 5, 20)),
            full_history_reconciled_at: Some(t(2012, 5, 1)),
            forward_readiness_through: Some(d(2012, 7, 15)),
            last_incremental_at: Some(t(2012, 5, 20)),
        },
        sources,
        evidence,
        alerts: vec![],
        rows,
    }
}

fn refresh_now() -> DateTime<Utc> {
    t(2012, 6, 6)
}

fn horizon() -> (NaiveDate, NaiveDate) {
    (d(2012, 5, 30), d(2012, 7, 20))
}

fn full_scope() -> RefreshScope {
    RefreshScope {
        from: d(2012, 6, 1),
        through: d(2012, 6, 5),
    }
}

/// Additive inputs: the krx-daily source re-supplies BOTH prior witnesses AND a new one for
/// 06-03 → the candidate only ESTABLISHES a previously-Unknown date (no high-risk removal).
fn ok_inputs_additive() -> RefreshInputs {
    RefreshInputs {
        sources: vec![src("krx-daily", SourceKind::KrxDailyMarket)],
        evidence: vec![
            ev("witness-0604", "krx-daily", d(2012, 6, 4), EvidenceKind::PositiveWitness, false),
            ev("witness-0603", "krx-daily", d(2012, 6, 3), EvidenceKind::PositiveWitness, false),
        ],
        outcomes: vec![SourceOutcome::ok("krx-daily", SourceKind::KrxDailyMarket)],
    }
}

/// Absence-driven destructive inputs: the krx-daily source succeeds but returns NOTHING, so
/// the prior 06-04 witness is removed → 06-04 falls back to Unknown (evidence removal +
/// historical status change + transition-to-Unknown — all HIGH-RISK).
fn absence_removal_inputs() -> RefreshInputs {
    RefreshInputs {
        sources: vec![src("krx-daily", SourceKind::KrxDailyMarket)],
        evidence: vec![],
        outcomes: vec![SourceOutcome::ok("krx-daily", SourceKind::KrxDailyMarket)],
    }
}

fn approval(reviewed_artifact_id: &str, acknowledged: Vec<String>) -> ActivationApproval {
    ActivationApproval {
        operator: "maintainer-1".to_string(),
        reason: "routine reviewed activation".to_string(),
        approved_at: t(2012, 6, 6),
        reviewed_artifact_id: reviewed_artifact_id.to_string(),
        acknowledged,
    }
}

/// Write `prior` to an active path and stage a candidate + diff from `inputs`. Returns the
/// active path, candidate path, and the refresh outcome.
fn stage(
    dir: &std::path::Path,
    prior: &Snapshot,
    inputs: RefreshInputs,
) -> (std::path::PathBuf, std::path::PathBuf, nautilus_ls::calendar_refresh::RefreshOutcome) {
    let active_path = dir.join("cal.json");
    std::fs::write(&active_path, serde_json::to_vec_pretty(prior).unwrap()).unwrap();
    let outcome = refresh(
        prior,
        &StaticEvidencePort::new(inputs),
        full_scope(),
        RefreshMode::Incremental,
        refresh_now(),
        horizon(),
    );
    let artifacts = write_candidate(&active_path, &outcome).unwrap();
    (active_path, artifacts.candidate_path, outcome)
}

#[test]
fn happy_valid_reviewed_candidate_records_approval_and_atomically_installs() {
    let dir = tempfile::tempdir().unwrap();
    let prior = prior_snapshot();
    let (active_path, candidate_path, outcome) = stage(dir.path(), &prior, ok_inputs_additive());

    // A clean additive candidate requires no high-risk acknowledgements.
    assert!(
        required_acknowledgments(&outcome.diff).is_empty(),
        "additive candidate should be low-risk: {:?}",
        outcome.diff.categories()
    );

    let approval = approval(&outcome.candidate.artifact_id, vec![]);
    let record = activate(&active_path, &candidate_path, &approval, refresh_now()).unwrap();

    // The old active becomes the recorded predecessor; the candidate is the new active.
    assert_eq!(record.predecessor_artifact_id, prior.artifact_id);
    assert_eq!(record.candidate_artifact_id, outcome.candidate.artifact_id);
    assert_eq!(record.operator, "maintainer-1");

    // The install is atomic: the active bytes are now byte-identical to the candidate.
    let installed = std::fs::read(&active_path).unwrap();
    let candidate_bytes = std::fs::read(&candidate_path).unwrap();
    assert_eq!(installed, candidate_bytes, "candidate must be installed verbatim");

    // The new active loads through the real loader with the candidate identity.
    let cal = KrxCalendar::load_from_path(&active_path, refresh_now()).unwrap();
    assert_eq!(cal.artifact_id(), outcome.candidate.artifact_id);
}

#[test]
fn installed_snapshot_is_owner_readable_only_0o600() {
    let dir = tempfile::tempdir().unwrap();
    let prior = prior_snapshot();
    let (active_path, candidate_path, outcome) = stage(dir.path(), &prior, ok_inputs_additive());
    let approval = approval(&outcome.candidate.artifact_id, vec![]);
    activate(&active_path, &candidate_path, &approval, refresh_now()).unwrap();

    let mode = std::fs::metadata(&active_path).unwrap().permissions().mode();
    assert_eq!(
        mode & 0o777,
        0o600,
        "installed snapshot must be owner read/write only, got {:o}",
        mode & 0o777
    );
}

#[test]
fn stale_base_predecessor_mismatch_is_refused_active_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let prior = prior_snapshot();
    let (active_path, candidate_path, outcome) = stage(dir.path(), &prior, ok_inputs_additive());

    // Someone else already activated a DIFFERENT snapshot: overwrite the active file so its
    // identity no longer matches the candidate's declared predecessor.
    let mut other = prior.clone();
    other.scope.calendar_name = "KRX domestic equity (SYNTHETIC, moved on)".to_string();
    let other = stamp(other);
    std::fs::write(&active_path, serde_json::to_vec_pretty(&other).unwrap()).unwrap();
    let active_before = std::fs::read(&active_path).unwrap();

    let approval = approval(&outcome.candidate.artifact_id, vec![]);
    let err = activate(&active_path, &candidate_path, &approval, refresh_now()).unwrap_err();
    assert!(matches!(err, ActivationError::StaleBase { .. }), "got {err:?}");

    let active_after = std::fs::read(&active_path).unwrap();
    assert_eq!(active_before, active_after, "a refusal must leave the active file unchanged");
}

#[test]
fn invalid_candidate_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let prior = prior_snapshot();
    let (active_path, candidate_path, outcome) = stage(dir.path(), &prior, ok_inputs_additive());
    let active_before = std::fs::read(&active_path).unwrap();

    // Tamper the candidate file AFTER stamping: flip a row status without recomputing the
    // identity → the declared artifact_id no longer matches content (HashMismatch).
    let mut tampered = outcome.candidate.clone();
    tampered.rows[0].status = DayStatus::Unknown;
    tampered.rows[0].decisive_evidence.clear();
    std::fs::write(&candidate_path, serde_json::to_vec_pretty(&tampered).unwrap()).unwrap();

    let approval = approval(&outcome.candidate.artifact_id, vec![]);
    let err = activate(&active_path, &candidate_path, &approval, refresh_now()).unwrap_err();
    assert!(
        matches!(err, ActivationError::Invalid(CalendarLoadError::HashMismatch { .. })),
        "got {err:?}"
    );
    assert_eq!(active_before, std::fs::read(&active_path).unwrap());
}

#[test]
fn unreviewed_candidate_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let prior = prior_snapshot();
    let (active_path, candidate_path, _outcome) = stage(dir.path(), &prior, ok_inputs_additive());
    let active_before = std::fs::read(&active_path).unwrap();

    // The approval names a DIFFERENT artifact_id — approving one candidate cannot rubber-stamp
    // this one.
    let approval = approval("some-other-candidate-artifact-id", vec![]);
    let err = activate(&active_path, &candidate_path, &approval, refresh_now()).unwrap_err();
    assert!(matches!(err, ActivationError::Unreviewed { .. }), "got {err:?}");
    assert_eq!(active_before, std::fs::read(&active_path).unwrap());
}

#[test]
fn blank_approval_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let prior = prior_snapshot();
    let (active_path, candidate_path, outcome) = stage(dir.path(), &prior, ok_inputs_additive());

    let mut blank = approval(&outcome.candidate.artifact_id, vec![]);
    blank.operator = "   ".to_string();
    let err = activate(&active_path, &candidate_path, &blank, refresh_now()).unwrap_err();
    assert!(
        matches!(err, ActivationError::ApprovalMissing { ref field } if field == "operator"),
        "got {err:?}"
    );
}

#[test]
fn absence_driven_destructive_candidate_refused_unless_acknowledged() {
    let dir = tempfile::tempdir().unwrap();
    let prior = prior_snapshot();
    let (active_path, candidate_path, outcome) = stage(dir.path(), &prior, absence_removal_inputs());

    // The diff carries HIGH-RISK entries (evidence removal / historical change / → Unknown).
    let required = required_acknowledgments(&outcome.diff);
    assert!(!required.is_empty(), "absence-driven removal must be high-risk");
    let active_before = std::fs::read(&active_path).unwrap();

    // Without acknowledging them → refused, active unchanged.
    let no_ack = approval(&outcome.candidate.artifact_id, vec![]);
    let err = activate(&active_path, &candidate_path, &no_ack, refresh_now()).unwrap_err();
    assert!(matches!(err, ActivationError::UnacknowledgedHighRisk { .. }), "got {err:?}");
    assert_eq!(active_before, std::fs::read(&active_path).unwrap());

    // Explicitly acknowledging every required key → activation proceeds.
    let acked = approval(&outcome.candidate.artifact_id, required);
    let record = activate(&active_path, &candidate_path, &acked, refresh_now()).unwrap();
    assert_eq!(record.predecessor_artifact_id, prior.artifact_id);
    assert!(!record.acknowledged_high_risk.is_empty());
    assert_eq!(std::fs::read(&active_path).unwrap(), std::fs::read(&candidate_path).unwrap());
}

#[test]
fn expired_authorization_on_active_snapshot_is_rejected_on_load() {
    // U3 reference: a snapshot whose authorization has expired at the as-of instant is a typed
    // Expired error on load — never a day fact. The active production snapshot is likewise
    // rejected once its recorded authorization lapses.
    let from = d(2012, 6, 1);
    let through = d(2012, 6, 5);
    let mut rows = Vec::new();
    let mut cursor = from;
    while cursor <= through {
        rows.push(DayRow {
            date: cursor,
            status: DayStatus::Unknown,
            decisive_evidence: vec![],
            conflicting_evidence: vec![],
            alerts: vec![],
        });
        cursor = cursor.succ_opt().unwrap();
    }
    let expiring = stamp(base_snapshot(from, through, vec![], vec![], rows, Some(t(2012, 12, 31))));

    // One tick before expiry: authorized. Strictly after: Expired.
    assert!(KrxCalendar::from_snapshot(expiring.clone(), t(2012, 12, 31)).is_ok());
    let err = KrxCalendar::from_snapshot(expiring, t(2013, 1, 1)).unwrap_err();
    assert!(matches!(err, CalendarLoadError::Expired), "got {err:?}");
}

#[test]
fn publication_boundary_active_snapshot_path_is_gitignored() {
    // The active production snapshot + candidate/diff artifacts must be gitignored so no
    // KRX-derived rows are ever committed (U15 publication boundary).
    let gitignore = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/.gitignore"))
        .expect("adapter .gitignore is present");
    assert!(gitignore.contains("/state"), "the owner-local snapshot tree must be gitignored");
    assert!(
        gitignore.contains("*.calendar.json"),
        "the calendar snapshot artifacts must be explicitly gitignored"
    );
    assert!(
        gitignore.contains("*.calendar.json.candidate.diff.json"),
        "the candidate diff artifact must be gitignored"
    );
}

// ---------------------------------------------------------------------------
// U2 rollback rehearsal — forward-activate of a PRIOR snapshot over the atomic install
// machinery, proving the prior artifact + adoption/activation identity is restored offline,
// with a coverage-for-as_of guard so a lapsed-coverage rollback is surfaced not silently
// installed. All synthetic, offline, fixed-clock (AE3, R4).

/// A rollback as-of whose KST civil date (2012-06-04) sits inside the prior snapshot's
/// materialized coverage (2012-06-01..2012-06-05), so the coverage guard passes.
fn rollback_as_of() -> DateTime<Utc> {
    t(2012, 6, 4)
}

/// Set up a rolled-forward state: write prior A, activate candidate B over it (active is now
/// B), and keep A in a separate `prior` file the operator retains for rollback. Returns the
/// active path, the prior (A) file path, the prior snapshot A, and B's refresh outcome.
fn stage_activated(
    dir: &std::path::Path,
) -> (
    std::path::PathBuf,
    std::path::PathBuf,
    Snapshot,
    nautilus_ls::calendar_refresh::RefreshOutcome,
) {
    let prior = prior_snapshot();
    let (active_path, candidate_path, outcome) = stage(dir, &prior, ok_inputs_additive());
    let approval = approval(&outcome.candidate.artifact_id, vec![]);
    activate(&active_path, &candidate_path, &approval, refresh_now()).unwrap();
    let prior_path = dir.join("cal.json.prior");
    std::fs::write(&prior_path, serde_json::to_vec_pretty(&prior).unwrap()).unwrap();
    (active_path, prior_path, prior, outcome)
}

#[test]
fn rollback_restores_prior_artifact_and_adoption_identity() {
    let dir = tempfile::tempdir().unwrap();
    let (active_path, prior_path, prior, outcome) = stage_activated(dir.path());

    // The activation chain is intact: candidate B records prior A as its predecessor.
    assert_eq!(
        outcome.candidate.predecessor_artifact_id.as_deref(),
        Some(prior.artifact_id.as_str()),
        "candidate B must record A as its predecessor for the chain to be consistent"
    );

    let approval = approval(&prior.artifact_id, vec![]);
    let record = rollback(&active_path, &prior_path, &approval, rollback_as_of()).unwrap();

    assert_eq!(record.restored_artifact_id, prior.artifact_id, "restored must be A");
    assert_eq!(
        record.superseded_artifact_id, outcome.candidate.artifact_id,
        "superseded must be the just-active B"
    );
    assert_eq!(record.operator, "maintainer-1");

    // The active file is now byte-identical to the prior snapshot, and reloads with A's id —
    // the prior artifact + adoption/activation identity is restored, no production artifact.
    assert_eq!(
        std::fs::read(&active_path).unwrap(),
        std::fs::read(&prior_path).unwrap(),
        "rollback must install the prior bytes verbatim"
    );
    let cal = KrxCalendar::load_from_path(&active_path, rollback_as_of()).unwrap();
    assert_eq!(cal.artifact_id(), prior.artifact_id);
}

#[test]
fn rollback_preserves_owner_only_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let (active_path, prior_path, prior, _outcome) = stage_activated(dir.path());
    let approval = approval(&prior.artifact_id, vec![]);
    rollback(&active_path, &prior_path, &approval, rollback_as_of()).unwrap();

    let mode = std::fs::metadata(&active_path).unwrap().permissions().mode();
    assert_eq!(
        mode & 0o777,
        0o600,
        "restored snapshot must be owner read/write only, got {:o}",
        mode & 0o777
    );
}

#[test]
fn rollback_of_an_unusable_prior_snapshot_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (active_path, _prior_path, prior, _outcome) = stage_activated(dir.path());
    let active_before = std::fs::read(&active_path).unwrap();

    // A prior snapshot whose authorization has already expired at as_of is unusable — the
    // real loader rejects it, so rollback fails closed (never a silent Unknown/install).
    let mut expired = prior.clone();
    expired.authorization.expires_at = Some(t(2012, 1, 1));
    let expired = stamp(expired);
    let expired_path = dir.path().join("cal.json.expired");
    std::fs::write(&expired_path, serde_json::to_vec_pretty(&expired).unwrap()).unwrap();

    let approval = approval(&expired.artifact_id, vec![]);
    let err = rollback(&active_path, &expired_path, &approval, rollback_as_of()).unwrap_err();
    assert!(
        matches!(err, RollbackError::PriorInvalid(CalendarLoadError::Expired)),
        "got {err:?}"
    );
    assert_eq!(
        active_before,
        std::fs::read(&active_path).unwrap(),
        "a refusal must leave the active file unchanged"
    );
}

#[test]
fn rollback_of_a_prior_snapshot_not_covering_as_of_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (active_path, prior_path, prior, _outcome) = stage_activated(dir.path());
    let active_before = std::fs::read(&active_path).unwrap();

    // A covers 2012-06-01..2012-06-05; roll back with an as_of whose KST date (2012-06-06) is
    // past that window — the prior loads and authorizes but no longer covers today, which the
    // per-date coverage query surfaces as an explicit refusal rather than a silent install
    // that would leave every Enforced consumer returning OutOfRange.
    let approval = approval(&prior.artifact_id, vec![]);
    let err = rollback(&active_path, &prior_path, &approval, t(2012, 6, 6)).unwrap_err();
    assert!(
        matches!(err, RollbackError::PriorDoesNotCoverAsOf { .. }),
        "got {err:?}"
    );
    assert_eq!(active_before, std::fs::read(&active_path).unwrap());
}

#[test]
fn rollback_with_blank_approval_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (active_path, prior_path, prior, _outcome) = stage_activated(dir.path());
    let active_before = std::fs::read(&active_path).unwrap();

    let mut blank = approval(&prior.artifact_id, vec![]);
    blank.operator = "   ".to_string();
    let err = rollback(&active_path, &prior_path, &blank, rollback_as_of()).unwrap_err();
    assert!(
        matches!(&err, RollbackError::ApprovalMissing { field } if field == "operator"),
        "got {err:?}"
    );
    assert_eq!(active_before, std::fs::read(&active_path).unwrap());
}

#[test]
fn rollback_of_an_unreviewed_prior_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (active_path, prior_path, _prior, _outcome) = stage_activated(dir.path());
    let active_before = std::fs::read(&active_path).unwrap();

    // The approval names a different artifact than the prior being restored — a rollback must
    // not rubber-stamp restoring a snapshot the operator did not review.
    let approval = approval("some-other-artifact-id", vec![]);
    let err = rollback(&active_path, &prior_path, &approval, rollback_as_of()).unwrap_err();
    assert!(matches!(err, RollbackError::Unreviewed { .. }), "got {err:?}");
    assert_eq!(active_before, std::fs::read(&active_path).unwrap());
}

// ---------------------------------------------------------------------------------------
// U6 (KTD5): first-install (genesis) activation — full ceremony minus stale-base/active-load,
// refuse-if-exists guard, genesis:no-predecessor ack, description-authorization match, and an
// EXCLUSIVE-CREATE install. Newly-live branch → the full refusal matrix, not diff-scoped.
// ---------------------------------------------------------------------------------------

use std::path::{Path, PathBuf};

fn genesis_inputs(
    window: DateRange,
    krx_through: NaiveDate,
    witnessed: &[NaiveDate],
    holidays: &[NaiveDate],
) -> RefreshInputs {
    let mut evidence = Vec::new();
    let mut cur = window.from;
    while cur <= window.through {
        if matches!(cur.weekday(), Weekday::Sat | Weekday::Sun) {
            evidence.push(ev(&format!("rule-{cur}"), "krx-rule", cur, EvidenceKind::DeterministicRule, false));
        }
        cur = cur.succ_opt().unwrap();
    }
    for &w in witnessed {
        evidence.push(ev(&format!("witness-{w}"), "krx-daily", w, EvidenceKind::PositiveWitness, false));
    }
    for &h in holidays {
        evidence.push(ev(&format!("kasi-{h}"), "kasi", h, EvidenceKind::HolidayFact, false));
        evidence.push(ev(&format!("rule-{h}"), "krx-rule", h, EvidenceKind::DeterministicRule, false));
    }
    RefreshInputs {
        sources: vec![
            src("krx-daily", SourceKind::KrxDailyMarket),
            src("kasi", SourceKind::KasiHoliday),
            src("krx-rule", SourceKind::KrxRule),
        ],
        evidence,
        outcomes: vec![
            SourceOutcome::ok_covering("krx-daily", SourceKind::KrxDailyMarket, vec![DateRange::new(window.from, krx_through)]),
            SourceOutcome::ok_covering("kasi", SourceKind::KasiHoliday, vec![window]),
            SourceOutcome::ok_covering("krx-rule", SourceKind::KrxRule, vec![window]),
        ],
    }
}

fn genesis_candidate() -> nautilus_ls_calendar::schema::Snapshot {
    let window = DateRange::new(d(2026, 6, 13), d(2026, 6, 19));
    let consumer = DateRange::new(d(2026, 6, 15), d(2026, 6, 19));
    let inputs = genesis_inputs(
        window,
        d(2026, 6, 19),
        &[d(2026, 6, 15), d(2026, 6, 17), d(2026, 6, 18), d(2026, 6, 19)],
        &[d(2026, 6, 16)],
    );
    let params = GenesisParams {
        scope: CalendarScope {
            calendar_name: "KRX domestic equity regular session".to_string(),
            venue: "XKRX".to_string(),
            instrument_class: "domestic-equity".to_string(),
            timezone: "Asia/Seoul".to_string(),
            synthetic: false,
        },
        authorization: Authorization {
            authorized: true,
            authority: "KRX Open API Agreement".to_string(),
            granted_at: t(2026, 1, 1),
            expires_at: Some(t(2027, 1, 1)),
            terminated_at: None,
        },
        source_availability: vec![SourceAvailabilityBound {
            source_id: "krx-daily".to_string(),
            available_from: Some(d(2010, 1, 4)),
            available_through: None,
        }],
        window,
        krx_through: d(2026, 6, 19),
        consumer_window: consumer,
    };
    build_genesis(&params, &inputs, t(2026, 6, 20)).expect("genesis builds")
}

fn consumer_window() -> DateRange {
    DateRange::new(d(2026, 6, 15), d(2026, 6, 19))
}

fn as_of() -> DateTime<Utc> {
    t(2026, 6, 20)
}

/// Lay a genesis candidate + its description artifact on disk. Returns (install dest, candidate
/// path, candidate). The install dest does NOT exist yet.
fn genesis_on_disk(dir: &Path) -> (PathBuf, PathBuf, nautilus_ls_calendar::schema::Snapshot) {
    let candidate = genesis_candidate();
    let candidate_path = dir.join("cal.json.candidate");
    let description = describe_genesis(&candidate, consumer_window());
    write_genesis(&candidate_path, &candidate, &description).unwrap();
    (dir.join("cal.json"), candidate_path, candidate)
}

fn genesis_approval(candidate: &nautilus_ls_calendar::schema::Snapshot) -> ActivationApproval {
    ActivationApproval {
        operator: "operator@lab".to_string(),
        reason: "genesis chain root".to_string(),
        approved_at: t(2026, 6, 20),
        reviewed_artifact_id: candidate.artifact_id.clone(),
        acknowledged: vec![GENESIS_ACK_KEY.to_string()],
    }
}

#[test]
fn first_install_succeeds_and_installs_owner_only_chain_root() {
    let dir = tempfile::tempdir().unwrap();
    let (active, candidate_path, candidate) = genesis_on_disk(dir.path());
    let approval = genesis_approval(&candidate);

    let record: GenesisActivationRecord =
        first_install(&active, &candidate_path, &approval, as_of()).expect("genesis installs");
    assert_eq!(record.candidate_artifact_id, candidate.artifact_id);
    assert!(record.acknowledged.contains(&GENESIS_ACK_KEY.to_string()));

    // Installed 0o600, byte-equal to the candidate.
    let mode = std::fs::metadata(&active).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    assert_eq!(std::fs::read(&active).unwrap(), std::fs::read(&candidate_path).unwrap());
}

#[test]
fn first_install_refuses_when_the_install_path_already_exists() {
    // AE8: a live chain root is never superseded through first-install.
    let dir = tempfile::tempdir().unwrap();
    let (active, candidate_path, candidate) = genesis_on_disk(dir.path());
    std::fs::write(&active, b"pre-existing-chain-root").unwrap();
    let approval = genesis_approval(&candidate);
    let err = first_install(&active, &candidate_path, &approval, as_of()).unwrap_err();
    assert!(matches!(err, FirstInstallError::AlreadyActive), "{err}");
    // The existing file is byte-identical.
    assert_eq!(std::fs::read(&active).unwrap(), b"pre-existing-chain-root");
}

#[test]
fn first_install_is_idempotent_second_run_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let (active, candidate_path, candidate) = genesis_on_disk(dir.path());
    let approval = genesis_approval(&candidate);
    first_install(&active, &candidate_path, &approval, as_of()).expect("first install");
    let err = first_install(&active, &candidate_path, &approval, as_of()).unwrap_err();
    assert!(matches!(err, FirstInstallError::AlreadyActive), "second run refuses (now active exists)");
}

#[test]
fn first_install_refuses_blank_operator_or_reason() {
    let dir = tempfile::tempdir().unwrap();
    let (active, candidate_path, candidate) = genesis_on_disk(dir.path());

    let mut blank_op = genesis_approval(&candidate);
    blank_op.operator = "  ".to_string();
    assert!(matches!(
        first_install(&active, &candidate_path, &blank_op, as_of()).unwrap_err(),
        FirstInstallError::ApprovalMissing { .. }
    ));
    assert!(!active.exists(), "no install on a blank-approval refusal");

    let mut blank_reason = genesis_approval(&candidate);
    blank_reason.reason = String::new();
    assert!(matches!(
        first_install(&active, &candidate_path, &blank_reason, as_of()).unwrap_err(),
        FirstInstallError::ApprovalMissing { .. }
    ));
}

#[test]
fn first_install_refuses_a_candidate_that_declares_a_predecessor() {
    let dir = tempfile::tempdir().unwrap();
    let (active, candidate_path, mut candidate) = genesis_on_disk(dir.path());
    // Give the candidate a predecessor and re-stamp so it still loads — it belongs to the normal path.
    candidate.predecessor_artifact_id = Some("some-prior".to_string());
    let candidate = stamp(candidate);
    std::fs::write(&candidate_path, serde_json::to_vec_pretty(&candidate).unwrap()).unwrap();
    let approval = genesis_approval(&candidate);
    let err = first_install(&active, &candidate_path, &approval, as_of()).unwrap_err();
    assert!(matches!(err, FirstInstallError::HasPredecessor { .. }), "{err}");
    assert!(!active.exists());
}

#[test]
fn first_install_refuses_an_invalid_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let (active, candidate_path, candidate) = genesis_on_disk(dir.path());
    // Tamper the artifact_id so loader revalidation fails (HashMismatch).
    let mut tampered = candidate.clone();
    tampered.artifact_id = "not-the-real-hash".to_string();
    std::fs::write(&candidate_path, serde_json::to_vec_pretty(&tampered).unwrap()).unwrap();
    let approval = genesis_approval(&candidate);
    let err = first_install(&active, &candidate_path, &approval, as_of()).unwrap_err();
    assert!(matches!(err, FirstInstallError::Invalid(_)), "{err}");
    assert!(!active.exists());
}

#[test]
fn first_install_refuses_an_approval_that_reviews_the_wrong_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let (active, candidate_path, candidate) = genesis_on_disk(dir.path());
    let mut approval = genesis_approval(&candidate);
    approval.reviewed_artifact_id = "some-other-candidate".to_string();
    let err = first_install(&active, &candidate_path, &approval, as_of()).unwrap_err();
    assert!(matches!(err, FirstInstallError::Unreviewed { .. }), "{err}");
    assert!(!active.exists());
}

#[test]
fn first_install_refuses_a_missing_description_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let (active, candidate_path, candidate) = genesis_on_disk(dir.path());
    std::fs::remove_file(genesis_description_path_for(&candidate_path)).unwrap();
    let approval = genesis_approval(&candidate);
    let err = first_install(&active, &candidate_path, &approval, as_of()).unwrap_err();
    assert!(matches!(err, FirstInstallError::Unreviewed { .. }), "{err}");
}

#[test]
fn first_install_refuses_a_description_that_names_a_different_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let (active, candidate_path, candidate) = genesis_on_disk(dir.path());
    // Rewrite the description to name a different candidate.
    let desc_path = genesis_description_path_for(&candidate_path);
    let mut desc: GenesisDescription =
        serde_json::from_slice(&std::fs::read(&desc_path).unwrap()).unwrap();
    desc.candidate_artifact_id = "different-candidate".to_string();
    std::fs::write(&desc_path, serde_json::to_vec_pretty(&desc).unwrap()).unwrap();
    let approval = genesis_approval(&candidate);
    let err = first_install(&active, &candidate_path, &approval, as_of()).unwrap_err();
    assert!(matches!(err, FirstInstallError::Unreviewed { .. }), "{err}");
}

#[test]
fn first_install_refuses_an_authorization_mismatch_between_description_and_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let (active, candidate_path, candidate) = genesis_on_disk(dir.path());
    let desc_path = genesis_description_path_for(&candidate_path);
    let mut desc: GenesisDescription =
        serde_json::from_slice(&std::fs::read(&desc_path).unwrap()).unwrap();
    desc.authority = "A Different Agreement".to_string(); // still names the candidate
    std::fs::write(&desc_path, serde_json::to_vec_pretty(&desc).unwrap()).unwrap();
    let approval = genesis_approval(&candidate);
    let err = first_install(&active, &candidate_path, &approval, as_of()).unwrap_err();
    assert!(matches!(err, FirstInstallError::AuthorizationMismatch { .. }), "{err}");
    assert!(!active.exists());
}

#[test]
fn first_install_refuses_a_missing_genesis_acknowledgment() {
    let dir = tempfile::tempdir().unwrap();
    let (active, candidate_path, candidate) = genesis_on_disk(dir.path());
    let mut approval = genesis_approval(&candidate);
    approval.acknowledged.clear(); // drop genesis:no-predecessor
    let err = first_install(&active, &candidate_path, &approval, as_of()).unwrap_err();
    match err {
        FirstInstallError::Unacknowledged { entries } => {
            assert!(entries.contains(&GENESIS_ACK_KEY.to_string()));
        }
        other => panic!("expected Unacknowledged, got {other}"),
    }
    assert!(!active.exists());
}
