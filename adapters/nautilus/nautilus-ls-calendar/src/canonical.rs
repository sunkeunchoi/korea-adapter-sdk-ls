//! Deterministic identities + SemVer schema compatibility (U2, KTD4).
//!
//! Two identities, both SHA-256 hex digests of a *canonical* JSON projection
//! (object keys recursively sorted, arrays in a deterministic order, numbers/strings
//! in their normalized serde form):
//!
//! - [`compute_artifact_id`] hashes the *entire* snapshot content **excluding the two
//!   identity fields themselves** (`artifact_id`, `calendar_id`). Any content change —
//!   including retrieval mechanics like fetch timestamps or source-availability bounds —
//!   moves it.
//! - [`compute_calendar_id`] hashes only the **effective statuses + decisive claim
//!   identities**: the per-date `(date, status, decisive-evidence ids)` triples plus the
//!   `(id, kind, valid)` identity of each decisive evidence claim. It **excludes retrieval
//!   mechanics** (recorded-at / freshness timestamps, source-availability bounds, source
//!   labels), so only an effective calendar/proof change moves it.
//!
//! The SHA-256 shape mirrors `lab`'s `artifacts::manifest::hash_bytes`, implemented
//! locally to keep this a pure leaf crate (KTD1/KTD4).

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::schema::Snapshot;

/// The SemVer schema-compatibility version this crate emits and understands. Snapshots
/// carry their own [`Snapshot::schema_version`]; the loader gates it with
/// [`schema_is_compatible`].
pub const SCHEMA_VERSION: &str = "1.0.0";

/// `true` iff a snapshot declaring `declared` (a `major.minor.patch` SemVer string) is
/// compatible with this crate's [`SCHEMA_VERSION`]. Compatibility = **same MAJOR**; any
/// other major (higher or lower) is unsupported, as is a malformed version string.
pub fn schema_is_compatible(declared: &str) -> bool {
    match (parse_major(declared), parse_major(SCHEMA_VERSION)) {
        (Some(d), Some(s)) => d == s,
        _ => false,
    }
}

/// Parse the MAJOR component of a strict `major.minor.patch` SemVer string. Returns
/// `None` unless there are exactly three numeric dot-separated components (a tiny
/// hand parser — the leaf crate takes no `semver` dependency).
fn parse_major(version: &str) -> Option<u64> {
    let mut parts = version.split('.');
    let major = parts.next()?;
    let minor = parts.next()?;
    let patch = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    // All three components must be valid u64; only MAJOR is returned.
    let major: u64 = major.parse().ok()?;
    minor.parse::<u64>().ok()?;
    patch.parse::<u64>().ok()?;
    Some(major)
}

/// Compute the deterministic `artifact_id`: SHA-256 (hex) of the canonicalized snapshot
/// content **excluding the `artifact_id` / `calendar_id` identity fields**. Any content
/// change moves it (KTD4).
pub fn compute_artifact_id(snapshot: &Snapshot) -> String {
    let mut value = serde_json::to_value(snapshot).expect("snapshot serializes to JSON");
    if let Some(obj) = value.as_object_mut() {
        obj.remove("artifact_id");
        obj.remove("calendar_id");
    }
    hash_canonical(&value)
}

/// Compute the deterministic `calendar_id`: SHA-256 (hex) of only the **effective
/// statuses + decisive claim identities**, excluding retrieval mechanics (KTD4).
///
/// The projection is:
/// - `rows`: for each row, `(date, status, sorted decisive-evidence ids)`, sorted by
///   `(date, status)` so row ordering in the snapshot never perturbs the id.
/// - `decisive_claims`: for each evidence record referenced as decisive by any row, its
///   `(id, kind, valid)` identity, sorted by id and de-duplicated.
///
/// Nothing else — no `recorded_at`, no `source_availability`, no `freshness`, no source
/// labels — feeds this hash.
pub fn compute_calendar_id(snapshot: &Snapshot) -> String {
    // Effective per-date facts.
    let mut rows: Vec<Value> = snapshot
        .rows
        .iter()
        .map(|row| {
            let mut decisive = row.decisive_evidence.clone();
            decisive.sort();
            Value::Array(vec![
                Value::String(row.date.to_string()),
                serde_json::to_value(row.status).expect("day status serializes"),
                Value::Array(decisive.into_iter().map(Value::String).collect()),
            ])
        })
        .collect();
    rows.sort_by(|a, b| canonical_string(a).cmp(&canonical_string(b)));

    // The identities of the evidence claims that actually decide a status.
    let mut decisive_ids: Vec<&String> = snapshot
        .rows
        .iter()
        .flat_map(|row| row.decisive_evidence.iter())
        .collect();
    decisive_ids.sort();
    decisive_ids.dedup();

    let mut claims: Vec<Value> = decisive_ids
        .iter()
        .filter_map(|id| snapshot.evidence.iter().find(|ev| &ev.id == *id))
        .map(|ev| {
            Value::Array(vec![
                Value::String(ev.id.clone()),
                serde_json::to_value(ev.kind).expect("evidence kind serializes"),
                Value::Bool(ev.valid),
            ])
        })
        .collect();
    claims.sort_by(|a, b| canonical_string(a).cmp(&canonical_string(b)));

    let projection = Value::Array(vec![Value::Array(rows), Value::Array(claims)]);
    hash_canonical(&projection)
}

/// SHA-256 (hex) of the canonical serialization of `value`.
fn hash_canonical(value: &Value) -> String {
    let canonical = canonical_string(value);
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    hex(&hasher.finalize())
}

/// A canonical, stable string encoding of a JSON value: object keys are recursively
/// sorted, so the encoding never depends on `serde_json` map ordering or feature flags.
/// Arrays keep their order (callers sort meaning-bearing arrays before encoding).
fn canonical_string(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        // `Number`'s Display is its normalized JSON form.
        Value::Number(n) => out.push_str(&n.to_string()),
        // Reuse serde's string escaping for a valid, stable JSON string literal.
        Value::String(s) => out.push_str(&serde_json::to_string(s).expect("string encodes")),
        Value::Array(arr) => {
            out.push('[');
            for (i, v) in arr.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(v, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).expect("key encodes"));
                out.push(':');
                write_canonical(&map[*key], out);
            }
            out.push('}');
        }
    }
}

/// Lowercase hex encoding (matches `lab`'s manifest hash shape).
fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
