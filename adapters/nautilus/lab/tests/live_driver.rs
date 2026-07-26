//! live-session-driver U3/U4 — the driver that owns `node.run`'s lifecycle and the full
//! watchdog envelope armed live (R4, R5; KTD5, KTD7, KTD8).
//!
//! `node.run` is the ONE seam never exercised offline: the driver takes it as a
//! substitutable closure, so every surrounding seam — the session timer, the watchdog
//! thread, the exactly-one teardown, the breaker feeders, the staging and the finalize —
//! is driven here against the **real** [`LiveTeardownSession`] over a mock gateway.
//!
//! The clock is injected and frozen; nothing waits on a real heartbeat interval.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{TimeZone, Utc};
use ls_sdk::LsSdk;
use ls_sdk_test_support::{mock_config, mount_token};
use nautilus_ls::execution::OrderDispatchTasks;
use nautilus_ls::orders::ledger::{FillLedger, FillObservation};
use nautilus_ls_lab::agent::sink::DecisionSink;
use nautilus_ls_lab::artifacts::manifest::{DispatchLink, Manifest};
use nautilus_ls_lab::artifacts::{aborted_runs, DATA_QUALITY_FILE, MANIFEST_FILE, PERFORMANCE_FILE};
use nautilus_ls_lab::dispatch::chain::{DispatchChain, RecordKind, SafetyTripKind};
use nautilus_ls_lab::params::OrbParams;
use nautilus_ls_lab::runner::live::{
    run_live_session, LiveDriverConfig, LiveSessionContext, LiveSessionHandles,
    LiveTeardownSession, SessionClock,
};
use nautilus_ls_lab::runner::pnl::MarkPolicy;
use nautilus_ls_lab::runner::watchdog::{Heartbeats, TripCause, TripLatch, WatchdogLimits};
use nautilus_ls_lab::strategy::orb::{EmissionGate, MarkFeed, SymbolMark};
use nautilus_live::node::LiveNodeHandle;
use nautilus_model::identifiers::{ClientOrderId, InstrumentId, TraderId};
use nautilus_model::orders::OrderAny;
use tempfile::{tempdir, TempDir};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ACCNO_PATH: &str = "/stock/accno";
const ORDER_PATH: &str = "/stock/order";

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

/// A frozen clock — the driver and the watchdog read the same instant every tick, so a
/// trip fires on evaluation, never on elapsed wall time.
fn frozen(at: i64) -> SessionClock {
    Arc::new(move || at)
}

fn ok_json(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_string(body.to_string())
        .insert_header("content-type", "application/json")
}

async fn mount_t0425(server: &MockServer, rows: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path(ACCNO_PATH))
        .and(header("tr_cd", "t0425"))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "00000",
            "t0425OutBlock": { "tqty": "0", "tcheqty": "0", "tordrem": "0", "cts_ordno": "" },
            "t0425OutBlock1": rows
        })))
        .mount(server)
        .await;
}

async fn mount_t0424_flat(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path(ACCNO_PATH))
        .and(header("tr_cd", "t0424"))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "00000", "t0424OutBlock": {}, "t0424OutBlock1": []
        })))
        .mount(server)
        .await;
}

async fn mount_cancel_ok(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path(ORDER_PATH))
        .and(header("tr_cd", "CSPAT00801"))
        .respond_with(ok_json(serde_json::json!({
            "rsp_cd": "00463", "rsp_msg": "OK",
            "CSPAT00801OutBlock1": {}, "CSPAT00801OutBlock2": { "OrdNo": "9001" }
        })))
        .mount(server)
        .await;
}

fn resting_row(ordno: &str) -> serde_json::Value {
    serde_json::json!({
        "ordno": ordno, "expcode": "005930", "medosu": "매수", "qty": "10",
        "price": "60000", "cheqty": "0", "ordrem": "10", "status": "접수",
        "orgordno": "", "ordtime": "0900"
    })
}

async fn count_requests(server: &MockServer, tr_cd: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| r.headers.get("tr_cd").and_then(|v| v.to_str().ok()) == Some(tr_cd))
        .count()
}

/// A test rig: a mock gateway, a seeded dispatch chain, and the node-free handle set the
/// driver runs on. No `LiveNode` is built — the driver never needs one.
struct Rig {
    home: TempDir,
    handles: LiveSessionHandles,
    ledger: Arc<Mutex<FillLedger>>,
    marks: MarkFeed,
}

async fn rig(server: &MockServer, heartbeat_at: i64) -> Rig {
    mount_token(server).await;
    let sdk = LsSdk::new(mock_config(&server.uri())).unwrap();
    let ledger: Arc<Mutex<FillLedger>> = Arc::new(Mutex::new(FillLedger::new()));
    let marks = MarkFeed::new();
    let session = LiveTeardownSession::new(
        EmissionGate::open(),
        sdk,
        Arc::clone(&ledger),
        OrderDispatchTasks::new(),
    );
    let home = tempdir().unwrap();
    seed_chain(home.path());
    Rig {
        handles: LiveSessionHandles {
            session,
            heartbeats: Heartbeats::new(heartbeat_at),
            handle: LiveNodeHandle::new(),
            sink: DecisionSink::new(),
            marks: marks.clone(),
        },
        home,
        ledger,
        marks,
    }
}

fn seed_chain(dir: &Path) {
    let chain = DispatchChain::open(dir).unwrap();
    chain
        .append(Utc.timestamp_opt(1_752_600_000, 0).unwrap(), 1, 1, None, RecordKind::Genesis)
        .unwrap();
}

fn limits() -> WatchdogLimits {
    WatchdogLimits { heartbeat_interval_secs: 30, max_loss_krw: 500_000.0 }
}

fn driver_cfg(keepalive: &Path) -> LiveDriverConfig {
    LiveDriverConfig {
        // Long enough that the timer never fires in a test that ends another way; the
        // timer's own test sets it to 0.
        session_secs: 3_600,
        // Milliseconds, not the operator's minute-scale default: long enough that a
        // cooperative node returns first, short enough that the suite never sleeps.
        stop_grace: Duration::from_millis(50),
        watchdog_tick: Duration::from_millis(10),
        limits: limits(),
        mark_policy: MarkPolicy::default(),
        keepalive_path: keepalive.to_path_buf(),
        cancel_attempts: 1,
        flat_attempts: 1,
        starting_balance: 10_000_000.0,
    }
}

fn ctx(home: &Path) -> LiveSessionContext {
    LiveSessionContext {
        data_home: home.to_path_buf(),
        run_id: "20260725T010000Z-live-orb-v34".to_string(),
        chain_rung: 1,
        dispatch: Some(DispatchLink {
            dispatch_id: "rec-1".into(),
            rung: 1,
            rung_fraction: 0.1,
            lane: "cafef00d".into(),
            trading_env: "paper".into(),
        }),
        params: OrbParams::default(),
        symbols: vec!["005930.XKRX".to_string()],
        trading_date: "20260725".to_string(),
        created_utc: "2026-07-25T01:00:00Z".to_string(),
    }
}

/// A fresh operator keepalive file (its mtime is the feeder).
fn keepalive(dir: &Path) -> std::path::PathBuf {
    let p = dir.join("op.keepalive");
    std::fs::write(&p, b"alive").unwrap();
    p
}

/// A scripted `node.run` that returns immediately — the session-end path.
async fn returns_immediately(_h: LiveNodeHandle) -> anyhow::Result<()> {
    Ok(())
}

/// A scripted `node.run` that blocks until the node is asked to stop — what the real
/// `node.run` does. Used to prove the timer and the watchdog can end a session.
async fn blocks_until_stopped(h: LiveNodeHandle) -> anyhow::Result<()> {
    while !h.should_stop() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Ok(())
}

/// The adversarial twin of [`blocks_until_stopped`]: a scripted `node.run` that **ignores**
/// the stop request and never returns — a wedged broker socket, a drain that never
/// completes, a bug upstream. Without a timed hard-stop the driver blocks on this forever
/// and neither the teardown nor the finalize is ever reached.
async fn never_stops(_h: LiveNodeHandle) -> anyhow::Result<()> {
    loop {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// A scripted `node.run` that observes the stop and then returns its OWN error — the error
/// must still reach the run's data quality, never masked by the hard-stop plumbing.
async fn errors_after_stop(h: LiveNodeHandle) -> anyhow::Result<()> {
    while !h.should_stop() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    anyhow::bail!("the node's own shutdown error")
}

/// Every hard-stop test must be bounded: a regression that reinstates the block would
/// otherwise wedge the whole gate instead of failing it.
const HARD_STOP_TEST_CEILING: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// U3 — one teardown, whoever wins the atomic claim.
// ---------------------------------------------------------------------------

/// Session-end path (no trip): the scripted `run_node` returns → the driver wins the
/// claim → exactly one fail-closed teardown, halt last → the run finalizes NORMAL with the
/// dispatch link, the performance report, and the drained decisions.
#[tokio::test]
async fn the_session_end_path_tears_down_once_and_finalizes_a_normal_run() {
    let server = MockServer::start().await;
    let base = now_secs();
    let r = rig(&server, base).await;
    mount_t0425(&server, serde_json::json!([])).await;
    mount_t0424_flat(&server).await;
    let ka = keepalive(r.home.path());

    let outcome = run_live_session(
        r.handles.clone(),
        &driver_cfg(&ka),
        &ctx(r.home.path()),
        frozen(base),
        returns_immediately,
    )
    .await
    .expect("the session finalizes");

    assert!(outcome.trip.is_none(), "no watchdog trip on a clean session end");
    assert!(!outcome.abnormal, "a confirmed-flat teardown finalizes NORMAL");
    assert!(!outcome.report.hard_failed());
    assert!(
        !r.handles.session.orders_enabled(),
        "the teardown engaged the kill switch (halt runs on every path)"
    );

    // Artifacts: finalized, linked, and nothing left aborted.
    let m: Manifest =
        serde_json::from_str(&std::fs::read_to_string(outcome.run_dir.join(MANIFEST_FILE)).unwrap())
            .unwrap();
    assert_eq!(m.dispatch.expect("the run carries its DispatchLink").dispatch_id, "rec-1");
    assert!(outcome.run_dir.join(PERFORMANCE_FILE).exists(), "the performance report is staged");
    assert!(
        aborted_runs(r.home.path()).is_empty(),
        "a finalized run leaves no `.tmp-` residue"
    );
}

/// The session timer stops the node: `node.run` has no duration of its own, so the driver
/// owns it. A zero-second session ends a `run_node` that would otherwise block forever.
#[tokio::test]
async fn the_session_timer_stops_a_node_that_would_run_forever() {
    let server = MockServer::start().await;
    let base = now_secs();
    let r = rig(&server, base).await;
    mount_t0425(&server, serde_json::json!([])).await;
    mount_t0424_flat(&server).await;
    let ka = keepalive(r.home.path());
    let mut cfg = driver_cfg(&ka);
    cfg.session_secs = 0;

    let outcome = run_live_session(
        r.handles.clone(),
        &cfg,
        &ctx(r.home.path()),
        frozen(base),
        blocks_until_stopped,
    )
    .await
    .expect("the timed-out session finalizes");
    assert!(outcome.trip.is_none(), "the timer is not a safety trip");
    assert!(r.handles.handle.should_stop(), "the timer asked the node to stop");
}

/// The hard-stop: a node that IGNORES `should_stop()` no longer blocks the driver. The
/// stop is requested (here by the session timer), the node is given its grace, and when it
/// does not return the driver abandons it and reaches the SAME downstream path — one
/// teardown, then finalize — marking the run ABNORMAL.
#[tokio::test]
async fn a_node_that_ignores_stop_is_hard_stopped_and_still_finalizes() {
    let server = MockServer::start().await;
    let base = now_secs();
    let r = rig(&server, base).await;
    mount_t0425(&server, serde_json::json!([])).await;
    mount_t0424_flat(&server).await;
    let ka = keepalive(r.home.path());
    let mut cfg = driver_cfg(&ka);
    cfg.session_secs = 0;

    let outcome = tokio::time::timeout(
        HARD_STOP_TEST_CEILING,
        run_live_session(r.handles.clone(), &cfg, &ctx(r.home.path()), frozen(base), never_stops),
    )
    .await
    .expect("the driver must NOT block on a node that ignores stop")
    .expect("the hard-stopped session finalizes");

    assert!(outcome.hard_stopped, "the node was abandoned at the deadline");
    assert!(outcome.abnormal, "a node that ignored its stop never reads as a clean session");
    assert!(outcome.trip.is_none(), "the hard-stop is the DRIVER's, not a safety trip");
    assert!(
        !r.handles.session.orders_enabled(),
        "the teardown still ran (halt runs on every path)"
    );
    assert!(
        aborted_runs(r.home.path()).is_empty(),
        "the run finalized — no `.tmp-` residue for the operator to reconcile by hand"
    );
    let dq: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(outcome.run_dir.join(DATA_QUALITY_FILE)).unwrap())
            .unwrap();
    assert_eq!(
        dq["hard_stopped"], serde_json::json!(true),
        "the TYPED flag is what the ladder and readiness scans read — finalizing this run \
         removed the `.tmp-` residue that used to be the only signal: {dq}"
    );
    assert!(
        dq["observations"].to_string().contains("HARD STOP"),
        "and the human-readable cause is greppable too: {dq}"
    );
}

/// The grace is STOP-RELATIVE, not session-relative. Here the session timer never fires
/// (an hour-long session) — the WATCHDOG asks the node to stop on its first tick. A
/// `session_secs + grace` deadline would leave the driver blocked for the rest of the hour;
/// the flag-armed backstop bounds it by the grace instead.
#[tokio::test]
async fn the_hard_stop_grace_is_relative_to_the_stop_request_not_the_session() {
    let server = MockServer::start().await;
    let base = now_secs();
    // A far-stale runtime heartbeat → the first watchdog tick trips the dead-man, which
    // tears down on its own thread and asks the node to stop mid-session.
    let r = rig(&server, base - 10_000).await;
    mount_t0425(&server, serde_json::json!([])).await;
    mount_t0424_flat(&server).await;
    let ka = keepalive(r.home.path());
    let cfg = driver_cfg(&ka); // session_secs stays 3_600 — the timer NEVER fires here.

    let outcome = tokio::time::timeout(
        HARD_STOP_TEST_CEILING,
        run_live_session(r.handles.clone(), &cfg, &ctx(r.home.path()), frozen(base), never_stops),
    )
    .await
    .expect("a mid-session stop must be bounded by the grace, not by the session length")
    .expect("the hard-stopped session finalizes");

    assert!(outcome.hard_stopped, "the node ignored the watchdog's stop too");
    assert_eq!(
        outcome.trip,
        Some(TripCause::DeadManRuntime),
        "the watchdog won the latch; the driver's hard-stop is what unblocked the finalize"
    );
    assert!(outcome.abnormal);
    assert!(aborted_runs(r.home.path()).is_empty(), "the run still finalized");
}

/// The backstop must not fire on the cooperative path: a node that observes the stop and
/// returns within the grace finalizes exactly as it did before the hard-stop existed. The
/// grace here is 100x the node's own poll interval, so a wrongly-armed backstop is visible.
#[tokio::test]
async fn a_cooperative_node_is_never_hard_stopped() {
    let server = MockServer::start().await;
    let base = now_secs();
    let r = rig(&server, base).await;
    mount_t0425(&server, serde_json::json!([])).await;
    mount_t0424_flat(&server).await;
    let ka = keepalive(r.home.path());
    let mut cfg = driver_cfg(&ka);
    cfg.session_secs = 0;
    cfg.stop_grace = Duration::from_secs(5);

    let outcome = tokio::time::timeout(
        HARD_STOP_TEST_CEILING,
        run_live_session(
            r.handles.clone(),
            &cfg,
            &ctx(r.home.path()),
            frozen(base),
            blocks_until_stopped,
        ),
    )
    .await
    .expect("a cooperative node returns long before its grace")
    .expect("the session finalizes");

    assert!(!outcome.hard_stopped, "the node stopped on request — nothing to hard-stop");
    assert!(!outcome.abnormal, "a confirmed-flat cooperative teardown still finalizes NORMAL");
    let dq = std::fs::read_to_string(outcome.run_dir.join(DATA_QUALITY_FILE)).unwrap();
    assert!(!dq.contains("HARD STOP"), "no spurious hard-stop observation: {dq}");
}

/// The node's OWN error still reaches the run's data quality — the hard-stop plumbing adds
/// a branch to `run_result`, and it must not swallow the branch that already existed.
#[tokio::test]
async fn a_node_error_after_stop_is_not_masked_by_the_hard_stop_plumbing() {
    let server = MockServer::start().await;
    let base = now_secs();
    let r = rig(&server, base).await;
    mount_t0425(&server, serde_json::json!([])).await;
    mount_t0424_flat(&server).await;
    let ka = keepalive(r.home.path());
    let mut cfg = driver_cfg(&ka);
    cfg.session_secs = 0;
    cfg.stop_grace = Duration::from_secs(5);

    let outcome = tokio::time::timeout(
        HARD_STOP_TEST_CEILING,
        run_live_session(
            r.handles.clone(),
            &cfg,
            &ctx(r.home.path()),
            frozen(base),
            errors_after_stop,
        ),
    )
    .await
    .expect("the node returned its error well inside the grace")
    .expect("a node error still finalizes");

    assert!(!outcome.hard_stopped, "the node DID return — it just returned an error");
    let dq = std::fs::read_to_string(outcome.run_dir.join(DATA_QUALITY_FILE)).unwrap();
    assert!(
        dq.contains("node.run returned an error"),
        "the node's own error is still observed: {dq}"
    );
    assert!(!dq.contains("HARD STOP"), "and it is not relabelled as a hard-stop: {dq}");
}

/// Trip-during-run: the watchdog claims the latch mid-run, tears down on ITS runtime
/// (halt last), unblocks `node.run`, and the post-run driver's `try_claim()` **loses** —
/// so the cancel is attempted exactly once. A non-atomic `is_tripped()` read here would
/// race the claim and let both paths tear down.
#[tokio::test]
async fn a_watchdog_trip_tears_down_once_and_the_driver_loses_the_claim() {
    let server = MockServer::start().await;
    let base = now_secs();
    // The runtime heartbeat is far stale → the first watchdog tick trips the dead-man.
    let r = rig(&server, base - 10_000).await;
    // A resting order that survives the cancel, so each teardown issues exactly one
    // cancel — the request count IS the teardown count.
    mount_t0425(&server, serde_json::json!([resting_row("1001")])).await;
    mount_t0424_flat(&server).await;
    mount_cancel_ok(&server).await;
    let ka = keepalive(r.home.path());

    let outcome = run_live_session(
        r.handles.clone(),
        &driver_cfg(&ka),
        &ctx(r.home.path()),
        frozen(base),
        blocks_until_stopped,
    )
    .await
    .expect("the tripped session still finalizes");

    assert_eq!(outcome.trip, Some(TripCause::DeadManRuntime), "the stale runtime feeder tripped");
    assert!(r.handles.handle.should_stop(), "the trip called handle.stop() to unblock node.run");
    assert_eq!(
        count_requests(&server, "CSPAT00801").await,
        1,
        "EXACTLY ONE teardown ran — the driver lost the atomic claim and did not tear down again"
    );
    assert!(!r.handles.session.orders_enabled(), "halt engaged");

    // The trip is durable: a fresh dispatch process reds on the persisted records.
    let state = DispatchChain::open(r.home.path()).unwrap().load();
    assert!(
        state.records.iter().any(|rec| matches!(&rec.body.kind,
            RecordKind::SafetyTrip(t) if t.trip == SafetyTripKind::Watchdog)),
        "the cause record is persisted"
    );
    assert!(state.kill_switch_engaged, "the kill-switch engagement is persisted");
    // A hard-failed teardown (the account never reads flat) finalizes ABNORMAL.
    assert!(outcome.abnormal, "an unconfirmed-flat teardown finalizes abnormal");
    assert!(outcome.run_dir.join(MANIFEST_FILE).exists(), "abnormal runs still leave artifacts");
}

/// The simulated tie the non-atomic read would have lost: many threads racing the SAME
/// latch yield exactly one winner, so session-end and a concurrent trip can never both
/// tear down.
#[test]
fn a_simulated_tie_on_the_trip_latch_yields_exactly_one_winner() {
    let latch = Arc::new(TripLatch::new());
    let winners = Arc::new(AtomicUsize::new(0));
    let mut threads = Vec::new();
    for _ in 0..16 {
        let latch = Arc::clone(&latch);
        let winners = Arc::clone(&winners);
        threads.push(std::thread::spawn(move || {
            if latch.try_claim() {
                winners.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }
    for t in threads {
        t.join().unwrap();
    }
    assert_eq!(winners.load(Ordering::SeqCst), 1, "exactly one teardown, however close the race");
}

/// A hard-failed teardown (the cancel cannot be confirmed) still finalizes — abnormally —
/// and the driver surfaces that to the caller. A session that carries limit events must
/// leave scannable artifacts (R5), never bail before writing them.
#[tokio::test]
async fn a_hard_failed_teardown_finalizes_abnormally_and_surfaces_it() {
    let server = MockServer::start().await;
    let base = now_secs();
    let r = rig(&server, base).await;
    mount_t0425(&server, serde_json::json!([resting_row("1001")])).await;
    mount_t0424_flat(&server).await;
    // The cancel is a MAY-REST 5xx — never confirmed, so the teardown is not safe.
    Mock::given(method("POST"))
        .and(path(ORDER_PATH))
        .and(header("tr_cd", "CSPAT00801"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let ka = keepalive(r.home.path());

    let outcome = run_live_session(
        r.handles.clone(),
        &driver_cfg(&ka),
        &ctx(r.home.path()),
        frozen(base),
        returns_immediately,
    )
    .await
    .expect("an abnormal session still finalizes");

    assert!(!outcome.report.canceled, "the cancel was never confirmed");
    assert!(outcome.abnormal, "the outcome surfaces the abnormal finalize to the caller");
    assert!(!r.handles.session.orders_enabled(), "the kill switch is engaged regardless");
    let dq = std::fs::read_to_string(outcome.run_dir.join(DATA_QUALITY_FILE)).unwrap();
    assert!(dq.contains("ABNORMAL"), "the data-quality report records the abnormal teardown: {dq}");
}

/// Mutual liveness (ladder KTD10): a silent supervisor makes the SESSION side tear down,
/// so a dead watchdog thread never silently degrades the envelope to attended-only — and
/// it shares the watchdog's **one** [`TripLatch`], so a real watchdog trip and this
/// session-side trip together still tear down exactly once.
///
/// Driven at the tick seam over the REAL session (the driver-level composition cannot
/// script a dead watchdog thread: its own dead-man would fire first once time advanced).
#[tokio::test]
async fn the_session_side_liveness_trip_shares_one_latch_with_the_watchdog() {
    use nautilus_ls_lab::runner::watchdog::{
        session_liveness_tick, watchdog_tick, WatchdogObservation,
    };
    let server = MockServer::start().await;
    let base = now_secs();
    let r = rig(&server, base).await;
    mount_t0425(&server, serde_json::json!([resting_row("1001")])).await;
    mount_t0424_flat(&server).await;
    mount_cancel_ok(&server).await;

    let chain = DispatchChain::open(r.home.path()).unwrap();
    let latch = TripLatch::new();

    // The supervisor has been silent past the interval → the SESSION side tears down.
    let cause = session_liveness_tick(
        &r.handles.session,
        &chain,
        &latch,
        base,
        base - 10_000,
        limits().heartbeat_interval_secs,
        Some("run-1"),
        1,
    )
    .await
    .unwrap();
    assert_eq!(cause, Some(TripCause::SupervisorSilent));
    assert!(!r.handles.session.orders_enabled(), "the session-side teardown halted, last");
    assert!(chain.load().kill_switch_engaged, "the trip persisted the kill switch");

    // Now the watchdog also sees a trip condition on the SAME latch — no second teardown.
    let obs = WatchdogObservation {
        now_unix: base,
        runtime_heartbeat_unix: base - 10_000,
        operator_keepalive_unix: base,
        realized_pnl_krw: 0.0,
        open_marked_pnl_krw: 0.0,
    };
    let second = watchdog_tick(&r.handles.session, &chain, &latch, &obs, &limits(), Some("run-1"), 1)
        .await
        .unwrap();
    assert_eq!(second, None, "the latch is already claimed — the watchdog does not re-tear-down");
    assert_eq!(
        count_requests(&server, "CSPAT00801").await,
        1,
        "exactly one teardown across both supervisors"
    );
}

// ---------------------------------------------------------------------------
// U4 — the live max-loss breaker: fed from the shared ledger + the mark feed.
// ---------------------------------------------------------------------------

fn limit_order(client_id: &str, symbol: &str, side: nautilus_model::enums::OrderSide, qty: i64, price: i64) -> OrderAny {
    use nautilus_model::enums::{OrderType, TimeInForce};
    use nautilus_model::identifiers::StrategyId;
    use nautilus_model::orders::OrderTestBuilder;
    use nautilus_model::types::{Price, Quantity};
    OrderTestBuilder::new(OrderType::Limit)
        .trader_id(TraderId::from("LS-LAB-001"))
        .strategy_id(StrategyId::from("S-ORB-1"))
        .instrument_id(InstrumentId::from(format!("{symbol}.XKRX").as_str()))
        .client_order_id(ClientOrderId::from(client_id))
        .side(side)
        .quantity(Quantity::from(qty as u64))
        .price(Price::from(price.to_string().as_str()))
        .time_in_force(TimeInForce::Day)
        .build()
}

/// Seed a filled order into the shared ledger — the fills the breaker's realized-P&L
/// accounting runs over.
fn seed_fill(
    ledger: &Mutex<FillLedger>,
    client_id: &str,
    ord_no: &str,
    symbol: &str,
    side: nautilus_model::enums::OrderSide,
    qty: i64,
    price: i64,
) {
    let mut led = ledger.lock().unwrap();
    led.register(limit_order(client_id, symbol, side, qty, price), ord_no);
    let outcome = led.apply(FillObservation::poll(ord_no, qty, price, false));
    assert_eq!(outcome.deltas.len(), 1, "the seeded fill emitted");
}

/// The breaker is live-fed (R5, KTD8): realized −200k from offsetting fills on the SHARED
/// ledger, plus an open position marked at the adverse edge for −300k, crosses the
/// pre-registered 500k threshold → a `MaxLoss` trip, one teardown, halt last, and a
/// `Breaker` safety record persisted.
#[tokio::test]
async fn the_max_loss_breaker_trips_on_realized_plus_adverse_marked_open_pnl() {
    use nautilus_model::enums::OrderSide;
    let server = MockServer::start().await;
    let base = now_secs();
    let r = rig(&server, base).await;
    mount_t0425(&server, serde_json::json!([resting_row("1001")])).await;
    mount_t0424_flat(&server).await;
    mount_cancel_ok(&server).await;
    let ka = keepalive(r.home.path());

    // Realized: bought 10 @ 60_000, sold 10 @ 40_000 → −200_000.
    seed_fill(&r.ledger, "O-1", "1001", "005930", OrderSide::Buy, 10, 60_000);
    seed_fill(&r.ledger, "O-2", "1002", "005930", OrderSide::Sell, 10, 40_000);
    // Open: 10 @ 100_000 on another symbol, marked at its stop 70_000 → −300_000.
    seed_fill(&r.ledger, "O-3", "1003", "000660", OrderSide::Buy, 10, 100_000);
    r.marks.observe(
        "000660",
        SymbolMark { last_close: 99_000, last_bar_unix: base, stop_price: Some(70_000) },
    );

    let outcome = run_live_session(
        r.handles.clone(),
        &driver_cfg(&ka),
        &ctx(r.home.path()),
        frozen(base),
        blocks_until_stopped,
    )
    .await
    .expect("the breaker trip finalizes");

    assert_eq!(outcome.trip, Some(TripCause::MaxLoss), "−200k realized + −300k marked = −500k");
    assert_eq!(count_requests(&server, "CSPAT00801").await, 1, "exactly one teardown");
    let state = DispatchChain::open(r.home.path()).unwrap().load();
    assert!(
        state.records.iter().any(|rec| matches!(&rec.body.kind,
            RecordKind::SafetyTrip(t) if t.trip == SafetyTripKind::Breaker)),
        "the MaxLoss cause maps to a Breaker safety record"
    );
    assert!(state.kill_switch_engaged);
}

/// The boundary: one KRW short of the threshold stays healthy, so the session runs to its
/// own end. Proves the breaker is not simply always-on.
#[tokio::test]
async fn a_loss_just_inside_the_threshold_does_not_trip() {
    use nautilus_model::enums::OrderSide;
    let server = MockServer::start().await;
    let base = now_secs();
    let r = rig(&server, base).await;
    mount_t0425(&server, serde_json::json!([])).await;
    mount_t0424_flat(&server).await;
    let ka = keepalive(r.home.path());

    seed_fill(&r.ledger, "O-1", "1001", "005930", OrderSide::Buy, 10, 60_000);
    seed_fill(&r.ledger, "O-2", "1002", "005930", OrderSide::Sell, 10, 40_000); // −200_000
    seed_fill(&r.ledger, "O-3", "1003", "000660", OrderSide::Buy, 10, 100_000);
    // Fresh mark with a stop at 70_100 → (70_100 − 100_000) × 10 = −299_000. Total −499_000.
    r.marks.observe(
        "000660",
        SymbolMark { last_close: 99_000, last_bar_unix: base, stop_price: Some(70_100) },
    );

    let outcome = run_live_session(
        r.handles.clone(),
        &driver_cfg(&ka),
        &ctx(r.home.path()),
        frozen(base),
        returns_immediately,
    )
    .await
    .expect("a healthy session finalizes");
    assert!(outcome.trip.is_none(), "−499_000 is inside the 500_000 threshold");
}

/// **The stale-feed floor (KTD8(b)) — the claim proven exactly where it matters.** The
/// last-seen price is FAVORABLE but the feed died: a naive last-price mark would report
/// the position at a profit and the breaker would never trip. The floor (the position's
/// stop level) takes over and the breaker trips.
#[tokio::test]
async fn a_stale_favorable_feed_still_trips_the_breaker_via_the_floor() {
    use nautilus_model::enums::OrderSide;
    let server = MockServer::start().await;
    let base = now_secs();
    let r = rig(&server, base).await;
    mount_t0425(&server, serde_json::json!([resting_row("1001")])).await;
    mount_t0424_flat(&server).await;
    mount_cancel_ok(&server).await;
    let ka = keepalive(r.home.path());

    seed_fill(&r.ledger, "O-1", "1001", "005930", OrderSide::Buy, 10, 60_000);
    seed_fill(&r.ledger, "O-2", "1002", "005930", OrderSide::Sell, 10, 40_000); // −200_000
    seed_fill(&r.ledger, "O-3", "1003", "000660", OrderSide::Buy, 10, 100_000);
    // A favorable last close from an hour ago — the market-data gap that accompanies the
    // adverse move. Marked at the stop instead → −300_000, tripping the breaker.
    r.marks.observe(
        "000660",
        SymbolMark { last_close: 120_000, last_bar_unix: base - 3_600, stop_price: Some(70_000) },
    );

    let outcome = run_live_session(
        r.handles.clone(),
        &driver_cfg(&ka),
        &ctx(r.home.path()),
        frozen(base),
        blocks_until_stopped,
    )
    .await
    .expect("the stale-feed trip finalizes");
    assert_eq!(
        outcome.trip,
        Some(TripCause::MaxLoss),
        "a stale-FAVORABLE price must never suppress the breaker"
    );
}

/// The operator keepalive is the second dead-man feeder: an absent file reads as stale
/// (fail-closed) and trips, while a fresh one does not.
#[tokio::test]
async fn an_absent_operator_keepalive_trips_the_dead_man() {
    let server = MockServer::start().await;
    let base = now_secs();
    let r = rig(&server, base).await;
    mount_t0425(&server, serde_json::json!([resting_row("1001")])).await;
    mount_t0424_flat(&server).await;
    mount_cancel_ok(&server).await;

    // The runtime feeder is fresh; the keepalive file does not exist.
    let cfg = driver_cfg(&r.home.path().join("never-written.keepalive"));
    let outcome = run_live_session(
        r.handles.clone(),
        &cfg,
        &ctx(r.home.path()),
        frozen(base),
        blocks_until_stopped,
    )
    .await
    .expect("the operator trip finalizes");
    assert_eq!(outcome.trip, Some(TripCause::DeadManOperator), "an absent keepalive is stale");
}

/// Fail-closed arming (KTD8 / ladder KTD9): a pre-registration missing the heartbeat
/// interval or the max-loss threshold cannot arm the envelope — the caller must refuse the
/// mount rather than run a half-envelope.
#[test]
fn the_envelope_refuses_to_arm_on_an_incomplete_pre_registration() {
    use nautilus_ls_lab::dispatch::prereg::PreRegistration;
    let missing_interval: PreRegistration =
        serde_json::from_value(serde_json::json!({ "version": 2, "session_max_loss_krw": 500000.0 }))
            .unwrap();
    assert!(WatchdogLimits::from_prereg(&missing_interval).is_err());

    let missing_threshold: PreRegistration =
        serde_json::from_value(serde_json::json!({ "version": 2, "heartbeat_interval_secs": 30 }))
            .unwrap();
    assert!(WatchdogLimits::from_prereg(&missing_threshold).is_err());

    let complete: PreRegistration = serde_json::from_value(serde_json::json!({
        "version": 2, "heartbeat_interval_secs": 30, "session_max_loss_krw": 500000.0
    }))
    .unwrap();
    let armed = WatchdogLimits::from_prereg(&complete).expect("a complete prereg arms");
    assert_eq!(armed.heartbeat_interval_secs, 30);
    assert_eq!(armed.max_loss_krw, 500_000.0);
}

/// A supervisor failure must never abandon a torn-down session. `execute_trip` runs the
/// teardown BEFORE it surfaces a chain-append error, so propagating that error would leave
/// a session that has already halted with **no run directory at all** — not even `.tmp-`
/// residue for the de-escalation scan to classify. Here the watchdog cannot even open the
/// chain (its dispatch dir is a regular file), and the session still runs and finalizes.
#[tokio::test]
async fn a_failed_watchdog_supervisor_still_finalizes_the_run() {
    let server = MockServer::start().await;
    let base = now_secs();
    mount_token(&server).await;
    let sdk = LsSdk::new(mock_config(&server.uri())).unwrap();
    let ledger: Arc<Mutex<FillLedger>> = Arc::new(Mutex::new(FillLedger::new()));
    let handles = LiveSessionHandles {
        session: LiveTeardownSession::new(
            EmissionGate::open(),
            sdk,
            Arc::clone(&ledger),
            OrderDispatchTasks::new(),
        ),
        heartbeats: Heartbeats::new(base),
        handle: LiveNodeHandle::new(),
        sink: DecisionSink::new(),
        marks: MarkFeed::new(),
    };
    mount_t0425(&server, serde_json::json!([])).await;
    mount_t0424_flat(&server).await;

    // Wedge the chain: `DispatchChain::open` create_dir_all's `<home>/dispatch`, which
    // fails when a regular file already occupies that path.
    let home = tempdir().unwrap();
    std::fs::write(home.path().join("dispatch"), b"not a directory").unwrap();
    let ka = keepalive(home.path());

    let outcome = run_live_session(
        handles.clone(),
        &driver_cfg(&ka),
        &ctx(home.path()),
        frozen(base),
        returns_immediately,
    )
    .await
    .expect("a dead supervisor is not a reason to abandon the session");

    assert!(outcome.run_dir.join(MANIFEST_FILE).exists(), "the run still finalized");
    assert!(!handles.session.orders_enabled(), "the teardown still halted");
    let dq = std::fs::read_to_string(outcome.run_dir.join(DATA_QUALITY_FILE)).unwrap();
    assert!(
        dq.contains("watchdog supervisor failed"),
        "the operator is told the envelope was down: {dq}"
    );
}
