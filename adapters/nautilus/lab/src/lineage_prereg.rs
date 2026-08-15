//! The **successor daily lineage's** pre-registration (plan 2026-08-14-001, P6 of the
//! 2026-08-10 scoping ladder) — the frozen terms a strategy lineage's search must hold
//! to, written down *before* its first turn.
//!
//! # Not the ladder pre-registration
//!
//! [`crate::dispatch::prereg`] freezes the **production ladder's** rung dosing and
//! expectation bands (`config/preregistration.json`). This module freezes a **strategy
//! lineage's search terms** (`config/lineage-preregistration.json`): the session ceiling
//! and its specification / holdout / reserved split, the search budget, the hypothesized
//! effect size, the verdict predicate, and the upgrade schedule. The two artifacts share
//! the word "pre-registration" and nothing else (KTD1); the ladder pair stays
//! byte-identical.
//!
//! # Three consumers, three rights
//!
//! - [`derive_split`] reproduces the supply split from calendar facts. It is the only
//!   part that needs a calendar, and it runs at freeze time (an operator step) and again
//!   whenever the re-derivation trigger fires — never in the verdict path.
//! - [`load`] parses the frozen artifact and emits the SHA-256 citation over its exact
//!   bytes, so a silent edit cannot masquerade under an old citation (R14).
//! - [`judge_holdout`] resolves the verdict — and **claims an attempt in the ledger
//!   before it answers** (KTD10), so declining to write a result back does not buy a
//!   second look.
//!
//! # Why the deriver stays after the freeze
//!
//! The plan asks this to be decided explicitly. It stays. The frozen artifact carries a
//! `rederivation_trigger` (R15); when it fires, the split must be re-derived from the
//! calendar, and a deleted deriver means re-authoring the arithmetic that KTD4's
//! reconciliation rests on. [`count_proven_sessions`] is also the helper that attributes
//! the origin-vs-walk ceiling delta to its date range, which is the evidence for the
//! three-session gap being a range mismatch rather than a disagreement.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::artifacts::manifest::hash_bytes;
use crate::dispatch::checks::CalendarDateFact;

// ===========================================================================
// U1 — supply split derivation
// ===========================================================================

/// The three named partitions of the lineage's session supply (R1).
///
/// The split is **one-time and disjoint**: the specification window is where the strategy
/// is specified, the holdout is spent by exactly one judgment, and the reserved tail is
/// quarantined for the forward-accrual clock the upgrade schedule runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Partition {
    /// Where the strategy specification is authored. Freely observable.
    Specification,
    /// Spent by exactly one judgment (`N_max = 1`). Never observed before that judgment.
    Holdout,
    /// Quarantined forward accrual for the upgrade schedule.
    Reserved,
}

impl Partition {
    /// The partition's stable name (matches the artifact's key).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Partition::Specification => "specification",
            Partition::Holdout => "holdout",
            Partition::Reserved => "reserved",
        }
    }
}

/// A typed split-derivation failure. Every variant is a refusal to produce a count that
/// would be wrong, never a silently-adjusted answer.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SplitError {
    /// A partition's `from` anchor is not a proven trading session. A partition that
    /// starts on a closed or unknown day has an ambiguous anchor — the operator must
    /// state the actual first session rather than let the deriver round to a neighbour.
    #[error("{partition} window starts at {date}, which is not a proven trading session ({fact:?}) — state the first session explicitly rather than rounding to a neighbour")]
    BoundaryNotASession {
        /// The partition whose anchor is bad.
        partition: &'static str,
        /// The offending anchor date.
        date: NaiveDate,
        /// What the calendar actually said about it.
        fact: CalendarDateFact,
    },
    /// The ceiling anchor (the split's last day) is not a proven trading session.
    #[error("ceiling anchor {date} is not a proven trading session ({fact:?})")]
    CeilingNotASession {
        /// The offending ceiling date.
        date: NaiveDate,
        /// What the calendar actually said about it.
        fact: CalendarDateFact,
    },
    /// The anchors are not in strictly increasing order, or a partition would be empty.
    #[error("split anchors are not strictly increasing: {detail}")]
    AnchorsOutOfOrder {
        /// Which ordering rule was violated.
        detail: String,
    },
    /// A proven trading session falls in the gap between one partition's `to` and the
    /// next partition's `from` — the split would silently drop it.
    #[error("proven session {date} falls between the {left} window's end and the {right} window's start — the split would drop it")]
    SessionInGap {
        /// The dropped session.
        date: NaiveDate,
        /// The partition on the left of the gap.
        left: &'static str,
        /// The partition on the right of the gap.
        right: &'static str,
    },
    /// The calendar could not answer for a date inside the ceiling range. An
    /// out-of-coverage or evidentially-unknown day makes the count a lower bound, not a
    /// count — fail closed rather than freeze an understated ceiling.
    #[error("calendar cannot prove {date} open or closed ({fact:?}) — the ceiling count would be a lower bound, not a count")]
    UnprovenDay {
        /// The date the calendar could not answer for.
        date: NaiveDate,
        /// What the calendar actually said about it.
        fact: CalendarDateFact,
    },
}

/// The dated cut points that define the split (R1).
///
/// Four dates, six boundaries: each partition's `to` is the anchor named here and the
/// next partition's `from` is its own anchor, so the deriver never invents a boundary.
/// A `to` anchor **may** be a closed day (the specification window's `2019-12-31` is one)
/// — a closed day at a boundary contributes no session to either side, which the
/// [`SplitError::SessionInGap`] check makes explicit rather than assumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitAnchors {
    /// The supply floor — the specification window's first day. Must be a proven session.
    pub floor: NaiveDate,
    /// The specification window's last day (inclusive). May be a closed day.
    pub specification_to: NaiveDate,
    /// The holdout's first day. Must be a proven session.
    pub holdout_from: NaiveDate,
    /// The holdout's last day (inclusive). May be a closed day.
    pub holdout_to: NaiveDate,
    /// The reserved tail's first day. Must be a proven session.
    pub reserved_from: NaiveDate,
    /// The ceiling — the reserved tail's last day. Must be a proven session.
    pub ceiling: NaiveDate,
}

/// One derived partition: its declared range, the session-tight span inside it, and the
/// count.
///
/// `from`/`to` are the **declared** range (what the artifact freezes);
/// `first_session`/`last_session` are the **observed** session-tight span. They differ
/// exactly when a boundary anchor is a closed day, and recording both keeps that visible
/// instead of leaving a reader to wonder whether `2019-12-31` was a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitPart {
    /// Which partition this is.
    pub partition: Partition,
    /// The declared range's first day (inclusive).
    pub from: NaiveDate,
    /// The declared range's last day (inclusive).
    pub to: NaiveDate,
    /// The first proven session at or after `from`.
    pub first_session: NaiveDate,
    /// The last proven session at or before `to`.
    pub last_session: NaiveDate,
    /// Proven trading sessions in `from ..= to`.
    pub sessions: usize,
}

/// The derived split: the ceiling and its three disjoint, exhaustive partitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerivedSplit {
    /// The ceiling's first day (`anchors.floor`).
    pub from: NaiveDate,
    /// The ceiling's last day (`anchors.ceiling`).
    pub to: NaiveDate,
    /// `S_max` — proven sessions in `from ..= to`. Equals the three partition counts.
    pub s_max: usize,
    /// The specification window.
    pub specification: SplitPart,
    /// The holdout.
    pub holdout: SplitPart,
    /// The reserved tail.
    pub reserved: SplitPart,
}

impl DerivedSplit {
    /// The partition a date belongs to, or `None` when it falls outside every declared
    /// range. Disjoint by construction — a date can never answer twice — but **not total
    /// over the ceiling**: a date inside `from ..= to` may still land in the gap a closed
    /// boundary day opens between two partitions (the specification window's
    /// `2019-12-31 → 2020-01-02` seam). `derive_split` proves no *session* falls in such a
    /// gap, which is the property that matters; a non-session date there answers `None`.
    #[must_use]
    pub fn partition_of(&self, date: NaiveDate) -> Option<Partition> {
        for part in [&self.specification, &self.holdout, &self.reserved] {
            if date >= part.from && date <= part.to {
                return Some(part.partition);
            }
        }
        None
    }
}

/// Proven trading sessions in `from ..= to` — the delta-attribution helper.
///
/// This is what attributes the origin-vs-walk ceiling gap to its date range (KTD4): the
/// origin plan counted to `2026-08-07` and the P4 walk to its own `2026-08-12` anchor, so
/// the three-session delta is exactly this count over `2026-08-08 ..= 2026-08-12`.
///
/// Only [`CalendarDateFact::TradingSession`] counts. A day the calendar cannot prove open
/// or closed is a refusal, not a zero — an understated ceiling would set the margin bar
/// too high and quietly strand a viable lineage.
///
/// # Errors
///
/// [`SplitError::UnprovenDay`] when any day in range is `Unknown` or `Unavailable`, or
/// [`SplitError::AnchorsOutOfOrder`] when `to` precedes `from`.
pub fn count_proven_sessions<F>(from: NaiveDate, to: NaiveDate, fact_for: &F) -> Result<usize, SplitError>
where
    F: Fn(NaiveDate) -> CalendarDateFact,
{
    if to < from {
        return Err(SplitError::AnchorsOutOfOrder {
            detail: format!("range end {to} precedes range start {from}"),
        });
    }
    let mut sessions = 0usize;
    let mut date = from;
    loop {
        match fact_for(date) {
            CalendarDateFact::TradingSession => sessions += 1,
            CalendarDateFact::Closed => {}
            fact @ (CalendarDateFact::Unknown | CalendarDateFact::Unavailable) => {
                return Err(SplitError::UnprovenDay { date, fact });
            }
        }
        if date == to {
            break;
        }
        date = date.succ_opt().ok_or_else(|| SplitError::AnchorsOutOfOrder {
            detail: format!("ran past the calendar date limit walking {from}..={to}"),
        })?;
    }
    Ok(sessions)
}

/// The first proven session in `from ..= to`, if any.
fn first_session<F>(from: NaiveDate, to: NaiveDate, fact_for: &F) -> Option<NaiveDate>
where
    F: Fn(NaiveDate) -> CalendarDateFact,
{
    let mut date = from;
    loop {
        if fact_for(date) == CalendarDateFact::TradingSession {
            return Some(date);
        }
        if date == to {
            return None;
        }
        date = date.succ_opt()?;
    }
}

/// The last proven session in `from ..= to`, if any.
fn last_session<F>(from: NaiveDate, to: NaiveDate, fact_for: &F) -> Option<NaiveDate>
where
    F: Fn(NaiveDate) -> CalendarDateFact,
{
    let mut date = to;
    loop {
        if fact_for(date) == CalendarDateFact::TradingSession {
            return Some(date);
        }
        if date == from {
            return None;
        }
        date = date.pred_opt()?;
    }
}

/// Any proven session strictly between `left_to` and `right_from` — a session the split
/// would drop.
fn session_in_gap<F>(left_to: NaiveDate, right_from: NaiveDate, fact_for: &F) -> Option<NaiveDate>
where
    F: Fn(NaiveDate) -> CalendarDateFact,
{
    let mut date = left_to.succ_opt()?;
    while date < right_from {
        if fact_for(date) == CalendarDateFact::TradingSession {
            return Some(date);
        }
        date = date.succ_opt()?;
    }
    None
}

/// Derive the supply split from calendar facts (R1, R13).
///
/// `fact_for` is the calendar-fact injection seam, mirroring
/// [`crate::queue::window::derive_window`]: the operator step passes
/// `|d| date_fact_from_view(view.as_ref(), d)` against the real snapshot, and the
/// committed tests pass a closure over synthetic facts. Nothing here reads env or I/O, so
/// the committed suite needs no `adapters/nautilus/state/` directory.
///
/// The three partitions are disjoint and exhaustive over `floor ..= ceiling`: their
/// counts sum to `s_max` by construction, and the [`SplitError::SessionInGap`] check
/// proves nothing falls between them.
///
/// # Errors
///
/// See [`SplitError`]. Every variant refuses rather than adjusts.
pub fn derive_split<F>(anchors: SplitAnchors, fact_for: F) -> Result<DerivedSplit, SplitError>
where
    F: Fn(NaiveDate) -> CalendarDateFact,
{
    let SplitAnchors { floor, specification_to, holdout_from, holdout_to, reserved_from, ceiling } =
        anchors;

    // Ordering first — an out-of-order anchor makes every later check meaningless.
    for (lo, hi, what) in [
        (floor, specification_to, "floor .. specification_to"),
        (specification_to, holdout_from, "specification_to .. holdout_from"),
        (holdout_from, holdout_to, "holdout_from .. holdout_to"),
        (holdout_to, reserved_from, "holdout_to .. reserved_from"),
        (reserved_from, ceiling, "reserved_from .. ceiling"),
    ] {
        if lo >= hi {
            return Err(SplitError::AnchorsOutOfOrder {
                detail: format!("{what}: {lo} is not before {hi}"),
            });
        }
    }

    // Each partition must START on a proven session — the "not rounded to a neighbour"
    // rule. A `to` anchor may be a closed day; that case is covered by the gap check.
    for (partition, date) in [
        (Partition::Specification.name(), floor),
        (Partition::Holdout.name(), holdout_from),
        (Partition::Reserved.name(), reserved_from),
    ] {
        let fact = fact_for(date);
        if fact != CalendarDateFact::TradingSession {
            return Err(SplitError::BoundaryNotASession { partition, date, fact });
        }
    }
    let ceiling_fact = fact_for(ceiling);
    if ceiling_fact != CalendarDateFact::TradingSession {
        return Err(SplitError::CeilingNotASession { date: ceiling, fact: ceiling_fact });
    }

    // Nothing may fall between the partitions. A closed day at a boundary (the
    // specification window's 2019-12-31) is fine precisely because this finds no session
    // in the gap it opens.
    if let Some(date) = session_in_gap(specification_to, holdout_from, &fact_for) {
        return Err(SplitError::SessionInGap {
            date,
            left: Partition::Specification.name(),
            right: Partition::Holdout.name(),
        });
    }
    if let Some(date) = session_in_gap(holdout_to, reserved_from, &fact_for) {
        return Err(SplitError::SessionInGap {
            date,
            left: Partition::Holdout.name(),
            right: Partition::Reserved.name(),
        });
    }

    let part = |partition: Partition, from: NaiveDate, to: NaiveDate| -> Result<SplitPart, SplitError> {
        let sessions = count_proven_sessions(from, to, &fact_for)?;
        // `from` is a proven session (checked above), so both spans exist.
        let first = first_session(from, to, &fact_for)
            .ok_or(SplitError::BoundaryNotASession { partition: partition.name(), date: from, fact: fact_for(from) })?;
        let last = last_session(from, to, &fact_for)
            .ok_or(SplitError::BoundaryNotASession { partition: partition.name(), date: from, fact: fact_for(from) })?;
        Ok(SplitPart { partition, from, to, first_session: first, last_session: last, sessions })
    };

    let specification = part(Partition::Specification, floor, specification_to)?;
    let holdout = part(Partition::Holdout, holdout_from, holdout_to)?;
    let reserved = part(Partition::Reserved, reserved_from, ceiling)?;
    let s_max = count_proven_sessions(floor, ceiling, &fact_for)?;

    // Disjoint + exhaustive. Structural given the ordering and gap checks; asserted so a
    // future edit to those checks cannot quietly break the invariant the freeze rests on.
    // A returned error, never a `debug_assert!` — a debug-only panic would both hide this
    // path from release builds and pre-empt the typed refusal under `cargo test`.
    let summed = specification.sessions + holdout.sessions + reserved.sessions;
    if summed != s_max {
        return Err(SplitError::AnchorsOutOfOrder {
            detail: format!("partitions sum to {summed} but the ceiling counts {s_max}"),
        });
    }

    Ok(DerivedSplit { from: floor, to: ceiling, s_max, specification, holdout, reserved })
}

// ===========================================================================
// U3 — the frozen artifact + typed loader
// ===========================================================================

/// The frozen artifact's filename under `config/`.
pub const LINEAGE_PREREG_FILE: &str = "lineage-preregistration.json";

/// The artifact schema version this build reads. A file declaring anything else is a
/// typed refusal in [`load`] — never a silent partial read of a newer freeze.
pub const LINEAGE_PREREG_SCHEMA_VERSION: u32 = 1;

/// The committed frozen artifact: `<crate>/config/lineage-preregistration.json`, baked
/// from `CARGO_MANIFEST_DIR` so it resolves the same from any working directory.
#[must_use]
pub fn frozen_lineage_prereg_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("config").join(LINEAGE_PREREG_FILE)
}

/// A typed load / accessor failure.
#[derive(Debug, thiserror::Error)]
pub enum LineagePreRegError {
    /// The file could not be read.
    #[error("reading lineage pre-registration {path}: {source}")]
    Read {
        /// The path that failed.
        path: String,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The file could not be parsed. A missing required field lands here, and serde names
    /// the field — a typed error, never a panic.
    #[error("parsing lineage pre-registration {path}: {source}")]
    Parse {
        /// The path that failed.
        path: String,
        /// The underlying serde error, which names the offending field.
        source: serde_json::Error,
    },
    /// A field that is null by design was found populated (or vice versa).
    #[error("lineage pre-registration invariant violated: {detail}")]
    Invariant {
        /// What was violated.
        detail: String,
    },
    /// A judgment could not be recorded or read back.
    #[error("holdout judgment ledger {path}: {detail}")]
    Ledger {
        /// The ledger path.
        path: String,
        /// What went wrong.
        detail: String,
    },
    /// The holdout has already been judged. The refusal (R11) — a returned error naming
    /// the recorded attempt, never a logged warning.
    #[error("holdout already judged: run {run_id} claimed it at {claimed_utc} (N_max = 1, the holdout is spent)")]
    AlreadyJudged {
        /// The run id recorded on the claiming attempt.
        run_id: String,
        /// When the claim was recorded.
        claimed_utc: String,
    },
    /// A dry run was pointed at holdout dates. The specification-window path must not be
    /// able to produce a verdict over the holdout.
    #[error("specification-window dry run refuses {date}: it falls in the {partition} window, not the specification window")]
    DryRunOutsideSpecification {
        /// The offending date.
        date: NaiveDate,
        /// Which partition it actually falls in ("outside the ceiling" when unsplit).
        partition: String,
    },
}

/// One partition as frozen in the artifact (R1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenPartition {
    /// Proven trading sessions in `from ..= to`.
    pub sessions: usize,
    /// The declared range's first day.
    pub from: NaiveDate,
    /// The declared range's last day (inclusive). May be a closed day.
    pub to: NaiveDate,
    /// The first proven session at or after `from`.
    pub first_session: NaiveDate,
    /// The last proven session at or before `to`. Differs from `to` exactly when `to` is
    /// a closed day.
    pub last_session: NaiveDate,
}

/// The three-way split (R1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenSplit {
    /// Where the strategy specification is authored.
    pub specification: FrozenPartition,
    /// Spent by exactly one judgment.
    pub holdout: FrozenPartition,
    /// Quarantined forward accrual.
    pub reserved: FrozenPartition,
}

impl FrozenSplit {
    /// The three counts summed — must equal the ceiling (AE3).
    #[must_use]
    pub fn total_sessions(&self) -> usize {
        self.specification.sessions + self.holdout.sessions + self.reserved.sessions
    }
}

/// The origin plan's ceiling, recorded beside the measured one so the reconciliation is
/// in-band rather than only in prose (KTD4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginCeiling {
    /// The origin plan's session count.
    pub sessions: usize,
    /// The date it counted through.
    pub to: NaiveDate,
    /// Why it differs from the measured ceiling.
    pub delta_note: String,
}

/// Where the split's counts can be reproduced from (R13). The snapshot is machine-local
/// and gitignored, so the counts are **citation**-reproducible, not test-reproducible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitProvenance {
    /// The calendar snapshot's `artifact_id` at freeze time.
    pub calendar_artifact_id: String,
    /// The calendar snapshot's `calendar_id` at freeze time.
    pub calendar_id: String,
    /// How to reproduce the counts.
    pub reproduction: String,
}

/// The supply block (R1, R9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Supply {
    /// `S_max` — proven sessions to the walk anchor.
    pub s_max: usize,
    /// The origin plan's ceiling and the reconciliation.
    pub s_max_origin_plan: OriginCeiling,
    /// The P4 walk's mean per-symbol **listing depth** — a survivorship UPPER bound on
    /// tradable participation, NOT the clustering table's `p` (KTD8).
    pub universe_listing_depth: f64,
    /// What `universe_listing_depth` does and does not mean.
    pub universe_listing_depth_basis: String,
    /// The universe's floor listed count, the denominator of `selection_breadth`.
    pub listed_count_min: usize,
    /// The three-way split.
    pub split: FrozenSplit,
    /// Where the counts come from.
    pub provenance: SplitProvenance,
}

/// The lineage-level multiplicity statement (KTD12, R16).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineageMultiplicity {
    /// The maximum number of one-sided judgments this lineage may ever take.
    pub judgments_max: usize,
    /// What correction is applied across those judgments, and why.
    pub lifetime_correction: String,
}

/// The search budget (R2, R16).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Search {
    /// Trials permitted against the holdout. Frozen at 1.
    pub n_max: usize,
    /// Null by design: at `N_max = 1` the expected maximum of one draw from a zero-mean
    /// null is exactly zero, so dispersion never enters a single judgment's arithmetic.
    pub sigma_trials: Option<f64>,
    /// The condition under which `sigma_trials` would enter the arithmetic.
    pub sigma_trials_trigger: String,
    /// The lineage-level multiplicity the finite cap does not correct for.
    pub lineage_multiplicity: LineageMultiplicity,
}

/// One frozen term's named derivation input (R4, R13).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivationNote {
    /// The named input the value derives from.
    pub input: String,
    /// Whether the value is measured, derived, or inferred (R16).
    pub basis: String,
}

/// The hypothesis block (R3, R4, R9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hypothesis {
    /// The strategy class this lineage may search within.
    pub class: String,
    /// The hypothesized effect size as a ratio to ORB's measured gross edge (R3).
    pub effect_size_ratio_to_orb_gross: f64,
    /// The origin plan's rounded quotation of the same ratio, kept for traceability.
    pub effect_size_ratio_origin_plan_rounded: f64,
    /// The required **net** RoR the ratio was solved against.
    pub effect_size_net_ror: f64,
    /// The required **gross** per-position edge in R, before the round-trip cost.
    pub effect_size_gross_r: f64,
    /// ORB's measured gross edge in R — the ratio's denominator.
    pub orb_measured_gross_r: f64,
    /// ORB's measured net edge in R (`sample-margin.json` `provenance.net_r_mean`).
    pub orb_measured_net_r: f64,
    /// ORB's measured round-trip cost in R — `gross − net` at its implied stop.
    pub orb_measured_cost_r: f64,
    /// The holding period in sessions — `ceil(ratio²)` under √-time scaling (R4).
    pub holding_period_sessions: usize,
    /// Long-only: long/short adds a borrow-availability surface the SDK cannot answer.
    pub directionality: String,
    /// Target trades per session (R4).
    pub target_m: usize,
    /// The clustering table's `p` — the fraction of sessions the strategy trades. A
    /// take-top-N-every-session ranking has `p = 1.0` by construction (KTD8).
    pub target_session_participation: f64,
    /// `target_m × holding_period_sessions` (R9).
    pub steady_state_concurrency: usize,
    /// `steady_state_concurrency / listed_count_min` (R9).
    pub selection_breadth: f64,
    /// The stop rule that denominates net RoR (R4, KTD14).
    pub stop_rule: String,
    /// The stop width as a fraction of price implied by ORB's measured cost.
    pub stop_implied_pct_of_price: f64,
    /// Per-term derivation inputs, keyed by field name.
    pub derivation: std::collections::BTreeMap<String, DerivationNote>,
}

/// The verdict predicate (R6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verdict {
    /// The statistic the verdict is taken on.
    pub statistic: String,
    /// The bootstrap block length in sessions — never below the holding period (R5).
    pub bootstrap_block_length_sessions: usize,
    /// The holdout bar: `margin_bar_n1` at the frozen holdout count.
    pub bar: f64,
    /// The frozen choice: the haircut as a fraction of the bar (KTD2).
    pub haircut_fraction: f64,
    /// Derived: `haircut_fraction × bar`.
    pub haircut: f64,
    /// Derived: `bar + haircut` — the hurdle the observed statistic must exceed.
    pub hurdle: f64,
    /// The executable predicate, stated so two operators reach the same boolean.
    pub predicate: String,
    /// What the haircut covers and why it is a pre-registered constant, not an estimate.
    pub haircut_basis: String,
    /// Why the block length is tied to the hold rather than frozen as a literal.
    pub block_length_basis: String,
}

/// One scheduled upgrade turn (R7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpgradeTurn {
    /// The turn number (1-based). These are the scheduled **upgrade** turns, additional to
    /// the turn-one holdout judgment — which is why lifetime judgments are
    /// `1 + max_turns`, not `max_turns`.
    pub turn: usize,
    /// The segment's session count.
    pub segment_sessions: usize,
    /// `margin_bar_n1` at `segment_sessions`.
    pub bar: f64,
    /// `haircut_fraction × bar`.
    pub haircut: f64,
    /// `bar + haircut` — must sit below the registered effect size or the turn is
    /// unclearable and the schedule is a lie.
    pub hurdle: f64,
}

/// The finite upgrade schedule (R7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpgradeSchedule {
    /// The maximum number of turns this lineage may ever take.
    pub max_turns: usize,
    /// The smallest segment at which the registered effect still clears its own
    /// `bar + haircut` at the registered power.
    pub segment_min_sessions: usize,
    /// The scheduled turns.
    pub turns: Vec<UpgradeTurn>,
    /// How the segment floor was solved, and why a shorter segment cannot be added.
    pub segment_basis: String,
    /// What exhausting the schedule means.
    pub exhaustion: String,
}

/// The two gates that bracket the lineage (R8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gates {
    /// The pre-turn admissibility re-check that must clear before the lineage opens.
    pub pre_turn_admissibility_recheck: String,
    /// The prospective paper stage that conditions labelling the lineage successful.
    pub prospective_paper_stage: String,
}

/// The registered statistical power the effect size and segment floor are solved at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Power {
    /// One-sided power (0.80).
    pub power: f64,
    /// Two-sided confidence the bar itself is struck at (0.95).
    pub confidence: f64,
    /// Which named constants realize these two figures, and the rounding involved.
    pub basis: String,
}

/// The lineage's identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lineage {
    /// The lineage's stable slug.
    pub name: String,
    /// The hypothesis class the specification must fall within.
    pub hypothesis_class: String,
    /// The predecessor lineage and its closure.
    pub predecessor: String,
}

/// The frozen lineage pre-registration.
///
/// Every field is required. Two are `Option` and **null by design**:
/// [`Search::sigma_trials`] (KTD12) and [`LineagePreRegistration::holdout_judged`]
/// (KTD10 — judgments live in the ledger so the artifact's bytes never change).
///
/// `deny_unknown_fields` is load-bearing, not tidiness. Serde reads a *missing* `Option`
/// field as `None`, so without it a rename — `holdout_judged` → `holdoutJudged` — would
/// drop the unknown key, default the real field to `None`, and load a drifted artifact
/// clean. The whole point of the freeze is that a silent edit cannot pass as the frozen
/// terms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineagePreRegistration {
    /// Schema version.
    pub schema_version: u32,
    /// When the terms were frozen.
    pub frozen_utc: String,
    /// The lineage's identity.
    pub lineage: Lineage,
    /// The registered power and confidence.
    pub power: Power,
    /// Session supply and its split.
    pub supply: Supply,
    /// The search budget.
    pub search: Search,
    /// The hypothesized effect and the terms it implies.
    pub hypothesis: Hypothesis,
    /// The verdict predicate.
    pub verdict: Verdict,
    /// The finite upgrade schedule.
    pub upgrade_schedule: UpgradeSchedule,
    /// The pre-turn and post-judgment gates.
    pub gates: Gates,
    /// **Always null.** The judgment record lives in the append-only ledger so this
    /// file's bytes — and therefore its content-hash citation — survive the judgment
    /// (KTD10, R12). A populated value here is a defect at any time.
    pub holdout_judged: Option<serde_json::Value>,
    /// What the freeze does not claim (R16).
    pub not_claimed: Vec<String>,
    /// The conditions that invalidate the freeze (R15).
    pub rederivation_trigger: String,
}

/// A loaded frozen artifact plus the SHA-256 citation over its exact bytes (R14).
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedLineagePreReg {
    /// The parsed values.
    pub values: LineagePreRegistration,
    /// SHA-256 hex of the raw file bytes — the citation every judgment records.
    pub content_hash: String,
}

/// Load the frozen artifact and emit its content-hash citation (R14).
///
/// # Errors
///
/// [`LineagePreRegError::Read`] when the file is absent or unreadable,
/// [`LineagePreRegError::Parse`] when it is malformed or missing a required field (serde
/// names the field), and [`LineagePreRegError::Invariant`] when `holdout_judged` is
/// populated — which it must never be.
pub fn load(path: &Path) -> Result<LoadedLineagePreReg, LineagePreRegError> {
    let bytes = std::fs::read(path)
        .map_err(|source| LineagePreRegError::Read { path: path.display().to_string(), source })?;
    let content_hash = hash_bytes(&bytes);
    let values: LineagePreRegistration = serde_json::from_slice(&bytes)
        .map_err(|source| LineagePreRegError::Parse { path: path.display().to_string(), source })?;
    if values.schema_version != LINEAGE_PREREG_SCHEMA_VERSION {
        return Err(LineagePreRegError::Invariant {
            detail: format!(
                "{} declares schema version {} but this build reads {} — a newer freeze is a typed refusal, never a silent partial read",
                path.display(),
                values.schema_version,
                LINEAGE_PREREG_SCHEMA_VERSION
            ),
        });
    }
    if values.holdout_judged.is_some() {
        return Err(LineagePreRegError::Invariant {
            detail: format!(
                "holdout_judged is populated in {} — judgments live in the ledger so this file's bytes never change (KTD10)",
                path.display()
            ),
        });
    }
    Ok(LoadedLineagePreReg { values, content_hash })
}

/// Load the frozen artifact if it exists.
///
/// # Errors
///
/// As [`load`], except that an absent file is `Ok(None)`.
pub fn load_optional(path: &Path) -> Result<Option<LoadedLineagePreReg>, LineagePreRegError> {
    // Same discipline as `JudgmentLedger::read_text`: only `NotFound` means absent. A
    // `Path::exists()` probe would map a permission error to "no freeze exists", which is
    // the opposite of fail-closed for a governance artifact.
    match std::fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        _ => Ok(Some(load(path)?)),
    }
}

impl LineagePreRegistration {
    /// The holdout partition — the only partition a verdict may be taken on.
    #[must_use]
    pub fn holdout(&self) -> &FrozenPartition {
        &self.supply.split.holdout
    }

    /// The specification window — freely observable, never a verdict surface.
    #[must_use]
    pub fn specification(&self) -> &FrozenPartition {
        &self.supply.split.specification
    }

    /// The reserved tail. Deliberately **not** reachable from any holdout accessor: the
    /// quarantine only means something if the judgment path cannot see it.
    #[must_use]
    pub fn reserved(&self) -> &FrozenPartition {
        &self.supply.split.reserved
    }

    /// The holdout bar.
    #[must_use]
    pub fn bar(&self) -> f64 {
        self.verdict.bar
    }

    /// The survivorship + eligibility haircut (`haircut_fraction × bar`).
    #[must_use]
    pub fn haircut(&self) -> f64 {
        self.verdict.haircut
    }

    /// The verdict hurdle — `bar + haircut`.
    #[must_use]
    pub fn hurdle(&self) -> f64 {
        self.verdict.hurdle
    }

    /// The verdict predicate, resolved (R6, AE1): `observed_net_ror − haircut > bar`.
    ///
    /// Callers must use this rather than re-deriving the arithmetic locally, so two
    /// operators reading the same holdout cannot reach different answers.
    #[must_use]
    pub fn clears(&self, observed_net_ror: f64) -> bool {
        observed_net_ror - self.haircut() > self.bar()
    }

    /// Whether `date` falls inside the specification window.
    #[must_use]
    pub fn is_specification_date(&self, date: NaiveDate) -> bool {
        let p = self.specification();
        date >= p.from && date <= p.to
    }

    /// Which partition `date` falls in, by name, or `None` outside the ceiling.
    #[must_use]
    pub fn partition_name_of(&self, date: NaiveDate) -> Option<&'static str> {
        for (part, name) in [
            (self.specification(), Partition::Specification.name()),
            (self.holdout(), Partition::Holdout.name()),
            (self.reserved(), Partition::Reserved.name()),
        ] {
            if date >= part.from && date <= part.to {
                return Some(name);
            }
        }
        None
    }
}

// ===========================================================================
// U5 — claim-then-evaluate refusal
// ===========================================================================

/// The current judgment-record schema version.
pub const JUDGMENT_SCHEMA_VERSION: u32 = 1;

/// The append-only holdout-judgment ledger, relative to the lab crate root — the same
/// tracked-home convention [`crate::trials::LEDGER_RELPATH`] uses.
pub const JUDGMENT_LEDGER_RELPATH: &str = "ledger/lineage-holdout-judgments.jsonl";

/// The committed judgment ledger path, baked from `CARGO_MANIFEST_DIR`.
#[must_use]
pub fn judgment_ledger_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(JUDGMENT_LEDGER_RELPATH)
}

/// One recorded holdout-evaluation **attempt** (R10).
///
/// The attempt is appended *before* the verdict is computed, so a crash, a panic, or an
/// operator who dislikes the answer and never writes it back all leave the claim behind.
/// The verdict fields are absent on the claim and are never back-filled by this module —
/// a claim is the load-bearing record, and a partial line is still a claim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JudgmentAttempt {
    /// Schema version.
    pub schema_version: u32,
    /// The run that claimed the holdout.
    pub run_id: String,
    /// The catalog fingerprint the evaluation ran against.
    pub catalog_fingerprint: String,
    /// When the claim was recorded (UTC, caller-supplied so tests are deterministic).
    pub claimed_utc: String,
    /// The frozen artifact's content hash at claim time — the citation the judgment binds
    /// to (R12/R14).
    pub prereg_content_hash: String,
    /// The verdict, when the evaluation got that far. **Never load-bearing for refusal.**
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_net_ror: Option<f64>,
    /// Whether the observed statistic cleared the hurdle, when computed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleared: Option<bool>,
}

/// A partially-written ledger line. Only `run_id` and `claimed_utc` are needed to refuse;
/// everything else is best-effort. A line that parses as *anything* is a recorded attempt.
#[derive(Debug, Clone, Deserialize)]
struct PartialAttempt {
    #[serde(default)]
    run_id: Option<String>,
    #[serde(default)]
    claimed_utc: Option<String>,
}

/// The append-only holdout-judgment ledger.
///
/// Mechanics mirror [`crate::trials::TrialsLedger`]: `OpenOptions::append(true)
/// .create(true)`, one `write_all` for record+newline so a crash between content and
/// terminator cannot tear a line, and create-if-missing with parent dirs.
#[derive(Clone, Debug)]
pub struct JudgmentLedger {
    path: PathBuf,
}

impl JudgmentLedger {
    /// A ledger at `path` (created lazily on first append).
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        JudgmentLedger { path: path.into() }
    }

    /// The ledger file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read the ledger's bytes, distinguishing "absent" from "cannot be read".
    ///
    /// **Never probe with [`Path::exists`] first.** `exists()` maps *every* `metadata`
    /// failure — permission denied, a dangling symlink, an I/O error — to `false`, so an
    /// unreadable-but-present ledger would read as "never judged" and buy a second
    /// verdict. Only `NotFound` may mean absent; everything else refuses.
    fn read_text(&self) -> Result<Option<String>, LineagePreRegError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(LineagePreRegError::Ledger {
                path: self.path.display().to_string(),
                detail: format!("cannot read: {e}"),
            }),
        }
    }

    /// The recorded claim, if the holdout has been claimed.
    ///
    /// **Fail-closed on garbage, and on an unreadable file.** A line that cannot be parsed
    /// at all is still evidence that *something* claimed the holdout, so it refuses with
    /// placeholders rather than reading as absent. A partial line whose `run_id` or
    /// `claimed_utc` is missing refuses with `"(unrecorded)"` in that slot. **A present
    /// but empty file is also a claim** — [`append`](Self::append) creates the file
    /// exclusively *before* it writes, so a zero-byte ledger is the fingerprint of a run
    /// that claimed and then died mid-write, which must not buy a second look. Only a
    /// `NotFound` ledger reads as "not yet judged".
    ///
    /// # Errors
    ///
    /// [`LineagePreRegError::Ledger`] when the file exists but cannot be read.
    pub fn claim(&self) -> Result<Option<(String, String)>, LineagePreRegError> {
        let Some(text) = self.read_text()? else {
            return Ok(None);
        };
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let (run_id, claimed_utc) = match serde_json::from_str::<PartialAttempt>(line) {
                Ok(p) => (p.run_id, p.claimed_utc),
                // Unparseable is still a claim — a torn line must never read as absent.
                Err(_) => (None, None),
            };
            return Ok(Some((
                run_id.unwrap_or_else(|| "(unrecorded)".to_string()),
                claimed_utc.unwrap_or_else(|| "(unrecorded)".to_string()),
            )));
        }
        // The file exists but carries no record: a claim that died before its bytes
        // landed. Refuse with placeholders rather than reading it as unclaimed.
        Ok(Some(("(unrecorded)".to_string(), "(unrecorded)".to_string())))
    }

    /// Append one attempt as a compact single-line JSON. Never truncates.
    ///
    /// **The first append is the atomic claim.** It opens with `create_new(true)`
    /// (`O_CREAT|O_EXCL`), mirroring [`nautilus_ls::lock::AdvisoryLock::acquire`]: the
    /// create either wins or returns `AlreadyExists`, so two processes racing
    /// [`judge_holdout`] cannot both pass the check-then-act window that a plain
    /// `create(true)` would leave open. A caller that legitimately holds the claim and is
    /// appending a *subsequent* audit record passes `first_claim: false` to fall back to
    /// an ordinary append.
    ///
    /// # Errors
    ///
    /// [`LineagePreRegError::AlreadyJudged`] when `first_claim` is set and the ledger
    /// already exists, or [`LineagePreRegError::Ledger`] when the parent directory or file
    /// cannot be created, the write fails, or serialization fails.
    pub fn append(&self, attempt: &JudgmentAttempt, first_claim: bool) -> Result<(), LineagePreRegError> {
        let fail = |detail: String| LineagePreRegError::Ledger {
            path: self.path.display().to_string(),
            detail,
        };
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| fail(format!("cannot create parent: {e}")))?;
        }
        let mut line =
            serde_json::to_string(attempt).map_err(|e| fail(format!("cannot serialize: {e}")))?;
        line.push('\n');
        let opened = if first_claim {
            // O_CREAT|O_EXCL — the create IS the claim. Losing this race is not an I/O
            // error, it is the refusal.
            OpenOptions::new().write(true).create_new(true).open(&self.path)
        } else {
            OpenOptions::new().append(true).create(true).open(&self.path)
        };
        let mut file = match opened {
            Ok(f) => f,
            Err(e) if first_claim && e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Another caller created the ledger first. Read back who, so the refusal
                // names them; a claim we cannot attribute still refuses.
                let (run_id, claimed_utc) = self.claim()?.unwrap_or_else(|| {
                    ("(unrecorded)".to_string(), "(unrecorded)".to_string())
                });
                return Err(LineagePreRegError::AlreadyJudged { run_id, claimed_utc });
            }
            Err(e) => return Err(fail(format!("cannot open for append: {e}"))),
        };
        file.write_all(line.as_bytes()).map_err(|e| fail(format!("cannot append: {e}")))?;
        Ok(())
    }

    /// Every attempt that parses, in append order (audit read; refusal never uses this).
    ///
    /// # Errors
    ///
    /// [`LineagePreRegError::Ledger`] when the file cannot be read or a line is malformed.
    pub fn read_all(&self) -> Result<Vec<JudgmentAttempt>, LineagePreRegError> {
        let Some(text) = self.read_text()? else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let rec: JudgmentAttempt =
                serde_json::from_str(line).map_err(|e| LineagePreRegError::Ledger {
                    path: self.path.display().to_string(),
                    detail: format!("line {}: {e}", i + 1),
                })?;
            out.push(rec);
        }
        Ok(out)
    }
}

/// The outcome of the one permitted holdout judgment.
#[derive(Debug, Clone, PartialEq)]
pub struct HoldoutVerdict {
    /// The observed statistic.
    pub observed_net_ror: f64,
    /// Whether it cleared `bar + haircut`.
    pub cleared: bool,
    /// The hurdle it was measured against.
    pub hurdle: f64,
    /// The frozen artifact's content hash the verdict binds to.
    pub prereg_content_hash: String,
    /// The run that claimed the holdout.
    pub run_id: String,
}

/// Judge the holdout — **claim first, then evaluate** (R10, R11, KTD10).
///
/// The attempt is appended to `ledger` *before* the verdict is computed. A second call
/// finds the claim and returns [`LineagePreRegError::AlreadyJudged`] naming the recorded
/// run id and UTC. The frozen artifact is never written to, so its content-hash citation
/// survives the judgment (R12).
///
/// The refusal is **git-auditable, not tamper-proof**: a revert can still remove the
/// ledger line. The prose companion says so.
///
/// # Errors
///
/// [`LineagePreRegError::AlreadyJudged`] when the holdout is already claimed, or
/// [`LineagePreRegError::Ledger`] when the claim cannot be recorded. A claim that cannot
/// be recorded refuses to evaluate — an unrecordable judgment is not a judgment.
pub fn judge_holdout(
    prereg: &LoadedLineagePreReg,
    ledger: &JudgmentLedger,
    run_id: &str,
    catalog_fingerprint: &str,
    claimed_utc: &str,
    observed_net_ror: f64,
) -> Result<HoldoutVerdict, LineagePreRegError> {
    // Read first so an already-spent holdout refuses with the recorded attribution rather
    // than the bare AlreadyExists the exclusive create would give.
    if let Some((claim_run, claim_utc)) = ledger.claim()? {
        return Err(LineagePreRegError::AlreadyJudged { run_id: claim_run, claimed_utc: claim_utc });
    }

    // Claim BEFORE evaluating, and claim ATOMICALLY: `first_claim` opens the ledger with
    // O_CREAT|O_EXCL, so the read above is an attribution nicety, not the guard. Two
    // processes that both saw an unclaimed ledger cannot both get past this line —
    // exactly one create wins and the loser is refused. Everything after it is
    // unreachable a second time.
    ledger.append(
        &JudgmentAttempt {
            schema_version: JUDGMENT_SCHEMA_VERSION,
            run_id: run_id.to_string(),
            catalog_fingerprint: catalog_fingerprint.to_string(),
            claimed_utc: claimed_utc.to_string(),
            prereg_content_hash: prereg.content_hash.clone(),
            observed_net_ror: None,
            cleared: None,
        },
        true,
    )?;

    let cleared = prereg.values.clears(observed_net_ror);
    Ok(HoldoutVerdict {
        observed_net_ror,
        cleared,
        hurdle: prereg.values.hurdle(),
        prereg_content_hash: prereg.content_hash.clone(),
        run_id: run_id.to_string(),
    })
}

/// A non-consuming dry run over the **specification window only**.
///
/// Deliberately a different function with a different name and a different range check:
/// the consuming path is [`judge_holdout`] and nothing else. This one takes no ledger, so
/// it structurally cannot claim, and it refuses any date outside the specification window.
///
/// **What the range check does not buy.** `observed_net_ror` is a bare `f64` supplied by
/// the caller, so this function cannot verify the statistic was actually computed over the
/// dates it was handed. A caller who computes a holdout number and passes specification
/// dates gets an answer. The window check makes the *declared* scope explicit and refuses
/// the obvious mistake; binding a statistic to the window that produced it needs a typed
/// observation carrying its own provenance, and there is no producer to type against until
/// the daily multi-session-hold backtest path exists (P7). Recorded in the artifact's
/// `not_claimed` rather than papered over here.
///
/// # Errors
///
/// [`LineagePreRegError::DryRunOutsideSpecification`] when `from` or `to` falls outside
/// the specification window, or when `to` precedes `from` — the endpoint check is only
/// sufficient for a contiguous window if the range is the right way round.
pub fn specification_dry_run(
    prereg: &LineagePreRegistration,
    from: NaiveDate,
    to: NaiveDate,
    observed_net_ror: f64,
) -> Result<bool, LineagePreRegError> {
    if to < from {
        return Err(LineagePreRegError::DryRunOutsideSpecification {
            date: to,
            partition: format!("an inverted range ending before its start {from}"),
        });
    }
    for date in [from, to] {
        if !prereg.is_specification_date(date) {
            return Err(LineagePreRegError::DryRunOutsideSpecification {
                date,
                partition: prereg
                    .partition_name_of(date)
                    .unwrap_or("outside the ceiling")
                    .to_string(),
            });
        }
    }
    Ok(prereg.clears(observed_net_ror))
}
