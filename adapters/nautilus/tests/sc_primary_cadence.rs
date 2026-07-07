//! Offline coverage for the off-by-default SC-primary cadence mechanism (U4, KTD-3/4/5).
//!
//! The mechanism is a pure cadence selector: OFF (the shipped default) leaves the
//! exec client polling at [`DEFAULT_POLL_CADENCE`] (poll authoritative, identical to
//! today); ON relaxes the poll to [`SC_PRIMARY_BACKSTOP_CADENCE`] so SC push-fills carry
//! the fill path and the poll becomes a fail-closed reconcile backstop. Its live
//! activation is U6 (verdict-gated); this file proves — before the scarce open-KRX
//! window — that shipping it is a no-op until flipped, that the poll is never disabled,
//! that the backstop respects the KTD-4 detection-latency ceiling, and that the
//! dual-source exactly-once dedup (which U6 makes load-bearing) holds regardless of
//! cadence.

use nautilus_ls::execution::{
    resolve_poll_cadence, DEFAULT_POLL_CADENCE, SC_FILL_DETECTION_CEILING,
    SC_PRIMARY_BACKSTOP_CADENCE,
};
use nautilus_ls::orders::ledger::{FillLedger, FillObservation};
use nautilus_model::enums::{OrderSide, OrderType, TimeInForce};
use nautilus_model::identifiers::{ClientOrderId, InstrumentId};
use nautilus_model::orders::{OrderAny, OrderTestBuilder};
use nautilus_model::types::{Price, Quantity};

/// Default (selector off): the exec client keeps the 2s poll cadence — shipping the
/// mechanism is a no-op until U6 flips it (KTD-5).
#[test]
fn selector_off_keeps_default_poll_cadence() {
    assert_eq!(
        resolve_poll_cadence(false),
        DEFAULT_POLL_CADENCE,
        "selector OFF must be byte-identical to today (poll authoritative)"
    );
}

/// Happy path (selector on): the exec client is constructed with the relaxed backstop
/// cadence, not the 2s default.
#[test]
fn selector_on_relaxes_to_backstop_cadence() {
    assert_eq!(resolve_poll_cadence(true), SC_PRIMARY_BACKSTOP_CADENCE);
    assert_ne!(
        resolve_poll_cadence(true),
        DEFAULT_POLL_CADENCE,
        "SC-primary must actually relax the cadence, not silently keep the default"
    );
}

/// Backstop invariant: SC-primary *relaxes* the poll, it never disables it — the
/// resolved cadence is strictly slower than the default (a genuine backstop, not a fill
/// path) yet still a finite, positive interval so the poll loop keeps reconciling.
#[test]
fn poll_loop_stays_a_genuine_backstop_never_disabled() {
    let backstop = resolve_poll_cadence(true);
    assert!(
        backstop > DEFAULT_POLL_CADENCE,
        "the backstop must be slower than the 2s default so SC (not poll) carries fills"
    );
    assert!(
        !backstop.is_zero(),
        "the poll loop is never disabled under SC-primary — only slowed"
    );
}

/// Detection ceiling (KTD-4): the poll loop consumes `reconcile_armed` and re-scans only
/// after `sleep(cadence)`, so the backstop cadence *is* the worst-case time-to-detect a
/// dropped SC fill. It must not exceed the stated ceiling — the maximum stale-state
/// window the bar strategy can tolerate.
#[test]
fn sc_primary_backstop_respects_the_detection_latency_ceiling() {
    assert!(
        SC_PRIMARY_BACKSTOP_CADENCE <= SC_FILL_DETECTION_CEILING,
        "backstop cadence {SC_PRIMARY_BACKSTOP_CADENCE:?} exceeds the detection ceiling \
         {SC_FILL_DETECTION_CEILING:?} — a dropped SC fill could stay invisible too long"
    );
}

fn order(client_id: &str, qty: i64, price: i64, side: OrderSide) -> OrderAny {
    OrderTestBuilder::new(OrderType::Limit)
        .instrument_id(InstrumentId::from("005930.XKRX"))
        .client_order_id(ClientOrderId::from(client_id))
        .side(side)
        .quantity(Quantity::from(qty))
        .price(Price::from(price.to_string().as_str()))
        .time_in_force(TimeInForce::Day)
        .build()
}

fn ledger_with(client_id: &str, qty: i64, price: i64, ord_no: &str) -> FillLedger {
    let mut led = FillLedger::new();
    led.register(order(client_id, qty, price, OrderSide::Buy), ord_no);
    led
}

/// Dedup regression under SC-primary (KTD-5, AE1): the same execution observed via SC
/// then poll (and vice versa) collapses to exactly one delta. The ledger's exactly-once
/// seam is cadence-independent — relaxing the poll (U6) makes this invariant *load-
/// bearing*, so it is asserted here at both arrival orderings as the mechanism's safety
/// precondition. (The full dedup matrix lives in `orders/ledger.rs` unit tests.)
#[test]
fn same_execution_via_both_lanes_emits_once_regardless_of_order() {
    // Poll first, then the same execution over SC1 → no second delta.
    let mut led = ledger_with("O-CAD-1", 100, 60_000, "1001");
    let out = led.apply(FillObservation::poll("1001", 30, 60_000, false));
    assert_eq!(out.deltas.len(), 1, "the poll fill emits once");
    let out = led.apply(FillObservation::sc("1001", 30, 60_100, "E1"));
    assert!(out.deltas.is_empty(), "the already-polled fill must not re-emit via SC");
    assert!(!out.reconcile_needed);

    // SC1 first, then the same execution surfaces on a poll row → no second delta.
    let mut led = ledger_with("O-CAD-2", 100, 60_000, "2001");
    let out = led.apply(FillObservation::sc("2001", 40, 60_000, "E9"));
    assert_eq!(out.deltas.len(), 1, "the SC fill emits once");
    let out = led.apply(FillObservation::poll("2001", 40, 60_000, false));
    assert!(out.deltas.is_empty(), "the already-SC'd fill must not re-emit via poll");
    assert!(!out.reconcile_needed);
}
