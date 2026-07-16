//! The lab-source build fingerprint (KTD5, U1) — runtime side.
//!
//! [`EMBEDDED`] is the digest `build.rs` computed over `src/**` + `Cargo.toml`
//! at compile time. [`recompute_from_dir`] runs the *identical* walk-and-hash at
//! run time (the two share [`compute_lab_fingerprint`] verbatim via `include!`,
//! so they cannot drift). The orchestrator (U7) requires a freshly built binary
//! to report an `EMBEDDED` that matches the recomputed tree hash before any
//! backtest runs — a stale binary that still carries an old tree's digest halts
//! instead of silently backtesting old code.
//!
//! This covers the *full* lab source, closing the `strategy_code_hash`-only gap
//! (that hash fingerprints `orb.rs` alone — past staleness surfaced through
//! params code, not the strategy file).

use std::path::Path;

use sha2::{Digest, Sha256};

include!("../fingerprint_core.rs");

/// The fingerprint `build.rs` embedded at compile time (hex SHA-256 over the lab
/// `src/**` tree plus `Cargo.toml`).
pub const EMBEDDED: &str = env!("LAB_SRC_FINGERPRINT");

/// Recompute the lab-source fingerprint from a source directory + its
/// `Cargo.toml` at run time. Equals [`EMBEDDED`] for the tree the running binary
/// was built from; a mismatch means the binary is stale relative to `src_dir`.
///
/// # Errors
///
/// If any file under `src_dir` (or `cargo_toml`) cannot be read.
pub fn recompute_from_dir(src_dir: &Path, cargo_toml: &Path) -> std::io::Result<String> {
    compute_lab_fingerprint(src_dir, cargo_toml)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The crate's own source tree (resolved at test-compile time).
    fn crate_src_and_toml() -> (std::path::PathBuf, std::path::PathBuf) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        (root.join("src"), root.join("Cargo.toml"))
    }

    #[test]
    fn embedded_is_64_hex() {
        assert_eq!(EMBEDDED.len(), 64, "SHA-256 hex is 64 chars: {EMBEDDED}");
        assert!(EMBEDDED.chars().all(|c| c.is_ascii_hexdigit()), "{EMBEDDED}");
    }

    #[test]
    fn recompute_from_the_current_tree_equals_embedded() {
        let (src, toml) = crate_src_and_toml();
        let live = recompute_from_dir(&src, &toml).unwrap();
        assert_eq!(live, EMBEDDED, "recompute of the current tree matches the embedded value");
    }

    #[test]
    fn a_one_byte_change_in_any_src_file_moves_the_fingerprint() {
        let (src, toml) = crate_src_and_toml();
        let base = recompute_from_dir(&src, &toml).unwrap();

        // Copy the tree into a tempdir, flip one byte in a src file, recompute.
        let tmp = tempfile::TempDir::new().unwrap();
        let tmp_src = tmp.path().join("src");
        copy_tree(&src, &tmp_src).unwrap();
        std::fs::copy(&toml, tmp.path().join("Cargo.toml")).unwrap();

        let target = tmp_src.join("lib.rs");
        let mut bytes = std::fs::read(&target).unwrap();
        bytes.push(b'\n'); // one appended byte
        std::fs::write(&target, &bytes).unwrap();

        let mutated = recompute_from_dir(&tmp_src, &tmp.path().join("Cargo.toml")).unwrap();
        assert_ne!(mutated, base, "a one-byte src edit changes the fingerprint");
    }

    fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            let dest = to.join(entry.file_name());
            if entry.path().is_dir() {
                copy_tree(&entry.path(), &dest)?;
            } else {
                std::fs::copy(entry.path(), &dest)?;
            }
        }
        Ok(())
    }
}
