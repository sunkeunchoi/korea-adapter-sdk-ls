//! Client factories + `LiveNode` wiring (U7).
//!
//! `LsDataClientFactory` / `LsExecutionClientFactory` implement the nautilus factory
//! traits, downcasting the `&dyn ClientConfig` the `LiveNode` builder hands them to
//! [`LsAdapterConfig`] and constructing the SDK-backed clients. A wrong config type
//! is a **named** error, never a silent mismap.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ls_sdk::LsSdk;
use nautilus_common::cache::CacheView;
use nautilus_common::clients::{DataClient, ExecutionClient};
use nautilus_common::clock::Clock;
use nautilus_common::factories::{ClientConfig, DataClientFactory, ExecutionClientFactory};
use nautilus_model::enums::AccountType;

use crate::config::LsAdapterConfig;
use crate::data::LsDataClient;
use crate::execution::LsExecClient;

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

/// Factory for the LS domestic cash-equity execution client.
#[derive(Debug, Default)]
pub struct LsExecutionClientFactory;

impl ExecutionClientFactory for LsExecutionClientFactory {
    fn create(
        &self,
        name: &str,
        config: &dyn ClientConfig,
        _cache: CacheView,
    ) -> anyhow::Result<Box<dyn ExecutionClient>> {
        let cfg = downcast(config)?;
        let (sdk, account_no) = build_sdk(cfg)?;
        let client = LsExecClient::new(
            name,
            cfg.trader_id.clone(),
            account_no,
            sdk,
            AccountType::Cash,
        );
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
        assert_eq!(LsExecutionClientFactory.name(), "LS-EXEC");
        assert_eq!(LsDataClientFactory.config_type(), CONFIG_TYPE);
        assert_eq!(LsExecutionClientFactory.config_type(), CONFIG_TYPE);
    }
}
