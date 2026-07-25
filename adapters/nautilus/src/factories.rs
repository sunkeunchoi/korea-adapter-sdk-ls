//! Client factories + `LiveNode` wiring (U7).
//!
//! `LsDataClientFactory` / `LsExecutionClientFactory` implement the nautilus factory
//! traits, downcasting the `&dyn ClientConfig` the `LiveNode` builder hands them to
//! [`LsAdapterConfig`] and constructing the SDK-backed clients. A wrong config type
//! is a **named** error, never a silent mismap.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use ls_sdk::LsSdk;
use nautilus_common::cache::CacheView;
use nautilus_common::clients::{DataClient, ExecutionClient};
use nautilus_common::clock::Clock;
use nautilus_common::factories::{ClientConfig, DataClientFactory, ExecutionClientFactory};
use nautilus_model::enums::AccountType;

use crate::config::LsAdapterConfig;
use crate::data::LsDataClient;
use crate::execution::LsExecClient;
use crate::orders::ledger::FillLedger;

/// The config type name both factories accept (for `config_type` + error text).
const CONFIG_TYPE: &str = "LsAdapterConfig";

fn downcast(config: &dyn ClientConfig) -> anyhow::Result<&LsAdapterConfig> {
    config
        .as_any()
        .downcast_ref::<LsAdapterConfig>()
        .ok_or_else(|| anyhow::anyhow!("expected a {CONFIG_TYPE}, got a different config type"))
}

fn build_sdk(cfg: &LsAdapterConfig) -> anyhow::Result<(LsSdk, String)> {
    let ls_config = cfg.build_config().map_err(|e| anyhow::anyhow!("{e}"))?;
    let account_no = ls_config.account_no.clone();
    let sdk = LsSdk::new(ls_config).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok((sdk, account_no))
}

/// Factory for the LS domestic-equity data client.
#[derive(Debug, Default)]
pub struct LsDataClientFactory;

impl DataClientFactory for LsDataClientFactory {
    fn create(
        &self,
        name: &str,
        config: &dyn ClientConfig,
        _cache: CacheView,
        _clock: Rc<RefCell<dyn Clock>>,
    ) -> anyhow::Result<Box<dyn DataClient>> {
        let cfg = downcast(config)?;
        let (sdk, _account) = build_sdk(cfg)?;
        // Segment routing is populated by the operator/provider; an empty map
        // defaults to KOSPI routing until instruments are known.
        let client = LsDataClient::new(name, sdk, HashMap::new());
        Ok(Box::new(client))
    }

    fn name(&self) -> &str {
        "LS-DATA"
    }

    fn config_type(&self) -> &str {
        CONFIG_TYPE
    }
}

/// What an [`LsExecutionClientFactory`] hands the node builder.
enum Preset {
    /// The shipped default: build a fresh client (and a fresh SDK) from the config.
    Stateless,
    /// A caller-built client to hand back on the next `create` (live-session-driver KTD3).
    Ready(Box<LsExecClient>),
    /// The pre-built client was already handed to a node.
    Consumed,
}

/// Factory for the LS domestic cash-equity execution client.
///
/// **Stateless by default** — `create` builds a fresh client from the config, exactly as
/// it always has. [`with_client`](Self::with_client) makes it *stateful*: it hands the
/// caller's pre-built client to the node builder **once** (live-session-driver KTD3).
///
/// Why the statefulness exists: a live session's fail-closed teardown must engage the kill
/// switch that gates **the node's** order path, and its max-loss breaker must read **the
/// node's** fills. Both live behind handles that are unreachable after `LiveNode::build()`
/// (the exec client is type-erased in `Vec<LiveExecutionClient>` with no downcast, and the
/// `ExecutionClient` trait exposes neither). A teardown that built its own client would
/// call `set_orders_enabled(false)` on a *different* `AtomicBool` and read an *empty*
/// `FillLedger` — two silent no-ops on exactly the safety acts that matter. So the runner
/// builds one `LsSdk` + one `Arc<Mutex<FillLedger>>`, constructs the client itself, hands
/// it here, and retains the shared state for the teardown/feeder handle.
///
/// Interior mutability is required because the nautilus trait takes `&self`.
#[derive(Default)]
pub struct LsExecutionClientFactory {
    preset: Mutex<Preset>,
    /// The shared state of the client actually handed to the node — the wiring tests'
    /// probe, so "the node's kill switch / ledger" is asserted against what the node
    /// really got, not against what the caller intended to give it.
    handed: Mutex<Option<HandedClient>>,
}

/// The shared state of the client a factory handed to a node (the wiring probe).
#[derive(Clone)]
pub struct HandedClient {
    /// The client's SDK handle — the same `Arc<Inner>`, hence the same kill switch.
    pub sdk: LsSdk,
    /// The client's fill ledger `Arc` — what the breaker feeder must share.
    pub ledger: Arc<Mutex<FillLedger>>,
}

impl Default for Preset {
    fn default() -> Self {
        Preset::Stateless
    }
}

impl std::fmt::Debug for LsExecutionClientFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the client/SDK: it carries resolved credential state.
        let state = match &*self.preset.lock().unwrap_or_else(|e| e.into_inner()) {
            Preset::Stateless => "stateless",
            Preset::Ready(_) => "pre-built",
            Preset::Consumed => "consumed",
        };
        f.debug_struct("LsExecutionClientFactory").field("preset", &state).finish()
    }
}

impl LsExecutionClientFactory {
    /// The shipped stateless factory — `create` builds a fresh client from the config.
    pub fn new() -> Self {
        Self::default()
    }

    /// A **stateful** factory that hands `client` to the node builder exactly once (KTD3).
    pub fn with_client(client: LsExecClient) -> Self {
        LsExecutionClientFactory {
            preset: Mutex::new(Preset::Ready(Box::new(client))),
            handed: Mutex::new(None),
        }
    }

    /// The shared state of the client this factory handed to a node, once `create` has
    /// run. `None` before the node builder calls it.
    pub fn handed(&self) -> Option<HandedClient> {
        self.handed.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl ExecutionClientFactory for LsExecutionClientFactory {
    fn create(
        &self,
        name: &str,
        config: &dyn ClientConfig,
        _cache: CacheView,
    ) -> anyhow::Result<Box<dyn ExecutionClient>> {
        let client = {
            let mut preset = self.preset.lock().unwrap_or_else(|e| e.into_inner());
            match std::mem::replace(&mut *preset, Preset::Consumed) {
                Preset::Ready(client) => *client,
                Preset::Stateless => {
                    // Restore: a stateless factory may serve repeatedly, as before.
                    *preset = Preset::Stateless;
                    let cfg = downcast(config)?;
                    let (sdk, account_no) = build_sdk(cfg)?;
                    LsExecClient::new(name, cfg.trader_id.clone(), account_no, sdk, AccountType::Cash)
                }
                // Fail loudly rather than quietly building a SECOND client: a second node
                // served from this factory would get a different kill switch and ledger
                // than the retained teardown handle — the exact silent no-op this design
                // exists to prevent.
                Preset::Consumed => anyhow::bail!(
                    "the pre-built {CONFIG_TYPE} execution client was already handed to a node — a \
                     stateful LS-EXEC factory serves exactly one node; building a fresh client here \
                     would give it a DIFFERENT kill switch and fill ledger than the retained \
                     teardown handle"
                ),
            }
        };
        *self.handed.lock().unwrap_or_else(|e| e.into_inner()) = Some(HandedClient {
            sdk: client.sdk(),
            ledger: client.ledger_handle(),
        });
        Ok(Box::new(client))
    }

    fn name(&self) -> &str {
        "LS-EXEC"
    }

    fn config_type(&self) -> &str {
        CONFIG_TYPE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct WrongConfig;
    impl ClientConfig for WrongConfig {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[test]
    fn downcast_rejects_wrong_config_with_named_error() {
        let err = downcast(&WrongConfig).unwrap_err();
        assert!(err.to_string().contains("LsAdapterConfig"), "names the expected type: {err}");
    }

    #[test]
    fn factory_names_and_config_types() {
        assert_eq!(LsDataClientFactory.name(), "LS-DATA");
        assert_eq!(LsExecutionClientFactory::new().name(), "LS-EXEC");
        assert_eq!(LsDataClientFactory.config_type(), CONFIG_TYPE);
        assert_eq!(LsExecutionClientFactory::new().config_type(), CONFIG_TYPE);
    }

    /// KTD3: a stateful factory serves exactly ONE node. A second `create` must fail
    /// loudly rather than quietly build a fresh client — a second client would carry a
    /// different kill switch and fill ledger than the retained teardown handle, silently
    /// defeating both safety acts.
    #[test]
    fn a_stateful_factory_refuses_to_serve_a_second_node() {
        use ls_core::{Environment, LsConfig};
        let ls = LsConfig {
            appkey: "test-appkey".into(),
            appsecretkey: "test-secret".into(),
            account_no: "00000000-01".into(),
            environment: Environment::Paper,
            rate_limits: None,
            base_url: None,
            ws_base_url: None,
            max_pages: None,
            connect_timeout_secs: None,
            request_timeout_secs: None,
            ws_connect_timeout_secs: None,
            allow_insecure_localhost: false,
            ws_channel_capacity: None,
            ws_overflow_policy: None,
        };
        let cfg = LsAdapterConfig::explicit(ls.clone());
        let sdk = LsSdk::new(ls).unwrap();
        let client = LsExecClient::new("LS-EXEC", "LS-LAB-001", "00000000-01", sdk.clone(), AccountType::Cash);
        let factory = LsExecutionClientFactory::with_client(client);

        let cache = || CacheView::new(Rc::new(RefCell::new(nautilus_common::cache::Cache::default())));

        assert!(factory.handed().is_none(), "nothing is handed before the builder calls create");
        assert!(
            factory.create("LS-EXEC", &cfg, cache()).is_ok(),
            "the pre-built client is handed to the first node"
        );
        let handed = factory.handed().expect("the factory records what it handed over");
        assert!(
            Arc::ptr_eq(handed.sdk.inner(), sdk.inner()),
            "the node got the caller's SDK — the same Arc<Inner>, hence the same kill switch"
        );

        let err = match factory.create("LS-EXEC", &cfg, cache()) {
            Ok(_) => panic!("a second node must be refused, not silently served a different client"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("already handed"), "names the cause: {err}");
    }
}
