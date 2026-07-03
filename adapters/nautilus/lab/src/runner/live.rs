//! Live paper runner (U6, F2) — the same ORB runs against the paper gateway and
//! emits the same artifact set into the same registry, fail-closed. **Operator-gated:
//! never run by the offline gate.** Proven offline by wiring tests + a direct-drive
//! test; the full `node.run` session is exercised only by the `lab-live` bin.
//!
//! Safety (KTD7): the runner takes the live advisory lock, honors the paper-only
//! interlock, and at exit/market-close runs a fail-closed teardown — stop the
//! strategy's order emission first, cancel all resting orders, run a quantity-keyed
//! t0425 flatness check (positive confirmation only), and engage the exec client's
//! kill switch only AFTER the closing cancels complete (the kill-switch-ordering
//! trap). Artifacts finalize on teardown; a crash leaves the `.tmp-` run directory as
//! the aborted-run marker.

use std::path::{Path, PathBuf};

use nautilus_ls::lock::{AdvisoryLock, LockKind};
use nautilus_ls::orders::ledger::FillDelta;
use nautilus_ls::orders::poll::PollOutcome;

use crate::artifacts::data_quality::{DataQualityReport, ReconcileCondition, ReconcileConditionKind};
use crate::params::OrbParams;

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

/// Run the fail-closed session teardown (KTD7). The ordering is the safety property:
/// stop emission → cancel resting (retried) → confirm flat (retried, positive-only) →
/// **halt after** → hard-fail if flatness could not be positively confirmed (never
/// conclude flat on ambiguity).
pub async fn run_teardown<S: LiveSession>(
    session: &S,
    cancel_attempts: usize,
    flat_attempts: usize,
) -> anyhow::Result<()> {
    // 1. Stop the strategy's order emission first.
    session.stop_emission();

    // 2. Cancel all resting orders, retrying.
    let mut canceled = false;
    for _ in 0..cancel_attempts.max(1) {
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

    // 5. Never conclude flat on ambiguity.
    if !canceled || !flat {
        anyhow::bail!(
            "teardown could not positively confirm a flat account — kill switch engaged; the operator must reconcile the paper account"
        );
    }
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

/// Record a poll pass's reconcile-advised condition into the data-quality report
/// (R7, AE3). The poll lane collapses its specific inconclusive reasons (truncation,
/// unresolved row, cumulative regression, request failure) into a single
/// `reconcile_needed` flag, so this records the honest
/// [`ReconcileConditionKind::PollInconclusive`] rather than mislabeling a specific
/// cause — the agent treats the run's accounting as suspect either way.
pub fn record_reconcile(dq: &mut DataQualityReport, outcome: &PollOutcome, symbol: &str) {
    if outcome.reconcile_needed {
        dq.reconcile_advised.push(ReconcileCondition {
            kind: ReconcileConditionKind::PollInconclusive,
            symbol: symbol.to_string(),
        });
    }
}

/// CLI entry point for the operator-gated `lab-live` bin. Refuses unless
/// `LS_TRADING_ENV=paper`; the full LiveNode session is the operator's to run per the
/// documented recipe (`adapters/nautilus/lab/README.md`). This offline-safe stub
/// installs the scrubber, enforces the paper interlock, and reports the staged status
/// rather than touching the gateway from a non-operator context.
pub fn main_cli() -> anyhow::Result<()> {
    nautilus_ls::scrub::install();
    if std::env::var("LS_TRADING_ENV").as_deref() != Ok("paper") {
        anyhow::bail!("refusing to run: set LS_TRADING_ENV=paper (this adapter is paper-only)");
    }
    // The live session (lock acquisition, LiveNode mount + run, fail-closed teardown,
    // artifact finalize) is documented as an operator recipe; wiring it end-to-end
    // requires live credentials + a KRX window and is never driven by the gate.
    anyhow::bail!(
        "lab-live is operator-gated: follow the recipe in adapters/nautilus/lab/README.md \
         (live credentials + an open KRX window required)"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A fake session recording the teardown call order + simulating still-resting /
    /// not-flat conditions.
    #[derive(Default)]
    struct FakeSession {
        log: RefCell<Vec<&'static str>>,
        cancel_ok: bool,
        flat: bool,
    }

    impl LiveSession for FakeSession {
        fn stop_emission(&self) {
            self.log.borrow_mut().push("stop_emission");
        }
        async fn cancel_all_resting(&self) -> anyhow::Result<usize> {
            self.log.borrow_mut().push("cancel");
            if self.cancel_ok {
                Ok(1)
            } else {
                anyhow::bail!("cancel failed")
            }
        }
        async fn is_flat(&self) -> bool {
            self.log.borrow_mut().push("is_flat");
            self.flat
        }
        fn halt(&self) {
            self.log.borrow_mut().push("halt");
        }
    }

    #[tokio::test]
    async fn teardown_order_is_stop_cancel_flat_then_halt() {
        let s = FakeSession { cancel_ok: true, flat: true, ..Default::default() };
        run_teardown(&s, 3, 3).await.unwrap();
        let log = s.log.borrow();
        assert_eq!(log[0], "stop_emission", "emission stopped FIRST");
        assert_eq!(*log.last().unwrap(), "halt", "kill switch engaged LAST");
        // A cancel and a flat check happened between.
        assert!(log.contains(&"cancel") && log.contains(&"is_flat"));
    }

    #[tokio::test]
    async fn teardown_hard_fails_when_not_flat_but_still_halts() {
        // Cancels error and flatness never confirms → hard-fail, but halt still ran.
        let s = FakeSession { cancel_ok: false, flat: false, ..Default::default() };
        let err = run_teardown(&s, 2, 2).await.unwrap_err();
        assert!(err.to_string().contains("could not positively confirm a flat account"), "err: {err}");
        assert_eq!(*s.log.borrow().last().unwrap(), "halt", "kill switch engaged even on failure");
    }

    #[tokio::test]
    async fn teardown_hard_fails_when_cancels_ok_but_not_flat() {
        // Cancels succeed but the flatness check never confirms (a fill landed during
        // teardown, or a truncated read): the `|| !flat` term alone must hard-fail —
        // the account is NOT concluded flat on ambiguity. Guards the flat term from
        // silently regressing (every other failure test also fails the cancel).
        let s = FakeSession { cancel_ok: true, flat: false, ..Default::default() };
        let err = run_teardown(&s, 2, 2).await.unwrap_err();
        assert!(err.to_string().contains("could not positively confirm a flat account"), "err: {err}");
        assert_eq!(*s.log.borrow().last().unwrap(), "halt", "kill switch engaged even when not flat");
    }

    #[tokio::test]
    async fn teardown_hard_fails_when_resting_order_remains() {
        // A still-resting order: cancels fail but the account later reads flat is
        // impossible here — cancel failure alone hard-fails after retries.
        let s = FakeSession { cancel_ok: false, flat: true, ..Default::default() };
        let err = run_teardown(&s, 3, 1).await.unwrap_err();
        assert!(err.to_string().contains("reconcile"), "err: {err}");
        // Cancel was retried the full 3 attempts.
        assert_eq!(s.log.borrow().iter().filter(|e| **e == "cancel").count(), 3);
    }
}
