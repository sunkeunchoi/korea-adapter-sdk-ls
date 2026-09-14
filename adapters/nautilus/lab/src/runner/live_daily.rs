//! The rehearsal's day: the book it inherits, the sweep that feeds it, the 15:20 decision,
//! and the synthetic bar that carries that decision into the frozen strategy (U9).
//!
//! Three things live here because they are one mechanism, not three:
//!
//! - **`rehearsal/book.json`** (KTD11) — the record of what the BROKER confirmed at the end
//!   of the last session. It is written from the teardown's t0424 snapshot and never from
//!   the fill ledger, so a late fill cannot make the next session's pre-mount probe refuse a
//!   book the account actually holds.
//! - **The day loop** — a single heartbeat-bearing loop through the 15:00→15:33 session. It
//!   is one loop rather than a phase per step because the dead-man feeder must be touched on
//!   EVERY tick: the strategy only touches it from `on_bar`, and this session has exactly one
//!   bar per symbol, twenty minutes in. A phase that forgot to heartbeat would trip the
//!   envelope on a perfectly healthy sweep.
//! - **The bar synthesis** (KTD4) — 15:20 is the end of continuous trading, so that read's
//!   `open/high/low/price/volume` IS the day's continuous-session bar. The decision taken on
//!   it executes in the 15:30 closing auction, which bounds the divergence from the frozen
//!   mechanism to two measurable quantities, both recorded here.

use std::process::ExitCode;



/// The schema version `rehearsal/book.json` is written at.
///
/// v2 replaced v1's positional `entry_session_ordinal` with `entry_date`; a v1 book cannot be
/// upgraded in place because the ordinal it stores can no longer be resolved to a date once
/// the calendar it was counted against has moved. [`RehearsalBook::load`] refuses it.
pub const BOOK_VERSION: u32 = 2;

/// The fixed date session ordinals are counted from.
///
/// Ordinals must be comparable ACROSS sessions — a leg's hold elapses as
/// `current_index − entry_index` (R23) — so they cannot be counted from a moving base such
/// as the calendar snapshot's materialized floor. Anchoring them to a constant and
/// recording that constant in the book is what makes a base change detectable instead of
/// silently shortening or extending every open hold.
pub const SESSION_ORDINAL_EPOCH: &str = "2010-01-04";


pub mod book;
pub mod day_loop;
pub mod mount;
pub mod quote;

// The sections below moved out verbatim; the re-exports keep every `runner::live_daily::…`
// path the crate and `lab/tests/` already read.
pub use book::*;
pub use day_loop::*;
pub use mount::*;
pub use quote::*;
