//! `ls-core` — the transport-agnostic runtime for the maintained LS Securities SDK.
//!
//! Ported from the Migration Source `korea-broker-sdk-ls` `crates/core`, stripped of
//! generator coupling. Houses auth, config, transport dispatch, rate limiting,
//! pagination, and the load-bearing serde wire-compat helpers.
