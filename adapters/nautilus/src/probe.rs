//! Staged SC live-probe support (U8, R12): observe the SC0/SC1 order-event lane
//! during an operator-gated in-window order and render a verdict.
//!
//! The probe is **operator-gated live behavior** — it is never run by the offline
//! gate. Its outcome settles the two empirical unknowns the design stays correct
//! under either way: whether the paper gateway delivers SC push frames at all, and
//! whether it tolerates the exec client's *second* concurrent WS session (KTD3).
//! Only the verdict formatter is unit-tested here; the lane it observes is the same
//! one certified offline by `tests/order_events.rs`.

use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::{timeout, Instant};

use crate::ws::rows::OrderEventMsg;

/// Which leg produced an observation. A resting (non-marketable) order can only ever
/// certify SC0 (it never fills), so a bare "silent" is uninterpretable without the
/// leg — the marketable leg is the only one that can witness an SC1 fill frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeLeg {
    /// The guarded resting-order chain (submit → modify → cancel). Certifies SC0.
    Resting,
    /// A small marketable buy + sign-aware close-out (bypasses the U6 guard under
    /// its own flag). The only leg that can witness an SC1 fill.
    Marketable,
}

/// What the SC lane delivered during the probe window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScObservation {
    /// SC0 accept frames seen.
    pub sc0_accepts: usize,
    /// SC1 fill frames seen.
    pub sc1_fills: usize,
    /// Whether the exec client's own (second) WS session was established (KTD3 — the
    /// gateway has not been exercised on two concurrent sessions per token).
    pub second_ws_session_ok: bool,
}

/// Drain order-event observations for `window`, counting SC0 accepts and SC1 fills.
pub async fn drain_observations(
    rx: &mut mpsc::UnboundedReceiver<OrderEventMsg>,
    window: Duration,
) -> (usize, usize) {
    let start = Instant::now();
    let (mut accepts, mut fills) = (0usize, 0usize);
    while start.elapsed() < window {
        match timeout(Duration::from_millis(250), rx.recv()).await {
            Ok(Some(OrderEventMsg::Accept { .. })) => accepts += 1,
            Ok(Some(OrderEventMsg::Fill(_))) => fills += 1,
            Ok(None) => break, // channel closed
            Err(_) => {}       // tick — keep waiting out the window
        }
    }
    (accepts, fills)
}

/// Render the probe verdict line — the record the operator files (smoke registry /
/// adapter README). Distinguishes SC0-seen / SC1-seen / silent PER LEG so a resting
/// "silent" is not misread as "SC frames don't arrive".
pub fn format_verdict(leg: ProbeLeg, obs: &ScObservation) -> String {
    let sc0 = if obs.sc0_accepts > 0 {
        format!("SC0-seen ({} accept frame(s))", obs.sc0_accepts)
    } else {
        "SC0-silent".to_string()
    };
    let sc1 = match leg {
        ProbeLeg::Resting => "SC1-n/a (a resting order never fills)".to_string(),
        ProbeLeg::Marketable if obs.sc1_fills > 0 => {
            format!("SC1-seen ({} fill frame(s)) — SC may become the primary fill source", obs.sc1_fills)
        }
        ProbeLeg::Marketable => "SC1-silent (poll remains authoritative)".to_string(),
    };
    let ws = if obs.second_ws_session_ok {
        "2nd-WS-session: tolerated"
    } else {
        "2nd-WS-session: NOT established (gateway may reject a concurrent session)"
    };
    format!("SC PROBE [{leg:?}]: {sc0}; {sc1}; {ws}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resting_leg_reports_sc0_and_marks_sc1_not_applicable() {
        let obs = ScObservation { sc0_accepts: 1, sc1_fills: 0, second_ws_session_ok: true };
        let v = format_verdict(ProbeLeg::Resting, &obs);
        assert!(v.contains("SC0-seen"), "{v}");
        assert!(v.contains("SC1-n/a"), "resting can't witness a fill: {v}");
        assert!(v.contains("tolerated"), "{v}");
    }

    #[test]
    fn resting_leg_silent_sc0_is_interpretable() {
        let obs = ScObservation { sc0_accepts: 0, sc1_fills: 0, second_ws_session_ok: false };
        let v = format_verdict(ProbeLeg::Resting, &obs);
        assert!(v.contains("SC0-silent"), "{v}");
        assert!(v.contains("NOT established"), "{v}");
    }

    #[test]
    fn marketable_leg_reports_sc1_fills() {
        let obs = ScObservation { sc0_accepts: 1, sc1_fills: 2, second_ws_session_ok: true };
        let v = format_verdict(ProbeLeg::Marketable, &obs);
        assert!(v.contains("SC1-seen (2 fill"), "{v}");
        assert!(v.contains("primary fill source"), "{v}");
    }

    #[test]
    fn marketable_leg_silent_keeps_poll_authoritative() {
        let obs = ScObservation { sc0_accepts: 0, sc1_fills: 0, second_ws_session_ok: true };
        let v = format_verdict(ProbeLeg::Marketable, &obs);
        assert!(v.contains("SC1-silent"), "{v}");
        assert!(v.contains("poll remains authoritative"), "{v}");
    }
}
