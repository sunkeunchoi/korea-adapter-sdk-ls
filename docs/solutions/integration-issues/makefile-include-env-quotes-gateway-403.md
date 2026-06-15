---
title: "make `include .env` keeps quotes, sending quoted credentials and causing a misleading gateway 403"
date: 2026-06-15
category: integration-issues
module: Paper Live Smoke harness
problem_type: integration_issue
component: tooling
symptoms:
  - "`make live-smoke` fails with HTTP 403 from /oauth2/token while the same paper key works elsewhere"
  - "`OAuth token acquisition failed: Http(Status(403, None), url: \".../oauth2/token\")`"
  - "A raw curl POST to /oauth2/token with the same appkey/secret returns HTTP 200 + a token"
root_cause: config_error
resolution_type: config_change
severity: medium
related_components:
  - authentication
tags: [makefile, dotenv, oauth-403, credentials, ls-gateway, live-smoke, env-loading]
---

# make `include .env` keeps quotes, sending quoted credentials and causing a misleading gateway 403

## Problem

`make live-smoke` (the credential-gated live smoke over the LS paper gateway) failed with an HTTP 403 on OAuth token acquisition. The 403 looked like an invalid/expired paper credential, but the key was valid the whole time — the Makefile was corrupting the credential values while loading `.env`.

## Symptoms

- `make live-smoke` panics: `OAuth token acquisition failed: Http(Status(403, None), url: "https://openapi.ls-sec.co.kr:8080/oauth2/token")`.
- The same `LS_PAPER_APPKEY`/`LS_PAPER_SECRET` authenticate fine outside make.
- The SDK's `fetch_token` calls `resp.error_for_status()`, which discards the 403 response body, so LS's actual reason never surfaced — reinforcing the "bad credentials" misread.

## What Didn't Work

- **Assuming the credentials were wrong/unprovisioned.** The natural first read of a 403 at `/oauth2/token`. Wrong — the key was valid.
- **Re-checking the env selector.** A separate (real) issue surfaced first: the guard refused because `LS_TRADING_ENV` was unset (the hardened guard requires it explicitly set to `paper`). Adding `LS_TRADING_ENV=paper` got past the guard but still 403'd, proving the creds were reaching the gateway and being rejected.

## Solution

Diagnose by isolating each layer, then fix the loader:

1. **Raw `curl` to the endpoint** with the same fields returned HTTP 200 + token → the key and endpoint are fine.
2. **Run the SDK test with creds shell-sourced** (`set -a; . ./.env; set +a; cargo test ... --ignored`) → HTTP 200 → the SDK request is fine.
3. **Compare variable *lengths*** (never print secrets): shell-sourced `appkey` = 36 chars, but make-exported `appkey` = 38 chars. The extra 2 characters are the two literal `"` quote characters — the `.env` values were double-quoted.

The `.env` had `LS_PAPER_APPKEY="..."`. make's `include` keeps the surrounding quotes as part of the value; the shell strips them. So the SDK was sending `"appkey"` (quotes included) and LS returned 403.

Fix — source `.env` in the recipe shell instead of using make's `include`:

```makefile
# WRONG — make keeps the surrounding quotes literally:
-include .env
export

# RIGHT — the shell strips quotes and tolerates # / $ in values:
export   # still export command-line vars (e.g. LS_LIVE_SMOKE_*) to recipes
define run_smoke
	@set -a; [ -f .env ] && . ./.env; set +a; \
	out=$$(cargo test -p ls-sdk --test live_smoke -- --ignored --exact --nocapture $(1) 2>&1); \
	echo "$$out"; \
	echo "$$out" | grep -q "1 passed" || { echo "FAIL: $(1) ..."; exit 1; }
endef
```

Also avoid `export $(shell cat .env | xargs)` — it mangles values with spaces/special chars and risks echoing secrets.

## Why This Works

`make`'s `include` directive parses each `KEY=VALUE` line literally; a quoted value keeps its quotes as part of the string. POSIX `.` (source) applies shell word-splitting and quote removal, so `KEY="v"` yields `v`. Sourcing in the recipe shell (with `set -a` to auto-export) delivers the value to the `cargo`/test subprocess exactly as the SDK's `from_env` expects — no quote contamination, and `#`/`$` in values are not treated as make-special.

## Prevention

- **Never load credential `.env` files via make `include`.** Source them in the recipe shell (`set -a; . ./.env; set +a`).
- **Diagnose auth 403s by isolating layers before blaming the credential:** raw `curl` → SDK-with-shell-sourced-creds → compare value *lengths* (a +2 length delta is the classic quote-contamination signature). Compare lengths, never print secret values.
- **Surface the gateway's error body.** `reqwest`'s `error_for_status()` drops the response body. For auth endpoints, read the body on non-2xx and include it in the error so the real reason (`invalid_client`, quota, etc.) is visible. See `crates/ls-core/src/auth.rs` `fetch_token`.
- `.env.example` documents that surrounding quotes are optional (the shell strips them); both quoted and unquoted values now work.

## Related Issues

- `Makefile`, `crates/ls-sdk/tests/live_smoke.rs` (the harness), `crates/ls-core/src/auth.rs` (`fetch_token`).
- Context: LS serves paper and real REST from one host (`:8080`), distinguished only by credentials; the live-smoke guard requires an explicit `LS_TRADING_ENV=paper`. See `docs/plans/2026-06-15-002-feat-paper-live-smoke-plan.md`.
- Fix commit: `f7ea85c` on `feat/sdk-first-vertical-slice`.
