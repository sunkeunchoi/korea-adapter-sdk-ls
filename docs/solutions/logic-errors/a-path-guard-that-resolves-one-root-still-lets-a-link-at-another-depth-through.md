---
title: "A path guard that canonicalizes one root still lets a link at another depth through, and a failed canonicalization is not proof there is nothing to follow"
date: 2026-09-15
category: logic-errors
module: "frozen-catalog write guard — nautilus_ls::ingest::ensure_catalog_writable (adapters/nautilus/src/ingest/mod.rs); sibling shell preflight (adapters/nautilus/scripts/session-morning.sh)"
problem_type: logic_error
component: ingest
symptoms:
  - "A data home whose catalog/ symlinked into the FROZEN judgment home passed the marker guard and wrote into the bars a committed catalog_fingerprint was taken from"
  - "After adding one resolved candidate (the real catalog's parent), three more path shapes still passed — the fix's own doc comment asserted the remaining case was the only one"
  - "A leaf link pointing one level INSIDE the frozen catalog (<home>/catalog -> <frozen>/catalog/data) resolved too deep, so the real parent carried no marker"
  - "A link at <catalog>/data — where every bar actually lives — left the resolved catalog ROOT an ordinary unmarked directory"
  - "An unresolvable path carrying `..` (<frozen>/missing/../catalog) missed every candidate, and the caller's own create_dir_all then made it resolve into the frozen catalog"
  - "The refusal told the operator to remove a symlink on a `..` path where no symlink existed anywhere"
root_cause: logic_error
resolution_type: code_fix
severity: high
related_components:
  - frozen-catalog-marker
  - ingest-write-path
  - session-morning-preflight
  - lineage-judgment-pin
tags:
  - path-canonicalization
  - symlink
  - fail-closed
  - guard-boundary
  - toctou
  - frozen-catalog
  - overclaiming-comment
---

# A path guard that canonicalizes one root still lets a link at another depth through, and a failed canonicalization is not proof there is nothing to follow

## Problem

`ensure_catalog_writable` refuses a catalog mutation when a `FROZEN-20260812` marker declares the
data home to be the lineage judgment catalog. It compared **logical** paths only — the home beside
the catalog, and the catalog itself — so a data home whose `catalog/` was a symlink into the frozen
home carried the marker on neither, while the write followed the link into the pinned bars.

The obvious repair is to canonicalize the catalog and look beside **that**. It is not enough, and
the dangerous part is that it *looks* complete: the repair shipped with a doc comment asserting the
leaf's own redirection was "the single case they miss". Three shapes still wrote into the frozen
bars, and that sentence is what would stop the next maintainer from checking.

## Symptoms

- `<home>/catalog -> <frozen>/catalog/data` — a **leaf** link, exactly the case the fix claimed to
  close, but pointing one level in. It resolves to `<frozen>/catalog/data`, whose parent is
  `<frozen>/catalog`, which carries no marker (the marker sits beside the catalog, not inside it).
- `<home>/catalog/data -> <frozen>/catalog/data` — the catalog root is a real, unmarked directory
  by every logical *and* resolved test of the root. The redirection is one level below it, and
  `ParquetDataCatalog` writes every bar under `catalog/data`.
- `<frozen>/missing/../catalog` — `canonicalize` fails on the absent segment, so the resolved
  candidate drops out; both logical candidates miss. The guard returns `Ok`, and the caller's own
  `create_dir_all` (which every mutating entry point runs immediately after the guard) then creates
  the segment, at which point the path resolves to `<frozen>/catalog`. `delete_bar_series` deletes
  a frozen series through it.
- The refusal message asserted a symlink whenever the hit came from the resolved candidate. On a
  `..` path with no link anywhere, the operator is sent to delete something that does not exist.

## What Didn't Work

- **Adding one resolved candidate.** Whack-a-mole: each new shape needs another candidate, and the
  comment written alongside it hardens the wrong boundary into the contract.
- **Walking every real ancestor.** It catches the shapes above but reaches the filesystem root and
  breaks the deliberate boundary `a_marker_two_levels_up_does_not_freeze_the_home` pins — an
  unrelated same-named file above a home must not freeze it.
- **Treating a failed `canonicalize` as "nothing on disk, so no link to follow".** True only when
  nobody creates the path afterwards. Here the caller creates it four lines later.

## Solution

Move the marker's own **contract** into the predicate instead of accumulating candidates. The
marker means *"the data home directly containing this `catalog/` is frozen"*, so:

1. Resolve the roots a write can land in — the catalog **and** `CATALOG_BAR_DIR` (`catalog/data`),
   the only subpath whose redirection reaches the pinned bars.
2. From each resolved root, treat any ancestor **literally named `catalog`** as a catalog root and
   test its home for the marker (`frozen_homes_of`). Only that name counts, which is what keeps the
   walk from reaching the filesystem root.
3. Refuse outright when a path does **not** resolve and still carries `..`. Fail closed on not
   knowing the destination; a plain not-yet-created catalog has no `..` and still writes.
4. State what the path **resolved to** rather than asserting a cause. "This path resolves to X,
   which is inside the frozen home; if a symlink is redirecting it, removing the link is the fix."

The logical pair stays, and one claim about it *is* load-bearing and true: `Path::exists` follows
the whole logical path, so a marker behind a linked **ancestor** is already found there. Only the
leaf and below need resolving. `a_linked_ancestor_is_caught_by_the_logical_candidates` pins that
reason so a refactor cannot quietly remove it.

## Why This Works

The guard stops asking "is the marker at one of these paths" and starts asking "does any home that
governs the place this write lands carry the marker" — which is what the freeze always meant. The
`..` refusal closes the one case where the guard genuinely cannot answer, in the fail-closed
direction, instead of guessing.

## Prevention

- When a guard resolves a path, ask **what the writer actually opens**, not what the caller passed.
  Here bars live at `<catalog>/data`, so canonicalizing `<catalog>` protected the wrong node.
- A guard's doc comment that names "the single remaining case" is a claim under test. Either pin it
  with a test or do not write it. This one was false in two ways and would have been believed.
- `canonicalize(...).ok()` discards *why* it failed. Before reading a failure as safe, check whether
  anything between the guard and the write creates the path.
- Test a refusal's **negative** side too: that a plainly marked home refuses *without* the
  link-specific sentence. A flag or ordering regression that marks every hit resolved is otherwise
  invisible.
- The cheapest way to settle a path-predicate argument is a **compiled replica** of the guard run
  against the real on-disk layout. That, not reasoning, is what found the leaf-into-subdirectory
  shape here.
- A boundary you deliberately leave open (links at any other depth) belongs in the comment as an
  open boundary, with what *does* cover it named — `rehearsal-bootstrap.sh` refuses to clone a
  source catalog containing any symlink, and that covers cloned sources only.

## Related Issues

- `docs/solutions/logic-errors/safety-invariant-proven-at-a-leaf-can-be-re-violated-by-a-coarser-grained-caller.md`
  — the sibling shape. This fix follows that lesson by living in the one choke point all five
  mutating entry points funnel through, rather than in the single caller that remembered to check.
- `docs/solutions/logic-errors/empty-repull-completing-destructive-heal-destroys-history.md` — why
  a redirection into the judgment catalog is data loss, not duplication: `heal_daily`,
  `compact_catalog` and `delete_bar_series` are genuinely destructive.
- `docs/solutions/conventions/testing-an-unreachable-fail-closed-branch-and-coverage-trim-invariants.md`
  — same module; the precedent for pinning a fail-closed branch that looks unreachable.
- PR #322 recorded this as an unapplied P2; PR #323 (merge `64aca71`) closed it.
