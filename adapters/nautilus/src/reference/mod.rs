//! Reference-data universe engine (Turn N, plan 2026-07-10-003).
//!
//! Decorates every KRX symbol with instrument metadata (market class, cap tier,
//! liquidity tier, index membership, derivative availability, tradability) and
//! lets per-session filters + ranking define the tradeable set. Two consumers
//! read the one [`universe_metadata::UniverseMetadata`] artifact: `ls-ingest`
//! (reproducible tier-stratified symbol selection, R6) and the lab backtest
//! (candidate join + gated selection, R10).
//!
//! - [`universe_metadata`] — the record/artifact schema plus the **pure**
//!   tier-assignment, tradability-gate, and stratified-sample logic (U1).
//! - [`capture`] — the live capture that joins the six reference TRs by
//!   `shcode` into the artifact (U2; attended, paper-only).

pub mod capture;
pub mod universe_metadata;
