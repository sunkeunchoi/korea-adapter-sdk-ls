# API Drift — first-baseline seed attestation

This records the review evidence for the **one-time provisional seed** of the
committed bounded baseline (U6, KTD-5). The reviewed commit that adds this
directory is the attestation trail; this file summarizes what was checked.

## Snapshot

- **Source:** live Rust-native fetch (`ls-trackers api-drift fetch --seed`) of
  `https://openapi.ls-sec.co.kr` (`/apiservice` menu + per-group TR / property /
  protocol endpoints).
- **Fetched at:** staged run `2026-06-16T01-46-25Z` (UTC).
- **Full inventory:** 365 distinct upstream TR codes across 41 API groups.
- **Maintained shapes normalized:** 7 (`token`, `revoke`, `t1102`, `t8412`,
  `CSPAQ12200`, `S3_`, `CSPAT00601`) — the TRs in `metadata/trs/`.
- **`code-set.json` `provisional`:** `true` (not yet independently attested as
  complete; cleared by an operator through normal maintenance — KTD-5).
- **Property-type mapping:** the LS `system-codes` endpoint returned HTTP 500 at
  fetch time; the fetch used the hardcoded fallback mapping and continued (a
  recoverable, warning-only path). Property-type *display names* on some fields
  therefore fall back to raw type codes until a future fetch resolves them; this
  does not affect the code-set, the completeness gate, or field identity.

## First-baseline parity check (computed, not copied)

A comparison code-set was **computed** by walking `categories → api_groups →
tr_list` in the migration source's `specs/ls_openapi_specs.json` (a parity
check, not a file copy):

| Set | Distinct TR codes |
|-----|-------------------|
| Migration source (computed) | 365 (41 groups) |
| Fresh Rust-native fetch | 365 |
| Intersection | 365 |
| In fresh, not source (newly appeared) | 0 |
| In source, not fresh (disappeared) | 0 |

The fresh Rust-native inventory is an exact match for the migration source's
computed inventory. The group set is non-empty (41).

## Round-trip verification

`api-drift check --staged <seed-run>` against the committed baseline exits `0`
with no drift findings (a self-diff; confirms storage + compare wiring, not
completeness). Coverage at seed time: 365 upstream, 7 metadata (6 implemented, 1
tracked-only), 0 metadata-missing-upstream, 358 upstream-missing-metadata.

## Re-attestation (KTD-5)

The code-set is re-attested **incrementally**: each future new-TR finding
(exit `1`) prompts an operator decision to admit the code; the reviewed commit
that updates `code-set.json` is the evidence trail. Clear `provisional` once an
operator has independently attested inventory completeness.
