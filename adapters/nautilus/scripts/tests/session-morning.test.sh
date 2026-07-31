#!/usr/bin/env bash
#
# Live-path regression tests for session-morning.sh — the calendar half of the
# morning chain (steps [1]-[5]) and the ingest pace gate (steps [7]-[9]), run
# against STUBBED binaries in a FIXTURE REPO.
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
# EVERY RUN IS CLOCK-INDEPENDENT, by two different means. The calendar tests pass
# `--stop-before-activate`, which exits after the step [5] diff gate and never
# reaches a clock at all. The pace-gate tests below DO take the live path through
# step [9], and get determinism from the DEADLINE side instead: `LS_SM_INGEST_BY`
# resolves as today at HH:MM, so `00:00` is elapsed at every hour a test can run.
# That matters because LS_SM_NOW — the obvious clock override — is refused on a
# real run by design, and these are real runs.
#
# SCOPE LIMIT: step [10] (lab-mount-universe) and the step [11] GO/NO-GO report are
# still never reached — the catch-up runs stop one step short of them by design, and
# nothing here drives the attended path past the 09:00 guard.

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
  for b in calendar-activate calendar-status lab-research lab-mount-universe; do
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

  # ls-ingest: a controllable long-running process. It does NOT mirror the real binary's
  # contract — the step [7] tests are about what the SCRIPT does to a running ingest, so all
  # the stub owes them is a process that can be observed, killed, or allowed to finish.
  # Two knobs, both inherited from the environment the chain passes down:
  #   STUB_INGEST_SECS        seconds it runs before finishing (0 = exit immediately)
  #   STUB_INGEST_ADVANCE_TO  the daily watermark it writes on a CLEAN finish
  # It logs "COMPLETED" only when it reaches the end, so the stub log tells "the pace gate
  # killed it" from "it ran to completion" without reading the script's own prose.
  cat >"$bin/ls-ingest" <<'STUB'
#!/usr/bin/env bash
echo "ls-ingest $*" >>"$STUB_LOG"
secs="${STUB_INGEST_SECS:-0}"
if [ "$secs" != "0" ]; then
  # `sleep` in the BACKGROUND plus `wait`, never a foreground `sleep`: bash defers a trapped
  # signal until the current foreground command returns, so a foreground sleep would swallow
  # the SIGTERM for its whole duration and the kill under test would look like a clean finish.
  trap 'kill "$sp" 2>/dev/null; exit 143' TERM
  sleep "$secs" & sp=$!
  wait "$sp"
fi
if [ -n "${STUB_INGEST_ADVANCE_TO:-}" ]; then
  python3 -c "
import json,sys
p,d=sys.argv[1],sys.argv[2]
c=json.load(open(p))
c['watermarks']={k:(d if k.endswith('|1-DAY') else v) for k,v in c['watermarks'].items()}
json.dump(c,open(p,'w'))" "$LS_INGEST_CATALOG/ingest-checkpoint.json" "$STUB_INGEST_ADVANCE_TO"
fi
echo "ls-ingest COMPLETED" >>"$STUB_LOG"
exit 0
STUB
  chmod +x "$bin/ls-ingest"

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

# CHAIN_ENV is a per-test list of `NAME=value` overrides layered on top of the two the
# harness always sets. Reset it before every run so one test cannot configure the next.
CHAIN_ENV=()

_run_in() {
  local root="$1"; shift
  local log="$root/stub.log"
  : >"$log"
  CHAIN_OUT="$(STUB_LOG="$log" LS_TRADING_ENV=paper \
         LS_SM_SESSION_DATE="$SESSION_DATE" LS_SM_MOUNT_DATE=2026-07-31 \
         env ${CHAIN_ENV[@]+"${CHAIN_ENV[@]}"} \
         bash "$root/adapters/nautilus/scripts/session-morning.sh" "$@" 2>&1)"
  CHAIN_RC=$?
  CHAIN_LOG="$(cat "$log" 2>/dev/null || true)"
}

drop_fixture() { [ -n "${CHAIN_ROOT:-}" ] && rm -rf "$CHAIN_ROOT"; CHAIN_ENV=(); return 0; }

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

# ===================================================================== steps [7]-[9]
# THE SECOND DEFECT CLASS THIS FILE GUARDS: the step [7] in-ingest pace check killed
# ls-ingest on ANY run whose LS_SM_INGEST_BY had already elapsed, leaving a partial
# watermark distribution. Correct for the attended path (a universe landing after 09:10
# takes zero trades); wrong for a catch-up, whose entire purpose is finishing the ingest.
#
# HOW THESE RUNS BECOME DETERMINISTIC WITHOUT A CLOCK SEAM. LS_SM_NOW is refused on a real
# run by design, and these ARE real runs — they take the live path all the way to step [9].
# So the elapsed deadline is manufactured from the deadline side instead: LS_SM_INGEST_BY
# resolves as TODAY at HH:MM, and `00:00` is the first instant of the day, so `now >= dl`
# holds at every hour a test can run. That is the weekend condition reproduced exactly,
# not simulated.
#
# The 30s poll would otherwise make these tests take minutes, so LS_SM_POLL_SECS drops it
# to 1s. It is a latency knob only — every input to pace_verdict still comes from the real
# clock and the real checkpoint — which is why it is bounded (1..30) rather than refused.

ingest_env() { # secs advance_to → CHAIN_ENV for a step [7] run
  CHAIN_ENV=(
    "LS_SM_POLL_SECS=1"
    "LS_SM_INGEST_BY=00:00"          # already elapsed at any hour: forces the LATE verdict
    "STUB_INGEST_SECS=$1"
    "STUB_INGEST_ADVANCE_TO=$2"
  )
}

# --- normal mode: an elapsed deadline plus a non-advancing ingest still stands down ---
# The stub is told to run for 10s and to advance nothing, so the only way it can stop is
# the script killing it. It must never reach its COMPLETED marker.
ingest_env 10 ""
CHAIN_ENV+=("LS_SM_UNIVERSE_BY=23:59")   # irrelevant: step [8] is unreachable from a kill
run_chain
assert_eq "normal mode: elapsed deadline + stalled ingest exits 40 (STAND-DOWN)" "40" "$CHAIN_RC"
case "$CHAIN_LOG" in
  *"ls-ingest COMPLETED"*)
    no "normal mode: the stalled ingest is killed" "no COMPLETED marker" "$CHAIN_LOG" ;;
  *"ls-ingest "*) ok "normal mode: the stalled ingest is killed" ;;
  *) no "normal mode: the stalled ingest is killed" "ls-ingest to have been started" "$CHAIN_LOG" ;;
esac
case "$CHAIN_OUT" in
  *"STAND DOWN — not on pace"*) ok "normal mode: reports the pace stand-down" ;;
  *) no "normal mode: reports the pace stand-down" "a 'STAND DOWN — not on pace' report" "$CHAIN_OUT" ;;
esac
drop_fixture

# --- catch-up mode, IDENTICAL conditions: no kill, the ingest finishes -----------------
# Same elapsed deadline. The stub runs long enough to be polled at least once, then
# advances both fixture watermarks to the session date so step [7]'s completeness check
# passes. LS_SM_UNIVERSE_BY is left in the FUTURE on purpose: the step [8] refusal below
# must be unconditional, not the clock standing the run down by coincidence.
ingest_env 3 "$SESSION_COMPACT"
CHAIN_ENV+=("LS_SM_UNIVERSE_BY=23:59")
run_chain --catch-up
case "$CHAIN_LOG" in
  *"ls-ingest COMPLETED"*) ok "catch-up: the ingest is NOT killed and runs to completion" ;;
  *) no "catch-up: the ingest is NOT killed and runs to completion" \
        "an 'ls-ingest COMPLETED' marker" "$CHAIN_LOG" ;;
esac
case "$CHAIN_OUT" in
  *"pace gate OFF (--catch-up)"*) ok "catch-up: progress is still reported, without a verdict" ;;
  *) no "catch-up: progress is still reported, without a verdict" \
        "a 'pace gate OFF (--catch-up)' progress line" "$CHAIN_OUT" ;;
esac
case "$CHAIN_OUT" in
  *"partial ingest"*) no "catch-up: the catalog is left complete, not partial" \
                         "no partial-ingest refusal" "$CHAIN_OUT" ;;
  *) ok "catch-up: the catalog is left complete, not partial" ;;
esac

# --- catch-up mode still refuses to resolve a universe at step [8] ---------------------
# The universe deadline is 23:59 and the ingest completed, so the ATTENDED path would have
# gone on to resolve one. Catch-up must not, and must say so with its own exit code.
assert_eq "catch-up: a complete run exits 41 (CATCH-UP COMPLETE), not 0 and not 40" "41" "$CHAIN_RC"
case "$CHAIN_LOG" in
  *lab-mount-universe*) no "catch-up: lab-mount-universe is never invoked" \
                           "no lab-mount-universe call" "$CHAIN_LOG" ;;
  *) ok "catch-up: lab-mount-universe is never invoked" ;;
esac
case "$CHAIN_LOG" in
  *"lab-research catalog status"*) ok "catch-up: step [9] still certifies the catalog" ;;
  *) no "catch-up: step [9] still certifies the catalog" \
        "a lab-research catalog status call" "$CHAIN_LOG" ;;
esac
drop_fixture

# --- NEGATIVE META-TESTS: prove the two guards above can be seen to fail ---------------
# Without these the assertions are unfalsifiable: a catch-up run that exits 41 for some
# unrelated reason would green both.

# Delete the `continue` that skips the pace verdict on a catch-up. The run then falls into
# the same LATE branch normal mode takes, and the kill it is supposed to prevent happens.
ingest_env 10 ""
CHAIN_ENV+=("LS_SM_UNIVERSE_BY=23:59")
run_chain_mutated '/^    continue$/d' --catch-up
if [ "$CHAIN_RC" = "40" ]; then
  case "$CHAIN_LOG" in
    *"ls-ingest COMPLETED"*)
      no "harness detects a catch-up stripped of the step [7] pace-gate skip" \
         "the ingest to have been killed" "$CHAIN_LOG" ;;
    *) ok "harness detects a catch-up stripped of the step [7] pace-gate skip" ;;
  esac
else
  no "harness detects a catch-up stripped of the step [7] pace-gate skip" \
     "exit 40 (the kill re-armed)" "exit $CHAIN_RC"
fi
drop_fixture

# Disable both column-0 `if (( catch_up ))` branches below the ingest — the step [8] refusal
# and the exit-41 report. The step [7] skip is indented, so it survives and the ingest still
# completes; only the universe half is stripped. With LS_SM_UNIVERSE_BY already elapsed the
# un-guarded path must fall through to the ordinary 40, NOT to 41. Asserting the exact code
# rather than `!= 41` is what stops a broken fixture from greening this by failing early.
ingest_env 3 "$SESSION_COMPACT"
CHAIN_ENV+=("LS_SM_UNIVERSE_BY=00:00")
run_chain_mutated 's/^if (( catch_up )); then$/if (( 0 )); then/' --catch-up
assert_eq "harness detects a catch-up stripped of the step [8] universe refusal" \
          "40" "$CHAIN_RC"
case "$CHAIN_LOG" in
  *"ls-ingest COMPLETED"*) ok "the step [8] mutation leaves the step [7] skip intact" ;;
  *) no "the step [8] mutation leaves the step [7] skip intact" \
        "the ingest to still run to completion" "$CHAIN_LOG" ;;
esac
drop_fixture

# --- LS_SM_POLL_SECS is bounded, so the test seam cannot misconfigure a real run -------
CHAIN_ENV=("LS_SM_POLL_SECS=0")
run_chain --dry-run
assert_eq "LS_SM_POLL_SECS=0 is refused as misconfiguration (64)" "64" "$CHAIN_RC"
drop_fixture
CHAIN_ENV=("LS_SM_POLL_SECS=abc")
run_chain --dry-run
assert_eq "a non-numeric LS_SM_POLL_SECS is refused (64)" "64" "$CHAIN_RC"
drop_fixture

# --- the hand-maintained --dry-run heredoc must describe catch-up mode too -------------
run_chain --catch-up --dry-run
assert_eq "--catch-up --dry-run exits 0" "0" "$CHAIN_RC"
assert_eq "--catch-up --dry-run invokes no binary" "" "$CHAIN_LOG"
case "$CHAIN_OUT" in
  *"exit 41"*) ok "--dry-run text states the catch-up exit code" ;;
  *) no "--dry-run text states the catch-up exit code" "exit 41 in the printed sequence" "$CHAIN_OUT" ;;
esac
case "$CHAIN_OUT" in
  *"[10] NOT RUN"*) ok "--dry-run text shows the universe step is not run under --catch-up" ;;
  *) no "--dry-run text shows the universe step is not run under --catch-up" \
        "a '[10] NOT RUN' line" "$CHAIN_OUT" ;;
esac
drop_fixture

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ] || exit 1
