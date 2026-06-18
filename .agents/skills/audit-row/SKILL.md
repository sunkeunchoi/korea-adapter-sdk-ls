---
name: audit-row
description: Audit one carried/discard row of the migration-source extraction ledger for the decommission gate — classify it (behavioral/knowledge/discard), apply the matching tiered bar, reach exactly one verdict (confirmed/refuted/unverifiable), write a self-contained credential-free per-row record while the old source is still readable, and emit a machine-readable return line. Use for a single ledger row ID (e.g. "audit L16"). Read-only verification; non-interactive and state-driven. The decommission-row-auditor agent and the audit-carried-rows orchestrator both run this recipe.
---

Audit exactly **one** ledger row and freeze a re-checkable record. This is a
read-only, state-driven recipe modeled on `promote-tr`: it classifies the row,
applies the bar for that class, and writes a verdict — it never promotes, flips
recommendations, regenerates docs, or edits the ledger (the roll-up phase owns
ledger re-disposition).

**Input:** one row ID — `L1`…`L26` (the `$ARGUMENTS`, e.g. `L16`).
**Output (last line, machine-readable):** `AUDITED <id> <verdict> records/<id>.yaml`
or `HELD <id> — <reason>`.

This skill is **non-interactive** — it asks no questions; it infers everything
from the manifest entry, the two repos, and (for behavioral rows) the smoke/test
result. Re-read repo + old-source truth rather than trust conversation memory.

Boundary: this skill owns exactly one row's record. It does **not** own the sweep
ledger, `outcomes.jsonl`, sibling dispatch, the gate computation, or the ledger
re-disposition — those belong to `audit-carried-rows` (U5/U6).

## 0. Read the assignment and re-read truth

Read the row's entry in `docs/migration-source/audit/manifest.yaml`
(`id`, `area`, `disposition`, `candidate_class`, pointers). Then re-read the
actual truth for this row:
- the old-source path(s) in `korea-broker-sdk-ls` (the sibling checkout — R10),
- the extracted target path(s) in this repo,
- for behavioral rows, the runnable target named in the entry.

Do not trust the manifest seeds as facts — they only orient you.

## 1. Classify and record the classification (R5)

Classify the row `behavioral` / `knowledge` / `discard` per
`references/classification-and-bars.md`. The manifest `candidate_class` seeds
this but does not bind it; record the class you actually apply. Discard rows are
always `discard`.

## 2. Apply the matching bar (`references/classification-and-bars.md`)

- **behavioral** — production-only ⇒ record `production_only` + reason and return
  `unverifiable` **without running an inadmissible test** (R6a/R11); else run the
  matching `make` smoke / `cargo test` / slice target and capture a
  credential-free `line` (R11/R12). Split the claims: lifecycle/structural claims
  the target proves are `confirmed`; sub-claims it does not exercise (e.g.
  WebSocket reconnect replay, terminal exhaustion, latest-only wakeup) are
  recorded `unverifiable`, never folded into a confirm. No passing proof ⇒
  `refuted`.
- **knowledge** — declare the extraction mode (no determinable mode ⇒
  `unverifiable`, R7b); build the claim-by-claim claim-map against that mode,
  transcribing each `claim_text` **inline in full** (a reference into the old
  source is invalid, R14); confirm only on no material loss (R7/R7a). A missing
  material claim ⇒ `refuted`, gap named.
- **discard** — presence-and-coherence on the recorded reason (R8). The audit
  does not judge whether the discard was correct.

## 3. Secret-safety check (blocking, R12)

The record is about to be **committed**. Run the credential scan from
`.agents/skills/audit-carried-rows/references/record-format.md` ("Credential-free
non-negotiable") across **every** transcribed or free-text field — the behavioral
`line`, every knowledge `claim_text`, and any `acceptance_reason` — not just the
`line`. Claims copied from old-source docs can carry credential field-names
(`appkey`, `account_no`, `rsp_msg`). On any match, **STOP**: do not write the
record; emit `HELD <id> — record field not credential-free, needs a fix`.

## 4. Write the per-row record (R13)

Write `docs/migration-source/audit/records/<id>.yaml` per the schema in
`.agents/skills/audit-carried-rows/references/record-format.md`: the secret-safety
comment header, then the common fields (`row_id`, `area`, `classification`,
`verdict`, `bar_applied`, `evidence_pointer`, `provenance`), the class-specific
block, and any verdict tail block (`re_disposition`+`gap` for refuted;
`unverifiable_reason` for unverifiable). The `evidence_pointer` MUST be `inline`
or a git-tracked repo-relative path that resolves on disk — never an old-source
path, a `target/` artifact, or an uncommitted file. Old-source paths go in
`provenance` only, marked non-load-bearing (R13/R14). Write **only this row's
record** — do not touch the ledger, `outcomes.jsonl`, or sibling records.

## 5. Emit the return line

Emit exactly one machine-readable final line for the orchestrator to parse:

```
AUDITED <id> <verdict> records/<id>.yaml
HELD <id> — <one-line reason>
```

where `<verdict>` is `confirmed`, `refuted`, or `unverifiable`
(`assumption-accepted` is only ever set later, by the roll-up's maintainer
acceptance — never by this recipe).

## Reference

- `references/classification-and-bars.md` — the behavioral / knowledge / discard
  bars and the verdict decision.
- `.agents/skills/audit-carried-rows/references/record-format.md` — the record
  schema, the shared credential-free pattern list, and the report format.
- `.agents/skills/audit-carried-rows/references/record-format.example.yaml` — a
  worked record per class.
- `.agents/skills/promote-tr/references/templates.md` — authoritative
  credential-free `line` shapes and the secret-safety blocking discipline.
- `.agents/skills/promote-tr/references/smoke-map.md` — the behavioral target map
  (the `S3_` → `live-smoke-ws` lifecycle-only target).
