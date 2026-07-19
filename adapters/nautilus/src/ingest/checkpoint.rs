//! Resumable ingest checkpoints (R5, AE2).
//!
//! A JSON state file beside the catalog records the completed
//! `(instrument, bar type, date range)` triples plus coverage gaps (empty history,
//! `01715`, paper-thin feeds), so an interrupted or repeated run **skips and
//! reports** rather than refetching. The checkpoint is written after each triple
//! completes, so a crash loses at most the in-flight triple.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use chrono::{Datelike, NaiveDate};
use nautilus_ls_calendar::{CalendarAdoption, DimensionStaleness};
use serde::{Deserialize, Serialize};

use crate::error::{AdapterError, AdapterResult};
use crate::ingest::{CalendarGate, ContinuityDecision};

/// Per-series (instrument + bar type) cap on retained [`RebaseEvent`]s (U6/KTD7),
/// oldest-dropped. Bounds the checkpoint-rewrite cost over the ~2,600-symbol
/// universe post-epoch. A judgment call, not a config knob — adjust here with a
/// comment if the audit depth needs to change; the per-origin evicted counters
/// preserve the true totals regardless of the cap.
const REBASE_EVENTS_PER_SERIES_CAP: usize = 4;

/// Why an `(instrument, bar type, range)` triple produced no bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapReason {
    /// The gateway returned no rows for the range (short/empty history).
    EmptyHistory,
    /// The gateway returned `01715` (a non-trading-day / bad-date range).
    NonTradingDay,
    /// A paper-thin feed (rows present but below a usable threshold).
    PaperThin,
}

/// A recorded coverage gap: a triple that yielded no usable bars.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageGap {
    /// The instrument id (`{shcode}.XKRX`).
    pub instrument: String,
    /// The bar-type label (e.g. `1-DAY`, `1-MINUTE`).
    pub bar_type: String,
    /// The requested date range (`sdate..edate`).
    pub range: String,
    /// Why the gap was recorded.
    pub reason: GapReason,
}

/// A `(instrument, bar type)` left with non-contiguous `completed` ranges after
/// the legacy `completed`→`watermarks` migration (U2/KTD-3): ranges beyond a
/// coverage hole (a weekday gap, or a `PaperThin` truncated fetch) are NOT folded
/// into the derived watermark — deriving past them would skip un-fetched history
/// forever (R2). They stay in `completed`, and this report entry names the escape
/// hatch so the operator can recover them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationRemainder {
    /// The instrument id (`{shcode}.XKRX`).
    pub instrument: String,
    /// The bar-type label (e.g. `1-DAY`).
    pub bar_type: String,
    /// The `completed` range keys left beyond the coverage hole.
    pub ranges: Vec<String>,
}

/// Whether a trading weekday lies strictly between `after` and `before`
/// (exclusive) — the coverage-hole test for the migration chain (KTD-3). A
/// weekend-only or empty gap is contiguous (no weekday between), so ranges either
/// side of it chain; a weekday in the gap is un-attested history and breaks the
/// chain.
fn weekday_strictly_between(after: NaiveDate, before: NaiveDate) -> bool {
    let mut d = match after.succ_opt() {
        Some(d) => d,
        None => return false,
    };
    while d < before {
        if d.weekday().num_days_from_monday() < 5 {
            return true;
        }
        d = match d.succ_opt() {
            Some(d) => d,
            None => return false,
        };
    }
    false
}

/// The origin of a basis-shift mark / re-base event (KTD6): an organic forward
/// detection vs the one-time epoch re-base, or unknown for rows written before
/// origin tracking existed. Snake_case serde; `#[serde(default)]` = unknown so
/// legacy checkpoints load (mirrors the [`GapReason`] precedent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RebaseOrigin {
    /// An organic basis-shift heal (forward detection during accumulate).
    Heal,
    /// The one-time epoch re-base (mark-all rollout).
    Epoch,
    /// Written before origin tracking — presumed organic under the sequencing
    /// assumption (see the README audit-trail note).
    #[default]
    Unknown,
}

impl RebaseOrigin {
    /// The stable string key used for the origin-split evicted counters (U6).
    pub fn as_key(self) -> &'static str {
        match self {
            RebaseOrigin::Heal => "heal",
            RebaseOrigin::Epoch => "epoch",
            RebaseOrigin::Unknown => "unknown",
        }
    }
}

/// Per-origin re-base totals (KTD6/R8): live event rows plus evicted counts, so
/// the audit trail survives the per-series cap eviction (U6). Unknown is presumed
/// organic under the sequencing assumption; the three buckets stay separate so an
/// operator can re-judge unknown rows if that assumption breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RebaseOriginTotals {
    /// Organic heal detections.
    pub heal: u64,
    /// Epoch re-base events.
    pub epoch: u64,
    /// Pre-tracking rows of unknown origin (presumed organic).
    pub unknown: u64,
}

impl RebaseOriginTotals {
    /// The organic total: heals plus unknown (presumed organic), excluding epoch
    /// (R8 — the organic metric must never count the one-time epoch rows).
    pub fn organic(&self) -> u64 {
        self.heal + self.unknown
    }
}

/// A durably recorded basis-shift heal (R5): one row per completed re-base, so an
/// operator can audit how often the gateway rewrites a symbol's adjusted series.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RebaseEvent {
    /// The instrument id (`{shcode}.XKRX`).
    pub instrument: String,
    /// The bar-type label (e.g. `1-DAY`).
    pub bar_type: String,
    /// The session date (`YYYYMMDD`) the shift was detected.
    pub detected: String,
    /// The session date (`YYYYMMDD`) the heal completed.
    pub healed: String,
    /// The origin of this re-base (KTD6), stamped from the mark's origin at heal
    /// time. `#[serde(default)]` = unknown so legacy rows load.
    #[serde(default)]
    pub origin: RebaseOrigin,
}

/// The persisted ingest state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Completed `(instrument, bar type, range)` keys (see [`Self::key`]).
    completed: BTreeSet<String>,
    /// Recorded coverage gaps.
    gaps: Vec<CoverageGap>,
    /// Per-`(instrument, bar type)` accumulate-forward coverage watermark: the last
    /// **closed** session date (`YYYYMMDD`) whose bars are covered (U5, KTD7). In
    /// accumulate mode this is the sole skip authority. `#[serde(default)]` so
    /// pre-U5 checkpoint files load unchanged (empty map → derive from scratch).
    #[serde(default)]
    watermarks: BTreeMap<String, String>,
    /// Per-`(instrument, bar type)` recorded backward-widen history floor
    /// (`YYYYMMDD`), keyed like `watermarks` (U3/KTD-2, R4/R6). The deepest
    /// configured lookback floor for which a backward-widen no-op warning has
    /// already fired, so a repeat run at the same-or-higher floor stays silent and
    /// skips the per-triple `stored_bar_intervals` read done solely for that check.
    /// A deeper floor (below the recorded one) re-warns and updates this. Absent =
    /// never warned. `#[serde(default)]` so legacy files load with an empty map.
    #[serde(default)]
    history_floors: BTreeMap<String, String>,
    /// Per-`(instrument, bar type)` basis-shift marks: detection date (`YYYYMMDD`)
    /// keyed like `watermarks`. A marked triple heals (wipe → re-pull → re-verify)
    /// before any append; the mark outranks the watermark as authority (KTD-2).
    /// `#[serde(default)]` so pre-heal checkpoint files load unchanged.
    #[serde(default)]
    shifted: BTreeMap<String, String>,
    /// Per-`(instrument, bar type)` basis-shift **origin** (KTD5/KTD6), keyed
    /// identically to `shifted` and maintained in lock-step inside
    /// [`Self::mark_shifted`]/[`Self::clear_shifted`] so it can never diverge from
    /// the mark. An absent key reads as [`RebaseOrigin::Unknown`], so legacy
    /// shifted marks (written before origin tracking) load as unknown.
    /// `#[serde(default)]` for legacy files.
    #[serde(default)]
    shifted_origin: BTreeMap<String, RebaseOrigin>,
    /// Completed re-base events, bounded per series (R5/R9, cap in
    /// [`Self::record_rebase_event`]). `#[serde(default)]` for legacy files.
    #[serde(default)]
    rebase_events: Vec<RebaseEvent>,
    /// Origin-split counters for re-base events evicted by the per-series cap
    /// (U6/KTD7), keyed by [`RebaseOrigin::as_key`]. Preserve the true per-origin
    /// totals across evictions so the audit metric is eviction-stable (R9).
    /// `BTreeMap` for deterministic serialization; `#[serde(default)]` for legacy.
    #[serde(default)]
    rebase_evicted: BTreeMap<String, u64>,
    /// Whether daily bars were ingested with adjusted prices (`sujung="Y"`,
    /// KTD5). Recorded as catalog metadata so downstream knows the price basis.
    pub adjusted_prices: bool,
}

impl Checkpoint {
    /// The canonical key for a triple.
    pub fn key(instrument: &str, bar_type: &str, range: &str) -> String {
        format!("{instrument}|{bar_type}|{range}")
    }

    /// The `(instrument, bar type)` watermark key (range-independent, U5).
    pub fn watermark_key(instrument: &str, bar_type: &str) -> String {
        format!("{instrument}|{bar_type}")
    }

    /// The coverage watermark for a `(instrument, bar type)`, if any (U5).
    pub fn watermark(&self, instrument: &str, bar_type: &str) -> Option<NaiveDate> {
        self.watermarks
            .get(&Self::watermark_key(instrument, bar_type))
            .and_then(|s| NaiveDate::parse_from_str(s.trim(), "%Y%m%d").ok())
    }

    /// Advance the coverage watermark for a `(instrument, bar type)` to `date` (U5).
    pub fn set_watermark(&mut self, instrument: &str, bar_type: &str, date: NaiveDate) {
        self.watermarks.insert(
            Self::watermark_key(instrument, bar_type),
            date.format("%Y%m%d").to_string(),
        );
    }

    /// The recorded backward-widen history floor for a `(instrument, bar type)`,
    /// if a no-op warning has already fired for it (U3/KTD-2). Parsed from the
    /// stored `YYYYMMDD`.
    pub fn history_floor(&self, instrument: &str, bar_type: &str) -> Option<NaiveDate> {
        self.history_floors
            .get(&Self::watermark_key(instrument, bar_type))
            .and_then(|s| NaiveDate::parse_from_str(s.trim(), "%Y%m%d").ok())
    }

    /// Record the backward-widen history floor for a `(instrument, bar type)` to
    /// `floor` (U3/KTD-2) — set when a no-op warning fires, so a later run at the
    /// same-or-higher floor stays silent and skips the interval read.
    pub fn set_history_floor(&mut self, instrument: &str, bar_type: &str, floor: NaiveDate) {
        self.history_floors.insert(
            Self::watermark_key(instrument, bar_type),
            floor.format("%Y%m%d").to_string(),
        );
    }

    /// Clear the coverage watermark for a `(instrument, bar type)` — the heal's
    /// wipe step (KTD-2): a wiped series must re-pull from the floor, so its
    /// watermark must not survive the wipe.
    pub fn clear_watermark(&mut self, instrument: &str, bar_type: &str) {
        self.watermarks
            .remove(&Self::watermark_key(instrument, bar_type));
    }

    /// The `completed` coverage intervals for a `(instrument, bar type)` whose end
    /// date lies strictly above `watermark` (U4/KTD-1, R1/R2/R3) — parsed to
    /// `(sdate, edate)` date pairs, sorted by start, and adjacent/overlapping spans
    /// merged into one. This is the in-memory record of coverage that survives
    /// above the (prefix) watermark after a legacy multi-range migration: the
    /// prune keeps ranges with `edate > watermark` and the migration keeps far
    /// ranges in `completed`, so the accumulate fetch can trim against these spans
    /// (skip re-fetching a range the checkpoint already records) without reading
    /// parquet or consulting a calendar. Ranges with `edate <= watermark` (already
    /// attested by the watermark, e.g. the prefix range that derived it) are
    /// excluded.
    pub fn completed_intervals_above(
        &self,
        instrument: &str,
        bar_type: &str,
        watermark: NaiveDate,
    ) -> Vec<(NaiveDate, NaiveDate)> {
        let prefix = format!("{instrument}|{bar_type}|");
        let mut spans: Vec<(NaiveDate, NaiveDate)> = self
            .completed
            .iter()
            .filter_map(|k| k.strip_prefix(&prefix))
            .filter_map(|range| {
                let mut it = range.split("..");
                let (s, e) = (it.next()?.trim(), it.next()?.trim());
                match (
                    NaiveDate::parse_from_str(s, "%Y%m%d"),
                    NaiveDate::parse_from_str(e, "%Y%m%d"),
                ) {
                    (Ok(sd), Ok(ed)) if ed > watermark => Some((sd, ed)),
                    _ => None,
                }
            })
            .collect();
        spans.sort_by_key(|(s, _)| *s);
        // Merge adjacent (end + 1 day == next start) or overlapping spans so the
        // caller subtracts one contiguous block, never a sliver-fragmented set.
        let mut merged: Vec<(NaiveDate, NaiveDate)> = Vec::new();
        for (s, e) in spans {
            match merged.last_mut() {
                Some((_, prev_end)) if s <= prev_end.succ_opt().unwrap_or(*prev_end) => {
                    if e > *prev_end {
                        *prev_end = e;
                    }
                }
                _ => merged.push((s, e)),
            }
        }
        merged
    }

    /// Whether a `(instrument, bar type)` is marked basis-shifted.
    pub fn is_shifted(&self, instrument: &str, bar_type: &str) -> bool {
        self.shifted
            .contains_key(&Self::watermark_key(instrument, bar_type))
    }

    /// The detection date (`YYYYMMDD`) a `(instrument, bar type)` was marked
    /// shifted, if it is.
    pub fn shifted_detected(&self, instrument: &str, bar_type: &str) -> Option<&str> {
        self.shifted
            .get(&Self::watermark_key(instrument, bar_type))
            .map(String::as_str)
    }

    /// Mark a `(instrument, bar type)` basis-shifted as of `detected` with its
    /// `origin` (KTD-2 step one / KTD5 — saved durably BEFORE any delete). Marking
    /// an already-marked triple keeps the original detection date AND origin (a
    /// re-entry, or an epoch re-base over an already-heal-marked series, must not
    /// rewrite history). Origin is written together with a NEW mark and only then,
    /// so the two maps can never diverge — a legacy mark with no origin stays
    /// unknown across a re-mark.
    pub fn mark_shifted(
        &mut self,
        instrument: &str,
        bar_type: &str,
        detected: NaiveDate,
        origin: RebaseOrigin,
    ) {
        let key = Self::watermark_key(instrument, bar_type);
        if self.shifted.contains_key(&key) {
            // Keep-original-on-re-mark: preserve both the detection date and the
            // stored origin (or its absence → unknown).
            return;
        }
        self.shifted.insert(key.clone(), detected.format("%Y%m%d").to_string());
        self.shifted_origin.insert(key, origin);
    }

    /// The origin of a `(instrument, bar type)`'s shifted mark (KTD5). An absent
    /// key reads as [`RebaseOrigin::Unknown`] (legacy marks + unmarked series).
    pub fn shifted_origin(&self, instrument: &str, bar_type: &str) -> RebaseOrigin {
        self.shifted_origin
            .get(&Self::watermark_key(instrument, bar_type))
            .copied()
            .unwrap_or_default()
    }

    /// Clear the shifted mark for a `(instrument, bar type)` (heal completion) —
    /// both the mark and its origin, in lock-step (KTD5).
    pub fn clear_shifted(&mut self, instrument: &str, bar_type: &str) {
        let key = Self::watermark_key(instrument, bar_type);
        self.shifted.remove(&key);
        self.shifted_origin.remove(&key);
    }

    /// The instruments currently marked shifted for `bar_type`, in key order
    /// (the lab's runner intersects this with a run's selected symbols, R7).
    pub fn shifted_instruments(&self, bar_type: &str) -> Vec<String> {
        let suffix = format!("|{bar_type}");
        self.shifted
            .keys()
            .filter_map(|k| k.strip_suffix(&suffix))
            .map(str::to_string)
            .collect()
    }

    /// Record a completed re-base, bounded per series (R5/R9, U6/KTD7). Appends the
    /// event, then — while this `(instrument, bar type)` series exceeds
    /// [`REBASE_EVENTS_PER_SERIES_CAP`] — evicts its **oldest** event (lowest index
    /// = earliest recorded, regardless of origin) and increments the origin-split
    /// evicted counter, so [`Self::rebase_origin_totals`] stays whole across
    /// eviction. Other series are untouched.
    pub fn record_rebase_event(&mut self, event: RebaseEvent) {
        let instrument = event.instrument.clone();
        let bar_type = event.bar_type.clone();
        self.rebase_events.push(event);
        // One event is appended per call, so this series can exceed the cap by at
        // most one. If it does, evict its oldest event (first matching index =
        // earliest recorded, regardless of origin) and fold it into the origin-split
        // counters so the audit totals stay whole across eviction (R9).
        let over_cap = self
            .rebase_events
            .iter()
            .filter(|e| e.instrument == instrument && e.bar_type == bar_type)
            .count()
            > REBASE_EVENTS_PER_SERIES_CAP;
        if over_cap {
            if let Some(oldest) = self
                .rebase_events
                .iter()
                .position(|e| e.instrument == instrument && e.bar_type == bar_type)
            {
                let evicted = self.rebase_events.remove(oldest);
                *self
                    .rebase_evicted
                    .entry(evicted.origin.as_key().to_string())
                    .or_insert(0) += 1;
            }
        }
    }

    /// The recorded re-base events, in append order.
    pub fn rebase_events(&self) -> &[RebaseEvent] {
        &self.rebase_events
    }

    /// The per-origin re-base totals (KTD6/R8): live event rows plus the evicted
    /// counters, so the operator's audit metric survives the per-series cap
    /// eviction. `.organic()` on the result excludes epoch-origin events. This is
    /// the single accessor the README's audit-trail sentence points at.
    pub fn rebase_origin_totals(&self) -> RebaseOriginTotals {
        let mut totals = RebaseOriginTotals::default();
        for e in &self.rebase_events {
            match e.origin {
                RebaseOrigin::Heal => totals.heal += 1,
                RebaseOrigin::Epoch => totals.epoch += 1,
                RebaseOrigin::Unknown => totals.unknown += 1,
            }
        }
        totals.heal += self.rebase_evicted.get(RebaseOrigin::Heal.as_key()).copied().unwrap_or(0);
        totals.epoch += self.rebase_evicted.get(RebaseOrigin::Epoch.as_key()).copied().unwrap_or(0);
        totals.unknown += self.rebase_evicted.get(RebaseOrigin::Unknown.as_key()).copied().unwrap_or(0);
        totals
    }

    /// Prune `completed` keys and `gaps` whose range end date is at or below the
    /// instrument's watermark (KTD7) — so daily accumulate runs, which rewrite the
    /// checkpoint each triple, do not grow the per-triple set without bound.
    pub fn prune_below_watermarks(&mut self) {
        let below = |instrument: &str, bar_type: &str, range: &str| -> bool {
            let edate = range.split("..").nth(1).unwrap_or("").trim();
            match (
                self.watermarks.get(&Self::watermark_key(instrument, bar_type)),
                NaiveDate::parse_from_str(edate, "%Y%m%d").ok(),
            ) {
                (Some(wm), Some(end)) => NaiveDate::parse_from_str(wm.trim(), "%Y%m%d")
                    .map(|w| end <= w)
                    .unwrap_or(false),
                _ => false,
            }
        };
        self.gaps
            .retain(|g| !below(&g.instrument, &g.bar_type, &g.range));
        let stale: Vec<String> = self
            .completed
            .iter()
            .filter(|k| {
                let parts: Vec<&str> = k.splitn(3, '|').collect();
                parts.len() == 3 && below(parts[0], parts[1], parts[2])
            })
            .cloned()
            .collect();
        for k in stale {
            self.completed.remove(&k);
        }
    }

    /// Migrate legacy `completed` ranges to `watermarks` (U2/KTD-3, R1/R2/R3), a
    /// pure in-memory transform run on load. For each `(instrument, bar type)` with
    /// **no existing watermark**, its `completed` range keys are sorted by start
    /// date and chained while no trading weekday lies strictly between the chain's
    /// running-maximum `edate` and the next range's start (the running-max
    /// comparison, so a contained range chains trivially). A `PaperThin`-gapped
    /// range terminates the chain before it (a truncated fetch left un-fetched
    /// history — deriving past it would silently gap, R2). The derived watermark is
    /// the chain's max `edate`, trusted as attested (the same trust
    /// [`Self::prune_below_watermarks`] applies). Ranges beyond the hole stay in
    /// `completed` and surface as a [`MigrationRemainder`]. Existing watermarks are
    /// never overridden, so a double-load derives nothing new (R3). Returns the
    /// per-triple remainders for the caller to warn about.
    ///
    /// The un-gated entry point is Legacy: the weekday hole test
    /// ([`weekday_strictly_between`]) stays authoritative, so behavior is byte-identical to the
    /// pre-migration path for every existing caller/test.
    pub fn migrate_completed_watermarks(&mut self) -> Vec<MigrationRemainder> {
        self.migrate_completed_watermarks_gated(&CalendarGate::legacy())
    }

    /// Migrate legacy `completed` ranges to `watermarks` under an injected [`CalendarGate`]
    /// (U10/KTD8). Identical to [`migrate_completed_watermarks`](Self::migrate_completed_watermarks)
    /// except the merge-hole test is routed through the calendar seam:
    ///
    /// - **Legacy** — the weekday hole test ([`weekday_strictly_between`]) decides (a weekday in
    ///   the gap breaks the chain).
    /// - **Shadow** — the weekday hole test still decides (byte-identical to Legacy), but the
    ///   calendar continuity verdict is RECORDED to the non-persisted diagnostic channel.
    /// - **Enforced** — the calendar decides: ranges merge ONLY when every intervening date is a
    ///   proven Closed date; a proven Trading Session in the gap breaks the chain (un-attested
    ///   history), and Unknown/unavailable evidence breaks it too (conservative over-fetch, with
    ///   a diagnostic) so newly-resolved evidence can re-chain the ranges on a later load.
    pub fn migrate_completed_watermarks_gated(
        &mut self,
        gate: &CalendarGate,
    ) -> Vec<MigrationRemainder> {
        // The merge-hole test under the adoption seam: `true` breaks the chain (keeps the far
        // range in `completed`). Legacy/Shadow keep the weekday result authoritative; Enforced
        // acts on the calendar continuity verdict.
        let hole_breaks = |cm: NaiveDate, sd: NaiveDate| -> bool {
            match gate.adoption() {
                CalendarAdoption::Legacy => weekday_strictly_between(cm, sd),
                CalendarAdoption::Shadow => {
                    let decision = gate.continuity_decision(cm, sd);
                    let weekday = weekday_strictly_between(cm, sd);
                    tracing::info!(
                        after = %cm.format("%Y%m%d"),
                        before = %sd.format("%Y%m%d"),
                        calendar_decision = ?decision,
                        weekday_breaks = weekday,
                        adoption = "shadow",
                        "calendar shadow checkpoint continuity verdict (recorded; weekday hole test authoritative)"
                    );
                    weekday
                }
                CalendarAdoption::Enforced => {
                    let freshness = gate.full_history_freshness();
                    if freshness != Some(DimensionStaleness::Fresh) {
                        tracing::warn!(
                            after = %cm.format("%Y%m%d"),
                            before = %sd.format("%Y%m%d"),
                            full_history_freshness = ?freshness,
                            "checkpoint continuity lacks fresh full-history evidence; keeping the ranges separate (conservative over-fetch)"
                        );
                        return true;
                    }
                    let decision = gate.continuity_decision(cm, sd);
                    if matches!(decision, ContinuityDecision::Indeterminate) {
                        tracing::warn!(
                            after = %cm.format("%Y%m%d"),
                            before = %sd.format("%Y%m%d"),
                            "checkpoint continuity INDETERMINATE: Unknown/unavailable calendar evidence in the gap and no proven Trading Session; keeping the ranges separate (conservative over-fetch) until the evidence resolves"
                        );
                    }
                    decision.breaks_chain()
                }
            }
        };
        self.migrate_completed_watermarks_inner(hole_breaks)
    }

    /// The shared migration transform (U2/U10), parameterized on the merge-hole predicate so the
    /// weekday (Legacy/Shadow) and calendar (Enforced) paths share one implementation.
    fn migrate_completed_watermarks_inner(
        &mut self,
        hole_breaks: impl Fn(NaiveDate, NaiveDate) -> bool,
    ) -> Vec<MigrationRemainder> {
        // Ranges keyed by (instrument, bar type), only for triples lacking a
        // watermark (derive into absent keys only, R3). Each range is
        // (start date, end date, raw range key).
        type Range = (NaiveDate, NaiveDate, String);
        let mut groups: BTreeMap<(String, String), Vec<Range>> = BTreeMap::new();
        for key in &self.completed {
            let parts: Vec<&str> = key.splitn(3, '|').collect();
            if parts.len() != 3 {
                continue;
            }
            let (instrument, bar_type, range) = (parts[0], parts[1], parts[2]);
            if self.watermarks.contains_key(&Self::watermark_key(instrument, bar_type)) {
                continue;
            }
            let mut it = range.split("..");
            let (s, e) = (it.next().unwrap_or("").trim(), it.next().unwrap_or("").trim());
            if let (Ok(sd), Ok(ed)) = (
                NaiveDate::parse_from_str(s, "%Y%m%d"),
                NaiveDate::parse_from_str(e, "%Y%m%d"),
            ) {
                groups
                    .entry((instrument.to_string(), bar_type.to_string()))
                    .or_default()
                    .push((sd, ed, range.to_string()));
            }
        }
        // A `PaperThin` gap records a truncated fetch: its range terminates the
        // chain before it. `EmptyHistory`/`NonTradingDay` gaps attest coverage
        // (`record_gap` also marks done) and never block derivation.
        let paper_thin: HashSet<(String, String, String)> = self
            .gaps
            .iter()
            .filter(|g| g.reason == GapReason::PaperThin)
            .map(|g| (g.instrument.clone(), g.bar_type.clone(), g.range.clone()))
            .collect();

        let mut remainders: Vec<MigrationRemainder> = Vec::new();
        let mut derived: Vec<(String, String, NaiveDate)> = Vec::new();
        for ((instrument, bar_type), mut ranges) in groups {
            ranges.sort_by_key(|(s, _, _)| *s);
            let mut chain_max: Option<NaiveDate> = None;
            let mut remainder_ranges: Vec<String> = Vec::new();
            let mut broke = false;
            for (sd, ed, raw) in &ranges {
                if broke {
                    remainder_ranges.push(raw.clone());
                    continue;
                }
                if paper_thin.contains(&(instrument.clone(), bar_type.clone(), raw.clone())) {
                    broke = true;
                    remainder_ranges.push(raw.clone());
                    continue;
                }
                match chain_max {
                    None => chain_max = Some(*ed),
                    Some(cm) => {
                        if hole_breaks(cm, *sd) {
                            broke = true;
                            remainder_ranges.push(raw.clone());
                        } else {
                            chain_max = Some(cm.max(*ed));
                        }
                    }
                }
            }
            if let Some(cm) = chain_max {
                derived.push((instrument.clone(), bar_type.clone(), cm));
            }
            if !remainder_ranges.is_empty() {
                remainders.push(MigrationRemainder {
                    instrument,
                    bar_type,
                    ranges: remainder_ranges,
                });
            }
        }
        for (instrument, bar_type, wm) in derived {
            // Never override an existing watermark (already filtered above; the
            // entry API keeps that invariant explicit and load-order independent).
            self.watermarks
                .entry(Self::watermark_key(&instrument, &bar_type))
                .or_insert_with(|| wm.format("%Y%m%d").to_string());
        }
        remainders
    }

    /// Load a checkpoint from `path`, returning an empty checkpoint if the file
    /// does not exist. A legacy `completed`-only checkpoint is migrated to
    /// `watermarks` on load (U2/KTD-3); non-contiguous remainder ranges are logged
    /// per triple, naming the escape hatch.
    ///
    /// # Errors
    ///
    /// [`AdapterError::Ingest`] if the file exists but cannot be read/parsed.
    pub fn load(path: &Path) -> AdapterResult<Self> {
        Self::load_gated(path, &CalendarGate::legacy())
    }

    /// Load a checkpoint from `path` under an injected [`CalendarGate`] (U10/KTD8): the
    /// legacy `completed`→`watermarks` migration's merge-hole test is routed through the
    /// calendar seam. Legacy (what [`load`](Self::load) injects) keeps the weekday hole test
    /// authoritative — byte-identical to the pre-migration path; Shadow records the calendar
    /// verdict; Enforced merges only fully-proven-Closed gaps.
    ///
    /// # Errors
    ///
    /// [`AdapterError::Ingest`] if the file exists but cannot be read/parsed.
    pub fn load_gated(path: &Path, gate: &CalendarGate) -> AdapterResult<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => {
                let mut cp: Self = serde_json::from_str(&s).map_err(|e| {
                    AdapterError::Ingest(format!("corrupt checkpoint {}: {e}", path.display()))
                })?;
                for rem in cp.migrate_completed_watermarks_gated(gate) {
                    tracing::warn!(
                        instrument = %rem.instrument,
                        bar_type = %rem.bar_type,
                        ranges = ?rem.ranges,
                        "legacy checkpoint migration left non-contiguous `completed` ranges beyond a coverage hole; they were NOT folded into the watermark — recover them with a fresh catalog at the wider lookback, or wipe + full re-pull"
                    );
                }
                Ok(cp)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Checkpoint::default()),
            Err(e) => Err(AdapterError::Ingest(format!(
                "cannot read checkpoint {}: {e}",
                path.display()
            ))),
        }
    }

    /// Persist the checkpoint to `path` (pretty JSON).
    ///
    /// # Errors
    ///
    /// [`AdapterError::Ingest`] on a write/serialize failure.
    pub fn save(&self, path: &Path) -> AdapterResult<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| AdapterError::Ingest(format!("serialize checkpoint: {e}")))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AdapterError::Ingest(format!("mkdir {}: {e}", parent.display())))?;
        }
        // Atomic write: a crash mid-write must never corrupt the live checkpoint
        // (the per-triple save is the crash-safety seam — an in-place `write` that
        // died half-way would lose the WHOLE file, not just the in-flight triple).
        // Write a sibling temp file, then rename over the target (atomic on the same
        // filesystem).
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)
            .map_err(|e| AdapterError::Ingest(format!("write checkpoint tmp {}: {e}", tmp.display())))?;
        std::fs::rename(&tmp, path)
            .map_err(|e| AdapterError::Ingest(format!("commit checkpoint {}: {e}", path.display())))
    }

    /// Whether a triple is already done.
    pub fn is_done(&self, instrument: &str, bar_type: &str, range: &str) -> bool {
        self.completed.contains(&Self::key(instrument, bar_type, range))
    }

    /// Mark a triple done.
    pub fn mark_done(&mut self, instrument: &str, bar_type: &str, range: &str) {
        self.completed.insert(Self::key(instrument, bar_type, range));
    }

    /// Record a coverage gap and mark the triple done (so a re-run skips it rather
    /// than refetching a known-empty feed).
    pub fn record_gap(&mut self, instrument: &str, bar_type: &str, range: &str, reason: GapReason) {
        self.mark_done(instrument, bar_type, range);
        self.gaps.push(CoverageGap {
            instrument: instrument.to_string(),
            bar_type: bar_type.to_string(),
            range: range.to_string(),
            reason,
        });
    }

    /// The recorded coverage gaps.
    pub fn gaps(&self) -> &[CoverageGap] {
        &self.gaps
    }

    /// The number of completed triples.
    pub fn completed_count(&self) -> usize {
        self.completed.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn missing_file_loads_empty() {
        let dir = tempdir().unwrap();
        let cp = Checkpoint::load(&dir.path().join("nope.json")).unwrap();
        assert_eq!(cp.completed_count(), 0);
    }

    #[test]
    fn watermarks_set_get_and_advance() {
        let mut cp = Checkpoint::default();
        assert!(cp.watermark("005930.XKRX", "1-DAY").is_none());
        cp.set_watermark("005930.XKRX", "1-DAY", NaiveDate::from_ymd_opt(2024, 1, 5).unwrap());
        assert_eq!(
            cp.watermark("005930.XKRX", "1-DAY"),
            Some(NaiveDate::from_ymd_opt(2024, 1, 5).unwrap())
        );
        // Different bar kind is a distinct watermark.
        assert!(cp.watermark("005930.XKRX", "1-MINUTE").is_none());
    }

    #[test]
    fn legacy_checkpoint_without_watermarks_migrates_on_load() {
        // A pre-U5 checkpoint file has no `watermarks` field. On load, the U2/KTD-3
        // migration derives the watermark from the covered `completed` range — the
        // deliberate behavior change this package ships (the assertion below flipped
        // from the pre-migration "no watermark derived yet").
        let dir = tempdir().unwrap();
        let path = dir.path().join("legacy.json");
        std::fs::write(
            &path,
            r#"{"completed":["005930.XKRX|1-DAY|20240101..20240105"],"gaps":[],"adjusted_prices":true}"#,
        )
        .unwrap();
        let cp = Checkpoint::load(&path).expect("legacy checkpoint loads");
        assert!(cp.is_done("005930.XKRX", "1-DAY", "20240101..20240105"));
        assert_eq!(
            cp.watermark("005930.XKRX", "1-DAY"),
            Some(NaiveDate::from_ymd_opt(2024, 1, 5).unwrap()),
            "the covered completed range migrates to a watermark on load"
        );
        assert!(cp.adjusted_prices);
    }

    #[test]
    fn history_floor_set_get_and_round_trips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut cp = Checkpoint::default();
        assert!(cp.history_floor("005930.XKRX", "1-DAY").is_none(), "unset floor reads none");
        cp.set_history_floor("005930.XKRX", "1-DAY", ymd(2024, 6, 1));
        assert_eq!(cp.history_floor("005930.XKRX", "1-DAY"), Some(ymd(2024, 6, 1)));
        // A deeper floor overwrites; a distinct series is independent.
        cp.set_history_floor("005930.XKRX", "1-DAY", ymd(2024, 5, 25));
        assert_eq!(cp.history_floor("005930.XKRX", "1-DAY"), Some(ymd(2024, 5, 25)));
        assert!(cp.history_floor("005930.XKRX", "1-MINUTE").is_none(), "distinct bar kind is a distinct floor");

        cp.save(&path).unwrap();
        let a = std::fs::read_to_string(&path).unwrap();
        cp.save(&path).unwrap();
        let b = std::fs::read_to_string(&path).unwrap();
        assert_eq!(a, b, "the marker serializes deterministically (byte-identical double-save)");
        let loaded = Checkpoint::load(&path).unwrap();
        assert_eq!(
            loaded.history_floor("005930.XKRX", "1-DAY"),
            Some(ymd(2024, 5, 25)),
            "the marker round-trips through save/load"
        );
    }

    #[test]
    fn legacy_checkpoint_without_history_floors_loads_as_first_seen() {
        // A pre-U3 checkpoint file has no `history_floors` field: it loads with an
        // empty map, so every triple reads as never-warned (first-seen).
        let dir = tempdir().unwrap();
        let path = dir.path().join("legacy.json");
        std::fs::write(
            &path,
            r#"{"completed":[],"gaps":[],"watermarks":{"005930.XKRX|1-DAY":"20240703"},"adjusted_prices":true}"#,
        )
        .unwrap();
        let cp = Checkpoint::load(&path).expect("legacy checkpoint loads");
        assert!(cp.history_floor("005930.XKRX", "1-DAY").is_none(), "absent field → first-seen");
    }

    #[test]
    fn prune_drops_completed_and_gaps_below_watermark() {
        let mut cp = Checkpoint::default();
        cp.mark_done("005930.XKRX", "1-DAY", "20240101..20240105");
        cp.record_gap("000660.XKRX", "1-DAY", "20240101..20240105", GapReason::EmptyHistory);
        // Advance both instruments' watermarks past those ranges' end date.
        cp.set_watermark("005930.XKRX", "1-DAY", NaiveDate::from_ymd_opt(2024, 1, 10).unwrap());
        cp.set_watermark("000660.XKRX", "1-DAY", NaiveDate::from_ymd_opt(2024, 1, 10).unwrap());
        cp.prune_below_watermarks();
        assert!(!cp.is_done("005930.XKRX", "1-DAY", "20240101..20240105"), "completed pruned below watermark");
        assert!(cp.gaps().is_empty(), "gap row pruned below watermark");
        // A range ending AFTER the watermark is retained.
        cp.mark_done("005930.XKRX", "1-DAY", "20240108..20240115");
        cp.prune_below_watermarks();
        assert!(cp.is_done("005930.XKRX", "1-DAY", "20240108..20240115"), "future range kept");
    }

    #[test]
    fn legacy_checkpoint_without_heal_fields_loads() {
        // A pre-heal checkpoint file has neither `shifted` nor `rebase_events`.
        let dir = tempdir().unwrap();
        let path = dir.path().join("legacy.json");
        std::fs::write(
            &path,
            r#"{"completed":[],"gaps":[],"watermarks":{"005930.XKRX|1-DAY":"20240105"},"adjusted_prices":true}"#,
        )
        .unwrap();
        let cp = Checkpoint::load(&path).expect("legacy checkpoint loads");
        assert!(!cp.is_shifted("005930.XKRX", "1-DAY"));
        assert!(cp.rebase_events().is_empty());
        assert!(cp.watermark("005930.XKRX", "1-DAY").is_some());
    }

    #[test]
    fn shifted_mark_round_trips_and_keeps_original_detection_date() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut cp = Checkpoint::default();
        cp.mark_shifted("005930.XKRX", "1-DAY", NaiveDate::from_ymd_opt(2024, 1, 5).unwrap(), RebaseOrigin::Heal);
        // Re-marking on heal re-entry (here as an epoch re-base) must not rewrite
        // the detection date OR the origin (keep-original-on-re-mark, KTD5/AE4).
        cp.mark_shifted("005930.XKRX", "1-DAY", NaiveDate::from_ymd_opt(2024, 1, 8).unwrap(), RebaseOrigin::Epoch);
        cp.save(&path).unwrap();

        let mut loaded = Checkpoint::load(&path).unwrap();
        assert!(loaded.is_shifted("005930.XKRX", "1-DAY"));
        assert_eq!(loaded.shifted_detected("005930.XKRX", "1-DAY"), Some("20240105"));
        assert_eq!(loaded.shifted_origin("005930.XKRX", "1-DAY"), RebaseOrigin::Heal, "re-mark keeps the original origin");
        assert_eq!(loaded.shifted_instruments("1-DAY"), vec!["005930.XKRX".to_string()]);
        assert!(loaded.shifted_instruments("1-MINUTE").is_empty());
        loaded.clear_shifted("005930.XKRX", "1-DAY");
        assert!(!loaded.is_shifted("005930.XKRX", "1-DAY"));
        assert_eq!(loaded.shifted_origin("005930.XKRX", "1-DAY"), RebaseOrigin::Unknown, "cleared origin reads unknown");
    }

    #[test]
    fn clearing_a_watermark_keeps_not_yet_recovered_rows_on_prune() {
        let mut cp = Checkpoint::default();
        cp.mark_done("005930.XKRX", "1-DAY", "20240101..20240105");
        cp.record_gap("005930.XKRX", "1-DAY", "20240102..20240103", GapReason::EmptyHistory);
        cp.set_watermark("005930.XKRX", "1-DAY", NaiveDate::from_ymd_opt(2024, 1, 10).unwrap());
        cp.clear_watermark("005930.XKRX", "1-DAY");
        assert!(cp.watermark("005930.XKRX", "1-DAY").is_none());
        cp.prune_below_watermarks();
        // With the watermark cleared, nothing counts as "below" it — the wiped
        // symbol's rows survive until the re-pull re-covers them.
        assert!(cp.is_done("005930.XKRX", "1-DAY", "20240101..20240105"));
        assert_eq!(cp.gaps().len(), 1, "gap row retained");
    }

    #[test]
    fn rebase_events_append_in_order_and_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut cp = Checkpoint::default();
        for (i, sym) in ["005930.XKRX", "000660.XKRX"].iter().enumerate() {
            cp.record_rebase_event(RebaseEvent {
                instrument: sym.to_string(),
                bar_type: "1-DAY".to_string(),
                detected: format!("2024010{}", i + 1),
                healed: format!("2024010{}", i + 2),
                origin: RebaseOrigin::Heal,
            });
        }
        cp.save(&path).unwrap();
        let a = std::fs::read_to_string(&path).unwrap();
        cp.save(&path).unwrap();
        let b = std::fs::read_to_string(&path).unwrap();
        assert_eq!(a, b, "serialization is deterministic");

        let loaded = Checkpoint::load(&path).unwrap();
        assert_eq!(loaded.rebase_events().len(), 2);
        assert_eq!(loaded.rebase_events()[0].instrument, "005930.XKRX");
        assert_eq!(loaded.rebase_events()[1].instrument, "000660.XKRX");
        assert_eq!(loaded.rebase_events()[0].detected, "20240101");
        assert_eq!(loaded.rebase_events()[0].healed, "20240102");
        assert_eq!(loaded.rebase_events()[0].origin, RebaseOrigin::Heal, "origin round-trips");
    }

    fn rebase_event(instrument: &str, healed: &str, origin: RebaseOrigin) -> RebaseEvent {
        RebaseEvent {
            instrument: instrument.to_string(),
            bar_type: "1-DAY".to_string(),
            detected: "20240101".to_string(),
            healed: healed.to_string(),
            origin,
        }
    }

    /// U5/AE5 origin half: a checkpoint written before origin tracking (raw JSON
    /// with no `origin`, `shifted_origin`, or `rebase_evicted` fields) loads with
    /// unknown origin and is counted organic.
    #[test]
    fn legacy_checkpoint_without_origin_fields_loads_as_unknown() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("legacy.json");
        std::fs::write(
            &path,
            r#"{"completed":[],"gaps":[],"watermarks":{},"shifted":{"005930.XKRX|1-DAY":"20240105"},
                "rebase_events":[{"instrument":"005930.XKRX","bar_type":"1-DAY","detected":"20240101","healed":"20240102"}],
                "adjusted_prices":true}"#,
        )
        .unwrap();
        let cp = Checkpoint::load(&path).expect("legacy checkpoint loads");
        // The legacy shifted mark reads as unknown origin.
        assert_eq!(cp.shifted_origin("005930.XKRX", "1-DAY"), RebaseOrigin::Unknown);
        // The legacy re-base row reads as unknown, and unknown is presumed organic.
        let totals = cp.rebase_origin_totals();
        assert_eq!(totals.unknown, 1);
        assert_eq!(totals.heal, 0);
        assert_eq!(totals.epoch, 0);
        assert_eq!(totals.organic(), 1, "unknown rows are presumed organic");
    }

    /// U5/R8: the organic metric excludes epoch-origin events.
    #[test]
    fn origin_totals_exclude_epoch_from_organic() {
        let mut cp = Checkpoint::default();
        cp.record_rebase_event(rebase_event("005930.XKRX", "20240102", RebaseOrigin::Heal));
        cp.record_rebase_event(rebase_event("000660.XKRX", "20240102", RebaseOrigin::Epoch));
        cp.record_rebase_event(rebase_event("000810.XKRX", "20240102", RebaseOrigin::Unknown));
        let totals = cp.rebase_origin_totals();
        assert_eq!(totals.heal, 1);
        assert_eq!(totals.epoch, 1);
        assert_eq!(totals.unknown, 1);
        assert_eq!(totals.organic(), 2, "organic = heal + unknown, never epoch");
    }

    /// U6/AE5 cap half: a fifth event on one series evicts the oldest and bumps its
    /// origin's evicted counter; the organic total is unchanged by eviction.
    #[test]
    fn per_series_cap_evicts_oldest_and_preserves_totals() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut cp = Checkpoint::default();
        // 5 heal events on one series (cap is 4).
        for i in 0..5 {
            cp.record_rebase_event(rebase_event("005930.XKRX", &format!("2024010{}", i + 2), RebaseOrigin::Heal));
        }
        assert_eq!(cp.rebase_events().len(), 4, "the series is capped at 4 retained events");
        // The oldest (healed 20240102) was evicted; the newest four remain.
        assert_eq!(cp.rebase_events()[0].healed, "20240103", "oldest evicted, order preserved");
        // The audit total is eviction-stable: 4 live + 1 evicted = 5 organic.
        let totals = cp.rebase_origin_totals();
        assert_eq!(totals.heal, 5, "evicted heal rows still count via the counter");
        assert_eq!(totals.organic(), 5);
        // Round-trips deterministically (double-save byte-identical) with new fields.
        cp.save(&path).unwrap();
        let a = std::fs::read_to_string(&path).unwrap();
        cp.save(&path).unwrap();
        let b = std::fs::read_to_string(&path).unwrap();
        assert_eq!(a, b, "serialization stays deterministic over origin + evicted fields");
        let loaded = Checkpoint::load(&path).unwrap();
        assert_eq!(loaded.rebase_origin_totals().organic(), 5, "totals survive save/load");
    }

    /// U6: eviction is strictly oldest-first regardless of origin, and the cap is
    /// per series (a second series is untouched).
    #[test]
    fn cap_evicts_strictly_oldest_and_is_per_series() {
        let mut cp = Checkpoint::default();
        // Series A: epoch, then 4 heals → 5 total, oldest (epoch) evicted.
        cp.record_rebase_event(rebase_event("005930.XKRX", "20240102", RebaseOrigin::Epoch));
        for i in 0..4 {
            cp.record_rebase_event(rebase_event("005930.XKRX", &format!("2024010{}", i + 3), RebaseOrigin::Heal));
        }
        // Series B: 2 heals — below the cap, untouched.
        cp.record_rebase_event(rebase_event("000660.XKRX", "20240102", RebaseOrigin::Heal));
        cp.record_rebase_event(rebase_event("000660.XKRX", "20240103", RebaseOrigin::Heal));

        let a_count = cp.rebase_events().iter().filter(|e| e.instrument == "005930.XKRX").count();
        let b_count = cp.rebase_events().iter().filter(|e| e.instrument == "000660.XKRX").count();
        assert_eq!(a_count, 4, "series A capped at 4");
        assert_eq!(b_count, 2, "series B below cap is untouched");
        let totals = cp.rebase_origin_totals();
        assert_eq!(totals.epoch, 1, "the evicted epoch row is preserved in the counter");
        assert_eq!(totals.heal, 6, "4 (A) + 2 (B) live heals");
        assert_eq!(totals.organic(), 6, "organic excludes the evicted epoch row");
    }

    /// U6: the evicted-counter + origin fields default to zero/unknown on a legacy
    /// load and do not perturb determinism.
    #[test]
    fn evicted_counters_default_on_legacy_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("legacy.json");
        std::fs::write(&path, r#"{"completed":[],"gaps":[],"adjusted_prices":false}"#).unwrap();
        let cp = Checkpoint::load(&path).unwrap();
        let totals = cp.rebase_origin_totals();
        assert_eq!(totals, RebaseOriginTotals::default(), "no events, no evicted counters");
    }

    // --- U2/KTD-3: completed→watermarks migration ---

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn migration_derives_watermark_from_a_covered_range() {
        let mut cp = Checkpoint::default();
        cp.mark_done("005930.XKRX", "1-DAY", "20240101..20240105");
        let rem = cp.migrate_completed_watermarks();
        assert!(rem.is_empty(), "a single covered range has no remainder");
        assert_eq!(cp.watermark("005930.XKRX", "1-DAY"), Some(ymd(2024, 1, 5)));
    }

    #[test]
    fn migration_gap_reasons_discriminate_coverage() {
        // EmptyHistory + NonTradingDay attest coverage (chain like a bars range); a
        // PaperThin range terminates the chain BEFORE it (un-fetched history).
        let mut cp = Checkpoint::default();
        // A bars-attested range then an EmptyHistory-gapped contiguous range.
        cp.mark_done("A.XKRX", "1-DAY", "20240101..20240103");
        cp.record_gap("A.XKRX", "1-DAY", "20240104..20240105", GapReason::EmptyHistory);
        // A separate triple with a NonTradingDay gap only.
        cp.record_gap("B.XKRX", "1-DAY", "20240101..20240105", GapReason::NonTradingDay);
        // A triple whose second range is a PaperThin (truncated) fetch.
        cp.mark_done("C.XKRX", "1-DAY", "20240101..20240105");
        cp.record_gap("C.XKRX", "1-DAY", "20240108..20240112", GapReason::PaperThin);

        let rem = cp.migrate_completed_watermarks();
        assert_eq!(cp.watermark("A.XKRX", "1-DAY"), Some(ymd(2024, 1, 5)), "EmptyHistory attests");
        assert_eq!(cp.watermark("B.XKRX", "1-DAY"), Some(ymd(2024, 1, 5)), "NonTradingDay attests");
        assert_eq!(
            cp.watermark("C.XKRX", "1-DAY"),
            Some(ymd(2024, 1, 5)),
            "the chain stops BEFORE the PaperThin range — never at or past it"
        );
        let c_rem = rem.iter().find(|r| r.instrument == "C.XKRX").expect("C has a remainder");
        assert_eq!(c_rem.ranges, vec!["20240108..20240112".to_string()], "the PaperThin range stays in completed");
    }

    #[test]
    fn migration_chains_across_a_weekend_but_breaks_on_a_weekday_hole() {
        // Two ranges separated only by a weekend chain into one watermark.
        let mut weekend = Checkpoint::default();
        weekend.mark_done("005930.XKRX", "1-DAY", "20240101..20240105"); // Mon..Fri
        weekend.mark_done("005930.XKRX", "1-DAY", "20240108..20240112"); // next Mon..Fri
        let rem = weekend.migrate_completed_watermarks();
        assert!(rem.is_empty(), "a weekend-only gap is contiguous");
        assert_eq!(weekend.watermark("005930.XKRX", "1-DAY"), Some(ymd(2024, 1, 12)));

        // A range straddling an intervening weekday (Jan 8/9) derives the prefix only.
        let mut hole = Checkpoint::default();
        hole.mark_done("005930.XKRX", "1-DAY", "20240101..20240105"); // Mon..Fri
        hole.mark_done("005930.XKRX", "1-DAY", "20240110..20240112"); // Wed..Fri (Mon/Tue un-covered)
        let rem = hole.migrate_completed_watermarks();
        assert_eq!(hole.watermark("005930.XKRX", "1-DAY"), Some(ymd(2024, 1, 5)), "prefix watermark only");
        assert_eq!(rem.len(), 1);
        assert_eq!(rem[0].ranges, vec!["20240110..20240112".to_string()]);
        // The remainder key stays in `completed`.
        assert!(hole.is_done("005930.XKRX", "1-DAY", "20240110..20240112"));
    }

    #[test]
    fn migration_report_entry_names_the_escape_hatch_via_load() {
        // The load-time warning path: a hole-straddling checkpoint file surfaces the
        // remainder (the load logs it; here we assert the derivation result directly).
        let dir = tempdir().unwrap();
        let path = dir.path().join("legacy.json");
        std::fs::write(
            &path,
            r#"{"completed":["005930.XKRX|1-DAY|20240101..20240105","005930.XKRX|1-DAY|20240110..20240112"],"gaps":[],"adjusted_prices":true}"#,
        )
        .unwrap();
        let cp = Checkpoint::load(&path).unwrap();
        assert_eq!(cp.watermark("005930.XKRX", "1-DAY"), Some(ymd(2024, 1, 5)));
        assert!(cp.is_done("005930.XKRX", "1-DAY", "20240110..20240112"), "remainder kept in completed");
    }

    #[test]
    fn migration_contained_range_chains_via_running_maximum() {
        // A contained range (0618..0703 inside 0601..0703) chains — the hole test
        // compares the next start against the running chain MAX edate, not the
        // adjacent sorted pair.
        let mut cp = Checkpoint::default();
        cp.mark_done("005930.XKRX", "1-DAY", "20240601..20240703");
        cp.mark_done("005930.XKRX", "1-DAY", "20240618..20240703");
        let rem = cp.migrate_completed_watermarks();
        assert!(rem.is_empty(), "a contained range never breaks the chain");
        assert_eq!(cp.watermark("005930.XKRX", "1-DAY"), Some(ymd(2024, 7, 3)));
    }

    #[test]
    fn migration_never_overrides_an_existing_watermark() {
        let mut cp = Checkpoint::default();
        cp.set_watermark("005930.XKRX", "1-DAY", ymd(2024, 1, 10));
        // A completed range that would derive a LATER or EARLIER watermark must not win.
        cp.mark_done("005930.XKRX", "1-DAY", "20240101..20240131");
        cp.migrate_completed_watermarks();
        assert_eq!(cp.watermark("005930.XKRX", "1-DAY"), Some(ymd(2024, 1, 10)), "existing watermark preserved");
    }

    #[test]
    fn migration_is_idempotent_across_double_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("legacy.json");
        std::fs::write(
            &path,
            r#"{"completed":["005930.XKRX|1-DAY|20240101..20240105"],"gaps":[],"adjusted_prices":true}"#,
        )
        .unwrap();
        let cp1 = Checkpoint::load(&path).unwrap();
        cp1.save(&path).unwrap();
        let a = std::fs::read_to_string(&path).unwrap();
        let cp2 = Checkpoint::load(&path).unwrap();
        cp2.save(&path).unwrap();
        let b = std::fs::read_to_string(&path).unwrap();
        assert_eq!(a, b, "double load→save is byte-identical (R3)");
        assert_eq!(cp2.watermark("005930.XKRX", "1-DAY"), Some(ymd(2024, 1, 5)));
    }

    #[test]
    fn migrated_completed_is_pruned_below_the_derived_watermark() {
        // The next accumulate's prune cleans migrated completed/gap keys at or below
        // the derived watermark.
        let mut cp = Checkpoint::default();
        cp.mark_done("005930.XKRX", "1-DAY", "20240101..20240105");
        cp.record_gap("005930.XKRX", "1-DAY", "20240102..20240103", GapReason::EmptyHistory);
        cp.migrate_completed_watermarks();
        assert_eq!(cp.watermark("005930.XKRX", "1-DAY"), Some(ymd(2024, 1, 5)));
        cp.prune_below_watermarks();
        assert!(!cp.is_done("005930.XKRX", "1-DAY", "20240101..20240105"), "migrated completed pruned");
        assert!(cp.gaps().is_empty(), "migrated gap pruned");
    }

    #[test]
    fn migration_failure_inversion_never_derives_past_a_hole() {
        // Failure-inversion (Success Criteria): a non-contiguous fixture must NOT
        // derive a watermark at or past the hole — an over-derivation bug fails HERE
        // rather than silently gapping un-fetched history.
        let mut cp = Checkpoint::default();
        cp.mark_done("005930.XKRX", "1-DAY", "20240101..20240105"); // Mon..Fri
        cp.mark_done("005930.XKRX", "1-DAY", "20240110..20240112"); // hole: Mon/Tue un-covered
        cp.migrate_completed_watermarks();
        let wm = cp.watermark("005930.XKRX", "1-DAY").unwrap();
        assert!(wm < ymd(2024, 1, 10), "watermark stays before the hole, got {wm}");
    }

    #[test]
    fn round_trips_completed_and_gaps() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut cp = Checkpoint::default();
        cp.mark_done("005930.XKRX", "1-DAY", "20240101..20241231");
        cp.record_gap("000660.XKRX", "1-MINUTE", "20240101..20240105", GapReason::EmptyHistory);
        cp.adjusted_prices = true;
        cp.save(&path).unwrap();

        let loaded = Checkpoint::load(&path).unwrap();
        assert!(loaded.is_done("005930.XKRX", "1-DAY", "20240101..20241231"));
        assert!(loaded.is_done("000660.XKRX", "1-MINUTE", "20240101..20240105")); // gap marks done
        assert_eq!(loaded.gaps().len(), 1);
        assert_eq!(loaded.gaps()[0].reason, GapReason::EmptyHistory);
        assert!(loaded.adjusted_prices);
    }
}
