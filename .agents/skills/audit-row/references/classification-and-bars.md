# Classification and tiered verification bars

The single-row audit (`audit-row`) classifies one ledger row, applies the bar
matching that class, and reaches exactly one verdict (R2). The manifest's
`candidate_class` / `candidate_extraction_mode` **seed** the decision; the
recorded classification is the agent's own judgment (R5).

Verdicts: `confirmed`, `refuted`, `unverifiable` (R2). A maintainer may later
promote `unverifiable` → `assumption-accepted` (R4a) in the roll-up; the auditor
itself never self-accepts.

---

## Classify

| Class | Signal | Bar |
|---|---|---|
| **behavioral** | a runnable test / smoke / slice target in **this** repo exercises the behavior | proof (below) |
| **knowledge** | design/lesson/snapshot captured in a doc or data file; no runtime to test (order rows are knowledge per ADR 0008) | completeness-vs-source |
| **discard** | the ledger disposition is `discard` | presence-and-coherence |

If the manifest seeds behavioral but the only target is a design doc with no
test, reclassify knowledge (or refute if even the doc is missing the claim). If
it seeds knowledge but a real runnable assertion exists, you may record
behavioral. Record the class you actually applied.

---

## Behavioral bar (R6, R6a, R11)

A behavioral row is **confirmed only by a passing test or recorded evidence in
this repo**. A doc *describing* the behavior is never sufficient (that is the
exact failure this audit exists to catch — AE2).

1. **Production-only check first (R6a/R11).** If the genuine behavior is
   reachable only in production (production trading is prohibited; a paper smoke
   cannot stand in), record `production_only: true` + a `reason`, set the verdict
   `unverifiable`, and **do not run an inadmissible test**. Route to maintainer
   acceptance (R4a) in the roll-up, never to a behavioral confirm.
2. **Otherwise run the matching target** (`make` smoke / `cargo test` / slice
   target) from the manifest entry. Capture the result line **verbatim**. For
   live smokes the live prerequisite is `LS_TRADING_ENV=paper`, credentials in
   `.env`, and a reachable paper gateway; absent those, the row is `unverifiable`
   with the blocking reason (AE5) — never a false green.
3. **Split the row's claims.** Claims the target actually exercises
   (lifecycle/structural) are `confirmed`. Claims it does **not** exercise — e.g.
   WebSocket reconnect replay, terminal exhaustion, latest-only wakeup on the
   lifecycle-only `live-smoke-ws` — are `unverifiable`, named explicitly, and
   never folded into the confirm.
4. A confirmed behavioral row's `line` is credential-free (R12, secret-safety
   step) and the `evidence_pointer` resolves in-repo or `inline` (R13).

Verdict: pass on the exercised claims → `confirmed`; no passing proof →
`refuted` (re-disposition); blocked by env/credential/production-only →
`unverifiable`.

---

## Knowledge bar (R7, R7a, R7b)

A knowledge/design/lesson row is **confirmed by claim-by-claim
completeness-vs-source**, not a holistic "looks complete".

1. **Declare the extraction mode (R7b):** `full-transcription`,
   `summary-plus-snapshot`, or `distilled-lesson`. A row with **no determinable
   mode is `unverifiable`** — stop here.
2. **Build the claim-map (R7a):** read the old source while it is still readable;
   enumerate its discrete claims and constraints; transcribe each `claim_text`
   **inline in full** (a claim recorded as "see old source line 42" violates R14
   and is invalid); map each to a `target_location` in this repo; mark `status`
   `present` / `adapted` / `missing`. Apply the bar to the declared mode:
   - *full-transcription* — total coverage; any missing claim refutes.
   - *summary-plus-snapshot* — the snapshot preserves the data and the summary
     preserves every decision-relevant fact.
   - *distilled-lesson* — the lesson preserves every decision-relevant
     constraint, even if prose is condensed.
3. **"Material" (R7a):** any behavioral constraint, numeric value, response
   code, threshold, or edge case whose loss would change an implementation
   decision. A `missing` material claim **refutes** the row — name the gap.
4. Record the distinct `source_documents` the claim-map covers, so U6's
   source-coverage reconciliation can flag an old-source doc no row claimed.

Verdict: no material loss → `confirmed`; a missing material claim → `refuted`
(re-disposition, gap named, re-opens as `extract`, AE4); no determinable mode →
`unverifiable` (R7b).

---

## Discard bar (R8)

A discard row is confirmed by **presence-and-coherence**: a non-carry `reason`
is recorded (in the ledger Notes cell or the named location) and is internally
coherent. The audit does **not** judge whether the discard decision was correct
(out of scope). A missing or incoherent reason → `refuted`/`unverifiable`.

Verdict: reason present and coherent → `confirmed`.

---

## Re-disposition (refuted rows, R3)

A refuted `carried` row never stays `carried`. Record `re_disposition`
(`extract` / `defer` / `discard`) and a named `gap`. `extract` re-blocks the gate
until separately resolved (out of this audit's scope). The roll-up phase, not the
per-row auditor, edits the ledger.
