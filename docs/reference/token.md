# SDK Reference: token

접근토큰 발급 (OAuth2 token issue)

> Generated from `ls-metadata` — do not edit by hand. Run `make docs` to regenerate.

- Owner class: `standalone`

## Recommendation

**Recommended behavior:** Paper OAuth access-token issuance

- Evidence: `evidence/token.yaml` (environment: `paper`)
- Freshness date: `2026-06-16` (`maintenance.last_reviewed`)
- What would revoke this claim (stated policy — not enforced by code today): a maintained-TR Structural API Shape change stales the backing Focused Evidence, or the 90-day backstop elapses from the freshness date, whichever comes first. Description / `korean_name` changes are informational and do not stale it. See `metadata/EVIDENCE-FRESHNESS.md`.

This recommendation does not claim:

- Production-credential token issuance (evidence is paper only)
- Broader OAuth edge-case coverage (refresh, revoke-races, error taxonomy)
- Non-auth authorization or session semantics beyond token issuance
- Stronger freshness automation than the stated policy (no enforcement in code today)
