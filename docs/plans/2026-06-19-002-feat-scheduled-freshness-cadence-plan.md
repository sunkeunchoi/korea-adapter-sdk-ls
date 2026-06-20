---
title: "feat: Scheduled freshness cadence — the repo's first automation"
date: 2026-06-19
type: feat
status: planned
origin: docs/brainstorms/2026-06-19-freshness-cadence-checkpoint-requirements.md
---

# feat: Scheduled Freshness Cadence — the Repo's First Automation

> Origin: `docs/brainstorms/2026-06-19-freshness-cadence-checkpoint-requirements.md`
> (problem frame, R1–R10, AE1–AE11, scope boundaries carried forward below).

## Summary

Give evidence freshness a cadence that does not depend on anyone remembering. A
scheduled, non-gating GitHub Actions workflow — the repository's **first**
automation under `.github/workflows/` — runs `make freshness-check` monthly and
maintains a single rolling **"Evidence freshness status"** issue: opened/updated
when Recommended TRs are stale, cleared when none are. The check touches no LS API
and needs no LS credentials. The schedule *is* the trigger.

Three things the brainstorm deferred to planning are now resolved:

1. **`ls-trackers freshness check` gains a `--json` mode** (approved scope addition)
   so the workflow consumes a stable machine-readable contract instead of scraping
   human-formatted stdout. This is the foundational unit.
2. **Watcher liveness (R9) is handled in-workflow** — an `if: failure()` step
   comments-and-@mentions a maintainer on a build/tooling failure (exit 2). Research
   corrected the brainstorm's premise: GitHub has **no** consecutive-failure
   auto-disable; the real silent-death vectors (60-day-inactivity disable on public
   repos, dropped runs) are only catchable by an external monitor, which is
   **deferred** as documented follow-up with the residual gap stated honestly.
3. **Build caching is skipped** — at monthly cadence GitHub's 7-day cache eviction
   guarantees a miss, so `Swatinem/rust-cache` would be pure overhead.

`api-drift` (network-touching, R19) and `spec-doc` (network-free, scope choice)
stay operator-run; a new `make maintenance-sweep` aggregates *those two* (freshness
stays standalone — its cadence guarantee is the schedule). The runbook's
`Checkpoint-host gap (U7 / R19)` note is updated to record the split.

---

## Problem Frame

PR #21 made the 90-day backstop enforced *when run*, but `make freshness-check` is
operator-invoked — a recommendation only surfaces as stale if a maintainer
remembers to run it. A documented operator checkpoint alone only relocates the
dependency ("forgot to run the check" → "forgot to run the monthly sweep"). The
control stays memory-dependent and its own lapse stays invisible.

Freshness is the one check that can be put on a timer without cost: it is
**network-free** (honors ADR 0009's network-free-CI posture) and **non-gating**
(`Severity::Evidence` sits below `Maintenance`, so `gates_for` never trips — honors
the advisory posture from PR #21). The standing R19 decision — "no cron/CI
scheduling; operator-run" — is **scoped to the network-touching `api-drift`
fetch**, not a blanket prohibition; the runbook itself anticipates "when a scheduled
review is introduced, fold the step in." Scheduling freshness therefore operates
*outside* R19's intent rather than overturning it (see origin: Key Decisions).

---

## Requirements Traceability

| ID | Requirement (abbreviated) | Implemented by |
|----|---------------------------|----------------|
| R1 | Scheduled workflow runs `make freshness-check` monthly; network-free; compiles `ls-trackers` | U3 |
| R2 | Non-gating: stale evidence never fails the job; failure = tooling/build/infra error | U1, U3 |
| R3 | Single rolling issue; idempotent; **never silent while stale** (resurfaces every run, even after manual close) | U2 |
| R4 | Rolling issue is **not** an `SDK work item`; escalation stays human (ADR 0013) | U2 |
| R5 | Credential-free: only `GITHUB_TOKEN` + `issues: write`; no LS access | U3 |
| R6 | `api-drift` + `spec-doc` stay operator-run at the checkpoint | U4, U5 |
| R7 | Manual sweep covers the operator-run checks; freshness standalone, **not** bundled | U4 |
| R8 | Runbook `Checkpoint-host gap (U7 / R19)` note updated; R19 clarified | U5 |
| R9 | Watcher's own failure visible (exit 2 / build error → notification); silent-disablement vectors documented as residual gap | U3 (+ deferred external heartbeat) |
| R10 | New staleness produces an actual notification (comment/assignment/mention), not a silent body edit | U2 |

---

## Key Technical Decisions

**KTD1 — Add a `--json` contract to `ls-trackers freshness check` rather than
scrape stdout.** The tool prints human text only and exits `0` whether stale or
fresh, so neither the exit code nor a stable parse target exists today. A `--json`
mode emits a **dedicated serialization DTO** (not a bare derive on the domain
struct) — Rust-first per ADR 0009, unit-testable, and decouples the workflow from
prose formatting. The DTO's field names *are* the workflow contract, so they are
pinned (see U1): the in-memory `FreshnessReport` today carries only `findings`,
`recommended_count`, and `unparseable`, so the DTO must rename `findings`→`stale`,
materialize `has_errors`, and thread in `as_of` + `window_days` (neither is stored
on the report). *(User-approved scope addition beyond the brainstorm.)*

**KTD2 — Exit-code discipline is the gating contract.** `--json` does **not** change
exit semantics: exit `0` for stale-or-fresh, exit `2` only on metadata load/parse
error or unparseable `last_reviewed`. The workflow reads *staleness* from the JSON
`stale[]` array and treats *exit `2`* as the watcher-liveness failure (R9). Stale
evidence therefore never fails the job (R2/AE4).

**KTD3 — Issue identity is a dedicated stable label, not the title.** The rolling
issue is found via `--label freshness-status --state all`. Titles carry dates/counts
and are fragile; a label is a durable key and search across `--state all` finds a
previously-closed issue for reopen. The label and a fixed title prefix (e.g.
`Evidence freshness status`) are deliberately **distinct** from the `[SDK work
item]:` prefix and the `queue:*`/`source:*`/`class:*`/`support:*`/`gate:*` Work
Queue label taxonomy (R4/AE5, ADR 0013).

**KTD4 — Notify only on transition, edit body every run.** GitHub does **not**
notify on issue *body* edits (research-confirmed). So: the body is rewritten every
run as a silent dashboard; a notifying **comment that @mentions the maintainer** is
posted only on a transition *into* staleness (fresh→stale, reopen after close, or a
*newly*-stale TR joining an already-open issue). Same-set re-runs stay silent to
avoid per-run spam (R10/AE11).

**KTD5 — "Never silent while stale" is owned by recomputation, not issue state.**
Every run recomputes from scratch; whenever `stale[]` is non-empty the workflow
*ensures the issue is open* regardless of prior state — including a maintainer's
manual close (reopen + notifying comment). The bot never trusts the issue's
open/closed flag as truth (R3/AE9).

**KTD6 — In-workflow liveness now; external heartbeat deferred.** An `if:
failure()` step posts an @mentioning comment so a run-and-fail (exit 2 / build
error) reaches a human, since GitHub's built-in failure email goes to only one
person (the last cron editor) and is account-deletion-fragile. The residual gap is
the three silent-*non-run* vectors — 60-day-inactivity disable (public repos),
dropped runs under load, and a malformed-YAML edit that disables the schedule —
none of which emit a run or failure event. `actionlint` guards the third; the first
two are *only* catchable by an external dead-man's-switch, **deferred** with the gap
documented (R9; see Risks and Deferred Work).

**KTD7 — Monthly cadence, off-peak, no cache.** `cron: '17 7 1 * *'` (1st of month,
07:17 UTC — dodges the top-of-hour high-load window) plus `workflow_dispatch:` for
manual test/recovery. Monthly is ample inside the 90-day backstop (~60 days lead
before a lapse, per origin R1). Caching is skipped: a once-a-month job always finds
its cache evicted (7-day idle limit ≪ 30-day interval).

---

## High-Level Technical Design

**Directional guidance, not implementation specification.**

### Pipeline (per scheduled run)

```
schedule (monthly) ─▶ install toolchain ─▶ cargo build/run
        └─▶ ls-trackers freshness check --json ─▶ findings.json
              ANY step fails (toolchain/build/compile, or check exit 2)
                              └──────────────────────────▶ [R9] if: failure() ─▶ comment + @mention
                              │ all steps succeed (exit 0, stale-or-fresh)
                              ▼
                    update-freshness-issue (gh) ──▶ rolling "Evidence freshness status" issue
```

### Rolling-issue decision logic (the core of U2)

```mermaid
flowchart TD
    A[Run freshness check --json] --> B{stale[] empty?}
    B -- "yes (all fresh)" --> C{existing open issue?}
    C -- yes --> D[Close issue + 'all clear' comment]
    C -- no --> E[No-op]
    B -- "no (stale)" --> F{find issue --state all by label}
    F -- "none exists" --> G[Create issue: body + comment @mention]
    F -- "open, same stale set" --> H[Edit body only — silent dashboard]
    F -- "open, NEW stale TR added" --> I[Edit body + comment @mention]
    F -- "closed (incl. manual close)" --> J[Reopen + edit body + comment @mention]
```

The three notifying paths (G, I, J) satisfy R10/AE11; path J satisfies the
never-silent-while-stale invariant after a manual close (R3/AE9); path D/E satisfy
AE2; the single-issue lookup-by-label guarantees idempotency (AE3).

---

## Output Structure

New files this plan introduces (per-unit `Files:` remain authoritative):

```
.github/
  workflows/
    freshness-cadence.yml          # U3 — scheduled trigger, build, run, liveness step
  scripts/
    update-freshness-issue.sh      # U2 — idempotent rolling-issue upsert (gh CLI)
crates/ls-trackers/src/
    freshness.rs / cli.rs          # U1 — --json output (modified)
Makefile                           # U4 — maintenance-sweep target (modified)
docs/MAINTENANCE_RUNBOOK.md        # U5 — Checkpoint-host gap note (modified)
```

---

## Implementation Units

### U1. `--json` machine-readable output for `freshness check`

**Goal:** Emit a stable JSON contract from `ls-trackers freshness check --json`
serializing the existing `FreshnessReport`, without changing exit-code semantics.

**Requirements:** R1, R2 (exit discipline). **Dependencies:** none.

**Files:**
- `crates/ls-trackers/src/cli.rs` — thread `--json` through **three** points (not just the printer): `parse_freshness` currently errors on any token after `check`, so it must accept the flag; `Command::FreshnessCheck` is a fieldless variant today and must carry `{ json: bool }`; the dispatch site branches the printer between text and JSON. Patching only `print_freshness_report` leaves the flag rejected at parse time.
- `crates/ls-trackers/src/freshness.rs` — derive/implement serialization on `FreshnessReport` / `FreshnessFinding`; tests.
- `crates/ls-metadata/src/freshness.rs` — only if the `FreshnessState`/finding types need a `Serialize` derive; prefer keeping serialization in `ls-trackers`.
- `crates/ls-trackers/Cargo.toml` — add `serde`/`serde_json` if not already present (verify at implementation time; api-drift normalized JSON suggests serde is already in the workspace).

**Approach:** Add a `--json` branch that writes one JSON object to stdout via a
**dedicated DTO**, *not* a `#[derive(Serialize)]` on `FreshnessReport` — the domain
struct's field names and shape do not match the contract below, and the workflow
(`jq .stale[].tr_code`, U2's marker diff) breaks silently if a bare derive emits
`"findings"` instead of `"stale"`. The DTO performs four explicit transformations:

1. **rename** the report's `findings` → `stale`;
2. **materialize** `has_errors` (it is a *method* on the report, not a field);
3. **thread in** `as_of` and `window_days` (neither is stored on the report today —
   `as_of` is a discarded parameter to the evaluator, `window_days` is the
   `DEFAULT_WINDOW_DAYS` constant; pass both into the DTO at construction);
4. **serialize** `FreshnessFinding.severity` (a `Severity` enum) to its lowercase
   string (`"evidence"`) — reuse the existing `Severity` `Serialize` derive
   (`#[serde(rename_all = "snake_case")]` already emits `"evidence"`); no new impl.

Pinned contract schema (these field names are load-bearing for U2):

```json
{
  "as_of": "2026-06-19",
  "window_days": 90,
  "recommended_count": 6,
  "has_errors": false,
  "stale": [
    { "tr_code": "t1102", "last_reviewed": "2026-03-01", "age_days": 95, "severity": "evidence" }
  ],
  "unparseable": ["token"]
}
```

Keep `stale[]` in the existing deterministic TR-code (BTreeMap) order. Exit mapping
is **unchanged** (`Exit::Ok` for stale-or-fresh, `Exit::Error` when `has_errors` or
metadata load fails). The text path stays the default; `--json` is opt-in. `serde`
/ `serde_json` are already workspace deps (see `cli.rs` `write_json`), so no new
dependency is needed — verify at implementation.

**Patterns to follow:** Existing `print_freshness_report` (`cli.rs`); colocated
`#[cfg(test)] mod tests`; inject the as-of date rather than reading the clock (the
midnight-flake discipline already used in `freshness.rs`). Mirror any existing
`--json`/serde usage in the `api-drift` normalized path.

**Test scenarios** (`crates/ls-trackers/src/freshness.rs` / `cli.rs` tests):
- All-fresh fixture → `--json` emits `stale: []`, `has_errors: false`, correct
  `recommended_count`; process exits `0`. **Covers AE4.**
- Two stale TRs (injected as-of date) → `stale[]` has exactly those two entries in
  deterministic TR-code order with correct `age_days`/`last_reviewed`; exits `0`.
  **Covers AE1, AE4.**
- Unparseable `last_reviewed` → `has_errors: true`, offending code in
  `unparseable[]`; process exits `2`. **Covers AE10** (this is the tooling-error
  signal the workflow surfaces).
- JSON is valid, single-object, parseable by `jq` (schema-stability assertion).
- **Field-name pinning:** the emitted object has keys exactly `as_of`,
  `window_days`, `recommended_count`, `has_errors`, `stale`, `unparseable`; each
  `stale[]` entry has `tr_code`, `last_reviewed`, `age_days`, `severity`. A bare
  derive on `FreshnessReport` would emit `findings` and fail this — the test guards
  the workflow contract.
- Text output for the default (no-flag) path is byte-identical to pre-change
  behavior (no regression).

**Verification:** `cargo run -q -p ls-trackers -- freshness check --json | jq .`
yields the documented object; `cargo test -p ls-trackers` green; clippy clean.

---

### U2. Idempotent rolling-issue upsert script

**Goal:** Given the freshness findings (U1 JSON) and the current state of the
`freshness-status` issue, drive the issue to the correct state per the decision
logic — idempotent, never-silent-while-stale, notify-on-transition, distinct from
the Work Queue.

**Requirements:** R3, R4, R10. **Dependencies:** U1.

**Files:**
- `.github/scripts/update-freshness-issue.sh` — the upsert logic, `gh` CLI based,
  with a `--dry-run` mode that prints the chosen action instead of calling `gh`.

**Approach:** Read `findings.json` (from U1). Resolve the existing issue via
`gh issue list --label freshness-status --state all --json number,state,title`.
Implement the decision tree from the HTD:
- **All fresh + open issue exists** → `gh issue close` with an "all clear" comment.
- **All fresh + no open issue** → no-op.
- **Stale + no issue** → `gh issue create --label freshness-status` (fixed title,
  body listing stale TRs) then a notifying `gh issue comment` @mentioning the
  maintainer.
- **Stale + open issue, same stale set** → `gh issue edit --body-file` only (silent
  dashboard refresh).
- **Stale + open issue, a newly-stale TR present** → edit body **and** notifying
  comment.
- **Stale + closed issue** (incl. manual close) → `gh issue reopen` + edit body +
  notifying comment.

**Diff rule (pin this — it decides notify vs. silent):** parse the prior stale TR
codes from the issue body's machine-readable marker block, then **notify iff
`current_stale \ prior_stale` is non-empty** (at least one TR is newly stale);
otherwise edit the body silently. This correctly handles the *shrinking-but-still-
stale* set (e.g. `{t1102, t8412}` → `{t1102}`): no new staleness, so silent edit,
not a notify and not a no-op.

**Missing/garbled marker fallback (pin this):** if the marker block is absent or
unparseable (first stale run ever, or a human edited the body), treat the prior set
as **empty** — every current stale TR reads as newly-stale → notify once. **Never
error on a bad marker** (erroring would trip the R9 failure path on a non-failure).

**Label bootstrap (first-run break):** `gh issue create --label freshness-status`
**fails if the label does not yet exist**, which would trip R9 on the inaugural
stale run. Ensure the label exists first — a guarded `gh label create
freshness-status --color … || true` step (idempotent) at the top of the script (or
a committed label definition). This is an explicit step, not an assumption.

Maintainer handle(s) come from a workflow input/repo variable (not a hardcoded
secret). The issue body carries the machine-readable marker block (e.g. an HTML
comment listing the stale TR codes) so the next run can diff without ambiguity. The
issue **never** uses the `[SDK work item]:` prefix or any `queue:*`/`source:*`/
`class:*`/`support:*`/`gate:*` label (R4).

**Patterns to follow:** `.github/ISSUE_TEMPLATE/sdk_work_item.yml` and
`docs/maintenance-labels.md` — to know exactly which titles/labels to **avoid**.
`gh` idempotent-upsert pattern (find-by-label, `--state all`).

**Execution note:** Separate the **pure decision core** (inputs: parsed
`findings.json` + parsed issue-state incl. the prior-stale set from the marker;
output: a chosen action) from the `gh` I/O, so the marker parse is part of the
testable core, not done inline in a `gh` call. Build the core first and exercise it
via `--dry-run` with mocked inputs before wiring real `gh` — the gating logic is the
risk, not the API call.

**Test scenarios** (dry-run assertions with mocked inputs; the script prints the
chosen action):
- Stale set `{t1102, token}`, no existing issue → action = *create + notify*; body
  lists both TRs. **Covers AE1.**
- All fresh, an open issue exists → action = *close + all-clear comment*; no new
  issue created. **Covers AE2.**
- Stale, an open issue with the *same* stale set → action = *edit body only* (no
  comment). **Covers AE3** (single issue, no duplicate, no spam).
- Stale, the issue was **closed** (simulating a manual close) → action = *reopen +
  notify*. **Covers AE9** (never silent while stale after manual close).
- Stale `{t1102}` already open, now `{t1102, t8412}` → action = *edit body + notify*
  (newly-stale TR triggers a real notification). **Covers AE11.**
- **Shrinking-but-still-stale:** `{t1102, t8412}` open, now `{t1102}` → action =
  *edit body only* (no new staleness, no notify, not a close). Guards the diff rule.
- **Missing/garbled marker:** stale input against a body whose marker block is absent
  or malformed → prior set treated as empty → action = *edit body + notify*; the
  script does **not** error (would otherwise trip the R9 failure path).
- **First-run label bootstrap:** stale input against a repo where the
  `freshness-status` label does not yet exist → the create path succeeds (label
  ensured first), no error.
- Generated body/labels contain no `[SDK work item]:` prefix and none of the Work
  Queue labels. **Covers AE5** (stays distinct from the human-curated queue).
- Idempotency: running the same stale input twice yields *create* then *edit-only*,
  never two `create` calls. **Covers AE3.**

**Verification:** `--dry-run` produces the expected action line for each scenario; a
live `workflow_dispatch` against a scratch issue walks create → edit → close →
reopen and never duplicates.

---

### U3. Scheduled workflow (trigger, build, run, liveness)

**Goal:** The `.github/workflows/freshness-cadence.yml` file — the repo's first
automation — schedules the monthly run, builds `ls-trackers` network-free, runs the
`--json` check, invokes the U2 script, and surfaces its own failure (R9).

**Requirements:** R1, R2, R5, R9. **Dependencies:** U1, U2.

**Files:**
- `.github/workflows/freshness-cadence.yml`

**Approach:**
- **Trigger:** `schedule: - cron: '17 7 1 * *'` plus `workflow_dispatch:`.
- **Permissions (job-level, minimal):** `contents: read`, `issues: write`. No other
  scope, no LS secrets (R5/AE7).
- **Steps:** checkout → install toolchain (`dtolnay/rust-toolchain@stable` — there
  is **no** `rust-toolchain.toml`, so the workflow must pick one) → invoke the JSON
  form of the check **directly**: `cargo run -q -p ls-trackers -- freshness check
  --json > findings.json`. (The plain `make freshness-check` target emits *text* and
  is unchanged — it stays the operator-facing command; the workflow consumes the
  same check's `--json` output rather than a make passthrough, so no new make target
  is needed. The Summary/R1 "runs the freshness check" framing refers to this same
  evaluation.) → run `.github/scripts/update-freshness-issue.sh` with
  `GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}` and the maintainer handle from a repo
  variable.
- **No build cache** (KTD7) — monthly cadence guarantees a miss.
- **Liveness (R9):** a final step `if: failure()` posts an @mentioning comment
  (failure = exit 2 from the check, or any build/tooling/infra error) linking the
  run URL, so the watcher's own death-by-failure is visible to a human, not just the
  single-recipient built-in email. The residual silent-disablement gap is documented
  in U5 + Risks, not solved here.

**Patterns to follow:** None local (first workflow) — follow the
framework-docs-researcher findings: explicit `permissions` block, off-peak cron,
`workflow_dispatch` companion, `GH_TOKEN` env for `gh`.

**Test scenarios** (verified via `workflow_dispatch`, not unit tests — this is YAML
glue):
- Manual dispatch with all TRs fresh → job exits success; no issue created/updated
  beyond the all-clear path. **Covers AE2.**
- Manual dispatch with a forced-stale fixture → job exits **success** (stale ≠
  failure) and the issue is opened/updated. **Covers AE1, AE4.**
- A deliberately broken build / forced exit-2 → job fails **and** the `if: failure()`
  step posts an @mentioning comment with the run link. **Covers AE10.**
- Workflow run uses only `GITHUB_TOKEN`; no LS secret is referenced anywhere in the
  file. **Covers AE7.**
- `Test expectation: none` for the cron-timing itself — schedule firing is GitHub
  infrastructure, exercised via `workflow_dispatch`.

**Verification:** `workflow_dispatch` run is green on fresh and on stale; the
failure path posts a notifying comment; `actionlint` (or GitHub's own validation)
passes on the YAML.

---

### U4. Manual maintenance sweep target

**Goal:** A `make maintenance-sweep` aggregate that runs the **operator-run** checks
(`api-drift-check` + `spec-doc-check`) in one command; freshness stays standalone and
is **not** bundled (its cadence guarantee is the schedule, R7).

**Requirements:** R6, R7. **Dependencies:** none (independent of U1–U3).

**Files:**
- `Makefile` — add `maintenance-sweep` target.

**Approach:** A `.PHONY` target invoking `api-drift-check` then `spec-doc-check`
sequentially. Define the aggregate's exit semantics (the brainstorm's deferred
question): the sweep surfaces each check's outcome and exits non-zero if either
gates (`api-drift` exit 1) or errors (exit 2), so an operator sees a clear pass/fail;
`spec-doc` is advisory (exit 0/2). Freshness is **deliberately excluded** — a comment
in the target states "freshness has a scheduled trigger; run `make freshness-check`
standalone." Keep freshness runnable standalone for offline convenience.

**Patterns to follow:** Existing one-line `cargo run` recipes for the three check
targets in `Makefile`; the header-comment convention explaining network posture.

**Test scenarios:**
- `Test expectation: none — Makefile target, no behavioral logic.` Verified by
  invocation: `make maintenance-sweep` runs both operator-run checks and excludes
  freshness. **Covers AE6.**

**Verification:** `make maintenance-sweep` runs `api-drift-check` + `spec-doc-check`
(not freshness); exit code reflects the worst outcome of the two.

---

### U5. Runbook and documentation update

**Goal:** Update the runbook to record that freshness now has a scheduled trigger
while `api-drift`/`spec-doc` remain operator-run, and clarify that R19 is scoped to
the network-touching check. Document the new workflow, the sweep, and the R9
residual gap honestly.

**Requirements:** R6, R8, R9 (documented gap). **Dependencies:** U3, U4 (so docs
describe what exists).

**Files:**
- `docs/MAINTENANCE_RUNBOOK.md` — rewrite the `Checkpoint-host gap (U7 / R19)` note;
  add a short "Scheduled freshness cadence" subsection; update the maintenance-sweep
  reference.

**Approach:** Rewrite the `Checkpoint-host gap (U7 / R19)` note to state: freshness's
cadence gap is now closed by a scheduled workflow (network-free, non-gating);
`api-drift` stays operator-run under R19 (no live fetch on a timer); `spec-doc` stays
operator-run this increment as a scope choice (not an R19 constraint). Add a brief
subsection describing `freshness-cadence.yml`: monthly cron, the rolling
`freshness-status` issue, that it is **not** a Work Queue item and escalation stays
human (R4), credential-free posture, and the **complete R9 residual gap** — three
silent-death vectors are uncovered in-repo: (1) 60-day-inactivity disable (public
repos), (2) dropped runs under high load, and (3) a malformed-YAML / workflow-syntax
error landed by a later edit that disables the schedule with no run and no failure
event. `actionlint` on the workflow is the only in-repo guard against (3); an
external heartbeat (the only mechanism catching (1) and (2)) is a documented
follow-up. Update the maintenance-checkpoint prose to reference
`make maintenance-sweep`.

**Patterns to follow:** Existing runbook section voice and structure; the prior note
text (lines ~7–12) is the exact replace target.

**Test scenarios:**
- `Test expectation: none — documentation.` Review-verified against AE8: the note
  records the freshness-scheduled / api-drift+spec-doc-operator-run split and the R19
  scoping clarification. **Covers AE8.**

**Verification:** The `Checkpoint-host gap (U7 / R19)` note reflects the new split;
the new subsection documents the workflow, the human-escalation boundary, and the R9
residual gap; markdown links resolve.

---

## Scope Boundaries

### Deferred to Follow-Up Work
- **External heartbeat / dead-man's-switch for R9** — the only mechanism that
  catches silent disablement (60-day inactivity, public repos) and dropped runs.
  Deferred because it requires an external service and a heartbeat-URL secret,
  denting the credential-free posture; the residual gap is documented (KTD6, U5).
- **A new aggregate `make` target *vs.* a runbook checklist** for the sweep was a
  planning fork; resolved in favor of a `make maintenance-sweep` target (U4). The
  individual check targets remain regardless.

### Deferred for later (from origin)
- **Scheduling the live `api-drift` fetch** — R19 stands for it (credentials, rate
  limits, failure handling need separate justification).
- **Scheduling `spec-doc`** — network-free and advisory like freshness, a natural
  follow-up; deferred to keep scope tight, not by R19.
- **Change-driven (API Drift → evidence) invalidation** — the next increment; shares
  the `Severity::Evidence` surface.

### Outside this work's identity (from origin)
- **Gating or failing the build on stale evidence** — the advisory posture from PR
  #21 is preserved (R2).
- **Auto-filing `SDK work item` issues from the bot** — that bypasses the
  human-reviewed Maintenance Work Queue (R4/AE5).

---

## Risks and Mitigations

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| **Silent non-run (R9 residual):** 60-day-inactivity disable (public repos), dropped runs under load, **or** a malformed-YAML edit that disables the schedule — none emit a run or a failure event | Low while the repo stays active | Documented honestly (U5, full 3-vector inventory); in-workflow `if: failure()` comment covers run-and-fail; `actionlint` guards the YAML-syntax vector; external heartbeat (the only catch for inactivity/dropped) deferred as explicit follow-up. Monthly maintainer commits keep the timer alive in practice. |
| **Notification spam:** commenting every run annoys maintainers and trains them to ignore the issue | Medium if mishandled | Notify-on-transition only (KTD4); same-set re-runs edit the body silently. |
| **JSON↔text drift:** the `--json` schema and text path diverge | Low | U1 keeps both paths off the one `FreshnessReport`; a no-regression test pins the text output. |
| **Issue-state race / wrong-issue match:** label matches an unrelated issue, or a second issue is created | Low | Dedicated `freshness-status` label + `--state all` lookup; idempotency test (AE3); label deliberately disjoint from Work Queue taxonomy. |
| **Toolchain unpinned:** `dtolnay/rust-toolchain@stable` drifts and breaks the build | Low | A build break is exit-nonzero → surfaced by the R9 failure step (not silent); pin a version later if drift bites. |
| **CI/local filesystem-case divergence** (Linux runner vs macOS APFS) | Low | Freshness path reads committed metadata only; note carried from `docs/solutions/architecture-patterns/change-tracker-baseline-clean-self-diff.md`. |

---

## Deferred to Implementation

- Exact `serde` wiring in `ls-trackers` (whether `serde_json` is already a workspace
  dep, or needs adding) — verify against `Cargo.toml` at implementation time.
- The precise machine-readable marker block format embedded in the issue body for
  newly-stale diffing (HTML comment vs. fenced block) — an implementation detail of
  U2, chosen when writing the script.
- Whether the maintainer handle is a repo variable, workflow input, or a `CODEOWNERS`
  lookup — resolve at U3 implementation; keep it out of secrets.
- The exact "all clear" comment vs. plain-close behavior on the cleared path — both
  satisfy AE2; pick during U2.

---

## Acceptance Examples → Coverage

| AE | Assertion | Unit / Test |
|----|-----------|-------------|
| AE1 | Stale → issue opened | U1 (json), U2 (create), U3 (dispatch) |
| AE2 | All fresh → issue cleared | U2 (close/all-clear), U3 |
| AE3 | Idempotent across runs | U2 (edit-only, no duplicate) |
| AE4 | Non-gating (stale exits 0) | U1 (exit), U3 (dispatch green on stale) |
| AE5 | Escalation stays human | U2 (no Work Queue prefix/labels) |
| AE6 | Operator sweep covers operator-run checks | U4 |
| AE7 | Credential-free | U3 (only `GITHUB_TOKEN`) |
| AE8 | Runbook updated | U5 |
| AE9 | Still-stale resurfaces after manual close | U2 (reopen + notify) |
| AE10 | Watcher's own failure visible | U1 (exit 2), U3 (`if: failure()` comment) |
| AE11 | New staleness notifies | U2 (transition comment + @mention) |

---

## Sources and Research

- **Origin requirements:** `docs/brainstorms/2026-06-19-freshness-cadence-checkpoint-requirements.md`.
- **Freshness evaluator (PR #21):** `docs/plans/2026-06-19-001-feat-evidence-freshness-evaluator-plan.md`; `crates/ls-trackers/src/freshness.rs`, `crates/ls-metadata/src/freshness.rs`, `crates/ls-trackers/src/cli.rs` (`print_freshness_report`, `freshness_exit_for`, `enum Exit`). Stable stdout format and exit-0-on-stale confirmed.
- **R19 source:** `docs/plans/2026-06-16-002-feat-api-drift-real-fetch-plan.md` — "do not wire scheduling/CI," scoped to the network-touching api-drift fetch.
- **Runbook:** `docs/MAINTENANCE_RUNBOOK.md` — `Checkpoint-host gap (U7 / R19)` note (the R8/AE8 edit target), per-check review sections, no existing aggregate target.
- **ADR 0009** (`docs/adr/0009-rust-first-permanent-tooling.md`) — network-free CI posture; "network-free" = no *LS* network (the build still fetches the toolchain/crates).
- **ADR 0013** (`docs/adr/0013-github-issues-maintenance-work-queue.md`), `.github/ISSUE_TEMPLATE/sdk_work_item.yml`, `docs/maintenance-labels.md` — the Work Queue taxonomy the rolling issue stays distinct from.
- **GitHub Actions (external, load-bearing — shaped KTD3/KTD4/KTD6/KTD7):**
  - Scheduled workflows are auto-disabled after **60 days of inactivity** (public repos); **no** consecutive-failure auto-disable exists (corrected the brainstorm's R9 premise). Failure email goes to one user (last cron editor), account-deletion-fragile. — docs.github.com Actions events / disable-and-enable-workflows / workflow-run notifications.
  - **Body edits do not notify**; comments, @mentions, assignment, and state changes do (drives KTD4/R10). — GitHub notifications docs.
  - Idempotent issue upsert via `gh` CLI by stable label, `--state all` for reopen; minimal `permissions:` block. — Actions workflow-syntax / `gh` CLI.
  - Off-peak cron to dodge top-of-hour drops (`'17 7 1 * *'`). — Actions schedule docs.
  - **Caching is ineffective at monthly cadence** (7-day cache eviction); `Swatinem/rust-cache@v2` would only help at weekly cadence. — Actions caching docs; `Swatinem/rust-cache`.
- **Adjacent learning:** `docs/solutions/architecture-patterns/change-tracker-baseline-clean-self-diff.md` — CI/local filesystem-case divergence caution.
