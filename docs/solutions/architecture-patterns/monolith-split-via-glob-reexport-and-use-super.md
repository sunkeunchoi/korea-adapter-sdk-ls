---
title: "Decomposing a per-TR monolith into per-domain submodules via `pub use submod::*` + `use super::*`, with a body-multiset preservation proof"
date: 2026-06-30
category: architecture-patterns
module: crates/ls-sdk, crates/ls-core
problem_type: architecture_pattern
component: tooling
severity: medium
applies_when:
  - "Splitting a large per-TR/per-item monolith file (market_session/mod.rs, realtime/frame.rs, account/mod.rs, endpoint_policy.rs) into per-domain sibling modules"
  - "Needing a refactor to keep every public path + the docgen count tests + the policy crosscheck lists byte-identical with ZERO test edits"
  - "Verifying that a pure-relocation refactor changed no behavior, without eyeballing tens of thousands of moved lines"
  - "Wondering why moved struct clusters compile in a new submodule without re-adding `use serde::{...}` / `use ls_core::{...}`"
tags:
  - ls-sdk
  - ls-core
  - refactor
  - module-decomposition
  - re-export
  - rust
---

# Decomposing a per-TR monolith into per-domain submodules

## Context

`crates/ls-sdk/src/market_session/mod.rs` (11,789 lines / 474 structs),
`realtime/frame.rs` (4,977 / 102), `account/mod.rs` (2,406 / 73), and
`crates/ls-core/src/endpoint_policy.rs` (5,077 lines / 297 `_POLICY` consts) are the
files the active TR add/flip loop edits on nearly every feature PR. Their size makes
navigation, review diffs, and per-TR onboarding heavier than they need to be. The goal
was to split each into per-domain sibling modules **without changing any behavior or
public API** — so the docgen count tests (`TRACKED_TRS=307`, `reference.len=279`,
`maintained_tr_count=307`, `banner_trs`) and both policy crosscheck lists stay green
with **no edits**. Plan `docs/plans/2026-06-29-003-...-split-plan.md`, baseline note
`docs/plans/notes/2026-06-29-003-refactor-baseline.md`.

## Guidance

Move item clusters to per-domain sibling files and bridge them back with two
one-line idioms. The split is then a **pure relocation** — verifiable mechanically.

1. **In the original file (now the module root), declare + re-export each submodule:**
   ```rust
   // market_session/mod.rs keeps ONLY: module docs + the module-level `use` block +
   // the facade struct & `impl MarketSession` + these declarations.
   mod quote;        pub use quote::*;
   mod investor_flow; pub use investor_flow::*;
   // ...one pair per family
   ```
   The `pub use <submod>::*;` is load-bearing: it re-exports every moved item into the
   original module path, so `ls_sdk::market_session::T1102Request` (and every other
   path) still resolves. Callers, the typed regression tests, and the docgen count
   tests (which source from `metadata/`, **not** file layout) are untouched.

2. **Start every new submodule with `use super::*;`** — and add nothing else:
   ```rust
   //! Domestic equity current-price / order-book quote reads.
   use super::*;

   pub struct T1102InBlock { /* moved verbatim */ }
   // ...
   ```
   This is the non-obvious enabler (see Why This Matters): `use super::*` pulls the
   parent module's **private `use` imports** (`serde::{Deserialize, Serialize}`,
   `ls_core::{Inner, LsResult}`, `EndpointPolicy`, `Protocol`, `RateLimitCategory`,
   …) into the child, so the moved structs/consts compile with **no per-file import
   edits**. Serde attributes that use a string path (`#[serde(with =
   "ls_core::string_or_number")]`) need nothing — the path is resolved by the macro.

3. **What stays put** (do not glob-split these):
   - The facade `impl` (`impl MarketSession`, `impl Account`) and its struct.
   - Shared helpers + anything **named**-re-exported elsewhere. `realtime/mod.rs` does
     `pub use frame::{ composite_key, S3Trade, …~108 names… };` (explicit, not glob),
     so `composite_key`/`WsLane`/`build_frame` stay in `frame.rs`; `frame.rs` then does
     `pub use ws_events::*;` etc. so the explicit name list resolves **transitively**.
   - The in-source `#[cfg(test)] mod tests { use super::*; }` block (its `use super::*`
     sees moved items via the glob re-export) and any `_POLICY` crosscheck test body
     that resolves consts by bare name.
   - Item-specific attachments travel **with their cluster**: manual `impl
     std::fmt::Debug for …OutBlock1` redaction impls, `impl Default`, and
     `ls_core::impl_has_pagination!(CSPAQ12200Request)` move into the same domain file
     as their struct (no cross-file bare-path reference is created).

4. **Directory idiom:** a file-module `foo.rs` houses its submodules in a sibling
   `foo/` directory (`realtime/frame.rs` + `realtime/frame/ws_events.rs`); a module
   with many submodules can also become `foo/mod.rs` (`endpoint_policy.rs` →
   `endpoint_policy/mod.rs` + `endpoint_policy/<domain>.rs`). Both are idiomatic.

5. **Prove preservation mechanically** instead of reading the diff. A pure relocation
   means the **multiset of top-level item bodies is identical before and after**.
   Extract every `pub struct` / `pub const` / `impl` / `enum` / macro-invocation body
   (brace-depth split), normalize, and diff the multiset of bodies old-vs-new:
   ```
   old item-bodies: 604   new item-bodies: 604
   missing (in old, not in new): 0
   extra   (in new, not in old): 0
   ```
   0/0 with byte-identical bodies proves nothing was dropped, renamed, altered, or
   added — stronger and faster than eyeballing ~21k moved lines.

## Why This Matters

The whole refactor's safety rests on one Rust fact that is easy to doubt:
**`use super::*` glob-imports the parent module's private `use`-imported names, not
just its `pub` items.** A child module can see its ancestors' private items, and a
glob `use super::*` brings those names (including `use serde::Serialize;`) into the
child's scope. That is *why* a moved struct cluster compiles in a new file with only
`use super::*;` at the top and no other imports. Without this, every one of the ~650
moved structs would need its own import block, and the "pure move" would become a
risky rewrite.

The second reason it matters: the gate's count tests key on **public item paths +
metadata**, never on file location (docgen renders from `metadata/`; no test
enumerates structs by file). So `pub use submod::*` — which preserves every path —
makes the entire decomposition invisible to the count tests and crosscheck lists. They
pass **unchanged, with zero edits**. This is what lets a 24k-line structural refactor
land without touching a single assertion.

Mis-filing an item into the "wrong" family is **silent** (it still compiles and the
gate stays green via the glob re-export), so family classification is a
review-quality concern, not a correctness one — capture the item→family map in a
committed baseline note for review rather than trying to enforce it.

## When to Apply

- Any monolith of independent per-item definitions (structs, consts) that share a
  module-level `use` block and a facade — the ideal pure-move shape.
- When the hard requirement is "no public-API/behavior/metadata change" and the
  regression net is an existing typed test suite (a dropped re-export fails `cargo
  test` at **compile** time, which is the verification — no new snapshot needed).
- Run it in a confirmed lull (no in-flight feature PR on the same file) and land each
  monolith as one atomic, gate-green commit to minimize conflict against the active
  loop.

## Examples

**Before** (`market_session/mod.rs`, 11,789 lines):
```rust
use serde::{Deserialize, Serialize};
use ls_core::{Inner, LsResult};

pub struct T1102InBlock { /* ... */ }
pub struct T1102Request { /* ... */ }
impl T1102Request { /* ... */ }
// ...474 struct clusters...
impl MarketSession { /* facade, ~1095 lines */ }
```

**After** — `mod.rs` (1,157 lines: docs + use + facade + re-exports):
```rust
use serde::{Deserialize, Serialize};
use ls_core::{Inner, LsResult};

mod quote; pub use quote::*;
mod investor_flow; pub use investor_flow::*;
// ...9 families...
pub struct MarketSession { inner: Arc<Inner> }
impl MarketSession { /* facade stays here */ }
```
`market_session/quote.rs`:
```rust
//! Domestic equity current-price / order-book / multi-symbol quote reads.
use super::*;                      // brings Serialize/Deserialize/Inner/LsResult

pub struct T1102InBlock { /* moved verbatim */ }
pub struct T1102Request { /* moved verbatim */ }
impl T1102Request { /* moved verbatim */ }
```

Result: 11,789 → 1,157 (mod.rs) + 9 cohesive family files; `cargo test` 1,182 passed
(identical to baseline), count tests + crosscheck lists unchanged with zero edits,
`make docs-check` clean. Same recipe applied to `frame.rs` (→1,471 + 4 files),
`account/mod.rs` (→316 + 3), and `endpoint_policy.rs` (→`mod.rs` 396 + 6 domain files).

**Counter-example — what NOT to consolidate while you're in there:** a `#[serde(with =
"ls_core::wire_str")]` module that bundles serialize+deserialize was prototyped to
shrink the repeated `deserialize_with = "ls_core::string_or_number"` attributes. It
passed tests and cut attribute-string chars 35%, but cut **zero lines** (still one
attr per field) and **hid the IGW40011 wire direction** that
`serialize_with`/`deserialize_with` document at every field. It was skipped — see the
baseline note's "U6 (Wave 4)" verdict. Decompose structure; leave the direction-naming
serde attributes explicit.
