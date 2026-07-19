//! The self-contained snapshot schema (U1).
//!
//! A [`Snapshot`] is the immutable JSON artifact the runtime calendar loads. It carries
//! everything needed to answer day/range facts offline with no network and no hidden
//! clock: schema identity, the two deterministic identities ([`Snapshot::artifact_id`],
//! [`Snapshot::calendar_id`] — computed by U2, left as plain string fields here),
//! calendar scope, authorization with recorded expiry/termination, distinct coverage
//! claims (KTD, AC8), freshness dimensions, normalized sources, evidence records,
//! alerts, and one tri-state [`DayRow`] per materialized civil date.
//!
//! Refs between collections are plain `String` ids (evidence ids, source ids, alert
//! ids) — simple and serde-round-trippable. A row references evidence/alerts by id;
//! evidence references its source by id.
//!
//! This unit defines only the serde shapes and derives `Debug, Clone, PartialEq,
//! Serialize, Deserialize`. Behavior is layered on by later units.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// The tri-state day fact (CONCEPTS.md "Trading Session status"). `Unknown` is a
/// *successful* factual result meaning maintained evidence does not cover the date — it
/// is never collapsed into `Closed`, and never produced by a load/use/query failure
/// (those are typed errors, KTD3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DayStatus {
    /// A regular KRX domestic-equity trading session positively holds on this date.
    TradingSession,
    /// The venue is proven closed on this date (rule, holiday+rule, or cited notice).
    Closed,
    /// Maintained evidence does not cover this date. A successful factual result, not an
    /// error and not a closure.
    Unknown,
}

/// The complete snapshot artifact — the calendar's on-disk / in-memory JSON shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// SemVer schema-compatibility version (U2 defines the compatibility predicate).
    pub schema_version: String,
    /// Deterministic hash of canonical content excluding this field (KTD4). Computed by
    /// U2; a plain field here (empty on a hand-built value before hashing).
    pub artifact_id: String,
    /// Deterministic hash of effective statuses + decisive claim identities, excluding
    /// retrieval mechanics (KTD4). Computed by U2; a plain field here.
    pub calendar_id: String,
    /// The `artifact_id` of the snapshot this one supersedes, if any.
    pub predecessor_artifact_id: Option<String>,
    /// What venue/instrument/timezone this calendar scopes.
    pub scope: CalendarScope,
    /// Recorded authorization to use the (license-restricted) calendar data, with expiry
    /// / termination instants. Evaluated against a caller-supplied as-of instant (KTD5).
    pub authorization: Authorization,
    /// The distinct coverage claims (AC8) — kept separate, never conflated.
    pub coverage: Coverage,
    /// The freshness dimensions used by the boundary-time staleness checks (U7).
    pub freshness: Freshness,
    /// The normalized evidence sources referenced by [`EvidenceRecord::source_id`].
    pub sources: Vec<Source>,
    /// The normalized evidence records referenced by [`DayRow::decisive_evidence`] /
    /// [`DayRow::conflicting_evidence`].
    pub evidence: Vec<EvidenceRecord>,
    /// The reconciliation alerts referenced by [`DayRow::alerts`].
    pub alerts: Vec<Alert>,
    /// One tri-state row per materialized civil date. A date with no row is *absent*
    /// (distinct from a materialized `Unknown` row).
    pub rows: Vec<DayRow>,
}

/// What this calendar scopes. `synthetic` marks a fixture snapshot that must never be
/// mistaken for a real KRX calendar (U7, AC11).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarScope {
    /// Human calendar name (e.g. "KRX domestic equity regular session").
    pub calendar_name: String,
    /// Venue / MIC (e.g. "XKRX").
    pub venue: String,
    /// Instrument class in scope (e.g. "domestic-equity").
    pub instrument_class: String,
    /// IANA timezone the civil dates are interpreted in (e.g. "Asia/Seoul").
    pub timezone: String,
    /// `true` iff this is an explicitly-synthetic / counterfactual snapshot.
    pub synthetic: bool,
}

/// Recorded authorization to use the calendar data. Redactable identity fields
/// (`authority`) are masked by the diagnostics layer (U8), never rendered raw.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Authorization {
    /// Whether the recorded grant authorizes use (subject to expiry/termination + as-of).
    pub authorized: bool,
    /// The granting authority / agreement identity (REDACTABLE — see U8).
    pub authority: String,
    /// When authorization was granted.
    pub granted_at: DateTime<Utc>,
    /// When authorization expires, if bounded. Evaluated at the caller's as-of instant.
    pub expires_at: Option<DateTime<Utc>>,
    /// When authorization was terminated early, if it has been.
    pub terminated_at: Option<DateTime<Utc>>,
}

/// The distinct coverage claims (AC8) — deliberately kept as separate fields so no
/// caller conflates "we have positive daily rows through X" with "we retrospectively
/// re-checked through Y" or "we evaluated scheduled closures through Z".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Coverage {
    /// First civil date materialized as a row.
    pub materialized_from: NaiveDate,
    /// Last civil date materialized as a row (every date in `[from, through]` has a row).
    pub materialized_through: NaiveDate,
    /// Last date whose evidence was retrospectively re-checked/reconciled.
    pub retrospectively_checked_through: NaiveDate,
    /// Last date through which scheduled (rule-based) closures were evaluated.
    pub scheduled_closure_evaluated_through: NaiveDate,
    /// Per-source availability bounds (retrieval mechanics — excluded from `calendar_id`).
    pub source_availability: Vec<SourceAvailabilityBound>,
}

/// The date range a given source's evidence is available for (retrieval-mechanic
/// metadata, not an effective-status input).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceAvailabilityBound {
    /// The [`Source::id`] this bound applies to.
    pub source_id: String,
    /// Earliest date this source can witness, if bounded.
    pub available_from: Option<NaiveDate>,
    /// Latest date this source can witness, if bounded.
    pub available_through: Option<NaiveDate>,
}

/// The freshness dimensions consumed by the boundary-time staleness checks (U7). Each is
/// an instant the corresponding refresh dimension last ran; staleness is evaluated at the
/// caller's as-of instant and never rewrites a status (AC8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Freshness {
    /// When the evidence set was last refreshed overall.
    pub evidence_refreshed_at: DateTime<Utc>,
    /// When holiday facts (KASI) were last checked — 14-day threshold dimension.
    pub holiday_facts_checked_at: Option<DateTime<Utc>>,
    /// When the full history was last reconciled — 120-day threshold dimension.
    pub full_history_reconciled_at: Option<DateTime<Utc>>,
    /// The forward horizon scheduled closures are established through — 45-day
    /// forward-readiness dimension.
    pub forward_readiness_through: Option<NaiveDate>,
    /// When the last incremental refresh ran — two-missed-opportunity dimension.
    pub last_incremental_at: Option<DateTime<Utc>>,
}

/// A normalized evidence source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// Stable id referenced by [`EvidenceRecord::source_id`] and
    /// [`SourceAvailabilityBound::source_id`].
    pub id: String,
    /// What kind of source this is.
    pub kind: SourceKind,
    /// Human label (REDACTABLE if it carries identity — see U8).
    pub label: String,
    /// `true` iff this is an explicitly-synthetic source.
    pub synthetic: bool,
}

/// The kind of a normalized evidence source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// KRX daily-market API (`stk_bydd_trd`) — positive Trading Session witnesses (KTD7).
    KrxDailyMarket,
    /// KASI public-holiday facts.
    KasiHoliday,
    /// A deterministic published KRX rule (weekend / Labor Day / year-end / holiday link).
    KrxRule,
    /// A cited first-party KRX notice (e.g. an exceptional closure).
    FirstPartyNotice,
    /// A maintainer correction targeting identified evidence.
    Correction,
    /// A human adjudication (rationale + citation + maintainer identity).
    HumanAdjudication,
}

/// A normalized evidence record. The reconciliation layer (U5) reads these; the loader
/// (U3) validates their reference integrity and supersession chains.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceRecord {
    /// Stable id referenced by [`DayRow`] evidence-ref lists and by supersession.
    pub id: String,
    /// The [`Source::id`] this evidence came from.
    pub source_id: String,
    /// The civil date this evidence bears on.
    pub date: NaiveDate,
    /// What kind of claim this evidence makes.
    pub kind: EvidenceKind,
    /// Whether this record is currently valid (a human adjudication can flip validity
    /// without writing a status — KTD6).
    pub valid: bool,
    /// The id of the evidence record that supersedes this one, if any (corrections
    /// supersede only *identified* evidence — no generic newest-wins, KTD6).
    pub superseded_by: Option<String>,
    /// The first-party citation backing this evidence, when the kind requires one
    /// (closure notices, corrections, adjudications).
    pub citation: Option<Citation>,
    /// When this evidence record was recorded.
    pub recorded_at: DateTime<Utc>,
}

/// The kind of claim an [`EvidenceRecord`] makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// A KRX positive Trading Session witness (KTD7).
    PositiveWitness,
    /// A KASI public-holiday fact.
    HolidayFact,
    /// A deterministic KRX rule outcome (weekend / Labor Day / year-end / holiday link).
    DeterministicRule,
    /// A cited first-party closure notice.
    ClosureNotice,
    /// A correction targeting identified evidence.
    Correction,
    /// A human adjudication (validity/supersession only, never a direct status).
    Adjudication,
}

/// A structured, verifiable first-party citation (never free text — U12 depends on this
/// shape for the attended override basis).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Citation {
    /// The verifiable reference (URL, notice number, document id).
    pub reference: String,
    /// The issuing authority (e.g. "KRX", "KASI").
    pub issuer: String,
    /// Optional human note.
    pub note: Option<String>,
}

/// A reconciliation alert (KTD6) — a retained disagreement/override annotation attached
/// to a date, referenced by [`DayRow::alerts`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    /// Stable id referenced by [`DayRow::alerts`].
    pub id: String,
    /// The civil date this alert bears on.
    pub date: NaiveDate,
    /// What kind of alert this is.
    pub kind: AlertKind,
    /// Human-readable alert message.
    pub message: String,
}

/// The kind of a reconciliation [`Alert`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertKind {
    /// A KRX positive witness overrode an inferred closure (disagreement retained).
    WitnessOverridesInference,
    /// A positive witness conflicted with a direct first-party closure notice → Unknown.
    WitnessVsClosureNotice,
    /// Two first-party claims disagree → Unknown.
    FirstPartyConflict,
    /// A later empty/malformed response was ignored (absence never retracts a witness).
    AbsenceIgnored,
    /// Evidence was superseded by an explicit correction.
    Superseded,
    /// A human adjudication changed evidence validity/supersession.
    Adjudicated,
}

/// One tri-state row per materialized civil date. Carries ONLY the date, the reconciled
/// status, and id-refs into the snapshot's evidence/alert collections — no embedded
/// evidence, so the row stays minimal and the collections stay the single source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DayRow {
    /// The civil date.
    pub date: NaiveDate,
    /// The reconciled tri-state status.
    pub status: DayStatus,
    /// Ids of the evidence records decisive for this status.
    pub decisive_evidence: Vec<String>,
    /// Ids of evidence records that conflicted but did not decide (disagreement retained).
    pub conflicting_evidence: Vec<String>,
    /// Ids of the alerts attached to this date.
    pub alerts: Vec<String>,
}
