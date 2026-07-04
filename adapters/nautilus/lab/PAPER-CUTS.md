# Paper cuts — first real loop cycle

Frictions observed while turning the strategy-improvement loop once on real
recorded data (plan `docs/plans/2026-07-04-001`). Each entry is framed as a
candidate requirement for the deferred `lab-research` CLI (or the named owner).
Credential-free by construction: no run ids, dates, account or symbol-embedded
values appear here.

## Candidate `lab-research` CLI requirements

1. **No parameter-override surface on `lab-backtest`.** The bin hardcodes
   `OrbParams::default()`; landing a param turn requires scratch code calling
   `runner::backtest::run` with an edited config. The CLI's turn command should
   accept parameter overrides plus a strategy-version bump and refuse when the
   override set differs from the proposal envelope it executes.

2. **No catalog inspector.** The only go/no-go between ingest and backtest is
   the ingest report; confirming what a catalog actually holds (bar counts per
   symbol/kind, span, basis) needed scratch code over `read_all_bars`. A
   `catalog status` command should print per-triple counts and spans and flag a
   span that undershoots the checkpoint's completed range (the truncation bug
   below produced exactly that silent state).

3. **No manifest-comparison command.** The loop's payoff step — assert a
   param-only turn isolates its delta — was hand-rolled JSON diffing in scratch
   code. A `runs compare` command should implement the corrected AE4 verdict:
   exactly-two-key param diff, code hash / fingerprint / range equality, and
   the universe-hash equal-or-explained clause.

4. **Replay-target guard.** A guardrail-swap replay against a run dir's
   telemetry stream evaluates zero cycles (`NotEvaluated` stages) and reads as
   a false "no divergence". The CLI's replay command should refuse (or loudly
   warn on) a stream whose evaluated count is zero.

5. **No analysis scaffold.** Authoring `analysis.md` starts from a blank file;
   the committed exemplar is the only template. An `analyze --scaffold` command
   pre-filling run facts (params, trade count, gap-noise summary) would keep
   analyses uniform and scrub-safe.

6. **`data_quality.json` gap noise.** Ingest writes the whole instrument
   universe while bars are bounded to the requested symbols, so every
   never-ingested instrument lands as a spurious `MissingPriorDaily` gap
   (thousands of entries drowning real signal). Bound instrument writes to the
   ingested universe, or filter never-ingested symbols from the gap report.

## Fixed during this cycle (recorded for the record)

7. **t8412 minute pagination was doubly broken** (fixed in the adapter's
   `SdkFetcher`): the SDK's `chart_all` fires continuation pages back-to-back —
   tripping the 1/s per-TR gateway cap (`IGW00201`) because the runtime limiter
   is per-category — and walks the `tr_cont` HTTP headers, which the live
   gateway terminates after page one while in-range rows remain. Any window
   wider than one page silently truncated to its newest slice AND the
   checkpoint marked the triple done with zero gaps. Residual owner: the SDK —
   `chart_all` remains unsafe for multi-page t8412 use, and the chart module
   docs still describe header-driven continuation; port the body-cursor +
   `tr_cont: Y` drive (or document `chart_page` as the only safe primitive).

8. **Constraint-schema preflight false-rejected the adapter's order path**
   (fixed in metadata): the order-submit schema required a member-number field
   the adapter's KRX submit legitimately sends empty — same class as the
   documented loan-date precedent, struct wins. Residual owner: repo process —
   the adapter workspace sits outside the root gate, so an SDK-side preflight
   change can redden the adapter invisibly; add the adapter's `cargo test
   --workspace` to the gate or CI.

## Minor

9. **Ingest gateway errors carry no context.** The first `IGW00201` surfaced as
   a bare one-line error with no TR/page/pacing context; localizing it needed a
   raw-probe A/B. Ingest should wrap gateway errors with the TR code, page
   index, and pacer state.

10. **`lab-backtest` buries its result line.** Engine INFO logs flood stdout;
    the finalize line (the only operator-relevant output) scrolls away. Quiet
    the engine by default or print a trailing summary block.

11. **README catalog-path inconsistency.** The adapter README's backfill
    example uses `./catalog` while the probe example and the lab README use
    `./data/catalog`; following the backfill example verbatim then running
    `lab-backtest` with the data home fails its catalog lookup. Standardize on
    `<data home>/catalog` (this cycle used the `data/` layout, now gitignored).
