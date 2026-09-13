//! U8 — the trip sink seam (KTD3, AE4).
//!
//! A safety trip has always been a **dispatch-chain** append, and the chain is also the
//! store that authorizes a mount. A paper rehearsal runs outside the ladder with no
//! dispatch at all (CONCEPTS.md), so leaving the trip path on the chain would have a
//! rehearsal *create* `dispatch/` — manufacturing the ladder's authorization store as a
//! side effect of watching its own heartbeat, on a machine whose operator never
//! authorized a ladder session.
//!
//! These tests drive the real `watchdog_tick` / `execute_trip` seam over both sinks. The
//! ladder's behaviour is asserted to be UNCHANGED, because "the rehearsal works" is only
//! half the claim worth making about a refactor of the safety path.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use chrono::{TimeZone, Utc};
use nautilus_ls_lab::dispatch::chain::{
    DispatchChain, RecordKind, SafetyTripKind, TripAction,
};
use nautilus_ls_lab::runner::live::LiveSession;
use nautilus_ls_lab::runner::watchdog::{
    execute_trip, watchdog_tick, ChainTripSink, RehearsalLedger, TripCause, TripLatch, TripSink,
    WatchdogLimits, WatchdogObservation,
};

const AT: i64 = 1_752_600_100;

/// A `Sync` fake session — the watchdog shares the handle across its own OS thread, so the
/// teardown target must be `Sync` (ladder KTD10). Records the call order; `halt` last is
/// the invariant every teardown assertion here reads.
#[derive(Default)]
struct FakeSession {
    log: Mutex<Vec<&'static str>>,
    teardowns: AtomicUsize,
}

impl LiveSession for FakeSession {
    fn stop_emission(&self) {
        self.log.lock().unwrap().push("stop_emission");
    }
    async fn cancel_all_resting(&self) -> anyhow::Result<usize> {
        self.teardowns.fetch_add(1, Ordering::SeqCst);
        self.log.lock().unwrap().push("cancel");
        Ok(0)
    }
    async fn is_flat(&self) -> bool {
        self.log.lock().unwrap().push("is_flat");
        true
    }
    fn halt(&self) {
        self.log.lock().unwrap().push("halt");
    }
}

fn limits() -> WatchdogLimits {
    WatchdogLimits { heartbeat_interval_secs: 30, max_loss_krw: 500_000.0 }
}

/// An observation with a stale runtime feeder — the dead-man condition.
fn stale() -> WatchdogObservation {
    WatchdogObservation {
        now_unix: AT,
        runtime_heartbeat_unix: AT - 40,
        operator_keepalive_unix: AT,
        realized_pnl_krw: 0.0,
        open_marked_pnl_krw: 0.0,
    }
}

fn seed_chain(dir: &std::path::Path) -> DispatchChain {
    let chain = DispatchChain::open(dir).unwrap();
    chain
        .append(Utc.timestamp_opt(1_752_600_000, 0).unwrap(), 1, 1, None, RecordKind::Genesis)
        .unwrap();
    chain
}

/// Covers AE4. The rehearsal's trip lands in its own ledger, and the ladder's authorization
/// store is never even created.
///
/// `dispatch/` existing at all on a rehearsal home is the failure this sink exists to
/// prevent: `DispatchChain::open` is a `create_dir_all`, so the pre-U8 code created the
/// directory merely by *watching*. A later reader finding `dispatch/` on a rehearsal home
/// cannot tell it apart from an abandoned ladder home.
#[tokio::test]
async fn a_rehearsal_trip_records_to_its_own_ledger_and_creates_no_dispatch_dir() {
    let tmp = tempfile::TempDir::new().unwrap();
    let home = tmp.path();
    let ledger = RehearsalLedger::new(home);
    let session = FakeSession::default();
    let latch = TripLatch::new();

    let cause = watchdog_tick(&session, &ledger, &latch, &stale(), &limits(), Some("run-r"))
        .await
        .unwrap();

    assert_eq!(cause, Some(TripCause::DeadManRuntime));
    assert_eq!(*session.log.lock().unwrap().last().unwrap(), "halt", "teardown ran, halt last");
    assert!(!home.join("dispatch").exists(), "a rehearsal never opens the dispatch chain");

    // The same PAIR of records `execute_trip` writes to the chain: the cause, then the
    // kill-switch engagement the halt performed.
    let rows = ledger.records().unwrap();
    assert_eq!(rows.len(), 2, "cause + kill-switch: {rows:?}");
    assert_eq!(rows[0].trip, SafetyTripKind::Watchdog);
    assert_eq!(rows[0].action, TripAction::Engage);
    assert_eq!(rows[0].run_id.as_deref(), Some("run-r"));
    assert_eq!(rows[1].trip, SafetyTripKind::KillSwitch);
    assert_eq!(rows[1].action, TripAction::Engage);
    assert!(rows[0].at_utc.starts_with("2025-"), "stamped at the observation's instant: {rows:?}");
}

/// The other half of the claim: the LADDER's trip is byte-for-byte the append it always
/// was. The sink refactor moved the writer; it must not have moved the record.
#[tokio::test]
async fn the_chain_sink_writes_the_record_the_ladder_has_always_written() {
    let tmp = tempfile::TempDir::new().unwrap();
    let chain = seed_chain(tmp.path());
    let sink = ChainTripSink::new(chain.clone(), 1);
    let session = FakeSession::default();
    let latch = TripLatch::new();

    let cause = watchdog_tick(&session, &sink, &latch, &stale(), &limits(), Some("run-l"))
        .await
        .unwrap();
    assert_eq!(cause, Some(TripCause::DeadManRuntime));

    let state = chain.load();
    assert!(
        state.records.iter().any(|r| matches!(&r.body.kind,
            RecordKind::SafetyTrip(t)
                if t.trip == SafetyTripKind::Watchdog
                    && t.action == TripAction::Engage
                    && t.run_id.as_deref() == Some("run-l"))),
        "the cause record is on the chain"
    );
    assert!(
        state.kill_switch_engaged,
        "and the kill-switch engagement persisted — the gate reds on the next --dispatch"
    );
    // No rehearsal ledger materializes beside it.
    assert!(!tmp.path().join("rehearsal").exists());
}

/// The latch is the arbiter regardless of sink: a second tick over the same latch does not
/// tear down again, so a rehearsal cannot double-halt where the ladder would not.
#[tokio::test]
async fn the_one_shot_latch_holds_across_the_rehearsal_sink_too() {
    let tmp = tempfile::TempDir::new().unwrap();
    let ledger = RehearsalLedger::new(tmp.path());
    let session = FakeSession::default();
    let latch = TripLatch::new();

    let first = watchdog_tick(&session, &ledger, &latch, &stale(), &limits(), None).await.unwrap();
    let second = watchdog_tick(&session, &ledger, &latch, &stale(), &limits(), None).await.unwrap();

    assert!(first.is_some());
    assert_eq!(second, None, "the second tick does not re-tear-down");
    assert_eq!(session.teardowns.load(Ordering::SeqCst), 1, "exactly one teardown");
    assert_eq!(ledger.records().unwrap().len(), 2, "and exactly one trip's worth of rows");
}

/// The ledger reopens the file on every append — that is what keeps it `Send + Sync` with no
/// lock, and it must therefore accumulate rather than truncate. An absent ledger is an empty
/// history (a rehearsal home that has never tripped has no file), which is the case U13's
/// mount gate reads as "no standing trip".
#[test]
fn the_ledger_appends_across_reopens_and_an_absent_one_is_an_empty_history() {
    let tmp = tempfile::TempDir::new().unwrap();
    let ledger = RehearsalLedger::new(tmp.path());
    assert!(ledger.records().unwrap().is_empty(), "an absent ledger is an empty history");

    let now = Utc.timestamp_opt(AT, 0).unwrap();
    ledger
        .record_trip(SafetyTripKind::Breaker, TripAction::Engage, Some("run-r"), "breaker", now)
        .unwrap();
    // A SEPARATE handle onto the same path, as U13's clear verb will be.
    RehearsalLedger::at(ledger.path().to_path_buf())
        .record_trip(SafetyTripKind::KillSwitch, TripAction::Clear, None, "operator clear", now)
        .unwrap();

    let rows = ledger.records().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].action, TripAction::Engage);
    assert_eq!(rows[1].action, TripAction::Clear, "the LAST row is what U13's gate reads");
    assert_eq!(rows[1].run_id, None);
}

/// A ledger whose tail cannot be parsed must NOT read as "no trip". U13 refuses the next
/// mount on a standing `Engage`, so an unreadable row has to fail closed — a tolerant read
/// would turn a corrupt ledger into a licence to mount.
#[test]
fn an_unparseable_ledger_row_is_an_error_not_an_empty_history() {
    let tmp = tempfile::TempDir::new().unwrap();
    let ledger = RehearsalLedger::new(tmp.path());
    ledger
        .record_trip(
            SafetyTripKind::Watchdog,
            TripAction::Engage,
            None,
            "dead man",
            Utc.timestamp_opt(AT, 0).unwrap(),
        )
        .unwrap();
    let mut text = std::fs::read_to_string(ledger.path()).unwrap();
    text.push_str("{ this is not a trip row }\n");
    std::fs::write(ledger.path(), text).unwrap();

    let err = ledger.records().expect_err("a corrupt tail is an error");
    assert!(err.to_string().contains("unreadable rehearsal trip row"), "{err}");
}

/// The chain scrubs a record's free text inside `append`; the rehearsal ledger has no such
/// wrapper, so it scrubs at the sink. A trip detail is the one free-text carrier in the row.
#[tokio::test]
async fn the_rehearsal_ledger_scrubs_the_detail_line() {
    let tmp = tempfile::TempDir::new().unwrap();
    let ledger = RehearsalLedger::new(tmp.path());
    let session = FakeSession::default();

    execute_trip(
        &session,
        &ledger,
        TripCause::MaxLoss,
        Some("run-r"),
        Utc.timestamp_opt(AT, 0).unwrap(),
    )
    .await
    .unwrap();

    let raw = std::fs::read_to_string(ledger.path()).unwrap();
    assert!(!raw.is_empty());
    // The scrubber is the adapter's; this pins that the ledger routes through it at all,
    // by checking a value the scrubber is known to redact.
    let scrubbed = nautilus_ls::scrub::scrub_secrets("appkey=PSxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
    ledger
        .record_trip(
            SafetyTripKind::Breaker,
            TripAction::Engage,
            None,
            "appkey=PSxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
            Utc.timestamp_opt(AT, 0).unwrap(),
        )
        .unwrap();
    let rows = ledger.records().unwrap();
    assert_eq!(rows.last().unwrap().detail, scrubbed, "the detail went through the scrubber");
}
