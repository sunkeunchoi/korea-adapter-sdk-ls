//! Explicit, atomic candidate activation (U15, KTD9).
//!
//! [`activate`] is the ONLY way a reviewed candidate ([`write_candidate`](super::write_candidate))
//! becomes the active production snapshot. It is deliberately paranoid — every refusal leaves
//! the active file byte-identical (installation is the very last step, an atomic temp+rename):
//!
//! 1. **Explicit approval** — a non-blank [`ActivationApproval`] (operator + reason +
//!    timestamp). A blank operator/reason is [`ActivationError::ApprovalMissing`].
//! 2. **Predecessor identity** — the current active snapshot's recomputed `artifact_id` must
//!    equal `candidate.predecessor_artifact_id` (a stale-base candidate built off a different
//!    predecessor is [`ActivationError::StaleBase`]).
//! 3. **Revalidation** — the candidate must load clean through the real
//!    [`KrxCalendar::from_snapshot`] at `as_of` ([`ActivationError::Invalid`] otherwise).
//! 4. **Reviewed** — the approval must name the EXACT candidate `artifact_id`
//!    (`reviewed_artifact_id`), and a reviewable diff artifact (the U14
//!    `<active>.candidate.diff.json`) must exist and describe this candidate — approving one
//!    candidate can never rubber-stamp another ([`ActivationError::Unreviewed`]).
//! 5. **High-risk acknowledgement** — every HIGH-RISK diff entry (evidence removal, coverage
//!    contraction, transition-to-Unknown, first-party conflict, historical/near-term closure
//!    change) AND a partial (source-failure/absence-driven) provenance must be EXPLICITLY
//!    acknowledged in the approval, or activation is [`ActivationError::UnacknowledgedHighRisk`].
//! 6. **Record + atomic install** — approval is recorded into an [`ActivationRecord`], the
//!    candidate bytes are written to a sibling tempfile created `0o600` (owner-readable ONLY)
//!    and `rename`d over the active path — no partial state, no world-readable window.
//!
//! The active production snapshot lives only under the gitignored, owner-readable `/state`
//! tree (see `adapters/nautilus/.gitignore`), so no KRX-derived rows are ever committed; and
//! it is rejected on load once its recorded authorization expires/terminates (U3
//! [`KrxCalendar::load_from_path`] returns [`CalendarLoadError::Expired`]).

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use nautilus_ls_calendar::schema::Snapshot;
use nautilus_ls_calendar::{compute_artifact_id, CalendarLoadError, KrxCalendar, QueryError};

use super::diff::{CategorizedDiff, DiffEntry};
use super::genesis::GenesisDescription;
use super::{diff_path_for, genesis_description_path_for};

/// Explicit maintainer approval for one activation. Modeled as data (serde-round-trippable)
/// so the CLI can load a reviewed, signed-off approval file. `reviewed_artifact_id` binds the
/// approval to ONE candidate; `acknowledged` lists the high-risk / partial keys the maintainer
/// explicitly accepts (see [`acknowledgment_key`] / [`required_acknowledgments`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivationApproval {
    /// The approving maintainer's identity (must be non-blank).
    pub operator: String,
    /// The human reason for the activation (must be non-blank).
    pub reason: String,
    /// When the approval was made.
    pub approved_at: DateTime<Utc>,
    /// The candidate `artifact_id` this approval reviewed — must equal the candidate being
    /// activated, so approving one candidate cannot rubber-stamp another.
    pub reviewed_artifact_id: String,
    /// The explicitly-acknowledged high-risk / partial keys (see [`acknowledgment_key`]).
    #[serde(default)]
    pub acknowledged: Vec<String>,
}

/// The recorded outcome of a successful activation — approval provenance plus the exact
/// predecessor/candidate identities and the acknowledged high-risk entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivationRecord {
    /// The approving maintainer.
    pub operator: String,
    /// The recorded reason.
    pub reason: String,
    /// When the maintainer approved.
    pub approved_at: DateTime<Utc>,
    /// The as-of instant activation was performed at.
    pub activated_at: DateTime<Utc>,
    /// The recomputed `artifact_id` of the snapshot that was replaced (the new predecessor).
    pub predecessor_artifact_id: String,
    /// The `artifact_id` of the newly-installed active snapshot.
    pub candidate_artifact_id: String,
    /// The high-risk / partial keys the approval acknowledged.
    pub acknowledged_high_risk: Vec<String>,
}

/// The recorded outcome of a successful rollback (U2, KTD5). A rollback is a forward-activate
/// of a PRIOR snapshot — it supersedes the current active snapshot with an earlier one — so
/// the record names the direction explicitly: which `artifact_id` was restored to active and
/// which was superseded. Parallels [`ActivationRecord`]'s shape so the operator's gate log and
/// the rollback rehearsal read the same fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackRecord {
    /// The approving maintainer.
    pub operator: String,
    /// The recorded reason.
    pub reason: String,
    /// When the maintainer approved.
    pub approved_at: DateTime<Utc>,
    /// The as-of instant the rollback was performed at.
    pub rolled_back_at: DateTime<Utc>,
    /// The `artifact_id` of the prior snapshot restored to active.
    pub restored_artifact_id: String,
    /// The recomputed `artifact_id` of the snapshot that was superseded (the just-active one).
    pub superseded_artifact_id: String,
}

/// A distinct, typed reason an activation was refused. Every variant leaves the active file
/// byte-identical (installation is the last step).
#[derive(Debug, thiserror::Error)]
pub enum ActivationError {
    /// A required approval field (operator / reason) was blank.
    #[error("activation approval is missing a required field: {field}")]
    ApprovalMissing {
        /// Which field was blank.
        field: String,
    },

    /// The current active snapshot could not be read/parsed to establish the predecessor
    /// identity.
    #[error("could not read the current active snapshot: {message}")]
    ActiveUnreadable {
        /// The underlying rendering.
        message: String,
    },

    /// The candidate snapshot could not be read/parsed.
    #[error("could not read the candidate snapshot: {message}")]
    CandidateUnreadable {
        /// The underlying rendering.
        message: String,
    },

    /// The candidate's declared predecessor does not match the current active snapshot's
    /// recomputed identity — the candidate was built off a stale base.
    #[error("stale base: active is {active}, candidate predecessor is {candidate_predecessor:?}")]
    StaleBase {
        /// The recomputed `artifact_id` of the current active snapshot.
        active: String,
        /// The `predecessor_artifact_id` the candidate declares.
        candidate_predecessor: Option<String>,
    },

    /// The candidate failed revalidation through the real loader at `as_of`.
    #[error("candidate failed revalidation: {0}")]
    Invalid(#[source] CalendarLoadError),

    /// The approval does not review this candidate (or no reviewable diff artifact exists).
    #[error("candidate is unreviewed: {detail}")]
    Unreviewed {
        /// Why the candidate is considered unreviewed.
        detail: String,
    },

    /// One or more HIGH-RISK / partial diff entries were not explicitly acknowledged.
    #[error("unacknowledged high-risk changes: {}", entries.join(", "))]
    UnacknowledgedHighRisk {
        /// The acknowledgment keys that were required but missing from the approval.
        entries: Vec<String>,
    },

    /// The atomic install (tempfile create / write / rename) failed.
    #[error("atomic install failed: {message}")]
    Io {
        /// The underlying I/O rendering.
        message: String,
    },
}

/// A distinct, typed reason a rollback was refused (U2). Like [`ActivationError`], every
/// variant leaves the active file byte-identical (installation is the last step) — a refused
/// rollback is fail-closed, never a silent partial install.
#[derive(Debug, thiserror::Error)]
pub enum RollbackError {
    /// A required approval field (operator / reason) was blank.
    #[error("rollback approval is missing a required field: {field}")]
    ApprovalMissing {
        /// Which field was blank.
        field: String,
    },

    /// The current active snapshot could not be read/parsed to establish the superseded
    /// identity.
    #[error("could not read the current active snapshot: {message}")]
    ActiveUnreadable {
        /// The underlying rendering.
        message: String,
    },

    /// The prior snapshot to restore could not be read/parsed.
    #[error("could not read the prior snapshot: {message}")]
    PriorUnreadable {
        /// The underlying rendering.
        message: String,
    },

    /// The approval does not review the prior snapshot being restored (a rollback cannot
    /// rubber-stamp restoring a different snapshot than the one signed off).
    #[error("prior snapshot is unreviewed: {detail}")]
    Unreviewed {
        /// Why the prior snapshot is considered unreviewed.
        detail: String,
    },

    /// The prior snapshot failed to load/authorize at `as_of` — corrupt, unauthorized,
    /// expired, or an integrity/coverage failure. Fail-closed; never silently Unknown.
    #[error("prior snapshot is unusable at as_of: {0}")]
    PriorInvalid(#[source] CalendarLoadError),

    /// The prior snapshot loads and authorizes, but its materialized coverage no longer
    /// includes the `as_of` operating date — restoring it would leave every Enforced consumer
    /// refusing on `OutOfRange`. Surfaced explicitly, not silently installed.
    #[error("prior snapshot does not cover the as_of date {as_of_date} (coverage lapsed)")]
    PriorDoesNotCoverAsOf {
        /// The as-of civil date (KST) the prior snapshot fails to cover.
        as_of_date: chrono::NaiveDate,
    },

    /// The atomic install (tempfile create / write / rename) failed.
    #[error("atomic install failed: {message}")]
    Io {
        /// The underlying I/O rendering.
        message: String,
    },
}

/// A stable, credential-free acknowledgment key for one diff entry. Used to match an
/// approval's `acknowledged` list against the diff's HIGH-RISK entries so acknowledging one
/// specific change cannot silently cover a different one.
pub fn acknowledgment_key(entry: &DiffEntry) -> String {
    format!(
        "{:?}|{}|{}",
        entry.category,
        entry.date.map(|d| d.to_string()).unwrap_or_default(),
        entry.detail
    )
}

/// The acknowledgment key marking a PARTIAL (source-failure / absence-driven) candidate.
pub const PARTIAL_ACK_KEY: &str = "partial:source-failure";

/// Every acknowledgment key a diff REQUIRES before it can be activated: one per HIGH-RISK
/// entry, plus [`PARTIAL_ACK_KEY`] when the diff is partial (absence-driven). An approval must
/// acknowledge all of these.
pub fn required_acknowledgments(diff: &CategorizedDiff) -> Vec<String> {
    let mut keys: Vec<String> = diff.high_risk_entries().map(acknowledgment_key).collect();
    if diff.partial {
        keys.push(PARTIAL_ACK_KEY.to_string());
    }
    keys
}

/// Activate `candidate_path` over `active_path` under explicit maintainer `approval`,
/// evaluating authorization/validity at `as_of`. On success the candidate is atomically
/// installed (owner-readable `0o600`) and an [`ActivationRecord`] is returned; every refusal
/// leaves the active file byte-identical. See the module docs for the full flow.
pub fn activate(
    active_path: &Path,
    candidate_path: &Path,
    approval: &ActivationApproval,
    as_of: DateTime<Utc>,
) -> Result<ActivationRecord, ActivationError> {
    // 1. Explicit approval: operator + reason must be non-blank.
    if approval.operator.trim().is_empty() {
        return Err(ActivationError::ApprovalMissing {
            field: "operator".to_string(),
        });
    }
    if approval.reason.trim().is_empty() {
        return Err(ActivationError::ApprovalMissing {
            field: "reason".to_string(),
        });
    }

    // 2. Current active snapshot → recomputed predecessor identity.
    let active_bytes =
        std::fs::read(active_path).map_err(|e| ActivationError::ActiveUnreadable {
            message: e.to_string(),
        })?;
    let active: Snapshot =
        serde_json::from_slice(&active_bytes).map_err(|e| ActivationError::ActiveUnreadable {
            message: e.to_string(),
        })?;
    let active_artifact_id = compute_artifact_id(&active);

    // 3. Candidate snapshot → revalidate through the REAL loader at `as_of`.
    let candidate_bytes =
        std::fs::read(candidate_path).map_err(|e| ActivationError::CandidateUnreadable {
            message: e.to_string(),
        })?;
    let candidate: Snapshot = serde_json::from_slice(&candidate_bytes).map_err(|e| {
        ActivationError::CandidateUnreadable {
            message: e.to_string(),
        }
    })?;
    let validated =
        KrxCalendar::from_snapshot(candidate.clone(), as_of).map_err(ActivationError::Invalid)?;
    let candidate_artifact_id = validated.artifact_id().to_string();

    // 4. Predecessor identity (stale-base guard).
    if candidate.predecessor_artifact_id.as_deref() != Some(active_artifact_id.as_str()) {
        return Err(ActivationError::StaleBase {
            active: active_artifact_id,
            candidate_predecessor: candidate.predecessor_artifact_id.clone(),
        });
    }

    // 5. Reviewed: the approval must name THIS candidate.
    if approval.reviewed_artifact_id != candidate_artifact_id {
        return Err(ActivationError::Unreviewed {
            detail: format!(
                "approval reviewed {:?}, candidate is {candidate_artifact_id}",
                approval.reviewed_artifact_id
            ),
        });
    }

    // ...and a reviewable diff artifact must exist AND describe this exact candidate.
    let diff_path = diff_path_for(active_path);
    let diff_bytes = std::fs::read(&diff_path).map_err(|e| ActivationError::Unreviewed {
        detail: format!("no reviewable diff at {}: {e}", diff_path.display()),
    })?;
    let diff: CategorizedDiff =
        serde_json::from_slice(&diff_bytes).map_err(|e| ActivationError::Unreviewed {
            detail: format!("diff artifact is unreadable: {e}"),
        })?;
    if diff.candidate_artifact_id != candidate_artifact_id {
        return Err(ActivationError::Unreviewed {
            detail: format!(
                "diff describes {}, not the candidate {candidate_artifact_id}",
                diff.candidate_artifact_id
            ),
        });
    }

    // 6. High-risk / partial acknowledgement: every required key must be acknowledged.
    let missing: Vec<String> = required_acknowledgments(&diff)
        .into_iter()
        .filter(|k| !approval.acknowledged.iter().any(|a| a == k))
        .collect();
    if !missing.is_empty() {
        return Err(ActivationError::UnacknowledgedHighRisk { entries: missing });
    }

    // 7. Record approval + atomically install (owner-readable 0o600).
    let record = ActivationRecord {
        operator: approval.operator.clone(),
        reason: approval.reason.clone(),
        approved_at: approval.approved_at,
        activated_at: as_of,
        predecessor_artifact_id: active_artifact_id,
        candidate_artifact_id,
        acknowledged_high_risk: approval.acknowledged.clone(),
    };
    atomic_install_owner_only(active_path, &candidate_bytes).map_err(|e| ActivationError::Io {
        message: e.to_string(),
    })?;
    Ok(record)
}

/// Roll the active snapshot back to a `prior_path` snapshot under explicit maintainer
/// `approval`, evaluating authorization/validity/coverage at `as_of` (U2, KTD5).
///
/// Rollback is a forward-activate of an EARLIER snapshot over the same atomic install path
/// ([`atomic_install_owner_only`]) — it deliberately supersedes the current active, so it does
/// NOT apply the stale-base guard `activate` uses (that guard rejects a candidate built off a
/// different predecessor; a rollback is a chosen supersession). Instead it proves the prior
/// snapshot is a valid, authorized [`KrxCalendar`] at `as_of` AND that its materialized
/// coverage still includes the `as_of` operating date — because [`KrxCalendar::from_snapshot`]
/// proves load-validity/authorization but NOT that the snapshot's coverage still reaches today
/// (that is a per-date query returning [`QueryError::OutOfRange`], not a load failure). Without
/// the coverage check an emergency rollback could silently install a snapshot whose coverage
/// has lapsed, leaving every Enforced consumer refusing on `OutOfRange` — an operational halt
/// the operator misreads as "rollback failed" rather than "prior snapshot no longer covers
/// today". Rollback therefore refuses (surfaces) a prior snapshot that is stale for `as_of`.
///
/// Like activation, rollback installs a file and requires a process restart to take effect
/// (the restart-identity proof is the operator's, F4); no hot-reload, no global mutable state.
/// Every refusal leaves the active file byte-identical (installation is the last step).
pub fn rollback(
    active_path: &Path,
    prior_path: &Path,
    approval: &ActivationApproval,
    as_of: DateTime<Utc>,
) -> Result<RollbackRecord, RollbackError> {
    // 1. Explicit approval: operator + reason must be non-blank.
    if approval.operator.trim().is_empty() {
        return Err(RollbackError::ApprovalMissing {
            field: "operator".to_string(),
        });
    }
    if approval.reason.trim().is_empty() {
        return Err(RollbackError::ApprovalMissing {
            field: "reason".to_string(),
        });
    }

    // 2. Current active snapshot → recomputed superseded identity.
    let active_bytes = std::fs::read(active_path).map_err(|e| RollbackError::ActiveUnreadable {
        message: e.to_string(),
    })?;
    let active: Snapshot =
        serde_json::from_slice(&active_bytes).map_err(|e| RollbackError::ActiveUnreadable {
            message: e.to_string(),
        })?;
    let superseded_artifact_id = compute_artifact_id(&active);

    // 3. Prior snapshot → revalidate through the REAL loader at `as_of` (auth + integrity).
    let prior_bytes = std::fs::read(prior_path).map_err(|e| RollbackError::PriorUnreadable {
        message: e.to_string(),
    })?;
    let prior: Snapshot =
        serde_json::from_slice(&prior_bytes).map_err(|e| RollbackError::PriorUnreadable {
            message: e.to_string(),
        })?;
    let validated =
        KrxCalendar::from_snapshot(prior.clone(), as_of).map_err(RollbackError::PriorInvalid)?;
    let restored_artifact_id = validated.artifact_id().to_string();

    // 4. Reviewed: the approval must name the prior snapshot being restored — a rollback
    //    cannot rubber-stamp restoring a different snapshot than the one signed off.
    if approval.reviewed_artifact_id != restored_artifact_id {
        return Err(RollbackError::Unreviewed {
            detail: format!(
                "approval reviewed {:?}, prior snapshot is {restored_artifact_id}",
                approval.reviewed_artifact_id
            ),
        });
    }

    // 5. Coverage-for-`as_of`: the prior snapshot must still cover the operating date, or every
    //    Enforced consumer would refuse on OutOfRange. KRX civil dates are KST (UTC+9). This is
    //    a per-date query — a lapsed-coverage snapshot loads and authorizes cleanly but returns
    //    OutOfRange here, which is exactly the silent-halt this guard turns into an explicit
    //    refusal.
    let as_of_date = (as_of + Duration::hours(9)).date_naive();
    let view = validated.as_of(as_of).map_err(RollbackError::PriorInvalid)?;
    match view.day(as_of_date) {
        Ok(_) => {}
        Err(QueryError::OutOfRange { .. }) | Err(QueryError::DateOverflow) => {
            return Err(RollbackError::PriorDoesNotCoverAsOf { as_of_date });
        }
    }

    // 6. Record + atomically install the prior bytes (owner-readable 0o600). The record names
    //    the restored + superseded identities so the operator's gate log (F4) is self-evident.
    let record = RollbackRecord {
        operator: approval.operator.clone(),
        reason: approval.reason.clone(),
        approved_at: approval.approved_at,
        rolled_back_at: as_of,
        restored_artifact_id,
        superseded_artifact_id,
    };
    atomic_install_owner_only(active_path, &prior_bytes).map_err(|e| RollbackError::Io {
        message: e.to_string(),
    })?;
    Ok(record)
}

/// The acknowledgment key a first-install (genesis) activation REQUIRES — a chain root has no
/// predecessor, so an all-additive genesis diff yields zero computed acknowledgments; this
/// explicit key forces the operator to accept the no-predecessor posture (KTD5).
pub const GENESIS_ACK_KEY: &str = "genesis:no-predecessor";

/// The acknowledgment keys a first-install REQUIRES for `description`: the mandatory
/// [`GENESIS_ACK_KEY`], plus any computed risk key the description ever surfaces. A clean
/// genesis (R12 guarantees zero Unknown consumer weekdays) requires only the genesis key.
pub fn required_genesis_acknowledgments(_description: &GenesisDescription) -> Vec<String> {
    vec![GENESIS_ACK_KEY.to_string()]
}

/// The recorded outcome of a successful first-install (genesis) activation (KTD5). A distinct
/// type beside [`ActivationRecord`] because a chain root has NO predecessor identity to record —
/// the existing `ActivationRecord` serde shape (with its required `predecessor_artifact_id`)
/// stays untouched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenesisActivationRecord {
    /// The approving maintainer.
    pub operator: String,
    /// The recorded reason.
    pub reason: String,
    /// When the maintainer approved.
    pub approved_at: DateTime<Utc>,
    /// The as-of instant the chain root was installed at.
    pub installed_at: DateTime<Utc>,
    /// The `artifact_id` of the newly-installed genesis snapshot (the chain root).
    pub candidate_artifact_id: String,
    /// The acknowledgments the approval carried (includes [`GENESIS_ACK_KEY`]).
    pub acknowledged: Vec<String>,
}

/// A distinct, typed reason a first-install was refused (KTD5). Every variant leaves the install
/// path untouched — the exclusive-create install is the very last step and cannot clobber.
#[derive(Debug, thiserror::Error)]
pub enum FirstInstallError {
    /// A required approval field (operator / reason) was blank.
    #[error("first-install approval is missing a required field: {field}")]
    ApprovalMissing {
        /// Which field was blank.
        field: String,
    },

    /// An active snapshot already exists at the install path (AE8) — superseding a live chain
    /// root requires the normal `activate` path with its stale-base protection, never first-install.
    #[error("an active snapshot already exists at the install path — use the normal activate path")]
    AlreadyActive,

    /// The candidate snapshot could not be read/parsed.
    #[error("could not read the candidate snapshot: {message}")]
    CandidateUnreadable {
        /// The underlying rendering.
        message: String,
    },

    /// The candidate declares a predecessor — it belongs to the normal `activate` path, not genesis.
    #[error("candidate declares predecessor {predecessor} — it is not a genesis (predecessor-less) candidate")]
    HasPredecessor {
        /// The predecessor id the candidate declares.
        predecessor: String,
    },

    /// The candidate failed revalidation through the real loader at `as_of`.
    #[error("candidate failed revalidation: {0}")]
    Invalid(#[source] CalendarLoadError),

    /// The approval does not review this candidate, or no genesis description artifact describes it.
    #[error("candidate is unreviewed: {detail}")]
    Unreviewed {
        /// Why the candidate is considered unreviewed.
        detail: String,
    },

    /// The genesis description's surfaced authorization does not match the candidate's stamped
    /// authorization — the ceremony's agreement-term check is mechanical and this mismatch refuses.
    #[error("authorization mismatch: {detail}")]
    AuthorizationMismatch {
        /// The specific mismatch.
        detail: String,
    },

    /// One or more required acknowledgments (the genesis key + any computed) were missing.
    #[error("unacknowledged: {}", entries.join(", "))]
    Unacknowledged {
        /// The required-but-missing acknowledgment keys.
        entries: Vec<String>,
    },

    /// The install destination appeared AFTER the exists-gate but before the commit — the
    /// exclusive-create install failed atomically, leaving the existing file byte-identical.
    #[error("install path appeared during the ceremony — refused to overwrite a concurrent chain root")]
    RaceLost,

    /// The exclusive-create install (tempfile create / write / link) failed.
    #[error("exclusive install failed: {message}")]
    Io {
        /// The underlying I/O rendering.
        message: String,
    },
}

/// First-install a predecessor-less genesis `candidate` as the chain root at `active_path` under
/// explicit maintainer `approval`, evaluating authorization/validity at `as_of` (U6, KTD5, R9).
///
/// Shares the approval / reviewed-artifact / acknowledgment legs with [`activate`], but replaces
/// the active-load + stale-base legs with a refuse-if-exists guard (a chain root has no
/// predecessor to compare), reviews the GENESIS DESCRIPTION artifact (not a diff) and requires
/// its surfaced authorization to match the candidate's, requires the [`GENESIS_ACK_KEY`]
/// acknowledgment, and commits with an EXCLUSIVE-CREATE install that fails atomically if the
/// destination appeared since the exists-gate — the rename-based installer clobbers and is NOT
/// reused here. Every refusal leaves the install path untouched.
pub fn first_install(
    active_path: &Path,
    candidate_path: &Path,
    approval: &ActivationApproval,
    as_of: DateTime<Utc>,
) -> Result<GenesisActivationRecord, FirstInstallError> {
    // 1. Explicit approval: operator + reason must be non-blank.
    if approval.operator.trim().is_empty() {
        return Err(FirstInstallError::ApprovalMissing {
            field: "operator".to_string(),
        });
    }
    if approval.reason.trim().is_empty() {
        return Err(FirstInstallError::ApprovalMissing {
            field: "reason".to_string(),
        });
    }

    // 2. Refuse if the install path already exists (AE8) — superseding a live chain root goes
    //    through the normal activate path with its stale-base protection.
    if active_path.exists() {
        return Err(FirstInstallError::AlreadyActive);
    }

    // 3. Read + parse the candidate; refuse one that declares a predecessor (normal path).
    let candidate_bytes = std::fs::read(candidate_path).map_err(|e| {
        FirstInstallError::CandidateUnreadable {
            message: e.to_string(),
        }
    })?;
    let candidate: Snapshot = serde_json::from_slice(&candidate_bytes).map_err(|e| {
        FirstInstallError::CandidateUnreadable {
            message: e.to_string(),
        }
    })?;
    if let Some(predecessor) = &candidate.predecessor_artifact_id {
        return Err(FirstInstallError::HasPredecessor {
            predecessor: predecessor.clone(),
        });
    }

    // 3b. Revalidate through the REAL loader at `as_of` (identity + coverage + authorization).
    let validated = KrxCalendar::from_snapshot(candidate.clone(), as_of)
        .map_err(FirstInstallError::Invalid)?;
    let candidate_artifact_id = validated.artifact_id().to_string();

    // 4. Reviewed: the approval must name THIS candidate.
    if approval.reviewed_artifact_id != candidate_artifact_id {
        return Err(FirstInstallError::Unreviewed {
            detail: format!(
                "approval reviewed {:?}, candidate is {candidate_artifact_id}",
                approval.reviewed_artifact_id
            ),
        });
    }

    // 5. Genesis description artifact present, names the candidate, and its authorization matches.
    let desc_path = genesis_description_path_for(candidate_path);
    let desc_bytes = std::fs::read(&desc_path).map_err(|e| FirstInstallError::Unreviewed {
        detail: format!("no genesis description at {}: {e}", desc_path.display()),
    })?;
    let description: GenesisDescription =
        serde_json::from_slice(&desc_bytes).map_err(|e| FirstInstallError::Unreviewed {
            detail: format!("genesis description is unreadable: {e}"),
        })?;
    if description.candidate_artifact_id != candidate_artifact_id {
        return Err(FirstInstallError::Unreviewed {
            detail: format!(
                "description describes {}, not the candidate {candidate_artifact_id}",
                description.candidate_artifact_id
            ),
        });
    }
    if description.authority != candidate.authorization.authority
        || description.granted_at != candidate.authorization.granted_at
        || description.expires_at != candidate.authorization.expires_at
    {
        return Err(FirstInstallError::AuthorizationMismatch {
            detail: "genesis description authorization does not match the candidate's stamped terms"
                .to_string(),
        });
    }

    // 6. Acknowledgments: the genesis key (+ any computed) must all be acknowledged.
    let missing: Vec<String> = required_genesis_acknowledgments(&description)
        .into_iter()
        .filter(|k| !approval.acknowledged.iter().any(|a| a == k))
        .collect();
    if !missing.is_empty() {
        return Err(FirstInstallError::Unacknowledged { entries: missing });
    }

    // 7. Record + exclusive-create install — fails atomically if the destination appeared since
    //    gate 2, so emptiness is re-proven at commit time, not just at gate time.
    let record = GenesisActivationRecord {
        operator: approval.operator.clone(),
        reason: approval.reason.clone(),
        approved_at: approval.approved_at,
        installed_at: as_of,
        candidate_artifact_id,
        acknowledged: approval.acknowledged.clone(),
    };
    match atomic_install_exclusive(active_path, &candidate_bytes) {
        Ok(()) => Ok(record),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(FirstInstallError::RaceLost),
        Err(e) => Err(FirstInstallError::Io {
            message: e.to_string(),
        }),
    }
}

/// Exclusive-create install (KTD5): write `bytes` to a PROCESS-UNIQUE sibling `0o600` tempfile
/// created with `create_new` (O_EXCL — never reuse, truncate, or follow a symlink at the temp
/// path), then atomically create `dest` as a hard link — which FAILS with
/// [`AlreadyExists`](std::io::ErrorKind::AlreadyExists) if `dest` appeared, never clobbering it.
/// The rename-based [`atomic_install_owner_only`] would silently overwrite, so it is deliberately
/// NOT reused for a chain-root install. The process-unique temp name means two concurrent
/// first-installs never share an inode (which would let one install `dest` with the OTHER's bytes
/// while recording its own identity). The temp is always cleaned up; on a lost race `dest` is left
/// byte-identical. (A stale same-name temp from a crashed prior run surfaces the temp create's
/// `AlreadyExists` as a fail-closed refusal, not an install.)
fn atomic_install_exclusive(dest: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = first_install_temp(dest);
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    let result = std::fs::hard_link(&tmp, dest);
    let _ = std::fs::remove_file(&tmp);
    result
}

fn first_install_temp(dest: &Path) -> PathBuf {
    // Process-unique so concurrent first-installs never share the same temp inode, and
    // deterministic within a process so the ceremony's single synchronous call is self-consistent.
    let mut name = dest.as_os_str().to_os_string();
    name.push(format!(".first-install.{}.tmp", std::process::id()));
    PathBuf::from(name)
}

/// Write `bytes` to a sibling tempfile created with `0o600` permissions (owner read/write
/// ONLY) then `rename` it over `active_path` — atomic, no partial state, never world-readable.
/// The mode is set BOTH at create time (`OpenOptions::mode`) and re-asserted with
/// `set_permissions` so a pre-existing temp or a lenient umask cannot widen it.
fn atomic_install_owner_only(active_path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = sibling_temp(active_path);
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    // Re-assert 0o600 in case the temp path pre-existed with wider bits.
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    std::fs::rename(&tmp, active_path)
}

fn sibling_temp(active_path: &Path) -> PathBuf {
    let mut name = active_path.as_os_str().to_os_string();
    name.push(".activate.tmp");
    PathBuf::from(name)
}

#[cfg(test)]
mod exclusive_install_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn exclusive_install_creates_owner_only_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("cal.json");
        atomic_install_exclusive(&dest, b"genesis-bytes").expect("fresh install succeeds");
        assert_eq!(std::fs::read(&dest).unwrap(), b"genesis-bytes");
        let mode = std::fs::metadata(&dest).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "installed 0o600");
        assert!(!first_install_temp(&dest).exists(), "no temp residue");
    }

    #[test]
    fn exclusive_install_refuses_to_clobber_an_existing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("cal.json");
        std::fs::write(&dest, b"pre-existing-chain-root").unwrap();
        let err = atomic_install_exclusive(&dest, b"would-clobber").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists, "the race is lost, not clobbered");
        // The existing file is byte-identical and no temp residue remains at the destination.
        assert_eq!(std::fs::read(&dest).unwrap(), b"pre-existing-chain-root");
        assert!(!first_install_temp(&dest).exists(), "no temp residue after a lost race");
    }

    #[test]
    fn exclusive_install_never_reuses_or_truncates_a_pre_existing_temp() {
        // create_new (O_EXCL) rejects a stale/planted temp rather than reusing or truncating it —
        // so a concurrent writer's inode or a symlink at the temp path can never be adopted.
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("cal.json");
        let tmp = first_install_temp(&dest);
        std::fs::write(&tmp, b"stale-temp").unwrap();
        let err = atomic_install_exclusive(&dest, b"genesis-bytes").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists, "a pre-existing temp is rejected");
        assert!(!dest.exists(), "no install happened");
    }
}
