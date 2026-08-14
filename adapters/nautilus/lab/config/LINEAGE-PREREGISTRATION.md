# Lineage pre-registration — the frozen terms of the successor daily lineage

**Frozen 2026-08-15.** Machine-readable record:
[`lineage-preregistration.json`](lineage-preregistration.json).
Derived by plan `docs/plans/2026-08-14-001-feat-next-lineage-preregistration-artifact-plan.md`
(P6 of the ladder in `docs/plans/2026-08-10-001-docs-next-strategy-lineage-scope-plan.md`).

This is **not** the ladder pre-registration and **not** the sample margin.
[`preregistration.json`](preregistration.json) freezes the production ladder's rung dosing
and expectation bands; [`sample-margin.json`](sample-margin.json) freezes the bar a future
*ORB* head must clear. This file freezes a **strategy lineage's search terms**: how much
session supply exists, how it is partitioned, how many looks the holdout may take, what
effect size is being hypothesized, and what predicate resolves the verdict. The three
artifacts share the word "pre-registration" and nothing else. The ladder pair is untouched
and stays byte-identical.

**This freeze does not open the lineage.** The TURN-LOG's standing block still reads
`currently open: NONE`. Opening happens in a later commit, gated on the pre-turn
admissibility re-check clearing — see [§ The two gates](#the-two-gates).

---

## Why freeze at all

A load-bearing choice left open after the specification window has been observed is not
pre-registered — it is a choice made after seeing the data. The ORB lineage is closed. Its
successor cannot start a turn until its terms are written down in a form a program can
read, a test can check, and a second evaluation can be refused against.

---

## The supply and its split

| | sessions | from | to | first session | last session |
|---|---:|---|---|---|---|
| **specification** | 837 | 2016-08-01 | 2019-12-31 | 2016-08-01 | 2019-12-30 |
| **holdout** | 1566 | 2020-01-02 | 2026-05-20 | 2020-01-02 | 2026-05-20 |
| **reserved** | 57 | 2026-05-21 | 2026-08-12 | 2026-05-21 | 2026-08-12 |
| **S_max** | **2460** | 2016-08-01 | 2026-08-12 | | |

The specification window's declared end, `2019-12-31`, is a **closed** day. Its last actual
session is `2019-12-30`. Both are recorded so nobody has to wonder which one the count used;
a closed day at a boundary contributes no session to either side, and the derivation guard
proves no session falls in the gap between one partition and the next.

### The ceiling reconciliation

The origin plan reports 2,457 sessions and the P4 pit walk reports 2,460. This is a
**date-range mismatch, not a disagreement**: the origin counted from the 2016-08-01 floor
through `2026-08-07`, and the walk counted the same floor through its own `2026-08-12`
anchor. The three-session delta is exactly the proven sessions in
`2026-08-08 ..= 2026-08-12`, and it lands **entirely inside the reserved tail**. The
specification and holdout counts — the two that move the margin bar — are identical under
both readings.

### These counts are citation-reproducible, not test-reproducible

The KRX calendar snapshot at `adapters/nautilus/state/krx.calendar.json` is machine-local
and gitignored; CI checks out a tree without it. No committed test can recount these
sessions. What the artifact records instead is the snapshot's two identities
(`artifact_id` `24ca3145…`, `calendar_id` `b4382ad3…`), so the count can be reproduced by
restoring a snapshot with those identities and running the operator harness:

```bash
cd adapters/nautilus
LS_CALENDAR_SNAPSHOT=state/krx.calendar.json \
  cargo test -p nautilus-ls-lab --test lineage_prereg_derive -- --ignored --nocapture
```

Every day in the ceiling range was proven open or proven closed in that snapshot — zero
`Unknown` days — so these are counts, not lower bounds. The committed guard asserts the
split's *relationships* (it sums to the ceiling, nothing is stranded between partitions,
the bar reproduces at the frozen holdout count) against synthetic calendar facts.

---

## The verdict

```
clears  ⟺  observed_net_ror − haircut > bar
```

| term | value | where it comes from |
|---|---:|---|
| `bar` | 0.0289065117 | `pit_walk::margin_bar_n1(1566)` = `1.96 × 0.087002 × √(45/1566)` |
| `haircut_fraction` | 0.25 | the frozen **choice** |
| `haircut` | 0.0072266279 | derived: `haircut_fraction × bar` |
| `hurdle` | 0.0361331396 | derived: `bar + haircut` |

The statistic is net RoR — `Σ realized / Σ risk_capital` — under a session-block bootstrap
with a **16-session block**, never shorter than the holding period. A one-session block
would assume independence between blocks that a 16-session hold spans straight across,
understating the standard error. The guard asserts `block ≥ hold` rather than a literal, so
the two cannot drift apart.

### Why the haircut is a fraction and not a constant

The haircut covers survivorship and eligibility bias. The pit universe was enumerated as of
the `2026-08-12` anchor, so it structurally cannot see names that delisted inside the
ceiling range. **Its magnitude cannot be estimated** without delisting data the universe
does not have — so it is a pre-registered conservative constant, not an estimate.

Freezing it as a **fraction of the bar** rather than an absolute number buys three things: it
re-derives automatically if the bar moves, it needs no external data, and it can only ever
be tightened. Later evidence may raise this fraction. It may not lower it.

---

## The hypothesis

| term | value | basis |
|---|---:|---|
| effect size, ratio to ORB's gross | 3.8804× | derived |
| effect size, net RoR | +0.0485456 | derived |
| effect size, gross R | +0.1102878 | derived |
| holding period | **16** sessions | derived: `ceil(ratio²)` |
| directionality | long only | chosen |
| target trades per session `m` | **8** | chosen, bounded by measurement |
| target session participation `p` | 1.0 | structural |
| steady-state concurrency | **128** | derived: `m × hold` |
| selection breadth | 0.5246 | derived: `128 / 244` |
| stop rule | 1.5 × ATR(1 session), per position | **inferred** width |

### Why the effect size is measured against the hurdle, not the bar

The origin plan derived `≥14` sessions and `3.626×` at 0.80 power against the **bare** bar.
The haircut raises the hurdle, so the figures must move with it — otherwise the lineage is
registered at roughly 64% realized power, and a true effect gets closed on measurement about
a third of the time. Re-solving at `hurdle + z(0.80)·SE(1566)` gives a net target of
`+0.0485456`; adding ORB's measured round-trip cost of `0.0617422 R` gives a required gross
of `+0.1102878 R`, which is **3.8804×** ORB's measured `0.028422 R`. Under √-time scaling
the implied horizon is `3.8804² = 15.06`, so the holding period floor is **16 sessions**.

### Why `m = 8` and not 10

The 16-session hold forces it. At `m = 10`, steady-state concurrency is 160 — and the P4
walk's derived block carries **verified** threshold rows only at 70 and 140. Freezing 160
would freeze supply the walk never measured. At `m = 8`, `m × hold = 128` sits under the
verified 140 row, and the clustering requirement at `m = 8` is bracketed by two computed
rows that both clear the 2,460 ceiling, so no new table row has to be invented. Selection
breadth is `128 / 244 = 0.5246` against the universe's **floor** listed count — the tightest
supply moment in the range, so that is the widest the breadth ever gets.

### Two participation numbers that are not the same number

The clustering table's `p` is the fraction of **sessions the strategy trades**. A
take-top-N-every-session ranking has `p = 1.0` by construction, so that is what
`hypothesis.target_session_participation` freezes.

The walk's `mean_participation` of `0.856407` is a different quantity entirely: mean
per-symbol **listing depth**, which `pit_walk.rs` documents as an *upper bound* on tradable
participation, and which is survivorship-biased upward because the universe was enumerated
at the anchor. It is recorded separately as `supply.universe_listing_depth`, flagged as an
upper bound, precisely so the two cannot be conflated.

### Why the stop rule is frozen at all

`cost_R = 0.0023 / stop_pct`. Transaction cost is fixed per round trip and asymmetric to the
sell side — 20 bps sell tax plus 1.5 bps per side, 23 bps round trip
([`transaction-costs.json`](transaction-costs.json)). ORB's measured cost of
`0.0617422 R` therefore inverts to an average stop of **3.73% of price**. Changing the stop
multiple changes `cost_R` and therefore the required gross edge and everything derived from
it, so the multiple is frozen and the guard asserts it against the frozen cost figure.

---

## The search budget

`N_max = 1`. One look at the holdout, ever.

`sigma_trials` is **null** — and null by design, not by omission. At `N_max = 1` the
expected maximum of a single draw from a zero-mean null is exactly zero for *any* dispersion,
so cross-trial dispersion cannot enter a single judgment's arithmetic. It would become
load-bearing the moment `N_max` rose above 1, which is a new pre-registration rather than an
amendment.

**The lineage-level multiplicity is stated, not corrected.** The schedule permits at most
**3** one-sided judgments of this lineage — the turn-one holdout judgment plus the 2
scheduled upgrade turns — so the lineage's lifetime false-pass rate is roughly three times
the per-judgment rate. No lifetime correction is applied; the finite cap *is* the control.
This is recorded explicitly so a reader does not infer from a null `sigma_trials` that no
multiplicity exists.

---

## The upgrade schedule, and why turn one is the only real shot

A segment is only real if the registered effect clears **its own** `bar + haircut` at the
registered power. Solving that at the frozen effect gives a floor of **1566** proven
sessions — identical to the turn-one holdout, because the power calculus is the same.

Both scheduled turns therefore draw a 1,566-session segment, and 2 turns is roughly
**12.7 years** of forward accrual at ~246 sessions a year.

Say that plainly: **turn one is this lineage's only shot within any realistic planning
horizon.** The schedule is finite and honest, but it is not a second chance anyone should
plan around. A shorter segment cannot be substituted — a 500-session segment carries a
post-haircut hurdle of `+0.0639` against a registered effect of `+0.0486`, and could never
be passed.

Exhausting the schedule is a **lineage-closure condition**. There is no extension: adding a
turn after the fact would retroactively invalidate the false-pass rate this freeze
registers.

---

## The refusal mechanic

Every holdout evaluation appends an attempt record — run id, catalog fingerprint, UTC — to
[`../ledger/lineage-holdout-judgments.jsonl`](../ledger) **before** it computes a verdict. A
second evaluation finds the attempt and returns an error naming the recorded run id and UTC.

The ordering is the whole point. Claiming first closes the hole where an operator evaluates,
dislikes the answer, declines to write it back, revises, and evaluates again without ever
seeing an error. Declining to record a verdict does not buy a second look, and neither does
a crash mid-verdict.

The frozen artifact's `holdout_judged` field stays `null` forever — judgments live in the
ledger, so this file's bytes never change and its content-hash citation survives the single
judgment.

**The refusal is git-auditable, not tamper-proof.** A revert can remove the ledger line and
restore the ability to judge. What the mechanic buys is that a second judgment cannot happen
*silently*: it requires an explicit, reviewable deletion in the history.

---

## The two gates

**Before the lineage opens** — the pre-turn admissibility re-check. The bar frozen here is a
*projection* under ORB's clustering, not a measurement of this lineage. Before any turn runs,
this lineage's own ICC, realized trades-per-session, and realized session participation are
measured on the **specification** window and the admissibility case re-evaluated at those
values. If the class no longer clears, the re-check **refuses to open** the lineage. It does
not close it, and it does not lower the bar.

**After a clearing judgment** — the prospective paper stage. A holdout judgment that clears
is not a successful lineage. Labelling it successful additionally requires the specified
strategy to run forward on the paper lane under the live designation policy, with its
realized behaviour compared against the registered effect. The paper lane can never accrue
clean sessions toward a production rung on its own; the stage is a falsification
opportunity, not a qualification.

---

## What this freeze does **not** claim

- **`005930`'s vendor floor is inferred, not measured.** The walk never observed a refusal
  at that symbol's earliest date; the floor is inferred from the served range.
- **The observed page cap *is* measured.** The walk requested 900 rows and observed a
  maximum of 501. 501 is strictly below 900, which is the condition `pit_walk.rs` names for
  measured status. These two are stated together because they are easy to conflate and only
  one of them is a caveat.
- **The margin bar is a projection.** `bar(N=1, S) = 1.96 × 0.087002 × √(45/S)` projects ORB
  v35's session-block bootstrap SE under *ORB's* clustering onto S sessions. This lineage's
  own clustering is unmeasured until the pre-turn re-check.
- **`universe_listing_depth` is a survivorship upper bound**, not a measurement of tradable
  participation.
- **The lineage-level multiplicity is stated, not corrected.**
- **The split counts are citation-reproducible, not test-reproducible.**
- **The refusal is git-auditable, not tamper-proof.** Two further boundaries of the same
  shape: the content-hash citation binds the **bytes read at load time**, so a caller that
  mutates the parsed values in memory before judging would record a citation that does not
  describe the terms it used; and the loader refuses a *drifted file*, but nothing stops a
  caller holding a legitimately-loaded artifact from lying to itself. Both sit outside the
  mechanic exactly the way a revert does — the control is that the artifact and the ledger
  are both committed and reviewable.
- **The judgment entry point cannot verify its input came from the holdout.** It takes the
  observed statistic as a bare number. The window check on the specification dry run
  refuses the obvious mistake and makes the declared scope explicit; it does not bind a
  statistic to the dates that produced it. Binding it needs a typed observation carrying
  its own date range and catalog fingerprint, and there is no producer to type against
  until the daily multi-session-hold backtest path exists (P7). Until then this rests on
  the operator, and the recorded attempt's catalog fingerprint is the audit trail.
- **The committed derivation guard cannot recount the split.** It proves the counts sum to
  the measured ceiling, that the outer boundaries match the P4 walk's own floor and anchor,
  that a partition starts on a session, and that the reserved tail opens the day after the
  holdout closes — but a coordinated edit moving an *interior* boundary and its session
  count together would pass, because no committed test has a calendar. That is the price of
  citation-reproducible counts; the `#[ignore]`d operator harness is what recounts them.
- **The 1.5× ATR stop's equivalence to a 3.73%-of-price width is inferred**, not measured on
  this universe.
- **Turn one is effectively the only shot** within a realistic planning horizon.

---

## When this freeze becomes invalid

The artifact's `rederivation_trigger` is authoritative. In short: a different catalog
fingerprint, an edit to
[`pit-universe-20260812.json`](pit-universe-20260812.json) (its hash is cited in the
trigger), a calendar re-count that moves the specification or holdout session count, a
pre-turn re-check that moves the projected bar, or `N_max` rising above 1. Any of these means
every figure above is re-derived before a turn runs — not amended.

---

## Where the pieces live

| | |
|---|---|
| frozen terms | [`lineage-preregistration.json`](lineage-preregistration.json) |
| loader + content hash, judgment ledger | `../src/lineage_prereg.rs` |
| derivation guard (hermetic) | `../tests/lineage_prereg_derivation.rs` |
| operator recount harness (`#[ignore]`d) | `../tests/lineage_prereg_derive.rs` |
| judgment ledger | `../ledger/lineage-holdout-judgments.jsonl` (created on first append) |
| the standing open-lineage block | [`../TURN-LOG.md`](../TURN-LOG.md) |
