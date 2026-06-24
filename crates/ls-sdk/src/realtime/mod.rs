//! Realtime dependency class — the S3_ (KOSPI 체결 / KOSPI-trade) WebSocket
//! subscription, ported with the Migration Source's FIXED concurrency patterns.
//!
//! ## Why a separate `TokenManager`, not `Arc<Inner>`
//!
//! [`WsManager`] holds `Arc<TokenManager>` (plus the HTTP client, rate limiter,
//! and resolved config) **directly** — never `Arc<Inner>`. The manager is built
//! with [`Arc::new_cyclic`] so its spawned tasks hold a `Weak<WsManager>`
//! back-reference for auto-reconnect without forming a cycle. Holding `Arc<Inner>`
//! here (when `Inner` were ever to hold the `WsManager`) would be the circular
//! `Arc` this layout exists to avoid; constructing from `Inner`'s components
//! (`inner.config`, `inner.token_manager`, `inner.rate_limiter`, `inner.client`)
//! keeps the dependency one-directional.
//!
//! ## The fixed patterns preserved here (verbatim from the old `ws_manager/*`)
//!
//! 1. `Arc::new_cyclic` + `Weak` self-reference for cycle-free reconnect.
//! 2. `ensure_connected` force-clears then refreshes the token on EACH
//!    (re)connect (`token_manager.clear()` then `get_or_refresh`), times out
//!    `connect_async`, splits, REPLAYS existing subscriptions, installs `tx`,
//!    spawns the forwarder + inbound dispatch tasks.
//! 3. SECURITY: the inbound path never logs raw frame text (ACK frames echo the
//!    bearer token) — see [`connection`] and [`dispatch`].
//! 4. Subscribe-frame `{header:{token,tr_type},body:{tr_cd,tr_key}}`; the lane's
//!    register `tr_type` is threaded per-subscription (market-data `"3"/"4"`,
//!    order-event `"1"/"2"`) and stored so replay rebuilds the right frame;
//!    composite key `"<tr_cd>:<tr_key>"`.
//! 5. Reconnect is bounded to 4 attempts, then delivers a terminal
//!    `LsError::WebSocket("reconnect budget exhausted")` and cleans up.
//! 6. Overflow `DropNewest` (default) / `LatestOnly`; the `LatestOnly` stream
//!    pins ONE `Notified` across polls (the lost-wakeup fix) and exposes an
//!    explicit terminal `None`.
//! 7. `subscribe_typed` returns `(SubscriptionHandle, WsStream<Res>)` with RAII
//!    unsubscribe; a subscription is RECORDED before the outbound send.

mod connection;
mod dispatch;
mod frame;
mod overflow;
mod stream;

pub use frame::{
    composite_key, FC9Trade, FH9Trade, GSCTrade, GSHTrade, H1Trade, HATrade, K3Trade, OC0Trade,
    OH0Trade, OVCTrade, OVHTrade, S2Trade, S3Trade, UH1Trade, US2Trade, US3Trade, WsLane,
};
pub use stream::WsStream;

pub(crate) use dispatch::{DispatchEntry, WS_OUTBOUND_CHANNEL_CAPACITY};
pub(crate) use frame::{build_subscribe_msg, build_unsubscribe_msg, split_composite_key};
pub(crate) use overflow::LatestOnlySlot;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use ls_core::auth::TokenManager;
use ls_core::config::WsOverflowPolicy;
use ls_core::config_resolve::ResolvedConfig;
use ls_core::rate_limiter::RateLimiterManager;
use ls_core::{Inner, LsError, LsResult};

/// Active subscription store + WebSocket connection lifecycle.
pub struct WsManager {
    /// `composite_key(tr_cd, tr_key)` → `(tr_cd, lane)`. The bearer token is NEVER
    /// stored here — only routing identifiers and the [`WsLane`], so the replay set
    /// carries no secret and rebuilds the correct register frame per lane.
    pub(crate) subscriptions: DashMap<String, (String, WsLane)>,

    /// Outbound frame sender — re-established atomically on every reconnect.
    /// `None` when no active connection.
    tx: Mutex<Option<tokio::sync::mpsc::Sender<Message>>>,

    /// Serializes slow connection establishment so concurrent first subscribers
    /// cannot both connect and replay a stale snapshot.
    connect_lock: Mutex<()>,

    /// Owned token manager (shared `Arc` with `Inner`, NOT `Arc<Inner>`). Used to
    /// force a fresh token on every (re)connect.
    token_manager: Arc<TokenManager>,

    /// HTTP client for the `/oauth2/token` refresh on reconnect.
    http_client: reqwest::Client,

    /// Shared rate limiter — reconnect token refreshes charge the same Auth
    /// bucket as REST token fetches.
    rate_limiter: Arc<RateLimiterManager>,

    /// Resolved runtime config — WS URL, connect timeout, channel capacity,
    /// overflow policy, and token-refresh inputs.
    config: ResolvedConfig,

    /// Per-key dispatch channels for typed subscribers, keyed by composite key.
    /// `Arc` so the reconnect inbound task can share it by clone.
    pub(crate) dispatch: Arc<DashMap<String, DispatchEntry>>,

    /// Weak self-reference so spawned tasks can trigger auto-reconnect without a
    /// circular `Arc` — paired with [`Arc::new_cyclic`].
    weak_self: std::sync::Weak<WsManager>,
}

impl WsManager {
    /// Construct (without connecting) from a shared [`Inner`]'s components.
    ///
    /// Pulls `token_manager`, `client`, `rate_limiter`, and `config` out of
    /// `Inner` — NOT `Arc<Inner>` — so there is no back-edge from `WsManager` to
    /// `Inner`. Built via [`Arc::new_cyclic`] so spawned tasks hold a `Weak`.
    pub fn from_inner(inner: &Arc<Inner>) -> Arc<Self> {
        Self::new(
            Arc::clone(&inner.token_manager),
            inner.client.clone(),
            Arc::clone(&inner.rate_limiter),
            inner.config.clone(),
        )
    }

    /// Construct from explicit components. `token_manager`/`http_client`/
    /// `rate_limiter`/`config` are cloned from `Inner`'s scope — no `Arc<Inner>`.
    pub fn new(
        token_manager: Arc<TokenManager>,
        http_client: reqwest::Client,
        rate_limiter: Arc<RateLimiterManager>,
        config: ResolvedConfig,
    ) -> Arc<Self> {
        Arc::new_cyclic(|weak| Self {
            subscriptions: DashMap::new(),
            tx: Mutex::new(None),
            connect_lock: Mutex::new(()),
            token_manager,
            http_client,
            rate_limiter,
            config,
            dispatch: Arc::new(DashMap::new()),
            weak_self: weak.clone(),
        })
    }

    /// Record and send a subscription — stores composite key → tr_cd ONLY.
    ///
    /// Record-before-send: the insert into `subscriptions` happens BEFORE the
    /// connection check and the outbound send. If the connection was live at
    /// check time but dropped before the send, the subscription is already in the
    /// map and will be replayed on auto-reconnect (no lost subscription). When no
    /// live connection exists, `ensure_connected`'s replay loop includes the
    /// just-inserted entry, so we return early to avoid a double-send.
    #[tracing::instrument(skip_all, fields(tr_cd, tr_key))]
    pub async fn subscribe(&self, tr_cd: &str, tr_key: &str, lane: WsLane) -> LsResult<()> {
        // Step 1: RECORD first (the ordering point). A send that fails after this
        // must not take the subscription with it. Store the `lane` so reconnect
        // replay rebuilds the correct register frame.
        self.subscriptions
            .insert(composite_key(tr_cd, tr_key), (tr_cd.to_string(), lane));

        // Step 2: check connection state under a single lock acquisition.
        let needs_connect = self.tx.lock().await.is_none();

        if needs_connect {
            // Not connected — establish. The replay loop iterates
            // `self.subscriptions` (now including this entry) and sends the
            // subscribe frame as part of replay; return early to avoid a
            // double-send.
            if let Some(arc_self) = self.weak_self.upgrade() {
                arc_self.ensure_connected().await?;
            }
            return Ok(());
        }

        // Step 3: already connected — send the subscribe frame explicitly. If the
        // connection dropped while we were in step 2, the silent fall-through is
        // safe: the subscription is recorded and will be replayed on reconnect.
        if let Some(tx) = self.tx.lock().await.as_ref() {
            let token = self
                .token_manager
                .get_or_refresh(&self.http_client, &self.config, &self.rate_limiter)
                .await?;
            let msg = build_subscribe_msg(tr_cd, tr_key, &token, lane);
            tx.send(msg)
                .await
                .map_err(|e| LsError::WebSocket(e.to_string()))?;
        }
        Ok(())
    }

    /// Idempotent connect / reconnect. Returns immediately if already connected.
    ///
    /// On a fresh connection: force-clears + refreshes the token, times out
    /// `connect_async`, splits the stream, REPLAYS every stored subscription with
    /// the fresh token, installs the outbound `tx`, and spawns the forwarder +
    /// inbound-dispatch tasks (which auto-reconnect on EOF).
    ///
    /// `impl Future + Send` (not `async fn`): the inbound task recursively calls
    /// `ensure_connected`; the explicit `Send` bound is the assertion that every
    /// type held across await points is `Send`-safe.
    #[allow(clippy::manual_async_fn)]
    #[tracing::instrument(skip_all, fields(reconnect, subscription_count))]
    pub fn ensure_connected(&self) -> impl std::future::Future<Output = LsResult<()>> + Send + '_ {
        async move {
            let span = tracing::Span::current();
            // Fast path: already connected.
            {
                let tx_guard = self.tx.lock().await;
                if tx_guard.is_some() {
                    return Ok(());
                }
            }

            let _connect_guard = self.connect_lock.lock().await;
            {
                let tx_guard = self.tx.lock().await;
                if tx_guard.is_some() {
                    return Ok(());
                }
            }

            let is_reconnect = !self.subscriptions.is_empty();
            span.record("reconnect", is_reconnect);
            span.record("subscription_count", self.subscriptions.len() as i64);

            // Step 1: force a FRESH token on every (re)connect (clear, then fetch).
            // The cleared-then-refreshed token is what makes reconnect-after-EOF
            // present a valid bearer even if the old one was revoked server-side.
            self.token_manager.clear().await;
            let token = self
                .token_manager
                .get_or_refresh(&self.http_client, &self.config, &self.rate_limiter)
                .await?;

            // Step 2: new WS connection, with connect_async wrapped in a timeout.
            // Double-Result: outer .map_err handles Elapsed; inner handles the
            // tungstenite error.
            let ws_url = &self.config.ws_url;
            let ws_timeout = self.config.ws_connect_timeout;
            let (ws_stream, _resp) = tokio::time::timeout(ws_timeout, connect_async(ws_url))
                .await
                .map_err(|_| {
                    LsError::WebSocket(format!(
                        "WS connect timed out after {}s",
                        ws_timeout.as_secs()
                    ))
                })?
                .map_err(|e| LsError::WebSocket(e.to_string()))?;
            let (mut sink, stream) = ws_stream.split();

            // Step 3: replay every stored subscription with the fresh token.
            self.replay_subscriptions(&mut sink, &token).await?;
            tracing::info!("ws connected");

            // Step 4: install the sender channel; spawn the read + forward tasks.
            let (tx, rx) = tokio::sync::mpsc::channel::<Message>(WS_OUTBOUND_CHANNEL_CAPACITY);
            {
                let mut tx_guard = self.tx.lock().await;
                if tx_guard.is_some() {
                    return Ok(());
                }
                *tx_guard = Some(tx);
            }

            connection::spawn_outbound_forwarder(rx, sink);
            connection::spawn_inbound_task(
                Arc::clone(&self.dispatch),
                std::sync::Weak::clone(&self.weak_self),
                stream,
            );

            Ok(())
        }
    }

    /// Subscribe to typed S3_ (or any market-data) frames for `(tr_cd, tr_key)`.
    ///
    /// Returns `(SubscriptionHandle, WsStream<Res>)`. The handle drives RAII
    /// cleanup — dropping it unsubscribes; `.unsubscribe().await` is the eager,
    /// awaitable form. The overflow policy and channel capacity come from
    /// `self.config` at subscribe time.
    pub async fn subscribe_typed<Res>(
        &self,
        tr_cd: &str,
        tr_key: &str,
        lane: WsLane,
    ) -> LsResult<(SubscriptionHandle, WsStream<Res>)>
    where
        Res: serde::de::DeserializeOwned + Send + 'static,
    {
        let capacity = self.config.ws_channel_capacity;
        let key = composite_key(tr_cd, tr_key);

        let (entry, consumer_stream) = match &self.config.ws_overflow_policy {
            WsOverflowPolicy::DropNewest => {
                let (tx, rx) = tokio::sync::mpsc::channel::<LsResult<serde_json::Value>>(capacity);
                (
                    DispatchEntry::DropNewest(tx, Arc::new(AtomicU64::new(0)), capacity),
                    WsStream::drop_newest(rx),
                )
            }
            WsOverflowPolicy::LatestOnly => {
                let slot_arc = Arc::new(LatestOnlySlot::new());
                (
                    DispatchEntry::LatestOnly(
                        Arc::downgrade(&slot_arc),
                        Arc::new(AtomicU64::new(0)),
                    ),
                    WsStream::latest_only(slot_arc),
                )
            }
        };

        // Single insert, moving `key`. A displaced LatestOnly entry is closed so
        // its orphaned stream ends rather than pending forever.
        close_latest_only_entry(self.dispatch.insert(key, entry));

        if let Err(e) = self.subscribe(tr_cd, tr_key, lane).await {
            // Roll back both maps on subscribe failure so no ghost subscription
            // is replayed to the server with no live consumer.
            let key = composite_key(tr_cd, tr_key);
            close_latest_only_entry(self.dispatch.remove(&key).map(|(_, entry)| entry));
            self.subscriptions.remove(&key);
            return Err(e);
        }

        // Always inside an async fn → a runtime is active; `current()` is safe.
        let runtime_handle = tokio::runtime::Handle::current();

        let ws_manager = self.weak_self.upgrade().ok_or_else(|| {
            LsError::WebSocket("WsManager Arc dropped during subscribe_typed".into())
        })?;

        let handle = SubscriptionHandle {
            ws_manager,
            tr_cd: tr_cd.to_string(),
            tr_key: tr_key.to_string(),
            lane,
            runtime_handle,
            consumed: AtomicBool::new(false),
        };

        Ok((handle, consumer_stream))
    }

    /// Unsubscribe from typed frames for `(tr_cd, tr_key)`.
    ///
    /// Sends the deregister frame for `lane` (`"4"` market-data, `"2"` order-event),
    /// then removes the entry from both the dispatch map
    /// (ending the subscriber stream under both policies) and the subscriptions
    /// map (preventing reconnect replay). Network/token errors are logged and do
    /// NOT abort local cleanup — local state is always removed.
    pub async fn unsubscribe_typed(&self, tr_cd: &str, tr_key: &str, lane: WsLane) -> LsResult<()> {
        let key = composite_key(tr_cd, tr_key);

        if self.tx.lock().await.is_some() {
            match self
                .token_manager
                .get_or_refresh(&self.http_client, &self.config, &self.rate_limiter)
                .await
            {
                Ok(token) => {
                    let msg = build_unsubscribe_msg(tr_cd, tr_key, &token, lane);
                    if let Some(tx) = self.tx.lock().await.as_ref() {
                        if let Err(e) = tx.send(msg).await {
                            tracing::warn!(
                                error = %e,
                                "unsubscribe_typed: send failed (connection may be gone)"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "unsubscribe_typed: token refresh failed, skipping unsubscribe frame"
                    );
                }
            }
        }

        // Always remove local state — DropNewest ends when its sender drops;
        // LatestOnly is closed so a parked consumer wakes, drains, and ends.
        close_latest_only_entry(self.dispatch.remove(&key).map(|(_, entry)| entry));
        self.subscriptions.remove(&key);
        Ok(())
    }

    /// Number of active dispatch entries (one per composite key).
    pub fn dispatch_len(&self) -> usize {
        self.dispatch.len()
    }

    /// Cumulative dropped-frame count for `(tr_cd, tr_key)`, or `0` if no entry.
    pub fn dropped_count(&self, tr_cd: &str, tr_key: &str) -> u64 {
        let key = composite_key(tr_cd, tr_key);
        match self.dispatch.get(&key).as_deref() {
            Some(DispatchEntry::DropNewest(_, counter, _)) => counter.load(Ordering::Relaxed),
            Some(DispatchEntry::LatestOnly(_, counter)) => counter.load(Ordering::Relaxed),
            None => 0,
        }
    }

    /// Number of active subscriptions in the replay store.
    pub fn subscription_count(&self) -> usize {
        self.subscriptions.len()
    }

    /// `true` if the replay store contains `(tr_cd, tr_key)`.
    pub fn has_subscription(&self, tr_cd: &str, tr_key: &str) -> bool {
        self.subscriptions
            .contains_key(&composite_key(tr_cd, tr_key))
    }

    /// Replay every stored subscription to the server using the fresh token.
    ///
    /// Builds every replay frame while iterating (await-free — no DashMap shard
    /// guard held across a suspension point), then sends after iteration.
    #[tracing::instrument(skip_all, fields(subscription_count))]
    async fn replay_subscriptions(
        &self,
        sink: &mut futures::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            Message,
        >,
        token: &str,
    ) -> LsResult<()> {
        let msgs: Vec<Message> = self
            .subscriptions
            .iter()
            .map(|e| {
                let (_, tr_key) = split_composite_key(e.key());
                let (tr_cd, lane) = e.value();
                build_subscribe_msg(tr_cd, tr_key, token, *lane)
            })
            .collect();
        let count = msgs.len();
        for msg in msgs {
            sink.send(msg)
                .await
                .map_err(|e| LsError::WebSocket(e.to_string()))?;
        }
        if count > 0 {
            tracing::info!(
                subscription_count = count,
                "ws subscriptions replayed after reconnect"
            );
        }
        Ok(())
    }
}

impl std::fmt::Debug for WsManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WsManager")
            .field("subscriptions_count", &self.subscriptions.len())
            .finish_non_exhaustive()
    }
}

/// Close the slot of a `LatestOnly` dispatch entry leaving the map (unsubscribe
/// or same-key displacement) so its stream wakes, drains, and ends instead of
/// pending forever. A `DropNewest` entry, a missing entry, or an already-dropped
/// consumer needs nothing.
fn close_latest_only_entry(entry: Option<DispatchEntry>) {
    if let Some(DispatchEntry::LatestOnly(weak_slot, _)) = entry {
        if let Some(slot_arc) = weak_slot.upgrade() {
            slot_arc.close();
        }
    }
}

/// RAII handle returned by [`WsManager::subscribe_typed`].
///
/// Dropping the handle triggers an async, fire-and-forget unsubscribe via the
/// captured runtime handle. `.unsubscribe().await` is the eager, awaitable form;
/// both converge on [`WsManager::unsubscribe_typed`].
pub struct SubscriptionHandle {
    ws_manager: Arc<WsManager>,
    tr_cd: String,
    tr_key: String,
    /// The subscription's [`WsLane`], carried so unsubscribe rebuilds the correct
    /// deregister frame. `Copy`, so Drop reuses it without a clone.
    lane: WsLane,
    /// Captured at construction (always inside an async fn) — more reliable than
    /// `Handle::current()` from `Drop`, which panics if the runtime is gone.
    runtime_handle: tokio::runtime::Handle,
    /// Set when `unsubscribe(self)` succeeds; Drop checks it to avoid a
    /// double-unsubscribe (Rust fires Drop on the moved-in `self` at fn-scope
    /// end).
    consumed: AtomicBool,
}

impl SubscriptionHandle {
    /// Eagerly unsubscribe and await server acknowledgement. `consumed` is set
    /// only on success, so Drop still cleans up on failure.
    pub async fn unsubscribe(self) -> LsResult<()> {
        let result = self
            .ws_manager
            .unsubscribe_typed(&self.tr_cd, &self.tr_key, self.lane)
            .await;
        if result.is_ok() {
            self.consumed.store(true, Ordering::SeqCst);
        }
        result
    }
}

impl Drop for SubscriptionHandle {
    fn drop(&mut self) {
        if self.consumed.load(Ordering::SeqCst) {
            return; // already cleaned up by explicit unsubscribe()
        }
        let arc = Arc::clone(&self.ws_manager);
        let tr_cd = self.tr_cd.clone();
        let tr_key = self.tr_key.clone();
        let lane = self.lane;
        // Fire-and-forget cleanup on the captured runtime. Drop has no fallible
        // return path; errors are logged, not surfaced.
        self.runtime_handle.spawn(async move {
            if let Err(e) = arc.unsubscribe_typed(&tr_cd, &tr_key, lane).await {
                tracing::warn!(
                    tr_cd = %tr_cd,
                    tr_key = %tr_key,
                    error = %e,
                    "SubscriptionHandle::drop: async unsubscribe failed"
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ls_core::auth::TokenData;
    use ls_core::config::{LsConfig, RateLimitConfig};
    use ls_core::Environment;

    fn test_resolved() -> ResolvedConfig {
        let cfg = LsConfig {
            appkey: "test-appkey".into(),
            appsecretkey: "test-appsecretkey".into(),
            account_no: "00000000-01".into(),
            environment: Environment::Paper,
            rate_limits: Some(RateLimitConfig {
                market_data_per_sec: Some(1000),
                orders_per_sec: Some(1000),
                account_per_sec: Some(1000),
                auth_per_sec: Some(1000),
            }),
            base_url: None,
            ws_base_url: None,
            max_pages: None,
            connect_timeout_secs: None,
            request_timeout_secs: None,
            ws_connect_timeout_secs: None,
            allow_insecure_localhost: true,
            ws_channel_capacity: None,
            ws_overflow_policy: None,
        };
        ResolvedConfig::from_raw(&cfg).expect("test config must resolve")
    }

    /// THE LOAD-BEARING ORDERING PROOF (record-before-send).
    ///
    /// `subscribe` must record the subscription in `self.subscriptions` BEFORE
    /// attempting the outbound send, so a send that fails synchronously still
    /// leaves the subscription eligible for reconnect replay. The integration
    /// suite cannot observe this through the public API — the outbound mpsc
    /// buffers sends on a dead socket — so this deterministic unit test OWNS the
    /// invariant.
    ///
    /// Choreography (no buffer in the observation path): seed a far-future token
    /// (cache fast path — no HTTP, no rate-limiter wait), install a `tx` whose
    /// receiver has been dropped (the connection LOOKS live so `subscribe` skips
    /// `ensure_connected`, but `tx.send()` fails SYNCHRONOUSLY with a closed
    /// channel), call `subscribe`, and assert the send error did NOT take the
    /// subscription with it.
    ///
    /// RED against the inverted (insert-only-after-send-Ok) order: under that
    /// order the synchronous send error returns BEFORE the insert runs, so
    /// `has_subscription` is false and this test fails on its discriminating
    /// assertion. Proven red against that inversion (see U14 summary).
    #[tokio::test]
    async fn subscribe_records_subscription_before_outbound_send() {
        let resolved = test_resolved();

        let token_manager = Arc::new(TokenManager::new());
        token_manager
            .seed_token(TokenData {
                access_token: "tok_seeded".into(),
                expires_at: chrono::Utc::now() + chrono::Duration::hours(24),
            })
            .await;

        let rate_limiter = Arc::new(
            RateLimiterManager::new(&resolved.rate_limits).expect("default rate limits resolve"),
        );
        let wm = WsManager::new(
            token_manager,
            reqwest::Client::new(),
            rate_limiter,
            resolved,
        );

        // Install a live-looking outbound sender whose receiver is dropped:
        // tx.is_some() passes (skips ensure_connected); tx.send() fails
        // synchronously with a closed-channel error.
        let (dead_tx, dead_rx) = tokio::sync::mpsc::channel::<Message>(1);
        drop(dead_rx);
        *wm.tx.lock().await = Some(dead_tx);

        let result = wm.subscribe("S3_", "005930", WsLane::MarketData).await;

        assert!(
            matches!(result, Err(LsError::WebSocket(_))),
            "send on a closed outbound channel must surface Err(WebSocket); got {result:?}"
        );
        assert!(
            wm.has_subscription("S3_", "005930"),
            "subscription must be recorded BEFORE the outbound send: a failed \
             send must not lose the entry, or reconnect replay would skip it"
        );
    }
}
