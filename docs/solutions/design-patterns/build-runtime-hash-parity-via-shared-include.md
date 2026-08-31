---
title: "Guarantee build/runtime fingerprint parity with one shared declared-input inventory"
date: 2026-07-16
last_updated: 2026-08-26
category: design-patterns
module: "adapters/nautilus/lab (build.rs, fingerprint_core.rs, src/fingerprint.rs, tests/fingerprint_closure.rs) — the declared lab build-input fingerprint; certified inputs also include adapters/nautilus/src, adapters/nautilus/nautilus-ls-calendar, crates/ls-sdk, crates/ls-core, metadata/constraints"
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
  - adapter-workspace
  - coverage-oracle
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
- `adapters/nautilus/src/**` — the `nautilus-ls` adapter source, including the
  `src/bin/**` binaries the lab never links (the tree is declared whole so that
  adding or removing a member cannot slip past the digest);
- `adapters/nautilus/nautilus-ls-calendar/src/**` and its manifest;
- the standalone adapter workspace manifest, lockfile, and Rust toolchain file.
  `adapters/nautilus/Cargo.toml` carries both `[package] name = "nautilus-ls"`
  and the `[workspace]` table, so that single entry is also the adapter package
  manifest — a second entry for it would fail the duplicate-normalized-path
  check.

The shared core file is intentionally inside the inventory now. Hashing its source
does not create a self-reference: the digest hashes the implementation bytes, not
the digest output. The obsolete rule that it must remain outside the hash applied
only to the earlier `src/**` shortcut.

This prerequisite deliberately does **not** certify:

- root `Cargo.lock` (the governed build resolves through the standalone adapter
  workspace lockfile);
- generated `target/**`, dev-only dependency sources such as
  `crates/ls-sdk-test-support`, or ambient compiler flags and command-line
  toolchain overrides;
- the operator-local KRX calendar snapshot **state** under
  `adapters/nautilus/state`. The calendar package *source* is certified; its
  snapshot state is not. That state is gitignored, credential-refreshed, and
  carries its own `artifact_id` identity, so declaring it would give one artifact
  two competing identities and make an ordinary calendar ingest invalidate every
  governed binary.

The boundary is closed and carries no package-specific deferral: a repository-local
crate the lab *links into its binaries* is either declared here or reported by the
coverage oracle. Dev-only dependency sources are outside it by the same rule — they
cannot change a shipped binary. The morning shell preflight's omission of the root SDK/core
manifests was a distinct residual, closed separately by adding
`crates/ls-sdk/Cargo.toml` and `crates/ls-core/Cargo.toml` to that script's own
hand-listed `BIN_EXTRA_FRESHNESS_INPUTS`. The two boundaries stay separate on
purpose: this runtime fingerprint does not discharge the shell one, and the shell's
mtime axis does not read this digest.

## Operator Consequences

A closed boundary with no exclusion predicate has four consequences worth knowing
before they surprise someone mid-session:

- **A stray untracked file inside a declared tree moves the digest.** Tree hashing
  covers every regular file, by design — an ignore predicate would be exactly the
  second input list this pattern exists to forbid. The recovery is to delete the
  stray file, *and to rebuild as well* if any build ran while it was present: a
  declared tree is a rebuild watch, so a stray that existed at build time is
  embedded in the digest, and deleting it is itself a stale-binary refusal until
  the binary is rebuilt.
- **Editing any adapter binary under `adapters/nautilus/src/bin/**` invalidates
  every governed lab binary.** The lab links only the `nautilus_ls` lib target, but
  the whole tree is declared, so a release rebuild is required before the next
  governed turn. This includes the four calendar binaries
  `adapters/nautilus/scripts/session-morning.sh` runs as prebuilt paths.
- **The two freshness oracles therefore disagree on those edits.** A `src/bin/**`
  edit moves the declared digest but never reaches the lab binary's Cargo
  dependency evidence, so the morning preflight's mtime axis reports fresh while a
  governed turn refuses.
- **A new repository-local crate must be seeded into the shared test fixture as
  well as declared.** Validation fails closed on a missing declared input, so
  declaring a crate without adding it to `lab/tests/support/fingerprint_fixture.rs`
  reddens every fixture-based test with a message about an untrustworthy inventory
  rather than about the omission.

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

- Cargo's generated dependency evidence for every repository-local package the lab
  links — `ls-sdk`, `ls-core`, `nautilus-ls`, and `nautilus-ls-calendar`. Prefer the
  built binary's `.d` sidecar when present, but fall back to Cargo's versioned, typed
  `.fingerprint/**/dep-lib-*` records because artifact caches do not guarantee that
  convenience sidecar survives. Both paths are exercised against the real build
  directory and must yield the same closed boundary, so the closure does not depend on
  which evidence format happens to survive;
- `ls-core` build-script output for rebuild-watched embedded metadata;
- explicit checks for manifests, lockfile, and toolchain inputs those oracles do
  not own.

The dependency-evidence oracle requires every repository-local compiled input it
observes to be covered by a declared entry, with no package-specific exception.
Its only subtraction is generated build output under `adapters/nautilus/target`,
which has no source form to declare — build-script output such as `ls-core`'s
generated metadata legitimately appears in dependency evidence.

Two classes escape that oracle by construction, so each gets its own check:

- **A build script at a linked package root** never appears in that package's lib
  dep-info, which is why `crates/ls-core/build.rs` is declared explicitly. A test
  asserts that every linked package's `build.rs` either does not exist or is covered,
  so adding one cannot silently escape.
- **A fifth repository-local crate** would be skipped by the fallback decoder's
  per-package dispatch. That table is pinned to an independent source of truth — the
  adapter lockfile, where a repository-local package is a `[[package]]` block with no
  `source` key — and every local package must be classified as linked or explicitly
  unlinked, with its reason. A new crate reds that test until it is classified.

Ship permanent falsifiers: a synthetic undeclared `crates/new/src/lib.rs`, a
synthetic undeclared input planted *inside the adapter workspace* — the shape the
deleted per-package deferral used to hide — and a synthetic undeclared
build-script data path must each make the coverage checker report the gap. The
adapter-workspace falsifier is a falsifier of the *predicate*: it reds against a
per-package exemption and passes without one, which is what proves the deletion
rather than assuming it.

Those three hand the checker a fabricated evidence string, so a broken decoder is
never on their execution path. One more plants an undeclared input as a real Cargo
dep-info record and drives the whole degraded chain — fingerprint-directory matching,
dep-info decoding, package-root resolution, coverage subtraction — so the fallback is
proven to *report* a gap rather than merely to be clean against today's tree.

Mutation tests must also flip one declared class at a time and prove that the
digest moves, adapter source, calendar source, and the calendar manifest included.
The retained negative controls are root `Cargo.lock`, generated output under any
`target/` directory, dev-only dependency sources, and the KRX snapshot state.

## Why This Matters

Build/runtime parity is only meaningful over the real declared input boundary.
Sharing an algorithm prevents implementation drift; sharing a typed inventory
also prevents scope drift. Independent Cargo/build-script evidence then protects
the inventory itself from becoming a confident but incomplete list.

Apply this pattern whenever separately compiled build and runtime code must agree
on a repository-derived identity, especially when local path dependencies or
build-time embedded data can affect the resulting binary.
