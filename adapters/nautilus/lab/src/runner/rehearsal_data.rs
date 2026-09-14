//! The rehearsal's data seam (U9, KTD15) — how a synthetic daily bar reaches `on_bar`.
//!
//! `DailyStrategy::on_start` subscribes a 1-DAY [`BarType`]. Handing the strategy a bar
//! directly would bypass the `DataEngine`, and with it the subscription routing, the cache
//! write and the actor dispatch the backtest path goes through — so the live callback would
//! be reached by a different mechanism than the one the frozen head was judged under. The
//! bar therefore has to enter through the node's **data event sender**, which is what the
//! `DataEngine` drains.
//!
//! That sender is a thread-local the live runner installs, readable only from the node's own
//! runtime thread. A client's `start()` runs there, so this client captures it into a shared
//! slot the runner can read from wherever it drives the day (KTD15). `daily.rs` is untouched
//! on this axis, which is the point: the strategy keeps its backtest-identical subscription.
//!
//! **Not an [`LsDataClient`](nautilus_ls::data::LsDataClient) wrapper**, though the plan
//! sketches it as one. That client's `start()` unconditionally spawns the `WsSupervisor` and
//! opens the realtime trade/quote lanes — lanes a rehearsal never subscribes to, because its
//! decision input is the 15:20 t8407 read and its only data event is the synthetic bar this
//! module delivers. Wrapping it would open a live gateway connection to pass nothing through,
//! and would make the offline whole-day test need a WS stub for a lane under no test.
//!
//! It DOES publish the session's instruments on connect, which is not incidental: an
//! UNCACHED instrument makes nautilus skip reconciliation **silently**
//! (`tests/execution_claims.rs`), so the inherited book would never be attributed to the
//! strategy and the exit orders would have nothing to close.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use nautilus_common::cache::CacheView;
use nautilus_common::clients::DataClient;
use nautilus_common::clock::Clock;
use nautilus_common::factories::{ClientConfig, DataClientFactory};
use nautilus_common::messages::DataEvent;
use nautilus_model::data::{Bar, Data};
use nautilus_model::identifiers::{ClientId, Venue};
use nautilus_model::instruments::InstrumentAny;

/// The node's data event sender, shared out of the client that captured it.
///
/// `Arc<Mutex<..>>` rather than a channel because there is exactly one writer (the client's
/// `start`) and one reader (the runner's day loop), and the reader must be able to ask
/// "has it been captured yet?" without consuming anything — a bar sent before the node
/// installed its sender has nowhere to go and must be refused, not dropped.
#[derive(Clone, Default)]
pub struct DataEventSink {
    inner: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<DataEvent>>>>,
}

impl std::fmt::Debug for DataEventSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataEventSink").field("captured", &self.is_captured()).finish()
    }
}

impl DataEventSink {
    /// An empty sink — nothing captured yet.
    #[must_use]
    pub fn new() -> Self {
        DataEventSink::default()
    }

    /// Whether the node's sender has been captured.
    #[must_use]
    pub fn is_captured(&self) -> bool {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    /// Capture the sender (idempotent — whichever lifecycle hook runs first wins).
    pub fn capture(&self, sender: tokio::sync::mpsc::UnboundedSender<DataEvent>) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            *guard = Some(sender);
        }
    }

    /// Deliver one synthetic bar to the `DataEngine` (KTD15).
    ///
    /// # Errors
    ///
    /// If the sender was never captured (no node started), or the receiving end is gone
    /// (the node shut down). Both are refusals rather than silent drops: a bar that did not
    /// reach `on_bar` means the session took no decision for that symbol, and the run has to
    /// say so.
    pub fn send_bar(&self, bar: Bar) -> anyhow::Result<()> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let sender = guard.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "the node's data event sender was never captured — no bar can reach `on_bar`; \
                 the rehearsal data client did not start"
            )
        })?;
        sender
            .send(DataEvent::Data(Data::Bar(bar)))
            .map_err(|_| anyhow::anyhow!("the node's data event receiver is closed"))
    }

    /// Deliver one instrument definition into the node's cache.
    ///
    /// # Errors
    ///
    /// As [`Self::send_bar`].
    pub fn send_instrument(&self, instrument: InstrumentAny) -> anyhow::Result<()> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let sender = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("the node's data event sender was never captured"))?;
        sender
            .send(DataEvent::Instrument(instrument))
            .map_err(|_| anyhow::anyhow!("the node's data event receiver is closed"))
    }
}

/// The rehearsal's data client: a sender capture plus an instrument publication, and
/// nothing else (KTD15).
#[derive(Debug)]
pub struct RehearsalDataClient {
    client_id: ClientId,
    venue: Venue,
    sink: DataEventSink,
    /// The session's KRX equities, published on connect so they are cached BEFORE the
    /// execution client connects and reconciles.
    instruments: Vec<InstrumentAny>,
    started: bool,
}

impl RehearsalDataClient {
    /// Build the client over the sink the runner will read the sender from.
    #[must_use]
    pub fn new(
        client_id: impl Into<String>,
        venue: Venue,
        sink: DataEventSink,
        instruments: Vec<InstrumentAny>,
    ) -> Self {
        RehearsalDataClient {
            client_id: ClientId::from(client_id.into().as_str()),
            venue,
            sink,
            instruments,
            started: false,
        }
    }

    /// Capture the runner's sender if it is installed on this thread.
    fn capture_sender(&self) {
        if let Some(sender) = nautilus_common::live::runner::try_get_data_event_sender() {
            self.sink.capture(sender);
        }
    }
}

#[async_trait(?Send)]
impl DataClient for RehearsalDataClient {
    fn client_id(&self) -> ClientId {
        self.client_id
    }

    fn venue(&self) -> Option<Venue> {
        Some(self.venue)
    }

    fn start(&mut self) -> anyhow::Result<()> {
        self.capture_sender();
        self.started = true;
        Ok(())
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        self.started = false;
        Ok(())
    }

    fn reset(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn dispose(&mut self) -> anyhow::Result<()> {
        self.started = false;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.started
    }

    fn is_disconnected(&self) -> bool {
        !self.started
    }

    async fn connect(&mut self) -> anyhow::Result<()> {
        // `connect` is the phase the node drains into the cache before execution clients
        // connect, so the instruments MUST be published here and not later — publishing
        // them after would leave reconciliation with an empty instrument cache, which it
        // handles by silently skipping the position.
        self.capture_sender();
        self.started = true;
        for instrument in &self.instruments {
            self.sink.send_instrument(instrument.clone())?;
        }
        Ok(())
    }
}

/// The factory the node builder calls, handing back one pre-configured
/// [`RehearsalDataClient`].
///
/// Stateful in the same shape as
/// [`LsExecutionClientFactory::with_client`](nautilus_ls::factories::LsExecutionClientFactory::with_client)
/// and for the same reason: the runner needs the client's shared state (here, the sink) and
/// the builder gives no handle back. A second `create` is refused rather than served a
/// fresh client whose sink nobody holds.
#[derive(Debug)]
pub struct RehearsalDataClientFactory {
    sink: DataEventSink,
    venue: Venue,
    instruments: Mutex<Option<Vec<InstrumentAny>>>,
}

impl RehearsalDataClientFactory {
    /// Build the factory over the sink the runner retains and the instruments to publish.
    #[must_use]
    pub fn new(sink: DataEventSink, venue: Venue, instruments: Vec<InstrumentAny>) -> Self {
        RehearsalDataClientFactory {
            sink,
            venue,
            instruments: Mutex::new(Some(instruments)),
        }
    }
}

impl DataClientFactory for RehearsalDataClientFactory {
    fn create(
        &self,
        name: &str,
        _config: &dyn ClientConfig,
        _cache: CacheView,
        _clock: std::rc::Rc<std::cell::RefCell<dyn Clock>>,
    ) -> anyhow::Result<Box<dyn DataClient>> {
        let instruments = self
            .instruments
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "the rehearsal data client was already handed to a node — a second one would \
                     capture a different sender than the runner holds, and its bars would reach \
                     no `on_bar`"
                )
            })?;
        Ok(Box::new(RehearsalDataClient::new(
            name,
            self.venue,
            self.sink.clone(),
            instruments,
        )))
    }

    fn name(&self) -> &str {
        "LS-REHEARSAL-DATA"
    }

    fn config_type(&self) -> &str {
        "LsAdapterConfig"
    }
}

/// The segment routing map the exec/mark paths key on, built from the session's cached
/// equities. Kept here beside the client that publishes them so one traversal produces both.
#[must_use]
pub fn market_map(instruments: &[InstrumentAny]) -> HashMap<nautilus_model::identifiers::InstrumentId, nautilus_ls::rules::Market> {
    instruments
        .iter()
        .filter_map(|i| match i {
            InstrumentAny::Equity(e) => Some((e.id, nautilus_ls::instruments::equity_market(e))),
            _ => None,
        })
        .collect()
}
