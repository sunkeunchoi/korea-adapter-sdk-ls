//! Structural API Shape types — the committed per-TR shape vocabulary.
//!
//! These types ([`TrShape`], [`BlockField`], [`Direction`]) originate with the
//! API Drift Tracker (`ls-trackers`), which normalizes a fetched LS API artifact
//! into a [`TrShape`] and diffs two shapes structurally. They live **here** in
//! `ls-metadata` so the Focused-Evidence record ([`crate::schema::EvidenceRecord`])
//! can carry a frozen attested [`TrShape`] as a typed field: `EvidenceRecord` is
//! metadata-owned and cannot depend on `ls-trackers` (the dependency runs the
//! other way), so the shape types are pushed down for the record and its
//! validator to hold a typed attested shape.
//!
//! `ls-trackers` re-exports these (`ls_trackers::{TrShape, BlockField, Direction}`)
//! for source compatibility, and continues to own the diff engine
//! (`diff_shapes`/`DriftChange`) and the normalizer that produces a [`TrShape`].
//! A lossy projection would not survive that engine — `diff_shapes` consumes every
//! [`BlockField`] field — so the full shape relocates rather than a subset.
//!
//! **Wire-format contract:** committed baseline `trs/*.json` and stored attested
//! shapes deserialize and re-serialize through these structs, so the derives and
//! serde attributes are load-bearing and must not drift. To stay forward/backward
//! compatible with already-stored attested shapes, `TrShape`/`BlockField` may only
//! ever **grow** `skip_serializing_if` `Option` fields; a non-additive change is
//! itself a re-attestation trigger.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::schema::Protocol;

/// Field traversal direction — part of field identity (R6).
///
/// Originally tracker-owned; relocated into `ls-metadata` alongside
/// [`TrShape`]/[`BlockField`] (it is a [`BlockField`] field). It is also a field
/// of `ls-trackers`' `DriftChange`/`SpecChange` (which stay up and consume it
/// through the dependency edge), so after the move it is **metadata-owned shared
/// field-traversal vocabulary**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Request,
    Response,
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Direction::Request => "request",
            Direction::Response => "response",
        })
    }
}

/// One normalized field within a request/response block (R6, R8). Field identity
/// is `(direction, block_name, field_index, field_name)`; `description_hash` is
/// the stable hash of the normalized long description so benign re-encoding does
/// not register as drift.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockField {
    pub direction: Direction,
    pub block_name: String,
    pub field_index: u32,
    pub field_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub korean_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u32>,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_hash: Option<String>,
}

/// The committed per-TR **Structural API Shape** (maintained TRs only, R5). Long
/// descriptions/examples are stored as `description_hash`; compact names are
/// preserved verbatim (R8). Endpoint/protocol/rate facts are top-level (R7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrShape {
    pub tr_code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tr_name: Option<String>,
    pub protocol: Protocol,
    pub is_websocket: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_group_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_group_name: Option<String>,
    /// Request-direction blocks, ordered as upstream presents them. `field_index`
    /// preserves position so U4's reorder reconciliation can run.
    #[serde(default)]
    pub request_blocks: Vec<BlockField>,
    /// Response-direction blocks, ordered as upstream presents them.
    #[serde(default)]
    pub response_blocks: Vec<BlockField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit_per_sec: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corp_rate_limit_per_sec: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_source_group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_hash: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ls-metadata` can build and serialize a `TrShape`/`BlockField`/`Direction`
    /// without referencing `ls-trackers` — the relocation is self-contained, and a
    /// shape round-trips byte-stably (the committed-baseline serde contract).
    #[test]
    fn shape_round_trips_without_trackers() {
        let shape = TrShape {
            tr_code: "t8412".to_string(),
            tr_name: Some("주식차트(N분)".to_string()),
            protocol: Protocol::Rest,
            is_websocket: false,
            endpoint_path: Some("/stock/chart".to_string()),
            api_group_id: Some("g1".to_string()),
            source_group_name: Some("주식시세".to_string()),
            request_blocks: vec![BlockField {
                direction: Direction::Request,
                block_name: "t8412InBlock".to_string(),
                field_index: 0,
                field_name: "shcode".to_string(),
                korean_name: Some("종목코드".to_string()),
                r#type: Some("String".to_string()),
                length: Some(6),
                required: true,
                description_hash: Some("abc123".to_string()),
            }],
            response_blocks: vec![],
            rate_limit_per_sec: Some(1),
            corp_rate_limit_per_sec: None,
            rate_source_group: Some("g1".to_string()),
            description_hash: Some("def456".to_string()),
        };
        let a = serde_json::to_vec(&shape).unwrap();
        let b = serde_json::to_vec(&shape).unwrap();
        assert_eq!(a, b, "TR shape serialization is byte-stable");
        let back: TrShape = serde_json::from_slice(&a).unwrap();
        assert_eq!(back, shape, "TR shape round-trips");
        assert_eq!(Direction::Request.to_string(), "request");
    }
}
