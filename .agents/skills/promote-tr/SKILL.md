---
name: promote-tr
description: Promote one maintained TR from support:implemented to support:recommended by capturing credential-free Focused Evidence from its existing Paper Live Smoke, using the proven t1101 recipe. Use for a single TR code (e.g. "promote t1102"). Runs non-interactively and state-driven; promotes when the smoke gate opens, otherwise HOLDS the TR with a recorded reason. Not for TRs that lack a smoke harness — those need ce-plan first.
---

Promote exactly **one** TR to `recommended` by recording Focused Evidence from a
green Paper Live Smoke. This is a state-driven recipe: it either PROMOTES the TR
(smoke gate opened, evidence credential-free, suite green) or HOLDS it (records
why, leaves it `implemented`). Never force a promotion without passing evidence.

**Input:** one TR code (the `$ARGUMENTS`, e.g. `t1102`).
**Output (last line, machine-readable):** `PROMOTED <tr> evidence/<tr>.yaml` or
`HELD <tr> — <reason>`.

This skill is **non-interactive** — it asks no questions; it infers everything
from repo state and the smoke result. The `tr-promoter` subagent and the
`promote-trs` orchestrator both execute this same recipe.

Boundary: this skill owns exactly one TR promotion attempt and, on success, one
focused commit. It does **not** own sweep ledgers, resume state, queue issues,
PRs, pushes, or merges; those belong to `promote-trs`.

## 0. Preconditions (decide promote-eligibility before running anything)

Read `metadata/trs/<tr>.yaml`. Bail early as HELD if:

- `support.recommended` is already `true` → `HELD <tr> — already recommended`.
- `support.implemented` is not `true` → `HELD <tr> — not implemented; needs ce-plan`.
- The TR has **no smoke target** in `references/smoke-map.md` (e.g. `revoke`) →
  `HELD <tr> — no smoke harness; route to ce-plan for a new smoke`. Do not
  fabricate evidence.

## 1. Run the smoke and capture the line

Resolve the smoke `make` target and any required env from `references/smoke-map.md`.
For `t8412` (chart), resolve a trading day: prefer today if it is a KST weekday;
the smoke validates weekday/not-future offline and the gateway returns `01715`
for a non-trading day — on `01715`, retry the prior weekday up to 3 times, else
`HELD <tr> — no reachable trading day`.

Run the target (it loads `.env` and hits the real **paper** gateway). Capture the
single `LIVE-SMOKE …` stdout line **verbatim** — this is the evidence.

## 2. Interpret the result (promote vs hold)

- **Smoke test failed / panicked** → HOLD. A failed account run emits a
  `SMOKE-FAIL` line (not `LIVE-SMOKE`) — `HELD <tr> — account-state gate
  (provisioning?)`. A gateway `01900` is paper-incompatible → `HELD <tr> —
  paper-incompatible (01900)` and consider `facets.paper_incompatible: true`.
- **Smoke passed, but `rsp_cd` is non-empty and not a success code** → investigate
  before trusting it. The success set is `00000`, empty, `00136`
  (조회가 완료되었습니다), `00707` (empty result set) — see
  `crates/ls-core/src/inner.rs::rsp_cd_is_success`. A success code other than
  `00000` is still a PROMOTE (note it in the evidence integrity comment, as
  `CSPAQ12200`'s `00136` does). Anything outside the set → `HELD <tr> —
  unexpected rsp_cd <code>`.
- **Smoke passed, success code, lifecycle/structural result** → PROMOTE (continue).

## 3. Secret-safety check (blocking)

The captured line is about to be **committed**. It MUST contain no OAuth token,
appkey, secret, or account number — only lengths, business `rsp_cd`, public
tickers/dates/ports, and structural counts. If `rsp_msg` or any account-identifying
text appears, STOP — the harness line needs a fix first (see the U1 pattern in
`crates/ls-sdk/tests/live_smoke.rs`); `HELD <tr> — smoke line not credential-free,
harness fix needed`. See `references/templates.md` for the exact safe shapes.

## 4. Write the evidence file

Create `metadata/evidence/<tr>.yaml` from the template in `references/templates.md`:
the secret-safety comment header, an integrity note explaining why the `rsp_cd`
proves a genuine round-trip, then `tr_code`, `date` (today, UTC), `env: paper`,
`target`, and the verbatim `line`. **`date` MUST equal the TR's
`maintenance.last_reviewed`** (the validator cross-checks this).

## 5. Flip the TR + write the recommendation block (the judgment step)

Edit `metadata/trs/<tr>.yaml`:
- `support.recommended: true`
- `maintenance.last_reviewed: <today>` (== evidence `date`)
- Add a `recommendation:` block: `behavior`, `evidence_ref: evidence/<tr>.yaml`,
  and `excludes`.

**Scope discipline (do not skip — this is the whole point of scoping):** the
`behavior` describes *exactly* what the smoke exercised (paper env, single
symbol/page/inquiry/lifecycle). For `excludes`, for every capability a reader
might naively assume from "recommended", ask *did this smoke actually prove it?*
— if not, exclude it explicitly. Always exclude production-credential variants
and overstated freshness automation. Per class, also exclude:
- market data (`t1101`/`t1102`): fields outside the modeled subset; correctness
  during halts/VI or outside KRX regular session.
- paginated (`t8412`): multi-page `chart_all` correctness beyond one page.
- account (`CSPAQ12200`): order/position-mutating state; balance-value correctness
  beyond a successful round-trip.
- realtime (`S3_`): **trade-data correctness, in-session row delivery, and
  reconnection** — a lifecycle smoke must never become a live-data recommendation.

See `references/templates.md` for worked `excludes` lists per class.

## 6. Update the docgen banner test

In `crates/ls-docgen/src/lib.rs`, function
`reference_covers_implemented_with_banner_and_omits_unimplemented`:
- remove `<tr>` from the `banner_trs` array,
- add `<tr>` to the recommended-no-banner `for rec in [...]` loop,
- update the count comment to match (the `reference.len()` assertion is unchanged
  — promoted TRs stay implemented).
If a per-TR dependency-facts test asserts `- Recommended: no` for this TR (only
`t8412` today, in `dependency_page_for_t8412_renders_all_metadata_facts`), flip
it to `yes`.

## 7. Bump the freshness count

In `metadata/EVIDENCE-FRESHNESS.md`, update the "With N Recommended TRs (…)
spanning M classes (…)" sentence: increment N, add `<tr>` to the list, and add the
TR's owner class to the class list if newly represented.

## 8. Regenerate docs and run the gate

```
make docs
cargo test                 # workspace
cargo test -p ls-core      # metadata re-validation + policy cross-check
make docs-check
```
If any gate is red: fix if it is a mechanical assertion you missed (banner list,
count comment, `Recommended:` flip); if the failure is substantive, `git checkout`
the TR's changes and `HELD <tr> — gate failed: <summary>`. Never leave the tree red.

## 9. Commit

Stage only this TR's files (evidence, TR yaml, docgen test, freshness doc, the four
regenerated docs for `<tr>` + the two index pages) and commit:

```
feat(metadata): promote <tr> to recommended with paper evidence
```
Body: the smoke target + captured result, the scope of the recommendation, and the
incremental count bump. Then emit the final line: `PROMOTED <tr> evidence/<tr>.yaml`.

## Reference

- `references/smoke-map.md` — TR → smoke target + gate notes.
- `references/templates.md` — evidence file + recommendation-block templates, with
  worked per-class `excludes` examples and the credential-free line shapes.
- Proven exemplars in-repo: `metadata/trs/t1101.yaml`, `metadata/evidence/t1101.yaml`,
  `metadata/evidence/token.yaml`.
