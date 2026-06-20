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
