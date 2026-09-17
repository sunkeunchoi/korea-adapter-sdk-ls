//! Lane-neutral live-session machinery (U9's split of `live.rs`).
//!
//! Everything here is used identically by the ladder's `--mount` and the rehearsal's
//! `--rehearse-daily`: the fail-closed teardown, the watchdog envelope, the driver that
//! owns `node.run`'s lifecycle, the staging/finalize tail, and the mount exit codes both
//! lanes refuse with. Nothing here knows which lane it is running under — that difference
//! is carried entirely by [`SessionAuthority`](crate::runner::authority::SessionAuthority).
//!
//! Split out verbatim from the pre-U9 `live.rs`; the section comments below are the
//! original ones and still name the units that wrote them.


use std::path::{Path, PathBuf};

use ls_sdk::LsSdk;
use nautilus_ls::execution::OrderDispatchTasks;
use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_ls::orders::ledger::{FillDelta, FillLedger};
use nautilus_ls::orders::poll::DrivenOutcome;

use crate::artifacts::data_quality::{DataQualityReport, ReconcileCondition, ReconcileConditionKind};
use crate::artifacts::RunWriter;

// The pre-split file carried one flat import set; these are the entries the lane-neutral
// half still needs (the ladder half's live on `mount`).
use chrono::Utc;
use nautilus_live::node::{LiveNode, LiveNodeHandle};
use nautilus_ls::config::LsAdapterConfig;
use nautilus_ls::ingest::budget::{spend_ledger_path, SpendLedger};

use crate::agent::sink::DecisionSink;
use crate::dispatch::chain::{
    DispatchChain, RecordKind, SafetyTrip, SafetyTripKind, TripAction,
};
use crate::dispatch::nonce::OperatorGate;
use crate::runner::watchdog::Heartbeats;
// The ladder's catalog-range helper stays with the dispatch gate that owns it; the
// staging tail is the one lane-neutral caller.
use super::mount::session_range_in_catalog;
use crate::params::OrbParams;
use crate::strategy::orb::EmissionGate;

/// Live run configuration.
#[derive(Debug, Clone)]
pub struct LiveConfig {
    /// The data home (registry + catalog live here).
    pub data_home: PathBuf,
    /// The ORB parameter set.
    pub params: OrbParams,
    /// The operator-resolved universe (live: from a t8407/daily read; the offline
    /// tests pass symbols directly).
    pub symbols: Vec<String>,
    /// Session duration (seconds) before an automatic time-flat teardown.
    pub session_secs: u64,
    /// Starting account balance (KRW) recorded for the equity curve.
    pub starting_balance: f64,
}

/// The fail-closed teardown seam (KTD7). Abstracted so the ordering invariant is
/// unit-testable with a fake; the live impl is backed by the exec client + SDK.
pub trait LiveSession {
    /// Stop the strategy's order emission (close its [`EmissionGate`]) — called
    /// FIRST so no new order races the cancels.
    fn stop_emission(&self);
    /// Cancel every resting order. Returns the count canceled; errors if a cancel
    /// could not be confirmed.
    fn cancel_all_resting(&self) -> impl std::future::Future<Output = anyhow::Result<usize>>;
    /// A quantity-keyed flatness check — POSITIVE confirmation only (a truncated or
    /// failed read returns `false`, never a false "flat").
    fn is_flat(&self) -> impl std::future::Future<Output = bool>;
    /// Engage the kill switch (blocks any further order placement). Called only
    /// AFTER the closing cancels.
    fn halt(&self);
}

/// The **production** [`LiveSession`] — the concrete, `Send + Sync` teardown handle the
/// live driver and the watchdog share (live-session-driver U1; R1, KTD1/KTD2/KTD4/KTD6).
///
/// Deliberately **not** an [`LsExecClient`](nautilus_ls::execution::LsExecClient): that
/// type is not `Clone` (it owns `JoinHandle`s) and cannot be retrieved after
/// `LiveNode::build()` type-erases it, and its `Send + Sync` would ride on the
/// `ExecutionEventEmitter`/`WsSupervisor` bounds. Instead every field here is
/// `Arc`-shared state captured **before** the builder:
///
/// - `gate` — a clone of the strategy's [`EmissionGate`] (`Arc<AtomicBool>`), taken before
///   `add_strategy` moves the strategy into the trader;
/// - `sdk` — a clone of the **node's** [`LsSdk`], so `halt()` flips the very
///   `Arc<Inner>::orders_enabled` the node's order path checks (KTD3: a separately-built
///   client would halt a *different* `AtomicBool` — a silent no-op on exactly the orders
///   that matter);
/// - `ledger` — the **node's** `Arc<Mutex<FillLedger>>` (a separate `Arc` from the SDK, so
///   `sdk.clone()` does not carry it), which the max-loss breaker feeder reads;
/// - `order_tasks` — the node client's retained order-dispatch tasks, quiesced before the
///   cancel scan (KTD2).
///
/// Ordering is **not** this type's job: [`run_teardown`] owns `stop → cancel → flat →
/// halt` and is reused unchanged.
/// (No `Debug`: neither `LsSdk` nor `FillLedger` implements it, and a derived one would
/// risk printing resolved credential state anyway — this handle is never logged.)
#[derive(Clone)]
pub struct LiveTeardownSession {
    gate: EmissionGate,
    sdk: LsSdk,
    ledger: std::sync::Arc<std::sync::Mutex<FillLedger>>,
    order_tasks: OrderDispatchTasks,
    quiesce_budget: std::time::Duration,
    /// Set on the REHEARSAL lane (U9): the safe end state is a matching BOOK, not an empty
    /// account. `None` keeps the ladder's flat assertion exactly as it was.
    book_intent: Option<BookIntent>,
}

/// The rehearsal teardown's book assertion and the snapshot it took (U9, KTD6/KTD11).
///
/// A ladder session's safe end state is flat, so its teardown asserts flatness. A rehearsal
/// deliberately ends holding a multi-session book, so asserting flatness there would fail
/// every healthy session. What replaces it is the same predicate with a different
/// expectation — U7 unified the two, so `flat` is simply the empty-book case — evaluated
/// against the intent [`intended_book`](crate::runner::pnl::intended_book) derives from the
/// shared fill ledger.
///
/// It carries the observed snapshot back out because the two must come from ONE t0424 read
/// (see [`verify_book_capturing`](nautilus_ls::execution::verify_book_capturing)): the book
/// the next session inherits is written from the broker's view, and a second read could
/// hand back a book this teardown never approved.
#[derive(Clone, Default)]
pub struct BookIntent {
    observed: Arc<std::sync::Mutex<Option<std::collections::BTreeMap<String, i64>>>>,
}

impl BookIntent {
    /// A fresh, unobserved intent.
    #[must_use]
    pub fn new() -> Self {
        BookIntent::default()
    }

    /// The holdings the teardown's book check actually saw, once one has succeeded.
    /// `None` when no check confirmed — which is exactly when the session is ABNORMAL and
    /// `book.json` must NOT be rewritten (KTD11: the book is a record of broker
    /// confirmation, so an unconfirmed session leaves the previous one standing).
    #[must_use]
    pub fn observed(&self) -> Option<std::collections::BTreeMap<String, i64>> {
        self.observed.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn record(&self, snapshot: std::collections::BTreeMap<String, i64>) {
        *self.observed.lock().unwrap_or_else(|e| e.into_inner()) = Some(snapshot);
    }
}

impl LiveTeardownSession {
    /// Build the teardown handle from the pieces captured before `LiveNode::build()`.
    pub fn new(
        gate: EmissionGate,
        sdk: LsSdk,
        ledger: std::sync::Arc<std::sync::Mutex<FillLedger>>,
        order_tasks: OrderDispatchTasks,
    ) -> Self {
        LiveTeardownSession {
            gate,
            sdk,
            ledger,
            order_tasks,
            quiesce_budget: nautilus_ls::execution::QUIESCE_BUDGET,
            book_intent: None,
        }
    }

    /// Assert a matching BOOK rather than a flat account at teardown (U9, the rehearsal
    /// lane). The handed-back [`BookIntent`] is how the caller reads the snapshot the check
    /// took.
    #[must_use]
    pub fn with_book_intent(mut self, intent: BookIntent) -> Self {
        self.book_intent = Some(intent);
        self
    }

    /// Override the in-flight order-dispatch drain budget (tests drive it to zero so the
    /// quiesce path is exercised without waiting).
    pub fn with_quiesce_budget(mut self, budget: std::time::Duration) -> Self {
        self.quiesce_budget = budget;
        self
    }

    /// The shared fill ledger — what the max-loss breaker feeder reads (KTD3).
    pub fn ledger(&self) -> std::sync::Arc<std::sync::Mutex<FillLedger>> {
        std::sync::Arc::clone(&self.ledger)
    }

    /// A clone of the mounted strategy's emission gate — the same `Arc<AtomicBool>` the
    /// strategy reads before every order it would emit (KTD4).
    pub fn emission_gate(&self) -> EmissionGate {
        self.gate.clone()
    }

    /// The lifetime order-dedup hit count on the shared SDK — a within-TTL identical
    /// re-send or a concurrent-duplicate rejection. A non-zero count on a real emission is
    /// a limit event (ladder R14(d)); the run's data-quality report persists it.
    pub fn dedup_hits(&self) -> u64 {
        self.sdk.inner().order_dedup.hit_count()
    }

    /// Whether the shared kill switch still permits order dispatch. `false` once
    /// [`LiveSession::halt`] has run — read by the wiring tests that prove the node's
    /// in-trader client shares this switch.
    pub fn orders_enabled(&self) -> bool {
        self.sdk.inner().orders_enabled()
    }
}

impl LiveSession for LiveTeardownSession {
    fn stop_emission(&self) {
        self.gate.stop();
    }

    async fn cancel_all_resting(&self) -> anyhow::Result<usize> {
        // KTD2: drain the detached submit/modify/cancel workers FIRST. `stop_emission`
        // only closes the strategy's gate — a submission already in flight would
        // otherwise reach the gateway after the scan below, pass the kill-switch check,
        // and rest an order the scan never saw.
        self.order_tasks.quiesce(self.quiesce_budget).await;
        nautilus_ls::execution::cancel_all_resting_on(&self.sdk)
            .await
            // `LsError` Display carries the broker's `rsp_msg`; scrub before it can reach
            // any record or output line.
            .map_err(|e| anyhow::anyhow!("{}", nautilus_ls::scrub::scrub_secrets(&e.to_string())))
    }

    async fn is_flat(&self) -> bool {
        // KTD1: positive confirmation only. `verify_flat_on` composes t0425 (resting
        // orders) + t0424 (`janqty` holdings), failing closed on truncation/garbage — so a
        // truncated, failed, or ambiguous read is `Err` and reads here as NOT flat.
        let Some(intent) = self.book_intent.as_ref() else {
            return nautilus_ls::execution::verify_flat_on(&self.sdk).await.is_ok();
        };
        // U9: the rehearsal's safe end state. The SAME predicate against a non-empty
        // expectation, with the same positive-confirmation-only reading — an unreadable or
        // truncated t0424 is `Err` and lands here as "the book is NOT confirmed", which
        // finalizes the session ABNORMAL rather than writing a book nobody verified.
        let expected = nautilus_ls::execution::ExpectedBook::from_pairs(
            crate::runner::pnl::intended_book(&self.ledger),
        );
        match nautilus_ls::execution::verify_book_capturing(&self.sdk, &expected).await {
            Ok(snapshot) => {
                intent.record(snapshot);
                true
            }
            Err(_) => false,
        }
    }

    fn halt(&self) {
        // The shared `Arc<Inner>` kill switch — the same `AtomicBool` `post_order` checks
        // first for every order the node dispatches (KTD3).
        self.sdk.inner().set_orders_enabled(false);
    }
}

/// The outcome of a fail-closed teardown (KTD7, R5). Carries the cancel-attempt count so
/// finalize can persist the retry metric (R14(d)) even when the teardown hard-failed —
/// [`run_teardown`] never errors, so the caller always gets the report and can finalize
/// abnormally before bailing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeardownReport {
    /// Number of cancel calls made (1..=`cancel_attempts`).
    pub cancel_attempts: u64,
    /// Whether cancels were confirmed.
    pub canceled: bool,
    /// Whether flatness was POSITIVELY confirmed.
    pub flat_confirmed: bool,
}

impl TeardownReport {
    /// Retries beyond the first cancel attempt (0 if the first succeeded). More than one
    /// retry is a limit event (R14(d)).
    pub fn retries(&self) -> u64 {
        self.cancel_attempts.saturating_sub(1)
    }

    /// Whether teardown could not positively confirm a flat account — the account must
    /// be treated as NOT flat (never conclude flat on ambiguity).
    pub fn hard_failed(&self) -> bool {
        !self.canceled || !self.flat_confirmed
    }
}

/// Run the fail-closed session teardown (KTD7). The ordering is the safety property:
/// stop emission → cancel resting (retried) → confirm flat (retried, positive-only) →
/// **halt after**. Returns a [`TeardownReport`] rather than erroring: a hard-failed
/// teardown must still leave scannable artifacts (R5), so the caller finalizes on the
/// report and bails afterward.
pub async fn run_teardown<S: LiveSession>(
    session: &S,
    cancel_attempts: usize,
    flat_attempts: usize,
) -> TeardownReport {
    // 1. Stop the strategy's order emission first.
    session.stop_emission();

    // 2. Cancel all resting orders, retrying. Count the attempts made (R14(d)).
    let mut canceled = false;
    let mut attempts = 0u64;
    for _ in 0..cancel_attempts.max(1) {
        attempts += 1;
        if session.cancel_all_resting().await.is_ok() {
            canceled = true;
            break;
        }
    }

    // 3. Quantity-keyed flatness check (positive confirmation only).
    let mut flat = false;
    for _ in 0..flat_attempts.max(1) {
        if session.is_flat().await {
            flat = true;
            break;
        }
    }

    // 4. Engage the kill switch AFTER the closing cancels — always, even on failure.
    session.halt();

    TeardownReport { cancel_attempts: attempts, canceled, flat_confirmed: flat }
}

/// Finalize a run's artifacts — ALWAYS, even after a hard-failed teardown (R5): a
/// session that carries limit events must still leave scannable artifacts. Stamps the
/// teardown retry count and dedup-hit count into the data-quality report (the fields
/// U9's exceedance scan reads, R10/R14(d)) and marks the run abnormal when teardown
/// could not confirm flat. Assumes the manifest/performance/decisions are already staged
/// into the writer's tmp dir. Consumes the writer.
pub fn finalize_session(
    writer: RunWriter,
    mut dq: DataQualityReport,
    report: &TeardownReport,
    dedup_hits: u64,
) -> anyhow::Result<PathBuf> {
    dq.teardown_retries = Some(report.retries());
    dq.dedup_hits = Some(dedup_hits);
    if report.hard_failed() {
        // The observation is scrubbed at write time; it is a fixed literal here.
        dq.observations.push(
            "ABNORMAL: teardown could not positively confirm a flat account — kill switch \
             engaged; operator must reconcile"
                .to_string(),
        );
    }
    writer.write_data_quality(&dq)?;
    writer.finalize()
}

/// Persist a safety-trip record to the dispatch chain at trip time (KTD4) — call this
/// BEFORE any finalize/bail. The runtime kill switch is a per-process in-memory
/// `AtomicBool`, so a fresh dispatch process would otherwise always read it disengaged
/// and the R1 kill-switch check would be a tautology; the persisted record is what the
/// gate reads.
///
/// Thin over [`ChainTripSink`] (U8/KTD3), which is now the one writer of a chain
/// safety-trip record: the watchdog reaches the same append through the `TripSink` seam, so
/// the chain and rehearsal lanes cannot drift on what a trip record contains.
pub fn record_safety_trip(
    chain: &DispatchChain,
    kind: SafetyTripKind,
    run_id: Option<&str>,
    detail: &str,
    now: chrono::DateTime<Utc>,
    chain_rung: u8,
) -> anyhow::Result<()> {
    crate::runner::watchdog::ChainTripSink::new(chain.clone(), chain_rung).record_trip(
        kind,
        TripAction::Engage,
        run_id,
        detail,
        now,
    )
}

/// Clear a persisted kill-switch trip — an explicit, nonce-gated operator action
/// recorded in the chain (KTD4). Re-enabling live dispatch after a safety trip is at
/// least as consequential as a deferral, so it is behind the same fresh-nonce +
/// no-TTY loud-refusal gate.
///
/// # Errors
///
/// Refuses (loudly) without a fresh operator nonce in an attended context; propagates a
/// chain-append failure.
pub fn clear_kill_switch(
    chain: &DispatchChain,
    gate: &OperatorGate,
    reason: &str,
    now: chrono::DateTime<Utc>,
    chain_rung: u8,
) -> anyhow::Result<()> {
    gate.authorize("kill-switch clear").map_err(|e| anyhow::anyhow!(e))?;
    // Scrub the operator reason before it lands (KTD4 clear-reason capture) — clearing an
    // auto-halt kill switch is the CLI's most safety-sensitive mutation and must leave an
    // audited who/why record with no secret in it (mirrors `chain.reregister`'s reason scrub).
    chain.append(
        now,
        chain_rung,
        chain_rung,
        None,
        RecordKind::SafetyTrip(SafetyTrip {
            trip: SafetyTripKind::KillSwitch,
            action: TripAction::Clear,
            run_id: None,
            detail: nautilus_ls::scrub::scrub_secrets(reason),
        }),
    )?;
    Ok(())
}

/// Acquire the live-session advisory lock on the catalog (KTD7). Refuses (errors) if
/// the ingest lock is held — a backfill and a live session cannot run concurrently.
/// Held for the session; released on drop.
pub fn live_guard(data_home: &Path) -> anyhow::Result<AdvisoryLock> {
    let catalog = data_home.join("catalog");
    std::fs::create_dir_all(&catalog)?;
    AdvisoryLock::acquire(&catalog, LockKind::Live)
        .map_err(|e| anyhow::anyhow!("live session refused — ingest in progress: {e}"))
}

/// Count the fills emitted at an approximated price (KTD4/R14) — the limit-price
/// fallbacks plus beyond-first poll partials. Feeds `data_quality.price_approximated_fills`.
pub fn count_approximated(deltas: &[FillDelta]) -> u64 {
    deltas.iter().filter(|d| d.price_approximated).count() as u64
}

/// Record a DRIVEN poll pass's reconcile-advised condition into the data-quality
/// report (R7/R9, AE3/AE4). The drive already self-heals transient
/// inconclusiveness with bounded re-polls, so only an **exhausted** drive — still
/// inconclusive after its budget — reaches the report. The drive collapses its
/// specific inconclusive reasons (truncation, unresolved row, cumulative
/// regression, request failure) into one terminal state, so this records the
/// honest [`ReconcileConditionKind::PollInconclusive`] rather than mislabeling a
/// specific cause — the agent treats the run's accounting as suspect either way.
pub fn record_reconcile(dq: &mut DataQualityReport, outcome: &DrivenOutcome, symbol: &str) {
    if outcome.exhausted() {
        dq.reconcile_advised.push(ReconcileCondition {
            kind: ReconcileConditionKind::PollInconclusive,
            symbol: symbol.to_string(),
        });
    }
}


/// Serializes `LiveNode::build()` across threads. Nautilus initializes the process-global
/// logger with a non-atomic check-then-set, so two concurrent builds intermittently trip
/// "a non-Nautilus logger is already registered"
/// (`docs/solutions/test-failures/nautilus-livenode-tests-race-on-the-global-logger-init.md`).
/// Poison-tolerant: a panicking build must not wedge every later one.
static NODE_BUILD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Hold the [`NODE_BUILD_LOCK`] across a `LiveNode::build()`. Public so the wiring tests
/// that build their own nodes serialize against the runner's builds.
pub fn node_build_lock() -> std::sync::MutexGuard<'static, ()> {
    NODE_BUILD_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A built live session: the node plus every handle the driver, the watchdog, and the
/// finalize path need — all captured **before** the builder (live-session-driver KTD4).
///
/// After `LiveNode::build()` the exec client is type-erased in `Vec<LiveExecutionClient>`
/// with no downcast, and `add_strategy` moves the strategy into the trader. Neither handle
/// can be retrieved afterwards, so they are taken here or not at all.
pub struct LiveMount {
    /// The built node. `node.run()` is live-only and is never driven by the gate — it is
    /// deliberately kept OUT of [`LiveSessionHandles`] so the driver can be exercised
    /// offline without one.
    pub node: LiveNode,
    /// Everything the driver, the watchdog, and the finalize path need.
    pub handles: LiveSessionHandles,
}

/// The handle set a live session is driven through — captured before the builder (KTD4)
/// and deliberately node-free, so [`run_live_session`] is fully offline-testable.
#[derive(Clone)]
pub struct LiveSessionHandles {
    /// The fail-closed teardown handle, sharing the node's kill switch + fill ledger.
    pub session: LiveTeardownSession,
    /// The dead-man feeders: the strategy touches `runtime`, the watchdog `supervisor`.
    pub heartbeats: Heartbeats,
    /// The node's stop handle, grabbed BEFORE `run` (the session timer and a watchdog
    /// trip both use it to unblock the run loop).
    pub handle: LiveNodeHandle,
    /// The session's decision sink — drained into the run artifacts at finalize.
    pub sink: DecisionSink,
    /// The strategy's published per-symbol market view — the breaker's mark source
    /// (KTD8(b)); the watchdog thread has no market-data access of its own.
    pub marks: MarkFeed,
}

// ---------------------------------------------------------------------------
// live-session-driver U3/U4 — the driver that owns `node.run`'s lifecycle (R4, R5;
// KTD5, KTD7, KTD8).
//
// `node.run(&mut self)` blocks the current thread and runs INDEFINITELY: it has no
// session timer and no market-close stop. The caller owns the stop. So the driver:
//
//   1. grabs `node.handle()` BEFORE `run` (KTD4/KTD5 — after `run` there is no way in);
//   2. spawns a session timer that calls `handle.stop()` at the session duration;
//   3. spins the full watchdog envelope on a DEDICATED OS thread with its own
//      current-thread runtime, so a stalled session runtime cannot stall its own
//      remediation (ladder KTD10), and runs the session-side mutual-liveness check on
//      the node runtime so a dead watchdog thread never degrades the envelope silently;
//   4. drives `node.run()` — the single seam never exercised offline, injected as a
//      substitutable closure so every surrounding seam IS — under a STOP-RELATIVE hard-stop
//      deadline, because `handle.stop()` is a request the node may ignore. Without the
//      deadline a node wedged on stop blocks this await forever and steps (5) and (6) are
//      unreachable: the session leaves only `.tmp-` residue and a consumed dispatch;
//   5. runs the fail-closed `run_teardown` afterwards **only if it wins the atomic
//      `TripLatch::try_claim`** — the same compare-exchange the watchdog uses. A
//      non-atomic `is_tripped()` read would race a concurrent watchdog claim and let
//      BOTH paths tear down;
//   6. stages the manifest (with the DispatchLink), the performance from the shared fill
//      ledger, and the drained decisions, then finalizes — abnormally when the teardown
//      hard-failed.
//
// The teardown runs AFTER `node.run`'s own graceful shutdown by design (KTD7): the node
// cancels and drains on stop, but that is not the sticky-kill-switch + positive
// t0424/t0425 confirmation the gate requires. Re-asserting the safety invariant at the
// driver altitude is the point.
// ---------------------------------------------------------------------------

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::artifacts::manifest::Manifest;
// U8's session authority + manifest identity live in a sibling module (see its header) and
// are re-exported here, so every existing `runner::live::…` path and U9's call sites read
// them from the driver they belong to.
pub use crate::runner::authority::{LiveManifestParts, SessionAuthority, SessionIdentity};
use crate::runner::pnl::{self, MarkPolicy};
use crate::runner::watchdog::{
    operator_keepalive_unix, session_liveness_tick_reporting, watchdog_tick_reporting,
    TripCause, TripLatch, TripSink, WatchdogLimits, WatchdogObservation,
};
// Through `strategy::hooks`, not `strategy::orb` (U8): the mark feed is a SHARED live hook,
// and the daily path now reaches it too. Importing it from ORB's module would say the
// driver's breaker input belongs to one strategy, which is the reading `hooks` exists to
// correct — the implementation stays where it is so ORB's identity-bearing source does not
// move (KTD5).
use crate::strategy::hooks::MarkFeed;

/// An injectable wall clock (unix seconds) — tests drive it rather than sleeping.
pub type SessionClock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// The real wall clock.
pub fn system_clock() -> SessionClock {
    Arc::new(|| Utc::now().timestamp())
}

/// The driver's tunables. Everything safety-bearing (`limits`) comes from the frozen
/// pre-registration; the cadences are operational.
#[derive(Debug, Clone)]
pub struct LiveDriverConfig {
    /// Session duration before the timer stops the node.
    pub session_secs: u64,
    /// How long the node may take to return from `run` **after a stop has been requested**
    /// — by the timer, the watchdog, or the mutual-liveness loop. STOP-RELATIVE by
    /// construction (see [`stop_requested_then_grace`]): a session-relative deadline would
    /// leave a mid-session trip blocked for the rest of the session. Keep it under
    /// `limits.heartbeat_interval_secs` so a node hung on stop is hard-stopped before the
    /// dead-man trips on the stalled drain — otherwise the two failure modes race and an
    /// operationally-recoverable stall costs a nonce-gated `--clear-killswitch`.
    pub stop_grace: Duration,
    /// Watchdog evaluation cadence. Well under the heartbeat interval so a stale feeder
    /// is caught promptly.
    pub watchdog_tick: Duration,
    /// The pre-registered dead-man interval + max-loss threshold (fail-closed armed).
    pub limits: WatchdogLimits,
    /// How the breaker marks open positions when the feed is stale (KTD8(b)).
    pub mark_policy: MarkPolicy,
    /// The operator keepalive file the attended operator refreshes (absent = stale).
    pub keepalive_path: PathBuf,
    /// Cancel/flat retry budgets for the session-end teardown.
    pub cancel_attempts: usize,
    /// Flat-confirmation attempts for the session-end teardown.
    pub flat_attempts: usize,
    /// Starting account balance (KRW) recorded on the equity curve.
    pub starting_balance: f64,
    /// The inherited book's stop prices, symbol → stop (U9, KTD13). EMPTY on the ladder,
    /// which is not a special case: a ladder session opens every position it holds, so every
    /// one of them has a published mark and there is nothing for a floor to cover. A
    /// rehearsal's restored leg may not print at all — a symbol halted the whole session
    /// never reaches the strategy's `on_bar` — and the breaker would then mark it at the
    /// configured worst case rather than at the bound the book already knows.
    pub book_stop_floors: std::collections::HashMap<String, i64>,
}

/// Typed rows a session's runner contributes to its own data-quality report (U9).
///
/// The day loop and the finalize path are on opposite sides of [`run_live_session`]: the
/// loop runs as a task on the node's runtime and is gone by the time the artifacts are
/// staged. A shared handle is how what it observed survives that — the same shared-handle
/// shape [`DecisionSink`] and [`MarkFeed`] already use, and for the same reason.
///
/// Empty on the ladder, which writes none of these row types.
#[derive(Clone, Default)]
pub struct SessionObservations {
    inner: Arc<std::sync::Mutex<ObservationState>>,
}

#[derive(Default)]
struct ObservationState {
    gaps: Vec<crate::artifacts::data_quality::HeldSymbolGap>,
    divergences: Vec<crate::artifacts::data_quality::RehearsalDivergence>,
    notes: Vec<String>,
}

impl std::fmt::Debug for SessionObservations {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let st = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        f.debug_struct("SessionObservations")
            .field("gaps", &st.gaps.len())
            .field("divergences", &st.divergences.len())
            .field("notes", &st.notes.len())
            .finish()
    }
}

impl SessionObservations {
    /// A fresh, empty handle.
    #[must_use]
    pub fn new() -> Self {
        SessionObservations::default()
    }

    /// Record a held symbol that received no usable bar (KTD12).
    pub fn gap(&self, row: crate::artifacts::data_quality::HeldSymbolGap) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).gaps.push(row);
    }

    /// Record a measured backtest-vs-live divergence (KTD4).
    pub fn divergence(&self, row: crate::artifacts::data_quality::RehearsalDivergence) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).divergences.push(row);
    }

    /// Record a free-text observation.
    pub fn note(&self, note: String) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).notes.push(note);
    }

    /// Everything recorded so far, left in place — the finalize path reads it, and a
    /// draining read would make a second call (a retry, a test assertion) see nothing.
    #[must_use]
    pub fn snapshot(
        &self,
    ) -> (
        Vec<crate::artifacts::data_quality::HeldSymbolGap>,
        Vec<crate::artifacts::data_quality::RehearsalDivergence>,
        Vec<String>,
    ) {
        let st = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        (st.gaps.clone(), st.divergences.clone(), st.notes.clone())
    }
}


/// The identity + artifact context a driven session finalizes under.
#[derive(Debug, Clone)]
pub struct LiveSessionContext {
    /// The data home (registry lives here; the chain does too on a ladder session).
    pub data_home: PathBuf,
    /// Who authorized this session, and where its trips are recorded (KTD3).
    pub authority: SessionAuthority,
    /// The run manifest, built at mount time by [`SessionIdentity::live_manifest`] and
    /// written verbatim at finalize. Pre-made because the finalize path may not fail.
    pub manifest: Manifest,
    /// The traded universe (instrument-id strings).
    pub symbols: Vec<String>,
    /// The KST trading date the run covers (`YYYYMMDD`).
    pub trading_date: String,
    /// Typed rows the session's own runner recorded (U9). Empty on the ladder.
    pub observations: SessionObservations,
    /// The book this session inherited, as it stood at mount time (U12). `None` on the
    /// ladder, which has no book.
    ///
    /// Captured into the run because the live `rehearsal/book.json` is rewritten by the
    /// teardown and keeps only still-held legs: the leg an exit closed has already been
    /// dropped from it when a report runs, taking its `entered_under` with it.
    pub inherited_book: Option<crate::runner::live_daily::RehearsalBook>,
    /// The D+2 deposit the rehearsal's pre-mount probe read (integer KRW), mirrored onto the
    /// data-quality report as `pre_mount_deposit_krw`. `None` on the ladder, which gates on
    /// its own preflight and records no book.
    pub deposit_krw: Option<i64>,
}


/// What a driven session finalized as.
#[derive(Debug, Clone)]
pub struct LiveSessionOutcome {
    /// The one fail-closed teardown's report (whichever path claimed it).
    pub report: TeardownReport,
    /// The watchdog/mutual-liveness cause, when a trip (not the session timer) ended it.
    pub trip: Option<TripCause>,
    /// The finalized run directory.
    pub run_dir: PathBuf,
    /// Whether the run finalized ABNORMAL — either the teardown could not positively
    /// confirm a flat account, or the node had to be hard-stopped. Either way the kill
    /// switch is engaged and an operator must reconcile.
    pub abnormal: bool,
    /// Whether the node ignored its stop request and was abandoned at the hard-stop
    /// deadline. Distinguishes the two ABNORMAL causes, so the operator is told the right
    /// one — a hard-stop can leave a CONFIRMED-flat account.
    pub hard_stopped: bool,
}

/// Assemble one watchdog observation from the live feeders (R5; KTD8). Pure given its
/// inputs, so the breaker's arithmetic is provable offline against scripted fixtures.
pub fn assemble_observation(
    now_unix: i64,
    heartbeats: &Heartbeats,
    keepalive_path: &Path,
    ledger: &std::sync::Mutex<FillLedger>,
    marks: &MarkFeed,
    policy: &MarkPolicy,
) -> WatchdogObservation {
    assemble_observation_with_floors(
        now_unix,
        heartbeats,
        keepalive_path,
        ledger,
        marks,
        policy,
        &std::collections::HashMap::new(),
    )
}

/// [`assemble_observation`] with the inherited book's stops available as [`MarkPolicy`]
/// floors (U9, KTD13).
///
/// An EMPTY floor map is exactly the ladder's arithmetic — `rehearsal_breaker_basis` with no
/// floors reduces to `mark_open_pnl` — which is why the ladder keeps the simpler entry point
/// above rather than being routed through a rehearsal-shaped one.
///
/// The floors are a FALLBACK, never a co-bound with a fresh mark: folding a stop in beside a
/// fresh close would mark every inherited leg at its stop for the whole session, and the
/// breaker would read a stop-loss-sized drawdown on a position that never moved.
#[allow(clippy::too_many_arguments)]
pub fn assemble_observation_with_floors(
    now_unix: i64,
    heartbeats: &Heartbeats,
    keepalive_path: &Path,
    ledger: &std::sync::Mutex<FillLedger>,
    marks: &MarkFeed,
    policy: &MarkPolicy,
    floors: &std::collections::HashMap<String, i64>,
) -> WatchdogObservation {
    // KTD8(a): realized P&L is ACCOUNTING over the shared ledger's fill journal, not a
    // sum — the ledger carries no cost basis. KTD8(b): open positions are marked at the
    // adverse edge with a stale-feed floor, never a last-seen favorable price.
    let session = pnl::account_shared(ledger);
    let (_, open_marked) =
        pnl::rehearsal_breaker_basis(&session, &marks.snapshot(), floors, now_unix, policy);
    WatchdogObservation {
        now_unix,
        runtime_heartbeat_unix: heartbeats.runtime_unix(),
        operator_keepalive_unix: operator_keepalive_unix(keepalive_path),
        realized_pnl_krw: session.realized_krw,
        open_marked_pnl_krw: open_marked,
    }
}

/// The stop request, latched on the DRIVER side.
///
/// `LiveNodeHandle::stop()` alone is not a durable record of "someone asked the node to
/// stop": nautilus **clears** `stop_flag` on every transition to `Running`
/// (`LiveNodeHandle::set_state`), and `LiveNode::run` makes that transition *after* client
/// connection and reconciliation. So a stop requested during node startup — exactly when a
/// wedged gateway makes the dead-man fire — is erased. A backstop that armed by polling
/// that flag could miss the transient entirely and never arm, restoring the very block it
/// exists to close.
///
/// This latch is set once and never cleared, so the hard-stop deadline arms on the
/// **request**, not on a flag the node owns.
#[derive(Clone)]
pub(crate) struct StopRequest {
    latch: Arc<AtomicBool>,
}

impl StopRequest {
    pub(crate) fn new() -> Self {
        StopRequest { latch: Arc::new(AtomicBool::new(false)) }
    }

    /// Ask the node to stop AND record that we asked. Every stop requester — the session
    /// timer, the watchdog thread, the mutual-liveness loop — goes through here rather than
    /// calling `handle.stop()` directly; keeping the pair in one place is what stops a
    /// future requester from silently un-arming the backstop. The latch is stored FIRST so
    /// an observer that sees the node's flag has already seen the latch.
    pub(crate) fn request(&self, handle: &LiveNodeHandle) {
        self.latch.store(true, Ordering::SeqCst);
        handle.stop();
    }

    pub(crate) fn requested(&self) -> bool {
        self.latch.load(Ordering::SeqCst)
    }
}

/// Everything the watchdog OS thread owns. All `Arc`-shared or owned outright, so the
/// thread needs nothing from the session runtime (ladder KTD10).
struct WatchdogArming {
    session: LiveTeardownSession,
    heartbeats: Heartbeats,
    marks: MarkFeed,
    latch: Arc<TripLatch>,
    node_handle: LiveNodeHandle,
    stop_request: StopRequest,
    stop: Arc<AtomicBool>,
    clock: SessionClock,
    trips: Arc<dyn TripSink>,
    keepalive_path: PathBuf,
    limits: WatchdogLimits,
    mark_policy: MarkPolicy,
    /// The inherited book's stops (U9) — empty on the ladder.
    book_stop_floors: std::collections::HashMap<String, i64>,
    tick: Duration,
    run_id: String,
}

/// Spin the watchdog on its own OS thread + current-thread runtime (ladder KTD10). On a
/// claimed trip it drives the fail-closed teardown **there** — a stalled session runtime
/// cannot stall its own remediation — and then calls `handle.stop()` to unblock
/// `node.run`. Returns the joinable thread; its value is the trip (cause + report), or
/// `None` if the session ended first.
fn spawn_watchdog(
    arming: WatchdogArming,
) -> std::thread::JoinHandle<anyhow::Result<Option<(TripCause, TeardownReport)>>> {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        // The sink is `Send + Sync` and holds no open handle, so the watchdog stays
        // independent of anything the session runtime owns — and, for a rehearsal, opens
        // no dispatch store.
        rt.block_on(async move {
            loop {
                if arming.stop.load(Ordering::SeqCst) {
                    return Ok(None);
                }
                let now = (arming.clock)();
                // Mutual liveness: the session side reads this to detect a dead watchdog.
                arming.heartbeats.touch_supervisor(now);
                let obs = assemble_observation_with_floors(
                    now,
                    &arming.heartbeats,
                    &arming.keepalive_path,
                    &arming.session.ledger(),
                    &arming.marks,
                    &arming.mark_policy,
                    &arming.book_stop_floors,
                );
                let tripped = watchdog_tick_reporting(
                    &arming.session,
                    arming.trips.as_ref(),
                    &arming.latch,
                    &obs,
                    &arming.limits,
                    Some(arming.run_id.as_str()),
                )
                .await;
                match tripped {
                    Ok(Some((cause, report))) => {
                        // The teardown already ran HERE, on this runtime (halt last), and
                        // the cause + kill-switch records are persisted. Unblock
                        // `node.run` so the driver can finalize on this very report.
                        arming.stop_request.request(&arming.node_handle);
                        return Ok(Some((cause, report)));
                    }
                    Ok(None) => {}
                    Err(e) => {
                        // A sink-append failure must never silently disarm the envelope:
                        // the teardown inside `execute_trip` always runs first, so the
                        // remediation happened. Unblock `node.run` BEFORE surfacing the
                        // error — this arm is only reachable after a claimed trip, so the
                        // account is already halted, and returning without the stop would
                        // leave a halted node running to its session timer with the latch
                        // claimed and no supervisor left watching it.
                        arming.stop_request.request(&arming.node_handle);
                        return Err(e);
                    }
                }
                tokio::time::sleep(arming.tick).await;
            }
        })
    })
}

/// Drive one attended live session end-to-end (R4, R5).
///
/// `run_node` is the **only** seam not exercised offline: the live call site passes
/// `move |_| async move { node.run().await }`; tests pass a scripted future. Everything
/// around it — the timer, the watchdog arming, the **hard-stop deadline**, the exactly-one
/// teardown, the staging and the finalize — runs in both.
///
/// The seam is bounded: a node that ignores `handle.stop()` is abandoned `cfg.stop_grace`
/// after the stop was requested, and the session finalizes ABNORMAL down the same path
/// (see [`stop_requested_then_grace`]). Without that, the one un-exercised seam could block
/// every tested seam behind it.
///
/// # Errors
///
/// A staging/finalize failure, or a watchdog chain-append failure. Note that neither a
/// *failed teardown* nor a *hard-stopped node* is an error: both finalize the run ABNORMAL
/// and are reported on the outcome, because either must still leave scannable
/// artifacts (R5).
pub async fn run_live_session<F, Fut>(
    handles: LiveSessionHandles,
    cfg: &LiveDriverConfig,
    ctx: &LiveSessionContext,
    clock: SessionClock,
    run_node: F,
) -> anyhow::Result<LiveSessionOutcome>
where
    F: FnOnce(LiveNodeHandle) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<()>>,
{
    let LiveSessionHandles { session, heartbeats, handle, sink, marks } = handles;
    let latch = Arc::new(TripLatch::new());
    let watchdog_stop = Arc::new(AtomicBool::new(false));
    // The durable record that SOMEONE asked the node to stop — see [`StopRequest`]. The
    // node's own flag is not durable, so this is what the hard-stop deadline arms on.
    let stop_request = StopRequest::new();

    // (3) The watchdog envelope, on its own thread + runtime.
    let watchdog = spawn_watchdog(WatchdogArming {
        session: session.clone(),
        heartbeats: heartbeats.clone(),
        marks: marks.clone(),
        latch: Arc::clone(&latch),
        node_handle: handle.clone(),
        stop_request: stop_request.clone(),
        stop: Arc::clone(&watchdog_stop),
        clock: Arc::clone(&clock),
        trips: Arc::clone(&ctx.authority.trips),
        keepalive_path: cfg.keepalive_path.clone(),
        limits: cfg.limits,
        mark_policy: cfg.mark_policy,
        book_stop_floors: cfg.book_stop_floors.clone(),
        tick: cfg.watchdog_tick,
        run_id: ctx.authority.run_id.clone(),
    });

    // (3b) Mutual liveness on the SESSION side: a dead watchdog thread must never
    // silently degrade the envelope to attended-operator-only. Shares the one latch.
    let liveness = tokio::spawn(session_liveness_loop(
        session.clone(),
        heartbeats.clone(),
        Arc::clone(&latch),
        handle.clone(),
        stop_request.clone(),
        Arc::clone(&clock),
        Arc::clone(&watchdog_stop),
        Arc::clone(&ctx.authority.trips),
        cfg.limits.heartbeat_interval_secs,
        cfg.watchdog_tick,
        ctx.authority.run_id.clone(),
    ));

    // (2) The session timer — `node.run` has none of its own.
    let timer_handle = handle.clone();
    let timer_stop_request = stop_request.clone();
    let session_secs = cfg.session_secs;
    let timer = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(session_secs)).await;
        timer_stop_request.request(&timer_handle);
    });

    // (4) The live-only seam, under the hard-stop deadline. `run_node(..)` is a future this
    // driver OWNS (never a spawned task), so losing the race DROPS it, which cancels it —
    // the detach trap applies to `JoinHandle`s, not to an owned future, and spawning to get
    // an abort handle would force `Send + 'static` onto the one seam that cannot be proven
    // offline. Dropping it does NOT stop whatever the node spawned internally; that is
    // exactly why the teardown below re-asserts the safety invariant at the driver altitude.
    let (run_result, hard_stop) = tokio::select! {
        // BIASED so the node's own return always wins a tie: if it completes in the same
        // poll as the grace elapsing, the session ended on the node's terms and must not be
        // recorded as an abandonment.
        biased;
        r = run_node(handle.clone()) => (r, false),
        () = stop_requested_then_grace(
            stop_request.clone(),
            handle.clone(),
            cfg.watchdog_tick,
            cfg.stop_grace,
        ) => (Ok(()), true),
    };
    timer.abort();

    // Signal both supervisors to stand down BEFORE the driver contends for the claim, then
    // wait for the session-side loop to finish. It is deliberately NOT aborted: aborting a
    // task that had just won the claim would strand a claimed latch with no teardown —
    // the one state this whole design exists to prevent.
    watchdog_stop.store(true, Ordering::SeqCst);
    let liveness_trip = liveness.await.unwrap_or(None);

    // (5) EXACTLY ONE teardown. The atomic claim is the arbiter — a non-atomic
    // `is_tripped()` read here would race the watchdog and let both paths tear down.
    let driver_report = if latch.try_claim() {
        Some(run_teardown(&session, cfg.cancel_attempts, cfg.flat_attempts).await)
    } else {
        None
    };

    // Collect the watchdog's verdict (it returns immediately after a trip; otherwise it
    // observes the stop flag within one tick). A watchdog failure is captured, NEVER
    // propagated: `execute_trip` runs the teardown *before* it surfaces a chain-append
    // error, so bailing here would abandon a session that has already been torn down —
    // with no run directory at all, not even `.tmp-` residue.
    let (trip, supervisor_error) = match tokio::task::spawn_blocking(move || watchdog.join())
        .await
        .map_err(|e| anyhow::anyhow!("watchdog join task: {e}"))?
    {
        Ok(Ok(t)) => (t, None),
        Ok(Err(e)) => (None, Some(nautilus_ls::scrub::scrub_secrets(&e.to_string()))),
        Err(_) => (None, Some("the watchdog thread panicked".to_string())),
    };

    let (report, cause) = match (driver_report, trip, liveness_trip) {
        // The driver won the claim: the session ended on its own terms.
        (Some(r), _, _) => (r, None),
        // The watchdog won: its teardown is the one that ran.
        (None, Some((cause, r)), _) => (r, Some(cause)),
        // The session-side mutual-liveness check won (a dead watchdog thread).
        (None, None, Some((cause, r))) => (r, Some(cause)),
        // A supervisor claimed the latch but handed back no report. The only routes here
        // are a chain-append failure inside `execute_trip` (which tears down first) or a
        // panic mid-teardown — so a teardown ran and its outcome is simply unrecorded.
        // Assume the WORST rather than finalize a possibly-not-flat session as NORMAL,
        // and never bail before the artifacts are written.
        (None, None, None) => (
            TeardownReport { cancel_attempts: 1, canceled: false, flat_confirmed: false },
            None,
        ),
    };

    // (6) Stage + finalize. ALWAYS — a hard-failed teardown must still leave scannable
    // artifacts (R5); `finalize_session` marks it abnormal.
    let run_dir = stage_and_finalize(
        &session,
        &sink,
        cfg,
        ctx,
        &report,
        run_result,
        supervisor_error,
        hard_stop,
        &marks,
    )?;

    // A node that ignored its stop request NEVER reads as a clean session, even when the
    // teardown behind it confirmed flat: the session was ended by abandonment, not by the
    // node's own shutdown, so `--mount` must exit 72 and an operator must look at it.
    Ok(LiveSessionOutcome {
        report,
        trip: cause,
        run_dir,
        abnormal: report.hard_failed() || hard_stop,
        hard_stopped: hard_stop,
    })
}

/// The hard-stop deadline (R1, R4): wait until SOMEONE has asked the node to stop, then
/// give it `grace` to return. Resolving means the grace elapsed with the node still inside
/// `run` — the caller drops the node future.
///
/// It arms on the flag rather than on the timer because three parties can request the stop
/// — the session timer, the watchdog thread, and the mutual-liveness loop — and all three
/// set the same [`LiveNodeHandle`] flag. Arming on the flag is therefore what makes the
/// grace **stop-relative**: a watchdog trip 100 s into a 6-hour session is bounded by
/// `grace`, not by the rest of the session.
///
/// It polls because neither carrier exposes a notify — both are bare atomic loads, which is
/// exactly how the node itself observes the stop. Arming latency is therefore at most one
/// `poll`, immaterial against a minute-scale grace.
///
/// It reads the driver's own [`StopRequest`] latch FIRST and the node's flag only as a
/// belt-and-braces second: the node's flag is not durable (nautilus clears it on every
/// `Running` transition), so a level-triggered read of it alone can miss the arming edge.
pub(crate) async fn stop_requested_then_grace(
    stop_request: StopRequest,
    handle: LiveNodeHandle,
    poll: Duration,
    grace: Duration,
) {
    while !stop_request.requested() && !handle.should_stop() {
        tokio::time::sleep(poll).await;
    }
    tokio::time::sleep(grace).await;
}

/// The session-side mutual-liveness loop (ladder KTD10). Runs on the NODE runtime and
/// shares the one [`TripLatch`], so a watchdog trip and this trip together still tear down
/// exactly once.
#[allow(clippy::too_many_arguments)]
async fn session_liveness_loop(
    session: LiveTeardownSession,
    heartbeats: Heartbeats,
    latch: Arc<TripLatch>,
    node_handle: LiveNodeHandle,
    stop_request: StopRequest,
    clock: SessionClock,
    stop: Arc<AtomicBool>,
    trips: Arc<dyn TripSink>,
    interval_secs: i64,
    tick: Duration,
    run_id: String,
) -> Option<(TripCause, TeardownReport)> {
    loop {
        if stop.load(Ordering::SeqCst) {
            return None;
        }
        tokio::time::sleep(tick).await;
        // Re-check after the sleep so a stand-down signal is never followed by one more
        // claim attempt.
        if stop.load(Ordering::SeqCst) {
            return None;
        }
        let tripped = session_liveness_tick_reporting(
            &session,
            trips.as_ref(),
            &latch,
            clock(),
            heartbeats.supervisor_unix(),
            interval_secs,
            Some(run_id.as_str()),
        )
        .await;
        if let Ok(Some(trip)) = tripped {
            stop_request.request(&node_handle);
            return Some(trip);
        }
    }
}

/// Stage the run's PRE-BUILT manifest, the performance assembled from the **shared** fill
/// ledger, and the drained decisions, then finalize (abnormally when the teardown
/// hard-failed).
///
/// The manifest arrives built (`ctx.manifest`, from [`SessionIdentity::live_manifest`])
/// rather than being assembled here, because assembling a DAILY manifest validates the
/// frozen terms and can refuse — and nothing fallible may enter this function ahead of
/// `finalize_session`'s always-emit tail (R5).
#[allow(clippy::too_many_arguments)]
fn stage_and_finalize(
    session: &LiveTeardownSession,
    sink: &DecisionSink,
    cfg: &LiveDriverConfig,
    ctx: &LiveSessionContext,
    report: &TeardownReport,
    run_result: anyhow::Result<()>,
    supervisor_error: Option<String>,
    hard_stopped: bool,
    marks: &MarkFeed,
) -> anyhow::Result<PathBuf> {
    use crate::artifacts::performance::PerformanceReport;

    let run_id = &ctx.manifest.run_id;
    let writer = RunWriter::new(&ctx.data_home, run_id)?;
    let ledger = session.ledger();
    let dedup_hits = session.dedup_hits();
    let (mut trades, approximated) = {
        let guard = ledger.lock().unwrap_or_else(|e| e.into_inner());
        let fills = guard.fills();
        let approximated = fills.iter().filter(|f| f.price_approximated).count() as u64;
        (pnl::session_trades(fills), approximated)
    };
    // The DAILY lane alone joins risk (U8): the lineage's verdict statistic is
    // `Σrealized/Σrisk_capital`, and an observation cannot be written without it. The ORB
    // lane is deliberately untouched — its live artifacts have always carried no risk join,
    // and moving that would re-baseline the ladder's own evidence in a unit about labels.
    let daily_params = ctx.manifest.daily_params.clone();
    if daily_params.is_some() {
        pnl::join_live_risk(&mut trades, &marks.snapshot());
    }
    let performance = PerformanceReport::assemble(trades, cfg.starting_balance);
    writer.write_performance(&performance)?;

    writer.write_manifest(&ctx.manifest)?;
    writer.write_decisions(&sink.snapshot())?;

    // Mirror the run's KTD2 labels onto the data-quality report so the artifact scans can
    // exclude a rehearsal without opening its manifest (the manifest stays the authority).
    let mut dq = DataQualityReport::backtest(ctx.symbols.clone(), Vec::new())
        .with_run_labels(ctx.manifest.rehearsal, ctx.manifest.paper_stage);
    dq.price_approximated_fills = approximated;
    // U9: what the session's own runner observed. Typed rows first, then its notes — the
    // notes carry the recorded non-failures (R33's "no decision", `--stop-before-orders`)
    // that must appear in the artifacts even though they end the session normally.
    let (gaps, divergences, notes) = ctx.observations.snapshot();
    dq.held_symbol_gaps = gaps;
    dq.rehearsal_divergences = divergences;
    dq.observations.extend(notes);
    // The one KRW figure the runbook asks for on EVERY session, typed so the scrub cannot
    // eat it and so it exists on a session that closed nothing (which writes no
    // observation.json).
    dq.pre_mount_deposit_krw = ctx.deposit_krw;
    // U12. The mount-time book, captured before the teardown rewrites the live one. This is
    // the only place a finished run can learn which run OPENED a leg it closed, so it is
    // written on the rehearsal lane unconditionally — including for an empty book, whose
    // emptiness is itself the answer ("every exit this session closed a leg it opened").
    //
    // FAIL-SOFT, and placed here rather than beside the manifest for the same reason
    // `write_session_observation` below is: this is a supplementary artifact on a session
    // that has ALREADY traded a real account and already torn down. Propagating a write
    // error with `?` would abort ahead of the always-emit tail, so `data_quality.json`
    // would never be written and `finalize` would never rename `.tmp-<run_id>` — trading
    // the whole run's realized P&L and decision evidence for a file that only makes a
    // LATER report more precise. Worse, the error reaches `run_live_session`'s caller in
    // `live_daily::mount`, which then skips the KTD11 book update, so the next mount
    // refuses on a stale stamp too. A missing capture degrades one report; an aborted
    // finalize strands a session nobody can re-run.
    if let Some(book) = &ctx.inherited_book {
        if let Err(e) = writer.write_inherited_book(book) {
            dq.observations.push(format!(
                "inherited-book.json was NOT written ({}) — this run's exits cannot be \
                 attributed to the run that opened their legs, so `report rehearsal` must \
                 treat their provenance as unestablished rather than as this run's",
                nautilus_ls::scrub::scrub_secrets(&e.to_string())
            ));
        }
    }

    // A live DAILY session writes `observation.json` too (U8): one session row, exit
    // attribution, risk capital from the filled quantity. FAIL-SOFT, unlike the backtest's
    // R25 refusal — a backtest with no return-on-risk is a run worth discarding and
    // re-running, whereas this run has already touched a real account. Refusing to finalize
    // it, or `?`-ing here, would trade an artifact for `.tmp-` residue on a session that
    // cannot be re-run.
    if let Some(daily) = daily_params.as_ref() {
        if let Err(note) = crate::artifacts::observation::write_session_observation(
            &writer,
            &ctx.manifest,
            &performance,
            &ctx.trading_date,
            daily,
        ) {
            dq.observations.push(note);
        }
    }
    if let Err(e) = run_result {
        // The node's own run error is a data-quality observation, not a reason to skip
        // finalize: the teardown already ran and the artifacts must stay scannable.
        dq.observations
            .push(format!("node.run returned an error: {}", nautilus_ls::scrub::scrub_secrets(&e.to_string())));
    }
    if hard_stopped {
        // The node never returned from `run` after being asked to stop. The teardown below
        // it still ran — this tells the operator the session ended by ABANDONMENT rather
        // than by the node's own shutdown.
        //
        // The TYPED flag is the load-bearing half: finalizing this run (rather than leaving
        // `.tmp-` residue, as the un-backstopped hang did) removes the residue signal that
        // `scan_limit_events` and `readiness_verdict` already scan for, so without a typed
        // carrier an abandoned-node session would silently score as CLEAN in the ladder's
        // trailing-K window. The observation below is for the human; the flag is for the gate.
        dq.hard_stopped = Some(true);
        dq.observations.push(format!(
            "ABNORMAL: HARD STOP — `node.run` did not return within {}s of the stop request; \
             the node was abandoned and the driver-side teardown ran without it. The teardown's \
             own verdict is recorded above; reconcile the account before the next dispatch",
            cfg.stop_grace.as_secs_f64()
        ));
    }
    if let Some(e) = supervisor_error {
        // The watchdog could not record its trip (or died). The teardown still ran — but
        // its outcome is unrecorded, so the report above is the conservative worst case and
        // this line is what tells the operator why the run reads abnormal.
        dq.observations.push(format!(
            "the watchdog supervisor failed after claiming a trip ({e}) — the teardown ran but \
             its outcome could not be recorded; treat the account as NOT flat and reconcile"
        ));
    }
    // R5's always-emit guarantee: this is the mandatory side effect, and NOTHING
    // fallible may precede it (a `?` here would abort before `write_data_quality`
    // + `finalize`, leaving `.tmp-` residue that `scan_limit_events` classifies as
    // a limit event — a self-inflicted de-escalation).
    let run_dir = finalize_session(writer, dq, report, dedup_hits)?;

    // The LADDER-ONLY tail (U8/KTD2). Everything above is written for every live session;
    // everything here is rung evidence, so it runs only for a session that actually holds a
    // dispatch. A rehearsal's sessions count toward no rung's N (CONCEPTS.md), and a
    // tracking sidecar is exactly the artifact a reducer would count — writing one for a
    // rehearsal would put unauthorized sessions into the ladder's trailing-K window by
    // accident, which is the same class of silent miscount the strategy partition guards.
    let Some(dispatch) = ctx.authority.dispatch.as_ref() else {
        return Ok(run_dir);
    };

    // R12 — the ONLY production caller of `produce_report`.
    // `clean_session_verdict` requires a produced twin at rung >= 2, so removing
    // this call makes rung 2 unreachable by construction: the gate would read a
    // sidecar nothing writes.
    //
    // Placed AFTER `finalize_session` for two independent reasons: the producer
    // reads the FINALIZED run dir (`decisions.jsonl`, `performance.json`,
    // `data-quality.json`, which exist only after the atomic rename), and it must
    // not sit ahead of the always-emit tail above. It is FAIL-SOFT for the same
    // reason: the sidecar lives outside the immutable run dir and is idempotent
    // per run id, so a write failure is re-runnable and must never cost the
    // operator the session's artifacts.
    let catalog_has_range =
        session_range_in_catalog(&ctx.data_home.join("catalog"), &ctx.trading_date, &ctx.symbols);
    let tracking =
        crate::dispatch::tracking::produce_report(&run_dir, run_id, dispatch.rung, catalog_has_range);
    if let Err(e) = crate::dispatch::tracking::write_report(&ctx.data_home, &tracking) {
        eprintln!(
            "live: warning — the tracking report for {run_id} could not be written ({}); the run's \
             own artifacts are finalized and the report is re-runnable per run id",
            nautilus_ls::scrub::scrub_secrets(&e.to_string())
        );
    }
    Ok(run_dir)
}

/// Record one of the mounted session's gateway dispatches (an order call, a t0425 poll)
/// into the per-credential spend-ledger bucket (KTD5). Today only the ingest pacer and
/// universe capture write the ledger, so a live session's own gateway calls would be
/// invisible to the budget-headroom check; this closes that gap. The ledger is a lower
/// bound on true spend, so the headroom verdict stays advisory-deferrable.
///
/// # Errors
///
/// A ledger-save (write/rename) failure.
pub fn record_session_spend(data_home: &Path, lane_hash: &str, at_unix: i64) -> anyhow::Result<()> {
    record_session_spend_n(data_home, lane_hash, at_unix, 1)
}

/// [`record_session_spend`] for `count` dispatches in ONE load/save round-trip.
///
/// The ledger is a file that is read, mutated and renamed on every save, so recording N
/// dispatches as N calls costs N synchronous round-trips. That is the wrong shape inside a
/// polling loop on a single-threaded runtime shared with `node.run` and the liveness check —
/// the pauses are not interleavable, and stalling that runtime is what the watchdog is for.
/// The recorded instants stay distinct so the ledger's per-second bucketing is unchanged.
///
/// # Errors
///
/// A ledger-save (write/rename) failure.
pub fn record_session_spend_n(
    data_home: &Path,
    lane_hash: &str,
    at_unix: i64,
    count: usize,
) -> anyhow::Result<()> {
    if count == 0 {
        return Ok(());
    }
    let catalog = data_home.join("catalog");
    let path = spend_ledger_path(&catalog);
    let mut ledger = SpendLedger::load(&path);
    for i in 0..count {
        ledger.record_spend(lane_hash, at_unix + i as i64);
    }
    ledger.save(&path)?;
    Ok(())
}

/// Resolve the credential lane hash from the lane env file (the bin path). Offline tests
/// pass the hash directly; the bin reads the resolved appkey and hashes it (spend-ledger
/// precedent — never the raw key).
///
/// # Errors
///
/// If the lane env file cannot resolve credentials.
pub fn resolve_lane_hash(lane_env_path: &Path) -> anyhow::Result<String> {
    let adapter_cfg = LsAdapterConfig::from_lane_file(lane_env_path);
    let resolved = adapter_cfg.build_config().map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(SpendLedger::hash_appkey(&resolved.appkey))
}


/// Distinct `--mount` exit codes — never `0`, so a no-TTY shell never mistakes a prepared-but-
/// unrun mount for a completed session (the "never look-like-ran" discipline).
pub(crate) const MOUNT_NOT_PAPER: u8 = 66; // the paper interlock refused (env != paper)
pub(crate) const MOUNT_REFUSED_ATTEND: u8 = 77; // no fresh nonce / no-TTY / no mountable dispatch
/// A fail-closed PRE-CONSUME precheck failed (prereg / fraction / watchdog arming /
/// keepalive / head params / universe / node build). Distinct because it is the one
/// refusal class that is recoverable **and** leaves the green dispatch unconsumed — the
/// operator fixes the input and re-runs `--mount` without a fresh `--dispatch` cycle.
pub(crate) const MOUNT_PRECHECK_FAILED: u8 = 71;
/// The session RAN but finalized ABNORMAL, from EITHER independent cause (or both — see
/// [`mount_verdict`]): the fail-closed teardown could not positively confirm a flat account,
/// or the node was hard-stopped after ignoring its stop request. Never `0` — the operator
/// must reconcile the account before the next dispatch. A persisted kill switch needs
/// clearing only when a watchdog/breaker trip is also recorded; the driver's own teardown
/// engages the switch in-process and appends no chain record.
pub(crate) const MOUNT_ABNORMAL: u8 = 72;

pub(crate) fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Default attended session length — one KRX continuous session with headroom. Overridden
/// by `LS_MOUNT_SESSION_SECS`.
pub(crate) const DEFAULT_SESSION_SECS: u64 = 6 * 60 * 60;
/// Default watchdog cadence — well under any sane pre-registered heartbeat interval, so a
/// stale feeder is caught promptly rather than one interval late.
pub(crate) const DEFAULT_WATCHDOG_TICK_SECS: u64 = 5;
/// Default drain budget after a stop is requested, before the driver abandons the node.
/// Deliberately UNDER the pre-registered `heartbeat_interval_secs` (90 s as frozen), so a
/// node hung on stop is hard-stopped by the driver *before* the dead-man trips on the
/// stalled drain: the dead-man's trip would engage the kill switch AND append a chain
/// record, reding the next `--dispatch` until a nonce-gated `--clear-killswitch`. Raising
/// this above the pre-registered interval reverses that order. Overridden by
/// `LS_MOUNT_STOP_GRACE_SECS`.
pub(crate) const DEFAULT_STOP_GRACE_SECS: u64 = 60;
/// Retry budgets for the session-end teardown (the watchdog path has its own).
pub(crate) const TEARDOWN_CANCEL_ATTEMPTS: usize = 3;
pub(crate) const TEARDOWN_FLAT_ATTEMPTS: usize = 3;
/// Recorded on the equity curve when the operator does not supply the account balance.
pub(crate) const DEFAULT_STARTING_BALANCE: f64 = 10_000_000.0;

/// The stop-relative drain budget, CLAMPED to `[1, heartbeat_interval_secs]`.
///
/// The floor is why the backstop cannot be disabled: a zero grace would abandon the node
/// the instant a stop was requested, and there is no "off" — a session with no hard-stop is
/// the defect this exists to close.
///
/// The ceiling is why it cannot be *effectively* disabled either. A grace above the frozen
/// heartbeat interval re-inverts the ordering the default exists to guarantee: the dead-man
/// would trip first on the stalled drain, engaging the kill switch AND appending a chain
/// record that reds the next `--dispatch` until a nonce-gated `--clear-killswitch`. An
/// operator raising `LS_MOUNT_STOP_GRACE_SECS` to "be generous" would be silently buying
/// that outcome, so the pre-registered interval is the hard ceiling.
pub(crate) fn stop_grace(secs: u64, heartbeat_interval_secs: i64) -> Duration {
    // A non-positive interval is nonsense — the envelope refuses to arm on one upstream —
    // so fall back to the default ceiling rather than clamping to a near-zero grace that
    // would abandon every node on sight. Defensive only; unreachable through `prepare_mount`.
    let ceiling = u64::try_from(heartbeat_interval_secs)
        .ok()
        .filter(|c| *c > 0)
        .unwrap_or(DEFAULT_STOP_GRACE_SECS);
    Duration::from_secs(secs.clamp(1, ceiling))
}

