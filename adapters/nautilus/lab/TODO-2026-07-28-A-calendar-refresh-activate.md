# TODO A — Front-load KRX calendar refresh + activate (tonight)

Created: 2026-07-28
Deadline: before 2026-07-29 09:00 KST (KRX open). Best run after ~18:00 KST when the 07-28 witness is published.
Effort: ~10 min. Network: KRX/KASI APIs only — **never the LS gateway**.

## Why

The active calendar (`state/krx.calendar.json`, artifact `818dcd74…`) has forward-freshness **Stale**; 2026-07-28 and 2026-07-29 are **Unknown**. Tomorrow's ingest needs **2026-07-28 = trading_session** (its `edate` / last-closed session). Certifying it tonight takes the whole refresh+activate step off tomorrow morning's critical path. (2026-07-29 itself stays Unknown until its own witness exists — every morning runs in that state; not a blocker.)

## Preconditions

- [ ] `set -a; . "$R/.env.calendar"; set +a` in the executing shell — exports `LS_KRX_APPKEY` and `LS_KASI_SERVICE_KEY` (both required; credentials ride **only** in env, never in arguments). `.env.calendar` is the gitignored `0600` file at the repo root, per Step 0 of [`RUNBOOK-session-morning.md`](RUNBOOK-session-morning.md); no credential lives in any runbook or TODO text.
- [ ] Working dir: `adapters/nautilus` (all commands below relative to it).
- [ ] Binaries are fresh (rebuilt 2026-07-28 evening from post-#228 HEAD `5f38144`).

STATUS: **COMPLETE 2026-07-29 08:53 KST.** KRX published the 07-28 witness at ~08:50 KST (T+1 morning, found on the overnight watcher's 8th hourly attempt). Full chain executed: refresh (diff: 2 entries, 0 high-risk, not partial, no regressions) → approval `state/refresh-20260729.approval.json` (note: `refresh-20260728.approval.json` was already taken by YESTERDAY's run — the convention keys on run date, not through-date) → activated artifact `42dd7de2…` → verified `2026-07-28 → TradingSession`. Morning runbook calendar step is now a no-op. Earlier attempt log follows.

EARLIER (2026-07-28 ~19:05 KST): step 1 executed — credentials verified, fetch pipeline works (47 evidence records; `krx-witness-2026-07-27` positive; weekend rules OK). **No `krx-witness-2026-07-28` as of attempts at 17:45 / 18:10 / 18:35 / 19:00 KST** — KRX publishes today's daily data later than early evening. Abort criterion honored: nothing refreshed/activated. Working artifacts: `state/refresh-20260728.calendar-inputs.json` + `.ckpt` (safe to overwrite on retry; delete the ckpt first to force a full re-fetch). If tonight's late watch also misses, run steps 1–6 in the morning — the witness will exist by then.

## Steps

1. **Fetch witness inputs** (the only networked step; KRX auth = `AUTH_KEY` header, KASI = `serviceKey` query param):
   ```sh
   ./target/debug/calendar-fetch-inputs \
     --krx-through 2026-07-28 \
     --inputs-out state/agentfull.calendar-inputs.json \
     --state-root state
   ```
   Accepted args (verified): `--window / --krx-through / --inputs-out / --state / --state-root / --pace-ms`. Precedent artifacts: `state/agentfull.calendar-inputs.json`, `state/agentfull.calendar-fetch.ckpt`.

2. **Refresh → candidate** (offline; reads the inputs file):
   ```sh
   ./target/debug/calendar-refresh \
     --active state/krx.calendar.json \
     --as-of "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
     --mode incremental \
     --through 2026-07-28 \
     --inputs state/agentfull.calendar-inputs.json
   ```
   Modes (verified): `incremental|full`. Produces `state/krx.calendar.json.candidate` + `state/krx.calendar.json.candidate.diff.json`.

3. **Review the diff** (`state/krx.calendar.json.candidate.diff.json`). Expect: small entry count (2026-07-28 Unknown→trading_session plus freshness bookkeeping), 0 high-risk, 0 alerts, not partial, **no status change to any previously-established date**.

4. **Author the approval JSON** binding the exact candidate — `state/refresh-20260728.approval.json` (schema per precedent `state/refresh-20260727.approval.json`):
   ```json
   {
     "operator": "sunkeunchoi",
     "reason": "Incremental refresh through 2026-07-28: establishes krx-witness for 2026-07-28 so it reads trading_session, front-loading tomorrow's runbook calendar step. Diff reviewed: <N> entries, 0 high-risk, not partial, 0 alerts; no status regressions.",
     "approved_at": "<now UTC RFC3339>",
     "reviewed_artifact_id": "<artifact_id FROM THE CANDIDATE — copy exactly>",
     "acknowledged": []
   }
   ```

5. **Activate**:
   ```sh
   ./target/debug/calendar-activate \
     --active state/krx.calendar.json \
     --candidate state/krx.calendar.json.candidate \
     --approval state/refresh-20260728.approval.json \
     --as-of "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
   ```

6. **Verify**:
   ```sh
   ./target/debug/calendar-status --as-of "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
     --snapshot state/krx.calendar.json --day 2026-07-28
   ```
   Done when: `day: 2026-07-28 → Trading Session` and the new `artifact_id` differs from `818dcd74…`.

## Abort criteria (clean stop — nothing lost, runbook does it tomorrow)

- Fetch fails or KRX hasn't published the 07-28 witness → the diff will NOT show 07-28→trading_session. **Stop before step 4.** Do not activate a candidate that doesn't certify 07-28.
- Diff shows any high-risk entry, alert, or regression on an established date → stop, keep the candidate + diff for review, do not approve.
- Recovery exists if needed: `calendar-rollback` + archives (`state/krx.calendar.json.archive-*`, `.prior`).

## Constraints

- No LS gateway calls. Never read/edit `lab/RUNBOOK-session-morning.md`. No writes under `data/turn4-fresh/` or the repo-root data home. Credentials never in args, files, or output.
