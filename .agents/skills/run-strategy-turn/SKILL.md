---
name: run-strategy-turn
description: Take one pre-authored ORB lever candidate through a full merit-bearing strategy-loop turn via the governed command — Phase-A diagnose, build + binary-fingerprint check, guarded flip, KEEP-rule verdict — with every gate reading appended to the TRIALS ledger. Use for a single candidate slug (e.g. "run-strategy-turn ratio-atr-tilt"). Runs non-interactively and state-driven; the command halts loudly at any red gate and the skill records the outcome. Not for authoring a candidate (that is the human/author step before this recipe) or for live-session governance (see the production-ladder plan).
---

Take **one** candidate through a full governed turn and record the outcome. This
is a state-driven recipe: it authors nothing about the strategy — the candidate
(pre-register + diagnostic + twin) is committed **before** this runs — and it
never softens a gate. It either completes the turn (the command emits KEEP /
REVERT) or it halts at a red gate (STOP / HELD), and in every case the turn is a
recorded, queryable event in the TRIALS ledger.

**Input:** one candidate slug (the `$ARGUMENTS`, e.g. `ratio-atr-tilt`) — the
name of a committed dir under `adapters/nautilus/lab/candidates/<slug>/`.

**Output (last line, machine-readable):** exactly the governed command's verdict
grammar, echoed verbatim —
`KEEP v<N> <hash>` / `REVERT <cause-code>` / `STOP <gate>` / `HELD <reason>`.

This skill is **non-interactive** — it asks no questions; it infers everything
from repo state and the command's verdict + exit code. Human judgment sits
**before** it (authoring the candidate, pre-register, and diagnostics) and
**after** it (reviewing artifacts, writing the TURN-LOG/memory capture,
committing). The command never commits to git; neither does this skill without
the author's review step.

Boundary: this skill owns exactly one governed turn and its record/commit. It
does **not** author candidates, set gate thresholds (frozen per-candidate in the
pre-register), decide a deflated KEEP margin (blocked until separately
pre-registered, KD2), or run any live-session governance (production-ladder
plan).

All work is offline and deterministic — no gateway, no credentials, no market
window. The commit gate is `make adapter-check` (from `adapters/nautilus`).

## 0. Preflight (incident-traceable — run before touching anything)

Read `references/preflight-checklist.md` and confirm every line. Each line cites
the documented incident or solution doc that created it; a line you cannot
confirm is a **STOP**, not a proceed. **Standing rule:** any new strategy-loop
`docs/solutions/workflow-issues/*` doc must land a new checklist line in the same
PR that documents it — the checklist is the incident memory, and it decays if a
new gotcha ships without one.

## 1. Verify the candidate is committed and frozen

- The dir `adapters/nautilus/lab/candidates/<slug>/` exists and holds
  `candidate.json` plus its declared diagnostic + twin.
- Its frozen inputs are **git-clean** (the command refuses a dirty frozen input
  with `FrozenInputDirty`, exit 40) — `git status --porcelain -- <those files>`
  is empty. Commit them first if not; commit history is the freeze evidence R2
  relies on.
- Bail as `HELD <slug> — candidate not committed/clean` if either fails.

## 2. Run the governed turn (one invocation)

From `adapters/nautilus`, with the per-lane data home:

```bash
LS_DATA_HOME=<data-home> \
LS_TURN_CANDIDATE=<slug> \
LS_TURN_PARAM=<flip-param> LS_TURN_VALUE=<flip-value> \
  cargo run --release -p nautilus-ls-lab --bin lab-research -- turn governed
```

For a **code turn** (a changed `orb.rs`, no swept param), drop `LS_TURN_PARAM` /
`LS_TURN_VALUE` and set `LS_TURN_CODE_BUMP=1` instead — the native version-bump
path (no manual seed-and-rerun; see the superseded solution doc).

The one invocation runs: parent fingerprint self-check → diagnose (or reuse a
committed GO) → foreground build → built-binary fingerprint check → flip in the
fresh child → KEEP-rule verdict, halting at the first red gate with a distinct
exit code. Do **not** re-run stages by hand; do **not** edit the pre-register
after a GO (the flip refuses with `PreRegisterHashMismatch`, exit 21).

## 3. Interpret the verdict (the command's last line)

Read the **last output line** and the exit code — never re-derive the verdict:

- `KEEP v<N> <hash>` (exit 0) — the flip improved the size-invariant
  return-on-risk crux with risk-cap dominance held. A real edge; the head advances.
- `REVERT <cause-code>` (exit 0) — a completed evaluation that did not clear the
  KEEP bar. A **valid recorded outcome**, not a failure. Cause codes are the
  shared grammar (`inverted-signal`, `collinear`, `coverage-cull`,
  `winner-cutting`, `ror-negative`; append-only).
- `STOP <gate>` (distinct non-zero) — a Phase-A gate stopped the turn
  (twin-mismatch / threshold-fail / script-failure). No build ran; the trial is
  still recorded.
- `HELD <reason>` (distinct non-zero) — an infrastructure or guard halt (stale
  binary, build failure, ungoverned flip, hash mismatch, …). Fix the named cause;
  never work around the gate.

## 4. Capture the turn (author review — TURN-LOG + memory)

- Append a TURN-LOG entry using the repeated 8-part template
  (`adapters/nautilus/lab/TURN-LOG.md`): the candidate, its Phase-A readings, the
  verdict, the bind prediction/validation, and the registry-state line.
- If the turn changed the head (KEEP), or taught a durable gotcha, write/update a
  memory note per the project's memory discipline.
- The TRIALS ledger already recorded each gate reading + the flip look (the
  command appends them; a hand-run gate reading lands its ledger record in the
  same commit as its artifacts — R10/R14).

## 5. Gate and commit

- `make adapter-check` from `adapters/nautilus` — green, no skipped failures.
- Commit the turn: the candidate's frozen inputs (if newly authored), the
  `gate-verdict.json`, the TRIALS ledger append, and the TURN-LOG/memory capture,
  in one focused commit. The command never commits; the author does, after review.

## Verdict grammar (shared with the command — echo it verbatim)

`KEEP v<N> <hash>` | `REVERT <cause-code>` | `STOP <gate>` | `HELD <reason>`.

The skill's last line **is** the command's last line — byte-for-byte. Do not
paraphrase, re-rank, or re-decide it (the anchor-on-decider convention: the fresh
child is the decider; every layer above it is transport).
