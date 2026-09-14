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

pub mod mount;
pub mod rehearsal;
pub mod shared;

// The pre-U9 `live.rs` was one flat module, and every call site in the crate and in
// `lab/tests/` reads its items as `runner::live::…`. The split is pure motion, so the
// paths stay: the glob re-exports keep each item exactly as visible as it was.
pub use mount::*;
pub use rehearsal::*;
pub use shared::*;

use std::process::ExitCode;

use crate::dispatch::checks::GateResult;

// The test modules below stayed with the CLI router when `live.rs` split, because they span
// both lanes' items and reach them through this module's re-exports. These are the imports
// only they need.
#[cfg(test)]
use std::path::Path;
#[cfg(test)]
use std::time::Duration;
#[cfg(test)]
use chrono::{TimeZone, Utc};
#[cfg(test)]
use nautilus_live::node::LiveNodeHandle;
#[cfg(test)]
use nautilus_ls::ingest::checkpoint::Checkpoint;
#[cfg(test)]
use nautilus_ls_calendar::CalendarAdoption;
#[cfg(test)]
use crate::artifacts::data_quality::DataQualityReport;
#[cfg(test)]
use crate::artifacts::RunWriter;
#[cfg(test)]
use crate::dispatch::checks::CalendarDateFact;

/// CLI entry point for the `lab-live` bin. `--dispatch` runs the phase-1 pre-flight gate;
/// `--genesis` registers the chain; a bare invocation points at the mount recipe (the
/// mounted session lands in U6). Installs the scrubber first; maps the verdict to an
/// exit code (`research.rs` shape).
pub fn main_cli() -> ExitCode {
    nautilus_ls::scrub::install();
    match dispatch_main() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {}", nautilus_ls::scrub::scrub_secrets(&e.to_string()));
            ExitCode::FAILURE
        }
    }
}

fn dispatch_main() -> anyhow::Result<ExitCode> {
    match std::env::args().nth(1).as_deref() {
        Some("--dispatch") => {
            // The `--dispatch` path emits its OWN deterministic, dispatch-date-targeted
            // startup record from a single load inside `run_dispatch` (KTD3), so the generic
            // `Utc::now()` `emit_startup_from_env` is suppressed here — exactly one
            // `calendar-startup` line fires per --dispatch run.
            //
            // The mandatory diagnostic must fire on EVERY exit path, and the config gather is
            // fallible (it reads and parses the operator's Unknown-override file), so a
            // gather failure would otherwise exit with ZERO calendar-startup lines — the
            // anti-pattern `docs/solutions/conventions/composition-root-always-emit-before-\
            // fallible-parse.md` documents. Emit the generic record only on that error path,
            // so the success path still fires exactly once from inside `run_dispatch`.
            let cfg = dispatch_gate_config_from_env().inspect_err(|_| {
                nautilus_ls::calendar::emit_startup_from_env("lab-live-dispatch");
            })?;
            let out = run_dispatch(&cfg)?;
            for l in &out.lines {
                println!("{l}");
            }
            Ok(match out.result {
                GateResult::Green => ExitCode::SUCCESS,
                GateResult::Refused => ExitCode::FAILURE,
                // A throttle is a re-run, not success and not a plain failure — a
                // distinct exit code so an operator/agent shell never mistakes it for
                // either (never look-like-ran).
                GateResult::Throttled => ExitCode::from(75),
            })
        }
        Some("--genesis") => {
            // Non-dispatch subcommands keep the generic `consumer=lab-live` startup record
            // (KTD6, uniform composition root); only `--dispatch` owns the dispatch-targeted one.
            nautilus_ls::calendar::emit_startup_from_env("lab-live");
            let cfg = dispatch_gate_config_from_env()?;
            for l in &run_genesis(&cfg)? {
                println!("{l}");
            }
            Ok(ExitCode::SUCCESS)
        }
        Some("--mount") => run_mount(),
        // U9. A distinct verb rather than a flag on `--mount`: the two lanes share a driver
        // but not a single precondition — one consumes a green dispatch, the other refuses
        // if a dispatch store even exists to consume from.
        Some("--rehearse-daily") => rehearsal::run_rehearsal(
            std::env::args().any(|a| a == "--stop-before-orders"),
        ),
        Some("--head") => run_head_diagnostic(),
        Some("--escalate") => run_escalate_cli(),
        Some("--reregister") => run_reregister_cli(),
        Some("--clear-killswitch") => run_clear_killswitch_cli(),
        Some("--rung-report") => run_rung_report(),
        _ => {
            nautilus_ls::calendar::emit_startup_from_env("lab-live");
            if std::env::var("LS_TRADING_ENV").as_deref() != Ok("paper") {
                anyhow::bail!("refusing to run: set LS_TRADING_ENV=paper (this adapter is paper-only)");
            }
            anyhow::bail!(
                "lab-live: run `lab-live --dispatch` for the pre-flight gate (`--genesis` to \
                 register the chain, `--mount` to RUN an attended rung-1 live session — it \
                 consumes the green dispatch, drives the session, and finalizes) — see \
                 adapters/nautilus/lab/RUNG1-PREFLIGHT.md"
            )
        }
    }
}


#[cfg(test)]
mod catalog_watermark_tests {
    use super::*;
    use chrono::NaiveDate;
    use nautilus_ls::ingest::checkpoint::GapReason;
    use tempfile::TempDir;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// Write one parquet under the `{instrument}-{bar_type}-LAST-EXTERNAL` series directory,
    /// mirroring the on-disk layout the real catalog uses.
    fn write_series(catalog: &Path, instrument: &str, bar_type: &str) {
        let series = catalog
            .join("data")
            .join("bars")
            .join(format!("{instrument}-{bar_type}-LAST-EXTERNAL"));
        std::fs::create_dir_all(&series).unwrap();
        std::fs::write(series.join("2026-07-28T06-30-00-000000000Z.parquet"), b"pq").unwrap();
    }

    /// A catalog whose daily watermarks are `watermarks`. `bars_for` names the instruments
    /// that actually have daily bars on disk — passing a strict subset of `watermarks` is how
    /// a PARTIAL destructive heal is expressed, which is the realistic shape: a heal wipes one
    /// instrument's series, not the whole tree.
    fn catalog_with_bars(
        tmp: &TempDir,
        watermarks: &[(&str, &str)],
        bars_for: &[&str],
    ) -> std::path::PathBuf {
        let catalog = tmp.path().join("catalog");
        std::fs::create_dir_all(&catalog).unwrap();
        for instrument in bars_for {
            write_series(&catalog, instrument, DAILY_BAR_TYPE);
        }
        let mut ckpt = Checkpoint::default();
        for (instrument, wm) in watermarks {
            ckpt.set_watermark(instrument, DAILY_BAR_TYPE, d(wm));
        }
        ckpt.save(&catalog.join("ingest-checkpoint.json")).unwrap();
        catalog
    }

    /// The common case: every watermarked instrument also has bars.
    fn catalog_with(tmp: &TempDir, watermarks: &[(&str, &str)], with_bars: bool) -> std::path::PathBuf {
        let bars: Vec<&str> = if with_bars { watermarks.iter().map(|(i, _)| *i).collect() } else { vec![] };
        catalog_with_bars(tmp, watermarks, &bars)
    }

    /// U8/R12 — the twin prerequisite. This gate decides whether a paper twin is
    /// `Computed`, and a `Computed` twin at rung >= 2 is load-bearing evidence for
    /// escalating real capital, so a FALSE POSITIVE here fabricates evidence while
    /// a false negative merely defers. Every unestablished input must therefore
    /// resolve to `false`. The live-session tests only ever reach the false branch
    /// (no ingest has run in their rigs), so these cover the admitting branch and
    /// each fail-closed exit directly.
    #[test]
    fn the_twin_prerequisite_admits_only_a_watermark_past_the_session_with_bars_present() {
        let tmp = TempDir::new().unwrap();
        let sym = vec!["005930.XKRX".to_string()];

        // Watermark reached the session AND the series is on disk -> admitted.
        let catalog = catalog_with(&tmp, &[("005930.XKRX", "2026-07-28")], true);
        assert!(session_range_in_catalog(&catalog, "20260728", &sym), "watermark AT the session");
        assert!(
            session_range_in_catalog(&catalog, "20260727", &sym),
            "a watermark PAST the session is coverage too"
        );

        // Watermark short of the session -> deferred. This is the ordinary
        // finalize-time state, because the KRX witness is retrospective.
        assert!(
            !session_range_in_catalog(&catalog, "20260729", &sym),
            "a watermark behind the session cannot attest it"
        );

        // The destructive-heal trap: a current watermark over a wiped series. A
        // watermark alone is not data presence, which is why the predicate asks both.
        // The tempdir is BOUND, not a temporary: `catalog_with(&tmp2(), ..)` would
        // drop it at the end of the statement and delete the tree, so the assertion
        // would pass through the unreadable-checkpoint exit rather than the branch it
        // names — vacuously green against exactly the regression it guards.
        let wiped_tmp = TempDir::new().unwrap();
        let wiped = catalog_with(&wiped_tmp, &[("005930.XKRX", "2026-07-28")], false);
        assert!(
            wiped.join("ingest-checkpoint.json").exists(),
            "the checkpoint must be READABLE, or the false below proves nothing about bars"
        );
        assert!(
            !session_range_in_catalog(&wiped, "20260728", &sym),
            "a fresh watermark over a wiped series must not attest coverage"
        );
    }

    /// The fail-closed exits, each named because a future refactor that drops one
    /// turns a deferral into fabricated evidence. The empty-symbol case is the
    /// sharp one: `iter().all()` over an empty slice is TRUE, so removing the
    /// guard would admit a twin for a session that traded nothing.
    #[test]
    fn the_twin_prerequisite_fails_closed_on_every_unestablished_input() {
        let tmp = TempDir::new().unwrap();
        let catalog = catalog_with(&tmp, &[("005930.XKRX", "2026-07-28")], true);
        let sym = vec!["005930.XKRX".to_string()];

        assert!(
            !session_range_in_catalog(&catalog, "20260728", &[]),
            "an empty symbol set must NOT pass vacuously — all() over empty is true"
        );
        assert!(
            !session_range_in_catalog(&catalog, "not-a-date", &sym),
            "an unparseable trading date establishes nothing"
        );
        assert!(
            !session_range_in_catalog(&tmp.path().join("no-such-catalog"), "20260728", &sym),
            "an unreadable checkpoint establishes nothing"
        );
        assert!(
            !session_range_in_catalog(&catalog, "20260728", &["000660.XKRX".to_string()]),
            "an instrument with no watermark of its own establishes nothing"
        );
        assert!(
            !session_range_in_catalog(
                &catalog,
                "20260728",
                &["005930.XKRX".to_string(), "000660.XKRX".to_string()]
            ),
            "one unattested instrument defers the whole session — all(), not any()"
        );
    }

    /// AE2/R5. The whole point: with no stub and a clean ingest, the check stands on its own.
    /// Before this, an unset stub read `(false, false)` unconditionally, so a flawless ingest
    /// still reddened and spent one of the three deferrals the pre-registration allows per
    /// 5-session window — every single session.
    #[test]
    fn a_clean_catalog_is_fresh_without_a_stub() {
        let tmp = TempDir::new().unwrap();
        let catalog = catalog_with(
            &tmp,
            &[("005930.XKRX", "2026-07-28"), ("000660.XKRX", "2026-07-28")],
            true,
        );
        assert_eq!(evaluate_catalog(&catalog, Some(d("2026-07-28"))), (true, true));
    }

    /// One lagging symbol is enough to make the whole catalog stale — a partial ingest is not
    /// a fresh one, and the mixed watermark distribution is exactly its signature.
    #[test]
    fn one_stale_symbol_yields_the_stale_red_not_the_bars_missing_red() {
        let tmp = TempDir::new().unwrap();
        let catalog = catalog_with(
            &tmp,
            &[("005930.XKRX", "2026-07-28"), ("000660.XKRX", "2026-07-27")],
            true,
        );
        // bars_present stays TRUE so `check_watermark` selects "watermark is stale" rather
        // than the destructive-heal message — two different operator stories.
        assert_eq!(evaluate_catalog(&catalog, Some(d("2026-07-28"))), (false, true));
    }

    /// KTD5. Current watermarks over a recorded coverage gap are NOT fresh. The watermark
    /// says the frontier arrived; the gap says the coverage behind it has a hole. Freshness
    /// that a recorded gap contradicts is not freshness.
    #[test]
    fn a_recorded_gap_defeats_current_watermarks() {
        let tmp = TempDir::new().unwrap();
        let catalog = catalog_with(&tmp, &[("005930.XKRX", "2026-07-28")], true);
        let path = catalog.join("ingest-checkpoint.json");
        let mut ckpt = Checkpoint::load(&path).unwrap();
        ckpt.record_gap("005930.XKRX", DAILY_BAR_TYPE, "20260720-20260721", GapReason::EmptyHistory);
        ckpt.save(&path).unwrap();
        assert_eq!(evaluate_catalog(&catalog, Some(d("2026-07-28"))), (false, true));
    }

    /// An empty catalog reports bars-missing. `bars_present` samples the FILES, not the
    /// checkpoint, so a destructive heal that wiped parquet while leaving the checkpoint
    /// current is visible instead of reading as perfectly fresh.
    #[test]
    fn an_empty_catalog_yields_the_bars_missing_red() {
        let tmp = TempDir::new().unwrap();
        let catalog = catalog_with(&tmp, &[("005930.XKRX", "2026-07-28")], false);
        assert_eq!(evaluate_catalog(&catalog, Some(d("2026-07-28"))), (true, false));
    }

    /// An absent catalog directory entirely.
    #[test]
    fn an_absent_catalog_is_not_fresh_and_has_no_bars() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(evaluate_catalog(&tmp.path().join("nope"), Some(d("2026-07-28"))), (false, false));
    }

    /// The baseline must be PROVEN. An unprovable last session — the normal state before the
    /// morning calendar refresh certifies yesterday — fails closed to the deferrable red
    /// rather than guessing a date the calendar declined to prove.
    #[test]
    fn an_unprovable_last_session_fails_closed() {
        let tmp = TempDir::new().unwrap();
        let catalog = catalog_with(&tmp, &[("005930.XKRX", "2026-07-28")], true);
        assert_eq!(evaluate_catalog(&catalog, None), (false, true));
    }

    /// A catalog ingested PAST the last proven session is ahead, not stale: the calendar lags
    /// the ingest on a day whose witness has not published yet.
    #[test]
    fn a_catalog_ahead_of_the_last_proven_session_is_fresh() {
        let tmp = TempDir::new().unwrap();
        let catalog = catalog_with(&tmp, &[("005930.XKRX", "2026-07-29")], true);
        assert_eq!(evaluate_catalog(&catalog, Some(d("2026-07-28"))), (true, true));
    }

    /// A checkpoint with no daily watermarks testifies to nothing, even with bars on disk.
    /// Presence is now asked per watermarked instrument, so an empty watermark set leaves no
    /// instrument to ask about and BOTH halves fail closed — deliberately stricter than
    /// answering "yes, some parquet exists somewhere", which is not a claim about this
    /// catalog's daily coverage.
    #[test]
    fn no_daily_watermarks_is_not_fresh_and_claims_no_bars() {
        let tmp = TempDir::new().unwrap();
        let catalog = catalog_with_bars(&tmp, &[], &["005930.XKRX"]);
        assert_eq!(evaluate_catalog(&catalog, Some(d("2026-07-28"))), (false, false));
    }

    /// A PARTIAL destructive heal is the realistic shape: one instrument's daily series is
    /// wiped while the rest survive. An unscoped `.any()` over the whole bars tree happily
    /// finds the survivors' parquet and reports bars present, so the check reads fully green
    /// on a catalog that is missing a symbol's data entirely.
    #[test]
    fn a_partial_heal_that_wipes_one_symbols_bars_is_not_bars_present() {
        let tmp = TempDir::new().unwrap();
        let catalog = catalog_with_bars(
            &tmp,
            &[("005930.XKRX", "2026-07-28"), ("000660.XKRX", "2026-07-28")],
            &["005930.XKRX"], // 000660's daily series was wiped
        );
        assert_eq!(evaluate_catalog(&catalog, Some(d("2026-07-28"))), (true, false));
    }

    /// The catalog holds 1-MINUTE series beside the daily ones. A heal that wipes every DAILY
    /// bar leaves the minute files untouched, and an unfiltered sample reads those as proof
    /// that daily bars exist -- defeating the destructive-heal trap entirely.
    #[test]
    fn minute_bars_alone_do_not_satisfy_the_daily_bar_presence_sample() {
        let tmp = TempDir::new().unwrap();
        let catalog = catalog_with_bars(&tmp, &[("005930.XKRX", "2026-07-28")], &[]);
        write_series(&catalog, "005930.XKRX", "1-MINUTE");
        assert_eq!(evaluate_catalog(&catalog, Some(d("2026-07-28"))), (true, false));
    }

    /// KTD5 scopes the gap test the same way the watermark test is scoped. The 1-MINUTE
    /// series share the gap list and lag the daily set by design, so an unfiltered read lets
    /// a minute-series gap red a perfectly current daily catalog -- and permanently, since
    /// recorded gaps are never cleared.
    #[test]
    fn a_minute_series_gap_does_not_red_the_daily_watermark() {
        let tmp = TempDir::new().unwrap();
        let catalog = catalog_with(&tmp, &[("005930.XKRX", "2026-07-28")], true);
        let path = catalog.join("ingest-checkpoint.json");
        let mut ckpt = Checkpoint::load(&path).unwrap();
        ckpt.record_gap("005930.XKRX", "1-MINUTE", "20260720-20260721", GapReason::EmptyHistory);
        ckpt.save(&path).unwrap();
        assert_eq!(evaluate_catalog(&catalog, Some(d("2026-07-28"))), (true, true));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_ls::calendar::ResultingAction;
    use std::cell::RefCell;

    /// The hard-stop backstop has no "off" at EITHER end. A `0` grace — the one value that
    /// would abandon the node the instant a stop was requested — floors to one second. A
    /// grace above the frozen heartbeat interval would let the dead-man trip first on the
    /// stalled drain (kill switch + chain record + a nonce-gated clear), effectively
    /// disabling the driver's own hard-stop, so it is clamped to that interval.
    #[test]
    fn the_stop_grace_is_clamped_between_one_second_and_the_heartbeat_interval() {
        assert_eq!(stop_grace(0, 90), Duration::from_secs(1), "a zero grace is floored");
        assert_eq!(stop_grace(1, 90), Duration::from_secs(1));
        assert_eq!(stop_grace(45, 90), Duration::from_secs(45), "an in-range grace is honored");
        assert_eq!(stop_grace(90, 90), Duration::from_secs(90), "exactly the interval is allowed");
        assert_eq!(
            stop_grace(600, 90),
            Duration::from_secs(90),
            "a supra-heartbeat grace is clamped — it would hand the race back to the dead-man"
        );
        // A nonsensical interval falls back to the default ceiling — never a near-zero
        // grace that would abandon every node on sight, and never an unbounded one.
        assert_eq!(stop_grace(60, 0), Duration::from_secs(DEFAULT_STOP_GRACE_SECS));
        assert_eq!(stop_grace(600, -5), Duration::from_secs(DEFAULT_STOP_GRACE_SECS));

        assert_eq!(DEFAULT_STOP_GRACE_SECS, 60);
        assert!(
            DEFAULT_STOP_GRACE_SECS < 90,
            "the default must stay under the pre-registered heartbeat interval, or a node hung \
             on stop trips the dead-man before the driver hard-stops it"
        );
    }

    /// The hard-stop must arm on the DRIVER's latch, never on the node's flag alone.
    ///
    /// `LiveNodeHandle::set_state` clears `stop_flag` on every transition to `Running`, and
    /// `LiveNode::run` makes that transition *after* client connection and reconciliation —
    /// so a stop requested during startup, exactly when a wedged gateway makes the dead-man
    /// fire, is erased. A backstop polling that flag would miss the edge and never arm,
    /// restoring the block the hard-stop exists to close.
    ///
    /// `stop_flag` is `pub(crate)` in nautilus, so no test here can clear a live handle;
    /// the cleared state is modeled by a fresh handle carrying the same latch. What is
    /// proven is the property that matters: the deadline fires while the node's flag reads
    /// FALSE for the whole wait.
    #[tokio::test]
    async fn the_hard_stop_arms_on_the_driver_latch_even_when_the_node_clears_its_flag() {
        let handle = LiveNodeHandle::new();
        let stop_request = StopRequest::new();
        stop_request.request(&handle);
        assert!(handle.should_stop() && stop_request.requested(), "both carriers see the request");

        // The node transitions to Running and drops the request on the floor.
        let cleared = LiveNodeHandle::new();
        assert!(!cleared.should_stop(), "the node's flag no longer reflects the stop request");
        assert!(stop_request.requested(), "the driver's latch still does — it is never cleared");

        tokio::time::timeout(
            Duration::from_secs(5),
            stop_requested_then_grace(
                stop_request,
                cleared.clone(),
                Duration::from_millis(5),
                Duration::from_millis(20),
            ),
        )
        .await
        .expect("the deadline must still fire — arming on the node's flag alone would hang here");
        assert!(!cleared.should_stop(), "and it fired with the node's flag false throughout");
    }

    /// The converse: with no stop requested by anyone, the backstop never fires. A
    /// spuriously-armed deadline would abandon a healthy node mid-session.
    #[tokio::test]
    async fn the_hard_stop_never_arms_before_a_stop_is_requested() {
        let handle = LiveNodeHandle::new();
        let stop_request = StopRequest::new();
        let fired = tokio::time::timeout(
            Duration::from_millis(120),
            stop_requested_then_grace(
                stop_request,
                handle,
                Duration::from_millis(5),
                Duration::from_millis(1),
            ),
        )
        .await;
        assert!(fired.is_err(), "the backstop must stay disarmed until someone asks for a stop");
    }


    /// A fake session recording the teardown call order + simulating still-resting /
    /// not-flat conditions. `cancel_fail_first` fails that many attempts before the
    /// `cancel_ok` verdict applies, so the retry count can be exercised.
    #[derive(Default)]
    struct FakeSession {
        log: RefCell<Vec<&'static str>>,
        cancel_ok: bool,
        flat: bool,
        cancel_fail_first: RefCell<usize>,
    }

    impl LiveSession for FakeSession {
        fn stop_emission(&self) {
            self.log.borrow_mut().push("stop_emission");
        }
        async fn cancel_all_resting(&self) -> anyhow::Result<usize> {
            self.log.borrow_mut().push("cancel");
            {
                let mut fails = self.cancel_fail_first.borrow_mut();
                if *fails > 0 {
                    *fails -= 1;
                    anyhow::bail!("cancel failed (transient)");
                }
            }
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
        let r = run_teardown(&s, 3, 3).await;
        assert!(!r.hard_failed());
        let log = s.log.borrow();
        assert_eq!(log[0], "stop_emission", "emission stopped FIRST");
        assert_eq!(*log.last().unwrap(), "halt", "kill switch engaged LAST");
        assert!(log.contains(&"cancel") && log.contains(&"is_flat"));
    }

    #[tokio::test]
    async fn teardown_hard_fails_when_not_flat_but_still_halts() {
        let s = FakeSession { cancel_ok: false, flat: false, ..Default::default() };
        let r = run_teardown(&s, 2, 2).await;
        assert!(r.hard_failed(), "not flat + cancels failed -> hard fail");
        assert_eq!(*s.log.borrow().last().unwrap(), "halt", "kill switch engaged even on failure");
    }

    #[tokio::test]
    async fn teardown_hard_fails_when_cancels_ok_but_not_flat() {
        // Cancels succeed but flatness never confirms: the account is NOT concluded flat
        // on ambiguity. Guards the flat term from silently regressing.
        let s = FakeSession { cancel_ok: true, flat: false, ..Default::default() };
        let r = run_teardown(&s, 2, 2).await;
        assert!(r.hard_failed(), "cancels ok but not flat -> still hard fail");
        assert!(r.canceled && !r.flat_confirmed);
        assert_eq!(*s.log.borrow().last().unwrap(), "halt", "kill switch engaged even when not flat");
    }

    #[tokio::test]
    async fn teardown_hard_fails_when_resting_order_remains() {
        let s = FakeSession { cancel_ok: false, flat: true, ..Default::default() };
        let r = run_teardown(&s, 3, 1).await;
        assert!(r.hard_failed(), "cancel failure hard-fails after retries");
        assert_eq!(r.cancel_attempts, 3, "cancel was retried the full 3 attempts");
    }

    #[tokio::test]
    async fn teardown_retry_count_is_recorded() {
        // First cancel fails, second succeeds → 2 attempts, 1 retry (R14(d) metric).
        let s = FakeSession {
            cancel_ok: true,
            flat: true,
            cancel_fail_first: RefCell::new(1),
            ..Default::default()
        };
        let r = run_teardown(&s, 3, 1).await;
        assert!(!r.hard_failed());
        assert_eq!(r.cancel_attempts, 2);
        assert_eq!(r.retries(), 1);
    }

    #[tokio::test]
    async fn finalize_runs_even_on_hard_fail_and_stamps_the_metrics() {
        use crate::artifacts::{data_quality::DataQualityReport, RunWriter};
        let tmp = tempfile::TempDir::new().unwrap();
        let writer = RunWriter::new(tmp.path(), "run-abnormal").unwrap();
        // A hard-failed teardown: finalize must STILL run and mark the run abnormal.
        let s = FakeSession { cancel_ok: false, flat: false, ..Default::default() };
        let report = run_teardown(&s, 2, 2).await;
        assert!(report.hard_failed());
        let dq = DataQualityReport::backtest(vec![], vec![]);
        let dir = finalize_session(writer, dq, &report, 2).unwrap();
        let text = std::fs::read_to_string(dir.join("data_quality.json")).unwrap();
        assert!(text.contains("ABNORMAL"), "abnormal note present: {text}");
        assert!(text.contains("teardown_retries"));
        assert!(text.contains("\"dedup_hits\": 2"), "{text}");
    }

    #[tokio::test]
    async fn safety_trip_is_recorded_before_finalize() {
        use crate::dispatch::chain::{DispatchChain, RecordKind, SafetyTripKind};
        let tmp = tempfile::TempDir::new().unwrap();
        let chain = DispatchChain::open(tmp.path()).unwrap();
        let now = Utc.timestamp_opt(1_752_600_000, 0).unwrap();
        chain.append(now, 1, 1, None, RecordKind::Genesis).unwrap();

        let s = FakeSession { cancel_ok: false, flat: false, ..Default::default() };
        let report = run_teardown(&s, 2, 2).await;
        assert!(report.hard_failed());
        // Trip record BEFORE finalize (KTD4): a fresh dispatch process must observe it.
        record_safety_trip(&chain, SafetyTripKind::KillSwitch, Some("run-x"), "teardown hard-fail", now, 1).unwrap();
        let writer = RunWriter::new(tmp.path(), "run-x").unwrap();
        finalize_session(writer, DataQualityReport::backtest(vec![], vec![]), &report, 0).unwrap();

        let state = chain.load();
        let trip = state.records.iter().any(|r| matches!(&r.body.kind,
            RecordKind::SafetyTrip(t) if t.trip == SafetyTripKind::KillSwitch));
        assert!(trip, "kill-switch trip persisted in the chain");
        assert!(tmp.path().join("runs").join("run-x").exists(), "run finalized despite hard-fail");
        assert!(state.kill_switch_engaged, "the gate would now read the kill switch engaged");
    }

    #[test]
    fn kill_switch_clear_needs_a_fresh_nonce_and_attendance() {
        use crate::dispatch::chain::{DispatchChain, RecordKind, TripAction, SafetyTripKind};
        use crate::dispatch::nonce::OperatorGate;
        let tmp = tempfile::TempDir::new().unwrap();
        let chain = DispatchChain::open(tmp.path()).unwrap();
        let now = Utc.timestamp_opt(1_752_600_000, 0).unwrap();
        chain.append(now, 1, 1, None, RecordKind::Genesis).unwrap();

        // Unattended (no-TTY / CI): refused, nothing appended.
        let unattended = OperatorGate { unattended_marker: Some("CI".into()), nonce: Some("1752600000".into()), now_unix: 1_752_600_000 };
        assert!(clear_kill_switch(&chain, &unattended, "clear", now, 1).is_err());
        let before = chain.load().records.len();

        // Attended + fresh nonce: appends a Clear record.
        let attended = OperatorGate { unattended_marker: None, nonce: Some("1752600000".into()), now_unix: 1_752_600_000 };
        clear_kill_switch(&chain, &attended, "operator cleared after reconcile", now, 1).unwrap();
        let state = chain.load();
        assert_eq!(state.records.len(), before + 1);
        assert!(state.records.iter().any(|r| matches!(&r.body.kind,
            RecordKind::SafetyTrip(t) if t.trip == SafetyTripKind::KillSwitch && t.action == TripAction::Clear)));
    }

    #[test]
    fn kill_switch_clear_scrubs_the_reason(){
        // KTD4 clear-reason capture: a planted secret in the reason never lands in the chain.
        use crate::dispatch::chain::{DispatchChain, RecordKind};
        use crate::dispatch::nonce::OperatorGate;
        let tmp = tempfile::TempDir::new().unwrap();
        let chain = DispatchChain::open(tmp.path()).unwrap();
        let now = Utc.timestamp_opt(1_752_600_000, 0).unwrap();
        chain.append(now, 1, 1, None, RecordKind::Genesis).unwrap();
        let attended = OperatorGate { unattended_marker: None, nonce: Some("1752600000".into()), now_unix: 1_752_600_000 };
        clear_kill_switch(&chain, &attended, "cleared after reconcile on acct 20187511401", now, 1).unwrap();
        let bytes = std::fs::read_to_string(chain.chain_path()).unwrap();
        assert!(!bytes.contains("20187511401"), "the kill-switch clear reason is scrubbed: {bytes}");
    }

    // -----------------------------------------------------------------------
    // U1 (#188) — the single-load dispatch composition root: resolver returns the
    // authoritative date fact AND the mandatory dispatch-date-targeted startup record.
    //
    // Single-load discipline is STRUCTURAL: `resolve_date_fact_and_record` takes an
    // already-loaded `&LoadedCalendar` (it CANNOT load), and `resolve_calendar_for_dispatch`
    // has exactly one `resolve_and_load` call site. The resolver tests inject a fixture-built
    // `LoadedCalendar` directly, so no env is read and the load count is one by construction.
    // -----------------------------------------------------------------------

    /// A short human-shaped authority the token heuristic would pass through — the redacted
    /// startup line must never leak it (mirrors `calendar_composition.rs` SECRET_AUTHORITY).
    const SECRET_AUTHORITY: &str = "Jane Doe / Agreement-7";

    /// The pinned dispatch instant: 2026-07-16 (Thu) 10:00 KST = 01:00 UTC — a KRX weekday,
    /// mid-session, matching the CLI suite's `weekday_ts()`.
    fn dispatch_now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 16, 1, 0, 0).unwrap()
    }

    /// Write a valid snapshot bracketing the pinned dispatch date whose 2026-07-16 row carries
    /// `mid_status`, then load it at `dispatch_now()`. `forward_through` sets the
    /// forward-readiness horizon (drives the `freshness=` token). The authority is
    /// `SECRET_AUTHORITY` so the redaction guard has something to catch.
    fn loaded_fixture(
        dir: &std::path::Path,
        mid_status: nautilus_ls_calendar::schema::DayStatus,
        forward_through: chrono::NaiveDate,
        adoption: CalendarAdoption,
    ) -> nautilus_ls::calendar::LoadedCalendar {
        use nautilus_ls_calendar::schema::{
            Authorization, CalendarScope, Coverage, DayRow, DayStatus, Freshness, Snapshot,
            SourceAvailabilityBound,
        };
        use nautilus_ls_calendar::{compute_artifact_id, compute_calendar_id};
        let d = |y, m, day| chrono::NaiveDate::from_ymd_opt(y, m, day).unwrap();
        let mut snap = Snapshot {
            schema_version: "1.0.0".to_string(),
            artifact_id: String::new(),
            calendar_id: String::new(),
            predecessor_artifact_id: None,
            scope: CalendarScope {
                calendar_name: "KRX domestic equity (SYNTHETIC)".to_string(),
                venue: "XKRX".to_string(),
                instrument_class: "domestic-equity".to_string(),
                timezone: "Asia/Seoul".to_string(),
                synthetic: true,
            },
            authorization: Authorization {
                authorized: true,
                authority: SECRET_AUTHORITY.to_string(),
                granted_at: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
                expires_at: Some(Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap()),
                terminated_at: None,
            },
            coverage: Coverage {
                materialized_from: d(2026, 7, 15),
                materialized_through: d(2026, 7, 17),
                retrospectively_checked_through: d(2026, 7, 17),
                scheduled_closure_evaluated_through: d(2026, 7, 17),
                source_availability: vec![SourceAvailabilityBound {
                    source_id: "s".to_string(),
                    available_from: None,
                    available_through: None,
                }],
            },
            freshness: Freshness {
                evidence_refreshed_at: Utc.with_ymd_and_hms(2026, 7, 16, 0, 0, 0).unwrap(),
                holiday_facts_checked_at: Some(Utc.with_ymd_and_hms(2026, 7, 15, 0, 0, 0).unwrap()),
                full_history_reconciled_at: Some(Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap()),
                forward_readiness_through: Some(forward_through),
                last_incremental_at: Some(Utc.with_ymd_and_hms(2026, 7, 16, 0, 0, 0).unwrap()),
            },
            sources: vec![],
            evidence: vec![],
            alerts: vec![],
            rows: vec![
                DayRow { date: d(2026, 7, 15), status: DayStatus::TradingSession, decisive_evidence: vec![], conflicting_evidence: vec![], alerts: vec![] },
                DayRow { date: d(2026, 7, 16), status: mid_status, decisive_evidence: vec![], conflicting_evidence: vec![], alerts: vec![] },
                DayRow { date: d(2026, 7, 17), status: DayStatus::TradingSession, decisive_evidence: vec![], conflicting_evidence: vec![], alerts: vec![] },
            ],
        };
        snap.artifact_id = compute_artifact_id(&snap);
        snap.calendar_id = compute_calendar_id(&snap);
        let path = dir.join("calendar.json");
        std::fs::write(&path, serde_json::to_vec_pretty(&snap).unwrap()).unwrap();
        nautilus_ls::calendar::resolve_and_load(Some(&path), dispatch_now(), adoption)
    }

    // (The Shadow byte-identical + Shadow-divergence-classification tests were retired with the
    //  Ladder Enforced-only cutover — the date gate no longer has a Legacy/Shadow path.)

    #[test]
    fn u188_enforced_trading_session_from_calendar_not_weekday() {
        use nautilus_ls_calendar::schema::DayStatus;
        let dir = tempfile::TempDir::new().unwrap();
        let loaded = loaded_fixture(dir.path(), DayStatus::TradingSession, chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(), CalendarAdoption::Enforced);
        let (fact, rec, _) = resolve_date_fact_and_record(CalendarAdoption::Enforced, &loaded, dispatch_now());
        assert_eq!(fact, CalendarDateFact::TradingSession);
        assert_eq!(rec.action, ResultingAction::EnforcedActive);
        assert!(rec.render_line().contains("action=enforced-active"));
    }

    #[test]
    fn u188_enforced_closed_from_calendar_fails_and_records_active() {
        use nautilus_ls_calendar::schema::DayStatus;
        let dir = tempfile::TempDir::new().unwrap();
        // 2026-07-16 is a weekday, but the calendar proves it Closed — Enforced returns Closed
        // (the calendar is authoritative), and the record shows the calendar is active.
        let loaded = loaded_fixture(dir.path(), DayStatus::Closed, chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(), CalendarAdoption::Enforced);
        let (fact, rec, _) = resolve_date_fact_and_record(CalendarAdoption::Enforced, &loaded, dispatch_now());
        assert_eq!(fact, CalendarDateFact::Closed, "Enforced reads the calendar, not the weekday");
        assert_eq!(rec.action, ResultingAction::EnforcedActive);
        let line = rec.render_line();
        assert!(line.contains("day=2026-07-16:Closed"), "{line}");
        assert!(line.contains("action=enforced-active"), "{line}");
    }

    #[test]
    fn u188_enforced_missing_snapshot_is_unavailable_and_fail_closed() {
        let dir = tempfile::TempDir::new().unwrap();
        let missing = dir.path().join("does-not-exist.json");
        let loaded = nautilus_ls::calendar::resolve_and_load(Some(&missing), dispatch_now(), CalendarAdoption::Enforced);
        let (fact, rec, _) = resolve_date_fact_and_record(CalendarAdoption::Enforced, &loaded, dispatch_now());
        assert_eq!(fact, CalendarDateFact::Unavailable, "no weekday fallback under Enforced");
        assert_eq!(rec.action, ResultingAction::EnforcedFailClosed);
        assert!(rec.render_line().contains("action=enforced-fail-closed"));
    }

    // (The Legacy weekday-authoritative tests were retired with the Ladder Enforced-only cutover.)

    #[test]
    fn u188_startup_line_is_redacted_no_authority_leak() {
        use nautilus_ls_calendar::schema::DayStatus;
        let dir = tempfile::TempDir::new().unwrap();
        let loaded = loaded_fixture(dir.path(), DayStatus::TradingSession, chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(), CalendarAdoption::Enforced);
        let (_, rec, _) = resolve_date_fact_and_record(CalendarAdoption::Enforced, &loaded, dispatch_now());
        let line = rec.render_line();
        assert!(!line.contains(SECRET_AUTHORITY), "authority leaked into the startup line: {line}");
        assert!(!line.contains("Jane Doe"), "{line}");
    }

    #[test]
    fn u188_stub_seam_still_builds_a_record_reflecting_adoption() {
        // The offline Enforced seam (`date_fact_stub`) wins but still yields a record whose
        // action mirrors what an injected calendar would derive — no snapshot is loaded, so
        // the diagnostic renders `snapshot=not-configured`.
        let rec = stub_startup_record(CalendarAdoption::Enforced, CalendarDateFact::TradingSession);
        assert_eq!(rec.action, ResultingAction::EnforcedActive);
        assert!(rec.render_line().contains("snapshot=not-configured"), "no snapshot loaded in stub mode");

        assert_eq!(
            stub_startup_record(CalendarAdoption::Enforced, CalendarDateFact::Unavailable).action,
            ResultingAction::EnforcedFailClosed
        );
    }
}
