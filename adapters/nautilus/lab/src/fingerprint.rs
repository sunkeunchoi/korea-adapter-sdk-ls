//! Runtime access to the declared lab build-input fingerprint.
//!
//! [`EMBEDDED`] carries the historical environment name `LAB_SRC_FINGERPRINT`; its
//! value certifies exactly the inventory [`declared_inventory`] returns. That function
//! is where the certified boundary is defined and documented — this module deliberately
//! does not restate it, so the two cannot drift apart.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

include!("../fingerprint_core.rs");

/// The declared lab build-input fingerprint embedded by `build.rs`.
pub const EMBEDDED: &str = env!("LAB_SRC_FINGERPRINT");

/// Recompute the production fingerprint from the repository containing this
/// compiled crate. No process environment variable can redirect this trust root.
pub fn recompute() -> std::io::Result<String> {
    recompute_from_root(&compiled_repo_root())
}

/// Recompute against an explicit complete repository fixture.
///
/// Production callers use [`recompute`]. This seam exists for unit and
/// process-boundary tests that need to mutate an isolated declared closure.
pub fn recompute_from_root(repo_root: &Path) -> std::io::Result<String> {
    compute_declared_fingerprint(repo_root)
}

/// Return the fixed repository root derived from the compiled lab manifest.
pub fn compiled_repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("lab manifest is under adapters/nautilus/lab")
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_is_sha256_hex() {
        assert_eq!(EMBEDDED.len(), 64, "SHA-256 hex is 64 chars: {EMBEDDED}");
        assert!(EMBEDDED
            .chars()
            .all(|character| character.is_ascii_hexdigit()));
    }

    #[test]
    fn current_declared_inputs_equal_the_embedded_value() {
        assert_eq!(recompute().unwrap(), EMBEDDED);
    }
}
