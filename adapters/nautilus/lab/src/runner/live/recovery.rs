//! The REHEARSAL lane's recovery verbs (U13, R27): `lab-live --rehearsal-clear-trip` and
//! `lab-live --rehearsal-book adopt`.
//!
//! A rehearsal home has two pieces of state the next mount trusts without asking the
//! operator: the trip ledger (`rehearsal/trips.jsonl`, whose standing `Engage` refuses the
//! mount) and the book (`rehearsal/book.json`, whose legs the pre-mount probe demands the
//! account match). A trip, a crash, a late fill, a delisting or a resting residue can leave
//! either one wrong, and without these verbs the only repair is a hand edit — which bypasses
//! the attendance gate, leaves no audit row, and is exactly how a stop gets invented.
//!
//! Both verbs sit behind the SAME operator gate as the mount (no-TTY or no nonce is exit 77,
//! nothing written), and both write an append-only row saying who changed what and why.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chrono::{DateTime, NaiveDate, TimeZone, Timelike, Utc};
use ls_sdk::account::T0424OutBlock1;
use ls_sdk::LsSdk;
use nautilus_ls::execution::{
    cancel_all_resting_on, check_stranded_orders_on, collect_holdings_on, held_quantities,
};
use serde::{Deserialize, Serialize};

use super::rehearsal::{
    kst_session_date, lane_env_path_from_env, rehearsal_now_unix, resolve_probe_sdk,
    standing_trips,
};
use super::mount::operator_gate_from_env;
use super::shared::{MOUNT_NOT_PAPER, MOUNT_PRECHECK_FAILED, MOUNT_REFUSED_ATTEND};
use crate::dispatch::chain::TripAction;
use crate::runner::live_daily::{load_universe, RehearsalBook, RehearsalBookLeg};
use crate::runner::watchdog::{RehearsalLedger, RehearsalTrip, TripSink};

// ---------------------------------------------------------------------------
// --rehearsal-clear-trip
// ---------------------------------------------------------------------------

/// Write one `Clear` row per still-engaged mechanism, carrying the operator's why (R27).
///
/// Every standing mechanism is cleared, not just the first: the operator reconciled ONE
/// account, and a verb that released the breaker while leaving a dead-man `Engage` standing
/// would send them straight back to a refused mount with a second, identical command to run.
///
/// Refuses — writing nothing — on an empty why (a re-armed session with no recorded reason is
/// the audit gap the verb exists to close) and when nothing stands (a `Clear` with no
/// `Engage` before it is noise in the ledger the gate reads).
///
/// # Errors
///
/// An empty why, no standing trip, or a ledger read/append failure.
pub fn clear_standing_trips(
    ledger: &RehearsalLedger,
    why: &str,
    now: DateTime<Utc>,
) -> anyhow::Result<Vec<RehearsalTrip>> {
    let why = why.trim();
    if why.is_empty() {
        anyhow::bail!(
            "--rehearsal-clear-trip refused: --why <text> is required — re-arming a rehearsal \
             home after a safety trip must record who cleared it and why"
        );
    }
    let standing = standing_trips(ledger)?;
    if standing.is_empty() {
        anyhow::bail!(
            "--rehearsal-clear-trip refused: no standing trip in {} — nothing to clear, and \
             nothing was written",
            ledger.path().display()
        );
    }
    for trip in &standing {
        // The ledger scrubs the detail at write time.
        ledger.record_trip(
            trip.trip,
            TripAction::Clear,
            trip.run_id.as_deref(),
            &format!("operator clear: {why}"),
            now,
        )?;
    }
    Ok(standing)
}

// ---------------------------------------------------------------------------
// --rehearsal-book adopt
// ---------------------------------------------------------------------------

/// `<data_home>/rehearsal/book-adoptions.jsonl` — one row per difference an adoption made.
#[must_use]
pub fn adoption_ledger_path(data_home: &Path) -> PathBuf {
    data_home.join("rehearsal").join("book-adoptions.jsonl")
}

/// What an adoption row records.
///
/// An adoption LANDED iff its rows (same `adoption_id`) carry no [`AdoptionKind::Aborted`]
/// row — and then the book's `run_id` equals that `adoption_id`. The rows are appended before
/// the book is written so every landed change has its row; a write that then fails is
/// marked, not erased, because the ledger is append-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdoptionKind {
    /// The summary row every adoption writes: the why, the cancel count, the leg count.
    Adopted,
    /// A holding the previous book did not know, admitted with a derived stop.
    Admitted,
    /// A leg the previous book carried that the broker no longer holds.
    Dropped,
    /// A known leg whose quantity the broker reports differently.
    QuantityChanged,
    /// The previous book could not be read, so every holding was re-admitted.
    PreviousBookUnreadable,
    /// The rows above it were appended but the book write failed: this adoption did NOT land,
    /// and the book on disk is still the previous one.
    Aborted,
}

/// The KST wall time (hour, minute) from which an adoption is refused: the continuous
/// session's open.
///
/// Every written leg's `prior_close` — the breaker's DAY basis (KTD13) — is taken from the
/// t0424 `price` (현재가). That field is the last close only while the market is NOT trading:
/// before the open it is the previous session's close, and after the closing auction has
/// printed it is today's close, which is exactly the basis the next mount marks against.
/// Inside the window it is a live trade price, and a breaker based on it would fire (or fail
/// to) against a number no session closed at.
pub const ADOPT_REFUSED_FROM_KST: (u32, u32) = (9, 0);

/// The KST wall time (hour, minute) an adoption is allowed again: ten minutes past the 15:30
/// closing auction, so the auction's print — not a pre-auction trade — is what t0424 reports.
/// See [`ADOPT_REFUSED_FROM_KST`].
pub const ADOPT_REFUSED_UNTIL_KST: (u32, u32) = (15, 40);

/// One row of `rehearsal/book-adoptions.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdoptionRow {
    /// When the adoption ran (RFC-3339 UTC).
    pub at_utc: String,
    /// The adoption's id — also the `run_id` stamped on the book it wrote.
    pub adoption_id: String,
    /// What this row records.
    pub kind: AdoptionKind,
    /// The symbol, for a per-leg row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shcode: Option<String>,
    /// The quantity the previous book carried.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_quantity: Option<i64>,
    /// The quantity the broker reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broker_quantity: Option<i64>,
    /// Scrubbed free text.
    pub detail: String,
}

/// What a landed adoption did.
#[derive(Debug, Clone)]
pub struct AdoptionOutcome {
    /// The book that was written.
    pub book: RehearsalBook,
    /// The rows appended to the adoption ledger.
    pub rows: Vec<AdoptionRow>,
    /// How many resting orders the cancel pass canceled.
    pub canceled: usize,
}

/// Reconcile `rehearsal/book.json` with the account (R27, F4).
///
/// **The order is the safety property.**
///
/// 1. Cancel every t0425 resting order, then CONFIRM the list is empty with a second read. An
///    unconfirmed cancel refuses and writes nothing: a book rewritten over an account that
///    still has a working order records a state the next fill invalidates, and the next
///    mount's probe would refuse it anyway — after the operator believed the home repaired.
/// 2. Only then read the fully enumerated t0424 holdings.
/// 3. Rebuild the book: the broker is authoritative for membership and quantity; each known
///    leg's entry-fixed facts (entry, stop, entry date, label, opening order) carry forward
///    from the previous book. A known leg the broker no longer reports is dropped only against
///    an operator `why` — its entry-fixed facts exist nowhere else, and an empty or truncated
///    t0424 read would otherwise erase every one of them. A holding the previous book never
///    saw is admitted only against an operator `why`, on a day the calendar can place a
///    session on, at today's session date, with its stop derived by the frozen rule
///    (`entry − stop_atr_mult × ATR(1)`) from the session's universe file — so the next mount
///    does not refuse it for a missing field, and nothing about it is invented by hand. EVERY
///    written leg's `prior_close` (the breaker's day basis, KTD13, and the one non-entry-fixed
///    field) is re-based on the t0424 `price`, which is why the verb refuses to run inside
///    [`ADOPT_REFUSED_FROM_KST`]..[`ADOPT_REFUSED_UNTIL_KST`]: a carried-forward basis would be
///    stale under the new stamp, and an intraday price is no close at all.
/// 4. Append every difference to `rehearsal/book-adoptions.jsonl`, then write the book. A
///    failed write appends one `aborted` row for the same adoption (best-effort): the adoption
///    landed iff its rows carry no `aborted` row and the book's `run_id` is its id.
///
/// The session-window refusal and the unreadable-book refusal both fire before step 1, so
/// they cancel nothing. Every refusal after step 1 leaves the book and the adoption ledger
/// untouched. The cancel itself is not undone — it cannot be — but it is the one step that is
/// safe on its own.
///
/// **Admission needs a session day.** An admitted leg's entry date is the KST date the verb
/// runs on, and the next mount resolves that date against its session list — a date that is
/// not on it refuses the leg, and re-running the adoption on the same day would stamp the same
/// date again. So when there are holdings to admit, `is_candidate_session` is asked about
/// today first and a `false` refuses, writing nothing; run it again on a trading day.
///
/// # Errors
///
/// A call inside the continuous-session window, an unreadable previous book with no why, a
/// failed or unconfirmed cancel, a failed holdings read, a dropped leg or an unknown holding
/// with no why, an admission on a non-session day (or a calendar that cannot say), an unknown
/// holding with no ATR, a held symbol with no usable t0424 price, an unusable derived leg, or
/// a write failure.
pub async fn adopt_book_on(
    sdk: &LsSdk,
    data_home: &Path,
    previous: anyhow::Result<RehearsalBook>,
    universe_path: Option<&Path>,
    why: Option<&str>,
    is_candidate_session: &dyn Fn(NaiveDate) -> anyhow::Result<bool>,
    now: DateTime<Utc>,
) -> anyhow::Result<AdoptionOutcome> {
    let kst = now.with_timezone(&chrono_tz::Asia::Seoul);
    let wall = (kst.hour(), kst.minute());
    if wall >= ADOPT_REFUSED_FROM_KST && wall < ADOPT_REFUSED_UNTIL_KST {
        anyhow::bail!(
            "--rehearsal-book adopt refused: it is {:02}:{:02} KST, inside the {:02}:{:02}–{:02}:{:02} \
             session window — t0424's price is a live trade there, not a close, and every leg's \
             prior_close is re-based on it. Run it before the open or after the closing auction. \
             Nothing was canceled and nothing was written",
            wall.0,
            wall.1,
            ADOPT_REFUSED_FROM_KST.0,
            ADOPT_REFUSED_FROM_KST.1,
            ADOPT_REFUSED_UNTIL_KST.0,
            ADOPT_REFUSED_UNTIL_KST.1
        );
    }
    let why = why.map(str::trim).filter(|w| !w.is_empty());
    let (previous, previous_unreadable) = match previous {
        Ok(book) => (book, None),
        Err(e) if why.is_some() => (RehearsalBook::empty(""), Some(e.to_string())),
        Err(e) => anyhow::bail!(
            "--rehearsal-book adopt refused: the previous book cannot be read ({e}). Replacing it \
             re-admits every holding with a derived stop, so it needs --why <text>. Nothing was \
             canceled and nothing was written"
        ),
    };

    // 1. Cancel, then confirm.
    let canceled = cancel_all_resting_on(sdk).await.map_err(|e| {
        anyhow::anyhow!(
            "--rehearsal-book adopt refused: the cancel pass failed ({e}) — the account may still \
             have a working order, so the book was not rewritten"
        )
    })?;
    check_stranded_orders_on(sdk).await.map_err(|e| {
        anyhow::anyhow!(
            "--rehearsal-book adopt refused: the cancel was not confirmed ({e}) — t0425 still does \
             not read empty after canceling {canceled} order(s), so the book was not rewritten"
        )
    })?;

    // 2. The holdings.
    let rows = collect_holdings_on(sdk)
        .await
        .map_err(|e| anyhow::anyhow!("--rehearsal-book adopt refused: the holdings read failed ({e})"))?;
    let snapshot = held_quantities(&rows)
        .map_err(|e| anyhow::anyhow!("--rehearsal-book adopt refused: {e}"))?;
    let average_prices = average_prices(&rows);
    let closes = closing_prices(&rows);

    // 3. The book.
    let session_date = kst_session_date(now).format("%Y-%m-%d").to_string();
    let adoption_id = format!("adopt-{}", now.format("%Y%m%dT%H%M%SZ"));
    let known: HashMap<&str, &RehearsalBookLeg> =
        previous.legs.iter().map(|l| (l.shcode.trim(), l)).collect();
    // `T0424Response.outblock1` defaults to empty, so a read that came back short is
    // indistinguishable here from a flat account — and a dropped leg's entry, stop and entry
    // date exist nowhere else. Losing them is irrecoverable, so it is the operator's call.
    let dropped: Vec<&str> = previous
        .legs
        .iter()
        .map(|l| l.shcode.trim())
        .filter(|s| !snapshot.contains_key(*s))
        .collect();
    if !dropped.is_empty() && why.is_none() {
        anyhow::bail!(
            "--rehearsal-book adopt refused: the book carries {} leg(s) the broker no longer reports \
             ({}). Dropping one erases entry-fixed facts nothing else records — and an empty or \
             incomplete t0424 read looks exactly like this — so it needs --why <text>. The resting \
             orders were canceled; the book was not rewritten",
            dropped.len(),
            dropped.join(", ")
        );
    }
    let unknown: Vec<&str> = snapshot
        .keys()
        .map(|s| s.trim())
        .filter(|s| !known.contains_key(s))
        .collect();
    if !unknown.is_empty() && why.is_none() {
        anyhow::bail!(
            "--rehearsal-book adopt refused: the account holds {} symbol(s) the book does not know \
             ({}). Admitting one records a stop the session never set, so it needs --why <text>. \
             The resting orders were canceled; the book was not rewritten",
            unknown.len(),
            unknown.join(", ")
        );
    }
    if !unknown.is_empty() {
        let today = kst_session_date(now);
        if !is_candidate_session(today)? {
            anyhow::bail!(
                "--rehearsal-book adopt refused: today is not a trading session the calendar can \
                 place a leg on ({today}) — an admitted leg is dated today, and the next mount \
                 refuses a leg whose entry date is not in its session list. Admit {} on a trading \
                 day. The resting orders were canceled; the book was not rewritten",
                unknown.join(", ")
            );
        }
    }
    let universe = if unknown.is_empty() {
        None
    } else {
        let path = universe_path.ok_or_else(|| {
            anyhow::anyhow!(
                "--rehearsal-book adopt refused: admitting {} needs each symbol's ATR(1) for its \
                 stop, and LS_REHEARSAL_UNIVERSE_FILE is not set. The book was not rewritten",
                unknown.join(", ")
            )
        })?;
        let file = load_universe(path, kst_session_date(now))
            .map_err(|e| anyhow::anyhow!("--rehearsal-book adopt refused: {e}"))?;
        Some(file.rows)
    };
    let stop_atr_mult = crate::params_daily::DailyParams::frozen().stop_atr_mult;

    let mut legs: Vec<RehearsalBookLeg> = Vec::new();
    let mut unpriceable: Vec<String> = Vec::new();
    for (symbol, qty) in &snapshot {
        let symbol = symbol.trim();
        let Some(prior_close) = closes.get(symbol).copied() else {
            unpriceable.push(format!("{symbol} (no usable t0424 price for its prior_close)"));
            continue;
        };
        if let Some(leg) = known.get(symbol) {
            legs.push(RehearsalBookLeg { quantity: *qty, prior_close, ..(*leg).clone() });
            continue;
        }
        let row = universe.as_deref().and_then(|u| u.iter().find(|r| r.shcode.trim() == symbol));
        let entry = average_prices.get(symbol).copied();
        match (row, entry) {
            (Some(row), Some(entry)) if row.prior_atr1.is_finite() && row.prior_atr1 > 0.0 => {
                legs.push(RehearsalBookLeg {
                    shcode: symbol.to_string(),
                    quantity: *qty,
                    entry_price: entry,
                    stop_price: entry - stop_atr_mult * row.prior_atr1,
                    prior_close,
                    entry_date: session_date.clone(),
                    entered_under: adoption_id.clone(),
                    opening_order_id: format!("{adoption_id}-{symbol}"),
                });
            }
            (None, _) => unpriceable.push(format!("{symbol} (no ATR in the universe file)")),
            (Some(_), None) => unpriceable.push(format!("{symbol} (no usable t0424 pamt)")),
            (Some(_), Some(_)) => unpriceable.push(format!("{symbol} (a non-positive ATR)")),
        }
    }
    if !unpriceable.is_empty() {
        anyhow::bail!(
            "--rehearsal-book adopt refused: no usable leg can be built for {} — every leg needs \
             t0424's price as its prior_close, and an admitted one's stop (entry − {stop_atr_mult} \
             x ATR(1)) needs both the broker's average price and the universe's ATR. The book was \
             not rewritten",
            unpriceable.join(", ")
        );
    }
    let book = RehearsalBook { run_id: adoption_id.clone(), legs, ..RehearsalBook::empty(&session_date) };
    book.validate_fields()
        .map_err(|e| anyhow::anyhow!("--rehearsal-book adopt refused: the adopted book is unusable ({e})"))?;

    // 4. The rows, then the book.
    let at_utc = now.to_rfc3339();
    let row = |kind, shcode: Option<&str>, previous_quantity, broker_quantity, detail: String| AdoptionRow {
        at_utc: at_utc.clone(),
        adoption_id: adoption_id.clone(),
        kind,
        shcode: shcode.map(str::to_string),
        previous_quantity,
        broker_quantity,
        detail: nautilus_ls::scrub::scrub_secrets(&detail),
    };
    let why_text = why.unwrap_or("(none given)");
    let mut out: Vec<AdoptionRow> = Vec::new();
    if let Some(e) = &previous_unreadable {
        out.push(row(
            AdoptionKind::PreviousBookUnreadable,
            None,
            None,
            None,
            format!("the previous book could not be read ({e}); every holding was re-admitted"),
        ));
    }
    let held: BTreeSet<&str> = snapshot.keys().map(|s| s.trim()).collect();
    for leg in &previous.legs {
        let symbol = leg.shcode.trim();
        match snapshot.get(symbol) {
            None => out.push(row(
                AdoptionKind::Dropped,
                Some(symbol),
                Some(leg.quantity),
                None,
                "the broker no longer holds this leg".to_string(),
            )),
            Some(qty) if *qty != leg.quantity => out.push(row(
                AdoptionKind::QuantityChanged,
                Some(symbol),
                Some(leg.quantity),
                Some(*qty),
                "the broker's quantity replaces the book's".to_string(),
            )),
            Some(_) => {}
        }
    }
    for leg in book.legs.iter().filter(|l| !known.contains_key(l.shcode.as_str())) {
        out.push(row(
            AdoptionKind::Admitted,
            Some(&leg.shcode),
            None,
            Some(leg.quantity),
            format!(
                "admitted at entry {} with stop {} (entry − {stop_atr_mult} x ATR(1)); why: {why_text}",
                leg.entry_price, leg.stop_price
            ),
        ));
    }
    out.push(row(
        AdoptionKind::Adopted,
        None,
        None,
        None,
        format!(
            "canceled {canceled} resting order(s); {} leg(s) across {} held symbol(s); why: {why_text}",
            book.legs.len(),
            held.len()
        ),
    ));
    let ledger_path = adoption_ledger_path(data_home);
    append_rows(&ledger_path, &out)?;
    if let Err(e) = book.write(data_home) {
        // Rows-then-book keeps every landed change recorded; the price is that a failed write
        // leaves rows describing an adoption that never landed. Mark it rather than leave the
        // ledger lying — best-effort, since the same disk may refuse this append too.
        let aborted = row(
            AdoptionKind::Aborted,
            None,
            None,
            None,
            format!("the book write failed ({e}); this adoption did not land"),
        );
        let marked = match append_rows(&ledger_path, std::slice::from_ref(&aborted)) {
            Ok(()) => "the adoption's rows are marked aborted".to_string(),
            Err(mark) => format!(
                "marking the adoption's rows aborted ALSO failed ({mark}) — rows for {adoption_id} \
                 describe an adoption that did not land"
            ),
        };
        anyhow::bail!(
            "--rehearsal-book adopt failed: the book was not rewritten ({}); {marked}. The resting \
             orders were canceled",
            nautilus_ls::scrub::scrub_secrets(&e.to_string())
        );
    }
    Ok(AdoptionOutcome { book, rows: out, canceled })
}

/// Each held symbol's t0424 `price` (현재가) as integer KRW — the first row for the symbol
/// that parses positive. Outside the continuous session this is the last close, which is why
/// the caller refuses to run inside it. A symbol with no such row is left out; the caller
/// refuses on it.
fn closing_prices(rows: &[T0424OutBlock1]) -> HashMap<String, i64> {
    let mut out: HashMap<String, i64> = HashMap::new();
    for row in rows {
        let Ok(price) = row.price.trim().parse::<f64>() else { continue };
        if !price.is_finite() || price.round() < 1.0 {
            continue;
        }
        out.entry(row.expcode.trim().to_string()).or_insert(price.round() as i64);
    }
    out
}

/// The broker's average entry price per symbol, quantity-weighted across `jangb` tranches.
/// A symbol whose price does not parse positive is left out — the caller refuses on it.
fn average_prices(rows: &[T0424OutBlock1]) -> HashMap<String, f64> {
    let mut sums: BTreeMap<String, (f64, i64)> = BTreeMap::new();
    for row in rows {
        let qty = row.janqty.trim().parse::<i64>().unwrap_or(0);
        let price = row.pamt.trim().parse::<f64>().unwrap_or(f64::NAN);
        if qty <= 0 {
            continue;
        }
        let entry = sums.entry(row.expcode.trim().to_string()).or_insert((0.0, 0));
        entry.0 += price * qty as f64;
        entry.1 += qty;
    }
    sums.into_iter()
        .map(|(s, (notional, qty))| (s, notional / qty as f64))
        .filter(|(_, p)| p.is_finite() && *p > 0.0)
        .collect()
}

fn append_rows(path: &Path, rows: &[AdoptionRow]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = String::new();
    for row in rows {
        text.push_str(&serde_json::to_string(row)?);
        text.push('\n');
    }
    // One write of every row, so a crash mid-append cannot leave half an adoption behind.
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(text.as_bytes())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// The operator commands
// ---------------------------------------------------------------------------

/// The value following `flag` in argv, if any.
fn argv_value(flag: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}

fn data_home_from_env() -> anyhow::Result<PathBuf> {
    std::env::var("LS_DATA_HOME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("LS_DATA_HOME is required (the rehearsal data home; ABSOLUTE path)"))
}

/// `lab-live --rehearsal-clear-trip --why <text>` (U13). Exit 77 on the operator gate, 71 on
/// any refusal (nothing written), 0 once every standing mechanism has its `Clear`.
///
/// Holds the Live advisory lock across the read-then-append, so a session that engages a new
/// trip between the two cannot have that fresh `Engage` released by a clear meant for the old.
///
/// # Errors
///
/// None in practice — every outcome is an exit code.
pub fn run_clear_trip_cli() -> anyhow::Result<ExitCode> {
    nautilus_ls::calendar::emit_startup_from_env("lab-live");
    let now_unix = rehearsal_now_unix();
    if let Err(e) = operator_gate_from_env(now_unix).authorize("rehearsal trip clear") {
        eprintln!("rehearsal-clear-trip refused: {}", nautilus_ls::scrub::scrub_secrets(&e));
        return Ok(ExitCode::from(MOUNT_REFUSED_ATTEND));
    }
    let now = Utc.timestamp_opt(now_unix, 0).single().unwrap_or_else(Utc::now);
    let result = data_home_from_env().and_then(|home| {
        let _lock = super::shared::live_guard(&home)?;
        clear_standing_trips(&RehearsalLedger::new(&home), &argv_value("--why").unwrap_or_default(), now)
    });
    match result {
        Ok(cleared) => {
            for trip in &cleared {
                println!(
                    "rehearsal-clear-trip: cleared the standing {:?} trip from {} (reason recorded, scrubbed)",
                    trip.trip, trip.at_utc
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("{}", nautilus_ls::scrub::scrub_secrets(&e.to_string()));
            Ok(ExitCode::from(MOUNT_PRECHECK_FAILED))
        }
    }
}

/// `lab-live --rehearsal-book adopt [--why <text>]` (U13). Exit 66 off the paper lane, 77 on
/// the operator gate, 71 on any refusal, 0 once the book is rewritten.
///
/// Takes the Live advisory lock for the whole reconciliation, for the reason the mount does:
/// the verb cancels, reads, then writes — and a session or a second adoption running between
/// those steps would make the book it writes a record of an account that has already moved.
///
/// # Errors
///
/// An unknown sub-verb, or a runtime build failure.
pub fn run_book_cli() -> anyhow::Result<ExitCode> {
    nautilus_ls::calendar::emit_startup_from_env("lab-live");
    if std::env::args().nth(2).as_deref() != Some("adopt") {
        anyhow::bail!("usage: lab-live --rehearsal-book adopt [--why <text>]");
    }
    if std::env::var("LS_TRADING_ENV").as_deref() != Ok("paper") {
        eprintln!("rehearsal-book adopt refused: LS_TRADING_ENV must be `paper`");
        return Ok(ExitCode::from(MOUNT_NOT_PAPER));
    }
    let now_unix = rehearsal_now_unix();
    if let Err(e) = operator_gate_from_env(now_unix).authorize("rehearsal book adoption") {
        eprintln!("rehearsal-book adopt refused: {}", nautilus_ls::scrub::scrub_secrets(&e));
        return Ok(ExitCode::from(MOUNT_REFUSED_ATTEND));
    }
    let now = Utc.timestamp_opt(now_unix, 0).single().unwrap_or_else(Utc::now);
    let why = argv_value("--why");
    let universe = std::env::var("LS_REHEARSAL_UNIVERSE_FILE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from);
    // Built before the closure so the loaded calendar outlives every view it hands out. Only
    // an admission consults it, so a home with no unknown holding needs no snapshot.
    let calendar = nautilus_ls::calendar::IngestCalendarContext::from_env(now);
    let is_candidate_session = |day: NaiveDate| -> anyhow::Result<bool> {
        let view = calendar.view().ok_or_else(|| {
            anyhow::anyhow!(
                "--rehearsal-book adopt refused: admitting a holding needs LS_CALENDAR_SNAPSHOT — \
                 the admitted leg is dated today, and only the calendar can say today is a session \
                 the next mount will find. The resting orders were canceled; the book was not \
                 rewritten"
            )
        })?;
        let range = nautilus_ls_calendar::DateRange::inclusive(day, day)
            .map_err(|e| anyhow::anyhow!("calendar range {day}..{day}: {e:?}"))?;
        let candidates = view
            .candidate_sessions_in(&range)
            .map_err(|e| anyhow::anyhow!("reading the calendar over {day}..{day}: {e:?}"))?;
        Ok(candidates.contains(&day))
    };
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let result = runtime.block_on(async {
        let home = data_home_from_env()?;
        let _lock = super::shared::live_guard(&home)?;
        let (sdk, _, _) = resolve_probe_sdk(&lane_env_path_from_env())?;
        adopt_book_on(
            &sdk,
            &home,
            RehearsalBook::load(&home),
            universe.as_deref(),
            why.as_deref(),
            &is_candidate_session,
            now,
        )
        .await
    });
    match result {
        Ok(outcome) => {
            println!(
                "rehearsal-book adopt: canceled {} resting order(s); wrote {} leg(s) stamped {}; {} \
                 adoption row(s) recorded",
                outcome.canceled,
                outcome.book.legs.len(),
                outcome.book.session_date,
                outcome.rows.len()
            );
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("{}", nautilus_ls::scrub::scrub_secrets(&e.to_string()));
            Ok(ExitCode::from(MOUNT_PRECHECK_FAILED))
        }
    }
}
