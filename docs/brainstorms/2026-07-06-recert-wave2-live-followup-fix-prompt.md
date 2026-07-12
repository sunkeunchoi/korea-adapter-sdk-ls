# Hand-off prompt — Re-cert wave 2 live follow-up (fix the 3 §27 reopen reasons + promote t1102)

Paste this into a fresh session (`/goal` or `compound-engineering:ce-work`) to execute the
follow-up. It is self-contained; verify every claim against the code before acting.

---

## Context (what already shipped)

- Plan `docs/plans/2026-07-06-002-feat-recert-wave-reopen-held-trs-plan.md` — offline hardening
  **MERGED PR #99** (`dad2c2e`): `chegb="0"`→`"2"` revert + bounded ordno fill-check + owned-only
  teardown; the per-class `gateway_tolerant` facet (**preflight unchanged, KTD3**; probe downgrades
  `Divergent`→`expected-tolerant`, KTD4); CSPAQ12200/t0425 Account-bucket pacing.
- **Ledger §27 (MERGED PR #100, `d3f08f5`)** is the current disposition of the 7 reopened TRs
  after the attended 2026-07-06 live re-probe: **1 CLEAN (t1102), 6 HELD for three new reasons.**
  Read §27 in `metadata/PROVISIONALITY-LEDGER.md` first — it has the exact probe lines.

## Goal

Land one offline PR that fixes the three §27 reopen reasons + promotes t1102, so the operator's
next attended open-KRX session can certify the rest. Keep the tree green. **The promotions of the
order quartet / t8412 / t0425 / CSPAQ12200 are operator-gated live re-probes — this PR only makes
them certifiable; do NOT flip them offline.**

## Work items

### A. Order-quartet single-page guard bug (the §27 reason-C defect) — HIGHEST VALUE
`scan_symbol_working_orders` (in **both** `crates/ls-sdk/tests/negative_probe.rs` and the twin in
`crates/ls-sdk/tests/order_smoke.rs`) decides "single page?" from `resp.tr_cont()` — the HTTP
**header**. But t0425 self-paginates on the **`cts_ordno` body cursor** (`orders/mod.rs`: "Self-
paginates on the `cts_ordno` body cursor; the `tr_cont`/`tr_cont_key` header tokens ride
defensively"). The gateway returns `tr_cont="0"` on ANY non-empty page, so the guard
(`!empty && !"N"` = paginated) fail-closes the instant the book has even one row — which is why
every order leg HELD once the probe placed its own control.
- **Fix:** gate single-page-ness on the response **`cts_ordno` cursor** (empty / `" "` / `"0"` /
  all-default ⇒ terminal, single page), NOT the `tr_cont` header. Prefer reusing the existing
  `ls_core::HasPagination` / `impl_has_pagination!` continuation logic (that is how `collect_all`
  already decides "more pages") rather than a hand-rolled check. Verify which response field
  carries the next-page `cts_ordno` (see `T0425Response` in `crates/ls-sdk/src/orders/mod.rs`,
  ~line 783).
- Add an **offline twin**: a response with a terminal `cts_ordno` (empty/" "/"0") ⇒ single-page OK;
  a response with a real order-number cursor ⇒ paginated (fail-closed). Keep the fail-closed
  direction on ambiguity.

### B. t8412 probe throttle (the §27 reason-A defect)
`live_smoke_t8412_negative` runs its OWN standalone loop (it does not route through the paced
shared `run_inblock_negative_probe`), firing ~12 rapid market-data calls unpaced → every variant
returned `IGW00201`. Apply an inter-dispatch pace to that loop (mirror the U6 pattern already in
the shared loop). Market-data bucket is 10/s, so a smaller pace than the 1500ms account-lane one
suffices, but match the existing pattern for consistency. This is why t8412's "all Clean" was
false (a throttle classifies as a rejection ⇒ Clean).

### C. Mark the two new gateway-tolerant pairs (the §27 reason-B decision)
**Operator decision (recommended: YES, mark them — consistent with the `chegb`/`exchgubun`
precedent that the merged PR already set for gateway-defaulted filter fields kept stricter as a
caller contract):**
- `metadata/constraints/t0425.yaml` — add `gateway_tolerant: [required]` to **`medosu`**.
- `metadata/constraints/CSPAQ12200.yaml` — add `gateway_tolerant: [required]` to **`BalCreTp`**.
- Update each field/header comment (the §27 live evidence) and confirm the ls-core test
  `gateway_tolerant_classes_are_real_generatable_classes` still passes.
- If the operator decides these are NOT genuine caller contracts, set `required: false` instead
  (see the decision criterion in
  `docs/solutions/conventions/gateway-tolerant-facet-preserves-preflight-while-unblocking-differential-probe.md`).

### D. Promote t1102 (the one CLEAN cert) via the `promote-tr` recipe
`.agents/skills/promote-tr/SKILL.md`. t1102's differential was CLEAN/expected-tolerant this
session. Steps: author `metadata/evidence/t1102.yaml` (the evidence file already exists — update
`date`/`env`/`target`/`line`, keep/refresh `attested_shape`) + create
`metadata/error-coverage/t1102.yaml` + flip `metadata/trs/t1102.yaml` (`recommended: true`,
recommendation block **with the gateway-not-enforced scope exclude** for shcode/exchgubun,
`evidence_ref`, `error_coverage_ref`, `last_reviewed`) + bump the **five count sites**: the
`slice_metadata` tripwire (`{S3_,t1101,token}` → add `t1102`), `recommended_no_banner`, the
freshness-count assertion, `EVIDENCE-FRESHNESS.md`, and the docgen banner test — this is the first
flip **3→4**. **OPEN QUESTION:** `ClassCoverage.status` has no `tolerant` value — the tolerant
`(shcode,required)`/`(exchgubun,required)` pairs are neither `confirmed` nor `divergent`; decide a
status string (it is a free `String` in `ls-metadata`) and note it, e.g. `gateway_tolerant`.

### Operator safety (BEFORE any live order re-probe)
A **stranded band-floor 005930 buy** rests in the paper account (the first `cspat00601` run placed
it; teardown then hit the guard bug). It cannot fill but blocks every order-probe scan. **Cancel it
out-of-band in the LS paper UI first.**

## Gate (must be green)
`make docs && make docs-check`, `cargo test --workspace`, `cargo test -p ls-core`,
`cargo test -p ls-metadata`, `make lane-check`. Do NOT `cargo fmt` the whole `ls-trackers` crate.

## Live re-validation (operator-gated, attended open-KRX — a separate tail, ledger §28)
After the offline PR merges and the stranded order is cleared, re-run in-window:
`make live-smoke-{cspat00601,cspat00701,cspat00801}-negative` (guard fix), `-t8412-negative`
(pacing), `-t0425-negative` + `-cspaq12200-negative` (new marks). Promote whatever returns
CLEAN/expected-tolerant via `promote-tr`; record §28, fail-closed. Split-facet promotions MUST
carry the gateway-not-enforced exclude.
