//! The three shared handles the daily runner and its strategy communicate through
//! (KTD16, U4), plus the [`DailyPathStrategy`] contract that exposes them.
//!
//! Every one is an `Arc<Mutex<..>>` newtype cloned off the strategy **before**
//! `BacktestEngine::add_strategy` consumes it, because that call takes the strategy by
//! value and the runner still needs to read what the strategy observes. They live
//! beside each other so the direction of travel is legible: the strategy writes
//! [`OpenPositionBook`], the runner writes [`DailySessionSignals`], and the runner
//! reads both between batches without the per-session position report R4 forbids.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use chrono::NaiveDate;
use nautilus_model::identifiers::{InstrumentId, PositionId};

use crate::artifacts::performance::ClientOrderEntryRiskLedger;

/// The shared open-position book (KTD16) — the single authority for both the
/// already-held exclusion and per-session batch membership.
///
/// The strategy writes it from its position callbacks; the runner clones a handle
/// off the strategy **before** `add_strategy` consumes it and reads the handle
/// between batches. This is not the R4 position report: it is a live view of
/// *which symbols are open right now*, which the cumulative cache read cannot
/// answer between batches without a per-session read.
///
/// Deriving the held set statically from entry date + hold length was rejected: it
/// blocks re-entry after an early stop-out and so violates R10.
#[derive(Debug, Clone, Default)]
pub struct OpenPositionBook {
    inner: Arc<Mutex<BookState>>,
}

#[derive(Debug, Default)]
struct BookState {
    /// The instruments currently holding an open position.
    open: BTreeSet<InstrumentId>,
    /// Every position id observed opening across the stream, in observation order.
    /// The runner compares this against the single post-`end()` cache read, which is
    /// the check that catches a Netting venue silently snapshotting earlier round
    /// trips out of the live index (KTD12).
    opened: Vec<PositionId>,
}

impl OpenPositionBook {
    /// A fresh, empty book.
    #[must_use]
    pub fn new() -> Self {
        OpenPositionBook::default()
    }

    /// Record a position opening on `instrument_id`.
    pub fn record_opened(&self, instrument_id: InstrumentId, position_id: PositionId) {
        let mut st = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        st.open.insert(instrument_id);
        st.opened.push(position_id);
    }

    /// Record the position on `instrument_id` closing.
    pub fn record_closed(&self, instrument_id: &InstrumentId) {
        let mut st = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        st.open.remove(instrument_id);
    }

    /// The instruments currently holding an open position.
    #[must_use]
    pub fn held(&self) -> BTreeSet<InstrumentId> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).open.clone()
    }

    /// Whether `instrument_id` currently holds an open position.
    #[must_use]
    pub fn is_held(&self, instrument_id: &InstrumentId) -> bool {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).open.contains(instrument_id)
    }

    /// Every position id observed opening across the stream, in observation order.
    #[must_use]
    pub fn opened_position_ids(&self) -> Vec<PositionId> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).opened.clone()
    }
}

// ---------------------------------------------------------------------------
// The shared per-session signal handle (U4 — KTD9, KTD11, KTD13, R22)
// ---------------------------------------------------------------------------

/// What the loop resolved for one session, published to the strategy before that
/// session's batch runs.
///
/// A daily strategy cannot derive any of this from its own bar stream. The batch
/// carries only the session's *taken* and *held* symbols, so a freshly taken symbol
/// arrives with no prior bars at all — and the stop's prior ATR is computed strictly
/// **before** the session (KTD9). Re-deriving it inside the strategy would need a
/// second full-catalog index, which R5 forbids; the ranked/taken/held triple and the
/// ordered session calendar are likewise loop state, not stream state.
#[derive(Debug, Clone, PartialEq)]
pub struct DailySessionContext {
    /// The session's ordinal in the run's in-range session list. **This** is the
    /// clock hold elapsed is counted on (R23) — never a bar-callback counter, so a
    /// duplicate bar cannot shorten a frozen hold.
    pub index: usize,
    /// The KST session date.
    pub date: NaiveDate,
    /// The session's ranked candidates, best first, in instrument order.
    pub ranked: Vec<InstrumentId>,
    /// The symbols this session's pre-batch step actually took (R10).
    pub taken: Vec<InstrumentId>,
    /// The symbols already holding an open position at the pre-batch step.
    pub held: Vec<InstrumentId>,
    /// Each candidate's prior ATR for this session. An absent key was not a
    /// candidate; a `None` value was a candidate with no derivable prior ATR. The
    /// stop fails closed on both (KTD9).
    pub prior_atr: HashMap<InstrumentId, Option<f64>>,
}

/// The shared per-session signal handle: the runner publishes one
/// [`DailySessionContext`] per session before that session's batch runs, plus the
/// ordered in-range session calendar once before the loop.
///
/// Same shared-handle pattern as [`OpenPositionBook`] (KTD16) and for the same
/// reason — the runner clones it off the strategy before `add_strategy` consumes
/// it. The direction of travel is the opposite one: the runner *writes*, the
/// strategy *reads*.
#[derive(Debug, Clone, Default)]
pub struct DailySessionSignals {
    inner: Arc<Mutex<SignalState>>,
}

#[derive(Debug, Default)]
struct SignalState {
    /// Every in-range session date, in order — the calendar a prospective hold
    /// window is measured on (R22).
    sessions: Vec<NaiveDate>,
    current: Option<DailySessionContext>,
}

impl DailySessionSignals {
    /// A fresh, empty handle.
    #[must_use]
    pub fn new() -> Self {
        DailySessionSignals::default()
    }

    /// Publish the ordered in-range session calendar (once, before the loop).
    pub fn publish_sessions(&self, sessions: Vec<NaiveDate>) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).sessions = sessions;
    }

    /// Publish one session's context (once per session, before its batch runs).
    pub fn publish_session(&self, ctx: DailySessionContext) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).current = Some(ctx);
    }

    /// The session currently being driven, if the loop has published one.
    #[must_use]
    pub fn current(&self) -> Option<DailySessionContext> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).current.clone()
    }

    /// The in-range session date at `index`.
    #[must_use]
    pub fn session_at(&self, index: usize) -> Option<NaiveDate> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).sessions.get(index).copied()
    }

    /// The number of in-range sessions published.
    #[must_use]
    pub fn session_count(&self) -> usize {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).sessions.len()
    }
}

/// A strategy this runner can drive: it must expose the shared open-position book
/// so the runner can read the held set between batches (KTD16), and the shared
/// entry-risk ledger so the runner can project it into cache-read order (KTD3).
pub trait DailyPathStrategy {
    /// A clone of the shared open-position book.
    fn open_position_book(&self) -> OpenPositionBook;

    /// A clone of the shared, client-order-keyed entry-risk ledger (KTD3, R12).
    /// The runner clones it **before** `add_strategy` consumes the strategy and
    /// projects it into the order of the single post-`end()` cache read.
    fn entry_risk_ledger(&self) -> ClientOrderEntryRiskLedger;

    /// A clone of the shared per-session signal handle the runner publishes each
    /// session's [`DailySessionContext`] into.
    ///
    /// Defaulted to a detached handle so a strategy that drives itself entirely off
    /// batch membership (U1's test-only always-enter strategy) needs no wiring: the
    /// runner still publishes, nothing reads. A strategy whose stop needs the prior
    /// ATR, whose hold is counted in loop-supplied session ordinals, or whose entry
    /// gate reads the prospective hold window must override it and return a clone of
    /// its own handle.
    fn session_signals(&self) -> DailySessionSignals {
        DailySessionSignals::new()
    }
}
