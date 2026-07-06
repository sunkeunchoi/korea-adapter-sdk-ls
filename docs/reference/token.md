# SDK Reference: token

접근토큰 발급 (OAuth2 token issue)

> Generated from `ls-metadata` — do not edit by hand. Run `make docs` to regenerate.

- Owner class: `standalone`

## Recommendation

**Recommended behavior:** Paper OAuth2 access-token issuance

- Evidence: `evidence/token.yaml` (environment: `paper`)
- Freshness date: `2026-07-06` (`maintenance.last_reviewed`)
- Review by: `2026-10-04` (freshness date + 90-day backstop)
- What would revoke this claim: the **90-day backstop is enforced** — `make freshness-check` flags this TR's Focused Evidence as stale once 90 days elapse from the freshness date (the review-by date above), and the recommendation must then be re-attested. **Change-driven staling is also enforced** — a qualifying Structural API Shape change (field add/remove/change or endpoint/protocol change) diverging from the attested shape stales the evidence (advisory, surfaced by the same check); only *auto-revoke* of the recommendation is deferred (a human re-attests or demotes). Description / `korean_name` / rate-limit / reorder changes are non-qualifying and do not stale it. See `metadata/EVIDENCE-FRESHNESS.md`.

This recommendation does not claim:

- Production-credential token issuance (evidence is paper only)
- Token-lifecycle semantics beyond a single successful issuance (refresh, expiry, revocation)
- Auto-revoking the recommendation on a change-driven structural change — detection ships (advisory); auto-revoke of support.recommended stays deferred

## Errors & validation

Preflight validation runs before any network call; an invalid request is rejected locally (`LsError::Invalid`) with no HTTP call. Type and required-ness always enforce; a value-class bound (enum/range/format) is permissive until the differential probe confirms it, so a valid request is never falsely rejected.

**Request field rules:**

- `grant_type` — type `string`; required; one of [client_credentials] (permissive until confirmed)
- `appkey` — type `string`; required
- `appsecretkey` — type `string`; required
- `scope` — type `string`; required; one of [oob] (permissive until confirmed)

**Reachable gateway errors** (explained once from the shared catalog; environment/entitlement codes are not reproduced per TR):

- `00000` — Success. The gateway processed the request and returned valid data.
- `01900` — This service is not provided in the Paper (모의투자) environment (모의투자에서는 해당업무가 제공되지 않습니다). A per-service capability gate: no Paper account can clear it. Not a problem with your request. Retry against a Production credential set, or treat the TR as paper-incompatible.

