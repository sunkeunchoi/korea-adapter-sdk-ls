//! `ls-metadata` — Rust-owned TR maintenance metadata.
//!
//! The serde structs and validator here are the single authority that validates
//! per-TR YAML; `metadata/tr-index.yaml` duplicates only routing fields and is
//! checked against `metadata/trs/*.yaml`. The change-scoped planner selects the
//! verification set from changed TRs, owning dependency classes, and facets.
//! No hand-maintained JSON Schema (ADR 0012).
