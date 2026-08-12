//! P4 pit-universe depth walk over `t8410` daily bars (plan
//! `2026-08-12-001-feat-pit-universe-depth-walk-t8410`).
//!
//! A TR-parameterized backward window walk that measures, for a frozen
//! board-ranked universe, per-symbol listing evidence — plus a row-level
//! measurement subset that upgrades the 500-row page-cap and per-symbol-floor
//! readings from *inferred* (`body_len` discrimination, 2026-08-10) to
//! *measured*. The `derive` arithmetic turns the outcomes into the universe's
//! effective `S_max`, per-symbol participation, and re-derived margin bars for
//! the P6 pre-registration.
//!
//! ## The forbidden inference, made unrepresentable
//!
//! Delisting is NOT measurable from `t8410` (parent plan KD8): the ingest
//! correctly refuses to read an empty page as delisting, and so does this walk
//! — [`ListingOutcome`] has **no delisting variant**. An all-empty screen is
//! [`ListingOutcome::NoServedRows`], surfaced as an anomaly (the frozen set
//! comes from a present-day master, so every member should serve recent rows);
//! survivorship stays bounded by the R14 haircut, never removed here.
//!
//! ## Screening protocol (KTD2)
//!
//! `[floor, anchor]` is partitioned into calendar-snapped windows of at most
//! [`MAX_SESSIONS_PER_WINDOW`] proven sessions (every boundary is a proven
//! Trading Session, so chart date fields are pinned to real trading days), and
//! windows are probed **oldest-first**: one page resolves the expected case
//! (the window fits under the inferred page cap), and the cursor walk keeps
//! correctness if that inference is wrong. `PreFloor` and `Listed` verdicts
//! read the **first served bar**, a participation lower bound — a symbol
//! suspended on its window's first session reads as listed slightly later,
//! which only ever under-counts `N(s)`, never inflates it.
//!
//! ## TR parameterization (KTD4)
//!
//! The walk is generic over [`crate::ingest::DailyFetcher`] — the same seam
//! the certified daily ingest drives (`cts_date` body cursor only; the live
//! gateway self-paginates `t8410` on the body cursor and the `tr_cont` header
//! is not the continuation signal). A later depth walk over another chart TR
//! (the arm-D residue: t8465 / o3103 / t8418) is a new fetcher impl, not a
//! new harness.

use std::collections::HashSet;
use std::time::Duration;

use chrono::NaiveDate;
use ls_core::LsError;
use nautilus_ls_calendar::schema::DayStatus;
use nautilus_ls_calendar::{AsOfView, DateRange, SessionSearch};
use serde::{Deserialize, Serialize};

use crate::error::{AdapterError, AdapterResult};
use crate::ingest::DailyFetcher;
use crate::reference::universe_metadata::{CapTier, MarketClass, UniverseMetadata};

/// The pre-registered history floor (parent plan KD2): the KRX 15:00 → 15:30
/// close move's effective date, below which `rules.rs`' pinned close would
/// mis-stamp bars.
pub fn default_floor() -> NaiveDate {
    NaiveDate::from_ymd_opt(2016, 8, 1).expect("static date")
}

/// Max proven sessions per screening window — under the inferred 500-row page
/// cap with margin, so a window is one page in the expected case (KTD2). The
/// cursor walk keeps correctness if the inference is wrong; the measurement
/// subset is what upgrades it to measured.
pub const MAX_SESSIONS_PER_WINDOW: usize = 450;

/// Defensive page bound per window walk. The deepest legitimate walk (the
/// pilot's unbounded measurement, ~46 years ≈ 23 pages at the inferred cap)
/// stays far under it; exhausting it fails closed — silence is never
/// completion.
pub const MAX_WALK_PAGES: usize = 64;

/// Bounded consecutive `IGW00201` retries before a symbol's walk errors out.
/// The rolling call budget is cumulative and warm-sensitive, so a throttle is
/// transient — back off and retry the same cursor — but a budget that stays
/// dead degrades the *symbol* (surfaced by the caller), never the whole run.
pub const MAX_THROTTLE_RETRIES: usize = 8;

// ---------------------------------------------------------------------------
// Margin scaling (frozen constants, cited)
// ---------------------------------------------------------------------------

/// v35's session-block bootstrap SE (`lab/config/sample-margin.json`) at its
/// 45-calendar-session root — the parent scope plan's `SE(S) = 0.087002 ×
/// √(45/S)` projection, which reproduces the committed `+0.128605` margin
/// threshold at 237 sessions and the `+0.028906` holdout bar at 1,566.
pub const SE_AT_ROOT: f64 = 0.087002;
/// The projection's calendar-session root (KTD10 of plan `2026-08-07-001`).
pub const SE_ROOT_SESSIONS: f64 = 45.0;
/// The frozen rule's z at 95% confidence (`sample-margin.json` applies 1.96).
pub const Z_95: f64 = 1.96;

/// The margin bar at `N_max = 1` (`E[max] = 0`) over `sessions` — a
/// PROJECTION under ORB's clustering structure, re-measured by R20 before the
/// holdout is spent; never a measurement.
pub fn margin_bar_n1(sessions: usize) -> f64 {
    Z_95 * SE_AT_ROOT * (SE_ROOT_SESSIONS / sessions as f64).sqrt()
}

// ---------------------------------------------------------------------------
// Frozen walk set (KTD1)
// ---------------------------------------------------------------------------

/// One frozen-set member (the walk's unit of work).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrozenMember {
    /// 6-digit KRX shcode.
    pub shcode: String,
    /// KOSPI/KOSDAQ, carried from the source capture.
    pub market_class: MarketClass,
    /// The board cap tier that admitted the symbol (`Top` or `Mid`).
    pub cap_tier: CapTier,
}

/// The frozen walk set: the board-ranked slice of a capture artifact, minus
/// numeric-coded preferred shares, frozen by the source's content hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrozenSet {
    /// Members, sorted by shcode.
    pub members: Vec<FrozenMember>,
    /// The source capture artifact's content hash (identity, not a path).
    pub source_content_hash: String,
    /// Shcodes dropped by the P5 issue-sequence rule (6th digit ≠ 0). P4
    /// applies the *rule* so no budget is spent on symbols P5 excludes; the
    /// capture-side residual closure stays P5's.
    pub dropped_preferred: Vec<String>,
    /// Shcodes dropped for not being 6 digits (defensive; the capture already
    /// drops letter-suffixed classes).
    pub dropped_malformed: Vec<String>,
}

/// Freeze the walk set from a capture artifact (KTD1): `cap_tier ∈ {Top, Mid}`
/// — the only slice with resolved cap evidence (`liquidity_tier` is `unknown`
/// for the whole capture, so no finer liquidity cut exists) — minus the P5
/// preferred-share rule. The designation gate is deliberately NOT applied
/// (parent plan KD7: it is a live-trading gate; applying today's designations
/// to history is a look-ahead).
pub fn freeze_walk_set(meta: &UniverseMetadata) -> FrozenSet {
    let mut members = Vec::new();
    let mut dropped_preferred = Vec::new();
    let mut dropped_malformed = Vec::new();
    for r in &meta.records {
        if !matches!(r.cap_tier, CapTier::Top | CapTier::Mid) {
            continue;
        }
        let code = r.shcode.trim();
        if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
            dropped_malformed.push(code.to_string());
            continue;
        }
        // P5's issue-sequence rule: the 6th digit encodes the issue sequence;
        // ≠ 0 is a preferred-share class (e.g. 005935 삼성전자우).
        if code.as_bytes()[5] != b'0' {
            dropped_preferred.push(code.to_string());
            continue;
        }
        members.push(FrozenMember {
            shcode: code.to_string(),
            market_class: r.market_class,
            cap_tier: r.cap_tier,
        });
    }
    members.sort_by(|a, b| a.shcode.cmp(&b.shcode));
    dropped_preferred.sort();
    dropped_malformed.sort();
    FrozenSet {
        members,
        source_content_hash: meta.content_hash(),
        dropped_preferred,
        dropped_malformed,
    }
}

// ---------------------------------------------------------------------------
// Calendar-snapped windows (KTD2)
// ---------------------------------------------------------------------------

/// One screening window. Both endpoints are proven Trading Sessions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WalkWindow {
    /// First proven session of the window (the request `sdate`).
    pub sdate: NaiveDate,
    /// Last proven session of the window (the request `edate`).
    pub edate: NaiveDate,
    /// Proven sessions the window spans.
    pub sessions: usize,
}

/// The proven-session structure of `[floor, anchor]`: the screening windows
/// plus the full session list `derive` consumes, and the Unknown-day count
/// (Unknown is never Closed — it is counted as a non-session, the same
/// convention the parent plan's `S_max` used, and reported rather than
/// hidden).
#[derive(Debug, Clone)]
pub struct RangeSessions {
    /// Oldest-first screening windows.
    pub windows: Vec<WalkWindow>,
    /// Every proven Trading Session in `[floor, anchor]`, ascending.
    pub sessions: Vec<NaiveDate>,
    /// In-range days whose status is `Unknown` (non-sessions by convention).
    pub unknown_days: usize,
}

fn q_err(e: nautilus_ls_calendar::QueryError) -> AdapterError {
    AdapterError::Ingest(format!("pit-walk calendar query: {e}"))
}

/// Resolve the walk anchor (proof-preserving). An `explicit` date must be a
/// proven Trading Session inside `[floor, upto]` — the bound keeps a
/// forward-proven session in the snapshot from anchoring a walk past the run
/// date (look-ahead). Otherwise the LAST proven session in `[floor, upto]` —
/// refusing when an Unknown blocks the backward scan rather than stepping
/// past it (the probe-anchor discipline).
pub fn resolve_anchor(
    view: &AsOfView<'_>,
    floor: NaiveDate,
    upto: NaiveDate,
    explicit: Option<NaiveDate>,
) -> AdapterResult<NaiveDate> {
    if let Some(d) = explicit {
        if d < floor || d > upto {
            return Err(AdapterError::Ingest(format!(
                "pit-walk anchor {d} is outside [{floor}, {upto}] — an explicit anchor cannot \
                 reach past the run date"
            )));
        }
        let fact = view.day(d).map_err(q_err)?;
        return match fact.status {
            DayStatus::TradingSession => Ok(d),
            other => Err(AdapterError::Ingest(format!(
                "pit-walk anchor {d} is {other:?}, not a proven Trading Session — pass a proven session"
            ))),
        };
    }
    let range = DateRange::inclusive(floor, upto).map_err(q_err)?;
    match view.last_session(&range).map_err(q_err)? {
        SessionSearch::Found(d) => Ok(d),
        SessionSearch::Indeterminate => Err(AdapterError::Ingest(format!(
            "pit-walk anchor: an Unknown day blocks the backward scan from {upto} — run after the \
             session witness lands, or pass an explicit proven-session anchor"
        ))),
        SessionSearch::None => Err(AdapterError::Ingest(format!(
            "pit-walk anchor: no proven Trading Session in [{floor}, {upto}]"
        ))),
    }
}

/// Partition `[floor, anchor]` into oldest-first calendar-snapped windows of
/// at most `max_sessions` proven sessions each (KTD2). Fails closed on a
/// coverage gap (`OutOfRange` is a typed error, never an empty result).
pub fn partition_windows(
    view: &AsOfView<'_>,
    floor: NaiveDate,
    anchor: NaiveDate,
    max_sessions: usize,
) -> AdapterResult<RangeSessions> {
    if floor > anchor {
        return Err(AdapterError::Ingest(format!(
            "pit-walk range: floor {floor} is after anchor {anchor}"
        )));
    }
    let max_sessions = max_sessions.max(1);
    let mut sessions = Vec::new();
    let mut unknown_days = 0usize;
    let mut d = floor;
    loop {
        match view.day(d).map_err(q_err)?.status {
            DayStatus::TradingSession => sessions.push(d),
            DayStatus::Unknown => unknown_days += 1,
            DayStatus::Closed => {}
        }
        if d == anchor {
            break;
        }
        d = d.succ_opt().ok_or_else(|| {
            AdapterError::Ingest("pit-walk range: date overflow walking to anchor".into())
        })?;
    }
    let windows = sessions
        .chunks(max_sessions)
        .map(|chunk| WalkWindow {
            sdate: chunk[0],
            edate: *chunk.last().expect("chunks are non-empty"),
            sessions: chunk.len(),
        })
        .collect();
    Ok(RangeSessions {
        windows,
        sessions,
        unknown_days,
    })
}

// ---------------------------------------------------------------------------
// The cursor walk
// ---------------------------------------------------------------------------

/// One served page's row-level record (the measurement evidence, KTD3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageRecord {
    /// 1-based page index within the walk.
    pub page: usize,
    /// Served candle rows on the page (raw, before window filtering).
    pub rows: usize,
    /// Oldest served row date on the page.
    pub first_date: Option<NaiveDate>,
    /// Newest served row date on the page.
    pub last_date: Option<NaiveDate>,
    /// The summary block's echoed `rec_count`.
    pub rec_count_echo: String,
    /// The echoed `cts_date` cursor after this page (`""` = exhausted).
    pub cursor_after: String,
}

/// A walk failure carrying the gateway calls already spent before the error
/// (the capture bin's `CaptureError` pattern): a failed symbol's spend must
/// still reach the shared ledger, or the budget planner under-counts real
/// usage.
#[derive(Debug)]
pub struct WalkError {
    /// Gateway calls made before the failure (throttled retries included).
    pub calls: u32,
    /// The underlying error.
    pub error: AdapterError,
}

impl std::fmt::Display for WalkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

/// One window walk's result.
#[derive(Debug, Clone)]
pub struct WindowWalk {
    /// Served row dates inside `[sdate, edate]`, unordered.
    pub in_window_dates: Vec<NaiveDate>,
    /// Per-page records, in walk order.
    pub pages: Vec<PageRecord>,
    /// Gateway calls made (throttled retries included).
    pub calls: u32,
    /// Whether a page served a row below `sdate` (completion evidence: the
    /// cursor pages recent → older, so every later page is out-of-window).
    pub reached_below_window: bool,
}

fn parse_row_date(raw: &str) -> AdapterResult<NaiveDate> {
    NaiveDate::parse_from_str(raw.trim(), "%Y%m%d").map_err(|e| {
        AdapterError::Ingest(format!("pit-walk: unparseable row date {raw:?}: {e}"))
    })
}

/// Walk the `cts_date` cursor for one symbol over `[sdate, edate]`, recording
/// per-page row evidence. Follows the certified `collect_daily` cursor-walk
/// shape, deliberately **tightened** for a measurement probe: `collect_daily`
/// treats a zero-row page with a live cursor or a repeated cursor as walk
/// completion (bars already read stay valid for ingest), while this walk
/// fails closed on both — completion evidence must be unambiguous (an empty
/// echoed cursor or a below-window row), because a silent truncation here
/// would misread a listing date. Page-cap exhaustion also fails closed.
/// `pace` sleeps before every dispatch; an `IGW00201` retries the same cursor
/// after the fetcher's backoff, bounded by [`MAX_THROTTLE_RETRIES`]. Every
/// error carries the calls already spent ([`WalkError`]).
pub async fn walk_window<F: DailyFetcher>(
    fetcher: &F,
    shcode: &str,
    sdate: NaiveDate,
    edate: NaiveDate,
    pace: Duration,
    max_pages: usize,
) -> Result<WindowWalk, WalkError> {
    let sd = sdate.format("%Y%m%d").to_string();
    let ed = edate.format("%Y%m%d").to_string();
    let mut walk = WindowWalk {
        in_window_dates: Vec::new(),
        pages: Vec::new(),
        calls: 0,
        reached_below_window: false,
    };
    let mut cts_date = String::new();
    let mut seen = HashSet::new();
    let mut throttle_retries = 0usize;
    // Every error return carries the calls already spent (WalkError).
    macro_rules! fail {
        ($err:expr) => {
            return Err(WalkError {
                calls: walk.calls,
                error: $err,
            })
        };
    }
    for page in 0..max_pages.max(1) {
        let resp = loop {
            tokio::time::sleep(pace).await;
            walk.calls += 1;
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
                Err(e) => {
                    fail!(AdapterError::Ingest(format!(
                        "pit-walk {shcode} [{sd}..{ed}] page {}: {e}",
                        page + 1
                    )))
                }
            }
        };
        let mut dates = Vec::with_capacity(resp.outblock1.len());
        for row in &resp.outblock1 {
            match parse_row_date(&row.date) {
                Ok(d) => dates.push(d),
                Err(e) => fail!(e),
            }
        }
        walk.pages.push(PageRecord {
            page: page + 1,
            rows: dates.len(),
            first_date: dates.iter().min().copied(),
            last_date: dates.iter().max().copied(),
            rec_count_echo: resp.outblock.rec_count.trim().to_string(),
            cursor_after: resp.outblock.cts_date.trim().to_string(),
        });
        for d in dates {
            if d < sdate {
                walk.reached_below_window = true;
            } else if d <= edate {
                walk.in_window_dates.push(d);
            }
            // Rows above `edate` are skipped (the window filter is the
            // gateway's; a stray future row must not extend the evidence).
        }
        if walk.reached_below_window {
            return Ok(walk);
        }
        let next = walk.pages.last().expect("just pushed").cursor_after.clone();
        if next.is_empty() {
            return Ok(walk);
        }
        if resp.outblock1.is_empty() {
            fail!(AdapterError::Ingest(format!(
                "pit-walk {shcode} [{sd}..{ed}]: zero-row page with a live cursor (suspect \
                 truncation — the gateway serves transiently empty pages; re-run)"
            )));
        }
        if !seen.insert(next.clone()) {
            fail!(AdapterError::Ingest(format!(
                "pit-walk {shcode} [{sd}..{ed}]: repeated cursor {next:?} (suspect truncation)"
            )));
        }
        cts_date = next;
    }
    Err(WalkError {
        calls: walk.calls,
        error: AdapterError::Ingest(format!(
            "pit-walk {shcode} [{sd}..{ed}]: page cap {max_pages} exhausted with a live cursor"
        )),
    })
}

// ---------------------------------------------------------------------------
// Screening (per-symbol verdicts)
// ---------------------------------------------------------------------------

/// A symbol's listing evidence. There is deliberately **no delisting
/// variant**: an empty window is never evidence of departure (KD8), so the
/// forbidden inference is unrepresentable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ListingOutcome {
    /// Served a bar on the range's first proven session: listed at or before
    /// the floor. The exact listing date is deliberately not resolved —
    /// participation over `[floor, anchor]` is identical either way.
    PreFloor,
    /// First served bar strictly after the range's first session: the symbol's
    /// first served date (`t8410` serves from listing — the 323410 negative
    /// control). A participation lower bound, exactly what `N(s)` consumes.
    Listed {
        /// The first served bar date in `[floor, anchor]`.
        first_served: NaiveDate,
    },
    /// Every screening window served zero rows (clean zero-row completions).
    /// An anomaly to surface — NEVER read as delisting or non-listing.
    NoServedRows {
        /// Windows probed before concluding.
        windows_probed: usize,
    },
}

/// One screened symbol's walk tallies.
#[derive(Debug, Clone)]
pub struct SymbolWalk {
    /// The verdict.
    pub outcome: ListingOutcome,
    /// Gateway calls spent on this symbol.
    pub calls: u32,
    /// Pages served for this symbol.
    pub pages: usize,
}

/// Screen one symbol: probe the calendar-snapped windows oldest-first and stop
/// at the first window that serves rows (KTD2). A failure carries the calls
/// spent across all windows probed so far ([`WalkError`]).
pub async fn screen_symbol<F: DailyFetcher>(
    fetcher: &F,
    shcode: &str,
    windows: &[WalkWindow],
    pace: Duration,
) -> Result<SymbolWalk, WalkError> {
    let mut calls = 0u32;
    let mut pages = 0usize;
    for (i, w) in windows.iter().enumerate() {
        let ww = walk_window(fetcher, shcode, w.sdate, w.edate, pace, MAX_WALK_PAGES)
            .await
            .map_err(|we| WalkError {
                calls: calls + we.calls,
                error: we.error,
            })?;
        calls += ww.calls;
        pages += ww.pages.len();
        if let Some(earliest) = ww.in_window_dates.iter().min().copied() {
            let outcome = if i == 0 && earliest == w.sdate {
                ListingOutcome::PreFloor
            } else {
                ListingOutcome::Listed {
                    first_served: earliest,
                }
            };
            return Ok(SymbolWalk {
                outcome,
                calls,
                pages,
            });
        }
    }
    Ok(SymbolWalk {
        outcome: ListingOutcome::NoServedRows {
            windows_probed: windows.len(),
        },
        calls,
        pages,
    })
}

// ---------------------------------------------------------------------------
// Artifact schema (KTD7)
// ---------------------------------------------------------------------------

/// The committed artifact's schema version.
pub const ARTIFACT_SCHEMA_VERSION: u32 = 1;

/// Walk provenance: what ran, against what, and what it cost.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalkProvenance {
    /// The walked TR (`t8410`).
    pub tr: String,
    /// RFC-3339 walk timestamp.
    pub probed_at: String,
    /// The proven-session anchor (`edate` of the newest window).
    pub anchor: NaiveDate,
    /// The pre-registered floor.
    pub floor: NaiveDate,
    /// The source capture artifact path (orientation only; identity is the hash).
    pub source_artifact: String,
    /// The source capture artifact's content hash.
    pub source_content_hash: String,
    /// Inter-call pacing (ms).
    pub pace_ms: u64,
    /// The requested per-page row count (deliberately above the inferred
    /// 500-row server cap, so the served page size measures the cap).
    pub qrycnt: usize,
    /// The calendar-snapped screening windows, oldest-first.
    pub windows: Vec<WalkWindow>,
    /// Proven Trading Sessions in `[floor, anchor]`.
    pub proven_sessions: usize,
    /// In-range Unknown days (non-sessions by the `S_max` convention).
    pub unknown_days: usize,
    /// Total gateway calls the run made.
    pub calls_made: u32,
    /// Shcodes the P5 preferred-share rule dropped at freeze.
    pub dropped_preferred: Vec<String>,
    /// Shcodes dropped as malformed at freeze (defensive; surfaced so no
    /// exclusion is silent — the KTD7 no-hidden-evidence posture).
    pub dropped_malformed: Vec<String>,
    /// Whether the run was restricted to a symbol subset (`LS_PIT_SYMBOLS`).
    /// A restricted artifact must never carry a derived block — its `N(s)`
    /// would describe the subset while reading as the frozen universe.
    pub restricted: bool,
}

/// One symbol's committed record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRecord {
    /// 6-digit KRX shcode.
    pub shcode: String,
    /// Market class, carried from the capture.
    pub market_class: MarketClass,
    /// Cap tier that admitted the symbol.
    pub cap_tier: CapTier,
    /// The listing verdict.
    pub outcome: ListingOutcome,
    /// Gateway calls spent.
    pub calls: u32,
    /// Pages served.
    pub pages: usize,
}

/// One measurement walk's committed record (KTD3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeasurementRecord {
    /// The walked symbol.
    pub shcode: String,
    /// Walk `sdate` (the deep pilot walk starts below the floor on purpose).
    pub sdate: NaiveDate,
    /// Walk `edate`.
    pub edate: NaiveDate,
    /// Per-page row evidence.
    pub pages: Vec<PageRecord>,
    /// Gateway calls spent.
    pub calls: u32,
}

/// A symbol whose walk errored (throttle-dead budget, gateway failure). Kept
/// distinct from [`ListingOutcome::NoServedRows`] — "we gave up" is never
/// "it served nothing". A non-empty list makes the run incomplete (non-zero
/// exit); the artifact is not committed until a re-run clears it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedSymbol {
    /// The symbol whose walk errored.
    pub shcode: String,
    /// The scrubbed error string.
    pub error: String,
}

/// The committed pit-universe artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PitUniverseArtifact {
    /// Schema version ([`ARTIFACT_SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// Walk provenance.
    pub provenance: WalkProvenance,
    /// Per-symbol outcomes, sorted by shcode.
    pub symbols: Vec<SymbolRecord>,
    /// The measurement subset's row-level evidence.
    pub measurements: Vec<MeasurementRecord>,
    /// Walks that errored (non-empty = incomplete run).
    pub failed: Vec<FailedSymbol>,
    /// The derived block (`None` only mid-assembly; `derive` refuses to leave
    /// it empty on a complete run).
    pub derived: Option<DerivedBlock>,
}

// ---------------------------------------------------------------------------
// Derivation (KTD6)
// ---------------------------------------------------------------------------

/// One concurrency threshold's effective-`S_max` row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThresholdRow {
    /// The concurrency floor (`concurrent = m × hold`; 70 and 140 are the
    /// parent plan's operative cells).
    pub concurrency: usize,
    /// Sessions with `N(s) ≥ concurrency` — the universe's effective `S_max`
    /// at this floor.
    pub effective_s_max: usize,
    /// First session at/above the floor (`None` = never reached).
    pub first_session_at_or_above: Option<NaiveDate>,
    /// The `N_max = 1` margin bar at `effective_s_max` (projection; see
    /// [`margin_bar_n1`]).
    pub margin_bar_n1: f64,
}

/// The derived block: the numbers P6's pre-registration re-derives from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivedBlock {
    /// Proven sessions in `[floor, anchor]` (the pilot-ceiling analogue).
    pub proven_sessions: usize,
    /// Symbols contributing to `N(s)` (PreFloor + Listed).
    pub symbols_counted: usize,
    /// Anomaly shcodes (`NoServedRows`) — excluded from `N(s)`, surfaced.
    pub no_served_rows: Vec<String>,
    /// Minimum of `N(s)` over the range.
    pub listed_count_min: usize,
    /// Median of `N(s)` over the range (nearest-rank p50: the lower-middle
    /// value on an even session count — a stated convention, since this feeds
    /// a pre-registration statistic).
    pub listed_count_median: usize,
    /// Maximum of `N(s)` over the range.
    pub listed_count_max: usize,
    /// Effective `S_max` per concurrency threshold.
    pub thresholds: Vec<ThresholdRow>,
    /// Mean per-symbol participation (fraction of sessions at/after the
    /// symbol's first served bar). An UPPER bound on tradable participation —
    /// delisting is unmeasurable, and the R14 haircut owns that bias.
    pub mean_participation: f64,
    /// Symbols with full participation (`PreFloor`).
    pub full_participation_symbols: usize,
    /// The largest served page observed by the measurement subset — a
    /// **lower bound** on the server page cap, not the cap itself. The cap
    /// (inferred 500, 2026-08-10) counts as *measured* only when this value
    /// sits strictly below the requested `qrycnt` in provenance (the server
    /// truncated a page it was asked to fill); an observed maximum equal to
    /// `qrycnt` or drawn from short windows is a config-capped sample.
    pub max_observed_rows_per_page: usize,
    /// The citation trail for the margin arithmetic.
    pub margin_note: String,
}

/// Derive the P6 inputs from walk outcomes (KTD6). Refuses when the
/// measurement subset is absent — the page-cap upgrade is a deliverable, not
/// an optional extra — and when no symbol resolved.
pub fn derive(
    symbols: &[SymbolRecord],
    measurements: &[MeasurementRecord],
    sessions: &[NaiveDate],
    thresholds: &[usize],
) -> AdapterResult<DerivedBlock> {
    if measurements.is_empty() {
        return Err(AdapterError::Ingest(
            "pit-walk derive: no measurement records — the row-level page-cap measurement is a \
             P4 deliverable; run the walk with a measurement subset"
                .into(),
        ));
    }
    if sessions.is_empty() {
        return Err(AdapterError::Ingest(
            "pit-walk derive: no proven sessions in range".into(),
        ));
    }
    let mut starts: Vec<usize> = Vec::new();
    let mut no_served_rows = Vec::new();
    for s in symbols {
        match &s.outcome {
            ListingOutcome::PreFloor => starts.push(0),
            ListingOutcome::Listed { first_served } => {
                starts.push(sessions.partition_point(|d| d < first_served));
            }
            ListingOutcome::NoServedRows { .. } => no_served_rows.push(s.shcode.clone()),
        }
    }
    if starts.is_empty() {
        return Err(AdapterError::Ingest(
            "pit-walk derive: no symbol resolved PreFloor or Listed".into(),
        ));
    }
    starts.sort_unstable();
    let s_total = sessions.len();
    // N(s) at session index i = symbols whose start index ≤ i (prefix count
    // over the sorted starts).
    let listed_at = |i: usize| starts.partition_point(|&st| st <= i);
    let counts: Vec<usize> = (0..s_total).map(listed_at).collect();
    let mut sorted_counts = counts.clone();
    sorted_counts.sort_unstable();
    let threshold_rows = thresholds
        .iter()
        .map(|&c| {
            let effective = counts.iter().filter(|&&n| n >= c).count();
            ThresholdRow {
                concurrency: c,
                effective_s_max: effective,
                first_session_at_or_above: counts
                    .iter()
                    .position(|&n| n >= c)
                    .map(|i| sessions[i]),
                margin_bar_n1: if effective > 0 {
                    margin_bar_n1(effective)
                } else {
                    f64::NAN
                },
            }
        })
        .collect();
    let participation: f64 = starts
        .iter()
        .map(|&st| (s_total - st) as f64 / s_total as f64)
        .sum::<f64>()
        / starts.len() as f64;
    let max_page = measurements
        .iter()
        .flat_map(|m| m.pages.iter().map(|p| p.rows))
        .max()
        .unwrap_or(0);
    Ok(DerivedBlock {
        proven_sessions: s_total,
        symbols_counted: starts.len(),
        no_served_rows,
        listed_count_min: sorted_counts[0],
        // Nearest-rank p50: lower middle on even counts (stated convention).
        listed_count_median: sorted_counts[(s_total - 1) / 2],
        listed_count_max: sorted_counts[s_total - 1],
        thresholds: threshold_rows,
        mean_participation: participation,
        full_participation_symbols: starts.iter().filter(|&&st| st == 0).count(),
        max_observed_rows_per_page: max_page,
        margin_note: format!(
            "bar(N=1, S) = {Z_95} × {SE_AT_ROOT} × √({SE_ROOT_SESSIONS}/S) — v35 session-block \
             bootstrap SE projection under ORB clustering (sample-margin.json; scope plan \
             2026-08-10-001). A PROJECTION: R20 re-measures ICC/m/participation before the \
             holdout is spent."
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::universe_metadata::{
        IndexMembership, InstrumentMetadata, LiquidityTier, MetadataProvenance, Resolved,
    };
    use async_trait::async_trait;
    use chrono::Datelike;
    use ls_sdk::paginated::{T8410OutBlock1, T8410Response};
    use nautilus_ls_calendar::schema::{DayRow, DayStatus as CalDayStatus};
    use nautilus_ls_calendar::{compute_artifact_id, compute_calendar_id, KrxCalendar};
    use std::sync::Mutex;

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn cal_as_of() -> chrono::DateTime<chrono::Utc> {
        use chrono::TimeZone;
        chrono::Utc.with_ymd_and_hms(2013, 6, 1, 0, 0, 0).unwrap()
    }

    /// A validated calendar over `[from, through]`: weekdays are proven
    /// Trading Sessions except the dates in `unknown` (Unknown) and `closed`
    /// (proven Closed); weekends are proven Closed. Reuses the base fixture's
    /// scope/authorization so it validates through the real loader.
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
            let weekend = matches!(
                d.weekday(),
                chrono::Weekday::Sat | chrono::Weekday::Sun
            );
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

    /// A scripted fetcher keyed by the FULL request tuple
    /// `(shcode, sdate, edate, cts_date)` — a regression that swaps the
    /// symbol or sends the wrong window surfaces as an unscripted-page error
    /// instead of silently serving another symbol's evidence. Errors on an
    /// unscripted request so a test cannot over-fetch.
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
    /// then delegates to a single scripted terminal page.
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

    // -- windows ------------------------------------------------------------

    #[test]
    fn partition_snaps_boundaries_to_proven_sessions_and_counts_unknowns() {
        let unknown = [ymd(2016, 12, 30)];
        let cal = calendar(ymd(2016, 8, 1), ymd(2017, 3, 31), &unknown, &[]);
        let view = cal.as_of(cal_as_of()).unwrap();
        let rs = partition_windows(&view, ymd(2016, 8, 1), ymd(2017, 3, 31), 60).unwrap();
        assert_eq!(rs.unknown_days, 1);
        assert!(!rs.sessions.contains(&ymd(2016, 12, 30)));
        // 2016-08-01 is a Monday: the first window starts on the floor itself.
        assert_eq!(rs.windows[0].sdate, ymd(2016, 8, 1));
        let total: usize = rs.windows.iter().map(|w| w.sessions).sum();
        assert_eq!(total, rs.sessions.len());
        for w in &rs.windows {
            assert!(w.sessions <= 60);
            // Boundaries are proven sessions (weekday, not unknown/closed).
            assert!(rs.sessions.contains(&w.sdate));
            assert!(rs.sessions.contains(&w.edate));
        }
        // Oldest-first and contiguous.
        for pair in rs.windows.windows(2) {
            assert!(pair[0].edate < pair[1].sdate);
        }
    }

    #[test]
    fn anchor_resolution_is_proof_preserving() {
        // Frontier: Mon 2017-04-03 is Unknown (witness not yet landed).
        let unknown = [ymd(2017, 4, 3)];
        let cal = calendar(ymd(2016, 8, 1), ymd(2017, 4, 3), &unknown, &[]);
        let view = cal.as_of(cal_as_of()).unwrap();
        // Backward scan from the Unknown frontier refuses.
        let err = resolve_anchor(&view, ymd(2016, 8, 1), ymd(2017, 4, 3), None).unwrap_err();
        assert!(err.to_string().contains("Unknown"), "{err}");
        // Scanning from the proven-Closed weekend finds Friday.
        let anchor = resolve_anchor(&view, ymd(2016, 8, 1), ymd(2017, 4, 2), None).unwrap();
        assert_eq!(anchor, ymd(2017, 3, 31));
        // An explicit anchor must be a proven session.
        assert!(resolve_anchor(&view, ymd(2016, 8, 1), ymd(2017, 4, 3), Some(ymd(2017, 4, 3)))
            .is_err());
        assert_eq!(
            resolve_anchor(&view, ymd(2016, 8, 1), ymd(2017, 4, 3), Some(ymd(2017, 3, 31)))
                .unwrap(),
            ymd(2017, 3, 31)
        );
        // An explicit anchor outside [floor, upto] refuses even when the
        // snapshot proves the day (no look-ahead past the run date).
        let err =
            resolve_anchor(&view, ymd(2016, 8, 1), ymd(2017, 3, 1), Some(ymd(2017, 3, 31)))
                .unwrap_err();
        assert!(err.to_string().contains("outside"), "{err}");
    }

    #[test]
    fn partition_refuses_a_floor_after_the_anchor() {
        let cal = calendar(ymd(2016, 8, 1), ymd(2016, 9, 30), &[], &[]);
        let view = cal.as_of(cal_as_of()).unwrap();
        let err = partition_windows(&view, ymd(2016, 9, 1), ymd(2016, 8, 1), 60).unwrap_err();
        assert!(err.to_string().contains("after"), "{err}");
    }

    // -- walk terminations --------------------------------------------------

    #[tokio::test]
    async fn walk_completes_on_empty_cursor_and_records_pages() {
        let f = ScriptedFetcher::new();
        f.script("005930", "20160801", "20160803", "", &["20160803", "20160802"], "20160801");
        f.script("005930", "20160801", "20160803", "20160801", &["20160801"], "");
        let ww = walk_window(&f, "005930", ymd(2016, 8, 1), ymd(2016, 8, 3), NO_PACE, 10)
            .await
            .unwrap();
        assert_eq!(ww.calls, 2);
        assert_eq!(ww.pages.len(), 2);
        assert_eq!(ww.in_window_dates.len(), 3);
        assert_eq!(ww.pages[0].rows, 2);
        assert_eq!(ww.pages[0].cursor_after, "20160801");
        assert_eq!(ww.pages[1].cursor_after, "");
        assert!(!ww.reached_below_window);
    }

    #[tokio::test]
    async fn walk_completes_on_below_window_row_without_following_cursor() {
        let f = ScriptedFetcher::new();
        // A row below sdate with a live cursor: completion evidence, cursor
        // NOT followed (collect_daily's below-window completion arm).
        f.script("005930", "20160801", "20160803", "", &["20160802", "20160729"], "20160728");
        let ww = walk_window(&f, "005930", ymd(2016, 8, 1), ymd(2016, 8, 3), NO_PACE, 10)
            .await
            .unwrap();
        assert!(ww.reached_below_window);
        assert_eq!(ww.in_window_dates, vec![ymd(2016, 8, 2)]);
        assert_eq!(ww.calls, 1);
    }

    #[tokio::test]
    async fn walk_excludes_rows_above_edate_from_window_evidence() {
        let f = ScriptedFetcher::new();
        // A stray future row (above edate) must not extend the evidence.
        f.script("005930", "20160801", "20160803", "", &["20160805", "20160802", "20160801"], "");
        let ww = walk_window(&f, "005930", ymd(2016, 8, 1), ymd(2016, 8, 3), NO_PACE, 10)
            .await
            .unwrap();
        assert_eq!(ww.in_window_dates, vec![ymd(2016, 8, 2), ymd(2016, 8, 1)]);
        // The raw page record still reports what was served.
        assert_eq!(ww.pages[0].rows, 3);
        assert_eq!(ww.pages[0].last_date, Some(ymd(2016, 8, 5)));
    }

    #[tokio::test]
    async fn walk_fails_closed_on_zero_row_live_cursor_repeat_and_page_cap() {
        // Zero-row page with a live cursor.
        let f = ScriptedFetcher::new();
        f.script("005930", "20160801", "20160803", "", &[], "20160730");
        let err = walk_window(&f, "005930", ymd(2016, 8, 1), ymd(2016, 8, 3), NO_PACE, 10)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("zero-row"), "{err}");
        assert_eq!(err.calls, 1);

        // Repeated cursor.
        let f = ScriptedFetcher::new();
        f.script("005930", "20160801", "20160803", "", &["20160803"], "20160802");
        f.script("005930", "20160801", "20160803", "20160802", &["20160802"], "20160802");
        let err = walk_window(&f, "005930", ymd(2016, 8, 1), ymd(2016, 8, 3), NO_PACE, 10)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("repeated cursor"), "{err}");
        assert_eq!(err.calls, 2);

        // Page-cap exhaustion with a live cursor.
        let f = ScriptedFetcher::new();
        f.script("005930", "20160801", "20160803", "", &["20160803"], "20160802");
        f.script("005930", "20160801", "20160803", "20160802", &["20160802"], "20160801x");
        let err = walk_window(&f, "005930", ymd(2016, 8, 1), ymd(2016, 8, 3), NO_PACE, 2)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("page cap"), "{err}");
        assert_eq!(err.calls, 2);
    }

    #[tokio::test]
    async fn walk_fails_loud_on_unparseable_row_date() {
        let f = ScriptedFetcher::new();
        f.script("005930", "20160801", "20160803", "", &["2016080x"], "");
        let err = walk_window(&f, "005930", ymd(2016, 8, 1), ymd(2016, 8, 3), NO_PACE, 10)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unparseable"), "{err}");
    }

    #[tokio::test]
    async fn walk_retries_throttles_then_succeeds_counting_every_call() {
        // Two IGW00201 throttles, then a clean terminal page: the walk
        // recovers on the SAME cursor and every dispatch is counted.
        let f = ThrottleFetcher::new(2, &["20160802", "20160801"]);
        let ww = walk_window(&f, "005930", ymd(2016, 8, 1), ymd(2016, 8, 3), NO_PACE, 10)
            .await
            .unwrap();
        assert_eq!(ww.calls, 3);
        assert_eq!(ww.in_window_dates.len(), 2);
    }

    #[tokio::test]
    async fn walk_bounds_throttle_retries_and_carries_spent_calls() {
        // A budget that never refills: the walk errors after the retry bound
        // and the error still carries every call spent (the spend-ledger
        // accounting the bin records on failure).
        let f = ThrottleFetcher::new(usize::MAX, &[]);
        let err = walk_window(&f, "005930", ymd(2016, 8, 1), ymd(2016, 8, 3), NO_PACE, 10)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("IGW00201"), "{err}");
        assert_eq!(err.calls, MAX_THROTTLE_RETRIES as u32 + 1);
    }

    // -- screening verdicts -------------------------------------------------

    fn two_windows() -> Vec<WalkWindow> {
        vec![
            WalkWindow {
                sdate: ymd(2016, 8, 1),
                edate: ymd(2016, 8, 3),
                sessions: 3,
            },
            WalkWindow {
                sdate: ymd(2016, 8, 4),
                edate: ymd(2016, 8, 5),
                sessions: 2,
            },
        ]
    }

    #[tokio::test]
    async fn screen_reads_pre_floor_from_first_session_bar() {
        let f = ScriptedFetcher::new();
        f.script("005930", "20160801", "20160803", "", &["20160803", "20160802", "20160801"], "");
        let sw = screen_symbol(&f, "005930", &two_windows(), NO_PACE).await.unwrap();
        assert_eq!(sw.outcome, ListingOutcome::PreFloor);
        assert_eq!(sw.calls, 1);
    }

    #[tokio::test]
    async fn screen_reads_listing_inside_first_window() {
        let f = ScriptedFetcher::new();
        f.script("323410", "20160801", "20160803", "", &["20160803", "20160802"], "");
        let sw = screen_symbol(&f, "323410", &two_windows(), NO_PACE).await.unwrap();
        assert_eq!(
            sw.outcome,
            ListingOutcome::Listed {
                first_served: ymd(2016, 8, 2)
            }
        );
    }

    #[tokio::test]
    async fn screen_walks_to_a_later_window_for_a_post_floor_listing() {
        let f = ScriptedFetcher::new();
        f.script("323410", "20160801", "20160803", "", &[], "");
        f.script("323410", "20160804", "20160805", "", &["20160805"], "");
        let sw = screen_symbol(&f, "323410", &two_windows(), NO_PACE).await.unwrap();
        assert_eq!(
            sw.outcome,
            ListingOutcome::Listed {
                first_served: ymd(2016, 8, 5)
            }
        );
        assert_eq!(sw.calls, 2);
    }

    #[tokio::test]
    async fn screen_surfaces_all_empty_as_no_served_rows_never_delisting() {
        let f = ScriptedFetcher::new();
        f.script("999990", "20160801", "20160803", "", &[], "");
        f.script("999990", "20160804", "20160805", "", &[], "");
        let sw = screen_symbol(&f, "999990", &two_windows(), NO_PACE).await.unwrap();
        assert_eq!(
            sw.outcome,
            ListingOutcome::NoServedRows { windows_probed: 2 }
        );
    }

    #[tokio::test]
    async fn screen_failure_carries_calls_across_windows() {
        let f = ScriptedFetcher::new();
        // Window 1 empty (1 call), window 2 unscripted -> error; the failure
        // must carry the calls from BOTH windows.
        f.script("323410", "20160801", "20160803", "", &[], "");
        let err = screen_symbol(&f, "323410", &two_windows(), NO_PACE).await.unwrap_err();
        assert_eq!(err.calls, 2);
    }

    // -- set freeze ----------------------------------------------------------

    fn record(shcode: &str, market_class: MarketClass, cap_tier: CapTier) -> InstrumentMetadata {
        InstrumentMetadata {
            shcode: shcode.into(),
            market_class,
            market_cap: Resolved::Unavailable,
            cap_tier,
            turnover: Resolved::Unavailable,
            liquidity_tier: LiquidityTier::Unknown,
            index_membership: Resolved::Proxy(IndexMembership::NotMember),
            has_derivative: Resolved::Value(false),
            designation: None,
            tradable: true,
        }
    }

    fn universe(records: Vec<InstrumentMetadata>) -> UniverseMetadata {
        UniverseMetadata {
            provenance: MetadataProvenance {
                captured_at: "2026-07-23T00:00:00Z".into(),
                session_date: "20260723".into(),
                source_trs: vec!["t8430".into()],
                instrument_type_filter: "test".into(),
                tier_boundary_rule: "test".into(),
                cap_cutoffs: Vec::new(),
                paper_incompatible: Vec::new(),
            },
            records,
        }
    }

    #[test]
    fn freeze_takes_board_tiers_and_applies_the_preferred_rule() {
        let mut meta = universe(vec![
            record("005930", MarketClass::Kospi, CapTier::Top),
            record("005935", MarketClass::Kospi, CapTier::Top),
            record("323410", MarketClass::Kosdaq, CapTier::Mid),
            record("000020", MarketClass::Kospi, CapTier::BelowBoard),
        ]);
        let set = freeze_walk_set(&meta);
        assert_eq!(
            set.members.iter().map(|m| m.shcode.as_str()).collect::<Vec<_>>(),
            vec!["005930", "323410"]
        );
        assert_eq!(set.dropped_preferred, vec!["005935"]);
        assert!(set.dropped_malformed.is_empty());
        assert_eq!(set.source_content_hash, meta.content_hash());
        // The hash freezes the source: mutating a record changes it.
        meta.records[0].shcode = "005940".into();
        assert_ne!(freeze_walk_set(&meta).source_content_hash, set.source_content_hash);
    }

    // -- derive ---------------------------------------------------------------

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

    fn measurement(rows_per_page: &[usize]) -> MeasurementRecord {
        MeasurementRecord {
            shcode: "005930".into(),
            sdate: ymd(2016, 8, 1),
            edate: ymd(2016, 8, 10),
            pages: rows_per_page
                .iter()
                .enumerate()
                .map(|(i, &rows)| PageRecord {
                    page: i + 1,
                    rows,
                    first_date: None,
                    last_date: None,
                    rec_count_echo: rows.to_string(),
                    cursor_after: String::new(),
                })
                .collect(),
            calls: rows_per_page.len() as u32,
        }
    }

    #[test]
    fn derive_counts_effective_s_max_participation_and_measured_page_cap() {
        let sessions: Vec<NaiveDate> = (1..=10).map(|d| ymd(2016, 8, d)).collect();
        let symbols = vec![
            sym("000100", ListingOutcome::PreFloor),
            sym("000200", ListingOutcome::PreFloor),
            sym(
                "000300",
                ListingOutcome::Listed {
                    first_served: ymd(2016, 8, 6),
                },
            ),
            sym("000400", ListingOutcome::NoServedRows { windows_probed: 2 }),
        ];
        let d = derive(&symbols, &[measurement(&[500, 500, 137])], &sessions, &[2, 3, 4]).unwrap();
        assert_eq!(d.proven_sessions, 10);
        assert_eq!(d.symbols_counted, 3);
        assert_eq!(d.no_served_rows, vec!["000400"]);
        assert_eq!(d.listed_count_min, 2);
        // Even session count: nearest-rank p50 takes the LOWER middle
        // (counts sorted = [2,2,2,2,2,3,3,3,3,3] -> index 4 -> 2).
        assert_eq!(d.listed_count_median, 2);
        assert_eq!(d.listed_count_max, 3);
        assert_eq!(d.full_participation_symbols, 2);
        assert_eq!(d.max_observed_rows_per_page, 500);
        // Threshold 2: all 10 sessions. Threshold 3: the 5 sessions from 08-06.
        assert_eq!(d.thresholds[0].effective_s_max, 10);
        assert_eq!(d.thresholds[1].effective_s_max, 5);
        assert_eq!(d.thresholds[1].first_session_at_or_above, Some(ymd(2016, 8, 6)));
        // Threshold 4: never reached.
        assert_eq!(d.thresholds[2].effective_s_max, 0);
        assert_eq!(d.thresholds[2].first_session_at_or_above, None);
        assert!(d.thresholds[2].margin_bar_n1.is_nan());
        // Participation: (10 + 10 + 5) / 3 / 10.
        assert!((d.mean_participation - (25.0 / 30.0)).abs() < 1e-12);
    }

    #[test]
    fn derive_refuses_without_measurements_and_reproduces_the_holdout_bar() {
        let sessions: Vec<NaiveDate> = (1..=10).map(|d| ymd(2016, 8, d)).collect();
        let symbols = vec![sym("000100", ListingOutcome::PreFloor)];
        let err = derive(&symbols, &[], &sessions, &[2]).unwrap_err();
        assert!(err.to_string().contains("measurement"), "{err}");
        // All-NoServedRows refuses too: N(s) over zero resolved symbols is
        // not a statistic, and the anomaly set must be surfaced, not derived.
        let anomalies = vec![
            sym("000100", ListingOutcome::NoServedRows { windows_probed: 2 }),
            sym("000200", ListingOutcome::NoServedRows { windows_probed: 2 }),
        ];
        let err = derive(&anomalies, &[measurement(&[500])], &sessions, &[2]).unwrap_err();
        assert!(err.to_string().contains("no symbol resolved"), "{err}");
        // The frozen-constant anchor: the parent plan's holdout bar at 1,566
        // sessions is +0.028906.
        assert!((margin_bar_n1(1566) - 0.028906).abs() < 5e-6);
        // And the full-ceiling bar at 2,457 is +0.023077.
        assert!((margin_bar_n1(2457) - 0.023077).abs() < 5e-6);
    }
}
