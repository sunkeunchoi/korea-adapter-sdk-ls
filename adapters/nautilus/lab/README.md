# nautilus-ls-lab — the strategy-improvement loop

The lab is a **separate crate** from the certified `nautilus-ls` adapter (KTD1). The
adapter ships no strategy — its contract is translation only. All strategy code, the
backtest/live runners, and the artifact writer live here, so strategy churn never
destabilizes the adapter.

The lab exists to turn one loop: **backtest → an agent analyzes the artifacts →
change the strategy → re-backtest → compare**. This README is the recipe an agent
follows to turn it, without reading source. Known frictions in the recipe — and the
requirements seed for the deferred `lab-research` CLI — live in
[PAPER-CUTS.md](PAPER-CUTS.md); read it before turning the loop.

## What a run produces (the append-only registry)

Runs live beside the catalog under one data home:

```
<data>/catalog/                     # ParquetDataCatalog (ingested bars + instruments)
<data>/runs/<run_id>/               # one immutable directory per run (never overwritten)
  manifest.json                     # strategy id/version, full params, pinned range,
                                    #   range-scoped catalog fingerprint, universe hash
  performance.json                  # trade/fill ledger, per-trade P&L, equity curve, stats
  decisions.jsonl                   # one decision envelope per decision (universe + transitions)
  data_quality.json                 # coverage gaps, adjustment-basis flag, approximated-fill
                                    #   count, reconcile-advised conditions (live), universe
  analysis.md                       # YOU write this (see below) — it co-locates here
<data>/decisions/decisions.jsonl    # cross-run agent-decision registry (append-only;
                                    #   intent-bearing Research-policy envelopes — never
                                    #   inside a finalized run dir)
<data>/probes/minute-lookback.json  # the max-lookback probe result (adapter README)
```

`run_id = <UTC start stamp>-<source>-<strategy_id>-v<strategy_version>` (second
granularity — two runs started in the same second collide and the second is refused,
by the append-only guard). `source` is `backtest` or `live`. A run writes into
`<data>/runs/.tmp-<run_id>/` and finalizes by atomic rename; a leftover `.tmp-`
directory is an **aborted run** — reported on the next writer construction, never
reused. Artifacts are credential-free by construction (typed enums + counts; the one
free-text field is scrubbed at write time).

Two runs are comparable from their manifests alone (AE1): the manifest pins
`strategy_version` **and** a `strategy_code_hash` (so a logic change without a version
bump is still visible), the full parameter set, the pinned `data_range`, a
range-scoped `catalog_fingerprint`, and the `universe_hash`. A backtest run trades a
**single session** — the last trading day whose daily bar falls in the pinned range.

### Artifact key reference (read artifacts without reading source)

`decisions.jsonl` — each line is one decision envelope
`{ schema_version, envelope_id, ts_event, trigger, context, policy_decision,
capability, guardrail, lowering, action?, decision_detail? }`. In-run strategy
telemetry rides `decision_detail` as `{ kind, symbol, decision?, filter?, values }`
(the pipeline stages of a telemetry envelope are explicitly `NotEvaluated`, its
`action` is absent, and its `context` is the minimal `form: Telemetry` snapshot:
strategy id/version, numeric params, running counts). The **cross-run registry**
(`<data>/decisions/decisions.jsonl`) carries the other shape: intent-bearing
envelopes with a `form: RunState` context (balance_krw, position summaries,
params, run_summary) and, on approved cycles, a populated
`action: { type: "ResearchCommand", description }`. Two unrelated fields share the
bare name `decision`: `policy_decision` tags on the JSON key `"decision"`
(PascalCase `Execute`/`NoAction`/`Failed`) while `decision_detail.decision` is
snake_case `accept`/`reject` — filter on the full path, never a bare `"decision"`
key. `decision_detail.values` keys per `kind`:

| `kind` | `decision` / `filter` | `values` keys |
|---|---|---|
| `universe` (accept) | `decision: accept` | `gap_pct`, `prior_turnover`, `rank` |
| `universe` (reject) | `decision: reject`, `filter: gap` \| `turnover_rank` | `gap_pct`,`prior_close`,`today_open` (gap) or `prior_turnover`,`rank` |
| `breakout` | — | `range_high`, `range_low`, `breakout_price` |
| `order_placed` | — | `qty`, `price` |
| `order_rejected_sizing` | `filter: emission_stopped` \| `notional_too_small` \| `max_concurrent` | `open_positions`, `qty` |
| `stop_hit` / `time_exit` | — | `qty`, `price` |
| `session_summary` | — | `session_high`, `session_low` |

`performance.json` `summary` — a flat `{key: number}` map. Lab-computed keys are
snake_case: `pnl_total`, `num_trades`, `max_drawdown` (all KRW / counts). The remaining
keys come from `nautilus-analysis`'s `PortfolioAnalyzer` and are Title-Case with spaces
(`"Win Rate"`, `"Expectancy"`, `"PnL (total)"`, winners/losers); returns-based stats
(Sharpe/Sortino) appear only when ≥2 daily account-balance snapshots exist, else they
are dropped (never emitted as NaN). All monetary values are KRW (single-currency,
domestic KRX).

## Turning the loop (backtest)

1. **Backfill data** (adapter README: probe → bounded minute backfill → accumulate).
2. **Run the baseline backtest** over a pinned range:

   ```
   LS_DATA_HOME=./data LS_BT_SDATE=20240102 LS_BT_EDATE=20240105 \
     cargo run --bin lab-backtest
   # → finalizes ./data/runs/<run_id>/ with the four artifacts.
   ```

3. **Analyze the run.** Read the four artifacts in the finalized run dir and write
   your findings to `analysis.md` **inside that same run directory**. Co-locating the
   analysis with the runs it analyzed (R15) is the whole point — the next turn builds
   on it. A committed example is `tests/fixtures/analysis.md`.
4. **Change the strategy.** For a parameter turn, bump `strategy_version` and change
   the parameter(s). ORB v0's parameters are all in `params.rs` (gap filter, opening
   range, entry/exit, sizing) and every value is recorded in the manifest.
5. **Re-run** over the **same pinned range** so the two runs are comparable.
6. **Compare via the manifests.** Two runs whose manifests differ only in
   `strategy_version` + the changed parameter are a clean loop turn: the parameter and
   data deltas are visible from the manifests alone, no re-run or source diff. Because
   the range is pinned, the range-scoped `catalog_fingerprint` is identical across
   accumulate days — a *changed* fingerprint means real in-range data drift, not the
   nightly accumulate growth.

The permanent proof of a turn is the test
`loop_turn_manifest_comparison_isolates_param_delta` in `tests/backtest_run.rs`, plus
the committed fixture analysis.

## The agent-decision layer (envelopes, guardrails, replay)

The `agent/` module is a native reimplementation of the *shape* of
[`nautechsystems/nautilus_agents`](https://github.com/nautechsystems/nautilus_agents)
(an early-alpha upstream protocol pinned to nautilus 0.55, which our 0.60 pin cannot
depend on): `AgentIntent` → deny-by-default `CapabilitySet` → `IntentGuardrail` →
lowering → one `DecisionEnvelope` per cycle. Shared fields mirror the upstream serde
tags so tracking it stays cheap; the lab's envelope is a **superset** (it adds the
`context` snapshot and `decision_detail`), shape-mirrored but not cross-validated
against 0.55 — a convenience optionality, relaxable if upstream churns.

Two envelope destinations:

- **In-run telemetry** — ORB's per-decision telemetry rides `DecisionEnvelope`s into
  the run dir's `decisions.jsonl` (above). These cycles carry `NotEvaluated`
  governance stages: ORB's entry/exit decisions are *recorded*, not routed through
  capability/guardrail as intents — that is the deferred live risk-monitor.
- **Cross-run registry** — the deterministic Research-tier `ResearchPolicy` reads a
  finalized run's artifacts, proposes a parameter change (`ProposeParameterChange`),
  and its **intent-bearing** envelope flows through the pipeline (`Research`
  capability + `ProposalBoundsGuardrail`) into `<data>/decisions/decisions.jsonl`.

**Replay (engine-free, guardrail-swap only).** `agent::replay::read_envelopes` loads
a recorded stream (typed per-line errors, per-line schema check);
`agent::replay::replay(envelopes, &guardrail)` re-evaluates each intent-bearing,
capability-granted cycle under the new guardrail against the envelope's captured
`context` — the recorded capability outcome is reused verbatim, so the delta is the
guardrail-stage delta. `ReplayResult.first_divergence` marks the audit boundary: on
causally-chained streams the per-envelope delta is trustworthy only up to the first
divergence. Guardrails must be pure per-cycle functions of `(intent, context)` —
stateful guardrails are out of contract until replay handles cross-cycle state.
Policy-level replay is deferred; the captured context is what unlocks it (a committed
test proves the shipped policy's decision is reconstructible from a recorded
envelope's context alone).

## The `lab-research` CLI (turning the loop scratch-free)

The `lab-research` bin is the production caller of the decision pipeline: it wires
`DecisionPipeline` with the proposal-bounds cap pinned at **0.5** relative change and
drives a loop turn end-to-end without scratch code. Seven subcommands, each over one
data home (`LS_DATA_HOME`); a governance refusal / verdict FAIL / replay refusal /
catalog no-go is a non-zero exit, a genuine error is scrubbed and non-zero too
(`report mfe` is the one verdict-free command: its exit code reflects I/O only —
a censored or out-of-band candidate is a reported fact, not a failure).

| Subcommand | What it does | Key env |
|---|---|---|
| `turn` | A governed parameter turn: resolve current params from the latest finalized manifest, govern the proposal (deny-by-default + bounds 0.5), append the envelope, run the backtest with the override + version bump. No override → a rerun (same params, no governance, no version bump). Optional `LS_TURN_EXPECT_VERSION`/`LS_TURN_EXPECT_GAP` assert the resolved identity before running — a mismatch (e.g. a fresh home falling back to the v0 default) is a hard stop, not a silent wrong-param run (KTD-5). | `LS_TURN_PARAM`, `LS_TURN_VALUE`, `LS_TURN_SDATE`/`LS_TURN_EDATE` (optional; inherited otherwise, required on a fresh home), `LS_TURN_EXPECT_VERSION`/`LS_TURN_EXPECT_GAP` (optional resolution assertion) |
| `runs compare` | The manifest verdict: `param` mode (exactly-two-key param diff, code/fingerprint/range equal, universe equal-or-explained) or `data` mode (zero-key param diff + code equal; fingerprint/range/universe deltas require an explanation). PASS/FAIL. | `LS_COMPARE_MODE` (`param`\|`data`), `LS_COMPARE_A`/`LS_COMPARE_B` (default two newest), `LS_COMPARE_EXPLANATION` |
| `replay` | Guardrail-swap replay over a recorded stream (default the cross-run registry). Refuses a telemetry-only stream (zero evaluated cycles) instead of reporting "no divergence". | `LS_REPLAY_STREAM` (default `<data>/decisions/decisions.jsonl`), `LS_REPLAY_CAP` |
| `catalog status` | The ingest→backtest go/no-go: per-(instrument, bar-kind) counts + spans; flags a span that undershoots the checkpoint watermark (and, with an expected range, front truncation). | `LS_STATUS_SDATE`/`LS_STATUS_EDATE` (optional expected range) |
| `catalog compact` | Collapses byte-identical duplicate bars per series into a clean file set (before/after file + bar counts); refuses a value-divergent series and never touches the checkpoint. | — |
| `analyze --scaffold` | Pre-fills a run's `analysis.md` with run facts (params, trade count, gap-noise summary), the **computed R1 decisiveness bar** (per-symbol fold + the three per-condition PASS/FAIL + named failing conditions), and the keep / revert / insufficient-evidence verdict skeleton. Refuses to overwrite. | `LS_ANALYZE_RUN` |
| `report mfe` | The MFE-distribution report over a run's `decisions.jsonl`: per-trade `mfe_r` percentiles (nearest-rank), MFE by exit reason, MFE by breakout-strength quartile (the entry-filter spec input), and the leg-2 profit-target candidate with its RUNNABLE / RIGHT-CENSORED / OUT-OF-BAND verdict. Prints the source run's own `profit_target_r` + target-exit share (every distribution is right-censored at that target) and notes when the reported run is not the latest finalized one (the turn guardrail bands off the latest run's params). Reads artifacts only — never moves the strategy code hash. | `LS_REPORT_RUN` (default: latest finalized, marked `[defaulted]`) |

A governed param turn, then the payoff compare:

```
# turn: gap_min_pct 2.4 -> 1.2 on the existing catalog (range inherited)
LS_DATA_HOME=./data LS_TURN_PARAM=gap_min_pct LS_TURN_VALUE=1.2 \
  cargo run --bin lab-research turn
# scaffold the analysis, fill the verdict, then assert the param-only delta:
LS_DATA_HOME=./data LS_ANALYZE_RUN=<new run id> cargo run --bin lab-research analyze --scaffold
LS_DATA_HOME=./data cargo run --bin lab-research runs compare   # two newest, param verdict
```

The deferred live risk-monitor is the other consumer of the decision layer; it is not
wired yet.

### Turn 3 — broaden-sample data turn (attended, paper) — the fresh-home recipe

Turn 3 is a **pure data turn**: hold all v3 params, broaden the sample to a rule-pinned
KOSPI cross-section, rerun, and render a verdict against the R1 decisiveness bar fixed
*before* the run. The bar is machine-computed in the scaffold; the verdict word is
hand-authored against it and never adjusted to the result (R3).

1. **Capture + freeze the universe (U1).** One live `t1444` KOSPI top-market-cap call
   materializes `lab/config/turn3-universe.json` (validated before write). Commit it.
   The board serves the top `LS_CAPTURE_N` names (default 30) but returns fewer when it
   holds fewer on the page — the committed turn-3 file froze **20** (all it served under
   closure), at the R2/U1 floor. To **reproduce** turn 3, use the committed file as-is;
   re-capturing overwrites it and can change the pinned set.
   ```
   LS_TRADING_ENV=paper LS_CAPTURE_LANE_FILE=.env.domestic \
     cargo run --bin capture-universe          # → lab/config/turn3-universe.json (top-N shcodes + provenance)
   ```
2. **Fresh-home ingest (U3).** A fresh `LS_DATA_HOME` gives a clean fingerprint and
   sidesteps the write-side overlap residual. The helper expands the frozen list into
   `LS_INGEST_SYMBOLS` and runs daily (whole range) then bounded minute:
   ```
   LS_TRADING_ENV=paper LS_DATA_HOME=./data-turn3 \
   LS_TURN3_DAILY_SDATE=20240102 LS_TURN3_SDATE=20240110 LS_TURN3_EDATE=20240216 \
     bash scripts/turn3-ingest.sh
   # then the go/no-go — pins the achievable range on front-truncation (OQ1):
   LS_DATA_HOME=./data-turn3 LS_STATUS_SDATE=20240110 LS_STATUS_EDATE=20240216 \
     cargo run --bin lab-research catalog status   # must be GO before proceeding
   ```
3. **Seed v3 params + rerun with the resolution assertion (U4, KTD-5).** A fresh home
   has no finalized run, so a rerun would fall back to `OrbParams::default` (v0, gap 3.0).
   Copy the turn-2b v3 run's `manifest.json` into the fresh home's `runs/<same-id>/` so
   `latest_finalized_run` resolves gap 0.6 / v3, then pin the assertion so a missing seed
   is a hard stop — not a silent v0 run:
   ```
   mkdir -p ./data-turn3/runs/<turn2b-v3-run-id>
   cp ./data/runs/<turn2b-v3-run-id>/manifest.json ./data-turn3/runs/<turn2b-v3-run-id>/
   LS_DATA_HOME=./data-turn3 LS_TURN_EXPECT_VERSION=3 LS_TURN_EXPECT_GAP=0.6 \
   LS_TURN_SDATE=20240110 LS_TURN_EDATE=20240216 \
     cargo run --bin lab-research turn            # rerun; refuses unless it resolves v3/0.6
   ```
4. **Reproducibility compare (U4, KTD-3).** Run a second identical rerun, then data-mode
   `runs compare` over the two — determinism (run A ≡ run B), not a narrow-vs-wide A/B:
   ```
   LS_DATA_HOME=./data-turn3 LS_TURN_EXPECT_VERSION=3 LS_TURN_EXPECT_GAP=0.6 \
   LS_TURN_SDATE=20240110 LS_TURN_EDATE=20240216 cargo run --bin lab-research turn
   LS_DATA_HOME=./data-turn3 LS_COMPARE_MODE=data \
   LS_COMPARE_EXPLANATION="v3-wide vs identical v3-wide rerun — determinism check" \
     cargo run --bin lab-research runs compare    # expect PASS "no data deltas"
   ```
5. **Scaffold + author the verdict (U4, R1/R7).** The scaffold prints the computed bar;
   author keep/revert only if all three conditions PASS, else insufficient-evidence naming
   the failing condition(s). Record the outcome in the ledger (`CONCEPTS.md` §-note).
   ```
   LS_DATA_HOME=./data-turn3 LS_ANALYZE_RUN=<newest run id> \
     cargo run --bin lab-research analyze --scaffold
   ```

## Live paper session (operator-gated)

The `lab-live` bin runs the **same** ORB against the paper gateway and emits the same
artifacts into the same registry, marked `source = live`. It is **operator-gated** —
never run by the gate — because it needs live paper credentials and an open KRX window.

Safety (KTD7): the runner takes the live advisory lock (refusing while a backfill is
running — they cannot run concurrently), honors the paper-only interlock
(`LS_TRADING_ENV=paper`), and at exit/market-close runs a **fail-closed** teardown:
stop the strategy's order emission first, cancel all resting orders, run a
quantity-keyed t0425 flatness check (positive confirmation only — a truncated read is
not flat), and engage the exec client's kill switch only **after** the closing cancels
complete. Artifacts finalize on teardown; a crash leaves the `.tmp-` directory as the
aborted-run marker.

Live fills report their true execution price: the SDK models `cheprice` on the t0425
row, and the poll lane emits fills at `cheprice` (falling back to the order's limit
price with a `price_approximated` flag counted in `data_quality.json`). Beyond-first
partial fills on one order are also flagged approximate — a row carries one `cheprice`
per order — so the agent never reads an approximated price as exact.

```
LS_TRADING_ENV=paper LS_NODE_LANE_FILE=.env.domestic LS_DATA_HOME=./data \
  cargo run --bin lab-live
```

> **Staging status:** the shipped `lab-live` bin validates the paper-only interlock
> and then exits with a pointer to this recipe — the full LiveNode session (lock,
> mount, `node.run`, teardown, artifact finalize) is **not yet wired**, because it
> needs live credentials and an open KRX window that the offline gate never has. The
> safety-critical pieces exist and are unit-tested: the fail-closed teardown
> (`runner::live::run_teardown`), the `LiveSession` seam, the live advisory-lock guard
> (`runner::live::live_guard`), the emission gate, and the reconcile/approximated-fill
> collectors. An operator wiring the session composes those against a real `LiveNode`.

## Scope

ORB v0 is a **starter, not a deliverable** — its parameters are plan-defined defaults
the loop exists to revise, not tuned claims. Strategy quality is not the goal of this
increment; the loop's existence is.
