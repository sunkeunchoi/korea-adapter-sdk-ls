#!/usr/bin/env bash
# turn3-ingest.sh — U3: fresh-home ingest of the frozen turn-3 universe.
#
# Expands lab/config/turn3-universe.json into LS_INGEST_SYMBOLS, then drives a
# daily pass (whole range) followed by a BOUNDED minute pass (frozen symbols only)
# into a FRESH LS_DATA_HOME (KTD-4, R4/R5). Attended, paper-only.
#
# The daily floor is pinned a few sessions before the minute floor so the universe
# scan's prior-session daily reads exist from the first backfilled minute day
# (README ordering note). Run `lab-research catalog status` afterward — it is the
# go/no-go and pins the achievable range on front-truncation (OQ1).
#
# Required env:
#   LS_DATA_HOME        a FRESH data home directory (its catalog is $LS_DATA_HOME/catalog)
#   LS_TURN3_SDATE      minute-range start YYYYMMDD (also the daily start unless overridden)
#   LS_TURN3_EDATE      range end YYYYMMDD
# Optional env:
#   LS_TURN3_DAILY_SDATE  daily-range start (default: LS_TURN3_SDATE; set ~5 sessions earlier)
#   LS_INGEST_LANE_FILE   lane env-file (default .env.domestic)
#   LS_TURN3_UNIVERSE     frozen universe file (default lab/config/turn3-universe.json)
#   LS_TURN3_MINUTE       minute bar-kind (default minute:1)
set -euo pipefail

: "${LS_TRADING_ENV:?set LS_TRADING_ENV=paper (paper-only)}"
: "${LS_DATA_HOME:?set LS_DATA_HOME to a FRESH data home directory}"
: "${LS_TURN3_SDATE:?set LS_TURN3_SDATE (minute-range start YYYYMMDD)}"
: "${LS_TURN3_EDATE:?set LS_TURN3_EDATE (range end YYYYMMDD)}"

lane_file="${LS_INGEST_LANE_FILE:-.env.domestic}"
universe="${LS_TURN3_UNIVERSE:-lab/config/turn3-universe.json}"
minute_kind="${LS_TURN3_MINUTE:-minute:1}"
daily_sdate="${LS_TURN3_DAILY_SDATE:-$LS_TURN3_SDATE}"
catalog="$LS_DATA_HOME/catalog"

if [[ ! -f "$universe" ]]; then
  echo "error: frozen universe $universe not found — run capture-universe first (U1)" >&2
  exit 1
fi

# Extract the comma-joined shcodes from the frozen file (schema: {"shcodes": [...]}).
symbols="$(python3 -c "import json,sys; print(','.join(json.load(open(sys.argv[1]))['shcodes']))" "$universe")"
if [[ -z "$symbols" ]]; then
  echo "error: no shcodes in $universe" >&2
  exit 1
fi
echo "frozen universe: $universe"
echo "LS_INGEST_SYMBOLS=$symbols"
echo "fresh data home: $LS_DATA_HOME (catalog: $catalog)"

# --- Daily pass (whole range, cheap) ---
echo "== daily ingest $daily_sdate..$LS_TURN3_EDATE =="
LS_TRADING_ENV=paper LS_INGEST_LANE_FILE="$lane_file" \
LS_INGEST_CATALOG="$catalog" LS_INGEST_SDATE="$daily_sdate" LS_INGEST_EDATE="$LS_TURN3_EDATE" \
LS_INGEST_KIND=daily LS_INGEST_SYMBOLS="$symbols" \
  cargo run --release --bin ls-ingest

# --- Bounded minute pass (frozen symbols only) ---
echo "== minute ingest ($minute_kind) $LS_TURN3_SDATE..$LS_TURN3_EDATE =="
LS_TRADING_ENV=paper LS_INGEST_LANE_FILE="$lane_file" \
LS_INGEST_CATALOG="$catalog" LS_INGEST_SDATE="$LS_TURN3_SDATE" LS_INGEST_EDATE="$LS_TURN3_EDATE" \
LS_INGEST_KIND="$minute_kind" LS_INGEST_SYMBOLS="$symbols" \
  cargo run --release --bin ls-ingest

echo "== ingest done — verify the go/no-go and pin the achievable range =="
echo "LS_DATA_HOME=$LS_DATA_HOME LS_STATUS_SDATE=$LS_TURN3_SDATE LS_STATUS_EDATE=$LS_TURN3_EDATE \\"
echo "  cargo run --bin lab-research catalog status"
