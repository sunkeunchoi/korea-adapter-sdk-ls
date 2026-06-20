# SDK Reference: token

접근토큰 발급 (OAuth2 token issue)

> Generated from `ls-metadata` — do not edit by hand. Run `make docs` to regenerate.

- Owner class: `standalone`

## Recommendation

**Recommended behavior:** Paper OAuth access-token issuance

- Evidence: `evidence/token.yaml` (environment: `paper`)
- Freshness date: `2026-06-16` (`maintenance.last_reviewed`)
- Review by: `2026-09-14` (freshness date + 90-day backstop)
- What would revoke this claim: the **90-day backstop is enforced** — `make freshness-check` flags this TR's Focused Evidence as stale once 90 days elapse from the freshness date (the review-by date above), and the recommendation must then be re-attested. **Change-driven staling is also enforced** — a qualifying Structural API Shape change (field add/remove/change or endpoint/protocol change) diverging from the attested shape stales the evidence (advisory, surfaced by the same check); only *auto-revoke* of the recommendation is deferred (a human re-attests or demotes). Description / `korean_name` / rate-limit / reorder changes are non-qualifying and do not stale it. See `metadata/EVIDENCE-FRESHNESS.md`.

This recommendation does not claim:

- Production-credential token issuance (evidence is paper only)
- Broader OAuth edge-case coverage (refresh, revoke-races, error taxonomy)
- Non-auth authorization or session semantics beyond token issuance
- Auto-revoking the recommendation on a change-driven structural change — change-driven *detection* ships (a qualifying Structural API Shape change stales the evidence, surfaced advisory by the freshness check); auto-revoke of support.recommended stays deferred
