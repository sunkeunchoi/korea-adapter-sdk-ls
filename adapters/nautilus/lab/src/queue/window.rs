//! Window derivation (U2, R1/R2/R3; KTD1) — calendar view + clock in,
//! `KnownClosed` / `PresumedOpen` / `GenuinelyUnknown` + next boundary out.
//!
//! Window state is derived from TWO existing seams, never the calendar alone
//! (KTD1): the [`CalendarDateFact`] tri-state from the loaded calendar view,
//! composed with the preserved 09:00–15:30 KST time-of-day seam
//! ([`WeekdayKrxCalendar::in_time_window`], `540..=930` INCLUSIVE of the 15:30
//! minute). A known closure is closed; a retrospective `Unknown` within
//! snapshot coverage and not a known closure is **presumed-open** during the
//! intraday window and closed outside it; unconfigured / unavailable /
//! outside-coverage is **genuinely-unknown** and fails closed.
//!
//! CRITICAL domain fact: the calendar's session status is retrospective-only —
//! today reads `Unknown` for its ENTIRE duration BY DESIGN (see
//! docs/solutions/architecture-patterns/krx-session-status-is-retrospective-only-unknown-is-not-a-defect.md).
//! `Unknown` is never an error here, and no caller may expect `TradingSession`
//! for today.
//!
//! The core is a pure function over facts + a passed clock — no `now()`, no
//! env, no I/O — so tests pin exact instants and the entry report (U5) supplies
//! the `CalendarDateFact` values it obtains itself.

use chrono::{DateTime, NaiveDate, TimeZone, Timelike, Utc};
use chrono_tz::Asia::Seoul;

use crate::dispatch::checks::{CalendarDateFact, TradingCalendar, WeekdayKrxCalendar};
use crate::queue::Window;

/// The KST open minute (09:00), inclusive — the preserved seam's lower bound.
const OPEN_MIN: u32 = 9 * 60;

/// The KST close minute (15:30), inclusive — the preserved seam's upper bound
/// (`540..=930`; the doc-comment folklore that 15:30 is out is wrong, the code
/// is authoritative).
const CLOSE_MIN: u32 = 15 * 60 + 30;

/// How many KST dates the close→open boundary scan walks before giving up. A
/// 30-day all-closed run has never occurred on KRX; past the cap the boundary
/// is reported as underivable (`None`) rather than fabricated.
const BOUNDARY_SCAN_DAYS: u32 = 30;

/// What the composition root knows about TODAY's KST date (KTD1). The entry
/// report maps its own calendar resolution here: a loaded view's
/// [`date_fact_from_view`](crate::dispatch::checks::date_fact_from_view) answer
/// becomes [`Fact`](DateEvidence::Fact); a `LoadedCalendar::NotConfigured` /
/// `::Unavailable` resolution becomes the matching variant WITHOUT ever
/// querying a view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateEvidence {
    /// A loaded calendar view answered for the date. Note
    /// [`CalendarDateFact::Unavailable`] from a LOADED view is a failed date
    /// query (out-of-range / use failure) and reads as out-of-coverage here.
    Fact(CalendarDateFact),
    /// No snapshot path is configured (`LS_CALENDAR_SNAPSHOT` unset/empty).
    NotConfigured,
    /// A snapshot path is configured but the snapshot failed to load.
    Unavailable,
}

/// Why the window is genuinely unknown (R3): each reason carries its own
/// repair action so the entry report can print the fix, not just the refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownReason {
    /// No calendar snapshot is configured at all.
    NotConfigured,
    /// A snapshot is configured but could not be loaded/used.
    Unavailable,
    /// The snapshot loaded but does not cover this KST date.
    OutOfCoverage,
}

impl UnknownReason {
    /// The operator repair action for this reason (printed by the entry
    /// report next to the `any`-only eligibility notice).
    pub fn repair_action(self) -> &'static str {
        match self {
            UnknownReason::NotConfigured => {
                "set LS_CALENDAR_SNAPSHOT=adapters/nautilus/state/krx.calendar.json — no calendar snapshot path is configured"
            }
            UnknownReason::Unavailable => {
                "the configured calendar snapshot failed to load — inspect/re-activate adapters/nautilus/state/krx.calendar.json (see adapters/nautilus/RUNBOOK-calendar-snapshot.md)"
            }
            UnknownReason::OutOfCoverage => {
                "the date is outside snapshot coverage — run the calendar refresh chain (adapters/nautilus/lab/RUNBOOK-session-morning.md Steps 1–3)"
            }
        }
    }
}

/// Why a `KnownClosed` window is closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosedReason {
    /// The venue is PROVEN Closed on this KST date (calendar authority).
    ClosureDate,
    /// A non-closed date, but the clock is outside the 09:00–15:30 KST minute
    /// window — closed by the preserved time-of-day seam, no calendar needed.
    OutsideHours,
}

/// The derived window state (KTD1). Eligibility is a property of the state
/// (see [`admits`](WindowState::admits)) so callers never re-derive it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    /// Closed with certainty — a proven closure date, or outside the intraday
    /// minute window.
    KnownClosed(ClosedReason),
    /// A retrospective-`Unknown` (or witnessed `TradingSession`) date inside
    /// the 09:00–15:30 KST window — presumed open, the normal live-day state.
    PresumedOpen,
    /// The calendar cannot answer — fail closed: only window-agnostic items
    /// plus the reason's repair action are offered (R3).
    GenuinelyUnknown(UnknownReason),
}

impl WindowState {
    /// Whether an item with window requirement `item` is eligible in this
    /// state (R4's window-compatibility predicate): `Any` always; `OpenAttended`
    /// only during presumed-open; `Closed` only during a known-closed window.
    /// Genuinely-unknown admits ONLY `Any` — fail closed.
    pub fn admits(&self, item: Window) -> bool {
        match (self, item) {
            (_, Window::Any) => true,
            (WindowState::PresumedOpen, Window::OpenAttended) => true,
            (WindowState::KnownClosed(_), Window::Closed) => true,
            _ => false,
        }
    }
}

/// The next window boundary, as a UTC instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextBoundary {
    /// Open→close: the instant the 15:30 KST minute ENDS (15:31:00 KST) — the
    /// seam is inclusive of the whole close minute.
    Close(DateTime<Utc>),
    /// Close→open: the next 09:00:00 KST on a non-closed date.
    Open(DateTime<Utc>),
}

/// One static morning-chain step (from
/// `adapters/nautilus/lab/RUNBOOK-session-morning.md`): a name, the exact
/// action, and its KST minute window. The table is CLOCK-ONLY — there is no
/// morning-chain runtime state, so the pointer says "the earliest step whose
/// deadline has not passed", and the operator knows which steps are already
/// behind them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainStep {
    /// Stable kebab-case step name.
    pub name: &'static str,
    /// The exact runbook step / command pointer (R5).
    pub action: &'static str,
    /// Earliest KST minute-of-day the step can start.
    pub from_min: u32,
    /// KST minute-of-day deadline (inclusive) — past it the step is behind.
    pub deadline_min: u32,
}

/// The static attended morning-chain table (RUNBOOK-session-morning.md).
/// Pre-open steps must finish before the 09:00 open; Step 6 (mount universe)
/// must run AFTER 09:00 (before it, t8407 answers with the previous session's
/// snapshot); post-close starts after the 15:30 minute ends. Deadline-ordered
/// so [`next_chain_step`]'s first-not-expired lookup is total.
pub const MORNING_CHAIN: &[ChainStep] = &[
    ChainStep {
        name: "source-credentials",
        action: "Step 0 — source .env.calendar at the top of the session shell (set -a; . $R/.env.calendar; set +a)",
        from_min: 0,
        deadline_min: OPEN_MIN - 1,
    },
    ChainStep {
        name: "krx-witness-probe",
        action: "Step 1 — the KRX decision probe for the previous session (rows>0 → refresh; 0 rows → Step 5 fidelity decision)",
        from_min: 0,
        deadline_min: OPEN_MIN - 1,
    },
    ChainStep {
        name: "calendar-refresh",
        action: "Steps 2–3 — archive the active snapshot, calendar-fetch-inputs, calendar-refresh (want partial=false), calendar-activate",
        from_min: 0,
        deadline_min: OPEN_MIN - 1,
    },
    ChainStep {
        name: "catalog-advance",
        action: "Step 4 — bounded ls-ingest accumulate (LS_INGEST_SKIP_UNIVERSE_LOAD=1 + explicit LS_INGEST_SYMBOLS); verify by watermark, never exit code",
        from_min: 0,
        deadline_min: OPEN_MIN - 1,
    },
    ChainStep {
        name: "mount-universe",
        action: "Step 6 — lab-mount-universe --out <path> (AFTER 09:00 KST; LS_DATA_HOME + LS_MOUNT_UNIVERSE_DATE + LS_MOUNT_UNIVERSE_METADATA + LS_DISPATCH_LANE_ENV all required)",
        from_min: OPEN_MIN,
        deadline_min: CLOSE_MIN,
    },
    ChainStep {
        name: "attended-session",
        action: "Step 7 — operator-only: --genesis → --dispatch → --mount per RUNBOOK-rung1.md (exit codes are the contract)",
        from_min: OPEN_MIN,
        deadline_min: CLOSE_MIN,
    },
    ChainStep {
        name: "post-close",
        action: "Post-close — ingest → tracking → --rung-report (today's session ingests only after ITS witness publishes, tomorrow)",
        from_min: CLOSE_MIN + 1,
        deadline_min: 24 * 60 - 1,
    },
];

/// The static clock-only morning-chain pointer: the first step in
/// [`MORNING_CHAIN`] whose deadline has not passed at `kst_minute` (minute of
/// the KST day). `None` only past the last deadline (never during the intraday
/// window).
pub fn next_chain_step(kst_minute: u32) -> Option<&'static ChainStep> {
    MORNING_CHAIN.iter().find(|s| s.deadline_min >= kst_minute)
}

/// The full derivation output: the state, the next window boundary (when one
/// is derivable), and the attended-chain pointer (presumed-open only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowReport {
    /// The derived window state.
    pub state: WindowState,
    /// The next boundary as a UTC instant; `None` when genuinely unknown (no
    /// boundary is derivable without a calendar) or past the scan cap.
    pub next_boundary: Option<NextBoundary>,
    /// The static attended morning-chain pointer — `Some` only during
    /// presumed-open (R2's "what the operator does next" surface).
    pub next_attended_step: Option<&'static ChainStep>,
}

/// Derive the window state at `now_utc` (KTD1).
///
/// `today` is the caller's evidence for **`now_utc`'s KST civil date** — the
/// caller resolves the calendar once and maps it to [`DateEvidence`] itself.
/// `fact_for` answers date-fact queries for OTHER dates (the close→open
/// boundary scan); a caller with a loaded view passes
/// `|d| date_fact_from_view(Some(view), d)` and tests pass a closure.
///
/// State mapping (KTD1, fail closed):
/// - `Fact(Closed)` → [`WindowState::KnownClosed`] (`ClosureDate`), boundary =
///   next 09:00 KST on a non-closed date.
/// - `Fact(Unknown | TradingSession)` → presumed-open inside the preserved
///   09:00–15:30 KST seam (boundary = 15:31 KST today); outside it,
///   `KnownClosed` (`OutsideHours`) with boundary today 09:00 (pre-open) or the
///   next non-closed date's 09:00 (post-close). `TradingSession` cannot occur
///   for today (retrospective-only) but a witnessed date composes identically.
/// - `NotConfigured` / `Unavailable` / `Fact(Unavailable)` →
///   [`WindowState::GenuinelyUnknown`] with the matching [`UnknownReason`],
///   regardless of the clock — an unanswerable calendar fails closed even at
///   an hour that is trivially outside the session.
pub fn derive_window<F>(now_utc: DateTime<Utc>, today: DateEvidence, fact_for: F) -> WindowReport
where
    F: Fn(NaiveDate) -> CalendarDateFact,
{
    let kst = now_utc.with_timezone(&Seoul);
    let kst_date = kst.date_naive();
    let minute = kst.hour() * 60 + kst.minute();
    // The PRESERVED time-of-day seam — composed, never re-derived (KTD1).
    let in_window = WeekdayKrxCalendar.in_time_window(now_utc);

    let genuinely_unknown = |reason| WindowReport {
        state: WindowState::GenuinelyUnknown(reason),
        next_boundary: None,
        next_attended_step: None,
    };

    match today {
        DateEvidence::NotConfigured => genuinely_unknown(UnknownReason::NotConfigured),
        DateEvidence::Unavailable => genuinely_unknown(UnknownReason::Unavailable),
        DateEvidence::Fact(CalendarDateFact::Unavailable) => {
            genuinely_unknown(UnknownReason::OutOfCoverage)
        }
        DateEvidence::Fact(CalendarDateFact::Closed) => WindowReport {
            state: WindowState::KnownClosed(ClosedReason::ClosureDate),
            next_boundary: next_open_boundary(kst_date.succ_opt(), &fact_for),
            next_attended_step: None,
        },
        DateEvidence::Fact(CalendarDateFact::Unknown | CalendarDateFact::TradingSession) => {
            if in_window {
                WindowReport {
                    state: WindowState::PresumedOpen,
                    // Open→close fires after the 15:30 minute ENDS.
                    next_boundary: Some(NextBoundary::Close(kst_instant(kst_date, CLOSE_MIN + 1))),
                    next_attended_step: next_chain_step(minute),
                }
            } else if minute < OPEN_MIN {
                // Pre-open on a non-closed date: today's own 09:00 is the boundary.
                WindowReport {
                    state: WindowState::KnownClosed(ClosedReason::OutsideHours),
                    next_boundary: Some(NextBoundary::Open(kst_instant(kst_date, OPEN_MIN))),
                    next_attended_step: None,
                }
            } else {
                // Post-close: scan forward from tomorrow.
                WindowReport {
                    state: WindowState::KnownClosed(ClosedReason::OutsideHours),
                    next_boundary: next_open_boundary(kst_date.succ_opt(), &fact_for),
                    next_attended_step: None,
                }
            }
        }
    }
}

/// The close→open boundary: the next 09:00 KST on a NON-closed date at or
/// after `from` (a retrospective `Unknown` or an out-of-coverage `Unavailable`
/// date counts as non-closed — only a PROVEN closure defers the boundary).
/// `None` past the scan cap or at the calendar-date limit.
fn next_open_boundary<F>(from: Option<NaiveDate>, fact_for: &F) -> Option<NextBoundary>
where
    F: Fn(NaiveDate) -> CalendarDateFact,
{
    let mut date = from?;
    for _ in 0..BOUNDARY_SCAN_DAYS {
        if fact_for(date) != CalendarDateFact::Closed {
            return Some(NextBoundary::Open(kst_instant(date, OPEN_MIN)));
        }
        date = date.succ_opt()?;
    }
    None
}

/// The UTC instant of `minute` (minute-of-day) on the KST civil date `date`.
/// Seoul has no DST, so every KST local time maps to exactly one instant.
fn kst_instant(date: NaiveDate, minute: u32) -> DateTime<Utc> {
    let local = date
        .and_hms_opt(minute / 60, minute % 60, 0)
        .expect("minute-of-day is always < 1440");
    Seoul
        .from_local_datetime(&local)
        .single()
        .expect("KST has no DST — every local time is unambiguous")
        .with_timezone(&Utc)
}
