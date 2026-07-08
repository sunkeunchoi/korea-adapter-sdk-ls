#!/usr/bin/env bash
# turn4-ingest.sh — U4: fresh-home DRIP-FED ingest of the frozen turn-4 universe.
#
# Expands lab/config/turn4-universe.json into a symbol list, then drives a daily
# pass (whole range, batched) followed by a DRIP-FED minute pass — one symbol at a
# time with IGW00201 backoff — into a FRESH LS_DATA_HOME (KTD-5, R4/R5/R6).
# Attended, paper-only.
#
# Why drip-fed (vs turn3-ingest.sh's single-shot minute pass): IGW00201 is a rolling
# call-count budget, not a per-second rate, so a bulk multi-symbol minute pull aborts
# partway once the budget is warm. Drip-feeding one symbol at a time with a ~120s
# backoff keeps each burst under the remaining budget and resumes cleanly (range-mode
# ingest is per-symbol idempotent — an already-covered symbol APPEND REFUSEs). See
# docs/solutions/integration-issues/ls-gateway-igw00201-bulk-minute-ingest-drip-feed.md
#
# Budget-aware layer (U2/U3): both the minute AND daily passes now recover from an
# IGW00201 in-process (backoff-and-retry, watermark withheld on a dead budget), and a
# persistent per-credential spend ledger (pinned via LS_SPEND_LEDGER_FILE below) lets
# the ingest stop before the cliff. The script's own grep-retry is now a thin outer
# backstop, not the primary recovery — LS_TURN4_TRIES defaults low.
#
# The daily floor is pinned a few sessions before the minute floor so the universe
# scan's prior-session daily reads exist from the first backfilled minute day. Run
# `lab-research catalog status` afterward — it is the go/no-go and pins the achievable
# range on front-truncation (OQ1); confirm per-symbol coverage by counting
# `1-MINUTE:` lines (a GO can mask partial minute coverage — U3 doc).
#
# Required env:
#   LS_DATA_HOME        a FRESH data home directory (its catalog is $LS_DATA_HOME/catalog)
#   LS_TURN4_SDATE      minute-range start YYYYMMDD (also the daily start unless overridden)
#   LS_TURN4_EDATE      range end YYYYMMDD
# Optional env:
#   LS_TURN4_DAILY_SDATE  daily-range start (default: 10 calendar days before SDATE for the
#                         prior-session cushion; override to pin exactly)
#   LS_INGEST_LANE_FILE   lane env-file (default .env.domestic)
#   LS_TURN4_UNIVERSE     frozen universe file (default lab/config/turn4-universe.json)
#   LS_TURN4_MINUTE       minute bar-kind (default minute:1)
#   LS_INGEST_BIN         prebuilt ls-ingest binary (default ./target/debug/ls-ingest —
#                         a release build repeatedly got killed mid-compile, U3 doc)
#   LS_TURN4_BACKOFF      IGW00201 backoff seconds (default 120) — outer backstop only
#   LS_TURN4_TRIES        per-symbol outer retry attempts (default 3; the in-process
#                         IGW00201 arms now own primary recovery, so this is a thin
#                         backstop for a killed process, not the recovery loop)
#   LS_SPEND_LEDGER_FILE  per-credential spend ledger path (default pinned below to
#                         adapters/nautilus/state/spend-ledger.json so it survives the
#                         fresh data home; KTD-3)
set -uo pipefail

: "${LS_TRADING_ENV:?set LS_TRADING_ENV=paper (paper-only)}"
: "${LS_DATA_HOME:?set LS_DATA_HOME to a FRESH data home directory}"
: "${LS_TURN4_SDATE:?set LS_TURN4_SDATE (minute-range start YYYYMMDD)}"
: "${LS_TURN4_EDATE:?set LS_TURN4_EDATE (range end YYYYMMDD)}"

lane_file="${LS_INGEST_LANE_FILE:-.env.domestic}"
universe="${LS_TURN4_UNIVERSE:-lab/config/turn4-universe.json}"
minute_kind="${LS_TURN4_MINUTE:-minute:1}"
bin="${LS_INGEST_BIN:-./target/debug/ls-ingest}"
backoff="${LS_TURN4_BACKOFF:-120}"
tries="${LS_TURN4_TRIES:-3}"
catalog="$LS_DATA_HOME/catalog"
# Pin the spend ledger OUTSIDE the fresh data home so it survives across turn runs
# (KTD-3): the ingest keys it by hashed appkey and never advances coverage from it —
# the gateway stays ground truth. /state is gitignored.
export LS_SPEND_LEDGER_FILE="${LS_SPEND_LEDGER_FILE:-adapters/nautilus/state/spend-ledger.json}"

# Daily cushion: default the daily floor ~10 calendar days before the minute floor so
# the prior-session daily close exists for the universe gap scan on the first minute day.
if [[ -n "${LS_TURN4_DAILY_SDATE:-}" ]]; then
  daily_sdate="$LS_TURN4_DAILY_SDATE"
else
  daily_sdate="$(python3 -c "import datetime,sys; d=datetime.datetime.strptime(sys.argv[1],'%Y%m%d')-datetime.timedelta(days=10); print(d.strftime('%Y%m%d'))" "$LS_TURN4_SDATE")"
fi

if [[ ! -f "$universe" ]]; then
  echo "error: frozen universe $universe not found — run capture-universe first (U1)" >&2
  exit 1
fi
if [[ ! -x "$bin" ]]; then
  echo "error: ls-ingest binary $bin not found/executable — build it first:" >&2
  echo "  cargo build --bin ls-ingest -p nautilus-ls" >&2
  exit 1
fi

# Space-separated shcodes (for the drip loop) and comma-joined (for the daily batch).
symbols_sp="$(python3 -c "import json,sys; print(' '.join(json.load(open(sys.argv[1]))['shcodes']))" "$universe")"
symbols_csv="${symbols_sp// /,}"
if [[ -z "$symbols_sp" ]]; then
  echo "error: no shcodes in $universe" >&2
  exit 1
fi
n_symbols="$(wc -w <<<"$symbols_sp" | tr -d ' ')"
echo "frozen universe: $universe ($n_symbols symbols)"
echo "fresh data home: $LS_DATA_HOME (catalog: $catalog)"
echo "daily $daily_sdate..$LS_TURN4_EDATE ; minute ($minute_kind) $LS_TURN4_SDATE..$LS_TURN4_EDATE"

# Run one ls-ingest invocation with IGW00201 backoff-and-retry. Idempotent per symbol.
run_ingest() {  # $1=label  $2=kind  $3=sdate  $4=symbols  $5=skip_universe_load(0/1)
  local label="$1" kind="$2" sdate="$3" syms="$4" skip="${5:-0}" try out
  for try in $(seq 1 "$tries"); do
    rm -f "$catalog/.ls-ingest.lock"   # clear a stale lock a killed run leaves
    out="$(LS_TRADING_ENV=paper LS_INGEST_LANE_FILE="$lane_file" \
      LS_INGEST_CATALOG="$catalog" LS_INGEST_SDATE="$sdate" LS_INGEST_EDATE="$LS_TURN4_EDATE" \
      LS_INGEST_KIND="$kind" LS_INGEST_SYMBOLS="$syms" LS_INGEST_SKIP_UNIVERSE_LOAD="$skip" \
      "$bin" 2>&1)"
    if grep -q "IGW00201" <<<"$out"; then
      echo "  [$label try $try] IGW00201 — backoff ${backoff}s"; sleep "$backoff"; continue
    fi
    if grep -qE "ingest complete|APPEND REFUSED|coverage" <<<"$out"; then
      echo "  [$label] ok"; return 0
    fi
    echo "  [$label try $try] unexpected:"; sed 's/^/    | /' <<<"$out"; sleep 30
  done
  echo "  [$label] FAILED after $tries tries" >&2; return 1
}

# --- Daily pass (whole range incl. cushion, batched — cheap: ~1 page/symbol) ---
# This pass does the ONE universe load (t8430 + 2x t9945) and persists the
# instrument snapshot; every later per-symbol minute pass then skips that 3-call
# load (LS_INGEST_SKIP_UNIVERSE_LOAD=1), the dominant avoidable IGW00201 cost.
echo "== daily ingest (batched, loads + persists the universe once) =="
run_ingest "daily" "daily" "$daily_sdate" "$symbols_csv" 0 || {
  echo "daily pass failed — investigate before the minute drip" >&2; exit 1; }

# --- Minute pass (DRIP-FED, one symbol at a time; universe load skipped) ---
echo "== minute ingest (drip-fed, ${backoff}s IGW00201 backoff, universe load skipped) =="
i=0
for s in $symbols_sp; do
  i=$((i+1))
  echo "-- [$i/$n_symbols] $s --"
  run_ingest "min:$s" "$minute_kind" "$LS_TURN4_SDATE" "$s" 1 || \
    echo "  WARNING: $s minute drip did not complete — re-run the script to resume" >&2
  sleep 8   # brief inter-symbol pace
done

echo "== ingest done — verify the go/no-go and per-symbol minute coverage =="
echo "LS_DATA_HOME=$LS_DATA_HOME LS_STATUS_SDATE=$LS_TURN4_SDATE LS_STATUS_EDATE=$LS_TURN4_EDATE \\"
echo "  ./target/debug/lab-research catalog status"
echo "# per-symbol minute coverage (must equal $n_symbols):"
echo "LS_DATA_HOME=$LS_DATA_HOME LS_STATUS_SDATE=$LS_TURN4_SDATE LS_STATUS_EDATE=$LS_TURN4_EDATE \\"
echo "  ./target/debug/lab-research catalog status | grep -c '1-MINUTE:'"
