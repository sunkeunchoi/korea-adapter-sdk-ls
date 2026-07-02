//! The WS reconnect supervisor (KTD8).
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

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::StreamExt;
use ls_core::LsError;
use ls_sdk::realtime::{SubscriptionHandle, WsLane, WsManager, WsStream};
use ls_sdk::LsSdk;
use nautilus_common::messages::DataEvent;
use nautilus_model::identifiers::InstrumentId;
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::now_nanos;
use super::rows::{BookRow, ToData, TradeRow};

/// Which parser a subscription uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// S3_/K3_ trade rows → `TradeTick`.
    Trade,
    /// H1_/HA_ book rows → top-of-book `QuoteTick`.
    Quote,
}

/// A fully-resolved subscription request (the market-segment routing has already
/// chosen the `tr_cd`).
#[derive(Debug, Clone)]
pub struct SubSpec {
    /// Resolved realtime code (S3_/K3_/H1_/HA_).
    pub tr_cd: String,
    /// Subscription key (shcode).
    pub tr_key: String,
    /// The nautilus instrument the ticks belong to.
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

/// Per-subscription delivery diagnostic: when it was subscribed and whether any
/// frame has ever been delivered (KTD8's never-delivered signal).
struct SubDiag {
    subscribed_at: Instant,
    first_frame: Arc<AtomicBool>,
}

/// Handle to the running supervisor. Cloneable senders let the (sync) data-client
/// trait methods enqueue commands from any thread.
pub struct WsSupervisor {
    cmd_tx: mpsc::UnboundedSender<Command>,
    connected: Arc<AtomicBool>,
    diagnostics: Arc<Mutex<HashMap<String, SubDiag>>>,
}

impl WsSupervisor {
    /// Spawn the supervisor over an SDK handle, emitting decoded data events to
    /// `emit` (in a live node this is `get_data_event_sender()`; tests inject their
    /// own channel).
    pub fn spawn(sdk: LsSdk, emit: mpsc::UnboundedSender<DataEvent>) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let connected = Arc::new(AtomicBool::new(true));
        let diagnostics = Arc::new(Mutex::new(HashMap::new()));

        let task = SupervisorTask {
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

struct SupervisorTask {
    sdk: LsSdk,
    manager: Arc<WsManager>,
    emit: mpsc::UnboundedSender<DataEvent>,
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

impl SupervisorTask {
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
        let (handle, reader) = match spec.kind {
            RowKind::Trade => {
                subscribe_and_spawn::<TradeRow>(
                    &self.manager,
                    &spec,
                    self.emit.clone(),
                    Arc::clone(&first_frame),
                    terminal_tx.clone(),
                )
                .await?
            }
            RowKind::Quote => {
                subscribe_and_spawn::<BookRow>(
                    &self.manager,
                    &spec,
                    self.emit.clone(),
                    Arc::clone(&first_frame),
                    terminal_tx.clone(),
                )
                .await?
            }
        };
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

async fn subscribe_and_spawn<Row: ToData>(
    manager: &Arc<WsManager>,
    spec: &SubSpec,
    emit: mpsc::UnboundedSender<DataEvent>,
    first_frame: Arc<AtomicBool>,
    terminal_tx: mpsc::UnboundedSender<()>,
) -> Result<(SubscriptionHandle, tokio::task::JoinHandle<()>), LsError> {
    let (handle, stream): (SubscriptionHandle, WsStream<Row>) = manager
        .subscribe_typed::<Row>(&spec.tr_cd, &spec.tr_key, WsLane::MarketData)
        .await?;
    let instrument_id = spec.instrument_id;
    let reader = tokio::spawn(reader_loop::<Row>(
        stream,
        instrument_id,
        emit,
        first_frame,
        terminal_tx,
    ));
    Ok((handle, reader))
}

async fn reader_loop<Row: ToData>(
    mut stream: WsStream<Row>,
    instrument_id: InstrumentId,
    emit: mpsc::UnboundedSender<DataEvent>,
    first_frame: Arc<AtomicBool>,
    terminal_tx: mpsc::UnboundedSender<()>,
) {
    while let Some(item) = stream.next().await {
        match item {
            Ok(row) => {
                // A registration-ACK / all-default row is a delivery signal, not a
                // tick: record it and skip emission.
                first_frame.store(true, Ordering::SeqCst);
                if row.is_ack() {
                    continue;
                }
                if let Some(data) = row.to_data(instrument_id, now_nanos()) {
                    let _ = emit.send(DataEvent::Data(data));
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
