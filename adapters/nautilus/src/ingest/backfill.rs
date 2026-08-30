//! P3 windowed daily backfill — the fresh 2016-floor catalog pull (plan
//! `2026-08-13-001-feat-daily-catalog-2016-floor-pull`).
//!
//! ## Why a new mode exists at all
//!
//! Neither existing bar-writing path can acquire this catalog. Range-mode
//! seeding and accumulate's first-ever backfill both issue **one wide
//! `collect_daily` range**, and P4 measured that a wide-range `t8410` request
//! serves only the newest ~501 rows with a *clean* empty `cts_date` cursor —
//! so those paths would append about two years of bars, advance the watermark,
//! and report success over ten missing years. History is reachable only through
//! explicit calendar-snapped windows of at most
//! [`MAX_SESSIONS_PER_WINDOW`](crate::reference::pit_walk::MAX_SESSIONS_PER_WINDOW)
//! proven sessions.
//!
//! ## What this module owns
//!
//! - [`BackfillPlan`] — the manifest reader and per-symbol window planning
//!   (U1): the committed pit-universe artifact plus the calendar snapshot in,
//!   a verified per-symbol pull plan out. The full-range window plan derived
//!   locally must equal the artifact's `provenance.windows` or planning fails
//!   closed **before any call is dispatched** (R5).
//! - [`pull_window`] — one window in, trimmed verified bars out, or a loud
//!   fail-closed error (U2). This is `pit_walk::walk_window`'s termination
//!   discipline with the OHLCV kept (the walk discards it) and the ingest's
//!   parsed-timestamp trim applied.
//! - [`BackfillReport`] — the manifest-aware completeness report (U5): the
//!   rung's GO/NO-GO evidence. `catalog_status_gated`'s uniform
//!   `expected_range` form cannot be used — it applies one range to every
//!   triple and would NO-GO every one of the 108 post-floor listings.
//!
//! The write side stays the ingest's: [`crate::ingest::append_bars_checked`]'s
//! overlap refusal, the checkpoint, the per-TR pacer, the spend ledger, and the
//! `IGW00201` backoff are all reused rather than re-earned (KTD1). The mode
//! runner that drives this module lives in [`crate::ingest::Ingestor`].

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use chrono::NaiveDate;
use ls_core::LsError;
use nautilus_core::UnixNanos;
use nautilus_model::data::{Bar, BarType};
use nautilus_model::instruments::Instrument;
use nautilus_ls_calendar::AsOfView;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AdapterError, AdapterResult};
use crate::ingest::checkpoint::Checkpoint;
use crate::ingest::{
    build_daily_bar, instrument_id_for, kst_date_of, kst_to_unix_nanos, read_all_instruments,
    read_bars_scoped, stored_bar_intervals, BarKind, DailyFetcher,
};
use crate::reference::universe_metadata::MetadataPin;
use crate::reference::pit_walk::{
    partition_windows, ListingOutcome, PitUniverseArtifact, RangeSessions, WalkError, WalkWindow,
    MAX_SESSIONS_PER_WINDOW, MAX_THROTTLE_RETRIES, MAX_WALK_PAGES,
};
use crate::rules::{regular_close, SessionRegime};

/// The smallest window this planner will emit. A `sdate == edate` request is
/// degenerate on the live gateway: it **ignores `sdate`** and serves `qrycnt`
/// rows ending at `edate`, so the append collides with stored coverage (see
/// `docs/solutions/integration-issues/ls-gateway-t8410-single-day-window-ignores-sdate-append-refused.md`).
/// R1 forbids emitting one.
pub const MIN_SESSIONS_PER_WINDOW: usize = 2;

// ---------------------------------------------------------------------------
// U1 — the manifest reader
// ---------------------------------------------------------------------------

/// One manifest member's pull plan.
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolPlan {
    /// 6-digit KRX shcode.
    pub shcode: String,
    /// The symbol's pull range start: `max(floor, first_served)` (R2). A
    /// `pre_floor` member starts at the floor, a `listed` member at its first
    /// served bar.
    pub start: NaiveDate,
    /// The calendar-snapped windows to walk, oldest-first. Every window spans
    /// at least [`MIN_SESSIONS_PER_WINDOW`] proven sessions and every `edate`
    /// is a full-range window boundary (the alignment
    /// [`reconcile_watermark`] depends on).
    pub windows: Vec<WalkWindow>,
    /// Proven Trading Sessions in `[start, anchor]` — the row count the
    /// completeness report measures a shortfall against (R15).
    pub expected_sessions: usize,
}

/// The verified pull plan: a manifest plus the per-symbol windows.
#[derive(Debug, Clone)]
pub struct BackfillPlan {
    /// The pre-registered history floor the artifact was walked from.
    pub floor: NaiveDate,
    /// The frozen anchor every symbol's pull ends at (KTD3).
    pub anchor: NaiveDate,
    /// The manifest artifact path (orientation only; identity is the hash).
    pub manifest_path: String,
    /// The manifest artifact's content hash — the identity R17 pins.
    pub manifest_hash: String,
    /// Per-symbol plans, in manifest order (sorted by shcode).
    pub symbols: Vec<SymbolPlan>,
    /// Manifest members with a `no_served_rows` outcome. They have no
    /// established participation, so they are **excluded from the pull** and
    /// surfaced here — never silently dropped, and never inferred as delisted
    /// (the P4 forbidden inference).
    pub excluded_no_served_rows: Vec<String>,
    /// The full-range calendar structure the plan was derived from.
    pub range: RangeSessions,
}

impl BackfillPlan {
    /// The manifest shcodes actually planned for the pull.
    pub fn shcodes(&self) -> Vec<String> {
        self.symbols.iter().map(|s| s.shcode.clone()).collect()
    }

    /// The plan for one shcode, if it is a planned member.
    pub fn symbol(&self, shcode: &str) -> Option<&SymbolPlan> {
        self.symbols.iter().find(|s| s.shcode == shcode)
    }
}

/// The manifest artifact's content hash (the [`crate::reference::universe_metadata::UniverseMetadata::content_hash`]
/// convention): sha256 over the artifact's canonical JSON. Identity is the
/// hash, never the path.
pub fn manifest_content_hash(artifact: &PitUniverseArtifact) -> String {
    let json = serde_json::to_string(artifact).expect("artifact serializes");
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    let digest = hasher.finalize();
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Read the committed pit-universe artifact as a backfill manifest (R14).
///
/// Two artifact shapes are refused outright, because either one would describe
/// something other than the frozen universe:
///
/// - `provenance.restricted` — a re-run/repair artifact over a symbol subset.
/// - `derived: null` — an incomplete run whose participation arithmetic never
///   closed.
pub fn load_manifest(path: &Path) -> AdapterResult<PitUniverseArtifact> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| AdapterError::Ingest(format!("backfill manifest {}: {e}", path.display())))?;
    let artifact: PitUniverseArtifact = serde_json::from_str(&text).map_err(|e| {
        AdapterError::Ingest(format!("backfill manifest {}: parse: {e}", path.display()))
    })?;
    if artifact.provenance.restricted {
        return Err(AdapterError::Ingest(format!(
            "backfill manifest {} is from a RESTRICTED walk (LS_PIT_SYMBOLS) — it describes a \
             symbol subset, not the frozen pit universe; supply the full artifact",
            path.display()
        )));
    }
    if artifact.derived.is_none() {
        return Err(AdapterError::Ingest(format!(
            "backfill manifest {} carries `derived: null` — the walk was incomplete or its \
             derivation failed, so its membership is not the frozen pit universe; complete the \
             walk (or run `pit-universe-walk derive`) first",
            path.display()
        )));
    }
    if !artifact.failed.is_empty() {
        return Err(AdapterError::Ingest(format!(
            "backfill manifest {} records {} failed walks — an incomplete membership must never \
             seed a catalog; re-run the failures and merge",
            path.display(),
            artifact.failed.len()
        )));
    }
    Ok(artifact)
}

/// Derive the full-range window plan from the local calendar snapshot and
/// assert it reproduces the artifact's `provenance.windows` (R5).
///
/// The artifact's in-range windows are **immutable facts**: the machine-local
/// snapshot may have grown since the walk (new witnessed sessions, resolved
/// Unknown days), but it must still reproduce them. A mismatch fails closed
/// here — before any gateway call — rather than silently re-deriving a
/// different partition, which would move every window boundary and desynchronize
/// the watermark alignment the resume path depends on.
pub fn verify_windows(
    view: &AsOfView<'_>,
    artifact: &PitUniverseArtifact,
) -> AdapterResult<RangeSessions> {
    let floor = artifact.provenance.floor;
    let anchor = artifact.provenance.anchor;
    let range = partition_windows(view, floor, anchor, MAX_SESSIONS_PER_WINDOW)?;
    if range.windows != artifact.provenance.windows {
        return Err(AdapterError::Ingest(format!(
            "backfill window plan disagrees with the manifest's provenance: the calendar snapshot \
             now yields {} windows over [{floor}, {anchor}] ({} proven sessions) vs the artifact's \
             {} windows ({} proven sessions) — the snapshot changed since the walk. Fail closed: \
             re-walk, or pin the walk-time snapshot. NEVER re-derive silently.",
            range.windows.len(),
            range.sessions.len(),
            artifact.provenance.windows.len(),
            artifact.provenance.proven_sessions,
        )));
    }
    // Defensive: a degenerate full-range window would force every symbol
    // starting inside it into a single-session request. The artifact's own
    // partition is the authority above, so this can only fire on a
    // pathological calendar — surface it rather than emitting the request the
    // gateway mis-serves.
    if let Some(w) = range.windows.iter().find(|w| w.sessions < MIN_SESSIONS_PER_WINDOW) {
        return Err(AdapterError::Ingest(format!(
            "backfill window plan contains a {}-session window [{}..{}] — a single-session \
             t8410 request ignores `sdate` and serves an out-of-window page; refusing to plan",
            w.sessions, w.sdate, w.edate
        )));
    }
    Ok(range)
}

/// The pull range start for one manifest outcome (R2): `max(floor,
/// first_served)`. A `no_served_rows` member has none.
fn range_start(outcome: &ListingOutcome, floor: NaiveDate) -> Option<NaiveDate> {
    match outcome {
        ListingOutcome::PreFloor => Some(floor),
        ListingOutcome::Listed { first_served } => Some((*first_served).max(floor)),
        ListingOutcome::NoServedRows { .. } => None,
    }
}

/// The per-symbol window plan over `[start, anchor]` (R1, R2).
///
/// Windows are the full-range partition's, restricted to those that intersect
/// the symbol's range, with the **first** kept window trimmed forward to
/// `start`. Every `edate` therefore stays a full-range boundary, which is what
/// makes [`reconcile_watermark`] able to map a stored bar back to the window
/// that appended it.
///
/// The trim can leave the first window spanning a single session (when `start`
/// lands exactly on a full-range window's `edate`). R1 forbids emitting that,
/// so the window is widened **backward** by one proven session. That extension
/// is always available (a trimmed window is a strict tail of a
/// [`MAX_SESSIONS_PER_WINDOW`]-session window), stays inside the same
/// full-range window — so it can never overlap a sibling plan window — and
/// reaches below `first_served`, where the vendor serves nothing by definition.
pub fn plan_symbol_windows(
    range: &RangeSessions,
    start: NaiveDate,
    anchor: NaiveDate,
) -> AdapterResult<Vec<WalkWindow>> {
    let mut out: Vec<WalkWindow> = Vec::new();
    for w in &range.windows {
        if w.edate < start {
            continue;
        }
        let sdate = w.sdate.max(start);
        let sessions = range
            .sessions
            .iter()
            .filter(|d| **d >= sdate && **d <= w.edate)
            .count();
        if sessions == 0 {
            continue;
        }
        out.push(WalkWindow {
            sdate,
            edate: w.edate,
            sessions,
        });
    }
    let Some(first) = out.first_mut() else {
        return Err(AdapterError::Ingest(format!(
            "backfill plan: no window covers [{start}, {anchor}] — the symbol's range start is \
             above the anchor"
        )));
    };
    if first.sessions < MIN_SESSIONS_PER_WINDOW {
        // Widen backward within the enclosing full-range window.
        let idx = range
            .sessions
            .iter()
            .position(|d| *d == first.sdate)
            .ok_or_else(|| {
                AdapterError::Ingest(format!(
                    "backfill plan: window start {} is not a proven Trading Session",
                    first.sdate
                ))
            })?;
        let extra = MIN_SESSIONS_PER_WINDOW - first.sessions;
        if idx < extra {
            return Err(AdapterError::Ingest(format!(
                "backfill plan: cannot widen the {}-session window ending {} to {} sessions — the \
                 range holds too few proven sessions below it; a single-session request would be \
                 mis-served",
                first.sessions, first.edate, MIN_SESSIONS_PER_WINDOW
            )));
        }
        first.sdate = range.sessions[idx - extra];
        first.sessions += extra;
    }
    Ok(out)
}

/// Build the verified pull plan (U1): manifest + calendar snapshot in, a
/// per-symbol window plan out. Fails closed on a restricted/underived manifest
/// (R14) and on a window-plan mismatch (R5) — both **before** anything
/// gateway-capable exists.
pub fn build_plan(
    view: &AsOfView<'_>,
    artifact: &PitUniverseArtifact,
    manifest_path: &str,
) -> AdapterResult<BackfillPlan> {
    let range = verify_windows(view, artifact)?;
    let floor = artifact.provenance.floor;
    let anchor = artifact.provenance.anchor;
    let mut symbols = Vec::new();
    let mut excluded_no_served_rows = Vec::new();
    for s in &artifact.symbols {
        let Some(start) = range_start(&s.outcome, floor) else {
            excluded_no_served_rows.push(s.shcode.clone());
            continue;
        };
        let windows = plan_symbol_windows(&range, start, anchor)?;
        let expected_sessions = range.sessions.iter().filter(|d| **d >= start).count();
        symbols.push(SymbolPlan {
            shcode: s.shcode.clone(),
            start,
            windows,
            expected_sessions,
        });
    }
    if symbols.is_empty() {
        return Err(AdapterError::Ingest(format!(
            "backfill manifest {manifest_path} plans zero symbols — every member reads \
             `no_served_rows`; refusing to seed an empty catalog"
        )));
    }
    Ok(BackfillPlan {
        floor,
        anchor,
        manifest_path: manifest_path.to_string(),
        manifest_hash: manifest_content_hash(artifact),
        symbols,
        excluded_no_served_rows,
        range,
    })
}

// ---------------------------------------------------------------------------
// U2 — the windowed pull core
// ---------------------------------------------------------------------------

/// One window pull's result.
#[derive(Debug, Clone)]
pub struct WindowPull {
    /// Bars inside `[sdate, edate]`, ascending by `ts_init`. Empty means the
    /// window completed cleanly with zero served rows — never "done", always
    /// an R8 anomaly candidate.
    pub bars: Vec<Bar>,
    /// Gateway calls made (throttled retries included).
    pub calls: u32,
}

/// Walk one window's `cts_date` cursor and return its **bars** (U2).
///
/// This is [`crate::reference::pit_walk::walk_window`]'s fail-closed
/// termination discipline with two differences: the OHLCV is kept (the walk
/// discards it, needing only dates), and the window filter runs on the
/// **parsed bar timestamp** the way `collect_daily` does — immune to padded
/// boundary dates and to a lenient parse a string compare would misclassify.
///
/// Completion evidence is unambiguous or the pull fails: an empty echoed
/// cursor, or a page carrying a row below `sdate` (the cursor pages
/// recent→older, so every later page is out-of-window). A zero-row page with a
/// **live** cursor, a repeated cursor, and page-cap exhaustion each fail closed
/// with nothing appended — silence is never completion (R3). Every error
/// carries the calls already spent so a failed symbol's budget still reaches
/// the shared ledger (R12).
pub async fn pull_window<F: DailyFetcher>(
    fetcher: &F,
    shcode: &str,
    bar_type: BarType,
    window: WalkWindow,
    pace: Duration,
    max_pages: usize,
) -> Result<WindowPull, WalkError> {
    let sd = window.sdate.format("%Y%m%d").to_string();
    let ed = window.edate.format("%Y%m%d").to_string();
    let mut calls = 0u32;
    macro_rules! fail {
        ($err:expr) => {
            return Err(WalkError {
                calls,
                error: $err,
            })
        };
    }
    // Window bounds as bar timestamps — the same convention `build_daily_bar`
    // stamps (KST regular close of the candle's date), so the trim compares
    // parsed values (R4). Each endpoint resolves against ITS OWN close
    // (R13/R29 class b): a window may SPAN the 2016-08-01 close extension, and
    // one regime for the pair trims in-range bars away — and a below-window row
    // is also completion evidence, so the loss reads as a clean silent
    // completion rather than an error.
    let (ts_start, ts_end) = match (
        kst_to_unix_nanos(window.sdate, regular_close(SessionRegime::for_date(window.sdate))),
        kst_to_unix_nanos(window.edate, regular_close(SessionRegime::for_date(window.edate))),
    ) {
        (Ok(s), Ok(e)) => (s.as_u64(), e.as_u64()),
        (Err(e), _) | (_, Err(e)) => fail!(e),
    };

    let mut bars: Vec<Bar> = Vec::new();
    let mut cts_date = String::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut throttle_retries = 0usize;

    for page in 0..max_pages.max(1) {
        let resp = loop {
            tokio::time::sleep(pace).await;
            calls += 1;
            match fetcher.fetch_daily_page(shcode, &sd, &ed, &cts_date).await {
                Ok(r) => {
                    throttle_retries = 0;
                    break r;
                }
                Err(AdapterError::Sdk(LsError::ApiError { code, .. }))
                    if code == "IGW00201" && throttle_retries < MAX_THROTTLE_RETRIES =>
                {
                    throttle_retries += 1;
                    tokio::time::sleep(fetcher.throttle_backoff()).await;
                }
                Err(e) => fail!(AdapterError::Ingest(format!(
                    "backfill {shcode} [{sd}..{ed}] page {}: {e}",
                    page + 1
                ))),
            }
        };
        let mut reached_below_window = false;
        for row in &resp.outblock1 {
            match build_daily_bar(bar_type, row) {
                Ok(Some(b)) => {
                    let ts = b.ts_init.as_u64();
                    if ts < ts_start {
                        // Below-window rows are completion evidence, never appended.
                        reached_below_window = true;
                    } else if ts <= ts_end {
                        bars.push(b);
                    }
                    // Rows above `edate` are dropped: the window filter is
                    // ours, and a stray newer row must not widen the append.
                }
                Ok(None) => {}
                Err(e) => fail!(e),
            }
        }
        if reached_below_window {
            break;
        }
        let next = resp.outblock.cts_date.trim().to_string();
        if next.is_empty() {
            break;
        }
        if resp.outblock1.is_empty() {
            fail!(AdapterError::Ingest(format!(
                "backfill {shcode} [{sd}..{ed}]: zero-row page with a live cursor (suspect \
                 truncation — the gateway serves transiently empty pages; re-run)"
            )));
        }
        if !seen.insert(next.clone()) {
            fail!(AdapterError::Ingest(format!(
                "backfill {shcode} [{sd}..{ed}]: repeated cursor {next:?} (suspect truncation)"
            )));
        }
        cts_date = next;
        if page + 1 == max_pages.max(1) {
            fail!(AdapterError::Ingest(format!(
                "backfill {shcode} [{sd}..{ed}]: page cap {max_pages} exhausted with a live cursor"
            )));
        }
    }
    // LS serves newest-first and the cursor pages recent→older, so bars
    // accumulate descending; the catalog requires ascending `ts_init`.
    bars.sort_by_key(|b| b.ts_init.as_u64());
    Ok(WindowPull { bars, calls })
}

/// Map the catalog's stored coverage for one series back to the window whose
/// append produced it, and return that window's `edate` (R6).
///
/// The parquet append and the checkpoint save cannot be atomic together. A kill
/// in that gap leaves bars on disk with a watermark below them; resuming from
/// the stale watermark would re-fetch an already-appended window and die on the
/// overlap refusal, stalling the symbol forever. Reconciling forward closes it.
///
/// Correctness rests on two facts: windows are walked oldest-first, and window
/// trims keep every append's upper bound at a full-range window `edate`. So the
/// newest stored bar always sits inside the last window that was appended, and
/// that window's `edate` is exactly the watermark the interrupted run would
/// have saved. A window that completes cleanly *empty* degrades the symbol
/// (R8) without appending, so no bars can exist above an un-advanced watermark.
pub fn reconcile_watermark(
    windows: &[WalkWindow],
    stored_intervals: &[(u64, u64)],
) -> Option<NaiveDate> {
    let newest = stored_intervals.iter().map(|(_, e)| *e).max()?;
    let date = kst_date_of(UnixNanos::from(newest));
    windows
        .iter()
        .rev()
        .find(|w| w.sdate <= date && date <= w.edate)
        .map(|w| w.edate)
}

/// Read the catalog's stored coverage and reconcile one series' watermark
/// forward (R6). Returns the reconciled date when the catalog is ahead of
/// `watermark`, `None` when it is not.
pub async fn reconcile_from_catalog(
    catalog_path: &Path,
    bar_type: BarType,
    windows: &[WalkWindow],
    watermark: Option<NaiveDate>,
) -> AdapterResult<Option<NaiveDate>> {
    let intervals = stored_bar_intervals(catalog_path, bar_type).await?;
    let Some(reconciled) = reconcile_watermark(windows, &intervals) else {
        return Ok(None);
    };
    match watermark {
        Some(w) if w >= reconciled => Ok(None),
        _ => Ok(Some(reconciled)),
    }
}

/// The windows of a symbol's plan still to pull, given its watermark.
pub fn remaining_windows(windows: &[WalkWindow], watermark: Option<NaiveDate>) -> Vec<WalkWindow> {
    windows
        .iter()
        .filter(|w| watermark.map_or(true, |wm| w.edate > wm))
        .copied()
        .collect()
}

/// The defensive page bound one backfill window walk may span. A ≤450-session
/// window is one page in the expected case (P4 measured a ~501-row served cap
/// against a 900-row request), so this is pure headroom; exhausting it fails
/// closed.
pub const MAX_BACKFILL_PAGES: usize = MAX_WALK_PAGES;

// ---------------------------------------------------------------------------
// U3 — the mode runner's report shapes
// ---------------------------------------------------------------------------

/// A symbol that stopped mid-plan (R3, R8). Its watermark is held below the
/// window that stopped it, so a later session resumes exactly there — a
/// degradation is always surfaced, never fatal to the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DegradedSymbol {
    /// 6-digit KRX shcode.
    pub shcode: String,
    /// The (credential-scrubbed) reason.
    pub reason: String,
}

/// One attended backfill session's outcome.
///
/// Progress is read from this **and** from the checkpoint's watermark census —
/// never from the exit code, which `ls-ingest` returns as 0 both when a run is
/// caught up and when it is fully blocked.
#[derive(Debug, Clone, Default)]
pub struct BackfillRunReport {
    /// Bars appended this session.
    pub bars_written: usize,
    /// Windows appended this session.
    pub windows_pulled: usize,
    /// Symbols the batch attempted.
    pub symbols_attempted: usize,
    /// Symbols whose watermark reached the anchor (this session or earlier).
    pub symbols_complete: usize,
    /// Symbols already at the anchor on entry (zero gateway calls).
    pub symbols_skipped_complete: usize,
    /// Symbols that stopped mid-plan.
    pub degraded: Vec<DegradedSymbol>,
    /// Windows recorded as uncovered gaps (R8) — a clean zero-row completion
    /// that survived the bounded re-fetch. The watermark never advanced over
    /// one.
    pub uncovered_gaps: Vec<String>,
    /// Symbols wiped and restarted after a cross-day basis shift (R9).
    pub restarted: Vec<String>,
    /// Appends refused fail-closed for interval overlap.
    pub append_refusals: Vec<String>,
    /// Whether this session cleared the `backfill_incomplete` marker — i.e.
    /// every manifest symbol reached the anchor.
    pub marker_cleared: bool,
}

impl BackfillRunReport {
    /// Whether the session carried anything an operator must act on.
    pub fn any_refused(&self) -> bool {
        !self.degraded.is_empty() || !self.append_refusals.is_empty()
    }
}

// ---------------------------------------------------------------------------
// U5 — the manifest-aware completeness report
// ---------------------------------------------------------------------------

/// The committed evidence record's schema version.
pub const REPORT_SCHEMA_VERSION: u32 = 1;

/// What the report was generated from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportProvenance {
    /// The manifest artifact path (orientation only).
    pub manifest_path: String,
    /// The manifest artifact's content hash — the identity.
    pub manifest_hash: String,
    /// The catalog home the report read.
    pub catalog: String,
    /// The pre-registered history floor.
    pub floor: NaiveDate,
    /// The frozen anchor.
    pub anchor: NaiveDate,
    /// Manifest members planned for the pull.
    pub symbols_planned: usize,
    /// Proven Trading Sessions in `[floor, anchor]`.
    pub proven_sessions: usize,
    /// Whether the catalog still carries the incomplete-backfill marker.
    pub backfill_incomplete: bool,
    /// RFC-3339 generation timestamp.
    pub generated_at: String,
}

/// One symbol's completeness verdict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolVerdict {
    /// 6-digit KRX shcode.
    pub shcode: String,
    /// The expected coverage front: `max(floor, first_served)` (R2). Only the
    /// manifest knows this per symbol — which is exactly why the uniform
    /// `expected_range` form of `catalog status` cannot be used here: one range
    /// applied to every triple NO-GOes every post-floor listing.
    pub expected_front: NaiveDate,
    /// The stored coverage front (`None` = no stored bars).
    pub front: Option<NaiveDate>,
    /// The stored coverage tail.
    pub tail: Option<NaiveDate>,
    /// The checkpoint watermark.
    pub watermark: Option<NaiveDate>,
    /// Proven Trading Sessions in `[expected_front, anchor]`.
    pub expected_sessions: usize,
    /// Distinct stored sessions (duplicate rows collapse — a polluted catalog
    /// must not read as complete).
    pub stored_sessions: usize,
    /// Proven Trading Sessions in `[expected_front, anchor]` with no stored bar
    /// — the exact set difference, not a count comparison. A bar stored on a
    /// non-session date must not offset a genuinely missing session.
    pub missing_sessions: usize,
    /// **GO-blocking** anomalies: the structural ones that mean the pull did
    /// not finish (no bars, front truncation, tail short of the anchor,
    /// watermark short of the anchor, a recorded degradation).
    pub anomalies: Vec<String>,
    /// Loud but **non-blocking** observations. A halted symbol legitimately
    /// serves fewer bars than the calendar has sessions, so a session shortfall
    /// is reported at full volume without making the rung uncloseable — on a
    /// 352-symbol decade some symbol has always halted somewhere.
    pub observations: Vec<String>,
}

/// The rung's GO/NO-GO evidence record (R15).
///
/// Every anomaly is enumerated loud and none is a hard failure: an attended
/// pull legitimately produces halts and degradations, and the operator needs to
/// see all of them at once, not to have the report abort on the first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackfillReport {
    /// [`REPORT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// What the report read.
    pub provenance: ReportProvenance,
    /// Per-symbol verdicts, in manifest order.
    pub symbols: Vec<SymbolVerdict>,
    /// Run-level **GO-blocking** anomalies (missing instrument definitions, the
    /// standing incomplete-backfill marker, excluded members).
    pub anomalies: Vec<String>,
    /// Run-level loud but non-blocking observations.
    pub observations: Vec<String>,
    /// `true` when no GO-blocking anomaly was found. Observations are reported
    /// at full volume either way — they are never silently dropped, and they
    /// never make the verdict unreachable.
    pub go: bool,
}

impl BackfillReport {
    /// Every GO-blocking anomaly across the report, run-level first.
    pub fn all_anomalies(&self) -> Vec<String> {
        let mut out = self.anomalies.clone();
        for s in &self.symbols {
            for a in &s.anomalies {
                out.push(format!("{}: {a}", s.shcode));
            }
        }
        out
    }

    /// Every non-blocking observation across the report, run-level first.
    pub fn all_observations(&self) -> Vec<String> {
        let mut out = self.observations.clone();
        for s in &self.symbols {
            for o in &s.observations {
                out.push(format!("{}: {o}", s.shcode));
            }
        }
        out
    }

    /// Write the evidence record.
    pub fn write(&self, path: &Path) -> AdapterResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AdapterError::Ingest(format!("creating {}: {e}", parent.display()))
            })?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| AdapterError::Ingest(format!("serializing the backfill report: {e}")))?;
        std::fs::write(path, format!("{json}\n"))
            .map_err(|e| AdapterError::Ingest(format!("writing {}: {e}", path.display())))
    }
}

/// Build the manifest-aware completeness report (U5, R15): an offline read of
/// the catalog, the checkpoint, and the manifest.
pub async fn build_report(
    catalog_path: &Path,
    plan: &BackfillPlan,
    checkpoint: &Checkpoint,
    generated_at: String,
) -> AdapterResult<BackfillReport> {
    let label = BarKind::Daily.label();
    // R11: instrument definitions are checked per MANIFEST MEMBER, not just
    // "the catalog has some". The lab resolves each symbol individually, so one
    // member missing from the bootstrap backtests empty just as silently as an
    // empty catalog does.
    let defined: std::collections::HashSet<String> = read_all_instruments(catalog_path)
        .await?
        .iter()
        .map(|i| i.id().to_string())
        .collect();
    let mut symbols = Vec::with_capacity(plan.symbols.len());
    for sp in &plan.symbols {
        let id = instrument_id_for(&sp.shcode);
        let instrument = id.to_string();
        let bar_type = BarKind::Daily.bar_type(id)?;
        // Distinct stored sessions — a duplicate-polluted series must not read
        // as complete on row count alone.
        let stored: std::collections::BTreeSet<NaiveDate> =
            read_bars_scoped(catalog_path, bar_type, None, None)
                .await?
                .iter()
                .map(|b| kst_date_of(b.ts_event))
                .collect();
        let front = stored.iter().next().copied();
        let tail = stored.iter().next_back().copied();
        let watermark = checkpoint.watermark(&instrument, &label);
        let mut anomalies = Vec::new();
        let mut observations = Vec::new();
        if !defined.contains(&instrument) {
            anomalies.push(
                "no instrument definition in the catalog — the lab would resolve nothing for this \
                 symbol and backtest it as empty"
                    .to_string(),
            );
        }
        match front {
            None => anomalies.push("no stored bars".to_string()),
            Some(f) if f > sp.start => anomalies.push(format!(
                "coverage front {f} is later than the expected {} (front truncation)",
                sp.start
            )),
            Some(f) if f < sp.start => anomalies.push(format!(
                "coverage front {f} precedes the expected {} — bars below the manifest's \
                 first-served evidence",
                sp.start
            )),
            Some(_) => {}
        }
        match tail {
            None => {}
            Some(t) if t != plan.anchor => anomalies.push(format!(
                "coverage tail {t} is not the anchor {}",
                plan.anchor
            )),
            Some(_) => {}
        }
        if watermark != Some(plan.anchor) {
            anomalies.push(match watermark {
                Some(w) => format!("watermark {w} has not reached the anchor {}", plan.anchor),
                None => "no watermark — the symbol was never pulled".to_string(),
            });
        }
        // The exact set difference, never a count comparison: a bar stored on a
        // non-session date would otherwise offset a genuinely missing session
        // and the two holes would cancel out into a clean-looking total.
        let missing_sessions = plan
            .range
            .sessions
            .iter()
            .filter(|d| **d >= sp.start && !stored.contains(*d))
            .count();
        if missing_sessions > 0 {
            observations.push(format!(
                "session shortfall: {} of {} proven sessions in [{}, {}] have no bar (a halt \
                 legitimately produces these)",
                missing_sessions, sp.expected_sessions, sp.start, plan.anchor
            ));
        }
        if let Some(reason) = checkpoint.backfill_degraded(&instrument, &label) {
            anomalies.push(format!("degraded: {reason}"));
        }
        symbols.push(SymbolVerdict {
            shcode: sp.shcode.clone(),
            expected_front: sp.start,
            front,
            tail,
            watermark,
            expected_sessions: sp.expected_sessions,
            stored_sessions: stored.len(),
            missing_sessions,
            anomalies,
            observations,
        });
    }

    let mut anomalies = Vec::new();
    let observations = Vec::new();
    // R11: a home with bars but no instrument definitions makes every lab
    // backtest read EMPTY — silently, with no error anywhere. The bootstrap
    // writes them on the first invocation; this is the check that the bootstrap
    // actually happened, and it belongs in the completeness proof rather than
    // being discovered by a backtest that returns nothing. (Per-symbol coverage
    // is checked above; this names the bootstrap-never-ran case outright.)
    if defined.is_empty() {
        anomalies.push(
            "the catalog holds no instrument definitions — a lab backtest would read empty with \
             no error. Re-run one invocation WITHOUT LS_INGEST_SKIP_UNIVERSE_LOAD to bootstrap them."
                .to_string(),
        );
    }
    if checkpoint.backfill_incomplete() {
        anomalies.push(
            "the catalog still carries the incomplete-backfill marker — not every manifest \
             symbol has reached the anchor"
                .to_string(),
        );
    }
    for shcode in &plan.excluded_no_served_rows {
        anomalies.push(format!(
            "manifest member {shcode} served no rows in the P4 walk and was excluded from the \
             pull — surfaced, never inferred as delisted"
        ));
    }

    let go = anomalies.is_empty() && symbols.iter().all(|s| s.anomalies.is_empty());
    Ok(BackfillReport {
        schema_version: REPORT_SCHEMA_VERSION,
        provenance: ReportProvenance {
            manifest_path: plan.manifest_path.clone(),
            manifest_hash: plan.manifest_hash.clone(),
            catalog: catalog_path.display().to_string(),
            floor: plan.floor,
            anchor: plan.anchor,
            symbols_planned: plan.symbols.len(),
            proven_sessions: plan.range.sessions.len(),
            backfill_incomplete: checkpoint.backfill_incomplete(),
            generated_at,
        },
        symbols,
        anomalies,
        observations,
        go,
    })
}

/// The manifest identity pin written into the catalog on a GO (R17).
///
/// Pinned only from a refusal-free state, per the Universe metadata pin
/// convention: a pin written despite anomalies would attest a membership whose
/// bars never fully landed.
pub fn manifest_pin(plan: &BackfillPlan, pinned_at: String) -> MetadataPin {
    let mut per_stratum = std::collections::BTreeMap::new();
    per_stratum.insert("pit_universe_manifest".to_string(), plan.symbols.len());
    MetadataPin {
        artifact_path: plan.manifest_path.clone(),
        content_hash: plan.manifest_hash.clone(),
        per_stratum,
        symbols: plan.shcodes(),
        pinned_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::pit_walk::{SymbolRecord, WalkProvenance, ARTIFACT_SCHEMA_VERSION};
    use crate::reference::universe_metadata::{CapTier, MarketClass};
    use async_trait::async_trait;
    use chrono::Datelike;
    use ls_sdk::paginated::{T8410OutBlock1, T8410Response};
    use nautilus_ls_calendar::schema::{DayRow, DayStatus as CalDayStatus};
    use nautilus_ls_calendar::{compute_artifact_id, compute_calendar_id, KrxCalendar};
    use nautilus_model::identifiers::InstrumentId;
    use std::sync::Mutex;
    use crate::rules::{KRX_REGULAR_CLOSE, KRX_REGULAR_CLOSE_PRE_2016};

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn cal_as_of() -> chrono::DateTime<chrono::Utc> {
        use chrono::TimeZone;
        chrono::Utc.with_ymd_and_hms(2013, 6, 1, 0, 0, 0).unwrap()
    }

    /// A validated calendar over `[from, through]`: weekdays are proven Trading
    /// Sessions except `unknown` (Unknown) and `closed` (proven Closed);
    /// weekends are proven Closed. (Mirrors `pit_walk`'s fixture — the two
    /// modules validate through the same real loader.)
    fn calendar(
        from: NaiveDate,
        through: NaiveDate,
        unknown: &[NaiveDate],
        closed: &[NaiveDate],
    ) -> KrxCalendar {
        let template = {
            let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("nautilus-ls-calendar/fixtures/base_2010_2012.json");
            KrxCalendar::load_from_path(&path, cal_as_of())
                .expect("base fixture loads")
                .snapshot()
                .clone()
        };
        let mut snap = template;
        let mut rows = Vec::new();
        let mut d = from;
        while d <= through {
            let weekend = matches!(d.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun);
            let status = if unknown.contains(&d) {
                CalDayStatus::Unknown
            } else if weekend || closed.contains(&d) {
                CalDayStatus::Closed
            } else {
                CalDayStatus::TradingSession
            };
            rows.push(DayRow {
                date: d,
                status,
                decisive_evidence: Vec::new(),
                conflicting_evidence: Vec::new(),
                alerts: Vec::new(),
            });
            d = d.succ_opt().unwrap();
        }
        snap.rows = rows;
        snap.coverage.materialized_from = from;
        snap.coverage.materialized_through = through;
        snap.coverage.retrospectively_checked_through = through;
        snap.coverage.scheduled_closure_evaluated_through = through;
        snap.artifact_id = compute_artifact_id(&snap);
        snap.calendar_id = compute_calendar_id(&snap);
        KrxCalendar::from_snapshot(snap, cal_as_of()).expect("test calendar validates")
    }

    /// The fixture anchor. `verify_windows` deliberately pins the production
    /// [`MAX_SESSIONS_PER_WINDOW`] (no caller may shrink it and change the
    /// partition semantics), so the fixture range must be long enough to
    /// partition into several windows — the shape of the real 6-window plan.
    const FIXTURE_ANCHOR: (i32, u32, u32) = (2020, 12, 31);

    fn fixture_range() -> (KrxCalendar, RangeSessions) {
        let (y, m, d) = FIXTURE_ANCHOR;
        let cal = calendar(ymd(2016, 8, 1), ymd(y, m, d), &[], &[]);
        let range = {
            let view = cal.as_of(cal_as_of()).unwrap();
            partition_windows(&view, ymd(2016, 8, 1), ymd(y, m, d), MAX_SESSIONS_PER_WINDOW)
                .unwrap()
        };
        assert!(range.windows.len() >= 3, "the fixture must span several windows");
        (cal, range)
    }

    fn artifact(
        range: &RangeSessions,
        symbols: Vec<SymbolRecord>,
        restricted: bool,
        derived: bool,
    ) -> PitUniverseArtifact {
        PitUniverseArtifact {
            schema_version: ARTIFACT_SCHEMA_VERSION,
            provenance: WalkProvenance {
                tr: "t8410".into(),
                probed_at: "2026-08-13T00:00:00Z".into(),
                anchor: {
                    let (y, m, d) = FIXTURE_ANCHOR;
                    ymd(y, m, d)
                },
                floor: ymd(2016, 8, 1),
                source_artifact: "lab/config/universe.json".into(),
                source_content_hash: "abc".into(),
                pace_ms: 1000,
                qrycnt: 900,
                windows: range.windows.clone(),
                proven_sessions: range.sessions.len(),
                unknown_days: range.unknown_days,
                calls_made: 1,
                dropped_preferred: Vec::new(),
                dropped_malformed: Vec::new(),
                restricted,
            },
            symbols,
            measurements: Vec::new(),
            failed: Vec::new(),
            derived: derived.then(|| {
                crate::reference::pit_walk::DerivedBlock {
                    proven_sessions: range.sessions.len(),
                    symbols_counted: 1,
                    no_served_rows: Vec::new(),
                    listed_count_min: 1,
                    listed_count_median: 1,
                    listed_count_max: 1,
                    thresholds: Vec::new(),
                    mean_participation: 1.0,
                    full_participation_symbols: 1,
                    max_observed_rows_per_page: 1,
                    margin_note: "test".into(),
                }
            }),
        }
    }

    fn sym(shcode: &str, outcome: ListingOutcome) -> SymbolRecord {
        SymbolRecord {
            shcode: shcode.into(),
            market_class: MarketClass::Kospi,
            cap_tier: CapTier::Top,
            outcome,
            calls: 1,
            pages: 1,
        }
    }

    fn write_artifact(dir: &Path, a: &PitUniverseArtifact) -> std::path::PathBuf {
        let path = dir.join("manifest.json");
        std::fs::write(&path, serde_json::to_string_pretty(a).unwrap()).unwrap();
        path
    }

    // -- U1: manifest admission -------------------------------------------

    #[test]
    fn a_restricted_or_underived_manifest_is_refused() {
        let (_cal, range) = fixture_range();
        let dir = tempfile::tempdir().unwrap();

        let restricted = artifact(&range, vec![sym("005930", ListingOutcome::PreFloor)], true, true);
        let err = load_manifest(&write_artifact(dir.path(), &restricted)).unwrap_err();
        assert!(err.to_string().contains("RESTRICTED"), "{err}");

        let underived =
            artifact(&range, vec![sym("005930", ListingOutcome::PreFloor)], false, false);
        let err = load_manifest(&write_artifact(dir.path(), &underived)).unwrap_err();
        assert!(err.to_string().contains("derived: null"), "{err}");

        let ok = artifact(&range, vec![sym("005930", ListingOutcome::PreFloor)], false, true);
        assert!(load_manifest(&write_artifact(dir.path(), &ok)).is_ok());
    }

    #[test]
    fn a_manifest_with_failed_walks_is_refused() {
        let (_cal, range) = fixture_range();
        let dir = tempfile::tempdir().unwrap();
        let mut a = artifact(&range, vec![sym("005930", ListingOutcome::PreFloor)], false, true);
        a.failed.push(crate::reference::pit_walk::FailedSymbol {
            shcode: "000660".into(),
            error: "throttled".into(),
        });
        let err = load_manifest(&write_artifact(dir.path(), &a)).unwrap_err();
        assert!(err.to_string().contains("failed walks"), "{err}");
    }

    // -- U1: window verification ------------------------------------------

    #[test]
    fn a_doctored_window_plan_fails_closed_before_planning() {
        let (cal, range) = fixture_range();
        let view = cal.as_of(cal_as_of()).unwrap();
        let mut a = artifact(&range, vec![sym("005930", ListingOutcome::PreFloor)], false, true);
        // Doctor one boundary — the exact "the snapshot changed" shape.
        a.provenance.windows[0].edate = ymd(2018, 6, 4);
        let err = build_plan(&view, &a, "manifest.json").unwrap_err();
        assert!(err.to_string().contains("disagrees with the manifest"), "{err}");
        assert!(err.to_string().contains("NEVER re-derive silently"), "{err}");
    }

    #[test]
    fn an_interior_unknown_day_fails_closed_from_the_partitioner() {
        // The window plan is derived from the snapshot; an Unknown day inside
        // the range removes a session and moves every later boundary, so the
        // provenance comparison refuses. (`partition_windows` itself counts
        // Unknown as a non-session rather than erroring — the fail-closed arm
        // is the R5 equality.)
        let (_cal, range) = fixture_range();
        let (y, m, d) = FIXTURE_ANCHOR;
        let cal2 = calendar(ymd(2016, 8, 1), ymd(y, m, d), &[ymd(2016, 9, 1)], &[]);
        let view = cal2.as_of(cal_as_of()).unwrap();
        let a = artifact(&range, vec![sym("005930", ListingOutcome::PreFloor)], false, true);
        let err = build_plan(&view, &a, "manifest.json").unwrap_err();
        assert!(err.to_string().contains("disagrees with the manifest"), "{err}");
    }

    // -- U1: per-symbol planning ------------------------------------------

    #[test]
    fn a_pre_floor_symbol_gets_the_full_window_plan() {
        let (cal, range) = fixture_range();
        let view = cal.as_of(cal_as_of()).unwrap();
        let a = artifact(&range, vec![sym("005930", ListingOutcome::PreFloor)], false, true);
        let plan = build_plan(&view, &a, "manifest.json").unwrap();
        let s = plan.symbol("005930").unwrap();
        assert_eq!(s.start, ymd(2016, 8, 1), "pre-floor starts at the floor");
        assert_eq!(s.windows, range.windows, "the full plan, untrimmed");
        assert_eq!(s.expected_sessions, range.sessions.len());
    }

    #[test]
    fn a_listed_symbol_starts_at_its_first_served_window_trimmed_to_it() {
        let (cal, range) = fixture_range();
        let view = cal.as_of(cal_as_of()).unwrap();
        // A first_served inside the SECOND window.
        let first_served = range.sessions[MAX_SESSIONS_PER_WINDOW + 3];
        let a = artifact(
            &range,
            vec![sym("323410", ListingOutcome::Listed { first_served })],
            false,
            true,
        );
        let plan = build_plan(&view, &a, "manifest.json").unwrap();
        let s = plan.symbol("323410").unwrap();
        assert_eq!(s.start, first_served);
        assert_eq!(s.windows.len(), range.windows.len() - 1, "window 1 is dropped whole");
        assert_eq!(s.windows[0].sdate, first_served, "the first window trims to first_served");
        assert_eq!(s.windows[0].edate, range.windows[1].edate, "edates stay artifact-aligned");
        assert_eq!(s.windows[0].sessions, MAX_SESSIONS_PER_WINDOW - 3);
        assert_eq!(
            s.expected_sessions,
            range.sessions.len() - (MAX_SESSIONS_PER_WINDOW + 3)
        );
        // Every later window is the full-range window verbatim.
        assert_eq!(&s.windows[1..], &range.windows[2..]);
    }

    #[test]
    fn a_first_served_at_the_anchor_still_yields_a_multi_session_window() {
        let (cal, range) = fixture_range();
        let view = cal.as_of(cal_as_of()).unwrap();
        let anchor = {
            let (y, m, d) = FIXTURE_ANCHOR;
            ymd(y, m, d)
        };
        let last = *range.sessions.last().unwrap();
        assert_eq!(last, anchor);
        for first_served in [last, range.sessions[range.sessions.len() - 2]] {
            let a = artifact(
                &range,
                vec![sym("999990", ListingOutcome::Listed { first_served })],
                false,
                true,
            );
            let plan = build_plan(&view, &a, "manifest.json").unwrap();
            let s = plan.symbol("999990").unwrap();
            assert_eq!(s.windows.len(), 1, "only the last window intersects");
            let w = s.windows[0];
            assert!(
                w.sessions >= MIN_SESSIONS_PER_WINDOW,
                "the merge guard widened it: {w:?}"
            );
            assert_ne!(w.sdate, w.edate, "a degenerate sdate == edate request is never emitted");
            assert_eq!(w.edate, anchor);
            // The widened start stays inside the enclosing full-range window,
            // so it can never overlap a sibling plan window.
            assert!(w.sdate >= range.windows.last().unwrap().sdate);
        }
    }

    #[test]
    fn no_served_rows_members_are_excluded_and_surfaced_never_inferred_delisted() {
        let (cal, range) = fixture_range();
        let view = cal.as_of(cal_as_of()).unwrap();
        let a = artifact(
            &range,
            vec![
                sym("005930", ListingOutcome::PreFloor),
                sym("999990", ListingOutcome::NoServedRows { windows_probed: 6 }),
            ],
            false,
            true,
        );
        let plan = build_plan(&view, &a, "manifest.json").unwrap();
        assert_eq!(plan.shcodes(), vec!["005930"]);
        assert_eq!(plan.excluded_no_served_rows, vec!["999990"]);
    }

    #[test]
    fn an_all_anomaly_manifest_refuses_rather_than_seeding_an_empty_catalog() {
        let (cal, range) = fixture_range();
        let view = cal.as_of(cal_as_of()).unwrap();
        let a = artifact(
            &range,
            vec![sym("999990", ListingOutcome::NoServedRows { windows_probed: 6 })],
            false,
            true,
        );
        let err = build_plan(&view, &a, "manifest.json").unwrap_err();
        assert!(err.to_string().contains("plans zero symbols"), "{err}");
    }

    #[test]
    fn the_manifest_hash_is_content_identity_not_path() {
        let (_cal, range) = fixture_range();
        let a = artifact(&range, vec![sym("005930", ListingOutcome::PreFloor)], false, true);
        let mut b = a.clone();
        assert_eq!(manifest_content_hash(&a), manifest_content_hash(&b));
        b.symbols[0].shcode = "000660".into();
        assert_ne!(manifest_content_hash(&a), manifest_content_hash(&b));
    }

    // -- U2: the window pull ----------------------------------------------

    /// A scripted fetcher keyed by the FULL request tuple — an unscripted page
    /// errors, so a test cannot over-fetch or silently serve another symbol.
    struct ScriptedFetcher {
        pages: Mutex<std::collections::HashMap<(String, String, String, String), T8410Response>>,
    }

    impl ScriptedFetcher {
        fn new() -> Self {
            ScriptedFetcher {
                pages: Mutex::new(std::collections::HashMap::new()),
            }
        }

        fn script(
            &self,
            shcode: &str,
            sdate: &str,
            edate: &str,
            cts: &str,
            dates: &[&str],
            cursor_after: &str,
        ) {
            let mut resp = T8410Response::default();
            resp.outblock.cts_date = cursor_after.to_string();
            resp.outblock.rec_count = dates.len().to_string();
            resp.outblock1 = dates
                .iter()
                .map(|d| T8410OutBlock1 {
                    date: (*d).to_string(),
                    open: "100".into(),
                    high: "110".into(),
                    low: "90".into(),
                    close: "105".into(),
                    jdiff_vol: "1000".into(),
                    ..Default::default()
                })
                .collect();
            self.pages.lock().unwrap().insert(
                (
                    shcode.to_string(),
                    sdate.to_string(),
                    edate.to_string(),
                    cts.to_string(),
                ),
                resp,
            );
        }
    }

    #[async_trait]
    impl DailyFetcher for ScriptedFetcher {
        async fn fetch_daily_page(
            &self,
            shcode: &str,
            sdate: &str,
            edate: &str,
            cts_date: &str,
        ) -> AdapterResult<T8410Response> {
            self.pages
                .lock()
                .unwrap()
                .get(&(
                    shcode.to_string(),
                    sdate.to_string(),
                    edate.to_string(),
                    cts_date.to_string(),
                ))
                .cloned()
                .ok_or_else(|| {
                    AdapterError::Ingest(format!(
                        "unscripted page: shcode={shcode} sdate={sdate} edate={edate} cts={cts_date:?}"
                    ))
                })
        }
    }

    /// A fetcher that throttles (`IGW00201`) for the first `failures` calls,
    /// then serves a single terminal page.
    struct ThrottleFetcher {
        failures: Mutex<usize>,
        page: T8410Response,
    }

    impl ThrottleFetcher {
        fn new(failures: usize, dates: &[&str]) -> Self {
            let mut page = T8410Response::default();
            page.outblock.cts_date = String::new();
            page.outblock.rec_count = dates.len().to_string();
            page.outblock1 = dates
                .iter()
                .map(|d| T8410OutBlock1 {
                    date: (*d).to_string(),
                    open: "100".into(),
                    high: "110".into(),
                    low: "90".into(),
                    close: "105".into(),
                    jdiff_vol: "1000".into(),
                    ..Default::default()
                })
                .collect();
            ThrottleFetcher {
                failures: Mutex::new(failures),
                page,
            }
        }
    }

    #[async_trait]
    impl DailyFetcher for ThrottleFetcher {
        async fn fetch_daily_page(
            &self,
            _shcode: &str,
            _sdate: &str,
            _edate: &str,
            _cts_date: &str,
        ) -> AdapterResult<T8410Response> {
            let mut left = self.failures.lock().unwrap();
            if *left > 0 {
                *left -= 1;
                return Err(AdapterError::Sdk(LsError::ApiError {
                    code: "IGW00201".into(),
                    message: "throttled".into(),
                }));
            }
            Ok(self.page.clone())
        }
    }

    const NO_PACE: Duration = Duration::ZERO;

    fn bar_type_for(shcode: &str) -> BarType {
        crate::ingest::BarKind::Daily
            .bar_type(InstrumentId::from(
                format!("{shcode}.{}", crate::KRX_VENUE).as_str(),
            ))
            .unwrap()
    }

    fn window(sdate: NaiveDate, edate: NaiveDate, sessions: usize) -> WalkWindow {
        WalkWindow {
            sdate,
            edate,
            sessions,
        }
    }

    fn bar_dates(pull: &WindowPull) -> Vec<NaiveDate> {
        pull.bars.iter().map(|b| kst_date_of(b.ts_event)).collect()
    }

    #[tokio::test]
    async fn a_happy_window_returns_ascending_trimmed_bars() {
        let f = ScriptedFetcher::new();
        f.script(
            "005930",
            "20160801",
            "20160805",
            "",
            &["20160805", "20160804"],
            "20160803",
        );
        f.script(
            "005930",
            "20160801",
            "20160805",
            "20160803",
            &["20160803", "20160802", "20160801"],
            "",
        );
        let pull = pull_window(
            &f,
            "005930",
            bar_type_for("005930"),
            window(ymd(2016, 8, 1), ymd(2016, 8, 5), 5),
            NO_PACE,
            10,
        )
        .await
        .unwrap();
        assert_eq!(pull.calls, 2);
        assert_eq!(
            bar_dates(&pull),
            vec![
                ymd(2016, 8, 1),
                ymd(2016, 8, 2),
                ymd(2016, 8, 3),
                ymd(2016, 8, 4),
                ymd(2016, 8, 5)
            ],
            "ascending by ts_init, as the catalog requires"
        );
    }

    #[tokio::test]
    async fn rows_outside_the_window_are_trimmed_before_the_caller_sees_them() {
        let f = ScriptedFetcher::new();
        // A stray future row above `edate` AND a below-`sdate` row (which is
        // also the completion evidence — the cursor is NOT followed).
        f.script(
            "005930",
            "20160802",
            "20160804",
            "",
            &["20160809", "20160804", "20160803", "20160802", "20160801"],
            "20160729",
        );
        let pull = pull_window(
            &f,
            "005930",
            bar_type_for("005930"),
            window(ymd(2016, 8, 2), ymd(2016, 8, 4), 3),
            NO_PACE,
            10,
        )
        .await
        .unwrap();
        assert_eq!(
            bar_dates(&pull),
            vec![ymd(2016, 8, 2), ymd(2016, 8, 3), ymd(2016, 8, 4)],
            "the append stays inside the window on BOTH sides"
        );
        assert_eq!(pull.calls, 1, "a below-window row completes without following the cursor");
    }

    /// U9/R29 consumer class (b): INGEST RANGE BOUNDS. The bounds must move in
    /// EXACT lockstep with the stamping (class a), and a window can SPAN
    /// [`crate::rules::CLOSE_REFORM_DATE`], so each endpoint must resolve against
    /// ITS OWN close — one regime for the pair puts in-range bars outside the scan
    /// window. Worse, a below-window row is also completion evidence
    /// (`reached_below_window` breaks the walk), so the loss reads as a clean,
    /// silent completion rather than an error.
    ///
    /// Blast radius: whole windows silently empty — a deeper backfill stops at the
    /// first pre-2016 session it reaches.
    #[tokio::test]
    async fn window_bounds_resolve_each_endpoint_against_its_own_effective_close() {
        let f = ScriptedFetcher::new();
        // sdate 2016-07-29 closes at 15:00; edate 2016-08-02 closes at 15:30.
        // 2016-07-28 is genuinely below the window (completion evidence).
        f.script(
            "005930",
            "20160729",
            "20160802",
            "",
            &["20160802", "20160801", "20160729", "20160728"],
            "20160727",
        );
        let pull = pull_window(
            &f,
            "005930",
            bar_type_for("005930"),
            window(ymd(2016, 7, 29), ymd(2016, 8, 2), 3),
            NO_PACE,
            10,
        )
        .await
        .unwrap();
        assert_eq!(
            bar_dates(&pull),
            vec![ymd(2016, 7, 29), ymd(2016, 8, 1), ymd(2016, 8, 2)],
            "both endpoints survive the trim: the pre-reform sdate bar is not below a \
             15:30 start bound, and the post-reform edate bar is not above a 15:00 end bound"
        );
        // Witnessed on the kept bars: each endpoint carries its own close.
        assert_eq!(
            pull.bars[0].ts_event,
            kst_to_unix_nanos(ymd(2016, 7, 29), KRX_REGULAR_CLOSE_PRE_2016).unwrap()
        );
        assert_eq!(
            pull.bars[2].ts_event,
            kst_to_unix_nanos(ymd(2016, 8, 2), KRX_REGULAR_CLOSE).unwrap()
        );
    }

    #[tokio::test]
    async fn a_clean_zero_row_window_returns_empty_never_an_error() {
        // R8's input: a clean completion that served nothing. The pull reports
        // it as empty; the caller (never this function) decides it is a gap.
        let f = ScriptedFetcher::new();
        f.script("005930", "20160801", "20160805", "", &[], "");
        let pull = pull_window(
            &f,
            "005930",
            bar_type_for("005930"),
            window(ymd(2016, 8, 1), ymd(2016, 8, 5), 5),
            NO_PACE,
            10,
        )
        .await
        .unwrap();
        assert!(pull.bars.is_empty());
        assert_eq!(pull.calls, 1);
    }

    #[tokio::test]
    async fn suspect_truncation_fails_closed_with_nothing_returned() {
        // Zero-row page with a LIVE cursor.
        let f = ScriptedFetcher::new();
        f.script("005930", "20160801", "20160805", "", &[], "20160730");
        let err = pull_window(
            &f,
            "005930",
            bar_type_for("005930"),
            window(ymd(2016, 8, 1), ymd(2016, 8, 5), 5),
            NO_PACE,
            10,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("zero-row"), "{err}");
        assert_eq!(err.calls, 1);

        // Repeated cursor.
        let f = ScriptedFetcher::new();
        f.script("005930", "20160801", "20160805", "", &["20160805"], "20160804");
        f.script(
            "005930",
            "20160801",
            "20160805",
            "20160804",
            &["20160804"],
            "20160804",
        );
        let err = pull_window(
            &f,
            "005930",
            bar_type_for("005930"),
            window(ymd(2016, 8, 1), ymd(2016, 8, 5), 5),
            NO_PACE,
            10,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("repeated cursor"), "{err}");
        assert_eq!(err.calls, 2);

        // Page-cap exhaustion with a live cursor.
        let f = ScriptedFetcher::new();
        f.script("005930", "20160801", "20160805", "", &["20160805"], "20160804");
        f.script(
            "005930",
            "20160801",
            "20160805",
            "20160804",
            &["20160804"],
            "20160803",
        );
        let err = pull_window(
            &f,
            "005930",
            bar_type_for("005930"),
            window(ymd(2016, 8, 1), ymd(2016, 8, 5), 5),
            NO_PACE,
            2,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("page cap"), "{err}");
        assert_eq!(err.calls, 2);
    }

    #[tokio::test]
    async fn a_throttle_retries_the_same_cursor_and_every_call_reaches_the_ledger() {
        let f = ThrottleFetcher::new(2, &["20160802", "20160801"]);
        let pull = pull_window(
            &f,
            "005930",
            bar_type_for("005930"),
            window(ymd(2016, 8, 1), ymd(2016, 8, 5), 5),
            NO_PACE,
            10,
        )
        .await
        .unwrap();
        assert_eq!(pull.calls, 3, "the throttled dispatches are counted");
        assert_eq!(pull.bars.len(), 2);

        // A budget that never refills degrades the SYMBOL, and the error still
        // carries every call spent (R12).
        let f = ThrottleFetcher::new(usize::MAX, &[]);
        let err = pull_window(
            &f,
            "005930",
            bar_type_for("005930"),
            window(ymd(2016, 8, 1), ymd(2016, 8, 5), 5),
            NO_PACE,
            10,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("IGW00201"), "{err}");
        assert_eq!(err.calls, MAX_THROTTLE_RETRIES as u32 + 1);
    }

    // -- U2: watermark reconciliation --------------------------------------

    fn ts(d: NaiveDate) -> u64 {
        kst_to_unix_nanos(d, KRX_REGULAR_CLOSE).unwrap().as_u64()
    }

    #[test]
    fn reconciliation_maps_stored_bars_back_to_the_window_that_appended_them() {
        let windows = vec![
            window(ymd(2016, 8, 1), ymd(2016, 8, 26), 20),
            window(ymd(2016, 8, 29), ymd(2016, 9, 23), 20),
            window(ymd(2016, 9, 26), ymd(2016, 10, 21), 20),
        ];
        // Nothing stored → nothing to reconcile.
        assert_eq!(reconcile_watermark(&windows, &[]), None);
        // A completed window 1 append.
        assert_eq!(
            reconcile_watermark(&windows, &[(ts(ymd(2016, 8, 1)), ts(ymd(2016, 8, 26)))]),
            Some(ymd(2016, 8, 26))
        );
        // The kill-in-the-gap case: window 2's bars are on disk but its last
        // session was a halt, so the newest bar is BELOW the window edate. The
        // watermark must still reconcile to the window edate, or the resume
        // re-fetches window 2 and dies on the overlap refusal.
        assert_eq!(
            reconcile_watermark(
                &windows,
                &[
                    (ts(ymd(2016, 8, 1)), ts(ymd(2016, 8, 26))),
                    (ts(ymd(2016, 8, 29)), ts(ymd(2016, 9, 20))),
                ]
            ),
            Some(ymd(2016, 9, 23)),
            "reconciled to window 2's edate, not to the newest stored bar"
        );
        // Coverage below the plan's first window (a foreign series) reconciles
        // nothing rather than guessing.
        assert_eq!(
            reconcile_watermark(&windows, &[(ts(ymd(2015, 1, 5)), ts(ymd(2015, 2, 5)))]),
            None
        );
    }

    #[test]
    fn remaining_windows_resume_from_the_watermark_without_refetching() {
        let windows = vec![
            window(ymd(2016, 8, 1), ymd(2016, 8, 26), 20),
            window(ymd(2016, 8, 29), ymd(2016, 9, 23), 20),
            window(ymd(2016, 9, 26), ymd(2016, 10, 21), 20),
        ];
        assert_eq!(remaining_windows(&windows, None), windows);
        assert_eq!(
            remaining_windows(&windows, Some(ymd(2016, 8, 26))),
            windows[1..].to_vec(),
            "a completed window is never re-fetched"
        );
        assert!(remaining_windows(&windows, Some(ymd(2016, 10, 21))).is_empty());
    }
}
