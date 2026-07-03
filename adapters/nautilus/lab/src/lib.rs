//! `nautilus-ls-lab` — the strategy-improvement loop for the `nautilus-ls` adapter.
//!
//! The lab is a **separate crate** from the certified adapter (KTD1): strategy code,
//! runners, and the artifact writer live here so strategy churn never destabilizes
//! the adapter, whose contract is translation only. This crate carries:
//!
//! - [`params`] — ORB v0's parameter set (all manifest-recorded, KTD6).
//! - [`strategy`] — the ORB v0 payload: universe scan + range/entry/exit machine (R2).
//! - [`signals`] — the per-decision signal log types + JSONL sink (KTD9, R6).
//! - [`artifacts`] — the RunWriter and the four run artifacts (KTD2, R4–R9).
//! - [`runner`] — the backtest and live-paper runners (F1, F2).
//!
//! Every run emits the same four artifacts (performance, signals, data-quality,
//! manifest) into an append-only registry so an agent can analyze any run — backtest
//! or live — and propose the next strategy change.

pub mod artifacts;
pub mod params;
pub mod runner;
pub mod signals;
pub mod strategy;
