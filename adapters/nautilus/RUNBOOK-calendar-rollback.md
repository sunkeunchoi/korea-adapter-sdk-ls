# Runbook — Activation + Rollback Rehearsal

Operator procedure (issue #189, R10; AC6 live half; F4) for the live half of the rollback
proof: **activate** a reviewed candidate and prove — by process restart — that the expected new
artifact identity loads, then **roll back** and prove the previous artifact identity and
adoption state are restored. The offline half (that the rollback machinery restores the prior
artifact + adoption/activation identity) is proven by `tests/calendar_activate.rs`
(`rollback_restores_prior_artifact_and_adoption_identity`, part of `make foundation-gate`); this
runbook repeats the same assertions against the **owner-local production snapshot** with the
restart the agent cannot perform.

> Owner-local + operator-attended. Runs against the gitignored `/state` snapshot. The `calendar`
> CLIs (`calendar-activate`, `calendar-rollback`, `calendar-status`) are pure filesystem
> operations — no credentials, no gateway.

## 0. Preconditions

- [ ] `LS_CALENDAR_SNAPSHOT` (the active path, call it A-active) is a valid, authorized snapshot
      that covers today (validate with **RUNBOOK-calendar-snapshot.md** first).
- [ ] A reviewed candidate B and its signed `ActivationApproval` JSON exist (naming B's exact
      `artifact_id` and acknowledging every HIGH-RISK entry `calendar-refresh` reported).
- [ ] You retain a copy of the current active snapshot A as a separate file (the rollback
      target) — e.g. `cp "$LS_CALENDAR_SNAPSHOT" "$LS_CALENDAR_SNAPSHOT.prior"`.

## 1. Record A's identity (owner-local)

```sh
cd adapters/nautilus
cargo run --release --bin calendar-status -- --as-of "<RFC3339-now>"   # note A's artifact_id (local log)
```

- [ ] Record A's `artifact_id` and adoption/activation identity in your **owner-local** log.

## 2. Activate B, then restart to prove the new identity loads

```sh
cargo run --release --bin calendar-activate -- \
  --active "$LS_CALENDAR_SNAPSHOT" --candidate <B.candidate> \
  --approval <B.approval.json> --as-of "<RFC3339-now>"
```

- [ ] The tool prints the `ActivationRecord` (predecessor = A, candidate = B) and
      `activated: <path>`.
- [ ] **Restart the consuming process** (ingest / lab-research / lab-live / budget-probe).
- [ ] After restart, its **startup calendar record** (stderr, `calendar-startup … artifact_id=…`)
      shows **B's** `artifact_id`. This is the restart-after-activation identity proof.

## 3. Roll back to A, then restart to prove the prior identity + adoption state are restored

```sh
cargo run --release --bin calendar-rollback -- \
  --active "$LS_CALENDAR_SNAPSHOT" --prior "$LS_CALENDAR_SNAPSHOT.prior" \
  --approval <A.approval.json> --as-of "<RFC3339-now>"
```

- [ ] The tool prints the `RollbackRecord` — `restored_artifact_id = A`, `superseded_artifact_id
      = B`, operator, reason.
- [ ] Rollback **refuses** (typed error, active file unchanged) if the prior snapshot is
      corrupt/unauthorized/expired, or **does not cover today** (`PriorDoesNotCoverAsOf`) — that
      refusal is the guard against silently installing a lapsed-coverage snapshot that would make
      every Enforced consumer return `OutOfRange`. If it refuses, treat as **HOLD** and fix the
      prior snapshot.
- [ ] **Restart the consuming process** again.
- [ ] After restart, the startup calendar record shows **A's** `artifact_id` again, and the
      adoption state is what it was before B — the restart-restore proof.

## 4. Verdict

- **PASS** (both restart proofs held; rollback restored A) → this satisfies the AC6 live half for
  the consumer's Consumer Retirement Gate.
- **HOLD** (any refusal, or the restart identity did not match) → do not proceed to Enforced;
  the consumer stays Shadow, Legacy authoritative (R16).

## Hold conditions (any → HOLD)

- Activation refused (stale-base / invalid / unreviewed / unacknowledged-high-risk).
- Rollback refused (unusable prior, or prior does not cover today).
- A restart did not load the expected `artifact_id`.
