//! Inbound frame routing by composite key.
//!
//! [`dispatch_frame_to_subscriber`] is the single named backpressure decision
//! point: it routes a decoded `body` `serde_json::Value` to the typed subscriber
//! registered under `lookup_key`, applying the per-entry overflow policy, and
//! returns a [`DispatchOutcome`] so the caller can perform the post-decision
//! `subscriptions` cleanup (this function deliberately does not take a
//! `subscriptions` reference).
//!
//! ## SECURITY — no raw frame text
//!
//! This module logs ONLY structured fields (`tr_cd`, `tr_key`, `lookup_key`,
//! `body_is_null`, outcome counters). It never logs the raw frame text or the
//! `body` value, because server ACK frames echo the bearer token in the header
//! and a logged frame would leak it. Every `tracing` call here is auditable
//! against that rule.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::Sender;

use super::overflow::LatestOnlySlot;
use ls_core::LsResult;

/// Outbound subscribe/unsubscribe message channel capacity.
/// All subscriptions share one sink; this value is ample for control-frame
/// muxing. Distinct from the per-subscriber capacity (`ws_channel_capacity`).
pub(crate) const WS_OUTBOUND_CHANNEL_CAPACITY: usize = 32;

/// Per-subscriber dispatch entry — either a bounded mpsc sender (`DropNewest`)
/// or a single-slot `Weak` reference (`LatestOnly`).
///
/// The dropped-frame counter (`Arc<AtomicU64>`) is shared with the inbound task
/// so overflow events accumulate without per-frame allocation.
pub(crate) enum DispatchEntry {
    /// Bounded channel with drop-newest overflow semantics. Third field is the
    /// effective channel capacity, for accurate structured tracing.
    DropNewest(Sender<LsResult<serde_json::Value>>, Arc<AtomicU64>, usize),
    /// Single-slot mailbox; the slot is held as `Weak` so consumer drop is
    /// detected when `weak.upgrade()` returns `None`.
    LatestOnly(std::sync::Weak<LatestOnlySlot>, Arc<AtomicU64>),
}

/// Outcome of a single [`dispatch_frame_to_subscriber`] call.
///
/// The caller uses this to decide whether to perform the post-decision
/// `subscriptions.remove` cleanup (which must stay inline — this function does
/// not take a `subscriptions` reference).
pub(crate) enum DispatchOutcome {
    /// Frame was delivered successfully to the subscriber channel.
    Delivered,
    /// Channel was full; the frame was dropped and the drop counter incremented.
    Dropped,
    /// Channel receiver was closed; dispatch entry removed, caller must clean up
    /// `subscriptions`.
    Closed,
}

/// Route a decoded frame `body` to the subscriber identified by `lookup_key`.
///
/// Handles both overflow arms (DropNewest full → counter increment; LatestOnly
/// overwrite → counter increment) and the closed/dropped-consumer paths. Does
/// NOT take `&self`. Does NOT touch `subscriptions`.
///
/// SECURITY: logs only structured fields; never the raw frame or `body` value.
pub(crate) fn dispatch_frame_to_subscriber(
    dispatch: &Arc<DashMap<String, DispatchEntry>>,
    lookup_key: &str,
    body: serde_json::Value,
    tr_cd_h: &str,
    tr_key_h: &str,
) -> DispatchOutcome {
    let Some(entry_ref) = dispatch.get(lookup_key) else {
        // No subscriber registered for this key — frame dropped. Logged at debug
        // so silent frame loss is diagnosable without noise on normal runs.
        tracing::debug!(
            tr_cd = %tr_cd_h,
            tr_key = %tr_key_h,
            lookup_key = %lookup_key,
            "ws dispatch: inbound frame with no registered subscriber — dropped"
        );
        return DispatchOutcome::Dropped;
    };
    // Records WHETHER the body is JSON null (registration-ACK shape) vs payload —
    // never the body value itself.
    tracing::debug!(
        tr_cd = %tr_cd_h,
        tr_key = %tr_key_h,
        body_is_null = body.is_null(),
        "ws dispatch: routing inbound frame to subscriber"
    );
    match entry_ref.value() {
        DispatchEntry::DropNewest(sender, counter, capacity) => {
            let sender = sender.clone();
            let counter = Arc::clone(counter);
            let capacity = *capacity;
            drop(entry_ref); // release DashMap read guard before the slow path
            match sender.try_send(Ok(body)) {
                Ok(()) => DispatchOutcome::Delivered,
                Err(TrySendError::Full(_)) => {
                    // No per-frame warn! — increment the cumulative counter.
                    counter.fetch_add(1, Ordering::Relaxed);
                    DispatchOutcome::Dropped
                }
                Err(TrySendError::Closed(_)) => {
                    let dropped = counter.load(Ordering::Relaxed);
                    tracing::warn!(
                        tr_cd = %tr_cd_h,
                        tr_key = %tr_key_h,
                        dropped_count = dropped,
                        channel_capacity = capacity as u64,
                        "ws dispatch: subscriber channel closed after backpressure drops"
                    );
                    dispatch.remove(lookup_key);
                    DispatchOutcome::Closed
                }
            }
        }
        DispatchEntry::LatestOnly(weak_slot, counter) => match weak_slot.upgrade() {
            None => {
                // Consumer dropped its Arc<LatestOnlySlot> — clean up.
                let dropped = counter.load(Ordering::Relaxed);
                drop(entry_ref);
                tracing::warn!(
                    tr_cd = %tr_cd_h,
                    tr_key = %tr_key_h,
                    dropped_count = dropped,
                    channel_capacity = 1u64,
                    "ws dispatch: LatestOnly subscriber dropped (consumer gone)"
                );
                dispatch.remove(lookup_key);
                DispatchOutcome::Closed
            }
            Some(slot_arc) => {
                let counter = Arc::clone(counter);
                drop(entry_ref); // release DashMap read guard
                match slot_arc.slot.try_lock() {
                    Ok(mut guard) => {
                        let was_some = guard.is_some();
                        *guard = Some(Ok(body));
                        drop(guard);
                        slot_arc.notify.notify_one();
                        if was_some {
                            counter.fetch_add(1, Ordering::Relaxed);
                            DispatchOutcome::Dropped
                        } else {
                            DispatchOutcome::Delivered
                        }
                    }
                    Err(_) => {
                        // Contention — rare since only one writer and one reader.
                        tracing::warn!(
                            tr_cd = %tr_cd_h,
                            tr_key = %tr_key_h,
                            "ws dispatch: LatestOnly slot contention, treating as transient drop"
                        );
                        counter.fetch_add(1, Ordering::Relaxed);
                        DispatchOutcome::Dropped
                    }
                }
            }
        },
    }
}
