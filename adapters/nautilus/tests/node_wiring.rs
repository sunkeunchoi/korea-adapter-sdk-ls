//! U7 offline: the adapter is mountable in a pure-Rust `LiveNode` — both factories
//! `create` their clients when the node builds. No network, no credentials beyond
//! the injected paper mock config.

use ls_core::{Environment as LsEnvironment, LsConfig};
use nautilus_common::enums::Environment;
use nautilus_ls::config::LsAdapterConfig;
use nautilus_ls::factories::{LsDataClientFactory, LsExecutionClientFactory};
use nautilus_live::node::LiveNode;
use nautilus_model::identifiers::TraderId;

fn paper_config() -> LsAdapterConfig {
    let ls = LsConfig {
        appkey: "test-appkey".into(),
        appsecretkey: "test-secret".into(),
        account_no: "00000000-01".into(),
        environment: LsEnvironment::Paper,
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
    LsAdapterConfig::explicit(ls)
}

#[tokio::test(flavor = "current_thread")]
async fn node_builds_with_both_clients_registered() {
    let node = LiveNode::builder(TraderId::from("LS-TRADER-001"), Environment::Live)
        .expect("builder")
        .with_name("ls-krx-test")
        .add_data_client(
            None,
            Box::new(LsDataClientFactory),
            Box::new(paper_config()),
        )
        .expect("data client registered + created")
        .add_exec_client(
            None,
            Box::new(LsExecutionClientFactory),
            Box::new(paper_config()),
        )
        .expect("exec client registered + created")
        .build();

    // The node builds offline with both LS clients mounted (factory `create` ran).
    assert!(node.is_ok(), "node builds with both LS clients: {:?}", node.err());
}
