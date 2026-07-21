# KRX Calendar Traceability Matrix

The maintained record that maps every named calendar **fixture scenario** (S1–S12), every
`calendar status` **diagnostic outcome**, and every consumer **calendar policy branch** to the
assertion that owns it (issue #189 R2/R3; AE1, AE2). It is the reviewer's map *and* the input
to a machine-checkable drift test: `tests/traceability.rs` re-declares the same anchors and
fails when any no longer resolves (a renamed test, a deleted scenario marker, a removed render
token, a moved policy branch). Update **both** this file and `tests/traceability.rs` together —
that coupling is what AE1 requires the Foundation Gate to catch.

- **Anchor** — a `(file, needle)` pair the drift check requires to be present. Test-function
  anchors use `fn <name>`; render tokens use the quoted literal from `src/diagnostics.rs`;
  fixture scenarios anchor on their `// --- Scenario N` construction marker in
  `tests/fixtures.rs` **and** the representative civil date in `fixtures/base_2010_2012.json`.
- **Paths** — relative to this crate (`adapters/nautilus/nautilus-ls-calendar/`). The six
  consumer seams live one level up in the adapter workspace, so their anchors are `../`-prefixed.
- **Run** — `cargo test -p nautilus-ls-calendar --test traceability`, and under the assembled
  `make foundation-gate` (U4).

There are **no gap rows**: every branch below resolves to a real owning assertion. A branch
that lacked one would appear here as an explicit `GAP:` row, never a silent omission.

## Fixture scenarios (S1–S12)

Construction: `build_base_snapshot()` in `tests/fixtures.rs`. Scenario identity is carried by
`// --- Scenario N` comments (no enum), so each row anchors on its marker + representative date.

| ID | Represents | Date | Marker (`tests/fixtures.rs`) | Owning assertion |
|----|-----------|------|------------------------------|------------------|
| S1 | ordinary Trading Session | 2010-06-15 | `Scenario 1 + 7:` | `named_scenarios_resolve_to_their_expected_status` |
| S2 | weekend closure (rule) | 2010-06-19 | `Scenario 2:` | `named_scenarios_resolve_to_their_expected_status` |
| S3 | weekday election closure (holiday+rule) | 2010-06-02 | `Scenario 3:` | `named_scenarios_resolve_to_their_expected_status` |
| S4 | Labor Day closure (rule) | 2012-05-01 | `Scenario 4:` | `named_scenarios_resolve_to_their_expected_status` |
| S5 | Lunar New Year cluster | 2011-02-02 | `Scenario 5:` | `named_scenarios_resolve_to_their_expected_status` |
| S6 | cited first-party closure | 2011-09-21 | `Scenario 6:` | `named_scenarios_resolve_to_their_expected_status` |
| S7 | isolated Unknown weekday | 2010-06-16 | `Scenario 1 + 7:` | `named_scenarios_resolve_to_their_expected_status` |
| S8 | year-end closure (rule) | 2010-12-31 | `Scenario 8:` | `named_scenarios_resolve_to_their_expected_status` |
| S9 | first materialization boundary (New Year) | 2010-01-01 | `Scenario 9:` | `named_scenarios_resolve_to_their_expected_status`, `materialization_boundaries_are_first_and_last_rows` |
| S9b | last materialization boundary (year-end) | 2012-12-31 | `Scenario 9b:` | `named_scenarios_resolve_to_their_expected_status`, `materialization_boundaries_are_first_and_last_rows` |
| S10 | witness overrides inferred closure | 2011-06-15 | `Scenario 10:` | `alert_bearing_scenarios_carry_their_alerts` |
| S11 | first-party disagreement → Unknown | 2011-10-05 | `Scenario 11:` | `alert_bearing_scenarios_carry_their_alerts` |
| S12 | retrospective correction pair | 2012-03-14 | `Scenario 12:` | `alert_bearing_scenarios_carry_their_alerts` |

Whole-corpus assertions: `base_fixture_loads_through_the_real_loader_with_correct_identities`
(1096 rows, coverage 2010-01-01..=2012-12-31) and
`base_fixture_cannot_be_mistaken_for_a_real_krx_calendar` (synthetic scope, >95% Unknown).

## Diagnostic outcomes (ten `calendar status` outcomes)

Render tokens are defined by `DiagnosticOutcome::token()` / `LoadFailure::token()` in
`src/diagnostics.rs`; the human form renders `load:<x>` and the JSON form the nested
`{"load":"<x>"}`, both from the same literal. Tests live in `tests/diagnostics.rs`.

| Outcome | Token (`src/diagnostics.rs`) | Human + JSON owning assertion |
|---------|------------------------------|-------------------------------|
| healthy | `"healthy"` | `healthy_case_renders_stably` (asserts both `outcome: healthy` and `"outcome": "healthy"`) |
| stale | `"stale"` | `stale_case_is_stale_not_a_status_rewrite` |
| Unknown | `"unknown"` | `unknown_case_reports_unknown` |
| conflict | `"conflict"` | `conflict_case_reports_conflict_with_alerts` |
| out-of-range | `"out_of_range"` | `out_of_range_case_reports_out_of_range` |
| missing | `"missing"` | `load_failure_cases_each_map_to_a_typed_outcome` |
| corrupt | `"corrupt"` | `load_failure_cases_each_map_to_a_typed_outcome` |
| incompatible | `"incompatible"` | `load_failure_cases_each_map_to_a_typed_outcome`, `incompatible_schema_names_the_version_but_stays_typed` |
| unauthorized | `"unauthorized"` | `load_failure_cases_each_map_to_a_typed_outcome` |
| expired | `"expired"` | `load_failure_cases_each_map_to_a_typed_outcome` |

Cross-cutting redaction (no raw authority/credential string in either render form):
`view_diagnostic_never_leaks_the_authority_identity`,
`masked_fingerprint_is_deterministic_but_non_reversible`.

## Consumer policy branches (six boundaries)

Each consumer boundary advances Legacy → Shadow → Enforced. Rows map each
Trading/Closed/Unknown/unavailable branch to its owning assertion in the consumer's test file.

### Accumulate / probe — `../src/ingest/mod.rs` (`CalendarGate`)

Enforced-only after the U6 retirement (weekday primitive + Legacy/Shadow branches removed).

| Branch | Owning assertion (`../tests/ingest.rs`) |
|--------|------------------------------------------|
| Enforced Unknown → no request/advance | `enforced_unknown_target_makes_no_request_and_no_advance` |
| Enforced Trading → fetch | `enforced_trading_session_target_fetches` |
| Enforced Closed → advance without request | `enforced_closed_target_advances_without_request` |
| Enforced unavailable → stop + preserve state | `enforced_unavailable_calendar_stops_and_preserves_state` |
| Enforced probe Unknown anchor → no request | `enforced_probe_unknown_anchor_makes_no_request` |
| Enforced probe session anchor → probes | `enforced_probe_session_anchor_probes` |

### Checkpoint continuity — `../src/ingest/checkpoint.rs`

Enforced-only after the U6 retirement.

| Branch | Owning assertion (`../tests/ingest.rs`) |
|--------|------------------------------------------|
| Enforced merges all-closed gap | `enforced_merges_an_all_closed_gap` |
| Enforced Trading in gap prevents merge | `enforced_trading_session_in_the_gap_prevents_merge` |
| Enforced Unknown gap kept separate | `enforced_keeps_separate_across_an_unknown_gap` |

### Backward-widen — `../src/ingest/mod.rs`

Enforced-only after the U6 retirement.

| Branch | Owning assertion (`../tests/ingest.rs`) |
|--------|------------------------------------------|
| Enforced Trading → warn + persist | `enforced_backward_widen_trading_session_warns_and_persists` |
| Enforced all-closed → silent | `enforced_backward_widen_all_closed_region_is_silent` |
| Enforced Unknown → uncertain + re-evaluate | `enforced_backward_widen_unknown_region_is_uncertain_and_reevaluates` |

### Catalog readiness — `../lab/src/runner/research.rs`

Enforced-only after the U7 retirement (weekday walk-back + Legacy/Shadow branches removed).

| Branch | Owning assertion (`../lab/tests/research_cli.rs`) |
|--------|---------------------------------------------------|
| Enforced Closed boundary no false-flag | `enforced_closed_watermark_boundary_does_not_false_flag` |
| Enforced Unknown → NO-GO indeterminate | `enforced_boundary_relevant_unknown_is_a_no_go_indeterminate` |
| Enforced out-of-coverage → NO-GO unavailable | `enforced_out_of_coverage_watermark_is_a_no_go_unavailable` |
| Enforced stale → GO + prominent warning | `enforced_stale_but_established_is_a_go_with_a_prominent_warning` |

### Budget-probe automatic selection — `../src/bin/budget-probe.rs`

Enforced-only after the U8 retirement (weekday anchor + Legacy/Shadow arm removed).

| Branch | Owning assertion |
|--------|------------------|
| Enforced selects proven, skips closed/unknown | `enforced_selects_most_recent_proven_session_skipping_trailing_closed_unknown` (`../src/bin/budget-probe.rs`) |
| Enforced no session → no live call | `enforced_no_session_makes_no_live_call_until_explicit_range` (`../src/bin/budget-probe.rs`) |
| Enforced unavailable records condition | `enforced_unavailable_records_unavailable_condition_and_still_calls` (`../src/bin/budget-probe.rs`) |
| Explicit-range bypass unchanged (KTD8 recovery lever) | `enforced_bypass_audit_does_not_change_range_or_request` (`../src/bin/budget-probe.rs`) |
| Enforced no-session refuses before gateway | `enforced_no_session_refuses_before_any_gateway_call` (`../tests/budget_probe_composition.rs`) |

### Production Ladder date-fact gate — `../lab/src/runner/live.rs`, `../lab/src/dispatch/checks.rs`

Enforced-only after the U9 retirement (weekday `date_fact` + Legacy/Shadow arm removed; the
time-of-day window is preserved, KTD7).

| Branch | Owning assertion |
|--------|------------------|
| Enforced Trading from calendar | `u188_enforced_trading_session_from_calendar_not_weekday` (`../lab/src/runner/live.rs`) |
| Enforced Closed fails + records active | `u188_enforced_closed_from_calendar_fails_and_records_active` (`../lab/src/runner/live.rs`) |
| Enforced missing snapshot → fail-closed | `u188_enforced_missing_snapshot_is_unavailable_and_fail_closed` (`../lab/src/runner/live.rs`) |
| Enforced Unknown refuses / Trading greens | `u12_failure_inversion_unknown_refuses_but_trading_session_greens` (`../lab/tests/dispatch_checks.rs`) |
| Time-window half preserved (KTD7) | `u12_time_window_preserved_for_a_proven_session_and_an_overridden_unknown` (`../lab/tests/dispatch_checks.rs`) |
| Weekday time-window preserved (KTD7) | `weekday_time_window_is_preserved` (`../lab/tests/dispatch_checks.rs`) |

## Rollback rehearsal (U2, R4; AE3)

The first-class rollback operation over the atomic activation machinery and its offline
rehearsal — part of the Foundation Gate matrix so removing it makes the gate verifiably
incomplete.

| Anchor | Owning assertion |
|--------|------------------|
| `rollback` operation + `RollbackRecord` | `fn rollback`, `struct RollbackRecord` (`../src/calendar_refresh/activate.rs`) |
| Restores prior artifact + adoption identity | `rollback_restores_prior_artifact_and_adoption_identity` (`../tests/calendar_activate.rs`) |
| Preserves owner-only 0o600 | `rollback_preserves_owner_only_permissions` (`../tests/calendar_activate.rs`) |
| Refuses corrupt/unauthorized/expired prior | `rollback_of_an_unusable_prior_snapshot_is_refused` (`../tests/calendar_activate.rs`) |
| Refuses prior not covering `as_of` | `rollback_of_a_prior_snapshot_not_covering_as_of_is_refused` (`../tests/calendar_activate.rs`) |
| Refuses blank approval | `rollback_with_blank_approval_is_refused` (`../tests/calendar_activate.rs`) |

## Shadow-divergence classification (U3, R5/R6; AE4)

Each consumer emits a classified, redacted, non-persisted Shadow-divergence observation.

| Anchor | Owning assertion |
|--------|------------------|
| `DivergenceClass` + `classify_divergence` | `enum DivergenceClass`, `fn classify_divergence` (`../src/calendar.rs`) |
| Redacted, non-persisted, classified | `divergence_observation_is_redacted_and_classified` (`../src/calendar.rs`) |

_(Each consumer's Shadow divergence row was removed as that consumer reached its Enforced-only retirement (ingest U6, catalog U7, budget-probe U8, Ladder U9); the shared divergence machinery is retired in U10.)_
