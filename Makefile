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
