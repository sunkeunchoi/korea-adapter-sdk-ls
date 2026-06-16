# Known Residuals — API Drift real-fetch slice

Accepted residual findings from the Tier 2 code review of the API Drift
real-fetch slice (plan `docs/plans/2026-06-16-002-feat-api-drift-real-fetch-plan.md`).
The high-confidence findings were fixed in commit `fix(ls-trackers): address
code-review findings`; the items below were consciously accepted for follow-up.

## R-1 — Block-header heuristic drops a real field whose code == Korean label (P2)

`ParsedProp::is_block_header` treats any body row where `propertyCd == propertyNm`
as a block delimiter and skips it as a field. Confirmed in real data: the `token`
TR's `scope` field (a genuine field, length 256) is dropped from the committed
shape in both request and response bodies.

- **Impact:** low — `scope` is a stable OAuth field; drift on it is unlikely, and
  the omission is consistent between baseline and staged runs, so it produces no
  *false* drift, only a coverage gap on that one field.
- **Fix shape:** require `length.is_none()` (block-header rows carry a null
  length; real fields carry a length) in `is_block_header`. This is a
  normalization-rule change → bump `NORMALIZER_VERSION` to 2 and re-seed the
  committed baseline (re-normalize from the committed raw evidence), an operator
  re-attestation per KTD-5. The version guard added in `run_check` already forces
  a re-baseline (exit 2) on a normalizer mismatch, so the change is safe to land
  deliberately rather than mid-slice.

## R-2 — Total facts outage stages an all-None run as complete (P2) — MITIGATED

`group_protocol` is best-effort (returns empty on failure, migration-source
parity). A *whole-inventory* outage would stage a run with all endpoint/rate
facts `None` and `fetch-report.ok = true`; the completeness gate (R12)
intentionally measures inventory *codes*, not facts, so it would not catch this.

- **Mitigated** (round 2): `fetch_and_stage` now counts groups with no protocol
  facts, records `facts_degraded_groups` in `fetch-report.json`, and warns on
  stderr at fetch time. The operator now has the signal at fetch/review time.
- **Still deferred:** the gate itself does not treat a wholesale facts outage as
  exit `2` — that remains a deliberate operator-review judgment, since a partial
  facts gap is normal and only a *total* outage is suspicious.

## R-3 — Duplicate menu group-id / same code across groups: last-wins shape (P3)

`normalize_run` keys shapes by TR code in a `BTreeMap`; if the same maintained
code appeared in two groups, the last-written shape wins (could flip
protocol/endpoint/rate facts on iteration order). `parse_menu` likewise does not
dedup `api_id`s.

- **Impact:** none today — the 7 maintained TRs each live in a single group.
- **Fix shape:** dedup `api_id`s in `parse_menu` and detect duplicate codes in
  `normalize_run` with a deterministic resolution + warning.

## R-4 — Serde forward-compatibility hardening (P3)

Persisted types could be hardened against future schema evolution:

- `DriftChange` (internally tagged) has no `#[serde(other)]` catch-all — a newer
  variant in a staged run would fail an older reader (`DriftFinding` is not
  persisted in the committed baseline today, so impact is nil now).
- Some persisted primitive fields (`Manifest.normalizer_version`,
  `BlockField.required`, `RawTr.is_websocket`, …) lack `#[serde(default)]`.
- `FetchReport.failure` stores the Rust `Debug` rendering of `GateOutcome` rather
  than a stable error code.
- `Protocol` is re-exported from `ls-metadata`, coupling committed-baseline
  serialization to that crate's evolution.

- **Impact:** none today — no pre-existing serialized files exist to break.
- **Fix shape:** add `#[serde(other)]`/`#[serde(default)]` and a stable
  `FetchReport.failure` code when forward-compat across binary versions becomes a
  real requirement (e.g. before the first normalizer bump).

## R-5 — Round-2 review nits accepted as-is (P3)

A second review round (reliability/testing/maintainability/project-standards
personas) confirmed the fixes above and surfaced minor items kept as-is:

- **`TrShape.protocol` + `is_websocket` redundancy** — both fields are in the
  plan's Structural API Shape schema, so the pair is spec-driven; `protocol` is
  derived from `is_websocket` at ingest. Kept per spec.
- **`stages::fetch()` not-implemented stub** — retained as PR #2 compatibility
  coverage; the `lib.rs` module doc now describes both layers accurately so the
  stub is no longer misleading.
- **Broad `lib.rs` re-exports / `RawTr` example fields** — the crate exposes its
  full surface for integration tests and operator inspection; not tightened.
- **Minor test-claim gaps** — the `gates_for` matrix omits the unreachable
  `Critical` rows (declared out of scope this slice); a bare-array
  `property_type_mapping` response path is covered only via the `{list:[]}` shape.

Round-2 fixes that WERE applied: stale `lib.rs` fetch-stubbed doc corrected;
retry no longer retries 4xx and caps the backoff shift; `RateLimitChanged`
relaxation + `EndpointChanged`/`ProtocolChanged` now tested; the duplicated
reorder/move reconcilers were consolidated into one `reconcile_pairs` helper.
