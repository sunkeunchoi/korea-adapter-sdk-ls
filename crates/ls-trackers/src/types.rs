//! Core change-tracking types, using `CONTEXT.md` vocabulary.
//!
//! [`StagedSnapshot`] is a captured upstream artifact; [`NormalizedArtifact`] is
//! its canonical projection; [`Change`] is one structural difference; [`Severity`]
//! is the Support-Aware Severity tier; and [`TrackerFinding`] pairs a change with
//! its classified severity. These define the shared contract both Change Trackers
//! (the API Drift Tracker and, later, the Specification Document Tracker) speak.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

/// A captured upstream LS API artifact, tagged with the TR it belongs to.
///
/// LS response payloads are keyed by block names (e.g. `CSPAQ12200OutBlock1`)
/// and carry no TR code, so `tr_code` is an **explicit** snapshot field rather
/// than something derived from block-name prefixes. fetch is stubbed this round
/// (R12), so snapshots are placed by hand as fixtures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StagedSnapshot {
    pub tr_code: String,
    pub payload: serde_json::Value,
}

/// The canonical leaf shape recorded for each field path — enough to flag an
/// incompatible type change without storing volatile sample values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldShape {
    Null,
    Bool,
    Number,
    String,
    Array,
    Object,
}

impl fmt::Display for FieldShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            FieldShape::Null => "null",
            FieldShape::Bool => "bool",
            FieldShape::Number => "number",
            FieldShape::String => "string",
            FieldShape::Array => "array",
            FieldShape::Object => "object",
        };
        f.write_str(s)
    }
}

/// A [`StagedSnapshot`] reduced to a canonical, sorted map of field path → leaf
/// shape. Array indices are collapsed to `[]`, so reordering within a list is
/// **not** a change — only field additions, removals, and type changes are
/// (resolving the U6 open question: some LS list orderings are semantically
/// meaningful, but the structural diff this round watches shape, not order).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedArtifact {
    pub tr_code: String,
    pub fields: BTreeMap<String, FieldShape>,
}

/// A single structural difference between two normalized artifacts.
///
/// Each variant carries the `tr_code` propagated from the snapshot — the lookup
/// key [`classify`](crate::stages::classify) uses, since the payload itself has
/// no TR code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// A field present in the candidate but not the baseline.
    FieldAdded { tr_code: String, path: String },
    /// A field present in the baseline but not the candidate.
    FieldRemoved { tr_code: String, path: String },
    /// A field present in both but with an incompatible leaf shape.
    FieldChanged {
        tr_code: String,
        path: String,
        from: FieldShape,
        to: FieldShape,
    },
}

impl Change {
    /// The TR this change belongs to.
    pub fn tr_code(&self) -> &str {
        match self {
            Change::FieldAdded { tr_code, .. }
            | Change::FieldRemoved { tr_code, .. }
            | Change::FieldChanged { tr_code, .. } => tr_code,
        }
    }

    /// The field path this change concerns.
    pub fn path(&self) -> &str {
        match self {
            Change::FieldAdded { path, .. }
            | Change::FieldRemoved { path, .. }
            | Change::FieldChanged { path, .. } => path,
        }
    }
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Change::FieldAdded { path, .. } => write!(f, "added field `{path}`"),
            Change::FieldRemoved { path, .. } => write!(f, "removed field `{path}`"),
            Change::FieldChanged { path, from, to, .. } => {
                write!(f, "field `{path}` changed shape {from} → {to}")
            }
        }
    }
}

/// Support-Aware Severity — the classification of a [`TrackerFinding`] (R11).
///
/// Variants are declared in **ascending** severity, so the derived `Ord` makes
/// `Breaking > Informational` etc. true as comparisons; findings are presented
/// highest-first by sorting in reverse. The migration plan's ladder is
/// critical / breaking / maintenance / evidence / informational (most to least
/// severe); this enum is that ladder reversed for `Ord`.
///
/// This round's fixtures reach only `Informational` / `Maintenance` / `Breaking`.
/// `Evidence` (stale focused evidence on a Recommended TR) and `Critical`
/// (auth/order-safety changes) are defined but unreachable here — no TR is
/// recommended and change-driven evidence invalidation is inactive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Informational,
    Evidence,
    Maintenance,
    Breaking,
    Critical,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Severity::Informational => "informational",
            Severity::Evidence => "evidence",
            Severity::Maintenance => "maintenance",
            Severity::Breaking => "breaking",
            Severity::Critical => "critical",
        };
        f.write_str(s)
    }
}

/// A severity-classified observation emitted by a Change Tracker before it
/// becomes SDK work. Advisory only — nothing auto-promotes it (R15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerFinding {
    pub tr_code: String,
    pub change: Change,
    pub severity: Severity,
}

impl fmt::Display for TrackerFinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}: {}", self.severity, self.tr_code, self.change)
    }
}

/// What a real `promote` would touch, enumerated by the dry-run (R13). The
/// dry-run writes nothing; this report is the contract a future mutating
/// promote must satisfy.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PromoteReport {
    /// Reviewed baseline fixture files a real promote would rewrite.
    pub baseline_files: Vec<String>,
    /// Metadata fields a real promote would update (e.g. `source_spec_hash`).
    pub metadata_fields: Vec<String>,
    /// Generated docs a real promote would regenerate.
    pub generated_docs: Vec<String>,
}

// ---------------------------------------------------------------------------
// API Drift signal model (D1, D2 — the bounded-baseline + support-aware exit
// contract). These types supersede the PR #2 sample-payload leaf-path model for
// real API Drift work; `StagedSnapshot`/`NormalizedArtifact`/`Change`/
// `TrackerFinding` above remain as PR #2 compatibility coverage only.
// ---------------------------------------------------------------------------

/// The reviewed full-inventory **code-set** (R3b): every upstream TR code, plus
/// a `provisional` flag set on the bootstrap seed (KTD-5). It drives the new-TR
/// gate (R9b), the completeness anchor (R12), and the coverage summary (R11) —
/// none of which need per-TR structural shape. `BTreeSet` makes serialization
/// deterministic and membership tests cheap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeSet {
    /// All upstream TR codes, sorted and de-duplicated.
    pub codes: std::collections::BTreeSet<String>,
    /// `true` while the seed has not been independently attested as complete by
    /// an operator (KTD-5). Cleared once an operator clears it through normal
    /// maintenance; the seed is explicitly untrusted-as-complete until then.
    #[serde(default)]
    pub provisional: bool,
}

impl CodeSet {
    /// Build a code-set from an iterator of codes, sorting and de-duplicating.
    pub fn new(codes: impl IntoIterator<Item = String>, provisional: bool) -> Self {
        CodeSet {
            codes: codes.into_iter().collect(),
            provisional,
        }
    }

    /// Number of distinct codes — the completeness gate's denominator (R12).
    pub fn len(&self) -> usize {
        self.codes.len()
    }

    /// Whether the code-set is empty (bootstrap: no committed code-set).
    pub fn is_empty(&self) -> bool {
        self.codes.is_empty()
    }

    /// Whether `code` is a known, reviewed upstream TR.
    pub fn contains(&self, code: &str) -> bool {
        self.codes.contains(code)
    }
}

/// The maintained-SDK support state of a TR, projected from `ls_metadata::Support`
/// for the exit gate (R17b). A TR with no metadata at all is [`Untracked`].
///
/// Projection precedence mirrors the Severity Policy's three tiers:
/// implemented/recommended ("strong") outrank tracked-only, which outranks
/// untracked. `implemented` is checked before `recommended` only to pick a
/// single label — both land in the same gating tier.
///
/// [`Untracked`]: SupportState::Untracked
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportState {
    Implemented,
    Recommended,
    Tracked,
    Untracked,
}

impl SupportState {
    /// Project an `ls_metadata::Support` record into a gating tier. Use
    /// [`SupportState::Untracked`] directly when no metadata exists for a TR.
    pub fn from_support(support: &ls_metadata::Support) -> Self {
        if support.implemented {
            SupportState::Implemented
        } else if support.recommended {
            SupportState::Recommended
        } else if support.tracked {
            SupportState::Tracked
        } else {
            SupportState::Untracked
        }
    }

    /// Whether this TR is part of the maintained SDK surface (anything but
    /// [`Untracked`]). Untracked TRs never gate on their own changes (R9, R13);
    /// only their *discovery* gates, via the `is_new_tr` arm of [`gates_for`].
    pub fn is_maintained(self) -> bool {
        !matches!(self, SupportState::Untracked)
    }
}

impl fmt::Display for SupportState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SupportState::Implemented => "implemented",
            SupportState::Recommended => "recommended",
            SupportState::Tracked => "tracked",
            SupportState::Untracked => "untracked",
        };
        f.write_str(s)
    }
}

/// The **single source** of the R17b exit-gate rule (KTD-1): a finding crosses
/// the exit-`1` threshold iff it touches a tracked/implemented/recommended TR at
/// **maintenance or higher**, OR it is a new untracked-TR discovery (R9b).
///
/// All other untracked-TR changes and **all** informational findings are
/// reported but do not gate. `Evidence` (below `Maintenance`) does not gate and
/// is unreachable this slice. This helper is pure so the matrix is unit-testable
/// here and reused verbatim at classify time in U4 — there is no second copy of
/// the rule.
pub fn gates_for(severity: Severity, support_state: SupportState, is_new_tr: bool) -> bool {
    is_new_tr || (support_state.is_maintained() && severity >= Severity::Maintenance)
}

/// Field traversal direction — part of field identity (R6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Request,
    Response,
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Direction::Request => "request",
            Direction::Response => "response",
        })
    }
}

/// One normalized field within a request/response block (R6, R8). Field identity
/// is `(direction, block_name, field_index, field_name)`; `description_hash` is
/// the stable hash of the normalized long description so benign re-encoding does
/// not register as drift.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockField {
    pub direction: Direction,
    pub block_name: String,
    pub field_index: u32,
    pub field_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub korean_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u32>,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_hash: Option<String>,
}

/// The committed per-TR **Structural API Shape** (maintained TRs only, R5). Long
/// descriptions/examples are stored as `description_hash`; compact names are
/// preserved verbatim (R8). Endpoint/protocol/rate facts are top-level (R7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrShape {
    pub tr_code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tr_name: Option<String>,
    pub protocol: Protocol,
    pub is_websocket: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_group_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_group_name: Option<String>,
    /// Request-direction blocks, ordered as upstream presents them. `field_index`
    /// preserves position so U4's reorder reconciliation can run.
    #[serde(default)]
    pub request_blocks: Vec<BlockField>,
    /// Response-direction blocks, ordered as upstream presents them.
    #[serde(default)]
    pub response_blocks: Vec<BlockField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit_per_sec: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corp_rate_limit_per_sec: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_source_group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_hash: Option<String>,
}

/// Re-exported here so callers of the trackers crate get the protocol vocabulary
/// without depending on `ls-metadata` directly.
pub use ls_metadata::Protocol;

/// Inventory facts for a normalized snapshot (R8): code-set size, source URLs,
/// the normalizer version (so a normalizer change is auditable), and per-TR
/// description hashes for the maintained shapes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Total upstream TR count recorded for this snapshot (full inventory).
    pub upstream_tr_count: usize,
    /// Number of maintained TRs normalized into committed shapes.
    pub maintained_tr_count: usize,
    /// Source URLs the snapshot was scraped from (menu + detail endpoints).
    #[serde(default)]
    pub source_urls: Vec<String>,
    /// The normalizer version that produced the committed shapes.
    pub normalizer_version: u32,
}

/// The machine-readable outcome of a fetch attempt (`fetch-report.json`, AE1).
/// On the failure path the gate records why exit `2` fired without a partial
/// staged run masquerading as complete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchReport {
    /// `true` when the scrape parsed and passed the split completeness gate.
    pub ok: bool,
    /// Distinct upstream codes discovered this run.
    pub fetched_count: usize,
    /// The committed code-set size compared against, when one exists (R12).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_code_set_len: Option<usize>,
    /// Groups whose protocol/rate facts could not be fetched (best-effort
    /// endpoint failed). A non-zero count warns that endpoint/rate facts are
    /// degraded — a wholesale outage would otherwise stage `ok: true` and then
    /// surface as spurious endpoint/rate drift at compare time.
    #[serde(default)]
    pub facts_degraded_groups: usize,
    /// TR codes that live under a facts-degraded group (endpoint/rate facts
    /// missing, so the group's id is `None`). The facts-outage gate (U5) joins
    /// these against the maintained set by **code**, since the group UUID is the
    /// field that went missing (KTD-4a).
    #[serde(default)]
    pub degraded_tr_codes: BTreeSet<String>,
    /// `true` when the whole-inventory property-type mapping served the hardcoded
    /// fallback (U4, KTD-5). Unlike endpoint/rate degradation this has no
    /// per-group granularity — it substitutes raw type codes for every TR — so
    /// the gate treats it as whole-inventory.
    #[serde(default)]
    pub property_type_fallback_served: bool,
    /// Set when `ok` is false: the gate that fired (parse failure or truncation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
}

/// Metadata-coverage reporting (R11), driven by the code-set, never structural
/// shape. Coverage summaries are informational — they never affect exit codes.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CoverageSummary {
    /// Distinct upstream TR codes in the (staged) inventory.
    pub upstream_count: usize,
    /// TRs with a metadata file under `metadata/trs/`.
    pub metadata_count: usize,
    /// Metadata TRs whose `support.implemented` is true.
    pub implemented_count: usize,
    /// Metadata TRs that are tracked but not implemented/recommended.
    pub tracked_only_count: usize,
    /// Metadata codes with no matching upstream code (possible removals).
    #[serde(default)]
    pub metadata_missing_upstream: Vec<String>,
    /// Upstream codes with no matching metadata (coverage gaps).
    #[serde(default)]
    pub upstream_missing_metadata: Vec<String>,
}

/// One structural change in the API Drift signal model. `tr_code` lives on the
/// [`DriftFinding`] that wraps a change, so variants carry only the change's own
/// locating fields. Field-level variants use the `(direction, block, index,
/// name)` identity from R6.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DriftChange {
    /// A TR code newly present upstream (gating is decided by `is_new_tr`, not
    /// the variant: a re-appearing known TR is not a discovery).
    TrAdded,
    /// A TR code absent from an otherwise well-parsed inventory (R12).
    TrRemoved,
    FieldAdded {
        direction: Direction,
        block_name: String,
        field_index: u32,
        field_name: String,
    },
    FieldRemoved {
        direction: Direction,
        block_name: String,
        field_index: u32,
        field_name: String,
    },
    /// Same field name, same block, shifted index — emitted only via U4's
    /// reorder reconciliation pass (R6).
    FieldReordered {
        direction: Direction,
        block_name: String,
        field_name: String,
        from_index: u32,
        to_index: u32,
    },
    /// Same field name moved to a different block.
    FieldMovedAcrossBlock {
        direction: Direction,
        field_name: String,
        from_block: String,
        to_block: String,
    },
    /// Type / length / required-flag change on a field present on both sides.
    FieldChanged {
        direction: Direction,
        block_name: String,
        field_index: u32,
        field_name: String,
        detail: String,
    },
    EndpointChanged {
        from: Option<String>,
        to: Option<String>,
    },
    ProtocolChanged {
        from: String,
        to: String,
    },
    RateLimitChanged {
        from: Option<u32>,
        to: Option<u32>,
    },
    /// A long description changed but nothing structural did (R13) — always
    /// informational, never gating.
    DescriptionChanged {
        location: String,
    },
    /// A whole-inventory facts dependency degraded (endpoint/rate facts or the
    /// property-type mapping) but only for **untracked** inventory (U5, R3).
    /// Emitted as a visible, non-gating finding at exit `0`; a degradation that
    /// touches a maintained TR is an exit-`2` error instead, not a finding.
    FactsDegraded {
        detail: String,
    },
}

/// A support-aware API Drift finding (R17b). Unlike the PR #2 [`TrackerFinding`],
/// it carries the projected `support_state` and a **stored** `gates` flag set at
/// classify time by [`gates_for`] — the exit-code mapping (U5) reads the
/// aggregate of these flags, never re-deriving the rule. `possible_rename` is the
/// minimal R14b hook: a co-occurring add/remove pair is annotated, with no
/// matching logic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriftFinding {
    pub tr_code: String,
    pub change: DriftChange,
    pub severity: Severity,
    pub support_state: SupportState,
    /// Whether this finding's TR was newly discovered this run (R9b).
    #[serde(default)]
    pub is_new_tr: bool,
    /// The stored exit-gate decision (R17b), set once at classify time.
    pub gates: bool,
    /// R14b adjacency note: the co-occurring counterpart code, when this finding
    /// is one half of an add/remove pair in the same run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub possible_rename: Option<String>,
}

// ---------------------------------------------------------------------------
// Specification Document Tracker — example projection (U1). The example facet is
// net-new: the API Drift normalizer never reads `req_example`/`res_example`.
// These persisted types live in the `spec-doc` baseline tree under their own
// `EXAMPLE_NORMALIZER_VERSION` (KTD2), with `#[serde(default)]` on optional
// fields from day one to heed the carried R-4 serde forward-compat residual
// (KTD6).
// ---------------------------------------------------------------------------

/// One TR's normalized request/response example projection (R2), stored in the
/// `spec-doc` baseline keyed by `tr_code`.
///
/// By construction it carries only structural descriptors — field-path → leaf
/// [`FieldShape`] for JSON, the key set for form-encoded, and nothing at all for
/// opaque/absent — never a raw example string or scalar value. The real-looking
/// credentials embedded in the `token`/`revoke`/`S3_` examples therefore can
/// never reach a committed baseline (KTD7): the type makes the unsafe write
/// unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExampleShape {
    pub tr_code: String,
    #[serde(default)]
    pub req: ExampleFacet,
    #[serde(default)]
    pub res: ExampleFacet,
}

impl ExampleShape {
    /// An all-[`ExampleFacet::Absent`] shape for `tr_code` — the synthetic "other
    /// side" when a TR's example appears or disappears across a comparison, so the
    /// per-facet diff handles add/remove without a special case.
    pub fn absent(tr_code: &str) -> Self {
        ExampleShape {
            tr_code: tr_code.to_string(),
            req: ExampleFacet::Absent,
            res: ExampleFacet::Absent,
        }
    }

    /// Whether neither direction carries an example (so the TR contributes no
    /// shape to the baseline).
    pub fn is_absent(&self) -> bool {
        self.req.is_absent() && self.res.is_absent()
    }
}

/// One direction's normalized example, projected per payload class (KTD3):
///
/// * [`Json`](ExampleFacet::Json) — a JSON-parseable example reduced to the same
///   field-path → leaf [`FieldShape`] map the API Drift leaf walker produces,
///   discarding scalar sample values so value churn is not drift (R2, AE4).
/// * [`Form`](ExampleFacet::Form) — a form-encoded example reduced to its key
///   set, discarding values so a secret-only change is not drift (R2, AE5).
/// * [`Opaque`](ExampleFacet::Opaque) — present but non-parseable; carries no
///   structure, compared only by class, never shape-diffed (R2, R9).
/// * [`Absent`](ExampleFacet::Absent) — no example in this direction.
///
/// `Opaque` and `Absent` carry no payload, so a non-parseable example's raw text
/// (which may embed credentials, e.g. the untracked `UBM` request JWT) never
/// lands in a committed baseline (KTD7).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExampleFacet {
    #[default]
    Absent,
    Json {
        #[serde(default)]
        shape: BTreeMap<String, FieldShape>,
    },
    Form {
        #[serde(default)]
        keys: BTreeSet<String>,
    },
    Opaque,
}

impl ExampleFacet {
    /// Whether this direction carries no example at all.
    pub fn is_absent(&self) -> bool {
        matches!(self, ExampleFacet::Absent)
    }
}

/// A single leaf-shape change at one field path within a JSON example, carrying
/// only the path name and the structural [`FieldShape`] on each side — never the
/// scalar sample value that changed (KTD7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShapePathChange {
    pub path: String,
    pub from: FieldShape,
    pub to: FieldShape,
}

/// One example-drift change in the Specification Document Tracker (U2). `tr_code`
/// lives on the wrapping [`SpecFinding`], so variants carry only the change's own
/// locating fields.
///
/// Every variant carries **only structural descriptors** — the request/response
/// [`Direction`], field-path names, key names, and [`FieldShape`]s — never a raw
/// example string or scalar value (KTD7), following the API Drift
/// [`DescriptionChanged`](DriftChange::DescriptionChanged) advisory template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpecChange {
    /// A direction gained an example where the baseline had none.
    ExampleAdded { direction: Direction },
    /// A direction lost the example the baseline carried.
    ExampleRemoved { direction: Direction },
    /// A JSON example's leaf-path shape set changed (R2). Pure scalar-value churn
    /// produces none of these (AE4) — only added/removed paths and leaf-shape
    /// changes qualify.
    ExampleShapeChanged {
        direction: Direction,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        added_paths: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        removed_paths: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        changed_paths: Vec<ShapePathChange>,
    },
    /// A form-encoded example's key set changed (R2, AE5). A secret-only value
    /// rotation produces none of these — only key add/remove qualifies.
    ExampleKeySetChanged {
        direction: Direction,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        added_keys: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        removed_keys: Vec<String>,
    },
    /// An example could not be structurally compared this run — it is opaque on
    /// one side, or transitioned across payload classes (R9, AE5). Always
    /// informational, never gating; carries no structure.
    ExampleUnparseable { direction: Direction },
}

/// A support-aware Specification Document Tracker finding (U2). Mirrors
/// [`DriftFinding`] but is **always advisory**: `gates` is `false` for every
/// example finding by construction (KTD4), so [`SpecReport::gates`] is never true.
/// `pointers` (U3) names the maintained SDK artifacts a Tracked TR's change should
/// prompt review of; it is empty for an untracked TR or a TR with no registered
/// artifact (R7).
///
/// [`SpecReport::gates`]: crate::spec_doc::SpecReport::gates
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecFinding {
    pub tr_code: String,
    pub change: SpecChange,
    pub severity: Severity,
    pub support_state: SupportState,
    /// Always `false`: example changes are advisory and never gate (KTD4).
    pub gates: bool,
    /// Maintained SDK artifacts to review for this change (U3); empty →
    /// informational-only, no pointer (R7).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pointers: Vec<ArtifactRef>,
}

/// A pointer to a maintained SDK artifact a changed TR references (R4, R5). The
/// first version covers only naming-convention-derivable docs (KTD5): the
/// per-TR Reference Doc (Implemented TRs only) and TR Dependency Doc (all Tracked
/// TRs). SDK-example and Focused-Evidence artifacts are deferred.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub kind: ArtifactKind,
    pub path: String,
}

/// Which maintained artifact an [`ArtifactRef`] points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// `docs/reference/{tr}.md` — only for Implemented TRs.
    ReferenceDoc,
    /// `docs/tr-dependencies/{tr}.md` — for every Tracked TR.
    DependencyDoc,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The derived `Ord` ranks the tiers by severity: breaking outranks
    /// informational, critical is the top.
    #[test]
    fn severity_orders_by_tier() {
        assert!(Severity::Breaking > Severity::Informational);
        assert!(Severity::Maintenance > Severity::Informational);
        assert!(Severity::Critical > Severity::Breaking);
        assert!(Severity::Maintenance > Severity::Evidence);

        let mut tiers = [
            Severity::Breaking,
            Severity::Informational,
            Severity::Critical,
            Severity::Maintenance,
        ];
        tiers.sort();
        assert_eq!(
            tiers,
            [
                Severity::Informational,
                Severity::Maintenance,
                Severity::Breaking,
                Severity::Critical,
            ]
        );
    }

    #[test]
    fn change_accessors_expose_tr_and_path() {
        let c = Change::FieldRemoved {
            tr_code: "t8412".to_string(),
            path: "t8412OutBlock.shcode".to_string(),
        };
        assert_eq!(c.tr_code(), "t8412");
        assert_eq!(c.path(), "t8412OutBlock.shcode");
        assert!(c.to_string().contains("removed field"));
    }

    // --- API Drift signal model (U1) ---------------------------------------

    fn support(tracked: bool, implemented: bool, recommended: bool) -> ls_metadata::Support {
        ls_metadata::Support {
            tracked,
            implemented,
            recommended,
        }
    }

    /// `SupportState` projects strong (impl/rec) above tracked-only above
    /// untracked, and a fully-false `Support` collapses to `Untracked`.
    #[test]
    fn support_state_projection_precedence() {
        assert_eq!(
            SupportState::from_support(&support(true, true, false)),
            SupportState::Implemented
        );
        assert_eq!(
            SupportState::from_support(&support(true, false, true)),
            SupportState::Recommended
        );
        assert_eq!(
            SupportState::from_support(&support(true, false, false)),
            SupportState::Tracked
        );
        assert_eq!(
            SupportState::from_support(&support(false, false, false)),
            SupportState::Untracked
        );
        assert!(SupportState::Implemented.is_maintained());
        assert!(!SupportState::Untracked.is_maintained());
    }

    /// The `gates_for` derivation matrix — every row of the Severity Policy
    /// table, expressed as (severity × support_state × is_new_tr) → gates.
    #[test]
    fn gates_for_covers_the_severity_policy_matrix() {
        use Severity::*;
        use SupportState::*;

        // New-TR discovery always gates, regardless of severity/state (R9b).
        assert!(gates_for(Maintenance, Untracked, true));
        assert!(gates_for(Informational, Untracked, true));

        // Maintained TR at >= maintenance gates; below maintenance does not.
        assert!(gates_for(Maintenance, Tracked, false)); // TR removed / reorder, tracked-only
        assert!(gates_for(Breaking, Implemented, false)); // field removed, implemented
        assert!(gates_for(Breaking, Recommended, false)); // endpoint changed, recommended
        assert!(gates_for(Maintenance, Implemented, false)); // rate limit decreased
        assert!(!gates_for(Informational, Implemented, false)); // description-only, impl
        assert!(!gates_for(Informational, Tracked, false)); // rate-limit informational, tracked
        assert!(!gates_for(Evidence, Implemented, false)); // sub-maintenance never gates

        // Untracked (known TR) never gates on a change — only on discovery.
        assert!(!gates_for(Maintenance, Untracked, false)); // TR removed, no metadata
        assert!(!gates_for(Informational, Untracked, false)); // shape changed, no metadata
    }

    /// A code-set round-trips deterministically with the `provisional` seed flag,
    /// sorting and de-duplicating input order.
    #[test]
    fn code_set_round_trips_deterministically() {
        let a = CodeSet::new(
            ["t8412", "token", "t1102", "t8412"]
                .into_iter()
                .map(String::from),
            true,
        );
        assert_eq!(a.len(), 3, "duplicates collapse");
        assert!(a.provisional);

        let bytes_a = serde_json::to_vec(&a).unwrap();
        let bytes_b = serde_json::to_vec(&a).unwrap();
        assert_eq!(bytes_a, bytes_b, "serialization is byte-stable across runs");

        let back: CodeSet = serde_json::from_slice(&bytes_a).unwrap();
        assert_eq!(back, a, "code-set round-trips");
        // Sorted output: t1102 precedes t8412 precedes token.
        let json = String::from_utf8(bytes_a).unwrap();
        let i1102 = json.find("t1102").unwrap();
        let i8412 = json.find("t8412").unwrap();
        let itoken = json.find("token").unwrap();
        assert!(i1102 < i8412 && i8412 < itoken, "codes serialize sorted");
    }

    /// A normalized TR shape and a manifest serialize to stable bytes across
    /// repeated serialization (deterministic field/Vec order).
    #[test]
    fn tr_shape_and_manifest_serialize_deterministically() {
        let shape = TrShape {
            tr_code: "t8412".to_string(),
            tr_name: Some("주식차트(N분)".to_string()),
            protocol: Protocol::Rest,
            is_websocket: false,
            endpoint_path: Some("/stock/chart".to_string()),
            api_group_id: Some("g1".to_string()),
            source_group_name: Some("주식시세".to_string()),
            request_blocks: vec![BlockField {
                direction: Direction::Request,
                block_name: "t8412InBlock".to_string(),
                field_index: 0,
                field_name: "shcode".to_string(),
                korean_name: Some("종목코드".to_string()),
                r#type: Some("String".to_string()),
                length: Some(6),
                required: true,
                description_hash: Some("abc123".to_string()),
            }],
            response_blocks: vec![],
            rate_limit_per_sec: Some(1),
            corp_rate_limit_per_sec: None,
            rate_source_group: Some("g1".to_string()),
            description_hash: Some("def456".to_string()),
        };
        let a = serde_json::to_vec(&shape).unwrap();
        let b = serde_json::to_vec(&shape).unwrap();
        assert_eq!(a, b, "TR shape serialization is byte-stable");
        let back: TrShape = serde_json::from_slice(&a).unwrap();
        assert_eq!(back, shape, "TR shape round-trips");

        let manifest = Manifest {
            upstream_tr_count: 365,
            maintained_tr_count: 7,
            source_urls: vec!["https://openapi.ls-sec.co.kr/apiservice".to_string()],
            normalizer_version: 1,
        };
        let m1 = serde_json::to_vec(&manifest).unwrap();
        let m2 = serde_json::to_vec(&manifest).unwrap();
        assert_eq!(m1, m2, "manifest serialization is byte-stable");
    }

    /// A finding carries its support state and a stored `gates` flag consistent
    /// with `gates_for`, and round-trips with its change variant.
    #[test]
    fn drift_finding_round_trips_with_stored_gate() {
        let finding = DriftFinding {
            tr_code: "t1102".to_string(),
            change: DriftChange::FieldRemoved {
                direction: Direction::Response,
                block_name: "t1102OutBlock".to_string(),
                field_index: 3,
                field_name: "price".to_string(),
            },
            severity: Severity::Breaking,
            support_state: SupportState::Implemented,
            is_new_tr: false,
            gates: gates_for(Severity::Breaking, SupportState::Implemented, false),
            possible_rename: None,
        };
        assert!(finding.gates, "breaking on implemented gates");
        let bytes = serde_json::to_vec(&finding).unwrap();
        let back: DriftFinding = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back, finding, "finding round-trips");
    }
}
