---
title: "A guard's verdict certifies nothing unless you ran the right binary and it was handed a true claim — prove the artifact, then discharge the residual"
date: 2026-08-04
category: workflow-issues
module: adapters/nautilus calendar snapshot cadence (RUNBOOK-calendar-snapshot.md § "Forward-readiness decay", scripts/session-morning.sh, src/calendar_refresh/candidate.rs, src/calendar_refresh/fetch_state.rs)
problem_type: workflow_issue
component: tooling
severity: high
applies_when:
  - "Running a guard, gate, or refusal path for the first time after the PR that shipped it merged — before any run has exercised it in anger"
  - "A runbook documents a recipe as `cargo run --release --bin X` while a script invokes the same binary as a prebuilt path, so which code actually executes depends on the invocation form"
  - "A guard validates a claim some upstream fetcher reports, so it can catch a missing claim but never a wrong one"
  - "The PR that shipped a guard names a residual it does not cover"
  - "Advancing `freshness.forward_readiness_through` on the KRX calendar snapshot via `calendar-refresh`"
  - "Reading a forward-window refresh result and deciding whether `unknown` rows or a changed `calendar_id` are a defect"
tags:
  - stale-binary
  - silent-refusal
  - absent-signal
  - guard-residual
  - offline-krx-calendar
  - forward-horizon
  - operator-runbook
  - kasi
---

# A guard's verdict certifies nothing unless you ran the right binary and it was handed a true claim

## Context

On 2026-08-04 the owner-local KRX calendar snapshot's forward horizon was advanced
`2026-09-06 → 2027-07-22` — the **first real-world exercise** of the per-source forward guard
shipped by [PR #258](https://github.com/sunkeunchoi/korea-adapter-sdk-ls/pull/258) (merged,
reachable from `971e906`). The advance succeeded, but two near-misses surfaced on the way, and they
share a single thesis:

> **Both defeat the guard from outside its reach** — one *upstream* of it (you ran a binary that
> does not contain it, so it never executes) and one *beneath* it (it validates a claim the fetcher
> reports, and for an unpublished holiday year that claim is itself false). A guard's verdict
> certifies the logic it encodes. It certifies neither its own presence nor the truth of its inputs.

The guard is `evidenced_forward_horizon` (`adapters/nautilus/src/calendar_refresh/candidate.rs:337-357`),
which advances `freshness.forward_readiness_through` only when `forward_span_is_evidenced`
(`candidate.rs:368-391`) finds the KASI and generated-rule sources both **present**, `ok`, and
carrying `covered` ranges spanning every date past the prior horizon. Why it is written that way is
[`status-only-gate-is-not-evidence-and-all-over-empty-is-true`](../logic-errors/status-only-gate-is-not-evidence-and-all-over-empty-is-true.md);
this doc is the operator-side complement — what running it for real requires.

## Guidance

### 1. Prove the new code path is in the artifact you are about to run — before you run it

`calendar-refresh` prints exactly three forward-horizon verdicts — `advanced` / `unchanged` /
`REFUSED` — from a three-arm match at `adapters/nautilus/src/bin/calendar-refresh.rs:97-113`. That
printing **did not exist before #258** (at `971e906^`, `calendar-refresh.rs` contained zero
occurrences of `forward_horizon`, and `candidate.rs` copied the prior value forward unconditionally),
and the code comment directly above it (`calendar-refresh.rs:91-94`) states why it was added: the
candidate diff compares rows, coverage and evidence but *not* freshness, so a refused extension is
otherwise indistinguishable from "there was nothing to extend."

Pre-#258 code cannot advance the horizon **and** emits no `forward_horizon=` line at all. An
operator running a stale binary therefore sees an unmoved horizon and no diagnostic — which reads
exactly like "the guard evaluated the evidence and refused." The fix that made refusal observable is
itself unobservable when absent.

Two invocation styles coexist in this tree and they differ on this point:

- The RUNBOOK's forward-extension recipe uses `cargo run --release --bin ...`
  (`adapters/nautilus/RUNBOOK-calendar-snapshot.md:167-173`), which **rebuilds** — safe by
  construction.
- `adapters/nautilus/scripts/session-morning.sh` pins `BIN="$NAUT/target/debug"` (line 68) and
  invokes `$BIN/calendar-fetch-inputs`, `$BIN/calendar-refresh`, `$BIN/calendar-activate`,
  `$BIN/calendar-status` as **prebuilt paths**. Its preflight (lines 223-228) checks only that each
  path exists (`-e`); it never checks age or content. Note the two forms do not even share a build
  directory, so rebuilding via the runbook's `--release` form does not freshen what the script runs.

At the start of the 2026-08-04 session the prebuilt `target/release/calendar-*` artifacts were dated
Jul 30 and Jul 23, predating the #258 merge. The check that closes it, in order:

```sh
cd adapters/nautilus
ls -la target/release/calendar-* target/debug/calendar-*     # mtime vs the merge date
cargo build --release --bin calendar-fetch-inputs --bin calendar-refresh --bin calendar-activate
strings target/release/calendar-refresh | grep -c 'REFUSED (asked for'   # want 1
```

Both invocation styles need their own assertion, and the release rebuild above satisfies only one
of them — the two forms do not share a build directory, so it does not freshen what
`session-morning.sh` runs. For the `target/debug` artifacts that chain pins:

```sh
cd adapters/nautilus
# --workspace is required even from here: lab-research and lab-mount-universe live in the `lab`
# member, not the default-run package, so a bare --bin fails to resolve them.
cargo build --workspace --bin calendar-refresh
grep -qa 'REFUSED (asked for' target/debug/calendar-refresh && echo "guard present in target/debug"
```

Presence, not a count, and for a reason the paragraph below spells out. Keep both checks: the
release recipe is what `RUNBOOK-calendar-snapshot.md` drives, the debug one is what the morning
chain executes, and the whole point of this section is that they drift.

mtime alone is weak evidence — a rebuild for an unrelated reason also bumps it — while the `strings`
assertion says the **behavior** is in the binary. Do both.

Choose the probe string for **uniqueness**, not convenience. `grep -c forward_horizon` also works
today (it returns `3`), but all three verdict literals share that byte-identical prefix, so the count
is an artifact of the compiler not merging them — an unrelated edit could collapse it to `1` and turn
a healthy binary red. `REFUSED (asked for` appears exactly once and exists only in the post-#258
code, so it asserts presence without depending on literal-deduplication behavior.

This trap reaches further than the forward-extension cadence. `session-morning.sh`'s argv is guarded
by replaying the script's captured argv against **the real compiled binary** rather than a stub
mirror (see [`shell-script-live-path-needs-stubbed-binary-tests`](shell-script-live-path-needs-stubbed-binary-tests.md)).
That is the right design for argv, but it does not cover freshness, and three details compound:

- The replay's oracle is a hardcoded prebuilt path
  (`adapters/nautilus/scripts/tests/session-morning.test.sh:50`), and
  neither the test nor `make script-check` ever runs `cargo build`. Its verdicts are
  accepted / no-binary / rejected — **there is no freshness axis at all.** It reports a *missing*
  binary, never a stale one.
- So **a pre-merge binary makes the chain and its own argv guard agree on stale behavior.** Both
  read the same artifact; neither can see that it is old.
- The replay covers only `calendar-fetch-inputs`. `calendar-refresh` — the binary that actually
  carries the #258 guard — is stubbed and never replayed.

Freshness is a separate property from argv correctness, and for those three reasons the argv guard
cannot be stretched to cover it.

### 1a. That residual is now discharged — by two axes, not one

`session-morning.sh`'s preflight no longer merely tests that its twelve required paths exist. The
seven `$BIN` entries are refused with **exit 64**, before any gateway traffic, on any of four
*discriminated* causes: **absent**, **stale by mtime**, **registered guard literal absent**, or
**freshness unevaluable**. Each arm names its own remedy, because a handler that cannot tell apart
the ways it fires will assert the one cause its author had in mind — see
[`shell-script-live-path-needs-stubbed-binary-tests`](shell-script-live-path-needs-stubbed-binary-tests.md).

The pairing this section prescribes is load-bearing, and neither half is redundant:

- **mtime** compares each binary against the source set cargo already recorded in
  `$BIN/<name>.d`. Reading metadata cargo has persisted is not the same as delegating the verdict
  to cargo — cargo has no check-only mode, so delegating would mean auto-remediation instead of
  refusal, and an unbounded rebuild inside a 09:05 deadline. Cargo's set also reaches what no
  hand-listed `src/` scan would: `crates/ls-core/build.rs` embeds the **repo-root `metadata/`
  tree** at compile time, so a `metadata/constraints/*.yaml` edit changes every binary's behaviour
  while moving no file under any `src/` directory. And because the set is **per binary**, rebuilding
  one stale binary clears its own refusal instead of leaving six others behind a shared timestamp
  that cargo then declines to rebuild.
- **the content literal** is the only axis that can see an **inverted** binary — newer than every
  source yet built from older code. A build racing a `git pull`, a build made in another worktree
  or branch, and `touch target/debug/*` all produce that state, and the last of those is the
  cheapest operator response to a false-stale. `calendar-refresh` is registered with
  `REFUSED (asked for` precisely because that is the guard whose absence reads as a clean pass.

Which is why the operator override for deliberately pinned binaries
(`LS_SM_ALLOW_STALE_BINARIES=1`, permitted on a real run and announced in the transcript) covers
the **mtime axis only**. A binary pinned on purpose is still pinned to code containing its
registered guard, so nothing legitimate needs that escape — and binding both axes to one switch
would let the noisy axis train the operator into disabling the quiet one.

Two limits survive the discharge, and both are real:

- It guards **`target/debug`**, which is what the chain pins. The `--release` artifacts — the ones
  that were actually stale in this incident — are **not** covered by it. That is why the recipe
  above keeps both assertions rather than replacing one with the other.
- `make script-check` still never *builds* anything. It now runs as `make gate-run` step 7,
  immediately after `adapter-check` — which is the step that produces `target/debug` — so it
  **consumes** those artifacts rather than making them, and reports loudly when one is absent.

The `calendar-refresh` half of that residual is now discharged too, on the two axes the gap
actually had (`scripts/tests/session-morning.test.sh`):

- **argv.** Step [4]'s marshalled argv is replayed against the real compiled `calendar-refresh`,
  credentials stripped, from a foreign CWD — the same construction step [3] already had. The
  oracle is a positive one: `run()` does `Args::parse` and then `KrxCalendar::load_from_path`
  and nothing else in between (`calendar-refresh.rs:45-49`), so reaching the snapshot-schema error
  proves every required flag was present and every value parsed, and it is reached long before
  `write_candidate`, so the replay mutates nothing. Two mutation meta-tests (`--mode` stripped,
  `--through` stripped) prove the assertion can see a broken step [4].
- **the registered literal, in the artifact.** R11 greps every `BIN_PROBE_LITERALS` literal out of
  the real `target/debug/<binary>` with the same `grep -qaF` the preflight runs. This is the axis
  that was genuinely absent: R10 checks the Rust *sources*, and the fixture *plants* each literal
  into its stub, so the content-axis tests pass by construction whatever the built tree contains.

What is still uncovered is the **mtime** axis against a real artifact — nothing in the test target
can tell a current `target/debug` from one built before the last `git pull`. That stays the
preflight's job at 08:45, deliberately: reproducing it would mean the test target owning a build.

### 2. Discharge the guard's known residual by probe, BEFORE choosing the input

`fetch_kasi_year` (`fetch_state.rs:244-274`) returns `Ok(holidays)` for any parseable response,
including a zero-holiday one. The caller then unconditionally stamps
`state.kasi_covered_through = Some(year_end_clamped(year, cfg.window.through))`
(`fetch_state.rs:225-226`, helper at `fetch_state.rs:404-408`) — i.e. **a not-yet-published year is
recorded as a fully covered year**. `assemble_inputs` turns that into an `ok_covering` outcome
(`fetch_state.rs:307-313`, `source_outcome` at `fetch_state.rs:325-340`).

The forward guard validates the `covered` **claim the fetcher reports**. When that claim is itself
wrong, the guard passes a horizon into an unpublished year and reports `advanced`. This is not
fixable inside the guard. It is an operator judgement, and the runbook already says so in prose
(`RUNBOOK-calendar-snapshot.md:202-206`) — what was missing was a recipe and any record of the
residual having been discharged. This run is the first discharge.

It is cheap. A direct credential-safe probe of the KASI `getRestDeInfo` endpoint answered it in
seconds. Probe the target year and the years on either side; print **only** `resultCode`,
`totalCount`, and the min/max date — never the key, never the full URL. On 2026-08-04:

| year | `totalCount` | span |
|------|--------------|------|
| 2026 | 22 | — |
| **2027** | **24** | `20270101..20271227` |
| 2028 | 19 | — |

2027 came back with a full 설날 / 추석 / 대체공휴일 set — published, not a stub. Verify the year is
published, *then* choose the horizon. (These figures are live-probe observations; they have no
witness in the tracked tree.)

Generalize the step, not the instance: when the shipping PR of a guard names a residual, that
residual is a checklist item on every run, and the question to ask is *"is it dischargeable by cheap
evidence, and can I gather that evidence before committing to the input?"* After the fact you cannot
distinguish `evidenced` from `stamped as evidenced`.

### 3. The forward-extension recipe that worked

From `adapters/nautilus`, with `.env.calendar` sourced (it exports `LS_KRX_APPKEY` +
`LS_KASI_SERVICE_KEY`; neither ever rides in an argument — `calendar-fetch-inputs.rs:126-138` adds
KRX's `AUTH_KEY` **header** and KASI's `serviceKey` **query param** at the composition root), after
rebuilding:

```sh
calendar-fetch-inputs --window 2026-09-06..2027-07-22 --krx-through 2026-09-06 \
  --inputs-out state/forward-20260804.calendar-inputs.json \
  --state state/forward-20260804.calendar-fetch.ckpt --state-root state --pace-ms 500

calendar-refresh --active state/krx.calendar.json --as-of <RFC3339 now UTC> \
  --mode incremental --through 2027-07-22 --inputs state/forward-20260804.calendar-inputs.json
```

Then: read the `forward_horizon=` line → review the candidate diff → **archive** the active snapshot
(copy + `cmp`, never a move, never a clobber) → author an `ActivationApproval` naming the exact
candidate `artifact_id` → `calendar-activate`.

Load-bearing notes:

- **Forward-only window.** Start at the current horizon, not the `2010-01-04` history floor — the
  floor re-walks the KRX per-weekday loop over fifteen years for nothing
  (`RUNBOOK-calendar-snapshot.md:175-178`).
- **Pass `--state-root` explicitly.** `DEFAULT_STATE_ROOT` is `"state"`, *relative to CWD*
  (`calendar-fetch-inputs.rs:36-37`, resolved at `:271-273`). The same footgun that bit
  `session-morning.sh` step [3].
- **`--krx-through` = the current horizon**, not the new one. KRX witnesses only the past.

## Why This Matters

The guard's refusal path is deliberately quiet in the artifact: `evidenced_forward_horizon` returns
the *prior* value on refusal (`candidate.rs:352-356`). #258 added the `forward_horizon=` line
precisely because refusal is otherwise unobservable. That fix works — but only if the binary you run
contains it. A stale binary reintroduces the exact ambiguity the line was added to remove, one level
up: now "no line at all" is the silent state. It is the general shape of
[`making-a-failure-graceful-can-delete-the-signal-that-detected-it`](../architecture-patterns/making-a-failure-graceful-can-delete-the-signal-that-detected-it.md),
reached from the operator's side.

The KASI residual is the same failure mode at the data layer. `Ok` from the endpoint answers *"did
the request parse"*, not *"is this year published"*. `fetch_kasi_year` cannot tell an empty year from
an unpublished one, so it reports the strictly stronger claim, and the guard — honest about the claim
it is handed — passes it through. #258 converts *status* into *evidence* one level down; nothing
converts *parseability* into *publication*. Only a probe does. That is the same instinct as
[`vendor-sample-endpoint-evidence-describes-the-sample-not-the-product`](../conventions/vendor-sample-endpoint-evidence-describes-the-sample-not-the-product.md):
vary the input and observe before inferring.

Note the asymmetry both facets exploit: **coverage still widens on status alone.**
`materialized_through` / `scheduled_closure_evaluated_through` advance under the bare
`all_sources_ok` test (`candidate.rs:76`, `:150-169`), while `forward_readiness_through` demands
covered ranges (`:238-242`). A candidate showing coverage through the new window end while the
forward horizon stays put is the guard working, not a bug.

This repo has already paid for the stale-binary class once and solved it *structurally*: PR #155
shares the walk-and-hash source between `build.rs` and the runtime via `include!` so a lab binary can
prove at run time which tree built it — see
[`build-runtime-hash-parity-via-shared-include`](../design-patterns/build-runtime-hash-parity-via-shared-include.md).
That doc's Governed Freshness Protocol makes refusal the only outcome of a fingerprint mismatch —
a lab binary built from a tree that no longer matches halts rather than reporting — and that
guarantee holds **only for binaries carrying the fingerprint**. The calendar tools carry none, so
for them a leftover binary produces exactly a false green — a missing `REFUSED` line read as a
clean pass. Where the structural self-check is absent, the check falls to the operator's hands.

## When to Apply

- **The first run of any newly-shipped guard, gate, or refusal path.** Before trusting the verdict,
  prove the artifact producing it postdates the merge — and prefer a content assertion (`strings`,
  `--version`, an embedded fingerprint) over mtime.
- **Any workflow that has both a `cargo run` recipe and a prebuilt-path script.** They will drift.
  Audit the script's preflight: an existence check is not a freshness check. `session-morning.sh`
  now has one (§1a) — for `target/debug` only, so the release-path recipe still needs running by
  hand. When adding a freshness check elsewhere, take the source set from cargo's dep-info rather
  than a hand-listed directory scan, and refuse rather than rebuild.
- **Whenever a guard ships whose absence is indistinguishable from its refusal.** Register a
  content literal for it in that script's probe registry, chosen for **uniqueness** rather than
  convenience, and test presence rather than a count. A sparse registry that grows when a guard
  ships says something; one filled in on a schedule asserts nothing.
- **Whenever a guard's own docs or shipping PR name a residual.** A named residual is a checklist
  item, not background reading.
- **Before choosing a calendar forward horizon**, or any parameter whose validity rests on an
  upstream publication schedule the fetcher cannot observe.

## Examples

### The 2026-08-04 run — results, and the shapes that are correct

```
forward_horizon=2026-09-06 -> 2027-07-22 advanced
```

- freshness `stale` → `fresh`; coverage `2010-01-04..2027-07-22`; **6090 → 6409 rows**;
  `partial=false`; **0 high-risk** of 451 diff entries; candidate `alerts: []`.
- All three sources reported `ok=true`: KASI covered `2026-09-06..2027-07-22`, krx-rule likewise
  (generated rules are deterministic and always span the whole window — `fetch_state.rs:314-316`),
  and **krx-daily covered only its single date** — `2026-09-06` is a Sunday, so `is_weekday` skipped
  the fetch (`fetch_state.rs:178`, `:400-402`) while the date was still marked completed (`:203`).
  krx-daily is **exempt from the forward-span requirement by design** (`candidate.rs:359-363`,
  `:375`), because it witnesses only the past.

Four results that look wrong and are not:

1. **The 319 new rows split 215 `unknown` / 104 `closed`.** Correct for a forward window: every
   future weekday stays `Unknown` until a KRX witness lands retrospectively — see
   [`todays-session-cannot-be-ingested-tonight-the-krx-witness-is-retrospective`](todays-session-cannot-be-ingested-tonight-the-krx-witness-is-retrospective.md).
   Not a defect.
2. **`calendar_id` changes whenever any effective status or decisive claim changes** — which in
   practice is every refresh. All five archives on disk carry distinct `calendar_id`s despite
   identical row counts. Not a regression.
3. **Coverage can outrun the forward horizon** — see the asymmetry above.
4. **No repo gate was run, and none was needed.** The entire change lands in gitignored `state/`;
   there is no tracked diff to gate. Worth stating because a reader will look for the gate line.

### The horizon is licence-bounded, not data-bounded

KASI publishes into 2028 (`totalCount=19`), so data was not the constraint. `2027-07-22` was chosen
because the snapshot's `authorization.expires_at` is `2027-07-23` and the earliest-expiring
per-service KRX term — `stk_bydd_trd`, the daily-market endpoint at `fetch_state.rs:385-390` — ends
`2027-07-22`. The loader rejects an expired authorization, so the term, not the upstream data, caps
the choice. (Both dates are owner-local operator facts read from `state/` and the KRX portal; the
term end has no witness in the tracked tree.)

### The negative case to keep in mind

Had the stale `target/release` binaries been used, the observable output would have been: no
`forward_horizon=` line, an unchanged `stale` verdict, a candidate diff that looks plausible, and
exit 0. Nothing in that output distinguishes "pre-#258 code, guard absent" from "post-#258 code,
guard refused." That is the whole learning.

## Related

- [`status-only-gate-is-not-evidence-and-all-over-empty-is-true`](../logic-errors/status-only-gate-is-not-evidence-and-all-over-empty-is-true.md)
  — why the guard checks covered spans, and where this residual was first recorded
- [`build-runtime-hash-parity-via-shared-include`](../design-patterns/build-runtime-hash-parity-via-shared-include.md)
  — the structural fix for stale binaries, applied to the lab fingerprint (PR #155); scopes to
  binaries that carry the fingerprint, which the calendar tools do not
- [`shell-script-live-path-needs-stubbed-binary-tests`](shell-script-live-path-needs-stubbed-binary-tests.md)
  — `session-morning.sh`'s live path and its argv-replay guard, whose fidelity depends on the built
  artifact being current
- [`krx-client-side-timeout-manufactures-a-dead-source-and-a-witness-less-snapshot`](../integration-issues/krx-client-side-timeout-manufactures-a-dead-source-and-a-witness-less-snapshot.md)
  — the same absent-vs-negative confusion at the fetch layer, in this subsystem
- [`making-a-failure-graceful-can-delete-the-signal-that-detected-it`](../architecture-patterns/making-a-failure-graceful-can-delete-the-signal-that-detected-it.md)
  — the general statement of the class
- [`coverage-only-change-is-verified-by-mutation-not-by-the-gate`](../conventions/coverage-only-change-is-verified-by-mutation-not-by-the-gate.md)
  — the test-suite form of the same demand: prove the change is present, do not infer it from a
  quiet outcome
