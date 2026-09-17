//! The rehearsal's day (U9): one heartbeat-bearing loop through the 15:00->15:33 session,
//! plus the session calendar its ordinals index into.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDate;
use ls_sdk::LsSdk;
use nautilus_model::identifiers::InstrumentId;

use crate::artifacts::data_quality::{HeldSymbolGap, RehearsalDivergence, RehearsalDivergenceKind};
use crate::runner::backtest_daily::{
    resolve_take, DailySessionContext, DailySessionSignals,
    OpenPositionBook,
};
use crate::runner::live::shared::{SessionClock, SessionObservations};
use crate::runner::mount_universe::DailyUniverseRow;
use crate::runner::rehearsal_data::DataEventSink;
use crate::runner::watchdog::Heartbeats;
use crate::strategy::hooks::MarkFeed;

use super::*;

// ---------------------------------------------------------------------------
// The day loop
// ---------------------------------------------------------------------------

/// Everything the day loop drives one rehearsal session with.
pub struct DayLoop {
    /// The shared SDK — the same one the pre-mount probe used.
    pub sdk: LsSdk,
    /// The injectable session clock (unix seconds).
    pub clock: SessionClock,
    /// How long to wait between sweeps.
    pub poll_interval: Duration,
    /// When the decision read happens.
    pub decision_unix: i64,
    /// When the closing auction clears.
    pub auction_end_unix: i64,
    /// When the loop stands down (the driver's timer stops the node at the same instant).
    pub session_end_unix: i64,
    /// How many times the decision read may be retried.
    pub decision_attempts: u32,
    /// The KST session date.
    pub session_date: NaiveDate,
    /// This session's ordinal, counted from [`SESSION_ORDINAL_EPOCH`].
    pub session_index: usize,
    /// The universe file's rows, in rank order.
    pub universe: Vec<DailyUniverseRow>,
    /// The strategy's entries-per-session target.
    pub target_m: usize,
    /// The runner→strategy signal handle.
    pub signals: DailySessionSignals,
    /// The strategy→runner open-position book.
    pub held_book: OpenPositionBook,
    /// The mark feed the sweep publishes into (the breaker's and the limit policy's input).
    pub marks: MarkFeed,
    /// Where a synthetic bar enters the node (KTD15).
    pub bars: DataEventSink,
    /// The dead-man feeder, touched on EVERY tick.
    pub heartbeats: Heartbeats,
    /// The typed rows this loop contributes to the run's data-quality report.
    pub observations: SessionObservations,
    /// Where the per-credential spend bucket lives.
    pub data_home: PathBuf,
    /// The credential lane hash the spend is bucketed under.
    pub lane_hash: String,
    /// Record the decision and stand down before any bar is delivered.
    pub stop_before_orders: bool,
    /// The session's shared fill ledger — read after the auction to tell an entry that
    /// filled from one that did not (AE6), and to recover each new leg's realized price.
    pub ledger: Arc<std::sync::Mutex<nautilus_ls::orders::ledger::FillLedger>>,
    /// The teardown handle, read ONLY to notice that the session has been halted.
    pub session: crate::runner::live::shared::LiveTeardownSession,
    /// The frozen stop multiple. The runner needs it to reproduce the entry-fixed stop of a
    /// leg the strategy just opened — see [`DayLoop::record_entries`] for why the formula is
    /// restated here rather than read off the strategy.
    pub stop_atr_mult: f64,
    /// The run this session finalizes under, stamped onto every leg it opens (KTD2's
    /// leg-level label).
    pub run_id: String,
    /// The ordered session calendar [`session_index`](Self::session_index) indexes into,
    /// published to the strategy once before the day (R22). It must extend PAST today by at
    /// least `holding_period_sessions`, or the adjustment-basis gate silently collapses its
    /// prospective window to a single day — see [`session_calendar`].
    pub session_calendar: Vec<NaiveDate>,
}

/// What the day loop resolved.
#[derive(Debug, Clone, Default)]
pub struct DayOutcome {
    /// The symbols the pre-batch take selected.
    pub taken: Vec<InstrumentId>,
    /// The decision-bar prices delivered, by instrument.
    pub decision_prices: HashMap<InstrumentId, i64>,
    /// Whether a decision read succeeded at all.
    pub decided: bool,
    /// How many synthetic bars reached the node.
    pub bars_delivered: usize,
    /// Whether the loop stood down before delivering any bar (`--stop-before-orders`).
    pub stopped_before_orders: bool,
    /// The legs this session OPENED, with the facts only the book remembers. The next
    /// book's membership still comes from the broker; these supply what the broker cannot.
    pub entered: Vec<RehearsalBookLeg>,
    /// This session's closing price per symbol — the NEXT session's breaker day basis
    /// (KTD13), read from the post-auction sweep.
    pub closes: HashMap<InstrumentId, i64>,
}

impl DayLoop {
    /// The sweep set: every held symbol, plus the top `2 x target_m` ranked candidates.
    ///
    /// The 2x is headroom, not a second selection rule: the take excludes already-held
    /// names, so a sweep of exactly `target_m` would come up short by however many of the
    /// top names are already held — and a candidate with no mark cannot be priced by the
    /// marketable-limit policy, so it would simply not be entered.
    fn sweep_set(&self, held: &BTreeSet<InstrumentId>) -> Vec<InstrumentId> {
        let mut out: BTreeSet<InstrumentId> = held.clone();
        for row in self.universe.iter().take(self.target_m.saturating_mul(2)) {
            if let Ok(id) = instrument_id_for(&row.shcode) {
                out.insert(id);
            }
        }
        out.into_iter().collect()
    }

    /// One sweep: read, publish marks, record spend. Never fails the session — a failed
    /// sweep is a gap in the marks, and the decision read has its own bounded retry.
    async fn sweep(&self, wanted: &[InstrumentId], now: i64) {
        let Ok(quotes) = sweep_quotes(&self.sdk, wanted).await else {
            return;
        };
        for (id, quote) in &quotes {
            self.marks.observe(
                id.symbol.as_str(),
                crate::strategy::orb::SymbolMark {
                    last_close: quote.price,
                    last_bar_unix: now,
                    // The mark feed's stop is the strategy's; the book's stops reach the
                    // breaker through `book_stop_floors`, never through here.
                    stop_price: None,
                },
            );
        }
        // A LOWER BOUND on true spend, exactly as the ladder treats its own (advisory,
        // deferrable): one recorded dispatch per batch actually sent.
        //
        // Written in ONE load/mutate/save round-trip rather than one per batch. Each call
        // reads the ledger file, mutates it and renames it back, and this runs on the same
        // single-threaded runtime that drives `node.run` and the session-side liveness check
        // — six synchronous file round-trips per tick is six pauses those tasks cannot
        // interleave with, on the one runtime whose stalling the watchdog exists to catch.
        let batches = wanted.len().div_ceil(T8407_BATCH);
        let _ = crate::runner::live::shared::record_session_spend_n(
            &self.data_home,
            &self.lane_hash,
            now,
            batches,
        );
    }

    /// Drive one rehearsal session.
    ///
    /// Never returns `Err` for a market condition: a failed sweep, an empty decision read,
    /// and a held symbol with no bar are all RECORDED outcomes (R33) — the session finalizes
    /// normally and the artifacts say what happened. Only a broken seam (no data event
    /// sender, an un-constructible bar type) errors, because those mean the session cannot
    /// have taken the decision it is about to claim.
    ///
    /// # Errors
    ///
    /// A bar-delivery seam failure. The error is ALSO recorded into `observations` before it
    /// is returned — see [`DayLoop::run`]'s wrapper — because the caller learns about it only
    /// after the artifacts are already written.
    pub async fn run(self) -> anyhow::Result<DayOutcome> {
        let observations = self.observations.clone();
        let result = self.run_inner().await;
        if let Err(e) = &result {
            // Recorded HERE, not by the caller. `drive_rehearsal` joins this task only after
            // `run_live_session` returns, and that call has already snapshotted
            // `ctx.observations` and written `data-quality.json` — so a note added there
            // lands in a handle nothing will read again. The one artifact an operator or a
            // gate opens would say the session was clean while its decision loop had failed.
            observations.note(format!(
                "ABNORMAL: the rehearsal day loop failed and the session did not complete its \
                 decision: {}",
                nautilus_ls::scrub::scrub_secrets(&e.to_string())
            ));
        }
        result
    }

    async fn run_inner(self) -> anyhow::Result<DayOutcome> {
        let mut outcome = DayOutcome::default();
        // Published ONCE, before the day — the same point in the sequence the backtest
        // publishes it. `hold_window_end` reads it to measure a prospective 16-session hold,
        // and an unpublished calendar makes that window collapse to `ctx.date`, which turns
        // the R22 adjustment-basis refusal into a same-day check that almost never fires.
        self.signals.publish_sessions(self.session_calendar.clone());
        let by_symbol: HashMap<String, InstrumentId> = self
            .universe
            .iter()
            .filter_map(|r| instrument_id_for(&r.shcode).ok().map(|id| (r.shcode.trim().to_string(), id)))
            .collect();

        // ---- Phase 1: the pre-decision sweep -------------------------------
        loop {
            let now = (self.clock)();
            // EVERY tick, before anything that can fail or block: the strategy touches the
            // runtime feeder only from `on_bar`, and this session's single bar is twenty
            // minutes away. Without this the dead-man trips on a healthy sweep — and a
            // throttled decision read would trip it while the retry was still in budget.
            self.heartbeats.touch_runtime(now);
            if now >= self.decision_unix {
                break;
            }
            if self.halted() {
                // A trip during the pre-decision sweep ends the session's usefulness: the
                // teardown has already run and the decision cannot be acted on.
                self.observations.note(
                    "the day loop stood down before the decision: a supervisor had already \
                     halted the session"
                        .to_string(),
                );
                return Ok(outcome);
            }
            let wanted = self.sweep_set(&self.held_book.held());
            self.sweep(&wanted, now).await;
            tokio::time::sleep(self.poll_interval).await;
        }

        // ---- Phase 2: the decision read ------------------------------------
        let held = self.held_book.held();
        let taken = resolve_take(
            &self.universe.iter().filter(|r| r.tradable).map(|r| r.shcode.trim().to_string()).collect::<Vec<_>>(),
            &held,
            &by_symbol,
            self.target_m,
        );
        outcome.taken = taken.clone();

        let mut wanted: BTreeSet<InstrumentId> = held.clone();
        wanted.extend(taken.iter().copied());
        let wanted: Vec<InstrumentId> = wanted.into_iter().collect();

        let mut quotes: Option<HashMap<InstrumentId, Quote>> = None;
        for attempt in 0..self.decision_attempts.max(1) {
            let now = (self.clock)();
            self.heartbeats.touch_runtime(now);
            match sweep_quotes(&self.sdk, &wanted).await {
                Ok(q) if !q.is_empty() => {
                    quotes = Some(q);
                    break;
                }
                Ok(_) | Err(_) => {
                    if attempt + 1 < self.decision_attempts.max(1) {
                        tokio::time::sleep(self.poll_interval).await;
                    }
                }
            }
        }

        let Some(quotes) = quotes else {
            // R33: no decision is a RECORDED outcome, never a trip. The session holds what
            // it holds, stands down at the session end, and its book is unchanged.
            self.observations.note(format!(
                "no decision: the {} decision read produced no usable t8407 row after {} \
                 attempt(s); this session took no entries and evaluated no stop — the holdings \
                 are unchanged and the next valid bar decides them",
                self.session_date,
                self.decision_attempts.max(1)
            ));
            self.wait_out(self.session_end_unix, &held.iter().copied().collect::<Vec<_>>())
                .await;
            return Ok(outcome);
        };

        // Publish BEFORE any bar reaches the strategy, so the session's first callback
        // already sees the ordinal, the take, and the prior ATRs its stop gate needs.
        self.signals.publish_session(DailySessionContext {
            index: self.session_index,
            date: self.session_date,
            ranked: self
                .universe
                .iter()
                .filter_map(|r| by_symbol.get(r.shcode.trim()).copied())
                .collect(),
            taken: taken.clone(),
            held: held.iter().copied().collect(),
            prior_atr: self
                .universe
                .iter()
                .filter_map(|r| by_symbol.get(r.shcode.trim()).map(|id| (*id, Some(r.prior_atr1))))
                .collect(),
        });

        for (id, quote) in &quotes {
            outcome.decision_prices.insert(*id, quote.price);
            self.marks.observe(
                id.symbol.as_str(),
                crate::strategy::orb::SymbolMark {
                    last_close: quote.price,
                    last_bar_unix: self.decision_unix,
                    stop_price: None,
                },
            );
        }

        // KTD12: a held symbol with no usable row KEEPS its holding. The gap is typed
        // because the backtest's policy for the same condition is to ABORT, and the
        // comparison report has to be able to count that divergence class.
        for id in held.iter().filter(|id| !quotes.contains_key(id)) {
            self.observations.gap(HeldSymbolGap {
                instrument_id: id.to_string(),
                session_date: self.session_date.to_string(),
                reason: "the 15:20 t8407 decision read returned no usable row (halt or empty \
                         quote) — the holding is KEPT and its stop/expiry is judged on the next \
                         valid bar"
                    .to_string(),
            });
        }

        if self.stop_before_orders {
            // The decision IS recorded (the signals were published and the marks observed);
            // what does not happen is the bar delivery that would make the strategy act on
            // it. Nothing has been submitted, so there is nothing to cancel.
            //
            // What this mode exists to observe — what t8407's `price` means between the
            // decision and the auction clear — is a PAIR of reads, and the second one is the
            // post-auction close. Session 1 of 2026-09-17 returned here before Phase 4, so
            // `decision_prices` and the take left with the process and the run could not
            // answer its own question. The branch now runs the same post-auction read the
            // trading path runs and writes the same typed rows, flagged observation-only;
            // the only thing it still skips is the bar delivery.
            outcome.stopped_before_orders = true;
            self.observations.note(format!(
                "--stop-before-orders: the {} decision was resolved and recorded ({} symbol(s) \
                 quoted, {} taken) and NO synthetic bar was delivered, so the strategy emitted \
                 no order. Between {} and {} KST the t8407 `price` is the last continuous trade \
                 or the auction's expected clearing price, and this mode is how that is observed \
                 without trading on it — see this run's decision_vs_close rows, which carry the \
                 {} price and the post-auction close for every quoted symbol",
                self.session_date,
                quotes.len(),
                taken.len(),
                kst_hhmm(self.decision_unix),
                kst_hhmm(self.auction_end_unix),
                kst_hhmm(self.decision_unix),
            ));
            self.wait_out(self.auction_end_unix, &wanted).await;
            outcome.closes = self.read_closes(&outcome).await;
            self.record_close_divergence(&outcome);
            self.wait_out(self.session_end_unix, &wanted).await;
            return Ok(outcome);
        }

        // ---- Phase 3: the bar reaches the node (KTD15) ----------------------
        for id in &wanted {
            let Some(quote) = quotes.get(id) else { continue };
            let bar = synthetic_bar(daily_bar_type(*id)?, quote, self.decision_unix)?;
            // A delivery failure is a SEAM failure, not a market condition: the strategy
            // never saw this symbol's bar, so the session cannot claim it decided on it.
            self.bars.send_bar(bar)?;
            outcome.bars_delivered += 1;
        }
        outcome.decided = true;

        // ---- Phase 4: wait out the auction, then measure the divergence -----
        self.wait_out(self.auction_end_unix, &wanted).await;
        outcome.closes = self.read_closes(&outcome).await;
        self.record_close_divergence(&outcome);
        // The ledger is read LAST, after the whole post-auction window has run. A closing
        // auction clears at 15:30 but its fills reach this process over the SC lane and the
        // t0425 poll, which can land seconds later. Recovering entries at the clear instant
        // therefore reads a ledger that does not yet contain them, and a symbol the broker
        // will confirm as held ends up in neither the previous book nor `entered` — so
        // `from_snapshot` drops it, the written book omits a position the account really
        // holds, and the NEXT mount's probe refuses on it as an unexpected holding. Waiting
        // costs nothing: the session is holding this window open regardless.
        self.wait_out(self.session_end_unix, &wanted).await;
        self.record_entries(&mut outcome);
        self.record_unfilled_entries(&outcome);
        Ok(outcome)
    }

    /// The legs this session OPENED, and the stop that makes them measurable (U9).
    ///
    /// Two things are recovered here, and they are the same fact read twice:
    ///
    /// - **The book's new legs.** A symbol the session entered is in the broker's snapshot
    ///   but in NO previous book, so without this every new entry would be dropped from the
    ///   written book and the next mount's probe would refuse on it. The broker still
    ///   decides membership and quantity; this supplies the entry price, the stop, the
    ///   ordinal and the label it cannot.
    /// - **The stop the risk join reads.** `join_live_risk` takes each trade's stop off the
    ///   PUBLISHED mark, and the strategy publishes a leg's stop from `on_bar` — which for a
    ///   newly entered symbol ran BEFORE the auction opened the position, so it published
    ///   `None`. Without this republication the session's own entries would carry no
    ///   `risk_capital`, and the observation's verdict statistic would be computed from the
    ///   inherited legs alone.
    ///
    /// **Why the stop formula is restated here.** The strategy fixes
    /// `stop = avg_px_open − stop_atr_mult × ATR` at the open and never re-derives it (R12).
    /// Reading it back off the strategy would need a new shared handle on `daily.rs`, which
    /// Sequencing item 5 forbids — the frozen head may not move after judgment. Restating it
    /// is safe precisely because both inputs are frozen: `stop_atr_mult` is a pre-registered
    /// term `DailyParams::validate` refuses to change, and the ATR is the universe file's
    /// `prior_atr1`, which is the same number the runner published as `prior_atr`. If either
    /// ever stops being frozen, this is a site that has to move with it.
    fn record_entries(&self, outcome: &mut DayOutcome) {
        let atr: HashMap<&str, f64> =
            self.universe.iter().map(|r| (r.shcode.trim(), r.prior_atr1)).collect();
        let filled = self.filled_entries();
        for id in &outcome.taken {
            let shcode = id.symbol.as_str();
            let Some((qty, avg_px)) = filled.get(shcode).copied() else { continue };
            let Some(prior_atr) = atr.get(shcode).copied() else { continue };
            let stop = avg_px - self.stop_atr_mult * prior_atr;
            if !(stop > 0.0) || !(avg_px > stop) {
                // A degenerate stop is not written as a leg: `seed_open_legs` would refuse
                // it next session, and refusing to RECORD it here is how the operator finds
                // out this session rather than at the next mount.
                self.observations.note(format!(
                    "{id} opened at {avg_px} with prior ATR {prior_atr}, which yields a \
                     non-positive entry-fixed risk — the leg is NOT written to the book and \
                     the next mount will refuse on the unattributed holding"
                ));
                continue;
            }
            // The stop the risk join reads, published against THIS session's close.
            self.marks.observe(
                shcode,
                crate::strategy::orb::SymbolMark {
                    last_close: outcome.closes.get(id).copied().unwrap_or(avg_px.round() as i64),
                    last_bar_unix: self.auction_end_unix,
                    stop_price: Some(stop.round() as i64),
                },
            );
            outcome.entered.push(RehearsalBookLeg {
                shcode: shcode.to_string(),
                quantity: qty,
                entry_price: avg_px,
                stop_price: stop,
                // The NEXT session's day basis (KTD13), not this one's.
                prior_close: outcome.closes.get(id).copied().unwrap_or(avg_px.round() as i64),
                entry_date: self.session_date.format("%Y-%m-%d").to_string(),
                entered_under: self.run_id.clone(),
                opening_order_id: format!("BOOK-{shcode}-{}", self.session_date),
            });
        }
    }

    /// Buy-side fills this session, by shcode: `(quantity, quantity-weighted price)`.
    ///
    /// The WEIGHTED price, not the last one: a closing auction can come back in pieces, and
    /// the strategy fixes its stop off `avg_px_open`. Taking one fill's price would put the
    /// book's stop a tick or two off the one the strategy is actually holding to.
    fn filled_entries(&self) -> HashMap<String, (i64, f64)> {
        let guard = self.ledger.lock().unwrap_or_else(|e| e.into_inner());
        let mut out: HashMap<String, (i64, f64)> = HashMap::new();
        for f in guard.fills() {
            if f.side != nautilus_model::enums::OrderSide::Buy || f.qty <= 0 {
                continue;
            }
            // A SEEDED leg is stamped at the prior session and is not an entry this session
            // made; a taken symbol carries no seed, so this is belt-and-braces for a book
            // that re-entered a symbol it had exited earlier the same session.
            if f.observed_ns < self.session_open_ns() {
                continue;
            }
            let e = out.entry(f.symbol.trim().to_string()).or_insert((0, 0.0));
            let notional = e.1 * e.0 as f64 + f.price as f64 * f.qty as f64;
            e.0 += f.qty;
            e.1 = notional / e.0 as f64;
        }
        out
    }

    /// The nanosecond stamp everything before this session carries less than.
    fn session_open_ns(&self) -> u64 {
        u64::try_from(self.decision_unix.max(0)).unwrap_or(0) * 1_000_000_000
    }

    /// AE6: an entry the closing auction did not fill.
    ///
    /// The frozen mechanism ALWAYS fills at the close, so an unfilled entry has no backtest
    /// counterpart at all — it is the divergence class with the largest effect on a
    /// session's realized take, and it is invisible in the performance report (a leg that
    /// never opened contributes no trade). Reading it off the ledger is unambiguous for a
    /// TAKEN symbol specifically: a take excludes already-held names, so the symbol carries
    /// no seeded leg and any buy fill against it must be this session's.
    fn record_unfilled_entries(&self, outcome: &DayOutcome) {
        let filled = self.filled_entries();
        for id in &outcome.taken {
            if filled.contains_key(id.symbol.as_str()) {
                continue;
            }
            self.observations.divergence(RehearsalDivergence {
                kind: RehearsalDivergenceKind::UnfilledEntry,
                instrument_id: id.to_string(),
                decision_price: outcome.decision_prices.get(id).copied(),
                realized_price: None,
                detail: "the session took this symbol on the decision bar but the closing \
                         auction filled none of it — the frozen mechanism always fills at the \
                         close, so this entry has no backtest counterpart"
                    .to_string(),
            });
        }
    }

    /// Whether a supervisor has already halted this session.
    ///
    /// The kill switch is the one trip signal reachable from here: `TripLatch` lives inside
    /// the driver, but every trip path engages the switch through the shared `LsSdk`, and the
    /// day loop holds a handle on the same one. Once it is off there is nothing left for this
    /// loop to do — orders are refused and the teardown has run — so continuing to sweep the
    /// gateway spends budget on a session that is already over.
    fn halted(&self) -> bool {
        !self.session.orders_enabled()
    }

    /// Heartbeat until `until`, refreshing `keep_fresh`'s marks as it goes.
    ///
    /// The refresh is not optional bookkeeping — it is what keeps the breaker's input a
    /// PRICE. After the decision the strategy publishes no further marks (it gets one bar per
    /// session), so without a refresh every mark ages past `MarkPolicy::max_mark_age_secs`
    /// within two minutes and `mark_price` falls to its stale branch: the stop for a leg that
    /// has one, and `avg_cost x (1 - worst_case_adverse_fraction)` — a 30% loss — for one that
    /// does not. Either way the breaker spends the auction reading a drawdown nothing in the
    /// market produced.
    ///
    /// It refreshes on a MULTIPLE of the poll interval rather than every tick: the heartbeat
    /// has to be fed on every tick, the mark only has to stay inside the freshness window, and
    /// the gateway's budget is cumulative. `REFRESH_EVERY` x `poll_interval` must stay under
    /// `max_mark_age_secs`; at the shipped 20 s interval that is 60 s against a 120 s window.
    async fn wait_out(&self, until: i64, keep_fresh: &[InstrumentId]) {
        const REFRESH_EVERY: u32 = 3;
        let mut tick: u32 = 0;
        loop {
            let now = (self.clock)();
            self.heartbeats.touch_runtime(now);
            if now >= until {
                return;
            }
            if self.halted() {
                return;
            }
            if !keep_fresh.is_empty() && tick % REFRESH_EVERY == 0 {
                self.sweep(keep_fresh, now).await;
            }
            tick = tick.wrapping_add(1);
            tokio::time::sleep(self.poll_interval).await;
        }
    }

    /// The KTD4 divergence: the decision was taken on the 15:20 price and executed at the
    /// 15:30 close, so the gap between them is measured on EVERY decided symbol — including
    /// the ones that did not move, because a divergence class with only its outliers
    /// recorded cannot be summarized.
    /// The post-auction sweep: the session's closing price per decided symbol.
    ///
    /// ONE read feeds three consumers — the divergence measurement, the day basis the next
    /// session's breaker marks against, and the mark the risk join reads. Two reads could
    /// disagree, and the book would then record a basis the divergence row contradicts.
    async fn read_closes(&self, outcome: &DayOutcome) -> HashMap<InstrumentId, i64> {
        let ids: Vec<InstrumentId> = outcome.decision_prices.keys().copied().collect();
        if ids.is_empty() {
            return HashMap::new();
        }
        match sweep_quotes(&self.sdk, &ids).await {
            Ok(q) => q.into_iter().map(|(id, quote)| (id, quote.price)).collect(),
            Err(_) => {
                self.observations.note(
                    "the post-auction close read failed — the 15:20-decision-vs-close \
                     divergence could not be measured for this session, and the legs it opened \
                     carry the decision price as their day basis"
                        .to_string(),
                );
                HashMap::new()
            }
        }
    }

    ///
    /// One row per DECIDED symbol, whether or not the post-auction read produced its close:
    /// the decision price is this session's own observation and is not dropped because the
    /// second read failed — the row then carries `realized_price: None` and says so. Under
    /// `--stop-before-orders` the rows are the whole point of the session, so they are
    /// written with the same shape and flagged observation-only: no order was sent, so the
    /// "realized" price is the auction's close as t8407 reports it, not a fill.
    fn record_close_divergence(&self, outcome: &DayOutcome) {
        let taken: BTreeSet<InstrumentId> = outcome.taken.iter().copied().collect();
        let mut ids: Vec<&InstrumentId> = outcome.decision_prices.keys().collect();
        ids.sort();
        for id in ids {
            let decision_price = outcome.decision_prices[id];
            let role = if taken.contains(id) { "TAKEN" } else { "HELD" };
            let how = if outcome.stopped_before_orders {
                "observed only (--stop-before-orders), no order sent"
            } else {
                "cleared in the closing auction"
            };
            let (realized, detail) = match outcome.closes.get(id).copied() {
                Some(close) => (
                    Some(close),
                    format!(
                        "{role}: decided on the {} bar at {decision_price} KRW, {how}; the \
                         post-auction t8407 price read {close} KRW ({:+.3}%)",
                        self.session_date,
                        (close - decision_price) as f64 / decision_price as f64 * 100.0
                    ),
                ),
                None => (
                    None,
                    format!(
                        "{role}: decided on the {} bar at {decision_price} KRW, {how}; the \
                         post-auction close read produced no row for it, so the divergence is \
                         unmeasured for this symbol",
                        self.session_date
                    ),
                ),
            };
            self.observations.divergence(RehearsalDivergence {
                kind: RehearsalDivergenceKind::DecisionVsClose,
                instrument_id: id.to_string(),
                decision_price: Some(decision_price),
                realized_price: realized,
                detail,
            });
        }
    }
}

/// A unix instant as a KST wall-clock `HH:MM`, for the human-readable notes.
///
/// The notes are scrubbed at write time and the scrub redacts any 6+-digit run, so a raw
/// unix timestamp in a note lands in the artifact as `***` — which is exactly what session 1
/// of 2026-09-17 recorded where it promised KST times.
#[must_use]
pub fn kst_hhmm(unix: i64) -> String {
    let kst = chrono::FixedOffset::east_opt(9 * 3600).expect("+09:00 is a valid offset");
    chrono::DateTime::<chrono::Utc>::from_timestamp(unix, 0)
        .map(|t| t.with_timezone(&kst).format("%H:%M").to_string())
        .unwrap_or_else(|| "??:??".to_string())
}

/// Count proven trading sessions in `[SESSION_ORDINAL_EPOCH, through]`, and report the last
/// one (KTD11).
///
/// Counts PROVEN sessions only: an `Unknown` day is not a session until it is proven one, so
/// counting it would move every later ordinal the moment the calendar resolved it. The hold
/// window is therefore measured on proven non-closed days exactly as KTD11 states.
///
/// # Errors
///
/// A calendar that is unavailable, or one whose window does not cover the epoch.
pub fn session_ordinal(
    view: &nautilus_ls_calendar::AsOfView<'_>,
    through: NaiveDate,
) -> anyhow::Result<(usize, Option<NaiveDate>)> {
    let epoch = NaiveDate::parse_from_str(SESSION_ORDINAL_EPOCH, "%Y-%m-%d")
        .map_err(|e| anyhow::anyhow!("the session-ordinal epoch is unparseable: {e}"))?;
    let range = nautilus_ls_calendar::DateRange::inclusive(epoch, through)
        .map_err(|e| anyhow::anyhow!("session-ordinal range {epoch}..{through}: {e:?}"))?;
    view.session_count(&range)
        .map_err(|e| anyhow::anyhow!("reading the calendar over {epoch}..{through}: {e:?}"))
}

/// The ordered session calendar the strategy measures a prospective hold on, and today's
/// index into it (R22, KTD11).
///
/// Three properties the strategy depends on, none of which a bare count supplies:
///
/// - **Today is IN it, at `session_index`.** The calendar can legitimately read `Unknown`
///   for today — the KRX witness is retrospective, so a session in progress is not yet
///   proven — and a list of proven sessions alone would therefore put the NEXT session at
///   today's index and date every entry one session early.
/// - **It extends PAST today**, and the forward half is built from CANDIDATE days, not proven
///   sessions. `hold_window_end` is `session_at(index + hold)`, so a list ending today makes
///   that `None`, collapses the window to `ctx.date`, and stops the R22 adjustment-basis
///   refusal firing for anything but a same-day shift. A proven session is retrospective by
///   construction — `DayStatus::TradingSession` needs a positive KRX witness — so no future
///   date is EVER proven and a forward window sourced from
///   [`sessions_in`](nautilus_ls_calendar::AsOfView::sessions_in) is empty on every real
///   calendar. Asking for proven future sessions is therefore not a strict gate, it is an
///   unsatisfiable one: it refuses every mount. The forward half asks the only question the
///   calendar can answer forward — which days are not proven shut — via
///   [`candidate_sessions_in`](nautilus_ls_calendar::AsOfView::candidate_sessions_in).
/// - **Its index space is the ordinal space.** Hold elapsed is a subtraction of two indices
///   into ONE sequence. The BACKWARD half stays proven-only so that subtraction is stable:
///   counting an `Unknown` past day would renumber every later index the moment the calendar
///   resolved it. A leg's entry index is never persisted — `RehearsalBook` stores the entry
///   DATE and [`RehearsalBook::strategy_legs`] re-derives the index against this list every
///   session, so a resolved `Unknown` moves entry and current together.
///
/// # Errors
///
/// A calendar read failure, or a horizon with fewer than `hold_sessions` candidate days
/// after today.
pub fn session_calendar(
    view: &nautilus_ls_calendar::AsOfView<'_>,
    today: NaiveDate,
    hold_sessions: usize,
) -> anyhow::Result<(Vec<NaiveDate>, usize)> {
    use nautilus_ls_calendar::DateRange;
    let epoch = NaiveDate::parse_from_str(SESSION_ORDINAL_EPOCH, "%Y-%m-%d")
        .map_err(|e| anyhow::anyhow!("the session-ordinal epoch is unparseable: {e}"))?;
    let horizon = view.calendar().coverage().materialized_through;
    let yesterday = today.pred_opt().ok_or_else(|| anyhow::anyhow!("{today} has no previous day"))?;

    // Backward: proven sessions only, so a persisted date always resolves to the same index.
    let past = DateRange::inclusive(epoch, yesterday)
        .map_err(|e| anyhow::anyhow!("session-calendar range {epoch}..{yesterday}: {e:?}"))?;
    let mut sessions = view
        .sessions_in(&past)
        .map_err(|e| anyhow::anyhow!("reading the calendar over {epoch}..{yesterday}: {e:?}"))?;
    let index = sessions.len();
    sessions.push(today);

    // Forward: candidate days, because nothing forward is provable.
    let ahead_count = if let Some(tomorrow) = today.succ_opt() {
        if tomorrow > horizon {
            0
        } else {
            let future = DateRange::inclusive(tomorrow, horizon).map_err(|e| {
                anyhow::anyhow!("session-calendar range {tomorrow}..{horizon}: {e:?}")
            })?;
            let candidates = view.candidate_sessions_in(&future).map_err(|e| {
                anyhow::anyhow!("reading the calendar over {tomorrow}..{horizon}: {e:?}")
            })?;
            let n = candidates.len();
            sessions.extend(candidates);
            n
        }
    } else {
        0
    };

    if ahead_count < hold_sessions {
        anyhow::bail!(
            "the calendar covers only {ahead_count} candidate trading day(s) after {today} \
             (horizon {horizon}), but a hold runs {hold_sessions} — the adjustment-basis window \
             would be clamped short and the R22 refusal would stop firing. Advance the calendar's \
             forward horizon before mounting"
        );
    }
    Ok((sessions, index))
}

