//! Mock WebSocket server for the realtime (S3_) suite.
//!
//! Ported from the Migration Source `crates/test-support/src/mock_support.rs` WS
//! helpers, stripped of the order-event scaffolding this slice does not exercise.
//! The server binds an ephemeral loopback port, accepts connections, records
//! every text frame it receives (so tests can assert subscribe/replay frames),
//! lets a test broadcast frames to all connected clients (so tests can push S3_
//! rows), and supports forcibly killing connections (so reconnect tests can
//! sever a live socket).
//!
//! The realtime tests inject `ws://127.0.0.1:<port>` through `LsConfig.ws_base_url`
//! (the single WS test seam), so the REAL connect / replay / dispatch / reconnect
//! code paths run against this server.

use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use tokio::sync::{broadcast, Mutex};
use tokio_tungstenite::tungstenite::Message;

/// A running mock WS server bound to an ephemeral loopback port.
///
/// Drop the handle to stop accepting new connections (the spawned accept loop is
/// detached; dropping `MockWsServer` drops the broadcast/kill senders).
pub struct MockWsServer {
    port: u16,
    /// Broadcast a JSON text frame to every connected client.
    broadcast_tx: broadcast::Sender<String>,
    /// Force every active connection closed (reconnect tests).
    kill_tx: broadcast::Sender<()>,
    /// Every text frame received from clients, accumulated across connections.
    received: Arc<Mutex<Vec<String>>>,
    /// `tr_cd`s the gateway is configured to REJECT (negative-control seam, KTD6).
    /// Fixed at construction; read inside the accept loop.
    rejected_tr_cds: Arc<Vec<String>>,
    /// The detached accept loop — aborting it closes the listening port so a
    /// reconnect can never succeed (reconnect-budget-exhaustion tests).
    accept_handle: tokio::task::JoinHandle<()>,
}

impl MockWsServer {
    /// Spawn a mock WS server on a random loopback port (rejects nothing).
    pub async fn start() -> Self {
        Self::start_rejecting(&[]).await
    }

    /// Spawn a mock WS server that REJECTS the given `tr_cd`s (the KTD6
    /// negative-control seam).
    ///
    /// The real LS gateway, on a bad `tr_cd`, replies with an error-shaped ACK
    /// frame rather than silently accepting. We model that: when a subscribe frame
    /// arrives whose `body.tr_cd` is in `reject`, the per-connection task
    /// immediately sends back, IN-BAND on the same socket, a frame
    ///
    /// ```json
    /// {"header":{"tr_cd":<cd>,"tr_key":<key>},
    ///  "body":{"rsp_cd":"IGW00001","rsp_msg":"rejected"}}
    /// ```
    ///
    /// The `header.tr_cd`/`header.tr_key` ECHO the subscribe, so the SDK inbound
    /// task routes the error `body` to THIS subscriber's stream by composite key.
    /// The SDK's subscribe is fire-and-forget and never reads an ACK, but the
    /// inbound dispatch DOES route any inbound frame matching a live composite key
    /// — so an in-band rejection surfaces to the subscriber as a delivered `body`
    /// it can inspect (e.g. a non-empty `rsp_cd`). A non-rejected `tr_cd` is
    /// recorded and otherwise ignored exactly as before, so an ACCEPTED subscribe
    /// is observably distinct (no inbound frame within the timebox) from a
    /// REJECTED one (an `rsp_cd` body arrives). This is what makes a smoke built on
    /// the gate able to FAIL.
    pub async fn start_rejecting(reject: &[&str]) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock ws listener");
        let port = listener.local_addr().expect("local addr").port();

        let received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let (broadcast_tx, _) = broadcast::channel::<String>(512);
        let (kill_tx, _) = broadcast::channel::<()>(16);
        let rejected_tr_cds: Arc<Vec<String>> =
            Arc::new(reject.iter().map(|s| s.to_string()).collect());

        let received_loop = Arc::clone(&received);
        let broadcast_loop = broadcast_tx.clone();
        let kill_loop = kill_tx.clone();
        let rejected_loop = Arc::clone(&rejected_tr_cds);

        let accept_handle = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let recv_inner = Arc::clone(&received_loop);
                let rejected = Arc::clone(&rejected_loop);
                let mut bcast_rx = broadcast_loop.subscribe();
                let mut kill_rx = kill_loop.subscribe();
                tokio::spawn(async move {
                    let mut ws = match tokio_tungstenite::accept_async(stream).await {
                        Ok(ws) => ws,
                        Err(_) => return,
                    };
                    loop {
                        tokio::select! {
                            frame = ws.next() => {
                                match frame {
                                    Some(Ok(Message::Text(t))) => {
                                        recv_inner.lock().await.push(t.to_string());
                                        // Negative-control path: if this subscribe
                                        // targets a rejected tr_cd, reply in-band
                                        // with an error-shaped ACK routed back to
                                        // the subscriber by composite key.
                                        if let Some(reply) = rejection_reply(&t, &rejected) {
                                            let _ = ws.send(Message::Text(reply.into())).await;
                                        }
                                    }
                                    _ => break,
                                }
                            }
                            result = bcast_rx.recv() => {
                                if let Ok(text) = result {
                                    let _ = ws.send(Message::Text(text.into())).await;
                                }
                            }
                            _ = kill_rx.recv() => break,
                        }
                    }
                });
            }
        });

        MockWsServer {
            port,
            broadcast_tx,
            kill_tx,
            received,
            rejected_tr_cds,
            accept_handle,
        }
    }

    /// Stop accepting connections AND sever active ones, then close the port.
    ///
    /// After this, a client reconnect attempt against [`Self::ws_url`] fails to
    /// connect — which is how a reconnect-budget-exhaustion test forces all four
    /// attempts to fail. The port may take a moment to free; tests that need a
    /// hard-dead port should also point reconnect at it only after `shutdown`.
    pub fn shutdown(&self) {
        let _ = self.kill_tx.send(());
        self.accept_handle.abort();
    }

    /// The `ws://127.0.0.1:<port>` URL clients connect to. Inject into
    /// `LsConfig.ws_base_url`.
    pub fn ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}", self.port)
    }

    /// Broadcast a raw JSON text frame to every connected client.
    ///
    /// Returns the number of clients the frame was queued for (0 if none are
    /// connected yet — broadcast does not buffer for future subscribers).
    pub fn push_frame(&self, json: impl Into<String>) -> usize {
        self.broadcast_tx.send(json.into()).unwrap_or(0)
    }

    /// Build and broadcast an S3_ push frame for `tr_key` with the given `body`
    /// object. The frame shape matches the LS gateway:
    /// `{"header":{"tr_cd":"S3_","tr_key":<key>},"body":<body>}`.
    pub fn push_s3(&self, tr_key: &str, body: serde_json::Value) -> usize {
        let frame = serde_json::json!({
            "header": { "tr_cd": "S3_", "tr_key": tr_key },
            "body": body,
        });
        self.push_frame(frame.to_string())
    }

    /// Force every active connection closed — drives a reconnect in the client.
    pub fn kill_connections(&self) {
        let _ = self.kill_tx.send(());
    }

    /// All text frames received from clients so far (subscribe/replay/unsubscribe
    /// frames), cloned out for assertion.
    pub async fn received_frames(&self) -> Vec<String> {
        self.received.lock().await.clone()
    }

    /// Count of received frames whose parsed JSON has `body.tr_cd == tr_cd`
    /// (i.e. subscribe/unsubscribe control frames for that TR).
    pub async fn count_subscribe_frames(&self, tr_cd: &str, tr_type: &str) -> usize {
        self.received
            .lock()
            .await
            .iter()
            .filter_map(|f| serde_json::from_str::<serde_json::Value>(f).ok())
            .filter(|v| {
                v["body"]["tr_cd"].as_str() == Some(tr_cd)
                    && v["header"]["tr_type"].as_str() == Some(tr_type)
            })
            .count()
    }

    /// The `tr_cd`s this server is configured to reject (empty unless built via
    /// [`Self::start_rejecting`]). Exposed for test introspection.
    pub fn rejected_tr_cds(&self) -> &[String] {
        &self.rejected_tr_cds
    }
}

/// Business `rsp_cd` carried by the mock gateway's rejection ACK. Mirrors the LS
/// `IGW`-family error prefix; the value is arbitrary but stable for assertions.
pub const MOCK_REJECTION_RSP_CD: &str = "IGW00001";

/// If `subscribe_text` is a subscribe frame for a rejected `tr_cd`, build the
/// error-shaped ACK reply (a JSON string), echoing the subscribe's `tr_cd`/
/// `tr_key` in the header so the SDK routes it back to the subscriber by
/// composite key. Returns `None` for a non-rejected `tr_cd` or an unparseable
/// frame (which is recorded but otherwise ignored, as before).
fn rejection_reply(subscribe_text: &str, rejected: &[String]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(subscribe_text).ok()?;
    let tr_cd = v["body"]["tr_cd"].as_str()?;
    if !rejected.iter().any(|r| r == tr_cd) {
        return None;
    }
    let tr_key = v["body"]["tr_key"].as_str().unwrap_or("");
    let reply = serde_json::json!({
        "header": { "tr_cd": tr_cd, "tr_key": tr_key },
        "body": { "rsp_cd": MOCK_REJECTION_RSP_CD, "rsp_msg": "rejected" },
    });
    Some(reply.to_string())
}
