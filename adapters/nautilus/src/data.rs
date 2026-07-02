//! The live market-data client — a nautilus [`DataClient`] over the SDK's realtime
//! WS lanes (U5).
//!
//! Subscribe/unsubscribe are synchronous trait methods; they resolve the
//! market-segment `tr_cd` (S3_/K3_ trades, H1_/HA_ quotes) and enqueue a command to
//! the [`WsSupervisor`], which owns the `WsManager` and performs the async work
//! (KTD8). Decoded ticks flow to nautilus via the data event sender.

use std::collections::HashMap;

use async_trait::async_trait;
use ls_sdk::LsSdk;
use nautilus_common::clients::DataClient;
use nautilus_common::messages::data::{
    SubscribeQuotes, SubscribeTrades, UnsubscribeQuotes, UnsubscribeTrades,
};
use nautilus_model::identifiers::{ClientId, InstrumentId, Venue};

use crate::rules::Market;
use crate::ws::supervisor::{RowKind, SubSpec, WsSupervisor};
use crate::ws::{quote_tr_cd, trade_tr_cd};
use crate::KRX_VENUE;

/// The LS domestic-equity data client.
pub struct LsDataClient {
    client_id: ClientId,
    venue: Venue,
    sdk: LsSdk,
    /// Segment routing: which KRX market each instrument trades on (S3_ vs K3_).
    market_map: HashMap<InstrumentId, Market>,
    /// Spawned lazily in [`DataClient::start`] (needs the runner's data event
    /// sender). Tests inject one via [`Self::with_supervisor`].
    supervisor: Option<WsSupervisor>,
}

impl LsDataClient {
    /// Build a data client. The supervisor is spawned in [`DataClient::start`],
    /// which captures the runner's data event sender.
    pub fn new(client_id: impl Into<String>, sdk: LsSdk, market_map: HashMap<InstrumentId, Market>) -> Self {
        LsDataClient {
            client_id: ClientId::from(client_id.into().as_str()),
            venue: Venue::from(KRX_VENUE),
            sdk,
            market_map,
            supervisor: None,
        }
    }

    /// Build a data client with a pre-spawned supervisor (test seam — bypasses the
    /// runner-thread data-event-sender capture).
    pub fn with_supervisor(
        client_id: impl Into<String>,
        sdk: LsSdk,
        market_map: HashMap<InstrumentId, Market>,
        supervisor: WsSupervisor,
    ) -> Self {
        LsDataClient {
            client_id: ClientId::from(client_id.into().as_str()),
            venue: Venue::from(KRX_VENUE),
            sdk,
            market_map,
            supervisor: Some(supervisor),
        }
    }

    fn market_of(&self, id: &InstrumentId) -> Market {
        self.market_map.get(id).copied().unwrap_or(Market::Kospi)
    }

    fn spec(&self, id: InstrumentId, kind: RowKind) -> SubSpec {
        let market = self.market_of(&id);
        let tr_cd = match kind {
            RowKind::Trade => trade_tr_cd(market),
            RowKind::Quote => quote_tr_cd(market),
        };
        SubSpec {
            tr_cd: tr_cd.to_string(),
            tr_key: id.symbol.as_str().to_string(),
            instrument_id: id,
            kind,
        }
    }

    fn supervisor(&self) -> Option<&WsSupervisor> {
        self.supervisor.as_ref()
    }
}

#[async_trait(?Send)]
impl DataClient for LsDataClient {
    fn client_id(&self) -> ClientId {
        self.client_id
    }

    fn venue(&self) -> Option<Venue> {
        Some(self.venue)
    }

    fn start(&mut self) -> anyhow::Result<()> {
        if self.supervisor.is_none() {
            // Capture the runner's data event sender (panics outside an
            // initialized runner — the live node initializes it before start()).
            let emit = nautilus_common::live::runner::get_data_event_sender();
            self.supervisor = Some(WsSupervisor::spawn(self.sdk.clone(), emit));
        }
        Ok(())
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        if let Some(sup) = &self.supervisor {
            sup.shutdown();
        }
        Ok(())
    }

    fn reset(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn dispose(&mut self) -> anyhow::Result<()> {
        if let Some(sup) = self.supervisor.take() {
            sup.shutdown();
        }
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.supervisor().map(|s| s.is_connected()).unwrap_or(false)
    }

    fn is_disconnected(&self) -> bool {
        !self.is_connected()
    }

    fn subscribe_trades(&mut self, cmd: SubscribeTrades) -> anyhow::Result<()> {
        if let Some(sup) = self.supervisor() {
            sup.subscribe(self.spec(cmd.instrument_id, RowKind::Trade));
        } else {
            anyhow::bail!("data client not started");
        }
        Ok(())
    }

    fn subscribe_quotes(&mut self, cmd: SubscribeQuotes) -> anyhow::Result<()> {
        if let Some(sup) = self.supervisor() {
            sup.subscribe(self.spec(cmd.instrument_id, RowKind::Quote));
        } else {
            anyhow::bail!("data client not started");
        }
        Ok(())
    }

    fn unsubscribe_trades(&mut self, cmd: &UnsubscribeTrades) -> anyhow::Result<()> {
        if let Some(sup) = self.supervisor() {
            let market = self.market_of(&cmd.instrument_id);
            sup.unsubscribe(trade_tr_cd(market), cmd.instrument_id.symbol.as_str());
        }
        Ok(())
    }

    fn unsubscribe_quotes(&mut self, cmd: &UnsubscribeQuotes) -> anyhow::Result<()> {
        if let Some(sup) = self.supervisor() {
            let market = self.market_of(&cmd.instrument_id);
            sup.unsubscribe(quote_tr_cd(market), cmd.instrument_id.symbol.as_str());
        }
        Ok(())
    }
}
