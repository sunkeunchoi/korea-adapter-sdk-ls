# Paper Live Smoke — convenience wrapper over the #[ignore] integration tests in
# crates/ls-sdk/tests/live_smoke.rs.
#
# Credentials load from the gitignored .env by sourcing it in the recipe shell
# (`set -a; . ./.env; set +a`). We deliberately do NOT use make's `include .env`
# — make keeps surrounding quotes literally, so a quoted "appkey" reaches the SDK
# with the quote characters and the gateway rejects it (403). The shell strips
# quotes and tolerates # / $ in values. We also avoid `export $(shell cat .env |
# xargs)`, which mangles values and risks echoing them. The SDK itself never
# reads .env; this file is the only dotenv layer.
#
# Each target runs exactly one #[ignore] test by name and fails if zero tests
# ran (a filter typo must not read as green).

# Export command-line / make variables (e.g. LS_LIVE_SMOKE_*) to recipe shells.
export

.PHONY: live-smoke live-smoke-chart live-smoke-account live-smoke-ws

# $(1) = exact test name in crates/ls-sdk/tests/live_smoke.rs
define run_smoke
	@set -a; [ -f .env ] && . ./.env; set +a; \
	out=$$(cargo test -p ls-sdk --test live_smoke -- --ignored --exact --nocapture $(1) 2>&1); \
	echo "$$out"; \
	echo "$$out" | grep -q "1 passed" || { echo "FAIL: $(1) did not run (0 tests) or did not pass"; exit 1; }
endef

## Default smoke: paper guard -> OAuth token -> one t1102 quote (no date needed).
live-smoke:
	$(call run_smoke,live_smoke_default)

## Chart smoke: requires LS_LIVE_SMOKE_T8412_DATE=YYYYMMDD (a real trading day).
##   make live-smoke-chart LS_LIVE_SMOKE_T8412_DATE=20260612
live-smoke-chart:
	$(call run_smoke,live_smoke_chart)

## Account smoke: read-only CSPAQ12200 balance inquiry.
live-smoke-account:
	$(call run_smoke,live_smoke_account)

## WebSocket smoke: S3_ connect/subscribe/unsubscribe lifecycle (timeboxed).
live-smoke-ws:
	$(call run_smoke,live_smoke_ws)

# ---------------------------------------------------------------------------
# Docs generation — ls-docgen projects TR Dependency Docs and SDK Reference
# Docs from ls-metadata. These targets need no credentials, so (unlike the
# live-smoke recipes above) they do NOT source .env. If a future docs target
# ever needs credentials, source .env in the recipe shell (`set -a; . ./.env;
# set +a`) — never via make `include` (see the header note and
# docs/solutions/integration-issues/makefile-include-env-quotes-gateway-403.md).
.PHONY: docs docs-check

## Regenerate TR Dependency Docs and SDK Reference Docs from ls-metadata.
docs:
	cargo run -q -p ls-docgen

## Drift gate: fail (non-zero) if committed docs no longer match ls-metadata.
docs-check:
	cargo run -q -p ls-docgen -- --check

# ---------------------------------------------------------------------------
# API Drift Tracker — opt-in, and deliberately EXCLUDED from default gates
# (`cargo test`/CI stays network-free, R18). Run by hand at a recurring operator
# checkpoint (R19; see docs/MAINTENANCE_RUNBOOK.md). `api-drift-fetch` and
# `api-drift-check` hit the live LS Open API; `api-drift-renormalize` is
# network-free (it reads only the committed raw evidence). Exit contract for
# `api-drift-check`: 0 no gating drift, 1 a finding crossed the gate threshold
# (review needed), 2 fetch/parse/baseline error.
.PHONY: api-drift-fetch api-drift-check api-drift-promote-dry-run api-drift-renormalize

## Live-fetch the full LS inventory into a timestamped staged run + latest.txt.
api-drift-fetch:
	cargo run -q -p ls-trackers -- api-drift fetch

## Re-seed the committed baseline from its reviewed raw evidence (network-free;
## no live fetch). Run after a normalizer-version bump, then review the diff.
api-drift-renormalize:
	cargo run -q -p ls-trackers -- api-drift renormalize

## Fetch + compare against the committed bounded baseline; tiered exit (R17).
api-drift-check:
	cargo run -q -p ls-trackers -- api-drift check

## Report what a real promote would touch (writes nothing).
api-drift-promote-dry-run:
	cargo run -q -p ls-trackers -- api-drift promote --dry-run
