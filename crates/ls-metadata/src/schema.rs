//! Serde structs for per-TR maintenance metadata and the routing index.
//!
//! These types mirror the per-TR YAML and `tr-index.yaml` shapes documented in
//! `docs/plans/maintained-sdk-migration-plan.md`. They are the Rust-owned schema
//! authority (ADR 0012): there is no parallel hand-maintained JSON Schema.
//!
//! Closed-set fields are modelled as enums so an unknown value is a deserialize
//! error located at the field. Fields whose value space is genuinely open
//! (`name`, `source_spec_hash`, `last_reviewed`, caller-supplied identifiers)
//! stay `String`.
//!
//! All types are `pub` so a future `ls-core` dev-test can load the parsed index
//! and cross-check each `{TR}_POLICY` runtime const against it.

use serde::{Deserialize, Serialize};

use crate::shape::TrShape;

/// The dependency class that owns a TR. Exactly one per TR — enforced
/// structurally by this being a single (non-`Vec`) field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerClass {
    Standalone,
    MarketSession,
    Paginated,
    Account,
    Orders,
    Realtime,
    PaperIncompatible,
}

/// Transport protocol facet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Rest,
    Websocket,
}

/// Rate-limiter bucket facet. Mirrors `ls_core::RateLimitCategory` vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateBucket {
    MarketData,
    Orders,
    Account,
    Auth,
}

/// The LS market/product area a TR belongs to. Modelled as a closed enum: the
/// migration plan enumerates a fixed instrument-domain vocabulary (domestic
/// stock, futures/options, overseas stock, overseas futures, sector/index,
/// realtime invest, misc). Unknown values surface as validation errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentDomain {
    Stock,
    FuturesOptions,
    OverseasStock,
    OverseasFutures,
    SectorIndex,
    RealtimeInvest,
    Misc,
}

/// The venue/session a TR's read is scoped to. Closed set: KRX regular/extended
/// sessions plus an `unspecified` marker for TRs without a session constraint
/// (e.g. auth primitives). Unknown values surface as validation errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VenueSession {
    KrxRegular,
    KrxExtended,
    Unspecified,
}

/// How a TR's behavior is certified/evidenced. Closed set per the migration
/// plan (`certification_path: automated` is the canonical example); `manual`
/// covers guarded operator evidence (e.g. orders) and `none` covers untracked
/// surfaces. Unknown values surface as validation errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CertificationPath {
    Automated,
    Manual,
    None,
}

/// Multi-valued facet metadata for test/evidence/doc/operator routing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facets {
    pub protocol: Protocol,
    pub instrument_domain: InstrumentDomain,
    pub venue_session: VenueSession,
    pub date_sensitive: bool,
    pub self_paginated: bool,
    pub account_state: bool,
    pub paper_incompatible: bool,
    pub certification_path: CertificationPath,
    pub rate_bucket: RateBucket,
    #[serde(default)]
    pub caller_supplied_identifiers: Vec<String>,
}

/// Prerequisite coupling: self-continuation (pagination) and order-number
/// coupling fields.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Dependencies {
    #[serde(default)]
    pub self_continuation_fields: Vec<String>,
    #[serde(default)]
    pub strong_order_fields: Vec<String>,
}

/// Support state: tracked / implemented / recommended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Support {
    pub tracked: bool,
    pub implemented: bool,
    pub recommended: bool,
}

/// Maintenance bookkeeping: upstream spec hash and last-review date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Maintenance {
    pub source_spec_hash: String,
    pub last_reviewed: String,
}

/// The user-facing recommendation contract for a **Recommended TR**. Carries the
/// narrative the page cannot derive — what behavior is recommended and what the
/// claim explicitly does *not* cover — plus a link to the Focused Evidence record
/// backing it. The freshness date stays on [`Maintenance::last_reviewed`] and the
/// environment level is read from the evidence record, so neither is duplicated
/// here. Required when `support.recommended == true`, absent otherwise — enforced
/// by the validator, not the type system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recommendation {
    /// What behavior is recommended (e.g. `"Paper OAuth access-token issuance"`).
    pub behavior: String,
    /// What the recommendation explicitly does **not** claim. Each entry is one
    /// excluded claim rendered verbatim into the contract's "does not claim" list.
    #[serde(default)]
    pub excludes: Vec<String>,
    /// Path of the **Focused Evidence** record backing the claim, relative to the
    /// metadata root (e.g. `evidence/token.yaml`). The validator resolves it and
    /// cross-checks its date against `maintenance.last_reviewed`; `ls-docgen`
    /// renders the record's environment level into the contract.
    pub evidence_ref: String,
}

/// A **Focused Evidence** record (`metadata/evidence/<tr>.yaml`): the durable,
/// credential-free proof backing a Recommended TR's claim. Parsed (rather than
/// only convention-linked by filename) so the validator can cross-check `date`
/// against `maintenance.last_reviewed` and `ls-docgen` can render the `env`
/// level. Extra fields in the file (e.g. `target`, `line`) are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRecord {
    pub tr_code: String,
    pub date: String,
    pub env: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    /// The structural API shape this evidence was attested against — an
    /// independent committed snapshot frozen at attestation time (R1, R4).
    /// Change-driven staling diffs this against the current committed baseline
    /// shape and keeps only qualifying changes (KTD1). Stored as the full
    /// [`crate::shape::TrShape`] (never an opaque hash) so a later diff can be
    /// classified as qualifying-or-not, and never a raw `serde_json::Value`, so
    /// no scalar sample value can land here. The stored shape is itself a
    /// frozen-format contract: see [`crate::shape`] on the additive-only rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attested_shape: Option<TrShape>,
    /// The `NORMALIZER_VERSION` the [`attested_shape`](Self::attested_shape) was
    /// captured under (R2a). A mismatch against the baseline manifest's version
    /// triggers re-attestation rather than a stale-by-change finding — a pure
    /// representation shift must never qualify as staling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attested_normalizer_version: Option<u32>,
}

// ---------------------------------------------------------------------------
// Error-resilience gate artifacts (constraint schema + error coverage).
//
// These types mirror the runtime copies in `ls_core::preflight` — the shared
// per-TR YAML (`metadata/constraints/<tr>.yaml`) is the contract between them.
// ls-core cannot depend on ls-metadata at runtime (it ships to consumers), so
// the type is deliberately duplicated; this copy is the authority for offline
// grounding, validation, and docgen projection.
// ---------------------------------------------------------------------------

/// The declared wire type of a request field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    String,
    Integer,
    Number,
}

/// Allowed-enum input class. `applicable: false` is the explicit N/A marker (R5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumRule {
    pub applicable: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    #[serde(default)]
    pub confirmed: bool,
}

/// Out-of-range input class (inclusive bounds).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RangeRule {
    pub applicable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    #[serde(default)]
    pub confirmed: bool,
}

/// A recognised value format for the malformed-symbol/date class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatKind {
    Symbol,
    Date,
}

/// Malformed-format input class. `applicable: false` is the explicit N/A marker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormatRule {
    pub applicable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<FormatKind>,
    #[serde(default)]
    pub confirmed: bool,
}

/// One request field's constraints across every input class. `enum`/`range`/
/// `format` are non-optional so an inapplicable class must be explicitly marked
/// N/A — exhaustiveness is auditable, not inferred from silence (R5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldConstraint {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    pub required: bool,
    #[serde(rename = "enum")]
    pub enum_rule: EnumRule,
    pub range: RangeRule,
    pub format: FormatRule,
}

/// A cross-field / combination-invalidity rule (R7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CrossFieldRule {
    DateOrder {
        start: String,
        end: String,
        #[serde(default)]
        confirmed: bool,
    },
}

/// A per-TR declarative request-field constraint schema
/// (`metadata/constraints/<tr>.yaml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstraintSchema {
    pub tr_code: String,
    pub fields: Vec<FieldConstraint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cross_field: Vec<CrossFieldRule>,
}

/// One entry in the shared gateway error-explanation catalog
/// (`metadata/error-catalog.yaml`). `ls-core` embeds the same file for runtime
/// `explain`; docgen projects these onto the Reference page (R8/R9/R11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    /// Coarse classification (`success`, `paper_incompatible`, `session_closed`,
    /// `request_shape`, `gateway_error`, ...).
    pub kind: String,
    /// The user-facing explanation.
    pub explanation: String,
}

/// The parsed shared error catalog: a `version` plus a code → entry map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorCatalog {
    pub version: u32,
    #[serde(default)]
    pub codes: std::collections::BTreeMap<String, CatalogEntry>,
}

/// Outcome of the differential negative probe for a TR (R10). Offline artifacts
/// are `NotProbed`; the operator probe records the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeStatus {
    /// No live probe has run yet (offline-authored coverage).
    NotProbed,
    /// Valid control succeeded and every invalid variant failed distinctly.
    Clean,
    /// Inconclusive — valid control failed (session-closed / unfunded / stale
    /// seed / paper-incompatible). Distinct from a divergence.
    Held,
    /// The declared bound diverged from observed behavior — promotion is blocked.
    Divergent,
}

/// Per-(field, class) probe outcome recorded in error coverage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassCoverage {
    /// The request field (or `"<cross-field>"` for a combination rule).
    pub field: String,
    /// The input class: `type`, `required`, `enum`, `range`, `format`, or
    /// `cross_field`.
    pub class: String,
    /// The per-class outcome: `declared` (offline, unprobed), `confirmed`,
    /// `held`, `divergent`, or `n_a`.
    pub status: String,
}

/// The per-TR error-coverage evidence artifact
/// (`metadata/error-coverage/<tr>.yaml`, R10/R11). Records the differential probe
/// outcome, the per-class coverage map, and the reachable gateway codes the
/// Reference page explains from the shared catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorCoverage {
    pub tr_code: String,
    #[serde(default = "default_probe_status")]
    pub probe_status: ProbeStatus,
    /// The maintained valid-seed fixture the probe's control uses, when declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
    #[serde(default)]
    pub input_classes: Vec<ClassCoverage>,
    /// Reachable gateway codes for this TR (a subset of `error-catalog.yaml`).
    #[serde(default)]
    pub gateway_codes: Vec<String>,
}

fn default_probe_status() -> ProbeStatus {
    ProbeStatus::NotProbed
}

/// The full per-TR maintenance metadata record (`metadata/trs/<tr>.yaml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrMetadata {
    pub tr_code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub owner_class: OwnerClass,
    pub facets: Facets,
    #[serde(default)]
    pub dependencies: Dependencies,
    pub support: Support,
    pub maintenance: Maintenance,
    /// Present iff the TR is recommended (validator-enforced). Carries the
    /// user-facing recommendation contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<Recommendation>,
    /// Path (relative to the metadata root) of the TR's request field-constraint
    /// schema (`constraints/<tr>.yaml`), when authored. Drives preflight, the
    /// negative probe, and the Reference "Errors & validation" section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints_ref: Option<String>,
    /// Path (relative to the metadata root) of the TR's error-coverage evidence
    /// (`error-coverage/<tr>.yaml`), when authored. Required for a `recommended`
    /// TR (error-resilience gate R1) — enforced by the validator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_coverage_ref: Option<String>,
}

/// One routing entry in `tr-index.yaml`. Duplicates only selector fields used
/// for fast routing; the per-TR file remains the full source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexEntry {
    pub file: String,
    pub owner_class: OwnerClass,
    pub protocol: Protocol,
    pub instrument_domain: InstrumentDomain,
    pub venue_session: VenueSession,
}

/// The routing index (`metadata/tr-index.yaml`): a `version` plus a map of TR
/// code to its routing entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrIndex {
    pub version: u32,
    #[serde(default)]
    pub trs: std::collections::BTreeMap<String, IndexEntry>,
}
