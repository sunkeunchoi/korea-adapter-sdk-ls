//! Historical bar ingestion into a `ParquetDataCatalog` (U3).
//!
//! Per KTD4/KTD5/KTD9: an adapter-side per-TR [`pacer`] meters t8410/t8412 to the
//! stricter of their per-TR and category caps; **daily** bars (t8410) are walked on
//! the body `cts_date` cursor (which is exactly the checkpointing seam R5 needs);
//! **minute** bars (t8412) are driven page-by-page on the body `cts_date`/`cts_time`
//! cursor plus the `tr_cont: Y` request header (one `chart_page` dispatch per pacer
//! acquire — the SDK's `chart_all` bursts pages and walks headers the live gateway
//! terminates early), halving the chunk and requeueing on `PaginationLimit`, which
//! also fail-closes suspect partials (empty page with a live cursor, cursor echo).
//! Fetched pages are discarded on that error, so chunk sizing is the cost control.
//! LS returns KST
//! wall-clock strings; the adapter converts to UTC `UnixNanos` with `ts_event` =
//! **bar close** (Nautilus convention). Runs are resumable via [`checkpoint`].

pub mod budget;
pub mod checkpoint;
pub mod pacer;

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use ls_core::endpoint_policy::{T8410_POLICY, T8412_POLICY};
use ls_core::{HasPagination, LsError};
use ls_sdk::paginated::{
    T8410OutBlock1, T8410Request, T8410Response, T8412OutBlock1, T8412Request, T8412Response,
};
use ls_sdk::LsSdk;
use nautilus_core::UnixNanos;
use nautilus_model::data::{Bar, BarSpecification, BarType};
use nautilus_model::enums::{AggregationSource, BarAggregation, PriceType};
use nautilus_model::identifiers::InstrumentId;
use nautilus_model::types::{Price, Quantity};
use nautilus_ls_calendar::schema::DayStatus;
use nautilus_ls_calendar::{AsOfView, CalendarAdoption, DimensionStaleness};

use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use serde::{Deserialize, Serialize};

use crate::error::{AdapterError, AdapterResult};
use crate::lock::{AdvisoryLock, LockKind};
use crate::parse::strict_i64;
use crate::rules::{KRX_REGULAR_CLOSE, KST_UTC_OFFSET_HOURS};
use self::budget::{spend_ledger_path, BudgetModel, SpendLedger};
use self::checkpoint::{Checkpoint, CoverageGap, GapReason, RebaseEvent, RebaseOrigin};
use self::pacer::{Pacer, MARKET_DATA_CATEGORY_PER_SEC};

/// Current wall-clock as a unix timestamp (seconds) — the spend-ledger dispatch
/// stamp. Isolated so the ledger's bucketing is the only `Utc::now` seam in the
/// fetch path.
fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

/// A defensive upper bound on daily-cursor pages per symbol (guards a gateway that
/// never terminates the cursor).
const MAX_DAILY_PAGES: usize = 500;

/// Continuation-page cap for one minute chunk (mirrors ls-core's
/// `DEFAULT_MAX_PAGES` so the `PaginationLimit` split-and-requeue semantics in
/// `collect_minute` are unchanged by the page-by-page drive).
const MINUTE_MAX_PAGES: usize = 100;

/// Backstop on **consecutive** `IGW00201` backoff-and-narrow retries within one
/// `collect_minute` call (KTD5) — the counter resets on any successful fetch, so a
/// deep, healthy pull that keeps making progress never accumulates toward it; only
/// a run of throttles with no success in between (a dead/too-slow budget) climbs.
/// Narrowing a very wide range to a budget-sized chunk costs ~log2 consecutive
/// throttles, far under this bound. At the 120s backoff this caps the worst-case
/// wall-clock spent on one throttled sub-range at ~32×120s ≈ 64min; on exhaustion
/// the sub-range degrades to an uncovered thin gap (bars retained, watermark
/// withheld) rather than aborting the whole run.
const MAX_THROTTLE_RETRIES: usize = 32;

/// The default post-close safety buffer for the accumulate session-clock rule
/// (16:30 KST): today counts as a *closed* session only after this time, so a
/// post-close cron delivers the just-closed session rather than lagging a day, and
/// the watermark never advances into an in-session day (U5, KTD7).
pub const ACCUMULATE_CLOSE_BUFFER: NaiveTime = match NaiveTime::from_hms_opt(16, 30, 0) {
    Some(t) => t,
    None => unreachable!(),
};

/// The last **closed** trading session date for the current KST wall-clock (U5,
/// KTD7). Today counts as closed once now-KST is past the regular close plus a
/// safety buffer (`close_buffer`); otherwise the last closed session is
/// yesterday-or-earlier. Weekends/holidays with no session simply yield no new
/// bars (the gateway returns a coverage gap) while the watermark still advances.
pub fn last_closed_session(now_kst: NaiveDateTime, close_buffer: NaiveTime) -> NaiveDate {
    if now_kst.time() >= close_buffer {
        now_kst.date()
    } else {
        now_kst
            .date()
            .pred_opt()
            .expect("a date always has a predecessor")
    }
}

/// The calendar-derived decision for a single target civil date (U9, KTD8), independent of
/// the adoption posture. Computed purely from the injected calendar view (or its absence),
/// so Shadow can RECORD it while the weekday path acts and Enforced can ACT on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarDecision {
    /// A proven Trading Session: eligible to fetch (respecting the close buffer already
    /// folded into `last_closed`).
    Fetch,
    /// A proven Closed date: skip the gateway call and advance coverage FROM closure
    /// evidence (the provenance guard — never advanced on Unknown, KTD8).
    ClosedAdvance,
    /// A successful Unknown day fact: stop before dispatch, preserve state.
    UnknownStop,
    /// No calendar injected, or the date is out of range / erroring: stop before dispatch.
    UnavailableStop,
}

/// The action the ingest next-fetch path takes for a target date under the adoption seam
/// (U9, KTD8). Only Enforced ever yields anything but [`Proceed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateAction {
    /// Run the existing weekday-authoritative (Legacy/Shadow) or proven-session (Enforced)
    /// fetch path unchanged.
    Proceed,
    /// Enforced + proven Closed: advance the watermark to the target WITHOUT a gateway call.
    SkipAdvance,
    /// Enforced + Unknown/unavailable: stop before dispatch; preserve checkpoint + watermark
    /// byte-for-byte and issue zero gateway requests for the target.
    Stop,
}

/// The probe-anchor decision under the adoption seam (U9). Legacy/Shadow keep the weekday
/// anchor authoritative (Shadow records the calendar-selected one); Enforced replaces it with
/// the most recent proven Trading Session, or stops when none can be proven.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeAnchor {
    /// Probe using this anchor date.
    Use(NaiveDate),
    /// Enforced + Unknown/unavailable: do not probe (zero gateway requests, nothing recorded).
    Stop,
}

/// The consumer-owned proof plan for one inclusive pending span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CalendarRangePlan {
    /// Last positively-established Trading Session eligible as a request endpoint.
    pub request_through: Option<NaiveDate>,
    /// Last established date that may become covered after the request succeeds.
    pub advance_through: Option<NaiveDate>,
    /// First Unknown, unavailable, or out-of-coverage date, if scanning stopped.
    pub stop_before: Option<NaiveDate>,
}

impl CalendarRangePlan {
    /// A wipe-and-repull heal cannot commit a partial prefix because it deletes the
    /// whole series first. Admit it only when the complete planned span is established.
    fn destructive_request_through(self) -> Option<NaiveDate> {
        self.stop_before.is_none().then_some(self.request_through).flatten()
    }
}

/// The adoption-INDEPENDENT calendar continuity verdict for an OPEN civil-date interval —
/// the checkpoint merge-hole test (U10, KTD8). Legacy checkpoint ranges either side of a
/// gap merge into one watermark ONLY when every intervening date is a proven Closed date; a
/// proven Trading Session in the gap is un-attested history that must keep the ranges
/// separate; Unknown/unavailable evidence is treated conservatively (stay separate +
/// over-fetch) so newly-resolved evidence can re-chain the ranges later. Computed purely
/// from the injected view, so Shadow can RECORD it while the weekday hole test acts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinuityDecision {
    /// Every intervening date is a proven Closed date (or the gap is empty) — contiguous,
    /// ranges MERGE.
    AllClosed,
    /// A proven Trading Session lies in the gap — un-attested history, ranges STAY SEPARATE.
    TradingPresent,
    /// An Unknown or unavailable date lies in the gap with no proven Trading Session — the
    /// conservative over-fetch verdict: ranges STAY SEPARATE until the evidence resolves.
    Indeterminate,
}

impl ContinuityDecision {
    /// Whether this verdict BREAKS the migration chain (keeps the ranges separate). Only a
    /// fully-proven all-Closed gap chains; a proven Trading Session or indeterminate evidence
    /// breaks it (the conservative default — never fold un-attested or unproven history).
    pub fn breaks_chain(self) -> bool {
        !matches!(self, ContinuityDecision::AllClosed)
    }
}

/// The backward-widen warning action under the adoption seam (U10, KTD8). Legacy/Shadow keep
/// the unconditional weekday warning authoritative (Shadow records the calendar verdict);
/// Enforced gates the warning on proven calendar evidence in the pre-coverage region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidenAction {
    /// Emit + PERSIST the normal backward-widen warning (Legacy/Shadow always; Enforced when
    /// a proven Trading Session lies in the pre-coverage region — real un-fetched history).
    EmitPersist,
    /// Emit the DISTINCT NON-PERSISTED uncertainty warning (Enforced + any
    /// Unknown/unavailable evidence): do NOT record a history floor, so a later run with
    /// newly-resolved calendar evidence re-evaluates the region.
    EmitUncertain,
    /// Emit nothing (Enforced + an all-proven-Closed pre-coverage region — no trading history
    /// was missed, so there is nothing to widen).
    Suppress,
}

/// The per-consumer calendar seam injected into the ingest accumulate + lookback-probe
/// boundaries (KTD8). Carries the adoption posture and, when a snapshot loaded and
/// authorized, an [`AsOfView`]. `Copy` (the view is `Copy` — a borrow + an instant), so it
/// threads cheaply through the per-triple loop. Enforced-only after the ingest Consumer
/// Retirement Gate (#189 U6): construct [`new`](Self::new) at the composition root.
#[derive(Debug, Clone, Copy)]
pub struct CalendarGate<'c> {
    adoption: CalendarAdoption,
    view: Option<AsOfView<'c>>,
}

impl<'c> CalendarGate<'c> {
    /// Build a gate for `adoption` with an optional as-of view (`None` = calendar unavailable
    /// — a missing/failed snapshot; Enforced fails closed). Enforced-only after the ingest
    /// Consumer Retirement Gate (#189 U6): the decision methods act on the injected calendar
    /// regardless of `adoption`, and a missing view stops before dispatch.
    pub fn new(adoption: CalendarAdoption, view: Option<AsOfView<'c>>) -> Self {
        Self { adoption, view }
    }

    /// The adoption posture this gate runs under.
    pub fn adoption(&self) -> CalendarAdoption {
        self.adoption
    }

    /// Full-history freshness for conservative checkpoint continuity decisions.
    pub fn full_history_freshness(&self) -> Option<DimensionStaleness> {
        self.view.map(|view| view.freshness().full_history)
    }

    pub fn has_stale_evidence(&self) -> bool {
        self.view.map(|view| view.freshness().any_stale()).unwrap_or(false)
    }

    /// The adoption-INDEPENDENT calendar decision for `target` (U9). Consults the injected
    /// view: a proven Trading Session → [`Fetch`](CalendarDecision::Fetch), proven Closed →
    /// [`ClosedAdvance`](CalendarDecision::ClosedAdvance), a successful Unknown →
    /// [`UnknownStop`](CalendarDecision::UnknownStop), and a missing view or any
    /// out-of-range/query error → [`UnavailableStop`](CalendarDecision::UnavailableStop). This
    /// is the value Shadow records and Enforced acts on.
    pub fn calendar_decision(&self, target: NaiveDate) -> CalendarDecision {
        match self.view {
            None => CalendarDecision::UnavailableStop,
            Some(view) => match view.day(target) {
                Ok(fact) => match fact.status {
                    DayStatus::TradingSession => CalendarDecision::Fetch,
                    DayStatus::Closed => CalendarDecision::ClosedAdvance,
                    DayStatus::Unknown => CalendarDecision::UnknownStop,
                },
                Err(_) => CalendarDecision::UnavailableStop,
            },
        }
    }

    /// The next-fetch action for `target` (U9, KTD8). Enforced-only after the ingest Consumer
    /// Retirement Gate (#189 U6): the injected calendar decides — a proven Trading Session →
    /// [`Proceed`](GateAction::Proceed), a proven Closed date → [`SkipAdvance`](GateAction::SkipAdvance),
    /// and an Unknown or unavailable date → [`Stop`](GateAction::Stop) (fail closed).
    pub fn action(&self, target: NaiveDate) -> GateAction {
        match self.calendar_decision(target) {
            CalendarDecision::Fetch => GateAction::Proceed,
            CalendarDecision::ClosedAdvance => GateAction::SkipAdvance,
            CalendarDecision::UnknownStop | CalendarDecision::UnavailableStop => GateAction::Stop,
        }
    }

    /// The next-fetch action for the INCLUSIVE range `[start, last_closed]` under the adoption
    /// seam — the range-aware form of [`action`](Self::action) that guards advance-without-fetch
    /// against false coverage. The single-date [`action`](Self::action) only inspects the
    /// endpoint, but a SkipAdvance replaces a fetch of the WHOLE range `[start, last_closed]`
    /// (`start` = watermark+1, or the lookback floor for the initial backfill); skip-advancing
    /// when a Trading Session lies inside that span would mark it covered with zero bars. Scans
    /// every date in the range (reusing [`scan_inclusive`](Self::scan_inclusive)): an
    /// all-proven-Closed range (or a single proven-Closed date) →
    /// [`SkipAdvance`](GateAction::SkipAdvance); any proven Trading Session →
    /// [`Proceed`](GateAction::Proceed) (fetch the range, never skip a session); any
    /// Unknown/unavailable date with no proven session → [`Stop`](GateAction::Stop). Legacy/Shadow
    /// keep the weekday path authoritative (Shadow records the range verdict). Passing
    /// `start == last_closed` recovers the single-date semantics exactly.
    pub fn range_action(&self, start: NaiveDate, last_closed: NaiveDate) -> GateAction {
        match self.scan_inclusive(start, last_closed) {
            ContinuityDecision::AllClosed => GateAction::SkipAdvance,
            ContinuityDecision::TradingPresent => GateAction::Proceed,
            ContinuityDecision::Indeterminate => GateAction::Stop,
        }
    }

    fn established_prefix(&self, start: NaiveDate, ceiling: NaiveDate) -> CalendarRangePlan {
        let mut plan = CalendarRangePlan {
            request_through: None,
            advance_through: None,
            stop_before: None,
        };
        if start > ceiling {
            return plan;
        }
        let Some(view) = self.view else {
            plan.stop_before = Some(start);
            return plan;
        };
        let coverage = view.calendar().coverage();
        if start < coverage.materialized_from || start > coverage.materialized_through {
            plan.stop_before = Some(start);
            return plan;
        }
        let scan_end = ceiling.min(coverage.materialized_through);
        for row in view
            .calendar()
            .snapshot()
            .rows
            .iter()
            .filter(|row| row.date >= start)
            .take_while(|row| row.date <= scan_end)
        {
            match row.status {
                DayStatus::TradingSession => {
                    plan.request_through = Some(row.date);
                    plan.advance_through = Some(row.date);
                }
                DayStatus::Closed => plan.advance_through = Some(row.date),
                DayStatus::Unknown => {
                    plan.stop_before = Some(row.date);
                    break;
                }
            }
        }
        if plan.stop_before.is_none() && ceiling > coverage.materialized_through {
            plan.stop_before = coverage.materialized_through.succ_opt();
        }
        plan
    }

    /// Plan one pending accumulate span. Enforced-only after the ingest Consumer Retirement
    /// Gate (#189 U6): the plan acts only on the established prefix and never selects an
    /// endpoint beyond its first uncertainty.
    fn accumulate_plan(&self, start: NaiveDate, ceiling: NaiveDate) -> CalendarRangePlan {
        self.established_prefix(start, ceiling)
    }

    /// The most recent proven Trading Session at or before `anchor` in the injected view, or
    /// `None` if there is no view, no proven session, or an `Unknown` sits at/before the
    /// anchor with no proven session first (proof-preserving — an `Unknown` never manufactures
    /// a session anchor). Used only under Enforced.
    fn select_recent_session(&self, anchor: NaiveDate) -> Option<NaiveDate> {
        let view = self.view?;
        let coverage = view.calendar().coverage();
        if anchor < coverage.materialized_from || anchor > coverage.materialized_through {
            return None;
        }
        for row in view
            .calendar()
            .snapshot()
            .rows
            .iter()
            .rev()
            .skip_while(|row| row.date > anchor)
        {
            match row.status {
                DayStatus::TradingSession => return Some(row.date),
                DayStatus::Closed => {}
                DayStatus::Unknown => return None,
            }
        }
        None
    }

    /// The probe anchor (U9). Enforced-only after the ingest Consumer Retirement Gate (#189
    /// U6): the anchor is the most recent proven Trading Session at or before `anchor`, or
    /// [`Stop`](ProbeAnchor::Stop) when the calendar is unavailable or no session can be proven
    /// at/before it.
    pub fn probe_anchor(&self, anchor: NaiveDate) -> ProbeAnchor {
        match self.select_recent_session(anchor) {
            Some(d) => ProbeAnchor::Use(d),
            None => ProbeAnchor::Stop,
        }
    }

    /// Scan the half-open civil-date interval `[first, end_exclusive)` for the continuity
    /// verdict (U10). A proven Trading Session short-circuits to
    /// [`TradingPresent`](ContinuityDecision::TradingPresent) (the most conclusive break — real
    /// un-attested history); any Unknown/unavailable date with no proven session yields
    /// [`Indeterminate`](ContinuityDecision::Indeterminate); an all-proven-Closed (or empty)
    /// span yields [`AllClosed`](ContinuityDecision::AllClosed). Consults `calendar_decision`
    /// per date, so a missing view makes every probe unavailable → `Indeterminate` for any
    /// non-empty span.
    fn scan_continuity(&self, first: NaiveDate, end_exclusive: NaiveDate) -> ContinuityDecision {
        let mut d = first;
        let mut indeterminate = false;
        while d < end_exclusive {
            match self.calendar_decision(d) {
                CalendarDecision::Fetch => return ContinuityDecision::TradingPresent,
                CalendarDecision::ClosedAdvance => {}
                CalendarDecision::UnknownStop | CalendarDecision::UnavailableStop => {
                    indeterminate = true;
                }
            }
            d = match d.succ_opt() {
                Some(n) => n,
                None => break,
            };
        }
        if indeterminate {
            ContinuityDecision::Indeterminate
        } else {
            ContinuityDecision::AllClosed
        }
    }

    /// Scan the INCLUSIVE civil-date range `[start, last_closed]` for the continuity verdict,
    /// reusing [`scan_continuity`](Self::scan_continuity) over the half-open
    /// `[start, last_closed + 1)`. An empty range (`start > last_closed`) is vacuously
    /// [`AllClosed`](ContinuityDecision::AllClosed). This is the range form the accumulate
    /// advance-without-fetch guard scans (the fetch a SkipAdvance replaces covers the whole
    /// range, not just the endpoint).
    fn scan_inclusive(&self, start: NaiveDate, last_closed: NaiveDate) -> ContinuityDecision {
        if start > last_closed {
            return ContinuityDecision::AllClosed;
        }
        match last_closed.succ_opt() {
            Some(end_exclusive) => self.scan_continuity(start, end_exclusive),
            // `last_closed` is the maximum representable date (unreachable in practice); the
            // half-open span already covers every date strictly before it, so the residual
            // endpoint is the only date unscanned — treat the vacuous tail as all-Closed.
            None => self.scan_continuity(start, last_closed),
        }
    }

    /// The adoption-INDEPENDENT continuity verdict for the OPEN interval `(after, before)` —
    /// the checkpoint merge-hole test (U10). Walks each civil date strictly between the two.
    /// This is the value Shadow records and Enforced acts on (via
    /// [`breaks_chain`](ContinuityDecision::breaks_chain)).
    pub fn continuity_decision(&self, after: NaiveDate, before: NaiveDate) -> ContinuityDecision {
        match after.succ_opt() {
            Some(first) => self.scan_continuity(first, before),
            None => ContinuityDecision::AllClosed,
        }
    }

    /// The adoption-INDEPENDENT continuity verdict for the backward-widen pre-coverage region
    /// `[floor, earliest_stored)` (U10) — the half-open span accumulate would NOT fetch. This
    /// is the value Shadow records and Enforced acts on (via [`Self::widen_action`]).
    pub fn widen_evidence(
        &self,
        floor: NaiveDate,
        earliest_stored: NaiveDate,
    ) -> ContinuityDecision {
        if floor >= earliest_stored {
            return ContinuityDecision::AllClosed;
        }
        let Some(view) = self.view else {
            return ContinuityDecision::Indeterminate;
        };
        let coverage = view.calendar().coverage();
        let last = match earliest_stored.pred_opt() {
            Some(last) => last,
            None => return ContinuityDecision::Indeterminate,
        };
        if floor < coverage.materialized_from || last > coverage.materialized_through {
            return ContinuityDecision::Indeterminate;
        }
        let mut saw_session = false;
        let mut indeterminate = false;
        for row in view
            .calendar()
            .snapshot()
            .rows
            .iter()
            .filter(|row| row.date >= floor)
            .take_while(|row| row.date < earliest_stored)
        {
            match row.status {
                DayStatus::TradingSession => saw_session = true,
                DayStatus::Closed => {}
                DayStatus::Unknown => indeterminate = true,
            }
        }
        if indeterminate {
            ContinuityDecision::Indeterminate
        } else if saw_session {
            ContinuityDecision::TradingPresent
        } else {
            ContinuityDecision::AllClosed
        }
    }

    /// The backward-widen warning action for the pre-coverage region `[floor, earliest_stored)`
    /// (U10, KTD8). Enforced-only after the ingest Consumer Retirement Gate (#189 U6): acts on
    /// [`widen_evidence`](Self::widen_evidence) — a proven Trading Session emits + persists, an
    /// all-Closed region suppresses, and Unknown/unavailable emits the distinct non-persisted
    /// uncertainty warning.
    pub fn widen_action(&self, floor: NaiveDate, earliest_stored: NaiveDate) -> WidenAction {
        match self.widen_evidence(floor, earliest_stored) {
            ContinuityDecision::TradingPresent => WidenAction::EmitPersist,
            ContinuityDecision::AllClosed => WidenAction::Suppress,
            ContinuityDecision::Indeterminate => WidenAction::EmitUncertain,
        }
    }
}

/// Which bar series to ingest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarKind {
    /// Daily bars via t8410 (`gubun="2"`).
    Daily,
    /// N-minute bars via t8412.
    Minute(u32),
}

impl BarKind {
    /// A short label used in checkpoint keys + coverage (`1-DAY`, `1-MINUTE`).
    pub fn label(self) -> String {
        match self {
            BarKind::Daily => "1-DAY".to_string(),
            BarKind::Minute(n) => format!("{n}-MINUTE"),
        }
    }

    /// The nautilus [`BarType`] for this kind on `instrument_id` (External source —
    /// required by the backtest engine's `add_data(validate=true)`).
    pub fn bar_type(self, instrument_id: InstrumentId) -> AdapterResult<BarType> {
        let (step, agg) = match self {
            BarKind::Daily => (1usize, BarAggregation::Day),
            BarKind::Minute(n) => (n as usize, BarAggregation::Minute),
        };
        let spec = BarSpecification::new_checked(step, agg, PriceType::Last)
            .map_err(|e| AdapterError::Ingest(format!("bad bar spec {self:?}: {e}")))?;
        Ok(BarType::new(instrument_id, spec, AggregationSource::External))
    }
}

/// Convert a KST wall-clock date + time to a UTC [`UnixNanos`] (KTD9).
///
/// # Errors
///
/// [`AdapterError::FieldParse`] if the date/time cannot be resolved to a unique
/// instant.
pub fn kst_to_unix_nanos(date: NaiveDate, time: NaiveTime) -> AdapterResult<UnixNanos> {
    let naive = NaiveDateTime::new(date, time);
    let kst = FixedOffset::east_opt(KST_UTC_OFFSET_HOURS * 3600)
        .expect("KST offset is valid");
    let dt = match kst.from_local_datetime(&naive).single() {
        Some(dt) => dt,
        None => {
            return Err(AdapterError::FieldParse {
                field: "ts_event".to_string(),
                value: format!("{naive}"),
                reason: "ambiguous KST instant".to_string(),
            })
        }
    };
    let nanos = dt.timestamp_nanos_opt().ok_or_else(|| AdapterError::FieldParse {
        field: "ts_event".to_string(),
        value: format!("{naive}"),
        reason: "timestamp out of range".to_string(),
    })?;
    // `UnixNanos` is a u64; a pre-1970 instant (negative nanos) would wrap to a
    // far-future timestamp. Reject it rather than silently corrupting the bar.
    if nanos < 0 {
        return Err(AdapterError::FieldParse {
            field: "ts_event".to_string(),
            value: format!("{naive}"),
            reason: "pre-epoch timestamp (negative nanos)".to_string(),
        });
    }
    Ok(UnixNanos::from(nanos as u64))
}

fn parse_yyyymmdd(field: &str, s: &str) -> AdapterResult<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y%m%d").map_err(|e| AdapterError::FieldParse {
        field: field.to_string(),
        value: s.to_string(),
        reason: format!("expected YYYYMMDD: {e}"),
    })
}

/// Parse an LS intraday time field (`HHMMSS` or `HHMM`) to a [`NaiveTime`].
fn parse_hms(field: &str, s: &str) -> AdapterResult<NaiveTime> {
    let t = s.trim();
    let fmt = match t.len() {
        6 => "%H%M%S",
        4 => "%H%M",
        _ => {
            return Err(AdapterError::FieldParse {
                field: field.to_string(),
                value: s.to_string(),
                reason: "expected HHMM or HHMMSS".to_string(),
            })
        }
    };
    NaiveTime::parse_from_str(t, fmt).map_err(|e| AdapterError::FieldParse {
        field: field.to_string(),
        value: s.to_string(),
        reason: format!("bad time: {e}"),
    })
}

fn price_from_krw(field: &str, s: &str) -> AdapterResult<Price> {
    let i = strict_i64(field, s)?;
    Ok(Price::from(i.max(0).to_string().as_str()))
}

fn qty_from_str(field: &str, s: &str) -> AdapterResult<Quantity> {
    let i = strict_i64(field, s)?;
    Ok(Quantity::from(i.max(0)))
}

/// Build a daily [`Bar`] from a t8410 row. `ts_event` = the session close
/// (15:30 KST) of the candle date (KTD9).
pub fn build_daily_bar(bar_type: BarType, row: &T8410OutBlock1) -> AdapterResult<Option<Bar>> {
    if row.date.trim().is_empty() {
        return Ok(None);
    }
    let date = parse_yyyymmdd("date", &row.date)?;
    let ts = kst_to_unix_nanos(date, KRX_REGULAR_CLOSE)?;
    build_bar(bar_type, &row.open, &row.high, &row.low, &row.close, &row.jdiff_vol, ts)
}

/// Build a minute [`Bar`] from a t8412 row. `ts_event` = the candle's own KST
/// timestamp (its close), converted to UTC (KTD9).
pub fn build_minute_bar(bar_type: BarType, row: &T8412OutBlock1) -> AdapterResult<Option<Bar>> {
    if row.date.trim().is_empty() || row.time.trim().is_empty() {
        return Ok(None);
    }
    let date = parse_yyyymmdd("date", &row.date)?;
    let time = parse_hms("time", &row.time)?;
    let ts = kst_to_unix_nanos(date, time)?;
    build_bar(bar_type, &row.open, &row.high, &row.low, &row.close, &row.jdiff_vol, ts)
}

#[allow(clippy::too_many_arguments)]
fn build_bar(
    bar_type: BarType,
    open: &str,
    high: &str,
    low: &str,
    close: &str,
    volume: &str,
    ts: UnixNanos,
) -> AdapterResult<Option<Bar>> {
    let bar = Bar::new_checked(
        bar_type,
        price_from_krw("open", open)?,
        price_from_krw("high", high)?,
        price_from_krw("low", low)?,
        price_from_krw("close", close)?,
        qty_from_str("volume", volume)?,
        ts,
        ts,
    );
    match bar {
        Ok(b) => Ok(Some(b)),
        // A row whose OHLC violates high≥open≥low etc. is skipped rather than
        // failing the whole run (real feeds occasionally emit a degenerate row).
        Err(e) => {
            tracing::warn!(error = %e, "skipping malformed OHLC bar");
            Ok(None)
        }
    }
}

// ---------------------------------------------------------------------------
// Fetcher seams — the cursor/narrowing loops are generic over these so their
// failure modes (cursor non-termination, page-discarding cap) are unit-testable
// with fakes, while production fetches route through the SDK + pacer.
// ---------------------------------------------------------------------------

/// Fetches one daily-chart page for a symbol at a body cursor over `[sdate, edate]`.
#[async_trait]
pub trait DailyFetcher {
    /// Fetch the t8410 page for `[sdate, edate]` at `cts_date` (`""` = first page).
    /// Per-call dates (matching [`MinuteFetcher`]) so accumulate-forward can request
    /// a different range per instrument (U5).
    async fn fetch_daily_page(
        &self,
        shcode: &str,
        sdate: &str,
        edate: &str,
        cts_date: &str,
    ) -> AdapterResult<T8410Response>;

    /// The TR label used in ingest gateway-error context (R9). Defaults to the
    /// daily TR; test fakes inherit it.
    fn tr_label(&self) -> &'static str {
        "t8410"
    }

    /// The pacer's per-second cap for gateway-error context (R9). `0` = a fake
    /// with no pacer.
    fn pace_per_sec(&self) -> u32 {
        0
    }

    /// Backoff to wait after an `IGW00201` throttle before retrying a daily page
    /// (KTD-4). Daily is ~1 page per symbol, so the recovery arm backs off and
    /// retries the same page rather than narrowing. Default zero so test fakes
    /// retry instantly; `SdkFetcher` takes it from the measured budget model.
    fn throttle_backoff(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }
}

/// Fetches all minute-chart pages for a symbol over a date chunk.
#[async_trait]
pub trait MinuteFetcher {
    /// Fetch every t8412 page for `[sdate, edate]`. Returns
    /// [`LsError::PaginationLimit`] (wrapped) when the chunk exceeds the page cap.
    async fn fetch_minute_chunk(
        &self,
        shcode: &str,
        ncnt: u32,
        sdate: &str,
        edate: &str,
    ) -> AdapterResult<Vec<T8412Response>>;

    /// The TR label used in ingest gateway-error context (R9). Defaults to the
    /// minute TR; test fakes inherit it.
    fn tr_label(&self) -> &'static str {
        "t8412"
    }

    /// The pacer's per-second cap for gateway-error context (R9). `0` = a fake
    /// with no pacer.
    fn pace_per_sec(&self) -> u32 {
        0
    }

    /// Backoff to wait after an `IGW00201` throttle before narrowing and retrying a
    /// range (KTD5 drip-feed). `IGW00201` is a rolling call-count budget, so a pause
    /// lets it refill; the default is zero so test fakes narrow instantly.
    fn throttle_backoff(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }
}

/// Production fetcher over the SDK, paced per-TR (KTD4). Dates ride per call (U5),
/// so one fetcher serves both the range-global backfill and per-instrument
/// accumulate-forward ranges.
pub struct SdkFetcher {
    sdk: LsSdk,
    daily_pacer: Pacer,
    minute_pacer: Pacer,
    daily_qrycnt: usize,
    minute_qrycnt: usize,
    /// SHA-256 of the resolved appkey — the spend-ledger key (KTD-3).
    cred_hash: String,
    /// IGW00201 backoff from the measured budget model (R10/KTD-6). Read once at
    /// construction; the trait default (zero, for test fakes) is untouched.
    throttle_backoff: std::time::Duration,
    /// Shared in-process spend ledger; each gateway dispatch records against
    /// `cred_hash` at the pacer-acquire seam (KTD-3). `None` disables recording
    /// (always `Some` in production; the field lets a bare fetcher skip it).
    ledger: Option<Arc<Mutex<SpendLedger>>>,
}

impl SdkFetcher {
    fn new(
        sdk: LsSdk,
        cred_hash: String,
        throttle_backoff: std::time::Duration,
        ledger: Option<Arc<Mutex<SpendLedger>>>,
    ) -> Self {
        SdkFetcher {
            sdk,
            daily_pacer: Pacer::for_policy(&T8410_POLICY, MARKET_DATA_CATEGORY_PER_SEC),
            minute_pacer: Pacer::for_policy(&T8412_POLICY, MARKET_DATA_CATEGORY_PER_SEC),
            daily_qrycnt: 900,
            minute_qrycnt: 900,
            cred_hash,
            throttle_backoff,
            ledger,
        }
    }

    /// Record one gateway dispatch in the shared spend ledger (KTD-3) — called at
    /// each pacer-acquire seam, so the count matches the calls the gateway charges.
    /// Best-effort: a poisoned lock or absent ledger silently skips (advisory data).
    fn record_dispatch(&self) {
        if let Some(ledger) = &self.ledger {
            if let Ok(mut l) = ledger.lock() {
                l.record_spend(&self.cred_hash, now_unix());
            }
        }
    }

    /// Record a model-miss for each observed `IGW00201` dispatch (KTD-3): the
    /// gateway tripped the budget the model did not keep us under. Under the
    /// provisional model (no plan-ahead) every trip is unpredicted; under a measured
    /// model the planner defers first, so a trip here is a genuine miss. Counts each
    /// throttled dispatch (a dead-budget symbol contributes several) — a rough signal
    /// of how throttle-heavy the run was, never trusted over the gateway.
    fn note_if_throttled<T>(&self, result: &ls_core::LsResult<T>) {
        if let Err(LsError::ApiError { code, .. }) = result {
            if code == "IGW00201" {
                if let Some(ledger) = &self.ledger {
                    if let Ok(mut l) = ledger.lock() {
                        l.record_model_miss(&self.cred_hash);
                    }
                }
            }
        }
    }
}

#[async_trait]
impl DailyFetcher for SdkFetcher {
    async fn fetch_daily_page(
        &self,
        shcode: &str,
        sdate: &str,
        edate: &str,
        cts_date: &str,
    ) -> AdapterResult<T8410Response> {
        self.daily_pacer.acquire().await;
        self.record_dispatch();
        let mut req = T8410Request::new(
            shcode,
            "2", // daily
            self.daily_qrycnt.to_string(),
            sdate.to_string(),
            edate.to_string(),
        );
        req.inblock.cts_date = cts_date.to_string();
        let resp = self.sdk.paginated().stock_chart_period(&req).await;
        self.note_if_throttled(&resp);
        Ok(resp?)
    }

    fn pace_per_sec(&self) -> u32 {
        self.daily_pacer.per_sec_cap()
    }

    fn throttle_backoff(&self) -> std::time::Duration {
        self.throttle_backoff
    }
}

#[async_trait]
impl MinuteFetcher for SdkFetcher {
    async fn fetch_minute_chunk(
        &self,
        shcode: &str,
        ncnt: u32,
        sdate: &str,
        edate: &str,
    ) -> AdapterResult<Vec<T8412Response>> {
        // Drive the continuation page-by-page on the BODY `cts_date`/`cts_time`
        // cursor, mirroring `collect_daily` — with a pacer acquire per dispatch.
        // Two live-observed defects in the `chart_all` delegation this replaces:
        // (1) `collect_all` fires continuation pages back-to-back, tripping
        // t8412's 1/s gateway cap (IGW00201) — the runtime limiter is
        // per-category (5/s); (2) it walks the `tr_cont` HTTP headers, but the
        // live gateway terminates them after page 1 while more in-range rows
        // exist — t8412 self-paginates on the body cursor like t8410, so the
        // header walk silently truncated the range to its newest page.
        let mut req = T8412Request::new(
            shcode,
            ncnt.to_string(),
            self.minute_qrycnt.to_string(),
            "0",
            sdate,
            edate,
            "N",
        );
        let mut pages: Vec<T8412Response> = Vec::new();
        let mut seen = HashSet::new();
        for _ in 0..MINUTE_MAX_PAGES {
            self.minute_pacer.acquire().await;
            self.record_dispatch();
            let page = self.sdk.paginated().chart_page(&req).await;
            self.note_if_throttled(&page);
            let page = page?;
            let next_date = page.outblock.cts_date.trim().to_string();
            let next_time = page.outblock.cts_time.trim().to_string();
            let next_key = page.tr_cont_key().to_string();
            let empty_rows = page.outblock1.is_empty();
            if next_date.is_empty() {
                // A genuinely exhausted cursor is the ONLY clean completion
                // (live-verified: the working full-range walk ends this way).
                pages.push(page);
                return Ok(pages);
            }
            if empty_rows || !seen.insert((next_date.clone(), next_time.clone())) {
                // Suspect partial, fail closed: a zero-row page with a live
                // cursor (the gateway serves transiently empty pages off-hours)
                // or a re-served page (cursor echo — its rows would duplicate
                // ones already collected, so it is NOT pushed). Returning Ok
                // here would let collect_minute report complete Bars and the
                // checkpoint mark the truncated range done — the silent-
                // truncation class this drive exists to prevent. Surfacing
                // PaginationLimit instead sends collect_minute down its
                // split-and-requeue path; a range that stays broken narrows to
                // a single-day PaperThin gap, which withholds the watermark.
                return Err(AdapterError::Sdk(LsError::PaginationLimit(MINUTE_MAX_PAGES)));
            }
            pages.push(page);
            // A continuation needs BOTH the body cursor and the `tr_cont: Y`
            // request header — live, the gateway re-serves the newest page when
            // the header is absent, even with the cts cursor threaded.
            req.inblock.cts_date = next_date;
            req.inblock.cts_time = next_time;
            req.set_tr_cont("Y".to_string());
            req.set_tr_cont_key(next_key);
        }
        Err(AdapterError::Sdk(LsError::PaginationLimit(MINUTE_MAX_PAGES)))
    }

    fn pace_per_sec(&self) -> u32 {
        self.minute_pacer.per_sec_cap()
    }

    fn throttle_backoff(&self) -> std::time::Duration {
        self.throttle_backoff
    }
}

/// A conservative page-cost estimate for a triple's un-covered sub-ranges (AE3/
/// KTD-3): daily is ~1 page per symbol; minute is ~1 t8412 page per ~2 trading
/// sessions at qrycnt 900 (the drip anchor). Trading days are approximated from the
/// calendar span at 5/7. Used only by the pre-dispatch budget planner.
fn estimate_pages(kind: BarKind, sub_ranges: &[(NaiveDate, NaiveDate)]) -> u32 {
    let calendar_days: i64 = sub_ranges.iter().map(|(s, e)| (*e - *s).num_days() + 1).sum();
    let trading = ((calendar_days * 5 + 6) / 7).max(1);
    match kind {
        // Daily walks ~`daily_qrycnt` (900) rows/page on the cursor: ~1 page for a
        // short accumulate window, but ceil(sessions/900) for a deep first backfill —
        // never a flat 1, which would under-count a multi-page daily walk.
        BarKind::Daily => (((trading + 899) / 900).max(1)) as u32,
        BarKind::Minute(_) => (((trading + 1) / 2).max(1)) as u32,
    }
}

/// The outcome of ingesting one `(instrument, bar-kind)` triple.
enum TripleOutcome {
    Bars(Vec<Bar>),
    Gap(GapReason),
}

/// Walk the daily cursor for one symbol, collecting bars. Terminates on an empty
/// next-cursor, a repeated cursor (defensive), an empty page, the page cap, or an
/// `01715` (non-trading-day) error → coverage gap.
async fn collect_daily<F: DailyFetcher>(
    fetcher: &F,
    shcode: &str,
    bar_type: BarType,
    sdate: &str,
    edate: &str,
) -> AdapterResult<TripleOutcome> {
    let mut bars = Vec::new();
    let mut cts_date = String::new();
    let mut seen = HashSet::new();
    let mut hit_cap = true;
    // Consecutive IGW00201 throttles since the last successful page (reset on any
    // `Ok`), bounding a dead/too-slow budget exactly like `collect_minute` (KTD-4).
    let mut throttle_retries = 0usize;

    for page in 0..MAX_DAILY_PAGES {
        // Retry the SAME page on an IGW00201 throttle (KTD-4): daily is ~1 page per
        // symbol, so narrow-and-requeue adds nothing — back off (measured refill
        // window) and retry the same cursor. On a budget that stays dead past
        // MAX_THROTTLE_RETRIES consecutive throttles, degrade the symbol to a thin
        // gap (watermark withheld, re-fetched on a later cold budget) rather than
        // aborting the whole multi-symbol run — mirroring the minute arm's discipline.
        let resp = loop {
            match fetcher.fetch_daily_page(shcode, sdate, edate, &cts_date).await {
                Ok(r) => {
                    throttle_retries = 0; // forward progress resets the throttle bound
                    break r;
                }
                Err(AdapterError::Sdk(LsError::ApiError { code, .. })) if code == "01715" => {
                    return Ok(TripleOutcome::Gap(GapReason::NonTradingDay));
                }
                Err(AdapterError::Sdk(LsError::ApiError { code, .. })) if code == "IGW00201" => {
                    if throttle_retries >= MAX_THROTTLE_RETRIES {
                        // Dead budget: withhold the watermark for this symbol (thin
                        // gap), never abort the run. Partial daily pages already read
                        // are discarded — the symbol re-pulls whole on a cold budget.
                        return Ok(TripleOutcome::Gap(GapReason::PaperThin));
                    }
                    throttle_retries += 1;
                    tokio::time::sleep(fetcher.throttle_backoff()).await;
                    continue;
                }
                // R9: wrap a genuine gateway failure with locating context (TR code,
                // page index, pacer cap) — the control-flow codes above are handled
                // first, so anything here is a real failure worth localizing.
                Err(e) => {
                    return Err(with_gateway_context(e, fetcher.tr_label(), page + 1, fetcher.pace_per_sec()))
                }
            }
        };
        for row in &resp.outblock1 {
            if let Some(b) = build_daily_bar(bar_type, row)? {
                bars.push(b);
            }
        }
        let next = resp.outblock.cts_date.trim().to_string();
        if next.is_empty() || resp.outblock1.is_empty() || !seen.insert(next.clone()) {
            hit_cap = false;
            break;
        }
        cts_date = next;
    }

    if bars.is_empty() {
        return Ok(TripleOutcome::Gap(GapReason::EmptyHistory));
    }
    // LS daily charts return newest-first and the cursor walks recent→older, so
    // bars accumulate DESCENDING across pages. The catalog requires ascending
    // `ts_init` (the disjoint check is skipped on write), so sort before returning
    // — exactly as `collect_minute` does.
    bars.sort_by_key(|b| b.ts_init.as_u64());
    if hit_cap {
        // The page cap was reached without the cursor terminating — the returned
        // history is truncated, not complete. Surface it as paper-thin/uncertain
        // rather than claiming a full ingest.
        tracing::warn!(shcode, "daily cursor hit the {MAX_DAILY_PAGES}-page cap; history truncated");
        return Ok(TripleOutcome::Gap(GapReason::PaperThin));
    }
    Ok(TripleOutcome::Bars(bars))
}

/// Ingest minute bars for one symbol over `[sdate, edate]`, halving the chunk and
/// requeueing on `PaginationLimit` (KTD5). A single-day chunk that still overflows
/// is recorded as a paper-thin/uningestable gap and skipped.
async fn collect_minute<F: MinuteFetcher>(
    fetcher: &F,
    shcode: &str,
    ncnt: u32,
    bar_type: BarType,
    sdate: &str,
    edate: &str,
) -> AdapterResult<TripleOutcome> {
    let start = parse_yyyymmdd("sdate", sdate)?;
    let end = parse_yyyymmdd("edate", edate)?;
    let mut bars = Vec::new();
    // A sub-range we could not fully cover (a single day that overflowed the page
    // cap, or a range the throttle budget stayed dead on): if the symbol ends up with
    // no bars at all this reports a thin gap rather than empty history, and either
    // way the watermark is withheld for the uncovered span.
    let mut left_uncovered_gap = false;
    let mut chunk = 0usize;
    // Consecutive IGW00201 throttles since the last successful fetch (reset on any
    // `Ok`). Bounds a dead/too-slow budget; a healthy pull that keeps progressing
    // never accumulates toward MAX_THROTTLE_RETRIES.
    let mut throttle_retries = 0usize;
    let mut queue: VecDeque<(NaiveDate, NaiveDate)> = VecDeque::new();
    queue.push_back((start, end));

    while let Some((s, e)) = queue.pop_front() {
        chunk += 1;
        let s_str = s.format("%Y%m%d").to_string();
        let e_str = e.format("%Y%m%d").to_string();
        match fetcher.fetch_minute_chunk(shcode, ncnt, &s_str, &e_str).await {
            Ok(pages) => {
                throttle_retries = 0; // forward progress resets the throttle bound
                for page in &pages {
                    for row in &page.outblock1 {
                        if let Some(b) = build_minute_bar(bar_type, row)? {
                            bars.push(b);
                        }
                    }
                }
            }
            Err(AdapterError::Sdk(LsError::PaginationLimit(_))) => {
                // Too many pages for this span — narrow and retry; a single day that
                // still overflows is an uncoverable thin gap.
                if !requeue_halves(&mut queue, s, e) {
                    left_uncovered_gap = true;
                }
            }
            // IGW00201 is the gateway's rolling call-count budget (KTD5), NOT a
            // page-size problem: back off to let it refill, then narrow-and-requeue so
            // the retry unit is smaller and fits the refilled window. Bars already
            // collected from completed chunks are retained. On a budget that stays
            // dead past MAX_THROTTLE_RETRIES *consecutive* throttles, degrade this
            // sub-range to an uncovered thin gap (keeping the bars gathered so far)
            // instead of aborting the whole multi-symbol run — the deep-pull dead-end
            // this arm exists to prevent must not be re-introduced by the exhaustion
            // path. A later cold-budget re-run re-fetches the withheld span.
            Err(AdapterError::Sdk(LsError::ApiError { code, .. })) if code == "IGW00201" => {
                if throttle_retries >= MAX_THROTTLE_RETRIES {
                    left_uncovered_gap = true;
                } else {
                    throttle_retries += 1;
                    tokio::time::sleep(fetcher.throttle_backoff()).await;
                    // A single day that still throttles is retried as-is (bounded by
                    // the counter); anything wider narrows first.
                    if !requeue_halves(&mut queue, s, e) {
                        queue.push_front((s, e));
                    }
                }
            }
            Err(AdapterError::Sdk(LsError::ApiError { code, .. })) if code == "01715" => {
                // Non-trading sub-range — skip it, keep the rest.
            }
            // R9: wrap a genuine gateway failure with locating context (the
            // pagination/throttle/non-trading control-flow codes above are handled
            // first, and the throttle now degrades to a gap rather than propagating).
            Err(e) => {
                return Err(with_gateway_context(e, fetcher.tr_label(), chunk, fetcher.pace_per_sec()))
            }
        }
    }

    if !bars.is_empty() {
        // Bars may arrive out of order across chunks; sort by ts_event ascending
        // (the catalog requires ascending ts_init).
        bars.sort_by_key(|b| b.ts_init.as_u64());
        Ok(TripleOutcome::Bars(bars))
    } else if left_uncovered_gap {
        Ok(TripleOutcome::Gap(GapReason::PaperThin))
    } else {
        Ok(TripleOutcome::Gap(GapReason::EmptyHistory))
    }
}

/// Wrap a propagating gateway error with ingest context (R9): the TR code, the
/// page/chunk index, and the pacer's per-second cap. Only an SDK gateway error
/// is wrapped — the ingest control-flow codes (`01715`, `PaginationLimit`) are
/// handled by the caller before reaching a propagation seam, so anything here
/// is a real failure; a non-SDK error (e.g. a field-parse) passes through
/// unchanged. The wrapped [`LsError`] stays reachable via `Error::source`.
fn with_gateway_context(
    e: AdapterError,
    tr: &'static str,
    page: usize,
    per_sec: u32,
) -> AdapterError {
    match e {
        AdapterError::Sdk(inner) => AdapterError::IngestGateway {
            tr,
            page,
            per_sec,
            source: Box::new(inner),
        },
        other => other,
    }
}

/// Split a `[s, e]` date range into two halves. Returns `None` if `s == e` (a
/// single day cannot be narrowed).
fn split_range(s: NaiveDate, e: NaiveDate) -> Option<((NaiveDate, NaiveDate), (NaiveDate, NaiveDate))> {
    if s >= e {
        return None;
    }
    let span = (e - s).num_days();
    let mid = s + ChronoDuration::days(span / 2);
    if mid >= e {
        // Adjacent days — split into [s,s] and [e,e].
        Some(((s, s), (e, e)))
    } else {
        Some(((s, mid), (mid + ChronoDuration::days(1), e)))
    }
}

/// Split `[s, e]` into narrower halves and requeue them at the FRONT (finish this
/// range before moving on — keeps memory bounded). Returns `false` when the range
/// is already a single day and cannot be narrowed further. Shared by the
/// `PaginationLimit` (page-size) and `IGW00201` (throttle) recovery arms of
/// [`collect_minute`], which narrow identically.
fn requeue_halves(queue: &mut VecDeque<(NaiveDate, NaiveDate)>, s: NaiveDate, e: NaiveDate) -> bool {
    match split_range(s, e) {
        Some((left, right)) => {
            queue.push_front(right);
            queue.push_front(left);
            true
        }
        None => false,
    }
}

// ---------------------------------------------------------------------------
// Basis-shift detection (KTD-3) — the adjusted daily series is rewritten
// server-side by every split/dividend, so accumulate-forward re-fetches a bounded
// overlap window and exact-compares it against stored bars before appending.
// ---------------------------------------------------------------------------

/// Default overlap-window size: the last N stored trading days ending at the
/// watermark (the `IngestConfig::overlap_days` knob).
pub const DEFAULT_OVERLAP_DAYS: usize = 5;

/// Minimum mutually-present dates for an overlap comparison to be meaningful
/// (KTD-3): fewer — including the no-watermark first-ever accumulate — skips
/// detection entirely rather than marking.
const MIN_OVERLAP_DATES: usize = 3;

/// The calendar start of the overlap window: wide enough that `overlap_days`
/// *trading* days ending at the watermark fit despite weekends/holiday clusters
/// (Seollal/Chuseok). A symbol suspended longer than this simply yields an
/// insufficient overlap and skips detection.
fn overlap_window_start(watermark: NaiveDate, overlap_days: usize) -> NaiveDate {
    watermark - ChronoDuration::days(overlap_days as i64 * 3 + 10)
}

/// The verdict of an overlap comparison (KTD-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverlapVerdict {
    /// Enough mutually-present dates and every OHLC matches exactly.
    Match,
    /// At least one mutually-present date differs — the basis shifted.
    Shifted,
    /// Fewer than [`MIN_OVERLAP_DATES`] mutually-present dates (short history,
    /// long suspension, truncated fetch) — detection is skipped, never marked.
    Insufficient,
}

/// The rows belonging to the last `n` **distinct** `ts_event` values of a series
/// — the overlap tail (KTD-4). Detection and the heal's re-verify MUST prepare
/// their stored side through this one helper: the re-verify exists to re-check the
/// invariant detection compared, so the two tails must be defined identically.
///
/// Keying on distinct sessions rather than raw `Vec` length is load-bearing: a
/// duplicate-polluted catalog (byte-identical re-pull rows for the same session)
/// would otherwise crowd distinct earlier sessions out of a length-based window,
/// diluting `compare_overlap`'s mutual-date count below `MIN_OVERLAP_DATES` and
/// silently suppressing a genuine basis shift. All rows of a kept session are
/// retained (including any same-`ts_event` divergent copy), so the caller can
/// still see an intra-session divergence.
fn overlap_tail(bars: Vec<Bar>, n: usize) -> Vec<Bar> {
    let mut distinct: Vec<u64> = bars.iter().map(|b| b.ts_event.as_u64()).collect();
    distinct.sort_unstable();
    distinct.dedup();
    if distinct.len() <= n {
        return bars;
    }
    // The first `ts_event` to keep = the start of the last `n` distinct sessions.
    let cutoff = distinct[distinct.len() - n];
    bars.into_iter().filter(|b| b.ts_event.as_u64() >= cutoff).collect()
}

/// The stored-side sufficiency decision for [`Ingestor::detect_shift`] (KTD-4).
enum TailGate {
    /// The tail carries value-divergent same-`ts_event` rows (surviving
    /// byte-identical dedup) — a basis-shift/mutation signal directly.
    ForceShift,
    /// Fewer than [`MIN_OVERLAP_DATES`] distinct sessions — skip detection.
    Insufficient,
    /// Enough distinct, non-divergent sessions — proceed to the overlap compare.
    Compare,
}

/// Gate an overlap tail on DISTINCT sessions, not raw rows (KTD-4). Byte-identical
/// duplicates are collapsed first (a redundant re-pull is not divergence); a
/// timestamp that still carries more than one row is value-divergent and forces a
/// shift verdict, because `compare_overlap`'s per-timestamp map would keep only the
/// last-inserted copy and hide it (read-order dependent). Otherwise sufficiency is
/// the count of distinct sessions.
fn stored_overlap_gate(tail: &[Bar]) -> TailGate {
    let mut deduped = tail.to_vec();
    dedup_bars(&mut deduped);
    let mut per_ts: BTreeMap<u64, usize> = BTreeMap::new();
    for b in &deduped {
        *per_ts.entry(b.ts_event.as_u64()).or_default() += 1;
    }
    if per_ts.values().any(|&c| c > 1) {
        TailGate::ForceShift
    } else if per_ts.len() < MIN_OVERLAP_DATES {
        TailGate::Insufficient
    } else {
        TailGate::Compare
    }
}

fn ohlc_by_ts(bars: &[Bar]) -> BTreeMap<u64, (Price, Price, Price, Price)> {
    bars.iter()
        .map(|b| (b.ts_event.as_u64(), (b.open, b.high, b.low, b.close)))
        .collect()
}

/// Exact-compare stored vs freshly-fetched bars on mutually-present dates only
/// (KTD-3). One-sided dates — gap/holiday days with no stored bar, a server-side
/// gap-fill, a dropped bar — are excluded: they are not a basis shift, and
/// including them would re-detect forever. Daily closes are integer KRW on the
/// wire, so the comparison is exact-match, not tolerance-based.
fn compare_overlap(stored: &[Bar], fetched: &[Bar]) -> OverlapVerdict {
    let stored = ohlc_by_ts(stored);
    let fetched = ohlc_by_ts(fetched);
    let mut mutual = 0usize;
    let mut shifted = false;
    for (ts, s) in &stored {
        if let Some(f) = fetched.get(ts) {
            mutual += 1;
            if s != f {
                shifted = true;
            }
        }
    }
    if mutual < MIN_OVERLAP_DATES {
        OverlapVerdict::Insufficient
    } else if shifted {
        OverlapVerdict::Shifted
    } else {
        OverlapVerdict::Match
    }
}

/// Fetch the server's overlap window `[wstart, wend]` for comparison. A gap
/// outcome (empty history, non-trading range, truncated fetch) yields `None` —
/// nothing trustworthy to compare, so detection is skipped (KTD-3).
async fn fetch_overlap<F: DailyFetcher>(
    fetcher: &F,
    shcode: &str,
    bar_type: BarType,
    wstart: NaiveDate,
    wend: NaiveDate,
) -> AdapterResult<Option<Vec<Bar>>> {
    let sdate = fmt_ymd(wstart);
    let edate = fmt_ymd(wend);
    match collect_daily(fetcher, shcode, bar_type, &sdate, &edate).await? {
        TripleOutcome::Bars(bars) => Ok(Some(bars)),
        TripleOutcome::Gap(_) => Ok(None),
    }
}

/// A refused heal wipe (KTD-2 precondition): the run's backfill floor is later
/// than the symbol's earliest stored bar, so wiping would silently truncate
/// stored history. The symbol stays marked until a run with an adequate floor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealRefusal {
    /// The instrument id (`{shcode}.XKRX`).
    pub instrument: String,
    /// The bar-type label (e.g. `1-DAY`).
    pub bar_type: String,
    /// The run's backfill floor (`YYYYMMDD`).
    pub floor: String,
    /// The symbol's earliest stored bar date (`YYYYMMDD`).
    pub earliest_stored: String,
}

/// The outcome of one per-symbol heal attempt.
enum HealOutcome {
    /// Wipe → re-pull → re-verify completed; the mark is cleared and the event
    /// recorded. Carries the number of bars re-written.
    Healed(usize),
    /// The wipe precondition refused (floor later than earliest stored bar).
    Refused(HealRefusal),
    /// The re-pull was truncated or the re-verify mismatched — the mark stays
    /// and the next run re-enters at the wipe.
    Incomplete,
    /// The re-pull append hit a fail-closed interval-overlap refusal (#104/R7).
    /// The wipe guarantees the re-pull is disjoint by construction, so this is a
    /// defensive catch for an unforeseen overlap (a delete that failed to clear,
    /// residual pollution): route it to the append-refusal vec per-triple, keep
    /// the mark, and let the run continue instead of aborting via a propagated
    /// fatal error.
    AppendRefused(AppendRefusal),
}

/// A per-run request-budget estimate (R4/KTD5).
#[derive(Debug, Clone)]
pub struct BudgetEstimate {
    /// Symbols in the universe.
    pub symbols: usize,
    /// Bar kinds requested per symbol.
    pub bar_kinds: usize,
    /// Requests-per-second cap the run pace to (the stricter per-TR cap).
    pub per_sec_cap: u32,
    /// A conservative lower bound on total requests (one page per triple).
    pub min_requests: usize,
}

impl BudgetEstimate {
    /// A lower-bound wall-clock estimate at the per-second cap.
    pub fn min_seconds(&self) -> f64 {
        if self.per_sec_cap == 0 {
            return f64::INFINITY;
        }
        self.min_requests as f64 / self.per_sec_cap as f64
    }
}

/// A range-mode series refused because it carries an unhealed basis-shift mark
/// (U4/KTD8): serving or completing it in range mode would put bars on a stale
/// adjustment basis — the exact corruption the heal machinery prevents. Range
/// mode does not heal; it refuses and directs the operator to accumulate/rebase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeRefusal {
    /// The instrument id (`{shcode}.XKRX`).
    pub instrument: String,
    /// The bar-type label (e.g. `1-DAY`).
    pub bar_type: String,
    /// The session date (`YYYYMMDD`) the shift was detected.
    pub detected: String,
}

/// A production append refused fail-closed because its date range overlaps
/// coverage already stored for the same `(instrument, bar-kind)` series (R5/KTD-1).
/// The refusal is per-triple, not run-fatal: the watermark does not advance, the
/// run continues, and the entry is surfaced (never silent) — the `ls-ingest` bin
/// prints it beside the other refusal vecs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendRefusal {
    /// The instrument id (`{shcode}.XKRX`).
    pub instrument: String,
    /// The bar-type label (e.g. `1-DAY`).
    pub bar_type: String,
    /// The attempted write's KST date range (`sdate..edate`).
    pub attempted: String,
    /// The overlapping stored coverage range(s) that triggered the refusal.
    pub stored: String,
}

/// A backward-widen loud no-op (R4/KTD-6): a triple whose configured lookback
/// floor precedes its earliest stored coverage, so the pre-coverage region will
/// not be fetched. The watermark carries no coverage start, so this is the
/// operator's signal to use the escape hatch (a fresh catalog at the wider
/// lookback, or wipe + full re-pull) — surfaced, never silent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackwardWidenWarning {
    /// The instrument id (`{shcode}.XKRX`).
    pub instrument: String,
    /// The bar-type label (e.g. `1-DAY`).
    pub bar_type: String,
    /// The configured lookback floor (`YYYYMMDD`).
    pub floor: String,
    /// The earliest stored coverage date (`YYYYMMDD`) the floor precedes.
    pub earliest_stored: String,
}

/// A backward-widen uncertainty (U10/KTD8, Enforced only): the pre-coverage region
/// `[floor, earliest_stored)` contains an Unknown or unavailable calendar date, so whether
/// the complete interval holds un-fetched trading history is undetermined. Unlike
/// [`BackwardWidenWarning`], this is deliberately NOT persisted (no `history_floor` is
/// recorded), so a later run with newly-resolved calendar evidence re-evaluates the region
/// and can escalate it to a real warning or clear it. Surfaced, never silent; never reddens
/// CI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackwardWidenUncertainty {
    /// The instrument id (`{shcode}.XKRX`).
    pub instrument: String,
    /// The bar-type label (e.g. `1-DAY`).
    pub bar_type: String,
    /// The configured lookback floor (`YYYYMMDD`).
    pub floor: String,
    /// The earliest stored coverage date (`YYYYMMDD`) the floor precedes.
    pub earliest_stored: String,
    /// Whether another calendar freshness dimension was stale at the fixed as-of instant.
    pub calendar_stale: bool,
}

/// A symbol/triple deferred pre-dispatch because its estimated page cost exceeds
/// the remaining measured budget (AE3/KTD-3): the ingest stopped before dispatching,
/// preserved the checkpoint unchanged, and scheduled the remainder — no IGW00201 was
/// provoked. Informational like a backward-widen warning: it never reddens CI, and a
/// later cold-window run resumes it. Only ever populated under a measured budget
/// model (`budget_calls: Some`); inert under the provisional default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetDeferral {
    /// The instrument id (`{shcode}.XKRX`).
    pub instrument: String,
    /// The bar-type label (e.g. `1-MINUTE`).
    pub bar_type: String,
    /// The estimated page cost of the deferred triple.
    pub estimated_pages: u32,
    /// The budget remaining in the current window when it was deferred.
    pub remaining_budget: u32,
}

/// The result of an ingest run.
#[derive(Debug, Clone)]
pub struct CoverageReport {
    /// Total bars written across all triples.
    pub bars_written: usize,
    /// Triples that produced bars.
    pub triples_ingested: usize,
    /// Triples skipped because the checkpoint already had them.
    pub triples_skipped: usize,
    /// Coverage gaps recorded this run.
    pub gaps: Vec<checkpoint::CoverageGap>,
    /// Heal wipes refused this run (KTD-2 precondition) — surfaced, never silent.
    pub heal_refusals: Vec<HealRefusal>,
    /// Range-mode series refused pending heal (U4/KTD8) — a marked series is
    /// refused, never served or completed on a stale basis, and never silent.
    pub range_refusals: Vec<RangeRefusal>,
    /// Appends refused fail-closed for interval overlap (R5/KTD-1) — a per-triple
    /// refusal that does not advance the watermark or abort the run.
    pub append_refusals: Vec<AppendRefusal>,
    /// Backward-widen loud no-ops (R4/KTD-6): triples whose lookback floor precedes
    /// their earliest stored coverage, so the pre-coverage region is unreachable.
    pub backward_widen_warnings: Vec<BackwardWidenWarning>,
    /// Backward-widen uncertainties (U10/KTD8, Enforced only): triples whose pre-coverage
    /// region has Unknown/unavailable calendar evidence and no proven Trading Session —
    /// surfaced as a DISTINCT non-persisted warning so a later run with resolved evidence
    /// re-evaluates. Always empty under Legacy/Shadow (weekday warning authoritative).
    pub backward_widen_uncertainties: Vec<BackwardWidenUncertainty>,
    /// Triples deferred pre-dispatch under a measured budget (AE3/KTD-3) — stopped
    /// before the cliff with the remainder scheduled, never provoking IGW00201.
    pub budget_deferrals: Vec<BudgetDeferral>,
    /// The request-budget estimate for the run.
    pub budget: BudgetEstimate,
}

/// Ingestion configuration.
#[derive(Debug, Clone)]
pub struct IngestConfig {
    /// Directory the `ParquetDataCatalog` + checkpoint + lockfile live in.
    pub catalog_path: PathBuf,
    /// Which bar series to ingest per symbol.
    pub bar_kinds: Vec<BarKind>,
    /// Range start (`YYYYMMDD`, a trading day) for minute chunks.
    pub sdate: String,
    /// Range end (`YYYYMMDD`, a trading day) for minute chunks.
    pub edate: String,
    /// Whether daily bars used adjusted prices (`sujung="Y"`, recorded in the
    /// checkpoint as the catalog price basis).
    pub adjusted_prices: bool,
    /// Basis-shift detection overlap: the last N stored trading days ending at
    /// the watermark are re-fetched and exact-compared before appending (KTD-3).
    /// Use [`DEFAULT_OVERLAP_DAYS`] unless a test needs otherwise.
    pub overlap_days: usize,
}

impl IngestConfig {
    fn checkpoint_path(&self) -> PathBuf {
        self.catalog_path.join("ingest-checkpoint.json")
    }
}

/// The historical-bar ingestor. Holds the SDK-backed fetcher, the catalog path,
/// and the resumable checkpoint. `ls-ingest` is the only entry point (it also
/// takes the R15 advisory lock — see [`Ingestor::run_locked`]).
pub struct Ingestor {
    fetcher: SdkFetcher,
    config: IngestConfig,
    /// Shared per-credential spend ledger (KTD-3): the fetcher records each
    /// dispatch into it, the accumulate planner reads it, and it is persisted after
    /// each run so a run interrupted by budget exhaustion resumes across sessions.
    ledger: Arc<Mutex<SpendLedger>>,
    /// Where [`Self::ledger`] persists (env override or `<catalog>/../state/`).
    ledger_path: PathBuf,
    /// The measured IGW00201 budget model (fail-open provisional until U6).
    budget_model: BudgetModel,
    /// This lane's credential-hash ledger key.
    cred_hash: String,
}

impl Ingestor {
    /// Build an ingestor over an SDK handle and config. Resolves the credential
    /// hash, loads the (fail-open) budget model, and loads + prunes the shared
    /// spend ledger — all advisory: an absent model/ledger keeps today's behavior.
    pub fn new(sdk: LsSdk, config: IngestConfig) -> Self {
        let cred_hash = SpendLedger::hash_appkey(&sdk.inner().config.appkey);
        let budget_model = BudgetModel::load_default();
        let ledger_path = spend_ledger_path(&config.catalog_path);
        let cutoff = now_unix() - budget_model.window_secs;
        let ledger = Arc::new(Mutex::new(SpendLedger::load_pruned(&ledger_path, cutoff)));
        let fetcher = SdkFetcher::new(
            sdk,
            cred_hash.clone(),
            budget_model.throttle_backoff(),
            Some(Arc::clone(&ledger)),
        );
        Ingestor {
            fetcher,
            config,
            ledger,
            ledger_path,
            budget_model,
            cred_hash,
        }
    }

    /// Persist the shared spend ledger (best-effort, advisory): a failure warns but
    /// never fails the run. Called at the end of a run so the next invocation (the
    /// per-symbol drip re-invokes the binary) sees this run's spend.
    fn save_ledger(&self) {
        if let Ok(l) = self.ledger.lock() {
            if let Err(e) = l.save(&self.ledger_path) {
                tracing::warn!(
                    path = %self.ledger_path.display(),
                    error = %e,
                    "failed to persist spend ledger (advisory; ingest unaffected)"
                );
            }
        }
    }

    /// Run ingestion while holding the R15 ingest lock (refuses if a live session
    /// is running). Releases the lock on return.
    pub async fn run_locked(&mut self, universe: &[InstrumentId]) -> AdapterResult<CoverageReport> {
        let _lock = AdvisoryLock::acquire(&self.config.catalog_path, LockKind::Ingest)?;
        self.run(universe).await
    }

    /// Run ingestion over `universe` into the catalog, resuming from the
    /// checkpoint. (Does not take the lock — use [`Self::run_locked`] for the
    /// entry-point path; this is exposed for tests that drive it directly.)
    pub async fn run(&mut self, universe: &[InstrumentId]) -> AdapterResult<CoverageReport> {
        std::fs::create_dir_all(&self.config.catalog_path).map_err(|e| {
            AdapterError::Ingest(format!("mkdir catalog {}: {e}", self.config.catalog_path.display()))
        })?;
        let checkpoint_path = self.config.checkpoint_path();
        // Range mode is the manual `SDATE`/`EDATE` path — it never consults the calendar for a
        // date decision. Enforced-only after the ingest Consumer Retirement Gate (#189 U6): the
        // legacy `completed`→`watermark` migration on load runs under a no-view gate (via
        // `Checkpoint::load`), which keeps legacy ranges conservatively separate (a fresh
        // catalog has nothing to migrate).
        let mut checkpoint = Checkpoint::load(&checkpoint_path)?;
        checkpoint.adjusted_prices = self.config.adjusted_prices;

        let range = format!("{}..{}", self.config.sdate, self.config.edate);
        let mut bars_written = 0usize;
        let mut ingested = 0usize;
        let mut skipped = 0usize;
        let mut gaps_this_run = Vec::new();
        let mut range_refusals: Vec<RangeRefusal> = Vec::new();
        let mut append_refusals: Vec<AppendRefusal> = Vec::new();
        let mut budget_deferrals: Vec<BudgetDeferral> = Vec::new();
        // Parsed once for the pre-dispatch budget planner (AE3/KTD-3). `None` if the
        // range dates don't parse (e.g. a probe-style empty range) → planning skipped.
        let range_span: Option<(NaiveDate, NaiveDate)> = match (
            parse_yyyymmdd("sdate", &self.config.sdate),
            parse_yyyymmdd("edate", &self.config.edate),
        ) {
            (Ok(s), Ok(e)) => Some((s, e)),
            _ => None,
        };

        for id in universe {
            let shcode = id.symbol.as_str().to_string();
            for &kind in &self.config.bar_kinds {
                let label = kind.label();
                // The shifted mark outranks the completion check (KTD8): a marked
                // series is refused pending heal BEFORE `is_done`, so a series
                // already recorded complete for this range is still not served on a
                // stale adjustment basis. Range mode never heals — it refuses and
                // directs the operator to accumulate/rebase mode. Unmarked series
                // in the same run are unaffected.
                if checkpoint.is_shifted(&id.to_string(), &label) {
                    range_refusals.push(RangeRefusal {
                        instrument: id.to_string(),
                        bar_type: label.clone(),
                        detected: checkpoint
                            .shifted_detected(&id.to_string(), &label)
                            .unwrap_or("")
                            .to_string(),
                    });
                    continue;
                }
                if checkpoint.is_done(&id.to_string(), &label, &range) {
                    skipped += 1;
                    continue;
                }
                // Pre-dispatch budget plan (AE3/KTD-3), range mode — the same guard
                // as accumulate: under a measured budget, stop before a triple whose
                // estimated page cost exceeds the remaining budget window (schedule the
                // remainder, no bars fetched), never marking it done so a later cold
                // run resumes it. Inert under the provisional model (`budget_calls:
                // None`), so today's behavior is unchanged.
                if let Some((s, e)) = range_span {
                    let estimated = estimate_pages(kind, &[(s, e)]);
                    let decision = match self.ledger.lock() {
                        Ok(led) => budget::plan_dispatch(
                            &self.budget_model,
                            &led,
                            &self.cred_hash,
                            now_unix(),
                            estimated,
                        ),
                        Err(_) => budget::BudgetDecision::Proceed,
                    };
                    if let budget::BudgetDecision::Defer { estimated, remaining } = decision {
                        tracing::info!(
                            instrument = %id,
                            bar_type = %label,
                            estimated,
                            remaining,
                            "deferring symbol pre-dispatch (range mode): estimated page cost exceeds the remaining budget window; scheduling the remainder (no bars fetched)"
                        );
                        budget_deferrals.push(BudgetDeferral {
                            instrument: id.to_string(),
                            bar_type: label.clone(),
                            estimated_pages: estimated,
                            remaining_budget: remaining,
                        });
                        continue;
                    }
                }
                let bar_type = kind.bar_type(*id)?;
                let outcome = match kind {
                    BarKind::Daily => {
                        collect_daily(&self.fetcher, &shcode, bar_type, &self.config.sdate, &self.config.edate).await?
                    }
                    BarKind::Minute(n) => {
                        collect_minute(
                            &self.fetcher,
                            &shcode,
                            n,
                            bar_type,
                            &self.config.sdate,
                            &self.config.edate,
                        )
                        .await?
                    }
                };
                match outcome {
                    // The collectors only ever return non-empty `Bars` (empty maps
                    // to a gap), but branch on is_empty defensively rather than
                    // relying on that with an unreachable arm.
                    TripleOutcome::Bars(bars) if !bars.is_empty() => {
                        let n = bars.len();
                        match append_bars_checked(&self.config.catalog_path, bar_type, bars).await {
                            Ok(()) => {
                                bars_written += n;
                                ingested += 1;
                                checkpoint.mark_done(&id.to_string(), &label, &range);
                            }
                            // An interval overlap is a fail-closed per-triple refusal
                            // (R5): the triple is NOT marked done (so a re-run re-surfaces
                            // it until the operator compacts/wipes), and the run continues.
                            Err(AdapterError::OverlapRefused { attempted, stored, .. }) => {
                                append_refusals.push(AppendRefusal {
                                    instrument: id.to_string(),
                                    bar_type: label.clone(),
                                    attempted,
                                    stored,
                                });
                            }
                            Err(e) => return Err(e),
                        }
                    }
                    TripleOutcome::Bars(_) => {
                        checkpoint.record_gap(&id.to_string(), &label, &range, GapReason::EmptyHistory);
                        gaps_this_run.push(last_gap(&checkpoint));
                    }
                    TripleOutcome::Gap(reason) => {
                        checkpoint.record_gap(&id.to_string(), &label, &range, reason);
                        gaps_this_run.push(last_gap(&checkpoint));
                    }
                }
                // Persist after every triple so a crash loses at most one triple.
                checkpoint.save(&checkpoint_path)?;
            }
        }

        let budget = BudgetEstimate {
            symbols: universe.len(),
            bar_kinds: self.config.bar_kinds.len(),
            per_sec_cap: self.fetcher.daily_pacer_cap(),
            min_requests: universe.len() * self.config.bar_kinds.len(),
        };
        tracing::info!(
            symbols = budget.symbols,
            bar_kinds = budget.bar_kinds,
            per_sec_cap = budget.per_sec_cap,
            min_requests = budget.min_requests,
            min_seconds = budget.min_seconds(),
            "ingest budget estimate"
        );

        // Persist this run's dispatch spend for the next invocation (KTD-3).
        self.save_ledger();
        Ok(CoverageReport {
            bars_written,
            triples_ingested: ingested,
            triples_skipped: skipped,
            gaps: gaps_this_run,
            heal_refusals: Vec::new(),
            range_refusals,
            append_refusals,
            backward_widen_warnings: Vec::new(),
            backward_widen_uncertainties: Vec::new(),
            budget_deferrals,
            budget,
        })
    }

    /// Accumulate-forward with an injected [`CalendarGate`] (U9, KTD8): grow whole-universe
    /// coverage from each instrument's watermark to `last_closed`, reusing the proven
    /// per-triple fetch loop over a per-instrument range (U5, KTD7). The **watermark map is the
    /// sole skip authority**: a triple already current makes zero bar fetches (R6/AE4). An
    /// instrument with no watermark starts at `lookback_floor` (the initial bounded backfill,
    /// R8; a newly-listed symbol begins here too, R7/AE5). Does not take the lock or
    /// re-snapshot the universe — the caller holds the R15 lock and pre-writes instruments.
    ///
    /// Enforced-only after the ingest Consumer Retirement Gate (#189 U6): the per-triple
    /// next-fetch is decided by the injected calendar for the target session `last_closed`:
    ///
    /// - **proven Trading Session** — fetch as usual (the close buffer is already folded into
    ///   `last_closed`).
    /// - **proven Closed** — skip the gateway call and advance the watermark to `last_closed`
    ///   FROM closure evidence (never on Unknown — the provenance guard, KTD8).
    /// - **Unknown / unavailable** — stop before dispatch; the checkpoint + watermark are
    ///   preserved byte-for-byte and zero gateway requests are issued for that target.
    pub async fn run_accumulate_gated(
        &mut self,
        universe: &[InstrumentId],
        last_closed: NaiveDate,
        lookback_floor: NaiveDate,
        calendar: CalendarGate<'_>,
    ) -> AdapterResult<CoverageReport> {
        std::fs::create_dir_all(&self.config.catalog_path).map_err(|e| {
            AdapterError::Ingest(format!("mkdir catalog {}: {e}", self.config.catalog_path.display()))
        })?;
        let checkpoint_path = self.config.checkpoint_path();
        // U10/KTD8: the legacy `completed`→`watermark` migration runs on load; route it
        // through the same calendar seam so Enforced merges only fully-proven-Closed gaps
        // (Legacy/Shadow stay weekday-authoritative and byte-identical).
        let mut checkpoint = Checkpoint::load_gated(&checkpoint_path, &calendar)?;
        checkpoint.adjusted_prices = self.config.adjusted_prices;
        let mut checkpoint_committed = false;

        let mut bars_written = 0usize;
        let mut ingested = 0usize;
        let mut skipped = 0usize;
        let mut gaps_this_run: Vec<CoverageGap> = Vec::new();
        let mut heal_refusals: Vec<HealRefusal> = Vec::new();
        let mut append_refusals: Vec<AppendRefusal> = Vec::new();
        let mut backward_widen_warnings: Vec<BackwardWidenWarning> = Vec::new();
        let mut backward_widen_uncertainties: Vec<BackwardWidenUncertainty> = Vec::new();
        let mut budget_deferrals: Vec<BudgetDeferral> = Vec::new();

        for id in universe {
            let shcode = id.symbol.as_str().to_string();
            let instrument = id.to_string();
            for &kind in &self.config.bar_kinds {
                let label = kind.label();
                let bar_type = kind.bar_type(*id)?;
                // Decide whether this triple heals or appends. Set only on the
                // normal (append) path.
                let mut pending_plan: Option<CalendarRangePlan> = None;
                let mut heal_plan: Option<CalendarRangePlan> = None;
                // The shifted mark outranks the watermark as authority (KTD-2): a
                // marked symbol heals regardless of watermark state, BEFORE the
                // already-current skip below.
                let heal_now = if matches!(kind, BarKind::Daily) && checkpoint.is_shifted(&instrument, &label) {
                    let plan = calendar.accumulate_plan(lookback_floor, last_closed);
                    let required_through = checkpoint
                        .watermark(&instrument, &label)
                        .into_iter()
                        .chain(
                            checkpoint
                                .shifted_detected(&instrument, &label)
                                .and_then(|date| NaiveDate::parse_from_str(date, "%Y%m%d").ok()),
                        )
                        .max();
                    let heal_through = plan.destructive_request_through();
                    if heal_through.is_none()
                        || required_through.is_some_and(|required| {
                            heal_through.is_some_and(|through| through < required)
                        })
                    {
                        skipped += 1;
                        continue;
                    }
                    heal_plan = Some(plan);
                    true
                } else {
                    let wm = checkpoint.watermark(&instrument, &label);
                    let start = match wm {
                        Some(d) => d.succ_opt().expect("a date always has a successor"),
                        None => lookback_floor,
                    };
                    if start <= last_closed {
                        let plan = calendar.accumulate_plan(start, last_closed);
                        if plan.request_through.is_none() {
                            if let Some(advance) = plan.advance_through {
                                if wm.map_or(true, |watermark| advance > watermark) {
                                    checkpoint.set_watermark(&instrument, &label, advance);
                                    checkpoint.save(&checkpoint_path)?;
                                    checkpoint_committed = true;
                                }
                            }
                            skipped += 1;
                            continue;
                        }
                        pending_plan = Some(plan);
                    }
                    // R4/KTD-6 backward-widen loud no-op: accumulate fetches from
                    // watermark+1, never below it, so a floor earlier than the
                    // earliest stored coverage cannot be reached. Warn and name the
                    // escape hatch (fresh catalog at the wider lookback, or wipe +
                    // full re-pull) instead of silently not fetching. Only meaningful
                    // once a watermark exists — an unseen instrument's floor fetch is
                    // the normal path, not a backward widen.
                    // R4/R6 (#103): warn at most once per triple per floor, and
                    // skip the per-triple `stored_bar_intervals` read once a floor
                    // is established. The read + warning run only when this floor is
                    // NEW information — no marker yet, or a floor deeper than the one
                    // last warned about (a deeper floor is genuinely new, R5). A
                    // repeat run at the same-or-higher floor short-circuits here: no
                    // parquet read, no warning.
                    let needs_check = wm.is_some()
                        && checkpoint
                            .history_floor(&instrument, &label)
                            .map_or(true, |recorded| lookback_floor < recorded);
                    if needs_check {
                        if let Some(earliest_ns) =
                            stored_bar_intervals(&self.config.catalog_path, bar_type)
                                .await?
                                .into_iter()
                                .map(|(s, _)| s)
                                .min()
                        {
                            let earliest_stored = kst_date_of(UnixNanos::from(earliest_ns));
                            if lookback_floor < earliest_stored {
                                // U10/KTD8: gate the widen warning on calendar evidence in the
                                // pre-coverage region [floor, earliest_stored). Legacy/Shadow
                                // keep the unconditional weekday warning (Shadow records the
                                // calendar verdict, non-persisted); Enforced acts on proven
                                // facts — a proven Trading Session still warns + persists, an
                                // all-Closed region emits nothing, and Unknown/unavailable emits
                                // the DISTINCT non-persisted uncertainty (no floor recorded, so a
                                // later run with resolved evidence re-evaluates).
                                match calendar.widen_action(lookback_floor, earliest_stored) {
                                    WidenAction::EmitPersist => {
                                        tracing::warn!(
                                            instrument = %instrument,
                                            floor = %fmt_ymd(lookback_floor),
                                            earliest = %fmt_ymd(earliest_stored),
                                            "backward widen is a no-op: the lookback floor precedes the earliest stored coverage and accumulate never fetches below the watermark; recover the pre-coverage region with a fresh catalog at the wider lookback, or wipe + full re-pull"
                                        );
                                        backward_widen_warnings.push(BackwardWidenWarning {
                                            instrument: instrument.clone(),
                                            bar_type: label.clone(),
                                            floor: fmt_ymd(lookback_floor),
                                            earliest_stored: fmt_ymd(earliest_stored),
                                        });
                                        // Record the warned floor and persist it now: an
                                        // already-current late-listed triple is skipped
                                        // below (a bare `continue`, no save), so relying on
                                        // the per-triple save would lose the marker and the
                                        // symbol would re-warn every run — the exact noise
                                        // this closes.
                                        checkpoint.set_history_floor(&instrument, &label, lookback_floor);
                                        checkpoint.save(&checkpoint_path)?;
                                        checkpoint_committed = true;
                                    }
                                    WidenAction::EmitUncertain => {
                                        // Enforced + Unknown/unavailable evidence: the region MAY
                                        // hold un-fetched trading history, but no proof yet.
                                        // Surface it distinctly and DO NOT persist a floor, so a
                                        // later run with newly-resolved evidence re-evaluates.
                                        tracing::warn!(
                                            instrument = %instrument,
                                            floor = %fmt_ymd(lookback_floor),
                                            earliest = %fmt_ymd(earliest_stored),
                                            "backward widen is INDETERMINATE: the pre-coverage region has Unknown/unavailable calendar evidence and no proven Trading Session; whether it holds un-fetched history is undetermined — not persisted, re-evaluated when the calendar resolves"
                                        );
                                        backward_widen_uncertainties.push(BackwardWidenUncertainty {
                                            instrument: instrument.clone(),
                                            bar_type: label.clone(),
                                            floor: fmt_ymd(lookback_floor),
                                            earliest_stored: fmt_ymd(earliest_stored),
                                            calendar_stale: calendar.has_stale_evidence(),
                                        });
                                    }
                                    WidenAction::Suppress => {
                                        // Enforced + an all-proven-Closed pre-coverage region: no
                                        // trading history was missed, so there is nothing to
                                        // widen. Emit nothing and record no floor.
                                    }
                                }
                            }
                        }
                    }
                    if pending_plan.is_none() {
                        // Already current. Backward-widen evidence above remains observable,
                        // but there is no pending forward span to plan or fetch.
                        skipped += 1;
                        continue;
                    }
                    // Basis-shift detection (KTD-3): before appending new daily bars,
                    // re-fetch the overlap window ending at the watermark and compare
                    // against stored bars. No watermark (first-ever accumulate) or an
                    // insufficient overlap skips detection entirely.
                    let mut detected = false;
                    if matches!(kind, BarKind::Daily) {
                        if let Some(wm) = checkpoint.watermark(&instrument, &label) {
                            let candidate = calendar.accumulate_plan(lookback_floor, last_closed);
                            let detection_authorized = candidate
                                .destructive_request_through()
                                .is_some_and(|heal_through| heal_through >= wm)
                                && matches!(calendar.calendar_decision(wm), CalendarDecision::Fetch);
                            if detection_authorized && self.detect_shift(&shcode, bar_type, wm).await? {
                                let heal_through = candidate
                                    .destructive_request_through()
                                    .expect("an authorized detection has a complete heal endpoint");
                                // Save the mark atomically BEFORE any delete (KTD-2:
                                // mark-before-wipe is load-bearing — the reverse order
                                // plus a crash would leave a high watermark over an
                                // empty store and silently truncate history forever).
                                checkpoint.mark_shifted(&instrument, &label, heal_through, RebaseOrigin::Heal);
                                checkpoint.save(&checkpoint_path)?;
                                checkpoint_committed = true;
                                heal_plan = Some(candidate);
                                tracing::warn!(instrument = %instrument, "adjustment-basis shift detected; healing");
                                detected = true;
                            }
                        }
                    }
                    detected
                };
                if heal_now {
                    let plan = heal_plan.expect("every admitted heal has a complete calendar plan");
                    let heal_through = plan
                        .destructive_request_through()
                        .expect("every admitted heal has an established request endpoint");
                    match self
                        .heal_daily(&mut checkpoint, &checkpoint_path, &shcode, &instrument, &label, bar_type, heal_through, lookback_floor)
                        .await?
                    {
                        HealOutcome::Healed(n) => {
                            bars_written += n;
                            ingested += 1;
                            if let Some(advance) = plan.advance_through {
                                if advance > heal_through {
                                    checkpoint.set_watermark(&instrument, &label, advance);
                                    checkpoint.save(&checkpoint_path)?;
                                }
                            }
                            checkpoint_committed = true;
                        }
                        HealOutcome::Refused(r) => heal_refusals.push(r),
                        // #104/R7: a heal re-pull append that hit an overlap
                        // refusal is per-triple, not run-fatal — record it and
                        // move on to the remaining triples (the mark stays).
                        HealOutcome::AppendRefused(r) => {
                            append_refusals.push(r);
                            checkpoint_committed = true;
                        }
                        HealOutcome::Incomplete => {
                            gaps_this_run.push(CoverageGap {
                                instrument: instrument.clone(),
                                bar_type: label.clone(),
                                range: format!("{}..{}", fmt_ymd(lookback_floor), fmt_ymd(last_closed)),
                                reason: GapReason::PaperThin,
                            });
                            checkpoint_committed = true;
                        }
                    }
                    continue;
                }
                let plan = pending_plan.expect("the append path always computed a calendar plan");
                let start = match checkpoint.watermark(&instrument, &label) {
                    Some(d) => d.succ_opt().expect("a date always has a successor"),
                    None => lookback_floor,
                };
                let request_through = plan
                    .request_through
                    .expect("the append path always has a request endpoint");
                let advance_through = plan.advance_through.unwrap_or(request_through);
                // #102/KTD-1: trim the fetch window [start, last_closed] against the
                // coverage the checkpoint already records above the watermark, using
                // the in-memory checkpoint — never parquet, never a calendar (R3). In
                // steady state (no far coverage) this yields the single segment
                // [start, last_closed], identical to before. For a legacy multi-range
                // checkpoint whose far ranges survive above a prefix watermark, it
                // yields only the un-covered gaps: we fetch/write those disjointly
                // (R2 — a genuine trading day in the gap is still fetched) and never
                // re-overlap or re-fetch a recorded range, so the stall never forms
                // (R1). `wm` is recomputed here — the append path never mutates the
                // checkpoint, so it equals the value that derived `start`.
                let wm = checkpoint.watermark(&instrument, &label);
                let covered = match wm {
                    Some(w) => checkpoint.completed_intervals_above(&instrument, &label, w),
                    None => Vec::new(),
                };
                // The advance target when no sub-range truncates: the coverage is now
                // contiguous through the highest recorded far edate within reach — so
                // the next run does not re-derive (and re-overlap) a fully-covered far
                // range even when `last_closed` sits at or below its edate.
                let highest_covered = covered
                    .iter()
                    .filter(|(cs, _)| *cs <= request_through)
                    .map(|(_, e)| {
                        if calendar.adoption() == CalendarAdoption::Enforced {
                            (*e).min(advance_through)
                        } else {
                            *e
                        }
                    })
                    .max();
                let sub_ranges = subtract_covered(start, request_through, &covered);

                // Pre-dispatch budget plan (AE3/KTD-3): under a measured budget, stop
                // before a triple whose estimated page cost exceeds the remaining
                // budget window — the checkpoint is unchanged (nothing lost), the
                // remainder is scheduled, and no IGW00201 is provoked. A no-op under
                // the provisional model (`budget_calls: None`), so today's behavior is
                // unchanged until U6 promotes measured numbers. The gateway stays
                // ground truth — this only ever refuses to dispatch, never fabricates
                // coverage.
                if !sub_ranges.is_empty() {
                    let estimated = estimate_pages(kind, &sub_ranges);
                    // Advisory: a poisoned ledger lock never blocks ingest (the
                    // gateway stays ground truth) — fall through to Proceed.
                    let decision = match self.ledger.lock() {
                        Ok(led) => budget::plan_dispatch(
                            &self.budget_model,
                            &led,
                            &self.cred_hash,
                            now_unix(),
                            estimated,
                        ),
                        Err(_) => budget::BudgetDecision::Proceed,
                    };
                    if let budget::BudgetDecision::Defer { estimated, remaining } = decision {
                        tracing::info!(
                            instrument = %instrument,
                            bar_type = %label,
                            estimated,
                            remaining,
                            "deferring symbol pre-dispatch: estimated page cost exceeds the remaining budget window; scheduling the remainder (no bars fetched)"
                        );
                        budget_deferrals.push(BudgetDeferral {
                            instrument: instrument.clone(),
                            bar_type: label.clone(),
                            estimated_pages: estimated,
                            remaining_budget: remaining,
                        });
                        continue;
                    }
                }

                let mut wrote_any = false;
                // The start of the first sub-range that halts the loop — a PaperThin
                // truncation (un-fetched older history) or an unforeseen overlap. The
                // watermark pins just BEFORE it: everything lower is contiguous and
                // attested, and no higher (disjoint) sub-range is fetched or written,
                // so no bars are orphaned above a low-pinned watermark (KTD-1).
                let mut halt_before: Option<NaiveDate> = None;
                for (s, e) in &sub_ranges {
                    let sdate = s.format("%Y%m%d").to_string();
                    let edate = e.format("%Y%m%d").to_string();
                    let range = format!("{sdate}..{edate}");
                    let outcome = match kind {
                        BarKind::Daily => {
                            collect_daily(&self.fetcher, &shcode, bar_type, &sdate, &edate).await?
                        }
                        BarKind::Minute(n) => {
                            collect_minute(&self.fetcher, &shcode, n, bar_type, &sdate, &edate).await?
                        }
                    };
                    match outcome {
                        TripleOutcome::Bars(bars) if !bars.is_empty() => {
                            let n = bars.len();
                            match append_bars_checked(&self.config.catalog_path, bar_type, bars).await {
                                Ok(()) => {
                                    bars_written += n;
                                    wrote_any = true;
                                }
                                // Fail-closed net (R5) for an overlap the trim did not
                                // anticipate: record it, halt before this sub-range
                                // (do not advance past it, do not fetch a higher one),
                                // and let the run continue — the next run re-surfaces
                                // it until the operator compacts/wipes.
                                Err(AdapterError::OverlapRefused { attempted, stored, .. }) => {
                                    append_refusals.push(AppendRefusal {
                                        instrument: instrument.clone(),
                                        bar_type: label.clone(),
                                        attempted,
                                        stored,
                                    });
                                    halt_before = Some(*s);
                                    break;
                                }
                                Err(e) => return Err(e),
                            }
                        }
                        TripleOutcome::Bars(_) => {
                            gaps_this_run.push(CoverageGap {
                                instrument: instrument.clone(),
                                bar_type: label.clone(),
                                range,
                                reason: GapReason::EmptyHistory,
                            });
                            if calendar.adoption() == CalendarAdoption::Enforced {
                                halt_before = Some(*s);
                                break;
                            }
                        }
                        TripleOutcome::Gap(reason) => {
                            let paper_thin = reason == GapReason::PaperThin;
                            gaps_this_run.push(CoverageGap {
                                instrument: instrument.clone(),
                                bar_type: label.clone(),
                                range,
                                reason,
                            });
                            // A truncated fetch means the sub-range is only partially
                            // retrieved: pin before it and stop, or the un-fetched
                            // older history is skipped forever (R2/R10).
                            if paper_thin || calendar.adoption() == CalendarAdoption::Enforced {
                                halt_before = Some(*s);
                                break;
                            }
                        }
                    }
                }
                if wrote_any {
                    ingested += 1;
                }
                match halt_before {
                    // Pin the watermark just before the halting sub-range — only when
                    // that is genuine forward progress over the existing watermark
                    // (earlier sub-ranges + recorded coverage were written/attested).
                    // A first-sub-range halt, or no prior watermark, advances nothing,
                    // so the triple re-surfaces next run (matching the pre-trim
                    // fail-closed refusal semantics).
                    Some(s) => {
                        if let (Some(w), Some(pin)) = (wm, s.pred_opt()) {
                            if pin > w {
                                checkpoint.set_watermark(&instrument, &label, pin);
                            }
                        }
                    }
                    // No truncation: coverage is contiguous through
                    // max(last_closed, highest recorded far edate). Advance there so
                    // the next run is a steady-state single-segment fetch (R1).
                    None => {
                        let target = highest_covered.map_or(advance_through, |hc| advance_through.max(hc));
                        checkpoint.set_watermark(&instrument, &label, target);
                    }
                }
                // Persist after each authorized triple for crash safety. Enforced stop/
                // incomplete paths with no progress leave legacy input bytes untouched.
                let changed = wrote_any || checkpoint.watermark(&instrument, &label) != wm;
                if calendar.adoption() != CalendarAdoption::Enforced || changed {
                    checkpoint.save(&checkpoint_path)?;
                    checkpoint_committed = true;
                }
            }
        }

        // Prune legacy completed/gap rows below the watermarks so daily runs stay
        // bounded (KTD7); the run's own gaps report comes from memory.
        if calendar.adoption() != CalendarAdoption::Enforced || checkpoint_committed {
            checkpoint.prune_below_watermarks();
            checkpoint.save(&checkpoint_path)?;
        }

        // Each pending daily triple now costs a detection overlap fetch ON TOP
        // of the append fetch (KTD-3/KTD-4) — the printed lower bound must say
        // so, or the operator sizes the no-live window at half the real cost.
        let daily_kinds = self
            .config
            .bar_kinds
            .iter()
            .filter(|k| matches!(k, BarKind::Daily))
            .count();
        let budget = BudgetEstimate {
            symbols: universe.len(),
            bar_kinds: self.config.bar_kinds.len(),
            per_sec_cap: self.fetcher.daily_pacer_cap(),
            min_requests: universe.len() * (self.config.bar_kinds.len() + daily_kinds),
        };
        tracing::info!(
            symbols = budget.symbols,
            ingested,
            skipped,
            gaps = gaps_this_run.len(),
            "accumulate-forward run complete"
        );
        // Persist this run's dispatch spend for the next invocation (KTD-3, R11):
        // the per-symbol drip re-invokes the binary, so this is the seam that lets a
        // budget-interrupted run resume across sessions.
        self.save_ledger();
        Ok(CoverageReport {
            bars_written,
            triples_ingested: ingested,
            triples_skipped: skipped,
            gaps: gaps_this_run,
            heal_refusals,
            // Accumulate mode heals marked series in place; range-mode refusal is a
            // range-only concept (KTD8).
            range_refusals: Vec::new(),
            append_refusals,
            backward_widen_warnings,
            backward_widen_uncertainties,
            budget_deferrals,
            budget,
        })
    }

    /// Calendar-aware epoch re-base (KTD-4, R6): mark every daily triple in `universe` shifted
    /// in ONE atomic checkpoint save, then run the accumulate/heal path. The per-symbol marks
    /// are the completion state, so the epoch is crash-resumable by construction: a resumed run
    /// (accumulate mode) heals only what remains. Forward-only detection cannot see splices
    /// already baked into the catalog; this is the one-time rollout that puts the whole catalog
    /// on a single basis. Minute triples are never marked (KTD-8). Enforced-only after the
    /// ingest Consumer Retirement Gate (#189 U6): admission and prefix selection happen before
    /// the durable mark-all boundary, so an unusable or zero-length proven prefix cannot mutate
    /// markers, wipe data, or dispatch to LS.
    pub async fn run_rebase_gated(
        &mut self,
        universe: &[InstrumentId],
        last_closed: NaiveDate,
        lookback_floor: NaiveDate,
        calendar: CalendarGate<'_>,
    ) -> AdapterResult<CoverageReport> {
        if !self.config.bar_kinds.iter().any(|k| matches!(k, BarKind::Daily)) {
            return Err(AdapterError::Ingest(
                "epoch re-base requires the daily bar kind (a mark with no daily lane would never heal)".to_string(),
            ));
        }
        let plan = calendar.accumulate_plan(lookback_floor, last_closed);
        let Some(rebase_end) = plan.request_through else {
            return Ok(CoverageReport {
                bars_written: 0,
                triples_ingested: 0,
                triples_skipped: universe.len() * self.config.bar_kinds.len(),
                gaps: Vec::new(),
                heal_refusals: Vec::new(),
                range_refusals: Vec::new(),
                append_refusals: Vec::new(),
                backward_widen_warnings: Vec::new(),
                backward_widen_uncertainties: Vec::new(),
                budget_deferrals: Vec::new(),
                budget: BudgetEstimate {
                    symbols: universe.len(),
                    bar_kinds: self.config.bar_kinds.len(),
                    per_sec_cap: self.fetcher.daily_pacer_cap(),
                    min_requests: 0,
                },
            });
        };
        std::fs::create_dir_all(&self.config.catalog_path).map_err(|e| {
            AdapterError::Ingest(format!("mkdir catalog {}: {e}", self.config.catalog_path.display()))
        })?;
        let checkpoint_path = self.config.checkpoint_path();
        let mut checkpoint = Checkpoint::load_gated(&checkpoint_path, &calendar)?;
        let daily_label = BarKind::Daily.label();
        for id in universe {
            // Epoch origin — the one-time rollout, kept out of the organic audit
            // metric (KTD5/R8). A series already heal-marked keeps its heal origin
            // (keep-original-on-re-mark).
            checkpoint.mark_shifted(&id.to_string(), &daily_label, rebase_end, RebaseOrigin::Epoch);
        }
        checkpoint.save(&checkpoint_path)?;
        tracing::info!(symbols = universe.len(), "epoch re-base: all daily triples marked; healing");
        self.run_accumulate_gated(universe, rebase_end, lookback_floor, calendar).await
    }

    /// Detect a basis shift for one daily triple (KTD-3): fetch the overlap
    /// window ending at the watermark, read the stored side through the scoped
    /// read (never `read_all_bars`), exact-compare mutually-present dates.
    async fn detect_shift(
        &self,
        shcode: &str,
        bar_type: BarType,
        watermark: NaiveDate,
    ) -> AdapterResult<bool> {
        let wstart = overlap_window_start(watermark, self.config.overlap_days);
        // Stored side: the last `overlap_days` stored trading days in the window.
        let ws_ns = kst_to_unix_nanos(wstart, NaiveTime::MIN)?;
        let we_ns = kst_to_unix_nanos(watermark, KRX_REGULAR_CLOSE)?;
        let mut stored = overlap_tail(
            read_bars_scoped(&self.config.catalog_path, bar_type, Some(ws_ns), Some(we_ns)).await?,
            self.config.overlap_days,
        );
        // Distinct-session gate (KTD-4): duplicates can neither fake sufficiency
        // nor dilute the count, and a value-divergent same-session pair forces the
        // shift verdict directly.
        match stored_overlap_gate(&stored) {
            TailGate::ForceShift => return Ok(true),
            TailGate::Insufficient => return Ok(false),
            TailGate::Compare => {}
        }
        // Feed a byte-identical-deduped tail to the compare so a redundant re-pull
        // row can't perturb the per-timestamp map.
        dedup_bars(&mut stored);
        let fetched = match fetch_overlap(&self.fetcher, shcode, bar_type, wstart, watermark).await? {
            Some(bars) => bars,
            None => return Ok(false),
        };
        Ok(compare_overlap(&stored, &fetched) == OverlapVerdict::Shifted)
    }

    /// Heal one marked daily triple (KTD-2): one idempotent re-entrant sequence
    /// that always restarts at the wipe. The caller has already durably saved the
    /// shifted mark. Wipe precondition first: refuse (and stay marked) if the
    /// run's floor is later than the earliest stored bar — a heal must never
    /// silently shrink stored history.
    #[allow(clippy::too_many_arguments)]
    async fn heal_daily(
        &self,
        checkpoint: &mut Checkpoint,
        checkpoint_path: &Path,
        shcode: &str,
        instrument: &str,
        label: &str,
        bar_type: BarType,
        last_closed: NaiveDate,
        floor: NaiveDate,
    ) -> AdapterResult<HealOutcome> {
        // Wipe precondition (KTD-2). An already-wiped re-entry has no stored bars
        // and passes trivially (there is no history left to truncate).
        let stored = read_bars_scoped(&self.config.catalog_path, bar_type, None, None).await?;
        if let Some(earliest) = stored.first() {
            let earliest_date = kst_date_of(earliest.ts_event);
            if floor > earliest_date {
                tracing::warn!(
                    instrument,
                    floor = %fmt_ymd(floor),
                    earliest = %fmt_ymd(earliest_date),
                    "heal refused: run floor is later than the earliest stored bar; symbol stays marked"
                );
                return Ok(HealOutcome::Refused(HealRefusal {
                    instrument: instrument.to_string(),
                    bar_type: label.to_string(),
                    floor: fmt_ymd(floor),
                    earliest_stored: fmt_ymd(earliest_date),
                }));
            }
        }

        // Wipe: true-delete the series, then drop the watermark so the accumulate
        // arithmetic re-pulls from the floor. Persist the wiped state — the mark
        // (already saved) makes any crash from here converge back to this wipe.
        delete_bar_series(&self.config.catalog_path, bar_type).await?;
        checkpoint.clear_watermark(instrument, label);
        checkpoint.save(checkpoint_path)?;

        // Re-pull the full history from the floor. Completion keys on the fetch
        // cursor completing, never on bar count (KTD-3) — a shallow-history
        // symbol (listed after the floor) still clears its mark. A truncated
        // fetch (page cap) is NOT complete: keep the mark for the next run.
        let sdate = fmt_ymd(floor);
        let edate = fmt_ymd(last_closed);
        let pulled = match collect_daily(&self.fetcher, shcode, bar_type, &sdate, &edate).await? {
            TripleOutcome::Bars(bars) => bars,
            TripleOutcome::Gap(GapReason::PaperThin) => {
                tracing::warn!(instrument, "heal re-pull truncated; symbol stays marked");
                return Ok(HealOutcome::Incomplete);
            }
            // Cursor completed with zero bars. Trust it as completion ONLY for a
            // series that was already empty before the wipe — for a series that
            // HAD stored bars, an empty page is far more likely a transient
            // gateway hiccup than a genuine delisting, and completing here would
            // pin the watermark over a wiped store: silent, permanent history
            // loss with no retry path. Keep the mark instead; the next run
            // re-enters at the (now no-op) wipe and re-pulls.
            TripleOutcome::Gap(_) => {
                if !stored.is_empty() {
                    tracing::warn!(
                        instrument,
                        "heal re-pull returned no bars for a previously non-empty series; symbol stays marked"
                    );
                    return Ok(HealOutcome::Incomplete);
                }
                Vec::new()
            }
        };
        // Capture what the re-verify needs BEFORE moving `pulled` into the write
        // (the tail is a handful of bars; cloning the whole multi-year series
        // per healed symbol would be pure ownership plumbing).
        let pulled_len = pulled.len();
        let wstart = overlap_window_start(last_closed, self.config.overlap_days);
        let ws_ns = kst_to_unix_nanos(wstart, NaiveTime::MIN)?.as_u64();
        let tail = overlap_tail(
            pulled.iter().filter(|b| b.ts_event.as_u64() >= ws_ns).cloned().collect(),
            self.config.overlap_days,
        );
        if !pulled.is_empty() {
            // The wipe (delete_bar_series above) left the series empty, so this
            // re-pull is disjoint by construction and passes the checked guard
            // (R6) — routing it through the same wrapper keeps raw `write_bars`
            // out of every production path (KTD-2). #104/R7: if an overlap ever
            // DOES survive the wipe (a delete that failed to clear, residual
            // pollution), catch it per-triple and route it to the append-refusal
            // vec — the mark stays and the run continues, instead of a propagated
            // fatal error aborting every remaining triple. Any other append error
            // still propagates (the arm is scoped to `OverlapRefused` only).
            match append_bars_checked(&self.config.catalog_path, bar_type, pulled).await {
                Ok(()) => {}
                Err(AdapterError::OverlapRefused { attempted, stored, .. }) => {
                    tracing::warn!(
                        instrument,
                        %attempted,
                        %stored,
                        "heal re-pull append refused (overlap survived the wipe); symbol stays marked, run continues"
                    );
                    return Ok(HealOutcome::AppendRefused(AppendRefusal {
                        instrument: instrument.to_string(),
                        bar_type: label.to_string(),
                        attempted,
                        stored,
                    }));
                }
                Err(e) => return Err(e),
            }
        }

        // Re-verify (the gateway may rewrite the series again while the heal is
        // in flight): one more overlap fetch against the just-pulled tail. Only a
        // positive mismatch keeps the mark — an insufficient overlap (shallow
        // history) must not pin a symbol shifted forever.
        if !tail.is_empty() {
            if let Some(fetched) =
                fetch_overlap(&self.fetcher, shcode, bar_type, wstart, last_closed).await?
            {
                if compare_overlap(&tail, &fetched) == OverlapVerdict::Shifted {
                    tracing::warn!(instrument, "heal re-verify mismatched; symbol stays marked");
                    return Ok(HealOutcome::Incomplete);
                }
            }
        }

        // Completion: clear the mark, record the re-base event, and set the
        // watermark in one save (KTD-2).
        let detected = checkpoint
            .shifted_detected(instrument, label)
            .unwrap_or(&fmt_ymd(last_closed))
            .to_string();
        // Read the origin stamped at mark time (survives crash-resume under a
        // different running mode, KTD5) BEFORE clearing the mark.
        let origin = checkpoint.shifted_origin(instrument, label);
        checkpoint.clear_shifted(instrument, label);
        checkpoint.record_rebase_event(RebaseEvent {
            instrument: instrument.to_string(),
            bar_type: label.to_string(),
            detected,
            healed: fmt_ymd(last_closed),
            origin,
        });
        checkpoint.set_watermark(instrument, label, last_closed);
        checkpoint.save(checkpoint_path)?;
        tracing::info!(instrument, bars = pulled_len, "basis-shift heal complete");
        Ok(HealOutcome::Healed(pulled_len))
    }
}

/// The KST calendar date of a bar timestamp — the inverse of
/// [`kst_to_unix_nanos`]'s date component. Public so downstream (the lab's
/// session-slicing) shares ONE KST-date conversion instead of re-deriving the
/// +9h offset.
pub fn kst_date_of(ts: UnixNanos) -> NaiveDate {
    let kst = FixedOffset::east_opt(KST_UTC_OFFSET_HOURS * 3600).expect("KST offset is valid");
    chrono::DateTime::<chrono::Utc>::from_timestamp_nanos(ts.as_u64() as i64)
        .with_timezone(&kst)
        .date_naive()
}

impl SdkFetcher {
    fn daily_pacer_cap(&self) -> u32 {
        // 1s / interval, rounded.
        let secs = self.daily_pacer.min_interval().as_secs_f64();
        if secs <= 0.0 {
            0
        } else {
            (1.0 / secs).round() as u32
        }
    }
}

fn last_gap(cp: &Checkpoint) -> checkpoint::CoverageGap {
    cp.gaps().last().cloned().expect("a gap was just recorded")
}

/// Write bars to the catalog on a blocking thread — a **fixture-only primitive**
/// (KTD-2). No production caller may use it directly: it skips the interval-overlap
/// guard, so a re-ingest that overlaps stored coverage writes a second overlapping
/// parquet file (the duplicate-pollution root cause). Every production writer goes
/// through [`append_bars_checked`], which refuses an overlap fail-closed.
///
/// `ParquetDataCatalog` drives an internal runtime via `block_on`, which panics if
/// called on a thread already running a tokio reactor — so every catalog
/// interaction is moved to the blocking pool (`spawn_blocking`). The catalog is
/// constructed, used, and dropped entirely inside the closure. Ascending `ts_init`
/// is guaranteed by the callers; the parquet disjoint check is skipped (via the
/// `Some(true)` argument) so tests can deliberately stage overlaps.
///
/// Public so the lab (and tooling) can stage a fixture catalog symmetrically with
/// [`read_all_bars`] / [`write_instruments`]. **This bypasses the ingest checkpoint**
/// — it advances no watermark and records no coverage, so production coverage growth
/// must go through [`Ingestor`] (which owns the checkpoint), never this. Reserve
/// direct use for test fixtures / one-off staging.
pub async fn write_bars(catalog_path: &Path, bars: Vec<Bar>) -> AdapterResult<()> {
    let path = catalog_path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&path)
            .map_err(|e| AdapterError::Ingest(format!("mkdir catalog {}: {e}", path.display())))?;
        let catalog = ParquetDataCatalog::new(&path, None, None, None, None);
        catalog
            .write_to_parquet(&bars, None, None, Some(true))
            .map(|_| ())
            .map_err(|e| AdapterError::Ingest(format!("catalog write: {e}")))
    })
    .await
    .map_err(|e| AdapterError::Ingest(format!("catalog write task panicked: {e}")))?
}

/// The stored coverage intervals `[start_ns, end_ns]` for one bar-type series,
/// read from the catalog's parquet filenames (no row reads), on the blocking pool
/// (KTD-1). An unwritten series returns an empty vec — the `create_dir_all`
/// envelope keeps a never-written catalog path from erroring (the
/// block-on-from-async construction gotcha).
pub async fn stored_bar_intervals(
    catalog_path: &Path,
    bar_type: BarType,
) -> AdapterResult<Vec<(u64, u64)>> {
    let path = catalog_path.to_path_buf();
    let identifier = bar_type.to_string();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&path)
            .map_err(|e| AdapterError::Ingest(format!("mkdir catalog {}: {e}", path.display())))?;
        let catalog = ParquetDataCatalog::new(&path, None, None, None, None);
        catalog
            .get_intervals("bars", Some(&identifier))
            .map_err(|e| AdapterError::Ingest(format!("catalog intervals {identifier}: {e}")))
    })
    .await
    .map_err(|e| AdapterError::Ingest(format!("catalog intervals task panicked: {e}")))?
}

/// Subtract the `covered` coverage spans (sorted, merged; each with `edate` above
/// the window's watermark) from the fetch window `[start, last_closed]`, yielding
/// the un-covered sub-ranges in date order (U4/KTD-1). A span reaching at/above
/// `last_closed` truncates the tail; a fully-covered window yields no sub-ranges.
/// Each returned sub-range is disjoint from every covered span, so appending it
/// passes the checked-write guard without refusing.
fn subtract_covered(
    start: NaiveDate,
    last_closed: NaiveDate,
    covered: &[(NaiveDate, NaiveDate)],
) -> Vec<(NaiveDate, NaiveDate)> {
    let mut out = Vec::new();
    let mut cursor = start;
    for (cs, ce) in covered {
        if *cs > last_closed {
            // The span starts beyond the window (future coverage) — leave the
            // remaining window as one trailing sub-range below it.
            break;
        }
        if *cs > cursor {
            // A gap before this span: [cursor, cs-1], clamped to the window.
            let end = cs.pred_opt().unwrap_or(*cs).min(last_closed);
            if cursor <= end {
                out.push((cursor, end));
            }
        }
        // Jump the cursor past the covered span.
        if let Some(next) = ce.succ_opt() {
            if next > cursor {
                cursor = next;
            }
        }
        if cursor > last_closed {
            break;
        }
    }
    if cursor <= last_closed {
        out.push((cursor, last_closed));
    }
    out
}

/// Format a `[min_ts, max_ts]` nanosecond interval as a KST `sdate..edate` string.
fn fmt_ts_range(min_ts: u64, max_ts: u64) -> String {
    format!(
        "{}..{}",
        fmt_ymd(kst_date_of(UnixNanos::from(min_ts))),
        fmt_ymd(kst_date_of(UnixNanos::from(max_ts)))
    )
}

/// Append bars to the catalog **only if** their `[min_ts, max_ts]` range is
/// disjoint from every stored interval for the same series (R5/R6/KTD-1). The
/// fail-closed write guard: a re-fetch-from-floor (legacy widen) or any unforeseen
/// overlap source is refused with a typed [`AdapterError::OverlapRefused`] that
/// names both remediations, instead of silently writing a second overlapping
/// parquet file. Disjoint writes on either side of existing coverage stay legal
/// (a backward-widen escape hatch), and the heal path's wipe-then-re-pull is
/// overlap-free by construction, so it passes the guard. Bounds are inclusive: a
/// write sharing a single boundary timestamp with stored coverage is refused. An
/// empty write is a no-op `Ok`. All production writers route through this;
/// [`write_bars`] stays a fixture-only primitive (KTD-2).
pub async fn append_bars_checked(
    catalog_path: &Path,
    bar_type: BarType,
    bars: Vec<Bar>,
) -> AdapterResult<()> {
    if bars.is_empty() {
        return Ok(());
    }
    let min_ts = bars.iter().map(|b| b.ts_init.as_u64()).min().expect("non-empty");
    let max_ts = bars.iter().map(|b| b.ts_init.as_u64()).max().expect("non-empty");
    let intervals = stored_bar_intervals(catalog_path, bar_type).await?;
    // Inclusive-bounds intersection: [min,max] overlaps [s,e] iff min<=e && s<=max.
    let overlapping: Vec<(u64, u64)> = intervals
        .into_iter()
        .filter(|(s, e)| min_ts <= *e && *s <= max_ts)
        .collect();
    if !overlapping.is_empty() {
        return Err(AdapterError::OverlapRefused {
            bar_type: bar_type.to_string(),
            attempted: fmt_ts_range(min_ts, max_ts),
            stored: overlapping
                .iter()
                .map(|(s, e)| fmt_ts_range(*s, *e))
                .collect::<Vec<_>>()
                .join(", "),
        });
    }
    write_bars(catalog_path, bars).await
}

/// Write instrument definitions to the catalog (so a backtest can load them). Runs
/// on the blocking pool for the same reason as [`write_bars`].
pub async fn write_instruments(
    catalog_path: &Path,
    instruments: Vec<nautilus_model::instruments::InstrumentAny>,
) -> AdapterResult<()> {
    let path = catalog_path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&path)
            .map_err(|e| AdapterError::Ingest(format!("mkdir catalog {}: {e}", path.display())))?;
        let catalog = ParquetDataCatalog::new(&path, None, None, None, None);
        catalog
            .write_instruments(instruments)
            .map(|_| ())
            .map_err(|e| AdapterError::Ingest(format!("catalog write_instruments: {e}")))
    })
    .await
    .map_err(|e| AdapterError::Ingest(format!("catalog write_instruments task panicked: {e}")))?
}

/// Read all bars back from the catalog on a blocking thread (round-trip helper for
/// tests + the backtest loader). Duplicate bars are deduplicated (see
/// [`dedup_bars`]): re-ingesting a range that overlaps already-stored bars writes a
/// second parquet file for the overlapping window, and the aggregate read would
/// otherwise double-count it.
pub async fn read_all_bars(catalog_path: &Path) -> AdapterResult<Vec<Bar>> {
    let mut bars = read_all_bars_raw(catalog_path).await?;
    dedup_bars(&mut bars);
    Ok(bars)
}

/// Read every bar back from the catalog WITHOUT deduplication, on a blocking
/// thread. The raw aggregate read surfaces overlap-duplicate rows (the pollution
/// [`read_all_bars`] masks) — compaction needs those raw rows to count the true
/// stored total and to detect value-divergent same-timestamp rows.
async fn read_all_bars_raw(catalog_path: &Path) -> AdapterResult<Vec<Bar>> {
    let path = catalog_path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut catalog = ParquetDataCatalog::new(&path, None, None, None, None);
        catalog
            .bars(None, None, None)
            .map_err(|e| AdapterError::Ingest(format!("catalog read: {e}")))
    })
    .await
    .map_err(|e| AdapterError::Ingest(format!("catalog read task panicked: {e}")))?
}

/// Remove bars that are *byte-identical* to one already seen, keeping the first
/// occurrence. The aggregate catalog read can surface the same bar twice when a
/// re-ingest wrote an overlapping-range parquet file: `write_to_parquet` skips the
/// disjoint check (see [`write_bars`]) and the normal accumulate *append* path
/// assumes each write is a disjoint forward range (`watermark+1 ..`), so it never
/// wipes — an overlap (e.g. a re-fetch from the floor when a watermark is absent)
/// leaves both files readable and the overlap double-counted. That corrupts the
/// backtest's universe scan, which reads the last two in-range daily bars as
/// prior→today and would otherwise pick two copies of the same session (a
/// nonsensical intraday self-gap). Bars are built with deterministic timestamps
/// (`ts_event == ts_init`, derived from the candle's own KST date/time, never a
/// wall clock), so a redundant re-pull is exactly equal and collapses cleanly.
/// Dedup is on the WHOLE bar, not a `(series, ts)` key: a same-timestamp bar whose
/// OHLCV differs is a genuine conflict — an adjustment-basis shift (the heal path's
/// concern, [`delete_bar_series`]) or a mid-run mutation the finalize fingerprint
/// re-check catches — and must NOT be silently dropped. The finalize re-check only
/// compares a run's start vs end, so it does not catch a *pre-existing* divergent
/// same-session overlap; the universe scan defends against that independently by
/// selecting prior/today on distinct sessions (`runner::backtest::build_candidates`).
fn dedup_bars(bars: &mut Vec<Bar>) {
    let mut seen = std::collections::HashSet::new();
    bars.retain(|b| seen.insert(*b));
}

/// True-delete one bar-type series (e.g. one symbol's daily bars) from the
/// catalog, on a blocking thread (same `spawn_blocking` rationale as
/// [`write_bars`]). The heal's wipe step (KTD-2): overwrite-and-tolerate is not
/// enough — `write_to_parquet` with the disjoint check skipped leaves stale
/// old-basis files readable wherever date ranges don't exactly coincide, so the
/// wipe must remove the files. Deleting a series with no stored bars is a no-op
/// `Ok`. Scoped to ONE bar type: a daily wipe never touches minute bars (KTD-8).
pub async fn delete_bar_series(catalog_path: &Path, bar_type: BarType) -> AdapterResult<()> {
    let path = catalog_path.to_path_buf();
    let identifier = bar_type.to_string();
    tokio::task::spawn_blocking(move || {
        // Mutating catalog entry points create the dir themselves (the
        // block-on-from-async solution doc: `new` canonicalizes and fails on a
        // missing dir) — this also keeps the no-op contract on a fresh path.
        std::fs::create_dir_all(&path)
            .map_err(|e| AdapterError::Ingest(format!("mkdir catalog {}: {e}", path.display())))?;
        let mut catalog = ParquetDataCatalog::new(&path, None, None, None, None);
        catalog
            .delete_data_range("bars", Some(&identifier), None, None)
            .map_err(|e| AdapterError::Ingest(format!("catalog delete {identifier}: {e}")))
    })
    .await
    .map_err(|e| AdapterError::Ingest(format!("catalog delete task panicked: {e}")))?
}

/// Read one bar-type series over a bounded `[start, end]` window, on a blocking
/// thread. This is the per-triple read primitive: [`read_all_bars`] loads the
/// entire catalog and must not be used per symbol (an accumulate run would
/// re-read the full multi-year catalog once per symbol — a cost small offline
/// fixtures never expose).
pub async fn read_bars_scoped(
    catalog_path: &Path,
    bar_type: BarType,
    start: Option<UnixNanos>,
    end: Option<UnixNanos>,
) -> AdapterResult<Vec<Bar>> {
    let path = catalog_path.to_path_buf();
    let identifier = bar_type.to_string();
    tokio::task::spawn_blocking(move || {
        let mut catalog = ParquetDataCatalog::new(&path, None, None, None, None);
        catalog
            .bars(Some(vec![identifier.clone()]), start, end)
            .map_err(|e| AdapterError::Ingest(format!("catalog scoped read {identifier}: {e}")))
    })
    .await
    .map_err(|e| AdapterError::Ingest(format!("catalog scoped read task panicked: {e}")))?
}

/// Read instrument definitions back from the catalog on a blocking thread (the
/// backtest loader, which loads instruments + bars from the catalog per F1).
pub async fn read_all_instruments(
    catalog_path: &Path,
) -> AdapterResult<Vec<nautilus_model::instruments::InstrumentAny>> {
    let path = catalog_path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let catalog = ParquetDataCatalog::new(&path, None, None, None, None);
        catalog
            .instruments(None, None, None)
            .map_err(|e| AdapterError::Ingest(format!("catalog read instruments: {e}")))
    })
    .await
    .map_err(|e| AdapterError::Ingest(format!("catalog read instruments task panicked: {e}")))?
}

// ---------------------------------------------------------------------------
// Catalog compaction (U5, KTD-5, R8/R9/R10) — the operator-run remediation that
// collapses byte-identical duplicate rows per series into a clean file set. It
// mutates only parquet files (never the checkpoint), holds the ingest advisory
// lock for the run, and is crash-recoverable via a per-series sidecar.
// ---------------------------------------------------------------------------

/// What compaction did to one series.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactOutcome {
    /// Byte-identical duplicates were removed and the series rewritten.
    Compacted,
    /// No duplicates — the series was already clean, nothing written.
    Clean,
    /// The series carries value-divergent same-timestamp rows — refused and left
    /// untouched (the heal path owns divergence, R9).
    RefusedDivergent,
}

/// Per-series compaction facts (R8): file and bar counts before/after + outcome.
#[derive(Debug, Clone)]
pub struct CompactSeriesReport {
    /// The bar-type identifier of the series.
    pub bar_type: String,
    /// Parquet file count before compaction.
    pub files_before: usize,
    /// Parquet file count after compaction.
    pub files_after: usize,
    /// Bar (row) count before compaction (raw, duplicate-inclusive).
    pub bars_before: usize,
    /// Bar (row) count after compaction.
    pub bars_after: usize,
    /// What compaction did.
    pub outcome: CompactOutcome,
}

/// A catalog compaction report, one entry per series.
#[derive(Debug, Clone)]
pub struct CompactReport {
    /// Per-series facts, in bar-type order.
    pub series: Vec<CompactSeriesReport>,
}

impl CompactReport {
    /// Whether any series was refused for value divergence (drives the CLI's
    /// non-zero exit).
    pub fn any_refused(&self) -> bool {
        self.series.iter().any(|s| s.outcome == CompactOutcome::RefusedDivergent)
    }
}

/// The sidecar directory beside the catalog (crash-recovery staging, KTD-5).
fn sidecars_dir(catalog_path: &Path) -> PathBuf {
    catalog_path.join("compact-sidecars")
}

/// The per-series sidecar path (`<catalog>/compact-sidecars/{bar_type}.json`).
fn sidecar_path(catalog_path: &Path, bar_type: BarType) -> PathBuf {
    sidecars_dir(catalog_path).join(format!("{}.json", bar_type))
}

/// Serialize the deduped bars to a series sidecar, atomically (temp + rename).
fn write_sidecar(path: &Path, bars: &[Bar]) -> AdapterResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AdapterError::Ingest(format!("mkdir {}: {e}", parent.display())))?;
    }
    let json = serde_json::to_string(bars)
        .map_err(|e| AdapterError::Ingest(format!("serialize sidecar: {e}")))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)
        .map_err(|e| AdapterError::Ingest(format!("write sidecar tmp {}: {e}", tmp.display())))?;
    std::fs::rename(&tmp, path)
        .map_err(|e| AdapterError::Ingest(format!("commit sidecar {}: {e}", path.display())))
}

/// Read a series sidecar's bars.
fn read_sidecar(path: &Path) -> AdapterResult<Vec<Bar>> {
    let s = std::fs::read_to_string(path)
        .map_err(|e| AdapterError::Ingest(format!("read sidecar {}: {e}", path.display())))?;
    serde_json::from_str(&s)
        .map_err(|e| AdapterError::Ingest(format!("corrupt sidecar {}: {e}", path.display())))
}

/// Remove a series sidecar; a missing sidecar is a no-op `Ok`.
fn remove_sidecar(path: &Path) -> AdapterResult<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AdapterError::Ingest(format!("remove sidecar {}: {e}", path.display()))),
    }
}

/// List the committed (`.json`) series sidecars, parsing each stem back to a
/// [`BarType`]. Partial `.json.tmp` sidecars (an interrupted write) are skipped —
/// the atomic rename guarantees a `.json` sidecar is complete.
fn list_sidecars(catalog_path: &Path) -> AdapterResult<Vec<(BarType, PathBuf)>> {
    let dir = sidecars_dir(catalog_path);
    let mut out = Vec::new();
    match std::fs::read_dir(&dir) {
        Ok(entries) => {
            for e in entries {
                let e = e.map_err(|e| AdapterError::Ingest(format!("read sidecar dir: {e}")))?;
                let path = e.path();
                if path.extension().and_then(|x| x.to_str()) != Some("json") {
                    continue;
                }
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(bt) = stem.parse::<BarType>() {
                        out.push((bt, path));
                    }
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(AdapterError::Ingest(format!("list sidecars: {e}"))),
    }
    // Deterministic order.
    out.sort_by_key(|(bt, _)| bt.to_string());
    Ok(out)
}

/// Rewrite one series from `bars` (already deduped + sorted) via the sidecar
/// sequence: stage the sidecar, `delete_bar_series`, re-append through the checked
/// wrapper (trivially disjoint after the delete, keeping raw `write_bars` out of
/// every production path), then drop the sidecar. A crash at any point leaves a
/// recoverable sidecar (KTD-5).
async fn rewrite_series(catalog_path: &Path, bar_type: BarType, bars: &[Bar]) -> AdapterResult<()> {
    let sidecar = sidecar_path(catalog_path, bar_type);
    write_sidecar(&sidecar, bars)?;
    delete_bar_series(catalog_path, bar_type).await?;
    append_bars_checked(catalog_path, bar_type, bars.to_vec()).await?;
    remove_sidecar(&sidecar)?;
    Ok(())
}

/// Fold a leftover sidecar back into its series (crash recovery, KTD-5): union the
/// sidecar rows with whatever the series currently holds (which may include bars
/// appended after the crash), collapse byte-identical duplicates, and rewrite —
/// idempotent across every crash point, losing no bar from either source.
async fn recover_sidecar(catalog_path: &Path, bar_type: BarType, sidecar: &Path) -> AdapterResult<()> {
    let mut union = read_sidecar(sidecar)?;
    union.extend(read_bars_scoped(catalog_path, bar_type, None, None).await?);
    dedup_bars(&mut union);
    union.sort_by_key(|b| b.ts_init.as_u64());
    rewrite_series(catalog_path, bar_type, &union).await
}

/// Compact one series' raw (duplicate-inclusive) rows.
async fn compact_one_series(
    catalog_path: &Path,
    bar_type: BarType,
    rows: Vec<Bar>,
) -> AdapterResult<CompactSeriesReport> {
    let files_before = stored_bar_intervals(catalog_path, bar_type).await?.len();
    let bars_before = rows.len();
    let mut deduped = rows;
    dedup_bars(&mut deduped);
    // A timestamp still carrying more than one row after byte-identical dedup is
    // value-divergent — refuse and leave the series untouched (R9).
    let mut per_ts: BTreeMap<u64, usize> = BTreeMap::new();
    for b in &deduped {
        *per_ts.entry(b.ts_event.as_u64()).or_default() += 1;
    }
    if per_ts.values().any(|&c| c > 1) {
        return Ok(CompactSeriesReport {
            bar_type: bar_type.to_string(),
            files_before,
            files_after: files_before,
            bars_before,
            bars_after: bars_before,
            outcome: CompactOutcome::RefusedDivergent,
        });
    }
    if deduped.len() == bars_before {
        return Ok(CompactSeriesReport {
            bar_type: bar_type.to_string(),
            files_before,
            files_after: files_before,
            bars_before,
            bars_after: bars_before,
            outcome: CompactOutcome::Clean,
        });
    }
    deduped.sort_by_key(|b| b.ts_init.as_u64());
    let bars_after = deduped.len();
    rewrite_series(catalog_path, bar_type, &deduped).await?;
    let files_after = stored_bar_intervals(catalog_path, bar_type).await?.len();
    Ok(CompactSeriesReport {
        bar_type: bar_type.to_string(),
        files_before,
        files_after,
        bars_before,
        bars_after,
        outcome: CompactOutcome::Compacted,
    })
}

/// Compact a catalog: collapse byte-identical duplicate bars per series into a
/// clean file set, refusing (and leaving untouched) any value-divergent series
/// (U5, R8/R9/R10). Holds the ingest advisory lock for the whole run (refuses
/// loudly if held), so no concurrent accumulate can write into the delete-rewrite
/// window. The checkpoint is never read or written (R10). Any leftover sidecar
/// from a prior crashed run is folded back first (KTD-5).
///
/// # Errors
///
/// [`AdapterError::Ingest`] if the lock is held or a catalog/sidecar I/O fails.
pub async fn compact_catalog(catalog_path: &Path) -> AdapterResult<CompactReport> {
    let _lock = AdvisoryLock::acquire(catalog_path, LockKind::Ingest)?;

    // Crash recovery first: fold any leftover sidecar back before enumerating, so
    // the raw read below reflects a consistent series set.
    for (bar_type, sidecar) in list_sidecars(catalog_path)? {
        recover_sidecar(catalog_path, bar_type, &sidecar).await?;
    }

    // Enumerate series from the raw (un-deduped) read so bars_before is the true
    // stored total and same-timestamp divergence is visible.
    let raw = read_all_bars_raw(catalog_path).await?;
    let mut groups: BTreeMap<BarType, Vec<Bar>> = BTreeMap::new();
    for b in raw {
        groups.entry(b.bar_type).or_default().push(b);
    }

    let mut series = Vec::new();
    for (bar_type, rows) in groups {
        series.push(compact_one_series(catalog_path, bar_type, rows).await?);
    }
    Ok(CompactReport { series })
}

// ---------------------------------------------------------------------------
// Max-lookback probe (U7, KTD10, R10) — the staged operator mode that locates the
// earliest served minute date for a pilot symbol and records it so the backfill can
// be sized. Both the probe and the backfill are operator-gated.
// ---------------------------------------------------------------------------

/// The recorded result of the minute-lookback probe (KTD10), persisted to
/// `<data>/probes/minute-lookback.json`. The backfill derives `LS_INGEST_LOOKBACK`
/// from either form — the explicit `earliest_date` or the rolling `depth_days`
/// (which keeps a lookback honest when the probe and the backfill run days apart).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinuteLookback {
    /// The earliest minute date the server served for the pilot (`YYYYMMDD`).
    pub earliest_date: String,
    /// Calendar-day depth from the earliest served date to the probe anchor.
    pub depth_days: i64,
    /// When the probe ran (RFC3339 — a stale probe should be re-run).
    pub probed_at: String,
}

fn fmt_ymd(d: NaiveDate) -> String {
    d.format("%Y%m%d").to_string()
}

/// The probes directory beside the catalog (`<data>/probes/`, KTD2).
pub fn probes_dir_for(catalog_path: &Path) -> PathBuf {
    catalog_path
        .parent()
        .map(|p| p.join("probes"))
        .unwrap_or_else(|| catalog_path.join("probes"))
}

/// Search backward in ≥7-calendar-day windows for the earliest served minute date on
/// `pilot` (KTD10). Each window spans at least a week so it always contains trading
/// days; only an **all-empty** window reads as beyond-lookback (a single-date probe
/// would converge wrongly on KRX weekends/holidays). Returns the earliest served date,
/// or `None` if the pilot serves nothing at all.
///
/// # Errors
///
/// Propagates a fetcher error (e.g. a pagination-limit on an over-wide window).
pub async fn probe_minute_lookback<F: MinuteFetcher>(
    fetcher: &F,
    pilot: &str,
    ncnt: u32,
    anchor: NaiveDate,
    window_days: i64,
    max_windows: usize,
) -> AdapterResult<Option<NaiveDate>> {
    let window_days = window_days.max(7);
    let mut earliest: Option<NaiveDate> = None;
    let mut wend = anchor;
    for _ in 0..max_windows.max(1) {
        let wstart = wend - ChronoDuration::days(window_days - 1);
        let resp = fetcher
            .fetch_minute_chunk(pilot, ncnt, &fmt_ymd(wstart), &fmt_ymd(wend))
            .await?;
        let mut wmin: Option<NaiveDate> = None;
        for row in resp.iter().flat_map(|r| &r.outblock1) {
            if let Ok(d) = NaiveDate::parse_from_str(row.date.trim(), "%Y%m%d") {
                wmin = Some(wmin.map_or(d, |m| m.min(d)));
            }
        }
        match wmin {
            // A non-empty window: extend the earliest and step the window back.
            Some(m) => {
                earliest = Some(earliest.map_or(m, |e| e.min(m)));
                wend = wstart - ChronoDuration::days(1);
            }
            // An all-empty window is beyond lookback — stop.
            None => break,
        }
    }
    Ok(earliest)
}

/// Persist a [`MinuteLookback`] to `<probes_dir>/minute-lookback.json` atomically
/// (temp file + rename, mirroring the checkpoint save).
pub fn write_minute_lookback(probes_dir: &Path, lb: &MinuteLookback) -> AdapterResult<()> {
    std::fs::create_dir_all(probes_dir)
        .map_err(|e| AdapterError::Ingest(format!("mkdir {}: {e}", probes_dir.display())))?;
    let json = serde_json::to_string_pretty(lb)
        .map_err(|e| AdapterError::Ingest(format!("serialize probe: {e}")))?;
    let path = probes_dir.join("minute-lookback.json");
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)
        .map_err(|e| AdapterError::Ingest(format!("write probe tmp {}: {e}", tmp.display())))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| AdapterError::Ingest(format!("commit probe {}: {e}", path.display())))
}

/// Read the recorded [`MinuteLookback`] from `<probes_dir>/minute-lookback.json`.
///
/// # Errors
///
/// [`AdapterError::Ingest`] if the file is missing or unparseable.
pub fn read_minute_lookback(probes_dir: &Path) -> AdapterResult<MinuteLookback> {
    let path = probes_dir.join("minute-lookback.json");
    let s = std::fs::read_to_string(&path)
        .map_err(|e| AdapterError::Ingest(format!("read probe {}: {e}", path.display())))?;
    serde_json::from_str(&s)
        .map_err(|e| AdapterError::Ingest(format!("corrupt probe {}: {e}", path.display())))
}

impl Ingestor {
    /// Run the staged max-lookback probe for a pilot symbol (KTD10): locate the
    /// earliest served minute date via a windowed backward search and, on success,
    /// write `<data>/probes/minute-lookback.json`. Returns the recorded result, or
    /// `None` when the pilot serves nothing (nothing is written).
    /// The staged max-lookback probe with an injected [`CalendarGate`] (U9, KTD8). Enforced-only
    /// after the ingest Consumer Retirement Gate (#189 U6): the anchor is the most recent proven
    /// Trading Session at or before `anchor`, and the probe STOPS (zero gateway requests, nothing
    /// recorded) when the calendar is unavailable or no session can be proven at/before it.
    pub async fn run_probe_lookback_gated(
        &self,
        pilot: &str,
        ncnt: u32,
        anchor: NaiveDate,
        probed_at: String,
        calendar: CalendarGate<'_>,
    ) -> AdapterResult<Option<MinuteLookback>> {
        let anchor = match calendar.probe_anchor(anchor) {
            ProbeAnchor::Use(a) => a,
            // Enforced + Unknown/unavailable: do not touch the gateway, record nothing.
            ProbeAnchor::Stop => return Ok(None),
        };
        let earliest = probe_minute_lookback(&self.fetcher, pilot, ncnt, anchor, 7, 400).await?;
        match earliest {
            Some(d) => {
                let lb = MinuteLookback {
                    earliest_date: fmt_ymd(d),
                    depth_days: anchor.signed_duration_since(d).num_days(),
                    probed_at,
                };
                write_minute_lookback(&probes_dir_for(&self.config.catalog_path), &lb)?;
                Ok(Some(lb))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kst_converts_with_date_rollover_at_midnight() {
        // 00:30 KST on 2024-01-05 = 15:30 UTC on 2024-01-04 (rolls back a day).
        let date = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        let time = NaiveTime::from_hms_opt(0, 30, 0).unwrap();
        let ns = kst_to_unix_nanos(date, time).unwrap();
        // Expected UTC: 2024-01-04 15:30:00.
        let expect = FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(2024, 1, 4, 15, 30, 0)
            .single()
            .unwrap()
            .timestamp_nanos_opt()
            .unwrap() as u64;
        assert_eq!(ns.as_u64(), expect);
    }

    #[test]
    fn daily_bar_close_is_1530_kst() {
        // 2024-01-05 daily close 15:30 KST = 06:30 UTC.
        let bar_type = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let row = T8410OutBlock1 {
            date: "20240105".to_string(),
            open: "60000".to_string(),
            high: "61000".to_string(),
            low: "59000".to_string(),
            close: "60500".to_string(),
            jdiff_vol: "1000000".to_string(),
            ..Default::default()
        };
        let bar = build_daily_bar(bar_type, &row).unwrap().unwrap();
        let expect = FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(2024, 1, 5, 6, 30, 0)
            .single()
            .unwrap()
            .timestamp_nanos_opt()
            .unwrap() as u64;
        assert_eq!(bar.ts_event.as_u64(), expect);
        assert_eq!(bar.close, Price::from("60500"));
    }

    #[test]
    fn last_closed_session_respects_the_post_close_buffer() {
        let day = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        // 18:00 KST (past the 16:30 buffer) → today is the last closed session.
        let evening = NaiveDateTime::new(day, NaiveTime::from_hms_opt(18, 0, 0).unwrap());
        assert_eq!(last_closed_session(evening, ACCUMULATE_CLOSE_BUFFER), day);
        // 10:00 KST (mid-session, before the buffer) → yesterday, never today (the
        // watermark must not advance into an in-session day, KTD7).
        let morning = NaiveDateTime::new(day, NaiveTime::from_hms_opt(10, 0, 0).unwrap());
        assert_eq!(
            last_closed_session(morning, ACCUMULATE_CLOSE_BUFFER),
            NaiveDate::from_ymd_opt(2024, 1, 4).unwrap()
        );
    }

    #[test]
    fn split_range_narrows_and_bottoms_out() {
        let s = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let e = NaiveDate::from_ymd_opt(2024, 1, 10).unwrap();
        let (left, right) = split_range(s, e).unwrap();
        assert!(left.1 < right.0); // disjoint halves
        assert_eq!(left.0, s);
        assert_eq!(right.1, e);
        // A single day cannot be narrowed.
        assert!(split_range(s, s).is_none());
    }

    // --- fetcher-loop fakes: cursor termination + PaginationLimit narrowing ---

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    fn daily_row(date: &str) -> T8410OutBlock1 {
        T8410OutBlock1 {
            date: date.to_string(),
            open: "100".to_string(),
            high: "110".to_string(),
            low: "90".to_string(),
            close: "105".to_string(),
            jdiff_vol: "1000".to_string(),
            ..Default::default()
        }
    }

    fn daily_resp(next_cursor: &str, row_date: &str) -> T8410Response {
        let mut resp = T8410Response {
            rsp_cd: "00000".to_string(),
            outblock1: vec![daily_row(row_date)],
            ..Default::default()
        };
        resp.outblock.cts_date = next_cursor.to_string();
        resp
    }

    struct FixedDaily {
        resp: T8410Response,
        calls: AtomicUsize,
    }
    #[async_trait]
    impl DailyFetcher for FixedDaily {
        async fn fetch_daily_page(&self, _shcode: &str, _sd: &str, _ed: &str, _cts: &str) -> AdapterResult<T8410Response> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.resp.clone())
        }
    }

    struct ErrDaily {
        code: String,
    }
    #[async_trait]
    impl DailyFetcher for ErrDaily {
        async fn fetch_daily_page(&self, _shcode: &str, _sd: &str, _ed: &str, _cts: &str) -> AdapterResult<T8410Response> {
            Err(AdapterError::Sdk(LsError::ApiError {
                code: self.code.clone(),
                message: "non-trading day".to_string(),
            }))
        }
    }

    #[tokio::test]
    async fn daily_bars_are_sorted_ascending_even_when_pages_are_newest_first() {
        // LS daily charts return newest-first; a single page here carries rows in
        // DESCENDING date order. The collector must sort ascending for the catalog.
        let bar_type = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let mut resp = T8410Response {
            rsp_cd: "00000".to_string(),
            outblock1: vec![daily_row("20240105"), daily_row("20240104"), daily_row("20240103")],
            ..Default::default()
        };
        resp.outblock.cts_date = String::new(); // single page
        let fetcher = FixedDaily { resp, calls: AtomicUsize::new(0) };
        let outcome = collect_daily(&fetcher, "005930", bar_type, "20240101", "20240131").await.unwrap();
        let bars = match outcome {
            TripleOutcome::Bars(b) => b,
            _ => panic!("expected bars"),
        };
        assert_eq!(bars.len(), 3);
        for w in bars.windows(2) {
            assert!(
                w[0].ts_init.as_u64() <= w[1].ts_init.as_u64(),
                "daily bars must be ascending for the catalog"
            );
        }
    }

    #[tokio::test]
    async fn daily_cursor_terminates_on_empty_next_cursor() {
        let bar_type = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let fetcher = FixedDaily {
            resp: daily_resp("", "20240105"),
            calls: AtomicUsize::new(0),
        };
        let outcome = collect_daily(&fetcher, "005930", bar_type, "20240101", "20240131").await.unwrap();
        assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1, "empty cursor stops after one page");
        assert!(matches!(outcome, TripleOutcome::Bars(ref b) if b.len() == 1));
    }

    #[tokio::test]
    async fn daily_cursor_defensive_stop_on_repeated_cursor() {
        let bar_type = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        // A gateway that echoes the same non-empty cursor forever must not loop.
        let fetcher = FixedDaily {
            resp: daily_resp("SAME", "20240105"),
            calls: AtomicUsize::new(0),
        };
        let outcome = collect_daily(&fetcher, "005930", bar_type, "20240101", "20240131").await.unwrap();
        assert_eq!(
            fetcher.calls.load(Ordering::SeqCst),
            2,
            "repeated cursor stops after the repeat is detected"
        );
        assert!(matches!(outcome, TripleOutcome::Bars(_)));
    }

    #[tokio::test]
    async fn daily_01715_becomes_non_trading_day_gap() {
        let bar_type = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let fetcher = ErrDaily { code: "01715".to_string() };
        let outcome = collect_daily(&fetcher, "005930", bar_type, "20240101", "20240131").await.unwrap();
        assert!(matches!(outcome, TripleOutcome::Gap(GapReason::NonTradingDay)));
    }

    /// A pacer-carrying fake so the gateway-context wrap (R9) reports a non-zero
    /// per-second cap like the production `SdkFetcher` does.
    struct PacedErrDaily {
        code: String,
    }
    #[async_trait]
    impl DailyFetcher for PacedErrDaily {
        async fn fetch_daily_page(&self, _shcode: &str, _sd: &str, _ed: &str, _cts: &str) -> AdapterResult<T8410Response> {
            Err(AdapterError::Sdk(LsError::ApiError {
                code: self.code.clone(),
                message: "rate limit exceeded".to_string(),
            }))
        }
        fn pace_per_sec(&self) -> u32 {
            1
        }
    }

    #[tokio::test]
    async fn daily_gateway_error_is_wrapped_with_tr_page_and_pacer_context() {
        // R9: a non-control-flow gateway error propagates wrapped with the TR code,
        // the page index, and the pacer cap — the raw LsError stays reachable via
        // Error::source for classification, and the message localizes the failure
        // without a raw-probe A/B. (IGW00201 is now a *control-flow* code for daily —
        // KTD-4 recovers it in-process — so this uses a genuinely-failing code.)
        let bar_type = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let fetcher = PacedErrDaily { code: "IGW00301".to_string() };
        let err = match collect_daily(&fetcher, "005930", bar_type, "20240101", "20240131").await {
            Ok(_) => panic!("expected a propagated gateway error"),
            Err(e) => e,
        };
        let AdapterError::IngestGateway { tr, page, per_sec, source } = &err else {
            panic!("expected an IngestGateway error, got {err:?}");
        };
        assert_eq!(*tr, "t8410", "TR code present");
        assert_eq!(*page, 1, "page index present");
        assert_eq!(*per_sec, 1, "pacer cap present");
        assert!(matches!(source.as_ref(), LsError::ApiError { code, .. } if code == "IGW00301"));
        let text = err.to_string();
        assert!(text.contains("t8410") && text.contains("page 1"), "context in Display: {text}");
        // The wrapped display carries no raw request body — only the TR/page/pace
        // and the gateway's own message.
        assert!(text.contains("rate limit exceeded"));
    }

    /// A daily fetcher that throttles (IGW00201) its first `throttle_first` calls,
    /// then serves a single terminating page (U3/KTD-4). `throttle_backoff` defaults
    /// to zero, so the test does not sleep.
    struct ThrottledDaily {
        throttle_first: usize,
        calls: AtomicUsize,
    }
    #[async_trait]
    impl DailyFetcher for ThrottledDaily {
        async fn fetch_daily_page(&self, _s: &str, _sd: &str, _ed: &str, _cts: &str) -> AdapterResult<T8410Response> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n < self.throttle_first {
                Err(AdapterError::Sdk(LsError::ApiError {
                    code: "IGW00201".to_string(),
                    message: "호출 거래건수를 초과하였습니다.".to_string(),
                }))
            } else {
                Ok(daily_resp("", "20240105")) // single terminating page, one row
            }
        }
    }

    #[tokio::test]
    async fn daily_igw00201_backs_off_retries_same_page_and_completes() {
        // KTD-4 regression: today `collect_daily` has no IGW00201 arm and aborts the
        // whole run on a throttle. The new arm backs off and retries the SAME page
        // (daily is ~1 page, so no narrowing), then completes with bars.
        let bar_type = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let fetcher = ThrottledDaily { throttle_first: 3, calls: AtomicUsize::new(0) };
        let outcome = collect_daily(&fetcher, "005930", bar_type, "20240101", "20240131")
            .await
            .expect("throttle recovers, does not abort the run");
        assert!(
            matches!(outcome, TripleOutcome::Bars(ref b) if !b.is_empty()),
            "backoff-and-retry recovers bars instead of aborting"
        );
        // 3 throttles + 1 success — the same page was retried, not narrowed.
        assert_eq!(fetcher.calls.load(Ordering::SeqCst), 4);
    }

    /// A daily fetcher whose budget is permanently dead: every call throttles.
    struct AlwaysThrottleDaily {
        calls: AtomicUsize,
    }
    #[async_trait]
    impl DailyFetcher for AlwaysThrottleDaily {
        async fn fetch_daily_page(&self, _s: &str, _sd: &str, _ed: &str, _cts: &str) -> AdapterResult<T8410Response> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(AdapterError::Sdk(LsError::ApiError {
                code: "IGW00201".to_string(),
                message: "호출 거래건수를 초과하였습니다.".to_string(),
            }))
        }
    }

    #[tokio::test]
    async fn daily_igw00201_dead_budget_degrades_to_gap_not_run_abort() {
        // A permanently-throttled daily budget terminates in bounded time (the
        // consecutive-throttle counter) and degrades to a thin GAP — watermark
        // withheld — instead of propagating an Err that aborts the multi-symbol run.
        let bar_type = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let fetcher = AlwaysThrottleDaily { calls: AtomicUsize::new(0) };
        let outcome = collect_daily(&fetcher, "005930", bar_type, "20240101", "20240131")
            .await
            .expect("a dead daily budget degrades to a gap, it does not error the run");
        assert!(
            matches!(outcome, TripleOutcome::Gap(GapReason::PaperThin)),
            "dead budget → thin gap (watermark withheld), got {:?}",
            std::mem::discriminant(&outcome)
        );
        let calls = fetcher.calls.load(Ordering::SeqCst);
        assert!(calls > 0 && calls < 1000, "bounded retries, not an infinite loop (calls={calls})");
    }

    fn minute_row(date: &str, time: &str) -> T8412OutBlock1 {
        T8412OutBlock1 {
            date: date.to_string(),
            time: time.to_string(),
            open: "100".to_string(),
            high: "110".to_string(),
            low: "90".to_string(),
            close: "105".to_string(),
            jdiff_vol: "10".to_string(),
            ..Default::default()
        }
    }

    fn minute_page(date: &str) -> T8412Response {
        T8412Response {
            rsp_cd: "00000".to_string(),
            outblock1: vec![minute_row(date, "0900")],
            ..Default::default()
        }
    }

    /// A fetcher that overflows (PaginationLimit) for any chunk spanning >2 days,
    /// and returns one row per narrow chunk otherwise. Records every requested
    /// range so the test can assert narrowing happened.
    struct NarrowingMinute {
        ranges: Mutex<Vec<(String, String)>>,
    }
    #[async_trait]
    impl MinuteFetcher for NarrowingMinute {
        async fn fetch_minute_chunk(
            &self,
            _shcode: &str,
            _ncnt: u32,
            sdate: &str,
            edate: &str,
        ) -> AdapterResult<Vec<T8412Response>> {
            self.ranges.lock().unwrap().push((sdate.to_string(), edate.to_string()));
            let s = NaiveDate::parse_from_str(sdate, "%Y%m%d").unwrap();
            let e = NaiveDate::parse_from_str(edate, "%Y%m%d").unwrap();
            if (e - s).num_days() > 2 {
                Err(AdapterError::Sdk(LsError::PaginationLimit(10)))
            } else {
                Ok(vec![minute_page(sdate)])
            }
        }
    }

    #[tokio::test]
    async fn minute_pagination_limit_narrows_and_ingests_all() {
        let bar_type = BarKind::Minute(1).bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let fetcher = NarrowingMinute { ranges: Mutex::new(Vec::new()) };
        let outcome = collect_minute(&fetcher, "005930", 1, bar_type, "20240101", "20240110")
            .await
            .unwrap();
        // Narrowing must have bottomed out into ≤2-day chunks that each returned a row.
        let bars = match outcome {
            TripleOutcome::Bars(b) => b,
            other => panic!("expected bars, got a gap: {:?}", std::mem::discriminant(&other)),
        };
        assert!(!bars.is_empty(), "narrowing should ingest rows");
        // ts_event ascending after the sort.
        for w in bars.windows(2) {
            assert!(w[0].ts_init.as_u64() <= w[1].ts_init.as_u64());
        }
        // The widest range was retried narrower (more than one distinct request).
        assert!(fetcher.ranges.lock().unwrap().len() > 1);
    }

    /// A fetcher that throttles (IGW00201) on any chunk spanning >2 days and returns
    /// one row per narrow chunk otherwise — modelling the rolling call-count budget
    /// that only serves a small range per window. `throttle_backoff` is zero so the
    /// test does not actually sleep.
    struct ThrottledMinute {
        ranges: Mutex<Vec<(String, String)>>,
    }
    #[async_trait]
    impl MinuteFetcher for ThrottledMinute {
        async fn fetch_minute_chunk(
            &self,
            _shcode: &str,
            _ncnt: u32,
            sdate: &str,
            edate: &str,
        ) -> AdapterResult<Vec<T8412Response>> {
            self.ranges.lock().unwrap().push((sdate.to_string(), edate.to_string()));
            let s = NaiveDate::parse_from_str(sdate, "%Y%m%d").unwrap();
            let e = NaiveDate::parse_from_str(edate, "%Y%m%d").unwrap();
            if (e - s).num_days() > 2 {
                Err(AdapterError::Sdk(LsError::ApiError {
                    code: "IGW00201".to_string(),
                    message: "호출 거래건수를 초과하였습니다.".to_string(),
                }))
            } else {
                Ok(vec![minute_page(sdate)])
            }
        }
    }

    #[tokio::test]
    async fn minute_igw00201_backs_off_narrows_and_ingests_all() {
        // Regression: a mid-range IGW00201 throttle must NOT abort the whole symbol
        // (discarding every bar) — it backs off and narrows so completed sub-ranges'
        // bars are retained and the deep pull makes incremental progress.
        let bar_type = BarKind::Minute(1).bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let fetcher = ThrottledMinute { ranges: Mutex::new(Vec::new()) };
        let outcome = collect_minute(&fetcher, "005930", 1, bar_type, "20240101", "20240110")
            .await
            .expect("throttle narrows and recovers rather than aborting");
        let bars = match outcome {
            TripleOutcome::Bars(b) => b,
            other => panic!("expected bars, got a gap: {:?}", std::mem::discriminant(&other)),
        };
        assert!(!bars.is_empty(), "IGW00201 narrowing recovers bars instead of aborting the symbol");
        for w in bars.windows(2) {
            assert!(w[0].ts_init.as_u64() <= w[1].ts_init.as_u64(), "ts ascending");
        }
        // The throttled wide range was narrowed and retried (more than one request).
        assert!(
            fetcher.ranges.lock().unwrap().len() > 1,
            "the throttled range was narrowed, not aborted"
        );
    }

    /// A fetcher whose budget is permanently dead: every chunk throttles.
    struct AlwaysThrottleMinute {
        calls: Mutex<usize>,
    }
    #[async_trait]
    impl MinuteFetcher for AlwaysThrottleMinute {
        async fn fetch_minute_chunk(
            &self,
            _shcode: &str,
            _ncnt: u32,
            _sdate: &str,
            _edate: &str,
        ) -> AdapterResult<Vec<T8412Response>> {
            *self.calls.lock().unwrap() += 1;
            Err(AdapterError::Sdk(LsError::ApiError {
                code: "IGW00201".to_string(),
                message: "호출 거래건수를 초과하였습니다.".to_string(),
            }))
        }
    }

    #[tokio::test]
    async fn minute_igw00201_dead_budget_degrades_to_gap_not_whole_run_abort() {
        // A permanently-throttled budget must terminate (bounded by
        // MAX_THROTTLE_RETRIES) and degrade to a thin GAP — NOT propagate an Err that
        // would abort the whole multi-symbol run and discard other symbols' bars.
        let bar_type = BarKind::Minute(1).bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let fetcher = AlwaysThrottleMinute { calls: Mutex::new(0) };
        let outcome = collect_minute(&fetcher, "005930", 1, bar_type, "20240101", "20240110")
            .await
            .expect("a dead budget degrades to a gap, it does not error the run");
        assert!(
            matches!(outcome, TripleOutcome::Gap(GapReason::PaperThin)),
            "dead budget → thin gap (bars withheld), got {:?}",
            std::mem::discriminant(&outcome)
        );
        // Terminated in bounded time — the consecutive-throttle counter capped the retries.
        let calls = *fetcher.calls.lock().unwrap();
        assert!(calls > 0 && calls < 10_000, "bounded retries, not an infinite loop (calls={calls})");
    }

    struct EmptyMinute;
    #[async_trait]
    impl MinuteFetcher for EmptyMinute {
        async fn fetch_minute_chunk(
            &self,
            _s: &str,
            _n: u32,
            _sd: &str,
            _ed: &str,
        ) -> AdapterResult<Vec<T8412Response>> {
            Ok(vec![]) // empty history
        }
    }

    // --- basis-shift overlap compare (KTD-3) — pure compare semantics ---

    fn ohlc_bar(date: &str, close: i64) -> Bar {
        let bar_type = BarKind::Daily.bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let row = T8410OutBlock1 {
            date: date.to_string(),
            open: (close - 5).to_string(),
            high: (close + 10).to_string(),
            low: (close - 10).to_string(),
            close: close.to_string(),
            jdiff_vol: "1000".to_string(),
            ..Default::default()
        };
        build_daily_bar(bar_type, &row).unwrap().unwrap()
    }

    #[test]
    fn overlap_matches_when_mutual_dates_agree() {
        let stored = vec![ohlc_bar("20240103", 100), ohlc_bar("20240104", 110), ohlc_bar("20240105", 120)];
        assert_eq!(compare_overlap(&stored, &stored.clone()), OverlapVerdict::Match);
    }

    #[test]
    fn read_dedup_drops_overlapping_duplicate_bars() {
        // Re-ingesting a range that overlaps stored bars writes a second parquet
        // file for the overlap window; the aggregate read then surfaces the same
        // (series, ts_event) twice. dedup_bars collapses them so the backtest's
        // prior→today universe scan never reads two copies of one session as a
        // nonsensical intraday self-gap (turn-2b certification catch).
        let mut bars = vec![
            ohlc_bar("20240103", 100),
            ohlc_bar("20240104", 110),
            ohlc_bar("20240104", 110), // byte-identical duplicate from the overlapping file
            ohlc_bar("20240105", 120),
            ohlc_bar("20240105", 120), // byte-identical duplicate
        ];
        dedup_bars(&mut bars);
        assert_eq!(bars.len(), 3, "byte-identical duplicates collapse to one each");
    }

    #[test]
    fn read_dedup_keeps_same_timestamp_bars_whose_values_differ() {
        // A same-(series, ts) bar with DIFFERENT OHLCV is a conflict, not a
        // redundant re-pull — an adjustment-basis shift or an in-range mutation the
        // finalize fingerprint re-check must catch. It must survive dedup.
        let mut bars = vec![ohlc_bar("20240104", 110), ohlc_bar("20240104", 60)];
        dedup_bars(&mut bars);
        assert_eq!(bars.len(), 2, "a value-divergent same-timestamp bar is not dropped");
    }

    // --- U4/KTD-4: distinct-session overlap tail ---

    #[test]
    fn overlap_tail_keeps_last_n_distinct_sessions_not_rows() {
        // Duplicates crowd the raw tail: a length-based window would keep the last 3
        // ROWS (three copies of one session → one distinct). The distinct-session
        // tail keeps the last 3 SESSIONS and all their rows — the dilution fix
        // (fails on the old row-length `overlap_tail`).
        let bars = vec![
            ohlc_bar("20240101", 1),
            ohlc_bar("20240102", 2),
            ohlc_bar("20240103", 3),
            ohlc_bar("20240104", 4),
            ohlc_bar("20240104", 4),
            ohlc_bar("20240104", 4),
            ohlc_bar("20240105", 5),
            ohlc_bar("20240105", 5),
            ohlc_bar("20240105", 5),
        ];
        let tail = overlap_tail(bars, 3);
        let distinct: std::collections::BTreeSet<u64> =
            tail.iter().map(|b| b.ts_event.as_u64()).collect();
        assert_eq!(distinct.len(), 3, "the last 3 DISTINCT sessions, not the last 3 rows");
        let oldest = ohlc_bar("20240103", 3).ts_event.as_u64();
        assert!(tail.iter().all(|b| b.ts_event.as_u64() >= oldest), "sessions before Jan3 dropped");
    }

    #[test]
    fn stored_overlap_gate_forces_shift_on_value_divergence() {
        // Two rows sharing a ts_event with different OHLC (survive byte-identical
        // dedup) → force the shift verdict directly (compare_overlap would hide it).
        let tail = vec![ohlc_bar("20240103", 100), ohlc_bar("20240103", 60)];
        assert!(matches!(stored_overlap_gate(&tail), TailGate::ForceShift));
    }

    #[test]
    fn stored_overlap_gate_counts_distinct_sessions_after_dedup() {
        // 4 rows, 2 distinct sessions (byte-identical dups) → Insufficient (< 3):
        // duplicates cannot fake sufficiency (AE5).
        let two = vec![
            ohlc_bar("20240104", 110),
            ohlc_bar("20240104", 110),
            ohlc_bar("20240105", 120),
            ohlc_bar("20240105", 120),
        ];
        assert!(matches!(stored_overlap_gate(&two), TailGate::Insufficient));
        // 5 byte-identical copies of ONE shifted session → 1 distinct → Insufficient.
        let five = vec![ohlc_bar("20240105", 120); 5];
        assert!(matches!(stored_overlap_gate(&five), TailGate::Insufficient));
        // 3 clean distinct sessions → proceed to the compare.
        let three = vec![ohlc_bar("20240103", 100), ohlc_bar("20240104", 110), ohlc_bar("20240105", 120)];
        assert!(matches!(stored_overlap_gate(&three), TailGate::Compare));
    }

    #[test]
    fn overlap_shifts_on_any_mutual_date_mismatch() {
        let stored = vec![ohlc_bar("20240103", 100), ohlc_bar("20240104", 110), ohlc_bar("20240105", 120)];
        // A post-split basis rewrites the whole series; one differing close is enough.
        let fetched = vec![ohlc_bar("20240103", 100), ohlc_bar("20240104", 110), ohlc_bar("20240105", 60)];
        assert_eq!(compare_overlap(&stored, &fetched), OverlapVerdict::Shifted);
    }

    #[test]
    fn overlap_excludes_one_sided_dates() {
        // Stored has a bar the server dropped; the server gap-filled a day with no
        // stored bar. Neither is a basis shift (KTD-3) — mutual dates agree.
        let stored = vec![
            ohlc_bar("20240102", 90),
            ohlc_bar("20240103", 100),
            ohlc_bar("20240105", 120),
            ohlc_bar("20240108", 130),
        ];
        let fetched = vec![
            ohlc_bar("20240102", 90),
            ohlc_bar("20240103", 100),
            ohlc_bar("20240104", 999), // server-side gap-fill, no stored counterpart
            ohlc_bar("20240105", 120),
            ohlc_bar("20240108", 130),
        ];
        assert_eq!(compare_overlap(&stored, &fetched), OverlapVerdict::Match);
    }

    #[test]
    fn overlap_insufficient_below_minimum_mutual_dates() {
        // Two mutual dates (< MIN_OVERLAP_DATES) — even a disagreement must skip
        // detection rather than mark.
        let stored = vec![ohlc_bar("20240104", 110), ohlc_bar("20240105", 120)];
        let fetched = vec![ohlc_bar("20240104", 55), ohlc_bar("20240105", 60)];
        assert_eq!(compare_overlap(&stored, &fetched), OverlapVerdict::Insufficient);
        // Disjoint dates: zero mutual.
        let other = vec![ohlc_bar("20240110", 1), ohlc_bar("20240111", 2), ohlc_bar("20240112", 3)];
        assert_eq!(compare_overlap(&stored, &other), OverlapVerdict::Insufficient);
    }

    #[test]
    fn overlap_window_start_spans_holiday_clusters() {
        let wm = NaiveDate::from_ymd_opt(2024, 10, 2).unwrap();
        let start = overlap_window_start(wm, DEFAULT_OVERLAP_DAYS);
        // 5 trading days ending at the watermark must fit even across a Chuseok
        // cluster + weekends (25 calendar days for the default knob).
        assert_eq!((wm - start).num_days(), 25);
    }

    #[tokio::test]
    async fn minute_empty_history_is_a_gap_not_a_failure() {
        let bar_type = BarKind::Minute(1).bar_type(InstrumentId::from("005930.XKRX")).unwrap();
        let outcome = collect_minute(&EmptyMinute, "005930", 1, bar_type, "20240101", "20240105")
            .await
            .unwrap();
        assert!(matches!(outcome, TripleOutcome::Gap(GapReason::EmptyHistory)));
    }

    // --- U4/KTD-1: subtract_covered range arithmetic ---

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn subtract_no_coverage_yields_the_whole_window() {
        // Steady state: no far coverage → exactly the single window segment.
        let out = subtract_covered(d(2024, 1, 6), d(2024, 1, 8), &[]);
        assert_eq!(out, vec![(d(2024, 1, 6), d(2024, 1, 8))]);
    }

    #[test]
    fn subtract_one_span_yields_gap_before_it() {
        // A far range [10..12] separated from the window start by a gap [6..9].
        let out = subtract_covered(d(2024, 1, 6), d(2024, 1, 12), &[(d(2024, 1, 10), d(2024, 1, 12))]);
        assert_eq!(out, vec![(d(2024, 1, 6), d(2024, 1, 9))], "only the un-covered gap remains");
    }

    #[test]
    fn subtract_span_across_last_closed_absorbs_the_tail() {
        // The far range's edate is at/above last_closed: the gap before it is the
        // only sub-range, and nothing trails (the far range covers the rest).
        let out = subtract_covered(d(2024, 1, 6), d(2024, 1, 11), &[(d(2024, 1, 10), d(2024, 1, 12))]);
        assert_eq!(out, vec![(d(2024, 1, 6), d(2024, 1, 9))]);
    }

    #[test]
    fn subtract_multiple_spans_yields_each_gap_and_the_forward_tail() {
        // Two far ranges → three un-covered sub-ranges (two interior gaps + tail).
        let out = subtract_covered(
            d(2024, 1, 6),
            d(2024, 1, 19),
            &[(d(2024, 1, 10), d(2024, 1, 11)), (d(2024, 1, 16), d(2024, 1, 17))],
        );
        assert_eq!(
            out,
            vec![
                (d(2024, 1, 6), d(2024, 1, 9)),
                (d(2024, 1, 12), d(2024, 1, 15)),
                (d(2024, 1, 18), d(2024, 1, 19)),
            ]
        );
    }

    #[test]
    fn subtract_fully_covered_window_yields_nothing() {
        // A span covering the whole window → no sub-ranges (nothing to fetch).
        let out = subtract_covered(d(2024, 1, 6), d(2024, 1, 12), &[(d(2024, 1, 6), d(2024, 1, 12))]);
        assert!(out.is_empty());
    }

    #[test]
    fn subtract_future_span_beyond_window_keeps_the_window() {
        // A far range entirely beyond last_closed does not truncate the window —
        // the whole window is un-covered and fetched (the future span is not
        // reachable yet and must not be skipped).
        let out = subtract_covered(d(2024, 1, 6), d(2024, 1, 8), &[(d(2024, 1, 15), d(2024, 1, 16))]);
        assert_eq!(out, vec![(d(2024, 1, 6), d(2024, 1, 8))]);
    }
}
