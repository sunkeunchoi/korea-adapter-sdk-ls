//! Overflow policies for typed WS subscriptions — `DropNewest` and `LatestOnly`.
//!
//! `DropNewest` (the default) is a bounded `mpsc` channel: when the consumer
//! lags, the newest frames are dropped and counted. Its stream behavior lives in
//! [`super::stream`] (a `ReceiverStream`), so this module only owns the
//! `LatestOnly` single-slot mailbox and its consumer stream.
//!
//! ## The lost-wakeup fix (ported verbatim from the Migration Source)
//!
//! `LatestOnlyStream` holds a single, persistent wakeup registration
//! (`Notify::notified_owned` → an owned, `'static` `OwnedNotified`) across
//! `Pending` returns. Re-creating the `Notified` on every poll — the original
//! defect — deregisters the waker on `Pending`, so a `notify_one` from the
//! dispatch path stores a permit nobody observes and a parked consumer hangs
//! forever. Holding ONE `Notified` across polls is the load-bearing fix.
//!
//! Two further invariants travel with it:
//! - the stream exposes an explicit terminal `None` (via [`LatestOnlySlot::close`]
//!   + the sticky `done` latch), instead of pending forever after exhaustion or
//!   unsubscribe;
//! - the terminal `Err` is written into the slot BEFORE `close()`, so a consumer
//!   always drains it before observing `None` ("yields Err then ends").
//!
//! The owning tests below are deterministic poll-first tests with a counting
//! waker — NO timeout/timer anywhere in the observation path, because a timer
//! expiry is itself a re-poll that masks a lost wakeup. See
//! `docs/solutions/runtime-errors/timeout-wrapped-polls-mask-lost-notify-wakeup.md`
//! in the Migration Source.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::Poll;

use futures::{Future, Stream};
use tokio::sync::{Mutex, Notify};

use ls_core::LsResult;

/// Shared state for the `LatestOnly` overflow policy — a single-slot mailbox
/// backed by `Notify` for wake-on-write. The dispatch map holds a `Weak`
/// reference so consumer drop is detected when `weak.upgrade()` returns `None`.
pub(crate) struct LatestOnlySlot {
    /// The single most-recent frame (or terminal `Err`). `try_lock` is used
    /// on both the writer (dispatch) and reader (poll) sides; contention is
    /// rare (one writer, one reader) and never yields a premature `None`.
    pub(crate) slot: Mutex<Option<LsResult<serde_json::Value>>>,
    /// `Arc` so the consumer stream can hold an owned (`'static`) wakeup
    /// registration (`Notify::notified_owned`) across polls. Cloning this
    /// inner `Arc` is fine — the single-`Arc` drop-detection contract is about
    /// `Arc<LatestOnlySlot>` itself, never about its fields.
    pub(crate) notify: Arc<Notify>,
    /// Terminal signal. Once set, the stream drains any remaining value and
    /// then yields `None`. Set via [`LatestOnlySlot::close`] only.
    pub(crate) closed: AtomicBool,
}

impl LatestOnlySlot {
    /// Fresh open slot (empty, not closed). Sole construction path.
    pub(crate) fn new() -> Self {
        Self {
            slot: Mutex::new(None),
            notify: Arc::new(Notify::new()),
            closed: AtomicBool::new(false),
        }
    }

    /// Mark the slot terminal and wake a parked consumer.
    ///
    /// `closed` is stored with `Release` BEFORE the notify so the woken poll's
    /// `Acquire` load observes it. Call sites: reconnect-budget exhaustion
    /// (after writing the terminal `Err`), `unsubscribe_typed`, and same-key
    /// re-subscribe displacement.
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.notify.notify_one();
    }
}

/// Consumer-side stream for the `LatestOnly` overflow policy.
///
/// Holds an `Arc<LatestOnlySlot>` (the dispatch map holds `Weak`) and a
/// persistent wakeup registration: `Notify::notified_owned` returns an owned
/// (`'static`) future, so the registration survives `Pending` returns instead
/// of being dropped (and thereby deregistered) every poll — dropping it was the
/// lost-wakeup defect. The future is `!Unpin`, hence `Pin<Box<...>>`; the box is
/// allocated once per consumed notification, never on the parked path.
pub(crate) struct LatestOnlyStream {
    pub(crate) slot: Arc<LatestOnlySlot>,
    /// Persistent registration. `None` until the first `Pending` path needs
    /// one, and cleared after the future fires (`OwnedNotified` is fused — a
    /// fired future must be replaced, not re-polled).
    notified: Option<std::pin::Pin<Box<tokio::sync::futures::OwnedNotified>>>,
    /// Sticky end. Set when `Ready(None)` is first yielded so a racing dispatch
    /// writer (one that upgraded its `Weak` before entry removal) cannot
    /// resurrect the stream past its documented end.
    done: bool,
}

impl LatestOnlyStream {
    /// Sole construction path (used by `WsStream::latest_only` and the inline
    /// tests). Field changes land here so callers and test bodies stay untouched
    /// when the stream grows state.
    pub(crate) fn new(slot: Arc<LatestOnlySlot>) -> Self {
        Self {
            slot,
            notified: None,
            done: false,
        }
    }
}

impl Stream for LatestOnlyStream {
    type Item = LsResult<serde_json::Value>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.done {
            return Poll::Ready(None);
        }
        loop {
            // Drain first; decide termination only on an OBSERVED empty slot.
            // On lock contention we fall through to registration instead —
            // returning None on a contended lock could skip an undrained
            // terminal Err still sitting in the slot.
            if let Ok(mut guard) = this.slot.slot.try_lock() {
                if let Some(value) = guard.take() {
                    return Poll::Ready(Some(value));
                }
                // `Acquire` pairs with the `Release` store in `close()`.
                if this.slot.closed.load(Ordering::Acquire) {
                    this.done = true;
                    return Poll::Ready(None);
                }
            }
            // Park: take (or create) the persistent registration. Creating it
            // after the slot check is race-free because `notify_one` stores a
            // permit when no waiter is registered — the first poll of a fresh
            // future consumes that permit and loops back to re-drain.
            let notified = this
                .notified
                .get_or_insert_with(|| Box::pin(this.slot.notify.clone().notified_owned()));
            match notified.as_mut().poll(cx) {
                Poll::Ready(()) => {
                    // Fired futures are fused — replace, never re-poll.
                    this.notified = None;
                    continue;
                }
                // Registration PERSISTS across this return (the fix): the
                // dispatch path's notify_one wakes the stored waker instead of
                // depositing a permit nobody observes.
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Deterministic poll-first tests — the owners of the LatestOnly wakeup and
/// termination invariants.
///
/// Manual polling with a counting waker, no runtime, no timers, and crucially
/// NO timeout wrapper anywhere in the observation path: a timer-bounded poll
/// masks a lost wakeup because the timer expiry forces the re-poll that consumes
/// the stored `Notify` permit (the false-green that hid the original defect).
/// The integration tests in `tests/realtime_tests.rs` own eventual-delivery and
/// latest-value semantics; these own wake-on-write and Err-then-None.
#[cfg(test)]
mod tests {
    use std::pin::Pin;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;
    use std::task::{Context, Wake, Waker};

    use super::*;
    use ls_core::LsError;

    /// Counts `wake` calls — the discriminating signal for the lost-wakeup
    /// inversion tests: the broken implementation drops its registration on
    /// `Pending`, so the count stays 0 no matter what is delivered.
    struct CountingWaker(AtomicUsize);

    impl Wake for CountingWaker {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn new_slot() -> Arc<LatestOnlySlot> {
        Arc::new(LatestOnlySlot::new())
    }

    fn counting_waker() -> (Arc<CountingWaker>, Waker) {
        let counter = Arc::new(CountingWaker(AtomicUsize::new(0)));
        let waker = Waker::from(counter.clone());
        (counter, waker)
    }

    fn poll(
        stream: &mut LatestOnlyStream,
        waker: &Waker,
    ) -> Poll<Option<LsResult<serde_json::Value>>> {
        let mut cx = Context::from_waker(waker);
        Pin::new(stream).poll_next(&mut cx)
    }

    /// Mimics the dispatch hot path exactly: write the newest value, then
    /// `notify_one` (dispatch.rs releases its guard before notifying).
    fn deliver(slot: &LatestOnlySlot, value: LsResult<serde_json::Value>) {
        *slot.slot.try_lock().expect("test holds no slot guard") = Some(value);
        slot.notify.notify_one();
    }

    /// Mimics the reconnect-exhaustion delivery in connection.rs exactly:
    /// terminal `Err` written first, then `close()`.
    fn deliver_terminal_err(slot: &LatestOnlySlot) {
        *slot.slot.try_lock().expect("test holds no slot guard") =
            Some(Err(LsError::WebSocket("reconnect budget exhausted".into())));
        slot.close();
    }

    #[test]
    fn parked_consumer_woken_on_frame_delivery() {
        let slot = new_slot();
        let mut stream = LatestOnlyStream::new(slot.clone());
        let (counter, waker) = counting_waker();

        // Park FIRST, then inject — the choreography that discriminates.
        assert!(poll(&mut stream, &waker).is_pending());
        deliver(&slot, Ok(serde_json::json!({"seq": 1})));

        assert!(
            counter.0.load(Ordering::SeqCst) >= 1,
            "parked consumer was not woken by frame delivery (lost wakeup)"
        );
        match poll(&mut stream, &waker) {
            Poll::Ready(Some(Ok(v))) => assert_eq!(v["seq"], 1),
            other => panic!("expected the delivered frame, got {other:?}"),
        }
    }

    #[test]
    fn parked_consumer_observes_terminal_err_then_end() {
        let slot = new_slot();
        let mut stream = LatestOnlyStream::new(slot.clone());
        let (counter, waker) = counting_waker();

        assert!(poll(&mut stream, &waker).is_pending());
        deliver_terminal_err(&slot);

        assert!(
            counter.0.load(Ordering::SeqCst) >= 1,
            "parked consumer was not woken by exhaustion delivery (lost wakeup)"
        );
        match poll(&mut stream, &waker) {
            Poll::Ready(Some(Err(LsError::WebSocket(msg)))) => {
                assert!(
                    msg.contains("reconnect budget exhausted"),
                    "wrong error: {msg}"
                );
            }
            other => panic!("expected the terminal Err, got {other:?}"),
        }
        assert!(
            matches!(poll(&mut stream, &waker), Poll::Ready(None)),
            "stream must end after the terminal Err is consumed"
        );
    }

    #[test]
    fn close_without_error_ends_parked_stream() {
        // The unsubscribe_typed / handle-drop / displacement shape: close() with
        // no value in the slot.
        let slot = new_slot();
        let mut stream = LatestOnlyStream::new(slot.clone());
        let (counter, waker) = counting_waker();

        assert!(poll(&mut stream, &waker).is_pending());
        slot.close();

        assert!(
            counter.0.load(Ordering::SeqCst) >= 1,
            "parked consumer was not woken by close (lost wakeup)"
        );
        assert!(
            matches!(poll(&mut stream, &waker), Poll::Ready(None)),
            "stream must end after close"
        );
    }

    #[test]
    fn value_in_slot_drains_before_end() {
        // None must never skip an undelivered value: close() after a write still
        // yields the value first ("yields Err/value then ends").
        let slot = new_slot();
        let mut stream = LatestOnlyStream::new(slot.clone());
        let (_counter, waker) = counting_waker();

        deliver(&slot, Ok(serde_json::json!({"seq": 7})));
        slot.close();

        match poll(&mut stream, &waker) {
            Poll::Ready(Some(Ok(v))) => assert_eq!(v["seq"], 7),
            other => panic!("expected the undelivered frame before None, got {other:?}"),
        }
        assert!(matches!(poll(&mut stream, &waker), Poll::Ready(None)));
    }

    #[test]
    fn registration_rearms_after_each_delivery() {
        // Proves the wakeup mechanism survives a full deliver/consume cycle (a
        // fused-future implementation that is never replaced would wake once and
        // then park forever).
        let slot = new_slot();
        let mut stream = LatestOnlyStream::new(slot.clone());
        let (counter, waker) = counting_waker();

        assert!(poll(&mut stream, &waker).is_pending());
        deliver(&slot, Ok(serde_json::json!({"seq": 1})));
        assert!(counter.0.load(Ordering::SeqCst) >= 1);
        assert!(matches!(
            poll(&mut stream, &waker),
            Poll::Ready(Some(Ok(_)))
        ));

        let wakes_after_first_cycle = counter.0.load(Ordering::SeqCst);
        assert!(poll(&mut stream, &waker).is_pending()); // park again
        deliver(&slot, Ok(serde_json::json!({"seq": 2})));
        assert!(
            counter.0.load(Ordering::SeqCst) > wakes_after_first_cycle,
            "second delivery did not wake the re-parked consumer (registration not re-armed)"
        );
        match poll(&mut stream, &waker) {
            Poll::Ready(Some(Ok(v))) => assert_eq!(v["seq"], 2),
            other => panic!("expected the second frame, got {other:?}"),
        }
    }

    #[test]
    fn ended_is_terminal_even_against_late_writers() {
        // The done latch: once None is yielded, a racing dispatch writer that
        // upgraded its Weak before entry removal must not resurrect the stream.
        let slot = new_slot();
        let mut stream = LatestOnlyStream::new(slot.clone());
        let (_counter, waker) = counting_waker();

        slot.close();
        assert!(matches!(poll(&mut stream, &waker), Poll::Ready(None)));
        assert!(matches!(poll(&mut stream, &waker), Poll::Ready(None)));

        deliver(&slot, Ok(serde_json::json!({"seq": 99}))); // late writer
        assert!(
            matches!(poll(&mut stream, &waker), Poll::Ready(None)),
            "ended stream must stay ended (done latch)"
        );
    }

    #[test]
    fn contention_never_yields_none() {
        // The observed-empty rule: with closed set but the slot lock held by
        // another party, poll must fall through to registration and pend —
        // returning None here could skip an undrained terminal Err.
        let slot = new_slot();
        let mut stream = LatestOnlyStream::new(slot.clone());
        let (_counter, waker) = counting_waker();

        let guard = slot.slot.try_lock().expect("first lock");
        slot.close();
        assert!(
            poll(&mut stream, &waker).is_pending(),
            "poll must never yield None while the slot lock is contended"
        );
        drop(guard);
        assert!(
            matches!(poll(&mut stream, &waker), Poll::Ready(None)),
            "stream must end once the closed slot is observably empty"
        );
    }
}
