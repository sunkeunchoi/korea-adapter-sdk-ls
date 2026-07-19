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

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use nautilus_ls_calendar::schema::Snapshot;
use nautilus_ls_calendar::{compute_artifact_id, CalendarLoadError, KrxCalendar};

use super::diff::{CategorizedDiff, DiffEntry};
use super::diff_path_for;

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
