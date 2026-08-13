# RUNBOOK — windowed daily backfill (`LS_INGEST_MODE=backfill`)

Plan: `docs/plans/2026-08-13-001-feat-daily-catalog-2016-floor-pull-plan.md` (P3).
Attended, paper-only, multi-session. Read this whole file before the first
session.

## What this mode is for, and why the others cannot do it

`range` and `accumulate` both acquire history by issuing **one wide
`collect_daily` range**. P4 measured (2026-08-12, 583 calls, zero anomalies)
that a wide-range `t8410` request serves only the newest ~501 rows and echoes a
**clean empty `cts_date` cursor** — completion evidence that lies. Seeding a
2016-floor catalog through either of them would append about two years of bars,
advance the watermark, and attest ten missing years.

History below that cap is reachable only through explicit calendar-snapped
windows of at most 450 proven sessions. `backfill` walks each manifest symbol's
windows oldest-first and appends one verified window at a time.

While a catalog carries an unfinished backfill, `range` (the default),
`accumulate`, and `rebase` all **refuse to run against it** — the
`backfill_incomplete` marker in `ingest-checkpoint.json`. The refusal names the
catalog and prints `BACKFILL INCOMPLETE`.

## Preconditions for every session

- `LS_TRADING_ENV=paper`, credentials from a lane env-file.
- `LS_CALENDAR_SNAPSHOT` set — the mode fails closed without a calendar.
- **Strictly after the morning chain finishes, and with no live mount.** The
  `IGW00201` budget is per-credential and **cumulative**; the advisory lock is
  per-catalog-dir, so it gives *no* protection against a concurrent run against
  the other catalog home. This sequencing is operator-enforced, not
  machine-enforced.
- One **absolute** `LS_SPEND_LEDGER_FILE`, the same path the morning chain's
  `ls-ingest` invocations use. That shared file is the only cross-session,
  cross-home spend memory.

## Session command

```
LS_TRADING_ENV=paper LS_INGEST_LANE_FILE=.env.domestic \
LS_CALENDAR_SNAPSHOT=<snapshot> \
LS_SPEND_LEDGER_FILE=/abs/path/state/spend-ledger.json \
LS_INGEST_MODE=backfill \
LS_INGEST_CATALOG=<repo>/data/next-daily-2016/catalog \
LS_BACKFILL_MANIFEST=lab/config/pit-universe-20260812.json \
LS_BACKFILL_BATCH=120 \
  cargo run --bin ls-ingest
```

Add `LS_INGEST_SKIP_UNIVERSE_LOAD=1` from the **second** session onward (see
Bootstrap). `LS_BACKFILL_SYMBOLS=<comma list>` overrides the batch selection
when re-running specific symbols.

### Bootstrap — the first session only

The first invocation runs the 3-call universe load (`t8430` + 2× `t9945`) and
writes the instrument definitions. A definition-less home makes lab backtests
silently empty, so this must happen once. Every later invocation sets
`LS_INGEST_SKIP_UNIVERSE_LOAD=1`, which makes universe expansion structurally
impossible for the rest of the pull.

### The home must be fresh

The first session refuses a catalog that already holds daily bars it did not
put there ("Point `LS_INGEST_CATALOG` at a fresh home"). The resume path maps
stored coverage back onto the plan's windows to recover the watermark, and
foreign coverage would map onto the wrong window and skip everything below it.
Use a new directory; do not point this at `data/turn4-fresh/catalog`.

### Batching

Any prefix is safe. Every window is independently appended and checkpointed, so
a session may be killed at any point and lose at most the window in flight.
`LS_BACKFILL_BATCH=<n>` takes the first N manifest members not yet at the
anchor.

Symbols are **atomic within a session** where possible: a symbol resumed on a
*later* day re-runs the 5-trading-day overlap check at its watermark first. A
clean overlap continues mid-symbol; a shifted one wipes the series and restarts
that symbol from its range start rather than splicing two adjustment bases.

## Reading progress — never the exit code

`ls-ingest` exits 0 both when a run is caught up and when it is fully blocked.
Progress is a **watermark census** over `ingest-checkpoint.json`: count the
`005930.XKRX|1-DAY` style watermark entries equal to the anchor.

```
jq -r '.watermarks | to_entries[] | select(.key | endswith("|1-DAY")) | .value' \
  <catalog>/ingest-checkpoint.json | sort | uniq -c
```

The session's own summary prints bars, windows, symbols complete, and one line
per `RESTARTED` / `UNCOVERED GAP` / `APPEND REFUSED` / `DEGRADED`.

## When a symbol degrades

A degradation is never fatal to the run: the symbol's watermark holds below the
window that stopped it, the reason is persisted in the checkpoint, and the run
moves on. Re-run the degraded symbols with `LS_BACKFILL_SYMBOLS` before the
final report.

| Line | Meaning | Action |
|---|---|---|
| `UNCOVERED GAP` | the window completed cleanly with zero rows through the bounded re-fetch | re-run the symbol later; if it persists, the gap is real and belongs in the evidence record |
| `DEGRADED: … repeated cursor` / `zero-row page with a live cursor` / `page cap` | suspect truncation — nothing was appended | re-run the symbol; the gateway serves transiently empty pages |
| `DEGRADED: … IGW00201` | the throttle budget stayed dead through the bounded backoff | stop the session; resume on a cold budget window |
| `APPEND REFUSED (overlap)` | an overlap the window trim did not anticipate | `lab-research catalog compact`, or wipe the series and re-pull it |
| `RESTARTED` | a cross-day basis shift was detected and the symbol was re-pulled whole | informational — this is the protection working |

## Closing the rung

1. Every manifest symbol at the anchor → the session prints that the marker is
   **CLEARED**.
2. Run the offline completeness report:

```
LS_TRADING_ENV=paper LS_CALENDAR_SNAPSHOT=<snapshot> \
LS_INGEST_MODE=backfill-report \
LS_INGEST_CATALOG=<repo>/data/next-daily-2016/catalog \
LS_BACKFILL_MANIFEST=lab/config/pit-universe-20260812.json \
  cargo run --bin ls-ingest
```

   It writes `lab/config/daily-catalog-<floor>-<anchor>.json` and prints one
   `ANOMALY:` line per finding plus a `VERDICT: GO` / `NO-GO`. On GO it writes
   the manifest pin into the catalog; on NO-GO the pin is **withheld** — a pin
   from a non-clean state would attest a membership whose bars never landed.

   Do **not** substitute the uniform `expected_range` form of
   `catalog status`: it applies one range to every triple and NO-GOes all 108
   post-floor listings (see
   `docs/solutions/workflow-issues/bounding-catalog-status-with-an-expected-range-forces-no-go-on-a-mixed-bar-kind-catalog.md`).

3. Confirm the watermark-gated `catalog status` form reads GO.
4. Commit the evidence record and the `lab/TURN-LOG.md` entry — `data/` is
   gitignored, so the committed record is the durable form.
5. Close the queue item: `lab-next done daily-catalog-2016-floor-pull`. Never
   hand-edit `queue/items.jsonl`.

## Not yet wired

- `make next` reads one `LS_DATA_HOME` and its ingest resume text names
  accumulate; this runbook carries the backfill resume commands until a second
  data home has a reader.
- The `BACKFILL INCOMPLETE` literal is not in the morning preflight registry
  (`scripts/session-morning.sh`) — the morning chain never runs backfill.
- Forward accrual past the anchor (hooking the new home into the morning chain,
  and the marker's successor semantics) is P6/P7 work.
