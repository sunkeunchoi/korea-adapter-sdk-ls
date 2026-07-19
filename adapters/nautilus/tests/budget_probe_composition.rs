//! U4 composition-root smoke for the `budget-probe` binary: explicit path resolution → one
//! load → injection → decision-relevant startup record → adoption-state reporting, plus the
//! end-to-end proof that an Enforced refusal (no proven session, no explicit range) issues
//! ZERO gateway requests — it refuses at the calendar gate BEFORE any SDK build or gateway
//! call. Fully offline: no production snapshot, no credentials, no network, no wall-clock
//! fixture (the synthetic snapshot's 2010–2012 coverage is unconditionally in the past, so
//! the refusal holds for any real run date).

use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{TimeZone, Utc};
use nautilus_ls::calendar::{resolve_and_load, CalendarAdoption};
use nautilus_ls_calendar::schema::DayStatus;

/// The committed synthetic (counterfactual) fixture. Its granting authority is a maintainer
/// identity the redaction must mask — the startup line must never contain it verbatim.
const FIXTURE_AUTHORITY: &str = "SYNTHETIC-MAINTAINER";

fn fixture_snapshot() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("nautilus-ls-calendar/fixtures/base_2010_2012.json")
}

/// Copy the committed synthetic fixture into `dir` so the smoke owns a deletable, explicitly
/// synthetic snapshot at a temporary path (a production snapshot is never touched).
fn temp_snapshot(dir: &Path) -> PathBuf {
    let dst = dir.join("calendar.json");
    std::fs::copy(fixture_snapshot(), &dst).expect("fixture copies");
    dst
}

fn probe() -> Command {
    Command::new(env!("CARGO_BIN_EXE_budget-probe"))
}

/// AE7 (scenario 3): Enforced with no proven session and no explicit range issues ZERO
/// gateway requests — the process refuses at the calendar gate (before any SDK build), and
/// emits exactly one decision-relevant, redacted startup record naming the adoption.
#[test]
fn enforced_no_session_refuses_before_any_gateway_call() {
    let dir = tempfile::tempdir().unwrap();
    let snap = temp_snapshot(dir.path());

    let out = probe()
        .env("LS_TRADING_ENV", "paper")
        .env("LS_CALENDAR_ADOPTION", "enforced")
        .env("LS_CALENDAR_SNAPSHOT", &snap)
        // A junk lane file guarantees no real credentials are ever resolved — but the probe
        // must refuse BEFORE it ever reaches the SDK build, so this is never consulted.
        .env("LS_PROBE_LANE_FILE", dir.path().join("does-not-exist.env"))
        .output()
        .expect("budget-probe runs");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "the Enforced no-session refusal exits non-zero: {stderr}");

    // Exactly one startup record, naming the Enforced adoption, redacted.
    assert_eq!(
        stderr.matches("calendar-startup").count(),
        1,
        "exactly one mandatory startup record: {stderr}"
    );
    assert!(stderr.contains("adoption=enforced"), "startup names the adoption: {stderr}");
    assert!(
        !stderr.contains(FIXTURE_AUTHORITY),
        "the granting authority must never leak into the startup line: {stderr}"
    );

    // The refusal is the CALENDAR gate refusal — proving it never reached the SDK/gateway.
    // (An SDK/credential error would read completely differently; a real gateway call is
    // impossible with the junk lane file.)
    assert!(
        stderr.contains("refusing to probe"),
        "the process refuses at the calendar gate, before any gateway call: {stderr}"
    );
    assert!(
        !stderr.contains("stage 0") && !stderr.contains("stage 1"),
        "no probe stage ran (no gateway call issued): {stderr}"
    );
}

/// U1 scenario 6: the always-emit invariant survives the single-load consolidation — a
/// non-paper invocation still emits exactly one mandatory startup record before it refuses.
#[test]
fn non_paper_invocation_still_emits_exactly_one_startup_record() {
    let dir = tempfile::tempdir().unwrap();
    let snap = temp_snapshot(dir.path());

    let out = probe()
        // LS_TRADING_ENV intentionally unset → the paper gate refuses.
        .env_remove("LS_TRADING_ENV")
        .env("LS_CALENDAR_ADOPTION", "shadow")
        .env("LS_CALENDAR_SNAPSHOT", &snap)
        .output()
        .expect("budget-probe runs");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "a non-paper invocation refuses: {stderr}");
    assert_eq!(
        stderr.matches("calendar-startup").count(),
        1,
        "the startup record still fires exactly once on the non-paper path: {stderr}"
    );
    assert!(stderr.contains("this probe is paper-only"), "the paper gate is the refusal: {stderr}");
}

/// AE1 (scenario 1, usability half): the calendar loaded at the composition root remains a
/// usable immutable in-memory value AFTER the source snapshot file is removed — the same
/// `resolve_and_load` path the binary uses, proven to hold no file handle post-load.
#[test]
fn loaded_calendar_stays_usable_after_source_file_removed() {
    let dir = tempfile::tempdir().unwrap();
    let snap = temp_snapshot(dir.path());

    // A fixed as-of instant within the fixture's authorization grant (KTD5 explicit instant,
    // not a wall-clock fixture).
    let as_of = Utc.with_ymd_and_hms(2013, 6, 1, 0, 0, 0).unwrap();
    let loaded = resolve_and_load(Some(&snap), as_of, CalendarAdoption::Enforced);
    let cal = loaded.calendar().expect("a valid synthetic snapshot injects");

    // Remove the source file — the load already read it fully into memory.
    std::fs::remove_file(&snap).unwrap();
    assert!(!snap.exists(), "the source snapshot is gone");

    // The calendar still answers the identical proven facts (no reload, no file handle).
    let view = cal.as_of(as_of).expect("authorized at the as-of instant");
    let fact = view
        .day(chrono::NaiveDate::from_ymd_opt(2010, 6, 15).unwrap())
        .expect("an in-window date resolves");
    assert_eq!(fact.status, DayStatus::TradingSession, "a proven session still resolves");
}
