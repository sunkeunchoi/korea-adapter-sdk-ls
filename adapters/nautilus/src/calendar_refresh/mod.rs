//! Maintainer refresh tooling (U14, KTD9) — candidate build + deterministic categorized
//! diff, NEVER overwriting the active snapshot.
//!
//! Refresh normalizes evidence through an injectable [`EvidenceInputPort`], recomputes a
//! CANDIDATE [`Snapshot`] ([`build_candidate`]) whose `predecessor_artifact_id` is the EXACT
//! active predecessor, diffs it against that predecessor into a [`CategorizedDiff`]
//! ([`diff_against_predecessor`]), and writes the candidate + diff to SEPARATE paths
//! ([`write_candidate`]) — the active file is never touched (activation is U15).
//!
//! Two modes ([`refresh_incremental`], [`refresh_full_history`]) are thin scopes over the
//! same [`refresh`] core. The live transport ([`LiveEvidencePort`]) is a separate impl fed
//! only maintainer-local credentials; the offline gate uses [`StaticEvidencePort`].
//!
//! ## U15 consumption contract
//!
//! Activation (U15) reads the candidate + its declared `predecessor_artifact_id`, loads the
//! current active snapshot, and refuses unless `active.artifact_id ==
//! candidate.predecessor_artifact_id` (stale-base guard). [`write_candidate`] returns the
//! [`CandidateArtifacts`] paths U15 revalidates + atomically installs.

pub mod activate;
pub mod candidate;
pub mod diff;
pub mod normalize;
pub mod port;
pub mod transport;

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, NaiveDate, Utc};

use nautilus_ls_calendar::schema::Snapshot;

pub use activate::{
    acknowledgment_key, activate, required_acknowledgments, rollback, ActivationApproval,
    ActivationError, ActivationRecord, RollbackError, RollbackRecord, PARTIAL_ACK_KEY,
};
pub use candidate::{
    build_candidate, build_genesis, GenesisParams, GenesisRefusal, RefreshMode,
    CONSUMER_WINDOW_START,
};
pub use diff::{diff_against_predecessor, CategorizedDiff, DiffCategory, DiffEntry};
pub use normalize::{
    fixed_closure_rules, generated_rules, holiday_evidence, midnight_utc, parse_kasi_holidays_xml,
    parse_krx_daily, weekend_rules, witness_evidence, KasiPage,
};
pub use port::{
    merge_ranges, uncovered_within, DateRange, EvidenceInputPort, RefreshInputs, RefreshScope,
    SourceFetchStatus, SourceOutcome, StaticEvidencePort,
};
pub use transport::{
    strip_url_credentials, LiveEvidencePort, MaintainerCredentials, KASI_SERVICE_KEY_ENV,
    KRX_APPKEY_ENV,
};

/// The KRX daily-market history floor (KTD7): full-history refresh starts here.
pub const HISTORY_FLOOR: (i32, u32, u32) = (2010, 1, 4);

/// Days BEFORE the as-of KST date the default operating horizon reaches back.
pub const OPERATING_HORIZON_BACK_DAYS: i64 = 7;
/// Days AFTER the as-of KST date the default operating horizon reaches forward (mirrors the
/// 45-day forward-readiness dimension).
pub const OPERATING_HORIZON_FORWARD_DAYS: i64 = 45;

/// The outcome of a refresh: the candidate snapshot + its categorized diff against the exact
/// active predecessor.
#[derive(Debug, Clone)]
pub struct RefreshOutcome {
    /// The recomputed candidate (identities stamped, predecessor set). Never written over
    /// the active file.
    pub candidate: Snapshot,
    /// The deterministic categorized diff vs. the exact active predecessor.
    pub diff: CategorizedDiff,
}

/// The filesystem paths a candidate + diff were written to (never the active path).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateArtifacts {
    /// The candidate snapshot path (`<active>.candidate`).
    pub candidate_path: PathBuf,
    /// The categorized-diff path (`<active>.candidate.diff.json`).
    pub diff_path: PathBuf,
}

/// Run a refresh: build the candidate from `prior` + `port` over `scope`, then diff it
/// against `prior` within `operating_horizon`. The `partial` provenance is derived from the
/// gathered source outcomes (any source failure ⇒ partial candidate requiring review).
pub fn refresh(
    prior: &Snapshot,
    port: &dyn EvidenceInputPort,
    scope: RefreshScope,
    mode: RefreshMode,
    as_of: DateTime<Utc>,
    operating_horizon: (NaiveDate, NaiveDate),
) -> RefreshOutcome {
    let inputs = port.gather(&scope);
    let partial = inputs.outcomes.iter().any(|o| !o.is_ok());
    let candidate = build_candidate(prior, &inputs, &scope, mode, as_of);
    let diff = diff_against_predecessor(prior, &candidate, operating_horizon, partial);
    RefreshOutcome { candidate, diff }
}

/// Incremental refresh: recompute the elapsed dates AFTER the predecessor's materialized
/// window through `elapsed_through`. Advances the incremental freshness dimension on full
/// success. Uses the default operating horizon around `as_of` (KST).
pub fn refresh_incremental(
    prior: &Snapshot,
    port: &dyn EvidenceInputPort,
    as_of: DateTime<Utc>,
    elapsed_through: NaiveDate,
) -> RefreshOutcome {
    let from = prior
        .coverage
        .materialized_through
        .succ_opt()
        .unwrap_or(prior.coverage.materialized_through);
    let scope = RefreshScope {
        from,
        through: elapsed_through.max(prior.coverage.materialized_through),
    };
    refresh(
        prior,
        port,
        scope,
        RefreshMode::Incremental,
        as_of,
        default_operating_horizon(as_of),
    )
}

/// Full-history refresh: recompute from the KRX history floor (2010-01-04) through
/// `through`. Advances the full-history freshness dimension on full success.
pub fn refresh_full_history(
    prior: &Snapshot,
    port: &dyn EvidenceInputPort,
    as_of: DateTime<Utc>,
    through: NaiveDate,
) -> RefreshOutcome {
    let (y, m, d) = HISTORY_FLOOR;
    let floor = NaiveDate::from_ymd_opt(y, m, d).expect("history floor is valid");
    let scope = RefreshScope {
        from: floor.min(prior.coverage.materialized_from),
        through: through.max(prior.coverage.materialized_through),
    };
    refresh(
        prior,
        port,
        scope,
        RefreshMode::FullHistory,
        as_of,
        default_operating_horizon(as_of),
    )
}

/// The default operating horizon (KST civil dates) around `as_of`: back
/// [`OPERATING_HORIZON_BACK_DAYS`], forward [`OPERATING_HORIZON_FORWARD_DAYS`].
pub fn default_operating_horizon(as_of: DateTime<Utc>) -> (NaiveDate, NaiveDate) {
    let kst_today = (as_of + Duration::hours(9)).date_naive();
    (
        kst_today - Duration::days(OPERATING_HORIZON_BACK_DAYS),
        kst_today + Duration::days(OPERATING_HORIZON_FORWARD_DAYS),
    )
}

/// Write the candidate + diff to paths DERIVED FROM the active path, never touching the
/// active file (`<active>.candidate` + `<active>.candidate.diff.json`). Atomic temp+rename
/// per file. The active snapshot is left byte-identical.
///
/// Returns the [`CandidateArtifacts`] paths U15 revalidates + atomically installs.
pub fn write_candidate(
    active_path: &Path,
    outcome: &RefreshOutcome,
) -> std::io::Result<CandidateArtifacts> {
    let candidate_path = candidate_path_for(active_path);
    let diff_path = diff_path_for(active_path);

    let candidate_json = serde_json::to_vec_pretty(&outcome.candidate)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let diff_json = serde_json::to_vec_pretty(&outcome.diff)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    atomic_write(&candidate_path, &candidate_json)?;
    atomic_write(&diff_path, &diff_json)?;

    Ok(CandidateArtifacts {
        candidate_path,
        diff_path,
    })
}

/// The candidate path for an active snapshot path (`<active>.candidate`).
pub fn candidate_path_for(active_path: &Path) -> PathBuf {
    append_ext(active_path, "candidate")
}

/// The categorized-diff path for an active snapshot path (`<active>.candidate.diff.json`).
pub fn diff_path_for(active_path: &Path) -> PathBuf {
    append_ext(active_path, "candidate.diff.json")
}

fn append_ext(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".");
    name.push(suffix);
    PathBuf::from(name)
}

/// Atomic sibling-temp + rename write (mirrors the ingest checkpoint write hardening and
/// [`atomic_install_owner_only`](activate::atomic_install_owner_only)). The candidate is a
/// full Snapshot with the same license-restricted KRX-derived rows as production, and the
/// diff carries KRX/KASI-derived facts, so the tempfile is created `0o600` (owner read/write
/// ONLY) and re-asserted with `set_permissions` before rename — never at the umask-default
/// world-readable `0o644`.
fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let tmp = append_ext(path, "tmp");
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
    std::fs::rename(&tmp, path)
}
