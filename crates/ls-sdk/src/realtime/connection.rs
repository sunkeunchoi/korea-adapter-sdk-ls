//! Connection lifecycle: outbound forwarder, inbound dispatch task, and the
//! bounded auto-reconnect loop.
//!
//! These free functions take ownership of what they need (no `&self`) so the
//! spawned tasks never close over an `Arc<WsManager>` directly — they hold a
//! `Weak<WsManager>` and upgrade only when needed, which is what keeps reconnect
//! cycle-free (the manager is built via `Arc::new_cyclic`).
//!
//! ## SECURITY — no raw frame text
//!
//! The inbound task NEVER logs raw frame text at any level. Server ACK frames
//! echo the bearer token in their header, so a logged frame would leak it. On a
//! parse failure it logs a fixed string with no payload; on success it delegates
//! to [`super::dispatch`], which logs only structured fields. Audit every
//! `tracing` call in this file against that rule.

use std::sync::Arc;

use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use super::dispatch::{dispatch_frame_to_subscriber, DispatchEntry, DispatchOutcome};
use super::frame::composite_key;
use super::WsManager;

/// Total bounded reconnect attempts before the budget is declared exhausted.
pub(super) const RECONNECT_MAX_ATTEMPTS: u64 = 4;

type WsSink = futures::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;

type WsSource = futures::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

/// Spawn the outbound forwarder task that drains `rx` into the WS sink.
///
/// Takes ownership of both `rx` and `sink`, so neither can be used in
/// `ensure_connected` after this call (called after `replay_subscriptions`
/// releases its `&mut sink` borrow).
pub(super) fn spawn_outbound_forwarder(rx: tokio::sync::mpsc::Receiver<Message>, mut sink: WsSink) {
    tokio::spawn(async move {
        let mut rx = rx;
        while let Some(msg) = rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    });
}

/// Spawn the inbound dispatch task: read frames, route `body` JSON to typed
/// subscribers by composite key. On EOF, clear `tx` and spawn the bounded
/// reconnect loop.
///
/// SECURITY: never logs raw frame text — see the module header.
pub(super) fn spawn_inbound_task(
    dispatch: Arc<DashMap<String, DispatchEntry>>,
    weak: std::sync::Weak<WsManager>,
    stream: WsSource,
) {
    tokio::spawn(async move {
        let mut stream = stream;
        while let Some(frame) = stream.next().await {
            if let Ok(Message::Text(txt)) = frame {
                match serde_json::from_str::<serde_json::Value>(&txt) {
                    Ok(mut val) => {
                        // Move the body out instead of cloning per frame. Taken
                        // BEFORE the header reads so the `&mut val` borrow ends
                        // before the immutable header borrows. A missing key or
                        // non-object frame yields Null.
                        let body = val
                            .get_mut("body")
                            .map(serde_json::Value::take)
                            .unwrap_or(serde_json::Value::Null);
                        let tr_cd_h = val["header"]["tr_cd"].as_str().unwrap_or("");
                        let tr_key_h = val["header"]["tr_key"].as_str().unwrap_or("");
                        let lookup_key = composite_key(tr_cd_h, tr_key_h);
                        let outcome = dispatch_frame_to_subscriber(
                            &dispatch,
                            &lookup_key,
                            body,
                            tr_cd_h,
                            tr_key_h,
                        );
                        // Remove ghost subscription on channel close.
                        if matches!(outcome, DispatchOutcome::Closed) {
                            if let Some(ws) = weak.upgrade() {
                                ws.subscriptions.remove(&lookup_key);
                            }
                        }
                    }
                    Err(_) => {
                        // SECURITY: fixed string only — never the frame text.
                        tracing::warn!("ws: malformed frame received, skipping");
                    }
                }
            }
        }
        // Stream ended — clear tx so ensure_connected actually reconnects, then
        // spawn the bounded reconnect task.
        if let Some(arc_self) = weak.upgrade() {
            *arc_self.tx.lock().await = None;
            tracing::info!("ws: connection lost — spawning auto-reconnect task");
            spawn_reconnect_task(arc_self);
        }
    });
}

/// Spawn the bounded [`RECONNECT_MAX_ATTEMPTS`]-attempt auto-reconnect loop.
///
/// On exhaustion, delivers the terminal `LsError::WebSocket("reconnect budget
/// exhausted")` to every active subscriber and cleans both maps up.
///
/// Rust has no for/else — `reconnected` distinguishes "loop broke on success"
/// from "loop fell off the end (every attempt failed)".
pub(super) fn spawn_reconnect_task(arc_self: Arc<WsManager>) {
    tokio::spawn(async move {
        let mut reconnected = false;
        for attempt in 1..=RECONNECT_MAX_ATTEMPTS {
            tokio::time::sleep(tokio::time::Duration::from_secs(attempt)).await;
            match arc_self.ensure_connected().await {
                Ok(()) => {
                    tracing::info!(attempt, "ws: auto-reconnect succeeded");
                    reconnected = true;
                    break;
                }
                Err(e) => {
                    tracing::warn!(
                        attempt,
                        error = %e,
                        "ws: auto-reconnect attempt failed"
                    );
                }
            }
        }
        if !reconnected {
            tracing::error!("ws: reconnect budget exhausted — notifying all subscribers");
            let keys: Vec<String> = arc_self.dispatch.iter().map(|e| e.key().clone()).collect();
            for key in keys {
                match arc_self.dispatch.remove(&key) {
                    Some((_, DispatchEntry::DropNewest(sender, _, _))) => {
                        // `.await` to guarantee delivery of the terminal error.
                        // A closed receiver returns Err(Closed) — ignore it.
                        let _ = sender
                            .send(Err(ls_core::LsError::WebSocket(
                                "reconnect budget exhausted".into(),
                            )))
                            .await;
                        // Sender drops here — that terminates the subscriber stream.
                    }
                    Some((_, DispatchEntry::LatestOnly(weak_slot, _))) => {
                        if let Some(slot_arc) = weak_slot.upgrade() {
                            // Err written BEFORE close() so the consumer always
                            // drains it before observing the terminal None.
                            *slot_arc.slot.lock().await = Some(Err(ls_core::LsError::WebSocket(
                                "reconnect budget exhausted".into(),
                            )));
                            slot_arc.close();
                        }
                    }
                    None => {} // Already removed (consumer dropped concurrently).
                }
                arc_self.subscriptions.remove(&key);
            }
        }
    });
}
