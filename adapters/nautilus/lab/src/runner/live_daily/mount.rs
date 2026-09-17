//! The rehearsal's mount and session orchestration (U9): every pre-build gate, the node
//! build, the driven session, and the book the next session inherits.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{NaiveDate, TimeZone, Utc};
use ls_sdk::LsSdk;
use nautilus_ls::execution::{MarketableLimitPolicy, SessionInstruments, StartPosture};
use nautilus_model::identifiers::InstrumentId;
use nautilus_model::instruments::InstrumentAny;

use crate::runner::backtest_daily::{
    DailyPathStrategy, DailySessionSignals, MountedSymbol,
    OpenPositionBook,
};
use crate::runner::live::rehearsal::{kst_instant, kst_session_date, RehearsalInputs};
use crate::runner::live::shared::SessionObservations;
use crate::runner::mount_universe::DailyUniverseFile;
use crate::runner::rehearsal_data::DataEventSink;
use crate::runner::watchdog::Heartbeats;
use crate::strategy::hooks::MarkFeed;

use super::*;

// ---------------------------------------------------------------------------
// The session orchestration
// ---------------------------------------------------------------------------

/// Load and bind the session's universe file (U10's output).
///
/// # Errors
///
/// A read/parse failure, or a file resolved for another session date.
pub fn load_universe(path: &Path, session_date: NaiveDate) -> anyhow::Result<DailyUniverseFile> {
    let bytes = std::fs::read(path)
        .map_err(|e| anyhow::anyhow!("reading the rehearsal universe {}: {e}", path.display()))?;
    let file: DailyUniverseFile = serde_json::from_slice(&bytes)
        .map_err(|e| anyhow::anyhow!("parsing the rehearsal universe {}: {e}", path.display()))?;
    let want = session_date.format("%Y-%m-%d").to_string();
    if file.session_date != want {
        anyhow::bail!(
            "the universe file was resolved for {} but this session is {want} — its ranks and \
             prior closes are yesterday's, and the runner re-runs no selection of its own",
            file.session_date
        );
    }
    if file.rows.is_empty() {
        anyhow::bail!("the universe file for {want} has no rows — nothing could be entered");
    }
    Ok(file)
}

/// The marketable-limit policy for this session (KTD5).
#[must_use]
pub fn limit_policy(
    ticks: i64,
    session_date: NaiveDate,
    marks: MarkFeed,
    equities: Arc<SessionEquities>,
) -> MarketableLimitPolicy {
    MarketableLimitPolicy::new(
        ticks,
        nautilus_ls::rules::TickRegime::for_date(session_date),
        Arc::new(SweptPrices::new(marks)),
        equities,
    )
}

/// The start posture a session mounts with: `BookAsserted` once a book has been proven,
/// `Flat` when there is nothing to inherit.
///
/// The distinction is not cosmetic. `BookAsserted` skips `connect()`'s flat assertion, which
/// is the ONE thing it skips — so mounting `BookAsserted` on a home with no book would give
/// up the flat-start check for nothing, while mounting `Flat` with a book would refuse a
/// healthy session at connect time, after the node exists.
#[must_use]
pub fn start_posture(book: &RehearsalBook) -> StartPosture {
    if book.legs.is_empty() {
        StartPosture::Flat
    } else {
        StartPosture::BookAsserted
    }
}

/// The KST instants this session's clock resolves to.
#[derive(Debug, Clone, Copy)]
pub struct SessionClockPoints {
    /// When the decision read happens.
    pub decision_unix: i64,
    /// When the closing auction clears.
    pub auction_end_unix: i64,
    /// When the driver requests the node's stop.
    pub session_end_unix: i64,
}

/// Resolve the envelope's KST wall times onto `session_date`.
///
/// # Errors
///
/// An unparseable or non-existent wall time.
pub fn session_clock_points(
    envelope: &crate::runner::live::rehearsal::RehearsalEnvelope,
    session_date: NaiveDate,
) -> anyhow::Result<SessionClockPoints> {
    Ok(SessionClockPoints {
        decision_unix: kst_instant(session_date, envelope.decision()?)?,
        auction_end_unix: kst_instant(session_date, envelope.auction_end()?)?,
        session_end_unix: kst_instant(session_date, envelope.session_end()?)?,
    })
}

/// Everything resolved and BUILT before the session runs (U9). Producing this successfully
/// is the guarantee that the session will run — the rehearsal counterpart of
/// [`PreparedMount`](crate::runner::live::mount::PreparedMount), and it earns the same
/// property: every fallible step is behind it, so the node exists only once nothing
/// recoverable can still refuse.
pub struct PreparedRehearsal {
    /// The built node.
    pub node: nautilus_live::node::LiveNode,
    /// The driver/watchdog/finalize handle set, captured before the builder.
    pub handles: crate::runner::live::shared::LiveSessionHandles,
    /// The driver tunables, armed from the envelope.
    pub driver: crate::runner::live::shared::LiveDriverConfig,
    /// The identity + artifact context the session finalizes under.
    pub ctx: crate::runner::live::shared::LiveSessionContext,
    /// The day loop, ready to be spawned onto the node's runtime.
    pub day: DayLoop,
    /// The teardown's book assertion, and the snapshot it will take.
    pub book_intent: crate::runner::live::shared::BookIntent,
    /// The book this session inherited — the base the next one is written from.
    pub previous_book: RehearsalBook,
    /// The rehearsal home.
    pub data_home: PathBuf,
    /// This session's KST date (`YYYY-MM-DD`).
    pub session_date: String,
    /// Where the session's own gateway dispatches are bucketed.
    pub lane_hash: String,
    /// The Live advisory lock, held from before the account probe until after the book is
    /// written. Never read — its lifetime IS its purpose, so it is dropped with this value.
    _live_lock: nautilus_ls::lock::AdvisoryLock,
}

/// `lab-live --rehearse-daily`'s body: every pre-build gate, the node, the day, the book.
///
/// # Errors
///
/// A staging/finalize failure. Every operator-recoverable refusal is an exit code, so the
/// error arm here is only reachable when the run left no artifacts at all.
pub async fn run_rehearsal_session(
    inputs: &RehearsalInputs,
    now_unix: i64,
) -> anyhow::Result<ExitCode> {
    let prepared = match prepare_rehearsal(inputs, now_unix).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "rehearsal refused (pre-build): {} — NO node was built and NO order was sent; \
                 fix the input and re-run",
                nautilus_ls::scrub::scrub_secrets(&e.to_string())
            );
            return Ok(ExitCode::from(crate::runner::live::shared::MOUNT_PRECHECK_FAILED));
        }
    };
    drive_rehearsal(prepared).await
}

/// Every pre-build gate, then the node (U9).
///
/// The ORDER is the safety property and mirrors [`prepare_mount`](crate::runner::live::mount::prepare_mount):
/// the free checks first, then the local files, then the account, then the build. A failure
/// anywhere leaves no node, no order, and an unchanged book.
///
/// # Errors
///
/// The offending message; the caller turns every one of them into exit 71.
pub async fn prepare_rehearsal(
    inputs: &RehearsalInputs,
    now_unix: i64,
) -> anyhow::Result<PreparedRehearsal> {
    use crate::runner::live::rehearsal::{preflight_offline, probe_book, resolve_probe_sdk};

    let now = Utc.timestamp_opt(now_unix, 0).single().unwrap_or_else(Utc::now);
    let session_date = kst_session_date(now);
    let session_date_str = session_date.format("%Y-%m-%d").to_string();

    // (a) The gates that need no gateway: envelope, keepalive, trip ledger, mount cutoff.
    let (envelope, mut driver) = preflight_offline(inputs, now_unix)?;
    let points = session_clock_points(&envelope, session_date)?;

    // (b) The session ordinal and the previous proven session, both off the calendar
    //     snapshot (KTD11). Counted THROUGH YESTERDAY: today is not a proven session until
    //     it is over, and a run that counted itself would give every leg it opens an
    //     ordinal one higher than the next session reads back.
    let calendar = nautilus_ls::calendar::IngestCalendarContext::from_env(now);
    let view = calendar.view().ok_or_else(|| {
        anyhow::anyhow!(
            "no calendar snapshot is available (LS_CALENDAR_SNAPSHOT) — session ordinals and the \
             book's freshness are both measured on the proven calendar, and neither can be \
             derived from the wall clock"
        )
    })?;
    let yesterday = session_date.pred_opt().ok_or_else(|| anyhow::anyhow!("{session_date} has no previous day"))?;
    let (_, previous_session) = session_ordinal(&view, yesterday)?;
    let previous_session = previous_session.ok_or_else(|| {
        anyhow::anyhow!(
            "the calendar proves no trading session before {session_date} — the book's freshness \
             cannot be judged and no session ordinal can be derived"
        )
    })?;
    let hold = crate::params_daily::DailyParams::frozen().holding_period_sessions;
    let (calendar_sessions, session_index) = session_calendar(&view, session_date, hold)?;

    // (c) The universe and the book — local files, bound to this session.
    let universe = load_universe(&inputs.universe_path, session_date)?;
    let book = RehearsalBook::load(&inputs.data_home)?;
    book.assert_fresh(previous_session)?;

    // (d) The Live advisory lock, BEFORE the first account read and held through the book
    //     write. The ladder takes it inside `authorize_mount` for the TOCTOU reason that
    //     applies here at least as strongly: this lane probes the account, then acts on what
    //     it saw. Without the lock a second rehearsal (or a ladder mount) can pass the same
    //     probe against the same account, and the two sessions then place orders neither
    //     accounted for, cancel each other's residue in their all-account teardowns, and race
    //     the same `book.json` temp file. It is taken after the free refusals so a late mount
    //     or a standing trip still fails without contending for it.
    let live_lock = crate::runner::live::shared::live_guard(&inputs.data_home)?;

    // (e) The account. One SDK for the probe AND the session (see `resolve_probe_sdk`).
    let (sdk, lane_hash, adapter_cfg) = resolve_probe_sdk(&inputs.lane_env_path)?;
    let probe = probe_book(&sdk, book, envelope.min_deposit_krw).await?;
    let deposit_krw = probe.deposit_krw;
    let book = probe.book;

    // (f) The instruments. An UNCACHED instrument makes nautilus skip reconciliation
    //     SILENTLY, so the inherited book would never be attributed to the strategy and its
    //     exits would have nothing to close — this is a precondition, not an optimization.
    let mounted_ids = mounted_instrument_ids(&universe, &book)?;
    let instruments = load_session_instruments(&sdk, &mounted_ids).await?;
    let equities = Arc::new(SessionEquities::new(&instruments));
    for id in &mounted_ids {
        if equities.equity(id).is_none() {
            anyhow::bail!(
                "{id} is in the mounted universe but the gateway's instrument master does not \
                 carry it — without a cached Equity the marketable-limit policy has no daily \
                 band to clamp to, and reconciliation would skip the symbol in silence"
            );
        }
    }

    // (g) The node. The last thing that can fail recoverably.
    let daily = crate::params_daily::DailyParams::frozen();
    driver.book_stop_floors = crate::runner::pnl::book_stop_floors(&book.pnl_legs());
    let mount = build_rehearsal_node(RehearsalNodeParts {
        adapter_cfg,
        sdk: sdk.clone(),
        daily: daily.clone(),
        book: &book,
        instruments,
        equities: Arc::clone(&equities),
        mounted_ids: mounted_ids.clone(),
        data_home: inputs.data_home.clone(),
        session_date,
        previous_session,
        marketable_limit_ticks: envelope.marketable_limit_ticks,
        now_unix,
        session_calendar: calendar_sessions.clone(),
    })?;

    // (h) The identity. Built at MOUNT time because the daily arm validates the frozen
    //     terms and can refuse — and `stage_and_finalize` may not fail.
    let authority = crate::runner::live::rehearsal::rehearsal_authority(
        &inputs.data_home,
        now,
        &daily.strategy_id,
        daily.strategy_version,
        lane_hash.clone(),
    );
    let symbols: Vec<String> = mounted_ids.iter().map(ToString::to_string).collect();
    let trading_date = session_date.format("%Y%m%d").to_string();
    let manifest = crate::runner::authority::SessionIdentity::Daily {
        daily: daily.clone(),
        assembly: crate::params::OrbParams::default(),
    }
    .live_manifest(crate::runner::authority::LiveManifestParts {
        authority: &authority,
        symbols: &symbols,
        trading_date: &trading_date,
        started_utc: now,
        // KTD2: the paper stage opens only after a U6 holdout CLEAR certifies the head.
        paper_stage: false,
    })
    .map_err(|e| anyhow::anyhow!("rehearsal refused: {e}"))?;

    let observations = SessionObservations::new();
    let ctx = crate::runner::live::shared::LiveSessionContext {
        data_home: inputs.data_home.clone(),
        authority: (*authority).clone(),
        manifest,
        symbols,
        trading_date,
        observations: observations.clone(),
        // U12. Captured HERE, from the probed book, not read back at finalize: by then the
        // teardown has rewritten `rehearsal/book.json` and the legs this session closed are
        // gone from it, `entered_under` with them.
        inherited_book: Some(book.clone()),
        // The figure the pre-mount probe gated on, carried to the artifacts so the operator
        // can log it from the run rather than from a second account read.
        deposit_krw: Some(deposit_krw),
    };

    let day = DayLoop {
        sdk,
        clock: crate::runner::live::shared::system_clock(),
        poll_interval: Duration::from_secs(envelope.poll_interval_secs.max(1)),
        decision_unix: points.decision_unix,
        auction_end_unix: points.auction_end_unix,
        session_end_unix: points.session_end_unix,
        decision_attempts: envelope.decision_read_attempts,
        session_date,
        session_index,
        universe: universe.rows,
        target_m: daily.target_m,
        ledger: mount.ledger,
        session: mount.handles.session.clone(),
        signals: mount.signals,
        held_book: mount.held_book,
        marks: mount.handles.marks.clone(),
        bars: mount.bars,
        heartbeats: mount.handles.heartbeats.clone(),
        observations,
        data_home: inputs.data_home.clone(),
        lane_hash: lane_hash.clone(),
        stop_before_orders: inputs.stop_before_orders,
        stop_atr_mult: daily.stop_atr_mult,
        run_id: authority.run_id.clone(),
        session_calendar: calendar_sessions,
    };

    Ok(PreparedRehearsal {
        node: mount.node,
        handles: mount.handles,
        driver,
        ctx,
        day,
        book_intent: mount.book_intent,
        previous_book: book,
        data_home: inputs.data_home.clone(),
        session_date: session_date_str,
        lane_hash,
        _live_lock: live_lock,
    })
}

/// Drive the prepared session, then write the book the next one inherits.
///
/// # Errors
///
/// A staging/finalize failure — the one class that leaves no artifacts.
pub async fn drive_rehearsal(prepared: PreparedRehearsal) -> anyhow::Result<ExitCode> {
    let PreparedRehearsal {
        mut node,
        handles,
        mut driver,
        ctx,
        day,
        book_intent,
        previous_book,
        data_home,
        session_date,
        lane_hash,
        // Bound to a name so it lives to the end of this function rather than being dropped
        // at the destructuring: the lock must outlast the book write, not the unpacking.
        _live_lock,
    } = prepared;

    println!(
        "rehearsal running: env=paper dispatch=none universe={} session_index={} \
         book_legs={} run_id={}",
        ctx.symbols.len(),
        day.session_index,
        previous_book.legs.len(),
        ctx.authority.run_id
    );

    // The day loop runs as a task on the NODE's runtime, interleaving with `node.run` at
    // its await points. It cannot be the driver's own future: `run_live_session` owns that
    // slot for the one seam that is never exercised offline.
    // The driver's timer is a RELATIVE sleep started here, but the session's end is an
    // ABSOLUTE KST instant, and `prepare_rehearsal` spent real time between the two: a
    // calendar read, a credential resolution, two paginated account reads, a deposit read and
    // the whole instrument master. Arming the timer on the span computed before all of that
    // makes the node stop that much LATE, while the day loop — which compares an absolute
    // instant against the clock — stands down exactly on time. The gap is a window in which
    // nothing feeds the runtime heartbeat, so a session that ran perfectly trips the dead-man
    // and leaves a standing trip that refuses the next mount. Re-derive it from the clock the
    // sleep will actually run against.
    let session_end_unix = day.session_end_unix;
    let now = Utc::now().timestamp();
    driver.session_secs = u64::try_from((session_end_unix - now).max(1)).unwrap_or(1);

    let session_probe = handles.session.clone();
    let day_task = tokio::spawn(day.run());

    let outcome = crate::runner::live::shared::run_live_session(
        handles,
        &driver,
        &ctx,
        crate::runner::live::shared::system_clock(),
        // THE live-only seam. Everything around it is offline-proven.
        move |_handle| async move { node.run().await },
    )
    .await?;

    // The loop is already finished in the ordinary case (it stands down at the session end,
    // which is when the driver's timer fires). Joining rather than aborting is what lets a
    // seam failure inside it reach the artifacts.
    // The loop records its own failure into `ctx.observations` before returning, so the
    // artifact already carries it by the time this runs (see `DayLoop::run`). What is decided
    // HERE is the exit code: a session whose decision loop failed or panicked did not do what
    // it was mounted to do, and `LiveSessionOutcome` cannot know that — it describes the node
    // and the teardown, both of which can be perfectly clean while no decision was ever taken.
    // Without this the operator gets exit 0 on a session that traded nothing.
    let (day_outcome, day_failed) = match day_task.await {
        Ok(Ok(outcome)) => (outcome, false),
        Ok(Err(_)) => (DayOutcome::default(), true),
        Err(e) => {
            if !e.is_cancelled() {
                ctx.observations
                    .note(format!("ABNORMAL: the rehearsal day loop panicked: {e}"));
            }
            // An empty outcome supplies no new leg facts, so a holding the broker reports
            // without them lands in `unattributed` and is reported rather than written —
            // which is the correct outcome for a session whose own record of what it did
            // was lost.
            (DayOutcome::default(), !e.is_cancelled())
        }
    };

    // KTD11: the book is a record of BROKER confirmation. `observed()` is `Some` only when
    // the teardown's book check positively confirmed, so an ABNORMAL session deliberately
    // leaves the previous book standing — the next mount then refuses on its stale stamp,
    // which is the correct outcome for an account nobody has reconciled.
    match book_intent.observed() {
        Some(snapshot) => {
            let unattributed =
                RehearsalBook::unattributed(&snapshot, &previous_book, &day_outcome.entered);
            // `closes` is instrument-keyed for the divergence rows; the book is shcode-keyed,
            // matching the broker's own `expcode`. The DECISION price backfills a symbol the
            // post-auction read never produced — it is this session's own observation and is
            // a far better day basis than the previous session's close, which is what the
            // field would otherwise keep while the stamp moved to today.
            let mut basis: HashMap<String, i64> = day_outcome
                .decision_prices
                .iter()
                .map(|(id, c)| (id.symbol.as_str().to_string(), *c))
                .collect();
            basis.extend(
                day_outcome
                    .closes
                    .iter()
                    .map(|(id, c)| (id.symbol.as_str().to_string(), *c)),
            );
            let (next, stale_basis) = RehearsalBook::from_snapshot(
                &snapshot,
                &previous_book,
                &day_outcome.entered,
                &basis,
                &session_date,
                &ctx.authority.run_id,
            );
            let path = next.write(&data_home)?;
            println!(
                "rehearsal book written: {} legs={} session_date={session_date}",
                path.display(),
                next.legs.len()
            );
            if !stale_basis.is_empty() {
                eprintln!(
                    "rehearsal WARNING: {} leg(s) kept the PREVIOUS session's day basis ({}) — \
                     this session read no price for them, so the next session's breaker will \
                     measure their move from a stale close",
                    stale_basis.len(),
                    stale_basis.join(", ")
                );
                ctx.observations.note(format!(
                    "day basis not advanced for {} leg(s): {}",
                    stale_basis.len(),
                    stale_basis.join(", ")
                ));
            }
            if !unattributed.is_empty() {
                eprintln!(
                    "rehearsal WARNING: the account reports {} holding(s) this book cannot \
                     attribute ({}) — they are NOT in the written book, so the next mount's probe \
                     will refuse until the book records them. Add each one's stop, entry date \
                     and label from the session that opened it, or clear the position at the \
                     broker. `lab-live --rehearsal-book adopt --why <text>` admits them from the \
                     account (nonce-gated).",
                    unattributed.len(),
                    unattributed.join(", ")
                );
            }
        }
        None => eprintln!(
            "rehearsal WARNING: the teardown could not confirm the account against the intended \
             book, so rehearsal/book.json was NOT rewritten — the previous session's book stands \
             and the next mount will refuse on its stamp until the account is reconciled"
        ),
    }

    // The session's own gateway dispatches, as a LOWER BOUND (the ladder's treatment).
    let observed = 2
        + outcome.report.cancel_attempts as usize
        + session_probe.ledger().lock().unwrap_or_else(|e| e.into_inner()).fills().len();
    let now_unix = Utc::now().timestamp();
    for i in 0..observed {
        crate::runner::live::shared::record_session_spend(
            &data_home,
            &lane_hash,
            now_unix + i as i64,
        )?;
    }

    let verdict = if outcome.abnormal { "ABNORMAL" } else { "clean" };
    println!(
        "rehearsal finalized ({verdict}): run_dir={} teardown_retries={} canceled={} \
         book_confirmed={} trip={:?} gateway_dispatches_recorded={observed}",
        outcome.run_dir.display(),
        outcome.report.retries(),
        outcome.report.canceled,
        outcome.report.flat_confirmed,
        outcome.trip
    );
    let (code, messages) = crate::runner::authority::mount_verdict(&outcome, true);
    for m in messages {
        eprintln!("{m}");
    }
    if day_failed && code == 0 {
        eprintln!(
            "rehearsal ABNORMAL: the node and the teardown were clean, but the day loop failed \
             before the session took its decision — see the run's data_quality observations. The \
             account is in whatever state the teardown confirmed; the book was written from that \
             confirmation, so the next mount is not blocked by this"
        );
        return Ok(ExitCode::from(crate::runner::live::shared::MOUNT_ABNORMAL));
    }
    Ok(ExitCode::from(code))
}

/// The mounted set: every held symbol plus every tradable candidate the sweep may reach.
///
/// A held symbol is mounted even when it is no longer a candidate and even when it is
/// designated (`tradable: false`): designation excludes a symbol from ENTRY only, and a
/// position that cannot be exited is a position the session cannot manage (R24).
///
/// # Errors
///
/// An shcode that is not a KRX short code.
pub fn mounted_instrument_ids(
    universe: &DailyUniverseFile,
    book: &RehearsalBook,
) -> anyhow::Result<Vec<InstrumentId>> {
    let mut ids: BTreeSet<InstrumentId> = BTreeSet::new();
    for leg in &book.legs {
        ids.insert(instrument_id_for(&leg.shcode)?);
    }
    for row in &universe.rows {
        ids.insert(instrument_id_for(&row.shcode)?);
    }
    Ok(ids.into_iter().collect())
}

/// Load the gateway's instrument master and keep the session's symbols.
///
/// # Errors
///
/// A master-read failure.
pub async fn load_session_instruments(
    sdk: &LsSdk,
    wanted: &[InstrumentId],
) -> anyhow::Result<Vec<InstrumentAny>> {
    let mut provider = nautilus_ls::instruments::InstrumentProvider::new(sdk.clone());
    provider
        .load_domestic_equities()
        .await
        .map_err(|e| anyhow::anyhow!("loading the KRX instrument master: {e}"))?;
    let keep: BTreeSet<InstrumentId> = wanted.iter().copied().collect();
    Ok(provider
        .all()
        .filter(|e| keep.contains(&e.id))
        .cloned()
        .map(InstrumentAny::Equity)
        .collect())
}

/// The pieces [`build_rehearsal_node`] assembles a session from.
pub struct RehearsalNodeParts<'a> {
    /// The credential lane config the node's clients resolve from.
    pub adapter_cfg: nautilus_ls::config::LsAdapterConfig,
    /// The SDK the probe already used — one SDK is one kill switch.
    pub sdk: LsSdk,
    /// The frozen daily parameter set.
    pub daily: crate::params_daily::DailyParams,
    /// The inherited book.
    pub book: &'a RehearsalBook,
    /// The session's instruments, published on connect.
    pub instruments: Vec<InstrumentAny>,
    /// The same instruments, indexed for the limit policy's clamp band.
    pub equities: Arc<SessionEquities>,
    /// The mounted universe.
    pub mounted_ids: Vec<InstrumentId>,
    /// The rehearsal home (its catalog checkpoint supplies the adjustment-basis ledger).
    pub data_home: PathBuf,
    /// This session's KST date (selects the tick ladder).
    pub session_date: NaiveDate,
    /// The previous proven session — where the seeded legs are stamped.
    pub previous_session: NaiveDate,
    /// The marketable-limit crossing offset `k`.
    pub marketable_limit_ticks: i64,
    /// Wall-clock unix seconds.
    pub now_unix: i64,
    /// This mount's ordered session list — the index space a restored leg's entry date is
    /// resolved against.
    pub session_calendar: Vec<NaiveDate>,
}

/// A built rehearsal session: the node plus every handle the driver, the day loop and the
/// finalize path need — all captured BEFORE the builder consumes them.
pub struct RehearsalMount {
    /// The built node.
    pub node: nautilus_live::node::LiveNode,
    /// The lane-neutral handle set [`run_live_session`] is driven through.
    pub handles: crate::runner::live::shared::LiveSessionHandles,
    /// The runner→strategy signal handle.
    pub signals: DailySessionSignals,
    /// The strategy→runner open-position book.
    pub held_book: OpenPositionBook,
    /// Where a synthetic bar enters the node.
    pub bars: DataEventSink,
    /// The teardown's book assertion.
    pub book_intent: crate::runner::live::shared::BookIntent,
    /// The session's shared fill ledger.
    pub ledger: Arc<std::sync::Mutex<nautilus_ls::orders::ledger::FillLedger>>,
}

/// Build the rehearsal's `LiveNode` with the frozen daily strategy mounted (U9).
///
/// **One SDK, one ledger** — the same invariant the ladder's build states, plus one more
/// that is specific here: the ledger is SEEDED with the inherited book before the node
/// starts, stamped at the PRIOR session (U8). A seed stamped today would make the run
/// observation count every inherited leg as an entry this session made.
///
/// # Errors
///
/// A node-builder / client-registration / strategy-mount / leg-restore failure.
pub fn build_rehearsal_node(parts: RehearsalNodeParts<'_>) -> anyhow::Result<RehearsalMount> {
    use nautilus_common::enums::Environment;
    use nautilus_model::identifiers::TraderId;

    let resolved = parts
        .adapter_cfg
        .build_config()
        .map_err(|e| anyhow::anyhow!("lane credentials: {e}"))?;
    let account_no = resolved.account_no.clone();
    let ledger: Arc<std::sync::Mutex<nautilus_ls::orders::ledger::FillLedger>> =
        Arc::new(std::sync::Mutex::new(nautilus_ls::orders::ledger::FillLedger::new()));

    // U8/KTD13: the breaker must see yesterday's exposure. Stamped at the prior session's
    // close, and at the prior session's CLOSE PRICE — see `seed_book_legs` for why the
    // entry price would make the breaker compare a multi-session drawdown against a
    // one-session threshold.
    let seeded_ns = u64::try_from(
        kst_instant(parts.previous_session, chrono::NaiveTime::from_hms_opt(15, 30, 0).unwrap())?
            .max(0),
    )
    .unwrap_or(0)
        * 1_000_000_000;
    let seeded = crate::runner::pnl::seed_book_legs(&ledger, &parts.book.pnl_legs(), seeded_ns);

    let marks = MarkFeed::new();
    let policy = limit_policy(
        parts.marketable_limit_ticks,
        parts.session_date,
        marks.clone(),
        Arc::clone(&parts.equities),
    );
    let exec = nautilus_ls::execution::LsExecClient::new_with_ledger(
        {
            use nautilus_common::factories::ExecutionClientFactory;
            // The builder derives the client name from the factory when `None` is passed,
            // so pre-building under the factory's own name keeps the node's client identity
            // byte-identical to the stateless path.
            nautilus_ls::factories::LsExecutionClientFactory::new().name().to_string()
        },
        parts.adapter_cfg.trader_id.clone(),
        account_no,
        parts.sdk.clone(),
        nautilus_model::enums::AccountType::Cash,
        Arc::clone(&ledger),
    )
    .with_start_posture(start_posture(parts.book))
    .with_marketable_limit_policy(policy);
    let order_tasks = exec.order_tasks();
    let exec_factory = nautilus_ls::factories::LsExecutionClientFactory::with_client(exec);

    let heartbeats = Heartbeats::new(parts.now_unix);
    let emission = crate::strategy::hooks::EmissionGate::open();
    let sink = crate::agent::sink::DecisionSink::new();
    let catalog = parts.data_home.join("catalog");
    let shifts = match crate::runner::backtest::load_checkpoint(&catalog) {
        Some(cp) => crate::strategy::daily::AdjustmentBasisShifts::from_checkpoint(&cp),
        None => crate::strategy::daily::AdjustmentBasisShifts::none(),
    };
    let mounted: Vec<MountedSymbol> = parts
        .mounted_ids
        .iter()
        .map(|id| {
            Ok(MountedSymbol { instrument_id: *id, bar_type: daily_bar_type(*id)? })
        })
        .collect::<anyhow::Result<_>>()?;

    let mut strategy = crate::strategy::daily::DailyStrategy::new(
        mounted,
        parts.daily,
        sink.clone(),
        shifts,
    )
    .with_emission_gate(emission.clone())
    .with_heartbeats(heartbeats.clone())
    .with_mark_feed(marks.clone())
    // KTD14: nautilus attributes an UNCLAIMED position to `EXTERNAL`, so the inherited
    // book would never reach this strategy's exit path. The claim covers the whole mounted
    // universe; an unexpected position outside it was already refused by the pre-mount probe.
    .with_external_order_claims(parts.mounted_ids.clone());
    strategy
        .seed_open_legs(&parts.book.strategy_legs(&parts.session_calendar)?)
        .map_err(|e| anyhow::anyhow!("restoring the rehearsal book into the strategy: {e}"))?;

    // Captured BEFORE `add_strategy` moves the strategy into the trader.
    let signals = strategy.session_signals();
    let held_book = strategy.open_position_book();

    let bars = DataEventSink::new();
    let data_factory = crate::runner::rehearsal_data::RehearsalDataClientFactory::new(
        bars.clone(),
        nautilus_model::identifiers::Venue::from(nautilus_ls::KRX_VENUE),
        parts.instruments,
    );

    let mut node = {
        // The logger-initializing build is serialized process-wide.
        let _guard = crate::runner::live::shared::node_build_lock();
        nautilus_live::node::LiveNode::builder(TraderId::from("LS-LAB-001"), Environment::Live)
            .map_err(|e| anyhow::anyhow!("live node builder: {e}"))?
            .with_name("ls-lab-rehearsal")
            .add_data_client(None, Box::new(data_factory), Box::new(parts.adapter_cfg.clone()))
            .map_err(|e| anyhow::anyhow!("data client: {e}"))?
            .add_exec_client(None, Box::new(exec_factory), Box::new(parts.adapter_cfg))
            .map_err(|e| anyhow::anyhow!("exec client: {e}"))?
            .build()
            .map_err(|e| anyhow::anyhow!("node build: {e}"))?
    };
    node.add_strategy(strategy)
        .map_err(|e| anyhow::anyhow!("mount the daily strategy: {e}"))?;
    let handle = node.handle();

    let book_intent = crate::runner::live::shared::BookIntent::new();
    eprintln!("rehearsal: seeded {seeded} inherited leg(s) into the session fill ledger");
    Ok(RehearsalMount {
        node,
        handles: crate::runner::live::shared::LiveSessionHandles {
            session: crate::runner::live::shared::LiveTeardownSession::new(
                emission,
                parts.sdk,
                Arc::clone(&ledger),
                order_tasks,
            )
            .with_book_intent(book_intent.clone()),
            heartbeats,
            handle,
            sink,
            marks,
        },
        signals,
        held_book,
        bars,
        book_intent,
        ledger,
    })
}
