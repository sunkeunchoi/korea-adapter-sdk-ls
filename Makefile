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

.PHONY: live-smoke live-smoke-book live-smoke-chart live-smoke-account live-smoke-ws live-smoke-t8425 live-smoke-t8436 live-smoke-t1531 live-smoke-t1537 live-smoke-t1452 live-smoke-t1403 live-smoke-t1441 live-smoke-t1463 live-smoke-t1466 live-smoke-t1489 live-smoke-t1492 live-smoke-t1866 live-smoke-t1859 live-smoke-t1826 live-smoke-t1825 live-smoke-t9905 live-smoke-t9907 live-smoke-t8431 live-smoke-t9942 raw-probe

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

## Order-book smoke: paper guard -> OAuth token -> one t1101 호가 quote (no date
## needed). Must run during an open KRX regular session for live depth.
live-smoke-book:
	$(call run_smoke,live_smoke_book)

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

## t8425 (전체테마) smoke: paper guard -> OAuth token -> one all-themes read.
live-smoke-t8425:
	$(call run_smoke,live_smoke_t8425)

## t8436 (주식종목조회) smoke: paper guard -> OAuth token -> one stock-list read.
live-smoke-t8436:
	$(call run_smoke,live_smoke_t8436)

## t1531 (테마별종목) smoke: token -> t8425 theme -> one theme-constituents read.
live-smoke-t1531:
	$(call run_smoke,live_smoke_t1531)

## t1537 (테마종목별시세) smoke: token -> t8425 theme -> one per-stock-quotes read.
live-smoke-t1537:
	$(call run_smoke,live_smoke_t1537)

## t1452 (거래량상위) smoke: token -> one single-page top-volume read.
live-smoke-t1452:
	$(call run_smoke,live_smoke_t1452)

## Remaining single-page paginated rank/screen smokes (one post_paginated each).
live-smoke-t1403:
	$(call run_smoke,live_smoke_t1403)
live-smoke-t1441:
	$(call run_smoke,live_smoke_t1441)
live-smoke-t1463:
	$(call run_smoke,live_smoke_t1463)
live-smoke-t1466:
	$(call run_smoke,live_smoke_t1466)
live-smoke-t1489:
	$(call run_smoke,live_smoke_t1489)
live-smoke-t1492:
	$(call run_smoke,live_smoke_t1492)

## t1866 (서버저장조건 리스트조회) smoke: token -> server-saved condition list (the
## saved-condition spine producer). Requires LS_PAPER_USER_ID + a seeded condition.
live-smoke-t1866:
	$(call run_smoke,live_smoke_t1866)

## t1859 (서버저장조건 조건검색) smoke: token -> t1866 saved-condition list ->
## one condition search keyed by the first query_index (chained, self-sourcing).
## Requires LS_PAPER_USER_ID + a seeded condition (else SMOKE-FAIL).
live-smoke-t1859:
	$(call run_smoke,live_smoke_t1859)

## t1826 (종목Q클릭검색리스트조회) smoke: token -> one ThinQ Q-click search-list
## read for search_gb=0 (핵심검색). Wave 3 spine producer (yields search_cd keys).
live-smoke-t1826:
	$(call run_smoke,live_smoke_t1826)

## t1825 (종목Q클릭검색) smoke: token -> t1826 search-list -> one Q-click search
## keyed by the first search_cd (chained, self-sourcing; search_cd not recorded).
live-smoke-t1825:
	$(call run_smoke,live_smoke_t1825)

## Wave 1 ELW universe/list reads (no caller input; non-empty success -> flip).
live-smoke-t9905:
	$(call run_smoke,live_smoke_t9905)
live-smoke-t9907:
	$(call run_smoke,live_smoke_t9907)
live-smoke-t8431:
	$(call run_smoke,live_smoke_t8431)
live-smoke-t9942:
	$(call run_smoke,live_smoke_t9942)

## Failure classifier (implement-tr R6): one credential-safe raw-HTTP POST that
## bypasses the SDK's typed deserialize. Requires LS_PROBE_TR_CD, LS_PROBE_PATH,
## and LS_PROBE_BODY. Prints a RAW-PROBE line (never a LIVE-SMOKE evidence line).
##   make raw-probe LS_PROBE_TR_CD=t8425 LS_PROBE_PATH=/stock/sector \
##     LS_PROBE_BODY='{"t8425InBlock":{"dummy":""}}'
raw-probe:
	$(call run_smoke,raw_http_probe)

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

## Preview a Baseline Promotion of the latest staged run (writes nothing). Pins
## the run named by `latest.txt` — run `api-drift-fetch` first; it never live-
## fetches. Tiered exit mirrors `check`: 0 clean, 1 gated findings, 2 error.
api-drift-promote-dry-run:
	cargo run -q -p ls-trackers -- api-drift promote --dry-run

# ---------------------------------------------------------------------------
# Specification Document Tracker — opt-in, and deliberately EXCLUDED from default
# gates (`cargo test`/CI stays untouched, R10). Run by hand at a recurring
# operator checkpoint (see docs/MAINTENANCE_RUNBOOK.md). Unlike the API Drift
# targets, BOTH spec-doc targets are network-free: they reuse the shared raw
# snapshot the API Drift staging path already produced
# (baselines/api-drift/raw/), so they add no new fetch source (R1, R9). Findings
# are ADVISORY and never gate (KTD4) — `spec-doc-check` exits 0 unless a
# load/parse/version error occurs (exit 2). An example change becomes an SDK
# Maintenance Work Item only after human review (R8); the tracker never mutates
# code, docs, metadata, examples, or baselines.
.PHONY: spec-doc-check spec-doc-renormalize

## Compare staged example shapes against the committed example baseline; print
## advisory findings and the maintained-artifact review pointers (network-free).
spec-doc-check:
	cargo run -q -p ls-trackers -- spec-doc check

## Re-seed the committed example baseline from the shared committed raw evidence
## (network-free; no live fetch). Run after an EXAMPLE_NORMALIZER_VERSION bump,
## then review the normalized/examples.json diff.
spec-doc-renormalize:
	cargo run -q -p ls-trackers -- spec-doc renormalize

# --- Evidence-Freshness Evaluator -------------------------------------------
# `freshness-check` evaluates the 90-day evidence backstop over Recommended TRs:
# it flags any whose `maintenance.last_reviewed` is more than 90 days before today
# (UTC). Findings are ADVISORY and never gate — `Severity::Evidence` sits below
# `Maintenance`, so this always exits 0 on stale evidence; only a metadata
# load/parse error exits 2. The evaluator is operator-invoked, reads metadata,
# and mutates nothing. Re-attest a stale TR by rerunning its Paper Live Smoke,
# updating the evidence file + `last_reviewed`, and regenerating docs.
# Network-free; excluded from default `cargo test`/CI like the other checkpoints.
.PHONY: freshness-check

## Report Recommended TRs whose Focused Evidence is past the 90-day backstop
## (advisory; network-free; exits 0 even when stale).
freshness-check:
	cargo run -q -p ls-trackers -- freshness check

.PHONY: freshness-re-pin

## Re-pin a Recommended TR's attested shape to the current committed baseline —
## the R11 re-attestation interface (populate-if-absent; pass FORCE=1 to overwrite
## during a real re-attestation). Run AFTER refreshing the baseline (api-drift
## fetch/renormalize), never against a stale baseline.
##   make freshness-re-pin TR=token            # populate if absent
##   make freshness-re-pin TR=token FORCE=1     # overwrite during re-attestation
freshness-re-pin:
	@test -n "$(TR)" || { echo "usage: make freshness-re-pin TR=<tr_code> [FORCE=1]"; exit 2; }
	cargo run -q -p ls-trackers -- freshness re-pin $(TR) $(if $(FORCE),--force,)

# ---------------------------------------------------------------------------
# Manual maintenance sweep — aggregates the two checks that stay OPERATOR-RUN:
# `api-drift-check` (network-touching, R19 — no live fetch on a timer) and
# `spec-doc-check` (network-free, operator-run this increment by scope choice).
# Run by hand at a maintenance checkpoint (see docs/MAINTENANCE_RUNBOOK.md).
#
# `freshness-check` is DELIBERATELY EXCLUDED (R7): it has its own scheduled
# trigger (.github/workflows/freshness-cadence.yml), so bundling it here would
# duplicate the cadence and re-introduce the "forgot to run the sweep" gap the
# schedule exists to close. Run `make freshness-check` standalone for offline
# convenience.
#
# Exit code is the worst outcome of the two checks (0 clean, 1 a finding gated,
# 2 an error), so the operator sees a single clear pass/fail. Both checks run
# even if the first is non-zero — the sweep reports the whole picture, not just
# the first failure.
.PHONY: maintenance-sweep

## Run the operator-run checks (api-drift + spec-doc); exit reflects the worst.
maintenance-sweep:
	@echo "== maintenance sweep: operator-run checks (freshness runs on a schedule, not here) =="
	@rc=0; \
	echo "-- api-drift check --"; \
	$(MAKE) --no-print-directory api-drift-check || rc=$$?; \
	echo "-- spec-doc check --"; \
	$(MAKE) --no-print-directory spec-doc-check || { sc=$$?; [ $$sc -gt $$rc ] && rc=$$sc; }; \
	echo "== sweep exit: $$rc =="; \
	exit $$rc
