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
  complete; cleared by an operator through normal maintenance — KTD-5). This
  remains the recorded governance stance: the seed is carried **visibly
  provisional**, with re-attestation flowing through the D5 new-TR review loop
  (see below). Independent operator attestation is a deliberate de-scope for a
  solo-maintainer project.
- **Property-type mapping:** the property-type fetch returned HTTP 500 at fetch
  time; the fetch used the hardcoded fallback mapping and continued (a recoverable,
  warning-only path). Property-type *display names* on some fields therefore fell
  back to raw type codes until a future fetch resolved them; this does not affect
  the code-set, the completeness gate, or field identity.
  - **Correction (2026-06-22):** the "chronic `system-codes` HTTP 500" was a
    **misdiagnosis** — not an upstream outage but a wrong endpoint in
    `crates/ls-trackers/src/fetch.rs`. It called
    `/api/codes/public/system-codes?groupCode=property_type` (500s for everyone);
    the live portal endpoint is `GET /api/codes/public/property_type`. The response
    parser and the hardcoded fallback values were also wrong (e.g. `A0004` was
    `Decimal`, really `Number`; `A0005` was `Binary`, really `Object Array`), so the
    seed's field types were genuinely incorrect. Fixed 2026-06-22; the committed raw
    has since been re-pinned from a clean fetch via an attested type-only promote
    (`PROVISIONALITY-LEDGER.md` §4 — retired).
  - **Update (normalizer v2, R3):** a property-type mapping fallback is no longer
    silent at *check* time. The support-aware facts-outage gate now treats it as
    a whole-inventory degradation — `api-drift check` exits `2` whenever the
    fallback was served and any maintained TR is in the run, rather than letting
    fallback type codes diff as false `FieldChanged` findings. The seed itself was
    fetched under this fallback, so a fresh operator fetch should resolve the
    real type names before the seed's `token`/maintained shapes are trusted for
    type-level drift.

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

## Normalizer v2 correction (R-1 closed)

The committed shapes were re-seeded at `normalizer_version: 2` via the
network-free `api-drift renormalize` affordance, re-normalizing the same reviewed
raw evidence in `raw/ls-openapi-full.json`. v2 fixes the block-header rule so a
real field whose compact code equals its Korean label is no longer dropped: the
`token` `scope` field (request + response, length 256) now appears in the
Structural API Shape, and `token_type` is filed under `response_body` rather than
a phantom `scope` block. Only `manifest.json` (the version) and `token.json`
changed; the other six maintained shapes re-normalized byte-for-byte. The
self-diff (`api-drift check --staged`) still exits `0`.

## Re-attestation (KTD-5)

The code-set is re-attested **incrementally**: each future new-TR finding
(exit `1`) prompts an operator decision to admit the code; the reviewed commit
that updates `code-set.json` is the evidence trail. Clear `provisional` once an
operator has independently attested inventory completeness.

### Admission: t1101 (2026-06-17)

`t1101` (주식현재가호가조회, market_session / stock) was admitted as the maintained
SDK surface's first Stage-2 expansion TR. Its Structural API Shape was projected
network-free via `api-drift renormalize` from the committed
`raw/ls-openapi-full.json`; `maintained_tr_count` went 7 → 8 and only
`normalized/trs/t1101.json` was added (no other maintained shape changed). At
admission time `t1101`'s type-level names inherited the seed's property-type
fallback caveat; that caveat was **retired on 2026-06-22** when the wrong-endpoint
bug was fixed and the committed raw was re-pinned from a clean `property_type`
fetch (see the 2026-06-22 correction above and `PROVISIONALITY-LEDGER.md` §4). This
admission does not clear `provisional`.
