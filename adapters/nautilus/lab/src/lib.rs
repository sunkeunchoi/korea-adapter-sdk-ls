//! `nautilus-ls-lab` — the strategy-improvement loop for the `nautilus-ls` adapter.
//!
//! The lab is a **separate crate** from the certified adapter (KTD1): strategy code,
//! runners, and the artifact writer live here so strategy churn never destabilizes
//! the adapter, whose contract is translation only. This crate carries:
//!
//! - [`agent`] — the agent decision layer: decision envelope, intent, runtime
//!   action, context, and the in-run decision sink (R1, R5, R6, R9).
//! - [`params`] — ORB v0's parameter set (all manifest-recorded, KTD6).
//! - [`params_daily`] — the daily-resolution multi-session-hold path's parameter set
//!   (P7), carried in the manifest as an optional sibling of [`params`] so no ORB
//!   identity hash moves.
//! - [`strategy`] — the ORB v0 payload: universe scan + range/entry/exit machine (R2).
//! - [`artifacts`] — the RunWriter and the run artifacts (KTD2, R4–R9): four on every
//!   run, plus the daily path's conditional [`artifacts::observation`] (P7).
//! - [`runner`] — the backtest and live-paper runners (F1, F2).
//! - [`margin`] — the frozen pre-registered sample margin (`config/sample-margin.json`).
//! - [`lineage_prereg`] — the successor daily lineage's frozen pre-registration
//!   (`config/lineage-preregistration.json`) and the claim-then-evaluate holdout
//!   judgment ledger (plan 2026-08-14-001). Distinct from [`dispatch::prereg`], which
//!   freezes the production ladder.
//! - [`stats`] — the sample-sufficiency statistics core: clustering, design
//!   effect, minimum detectable edge, required trade count, and the
//!   trials-corrected margin threshold (plan 2026-08-05-001).
//!
//! Every run emits the same four artifacts (performance, decisions, data-quality,
//! manifest) into an append-only registry so an agent can analyze any run — backtest
//! or live — and propose the next strategy change.
//!
//! A **daily-path** run emits a fifth, `observation.json` (P7/U6): the typed run
//! observation the holdout judgment and the pre-turn admissibility re-check both read. It
//! is conditional in two directions — an ORB run never writes one, and a daily run whose
//! `return_on_risk` is `None` refuses to (R25). So "the same four" still holds as the
//! floor; five is not the new invariant.

pub mod agent;
pub mod artifacts;
pub mod candidates;
pub mod dispatch;
pub mod fingerprint;
pub mod lineage_prereg;
pub mod margin;
pub mod params;
pub mod params_daily;
pub mod queue;
pub mod runner;
pub mod stats;
pub mod strategy;
pub mod trials;
