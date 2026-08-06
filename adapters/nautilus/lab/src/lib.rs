//! `nautilus-ls-lab` — the strategy-improvement loop for the `nautilus-ls` adapter.
//!
//! The lab is a **separate crate** from the certified adapter (KTD1): strategy code,
//! runners, and the artifact writer live here so strategy churn never destabilizes
//! the adapter, whose contract is translation only. This crate carries:
//!
//! - [`agent`] — the agent decision layer: decision envelope, intent, runtime
//!   action, context, and the in-run decision sink (R1, R5, R6, R9).
//! - [`params`] — ORB v0's parameter set (all manifest-recorded, KTD6).
//! - [`strategy`] — the ORB v0 payload: universe scan + range/entry/exit machine (R2).
//! - [`artifacts`] — the RunWriter and the four run artifacts (KTD2, R4–R9).
//! - [`runner`] — the backtest and live-paper runners (F1, F2).
//! - [`margin`] — the frozen pre-registered sample margin (`config/sample-margin.json`).
//! - [`stats`] — the sample-sufficiency statistics core: clustering, design
//!   effect, minimum detectable edge, required trade count, and the
//!   trials-corrected margin threshold (plan 2026-08-05-001).
//!
//! Every run emits the same four artifacts (performance, decisions, data-quality,
//! manifest) into an append-only registry so an agent can analyze any run — backtest
//! or live — and propose the next strategy change.

pub mod agent;
pub mod artifacts;
pub mod candidates;
pub mod dispatch;
pub mod fingerprint;
pub mod margin;
pub mod params;
pub mod queue;
pub mod runner;
pub mod stats;
pub mod strategy;
pub mod trials;
