# Runbook — session morning (catalog → calendar → mount universe)

The sequence that gets a rung-1 attended session from "yesterday's catalog" to a resolved mount
universe. Worked example throughout: **session date 2026-07-28 (Tue)**, previous session
**2026-07-27 (Mon)**. Substitute your two dates.

Companion docs: [`RUNG1-PREFLIGHT.md`](RUNG1-PREFLIGHT.md) (the preflight this feeds),
[`RUNBOOK-rung1.md`](RUNBOOK-rung1.md) (the attended session itself),
[`../RUNBOOK-calendar-snapshot.md`](../RUNBOOK-calendar-snapshot.md) (the calendar chain in full).

> **Why this exists.** "Ingest the catalog forward through the previous session" reads like a
> night-before chore. It is not: the ingest cannot advance into a date the calendar has not yet
> witnessed, and the KRX witness for a session is published **retrospectively**. So the previous
> session lands on the *morning of*, not the night before. See
> [`todays-session-cannot-be-ingested-tonight-the-krx-witness-is-retrospective`](../../../docs/solutions/workflow-issues/todays-session-cannot-be-ingested-tonight-the-krx-witness-is-retrospective.md).

---

## Step 0 — Source the credentials (once, at the top of the session shell)

The KRX and KASI keys live in `.env.calendar` at the repo root — gitignored by the same `.env.*`
rule that covers `.env.domestic`, installed `0600`. They are **not** in this file: a runbook is a
procedure, and a procedure that carries live keys cannot be committed and is one `git add -A` away
from being published.

```sh
R=/ABSOLUTE/path/to/repo
set -a; . "$R/.env.calendar"; set +a
[ -n "$LS_KRX_APPKEY" ] && [ -n "$LS_KASI_SERVICE_KEY" ] && echo "credentials loaded"
```

`set -a` exports every assignment the file makes, so both keys reach the child processes that need
them without ever appearing in an argument, a log line, or this document. If `.env.calendar` is
missing, the operator holds the values — the agent does not, and never reads them out of any file
into chat or a command.

---

## Step 1 — The decision probe (30 seconds, do this first)

One call decides the whole morning. It is read-only and touches no artifact.

```sh
# LS_KRX_APPKEY comes from Step 0; do not re-export it here.
curl -s -o /tmp/krx.json -w 'http=%{http_code} bytes=%{size_download}\n' --max-time 240 \
  -H "AUTH_KEY: $LS_KRX_APPKEY" \
  "https://data-dbg.krx.co.kr/svc/apis/sto/stk_bydd_trd?basDd=20260727"
python3 -c "import json;print('rows:',len(json.load(open('/tmp/krx.json'))['OutBlock_1']))"
```

| result | meaning | go to |
|---|---|---|
| **rows > 0** (a full session is ~943) | KRX published the previous session | Step 2 |
| **rows: 0**, `http=200`, ~17 bytes | not published yet — the catalog **cannot** advance | **Step 5** |
| `http=401` fast | key rejected | fix the credential |
| hangs / 0 bytes | endpoint degraded, not down — raise the budget and retry | — |

**Run this before any refresh.** A `200` with zero rows is a clean negative, not a failure, and no
amount of refreshing invents a witness. Skipping this probe is how you spend a chain transition
for nothing.

The `401`-without-a-key control (fast `401` = host alive and auth-gating works) is worth keeping
in your pocket: it separates "unreachable/blocked" from "reachable but slow", which every other
symptom is ambiguous about.

---

## Step 2 — Archive, then fetch inputs *(only if Step 1 returned rows)*

Archive first — a verified **copy**, never a move, so a rollback target always exists.

```sh
cd adapters/nautilus
cp state/krx.calendar.json state/krx.calendar.json.archive-20260728
cmp state/krx.calendar.json state/krx.calendar.json.archive-20260728 && echo "archive verified"
```

```sh
# Both keys come from Step 0's `set -a; . "$R/.env.calendar"; set +a`.
LS_CALENDAR_HTTP_TIMEOUT_SECS=180 \
  cargo run --release --bin calendar-fetch-inputs -- \
  --window 2026-07-27..2026-07-27 --krx-through 2026-07-27 \
  --inputs-out state/refresh-20260728.inputs.json \
  --state state/refresh-20260728.ckpt --pace-ms 500
```

Success is `source krx-daily ok=true covered=[...]`.

`ok=false ... failed=error sending request` is the **client-side timeout trap**, not a dead
source: the KRX daily endpoint has been observed at 14–59 s per day under load. Raise
`LS_CALENDAR_HTTP_TIMEOUT_SECS` and re-run — the checkpoint resumes, so only un-fetched days
cost anything. See
[`krx-client-side-timeout-manufactures-a-dead-source-and-a-witness-less-snapshot`](../../../docs/solutions/integration-issues/krx-client-side-timeout-manufactures-a-dead-source-and-a-witness-less-snapshot.md).

Credentials ride in the process env only — never an argument, never a stageable file. Their one
at-rest home is the gitignored `0600` `.env.calendar` of Step 0.

## Step 3 — Build the candidate, then activate

```sh
cargo run --release --bin calendar-refresh -- \
  --active state/krx.calendar.json --as-of "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --mode incremental --through <operating horizon> \
  --inputs state/refresh-20260728.inputs.json
```

Want `partial=false`, with a `status_established` / `new_evidence` diff entry naming
`2026-07-27`.

> **Do not activate a `partial=true` candidate.** `partial:source-failure` is an *acknowledgeable*
> key, which is the trap: acknowledging it consumes the chain transition, changes `artifact_id`,
> and leaves every consumer exactly as blocked as before.

Then activate with `calendar-activate` (stale-base check active).

> ⚠️ **Activate BEFORE the operator authors the attended Unknown override.** Activation changes
> `artifact_id`, and an override is bound to the in-force snapshot identity — authoring it first
> invalidates it.

---

## Step 4 — Advance the catalog *(agent-runnable)*

Bounded to the catalog's existing symbols. **Never run this unbounded**: catalog membership is an
input to `select_universe`, so an unbounded accumulate re-composes the head's tradable universe.
See [`unbounded-accumulate-ingest-widens-the-catalog-and-moves-the-head-universe`](../../../docs/solutions/workflow-issues/unbounded-accumulate-ingest-widens-the-catalog-and-moves-the-head-universe.md).

```sh
cd adapters/nautilus
R=/ABSOLUTE/path/to/repo
SYMS=$(python3 -c "
import json;w=json.load(open('$R/data/turn4-fresh/catalog/ingest-checkpoint.json'))['watermarks']
print(','.join(sorted({k.split('.')[0] for k in w if k.endswith('1-DAY')})))")

LS_TRADING_ENV=paper \
LS_INGEST_LANE_FILE=$R/.env.domestic \
LS_CALENDAR_SNAPSHOT=$R/adapters/nautilus/state/krx.calendar.json \
LS_SPEND_LEDGER_FILE=$R/data/turn4-fresh/state/spend-ledger.json \
LS_INGEST_CATALOG=$R/data/turn4-fresh/catalog \
LS_NODE_LOCK_DIR=$R/data/turn4-fresh/catalog \
LS_INGEST_KIND=daily LS_INGEST_MODE=accumulate \
LS_INGEST_SKIP_UNIVERSE_LOAD=1 LS_INGEST_LOOKBACK=<catalog coverage start> \
LS_INGEST_SYMBOLS="$SYMS" \
  ./target/debug/ls-ingest
```

`LS_INGEST_SKIP_UNIVERSE_LOAD=1` refuses without an explicit symbol list, so the two flags
together make universe expansion structurally impossible. It also drops `t8430` + 2× `t9945`, the
dominant avoidable `IGW00201` cost.

**Verify with the watermark, never the exit code** — `exit 0 / 0 bars / N skipped` is the
signature of a fully-blocked run *and* a fully-up-to-date one:

```sh
python3 -c "
import json,collections
d=json.load(open('$R/data/turn4-fresh/catalog/ingest-checkpoint.json'))
print(collections.Counter(v for k,v in d['watermarks'].items() if k.endswith('1-DAY')))
print('gaps',d['gaps'],'shifted',d['shifted'])"
```

Want every daily watermark at the previous session and `gaps`/`shifted` empty. A mixed
distribution is a partial run — resume, do not proceed.

Traps: `LS_INGEST_LANE_FILE` must be **absolute** (read CWD-relative, no upward search);
`LS_CALENDAR_SNAPSHOT` unset reads as enforced-fail-closed and refuses every dispatch; run from
`adapters/nautilus`, never the repo root; a killed run leaves `$CATALOG/.ls-ingest.lock`.

### Optional sanity check

```sh
LS_DATA_HOME=$R/data/turn4-fresh \
LS_CALENDAR_SNAPSHOT=$R/adapters/nautilus/state/krx.calendar.json \
  ./target/debug/lab-research catalog status
```

Two invocation traps, both producing a NO-GO that says nothing about the catalog:

`LS_CALENDAR_SNAPSHOT` is mandatory — without it every symbol reports `calendar unavailable`, a
different NO-GO with the same headline.

**Do not set `LS_STATUS_SDATE`/`LS_STATUS_EDATE` here.** They are not a query filter: an expected
range is a *whole-catalog* span assertion applied to every `(instrument, bar-kind)` series
regardless of bar kind. This catalog's 75 `1-MINUTE` series are deliberately frozen weeks behind
the daily ones, so any daily-derived range flags all of them and forces NO-GO no matter how
healthy the daily frontier is. The form above is watermark-gated — each series is judged against
its own watermark — which is what makes its verdict about catalog health. It also never reaches
today's unprovable boundary, so the `calendar indeterminate` family cannot fire either. See
[`bounding-catalog-status-with-an-expected-range-forces-no-go-on-a-mixed-bar-kind-catalog`](../../../docs/solutions/workflow-issues/bounding-catalog-status-with-an-expected-range-forces-no-go-on-a-mixed-bar-kind-catalog.md).

---

## Step 5 — The fidelity decision *(only if Step 1 returned 0 rows)*

**The session is not blocked.** Eligibility is staleness, not same-day currency:
`(session_date − prior_date) <= MAX_PRIOR_STALENESS_DAYS` (10), and `select_prior` takes the
latest daily bar *strictly before* the session date. A Tuesday session with only Friday on disk
is 4 days — it resolves.

The cost is fidelity, not availability: `prior_close` / `prior_atr` come from Friday, so the
head's overnight-gap term becomes a multi-session return that clears the gap floor too easily.

**Decide deliberately — proceed, or postpone to a day with a clean prior.** Do not discover this
from the fill log.

---

## Step 6 — Resolve the mount universe — **after 09:00 KST**

```sh
LS_DATA_HOME=/ABSOLUTE/path/to/data-home \
LS_MOUNT_UNIVERSE_DATE=2026-07-28 \
LS_MOUNT_UNIVERSE_METADATA=/ABSOLUTE/path/to/universe-metadata-YYYYMMDD.json \
LS_DISPATCH_LANE_ENV=/ABSOLUTE/path/to/.env.domestic \
  cargo run --release -p nautilus-ls-lab --bin lab-mount-universe -- --out <path>
```

All four are load-bearing; three fail closed. `LS_DATA_HOME` is unconditionally required.
`LS_MOUNT_UNIVERSE_METADATA` is required because the head is metadata-driven — without it the
producer refuses rather than silently dropping the tradability gate.

**Must run after 09:00.** Before the opening auction t8407 answers with the *previous* session's
snapshot, whose `open` is a perfectly positive integer, so the producer refuses on the clock
rather than silently resolving yesterday's opens.

Point `LS_MOUNT_UNIVERSE_FILE` at `--out` for the attended `--mount`. Never hand-author the file
— a row missing `prior_atr` silently disables the armed OR-width gate.

---

## Step 7 — The attended session (operator-only)

`--genesis` → `--dispatch` → `--mount`, per [`RUNBOOK-rung1.md`](RUNBOOK-rung1.md). The current
day reads `Unknown` in the calendar — that is the normal morning state, and only the bound,
audited attended override proceeds it. **The agent never authors or supplies it.**

**Exit codes are the contract — never read success from log text.**

| command | exit | meaning |
|---|---|---|
| `--dispatch` | 0 / 1 / 75 | green / refused / throttled (re-run, never terminal) |
| `--mount` | 0 / 66 / 71 / 72 / 77 | ran + finalized clean / not-paper / precheck failed (dispatch NOT consumed) / ran but **ABNORMAL** / no-TTY |

`72` is **never** success: the kill switch is engaged and reds the next `--dispatch` until a
nonce-gated `--clear-killswitch`. Only `71` and `77` leave a green dispatch unconsumed.

---

## Post-close

Ingest → tracking → `--rung-report`. Note that the session you just traded will not be
ingestible until *its* witness publishes — which returns you to Step 1 tomorrow.
