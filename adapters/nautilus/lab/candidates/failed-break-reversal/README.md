# failed-break-reversal — Lever 8 pre-register (plan 2026-07-22-001)

An **additive entry-stream** lever, not a re-weighting of existing trades. Lever 8 adds a second,
long-only entry stream to the ORB strategy that trades the **failure of a confirmed downside break**
of the opening range — a confirmed close below the fixed range low followed by a confirmed close back
above it before flat time. The v32 breakout leg is untouched; the governed flip compares v32 against
v32-plus-stream (KTD1). Because the reversal trades do not exist in the head run, there is no
incumbent signal to correlate against, so the stop-geometry **collinearity gates are dropped** (KTD3).

## Head + stop mode (R6)

v32 head `20260717T094841Z-backtest-orb-v32`, RoR **0.1876**, `stop_mode = 0.0` = **RangeLow**,
`entry_confirm = 1.0` = **close-confirm**. Both `diagnostic.py` and `twin.py` assert this mode, the
close-confirm premise, the catalog content fingerprint, and the frozen identity from the manifest
before any reading. A reversal long reuses the Long machinery wholesale (KTD5): target/breakeven use
`r_denom = range_high − range_low` (OR-width, **decoupled** and fixed), so a stop-anchor choice moves
only the stop and — like the breakout leg — is CLASS-B-absorbed on the RoR denominator; only
stop-out geometry survives.

## The two grammars (documented ids)

| id | grammar | population |
|----|---------|-----------|
| 1 | `breakdown-recovery` (PRIMARY, the true inversion) | selected symbol-sessions that took **no v32 trade** (the pure additive population — "sessions that currently never enter"), gated by the same session gates as a breakout entry (gap filter, OR-width, gap-retention — R8), exhibiting a confirmed breakdown then recovery. Enter long at the recovery close. |
| 2 | `post-stop re-entry` (SECONDARY, capped by v32's trade count) | each v32 Long that resolves to `stop` under this screen's barrier re-sim, re-entered at the first later close back above the range low (before flat), anchored at the range low. **A grammar-B win RETURNS TO PLANNING** — it makes the session-terminal state re-entrant, a structurally different change the build units do not cover (Scope Boundaries). |

**Same-bar semantics (KTD6).** Breakdown confirms on a bar CLOSE strictly below the range low (a
wick-below-close-inside does **not** confirm — AE2); recovery on a LATER bar CLOSE strictly above it
(one bar cannot be both). A recovery close above the range high is the breakout leg — it wins, and
such a session took a v32 trade and is already excluded from the additive population. A reversal
entry with zero stop distance (recovery close == stop anchor) is rejected at sizing, never divided
through (the ATR-zero lesson class).

**Stop-anchor sweep (KTD5).** Grammar A is scored under two stop anchors and the best-by-`ror_shift`
anchor wins: `1` = breakdown session low (lowest low from the breakdown bar through the recovery bar),
`2` = range low. Grammar B is anchored at the range low (id 2). `stop_anchor_id` rides the readings as
the arm's companion seed — the code implements only the winner.

## Gates (frozen in candidate.json)

An additive stream keeps two STOP gates (KTD3):

- **Gate 1** `population_count ≥ 12` — a thin population is a NO-BUILD **regardless of projected
  shift**. Below this the additive RoR estimate is dominated by a handful of trades, and under the
  shared `max_concurrent 7` budget the realized post-contention population would be thinner still. Set
  to twice the stop-geometry resolution-moved scale (~6 over 77).
- **Gate 2** `ror_shift ≥ 0.005` — the ADDITIVE shift `RoR(base + winner) − RoR(base)`, both under
  this screen's own barrier re-sim of the v32 baseline (**not** the run's realized 0.1876), so the
  pessimistic-flat-fill bias cancels to first order. Sizing is CLASS-B and **ceiling-aware**
  (`min(floor(budget·w_ratio/rps), floor(notional/price))` — the notional clip the amihud
  mis-prediction omitted). Floor is the standing 0.005: below the smallest historically-KEPT lever
  gain (ratio-ATR +0.0091), above demonstrated screen-prediction noise.

`resolution_target_share` (with `resolution_stop_share` / `resolution_timeflat_share`) is the
**fill-price-independent primary reading** — which barrier a trade hits first is pure geometry,
independent of the fill price, whereas any qty-weighted P&L stat inherits the flat-fill bias. It is a
**recorded** reading (emitted, twin-agreed, disclosed), not a hard threshold: the two pre-registered
STOP gates are the count floor and `ror_shift` (Success Criteria).

## Winner selection (KTD4)

The gate contract is single-winner, so the argmax happens inside both scripts: prefer the PRIMARY
grammar A when it clears both gates (→ BUILD); else grammar B when it clears (→ RETURN-TO-PLANNING);
else the best-by-`ror_shift` among both (its readings then fail a threshold → the tool records STOP =
NO-BUILD). Ties in `ror_shift` break deterministically toward the lower grammar / anchor id.
`winning_grammar_id` (tolerance 0) forces the independently authored twin to agree on the winner; the
winner's readings feed the gate.

**Operator gate on a grammar-B GO (KTD4 / Stop conditions).** Because the flip param `reversal_arm`
implements grammar A only, a machine GO whose `winning_grammar_id == 2` is a RETURN-TO-PLANNING
signal, not a build authorization — the build units (U3–U7) do not cover the re-entrant grammar B. The
threshold gate alone does not distinguish the two (grammar B's readings clear the same count/ror
floors), so the distinction is carried by `winning_grammar_id` in the verdict's `agreed_readings`
(machine-readable, not merely stdout): the U2 operator must inspect it before running `turn governed`,
which reuses any recorded GO. Backstop: even a mistaken grammar-A build off a grammar-B GO would
REVERT at the flip (stream-ON RoR must strictly beat v32), so a losing lever never permanently ships.

`gate-verdict.json` is a command output, not a frozen input.

## Screen result (informational — the committed verdict is the gate's)

Both implementations agree byte-identically. v32 baseline re-sim RoR **0.1522** (77/77 qty
reconstructed exactly — the barrier + sizing model is faithful). Grammar A (breakdown-recovery,
n=53) is **decisively negative** — `ror_shift −0.063`, with **73.6 % of reversal longs stopping out**
and only 3.8 % reaching target: on this large-cap universe a confirmed breakdown tends to continue,
not recover into a sustained move. Grammar B (post-stop re-entry, n=14) is `ror_shift −0.006`. Neither
clears → **NO-BUILD**, the primary hypothesis falsified.

## Limitations (what this screen does NOT establish)

- **Additive upper bound.** The population is scored UNCONSTRAINED: reversal entries contend for the
  same `max_concurrent 7` slots and risk budget as breakout entries, so the realized (post-contention)
  population at the flip can be thinner and displacement of breakout trades is a substitution measured
  only at the flip, never here (KTD3 caveat, recorded in `keep_anchor`). The projected shift is an
  **upper bound** on the additive effect — which makes a negative screen a robust NO-BUILD.
- **`ror_shift` is a conservative lower bound.** The re-sim books targets at the flat target price and
  omits the run's favorable gap-through-limit fills; the bias cancels only to first order. For a
  decisively-negative grammar this only strengthens the NO-BUILD.
- **Twin certifies reproducibility, not fidelity.** `twin.py` re-derives reconstruction (catalog-wide
  maps vs the diagnostic's entry-local loads) and statistics independently but shares the barrier
  SEMANTICS by design, so its byte-identical agreement cannot catch a shared barrier/rounding error.
  Barrier-model fidelity is cross-checked against `orb.rs` (RangeLow decoupled geometry, close-confirm
  no-same-bar-stop, stop-first pessimism, breakeven ratchet to entry).
- **Long-only, one direction.** The standing long-only constraint holds — the inverse (short the
  continuation) is out of scope. A grammar-A NO-BUILD is "the failed-break **long** reversal has no
  edge on this universe", not "no edge exists in the event".
