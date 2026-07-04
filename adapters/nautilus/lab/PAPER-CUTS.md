# Paper cuts — first real loop cycle

Frictions observed while turning the strategy-improvement loop once on real
recorded data (plan `docs/plans/2026-07-04-001`). Each entry was a candidate
requirement for the deferred `lab-research` CLI (or a named owner).

**Status:** items 1–6 and 9–11 shipped in the `lab-research` CLI wave (plan
`docs/plans/2026-07-04-002`) — retired below with a pointer to the command that
addresses each. Items 7–8 stay open with their named residual owners.
Credential-free by construction: no run ids, dates, account or symbol-embedded
values appear here.

## Retired — shipped in the `lab-research` CLI

1. **No parameter-override surface on `lab-backtest`.** → `lab-research turn`
   (`src/runner/research.rs`): resolves current params from the latest finalized
   manifest, governs the proposal through the pinned pipeline, applies the
   override + a version bump, and refuses when the executed override set differs
   from the envelope it governs.

2. **No catalog inspector.** → `lab-research catalog status`: per-(instrument,
   bar-kind) counts + spans, flags a span that undershoots the checkpoint's
   completed range (and, with an expected range, front truncation).

3. **No manifest-comparison command.** → `lab-research runs compare`: the
   corrected AE4 param-turn verdict (exactly-two-key diff, code/fingerprint/range
   equal, universe equal-or-explained) plus a data-turn verdict for the wider
   slice (zero-key diff, code equal, explained deltas).

4. **Replay-target guard.** → `lab-research replay`: refuses a stream whose
   evaluated count is zero (telemetry-only) instead of reporting "no divergence".

5. **No analysis scaffold.** → `lab-research analyze --scaffold`: pre-fills
   `analysis.md` with run facts (params, trade count, gap-noise summary) and the
   keep / revert / insufficient-evidence verdict skeleton; refuses to overwrite.

6. **`data_quality.json` gap noise.** → the backtest runner's candidate scan now
   skips instruments with no daily bars anywhere in the catalog (never-ingested)
   before the missing-prior-daily path (`src/runner/backtest.rs`, KTD5). A symbol
   that has bars but lacks the prior session's daily still reports; the universe
   snapshot still documents the full instrument count.

## Fixed during the first cycle — residual owners remain (7–8)

7. **t8412 minute pagination was doubly broken** (fixed in the adapter's
   `SdkFetcher`): the SDK's `chart_all` fires continuation pages back-to-back —
   tripping the 1/s per-TR gateway cap (`IGW00201`) because the runtime limiter
   is per-category — and walks the `tr_cont` HTTP headers, which the live
   gateway terminates after page one while in-range rows remain. **Residual
   owner: the SDK** — `chart_all` remains unsafe for multi-page t8412 use, and
   the chart module docs still describe header-driven continuation; port the
   body-cursor + `tr_cont: Y` drive (or document `chart_page` as the only safe
   primitive).

8. **Constraint-schema preflight false-rejected the adapter's order path**
   (fixed in metadata): the order-submit schema required a member-number field
   the adapter's KRX submit legitimately sends empty — struct wins. **Residual
   owner: repo process** — the adapter workspace sits outside the root gate, so
   an SDK-side preflight change can redden the adapter invisibly; add the
   adapter's `cargo test --workspace` to the gate or CI.

## Found + fixed during turn-2 certification

12. **Float dust denied an on-bound governed step.** Turning the loop surfaced
    that a stored param carries rounding from the prior turn (turn 1's
    `3.0 * 0.8` stores `2.4000000000000004`), so the intended clean half-step to
    `1.2` computes a relative change of `0.5000000000000001` — a bare `<=`
    rejected it by 1e-16 while the guardrail's own `{:.4}` reason printed the
    absurd "relative change 0.5000 exceeds bound 0.5000". Fixed with a dust-sized
    bound tolerance (`BOUND_EPSILON = 1e-9`) in
    `agent/guardrails/proposal_bounds.rs` so the 0.5 policy is enforced at the
    precision it is displayed and specified in; NaN and the zero-current
    `INFINITY` still fail closed, and a genuinely over-bound change still rejects.
    This is the CLI's first real governance decision on live-derived state, and
    the tolerance keeps the chained-turn path (2.4 → 1.2 → 0.6) usable without
    weakening the guardrail.

13. **Re-ingesting an overlapping range silently duplicated bars — corrupting
    the backtest universe scan and the catalog-status counts.** Turn 2b widened
    the slice with an earlier accumulate floor. The prior catalog's checkpoint
    predated the watermark format (it carried legacy `completed` ranges, empty
    `watermarks`), so accumulate saw every triple as never-seen and re-fetched
    from the floor — writing a second parquet file for the whole range beside the
    original. `write_to_parquet` skips the disjoint check and the accumulate
    *append* path (unlike the heal path) never wipes, so the overlap stayed
    readable twice. Two consequences: `lab-research catalog status` counted the
    overlap doubled, and the runner's universe scan — which reads the last two
    in-range daily bars as prior→today — picked two copies of the final session,
    computing a nonsensical intraday self-gap (open vs its own close, always
    negative) that rejected every symbol and emptied the universe. Fixed with
    read-side dedup in `read_all_bars` (`src/ingest/mod.rs`, `dedup_bars`): a bar
    is unique by `(series, ts_event)`; the overlap re-pull is value-identical so
    the first copy wins, while a value-DIVERGENT duplicate stays the heal path's
    adjustment-shift wipe, not this. Also fixed a second turn-2b blocker: a
    weekend accumulate advances the checkpoint watermark onto a non-session day,
    and `catalog status`'s tail check compared the last bar against the raw
    watermark — false-flagging a healthy Friday-closed catalog as a NO-GO. The
    tail check now compares against the last weekday on-or-before the watermark
    (`last_weekday_on_or_before`); holidays remain undetectable (no trading
    calendar in the repo).

## Retired — shipped (operability minors)

9. **Ingest gateway errors carry no context.** → gateway fetch failures are now
   wrapped with the TR code, page/chunk index, and pacer cap
   (`AdapterError::IngestGateway`, `src/ingest/mod.rs`), so a first `IGW00201`
   localizes without a raw-probe A/B. No raw request body is included.

10. **`lab-backtest` buries its result line.** → a trailing summary block (run
    id, trade count, run dir) is printed after all engine logs
    (`runner::backtest::summary_block`).

11. **README catalog-path inconsistency.** → the adapter README's backfill,
    accumulate, and rebase examples standardize on `<data home>/catalog`
    (`./data/catalog`), matching the probe example and the lab README.
