---
title: "Guarantee build-time vs run-time hash parity by sharing the walk-and-hash source via include!"
date: 2026-07-16
category: design-patterns
module: adapters/nautilus/lab (build.rs, fingerprint_core.rs, src/fingerprint.rs) — the lab-source build fingerprint
problem_type: design_pattern
component: development_workflow
severity: medium
applies_when:
  - "A binary must prove at run time that it was built from the source tree it is currently looking at (anti-stale-binary check)"
  - "You compute a fingerprint/hash of source (or any inputs) in build.rs AND recompute the same fingerprint at run time to compare them"
  - "Two hashes that must be byte-identical are produced by two separately-compiled code paths (a build script vs the crate's own library)"
tags:
  - build-fingerprint
  - stale-binary
  - build-script
  - include-macro
  - hash-parity
  - strategy-loop
---

## Context

The lab's governed strategy turn must refuse to run a **stale binary** — one whose
compiled code no longer matches the source tree the operator is editing. Past
staleness bit the strategy loop twice (2026-07-12, 2026-07-15): a background build
from the wrong directory left an old binary that silently backtested old strategy
code, and the existing `strategy_code_hash` covered only `orb.rs`, so a change in
*params* code slipped through.

The fix is a fingerprint over the **whole** `lab/src/**` tree (plus `Cargo.toml`):
`build.rs` embeds it as an env constant at compile time, and the running binary
recomputes the same walk-and-hash from the source dir at run time; the orchestrator
requires the two to match before any backtest. This only works if the two hashes
are **guaranteed identical for an unchanged tree** — and they are produced by two
*separately compiled* code paths (the build script and the crate library), which is
exactly where a subtle drift (different separator, sort order, or newline handling)
would reintroduce the stale-binary class it is meant to close.

## Guidance

**Put the walk-and-hash logic in ONE standalone source file and `include!` it into
both the build script and the runtime module.** Do not re-implement it in each —
share the literal source so the two implementations *cannot* drift.

- Place the shared file at the crate root (e.g. `fingerprint_core.rs`), NOT under
  `src/`, so it is not itself part of the hashed tree (avoids a self-reference
  paradox and keeps the hash input set stable).
- The shared file references only `std` + one hashing crate (e.g. `sha2`), and
  assumes the including scope brought those names into scope *before* the
  `include!`. It must not reference any crate-internal type — a build script cannot
  see the crate's own compiled library.
- Add `sha2` (or the hashing crate) to BOTH `[build-dependencies]` and
  `[dependencies]`.
- In `build.rs`, resolve the tree root from `CARGO_MANIFEST_DIR` (build-script cwd
  is not guaranteed), emit `cargo:rustc-env=<NAME>=<hex>`, and add
  `cargo:rerun-if-changed=` lines covering every hash input (`src`, `Cargo.toml`,
  the shared file, `build.rs` itself) so a stale embedded value can never survive
  an edit.
- Expose the embedded value at run time as `pub const EMBEDDED: &str =
  env!("<NAME>");` and a `recompute_from_dir(src, cargo_toml)` that calls the same
  shared function. Tests resolve the crate's own tree via
  `env!("CARGO_MANIFEST_DIR")`.

## Why This Matters

Any anti-stale mechanism whose two hashes are computed by non-shared code is only
as trustworthy as the hand-maintained agreement between them. Share the source and
parity is **structural**, not conventional: the same walk order, the same NUL
separators, the same byte reads, by construction. A one-byte change in any hashed
file moves both hashes together; an unchanged tree yields identical hashes on every
platform run. This makes "which binary did I actually build?" a decidable question
— the fingerprint equality is what matters, so path confusion, `CARGO_TARGET_DIR`
redirection, or a leftover binary can only cause a spurious *halt* (false-stale),
never a false *green*.

Note the intended two-build cost for a code turn: the orchestrator's own parent
self-check halts as stale until the operator rebuilds the parent, and it then does
a second foreground build for the fresh child it actually runs the flip in. That is
by design, not the gate misfiring.

## When to Apply

Any time a value must be identical across the build/run-time boundary and is
otherwise produced by two separately-compiled code paths — build fingerprints,
embedded schema hashes, generated-vs-checked-in parity guards. Reach for a shared
`include!`d core rather than trusting two hand-kept implementations to stay in
lockstep. Do NOT put the shared file inside the directory it hashes.

## Examples

Layout (`adapters/nautilus/lab/`):

```
fingerprint_core.rs      # shared: fn compute_lab_fingerprint(src_dir, cargo_toml) -> io::Result<String>
build.rs                 # use sha2::{Digest, Sha256}; include!("fingerprint_core.rs");
src/fingerprint.rs        # use sha2::{Digest, Sha256}; include!("../fingerprint_core.rs");
```

`build.rs`:

```rust
use sha2::{Digest, Sha256};
include!("fingerprint_core.rs");

fn main() {
    let root = std::env::var("CARGO_MANIFEST_DIR").expect("set for build scripts");
    let root = std::path::Path::new(&root);
    let fp = compute_lab_fingerprint(&root.join("src"), &root.join("Cargo.toml")).unwrap();
    println!("cargo:rustc-env=LAB_SRC_FINGERPRINT={fp}");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=fingerprint_core.rs");
    println!("cargo:rerun-if-changed=build.rs");
}
```

`src/fingerprint.rs`:

```rust
use sha2::{Digest, Sha256};
include!("../fingerprint_core.rs");

pub const EMBEDDED: &str = env!("LAB_SRC_FINGERPRINT");

pub fn recompute_from_dir(src_dir: &Path, cargo_toml: &Path) -> std::io::Result<String> {
    compute_lab_fingerprint(src_dir, cargo_toml)
}
```

The parity test that would catch any drift:

```rust
#[test]
fn recompute_from_the_current_tree_equals_embedded() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let live = recompute_from_dir(&root.join("src"), &root.join("Cargo.toml")).unwrap();
    assert_eq!(live, EMBEDDED); // holds only because both call the SAME include!d fn
}
```
