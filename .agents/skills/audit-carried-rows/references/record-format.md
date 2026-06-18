# Decommission audit — per-row record & roll-up report format

This is the authoritative schema for the two durable artifacts the audit
produces:

1. **Per-row records** — `docs/migration-source/audit/records/L<N>.yaml`, one per
   ledger row, written by the `decommission-row-auditor` agent while the old
   source is still readable.
2. **The roll-up report** — `docs/migration-source/audit/decommission-audit-report.md`,
   produced in the serial roll-up phase.

The committed validator `crates/ls-trackers/tests/decommission_audit.rs`
deserializes its **own** struct from these record files — the audit record is
**not** an `EvidenceRecord` (`crates/ls-metadata/src/schema.rs`). The record
mirrors only the *header + body convention* of `metadata/evidence/<tr>.yaml`
(a secret-safety comment header, then a flat machine-readable body), not its
fields.

A worked example per class lives in `record-format.example.yaml` — deliberately
outside `records/` so the validator's "exactly 26 records" check has nothing
extra to skip.

---

## Header convention (every record)

Open every record with a secret-safety comment header, mirroring
`metadata/evidence/t1101.yaml`:

```yaml
# Decommission audit record — L<N> (<area>)
#
# A durable, credential-free record of one ledger-row audit, captured while the
# old source was still readable. Self-contained (R13/R14): the evidence pointer
# resolves inside this repo or inline; old-source paths appear only as
# non-load-bearing provenance.
#
# Secret-safety: no token, appkey, secret, account number, or rsp_msg in any
# transcribed or free-text field (line / claim_text / acceptance_reason). See the
# credential-free non-negotiable below.
```

---

## Body schema

### Common fields (required on every record)

| Field | Type | Notes |
|---|---|---|
| `row_id` | string | `L1`…`L26`; must equal the file stem and a manifest/ledger ID |
| `area` | string | the ledger area name (matches the manifest entry) |
| `classification` | enum | `behavioral` \| `knowledge` \| `discard` — the agent's recorded judgment (R5), not the manifest seed |
| `verdict` | enum | `confirmed` \| `refuted` \| `unverifiable` \| `assumption-accepted` (R2/R4a) |
| `bar_applied` | string | one line naming the bar actually applied (e.g. `passing smoke`, `completeness-vs-source (full-transcription)`, `presence-and-coherence`) |
| `evidence_pointer` | string | exactly `inline`, **or** a git-tracked repo-relative path that resolves on disk (R13). Never an old-source path, a `target/` artifact, or an uncommitted file |
| `provenance` | list<string> | old-source paths compared, marked non-load-bearing. Optional but recommended |

### Class-specific block (exactly the one matching `classification`)

**`behavioral:`**

| Field | Type | Notes |
|---|---|---|
| `target` | string | the runnable target (`make live-smoke-ws`, `cargo test -p ls-core --test inner`, …) |
| `line` | string | the verbatim credential-free result line captured from the run. Required unless `production_only` |
| `production_only` | bool | `true` ⇒ behavior reachable only in production (R6a); omit/`false` otherwise |
| `reason` | string | required iff `production_only: true` — why the genuine behavior is production-only |

A `confirmed` behavioral row needs a `line` (proof). A `production_only: true`
behavioral row is `unverifiable`, never `confirmed` (R6a). Lifecycle/structural
sub-claims a target proves are `confirmed`; sub-claims it does not exercise
(e.g. WebSocket reconnect replay, terminal exhaustion, latest-only wakeup) are
recorded on a separate `unverifiable` row/record or split out in `bar_applied`
and routed to acceptance — never silently folded into a behavioral confirm.

**`knowledge:`**

| Field | Type | Notes |
|---|---|---|
| `extraction_mode` | enum | `full-transcription` \| `summary-plus-snapshot` \| `distilled-lesson` (R7b). A knowledge row with no mode is `unverifiable` |
| `claim_map` | list<claim> | the claim-by-claim comparison (R7a). Required for a `confirmed` knowledge row |
| `source_documents` | list<string> | the distinct old-source documents this claim-map covers — feeds U6 source-coverage reconciliation |

Each **claim** is:

| Field | Type | Notes |
|---|---|---|
| `claim_text` | string | the source claim **transcribed inline in full**. A claim recorded as a reference into the old source (empty text, or text containing an old-source path/line pointer like `see old source line 42`) violates R14 and is rejected |
| `target_location` | string | repo-relative path (optionally with `#anchor`) where the claim is represented |
| `status` | enum | `present` \| `adapted` \| `missing`. A `missing` material claim refutes the row (R7/R7a) |

**`discard:`**

| Field | Type | Notes |
|---|---|---|
| `reason` | string | the recorded non-carry reason |
| `coherence_note` | string | one line affirming the reason is internally coherent (R8). The audit does **not** judge whether the discard decision was correct |

### Verdict-specific tail blocks

| Block / field | Required when | Notes |
|---|---|---|
| `re_disposition` (enum `extract`\|`defer`\|`discard`) | `verdict: refuted` | the ledger row is re-dispositioned to this, re-blocking the gate (R3) |
| `gap` (string) | `verdict: refuted` | names the specific material loss / missing proof |
| `unverifiable_reason` (string) | `verdict: unverifiable` or `assumption-accepted` | why it could not be verified (R4) |
| `acceptance:` block | `verdict: assumption-accepted` | promotes an unverifiable row (R4a) |

The **`acceptance:`** block:

| Field | Type | Notes |
|---|---|---|
| `accepted_by` | string | the named maintainer |
| `acceptance_reason` | string | names the **specific residual risk** accepted — never a bare "accepted", never a credential/account value |
| `accepted_date` | string | `YYYY-MM-DD` |

---

## Credential-free non-negotiable (R12) — the shared pattern list

This list is **authoritative** and shared verbatim by the `audit-row` recipe's
secret-safety step and the U6 validator's credential scan, so the recipe and the
validator agree on what counts. It applies to **every transcribed or free-text
field** — the behavioral `line` and `reason`, every knowledge `claim_text` and
its `target_location`, the `discard` `reason` and `coherence_note`, the
`unverifiable_reason`, the refuted `gap`, and the `acceptance_reason` — not just
the behavioral line. Any field copied from an old-source doc can carry a
credential field-name into a committed record.

A field is **rejected** if, case-insensitively, it contains any of:

```
rsp_msg
appkey   app_key   apikey   api_key
secret
password   passwd
bearer
authorization
account_no   acnt_no   accountno   account_number
token=        # a token VALUE assignment; "token_len=" is safe (a length, not the token)
```

Deliberately **not** flagged: the bare word `account` (legitimate in prose like
"account balance inquiry"), response codes / prices / counts (digits in
`rsp_cd=00000`, `price=346500`, `reccnt=1` are public/structural). Match
field-names that carry secrets, not every digit. Safe line shapes are in
`.agents/skills/promote-tr/references/templates.md`.

---

## Roll-up report format (`decommission-audit-report.md`)

The report is markdown, produced in the serial roll-up phase, and states the gate
explicitly. Shape (mirrors the ledger table + the candid tone of
`metadata/EVIDENCE-FRESHNESS.md`):

1. **Gate verdict line** — exactly one of:
   - `GATE: TRUSTWORTHY-GREEN — all 26 rows confirmed or assumption-accepted, reconciled against the ledger, no unresolved extract.`
   - `GATE: NOT-GREEN — <n> blocking row(s): <ids>.`
2. **Counts** — `confirmed`, `assumption-accepted` (counted toward green but
   **reported apart from** `confirmed`), `refuted`, `unverifiable (un-accepted)`,
   `missing verdict`. A gate green only via acceptances is visibly distinct from
   one green via proof.
3. **All-rows table** — `| ID | area | classification | verdict | bar | evidence pointer |` for all 26 rows.
4. **Refuted list** — each refuted row with its `re_disposition` and named `gap`.
5. **Unverifiable (un-accepted) list** — each with its blocking reason.
6. **Assumption-accepted list** — each with `accepted_by` and the named residual risk.
7. **Source-coverage note** — any old-source document referenced by a manifest
   row that no record's `claim_map` claims a section of (under-enumeration
   surface, R7a / U6).

The gate is **trustworthy-green** only when all 26 rows have a recorded verdict
reconciled against the ledger, every row is `confirmed` or `assumption-accepted`,
and no row is an unresolved `extract` (R15). A missing verdict for any row is
not-green.
