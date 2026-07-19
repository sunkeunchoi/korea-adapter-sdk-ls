//! The injectable evidence input port (U14, KTD9).
//!
//! Refresh never touches transport directly: it pulls NORMALIZED evidence through an
//! [`EvidenceInputPort`]. The offline gate feeds a [`StaticEvidencePort`] with synthetic
//! inputs; the maintainer-run live transport
//! ([`LiveEvidencePort`](crate::calendar_refresh::transport::LiveEvidencePort)) is a
//! separate impl that yields the same [`RefreshInputs`] shape. Raw KRX/KASI bodies never
//! reach the port's output — a port yields already-normalized [`EvidenceRecord`]s + typed
//! per-source fetch outcomes, so a failed source is recorded (retention) rather than
//! silently dropping accepted evidence.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use nautilus_ls_calendar::schema::{EvidenceRecord, Source, SourceKind};

/// The civil-date span a refresh recomputes. Both endpoints inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshScope {
    /// First civil date in scope.
    pub from: NaiveDate,
    /// Last civil date in scope.
    pub through: NaiveDate,
}

/// Whether a source's fetch during a refresh succeeded or failed. A failed status carries
/// a CREDENTIAL-SAFE reason (the transport strips query-string credentials before it ever
/// reaches this field — see [`strip_url_credentials`](crate::calendar_refresh::transport::strip_url_credentials)).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum SourceFetchStatus {
    /// The source responded and its evidence is in [`RefreshInputs::evidence`].
    Ok,
    /// The source failed. Its accepted evidence + coverage are RETAINED (never removed by
    /// absence); this reason is credential-safe.
    Failed {
        /// A credential-safe description of the failure.
        reason: String,
    },
}

/// The per-source outcome of a refresh attempt. Source-failure retention is driven off
/// these: a [`Failed`](SourceFetchStatus::Failed) source keeps its prior evidence/coverage
/// and ages its freshness dimension, and can never expand coverage by absence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceOutcome {
    /// The [`Source::id`] this outcome is for.
    pub source_id: String,
    /// The kind of source (drives which freshness dimension ages).
    pub kind: SourceKind,
    /// Whether the fetch succeeded or failed.
    pub status: SourceFetchStatus,
}

impl SourceOutcome {
    /// A successful outcome for `source_id`.
    pub fn ok(source_id: impl Into<String>, kind: SourceKind) -> Self {
        SourceOutcome {
            source_id: source_id.into(),
            kind,
            status: SourceFetchStatus::Ok,
        }
    }

    /// A failed outcome for `source_id` with a credential-safe `reason`.
    pub fn failed(source_id: impl Into<String>, kind: SourceKind, reason: impl Into<String>) -> Self {
        SourceOutcome {
            source_id: source_id.into(),
            kind,
            status: SourceFetchStatus::Failed {
                reason: reason.into(),
            },
        }
    }

    /// `true` iff the fetch succeeded.
    pub fn is_ok(&self) -> bool {
        matches!(self.status, SourceFetchStatus::Ok)
    }

    /// The credential-safe failure reason, if this outcome failed.
    pub fn failure_reason(&self) -> Option<&str> {
        match &self.status {
            SourceFetchStatus::Failed { reason } => Some(reason.as_str()),
            SourceFetchStatus::Ok => None,
        }
    }
}

/// The normalized inputs a port yields for a refresh scope: the source records, the
/// normalized evidence the SUCCESSFUL sources produced, and the per-source outcomes. Never
/// carries a raw response body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefreshInputs {
    /// The normalized [`Source`] records the successful sources correspond to.
    pub sources: Vec<Source>,
    /// The normalized evidence records the successful sources produced.
    pub evidence: Vec<EvidenceRecord>,
    /// The per-source fetch outcomes (retention is driven off these).
    pub outcomes: Vec<SourceOutcome>,
}

impl RefreshInputs {
    /// An empty input set (every source absent).
    pub fn empty() -> Self {
        RefreshInputs {
            sources: Vec::new(),
            evidence: Vec::new(),
            outcomes: Vec::new(),
        }
    }
}

/// The injectable evidence input port. `gather` yields already-normalized evidence for the
/// scope — no raw bodies, no persisted responses. Implementations: [`StaticEvidencePort`]
/// (synthetic, offline) and the maintainer live transport (separate impl).
pub trait EvidenceInputPort {
    /// Gather normalized evidence + per-source outcomes for `scope`.
    fn gather(&self, scope: &RefreshScope) -> RefreshInputs;
}

/// A port over a precomputed [`RefreshInputs`] — the offline/synthetic driver, and the
/// shape the maintainer CLI uses when fed a reviewed normalized-inputs file.
#[derive(Debug, Clone)]
pub struct StaticEvidencePort {
    inputs: RefreshInputs,
}

impl StaticEvidencePort {
    /// Wrap precomputed `inputs`.
    pub fn new(inputs: RefreshInputs) -> Self {
        StaticEvidencePort { inputs }
    }
}

impl EvidenceInputPort for StaticEvidencePort {
    fn gather(&self, _scope: &RefreshScope) -> RefreshInputs {
        self.inputs.clone()
    }
}
