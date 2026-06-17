# Maintenance Runbook

Recurring, operator-run maintenance steps for the LS adapter SDK. These are
**opt-in** and network-touching; the default `cargo test` and CI gates stay
network-free (ADR 0009, R18).

> **Checkpoint-host gap (U7 / R19).** This repo does not yet have a pre-existing
> recurring operator checkpoint (release checklist / periodic review) to host
> the API Drift check. This runbook *is* that host for now. When a release
> checklist or scheduled review is introduced, fold the "API Drift review" step
> below into it and link back here. No cron/CI scheduling is added (R19) — the
> trigger is an operator running the step at this checkpoint.

## Maintenance Work Queue

GitHub Issues are the **Maintenance Work Queue** (ADR 0013). After reviewing a
Tracker Finding or Manual Maintenance Input, decide whether it is accepted,
deferred, or rejected. Accepted items must be opened with the
`SDK work item` issue template and labelled according to
[`docs/maintenance-labels.md`](maintenance-labels.md).

An accepted issue must name the source signal, acceptance rationale, work item
type, affected TRs, dependency class, support state, required maintained
artifacts, selected Change-Scoped Gate, Baseline Promotion decision, and Focused
Evidence decision. A code change alone does not complete the item; completion
requires the selected gate to pass and the baseline/evidence decisions to be
recorded.

**Labels are the contract; the body is reviewed prose.** Labels are the only
machine-checkable part of a queue item — they are durable and filterable. The
issue body (the gate, baseline, and evidence decisions) is human-reviewed prose:
the template seeds it but does not enforce it after creation, so the body can be
edited freely. "Closed cleanly" therefore means a reviewer read the body and
confirmed it, not that any tool verified it. Do not treat the body as a machine
contract.

### Foundation Complete checkpoint

Foundation Complete is proven in **two stages**, not by one issue.

- **Stage 1 — plumbing.**
  [`#9 Prove Maintenance Work Queue end-to-end`](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/issues/9)
  proves the queue mechanics are self-consistent: labels, template fields, the
  completion checklist, and the runbook path. Because #9 carries no real TR and
  records Baseline Promotion and Focused Evidence as "Not needed," closing it
  proves the form is fillable — not that the flow carries weight.
- **Stage 2 — weight.** The next proof must be one **real** SDK-facing
  maintenance or expansion issue: a real affected TR, a real Change-Scoped Gate
  that runs, and genuine Baseline Promotion / Focused Evidence decisions.

**Foundation Complete is claimed only after Stage 2 closes**, not at #9.

## API Drift review

Detects upstream LS Open API changes against the committed bounded baseline and
the reviewed code-set. Run at each maintenance checkpoint:

```sh
make api-drift-check
```

Interpret the [tiered exit code](../docs/plans/2026-06-16-002-feat-api-drift-real-fetch-plan.md) (R17):

| Exit | Meaning | Action |
|------|---------|--------|
| `0` | No finding crossed the gate threshold | Nothing required. Report-only findings (untracked drift, description-only changes) may still print — review at your discretion. |
| `1` | A finding touches a tracked/implemented/recommended TR at ≥ maintenance, **or** a new untracked TR was discovered (R17b) | Review each gating finding. For a **new-TR discovery**, decide whether to admit the code into the reviewed code-set (KTD-5) — re-attestation is the reviewed commit that updates `code-set.json`; no separate attestation file. For a **maintained-TR change**, accept/defer/reject the finding; accepted findings become GitHub Issues in the Maintenance Work Queue. |
| `2` | Fetch, parse, baseline, or staged-run error | Investigate. A menu/group parse failure or a suspected mass-truncation (full inventory shrank > 10% vs the committed code-set) aborts before staging; a single baselined-TR absence is **not** an error (it is a removal finding at exit `1`). |

To review a previously staged run without re-fetching:

```sh
make api-drift-fetch                      # writes a timestamped staged run + latest.txt
cargo run -q -p ls-trackers -- api-drift check --staged target/ls-trackers/api-drift/runs/<timestamp>
```

To see what a real (future, mutating) promote would touch, without writing
anything:

```sh
make api-drift-promote-dry-run
```

### Notes

- The committed bounded baseline lives at
  `crates/ls-trackers/baselines/api-drift/`; it covers the 7 maintained TRs plus
  the full-inventory code-set. It is seeded once (provisionally) and re-attested
  incrementally as new TRs are admitted (KTD-2, KTD-5).
- `api-drift check` never edits `metadata/` or SDK code (R10); it is advisory.

## Specification Document review (example drift)

Detects upstream request/response **example** drift — the one documentation facet
the API Drift Tracker does not diff — against the committed example baseline, and
points at the maintained SDK artifacts a changed TR references. Run at each
maintenance checkpoint:

```sh
make spec-doc-check
```

Unlike `api-drift-check`, this is **network-free**: it re-projects examples from
the shared raw snapshot the API Drift staging path already produced
(`crates/ls-trackers/baselines/api-drift/raw/`), adding no new fetch source (R1).
Findings are **advisory and never gate** (KTD4):

| Exit | Meaning | Action |
|------|---------|--------|
| `0` | The comparison completed | Review any printed findings at your discretion. Each names the changed TR, the payload class that drifted, and — for a Tracked TR — the maintained docs (`docs/reference/{tr}.md` for Implemented TRs, `docs/tr-dependencies/{tr}.md` for all Tracked TRs) to review as candidates. An untracked-only change prints with no pointer. |
| `2` | Load, parse, or example-normalizer-version error | Investigate. After an `EXAMPLE_NORMALIZER_VERSION` bump, re-seed first (below). |

A finding becomes an **SDK Maintenance Work Item only after human review** (R8) —
you judge whether the referenced artifacts are stale; the tracker proves nothing
about staleness. SDK reference docs stay generated from maintained behavior and
metadata via `make docs` (R11); `spec-doc-check` resolves to existing doc paths,
it does not generate or mirror upstream text. Accepted findings become GitHub
Issues in the Maintenance Work Queue.

To re-seed the example baseline after an `EXAMPLE_NORMALIZER_VERSION` bump
(network-free; reads only the shared committed raw), then review the diff:

```sh
make spec-doc-renormalize
git diff crates/ls-trackers/baselines/spec-doc/normalized/examples.json
```

### Notes

- The committed example baseline lives at
  `crates/ls-trackers/baselines/spec-doc/`; it covers the full upstream inventory
  (every TR carrying an example, ~355) as one aggregated
  `normalized/examples.json` map, under its own `EXAMPLE_NORMALIZER_VERSION`
  (KTD2). It stores only structural descriptors — form keys and JSON
  field-name→leaf-type shapes — never a raw example value (KTD7).
- `spec-doc check` never edits `metadata/`, SDK code, docs, examples, or
  baselines (R8); it is advisory.
