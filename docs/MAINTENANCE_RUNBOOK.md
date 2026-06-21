# Maintenance Runbook

Recurring, operator-run maintenance steps for the LS adapter SDK. These are
**opt-in** and network-touching; the default `cargo test` and CI gates stay
network-free (ADR 0009, R18).

> **Checkpoint-host split (U7 / R19).** The maintenance checks now have **two**
> hosts, by trigger posture:
>
> - **Evidence freshness is on a timer.** Its cadence gap is closed by a scheduled,
>   non-gating GitHub Actions workflow
>   ([`.github/workflows/freshness-cadence.yml`](../.github/workflows/freshness-cadence.yml)) —
>   the repo's first automation. See [Scheduled freshness cadence](#scheduled-freshness-cadence)
>   below. Freshness qualifies because it is network-free (ADR 0009) and advisory
>   (`Severity::Evidence` never gates).
> - **API Drift and Specification Document stay operator-run.** This runbook is
>   their host. Run them at a maintenance checkpoint with
>   [`make maintenance-sweep`](#manual-maintenance-sweep) (or each target
>   individually). `api-drift` stays off any timer under **R19** — it makes a live
>   LS fetch, whose credentials, rate limits, and failure handling need separate
>   justification before scheduling. `spec-doc` is network-free and *could* be
>   scheduled like freshness; it stays operator-run **this increment by scope
>   choice**, not by an R19 constraint.
>
> R19 is therefore scoped to the network-touching `api-drift` fetch, not a blanket
> prohibition: scheduling the network-free freshness check operates outside R19's
> intent rather than overturning it. When a release checklist or broader scheduled
> review is introduced, fold the operator-run steps into it and link back here.

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

## Manual maintenance sweep

The two operator-run checks — API Drift and Specification Document — are
aggregated behind one target so a checkpoint is a single command:

```sh
make maintenance-sweep
```

It runs `api-drift-check` then `spec-doc-check` in sequence (both always run, even
if the first is non-zero) and exits with the **worst** outcome of the two: `0`
clean, `1` a finding gated (api-drift review needed), `2` an error. Evidence
freshness is **deliberately not** part of the sweep — it runs on its own schedule
(see [Scheduled freshness cadence](#scheduled-freshness-cadence)); bundling it
here would re-introduce the "forgot to run it" gap the schedule exists to close.
Run `make freshness-check` standalone for offline convenience.

The individual sections below document each check's exit semantics and the action
each finding calls for.

## API Drift review

Detects upstream LS Open API changes against the committed bounded baseline and
the reviewed code-set. Run at each maintenance checkpoint (or via
[`make maintenance-sweep`](#manual-maintenance-sweep)):

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

### Field-type re-pin (clean baseline refresh)

A one-time, type-scoped Baseline Promotion that resolves the HTTP-500-seeded
field-`type` provisionality recorded in
[`metadata/PROVISIONALITY-LEDGER.md` §4](../metadata/PROVISIONALITY-LEDGER.md):
the 36 normalized shapes' `type` values were derived from the hardcoded
property-type fallback the seed snapshot was fetched under, not a live
`system-codes` mapping. Re-deriving them requires a clean fetch plus a reviewed
promote, guarded by the opt-in **type-only gate** so unrelated structural drift
cannot ride into the Reviewed Baseline. Background:
[the re-pin brainstorm](brainstorms/2026-06-21-field-type-repin-clean-baseline-refresh-requirements.md).

The gate is opt-in: this procedure is the only flow that passes `--type-only`.
General baseline promotion (`promote --attest` without `--type-only`) is
unaffected.

1. **Fetch cleanly.** Re-fetch while `system-codes` is healthy and confirm the
   fetch report shows `property_type_fallback_served == false`:

   ```sh
   make api-drift-fetch   # writes a timestamped staged run + latest.txt
   ```

   A `false` flag proves the mapping *source* was live. It does **not** guarantee
   every field resolved — a live mapping missing a particular property-type code
   still falls back to the raw code for that field, which is why retirement is
   evaluated per facet at step 6, not assumed from the flag. If the fetch was
   served the fallback, the next step's gate (and `api-drift check`) exits `2`
   with *"facts outage affects a maintained TR … re-fetch before comparing"* —
   read that as "`system-codes` was unhealthy, retry the fetch", **not** a
   type-only-gate failure. Wait for `system-codes` to recover and re-fetch.

2. **Preview the type-only gate.** Review the drift and the gate decision without
   writing anything:

   ```sh
   cargo run -q -p ls-trackers -- api-drift promote --type-only --dry-run
   ```

   The preview prints the drift report plus a `type-only gate: ADMIT` or
   `type-only gate: BLOCKED — <reason>` line. The maintained type wave itself
   gates (Breaking for implemented/recommended TRs, Maintenance for tracked-only),
   so an admitted run still exits `1` — that is the signal that `--attest` is
   required, not a block.

3. **If the gate blocks, stop.** A `BLOCKED` decision means the clean fetch
   carried non-type drift on a maintained TR (a new/removed/reordered/moved
   field, a length or required-flag change, an endpoint/protocol/rate change, or a
   new/removed TR). Do **not** force it — `--attest` cannot satisfy the type-only
   gate. Open a separate Maintenance Review Decision for that drift and re-run the
   re-pin once it is resolved.

4. **Promote (attested).** On an admitted gate, perform the type-only promote:

   ```sh
   cargo run -q -p ls-trackers -- api-drift promote --type-only --attest <operator-or-issue>
   ```

   This runs the normal whole-raw promote: the committed raw is replaced by the
   clean staged run's raw, the normalized baselines are re-derived from the
   staged mapping (no live re-resolution at promote time), and one promotion
   record is appended. Exit `0` on success; exit `2` with zero mutation if the
   gate blocks.

5. **Confirm a clean self-diff.** Re-check the just-promoted baseline to confirm
   the refresh did not break the zero-finding self-diff invariant:

   ```sh
   make api-drift-check
   ```

   Expect exit `0` (no gating findings) over the maintained inventory.

6. **Retire ledger §4 per facet.** Hand-edit
   [`metadata/PROVISIONALITY-LEDGER.md` §4](../metadata/PROVISIONALITY-LEDGER.md),
   replacing the batch-wide row with the explicit Retired / Still-provisional
   split below. Retire **only** the facets the clean fetch concretely resolved;
   every residual names its exact reason. Leave no batch-wide "all 36 provisional"
   claim and no mixed-state ambiguity.

   ```markdown
   ## 4. Field-level `type` facets — re-pinned from clean `system-codes` (YYYY-MM-DD)

   Re-derived from a clean `system-codes` fetch (`property_type_fallback_served ==
   false`) via an attested type-only Baseline Promotion (promotion record
   `<attested-by>`, raw_hash `<hash>`). The HTTP-500 seed framing is retired;
   field `type` provisionality is now tracked per facet.

   **Retired** — type resolved by a non-fallback `system-codes` mapping:

   | TR / facet | Resolved type source |
   |---|---|
   | <tNNNN> (all fields) | live `system-codes` mapping, clean fetch YYYY-MM-DD |
   | … | … |

   **Still-provisional** — not resolved by the clean fetch; each names its reason:

   | TR / facet | Reason (untyped / raw-coded after clean fetch / blocked path) |
   |---|---|
   | <tNNNN>.<field> | live mapping had no entry for the property-type code → still raw-coded |
   | … | … |
   ```

   Mirror ledger §5's `End state` framing: a decided, per-facet split with a
   credential-free basis line. If the clean fetch resolved every facet, the
   Still-provisional table is empty and §4 is fully retired.

> The live re-pin run itself (fetch against live `system-codes`, the attested
> `--type-only` promote, and the data-dependent §4 edit) is operator-executed and
> intentionally deferred — the gate, this procedure, and the §4 template are the
> shipped capability.

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

## Evidence-freshness review (age backstop + change-driven staling)

Check whether any **Recommended TR**'s Focused Evidence has gone stale — either
more than 90 days since its `maintenance.last_reviewed` (**age**), or structurally
diverged from the shape it was attested against (**change**):

```sh
make freshness-check
```

This is **network-free** and **advisory**: it reads committed metadata and the
committed baseline, evaluates each Recommended TR against today (UTC), and prints
any stale entry with its `reasons` (`age`, `change`, or both). `Severity::Evidence`
sits below `Maintenance`, so a stale finding never gates — `freshness-check` exits 0
even when evidence is stale; only a metadata load/parse error or an
absent/unreadable committed baseline exits 2. The evaluator mutates nothing.

The two reasons clear **independently** (R10):

- **Age-stale** clears by refreshing the review date.
- **Change-stale** clears by re-pinning the attested shape to the current baseline.
- Refreshing the date does **not** clear change-staleness, and re-pinning does
  **not** clear age-staleness. A both-stale TR needs both.

When a TR is flagged stale, **re-attest** it (the same human flow as promotion):

1. Rerun its Paper Live Smoke and capture a fresh credential-free evidence line.
2. Update the evidence file's `date` and `maintenance.last_reviewed` (the
   validator keeps them equal) to the new run date. *(Clears `age`.)*
3. **If stale by `change`** (or after any baseline refresh): re-pin the attested
   shape to the current committed baseline. *(Clears `change`.)*
   ```sh
   make freshness-re-pin TR=<tr> FORCE=1   # FORCE overwrites the standing attested shape
   ```
   Re-pin is **populate-if-absent** by default — it refuses to overwrite an
   existing attested shape (which would silently clear a genuine stale-by-change
   signal); `FORCE=1` is required during a real re-attestation. **Always re-pin
   against a freshly-fetched baseline** (run `make api-drift-fetch` and review/commit
   the baseline first) — re-pinning against a stale baseline bakes in
   `attested == stale baseline`, the silent-green this feature exists to remove.
4. Regenerate docs with `make docs`.

The next `freshness-check` then finds the TR fresh. Clearing is
recompute-on-invocation: the prior finding is not retracted, it simply is not
re-emitted.

### Baseline staleness and re-attestation advisories

The check surfaces two advisory signals beyond stale findings:

- **Baseline staleness (R9a).** When the committed baseline's stamped `refreshed`
  date is older than 90 days (or missing), the check warns that change-detection is
  comparing against possibly-outdated structural truth. Clear it by refreshing the
  baseline (`make api-drift-fetch`, review, commit) — its manifest is stamped with
  the refresh date at write time. A never-stamped baseline reads as a warning, never
  a silent pass.
- **Re-attestation advisory.** A TR whose `attested_normalizer_version` differs from
  the baseline manifest's `normalizer_version`, or whose per-TR baseline shape is
  missing, is reported as needing re-attestation — its change-detection is suppressed
  (never a silent fresh-by-change), so re-pin it against the current baseline.

**Normalizer-version bumps (R2a).** A `NORMALIZER_VERSION` bump re-projects every
baseline shape, so every Recommended TR's `attested_normalizer_version` will mismatch
and route to the re-attestation advisory — detection is **blind** for those TRs until
they are re-pinned. To bound that blind window, **re-attest all six Recommended TRs
within 7 days of any `NORMALIZER_VERSION` bump** (re-seed the baseline with
`make api-drift-renormalize`, then `make freshness-re-pin TR=<tr> FORCE=1` for each of
`token`, `t1101`, `t1102`, `t8412`, `S3_`, `CSPAQ12200`). Any normalizer change that
can alter field-name or block-name projection **must** bump `NORMALIZER_VERSION` — a
projection change shipped without a bump surfaces as a (correct, but spurious-looking)
mass stale-by-change instead of routing through re-attestation.

### Scheduled freshness cadence

Unlike the operator-run checks above, evidence freshness also runs **on a timer**
so a lapse surfaces without anyone remembering to check. The workflow
[`.github/workflows/freshness-cadence.yml`](../.github/workflows/freshness-cadence.yml)
— the repository's first automation — runs monthly (`cron: '17 7 1 * *'`, plus a
`workflow_dispatch` for manual test/recovery) and:

- Runs the same evaluation as `make freshness-check`, in its `--json` form, and
  drives a single rolling **"Evidence freshness status"** issue (carrying the
  dedicated `freshness-status` label): opened/updated when Recommended TRs are
  stale, closed when all are fresh. The issue body is rewritten every run as a
  silent dashboard; a notifying comment that @mentions the maintainer is posted
  only on a transition *into* staleness (first appearance, a newly-stale TR, or a
  reopen after a manual close), so same-set re-runs do not spam.
- Is **non-gating**: stale evidence never fails the job (the check exits 0 whether
  stale or fresh). A *failure* means a build/tooling error or exit 2 — the check
  genuinely could not run.
- Is **credential-free**: network-free in the LS sense (no LS API call, no LS
  secret), using only the automatic `GITHUB_TOKEN` with `issues: write`.
- Is **not** a Maintenance Work Queue item. The rolling issue deliberately avoids
  the `[SDK work item]:` title prefix and every `queue:*`/`source:*`/`class:*`/
  `support:*`/`gate:*` label; **escalation stays human** (R4, ADR 0013) — the
  issue prompts a maintainer to re-attest, it never auto-files SDK work.

The maintainer handle @mentioned on a transition comes from the `FRESHNESS_MAINTAINER`
repository variable (not a secret); leave it unset to post without a mention.

**Watcher-liveness residual gap (R9).** An `if: failure()` step makes the watcher's
own death-by-failure visible — on a build/tooling error or exit 2 it posts an
@mentioning comment with the run link, rather than relying on GitHub's built-in
failure email (which reaches only the last cron editor and is account-deletion-
fragile). Three silent-death vectors remain **uncovered in-repo**, because none
emits a run or a failure event:

1. **60-day-inactivity disable** — GitHub disables scheduled workflows on a public
   repo after 60 days without repo activity.
2. **Dropped runs** — a scheduled run can be silently skipped under platform load.
3. **Schedule-disabling edit** — a later malformed-YAML / workflow-syntax error
   that disables the schedule with no run and no failure event.

`actionlint` on the workflow is the only in-repo guard, and it covers **only**
vector 3. Vectors 1 and 2 are catchable only by an **external dead-man's-switch
(heartbeat)**, which is deferred follow-up: it requires an external service and a
heartbeat-URL secret, denting the credential-free posture. Monthly maintainer
commits keep the timer alive in practice, but the gap is real and stated here
honestly rather than papered over.

### Notes

- The age backstop and change-driven staling are both enforced by the freshness
  evaluator (`metadata/EVIDENCE-FRESHNESS.md`), sharing the same `Severity::Evidence`
  surface and the same rolling issue (distinguished by `reasons`). Only the
  *auto-revoke* arm (flipping `support.recommended` on a detected change) stays
  deferred — staling is advisory; a human re-attests or demotes.
- Generated docs render a deterministic **review-by date** (`last_reviewed` + 90
  days) so they stay byte-identical across runs; the live stale verdict comes from
  `freshness-check`, not the committed page.
