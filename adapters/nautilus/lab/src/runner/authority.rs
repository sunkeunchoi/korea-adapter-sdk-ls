//! U8 — who a live session runs as, and what its manifest says (KTD2, KTD3, KTD14).
//!
//! A sibling of [`crate::runner::live`] rather than more of it. Three reasons, in order of
//! weight:
//!
//! 1. `live.rs` is the file U9 splits into `live/{mount,rehearsal,shared}.rs`, and U8's
//!    verification pins its line count for exactly that reason — the split is U9's to make
//!    on the contents that are already there, not on contents U8 added first.
//! 2. Nothing here drives anything. [`SessionAuthority`] is a value, [`SessionIdentity`] is
//!    a value plus one pure constructor; the whole module is offline-provable with no node,
//!    no runtime, and no gateway, which is the opposite of what `live.rs` is.
//! 3. The ladder/rehearsal distinction is decided ONCE, here, and every consumer below
//!    reads it off these types. Keeping that decision adjacent to the driver invites the
//!    driver to re-derive it.

use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::artifacts::manifest::{DispatchLink, Manifest};
use crate::artifacts::RunSource;
use crate::params::OrbParams;
use crate::params_daily::DailyParams;
use crate::runner::live::{LiveSessionOutcome, MountAuthorization, MOUNT_ABNORMAL};
use crate::runner::watchdog::{ChainTripSink, TripSink};

impl MountAuthorization {
    /// The [`SessionAuthority`] a LADDER mount drives its session under: this
    /// authorization plus a [`ChainTripSink`] at the CHAIN-authorized rung (the rung
    /// safety-trip records have always been appended at — not the effective rung).
    ///
    /// U9's `authorize_rehearsal` is this function's sibling and the reason the conversion
    /// lives here rather than on the driver: the two lanes differ in exactly one value, and
    /// keeping both constructions in one module is what makes that visible.
    ///
    /// # Errors
    ///
    /// A chain-open failure.
    pub fn session_authority(&self, data_home: &std::path::Path) -> anyhow::Result<SessionAuthority> {
        Ok(SessionAuthority {
            run_id: self.run_id.clone(),
            lane_hash: self.lane_hash.clone(),
            trading_env: self.trading_env.clone(),
            dispatch: Some(self.dispatch_link()),
            trips: Arc::new(ChainTripSink::open(data_home, self.chain_rung)?),
        })
    }
}

/// The authority ONE live session runs under (U8, KTD3) — everything the driver, the
/// watchdog, and the finalize path need to know about who authorized this session, with no
/// reference to the ladder's dispatch store.
///
/// [`MountAuthorization`] stays a ladder type: it is the *result* of consuming a green
/// dispatch, and it names a rung. This is what a session is handed afterwards, and it is
/// also what U9's `authorize_rehearsal` produces with no dispatch at all. What makes the
/// difference invisible below this point is [`TripSink`]: `dispatch: None` with a
/// [`RehearsalLedger`](crate::runner::watchdog::RehearsalLedger) is a rehearsal, `Some`
/// with a [`ChainTripSink`](crate::runner::watchdog::ChainTripSink) is a ladder session,
/// and [`run_live_session`] never asks which it has.
///
/// Splitting it this way is what keeps a rehearsal from *creating* `dispatch/`: the old
/// `data_home + chain_rung` pair made every supervisor open a `DispatchChain` for itself,
/// and `DispatchChain::open` creates the directory. A session with no ladder authorization
/// would have manufactured the ladder's authorization store as a side effect of watching
/// its own heartbeat.
#[derive(Clone)]
pub struct SessionAuthority {
    /// The run id the session finalizes under. For a ladder mount this is the id the
    /// consumption marker already recorded AT MOUNT TIME (R14(f)), so it is read here
    /// rather than re-derived.
    pub run_id: String,
    /// The credential lane hash (SHA-256 of the appkey) — never the raw key or account
    /// number.
    pub lane_hash: String,
    /// The resolved trading environment (`"paper"` | `"live"`).
    pub trading_env: String,
    /// The dispatch↔run linkage when the session is ladder-authorized; `None` for a paper
    /// rehearsal. This is the field the ladder-only tail of [`stage_and_finalize`] gates
    /// on, so a rehearsal writes no tracking sidecar and produces no rung evidence.
    pub dispatch: Option<DispatchLink>,
    /// Where a claimed safety trip is durably recorded.
    pub trips: Arc<dyn TripSink>,
}

impl SessionAuthority {
    /// Whether this session runs OUTSIDE the ladder — no dispatch, hence a paper rehearsal
    /// (KTD2). The single derivation: the KTD2 manifest label is stamped from it, so the
    /// label and the authorization cannot disagree.
    pub fn is_rehearsal(&self) -> bool {
        self.dispatch.is_none()
    }
}

// `SessionAuthority` holds an `Arc<dyn TripSink>`, which is not `Debug`; the context's
// derived `Debug` is still wanted for test failure output, so the authority prints as its
// identifying fields rather than forcing `Debug` onto every sink implementation.
impl std::fmt::Debug for SessionAuthority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionAuthority")
            .field("run_id", &self.run_id)
            .field("trading_env", &self.trading_env)
            .field("dispatch", &self.dispatch)
            .field("rehearsal", &self.is_rehearsal())
            .finish_non_exhaustive()
    }
}

/// Which strategy a live session runs, and the parameter set its manifest records (U8).
///
/// A type rather than a flag because the two paths disagree on manifest *shape*, not on a
/// value: `Manifest.params` is a non-optional [`OrbParams`], so a daily session recording
/// its assembly params there would assert a complete, fictitious ORB parameter set — the
/// silent misread `Manifest::validate_strategy_identity` exists to refuse. Making the
/// choice a type is what stops a live daily session from reaching the ORB manifest shape.
#[derive(Debug, Clone)]
pub enum SessionIdentity {
    /// The ORB path: its governed params ARE the manifest's params.
    Orb(OrbParams),
    /// The daily multi-session-hold path (KTD4).
    Daily {
        /// The frozen daily parameter set — the source of the strategy id, version, and
        /// code hash. Validated when the manifest is built.
        daily: DailyParams,
        /// The `OrbParams` the *shared* candidate assembly ran with, recorded verbatim for
        /// reproducibility. Its `strategy_id` is deliberately ignored (KTD14).
        assembly: OrbParams,
    },
}

/// The inputs [`SessionIdentity::live_manifest`] cannot derive for itself.
pub struct LiveManifestParts<'a> {
    /// The authority the session runs under. The run id, the dispatch link, and the KTD2
    /// `rehearsal` label all come from it, so the three cannot disagree.
    pub authority: &'a SessionAuthority,
    /// The traded universe (instrument-id strings), for the manifest's universe hash.
    pub symbols: &'a [String],
    /// The KST trading date the run covers (`YYYYMMDD`) — both ends of the data range.
    pub trading_date: &'a str,
    /// When the session started; the manifest's `created_utc`. Supplied rather than read
    /// from the clock so a driven session's artifacts are reproducible in tests.
    pub started_utc: DateTime<Utc>,
    /// Whether this session ran under the post-judgment paper stage (KTD2). `false` until
    /// a U6 holdout CLEAR certifies the head.
    pub paper_stage: bool,
}

impl SessionIdentity {
    /// The run manifest this identity finalizes under (U8; KTD2, KTD3, KTD14).
    ///
    /// Built at MOUNT time, never on the finalize path: the daily arm runs
    /// [`DailyParams::validate`], and a refusal has to land while the green dispatch is
    /// still unconsumed and no node exists. `stage_and_finalize`'s contract is that it
    /// ALWAYS emits (R5), so nothing fallible may be introduced into it.
    ///
    /// # Errors
    ///
    /// The daily arm's [`DailyParams::validate`] / `validate_strategy_identity` message.
    pub fn live_manifest(&self, parts: LiveManifestParts<'_>) -> Result<Manifest, String> {
        use crate::artifacts::manifest::{
            universe_hash, DailyManifestParts, DailyRunLabel, DataRange,
        };
        let auth = parts.authority;
        let range =
            DataRange { start: parts.trading_date.to_string(), end: parts.trading_date.to_string() };
        // The KTD2 labels are stamped from the authority, not passed in beside it.
        let label = DailyRunLabel {
            source: RunSource::Live,
            run_id: Some(auth.run_id.clone()),
            dispatch: auth.dispatch.clone(),
            rehearsal: Some(auth.is_rehearsal()),
            paper_stage: Some(parts.paper_stage),
        };
        match self {
            SessionIdentity::Orb(params) => {
                let manifest = Manifest {
                    run_id: auth.run_id.clone(),
                    source: RunSource::Live,
                    strategy_id: params.strategy_id.clone(),
                    strategy_version: params.strategy_version,
                    params: params.clone(),
                    data_range: range,
                    catalog_fingerprint: String::new(),
                    universe_hash: universe_hash(parts.symbols),
                    strategy_code_hash: crate::artifacts::manifest::strategy_code_hash(),
                    lab_src_fingerprint: None,
                    checkpoint_hash: None,
                    universe_metadata_hash: None,
                    dispatch: label.dispatch,
                    daily_params: None,
                    rehearsal: label.rehearsal,
                    paper_stage: label.paper_stage,
                    created_utc: parts.started_utc.to_rfc3339(),
                };
                manifest.validate_strategy_identity()?;
                Ok(manifest)
            }
            // The SAME constructor the daily backtest uses, so the discriminator, the run
            // id shape, and the code hash are derived identically on both paths (KTD14).
            SessionIdentity::Daily { daily, assembly } => Manifest::new_daily(DailyManifestParts {
                daily: daily.clone(),
                assembly_params: assembly.clone(),
                daily_source: crate::strategy::DAILY_SOURCE,
                started_utc: parts.started_utc,
                data_range: range,
                catalog_fingerprint: String::new(),
                universe_hash: universe_hash(parts.symbols),
                lab_src_fingerprint: None,
                checkpoint_hash: None,
                universe_metadata_hash: None,
                label,
            }),
        }
    }
}

/// The `--mount` exit code and the operator messages for a finalized session — extracted as
/// a PURE function so the matrix is testable offline. Reaching it end-to-end would require
/// driving a real `node.run`, which the gate forbids.
///
/// The two abnormalities are INDEPENDENT and both are reported: a hard-stopped node whose
/// teardown also failed to confirm flat is the worst combination there is, and an early
/// return on either one would hide the other. Ordering puts the not-flat message last so it
/// is the line left on the operator's screen.
///
/// `rehearsal` selects the recovery instructions (U8/KTD3). A rehearsal has no dispatch
/// chain, so the ladder's `--clear-killswitch` does not apply to it and naming it would
/// send the operator to a nonce-gated verb that writes into a store their home does not
/// have; its own recovery verbs are U13's.
pub(crate) fn mount_verdict(outcome: &LiveSessionOutcome, rehearsal: bool) -> (u8, Vec<String>) {
    // The lane's own recovery path, resolved ONCE and shared by both causes — the
    // diagnosis is identical across lanes, only the verb that fixes it differs.
    let (lane, recovery) = if rehearsal {
        ("rehearsal", "A rehearsal's sessions count toward no rung, so nothing de-escalates — \
         but the next session inherits this account. Reconcile it against \
         `rehearsal/book.json`: `lab-live --rehearsal-book adopt` takes the broker's view and \
         `lab-live --rehearsal-clear-trip --why <text>` clears a recorded trip (both \
         nonce-gated). A trip in `rehearsal/trips.jsonl` refuses the next mount until cleared. \
         The ladder's --clear-killswitch does NOT apply here — it writes to a dispatch chain a \
         rehearsal home does not have. See lab/RUNBOOK-rehearsal-daily.md.")
    } else {
        ("mount", "`hard_stopped` is a typed limit event: the ladder de-escalates and the \
         readiness window reds on it. Reconcile the account before the next dispatch. A \
         recorded watchdog/breaker trip reds the next --dispatch until you clear it with \
         `lab-live --clear-killswitch` (nonce-gated). See lab/RUNBOOK-rung1.md.")
    };
    let mut messages = Vec::new();
    if outcome.hard_stopped {
        // A DIFFERENT abnormality from a failed flat-confirmation: the teardown may well
        // have confirmed flat. What failed is the node — it did not return from `run`
        // within the grace after being asked to stop, so the driver abandoned it.
        messages.push(format!(
            "{lane} ABNORMAL (HARD STOP): `node.run` did not return within \
             LS_MOUNT_STOP_GRACE_SECS of the stop request, so the driver abandoned the node and \
             tore down without it. The run IS finalized and scannable — its data_quality \
             carries `hard_stopped` plus the teardown's own flat verdict. {recovery}"
        ));
    }
    if outcome.report.hard_failed() {
        messages.push(format!(
            "{lane} ABNORMAL: the teardown could not positively confirm a flat account — the kill \
             switch is engaged. {recovery}"
        ));
    }
    if outcome.abnormal {
        (MOUNT_ABNORMAL, messages)
    } else {
        (0, messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::live::TeardownReport;
    use std::path::PathBuf;

    fn outcome_fixture(hard_stopped: bool, flat_confirmed: bool) -> LiveSessionOutcome {
        let report =
            TeardownReport { cancel_attempts: 1, canceled: true, flat_confirmed };
        LiveSessionOutcome {
            report,
            trip: None,
            run_dir: PathBuf::from("/runs/x"),
            abnormal: report.hard_failed() || hard_stopped,
            hard_stopped,
        }
    }

    /// The two ABNORMAL causes are INDEPENDENT, and the worst case is both at once. An
    /// early return on either would hide the other from the operator — this pins that both
    /// are reported, and that the exit contract stays `0`/`72` with no third code.
    #[test]
    fn the_mount_verdict_reports_both_abnormal_causes_and_never_mints_an_exit_code() {
        let (code, msgs) = mount_verdict(&outcome_fixture(false, true), false);
        assert_eq!(code, 0, "a clean session exits 0");
        assert!(msgs.is_empty(), "and says nothing alarming");

        let (code, msgs) = mount_verdict(&outcome_fixture(true, true), false);
        assert_eq!(code, MOUNT_ABNORMAL, "a hard-stop with a CONFIRMED-flat teardown is still 72");
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].contains("HARD STOP"));

        let (code, msgs) = mount_verdict(&outcome_fixture(false, false), false);
        assert_eq!(code, MOUNT_ABNORMAL);
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].contains("could not positively confirm a flat account"));

        // The combination that matters: neither cause may shadow the other.
        let (code, msgs) = mount_verdict(&outcome_fixture(true, false), false);
        assert_eq!(code, MOUNT_ABNORMAL);
        assert_eq!(msgs.len(), 2, "both causes are reported: {msgs:?}");
        assert!(msgs[0].contains("HARD STOP"));
        assert!(
            msgs[1].contains("could not positively confirm a flat account"),
            "the not-flat line is LAST — it is the one left on the operator's screen"
        );
    }

    /// U8/KTD3. The rehearsal arm names the rehearsal's own recovery verbs and must NOT
    /// name `--clear-killswitch`: that verb writes into a dispatch chain a rehearsal home
    /// does not have, so following it would leave the operator running a nonce-gated
    /// command against a store that does not exist while the real blocker — a standing
    /// `Engage` in `rehearsal/trips.jsonl` — still refuses the next mount.
    #[test]
    fn the_rehearsal_arm_names_the_rehearsal_recovery_verbs_and_not_the_ladders() {
        let (code, msgs) = mount_verdict(&outcome_fixture(true, false), true);
        assert_eq!(code, MOUNT_ABNORMAL, "the exit contract is the same in both lanes");
        assert_eq!(msgs.len(), 2, "both causes are still reported: {msgs:?}");
        for m in &msgs {
            assert!(m.starts_with("rehearsal ABNORMAL"), "labelled as a rehearsal: {m}");
            assert!(m.contains("--rehearsal-book adopt"), "{m}");
            assert!(m.contains("--rehearsal-clear-trip"), "{m}");
            assert!(
                !m.contains("`lab-live --clear-killswitch`"),
                "the ladder verb is not offered to a rehearsal: {m}"
            );
            assert!(!m.contains("RUNBOOK-rung1"), "nor the ladder runbook: {m}");
        }

        // And the ladder arm still says the ladder things.
        let (_, ladder) = mount_verdict(&outcome_fixture(true, false), false);
        assert!(ladder[0].contains("de-escalates"), "{ladder:?}");
        assert!(ladder[1].contains("--clear-killswitch"), "{ladder:?}");
    }
}
