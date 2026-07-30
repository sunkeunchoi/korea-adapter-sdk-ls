//! Window-derivation tests (U2, R1/R2/R3; KTD1; AE2). Pure-function tests —
//! every instant is pinned (the core takes a passed clock, never `now()`), the
//! calendar facts are supplied directly, and no subprocess or snapshot file is
//! involved.
//!
//! Domain fact under test: the calendar's session status is retrospective-only —
//! today reads `Unknown` for its entire duration BY DESIGN, so `Unknown` within
//! coverage is presumed-open during the 09:00–15:30 KST window, never an error.

use chrono::{DateTime, NaiveDate, Utc};

use nautilus_ls_lab::dispatch::checks::CalendarDateFact;
use nautilus_ls_lab::queue::window::{
    derive_window, next_chain_step, ClosedReason, DateEvidence, NextBoundary, UnknownReason,
    WindowState, MORNING_CHAIN,
};
use nautilus_ls_lab::queue::Window;

fn utc(s: &str) -> DateTime<Utc> {
    s.parse().unwrap_or_else(|e| panic!("bad test instant {s:?}: {e}"))
}

fn date(s: &str) -> NaiveDate {
    s.parse().unwrap_or_else(|e| panic!("bad test date {s:?}: {e}"))
}

/// The normal live-day calendar: every date reads retrospective `Unknown`.
fn all_unknown(_: NaiveDate) -> CalendarDateFact {
    CalendarDateFact::Unknown
}

// ---------------------------------------------------------------------------
// AE2: presumed-open weekday morning
// ---------------------------------------------------------------------------

#[test]
fn ae2_unknown_in_coverage_at_0905_kst_is_presumed_open_with_attended_step() {
    // 2026-07-29 (Wed) 09:05 KST == 00:05 UTC; today reads Unknown BY DESIGN.
    let report = derive_window(
        utc("2026-07-29T00:05:00Z"),
        DateEvidence::Fact(CalendarDateFact::Unknown),
        all_unknown,
    );
    assert_eq!(report.state, WindowState::PresumedOpen);
    // The attended-chain pointer is surfaced, static from the runbook table.
    let step = report.next_attended_step.expect("presumed-open surfaces the attended next step");
    assert_eq!(step.name, "mount-universe", "09:05 points at Step 6 (after 09:00 KST)");
    assert!(!step.action.is_empty(), "the step carries its exact action text");
    // Open→close boundary fires after the 15:30 minute ends: 15:31 KST == 06:31 UTC.
    assert_eq!(
        report.next_boundary,
        Some(NextBoundary::Close(utc("2026-07-29T06:31:00Z")))
    );
    // Eligibility during presumed-open: open-attended + any, not closed-window items.
    assert!(report.state.admits(Window::OpenAttended));
    assert!(report.state.admits(Window::Any));
    assert!(!report.state.admits(Window::Closed));
}

// ---------------------------------------------------------------------------
// Known closure / retrospective-Unknown outside hours
// ---------------------------------------------------------------------------

#[test]
fn known_closed_date_at_1000_kst_is_known_closed_with_next_open_on_a_non_closed_date() {
    // 10:00 KST == 01:00 UTC on a proven-Closed date; 07-30 also Closed, 07-31 Unknown.
    let facts = |d: NaiveDate| {
        if d <= date("2026-07-30") {
            CalendarDateFact::Closed
        } else {
            CalendarDateFact::Unknown
        }
    };
    let report = derive_window(
        utc("2026-07-29T01:00:00Z"),
        DateEvidence::Fact(CalendarDateFact::Closed),
        facts,
    );
    assert_eq!(report.state, WindowState::KnownClosed(ClosedReason::ClosureDate));
    assert_eq!(report.next_attended_step, None, "no attended pointer outside presumed-open");
    // Close→open at the next 09:00 KST on a non-closed date: skips Closed 07-30,
    // lands on 07-31 09:00 KST == 00:00 UTC.
    assert_eq!(
        report.next_boundary,
        Some(NextBoundary::Open(utc("2026-07-31T00:00:00Z")))
    );
    // Eligibility while closed: closed-window + any, not open-attended.
    assert!(report.state.admits(Window::Closed));
    assert!(report.state.admits(Window::Any));
    assert!(!report.state.admits(Window::OpenAttended));
}

#[test]
fn retrospective_unknown_at_1600_kst_is_a_closed_window() {
    // 16:00 KST == 07:00 UTC — after the close, on today's normal Unknown.
    let report = derive_window(
        utc("2026-07-29T07:00:00Z"),
        DateEvidence::Fact(CalendarDateFact::Unknown),
        all_unknown,
    );
    assert_eq!(report.state, WindowState::KnownClosed(ClosedReason::OutsideHours));
    // Next open is tomorrow 09:00 KST (2026-07-30T00:00:00Z) — tomorrow reads Unknown.
    assert_eq!(
        report.next_boundary,
        Some(NextBoundary::Open(utc("2026-07-30T00:00:00Z")))
    );
    assert_eq!(report.next_attended_step, None);
}

// ---------------------------------------------------------------------------
// Genuinely-unknown: not-configured / unavailable / out-of-coverage
// ---------------------------------------------------------------------------

#[test]
fn not_configured_snapshot_is_genuinely_unknown_and_admits_only_any() {
    let report = derive_window(
        utc("2026-07-29T00:05:00Z"),
        DateEvidence::NotConfigured,
        all_unknown,
    );
    assert_eq!(
        report.state,
        WindowState::GenuinelyUnknown(UnknownReason::NotConfigured)
    );
    assert_eq!(report.next_boundary, None, "no boundary is derivable without a calendar");
    assert_eq!(report.next_attended_step, None);
    // Only any-tagged items are eligible; the reason carries the repair action.
    assert!(report.state.admits(Window::Any));
    assert!(!report.state.admits(Window::Closed));
    assert!(!report.state.admits(Window::OpenAttended));
    assert!(
        UnknownReason::NotConfigured.repair_action().contains("LS_CALENDAR_SNAPSHOT"),
        "repair action names the missing configuration"
    );
}

#[test]
fn unavailable_snapshot_is_genuinely_unknown_and_admits_only_any() {
    let report = derive_window(
        utc("2026-07-29T00:05:00Z"),
        DateEvidence::Unavailable,
        all_unknown,
    );
    assert_eq!(
        report.state,
        WindowState::GenuinelyUnknown(UnknownReason::Unavailable)
    );
    assert!(report.state.admits(Window::Any));
    assert!(!report.state.admits(Window::Closed));
    assert!(!report.state.admits(Window::OpenAttended));
    assert!(!UnknownReason::Unavailable.repair_action().is_empty());
}

#[test]
fn date_outside_snapshot_coverage_is_genuinely_unknown_out_of_coverage() {
    // A loaded view whose day() query failed maps to CalendarDateFact::Unavailable
    // (checks.rs: out-of-range / use failure) — here that means out-of-coverage.
    let report = derive_window(
        utc("2026-07-29T00:05:00Z"),
        DateEvidence::Fact(CalendarDateFact::Unavailable),
        all_unknown,
    );
    assert_eq!(
        report.state,
        WindowState::GenuinelyUnknown(UnknownReason::OutOfCoverage)
    );
    assert!(report.state.admits(Window::Any));
    assert!(!report.state.admits(Window::OpenAttended));
    assert!(!UnknownReason::OutOfCoverage.repair_action().is_empty());
}

// ---------------------------------------------------------------------------
// Minute-window boundaries (the preserved 540..=930 seam, INCLUSIVE of 15:30)
// ---------------------------------------------------------------------------

#[test]
fn minute_boundaries_0859_out_0900_in_1530_in_1531_out() {
    let unknown_today = DateEvidence::Fact(CalendarDateFact::Unknown);
    // 08:59 KST == 2026-07-28T23:59 UTC (the KST date is still 07-29): out.
    let before = derive_window(utc("2026-07-28T23:59:00Z"), unknown_today, all_unknown);
    assert_eq!(before.state, WindowState::KnownClosed(ClosedReason::OutsideHours));
    // Pre-open on a non-closed date: opens TODAY at 09:00 KST == 00:00 UTC.
    assert_eq!(
        before.next_boundary,
        Some(NextBoundary::Open(utc("2026-07-29T00:00:00Z")))
    );

    // 09:00 KST == 00:00 UTC: inclusive open.
    let open = derive_window(utc("2026-07-29T00:00:00Z"), unknown_today, all_unknown);
    assert_eq!(open.state, WindowState::PresumedOpen);

    // 15:30 KST == 06:30 UTC: still in-window (540..=930 is inclusive of 930).
    let close_minute = derive_window(utc("2026-07-29T06:30:00Z"), unknown_today, all_unknown);
    assert_eq!(close_minute.state, WindowState::PresumedOpen);
    // The open→close boundary is one minute later: after the 15:30 minute ends.
    assert_eq!(
        close_minute.next_boundary,
        Some(NextBoundary::Close(utc("2026-07-29T06:31:00Z")))
    );

    // 15:31 KST == 06:31 UTC: out.
    let after = derive_window(utc("2026-07-29T06:31:00Z"), unknown_today, all_unknown);
    assert_eq!(after.state, WindowState::KnownClosed(ClosedReason::OutsideHours));
    assert_eq!(
        after.next_boundary,
        Some(NextBoundary::Open(utc("2026-07-30T00:00:00Z")))
    );
}

// ---------------------------------------------------------------------------
// The static morning-chain table
// ---------------------------------------------------------------------------

#[test]
fn morning_chain_lookup_is_static_clock_only() {
    // Pre-open (07:30 KST == 450): the first pre-open step is still ahead.
    let pre = next_chain_step(450).expect("pre-open step");
    assert_eq!(pre.name, "source-credentials", "the chain starts at Step 0");
    // In-window (09:05 KST == 545): the pre-open steps' deadlines have passed.
    let mid = next_chain_step(545).expect("in-window step");
    assert_eq!(mid.name, "mount-universe");
    // After the close minute ends (15:40 KST == 940): post-close.
    let post = next_chain_step(940).expect("post-close step");
    assert_eq!(post.name, "post-close");
    // The table is ordered by deadline so the first-not-expired lookup is total.
    let deadlines: Vec<u32> = MORNING_CHAIN.iter().map(|s| s.deadline_min).collect();
    let mut sorted = deadlines.clone();
    sorted.sort_unstable();
    assert_eq!(deadlines, sorted, "MORNING_CHAIN must be deadline-ordered");
}
