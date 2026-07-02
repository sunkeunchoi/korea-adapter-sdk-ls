# Paper Live Smoke — convenience wrapper over the #[ignore] integration tests in
# crates/ls-sdk/tests/live_smoke.rs.
#
# Credentials load from a gitignored named per-lane env file (`.env.<lane>`) by
# sourcing it in the recipe shell (`set -a; . ./.env.<lane>; set +a`). The legacy
# single `.env` was retired (env-lane cutover, plan 2026-07-01-002): the default
# lane is now `.env.domestic` and there is NO `.env` fallback — a missing lane
# file fails loud (wrong-account hazard) rather than silently authenticating as
# the wrong account. We deliberately do NOT use make's `include` — make keeps
# surrounding quotes literally, so a quoted "appkey" reaches the SDK with the
# quote characters and the gateway rejects it (403). The shell strips quotes and
# tolerates # / $ in values. We also avoid `export $(shell cat .env.<lane> |
# xargs)`, which mangles values and risks echoing them. The SDK itself never
# reads these files; this Makefile is the only dotenv layer.
#
# Each target runs exactly one #[ignore] test by name and fails if zero tests
# ran (a filter typo must not read as green).

# Export command-line / make variables (e.g. LS_LIVE_SMOKE_*) to recipe shells.
export

.PHONY: live-smoke live-smoke-book live-smoke-chart live-smoke-account live-smoke-ws live-smoke-ws-negative live-smoke-k3 live-smoke-ws-p1 live-smoke-ws-p2 live-smoke-ws-p3 live-smoke-ws-p4 live-smoke-t8425 live-smoke-t8436 live-smoke-t1531 live-smoke-t1537 live-smoke-t1452 live-smoke-t1403 live-smoke-t1441 live-smoke-t1463 live-smoke-t1466 live-smoke-t1489 live-smoke-t1492 live-smoke-t1481 live-smoke-t1482 live-smoke-t1866 live-smoke-t1859 live-smoke-t1826 live-smoke-t1825 live-smoke-t9905 live-smoke-t9907 live-smoke-t8431 live-smoke-t8430 live-smoke-t9942 live-smoke-t1958 live-smoke-t1964 live-smoke-t1601 live-smoke-t1615 live-smoke-t1640 live-smoke-t1662 live-smoke-t1664 live-smoke-t3341 live-smoke-t8424 live-smoke-t1511 live-smoke-t1485 live-smoke-t1516 live-smoke-t1514 live-smoke-cspaq12300 live-smoke-cspaq22200 live-smoke-cfobq10500 live-smoke-ccenq90200 live-smoke-cfoaq10100 live-smoke-ccenq10100 live-smoke-t2301 live-smoke-t2522 live-smoke-t8401 live-smoke-t8426 live-smoke-t8433 live-smoke-t8435 live-smoke-t8467 live-smoke-t9943 live-smoke-t9944 live-smoke-t2111 live-smoke-t2112 live-smoke-t2106 live-smoke-t8402 live-smoke-t8403 live-smoke-t8434 live-smoke-t1988 live-smoke-t3320 live-smoke-t8455 live-smoke-t8460 live-smoke-t8463 live-smoke-g3101 live-smoke-g3104 live-smoke-g3106 live-smoke-g3102 live-smoke-g3103 live-smoke-g3190 live-smoke-o3101 live-smoke-o3121 live-smoke-o3105 live-smoke-o3106 live-smoke-o3125 live-smoke-o3126 live-smoke-t9945 live-smoke-t3202 live-smoke-t3401 live-smoke-t3518 live-smoke-t3521 live-smoke-o3103 live-smoke-o3104 live-smoke-o3108 live-smoke-o3116 live-smoke-o3117 live-smoke-o3123 live-smoke-o3127 live-smoke-o3128 live-smoke-o3136 live-smoke-o3137 live-smoke-o3139 live-smoke-t8462 live-smoke-t8410 live-smoke-t8451 live-smoke-t8419 live-smoke-t4203 live-smoke-t1901 live-smoke-t1906 live-smoke-t8450 live-smoke-t1638 live-smoke-t1308 live-smoke-t1449 live-smoke-t1621 live-smoke-t2545 live-smoke-t8406 live-smoke-t8407 live-smoke-t1631 live-smoke-t1632 live-smoke-t1633 live-smoke-t1716 live-smoke-t1902 live-smoke-t1904 live-smoke-t1927 live-smoke-t1941 live-smoke-t1702 live-smoke-t1717 live-smoke-t1665 live-smoke-t1471 live-smoke-t1475 live-smoke-t1959 live-smoke-t1950 live-smoke-t1954 live-smoke-t1971 live-smoke-t1972 live-smoke-t1974 live-smoke-t1956 live-smoke-t1969 live-smoke-t1105 live-smoke-t1104 live-smoke-t1305 live-smoke-t1310 live-smoke-t1404 live-smoke-t1410 live-smoke-t1411 live-smoke-t1488 live-smoke-t1636 live-smoke-t1809 live-smoke-t1109 live-smoke-t1301 live-smoke-t1486 live-smoke-t8454 live-smoke-t1637 live-smoke-t1602 live-smoke-t1603 live-smoke-t1617 live-smoke-t1752 live-smoke-t1771 live-smoke-t8417 live-smoke-t8418 live-smoke-t8411 live-smoke-t8452 live-smoke-t8453 live-smoke-t1302 live-smoke-t8464 live-smoke-t8465 live-smoke-t8466 live-smoke-t2216 live-smoke-t8405 live-smoke-t1444 live-smoke-t1422 live-smoke-t1427 live-smoke-t1442 live-smoke-t1405 live-smoke-t1960 live-smoke-t1961 live-smoke-t1966 live-smoke-t1921 live-smoke-t1532 live-smoke-t1533 live-smoke-t1926 live-smoke-t1764 live-smoke-t1903 live-smoke-t0424 live-smoke-t0167 live-smoke-cspbq00200 live-smoke-clnaq00100 live-smoke-cfoeq11100 live-smoke-t0441 live-smoke-cidbq01400 live-smoke-cidbq03000 live-smoke-cidbq05300 live-smoke-t8427 live-smoke-t2210 live-smoke-t2424 live-smoke-t2541 live-smoke-t2214 live-smoke-t8428 live-smoke-nws-t3102 live-smoke-order live-smoke-order-chain live-smoke-fo-order live-smoke-overseas-fo-order live-smoke-fo-position raw-probe live-smoke-t8412-negative

# Per-account credential lanes (paper-account-credential-lanes wave; env-lane
# cutover, plan 2026-07-01-002):
# Each LS paper account is bound to its own appkey; the account is whichever the
# token resolves to, so multi-account access is one credential FILE per account.
# A smoke sources its TR's lane by instrument_domain:
#   stock / unmapped -> .env.domestic          (the default lane, account ...3701)
#   futures_options  -> .env.domestic_option   (account ...51)
#   overseas_stock   -> .env.overseas          (account ...)
#   overseas_futures -> .env.overseas_option   (account ...71)
# A non-default target sets LS_SMOKE_LANE as a TARGET-SPECIFIC variable (the
# grouped lines below). run_smoke resolves an empty LS_SMOKE_LANE to `domestic`,
# then sources `.env.<lane>` and FAILS FAST if that file is missing — EVERY lane
# (including the default) runs through the guard; there is NO fallback to a bare
# `.env`, which would re-introduce the wrong-account bug this wave fixes. Sourcing
# stays in the recipe shell (never make `include`; see makefile-include-env-quotes
# solution). Pass LS_SMOKE_LANE=<lane> on the command line to point `raw-probe`
# at a lane too. The fail-fast guard is regression-tested by `make lane-check`.

# $(1) = exact test name in crates/ls-sdk/tests/live_smoke.rs
#
# Lane resolution: an empty LS_SMOKE_LANE defaults to `domestic` (.env.domestic),
# so EVERY lane — including the default — runs through the fail-fast guard. There
# is no `.env` fallback: a missing lane file exits non-zero (a missing file must
# fail loud, never silently authenticate as the wrong account).
define run_smoke
	@lane="$(LS_SMOKE_LANE)"; [ -n "$$lane" ] || lane="domestic"; \
	lane_file=".env.$$lane"; \
	[ -f "$$lane_file" ] || { echo "FAIL: $(1): lane file $$lane_file missing (LS_SMOKE_LANE=$$lane); refusing to fall back to .env (wrong-account hazard)"; exit 1; }; \
	set -a; . "./$$lane_file"; set +a; \
	out=$$(cargo test -p ls-sdk --test live_smoke -- --ignored --exact --nocapture $(1) 2>&1); \
	echo "$$out"; \
	echo "$$out" | grep -q "1 passed" || { echo "FAIL: $(1) did not run (0 tests) or did not pass"; exit 1; }
endef

# --- Lane assignments (target-specific LS_SMOKE_LANE; R5 mapping) ---------------
# futures_options reads authenticate as the domestic-option account (...51).
live-smoke-cfoeq11100 live-smoke-t0441 live-smoke-cfobq10500 live-smoke-cfoaq10100 \
live-smoke-ccenq90200 live-smoke-ccenq10100 live-smoke-t2301 live-smoke-t2522 \
live-smoke-t8401 live-smoke-t8426 live-smoke-t8433 live-smoke-t8435 live-smoke-t8467 \
live-smoke-t9943 live-smoke-t9944 live-smoke-t2111 live-smoke-t2112 live-smoke-t2106 \
live-smoke-t8402 live-smoke-t8403 live-smoke-t8434 live-smoke-t8455 live-smoke-t8460 \
live-smoke-t8463 live-smoke-t2545 live-smoke-t8406 live-smoke-t8462 \
live-smoke-t8427 live-smoke-t2210 live-smoke-t2424 live-smoke-t2541 \
live-smoke-t2214 live-smoke-fo-order live-smoke-fo-position: LS_SMOKE_LANE = domestic_option

# overseas_futures reads authenticate as the overseas-option account (...71).
live-smoke-cidbq01400 live-smoke-cidbq03000 live-smoke-cidbq05300 \
live-smoke-o3101 live-smoke-o3121 live-smoke-o3105 \
live-smoke-o3106 live-smoke-o3125 live-smoke-o3126 \
live-smoke-o3103 live-smoke-o3104 live-smoke-o3108 live-smoke-o3116 live-smoke-o3117 \
live-smoke-o3123 live-smoke-o3127 live-smoke-o3128 live-smoke-o3136 live-smoke-o3137 \
live-smoke-o3139 live-smoke-overseas-fo-order: LS_SMOKE_LANE = overseas_option

# overseas_stock reads authenticate as the overseas-stock account (.env.overseas);
# the lane token is `overseas` (the file suffix), not the instrument_domain name.
# Previously these fell through to the domestic default — the live wrong-account fix.
live-smoke-g3101 live-smoke-g3102 live-smoke-g3103 live-smoke-g3104 \
live-smoke-g3106 live-smoke-g3190: LS_SMOKE_LANE = overseas

## Default smoke: paper guard -> OAuth token -> one t1102 quote (no date needed).
live-smoke:
	$(call run_smoke,live_smoke_default)

## Guarded paper-order evidence matrix (CSPAT00601 submit + t0425 reconcile).
## Places REAL paper orders — gated behind an EXPLICIT opt-in (LS_ORDER_SMOKE=1)
## beyond the paper guard. Runs the resting-buy/resting-sell/marketable/
## deliberate-reject matrix, captures every rsp_cd to pin the order predicate,
## and reconciles resting orders via t0425. Records Pending (still "passes") if
## the paper account cannot place in-window. Cleanup is by paper reset — any
## unexpected fill must be unwound out-of-band by the operator.
##   make live-smoke-order            # symbol defaults to 005930, MbrNo to NXT
live-smoke-order:
	@lane="$(LS_SMOKE_LANE)"; [ -n "$$lane" ] || lane="domestic"; \
	lane_file=".env.$$lane"; \
	[ -f "$$lane_file" ] || { echo "FAIL: order smoke: lane file $$lane_file missing (LS_SMOKE_LANE=$$lane); refusing to fall back to .env (wrong-account hazard)"; exit 1; }; \
	set -a; . "./$$lane_file"; set +a; \
	export LS_ORDER_SMOKE=1 LS_ORDER_SMOKE_TR=CSPAT00601; \
	out=$$(cargo test -p ls-sdk --test order_smoke -- --ignored --exact --nocapture chain::order_smoke_matrix 2>&1); \
	echo "$$out"; \
	echo "$$out" | grep -q "1 passed" || { echo "FAIL: order smoke did not run (0 tests) or did not pass"; exit 1; }

## AUTONOMOUS chained paper-order run (submit -> modify -> cancel -> flat-assert).
## The agent invokes this directly during a human-present wave — NO operator handoff.
## The FIRST leg is gate 1's evidence (CSPAT00601 + t0425); the modify/cancel legs are
## gate 2's (CSPAT00701 + CSPAT00801). After teardown it asserts the account is
## account-wide FLAT (U3): a resting remainder is retry-canceled then hard-failed, a
## fill hard-fails immediately.
##
## FAIL-CLOSED autonomy preconditions (the smoke refuses unless ALL hold):
##   - LS_TRADING_ENV=paper + LS_ORDER_SMOKE=1 (the standing double opt-in), AND
##   - NO CI/no-TTY marker (run in an attended PTY), AND
##   - a FRESH per-wave human nonce within TTL — mint it each wave:
##       export LS_ORDER_SMOKE_NONCE=$(date +%s)
##     (a static/reused nonce is rejected; the nonce is the human-present signal that
##      passive CI detection alone cannot provide — KTD1. Do NOT put the nonce in
##      .env.domestic — this recipe sources the lane file and a stale value there
##      would clobber it.)
## Pending vs hard-fail: if NOTHING is placed (out-of-window / not order-capable /
## degenerate band) it records Pending and "passes". But once an order is PLACED, a
## still-resting order, an unexpected fill, or a failed flat scan HARD-FAILS the build
## (there is no operator to clean up — autonomy trades the pre-placement checkpoint for
## loud post-run detection). gate 1 never waits on gate 2.
##   export LS_ORDER_SMOKE_NONCE=$(date +%s); make live-smoke-order-chain
live-smoke-order-chain:
	@lane="$(LS_SMOKE_LANE)"; [ -n "$$lane" ] || lane="domestic"; \
	lane_file=".env.$$lane"; \
	[ -f "$$lane_file" ] || { echo "FAIL: chained order smoke: lane file $$lane_file missing (LS_SMOKE_LANE=$$lane); refusing to fall back to .env (wrong-account hazard)"; exit 1; }; \
	set -a; . "./$$lane_file"; set +a; \
	export LS_ORDER_SMOKE=1 LS_ORDER_SMOKE_TR=CSPAT00601; \
	out=$$(cargo test -p ls-sdk --test order_smoke -- --ignored --exact --nocapture chain::order_chained_smoke 2>&1); \
	echo "$$out"; \
	echo "$$out" | grep -q "1 passed" || { echo "FAIL: chained order smoke did not run (0 tests) or did not pass"; exit 1; }

## Guarded F/O chained paper order (plan 2026-06-30-003, U4/U6): submit → modify →
## cancel the DOMESTIC futures/options order chain (CFOAT00100/00200/00300) against the
## real LS paper gateway. Same fail-closed guards as live-smoke-order-chain (paper +
## LS_ORDER_SMOKE=1 double opt-in, no CI/no-TTY, a FRESH per-wave nonce). The CURRENT
## valid F/O contract is self-sourced at runtime from the t8467 index-futures master
## (front-month) so the operator supplies NO contract; set LS_FO_ORDER_SMOKE_SHCODE only
## to OVERRIDE with a specific contract (a stale hardcoded value would fail).
## Pricing is at the daily limit (t2111 상한가/하한가) so the order rests far from market;
## flatness is two-part and fail-closed (t0441 fill-detection + clean-cancel removal).
## OPERATOR-RUN, never autonomous (U4/U6). Run in an attended PTY:
##   export LS_ORDER_SMOKE_NONCE=$(date +%s); make live-smoke-fo-order
## Authenticates on the domestic_option lane (...51, LS_SMOKE_LANE mapping above) —
## the SAME F/O-capable account the F/O reads (incl. t0441) use — with a fail-closed
## guard that refuses to fall back to .env when the lane file is absent (wrong-account
## hazard; mirrors the run_smoke guard). Plan 2026-07-01-001 U1 (R1, R2).
live-smoke-fo-order:
	@lane="$(LS_SMOKE_LANE)"; [ -n "$$lane" ] || lane="domestic"; \
	lane_file=".env.$$lane"; \
	[ -f "$$lane_file" ] || { echo "FAIL: fo_order_chained_smoke: lane file $$lane_file missing (LS_SMOKE_LANE=$$lane); refusing to fall back to .env (wrong-account hazard)"; exit 1; }; \
	set -a; . "./$$lane_file"; set +a; \
	export LS_ORDER_SMOKE=1; \
	out=$$(cargo test -p ls-sdk --test order_smoke -- --ignored --exact --nocapture fo::fo_order_chained_smoke 2>&1); \
	echo "$$out"; \
	echo "$$out" | grep -q "1 passed" || { echo "FAIL: F/O chained order smoke did not run (0 tests) or did not pass"; exit 1; }

## Guarded OVERSEAS-F/O chained order smoke (plan 2026-07-01-005, U3): submit → modify →
## cancel the OVERSEAS futures order chain (CIDBT00100/00900/01000) against the real LS
## paper gateway. Same fail-closed guards as live-smoke-fo-order (paper + LS_ORDER_SMOKE=1
## double opt-in, no CI/no-TTY, a FRESH per-wave nonce). This is the CAPABILITY GATE: the
## submit leg proves whether the overseas-option account is order-capable (a `01491`
## outcome is in scope). The overseas-futures order surface exposes NO daily price-band
## read and NO transient-position read, so the operator PINS the front-month contract and
## a far-from-market resting price:
##   export LS_OVERSEAS_FO_ORDER_SMOKE_SYMBOL=<front-month symbol, e.g. ESU26>
##   export LS_OVERSEAS_FO_ORDER_SMOKE_DUEYYMM=<YYYYMM, e.g. 202609>
##   export LS_OVERSEAS_FO_ORDER_SMOKE_PRICE=<a valid far-below-market tick>   # optional
## The pinned symbol is VALIDATED against the o3101 master universe (ExchCode/CrcyCode/
## PrdtCode sourced from that row); flatness is CLEAN-CANCEL-ONLY and fail-closed (there
## is no overseas transient-position read). A serde defect surfaces as IGW40011 and is a
## LOUD failure, never a capability PENDING. OPERATOR-RUN, never autonomous. Run in an open
## overseas-futures window in an attended PTY:
##   export LS_ORDER_SMOKE_NONCE=$(date +%s); make live-smoke-overseas-fo-order
## Authenticates on the overseas_option lane (...71, LS_SMOKE_LANE mapping above) — the
## SAME account the overseas-futures reads (o31xx / CIDBQ*) use — with a fail-closed guard
## that refuses to fall back to .env when the lane file is absent (wrong-account hazard).
live-smoke-overseas-fo-order:
	@lane="$(LS_SMOKE_LANE)"; [ -n "$$lane" ] || lane="domestic"; \
	lane_file=".env.$$lane"; \
	[ -f "$$lane_file" ] || { echo "FAIL: overseas_fo_chained_smoke: lane file $$lane_file missing (LS_SMOKE_LANE=$$lane); refusing to fall back to .env (wrong-account hazard)"; exit 1; }; \
	set -a; . "./$$lane_file"; set +a; \
	export LS_ORDER_SMOKE=1; \
	out=$$(cargo test -p ls-sdk --test order_smoke -- --ignored --exact --nocapture overseas_fo::overseas_fo_chained_smoke 2>&1); \
	echo "$$out"; \
	echo "$$out" | grep -q "1 passed" || { echo "FAIL: overseas-F/O chained order smoke did not run (0 tests) or did not pass"; exit 1; }

## Guarded F/O position MANUFACTURE (plan 2026-07-01-003, U2): submit a MARKETABLE
## domestic F/O buy so it FILLS, certify the non-empty t0441 balance-valuation read (the
## t0441 flip witness), then flatten fail-closed via an opposite-side marketable close.
## UNLIKE live-smoke-fo-order (which rests unfillable at the daily limit and never holds a
## position), this DELIBERATELY manufactures a transient position — so it places REAL
## marketable paper orders and MUST run in an open KRX F/O window. The harness IS the
## flatten-feasibility gate (no separate hand-run spike): its preflight flat-gate, 1-lot
## marketable buy, bounded flatten (<=2 attempts), and kill-switch-after-teardown machinery
## prove in-session flatness safely and fail closed if they cannot (no out-of-band reset).
## Same fail-closed guards as live-smoke-fo-order (paper +
## LS_ORDER_SMOKE=1 double opt-in, no CI/no-TTY, a FRESH per-wave nonce); authenticates on
## the domestic_option lane (...51, the F/O-capable account t0441 reads). The fill-poll
## bound is U1-calibrated — override with LS_FO_MANUFACTURE_POLL_ATTEMPTS if U1 measured a
## slower fill. OPERATOR-RUN, never autonomous. Run in an attended PTY:
##   export LS_ORDER_SMOKE_NONCE=$(date +%s); make live-smoke-fo-position
live-smoke-fo-position:
	@lane="$(LS_SMOKE_LANE)"; [ -n "$$lane" ] || lane="domestic"; \
	lane_file=".env.$$lane"; \
	[ -f "$$lane_file" ] || { echo "FAIL: fo_position_manufacture_smoke: lane file $$lane_file missing (LS_SMOKE_LANE=$$lane); refusing to fall back to .env (wrong-account hazard)"; exit 1; }; \
	set -a; . "./$$lane_file"; set +a; \
	export LS_ORDER_SMOKE=1; \
	out=$$(cargo test -p ls-sdk --test order_smoke -- --ignored --exact --nocapture fo::fo_position_manufacture_smoke 2>&1); \
	echo "$$out"; \
	echo "$$out" | grep -q "1 passed" || { echo "FAIL: F/O position manufacture smoke did not run (0 tests) or did not pass"; exit 1; }

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

## Stock balance smoke: read-only t0424 (cash summary + holdings array; the U2
## holdings gate — holdings=0 is the cash-only account, not a defect).
live-smoke-t0424:
	$(call run_smoke,live_smoke_t0424)

## Server-time smoke: read-only t0167 utility (always populated, closure-viable).
live-smoke-t0167:
	$(call run_smoke,live_smoke_t0167)

## Order-capacity smoke: read-only CSPBQ00200 (capacity by margin rate; numeric
## RecCnt/OrdPrc serialize as JSON numbers or IGW40011).
live-smoke-cspbq00200:
	$(call run_smoke,account::live_smoke_cspbq00200)

## Loanable-stock smoke: read-only CLNAQ00100 reference list (full-list mode;
## persistent universe, closure-viable).
live-smoke-clnaq00100:
	$(call run_smoke,live_smoke_clnaq00100)

## F/O deposit-detail smoke: read-only CFOEQ11100 (예수금/증거금; empty 00707 on a
## position-less paper account → PENDING).
live-smoke-cfoeq11100:
	$(call run_smoke,live_smoke_cfoeq11100)

## F/O balance-valuation smoke: read-only t0441 (positions + summary; empty on a
## position-less paper account → PENDING).
live-smoke-t0441:
	$(call run_smoke,account::live_smoke_t0441)

## Overseas-futures order-qty smoke: read-only CIDBQ01400 (overseas paper feeds
## historically empty → PENDING).
live-smoke-cidbq01400:
	$(call run_smoke,live_smoke_cidbq01400)

## Overseas-futures deposit/balance smoke: read-only CIDBQ03000 (lane overseas_option,
## account …71; empty/all-default on the wrong account → PENDING).
live-smoke-cidbq03000:
	$(call run_smoke,live_smoke_cidbq03000)

## Overseas-futures deposited-assets smoke: read-only CIDBQ05300 (lane overseas_option,
## account …71; the cash account returned IGW40013 — wrong-account artifact).
live-smoke-cidbq05300:
	$(call run_smoke,live_smoke_cidbq05300)

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
## Runs the generic ws_lifecycle_smoke helper for S3_ (market-data, tr_type 3).
live-smoke-ws:
	$(call run_smoke,live_smoke_ws)

## K3_ (KOSDAQ 체결) lifecycle smoke — the flip gate for K3_ (market-data, tr_type
## 3). Per KTD6 (NOT-OBSERVABLE), a clean run proves connection reachability only.
## Set LS_LIVE_SMOKE_SHCODE to a KOSDAQ code for a venue-representative run.
live-smoke-k3:
	$(call run_smoke,live_smoke_k3)

## Combined P1 market-data WS smoke — the ONE command that gates the 14-TR wave
## (H1_/HA_/S2_/US3/UH1/US2 stock, GSC/GSH overseas-stock, OVC/OVH overseas-
## futures, OC0/OH0/FC9/FH9 F-O). Each TR runs on a fresh manager; a per-TR
## failure is recorded (not abort) and the run fails red iff any TR failed. Per
## KTD6 (NOT-OBSERVABLE) a clean run proves connection reachability only. Override
## the stock key via LS_LIVE_SMOKE_SHCODE; overseas/F-O keys are public symbols.
live-smoke-ws-p1:
	$(call run_smoke,live_smoke_ws_p1)

## Combined P2 order-event WS smoke — the ONE command that gates the 16-TR wave
## (SC0-SC4 stock, C01/O01/H01 F-O, AS0-AS4 overseas-stock, TC1-TC3 overseas-
## futures). OBSERVATION-ONLY: subscribes/unsubscribes order-event feeds; NEVER
## places, amends, or cancels an order. Each TR runs on a fresh manager; a per-TR
## failure is recorded (not abort) and the run fails red iff any TR failed. SC*
## are account-bound (empty tr_key); others use cert symbols. Per KTD6
## (NOT-OBSERVABLE) and unestablished paper reachability a clean run proves
## connection reachability only — a meaningful share may stay Tracked-only.
live-smoke-ws-p2:
	$(call run_smoke,live_smoke_ws_p2)

## Combined P3 market-data WS smoke — the ONE command that gates the closure-flip
## WS batch (plan -004): 31 NEW realtime market-data channels (stock 15, F-O 8,
## sector/overseas-fut/etc/investinfo 8). Each TR on a fresh manager; a per-TR
## failure is recorded (not abort) and the run fails red iff any TR failed. Per
## KTD6 (NOT-OBSERVABLE) a clean run proves connection reachability only — flip
## each channel connection-reachable-only. NO raw-frame logging.
live-smoke-ws-p3:
	$(call run_smoke,live_smoke_ws_p3)

## WebSocket COMBINED lifecycle smoke for the 39 open-window-wave market-data
## channels (plan 2026-06-29-001). One command gates the whole flip; each TR on a
## fresh manager, RESILIENT (one bad TR cannot hide the others), red only if ANY
## TR failed its lifecycle. KTD6 (NOT-OBSERVABLE): a clean run proves connection
## reachability only — flip each channel connection-reachable-only. NO raw-frame
## logging.
live-smoke-ws-p4:
	$(call run_smoke,live_smoke_ws_p4)

## WebSocket NEGATIVE control (KTD6): subscribe a deliberately-INVALID tr_cd and
## record whether the paper gateway emits an OBSERVABLE rejection in the timebox.
## The live half of "does an unknown tr_cd produce an observable signal?" — its
## recorded result decides whether U5/U6 flips are per-TR-reachable or only
## connection-reachable-only. Do NOT fabricate the answer; run it live.
live-smoke-ws-negative:
	$(call run_smoke,live_smoke_ws_negative)

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
live-smoke-t8430:
	$(call run_smoke,live_smoke_t8430)
## ETF quote smoke: read-only t1901 ETF현재가 (shcode defaults to 069500 KODEX 200);
## KRX-session-dependent. Override with LS_LIVE_SMOKE_T1901_SHCODE.
live-smoke-t1901:
	$(call run_smoke,live_smoke_t1901)
## ETF LP order-book smoke: read-only t1906 ETFLP호가 (shcode defaults to 152100);
## persistent read reachable under closure. Override with LS_LIVE_SMOKE_T1906_SHCODE.
live-smoke-t1906:
	$(call run_smoke,live_smoke_t1906)
## Current-price/order-book smoke: read-only t8450 (통합)주식현재가호가조회2
## (shcode defaults 005930, exchgubun N); reachable under closure. Override with
## LS_LIVE_SMOKE_T8450_SHCODE / LS_LIVE_SMOKE_T8450_EXCHGUBUN.
live-smoke-t8450:
	$(call run_smoke,live_smoke_t8450)
## Remaining-quantity/pre-disclosure ranking smoke: read-only t1638 종목별잔량/사전공시
## (gubun1=1, shcode="" full list, gubun2=1, exchgubun=""); reachable under closure.
## Override with LS_LIVE_SMOKE_T1638_GUBUN1/_SHCODE/_GUBUN2/_EXCHGUBUN.
live-smoke-t1638:
	$(call run_smoke,live_smoke_t1638)
## Time-bucketed trade-chart smoke: read-only t1308 주식시간대별체결조회챠트
## (shcode 005930, starttime/endtime "" full session, bun_term 1, exchgubun ""); reachable under closure.
## Override with LS_LIVE_SMOKE_T1308_SHCODE/_STARTTIME/_ENDTIME/_BUN_TERM/_EXCHGUBUN.
live-smoke-t1308:
	$(call run_smoke,live_smoke_t1308)
## Price-band trade-weight smoke: read-only t1449 가격대별매매비중조회
## (shcode 005930, dategb 1 — dategb MUST be non-empty or the board is empty); reachable under closure.
## Override with LS_LIVE_SMOKE_T1449_SHCODE/_DATEGB.
live-smoke-t1449:
	$(call run_smoke,live_smoke_t1449)
## By-time investor-trading smoke: read-only t1621 업종별분별투자자매매동향
## (upcode 001, nmin 0, cnt 20, bgubun 0, exchgubun ""); nmin/cnt serialize as JSON
## NUMBERS (string form returns IGW40011, KTD3); reachable under closure.
## Override with LS_LIVE_SMOKE_T1621_UPCODE/_NMIN/_CNT/_BGUBUN/_EXCHGUBUN.
live-smoke-t1621:
	$(call run_smoke,live_smoke_t1621)
## F/O by-time investor-trading smoke: read-only t2545 상품선물투자자매매동향
## (eitem 01, sgubun 0, upcode 001, nmin 0, cnt 10, bgubun 0); nmin/cnt serialize
## as JSON NUMBERS and bgubun MUST be "0" (string nmin/cnt or bgubun="1" returns
## IGW40011/IGW50008, KTD3); reachable under closure.
## Override with LS_LIVE_SMOKE_T2545_EITEM/_SGUBUN/_UPCODE/_NMIN/_CNT/_BGUBUN.
live-smoke-t2545:
	$(call run_smoke,live_smoke_t2545)
## F/O by-tick conclusion smoke: read-only t8406 주식선물틱분별체결조회 — self-sources a
## live front-month contract from the t8467 index-futures master, then reads it
## (cgubun 1, bgubun 0, cnt 10); bgubun/cnt serialize as JSON NUMBERS (string form
## returns IGW40011, KTD3). F/O conclusion is session-dependent — an empty 00707
## even with a live contract dispositions to PENDING, not a flip.
live-smoke-t8406:
	$(call run_smoke,live_smoke_t8406)
## Multi-symbol current-price smoke: read-only t8407 API용주식멀티현재가조회
## (nrec 3, shcode "005930000660001200" = 3 concatenated codes); nrec serializes as
## a JSON NUMBER (string form returns IGW40011, KTD3); reachable under closure.
## Override with LS_LIVE_SMOKE_T8407_NREC/_SHCODE.
live-smoke-t8407:
	$(call run_smoke,live_smoke_t8407)
## LP-target ELW issue-list smoke: read-only t1959 LP대상종목정보조회 (shcode
## Program-trade综합 smoke: read-only t1631 프로그램매매종합조회 (market-wide,
## today's date). Override LS_LIVE_SMOKE_DATE / LS_LIVE_SMOKE_T1631_GUBUN.
live-smoke-t1631:
	$(call run_smoke,live_smoke_t1631)
## Chained NWS→t3102 smoke: subscribe realtime NWS news feed, thread a live
## realkey into t3102 (뉴스본문) as sNewsno. Gated on a live news frame
## (off-hours base rate may be ~zero → SMOKE-FAIL/HELD). Override
## LS_NWS_SMOKE_SECS (wait window) / LS_NWS_TR_KEY (subscribe key).
live-smoke-nws-t3102:
	$(call run_smoke,market_session_charts::live_smoke_nws_t3102)
## Program-trade intraday-trend smoke: read-only t1632 프로그램매매추이(시간)
## (market-wide, today). Override LS_LIVE_SMOKE_DATE.
live-smoke-t1632:
	$(call run_smoke,live_smoke_t1632)
## Program-trade daily-trend smoke: read-only t1633 프로그램매매추이(일별)
## (market-wide, recent range). Override LS_LIVE_SMOKE_DATE / LS_LIVE_SMOKE_T1633_FDATE.
live-smoke-t1633:
	$(call run_smoke,live_smoke_t1633)
## Foreign/institution by-issue trend smoke: read-only t1716 외인기관종목별동향
## (public ticker, recent range; prapp numeric). Override LS_LIVE_SMOKE_T1716_SHCODE / _FROMDT / LS_LIVE_SMOKE_DATE.
live-smoke-t1716:
	$(call run_smoke,live_smoke_t1716)
## ETF intraday NAV/price-trend smoke: read-only t1902 ETF시간별추이
## (public ETF). Override LS_LIVE_SMOKE_T1902_SHCODE.
live-smoke-t1902:
	$(call run_smoke,live_smoke_t1902)
## ETF constituent/PDF smoke: read-only t1904 ETF구성종목조회
## (public ETF, recent apply date). Override LS_LIVE_SMOKE_T1904_SHCODE / _DATE / LS_LIVE_SMOKE_DATE.
live-smoke-t1904:
	$(call run_smoke,live_smoke_t1904)
## Short-selling daily-trend smoke: read-only t1927 공매도일별추이
## (public ticker, recent range). Override LS_LIVE_SMOKE_T1927_SHCODE / _SDATE / LS_LIVE_SMOKE_DATE.
live-smoke-t1927:
	$(call run_smoke,live_smoke_t1927)
## Stock-loan/대차 daily-trend smoke: read-only t1941 종목별대차거래일간추이
## (public ticker, recent range). Override LS_LIVE_SMOKE_T1941_SHCODE / _SDATE / LS_LIVE_SMOKE_DATE.
live-smoke-t1941:
	$(call run_smoke,live_smoke_t1941)
## Foreign/institution by-issue trend smoke: read-only t1702 외국인/기관별 매매추이
## (public ticker, recent range). Override LS_LIVE_SMOKE_T1702_SHCODE / _FROMDT / LS_LIVE_SMOKE_DATE.
live-smoke-t1702:
	$(call run_smoke,live_smoke_t1702)
## Foreign/institution net-buy trend smoke: read-only t1717 외국인/기관 순매수추이
## (public ticker, recent range). Override LS_LIVE_SMOKE_T1717_SHCODE / _FROMDT / LS_LIVE_SMOKE_DATE.
live-smoke-t1717:
	$(call run_smoke,live_smoke_t1717)
## Investor-by-sector trend smoke: read-only t1665 투자자별 매매추이(업종) (KOSPI
## sector 001, recent range). Override LS_LIVE_SMOKE_T1665_UPCODE / _FROMDATE / LS_LIVE_SMOKE_DATE.
live-smoke-t1665:
	$(call run_smoke,live_smoke_t1665)
## Intraday quote-remainder trend smoke: read-only t1471 시간대별호가잔량추이
## (public ticker). Override LS_LIVE_SMOKE_T1471_SHCODE / _CNT.
live-smoke-t1471:
	$(call run_smoke,live_smoke_t1471)
## VP-relative rise/fall ranking smoke: read-only t1475 VP대비등락률상하위 (public
## ticker; numeric request slots). Override LS_LIVE_SMOKE_T1475_SHCODE / _DATACNT.
live-smoke-t1475:
	$(call run_smoke,live_smoke_t1475)
## defaults to "" = the full LP-target list; reachable under closure).
## Override with LS_LIVE_SMOKE_T1959_SHCODE.
live-smoke-t1959:
	$(call run_smoke,live_smoke_t1959)
## ELW current-price/quote smoke: read-only t1950 ELW현재가(시세)조회. CHAINS off
## t8431 for a FRESH ELW shcode (ELW codes expire), then quotes it.
live-smoke-t1950:
	$(call run_smoke,live_smoke_t1950)
## ELW daily-price smoke: read-only t1954 ELW일별주가. CHAINS off t8431 for a FRESH
## ELW shcode (ELW codes expire), then reads its daily OHLCV series (witness: close).
live-smoke-t1954:
	$(call run_smoke,live_smoke_t1954)
## ELW current-price + quote-board smoke: read-only t1971 ELW현재가호가조회. CHAINS
## off t8431 for a FRESH ELW shcode (ELW codes expire), then quotes its 10-level board.
live-smoke-t1971:
	$(call run_smoke,live_smoke_t1971)
## ELW current-price + trading-member (거래원) board smoke: read-only t1972
## ELW현재가(거래원)조회. CHAINS off t8431 for a FRESH ELW shcode (ELW codes expire),
## then reads its member board.
live-smoke-t1972:
	$(call run_smoke,live_smoke_t1972)
## ELWs-sharing-a-base-asset smoke: read-only t1974 ELW기초자산동일종목. CHAINS off
## t8431 for a FRESH ELW shcode (ELW codes expire), then reads the same-base sibling
## issue list (cnt summary + per-issue array).
live-smoke-t1974:
	$(call run_smoke,live_smoke_t1974)
## ELW current-price/payout smoke: read-only t1956 ELW현재가(확정지급액)조회. CHAINS off
## t8431 for a FRESH ELW shcode (ELW codes expire), then reads the single snapshot
## (hname/price/payout/analytics) + basket array.
live-smoke-t1956:
	$(call run_smoke,live_smoke_t1956)
## ELW screener smoke: read-only t1969 ELW지표검색 — the all-ELWs default screen
## (T1969Request::new; every chk* off, numeric ranges 0/0). Reachable under closure.
live-smoke-t1969:
	$(call run_smoke,live_smoke_t1969)
## Pivot/demark smoke: read-only t1105 (shcode defaults 005930, exchgubun K).
live-smoke-t1105:
	$(call run_smoke,live_smoke_t1105)
## Price-memo smoke: read-only t1104 (code defaults 005930, nrec 1, exchgubun K).
live-smoke-t1104:
	$(call run_smoke,live_smoke_t1104)
## Period-price smoke: read-only t1305 기간별주가 (shcode 005930, daily, today, cnt 10).
live-smoke-t1305:
	$(call run_smoke,live_smoke_t1305)
## Closed-window flip wave (plan -003). Tick/min chart t1310 (shcode 005930) +
## administrative-designation board t1404 (gubun 0, jongchk 1). Non-empty -> flip;
## empty 00707 -> PENDING (closed-window reachability is unproven, R5/R6/R7).
live-smoke-t1310:
	$(call run_smoke,live_smoke_t1310)
live-smoke-t1404:
	$(call run_smoke,live_smoke_t1404)
## Closed-window more-flips wave (plan -001). Ultra-low-liquidity board t1410
## (gubun 0, first-page cts_shcode). Non-empty -> flip; empty 00707 -> PENDING.
live-smoke-t1410:
	$(call run_smoke,live_smoke_t1410)
## Closed-window more-flips wave (plan -001). Stocks-by-margin-rate t1411 (gubun 0,
## jongchk 1, jkrate 1, shcode 005930, body idx=0 as a number). Non-empty -> flip;
## empty 00707 -> PENDING.
live-smoke-t1411:
	$(call run_smoke,live_smoke_t1411)
## Closed-window more-flips wave (plan -001). Expected-exec top-change-rate t1488
## (gubun 0, sign 1, jgubun 1, jongchk 0, volume 0, body idx=0 + yesprice/yeeprice/
## yevolume=0 as numbers). Non-empty -> flip; empty 00707 -> PENDING.
live-smoke-t1488:
	$(call run_smoke,live_smoke_t1488)
## Closed-window more-flips wave (plan -001). Per-stock program-trading-trend t1636
## (gubun 0, gubun1 0, gubun2 0, shcode 005930, exchgubun "", body cts_idx=0 as a
## number). Non-empty -> flip; empty 00707 -> PENDING.
live-smoke-t1636:
	$(call run_smoke,live_smoke_t1636)
## Closed-window more-flips wave (plan -001). Signal search t1809 (gubun 1, jmGb 1,
## jmcode 1, first-page string cts="1"). Non-empty -> flip; empty 00707 -> PENDING.
live-smoke-t1809:
	$(call run_smoke,live_smoke_t1809)
## Open-window domestic reads (plan -001). After-hours tick conclusion t1109
## (shcode 005930, first-page idx=0 as a number). Non-empty -> flip; empty -> PENDING.
live-smoke-t1109:
	$(call run_smoke,live_smoke_t1109)
## Open-window domestic reads (plan -001). Time-band tick conclusion t1301 (shcode
## 005930, cvolume=0 as a number, 0900-1530). Non-empty -> flip; empty -> PENDING.
live-smoke-t1301:
	$(call run_smoke,live_smoke_t1301)
## Open-window domestic reads (plan -001). Expected-conclusion t1486 (shcode 005930,
## cnt=20 as a number, exchgubun 1). Non-empty -> flip; auction-only/empty -> PENDING.
live-smoke-t1486:
	$(call run_smoke,live_smoke_t1486)
## Open-window domestic reads (plan -001). Exchange-qualified time-band tick t8454
## (shcode 005930, cvolume=0 as a number, 0900-1530, exchgubun 1). Non-empty -> flip.
live-smoke-t8454:
	$(call run_smoke,live_smoke_t8454)
## Open-window domestic reads (plan -001). Per-stock program-trade flow t1637
## (gubun1 0, gubun2 0, shcode 005930, today, exchgubun 1, cts_idx=0 as a number).
## Non-empty -> flip; empty -> PENDING.
live-smoke-t1637:
	$(call run_smoke,live_smoke_t1637)
## Open-window domestic reads (plan -001). Time-band investor flow by sector t1602
## (market 1, upcode 001, gubun1 1, gubun2 0, exchgubun 1, cts_idx=0/cnt=20 numbers).
## Non-empty -> flip; empty -> PENDING.
live-smoke-t1602:
	$(call run_smoke,live_smoke_t1602)
## Open-window domestic reads (plan -001). Investor detail by issue t1603 (market 1,
## gubun1 1, gubun2 0, upcode 001, exchgubun 1, cts_idx=0/cnt=20 numbers).
## Non-empty -> flip; empty -> PENDING.
live-smoke-t1603:
	$(call run_smoke,live_smoke_t1603)
## Open-window domestic reads (plan -001). Investor time/daily flow t1617 (gubun1 1,
## gubun2 1, gubun3 1, exchgubun 1; all-String request). Non-empty -> flip; empty -> PENDING.
live-smoke-t1617:
	$(call run_smoke,live_smoke_t1617)
## Open-window domestic reads (plan -001). Broker-by-issue t1752 (shcode 005930,
## traddate today, fwgubun1 0, exchgubun 1, cts_idx=0 as a number).
## Non-empty -> flip; empty -> PENDING.
live-smoke-t1752:
	$(call run_smoke,live_smoke_t1752)
## Open-window domestic reads (plan -001). Broker time-series t1771 (shcode 005930,
## tradno "", gubun1 0, traddate today, exchgubun 1, cts_idx=0/cnt=20 numbers; rows
## under t1771OutBlock2). Non-empty -> flip; empty -> PENDING.
live-smoke-t1771:
	$(call run_smoke,live_smoke_t1771)
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

## t1988 (기초자산리스트조회) smoke: token -> one ELW underlying-asset list read (all markets, filters off).
live-smoke-t1988:
	$(call run_smoke,live_smoke_t1988)

## t3320 (FNG_요약) smoke: token -> one FnGuide company-summary read (public gicode A005930).
live-smoke-t3320:
	$(call run_smoke,live_smoke_t3320)

## t8455 (KRX야간파생 마스터조회) smoke: token -> one night-derivatives master read (gubun=NF).
## venue_session krx_extended: meaningful only in the night session (~18:00-05:00 KST).
live-smoke-t8455:
	$(call run_smoke,live_smoke_t8455)

## t8460 (KRX야간파생 옵션 전광판) smoke: token -> one night-option-board read (gubun=G, near month).
## venue_session krx_extended (~18:00-05:00 KST).
live-smoke-t8460:
	$(call run_smoke,live_smoke_t8460)

## t8463 (KRX야간파생 투자자시간대별) smoke: token -> one investor-by-timeslot read (N/F/101).
## venue_session krx_extended (~18:00-05:00 KST).
live-smoke-t8463:
	$(call run_smoke,live_smoke_t8463)

## Overseas-stock reads (reach wave U7): token -> one read keyed by a public US
## ticker (82/TSLA = TSLA on NASDAQ). Domain overseas_stock, market_session route.
live-smoke-g3101:
	$(call run_smoke,live_smoke_g3101)

live-smoke-g3104:
	$(call run_smoke,live_smoke_g3104)

live-smoke-g3106:
	$(call run_smoke,live_smoke_g3106)

live-smoke-g3102:
	$(call run_smoke,live_smoke_g3102)

live-smoke-g3103:
	$(call run_smoke,live_smoke_g3103)

## g3190 (해외주식 마스터): token -> one master-list read (US, exchange 2, 10 rows).
live-smoke-g3190:
	$(call run_smoke,live_smoke_g3190)

## o3101 (해외선물 마스터): token -> one futures-master read (gubun=all).
live-smoke-o3101:
	$(call run_smoke,live_smoke_o3101)

## o3121 (해외선물옵션 마스터): token -> one option-master read (MktGb=O).
live-smoke-o3121:
	$(call run_smoke,live_smoke_o3121)

## o3105 (해외선물 현재가): token -> one futures-quote read (CUSN23).
live-smoke-o3105:
	$(call run_smoke,live_smoke_o3105)

## o3106 (해외선물 현재가호가): token -> one futures-order-book read (ADM23).
live-smoke-o3106:
	$(call run_smoke,live_smoke_o3106)

## o3125 (해외선물옵션 현재가): token -> one option-quote read (F/HSIM23).
live-smoke-o3125:
	$(call run_smoke,live_smoke_o3125)

## o3126 (해외선물옵션 현재가호가): token -> one option-order-book read (F/ADM23).
live-smoke-o3126:
	$(call run_smoke,live_smoke_o3126)

## Domestic stock master/reference breadth wave (plan -004).
live-smoke-t9945:
	$(call run_smoke,live_smoke_t9945)
live-smoke-t3202:
	$(call run_smoke,live_smoke_t3202)
live-smoke-t3401:
	$(call run_smoke,live_smoke_t3401)
live-smoke-t3518:
	$(call run_smoke,live_smoke_t3518)
live-smoke-t3521:
	$(call run_smoke,live_smoke_t3521)
live-smoke-o3103:
	$(call run_smoke,live_smoke_o3103)
live-smoke-o3104:
	$(call run_smoke,live_smoke_o3104)
live-smoke-o3108:
	$(call run_smoke,live_smoke_o3108)
live-smoke-o3116:
	$(call run_smoke,live_smoke_o3116)
live-smoke-o3117:
	$(call run_smoke,live_smoke_o3117)
live-smoke-o3123:
	$(call run_smoke,live_smoke_o3123)
live-smoke-o3127:
	$(call run_smoke,live_smoke_o3127)
## F-O open-window flip wave (plan -001): contract self-sourced from t8467.
live-smoke-t8427:
	$(call run_smoke,live_smoke_t8427)
live-smoke-t2210:
	$(call run_smoke,live_smoke_t2210)
live-smoke-t2424:
	$(call run_smoke,live_smoke_t2424)
live-smoke-t2541:
	$(call run_smoke,live_smoke_t2541)
live-smoke-t2214:
	$(call run_smoke,live_smoke_t2214)
## t8428 deposit-balance trend — domestic, default (MAIN) lane.
live-smoke-t8428:
	$(call run_smoke,live_smoke_t8428)
live-smoke-o3128:
	$(call run_smoke,live_smoke_o3128)
live-smoke-o3136:
	$(call run_smoke,live_smoke_o3136)
live-smoke-o3137:
	$(call run_smoke,live_smoke_o3137)
live-smoke-o3139:
	$(call run_smoke,live_smoke_o3139)
live-smoke-t8462:
	$(call run_smoke,live_smoke_t8462)
live-smoke-t8410:
	$(call run_smoke,live_smoke_t8410)
live-smoke-t8451:
	$(call run_smoke,live_smoke_t8451)
live-smoke-t8419:
	$(call run_smoke,live_smoke_t8419)
live-smoke-t4203:
	$(call run_smoke,live_smoke_t4203)

live-smoke-t8417:
	$(call run_smoke,live_smoke_t8417)

live-smoke-t8418:
	$(call run_smoke,live_smoke_t8418)

live-smoke-t8411:
	$(call run_smoke,live_smoke_t8411)

live-smoke-t8452:
	$(call run_smoke,live_smoke_t8452)

live-smoke-t8453:
	$(call run_smoke,live_smoke_t8453)

live-smoke-t1302:
	$(call run_smoke,live_smoke_t1302)

live-smoke-t8464:
	$(call run_smoke,live_smoke_t8464)

live-smoke-t8465:
	$(call run_smoke,live_smoke_t8465)

live-smoke-t8466:
	$(call run_smoke,live_smoke_t8466)

live-smoke-t2216:
	$(call run_smoke,live_smoke_t2216)

live-smoke-t8405:
	$(call run_smoke,live_smoke_t8405)

live-smoke-t1444:
	$(call run_smoke,live_smoke_t1444)

live-smoke-t1422:
	$(call run_smoke,live_smoke_t1422)

live-smoke-t1427:
	$(call run_smoke,live_smoke_t1427)

live-smoke-t1442:
	$(call run_smoke,live_smoke_t1442)

live-smoke-t1405:
	$(call run_smoke,live_smoke_t1405)

live-smoke-t1960:
	$(call run_smoke,live_smoke_t1960)

live-smoke-t1961:
	$(call run_smoke,live_smoke_t1961)

live-smoke-t1966:
	$(call run_smoke,live_smoke_t1966)

live-smoke-t1921:
	$(call run_smoke,live_smoke_t1921)

live-smoke-t1532:
	$(call run_smoke,live_smoke_t1532)

live-smoke-t1533:
	$(call run_smoke,live_smoke_t1533)

live-smoke-t1926:
	$(call run_smoke,live_smoke_t1926)

live-smoke-t1764:
	$(call run_smoke,live_smoke_t1764)

live-smoke-t1903:
	$(call run_smoke,live_smoke_t1903)

## Failure classifier (implement-tr R6): one credential-safe raw-HTTP POST that
## bypasses the SDK's typed deserialize. Requires LS_PROBE_TR_CD, LS_PROBE_PATH,
## and LS_PROBE_BODY. Prints a RAW-PROBE line (never a LIVE-SMOKE evidence line).
## Pass LS_SMOKE_LANE=domestic_option|overseas_option to probe under a credential
## lane (U4/U5 re-probe F/O and overseas-F/O candidates under their account).
##   make raw-probe LS_PROBE_TR_CD=t8425 LS_PROBE_PATH=/stock/sector \
##     LS_PROBE_BODY='{"t8425InBlock":{"dummy":""}}'
##   make raw-probe LS_SMOKE_LANE=domestic_option LS_PROBE_TR_CD=CFOEQ11100 ...
raw-probe:
	$(call run_smoke,raw_http_probe)

## Differential negative probe (plan 2026-07-01-004, U4/R10): run t8412's valid
## control plus each mechanically-generated invalid variant against the REAL paper
## gateway IN THE SAME SESSION, classify each HELD/CLEAN/DIVERGENT, and print a
## credential-free NEG-PROBE line. OPERATOR-RUN, in-window; gates re-promotion to
## Recommended (U8), never CI. A valid-control failure is HELD, not a divergence.
##   make live-smoke-t8412-negative
live-smoke-t8412-negative:
	@lane="$(LS_SMOKE_LANE)"; [ -n "$$lane" ] || lane="domestic"; \
	lane_file=".env.$$lane"; \
	[ -f "$$lane_file" ] || { echo "FAIL: t8412-negative: lane file $$lane_file missing (LS_SMOKE_LANE=$$lane); refusing to fall back to .env (wrong-account hazard)"; exit 1; }; \
	set -a; . "./$$lane_file"; set +a; \
	out=$$(cargo test -p ls-sdk --test negative_probe -- --ignored --exact --nocapture live_smoke_t8412_negative 2>&1); \
	echo "$$out"; \
	echo "$$out" | grep -q "1 passed" || { echo "FAIL: t8412 negative probe did not run (0 tests) or did not pass"; exit 1; }

.PHONY: lane-check

## Offline regression check for the fail-fast lane guard (plan 2026-07-01-002,
## U2): asserts a MISSING lane file exits non-zero with the wrong-account-hazard
## message and a PRESENT lane file passes the guard — with NO live gateway call.
## The safety net that replaces the deleted legacy .env; run it in the offline gate.
lane-check:
	@scripts/lane-fail-fast-check.sh

# ---------------------------------------------------------------------------
# Docs generation — ls-docgen projects TR Dependency Docs and SDK Reference
# Docs from ls-metadata. These targets need no credentials, so (unlike the
# live-smoke recipes above) they source no lane file. If a future docs target
# ever needs credentials, source a named lane file in the recipe shell
# (`set -a; . ./.env.domestic; set +a`) — never via make `include` (see the
# header note and
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
