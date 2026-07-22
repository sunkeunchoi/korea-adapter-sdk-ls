# Residual review findings — feat/cspat00601-booking-determining-skip

Source: `ce-code-review` run `20260722-104138-5e981472` (6 personas + learnings + Codex cross-model adversarial pass) on the branch's committed diff (base `e5b6a91`). All P0/P1 blocking issues were fixed and committed; the findings below harden the **`#[ignore]` operator-attended A/B harness** (`run_booking_determining_ab_probe`), which is part of the U4 operator tail and does not run offline or in CI. They were staged for a fix pass that was interrupted by a session limit; recorded here so they are not lost. All are in `crates/ls-sdk/tests/negative_probe.rs`.

## Fixed during review (not residual)
- **P0 — untracked error-coverage artifact.** `metadata/trs/CSPAT00601.yaml` declared `error_coverage_ref` but `metadata/error-coverage/CSPAT00601.yaml` was left untracked; a clean checkout would fail validation. Committed `68355e5`. (Found independently by agent-native, project-standards, correctness.)
- **Pre-existing P1 — stale freshness count.** `ls-trackers` `cli.rs` real-metadata freshness test asserted 8 Recommended TRs; CSPAT00701's promotion (§31/#151) took the real set to 9 on main. Not caused by this branch; surfaced by the full gate. Committed `8f0dda9`.

## Residual (harden the attended U4 harness before it runs)

1. **P1 — `rejected` verdict needs a submit-leg merits-reject allowlist.** *(in-process adversarial + Codex cross-model, agreed; the strongest signal in the run.)* `classify_booking_ab` reaches `Rejected` for any placed-nothing outcome, including a degraded/empty `rsp_cd` (HTTP 429 throttle, non-JSON body, or a reject for a reason unrelated to the injected omission). A `Rejected` verdict drives an R8/R11 annotation **lift**, so a false `Rejected` un-blinds a field that places real orders. Fix: require a parsed, non-empty `rsp_cd` in an explicit allowlist (at least `IGW40011` via `ls_core::is_ingress_validation_reject`, and `01407`); everything else → `Inconclusive`. Add classifier cases for 429+empty and unrelated-code rejects.

2. **P1 — a non-flat / filled final state must fail the attended test.** `plan_close_out` returns `None` when the fill delta is positive but `sellable_post` (mdposqt) is zero (same-day T+2-unsettled BUY fill), so the harness can finish with a real open paper position while the test returns `Ok(())` and only prints `flat=NOT-confirmed`. The Makefile greps `"1 passed"`, so green ships with an open position. Fix: panic (scrubbed, credential-free) on a non-confirmed-flat final read or an un-plannable required close-out.

3. **P2 — a `refused` gate outcome must not exit green.** `booking_ab_field_gate` refusal prints `verdict=refused` and returns from the unit-returning tokio test → success under `grep "1 passed"`; a mistyped `LS_AB_FIELD` looks like a passing run with no evidence. Fix: the live `run_*` wrapper panics on refusal (the pure `booking_ab_field_gate` keeps returning its Result for offline assertions).

4. **P1 — extract inline fill-detection into a pure tested fn.** *(testing + correctness.)* The three-way fill decision (`reads_trusted && (janqty delta || partial_fill || accepted&&child&&!resting)`) is inline and untested. Extract `detect_fill(...)` with a unit-test table (untrusted→false; delta alone→true; partial alone→true; accepted+child+not-resting→true; accepted+child+resting→false; accepted+no-child→false).

5. **P2 — extract the duplicated AB-probe setup helper.** *(maintainability.)* `run_igw00000_ab_probe` and `run_booking_determining_ab_probe` copy-paste the guard chain + SDK/band/token construction + pre-assert-flat scan. Extract one shared async setup helper; behavior-preserving.

## Also noted (advisory, no action required)
- `negative_probe.rs` is well over the file-size guideline (pre-existing); candidate to split generic helpers into `ls-sdk-test-support`.
- `LS_AB_FIELD` is generic; if a second order TR gets a booking A/B harness, disambiguate the env var name.
