//! Strategy payloads for the lab. ORB v0 is the first (KTD6).

pub mod daily;
pub mod orb;

/// The ORB strategy source, embedded at compile time. Its hash rides in the run
/// manifest (`strategy_code_hash`) so a change to the strategy *logic* is visible in
/// the manifest even if the operator forgets to bump `strategy_version` — the
/// comparability guarantee (AE1) then catches silent logic drift, not just parameter
/// changes.
pub const ORB_SOURCE: &str = include_str!("orb.rs");

/// The daily multi-session-hold strategy source, embedded at compile time (P7 U3/U4).
/// Hashed by [`crate::artifacts::manifest::daily_strategy_code_hash`] into a daily
/// run's `strategy_code_hash`, exactly as [`ORB_SOURCE`] is for an ORB run — so the
/// two lineages carry distinct source identities and neither can be mistaken for the
/// other in the registry. Deliberately a SIBLING const rather than a strategy-id
/// dispatch inside `strategy_code_hash()` (KTD5): that function's eight production
/// call sites are all ORB-domain, so parameterising it would buy eight edits that all
/// pass the literal `"orb"` on the most identity-critical function in the crate.
pub const DAILY_SOURCE: &str = include_str!("daily.rs");
