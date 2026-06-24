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

.PHONY: live-smoke live-smoke-book live-smoke-chart live-smoke-account live-smoke-ws live-smoke-t8425 live-smoke-t8436 live-smoke-t1531 live-smoke-t1537 live-smoke-t1452 live-smoke-t1403 live-smoke-t1441 live-smoke-t1463 live-smoke-t1466 live-smoke-t1489 live-smoke-t1492 live-smoke-t1481 live-smoke-t1482 live-smoke-t1866 live-smoke-t1859 live-smoke-t1826 live-smoke-t1825 live-smoke-t9905 live-smoke-t9907 live-smoke-t8431 live-smoke-t9942 live-smoke-t1958 live-smoke-t1964 live-smoke-t1601 live-smoke-t1615 live-smoke-t1640 live-smoke-t1662 live-smoke-t1664 live-smoke-t3341 live-smoke-t8424 live-smoke-t1511 live-smoke-t1485 live-smoke-t1516 live-smoke-t1514 live-smoke-cspaq12300 live-smoke-cspaq22200 live-smoke-cfobq10500 live-smoke-ccenq90200 live-smoke-cfoaq10100 live-smoke-ccenq10100 live-smoke-t2301 live-smoke-t2522 live-smoke-t8401 live-smoke-t8426 live-smoke-t8433 live-smoke-t8435 live-smoke-t8467 live-smoke-t9943 live-smoke-t9944 live-smoke-t2111 live-smoke-t2112 live-smoke-t2106 live-smoke-t8402 live-smoke-t8403 live-smoke-t8434 raw-probe

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

## Account BEP smoke: read-only CSPAQ12300 BEP/balance inquiry.
live-smoke-cspaq12300:
	$(call run_smoke,live_smoke_cspaq12300)

## Account orderable smoke: read-only CSPAQ22200 orderable-amount/valuation inquiry.
live-smoke-cspaq22200:
	$(call run_smoke,live_smoke_cspaq22200)

## F/O account deposit smoke: read-only CFOBQ10500 deposit/margin inquiry (may
## return an empty 00707 on a position-less paper account → PENDING).
live-smoke-cfobq10500:
	$(call run_smoke,live_smoke_cfobq10500)

## KRX night-derivatives balance smoke: read-only CCENQ90200 (krx_extended; an
## empty/off-window result → PENDING, the regular clock does not apply).
live-smoke-ccenq90200:
	$(call run_smoke,live_smoke_ccenq90200)

## F/O orderable-quantity smoke: read-only CFOAQ10100 inquiry (NOT an order);
## set LS_LIVE_SMOKE_FNOISU to a current KOSPI200-futures code.
live-smoke-cfoaq10100:
	$(call run_smoke,live_smoke_cfoaq10100)

## KRX night-derivatives orderable-quantity smoke: read-only CCENQ10100 inquiry
## (NOT an order; krx_extended). Set LS_LIVE_SMOKE_FNOISU to a current code.
live-smoke-ccenq10100:
	$(call run_smoke,live_smoke_ccenq10100)

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

## t1481 (시간외등락율상위) smoke: token -> one single-page after-hours top
## change-rate read (body idx serialized as a number at first-page convention 0).
live-smoke-t1481:
	$(call run_smoke,live_smoke_t1481)

## t1482 (시간외거래량상위) smoke: token -> one single-page after-hours top-volume
## read (sort_gbn + body idx serialized as numbers at first-page convention 0).
live-smoke-t1482:
	$(call run_smoke,live_smoke_t1482)

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

## t1958 (ELW종목비교) smoke: token -> t8431 ELW list -> compare first two shcodes
## (chained, self-sourcing). t1964 (ELW전광판) smoke: token -> t9905 underlying
## list -> board for the first underlying with listed ELWs (chained).
live-smoke-t1958:
	$(call run_smoke,live_smoke_t1958)
live-smoke-t1964:
	$(call run_smoke,live_smoke_t1964)

## Wave 2 market-flow analytics reads (documented gubun defaults; non-empty -> flip).
live-smoke-t1601:
	$(call run_smoke,live_smoke_t1601)
live-smoke-t1615:
	$(call run_smoke,live_smoke_t1615)
live-smoke-t1640:
	$(call run_smoke,live_smoke_t1640)
live-smoke-t1662:
	$(call run_smoke,live_smoke_t1662)
live-smoke-t1664:
	$(call run_smoke,live_smoke_t1664)

## t3341 (재무순위종합) smoke: token -> one single-page financial-ranking read
## (body idx serialized as a number at first-page convention 0).
live-smoke-t3341:
	$(call run_smoke,live_smoke_t3341)

## t8424 (전체업종) smoke: token -> one all-sectors read (anchor + upcode source).
live-smoke-t8424:
	$(call run_smoke,live_smoke_t8424)

## t1511 (업종현재가) smoke: token -> one sector index snapshot (upcode=001). In-session.
live-smoke-t1511:
	$(call run_smoke,live_smoke_t1511)

## t1485 (예상지수) smoke: token -> one sector expected-index read (upcode=001). In-session.
live-smoke-t1485:
	$(call run_smoke,live_smoke_t1485)

## t1516 (업종별종목시세) smoke: token -> one per-sector stock board (upcode=001, shcode=005930). In-session.
live-smoke-t1516:
	$(call run_smoke,live_smoke_t1516)

## t1514 (업종기간별추이) smoke: token -> one first-page sector period-trend (upcode=001; cnt numeric).
live-smoke-t1514:
	$(call run_smoke,live_smoke_t1514)

## t2301 (옵션전광판) smoke: token -> one F/O option-board read (yyyymm=202609, gubun=G). Master read.
live-smoke-t2301:
	$(call run_smoke,live_smoke_t2301)

## t2522 (주식선물기초자산조회) smoke: token -> one F/O underlying-asset master read (no caller input). Master read.
live-smoke-t2522:
	$(call run_smoke,live_smoke_t2522)

## t8401 (주식선물마스터조회) smoke: token -> one F/O stock-futures master read (no caller input). Master read.
live-smoke-t8401:
	$(call run_smoke,live_smoke_t8401)

## t8426 (상품선물마스터조회) smoke: token -> one F/O commodity-futures master read (no caller input). Master read.
live-smoke-t8426:
	$(call run_smoke,live_smoke_t8426)

## t8433 (지수옵션마스터조회) smoke: token -> one F/O index-option master read (no caller input). Master read.
live-smoke-t8433:
	$(call run_smoke,live_smoke_t8433)

## t8435 (파생종목마스터조회) smoke: token -> one F/O derivatives master read (gubun=MF). Master read.
live-smoke-t8435:
	$(call run_smoke,live_smoke_t8435)

## t8467 (지수선물마스터조회) smoke: token -> one F/O index-futures master read (gubun=Q). Master read.
live-smoke-t8467:
	$(call run_smoke,live_smoke_t8467)

## t9943 (지수선물마스터조회) smoke: token -> one F/O index-futures master read (gubun=V). Master read.
live-smoke-t9943:
	$(call run_smoke,live_smoke_t9943)

## t9944 (지수옵션마스터조회) smoke: token -> one F/O index-option master read (no caller input). Master read.
live-smoke-t9944:
	$(call run_smoke,live_smoke_t9944)

## t2111 (선물/옵션현재가시세) smoke: token -> t8467 contract source -> one F/O current-price read.
live-smoke-t2111:
	$(call run_smoke,live_smoke_t2111)

## t2112 (선물/옵션현재가호가) smoke: token -> t8467 contract source -> one F/O order-book read.
live-smoke-t2112:
	$(call run_smoke,live_smoke_t2112)

## t2106 (선물/옵션현재가시세메모) smoke: token -> t8467 contract source -> one F/O price-memo read.
live-smoke-t2106:
	$(call run_smoke,live_smoke_t2106)

## t8402 (주식선물현재가) smoke: token -> t8401 contract source -> one stock-futures current-price read.
live-smoke-t8402:
	$(call run_smoke,live_smoke_t8402)

## t8403 (주식선물호가) smoke: token -> t8401 contract source -> one stock-futures order-book read.
live-smoke-t8403:
	$(call run_smoke,live_smoke_t8403)

## t8434 (선물/옵션멀티현재가) smoke: token -> t8467 contract source -> one F/O multi current-price read (qrycnt=1).
live-smoke-t8434:
	$(call run_smoke,live_smoke_t8434)

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
