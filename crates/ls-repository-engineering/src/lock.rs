//! Deterministic exact-lock construction with separated normative identity.

use crate::identity::{normalize_normative, package_lock_id, IdentityError};
use crate::schema::{BuildProvenance, ExactLock, NormativeLockClosure, SchemaVersion};

pub fn build_lock(
    normative: NormativeLockClosure,
    mut build_provenance: BuildProvenance,
) -> Result<ExactLock, IdentityError> {
    let normative = normalize_normative(&normative);
    let package_lock_id = package_lock_id(&normative)?;
    build_provenance
        .workflow_pins
        .sort_by(|left, right| left.path.cmp(&right.path));
    Ok(ExactLock {
        schema_version: SchemaVersion::V0,
        package_lock_id,
        normative,
        build_provenance,
    })
}

pub fn lock_bytes(lock: &ExactLock) -> Result<Vec<u8>, serde_json::Error> {
    let mut bytes = serde_json::to_vec_pretty(lock)?;
    bytes.push(b'\n');
    Ok(bytes)
}
