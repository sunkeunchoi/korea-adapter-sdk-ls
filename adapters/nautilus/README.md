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

### Historical backfill

```
LS_TRADING_ENV=paper LS_INGEST_LANE_FILE=.env.domestic \
LS_INGEST_CATALOG=./catalog LS_INGEST_SDATE=20240102 LS_INGEST_EDATE=20240105 \
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
LS_INGEST_MODE=accumulate LS_INGEST_CATALOG=./catalog \
LS_INGEST_LOOKBACK=20240101 LS_INGEST_KIND=daily,minute:1 \
  cargo run --bin ls-ingest
```

Scheduling is a documented recipe, not a daemon — a post-close cron (the adapter
owns no scheduler). The lock dir MUST be the catalog directory so the R15 exclusion
actually contends with a live node / tester (`LS_NODE_LOCK_DIR=./catalog`):

```cron
# 17:00 KST every weekday, after the 16:30 close buffer. Adjust TZ/path.
0 17 * * 1-5  cd /path/to/adapters/nautilus && \
  LS_TRADING_ENV=paper LS_INGEST_LANE_FILE=.env.domestic \
  LS_INGEST_MODE=accumulate LS_INGEST_CATALOG=./catalog \
  LS_INGEST_LOOKBACK=20240101 LS_INGEST_KIND=daily,minute:1 \
  cargo run --release --bin ls-ingest >> ./catalog/ingest.log 2>&1
```

The server-side minute lookback cap is unknown; size `LS_INGEST_LOOKBACK` after the
staged max-lookback probe (see the v1 plan). Minute accumulate over the whole
universe is still large per run — bound the first few runs with `LS_INGEST_SYMBOLS`
until the shallow minute history deepens.

**Known limitation — adjustment-basis bias (decided at U5 kickoff).** Daily bars are
ingested on an **adjusted-price** basis (`adjusted_prices` recorded in the
checkpoint), and adjusted series are rewritten server-side by every split/dividend.
Because accumulate-forward *appends* bars, bars accrued before a corporate action sit
on a different price basis than bars appended after it — a spliced series can show an
overnight discontinuity a gap/stocks-in-play scanner would misread as signal. This
increment **keeps adjusted accumulation** (recorded, not silently mixed) and defers
re-basing; the follow-on strategy plan must price this in (options: unadjusted
accumulation with read-time adjustment, or a per-symbol re-pull on a detected basis
shift).

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
(authoritative — fills emit on bare paper with no push frames, paced to the 2/s
t0425 cap) and the **SC0/SC1 order-event WS lane** (subordinate until the live probe
below certifies it). KRX modify/cancel issue new order numbers; the ledger chains
them, so a fill keyed on any chained OrdNo resolves to the originating order — and a
rejected cancel emits **cancel-rejected** (the order stays open), never a canceled
event. Poll-derived fills emit at the order's limit price (t0425 models no execution
price; a v-next SDK follow-up adds `cheprice` to `T0425OutBlock1`).

### Staged SC live probe (settles SC-on-paper)

Whether the paper gateway delivers SC push frames — and whether it tolerates the
exec client's second concurrent WS session — is unknown; the design is correct
either way (poll is authoritative), and this operator-gated probe settles it. Its
verdict decides whether SC later becomes the primary fill source.

```
# Leg 1 (SC0): observe accepts during the guarded resting chain (never fills).
LS_TRADING_ENV=paper LS_NODE_LANE_FILE=.env.domestic LS_NODE_SYMBOL=005930 \
LS_NODE_PRICE=<safe-resting-price> LS_NODE_SC_PROBE=1 \
  cargo run --bin node_exec_tester

# Leg 2 (SC1): a 1-lot marketable buy + sign-aware close (BYPASSES the band guard) —
# the only way to witness a fill frame. Requires an open KRX window.
LS_TRADING_ENV=paper LS_NODE_LANE_FILE=.env.domestic LS_NODE_SYMBOL=005930 \
LS_NODE_SC_MARKETABLE=1 \
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
