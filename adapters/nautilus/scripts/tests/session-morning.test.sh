#!/usr/bin/env bash
#
# Live-path regression tests for session-morning.sh — the calendar half of the
# morning chain (steps [1]-[5]), run against STUBBED binaries in a FIXTURE REPO.
# Run with: bash adapters/nautilus/scripts/tests/session-morning.test.sh  (or `make script-check`)
#
# WHY A FIXTURE REPO AND NOT A `PATH` STUB. The script resolves every binary as
# an ABSOLUTE path ($BIN="$NAUT/target/debug/..."), deliberately, so it cannot be
# hijacked by a poisoned PATH. That also means a PATH stub cannot intercept it.
# The script derives its one repo-root variable from its own location
# (`script_dir/../../..`), so invoking a SYMLINK to the real script from inside a
# throwaway tree relocates every path it computes into that tree — no copy of the
# script to drift, no write anywhere near the real state/ or catalog.
#
# WHY THIS FILE EXISTS AT ALL. The script's `--self-test` covers only the pure
# decision core (pace_verdict) and `--dry-run` only prints a hand-written heredoc
# describing the intended commands. Neither invokes a binary's argument parser,
# so neither can detect a MISSING REQUIRED ARGUMENT — which is exactly how step
# [3] shipped without `--window`, and then without `--state-root`, and died on its
# first real run (2026-07-31). See
# docs/solutions/workflow-issues/shell-script-live-path-needs-stubbed-binary-tests.md,
# whose "Recurrence: operator scripts" section names this script.
#
# THE ARGV CONTRACT IS CHECKED AGAINST THE REAL BINARY, NOT A MIRROR. A stub that
# hand-reimplements `Args::parse` is itself a thing that can drift, and a stale
# mirror greens the guard on argv the real binary rejects — the same silent-drift
# class this file exists to kill. So the chain runs against stubs (no network),
# and then the argv the script ACTUALLY marshalled is replayed against the real
# compiled `calendar-fetch-inputs` with credentials stripped. That replay exercises
# the real `Args::parse` AND the real `confine()` state-root check, and stops at
# the credential refusal — before any HTTP client is constructed. Zero traffic.
#
# The run is CLOCK-INDEPENDENT: `--stop-before-activate` exits after the step [5]
# diff gate, so it never reaches the 09:00 universe guard or the pace gate.
# Steps [6]-[11] are deliberately OUT OF SCOPE here.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REAL_SCRIPT="$HERE/../session-morning.sh"
REAL_BIN="$HERE/../../target/debug/calendar-fetch-inputs"

pass=0
fail=0
ok() { printf 'ok   - %s\n' "$1"; pass=$((pass + 1)); }
no() { printf 'FAIL - %s\n     expected: %s\n     actual:   %s\n' "$1" "$2" "$3"; fail=$((fail + 1)); }
assert_eq() { # desc expected actual
  if [ "$2" = "$3" ]; then ok "$1"; else no "$1" "$2" "$3"; fi
}

# The operator shell exports a dozen LS_* vars that would leak into the run and
# decide its outcome from outside the fixture. Strip them all.
# See docs/solutions/test-failures/operator-shell-ls-env-makes-the-adapter-suite-look-red-on-pristine-main.md
while read -r v; do [ -n "$v" ] && unset "$v"; done < <(env | sed -n 's/^\(LS_[A-Za-z0-9_]*\)=.*/\1/p')

SESSION_DATE=2026-07-30
SESSION_COMPACT=20260730

# ---------------------------------------------------------------- the fixture repo
# Builds a throwaway tree with the exact layout session-morning.sh's preflight
# requires, stubs every binary it invokes, and echoes the root.
#
# $1 (optional): a `sed` expression. When given, the script is COPIED and mutated
# instead of symlinked — that is how the negative meta-test below breaks step [3]
# on purpose. With no argument the REAL script is symlinked, never copied.
make_fixture() { # [sed_expr] -> repo root on stdout
  local mutation="${1:-}" root
  root="$(mktemp -d)"
  local naut="$root/adapters/nautilus"
  local bin="$naut/target/debug"
  mkdir -p "$bin" "$naut/state" "$naut/scripts" "$naut/lab/config" \
           "$root/data/turn4-fresh/catalog" "$root/data/turn4-fresh/state"

  if [ -n "$mutation" ]; then
    sed "$mutation" "$REAL_SCRIPT" >"$naut/scripts/session-morning.sh"
    chmod +x "$naut/scripts/session-morning.sh"
  else
    # A symlink, never a copy — a copied script silently stops testing the real one.
    ln -s "$REAL_SCRIPT" "$naut/scripts/session-morning.sh"
  fi

  # Step [1] reads this and skips the network probe entirely when it finds a
  # POSITIVE line for the session date.
  printf '%s\n' \
    "2026-07-31T08:20:42+0900 basDd=$SESSION_COMPACT attempt=17 http=200 bytes=293133 rows=943 verdict=POSITIVE" \
    >"$naut/scripts/krx-witness-watch.log"

  printf '%s\n' '{"artifact_id":"fixture0000000000000000000000000000000000000000000000000000000","alerts":[]}' \
    >"$naut/state/krx.calendar.json"
  printf '%s\n' '{}' >"$naut/lab/config/universe-metadata-20260723.json"
  printf '%s\n' 'LS_ACCOUNT=fixture' >"$root/.env.domestic"
  # Placeholder credentials: the preflight refuses unless both are exported. These
  # are literals in a throwaway tree — never a real key, and never committed.
  printf '%s\n' 'LS_KRX_APPKEY=fixture-krx-key' 'LS_KASI_SERVICE_KEY=fixture-kasi-key' \
    >"$root/.env.calendar"
  printf '%s\n' '{"watermarks":{"005930.XKRX|1-DAY":"20260729","000660.XKRX|1-DAY":"20260729"},"gaps":[],"shifted":{}}' \
    >"$root/data/turn4-fresh/catalog/ingest-checkpoint.json"

  # ---- stubs: every one logs its full argv, so assertions can read the real call ----
  local b
  for b in calendar-activate calendar-status ls-ingest lab-research lab-mount-universe; do
    cat >"$bin/$b" <<STUB
#!/usr/bin/env bash
echo "$b \$*" >>"\$STUB_LOG"
exit 0
STUB
    chmod +x "$bin/$b"
  done

  # calendar-fetch-inputs: logs argv and writes the inputs artifact. It does NOT
  # re-implement the real required-argument contract — the real binary itself
  # enforces that, in assert_real_binary_accepts below.
  cat >"$bin/calendar-fetch-inputs" <<'STUB'
#!/usr/bin/env bash
echo "calendar-fetch-inputs $*" >>"$STUB_LOG"
inputs_out=""
while [ $# -gt 0 ]; do
  case "$1" in
    --inputs-out) inputs_out="$2"; shift 2 ;;
    *) shift ;;
  esac
done
[ -n "$inputs_out" ] && printf '%s\n' '{"evidence":[]}' >"$inputs_out"
echo "source krx-daily ok=true"
exit 0
STUB
  chmod +x "$bin/calendar-fetch-inputs"

  # calendar-refresh: writes the candidate + diff the step [5] gate reads.
  cat >"$bin/calendar-refresh" <<'STUB'
#!/usr/bin/env bash
echo "calendar-refresh $*" >>"$STUB_LOG"
active=""; through=""
while [ $# -gt 0 ]; do
  case "$1" in
    --active) active="$2"; shift 2 ;;
    --through) through="$2"; shift 2 ;;
    *) shift ;;
  esac
done
[ -z "$active" ] && { echo "error: missing --active" >&2; exit 1; }
printf '%s\n' "{\"artifact_id\":\"cand000000000000000000000000000000000000000000000000000000000\",\"alerts\":[]}" >"$active.candidate"
printf '%s\n' "{\"partial\":false,\"entries\":[{\"category\":\"status_established\",\"date\":\"$through\",\"detail\":\"Unknown -> TradingSession\",\"high_risk\":false}]}" >"$active.candidate.diff.json"
echo "candidate written; requires_review=true high_risk=0 partial=false"
exit 0
STUB
  chmod +x "$bin/calendar-refresh"

  printf '%s' "$root"
}

# run_chain [--flag...] → sets CHAIN_RC, CHAIN_LOG, CHAIN_OUT, CHAIN_ROOT.
# The caller owns cleanup (drop_fixture) because the argv replay below needs the
# fixture paths to still exist after the chain exits.
run_chain() {
  CHAIN_ROOT="$(make_fixture)"
  _run_in "$CHAIN_ROOT" "$@"
}

# run_chain_mutated <sed_expr> [--flag...] — same, against a deliberately broken copy.
run_chain_mutated() {
  local mutation="$1"; shift
  CHAIN_ROOT="$(make_fixture "$mutation")"
  _run_in "$CHAIN_ROOT" "$@"
}

_run_in() {
  local root="$1"; shift
  local log="$root/stub.log"
  : >"$log"
  CHAIN_OUT="$(STUB_LOG="$log" LS_TRADING_ENV=paper \
         LS_SM_SESSION_DATE="$SESSION_DATE" LS_SM_MOUNT_DATE=2026-07-31 \
         bash "$root/adapters/nautilus/scripts/session-morning.sh" "$@" 2>&1)"
  CHAIN_RC=$?
  CHAIN_LOG="$(cat "$log" 2>/dev/null || true)"
}

drop_fixture() { [ -n "${CHAIN_ROOT:-}" ] && rm -rf "$CHAIN_ROOT"; }

# Replay the argv the script actually marshalled against the REAL binary, with
# credentials stripped and from a foreign CWD (/), and echo a verdict word:
#   accepted  — argument parsing and state-root confinement BOTH passed; the run
#               stopped at the credential refusal, before any HTTP client exists
#   rejected  — the real binary refused the argv (missing/unknown argument, or a
#               path outside the owner-local state root)
#   nobinary  — the compiled binary is absent; caller reports this, never silently passes
#
# Running from `/` is deliberate: it proves the invocation is CWD-INDEPENDENT.
# A step [3] that only works from adapters/nautilus is the exact defect that made
# --state-root necessary, and a same-CWD replay would not see it.
replay_real_binary() { # full stub-log line args -> verdict on stdout
  local argv="$1" out
  [ -x "$REAL_BIN" ] || { printf 'nobinary'; return; }
  # shellcheck disable=SC2086  # fixture paths are mktemp-generated and space-free
  out="$(cd / && env -u LS_KRX_APPKEY -u LS_KASI_SERVICE_KEY -u LS_CALENDAR_STATE_ROOT \
           "$REAL_BIN" $argv 2>&1)"
  case "$out" in
    *"must be set"*)  printf 'accepted' ;;
    *)                printf 'rejected: %s' "$(printf '%s' "$out" | tr '\n' ' ')" ;;
  esac
}

fetch_argv_from_log() { # stub log -> the calendar-fetch-inputs arguments
  printf '%s\n' "$1" | sed -n 's/^calendar-fetch-inputs //p' | head -1
}

# ------------------------------------------------------------------------ tests
run_chain --stop-before-activate

# THE REGRESSION. Before the fix the script called calendar-fetch-inputs with
# --krx-through alone; the binary requires --window, so the chain died at step [3].
case "$CHAIN_LOG" in
  *"calendar-fetch-inputs "*"--window "*)
    ok "step [3] passes the required --window to calendar-fetch-inputs" ;;
  *"calendar-fetch-inputs "*)
    no "step [3] passes the required --window to calendar-fetch-inputs" \
       "argv contains --window" "$CHAIN_LOG" ;;
  *)
    no "step [3] invokes calendar-fetch-inputs at all" "a calendar-fetch-inputs call" "$CHAIN_LOG" ;;
esac

case "$CHAIN_LOG" in
  *"--window $SESSION_DATE..$SESSION_DATE"*)
    ok "step [3] window covers the session date" ;;
  *)
    no "step [3] window covers the session date" \
       "--window $SESSION_DATE..$SESSION_DATE" "$CHAIN_LOG" ;;
esac

# THE CONTRACT CHECK THAT CANNOT GO STALE: the real parser and the real confine()
# judge the real argv. Catches a missing/renamed required flag AND a state-root
# that disagrees with the output paths, without a hand-written mirror to maintain.
FETCH_ARGV="$(fetch_argv_from_log "$CHAIN_LOG")"
if [ -z "$FETCH_ARGV" ]; then
  no "step [3] argv is replayable against the real binary" "a logged argv" "$CHAIN_LOG"
else
  VERDICT="$(replay_real_binary "$FETCH_ARGV")"
  case "$VERDICT" in
    accepted) ok "real calendar-fetch-inputs accepts step [3]'s argv from a foreign CWD" ;;
    nobinary) no "real calendar-fetch-inputs accepts step [3]'s argv from a foreign CWD" \
                 "a compiled $REAL_BIN (run: cargo build --bin calendar-fetch-inputs)" "binary not built" ;;
    *)        no "real calendar-fetch-inputs accepts step [3]'s argv from a foreign CWD" \
                 "argument parsing + state-root confinement to pass" "$VERDICT" ;;
  esac
fi

assert_eq "chain reaches --stop-before-activate cleanly" "0" "$CHAIN_RC"

case "$CHAIN_LOG" in
  *"calendar-refresh "*"--through $SESSION_DATE"*)
    ok "step [4] runs with --through the session date" ;;
  *)
    no "step [4] runs with --through the session date" \
       "a calendar-refresh --through $SESSION_DATE call" "$CHAIN_LOG" ;;
esac

case "$CHAIN_LOG" in
  *lab-mount-universe*|*ls-ingest*)
    no "no ingest/universe work before the activation stop" "neither binary called" "$CHAIN_LOG" ;;
  *) ok "no ingest/universe work before the activation stop" ;;
esac
drop_fixture

# NEGATIVE META-TEST: prove this harness can SEE a broken step [3]. Without it a
# permissive check passes on any argv and the whole file is theatre — which is how
# the missing --state-root survived the first version of this test.
run_chain_mutated '/--state-root "\$STATE"/d' --stop-before-activate
MUT_ARGV="$(fetch_argv_from_log "$CHAIN_LOG")"
if [ -z "$MUT_ARGV" ]; then
  no "harness detects a step [3] stripped of --state-root" "a logged argv" "$CHAIN_LOG"
else
  case "$(replay_real_binary "$MUT_ARGV")" in
    rejected*) ok "harness detects a step [3] stripped of --state-root" ;;
    nobinary)  no "harness detects a step [3] stripped of --state-root" \
                  "a compiled $REAL_BIN" "binary not built" ;;
    accepted)  no "harness detects a step [3] stripped of --state-root" \
                  "the real binary to REFUSE the mutated argv" "it accepted it" ;;
  esac
fi
drop_fixture

# Same, for a required argument rather than a confinement one.
run_chain_mutated '/--state "\$FETCH_CKPT"/d' --stop-before-activate
MUT_ARGV="$(fetch_argv_from_log "$CHAIN_LOG")"
if [ -z "$MUT_ARGV" ]; then
  no "harness detects a step [3] stripped of --state" "a logged argv" "$CHAIN_LOG"
else
  case "$(replay_real_binary "$MUT_ARGV")" in
    rejected*) ok "harness detects a step [3] stripped of --state" ;;
    nobinary)  no "harness detects a step [3] stripped of --state" \
                  "a compiled $REAL_BIN" "binary not built" ;;
    accepted)  no "harness detects a step [3] stripped of --state" \
                  "the real binary to REFUSE the mutated argv" "it accepted it" ;;
  esac
fi
drop_fixture

# --dry-run must issue NO traffic at all: no stub may be invoked.
run_chain --dry-run
assert_eq "--dry-run exits 0" "0" "$CHAIN_RC"
assert_eq "--dry-run invokes no binary" "" "$CHAIN_LOG"

# The dry-run text is a hand-maintained transcription of the live commands, so it
# can drift from them. Pin the two arguments whose absence broke the chain.
for flag in --window --state-root; do
  case "$CHAIN_OUT" in
    *"$flag"*) ok "--dry-run text shows the $flag argument" ;;
    *) no "--dry-run text shows the $flag argument" "$flag in the printed sequence" "$CHAIN_OUT" ;;
  esac
done
drop_fixture

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ] || exit 1
