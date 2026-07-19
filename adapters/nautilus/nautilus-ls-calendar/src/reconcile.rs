//! Evidence reconciliation authority matrix (U5, KTD6).
//!
//! [`reconcile`] is a PURE, side-effect-free function: it takes one civil date and the
//! evidence records bearing on it, and returns the decisive tri-state [`DayStatus`] plus
//! the decisive / conflicting evidence ids and the retained-disagreement alerts. It does
//! NO transport and NO I/O — the refresh tooling (U14) calls it to build snapshot rows,
//! and the caller stamps alert ids.
//!
//! # The settled authority matrix (KTD6)
//!
//! Precedence, evaluated on the *effective* evidence for the date (see below):
//!
//! 1. A KRX positive witness on an otherwise-inferred closure (a [`DeterministicRule`] or
//!    a [`HolidayFact`]) → [`TradingSession`] + [`WitnessOverridesInference`]. Observed
//!    operation wins; the inferred-closure claim is retained as *conflicting*.
//! 2. A KRX positive witness vs. a direct first-party cited [`ClosureNotice`] →
//!    [`Unknown`] + [`WitnessVsClosureNotice`]. An unresolved first-party conflict; both
//!    claims are retained as conflicting.
//! 3. A later empty/malformed KRX response after an accepted witness → [`TradingSession`]
//!    preserved + [`AbsenceIgnored`]. Absence NEVER retracts. A recorded empty/malformed
//!    response is modeled as a non-qualifying (`valid == false`, un-superseded) witness
//!    record — it can never flip an accepted witness.
//! 4. A KASI [`HolidayFact`] + an applicable published [`DeterministicRule`] → [`Closed`]
//!    (the rule connects the holiday to the scoped session). A holiday fact with NO
//!    connecting rule is NOT [`Closed`] → [`Unknown`].
//! 5. Weekend / Labor Day / year-end per a published [`DeterministicRule`] → [`Closed`]
//!    (rule authority).
//! 6. An exceptional closure with a cited first-party [`ClosureNotice`] → [`Closed`]. An
//!    UN-cited closure notice is rejected — it cannot create a bare status.
//! 7. Two conflicting first-party claims (≥2 distinct effective cited [`ClosureNotice`]s,
//!    neither superseding the other) → [`Unknown`] + [`FirstPartyConflict`].
//! 8. An explicit [`Correction`] supersedes ONLY the identified evidence (never a generic
//!    newest-wins) — a sibling claim is untouched. Emits [`Superseded`].
//! 9. A human [`Adjudication`] changes only validity/supersession — it CANNOT write a
//!    status directly. Its presence emits [`Adjudicated`].
//! 10. No covering evidence (empty, or all-invalid/superseded) → [`Unknown`] (a successful
//!     factual result).
//!
//! # How corrections & adjudications are modeled
//!
//! Corrections and adjudications never bear a status themselves. Their effect on a target
//! is carried ON THE TARGET record: `valid == false` (invalidated) and/or
//! `superseded_by == Some(<correction/adjudication id>)` (superseded). Reconciliation
//! *applies* them by honoring those fields BEFORE deciding — a superseded or invalidated
//! claim is not *effective* and so is never decisive. Supersession is honored only when
//! the identified superseding record is present in the date's set, so a correction
//! supersedes only the claim it identifies (KTD6, no newest-wins).
//!
//! [`TradingSession`]: DayStatus::TradingSession
//! [`Closed`]: DayStatus::Closed
//! [`Unknown`]: DayStatus::Unknown
//! [`DeterministicRule`]: crate::schema::EvidenceKind::DeterministicRule
//! [`HolidayFact`]: crate::schema::EvidenceKind::HolidayFact
//! [`ClosureNotice`]: crate::schema::EvidenceKind::ClosureNotice
//! [`Correction`]: crate::schema::EvidenceKind::Correction
//! [`Adjudication`]: crate::schema::EvidenceKind::Adjudication
//! [`WitnessOverridesInference`]: crate::schema::AlertKind::WitnessOverridesInference
//! [`WitnessVsClosureNotice`]: crate::schema::AlertKind::WitnessVsClosureNotice
//! [`AbsenceIgnored`]: crate::schema::AlertKind::AbsenceIgnored
//! [`FirstPartyConflict`]: crate::schema::AlertKind::FirstPartyConflict
//! [`Superseded`]: crate::schema::AlertKind::Superseded
//! [`Adjudicated`]: crate::schema::AlertKind::Adjudicated

use chrono::NaiveDate;

use crate::schema::{AlertKind, DayStatus, EvidenceKind, EvidenceRecord};

/// A reconciliation alert with no id — the caller (U14) stamps a stable id when it writes
/// the alert into a snapshot. Carries the same [`AlertKind`] + `message` + `date` the
/// snapshot [`Alert`](crate::schema::Alert) records, so stamping is a pure id assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileAlert {
    /// The civil date this alert bears on.
    pub date: NaiveDate,
    /// What kind of disagreement/override this alert records.
    pub kind: AlertKind,
    /// Human-readable message.
    pub message: String,
}

/// The reconciled outcome for one civil date: the decisive tri-state status, the ids of
/// the decisive / conflicting evidence (retained disagreement), and the alerts. The
/// caller maps this onto a [`DayRow`](crate::schema::DayRow) + [`Alert`](crate::schema::Alert)s,
/// stamping alert ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciledDay {
    /// The decisive tri-state status.
    pub status: DayStatus,
    /// Ids of the evidence records decisive for `status` (empty for `Unknown`).
    pub decisive_evidence: Vec<String>,
    /// Ids of evidence records that conflicted but did not decide (disagreement retained).
    pub conflicting_evidence: Vec<String>,
    /// The alerts for this date, without ids (the caller stamps them).
    pub alerts: Vec<ReconcileAlert>,
}

/// Reconcile the `evidence` bearing on `date` into a decisive [`ReconciledDay`] per the
/// settled authority matrix (module docs, KTD6). Pure and side-effect-free.
///
/// Only records whose `date` matches are considered (the caller passes a date's set, but
/// filtering keeps the function total). Corrections/adjudications are *applied* by honoring
/// each target's `valid` / `superseded_by` fields before deciding.
pub fn reconcile(date: NaiveDate, evidence: &[EvidenceRecord]) -> ReconciledDay {
    // The set of records bearing on this date.
    let set: Vec<&EvidenceRecord> = evidence.iter().filter(|r| r.date == date).collect();

    // A record is *superseded* iff it identifies a superseding record that is present in
    // the set — a correction supersedes only the claim it names (KTD6, no newest-wins).
    let is_superseded = |r: &EvidenceRecord| -> bool {
        match &r.superseded_by {
            Some(sid) => set.iter().any(|x| &x.id == sid),
            None => false,
        }
    };
    // A record is *effective* (decisive-eligible) iff it is valid and not superseded.
    let effective = |r: &EvidenceRecord| -> bool { r.valid && !is_superseded(r) };

    // Effective claims partitioned by kind, in input order (deterministic output).
    let mut witnesses: Vec<String> = Vec::new();
    let mut holiday_facts: Vec<String> = Vec::new();
    let mut rules: Vec<String> = Vec::new();
    let mut cited_closures: Vec<String> = Vec::new();
    for r in &set {
        if !effective(r) {
            continue;
        }
        match r.kind {
            EvidenceKind::PositiveWitness => witnesses.push(r.id.clone()),
            EvidenceKind::HolidayFact => holiday_facts.push(r.id.clone()),
            EvidenceKind::DeterministicRule => rules.push(r.id.clone()),
            // Row 6: an UN-cited closure notice cannot create a bare status — rejected.
            EvidenceKind::ClosureNotice => {
                if r.citation.is_some() {
                    cited_closures.push(r.id.clone());
                }
            }
            // Corrections/adjudications never bear a status; they only supersede/invalidate.
            EvidenceKind::Correction | EvidenceKind::Adjudication => {}
        }
    }

    // Provenance annotations, independent of the decisive branch:
    // a record superseded by a present *Correction* (row 8),
    let superseded_by_correction = set.iter().any(|r| match &r.superseded_by {
        Some(sid) => set
            .iter()
            .any(|x| &x.id == sid && x.kind == EvidenceKind::Correction),
        None => false,
    });
    // the presence of any human *Adjudication* record (row 9),
    let has_adjudication = set.iter().any(|r| r.kind == EvidenceKind::Adjudication);
    // a recorded empty/malformed KRX response: a non-qualifying (`valid == false`,
    // un-superseded) witness record — the "absence" of row 3.
    let absence_marker = set.iter().any(|r| {
        r.kind == EvidenceKind::PositiveWitness && !r.valid && r.superseded_by.is_none()
    });

    let mut alerts: Vec<ReconcileAlert> = Vec::new();
    let mut decisive: Vec<String> = Vec::new();
    let mut conflicting: Vec<String> = Vec::new();
    let mut push_alert = |kind: AlertKind, message: &str| {
        alerts.push(ReconcileAlert {
            date,
            kind,
            message: message.to_string(),
        });
    };

    let status = if !witnesses.is_empty() {
        if !cited_closures.is_empty() {
            // Row 2 — witness vs. direct first-party closure notice: unresolved conflict.
            conflicting.extend(witnesses.iter().cloned());
            conflicting.extend(cited_closures.iter().cloned());
            push_alert(
                AlertKind::WitnessVsClosureNotice,
                "KRX positive witness conflicts with a direct first-party closure notice",
            );
            DayStatus::Unknown
        } else {
            // Rows 1 & 3 — observed operation wins; absence never retracts.
            decisive.extend(witnesses.iter().cloned());
            // Row 1 — an inferred closure (rule or holiday) is overridden, retained as
            // conflicting.
            let inferred: Vec<String> = holiday_facts
                .iter()
                .chain(rules.iter())
                .cloned()
                .collect();
            if !inferred.is_empty() {
                conflicting.extend(inferred.iter().cloned());
                push_alert(
                    AlertKind::WitnessOverridesInference,
                    "KRX positive witness overrides an inferred closure",
                );
            }
            // Row 3 — a recorded later empty/malformed response was ignored.
            if absence_marker {
                push_alert(
                    AlertKind::AbsenceIgnored,
                    "a later empty/malformed KRX response was ignored; the accepted witness stands",
                );
            }
            DayStatus::TradingSession
        }
    } else if cited_closures.len() >= 2 {
        // Row 7 — two conflicting first-party claims, neither superseding the other.
        conflicting.extend(cited_closures.iter().cloned());
        push_alert(
            AlertKind::FirstPartyConflict,
            "two conflicting first-party closure notices for the date",
        );
        DayStatus::Unknown
    } else if cited_closures.len() == 1 {
        // Row 6 — exceptional closure with a cited first-party notice.
        decisive.extend(cited_closures.iter().cloned());
        DayStatus::Closed
    } else if !rules.is_empty() {
        // Rows 4 & 5 — a published KRX rule (with a connecting holiday fact if present)
        // establishes a scheduled closure.
        decisive.extend(holiday_facts.iter().cloned());
        decisive.extend(rules.iter().cloned());
        DayStatus::Closed
    } else {
        // Row 4 (negative) — a holiday fact with no connecting rule is NOT Closed; and
        // Row 10 — no covering evidence. Both are a successful `Unknown`.
        DayStatus::Unknown
    };

    // Retained-disagreement annotations, valid in any branch.
    if superseded_by_correction {
        push_alert(
            AlertKind::Superseded,
            "evidence was superseded by an explicit correction",
        );
    }
    if has_adjudication {
        push_alert(
            AlertKind::Adjudicated,
            "a human adjudication changed evidence validity/supersession",
        );
    }

    ReconciledDay {
        status,
        decisive_evidence: decisive,
        conflicting_evidence: conflicting,
        alerts,
    }
}
