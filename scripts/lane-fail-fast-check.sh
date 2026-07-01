#!/usr/bin/env bash
# Fail-fast lane-guard regression check (plan 2026-07-01-002, U2).
#
# The env-lane cutover deleted the legacy `.env` and made every smoke recipe
# hard-require its named lane file (`.env.<lane>`), refusing to silently fall
# back to `.env` (the wrong-account hazard). This check is the safety net that
# replaces the deleted `.env`: it asserts a MISSING lane file makes the recipe
# exit non-zero with the wrong-account-hazard message, and a PRESENT lane file
# lets the recipe proceed — with NO live gateway call.
#
# It runs the REAL Makefile recipe (no duplicated guard logic to drift) in an
# isolated temp dir, with a `cargo` shim on PATH so the "present lane" case
# reaches a green result offline instead of hitting the network.
#
# Usage: scripts/lane-fail-fast-check.sh   (exit 0 = guard behaves; non-0 = regression)
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# --- cargo shim: pretend the #[ignore] smoke ran and passed (no network) -------
mkdir -p "$work/bin"
cat > "$work/bin/cargo" <<'SHIM'
#!/bin/sh
# Only the `cargo test ... live_smoke/order_smoke` invocations matter here; emit
# the "1 passed" line run_smoke greps for and exit clean. No compilation, no net.
echo "test result: ok. 1 passed; 0 failed; 0 ignored"
exit 0
SHIM
chmod +x "$work/bin/cargo"

cp "$repo_root/Makefile" "$work/Makefile"
export PATH="$work/bin:$PATH"

fails=0
run_make() { ( cd "$work" && make "$@" 2>&1 ); }

# --- Case A: missing default lane (.env.domestic absent) -> loud non-zero ------
# No lane files exist in $work, so the default lane must fail fast.
out="$(run_make live-smoke)"; rc=$?
if [ "$rc" -eq 0 ]; then
  echo "FAIL[A]: default lane with no .env.domestic exited 0 (expected non-zero)"; fails=1
elif ! printf '%s' "$out" | grep -q "wrong-account hazard"; then
  echo "FAIL[A]: non-zero exit but missing 'wrong-account hazard' message"; echo "$out"; fails=1
else
  echo "ok[A]: missing .env.domestic on default lane fails fast with hazard message"
fi

# --- Case B: missing overseas lane (LS_SMOKE_LANE=overseas) -> loud non-zero ---
out="$(run_make live-smoke-g3101)"; rc=$?
if [ "$rc" -eq 0 ]; then
  echo "FAIL[B]: overseas lane with no .env.overseas exited 0 (expected non-zero)"; fails=1
elif ! printf '%s' "$out" | grep -q "wrong-account hazard"; then
  echo "FAIL[B]: non-zero exit but missing 'wrong-account hazard' message"; echo "$out"; fails=1
else
  echo "ok[B]: missing .env.overseas on overseas lane fails fast with hazard message"
fi

# --- Case B2: missing lane on an order recipe (rewired under R3) ---------------
out="$(run_make live-smoke-order-chain LS_SMOKE_LANE=domestic)"; rc=$?
if [ "$rc" -eq 0 ] || ! printf '%s' "$out" | grep -q "wrong-account hazard"; then
  echo "FAIL[B2]: order-chain recipe did not fail fast on a missing lane file"; echo "$out"; fails=1
else
  echo "ok[B2]: order-chain recipe fails fast on a missing lane file"
fi

# --- Case C: present lane file (dummy placeholder values) -> guard passes ------
# A present lane file must NOT trip the guard; the shimmed cargo returns green.
printf 'LS_TRADING_ENV=paper\nLS_PAPER_APIKEY=dummy\nLS_PAPER_SECRET=dummy\nLS_PAPER_ACCOUNT=0000000000\n' > "$work/.env.domestic"
out="$(run_make live-smoke)"; rc=$?
if [ "$rc" -ne 0 ]; then
  echo "FAIL[C]: present .env.domestic tripped the guard or failed (expected pass)"; echo "$out"; fails=1
elif printf '%s' "$out" | grep -q "wrong-account hazard"; then
  echo "FAIL[C]: present lane file still printed the hazard message"; echo "$out"; fails=1
else
  echo "ok[C]: present .env.domestic passes the guard (no false positive)"
fi

if [ "$fails" -ne 0 ]; then
  echo "lane-fail-fast-check: FAILED"; exit 1
fi
echo "lane-fail-fast-check: all guard cases pass"
