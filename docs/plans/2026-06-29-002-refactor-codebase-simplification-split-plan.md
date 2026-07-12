# Plan — Codebase-wide simplification, split into manageable waves

**Date:** 2026-06-29 · **Type:** refactor (behavior-preserving) · **Status:** planned, not executed

## Goal

Simplify the whole `ls-*` workspace for clarity, consistency, and maintainability
**without changing any behavior or public API**, split into independently-shippable
chunks. Each chunk is sized to run cleanly through the `ce-simplify-code` flow
(3-reviewer pass → fix → gate) and land as one reviewable PR.

## Why now / what the measurement showed

| File | Lines | Structs | Shape |
|---|---:|---:|---|
| `crates/ls-sdk/src/market_session/mod.rs` | 11,789 | 474 | per-TR monolith, one file |
| `crates/ls-core/src/endpoint_policy.rs` | 5,077 | — | flat policy registry (509 `_POLICY`) |
| `crates/ls-sdk/src/realtime/frame.rs` | 4,977 | 102 | per-channel monolith, one file |
| `crates/ls-sdk/src/account/mod.rs` | 2,406 | 73 | per-TR monolith, one file |
| `crates/ls-sdk/src/paginated/*` | — | — | **already split per-domain — the target shape** |

The dominant cost is **structural boilerplate**, not algorithmic complexity:
~474+102+73 single-purpose per-TR structs in three flat files, plus ~2,400 repeated
serde-attr lines. `paginated/` already demonstrates the per-domain layout we want
everywhere.

### Premise check — confirm the pain before spending the PRs

The measurement above is file-size, which is a *proxy*. Before committing the wave,
record the concrete maintainer/agent pain it relieves — a recent merge conflict in
the monolith, a measured compile-time delta, or an agent/recipe navigation failure.
If no such signal exists, **do not run the full wave**; take the *stop-the-bleeding*
alternative below instead. File size alone is not a sufficient mandate for ~18 PRs.

**Churn caveat (time-sensitive).** `market_session/mod.rs`, `realtime/frame.rs`, and
`endpoint_policy.rs` are the exact files the active TR add/flip loop edits on nearly
every feature PR (e.g. #68–#71 all landed 2026-06-29 touching them). A multi-PR
refactor that reshuffles these files collides head-on with that loop. This wave is
only safe to run in a **confirmed lull** — note that MEMORY records the raw TR pool
as exhausted (0 left) as of 2026-06-29, which *may* be that lull. Confirm the loop is
quiesced (or freeze it per the merge-window rule below) before starting Wave 1.

### Alternative considered — "stop the bleeding" (the 80/20)

Instead of retro-migrating ~650 already-stable structs across three files
(~10 mechanical PRs + conflict risk), the cheaper path:

1. Split only the single worst file (`market_session/mod.rs`) once, as **one atomic
   PR** during a lull — not 5 sequential PRs.
2. Mandate that **new TRs land in per-domain sibling files** going forward (the
   `paginated/` shape already proves this works with `pub use submod::*`).

This captures most of the navigability benefit, ends the "everything in one flat
file" growth, and avoids most of the churn against in-flight work. `frame.rs`,
`account/mod.rs`, and `endpoint_policy.rs` splits (Waves 2–3) become **optional
follow-ups gated on a real navigation complaint**, not bundled up front. Prefer this
alternative unless the premise check above surfaces pain that justifies the full
retro-split.

## Load-bearing safety property (read first)

In `market_session/mod.rs` each TR's **struct cluster** is self-contained and
delimited by `// ----------` seams: doc comment → `…Request` → `…InBlock`/`…OutBlock`
structs → `impl …Request`. The gate's count tests (`docgen reference.len` /
`banner_trs` / `TRACKED_TRS`, `ls-trackers cli.rs` literals, `api_drift`) key on
**public item paths and metadata**, not file locations.

> **Therefore: moving a TR's struct cluster to a sibling file and re-exporting it
> with `pub use submod::*` from `mod.rs` changes zero public paths → all count tests
> stay identical → the gate stays green with no count-test edits.** Verified against
> the codebase: docgen renders from `metadata/`, not SDK source layout; no count
> test enumerates structs by file; the 297 `_POLICY` consts are unique by TR (no
> glob collision).

**What is NOT in a section (do not try to move these):** the **facade methods** all
live in a single bottom `impl MarketSession` block (≈line 10,695) — an `impl` block
cannot be glob-split, so it stays in `mod.rs`. The **`{TR}_POLICY` consts** live in
`ls-core/src/endpoint_policy.rs`, not in the SDK section at all (the facade only
*references* them). So a Wave-1 "section move" relocates the struct cluster only; the
facade method and the policy const stay where they are.

This makes Waves 1–3 (the bulk of the LOC) low-risk mechanical moves — with the
per-file caveats called out in each wave below (shared helpers in `frame.rs`,
crosscheck-list scope in `endpoint_policy.rs`). Waves 4–5 touch logic/serde and get
the full reviewer scrutiny.

## Gate (every chunk, no exceptions)

```
make docs && cargo test && cargo test -p ls-core && make docs-check
```

Never commit on red. **Do not `cargo fmt` the whole `ls-trackers` crate** (main is
intentionally unformatted there; CI doesn't enforce it — a blanket format yields a
huge spurious diff). Format only the lines you touch.

---

## Wave 0 — Baseline & invariant harness (prereq, 1 chunk)

Lock in the behavior-preservation contract before any move. This is a **checklist
folded into the Wave-1 PR description**, not a separate committed artifact.

- Capture a green baseline: run the full gate, record the current count-test values
  (`reference.len`, `banner_trs`, `maintained_tr_count`, `TRACKED_TRS`) in the PR
  description; they must be identical after merge.
- **In-repo public-path coverage is already provided by the compile gate**, not a
  snapshot: `crates/ls-sdk/tests/market_session_tests.rs` imports ~30+ struct names
  from `ls_sdk::market_session`, so a dropped re-export fails `cargo test` at compile
  time. Do not hand-roll a `grep pub` snapshot — it cannot detect re-export ordering
  or path drift and duplicates what the compiler already enforces.
- **Cross-crate consumer coverage (only if it matters):** this is an SDK crate, so a
  glob re-export *could* in principle reorder a rustdoc path a downstream consumer
  outside this repo depends on. If that risk is in scope, add `cargo public-api` as
  an explicit install step (it needs the rustdoc-json nightly) and diff its output
  per chunk — a deterministic API diff, not `grep`. If no out-of-repo consumer
  exists, skip this entirely; the compile gate suffices.
- Document the re-export rule (above) in the chunk PR description as the review
  acceptance criterion.

**Exit:** baseline count-test values recorded in the Wave-1 PR description; re-export
rule written down; (optional) `cargo public-api` baseline captured if cross-crate
coverage is in scope.

---

## Wave 1 — Split `market_session/mod.rs` (the monolith), 5 chunks

Mirror the `paginated/` layout: `market_session/` stays a directory module whose
`mod.rs` retains the `MarketSession` facade `impl` block (≈line 10,695, **1,099
lines** — it does not move) + `pub use` re-exports; each TR's **struct cluster**
moves into a per-family sibling file.

Group the ~50 TR struct clusters by family into **~5 chunks of ~10 each**
(quote/price, ranking, sector/index, investor/flow, masters/reference — final
grouping read off the actual section headers). Per chunk:

1. Create `market_session/<family>.rs`, move those struct clusters verbatim
   (cut/paste, no edits). Move the structs + their `impl …Request` only — **leave the
   facade methods in the bottom `impl MarketSession` block** and the `_POLICY` consts
   in `endpoint_policy.rs` (they were never in the section).
2. Add `mod <family>; pub use <family>::*;` to `mod.rs`.
3. Run the gate. Green + the count-test values from Wave 0 unchanged ⇒ ship.

No logic, naming, or serde changes in this wave — pure relocation of struct clusters.
Keeps each PR a clean move diff that's trivial to review.

**Within-wave PRs are strictly sequential, not parallel:** all 5 chunks add lines to
the same `mod.rs` (the `mod <family>;` / `pub use` declarations), so two open
concurrently will merge-conflict. Land them one at a time, or collapse the whole
split into **one atomic PR** during a lull (preferred per the stop-the-bleeding
alternative — fewer conflict windows against the active TR loop).

**Exit:** every TR struct cluster lives in a per-family file; `market_session/mod.rs`
holds only the ~1,099-line facade `impl` + module docs + `mod`/`pub use` lines
(≈1,150 lines total). Splitting the facade itself is **out of scope** for this wave.

---

## Wave 2 — Split `realtime/frame.rs` (2 chunks) + `account/mod.rs` (1–2 chunks)

Similar in spirit to Wave 1, but **`frame.rs` is NOT a pure-move recipe** — it has
structure `market_session` lacks. Resolve these three preconditions *before* moving
any channel cluster, or the "move" won't compile and the diff won't be clean:

- **Shared module-private helpers.** `frame.rs` has top-level fns that moved channel
  clusters call — `build_frame` (private), `composite_key` (`pub`),
  `split_composite_key` / `build_subscribe_msg` / `build_unsubscribe_msg`
  (`pub(crate)`). A cluster moved to `realtime/frames/<family>.rs` that references
  `build_frame` won't see it. **Decide their home first:** keep the helpers in
  `frame.rs` and make any private one (`build_frame`) `pub(crate)`, and add
  `use super::*;` (or explicit `use super::build_frame;`) in each submodule. This is
  a real (small) edit — not zero-edit relocation.
- **Inline tests stay put.** `frame.rs` has ~107 `#[test]`s in a tail
  `#[cfg(test)] mod tests { use super::*; }`. Leave that block in `frame.rs`; its
  `use super::*` resolves the moved structs through the `pub use frames::*` re-export.
  Do not relocate tests piecemeal.
- **Named re-exports must keep resolving.** `realtime/mod.rs` re-exports `frame`'s
  items by **explicit named list** (`pub use frame::{…}`), not a glob. After moving
  clusters into `frames/<family>.rs` and globbing them back into `frame.rs` via
  `pub use frames::*`, confirm every name in `mod.rs`'s named list still resolves.

Then: `frame.rs` (102 structs) splits per channel-family into
`realtime/frames/<family>.rs`, re-exported from `frame.rs`. ~2 chunks (sequential —
same shared-`frame.rs` conflict rule as Wave 1).

`account/mod.rs` (73 structs) is closer to the pure-move recipe (no module-private
helpers shared across clusters; manual `Debug` impls move verbatim with their
structs): split per account-domain (balance, holdings, orders-inquiry,
capacity/deposit) into `account/<domain>.rs`, re-export. 1–2 chunks. Verify it has no
shared top-level helper before treating it as zero-edit.

**Exit:** no SDK source file > ~1,500 lines except by genuine cohesion. (`frame.rs`
at 4,977 lines may need >2 channel-families to clear the ceiling — size the chunks to
the threshold, not to a fixed count.)

---

## Wave 3 — Split `endpoint_policy.rs` registry (1–2 chunks)

`ls-core/src/endpoint_policy.rs` (5,077 lines, 509 policies). Split the flat const
block by `owner_class`/domain into `endpoint_policy/<group>.rs` submodules,
re-export so existing `endpoint_policy::FOO_POLICY` paths are unchanged.

**Caution:** the **two crosscheck lists** enumerate policy consts by bare name:
`slice_rest_policies_are_non_order_rest` (in-source `#[test]` at
`endpoint_policy.rs:4832`) and `slice_policies_mirror_metadata_index` (in
`crates/ls-core/tests/policy_index_crosscheck.rs`). Both resolve the consts by bare
name through `use` imports / an in-source array. **Keep both test bodies in the new
`endpoint_policy/mod.rs`** (the in-source one) and the integration test importing from
`ls_core::endpoint_policy` — since `mod.rs` does `pub use submod::*`, every bare const
name stays in scope with no extra imports. Do not move a crosscheck body *into* a
submodule, or its bare const references break. This is the one place a move can break
a (test-only) list; the gate catches it.

**Exit:** policy registry navigable by domain; both crosscheck lists green.

---

## Wave 4 — Serde-attr / derive boilerplate consolidation (3–4 chunks)

Now that files are small, collapse the repeated
`#[serde(serialize_with = "string_as_number")]` / `string_or_number` /
`string_as_decimal` attributes where it is behavior-safe.

**This wave touches serialization → behavior-sensitive.** Smaller chunks, full
reviewer scrutiny, and a serde round-trip assertion per touched struct family.

**The "~2,400 attrs" figure is the gross count, not the achievable reduction.** A
single wrapper type *cannot* serve both directions: a `String` field needs JSON-number
serialization on requests (`string_as_number`, ~84 in `market_session` alone) but
tolerant deserialization on responses (`string_or_number`, ~1,053). A direction-keyed
newtype is therefore *two* types, and the real reduction is bounded by how many fields
share **both** a direction and a coercion. Size this wave to that number, not to 2,400.

**Decision gate (run before committing the wave's PR budget).** Sample one already-split,
stable family from `paginated/` (it has offline decode/serialize tests) and prototype
the `WireNum`/`WireDec` newtype against it:

- If the newtype passes the existing tests and meaningfully cuts attrs → **adopt**, and
  record the realistic attr-reduction ceiling. Then size the chunk count to that ceiling
  (it may be fewer than 3–4 PRs).
- If the `IGW40011` split or per-field exceptions block equivalence → **skip the wave
  entirely** and note why. Do not spend 3–4 PRs discovering this per-family.

- Apply per file-group (one chunk per Wave-1/2 family), each gated on existing offline
  decode/serialize tests staying green.
- **`paginated/` scope:** it holds ~1,018 of these attrs in already-split files. To
  avoid two coexisting conventions, **back-apply any adopted wrapper to `paginated/`**
  as part of this wave (or explicitly declare `paginated/` a follow-on and lock that
  decision here so a future implementer doesn't re-litigate it).

**Exit:** serde attrs reduced where behavior-safe (to the measured ceiling, or wave
skipped with rationale); `IGW40011` direction invariant preserved and asserted;
`paginated/` convention reconciled or explicitly deferred.

---

## Wave 5 — Logic-heavy reuse/quality/efficiency passes (1 chunk per file)

> **Scope note — this wave is separable from the structural mandate.** The plan's
> goal is structural boilerplate, *not* algorithmic complexity, and this wave touches
> the highest-risk behavior-sensitive logic (`reconcile.rs` carries the 취소/거부 P0
> history). It is the lowest goal-alignment, highest-risk wave. Prefer to run each
> file's pass as a **separately-justified task driven by an actual defect or
> complexity signal**, not bundled into the structural split. Kept here only as a
> backlog of candidates.

The genuine `ce-simplify-code` 3-reviewer passes, on the files with real control
flow (not boilerplate). One chunk each, in this order:

1. `ls-sdk/src/orders/reconcile.rs` (975) — state-classification logic; highest
   value, highest care (the 취소/거부 mis-classification history lives here — do
   **not** thin any status-classification branch without the offline matcher tests
   green).
2. `ls-core/src/inner.rs` (1,721) — dispatch/pagination core.
3. `ls-metadata/src/validator.rs` (890).
4. `ls-trackers/src/freshness.rs` (1,205) + `fetch.rs` (1,129) — note prior
   live-path shell-bug history; keep regression tests.
5. `ls-trackers/src/cli.rs` (3,368) — large but mostly count literals; reuse pass
   only, **no blanket fmt**.

Each runs the standard reviewer trio (reuse / quality / efficiency), fixes applied
only where behavior is provably preserved, gated. Safety checks
(validation, error handling, secret-scrubbing, the dispatch log suppressor) are
**never** simplified away.

**Exit:** logic files pass a clean reviewer trio with no actionable findings left.

---

## Sequencing & sizing summary

| Wave | Chunks | Risk | Nature |
|---|---:|---|---|
| 0 Baseline | folded into Wave 1 PR | none | checklist + rule |
| 1 market_session split | 5 (or 1 atomic) | low | struct-cluster moves |
| 2 frame + account split | 3–4 | low-med | moves + `frame.rs` helper edits |
| 3 policy registry split | 1–2 | low-med | moves + crosscheck-list scope |
| 4 serde consolidation | 0–4 (gated) | med | serde-sensitive, pre-check gate |
| 5 logic passes | 5 (separable) | med-high | real simplification, defect-driven |

**~15–21 PRs** depending on the stop-the-bleeding decision and the Wave-4 gate
outcome. Sequencing rules:

- **Within a wave, PRs that touch the same file are strictly sequential, not
  parallel.** Wave 1's 5 chunks all edit `market_session/mod.rs`; Wave 3's chunks all
  edit `endpoint_policy.rs`; Wave 2's `frame.rs` chunks all edit `frame.rs`. Two open
  at once conflict. *Cross-wave* parallelism (e.g. a Wave-2 `account/` PR while a
  Wave-1 PR is open) is fine — different files.
- **Merge-window coordination with the active TR loop is mandatory.** These three
  files are edited by nearly every feature PR. Either freeze TR-flip work against the
  target file while its split PR is open, or land each monolith's split as a single
  fast PR in a confirmed lull — do not run a long-lived refactor branch against
  in-flight flip waves, it will rot.
- Wave 0 checklist is part of the first Wave-1 PR; Wave 4 should follow Waves 1–2
  (smaller files = safer edits); Wave 5 files don't overlap Waves 1–3, so they may run
  anytime — but `cli.rs` (Wave 5) is also touched by TR-flip count-literal edits, so
  apply the same merge-window rule to it.

## Out of scope / explicitly not doing

- No public API changes, no renames of TR structs/policies, no metadata/baseline edits.
- No behavior changes to wire serialization beyond behavior-equivalent helper extraction.
- No reformatting of `ls-trackers` beyond touched lines.
- Generated docs (`docs/reference/`, `docs/tr-dependencies/`) are projected — never hand-edited.
