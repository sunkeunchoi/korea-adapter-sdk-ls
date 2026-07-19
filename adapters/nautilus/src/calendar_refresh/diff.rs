//! Deterministic categorized diff of a candidate vs. its exact active predecessor (U14).
//!
//! [`diff_against_predecessor`] compares a candidate [`Snapshot`] against the predecessor it
//! declares (`predecessor_artifact_id`) and produces a [`CategorizedDiff`]: a
//! deterministically-ordered list of typed [`DiffEntry`]s. Six classes are HIGH-RISK
//! (historical status change, transition to Unknown, evidence removal, first-party conflict,
//! coverage contraction, near-term closure change); the rest (new forward coverage, an
//! Unknown getting established, new additive evidence) are additive and low-risk.
//!
//! Determinism: identical `prior`/`candidate`/`horizon`/`partial` inputs always produce an
//! equal [`CategorizedDiff`] — entries are sorted by `(date, category, detail)` and carry no
//! timestamps.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use serde::Serialize;

use nautilus_ls_calendar::schema::{AlertKind, DayStatus, Snapshot};

/// A typed category of change in a categorized diff. The first six are HIGH-RISK; the last
/// three are additive/low-risk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffCategory {
    /// An already-materialized past date's established status changed (known → different
    /// known, or known → Unknown). HIGH-RISK.
    HistoricalStatusChange,
    /// A closure status changed (to/from Closed) on a date inside the operating horizon.
    /// HIGH-RISK.
    NearTermClosureChange,
    /// A date's status became Unknown. HIGH-RISK.
    TransitionToUnknown,
    /// An evidence record present in the predecessor is absent from the candidate. HIGH-RISK.
    EvidenceRemoval,
    /// A new unresolved first-party conflict (witness-vs-notice, or two notices) appeared.
    /// HIGH-RISK.
    FirstPartyConflict,
    /// A coverage claim (materialized / retrospective / scheduled window, or a dropped row)
    /// contracted. HIGH-RISK.
    CoverageContraction,
    /// New forward materialization (a date the predecessor did not cover). Additive.
    NewCoverage,
    /// A previously-Unknown date became a proven status. Additive.
    StatusEstablished,
    /// New additive evidence the predecessor did not carry. Additive.
    NewEvidence,
}

impl DiffCategory {
    /// `true` iff this category is one of the six flagged HIGH-RISK classes.
    pub fn is_high_risk(&self) -> bool {
        matches!(
            self,
            DiffCategory::HistoricalStatusChange
                | DiffCategory::NearTermClosureChange
                | DiffCategory::TransitionToUnknown
                | DiffCategory::EvidenceRemoval
                | DiffCategory::FirstPartyConflict
                | DiffCategory::CoverageContraction
        )
    }
}

/// One typed change in a categorized diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiffEntry {
    /// The category of change.
    pub category: DiffCategory,
    /// The civil date the change bears on, if date-specific.
    pub date: Option<NaiveDate>,
    /// A stable, credential-free human description.
    pub detail: String,
    /// Whether this entry is HIGH-RISK (derived from `category`).
    pub high_risk: bool,
}

/// A deterministic categorized diff of a candidate vs. its exact active predecessor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CategorizedDiff {
    /// The predecessor `artifact_id` the candidate declares (the exact active predecessor).
    pub predecessor_artifact_id: Option<String>,
    /// The candidate `artifact_id`.
    pub candidate_artifact_id: String,
    /// The typed changes, deterministically ordered.
    pub entries: Vec<DiffEntry>,
    /// `true` iff the candidate is PARTIAL (a source failed during the refresh) — it still
    /// requires review even if no effective fact changed.
    pub partial: bool,
}

impl CategorizedDiff {
    /// The HIGH-RISK entries, in diff order.
    pub fn high_risk_entries(&self) -> impl Iterator<Item = &DiffEntry> {
        self.entries.iter().filter(|e| e.high_risk)
    }

    /// The distinct categories present, in category order (deduped).
    pub fn categories(&self) -> Vec<DiffCategory> {
        let set: BTreeSet<DiffCategory> = self.entries.iter().map(|e| e.category).collect();
        set.into_iter().collect()
    }

    /// Whether this candidate requires maintainer review: any change, or any HIGH-RISK flag,
    /// or a partial (source-failure) provenance.
    pub fn requires_review(&self) -> bool {
        self.partial || !self.entries.is_empty()
    }
}

/// Diff `candidate` against `prior` within `operating_horizon`, tagging `partial` provenance.
///
/// `operating_horizon` is the inclusive `(from, through)` window inside which a closure
/// change is operationally near-term. `partial` marks a source-failure candidate.
pub fn diff_against_predecessor(
    prior: &Snapshot,
    candidate: &Snapshot,
    operating_horizon: (NaiveDate, NaiveDate),
    partial: bool,
) -> CategorizedDiff {
    let mut entries: Vec<DiffEntry> = Vec::new();
    let (horizon_from, horizon_through) = operating_horizon;
    let in_horizon = |date: NaiveDate| date >= horizon_from && date <= horizon_through;

    let prior_rows: BTreeMap<NaiveDate, DayStatus> =
        prior.rows.iter().map(|r| (r.date, r.status)).collect();
    let cand_rows: BTreeMap<NaiveDate, DayStatus> =
        candidate.rows.iter().map(|r| (r.date, r.status)).collect();

    // Per-date status comparison.
    let all_dates: BTreeSet<NaiveDate> =
        prior_rows.keys().chain(cand_rows.keys()).copied().collect();
    for date in all_dates {
        match (prior_rows.get(&date), cand_rows.get(&date)) {
            (Some(&before), Some(&after)) if before != after => {
                let involves_closed = before == DayStatus::Closed || after == DayStatus::Closed;
                if before == DayStatus::Unknown && after != DayStatus::Unknown {
                    // An Unknown getting established is additive/expected during backfill.
                    entries.push(entry(
                        DiffCategory::StatusEstablished,
                        Some(date),
                        format!("{before:?} -> {after:?}"),
                    ));
                } else {
                    entries.push(entry(
                        DiffCategory::HistoricalStatusChange,
                        Some(date),
                        format!("{before:?} -> {after:?}"),
                    ));
                    if after == DayStatus::Unknown {
                        entries.push(entry(
                            DiffCategory::TransitionToUnknown,
                            Some(date),
                            format!("{before:?} -> Unknown"),
                        ));
                    }
                }
                if involves_closed && in_horizon(date) {
                    entries.push(entry(
                        DiffCategory::NearTermClosureChange,
                        Some(date),
                        format!("near-term closure change {before:?} -> {after:?}"),
                    ));
                }
            }
            (Some(_), None) => {
                // A previously-materialized date dropped from the candidate.
                entries.push(entry(
                    DiffCategory::CoverageContraction,
                    Some(date),
                    "materialized date removed from candidate".to_string(),
                ));
            }
            (None, Some(&after)) => {
                entries.push(entry(
                    DiffCategory::NewCoverage,
                    Some(date),
                    format!("newly materialized as {after:?}"),
                ));
            }
            _ => {}
        }
    }

    // Coverage-claim contraction (the window edges moving the wrong way).
    coverage_contraction(prior, candidate, &mut entries);

    // Evidence removal: any predecessor evidence id absent from the candidate.
    let cand_ev_ids: BTreeSet<&str> = candidate.evidence.iter().map(|e| e.id.as_str()).collect();
    for e in &prior.evidence {
        if !cand_ev_ids.contains(e.id.as_str()) {
            entries.push(entry(
                DiffCategory::EvidenceRemoval,
                Some(e.date),
                format!("evidence {} removed", e.id),
            ));
        }
    }

    // New additive evidence.
    let prior_ev_ids: BTreeSet<&str> = prior.evidence.iter().map(|e| e.id.as_str()).collect();
    for e in &candidate.evidence {
        if !prior_ev_ids.contains(e.id.as_str()) {
            entries.push(entry(
                DiffCategory::NewEvidence,
                Some(e.date),
                format!("evidence {} added", e.id),
            ));
        }
    }

    // New first-party conflicts (witness-vs-notice or two notices) not present before.
    let prior_conflicts: BTreeSet<(NaiveDate, String)> = prior
        .alerts
        .iter()
        .filter(|a| is_first_party_conflict(a.kind))
        .map(|a| (a.date, format!("{:?}", a.kind)))
        .collect();
    for a in &candidate.alerts {
        if is_first_party_conflict(a.kind)
            && !prior_conflicts.contains(&(a.date, format!("{:?}", a.kind)))
        {
            entries.push(entry(
                DiffCategory::FirstPartyConflict,
                Some(a.date),
                format!("new first-party conflict ({:?})", a.kind),
            ));
        }
    }

    // Deterministic ordering: (date, category, detail). None-dated entries sort first.
    entries.sort_by(|x, y| {
        x.date
            .cmp(&y.date)
            .then(x.category.cmp(&y.category))
            .then_with(|| x.detail.cmp(&y.detail))
    });
    entries.dedup();

    CategorizedDiff {
        predecessor_artifact_id: candidate.predecessor_artifact_id.clone(),
        candidate_artifact_id: candidate.artifact_id.clone(),
        entries,
        partial,
    }
}

fn is_first_party_conflict(kind: AlertKind) -> bool {
    matches!(
        kind,
        AlertKind::FirstPartyConflict | AlertKind::WitnessVsClosureNotice
    )
}

fn coverage_contraction(prior: &Snapshot, candidate: &Snapshot, entries: &mut Vec<DiffEntry>) {
    let p = &prior.coverage;
    let c = &candidate.coverage;
    if c.materialized_through < p.materialized_through {
        entries.push(entry(
            DiffCategory::CoverageContraction,
            None,
            format!(
                "materialized_through contracted {} -> {}",
                p.materialized_through, c.materialized_through
            ),
        ));
    }
    if c.materialized_from > p.materialized_from {
        entries.push(entry(
            DiffCategory::CoverageContraction,
            None,
            format!(
                "materialized_from contracted {} -> {}",
                p.materialized_from, c.materialized_from
            ),
        ));
    }
    if c.retrospectively_checked_through < p.retrospectively_checked_through {
        entries.push(entry(
            DiffCategory::CoverageContraction,
            None,
            format!(
                "retrospectively_checked_through contracted {} -> {}",
                p.retrospectively_checked_through, c.retrospectively_checked_through
            ),
        ));
    }
    if c.scheduled_closure_evaluated_through < p.scheduled_closure_evaluated_through {
        entries.push(entry(
            DiffCategory::CoverageContraction,
            None,
            format!(
                "scheduled_closure_evaluated_through contracted {} -> {}",
                p.scheduled_closure_evaluated_through, c.scheduled_closure_evaluated_through
            ),
        ));
    }
}

fn entry(category: DiffCategory, date: Option<NaiveDate>, detail: String) -> DiffEntry {
    DiffEntry {
        category,
        date,
        detail,
        high_risk: category.is_high_risk(),
    }
}
