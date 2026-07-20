# Gate-verdict records — the mechanical merge-block

One committed, **non-sensitive** record per Consumer Retirement Gate (issue #189, KTD3 —
the three ingest boundaries share one gate). Each is the machine-readable trigger the
mechanical merge-block reads: a consumer's weekday primitive (and its `Legacy | Shadow` arm)
may be deleted **only** when this consumer's record is present and `"verdict": "PASS"`.

- **Enforcement**: `nautilus-ls-calendar/tests/merge_block.rs` (`tree_respects_the_merge_block`,
  run by `make merge-block-check` and in CI). If a consumer's weekday primitive marker is gone
  from the tree while its record is absent or `HOLD`, the check fails — a technical gate, not
  reviewer discipline (KTD1/R7). The pure coupling rule is unit-tested both directions in the
  same file (`merge_block_blocks_deletion_without_a_pass_verdict`), which runs in the default
  `make foundation-gate` / `make adapter-check`.
- **Default state**: `HOLD`. The consumer stays Shadow; Legacy stays authoritative (R16).
- **Flip to `PASS`**: only after the live, operator-attended Consumer Retirement Gate is
  recorded — owner-local canary, restart-after-activation, rehearsed rollback (the Ladder also
  an attended paper-session preflight). See each `RUNBOOK-retire-<consumer>.md`. Recording
  `PASS` here is the merge trigger for that consumer's staged retirement diff.

## Publication boundary (R17 / KTD9)

**Verdict-only.** These records carry the consumer name, gate name, `PASS`/`HOLD` verdict, and
software/schema versions — and nothing else. Never write a KRX-derived date, a snapshot
`artifact_id`/`calendar_id`/fingerprint, or an owner-local canary fact into them. Those live
only in the operator's local gate log.

## Fields

| Field | Meaning |
|-------|---------|
| `consumer` | The gate's consumer boundary (`ingest` / `catalog` / `budget-probe` / `ladder`). |
| `gate` | Human name of the Consumer Retirement Gate. |
| `verdict` | `HOLD` (default) or `PASS` (live gate recorded). |
| `software_version` | The adapter software version the verdict was recorded against. |
| `schema_version` | The calendar snapshot schema version. |
| `note` | The pending live steps / runbook reference. No KRX facts. |
