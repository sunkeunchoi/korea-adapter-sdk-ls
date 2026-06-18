---
title: "feat: Physically decommission the migration source (in-repo half)"
type: feat
date: 2026-06-18
origin: docs/brainstorms/2026-06-18-physical-decommission-migration-source-requirements.md
---

# feat: Physically decommission the migration source (in-repo half)

## Summary

Declare `korea-broker-sdk-ls` a **Decommissioned Migration Source** inside this
repo: add an anchor marker at `docs/migration-source/README.md`, record the
decommission in new ADR `0014`, rewrite the active framing that still calls the
old repo a live reference (the `README.md` role section and the `fetch.rs`
provenance comment), and add a regression guard in the existing
`decommission_audit.rs` that keeps the boundary closed. Retained audit evidence
and historical `Provenance:` citations stay untouched.

---

## Problem Frame

The decommission audit reached **TRUSTWORTHY-GREEN** (26/26 rows confirmed, 0
acceptances), and its committed validator `crates/ls-trackers/tests/decommission_audit.rs`
recomputes that gate from frozen records so it stays defensible in CI after the
old source is gone. The audit report names this physical-decommission step as
the follow-up that must *cite* that verdict (see origin:
`docs/brainstorms/2026-06-18-physical-decommission-migration-source-requirements.md`).

What is missing is the in-repo declaration that the precondition has been met.
The repo still carries present-tense language treating the old repo as something
maintainers reference — in `README.md` and in `crates/ls-trackers/src/fetch.rs`.
Until that is corrected and guarded, nothing prevents a future change from
quietly reintroducing a dependency on a source that is supposed to be gone.

This PR cannot delete the sibling repo — that is an external ops act. It does
the in-repo half: mark the relationship decommissioned, fix the stale pointers,
and lock the boundary with a test. The guard distinguishes **retained evidence**
(allowed: `Provenance:` citations, the audit tree, the frozen ledger) from a
**live dependency** (forbidden: a filesystem path to the old repo, or
present-tense instruction to consult it for ordinary maintenance).

---

## Requirements

**Anchor marker**

- R1. `docs/migration-source/README.md` exists and states that `korea-broker-sdk-ls`
  is a Decommissioned Migration Source: retained provenance and audit evidence may
  cite it, but ordinary maintenance must not consult, import, build against, or
  otherwise depend on it.
- R2. The marker points to the retained evidence under `docs/migration-source/audit/`
  and names the TRUSTWORTHY-GREEN audit gate as the precondition that authorized
  the decommission.

**Decision record**

- R3. `docs/adr/0014-migration-source-decommissioned.md` exists, status accepted
  2026-06-18, recording that the old repo is now a Decommissioned Migration Source
  and that ordinary maintenance must not consult, import, build against, or depend
  on it.
- R4. ADR `0014` supersedes the operational posture of ADR `0010` without editing
  `0010`; it references `docs/migration-source/README.md` as the anchor and
  `docs/migration-source/audit/` as retained evidence.

**Active-source correction**

- R5. The `README.md` "role of `korea-broker-sdk-ls`" section is rewritten from
  present-tense reference framing to the decommissioned posture, consistent with
  the marker and ADR.
- R6. No `Provenance:` citation in `docs/design/` or `docs/operations/`, and no
  content in `docs/plans/` or `docs/brainstorms/`, is altered.
- R12. The `crates/ls-trackers/src/fetch.rs:3` module-doc comment is normalized
  from present-tense dependency framing ("**is** the endpoint/retry/fallback
  reference", plus the `~/dev/korea-broker-sdk-ls/...` path) to past-tense
  attribution. The `crates/ls-core/src/lib.rs:3` "Ported from …" comment is left
  as-is — attribution, not instruction. (Plan-local, derived from R8: the guard's
  filesystem-path and consult-language checks both fire on the current `fetch.rs`
  line, so it must change for the guard to be green.)

**Regression guard**

- R7. A test in `crates/ls-trackers/tests/decommission_audit.rs` asserts the
  anchor marker `docs/migration-source/README.md` exists.
- R8. The same guard asserts no active non-test source file or active doc carries
  a live old-source dependency: a filesystem path to the old repo, or an
  imperative/present-tense instruction to consult it for ordinary maintenance.
- R9. The guard's scan excludes retained evidence and history so it does not
  self-trip: `docs/plans/`, `docs/brainstorms/`, the audit records and manifest
  under `docs/migration-source/audit/`, the retained `tr-dependencies-2026-06-14.json`,
  `Provenance:` citation lines, and test files. (Implementation extends this set —
  see KTD-4.)

**Verification**

- R10. `cargo test -p ls-trackers` passes, including the existing gate validator
  and the new guard.
- R11. `cargo test --workspace` passes.

---

## Key Technical Decisions

- **KTD-1. New ADR `0014`, leave `0010` untouched.** Add a new decision record;
  `0010` stays as the historical decision and `0014` supersedes only its
  operational posture, not the record. `0011`–`0013` already exist, so `0014` is
  the next free number. No existing ADR uses "supersede" language, so `0014`
  introduces it. (see origin: Key Decisions)

- **KTD-2. The guard is a three-part assertion, line-oriented.** Over the active
  scan domain (KTD-4), the guard asserts: (1) the marker file exists; (2) no line
  in an active non-test file contains a **live filesystem path** to the old repo;
  (3) no line contains **present-tense/imperative consult language adjacent to the
  literal `korea-broker-sdk-ls` name**. Line-orientation and name-adjacency are
  what let retained evidence pass: a pure path check would miss "consult
  `korea-broker-sdk-ls` before changing retry behavior", and a whole-file or
  single-word check would false-positive on attribution prose and on CONTEXT.md's
  "should not need to consult **it**" (no repo name on that line). The matcher is
  a tripwire for known live-framing phrasings, not a general consult detector — a
  live instruction phrased outside the calibrated list (e.g. "check
  `korea-broker-sdk-ls`", "pull from `korea-broker-sdk-ls`") would pass; review,
  not the guard, is the backstop for novel phrasings (see Risks).

- **KTD-3. Path patterns are broadened beyond `../`.** The filesystem-path check
  (KTD-2 part 2) matches `../korea-broker-sdk-ls`, `/korea-broker-sdk-ls`,
  `/Users/.../korea-broker-sdk-ls`, and `~/dev/korea-broker-sdk-ls`. A bare
  `korea-broker-sdk-ls/docs/...` (extraction-provenance form, no leading slash or
  home prefix) does **not** match — those are historical citations, not live
  paths. The consult-language list (KTD-2 part 3) is narrow and calibrated:
  `consult`, `read`, `look at`, `compare against`, `use as reference`,
  `must remain readable`, `grounded by`. Past-tense attribution verbs
  (`Ported from`, `Extracted from`, `extracts ... from`, `to preserve`) are
  deliberately absent so attribution survives. **Phrases match on word boundaries
  (e.g. `\bread\b`), never raw substring** — a substring `read` false-positives on
  `docs/design/release-readiness-and-residual-lessons.md:4` ("…production/**read**iness…",
  which also carries the repo name), and `readable`, `already`, `thread` are
  likewise matched only as whole tokens. This is the single most load-bearing
  precision rule in the guard; U4 pins it with a green calibration fixture.

- **KTD-4. Scan domain = maintained product-surface allowlist; exclusions extend R9.**
  The guard adds a **new** `git ls-files` enumeration (no path argument; parse
  stdout into repo-relative paths), reusing only the existing `workspace_root()`
  and `is_git_tracked()` helpers — the current file uses
  `git ls-files --error-unmatch <rel>` as a per-path tracked predicate, not a
  lister, so the enumeration call is new. The domain is an **allowlist**:
  `crates/**` (non-test) + `docs/**` + root-level `*.md`.
  - **Non-test** = exclude any path containing a `/tests/` segment. This matters:
    the guard file itself, `crates/ls-trackers/tests/decommission_audit.rs`, holds
    the literal `korea-broker-sdk-ls` name and a deliberate
    `consult korea-broker-sdk-ls …` fixture line; the `/tests/` exclusion is what
    stops the guard self-tripping on its own fixtures (pinned by a U4 scenario).
  - Within the allowlist, also exclude `docs/migration-source-extraction-ledger.md`
    — retained evidence; its line 3 legitimately holds
    `/Users/mini/dev/korea-broker-sdk-ls` (the frozen gate target), which would
    otherwise trip KTD-3's absolute-path pattern. This is the one R9-extending
    exclusion the broadened patterns actually require.
  - The agent-harness dirs `.claude/**`, `.agents/**`, `.compound-engineering/**`
    sit **outside** the allowlist, so they are already unscanned — listing them as
    "exclusions" is redundant. The honest framing: the guard intentionally does
    **not** cover the agent harness. That is a **known residual gap**, because a
    future automation hardcoding `~/dev/korea-broker-sdk-ls` or instructing "consult
    korea-broker-sdk-ls" would most plausibly live there. Whether to extend the
    scan to the harness is an Open Question.

- **KTD-5. Guard lives in the existing `decommission_audit.rs`, no new deps.** Per
  R7 the guard is added to the existing file, reusing `workspace_root()`,
  `is_git_tracked()`, and the "prove the logic against in-test fixtures before the
  real artifacts" discipline already established there. No new crate or
  dev-dependency is introduced; the addition is additive and does not touch the
  existing record-consistency gate logic. (see origin: Scope Boundaries)

---

## High-Level Technical Design

The guard's decision flow, applied per git-tracked file then per line:

```
git ls-files  (new enumeration; parse stdout)
   │
   ├─ in allowlist?  (crates/** non-`/tests/`, docs/**, root *.md)      ── no ─▶ skip
   │      └─ minus ledger (retained evidence).  harness dirs            ── no ─▶ skip
   │         (.claude/ .agents/ .compound-engineering/) are already
   │         outside the allowlist — known residual gap, not scanned.
   ▼
per line ──┬─ physical line is a `Provenance:` citation? ──────────────── yes ─▶ skip line
           │   (wrapped continuation lines are NOT skipped — see Risks)
           │
           ├─ line matches a live filesystem path pattern (KTD-3)? ────── yes ─▶ FAIL
           │     ../kbsl · /kbsl · /Users/.../kbsl · ~/dev/kbsl
           │
           └─ line has `korea-broker-sdk-ls` AND a whole-word consult     yes ─▶ FAIL
              phrase (KTD-3)?
                 consult · read · look at · compare against ·
                 use as reference · must remain readable · grounded by

separately: assert docs/migration-source/README.md exists ─── absent ──────────▶ FAIL
```

The pass/fail predicates are pure functions over `(path, line)` so the negative
acceptance cases (AE1, AE4) are proven against in-test fixtures rather than by
mutating the real tree; the same predicates then run over the real git-tracked
file set. This mirrors the existing file's gate-invariant fixtures pattern
(`fixtures_each_class_confirmed_pass`, etc.).

---

## Implementation Units

### U1. Anchor marker doc

**Goal:** Create the in-repo declaration that `korea-broker-sdk-ls` is a
Decommissioned Migration Source.

**Requirements:** R1, R2.

**Dependencies:** none.

**Files:**
- `docs/migration-source/README.md` (create)

**Approach:** Short marker doc. State the decommissioned posture in the
`CONTEXT.md` vocabulary (Decommissioned Migration Source): retained provenance
and audit evidence may cite the old repo, but ordinary maintenance must not
consult, import, build against, or depend on it. Point to retained evidence under
`docs/migration-source/audit/` (manifest, report, `records/L1–L26.yaml`) and name
the TRUSTWORTHY-GREEN gate as the authorizing precondition. Do not paste a live
filesystem path to the old repo into the marker (that would trip the guard);
reference the old repo by name only.

**Patterns to follow:** Term usage from `CONTEXT.md` ("Decommissioned Migration
Source"); evidence layout under `docs/migration-source/audit/`.

**Test scenarios:** Covered transitively by U4 (R7 asserts existence; AE4 asserts
deletion fails the guard). `Test expectation: none -- content doc, behavior is
enforced by U4's guard.`

**Verification:** File exists at `docs/migration-source/README.md`, uses the
canonical term, links the audit evidence, and names the gate. Contains no live
filesystem path to the old repo.

### U2. ADR 0014 — migration source decommissioned

**Goal:** Record the decommission as a decision that supersedes `0010`'s
operational posture without editing `0010`.

**Requirements:** R3, R4.

**Dependencies:** U1 (references the marker as the anchor).

**Files:**
- `docs/adr/0014-migration-source-decommissioned.md` (create)

**Approach:** Follow the existing ADR shape (`#` title, a tight rationale body;
see `0010`–`0013`). Status: accepted 2026-06-18. Record that the old repo is now
a Decommissioned Migration Source; ordinary maintenance must not consult, import,
build against, or depend on it. State explicitly that `0014` supersedes the
operational posture of `0010` (which remains the historical record, unedited).
Reference `docs/migration-source/README.md` as the anchor and
`docs/migration-source/audit/` as retained evidence. Keep "reference" used only
as a noun for evidence; avoid present-tense consult phrasing adjacent to the repo
name so the ADR itself stays green under the guard.

**Patterns to follow:** `docs/adr/0010-old-repository-is-migration-source-only.md`
and `docs/adr/0011-ls-crate-layout.md` for tone and length.

**Test scenarios:** `Test expectation: none -- decision record, no behavior.`

**Verification:** ADR `0014` exists with the accepted-2026-06-18 status, names the
supersession of `0010`'s posture, leaves `0010` unmodified (`git diff` shows no
change to `0010`), and links the marker + audit evidence.

### U3. Correct live-reference framing in active sources

**Goal:** Remove the two present-tense "we reference the old repo" framings from
active, non-test, non-evidence files so the posture is consistent and the guard
is green.

**Requirements:** R5, R6, R12.

**Dependencies:** none.

**Files:**
- `README.md` (modify — the "Standalone — and the role of `korea-broker-sdk-ls`"
  section, ~lines 50-63)
- `crates/ls-trackers/src/fetch.rs` (modify — the module-doc comment at line 3)

**Approach:**
- `README.md`: rewrite the role section from "a repository we reference to pull
  already-generated code…" (present-tense live reference) to the decommissioned
  posture — standalone, no build/runtime dependency, old repo retained only as
  cited audit evidence, new behavior belongs in the maintained surface. Keep it
  consistent with the marker (U1) and ADR (U2).
- `crates/ls-trackers/src/fetch.rs`: normalize the comment "The migration source
  (`~/dev/korea-broker-sdk-ls/scripts/fetch_ls_specs.py`) **is** the
  endpoint/retry/fallback reference" to past-tense attribution that names what was
  ported and drops the live `~/dev/...` path (e.g., framed as "this adapter was
  ported from the migration source's fetch script; its fixed `MIN_TR_COUNT` floor
  was replaced here by …"). Preserve the technical content (the `MIN_TR_COUNT`
  replacement note); only the framing and the path change.
- Do **not** touch `Provenance:` lines in `docs/design/`/`docs/operations/`,
  anything under `docs/plans/`/`docs/brainstorms/`, `CONTEXT.md`, ADR `0010`, or
  the `crates/ls-core/src/lib.rs:3` "Ported from …" comment (already attribution).

**Patterns to follow:** Past-tense attribution shape already used by
`crates/ls-core/src/lib.rs:3` ("Ported from the Migration Source …").

**Test scenarios:** Enforced by U4 — after this unit, the calibration test
(U4 scenarios) must show `fetch.rs` no longer trips, and AE1/AE2 cover the
path-vs-provenance distinction. `Test expectation: none in this unit -- framing
change; guard behavior verified in U4.`

**Verification:** `grep` over active surface shows no live filesystem path to the
old repo outside excluded files; `README.md` role section reads as decommissioned;
`fetch.rs` comment is past-tense with no `~/dev/...` path; `git diff` shows no
change to `Provenance:` lines, `CONTEXT.md`, ADR `0010`, or `ls-core/src/lib.rs`.

### U4. Regression guard in `decommission_audit.rs`

**Goal:** Add a test that keeps the boundary closed — marker present, no live
old-source dependency in the active surface — without self-tripping on retained
evidence or the audit machinery.

**Requirements:** R7, R8, R9, R10, R11.

**Dependencies:** U1 (marker must exist for the real-tree assertion), U3 (live
framing removed for the real-tree assertion to pass).

**Files:**
- `crates/ls-trackers/tests/decommission_audit.rs` (modify — add the guard +
  fixtures; do not alter the existing record-consistency gate logic)

**Approach:** Add pure predicates over `(path, line)`:
1. `in_scan_domain(path)` — allowlist `crates/**` (path has no `/tests/` segment) +
   `docs/**` + root `*.md`, minus the one R9-extending exclusion that the allowlist
   doesn't already cover: `docs/migration-source-extraction-ledger.md` (KTD-4). The
   `/tests/`-segment exclusion is load-bearing — it keeps the guard file's own
   fixtures out of scope.
2. `live_path_hit(line)` — matches `../korea-broker-sdk-ls`, `/korea-broker-sdk-ls`,
   `/Users/.../korea-broker-sdk-ls`, `~/dev/korea-broker-sdk-ls` (KTD-3).
3. `consult_hit(line)` — line contains `korea-broker-sdk-ls` AND a **whole-word**
   phrase from the calibrated consult list (KTD-3: word-boundary, not substring);
   `Provenance:` lines are skipped first.

Enumerate via a new `git ls-files` call (reuse `workspace_root()`/`is_git_tracked()`),
restrict to the allowlist, scan each line through the predicates, and assert
`docs/migration-source/README.md` exists. Prove the predicates against in-test
fixture line/file sets first (the file's existing discipline), then assert over
the real tree.

**Execution note:** Prove the path/consult/exclusion predicates against in-test
fixtures before asserting over the real tree — the negative acceptance cases (a
bad path fails; a deleted marker fails) cannot mutate the real tree, so they must
be fixture-driven, mirroring the existing gate-invariant fixtures.

**Patterns to follow:** Existing `decommission_audit.rs` — `workspace_root()`,
`is_git_tracked()`, `real_resolver`, and the `fixtures_*` / `gate_*` in-test
fixture tests.

**Technical design (directional):** predicate signatures, not implementation —
`fn in_scan_domain(rel: &str) -> bool`, `fn live_path_hit(line: &str) -> bool`,
`fn consult_hit(line: &str) -> bool`. Real-tree test composes them over
`git ls-files`; fixture tests feed crafted `(rel, line)` pairs.

**Test scenarios:**
- Covers AE1. Fixture: a non-test `crates/` line `let p = "../korea-broker-sdk-ls/x";`
  → `live_path_hit` true → guard fails. Remove the path → guard passes.
- Covers AE2. Fixture: a `docs/design/` line
  `(Provenance: korea-broker-sdk-ls/docs/DIAGNOSTICS_CONTRACT.md)` → skipped as a
  `Provenance:` line → guard passes.
- Covers AE3. Fixture: a path under `docs/migration-source/audit/` naming
  old-source docs → excluded by scan domain → guard passes.
- Covers AE4. Fixture/real: `docs/migration-source/README.md` absent → existence
  assertion fails.
- Calibration (real tree): the ledger line holding `/Users/mini/dev/korea-broker-sdk-ls`
  is excluded (KTD-4) → guard passes.
- Calibration (real tree): ADR `0010`'s "reference documentation" and CONTEXT.md's
  "should not need to consult **it**" stay green (no repo name + consult phrase on
  one line) → guard passes.
- Calibration (real tree): extraction-attribution prose in `docs/design/` /
  `docs/operations/` ("Extracted from …", bare `korea-broker-sdk-ls/docs/…`) stays
  green.
- Word-boundary fixture (the load-bearing precision case): the line
  `korea-broker-sdk-ls Migration Source to preserve production/readiness` (the real
  `release-readiness-and-residual-lessons.md:4`) → `consult_hit` **false** → guard
  passes. A substring matcher would fail here on `read` inside `readiness`; this
  fixture pins the word-boundary rule.
- Self-exclusion: the guard file `crates/ls-trackers/tests/decommission_audit.rs`
  — which holds the literal name and the `consult korea-broker-sdk-ls …` fixture —
  is out of scope (`/tests/` segment) → guard does not self-trip.
- Wrapped-Provenance fixture: a citation whose `(Provenance:` token and
  `korea-broker-sdk-ls/docs/…` path land on different physical lines → the
  continuation line is NOT caught by the per-line Provenance skip, but stays green
  via the bare-path exclusion (no leading slash) → guard passes.
- Calibration (real tree): `crates/ls-core/src/lib.rs:3` "Ported from …" stays
  green; `crates/ls-trackers/src/fetch.rs` is green only **after** U3.
- Consult-language positive: a fixture line `consult korea-broker-sdk-ls before
  changing retry behavior` → `consult_hit` true → guard fails (proves the
  non-path half of R8).
- Path-pattern coverage: fixtures for each of the four path forms in KTD-3 hit;
  a bare `korea-broker-sdk-ls/docs/x.md` line does **not** hit.

**Verification:** `cargo test -p ls-trackers` passes (existing gate validator +
new guard) (R10); `cargo test --workspace` passes (R11); the guard fails when the
marker is removed or a live path/consult line is introduced into the active
surface (proven via fixtures), and passes on the real tree after U1 + U3.

---

## Scope Boundaries

- Physically deleting or archiving the sibling `korea-broker-sdk-ls` repository —
  an external ops act, outside this repo's PR.
- Any new extraction from the old source. The audit is TRUSTWORTHY-GREEN; the only
  re-extraction was the audit's own re-extract step, already merged.
- Removing retained evidence: `docs/migration-source/tr-dependencies-2026-06-14.json`,
  the audit records/manifest/report under `docs/migration-source/audit/`, the
  frozen `docs/migration-source-extraction-ledger.md`, and `Provenance:` citations
  in design/operations docs.
- Editing `CONTEXT.md` or ADR `0010` (the `Decommissioned Migration Source` term
  is already current; `0010` is the historical record `0014` supersedes).
- The `crates/ls-core/src/lib.rs:3` "Ported from …" comment — attribution, left
  as-is.
- Changing what the existing gate validator recomputes — the new guard is
  additive, not a rewrite of the record-consistency logic.

---

## Risks & Dependencies

- **Phrase-matcher calibration is the main risk.** The consult-language check is a
  natural-language matcher; substring (vs. word-boundary) matching, an over-broad
  word list, or whole-file (vs. line + name-adjacency) matching would false-positive
  on attribution prose, CONTEXT.md, ADR `0010`, or the audit machinery. The
  concrete trap is `read` inside `readiness` (KTD-3). Mitigation: word-boundary
  matching (KTD-3) plus the U4 calibration fixtures pin every known near-line case
  green; the implementer runs the guard against the real tree and confirms only
  `README.md` and `fetch.rs` trip pre-correction, adjusting the word list or
  exclusions if any other active file false-positives.
- **The guard under-covers R8's "instruction to consult" half by design.** It is a
  tripwire for the calibrated phrasings, not a general detector. Two known gaps,
  accepted deliberately: (1) bare `reference` is excluded from the consult list
  (only the multi-word `use as reference` is in it), so the guard would *not* catch
  a future re-addition of `README.md`'s "a repository we reference to pull …"
  framing — review, not the guard, backstops that class. Broadening to bare
  `reference` is rejected because it would false-positive on ADR `0010`'s
  "reference documentation". (2) The agent-harness dirs are unscanned (KTD-4), so a
  live path or consult instruction added under `.claude/`, `.agents/`, or
  `.compound-engineering/` is invisible to the guard — see Open Questions.
- **Provenance-skipping is physical-line scoped, not logical-citation scoped.**
  Several real citations wrap across two lines (`(Provenance:` ends one line, the
  `korea-broker-sdk-ls/docs/…` path begins the next). Continuation lines are not
  recognized as Provenance lines; today they stay green only because the bare-path
  form (no leading slash) is excluded from `live_path_hit`. A future wrapped
  citation whose continuation lands on a `/Users/…`/`~/dev/…` path or a consult
  word would trip. The U4 wrapped-Provenance fixture pins the current behavior.
- **Dependency precondition is already satisfied:** the audit gate is
  TRUSTWORTHY-GREEN and recomputed in CI by the existing validator. This PR cites
  that verdict; it does not re-run the audit. (see origin: Dependencies/Assumptions)
- **The origin's "README is the only active doc" assumption was found incomplete.**
  Planning (guard calibration) revealed `crates/ls-trackers/src/fetch.rs:3` also
  carries present-tense live-reference framing, so R12 extends the origin's
  single-doc assumption. `README.md` (~lines 50-63) and `fetch.rs:3` are now the
  believed-complete set; the U4 real-tree calibration run is the check, and any
  further active occurrence falls under R5/R12's intent.

---

## Open Questions

- **Should the guard scan the agent-harness dirs (`.claude/`, `.agents/`,
  `.compound-engineering/`)?** As planned (KTD-4) the scan domain is the maintained
  product surface and the harness is a known residual gap — yet that is the most
  plausible place a future automation would hardcode `~/dev/korea-broker-sdk-ls` or
  an instruction to consult the old source, the exact reintroduction vector R8
  exists to stop. Extending coverage there (with whole-word matching and an
  exclusion for the audit-machinery files that legitimately name the old source) is
  a real option; the trade-off is added calibration surface against transient run
  state. Default if unresolved: leave as a documented gap and revisit if a future
  change adds product logic under those dirs.
