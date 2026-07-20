# Runbook — Production Snapshot Validation

Operator procedure (issue #189, R9/R17; AC5) for confirming an **owner-local** KRX calendar
snapshot is authorized for the current agreement and covers the planned operating horizon —
**without copying any KRX-derived facts into a committed artifact**. Run this before any
Consumer Retirement Gate; a snapshot that fails here forces **HOLD** (stay Shadow, Legacy
authoritative).

> Owner-local + offline. The production snapshot lives only under the gitignored, owner-readable
> `/state` (and `/calendar-snapshots`, `*.calendar.json*`) tree — never committed. This runbook
> reads it locally; it publishes nothing.

## Record-boolean-not-dates rule

You will inspect **real KRX coverage dates and identities** locally to validate the horizon.
You record only a **PASS/HOLD** verdict in the committed gate-verdict record
(`gate-verdicts/<consumer>.json`). Dates, `artifact_id`/`calendar_id`, authority, and coverage
endpoints stay in your **owner-local gate log**, never in a committed file (R17/KTD9). The
closeout scan (`make foundation-gate`) fails the build if `CLOSEOUT.md` leaks a hash or ISO date.

## 0. Preconditions

- [ ] `LS_CALENDAR_SNAPSHOT` points at the owner-local snapshot path.
- [ ] The path is under the gitignored `/state` (or `/calendar-snapshots`) tree — confirm it is
      NOT tracked (`git status --porcelain <path>` prints nothing / ignored).
- [ ] You have the current KRX data agreement's authorized `authority` label and its
      expiry/termination terms to compare against.

## 1. Load + inspect (owner-local)

```sh
cd adapters/nautilus
cargo run --release --bin calendar-status -- --as-of "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
# (reads $LS_CALENDAR_SNAPSHOT; prints the REDACTED diagnostic — authority is fingerprinted)
```

Confirm, from the human/JSON diagnostic and your local records:

- [ ] **Outcome is usable** — `healthy` or `stale` (not `load:*`, not `out_of_range`). A
      `load:unauthorized` / `load:expired` / `load:missing` / `load:corrupt` /
      `load:incompatible` is an automatic **HOLD**.
- [ ] **Authorized for the current agreement** — `authorization: authorized`, and the masked
      `authority_fingerprint` matches the fingerprint you recorded for the current agreement in
      your owner-local log (the raw authority never prints).
- [ ] **Not expired/terminated** at the planned operating horizon — `expires_at` /
      `terminated_at` (if present) are beyond the last date you plan to operate.
- [ ] **Coverage spans the horizon** — `coverage: materialized <from>..<through>` includes every
      date from today through the planned operating horizon. A per-date `calendar status` query
      for the horizon endpoints must NOT return `out_of_range` (a lapsed-coverage snapshot loads
      cleanly but returns `OutOfRange` on the uncovered date — the exact failure that would make
      every Enforced consumer refuse).
- [ ] **Freshness** acceptable — a `stale` outcome is usable but note the stale dimension(s);
      decide per your operating policy whether stale is acceptable for this gate.

## 2. Verdict

- **PASS** → record `PASS` only (no dates/identities) in the committed
  `gate-verdicts/<consumer>.json`; keep the inspected dates/identities in your owner-local log.
- **HOLD** → leave the record `HOLD`; the consumer stays Shadow, Legacy authoritative (R16). Fix
  the snapshot (re-authorize, re-materialize coverage, refresh) and re-run this runbook.

## Hold conditions (any → HOLD)

- Unauthorized / expired / terminated authorization at the horizon.
- Coverage does not reach the planned operating horizon (a horizon endpoint is `out_of_range`).
- The snapshot fails to load/validate (`load:*`).
- The authority fingerprint does not match the current agreement.
