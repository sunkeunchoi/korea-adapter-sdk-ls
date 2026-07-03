//! Concrete [`crate::agent::policy::AgentPolicy`] implementations (R2, R8).
//!
//! One policy ships this increment — [`research`]'s deterministic
//! Research-tier demonstrator, the sole intent producer (KTD3); LLM-backed or
//! management-tier policies slot in beside it behind the same seam.

pub mod research;

pub use research::*;
