//! U7 — the watchdog envelope (R6, R7, KD4, KD5; KTD10; AE2, AE3 record side).
//!
//! The software half of "attended": a dead-man heartbeat and a session max-loss breaker,
//! both routing into the ONE tested teardown seam ([`run_teardown`], stop → cancel →
//! flat-check → halt-last — reused, never re-ordered) and appending a durable safety-trip
//! record at trip time (KTD4).
//!
//! **Two heartbeat feeders** (KTD10): (a) a runtime atomic timestamp the session loop
//! touches on every processed event/tick (covers task/loop stalls), and (b) an operator
//! keepalive file whose mtime the attended operator refreshes (covers operator absence).
//! Either going stale beyond the pre-registered interval trips the envelope.
//!
//! **Independent runtime** (KTD10): the watchdog runs on its own OS thread with a
//! dedicated current-thread tokio runtime; teardown futures are driven exclusively there,
//! so a stalled session runtime cannot stall its own remediation.
//!
//! **Mutual liveness** (KTD10): supervision is two-way — the session loop treats
//! supervisor silence beyond the interval as its own trip condition, so a dead watchdog
//! thread never silently degrades the envelope to attended-operator-only.
//!
//! **Fail-closed arming** (U4): the interval and the loss threshold come from the
//! pre-registration values; a missing value refuses to arm (a missing heartbeat interval
//! blocks the mount, KTD9).
//!
//! Offline-tested by driving the clock (scripted observations), never by sleeping — the
//! live looping thread is thin glue over these pure/executable seams.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};

use crate::dispatch::chain::{DispatchChain, SafetyTripKind};
use crate::dispatch::prereg::PreRegistration;
use crate::runner::live::{record_safety_trip, run_teardown, LiveSession, TeardownReport};

/// Cancel/flat retry budgets for a watchdog-driven teardown (the live runner's shape).
const WATCHDOG_CANCEL_ATTEMPTS: usize = 3;
const WATCHDOG_FLAT_ATTEMPTS: usize = 3;

/// Which supervisor condition tripped — names the cause carried into the safety-trip
/// record (AE2/AE3) and selects the record kind (R14(d)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TripCause {
    /// The runtime heartbeat went stale (a task/loop stall).
    DeadManRuntime,
    /// The operator keepalive went stale (the operator stepped away).
    DeadManOperator,
    /// The session max-loss breaker crossed the pre-registered threshold.
    MaxLoss,
}

impl TripCause {
    /// The safety-trip record kind (a dead-man is a watchdog trip; the breaker its own).
    pub fn safety_kind(self) -> SafetyTripKind {
        match self {
            TripCause::DeadManRuntime | TripCause::DeadManOperator => SafetyTripKind::Watchdog,
            TripCause::MaxLoss => SafetyTripKind::Breaker,
        }
    }

    /// The scrubbed-at-write detail line recorded for this cause.
    pub fn detail(self) -> &'static str {
        match self {
            TripCause::DeadManRuntime => {
                "watchdog dead-man: runtime heartbeat stale (task/loop stall)"
            }
            TripCause::DeadManOperator => {
                "watchdog dead-man: operator keepalive stale (operator absent)"
            }
            TripCause::MaxLoss => {
                "session max-loss breaker: P&L basis crossed the pre-registered threshold"
            }
        }
    }
}

/// The pre-registered envelope thresholds (KTD9). Built fail-closed from the values file:
/// a missing interval or threshold refuses to arm.
#[derive(Debug, Clone, Copy)]
pub struct WatchdogLimits {
    /// The dead-man interval (seconds); a feeder stale beyond this trips.
    pub heartbeat_interval_secs: i64,
    /// The session max-loss threshold (KRW); a P&L basis at or below `-threshold` trips.
    pub max_loss_krw: f64,
}

impl WatchdogLimits {
    /// Build from the pre-registration values, fail-closed: a missing heartbeat interval
    /// or max-loss threshold is an error, so the envelope refuses to arm and the mount
    /// refuses (KTD9, U4 contract).
    ///
    /// # Errors
    ///
    /// If either the heartbeat interval or the max-loss threshold is not pre-registered.
    pub fn from_prereg(values: &PreRegistration) -> anyhow::Result<Self> {
        Ok(WatchdogLimits {
            heartbeat_interval_secs: values.heartbeat_interval_secs()? as i64,
            max_loss_krw: values.session_max_loss_krw()?,
        })
    }
}

/// The feeders + loss basis observed at one tick. Gathered by the live loop; pure to
/// evaluate so tests drive the clock rather than sleeping.
#[derive(Debug, Clone, Copy)]
pub struct WatchdogObservation {
    /// Wall-clock unix seconds at this tick.
    pub now_unix: i64,
    /// The runtime heartbeat timestamp (last session-loop touch).
    pub runtime_heartbeat_unix: i64,
    /// The operator keepalive timestamp (the keepalive file's mtime; `0` = absent = stale).
    pub operator_keepalive_unix: i64,
    /// Realized session P&L (KRW).
    pub realized_pnl_krw: f64,
    /// Open positions' P&L marked CONSERVATIVELY (approximated fills at the adverse edge),
    /// so the breaker never under-reports the loss (KTD10).
    pub open_marked_pnl_krw: f64,
}

/// The pure dead-man + breaker decision. The dead-man feeders are checked first (either
/// stale beyond the interval trips), then the max-loss basis (realized plus the
/// conservatively-marked open position). Returns the first trip cause, or `None` when the
/// envelope is healthy.
pub fn evaluate_trip(obs: &WatchdogObservation, limits: &WatchdogLimits) -> Option<TripCause> {
    if obs.now_unix - obs.runtime_heartbeat_unix > limits.heartbeat_interval_secs {
        return Some(TripCause::DeadManRuntime);
    }
    if obs.now_unix - obs.operator_keepalive_unix > limits.heartbeat_interval_secs {
        return Some(TripCause::DeadManOperator);
    }
    let basis = obs.realized_pnl_krw + obs.open_marked_pnl_krw;
    if basis <= -limits.max_loss_krw {
        return Some(TripCause::MaxLoss);
    }
    None
}

/// Mutual liveness (KTD10): the session loop treats supervisor silence beyond the interval
/// as its own trip condition, so a dead watchdog thread never silently degrades the
/// envelope to attended-only.
pub fn supervisor_silent(now_unix: i64, last_supervisor_touch_unix: i64, interval_secs: i64) -> bool {
    now_unix - last_supervisor_touch_unix > interval_secs
}

/// A one-shot trip latch (KTD10): the first claimant runs the teardown; racing conditions
/// (both feeders stale, or a feeder plus the breaker, or the session-side mutual-liveness
/// check plus the watchdog) resolve to exactly one teardown.
#[derive(Debug, Default)]
pub struct TripLatch(AtomicBool);

impl TripLatch {
    /// A fresh, un-tripped latch.
    pub fn new() -> Self {
        TripLatch(AtomicBool::new(false))
    }

    /// Claim the trip. Returns `true` exactly once; every later call returns `false`.
    pub fn try_claim(&self) -> bool {
        self.0
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Whether the latch has been claimed.
    pub fn is_tripped(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Execute a claimed trip (KTD4, KTD10). In strict order:
///
/// 1. persist the cause safety-trip record BEFORE any remediation — a fresh dispatch
///    process must observe the trip (the runtime kill switch is a per-process in-memory
///    `AtomicBool`);
/// 2. run the fail-closed teardown (the one tested seam; `halt` is inside, last);
/// 3. persist the kill-switch engagement the halt performed, so the gate's kill-switch
///    check reds until an explicit, nonce-gated clear (otherwise the check is a tautology).
///
/// The caller drives this on the WATCHDOG's own runtime, so a stalled session runtime
/// cannot stall its own remediation (KTD10). Returns the teardown report ([`run_teardown`]
/// never errors); only the two chain appends can fail.
///
/// # Errors
///
/// A chain-append failure on either safety-trip record.
pub async fn execute_trip<S: LiveSession>(
    session: &S,
    chain: &DispatchChain,
    cause: TripCause,
    run_id: Option<&str>,
    now: DateTime<Utc>,
    chain_rung: u8,
) -> anyhow::Result<TeardownReport> {
    // 1. Cause record at trip time, before remediation (KTD4).
    record_safety_trip(chain, cause.safety_kind(), run_id, cause.detail(), now, chain_rung)?;
    // 2. Fail-closed teardown (reuse; halt-last is inside).
    let report = run_teardown(session, WATCHDOG_CANCEL_ATTEMPTS, WATCHDOG_FLAT_ATTEMPTS).await;
    // 3. Persist the kill-switch engagement the halt performed (KTD4).
    record_safety_trip(
        chain,
        SafetyTripKind::KillSwitch,
        run_id,
        "kill switch engaged by watchdog teardown",
        now,
        chain_rung,
    )?;
    Ok(report)
}

/// One watchdog tick: evaluate the observation and, on a trip this tick has not yet
/// handled, claim the latch and execute the teardown on the caller's (watchdog) runtime.
/// Returns `Some(cause)` only when THIS tick executed the teardown; a trip already claimed
/// by an earlier tick or a racing feeder returns `None` (no second teardown). The live
/// loop calls this on its cadence with real observations; tests supply scripted ones.
///
/// # Errors
///
/// Propagates an [`execute_trip`] chain-append failure.
pub async fn watchdog_tick<S: LiveSession>(
    session: &S,
    chain: &DispatchChain,
    latch: &TripLatch,
    obs: &WatchdogObservation,
    limits: &WatchdogLimits,
    run_id: Option<&str>,
    chain_rung: u8,
) -> anyhow::Result<Option<TripCause>> {
    match evaluate_trip(obs, limits) {
        Some(cause) if latch.try_claim() => {
            let now = Utc.timestamp_opt(obs.now_unix, 0).single().unwrap_or_else(Utc::now);
            execute_trip(session, chain, cause, run_id, now, chain_rung).await?;
            Ok(Some(cause))
        }
        // A trip is present but was already claimed (a racing feeder / earlier tick) — the
        // teardown ran exactly once; do not run it again.
        Some(_) => Ok(None),
        None => Ok(None),
    }
}

/// Shared, cheaply-cloneable heartbeat feeders the session loop touches and the watchdog
/// reads (KTD10). Both timestamps start fresh so the envelope does not trip on arm.
#[derive(Debug, Clone)]
pub struct Heartbeats {
    runtime: Arc<AtomicI64>,
    supervisor: Arc<AtomicI64>,
}

impl Heartbeats {
    /// Arm both feeders at `now_unix`.
    pub fn new(now_unix: i64) -> Self {
        Heartbeats {
            runtime: Arc::new(AtomicI64::new(now_unix)),
            supervisor: Arc::new(AtomicI64::new(now_unix)),
        }
    }

    /// The session loop touches this on every processed event/tick (feeder a).
    pub fn touch_runtime(&self, now_unix: i64) {
        self.runtime.store(now_unix, Ordering::SeqCst);
    }

    /// The watchdog touches this each tick; the session loop reads it for mutual liveness.
    pub fn touch_supervisor(&self, now_unix: i64) {
        self.supervisor.store(now_unix, Ordering::SeqCst);
    }

    /// The last runtime-heartbeat timestamp.
    pub fn runtime_unix(&self) -> i64 {
        self.runtime.load(Ordering::SeqCst)
    }

    /// The last supervisor-touch timestamp.
    pub fn supervisor_unix(&self) -> i64 {
        self.supervisor.load(Ordering::SeqCst)
    }
}

/// The operator keepalive timestamp: the file's mtime as unix seconds, or `0` (absent /
/// unreadable) — which reads as stale, so a missing keepalive fails closed to a trip.
pub fn operator_keepalive_unix(path: &Path) -> i64 {
    use std::time::UNIX_EPOCH;
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Mutex;

    /// A **Sync** fake session (KTD10 requires the handle be Sync so it can move to the
    /// watchdog thread and be shared): records the teardown call order and simulates the
    /// still-resting / not-flat conditions with atomics + a Mutex-guarded log.
    #[derive(Default)]
    struct SyncFakeSession {
        log: Mutex<Vec<&'static str>>,
        cancel_ok: bool,
        flat: bool,
        cancel_calls: AtomicUsize,
    }

    impl LiveSession for SyncFakeSession {
        fn stop_emission(&self) {
            self.log.lock().unwrap().push("stop_emission");
        }
        async fn cancel_all_resting(&self) -> anyhow::Result<usize> {
            self.log.lock().unwrap().push("cancel");
            self.cancel_calls.fetch_add(1, Ordering::SeqCst);
            if self.cancel_ok {
                Ok(1)
            } else {
                anyhow::bail!("cancel failed")
            }
        }
        async fn is_flat(&self) -> bool {
            self.log.lock().unwrap().push("is_flat");
            self.flat
        }
        fn halt(&self) {
            self.log.lock().unwrap().push("halt");
        }
    }

    fn limits() -> WatchdogLimits {
        WatchdogLimits { heartbeat_interval_secs: 30, max_loss_krw: 500_000.0 }
    }

    fn healthy(now: i64) -> WatchdogObservation {
        WatchdogObservation {
            now_unix: now,
            runtime_heartbeat_unix: now,
            operator_keepalive_unix: now,
            realized_pnl_krw: 0.0,
            open_marked_pnl_krw: 0.0,
        }
    }

    #[test]
    fn healthy_feeders_never_trip_across_many_intervals() {
        let l = limits();
        for step in 0..1000 {
            // Both feeders touched each tick; P&L flat.
            let now = 1_752_600_000 + step;
            assert_eq!(evaluate_trip(&healthy(now), &l), None);
        }
    }

    #[test]
    fn stale_runtime_heartbeat_trips_dead_man() {
        let l = limits();
        let mut obs = healthy(1_752_600_100);
        obs.runtime_heartbeat_unix = 1_752_600_100 - 31; // > 30s stale
        assert_eq!(evaluate_trip(&obs, &l), Some(TripCause::DeadManRuntime));
    }

    #[test]
    fn stale_operator_keepalive_trips_dead_man() {
        let l = limits();
        let mut obs = healthy(1_752_600_100);
        obs.operator_keepalive_unix = 1_752_600_100 - 31; // fresh runtime, stale operator
        assert_eq!(evaluate_trip(&obs, &l), Some(TripCause::DeadManOperator));
    }

    #[test]
    fn max_loss_including_a_conservatively_marked_open_position_trips_the_breaker() {
        let l = limits();
        let mut obs = healthy(1_752_600_100);
        // Realized −200k plus an open position marked −350k at the adverse edge → −550k.
        obs.realized_pnl_krw = -200_000.0;
        obs.open_marked_pnl_krw = -350_000.0;
        assert_eq!(evaluate_trip(&obs, &l), Some(TripCause::MaxLoss));
        // Just above threshold stays healthy.
        obs.open_marked_pnl_krw = -299_000.0;
        assert_eq!(evaluate_trip(&obs, &l), None);
    }

    #[test]
    fn supervisor_silence_is_a_trip_condition() {
        assert!(!supervisor_silent(1000, 990, 30));
        assert!(supervisor_silent(1000, 960, 30), "silence beyond the interval trips");
    }

    #[test]
    fn from_prereg_fails_closed_on_a_missing_interval() {
        // A values file with no heartbeat interval refuses to arm (U4 contract).
        let values: PreRegistration =
            serde_json::from_value(serde_json::json!({ "version": 1, "session_max_loss_krw": 500000.0 })).unwrap();
        assert!(WatchdogLimits::from_prereg(&values).is_err());
        // With both present it arms.
        let values: PreRegistration = serde_json::from_value(serde_json::json!({
            "version": 1, "heartbeat_interval_secs": 30, "session_max_loss_krw": 500000.0
        }))
        .unwrap();
        let l = WatchdogLimits::from_prereg(&values).unwrap();
        assert_eq!(l.heartbeat_interval_secs, 30);
        assert_eq!(l.max_loss_krw, 500_000.0);
    }

    fn seed_chain(dir: &Path) -> DispatchChain {
        use crate::dispatch::chain::RecordKind;
        let chain = DispatchChain::open(dir).unwrap();
        let now = Utc.timestamp_opt(1_752_600_000, 0).unwrap();
        chain.append(now, 1, 1, None, RecordKind::Genesis).unwrap();
        chain
    }

    #[tokio::test]
    async fn a_dead_man_tick_tears_down_in_order_and_records_the_cause() {
        use crate::dispatch::chain::RecordKind;
        let tmp = tempfile::TempDir::new().unwrap();
        let chain = seed_chain(tmp.path());
        let session = SyncFakeSession { cancel_ok: true, flat: true, ..Default::default() };
        let latch = TripLatch::new();

        let mut obs = healthy(1_752_600_100);
        obs.runtime_heartbeat_unix = 1_752_600_100 - 40; // stale runtime
        let cause = watchdog_tick(&session, &chain, &latch, &obs, &limits(), Some("run-w"), 1)
            .await
            .unwrap();
        assert_eq!(cause, Some(TripCause::DeadManRuntime));

        // Teardown ran in order, halt last.
        let log = session.log.lock().unwrap();
        assert_eq!(log[0], "stop_emission");
        assert_eq!(*log.last().unwrap(), "halt");

        // A Watchdog cause record AND a KillSwitch engage record persisted (the gate reds
        // on the next dispatch).
        let state = chain.load();
        assert!(state.records.iter().any(|r| matches!(&r.body.kind,
            RecordKind::SafetyTrip(t) if t.trip == SafetyTripKind::Watchdog)));
        assert!(state.kill_switch_engaged, "kill switch persisted engaged after the trip");
    }

    #[tokio::test]
    async fn racing_trips_tear_down_exactly_once() {
        let tmp = tempfile::TempDir::new().unwrap();
        let chain = seed_chain(tmp.path());
        let session = SyncFakeSession { cancel_ok: true, flat: true, ..Default::default() };
        let latch = TripLatch::new();

        // Both feeders stale AND the breaker breached, evaluated twice: exactly one teardown.
        let obs = WatchdogObservation {
            now_unix: 1_752_600_100,
            runtime_heartbeat_unix: 1_752_600_100 - 40,
            operator_keepalive_unix: 1_752_600_100 - 40,
            realized_pnl_krw: -600_000.0,
            open_marked_pnl_krw: 0.0,
        };
        let first = watchdog_tick(&session, &chain, &latch, &obs, &limits(), None, 1).await.unwrap();
        let second = watchdog_tick(&session, &chain, &latch, &obs, &limits(), None, 1).await.unwrap();
        assert!(first.is_some(), "first tick handled the trip");
        assert_eq!(second, None, "second tick does not re-tear-down");
        assert_eq!(session.cancel_calls.load(Ordering::SeqCst), 1, "cancel attempted in exactly one teardown");
    }

    #[test]
    fn execute_trip_runs_on_a_dedicated_runtime_even_if_the_caller_thread_would_block() {
        // KTD10: the watchdog owns its runtime; teardown completes there independently of
        // the (here, entirely separate) thread. A stalled session runtime cannot stall it.
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().to_path_buf();
        let handle = std::thread::spawn(move || {
            let chain = seed_chain(&dir);
            let session = SyncFakeSession { cancel_ok: true, flat: true, ..Default::default() };
            // A dedicated current-thread runtime owned by this (watchdog) thread.
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            let report = rt
                .block_on(execute_trip(
                    &session,
                    &chain,
                    TripCause::DeadManRuntime,
                    Some("run-w"),
                    Utc.timestamp_opt(1_752_600_100, 0).unwrap(),
                    1,
                ))
                .unwrap();
            assert!(!report.hard_failed(), "teardown completed on the watchdog runtime");
            chain.load().kill_switch_engaged
        });
        assert!(handle.join().unwrap(), "the watchdog runtime completed the teardown + trip record");
    }

    #[test]
    fn operator_keepalive_absent_reads_as_stale() {
        let tmp = tempfile::TempDir::new().unwrap();
        // Absent file → 0 → stale (fail-closed).
        assert_eq!(operator_keepalive_unix(&tmp.path().join("absent.keepalive")), 0);
        // Present file → a positive mtime.
        let p = tmp.path().join("op.keepalive");
        std::fs::write(&p, b"alive").unwrap();
        assert!(operator_keepalive_unix(&p) > 0);
    }
}
