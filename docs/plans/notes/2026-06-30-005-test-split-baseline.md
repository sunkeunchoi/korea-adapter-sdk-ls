# Test-split baseline — plan 2026-06-30-005 (test-file decomposition)

Committed, diffable behavior-preservation anchor for the five-file integration-test
decomposition. Every later unit diffs its per-file `--list` test-name set and the
docgen count values against **this file**, not an ephemeral PR description (U1 in the
plan). Captured on a clean, gate-green `main` at the start of the refactor.

Lull confirmed: the raw TR pool is exhausted as of 2026-06-30 (MEMORY); PR #75's F/O
order chain is already merged. The two high-churn giants (`market_session_tests.rs`,
`live_smoke.rs`) land in this window (R7).

## The load-bearing check — per-file test-name snapshot (KD3 / R2)

For each target binary, the test-name set MUST be invariant before and after the
split. A dropped `mod` (silent test loss) or a renamed test shows up as a non-empty
diff and **blocks the merge**.

**Realized form of KD3 (the `family::` prefix).** Moving a test into a named
submodule unavoidably prefixes its `--list` path: `foo` becomes `balance::foo`.
There is no test-discovery analog of the source split's `pub use submod::*`
flattening — Rust enumerates tests by their real module path. So the *full-path*
name set is NOT byte-identical; what is invariant (and what the silent-drop guard
actually needs) is the **base-name multiset** — the final `::` segment — plus the
count. A dropped family still makes its base names vanish ⇒ non-empty diff ⇒ caught.
The prefix is run-semantics-preserving: `cargo test foo` still substring-matches
`balance::foo`, the `#[ignore]` set is unchanged, and the count is unchanged.

```
# BEFORE (flat names):
cargo test -p ls-sdk --test <bin> -- --list 2>/dev/null | grep ': test$' | sort > before.txt
#  ... perform the split ...
# AFTER: strip the module path to the base name, then diff:
cargo test -p ls-sdk --test <bin> -- --list 2>/dev/null | grep ': test$' \
  | sed -E 's/^.*:://' | sort > after.base.txt
diff before.txt after.base.txt   # MUST be empty; count MUST match
```

`scripts/test-name-snapshot.sh <bin>` wraps this (emits the sorted base-name set).

A second, stronger pure-relocation check (catches a silently *edited* test body that
`--list` cannot): the non-blank line multiset across the new parent + family files —
minus the added `mod <fam>;` / `#[path = "…"]` / `use super::*;` lines and with
`../fixtures` normalized back to `fixtures` — equals the multiset of the original
monolith's lines. Run it with the committed, locale-safe verifier:

```
scripts/relocation-check.py <base-ref> <parent-path> <family-dir>
# e.g. scripts/relocation-check.py main \
#        crates/ls-sdk/tests/account_tests.rs crates/ls-sdk/tests/account
```

`RELOCATION OK` ⇒ relocation only. **Do not** reimplement this as a shell
`diff <(sort …) <(sort …)`: the default UTF-8 collation makes `sort` reorder the
Korean fixture strings (`정상처리` / `조회완료` …) inconsistently between the two
sides, producing a permanent FALSE-POSITIVE symmetric diff that never reaches empty —
masking exactly the one-line body edit this check exists to catch. The committed
script compares multisets in Python (collation-independent); a shell fallback MUST
pin `LC_ALL=C` on every `sort`.

## Per-file BEFORE test counts (the diffable "before")

| File | Lines | `--list` tests | `#[ignore]` |
|---|---|---|---|
| `live_smoke.rs` | 8,172 | **204** | **198** |
| `market_session_tests.rs` | 7,395 | **347** | 0 |
| `paginated_tests.rs` | 3,822 | **158** | 0 |
| `order_smoke.rs` | 2,295 | **33** | **3** |
| `account_tests.rs` | 1,961 | **72** | 0 |

The per-binary `before.txt` / `*.ignored.before.txt` snapshots are captured at the
start of each unit; the counts above are the invariants each split must reproduce.

## Count-test invariant (must be byte-identical after every unit; zero test edits)

| Count test | Value | Source of truth |
|---|---|---|
| `TRACKED_TRS` length | **320** | `crates/ls-docgen/src/lib.rs:677` (`[&str; 320]`) |
| `reference.len()` | **280** | `crates/ls-docgen/src/lib.rs:1408` |
| `maintained_tr_count` | **320** | `crates/ls-trackers/tests/api_drift.rs` + `baselines/api-drift/normalized/manifest.json:3` |
| `banner_trs` list | unchanged literal (265 entries) | `crates/ls-docgen/src/lib.rs:1116` |

This refactor touches no `src/`, metadata, or baseline file, so all four are
guaranteed unchanged with zero edits — but each unit re-runs the gate to prove it.

## Starting gate (all green, recorded pre-move)

```
make docs && cargo test && cargo test -p ls-core && make docs-check
```
→ `cargo test` workspace: **1321 passed, 201 ignored** (201 = 198 `live_smoke` + 3
`order_smoke`); `cargo test -p ls-core`: green; `make docs-check`: no diff; exit 0.

## Recipe (every unit)

1. Capture `before.txt` (and `*.ignored.before.txt` for the smoke files).
2. Parent keeps: module doc + `use` block + shared `const`s + shared helper fns;
   gains `mod <family>;` declarations. **`include_str!` consts that stay in the
   parent need no `../` fixup** (KD6 fixups apply only to `include_str!` that *moves*
   into a `tests/<area>/` subfile).
3. Each family file: `use super::*;` header + its moved test clusters (with section
   comments), `../fixtures/` fixup on any moved `include_str!`.
4. Capture `after.txt`; `diff` MUST be empty; run the line-multiset relocation check.
5. Full gate green. One atomic PR per file (R7).
