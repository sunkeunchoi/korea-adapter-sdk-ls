# nautilus-ls — nautilus_trader v2 adapter for LS Securities (Korea)

A standalone Cargo workspace that lets [nautilus_trader](https://nautilustrader.io)
v2 (Rust) backtest and paper-trade **domestic KRX cash equities** through the LS
SDK (`ls-sdk` / `ls-core`). It is a translation layer: it owns no transport,
credentials, or rate limiting of its own — `ls-core` remains the single transport
and safety authority (rate buckets, kill switch, order dedup, preflight,
ambiguous-order fail-closed).

v1 ships domestic equities certified; domestic F/O and overseas domains are
mapped-for but not built.

## Why a nested workspace

`adapters/nautilus/Cargo.toml` carries its own `[workspace]` table (opting out of
the root SDK workspace) plus its own `Cargo.lock` and `rust-toolchain.toml` pinning
Rust **1.96**. nautilus 0.60.0 requires Rust 1.96 / edition 2024, while the six SDK
crates pin 1.75 / edition 2021. The nested table achieves the isolation with **zero
edits** to the SDK crates or the root `Cargo.toml`. The SDK is consumed by path
(`ls-sdk`, `ls-core`; dev-only `ls-sdk-test-support`).

## Trait-shape verification (0.60.0, verified 2026-07-02)

The adapter contract was verified against the published `=0.60.0` source:

- `DataClient` / `ExecutionClient` live at `nautilus_common::clients` and are
  `#[async_trait(?Send)]`. Required methods (no default): `DataClient` =
  `client_id / venue / start / stop / reset / dispose / is_connected /
  is_disconnected`; `ExecutionClient` = `is_connected / client_id / account_id /
  venue / oms_type / get_account / generate_account_state / start / stop`. All
  subscribe/request and order-command methods are provided (default-no-op)
  overrides.
- **Order events flow through `ExecutionEventEmitter`** (`nautilus-live`), not
  through per-transition trait methods — the trait carries only
  `generate_account_state` plus the async report generators.
- Factories: `DataClientFactory` / `ExecutionClientFactory` / `ClientConfig` in
  `nautilus_common::factories`; the data factory's `create` also takes a
  `clock: Rc<RefCell<dyn Clock>>`, the exec factory's does not. Config is passed as
  `&dyn ClientConfig` and downcast via `as_any().downcast_ref::<LsAdapterConfig>()`.
- `LiveNode::builder(trader_id, Environment) -> LiveNodeBuilder`
  (`.add_data_client` / `.add_exec_client` / `.build`), pure-Rust.
- `get_data_event_sender()` / `get_exec_event_sender()` at
  `nautilus_common::live::runner` **panic** if the runner is uninitialized; tests
  and tester binaries bind a sender first (`AsyncRunner::bind_senders()` or
  `replace_*_event_sender`).
- `ParquetDataCatalog::new` + `write_instruments` / `write_to_parquet::<Bar>` /
  `bars(...)`; `Equity::new_checked` with `tick_scheme: Option<Ustr>`;
  `BacktestEngine` / `BacktestNode`.

`LsAdapterConfig` implements `ClientConfig` (`src/config.rs`); the workspace builds
clean (`cargo build`) and the trait-shape claims above are exercised by the offline
test suite.

## Building & testing

```
cd adapters/nautilus
cargo test --workspace   # offline, no credentials, no network
cargo build --bins       # ls-ingest, node_data_tester, node_exec_tester
```

### Calendar Foundation Gate

```
make foundation-gate     # from the repo ROOT (mirrors `make adapter-check`)
```

The **Calendar Foundation Gate** (issue #189) is the named, reproducible offline invocation that
proves the shared KRX calendar machinery is trustworthy before any consumer's weekday-era
workaround is retired. It asserts the entire calendar surface — core, refresh, activation,
diagnostics (the ten `calendar status` outcomes, redacted), the reproducible fixtures, the six
consumer seams, the composition roots, the failure-inversion suite, the traceability drift check
(`nautilus-ls-calendar/TRACEABILITY.md`), the rollback rehearsal, and the Shadow-divergence
classification — fixed-clock, with no production snapshot, credentials, network, or real
KRX-derived rows. It also runs the closeout publication-boundary scan, which fails the gate if
[`CLOSEOUT.md`](CLOSEOUT.md) leaks a snapshot identity or an ISO date (the closeout is
verdict-only). Passing the Foundation Gate does NOT by itself authorize enforcing any consumer —
that is each consumer's own **Consumer Retirement Gate** (an operator-attended live ceremony; see
the retirement runbooks).

## Operator run-book (live, paper-only)

All three binaries are **operator-gated**: paper-only, session-windowed, and never
run by the offline gate. Each installs credential scrubbing + dispatch-log
suppression before any output, and refuses to run unless `LS_TRADING_ENV=paper`.

### Lanes & credentials

Credentials come from a gitignored per-lane env file (`.env.domestic` for domestic
equities), sourced by the shell or passed as `LS_*_LANE_FILE`. Never commit an env
file; never print credentials (the scrub module masks account numbers and bearer
tokens/appkeys out of all output).

### R15 mutual exclusion (ingest ↔ live)

Rate buckets are **per-process**, so bulk ingestion and a live session must not run
concurrently against the gateway. `ls-ingest` and the tester binaries each take an
advisory lockfile beside the catalog (`.ls-ingest.lock` / `.ls-live.lock`) and
**refuse to start while the counterpart lock is held**. A stale lock from a crash
blocks until cleared manually (`rm <catalog>/.ls-*.lock`) — a deliberate fail-safe.

### Offline KRX calendar (adoption states)

Two consumers — `lab-research catalog status` (the ingest→backtest go/no-go) and
`budget-probe` (the IGW00201 measurement) — read the shared offline KRX calendar
(`nautilus-ls-calendar`, issues #184/#185, PR #190) at their composition root. Two env
vars configure it, both resolved once per invocation (single load, single as-of):

- `LS_CALENDAR_SNAPSHOT` — an **explicit** path to an immutable, authorized calendar
  snapshot. Unset/empty means no snapshot configured (the normal slice-deploy state);
  the calendar core never picks a default path. A missing/failed snapshot is non-fatal
  under Legacy/Shadow and fail-closed under Enforced.
- `LS_CALENDAR_ADOPTION` — `legacy` | `shadow` | `enforced`. Unset/invalid → the
  **composed default `shadow`**. The live Enforced cutover and a production snapshot are
  the deferred Consumer Retirement Gate (#189); the composed default stays Shadow.

Every invocation emits one redacted, decision-relevant **startup record** to stderr
(never a persisted artifact): adoption, snapshot identity, coverage, freshness, the
factual query, and the resulting action — with authority/credential identities masked.

> **`LS_CALENDAR_ADOPTION` is process-wide.** Setting it to `enforced` flips **every**
> not-yet-retired consumer in that process at once — during a canary, use per-consumer scoping
> (a dedicated invocation), never a global `enforced` flip. Independence between consumers is
> achieved by per-consumer arm removal in each staged retirement diff, not by this env var.

**Retirement (issue #189).** Each consumer advances Shadow → **Enforced** only through its
operator-attended **Consumer Retirement Gate**, after which its weekday primitive and stale
guidance are removed. The operator runbooks:
[production-snapshot validation](RUNBOOK-calendar-snapshot.md),
[activation + rollback rehearsal](RUNBOOK-calendar-rollback.md), and the generic
[Consumer Retirement Gate template](RUNBOOK-consumer-retirement-gate.md) (per-consumer:
`RUNBOOK-retire-{ingest,catalog,budget-probe,ladder}.md`). Merge of a staged retirement diff is
blocked **mechanically** — `make merge-block-check` fails any diff that deletes a consumer's
weekday primitive unless its committed [`gate-verdicts/<consumer>.json`](gate-verdicts/README.md)
is present and `PASS` (written only after the live gate). The public
[`CLOSEOUT.md`](CLOSEOUT.md) is verdict-only.

**`catalog status` behavior (Enforced, after the #189 U7 Consumer Retirement Gate):**
watermark and expected-range boundaries resolve against **proven Trading Sessions** — a real
holiday closure no longer false-flags; a boundary-relevant Unknown is `NO-GO — calendar
indeterminate`; an out-of-coverage/unavailable boundary is `NO-GO — calendar unavailable`; a
stale-but-established boundary is a **GO** with a prominent warning naming the freshness
dimension(s) that bound the queried dates. There is no weekday fallback; the
`last_weekday_on_or_before` walk-back is retired.

**`budget-probe` behavior (Enforced, after the #189 U8 Consumer Retirement Gate):** automatic
selection is the **most recent proven Trading Session**, and the probe makes **no gateway call**
when none can be proven (or the calendar is unavailable) until an explicit
`LS_PROBE_SDATE`/`LS_PROBE_EDATE` range is supplied. An explicit range is an auditable
**bypass** (the preserved reproducibility/recovery lever, KTD8) — it records the operator, run
context, and the calendar condition automatic selection skipped, and changes neither probe
status nor any dispatch authorization. The weekday `recent_trading_day` anchor is retired.

**`ls-ingest` calendar behavior (Enforced-only after the #189 U6 Consumer Retirement Gate).**
`ls-ingest` reads the shared calendar, resolving `LS_CALENDAR_SNAPSHOT` once per invocation after
acquiring the ingest lock and evaluating it at one fixed instant. The weekday primitive and the
`LS_CALENDAR_ADOPTION` seam are retired for ingest — the date decision is always Enforced:

- Automatic `accumulate`, `probe-lookback`, and `rebase` fail closed before SDK construction when
  the explicit snapshot is missing, unauthorized, expired, or cannot cover the relevant date; there
  is no weekday fallback. The 16:30 KST close buffer determines only the latest eligible civil date,
  then calendar facts select the established prefix: requests end on the last reachable Trading
  Session, proven trailing Closed dates advance only after successful coverage, and scanning stops
  before the first Unknown/unavailable date. An all-Closed prefix can advance without a gateway
  request; `probe-lookback` cannot jump across an Unknown boundary; `rebase` performs the same
  admission before it writes epoch marks or wipes a series.
- Manual `range` mode (`SDATE`/`EDATE`) never consults the calendar for a date decision; its legacy
  `completed`→`watermark` migration on load runs under a no-view gate that keeps ranges
  conservatively separate.

Checkpoint migration merges legacy ranges across a proven all-Closed gap only when full-history
freshness is `fresh`; stale or unevaluated evidence keeps the ranges separate for conservative
over-fetch. Staleness remains a diagnostic and never rewrites a day fact. `BACKWARD WIDEN UNCERTAIN`
is likewise non-persisted and reevaluated after evidence changes.

### Max-lookback probe (run FIRST — sizes the backfill)

Before the first minute backfill, run the staged probe to learn how deep the server
serves minute history. It walks a single liquid pilot symbol (`005930` by default)
backward in ≥7-calendar-day windows (each window always spans trading days, so only an
**all-empty** window reads as beyond-lookback — a single-date probe would converge
wrongly on KRX weekends/holidays), and writes `<data>/probes/minute-lookback.json`
recording the earliest served date, the derived depth in calendar days, and the probe
timestamp. `<data>` is the catalog's parent directory.

```
LS_TRADING_ENV=paper LS_INGEST_LANE_FILE=.env.domestic \
LS_INGEST_MODE=probe-lookback LS_INGEST_CATALOG=./data/catalog \
LS_PROBE_SYMBOL=005930 LS_PROBE_NCNT=1 \
  cargo run --bin ls-ingest
# → writes ./data/probes/minute-lookback.json and prints the derived LS_INGEST_LOOKBACK.
```

Then size the backfill floor from the recorded result — either form works:
`LS_INGEST_LOOKBACK=<earliest_date>`, or (rolling-window safe if the probe and the
backfill run days apart) `anchor − depth_days`. Bound it with an explicit operator
budget floor — don't backfill deeper than you intend to store. If the server allows
only a shallow history, the loop proceeds on thin data and depth grows via daily
accumulation. A probe older than a few sessions should be re-run.

**Ordering:** probe → bounded minute backfill (below) → scheduled accumulate-forward.
Set the **daily** floor at least 5 sessions earlier than the minute floor so the
universe scan's prior-session daily reads exist from the first backfilled day.

### Historical backfill

```
LS_TRADING_ENV=paper LS_INGEST_LANE_FILE=.env.domestic \
LS_INGEST_CATALOG=./data/catalog LS_INGEST_SDATE=20240102 LS_INGEST_EDATE=20240105 \
LS_INGEST_KIND=daily LS_INGEST_SYMBOLS=005930,000660 \
  cargo run --bin ls-ingest
```

Budget note: at the 1 req/s per-TR cap a full-universe **daily** pass is ~2,700
requests (~45 min); a multi-year full-universe **minute** backfill is ~10⁶ requests
(12+ days), so minute ingestion MUST be bounded (`LS_INGEST_SYMBOLS` and/or a short
range) and grown via scheduled accumulate-forward runs. Paper history may be short
or empty per symbol — the run records coverage gaps rather than failing.

### Accumulate-forward (idempotent, cron-safe)

`LS_INGEST_MODE=accumulate` grows whole-universe coverage from each instrument's
**watermark** (last covered closed session, stored in the checkpoint) to the last
closed session — re-snapshotting the instrument universe each run so newly-listed
symbols enter coverage (bounding survivorship bias from adoption day forward). It is
**idempotent**: invoked when coverage is already current it makes zero bar fetches.
`LS_INGEST_LOOKBACK` is the floor an unseen/newly-listed instrument starts at (also
the initial bounded backfill). "Last closed session" includes today only once
now-KST is past **16:30 KST**, so a post-close cron delivers the just-closed session
rather than lagging a day; the watermark never advances into an in-session day.

```
LS_TRADING_ENV=paper LS_INGEST_LANE_FILE=.env.domestic \
LS_INGEST_MODE=accumulate LS_INGEST_CATALOG=./data/catalog \
LS_INGEST_LOOKBACK=20240101 LS_INGEST_KIND=daily,minute:1 \
  cargo run --bin ls-ingest
```

Scheduling is a documented recipe, not a daemon — a post-close cron (the adapter
owns no scheduler). The lock dir MUST be the catalog directory so the R15 exclusion
actually contends with a live node / tester (`LS_NODE_LOCK_DIR=./data/catalog`):

```cron
# 17:00 KST daily, after the 16:30 close buffer. Adjust TZ/path. Runs every day, not
# just weekdays (KTD8): the Enforced calendar decides whether the day is a Trading
# Session — a weekend/holiday simply advances coverage from proven closure or stops
# fail-closed, so there is no `1-5` weekday restriction to keep in sync with KRX.
0 17 * * *  cd /path/to/adapters/nautilus && \
  LS_TRADING_ENV=paper LS_INGEST_LANE_FILE=.env.domestic \
  LS_CALENDAR_SNAPSHOT=./data/krx-calendar.json \
  LS_INGEST_MODE=accumulate LS_INGEST_CATALOG=./data/catalog \
  LS_INGEST_LOOKBACK=20240101 LS_INGEST_KIND=daily,minute:1 \
  cargo run --release --bin ls-ingest >> ./data/catalog/ingest.log 2>&1
```

The server-side minute lookback cap is unknown; size `LS_INGEST_LOOKBACK` after the
staged max-lookback probe (see the v1 plan). Minute accumulate over the whole
universe is still large per run — bound the first few runs with `LS_INGEST_SYMBOLS`
until the shallow minute history deepens.

**Write-side hardening (exit codes + coverage trust).** An accumulate run trusts the
coverage its own checkpoint already records: when a legacy checkpoint's `completed`
ranges are separated only by a non-trading gap (a holiday cluster), the run fetches
only the un-covered gap and never re-fetches — and thus never overlap-refuses — a
range it already holds, with no trading-day calendar (a genuine trading day still in
the gap *is* fetched). The backward-widen no-op warning (`BACKWARD WIDEN NO-OP`, a
late-listed symbol whose floor precedes its earliest stored coverage) fires at most
once per symbol per floor and is **informational** — a deeper floor re-warns. The
process **exit code** distinguishes outcomes: `0` = clean (backward-widen warnings do
not redden it), `2` = the run completed but one or more triples were **refused**
(`HEAL REFUSED` / `REFUSED PENDING HEAL` / `APPEND REFUSED`) and need an operator
(re-run with an adequate floor, or `lab-research catalog compact` / wipe + re-pull),
`1` = the run itself errored. A cron can gate on nonzero to page only on real stalls.

**Adjustment-basis shifts are self-healing (daily bars).** Daily bars are ingested
on an **adjusted-price** basis (`adjusted_prices` recorded in the checkpoint), and
adjusted series are rewritten server-side by every split/dividend. Accumulate-forward
now *detects* a per-symbol basis shift before appending — it re-fetches a bounded
overlap window (the last `overlap_days` stored trading days ending at the watermark)
and exact-compares OHLC on mutually-present dates — and *heals* it: durably mark the
symbol shifted, true-delete its daily series, clear its watermark, re-pull from the
`LS_INGEST_LOOKBACK` floor, re-verify, then clear the mark and record a re-base event
in the checkpoint (the operator audit trail for how often the gateway rewrites
series). Each re-base event carries an **origin** — `heal` (organic forward
detection) or `epoch` (the one-time rollout), stamped at mark time so it survives a
crash-resumed heal running under a different mode — plus `unknown` for rows written
before origin tracking (presumed organic). The audit metric is
`Checkpoint::rebase_origin_totals()`, whose `.organic()` counts heal + unknown and
**excludes** epoch, so the operator's "how often does the gateway rewrite series"
signal is not inflated by the one-time epoch. The per-series event log is bounded
(cap 4, oldest-dropped); evicted rows are folded into origin-split counters so the
per-origin totals stay whole across eviction. The mark outranks the watermark, so an
interrupted heal resumes at the wipe on the next run; a run whose floor is later than
the symbol's earliest stored bar **refuses** the wipe (printed as `HEAL REFUSED`)
rather than silently truncating history — re-run with an adequate floor.
**Range mode never heals:** a `LS_INGEST_MODE` range run refuses a marked daily
series pending heal (printed as `REFUSED PENDING HEAL`, a distinct counted line in
the summary) rather than serving or completing it on a stale basis — run
`accumulate`/`rebase` to heal. Backtests over a still-marked symbol report
it in `adjustment_basis_shift_symbols`. **Minute-basis residual:** t8412 exposes no
adjusted-price request flag, so minute bars keep whatever basis the server serves; a
daily re-base never touches minute bars, and minute-basis fidelity remains a
documented residual.

### Epoch re-base (one-time rollout runbook)

Forward-only detection cannot see splices already baked into an accumulated catalog.
`LS_INGEST_MODE=rebase` performs the one-time whole-catalog re-base: it marks every
daily triple shifted in one atomic checkpoint save, then heals each through the same
per-symbol path — after it, the catalog sits on a single basis and detection
maintains that invariant.

```
LS_TRADING_ENV=paper LS_INGEST_LANE_FILE=.env.domestic \
LS_INGEST_MODE=rebase LS_INGEST_CATALOG=./data/catalog \
LS_INGEST_LOOKBACK=20240101 LS_INGEST_KIND=daily \
  cargo run --release --bin ls-ingest
```

- **Size the window from observed wall time, not the budget lower bound.** The
  ~2,700-request ≈ 45-minute full-universe figure above is a one-page,
  one-fetch-per-triple lower bound; the heal costs at least two fetches per symbol
  (re-pull + re-verify) and a full-depth re-pull is multi-page per symbol, so a
  realistic epoch is ≥ 5,400 requests ≈ 90 minutes and scales with floor depth. Run
  it inside a no-live window sized from the original range-mode backfill's observed
  wall time. (Steady-state accumulate runs also gain one overlap request per daily
  triple.)
- **Pin `LS_INGEST_LOOKBACK` at or before the original backfill start.** The wipe
  precondition refuses a shallower floor per symbol (`HEAL REFUSED`), leaving those
  symbols marked until a run with an adequate floor.
- **The ingest↔live advisory lock is held for the duration** — a live node cannot
  start mid-epoch, and the epoch refuses to start while a live session runs.
- **Crash/resume:** the per-symbol marks are the completion state. A crash leaves a
  stale `.ls-ingest.lock` in the catalog dir — remove it manually, then resume with
  `LS_INGEST_MODE=accumulate` (heals only the still-marked remainder; re-running
  `rebase` re-marks and re-pulls everything). Origin is stamped `epoch` at mark time,
  so a resumed heal under `accumulate` still records epoch origin and the organic
  audit metric stays clean. A series already `heal`-marked when the epoch runs keeps
  its heal origin (keep-original-on-re-mark). **Sequencing:** run the epoch only
  after landing origin tracking — pre-tracking rows read as `unknown` and are
  presumed organic, so an epoch run before this would mix unlabeled epoch rows into
  the organic metric.

### Data smoke

```
LS_TRADING_ENV=paper LS_NODE_LANE_FILE=.env.domestic LS_NODE_SYMBOL=005930 \
  cargo run --bin node_data_tester        # prints scrubbed ticks for ~20s (in-session)
```

### Execution smoke

Before running, clean any smoke-test residue off the shared paper account (open
orders / holdings) or the R14 flat-start gate will refuse. Supply a **safe resting
buy price below market but within the daily band** via `LS_NODE_PRICE`:

```
LS_TRADING_ENV=paper LS_NODE_LANE_FILE=.env.domestic LS_NODE_SYMBOL=005930 \
LS_NODE_PRICE=<safe-resting-price> \
  cargo run --bin node_exec_tester        # flat-gate → submit resting → cancel → halt
```

The exec tester routes every order through the SDK's `post_order`
dedup/no-retry/kill-switch path, verifies flatness before and after, engages the
kill switch only **after** the closing cancel, and — new in this increment —
**refuses before placing anything** if `LS_NODE_PRICE` is marketable (≥ the t8450
best ask) or outside the daily band (the U6 band guard, fail-closed).

### Execution lane (fills, modify, cancel)

The execution client now emits **`OrderFilled`** (full + partial), and exposes
**modify/cancel** through the Nautilus `ExecutionClient` surface. Fills come from two
sources feeding one exactly-once ledger (`orders::ledger`): a t0425 **poll loop**
(currently authoritative — fills emit on bare paper with no push frames, paced to the
2/s t0425 cap) and the **SC0/SC1 order-event WS lane**. Both sources already flow
through the one exactly-once seam, so a fill observed by both lanes collapses to a
single `FillDelta` regardless of arrival order (`orders::ledger`, AE1) — SC is not a
separate, un-deduped path. The poll is authoritative by default; the staged live probe
below has **certified** SC push-fills (§28), so an operator may flip the off-by-default
SC-primary selector (`LS_NODE_SC_PRIMARY=1`) to relax the poll to a slow fail-closed
backstop cadence and let SC carry fills. KRX modify/cancel issue new order numbers; the ledger
chains them, so a fill keyed on any chained OrdNo resolves to the originating order —
and a rejected cancel emits **cancel-rejected** (the order stays open), never a
canceled event. Poll-derived fills emit at the row's **`cheprice`** when it parses
positive; otherwise they fall back to the order's limit price and set
`price_approximated` (also set on any beyond-first-partial poll fill). `cheprice` is
wired end-to-end today on `T0425OutBlock1` and consumed in `orders::poll` — there is
no pending SDK follow-up to add it.

### Staged SC live probe — CERTIFIED (2026-07-07, ledger §28)

The paper gateway **does** deliver SC push frames and **does** tolerate the exec client's
second concurrent WS session — certified live in an attended open-KRX window. The
`LS_NODE_SC_CERTIFY=1` leg drove a marketable 1-lot buy and witnessed the same fill via
**both** the SC1 frame and the t0425 poll through one production ledger, printing
`sc1_frames=1 sc_execprc_positive=true poll_saw_fill=true cheprice_populated=true
total_fill_deltas=1 dedup_collapsed_to_one=true 2nd_ws_tolerated=true => CERTIFIED`. So the
exactly-once dedup (the invariant the SC-primary backstop relaxation makes load-bearing) is
proven against real frames, and `cheprice` came back exact (not the limit-price fallback).

**SC-primary is therefore authorized:** set `LS_NODE_SC_PRIMARY=1` on the live node to run
with SC as the primary fill source and the poll demoted to the `SC_PRIMARY_BACKSTOP_CADENCE`
(15s) fail-closed backstop. Off by default = poll authoritative, byte-identical to prior.

```
# SC certification leg (U3/U6): marketable 1-lot buy, dual-source dedup witness + verdict.
LS_TRADING_ENV=paper LS_NODE_LANE_FILE=.env.domestic LS_NODE_SYMBOL=005930 \
LS_NODE_SC_CERTIFY=1 \
  cargo run --bin node_exec_tester

# Leg 1 (SC0 only): observe accepts during the guarded resting chain (never fills).
LS_TRADING_ENV=paper LS_NODE_LANE_FILE=.env.domestic LS_NODE_SYMBOL=005930 \
LS_NODE_PRICE=<safe-resting-price> LS_NODE_SC_PROBE=1 \
  cargo run --bin node_exec_tester

# Recovery: flatten a residual holding a probe left un-netted (fail-closed, never buys).
LS_TRADING_ENV=paper LS_NODE_LANE_FILE=.env.domestic LS_NODE_SYMBOL=005930 \
LS_NODE_CLOSE_ONLY=1 \
  cargo run --bin node_exec_tester
```

Each leg prints an `SC PROBE [...]` verdict line (SC0-seen / SC1-seen / silent, per
leg, plus whether the second WS session was tolerated) — file it in the smoke
registry. A bare "silent" on the resting leg is *not* evidence SC frames don't
arrive (a resting order never fills); only Leg 2 can certify SC1.

### Depth is NOT purely additive (correction)

The v1 plan recorded that the WS row structs "already decode the full ladder" — that
is wrong. `BookRow` decodes only levels 1–2 + book totals; full 10-level depth
(`OrderBookDeltas`/`Depth10`) is **new decode work**, deferred. A bar-driven
scan strategy needs no depth, so this does not block the follow-on module.

## License note

The adapter links LGPL-3.0-or-later nautilus crates. Distributing it **as source**
keeps MIT licensing unproblematic; distributing linked binaries later carries LGPL
relink/source obligations.
