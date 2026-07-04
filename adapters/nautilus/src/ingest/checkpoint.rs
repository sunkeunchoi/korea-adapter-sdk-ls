//! Resumable ingest checkpoints (R5, AE2).
//!
//! A JSON state file beside the catalog records the completed
//! `(instrument, bar type, date range)` triples plus coverage gaps (empty history,
//! `01715`, paper-thin feeds), so an interrupted or repeated run **skips and
//! reports** rather than refetching. The checkpoint is written after each triple
//! completes, so a crash loses at most the in-flight triple.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::error::{AdapterError, AdapterResult};

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

    /// Clear the coverage watermark for a `(instrument, bar type)` — the heal's
    /// wipe step (KTD-2): a wiped series must re-pull from the floor, so its
    /// watermark must not survive the wipe.
    pub fn clear_watermark(&mut self, instrument: &str, bar_type: &str) {
        self.watermarks
            .remove(&Self::watermark_key(instrument, bar_type));
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
        loop {
            let series_indices: Vec<usize> = self
                .rebase_events
                .iter()
                .enumerate()
                .filter(|(_, e)| e.instrument == instrument && e.bar_type == bar_type)
                .map(|(i, _)| i)
                .collect();
            if series_indices.len() <= REBASE_EVENTS_PER_SERIES_CAP {
                break;
            }
            let evicted = self.rebase_events.remove(series_indices[0]);
            *self
                .rebase_evicted
                .entry(evicted.origin.as_key().to_string())
                .or_insert(0) += 1;
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

    /// Load a checkpoint from `path`, returning an empty checkpoint if the file
    /// does not exist.
    ///
    /// # Errors
    ///
    /// [`AdapterError::Ingest`] if the file exists but cannot be read/parsed.
    pub fn load(path: &Path) -> AdapterResult<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s)
                .map_err(|e| AdapterError::Ingest(format!("corrupt checkpoint {}: {e}", path.display()))),
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
    fn legacy_checkpoint_without_watermarks_loads() {
        // A pre-U5 checkpoint file has no `watermarks` field.
        let dir = tempdir().unwrap();
        let path = dir.path().join("legacy.json");
        std::fs::write(
            &path,
            r#"{"completed":["005930.XKRX|1-DAY|20240101..20240105"],"gaps":[],"adjusted_prices":true}"#,
        )
        .unwrap();
        let cp = Checkpoint::load(&path).expect("legacy checkpoint loads");
        assert!(cp.is_done("005930.XKRX", "1-DAY", "20240101..20240105"));
        assert!(cp.watermark("005930.XKRX", "1-DAY").is_none(), "no watermark derived yet");
        assert!(cp.adjusted_prices);
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
