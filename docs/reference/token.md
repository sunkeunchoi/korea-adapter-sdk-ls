# SDK Reference: token

접근토큰 발급 (OAuth2 token issue)

> Generated from `ls-metadata` — do not edit by hand. Run `make docs` to regenerate.

- Owner class: `standalone`

## Recommendation

**Recommended behavior:** Paper OAuth access-token issuance

- Evidence: `evidence/token.yaml` (environment: `paper`)
- Freshness date: `2026-06-16` (`maintenance.last_reviewed`)
- Review by: `2026-09-14` (freshness date + 90-day backstop)
- What would revoke this claim: the **90-day backstop is enforced** — `make freshness-check` flags this TR's Focused Evidence as stale once 90 days elapse from the freshness date (the review-by date above), and the recommendation must then be re-attested. A maintained-TR Structural API Shape change that stales the evidence is **stated policy, not yet enforced by code** (change-driven invalidation is deferred). Description / `korean_name` changes are informational and do not stale it. See `metadata/EVIDENCE-FRESHNESS.md`.

This recommendation does not claim:

- Production-credential token issuance (evidence is paper only)
- Broader OAuth edge-case coverage (refresh, revoke-races, error taxonomy)
- Non-auth authorization or session semantics beyond token issuance
- Change-driven evidence invalidation (a Structural API Shape change auto-staling evidence) — stated policy, not yet enforced
