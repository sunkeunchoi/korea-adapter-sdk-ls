# Residual review findings — feat/cspat00601-booking-determining-skip

Source: `ce-code-review` run `20260722-104138-5e981472` (6 personas + learnings + Codex cross-model adversarial pass) on the branch's committed diff (base `e5b6a91`). All P0/P1 blocking issues were fixed and committed; the findings below harden the **`#[ignore]` operator-attended A/B harness** (`run_booking_determining_ab_probe`), which is part of the U4 operator tail and does not run offline or in CI. They were staged for a fix pass that was interrupted by a session limit; recorded here so they are not lost. All are in `crates/ls-sdk/tests/negative_probe.rs`.

## Fixed during review (not residual)
- **P0 — untracked error-coverage artifact.** `metadata/trs/CSPAT00601.yaml` declared `error_coverage_ref` but `metadata/error-coverage/CSPAT00601.yaml` was left untracked; a clean checkout would fail validation. Committed `68355e5`. (Found independently by agent-native, project-standards, correctness.)
- **Pre-existing P1 — stale freshness count.** `ls-trackers` `cli.rs` real-metadata freshness test asserted 8 Recommended TRs; CSPAT00701's promotion (§31/#151) took the real set to 9 on main. Not caused by this branch; surfaced by the full gate. Committed `8f0dda9`.

## Applied (post-review pass)

1. **DONE (`dd364b6`) — `rejected` verdict gated behind a submit-leg merits-reject allowlist.** *(adversarial + Codex cross-model, agreed; the strongest signal in the run.)* `is_booking_ab_merits_reject` (ingress-validation via `ls_core::is_ingress_validation_reject`, plus catalogued `01407`) now gates `BookingAbVerdict::Rejected`; a 429/empty/uncatalogued reject → `Inconclusive`, never lifts an annotation. False-lift regression cases added to `classify_booking_ab_covers_every_verdict_arm`.

2. **DONE (`f27c0c6`) — non-flat filled final state now fails the attended test.** A filled fire whose close-out leaves the position non-flat (NOT-confirmed / UNVERIFIED / no-closable-delta) records `filled_unflattened` and panics after teardown + diagnostics print, so `make ... | grep "1 passed"` can't ship over an open position.

3. **DONE (`f27c0c6`) — field-gate refusal now fails instead of exiting green.** A mistyped/unannotated `LS_AB_FIELD` panics (credential-free) rather than returning `Ok(())`. Pre-placement inconclusive aborts still return green no-op, unchanged.

4. **DONE (`cd6c24a`) — inline fill-detection extracted to a pure tested fn.** `detect_fill(...)` with a channel-coverage table (`detect_fill_covers_every_channel`).

## Residual (still open — behavior-preserving cleanup, no safety impact)

5. **P2 — extract the duplicated AB-probe setup helper.** *(maintainability.)* `run_igw00000_ab_probe` and `run_booking_determining_ab_probe` copy-paste the guard chain + SDK/band/token construction + pre-assert-flat scan. Extract one shared async setup helper; behavior-preserving. Deferred: pure dedup, touches the unrelated igw00000 probe, no correctness/safety bearing.

## Also noted (advisory, no action required)
- `negative_probe.rs` is well over the file-size guideline (pre-existing); candidate to split generic helpers into `ls-sdk-test-support`.
- `LS_AB_FIELD` is generic; if a second order TR gets a booking A/B harness, disambiguate the env var name.
