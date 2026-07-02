---
title: "Decomposing a Rust integration-test monolith into per-family submodules (the test-tree analog of the source split — distinct mechanics)"
date: 2026-06-30
category: architecture-patterns
module: crates/ls-sdk/tests
problem_type: architecture_pattern
component: tooling
severity: medium
applies_when:
  - "Splitting a large integration-test file under crates/ls-sdk/tests/ (market_session_tests.rs, live_smoke.rs, paginated_tests.rs, order_smoke.rs, account_tests.rs) into per-family sibling submodules"
  - "Wondering why a bare `mod foo;` in a tests/*.rs file fails to compile or silently creates a second test binary"
  - "Needing to prove a test-file move dropped/renamed/edited no test, when the compiler gives NO safety net for a forgotten module"
  - "Reconciling that the `cargo test --list` names changed (gained a `family::` prefix) even though the move was pure"
  - "A `diff <(sort) <(sort)` relocation check that never reaches empty on files containing Korean (multibyte) fixture strings"
tags:
  - ls-sdk
  - tests
  - refactor
  - module-decomposition
  - integration-tests
  - rust
  - cargo
---

# Decomposing a Rust integration-test monolith into per-family submodules

## Context

The source `mod.rs` monoliths were decomposed via `pub use submod::*` + `use super::*`
(see [`monolith-split-via-glob-reexport-and-use-super`](monolith-split-via-glob-reexport-and-use-super.md);
PR #72/#77). The test tree (`crates/ls-sdk/tests/`) was explicitly out of scope, so its
five largest files — `live_smoke.rs` (8,172), `market_session_tests.rs` (7,395),
`paginated_tests.rs` (3,822), `order_smoke.rs` (2,295), `account_tests.rs` (1,961) —
became the heaviest in the repo. Splitting them into per-family submodules (plan
2026-06-30-005) looks like the same refactor, but the test tree has **four mechanical
differences** the source split's `pub use` flattening quietly handled for you. Each one,
gotten wrong, fails in a way the compiler does **not** catch.

## Guidance

### 1. `#[path]` is mandatory — an integration-test file is a crate root

Each file directly under `tests/` compiles as its **own crate root** (its own test
binary). For a crate root, `mod foo;` resolves to `tests/foo.rs` (same directory) — **not**
`tests/<parent>/foo.rs`. And a file sitting directly in `tests/` is auto-compiled by cargo
as a *separate* test binary. So a naive `mod balance;` in `tests/account_tests.rs` either
fails (`file not found for module`) or, if `tests/balance.rs` exists, silently spawns a
second binary.

The fix: put family files in a subdirectory (subdir files are never auto-compiled as
binaries) and point at them explicitly with `#[path]`:

```rust
// tests/account_tests.rs  (the thin parent / crate root)
use std::sync::Arc;
use ls_sdk::account::*;
// ... shared imports + fixture consts + helper fns stay here ...

#[path = "account/balance.rs"]
mod balance;
#[path = "account/capacity.rs"]
mod capacity;
#[path = "account/holdings.rs"]
mod holdings;
```

```rust
// tests/account/balance.rs
use super::*;   // re-reaches every parent import/const/helper (see #3)

#[test]
fn cspaq12200_request_serializes_inblock_only() { /* moved verbatim */ }
```

### 2. The `--list` name set gains a `family::` prefix — invariant is base-name + count

Moving a test into a named submodule prefixes its `cargo test --list` path:
`foo` becomes `balance::foo`. There is **no** test-discovery analog of the source split's
`pub use submod::*` flattening — Rust enumerates tests by their real module path. So the
*full-path* name set is **not** byte-identical after a pure move.

What is invariant — and what the silent-drop guard actually needs — is the **base-name
multiset** (the final `::` segment) plus the count. A dropped family makes its base names
vanish, so the diff is non-empty and the drop is caught. The prefix is run-semantics-
preserving for **substring** invocation: `cargo test foo` still substring-matches
`balance::foo`, the `#[ignore]` set is unchanged, and the count is unchanged.

**Caveat — this is NOT preserving for `--exact` callers.** A caller that passes `--exact`
(the repo's `make live-smoke-*` targets do — `run_smoke` runs `cargo test … --exact <name>`)
matches the *full* module path only, so a bare `foo` matches **zero** tests after the move.
The internal base-name snapshot proves no test was dropped, but it does **not** cover
external tooling that names tests by path — that is a separate repo-wide grep, and skipping
it left 194 `run_smoke` targets silently broken after this split. See
[`test-decomposition-breaks-exact-name-callers`](../conventions/test-decomposition-breaks-exact-name-callers.md).

```bash
# scripts/test-name-snapshot.sh — strip to base name, sort, diff before vs after
cargo test -p ls-sdk --test "$bin" -- --list 2>/dev/null \
  | grep ': test$' | sed -E 's/^.*:://' | LC_ALL=C sort
```

This is the **load-bearing** check: a test moved into a family file that is never `mod`-ed
compiles clean and silently runs zero of its tests — the compiler cannot see it. Capture
the base-name set before, diff after, require empty. Also snapshot `--list --ignored`
separately so a stripped `#[ignore]` (which `--list` alone would not reveal) is caught.

### 3. `use super::*;` re-exports the parent's *private* `use` imports to children

Within one crate, `use super::*;` in a child module pulls in the parent module's items
**including the names the parent brought in via its own (non-`pub`) `use` statements** —
because a child can see an ancestor's private items. So the thin parent keeps every
`use`/`const`/helper, and each family file needs only `use super::*;` to resolve all the
types, wiremock matchers, fixture consts, and `sdk_for`-style helpers the moved tests
reference. A pre-existing unused-import warning in the parent stays exactly as it was
(faithful relocation; do not "clean it up" if you want a provable warning-count-unchanged
story).

### 4. `include_str!` fixture consts that stay in the parent need NO `../` fixup

`include_str!` resolves relative to the *current source file's directory*, so the classic
worry is that a cluster moving into `tests/area/family.rs` must rewrite
`include_str!("fixtures/x.json")` to `include_str!("../fixtures/x.json")`. In practice every
fixture here is loaded into a **top-level `const`** (`const X_FIXTURE: &str =
include_str!("fixtures/...")`) that stays in the thin parent and is reached by children via
`use super::*` (#3). Nothing moves into a subdir, so **zero** `../` fixups are needed — a
naive grep-count of `include_str!` over-estimates the work (the plan guessed 15/15/22; the
real count was 0/0/0). Only an `include_str!` that physically moves into a subfile needs the
fixup, and it is compiler-caught (file-not-found at compile time) if missed.

### 5. The relocation/no-body-edit proof — and its locale trap

The `--list` base-name check catches drops/renames but **cannot** catch a silently *edited*
assertion inside a moved test. The stronger guard is a line-multiset proof: the non-blank
line multiset of (new parent + all family files), minus the structural additions
(`mod`/`#[path]`/`use super::*;`) and with `../fixtures` normalized back to `fixtures`,
equals the original monolith's multiset. Run it with the committed verifier:

```bash
scripts/relocation-check.py <base-ref> <parent-path> <family-dir>
# RELOCATION OK  ⇒  pure move, no test body changed
```

**Do not** reimplement this as a shell `diff <(sort …) <(sort …)`: under default UTF-8
collation, `sort` reorders the Korean fixture strings (`정상처리` / `조회완료` …)
inconsistently between the two sides, producing a permanent false-positive symmetric diff
that never reaches empty — masking the exact one-line edit the check exists to catch. The
committed script compares multisets in Python (collation-independent); any shell fallback
MUST pin `LC_ALL=C` on every `sort`.

### Order of operations per file (one atomic PR each)

1. Capture `before.txt` (base names) and `*.ignored.before.txt`.
2. Move each `// ----`/per-TR cluster into `tests/<area>/<family>.rs`; assign family by
   TR→family membership read from `src/<area>/*.rs` (`pub struct <TR>Request`), not guesswork.
3. Thin parent keeps imports + consts + helpers + `#[path] mod` decls; each family gets
   `use super::*;`.
4. `relocation-check.py` ⇒ RELOCATION OK; `--list` base-name diff empty; ignore-set diff
   empty; suite passes; no new warnings.
5. Full gate green (`make docs && cargo test && cargo test -p ls-core && make docs-check`) —
   docgen count tests stay byte-identical with zero edits because no `src/`/metadata/baseline
   file is touched.

## Why This Matters

The defining risk that separates this from the source decomposition: a moved-but-unwired
test fails **silently**. A forgotten `mod` drops a whole file's tests with no compile error
and a green gate. The compiler is the source split's safety net (a dropped `pub use` breaks
every caller); here the **base-name snapshot is the only thing standing between a "clean"
refactor and silently deleting hundreds of tests**. Mis-handling `#[path]` (silent extra
binary), the `family::` prefix (false-alarm diff that trains you to ignore real ones), or
the locale trap (a relocation proof that can never go green) each erode that net.

## When to Apply

- Splitting any `crates/ls-sdk/tests/*.rs` integration-test monolith into submodules.
- Any Rust integration-test (under `tests/`) submodule decomposition — the crate-root
  module-resolution and `--list` prefix behaviors are language-level, not repo-specific.
- Reviewing such a refactor: confirm every family file on disk is `mod`-ed in its parent
  (an orphaned `tests/<area>/x.rs` is invisible to `cargo test`), and that the relocation
  proof was run locale-safe.

## Examples

Result of plan 2026-06-30-005 (branch `refactor/test-file-decomposition`): five monoliths →
thin parents + per-family submodules — `account_tests` 72 tests → 3 families;
`paginated_tests` 158 → 18; `market_session_tests` 347 → 9; `order_smoke` 33 → parent +
`order/{chain,fo}` (the F/O block was a self-contained contiguous tail, extracted whole
with its helpers; the autonomy/scrub/suppressor/nonce/flat_verdict helper-core + its safety
tests stayed byte-identical in the parent); `live_smoke` 204 (198 `#[ignore]`) → 8 buckets.
Gate green throughout, `1321 passed / 201 ignored` identical to baseline, every per-file
base-name `--list` set and ignore-set identical, every relocation proof `RELOCATION OK`.

See also: [`monolith-split-via-glob-reexport-and-use-super`](monolith-split-via-glob-reexport-and-use-super.md)
(the source-tree counterpart and its body-multiset proof) and
`docs/plans/notes/2026-06-30-005-test-split-baseline.md` (the committed baseline + procedure).
