# Paper Live Smoke — convenience wrapper over the #[ignore] integration tests in
# crates/ls-sdk/tests/live_smoke.rs.
#
# Credentials load from the gitignored .env via `include` + `export` so they
# reach the cargo test subprocess. We deliberately do NOT use
# `export $(shell cat .env | xargs)` — that writes credentials through a shell
# expansion (log/echo and /proc exposure risk). The SDK itself never reads .env;
# this file is the only dotenv layer.
#
# Each target runs exactly one #[ignore] test by name and fails if zero tests
# ran (a filter typo must not read as green).

-include .env
export

.PHONY: live-smoke live-smoke-chart live-smoke-account live-smoke-ws

# $(1) = exact test name in crates/ls-sdk/tests/live_smoke.rs
define run_smoke
	@out=$$(cargo test -p ls-sdk --test live_smoke -- --ignored --exact --nocapture $(1) 2>&1); \
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
