//! `nautilus-ls` — a nautilus_trader v2 (Rust) adapter for LS Securities (Korea),
//! v1 scope domestic KRX cash equities.
//!
//! The adapter is a **translation layer** over [`ls_sdk::LsSdk`]: it owns no
//! transport, credentials, or rate limiting of its own. ls-core remains the single
//! transport + safety authority (rate buckets, kill switch, order dedup, preflight,
//! ambiguous-order fail-closed). See `docs/plans/2026-07-02-003-…` for the contract.
//!
//! ## Module map
//!
//! - [`config`] — adapter config → [`ls_core::LsConfig`], paper-only interlock (U1).
//! - [`rules`] — KRX/KOSDAQ tick-size bands (both regimes) + session times (U2).
//! - [`instruments`] — t8430/t9945 → nautilus `Equity`, domain gating (U2).
//! - [`ingest`] — resumable, rate-correct historical bar backfill (U3).
//! - [`ws`] — adapter-owned frame rows + reconnect supervisor (U5/U6).
//! - [`data`] — `DataClient` impl: trades + top-of-book quotes (U5).
//! - [`execution`] / [`orders`] — `ExecutionClient` impl, variant-keyed mapping (U6).
//! - [`factories`] — factory traits + `LiveNode` wiring (U7).
//! - [`scrub`] — credential scrubbing for the bin targets (U7).
//!
//! Everything is verifiable offline against the mock gateway
//! (`ls-sdk-test-support`); the live tester binaries are operator-gated.

pub mod config;
pub mod error;
pub mod instruments;
pub mod lock;
pub mod rules;

pub mod ingest;

pub mod data;
pub mod execution;
pub mod factories;
pub mod orders;
pub mod scrub;
pub mod ws;

pub use error::{AdapterError, AdapterResult};

/// The KRX venue MIC used for every domestic instrument id (`{shcode}.XKRX`, KTD7).
///
/// KRX (Korea Exchange) publishes both the KOSPI and KOSDAQ segments under the
/// single ISO-10383 MIC `XKRX`; the adapter keys nautilus `Venue` on it.
pub const KRX_VENUE: &str = "XKRX";
