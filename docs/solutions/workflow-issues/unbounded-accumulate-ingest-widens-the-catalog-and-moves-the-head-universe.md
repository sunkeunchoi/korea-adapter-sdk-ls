---
title: "An unbounded `ls-ingest accumulate` silently widens the catalog — and catalog membership is an input to the head's tradable universe, not just disk usage"
date: 2026-07-27
category: workflow-issues
module: "adapters/nautilus ls-ingest (src/bin/ls-ingest.rs: accumulate mode, LS_INGEST_SYMBOLS, LS_INGEST_SKIP_UNIVERSE_LOAD), lab mount-universe producer (lab/src/runner/mount_universe.rs), lab universe selection (lab/src/strategy/orb.rs: select_universe)"
problem_type: workflow_issue
component: tooling
severity: high
applies_when:
  - "Running `ls-ingest` in accumulate mode to catch a catalog up to the last closed session"
  - "A catalog that backed a certified backtest head suddenly contains far more symbols than expected"
  - "lab-mount-universe starts selecting different symbols for a date it previously resolved"
  - "Deciding whether an ingest run needs LS_INGEST_SYMBOLS bounding"
  - "Recovering a catalog after an ingest wrote symbols that were never part of the head's universe"
related_components:
  - ls-ingest
  - lab-mount-universe
  - production-ladder
  - universe-metadata-pin
tags:
  - ls-ingest
  - accumulate
  - catalog-membership
  - head-fidelity
  - universe-selection
  - igw00201
  - blast-radius
---

## Problem

`ls-ingest` in `accumulate` mode with no `LS_INGEST_SYMBOLS` does not mean "catch up the
symbols already in this catalog". It means "catch up **the whole loaded universe**" — it runs
the universe load (`t8430` + 2× `t9945`), gets every domestic-equity instrument, and backfills
each unseen one from the `LS_INGEST_LOOKBACK` floor.

The catalog is not just storage. `lab-mount-universe` builds its candidate set from *every*
instrument in the catalog, and `select_universe` takes the top-N by turnover. So widening the
catalog **re-composes the traded universe**, silently.

## Symptoms

Observed 2026-07-27 on a catalog that backed the certified v34 head:

- Run intended to add two missing sessions for 75 symbols; loaded **4293** instruments.
- Killed partway, but 145 new symbols had already landed: catalog **75 → 220** symbols.
- Producer output for the same past date changed from **27 → 39** symbols (15 added, 3 lost).
- Meanwhile only **3 of the original 75** reached the target date — the run spent its paced
  budget backfilling strangers instead of doing the job it was started for.

Nothing errored. `lab-research catalog status` reported **GO** throughout.

## What Didn't Work

**Reading the log.** `loaded 4293 domestic-equity instruments` is one line among startup noise
and reads as informational. The damage signal is not in the output; it is in the *watermark
count* (150 → 295) and only becomes visible if you compare producer output before and after.

**Assuming "accumulate" is scoped to what exists.** The name suggests incremental catch-up of
the current set. The mode doc says otherwise — *"else the whole loaded universe"* — but the
default is the dangerous one.

**Reasoning about blast radius as disk/time.** The real cost is not gateway budget or storage;
it is that the head's tradable universe moved. That consequence is invisible unless you already
know that catalog membership feeds selection.

## Solution

**Bound every catch-up ingest to the symbols already in the catalog:**

```sh
SYMS=$(<list of the catalog's existing shcodes, comma-separated>)
LS_INGEST_MODE=accumulate \
LS_INGEST_SYMBOLS="$SYMS" \
LS_INGEST_SKIP_UNIVERSE_LOAD=1 \
LS_INGEST_KIND=daily \
LS_INGEST_LOOKBACK=<the catalog's own coverage start> \
  ./target/release/ls-ingest
```

`LS_INGEST_SKIP_UNIVERSE_LOAD=1` is the belt-and-braces half: it refuses without an explicit
symbol list, so the two flags together make universe expansion structurally impossible. It also
removes the `t8430` + 2× `t9945` calls, the dominant avoidable `IGW00201` cost.

**Recovering from an unbounded run.** The newly-added symbols are separable because they were
backfilled *from the lookback floor*, so their coverage start differs from the original set's:

```sh
# original symbols started 2026-05-18; everything starting at the LS_INGEST_LOOKBACK
# floor (2026-06-01) was added by the runaway run
lab-research catalog status | grep "1-DAY:"   # read the coverage START per symbol
```

Then, under the data home's catalog directory (`$LS_DATA_HOME/catalog/`, gitignored runtime
state — not a repo path), remove each stray symbol's `data/bars/<ID>-1-{DAY,MINUTE}-LAST-EXTERNAL`
directory and prune its entries from `ingest-checkpoint.json` (`watermarks`, `history_floors`,
`empty_retries`). Back the checkpoint up first. Verify by re-running the producer for a past
date and diffing against a known-good output — that, not the file count, is the real check.

## Why This Works

The coverage-start discriminator works because `accumulate` backfills an *unseen* instrument
from `LS_INGEST_LOOKBACK`, while the pre-existing set carries whatever start its original
backfill used. Two different provenances produce two different start dates, so the sets are
mechanically separable without needing a record of what was there before.

Bounding works because the expansion has exactly one source: the per-invocation universe load.
Remove it (or override its output with an explicit list) and accumulate can only advance
triples that already exist.

## Prevention

**Treat catalog membership as part of the head's behavioral surface.** The universe-metadata
artifact pins *tradability tiers*, not the candidate pool. A symbol that enters the catalog
becomes a selection candidate, and selection is top-N — so adding symbols does not merely add
options, it can **displace** ones the head would have traded. Any operation that changes which
instruments are in the catalog deserves the same care as a params change.

**Snapshot before ingesting.** `cp catalog/ingest-checkpoint.json{,.bak-YYYYMMDD}` costs
nothing and is the difference between a surgical revert and a guess.

**Verify with the consumer, not the store.** After any ingest touching a catalog that backs a
head, re-run `lab-mount-universe` for a past date and diff it against the previous output. A
byte-identical result is the only cheap proof that selection did not move; symbol counts and
`catalog status: GO` will not tell you.

**Beware the shell traps that hide during cleanup.** Two bit this recovery: `while read` skips
a final line with no trailing newline (one stray symbol survived), and `awk`'s `split(s, a, "..")`
treats `..` as a *regex* matching any two characters, silently mis-parsing every `START..END`
coverage range. Prefer explicit parsing when the output drives a delete.

## Related

- [`ls-gateway-igw00201-bulk-minute-ingest-drip-feed`](../integration-issues/ls-gateway-igw00201-bulk-minute-ingest-drip-feed.md)
  — why the universe load is the dominant avoidable budget cost
- [`re-ingesting-an-overlapping-range-duplicates-catalog-bars`](../logic-errors/re-ingesting-an-overlapping-range-duplicates-catalog-bars.md)
  — the other way an ingest corrupts a catalog (duplicate bars, not extra symbols)
- [`mount-universe-producer-cannot-be-fed-on-a-session-morning`](../architecture-patterns/mount-universe-producer-cannot-be-fed-on-a-session-morning.md)
  — the producer whose output moved
