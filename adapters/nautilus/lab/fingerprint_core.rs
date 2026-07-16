// Shared walk-and-hash core for the lab-source fingerprint (KTD5).
//
// This file is `include!`d verbatim by BOTH `build.rs` (which embeds the
// fingerprint of the source tree at compile time) and `src/fingerprint.rs`
// (which recomputes it at run time so the orchestrator can require a freshly
// built binary to report the matching value). Sharing the literal source means
// the two implementations cannot drift — a stale binary that backtested old
// code is structurally impossible to mistake for a fresh one.
//
// The including scope must bring `sha2::{Digest, Sha256}` into scope before the
// `include!`. This file references only `std` and those two `sha2` items — no
// crate-internal types — so it is include-safe from a build script (which
// cannot see the crate's own compiled library).

/// Compute a stable hex fingerprint over every file under `src_dir` (recursively,
/// sorted by relative path) plus `cargo_toml`. Each file contributes its
/// relative path and its bytes, NUL-separated, so a rename or a one-byte content
/// change both move the digest. The walk is deterministic (sorted) so the same
/// tree always hashes identically regardless of directory-read order.
fn compute_lab_fingerprint(
    src_dir: &std::path::Path,
    cargo_toml: &std::path::Path,
) -> std::io::Result<String> {
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    collect_files_sorted(src_dir, &mut files)?;
    files.sort();

    let mut hasher = Sha256::new();
    for f in &files {
        let rel = f.strip_prefix(src_dir).unwrap_or(f);
        hasher.update(rel.to_string_lossy().as_bytes());
        hasher.update([0u8]);
        hasher.update(std::fs::read(f)?);
        hasher.update([0u8]);
    }
    // Cargo.toml is a hash input too — a dependency or feature edit is a source
    // change even when no `src/` byte moves.
    hasher.update(b"Cargo.toml");
    hasher.update([0u8]);
    hasher.update(std::fs::read(cargo_toml)?);
    hasher.update([0u8]);

    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        hex.push_str(&format!("{b:02x}"));
    }
    Ok(hex)
}

/// Recursively collect every regular file under `dir` into `out`.
fn collect_files_sorted(
    dir: &std::path::Path,
    out: &mut Vec<std::path::PathBuf>,
) -> std::io::Result<()> {
    let mut entries: Vec<std::path::PathBuf> =
        std::fs::read_dir(dir)?.map(|e| e.map(|e| e.path())).collect::<Result<_, _>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_files_sorted(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}
