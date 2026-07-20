# Paper cuts — first real loop cycle

Frictions observed while turning the strategy-improvement loop once on real
recorded data (plan `docs/plans/2026-07-04-001`). Each entry was a candidate
requirement for the deferred `lab-research` CLI (or a named owner).

**Status:** items 1–6 and 9–11 shipped in the `lab-research` CLI wave (plan
`docs/plans/2026-07-04-002`) — retired below with a pointer to the command that
addresses each. Items 7–8 shipped this session (PRs #142 and #143) and are
retired below with their closing pointers. Credential-free by construction: no
run ids, dates, account or symbol-embedded values appear here.

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

## Retired — shipped (residual owners closed, 7–8)

7. **t8412 minute pagination was doubly broken** (fixed in the adapter's
   `SdkFetcher`): the SDK's `chart_all` fired continuation pages back-to-back —
   tripping the 1/s per-TR gateway cap (`IGW00201`) because the runtime limiter
   is per-category — and walked the `tr_cont` HTTP headers, which the live
   gateway terminates after page one while in-range rows remain. → **PR #142**
   (commit `899497b`): `chart_all` was ported off the generic header-driven
   `collect_all` onto a hand-rolled body `cts_date`/`cts_time` cursor loop
   (mirroring this adapter's `fetch_minute_chunk`) — completes on an empty
   `cts_date`, threads `tr_cont: Y` + `tr_cont_key` per page, fails closed to
   `PaginationLimit` on a repeated cursor / zero-row live-cursor page / max_pages,
   with offline tests keyed on the full cursor tuple. `chart_all` is now the safe
   multi-page primitive. Residual pacing caveat: `chart_all` still bursts pages,
   so the adapter's paced `fetch_minute_chunk` remains the IGW00201-safe path for
   bulk pulls.

8. **Constraint-schema preflight false-rejected the adapter's order path**
   (fixed in metadata): the order-submit schema required a member-number field
   the adapter's KRX submit legitimately sends empty — struct wins. The residual
   owner was repo process: the adapter workspace sits outside the root gate, so
   an SDK-side preflight change can redden the adapter invisibly. → **PR #143**
   (commit `0dbd522`): added the `make adapter-check` gate step
   (`cd adapters/nautilus && cargo test --workspace`, documented under AGENTS.md
   "Gate") plus `.github/workflows/adapter-check.yml` running the full unfiltered
   adapter workspace test on push/PR. Manual triage is now a backstop, not the
   primary detector.

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
    (`last_weekday_on_or_before`).
    **Retired — shipped (calendar-backed, issues #185/#187):** the
    weekday walk-back is now the Legacy/Shadow path only. The shared offline KRX
    calendar (`nautilus-ls-calendar`, PR #190) added the Enforced adoption, under
    which `catalog status` resolves watermark and expected-range boundaries against
    PROVEN Trading Sessions (`CatalogCalendarGate`) — a real holiday closure no
    longer false-flags, a boundary-relevant Unknown is `NO-GO — calendar
    indeterminate`, and an out-of-coverage/unavailable boundary is `NO-GO —
    calendar unavailable`. Holidays are no longer undetectable. The composed default
    stays Shadow (byte-identical to Legacy while recording the calendar verdict);
    the live Enforced cutover is the deferred Consumer Retirement Gate (#189). See
    the adapter README "Offline KRX calendar" section for configuration.

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

## Open — reference-data universe engine (plan 2026-07-10-003 review)

14. **The governed `turn` flow cannot enable a zero-defaulted gate parameter.**
    `OrbParams.turnover_floor_krw` defaults `0.0` (floor off), and
    `ProposalBoundsGuardrail` fail-closes any change away from an exactly-zero
    current value (relative change from zero is undefined → out of bounds, by
    design). So the R5 liquidity floor can only enter via a code/default turn
    (default change + version bump + re-baseline), never via a governed param
    turn. Deliberate governance semantics, but worth knowing before proposing
    `turnover_floor_krw` as a param leg. Owner: next code turn that enables
    the floor.

15. **An `LS_BT_PARAMS_FROM_RUN` count run becomes the latest finalized run.**
    The Turn-N count run adopts a prior identity's params and finalizes into
    the shared registry; the next `turn` resolves current params from it.
    Mitigated: the bin now requires `LS_BT_VERSION` (distinct version) and
    prints a reminder to pin `LS_TURN_EXPECT_VERSION`; `runs compare` now
    fails on a `universe_metadata_hash` difference. Residual: `turn` itself
    never sets `metadata_path`, so a metadata-gated baseline vs ungated turn
    run still needs the operator to notice the compare FAIL. Owner: a
    turn-integrated metadata mode if Turn N+1 keeps the engine.
