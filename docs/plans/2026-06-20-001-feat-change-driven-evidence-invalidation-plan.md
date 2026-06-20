---
title: "feat: Change-Driven Evidence Invalidation"
type: feat
date: 2026-06-20
deepened: 2026-06-20
origin: docs/brainstorms/2026-06-20-change-driven-evidence-invalidation-requirements.md
---

# feat: Change-Driven Evidence Invalidation

## Summary

Stale a Recommended TR's Focused Evidence when its committed structural baseline shape diverges from the shape the evidence was attested against, surfaced inside the existing network-free freshness check and rolling issue — advisory, non-gating, detection-only. The diff reuses the drift tracker's `diff_shapes` engine over a frozen attested `TrShape` stored on the evidence record; surfacing reuses the single `freshness check --json` contract and the rolling "Evidence freshness status" issue, distinguished from age-staleness by a `reasons` field. No metadata mutation, no `support.recommended` flip.

---

## Problem Frame

The 90-day backstop presumes Focused Evidence valid for 90 days from `maintenance.last_reviewed` absent any signal. But an upstream structural change — a field added, an endpoint moved, a protocol switch — invalidates what the evidence proved the moment it lands, not 90 days later. Today the drift tracker classifies structural changes and the freshness evaluator ages evidence, but the two never meet: `crates/ls-trackers/src/api_drift.rs` and `crates/ls-trackers/src/freshness.rs` run independently, so between operator-run drift checkpoints a Recommended TR can carry evidence the dashboard reports as valid while its real API shape has already drifted.

`metadata/EVIDENCE-FRESHNESS.md` already specifies the remedy as policy, and all six Recommended TRs carry an identical `recommendation.excludes` line naming change-driven invalidation as "stated policy, not yet enforced." This plan brings the *detection* half online: it flags drifted evidence inside the same monthly cadence that ships the age backstop. The "revokes the claim pending review" arm stays unenforced — a drifted TR keeps rendering as Recommended until a human re-attests or demotes it. That residual exposure is accepted and deferred (see origin: `docs/brainstorms/2026-06-20-change-driven-evidence-invalidation-requirements.md`).

---

## Requirements

Carried from the origin requirements doc. R-IDs preserved verbatim from origin for traceability.

**Detection**

- R1. Change-driven staling compares each Recommended TR's committed structural baseline shape against the structural shape its current Focused Evidence was attested against. The attested shape is an independent committed snapshot frozen at attestation time, not a re-read of the live baseline; a baseline update moves the baseline ahead of the frozen snapshot, creating divergence. The comparison is on representation-invariant structural content, so a global re-normalization that left a TR's shape unchanged must not register divergence for that TR.
- R2. The qualifying change set is exactly `FieldAdded`, `FieldRemoved`, `FieldChanged`, `EndpointChanged`, `ProtocolChanged`. `FieldReordered`, `FieldMovedAcrossBlock`, `RateLimitChanged`, `TrAdded`, `TrRemoved`, `DescriptionChanged`, and `FactsDegraded` never stale.
- R2a. A pure `NORMALIZER_VERSION`-driven representation shift must never qualify as staling. The attested shape carries the normalizer version it was captured under; a version mismatch against the baseline triggers re-attestation, not a stale-by-change finding.
- R3. Evaluation reads only committed artifacts — the baseline under `crates/ls-trackers/baselines/api-drift/normalized/` and the per-TR attested shape in metadata — and performs no network fetch.
- R4. The recorded attested shape must carry enough structural fidelity to classify a later diff as qualifying-or-not. `maintenance.source_spec_hash` (opaque) cannot serve; a new metadata/evidence-record field carries the structural snapshot at R2-classifying fidelity.
- R5. Selection is Recommended TRs only (`support.recommended == true`), matching the 90-day backstop evaluator.

**Surfacing**

- R6. Change-driven staleness surfaces through the same `freshness check --json` contract and the same rolling "Evidence freshness status" issue as the 90-day backstop, sharing the `Severity::Evidence` surface. Extending the pinned `--json` key set and the issue-table renderer is in-scope, in lockstep with the pin test.
- R7. Each stale entry carries a `reasons` field (an array) distinguishing `age` from `change`; an entry stale for both carries `["age","change"]`. A change entry also carries a short summary of what drifted. The dashboard renders the reasons — age-only entries show age, change-only entries show the drifted shape, both-reasons entries show both.
- R8. Change-driven staling is advisory: it emits `Severity::Evidence`, never mutates metadata, never flips `support.recommended`. Exit semantics unchanged — stale or fresh both exit 0.
- R9. The rolling issue's notify-on-transition behavior is unchanged: a newly stale TR joining the stale set posts a maintainer @mention; same-set re-runs stay silent.
- R9a. The freshness check surfaces baseline staleness: when the committed structural baseline is older than a threshold, the check emits an advisory warning. The age comes from a refresh date stamped into the committed baseline artifact at baseline-update time — not git commit date or filesystem mtime.

**Clearing / re-attestation**

- R10. Age-staleness and change-staleness clear independently. Refreshing `maintenance.last_reviewed` clears age-staleness but not change-staleness.
- R11. Clearing change-staleness requires re-pinning the TR's attested shape to the current committed baseline. The re-attestation flow must re-pin the attested shape, or the TR re-fires as stale-by-change on the next run.

---

## Key Technical Decisions

- KTD1. **Attested shape = full `TrShape` snapshot + its `normalizer_version`, stored on the evidence record; comparison filters `diff_shapes` output, never raw `TrShape` equality.** The evidence record carries the full structural `TrShape` captured at attestation plus the `normalizer_version` it was captured under (an opaque hash is ruled out by R4). Detection runs the drift tracker's existing `diff_shapes(attested, baseline)` and keeps only changes in the R2 allow-list. **It must not compare `TrShape` by derived `PartialEq`:** `BlockField` carries `field_index` and `description_hash`, so raw equality would mass-stale every TR on a global re-normalization that merely reordered fields or rehashed descriptions. Running `diff_shapes` and filtering is what resolves the origin's central design decision — the engine reconciles a reorder into `FieldReordered` (filtered out, never mis-read as remove+add) and emits `DescriptionChanged` separately (filtered out), so divergence registers only on genuine structural content change (R1/R2). The full `TrShape` is stored rather than a projection because `diff_shapes` consumes every `BlockField` field; a lossy projection would defeat the free reuse. Alternative (sidecar artifact) rejected — see Alternative Approaches.

- KTD2. **Relocate the structural-shape types (`TrShape`, `BlockField`, `Direction`) into `ls-metadata`; re-export from `ls-trackers`.** `EvidenceRecord` lives in `ls-metadata`, which cannot depend on `ls-trackers` (the dependency runs the other way), so the shape types move down for the evidence record and its validator to hold a typed attested shape. The justification is KTD1's: `diff_shapes` needs the full `BlockField`, so a projection would be lossy — full relocation is the only way to store the diffable shape in `ls-metadata`. (The `Protocol` re-export at `crates/ls-trackers/src/types.rs:369` is a *syntactic* precedent for relocate-and-re-export, not a semantic one — `Protocol` is metadata-owned and borrowed up, whereas `TrShape` is tracker-owned and pushed down; cite it only for the mechanism.) `diff_shapes` (made `pub(crate)`, not `pub`, to avoid widening the crate's public surface) and `DriftChange` stay in `ls-trackers`. After the move, `Direction` is referenced across the boundary by `DriftChange`/`SpecChange` (which stay up); record in the doc-comment that `Direction` is now metadata-owned shared field-traversal vocabulary. `FieldShape` does **not** move — it is not part of the `TrShape`/`BlockField` closure (it belongs to `NormalizedArtifact`/`Change`/`ExampleShape`, which stay in `ls-trackers`). This is the load-bearing structural change and the plan's central risk.

- KTD3. **`reasons` as an array per stale entry; `age_days` becomes `Option<i64>` with the key always present; change entries carry `change_summary`.** A TR stale for both age and change is one entry with `reasons: ["age","change"]` (AE6); partial clearing leaves `reasons: ["change"]` alone (AE7). This resolves the origin's "one combined value vs two flags" question in favor of a list — the only shape representing the both-case and partial-clear in a single entry. `age_days` serializes as `null` when `age` is absent (never `skip_serializing_if` — the pin test asserts the key is present, and the shell consumer must see a stable key set); `change_summary` is `null` when `change` is absent. A per-TR aggregator joins the age-finding stream and the change-finding stream by `tr_code` into one entry at report-assembly time (see KTD8).

- KTD4. **R2a handled by version-aware comparison, not a re-attest-all sweep.** When the attested `normalizer_version` differs from the baseline manifest's `normalizer_version`, change-detection is suppressed for that TR (no stale-by-change) and the check emits a re-attestation-needed advisory. This mirrors the cross-version guard at `crates/ls-trackers/src/cli.rs:448-453` and avoids a normalizer bump mass-staling every Recommended TR. **Version-mismatch suppression is detection-blind, not detection-deferred:** a genuine upstream change on a version-mismatched TR is invisible until re-attestation, so the advisory must be surfaced loudly and should age (KTD8 / Risks). **Load-bearing operator discipline:** `diff_shapes` reconciles index and description re-projection invisibly, but reconciles a *field-name or block-name* re-projection as raw `FieldRemoved`+`FieldAdded` (both qualifying) → false mass-stale. So any normalizer change that can alter field-name or block-name projection MUST bump `NORMALIZER_VERSION` (routing affected TRs to re-attestation via this version gate). A projection change shipped without a version bump defeats R1 invariance.

- KTD5. **Baseline-staleness age source = a `refreshed` date stamped into the committed manifest; check reuses `ls_metadata::evaluate`; threshold default 90 days.** The manifest gains a `refreshed: "YYYY-MM-DD"` field written by the two baseline-update paths (`write_normalized`, `renormalize_committed`). The staleness check calls the single-sourced `ls_metadata::evaluate(refreshed, as_of, 90)` (reusing its parse, its `>` boundary, and `FreshnessError`) rather than introducing new date arithmetic. A missing `refreshed` (cold-start, before the field exists) reads as *warn* — surfacing the never-stamped baseline R9a targets, never silent. Network-free and deterministic, satisfying R9a's rejection of git/mtime sources.

- KTD6. **Notify stays keyed by `tr_code`.** R9's "newly stale TR joining the stale set" is read as a set-membership transition keyed by TR code, matching the existing marker (`<!-- freshness-stale: <codes> -->`). A TR already in the set for age that also drifts does not re-join, so it does not re-notify. Reason-granular re-notify is deferred (see Scope Boundaries). The notify *comment wording* is reason-aware even though keying is not (the current text "past the 90-day backstop" is wrong for a change-only entry — U9 fixes it).

- KTD7. **Presence backstop in the validator; version-coupling in the freshness path; no value-equality invariant.** No value-level invariant exists — `attested_shape == baseline` is the *fresh* signal, so nothing can assert it. Split across two surfaces by crate reachability: the `ls-metadata` validator asserts a Recommended TR's evidence record carries `attested_shape` and `attested_normalizer_version` (catches "never captured"), staying intra-metadata; the version-coupling assertion — `attested_normalizer_version` equals the manifest version unless the TR is flagged for re-attestation — lives in the freshness path (U4), which already loads the manifest. Together they catch the false-fresh modes presence-only misses (re-pin against a stale baseline, normalizer bump without re-attestation). The "refreshed the date but forgot to re-pin against a *same-version* baseline" case remains caught by detection re-firing (AE4).

- KTD8. **The freshness evaluator becomes a baseline consumer via a two-evaluator split, merged by `tr_code`.** Today `evaluate_recommended` reads metadata only (`crates/ls-trackers/src/freshness.rs:158`); change-detection requires it to also read the committed baseline `NormalizedRun` (for the baseline `TrShape` and manifest `normalizer_version`) and the attested shapes. Keep `evaluate_recommended` as the age-only rule and add a separate change-drift evaluator, merging the two finding streams by `tr_code` at report-assembly time (a `BTreeMap<String, MergedEntry>` join, so implementers don't each invent an aggregation shape) — preserving the codebase's one-rule-per-function discipline rather than fusing age and change logic. The merge is **three-way**, not two: the existing `unparseable` channel (`freshness.rs:57`) collides on `tr_code`, so define the reconciliation — a TR that is both unparseable-date and stale-by-change surfaces its change entry (`reasons: ["change"]`, `age_days: null`) and remains in the loud `unparseable` error set, with notify keyed once by `tr_code` (KTD6). A baseline that is absent or unreadable at check time — whole run or a single Recommended TR's shape file — is a **loud error/warning, never silent fresh-by-change** (matching the `unparseable` precedent); a missing per-TR baseline emits a re-attestation advisory.

---

## High-Level Technical Design

Detection data flow per Recommended TR, inside the existing freshness check. All operands are committed artifacts; no network.

```mermaid
flowchart TB
  subgraph inputs[Committed inputs]
    EV[Evidence record:<br/>attested TrShape +<br/>attested normalizer_version]
    BL[Baseline normalized run:<br/>per-TR TrShape +<br/>manifest normalizer_version + refreshed date]
    META[TR metadata:<br/>support.recommended,<br/>last_reviewed]
  end
  META --> SEL{recommended?}
  SEL -->|no| SKIP[exempt]
  SEL -->|yes| VC{attested ver ==<br/>baseline ver?}
  EV --> VC
  BL --> VC
  VC -->|mismatch| READV[re-attest advisory<br/>no stale-by-change]
  VC -->|match| DIFF[diff_shapes attested vs baseline]
  DIFF --> FILT{any change in<br/>R2 allow-list?}
  FILT -->|no| FRESH[fresh-by-change]
  FILT -->|yes| STALE[stale-by-change · Severity::Evidence<br/>reasons += change · change_summary]
  BL --> BAGE{baseline refreshed<br/>older than threshold?}
  BAGE -->|yes| BWARN[baseline-stale warning · advisory]
  STALE --> REPORT[FreshnessReport]
  FRESH --> REPORT
  READV --> REPORT
  BWARN --> REPORT
  REPORT --> JSON[freshness check --json:<br/>stale[] reasons + change_summary,<br/>baseline_age fields] --> ISSUE[rolling issue table + @mention]
```

Age-staleness (the existing 90-day path) runs alongside and contributes `reasons += age` to the same per-TR entry; the two reasons combine in one entry and clear independently (R10).

---

## Implementation Units

### U1. Relocate structural-shape types into `ls-metadata`

**Goal:** Move `TrShape`, `BlockField`, and `Direction` from `ls-trackers` into `ls-metadata` so the evidence schema and its validator can hold a typed attested shape, with `ls-trackers` re-exporting them for source compatibility.

**Requirements:** Enables R4, R1 (foundational).

**Dependencies:** none.

**Files:**
- `crates/ls-metadata/src/` (new module, e.g. `shape.rs`, or extend an existing schema module) — define `TrShape`, `BlockField`, `Direction`.
- `crates/ls-metadata/src/lib.rs` — export the relocated types.
- `crates/ls-trackers/src/types.rs` — remove the local definitions; re-export from `ls_metadata` (mirror the existing `Protocol` re-export at `types.rs:369`).
- `crates/ls-trackers/src/api_drift.rs`, `crates/ls-trackers/src/cli.rs` — adjust imports if needed (should be transparent via re-export).

**Approach:** Pure move. `TrShape` already references `ls_metadata::Protocol`, so the types belong naturally in `ls-metadata`. `DriftChange`, `SpecChange`, `diff_shapes`, `change_severity`, and the drift pipeline stay in `ls-trackers` and consume the relocated `TrShape`/`Direction` through the dependency edge. `Direction` is also a field of `DriftChange`/`SpecChange` (which stay up), so after the move it is metadata-owned shared field-traversal vocabulary — record this in its doc-comment. `FieldShape` stays in `ls-trackers` — it is not referenced by `TrShape`/`BlockField` (it belongs to `NormalizedArtifact`/`Change`/`ExampleShape` on the stages/example diff path), so moving it would add an unrelated type to `ls-metadata`'s surface for no structural payoff. Preserve all derives (`Serialize, Deserialize, PartialEq, Eq, Clone`) and serde attributes (`rename_all`, `skip_serializing_if`, field order) byte-for-byte so committed baseline JSON still deserializes and re-serializes unchanged.

**Patterns to follow:** The `Protocol` relocate+re-export *mechanism* (`ls_metadata::Protocol` re-exported at `crates/ls-trackers/src/types.rs:369`) — cite for the syntactic move only, not as semantic justification (see KTD2).

**Test scenarios:**
- Covers the central wire-format risk. Every committed baseline `trs/*.json` deserializes into the relocated `TrShape` and re-serializes byte-identically (highest-value test — a silent serde drift here corrupts every committed baseline).
- Existing `ls-trackers` drift tests still pass unchanged (the re-export preserves `ls_trackers::TrShape`).
- Determinism round-trip preserved (mirrors `crates/ls-trackers/src/types.rs:830-832`).
- `ls-metadata` compiles and exposes `TrShape`/`BlockField`/`Direction` without referencing `ls-trackers`.

**Verification:** `cargo test` green across the workspace; `cargo tree` shows the `ls-trackers → ls-metadata` edge only one way; committed baselines deserialize unchanged.

### U2. Add attested-shape fields to the evidence record schema

**Goal:** `EvidenceRecord` carries the frozen attested shape and the normalizer version it was captured under.

**Requirements:** R1, R4, R11.

**Dependencies:** U1.

**Files:**
- `crates/ls-metadata/src/schema.rs` — add `attested_shape: Option<TrShape>` and `attested_normalizer_version: Option<u32>` to `EvidenceRecord` (`schema.rs:156-165`).
- `crates/ls-metadata/tests/slice_metadata.rs` — schema round-trip coverage.

**Approach:** Both fields `Option`, with `#[serde(default, skip_serializing_if = "Option::is_none")]` to match house style for new optional fields (`schema.rs:161`) and stay forward/backward compatible while the six records are backfilled (U8). The evidence record is the home because it is the artifact captured at attestation time and already drives `last_reviewed` consistency. **The stored attested `TrShape` is itself a frozen-format contract:** an attested shape captured under today's `TrShape` must still deserialize and diff as zero-divergence against a same-shape baseline after future `TrShape` evolution. Constrain `TrShape`/`BlockField` to only ever grow `skip_serializing_if` `Option` fields (a new non-`Option` field would change how an old stored shape deserializes and could read as spurious divergence); a non-additive change to `TrShape` is itself a re-attestation trigger.

**Patterns to follow:** Existing optional `EvidenceRecord` fields `target`/`line` (`schema.rs:161-164`); serde-default forward-compat convention.

**Test scenarios:**
- An evidence YAML without the new fields deserializes with `attested_shape: None` (backward compat).
- An evidence YAML with a full `attested_shape` + `attested_normalizer_version` round-trips and re-serializes equal.
- Extra/unknown fields remain ignored (existing behavior, `schema.rs:155`).

**Verification:** `cargo test -p ls-metadata` green; both new fields parse from authored YAML.

### U3. Stamp a refresh date into the committed baseline manifest

**Goal:** The committed manifest carries a deterministic, network-free baseline-refresh date for R9a.

**Requirements:** R9a.

**Dependencies:** none (parallel to U1/U2).

**Files:**
- `crates/ls-trackers/src/types.rs` (or wherever `Manifest` is defined, `types.rs:384`) — add `refreshed: String` (ISO date) to `Manifest`.
- `crates/ls-trackers/src/cli.rs` — `write_normalized` (`cli.rs:271-279`) and `renormalize_committed` (`cli.rs:534-572`) stamp the refresh date at baseline-update time.
- `crates/ls-trackers/baselines/api-drift/normalized/manifest.json` — regenerated with the new field.

**Approach:** The date is injected (an `as_of`/clock seam), never read from a wall clock inside the pure layer, matching the freshness evaluator's injected-`as_of` discipline (`crates/ls-trackers/src/freshness.rs`). Operator baseline-update paths pass today's date; tests pass a fixed date. `normalizer_version` already lives in the manifest (`manifest.json:48`) — no change there.

**Patterns to follow:** Injected-date determinism (`crates/ls-metadata/src/freshness.rs:89-101`); manifest write at `cli.rs:271-279`.

**Test scenarios:**
- `write_normalized` with an injected date produces a manifest whose `refreshed` equals that date.
- `renormalize_committed` updates `refreshed` to the injected date.
- Manifest round-trips with the new field; an old manifest without `refreshed` is handled (serde default or explicit migration of the committed file in this unit).

**Verification:** `cargo test -p ls-trackers` green; committed `manifest.json` carries `refreshed`.

### U4. Change-driven detection in the freshness evaluator

**Goal:** Per Recommended TR, version-check, diff attested vs baseline `TrShape`, filter to the R2 allow-list, and emit a `Severity::Evidence` stale-by-change finding with a drifted-shape summary — via a change-drift evaluator separate from the age evaluator, merged by `tr_code`.

**Requirements:** R1, R2, R2a, R3, R5, R8.

**Dependencies:** U1, U2, U3, U8. (U8 backfills the six evidence records before detection goes live; landing the plumbing inert first then enabling detection after backfill is the safe order — see Execution note.)

**Files:**
- `crates/ls-trackers/src/types.rs` — add `is_qualifying(&DriftChange) -> bool` (the R2 allow-list predicate) beside `DriftChange`/`gates_for`, the established home for one-copy classification rules.
- `crates/ls-trackers/src/api_drift.rs` — make `diff_shapes` `pub(crate)` (currently private at `api_drift.rs:632`).
- `crates/ls-trackers/src/freshness.rs` — keep `evaluate_recommended` as the age-only rule; add a separate change-drift evaluator taking the baseline `NormalizedRun` + evidence map; extend `FreshnessFinding` (`freshness.rs:32-38`) with `reasons` and `change_summary`; merge the two finding streams by `tr_code` at report assembly.
- `crates/ls-trackers/src/cli.rs` — `run_freshness_check` (`cli.rs:668-674`) also loads the baseline via `load_normalized` (`cli.rs:293-315`) and passes it plus the evidence map into the change-drift evaluator.

**Approach:** Two-evaluator split (KTD8): the age rule stays untouched; a new change-drift evaluator owns the comparison; a `tr_code` join produces one entry per TR carrying combined `reasons`. The R2 predicate `is_qualifying` maps exactly to the allow-list (`FieldAdded | FieldRemoved | FieldChanged | EndpointChanged | ProtocolChanged`). Per recommended TR: if `attested_shape` is `None`, no stale-by-change (the U7 validator backstops absence). If the baseline shape for that TR is absent/unreadable, emit a re-attestation advisory — never silent fresh-by-change (KTD8). If `attested_normalizer_version != manifest.normalizer_version`, suppress detection and record a re-attestation advisory (R2a / KTD4). Otherwise run `diff_shapes(attested, baseline)`, keep `is_qualifying` changes, and if any survive set `reasons += change` and a `change_summary` rendered from the surviving `DriftChange`s. Severity is `Severity::Evidence` set directly (pattern at `freshness.rs:169-174`); `Evidence` never gates (`types.rs:292-294`), so exit stays 0 (R8). The change evaluator deliberately ignores `change_severity` — it reports *evidence staleness*, not *API breakage*, so the same change that classifies `Breaking` in `api-drift compare` surfaces as `Evidence` here (two-lens intent; pin with a test).

**Execution note:** Land in two safe phases — first the plumbing (extend `FreshnessFinding`, `pub(crate) diff_shapes`, `is_qualifying`, thread baseline+evidence into the evaluator) inert while `attested_shape` is `None`; then U8 backfill. Activation is **automatic**, not a feature flag: once the six records carry `attested_shape` (U8's all-or-nothing commit), the detection path fires because the shape is no longer `None`. Recommended merge order: U1 → U2 → U3 → U8 → U4 → U5 → U6 → U7 → U9 → U10.

**Technical design (directional, not implementation spec):**
```
for tr in recommended(trs):
    entry = stale_entry_for(tr)        # one entry per tr, reasons accumulate
    if age_stale(tr, as_of): entry.reasons += "age"; entry.age_days = ...
    ev = evidence[tr]
    if ev.attested_shape is None: continue            # U7 validator catches this
    if ev.attested_normalizer_version != manifest.normalizer_version:
        report.readvise += tr; continue              # R2a: no stale-by-change
    changes = diff_shapes(ev.attested_shape, baseline.shapes[tr])
    qualifying = [c for c in changes if is_qualifying(c)]
    if qualifying:
        entry.reasons += "change"
        entry.change_summary = summarize(qualifying)
```

**Patterns to follow:** Recommended selection + `Severity::Evidence` emission (`crates/ls-trackers/src/freshness.rs:163-174`); `diff_shapes` reconciliation semantics (`api_drift.rs:632-700`); two-evaluator separation mirrors the age-rule/`gates_for` single-responsibility split.

**Test scenarios:**
- Covers AE1. A Recommended TR whose baseline has a `FieldAdded` vs its attested shape → entry has `reasons` containing `change` and a non-empty `change_summary`.
- `FieldRemoved`, `FieldChanged`, `EndpointChanged`, `ProtocolChanged` each independently mark stale-by-change.
- Covers AE2. Divergence only `DescriptionChanged` → not stale-by-change.
- Covers AE3. Divergence only `RateLimitChanged` → not stale-by-change.
- `FieldReordered` and `FieldMovedAcrossBlock` → not stale-by-change (reconciled by `diff_shapes`, then filtered out).
- R1 representation-invariance: re-normalizing the baseline under the same version with no upstream change for a TR yields zero qualifying changes (fresh-by-change) — confirms detection rides on `diff_shapes`+filter, not raw `TrShape` equality over `field_index`/`description_hash`.
- R2a (dual outcome): `attested_normalizer_version` != manifest version → no stale-by-change finding AND the TR appears in the re-attestation advisory set (this is the freshness-path version-coupling backstop from KTD7).
- Name-reprojection guard: a same-version baseline whose only difference is a field-name/block-name re-projection (no upstream change) — assert the intended behavior (it surfaces as stale-by-change, making the missing `NORMALIZER_VERSION` bump a loud, caught discipline failure per KTD4, rather than a silent mass-stale that trains maintainers to ignore the signal).
- Two-lens severity: a change that classifies `Breaking` in `api-drift compare` surfaces as `Severity::Evidence` in the freshness path.
- Baseline-absent: a Recommended TR with a missing/unreadable per-TR baseline shape → re-attestation advisory, never silent fresh-by-change.
- `attested_shape: None` → no stale-by-change finding emitted for that TR (no panic).
- Exit code is 0 whether or not any TR is stale-by-change (R8).
- A TR stale for both age and change → one merged entry with `reasons: ["age","change"]`, `age_days` set, `change_summary` set.
- Three-way merge: a Recommended TR that is both unparseable-date and stale-by-change → a change entry (`reasons: ["change"]`, `age_days: null`) AND membership in the loud `unparseable` set, notified once by `tr_code`.
- Inherited-semantics swap case: a field that changes type AND shifts index with no net add/remove (two fields swap positions, one also changes type) — assert the intended outcome, documenting whether `diff_shapes`' reorder reconciliation absorbs the attribute change (under-stale) or the change-drift evaluator detects the attribute change independent of index.

**Verification:** `cargo test -p ls-trackers` green; a hand-constructed drifted fixture produces exactly one stale-by-change entry with the expected summary; the version-mismatch fixture produces an advisory and no stale-by-change.

### U5. Baseline-staleness warning

**Goal:** Emit an advisory warning when the committed baseline's `refreshed` date is older than the threshold.

**Requirements:** R9a.

**Dependencies:** U3.

**Files:**
- `crates/ls-trackers/src/freshness.rs` — add `BASELINE_STALE_DAYS` constant (default 90); call `ls_metadata::evaluate(refreshed, as_of, BASELINE_STALE_DAYS)` and branch on `FreshnessState::Stale`; thread the result into `FreshnessReport`.

**Approach:** Reuse the single-sourced date rule — no new date arithmetic. `ls_metadata::evaluate` (`crates/ls-metadata/src/freshness.rs:89-101`) already provides the parse, the `>` boundary (proven by its `boundary_exactly_ninety_is_fresh` test), and `FreshnessError::{UnparseableDate, OutOfRange}`. A missing `refreshed` field (cold-start, before U3's field exists on the committed manifest) reads as *warn*, surfacing the never-stamped baseline R9a targets — never silent. Advisory only; does not change exit (R8/R9a).

**Patterns to follow:** Reuse of the pure rule, mirroring how `ls-docgen` consumes `ls_metadata::evaluate`/`review_by` rather than re-deriving (`crates/ls-metadata/src/freshness.rs:89-106`).

**Test scenarios:**
- `refreshed` exactly `BASELINE_STALE_DAYS` ago → not stale (boundary `>`, inherited from `evaluate`).
- `refreshed` one day past threshold → stale, age reported.
- Missing `refreshed` field → warns (never-stamped baseline), not a crash, not silent-fresh.
- Unparseable `refreshed` → surfaced as a warning (via `FreshnessError`), not a crash.
- Exit code unaffected by baseline staleness.

**Verification:** `cargo test -p ls-trackers` green; injected old date produces the warning, fresh date does not.

### U6. Extend the `freshness check --json` contract and its pin test

**Goal:** The JSON contract carries `reasons`, `change_summary`, nullable `age_days`, and top-level baseline-staleness fields; the pin test updates in lockstep.

**Requirements:** R6, R7.

**Dependencies:** U4, U5.

**Files:**
- `crates/ls-trackers/src/freshness.rs` — extend `FreshnessFindingJson` (`freshness.rs:97-105`) and `FreshnessReportJson` (`freshness.rs:109-117`); update `report_to_json` (`freshness.rs:125-144`); update the pin test `json_field_names_are_pinned` (`freshness.rs:344-371`).

**Approach:** Per-entry keys become `{tr_code, last_reviewed, reasons, age_days, change_summary, severity}` — `age_days` is `Option<i64>` serializing as `null` (key always present, never `skip_serializing_if`, so the `jq` consumer sees a stable key set), `change_summary` likewise nullable (KTD3). Top-level gains baseline-staleness keys (e.g. `baseline_refreshed`, `baseline_age_days`, `baseline_stale`) and the re-attestation-advisory list (KTD4/KTD8). The pin test's expected top-level and per-entry key sets are updated to the exact new sets — this is the load-bearing workflow-contract change, a contract version bump, updated deliberately not loosened. The merged-entry shape comes from the `tr_code` aggregator (KTD8); the DTO renders it.

**Patterns to follow:** The DTO-not-derive contract and pin-test discipline (`crates/ls-trackers/src/freshness.rs:82-117, 344-371`).

**Test scenarios:**
- Pin test asserts the exact new top-level key set and the exact new per-entry key set.
- A change-only entry serializes with `age_days: null` and a non-null `change_summary`.
- An age-only entry serializes with a non-null `age_days` and `change_summary: null` and `reasons: ["age"]`.
- A both-reasons entry serializes with both non-null and `reasons: ["age","change"]`.
- Baseline-stale run includes the top-level baseline fields with expected values.

**Verification:** `cargo test -p ls-trackers` green including the updated pin test; `freshness check --json` output matches the pinned shape.

### U7. Attested-shape presence validator

**Goal:** A located validation error fires when a Recommended TR's evidence record lacks the attested shape or its normalizer version. (The version-coupling assertion lives in U4's freshness path, not here — see KTD7.)

**Requirements:** R11; origin consistency-model dependency.

**Dependencies:** U2, U8. (U8 backfills the six records first; landing U7's hard error before backfill would red-fail `validate_dir` on every Recommended TR — `validate_dir` is called by the evaluator's own tests and across docgen/CLI.)

**Files:**
- `crates/ls-metadata/src/validator.rs` — add `ValidationError::AttestedShapeMissing { tr_code, .. }` (+ `Display` arm, `validator.rs:27-167`); extend `check_recommendation` (`validator.rs:274-331`) alongside the existing `EvidenceDateMismatch` check.

**Approach:** Inside `check_recommendation` — which already resolves the `EvidenceRecord` for a recommended TR (`validator.rs:320-328`) — push `AttestedShapeMissing` when `attested_shape` or `attested_normalizer_version` is `None` (catches "never captured"). This stays intra-metadata: `validate_dir(metadata_root)` reads only the metadata tree and is shared by `ls-docgen` and the planner tests, so it must not learn the `ls-trackers` baseline path. The version-coupling check (attested version vs manifest version) therefore lives in U4's freshness path, which already loads the manifest (KTD7). Located, never anonymous (house style).

**Patterns to follow:** `EvidenceDateMismatch` check shape and located-error convention (`crates/ls-metadata/src/validator.rs:320-327`); validator registration in `validate_dir` (`validator.rs:341-410`); the existing `EvidenceDateMismatch` test asserting `tr_code == "token"` (`validator.rs:790`).

**Test scenarios:**
- A recommended TR whose evidence record lacks `attested_shape` → `AttestedShapeMissing` naming the TR.
- A recommended TR whose evidence record lacks `attested_normalizer_version` → error.
- A fully-populated recommended TR (post-U8 backfill) → no error.
- A non-recommended TR with no attested shape → no error (validator scoped to recommended).

**Verification:** `cargo test -p ls-metadata` green; tempdir fixtures (`validator.rs:471-478, 711-794`) exercise missing / clean cases.

### U8. Re-pin mechanism and backfill the six Recommended TRs

**Goal:** A re-pin mechanism that captures the current baseline shape into an evidence record, used to backfill the six Recommended TRs so detection starts from a clean baseline — before detection (U4) and the hard validator (U7) go live.

**Requirements:** R10, R11.

**Dependencies:** U2, U3. (Needs only the schema field and the baseline source; it does NOT need U4's evaluator. U4 and U7 depend on this unit, not the reverse.)

**Files:**
- `crates/ls-trackers/src/cli.rs` — a re-pin path that reads the current baseline `TrShape` + manifest `normalizer_version` and writes them into the named TR's evidence record; extend `parse_freshness` (`cli.rs:146-159`, currently only handles `check`) with a `re-pin <tr>` subcommand arm. This re-pin is the **permanent R11 re-attestation interface**, not only a one-time backfill vehicle — every future re-attestation calls it.
- `metadata/evidence/{token,t1101,t1102,t8412,S3_,CSPAQ12200}.yaml` — backfilled with `attested_shape` + `attested_normalizer_version: 2`.
- `Makefile` — optional convenience target mirroring existing operator targets (`Makefile:78-92`).

**Approach:** Re-pin reads the committed baseline (`load_normalized`, `cli.rs:293-315`) for the TR, serializes its `TrShape` into the evidence YAML field, and stamps the manifest's current `normalizer_version`. **Populate-if-absent, never overwrite-to-current:** an unconditional re-pin re-run would silently clear a genuine, intentionally-standing stale-by-change signal (mechanized hand-edit) — so re-pin refuses to overwrite an existing `attested_shape` unless explicitly forced during a real re-attestation. **Backfill against a freshly-fetched baseline:** perform the backfill immediately after an operator api-drift fetch in the same session, not merely when the R9a check reads green — the `refreshed` stamp is self-attested and can read fresh while content is stale (Risks), so launching the six pinned to a stale-but-recently-stamped baseline would bake in the silent-green this feature removes. Backfill the six recommended codes as a **single all-or-nothing commit** (a half-applied backfill reaching `main` with detection live leaves the unbackfilled TRs in the baseline-absent/None path). To zero the window where `validate_dir` would pass records lacking `attested_shape`, land U2 + U3 + U8 as one grouped/stacked merge unit before U7. Because attested == baseline at backfill, all six start fresh-by-change.

**Patterns to follow:** `load_normalized` baseline read (`cli.rs:293-315`); the six recommended codes hardcoded in `freshness.rs` tests (`freshness.rs:237-253`).

**Test scenarios:**
- Re-pin on a TR writes an `attested_shape` equal to the current baseline `TrShape` and the manifest's `normalizer_version`.
- Re-pin is populate-if-absent: re-running against a TR that already has an `attested_shape` is a no-op (does not overwrite), unless forced.
- After backfill, the freshness check reports all six recommended TRs fresh-by-change.
- Re-pinning a TR whose baseline has drifted (forced, during re-attestation) clears its stale-by-change on the next check (AE5 mechanics).
- `Test expectation:` the six backfilled YAMLs pass the U7 validator.

**Verification:** Backfilled metadata validates; `freshness check --json` shows zero stale-by-change entries immediately after backfill.

### U9. Rolling-issue renderer and notify, with stubbed live-path tests

**Goal:** The issue table renders reason + drifted-shape summary; steady-state stays exit-0 and silent; new behavior is covered by a stubbed-`gh` live-path test.

**Requirements:** R6, R7, R9.

**Dependencies:** U6.

**Files:**
- `.github/scripts/update-freshness-issue.sh` — extend `render_body` (`update-freshness-issue.sh:126-146`) to render a reason column and, for change entries, the drifted-shape summary; make `render_notify_comment` (`:152-157`) reason-aware (the current "past the 90-day backstop" wording is wrong for a change-only entry); keep `decide_action` notify-on-transition (`:99-123`) keyed by `tr_code` (KTD6).
- `.github/scripts/tests/update-freshness-issue.test.sh` — add stubbed-`gh` live-path coverage.

**Approach:** The table render switches on `reasons`: an age row shows `age (days)`; a change row shows the drifted shape; a both-reasons row shows both. The jq extraction (`update-freshness-issue.sh:138-140`) widens to read `reasons`/`change_summary`/nullable `age_days` — handle `age_days: null` explicitly so a change-only entry renders rather than dropping. The notify comment text reflects the reason set (age vs change vs both) instead of hardcoding the backstop phrasing. Per the recent cadence learning (`docs/solutions/workflow-issues/shell-script-live-path-needs-stubbed-binary-tests.md`): add at least one stubbed-`gh` test (fake `gh` first on `PATH`, logging args + emitting canned JSON), assert both exit code and call log; reuse the existing explicit-`if` (not `[ ] && cmd`) and subshell-stdout patterns rather than reinventing them; audit any new branch's last statement and any new `< <(fn)`.

**Patterns to follow:** Sourced-helper unit-test pattern (`.github/scripts/tests/update-freshness-issue.test.sh:15`); the source guard (`update-freshness-issue.sh:280`); marker keying (`:56-57, 144`); the established subshell-stdout and explicit-conditional guards already in the script.

**Test scenarios:**
- Covers AE1 (surfacing). A change-stale TR renders a table row showing the drifted shape and posts exactly one `@mention` when newly stale (stubbed `gh`, asserting call log + exit 0).
- Live-path render of a mixed age/change body proves `gh issue edit --body-file` emits the drifted-shape summary, not just the dry-run decision.
- Steady-state (nothing stale) exits 0 and issues no `gh issue comment`.
- An age-only stale set still renders the age column unchanged (no regression).
- A both-reasons TR renders both age and drifted-shape in one row.
- Reason changes but the TR stays in the stale set (`{age,change}` → `{change}`): confirm the intended behavior (silent `edit`, no re-notify per KTD6) and that the notify comment wording is reason-correct.
- `age_days: null` change-only entry renders (not dropped) in both dry-run and live paths.
- Baseline-stale warning (from U6 top-level fields) renders an advisory line in the body.
- `set -e` safety: the new reason branch's final statement does not trip the failure path on a healthy run.

**Verification:** `bash .github/scripts/tests/update-freshness-issue.test.sh` passes, including the new live-path test; a `--dry-run` render shows the reason column.

### U10. Update policy docs and flip the excludes clause

**Goal:** `EVIDENCE-FRESHNESS.md`, the runbook, and the six TRs' `recommendation.excludes` reflect that change-driven *detection* now ships (revoke arm still deferred).

**Requirements:** R6, R8; origin Scope Boundaries.

**Dependencies:** U4, U6, U9 (flip only once detection is live end-to-end).

**Files:**
- `metadata/EVIDENCE-FRESHNESS.md` — document the change-driven detection arm as enforced; describe the `reasons` surface and independent clearing.
- `docs/MAINTENANCE_RUNBOOK.md` — document re-pin on re-attestation via the `freshness re-pin` command (R11), the baseline-staleness warning (R9a), the fetch-immediately-before-backfill/re-pin discipline, and the normalizer-bump re-attest path (R2a/KTD4) including a **fixed window within which all six Recommended TRs must be re-attested after a `NORMALIZER_VERSION` bump** (bounds the detection-blind window — see Risks).
- `metadata/trs/{token,t1101,t1102,t8412,S3_,CSPAQ12200}.yaml` — update the "stated policy, not yet enforced" `recommendation.excludes` line to reflect detection-shipped / revoke-deferred.

**Approach:** The excludes clause currently reads "...stated policy, not yet enforced" on all six (`metadata/trs/token.yaml:28-32`). Reword to: detection enforced via the freshness check; auto-revoke deliberately deferred. Keep the line present (the deferral is still a real exclusion). Wording consistent across all six.

**Patterns to follow:** The identical excludes clause across the six recommended TRs; the operator-run vs scheduled split documented in `docs/MAINTENANCE_RUNBOOK.md`.

**Test scenarios:**
- `Test expectation: none` — documentation and metadata-prose changes. The six edited TR YAMLs still validate (`cargo test -p ls-metadata`), which is covered by existing validation tests rather than new ones.

**Verification:** `cargo test` green (metadata still validates); docs describe the shipped detection arm and the deferred revoke arm.

---

## Output Structure

No new directory hierarchy — all units modify existing files or add evidence-record fields. The per-unit `**Files:**` lists are authoritative.

---

## Acceptance Examples

Carried from origin; each maps to test scenarios in the named units.

- AE1. Qualifying field change stales. Baseline has a `FieldAdded` vs attested → `stale[]` entry with `reasons` containing `change`, a drifted-shape summary, and a maintainer @mention if newly stale. (U4, U9; covers R1, R2, R6, R7, R9.)
- AE2. Informational-only change does not stale. Divergence only `DescriptionChanged` → not stale-by-change. (U4; covers R2.)
- AE3. Rate-limit change does not stale. Divergence only `RateLimitChanged` → not stale-by-change. (U4; covers R2.)
- AE4. Touching the date alone does not clear change-staleness. Refreshing `last_reviewed` without re-pinning → fresh-by-age but still stale-by-change. (U4, U8; covers R10, R11.)
- AE5. Full re-attestation clears it. Re-pin attested shape + refresh evidence/`last_reviewed` → next check reports fresh, rolling issue closes all-clear. (U8, U9; covers R6, R10, R11.)
- AE6. Stale for both reasons must clear both. Past 90 days and structurally drifted → entry reflects both reasons; clears only once date refreshed and shape re-pinned. (U4, U6; covers R7, R10, R11.)
- AE7. Partial clearing leaves the other reason standing. Refresh `last_reviewed` but not the shape → next check reports `reasons: ["change"]` only. (U4, U6; covers R7, R10.)

---

## Scope Boundaries

**Deferred for later**

- Auto-revoking `support.recommended` on a detected structural change (the "revoke the claim" reading) — revisit once the advisory flag has run against real drift events.
- Scheduling the `spec-doc` check — a separate, independent follow-up.
- Reason-granular re-notify (KTD6): a TR already stale-by-age that also drifts does not currently post a fresh @mention. Reconsider if real usage shows the missing ping matters.
- Cross-crate baseline-consistency validation (KTD7): the validator checks attested-shape presence only, not that the attested shape matches the baseline at attestation time.

**Outside this increment**

- The api-drift network fetch and baseline-refresh stay operator-run; this increment only reads the already-committed baseline. R19's network scope is unchanged.
- Rename fingerprinting / adjacency computation plays no part here.

---

## Alternative Approaches Considered

- **Tracker-owned sidecar artifact instead of an evidence-record field.** Store the attested shape under `crates/ls-trackers/baselines/api-drift/attested/<tr>.json`, avoiding the type relocation (U1). Rejected: R4 calls for a metadata/evidence-record field, and the validator backstop (R11) wants to live in `ls-metadata` beside `EvidenceDateMismatch`. A sidecar splits ownership away from the evidence schema and pushes consistency-checking into the wrong crate.
- **Opaque structural hash instead of the full `TrShape`.** Rejected by R4 directly — a hash that cannot distinguish a field change from a description change can't classify a diff as qualifying-or-not.
- **Purpose-built name-keyed projection + a new comparator.** Define a minimal `AttestedShape` in `ls-metadata` (field names, endpoint, protocol — position and description dropped) and write a small comparator so R2 semantics emerge by construction. Rejected: it reintroduces a second structural-comparison path against house single-sourcing, and the reorder/move reconciliation in `diff_shapes` is non-trivial to reimplement correctly. Reusing `diff_shapes` + filter is the lower-risk single-source path; `diff_shapes` consumes the full `BlockField`, so the projection couldn't be lossy anyway.
- **Re-attest-all sweep on a normalizer bump instead of version-aware comparison.** Rejected by KTD4 — mass-staling every Recommended TR on a representation-only shift trains maintainers to ignore the signal.

---

## Open Questions

**Deferred to implementation**

- (Resolved during review — recorded here for traceability.) The U7 validator is **presence-only**; the version-coupling check lives in the **freshness path** (U4), not the `ls-metadata` validator. `validate_dir(metadata_root)` reads only the metadata tree and is also called by `ls-docgen` and the metadata planner tests; threading the `ls-trackers`-owned manifest version into it would force a three-call-site signature change and a layering inversion (the lower crate learning a path the upper crate owns). The freshness check already loads the manifest, so the version-coupling assertion is reachable there. Both placements satisfy KTD7's intent; the freshness-path placement avoids the cross-crate churn.
- The exact `change_summary` rendering from a `Vec<DriftChange>` (how much per-field detail to include in the one-line summary) — settle against the issue-table width during U6/U9.
- (Resolved during review) `FieldShape` stays in `ls-trackers` — it is not part of the `TrShape`/`BlockField` closure, so only `TrShape`/`BlockField`/`Direction` relocate in U1.

---

## Risks & Dependencies

- **Diff-operand precision (P1, the central correctness point).** Detection must run `diff_shapes` and filter to the R2 allow-list, never compare `TrShape` by derived `PartialEq` — `BlockField` carries `field_index` and `description_hash`, so raw equality mass-stales every TR (false-stale) on a global re-normalization that merely reordered fields or rehashed descriptions. Mitigation: KTD1 pins the diff-and-filter approach; the R1 representation-invariance test in U4 guards it.
- **Re-pin skew → false-fresh (P1).** A re-pin taken against a baseline that is itself stale produces `attested == baseline`, zero divergence, indistinguishable from a correct re-pin — the false-fresh dual of baseline liveness. Mitigation: re-pin is populate-if-absent and gated on the R9a check being green at backfill time (U8); the U7 version-coupling validator catches the normalizer-bump variant; ordering re-pin strictly after the operator's baseline fetch stays a runbook discipline (U10).
- **Missing/unreadable per-TR baseline (P1).** A deleted, truncated, or unparseable `normalized/trs/<tr>.json` must surface as a re-attestation advisory, never default to silent fresh-by-change — the per-TR analogue of R9a. Mitigation: KTD8 / U4 baseline-absent handling.
- **Presence-only validation is insufficient.** No value-equality invariant exists (equality is the fresh signal), so the validator adds baseline-identity coupling (version match unless flagged for re-attestation, U7/KTD7) to catch re-pin skew and hand-edited records. Residual: a hand-edit that fabricates a same-version equal shape is still only caught by the next genuine re-normalization.
- **`refreshed` stamp is self-attested.** R9a's liveness signal is only as honest as the manually-stamped date — a forgotten stamp false-warns (low harm), a forged/stale stamp reads fresh while content is stale (silent-green, the failure R9a exists to remove). Mitigation candidate (deferred): couple `refreshed` to a baseline content-hash so a stamp that advances without content change is flagged. Stated here as a known residual, not solved.
- **Advisories don't escalate; the version-transition window is detection-blind (P1).** `Severity::Evidence` findings and the re-attestation advisory are non-gating and non-aging, so a chronically-ignored change-stale or version-mismatched TR (the latter is detection-*blind*, KTD4) carries no increasing pressure — and a normalizer bump is exactly when a genuine upstream change can ride in unseen. This is the feature's sharpest residual: it leaves open, during version transitions, the silent-green window the feature exists to close. Bounded operationally in U10 — the runbook requires re-attesting all six Recommended TRs within a fixed window of any `NORMALIZER_VERSION` bump, so the blind window is closed by discipline rather than left open-ended. Stronger mitigation (deferred): age the advisory by carrying a first-seen date into the rolling issue so chronic ignores worsen visibly. Overlaps the origin's deferred dead-man's-switch.
- **Type relocation blast radius (U1).** Moving the shape types into `ls-metadata` touches every `ls-trackers` import site and reshapes the crate's field-vocabulary boundary (`Direction`/`FieldShape`). Mitigation: re-export keeps `ls_trackers::TrShape` working; the byte-identical baseline round-trip test (U1) guards serde stability; a doc-comment records `Direction` as now-shared metadata vocabulary.
- **Baseline liveness (load-bearing, from origin).** Detection is only as current as the committed baseline; if the operator never refreshes it, an upstream change is never introduced as divergence. R9a/U5 make a stale baseline *visible* but do not solve the liveness dependency — refreshing the baseline remains the operator's job (`docs/MAINTENANCE_RUNBOOK.md`).
- **Normalizer-version bump (R2a/KTD4).** A `NORMALIZER_VERSION` bump re-normalizes baseline shapes; version-aware comparison suppresses false mass-staling and routes affected TRs to a re-attestation advisory. Without the version stamp, a bump would mass-stale every Recommended TR and train maintainers to ignore the signal.
- **Pinned `--json` contract (R6/U6).** `reasons`/`change_summary`/`Option<i64>` `age_days` and baseline fields are a workflow-contract version bump; the pin test and every consumer (the shell renderer, which must handle `age_days: null`) move in lockstep (U6 ↔ U9). The shell change carries the live-path testing risk documented in `docs/solutions/workflow-issues/shell-script-live-path-needs-stubbed-binary-tests.md`.
- **Secret safety at the contract boundary.** Per `docs/solutions/architecture-patterns/change-tracker-baseline-clean-self-diff.md`, the attested shape and any `--json`/issue-rendered structure must use the structural `TrShape` type (which discards scalar sample values), never a raw `serde_json::Value` — LS example payloads carry real-looking secrets. Storing `TrShape` (KTD1) satisfies this by construction.

---

## Sources / Research

- `crates/ls-trackers/src/types.rs` — `Severity` (`:131-139`, Evidence < Maintenance), `gates_for` (`:292-294`, Evidence never gates), `DriftChange` variants (`:446-514`), `TrShape`/`BlockField` (`:318-365`), `Protocol` re-export precedent (`:369`), `Manifest.normalizer_version` (`:384`).
- `crates/ls-trackers/src/freshness.rs` — live freshness evaluator, recommended selection (`:163-167`), `FreshnessFinding` (`:32-38`), `--json` DTO (`:82-117`), `report_to_json` (`:125-144`), pin test `json_field_names_are_pinned` (`:344-371`).
- `crates/ls-metadata/src/freshness.rs` — pure 90-day rule, `evaluate` boundary semantics (`:89-101`), injected `as_of`.
- `crates/ls-trackers/src/api_drift.rs` — `diff_shapes` (`:632-673`, private — needs `pub(crate)`), `change_severity` contextual (`:558-600`), `NORMALIZER_VERSION` (`:33`).
- `crates/ls-trackers/src/cli.rs` — `load_normalized` (`:293-315`), `run_freshness_check` (`:668-674`), cross-version guard precedent (`:448-453`), baseline write paths `write_normalized` (`:271-279`) / `renormalize_committed` (`:534-572`).
- `crates/ls-metadata/src/schema.rs` — `EvidenceRecord` (`:156-165`), `Maintenance.source_spec_hash` ruled out by R4 (`:123-127`), `Support` (`:115-120`).
- `crates/ls-metadata/src/validator.rs` — `EvidenceDateMismatch` + `check_recommendation` (`:274-331`), located-error convention (`:83-167`), `validate_dir` (`:341-410`), tempdir test fixtures (`:471-478, 711-794`).
- `crates/ls-trackers/baselines/api-drift/normalized/manifest.json` — `normalizer_version: 2` (`:48`), no refresh date today (R9a gap).
- `metadata/trs/token.yaml` — the "stated policy, not yet enforced" excludes clause (`:28-32`), shared across all six recommended TRs.
- `.github/scripts/update-freshness-issue.sh` — `render_body` (`:126-146`), `decide_action` (`:99-123`), `compute_stale_set` tr_code keying (`:72-75`), marker (`:56-57, 144`), source guard (`:280`).
- `docs/solutions/workflow-issues/shell-script-live-path-needs-stubbed-binary-tests.md` — stubbed-`gh` live-path testing discipline; subshell and `set -e` traps.
- `docs/solutions/architecture-patterns/change-tracker-baseline-clean-self-diff.md` — determinism via value-discarding; secret-safe structural types; normalizer-bump re-baseline coupling.
- `metadata/EVIDENCE-FRESHNESS.md`, `docs/MAINTENANCE_RUNBOOK.md` — the combine policy and operator-run vs scheduled split.
