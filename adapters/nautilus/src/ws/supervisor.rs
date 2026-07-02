//! The WS reconnect supervisor (KTD8), generic over the delivery lane (KTD3, R5).
//!
//! The nautilus client traits' subscribe/unsubscribe methods are synchronous and
//! `?Send`; they enqueue commands over a channel to this supervisor task, which
//! owns the [`WsManager`] and the active-subscription set and performs the async
//! `subscribe_typed` calls. Only `Send` state (streams, handles) crosses into
//! spawned reader tasks. The supervisor catches the SDK's terminal reconnect-budget
//! error (`WebSocket("reconnect budget exhausted")`) and **rebuilds** `realtime()`,
//! resubscribing the active set with unbounded backoff; the SDK's in-budget
//! reconnects are invisible here (they deliver no missed frames). Registration-ACK
//! frames (all-default rows / null bodies) are filtered from emission but recorded
//! as delivery signals for the never-delivered diagnostic.
//!
//! The exact same machinery drives two lanes (KTD3): the market-data lane
//! ([`WsSupervisor::spawn`], emits [`DataEvent`]) and the order-event lane
//! ([`WsSupervisor::spawn_order_events`], emits [`OrderEventMsg`]). A [`LaneProfile`]
//! parameterizes the emitted event type, the [`WsLane`], and the row→event decode;
//! the reconnect/rebuild/terminal-coalescing/never-delivered lifecycle is shared, so
//! the SC lane inherits the v1 resilience rather than growing its own (R5).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use ls_core::LsError;
use ls_sdk::realtime::{SubscriptionHandle, WsLane, WsManager, WsStream};
use ls_sdk::LsSdk;
use nautilus_common::messages::DataEvent;
use nautilus_model::identifiers::InstrumentId;
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::now_nanos;
use super::rows::{BookRow, OrderEventMsg, Sc0Row, Sc1Row, ToEvent, TradeRow};

/// Which parser a subscription uses. The market-data lane uses [`Self::Trade`] /
/// [`Self::Quote`]; the order-event lane uses [`Self::OrderAccept`] /
/// [`Self::OrderFill`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// S3_/K3_ trade rows → `TradeTick`.
    Trade,
    /// H1_/HA_ book rows → top-of-book `QuoteTick`.
    Quote,
    /// SC0 order-accept rows → [`OrderEventMsg::Accept`].
    OrderAccept,
    /// SC1 order-fill rows → [`OrderEventMsg::Fill`].
    OrderFill,
}

/// A fully-resolved subscription request (the market-segment routing has already
/// chosen the `tr_cd`).
#[derive(Debug, Clone)]
pub struct SubSpec {
    /// Resolved realtime code (S3_/K3_/H1_/HA_ for market data; SC0/SC1 for orders).
    pub tr_cd: String,
    /// Subscription key (shcode for market data; `""` account-wide for orders).
    pub tr_key: String,
    /// The nautilus instrument the ticks belong to (a placeholder for the
    /// account-wide order-event lane, which keys fills by OrdNo, not instrument).
    pub instrument_id: InstrumentId,
    /// Which parser to apply.
    pub kind: RowKind,
}

impl SubSpec {
    fn key(&self) -> String {
        format!("{}:{}", self.tr_cd, self.tr_key)
    }
}

enum Command {
    Subscribe(SubSpec),
    Unsubscribe { key: String },
    Shutdown,
}

/// A delivery lane: the emitted event type, its [`WsLane`], and the row→event decode
/// (KTD3). The reconnect/rebuild lifecycle is lane-agnostic and shared.
#[async_trait]
pub trait LaneProfile: Send + Sync + 'static {
    /// The event this lane emits (`DataEvent` for market data, `OrderEventMsg` for
    /// the order lane).
    type Event: Send + 'static;

    /// The realtime lane these subscriptions register on.
    fn ws_lane() -> WsLane;

    /// Subscribe one spec on this lane and spawn its reader task.
    async fn subscribe(
        manager: &Arc<WsManager>,
        spec: &SubSpec,
        emit: mpsc::UnboundedSender<Self::Event>,
        first_frame: Arc<AtomicBool>,
        terminal_tx: mpsc::UnboundedSender<()>,
    ) -> Result<(SubscriptionHandle, tokio::task::JoinHandle<()>), LsError>;
}

/// The market-data lane (S3_/K3_ trades, H1_/HA_ books → [`DataEvent`]).
pub struct MarketDataProfile;

#[async_trait]
impl LaneProfile for MarketDataProfile {
    type Event = DataEvent;

    fn ws_lane() -> WsLane {
        WsLane::MarketData
    }

    async fn subscribe(
        manager: &Arc<WsManager>,
        spec: &SubSpec,
        emit: mpsc::UnboundedSender<DataEvent>,
        first_frame: Arc<AtomicBool>,
        terminal_tx: mpsc::UnboundedSender<()>,
    ) -> Result<(SubscriptionHandle, tokio::task::JoinHandle<()>), LsError> {
        match spec.kind {
            RowKind::Trade => {
                subscribe_and_spawn::<TradeRow, DataEvent>(
                    manager, spec, WsLane::MarketData, emit, first_frame, terminal_tx,
                )
                .await
            }
            RowKind::Quote => {
                subscribe_and_spawn::<BookRow, DataEvent>(
                    manager, spec, WsLane::MarketData, emit, first_frame, terminal_tx,
                )
                .await
            }
            other => Err(LsError::WebSocket(format!(
                "market-data lane cannot subscribe a {other:?} row kind"
            ))),
        }
    }
}

/// The order-event lane (SC0 accepts, SC1 fills → [`OrderEventMsg`]).
pub struct OrderEventProfile;

#[async_trait]
impl LaneProfile for OrderEventProfile {
    type Event = OrderEventMsg;

    fn ws_lane() -> WsLane {
        WsLane::OrderEvent
    }

    async fn subscribe(
        manager: &Arc<WsManager>,
        spec: &SubSpec,
        emit: mpsc::UnboundedSender<OrderEventMsg>,
        first_frame: Arc<AtomicBool>,
        terminal_tx: mpsc::UnboundedSender<()>,
    ) -> Result<(SubscriptionHandle, tokio::task::JoinHandle<()>), LsError> {
        match spec.kind {
            RowKind::OrderAccept => {
                subscribe_and_spawn::<Sc0Row, OrderEventMsg>(
                    manager, spec, WsLane::OrderEvent, emit, first_frame, terminal_tx,
                )
                .await
            }
            RowKind::OrderFill => {
                subscribe_and_spawn::<Sc1Row, OrderEventMsg>(
                    manager, spec, WsLane::OrderEvent, emit, first_frame, terminal_tx,
                )
                .await
            }
            other => Err(LsError::WebSocket(format!(
                "order-event lane cannot subscribe a {other:?} row kind"
            ))),
        }
    }
}

/// Per-subscription delivery diagnostic: when it was subscribed and whether any
/// frame has ever been delivered (KTD8's never-delivered signal).
struct SubDiag {
    subscribed_at: Instant,
    first_frame: Arc<AtomicBool>,
}

/// Handle to the running supervisor. Cloneable senders let the (sync) client trait
/// methods enqueue commands from any thread. The handle is lane-agnostic (the
/// emitted event type is fixed at spawn), so both lanes share one handle type.
pub struct WsSupervisor {
    cmd_tx: mpsc::UnboundedSender<Command>,
    connected: Arc<AtomicBool>,
    diagnostics: Arc<Mutex<HashMap<String, SubDiag>>>,
}

impl WsSupervisor {
    /// Spawn the market-data supervisor over an SDK handle, emitting decoded data
    /// events to `emit` (in a live node this is `get_data_event_sender()`; tests
    /// inject their own channel).
    pub fn spawn(sdk: LsSdk, emit: mpsc::UnboundedSender<DataEvent>) -> Self {
        Self::spawn_with::<MarketDataProfile>(sdk, emit)
    }

    /// Spawn the order-event supervisor over an SDK handle (KTD3 — the exec client
    /// spawns this over its **own** `WsManager`, isolating failure domains from the
    /// market-data lane). SC0/SC1 subscriptions register on [`WsLane::OrderEvent`];
    /// decoded fills/accepts flow to `emit` (an order-event sink on the exec side).
    pub fn spawn_order_events(sdk: LsSdk, emit: mpsc::UnboundedSender<OrderEventMsg>) -> Self {
        Self::spawn_with::<OrderEventProfile>(sdk, emit)
    }

    fn spawn_with<P: LaneProfile>(sdk: LsSdk, emit: mpsc::UnboundedSender<P::Event>) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let connected = Arc::new(AtomicBool::new(true));
        let diagnostics = Arc::new(Mutex::new(HashMap::new()));

        let task = SupervisorTask::<P> {
            sdk: sdk.clone(),
            manager: sdk.realtime(),
            emit,
            active: HashMap::new(),
            connected: Arc::clone(&connected),
            diagnostics: Arc::clone(&diagnostics),
        };
        tokio::spawn(task.run(cmd_rx));

        WsSupervisor {
            cmd_tx,
            connected,
            diagnostics,
        }
    }

    /// Enqueue a subscribe command (non-blocking).
    pub fn subscribe(&self, spec: SubSpec) {
        let _ = self.cmd_tx.send(Command::Subscribe(spec));
    }

    /// Enqueue an unsubscribe command for `tr_cd`/`tr_key` (non-blocking).
    pub fn unsubscribe(&self, tr_cd: &str, tr_key: &str) {
        let _ = self.cmd_tx.send(Command::Unsubscribe {
            key: format!("{tr_cd}:{tr_key}"),
        });
    }

    /// Whether the supervisor currently has a live session with all active
    /// subscriptions established. Goes `false` during a rebuild and `true` again
    /// once resubscription succeeds (AE4).
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Subscriptions that have received **no** frame after at least `min_age`
    /// (the never-delivered diagnostic — a dead subscription is otherwise
    /// indistinguishable from a quiet market).
    pub fn never_delivered(&self, min_age: Duration) -> Vec<String> {
        let now = Instant::now();
        self.diagnostics
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, d)| {
                !d.first_frame.load(Ordering::SeqCst)
                    && now.duration_since(d.subscribed_at) >= min_age
            })
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Signal the supervisor to stop.
    pub fn shutdown(&self) {
        let _ = self.cmd_tx.send(Command::Shutdown);
    }
}

struct ActiveSub {
    spec: SubSpec,
    #[allow(dead_code)] // held to keep the subscription alive (RAII); dropped to release
    handle: SubscriptionHandle,
    reader: tokio::task::JoinHandle<()>,
}

struct SupervisorTask<P: LaneProfile> {
    sdk: LsSdk,
    manager: Arc<WsManager>,
    emit: mpsc::UnboundedSender<P::Event>,
    active: HashMap<String, ActiveSub>,
    connected: Arc<AtomicBool>,
    diagnostics: Arc<Mutex<HashMap<String, SubDiag>>>,
}

/// Whether `rebuild` returned because it reconnected or because a Shutdown arrived
/// mid-rebuild.
#[derive(PartialEq, Eq)]
enum RebuildControl {
    Reconnected,
    Shutdown,
}

impl<P: LaneProfile> SupervisorTask<P> {
    async fn run(mut self, mut cmd_rx: mpsc::UnboundedReceiver<Command>) {
        // Reader tasks signal a terminal reconnect-budget error over this channel.
        let (terminal_tx, mut terminal_rx) = mpsc::unbounded_channel::<()>();

        loop {
            tokio::select! {
                cmd = cmd_rx.recv() => match cmd {
                    Some(Command::Subscribe(spec)) => {
                        if let Err(e) = self.subscribe_one(spec.clone(), &terminal_tx).await {
                            tracing::warn!(error = %e, key = %spec.key(), "subscribe failed");
                        } else {
                            self.connected.store(true, Ordering::SeqCst);
                        }
                    }
                    Some(Command::Unsubscribe { key }) => {
                        self.do_unsubscribe(&key);
                    }
                    Some(Command::Shutdown) | None => break,
                },
                _ = terminal_rx.recv() => {
                    // On reconnect-budget exhaustion EVERY reader errors, queuing N
                    // terminal signals; coalesce them so we rebuild once, not N times
                    // (each redundant rebuild would tear down the healthy session and
                    // re-hammer the gateway).
                    while terminal_rx.try_recv().is_ok() {}
                    if self.rebuild(&terminal_tx, &mut cmd_rx, &mut terminal_rx).await
                        == RebuildControl::Shutdown
                    {
                        break;
                    }
                }
            }
        }
        // Deterministic teardown: abort every reader rather than detaching them.
        for (_, sub) in self.active.drain() {
            sub.reader.abort();
        }
    }

    /// Establish one subscription (kind-dispatch + diagnostics + active-set insert),
    /// shared by the initial subscribe and the rebuild resubscribe. Does not touch
    /// `connected` — the caller owns that transition.
    async fn subscribe_one(
        &mut self,
        spec: SubSpec,
        terminal_tx: &mpsc::UnboundedSender<()>,
    ) -> Result<(), LsError> {
        let key = spec.key();
        let first_frame = Arc::new(AtomicBool::new(false));
        let (handle, reader) = P::subscribe(
            &self.manager,
            &spec,
            self.emit.clone(),
            Arc::clone(&first_frame),
            terminal_tx.clone(),
        )
        .await?;
        self.diagnostics.lock().unwrap().insert(
            key.clone(),
            SubDiag {
                subscribed_at: Instant::now(),
                first_frame,
            },
        );
        self.active.insert(key, ActiveSub { spec, handle, reader });
        Ok(())
    }

    fn do_unsubscribe(&mut self, key: &str) {
        if let Some(sub) = self.active.remove(key) {
            sub.reader.abort(); // stop reading
            drop(sub.handle); // RAII deregister frame
        }
        self.diagnostics.lock().unwrap().remove(key);
    }

    /// Rebuild the realtime session after a terminal error and resubscribe the
    /// active set with unbounded backoff (KTD8).
    async fn rebuild(
        &mut self,
        terminal_tx: &mpsc::UnboundedSender<()>,
        cmd_rx: &mut mpsc::UnboundedReceiver<Command>,
        terminal_rx: &mut mpsc::UnboundedReceiver<()>,
    ) -> RebuildControl {
        self.connected.store(false, Ordering::SeqCst);
        // The desired subscription set — mutated if commands arrive mid-rebuild.
        let mut desired: Vec<SubSpec> = self.active.values().map(|a| a.spec.clone()).collect();
        for (_, sub) in self.active.drain() {
            sub.reader.abort();
        }

        let mut backoff = Duration::from_millis(200);
        loop {
            // Fresh manager (new connection lifecycle).
            self.manager = self.sdk.realtime();
            let mut all_ok = true;
            for spec in &desired {
                if let Err(e) = self.subscribe_one(spec.clone(), terminal_tx).await {
                    tracing::warn!(error = %e, "resubscribe failed; backing off");
                    all_ok = false;
                    break;
                }
            }
            if all_ok {
                self.connected.store(true, Ordering::SeqCst);
                // A terminal signal may have arrived during the resubscribe; coalesce
                // it so we don't immediately tear the fresh session back down.
                while terminal_rx.try_recv().is_ok() {}
                return RebuildControl::Reconnected;
            }
            // Drop any partial subscriptions before retrying.
            for (_, sub) in self.active.drain() {
                sub.reader.abort();
            }
            // Stay responsive during the backoff: honour Shutdown immediately, and
            // fold Subscribe/Unsubscribe into the desired set so the outage cannot
            // starve the command channel (the deadlock the reviewers caught).
            tokio::select! {
                _ = tokio::time::sleep(backoff) => {}
                cmd = cmd_rx.recv() => match cmd {
                    Some(Command::Shutdown) | None => return RebuildControl::Shutdown,
                    Some(Command::Unsubscribe { key }) => {
                        desired.retain(|s| s.key() != key);
                    }
                    Some(Command::Subscribe(spec)) => {
                        if !desired.iter().any(|s| s.key() == spec.key()) {
                            desired.push(spec);
                        }
                    }
                },
            }
            backoff = (backoff * 2).min(Duration::from_secs(5));
        }
    }
}

async fn subscribe_and_spawn<Row, E>(
    manager: &Arc<WsManager>,
    spec: &SubSpec,
    lane: WsLane,
    emit: mpsc::UnboundedSender<E>,
    first_frame: Arc<AtomicBool>,
    terminal_tx: mpsc::UnboundedSender<()>,
) -> Result<(SubscriptionHandle, tokio::task::JoinHandle<()>), LsError>
where
    Row: ToEvent<E>,
    E: Send + 'static,
{
    let (handle, stream): (SubscriptionHandle, WsStream<Row>) = manager
        .subscribe_typed::<Row>(&spec.tr_cd, &spec.tr_key, lane)
        .await?;
    let instrument_id = spec.instrument_id;
    let reader = tokio::spawn(reader_loop::<Row, E>(
        stream,
        instrument_id,
        emit,
        first_frame,
        terminal_tx,
    ));
    Ok((handle, reader))
}

async fn reader_loop<Row, E>(
    mut stream: WsStream<Row>,
    instrument_id: InstrumentId,
    emit: mpsc::UnboundedSender<E>,
    first_frame: Arc<AtomicBool>,
    terminal_tx: mpsc::UnboundedSender<()>,
) where
    Row: ToEvent<E>,
    E: Send + 'static,
{
    while let Some(item) = stream.next().await {
        match item {
            Ok(row) => {
                // A registration-ACK / all-default row is a delivery signal, not a
                // tick: record it and skip emission.
                first_frame.store(true, Ordering::SeqCst);
                if row.is_ack() {
                    continue;
                }
                if let Some(event) = row.to_event(instrument_id, now_nanos()) {
                    let _ = emit.send(event);
                }
            }
            Err(LsError::WebSocket(msg)) if msg.contains("reconnect budget exhausted") => {
                // Terminal: the SDK exhausted its reconnect budget and purged this
                // subscription. Signal the supervisor to rebuild.
                let _ = terminal_tx.send(());
                return;
            }
            // Decode errors (null-body ACK) and other transients are non-terminal.
            Err(_) => continue,
        }
    }
}
