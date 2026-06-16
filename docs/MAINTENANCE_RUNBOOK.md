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
| `1` | A finding touches a tracked/implemented/recommended TR at ≥ maintenance, **or** a new untracked TR was discovered (R17b) | Review each gating finding. For a **new-TR discovery**, decide whether to admit the code into the reviewed code-set (KTD-5) — re-attestation is the reviewed commit that updates `code-set.json`; no separate attestation file. For a **maintained-TR change**, plan the SDK/metadata follow-up. |
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
