//! Strategy payloads for the lab. ORB v0 is the first (KTD6).

pub mod orb;

/// The ORB strategy source, embedded at compile time. Its hash rides in the run
/// manifest (`strategy_code_hash`) so a change to the strategy *logic* is visible in
/// the manifest even if the operator forgets to bump `strategy_version` — the
/// comparability guarantee (AE1) then catches silent logic drift, not just parameter
/// changes.
pub const ORB_SOURCE: &str = include_str!("orb.rs");
