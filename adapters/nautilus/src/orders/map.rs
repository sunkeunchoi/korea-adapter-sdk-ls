//! Variant-keyed order-event mapping (KTD6).
//!
//! The mapping is keyed on the [`LsError`] **variant**, never `rsp_cd` alone —
//! this is the documented fail-open trap two reviewers caught in the F/O chain.
//! `ApiError` (a clean 2xx business rejection — the gateway placed nothing) maps to
//! a rejection; `Invalid` (client-side preflight) to a denial; `DuplicateOrder` is
//! a dedup hit and is dropped; `AmbiguousOrder`/`Http`/`Decode` **may have rested**
//! at the venue, so they hold a pending state and drive `Orders::reconcile`.
//! `Auth`/`RateLimited`/`Config` are pre-network failures (placed nothing) →
//! denial; `Parse`/`PaginationLimit`/`WebSocket` should not occur on the submit
//! path and — together with any future variant — fall to a **fail-closed** default
//! arm (pending + reconcile), never a rejection event.

use ls_core::LsError;
use ls_sdk::orders::{OrderState, ReconcileOutcome};

/// What to do with a submit/modify/cancel outcome, before consulting the venue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitAction {
    /// The gateway accepted the order — adopt the returned OrdNo and emit an
    /// accepted/submitted event.
    Accept,
    /// A clean business rejection — the gateway placed nothing. Emit a rejection.
    Reject,
    /// A client-side or pre-network failure — placed nothing. Emit a denial.
    Deny,
    /// A dedup reservation hit — drop (the original submit owns the order).
    DropDuplicate,
    /// The outcome may have rested at the venue — hold pending and reconcile.
    Pending,
}

/// Classify a submit/modify/cancel **error** into a [`SubmitAction`] (KTD6). The
/// `Ok` case is always [`SubmitAction::Accept`] and is handled by the caller.
pub fn classify_submit_error(err: &LsError) -> SubmitAction {
    match err {
        // Clean 2xx business rejection — placed nothing.
        LsError::ApiError { .. } => SubmitAction::Reject,
        // Client-side preflight rejection — no network call was made.
        LsError::Invalid { .. } => SubmitAction::Deny,
        // Dedup reservation hit — a concurrent identical submit is in flight.
        LsError::DuplicateOrder => SubmitAction::DropDuplicate,
        // May have rested at the venue — reconcile before deciding.
        LsError::AmbiguousOrder { .. } | LsError::Http(_) | LsError::Decode(_) => {
            SubmitAction::Pending
        }
        // Pre-network failures — placed nothing.
        LsError::Auth(_) | LsError::RateLimited | LsError::Config(_) => SubmitAction::Deny,
        // Should not occur on the submit path; fail closed (never a rejection) so a
        // possibly-resting order is reconciled, not silently dropped. Also catches
        // any future #[non_exhaustive] variant.
        LsError::Parse(_) | LsError::PaginationLimit(_) | LsError::WebSocket(_) => {
            SubmitAction::Pending
        }
        _ => SubmitAction::Pending,
    }
}

/// The event to emit after a reconciliation, given the original order action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcileEvent {
    /// The order was found accepted/resting → emit Accepted.
    Accepted,
    /// The order was found modified → emit Updated/Modified.
    Modified,
    /// The order was found canceled → emit Canceled.
    Canceled,
    /// The venue rejected the ORIGINAL action → emit a rejection appropriate to the
    /// action (a rejected cancel emits **cancel-rejected**, NOT canceled, and the
    /// order stays open).
    Rejected,
    /// The state could not be proven → stay pending + alert; never retry.
    StayPending,
}

/// Map a [`ReconcileOutcome`] to the event to emit. The caller pairs the result
/// with the original `OrderAction`: a `Rejected` on a cancel surfaces as
/// **cancel-rejected** (order stays open), never OrderCanceled.
pub fn classify_reconcile(outcome: ReconcileOutcome) -> ReconcileEvent {
    match outcome.state {
        OrderState::Accepted => ReconcileEvent::Accepted,
        OrderState::Modified => ReconcileEvent::Modified,
        OrderState::Canceled => ReconcileEvent::Canceled,
        OrderState::Rejected => ReconcileEvent::Rejected,
        // Unknown never authorizes a retry; a dedup Duplicate stays pending too.
        OrderState::Unknown | OrderState::Duplicate => ReconcileEvent::StayPending,
    }
}

/// Whether a reconcile outcome authorizes a retry of the original action. Mirrors
/// the SDK's `safe_to_retry` and is never `true` for `Unknown`.
pub fn may_retry(outcome: ReconcileOutcome) -> bool {
    outcome.safe_to_retry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_error_variant_maps_per_ktd6() {
        use SubmitAction::*;
        // 2xx business rejection — placed nothing.
        assert_eq!(
            classify_submit_error(&LsError::ApiError { code: "40510".into(), message: "reject".into() }),
            Reject
        );
        // Preflight — no network.
        assert_eq!(
            classify_submit_error(&LsError::Invalid { field: "OrdQty".into(), reason: "range".into() }),
            Deny
        );
        // Dedup reservation hit.
        assert_eq!(classify_submit_error(&LsError::DuplicateOrder), DropDuplicate);
        // May rest — pending + reconcile.
        assert_eq!(
            classify_submit_error(&LsError::AmbiguousOrder { code: String::new(), message: "5xx".into() }),
            Pending
        );
        assert_eq!(
            classify_submit_error(&LsError::WebSocket("x".into())),
            Pending
        );
        // Pre-network — denied.
        assert_eq!(classify_submit_error(&LsError::Auth("bad".into())), Deny);
        assert_eq!(classify_submit_error(&LsError::RateLimited), Deny);
        assert_eq!(classify_submit_error(&LsError::Config("x".into())), Deny);
        // Should-not-occur on submit — fail closed to pending, never Reject.
        assert_eq!(classify_submit_error(&LsError::Parse("x".into())), Pending);
        assert_eq!(classify_submit_error(&LsError::PaginationLimit(3)), Pending);
    }

    /// AE1 core: an ambiguous/transport (5xx) submit is NEVER a rejection — it must
    /// go to pending + reconcile.
    #[test]
    fn ambiguous_and_transport_never_map_to_reject() {
        for err in [
            LsError::AmbiguousOrder { code: String::new(), message: "5xx".into() },
            LsError::WebSocket("dropped".into()),
        ] {
            let a = classify_submit_error(&err);
            assert_eq!(a, SubmitAction::Pending);
            assert_ne!(a, SubmitAction::Reject);
            assert_ne!(a, SubmitAction::Deny);
        }
    }

    fn outcome(state: OrderState, retry: bool) -> ReconcileOutcome {
        ReconcileOutcome { state, safe_to_retry: retry }
    }

    #[test]
    fn reconcile_states_map_to_events() {
        assert_eq!(classify_reconcile(outcome(OrderState::Accepted, false)), ReconcileEvent::Accepted);
        assert_eq!(classify_reconcile(outcome(OrderState::Modified, false)), ReconcileEvent::Modified);
        assert_eq!(classify_reconcile(outcome(OrderState::Canceled, false)), ReconcileEvent::Canceled);
        // A rejected cancel maps to Rejected (the caller emits cancel-rejected, order stays open).
        assert_eq!(classify_reconcile(outcome(OrderState::Rejected, false)), ReconcileEvent::Rejected);
        // Unknown never authorizes retry and stays pending.
        let unknown = outcome(OrderState::Unknown, false);
        assert_eq!(classify_reconcile(unknown), ReconcileEvent::StayPending);
        assert!(!may_retry(unknown), "Unknown never authorizes retry");
    }
}
