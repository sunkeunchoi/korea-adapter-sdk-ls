# Runbook — Production Snapshot Genesis + Validation

Operator procedure for **producing** the first owner-local KRX calendar snapshot (the *Genesis*
section) and for **validating** that an owner-local snapshot is authorized for the current
agreement and covers the planned operating horizon (issue #189, R9/R17; AC5) — **without copying
any KRX-derived facts into a committed artifact**. Run the validation before any Consumer
Retirement Gate; a snapshot that fails there forces **HOLD** (stay Shadow, Legacy authoritative).
The Genesis section is the one-time bootstrap that no other tooling can perform; every later
refresh is incremental (`calendar-refresh`).

> Owner-local + offline. The production snapshot lives only under the gitignored, owner-readable
> `/state` (and `/calendar-snapshots`, `*.calendar.json*`) tree — never committed. This runbook
> reads it locally; it publishes nothing.

## Record-boolean-not-dates rule

You will inspect **real KRX coverage dates and identities** locally to validate the horizon.
You record only a **PASS/HOLD** verdict in the committed gate-verdict record
(`gate-verdicts/<consumer>.json`). Dates, `artifact_id`/`calendar_id`, authority, and coverage
endpoints stay in your **owner-local gate log**, never in a committed file (R17/KTD9). The
closeout scan (`make foundation-gate`) fails the build if `CLOSEOUT.md` leaks a hash or ISO date.

## Genesis: producing and installing the FIRST snapshot

The first snapshot has no predecessor — no `calendar-refresh` can bootstrap it (refresh requires
a loadable `--active`, `build_candidate` always stamps a predecessor, and activation refuses a
mismatched predecessor). Produce it once with the genesis pipeline; every later refresh is
incremental and reuses the same fetcher. **This whole section is operator-attended and live (real
credentials, real KRX/KASI endpoints) — an autonomous agent stops here and hands the runbook to
the maintainer.** The record-boolean-not-dates rule above applies throughout: nothing below is
committed except a PASS/HOLD verdict; every artifact stays owner-local under `/state`.

### G0. Provision credentials

Both keys live ONLY in the gitignored maintainer env — never an argument, never committed:

- `LS_KRX_APPKEY` — the approved KRX Open API authentication key (`openapi.krx.co.kr`; both a key
  and per-API approval are required; 10,000 calls/day; the agreement's use period is one year,
  renewable).
- `LS_KASI_SERVICE_KEY` — the KASI `getRestDeInfo` service key (`apis.data.go.kr`; per-year
  requests; 10,000-request dataset quota). Provision this before the run.

### G1. Probe first (bounded window = the probe)

Before the bulk fetch, run bounded `--window` runs to confirm the endpoints answer as U3's
parsers expect and to settle the two assumptions (KASI history depth; the KRX floor). Capture the
native envelopes and reconcile them against the parsers — **a shape mismatch routes back through
U3 and `make adapter-check` BEFORE the bulk run resumes** (the probe gate). A bounded `--window`
run doubles as the probe; there is no separate probe tool.

```sh
cd adapters/nautilus
cargo run --release --bin calendar-fetch-inputs -- \
  --window <floor..floor-plus-a-few-days> --krx-through <floor-plus-a-few-days> \
  --inputs-out state/probe.calendar-inputs.json --state state/probe.calendar-fetch.ckpt
```

If a parser disagrees with a captured envelope, STOP: fix U3, re-run `make adapter-check`, then
resume. If KASI depth falls short of the history floor, or the KRX floor is not answerable, that
is a **stop condition** (return to planning) — the coverage window is undeliverable as specified.

### G2. Bulk fetch (resumable, quota-honest)

```sh
cargo run --release --bin calendar-fetch-inputs -- \
  --window 2010-01-04..<operating-horizon> --krx-through <last-closed-session> \
  --inputs-out state/genesis.calendar-inputs.json --state state/genesis.calendar-fetch.ckpt \
  --pace-ms <cadence>
```

If a source reports `ok=false ... failed=client-side timeout after Ns`, the source did **not**
refuse — we hung up first. The KRX daily endpoint has been observed at 14-59 s per day under
load; raise `LS_CALENDAR_HTTP_TIMEOUT_SECS` (default 120, max 600) and re-run. This matters
because a timed-out source yields a **partial candidate with zero witnesses**, which reads
exactly like "KRX has no data for this window" — the two were indistinguishable before the
timeout was labelled, and telling them apart is what the label is for.

Resumable: if the run is interrupted or hits a daily quota, re-run the SAME command — it
continues from the 0o600 checkpoint, never restarting. A source that fails mid-run is recorded
partial (its covered range ends at the last completed date) and resumes on the next run. No
credential or raw KRX/KASI row ever reaches the checkpoint or the inputs artifact. All output
paths are confined beneath the owner-local state root.

### G3. Genesis build + review the description artifact

```sh
cargo run --release --bin calendar-genesis -- \
  --inputs state/genesis.calendar-inputs.json --out state/krx.calendar.json.candidate \
  --as-of "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --authority "<the current agreement's authorized authority label>" \
  --granted <RFC3339 grant instant> --expires <RFC3339 agreement-expiry instant> \
  --krx-through <last-closed-session>
```

The build **refuses in code** (not a checklist item) if any consumer-window weekday is still
Unknown (R12) or a source's coverage falls short of the genesis window — it names the offending
dates/ranges and writes NO candidate. Remedy a coverage gap with a top-up fetch (G2 resumes),
then re-build. On success it writes the candidate plus a genesis **description artifact**
(`…candidate.genesis-description.json`). Review the description:

- [ ] Coverage endpoints span the full window (history floor → operating horizon).
- [ ] **Consumer-window Unknown weekdays = 0** — confirm the R12 refusal was not overridden.
- [ ] Per-status and per-source counts look sane; the exact candidate `artifact_id` is recorded
      in your owner-local log.
- [ ] The stamped authorization (authority label, granted/expires) matches the current agreement.

### G4. First-install (full ceremony, exclusive create)

Author an `ActivationApproval` JSON naming the exact candidate `artifact_id` from G3 and listing
`genesis:no-predecessor` in `acknowledged`, then:

```sh
cargo run --release --bin calendar-activate -- --first-install \
  --active "$LS_CALENDAR_SNAPSHOT" --candidate state/krx.calendar.json.candidate \
  --approval <approval.json> --as-of "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
```

First-install refuses if `$LS_CALENDAR_SNAPSHOT` already exists — a live chain root is superseded
only through the normal `calendar-activate` path with its stale-base protection. It installs the
chain root owner-only (`0o600`) via an exclusive create that cannot clobber a concurrently
appearing file.

### G5. Validate + hand back

Run the validation checklist (§0–§2 below) against the installed snapshot. With it installed,
`catalog status` returns GO and #118 U4 Enforced ingest proceeds against the captured universe.

---

## Chain continuity: ARCHIVE before the first refresh (R10)

Before the first ordinary `calendar-refresh` supersedes the genesis snapshot, **archive** the
installed snapshot — a verified **copy**, never a move: the active file stays at the consumer
path until the successor's atomic install completes, so a rollback target always exists. Copy
`$LS_CALENDAR_SNAPSHOT` to an owner-local archive path under `/state`, confirm the copy is
byte-identical, then run `calendar-refresh` + the normal `calendar-activate` (stale-base check
active) to prove the repeatable-refresh path end to end. See **RUNBOOK-calendar-rollback.md** for
the rollback rehearsal that consumes this archive.

## Back-out

To back out an installed genesis snapshot before any successor exists, **delete the active file**.
Consumers then fail closed — `catalog status` returns NO-GO ("calendar unavailable") by design
(there is no weekday fallback). Re-run G4 to reinstall.

## Authorization expiry → re-genesis (KTD7)

The snapshot stamps the real agreement term, and the loader rejects an expired snapshot, so the
chain has a scheduled end at agreement expiry. Renewal is not automatic (a renewal application
opens 30 days before expiry). On renewal the snapshot must be **re-produced from genesis** with
the new term — there is no in-place re-stamp; repeat this Genesis section. Post-expiry use is
prohibited per `docs/research/krx-calendar-publication-rights.md`.

## Forward-readiness decay is expected, not a defect

After genesis the forward-readiness freshness dimension is stamped at the window end and decays
to `stale` over time. This is **usable by design** — the loader treats stale as usable, so a
`stale` outcome in §1 is not a HOLD by itself. Advancing the forward horizon is a deferred
cadence item, not part of genesis.

## 0. Preconditions

- [ ] `LS_CALENDAR_SNAPSHOT` points at the owner-local snapshot path.
- [ ] The path is under the gitignored `/state` (or `/calendar-snapshots`) tree — confirm it is
      NOT tracked (`git status --porcelain <path>` prints nothing / ignored).
- [ ] You have the current KRX data agreement's authorized `authority` label and its
      expiry/termination terms to compare against.

## 1. Load + inspect (owner-local)

```sh
cd adapters/nautilus
cargo run --release --bin calendar-status -- \
  --as-of "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --snapshot "$LS_CALENDAR_SNAPSHOT"
# (the bin takes the EXPLICIT --snapshot path — it reads no env; prints the REDACTED
#  diagnostic — authority is fingerprinted)
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
