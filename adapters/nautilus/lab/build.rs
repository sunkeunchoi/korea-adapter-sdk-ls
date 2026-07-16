//! Build script: embed a fingerprint over the full lab source tree (KTD5, U1).
//!
//! Walks `src/**` plus `Cargo.toml` at compile time and emits the digest as the
//! `LAB_SRC_FINGERPRINT` env constant. The runtime module (`src/fingerprint.rs`)
//! recomputes the identical walk-and-hash from the shared `fingerprint_core.rs`,
//! so a freshly built binary can prove it embeds the tree it was built from —
//! deleting the stale-binary gotcha (past staleness silently backtested old
//! code) rather than reporting it.

use sha2::{Digest, Sha256};

include!("fingerprint_core.rs");

fn main() {
    // Build scripts run with cwd set to the package root, but pin it explicitly
    // from CARGO_MANIFEST_DIR so the walk is unambiguous.
    let root = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set for build scripts");
    let root = std::path::Path::new(&root);
    let src_dir = root.join("src");
    let cargo_toml = root.join("Cargo.toml");

    let fingerprint = compute_lab_fingerprint(&src_dir, &cargo_toml)
        .expect("hashing the lab source tree for the build fingerprint");
    println!("cargo:rustc-env=LAB_SRC_FINGERPRINT={fingerprint}");

    // Rerun whenever any hash input changes, so a stale embedded value can never
    // survive an edit.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=fingerprint_core.rs");
    println!("cargo:rerun-if-changed=build.rs");
}
