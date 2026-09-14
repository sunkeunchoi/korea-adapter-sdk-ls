//! U9 — the rehearsal mount path, the bar-delivery data seam, and the closing-auction
//! session.
//!
//! Three altitudes, deliberately:
//!
//! 1. **The bin** (`CARGO_BIN_EXE_lab-live --rehearse-daily`) — the entrypoint is reachable
//!    and every pre-build refusal carries its own exit code. This is the only altitude that
//!    proves the verb exists; a library test of `run_rehearsal` would pass with the argv arm
//!    never wired.
//! 2. **The gates** — the envelope, the trip ledger, the cutoff, and the account probe, each
//!    refusing for its own reason with no node built.
//! 3. **The day** — a whole session over a stub t8407 and a stub account: the sweep, the
//!    15:20 decision, the synthetic bar reaching the data event sender exactly once per
//!    symbol, the held-symbol gap, the divergence rows, and the book the next session
//!    inherits.
//!
//! `node.run` is never driven here — the repo's documented invariant. What is proven instead
//! is every seam around it, which is why the day loop is a value the test can drive directly.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{NaiveDate, TimeZone, Utc};
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::orders::ledger::FillLedger;
use nautilus_ls_lab::artifacts::data_quality::RehearsalDivergenceKind;
use nautilus_ls_lab::runner::backtest_daily::{DailySessionSignals, OpenPositionBook};
use nautilus_ls_lab::runner::live::rehearsal::{
    preflight_offline, probe_book, standing_trip, RehearsalEnvelope, RehearsalInputs,
};
use nautilus_ls_lab::runner::live::shared::SessionObservations;
use nautilus_ls_lab::runner::live_daily::{
    daily_bar_type, instrument_id_for, session_calendar, session_ordinal, start_posture, DayLoop,
    RehearsalBook,
    RehearsalBookLeg, BOOK_VERSION, SESSION_ORDINAL_EPOCH,
};
use nautilus_ls_lab::runner::mount_universe::DailyUniverseRow;
use nautilus_ls_lab::runner::rehearsal_data::DataEventSink;
use nautilus_ls_lab::runner::watchdog::{Heartbeats, RehearsalLedger, TripSink};
use nautilus_ls_lab::strategy::hooks::MarkFeed;
use nautilus_model::identifiers::InstrumentId;
use tempfile::{tempdir, TempDir};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const MARKET_DATA_PATH: &str = "/stock/market-data";
const ACCNO_PATH: &str = "/stock/accno";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A weekday KST instant inside the rehearsal window: 2026-09-11 (a Friday) at 15:00 KST.
const MOUNT_UNIX: i64 = 1_789_106_400;
/// The session the fixtures run for, and the proven session before it.
const SESSION_DATE: &str = "2026-09-11";
const PREVIOUS_SESSION: &str = "2026-09-10";

fn ok_json(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_string(body.to_string())
        .insert_header("content-type", "application/json")
}

fn envelope_json() -> serde_json::Value {
    serde_json::json!({
        "version": 1,
        "heartbeat_interval_secs": 90,
        "session_max_loss_krw": 3_000_000.0,
        "mount_cutoff_kst": "15:15",
        "poll_interval_secs": 20,
        "decision_kst": "15:20",
        "auction_end_kst": "15:30",
        "session_end_kst": "15:33",
        "stop_grace_secs": 60,
        "watchdog_tick_secs": 5,
        "decision_read_attempts": 3,
        "marketable_limit_ticks": 3,
        "min_deposit_krw": 100_000_000,
        "starting_balance_krw": 100_000_000.0
    })
}

fn write_envelope(home: &Path, mutate: impl FnOnce(&mut serde_json::Value)) -> std::path::PathBuf {
    let mut v = envelope_json();
    mutate(&mut v);
    let p = home.join("rehearsal-envelope.json");
    std::fs::write(&p, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    p
}

fn inputs_for(home: &TempDir) -> RehearsalInputs {
    let envelope_path = write_envelope(home.path(), |_| {});
    let keepalive = home.path().join("keepalive");
    std::fs::write(&keepalive, "alive").unwrap();
    let lane_env = home.path().join("lane.env");
    std::fs::write(&lane_env, "APPKEY=x\n").unwrap();
    RehearsalInputs {
        data_home: home.path().to_path_buf(),
        envelope_path,
        keepalive_path: keepalive,
        lane_env_path: lane_env,
        universe_path: home.path().join("universe.json"),
        stop_before_orders: false,
    }
}

fn leg(shcode: &str, qty: i64) -> RehearsalBookLeg {
    RehearsalBookLeg {
        shcode: shcode.to_string(),
        quantity: qty,
        entry_price: 60_000.0,
        stop_price: 57_000.0,
        prior_close: 61_000.0 as i64,
        entry_date: "2026-08-14".to_string(),
        entered_under: "20260910T060000Z-live-daily-ms-v1".to_string(),
        opening_order_id: "O-005930-1".to_string(),
    }
}

fn book_with(legs: Vec<RehearsalBookLeg>, stamp: &str) -> RehearsalBook {
    RehearsalBook {
        version: BOOK_VERSION,
        session_date: stamp.to_string(),
        run_id: "20260910T060000Z-live-daily-ms-v1".to_string(),
        ordinal_epoch: SESSION_ORDINAL_EPOCH.to_string(),
        legs,
    }
}

fn universe_rows(codes: &[&str]) -> Vec<DailyUniverseRow> {
    codes
        .iter()
        .enumerate()
        .map(|(i, c)| DailyUniverseRow {
            shcode: (*c).to_string(),
            prior_close: 60_000,
            prior_atr1: 1_200.0,
            signal_value: Some(1.0 - i as f64 / 100.0),
            rank: i,
            tradable: true,
        })
        .collect()
}

/// A t8407 stub that quotes every code it is asked about at `price`.
async fn mount_t8407(server: &MockServer, rows: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path(MARKET_DATA_PATH))
        .and(header("tr_cd", "t8407"))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "00000",
            "t8407OutBlock": {},
            "t8407OutBlock1": rows
        })))
        .mount(server)
        .await;
}

fn quote_row(shcode: &str, price: i64) -> serde_json::Value {
    serde_json::json!({
        "shcode": shcode, "hname": "테스트", "price": price.to_string(),
        "sign": "2", "change": "0", "diff": "0.00", "volume": "1000",
        "open": (price - 500).to_string(),
        "high": (price + 700).to_string(),
        "low": (price - 900).to_string()
    })
}

/// A t8407 stub that THROTTLES: every request fails with the gateway's rate-limit code.
async fn mount_t8407_throttled(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path(MARKET_DATA_PATH))
        .and(header("tr_cd", "t8407"))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "IGW00201", "rsp_msg": "초당 거래건수를 초과하였습니다."
        })))
        .mount(server)
        .await;
}

async fn mount_t0424(server: &MockServer, rows: serde_json::Value, sunamt1: &str) {
    Mock::given(method("POST"))
        .and(path(ACCNO_PATH))
        .and(header("tr_cd", "t0424"))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "00000",
            "t0424OutBlock": { "sunamt1": sunamt1, "cts_expcode": "" },
            "t0424OutBlock1": rows
        })))
        .mount(server)
        .await;
}

async fn mount_cancel_ok(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/stock/order"))
        .and(header("tr_cd", "CSPAT00801"))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "00463", "rsp_msg": "OK",
            "CSPAT00801OutBlock1": {}, "CSPAT00801OutBlock2": { "OrdNo": "9001" }
        })))
        .mount(server)
        .await;
}

async fn mount_t0425_empty(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path(ACCNO_PATH))
        .and(header("tr_cd", "t0425"))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "00000",
            "t0425OutBlock": { "tqty": "0", "tcheqty": "0", "tordrem": "0", "cts_ordno": "" },
            "t0425OutBlock1": []
        })))
        .mount(server)
        .await;
}

fn holding_row(shcode: &str, qty: i64) -> serde_json::Value {
    serde_json::json!({
        "expcode": shcode, "janqty": qty.to_string(), "pamt": "60000",
        "hname": "테스트", "mdposqt": "0", "jangb": "01"
    })
}

fn id(shcode: &str) -> InstrumentId {
    instrument_id_for(shcode).unwrap()
}

fn day(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

/// The operator's calendar snapshot, when one is configured.
///
/// Returns `None` rather than failing when it is absent, but note what that costs: the test
/// that uses it is the one that catches an unsatisfiable forward gate, so a CI run without a
/// snapshot does not cover the defect it exists for. That is recorded as a known gap rather
/// than papered over with a synthetic calendar, because a hand-built fixture is exactly what
/// let the original defect through -- it would contain whatever future rows the author put in
/// it, including proven ones a real calendar can never have.
fn support_calendar() -> Option<nautilus_ls_calendar::KrxCalendar> {
    let raw = std::env::var_os("LS_CALENDAR_SNAPSHOT")?;
    // A test runs with its PACKAGE as the working directory, not the adapter root, so the
    // operator's habitual relative `state/krx.calendar.json` does not resolve here. Anchor a
    // relative value to the adapter root; an absolute one is used as given.
    let candidate = std::path::PathBuf::from(&raw);
    let path = if candidate.is_absolute() {
        candidate
    } else {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("the lab package sits under the adapter root")
            .join(candidate)
    };
    // Absent variable: skip. Present but unloadable: PANIC. Silently returning `None` for a
    // snapshot the operator did configure is how a test that no longer runs still reports
    // green -- the same skip-quietly shape that let the forward-gate defect ship.
    Some(
        nautilus_ls_calendar::KrxCalendar::load_from_path(
            &path,
            Utc.timestamp_opt(MOUNT_UNIX, 0).unwrap(),
        )
        .unwrap_or_else(|e| {
            panic!(
                "LS_CALENDAR_SNAPSHOT resolved to {} but the calendar will not load: {e:?}",
                path.display()
            )
        }),
    )
}

// ---------------------------------------------------------------------------
// 1. The bin: the entrypoint exists and every pre-build refusal has its own code
// ---------------------------------------------------------------------------

fn bin_rehearse(home: &Path, extra: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_lab-live"));
    cmd.arg("--rehearse-daily")
        .env("LS_DATA_HOME", home)
        .env("LS_TRADING_ENV", "paper")
        .env("LS_DISPATCH_NOW_UNIX", MOUNT_UNIX.to_string())
        // The injected clock is a deliberately-armed test seam on this lane, not an ambient
        // convention -- see `run_rehearsal`.
        .env("LS_REHEARSAL_STUB_CLOCK", "1")
        .env_remove("LS_DISPATCH_NONCE")
        .env_remove("LS_REHEARSAL_ENVELOPE")
        .env_remove("LS_REHEARSAL_UNIVERSE_FILE")
        .env_remove("LS_MOUNT_KEEPALIVE")
        .env_remove("LS_CALENDAR_SNAPSHOT")
        .env_remove("LS_CALENDAR_ADOPTION");
    for (k, v) in extra {
        cmd.env(k, v);
    }
    cmd.output().unwrap()
}

/// The paper interlock is FIRST — before any file is read, any credential resolved, or any
/// gate evaluated. A live-lane rehearsal is the one refusal that must not depend on the rest
/// of the configuration being right.
#[test]
fn bin_a_non_paper_environment_refuses_with_the_paper_interlock_code() {
    let home = tempdir().unwrap();
    let out = bin_rehearse(home.path(), &[("LS_TRADING_ENV", "live")]);
    assert_eq!(out.status.code(), Some(66), "the paper interlock is exit 66");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("LS_TRADING_ENV must be `paper`"), "{stderr}");
    assert!(
        !home.path().join("rehearsal").exists(),
        "nothing is created on the interlock path"
    );
}

/// A rehearsal places real paper orders against an account that carries an inherited book,
/// so it is attended. In a no-TTY shell with no nonce the refusal is LOUD and distinct —
/// never a silent success a scripted caller could mistake for a completed session.
#[test]
fn bin_an_unattended_invocation_refuses_with_the_attendance_code() {
    let home = tempdir().unwrap();
    let out = bin_rehearse(home.path(), &[]);
    assert_eq!(out.status.code(), Some(77), "no nonce in a no-TTY shell is exit 77");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("rehearsal refused"), "{stderr}");
}

/// Past the cutoff the sweep has too little time left to establish a mark for every mounted
/// symbol — and the marketable-limit policy REFUSES to price an order without one, so a late
/// mount would hold a live node open against an inherited book and trade nothing.
#[test]
fn bin_a_mount_after_the_cutoff_refuses_pre_build() {
    let home = tempdir().unwrap();
    let inputs = inputs_for(&home);
    // 15:16 KST — one minute past the envelope's 15:15 cutoff.
    let late = MOUNT_UNIX + 16 * 60;
    let out = bin_rehearse(
        home.path(),
        &[
            ("LS_DISPATCH_NONCE", "test-nonce"),
            ("LS_DISPATCH_NOW_UNIX", &late.to_string()),
            ("LS_REHEARSAL_ENVELOPE", inputs.envelope_path.to_str().unwrap()),
            ("LS_MOUNT_KEEPALIVE", inputs.keepalive_path.to_str().unwrap()),
            ("LS_REHEARSAL_UNIVERSE_FILE", inputs.universe_path.to_str().unwrap()),
            ("LS_DISPATCH_LANE_ENV", inputs.lane_env_path.to_str().unwrap()),
            ("LS_CI_ATTENDED", "1"),
        ],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Either the attendance gate (no TTY) or the cutoff refuses; both are pre-build and
    // neither builds a node. What must NEVER happen is a zero exit.
    assert_ne!(out.status.code(), Some(0), "a refused mount never exits 0: {stderr}");
    assert!(
        !home.path().join("dispatch").exists(),
        "a rehearsal never manufactures the ladder's dispatch store: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// 2. The gates
// ---------------------------------------------------------------------------

/// The envelope is fail-closed on every axis a half-configured session could run under.
#[test]
fn the_envelope_refuses_an_out_of_order_clock_and_an_over_long_stop_grace() {
    let home = tempdir().unwrap();

    let p = write_envelope(home.path(), |v| v["decision_kst"] = "15:35".into());
    let err = RehearsalEnvelope::load(&p).unwrap_err().to_string();
    assert!(err.contains("session clock must run"), "{err}");

    let p = write_envelope(home.path(), |v| v["stop_grace_secs"] = 120.into());
    let err = RehearsalEnvelope::load(&p).unwrap_err().to_string();
    assert!(err.contains("exceeds heartbeat_interval_secs"), "{err}");

    let p = write_envelope(home.path(), |v| v["version"] = 2.into());
    let err = RehearsalEnvelope::load(&p).unwrap_err().to_string();
    assert!(err.contains("not understood"), "{err}");

    let p = write_envelope(home.path(), |v| v["decision_read_attempts"] = 0.into());
    let err = RehearsalEnvelope::load(&p).unwrap_err().to_string();
    assert!(err.contains("at least 1"), "{err}");

    // And the shipped file itself loads — a committed envelope that cannot be read would
    // make the verb unreachable in production while every unit test passed.
    let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("config/rehearsal-envelope.json");
    let env = RehearsalEnvelope::load(&shipped).expect("the shipped envelope is valid");
    assert_eq!(env.version, 1);
    assert!(env.mount_cutoff().unwrap() < env.decision().unwrap());
}

/// U13's gate, from U9's side: the LAST row per mechanism decides. A cleared history is a
/// healthy home — a gate on "has ever tripped" would make the ledger unusable after its
/// first entry.
#[test]
fn the_trip_gate_reads_the_last_row_per_mechanism() {
    use nautilus_ls_lab::dispatch::chain::{SafetyTripKind, TripAction};
    let home = tempdir().unwrap();
    let ledger = RehearsalLedger::new(home.path());
    let at = Utc.timestamp_opt(MOUNT_UNIX, 0).unwrap();

    assert!(standing_trip(&ledger).unwrap().is_none(), "a home that never tripped is clear");

    ledger
        .record_trip(SafetyTripKind::Breaker, TripAction::Engage, Some("r1"), "breaker", at)
        .unwrap();
    let standing = standing_trip(&ledger).unwrap().expect("the engage stands");
    assert_eq!(standing.trip, SafetyTripKind::Breaker);

    ledger
        .record_trip(SafetyTripKind::Breaker, TripAction::Clear, Some("r1"), "cleared", at)
        .unwrap();
    assert!(standing_trip(&ledger).unwrap().is_none(), "the clear releases it");

    // Per MECHANISM: a dead-man engage is NOT released by the breaker's clear above it.
    ledger
        .record_trip(SafetyTripKind::Watchdog, TripAction::Engage, Some("r2"), "dead man", at)
        .unwrap();
    ledger
        .record_trip(SafetyTripKind::Breaker, TripAction::Clear, Some("r2"), "unrelated", at)
        .unwrap();
    let standing = standing_trip(&ledger).unwrap().expect("the dead-man engage still stands");
    assert_eq!(standing.trip, SafetyTripKind::Watchdog);
}

/// A standing trip refuses the mount before any credential is resolved: the account the
/// previous session left is exactly the one this session would inherit.
#[test]
fn a_standing_trip_refuses_the_mount_before_any_gateway_call() {
    use nautilus_ls_lab::dispatch::chain::{SafetyTripKind, TripAction};
    let home = tempdir().unwrap();
    let inputs = inputs_for(&home);
    RehearsalLedger::new(home.path())
        .record_trip(
            SafetyTripKind::Breaker,
            TripAction::Engage,
            Some("r1"),
            "the breaker fired",
            Utc.timestamp_opt(MOUNT_UNIX, 0).unwrap(),
        )
        .unwrap();
    let err = preflight_offline(&inputs, MOUNT_UNIX).unwrap_err().to_string();
    assert!(err.contains("standing"), "{err}");
    assert!(err.contains("--rehearsal-clear-trip"), "it names the rehearsal's verb: {err}");
    // The ladder verb appears only to be DISCLAIMED — never offered as the fix. Following it
    // would leave the operator running a nonce-gated command against a store this home does
    // not have while the real blocker still refuses the next mount.
    assert!(
        !err.contains("clear it with `lab-live --clear-killswitch`"),
        "the ladder verb is not offered as the remedy: {err}"
    );
    assert!(err.contains("does NOT apply"), "and is explicitly disclaimed: {err}");
}

/// An absent keepalive file would trip the operator dead-man on the first watchdog tick, so
/// it refuses up front rather than after the node exists.
#[test]
fn an_absent_keepalive_refuses_pre_build() {
    let home = tempdir().unwrap();
    let mut inputs = inputs_for(&home);
    inputs.keepalive_path = home.path().join("does-not-exist");
    let err = preflight_offline(&inputs, MOUNT_UNIX).unwrap_err().to_string();
    assert!(err.contains("keepalive"), "{err}");
}

/// The book's own fields refuse first — they cost no gateway call, and a leg with no usable
/// stop is unrunnable whatever the broker says.
#[test]
fn a_book_leg_without_a_usable_stop_refuses() {
    let mut bad = leg("005930", 10);
    bad.stop_price = 0.0;
    let err = book_with(vec![bad], PREVIOUS_SESSION).validate_fields().unwrap_err().to_string();
    assert!(err.contains("stop_price"), "{err}");

    let mut inverted = leg("005930", 10);
    inverted.stop_price = 70_000.0;
    let err =
        book_with(vec![inverted], PREVIOUS_SESSION).validate_fields().unwrap_err().to_string();
    assert!(err.contains("not below entry_price"), "{err}");

    let mut anonymous = leg("005930", 10);
    anonymous.entered_under = "  ".to_string();
    let err =
        book_with(vec![anonymous], PREVIOUS_SESSION).validate_fields().unwrap_err().to_string();
    assert!(err.contains("entered_under"), "{err}");

    // The healthy book passes, and its projections are consistent with each other.
    let good = book_with(vec![leg("005930", 10), leg("000660", 5)], PREVIOUS_SESSION);
    good.validate_fields().unwrap();
    assert_eq!(good.expected_book().len(), 2);
    assert_eq!(good.pnl_legs().len(), 2);
    let sessions = vec![day("2026-08-14"), day(PREVIOUS_SESSION), day(SESSION_DATE)];
    let legs = good.strategy_legs(&sessions).unwrap();
    assert_eq!(legs.len(), 2);
    assert_eq!(legs[0].entry_session_ordinal, 0, "the entry DATE resolved against this list");
    // A leg whose entry day is not a session in THIS mount's list is refused, never clamped:
    // snapping it to a neighbour would change a live position's hold length.
    let err = good.strategy_legs(&[day(SESSION_DATE)]).unwrap_err().to_string();
    assert!(err.contains("hold length cannot be measured"), "{err}");
    assert_eq!(start_posture(&good), nautilus_ls::execution::StartPosture::BookAsserted);
    assert_eq!(
        start_posture(&book_with(Vec::new(), PREVIOUS_SESSION)),
        nautilus_ls::execution::StartPosture::Flat,
        "a home with nothing to inherit keeps the flat-start assertion"
    );
}

/// Freshness is measured on the PROVEN calendar, not on "yesterday": a Monday mount inherits
/// Friday's book. An older stamp means a session's teardown never wrote, so the legs and
/// ordinals predate the account's current state.
#[test]
fn the_book_stamp_must_be_the_previous_proven_session_including_across_a_weekend() {
    let friday = day("2026-09-11");
    let monday_book = book_with(vec![leg("005930", 10)], "2026-09-11");
    monday_book
        .assert_fresh(friday)
        .expect("a Monday mount accepts Friday's book");

    let stale = book_with(vec![leg("005930", 10)], "2026-09-09");
    let err = stale.assert_fresh(friday).unwrap_err().to_string();
    assert!(err.contains("previous proven trading session"), "{err}");
    assert!(err.contains("Repair the book against the account"), "{err}");

    // A stamp NEWER than the last proven session is healthy, not stale: the KRX witness is
    // retrospective, so yesterday's session routinely reads Unknown this morning and a book
    // written by yesterday's own teardown is ahead of what the calendar can prove.
    book_with(vec![leg("005930", 10)], "2026-09-14")
        .assert_fresh(friday)
        .expect("a book ahead of the proven calendar is a lagging witness, not a stale book");

    // A bootstrapped home has no previous session to be stale against.
    RehearsalBook::empty("").assert_fresh(friday).expect("an empty book is exempt");
}

/// A book at another schema version, or counting ordinals from another epoch, refuses rather
/// than silently re-dating every open hold.
#[test]
fn a_book_from_another_schema_or_epoch_refuses_to_restore() {
    let home = tempdir().unwrap();
    std::fs::create_dir_all(home.path().join("rehearsal")).unwrap();

    let mut v = serde_json::to_value(book_with(vec![leg("005930", 10)], PREVIOUS_SESSION)).unwrap();
    v["version"] = 99.into();
    std::fs::write(RehearsalBook::path(home.path()), v.to_string()).unwrap();
    let err = RehearsalBook::load(home.path()).unwrap_err().to_string();
    assert!(err.contains("version"), "{err}");

    let mut v = serde_json::to_value(book_with(vec![leg("005930", 10)], PREVIOUS_SESSION)).unwrap();
    v["ordinal_epoch"] = "2016-08-01".into();
    std::fs::write(RehearsalBook::path(home.path()), v.to_string()).unwrap();
    let err = RehearsalBook::load(home.path()).unwrap_err().to_string();
    assert!(err.contains("session ordinals"), "{err}");

    // An ABSENT file is a flat start, which a bootstrapped home legitimately has.
    std::fs::remove_file(RehearsalBook::path(home.path())).unwrap();
    assert!(RehearsalBook::load(home.path()).unwrap().legs.is_empty());
}

/// AE3: a t0424 that disagrees with the book refuses, with no node built and no order sent.
#[tokio::test]
async fn a_mismatched_account_refuses_the_probe_and_names_the_symbols() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    mount_t0425_empty(&server).await;
    // The book expects 10 of 005930; the account reports 7.
    mount_t0424(&server, serde_json::json!([holding_row("005930", 7)]), "200000000").await;
    let sdk = LsSdk::new(mock_config(&server.uri())).unwrap();

    let book = book_with(vec![leg("005930", 10)], PREVIOUS_SESSION);
    let err = probe_book(&sdk, book, 100_000_000).await.unwrap_err().to_string();
    assert!(err.contains("pre-mount book probe refused"), "{err}");
    assert!(err.contains("005930"), "it names the offending symbol: {err}");
    assert!(err.contains("No node was built"), "{err}");
}

/// The deposit preflight refuses a session that would size entries against cash the account
/// does not have. It is read LAST, and only after the book matched.
#[tokio::test]
async fn a_deposit_below_the_envelope_floor_refuses_the_probe() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    mount_t0425_empty(&server).await;
    mount_t0424(&server, serde_json::json!([holding_row("005930", 10)]), "50000000").await;
    let sdk = LsSdk::new(mock_config(&server.uri())).unwrap();

    let book = book_with(vec![leg("005930", 10)], PREVIOUS_SESSION);
    let err = probe_book(&sdk, book, 100_000_000).await.unwrap_err().to_string();
    assert!(err.contains("deposit preflight refused"), "{err}");
    assert!(err.contains("50000000"), "it names what it read: {err}");
}

/// The matching case: the probe confirms, hands back the expectation the session mounts
/// `BookAsserted` on, and reads the deposit off the SAME holdings inquiry.
#[tokio::test]
async fn a_matching_account_passes_the_probe_and_carries_the_expectation_forward() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    mount_t0425_empty(&server).await;
    mount_t0424(
        &server,
        serde_json::json!([holding_row("005930", 10), holding_row("000660", 5)]),
        "150000000",
    )
    .await;
    let sdk = LsSdk::new(mock_config(&server.uri())).unwrap();

    let book = book_with(vec![leg("005930", 10), leg("000660", 5)], PREVIOUS_SESSION);
    let probe = probe_book(&sdk, book, 100_000_000).await.expect("the account matches");
    assert_eq!(probe.deposit_krw, 150_000_000);
    assert_eq!(probe.expected.len(), 2);
    assert_eq!(probe.book.legs.len(), 2);
}

// ---------------------------------------------------------------------------
// 3. The day
// ---------------------------------------------------------------------------

struct DayRig {
    home: TempDir,
    signals: DailySessionSignals,
    held: OpenPositionBook,
    marks: MarkFeed,
    observations: SessionObservations,
    ledger: Arc<Mutex<FillLedger>>,
    session: nautilus_ls_lab::runner::live::LiveTeardownSession,
    rx: tokio::sync::mpsc::UnboundedReceiver<nautilus_common::messages::DataEvent>,
}

/// A day loop wired to a captured sender, so the bars it emits are observable without a
/// node — the seam's contract, proven at the seam.
fn day_rig(server_uri: &str, held_codes: &[&str]) -> (DayLoop, DayRig) {
    let home = tempdir().unwrap();
    let sdk = LsSdk::new(mock_config(server_uri)).unwrap();
    let signals = DailySessionSignals::new();
    let held = OpenPositionBook::new();
    for c in held_codes {
        held.seed_held(id(c));
    }
    let marks = MarkFeed::new();
    let bars = DataEventSink::new();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    bars.capture(tx);
    let observations = SessionObservations::new();
    let ledger: Arc<Mutex<FillLedger>> = Arc::new(Mutex::new(FillLedger::new()));
    // The loop reads this only to notice it has been halted; the rig's SDK is the mock
    // gateway, so `orders_enabled()` is true until something engages the kill switch.
    let session = nautilus_ls_lab::runner::live::LiveTeardownSession::new(
        nautilus_ls_lab::strategy::hooks::EmissionGate::open(),
        LsSdk::new(mock_config(server_uri)).unwrap(),
        Arc::clone(&ledger),
        nautilus_ls::execution::OrderDispatchTasks::new(),
    );

    let session_date = day(SESSION_DATE);
    // A compressed clock: the test advances through the whole session in milliseconds. The
    // loop reads the clock, never the wall, which is exactly why it can.
    let tick = Arc::new(std::sync::atomic::AtomicI64::new(MOUNT_UNIX));
    let clock_tick = Arc::clone(&tick);
    let loop_cfg = DayLoop {
        sdk,
        clock: Arc::new(move || {
            clock_tick.fetch_add(120, std::sync::atomic::Ordering::SeqCst)
        }),
        poll_interval: Duration::from_millis(1),
        decision_unix: MOUNT_UNIX + 20 * 60,
        auction_end_unix: MOUNT_UNIX + 30 * 60,
        session_end_unix: MOUNT_UNIX + 33 * 60,
        decision_attempts: 3,
        session_date,
        session_index: 4_010,
        universe: universe_rows(&["005930", "000660", "035420", "051910"]),
        target_m: 2,
        signals: signals.clone(),
        held_book: held.clone(),
        marks: marks.clone(),
        bars: bars.clone(),
        heartbeats: Heartbeats::new(MOUNT_UNIX),
        observations: observations.clone(),
        data_home: home.path().to_path_buf(),
        lane_hash: "cafef00d".to_string(),
        stop_before_orders: false,
        ledger: Arc::clone(&ledger),
        session: session.clone(),
        stop_atr_mult: 1.5,
        run_id: "20260911T060000Z-live-daily-ms-v1".to_string(),
        // Today at `session_index`, and sixteen sessions of headroom past it — the shape
        // `hold_window_end` needs to measure a prospective hold (R22).
        session_calendar: (0..4_030)
            .map(|i| session_date + chrono::Duration::days(i as i64 - 4_010))
            .collect(),
    };
    (loop_cfg, DayRig { home, signals, held, marks, observations, rx, ledger, session })
}

fn bars_received(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<nautilus_common::messages::DataEvent>,
) -> Vec<nautilus_model::data::Bar> {
    let mut out = Vec::new();
    while let Ok(evt) = rx.try_recv() {
        if let nautilus_common::messages::DataEvent::Data(nautilus_model::data::Data::Bar(b)) = evt
        {
            out.push(b);
        }
    }
    out
}

/// The ordinary day: the sweep publishes marks, the 15:20 read becomes ONE synthetic 1-DAY
/// bar per mounted symbol on the data event path, the take is the top `target_m` of the
/// unheld ranked names, and the decision-vs-close divergence is measured on every one.
#[tokio::test]
async fn a_whole_day_delivers_one_bar_per_symbol_and_measures_the_close_divergence() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    mount_t8407(
        &server,
        serde_json::json!([
            quote_row("005930", 61_000),
            quote_row("000660", 62_000),
            quote_row("035420", 63_000),
            quote_row("051910", 64_000),
        ]),
    )
    .await;

    let (loop_cfg, mut rig) = day_rig(&server.uri(), &["005930"]);
    // The closing auction fills ONE of the two entries the take makes. Seeding it before
    // the loop reaches its post-auction read is what lets the assertion below distinguish
    // "this entry filled" from "the ledger was empty" — an all-unfilled fixture would pass
    // whether or not the code looked at the ledger at all.
    rig.ledger.lock().unwrap().seed_fill(nautilus_ls::orders::ledger::LedgerFill {
        symbol: "000660".to_string(),
        side: nautilus_model::enums::OrderSide::Buy,
        qty: 12,
        price: 62_000,
        price_approximated: false,
        trade_id: nautilus_model::identifiers::TradeId::new("FILL-000660"),
        // Stamped in the closing auction. A seeded inherited leg is stamped at the PRIOR
        // session and must not read as an entry this session made, so the recovery filters
        // on this — a fill stamped at the epoch would be excluded, correctly.
        observed_ns: (MOUNT_UNIX as u64 + 30 * 60) * 1_000_000_000,
    });
    let outcome = loop_cfg.run().await.expect("the day completes");

    assert!(outcome.decided, "a decision was taken");
    // The take excludes the already-held name, so the top 2 UNHELD ranked names are taken.
    assert_eq!(
        outcome.taken,
        vec![id("000660"), id("035420")],
        "the take is the top target_m of the ranked list, already-held excluded"
    );

    // Exactly one bar per symbol in `held ∪ taken` — the bar-delivery contract.
    let delivered = bars_received(&mut rig.rx);
    let mut symbols: Vec<String> = delivered
        .iter()
        .map(|b| b.bar_type.instrument_id().to_string())
        .collect();
    symbols.sort();
    assert_eq!(
        symbols,
        vec!["000660.XKRX", "005930.XKRX", "035420.XKRX"],
        "held ∪ taken, and nothing else"
    );
    assert_eq!(delivered.len(), 3, "exactly once per symbol, never twice");
    assert_eq!(
        delivered[0].bar_type,
        daily_bar_type(delivered[0].bar_type.instrument_id()).unwrap(),
        "the bar carries the 1-DAY type the strategy subscribed"
    );

    // The 15:20 read is the day's continuous-session bar (KTD4): its close IS the decision
    // price, and its open/high/low come from the same row.
    let samsung = delivered
        .iter()
        .find(|b| b.bar_type.instrument_id() == id("005930"))
        .unwrap();
    assert_eq!(samsung.close.as_f64(), 61_000.0);
    assert_eq!(samsung.open.as_f64(), 60_500.0);
    assert_eq!(samsung.high.as_f64(), 61_700.0);
    assert_eq!(samsung.low.as_f64(), 60_100.0);

    // The session context reached the strategy BEFORE any bar did.
    let ctx = rig.signals.current().expect("the session context was published");
    assert_eq!(ctx.index, 4_010);
    assert_eq!(ctx.date, day(SESSION_DATE));
    assert_eq!(ctx.taken, outcome.taken);
    assert_eq!(ctx.held, vec![id("005930")]);
    assert_eq!(ctx.prior_atr.get(&id("005930")), Some(&Some(1_200.0)));

    // The marks the marketable-limit policy prices from were published by the sweep.
    assert_eq!(rig.marks.get("005930").map(|m| m.last_close), Some(61_000));

    // KTD4's divergence class, recorded for EVERY decided symbol — including the ones that
    // did not move, or the class cannot be summarized.
    let (gaps, divergences, _) = rig.observations.snapshot();
    assert!(gaps.is_empty(), "nothing was halted: {gaps:?}");
    let close_rows: Vec<_> = divergences
        .iter()
        .filter(|d| d.kind == RehearsalDivergenceKind::DecisionVsClose)
        .collect();
    assert_eq!(close_rows.len(), 3, "one per decided symbol: {divergences:?}");
    assert!(close_rows
        .iter()
        .all(|d| d.decision_price.is_some() && d.realized_price.is_some()));

    // AE6: the entry the auction did not fill is the divergence with no backtest
    // counterpart at all, and the one that filled must NOT be reported as unfilled.
    let unfilled: Vec<&str> = divergences
        .iter()
        .filter(|d| d.kind == RehearsalDivergenceKind::UnfilledEntry)
        .map(|d| d.instrument_id.as_str())
        .collect();
    assert_eq!(
        unfilled,
        vec!["035420.XKRX"],
        "only the entry the auction left unfilled: {divergences:?}"
    );

    // A rehearsal never manufactures the ladder's authorization store.
    assert!(!rig.home.path().join("dispatch").exists());
}

/// AE7/KTD12: a held symbol the gateway does not quote gets a TYPED gap row and KEEPS its
/// holding — the backtest's policy for the same condition is to abort, and the comparison
/// report has to be able to count that divergence rather than grep for it.
#[tokio::test]
async fn a_held_symbol_with_no_quote_writes_a_gap_row_and_keeps_the_holding() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    // 005930 is held but NOT quoted; the others are.
    mount_t8407(
        &server,
        serde_json::json!([quote_row("000660", 62_000), quote_row("035420", 63_000)]),
    )
    .await;

    let (loop_cfg, mut rig) = day_rig(&server.uri(), &["005930"]);
    let outcome = loop_cfg.run().await.expect("the session ends normally");

    let (gaps, _, _) = rig.observations.snapshot();
    assert_eq!(gaps.len(), 1, "one gap row: {gaps:?}");
    assert_eq!(gaps[0].instrument_id, "005930.XKRX");
    assert_eq!(gaps[0].session_date, SESSION_DATE);
    assert!(gaps[0].reason.contains("KEPT"), "{:?}", gaps[0]);

    assert!(
        rig.held.is_held(&id("005930")),
        "the holding is KEPT — a halt is not evidence to exit"
    );
    let delivered = bars_received(&mut rig.rx);
    assert!(
        !delivered.iter().any(|b| b.bar_type.instrument_id() == id("005930")),
        "and no bar was fabricated for it"
    );
    assert!(outcome.decided, "the session still decided on the symbols it could read");
}

/// AE9/R33: a throttled decision read records "no decision" and ends the session NORMALLY.
/// It is not a trip: nothing unsafe happened, the account holds what it held, and the next
/// valid bar decides. The loop keeps feeding the dead-man throughout, so the retry budget
/// cannot itself cause a trip.
#[tokio::test]
async fn a_throttled_decision_read_records_no_decision_without_tripping() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    mount_t8407_throttled(&server).await;

    let (loop_cfg, mut rig) = day_rig(&server.uri(), &["005930"]);
    let heartbeats = loop_cfg.heartbeats.clone();
    let outcome = loop_cfg.run().await.expect("a throttle is not an error");

    assert!(!outcome.decided, "no decision was taken");
    assert_eq!(outcome.bars_delivered, 0);
    assert!(bars_received(&mut rig.rx).is_empty(), "no bar was fabricated");

    let (_, _, notes) = rig.observations.snapshot();
    assert!(
        notes.iter().any(|n| n.contains("no decision")),
        "the outcome is RECORDED, not silent: {notes:?}"
    );
    assert!(
        notes.iter().any(|n| n.contains("holdings are unchanged")),
        "and says what it means for the account: {notes:?}"
    );

    // The dead-man was fed on every tick, including through the retries — otherwise a
    // throttle would masquerade as a stalled runtime.
    assert!(
        heartbeats.runtime_unix() >= MOUNT_UNIX,
        "the loop touched the runtime feeder"
    );
    assert!(
        rig.held.is_held(&id("005930")),
        "the account still holds what it held"
    );
}

/// `--stop-before-orders`: the decision is resolved and RECORDED, and not one bar reaches
/// the node — so the strategy emits nothing. This is how the 15:20-to-15:30 meaning of
/// t8407's `price` is observed without trading on it.
#[tokio::test]
async fn stop_before_orders_records_the_decision_and_delivers_no_bar() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    mount_t8407(
        &server,
        serde_json::json!([quote_row("005930", 61_000), quote_row("000660", 62_000)]),
    )
    .await;

    let (mut loop_cfg, mut rig) = day_rig(&server.uri(), &["005930"]);
    loop_cfg.stop_before_orders = true;
    let outcome = loop_cfg.run().await.expect("the session ends normally");

    assert!(outcome.stopped_before_orders);
    assert_eq!(outcome.bars_delivered, 0, "no bar was delivered");
    assert!(bars_received(&mut rig.rx).is_empty(), "the submit path is never reached");
    assert!(
        rig.signals.current().is_some(),
        "but the decision context WAS published — the decision is recorded"
    );
    let (_, _, notes) = rig.observations.snapshot();
    assert!(
        notes.iter().any(|n| n.contains("--stop-before-orders")),
        "{notes:?}"
    );
}

/// The session's OWN entries must survive into the book, with the stop that makes them
/// measurable. Without this the new leg is in the broker's snapshot and in no book, so it is
/// dropped as unattributable and the NEXT mount refuses on it — a rehearsal that could never
/// run two days in a row.
///
/// The same republication is what gives `join_live_risk` a stop to read: the strategy
/// publishes a leg's stop from `on_bar`, which for a newly entered symbol ran before the
/// auction opened the position and therefore published `None`.
#[tokio::test]
async fn a_new_entry_is_written_into_the_book_with_the_stop_the_risk_join_reads() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    mount_t8407(
        &server,
        serde_json::json!([quote_row("000660", 62_000), quote_row("035420", 63_000)]),
    )
    .await;

    let (loop_cfg, rig) = day_rig(&server.uri(), &[]);
    let stop_mult = loop_cfg.stop_atr_mult;
    rig.ledger.lock().unwrap().seed_fill(nautilus_ls::orders::ledger::LedgerFill {
        symbol: "000660".to_string(),
        side: nautilus_model::enums::OrderSide::Buy,
        qty: 12,
        price: 62_000,
        price_approximated: false,
        trade_id: nautilus_model::identifiers::TradeId::new("FILL-000660"),
        observed_ns: (MOUNT_UNIX as u64 + 30 * 60) * 1_000_000_000,
    });
    let outcome = loop_cfg.run().await.expect("the day completes");

    assert_eq!(outcome.entered.len(), 1, "only the filled entry: {:?}", outcome.entered);
    let entered = &outcome.entered[0];
    assert_eq!(entered.shcode, "000660");
    assert_eq!(entered.quantity, 12);
    assert_eq!(entered.entry_price, 62_000.0, "the realized fill price, not the decision one");
    // The strategy fixes `stop = avg_px_open − stop_atr_mult × ATR` at the open (R12); the
    // runner restates that formula from two frozen inputs rather than reaching into the
    // frozen head for it.
    assert_eq!(entered.stop_price, 62_000.0 - stop_mult * 1_200.0);
    assert!(entered.stop_price < entered.entry_price, "the entry-fixed risk is positive");
    // KTD13: the leg's day basis is THIS session's close — the number the next session's
    // breaker marks it against.
    assert_eq!(entered.prior_close, 62_000);
    assert_eq!(entered.entry_date, SESSION_DATE, "the session that opened it");
    assert_eq!(
        entered.entered_under, "20260911T060000Z-live-daily-ms-v1",
        "KTD2's leg-level label: a book may mix legs entered under different signals"
    );

    // And the book accepts what the session wrote — the round trip that stops the next
    // mount from refusing on a leg this one produced.
    let mut snapshot = std::collections::BTreeMap::new();
    snapshot.insert("000660".to_string(), 12);
    let previous = book_with(Vec::new(), PREVIOUS_SESSION);
    assert!(
        RehearsalBook::unattributed(&snapshot, &previous, &outcome.entered).is_empty(),
        "the session's own entry is attributable"
    );
    let closes: HashMap<String, i64> =
        outcome.closes.iter().map(|(id, c)| (id.symbol.as_str().to_string(), *c)).collect();
    let (next, _) = RehearsalBook::from_snapshot(
        &snapshot,
        &previous,
        &outcome.entered,
        &closes,
        SESSION_DATE,
        "run-2",
    );
    assert_eq!(next.legs.len(), 1);
    next.validate_fields().expect("the written book restores next session");

    // The stop the risk join reads was published against this session's close.
    let mark = rig.marks.get("000660").expect("the entry's mark was republished");
    assert_eq!(mark.stop_price, Some((62_000.0 - stop_mult * 1_200.0).round() as i64));
    assert_eq!(mark.last_close, 62_000);
}

/// The defect the 25-test suite could not see: `session_calendar` is the one function the
/// day-loop tests bypass, because `day_rig` hands `DayLoop` a hand-built list. It demanded
/// PROVEN sessions after today, and a proven session is retrospective by construction — so on
/// every real calendar it refused every mount, and nothing failed.
///
/// This drives the real function against a real calendar whose coverage ends in the future,
/// which is the shape an operator actually mounts against.
#[test]
fn session_calendar_builds_its_forward_window_from_candidate_days_not_proven_sessions() {
    use nautilus_ls_calendar::{AsOfView, DateRange};
    let Some(cal) = support_calendar() else { return };
    let view = AsOfView::new(&cal, Utc.timestamp_opt(MOUNT_UNIX, 0).unwrap())
        .expect("the fixture calendar authorizes this instant");
    let today = day(SESSION_DATE);
    let horizon = view.calendar().coverage().materialized_through;

    // The premise: the calendar proves NO session after today. If this ever stops holding,
    // the bug this test pins could return without the test noticing.
    let ahead = DateRange::inclusive(today.succ_opt().unwrap(), horizon).unwrap();
    assert!(
        view.sessions_in(&ahead).unwrap().is_empty(),
        "a proven session is retrospective, so the forward half can never come from sessions_in"
    );

    let (sessions, index) = session_calendar(&view, today, 16)
        .expect("a hold-length window is satisfiable from candidate days");
    assert_eq!(sessions[index], today, "today sits at the returned index");
    assert!(
        sessions.len() - index - 1 >= 16,
        "and the forward window is long enough for a full hold: {} ahead",
        sessions.len() - index - 1
    );
    assert!(sessions.windows(2).all(|w| w[0] < w[1]), "strictly ascending");

    // The backward half stays PROVEN-only, so a persisted entry date keeps its index.
    let past = DateRange::inclusive(day("2010-01-04"), today.pred_opt().unwrap()).unwrap();
    assert_eq!(&sessions[..index], view.sessions_in(&past).unwrap().as_slice());

    // A horizon that cannot cover a hold still refuses -- the gate is real, just satisfiable.
    let err = session_calendar(&view, today, 100_000).unwrap_err().to_string();
    assert!(err.contains("candidate trading day"), "{err}");
}

/// A leg whose entry-fixed risk comes out non-positive is REPORTED and excluded, not written
/// with a stop the strategy would refuse to restore next session.
#[tokio::test]
async fn a_degenerate_stop_excludes_the_leg_and_says_so() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    // Priced at 1,000 with a prior ATR of 1,200: `1000 - 1.5*1200` is negative.
    mount_t8407(&server, serde_json::json!([quote_row("000660", 1_000)])).await;

    let (loop_cfg, rig) = day_rig(&server.uri(), &[]);
    rig.ledger.lock().unwrap().seed_fill(nautilus_ls::orders::ledger::LedgerFill {
        symbol: "000660".to_string(),
        side: nautilus_model::enums::OrderSide::Buy,
        qty: 5,
        price: 1_000,
        price_approximated: false,
        trade_id: nautilus_model::identifiers::TradeId::new("FILL-DEGEN"),
        observed_ns: (MOUNT_UNIX as u64 + 30 * 60) * 1_000_000_000,
    });
    let outcome = loop_cfg.run().await.expect("the session still completes");

    assert!(outcome.entered.is_empty(), "the leg is not written: {:?}", outcome.entered);
    let (_, _, notes) = rig.observations.snapshot();
    assert!(
        notes.iter().any(|n| n.contains("non-positive entry-fixed risk")),
        "and the operator is told why, before the next mount refuses on it: {notes:?}"
    );
}

// ---------------------------------------------------------------------------
// 4. The book the next session inherits
// ---------------------------------------------------------------------------

/// KTD11: the book is written from the BROKER's snapshot, and the leg facts only the book
/// remembers ride along. A partially-exited leg carries the broker's remaining quantity; a
/// symbol the broker no longer reports is gone whatever the ledger thought.
#[test]
fn the_next_book_takes_membership_from_the_broker_and_leg_facts_from_the_previous_one() {
    let previous = book_with(
        vec![leg("005930", 10), leg("000660", 5), leg("035420", 3), leg("068270", 7)],
        PREVIOUS_SESSION,
    );
    let mut snapshot = std::collections::BTreeMap::new();
    snapshot.insert("005930".to_string(), 10); // untouched
    snapshot.insert("000660".to_string(), 2); // partially exited
    snapshot.insert("068270".to_string(), 7); // held, but halted -- no price read this session
                                              // 035420 fully exited — absent from the snapshot.

    let closes: HashMap<String, i64> =
        [("005930".to_string(), 63_000), ("000660".to_string(), 59_000)].into_iter().collect();
    let (next, stale) =
        RehearsalBook::from_snapshot(&snapshot, &previous, &[], &closes, SESSION_DATE, "run-2");
    // A leg the session could not price keeps yesterday's basis while the stamp moves to
    // today, so it is REPORTED rather than written silently — the next session's breaker
    // would otherwise measure its move from the wrong day and nothing downstream checks it.
    assert_eq!(stale, vec!["068270".to_string()], "{stale:?}");
    assert_eq!(next.legs.len(), 3, "the broker decides membership: {:?}", next.legs);
    let by_code: HashMap<&str, &RehearsalBookLeg> =
        next.legs.iter().map(|l| (l.shcode.as_str(), l)).collect();
    assert_eq!(by_code["000660"].quantity, 2, "the broker decides quantity");
    assert_eq!(
        by_code["000660"].stop_price, 57_000.0,
        "and the entry-fixed stop rides across from the previous book"
    );
    assert_eq!(
        by_code["000660"].entry_date, "2026-08-14",
        "as does the entry date the hold is measured from"
    );
    // KTD13: `prior_close` is the ONE field here that is not entry-fixed — it is the day
    // basis the next session's breaker marks against, so it advances every session. Carrying
    // the old value forward would turn an inherited leg's open mark into an
    // inception-to-date P&L that grows with every session held.
    assert_eq!(
        by_code["005930"].prior_close, 63_000,
        "the day basis is re-based on THIS session's close"
    );
    assert_eq!(by_code["000660"].prior_close, 59_000);
    assert_eq!(next.session_date, SESSION_DATE);
    assert_eq!(next.run_id, "run-2");
    assert_eq!(next.ordinal_epoch, SESSION_ORDINAL_EPOCH);
}

/// A holding the book cannot attribute is DROPPED from the written book — a leg with no stop
/// is one the strategy refuses to restore — and reported, so the operator learns why the next
/// mount will refuse before they get there.
#[test]
fn an_unattributable_holding_is_reported_rather_than_silently_written() {
    let previous = book_with(vec![leg("005930", 10)], PREVIOUS_SESSION);
    let mut snapshot = std::collections::BTreeMap::new();
    snapshot.insert("005930".to_string(), 10);
    snapshot.insert("068270".to_string(), 4); // nobody's leg

    let unattributed = RehearsalBook::unattributed(&snapshot, &previous, &[]);
    assert_eq!(unattributed, vec!["068270".to_string()]);
    let (next, _) = RehearsalBook::from_snapshot(
        &snapshot,
        &previous,
        &[],
        &HashMap::new(),
        SESSION_DATE,
        "run-2",
    );
    assert_eq!(next.legs.len(), 1, "the unattributable holding is not written");
}

/// The write is tmp + rename: a crash mid-write leaves the PREVIOUS book intact, which is
/// the one state the next session's probe can still act on.
#[test]
fn the_book_write_is_atomic_and_round_trips() {
    let home = tempdir().unwrap();
    let book = book_with(vec![leg("005930", 10)], SESSION_DATE);
    let path = book.write(home.path()).unwrap();
    assert_eq!(path, RehearsalBook::path(home.path()));
    assert!(
        !home.path().join("rehearsal").join("book.json.tmp").exists(),
        "the temp file does not survive the rename"
    );
    let restored = RehearsalBook::load(home.path()).unwrap();
    assert_eq!(restored, book, "the book round-trips byte-for-byte in meaning");
    restored.validate_fields().unwrap();
}

/// The rehearsal's teardown asserts a matching BOOK where the ladder's asserts flatness —
/// and hands back the snapshot the next book is written from, out of ONE t0424 read.
///
/// A ladder session's safe end state is flat. A rehearsal deliberately ends holding a
/// multi-session book, so asserting flatness would fail every healthy session; asserting
/// nothing would finalize a session that silently lost a position.
#[tokio::test]
async fn the_rehearsal_teardown_asserts_the_book_and_captures_the_snapshot() {
    use nautilus_ls::execution::QUIESCE_BUDGET;
    use nautilus_ls_lab::runner::live::{run_teardown, BookIntent, LiveTeardownSession};
    use nautilus_ls_lab::runner::pnl::seed_book_legs;
    use nautilus_ls_lab::strategy::hooks::EmissionGate;

    let server = MockServer::start().await;
    mount_token(&server).await;
    mount_t0425_empty(&server).await;
    mount_cancel_ok(&server).await;
    // The account holds exactly what the session intended to end holding.
    mount_t0424(
        &server,
        serde_json::json!([holding_row("005930", 10), holding_row("000660", 5)]),
        "150000000",
    )
    .await;

    let sdk = LsSdk::new(mock_config(&server.uri())).unwrap();
    let ledger: Arc<Mutex<FillLedger>> = Arc::new(Mutex::new(FillLedger::new()));
    let book = book_with(vec![leg("005930", 10), leg("000660", 5)], PREVIOUS_SESSION);
    seed_book_legs(&ledger, &book.pnl_legs(), 1_000);

    let intent = BookIntent::new();
    let session = LiveTeardownSession::new(
        EmissionGate::open(),
        sdk,
        Arc::clone(&ledger),
        nautilus_ls::execution::OrderDispatchTasks::new(),
    )
    .with_quiesce_budget(QUIESCE_BUDGET)
    .with_book_intent(intent.clone());

    let report = run_teardown(&session, 1, 1).await;
    assert!(report.canceled, "the cancels confirmed");
    assert!(
        report.flat_confirmed,
        "a matching BOOK is the rehearsal's confirmed end state, not an empty account"
    );
    assert!(!report.hard_failed(), "so the session is not ABNORMAL");
    assert!(!session.orders_enabled(), "and the kill switch went last");

    // KTD11: the snapshot came back out of the SAME read that produced the verdict, so the
    // book the next session inherits cannot be one this teardown never approved.
    let snapshot = intent.observed().expect("the confirming read captured what it saw");
    assert_eq!(snapshot.get("005930"), Some(&10));
    assert_eq!(snapshot.get("000660"), Some(&5));
    assert_eq!(snapshot.len(), 2);
}

/// The 72 path: an account that does not match the intent leaves `flat_confirmed` false AND
/// captures no snapshot — so `rehearsal/book.json` is not rewritten, the previous session's
/// book stands, and the next mount refuses on its stamp until someone reconciles.
#[tokio::test]
async fn an_unconfirmed_book_finalizes_abnormal_and_captures_no_snapshot() {
    use nautilus_ls_lab::runner::live::{run_teardown, BookIntent, LiveTeardownSession};
    use nautilus_ls_lab::runner::pnl::seed_book_legs;
    use nautilus_ls_lab::strategy::hooks::EmissionGate;

    let server = MockServer::start().await;
    mount_token(&server).await;
    mount_t0425_empty(&server).await;
    mount_cancel_ok(&server).await;
    // The session intended to end holding 10 of 005930; the account reports 4.
    mount_t0424(&server, serde_json::json!([holding_row("005930", 4)]), "150000000").await;

    let sdk = LsSdk::new(mock_config(&server.uri())).unwrap();
    let ledger: Arc<Mutex<FillLedger>> = Arc::new(Mutex::new(FillLedger::new()));
    seed_book_legs(&ledger, &book_with(vec![leg("005930", 10)], PREVIOUS_SESSION).pnl_legs(), 1_000);

    let intent = BookIntent::new();
    let session = LiveTeardownSession::new(
        EmissionGate::open(),
        sdk,
        Arc::clone(&ledger),
        nautilus_ls::execution::OrderDispatchTasks::new(),
    )
    .with_book_intent(intent.clone());

    let report = run_teardown(&session, 1, 1).await;
    assert!(
        !report.flat_confirmed,
        "the book was NOT confirmed — positive confirmation only, never a conclusion on ambiguity"
    );
    assert!(report.hard_failed(), "which is what finalizes the run ABNORMAL (72)");
    assert!(!session.orders_enabled(), "the kill switch still engaged, and still last");
    assert!(
        intent.observed().is_none(),
        "and NO snapshot was captured, so book.json is left standing rather than overwritten \
         with a book nobody verified"
    );
}

/// A session that seeds an inherited book puts it into the SAME ledger its own fills land
/// in, so the teardown's intent — "what I came in holding, plus what I bought, minus what I
/// sold" — falls out of the existing accounting with no second P&L path.
#[test]
fn the_teardown_intent_is_the_seeded_book_plus_the_sessions_own_fills() {
    use nautilus_ls_lab::runner::pnl::{intended_book, seed_book_legs};
    let ledger: Mutex<FillLedger> = Mutex::new(FillLedger::new());
    let book = book_with(vec![leg("005930", 10), leg("000660", 5)], PREVIOUS_SESSION);
    let seeded = seed_book_legs(&ledger, &book.pnl_legs(), 1_000);
    assert_eq!(seeded, 2);

    let intent = intended_book(&ledger);
    let by: HashMap<String, i64> = intent.into_iter().collect();
    assert_eq!(by.get("005930"), Some(&10));
    assert_eq!(by.get("000660"), Some(&5));
    assert_eq!(by.len(), 2, "and nothing else");
}

/// KTD11's ordinal contract: PROVEN sessions only, counted from a fixed epoch, and the last
/// one is the session the book must be stamped with.
#[test]
fn session_ordinals_count_proven_sessions_from_the_fixed_epoch() {
    // The epoch is a constant, not the snapshot's floor: a moving base would silently
    // shorten or extend every open hold the moment coverage changed.
    assert_eq!(SESSION_ORDINAL_EPOCH, "2010-01-04");
    let Some(snapshot) = std::env::var_os("LS_CALENDAR_SNAPSHOT") else {
        // The counting itself is proven against the operator's snapshot when one is
        // configured; the epoch contract above holds unconditionally.
        return;
    };
    let calendar = nautilus_ls::calendar::IngestCalendarContext::resolve(
        Some(std::path::PathBuf::from(snapshot)),
        Utc.timestamp_opt(MOUNT_UNIX, 0).unwrap(),
        nautilus_ls_calendar::CalendarAdoption::Enforced,
    );
    let Some(view) = calendar.view() else { return };
    let (count, last) = session_ordinal(&view, day(PREVIOUS_SESSION)).unwrap();
    assert!(count > 3_000, "sixteen years of sessions: {count}");
    assert!(last.is_some(), "and a last proven session to stamp the book with");
}
