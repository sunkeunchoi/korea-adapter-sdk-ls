//! The REHEARSAL lane (U9): `lab-live --rehearse-daily`.
//!
//! The daily strategy runs on the paper lane under the same safety envelope as a ladder
//! mount, but with **no dispatch chain at all** — nothing is consumed, nothing is appended,
//! and no rung evidence is produced. What replaces the ladder's peek/consume is:
//!
//! 1. the trip gate — a standing `Engage` in `rehearsal/trips.jsonl` refuses the mount
//!    (U13 owns the verb that clears it; the gate itself has to exist here, because the
//!    session it refuses is this one);
//! 2. a `mount_cutoff_kst` check — mounting after the cutoff would start a session that
//!    cannot establish the marks the marketable-limit policy refuses to price without;
//! 3. a pre-build probe of [`verify_book_on`] + the D+2 deposit + the `book.json` fields.
//!
//! All three are PRE-BUILD, which is the ordering property this lane inherits from
//! [`super::mount::run_mount`]: a failure is exit 71 with **no node built and no order
//! sent**. The ladder's version of that property protects a single-use green dispatch; this
//! one protects a live account that already holds an inherited book — a refusal after the
//! node exists has already connected an execution client to it.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Asia::Seoul;
use ls_sdk::LsSdk;
use nautilus_ls::config::LsAdapterConfig;
use nautilus_ls::execution::{read_d2_deposit_on, verify_book_on, ExpectedBook};
use serde::Deserialize;

use super::shared::*;
use crate::dispatch::chain::{kst_trading_date, TripAction};
use crate::dispatch::nonce::{detect_unattended_marker, OperatorGate};
use crate::runner::authority::authorize_rehearsal;
use crate::runner::live_daily::{self, RehearsalBook};
use crate::runner::watchdog::{RehearsalLedger, RehearsalTrip, WatchdogLimits};

// ---------------------------------------------------------------------------
// The envelope (KTD3)
// ---------------------------------------------------------------------------

/// The rehearsal lane's safety envelope and session clock, read from
/// `lab/config/rehearsal-envelope.json`.
///
/// It is the rehearsal's counterpart to the ladder's frozen pre-registration, and
/// deliberately NOT the same file: the pre-registration is a governance artifact whose
/// values re-baseline the ladder's evidence, while a rehearsal produces no rung evidence at
/// all (CONCEPTS.md), so its thresholds are operational. Sequencing item 4 forbids any unit
/// here from touching `preregistration.json`, and reading the ladder's envelope for a lane
/// that cannot produce its evidence would be the first step toward editing it.
#[derive(Debug, Clone, Deserialize)]
pub struct RehearsalEnvelope {
    /// Schema version; only `1` is understood.
    pub version: u32,
    /// The dead-man interval (seconds) — a feeder stale beyond this trips.
    pub heartbeat_interval_secs: i64,
    /// The session max-loss threshold (KRW), compared against the KTD13 basis.
    pub session_max_loss_krw: f64,
    /// The latest KST wall time (`HH:MM`) a mount may still start at.
    pub mount_cutoff_kst: String,
    /// How often the pre-decision sweep re-reads t8407 (seconds).
    pub poll_interval_secs: u64,
    /// The KST wall time (`HH:MM`) whose t8407 read becomes the decision bar (KTD4).
    pub decision_kst: String,
    /// When the closing single-price auction clears (KST `HH:MM`).
    pub auction_end_kst: String,
    /// When the driver requests the node's stop (KST `HH:MM`).
    pub session_end_kst: String,
    /// Drain budget after the stop request, before the node is abandoned.
    pub stop_grace_secs: u64,
    /// Watchdog evaluation cadence (seconds).
    pub watchdog_tick_secs: u64,
    /// How many times the decision read may be retried before the session records
    /// "no decision" (never a trip — R33).
    pub decision_read_attempts: u32,
    /// The crossing offset `k` handed to the marketable-limit policy (KTD5).
    pub marketable_limit_ticks: i64,
    /// The D+2 deposit the pre-mount probe requires (KRW).
    pub min_deposit_krw: i64,
    /// The account balance recorded on the equity curve.
    pub starting_balance_krw: f64,
}

/// The widest dead-man interval that can still fire inside one rehearsal session.
const MAX_HEARTBEAT_SECS: i64 = 15 * 60;
/// The widest session max-loss that is still a bound: the full steady-state envelope the
/// frozen terms imply (`target_m` 8 x hold 16 x `notional_per_position` 781,250).
const MAX_SESSION_LOSS_KRW: f64 = 100_000_000.0;

impl RehearsalEnvelope {
    /// Load and validate the envelope.
    ///
    /// # Errors
    ///
    /// A read/parse failure, an unknown `version`, an out-of-order session clock, or a
    /// `stop_grace` above `heartbeat_interval_secs`.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let bytes = std::fs::read(path)
            .map_err(|e| anyhow::anyhow!("reading the rehearsal envelope {}: {e}", path.display()))?;
        let env: RehearsalEnvelope = serde_json::from_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("parsing the rehearsal envelope {}: {e}", path.display()))?;
        env.validate()?;
        Ok(env)
    }

    /// Fail-closed checks over a loaded envelope.
    ///
    /// # Errors
    ///
    /// The offending message.
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.version != 1 {
            anyhow::bail!(
                "rehearsal envelope version {} is not understood (this binary reads version 1) — \
                 refusing rather than running a session on fields it may be misreading",
                self.version
            );
        }
        // Both bounds are two-sided. A positive-but-absurd value disarms the mechanism just
        // as effectively as a missing one, and this file is named by an environment variable —
        // so "the envelope was armed" has to mean the thresholds can still fire, not merely
        // that they parsed. The ceilings are the widest values that remain meaningful at the
        // frozen terms: the whole ~100,000,000 KRW steady-state envelope, and an interval that
        // still catches a stall inside one session.
        if self.heartbeat_interval_secs <= 0 || self.heartbeat_interval_secs > MAX_HEARTBEAT_SECS {
            anyhow::bail!(
                "rehearsal envelope: heartbeat_interval_secs must be in 1..={MAX_HEARTBEAT_SECS} \
                 (got {}) — a longer dead-man interval than a session cannot fire within one",
                self.heartbeat_interval_secs
            );
        }
        if !(self.session_max_loss_krw.is_finite()
            && self.session_max_loss_krw > 0.0
            && self.session_max_loss_krw <= MAX_SESSION_LOSS_KRW)
        {
            anyhow::bail!(
                "rehearsal envelope: session_max_loss_krw must be in (0, {MAX_SESSION_LOSS_KRW}] \
                 (got {}) — a threshold at or above the whole committed envelope is a breaker \
                 that cannot trip",
                self.session_max_loss_krw
            );
        }
        if self.min_deposit_krw <= 0 {
            anyhow::bail!("rehearsal envelope: min_deposit_krw must be positive");
        }
        if self.decision_read_attempts == 0 {
            anyhow::bail!(
                "rehearsal envelope: decision_read_attempts must be at least 1 — zero attempts \
                 would record `no decision` without ever reading"
            );
        }
        // The clock must be monotone, or the day loop's phases overlap and the decision bar
        // is read after the auction it is supposed to feed.
        let cutoff = self.mount_cutoff()?;
        let decision = self.decision()?;
        let auction = self.auction_end()?;
        let end = self.session_end()?;
        if !(cutoff < decision && decision < auction && auction <= end) {
            anyhow::bail!(
                "rehearsal envelope: the session clock must run mount_cutoff ({}) < decision ({}) \
                 < auction_end ({}) <= session_end ({})",
                self.mount_cutoff_kst,
                self.decision_kst,
                self.auction_end_kst,
                self.session_end_kst
            );
        }
        // The same ordering the ladder's `stop_grace` clamp enforces, but stated as a
        // refusal rather than silently clamped: the ladder's grace comes from an env var an
        // operator may fat-finger, whereas this one is a committed file, so a bad value is a
        // config error to be fixed rather than a value to be corrected behind their back.
        if self.stop_grace_secs > self.heartbeat_interval_secs.unsigned_abs() {
            anyhow::bail!(
                "rehearsal envelope: stop_grace_secs ({}) exceeds heartbeat_interval_secs ({}) — a \
                 node hung on stop would then trip the dead-man before the driver's own hard stop, \
                 turning an operationally-recoverable stall into a recorded trip that refuses the \
                 next mount",
                self.stop_grace_secs,
                self.heartbeat_interval_secs
            );
        }
        Ok(())
    }

    /// The mount cutoff as a KST wall time.
    ///
    /// # Errors
    ///
    /// An unparseable `HH:MM`.
    pub fn mount_cutoff(&self) -> anyhow::Result<NaiveTime> {
        parse_kst_hhmm("mount_cutoff_kst", &self.mount_cutoff_kst)
    }

    /// The decision read's KST wall time.
    ///
    /// # Errors
    ///
    /// An unparseable `HH:MM`.
    pub fn decision(&self) -> anyhow::Result<NaiveTime> {
        parse_kst_hhmm("decision_kst", &self.decision_kst)
    }

    /// When the closing auction clears (KST).
    ///
    /// # Errors
    ///
    /// An unparseable `HH:MM`.
    pub fn auction_end(&self) -> anyhow::Result<NaiveTime> {
        parse_kst_hhmm("auction_end_kst", &self.auction_end_kst)
    }

    /// When the driver requests the stop (KST).
    ///
    /// # Errors
    ///
    /// An unparseable `HH:MM`.
    pub fn session_end(&self) -> anyhow::Result<NaiveTime> {
        parse_kst_hhmm("session_end_kst", &self.session_end_kst)
    }
}

impl WatchdogLimits {
    /// Arm the envelope from the rehearsal's own config file (U9, KTD3) — the sibling of
    /// [`WatchdogLimits::from_prereg`].
    ///
    /// Infallible, unlike the ladder's, because [`RehearsalEnvelope::validate`] has already
    /// refused a half-envelope at load: the ladder reads a values map where a threshold can
    /// simply be absent, whereas these are non-optional fields that cannot deserialize
    /// missing. The fail-closed property is the same, it just lands one step earlier.
    #[must_use]
    pub fn from_envelope(env: &RehearsalEnvelope) -> Self {
        WatchdogLimits {
            heartbeat_interval_secs: env.heartbeat_interval_secs,
            max_loss_krw: env.session_max_loss_krw,
        }
    }
}

fn parse_kst_hhmm(field: &str, raw: &str) -> anyhow::Result<NaiveTime> {
    NaiveTime::parse_from_str(raw.trim(), "%H:%M").map_err(|e| {
        anyhow::anyhow!("rehearsal envelope: {field} {raw:?} is not an `HH:MM` KST wall time ({e})")
    })
}

/// The unix instant of `time` on `date`, read as an Asia/Seoul wall clock.
///
/// KST has no DST, so the mapping is total and unambiguous — the `LocalResult` arms below
/// are unreachable in Seoul and are handled rather than unwrapped only because the type
/// says they exist.
///
/// # Errors
///
/// If the wall time does not exist in the zone (unreachable for KST).
pub fn kst_instant(date: NaiveDate, time: NaiveTime) -> anyhow::Result<i64> {
    match Seoul.from_local_datetime(&date.and_time(time)).single() {
        Some(dt) => Ok(dt.timestamp()),
        None => anyhow::bail!("{date} {time} is not a valid Asia/Seoul wall time"),
    }
}

// ---------------------------------------------------------------------------
// The trip gate (U13's verb clears it; U9 is what it refuses)
// ---------------------------------------------------------------------------

/// The standing trip, if the rehearsal ledger's history ends in one (U13's gate, R33).
///
/// The predicate is on the LAST row per mechanism, not on any row: a ledger that recorded a
/// breaker trip in March and a `Clear` in April is a healthy home, and a gate that refused
/// on "has ever tripped" would make the ledger unusable after its first entry. Returns the
/// first still-engaged mechanism in ledger order so the message names one cause rather than
/// a set.
///
/// Fails CLOSED on an unreadable ledger, inherited from
/// [`RehearsalLedger::records`](crate::runner::watchdog::RehearsalLedger::records): a tail
/// that cannot be parsed must never resolve to "no trip".
///
/// # Errors
///
/// A ledger read/parse failure.
pub fn standing_trip(ledger: &RehearsalLedger) -> anyhow::Result<Option<RehearsalTrip>> {
    let rows = ledger.records()?;
    let mut standing: Vec<RehearsalTrip> = Vec::new();
    for row in rows {
        // Per MECHANISM: a dead-man `Engage` is not cleared by a later breaker `Clear`.
        standing.retain(|s| s.trip != row.trip);
        if row.action == TripAction::Engage {
            standing.push(row);
        }
    }
    Ok(standing.into_iter().next())
}

// ---------------------------------------------------------------------------
// The pre-build probe (KTD6)
// ---------------------------------------------------------------------------

/// What the pre-build probe confirmed about the account, before any node exists.
#[derive(Debug, Clone)]
pub struct BookProbe {
    /// The restored book, field-validated and stamp-fresh.
    pub book: RehearsalBook,
    /// The D+2 deposit read off t0424 `sunamt1`.
    pub deposit_krw: i64,
    /// The book the broker was asked to confirm — handed on to `StartPosture::BookAsserted`
    /// and re-used by the teardown's `book_matches_intent`.
    pub expected: ExpectedBook,
}

/// Prove the account matches the rehearsal book BEFORE the node is built (KTD6, AE3).
///
/// The order is chosen so the cheapest refusal comes first and the account is touched last:
///
/// 1. the book's own fields (a local read — a leg with no stop is unrunnable whatever the
///    broker says, and refusing here costs no gateway call);
/// 2. `verify_book_on` — t0425 resting orders, then the `cts_expcode`-paginated t0424
///    holdings compared against the book's quantities;
/// 3. the D+2 deposit.
///
/// **The deposit is read last and is the one number with a live-hazard caveat.** t0424's
/// `sunamt1` lags `d2dps` by one session on a day the account filled, which is harmless
/// here — this runs at the 15:00 pre-mount, before this session's own fills — but is
/// exactly why nothing downstream may re-read it as a *current* figure after the auction.
///
/// # Errors
///
/// Any of the three, with the offending symbols/values named so the refusal is diagnosable
/// without a second read.
pub async fn probe_book(
    sdk: &LsSdk,
    book: RehearsalBook,
    min_deposit_krw: i64,
) -> anyhow::Result<BookProbe> {
    book.validate_fields()?;
    let expected = book.expected_book();
    verify_book_on(sdk, &expected).await.map_err(|e| {
        anyhow::anyhow!(
            "the pre-mount book probe refused ({e}) — the account does not match \
             rehearsal/book.json. No node was built and no order was sent. Repair the book \
             against the account before re-mounting: the broker's holdings are authoritative for \
             membership and quantity, and rehearsal/book.json is the only record of each leg's \
             stop, entry date and label. (`lab-live --rehearsal-book adopt` will do this; that \
             verb is a later unit and does not exist yet.)"
        )
    })?;
    let deposit_krw = read_d2_deposit_on(sdk)
        .await
        .map_err(|e| anyhow::anyhow!("the pre-mount deposit read failed ({e})"))?;
    if deposit_krw < min_deposit_krw {
        anyhow::bail!(
            "the pre-mount deposit preflight refused: D+2 deposit {deposit_krw} KRW is below the \
             envelope's min_deposit_krw {min_deposit_krw} — the session would size entries against \
             cash the account does not have"
        );
    }
    Ok(BookProbe { book, deposit_krw, expected })
}

// ---------------------------------------------------------------------------
// The operator command
// ---------------------------------------------------------------------------

/// The `--rehearse-daily` file/tunable inputs, gathered from the environment by the bin but
/// **constructible directly**, so every pre-build refusal is testable without mutating the
/// process environment.
#[derive(Debug, Clone)]
pub struct RehearsalInputs {
    /// The rehearsal data home (`LS_DATA_HOME`): `rehearsal/`, `catalog/`, the registry.
    pub data_home: PathBuf,
    /// The envelope file (`LS_REHEARSAL_ENVELOPE`).
    pub envelope_path: PathBuf,
    /// The operator keepalive file (`LS_MOUNT_KEEPALIVE`).
    pub keepalive_path: PathBuf,
    /// The credential lane env file.
    pub lane_env_path: PathBuf,
    /// The `lab-mount-universe --daily` output (`LS_REHEARSAL_UNIVERSE_FILE`).
    pub universe_path: PathBuf,
    /// Record the decision and exit before submitting anything (`--stop-before-orders`).
    pub stop_before_orders: bool,
}

/// Gather the `--rehearse-daily` inputs from the process environment (the bin path).
///
/// # Errors
///
/// If a required path variable is unset or empty.
pub fn rehearsal_inputs_from_env(stop_before_orders: bool) -> anyhow::Result<RehearsalInputs> {
    let required = |key: &str, what: &str| -> anyhow::Result<PathBuf> {
        std::env::var(key)
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("{key} is required ({what}; ABSOLUTE path)"))
    };
    let lane_name = std::env::var("LS_LANE").unwrap_or_else(|_| "domestic".to_string());
    Ok(RehearsalInputs {
        data_home: required("LS_DATA_HOME", "the rehearsal data home")?,
        envelope_path: required(
            "LS_REHEARSAL_ENVELOPE",
            "the rehearsal safety envelope + session clock",
        )?,
        keepalive_path: required(
            "LS_MOUNT_KEEPALIVE",
            "the operator keepalive file the attended operator refreshes; its mtime is the \
             operator dead-man feeder",
        )?,
        universe_path: required(
            "LS_REHEARSAL_UNIVERSE_FILE",
            "the `lab-mount-universe --daily` output for this session",
        )?,
        lane_env_path: std::env::var("LS_DISPATCH_LANE_ENV")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(format!(".env.{lane_name}"))),
        stop_before_orders,
    })
}

/// `lab-live --rehearse-daily` (U9) — run one attended rehearsal session.
///
/// The refusal ORDER mirrors [`run_mount`](super::mount::run_mount) and is the safety
/// property, not a style: paper interlock (66) → attendance/nonce (77) → every fail-closed
/// precheck (71) → only then a node. What differs is what the prechecks are, because there
/// is no dispatch to protect: the ladder's peek/consume is replaced by the trip gate, the
/// mount cutoff, and the pre-build book probe (KTD3).
///
/// `node.run` is driven here and ONLY here on this lane.
///
/// # Errors
///
/// A staging/finalize failure that leaves no artifacts; every operator-recoverable refusal
/// is an exit code, not an error.
pub fn run_rehearsal(stop_before_orders: bool) -> anyhow::Result<ExitCode> {
    nautilus_ls::calendar::emit_startup_from_env("lab-live");
    // 1. Paper interlock FIRST — before any resolution, file read, or gate (R3).
    if std::env::var("LS_TRADING_ENV").as_deref() != Ok("paper") {
        eprintln!(
            "rehearsal refused: LS_TRADING_ENV must be `paper` (this adapter is paper-only; the \
             live-lane flip is a separate later step)"
        );
        return Ok(ExitCode::from(MOUNT_NOT_PAPER));
    }
    // The clock the mount cutoff is checked against is the WALL clock, unless a test seam is
    // explicitly armed. `LS_DISPATCH_NOW_UNIX` alone is the ladder's convention, but on that
    // lane a backdated clock still has to get past a green, same-KST-day dispatch record; this
    // lane has no chain, so the variable would be the whole gate. An operator with a stale
    // export in their shell — or a script that set it for an earlier command — would walk
    // straight through a cutoff that exists because a late mount cannot establish the marks
    // the limit policy refuses to price without. Requiring a second, deliberate variable makes
    // overriding the clock an act rather than an accident, and the day loop runs on the system
    // clock either way, so an override that reached only the prechecks would put the gate and
    // the session on different days.
    let stub_clock = std::env::var("LS_REHEARSAL_STUB_CLOCK").as_deref() == Ok("1");
    let now_unix: i64 = std::env::var("LS_DISPATCH_NOW_UNIX")
        .ok()
        .filter(|_| stub_clock)
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| Utc::now().timestamp());
    let nonce = std::env::var("LS_DISPATCH_NONCE").ok().filter(|s| !s.trim().is_empty());

    // 2. Operator attendance/nonce gate — a rehearsal places real paper orders against an
    //    account that carries an inherited book, so it is attended and no-TTY loud (R3).
    let gate = OperatorGate { unattended_marker: detect_unattended_marker(), nonce, now_unix };
    if let Err(e) = gate.authorize("daily rehearsal mount") {
        eprintln!("rehearsal refused: {}", nautilus_ls::scrub::scrub_secrets(e.as_str()));
        return Ok(ExitCode::from(MOUNT_REFUSED_ATTEND));
    }

    let inputs = match rehearsal_inputs_from_env(stop_before_orders) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("rehearsal refused (pre-build): {e}");
            return Ok(ExitCode::from(MOUNT_PRECHECK_FAILED));
        }
    };

    // 3-8. Every remaining fail-closed precheck, ALL of them pre-build, then the session.
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(live_daily::run_rehearsal_session(&inputs, now_unix))
}

/// The pre-build gates that need no gateway (U9): the envelope, the trip ledger, and the
/// mount cutoff. Separated from [`probe_book`] because these cost nothing and must therefore
/// run first — an operator who mounted at 15:40 should be told so without a credential
/// resolution, and a home with a standing trip should never open an SDK at all.
///
/// # Errors
///
/// The offending message; every arm is exit 71 at the call site.
pub fn preflight_offline(
    inputs: &RehearsalInputs,
    now_unix: i64,
) -> anyhow::Result<(RehearsalEnvelope, LiveDriverConfig)> {
    let envelope = RehearsalEnvelope::load(&inputs.envelope_path)?;

    // (a) The operator keepalive file. Its mtime is the operator dead-man feeder, and an
    //     absent file reads as stale — so a missing one would trip the envelope on the
    //     first watchdog tick.
    if !inputs.keepalive_path.exists() {
        anyhow::bail!(
            "the operator keepalive file does not exist — create it before mounting, or the \
             operator dead-man trips on the first watchdog tick"
        );
    }

    // (b) The trip gate (U13). A standing Engage refuses the mount: the previous session
    //     ended on a safety mechanism, and the account it left behind is exactly the one
    //     this session would inherit.
    let ledger = RehearsalLedger::new(&inputs.data_home);
    if let Some(trip) = standing_trip(&ledger)? {
        anyhow::bail!(
            "a standing {:?} trip from {} refuses this mount ({}). Reconcile the account, then \
             clear it with `lab-live --rehearsal-clear-trip --why <text>` (nonce-gated). The \
             ladder's --clear-killswitch does NOT apply — it writes to a dispatch chain this home \
             does not have",
            trip.trip,
            trip.at_utc,
            trip.detail
        );
    }

    // (c) The mount cutoff. Past it there is not enough of the 15:00-15:20 sweep left to
    //     establish a mark for every mounted symbol — and the marketable-limit policy
    //     REFUSES to price a market order with no decision-time mark (KTD5), so a late
    //     mount does not trade badly, it trades nothing while holding a live node open
    //     against an inherited book.
    let now = Utc.timestamp_opt(now_unix, 0).single().unwrap_or_else(Utc::now);
    let session_date = kst_session_date(now);
    let cutoff_unix = kst_instant(session_date, envelope.mount_cutoff()?)?;
    if now_unix > cutoff_unix {
        anyhow::bail!(
            "the mount cutoff has passed: {} KST is past mount_cutoff_kst {} on {session_date} — \
             too little of the pre-decision sweep remains to establish a mark for every mounted \
             symbol, and the marketable-limit policy refuses to price an order without one",
            now.with_timezone(&Seoul).format("%H:%M"),
            envelope.mount_cutoff_kst
        );
    }

    let limits = WatchdogLimits::from_envelope(&envelope);
    let driver = LiveDriverConfig {
        // The session runs to `session_end_kst`, not for a fixed span: a mount at 15:02 and
        // a mount at 15:10 must both stop at 15:33, or the later one would sit open past the
        // auction it exists to clear through.
        session_secs: u64::try_from((kst_instant(session_date, envelope.session_end()?)? - now_unix).max(1))
            .unwrap_or(1),
        stop_grace: Duration::from_secs(envelope.stop_grace_secs.max(1)),
        watchdog_tick: Duration::from_secs(envelope.watchdog_tick_secs.max(1)),
        limits,
        mark_policy: crate::runner::pnl::MarkPolicy::default(),
        keepalive_path: inputs.keepalive_path.clone(),
        cancel_attempts: TEARDOWN_CANCEL_ATTEMPTS,
        flat_attempts: TEARDOWN_FLAT_ATTEMPTS,
        // Filled from the inherited book once it has been probed (KTD13).
        book_stop_floors: std::collections::HashMap::new(),
        starting_balance: envelope.starting_balance_krw,
    };
    Ok((envelope, driver))
}

/// The KST session date of a UTC instant, as a [`NaiveDate`].
///
/// [`kst_trading_date`] returns the same day as a string; this is the parsed form the
/// calendar queries and the session-ordinal count need. Deliberately routed through the one
/// function rather than re-deriving the +9h shift, so the two can never disagree about which
/// session an instant belongs to.
#[must_use]
pub fn kst_session_date(now: DateTime<Utc>) -> NaiveDate {
    NaiveDate::parse_from_str(&kst_trading_date(now), "%Y-%m-%d")
        // `kst_trading_date` formats with `%Y-%m-%d`, so this is unreachable; the fallback
        // is the UTC date rather than a panic.
        .unwrap_or_else(|_| now.date_naive())
}

/// Resolve the credential lane's SDK for the pre-build probe (U9).
///
/// The SAME `LsSdk` is threaded into the node build afterwards, for the reason
/// [`build_live_session_node`](super::mount::build_live_session_node) states for the ladder:
/// one SDK is one kill switch. Here it buys a second thing — the probe and the session read
/// the account through one credential, so "the book I proved" and "the book I trade" cannot
/// be two different accounts.
///
/// # Errors
///
/// A credential-resolution failure.
pub fn resolve_probe_sdk(lane_env_path: &Path) -> anyhow::Result<(LsSdk, String, LsAdapterConfig)> {
    let adapter_cfg = LsAdapterConfig::from_lane_file(lane_env_path);
    let resolved = adapter_cfg
        .build_config()
        .map_err(|e| anyhow::anyhow!("lane credentials: {e}"))?;
    let lane_hash = nautilus_ls::ingest::budget::SpendLedger::hash_appkey(&resolved.appkey);
    let sdk = LsSdk::new(resolved).map_err(|e| anyhow::anyhow!("sdk: {e}"))?;
    Ok((sdk, lane_hash, adapter_cfg))
}

/// Mint the run id a rehearsal session finalizes under, and the authority that carries it.
///
/// A rehearsal has no consumption marker to have recorded an id at mount time, so it is
/// minted here — from the same [`crate::artifacts::run_id`] the ladder uses, so both lanes'
/// run directories are named identically and one registry scan reads both.
#[must_use]
pub fn rehearsal_authority(
    data_home: &Path,
    now: DateTime<Utc>,
    strategy_id: &str,
    strategy_version: u32,
    lane_hash: String,
) -> Arc<crate::runner::authority::SessionAuthority> {
    let run_id = crate::artifacts::run_id(
        now,
        crate::artifacts::RunSource::Live,
        strategy_id,
        strategy_version,
    );
    Arc::new(authorize_rehearsal(data_home, run_id, lane_hash, "paper".to_string()))
}
