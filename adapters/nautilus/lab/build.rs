//! Embed the declared lab build-input fingerprint and project its Cargo watches.

use sha2::{Digest, Sha256};

include!("fingerprint_core.rs");

fn main() {
    let lab_root = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set for build scripts"),
    );
    let repo_root = lab_root
        .ancestors()
        .nth(3)
        .expect("lab manifest is under adapters/nautilus/lab");

    let fingerprint = compute_declared_fingerprint(repo_root)
        .expect("hashing the declared lab build-input inventory");
    println!("cargo:rustc-env=LAB_SRC_FINGERPRINT={fingerprint}");

    for path in watch_paths_from_root(repo_root).expect("projecting fingerprint inventory watches")
    {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}
