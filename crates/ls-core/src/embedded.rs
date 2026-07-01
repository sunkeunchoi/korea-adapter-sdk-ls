//! Build-time-embedded `metadata/` data (see `build.rs`).
//!
//! `metadata/` is at the workspace root, outside this crate. `build.rs` reads the
//! runtime-consumed data files and emits `embedded_metadata.rs` into `OUT_DIR`
//! with their contents as `&'static str` literals; this module `include!`s it so
//! the rest of the crate reads embedded data with zero filesystem access.

include!(concat!(env!("OUT_DIR"), "/embedded_metadata.rs"));
