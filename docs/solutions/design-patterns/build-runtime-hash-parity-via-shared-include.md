---
title: "Guarantee build/runtime fingerprint parity with one shared declared-input inventory"
date: 2026-07-16
last_updated: 2026-08-19
category: design-patterns
module: adapters/nautilus/lab (build.rs, fingerprint_core.rs, src/fingerprint.rs) — the declared lab build-input fingerprint
problem_type: design_pattern
component: development_workflow
severity: medium
applies_when:
  - "A binary must prove at runtime that it was built from the authoritative inputs it is currently using"
  - "A build script embeds a fingerprint and runtime code recomputes it"
  - "Path dependencies or build-script data can change the binary without changing the package's own source"
tags:
  - build-fingerprint
  - stale-binary
  - build-script
  - include-macro
  - hash-parity
  - input-inventory
  - strategy-loop
---

## Context

The lab's governed strategy turn must refuse a stale binary. The original guard
hashed `lab/src/**` plus `lab/Cargo.toml` in shared build/runtime code. Sharing the
walk-and-hash algorithm prevented byte-level drift, but it did not make the input
boundary complete. The lab also compiles root path dependencies `crates/ls-sdk`
and `crates/ls-core`; `ls-core/build.rs` embeds root metadata. Those inputs could
change the binary while the old digest stayed equal, producing a false green.

There were two related lifecycle gaps. Cargo rebuild watches duplicated the hash
input list, and the governed parent reused its pre-build tree digest after a
foreground build. A correct hash algorithm could therefore coexist with stale
embedding or with an unapproved mutation during the parent/build/child handoff.

## Guidance

Put the inventory, validation, deterministic traversal, hashing, and Cargo watch
projection in one dependency-light source file and `include!` it from both the
build script and runtime module. Sharing only the hash loop is insufficient; every
consumer must derive from the same typed entries.

Each entry carries:

- a stable logical label;
- a repository-relative path;
- a node kind (`File` or `Tree`).

The digest frames the inventory version, entry count, label, node kind, logical
path, tree member kind/path, and file bytes with explicit lengths. Entries and
tree members are sorted before hashing. Absolute checkout paths never enter the
digest, so relocating an identical repository does not change the result.

Validation is fail-closed. Reject missing or unreadable inputs, file/tree type
mismatches, symlinks, special nodes, duplicate labels, duplicate normalized paths,
and overlapping file/tree declarations. Do not skip an input and continue with a
partial certification boundary.

Cargo watches are a projection of the validated inventory:

```rust
let fingerprint = compute_declared_fingerprint(repo_root)?;
println!("cargo:rustc-env=LAB_SRC_FINGERPRINT={fingerprint}");
for path in watch_paths_from_root(repo_root)? {
    println!("cargo:rerun-if-changed={}", path.display());
}
```

Production runtime recomputation resolves the repository from the compiled
`CARGO_MANIFEST_DIR`. Do not accept environment variables that redirect the trust
root. Tests inject a complete temporary repository root directly into the shared
library seam.

## Certified Boundary

`LAB_SRC_FINGERPRINT` remains the compatibility name, but it now means the
declared lab build-input fingerprint. It covers:

- `adapters/nautilus/lab/src/**`, `Cargo.toml`, `build.rs`, and
  `fingerprint_core.rs`;
- root `Cargo.toml`;
- `crates/ls-sdk/src/**` and its manifest;
- `crates/ls-core/src/**`, its manifest, and its build script;
- `metadata/error-catalog.yaml` and `metadata/constraints/**`;
- the standalone adapter workspace manifest, lockfile, and Rust toolchain file.

The shared core file is intentionally inside the inventory now. Hashing its source
does not create a self-reference: the digest hashes the implementation bytes, not
the digest output. The obsolete rule that it must remain outside the hash applied
only to the earlier `src/**` shortcut.

This prerequisite deliberately does **not** certify:

- `adapters/nautilus/src/**`;
- `adapters/nautilus/nautilus-ls-calendar/src/**` or the calendar manifest;
- root `Cargo.lock` (the governed build resolves through the standalone adapter
  workspace lockfile);
- generated `target/**`, dev-only dependencies, or ambient compiler flags and
  command-line toolchain overrides.

The adapter/calendar extension remains separate work. The morning shell
preflight's omission of the root SDK/core manifests is also a distinct residual;
this runtime fingerprint does not silently discharge that shell boundary.

## Governed Freshness Protocol

One pre-diagnosis digest authorizes the turn:

1. Require `current == parent embedded` and pin that digest as `approved`.
2. Run diagnosis and the foreground build from `adapters/nautilus`.
3. Recompute and require `post-build == approved` before invoking a reporter.
4. Require the fresh reporter's embedded digest to equal `approved`.
5. Pass `approved` into the separate decider.
6. As the decider's first action, require
   `approved == decider embedded == current` before configuration, stage logging,
   runtime creation, trials, or `turn`.

This closes mutations observable at each validation boundary. It does not claim
atomicity after the decider's final read; a repository-wide writer lock would be
required to remove that final TOCTOU residual.

## Coverage Proof

Do not trust the inventory merely because its own tests are green. Compare it to
independent evidence:

- Cargo's generated dependency evidence for repository-local `ls-sdk`/`ls-core`
  source inputs. Prefer the built binary's `.d` sidecar when present, but fall
  back to Cargo's versioned, typed `.fingerprint/**/dep-lib-*` records because
  artifact caches do not guarantee that convenience sidecar survives;
- `ls-core` build-script output for rebuild-watched embedded metadata;
- explicit checks for manifests, lockfile, and toolchain inputs those oracles do
  not own.

Ship permanent falsifiers: a synthetic undeclared `crates/new/src/lib.rs` and a
synthetic undeclared build-script data path must make the coverage checker report
the gap. Mutation tests must also flip one declared class at a time and prove that
the digest moves, while adapter/calendar/root-lock/generated-output mutations stay
equal as negative controls.

## Why This Matters

Build/runtime parity is only meaningful over the real declared input boundary.
Sharing an algorithm prevents implementation drift; sharing a typed inventory
also prevents scope drift. Independent Cargo/build-script evidence then protects
the inventory itself from becoming a confident but incomplete list.

Apply this pattern whenever separately compiled build and runtime code must agree
on a repository-derived identity, especially when local path dependencies or
build-time embedded data can affect the resulting binary.
